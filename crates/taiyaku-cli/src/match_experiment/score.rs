//! A split's pairs scored and judged against the rules.
//!
//! Every pair [`scored_pairs`] names for the split, the explicit pairs and
//! each member's top leaders from `leaders.json`, is aligned on its oriented
//! slots and written to `scores.tsv`. Each rule's report goes to
//! `rules.txt`, beside the instrument's controls, the omnibus pairs and the
//! lookup's predictions on the manifest as adjudicated. The pairs a person
//! must judge go to `adjudicate-N.json`, with the pages the alignment
//! paired, which [`render`] sets out with `pairing_sheet`.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use taiyaku_core::{PageHash, Run, Slot, Verdict};

use super::features::{PairFeatures, aligned, features, oriented};
use super::lookup::{LEADERS, Lookup, Print, at, print_member, query_report};
use super::manifest::{
    Kind, Label, Manifest, Member, Source, Split, class, explicit_pairs, held, is_omnibus_name,
    label, scored_pairs, split_of, verdict_on,
};
use super::rule::{Rule, SHOWN_NEGATIVE, ScoredRow, dev_outcome, feature_line, label_name, report};
use super::sheet::{Question, SheetPair, pairing_sheet};
use super::{put, stamp};

pub(super) const RULES: [Rule; 3] = [Rule::Volume, Rule::Chapter, Rule::Pack];

/// The most pairs one round of judging holds.
pub(super) const ROUND: usize = 40;

pub(super) fn split_name(split: Split) -> &'static str {
    match split {
        Split::Dev => "dev",
        Split::HeldOut => "held-out",
    }
}

fn index_u32(i: usize) -> u32 {
    u32::try_from(i).expect("a library's member count fits u32")
}

/// What `leaders.json` gives scoring, by manifest position: each member's
/// top [`LEADERS`] as pairs with it, and every looked-up member's votes as
/// far as the file keeps them.
#[derive(Debug)]
pub(super) struct Leaders {
    pub stamp: String,
    pub pairs: Vec<(usize, usize)>,
    pub lookups: Vec<Option<Lookup>>,
}

/// `leaders.json` read against `m`. A path the manifest does not hold is
/// refused: the file is from another manifest, and its pairs would be some
/// other measurement's.
pub(super) fn read_leaders(text: &str, m: &Manifest) -> Result<Leaders, String> {
    #[derive(Deserialize)]
    struct Entry {
        informative: u32,
        leaders: Vec<(String, u32)>,
    }
    #[derive(Deserialize)]
    struct File {
        stamp: String,
        members: BTreeMap<String, Entry>,
    }

    let file: File = serde_json::from_str(text).map_err(|e| format!("leaders.json: {e}"))?;
    let position: HashMap<&str, usize> = (m.members.iter().enumerate())
        .map(|(i, x)| (x.path.as_str(), i))
        .collect();
    let find = |path: &str| {
        (position.get(path).copied())
            .ok_or_else(|| format!("leaders.json names {path}, which the manifest does not hold"))
    };
    let mut pairs = Vec::new();
    let mut lookups = vec![None; m.members.len()];
    for (path, entry) in &file.members {
        let query = find(path)?;
        let votes = (entry.leaders.iter())
            .map(|(other, n)| find(other).map(|j| (index_u32(j), *n)))
            .collect::<Result<Vec<_>, _>>()?;
        let top = votes.iter().take(LEADERS);
        pairs.extend(top.map(|&(j, _)| (query, at(j))));
        lookups[query] = Some(Lookup {
            informative: entry.informative,
            votes,
        });
    }
    Ok(Leaders {
        stamp: file.stamp,
        pairs,
        lookups,
    })
}

/// One pair aligned: `a` is the oriented short side, by manifest position.
#[derive(Debug)]
pub(super) struct Scored {
    pub a: usize,
    pub b: usize,
    pub source: Source,
    pub label: Label,
    pub features: PairFeatures,
    pub runs: usize,
    pub ms: f64,
}

