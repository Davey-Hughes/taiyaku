use super::*;

/// The ordinary case: two constant offsets in sequence are one constant
/// offset, and must stay a single run rather than fragmenting.
#[test]
fn composing_two_offsets_adds_them() {
    let a_to_hub = vec![Run::paired(PageRange::new(0, 9), PageRange::new(2, 11))];
    let hub_to_b = vec![Run::paired(PageRange::new(0, 20), PageRange::new(3, 23))];

    let composed = compose(&a_to_hub, &hub_to_b);

    assert_eq!(
        composed,
        vec![Run::paired(PageRange::new(0, 9), PageRange::new(5, 14))]
    );
}

/// The cost hub-and-spoke is predicted to carry, pinned down so it cannot
/// change unnoticed. A page that exists in A but not in the hub has nowhere
/// to land, so the chain ends there — even in the case where B does hold a
/// counterpart and a direct alignment would have found it. Whether that
/// costs anything in practice is what the topology check measures.
#[test]
fn a_page_landing_on_a_hub_gap_loses_its_counterpart() {
    let a_to_hub = vec![
        Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
        Run::gap(PageRange::new(1, 1)),
        Run::paired(PageRange::new(2, 2), PageRange::new(1, 1)),
    ];
    let hub_to_b = vec![Run::paired(PageRange::new(0, 1), PageRange::new(0, 1))];

    let composed = compose(&a_to_hub, &hub_to_b);

    assert_eq!(
        composed,
        vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 2), PageRange::new(1, 1)),
        ]
    );
}

/// Shaped after the real corpus: Go-Toubun no Hanayome volume 1, raw
/// monochrome (195pp) against the English official release (187pp).
fn sample() -> Vec<Run> {
    vec![
        Run::paired(PageRange::new(0, 43), PageRange::new(0, 43)),
        Run::gap(PageRange::new(44, 44)),
        Run::paired(PageRange::new(45, 194), PageRange::new(44, 193)),
    ]
}

#[test]
fn maps_within_the_leading_diagonal() {
    assert_eq!(counterpart(&sample(), 0), Some(0));
    assert_eq!(counterpart(&sample(), 43), Some(43));
}

#[test]
fn reports_no_counterpart_for_a_gap() {
    assert_eq!(counterpart(&sample(), 44), None);
}

#[test]
fn applies_the_offset_after_a_gap() {
    assert_eq!(counterpart(&sample(), 45), Some(44));
    assert_eq!(counterpart(&sample(), 194), Some(193));
}

#[test]
fn reports_no_counterpart_beyond_the_aligned_range() {
    assert_eq!(counterpart(&sample(), 195), None);
}

#[test]
fn range_length_counts_both_endpoints() {
    assert_eq!(PageRange::new(0, 43).len(), 44);
    assert_eq!(PageRange::new(7, 7).len(), 1);
}

/// `PageRange` is inclusive, so `new(4, 4)` is one page, not zero. The
/// method exists because `len` does and clippy pairs them; it is a
/// constant, and a constant with no test is a constant that can be
/// flipped without anything noticing.
#[test]
fn an_inclusive_page_range_is_never_empty() {
    assert!(!PageRange::new(4, 4).is_empty());
    assert_eq!(PageRange::new(4, 4).len(), 1);
    assert!(!PageRange::new(0, 209).is_empty());
    assert_eq!(PageRange::new(0, 209).len(), 210);
}

/// The ordinary correction: one slot in the middle of a long run is
/// asserted to pair with a different slot, so the run splits around it.
#[test]
fn correcting_a_slot_inside_a_run_splits_it() {
    let runs = vec![Run::paired(PageRange::new(0, 9), PageRange::new(0, 9))];

    let fixed = correct_runs(&runs, 5, Some(7));

    assert_eq!(
        fixed,
        vec![
            Run::paired(PageRange::new(0, 4), PageRange::new(0, 4)),
            Run::paired(PageRange::new(5, 5), PageRange::new(7, 7)),
            // Slot 6 still pairs with 6 — it just cannot stay inside
            // either neighbour's run any more, because the offset either
            // side of it changed.
            Run::paired(PageRange::new(6, 6), PageRange::new(6, 6)),
            Run::gap(PageRange::new(7, 7)),
            Run::paired(PageRange::new(8, 9), PageRange::new(8, 9)),
        ],
        "slot 7 is the one that gave up its target; slot 6 keeps its own pairing"
    );
}

/// The invariant that makes the rest of the format work. Asserting that `a`
/// pairs with `b` is also asserting that nothing else does — leave the old
/// owner in place and two source slots claim one target, which `invert`
/// turns into two runs starting at the same slot and `counterpart` answers
/// from whichever comes first, silently.
#[test]
fn a_corrected_target_is_taken_from_whoever_held_it() {
    let runs = vec![Run::paired(PageRange::new(0, 9), PageRange::new(0, 9))];

    let fixed = correct_runs(&runs, 5, Some(7));

    let mut targets: Vec<u32> = fixed
        .iter()
        .filter_map(|r| r.b)
        .flat_map(|b| b.start..=b.end)
        .collect();
    let claimed = targets.len();
    targets.sort_unstable();
    targets.dedup();
    assert_eq!(
        claimed,
        targets.len(),
        "no target slot may be claimed twice"
    );
    assert_eq!(counterpart(&fixed, 7), None, "slot 7 gave up slot 7");
    assert_eq!(counterpart(&fixed, 5), Some(7));
}

/// A correction that agrees with its neighbour merges rather than
/// splitting, because `compress` is the only thing deciding run shape.
/// Pairing `raw-mono`'s gap at slot 6 with `en-mono` slot 5 takes the stored
/// five runs down to three.
#[test]
fn a_correction_that_agrees_with_its_neighbour_merges() {
    let runs = vec![
        Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
        Run::gap(PageRange::new(1, 1)),
        Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
        Run::gap(PageRange::new(6, 6)),
        Run::paired(PageRange::new(7, 194), PageRange::new(6, 193)),
    ];

    let fixed = correct_runs(&runs, 6, Some(5));

    assert_eq!(
        fixed,
        vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 194), PageRange::new(1, 193)),
        ]
    );
}

/// Unpairing is a correction too, and it displaces nobody.
#[test]
fn correcting_a_slot_to_nothing_leaves_a_gap() {
    let runs = vec![Run::paired(PageRange::new(0, 3), PageRange::new(0, 3))];

    let fixed = correct_runs(&runs, 2, None);

    assert_eq!(
        fixed,
        vec![
            Run::paired(PageRange::new(0, 1), PageRange::new(0, 1)),
            Run::gap(PageRange::new(2, 2)),
            Run::paired(PageRange::new(3, 3), PageRange::new(3, 3)),
        ]
    );
}

/// A run list need not cover its edition — the star fixture's `a -> c`
/// starts at slot 2 — and inventing gap runs for the head would put slots
/// into the document the alignment never claimed anything about.
#[test]
fn slots_the_runs_never_mention_stay_unmentioned() {
    let runs = vec![Run::paired(PageRange::new(2, 5), PageRange::new(0, 3))];

    let fixed = correct_runs(&runs, 3, Some(9));

    assert_eq!(fixed[0].a, PageRange::new(2, 2), "nothing before slot 2");
    assert_eq!(counterpart(&fixed, 0), None);
    assert_eq!(counterpart(&fixed, 1), None);
}
