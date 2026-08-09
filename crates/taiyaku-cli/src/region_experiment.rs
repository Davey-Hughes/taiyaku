//! Why the text-block detector fails, reproduced against real pages — a one-off.
//!
//! The diagnosis behind `docs/notes.md`, "The text-block detector, measured
//! against four works, and why it does not work yet" was a throwaway program:
//! `Bounds`, `otsu`, `components`, `stroke_scale`, `merge_along` and `axis_of`
//! copied verbatim out of `detect.rs`, run once against nine hand-picked pages,
//! and discarded once that section was written. That left the section's nine
//! `regions:` lines reproducible — `taiyaku regions` still runs — and the
//! eight-page diagnosis behind them not, which is backwards: the diagnosis is
//! what turns the negative result into a finding rather than a symptom of some
//! later, unrelated bug. This module puts it back, calling `detect.rs`'s own
//! `otsu`, `components`, `stroke_scale`, `axis_of` and `merge_along` rather
//! than a second copy of them, so nothing here can drift from what
//! `taiyaku regions` actually does — see `diagnose` below for the one place
//! that still has to restate anything, and why what it restates is bookkeeping
//! rather than algorithm.
//!
//! **The precedent is `geometry_experiment.rs`, and this module follows its
//! shape rather than inventing a second convention:** `#[cfg(test)]` so no
//! part of it reaches the binary, gated on `TAIYAKU_CORPUS`, skips cleanly
//! when that is unset, and asserts only that it measured something — a
//! non-empty sample, every sampled page accounted for, every figure finite.
//! It must not and does not assert a `blocks_per_page` figure or any other
//! quality bar: those move when the detector changes, and a measurement that
//! fails when the thing it measures improves is a check pretending to be a
//! measurement. The one assertion specific to this module is a consistency
//! check rather than a quality bar: the staged pipeline computed here must
//! land on exactly the block count `detect_page` itself returns for the same
//! page, because that equality is what "calling the real functions instead of
//! copies" is for — if this module's bookkeeping ever drifted from
//! `detect_page`'s own filter order, this is what would catch it.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli region_experiment -- --nocapture
//! ```
//!
//! **`--release`, not optional in practice**, for the reason
//! `geometry_experiment.rs` gives for its own run: `components` is a
//! flood-fill over every pixel of a full-resolution scan, run twice per
//! sampled page (once for the staged pipeline below, once more inside
//! `detect_page` itself, for the cross-check), and an unoptimised build pays
//! for that twice over. **`check-all.sh` does not run this module** — see
//! `geometry_experiment.rs`'s header, which carries the full account of a
//! sentence that has now been each answer once. In short: the gate's *corpus*
//! job filters on `corpus::` and so misses this module; for a while the gate
//! also exported `TAIYAKU_CORPUS` before its unfiltered
//! `cargo test --workspace` and so ran it there in debug anyway, at about a
//! minute; the `test` job now runs with that variable removed and this is out
//! of the gate again. This one was never what made a local gate run slow —
//! `geometry_experiment`'s forty minutes were — but both left by the same
//! change.
//!
//! **Nine editions, named rather than globbed** — the same four works
//! `region-survey.sh` covered (`gotoubun-v01`, `kaguya-v01`, `oshinoko-v01`,
//! `kobayashi-v01`) and the same nine editions its `regions:` lines report, so
//! this module's aggregate figures sit next to `notes.md`'s table without a
//! second sampling decision in between. A glob that stops matching is the
//! silent-nothing failure this module's assertions refuse to have, and the
//! assertion that *every* named edition contributed a row only means
//! something if the list is fixed, the same reasoning `geometry_experiment`'s
//! `VOLUMES` gives. No sidecar is read to reach them: `taiyaku regions`
//! identifies an edition by matching a sidecar, but this module calls
//! `detect_page` directly on decoded pages, and `docs/notes.md` already
//! recorded that ten of the twelve corpus works have no sidecar to match
//! against and that most raw editions outside `gotoubun` are directories of
//! loose images rather than `.cbz` — neither constrains what can be read this
//! way, and nothing here needs `taiyaku_read::read_edition` to see anything
//! but a directory or a `.cbz`.
//!
//! **What it found, so this header is not just a description of a method.**
//! Over the 90 pages this samples (10 per edition, 9 editions), the same
//! mechanism `notes.md` names holds and at a scale ten times its own: 82 of 90
//! pages (91%) have at least half their components under 5px, `MAX_BLOCK_AREA`
//! discards something on 89 of 90 (99%) — usually the largest thing on the
//! page, 21.3% to 100.0% of it, median 51.6% — and 55 of 90 (61%) come back
//! with zero final blocks, all consistent with `notes.md`'s smaller, hand-
//! picked sample. One sampled page is the same page `notes.md`'s own
//! discarded program measured — `kobayashi-v01/raw-mono` page 7 — and every
//! figure this module reports for it matches `notes.md`'s row exactly: 3,682
//! components, 63% under 5px, `stroke_scale` 3px giving a 6px gap, one final
//! block, and the block `MAX_BLOCK_AREA` discarded there is 51.6% of the page.
//! See `docs/notes.md`'s section for the full account, and `l2a-task-8-
//! report.md` for the rest of the comparison, including where the two
//! disagree and why.
//!
//! **Task 9 extended this module rather than replacing it**, adding
//! `ScaleRule` so `stroke_scale`, `stroke_scale_above_floor` and
//! `stroke_scale_modal` are all measured from the same strokes on the same
//! 90 pages — the comparison the L2A Task 9 brief's acceptance rule needs to
//! be like-for-like. Neither candidate cleared the rule fixed before either
//! was measured; see `docs/notes.md`, "The scale rule, tried three ways, and
//! neither candidate clears the bar", and `l2a-task-9-report.md`. `detect_page`
//! is unchanged, so this module's own "what it found" paragraph above — all
//! of it computed under `ScaleRule::All` — is still exactly what
//! `taiyaku regions` does today.
//!
//! **It was extended once more before any redesign, for a hole in the
//! diagnosis rather than for a new candidate.** Every figure above is a
//! *count* of blocks, and a count cannot say what a block holds. That mattered
//! because the one edition of the nine ever to clear the design's
//! `blocks_per_page` bar — `gotoubun-v01/en-mono`, 2.50 — is also the noisiest
//! sampled, at 93%–99% of its components under `TINY_STROKE`, which is
//! precisely the edition the screentone-welding account predicts should fail
//! worst. So this module now also reports, for every block that clears all
//! four filters, what it is made of (`SurvivorDiag`), the page's own pixel
//! dimensions and the smear gap as a fraction of them, the largest single
//! component *before* any smearing, and what each of the four filters rejected
//! over the whole sample. It answered the question and corrected the mechanism
//! in two places; see `docs/notes.md`, "The edition that clears the bar is the
//! one finding no text", for the account, and O34 for what it opened.
//! `detect_page` is still unchanged and no constant moved.

