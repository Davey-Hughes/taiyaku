//! The test's one run: what must hold before a page is read, every pair
//! scored against the frozen bar, and the report of what that found.
//!
//! The refusals come first because the order is the evidence. A bar that may
//! have moved, or a cache that may answer for pages this run never read,
//! would leave a result no one could trust, so neither is allowed past
//! [`preflight`] and [`cache_is_unused`].

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use super::super::features::{aligned, volume_score};
use super::super::held_out::{frozen_file, one_commit, provenance_of};
use super::super::lookup::{Row, members_tsv, print_all, print_member, unreadable_line};
use super::super::manifest::{Adjudication, Manifest, Source};
use super::super::rule::{GAP, held_out_failures};
use super::super::score::{Scored, score_pairs, sheet_pages};
use super::super::sheet::{Question, SheetPair, pairing_sheet};
use super::super::{Log, Material, git, put, stamp};
use super::bar::{FrozenBar, derive, number, pairs_text, read_frozen, read_pr1, sha256_hex};
use super::members::{PairClass, TestMember, all_pairs, pair_class, to_manifest};
use super::unread::left_out;

/// Where the frozen bar lives, relative to this package.
pub(super) const BAR_FILE: &str = "src/match_experiment/volume_test/bar.json";

/// The owner's rulings, from the repository's top: the one file under `crates/`
/// a commit may touch once the run has scored, because recording a ruling is
/// the step that follows it.
pub(super) const VERDICTS_PATH: &str =
    "crates/taiyaku-cli/src/match_experiment/volume_test/verdicts.json";

/// The digest of `scores.tsv`, written with it, so that a table changed later
/// cannot be judged as the one the run scored.
const SCORES_DIGEST: &str = "scores.sha256";

/// What the one scoring leaves behind. Finding any of them means it has
/// happened, and writing them again would replace the record of what it said.
const SCORING_FILES: [&str; 4] = [
    "scores.tsv",
    SCORES_DIGEST,
    "report.txt",
    "report-first.txt",
];

/// The kinds of edition a true pair can be, in the order the report gives
/// them, so that a kind with no pair clearing the bar still shows as none.
const EDITION_KINDS: [&str; 4] = [
    "tongari raw-en",
    "ruridragon raw-en",
    "hunter-x-hunter bw-colour",
    "parasyte bw-collection",
];

/// How many different-split pairs the report names: the nearest to the bar
/// say how close the rule came, and the rest would bury what follows them.
const SPLITS_SHOWN: usize = 10;

const SCORES_COLUMNS: &str =
    "a\tb\tclass\tedition_kind\tlong_paired\ta_slots\tb_slots\tV\tverdict\tseparation";

/// One scored pair. `a` is the shorter side.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PairRow {
    pub a: String,
    pub b: String,
    pub class: PairClass,
    /// A true pair's edition kind, `-` for any other pair.
    pub edition_kind: String,
    pub v: f64,
    pub long_paired: u32,
    pub a_slots: u32,
    pub b_slots: u32,
    pub verdict: String,
    pub separation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Passes {
        relabels: usize,
    },
    Fails,
    /// No wrong pair was there to refuse, which shows nothing about the rule.
    Untested,
}

/// The rows against the bar, each list by position in the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Judged {
    pub outcome: Outcome,
    /// The wrong pairs left once the owner's relabels are taken out.
    pub wrong: usize,
    pub at_bar: Vec<usize>,
    pub relabelled: Vec<usize>,
}

fn is_wrong(class: PairClass) -> bool {
    matches!(class, PairClass::Wrong | PairClass::DifferentSplit)
}

/// The row a verdict rules on, the pair written either way round: rows are
/// oriented by slot count, which the owner reading two names knows nothing of.
fn ruled_row(rows: &[PairRow], verdict: &Adjudication) -> Option<usize> {
    (rows.iter()).position(|row| {
        (row.a == verdict.a && row.b == verdict.b) || (row.a == verdict.b && row.b == verdict.a)
    })
}

/// Why a verdict cannot stand on this row. A verdict can only take a pair the
/// rule caught out of the wrong pairs: anywhere else it would change no
/// result, or quietly remove a pair the owner has already ruled wrong.
fn refused(row: &PairRow, bar: f64, verdict: &Adjudication) -> Option<String> {
    let pair = format!("{} and {}", row.a, row.b);
    if !is_wrong(row.class) {
        return Some(format!(
            "{pair}: a verdict on a {:?} pair, which the rule does not judge",
            row.class
        ));
    }
    if row.v < bar {
        return Some(format!("{pair}: a verdict on a pair below the bar"));
    }
    (verdict.same && row.class == PairClass::DifferentSplit)
        .then(|| format!("{pair}: \"same\" on a different split, which the owner has ruled wrong"))
}

