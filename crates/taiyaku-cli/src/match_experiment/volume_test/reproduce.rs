//! Whether the server's scorer (`taiyaku_serve::volume_match`) gives every
//! pair the O340 measurement scored the score its `scores.tsv` records, read
//! from the pages as the fingerprint pass stores them
//! (`docs/superpowers/specs/2026-11-19-content-matching-volume-joins-design.md`,
//! "Checks that prove it", 1). The bar holds for the server only while it does.
//!
//! It reads every member from the local test library, so it is ignored and run
//! by hand; CI holds the two scorers equal on fixtures
//! (`the_server_scorer_agrees_with_the_frozen_one`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use taiyaku_core::Direction;
use taiyaku_serve::fingerprint::{self, PagePrint};
use taiyaku_serve::volume_match::{self, Scored, Slots};

use super::super::{put, stamp};
use super::bar::read_frozen;
use super::members::{TestMember, all_pairs, committed};
use super::run::{PairRow, read_scores};

const TEST: &str = "the_server_scorer_gives_every_o340_pair_its_score";

/// A row's compared cells, as `scores.tsv` prints them: V to six decimals.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cells {
    a: String,
    b: String,
    a_slots: u32,
    b_slots: u32,
    long_paired: u32,
    v: String,
}

impl Cells {
    fn recorded(row: &PairRow) -> Self {
        Cells {
            a: row.a.clone(),
            b: row.b.clone(),
            a_slots: row.a_slots,
            b_slots: row.b_slots,
            long_paired: row.long_paired,
            v: format!("{:.6}", row.v),
        }
    }

    /// `x` and `y` as the scorer was given them, which it may have swapped.
    fn scored(x: &str, y: &str, scored: &Scored) -> Self {
        let (a, b) = if scored.swapped { (y, x) } else { (x, y) };
        Cells {
            a: a.to_owned(),
            b: b.to_owned(),
            a_slots: scored.a_slots,
            b_slots: scored.b_slots,
            long_paired: scored.long_paired,
            v: format!("{:.6}", scored.v),
        }
    }

    fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.a, self.b, self.a_slots, self.b_slots, self.long_paired, self.v
        )
    }
}

/// A pair's paths in sorted order, the key a row finds its pair by.
type Key = (String, String);

/// Each row of `scores.tsv` and what the server's scorer gave its pair, and
/// the pairs of members no row named.
#[derive(Debug)]
struct Compared {
    rows: Vec<(Cells, Option<Cells>)>,
    unlisted: Vec<Key>,
}

/// An equal-length pair whose V keyed by digest is not its V keyed by path.
#[derive(Debug)]
struct Moved {
    by_path: Cells,
    digests: (String, String),
    by_digest: String,
    crosses: bool,
}

/// What keying the equal-length pairs by digest changed.
#[derive(Debug)]
struct ByDigest {
    pairs: usize,
    other_side_first: usize,
    moved: Vec<Moved>,
}

/// `var`'s path, refused unless absolute and `exists` holds of it: cargo runs
/// a package's tests from that package's directory, where a relative path
/// finds nothing, and a run that read nothing must not pass for one that read
/// every member.
fn path_in(var: &str, exists: fn(&Path) -> bool) -> PathBuf {
    let value = std::env::var(var).unwrap_or_else(|_| panic!("{var} is not set"));
    let path = PathBuf::from(&value);
    assert!(
        path.is_absolute() && exists(&path),
        "{var}={value} is not an absolute path to an existing file or directory of the kind \
         this test reads; a relative path resolves under crates/taiyaku-cli/"
    );
    path
}

