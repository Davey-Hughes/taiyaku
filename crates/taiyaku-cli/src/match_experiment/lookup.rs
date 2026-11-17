//! The library-wide lookup: which members a member's pages point at, before
//! any pair is aligned. A scan cannot afford to solve every pair, so this is
//! how it would choose the few worth solving.
//!
//! Each member is fingerprinted as `align` reads it ([`print_all`], one
//! `members.tsv` row each, readable or not), every member's slots vote over
//! an index of all the others ([`lookup_all`]), and the reports read the
//! votes: `leaders.json`, `lookup.txt` for the cost, and [`query_report`] for
//! where each query's best positive ranked.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use taiyaku_core::{Direction, PageHash, Slot};

use super::manifest::{ARCHIVES, Kind, Label, Manifest, Member, Split, label};
use super::rule::MIN_CHAPTER_SLOTS;
use crate::fingerprint::hamming;

/// How many of a member's leaders are scored as candidate pairs.
pub(super) const LEADERS: usize = 3;

/// Whether a slot can say anything: a blank or solid page hashes to nearly
/// all zeros or all ones, sits near every other blank page, and would vote
/// for whichever member happens to hold the most of them.
pub(super) fn informative(h: PageHash) -> bool {
    let ones: u32 = h.0.iter().map(|word| word.count_ones()).sum();
    (8..=248).contains(&ones)
}

/// Every informative slot of every member, beside the member it belongs to.
#[derive(Debug)]
pub(super) struct Index {
    pub hashes: Vec<PageHash>,
    pub owner: Vec<u32>,
}

pub(super) fn index(members: &[Vec<PageHash>]) -> Index {
    let mut hashes = Vec::new();
    let mut owner = Vec::new();
    for (member, slots) in members.iter().enumerate() {
        let member = u32::try_from(member).expect("a library's member count fits u32");
        for &slot in slots.iter().filter(|&&slot| informative(slot)) {
            hashes.push(slot);
            owner.push(member);
        }
    }
    Index { hashes, owner }
}

/// Each informative slot of `query` votes for the owner of its nearest
/// indexed slot among the other members; a tie goes to the earlier position.
/// Returns `(member, votes)`, most votes first, then by member.
pub(super) fn votes(index: &Index, query: u32, slots: &[PageHash]) -> Vec<(u32, u32)> {
    let mut tally: BTreeMap<u32, u32> = BTreeMap::new();
    for &slot in slots.iter().filter(|&&slot| informative(slot)) {
        let mut nearest: Option<(u32, u32)> = None;
        for (&stored, &owner) in index.hashes.iter().zip(&index.owner) {
            if owner == query {
                continue;
            }
            let distance = hamming(slot, stored);
            if nearest.is_none_or(|(best, _)| distance < best) {
                nearest = Some((distance, owner));
            }
        }
        if let Some((_, owner)) = nearest {
            *tally.entry(owner).or_default() += 1;
        }
    }
    let mut ranked: Vec<(u32, u32)> = tally.into_iter().collect();
    ranked.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
    ranked
}

/// The lookup's inner loop alone, timed: every query's nearest neighbour over
/// every stored hash. Returns the comparisons made and the wall time.
pub(super) fn kernel(queries: &[PageHash], stored: &[PageHash]) -> (u64, Duration) {
    let started = Instant::now();
    let comparisons = queries
        .par_iter()
        .map(|&query| {
            let mut made = 0u64;
            let mut best = u32::MAX;
            for &other in stored {
                best = best.min(hamming(query, other));
                made += 1;
            }
            std::hint::black_box(best);
            made
        })
        .sum();
    (comparisons, started.elapsed())
}

/// One member's fingerprints, in member order. `hits` of the `paths` read
/// came from the cache, so their fingerprints may be older than this run.
#[derive(Debug)]
pub(super) struct Print {
    pub slots: Vec<PageHash>,
    pub table: Vec<Slot>,
    pub pages: u32,
    pub bytes: u64,
    pub hits: usize,
    pub paths: usize,
}

/// One line of `members.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub path: String,
    pub kind: Kind,
    pub pages: u32,
    pub slots: usize,
    pub bytes: u64,
    pub ms: u64,
    /// A printed member's cache hits and paths read.
    pub cache: Option<(usize, usize)>,
    pub error: Option<String>,
}

