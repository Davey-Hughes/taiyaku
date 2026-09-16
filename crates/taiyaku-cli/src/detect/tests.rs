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
    let marks: Vec<(u32, u32, u32, u32)> = (0..6).map(|i| (100u32, 40 + i * 30, 20, 20)).collect();
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
    let marks: Vec<(u32, u32, u32, u32)> = (0..10).map(|i| (100 + 5 * i, 17 * i, 6, 6)).collect();
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

/// Solid black rectangles, `(x0, y0, w, h)`, on a white page of any size.
fn page_of(width: u32, height: u32, marks: &[(u32, u32, u32, u32)]) -> GrayImage {
    GrayImage::from_fn(width, height, |x, y| {
        let inked = marks
            .iter()
            .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
        Luma([if inked { 0 } else { 255 }])
    })
}

/// Six bars on a 400x600 page, each 40px long and `thick` px thick, on a 15px
/// pitch: stacked down the page, so they smear into one vertical block, or
/// with `turned` laid side by side, so they smear into one horizontal one.
///
/// Every bar's longer side is 40 whichever way it lies, so `stroke_scale` is
/// 40 on all four pages this draws and `MIN_BLOCK_STROKE_SPAN` asks for a
/// block 80px long. The block is `15 * 5 + thick` long — 79 at `thick` 4, 80
/// at 5 — and 40 across, because the bars are.
fn six_bars(thick: u32, turned: bool) -> GrayImage {
    let marks: Vec<(u32, u32, u32, u32)> = (0..6)
        .map(|k| {
            if turned {
                (100 + 15 * k, 200, thick, 40)
            } else {
                (100, 200 + 15 * k, 40, thick)
            }
        })
        .collect();
    page_of(400, 600, &marks)
}

/// **`MIN_BLOCK_STROKE_SPAN` rejects a block on its own, and one pixel more
/// gets through** — O33, which recorded that no fixture here could fail if
/// this filter's arithmetic were wrong by a factor of several: everything this
/// file refuses is refused before the filter, and every block it keeps spans
/// many stroke-scales.
///
/// A block can only be short against the page's own strokes if those strokes
/// lie *across* it, which is why these are bars rather than glyph-sized
/// squares: four square marks in a line already span four scales. Nothing
/// else is near its limit on any of the four pages: six strokes against
/// `MIN_STROKES`' four, 666‱ inside `BAND`, ink at 0.30 and 0.38 against
/// `MIN_INK`'s 0.05, and a block about 0.013 of the page. So the 80px page
/// coming back as a region is what shows the 79px one reaches this filter, and
/// the 79px one coming back empty is what shows the filter refuses it.
///
/// Both orientations, because `longer` is the one accessor here that has to
/// read the right side: a filter reading `width()` fails the stacked pair and
/// one reading `height()` fails the turned pair.
#[test]
fn a_block_shorter_than_two_stroke_scales_is_not_a_region() {
    for turned in [false, true] {
        let short = six_bars(4, turned);
        assert_eq!(
            stroke_scale(&glyph_band(&components(&short, otsu(&short)), 600, BAND)),
            Some(40),
            "fixture must bite: all six bars must pass the band and set the scale, \
             turned={turned}"
        );
        let found = detect_page(&short);
        assert!(
            found.is_empty(),
            "79px against a span of 80, turned={turned}: got {found:?}"
        );

        let long = detect_page(&six_bars(5, turned));
        assert_eq!(
            long.len(),
            1,
            "80px against a span of 80, turned={turned}: got {long:?}"
        );
        let (axis, along, across) = if turned {
            (Axis::Horizontal, long[0].w * 400.0, long[0].h * 600.0)
        } else {
            (Axis::Vertical, long[0].h * 600.0, long[0].w * 400.0)
        };
        assert_eq!(long[0].axis, axis, "turned={turned}: got {long:?}");
        assert!(
            (along - 80.0).abs() < 0.5 && (across - 40.0).abs() < 0.5,
            "the block is the bars' own 80x40, turned={turned}: got {long:?}"
        );
    }
}

/// Five bars 100px wide and 39px tall on a 400x1000 page, stacked on a
/// `pitch` of 140 or 141: one vertical block 100px wide and `4 * pitch + 39`
/// tall — 599 or 603 — on a page of 400,000px².
fn five_wide_bars(pitch: u32) -> GrayImage {
    let marks: Vec<(u32, u32, u32, u32)> =
        (0..5).map(|k| (150, 200 + pitch * k, 100, 39)).collect();
    page_of(400, 1000, &marks)
}

/// **`MAX_BLOCK_AREA` rejects a block on its own, and four rows fewer gets
/// through** — O33's other half. The 603px block is 0.15075 of the page and
/// the 599px one 0.14975, either side of 0.15, and the constant sits between
/// them rather than on either.
///
/// Nothing else is near its limit: five strokes against `MIN_STROKES`' four, a
/// block about six stroke-scales long against `MIN_BLOCK_STROKE_SPAN`'s two, 1,000‱
/// inside `BAND`, and ink at 0.32 and 0.33 against `MIN_INK`'s 0.05. The pairing is
/// what makes it bite, as it is for `a_block_of_hollow_strokes_is_mostly_paper`:
/// the two pages differ by one pixel of pitch, so the only filter that can tell
/// them apart is the one that reads area.
#[test]
fn a_block_over_the_area_ceiling_is_not_a_region() {
    let over = five_wide_bars(141);
    assert_eq!(
        stroke_scale(&glyph_band(&components(&over, otsu(&over)), 1000, BAND)),
        Some(100),
        "fixture must bite: all five bars must pass the band and set the scale"
    );
    let found = detect_page(&over);
    assert!(
        found.is_empty(),
        "100x603 is 0.15075 of the page, against 0.15: got {found:?}"
    );

    let under = detect_page(&five_wide_bars(140));
    assert_eq!(
        under.len(),
        1,
        "100x599 is 0.14975 of the page, against 0.15: got {under:?}"
    );
    assert!(
        (under[0].w * 400.0 - 100.0).abs() < 0.5 && (under[0].h * 1000.0 - 599.0).abs() < 0.5,
        "the block is the bars' own 100x599: got {under:?}"
    );
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
    let marks: Vec<(u32, u32, u32, u32)> = (0..8).map(|k| (200u32, 200 + 80 * k, 40, 40)).collect();
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
    let column: Vec<(u32, u32, u32, u32)> = (0..6).map(|i| (80u32, 100 + i * 60, 40, 40)).collect();
    let row: Vec<(u32, u32, u32, u32)> = (0..6).map(|i| (500u32 + i * 60, 700, 40, 40)).collect();
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
