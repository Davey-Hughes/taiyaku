use super::*;
use taiyaku_core::{Axis, Kind, Region, TruthRegion};

fn truth(x: f64, y: f64, w: f64, h: f64, kind: Kind) -> TruthRegion {
    TruthRegion {
        x,
        y,
        w,
        h,
        axis: Some(Axis::Vertical),
        kind,
        ignore: false,
        text: None,
    }
}

fn ignored(x: f64, y: f64, w: f64, h: f64) -> TruthRegion {
    TruthRegion {
        ignore: true,
        ..truth(x, y, w, h, Kind::Sfx)
    }
}

fn predicted(x: f64, y: f64, w: f64, h: f64) -> Region {
    Region {
        x,
        y,
        w,
        h,
        axis: Axis::Vertical,
    }
}

#[test]
fn one_prediction_over_one_truth_box_is_one_to_one() {
    let scored = score_page(
        &[truth(0.1, 0.1, 0.2, 0.2, Kind::Dialogue)],
        &[predicted(0.1, 0.1, 0.2, 0.2)],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::OneToOne]);
    assert_eq!(scored.spurious, 0);
}

#[test]
fn one_prediction_swallowing_three_truth_boxes_is_a_merge_not_three_misses() {
    // The failure `notes.md` found by cropping: the detector returns whole
    // panels. Under IoU@0.5 this is three misses and says nothing about
    // why. It must come back as three merges.
    let scored = score_page(
        &[
            truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
            truth(0.3, 0.1, 0.1, 0.1, Kind::Dialogue),
            truth(0.5, 0.1, 0.1, 0.1, Kind::Dialogue),
        ],
        &[predicted(0.05, 0.05, 0.6, 0.2)],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Merged; 3]);
    assert_eq!(
        scored.spurious, 0,
        "the prediction is accounted for, not spurious"
    );
}

#[test]
fn several_predictions_over_one_truth_box_is_a_split() {
    // The opposite failure on the same page: seven "blocks" that are
    // vertical slices of one word.
    let scored = score_page(
        &[truth(0.1, 0.1, 0.3, 0.1, Kind::Dialogue)],
        &[
            predicted(0.10, 0.1, 0.09, 0.1),
            predicted(0.21, 0.1, 0.09, 0.1),
            predicted(0.31, 0.1, 0.09, 0.1),
        ],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Split]);
}

#[test]
fn nothing_overlapping_a_truth_box_is_a_miss() {
    let scored = score_page(
        &[truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)],
        &[predicted(0.7, 0.7, 0.1, 0.1)],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Missed]);
    assert_eq!(scored.spurious, 1);
}

#[test]
fn a_prediction_over_nothing_is_spurious() {
    let scored = score_page(&[], &[predicted(0.7, 0.7, 0.1, 0.1)], COVERAGE);
    assert!(scored.outcomes.is_empty());
    assert_eq!(scored.spurious, 1);
}

#[test]
fn a_prediction_landing_on_an_ignored_box_is_neither_hit_nor_false_positive() {
    let scored = score_page(
        &[ignored(0.1, 0.1, 0.2, 0.2)],
        &[predicted(0.1, 0.1, 0.2, 0.2)],
        COVERAGE,
    );
    assert!(scored.outcomes.is_empty(), "an ignored box is not scored");
    assert_eq!(scored.spurious, 0, "and its prediction is not punished");
    assert_eq!(scored.ignored, 1, "but it is counted out loud");
}

#[test]
fn a_grazing_overlap_below_the_threshold_does_not_match() {
    let scored = score_page(
        &[truth(0.1, 0.1, 0.2, 0.2, Kind::Dialogue)],
        &[predicted(0.28, 0.28, 0.2, 0.2)],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Missed]);
    assert_eq!(scored.spurious, 1);
}

#[test]
fn an_empty_page_scores_nothing_and_does_not_panic() {
    let scored = score_page(&[], &[], COVERAGE);
    assert!(scored.outcomes.is_empty());
    assert_eq!(scored.spurious, 0);
    assert_eq!(scored.ignored, 0);
}

