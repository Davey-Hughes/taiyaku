//! O340's volume scorer, as the server runs it: a file's prints become slots,
//! and a pair of files' slots become V, the runs it was read from and the
//! solver's floor. The lookup picks which pairs to score, and the runs are cut
//! to whole pages before they are stored.
//!
//! A copy of the CLI's `match_experiment` scorer (`features.rs` and
//! `main.rs`'s `slot_hashes`), because editing that one would change the code
//! the bar was frozen with. The bar holds for this copy only while the two
//! agree, which the CLI's `the_server_scorer_agrees_with_the_frozen_one` holds.
//! The lookup is `lookup.rs`'s `votes`, kept to files sharing a library.

use std::collections::{BTreeMap, BTreeSet};

use taiyaku_core::{
    Calibration, Direction, PageHash, PageRange, Run, Slot, calibrate, expand, solve,
};

use crate::fingerprint::PagePrint;

/// The shortest paired run that counts towards V. Two unrelated files still
/// pair a few pages by chance, and chance seldom keeps going for four.
pub const LONG_RUN: u32 = 4;

/// Recorded with each file looked up, so a scorer that no longer gives the
/// same V can tell which files were scored by the old one.
pub const SCORER_VERSION: u32 = 1;

/// Bits differing between two hashes, 0 to 256.
pub fn hamming(a: PageHash, b: PageHash) -> u32 {
    a.0.iter()
        .zip(b.0.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

/// Whether a slot can say anything: a blank or solid page hashes to nearly
/// all zeros or all ones and sits near every other blank page.
pub fn informative(h: PageHash) -> bool {
    let ones: u32 = h.0.iter().map(|word| word.count_ones()).sum();
    (8..=248).contains(&ones)
}

/// A file's slots: one hash each, and the page (and half) each stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slots {
    pub hashes: Vec<PageHash>,
    pub table: Vec<Slot>,
}

/// One slot per page, and two for a page that reads as a joined spread and
/// has halves, in reading order. Whether a page is a spread is judged against
/// the whole file, so this takes the file rather than a page. A wide page
/// without halves keeps its whole hash rather than giving nothing.
pub fn slots(prints: &[PagePrint], direction: Direction) -> Slots {
    let dimensions: Vec<(u32, u32)> = prints.iter().map(|p| (p.width, p.height)).collect();
    let flags = taiyaku_core::spread_flags(&dimensions);

    let mut hashes = Vec::new();
    let mut wide = Vec::new();
    for (page, spread) in prints.iter().zip(&flags) {
        if let (true, Some((left, right))) = (*spread, page.halves) {
            // Reading order, matching the slot table `expand` gives.
            match direction {
                Direction::RightToLeft => hashes.extend([right, left]),
                Direction::LeftToRight => hashes.extend([left, right]),
            }
            wide.push(true);
        } else {
            hashes.push(page.hash);
            wide.push(false);
        }
    }
    Slots {
        hashes,
        table: expand(&wide, direction),
    }
}

/// A pair scored. `a_slots` is the side that went first, the shorter; `runs`
/// are from its slots to the other's.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub v: f64,
    pub long_paired: u32,
    pub a_slots: u32,
    pub b_slots: u32,
    pub runs: Vec<Run>,
    /// The floor the solver paired at, where it accepted the pair at all.
    pub floor: Option<f32>,
    /// Whether `y` went first.
    pub swapped: bool,
}

/// Score a pair, the shorter side first so a pair scores the same whichever
/// file names it first; equal lengths put the smaller key first.
pub fn score(x: (&str, &Slots), y: (&str, &Slots)) -> Scored {
    let swapped = (y.1.hashes.len(), y.0) < (x.1.hashes.len(), x.0);
    let (a, b) = if swapped {
        (&y.1.hashes, &x.1.hashes)
    } else {
        (&x.1.hashes, &y.1.hashes)
    };
    let runs = solve(a, b);
    let floor = calibrate(a, b)
        .filter(Calibration::accepted)
        .map(|cal| cal.floor);
    let a_slots = u32::try_from(a.len()).expect("a file's slot count fits u32");
    let b_slots = u32::try_from(b.len()).expect("a file's slot count fits u32");
    let long_paired = (runs.iter())
        .filter(|run| run.a.len() >= LONG_RUN)
        .filter_map(|run| run.b)
        .map(|b| b.len())
        .sum();
    Scored {
        v: volume_score(long_paired, a_slots, b_slots),
        long_paired,
        a_slots,
        b_slots,
        runs,
        floor,
        swapped,
    }
}

/// V: low unless long runs cover most of both sides.
fn volume_score(long_paired: u32, a: u32, b: u32) -> f64 {
    match a.max(b) {
        0 => 0.0,
        longer => f64::from(long_paired) / f64::from(longer),
    }
}

/// How many of a file's leaders are scored as candidate pairs.
pub const LEADERS: usize = 3;

