//! Which pages are the dev set, and which are sealed.
//!
//! **Named rather than globbed.** A glob that stops matching is the
//! silent-nothing failure `region_experiment.rs` refuses to have, and the
//! assertion that every edition contributed only means something if the list
//! is fixed. The same reasoning that module's header gives for its nine.

/// The nine editions `region_experiment.rs` already samples.
///
/// Labelling these means every figure already in `docs/notes.md` stays
/// readable against the new labels, with no second sampling rule in between.
pub const DEV_EDITIONS: &[(&str, &str)] = &[
    ("gotoubun-v01", "raw-mono"),
    ("gotoubun-v01", "en-mono"),
    ("gotoubun-v01", "en-fan"),
    ("kaguya-v01", "raw-mono"),
    ("kaguya-v01", "en-mono"),
    ("oshinoko-v01", "raw-mono"),
    ("oshinoko-v01", "en-mono"),
    ("kobayashi-v01", "raw-mono"),
    ("kobayashi-v01", "en-mono"),
];

/// Four regimes the survey has never touched, all already on disk.
///
/// **Sealed.** Labelled up front, opened once at the end, reported whatever it
/// says, and never used to choose between candidates. Paths are under
/// `corpus/incoming/` and are laid out differently from the settled tier.
pub const HELD_OUT_EDITIONS: &[(&str, &str)] = &[
    ("incoming/berserk-cleaned", "Berserk.T01"),
    ("incoming/onepiece", "op-raw-v01"),
    ("incoming/nausicaa", "raw-v01"),
    ("incoming/bocchi", "raw-v01-src-hamaji"),
    ("incoming/bocchi", "raw-v01-src-krc"),
    ("incoming/bocchi", "raw-v01-src-mangazip"),
];

/// The dev set's page indices: every other one of `region_experiment`'s ten.
///
/// A strict subset of the pages `notes.md`'s figures are computed over.
/// **Thinned rather than recomputed**: calling that module's `sample_indices`
/// with a count of 5 lands on *different* pages, because the formula divides
/// by the count, and the subset property is the whole point.
///
/// The spread is preserved deliberately. The front of a volume is colour
/// inserts, a title page and blank interstitials — the least representative
/// pages there are — which is why `sample_indices` spreads at all, and why
/// taking the first five of ten would undo it.
#[must_use]
pub fn dev_pages(available: usize) -> Vec<usize> {
    if available == 0 {
        return Vec::new();
    }
    (0..10)
        .step_by(2)
        .map(|i| i * available / 10 + available / 20)
        .filter(|page| *page < available)
        .collect()
}

#[cfg(test)]
mod tests;
