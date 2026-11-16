//! How well two editions of a book align, measured against keys.
//!
//! The measurement design's §3 and §7
//! (`docs/superpowers/specs/2026-09-30-book-alignment-measurement-design.md`).
//! Every pair under `$TAIYAKU_CORPUS/books/` is aligned whole, at
//! each setting in [`SETTINGS`], and scored three ways where it can be:
//!
//! - against each hand-made key, `key*.json`, on the stretch it covers;
//! - against **chapter anchors**, a free key read off chapter headings where a
//!   pair's headings carry numbers ([`chapter_anchors`]);
//! - and, for every pair, by how many blocks the alignment left one-sided.
//!
//! The whole book is aligned, not the keyed chapter, because the aligner must
//! find the chapter without being told where it is.
//!
//! **It asserts nothing about quality.** The tables are read and the decisions
//! recorded in `docs/notes.md`. What it does assert are the instrument's
//! controls (§8): each key scored against itself is perfect, the key shifted by
//! one block scores lower on every metric, and chapter anchors run in order on
//! both sides.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli book_experiment -- --nocapture
//! ```
//!
//! **The key's format.** `{"a": "<file>", "b": "<file>", "links": [{"a": [ids],
//! "b": [ids]}, ...]}`, each id a block's `document#ordinal`, each group a run
//! of consecutive blocks, and an empty group text the other side does not have.
//! It holds ids, never text.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::Path,
    time::Instant,
};

use serde::Deserialize;
use taiyaku_core::{
    AnchorScorer, Anchors, BlockLink, BlockParams, BlockScorer, GALE_CHURCH_VARIANCE, LengthScorer,
    align_blocks, english_number,
};
use taiyaku_read::{Block, read_book};

use crate::corpus::corpus;

/// One way of aligning a pair. [`SETTINGS`]' first entry is the baseline, and
/// most others change one thing from it, so a difference has one cause. A
/// one-sided run's length and its cost per block change together, being one
/// feature, and the two entries named for several things combine settings
/// each tried alone above them.
struct Setting {
    name: &'static str,
    scorer: Scorer,
    max_group: usize,
    skip_cost: f64,
    /// One-sided runs: the most blocks one step may leave out, and what each
    /// block past the first costs.
    max_skip: usize,
    skip_extend: f64,
    variance: f64,
    band: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Scorer {
    Length,
    Anchors,
    /// Anchors, counting a number word in running text too, where
    /// [`Anchors::of`] counts only one set in capitals.
    AnchorsWithNumberWords,
    Sentences,
    /// Shared words: only for a pair in one language, where it is skipped
    /// otherwise, since two languages share no words to count.
    Words,
}

const BASELINE: Setting = Setting {
    name: "baseline",
    scorer: Scorer::Length,
    max_group: 4,
    skip_cost: 4.5,
    max_skip: 1,
    skip_extend: 4.5,
    variance: GALE_CHURCH_VARIANCE,
    band: 400,
};

const SETTINGS: [Setting; 16] = [
    BASELINE,
    Setting {
        name: "groups ≤ 3",
        max_group: 3,
        ..BASELINE
    },
    Setting {
        name: "groups ≤ 5",
        max_group: 5,
        ..BASELINE
    },
    Setting {
        name: "variance 3.4",
        variance: 3.4,
        ..BASELINE
    },
    Setting {
        name: "variance 13.6",
        variance: 13.6,
        ..BASELINE
    },
    Setting {
        name: "skip 9",
        skip_cost: 9.0,
        ..BASELINE
    },
    Setting {
        name: "+ anchors",
        scorer: Scorer::Anchors,
        ..BASELINE
    },
    Setting {
        name: "+ anchors, every number word",
        scorer: Scorer::AnchorsWithNumberWords,
        ..BASELINE
    },
    Setting {
        name: "+ sentences",
        scorer: Scorer::Sentences,
        ..BASELINE
    },
    Setting {
        name: "+ shared words",
        scorer: Scorer::Words,
        ..BASELINE
    },
    Setting {
        name: "band 100",
        band: 100,
        ..BASELINE
    },
    Setting {
        name: "anchors, skip 9, variance 13.6",
        scorer: Scorer::Anchors,
        skip_cost: 9.0,
        variance: 13.6,
        ..BASELINE
    },
    Setting {
        name: "runs ≤ 32, extend 0.5",
        max_skip: 32,
        skip_extend: 0.5,
        ..BASELINE
    },
    Setting {
        name: "runs ≤ 32, extend 2",
        max_skip: 32,
        skip_extend: 2.0,
        ..BASELINE
    },
    Setting {
        name: "runs ≤ 32, extend 3",
        max_skip: 32,
        skip_extend: 3.0,
        ..BASELINE
    },
    Setting {
        name: "anchors + runs ≤ 32, extend 3",
        scorer: Scorer::Anchors,
        max_skip: 32,
        skip_extend: 3.0,
        ..BASELINE
    },
];

#[derive(Deserialize)]
struct Key {
    a: String,
    b: String,
    links: Vec<KeyLink>,
}

#[derive(Deserialize)]
struct KeyLink {
    a: Vec<String>,
    b: Vec<String>,
}

#[test]
fn book_experiment() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let Ok(pairs) = std::fs::read_dir(root.join("books")) else {
        eprintln!("{} has no books/; skipping", root.display());
        return;
    };
    let mut pairs: Vec<_> = pairs
        .map(|pair| pair.expect("pair").path())
        .filter(|pair| pair.is_dir())
        .collect();
    pairs.sort();
    assert!(!pairs.is_empty(), "books/ holds no pair");
    for pair in pairs {
        measure(&pair);
    }
}