#[test]
fn a_report_tallies_outcomes_across_pages() {
    let mut report = Report::default();
    let page = [truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)];
    report.add_page(
        &page,
        &score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE),
    );
    report.add_page(&page, &score_page(&page, &[], COVERAGE));
    assert_eq!(report.one_to_one, 1);
    assert_eq!(report.missed, 1);
    assert_eq!(report.pages, 2);
}

#[test]
fn recall_is_reported_per_kind_so_dialogue_and_sfx_are_separable() {
    // The whole reason kinds were not collapsed into one class: a later
    // decision to stop caring about sound effects must cost no relabelling.
    let page = [
        truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
        truth(0.5, 0.5, 0.1, 0.1, Kind::Sfx),
    ];
    let mut report = Report::default();
    report.add_page(
        &page,
        &score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE),
    );
    assert_eq!(
        report.by_kind[&Kind::Dialogue],
        KindTally { found: 1, total: 1 }
    );
    assert_eq!(report.by_kind[&Kind::Sfx], KindTally { found: 0, total: 1 });
}

#[test]
fn only_one_to_one_counts_as_found() {
    // A merge and a split are failures, not partial credit. Counting them as
    // found would let a detector returning whole panels score as working,
    // which is the exact mistake blocks_per_page made.
    let page = [
        truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
        truth(0.3, 0.1, 0.1, 0.1, Kind::Dialogue),
    ];
    let mut report = Report::default();
    report.add_page(
        &page,
        &score_page(&page, &[predicted(0.05, 0.05, 0.4, 0.2)], COVERAGE),
    );
    assert_eq!(report.merged, 2);
    assert_eq!(
        report.by_kind[&Kind::Dialogue],
        KindTally { found: 0, total: 2 }
    );
}

#[test]
fn the_ignored_count_is_carried_so_the_hatch_is_never_silent() {
    let page = [ignored(0.1, 0.1, 0.1, 0.1)];
    let mut report = Report::default();
    report.add_page(&page, &score_page(&page, &[], COVERAGE));
    assert_eq!(report.ignored, 1);
    assert!(report.render().contains("ignored"), "and it is printed");
}

#[test]
fn spurious_is_a_rate_because_a_candidate_may_predict_more_in_total() {
    let mut report = Report::default();
    let page = [truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)];
    report.add_page(
        &page,
        &score_page(&page, &[predicted(0.8, 0.8, 0.1, 0.1)], COVERAGE),
    );
    report.add_page(&page, &score_page(&page, &[], COVERAGE));
    assert!((report.spurious_per_page() - 0.5).abs() < 1e-9);
}

#[test]
fn a_report_over_no_pages_renders_without_dividing_by_zero() {
    let report = Report::default();
    assert_eq!(report.spurious_per_page(), 0.0);
    assert!(!report.render().is_empty());
}

#[test]
fn a_whole_panel_prediction_over_one_balloon_is_a_merge_not_one_to_one() {
    // The regression this instrument exists for. Of the ten blocks
    // hand-cropped in `docs/notes.md:7046-7057`, five were confirmed by eye
    // to be whole panels and **three of those contain exactly one balloon**
    // — `gotoubun p139 b1` ("a figure on a bench, tone, one balloon"),
    // `gotoubun p158 b2` ("one balloon plus a black silhouette") and
    // `oshinoko p78 b0` ("one balloon plus art"). Decided on a count of how
    // many labelled boxes a prediction reaches, every one of those three
    // scores as a clean 1:1 hit, `add_page` credits it as `found`, and the
    // detector that returns panels ranks *better*. The prediction here
    // covers 0.25% of its own area with balloon; it is a merge.
    let scored = score_page(
        &[truth(0.4, 0.4, 0.05, 0.05, Kind::Dialogue)],
        &[predicted(0.0, 0.0, 1.0, 1.0)],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Merged]);
    assert_eq!(
        scored.spurious, 0,
        "the panel is accounted for by the box it swallowed, not spurious"
    );
}

#[test]
fn a_prediction_covering_a_sliver_of_a_truth_box_is_a_fragment() {
    // `gotoubun p102 b3`, 16×82: "a **letter fragment**, a vertical slice
    // of one word" (`docs/notes.md:7053`). The prediction lies wholly
    // inside the truth box and covers 0.25% of it. Association still
    // holds — it is plainly *about* this box — but it found a quarter of a
    // percent of it, which is a failure and not a hit.
    let scored = score_page(
        &[truth(0.1, 0.1, 0.4, 0.4, Kind::Dialogue)],
        &[predicted(0.2, 0.2, 0.02, 0.02)],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Fragment]);
    assert_eq!(scored.spurious, 0, "it is not a false positive either");
}