impl Scored {
    pub(super) fn score(&self, rule: Rule) -> f64 {
        match rule {
            Rule::Volume => self.features.volume,
            Rule::Chapter | Rule::Pack => self.features.containment,
        }
    }
}

/// Every pair whose two members printed, oriented and aligned one at a time
/// so each pair's milliseconds are its own, and the pairs one of whose
/// members did not print.
pub(super) fn score_pairs(
    m: &Manifest,
    prints: &[Option<Print>],
    pairs: &[((usize, usize), Source)],
) -> (Vec<Scored>, Vec<(usize, usize)>) {
    let (mut scored, mut unscored) = (Vec::new(), Vec::new());
    for &(pair, source) in pairs {
        let (Some(one), Some(other)) = (&prints[pair.0], &prints[pair.1]) else {
            unscored.push(pair);
            continue;
        };
        let path = |k: usize| m.members[k].path.as_str();
        let [first, _] = oriented(
            (path(pair.0), one.slots.as_slice()),
            (path(pair.1), other.slots.as_slice()),
        );
        let (short, long) = if first.0 == path(pair.0) {
            ((pair.0, one), (pair.1, other))
        } else {
            ((pair.1, other), (pair.0, one))
        };
        let started = Instant::now();
        let (features, runs) = aligned(&short.1.slots, &long.1.slots);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        scored.push(Scored {
            a: short.0,
            b: long.0,
            source,
            label: label(m, &m.members[short.0], &m.members[long.0]),
            features,
            runs: runs.len(),
            ms,
        });
    }
    (scored, unscored)
}

/// Whether a pair meets a rule's gates, the kind and slot gates `class`
/// applies. They are asked of `class` itself with a label the rule counts,
/// so an unlabelled pair meets exactly the gates a labelled one does.
pub(super) fn in_gates(rule: Rule, a: &Member, b: &Member, f: &PairFeatures) -> bool {
    let counted = match rule {
        Rule::Volume => Label::SameVolume { copy: false },
        Rule::Chapter => Label::ChapterIn,
        Rule::Pack => Label::InPack,
    };
    class(rule, counted, a, b, f).is_some()
}

/// The sub-class a pair's positives are reported under: for volumes by
/// edition, a copy before a fan or colour edition before raw against EN;
/// for chapters A's series, an issue being the comics; packs as one.
pub(super) fn sub_class(rule: Rule, label: Label, a: &Member, b: &Member) -> String {
    match rule {
        Rule::Volume => {
            let either = |word: &str| a.edition.contains(word) || b.edition.contains(word);
            let raw = |x: &Member| x.edition.starts_with("raw");
            if label == (Label::SameVolume { copy: true }) {
                "copy".to_owned()
            } else if either("fan") {
                "fan".to_owned()
            } else if either("colour") {
                "colour".to_owned()
            } else if raw(a) != raw(b) {
                "raw-en".to_owned()
            } else {
                let (x, y) = (a.edition.as_str(), b.edition.as_str());
                format!("{}-{}", x.min(y), x.max(y))
            }
        }
        Rule::Chapter if a.kind == Kind::Issue => "comics".to_owned(),
        Rule::Chapter => a.series.clone(),
        Rule::Pack => "pack".to_owned(),
    }
}

/// A rule's rows: its labelled pairs and the unlabelled pairs inside its
/// gates, each beside its position in `scored`.
pub(super) fn rule_rows(rule: Rule, m: &Manifest, scored: &[Scored]) -> Vec<(usize, ScoredRow)> {
    (scored.iter().enumerate())
        .filter_map(|(n, s)| {
            let (a, b) = (&m.members[s.a], &m.members[s.b]);
            let place = class(rule, s.label, a, b, &s.features);
            let unlabelled = s.label == Label::Unlabelled && in_gates(rule, a, b, &s.features);
            (place.is_some() || unlabelled).then(|| {
                let row = ScoredRow {
                    a: a.path.clone(),
                    b: b.path.clone(),
                    label: s.label,
                    sub_class: sub_class(rule, s.label, a, b),
                    class: place,
                    score: s.score(rule),
                };
                (n, row)
            })
        })
        .collect()
}

