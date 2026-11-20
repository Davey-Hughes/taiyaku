use std::collections::BTreeSet;

use taiyaku_core::{Direction, Half, PageHash, PageRange, Run, Slot, calibrate, expand};

use super::{Entry, Slots, hamming, informative, leaders, page_runs, score, slots};
use crate::fingerprint::PagePrint;

/// `n` hashes fixed by `seed`, by xorshift, so a pair of fixtures shares a
/// page only where a test puts the same hash in both.
fn hashes(seed: u64, n: usize) -> Vec<PageHash> {
    let mut state = seed;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..n)
        .map(|_| PageHash([next(), next(), next(), next()]))
        .collect()
}

/// A hash with its lowest `n` bits set.
fn with_ones(n: usize) -> PageHash {
    let mut words = [0u64; 4];
    for bit in 0..n {
        words[bit / 64] |= 1 << (bit % 64);
    }
    PageHash(words)
}

fn portrait(hash: PageHash) -> PagePrint {
    PagePrint {
        width: 1200,
        height: 1800,
        hash,
        halves: None,
    }
}

/// One whole portrait page a hash, so a side's slots are its hashes in order.
fn portraits(hashes: &[PageHash]) -> Slots {
    let prints: Vec<PagePrint> = hashes.iter().copied().map(portrait).collect();
    slots(&prints, Direction::RightToLeft)
}

/// The two sides of the V fixture: 10 slots against 14, sharing a run of 3
/// and a run of exactly 4, each run set apart by pages found on one side only.
/// A run of 3 counting, a run of 4 not counting, or V read over the shorter
/// side each gives a different V.
fn three_and_four() -> (Slots, Slots) {
    let shared = hashes(1, 7);
    let noise = hashes(2, 10);
    let (three, four) = (&shared[..3], &shared[3..]);
    let short: Vec<PageHash> = [&noise[..1], three, &noise[1..2], four, &noise[2..3]].concat();
    let long: Vec<PageHash> = [&noise[3..5], three, &noise[5..7], four, &noise[7..]].concat();
    (portraits(&short), portraits(&long))
}

fn whole(page: u32) -> Slot {
    Slot { page, half: None }
}

fn half(page: u32, half: Half) -> Slot {
    Slot {
        page,
        half: Some(half),
    }
}

/// A landscape page with halves splits into two slots in reading order; a
/// landscape page without halves stays one slot holding its whole hash, and so
/// does a page too wide to be a spread (a jacket), though the pass gives every
/// page wider than tall its halves.
#[test]
fn slots_split_a_spread_with_halves_in_reading_order() {
    let h = hashes(7, 9);
    let (p0, w1, l1, r1, p2, w3) = (h[0], h[1], h[2], h[3], h[4], h[5]);
    let (j4, jl, jr) = (h[6], h[7], h[8]);
    let landscape = |width, hash, halves| PagePrint {
        width,
        height: 1700,
        hash,
        halves,
    };
    let prints = [
        portrait(p0),
        landscape(2400, w1, Some((l1, r1))),
        portrait(p2),
        landscape(2400, w3, None),
        landscape(4200, j4, Some((jl, jr))),
    ];

    let rtl = slots(&prints, Direction::RightToLeft);
    assert_eq!(rtl.hashes, [p0, r1, l1, p2, w3, j4]);
    assert_eq!(
        rtl.table,
        [
            whole(0),
            half(1, Half::Right),
            half(1, Half::Left),
            whole(2),
            whole(3),
            whole(4)
        ]
    );

    let ltr = slots(&prints, Direction::LeftToRight);
    assert_eq!(ltr.hashes, [p0, l1, r1, p2, w3, j4]);
    assert_eq!(
        ltr.table,
        [
            whole(0),
            half(1, Half::Left),
            half(1, Half::Right),
            whole(2),
            whole(3),
            whole(4)
        ]
    );
}

/// The fixture's runs are the 3 and the 4 it claims, the shorter side goes
/// first though its key is the larger, and only the run of 4 counts, over 14.
#[test]
fn v_counts_runs_of_four_or_more_over_the_larger_side() {
    let (short, long) = three_and_four();
    let scored = score(("a", &long), ("b", &short));
    let paired: Vec<u32> = (scored.runs.iter())
        .filter_map(|run| run.b.map(|b| b.len()))
        .collect();
    assert_eq!(paired, [3, 4]);
    assert!(scored.swapped);
    assert_eq!((scored.a_slots, scored.b_slots), (10, 14));
    assert_eq!(scored.long_paired, 4);
    assert_eq!(scored.v.to_bits(), (4.0_f64 / 14.0).to_bits());
}

