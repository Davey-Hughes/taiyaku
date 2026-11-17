use super::*;

fn params(max_group: usize) -> BlockParams {
    BlockParams {
        max_group,
        skip_cost: 4.5,
        max_skip: 1,
        skip_extend: 4.5,
        band: 100,
    }
}

fn link(a: Range<usize>, b: Range<usize>) -> BlockLink {
    BlockLink { a, b }
}

fn by_length(a: &[u32], b: &[u32], max_group: usize) -> Vec<BlockLink> {
    let scorer = LengthScorer::new(a, b, GALE_CHURCH_VARIANCE);
    align_blocks(a.len(), b.len(), &scorer, params(max_group))
}

#[test]
fn equal_lengths_pair_one_to_one() {
    let lengths = [120, 40, 300, 75];
    assert_eq!(
        by_length(&lengths, &lengths, 4),
        [
            link(0..1, 0..1),
            link(1..2, 1..2),
            link(2..3, 2..3),
            link(3..4, 3..4)
        ]
    );
}

#[test]
fn a_paragraph_split_in_two_is_one_link_and_one_to_one_cannot_find_it() {
    let a = [200, 400, 200];
    let b = [200, 210, 190, 200];
    let right = [link(0..1, 0..1), link(1..2, 1..3), link(2..3, 3..4)];
    assert_eq!(by_length(&a, &b, 2), right);
    assert_ne!(by_length(&a, &b, 1), right);
}

#[test]
fn a_block_one_side_lacks_is_a_one_sided_link() {
    let shared = [
        200, 310, 150, 420, 260, 180, 330, 240, 390, 170, 280, 220, 360, 190, 300, 250, 410, 160,
        340, 230,
    ];
    let mut a = shared.to_vec();
    a.insert(10, 600);
    let links = by_length(&a, &shared, 4);
    assert!(links.contains(&link(10..11, 10..10)), "{links:?}");
    assert_eq!(links.len(), 21, "{links:?}");
}

#[test]
fn a_passage_one_side_lacks_is_left_out_whole_when_runs_are_cheap() {
    let shared = [
        200, 310, 150, 420, 260, 180, 330, 240, 390, 170, 280, 220, 360, 190, 300, 250, 410, 160,
        340, 230,
    ];
    let mut a = shared.to_vec();
    a.splice(10..10, [40, 35, 50, 45, 30, 40]);
    let scorer = LengthScorer::new(&a, &shared, GALE_CHURCH_VARIANCE);
    let one_by_one = align_blocks(a.len(), shared.len(), &scorer, params(4));
    let runs = align_blocks(
        a.len(),
        shared.len(),
        &scorer,
        BlockParams {
            max_skip: 8,
            skip_extend: 0.5,
            ..params(4)
        },
    );
    assert!(runs.contains(&link(10..16, 10..10)), "{runs:?}");
    assert!(
        !one_by_one.contains(&link(10..16, 10..10)),
        "{one_by_one:?}"
    );
}

#[test]
fn every_block_is_in_exactly_one_link_in_order() {
    let a = [10, 500, 30, 30, 220, 15, 15, 15, 400];
    let b = [12, 260, 250, 62, 210, 40, 380, 9, 9];
    let links = by_length(&a, &b, 3);
    let (mut next_a, mut next_b) = (0, 0);
    for l in &links {
        assert_eq!((l.a.start, l.b.start), (next_a, next_b), "{links:?}");
        assert!(!l.a.is_empty() || !l.b.is_empty());
        (next_a, next_b) = (l.a.end, l.b.end);
    }
    assert_eq!((next_a, next_b), (a.len(), b.len()));
}

#[test]
fn an_empty_side_is_all_one_sided_links() {
    let scorer = LengthScorer::new(&[], &[5, 6], GALE_CHURCH_VARIANCE);
    assert_eq!(
        align_blocks(0, 2, &scorer, params(2)),
        [link(0..0, 0..1), link(0..0, 1..2)]
    );
}

#[test]
fn the_ratio_is_fitted_from_the_totals() {
    let scorer = LengthScorer::new(&[100, 300], &[60, 180], GALE_CHURCH_VARIANCE);
    assert!((scorer.ratio() - 0.6).abs() < 1e-12);
    let cheap = scorer.cost(0..1, 0..1);
    let costly = scorer.cost(0..1, 1..2);
    assert!(cheap < costly, "{cheap} against {costly}");
}

