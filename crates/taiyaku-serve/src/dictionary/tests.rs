use axum::body::to_bytes;

use super::*;

/// A server whose dictionaries directory holds Russian, Japanese and Korean,
/// or a server given no dictionaries directory.
fn state(dir: &std::path::Path, with_dictionaries: bool) -> Arc<ServeState> {
    let words = dir.join("dictionaries");
    std::fs::create_dir_all(&words).unwrap();
    let conn = rusqlite::Connection::open(words.join("ru.db")).unwrap();
    taiyaku_library::dictionary::import(
        &conn,
        r#"{"word":"говорить","pos":"verb","senses":[{"glosses":["to speak"]}],"forms":[{"form":"говори́т","tags":["present"]}]}"#
            .as_bytes(),
    )
    .unwrap();
    let conn = rusqlite::Connection::open(words.join("ko.db")).unwrap();
    taiyaku_library::dictionary::import(
        &conn,
        r#"{"word":"해리","pos":"name","senses":[{"glosses":["Harry"]}]}"#.as_bytes(),
    )
    .unwrap();
    let mut state = crate::test_support::state(dir);
    let conn = rusqlite::Connection::open(words.join("ja.db")).unwrap();
    taiyaku_library::dictionary::import(
        &conn,
        concat!(
            r#"{"word":"食べる","pos":"verb","senses":[{"glosses":["to eat"]}],"forms":[{"form":"食べました","tags":["polite"]}]}"#,
            "\n",
            r#"{"word":"𠮷野","pos":"noun","senses":[{"glosses":["Yoshino"]}]}"#,
            "\n",
            r#"{"word":"葛","pos":"noun","senses":[{"glosses":["kudzu"]}]}"#,
        )
        .as_bytes(),
    )
    .unwrap();
    state.dictionaries = with_dictionaries.then_some(words);
    Arc::new(state)
}

async fn asked(state: Arc<ServeState>, lang: &str, word: &str) -> (StatusCode, String) {
    let response = lookup(State(state), AxPath((lang.into(), word.into()))).await;
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn a_form_read_is_looked_up_in_its_languages_dictionary() {
    let tmp = crate::test_support::Fixture::new("dictionary-found");
    let (status, body) = asked(state(&tmp, true), "ru-RU", "говорит").await;
    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        found,
        serde_json::json!([{
            "word": "говорить", "pos": "verb", "glosses": ["to speak"], "via": "говорит"
        }])
    );
}

#[tokio::test]
async fn a_language_with_no_dictionary_here_is_not_found_and_says_so() {
    let tmp = crate::test_support::Fixture::new("dictionary-none");
    let (status, body) = asked(state(&tmp, true), "es", "hablar").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("no dictionary for es"), "{body}");
    let (status, _) = asked(state(&tmp, false), "ru", "говорит").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a server given no dictionaries"
    );
}

#[tokio::test]
async fn a_language_that_could_name_another_file_is_refused() {
    let tmp = crate::test_support::Fixture::new("dictionary-refused");
    for lang in ["..", "ru.db", "r", "russian"] {
        let (status, _) = asked(state(&tmp, true), lang, "x").await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{lang}");
    }
}

async fn asked_at(state: Arc<ServeState>, lang: &str, text: &str) -> (StatusCode, String) {
    let response = lookup_at(State(state), AxPath((lang.into(), text.into()))).await;
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn the_longest_word_a_text_begins_with_is_answered_with_its_length() {
    let tmp = crate::test_support::Fixture::new("dictionary-at");
    let (status, body) = asked_at(state(&tmp, true), "ja", "食べました。").await;
    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        found,
        serde_json::json!({
            "length": 5,
            "entries": [{
                "word": "食べる", "pos": "verb", "glosses": ["to eat"], "via": "食べました"
            }]
        })
    );
}

#[tokio::test]
async fn the_length_counts_utf16_units_so_an_astral_char_is_two() {
    let tmp = crate::test_support::Fixture::new("dictionary-at-astral");
    let (status, body) = asked_at(state(&tmp, true), "ja-JP", "𠮷野家").await;
    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(found["length"], 3);
    assert_eq!(found["entries"][0]["word"], "𠮷野");
}

#[tokio::test]
async fn the_length_takes_in_the_variation_selector_after_the_word() {
    let tmp = crate::test_support::Fixture::new("dictionary-at-selector");
    let (status, body) = asked_at(state(&tmp, true), "ja", "葛\u{E0100}の").await;
    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(found["length"], 3, "葛 and its selector's surrogate pair");
    assert_eq!(found["entries"][0]["word"], "葛");
}