/// A pair's two books, A and B: the first two EPUBs by name, or the two its
/// keys name.
fn books(pair: &Path, keys: &[(String, Key)]) -> (String, String) {
    if let Some((_, key)) = keys.first() {
        return (key.a.clone(), key.b.clone());
    }
    let mut epubs: Vec<String> = std::fs::read_dir(pair)
        .expect("pair reads")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".epub"))
        .collect();
    epubs.sort();
    assert_eq!(epubs.len(), 2, "{} must hold two EPUBs", pair.display());
    (epubs[0].clone(), epubs[1].clone())
}

fn measure(pair: &Path) {
    let mut keys: Vec<(String, Key)> = std::fs::read_dir(pair)
        .expect("pair reads")
        .map(|e| e.expect("entry").path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            name.starts_with("key") && name.ends_with(".json")
        })
        .map(|p| {
            let key = serde_json::from_str(&std::fs::read_to_string(&p).expect("key reads"))
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            (
                p.file_stem().expect("stem").to_string_lossy().into_owned(),
                key,
            )
        })
        .collect();
    keys.sort_by(|x, y| x.0.cmp(&y.0));
    let (a_name, b_name) = books(pair, &keys);
    let a_book = read_book(&pair.join(&a_name)).expect("book a reads");
    let b_book = read_book(&pair.join(&b_name)).expect("book b reads");
    let (a, b) = (&a_book.blocks, &b_book.blocks);
    let same_language =
        language(a_book.language.as_deref()) == language(b_book.language.as_deref());
    let pair_name = pair
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();

    println!(
        "\n## {pair_name}: {a_name} ({}) against {b_name} ({})",
        a.len(),
        b.len()
    );

    let truths: Vec<(String, Vec<BlockLink>)> = keys
        .iter()
        .map(|(name, key)| {
            assert_eq!(
                (&key.a, &key.b),
                (&a_name, &b_name),
                "{name} names other books"
            );
            (name.clone(), truth_links(key, a, b))
        })
        .collect();
    for (name, truth) in &truths {
        let perfect = Scores::of(truth, truth);
        let shifted = Scores::of(&shift_by_one(truth, b.len()), truth);
        println!(
            "{name}: {} links, {}; shifted by one: {shifted}",
            truth.len(),
            group_sizes(truth)
        );
        assert!(
            perfect.is_perfect(),
            "{name} against itself is not perfect: {perfect}"
        );
        assert!(
            shifted.is_lower_everywhere_than(&perfect),
            "{name}: a metric did not drop when shifted"
        );
    }
    let anchors = chapter_anchors(&pair_name, a, b);
    if !anchors.is_empty() {
        println!("chapter anchors: {}", anchors.len());
    }

    let lengths = |blocks: &[Block]| -> Vec<u32> {
        blocks
            .iter()
            .map(|block| u32::try_from(block.text.chars().count()).unwrap_or(u32::MAX))
            .collect()
    };
    let (a_lengths, b_lengths) = (lengths(a), lengths(b));

    let mut header = String::from("| setting |");
    for (name, _) in &truths {
        header.push_str(&format!(" {name} exact | ±1 |"));
    }
    if !anchors.is_empty() {
        header.push_str(" chapters exact | worst |");
    }
    header.push_str(" one-sided | seconds |");
    println!(
        "\n{header}\n|{}",
        "---|".repeat(header.matches('|').count() - 1)
    );

    for setting in &SETTINGS {
        if setting.scorer == Scorer::Words && !same_language {
            continue;
        }
        let length = LengthScorer::new(&a_lengths, &b_lengths, setting.variance);
        let params = BlockParams {
            max_group: setting.max_group,
            skip_cost: setting.skip_cost,
            max_skip: setting.max_skip,
            skip_extend: setting.skip_extend,
            band: setting.band,
        };
        let started = Instant::now();
        let links = match setting.scorer {
            Scorer::Length => align_blocks(a.len(), b.len(), &length, params),
            Scorer::Anchors | Scorer::AnchorsWithNumberWords => {
                let running_text = setting.scorer == Scorer::AnchorsWithNumberWords;
                let of = |blocks: &[Block]| -> Vec<Anchors> {
                    blocks
                        .iter()
                        .map(|block| {
                            let mut anchors = Anchors::of(&block.text);
                            if running_text {
                                anchors.numbers += lower_case_number_words(&block.text);
                            }
                            anchors
                        })
                        .collect()
                };
                let scorer = AnchorScorer::new(length, &of(a), &of(b), 1.0);
                align_blocks(a.len(), b.len(), &scorer, params)
            }
            Scorer::Sentences => {
                let scorer = Counted::new(length, a, b, sentences, 1.0);
                align_blocks(a.len(), b.len(), &scorer, params)
            }
            Scorer::Words => {
                let scorer = SharedWords::new(length, a, b, 4.0);
                align_blocks(a.len(), b.len(), &scorer, params)
            }
        };
        let seconds = started.elapsed().as_secs_f64();

        let mut row = format!("| {} |", setting.name);
        for (_, truth) in &truths {
            let s = Scores::of(&links, truth);
            row.push_str(&format!(
                " {} | {} |",
                percent(s.landing[0]),
                percent(s.landing[1])
            ));
        }
        if !anchors.is_empty() {
            let (exact, worst) = anchor_landing(&links, &anchors);
            row.push_str(&format!(" {} | {worst} |", percent(exact)));
        }
        let one_sided = links
            .iter()
            .filter(|l| l.a.is_empty() || l.b.is_empty())
            .map(|l| l.a.len() + l.b.len())
            .sum::<usize>();
        row.push_str(&format!(
            " {} | {seconds:.1} |",
            percent(share(one_sided, a.len() + b.len()))
        ));
        println!("{row}");
    }
}