#[test]
fn a_prediction_spanning_a_scored_and_an_ignored_box_is_a_merge() {
    // `reach` counts scored boxes only, so a prediction swallowing one
    // balloon together with a sound effect flagged `ignore` reaches
    // exactly one scored box and reads 1:1 on a count. It is the same
    // over-merge, and the ignore flag forgives the prediction — it does
    // not turn it into a hit.
    let page = [
        truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
        ignored(0.3, 0.1, 0.1, 0.1),
    ];
    let scored = score_page(&page, &[predicted(0.05, 0.05, 0.4, 0.2)], COVERAGE);
    assert_eq!(scored.outcomes, vec![Outcome::Merged]);
    assert_eq!(scored.ignored, 1);
    assert_eq!(scored.spurious, 0);
}

#[test]
fn a_coverage_of_zero_does_not_match_disjoint_boxes() {
    // `COVERAGE` is provisional and meant to be swept. A ratio test alone
    // is vacuously true at zero, so the sweep's first step would report a
    // flawless page — every prediction matched to every truth box,
    // including boxes on opposite corners.
    let scored = score_page(
        &[truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)],
        &[predicted(0.7, 0.7, 0.1, 0.1)],
        0.0,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Missed]);
    assert_eq!(scored.spurious, 1);
}

#[test]
fn a_fragment_is_not_counted_as_found() {
    // Same rule as a merge and a split: a quarter of a percent of a box is
    // not partial credit, and recall must not move for it.
    let page = [truth(0.1, 0.1, 0.4, 0.4, Kind::Dialogue)];
    let mut report = Report::default();
    report.add_page(
        &page,
        &score_page(&page, &[predicted(0.2, 0.2, 0.02, 0.02)], COVERAGE),
    );
    assert_eq!(report.fragment, 1);
    assert_eq!(report.one_to_one, 0);
    assert_eq!(
        report.by_kind[&Kind::Dialogue],
        KindTally { found: 0, total: 1 }
    );
    let rendered = report.render();
    assert!(
        rendered.contains("split=0 fragment=1 missed=0"),
        "and it is printed beside merged/split/missed: {rendered}"
    );
    assert!(
        rendered.contains("/page) ignored=0"),
        "the wrapped format string keeps its space — a `\\` continuation \
             eats the newline *and* the indent that follows it: {rendered}"
    );
}

#[test]
fn a_zero_area_ignored_box_still_absorbs_its_prediction() {
    // A click without a drag on the ignore hatch. Refusing it would make
    // the one labelling slip that *forgives* a prediction punish it
    // instead, which is backwards.
    let page = [ignored(0.15, 0.15, 0.0, 0.0)];
    let scored = score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE);
    assert_eq!(scored.spurious, 0, "the mis-click still forgives");
    assert_eq!(scored.ignored, 1, "and is still counted out loud");

    let elsewhere = score_page(&page, &[predicted(0.7, 0.7, 0.1, 0.1)], COVERAGE);
    assert_eq!(
        elsewhere.spurious, 1,
        "but it does not forgive the whole page"
    );
}

