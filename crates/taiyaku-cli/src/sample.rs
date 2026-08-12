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
/// `corpus/incoming/` and are laid out differently from the settled tier —
/// confirm each against the tree before the first run rather than assuming.
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
/// A strict subset of the 90 pages `notes.md`'s figures are computed over.
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
mod tests {
    use super::*;

    #[test]
    fn the_dev_set_is_nine_editions_and_five_pages_each() {
        assert_eq!(DEV_EDITIONS.len(), 9);
        assert_eq!(dev_pages(200).len(), 5, "45 pages over nine editions");
    }

    #[test]
    fn the_picks_are_a_strict_subset_of_the_ten_the_survey_uses() {
        let ten: Vec<usize> = (0..10).map(|i| i * 195 / 10 + 195 / 20).collect();
        for page in dev_pages(195) {
            assert!(
                ten.contains(&page),
                "page {page} is not one of the survey's ten"
            );
        }
    }

    #[test]
    fn the_picks_are_spread_rather_than_taken_from_the_front() {
        let picked = dev_pages(200);
        assert!(picked[0] < 40, "starts near the front");
        assert!(*picked.last().unwrap() > 150, "and reaches the back");
    }

    #[test]
    fn an_edition_shorter_than_the_sample_yields_only_pages_it_has() {
        assert!(dev_pages(3).iter().all(|page| *page < 3));
        assert!(dev_pages(0).is_empty());
    }
}
