//! The held-out split scored against the bars dev froze in `bars.json`.
//!
//! Nothing here sets a bar. Each rule's held-out negatives are read against
//! its frozen bar with [`held_out_failures`], and the exploratory
//! chapter-in-volume line, which the pre-registration does not hold, against
//! its own. An unlabelled pair at or above a bar counts as different, so as a
//! failure, until a person says it is the same book; the pairs whose verdict
//! can change a result therefore lead the list a person judges.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use super::lookup::{query_report, share};
use super::manifest::{Kind, Label, Manifest, Member, Split, class, label, split_of, verdict_on};
use super::rule::{Rule, SHOWN_NEGATIVE, ScoredRow, held_out_failures, label_name};
use super::score::{
    Scored, adjudication_line, described, header, in_gates, omnibus, predictions, prepare,
    rule_rows, scores_tsv, split_name, sub_class, to_judge,
};
use super::{Log, put, stamp, utc};

/// The bars dev froze, as `bars.json` holds them; `None` where no bar fit.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bars {
    pub volume: Option<f64>,
    pub chapter: Option<f64>,
    pub pack: Option<f64>,
    pub chapter_in_volume_exploratory: Option<f64>,
    pub manifest_commit: String,
}

const BAR_KEYS: [&str; 5] = [
    "volume",
    "chapter",
    "pack",
    "chapter_in_volume_exploratory",
    "manifest_commit",
];

/// `bars.json` read whole. Every key must be there, as serde reads a missing
/// `Option` as `None`, and a bar left out would read as one that never fit.
pub(super) fn read_bars(text: &str) -> Result<Bars, String> {
    let raw: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("bars.json: {e}"))?;
    let missing: Vec<&str> = (BAR_KEYS.iter().copied())
        .filter(|key| raw.get(key).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(format!("bars.json lacks {}", missing.join(", ")));
    }
    serde_json::from_value(raw).map_err(|e| format!("bars.json: {e}"))
}

/// Whether the bars compiled in are the ones HEAD commits. A bar edited
/// since the freeze, or never committed, is not frozen, and judging against
/// it would let the held-out scores set it.
pub(super) fn frozen(compiled: &str, committed: Option<&str>) -> Result<(), String> {
    match committed {
        Some(text) if text == compiled => Ok(()),
        Some(_) => Err(
            "bars.json differs from the one HEAD commits, so these bars are not the frozen ones"
                .to_owned(),
        ),
        None => Err("HEAD commits no bars.json, so no bar is frozen".to_owned()),
    }
}

/// The log's line saying exactly one commit touches `bars.json`, from `git
/// log --format=%H`. The bars are used as frozen, and a second commit touching
/// them means they changed; HEAD would then commit the edited text, which
/// [`frozen`] cannot tell from the frozen one.
pub(super) fn one_freeze(git_log: Option<&str>) -> Result<String, String> {
    let Some(text) = git_log else {
        return Err("git could not list the commits touching bars.json".to_owned());
    };
    let commits: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    match commits.as_slice() {
        [] => Err("no commit touches bars.json, so no bar is frozen".to_owned()),
        [one] => Ok(format!("bars.json: 1 commit touches it, {one}")),
        many => Err(format!(
            "{} commits touch bars.json ({}), so its bars changed after the first froze them",
            many.len(),
            many.join(", ")
        )),
    }
}

/// The log's line naming the commit that froze the bars, from `git log -1
/// --format='%h %ct'`, so the log shows the freeze came before its stamp.
pub(super) fn provenance(git_log: Option<&str>) -> String {
    let parsed = (git_log.map(str::trim))
        .and_then(|line| line.split_once(' '))
        .and_then(|(hash, secs)| secs.parse::<u64>().ok().map(|secs| (hash, secs)));
    match parsed {
        Some((hash, secs)) => format!("bars.json: committed {} in {hash}", utc(secs)),
        None => "bars.json: its commit could not be read".to_owned(),
    }
}

