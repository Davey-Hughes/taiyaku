//! The bar the test series are held to, taken only from the scores of the
//! first measurement.
//!
//! A bar that had seen the series it judges would predict nothing, so a score
//! touching a directory under test stops the derivation. Only a row of two
//! volumes has its label checked against the manifest those scores were made
//! under; any other row need only have its paths there.

use std::collections::HashMap;
use std::fmt::{Display, Write as _};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::super::features::volume_score;
use super::super::manifest::{Kind, Manifest, Member, label};
use super::super::rule::{dev_outcome, label_name};
use super::members::{ROWS, TestMember, all_pairs, pair_class};

/// The columns `score::scores_tsv` writes. Rows are read by position, so a
/// table of any other shape would put one column's number in another's place.
const COLUMNS: &str = "a\tb\tsource\tlabel\tvolume\tchapter\tpack\ta_slots\tb_slots\tverdict\t\
     separation\tmargin\tavg_run\tpaired\truns\tlong_paired\tspan_b\tV\tC\tms";

/// One row of the first measurement's `scores.tsv`. `class` is the volume
/// rule's cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pr1Row {
    pub a: String,
    pub b: String,
    pub label: String,
    pub class: String,
    pub verdict: String,
    pub v_cell: String,
    pub a_slots: u32,
    pub b_slots: u32,
    pub paired: u32,
    pub long_paired: u32,
    pub runs: usize,
}

impl Pr1Row {
    /// V from the integers: the printed cell is rounded, and a bar taken from
    /// it could sit just under a score it should keep out.
    pub(super) fn v(&self) -> f64 {
        volume_score(self.long_paired, self.a_slots, self.b_slots)
    }
}

pub(super) fn number<T: FromStr>(pair: &str, column: &str, text: &str) -> Result<T, String>
where
    T::Err: Display,
{
    (text.parse()).map_err(|e| format!("{pair}: {column} {text:?}: {e}"))
}

fn read_row(line: &str) -> Result<Pr1Row, String> {
    let cells: Vec<&str> = line.split('\t').collect();
    let &[
        a,
        b,
        _source,
        label,
        class,
        _chapter,
        _pack,
        a_slots,
        b_slots,
        verdict,
        _separation,
        _margin,
        _avg_run,
        paired,
        runs,
        long_paired,
        _span_b,
        v_cell,
        _c,
        _ms,
    ] = cells.as_slice()
    else {
        return Err(format!("{} cells in the row {line:?}", cells.len()));
    };
    let pair = format!("{a} and {b}");
    let row = Pr1Row {
        a: a.to_owned(),
        b: b.to_owned(),
        label: label.to_owned(),
        class: class.to_owned(),
        verdict: verdict.to_owned(),
        v_cell: v_cell.to_owned(),
        a_slots: number(&pair, "a_slots", a_slots)?,
        b_slots: number(&pair, "b_slots", b_slots)?,
        paired: number(&pair, "paired", paired)?,
        long_paired: number(&pair, "long_paired", long_paired)?,
        runs: number(&pair, "runs", runs)?,
    };
    let printed = format!("{:.6}", row.v());
    if row.v_cell != printed {
        return Err(format!(
            "{pair}: V is printed as {} but its integers give {printed}",
            row.v_cell
        ));
    }
    Ok(row)
}

/// Every row of such a `scores.tsv`, the stamp and the header first.
pub(super) fn read_pr1(text: &str) -> Result<Vec<Pr1Row>, String> {
    let mut lines = text.lines();
    if !lines.next().is_some_and(|stamp| stamp.starts_with("# ")) {
        return Err("the table does not open with a run's stamp".to_owned());
    }
    if lines.next() != Some(COLUMNS) {
        return Err("the table's columns are not the ones the scorer writes".to_owned());
    }
    lines.map(read_row).collect()
}

/// The bar and the negative it stands on.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Derived {
    pub max_negative: f64,
    pub bar: f64,
    pub pair: Pr1Row,
}

/// The bar over the volume rule's negatives among pairs of two volumes. The
/// first of two equal negatives is named, so a rerun names the same pair.
pub(super) fn derive(m: &Manifest, dev: &[Pr1Row], held_out: &[Pr1Row]) -> Result<Derived, String> {
    let at: HashMap<&str, &Member> = (m.members.iter())
        .map(|member| (member.path.as_str(), member))
        .collect();
    let member = |path: &str| {
        (at.get(path).copied()).ok_or_else(|| format!("{path}: not in PR 1's manifest"))
    };
    let mut negatives = Vec::new();
    for row in dev.iter().chain(held_out) {
        for path in [&row.a, &row.b] {
            let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
            if ROWS.iter().any(|under_test| under_test.dir == dir) {
                return Err(format!("{path}: PR 1 scored a series under test"));
            }
        }
        let (a, b) = (member(&row.a)?, member(&row.b)?);
        if a.kind != Kind::Volume || b.kind != Kind::Volume {
            continue;
        }
        let names = label_name(label(m, a, b));
        if row.label != names {
            return Err(format!(
                "{} and {}: labelled {} in the scores but {names} by the manifest",
                row.a, row.b, row.label
            ));
        }
        if row.class == "negative" {
            negatives.push(row);
        }
    }
    let highest =
        (negatives.iter().copied()).reduce(|best, row| if row.v() > best.v() { row } else { best });
    let Some(pair) = highest else {
        return Err("no bar: PR 1 scored no negative between two volumes".to_owned());
    };
    let scored: Vec<(bool, f64)> = negatives.iter().map(|row| (false, row.v())).collect();
    let Some(bar) = dev_outcome(&scored).bar else {
        return Err(format!("no bar fits above the negative at {}", pair.v()));
    };
    Ok(Derived {
        max_negative: pair.v(),
        bar,
        pair: pair.clone(),
    })
}

/// `bar.json`: the bar, the integers behind it, and the digests of what it
/// was frozen over, so a change to any of them after the freeze shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FrozenBar {
    pub max_negative: f64,
    pub gap: f64,
    pub bar: f64,
    pub pair_a: String,
    pub pair_b: String,
    pub long_paired: u32,
    pub a_slots: u32,
    pub b_slots: u32,
    pub pr1_dev_sha256: String,
    pub pr1_held_out_sha256: String,
    pub members_sha256: String,
    pub pairs_sha256: String,
}

pub(super) fn read_frozen(text: &str) -> Result<FrozenBar, String> {
    serde_json::from_str(text).map_err(|e| format!("bar.json: {e}"))
}

pub(super) fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Every pair and what it is, one `a<TAB>b<TAB>class` line each: the text the
/// freeze digests, so a class changed afterwards cannot pass unnoticed.
pub(super) fn pairs_text(members: &[TestMember]) -> String {
    let mut out = String::new();
    for (i, j) in all_pairs(members.len()) {
        let (a, b) = (&members[i], &members[j]);
        let _ = writeln!(out, "{}\t{}\t{:?}", a.path, b.path, pair_class(a, b));
    }
    out
}
