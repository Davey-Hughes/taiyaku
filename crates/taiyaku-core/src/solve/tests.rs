use super::*;

/// Composing an alignment through the hub is only as trustworthy as its
/// weakest edge, so the three verdicts have to compare. The declaration
/// order is the ranking, worst first, and `min` is what picks the weaker
/// of two edges.
#[test]
fn verdicts_rank_from_refused_up_to_sound() {
    assert!(Verdict::Refused < Verdict::Fragmented);
    assert!(Verdict::Fragmented < Verdict::Sound);
    assert_eq!(Verdict::Sound.min(Verdict::Fragmented), Verdict::Fragmented);
    assert_eq!(Verdict::Fragmented.min(Verdict::Refused), Verdict::Refused);
}

/// The verdict goes into a JSON document, so it needs a wire form, and it
/// must be the lowercase one the rest of the format already uses for
/// `Source`.
#[test]
fn a_verdict_round_trips_through_lowercase_json() {
    assert_eq!(
        serde_json::to_string(&Verdict::Fragmented).unwrap(),
        "\"fragmented\""
    );
    assert_eq!(
        serde_json::from_str::<Verdict>("\"sound\"").unwrap(),
        Verdict::Sound
    );
}

/// A sidecar records how many slots found a counterpart, so the count has
/// to be callable rather than buried inside `verdict`. Gap runs do not
/// count towards it.
#[test]
fn paired_slots_counts_only_the_runs_that_found_a_counterpart() {
    let runs = vec![
        Run::paired(PageRange::new(0, 9), PageRange::new(0, 9)),
        Run::gap(PageRange::new(10, 19)),
        Run::paired(PageRange::new(20, 24), PageRange::new(10, 14)),
    ];

    assert_eq!(paired_slots(&runs), 15);
    assert_eq!(paired_slots(&[Run::gap(PageRange::new(0, 9))]), 0);
}

/// A hash that is distinct per `seed` and stable across calls.
fn h(seed: u64) -> PageHash {
    let mut w = [0u64; 4];
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
    for slot in &mut w {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *slot = x;
    }
    PageHash(w)
}

/// Flip `bits` bits, to model a counterpart that is the same artwork seen
/// through a different scan, crop and translation.
fn degrade(p: PageHash, bits: u32) -> PageHash {
    let mut w = p.0;
    for k in 0..bits {
        w[(k / 64) as usize] ^= 1 << (k % 64);
    }
    PageHash(w)
}

/// Flip `bits` bits starting at `start`, so two degradations can be made
/// the same distance from a common original while differing from each
/// other. `degrade` always flips from bit zero, which cannot express that.
fn degrade_at(p: PageHash, start: u32, bits: u32) -> PageHash {
    let mut w = p.0;
    for k in start..start + bits {
        w[(k / 64) as usize] ^= 1 << (k % 64);
    }
    PageHash(w)
}

/// The hazard the report exists to make visible. Chapter divider pages
/// share a template, so one of them resembles all nine of its siblings.
/// Similarity says 0.98 and means nothing; only the margin over the next
/// candidate reveals that the choice between them was close to arbitrary.
#[test]
fn a_page_with_near_identical_rivals_scores_high_but_with_no_margin() {
    let template = h(1);
    // Four dividers barely distinguishable from each other, then one page
    // that resembles nothing else.
    let b: Vec<PageHash> = (0..4)
        .map(|k| degrade(template, k))
        .chain(std::iter::once(h(99)))
        .collect();
    let a = vec![b[0], b[4]];
    let runs = vec![Run::paired(PageRange::new(0, 1), PageRange::new(0, 1))];

    let scored = confidence(&a, &b, &runs);

    // Page 0 is paired with a near-twin of three other candidates.
    assert!(scored[0].similarity > 0.98, "{:?}", scored[0]);
    assert!(scored[0].margin < 0.02, "{:?}", scored[0]);
    // Page 1 was mis-paired by these hand-written runs; its real twin is
    // page 4, so the report must show the pairing it was actually given.
    assert_eq!(scored[1].counterpart, Some(1));
    assert!(scored[1].similarity < 0.7, "{:?}", scored[1]);
}