/// What a held-out pair is read against: a rule's frozen bar, or the
/// exploratory chapter-in-volume line over some of the chapter rule's pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Against {
    Rule(Rule),
    ChapterInVolume,
}

pub(super) const AGAINST: [Against; 4] = [
    Against::Rule(Rule::Volume),
    Against::Rule(Rule::Chapter),
    Against::Rule(Rule::Pack),
    Against::ChapterInVolume,
];

/// Every line about the chapter-in-volume line carries this.
pub(super) const EXPLORATORY: &str = "[exploratory, not pre-registered]";

impl Against {
    pub(super) fn bar(self, bars: &Bars) -> Option<f64> {
        match self {
            Against::Rule(Rule::Volume) => bars.volume,
            Against::Rule(Rule::Chapter) => bars.chapter,
            Against::Rule(Rule::Pack) => bars.pack,
            Against::ChapterInVolume => bars.chapter_in_volume_exploratory,
        }
    }

    fn rule(self) -> Rule {
        match self {
            Against::Rule(rule) => rule,
            Against::ChapterInVolume => Rule::Chapter,
        }
    }

    fn name(self) -> String {
        match self {
            Against::Rule(rule) => format!("{} rule", rule.name()),
            Against::ChapterInVolume => "chapter in volume".to_owned(),
        }
    }

    fn tag(self) -> String {
        match self {
            Against::Rule(_) => String::new(),
            Against::ChapterInVolume => format!(" {EXPLORATORY}"),
        }
    }
}

/// A pair the exploratory line reads: a manga chapter against a book that
/// can print it. Its slot and kind gates are the chapter rule's.
pub(super) fn chapter_in_volume(a: &Member, b: &Member) -> bool {
    a.kind == Kind::Chapter && matches!(b.kind, Kind::Volume | Kind::Omnibus | Kind::Pack)
}

/// A pair as held-out reads it: beside its label, the one its names alone
/// give and what that counts as under the rule, so a pass resting on a
/// person's relabel shows. A pair relabelled out of the rule stays, with
/// `row.class` `None` and a label other than `Unlabelled`.
#[derive(Debug, Clone)]
pub(super) struct HeldRow {
    pub row: ScoredRow,
    pub names: Label,
    pub names_class: Option<bool>,
}

/// `against`'s pairs, by position in `scored`: those its rule counts, the
/// unlabelled ones inside its gates, and those its names made one it counts.
pub(super) fn held_rows(
    against: Against,
    m: &Manifest,
    scored: &[Scored],
) -> Vec<(usize, HeldRow)> {
    let rule = against.rule();
    (scored.iter().enumerate())
        .filter_map(|(n, s)| {
            let (a, b, f) = (&m.members[s.a], &m.members[s.b], &s.features);
            if against == Against::ChapterInVolume && !chapter_in_volume(a, b) {
                return None;
            }
            let names = label(&Manifest::default(), a, b);
            let names_class = class(rule, names, a, b, f);
            let place = class(rule, s.label, a, b, f);
            let unlabelled = s.label == Label::Unlabelled && in_gates(rule, a, b, f);
            (place.is_some() || unlabelled || names_class.is_some()).then(|| {
                let row = ScoredRow {
                    a: a.path.clone(),
                    b: b.path.clone(),
                    label: s.label,
                    sub_class: sub_class(rule, s.label, a, b),
                    class: place,
                    score: s.score(rule),
                };
                let held = HeldRow {
                    row,
                    names,
                    names_class,
                };
                (n, held)
            })
        })
        .collect()
}

/// Rows against a frozen bar, each list by position in the rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Judgement {
    pub bar: Option<f64>,
    pub positives: usize,
    pub negatives: usize,
    pub unlabelled: usize,
    /// Labelled negatives at or above the bar.
    pub failures: Vec<usize>,
    /// Unlabelled pairs at or above it that no verdict has labelled.
    pub unjudged: Vec<usize>,
    /// Pairs the names made negatives at or above it that a verdict took out
    /// of the negatives: the result rests on each.
    pub relabels: Vec<usize>,
    pub clearing: usize,
}

