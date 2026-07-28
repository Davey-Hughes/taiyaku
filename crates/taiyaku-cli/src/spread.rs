//! Treating a joined double-page spread as two alignable halves.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Manga: the right-hand half of a spread is the earlier page.
    RightToLeft,
    /// Western comics: the left-hand half is the earlier page.
    LeftToRight,
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