/// Some edition pairs are weak everywhere. Kaguya's raw against its English
/// release differs by a median of 57-69 bits of 256, with a tenth of pages
/// past 89 — similarity around 0.65 for real, correct matches. What makes a
/// match real is standing clear of noise, not clearing an absolute bar, so
/// a floor fixed in advance discards true pairings on the weaker editions.
#[test]
fn a_uniformly_weak_pairing_still_aligns() {
    let a: Vec<PageHash> = (0..6).map(h).collect();
    let b: Vec<PageHash> = a.iter().map(|p| degrade(*p, 97)).collect();
    // 97 of 256 bits => 0.621, well above the ~0.5 of unrelated pages.
    assert!((similarity(a[0], b[0]) - 0.621).abs() < 0.01);
    assert!(similarity(a[0], b[1]) < 0.60);

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 5), PageRange::new(0, 5))]
    );
}

/// The commonest real pattern: the translation carries front matter the raw
/// does not, so the whole volume sits at a constant offset. It must stay one
/// run — a constant offset is the case run-length encoding exists for.
#[test]
fn a_constant_offset_stays_a_single_run() {
    let a: Vec<PageHash> = (0..5).map(h).collect();
    let mut b = vec![h(50), h(51)];
    b.extend(a.iter().copied());

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 4), PageRange::new(2, 6))]
    );
}

/// Coverage has to gate fragmentation, and the reason is a hole that a
/// runs-per-page measure cannot see on its own: a refused pair produces
/// exactly one run covering everything, which scores about 0.9 runs per
/// hundred pages and therefore *better* than every correct alignment
/// measured. Judging on fragmentation alone calls a total refusal excellent.
#[test]
fn a_refusal_is_not_mistaken_for_a_clean_alignment() {
    // A refusal: one gap run over the whole edition.
    let refused = vec![Run::gap(PageRange::new(0, 209))];
    assert_eq!(verdict(&refused, 210), Verdict::Refused);

    // Go-Toubun volume 1 as actually solved: five runs, two of them gaps.
    let sound = vec![
        Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
        Run::gap(PageRange::new(1, 1)),
        Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
        Run::gap(PageRange::new(6, 6)),
        Run::paired(PageRange::new(7, 194), PageRange::new(5, 192)),
    ];
    assert_eq!(verdict(&sound, 195), Verdict::Sound);

    // A mislabelled copy: most pages paired, but in confetti. Shaped after
    // [Oshi no Ko] v01's bad copy at 34 runs per hundred pages.
    let fragmented: Vec<Run> = (0..70)
        .map(|i| {
            Run::paired(
                PageRange::new(i * 3, i * 3 + 2),
                PageRange::new(i * 3, i * 3 + 2),
            )
        })
        .collect();
    assert_eq!(verdict(&fragmented, 210), Verdict::Fragmented);
}

/// A pair where only a minority of A has counterparts is refused, and that
/// is deliberate rather than accidental. Estimating the signal from a high
/// percentile of best matches instead of the median does make this case
/// align, but it costs more than it buys: unrelated works stop being refused
/// and start pairing most of their pages, and genuine alignments degrade as
/// the raised floor rejects real matches. This test pins the refusal so the
/// trade is not made again by accident.
#[test]
fn a_pair_where_only_part_of_a_matches_is_refused() {
    // Ten pages with counterparts, thirty with none.
    let mut a: Vec<PageHash> = (0..10).map(h).collect();
    a.extend((100..130).map(h));
    let b: Vec<PageHash> = (0..10).map(|i| degrade(h(i), 40)).collect();

    assert_eq!(solve(&a, &b), all_unmatched(a.len()));

    // And a pair with nothing in common, likewise.
    let unrelated: Vec<PageHash> = (500..540).map(h).collect();
    assert_eq!(solve(&a, &unrelated), all_unmatched(a.len()));
}

/// An archive can legitimately contain no pages — a cbz holding only a
/// ComicInfo.xml, say. Every page of A is then unmatched, which is an
/// answer; panicking is not.
#[test]
fn an_empty_edition_leaves_every_page_unmatched() {
    let a: Vec<PageHash> = (0..3).map(h).collect();

    let runs = solve(&a, &[]);

    assert_eq!(runs, vec![Run::gap(PageRange::new(0, 2))]);
}