/// A member's fingerprints through the CLI's cache, as `align` reads it. A
/// page's own error names only the page, so the member is added to it.
pub(super) fn print_member(cache: &Path, library: &Path, m: &Member) -> Result<Print, String> {
    let direction = if m.ltr {
        Direction::LeftToRight
    } else {
        Direction::RightToLeft
    };
    let read = || {
        let edition = edition_paths(&library.join(&m.path))?;
        let (slots, table, hits) = crate::cached_edition(cache, &edition, direction)?;
        let bytes = edition.iter().map(|p| size(p)).sum::<Result<u64, _>>()?;
        let pages = table.last().map_or(0, |slot| slot.page + 1);
        Ok(Print {
            slots,
            table,
            pages,
            bytes,
            hits,
            paths: edition.len(),
        })
    };
    read().map_err(|e: String| {
        if e.contains(&m.path) {
            e
        } else {
            format!("{}: {e}", m.path)
        }
    })
}

/// What `cached_edition` reads for a member: the member itself, or, for a
/// directory of archives, those archives in the order it joins them, so a
/// joined page index can be traced to its archive. Pages beside the
/// archives, loose or in a subdirectory, are refused, as either reading
/// would drop some.
pub(super) fn edition_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Ok(vec![root.to_path_buf()]);
    }
    let fail = |e: std::io::Error| format!("{}: {e}", root.display());
    let (mut archives, mut loose) = (Vec::new(), false);
    for entry in std::fs::read_dir(root).map_err(fail)? {
        let path = entry.map_err(fail)?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        if path.is_dir() {
            loose |= taiyaku_read::member_pages(&path).is_ok();
        } else if ext.is_some_and(|e| ARCHIVES.contains(&e.as_str())) {
            archives.push(path);
        } else {
            loose |= taiyaku_read::is_page_name(&name);
        }
    }
    match (archives.is_empty(), loose) {
        (true, _) => Ok(vec![root.to_path_buf()]),
        (false, false) => {
            archives.sort_by(|a, b| {
                taiyaku_read::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy())
            });
            Ok(archives)
        }
        (false, true) => Err(format!(
            "{}: holds both archives and loose pages",
            root.display()
        )),
    }
}

/// A file's length, or every file's under a directory.
pub(super) fn size(path: &Path) -> Result<u64, String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    let meta = std::fs::metadata(path).map_err(fail)?;
    if !meta.is_dir() {
        return Ok(meta.len());
    }
    let mut total = 0;
    for entry in std::fs::read_dir(path).map_err(fail)? {
        total += size(&entry.map_err(fail)?.path())?;
    }
    Ok(total)
}

/// Every member's print, or `None` with the reason in its row. Members run
/// one at a time, since a member's pages already decode in parallel and a
/// row's milliseconds should be its own.
pub(super) fn print_all(
    cache: &Path,
    library: &Path,
    m: &Manifest,
) -> (Vec<Option<Print>>, Vec<Row>) {
    (m.members.iter())
        .map(|member| {
            let started = Instant::now();
            let print = print_member(cache, library, member);
            let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let row = Row {
                path: member.path.clone(),
                kind: member.kind,
                pages: print.as_ref().map_or(0, |p| p.pages),
                slots: print.as_ref().map_or(0, |p| p.slots.len()),
                bytes: print.as_ref().map_or(0, |p| p.bytes),
                ms,
                cache: print.as_ref().ok().map(|p| (p.hits, p.paths)),
                error: print.as_ref().err().cloned(),
            };
            (print.ok(), row)
        })
        .unzip()
}

/// One TSV cell: an error can hold a tab or a line break.
pub(super) fn cell(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

fn kind_name(kind: Kind) -> String {
    let json = serde_json::to_string(&kind).expect("a kind serialises");
    json.trim_matches('"').to_owned()
}

/// `hit` when every path came from the cache, `miss` when none did, else
/// `k of n`; `-` for a member that did not print.
fn cache_cell(cache: Option<(usize, usize)>) -> String {
    match cache {
        None => "-".to_owned(),
        Some((0, _)) => "miss".to_owned(),
        Some((hits, paths)) if hits == paths => "hit".to_owned(),
        Some((hits, paths)) => format!("{hits} of {paths}"),
    }
}

pub(super) fn members_tsv(stamp: &str, rows: &[Row]) -> String {
    let mut out = format!("{stamp}\npath\tkind\tpages\tslots\tbytes\tms\tcache\terror\n");
    for r in rows {
        let error = r.error.as_deref().map(cell).unwrap_or_default();
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{error}",
            cell(&r.path),
            kind_name(r.kind),
            r.pages,
            r.slots,
            r.bytes,
            r.ms,
            cache_cell(r.cache)
        );
    }
    out
}