#[test]
fn equal_lengths_put_the_smaller_key_first() {
    let (x, y) = (portraits(&hashes(5, 6)), portraits(&hashes(6, 6)));
    assert!(!score(("a", &x), ("b", &y)).swapped);
    assert!(score(("b", &x), ("a", &y)).swapped);
}

/// The floor is the solver's only where the solver accepted the pair: an
/// unrelated pair is calibrated but refused, and keeps none.
#[test]
fn the_floor_is_kept_only_where_the_solver_accepted() {
    let (short, long) = three_and_four();
    let accepted = calibrate(&short.hashes, &long.hashes).expect("both sides have slots");
    assert!(accepted.accepted());
    let scored = score(("a", &short), ("b", &long));
    assert_eq!(
        scored.floor.map(f32::to_bits),
        Some(accepted.floor.to_bits())
    );

    let (x, y) = (portraits(&hashes(8, 6)), portraits(&hashes(9, 6)));
    let refused = calibrate(&x.hashes, &y.hashes).expect("both sides have slots");
    assert!(!refused.accepted());
    assert_eq!(score(("a", &x), ("b", &y)).floor, None);
}

#[test]
fn informative_counts_8_and_248_and_neither_neighbour() {
    assert!(!informative(with_ones(7)));
    assert!(informative(with_ones(8)));
    assert!(informative(with_ones(248)));
    assert!(!informative(with_ones(249)));
}

/// Every word counts, each differing bit once.
#[test]
fn hamming_counts_differing_bits_in_every_word() {
    assert_eq!(hamming(with_ones(0), with_ones(256)), 256);
    assert_eq!(hamming(with_ones(64), with_ones(200)), 136);
    let one_a_word = PageHash([1, 1 << 9, 1 << 33, 1 << 63]);
    assert_eq!(hamming(with_ones(0), one_a_word), 4);
    assert_eq!(hamming(one_a_word, one_a_word), 0);
}

/// A file of library 1 with no series or volume, so a fixture sets only what
/// its test separates.
fn entry(key: &str, slots: &[PageHash]) -> Entry {
    Entry {
        key: key.to_owned(),
        slots: slots.to_vec(),
        libraries: BTreeSet::from([1]),
        series: None,
        volume: None,
    }
}

/// `h` with its lowest `bits` bits flipped: that many bits from it.
fn flipped(h: PageHash, bits: u32) -> PageHash {
    let mut words = h.0;
    words[0] ^= (1 << bits) - 1;
    PageHash(words)
}

/// The query, in series 1 at volume 3, holds six slots and each is copied
/// into exactly one other file, so a file's votes are the copies it holds:
/// e and a 2 each, b and d 1 each. e (series 1, volume 4) beats a (series 2,
/// volume 3) only by series, and b and d (both series 1, volume 3) differ
/// only by digest, which also cuts d from the top three. b and d share the
/// query's series and volume and still trail a, so votes come first.
#[test]
fn leaders_rank_by_votes_then_series_then_volume_then_digest() {
    let q = hashes(11, 6);
    let at = |series, volume, slots: &[PageHash], key| Entry {
        series: Some(series),
        volume: Some(volume),
        ..entry(key, slots)
    };
    let entries = [
        at(2, 3.0, &[q[2], q[3]], "a"),
        at(1, 3.0, &[q[4]], "b"),
        at(1, 3.0, &q, "c"),
        at(1, 3.0, &[q[5]], "d"),
        at(1, 4.0, &[q[0], q[1]], "e"),
    ];
    assert_eq!(leaders(&entries, 2), [4, 0, 1]);
}

/// Every candidate takes one vote. With no series on the query, a candidate
/// without one is not in its series, so c's shared volume puts it first and
/// the digest orders the rest.
#[test]
fn leaders_tie_on_volume_and_a_missing_series_matches_none() {
    let q = hashes(12, 3);
    let entries = [
        Entry {
            volume: Some(1.0),
            ..entry("a", &[q[0]])
        },
        Entry {
            series: Some(5),
            volume: Some(1.0),
            ..entry("b", &[q[1]])
        },
        Entry {
            series: Some(5),
            volume: Some(2.0),
            ..entry("c", &[q[2]])
        },
        Entry {
            volume: Some(2.0),
            ..entry("d", &q)
        },
    ];
    assert_eq!(leaders(&entries, 3), [2, 0, 1]);
}