/// Two editions that are not the same volume must produce nothing, rather
/// than the best available nonsense. The library really does contain files
/// like this: two copies of [Oshi no Ko] v01 both claim the volume and only
/// one of them is it. Calibrating the floor from the data is what makes this
/// case dangerous — with no signal present, an estimator that always splits
/// the difference will happily certify noise.
#[test]
fn unrelated_editions_produce_no_pairings() {
    let a: Vec<PageHash> = (0..6).map(h).collect();
    let b: Vec<PageHash> = (100..106).map(h).collect();

    let runs = solve(&a, &b);

    assert_eq!(runs, vec![Run::gap(PageRange::new(0, 5))]);
}

/// A colour insert present in the raw and dropped from the translation:
/// the page must become a gap and the pages after it must keep matching,
/// at an offset. Naive index-for-index pairing gets everything after the
/// insert wrong, which is the whole reason sequencing is needed.
#[test]
fn a_page_absent_from_b_becomes_a_gap_and_shifts_the_rest() {
    let a: Vec<PageHash> = (0..4).map(h).collect();
    let b: Vec<PageHash> = vec![h(0), h(2), h(3)];

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 3), PageRange::new(1, 2)),
        ]
    );
}

#[test]
fn identical_sequences_collapse_to_one_paired_run() {
    let pages: Vec<PageHash> = (0..5).map(h).collect();

    let runs = solve(&pages, &pages);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 4), PageRange::new(0, 4))]
    );
}

/// Margin is what does not inflate with the length of B. A row's best is a
/// maximum over every candidate, so it climbs as candidates are added; the
/// distance from that best to the runner-up does not. No guard uses it:
/// calibration carries it so `align --stats` can print it, and the survey
/// that printed it is what rejected the guard.
#[test]
fn the_margin_is_the_median_gap_from_a_row_s_best_to_its_runner_up() {
    // Every A page has a counterpart 8 bits away and a decoy 40 bits away,
    // so every row's margin is exactly (40 - 8) / 256.
    let a: Vec<PageHash> = (1..=3).map(h).collect();
    let b: Vec<PageHash> = a
        .iter()
        .map(|p| degrade(*p, 8))
        .chain(a.iter().map(|p| degrade(*p, 40)))
        .collect();

    let cal = calibrate(&a, &b).expect("both sides have pages");

    assert!((cal.margin - 32.0 / 256.0).abs() < 1e-6, "{cal:?}");
    assert!(cal.signal > 0.95, "{cal:?}");
    assert!(cal.noise < 0.60, "{cal:?}");
    // The floor is still the midpoint of noise and signal, unchanged.
    assert!(
        (cal.floor - f32::midpoint(cal.noise, cal.signal)).abs() < 1e-6,
        "{cal:?}"
    );
}

/// Margin handling has two untested edge cases: a missing runner-up when B
/// is single-page, and a perfect tie between best and runner-up. Regression
/// in either would go undetected.
#[test]
fn the_margin_is_the_best_when_b_has_one_page_and_zero_for_tied_candidates() {
    // B with one page: no runner-up, so margin is the best similarity itself.
    let a_single = vec![h(1)];
    let b_single = vec![degrade(a_single[0], 20)];

    let cal_single = calibrate(&a_single, &b_single).expect("both sides have pages");
    let expected_margin = similarity(a_single[0], b_single[0]);
    assert!(
        (cal_single.margin - expected_margin).abs() < 1e-6,
        "{cal_single:?}"
    );

    // B with two equidistant pages: best and runner-up are tied, so margin is zero.
    let a_tie = vec![h(2)];
    let b_tie = vec![
        degrade(a_tie[0], 10),        // Bits 0-9
        degrade_at(a_tie[0], 10, 10), // Bits 10-19
    ];

    // Both pages score identically, verifying the tie setup.
    let s1 = similarity(a_tie[0], b_tie[0]);
    let s2 = similarity(a_tie[0], b_tie[1]);
    assert!((s1 - s2).abs() < 1e-6, "equidistant setup failed");

    let cal_tie = calibrate(&a_tie, &b_tie).expect("both sides have pages");
    assert!((cal_tie.margin - 0.0).abs() < 1e-6, "{cal_tie:?}");
}

