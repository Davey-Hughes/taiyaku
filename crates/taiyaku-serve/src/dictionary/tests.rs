use axum::body::to_bytes;

use super::*;

/// A server whose dictionaries directory holds Russian alone, or none.
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
    let mut state = crate::test_support::state(dir);
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