/// A member's slots as the server takes them: its pages through
/// `taiyaku_read`, the border its sampled pages agree on, each page described,
/// the prints through the blob the fingerprint pass stores and the match pass
/// reads back, slotted right to left as O340 read every member
/// (`members::row`).
fn slots_of(library: &Path, member: &TestMember) -> Slots {
    let pages = taiyaku_read::read_edition(&[library.join(&member.path)])
        .unwrap_or_else(|why| panic!("{}: {why}", member.path));
    let border = fingerprint::common_border(&pages);
    let described = crate::fingerprint::decode_all(&pages, |_, luma| {
        Some(fingerprint::describe_page(luma, border))
    })
    .unwrap_or_else(|why| panic!("{}: {why}", member.path));
    let blob = fingerprint::encode(&described);
    let prints: Vec<PagePrint> = fingerprint::decode(&blob)
        .unwrap_or_else(|why| panic!("{}: {why}", member.path))
        .into_iter()
        .enumerate()
        .map(|(i, page)| {
            page.unwrap_or_else(|| panic!("{}: page {i} came back unread", member.path))
        })
        .collect();
    volume_match::slots(&prints, Direction::RightToLeft)
}

fn ordered(x: &str, y: &str) -> Key {
    let (x, y) = if x <= y { (x, y) } else { (y, x) };
    (x.to_owned(), y.to_owned())
}

/// Every pair of members scored keyed by path, as O340 keyed them.
fn by_path(members: &[TestMember], slots: &[Slots]) -> BTreeMap<Key, Cells> {
    (all_pairs(members.len()).par_iter())
        .map(|&(i, j)| {
            let (x, y) = (members[i].path.as_str(), members[j].path.as_str());
            let scored = volume_match::score((x, &slots[i]), (y, &slots[j]));
            (ordered(x, y), Cells::scored(x, y, &scored))
        })
        .collect()
}

/// Each row with what the server's scorer gave its pair, `None` where the row
/// names no pair of members or one an earlier row took; and the pairs no row
/// took.
fn compare(rows: &[PairRow], mut scored: BTreeMap<Key, Cells>) -> Compared {
    let rows = (rows.iter())
        .map(|row| {
            (
                Cells::recorded(row),
                scored.remove(&ordered(&row.a, &row.b)),
            )
        })
        .collect();
    Compared {
        rows,
        unlisted: scored.into_keys().collect(),
    }
}

/// The pairs of equal slot counts keyed by digest, as the server keys them:
/// the smaller key goes first and `solve` is not symmetric, so a pair whose
/// digests order its sides the other way from its paths can score another V.
fn by_digest(library: &Path, members: &[TestMember], slots: &[Slots], bar: f64) -> ByDigest {
    let pairs: Vec<(usize, usize)> = (all_pairs(members.len()).into_iter())
        .filter(|&(i, j)| slots[i].hashes.len() == slots[j].hashes.len())
        .collect();
    let read: BTreeSet<usize> = pairs.iter().flat_map(|&(i, j)| [i, j]).collect();
    let digests: BTreeMap<usize, String> = (read.into_par_iter())
        .map(|i| {
            let path = &members[i].path;
            let digest = taiyaku_read::member_digest(&library.join(path))
                .unwrap_or_else(|why| panic!("{path}: {why}"));
            (i, digest)
        })
        .collect();
    let mut found = ByDigest {
        pairs: pairs.len(),
        other_side_first: 0,
        moved: Vec::new(),
    };
    for (i, j) in pairs {
        let (x, y) = (members[i].path.as_str(), members[j].path.as_str());
        let (x_key, y_key) = (digests[&i].as_str(), digests[&j].as_str());
        let path_keyed = volume_match::score((x, &slots[i]), (y, &slots[j]));
        let digest_keyed = volume_match::score((x_key, &slots[i]), (y_key, &slots[j]));
        found.other_side_first += usize::from(path_keyed.swapped != digest_keyed.swapped);
        let by_path = Cells::scored(x, y, &path_keyed);
        let by_digest = format!("{:.6}", digest_keyed.v);
        if by_digest != by_path.v {
            let (a_key, b_key) = if path_keyed.swapped {
                (y_key, x_key)
            } else {
                (x_key, y_key)
            };
            found.moved.push(Moved {
                by_path,
                digests: (a_key.to_owned(), b_key.to_owned()),
                by_digest,
                crosses: (path_keyed.v >= bar) != (digest_keyed.v >= bar),
            });
        }
    }
    found
}