/// `items` in a fixed pseudo-random order, the same on every run.
fn shuffle(items: &[PageHash]) -> Vec<PageHash> {
    let mut out = items.to_vec();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for i in (1..out.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bound = u64::try_from(i + 1).expect("a slot count fits u64");
        let j = usize::try_from(state % bound).expect("an index below a slot count fits usize");
        out.swap(i, j);
    }
    out
}

/// The instrument's two asserted controls on one pair: A against itself
/// scores V and C at least 0.99, and A against B shuffled scores V at most
/// 0.10. The line is `Err` when either fails, and then no rule is judged.
pub(super) fn controls(a: &[PageHash], b: &[PageHash]) -> Result<String, String> {
    control_verdict(&features(a, a), &features(a, &shuffle(b)))
}

/// The controls' bounds read on A against itself and A against B shuffled.
pub(super) fn control_verdict(
    own: &PairFeatures,
    shuffled: &PairFeatures,
) -> Result<String, String> {
    let line = format!(
        "A against itself: V {:.4}, C {:.4}; A against B shuffled: V {:.4}",
        own.volume, own.containment, shuffled.volume
    );
    if own.volume >= 0.99 && own.containment >= 0.99 && shuffled.volume <= 0.10 {
        Ok(line)
    } else {
        Err(line)
    }
}

fn class_cell(class: Option<bool>, unlabelled: bool) -> &'static str {
    match (class, unlabelled) {
        (Some(true), _) => "positive",
        (Some(false), _) => "negative",
        (None, true) => "unlabelled",
        (None, false) => "-",
    }
}

fn source_name(source: Source) -> &'static str {
    match source {
        Source::Explicit => "explicit",
        Source::Leader => "leader",
        Source::Both => "both",
    }
}

fn optional(value: Option<f32>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| format!("{v:.4}"))
}

pub(super) fn scores_tsv(stamp: &str, m: &Manifest, scored: &[Scored]) -> String {
    let mut out = format!(
        "{stamp}\na\tb\tsource\tlabel\tvolume\tchapter\tpack\ta_slots\tb_slots\tverdict\t\
         separation\tmargin\tavg_run\tpaired\truns\tlong_paired\tspan_b\tV\tC\tms\n"
    );
    for s in scored {
        let (a, b, f) = (&m.members[s.a], &m.members[s.b], &s.features);
        let classes = RULES.map(|rule| {
            let unlabelled = s.label == Label::Unlabelled && in_gates(rule, a, b, f);
            class_cell(class(rule, s.label, a, b, f), unlabelled)
        });
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:?}\t{}\t{}\t{:.2}\t{}\t{}\t{}\t{}\t{:.6}\t{:.6}\t{:.3}",
            a.path,
            b.path,
            source_name(s.source),
            label_name(s.label),
            classes.join("\t"),
            f.a_slots,
            f.b_slots,
            f.verdict,
            optional(f.separation),
            optional(f.margin),
            f.avg_run,
            f.paired,
            s.runs,
            f.long_paired,
            f.span_b,
            f.volume,
            f.containment,
            s.ms,
        );
    }
    out
}

fn verdict_rank(verdict: Verdict) -> f64 {
    match verdict {
        Verdict::Refused => 0.0,
        Verdict::Fragmented => 1.0,
        Verdict::Sound => 2.0,
    }
}