pub(super) fn judge(rows: &[HeldRow], bar: Option<f64>) -> Judgement {
    let count = |class: bool| rows.iter().filter(|r| r.row.class == Some(class)).count();
    let mut out = Judgement {
        bar,
        positives: count(true),
        negatives: count(false),
        unlabelled: (rows.iter())
            .filter(|r| r.row.label == Label::Unlabelled)
            .count(),
        ..Judgement::default()
    };
    let Some(bar) = bar else { return out };
    let labelled: Vec<(usize, (bool, f64))> = (rows.iter().enumerate())
        .filter_map(|(i, r)| r.row.class.map(|c| (i, (c, r.row.score))))
        .collect();
    let scores: Vec<(bool, f64)> = labelled.iter().map(|&(_, x)| x).collect();
    let positions = |keep: &dyn Fn(&HeldRow) -> bool| -> Vec<usize> {
        (rows.iter().enumerate())
            .filter(|(_, r)| r.row.score >= bar && keep(r))
            .map(|(i, _)| i)
            .collect()
    };
    out.failures = (held_out_failures(&scores, bar).into_iter())
        .map(|k| labelled[k].0)
        .collect();
    out.unjudged = positions(&|r| r.row.label == Label::Unlabelled);
    out.relabels = positions(&|r| r.names_class == Some(false) && r.row.class != Some(false));
    out.clearing = (rows.iter())
        .filter(|r| r.row.class == Some(true) && r.row.score >= bar)
        .count();
    out
}

/// The result on held-out. A failure, or an unlabelled pair at the bar with
/// no verdict, which counts as different, fails; no negative leaves the
/// rule untested, which is no pass; a pass resting on relabels says so.
pub(super) fn verdict(j: &Judgement) -> String {
    if j.bar.is_none() {
        "no bar, not tested".to_owned()
    } else if !j.failures.is_empty() || !j.unjudged.is_empty() {
        "fails".to_owned()
    } else if j.negatives == 0 {
        "untested on held-out, no held-out negative".to_owned()
    } else if j.relabels.is_empty() {
        "passes".to_owned()
    } else {
        format!("passes after {} relabels", j.relabels.len())
    }
}

fn pair(r: &ScoredRow) -> String {
    format!("{:.4}, {} and {}", r.score, r.a, r.b)
}

/// One section of held-out `rules.txt`: the counts, the highest negative
/// (the first of a tie), each failure and each relabel the result rests on,
/// the positives clearing the bar by sub-class, and the verdict. With no bar
/// the positives above the highest negative are given instead.
pub(super) fn section(title: &str, rows: &[HeldRow], bar: Option<f64>) -> String {
    let j = judge(rows, bar);
    let mut out = format!("## {title}\n");
    let _ = match bar {
        Some(bar) => writeln!(out, "frozen bar: {bar:.2} (exactly {bar})"),
        None => writeln!(out, "frozen bar: none, as none fit on dev"),
    };
    let _ = writeln!(
        out,
        "positives: {}, negatives: {}, unlabelled in its gates: {}",
        j.positives, j.negatives, j.unlabelled
    );
    let highest = (rows.iter().map(|r| &r.row))
        .filter(|r| r.class == Some(false))
        .fold(None, |best: Option<&ScoredRow>, r| match best {
            Some(b) if b.score >= r.score => Some(b),
            _ => Some(r),
        });
    let _ = match highest {
        Some(r) => writeln!(
            out,
            "highest negative: {} ({})",
            pair(r),
            label_name(r.label)
        ),
        None => writeln!(out, "highest negative: none"),
    };
    if bar.is_some() {
        against_bar(&mut out, rows, &j);
    } else {
        out.push_str("failures: no bar\npositives at or above the bar: no bar\n");
        if let Some(h) = highest {
            let above = (rows.iter())
                .filter(|r| r.row.class == Some(true) && r.row.score > h.score)
                .count();
            let _ = writeln!(
                out,
                "positives above the highest negative: {above} of {} ({})",
                j.positives,
                share(above, j.positives)
            );
        }
    }
    let _ = writeln!(out, "verdict on held-out: {}", verdict(&j));
    out
}

