use super::*;

fn cand(x0: f64, y0: f64, x1: f64, y1: f64, score: f32) -> Cand {
    Cand {
        x0,
        y0,
        x1,
        y1,
        score,
    }
}

#[test]
fn suppression_drops_the_weaker_of_two_overlapping_boxes_of_one_class() {
    let kept = suppress(vec![
        cand(0.0, 0.0, 100.0, 100.0, 0.9),
        cand(5.0, 5.0, 105.0, 105.0, 0.8),
    ]);
    assert_eq!(kept.len(), 1, "near-identical boxes are one detection");
    assert!(
        (kept[0].score - 0.9).abs() < 1e-6,
        "the stronger one survives"
    );
}

#[test]
fn suppression_collapses_a_twin_that_only_disagrees_about_its_class() {
    // The same geometry, two classes. **This used to assert the opposite**,
    // and the change is deliberate rather than a relaxation — see
    // `suppress`'s own doc comment for why the agnostic setting is the
    // right one here. Measured before the change: 40 of 161 English regions
    // carried a near-identical twin at IoU 0.94–0.99, against 0 of 147
    // Japanese ones, and a twin is not a second text block.
    let kept = suppress(vec![
        cand(0.0, 0.0, 100.0, 100.0, 0.9),
        cand(0.0, 0.0, 100.0, 100.0, 0.8),
    ]);
    assert_eq!(kept.len(), 1, "one block, whatever the class channel says");
    assert!(
        (kept[0].score - 0.9).abs() < 1e-6,
        "the stronger one survives"
    );
}

#[test]
fn suppression_keeps_two_balloons_that_merely_sit_near_each_other() {
    let kept = suppress(vec![
        cand(0.0, 0.0, 100.0, 100.0, 0.9),
        cand(200.0, 200.0, 300.0, 300.0, 0.8),
    ]);
    assert_eq!(kept.len(), 2, "disjoint boxes are two detections");
}

#[test]
fn a_candidate_filling_the_live_region_becomes_the_whole_page() {
    // dw = dh = 0, so the live region is the whole square, and a box over
    // all of it is the whole page in fractions.
    let regions = to_regions(&[cand(0.0, 0.0, 1024.0, 1024.0, 0.9)], 0, 0);
    assert_eq!(regions.len(), 1);
    assert!((regions[0].x - 0.0).abs() < 1e-9);
    assert!((regions[0].w - 1.0).abs() < 1e-9);
    assert!((regions[0].h - 1.0).abs() < 1e-9);
}

#[test]
fn the_padding_is_excluded_from_the_page_fraction() {
    // Half the square is padding on the bottom. A box over the live half is
    // the whole page height, not half of it — this is the arithmetic that
    // maps square-space back onto the scan.
    let regions = to_regions(&[cand(0.0, 0.0, 1024.0, 512.0, 0.9)], 0, 512);
    assert!(
        (regions[0].h - 1.0).abs() < 1e-9,
        "the live region is the page, and the padding is not"
    );
}

#[test]
fn a_tall_box_is_vertical_and_a_wide_one_is_horizontal() {
    let tall = to_regions(&[cand(0.0, 0.0, 100.0, 900.0, 0.9)], 0, 0);
    let wide = to_regions(&[cand(0.0, 0.0, 900.0, 100.0, 0.9)], 0, 0);
    assert_eq!(tall[0].axis, Axis::Vertical);
    assert_eq!(wide[0].axis, Axis::Horizontal);
}

#[test]
fn a_candidate_below_the_confidence_floor_is_not_proposed() {
    // One grid cell, objectness under the floor. BLK_STRIDE floats:
    // cx, cy, w, h, objectness, class0, class1.
    let weak = [512.0, 512.0, 100.0, 100.0, 0.05, 0.9, 0.1];
    assert!(candidates(&weak).is_empty(), "objectness gates first");

    let strong = [512.0, 512.0, 100.0, 100.0, 0.95, 0.9, 0.1];
    assert_eq!(candidates(&strong).len(), 1, "a confident cell is proposed");
}

/// Pins `candidates`' corner arithmetic to known values, not just a
/// count. `w` and `h` differ (200 vs 50) so a `w`/`h` swap in the corner
/// computation changes the corners without changing how many candidates
/// come back or whether they clear either confidence gate — the failure
/// mode O33 exists to catch, where a shape bug ships because only counts
/// were ever asserted. `cx` and `cy` also differ (300 vs 700) so a
/// `cx`/`cy` mix-up is caught the same way.
///
/// Proved to bite: with the corner computation changed to swap `w` and
/// `h` (`x0 = cx - h / 2.0`, `y0 = cy - w / 2.0`, etc.), the `x0`/`x1`
/// assertions here fail (200/400 expected, 275/325 produced) while every
/// other test in this module still passes — the swap is invisible to
/// anything that only checks a length.
#[test]
fn candidates_corners_match_the_known_geometry_of_one_cell() {
    // cx, cy, w, h, objectness, class0, class1.
    let cell = [300.0, 700.0, 200.0, 50.0, 0.9, 0.9, 0.1];
    let out = candidates(&cell);
    assert_eq!(out.len(), 1);
    assert!((out[0].x0 - 200.0).abs() < 1e-9, "x0 = cx - w/2");
    assert!((out[0].x1 - 400.0).abs() < 1e-9, "x1 = cx + w/2");
    assert!((out[0].y0 - 675.0).abs() < 1e-9, "y0 = cy - h/2");
    assert!((out[0].y1 - 725.0).abs() < 1e-9, "y1 = cy + h/2");
}

/// Isolates the second confidence gate (`objectness * class_score`) from
/// the first (`objectness` alone). Every other fixture in this module
/// uses an objectness strong enough that its product with the class
/// score also clears `BLK_CONF`, so the second gate is never the reason
/// a candidate is rejected there — deleting it would leave the rest of
/// this module green.
///
/// Proved to bite: with the `score < BLK_CONF` check removed (the
/// objectness gate left in place), `candidates` no longer rejects this
/// cell and the `is_empty` assertion fails, while every other test in
/// this module still passes.
#[test]
fn a_candidate_clearing_objectness_alone_can_still_fail_the_combined_gate() {
    // objectness 0.41 clears BLK_CONF by itself, but the winning class
    // score is 0.5 (not 0.1), and 0.41 * 0.5 = 0.205 does not.
    let cell = [512.0, 512.0, 100.0, 100.0, 0.41, 0.5, 0.1];
    assert!(
        candidates(&cell).is_empty(),
        "objectness clears the floor but objectness * class_score does not"
    );
}
