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
/// paired. It is classified `Fragmented`, and the average run length is the
/// figure that carries the argument.
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