/// A log line for a member that did not print. Its error already names the
/// member, so the line is the error alone, on one line.
pub(super) fn unreadable_line(row: &Row) -> Option<String> {
    (row.error.as_deref()).map(|error| format!("unreadable: {}", cell(error)))
}

/// One member's lookup: its informative slots, and the members they voted for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Lookup {
    pub informative: u32,
    pub votes: Vec<(u32, u32)>,
}

/// What the whole lookup cost.
#[derive(Debug, Clone, Copy)]
pub(super) struct Cost {
    pub indexed: usize,
    pub comparisons: u64,
    pub wall: Duration,
    pub threads: usize,
}

pub(super) fn at(member: u32) -> usize {
    usize::try_from(member).expect("a member number fits usize")
}

/// Every printed member's votes, timed. An unreadable member is indexed as
/// holding nothing, so a vote's member number stays its manifest position.
pub(super) fn lookup_all(prints: &[Option<Print>]) -> (Vec<Option<Lookup>>, Cost) {
    let started = Instant::now();
    let slots: Vec<Vec<PageHash>> = (prints.iter())
        .map(|p| p.as_ref().map_or_else(Vec::new, |p| p.slots.clone()))
        .collect();
    let idx = index(&slots);
    let lookups: Vec<Option<Lookup>> = (prints.par_iter().enumerate())
        .map(|(i, p)| {
            let p = p.as_ref()?;
            let query = u32::try_from(i).expect("a library's member count fits u32");
            let count = p.slots.iter().filter(|&&slot| informative(slot)).count();
            Some(Lookup {
                informative: u32::try_from(count).expect("a member's slot count fits u32"),
                votes: votes(&idx, query, &p.slots),
            })
        })
        .collect();
    let wall = started.elapsed();
    let indexed = u64::try_from(idx.hashes.len()).expect("an index length fits u64");
    // `votes` skips the query's own indexed slots before comparing.
    let comparisons = (lookups.iter().flatten())
        .map(|l| u64::from(l.informative) * (indexed - u64::from(l.informative)))
        .sum();
    let cost = Cost {
        indexed: idx.hashes.len(),
        comparisons,
        wall,
        threads: rayon::current_num_threads(),
    };
    (lookups, cost)
}

/// How many leaders `leaders.json` keeps for each member.
const REPORTED: usize = 10;

/// `leaders.json`: each looked-up member's informative slot count and its
/// first [`REPORTED`] leaders as `[path, votes]`.
pub(super) fn leaders_json(stamp: &str, members: &[Member], lookups: &[Option<Lookup>]) -> String {
    #[derive(serde::Serialize)]
    struct Entry<'a> {
        informative: u32,
        leaders: Vec<(&'a str, u32)>,
    }
    #[derive(serde::Serialize)]
    struct File<'a> {
        stamp: &'a str,
        members: BTreeMap<&'a str, Entry<'a>>,
    }

    let entries = (members.iter().zip(lookups))
        .filter_map(|(member, lookup)| {
            let lookup = lookup.as_ref()?;
            let leaders = (lookup.votes.iter().take(REPORTED))
                .map(|&(other, n)| (members[at(other)].path.as_str(), n))
                .collect();
            let entry = Entry {
                informative: lookup.informative,
                leaders,
            };
            Some((member.path.as_str(), entry))
        })
        .collect();
    let file = File {
        stamp,
        members: entries,
    };
    serde_json::to_string_pretty(&file).expect("leaders serialise") + "\n"
}

/// `count` over `wall` as a whole number per second, `none` when no time
/// passed.
pub(super) fn per_second(count: u64, wall: Duration) -> String {
    (u128::from(count) * 1_000_000_000)
        .checked_div(wall.as_nanos())
        .map_or_else(|| "none".to_owned(), |rate| rate.to_string())
}

/// `lookup.txt`: what the lookup over the whole library cost.
pub(super) fn lookup_txt(stamp: &str, cost: &Cost) -> String {
    let rate = per_second(cost.comparisons, cost.wall);
    format!(
        "{stamp}\nslots indexed\t{}\ncomparisons\t{}\nwall seconds\t{:.3}\nthreads\t{}\n\
         comparisons per second\t{rate}\n",
        cost.indexed,
        cost.comparisons,
        cost.wall.as_secs_f64(),
        cost.threads
    )
}