/// A language tag's primary subtag, lower-cased: `en-US` and `en` are one language.
fn language(tag: Option<&str>) -> Option<String> {
    tag.map(|t| t.split('-').next().unwrap_or(t).to_ascii_lowercase())
}

fn percent(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

/// The key's links as block ranges, checked to be runs of consecutive blocks.
fn truth_links(key: &Key, a: &[Block], b: &[Block]) -> Vec<BlockLink> {
    let index = |blocks: &[Block]| -> HashMap<String, usize> {
        blocks
            .iter()
            .enumerate()
            .map(|(i, block)| (format!("{}#{}", block.id.document, block.id.ordinal), i))
            .collect()
    };
    let (a_index, b_index) = (index(a), index(b));
    let range = |ids: &[String], index: &HashMap<String, usize>, at: usize| -> Range<usize> {
        let positions: Vec<usize> = ids
            .iter()
            .map(|id| {
                *index
                    .get(id)
                    .unwrap_or_else(|| panic!("the key names `{id}`, which the book lacks"))
            })
            .collect();
        let Some(&start) = positions.first() else {
            return at..at;
        };
        assert!(
            positions.windows(2).all(|w| w[1] == w[0] + 1),
            "a key group is not a run of consecutive blocks: {ids:?}"
        );
        start..start + positions.len()
    };
    let mut links: Vec<BlockLink> = Vec::with_capacity(key.links.len());
    for link in &key.links {
        let (next_a, next_b) = links.last().map_or((0, 0), |l| (l.a.end, l.b.end));
        let a_range = range(&link.a, &a_index, next_a);
        let b_range = range(&link.b, &b_index, next_b);
        links.push(BlockLink {
            a: a_range,
            b: b_range,
        });
    }
    links
}

fn group_sizes(truth: &[BlockLink]) -> String {
    let mut sizes: Vec<((usize, usize), usize)> = Vec::new();
    for link in truth {
        let size = (link.a.len(), link.b.len());
        match sizes.iter_mut().find(|(s, _)| *s == size) {
            Some((_, n)) => *n += 1,
            None => sizes.push((size, 1)),
        }
    }
    sizes.sort_unstable();
    let shown: Vec<String> = sizes
        .iter()
        .map(|((x, y), n)| format!("{x}:{y} ×{n}"))
        .collect();
    shown.join(", ")
}

/// The key with every B group moved one block later: what a misalignment by one
/// paragraph looks like.
fn shift_by_one(truth: &[BlockLink], b_len: usize) -> Vec<BlockLink> {
    truth
        .iter()
        .map(|l| BlockLink {
            a: l.a.clone(),
            b: (l.b.start + 1).min(b_len)..(l.b.end + 1).min(b_len),
        })
        .collect()
}

/// §7's figures for one alignment, on the stretch the key covers.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Scores {
    /// Shares of keyed blocks, both sides together, whose swap lands inside
    /// the true counterpart group, or so many blocks from it, or on nothing.
    landing: [f64; 6],
    strict_f1: f64,
    lax_f1: f64,
}