fn against_bar(out: &mut String, rows: &[HeldRow], j: &Judgement) {
    let _ = writeln!(
        out,
        "failures: {}, labelled negatives at or above the bar: {}, unlabelled at or above it \
         with no verdict, each counted as different: {}",
        j.failures.len() + j.unjudged.len(),
        j.failures.len(),
        j.unjudged.len()
    );
    for &i in &j.failures {
        let r = &rows[i].row;
        let _ = writeln!(out, "  failure: {} ({})", pair(r), label_name(r.label));
    }
    for &i in &j.unjudged {
        let _ = writeln!(
            out,
            "  no verdict, counted as different: {}",
            pair(&rows[i].row)
        );
    }
    let _ = writeln!(out, "relabels the result rests on: {}", j.relabels.len());
    for &i in &j.relabels {
        let r = &rows[i];
        let _ = writeln!(
            out,
            "  relabelled: {} ({} by its names, now {})",
            pair(&r.row),
            label_name(r.names),
            label_name(r.row.label)
        );
    }
    let _ = writeln!(
        out,
        "positives at or above the bar: {} of {} ({})",
        j.clearing,
        j.positives,
        share(j.clearing, j.positives)
    );
    let bar = j.bar.unwrap_or(f64::INFINITY);
    let mut by_sub: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for r in rows
        .iter()
        .map(|r| &r.row)
        .filter(|r| r.class == Some(true))
    {
        let entry = by_sub.entry(r.sub_class.as_str()).or_default();
        entry.0 += usize::from(r.score >= bar);
        entry.1 += 1;
    }
    for (sub, (clearing, all)) in by_sub {
        let _ = writeln!(
            out,
            "  {sub}: {clearing} of {all} ({})",
            share(clearing, all)
        );
    }
}

/// The held-out pairs a person must judge, by position in `scored`, each
/// with why, and how many lead. A pair is shown as pre-registered: unlabelled
/// at or above a frozen bar or the line, or a labelled negative at
/// [`SHOWN_NEGATIVE`] or more. Those at or above a bar or the line lead, as a
/// verdict can change a result on them. A labelled negative under 0.50 is not
/// shown even at a bar, since a verdict could only rescue it: it fails as
/// labelled. Each part keeps the scored order; a judged pair is not shown.
pub(super) fn held_out_list(
    m: &Manifest,
    scored: &[Scored],
    bars: &Bars,
) -> (Vec<(usize, Vec<String>)>, usize) {
    let mut why: BTreeMap<usize, (bool, Vec<String>)> = BTreeMap::new();
    for against in AGAINST {
        let (bar, name, tag) = (against.bar(bars), against.name(), against.tag());
        for (n, held) in held_rows(against, m, scored) {
            let r = &held.row;
            let unlabelled = r.label == Label::Unlabelled;
            let shown_negative = r.class == Some(false) && r.score >= SHOWN_NEGATIVE;
            if let Some(bar) = bar
                && r.score >= bar
                && (unlabelled || shown_negative)
            {
                let what = if unlabelled {
                    "unlabelled".to_owned()
                } else {
                    format!("{} negative", label_name(r.label))
                };
                let entry = why.entry(n).or_default();
                entry.0 = true;
                let reason = format!(
                    "{what}, {name} {:.4} at or above its frozen bar {bar:.4}{tag}",
                    r.score
                );
                entry.1.push(reason);
            }
            if matches!(against, Against::Rule(_)) && shown_negative {
                let reason = format!(
                    "{} negative, {name} {:.4} at or above {SHOWN_NEGATIVE:.2}",
                    label_name(r.label),
                    r.score
                );
                why.entry(n).or_default().1.push(reason);
            }
        }
    }
    why.retain(|n, _| {
        let s = &scored[*n];
        verdict_on(m, &m.members[s.a].path, &m.members[s.b].path).is_none()
    });
    let (lead, rest): (Vec<_>, Vec<_>) = why.into_iter().partition(|(_, (leads, _))| *leads);
    let leading = lead.len();
    let list = (lead.into_iter().chain(rest))
        .map(|(n, (_, why))| (n, why))
        .collect();
    (list, leading)
}

