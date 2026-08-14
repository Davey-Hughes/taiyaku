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

/// An index into `landscape`, the page-flag sequence `expand` walks below.
///
/// # Panics
///
/// Panics if `i` exceeds `u32::MAX`. It cannot in this crate: `landscape`
/// carries one entry per page of the edition being expanded into slots, and
/// a page count is bounded by what a book can physically contain, far below
/// `u32::MAX`.
fn page_index(i: usize) -> u32 {
    u32::try_from(i).expect("page index exceeds u32::MAX")
}

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
#[must_use]
pub fn is_spread(page_aspect: f64, ordinary_aspect: f64) -> bool {
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
    // w and h are a page's pixel dimensions; the corpus's largest measured page
    // is 10200x14039 (crates/taiyaku-read/src/lib.rs's MAX_DECODE_ALLOC, the
    // Color Walk art books), far below 2^24 (16,777,216) where f32 begins
    // losing integer precision, and further still below f64's 2^53. f64::from
    // (u32) is lossless outright (u32's 32 bits fit f64's 52-bit mantissa),
    // where there is no equivalent `From<u32> for f32` (f32's mantissa is
    // only 24 bits) — so widening `is_spread`'s parameters and every value
    // computed here to f64 closes the cast without a suppression.
    let aspect = |(w, h): (u32, u32)| f64::from(w) / f64::from(h);
    let mut portrait: Vec<f64> = dimensions
        .iter()
        .copied()
        .filter(|(w, h)| w <= h)
        .map(aspect)
        .collect();
    portrait.sort_by(f64::total_cmp);

    let ordinary = if portrait.is_empty() {
        // No portrait page anywhere: the typical page of this edition is itself
        // a joined spread, so an ordinary single page is half of one.
        let mut all: Vec<f64> = dimensions.iter().copied().map(aspect).collect();
        all.sort_by(f64::total_cmp);
        if all.is_empty() {
            return Vec::new();
        }
        all[all.len() / 2] / 2.0
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
#[must_use]
pub fn expand(landscape: &[bool], direction: Direction) -> Vec<Slot> {
    let (first, second) = match direction {
        Direction::RightToLeft => (Half::Right, Half::Left),
        Direction::LeftToRight => (Half::Left, Half::Right),
    };
    landscape
        .iter()
        .enumerate()
        .flat_map(|(page, wide)| {
            let page = page_index(page);
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
mod tests;