#[test]
fn a_merge_costs_more_than_the_same_lengths_unmerged() {
    let scorer = LengthScorer::new(&[100, 100], &[100, 100], GALE_CHURCH_VARIANCE);
    let one = scorer.cost(0..1, 0..1);
    let merged = scorer.cost(0..2, 0..2);
    assert!(
        merged > one + 2.0 * GALE_CHURCH_MERGE - 1.0,
        "{one} {merged}"
    );
}

#[test]
fn ln_erfc_matches_known_values_and_stays_finite_far_out() {
    for (x, erfc) in [
        (0.0, 1.0_f64),
        (0.5, 0.479_500_122_186_953_5),
        (1.0, 0.157_299_207_050_285_1),
        (3.0, 2.209_049_699_858_544e-5),
    ] {
        assert!((ln_erfc(x) - erfc.ln()).abs() < 1e-6, "x = {x}");
    }
    assert!(ln_erfc(40.0).is_finite());
    assert!(ln_erfc(40.0) < ln_erfc(30.0));
}

#[test]
fn anchors_count_speech_questions_exclamations_ellipses_and_numbers() {
    assert_eq!(
        Anchors::of("“Shoo!” he said. “Is it 8:30? ‘Owls’ . . . and 1,000 stars…” It’s fine..."),
        Anchors {
            quotes: 3,
            questions: 1,
            exclamations: 1,
            ellipses: 3,
            numbers: 3,
        }
    );
    assert_eq!(Anchors::of("“글쎄, 난 그저…… 어쩌면……”").ellipses, 2);
    assert_eq!(Anchors::of("He stopped. Then he went on.").ellipses, 0);
}

#[test]
fn a_number_word_in_capitals_is_a_number_and_one_in_running_text_is_not() {
    assert_eq!(Anchors::of("CHAPTER THIRTEEN").numbers, 1);
    assert_eq!(Anchors::of("CHAPTER THIRTY-SEVEN").numbers, 1);
    assert_eq!(Anchors::of("13장 아주 비밀스러운 일기장").numbers, 1);
    assert_eq!(Anchors::of("Thirteen of them, and one more.").numbers, 0);
    assert_eq!(Anchors::of("NONE AT ONCE").numbers, 0);
}

#[test]
fn english_numbers_read_tens_before_ones() {
    assert_eq!(english_number("thirteen"), Some(13));
    assert_eq!(english_number("THIRTY-SEVEN"), Some(37));
    assert_eq!(english_number("ninety"), Some(90));
    assert_eq!(english_number("seven-thirty"), None);
    assert_eq!(english_number("twenty-zero"), None);
    assert_eq!(english_number("eleventy"), None);
}

#[test]
fn a_chapter_number_keeps_a_short_heading_with_its_chapter() {
    let (a, b) = heading_at_a_chapter_break();
    let lengths = |blocks: &[String]| -> Vec<u32> {
        blocks
            .iter()
            .map(|t| u32::try_from(t.chars().count()).unwrap())
            .collect()
    };
    let anchors =
        |blocks: &[String]| -> Vec<Anchors> { blocks.iter().map(|t| Anchors::of(t)).collect() };
    let length = LengthScorer::new(&lengths(&a), &lengths(&b), GALE_CHURCH_VARIANCE);
    let scorer = AnchorScorer::new(length.clone(), &anchors(&a), &anchors(&b), 1.0);
    let heading = link(1..3, 2..3);
    let with_anchors = align_blocks(4, 4, &scorer, params(4));
    let by_length = align_blocks(4, 4, &length, params(4));
    assert!(with_anchors.contains(&heading), "{with_anchors:?}");
    assert!(!by_length.contains(&heading), "{by_length:?}");
}

#[test]
fn texts_align_with_anchors_and_groups() {
    let (a, b) = heading_at_a_chapter_break();
    let a: Vec<&str> = a.iter().map(String::as_str).collect();
    let b: Vec<&str> = b.iter().map(String::as_str).collect();
    assert!(align_texts(&a, &b).contains(&link(1..3, 2..3)));
}

/// A paragraph, then `CHAPTER THIRTEEN` and its title, then a paragraph, in an
/// English edition and a Korean one that runs about 0.6 times as long.
fn heading_at_a_chapter_break() -> ([String; 4], [String; 4]) {
    let a = [
        "x".repeat(300),
        "CHAPTER THIRTEEN".to_string(),
        "THE VERY SECRET DIARY".to_string(),
        "y".repeat(300),
    ];
    let b = [
        "x".repeat(90),
        "x".repeat(95),
        "13장 아주 비밀스러운 일기장".to_string(),
        "y".repeat(180),
    ];
    (a, b)
}