/// Each row of `scores.tsv` beside what the server's scorer gave its pair.
fn reproduce_tsv(stamped: &str, compared: &[(Cells, Option<Cells>)]) -> String {
    let mut out = format!(
        "{stamped}\nrow\ta\tb\ta_slots\tb_slots\tlong_paired\tV\t\
         got_a\tgot_b\tgot_a_slots\tgot_b_slots\tgot_long_paired\tgot_V\tequal\n"
    );
    for (row, (expected, got)) in compared.iter().enumerate() {
        let got_line = got
            .as_ref()
            .map_or_else(|| ["-"; 6].join("\t"), Cells::line);
        let equal = if got.as_ref() == Some(expected) {
            "yes"
        } else {
            "no"
        };
        let _ = writeln!(out, "{}\t{}\t{got_line}\t{equal}", row + 1, expected.line());
    }
    out
}

fn summary_text(
    stamped: &str,
    (equal, rows): (usize, usize),
    unlisted: &[Key],
    bar: f64,
    digest: &ByDigest,
) -> String {
    let mut out = format!("{stamped}\nequal: {equal} of {rows}\n");
    let _ = writeln!(out, "pairs of members no row names: {}", unlisted.len());
    for (x, y) in unlisted {
        let _ = writeln!(out, "  {x}\t{y}");
    }
    let crossing = digest.moved.iter().filter(|moved| moved.crosses).count();
    let _ = write!(
        out,
        "\nthe pairs of equal slot counts, keyed by digest as the server keys them: {}\n\
         the digests put the other side first: {}\n\
         V differs: {}, crossing the bar {bar}: {crossing}\n",
        digest.pairs,
        digest.other_side_first,
        digest.moved.len()
    );
    if !digest.moved.is_empty() {
        out.push_str("a\tb\tdigest_a\tdigest_b\tV_by_path\tV_by_digest\tcrosses\n");
    }
    for moved in &digest.moved {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            moved.by_path.a,
            moved.by_path.b,
            moved.digests.0,
            moved.digests.1,
            moved.by_path.v,
            moved.by_digest,
            if moved.crosses { "yes" } else { "no" }
        );
    }
    out
}

#[test]
#[ignore = "reads every O340 member from the local test library; run by hand"]
fn the_server_scorer_gives_every_o340_pair_its_score() {
    let library = path_in("TAIYAKU_O340_LIBRARY", Path::is_dir);
    let scores = path_in("TAIYAKU_O340_SCORES", Path::is_file);
    let out = path_in("TAIYAKU_O340_OUT", Path::is_dir);
    let text =
        std::fs::read_to_string(&scores).unwrap_or_else(|e| panic!("{}: {e}", scores.display()));
    let (_, rows) = read_scores(&text).unwrap_or_else(|why| panic!("{why}"));
    let bar = (read_frozen(include_str!("bar.json")))
        .unwrap_or_else(|why| panic!("{why}"))
        .bar;

    let members = committed();
    let slots: Vec<Slots> = (members.iter())
        .map(|member| slots_of(&library, member))
        .collect();
    let compared = compare(&rows, by_path(&members, &slots));
    let equal = (compared.rows.iter())
        .filter(|(expected, got)| got.as_ref() == Some(expected))
        .count();
    let digest = by_digest(&library, &members, &slots, bar);

    let stamped = stamp(TEST);
    put(
        &out.join("reproduce.tsv"),
        &reproduce_tsv(&stamped, &compared.rows),
    );
    let summary = summary_text(
        &stamped,
        (equal, rows.len()),
        &compared.unlisted,
        bar,
        &digest,
    );
    put(&out.join("summary.txt"), &summary);
    println!("{summary}");

    assert!(
        compared.unlisted.is_empty(),
        "{} pairs of members are in no row of {}",
        compared.unlisted.len(),
        scores.display()
    );
    assert_eq!(
        equal,
        rows.len(),
        "rows of {} the server's scorer does not reproduce: see reproduce.tsv",
        scores.display()
    );
}