impl Scores {
    fn of(links: &[BlockLink], truth: &[BlockLink]) -> Self {
        let mut buckets = [0usize; 6];
        let mut total = 0usize;
        let a_owner = owners(links, |l| &l.a);
        let b_owner = owners(links, |l| &l.b);
        for t in truth {
            for x in t.a.clone() {
                buckets[a_owner.get(&x).map_or(5, |&n| bucket(&links[n].b, &t.b))] += 1;
                total += 1;
            }
            for x in t.b.clone() {
                buckets[b_owner.get(&x).map_or(5, |&n| bucket(&links[n].a, &t.a))] += 1;
                total += 1;
            }
        }
        let landing = buckets.map(|n| share(n, total));

        let a_span = truth.first().map_or(0, |l| l.a.start)..truth.last().map_or(0, |l| l.a.end);
        let b_span = truth.first().map_or(0, |l| l.b.start)..truth.last().map_or(0, |l| l.b.end);
        let scoped: Vec<&BlockLink> = links
            .iter()
            .filter(|l| meets(&l.a, &a_span) || meets(&l.b, &b_span))
            .collect();
        let strict = |p: &BlockLink, t: &BlockLink| p == t;
        let lax = |p: &BlockLink, t: &BlockLink| meets(&p.a, &t.a) && meets(&p.b, &t.b);
        Self {
            landing,
            strict_f1: f1(&scoped, truth, strict),
            lax_f1: f1(&scoped, truth, lax),
        }
    }

    fn is_perfect(&self) -> bool {
        (self.landing[0] - 1.0).abs() < 1e-12
            && (self.strict_f1 - 1.0).abs() < 1e-12
            && (self.lax_f1 - 1.0).abs() < 1e-12
    }

    fn is_lower_everywhere_than(&self, other: &Self) -> bool {
        self.landing[0] < other.landing[0]
            && self.strict_f1 < other.strict_f1
            && self.lax_f1 < other.lax_f1
    }
}

impl std::fmt::Display for Scores {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "exact {}, strict F1 {}, lax F1 {}",
            percent(self.landing[0]),
            percent(self.strict_f1),
            percent(self.lax_f1)
        )
    }
}

fn owners(
    links: &[BlockLink],
    side: impl Fn(&BlockLink) -> &Range<usize>,
) -> HashMap<usize, usize> {
    links
        .iter()
        .enumerate()
        .flat_map(|(n, l)| side(l).clone().map(move |x| (x, n)))
        .collect()
}