#[test]
fn a_non_finite_coordinate_is_not_a_hit() {
    // `f64::max` and `f64::min` discard NaN, so the intersection of a NaN
    // box collapses onto the other box's own edges and the ratio comes out
    // 1.0 — corrupt input scoring as a perfect match.
    let scored = score_page(
        &[truth(f64::NAN, 0.1, 0.2, 0.2, Kind::Dialogue)],
        &[predicted(0.1, 0.1, 0.2, 0.2)],
        COVERAGE,
    );
    assert_eq!(scored.outcomes, vec![Outcome::Missed]);
    assert_eq!(scored.spurious, 1);
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

#[test]
fn score_with_no_arguments_prints_its_usage() {
    let err = command(&argv(&[])).unwrap_err();
    assert!(err.contains("usage"), "{err}");
}

#[test]
fn a_misspelled_flag_is_refused_rather_than_ignored() {
    // O21's rule: a flag typed with a typo must not be accepted in silence.
    let err = command(&argv(&["--sidecarr", "x", "page.jpg"])).unwrap_err();
    assert!(
        err.contains("--sidecarr"),
        "the message names the flag: {err}"
    );
}

#[test]
fn score_needs_a_sidecar_because_truth_is_recorded_against_an_edition() {
    let err = command(&argv(&["page.jpg"])).unwrap_err();
    assert!(err.contains("--sidecar"), "{err}");
}

#[test]
fn a_coverage_outside_zero_to_one_is_refused() {
    let err = command(&argv(&[
        "--sidecar",
        "t.json",
        "--coverage",
        "1.5",
        "p.jpg",
    ]))
    .unwrap_err();
    assert!(err.contains("coverage"), "{err}");
}

#[test]
fn sample_with_no_corpus_root_is_refused() {
    let err = command(&argv(&["--sample"])).unwrap_err();
    assert!(err.contains("--sample"), "{err}");
}

#[test]
fn sample_combined_with_sidecar_is_refused_and_names_the_conflict() {
    // O19, O20, O22's rule: a flag that cannot take effect is refused, not
    // silently ignored. --sample does not score anything, so --sidecar
    // has nothing to do under it.
    let err = command(&argv(&["--sample", "corpus", "--sidecar", "x"])).unwrap_err();
    assert!(err.contains("--sample"), "{err}");
    assert!(err.contains("--sidecar"), "{err}");
}

#[test]
fn sample_combined_with_coverage_is_refused_and_names_the_conflict() {
    let err = command(&argv(&["--sample", "corpus", "--coverage", "0.5"])).unwrap_err();
    assert!(err.contains("--sample"), "{err}");
    assert!(err.contains("--coverage"), "{err}");
}

/// O21: a misspelling must not be accepted in silence, --sample included.
#[test]
fn sample_with_a_misspelled_flag_is_refused() {
    let err = command(&argv(&["--sample", "corpus", "--covrage", "0.5"])).unwrap_err();
    assert!(err.contains("--covrage"), "{err}");
}

#[test]
fn sample_needs_the_corpus_root_itself_to_be_a_directory() {
    let err =
        sample_worklist(std::path::Path::new("/no/such/taiyaku-sample-corpus-root")).unwrap_err();
    assert!(err.contains("not a directory"), "{err}");
}

/// The failure this whole sub-project exists to fight: an edition that
/// cannot be read must be named on its own, not folded away into a bare
/// count or dropped from the worklist. Every `sample.rs` entry is missing
/// under this empty root, so every one of the fifteen must be named.
#[test]
fn a_missing_edition_directory_is_reported_by_name_not_skipped_silently() {
    let root = std::env::temp_dir().join("taiyaku-test-sample-missing-root");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create an empty corpus root");

    let err = sample_worklist(&root).expect_err("no sample.rs entry exists under this root");
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        err.contains("15 edition(s) could not be read"),
        "all nine dev and six sealed entries are missing: {err}"
    );
    assert!(
        err.contains("gotoubun-v01/raw-mono"),
        "a specific missing dev edition must be named: {err}"
    );
    assert!(
        err.contains("incoming/berserk-cleaned/Berserk.T01"),
        "a specific missing sealed edition must be named too: {err}"
    );
}

/// A scratch directory nobody else is using, emptied first in case a
/// previous run left one behind.
fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("taiyaku-test-score-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create a scratch directory");
    root
}

/// A one-page cbz whose page is a real, decodable image.
///
/// `command` runs the whole decode path — `read_members_in_order` then
/// `fingerprint::decode_all` — so the entry has to be bytes an image
/// decoder accepts, not a stand-in. `regions.rs`'s `cbz_with_page` is the
/// same fixture for the same reason; the entry name differs per member so
/// the two archives hash differently, which is the whole basis of every
/// order comparison below.
fn cbz_with_page(path: &std::path::Path, page: &str) {
    use std::io::Write;
    let image = image::GrayImage::from_pixel(4, 4, image::Luma([255]));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("a 4x4 gray image always encodes");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).expect("create cbz"));
    zip.start_file(page, zip::write::SimpleFileOptions::default())
        .expect("start entry");
    zip.write_all(&bytes).expect("write page bytes");
    zip.finish().expect("finish cbz");
}