/// Holds the rows to the bar. A wrong pair the owner has called the same book
/// stops counting as wrong; every other wrong pair at or above the bar fails
/// the rule, and the bar itself is never moved to let one through.
pub(super) fn judge(
    rows: &[PairRow],
    bar: f64,
    verdicts: &[Adjudication],
) -> Result<Judged, String> {
    let mut ruled = BTreeSet::new();
    let mut relabelled = BTreeSet::new();
    for verdict in verdicts {
        let Some(at) = ruled_row(rows, verdict) else {
            return Err(format!(
                "{} and {}: a verdict on a pair the test does not hold",
                verdict.a, verdict.b
            ));
        };
        let row = &rows[at];
        if !ruled.insert(at) {
            return Err(format!("{} and {}: two verdicts on one pair", row.a, row.b));
        }
        if let Some(why) = refused(row, bar, verdict) {
            return Err(why);
        }
        if verdict.same {
            relabelled.insert(at);
        }
    }
    let scored: Vec<(bool, f64)> = (rows.iter().enumerate())
        .map(|(at, row)| (!is_wrong(row.class) || relabelled.contains(&at), row.v))
        .collect();
    let at_bar = held_out_failures(&scored, bar);
    let wrong = scored.iter().filter(|(kept_out, _)| !kept_out).count();
    let outcome = if !at_bar.is_empty() {
        Outcome::Fails
    } else if wrong == 0 {
        Outcome::Untested
    } else {
        Outcome::Passes {
            relabels: relabelled.len(),
        }
    };
    Ok(Judged {
        outcome,
        wrong,
        at_bar,
        relabelled: relabelled.into_iter().collect(),
    })
}

/// The lines that say what the bar was frozen over and when.
fn provenance_lines(touched: &str, commit: Option<&str>, frozen: &FrozenBar) -> String {
    format!(
        "{}\n{touched}\nfrozen over: dev scores {}, held-out scores {}, members {}, pairs {}",
        provenance_of("bar.json", commit),
        frozen.pr1_dev_sha256,
        frozen.pr1_held_out_sha256,
        frozen.members_sha256,
        frozen.pairs_sha256
    )
}

/// The frozen bar and the log's line on its commit, if `text` is still that
/// bar. Comparing with HEAD is not enough: a later commit that moved the bar
/// makes HEAD agree with the moved text, so the commits are counted as well.
pub(super) fn settled_bar(
    text: &str,
    committed: Option<&str>,
    touching: Option<&str>,
) -> Result<(FrozenBar, String), String> {
    frozen_file("bar.json", text, committed)?;
    let touched = one_commit("bar.json", touching)?;
    Ok((read_frozen(text)?, touched))
}

/// Everything the bar is derived from and frozen over.
pub(super) struct Freeze {
    pub manifest: Manifest,
    pub pr1_dev_scores: String,
    pub pr1_held_out_scores: String,
    pub members_json: String,
    pub members: Vec<TestMember>,
}

impl Freeze {
    /// The text `bar.json` must hold.
    pub(super) fn text(&self) -> Result<String, String> {
        frozen_text(
            &self.pr1_dev_scores,
            &self.pr1_held_out_scores,
            &self.manifest,
            &self.members_json,
            &self.members,
        )
    }
}

/// What git says of the checkout, each answer `None` where git could not give it.
#[derive(Debug, Clone, Default)]
pub(super) struct GitFacts {
    /// `bar.json` as HEAD commits it.
    pub committed: Option<String>,
    /// The commits touching `bar.json`.
    pub touching: Option<String>,
    /// The hash and time of the commit that froze it.
    pub commit: Option<String>,
    /// Every uncommitted change in the repository.
    pub status: Option<String>,
    /// The paths of the code that commits since the freeze touch.
    pub later: Option<String>,
}