/// Where a swap from a keyed block lands, against where it should: 0 inside the
/// true group, 1 one block off, 2 two or three, 3 four to ten, 4 more, and 5 on
/// nothing, when the alignment pairs the block with no counterpart, puts it
/// in no link, or pairs it when the key says it has no counterpart.
fn bucket(landed: &Range<usize>, wanted: &Range<usize>) -> usize {
    if landed.is_empty() || wanted.is_empty() {
        return if landed.is_empty() && wanted.is_empty() {
            0
        } else {
            5
        };
    }
    match distance(landed.start, wanted) {
        0 => 0,
        1 => 1,
        2..=3 => 2,
        4..=10 => 3,
        _ => 4,
    }
}

/// How many blocks `at` sits outside `wanted`, 0 inside it.
fn distance(at: usize, wanted: &Range<usize>) -> usize {
    if at < wanted.start {
        wanted.start - at
    } else {
        (at + 1).saturating_sub(wanted.end)
    }
}

/// Whether two ranges share a block, or are both empty.
fn meets(x: &Range<usize>, y: &Range<usize>) -> bool {
    if x.is_empty() || y.is_empty() {
        return x.is_empty() && y.is_empty();
    }
    x.start < y.end && y.start < x.end
}

fn f1(
    predicted: &[&BlockLink],
    truth: &[BlockLink],
    matches: impl Fn(&BlockLink, &BlockLink) -> bool,
) -> f64 {
    let hit_p = predicted
        .iter()
        .filter(|p| truth.iter().any(|t| matches(p, t)))
        .count();
    let hit_t = truth
        .iter()
        .filter(|t| predicted.iter().any(|p| matches(p, t)))
        .count();
    let (precision, recall) = (share(hit_p, predicted.len()), share(hit_t, truth.len()));
    if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

fn share(n: usize, of: usize) -> f64 {
    let to_f64 = |x: usize| f64::from(u32::try_from(x).unwrap_or(u32::MAX));
    if of == 0 { 0.0 } else { to_f64(n) / to_f64(of) }
}

/// Of the anchors' blocks, both sides together, the share whose swap lands
/// exactly on the other side's anchor block, and the farthest any lands.
fn anchor_landing(links: &[BlockLink], anchors: &[(usize, usize)]) -> (f64, String) {
    let a_owner = owners(links, |l| &l.a);
    let b_owner = owners(links, |l| &l.b);
    let mut exact = 0;
    let mut worst: Option<usize> = Some(0);
    let mut land =
        |owner: Option<&usize>, side: fn(&BlockLink) -> &Range<usize>, want: usize| match owner
            .map(|&n| side(&links[n]))
            .filter(|r| !r.is_empty())
        {
            Some(r) => {
                let off = distance(r.start, &(want..want + 1));
                exact += usize::from(off == 0);
                worst = worst.map(|w| w.max(off));
            }
            None => worst = None,
        };
    for &(i, j) in anchors {
        land(a_owner.get(&i), |l| &l.b, j);
        land(b_owner.get(&j), |l| &l.a, i);
    }
    let worst = worst.map_or_else(|| "unpaired".to_string(), |w| w.to_string());
    (share(exact, 2 * anchors.len()), worst)
}

/// A free key from numbered chapter headings: (A's block, B's block) a chapter.
///
/// A contents page is dropped first ([`without_contents`]); a preview of the
/// next book can still repeat chapter one, so each number is paired nearest
/// first by place in the book as a share of its blocks, and the result must run
/// in order on both sides. Matching is on text, never on heading markup, since
/// one Korean edition sets its chapter titles in a plain paragraph.
fn chapter_anchors(pair: &str, a: &[Block], b: &[Block]) -> Vec<(usize, usize)> {
    // Each pair sets its headings its own way, so its directory under
    // `corpus/books/` (the measurement design's §2 lists the pairs) picks the
    // reader, and any other pair has no chapter anchors.
    let (a_marks, b_marks) = if pair.starts_with("harry-potter") {
        (marks(a, english_chapter), marks(b, korean_chapter))
    } else if pair == "crime-and-punishment" {
        (part_and_chapter(a), part_and_chapter(b))
    } else {
        return Vec::new();
    };
    let (a_marks, b_marks) = (without_contents(a_marks), without_contents(b_marks));
    let place = |i: usize, len: usize| share(i, len);
    let mut anchors = Vec::new();
    for (mark, a_at) in &a_marks {
        let Some(b_at) = b_marks.get(mark) else {
            continue;
        };
        let mut pairs: Vec<(f64, usize, usize)> = a_at
            .iter()
            .flat_map(|&x| b_at.iter().map(move |&y| (x, y)))
            .map(|(x, y)| ((place(x, a.len()) - place(y, b.len())).abs(), x, y))
            .collect();
        pairs.sort_by(|p, q| p.0.total_cmp(&q.0));
        let (mut used_a, mut used_b) = (HashSet::new(), HashSet::new());
        for (_, x, y) in pairs {
            if !used_a.contains(&x) && !used_b.contains(&y) {
                used_a.insert(x);
                used_b.insert(y);
                anchors.push((x, y));
            }
        }
    }
    anchors.sort_unstable();
    assert!(
        anchors.windows(2).all(|w| w[0].1 < w[1].1),
        "{pair}: chapter anchors do not run in order on both sides"
    );
    anchors
}

/// The most blocks between two entries of one contents page.
const CONTENTS_GAP: usize = 3;

/// `marks` without its contents pages: runs of two or more markers, each within
/// [`CONTENTS_GAP`] blocks of the last and numbered after it. A real chapter one
/// right after a contents page that ends at seventeen starts a run of its own.
fn without_contents(marks: HashMap<(u32, u32), Vec<usize>>) -> HashMap<(u32, u32), Vec<usize>> {
    let mut all: Vec<(usize, (u32, u32))> = marks
        .iter()
        .flat_map(|(mark, at)| at.iter().map(move |&i| (i, *mark)))
        .collect();
    all.sort_unstable();
    let mut contents = HashSet::new();
    let mut run: Vec<usize> = Vec::new();
    for (n, &(i, mark)) in all.iter().enumerate() {
        let continues = n > 0 && i - all[n - 1].0 <= CONTENTS_GAP && mark > all[n - 1].1;
        if !continues {
            if run.len() >= 2 {
                contents.extend(run.drain(..));
            }
            run.clear();
        }
        run.push(i);
    }
    if run.len() >= 2 {
        contents.extend(run);
    }
    marks
        .into_iter()
        .map(|(mark, at)| {
            (
                mark,
                at.into_iter().filter(|i| !contents.contains(i)).collect(),
            )
        })
        .collect()
}

fn marks(blocks: &[Block], read: fn(&str) -> Option<u32>) -> HashMap<(u32, u32), Vec<usize>> {
    let mut found: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, block) in blocks.iter().enumerate() {
        if let Some(n) = read(&block.text) {
            found.entry((0, n)).or_default().push(i);
        }
    }
    found
}

