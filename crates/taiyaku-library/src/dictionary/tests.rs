use super::*;

/// A few lines in kaikki.org's shape: a lemma with its forms, a sense that
/// is only a form of another word, and an entry with a romanization.
const SAMPLE: &str = r#"{"word":"hablar","pos":"verb","lang_code":"es","senses":[{"glosses":["to speak"]},{"glosses":["to talk"]}],"forms":[{"form":"hablaron","tags":["third-person","plural","preterite"]},{"form":"hablar","tags":["infinitive"]},{"form":"hablar-conj","tags":["table-tags"]}]}
{"word":"habló","pos":"verb","lang_code":"es","senses":[{"glosses":["third-person singular preterite indicative of hablar"],"form_of":[{"word":"hablar"}]}]}
{"word":"говорить","pos":"verb","lang_code":"ru","senses":[{"glosses":["to speak, to talk"]}],"forms":[{"form":"говори́т","tags":["present","third-person"]},{"form":"govorítʹ","tags":["romanization"]}]}
{"word":"言う","pos":"verb","lang_code":"ja","senses":[{"glosses":["to say"]}],"forms":[{"form":"いう","tags":["hiragana"]},{"form":"iu","tags":["romanization"]}]}
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