/// Each feature no rule judges on, read on a rule's labelled pairs.
pub(super) fn described(rule: Rule, scored: &[Scored], rows: &[(usize, ScoredRow)]) -> String {
    let labelled: Vec<(bool, &PairFeatures)> = (rows.iter())
        .filter_map(|(n, r)| r.class.map(|c| (c, &scored[*n].features)))
        .collect();
    let read = |value: &dyn Fn(&PairFeatures) -> Option<f64>| -> Vec<(bool, f64)> {
        (labelled.iter())
            .filter_map(|&(c, f)| value(f).map(|v| (c, v)))
            .collect()
    };
    let other: (&str, fn(&PairFeatures) -> f64) = match rule {
        Rule::Volume => ("C", |f| f.containment),
        Rule::Chapter | Rule::Pack => ("V", |f| f.volume),
    };
    let lines = [
        feature_line(
            "verdict (0 Refused, 1 Fragmented, 2 Sound)",
            &read(&|f| Some(verdict_rank(f.verdict))),
        ),
        feature_line("separation", &read(&|f| f.separation.map(f64::from))),
        feature_line("margin", &read(&|f| f.margin.map(f64::from))),
        feature_line("avg_run", &read(&|f| Some(f.avg_run))),
        feature_line(
            "paired / |A|",
            &read(&|f| Some(f64::from(f.paired) / f64::from(f.a_slots.max(1)))),
        ),
        feature_line("long_paired", &read(&|f| Some(f64::from(f.long_paired)))),
        feature_line(other.0, &read(&|f| Some((other.1)(f)))),
    ];
    format!("other features, descriptively:\n{}\n", lines.join("\n"))
}

/// The omnibus pairs with V and C, and whether `is_omnibus_name` marks
/// every omnibus member of the split and no other.
pub(super) fn omnibus(m: &Manifest, split: Split, scored: &[Scored]) -> String {
    let mut out = "## omnibus pairs, reported only\na\tb\tV\tC\n".to_owned();
    for s in scored.iter().filter(|s| s.label == Label::Omnibus) {
        let _ = writeln!(
            out,
            "{}\t{}\t{:.4}\t{:.4}",
            m.members[s.a].path, m.members[s.b].path, s.features.volume, s.features.containment
        );
    }
    let members = m.members.iter().filter(|x| x.split == split);
    let (mut omnibus, mut marked, mut others, mut wrong) = (0, 0, 0, Vec::new());
    for x in members {
        let (named, is) = (is_omnibus_name(&x.path), x.kind == Kind::Omnibus);
        omnibus += usize::from(is);
        marked += usize::from(named && is);
        others += usize::from(named && !is);
        if named != is {
            wrong.push(&x.path);
        }
    }
    let _ = writeln!(
        out,
        "is_omnibus_name marks {marked} of {omnibus} omnibus members and {others} other members"
    );
    for path in wrong {
        let _ = writeln!(out, "  by name or by kind, not both: {path}");
    }
    out
}

/// The median of `values`, `None` for none.
pub(super) fn median(values: &[f64]) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(sorted[n / 2]),
        _ => Some(f64::midpoint(sorted[n / 2 - 1], sorted[n / 2])),
    }
}

/// A pair shown to a person: its names, why it is shown, and each A page
/// beside the B page the alignment paired it with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Shown {
    pub a: String,
    pub b: String,
    pub question: Question,
    pub label: String,
    pub why: Vec<String>,
    pub a_pages: Vec<u32>,
    pub b_pages: Vec<u32>,
    pub a_count: u32,
    pub b_count: u32,
}

/// One round of judging, `adjudicate-N.json`.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ToJudge {
    pub stamp: String,
    pub round: u32,
    pub cap: usize,
    pub pairs: Vec<Shown>,
}

/// The score at or above which an unlabelled pair inside a rule's gates is
/// shown, `None` for none, and what the line is, for the reason given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShowLine {
    pub rule: Rule,
    pub at: Option<f64>,
    pub what: &'static str,
}

/// Each rule's line on dev: its highest labelled dev negative. Held-out
/// pairs are shown at the frozen bar instead, so they take other lines.
pub(super) fn dev_lines(m: &Manifest, scored: &[Scored]) -> [ShowLine; 3] {
    RULES.map(|rule| {
        let labelled: Vec<(bool, f64)> = (rule_rows(rule, m, scored).iter())
            .filter_map(|(_, r)| r.class.map(|c| (c, r.score)))
            .collect();
        ShowLine {
            rule,
            at: dev_outcome(&labelled).max_negative,
            what: "its max negative",
        }
    })
}

