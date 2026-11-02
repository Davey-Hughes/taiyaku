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
/// Both fragmentation measures appear, and neither is meaningful without the
/// coverage beside it: a refused pair is one long gap run, which reads as
/// excellent on any runs-per-page scale.
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
/// case, because a table of measurements in `notes.md` records that
/// spelling verbatim.
pub fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Refused => "REFUSED",
        Verdict::Fragmented => "FRAGMENTED",
        Verdict::Sound => "sound",
    }
}

/// The verdict in words, for a person rather than for a survey.
///
/// Carries the average run length because that is the figure that separates
/// a confirmed-good pairing from one that must not pass. It is also the
/// figure a reader can act on, being in slots rather than per hundred pages,
/// so it does not have to be rescaled by the length of the edition before it
/// means anything.
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
mod tests;
