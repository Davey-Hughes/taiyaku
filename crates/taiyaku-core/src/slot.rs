//! Slots: the unit an alignment actually addresses.
//!
//! A slot is a whole page, or one half of a joined double-page spread. This
//! lives in the core rather than the CLI because it is a property of the
//! format: a sidecar records correspondences between slots, so any reader of
//! the format needs the same notion.
//!
//! An edition that joins spreads carries one wide page where the other edition
//! carries two. Measured on the corpus, cutting that wide page down the middle
//! and hashing each half finds exactly the two pages the other edition split it
//! into, at similarities indistinguishable from ordinary matches.
//!
//! The trick is to do this before solving. A landscape page becomes two entries
//! in the hashed sequence, the alignment stays strictly one-to-one, and the
//! halves are folded back afterwards. The solver never learns spreads exist.

/// Which side of a joined spread a page corresponds to.
///
/// Sides are in image coordinates, not reading order. In this corpus the right
/// half is always the earlier page because manga reads right to left, but that
/// is a property of the work rather than of the format, so it is measured and
/// recorded rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Half {
    Left,
    Right,
}

/// One alignable unit: a whole page, or one half of a joined spread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub page: u32,
    pub half: Option<Half>,
}

/// Which way the work reads, which decides which half of a spread comes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    /// Manga: the right-hand half of a spread is the earlier page.
    #[serde(rename = "rtl")]
    RightToLeft,
    /// Western comics: the left-hand half is the earlier page.
    #[serde(rename = "ltr")]
    LeftToRight,
}

/// Is a landscape page a joined spread, or something else that happens to be
/// wide?
///
/// Both aspect ratios are width over height. A joined spread is two portrait
/// pages side by side, so its aspect lands near twice the aspect of an ordinary
/// page in the same edition — measured across the corpus, between 1.95 and 2.13
/// times, never exactly twice outside the cleanest digital releases.
///
/// The bound matters because not every wide page is a spread. One Piece volume
/// 60 opens with its wraparound dust jacket scanned as one image: front cover,
/// spine, back cover and both inside flaps, at **3.43 times** a page width.
/// Splitting that down the middle produces two halves corresponding to nothing.
///
/// The upper bound is 2.8 rather than something tighter because spreads are
/// sometimes **padded**. Tongari volume 9 is spreads letterboxed into 1920×1080,
/// which reads as 2.53 times an ordinary 1125×1600 page — the two pages are
/// correct and the white bars either side are not. Everything genuinely
/// measured falls either at 1.95 to 2.53, or at 3.43 for the jacket, so the
/// line sits in the gap between them.
pub fn is_spread(page_aspect: f32, ordinary_aspect: f32) -> bool {
    if ordinary_aspect <= 0.0 {
        return false;
    }
    let doubling = page_aspect / ordinary_aspect;
    (1.7..=2.8).contains(&doubling)
}

/// Decide which pages of an edition are joined spreads, from every page's
/// dimensions.
///
/// The baseline is what an ordinary page of *this* edition looks like, which is
/// why it cannot be judged page by page. Taking the median of the portrait pages
/// is the obvious choice and fails on an edition that has none: some scans are
/// published two facing pages per image throughout, so every page is landscape
/// and there is no portrait baseline to measure against.
pub fn spread_flags(dimensions: &[(u32, u32)]) -> Vec<bool> {
    let aspect = |(w, h): (u32, u32)| w as f32 / h as f32;
    let mut portrait: Vec<f32> = dimensions
        .iter()
        .copied()
        .filter(|(w, h)| w <= h)
        .map(aspect)
        .collect();
    portrait.sort_by(f32::total_cmp);

    let ordinary = if portrait.is_empty() {
        // No portrait page anywhere: the typical page of this edition is itself
        // a joined spread, so an ordinary single page is half of one.
        let mut all: Vec<f32> = dimensions.iter().copied().map(aspect).collect();
        all.sort_by(f32::total_cmp);
        match all.is_empty() {
            true => return Vec::new(),
            false => all[all.len() / 2] / 2.0,
        }
    } else {
        portrait[portrait.len() / 2]
    };

    dimensions
        .iter()
        .copied()
        .map(|d| is_spread(aspect(d), ordinary))
        .collect()
}

