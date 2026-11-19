use super::*;
use crate::dictionary::import;

fn line(word: &str, pos: &str, gloss: &str, forms: &[&str]) -> String {
    let forms: Vec<String> = forms
        .iter()
        .map(|f| format!(r#"{{"form":"{f}","tags":["inflection"]}}"#))
        .collect();
    format!(
        r#"{{"word":"{word}","pos":"{pos}","lang_code":"ko","senses":[{{"glosses":["{gloss}"]}}],"forms":[{}]}}"#,
        forms.join(",")
    )
}

fn dictionary_of(lines: &[String]) -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory");
    import(&conn, lines.join("\n").as_bytes()).expect("imports");
    conn
}

fn built() -> Connection {
    let lines = [
        line("해리", "name", "Harry", &[]),
        line("학교", "noun", "school", &[]),
        line("친구", "noun", "friend", &[]),
        line("먹다", "verb", "to eat", &["먹었습니다", "먹어요"]),
        line("집", "noun", "house", &[]),
        line("층", "noun", "floor", &[]),
        line("길", "noun", "road", &[]),
        line("Harry", "name", "a given name", &[]),
        line("학생", "noun", "student", &[]),
        line("솔다", "verb", "to narrow", &["소는"]),
        line("소", "noun", "cow", &[]),
    ];
    dictionary_of(&lines)
}

fn headwords(conn: &Connection, word: &str) -> Vec<(String, Option<String>)> {
    lookup_korean(conn, word)
        .expect("looks up")
        .into_iter()
        .map(|f| (f.word, f.via))
        .collect()
}

fn found(word: &str, via: Option<&str>) -> Vec<(String, Option<String>)> {
    vec![(word.to_string(), via.map(str::to_string))]
}

#[test]
fn a_particle_after_a_vowel_is_taken_off_and_the_word_as_read_is_said() {
    let conn = built();
    for written in ["해리가", "해리는", "해리를", "해리의"] {
        assert_eq!(headwords(&conn, written), found("해리", Some(written)));
    }
}

#[test]
fn a_two_syllable_or_a_stacked_particle_is_taken_off() {
    let conn = built();
    for written in ["학교에서", "학교에서는", "학교까지"] {
        assert_eq!(headwords(&conn, written), found("학교", Some(written)));
    }
    assert_eq!(
        headwords(&conn, "친구들에게는"),
        found("친구", Some("친구들에게는"))
    );
}

#[test]
fn a_particle_must_agree_with_the_final_consonant() {
    let conn = built();
    for written in ["집은", "집을", "집이"] {
        assert_eq!(headwords(&conn, written), found("집", Some(written)));
    }
    assert!(headwords(&conn, "집는").is_empty());
    assert!(headwords(&conn, "해리은").is_empty());
}

#[test]
fn a_final_rieul_takes_the_ro_forms() {
    let conn = built();
    assert_eq!(headwords(&conn, "길로"), found("길", Some("길로")));
    assert!(headwords(&conn, "집로").is_empty());
}

#[test]
fn a_word_found_as_written_is_never_stripped() {
    let conn = dictionary_of(&[
        line("바다", "noun", "sea", &[]),
        line("바", "noun", "bar", &[]),
    ]);
    assert_eq!(headwords(&conn, "바다"), found("바다", None));
}

#[test]
fn a_word_that_is_both_an_entry_and_a_form_is_not_stripped() {
    let conn = dictionary_of(&[
        line("하나", "noun", "one", &[]),
        line("하다", "verb", "to do", &["하나"]),
        line("하", "noun", "the lower part", &[]),
    ]);
    assert_eq!(
        headwords(&conn, "하나"),
        vec![
            ("하나".to_string(), None),
            ("하다".to_string(), Some("하나".to_string())),
        ]
    );
}

#[test]
fn a_form_finds_its_headword_as_lookup_does() {
    let conn = built();
    assert_eq!(
        headwords(&conn, "먹었습니다"),
        found("먹다", Some("먹었습니다"))
    );
}

#[test]
fn a_particle_alone_finds_nothing() {
    let conn = built();
    for written in ["이", "가", "에서", "들에게는"] {
        assert!(headwords(&conn, written).is_empty(), "{written}");
    }
}

#[test]
fn a_latin_stem_takes_either_variant() {
    let conn = built();
    assert_eq!(headwords(&conn, "Harry가"), found("Harry", Some("harry가")));
}

#[test]
fn a_digit_stays_in_the_key_so_a_numbered_floor_finds_nothing() {
    let conn = built();
    assert!(headwords(&conn, "3층에").is_empty());
}

#[test]
fn punctuation_round_the_word_is_dropped_first() {
    let conn = built();
    assert_eq!(
        headwords(&conn, "「해리가!」"),
        found("해리", Some("해리가"))
    );
}

#[test]
fn a_word_nothing_finds_is_empty() {
    let conn = built();
    assert!(headwords(&conn, "없는말에서").is_empty());
    assert!(headwords(&conn, "").is_empty());
}

#[test]
fn a_vowel_takes_the_ro_forms_and_refuses_the_euro_forms() {
    let conn = built();
    assert_eq!(headwords(&conn, "학교로"), found("학교", Some("학교로")));
    assert!(headwords(&conn, "학교으로").is_empty());
}

#[test]
fn a_final_rieul_refuses_the_euro_forms_but_takes_the_other_consonant_particles() {
    let conn = built();
    for written in ["길은", "길이", "길을"] {
        assert_eq!(headwords(&conn, written), found("길", Some(written)));
    }
    for written in ["길으로", "길으로는", "길으로서", "길으로써"] {
        assert!(headwords(&conn, written).is_empty(), "{written}");
    }
    assert_eq!(headwords(&conn, "집으로"), found("집", Some("집으로")));
}

#[test]
fn the_copula_follows_a_vowel_or_a_consonant() {
    let conn = built();
    assert_eq!(
        headwords(&conn, "학교입니다"),
        found("학교", Some("학교입니다"))
    );
    assert_eq!(
        headwords(&conn, "학교이다"),
        found("학교", Some("학교이다"))
    );
    assert_eq!(
        headwords(&conn, "친구였다"),
        found("친구", Some("친구였다"))
    );
    assert_eq!(
        headwords(&conn, "학생입니다"),
        found("학생", Some("학생입니다"))
    );
    assert_eq!(
        headwords(&conn, "학생이었다"),
        found("학생", Some("학생이었다"))
    );
}

#[test]
fn the_copula_with_go_follows_a_vowel_or_a_consonant() {
    let conn = built();
    assert_eq!(
        headwords(&conn, "학교이고"),
        found("학교", Some("학교이고"))
    );
    assert_eq!(
        headwords(&conn, "학생이고"),
        found("학생", Some("학생이고"))
    );
}

#[test]
fn the_contracted_copula_follows_a_vowel_and_the_full_one_a_consonant() {
    let conn = built();
    for written in ["친구였고", "친구였는데", "학생이었고", "학생이었는데"] {
        let stem = if written.starts_with("친구") {
            "친구"
        } else {
            "학생"
        };
        assert_eq!(headwords(&conn, written), found(stem, Some(written)));
    }
    assert!(headwords(&conn, "학생였다").is_empty());
}

#[test]
fn a_form_found_alone_is_followed_by_what_stripping_finds() {
    let conn = built();
    assert_eq!(headwords(&conn, "소는"), listed(&["솔다", "소"], "소는"));
}

/// The pronouns beside the verbs and adjectives their particles spell a form
/// of, and the words that must not be taken for them.
fn pronouns() -> Connection {
    let lines = [
        line("그", "pron", "he", &[]),
        line("나", "pron", "I", &[]),
        line("너", "pron", "you", &[]),
        line("우리", "pron", "we", &[]),
        line("글다", "verb", "to rot", &["그는", "그와"]),
        line("그르다", "adj", "to be wrong", &["그를"]),
        line("나르다", "verb", "to carry", &["나를"]),
        line("너르다", "adj", "to be spacious", &["너를"]),
        line("우리다", "verb", "to steep", &["우리는"]),
        line("나는", "pron", "I, as the topic", &[]),
        line("날다", "verb", "to fly", &["나는"]),
        line("나오다", "verb", "to come out", &["나와"]),
        line("아", "intj", "ah", &[]),
        line("알다", "verb", "to know", &["아는"]),
    ];
    dictionary_of(&lines)
}

/// Each of `words` in order, every one found by `via`.
fn listed(words: &[&str], via: &str) -> Vec<(String, Option<String>)> {
    words
        .iter()
        .map(|w| ((*w).to_string(), Some(via.to_string())))
        .collect()
}

#[test]
fn a_pronoun_with_the_topic_or_object_particle_comes_before_the_verb_it_spells() {
    let conn = pronouns();
    for (written, pronoun, verb) in [
        ("그는", "그", "글다"),
        ("그를", "그", "그르다"),
        ("나를", "나", "나르다"),
        ("너를", "너", "너르다"),
        ("우리는", "우리", "우리다"),
    ] {
        assert_eq!(
            headwords(&conn, written),
            listed(&[pronoun, verb], written),
            "{written}"
        );
    }
}

#[test]
fn a_pronoun_is_recognised_inside_punctuation() {
    let conn = pronouns();
    assert_eq!(
        headwords(&conn, "「그는!」"),
        listed(&["그", "글다"], "그는")
    );
}

#[test]
fn a_pronoun_with_any_other_particle_keeps_the_form_first() {
    let conn = pronouns();
    assert_eq!(headwords(&conn, "그와"), listed(&["글다", "그"], "그와"));
    assert_eq!(headwords(&conn, "나와"), listed(&["나오다", "나"], "나와"));
}

#[test]
fn a_word_that_is_no_pronoun_keeps_the_form_first_with_the_same_particle() {
    let conn = pronouns();
    assert_eq!(headwords(&conn, "아는"), listed(&["알다", "아"], "아는"));
}

#[test]
fn a_pronoun_with_its_own_entry_stays_as_it_is_found() {
    let conn = pronouns();
    assert_eq!(
        headwords(&conn, "나는"),
        vec![
            ("나는".to_string(), None),
            ("날다".to_string(), Some("나는".to_string())),
        ]
    );
}

#[test]
fn a_pronoun_with_no_form_hit_is_only_the_pronoun() {
    let conn = pronouns();
    assert_eq!(headwords(&conn, "우리를"), found("우리", Some("우리를")));
}

#[test]
fn a_verb_the_pronoun_also_finds_is_listed_once_after_it() {
    let conn = dictionary_of(&[
        line("나", "pron", "I", &[]),
        line("나르다", "verb", "to carry", &["나를", "나"]),
    ]);
    assert_eq!(headwords(&conn, "나를"), listed(&["나", "나르다"], "나를"));
}

#[test]
fn a_form_whose_stem_is_its_own_headword_is_not_listed_twice() {
    let conn = dictionary_of(&[line("가다", "verb", "to go", &["가는", "가"])]);
    assert_eq!(headwords(&conn, "가는"), found("가다", Some("가는")));
}

#[test]
fn a_suffix_is_no_stem_so_the_longer_particle_finds_the_noun() {
    let conn = dictionary_of(&[
        line("-거라", "suffix", "imperative ending", &[]),
        line("거", "noun", "thing", &[]),
    ]);
    assert_eq!(headwords(&conn, "거라는"), found("거", Some("거라는")));
}

#[test]
fn a_stem_that_is_an_affix_and_a_form_keeps_only_the_form() {
    let conn = dictionary_of(&[
        line("-갔-", "suffix", "past tense", &[]),
        line("가다", "verb", "to go", &["갔"]),
    ]);
    assert_eq!(headwords(&conn, "갔을"), found("가다", Some("갔을")));
}

#[test]
fn a_prefix_is_no_stem_either() {
    let conn = dictionary_of(&[
        line("거라-", "prefix", "a prefix", &[]),
        line("거", "noun", "thing", &[]),
    ]);
    assert_eq!(headwords(&conn, "거라는"), found("거", Some("거라는")));
}

#[test]
fn a_stem_found_only_as_an_affix_is_stripped_further() {
    let conn = dictionary_of(&[
        line("-학교도", "suffix", "an ending", &[]),
        line("학교", "noun", "school", &[]),
    ]);
    assert_eq!(
        headwords(&conn, "학교도를"),
        found("학교", Some("학교도를"))
    );
}

#[test]
fn a_word_whose_only_stem_is_an_affix_finds_nothing() {
    let conn = dictionary_of(&[line("-거라", "suffix", "imperative ending", &[])]);
    assert!(headwords(&conn, "거라는").is_empty());
}

#[test]
fn an_affix_still_answers_for_the_word_as_written() {
    let conn = dictionary_of(&[line("-거라", "suffix", "imperative ending", &["거라서"])]);
    assert_eq!(headwords(&conn, "거라"), found("-거라", None));
    assert_eq!(headwords(&conn, "거라서"), found("-거라", Some("거라서")));
}

#[test]
fn more_particles_agree_with_the_stem() {
    let conn = built();
    let cases = [
        ("해리야", "해리"),
        ("학생이야", "학생"),
        ("학교라는", "학교"),
        ("학생이라는", "학생"),
        ("학교라도", "학교"),
        ("학생이라도", "학생"),
        ("학교인", "학교"),
        ("학생인", "학생"),
        ("학교이고", "학교"),
        ("학생이고", "학생"),
        ("학교만큼", "학교"),
        ("학생만큼", "학생"),
        ("학교대로", "학교"),
        ("학생대로", "학생"),
    ];
    for (written, stem) in cases {
        assert_eq!(
            headwords(&conn, written),
            found(stem, Some(written)),
            "{written}"
        );
    }
    for written in [
        "해리이야",
        "학생야",
        "학교이라는",
        "학생라도",
        "학교고",
        "학생고",
    ] {
        assert!(headwords(&conn, written).is_empty(), "{written}");
    }
}

/// Every row of `PARTICLES`, written out, once for each kind of stem the row
/// agrees with: 학교 ends in a vowel, 길 in ㄹ and 집 in another consonant. A row
/// that a shorter row stacked on another could also reach (들은 through 은 and
/// 들) stands behind 도도, 도를 or 만만, which leave only the row itself enough
/// strips to reach 학교 and put a vowel, ㄹ or another consonant before it.
const TAKEN_OFF: &[(&str, &str)] = &[
    ("길은", "길"),
    ("집은", "집"),
    ("길이", "길"),
    ("집이", "집"),
    ("길을", "길"),
    ("집을", "집"),
    ("길과", "길"),
    ("집과", "집"),
    ("학교는", "학교"),
    ("학교가", "학교"),
    ("학교를", "학교"),
    ("학교와", "학교"),
    ("학교로", "학교"),
    ("길로", "길"),
    ("학교랑", "학교"),
    ("학교나", "학교"),
    ("학교다", "학교"),
    ("학교의", "학교"),
    ("길의", "길"),
    ("집의", "집"),
    ("학교에", "학교"),
    ("길에", "길"),
    ("집에", "집"),
    ("학교께", "학교"),
    ("길께", "길"),
    ("집께", "집"),
    ("학교도", "학교"),
    ("길도", "길"),
    ("집도", "집"),
    ("학교만", "학교"),
    ("길만", "길"),
    ("집만", "집"),
    ("학교들", "학교"),
    ("길들", "길"),
    ("집들", "집"),
    ("학교야", "학교"),
    ("학교인", "학교"),
    ("길인", "길"),
    ("집인", "집"),
    ("집으로", "집"),
    ("학교도를이랑", "학교"),
    ("학교만만이랑", "학교"),
    ("학교도를이나", "학교"),
    ("학교만만이나", "학교"),
    ("학교도도이다", "학교"),
    ("학교도를이다", "학교"),
    ("학교만만이다", "학교"),
    ("학교도도로는", "학교"),
    ("학교도를로는", "학교"),
    ("학교로서", "학교"),
    ("길로서", "길"),
    ("학교로써", "학교"),
    ("길로써", "길"),
    ("학교예요", "학교"),
    ("학교라고", "학교"),
    ("학교에서", "학교"),
    ("길에서", "길"),
    ("집에서", "집"),
    ("학교에게", "학교"),
    ("길에게", "길"),
    ("집에게", "집"),
    ("학교한테", "학교"),
    ("길한테", "길"),
    ("집한테", "집"),
    ("학교께서", "학교"),
    ("길께서", "길"),
    ("집께서", "집"),
    ("학교까지", "학교"),
    ("길까지", "길"),
    ("집까지", "집"),
    ("학교부터", "학교"),
    ("길부터", "길"),
    ("집부터", "집"),
    ("학교처럼", "학교"),
    ("길처럼", "길"),
    ("집처럼", "집"),
    ("학교보다", "학교"),
    ("길보다", "길"),
    ("집보다", "집"),
    ("학교하고", "학교"),
    ("길하고", "길"),
    ("집하고", "집"),
    ("학교마다", "학교"),
    ("길마다", "길"),
    ("집마다", "집"),
    ("학교조차", "학교"),
    ("길조차", "길"),
    ("집조차", "집"),
    ("학교밖에", "학교"),
    ("길밖에", "길"),
    ("집밖에", "집"),
    ("학교도도들은", "학교"),
    ("학교도를들은", "학교"),
    ("학교만만들은", "학교"),
    ("학교도도들이", "학교"),
    ("학교도를들이", "학교"),
    ("학교만만들이", "학교"),
    ("학교도도들을", "학교"),
    ("학교도를들을", "학교"),
    ("학교만만들을", "학교"),
    ("학교도도들의", "학교"),
    ("학교도를들의", "학교"),
    ("학교만만들의", "학교"),
    ("학교였다", "학교"),
    ("학교였고", "학교"),
    ("학교도를이야", "학교"),
    ("학교만만이야", "학교"),
    ("학교라는", "학교"),
    ("학교라도", "학교"),
    ("학교이고", "학교"),
    ("길이고", "길"),
    ("집이고", "집"),
    ("학교만큼", "학교"),
    ("길만큼", "길"),
    ("집만큼", "집"),
    ("학교대로", "학교"),
    ("길대로", "길"),
    ("집대로", "집"),
    ("학교만만으로는", "학교"),
    ("집으로서", "집"),
    ("집으로써", "집"),
    ("길이에요", "길"),
    ("집이에요", "집"),
    ("학교입니다", "학교"),
    ("길입니다", "길"),
    ("집입니다", "집"),
    ("길이었다", "길"),
    ("집이었다", "집"),
    ("학교도를이라고", "학교"),
    ("학교만만이라고", "학교"),
    ("학교도도에서는", "학교"),
    ("학교도를에서는", "학교"),
    ("학교만만에서는", "학교"),
    ("학교도도에서도", "학교"),
    ("학교도를에서도", "학교"),
    ("학교만만에서도", "학교"),
    ("학교에게서", "학교"),
    ("길에게서", "길"),
    ("집에게서", "집"),
    ("학교도도에게는", "학교"),
    ("학교도를에게는", "학교"),
    ("학교만만에게는", "학교"),
    ("학교한테서", "학교"),
    ("길한테서", "길"),
    ("집한테서", "집"),
    ("학교도도들에게", "학교"),
    ("학교도를들에게", "학교"),
    ("학교만만들에게", "학교"),
    ("학교였는데", "학교"),
    ("길이었고", "길"),
    ("집이었고", "집"),
    ("학교도를이라는", "학교"),
    ("학교만만이라는", "학교"),
    ("학교도를이라도", "학교"),
    ("학교만만이라도", "학교"),
    ("길이었는데", "길"),
    ("집이었는데", "집"),
];

/// The same rows on the stems they do not agree with.
const REFUSED: &[(&str, &str)] = &[
    ("학교은", "학교"),
    ("학교이", "학교"),
    ("학교을", "학교"),
    ("학교과", "학교"),
    ("길는", "길"),
    ("집는", "집"),
    ("길가", "길"),
    ("집가", "집"),
    ("길를", "길"),
    ("집를", "집"),
    ("길와", "길"),
    ("집와", "집"),
    ("집로", "집"),
    ("길랑", "길"),
    ("집랑", "집"),
    ("길나", "길"),
    ("집나", "집"),
    ("길다", "길"),
    ("집다", "집"),
    ("길야", "길"),
    ("집야", "집"),
    ("학교으로", "학교"),
    ("길으로", "길"),
    ("학교도도이랑", "학교"),
    ("학교도도이나", "학교"),
    ("학교만만로는", "학교"),
    ("집로서", "집"),
    ("집로써", "집"),
    ("길예요", "길"),
    ("집예요", "집"),
    ("길라고", "길"),
    ("집라고", "집"),
    ("길였다", "길"),
    ("집였다", "집"),
    ("길였고", "길"),
    ("집였고", "집"),
    ("학교도도이야", "학교"),
    ("길라는", "길"),
    ("집라는", "집"),
    ("길라도", "길"),
    ("집라도", "집"),
    ("학교도도으로는", "학교"),
    ("학교도를으로는", "학교"),
    ("학교으로서", "학교"),
    ("길으로서", "길"),
    ("학교으로써", "학교"),
    ("길으로써", "길"),
    ("학교이에요", "학교"),
    ("학교이었다", "학교"),
    ("학교도도이라고", "학교"),
    ("길였는데", "길"),
    ("집였는데", "집"),
    ("학교이었고", "학교"),
    ("학교도도이라는", "학교"),
    ("학교도도이라도", "학교"),
    ("학교이었는데", "학교"),
];

#[test]
fn every_particle_comes_off_the_stems_it_agrees_with_and_no_others() {
    let conn = dictionary_of(&[
        line("학교", "noun", "school", &[]),
        line("길", "noun", "road", &[]),
        line("집", "noun", "house", &[]),
    ]);
    for (written, stem) in TAKEN_OFF {
        assert_eq!(
            headwords(&conn, written),
            found(stem, Some(written)),
            "{written}"
        );
    }
    for (written, stem) in REFUSED {
        assert!(
            headwords(&conn, written).is_empty(),
            "{written} must not find {stem}"
        );
    }
}

#[test]
fn particles_are_ordered_shortest_first() {
    let lengths: Vec<usize> = PARTICLES.iter().map(|(p, _)| p.chars().count()).collect();
    assert!(lengths.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn at_most_three_particles_are_stacked() {
    let conn = built();
    assert_eq!(
        headwords(&conn, "학교도도도"),
        found("학교", Some("학교도도도"))
    );
    assert!(headwords(&conn, "학교도도도도").is_empty());
}
