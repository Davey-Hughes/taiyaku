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
mod narrow;
mod regions;
mod route;
mod sidecar;
mod slot;
mod solve;
mod truth;

pub use credits::{UnpairedRun, unpaired_runs};
pub use narrow::{FromFull, Narrowed};
pub use regions::{
    Axis, Region, RegionDoc, RegionEdition, RegionMember, VERSION as REGIONS_VERSION,
};
pub use route::{Route, Via, invert, resolve, route};
pub use sidecar::{
    Alignment, Anchor, ChapterAt, Corrected, Edition, Member, Merged, NotCorrected, Quality,
    RunEntry, Sidecar, SlotSpan, SolverInfo, Source, VERSION, Work, entries_from_runs,
    refuse_newer, rounded_avg_run,
};
pub use slot::{Direction, Half, Slot, expand, is_spread, spread_flags};
pub use solve::{
    Calibration, PageConfidence, PageHash, Verdict, average_run_length, calibrate, confidence,
    paired_slots, similarity, solve, verdict,
};
pub use truth::{Kind, TruthDoc, TruthEdition, TruthRegion, VERSION as TRUTH_VERSION};

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
    /// # Panics
    ///
    /// Panics if `start > end`, but only in a build with `debug_assertions`
    /// enabled — the check is a `debug_assert!` and does not fire in
    /// release, which is the profile `check-all.sh` runs the corpus checks
    /// under. Callers that must turn a backwards span into an `Err` rather
    /// than risk this — a hand-edited sidecar, say — validate before calling
    /// `new`; see `Sidecar::runs_of`.
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "page range must not be inverted");
        Self { start, end }
    }

    #[must_use]
    pub fn len(&self) -> u32 {
        self.end - self.start + 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        false // an inclusive range always covers at least one page
    }

    #[must_use]
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
    ///
    /// # Panics
    ///
    /// Panics if `a` and `b` do not span the same number of pages, but only
    /// in a build with `debug_assertions` enabled — the check is a
    /// `debug_assert_eq!` and does not fire in release, which is the profile
    /// `check-all.sh` runs the corpus checks under. `Sidecar::runs_of`
    /// checks the lengths itself and returns an `Err` naming both spans
    /// before calling this, precisely so a hand-edited mismatch cannot reach
    /// here.
    #[must_use]
    pub fn paired(a: PageRange, b: PageRange) -> Self {
        debug_assert_eq!(a.len(), b.len(), "paired run sides must be equal length");
        Self { a, b: Some(b) }
    }

    /// A run present in edition A only.
    #[must_use]
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
#[must_use]
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
#[must_use]
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
#[must_use]
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
mod tests;