/// Average paired-run length is the cheaper statement of what
/// fragmentation measures, and the survey needs it to decide whether it is
/// also the better one. Gap runs are excluded: a refused pair is one long
/// gap, and counting it would make a total refusal look like the longest
/// run in the corpus.
#[test]
fn average_run_length_counts_paired_runs_only() {
    let runs = vec![
        Run::paired(PageRange::new(0, 9), PageRange::new(0, 9)), // 10 slots
        Run::gap(PageRange::new(10, 109)),                       // ignored
        Run::paired(PageRange::new(110, 129), PageRange::new(10, 29)), // 20 slots
    ];

    assert!((average_run_length(&runs) - 15.0).abs() < 1e-6);

    // A refusal has no paired run at all, and must not divide by zero.
    let refused = vec![Run::gap(PageRange::new(0, 209))];
    assert_eq!(average_run_length(&refused), 0.0);
}

/// A threshold fixed in absolute slots cannot be met by an edition shorter
/// than the threshold. A legitimate twenty-page one-shot that aligns as one
/// perfect run averages twenty slots and would be called `Fragmented` by
/// arithmetic alone — and the constant says nothing about that size, being
/// measured on pairings of 146 slots and up. One clean run is the best shape
/// an alignment can have, so it has to read `Sound` at any length.
#[test]
fn a_short_edition_aligning_as_one_run_is_sound() {
    let runs = vec![Run::paired(PageRange::new(0, 19), PageRange::new(0, 19))];

    // Below the bare constant, which is what makes this the failing case.
    assert!(average_run_length(&runs) < f64::from(MIN_AVERAGE_RUN));
    assert_eq!(verdict(&runs, 20), Verdict::Sound);

    // The cap must not rescue a needle. Color Walk's nine art book pages
    // against a One Piece volume pair one slot at a time, which is confetti
    // at nine pages exactly as it is at nine hundred.
    let needle: Vec<Run> = (0..9)
        .map(|i| Run::paired(PageRange::new(i, i), PageRange::new(i * 20, i * 20)))
        .collect();
    assert_eq!(verdict(&needle, 9), Verdict::Fragmented);
}

/// The case that distinguishes the two fragmentation measures, which most
/// do not: twenty paired runs of two slots each across 210 pages is 9.5
/// runs per hundred, comfortably under the old threshold of 15 and so
/// called sound, while averaging two slots a run. Twenty runs of two is
/// confetti whatever the page count says, and the survey put every
/// confirmed-good pairing at 36 slots a run or better.
#[test]
fn a_short_run_average_is_fragmented_even_when_runs_per_page_looks_healthy() {
    // Named and typed so the ratio below can read it back losslessly with
    // f32::from rather than casting runs.len() and asserting it matches.
    let run_count: u16 = 20;
    let runs: Vec<Run> = (0..u32::from(run_count))
        .map(|i| {
            Run::paired(
                PageRange::new(i * 10, i * 10 + 1),
                PageRange::new(i * 10, i * 10 + 1),
            )
        })
        .collect();

    // The measure this replaces would have called it sound.
    let ratio = 100.0 * f32::from(run_count) / 210.0;
    assert!(ratio < 15.0);
    assert!(average_run_length(&runs) < 3.0);

    assert_eq!(verdict(&runs, 210), Verdict::Fragmented);
}

