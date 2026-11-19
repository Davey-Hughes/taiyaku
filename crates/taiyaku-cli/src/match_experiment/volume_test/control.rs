//! Whether this code still gives the scores the bar was taken from.
//!
//! A bar can only be compared with new scores if the fingerprints and the
//! solver behind it have not moved, so pairs already scored are fingerprinted
//! again from nothing and must come out the same in every field.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use serde::Deserialize;

use super::super::Log;
use super::super::features::PairFeatures;
use super::super::lookup::{Print, print_member};
use super::super::manifest::{Manifest, Source};
use super::super::score::score_pairs;
use super::bar::{Pr1Row, derive, read_pr1};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    a: String,
    b: String,
}

/// The sampled pairs, as `control_pairs.json` lists them.
pub(super) fn pairs() -> Vec<(String, String)> {
    let listed: Vec<Pair> =
        serde_json::from_str(include_str!("control_pairs.json")).expect("the pairs parse");
    listed.into_iter().map(|pair| (pair.a, pair.b)).collect()
}

/// What in a fresh score differs from the row it repeats, with both values.
pub(super) fn differs(row: &Pr1Row, f: &PairFeatures, runs: usize) -> Option<String> {
    let fields = [
        ("V", row.v_cell.clone(), format!("{:.6}", f.volume)),
        ("a_slots", row.a_slots.to_string(), f.a_slots.to_string()),
        ("b_slots", row.b_slots.to_string(), f.b_slots.to_string()),
        ("paired", row.paired.to_string(), f.paired.to_string()),
        (
            "long_paired",
            row.long_paired.to_string(),
            f.long_paired.to_string(),
        ),
        ("runs", row.runs.to_string(), runs.to_string()),
        ("verdict", row.verdict.clone(), format!("{:?}", f.verdict)),
    ];
    let changed: Vec<String> = (fields.iter())
        .filter(|(_, then, now)| then != now)
        .map(|(name, then, now)| format!("{name} was {then} in PR 1 and is {now} now"))
        .collect();
    (!changed.is_empty()).then(|| changed.join(", "))
}

/// The one scored row for a pair, written either way round: it is oriented by
/// slot count, which the sample's order knows nothing of.
pub(super) fn find<'r>(
    a: &str,
    b: &str,
    dev: &'r [Pr1Row],
    held_out: &'r [Pr1Row],
) -> Result<&'r Pr1Row, String> {
    let mut rows = (dev.iter().chain(held_out))
        .filter(|row| (row.a == a && row.b == b) || (row.a == b && row.b == a));
    match (rows.next(), rows.next()) {
        (Some(row), None) => Ok(row),
        (None, _) => Err(format!("{a} and {b}: in neither of PR 1's scores")),
        (Some(_), Some(_)) => Err(format!("{a} and {b}: more than one row in PR 1's scores")),
    }
}

/// The sample must hold the negative that sets the bar: a control that passed
/// without it would say nothing about the one score the bar rests on.
pub(super) fn holds_the_pair(sample: &[(String, String)], pair: &Pr1Row) -> Result<(), String> {
    let held = (sample.iter())
        .any(|(a, b)| (*a == pair.a && *b == pair.b) || (*a == pair.b && *b == pair.a));
    if held {
        Ok(())
    } else {
        Err(format!(
            "the control's sample lacks {} and {}, the pair that sets the bar",
            pair.a, pair.b
        ))
    }
}

/// The `need`ed members' prints, by manifest position. A cache hit is refused:
/// an entry written by earlier code would reproduce that code's scores and
/// hide the very change the control is here to catch.
pub(super) fn fresh_prints(
    cache: &Path,
    library: &Path,
    m: &Manifest,
    need: &[usize],
) -> Result<Vec<Option<Print>>, String> {
    let read: Vec<(usize, Result<Print, String>)> = (need.par_iter())
        .map(|&k| (k, print_member(cache, library, &m.members[k])))
        .collect();
    let mut prints: Vec<Option<Print>> = m.members.iter().map(|_| None).collect();
    for (k, print) in read {
        let print = print?;
        if print.hits > 0 {
            return Err(format!(
                "{}: answered from the fingerprint cache, not from its pages",
                m.members[k].path
            ));
        }
        prints[k] = Some(print);
    }
    Ok(prints)
}

/// A directory no run has written to. `create_dir` refuses one that exists,
/// so two runs can never share entries.
fn new_cache(ev: &Path) -> Result<PathBuf, String> {
    let secs = (SystemTime::now().duration_since(UNIX_EPOCH))
        .map_err(|e| format!("the clock: {e}"))?
        .as_secs();
    let parent = ev.join("control");
    std::fs::create_dir_all(&parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let dir = parent.join(format!("fp-cache-{secs}"));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// Rescores the sample and compares it with the scored rows. Everything that can
/// be refused from the names is refused before the first page is read.
pub(super) fn run(library: &Path, ev: &Path, pr1: &Path, log: &mut Log) -> Result<(), String> {
    let m: Manifest = serde_json::from_str(include_str!("../manifest.json"))
        .map_err(|e| format!("manifest.json: {e}"))?;
    let scores = |split: &str| {
        let file = pr1.join(split).join("scores.tsv");
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        read_pr1(&text).map_err(|why| format!("{}: {why}", file.display()))
    };
    let (dev, held_out) = (scores("dev")?, scores("held-out")?);
    let derived = derive(&m, &dev, &held_out)?;
    let sample = pairs();
    holds_the_pair(&sample, &derived.pair)?;
    let rows = (sample.iter())
        .map(|(a, b)| find(a, b, &dev, &held_out))
        .collect::<Result<Vec<_>, _>>()?;

    let position = |path: &str| {
        (m.members.iter().position(|member| member.path == path))
            .ok_or_else(|| format!("{path}: not in PR 1's manifest"))
    };
    let listed = (sample.iter())
        .map(|(a, b)| Ok(((position(a)?, position(b)?), Source::Explicit)))
        .collect::<Result<Vec<_>, String>>()?;
    let need: BTreeSet<usize> = listed.iter().flat_map(|&((a, b), _)| [a, b]).collect();
    let need: Vec<usize> = need.into_iter().collect();

    let prints = fresh_prints(&new_cache(ev)?, library, &m, &need)?;
    let (scored, unscored) = score_pairs(&m, &prints, &listed);
    if !unscored.is_empty() {
        return Err(format!("{} pairs were not scored", unscored.len()));
    }
    let mut failures = Vec::new();
    for (score, row) in scored.iter().zip(&rows) {
        let pair = format!("{} and {}", row.a, row.b);
        match differs(row, &score.features, score.runs) {
            Some(why) => {
                log.line(&format!("differs: {pair}: {why}"));
                failures.push(format!("{pair}: {why}"));
            }
            None => log.line(&format!("reproduced: {pair}: V {}", row.v_cell)),
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "{} of {} pairs differ from PR 1: {}",
            failures.len(),
            rows.len(),
            failures.join("; ")
        ));
    }
    log.line(&format!("control: {0} of {0} pairs reproduced", rows.len()));
    Ok(())
}