use std::path::{Path, PathBuf};

use image::GrayImage;
use taiyaku_core::Axis;

use crate::corpus::{corpus, members};
use crate::detect::{self, Bounds};
use crate::fingerprint::decode_luma;

/// The nine editions `docs/notes.md`'s survey reports on, paired with the axis
/// a healthy detector should find there — vertical for the Japanese raws,
/// horizontal for the translated editions. Named rather than globbed; see the
/// module header for why. The axis is carried purely so a wrong one can be
/// *named* as such in the per-page table below — nothing here is graded
/// against it, and `MAX_BLOCK_AREA`'s assertion does not look at it either.
const EDITIONS: [(&str, &str, Axis); 9] = [
    ("gotoubun-v01", "raw-mono", Axis::Vertical),
    ("gotoubun-v01", "en-mono", Axis::Horizontal),
    ("gotoubun-v01", "en-fan", Axis::Horizontal),
    ("kaguya-v01", "raw-mono", Axis::Vertical),
    ("kaguya-v01", "en-fan", Axis::Horizontal),
    ("oshinoko-v01", "raw-mono", Axis::Vertical),
    ("oshinoko-v01", "en-mono", Axis::Horizontal),
    ("kobayashi-v01", "raw-mono", Axis::Vertical),
    ("kobayashi-v01", "en-mono", Axis::Horizontal),
];

/// Pages sampled per edition.
///
/// Each sampled page runs `components` — an O(pixels) flood fill — twice: once
/// for the staged pipeline, once more inside `detect_page` for the
/// cross-check. 10 pages across 9 editions is 90 pages, 180 flood fills, and
/// keeps the whole run in the tens of seconds in release, the same budget
/// `geometry_experiment`'s own `SAMPLE` reasons about.
const SAMPLE: usize = 10;

/// Longer side, in pixels, under which `notes.md` calls a component "tiny" —
/// screentone dots, JPEG ringing and antialiasing fragments, rather than a
/// glyph or a line stroke.
const TINY_STROKE: u32 = 5;

/// The three Task 9 scale rules, measured over the same pages and the same
/// components so the comparison the L2A Task 9 brief asks for is
/// like-for-like. `All` is `stroke_scale` itself, today's production
/// behaviour, called by `detect_page`; `AboveFloor` and `Modal` are the two
/// candidates. Only `All` is cross-checked against `detect_page` in
/// `diagnose_from_strokes` — see `PageDiag::detect_page_len` — because
/// only `All` is what `detect_page` actually calls today.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ScaleRule {
    All,
    AboveFloor,
    Modal,
}

impl ScaleRule {
    const ALL: [ScaleRule; 3] = [ScaleRule::All, ScaleRule::AboveFloor, ScaleRule::Modal];

    fn label(self) -> &'static str {
        match self {
            ScaleRule::All => "all",
            ScaleRule::AboveFloor => "above-floor",
            ScaleRule::Modal => "modal",
        }
    }

    /// This rule's position in `ScaleRule::ALL` — the index a `PageDiag`
    /// triple stores it at.
    fn index(self) -> usize {
        match self {
            ScaleRule::All => 0,
            ScaleRule::AboveFloor => 1,
            ScaleRule::Modal => 2,
        }
    }

    /// The scale this rule computes, calling `detect.rs`'s own function —
    /// never a copy — the same discipline the module header states for
    /// `otsu`, `components`, `axis_of` and `merge_along`.
    fn scale(self, strokes: &[Bounds]) -> Option<u32> {
        match self {
            ScaleRule::All => detect::stroke_scale(strokes),
            ScaleRule::AboveFloor => detect::stroke_scale_above_floor(strokes),
            ScaleRule::Modal => detect::stroke_scale_modal(strokes),
        }
    }
}

/// `count` page indices spread evenly through `0..available`, or every index
/// when there are fewer pages than that.
///
/// The same formula `geometry_experiment.rs`'s `spread_sample` uses, restated
/// over indices rather than items, for the same reason it gives: the front of
/// a volume is colour inserts, a title page and a table of contents — the
/// least representative pages in any volume and the ones likeliest to be
/// blank interstitials.
fn sample_indices(available: usize, count: usize) -> Vec<usize> {
    if count == 0 || available <= count {
        return (0..available).collect();
    }
    (0..count)
        .map(|i| i * available / count + available / (2 * count))
        .collect()
}

/// Strokes wholly inside `block` — the containment test `detect_page` applies
/// for both `MIN_STROKES` and `MIN_INK`. Restated here because in `detect.rs`
/// it lives as an inline closure over a locally-borrowed `strokes`, not a
/// function this module can call; what it tests — `s.x0 >= block.x0` and so on
/// — is a bounding-box containment check, not part of the detector's
/// heuristic, so restating it is bookkeeping rather than a second copy of the
/// algorithm the module header promises not to duplicate.
fn strokes_in<'a>(strokes: &'a [Bounds], block: &Bounds) -> Vec<&'a Bounds> {
    strokes
        .iter()
        .filter(|s| s.x0 >= block.x0 && s.x1 <= block.x1 && s.y0 >= block.y0 && s.y1 <= block.y1)
        .collect()
}

/// One block's area as a fraction of the page — the same quantity
/// `MAX_BLOCK_AREA` is compared against and a `Region`'s `w * h`.
fn area_fraction(block: &Bounds, page_area: f64) -> f64 {
    (f64::from(block.width()) * f64::from(block.height())) / page_area
}