/// Expand a sequence of pages into the slot sequence the solver aligns against.
///
/// `landscape[i]` says whether page `i` is a joined spread. Halves are emitted
/// in reading order, because the alignment is monotonic and cannot run backwards
/// across a pair.
pub fn expand(landscape: &[bool], direction: Direction) -> Vec<Slot> {
    let (first, second) = match direction {
        Direction::RightToLeft => (Half::Right, Half::Left),
        Direction::LeftToRight => (Half::Left, Half::Right),
    };
    landscape
        .iter()
        .enumerate()
        .flat_map(|(page, wide)| {
            let page = page as u32;
            if *wide {
                vec![
                    Slot {
                        page,
                        half: Some(first),
                    },
                    Slot {
                        page,
                        half: Some(second),
                    },
                ]
            } else {
                vec![Slot { page, half: None }]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real dimensions from four editions. The baseline for "ordinary page" is
    /// per-edition, and the case that breaks a naive version is an edition
    /// published two facing pages per image throughout — every page landscape,
    /// no portrait page anywhere to measure against. One Piece volume 1 exists
    /// in the corpus in exactly that form, 107 images of 1524x1200 for a
    /// 210-page volume, and treating none of them as spreads makes the whole
    /// edition unalignable.
    #[test]
    fn an_edition_that_is_entirely_spreads_is_still_all_spreads() {
        // Mixed edition: Go-Toubun, one spread among portrait pages.
        let mixed = vec![(2147, 3056), (2147, 3056), (4294, 3056), (2147, 3056)];
        assert_eq!(spread_flags(&mixed), vec![false, false, true, false]);

        // Two facing pages per image, throughout. Every one is a spread.
        let paired = vec![(1524, 1200), (1524, 1200), (1518, 1200), (1524, 1200)];
        assert_eq!(spread_flags(&paired), vec![true; 4]);

        // A wraparound dust jacket among portrait pages is not a spread.
        let jacket = vec![(3450, 1600), (1005, 1600), (1005, 1600), (2140, 1600)];
        assert_eq!(spread_flags(&jacket), vec![false, false, false, true]);

        // Tongari volume 9 is spreads letterboxed into 1920x1080. The padding
        // inflates the aspect to 2.53 times an ordinary 1125x1600 page, so a
        // bound tight enough to reject a 3.43x dust jacket must still be loose
        // enough to accept this. It matters at series scope, where a padded
        // volume sits among portrait ones and cannot rely on the
        // all-landscape fallback.
        let padded_in_series = vec![(1125, 1600), (1125, 1600), (1920, 1080), (1125, 1600)];
        assert_eq!(
            spread_flags(&padded_in_series),
            vec![false, false, true, false]
        );

        // All portrait, nothing to split.
        assert_eq!(
            spread_flags(&[(760, 1200), (760, 1200)]),
            vec![false, false]
        );
        assert!(spread_flags(&[]).is_empty());
    }

    /// The baseline is the median aspect ratio, and the median index is
    /// `len / 2`. Replacing it with `len % 2` picks index 0 or 1 — the two
    /// narrowest pages — which on a real edition means the baseline is set by
    /// whichever page was cropped tightest. The fixture needs six portrait
    /// pages, because with three `/` and `%` agree.
    #[test]
    fn the_ordinary_page_is_the_median_and_not_the_narrowest() {
        // Six portrait pages, one much narrower than the rest, plus one true
        // spread at roughly twice the ordinary aspect.
        let mut dims: Vec<(u32, u32)> = vec![
            (700, 1400), // markedly narrow: 0.50
            (1000, 1400),
            (1010, 1400),
            (1020, 1400),
            (1030, 1400),
            (1040, 1400),
        ];
        dims.push((2040, 1400)); // a joined spread of the ordinary page

        let flags = spread_flags(&dims);

        assert_eq!(
            flags,
            vec![false, false, false, false, false, false, true],
            "the narrow page is not a spread and must not set the baseline"
        );
    }

    /// The other branch: an edition published entirely as joined spreads has
    /// no portrait page at all, so the baseline comes from halving the median
    /// of every page. Same median-index hazard, different array.
    ///
    /// Both fixtures here are constructed rather than measured, because the
    /// measured all-landscape edition — One Piece volume 1, 107 images within
    /// six pixels of each other — cannot tell a median from a minimum. The
    /// spread of aspects is the point of the fixture and not incidental to it:
    /// with every page the same width, every index of the sorted array holds
    /// the same number.
    #[test]
    fn an_all_landscape_edition_takes_its_baseline_from_the_halved_median() {
        // Six landscape pages, all of them spreads of a roughly 0.75-aspect
        // page. The widest is 1.45 times the narrowest, which is what makes
        // the median distinguishable: taking the narrowest as the baseline
        // puts the widest at 2.90 times it, past the upper bound.
        let dims: Vec<(u32, u32)> = vec![
            (1359, 1000),
            (1400, 1000),
            (1450, 1000),
            (1500, 1000),
            (1700, 1000),
            (1970, 1000),
        ];

        let flags = spread_flags(&dims);

        assert_eq!(flags, vec![true; 6]);

        // The safety net, and it is here because the fixture above clears the
        // bar by only 3.5% — 2.899 against an upper bound of 2.8. That margin
        // is coupled to `is_spread`'s band: widen the band to 2.9 for some
        // future wide-page case and the fixture above silently stops telling
        // `/` from `%` while still passing. This one clears by 16% against the
        // *lower* bound instead, so it fails differently and survives that.
        //
        // Two image sizes in one edition, both real — One Piece volume 1's
        // 1524x1200 and Tongari volume 9's letterboxed 1920x1080. The median
        // is 1.778, so an ordinary page of this edition is 0.889 wide and a
        // 1.27 image is only 1.43 of them: wide, but not two pages. Under the
        // narrowest-page baseline all six come out spreads instead.
        let mixed: Vec<(u32, u32)> = vec![
            (1524, 1200),
            (1524, 1200),
            (1524, 1200),
            (1920, 1080),
            (1920, 1080),
            (1920, 1080),
        ];

        assert_eq!(
            spread_flags(&mixed),
            vec![false, false, false, true, true, true],
            "a page well under twice the median is not a joined spread"
        );
    }

    /// Real measurements, all of them. The wide pages that are spreads and the
    /// wide page that is not have to come out on opposite sides of this.
    #[test]
    fn a_wraparound_cover_is_not_a_spread() {
        let aspect = |w: f32, h: f32| w / h;

        // Genuine joined spreads, from four series. None is exactly double.
        let gotoubun = aspect(2147.0, 3056.0);
        assert!(
            is_spread(aspect(4294.0, 3056.0), gotoubun),
            "Go-Toubun 2.00x"
        );
        let berserk = aspect(1810.0, 2560.0);
        assert!(is_spread(aspect(3620.0, 2560.0), berserk), "Berserk 2.00x");
        let onepiece_en = aspect(1500.0, 2250.0);
        assert!(
            is_spread(aspect(2922.0, 2250.0), onepiece_en),
            "One Piece EN 1.95x"
        );
        let onepiece_raw = aspect(1005.0, 1600.0);
        assert!(
            is_spread(aspect(2140.0, 1600.0), onepiece_raw),
            "One Piece raw 2.13x"
        );

        // One Piece volume 60 opens with its wraparound dust jacket: front,
        // spine, back and both flaps in one image, at 3.43 times a page.
        // Splitting it in half yields two halves matching nothing.
        assert!(
            !is_spread(aspect(3450.0, 1600.0), onepiece_raw),
            "a 3.43x dust jacket is not a spread"
        );
        // A portrait page is never a spread however the numbers arrive.
        assert!(!is_spread(onepiece_raw, onepiece_raw), "portrait");
    }

    /// The index arithmetic is the whole risk here. Once a wide page occupies
    /// two entries, every later slot is offset, and an off-by-one silently
    /// sends a reader to its neighbour rather than failing loudly.
    #[test]
    fn a_landscape_page_takes_two_slots_and_shifts_everything_after_it() {
        // pages 0 and 3 are portrait, 1 is a joined spread, 2 is portrait
        let slots = expand(&[false, true, false, false], Direction::LeftToRight);

        assert_eq!(
            slots,
            vec![
                Slot {
                    page: 0,
                    half: None
                },
                Slot {
                    page: 1,
                    half: Some(Half::Left)
                },
                Slot {
                    page: 1,
                    half: Some(Half::Right)
                },
                Slot {
                    page: 2,
                    half: None
                },
                Slot {
                    page: 3,
                    half: None
                },
            ]
        );
        // Five slots for four pages, and page 3 sits at slot 4.
        assert_eq!(slots.len(), 5);
        assert_eq!(
            slots[4],
            Slot {
                page: 3,
                half: None
            }
        );
    }

    /// Halves must be emitted in reading order, not image order.
    ///
    /// This is not cosmetic. In a right-to-left work the right half of a spread
    /// is the *earlier* page, so listing halves left-first would require the
    /// alignment to run backwards across the pair. The solver enforces
    /// monotonicity and correctly refuses, which costs one of the two halves of
    /// every spread — measured, exactly seven of fourteen recoverable pages on
    /// Go-Toubun volume 1. The alignment cannot discover the order for itself;
    /// it has to be told.
    #[test]
    fn right_to_left_reading_puts_the_right_half_first() {
        let slots = expand(&[true], Direction::RightToLeft);

        assert_eq!(
            slots,
            vec![
                Slot {
                    page: 0,
                    half: Some(Half::Right)
                },
                Slot {
                    page: 0,
                    half: Some(Half::Left)
                },
            ]
        );
    }
}
