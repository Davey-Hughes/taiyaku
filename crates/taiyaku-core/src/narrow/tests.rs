use super::*;
use crate::{Member, Slot};

const READINGS: [Direction; 2] = [Direction::RightToLeft, Direction::LeftToRight];

/// The account sees A, C and D but not B.
const HIDE_B: [bool; 4] = [true, false, true, true];

fn member(path: &str, pages: u32) -> Member {
    Member {
        path: path.to_owned(),
        sha256: format!("sha-{path}"),
        pages,
    }
}

/// Members A (3 pages), B (2), C (0) and D (4), with spreads on B's first
/// page and D's first and a credit on D's last.
///
/// The hidden member is B, in the middle, so a translation that does nothing
/// lands on a different page; and C has no pages, so a translation that counts
/// members instead of pages is wrong too. Every test calls this first, so its
/// first assertions are the fixture's shape.
fn fixture() -> Edition {
    let edition = Edition {
        label: "merged".to_owned(),
        members: vec![
            member("A", 3),
            member("B", 2),
            member("C", 0),
            member("D", 4),
        ],
        spreads: vec![3, 5],
        credits: vec![8],
    };
    assert_eq!(edition.total_pages(), 9, "the fixture has 9 full pages");
    let owners: Vec<usize> = (0..9)
        .map(|page| edition.locate(page).expect("a page of the fixture").0)
        .collect();
    assert_eq!(
        owners,
        [0, 0, 0, 1, 1, 3, 3, 3, 3],
        "A is pages 0-2, B is 3-4, D is 5-8"
    );
    edition
}

/// Members A (1 page, a spread) and B (1 page), with B hidden.
///
/// The visible page before B is a spread, so its first slot and the last
/// narrowed slot before B differ; and B is last, so nothing visible follows it.
fn spread_then_hidden() -> Edition {
    let edition = Edition {
        label: "spread".to_owned(),
        members: vec![member("A", 1), member("B", 1)],
        spreads: vec![0],
        credits: vec![],
    };
    assert_eq!(edition.total_pages(), 2, "the fixture has 2 full pages");
    let owners: Vec<usize> = (0..2)
        .map(|page| edition.locate(page).expect("a page of the fixture").0)
        .collect();
    assert_eq!(owners, [0, 1], "A is page 0, a spread, and B is page 1");
    edition
}

fn narrowed(edition: &Edition) -> Narrowed {
    Narrowed::new(edition, &HIDE_B).expect("A and D have pages")
}

/// The full slots on `pages`, in order.
fn full_slots_on(edition: &Edition, reading: Direction, pages: &[u32]) -> Vec<u32> {
    edition
        .slots(reading)
        .iter()
        .enumerate()
        .filter(|(_, slot)| pages.contains(&slot.page))
        .map(|(index, _)| u32::try_from(index).expect("a small fixture"))
        .collect()
}

#[test]
fn the_narrowed_edition_holds_the_visible_members_with_spreads_and_credits_renumbered() {
    let full = fixture();
    let narrowed = narrowed(&full);
    let edition = narrowed.edition();

    let paths: Vec<&str> = edition.members.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, ["A", "C", "D"]);
    assert_eq!(
        edition.members[2], full.members[3],
        "a member is sent whole"
    );
    assert_eq!(edition.label, "merged");
    assert_eq!(edition.total_pages(), 7);
    assert_eq!(edition.spreads, [3], "D's first page is narrowed page 3");
    assert_eq!(edition.credits, [6], "D's last page is narrowed page 6");
}

#[test]
fn pages_translate_both_ways_and_a_hidden_page_has_no_narrowed_index() {
    let full = fixture();
    let narrowed = narrowed(&full);

    let to_full: Vec<Option<u32>> = [0, 2, 3, 6, 7]
        .iter()
        .map(|&page| narrowed.page_to_full(page))
        .collect();
    assert_eq!(to_full, [Some(0), Some(2), Some(5), Some(8), None]);

    let from_full: Vec<Option<u32>> = [0, 2, 3, 4, 5, 8, 9]
        .iter()
        .map(|&page| narrowed.page_from_full(page))
        .collect();
    assert_eq!(
        from_full,
        [Some(0), Some(2), None, None, Some(3), Some(6), None]
    );
}

#[test]
fn a_narrowed_member_translates_to_its_index_among_the_full_members() {
    let full = fixture();
    let narrowed = narrowed(&full);

    let to_full: Vec<Option<usize>> = (0..4).map(|m| narrowed.member_to_full(m)).collect();
    assert_eq!(to_full, [Some(0), Some(2), Some(3), None]);
}