/// The query's one slot sits in a library-2 file unchanged, 1 bit from d
/// (libraries 1 and 2) and 2 bits from b (library 1). Sharing one library is
/// enough, and the vote goes to the nearest file sharing one.
#[test]
fn a_slot_votes_only_among_files_sharing_a_library() {
    let s = hashes(13, 1)[0];
    let entries = [
        Entry {
            libraries: BTreeSet::from([2]),
            ..entry("a", &[s])
        },
        entry("b", &[flipped(s, 2)]),
        entry("c", &[s]),
        Entry {
            libraries: BTreeSet::from([1, 2]),
            ..entry("d", &[flipped(s, 1)])
        },
    ];
    assert_eq!(leaders(&entries, 2), [3]);
}

/// a and c each hold a slot 1 bit from the query's, a different bit: the vote
/// goes to a, whose slot comes first in the index, by digest.
#[test]
fn a_distance_tie_goes_to_the_smaller_digest() {
    let s = hashes(14, 1)[0];
    let mut top_bit = s.0;
    top_bit[3] ^= 1 << 63;
    let entries = [
        entry("a", &[flipped(s, 1)]),
        entry("b", &[s]),
        entry("c", &[PageHash(top_bit)]),
    ];
    assert_eq!(leaders(&entries, 1), [0]);
}

/// c's slots (0, 7 and 249 bits) would vote for d, 1 bit from two of them;
/// d's (8 and 248) would vote for a's and c's copies of 7 and 249, 1 bit
/// away, before b's, 8 away.
#[test]
fn uninformative_slots_neither_vote_nor_draw_votes() {
    let [none, seven, eight] = [0, 7, 8].map(with_ones);
    let [sixteen, b240, b248, b249] = [16, 240, 248, 249].map(with_ones);
    let entries = [
        entry("a", &[seven, b249, none]),
        entry("b", &[sixteen, b240]),
        entry("c", &[none, seven, b249]),
        entry("d", &[eight, b248]),
    ];
    assert_eq!(leaders(&entries, 2), Vec::<usize>::new());
    assert_eq!(leaders(&entries, 3), [1]);
}

fn range(start: u32, end: u32) -> PageRange {
    PageRange::new(start, end)
}

fn paired(a: (u32, u32), b: (u32, u32)) -> Run {
    Run::paired(range(a.0, a.1), range(b.0, b.1))
}

/// A has a spread at page 5, which B has as its pages 5 and 6: one solver
/// run pairs every slot. A's 5 and B's 5 and 6 are left unpaired, and the
/// same holds with the sides swapped.
#[test]
fn page_runs_cut_where_one_side_split_a_spread() {
    let mut wide = [false; 10];
    wide[5] = true;
    let a = expand(&wide, Direction::RightToLeft);
    let b = expand(&[false; 11], Direction::RightToLeft);
    let runs = [paired((0, 10), (0, 10))];
    assert_eq!(
        page_runs(&runs, &a, &b),
        [(range(0, 4), range(0, 4)), (range(6, 9), range(7, 10))]
    );
    assert_eq!(
        page_runs(&runs, &b, &a),
        [(range(0, 4), range(0, 4)), (range(7, 10), range(6, 9))]
    );
}

/// Both sides split the same spread, A's page 3 and B's page 4, B reading the
/// other way: its halves pair in reading order, so the page stays paired and
/// the run runs through it. B's page 0 has no partner.
#[test]
fn page_runs_keep_a_spread_split_on_both_sides() {
    let a = expand(
        &[false, false, false, true, false, false, false],
        Direction::RightToLeft,
    );
    let b = expand(
        &[false, false, false, false, true, false, false, false],
        Direction::LeftToRight,
    );
    let runs = [paired((0, 7), (1, 8))];
    assert_eq!(page_runs(&runs, &a, &b), [(range(0, 6), range(1, 7))]);
}

/// A joins its pages 1 and 2 into one spread where B joins 0 and 1, and 2
/// and 3: A's spread has the same slot count as each of B's, but its halves
/// pair with the second half of one and the first half of the other, so no
/// page is kept.
#[test]
fn page_runs_cut_a_spread_whose_halves_land_in_two_spreads() {
    let a = expand(&[false, true, false], Direction::RightToLeft);
    let b = expand(&[true, true], Direction::RightToLeft);
    let runs = [paired((0, 3), (0, 3))];
    assert_eq!(page_runs(&runs, &a, &b), []);
}

/// B's page 4 has no partner, so the pages either side of it are consecutive
/// on A only and stay two runs.
#[test]
fn page_runs_break_where_b_skips_a_page() {
    let a = expand(&[false; 8], Direction::RightToLeft);
    let b = expand(&[false; 9], Direction::RightToLeft);
    let runs = [paired((0, 3), (0, 3)), paired((4, 7), (5, 8))];
    assert_eq!(
        page_runs(&runs, &a, &b),
        [(range(0, 3), range(0, 3)), (range(4, 7), range(5, 8))]
    );
}
