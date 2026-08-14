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
/// `diagnose` reproduces `detect_page`'s band-free answer on a fixture
/// whose answer is already known, before either is ever pointed at the
/// corpus. (Its 20px marks on a 400x600 page are 333‱ — well inside
/// `BAND` — so the banded and band-free answers agree here too; the
/// distinction only bites on corpus pages with small components.)
fn page_with_a_column() -> GrayImage {
    let marks: Vec<(u32, u32, u32, u32)> = (0..6).map(|i| (100u32, 40 + i * 30, 20, 20)).collect();
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
    assert_eq!(diag.band_free_len, Some(diag.stages[4]));
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
    // Ink *pixels* over the block's own box. The marks are solid and
    // disjoint, so a pixel count and the summed bounding boxes this used
    // to be come to the same 2,400 — which is exactly why this fixture
    // does not pin that change and `detect.rs`'s
    // `a_block_of_hollow_strokes_is_mostly_paper` does.
    assert!(
        (survivor.ink - 6.0 * 400.0 / (20.0 * 170.0)).abs() < 1e-9,
        "six solid 20x20 marks over the block's own box: {}",
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
    assert_eq!(diag.band_free_len, Some(0));
}

/// The two candidates are exercised by `region_experiment_reproduces_the_
/// filter_pipeline` against real corpus pages, but that test is gated on
/// `TAIYAKU_CORPUS`; this fixture pins their wiring — that
/// `diagnose_from_strokes` actually calls each rule's function and that
/// only `All` carries a `band_free_len` — without the corpus.
#[test]
fn diagnose_from_strokes_wires_up_all_three_scale_rules() {
    let page = page_with_a_column();
    let (strokes, threshold) = page_strokes(&page);

    let all = diagnose_from_strokes(&strokes, threshold, &page, 0, ScaleRule::All);
    let above_floor = diagnose_from_strokes(&strokes, threshold, &page, 0, ScaleRule::AboveFloor);
    let modal = diagnose_from_strokes(&strokes, threshold, &page, 0, ScaleRule::Modal);

    assert!(all.stroke_scale.is_some());
    assert!(above_floor.stroke_scale.is_some());
    assert!(modal.stroke_scale.is_some());
    assert_eq!(all.band_free_len, Some(all.stages[4]));
    assert_eq!(above_floor.band_free_len, None);
    assert_eq!(modal.band_free_len, None);
}