/// `percentile` is what turns the similarity matrix into `Calibration`'s
/// three medians, and every acceptance decision rests on those. The
/// index arithmetic is off-by-one-prone in both directions: the `- 1.0`
/// is what makes fraction 1.0 select the last element rather than run
/// past the end, and fraction 0.0 the first.
#[test]
fn percentile_selects_by_position_across_the_whole_range() {
    let values = [0.0f32, 10.0, 20.0, 30.0, 40.0];

    assert_eq!(percentile(&mut values.clone(), 0.0), 0.0);
    assert_eq!(percentile(&mut values.clone(), 0.5), 20.0);
    assert_eq!(percentile(&mut values.clone(), 1.0), 40.0);

    // Unsorted input must give the same answer: the function sorts.
    let mut shuffled = [30.0f32, 0.0, 40.0, 10.0, 20.0];
    assert_eq!(percentile(&mut shuffled, 0.5), 20.0);

    // A single value is its own median, and its own every percentile.
    assert_eq!(percentile(&mut [7.0f32], 0.0), 7.0);
    assert_eq!(percentile(&mut [7.0f32], 1.0), 7.0);

    // An even count has no middle element; the round is what picks a side.
    assert_eq!(percentile(&mut [0.0f32, 10.0, 20.0, 30.0], 0.5), 20.0);
    assert_eq!(median(&mut [0.0f32, 10.0, 20.0, 30.0]), 20.0);
}

/// `margin` is the one calibration statistic that does not inflate with
/// the length of B, and it is the difference between a row's best score
/// and its runner-up. Exact ties are not hypothetical: chapter dividers
/// share a template, and two identical pages in B score identically
/// against the same page of A. What this pins is that such a row reports a
/// margin of exactly zero — the reading `align --stats` prints and the
/// reason a divider is not mistaken for a confident match.
///
/// It does not pin the two `>` comparisons that produce it, and no test
/// can: relaxing either to `>=` is unobservable, for the reason given
/// beside them in `calibrate`.
#[test]
fn an_exact_tie_in_b_leaves_no_margin() {
    let page = h(1);
    // Two pages of B that are byte-identical, so both score exactly 1.0
    // against A's single page, plus one unrelated page.
    let b = vec![page, page, h(99)];
    let a = vec![page];

    let cal = calibrate(&a, &b).expect("both sides are non-empty");

    assert_eq!(cal.signal, 1.0, "{cal:?}");
    assert_eq!(cal.margin, 0.0, "{cal:?}");
}

/// The complement: with no tie, the margin is the real distance from best
/// to runner-up, and it must not be zero. Without this the test above
/// passes against an implementation that always reports zero.
#[test]
fn a_clear_winner_leaves_a_margin_of_the_real_distance() {
    let page = h(1);
    // 26 of 256 bits => the runner-up scores 1.0 - 26/256 = 0.8984.
    let b = vec![page, degrade(page, 26), h(99)];
    let a = vec![page];

    let cal = calibrate(&a, &b).expect("both sides are non-empty");

    assert_eq!(cal.signal, 1.0, "{cal:?}");
    assert!((cal.margin - 26.0 / 256.0).abs() < 1e-6, "{cal:?}");
}

/// The first column and first row of the score table say what it costs to
/// reach a cell having skipped that many pages, and the cost is negative
/// because skipping is a penalty. Delete the minus and skipping pays: the
/// solver then prefers to open with a long gap rather than pair from the
/// start. Every other fixture in this file has a pairing available from
/// page zero, so none of them can tell a penalty from a bonus.
#[test]
fn a_leading_run_of_unmatched_pages_stays_a_gap_and_the_rest_still_pairs() {
    // A opens with three pages B does not have, then eight it does.
    let shared: Vec<PageHash> = (10..18).map(h).collect();
    let mut a: Vec<PageHash> = vec![h(1), h(2), h(3)];
    a.extend(shared.iter().copied());
    let b: Vec<PageHash> = shared.iter().map(|p| degrade(*p, 20)).collect();

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![
            Run::gap(PageRange::new(0, 2)),
            Run::paired(PageRange::new(3, 10), PageRange::new(0, 7)),
        ],
        "a leading gap must cost, not pay"
    );
}