/// The median longer side over a set of components — `stroke_scale`'s own
/// quantity, computed over an arbitrary subset rather than a whole page.
///
/// Restated rather than shared: `detect::stroke_scale` takes `&[Bounds]` and
/// `strokes_in` hands back `Vec<&Bounds>`, so calling it would mean copying
/// every contained stroke into a fresh `Vec` on pages that carry 170,000 of
/// them. The quantity is a median of `longer()`, which is bookkeeping in the
/// same sense `strokes_in` is, not a second copy of the detector's heuristic.
fn median_longer(strokes: &[&Bounds]) -> Option<u32> {
    if strokes.is_empty() {
        return None;
    }
    let mut sides: Vec<u32> = strokes.iter().map(|s| s.longer()).collect();
    sides.sort_unstable();
    Some(sides[sides.len() / 2])
}

/// What one block that cleared all four filters is made of.
///
/// The four filters answer "how many blocks" and nothing about what they hold,
/// which is exactly the hole the `gotoubun-v01/en-mono` result opened: that
/// edition is the noisiest sampled — 93%–99% of its components under
/// `TINY_STROKE` — and the only one of the nine to clear the design's
/// `blocks_per_page` bar, which the screentone-welding account predicts should
/// fail worst of all. A count cannot tell a balloon of type from a patch of
/// screentone that happened to survive; this can.
struct SurvivorDiag {
    /// Fraction of the page — what `median_area` is taken over.
    area: f64,
    width: u32,
    height: u32,
    /// Components wholly inside it — the same population `MIN_STROKES`
    /// counted and `MIN_INK` summed.
    strokes: usize,
    /// Fraction of those components whose longer side is under
    /// `TINY_STROKE`. Against the whole page's own `tiny_fraction`, this is
    /// what says whether a surviving block is made of different material than
    /// the page around it or of exactly the same noise.
    tiny_fraction: f64,
    /// Median longer side of those components, in pixels — the block's own
    /// stroke scale, against the page-wide one `stroke_scale` computed.
    median_stroke: Option<u32>,
    /// Summed stroke bounding-box area over block area — the ratio `MIN_INK`
    /// is compared against. See `MIN_INK`'s doc comment for what it does and
    /// does not establish.
    ink: f64,
}

/// What happened to one page under one scale rule, with every intermediate
/// count `detect_page` itself throws away kept.
struct PageDiag {
    page: usize,
    rule: ScaleRule,
    /// The page's own pixel dimensions. Kept because the corpus spans an
    /// eightfold range of them for editions of the same work — the fact
    /// `stroke_scale`'s doc comment gives for why the gap is stroke-relative
    /// — and because a gap in pixels means nothing without them.
    width: u32,
    height: u32,
    components: usize,
    /// Fraction of `components` whose longer side is under `TINY_STROKE`.
    tiny_fraction: f64,
    /// The largest single component's bounding box as a fraction of the page,
    /// **before any smearing**. `largest_area_discarded` below is measured
    /// after `merge_along`, so it cannot tell a page the smear welded together
    /// from a page that arrived welded — a scan downsampled far enough that
    /// screentone is no longer separable dots binarises into large connected
    /// fields, and then there is nothing for a gap of any size to do.
    largest_component: f64,
    stroke_scale: Option<u32>,
    gap: Option<u32>,
    axis: Option<Axis>,
    /// Blocks surviving, in order: smeared (no filter yet), `MIN_STROKES`,
    /// `MIN_BLOCK_STROKE_SPAN`, `MAX_BLOCK_AREA`, `MIN_INK`. The last entry is
    /// what `detect_page` returns when `rule` is `ScaleRule::All`, and
    /// `detect_page_len` below is the cross-check that it actually is.
    stages: [usize; 5],
    /// The largest block `MAX_BLOCK_AREA` discarded, as a fraction of the
    /// page — `None` when nothing reached that filter, or nothing that did
    /// was over the limit.
    largest_area_discarded: Option<f64>,
    /// Every block that survived all four filters, with what it is made of —
    /// what an edition-level median area is taken over, and what says whether
    /// a surviving block is type or a patch of the same noise as the page.
    survivors: Vec<SurvivorDiag>,
    /// `detect_page(page).len()`, computed independently of the staged
    /// pipeline above — the cross-check the module header names. Only
    /// meaningful for `ScaleRule::All`, the rule `detect_page` actually
    /// calls today; `None` for the two candidates, which nothing in
    /// production calls yet.
    detect_page_len: Option<usize>,
}

/// The one expensive, per-pixel stage (`otsu` + `components`), computed once
/// per page rather than once per scale rule — otherwise measuring three
/// candidates would triple the flood fill for work that does not depend on
/// which rule is under test.
fn page_strokes(page: &GrayImage) -> Vec<Bounds> {
    detect::components(page, detect::otsu(page))
}