/// A sidecar naming one edition made of `members`, **in the order given**.
fn write_sidecar(root: &std::path::Path, members: &[(&str, &str)]) -> std::path::PathBuf {
    let entries: Vec<String> = members
        .iter()
        .map(|(path, sha256)| {
            format!(r#"{{ "path": "{path}", "sha256": "{sha256}", "pages": 1 }}"#)
        })
        .collect();
    let path = root.join("taiyaku.json");
    std::fs::write(
        &path,
        format!(
            r#"{{
  "taiyaku": 1,
  "work": {{ "title": "t", "reading": "rtl" }},
  "editions": {{
    "raw-mono": {{ "label": "raw-mono", "members": [{}] }}
  }},
  "hub": "raw-mono",
  "alignments": []
}}"#,
            entries.join(", ")
        ),
    )
    .expect("write sidecar");
    path
}

/// The truth document beside `sidecar`, with `members` in the order the
/// browser writes them — the sidecar's own — and `pages` verbatim.
fn write_truth(sidecar: &std::path::Path, members: &[&str], pages: &str) {
    let entries: Vec<String> = members
        .iter()
        .map(|sha256| format!(r#"{{ "sha256": "{sha256}" }}"#))
        .collect();
    std::fs::write(
        taiyaku_core::TruthDoc::path_beside(sidecar),
        format!(
            r#"{{ "taiyaku_truth": 1, "editions": {{ "raw-mono": {{
                   "members": [{}], "pages": {pages} }} }} }}"#,
            entries.join(", ")
        ),
    )
    .expect("write truth document");
}

const ONE_BOX_ON_PAGE_0: &str =
    r#"{"0":[{"x":0.1,"y":0.1,"w":0.2,"h":0.2,"axis":"v","kind":"dialogue"}]}"#;

/// Two members whose sidecar order is not their sorted order, typed in
/// sorted order — and the good truth document beside them must not be
/// called stale.
///
/// `edition_of_digests` matches by **sorted set**, deliberately, so the
/// order the members were typed in does not decide *which* edition they
/// are — `score.rs`'s own comment says exactly that. `is_stale_for`
/// compares **element-wise**, deliberately, because content-in-order is
/// what says these labels describe these pages. Handing the argv-ordered
/// list to that element-wise check makes the two rules contradict each
/// other, and it refused a perfectly good document telling a person to
/// redo hours of labelling.
///
/// **Live on this corpus.** `corpus/gotoubun-v01`'s sidecar lists en-fan
/// as `…c004, c005, c005.5`, while `en-fan/*.cbz` expands to `…c004,
/// c005.5, c005` — `5` sorts before `c`. The fixture reproduces that
/// shape: sidecar order `b, a`, command line `a, b`. A fixture whose two
/// orders agreed could not tell the two lists apart at all.
#[test]
fn a_glob_ordered_command_line_does_not_make_a_good_truth_document_stale() {
    let root = scratch("glob-order");
    let a = root.join("a.cbz");
    let b = root.join("b.cbz");
    cbz_with_page(&a, "a1.png");
    cbz_with_page(&b, "b1.png");
    let digest_a = taiyaku_read::member_digest(&a).expect("digest a");
    let digest_b = taiyaku_read::member_digest(&b).expect("digest b");
    assert_ne!(digest_a, digest_b, "or no order can be told from another");

    // Sidecar order b, a — the reverse of sorted order.
    let sidecar = write_sidecar(&root, &[("b.cbz", &digest_b), ("a.cbz", &digest_a)]);
    // The browser writes the truth document's members in sidecar order.
    write_truth(&sidecar, &[&digest_b, &digest_a], ONE_BOX_ON_PAGE_0);

    // Typed a, b — sorted order, which is what a shell glob hands over.
    let result = command(&argv(&[
        "--sidecar",
        sidecar.to_str().expect("sidecar path is utf8"),
        a.to_str().expect("a path is utf8"),
        b.to_str().expect("b path is utf8"),
    ]));
    let _ = std::fs::remove_dir_all(&root);
    result.expect("a truth document drawn against these very archives is not stale");
}

