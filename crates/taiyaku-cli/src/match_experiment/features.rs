//! What one pair of members looks like to the solver, and the two scores the
//! rules read from it.

use taiyaku_core::{
    PageHash, Run, Verdict, average_run_length, calibrate, paired_slots, solve, verdict,
};

/// The shortest paired run that counts towards V and C. Two unrelated members
/// still pair a few pages by chance, and chance seldom keeps going for four.
pub(super) const LONG_RUN: u32 = 4;

/// Everything reported for a pair. `a_slots` is the shorter side.
#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct PairFeatures {
    pub a_slots: u32,
    pub b_slots: u32,
    pub verdict: Verdict,
    pub separation: Option<f32>,
    pub margin: Option<f32>,
    pub avg_run: f64,
    pub paired: u32,
    pub long_paired: u32,
    pub span_b: u32,
    pub volume: f64,
    pub containment: f64,
}

/// The slots in paired runs of at least `min_len`, and the stretch of B those
/// runs cover, from the first one's start to the last one's end inclusive.
/// B's span reads the runs in order, which `solve`'s monotone runs always are.
pub(super) fn long_runs(runs: &[Run], min_len: u32) -> (u32, u32) {
    let long: Vec<_> = runs
        .iter()
        .filter(|run| run.a.len() >= min_len)
        .filter_map(|run| run.b)
        .collect();
    let paired = long.iter().map(taiyaku_core::PageRange::len).sum();
    let span = match (long.first(), long.last()) {
        (Some(first), Some(last)) => last.end - first.start + 1,
        _ => 0,
    };
    (paired, span)
}

/// V: low unless long runs cover most of both sides.
pub(super) fn volume_score(long_paired: u32, a: u32, b: u32) -> f64 {
    match a.max(b) {
        0 => 0.0,
        longer => f64::from(long_paired) / f64::from(longer),
    }
}

/// C: how much of A long runs cover, times how tightly they sit in B. A
/// chapter scattered across a whole volume covers itself but not tightly.
pub(super) fn containment_score(long_paired: u32, a: u32, span_b: u32) -> f64 {
    if a == 0 || span_b == 0 {
        return 0.0;
    }
    let lp = f64::from(long_paired);
    (lp / f64::from(a)) * (lp / f64::from(span_b))
}

/// The pair with its shorter side first, so a pair scores the same whichever
/// member names it first. Equal lengths fall back to the path, lexically.
pub(super) fn oriented<'x, T>(
    x: (&'x str, &'x [T]),
    y: (&'x str, &'x [T]),
) -> [(&'x str, &'x [T]); 2] {
    if (y.1.len(), y.0) < (x.1.len(), x.0) {
        [y, x]
    } else {
        [x, y]
    }
}

/// Align `a` against `b` and read off every feature. `a` is already the
/// shorter side (`oriented`).
pub(super) fn features(a: &[PageHash], b: &[PageHash]) -> PairFeatures {
    aligned(a, b).0
}

/// [`features`] and the runs they were read from, for a caller that reports
/// the run count or shows the pages paired: one solve serves both.
pub(super) fn aligned(a: &[PageHash], b: &[PageHash]) -> (PairFeatures, Vec<Run>) {
    debug_assert!(
        a.len() <= b.len(),
        "A must be the shorter side: {} slots against {}",
        a.len(),
        b.len()
    );
    let runs = solve(a, b);
    let calibration = calibrate(a, b);
    let a_slots = u32::try_from(a.len()).expect("a member's slot count fits u32");
    let b_slots = u32::try_from(b.len()).expect("a member's slot count fits u32");
    let (long_paired, span_b) = long_runs(&runs, LONG_RUN);
    let features = PairFeatures {
        a_slots,
        b_slots,
        verdict: verdict(&runs, a.len()),
        separation: calibration.map(|c| c.separation()),
        margin: calibration.map(|c| c.margin),
        avg_run: average_run_length(&runs),
        paired: paired_slots(&runs),
        long_paired,
        span_b,
        volume: volume_score(long_paired, a_slots, b_slots),
        containment: containment_score(long_paired, a_slots, span_b),
    };
    (features, runs)
}