/// P2's share, in percent, of chapter queries a holding member must lead.
const P2_PERCENT: usize = 90;

/// Whether `hits` of `n` is at least `percent`. No queries reach nothing,
/// since a prediction nothing tested has not been shown to hold.
pub(super) fn reaches(hits: usize, n: usize, percent: usize) -> bool {
    n > 0 && 100 * hits >= percent * n
}

fn verdict(hits: usize, n: usize, percent: usize) -> &'static str {
    match (n, reaches(hits, n, percent)) {
        (0, _) => "not measured",
        (_, true) => "holds",
        (_, false) => "fails",
    }
}

/// What a pair is to the lookup, whichever member is the query: `Some(true)`
/// for content a scan must link, so the lookup must find the pair to align
/// it, a pack holding a volume included; `Some(false)` for a pair it need not
/// find; `None` for neither. Every label is named, so a new one fails to
/// compile here until it is placed.
pub(super) fn lookup_class(label: Label) -> Option<bool> {
    use Label as L;
    match label {
        L::SameVolume { .. } | L::InPack | L::ChapterIn | L::InCollected => Some(true),
        L::Neighbour
        | L::SameSeriesOther
        | L::NotInPack
        | L::ChapterNotIn
        | L::NotInCollected
        | L::OtherSeries { .. }
        | L::CoverStories => Some(false),
        L::Omnibus | L::PackOverlap | L::Unlabelled => None,
    }
}

/// Each member's label with `query`, by manifest position, `None` for the
/// query itself. Members that did not print keep their label, so a query
/// whose positives all failed to print can say so.
fn labels(m: &Manifest, query: usize) -> Vec<Option<Label>> {
    let q = &m.members[query];
    (m.members.iter().enumerate())
        .map(|(j, other)| (j != query).then(|| label(m, q, other)))
        .collect()
}

/// Whether a query has a positive among the members that printed, has only
/// positives that did not, or has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Positives {
    Printed,
    AllUnprinted,
    Absent,
}

/// Where a query's best positive and best negative fell among the members
/// it voted for. `rank` counts from 1 and is `None` when no positive got a
/// vote; `negative` is `None` when no negative did. `chapter_in` says a
/// printed member holds the query, a chapter.
#[derive(Debug)]
struct Outcome {
    positives: Positives,
    rank: Option<usize>,
    positive: u32,
    negative: Option<u32>,
    chapter_in: bool,
    chapter_in_leads: bool,
}

fn outcome(votes: &[(u32, u32)], labels: &[Option<Label>], prints: &[Option<Print>]) -> Outcome {
    let class = |member: u32| labels[at(member)].and_then(lookup_class);
    let any_positive = |printed: bool| {
        (labels.iter().zip(prints))
            .any(|(l, p)| p.is_some() == printed && l.and_then(lookup_class) == Some(true))
    };
    let printed_chapter_in =
        (labels.iter().zip(prints)).any(|(l, p)| p.is_some() && *l == Some(Label::ChapterIn));
    let best = |want: bool| {
        (votes.iter().enumerate()).find(|(_, (member, _))| class(*member) == Some(want))
    };
    let positive = best(true);
    let chapter_in = |member: u32| labels[at(member)] == Some(Label::ChapterIn);
    Outcome {
        positives: if any_positive(true) {
            Positives::Printed
        } else if any_positive(false) {
            Positives::AllUnprinted
        } else {
            Positives::Absent
        },
        rank: positive.map(|(i, _)| i + 1),
        positive: positive.map_or(0, |(_, &(_, n))| n),
        negative: best(false).map(|(_, &(_, n))| n),
        chapter_in: printed_chapter_in,
        chapter_in_leads: votes.first().is_some_and(|&(member, _)| chapter_in(member)),
    }
}

/// `hits` of `n` as a percentage to one place, `-` when `n` is zero.
pub(super) fn share(hits: usize, n: usize) -> String {
    (1000 * hits + n / 2)
        .checked_div(n)
        .map_or_else(|| "-".to_owned(), |t| format!("{}.{}%", t / 10, t % 10))
}