/// The order of a held-out round, for `rules.txt`.
pub(super) fn order_line(leading: usize, all: usize) -> String {
    format!(
        "order: the first {leading} can change a result, each an unlabelled pair at or above a \
         frozen bar or the chapter-in-volume line {EXPLORATORY}, or a labelled negative at or \
         above {SHOWN_NEGATIVE:.2} and one of them; the other {} are labelled negatives at or \
         above {SHOWN_NEGATIVE:.2} below every bar and the line, which no verdict can make a \
         failure; each part in scored order",
        all - leading
    )
}

/// The manifest without the split's own verdicts: what its predictions read
/// before any of its rounds.
pub(super) fn before_rounds(m: &Manifest, split: Split) -> Manifest {
    let at: HashMap<&str, &Member> = m.members.iter().map(|x| (x.path.as_str(), x)).collect();
    let adjudications = (m.adjudications.iter())
        .filter(|j| match (at.get(j.a.as_str()), at.get(j.b.as_str())) {
            (Some(a), Some(b)) => split_of(a, b) != split,
            _ => true,
        })
        .cloned()
        .collect();
    Manifest {
        members: m.members.clone(),
        adjudications,
    }
}

fn bars_line(b: &Bars) -> String {
    let bar = |x: Option<f64>| x.map_or_else(|| "none".to_owned(), |v| v.to_string());
    format!(
        "frozen bars, manifest {}: volume {}, chapter {}, pack {}, chapter in volume {} \
         {EXPLORATORY}",
        b.manifest_commit,
        bar(b.volume),
        bar(b.chapter),
        bar(b.pack),
        bar(b.chapter_in_volume_exploratory)
    )
}

/// The comics: every pair whose A is an issue, judged under the chapter
/// rule's frozen bar. A failed manga chapter rule leaves no trusted bar, and
/// then they are reported descriptively.
fn comics(m: &Manifest, scored: &[Scored], rows: &[HeldRow], bars: &Bars) -> String {
    let all = scored.iter().filter(|s| m.members[s.a].kind == Kind::Issue);
    let mut out = section(
        "comics, every pair whose A is an issue, on C under the chapter rule's frozen bar",
        rows,
        bars.chapter,
    );
    let _ = writeln!(
        out,
        "pairs whose A is an issue: {}, in the chapter rule's gates: {}",
        all.count(),
        rows.len()
    );
    if bars.chapter.is_none() {
        out.push_str(
            "with no frozen chapter bar, issues are suggestions only whatever the comics score: \
             reported descriptively\n",
        );
    }
    out
}

/// `against`'s part of `rules.txt`, each line tagged if it is about the
/// exploratory line, and its judgement. The chapter rule's held-out verdict
/// covers its pairs whose A is a chapter, so the comics follow on their own.
fn against_section(
    against: Against,
    m: &Manifest,
    scored: &[Scored],
    bars: &Bars,
) -> (String, Judgement) {
    let (bar, rows) = (against.bar(bars), held_rows(against, m, scored));
    let (text, judged) = if against == Against::Rule(Rule::Chapter) {
        let (manga, issues): (Vec<_>, Vec<_>) = (rows.into_iter())
            .map(|(n, r)| (m.members[scored[n].a].kind, r))
            .partition(|(kind, _)| *kind == Kind::Chapter);
        let manga: Vec<HeldRow> = manga.into_iter().map(|(_, r)| r).collect();
        let issues: Vec<HeldRow> = issues.into_iter().map(|(_, r)| r).collect();
        let title = "manga chapter rule, on C: the chapter rule's pairs whose A is a chapter";
        let text = section(title, &manga, bar) + &comics(m, scored, &issues, bars);
        (text, judge(&manga, bar))
    } else {
        let rows: Vec<HeldRow> = rows.into_iter().map(|(_, r)| r).collect();
        let title = format!("{}, on {}", against.name(), against.rule().score_name());
        (section(&title, &rows, bar), judge(&rows, bar))
    };
    let (tag, mut out) = (against.tag(), String::new());
    for line in text.lines() {
        let _ = writeln!(out, "{line}{tag}");
    }
    (out, judged)
}

