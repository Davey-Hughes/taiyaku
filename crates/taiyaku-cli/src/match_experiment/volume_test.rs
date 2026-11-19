//! Whether the volume rule's bar, frozen from the scores of other series,
//! holds on series this measurement has not scored
//! (`docs/superpowers/specs/2026-11-18-content-matching-volume-test-design.md`).
//!
//! The bar is a prediction only while these series stay unread, so nothing
//! here opens a member of them before the freeze: `draft_volume_test_members`
//! lists directory names alone, and the control reads only series already
//! scored.

use std::path::{Path, PathBuf};

use super::manifest::Adjudication;
use super::{Log, git, material, refuse_unusable, stamp};

mod bar;
mod control;
mod members;
mod run;
mod tests;
mod unread;

/// The first measurement's evidence directory. It must be set once the library
/// is, since a run that skipped what it holds would look like one that passed.
fn pr1_evidence() -> PathBuf {
    let Ok(pr1) = std::env::var("TAIYAKU_MATCH_PR1_EVIDENCE") else {
        panic!("TAIYAKU_MATCH_LIBRARY is set but TAIYAKU_MATCH_PR1_EVIDENCE is not");
    };
    refuse_unusable("TAIYAKU_MATCH_PR1_EVIDENCE", &pr1);
    PathBuf::from(pr1)
}

fn pr1_scores(pr1: &Path, split: &str) -> String {
    let file = pr1.join(split).join("scores.tsv");
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

fn bar_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(run::BAR_FILE)
}

fn verdicts() -> Vec<Adjudication> {
    serde_json::from_str(include_str!("volume_test/verdicts.json")).expect("verdicts.json parses")
}

/// What the bar is derived from, as the checkout and the first measurement's
/// evidence hold it.
fn freeze_inputs(pr1: &Path) -> run::Freeze {
    run::Freeze {
        manifest: serde_json::from_str(include_str!("manifest.json"))
            .expect("manifest.json parses"),
        pr1_dev_scores: pr1_scores(pr1, "dev"),
        pr1_held_out_scores: pr1_scores(pr1, "held-out"),
        members_json: include_str!("volume_test/members.json").to_owned(),
        members: members::committed(),
    }
}

/// The bar a finished run is read against, refused unless one commit froze it,
/// the rule derives it and nothing has changed it since: a verdict weighed
/// against a moved bar would judge nothing.
fn committed_bar() -> bar::FrozenBar {
    let file = bar_path();
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let freeze = freeze_inputs(&pr1_evidence());
    run::finished_bar(&text, &freeze, &run::git_facts(git)).unwrap_or_else(|why| panic!("{why}"))
}

/// Drafts the members from names alone into the committed `members.json`,
/// and prints each directory's count to be read against `ls`.
#[test]
fn draft_volume_test_members() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let members = members::draft(&material.library).unwrap_or_else(|why| panic!("{why}"));
    for row in &members::ROWS {
        let n = (members.iter())
            .filter(|m| m.directory() == row.dir)
            .count();
        println!("{n:5}  {}", row.dir);
    }
    println!("{:5}  in all", members.len());
    let out =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/match_experiment/volume_test/members.json");
    let json = serde_json::to_string_pretty(&members).expect("members serialise");
    std::fs::write(&out, json + "\n").unwrap_or_else(|e| panic!("{}: {e}", out.display()));
}

/// Rescores pairs the first measurement scored ([`control::run`]), reading only
/// its series.
#[test]
fn control_rescoring() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let pr1 = pr1_evidence();
    let ev = &material.evidence;
    let mut log = Log::create(&ev.join("logs/control.log"), &stamp("control_rescoring"));
    if let Err(why) = control::run(&material.library, ev, &pr1, &mut log) {
        log.line(&format!("control failed: {why}"));
        panic!("{why}");
    }
}

/// Writes `bar.json` from the first measurement's scores. A bar already there
/// is refused, because a second freeze could follow a look at the results.
#[test]
fn freeze_volume_bar() {
    if material().is_none() {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    }
    let pr1 = pr1_evidence();
    let out = bar_path();
    assert!(
        !out.exists(),
        "{}: the bar is already frozen, and is never frozen twice",
        out.display()
    );
    let text = (freeze_inputs(&pr1).text()).unwrap_or_else(|why| panic!("{why}"));
    std::fs::write(&out, &text).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    println!("{text}");
}

/// The measurement ([`run::measure`]), opened by the control ([`control::run`]).
#[test]
fn score_volume_test() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let pr1 = pr1_evidence();
    let compiled = match std::fs::read_to_string(bar_path()) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("{}: {e}", bar_path().display()),
    };
    let judged = run::measure(
        &material,
        compiled.as_deref(),
        &freeze_inputs(&pr1),
        &run::git_facts(git),
        &stamp("score_volume_test"),
        &verdicts(),
        |log| control::run(&material.library, &material.evidence, &pr1, log),
    );
    if let Err(why) = judged {
        panic!("{why}");
    }
}

/// Rewrites `report.txt` from the saved scores and the owner's verdicts.
#[test]
fn rejudge_volume_test() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let judged = run::rejudge(
        &material.evidence,
        &committed_bar(),
        &members::committed(),
        &verdicts(),
    )
    .unwrap_or_else(|why| panic!("{why}"));
    println!("{:?}", judged.outcome);
}

/// The pages of each wrong pair at the bar that the owner has yet to rule on.
#[test]
fn volume_test_sheets() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let written = run::sheets(
        &material.library,
        &material.evidence,
        &members::committed(),
        &committed_bar(),
        &verdicts(),
    )
    .unwrap_or_else(|why| panic!("{why}"));
    println!("{written} sheets");
}