/// The pairs a person must judge, by position in `scored`, each with why:
/// an unlabelled pair at or above `lines`' line for a rule whose gates it
/// meets, or a labelled negative at or above [`SHOWN_NEGATIVE`] under the
/// rule it is a negative of. A pair already judged is not shown again,
/// whatever it scores.
pub(super) fn shown(
    m: &Manifest,
    scored: &[Scored],
    lines: &[ShowLine],
) -> Vec<(usize, Vec<String>)> {
    let mut why: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for rule in RULES {
        let rows = rule_rows(rule, m, scored);
        let given = lines.iter().find(|l| l.rule == rule);
        let (line, what) = given.map_or((None, ""), |l| (l.at, l.what));
        for (n, r) in &rows {
            let name = rule.name();
            if let (None, Some(line)) = (r.class, line)
                && r.score >= line
            {
                let reason = format!(
                    "unlabelled, {name} rule {:.4} at or above {what} {line:.4}",
                    r.score
                );
                why.entry(*n).or_default().push(reason);
            }
            if r.class == Some(false) && r.score >= SHOWN_NEGATIVE {
                let reason = format!(
                    "{} negative, {name} rule {:.4} at or above {SHOWN_NEGATIVE:.2}",
                    label_name(r.label),
                    r.score
                );
                why.entry(*n).or_default().push(reason);
            }
        }
    }
    why.retain(|n, _| {
        let s = &scored[*n];
        verdict_on(m, &m.members[s.a].path, &m.members[s.b].path).is_none()
    });
    why.into_iter().collect()
}

/// Each A page beside the B page its slot was paired with. Slots are pages
/// with spreads split, so two slots can name one pair of pages, kept once.
pub(super) fn sheet_pages(runs: &[Run], a: &[Slot], b: &[Slot]) -> (Vec<u32>, Vec<u32>) {
    let mut pages: Vec<(u32, u32)> = Vec::new();
    for run in runs {
        let Some(on_b) = run.b else { continue };
        for k in 0..run.a.len() {
            let page = |table: &[Slot], start: u32| table[at(start + k)].page;
            let pair = (page(a, run.a.start), page(b, on_b.start));
            if pages.last() != Some(&pair) {
                pages.push(pair);
            }
        }
    }
    pages.into_iter().unzip()
}

/// The round the next list is for: one past the highest round any of the
/// split's adjudications names, its note opening `<split> round N`.
pub(super) fn next_round(m: &Manifest, split: Split) -> u32 {
    let prefix = format!("{} round ", split_name(split));
    (m.adjudications.iter())
        .filter_map(|j| j.note.strip_prefix(&prefix))
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<u32>().ok()
        })
        .max()
        .map_or(1, |n| n + 1)
}

/// What the split's adjudications so far say: how many, how many were
/// "same", and how many changed the label the names had given a pair. A
/// verdict that keeps a label, or labels an unlabelled pair, is no relabel.
pub(super) fn adjudication_line(m: &Manifest, split: Split) -> String {
    let at: HashMap<&str, &Member> = m.members.iter().map(|x| (x.path.as_str(), x)).collect();
    let (mut all, mut same, mut relabels) = (0, 0, 0);
    for j in &m.adjudications {
        let (Some(a), Some(b)) = (at.get(j.a.as_str()), at.get(j.b.as_str())) else {
            continue;
        };
        if split_of(a, b) != split {
            continue;
        }
        all += 1;
        same += usize::from(j.same);
        let by_names = label(&Manifest::default(), a, b);
        relabels += usize::from(by_names != Label::Unlabelled && label(m, a, b) != by_names);
    }
    format!(
        "adjudications of {} pairs: {all}, same: {same}, relabels of a labelled pair: {relabels}, \
         rounds: {}",
        split_name(split),
        next_round(m, split) - 1
    )
}

