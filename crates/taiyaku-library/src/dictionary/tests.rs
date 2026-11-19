use super::*;

/// A few lines in kaikki.org's shape: a lemma with its forms, a sense that
/// is only a form of another word, and an entry with a romanization. Some
/// headwords are written with a combining mark or a variation selector inside
/// them, escaped (U+E0100 as its UTF-16 pair), and one spans punctuation.
/// No headword is a variant of another by a selector at its end.
const SAMPLE: &str = r#"{"word":"hablar","pos":"verb","lang_code":"es","senses":[{"glosses":["to speak"]},{"glosses":["to talk"]}],"forms":[{"form":"hablaron","tags":["third-person","plural","preterite"]},{"form":"hablar","tags":["infinitive"]},{"form":"hablar-conj","tags":["table-tags"]}]}
{"word":"habló","pos":"verb","lang_code":"es","senses":[{"glosses":["third-person singular preterite indicative of hablar"],"form_of":[{"word":"hablar"}]}]}
{"word":"говорить","pos":"verb","lang_code":"ru","senses":[{"glosses":["to speak, to talk"]}],"forms":[{"form":"говори́т","tags":["present","third-person"]},{"form":"govorítʹ","tags":["romanization"]}]}
{"word":"言う","pos":"verb","lang_code":"ja","senses":[{"glosses":["to say"]}],"forms":[{"form":"いう","tags":["hiragana"]},{"form":"iu","tags":["romanization"]}]}
{"word":"食べる","pos":"verb","lang_code":"ja","senses":[{"glosses":["to eat"]}],"forms":[{"form":"食べました","tags":["polite","past"]},{"form":"食べ","tags":["stem"]}]}
{"word":"は","pos":"particle","lang_code":"ja","senses":[{"glosses":["topic marker"]}]}
{"word":"葛","pos":"noun","lang_code":"ja","senses":[{"glosses":["kudzu"]}]}
{"word":"葛\uDB40\uDD00城","pos":"name","lang_code":"ja","senses":[{"glosses":["Katsuragi"]}]}
{"word":"辻","pos":"noun","lang_code":"ja","senses":[{"glosses":["crossroads"]}]}
{"word":"𠮷野","pos":"name","lang_code":"ja","senses":[{"glosses":["Yoshino"]}]}
{"word":"ありがとう","pos":"intj","lang_code":"ja","senses":[{"glosses":["thank you"]}]}
{"word":"まと","pos":"noun","lang_code":"ja","senses":[{"glosses":["target"]}]}
{"word":"か\u3099っこう","pos":"noun","lang_code":"ja","senses":[{"glosses":["school"]}]}
{"word":"ジョン・スミス","pos":"name","lang_code":"ja","senses":[{"glosses":["John Smith"]}]}
{"word":"pingu\u0308ino","pos":"noun","lang_code":"es","senses":[{"glosses":["penguin"]}]}
{"word":"ああああああああああああああああ","pos":"noun","lang_code":"ja","senses":[{"glosses":["sixteen"]}]}
{"word":"あああああああああああああああああ","pos":"noun","lang_code":"ja","senses":[{"glosses":["seventeen"]}]}
not json at all
{"word":"Casa","pos":"noun","lang_code":"es","senses":[{"glosses":["house"]},{"glosses":["house","home, household"]},{"glosses":["house"]},{"glosses":["a"]},{"glosses":["b"]},{"glosses":["c"]},{"glosses":["d"]}]}"#;

fn built() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory");
    let report = import(&conn, SAMPLE.as_bytes()).expect("imports");
    assert_eq!(report.skipped, 1, "the line that is not JSON");
    conn
}

fn glosses(found: &[Found]) -> Vec<(String, String, Option<String>)> {
    found
        .iter()
        .map(|f| (f.word.clone(), f.glosses.join("; "), f.via.clone()))
        .collect()
}

#[test]
fn a_word_is_found_as_itself_with_its_senses() {
    let conn = built();
    assert_eq!(
        glosses(&lookup(&conn, "hablar").expect("looks up")),
        [("hablar".into(), "to speak; to talk".into(), None)]
    );
}