/// The score table is flattened by hand, and `at` is the stride. A wrong
/// stride makes rows overlap, which a square fixture cannot show — with
/// equal sides several wrong strides still land on the right cell often
/// enough to produce the right answer. This one is deliberately oblong:
/// eight pages of A against thirteen of B, with a repagination in the
/// middle so the diagonal bends.
#[test]
fn the_score_table_stride_survives_an_oblong_matrix() {
    let shared: Vec<PageHash> = (20..28).map(h).collect();
    let a: Vec<PageHash> = shared.clone();
    // B carries the same eight pages with five extras interleaved after
    // the third, so the alignment is two runs at different offsets.
    let mut b: Vec<PageHash> = shared[..3].iter().map(|p| degrade(*p, 20)).collect();
    b.extend((90..95).map(h));
    b.extend(shared[3..].iter().map(|p| degrade(*p, 20)));

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![
            Run::paired(PageRange::new(0, 2), PageRange::new(0, 2)),
            Run::paired(PageRange::new(3, 7), PageRange::new(8, 12)),
        ],
        "an oblong table must index the same as a square one"
    );
}

/// A weak opening pair is still worth making, and that is what the sign of
/// the border says. Gapping both sides costs two gap penalties, so a first
/// page sitting below the floor by less than that pairs anyway. Delete the
/// unary minus and the border becomes a bonus: reaching the interior by
/// skipping is then free money, so the solver opens with a gap it should
/// not take and the run starts a page late. The existing fixtures cannot
/// see this — every one of them has a pairing available from page zero
/// that wins by so much the border's sign never decides anything.
#[test]
fn a_weak_opening_pair_beats_gapping_both_sides() {
    let shared: Vec<PageHash> = (60..68).map(h).collect();
    let opener = h(59);
    let mut a: Vec<PageHash> = vec![opener];
    a.extend(shared.iter().copied());
    let mut b: Vec<PageHash> = vec![degrade(opener, 87)];
    b.extend(shared.iter().map(|p| degrade(*p, 20)));

    let cal = calibrate(&a, &b).expect("both sides are non-empty");
    let weak = similarity(a[0], b[0]);
    // The opener is below the floor, so pairing it scores negative...
    assert!(weak < cal.floor, "{weak} vs {cal:?}");
    // ...but by less than the two gap penalties that skipping both sides
    // costs, which is the whole reason it is still the better move.
    assert!(cal.floor - weak < 2.0 * GAP_PENALTY, "{weak} vs {cal:?}");

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 8), PageRange::new(0, 8))],
        "the border is a cost, so opening with a gap must not pay"
    );
}

/// `floor` is subtracted from every pairing score, which is what makes a
/// below-floor match lose to a gap. Added instead — or divided by — every
/// weak match becomes attractive and pages that correspond to nothing get
/// paired to whatever scored highest. Both editions have to carry their
/// own tail for this to show: a fixture where only A has spare pages
/// cannot use them, because there is nothing left in B to pair them to,
/// so the wrong sign produces the right answer anyway.
#[test]
fn a_page_below_the_floor_takes_a_gap_rather_than_a_weak_pairing() {
    let shared: Vec<PageHash> = (30..40).map(h).collect();
    let mut a: Vec<PageHash> = shared.clone();
    // A bonus chapter in the raw, unrelated to anything in B.
    a.push(h(500));
    a.push(h(501));
    let mut b: Vec<PageHash> = shared.iter().map(|p| degrade(*p, 20)).collect();
    // An afterword in the translation, unrelated to anything in A.
    b.push(h(600));
    b.push(h(601));

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![
            Run::paired(PageRange::new(0, 9), PageRange::new(0, 9)),
            Run::gap(PageRange::new(10, 11)),
        ],
        "pages below the floor must gap, not pair"
    );
}

/// Ties favour pairing, and the rule is `pair >= skip_a && pair >= skip_b`
/// — pairing must beat both alternatives, not either. With `||` a pairing
/// that ties one alternative and loses to the other still wins, which
/// splits a clean diagonal around equally-scored detours. Repeated pages
/// are what produce those ties: a volume with a run of identical blank
/// pages scores the same in several directions at once.
#[test]
fn a_stretch_of_identical_pages_stays_one_run_rather_than_splitting() {
    let blank = h(7);
    let mut a: Vec<PageHash> = (40..44).map(h).collect();
    // Six identical blanks in the middle: every cell in that block ties.
    a.extend(std::iter::repeat_n(blank, 6));
    a.extend((44..48).map(h));
    let b: Vec<PageHash> = a.iter().map(|p| degrade(*p, 20)).collect();

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 13), PageRange::new(0, 13))],
        "an identical stretch must not fragment the diagonal"
    );
}