/// The paths adjudications name that no member has, each once, in the order
/// named: a verdict on a mistyped path would match no pair, fall out of
/// every count and leave its pair to be shown again.
fn strangers(m: &Manifest) -> Vec<&str> {
    let paths: std::collections::HashSet<&str> =
        m.members.iter().map(|x| x.path.as_str()).collect();
    let mut out: Vec<&str> = Vec::new();
    for path in (m.adjudications.iter()).flat_map(|j| [j.a.as_str(), j.b.as_str()]) {
        if !paths.contains(path) && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// Every member a split's scoring reads: the split's own, which the lookup's
/// predictions need, and both members of each of its pairs.
fn needed(m: &Manifest, split: Split, pairs: &[((usize, usize), Source)]) -> Vec<bool> {
    let mut need: Vec<bool> = m.members.iter().map(|x| x.split == split).collect();
    for &((i, j), _) in pairs {
        need[i] = true;
        need[j] = true;
    }
    need
}

/// The needed members' prints through the cache, in parallel, as nothing
/// here is timed; a member not needed or not readable is `None`.
fn prints(
    cache: &Path,
    library: &Path,
    m: &Manifest,
    need: &[bool],
) -> (Vec<Option<Print>>, Vec<String>) {
    let results: Vec<Option<Result<Print, String>>> = (m.members.par_iter().zip(need))
        .map(|(member, &need)| need.then(|| print_member(cache, library, member)))
        .collect();
    let mut errors = Vec::new();
    let prints = (results.into_iter())
        .map(|r| match r {
            Some(Ok(print)) => Some(print),
            Some(Err(e)) => {
                errors.push(e);
                None
            }
            None => None,
        })
        .collect();
    (prints, errors)
}

/// A split's pairs, from the manifest and `leaders.json`, aligned: what
/// both splits' runs read and score before they judge anything.
#[derive(Debug)]
pub(super) struct Prepared {
    pub leaders: Leaders,
    pub pairs: Vec<((usize, usize), Source)>,
    pub needed: usize,
    pub prints: Vec<Option<Print>>,
    pub errors: Vec<String>,
    pub scored: Vec<Scored>,
    pub unscored: Vec<(usize, usize)>,
}

/// The split's pairs scored, once no adjudication names a path the
/// manifest does not hold.
pub(super) fn prepare(
    library: &Path,
    ev: &Path,
    m: &Manifest,
    split: Split,
) -> Result<Prepared, String> {
    let strangers = strangers(m);
    if !strangers.is_empty() {
        return Err(format!(
            "adjudications name paths the manifest does not hold: {}",
            strangers.join(", ")
        ));
    }
    let file = ev.join("leaders.json");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let leaders = read_leaders(&text, m)?;
    let pairs: Vec<((usize, usize), Source)> = scored_pairs(&explicit_pairs(m), &leaders.pairs)
        .into_iter()
        .filter(|&((i, j), _)| split_of(&m.members[i], &m.members[j]) == split)
        .collect();
    let need = needed(m, split, &pairs);
    let (prints, errors) = prints(&ev.join("fp-cache"), library, m, &need);
    let (scored, unscored) = score_pairs(m, &prints, &pairs);
    Ok(Prepared {
        leaders,
        pairs,
        needed: need.iter().filter(|&&n| n).count(),
        prints,
        errors,
        scored,
        unscored,
    })
}

/// The opening of `rules.txt`: what was read and scored, `checked` (what
/// the run holds its judging to), the time per pair, each unreadable member.
pub(super) fn header(stamp: &str, p: &Prepared, checked: &str) -> String {
    let hits = (p.prints.iter().flatten())
        .filter(|x| x.hits == x.paths)
        .count();
    let by_source = |source: Source| p.pairs.iter().filter(|(_, s)| *s == source).count();
    let times: Vec<f64> = p.scored.iter().map(|s| s.ms).collect();
    let mut out = format!(
        "{stamp}\nleaders.json: {}\npairs: {}, explicit only: {}, leaders only: {}, both: {}\n\
         scored: {}, not scored as a member did not print: {}\n\
         members read: {}, unreadable: {}, every path from the cache: {hits}\n\
         {checked}\n\
         median scoring time per pair: {:.3} ms over {} pairs, one at a time\n\n",
        p.leaders.stamp,
        p.pairs.len(),
        by_source(Source::Explicit),
        by_source(Source::Leader),
        by_source(Source::Both),
        p.scored.len(),
        p.unscored.len(),
        p.needed,
        p.errors.len(),
        median(&times).unwrap_or(0.0),
        times.len(),
    );
    for e in &p.errors {
        let _ = writeln!(out, "unreadable: {}", e.replace(['\t', '\n', '\r'], " "));
    }
    out
}

/// The lines of a [`query_report`] that judge its predictions.
pub(super) fn predictions(report: &str) -> impl Iterator<Item = &str> {
    report.lines().filter(|l| {
        ["queries whose positives all failed to print", "P1", "P2"]
            .iter()
            .any(|start| l.starts_with(start))
    })
}

/// Scores the dev split under `ev` and writes `dev/scores.tsv`,
/// `dev/rules.txt` and `dev/adjudicate-N.json`, after asserting the
/// controls on the first `SameVolume` pair scored.
pub(super) fn dev(library: &Path, ev: &Path, m: &Manifest) -> Result<String, String> {
    let split = Split::Dev;
    let stamp = stamp("score_dev");
    let p = prepare(library, ev, m, split)?;
    let dir = ev.join(split_name(split));
    let (prints, scored) = (&p.prints, &p.scored);

    let first = (scored.iter())
        .find(|s| matches!(s.label, Label::SameVolume { .. }))
        .ok_or("no SameVolume pair was scored, so the controls cannot run")?;
    let slots = |i: usize| prints[i].as_ref().map_or(&[][..], |p| p.slots.as_slice());
    let (a, b) = (&m.members[first.a].path, &m.members[first.b].path);
    let control = controls(slots(first.a), slots(first.b))
        .map_err(|line| format!("a control failed on {a} and {b}: {line}. No rule is judged."))?;

    put(&dir.join("scores.tsv"), &scores_tsv(&stamp, m, scored));
    let mut rules = header(
        &stamp,
        &p,
        &format!("controls on {a} and {b}: {control}: hold"),
    );
    for rule in RULES {
        let rows = rule_rows(rule, m, scored);
        let plain: Vec<ScoredRow> = rows.iter().map(|(_, r)| r.clone()).collect();
        let _ = write!(
            rules,
            "\n{}{}",
            report(rule, &plain),
            described(rule, scored, &rows)
        );
    }
    let _ = write!(rules, "\n{}", omnibus(m, split, scored));

    let list = shown(m, scored, &dev_lines(m, scored));
    let (listed, round_line) = to_judge(m, split, scored, list, prints, &stamp);
    let _ = write!(
        rules,
        "\n## adjudication\n{}\n{round_line}\n",
        adjudication_line(m, split)
    );

    let lookup = query_report(&stamp, split, m, prints, &p.leaders.lookups);
    let _ = writeln!(
        rules,
        "\n## the lookup's predictions on the manifest as adjudicated, from leaders.json"
    );
    for line in predictions(&lookup) {
        let _ = writeln!(rules, "{line}");
    }
    put(&dir.join("rules.txt"), &rules);
    let json = serde_json::to_string_pretty(&listed).expect("a round serialises") + "\n";
    put(
        &dir.join(format!("adjudicate-{}.json", listed.round)),
        &json,
    );
    Ok(round_line)
}

/// A pair as a person is asked about it, by manifest position, with each A
/// page beside the B page it was paired with.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Asked {
    pub a: usize,
    pub b: usize,
    pub question: Question,
    pub a_pages: Vec<u32>,
    pub b_pages: Vec<u32>,
}

/// A scored pair as it is asked about: where one member can hold the other,
/// the held member first, as A, with its pages, asked whether it lies
/// inside B; any other pair as scored, asked whether it is one book.
pub(super) fn framed(m: &Manifest, a: usize, b: usize, pages: (Vec<u32>, Vec<u32>)) -> Asked {
    let (a_pages, b_pages) = pages;
    match held(&m.members[a], &m.members[b]) {
        Some(false) => Asked {
            a: b,
            b: a,
            question: Question::Inside,
            a_pages: b_pages,
            b_pages: a_pages,
        },
        found => Asked {
            a,
            b,
            question: if found.is_some() {
                Question::Inside
            } else {
                Question::Same
            },
            a_pages,
            b_pages,
        },
    }
}

/// A round's line for `rules.txt`, flagging a list longer than a round may
/// hold, which is written whole for the controller to split.
pub(super) fn round_line(round: u32, pairs: usize) -> String {
    let over = if pairs > ROUND {
        format!(", over the {ROUND} a round holds: the controller splits it")
    } else {
        String::new()
    };
    format!("round {round}: {pairs} pairs to judge, in adjudicate-{round}.json{over}")
}

/// The next round of judging, `list` in its order, each pair with the pages
/// its alignment paired, and the round's line for `rules.txt`. A list over
/// [`ROUND`] is written whole and flagged: only the controller splits it.
pub(super) fn to_judge(
    m: &Manifest,
    split: Split,
    scored: &[Scored],
    list: Vec<(usize, Vec<String>)>,
    prints: &[Option<Print>],
    stamp: &str,
) -> (ToJudge, String) {
    let round = next_round(m, split);
    let pairs: Vec<Shown> = (list.into_iter())
        .map(|(n, why)| {
            let s = &scored[n];
            let (a, b) = (prints[s.a].as_ref(), prints[s.b].as_ref());
            let (a, b) = a.zip(b).expect("a scored pair's members printed");
            let (_, runs) = aligned(&a.slots, &b.slots);
            let asked = framed(m, s.a, s.b, sheet_pages(&runs, &a.table, &b.table));
            Shown {
                a: m.members[asked.a].path.clone(),
                b: m.members[asked.b].path.clone(),
                question: asked.question,
                label: label_name(s.label),
                why,
                a_pages: asked.a_pages,
                b_pages: asked.b_pages,
                a_count: prints[asked.a].as_ref().map_or(0, |p| p.pages),
                b_count: prints[asked.b].as_ref().map_or(0, |p| p.pages),
            }
        })
        .collect();
    let line = round_line(round, pairs.len());
    let listed = ToJudge {
        stamp: stamp.to_owned(),
        round,
        cap: ROUND,
        pairs,
    };
    (listed, line)
}

/// The highest-numbered `adjudicate-N.json` in `dir`, rendered beside it as
/// `adjudicate-N.html`. `None` when it lists no pair: nothing to judge. A
/// list longer than a round holds is refused, as no sheet may ask more.
pub(super) fn render(library: &Path, dir: &Path) -> Result<Option<PathBuf>, String> {
    let fail = |e: std::io::Error| format!("{}: {e}", dir.display());
    let mut newest: Option<u32> = None;
    for entry in std::fs::read_dir(dir).map_err(fail)? {
        let name = entry.map_err(fail)?.file_name();
        let name = name.to_string_lossy();
        let n = (name.strip_prefix("adjudicate-"))
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(|n| n.parse::<u32>().ok());
        newest = newest.max(n);
    }
    let n = newest.ok_or_else(|| format!("{}: no adjudicate-N.json", dir.display()))?;
    let file = dir.join(format!("adjudicate-{n}.json"));
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let listed: ToJudge =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    if listed.pairs.len() > ROUND {
        return Err(format!(
            "{}: {} pairs, over the {ROUND} a round holds; split it into rounds of at most {ROUND}",
            file.display(),
            listed.pairs.len()
        ));
    }
    if listed.pairs.is_empty() {
        return Ok(None);
    }
    let pairs: Vec<SheetPair> = (listed.pairs.into_iter())
        .map(|s| SheetPair {
            a: s.a,
            b: s.b,
            question: s.question,
            a_pages: s.a_pages,
            b_pages: s.b_pages,
            a_count: s.a_count,
            b_count: s.b_count,
        })
        .collect();
    let out = dir.join(format!("adjudicate-{n}.html"));
    pairing_sheet(library, &out, &pairs)?;
    Ok(Some(out))
}