#[test]
fn an_inflected_form_finds_its_headword_and_says_which_form_led_there() {
    let conn = built();
    for form in ["hablaron", "habló", "Habló"] {
        assert_eq!(
            glosses(&lookup(&conn, form).expect("looks up")),
            [(
                "hablar".into(),
                "to speak; to talk".into(),
                Some(form.to_lowercase())
            )],
            "{form}"
        );
    }
}

#[test]
fn russian_stress_marks_and_case_match_the_plain_print() {
    let conn = built();
    assert_eq!(
        glosses(&lookup(&conn, "Говорит").expect("looks up")),
        [(
            "говорить".into(),
            "to speak, to talk".into(),
            Some("говорит".into())
        )]
    );
}

#[test]
fn a_reading_finds_its_word_and_a_romanization_is_no_form() {
    let conn = built();
    assert_eq!(
        glosses(&lookup(&conn, "いう").expect("looks up"))[0].0,
        "言う"
    );
    assert!(lookup(&conn, "iu").expect("looks up").is_empty());
    assert!(lookup(&conn, "hablar-conj").expect("looks up").is_empty());
}

#[test]
fn a_word_keeps_its_first_senses_and_the_punctuation_round_it_is_not_looked_up() {
    let conn = built();
    let found = lookup(&conn, "«casa,»").expect("looks up");
    assert_eq!(found.len(), 1);
    // A sub-sense's own gloss, not its parent's again, and no sense twice.
    assert_eq!(
        found[0].glosses,
        ["house", "home, household", "a", "b", "c"]
    );
}

#[test]
fn a_word_not_held_is_found_nowhere() {
    assert!(lookup(&built(), "zzz").expect("looks up").is_empty());
}

fn at(text: &str) -> (usize, Vec<(String, String, Option<String>)>) {
    let (length, found) = longest_at(&built(), text).expect("looks up");
    (length, glosses(&found))
}

fn words(conn: &Connection, text: &str) -> Vec<String> {
    lookup(conn, text)
        .expect("looks up")
        .into_iter()
        .map(|f| f.word)
        .collect()
}

#[test]
fn the_existing_route_looks_a_word_up_without_the_selector_after_it() {
    let conn = built();
    assert_eq!(
        words(&conn, "葛\u{E0100}"),
        ["葛"],
        "an ideograph's variant"
    );
    assert_eq!(
        words(&conn, "ありがとう❤\u{FE0F}"),
        ["ありがとう"],
        "an emoji's presentation"
    );
}

#[test]
fn a_variant_headword_filed_under_its_base_is_found_by_its_own_spelling() {
    let conn = built();
    let variant = serde_json::json!({
        "word": "葛\u{E0100}", "pos": "name", "senses": [{"glosses": ["Katsu"]}]
    });
    import(&conn, variant.to_string().as_bytes()).expect("imports");
    // Filed by hand, not left to `key`: a dictionary already on disk holds
    // the variant under its base, whatever `key` would file it under now.
    conn.execute(
        "UPDATE entry SET key = '葛' WHERE word = ?1",
        ["葛\u{E0100}"],
    )
    .expect("files the variant");
    assert_eq!(words(&conn, "葛\u{E0100}"), ["葛", "葛\u{E0100}"]);
}

#[test]
fn a_match_takes_in_the_variation_selectors_after_it() {
    assert_eq!(
        at("葛\u{E0100}"),
        (2, vec![("葛".into(), "kudzu".into(), None)])
    );
    assert_eq!(
        at("辻\u{E0101}堂").0,
        2,
        "an ideographic variation selector"
    );
    assert_eq!(at("辻\u{FE00}堂").0, 2, "a standardized variation selector");
    assert_eq!(at("辻\u{FE00}\u{E0101}堂").0, 3, "every selector after it");
}

#[test]
fn a_variation_selector_inside_a_headword_continues_the_word() {
    assert_eq!(
        at("葛\u{E0100}城へ"),
        (3, vec![("葛\u{E0100}城".into(), "Katsuragi".into(), None)])
    );
}

#[test]
fn a_match_never_ends_before_a_combining_mark() {
    assert_eq!(at("まと\u{3099}が"), (0, vec![]), "まど written decomposed");
    assert_eq!(at("まと\u{309A}が"), (0, vec![]));
    assert_eq!(
        at("辻\u{FE00}\u{3099}"),
        (0, vec![]),
        "a mark after the selectors the match would take in"
    );
}

