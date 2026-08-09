//! Portable core for taiyaku.
//!
//! Everything here must compile to `wasm32-unknown-unknown` so the browser
//! reader and a future native shell can share it rather than reimplementing the
//! alignment format. CI enforces that; see `.forgejo/workflows/ci.yml`.
//!
//! Fingerprinting stays out of here because it needs image decoding, which is
//! native-only. Solving does not: the dynamic programming runs over page
//! hashes, so it is portable and lives here alongside the lookup.

mod credits;
mod regions;
mod route;
mod sidecar;
mod slot;
mod solve;

pub use credits::{UnpairedRun, unpaired_runs};
pub use regions::{
    Axis, Region, RegionDoc, RegionEdition, RegionMember, VERSION as REGIONS_VERSION,
};
pub use route::{Route, Via, invert, resolve, route};
pub use sidecar::{
    Alignment, Anchor, ChapterAt, Corrected, Edition, Member, Merged, NotCorrected, Quality,
    RunEntry, Sidecar, SlotSpan, SolverInfo, Source, VERSION, Work, entries_from_runs,
};
pub use slot::{Direction, Half, Slot, expand, is_spread, spread_flags};
pub use solve::{
    Calibration, PageConfidence, PageHash, Verdict, average_run_length, calibrate, confidence,
    paired_slots, similarity, solve, verdict,
};

/// An inclusive range of page indices within a single edition.
///
/// Indices are relative to the edition's flattened page sequence, not to any
/// one archive. An edition assembled from five chapter files numbers its pages
/// 0..n continuously across all five.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    pub start: u32,
    pub end: u32,
}

impl PageRange {
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "page range must not be inverted");
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start + 1
    }

    pub fn is_empty(&self) -> bool {
        false // an inclusive range always covers at least one page
    }

    pub fn contains(&self, page: u32) -> bool {
        page >= self.start && page <= self.end
    }
}

/// A contiguous stretch of correspondence between two editions.
///
/// `b` is `None` where pages exist in edition A with no counterpart in B — a
/// colour insert, a scanlator credit page, a bonus chapter. Those gaps are the
/// whole reason this format is run-based rather than a flat page table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub a: PageRange,
    pub b: Option<PageRange>,
}

impl Run {
    /// A paired run, where both editions carry the same number of pages.
    pub fn paired(a: PageRange, b: PageRange) -> Self {
        debug_assert_eq!(a.len(), b.len(), "paired run sides must be equal length");
        Self { a, b: Some(b) }
    }

    /// A run present in edition A only.
    pub fn gap(a: PageRange) -> Self {
        Self { a, b: None }
    }
}

/// Resolve a page in edition A to its counterpart in edition B.
///
/// Returns `None` when the page has no counterpart, either because it falls in
/// a gap or because it lies outside the aligned range entirely. Callers should
/// treat those two cases the same way: there is nothing to swap to.
///
/// Runs are expected to be ordered and non-overlapping on the A side. A linear
/// scan is deliberate — a typical volume resolves to on the order of ten runs,
/// so the indirection of a binary search would cost more than it saves.
pub fn counterpart(runs: &[Run], page: u32) -> Option<u32> {
    let run = runs.iter().find(|r| r.a.contains(page))?;
    let b = run.b?;
    Some(b.start + (page - run.a.start))
}

/// Chain two alignments: A to a hub, then that hub to B.
///
/// This is what makes hub-and-spoke work. With one edition as the reference,
/// N editions need N-1 alignments rather than N(N-1)/2, and any A-to-B lookup
/// resolves by composing through the hub, so transitivity holds by construction
/// and two editions cannot disagree about where a page went.
pub fn compose(a_to_hub: &[Run], hub_to_b: &[Run]) -> Vec<Run> {
    let chained: Vec<(u32, Option<u32>)> = a_to_hub
        .iter()
        .flat_map(|run| run.a.start..=run.a.end)
        .map(|page| {
            (
                page,
                counterpart(a_to_hub, page).and_then(|via| counterpart(hub_to_b, via)),
            )
        })
        .collect();
    solve::compress(&chained)
}

/// Assert by hand that slot `a` of the source edition pairs with slot `b` of
/// the target — or, where `b` is `None`, with nothing.
///
/// The whole run list is rebuilt through the same `compress` that `compose`
/// and the solver itself end on, so a correction cannot produce a run shape
/// the solver could not have produced. That is the point: the correction
/// changes one slot's answer, and nothing else decides what the runs look like
/// afterwards. A correction that disagrees with its neighbours splits a run;
/// one that agrees with them merges two.
///
/// **Whatever used to hold `b` gives it up.** The alignment is strictly 1:1 in
/// slots — that is what expanding spreads before solving buys — so asserting
/// that `a` pairs with `b` is also asserting that nothing else does. Leaving
/// the old owner in place produces a run list where two source slots claim one
/// target, which `invert` turns into two runs starting at the same slot and
/// `counterpart` then answers from whichever comes first, silently. The
/// displaced slot becomes a gap rather than being guessed at: the document
/// knows it no longer pairs with `b` and knows nothing about what it does pair
/// with.
///
/// Slots the run list never mentions stay unmentioned. A run list need not
/// cover its edition, and inventing gap runs for the head of one would put
/// slots into the document that the alignment never claimed anything about.
pub fn correct_runs(runs: &[Run], a: u32, b: Option<u32>) -> Vec<Run> {
    let end = runs.iter().map(|r| r.a.end).max().unwrap_or(a).max(a);
    let mentioned = |slot: u32| runs.iter().any(|r| r.a.contains(slot));
    let displaced = b.and_then(|target| {
        (0..=end)
            .find(|&slot| slot != a && mentioned(slot) && counterpart(runs, slot) == Some(target))
    });
    let pairs: Vec<(u32, Option<u32>)> = (0..=end)
        .filter(|&slot| slot == a || mentioned(slot))
        .map(|slot| match (slot == a, Some(slot) == displaced) {
            (true, _) => (slot, b),
            (_, true) => (slot, None),
            _ => (slot, counterpart(runs, slot)),
        })
        .collect();
    solve::compress(&pairs)
}

#[cfg(test)]
mod tests {
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
    /// Measured on the real fixture: pairing `raw-mono`'s gap at slot 6 with
    /// `en-mono` slot 5 takes the stored five runs down to three.
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
}