/// Run `detect.rs`'s own pipeline on one page's strokes under one scale
/// rule, keeping every stage.
///
/// Calls `detect::stroke_scale` (via `ScaleRule::scale`), `detect::axis_of`
/// and `detect::merge_along` — never a copy of them — in the same order
/// `detect_page` calls them, then applies its four filters one at a time
/// instead of chained, so each stage's survivor count can be kept rather than
/// only the last. For `ScaleRule::All`, `detect::detect_page` is also called
/// on the same page, purely so `PageDiag::detect_page_len` can be checked
/// against `stages[4]` — see the module header.
fn diagnose_from_strokes(
    strokes: &[Bounds],
    page: &GrayImage,
    index: usize,
    rule: ScaleRule,
) -> PageDiag {
    let (width, height) = page.dimensions();
    let page_area = f64::from(width) * f64::from(height);

    let components = strokes.len();
    let tiny = strokes.iter().filter(|s| s.longer() < TINY_STROKE).count();
    let tiny_fraction = match components {
        0 => 0.0,
        n => tiny as f64 / n as f64,
    };
    let largest_component = strokes
        .iter()
        .map(|s| area_fraction(s, page_area))
        .fold(0.0, f64::max);
    let detect_page_len = match rule {
        ScaleRule::All => Some(detect::detect_page(page).len()),
        ScaleRule::AboveFloor | ScaleRule::Modal => None,
    };

    let Some(scale) = rule.scale(strokes) else {
        return PageDiag {
            page: index,
            rule,
            width,
            height,
            components,
            tiny_fraction,
            largest_component,
            stroke_scale: None,
            gap: None,
            axis: None,
            stages: [0; 5],
            largest_area_discarded: None,
            survivors: Vec::new(),
            detect_page_len,
        };
    };

    let gap = (scale * detect::SMEAR_K).max(1);
    let axis = detect::axis_of(strokes, gap);
    let merged = detect::merge_along(strokes, axis, gap);
    let stage0 = merged.len();

    let after_strokes: Vec<Bounds> = merged
        .into_iter()
        .filter(|b| strokes_in(strokes, b).len() >= detect::MIN_STROKES)
        .collect();
    let stage1 = after_strokes.len();

    let after_span: Vec<Bounds> = after_strokes
        .into_iter()
        .filter(|b| b.longer() >= scale * detect::MIN_BLOCK_STROKE_SPAN)
        .collect();
    let stage2 = after_span.len();

    let (after_area, discarded_by_area): (Vec<Bounds>, Vec<Bounds>) = after_span
        .into_iter()
        .partition(|b| area_fraction(b, page_area) <= f64::from(detect::MAX_BLOCK_AREA));
    let stage3 = after_area.len();
    let largest_area_discarded = discarded_by_area
        .iter()
        .map(|b| area_fraction(b, page_area))
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a: f64| a.max(v)))
        });

    let after_ink: Vec<Bounds> = after_area
        .into_iter()
        .filter(|b| {
            let area = f64::from(b.width()) * f64::from(b.height());
            let ink: u64 = strokes_in(strokes, b)
                .iter()
                .map(|s| u64::from(s.width()) * u64::from(s.height()))
                .sum();
            area > 0.0 && ink as f64 / area >= f64::from(detect::MIN_INK)
        })
        .collect();
    let stage4 = after_ink.len();
    let survivors: Vec<SurvivorDiag> = after_ink
        .iter()
        .map(|b| {
            let inside = strokes_in(strokes, b);
            let area = f64::from(b.width()) * f64::from(b.height());
            let ink: u64 = inside
                .iter()
                .map(|s| u64::from(s.width()) * u64::from(s.height()))
                .sum();
            let tiny = inside.iter().filter(|s| s.longer() < TINY_STROKE).count();
            SurvivorDiag {
                area: area_fraction(b, page_area),
                width: b.width(),
                height: b.height(),
                strokes: inside.len(),
                tiny_fraction: match inside.len() {
                    0 => 0.0,
                    n => tiny as f64 / n as f64,
                },
                median_stroke: median_longer(&inside),
                ink: match area {
                    0.0 => 0.0,
                    a => ink as f64 / a,
                },
            }
        })
        .collect();

    PageDiag {
        page: index,
        rule,
        width,
        height,
        components,
        tiny_fraction,
        largest_component,
        stroke_scale: Some(scale),
        gap: Some(gap),
        axis: Some(axis),
        stages: [stage0, stage1, stage2, stage3, stage4],
        largest_area_discarded,
        survivors,
        detect_page_len,
    }
}

/// The `All` rule's diagnosis of one page — a thin wrapper for callers (this
/// module's own fixture tests, and the page-by-page table below) that only
/// need today's production rule and do not want to compute `page_strokes`
/// themselves.
fn diagnose(page: &GrayImage, index: usize) -> PageDiag {
    let strokes = page_strokes(page);
    diagnose_from_strokes(&strokes, page, index, ScaleRule::All)
}

/// Everything one edition contributed — one `[PageDiag; 3]` triple per
/// sampled page, indexed by `ScaleRule::index`, so all three rules were
/// computed from the same strokes on the same page.
struct EditionDiag {
    work: &'static str,
    edition: &'static str,
    expected_axis: Axis,
    /// Pages the edition has, whether or not they were sampled.
    available: usize,
    rows: Vec<[PageDiag; 3]>,
}

impl EditionDiag {
    fn rule_rows(&self, rule: ScaleRule) -> impl Iterator<Item = &PageDiag> {
        self.rows.iter().map(move |triple| &triple[rule.index()])
    }
}

/// Read one edition directly — no sidecar — and diagnose a spread sample of
/// its pages under all three scale rules.
fn measure_edition(
    root: &Path,
    work: &'static str,
    edition: &'static str,
    expected_axis: Axis,
) -> Result<EditionDiag, String> {
    let dir = root.join(work).join(edition);
    let member_paths: Vec<PathBuf> = members(&dir).iter().map(PathBuf::from).collect();
    let pages = taiyaku_read::read_edition(&member_paths)?;
    let available = pages.len();
    let picked = sample_indices(available, SAMPLE);

    let mut rows = Vec::with_capacity(picked.len());
    for index in picked {
        let page = pages
            .get(index)
            .ok_or_else(|| format!("{work}/{edition}: page {index} is past the end"))?;
        let decoded = decode_luma(&page.bytes).map_err(|e| format!("{}: {e}", page.name))?;
        let strokes = page_strokes(&decoded);
        let triple =
            ScaleRule::ALL.map(|rule| diagnose_from_strokes(&strokes, &decoded, index, rule));
        rows.push(triple);
    }

    Ok(EditionDiag {
        work,
        edition,
        expected_axis,
        available,
        rows,
    })
}

/// median, over the areas of every block that survived all four filters,
/// across every sampled page of an edition — the same quantity
/// `regions.rs::median_area` reports, taken over the sample rather than the
/// whole edition. `median_area` is not part of Task 9's acceptance rule (see
/// the L2A Task 9 brief for why) but is kept in the printed tables because it
/// is what `notes.md`'s existing nine-line table already reports.
fn median_final_area<'a>(rows: impl Iterator<Item = &'a PageDiag>) -> f64 {
    median_f64(rows.flat_map(|r| r.survivors.iter().map(|s| s.area))).unwrap_or(0.0)
}

/// Median of a sequence of finite figures, or `None` when it is empty.
fn median_f64(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut values: Vec<f64> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 2])
}

