//! Portable core for taiyaku.
//!
//! Everything here must compile to `wasm32-unknown-unknown` so the browser
//! reader and a future native shell can share it rather than reimplementing the
//! alignment format. CI enforces that; see `.forgejo/workflows/ci.yml`.
//!
//! Fingerprinting stays out of here because it needs image decoding, which is
//! native-only. Solving does not: the dynamic programming runs over page
//! hashes, so it is portable and lives here alongside the lookup.

mod sidecar;
mod slot;
mod solve;

pub use sidecar::{
    Alignment, Edition, Member, RunEntry, Sidecar, SlotSpan, SolverInfo, Source, VERSION, Work,
    entries_from_runs, resolve,
};
pub use slot::{Direction, Half, Slot, expand, is_spread, spread_flags};
pub use solve::{
    Calibration, PageConfidence, PageHash, Verdict, average_run_length, calibrate, confidence,
    similarity, solve, verdict,
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
}