/// Russian as a learner's book prints it, stress marked: each word, and a
/// shorter word or form its first letters also spell.
const STRESSED: &str = r#"{"word":"молоко","pos":"noun","senses":[{"glosses":["milk"]}]}
{"word":"вода","pos":"noun","senses":[{"glosses":["water"]}],"forms":[{"form":"вод","tags":["genitive","plural"]}]}
{"word":"говорить","pos":"verb","senses":[{"glosses":["to speak"]}],"forms":[{"form":"говорю","tags":["first-person"]}]}
{"word":"говор","pos":"noun","senses":[{"glosses":["dialect"]}]}"#;

fn stressed(text: &str) -> (usize, Vec<String>) {
    let conn = Connection::open_in_memory().expect("in-memory");
    import(&conn, STRESSED.as_bytes()).expect("imports");
    let (length, found) = longest_at(&conn, text).expect("looks up");
    (length, found.into_iter().map(|f| f.word).collect())
}

#[test]
fn a_stress_mark_after_a_headword_is_taken_into_its_length() {
    assert_eq!(stressed("молоко\u{301}"), (7, vec!["молоко".to_string()]));
}

#[test]
fn a_stress_mark_after_a_word_does_not_leave_the_shorter_form_inside_it() {
    assert_eq!(stressed("вода\u{301}"), (5, vec!["вода".to_string()]));
}

#[test]
fn a_stress_mark_after_a_form_does_not_leave_the_shorter_word_inside_it() {
    assert_eq!(stressed("говорю\u{301}"), (7, vec!["говорить".to_string()]));
}

#[test]
fn a_grave_stress_mark_is_taken_in_as_the_acute_is() {
    assert_eq!(stressed("молоко\u{300}"), (7, vec!["молоко".to_string()]));
}

#[test]
fn a_combining_mark_inside_a_headword_continues_the_word() {
    assert_eq!(at("か\u{3099}っこうへ").0, 5, "a kana voicing mark");
    assert_eq!(at("pingu\u{308}ino es").0, 9, "a diacritic");
}

#[test]
fn punctuation_inside_a_headword_ends_the_word() {
    assert_eq!(at("ジョン・スミス"), (0, vec![]));
}

#[test]
fn the_longest_form_a_text_begins_with_wins_over_its_shorter_stem() {
    assert_eq!(
        at("食べました。今日"),
        (
            5,
            vec![("食べる".into(), "to eat".into(), Some("食べました".into()))]
        )
    );
    assert_eq!(
        at("食べ物"),
        (
            2,
            vec![("食べる".into(), "to eat".into(), Some("食べ".into()))]
        )
    );
    assert_eq!(
        at("はい"),
        (1, vec![("は".into(), "topic marker".into(), None)])
    );
}

#[test]
fn a_prefix_cannot_start_with_punctuation_and_an_astral_char_counts_once() {
    assert_eq!(at("。食べる"), (0, vec![]));
    assert_eq!(at("𠮷野家").0, 2);
    assert_eq!(at(""), (0, vec![]));
}

#[test]
fn a_prefix_stops_at_whitespace() {
    assert_eq!(
        at("hablaron mucho"),
        (
            8,
            vec![(
                "hablar".into(),
                "to speak; to talk".into(),
                Some("hablaron".into())
            )]
        )
    );
}

#[test]
fn a_prefix_the_key_would_change_is_skipped() {
    assert_eq!(at("Говорит").0, 7, "case alone does not make a key differ");
    assert_eq!(
        at("говори\u{301}т").0,
        0,
        "a stress mark is stripped by the key"
    );
    assert_eq!(at("говорит, да").0, 7);
}

#[test]
fn only_the_first_sixteen_chars_are_considered() {
    let (length, found) = at(&"あ".repeat(17));
    assert_eq!(length, 16, "the 17-char headword is out of reach");
    assert_eq!(found[0].1, "sixteen");
    let sixteen = "あ".repeat(16);
    assert_eq!(
        at(&format!("{sixteen}\u{FE0F}")).0,
        17,
        "a selector after the sixteenth char is still taken in"
    );
    assert_eq!(
        at(&format!("{sixteen}\u{3099}")),
        (0, vec![]),
        "a mark after the sixteenth char still keeps the match from ending there"
    );
}