#[tokio::test]
async fn a_text_with_nothing_found_answers_length_zero() {
    let tmp = crate::test_support::Fixture::new("dictionary-at-none");
    let (status, body) = asked_at(state(&tmp, true), "ja", "ん").await;
    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(found, serde_json::json!({"length": 0, "entries": []}));
}

#[tokio::test]
async fn a_text_of_more_than_16_code_points_is_refused() {
    let tmp = crate::test_support::Fixture::new("dictionary-at-long");
    let (status, _) = asked_at(state(&tmp, true), "ja", &"食".repeat(17)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = asked_at(state(&tmp, true), "ja", &"食".repeat(16)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = asked_at(state(&tmp, true), "ja", &"𠮷".repeat(16)).await;
    assert_eq!(status, StatusCode::OK, "code points, not UTF-16 units");
    let (status, _) = asked_at(state(&tmp, true), "ja", &"𠮷".repeat(17)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn at_with_no_dictionary_here_is_not_found_and_says_so() {
    let tmp = crate::test_support::Fixture::new("dictionary-at-nodict");
    let (status, body) = asked_at(state(&tmp, true), "es", "hablar").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("no dictionary for es"), "{body}");
    let (status, _) = asked_at(state(&tmp, true), "..", "x").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

async fn routed(router: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = crate::test_support::get(router, uri).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;
    (status, serde_json::from_str(&body).unwrap())
}

#[tokio::test]
async fn the_word_at_is_still_a_word_to_the_existing_route() {
    let tmp = crate::test_support::Fixture::new("dictionary-at-word");
    let state = state(&tmp, true);
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);
    let (status, word) = routed(&router, "/api/dictionary/ja/at").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(word, serde_json::json!([]), "the word route's array");
    let text: String = "食べました".bytes().map(|b| format!("%{b:02X}")).collect();
    let (status, at) = routed(&router, &format!("/api/dictionary/ja/at/{text}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(at["length"], 5, "the tap route's object");
    assert_eq!(at["entries"][0]["word"], "食べる");
}

/// What the router answers at `route` where the Russian dictionary is no
/// database. The file is there, so `dictionary_for` passes it, and only the
/// first query finds out what it holds.
async fn routed_to_an_unreadable_dictionary(fixture: &str, route: &str) -> (StatusCode, String) {
    let tmp = crate::test_support::Fixture::new(fixture);
    let state = state(&tmp, true);
    std::fs::write(
        tmp.join("dictionaries").join("ru.db"),
        "this is no database. ".repeat(64),
    )
    .unwrap();
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);
    let word: String = "говорит".bytes().map(|b| format!("%{b:02X}")).collect();
    let response = crate::test_support::get(&router, &format!("{route}/{word}")).await;
    let status = response.status();
    (status, crate::test_support::body_string(response).await)
}

#[tokio::test]
async fn an_unreadable_dictionary_answers_500_to_a_word() {
    let (status, body) =
        routed_to_an_unreadable_dictionary("dictionary-unreadable", "/api/dictionary/ru").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(
        body, "the dictionary could not be read; the server's log says why",
        "the cause stays in the log"
    );
}

#[tokio::test]
async fn an_unreadable_dictionary_answers_500_to_a_tap() {
    let (status, body) =
        routed_to_an_unreadable_dictionary("dictionary-at-unreadable", "/api/dictionary/ru/at")
            .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(
        body, "the dictionary could not be read; the server's log says why",
        "the cause stays in the log"
    );
}

#[tokio::test]
async fn only_korean_has_particles_taken_off() {
    let tmp = crate::test_support::Fixture::new("dictionary-russian-particle");
    let (status, body) = asked(state(&tmp, true), "ru", "говорить가").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "[]");
}

#[tokio::test]
async fn a_korean_word_is_found_through_its_particle() {
    let tmp = crate::test_support::Fixture::new("dictionary-korean");
    let state = state(&tmp, true);
    for lang in ["ko", "ko-KR"] {
        let (status, body) = asked(state.clone(), lang, "해리가").await;
        assert_eq!(status, StatusCode::OK, "{lang}");
        let found: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            found,
            serde_json::json!([{
                "word": "해리", "pos": "name", "glosses": ["Harry"], "via": "해리가"
            }]),
            "{lang}"
        );
    }
}