/// The lookup report for one split's queries (`dev/lookup.txt`): a row per
/// query, then the depth the best positives need and P1 and P2 as measured.
/// A query's candidates are every other printed member, of either split,
/// each a positive, a negative or neither by its label ([`lookup_class`]).
pub(super) fn query_report(
    stamp: &str,
    split: Split,
    m: &Manifest,
    prints: &[Option<Print>],
    lookups: &[Option<Lookup>],
) -> String {
    let queries: Vec<(&Member, usize, &Lookup, Outcome)> = (0..m.members.len())
        .into_par_iter()
        .filter_map(|i| {
            let member = &m.members[i];
            let lookup = lookups[i].as_ref().filter(|_| member.split == split)?;
            let slots = prints[i].as_ref()?.slots.len();
            let outcome = outcome(&lookup.votes, &labels(m, i), prints);
            Some((member, slots, lookup, outcome))
        })
        .collect();

    let mut out = format!("{stamp}\nquery\tkind\tinformative\trank\tpositive\tnegative\tmargin\n");
    for (member, _, lookup, o) in &queries {
        let negative = o.negative.unwrap_or(0);
        let without = |why: &str| {
            (
                why.to_owned(),
                "-".to_owned(),
                o.negative.map(|n| -f64::from(n)),
            )
        };
        let (rank, positive, spread) = match o.positives {
            Positives::Printed => {
                let rank = o.rank.map_or_else(|| "none".to_owned(), |r| r.to_string());
                let spread = f64::from(o.positive) - f64::from(negative);
                (rank, o.positive.to_string(), Some(spread))
            }
            Positives::AllUnprinted => without("positive unprinted"),
            Positives::Absent => without("no positive"),
        };
        let informative = lookup.informative;
        let margin = (spread.filter(|_| informative > 0)).map_or_else(
            || "none".to_owned(),
            |s| format!("{:.4}", s / f64::from(informative)),
        );
        let (path, kind) = (cell(&member.path), kind_name(member.kind));
        let _ = writeln!(
            out,
            "{path}\t{kind}\t{informative}\t{rank}\t{positive}\t{negative}\t{margin}"
        );
    }

    let with: Vec<&Outcome> = (queries.iter())
        .map(|(_, _, _, o)| o)
        .filter(|o| o.positives == Positives::Printed)
        .collect();
    let n = with.len();
    let unprinted = (queries.iter())
        .filter(|(_, _, _, o)| o.positives == Positives::AllUnprinted)
        .count();
    let leads = with.iter().filter(|o| o.rank == Some(1)).count();
    let unvoted = with.iter().filter(|o| o.rank.is_none()).count();
    let depth = match (unvoted, with.iter().filter_map(|o| o.rank).max()) {
        (0, Some(deepest)) => deepest.to_string(),
        (0, None) => "-".to_owned(),
        (1, _) => "none, 1 best positive got no vote".to_owned(),
        (k, _) => format!("none, {k} best positives got no vote"),
    };
    let in_top = (with.iter())
        .filter(|o| o.rank.is_some_and(|r| r <= LEADERS))
        .count();
    let chapters: Vec<(usize, &Outcome)> = (queries.iter())
        .filter(|(member, _, _, o)| member.kind == Kind::Chapter && o.chapter_in)
        .map(|(_, slots, _, o)| (*slots, o))
        .collect();
    let led = |min: usize| {
        let pool = chapters.iter().filter(|(slots, _)| *slots >= min);
        let hits = pool.clone().filter(|(_, o)| o.chapter_in_leads).count();
        (hits, pool.count())
    };
    let (hits, all) = led(0);
    let long = usize::try_from(MIN_CHAPTER_SLOTS).expect("a slot count fits usize");
    let (long_hits, long_all) = led(long);
    let _ = write!(
        out,
        "queries: {}, with a positive: {n}\n\
         queries whose positives all failed to print: {unprinted}\n\
         best positive leads: {leads} of {n}\n\
         depth needed: {depth}\n\
         P1, every query's best positive in its top {LEADERS}: {in_top} of {n} queries with a \
         positive, {}\n\
         P2, a member holding the chapter leads at least {P2_PERCENT}% of chapter queries: \
         {hits} of {all} chapters with a holder ({}), {}\n\
         P2 on chapters of {MIN_CHAPTER_SLOTS} or more slots: {long_hits} of {long_all} ({})\n",
        queries.len(),
        verdict(in_top, n, 100),
        share(hits, all),
        verdict(hits, all, P2_PERCENT),
        share(long_hits, long_all),
    );
    out
}