/// A file taking part in the lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The file's digest.
    pub key: String,
    pub slots: Vec<PageHash>,
    /// The libraries with matching on that reach the file.
    pub libraries: BTreeSet<i64>,
    /// The parsed series and volume of the work holding the file, which only
    /// break a tie in votes.
    pub series: Option<i64>,
    pub volume: Option<f64>,
}

/// The files `entries[query]` should be scored against, at most [`LEADERS`],
/// best first, as indices into `entries`, which are sorted by key.
///
/// Each informative slot of the query votes for the file holding its nearest
/// informative slot among the other files sharing a library with it; a
/// distance tie goes to the earlier slot, by digest and then position. Files
/// tied on votes go to the query's series, then its volume, then the smaller
/// digest, so names order candidates but never change a count.
pub fn leaders(entries: &[Entry], query: usize) -> Vec<usize> {
    let q = &entries[query];
    let asking: Vec<PageHash> = (q.slots.iter().copied())
        .filter(|&slot| informative(slot))
        .collect();
    // Each asking slot's nearest so far, and its file. Stored slots are walked
    // in index order and only a nearer one replaces, so a tie keeps the
    // earlier; walking them once per query holds no copy of the index.
    let mut nearest: Vec<Option<(u32, usize)>> = vec![None; asking.len()];
    let others = (entries.iter().enumerate())
        .filter(|&(owner, e)| owner != query && !e.libraries.is_disjoint(&q.libraries));
    for (owner, e) in others {
        for &stored in e.slots.iter().filter(|&&slot| informative(slot)) {
            for (&slot, best) in asking.iter().zip(&mut nearest) {
                let distance = hamming(slot, stored);
                if best.is_none_or(|(d, _)| distance < d) {
                    *best = Some((distance, owner));
                }
            }
        }
    }
    let mut tally: BTreeMap<usize, u32> = BTreeMap::new();
    for (_, owner) in nearest.into_iter().flatten() {
        *tally.entry(owner).or_default() += 1;
    }

    let mut ranked: Vec<(usize, u32)> = tally.into_iter().collect();
    ranked.sort_by(|&(x, x_votes), &(y, y_votes)| {
        let (x, y) = (&entries[x], &entries[y]);
        (y_votes.cmp(&x_votes))
            .then_with(|| same(y.series, q.series).cmp(&same(x.series, q.series)))
            .then_with(|| same(y.volume, q.volume).cmp(&same(x.volume, q.volume)))
            .then_with(|| x.key.cmp(&y.key))
    });
    ranked.into_iter().take(LEADERS).map(|(e, _)| e).collect()
}

/// Whether a candidate shares the query's series or volume. A query without
/// one shares it with no file, though the file lacks it too.
fn same<T: PartialEq>(candidate: Option<T>, query: Option<T>) -> bool {
    query.is_some() && candidate == query
}

/// For each slot, its page's first slot and slot count.
fn page_of(table: &[Slot]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(table.len());
    for page in table.chunk_by(|x, y| x.page == y.page) {
        out.extend(std::iter::repeat_n((out.len(), page.len()), page.len()));
    }
    out
}

fn slot_at(index: u32) -> usize {
    usize::try_from(index).expect("a slot index fits usize")
}

/// The solver's slot runs from `a`'s slots to `b`'s, cut to whole pages.
///
/// A page pair is kept where the runs pair every slot of each page with a
/// slot of the other: both whole, or both split with their halves paired in
/// reading order. Editions a scan makes carry no spread flags, and a run
/// naming half a page is refused on them, so any other page a paired slot
/// touches is left unpaired, on both sides. Kept pairs consecutive on both
/// sides form one run.
pub fn page_runs(runs: &[Run], a: &[Slot], b: &[Slot]) -> Vec<(PageRange, PageRange)> {
    let mut partner: Vec<Option<usize>> = vec![None; a.len()];
    for run in runs {
        let Some(to) = run.b else { continue };
        for k in 0..run.a.len() {
            partner[slot_at(run.a.start + k)] = Some(slot_at(to.start + k));
        }
    }

    let b_pages = page_of(b);
    let mut out: Vec<(PageRange, PageRange)> = Vec::new();
    let mut first = 0;
    for page in a.chunk_by(|x, y| x.page == y.page) {
        let slots = first..first + page.len();
        first = slots.end;
        let Some(start) = partner[slots.start] else {
            continue;
        };
        let (b_first, b_count) = b_pages[start];
        let whole = b_count == page.len()
            && (slots.zip(b_first..)).all(|(from, to)| partner[from] == Some(to));
        if !whole {
            continue;
        }
        let (p, q) = (page[0].page, b[b_first].page);
        match out.last_mut() {
            Some((ra, rb)) if ra.end + 1 == p && rb.end + 1 == q => {
                ra.end = p;
                rb.end = q;
            }
            _ => out.push((PageRange::new(p, p), PageRange::new(q, q))),
        }
    }
    out
}

#[cfg(test)]
mod tests;
