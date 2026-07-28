//! Portable core for taiyaku.
//!
//! Everything here must compile to `wasm32-unknown-unknown` so the browser
//! reader and a future native shell can share it rather than reimplementing the
//! alignment format. CI enforces that; see `.forgejo/workflows/ci.yml`.
//!
//! Only the alignment lookup exists so far. Fingerprinting and the alignment
//! solver land later.

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

#[cfg(test)]
mod tests {
    use super::*;

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
