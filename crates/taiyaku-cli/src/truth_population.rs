//! What text is made of, measured from the boxes a person drew.
//!
//! `detect.rs`'s module doc says which of its constants cite these
//! measurements and which are starting values. The material is 480
//! hand-drawn boxes over 45 pages.
//! The components *inside* a box are the glyph population; the ones outside are
//! the reject population. A size band, an ink floor and a smear gap are each
//! read off those two distributions rather than guessed.
//!
//! **The shape is `region_experiment.rs`'s**, deliberately, rather than a
//! second convention: `#[cfg(test)]`, gated on `TAIYAKU_CORPUS`, skipping
//! cleanly when unset, calling `detect.rs`'s own `otsu`, `components` and
//! `merge_along` rather than copies, and asserting only that it measured
//! something — never a quality figure. A measurement that fails when the thing
//! it measures improves is a check pretending to be a measurement.
//!
//! It reads its pages through `score::resolve_labelled`, so it measures
//! **exactly the pages `taiyaku score` scores**. Resolving them independently
//! would be two chances to disagree while both printed plausible numbers.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli truth_population -- --nocapture
//! ```
//!
//! **`--release` is not optional in practice**: `components` is a flood fill
//! over every pixel of a full-resolution scan, once per labelled page.

use std::path::PathBuf;

use image::GrayImage;
use taiyaku_core::{Axis, TruthRegion};

use crate::corpus::{corpus, members};
use crate::detect::{self, Bounds};
use crate::score::resolve_labelled;

/// The nine editions of four works that carry labels, named rather than
/// globbed. A glob that stops matching is the silent-nothing failure the
/// assertions below refuse to have — the same reasoning `region_experiment`'s
/// own `EDITIONS` gives.
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

/// A truth box in this page's pixels, inclusive on both corners, alongside the
/// axis the labeller gave it.
struct TruthPixels {
    bounds: Bounds,
    axis: Option<Axis>,
}