/// One edition's figures under one scale rule — the same shape
/// `notes.md`'s nine `regions:` lines report, taken over the sample.
struct EditionSummary {
    sampled: usize,
    blocks: usize,
    vertical: usize,
    horizontal: usize,
    median_area: f64,
    blocks_per_page: f64,
    /// Median of `stroke_scale` (the rule's own gap-building number, in
    /// pixels) over the sampled pages that had one — a diagnostic, not part
    /// of the acceptance rule, for reading *why* a candidate's block count
    /// moved rather than only that it did.
    median_scale: f64,
}

/// Fold one edition's per-page diagnoses under `rule` into the figures the
/// Task 9 acceptance rule and `notes.md`'s table are stated in terms of.
///
/// Block-weighted, not page-counted: `detect_page` assigns one axis to every
/// block on a page (`axis_of` runs once per page, ahead of the smear), so
/// `regions.rs::survey_line`'s v/h — a count of *blocks* — is this page's
/// final block count folded into whichever side its one axis landed on, not
/// a count of pages.
fn summarize(e: &EditionDiag, rule: ScaleRule) -> EditionSummary {
    let rows: Vec<&PageDiag> = e.rule_rows(rule).collect();
    let blocks: usize = rows.iter().map(|r| r.stages[4]).sum();
    let vertical: usize = rows
        .iter()
        .filter(|r| r.axis == Some(Axis::Vertical))
        .map(|r| r.stages[4])
        .sum();
    let horizontal: usize = rows
        .iter()
        .filter(|r| r.axis == Some(Axis::Horizontal))
        .map(|r| r.stages[4])
        .sum();
    let mut scales: Vec<u32> = rows.iter().filter_map(|r| r.stroke_scale).collect();
    scales.sort_unstable();
    let median_scale = if scales.is_empty() {
        0.0
    } else {
        f64::from(scales[scales.len() / 2])
    };
    EditionSummary {
        sampled: rows.len(),
        blocks,
        vertical,
        horizontal,
        median_area: median_final_area(e.rule_rows(rule)),
        blocks_per_page: blocks as f64 / rows.len() as f64,
        median_scale,
    }
}