fn lower_case_number_words(text: &str) -> u32 {
    let words = text
        .split(|c: char| !(c.is_ascii_alphabetic() || c == '-'))
        .filter(|word| word.chars().any(|c| c.is_ascii_lowercase()))
        .filter(|word| english_number(word).is_some())
        .count();
    u32::try_from(words).unwrap_or(u32::MAX)
}

/// `CHAPTER` and an English number word, as [`english_number`] reads one.
fn english_chapter(text: &str) -> Option<u32> {
    english_number(text.strip_prefix("CHAPTER ")?.trim())
}

/// `제 1 장`, `제14 장`, or `13장` opening a title paragraph.
fn korean_chapter(text: &str) -> Option<u32> {
    let rest = text.strip_prefix('제').unwrap_or(text).trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = rest[digits.len()..].trim_start();
    let tail = after.strip_prefix('장')?;
    if !(tail.is_empty() || tail.starts_with(char::is_whitespace)) {
        return None;
    }
    digits.parse().ok()
}

/// `PART THREE` or `Part Three` sets the part, and `Epilogue` a seventh, after
/// the novel's six; a block that is only a Roman numeral is that chapter of it.
fn part_and_chapter(blocks: &[Block]) -> HashMap<(u32, u32), Vec<usize>> {
    const PARTS: [&str; 6] = ["one", "two", "three", "four", "five", "six"];
    const NUMERALS: [&str; 12] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
    ];
    let mut part = None;
    let mut found: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, block) in blocks.iter().enumerate() {
        let text = block.text.trim();
        let lower = text.to_ascii_lowercase();
        if let Some(word) = lower.strip_prefix("part ")
            && let Some(n) = PARTS.iter().position(|p| *p == word)
        {
            part = u32::try_from(n + 1).ok();
            continue;
        }
        if lower == "epilogue" {
            part = Some(7);
            continue;
        }
        if let (Some(p), Some(n)) = (part, NUMERALS.iter().position(|r| *r == text)) {
            found
                .entry((p, u32::try_from(n + 1).unwrap_or(0)))
                .or_default()
                .push(i);
        }
    }
    found
}