/// A page-fraction truth box in the pixels of a page this size.
///
/// Saturating rather than wrapping at the right and bottom edges: a box drawn
/// flush to the edge rounds to exactly `width`, and a `Bounds` is inclusive,
/// so the last valid coordinate is `width - 1`.
fn to_pixels(t: &TruthRegion, width: u32, height: u32) -> Option<TruthPixels> {
    if width == 0 || height == 0 {
        return None;
    }
    let scale = |v: f64, span: u32| -> u32 {
        let clamped = v.clamp(0.0, 1.0) * f64::from(span);
        (clamped as u32).min(span - 1)
    };
    let x0 = scale(t.x, width);
    let y0 = scale(t.y, height);
    let x1 = scale(t.x + t.w, width);
    let y1 = scale(t.y + t.h, height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(TruthPixels {
        bounds: Bounds { x0, y0, x1, y1 },
        axis: t.axis,
    })
}

/// Whether a component belongs to a truth box: its **centre** lies inside.
///
/// The alternative, full containment, is wrong for a glyph clipped by the edge
/// of a hand-drawn box, and a human drawing 480 boxes clips some. Picked here
/// rather than left implicit, because the two rules give different populations
/// and the band is read off whichever one this is.
fn centre_inside(stroke: &Bounds, box_: &Bounds) -> bool {
    let cx = stroke.x0 + stroke.width() / 2;
    let cy = stroke.y0 + stroke.height() / 2;
    cx >= box_.x0 && cx <= box_.x1 && cy >= box_.y0 && cy <= box_.y1
}

/// The smallest gap at which `strokes` merge into a single block along `axis`,
/// searched over `0..=limit`.
///
/// Binary search is valid because `merge_along`'s block count is monotone
/// non-increasing in the gap: a larger tolerance can only join more. Calls
/// `merge_along` itself rather than reimplementing the join rule, so this
/// measures the gap **the production smear would need**, not the gap some
/// other definition of adjacency would.
///
/// `None` when they never merge at any gap up to `limit` — two components that
/// do not overlap *across* the axis never join however close they are along
/// it, which is `merge_along`'s documented and deliberate behaviour.
fn merge_gap(strokes: &[Bounds], axis: Axis, limit: u32) -> Option<u32> {
    if strokes.len() <= 1 {
        return Some(0);
    }
    if detect::merge_along(strokes, axis, limit).len() != 1 {
        return None;
    }
    let (mut lo, mut hi) = (0u32, limit);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if detect::merge_along(strokes, axis, mid).len() == 1 {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

/// Ink pixels inside `bounds`, as a fraction of its area.
///
/// **Counts pixels, not bounding boxes**, which would double-count every
/// overlap between strokes (O34). This is the quantity `MIN_INK` is a floor on,
/// measured on the material.
fn ink_fraction(page: &GrayImage, threshold: u8, bounds: &Bounds) -> f64 {
    let mut ink = 0u64;
    for y in bounds.y0..=bounds.y1 {
        for x in bounds.x0..=bounds.x1 {
            if page.get_pixel(x, y).0[0] <= threshold {
                ink += 1;
            }
        }
    }
    let area = u64::from(bounds.width()) * u64::from(bounds.height());
    if area == 0 {
        return 0.0;
    }
    ink as f64 / area as f64
}

/// Everything one page contributes to the two distributions.
#[derive(Default)]
struct PageMeasure {
    /// Longer side of every component whose centre is inside a scored box.
    inside_sides: Vec<u32>,
    /// Longer side of every other component on the page.
    outside_sides: Vec<u32>,
    /// Components per scored truth box.
    per_box: Vec<usize>,
    /// True ink fraction per scored truth box.
    ink: Vec<f64>,
    /// Smallest gap merging one box's own components into one block, in
    /// pixels. Boxes whose axis the labeller declined are skipped.
    within_gap: Vec<u32>,
    /// Smallest gap welding two boxes into one block — the ceiling.
    between_gap: Vec<u32>,
    height: u32,
}

fn measure_page(page: &GrayImage, drawn: &[TruthRegion]) -> PageMeasure {
    let (width, height) = page.dimensions();
    let threshold = detect::otsu(page);
    let strokes = detect::components(page, threshold);

    let boxes: Vec<TruthPixels> = drawn
        .iter()
        .filter(|t| !t.ignore)
        .filter_map(|t| to_pixels(t, width, height))
        .collect();

    let mut m = PageMeasure {
        height,
        ..PageMeasure::default()
    };

    // Which box each component belongs to, or none.
    let owner: Vec<Option<usize>> = strokes
        .iter()
        .map(|s| boxes.iter().position(|b| centre_inside(s, &b.bounds)))
        .collect();

    for (stroke, owner) in strokes.iter().zip(&owner) {
        if owner.is_some() {
            m.inside_sides.push(stroke.longer());
        } else {
            m.outside_sides.push(stroke.longer());
        }
    }

    // The gap search is bounded by the page's longer side: no gap beyond that
    // can join anything a smaller one did not.
    let limit = width.max(height);

    for (index, b) in boxes.iter().enumerate() {
        let mine: Vec<Bounds> = strokes
            .iter()
            .zip(&owner)
            .filter(|(_, o)| **o == Some(index))
            .map(|(s, _)| *s)
            .collect();
        m.per_box.push(mine.len());
        m.ink.push(ink_fraction(page, threshold, &b.bounds));
        if let Some(axis) = b.axis
            && let Some(gap) = merge_gap(&mine, axis, limit)
        {
            m.within_gap.push(gap);
        }
    }

    // The ceiling: for every pair of boxes sharing an axis, the gap at which
    // their two component sets weld into one block. A smear gap at or above
    // this merges two balloons and produces a `merged`.
    for (i, b) in boxes.iter().enumerate() {
        let Some(axis) = b.axis else { continue };
        for (j, other) in boxes.iter().enumerate() {
            if i >= j || other.axis != Some(axis) {
                continue;
            }
            let both: Vec<Bounds> = strokes
                .iter()
                .zip(&owner)
                .filter(|(_, o)| **o == Some(i) || **o == Some(j))
                .map(|(s, _)| *s)
                .collect();
            if let Some(gap) = merge_gap(&both, axis, limit) {
                m.between_gap.push(gap);
            }
        }
    }

    m
}

/// The one percentile index this file uses: `round((n - 1) · p)`, clamped into
/// range.
///
/// **One convention, in one place**, so the ink line and `quantile`, printed
/// together under the same names, index the same way. `MIN_INK`'s doc cites the
/// ink line's p05, so which convention produced it is not a detail.
///
/// Also the file's only cast from a length to a float and back, rather than one
/// per percentile.
fn percentile_index(len: usize, p: f64) -> usize {
    if len == 0 {
        return 0;
    }
    let i = ((len as f64 - 1.0) * p).round() as usize;
    i.min(len - 1)
}

fn quantile(sorted: &[u32], p: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[percentile_index(sorted.len(), p)]
}

/// `quantile` over a float distribution — the ink fractions — so the two are
/// the same percentile of the same shape and not merely the same word.
fn quantile_f64(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[percentile_index(sorted.len(), p)]
}

#[test]
fn what_text_is_made_of() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };

    let mut inside: Vec<u32> = Vec::new();
    let mut outside: Vec<u32> = Vec::new();
    let mut inside_frac: Vec<u32> = Vec::new();
    let mut outside_frac: Vec<u32> = Vec::new();
    let mut per_box: Vec<u32> = Vec::new();
    let mut ink: Vec<f64> = Vec::new();
    let mut within: Vec<u32> = Vec::new();
    let mut between: Vec<u32> = Vec::new();
    let mut heights: Vec<u32> = Vec::new();
    let mut pages_seen = 0usize;
    let mut editions_seen = 0usize;

    // Declared before the edition loop rather than after it, because the
    // per-edition rows below print the same five percentiles as the global
    // table and a second formatter would be a second convention.
    let q = |v: &[u32]| {
        format!(
            "p05={} p25={} med={} p75={} p95={}",
            quantile(v, 0.05),
            quantile(v, 0.25),
            quantile(v, 0.50),
            quantile(v, 0.75),
            quantile(v, 0.95)
        )
    };

    for (work, edition) in EDITIONS {
        let dir = root.join(work).join(edition);
        let sidecar = root.join(work).join("taiyaku.json");
        let paths: Vec<PathBuf> = members(&dir).into_iter().map(PathBuf::from).collect();
        let resolved = resolve_labelled(
            sidecar.to_str().expect("a corpus path is valid UTF-8"),
            &paths,
        )
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // Measure **inside** the decode closure. `decode_all` bounds memory by
        // decoding the large pages one at a time, and collecting every decoded
        // `GrayImage` first throws that away.
        //
        // The closure is handed no index, so truth is looked up by page name.
        // Names are unique within an edition (archive entry names, or
        // filenames on disk); the assertion says so out loud rather than
        // trusting it, because a collision would silently score one page's
        // components against another page's boxes.
        let by_name: std::collections::BTreeMap<&str, &Vec<TruthRegion>> = resolved
            .pages
            .iter()
            .map(|p| p.name.as_str())
            .zip(&resolved.truth)
            .collect();
        assert_eq!(
            by_name.len(),
            resolved.pages.len(),
            "{work}/{edition}: two labelled pages share a name, so truth cannot be \
             matched to them by name"
        );
        let measured = crate::fingerprint::decode_all(&resolved.pages, |page, luma| {
            measure_page(luma, by_name[page.name.as_str()])
        })
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // Per edition as well as globally, because the global row cannot
        // separate two explanations of the raw editions' zero dialogue recall
        // in the dev-set scores: "vertical type does not smear into one block"
        // and "the raw editions are scanned at a different size". `BAND`'s
        // `lo` is a fraction of page height, and a single page-height range
        // over all 45 pages cannot tell them apart. Per edition it can: if the
        // zero tracks page height, the two stay entangled; if it does not, the
        // scan-scale account is refused. No extra decode — `m.height` is
        // already per page and this loop already exists.
        let mut edition_inside = 0usize;
        let mut edition_heights: Vec<u32> = Vec::new();
        let mut edition_inside_frac: Vec<u32> = Vec::new();
        let mut edition_outside_frac: Vec<u32> = Vec::new();
        for m in &measured {
            pages_seen += 1;
            edition_inside += m.inside_sides.len();
            heights.push(m.height);
            edition_heights.push(m.height);
            for &side in &m.inside_sides {
                inside.push(side);
                // Per ten-thousandths of page height, so a fraction survives
                // as an integer and the band can be read off directly.
                let frac = (u64::from(side) * 10_000 / u64::from(m.height)) as u32;
                inside_frac.push(frac);
                edition_inside_frac.push(frac);
            }
            for &side in &m.outside_sides {
                // Same conversion as `inside_frac` above, over the outside
                // population instead: the figure `BAND`'s doc comment quotes.
                let frac = (u64::from(side) * 10_000 / u64::from(m.height)) as u32;
                outside_frac.push(frac);
                edition_outside_frac.push(frac);
            }
            outside.extend(&m.outside_sides);
            per_box.extend(m.per_box.iter().map(|&n| n as u32));
            ink.extend(&m.ink);
            within.extend(&m.within_gap);
            between.extend(&m.between_gap);
        }

        edition_heights.sort_unstable();
        edition_inside_frac.sort_unstable();
        edition_outside_frac.sort_unstable();

        println!(
            "edition={work}/{edition} pages={} inside={} outside={}",
            measured.len(),
            edition_inside,
            measured
                .iter()
                .map(|m| m.outside_sides.len())
                .sum::<usize>(),
        );
        println!(
            "  page height px  min={} med={} max={}",
            edition_heights.first().copied().unwrap_or(0),
            edition_heights
                .get(percentile_index(edition_heights.len(), 0.50))
                .copied()
                .unwrap_or(0),
            edition_heights.last().copied().unwrap_or(0),
        );
        println!("  inside  ‱page-h {}", q(&edition_inside_frac));
        println!("  outside ‱page-h {}", q(&edition_outside_frac));
        editions_seen += 1;
    }

    inside.sort_unstable();
    outside.sort_unstable();
    inside_frac.sort_unstable();
    outside_frac.sort_unstable();
    per_box.sort_unstable();
    within.sort_unstable();
    between.sort_unstable();
    heights.sort_unstable();
    ink.sort_by(f64::total_cmp);

    println!("\ninside  longer-side px      {}", q(&inside));
    println!("outside longer-side px      {}", q(&outside));
    println!("inside  longer-side ‱page-h {}", q(&inside_frac));
    println!("outside longer-side ‱page-h {}", q(&outside_frac));
    // Pixel figures converted to ten-thousandths of page height assume a page
    // height; this is the range that assumption stands on, over the same 45 dev
    // pages, one height per page. The per-edition rows above are the same
    // figure split nine ways, which is the form that can say whether an
    // edition's page size explains its score.
    println!(
        "page height px  min={} med={} max={}",
        heights.first().copied().unwrap_or(0),
        heights
            .get(percentile_index(heights.len(), 0.50))
            .copied()
            .unwrap_or(0),
        heights.last().copied().unwrap_or(0),
    );
    println!("components per truth box    {}", q(&per_box));
    println!("within-box merge gap px     {}", q(&within));
    println!("between-box merge gap px    {}", q(&between));
    println!(
        "true ink fraction in box    p05={:.3} med={:.3} p95={:.3}",
        quantile_f64(&ink, 0.05),
        quantile_f64(&ink, 0.50),
        quantile_f64(&ink, 0.95)
    );

    // Assertions: that it measured something. Never a quality figure — those
    // move when the detector improves, and a check that fails on improvement
    // is not a measurement.
    assert_eq!(
        editions_seen,
        EDITIONS.len(),
        "every named edition measured"
    );
    assert_eq!(pages_seen, 45, "every labelled page measured");
    assert!(!inside.is_empty(), "some component fell inside a truth box");
    assert!(!outside.is_empty(), "some component fell outside one");
    assert!(!within.is_empty(), "some box's own components merged");
    assert!(ink.iter().all(|v| v.is_finite()), "every ink figure finite");
}