#[test]
fn anchors_tip_a_choice_lengths_alone_cannot_make() {
    let a = [100, 100];
    let b = [100, 100];
    let (question, plain) = (Anchors::of("Why?"), Anchors::of("Because."));
    let length = LengthScorer::new(&a, &b, GALE_CHURCH_VARIANCE);
    let scorer = AnchorScorer::new(length.clone(), &[question, plain], &[plain, question], 1.0);
    assert!((length.cost(0..1, 0..1) - length.cost(0..1, 1..2)).abs() < 1e-12);
    assert!(scorer.cost(0..1, 1..2) < scorer.cost(0..1, 0..1));
}

#[test]
fn a_narrow_band_still_reaches_the_end() {
    let a = [50; 30];
    let b = [25; 60];
    let scorer = LengthScorer::new(&a, &b, GALE_CHURCH_VARIANCE);
    let links = align_blocks(
        a.len(),
        b.len(),
        &scorer,
        BlockParams {
            band: 0,
            ..params(2)
        },
    );
    assert_eq!(links.last().map(|l| (l.a.end, l.b.end)), Some((30, 60)));
    assert!(links.iter().all(|l| l.a.len() == 1 && l.b.len() == 2));
}

#[test]
fn the_band_holds_the_diagonal_and_not_far_from_it() {
    let band = Band::new(100, 200, 10);
    assert_eq!(band.centre(50), 100);
    assert!(band.holds(50, 110));
    assert!(!band.holds(50, 111));
    assert!(!band.holds(50, 89));
}

/// Five blocks a side, all one length, so unpinned they pair one to one.
fn even() -> (Vec<&'static str>, Vec<&'static str>) {
    (vec!["aaaaaaaaaa"; 5], vec!["bbbbbbbbbb"; 5])
}

#[test]
fn with_no_pins_the_alignment_is_align_texts() {
    let (a, b) = even();
    assert_eq!(align_texts_pinned(&a, &b, &[]), align_texts(&a, &b));
}

#[test]
fn a_pin_starts_a_link_on_both_sides_and_each_stretch_is_aligned_alone() {
    let (a, b) = even();
    let links = align_texts_pinned(&a, &b, &[(2, 3)]);
    assert!(
        links.iter().any(|l| l.a.start == 2 && l.b.start == 3),
        "{links:?}"
    );
    assert!(
        links.iter().all(|l| l.a.end <= 2 || l.a.start >= 2),
        "{links:?}"
    );
    assert!(
        links.iter().all(|l| l.b.end <= 3 || l.b.start >= 3),
        "{links:?}"
    );
    assert_eq!(links.first().map(|l| (l.a.start, l.b.start)), Some((0, 0)));
    assert_eq!(links.last().map(|l| (l.a.end, l.b.end)), Some((5, 5)));
}

#[test]
fn a_pin_that_leaves_one_side_nothing_before_it_makes_a_one_sided_link() {
    let (a, b) = even();
    let links = align_texts_pinned(&a, &b, &[(0, 2)]);
    assert_eq!(links[0], link(0..0, 0..2));
    assert_eq!(links[1].a.start, 0);
    assert_eq!(links[1].b.start, 2);
}

#[test]
fn a_pin_out_of_order_or_past_the_end_is_passed_over() {
    let (a, b) = even();
    let honoured = align_texts_pinned(&a, &b, &[(2, 3)]);
    assert_eq!(align_texts_pinned(&a, &b, &[(2, 3), (3, 1)]), honoured);
    assert_eq!(align_texts_pinned(&a, &b, &[(2, 3), (4, 9)]), honoured);
    assert_eq!(align_texts_pinned(&a, &b, &[(2, 3), (2, 3)]), honoured);
}

#[test]
fn two_empty_books_have_no_links() {
    assert_eq!(align_texts_pinned(&[], &[], &[]), []);
}

#[test]
fn a_pair_of_straight_quotes_is_one_quotation_as_a_curly_pair_is() {
    let curly = Anchors::of("“Who?” he said. “Me.”");
    let straight = Anchors::of("\"Who?\" he said. \"Me.\"");
    assert_eq!(straight.quotes, curly.quotes);
    assert_eq!(straight.quotes, 2);
    assert_eq!(
        Anchors::of("\"Who? he began").quotes,
        1,
        "opened, not yet closed"
    );
}