/// The other half of the tie rule, and the half a stretch of ties cannot
/// reach: `&&` also refuses a pairing that beats one alternative while
/// losing to the other. Editions really do carry a page twice — a recap
/// thumbnail, a next-volume preview reusing the artwork — and against the
/// weaker copy pairing looks better than dropping A's page but worse than
/// dropping B's. With `||` the solver takes it, abandons the true
/// counterpart, and splits one clean run in two.
#[test]
fn a_weaker_echo_later_in_b_does_not_steal_the_pairing() {
    let shared: Vec<PageHash> = (70..78).map(h).collect();
    let a: Vec<PageHash> = shared.clone();
    let mut b: Vec<PageHash> = shared.iter().map(|p| degrade(*p, 20)).collect();
    // A second, degraded copy of A's last page at the back of B.
    b.push(degrade(a[7], 64));

    // The echo has to be worth pairing against a gap and not worth pairing
    // against the real counterpart, or the cell never poses the question.
    let cal = calibrate(&a, &b).expect("both sides are non-empty");
    let echo = similarity(a[7], b[8]);
    assert!(echo > cal.floor - 2.0 * GAP_PENALTY, "{echo} vs {cal:?}");
    assert!(echo < similarity(a[7], b[7]), "{echo} vs {cal:?}");

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 7), PageRange::new(0, 7))],
        "pairing must beat both alternatives, not either"
    );
}

/// Two gap runs separated by a paired stretch are two runs. `compress`
/// extends a gap only when it is the immediate successor of the previous
/// one; without that guard a volume with a colour insert at the front and
/// a bonus chapter at the back reports one gap spanning the whole book,
/// including everything paired in between.
///
/// The adjacency half of the guard needs its own case, because a pairing
/// that came from `solve` can never violate it — the traceback emits every
/// slot of A exactly once, in order. `correct_runs` can: a run list need
/// not cover its edition, and the slots it never mentions stay unmentioned,
/// so two gaps arrive with a hole between them.
#[test]
fn two_separated_gaps_do_not_merge_into_one() {
    let pairs: Vec<(u32, Option<u32>)> = vec![
        (0, None),
        (1, None),
        (2, Some(0)),
        (3, Some(1)),
        (4, None),
        (5, None),
    ];

    let runs = compress(&pairs);

    assert_eq!(
        runs,
        vec![
            Run::gap(PageRange::new(0, 1)),
            Run::paired(PageRange::new(2, 3), PageRange::new(0, 1)),
            Run::gap(PageRange::new(4, 5)),
        ]
    );

    // Slots 2 and 3 are absent from the pairing altogether, so the two
    // gaps must stay apart rather than swallowing pages nothing said
    // anything about.
    let sparse: Vec<(u32, Option<u32>)> = vec![(0, None), (1, None), (4, None), (5, None)];

    assert_eq!(
        compress(&sparse),
        vec![
            Run::gap(PageRange::new(0, 1)),
            Run::gap(PageRange::new(4, 5)),
        ]
    );
}

/// The margin is the distance from the chosen counterpart to the best
/// *other* candidate, so the counterpart must be excluded from the rival
/// search. Include it and the rival is the subject itself, which makes
/// every margin exactly zero — and zero margin is the report's danger
/// signal, so the whole verification contact sheet would light up red.
#[test]
fn the_chosen_counterpart_is_not_its_own_rival() {
    let page = h(1);
    // One page of A, two of B: an exact match and something unrelated.
    let a = vec![page];
    let b = vec![page, h(99)];
    let runs = vec![Run::paired(PageRange::new(0, 0), PageRange::new(0, 0))];

    let scored = confidence(&a, &b, &runs);

    assert_eq!(scored[0].counterpart, Some(0));
    assert_eq!(scored[0].similarity, 1.0, "{:?}", scored[0]);
    assert!(
        scored[0].margin > 0.3,
        "margin must measure distance to the *other* candidate: {:?}",
        scored[0]
    );
}
