use super::*;

fn split(text: &str) -> Vec<&str> {
    sentences(text).into_iter().map(|r| &text[r]).collect()
}

#[test]
fn a_sentence_ends_at_its_stop_and_the_space_after_it_is_neither_sentences() {
    assert_eq!(
        split("He came.  He saw! Did he win?"),
        ["He came.", "He saw!", "Did he win?"]
    );
}

#[test]
fn text_with_no_stop_is_one_sentence_and_no_text_is_none() {
    assert_eq!(split("  Chapter One "), ["Chapter One"]);
    assert!(sentences("   ").is_empty());
}

#[test]
fn a_quote_closing_after_the_stop_stays_with_its_sentence() {
    assert_eq!(
        split("“Me.” He ran. (Far away.) Then «non!» Bien."),
        ["“Me.”", "He ran.", "(Far away.)", "Then «non!»", "Bien."]
    );
}

#[test]
fn a_stop_before_a_lowercase_word_does_not_end_the_sentence() {
    // Dialogue attribution, and an ellipsis inside a sentence.
    assert_eq!(
        split("“Who?” said Harry. Well... maybe not. Yes."),
        ["“Who?” said Harry.", "Well... maybe not.", "Yes."]
    );
}

#[test]
fn a_dash_before_the_lowercase_word_is_looked_past() {
    assert_eq!(
        split("Кто там? — спросил он. Никто."),
        ["Кто там? — спросил он.", "Никто."]
    );
}

#[test]
fn an_initial_or_a_title_before_a_point_is_no_stop() {
    assert_eq!(
        split("Mr. Dursley met J. K. Rowling. She waved."),
        ["Mr. Dursley met J. K. Rowling.", "She waved."]
    );
}

#[test]
fn a_point_with_no_space_after_it_is_inside_a_word_or_a_number() {
    assert_eq!(
        split("It cost 3.50 at www.example.com today. Fine."),
        ["It cost 3.50 at www.example.com today.", "Fine."]
    );
}

#[test]
fn spanish_opening_marks_start_a_sentence() {
    assert_eq!(
        split("¿Quién eres? ¡Nadie! Bien."),
        ["¿Quién eres?", "¡Nadie!", "Bien."]
    );
}

#[test]
fn a_wide_stop_needs_no_space_and_a_quote_running_on_keeps_its_sentence() {
    assert_eq!(
        split("「何だ。」と言った。それから寝た。「よし。」「行こう！」"),
        [
            "「何だ。」と言った。",
            "それから寝た。",
            "「よし。」",
            "「行こう！」"
        ]
    );
}

#[test]
fn a_script_with_no_case_splits_at_every_stop_before_a_space() {
    assert_eq!(
        split("해리는 웃었다. 그리고 잤다."),
        ["해리는 웃었다.", "그리고 잤다."]
    );
}

fn blocks(texts: &[&str]) -> Vec<Vec<String>> {
    texts
        .iter()
        .map(|t| split(t).into_iter().map(str::to_string).collect())
        .collect()
}

fn link(a: std::ops::Range<usize>, b: std::ops::Range<usize>) -> BlockLink {
    BlockLink { a, b }
}

#[test]
fn sentences_pair_inside_their_blocks_link_numbered_through_the_book() {
    let a = blocks(&[
        "One short. Then a much longer second sentence here.",
        "Alone.",
    ]);
    let b = blocks(&[
        "Uno corto. Y luego una segunda frase mucho más larga aquí.",
        "Sola.",
    ]);
    let links = align_sentences(&a, &b, &[link(0..1, 0..1), link(1..2, 1..2)]);
    assert_eq!(
        links,
        [link(0..1, 0..1), link(1..2, 1..2), link(2..3, 2..3)]
    );
}

#[test]
fn a_block_with_no_counterpart_leaves_its_sentences_out_together() {
    let a = blocks(&["A note. Of two.", "Shared."]);
    let b = blocks(&["Compartida."]);
    let links = align_sentences(&a, &b, &[link(0..1, 0..0), link(1..2, 0..1)]);
    assert_eq!(links, [link(0..2, 0..0), link(2..3, 0..1)]);
}

#[test]
fn one_sentence_against_several_is_one_link_of_them_all() {
    // Past the most a step may group, and no sentence of either is extra.
    let a = blocks(&["One. Two. Three. Four. Five. Six."]);
    let b = blocks(&["Una sola frase que lo dice todo de una vez."]);
    let links = align_sentences(&a, &b, &[link(0..1, 0..1)]);
    assert_eq!(links, [link(0..6, 0..1)]);
}

#[test]
fn a_sentence_one_edition_ends_a_paragraph_with_pairs_where_the_other_starts_the_next() {
    // The Spanish runs "Beta dos." on at the end of its first paragraph; the
    // English starts its second with it.
    let a = blocks(&[
        "Alpha is the first sentence, and a long one.",
        "Beta two. Gamma is the third sentence here.",
    ]);
    let b = blocks(&[
        "Alfa es la primera frase, y una larga. Beta dos.",
        "Gamma es la tercera frase aquí.",
    ]);
    let links = align_sentences(&a, &b, &[link(0..1, 0..1), link(1..2, 1..2)]);
    assert_eq!(
        links,
        [link(0..1, 0..1), link(1..2, 1..2), link(2..3, 2..3)]
    );
}

#[test]
fn a_lopsided_pairing_never_joins_a_passage_the_book_left_out() {
    let a = blocks(&["Alpha here. Beta here.", "A note the other lacks. Of two."]);
    let b = blocks(&["Alfa y beta aquí."]);
    let links = align_sentences(&a, &b, &[link(0..1, 0..1), link(1..2, 1..1)]);
    assert_eq!(links, [link(0..2, 0..1), link(2..4, 1..1)]);
}

#[test]
fn lopsided_pairings_join_no_more_than_four_block_links() {
    // Three links that each pair one sentence with two fill a pairing up to
    // the fourth, whose sentence the Spanish runs into the fifth's paragraph:
    // the pairing stops there, which bounds its cost, so that one stays as
    // the book's links drew it.
    let a = blocks(&[
        "One.",
        "Two.",
        "Three.",
        "Alpha is the first sentence, and a long one.",
        "Beta two. Gamma is the third sentence here.",
    ]);
    let b = blocks(&[
        "Uno. Un.",
        "Dos. Do.",
        "Tres. Tr.",
        "Alfa es la primera frase, y una larga. Beta dos.",
        "Gamma es la tercera frase aquí.",
    ]);
    let block_links: Vec<BlockLink> = (0..5).map(|i| link(i..i + 1, i..i + 1)).collect();
    let links = align_sentences(&a, &b, &block_links);
    assert!(
        links.iter().any(|l| (l.a.start, l.b.start) == (4, 8)),
        "the fifth block link's sentences start a pairing of their own: {links:?}"
    );
}