/// Asks `ask`, which runs git in this package's directory, for the facts. The
/// paths are given from the repository's top for that reason. `-m` keeps a merge
/// from hiding what it or its side branch brought in, and `--no-renames` keeps a
/// file moved onto the verdicts from reading as the verdicts alone.
pub(super) fn git_facts(ask: impl Fn(&[&str]) -> Option<String>) -> GitFacts {
    let touching = ask(&["log", "--format=%H", "--", BAR_FILE]);
    let freeze = (touching.as_deref().map(str::trim))
        .filter(|commits| !commits.is_empty() && !commits.contains('\n'));
    let later = freeze.and_then(|freeze| {
        ask(&[
            "log",
            "-m",
            "--no-renames",
            "--format=",
            "--name-only",
            &format!("{freeze}..HEAD"),
            "--",
            ":/crates",
            ":/Cargo.toml",
            ":/Cargo.lock",
        ])
    });
    GitFacts {
        committed: ask(&["show", &format!("HEAD:./{BAR_FILE}")]),
        commit: ask(&["log", "-1", "--format=%h %ct", "--", BAR_FILE]),
        status: ask(&["status", "--porcelain", "--untracked-files=normal"]),
        touching,
        later,
    }
}

/// Refuses a file that is not what the rule derives from the first
/// measurement's scores. The file's own numbers cannot show this: a moved bar
/// can be made to agree with itself, and amending the freeze leaves one commit
/// touching it.
fn derived_by_the_rule(text: &str, freeze: &Freeze) -> Result<(), String> {
    if text == freeze.text()? {
        Ok(())
    } else {
        Err("bar.json is not the bar PR 1's scores derive".to_owned())
    }
}

/// Refuses code committed since the freeze, `allowed` paths apart: the bar's
/// meaning rests on the scoring and judging code it was frozen with.
fn no_code_since_the_freeze(later: Option<&str>, allowed: &[&str]) -> Result<(), String> {
    let Some(later) = later else {
        return Err("git could not list the commits since the freeze".to_owned());
    };
    let touched: BTreeSet<&str> = (later.lines().map(str::trim))
        .filter(|path| !path.is_empty() && !allowed.contains(path))
        .collect();
    if touched.is_empty() {
        return Ok(());
    }
    let paths: Vec<&str> = touched.into_iter().collect();
    Err(format!(
        "commits since the freeze touch {}, so this is not the code the bar was frozen with",
        paths.join(", ")
    ))
}

/// Refuses uncommitted changes anywhere in the repository. The stamp looks only
/// at this module, and the code that fingerprints and aligns lives outside it.
fn all_committed(status: Option<&str>) -> Result<(), String> {
    match status.map(str::trim) {
        None => Err("git could not say whether the checkout is committed".to_owned()),
        Some("") => Ok(()),
        Some(changes) => Err(format!(
            "the checkout has uncommitted changes, so the run could not be traced to its code: \
             {changes}"
        )),
    }
}

/// The bar a finished run is read against. The owner's verdicts are the one
/// commit that may follow the freeze here.
pub(super) fn finished_bar(
    text: &str,
    freeze: &Freeze,
    facts: &GitFacts,
) -> Result<FrozenBar, String> {
    let (frozen, _) = settled_bar(text, facts.committed.as_deref(), facts.touching.as_deref())?;
    derived_by_the_rule(text, freeze)?;
    no_code_since_the_freeze(facts.later.as_deref(), &[VERDICTS_PATH])?;
    Ok(frozen)
}

/// Whether the run may start, and the log's provenance lines if it may. Every
/// refusal here comes before a page is read, because a run over a bar that
/// moved, or from code no commit names, could not be told from a clean one.
pub(super) fn preflight(
    compiled: Option<&str>,
    freeze: &Freeze,
    facts: &GitFacts,
    stamp: &str,
) -> Result<(FrozenBar, String), String> {
    let Some(compiled) = compiled else {
        return Err("bar.json does not exist, so no bar is frozen".to_owned());
    };
    let (frozen, touched) = settled_bar(
        compiled,
        facts.committed.as_deref(),
        facts.touching.as_deref(),
    )?;
    let digests = [
        (
            "members_sha256",
            &frozen.members_sha256,
            sha256_hex(&freeze.members_json),
        ),
        (
            "pairs_sha256",
            &frozen.pairs_sha256,
            sha256_hex(&pairs_text(&freeze.members)),
        ),
        (
            "pr1_dev_sha256",
            &frozen.pr1_dev_sha256,
            sha256_hex(&freeze.pr1_dev_scores),
        ),
        (
            "pr1_held_out_sha256",
            &frozen.pr1_held_out_sha256,
            sha256_hex(&freeze.pr1_held_out_scores),
        ),
    ];
    for (name, held, now) in digests {
        if *held != now {
            return Err(format!(
                "bar.json's {name} is {held}, but what it covers now hashes to {now}"
            ));
        }
    }
    let head = stamp.rsplit_once(" HEAD ").map(|(_, head)| head.trim());
    if !head.is_some_and(|h| !h.is_empty() && !h.contains("+dirty") && h != "unknown") {
        return Err(format!(
            "the stamp {stamp:?} names no clean commit, so the run could not be traced to its code"
        ));
    }
    derived_by_the_rule(compiled, freeze)?;
    no_code_since_the_freeze(facts.later.as_deref(), &[])?;
    all_committed(facts.status.as_deref())?;
    let provenance = provenance_lines(&touched, facts.commit.as_deref(), &frozen);
    Ok((frozen, provenance))
}