/// Sentence ends: `.`, `?` or `!`, once per run, followed by a space, a closing
/// quotation mark or the end. An ellipsis is not one.
fn sentences(text: &str) -> u32 {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0;
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '.' | '?' | '!') {
            let start = i;
            while i + 1 < chars.len() && matches!(chars[i + 1], '.' | '?' | '!') {
                i += 1;
            }
            let ellipsis = i > start && chars[start] == '.';
            let after = chars.get(i + 1);
            if !ellipsis
                && after.is_none_or(|c| c.is_whitespace() || matches!(c, '”' | '’' | '"' | '\''))
            {
                count += 1;
            }
        }
        i += 1;
    }
    count
}

/// Length, plus `weight` per unit of disagreement in a count both groups carry.
struct Counted {
    length: LengthScorer,
    a: Vec<u32>,
    b: Vec<u32>,
    weight: f64,
}

impl Counted {
    fn new(
        length: LengthScorer,
        a: &[Block],
        b: &[Block],
        count: fn(&str) -> u32,
        weight: f64,
    ) -> Self {
        Self {
            length,
            a: a.iter().map(|x| count(&x.text)).collect(),
            b: b.iter().map(|x| count(&x.text)).collect(),
            weight,
        }
    }
}

impl BlockScorer for Counted {
    fn cost(&self, a: Range<usize>, b: Range<usize>) -> f64 {
        let (x, y): (u32, u32) = (
            self.a[a.clone()].iter().sum(),
            self.b[b.clone()].iter().sum(),
        );
        self.length.cost(a, b) + self.weight * f64::from(x.abs_diff(y))
    }
}

/// The sorted, deduplicated union of a group's word hashes.
fn union(sets: &[Vec<u64>]) -> Vec<u64> {
    let mut all: Vec<u64> = sets.iter().flatten().copied().collect();
    all.sort_unstable();
    all.dedup();
    all
}

/// How many values two sorted, deduplicated lists share.
fn common(x: &[u64], y: &[u64]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < x.len() && j < y.len() {
        match x[i].cmp(&y[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => (n, i, j) = (n + 1, i + 1, j + 1),
        }
    }
    n
}

/// Length, plus `weight` times one minus the Dice overlap of the two groups'
/// words of four letters or more, lower-cased: two translations in one language
/// share their names and nouns.
struct SharedWords {
    length: LengthScorer,
    a: Vec<Vec<u64>>,
    b: Vec<Vec<u64>>,
    weight: f64,
}

impl SharedWords {
    fn new(length: LengthScorer, a: &[Block], b: &[Block], weight: f64) -> Self {
        let words = |blocks: &[Block]| -> Vec<Vec<u64>> {
            blocks
                .iter()
                .map(|x| {
                    let hashes: Vec<u64> = x
                        .text
                        .split(|c: char| !c.is_alphanumeric())
                        .filter(|w| w.chars().count() >= 4)
                        .map(|w| {
                            let mut hasher = std::hash::DefaultHasher::new();
                            std::hash::Hash::hash(&w.to_lowercase(), &mut hasher);
                            std::hash::Hasher::finish(&hasher)
                        })
                        .collect();
                    union(&[hashes])
                })
                .collect()
        };
        Self {
            length,
            a: words(a),
            b: words(b),
            weight,
        }
    }
}

impl BlockScorer for SharedWords {
    fn cost(&self, a: Range<usize>, b: Range<usize>) -> f64 {
        let (x, y) = (union(&self.a[a.clone()]), union(&self.b[b.clone()]));
        let both = common(&x, &y);
        let dice = if x.is_empty() && y.is_empty() {
            1.0
        } else {
            2.0 * share(both, x.len() + y.len())
        };
        self.length.cost(a, b) + self.weight * (1.0 - dice)
    }
}