/// And the guard is not weakened to get there: a member whose bytes are
/// not the ones the labels were drawn against is still refused.
///
/// Without this, dropping the staleness check entirely would pass the test
/// above. Stale ground truth "draws a box on the wrong art on the *right*
/// page" (`truth.rs`), which is the sharpest form of the problem here.
#[test]
fn a_replaced_archive_is_still_refused_after_the_order_fix() {
    let root = scratch("replaced");
    let a = root.join("a.cbz");
    let b = root.join("b.cbz");
    cbz_with_page(&a, "a1.png");
    cbz_with_page(&b, "b1.png");
    let digest_a = taiyaku_read::member_digest(&a).expect("digest a");
    let digest_b = taiyaku_read::member_digest(&b).expect("digest b");

    let sidecar = write_sidecar(&root, &[("b.cbz", &digest_b), ("a.cbz", &digest_a)]);
    // Drawn against a different second archive: same count, same order,
    // one digest that is not on disk any more.
    write_truth(
        &sidecar,
        &[
            &digest_b,
            "0000000000000000000000000000000000000000000000000000000000000000",
        ],
        ONE_BOX_ON_PAGE_0,
    );

    let err = command(&argv(&[
        "--sidecar",
        sidecar.to_str().expect("sidecar path is utf8"),
        a.to_str().expect("a path is utf8"),
        b.to_str().expect("b path is utf8"),
    ]))
    .unwrap_err();
    let _ = std::fs::remove_dir_all(&root);
    assert!(err.contains("drawn against different files"), "{err}");
}

/// A labelled page this edition cannot reach is named and refused, not
/// quietly left out of the count.
///
/// The filter that keeps only labelled pages cannot see a truth key past
/// the end — it simply never appears — so the report printed a smaller
/// `pages=` with nothing saying which page went unmeasured.
/// `score_corpus.rs` already fails by name on exactly this, and the
/// shipped command must not be the softer of the two.
#[test]
fn a_labelled_page_past_the_end_of_the_edition_is_named_rather_than_dropped() {
    let root = scratch("past-the-end");
    let a = root.join("a.cbz");
    cbz_with_page(&a, "a1.png");
    let digest_a = taiyaku_read::member_digest(&a).expect("digest a");

    let sidecar = write_sidecar(&root, &[("a.cbz", &digest_a)]);
    // One page in the edition; labels on page 0 and on page 7.
    write_truth(
        &sidecar,
        &[&digest_a],
        r#"{"0":[{"x":0.1,"y":0.1,"w":0.2,"h":0.2,"axis":"v","kind":"dialogue"}],
               "7":[{"x":0.1,"y":0.1,"w":0.2,"h":0.2,"axis":"v","kind":"dialogue"}]}"#,
    );

    let err = command(&argv(&[
        "--sidecar",
        sidecar.to_str().expect("sidecar path is utf8"),
        a.to_str().expect("a path is utf8"),
    ]))
    .unwrap_err();
    let _ = std::fs::remove_dir_all(&root);
    assert!(err.contains('7'), "the message names the page: {err}");
    assert!(
        err.contains("1 page(s)"),
        "and how long the edition actually is: {err}"
    );
}

/// The success path: a real edition directory of loose page images
/// resolves to its page count and the exact indices `dev_pages` picks —
/// the same function `region_experiment.rs`'s survey already samples
/// with, so the worklist stays a subset of it.
#[test]
fn a_resolved_edition_reports_its_page_count_and_the_pages_to_label() {
    let root = std::env::temp_dir().join("taiyaku-test-sample-resolved-root");
    let dir = root.join("gotoubun-v01").join("raw-mono");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&dir).expect("create an edition directory");

    let image = image::GrayImage::from_pixel(4, 4, image::Luma([255]));
    for i in 0..6 {
        image
            .save(dir.join(format!("p{i:03}.png")))
            .expect("a 4x4 gray image always encodes");
    }

    let result = resolve_sample_edition(&root, "gotoubun-v01", "raw-mono");
    let _ = std::fs::remove_dir_all(&root);
    let (available, pages) = result.expect("six loose page images resolve");

    assert_eq!(available, 6);
    assert_eq!(pages, crate::sample::dev_pages(6));
}