/// Refuses a run that would not be the first scoring. A verdict is refused too:
/// the first report records what the rule said before anyone ruled on a pair.
pub(super) fn first_scoring(ev: &Path, verdicts: &[Adjudication]) -> Result<(), String> {
    if !verdicts.is_empty() {
        return Err(format!(
            "verdicts.json holds {} verdicts before the first report is written",
            verdicts.len()
        ));
    }
    for name in SCORING_FILES {
        let file = ev.join(name);
        match std::fs::symlink_metadata(&file) {
            Ok(_) => {
                return Err(format!(
                    "{} exists: the test is scored only once",
                    file.display()
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", file.display())),
        }
    }
    Ok(())
}

/// The measurement, each step finished before the next begins. Every refusal
/// comes before the log is opened, as opening it replaces an earlier run's, and
/// no test page is read until `control` has shown the code still gives the first
/// measurement's scores.
pub(super) fn measure(
    material: &Material,
    compiled: Option<&str>,
    freeze: &Freeze,
    facts: &GitFacts,
    stamped: &str,
    verdicts: &[Adjudication],
    control: impl FnOnce(&mut Log) -> Result<(), String>,
) -> Result<Judged, String> {
    let ev = &material.evidence;
    let (frozen, provenance) = preflight(compiled, freeze, facts, stamped)?;
    cache_is_unused(&ev.join("fp-cache"))?;
    first_scoring(ev, verdicts)?;

    let mut log = Log::create(&ev.join("logs/volume-test.log"), stamped);
    for line in provenance.lines() {
        log.line(line);
    }
    if let Err(why) = control(&mut log) {
        log.line(&format!("control failed: {why}"));
        return Err(why);
    }
    let judged = score(
        &material.library,
        ev,
        &freeze.members,
        &frozen,
        verdicts,
        &mut log,
    );
    if let Err(why) = &judged {
        log.line(&format!("the run stopped: {why}"));
    }
    judged
}

/// Refuses a fingerprint cache holding any entry: an entry was written by an
/// earlier read of a test page, and answering from it would hide that read.
pub(super) fn cache_is_unused(dir: &Path) -> Result<(), String> {
    let mut entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    match entries.next() {
        None => Ok(()),
        Some(entry) => {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            Err(format!(
                "{} holds {}: this run's fingerprints would not all be its own",
                dir.display(),
                entry.file_name().display()
            ))
        }
    }
}

/// The text of `bar.json`: the bar derived from the first measurement's
/// scores and the digests of everything it is frozen over.
pub(super) fn frozen_text(
    pr1_dev_scores: &str,
    pr1_held_out_scores: &str,
    m: &Manifest,
    members_json: &str,
    members: &[TestMember],
) -> Result<String, String> {
    let dev = read_pr1(pr1_dev_scores).map_err(|why| format!("the dev scores: {why}"))?;
    let held_out =
        read_pr1(pr1_held_out_scores).map_err(|why| format!("the held-out scores: {why}"))?;
    let derived = derive(m, &dev, &held_out)?;
    let frozen = FrozenBar {
        max_negative: derived.max_negative,
        gap: GAP,
        bar: derived.bar,
        pair_a: derived.pair.a.clone(),
        pair_b: derived.pair.b.clone(),
        long_paired: derived.pair.long_paired,
        a_slots: derived.pair.a_slots,
        b_slots: derived.pair.b_slots,
        pr1_dev_sha256: sha256_hex(pr1_dev_scores),
        pr1_held_out_sha256: sha256_hex(pr1_held_out_scores),
        members_sha256: sha256_hex(members_json),
        pairs_sha256: sha256_hex(&pairs_text(members)),
    };
    Ok(serde_json::to_string_pretty(&frozen).expect("a bar serialises") + "\n")
}

/// A true pair's edition kind, named for the report's four groups.
pub(super) fn edition_kind(a: &TestMember, b: &TestMember) -> String {
    if pair_class(a, b) != PairClass::True {
        return "-".to_owned();
    }
    let editions = match a.series.as_str() {
        "hunter-x-hunter" => "bw-colour",
        "parasyte" => "bw-collection",
        _ => "raw-en",
    };
    format!("{} {editions}", a.series)
}

fn pair_rows(members: &[TestMember], scored: &[Scored]) -> Vec<PairRow> {
    (scored.iter())
        .map(|s| {
            let (a, b, f) = (&members[s.a], &members[s.b], &s.features);
            PairRow {
                a: a.path.clone(),
                b: b.path.clone(),
                class: pair_class(a, b),
                edition_kind: edition_kind(a, b),
                v: f.volume,
                long_paired: f.long_paired,
                a_slots: f.a_slots,
                b_slots: f.b_slots,
                verdict: format!("{:?}", f.verdict),
                separation: f
                    .separation
                    .map_or_else(|| "-".to_owned(), |s| format!("{s:.4}")),
            }
        })
        .collect()
}

/// `scores.tsv`: a row for every pair, the stamp and the columns first.
pub(super) fn scores_text(stamp: &str, rows: &[PairRow]) -> String {
    let mut out = format!("{stamp}\n{SCORES_COLUMNS}\n");
    for r in rows {
        let _ = writeln!(
            out,
            "{}\t{}\t{:?}\t{}\t{}\t{}\t{}\t{:.6}\t{}\t{}",
            r.a,
            r.b,
            r.class,
            r.edition_kind,
            r.long_paired,
            r.a_slots,
            r.b_slots,
            r.v,
            r.verdict,
            r.separation
        );
    }
    out
}

fn class_named(name: &str) -> Option<PairClass> {
    match name {
        "True" => Some(PairClass::True),
        "Damaged" => Some(PairClass::Damaged),
        "DifferentSplit" => Some(PairClass::DifferentSplit),
        "Wrong" => Some(PairClass::Wrong),
        _ => None,
    }
}

/// One row back from the table. V is taken from the integers, as the printed
/// cell is rounded and a pair just under the bar could print as at it.
fn read_row(line: &str) -> Result<PairRow, String> {
    let cells: Vec<&str> = line.split('\t').collect();
    let &[
        a,
        b,
        class,
        kind,
        long_paired,
        a_slots,
        b_slots,
        v_cell,
        verdict,
        separation,
    ] = cells.as_slice()
    else {
        return Err(format!("{} cells in the row {line:?}", cells.len()));
    };
    let pair = format!("{a} and {b}");
    let class = class_named(class).ok_or_else(|| format!("{pair}: no class {class:?}"))?;
    let long_paired = number(&pair, "long_paired", long_paired)?;
    let a_slots = number(&pair, "a_slots", a_slots)?;
    let b_slots = number(&pair, "b_slots", b_slots)?;
    let v = volume_score(long_paired, a_slots, b_slots);
    let printed = format!("{v:.6}");
    if v_cell != printed {
        return Err(format!(
            "{pair}: V is printed as {v_cell} but its integers give {printed}"
        ));
    }
    Ok(PairRow {
        a: a.to_owned(),
        b: b.to_owned(),
        class,
        edition_kind: kind.to_owned(),
        v,
        long_paired,
        a_slots,
        b_slots,
        verdict: verdict.to_owned(),
        separation: separation.to_owned(),
    })
}

/// The stamp and every row of `scores.tsv`.
pub(super) fn read_scores(text: &str) -> Result<(String, Vec<PairRow>), String> {
    let mut lines = text.lines();
    let stamp = (lines.next())
        .filter(|line| line.starts_with("# "))
        .ok_or_else(|| "scores.tsv does not open with a run's stamp".to_owned())?;
    if lines.next() != Some(SCORES_COLUMNS) {
        return Err("scores.tsv's columns are not the ones the run writes".to_owned());
    }
    let rows = lines.map(read_row).collect::<Result<Vec<_>, _>>()?;
    Ok((stamp.to_owned(), rows))
}

fn row_line(r: &PairRow) -> String {
    format!(
        "{} and {}: {:?}, V {:.6}, long_paired {}, slots {} and {}, verdict {}, separation {}",
        r.a, r.b, r.class, r.v, r.long_paired, r.a_slots, r.b_slots, r.verdict, r.separation
    )
}

fn listed<'r>(out: &mut String, heading: &str, rows: impl Iterator<Item = &'r PairRow>) {
    let _ = writeln!(out, "\n{heading}");
    for row in rows {
        let _ = writeln!(out, "{}", row_line(row));
    }
}

fn class_counts(out: &mut String, rows: &[PairRow]) {
    let count = |class: PairClass| rows.iter().filter(|r| r.class == class).count();
    let _ = write!(
        out,
        "\n## pairs by class\ntrue: {}\ndamaged: {}\ndifferent split: {}\nwrong: {}\n",
        count(PairClass::True),
        count(PairClass::Damaged),
        count(PairClass::DifferentSplit),
        count(PairClass::Wrong)
    );
}

fn wrong_pairs(out: &mut String, rows: &[PairRow], judged: &Judged) {
    let still_wrong = (rows.iter().enumerate())
        .filter(|(at, row)| is_wrong(row.class) && !judged.relabelled.contains(at))
        .map(|(_, row)| row);
    let highest = still_wrong.reduce(|best, row| if row.v > best.v { row } else { best });
    let _ = writeln!(out, "\n## the highest wrong pair");
    let _ = writeln!(
        out,
        "{}",
        highest.map_or_else(|| "none".to_owned(), row_line)
    );
    listed(
        out,
        &format!(
            "## wrong pairs at or above the bar: {} of {}",
            judged.at_bar.len(),
            judged.wrong
        ),
        judged.at_bar.iter().map(|&at| &rows[at]),
    );
}

fn verdict_lines(out: &mut String, verdicts: &[Adjudication], judged: &Judged) {
    let _ = writeln!(
        out,
        "\n## verdicts: {}, relabels: {}",
        verdicts.len(),
        judged.relabelled.len()
    );
    for v in verdicts {
        let ruling = if v.same {
            "same, relabelled"
        } else {
            "different"
        };
        let _ = writeln!(
            out,
            "{} and {}: {ruling} ({}, {}): {}",
            v.a, v.b, v.by, v.date, v.note
        );
    }
}

fn true_pairs(out: &mut String, rows: &[PairRow], bar: f64) {
    let trues: Vec<&PairRow> = rows.iter().filter(|r| r.class == PairClass::True).collect();
    let clearing = |pairs: &[&PairRow]| pairs.iter().filter(|r| r.v >= bar).count();
    let _ = write!(
        out,
        "\n## true pairs clearing the bar\ntrue pairs at or above the bar: {} of {}\n",
        clearing(&trues),
        trues.len()
    );
    for kind in EDITION_KINDS {
        let of_kind: Vec<&PairRow> = (trues.iter().copied())
            .filter(|r| r.edition_kind == kind)
            .collect();
        let _ = writeln!(out, "  {kind}: {} of {}", clearing(&of_kind), of_kind.len());
    }
    let below: Vec<&PairRow> = trues.iter().copied().filter(|r| r.v < bar).collect();
    listed(
        out,
        &format!("## true pairs below the bar: {}", below.len()),
        below.iter().copied(),
    );
}

fn verdict_line(outcome: Outcome) -> String {
    match outcome {
        Outcome::Passes { relabels: 0 } => "verdict: passes".to_owned(),
        Outcome::Passes { relabels } => format!("verdict: passes after {relabels} relabels"),
        Outcome::Fails => "verdict: fails".to_owned(),
        Outcome::Untested => "verdict: untested".to_owned(),
    }
}

pub(super) fn report_text(
    head: &str,
    frozen: &FrozenBar,
    rows: &[PairRow],
    verdicts: &[Adjudication],
    judged: &Judged,
) -> String {
    let mut out = head.to_owned();
    let _ = write!(
        out,
        "\nfrozen bar: {bar:.2} (exactly {bar})\nset by: {} and {}, V {:.6} (long_paired {}, \
         slots {} and {}), plus the gap {}\n",
        frozen.pair_a,
        frozen.pair_b,
        frozen.max_negative,
        frozen.long_paired,
        frozen.a_slots,
        frozen.b_slots,
        frozen.gap,
        bar = frozen.bar,
    );
    class_counts(&mut out, rows);
    wrong_pairs(&mut out, rows, judged);
    verdict_lines(&mut out, verdicts, judged);
    true_pairs(&mut out, rows, frozen.bar);
    let damaged: Vec<&PairRow> = (rows.iter())
        .filter(|r| r.class == PairClass::Damaged)
        .collect();
    listed(
        &mut out,
        &format!(
            "## damaged pairs, apart from the true pairs: {}",
            damaged.len()
        ),
        damaged.iter().copied(),
    );
    let mut splits: Vec<&PairRow> = (rows.iter())
        .filter(|r| r.class == PairClass::DifferentSplit)
        .collect();
    splits.sort_by(|x, y| y.v.total_cmp(&x.v));
    listed(
        &mut out,
        "## the highest different-split pairs",
        splits.iter().copied().take(SPLITS_SHOWN),
    );
    let _ = writeln!(out, "\n{}", verdict_line(judged.outcome));
    out
}

/// The report's opening lines. The freeze is read from git again here so that
/// a report rewritten later still says what git says of the bar at that time.
fn report_head(stamped: &str, scores_stamp: &str, frozen: &FrozenBar) -> String {
    let commits = git(&["log", "--format=%H", "--", BAR_FILE]);
    let commit = git(&["log", "-1", "--format=%h %ct", "--", BAR_FILE]);
    let touched = one_commit("bar.json", commits.as_deref()).unwrap_or_else(|why| why);
    format!(
        "{stamped}\nscored by: {scores_stamp}\n{}\n",
        provenance_lines(&touched, commit.as_deref(), frozen)
    )
}

/// `members.tsv`, with how many entries of each member were passed over as not
/// being pages.
fn members_table(stamped: &str, read: &[Row], skipped: &[usize]) -> String {
    let table = members_tsv(stamped, read);
    let mut lines = table.lines();
    let (stamp_line, columns) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
    let mut out = format!("{stamp_line}\n{columns}\tskipped\n");
    for (line, skipped) in lines.zip(skipped) {
        let _ = writeln!(out, "{line}\t{skipped}");
    }
    out
}

/// Fingerprints every member once, scores every pair and writes the report.
/// A member that cannot be read whole, or a fingerprint this run did not take,
/// ends the run with no report, since a partial result could pass for a
/// whole one.
pub(super) fn score(
    library: &Path,
    ev: &Path,
    members: &[TestMember],
    frozen: &FrozenBar,
    verdicts: &[Adjudication],
    log: &mut Log,
) -> Result<Judged, String> {
    first_scoring(ev, verdicts)?;
    let skipped = left_out(library, members)?;
    let stamped = stamp("score_volume_test");
    let m = to_manifest(members);
    let (prints, read) = print_all(&ev.join("fp-cache"), library, &m);
    put(
        &ev.join("members.tsv"),
        &members_table(&stamped, &read, &skipped),
    );
    let printed = prints.iter().flatten().count();
    let hits = prints.iter().flatten().filter(|p| p.hits > 0).count();
    let unreadable = read.len() - printed;
    log.line(&format!(
        "members: {}, fingerprinted: {printed}, unreadable: {unreadable}, cache hits: {hits}",
        read.len()
    ));
    for line in read.iter().filter_map(unreadable_line) {
        log.line(&line);
    }
    if let Some(error) = read.iter().find_map(|row| row.error.as_ref()) {
        return Err(format!("{unreadable} members could not be read: {error}"));
    }
    let answered: Vec<&str> = (read.iter())
        .filter(|row| row.cache.is_some_and(|(hits, _)| hits > 0))
        .map(|row| row.path.as_str())
        .collect();
    if !answered.is_empty() {
        return Err(format!(
            "answered from the fingerprint cache, not from their pages (two members of the same \
             bytes share an entry): {}",
            answered.join(", ")
        ));
    }

    let listed: Vec<((usize, usize), Source)> = (all_pairs(members.len()).into_iter())
        .map(|pair| (pair, Source::Explicit))
        .collect();
    let (scored, unscored) = score_pairs(&m, &prints, &listed);
    if !unscored.is_empty() {
        return Err(format!("{} pairs were not scored", unscored.len()));
    }
    let rows = pair_rows(members, &scored);
    let table = scores_text(&stamped, &rows);
    put(&ev.join("scores.tsv"), &table);
    put(
        &ev.join(SCORES_DIGEST),
        &format!("{}  scores.tsv\n", sha256_hex(&table)),
    );

    let judged = judge(&rows, frozen.bar, verdicts)?;
    let head = report_head(&stamped, &stamped, frozen);
    let report = report_text(&head, frozen, &rows, verdicts, &judged);
    put(&ev.join("report.txt"), &report);
    put(&ev.join("report-first.txt"), &report);
    log.line(&verdict_line(judged.outcome));
    Ok(judged)
}

/// Refuses rows that are not every pair of the members once, each as the
/// rulings class it. A lost or reclassified wrong pair would otherwise read as
/// a pass.
fn holds_every_pair(rows: &[PairRow], members: &[TestMember]) -> Result<(), String> {
    let at: HashMap<&str, usize> = (members.iter().enumerate())
        .map(|(i, member)| (member.path.as_str(), i))
        .collect();
    let mut seen = BTreeSet::new();
    for row in rows {
        let pair = format!("{} and {}", row.a, row.b);
        let (Some(&i), Some(&j)) = (at.get(row.a.as_str()), at.get(row.b.as_str())) else {
            return Err(format!("{pair}: not a pair of the test's members"));
        };
        if i == j {
            return Err(format!("{pair}: a member against itself"));
        }
        if !seen.insert((i.min(j), i.max(j))) {
            return Err(format!("{pair}: in the table twice"));
        }
        let class = pair_class(&members[i], &members[j]);
        if row.class != class {
            return Err(format!("{pair}: classed {:?}, but is {class:?}", row.class));
        }
        let kind = edition_kind(&members[i], &members[j]);
        if row.edition_kind != kind {
            return Err(format!(
                "{pair}: edition kind {}, but is {kind}",
                row.edition_kind
            ));
        }
    }
    let missing = (all_pairs(members.len()).into_iter()).find(|pair| !seen.contains(pair));
    match missing {
        Some((i, j)) => Err(format!(
            "{} and {}: not in the table",
            members[i].path, members[j].path
        )),
        None => Ok(()),
    }
}

/// The table the run left, refused if it is not that table or does not hold
/// the test's pairs.
fn saved_scores(ev: &Path, members: &[TestMember]) -> Result<(String, Vec<PairRow>), String> {
    let file = ev.join("scores.tsv");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let digest = ev.join(SCORES_DIGEST);
    let recorded =
        std::fs::read_to_string(&digest).map_err(|e| format!("{}: {e}", digest.display()))?;
    let now = sha256_hex(&text);
    if recorded.split_whitespace().next() != Some(now.as_str()) {
        return Err(format!(
            "{} hashes to {now}, which is not what {SCORES_DIGEST} records",
            file.display()
        ));
    }
    let (scores_stamp, rows) =
        read_scores(&text).map_err(|why| format!("{}: {why}", file.display()))?;
    holds_every_pair(&rows, members).map_err(|why| format!("{}: {why}", file.display()))?;
    Ok((scores_stamp, rows))
}

/// Judges the saved scores again under the owner's verdicts. Only `report.txt`
/// is rewritten: `report-first.txt` keeps what the run said before anyone ruled.
pub(super) fn rejudge(
    ev: &Path,
    frozen: &FrozenBar,
    members: &[TestMember],
    verdicts: &[Adjudication],
) -> Result<Judged, String> {
    let (scores_stamp, rows) = saved_scores(ev, members)?;
    let judged = judge(&rows, frozen.bar, verdicts)?;
    let head = report_head(&stamp("rejudge_volume_test"), &scores_stamp, frozen);
    let report = report_text(&head, frozen, &rows, verdicts, &judged);
    put(&ev.join("report.txt"), &report);
    Ok(judged)
}

/// One pair as the owner is shown it, each A page beside the B page the
/// alignment paired it with.
fn sheet_pair(library: &Path, ev: &Path, m: &Manifest, row: &PairRow) -> Result<SheetPair, String> {
    let print = |path: &str| {
        let member = (m.members.iter())
            .find(|member| member.path == path)
            .ok_or_else(|| format!("{path}: not a member of the test"))?;
        print_member(&ev.join("fp-cache"), library, member)
    };
    let (a, b) = (print(&row.a)?, print(&row.b)?);
    let (_, runs) = aligned(&a.slots, &b.slots);
    let (a_pages, b_pages) = sheet_pages(&runs, &a.table, &b.table);
    Ok(SheetPair {
        a: row.a.clone(),
        b: row.b.clone(),
        question: Question::Same,
        a_pages,
        b_pages,
        a_count: a.pages,
        b_count: b.pages,
    })
}

/// `sheets/pair-NN.html` for each wrong pair at the bar no verdict has ruled
/// on, and how many were written. A pair's number counts every pair the bar
/// caught, ruled or not, so it keeps its file as verdicts arrive.
pub(super) fn sheets(
    library: &Path,
    ev: &Path,
    members: &[TestMember],
    frozen: &FrozenBar,
    verdicts: &[Adjudication],
) -> Result<usize, String> {
    let (_, rows) = saved_scores(ev, members)?;
    judge(&rows, frozen.bar, verdicts)?;
    let ruled: BTreeSet<usize> = (verdicts.iter())
        .filter_map(|verdict| ruled_row(&rows, verdict))
        .collect();
    let caught = judge(&rows, frozen.bar, &[])?.at_bar;
    let dir = ev.join("sheets");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let m = to_manifest(members);
    let mut written = 0;
    for (n, at) in caught.into_iter().enumerate() {
        if ruled.contains(&at) {
            continue;
        }
        let pair = sheet_pair(library, ev, &m, &rows[at])?;
        pairing_sheet(
            library,
            &dir.join(format!("pair-{:02}.html", n + 1)),
            &[pair],
        )?;
        written += 1;
    }
    Ok(written)
}
