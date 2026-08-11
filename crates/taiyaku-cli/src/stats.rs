//! The `--stats` lines: what calibration saw, and what shape came out.
//!
//! Formatting lives in its own module and returns strings rather than printing
//! them, because these two lines are the entire evidence base for two tuning
//! constants and a format nobody can test is a format that drifts.

use taiyaku_core::{Calibration, Run, Verdict, average_run_length, verdict};

/// What calibration measured, whether or not it accepted the pair.
///
/// Fixed field order and three decimals throughout, so a survey can collect
/// dozens of these into a table with `grep` and read down a column.
pub fn calibration_line(cal: Option<&Calibration>, a_len: usize, b_len: usize) -> String {
    match cal {
        None => format!("stats: a={a_len} b={b_len} no calibration: a side has no pages"),
        Some(cal) => format!(
            "stats: a={a_len} b={b_len} noise={:.3} signal={:.3} sep={:.3} margin={:.3} \
             floor={:.3} accepted={}",
            cal.noise,
            cal.signal,
            cal.separation(),
            cal.margin,
            cal.floor,
            if cal.accepted() { "yes" } else { "no" }
        ),
    }
}

/// What the alignment came out looking like.
///
/// Both fragmentation measures appear because the survey exists to choose
/// between them, and neither is meaningful without the coverage beside it: a
/// refused pair is one long gap run, which reads as excellent on any
/// runs-per-page scale.
pub fn shape_line(runs: &[Run], a_len: usize) -> String {
    let paired = taiyaku_core::paired_slots(runs);
    #[expect(
        clippy::cast_precision_loss,
        reason = "runs.len() and a_len count alignment runs and pages for one pairing; real \
                  editions run to at most a few hundred pages and runs, far below f32's 2^24 \
                  exact-integer bound, so the conversion loses no precision — a \
                  property of the material, not enforced by the type (both are usize with no \
                  cap). The lossless-From route is closed too: both are usize, and std has \
                  no From<usize> for f32 — usize's width is platform-dependent, \
                  unlike u32/u16/u8, so no such impl exists."
    )]
    let per_hundred = 100.0 * runs.len() as f32 / a_len.max(1) as f32;
    let call = match verdict(runs, a_len) {
        Verdict::Refused => "Refused",
        Verdict::Fragmented => "Fragmented",
        Verdict::Sound => "Sound",
    };
    format!(
        "shape: runs={} paired={paired} avg_run={:.1} per_100={per_hundred:.1} verdict={call}",
        runs.len(),
        average_run_length(runs)
    )
}

/// How a verdict is written for a person.
///
/// Shouted for the two that want attention and lowercase for the one that does
/// not, so in prose output — `lookup`'s trust line and `verdict_sentence` —
/// capitals mean "look at this". This is the form for a person and only that:
/// the machine-readable `shape:` line keeps the enum's own spelling, title
/// case, because `scripts/margin-survey.sh` greps `verdict=Sound` out of it and
/// a table of measurements in `notes.md` records that spelling verbatim.
pub fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Refused => "REFUSED",
        Verdict::Fragmented => "FRAGMENTED",
        Verdict::Sound => "sound",
    }
}

/// The verdict in words, for a person rather than for a survey.
///
/// Carries the average run length because that is the figure that states the
/// property directly: measured across the corpus, every confirmed-good pairing
/// runs 36 slots or more and everything that must not pass 6.8 or fewer. It is
/// also the figure a reader can act on, being in slots rather than per hundred
/// pages, so it does not have to be rescaled by the length of the edition
/// before it means anything.
pub fn verdict_sentence(runs: &[Run], a_len: usize) -> String {
    match verdict(runs, a_len) {
        // No paired run, so there is no average worth quoting.
        Verdict::Refused => format!(
            "{} - these editions do not correspond",
            verdict_word(Verdict::Refused)
        ),
        Verdict::Fragmented => format!(
            "{} - paired, but too broken up to trust | average run {:.1} slots",
            verdict_word(Verdict::Fragmented),
            average_run_length(runs)
        ),
        Verdict::Sound => format!(
            "{} | average run {:.1} slots",
            verdict_word(Verdict::Sound),
            average_run_length(runs)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taiyaku_core::PageRange;

    #[test]
    fn the_calibration_line_reports_every_figure_that_decided_the_answer() {
        let cal = Calibration {
            noise: 0.518,
            signal: 0.856,
            margin: 0.213,
            floor: 0.687,
        };

        let line = calibration_line(Some(&cal), 195, 187);

        assert_eq!(
            line,
            "stats: a=195 b=187 noise=0.518 signal=0.856 sep=0.338 margin=0.213 \
             floor=0.687 accepted=yes"
        );
    }

    /// An empty edition has no calibration at all, and the line still has to
    /// say something a survey loop can parse rather than vanishing.
    #[test]
    fn an_uncalibratable_pair_still_produces_a_line() {
        let line = calibration_line(None, 3, 0);

        assert_eq!(line, "stats: a=3 b=0 no calibration: a side has no pages");
    }

    #[test]
    fn the_shape_line_reports_runs_coverage_and_both_fragmentation_measures() {
        // Go-Toubun volume 1 as actually solved: five runs, two of them gaps.
        let runs = vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
            Run::gap(PageRange::new(6, 6)),
            Run::paired(PageRange::new(7, 194), PageRange::new(5, 192)),
        ];

        let line = shape_line(&runs, 195);

        assert_eq!(
            line,
            "shape: runs=5 paired=193 avg_run=64.3 per_100=2.6 verdict=Sound"
        );
    }

    /// The needle case: nine art book pages against a whole volume, every one
    /// paired. It is already classified `Fragmented` and the classification was
    /// never printed in the path the documentation tells people to use. The
    /// average run length is the figure that carries the argument — the survey
    /// measured every correct pairing at 36 slots or more.
    #[test]
    fn a_fragmented_alignment_says_so_and_shows_how_short_its_runs_are() {
        let runs: Vec<Run> = (0..9)
            .map(|i| Run::paired(PageRange::new(i, i), PageRange::new(i * 20, i * 20)))
            .collect();

        assert_eq!(
            verdict_sentence(&runs, 9),
            "FRAGMENTED - paired, but too broken up to trust | average run 1.0 slots"
        );
    }

    #[test]
    fn a_sound_alignment_reports_its_average_run() {
        let runs = vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
            Run::gap(PageRange::new(6, 6)),
            Run::paired(PageRange::new(7, 194), PageRange::new(5, 192)),
        ];

        assert_eq!(
            verdict_sentence(&runs, 195),
            "sound | average run 64.3 slots"
        );
    }

    /// A refusal has no paired run, so an average would be meaningless and is
    /// left off rather than printed as zero.
    #[test]
    fn a_refusal_says_so_without_quoting_an_average_of_nothing() {
        assert_eq!(
            verdict_sentence(&[Run::gap(PageRange::new(0, 209))], 210),
            "REFUSED - these editions do not correspond"
        );
    }
}