#[test]
fn every_narrowed_slot_round_trips_and_keeps_its_half_in_both_directions() {
    let full = fixture();
    let narrowed = narrowed(&full);

    for reading in READINGS {
        let slots = narrowed.edition().slots(reading);
        let full_slots = full.slots(reading);
        assert_eq!(slots.len(), 8, "{reading:?}: 7 pages, one of them a spread");
        assert_eq!(
            (slots[3].page, slots[4].page),
            (3, 3),
            "{reading:?}: the spread at narrowed page 3 is two slots"
        );
        assert_ne!(slots[3].half, slots[4].half);

        let mapped: Vec<u32> = (0..8)
            .map(|slot| {
                narrowed
                    .slot_to_full(slot, reading)
                    .expect("a narrowed slot")
            })
            .collect();
        assert_eq!(mapped, [0, 1, 2, 6, 7, 8, 9, 10], "{reading:?}");

        for (slot, at) in slots.iter().enumerate() {
            let slot = u32::try_from(slot).expect("a small fixture");
            let full_slot = narrowed.slot_to_full(slot, reading).expect("mapped");
            let full_at = full_slots[full_slot as usize];
            assert_eq!(
                full_at,
                Slot {
                    page: narrowed.page_to_full(at.page).expect("visible"),
                    half: at.half,
                },
                "{reading:?}: narrowed slot {slot} names the same half of its page"
            );
            assert_eq!(
                narrowed.slot_from_full(full_slot, reading),
                FromFull::Visible(slot),
                "{reading:?}: full slot {full_slot} comes back to narrowed slot {slot}"
            );
        }
        assert_eq!(narrowed.slot_to_full(8, reading), None, "{reading:?}");
    }
}

#[test]
fn a_slot_on_a_hidden_page_falls_back_to_the_first_slot_of_the_visible_page_before_it() {
    let full = fixture();
    let narrowed = narrowed(&full);

    for reading in READINGS {
        let hidden = full_slots_on(&full, reading, &[3, 4]);
        assert_eq!(
            hidden,
            [3, 4, 5],
            "{reading:?}: B's spread and its next page"
        );
        let page_two = narrowed
            .edition()
            .slots(reading)
            .iter()
            .position(|slot| slot.page == 2);
        assert_eq!(
            page_two,
            Some(2),
            "{reading:?}: narrowed page 2 starts at slot 2"
        );
        for slot in hidden {
            assert_eq!(
                narrowed.slot_from_full(slot, reading),
                FromFull::Hidden { fallback: 2 },
                "{reading:?}: full slot {slot}"
            );
        }
    }
}

#[test]
fn a_slot_on_a_hidden_page_with_no_visible_page_before_it_falls_back_to_slot_zero() {
    let full = fixture();
    let narrowed = Narrowed::new(&full, &[false, true, true, true]).expect("B and D have pages");
    assert_eq!(
        narrowed.page_to_full(0),
        Some(3),
        "B's first page leads the narrowed edition"
    );

    for reading in READINGS {
        assert_eq!(
            narrowed.slot_from_full(0, reading),
            FromFull::Hidden { fallback: 0 },
            "{reading:?}"
        );
    }
}

#[test]
fn a_slot_on_a_hidden_last_member_falls_back_to_the_first_slot_of_a_spread_before_it() {
    let full = spread_then_hidden();
    let narrowed = Narrowed::new(&full, &[true, false]).expect("A has a page");

    for reading in READINGS {
        assert_eq!(
            full_slots_on(&full, reading, &[1]),
            [2],
            "{reading:?}: A's spread is full slots 0 and 1, B's page is slot 2"
        );
        let pages: Vec<u32> = narrowed
            .edition()
            .slots(reading)
            .iter()
            .map(|slot| slot.page)
            .collect();
        assert_eq!(
            pages,
            [0, 0],
            "{reading:?}: the narrowed edition is A's spread, slots 0 and 1"
        );
        assert_eq!(
            narrowed.slot_from_full(2, reading),
            FromFull::Hidden { fallback: 0 },
            "{reading:?}: B's page falls back to the spread's first slot, not its last"
        );
    }
}

#[test]
fn a_slot_past_the_end_of_the_full_edition_is_outside() {
    let full = fixture();
    let narrowed = narrowed(&full);

    for reading in READINGS {
        assert_eq!(full.slots(reading).len(), 11, "{reading:?}");
        assert_eq!(
            narrowed.slot_from_full(10, reading),
            FromFull::Visible(7),
            "{reading:?}: the last full slot is visible"
        );
        assert_eq!(
            narrowed.slot_from_full(11, reading),
            FromFull::Outside,
            "{reading:?}"
        );
    }
}

#[test]
fn nothing_is_narrowed_for_a_mask_of_the_wrong_length_or_with_no_visible_page() {
    let full = fixture();

    assert!(Narrowed::new(&full, &[true, false, true]).is_none());
    assert!(Narrowed::new(&full, &[true, false, true, true, true]).is_none());
    assert!(
        Narrowed::new(&full, &[false, false, true, false]).is_none(),
        "C, the only visible member, has no pages"
    );
    assert!(Narrowed::new(&full, &HIDE_B).is_some());
}