/// Scores the held-out split under `ev` against `bars` into `held-out/`'s
/// `scores.tsv`, `rules.txt`, `lookup.txt` and `adjudicate-N.json`. Its log,
/// `logs/score-held-out.log`, opens with the stamp and `provenance` before
/// any pair is read.
pub(super) fn run(
    library: &Path,
    ev: &Path,
    m: &Manifest,
    bars: &Bars,
    provenance: &str,
) -> Result<String, String> {
    let split = Split::HeldOut;
    let stamp = stamp("score_held_out");
    let mut log = Log::create(&ev.join("logs/score-held-out.log"), &stamp);
    let checked = format!("{provenance}\n{}", bars_line(bars));
    for line in checked.lines() {
        log.line(line);
    }
    let p = prepare(library, ev, m, split)?;
    let (dir, scored) = (ev.join(split_name(split)), &p.scored);
    put(&dir.join("scores.tsv"), &scores_tsv(&stamp, m, scored));

    let mut rules = header(&stamp, &p, &checked);
    for against in AGAINST {
        let (text, judged) = against_section(against, m, scored, bars);
        let tag = against.tag();
        let _ = write!(rules, "\n{text}");
        if let Against::Rule(rule) = against {
            // The chapter rule's text ends with the comics, so its features,
            // read on manga and comics together, need a heading of their own.
            if rule == Rule::Chapter {
                rules.push_str("\n## chapter rule, all its pairs, manga and comics\n");
            }
            let _ = write!(
                rules,
                "{}",
                described(rule, scored, &rule_rows(rule, m, scored))
            );
        }
        let figures = match judged.bar {
            Some(_) => format!(
                "failures {}; positives at or above the bar {} of {}",
                judged.failures.len() + judged.unjudged.len(),
                judged.clearing,
                judged.positives
            ),
            None => "failures no bar; positives at or above the bar no bar".to_owned(),
        };
        let (name, result) = (against.name(), verdict(&judged));
        log.line(&format!("{name}: {result}; {figures}{tag}"));
    }
    let _ = write!(rules, "\n{}", omnibus(m, split, scored));

    let (list, leading) = held_out_list(m, scored, bars);
    let order = order_line(leading, list.len());
    let (listed, round_line) = to_judge(m, split, scored, list, &p.prints, &stamp);
    let _ = write!(
        rules,
        "\n## adjudication\n{}\n{round_line}\n{order}\n",
        adjudication_line(m, split)
    );

    let before = before_rounds(m, split);
    let lookups = &p.leaders.lookups;
    let earlier = query_report(&stamp, split, &before, &p.prints, lookups);
    let now = query_report(&stamp, split, m, &p.prints, lookups);
    let _ = writeln!(
        rules,
        "\n## the lookup's predictions on held-out queries, from leaders.json"
    );
    for (when, report) in [
        ("as adjudicated", &now),
        ("before any held-out round", &earlier),
    ] {
        for line in predictions(report) {
            let _ = writeln!(rules, "{when}: {line}");
        }
    }
    put(&dir.join("lookup.txt"), &earlier);
    put(&dir.join("rules.txt"), &rules);
    let json = serde_json::to_string_pretty(&listed).expect("a round serialises") + "\n";
    put(
        &dir.join(format!("adjudicate-{}.json", listed.round)),
        &json,
    );
    log.line(&round_line);
    log.line(&order);
    Ok(round_line)
}