/// Which of `detect.rs`'s constants are failing this edition, reproduced
/// against real pages rather than argued about in the abstract.
///
/// See the module header for what is asserted and what deliberately is not.
#[test]
fn region_experiment_reproduces_the_filter_pipeline() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };

    let started = std::time::Instant::now();
    let measured: Vec<EditionDiag> = EDITIONS
        .iter()
        .map(|&(work, edition, expected_axis)| {
            eprintln!("  {work}/{edition}: reading and detecting…");
            let diag = measure_edition(&root, work, edition, expected_axis)
                .unwrap_or_else(|e| panic!("measuring {work}/{edition}: {e}"));
            eprintln!(
                "  {work}/{edition}: {} of {} pages sampled, {:.0}s elapsed",
                diag.rows.len(),
                diag.available,
                started.elapsed().as_secs_f64()
            );
            diag
        })
        .collect();

    println!("\n=== region_experiment: detect.rs's own filter pipeline, page by page ===");
    println!(
        "{} editions, up to {SAMPLE} pages each | otsu/components/axis_of/merge_along called \
         from detect.rs, not copied | rule=all shown page by page; above-floor and modal are \
         summarised per edition below",
        measured.len()
    );
    println!(
        "{:<24} {:>4} {:>11} {:>7} {:>8} {:>6} {:>5} {:>7} {:>7} {:>15} {:>9} {:<10}",
        "edition",
        "pg",
        "page px",
        "comps",
        "under5px",
        "scale",
        "gap",
        "gap/w‰",
        "biggest",
        "stages 0>1>2>3>4",
        "max-area",
        "axis"
    );
    for e in &measured {
        for row in e.rule_rows(ScaleRule::All) {
            let stages = row
                .stages
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(">");
            let axis_note = match row.axis {
                Some(a) if a == e.expected_axis => format!("{a:?}"),
                Some(a) => format!("{a:?} WRONG"),
                None => "-".to_string(),
            };
            println!(
                "{:<24} {:>4} {:>11} {:>7} {:>7.0}% {:>6} {:>5} {:>7} {:>6.1}% {:>15} {:>8} {axis_note}",
                format!("{}/{}", e.work, e.edition),
                row.page,
                format!("{}x{}", row.width, row.height),
                row.components,
                row.tiny_fraction * 100.0,
                row.stroke_scale
                    .map_or_else(|| "-".to_string(), |s| s.to_string()),
                row.gap.map_or_else(|| "-".to_string(), |g| g.to_string()),
                row.gap.map_or_else(
                    || "-".to_string(),
                    |g| format!("{:.1}", f64::from(g) * 1000.0 / f64::from(row.width))
                ),
                row.largest_component * 100.0,
                stages,
                row.largest_area_discarded
                    .map_or_else(|| "-".to_string(), |f| format!("{:.1}%", f * 100.0)),
            );
        }
    }

    // What the four filters cannot say, and the reason this section exists: a
    // block count answers "how many" and nothing about what they hold. Both
    // tables below are over the blocks that cleared all four filters under
    // today's rule — the first summarised per edition, the second block by
    // block, all 9 editions and no second sampling decision.
    //
    // `pg under5` beside `under5px` is the comparison the whole section is
    // for: a block made of the same sub-`TINY_STROKE` material as the page
    // around it is a patch of screentone that survived, and a block made of
    // markedly coarser components than its page is type. A count cannot tell
    // those apart and `blocks_per_page` is a count.
    println!("\n-- what survives, per edition (rule=all): medians over surviving blocks --");
    println!(
        "{:<24} {:>6} {:>11} {:>7} {:>8} {:>8} {:>8} {:>9} {:>8} {:>8} {:>7}",
        "edition",
        "blocks",
        "page px",
        "gap/w‰",
        "biggest",
        "of page",
        "strokes",
        "under5px",
        "pg und5",
        "med str",
        // Survivors whose own median component is not itself under
        // `TINY_STROKE` — the blocks that are made of coarser material than
        // the noise around them. Counted against `TINY_STROKE`, which this
        // module already defines and the `under5px` columns already use,
        // rather than against a fresh threshold: a second unmeasured constant
        // is exactly what a measurement taken before a redesign exists to
        // avoid.
        "coarse"
    );
    for e in &measured {
        let rows: Vec<&PageDiag> = e.rule_rows(ScaleRule::All).collect();
        let survivors: Vec<&SurvivorDiag> = rows.iter().flat_map(|r| r.survivors.iter()).collect();
        let show = |v: Option<f64>, unit: &str| match v {
            Some(v) => format!("{v:.1}{unit}"),
            None => "-".to_string(),
        };
        let coarse = survivors
            .iter()
            .filter(|s| s.median_stroke.is_some_and(|m| m >= TINY_STROKE))
            .count();
        println!(
            "{:<24} {:>6} {:>11} {:>7} {:>8} {:>8} {:>8} {:>9} {:>8} {:>8} {:>4}/{}",
            format!("{}/{}", e.work, e.edition),
            survivors.len(),
            format!(
                "{}x{}",
                median_f64(rows.iter().map(|r| f64::from(r.width))).unwrap_or(0.0) as u32,
                median_f64(rows.iter().map(|r| f64::from(r.height))).unwrap_or(0.0) as u32,
            ),
            show(
                median_f64(
                    rows.iter()
                        .filter_map(|r| r.gap.map(|g| f64::from(g) * 1000.0 / f64::from(r.width)))
                ),
                ""
            ),
            show(
                median_f64(rows.iter().map(|r| r.largest_component * 100.0)),
                "%"
            ),
            show(median_f64(survivors.iter().map(|s| s.area * 100.0)), "%"),
            show(median_f64(survivors.iter().map(|s| s.strokes as f64)), ""),
            show(
                median_f64(survivors.iter().map(|s| s.tiny_fraction * 100.0)),
                "%"
            ),
            show(
                median_f64(rows.iter().map(|r| r.tiny_fraction * 100.0)),
                "%"
            ),
            show(
                median_f64(
                    survivors
                        .iter()
                        .filter_map(|s| s.median_stroke.map(f64::from))
                ),
                "px"
            ),
            coarse,
            survivors.len(),
        );
    }

    // Which of the four filters actually rejects anything on real material,
    // summed over the whole sample. O32 and O33 record that two of the five
    // constants reject nothing in any *test*; this says what they do against
    // the corpus, which is the question a redesign has to answer before it
    // moves any of them.
    println!("\n-- what each filter rejected, over all sampled pages (rule=all) --");
    let all_rows: Vec<&PageDiag> = measured
        .iter()
        .flat_map(|e| e.rule_rows(ScaleRule::All))
        .collect();
    for (i, name) in [
        "MIN_STROKES",
        "MIN_BLOCK_STROKE_SPAN",
        "MAX_BLOCK_AREA",
        "MIN_INK",
    ]
    .iter()
    .enumerate()
    {
        let rejected: usize = all_rows
            .iter()
            .map(|r| r.stages[i].saturating_sub(r.stages[i + 1]))
            .sum();
        let pages = all_rows
            .iter()
            .filter(|r| r.stages[i] > r.stages[i + 1])
            .count();
        println!(
            "{name:<24} rejected {rejected:>5} blocks, on {pages:>2} of {} pages",
            all_rows.len()
        );
    }

    // Whether `merge_along` is what builds the page-spanning blob, or whether
    // it arrives already built. `notes.md`'s diagnosis says the former — "a
    // few pixels of tolerance already lets `merge_along`'s transitive passes
    // weld thousands of specks into one component spanning most of a page" —
    // and that is testable by comparing the largest block the smear produced
    // against the largest component that existed before it ran.
    let welded_before: usize = all_rows
        .iter()
        .filter(|r| {
            r.largest_area_discarded
                .is_some_and(|d| (d - r.largest_component).abs() < 0.01)
        })
        .count();
    let discarded_pages = all_rows
        .iter()
        .filter(|r| r.largest_area_discarded.is_some())
        .count();
    println!(
        "\nof the {discarded_pages} pages where MAX_BLOCK_AREA discarded something, the largest \
         thing discarded was\nalready one connected component before any smearing on \
         {welded_before} of them (within 1 point of page area)"
    );

    println!(
        "\n-- every block that cleared all four filters (rule=all), and what it is made of --"
    );
    println!(
        "{:<24} {:>4} {:>11} {:>8} {:>8} {:>8} {:>9} {:>8} {:>8} {:<10}",
        "edition",
        "pg",
        "block px",
        "of page",
        "strokes",
        "under5px",
        "pg under5",
        "med str",
        "ink",
        "axis"
    );
    for e in &measured {
        for row in e.rule_rows(ScaleRule::All) {
            for s in &row.survivors {
                let axis_note = match row.axis {
                    Some(a) if a == e.expected_axis => format!("{a:?}"),
                    Some(a) => format!("{a:?} WRONG"),
                    None => "-".to_string(),
                };
                println!(
                    "{:<24} {:>4} {:>11} {:>7.1}% {:>8} {:>7.0}% {:>8.0}% {:>6}px {:>7.2} {axis_note}",
                    format!("{}/{}", e.work, e.edition),
                    row.page,
                    format!("{}x{}", s.width, s.height),
                    s.area * 100.0,
                    s.strokes,
                    s.tiny_fraction * 100.0,
                    row.tiny_fraction * 100.0,
                    s.median_stroke
                        .map_or_else(|| "-".to_string(), |m| m.to_string()),
                    s.ink,
                );
            }
        }
    }

    // Per edition, over the sample only — explicitly "of N sampled" rather
    // than "of N pages": `taiyaku regions` (and `notes.md`'s nine `regions:`
    // lines) count every page of the edition, this counts only the ones drawn
    // above, and conflating the two would silently claim a coverage this
    // module does not have. Printed once per scale rule, all three over the
    // same sample, so the Task 9 comparison is like-for-like.
    println!(
        "\n-- per edition, over the sample, by scale rule (beside notes.md's `regions:` line) --"
    );
    let summaries: Vec<[EditionSummary; 3]> = measured
        .iter()
        .map(|e| ScaleRule::ALL.map(|rule| summarize(e, rule)))
        .collect();
    for rule in ScaleRule::ALL {
        println!("\nrule={}", rule.label());
        println!(
            "{:<24} {:>9} {:>7} {:>5} {:>5} {:>12} {:>12} {:>11}",
            "edition", "sampled", "blocks", "v", "h", "median area", "blocks/page", "med. scale"
        );
        for (e, triple) in measured.iter().zip(&summaries) {
            let s = &triple[rule.index()];
            println!(
                "{:<24} {:>4}/{:<4} {:>7} {:>5} {:>5} {:>12.4} {:>12.2} {:>10.1}px",
                format!("{}/{}", e.work, e.edition),
                s.sampled,
                e.available,
                s.blocks,
                s.vertical,
                s.horizontal,
                s.median_area,
                s.blocks_per_page,
                s.median_scale,
            );
        }
    }

    // The two Task 9 acceptance checks, computed directly rather than left
    // for a person to eyeball out of the tables above: `blocks_per_page`
    // above 2 (the design's own bar) and the v/h split following the edition
    // — a majority-vertical raw and a majority-horizontal English edition —
    // on both editions of at least one work. See the L2A Task 9 brief.
    println!("\n-- Task 9 acceptance check --");
    println!(
        "{:<12} {:>8} {:>8} {:>7}  works where raw is majority-v and some en edition is majority-h",
        "rule", "min b/p", "mean b/p", "n>2/9"
    );
    for rule in ScaleRule::ALL {
        let values: Vec<f64> = summaries
            .iter()
            .map(|t| t[rule.index()].blocks_per_page)
            .collect();
        let min_bpp = values.iter().copied().fold(f64::INFINITY, f64::min);
        let mean_bpp = values.iter().sum::<f64>() / values.len() as f64;
        let clearing = values.iter().filter(|&&v| v > 2.0).count();

        let mut matching_works: Vec<&str> = Vec::new();
        for &work in &[
            "gotoubun-v01",
            "kaguya-v01",
            "oshinoko-v01",
            "kobayashi-v01",
        ] {
            let raw_majority_v = measured
                .iter()
                .zip(&summaries)
                .find(|(e, _)| e.work == work && e.edition == "raw-mono")
                .is_some_and(|(_, t)| {
                    let s = &t[rule.index()];
                    s.vertical > s.horizontal
                });
            let some_en_majority_h = measured.iter().zip(&summaries).any(|(e, t)| {
                e.work == work && e.edition != "raw-mono" && {
                    let s = &t[rule.index()];
                    s.horizontal > s.vertical
                }
            });
            if raw_majority_v && some_en_majority_h {
                matching_works.push(work);
            }
        }
        println!(
            "{:<12} {:>8.2} {:>8.2} {:>4}/9  {matching_works:?}",
            rule.label(),
            min_bpp,
            mean_bpp,
            clearing,
        );
    }

    println!("\nmeasured in {:.1}s", started.elapsed().as_secs_f64());

    // The floor, and only the floor. Nothing here bounds a block count, a
    // median area or an axis split — that is a decision for later, once there
    // is something to decide between, per the L2A task 8 brief. The Task 9
    // acceptance decision is made by a person reading the printed tables
    // above, not by an assertion here, for the same reason.
    let total_sampled: usize = measured.iter().map(|e| e.rows.len()).sum();
    assert!(
        total_sampled > 0,
        "the sample is empty: no edition matched, or no page decoded"
    );
    for e in &measured {
        assert!(
            !e.rows.is_empty(),
            "{}/{} contributed nothing: 0 of {} pages",
            e.work,
            e.edition,
            e.available
        );
        assert_eq!(
            e.rows.len(),
            sample_indices(e.available, SAMPLE).len(),
            "{}/{}: not every sampled page produced a row",
            e.work,
            e.edition
        );
    }
    for e in &measured {
        for triple in &e.rows {
            for row in triple {
                assert!(
                    row.tiny_fraction.is_finite(),
                    "{}/{} page {} rule={}: tiny_fraction is not finite",
                    e.work,
                    e.edition,
                    row.page,
                    row.rule.label()
                );
                if let Some(f) = row.largest_area_discarded {
                    assert!(
                        f.is_finite(),
                        "{}/{} page {} rule={}: largest_area_discarded is not finite",
                        e.work,
                        e.edition,
                        row.page,
                        row.rule.label()
                    );
                }
                assert!(
                    row.largest_component.is_finite(),
                    "{}/{} page {} rule={}: largest_component is not finite",
                    e.work,
                    e.edition,
                    row.page,
                    row.rule.label()
                );
                // The survivor profile has to cover exactly the blocks the
                // staged pipeline says survived, or the table it prints is
                // describing a different set of blocks than the counts above
                // it — the same bookkeeping-not-quality assertion as the
                // `detect_page_len` check below.
                assert_eq!(
                    row.survivors.len(),
                    row.stages[4],
                    "{}/{} page {} rule={}: {} survivors profiled against {} surviving blocks",
                    e.work,
                    e.edition,
                    row.page,
                    row.rule.label(),
                    row.survivors.len(),
                    row.stages[4]
                );
                for s in &row.survivors {
                    assert!(
                        s.area.is_finite() && s.tiny_fraction.is_finite() && s.ink.is_finite(),
                        "{}/{} page {} rule={}: a surviving block's profile is not finite",
                        e.work,
                        e.edition,
                        row.page,
                        row.rule.label()
                    );
                    // A block reached `MIN_STROKES` to get here, so it holds
                    // components and therefore has a median size. `None` here
                    // would mean `strokes_in` disagreed with the filter that
                    // admitted the block.
                    assert!(
                        s.median_stroke.is_some(),
                        "{}/{} page {} rule={}: a surviving block holds no strokes",
                        e.work,
                        e.edition,
                        row.page,
                        row.rule.label()
                    );
                }
                // The check specific to this module: calling detect.rs's real
                // functions has to reproduce detect.rs's real answer, or this
                // module is measuring its own bookkeeping rather than the
                // detector. Only `ScaleRule::All` has a `detect_page_len` to
                // check against — the two candidates are not called by
                // `detect_page` (see `PageDiag::detect_page_len`).
                if let Some(detect_page_len) = row.detect_page_len {
                    assert_eq!(
                        row.stages[4],
                        detect_page_len,
                        "{}/{} page {} rule={}: the staged pipeline landed on {} blocks \
                         but detect_page returned {} for the same page",
                        e.work,
                        e.edition,
                        row.page,
                        row.rule.label(),
                        row.stages[4],
                        detect_page_len
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fewer pages than asked for is everything, unchanged.
    #[test]
    fn fewer_pages_than_the_sample_is_every_page() {
        assert_eq!(sample_indices(5, 10), vec![0, 1, 2, 3, 4]);
    }

    /// The sample must not start at the very first page, which is colour
    /// inserts and front matter, and must never run off the end.
    #[test]
    fn the_sample_is_spread_through_the_edition_rather_than_taken_from_its_front() {
        let picked = sample_indices(100, 8);
        assert_eq!(picked.len(), 8);
        assert!(picked[0] > 0, "not the very first page: {picked:?}");
        assert!(picked[7] < 100);
        assert!(picked.windows(2).all(|w| w[0] < w[1]), "{picked:?}");
    }

    /// A page with one balloon's worth of vertical text and nothing else —
    /// the same fixture `detect.rs`'s own
    /// `a_column_of_glyphs_is_one_vertical_region` uses, so this pins that
    /// `diagnose` reproduces `detect_page`'s answer on a fixture whose answer
    /// is already known, before either is ever pointed at the corpus.
    fn page_with_a_column() -> GrayImage {
        let marks: Vec<(u32, u32, u32, u32)> =
            (0..6).map(|i| (100u32, 40 + i * 30, 20, 20)).collect();
        GrayImage::from_fn(400, 600, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            image::Luma([if inked { 0 } else { 255 }])
        })
    }

    #[test]
    fn diagnose_reproduces_detect_page_on_a_known_fixture() {
        let diag = diagnose(&page_with_a_column(), 0);
        assert_eq!(diag.stages[4], 1, "one block, got {:?}", diag.stages);
        assert_eq!(diag.detect_page_len, Some(diag.stages[4]));
        assert_eq!(diag.axis, Some(Axis::Vertical));
        assert!(diag.stroke_scale.is_some());
        assert!(diag.gap.is_some());
        assert_eq!(diag.survivors.len(), 1);
        assert!(
            diag.largest_area_discarded.is_none(),
            "nothing else to discard"
        );
    }

    /// The survivor profile describes the block it came from, not the page.
    ///
    /// The fixture is six 20x20 marks in a column on a 400x600 page: the one
    /// surviving block holds exactly those six, none of them under
    /// `TINY_STROKE`, and its area is the column's rather than the page's.
    /// Without this, `SurvivorDiag` could be summing over the whole page and
    /// the corpus tables would look plausible while measuring nothing local.
    #[test]
    fn a_survivor_is_profiled_from_its_own_strokes() {
        let diag = diagnose(&page_with_a_column(), 0);
        let [survivor] = &diag.survivors[..] else {
            panic!("one block, got {}", diag.survivors.len());
        };
        assert_eq!(survivor.strokes, 6, "the six marks, and nothing else");
        assert_eq!(survivor.tiny_fraction, 0.0, "a 20px mark is not tiny");
        assert_eq!(survivor.median_stroke, Some(20));
        // Six marks 20px tall on a 30px pitch: y0=40 to y1=209, so 170 tall.
        assert_eq!((survivor.width, survivor.height), (20, 170));
        assert!(
            survivor.area < 0.02,
            "the column's area, not the page's: {}",
            survivor.area
        );
        assert!(
            (survivor.ink - 6.0 * 400.0 / (20.0 * 170.0)).abs() < 1e-9,
            "six 20x20 marks over the block's own box: {}",
            survivor.ink
        );
    }

    /// The largest raw component is measured before the smear, so a page whose
    /// ink arrives already welded is distinguishable from one the gap welded.
    #[test]
    fn the_largest_component_is_measured_before_any_smear() {
        // One mark spanning a quarter of the page, plus the column above it.
        let page = GrayImage::from_fn(400, 600, |x, y| {
            let bar = (0..200).contains(&x) && (300..600).contains(&y);
            image::Luma([if bar { 0 } else { 255 }])
        });
        let diag = diagnose(&page, 0);
        assert!(
            (diag.largest_component - 0.25).abs() < 1e-9,
            "a 200x300 bar on a 400x600 page is a quarter of it: {}",
            diag.largest_component
        );
    }

    /// A blank page has no strokes, so every stage is `None`/zero rather than
    /// a division by zero or a panic — the same shape `stroke_scale`'s own
    /// `None` return takes in `detect.rs`.
    #[test]
    fn a_blank_page_has_no_stages() {
        let blank = GrayImage::from_fn(400, 600, |_, _| image::Luma([255]));
        let diag = diagnose(&blank, 0);
        assert_eq!(diag.components, 0);
        assert_eq!(diag.tiny_fraction, 0.0);
        assert_eq!(diag.stroke_scale, None);
        assert_eq!(diag.gap, None);
        assert_eq!(diag.stages, [0; 5]);
        assert_eq!(diag.detect_page_len, Some(0));
    }

    /// The two candidates are exercised by `region_experiment_reproduces_the_
    /// filter_pipeline` against real corpus pages, but that test is gated on
    /// `TAIYAKU_CORPUS`; this fixture pins their wiring — that
    /// `diagnose_from_strokes` actually calls each rule's function and that
    /// only `All` carries a `detect_page_len` — without the corpus.
    #[test]
    fn diagnose_from_strokes_wires_up_all_three_scale_rules() {
        let page = page_with_a_column();
        let strokes = page_strokes(&page);

        let all = diagnose_from_strokes(&strokes, &page, 0, ScaleRule::All);
        let above_floor = diagnose_from_strokes(&strokes, &page, 0, ScaleRule::AboveFloor);
        let modal = diagnose_from_strokes(&strokes, &page, 0, ScaleRule::Modal);

        assert!(all.stroke_scale.is_some());
        assert!(above_floor.stroke_scale.is_some());
        assert!(modal.stroke_scale.is_some());
        assert_eq!(all.detect_page_len, Some(all.stages[4]));
        assert_eq!(above_floor.detect_page_len, None);
        assert_eq!(modal.detect_page_len, None);
    }
}
