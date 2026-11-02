//! Where the component-size band goes, measured against the boxes a person drew.
//!
//! `detect.rs`'s own header calls a constant asserted as measured a defect.
//! This is the measurement for `BAND`: the whole dev set, scored at several
//! bands, so the value is read off a table rather than argued for.
//!
//! **The shape is `truth_population.rs`'s**, deliberately, rather than a third
//! convention: `#[cfg(test)]`, gated on `TAIYAKU_CORPUS`, skipping cleanly when
//! unset, reading its pages through `score::resolve_labelled` so it measures
//! **exactly the pages `taiyaku score` scores**, calling `detect.rs`'s own
//! `detect_page_with` rather than a staged copy that could drift, and asserting
//! only that it measured something — never a quality figure. A measurement that
//! fails when the thing it measures improves is a check pretending to be a
//! measurement.
//!
//! Scoring goes through `score::score_page` and `score::Report` at
//! `score::COVERAGE`, which is the same instrument, at the same threshold, as
//! `taiyaku score` and `scripts/score-dev-set.sh`. A second scorer here would be
//! two chances to disagree while both printed plausible numbers.
//!
//! **This is a one-parameter sweep over a fixed dev set, which is a mild overfit
//! and is stated as one.** The held-out pages are what would check it, and they
//! are not opened here.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli band_sweep -- --nocapture
//! ```
//!
//! **`--release` is not optional in practice**: `detect_page_with` runs a flood
//! fill over every pixel of a full-resolution scan, once per labelled page.

use std::path::PathBuf;

use crate::corpus::{corpus, members};
use crate::detect::{self, Band};
use crate::score::{COVERAGE, Report, resolve_labelled, score_page};

/// The nine editions of four works that carry labels, named rather than
/// globbed — `truth_population.rs`'s array, for its reason: a glob that stops
/// matching is the silent-nothing failure the assertions below refuse to have.
const EDITIONS: [(&str, &str); 9] = [
    ("gotoubun-v01", "raw-mono"),
    ("gotoubun-v01", "en-mono"),
    ("gotoubun-v01", "en-fan"),
    ("kaguya-v01", "raw-mono"),
    ("kaguya-v01", "en-mono"),
    ("kobayashi-v01", "raw-mono"),
    ("kobayashi-v01", "en-mono"),
    ("oshinoko-v01", "raw-mono"),
    ("oshinoko-v01", "en-mono"),
];

/// Bands to sweep, `lo` in ten-thousandths of page height.
///
/// **`lo = 0` is the control arm** — the low cut removed entirely — and it is
/// in the list on purpose. A sweep whose worst option is still a change cannot
/// report that the change did not help.
///
/// **It is not byte-for-byte the band-free incumbent**, and saying so would be
/// this file claiming more than it measures. `hi` is held at `BAND.hi` on every
/// row (see [`band`]), where `NO_BAND`'s `hi` is `u32::MAX`, so `lo = 0` here
/// is the incumbent's population *minus* components spanning more than a fifth
/// of page height — which is the one thing `hi` exists to drop, a panel border.
/// **And `hi` is not the only difference**: the detector around this row is a
/// two-axis union with a pixel-counting `MIN_INK`, which the incumbent's was
/// not (O34). It is the control for the question this sweep asks, which is
/// where `lo` goes.
///
/// The low end brackets the outside population's p95 (17‱) and median (6‱); the
/// high end goes well past them because spurious-per-page misses its ceiling at
/// 16 and nothing has measured where it crosses.
const SWEEP: [u32; 8] = [0, 8, 16, 32, 64, 128, 256, 512];

/// `hi` is held at `BAND.hi` throughout: one parameter at a time. `hi` is not
/// load-bearing — it exists to drop a panel border, one component spanning the
/// whole page at ~10,000 — so sweeping both at once would spend eight times the
/// decodes to answer a question about a constant nothing has said is wrong.
fn band(lo: u32) -> Band {
    Band {
        lo,
        hi: detect::BAND.hi,
    }
}

#[test]
fn where_the_band_goes() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };

    // One report per band, folded across all nine editions, because the
    // promotion rule is a sum over the whole dev set rather than a per-edition
    // verdict.
    let mut reports: Vec<Report> = SWEEP.iter().map(|_| Report::default()).collect();
    let mut editions_seen = 0usize;

    for (work, edition) in EDITIONS {
        let dir = root.join(work).join(edition);
        let sidecar = root.join(work).join("taiyaku.json");
        let paths: Vec<PathBuf> = members(&dir).into_iter().map(PathBuf::from).collect();
        let resolved = resolve_labelled(
            sidecar.to_str().expect("a corpus path is valid UTF-8"),
            &paths,
        )
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // **Every band is detected inside one decode**, which is the whole
        // reason this is an array rather than a loop around `decode_all`.
        // `decode_all` bounds memory by decoding the large pages one at a time
        // — the corpus holds large TIFFs — so the decoded pixels are not
        // available afterwards to run a second band over, and re-entering
        // `decode_all` once per band would decode every page at full resolution
        // each time. The `Region` lists it returns instead are small.
        //
        // Nothing is shared between the bands: each call is
        // `detect_page_with`'s own stages from the same `luma`, so the rows are
        // independent measurements rather than one run edited.
        let detected = crate::fingerprint::decode_all(&resolved.pages, |_, luma| {
            SWEEP.map(|lo| detect::detect_page_with(luma, band(lo)))
        })
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // `resolve_labelled` returns `truth` and `pages` parallel and ascending
        // and `decode_all` returns its results in page order, so zipping them
        // is what `score::command` itself does — no lookup by name is needed
        // here, because nothing is measured inside the closure.
        for (drawn, found) in resolved.truth.iter().zip(&detected) {
            for (report, regions) in reports.iter_mut().zip(found) {
                report.add_page(drawn, &score_page(drawn, regions, COVERAGE));
            }
        }

        println!("edition={work}/{edition} pages={}", detected.len());
        editions_seen += 1;
    }

    println!("\n  lo    1:1  merged   split    frag  missed  spurious   /page");
    for (lo, report) in SWEEP.iter().zip(&reports) {
        println!(
            "{lo:>4} {:>6} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7.2}",
            report.one_to_one,
            report.merged,
            report.split,
            report.fragment,
            report.missed,
            report.spurious,
            report.spurious_per_page(),
        );
    }

    // Per-kind beside the total, because they are different results: a
    // candidate that finds dialogue and no sound effects is not the same as one
    // finding a little of each, and that distinction is the evidence a later
    // decision about balloon-interior detection turns on (spec §3.4).
    println!("\nfound/drawn per kind, same runs:");
    for (lo, report) in SWEEP.iter().zip(&reports) {
        let kinds: Vec<String> = report
            .by_kind
            .iter()
            .map(|(kind, tally)| format!("{kind:?}={}/{}", tally.found, tally.total))
            .collect();
        println!("{lo:>4}  {}", kinds.join(" "));
    }

    // Assertions: that it measured something. Never a quality figure — those
    // move when the detector improves, and a check that fails on improvement is
    // not a measurement. The band this file exists to choose is chosen by a
    // person reading the table above against the promotion rule, not here.
    assert_eq!(editions_seen, EDITIONS.len(), "every named edition scored");
    for (lo, report) in SWEEP.iter().zip(&reports) {
        assert_eq!(report.pages, 45, "every labelled page scored at lo={lo}");
        assert!(
            !report.by_kind.is_empty(),
            "some truth box was scored at lo={lo}"
        );
    }
}
