use super::*;

/// The one shape [`full_hash`] lets through.
const FULL: &str = "0123456789abcdef0123456789abcdef01234567";

/// [`FULL`] with its last character made not hex, so length cannot be the
/// reason it is refused.
const NOT_HEX: &str = "0123456789abcdef0123456789abcdef0123456g";

/// [`FULL`] in upper case, so case is the only thing wrong with it.
const UPPER: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

/// A second full hash, for git's HEAD, so a case can tell which source won.
const HEAD: &str = "89abcdef0123456789abcdef0123456789abcdef";

#[test]
fn an_unset_commit_is_no_commit() {
    assert_eq!(full_hash(None), None);
}

#[test]
fn an_empty_commit_is_no_commit() {
    assert_eq!(full_hash(Some("")), None);
}

/// The seven characters the label shows, which are hex: refused on length
/// alone.
#[test]
fn a_short_hash_is_no_commit() {
    let short = &FULL[..7];
    assert!(short.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(full_hash(Some(short)), None);
}

#[test]
fn a_full_length_string_with_a_character_that_is_not_hex_is_no_commit() {
    assert_eq!(NOT_HEX.len(), FULL.len());
    assert_eq!(full_hash(Some(NOT_HEX)), None);
}

#[test]
fn an_uppercase_hash_is_no_commit() {
    assert_eq!(UPPER.to_ascii_lowercase(), FULL);
    assert_eq!(full_hash(Some(UPPER)), None);
}

#[test]
fn a_full_lowercase_hash_is_the_commit() {
    assert_eq!(full_hash(Some(FULL)), Some(FULL));
}

#[test]
fn a_valid_taiyaku_commit_wins_over_a_changed_head_and_is_not_marked() {
    assert_eq!(
        build_label(Some(FULL), Some(HEAD), Some("1")),
        BuildLabel {
            commit: Some(FULL),
            dirty: false,
        },
    );
}

#[test]
fn an_invalid_taiyaku_commit_falls_through_to_a_changed_head_which_is_marked() {
    assert_eq!(
        build_label(Some(NOT_HEX), Some(HEAD), Some("1")),
        BuildLabel {
            commit: Some(HEAD),
            dirty: true,
        },
    );
}

#[test]
fn without_taiyaku_commit_a_clean_head_is_the_commit_and_is_not_marked() {
    assert_eq!(
        build_label(None, Some(HEAD), None),
        BuildLabel {
            commit: Some(HEAD),
            dirty: false,
        },
    );
}

#[test]
fn with_neither_there_is_no_commit_and_no_mark_even_with_the_dirty_fact_set() {
    assert_eq!(
        build_label(None, None, Some("1")),
        BuildLabel {
            commit: None,
            dirty: false,
        },
    );
}

#[test]
fn an_invalid_head_is_no_commit_and_no_mark() {
    assert_eq!(
        build_label(None, Some(UPPER), Some("1")),
        BuildLabel {
            commit: None,
            dirty: false,
        },
    );
}

/// The image's own configuration: the Dockerfile passes the commit, and its
/// build context has no `.git`, so there is no HEAD.
#[test]
fn a_valid_taiyaku_commit_without_a_head_is_the_commit_and_is_not_marked() {
    assert_eq!(
        build_label(Some(FULL), None, None),
        BuildLabel {
            commit: Some(FULL),
            dirty: false,
        },
    );
}

#[test]
fn an_invalid_taiyaku_commit_without_a_head_is_no_commit_and_no_mark() {
    assert_eq!(
        build_label(Some(NOT_HEX), None, Some("1")),
        BuildLabel {
            commit: None,
            dirty: false,
        },
    );
}

#[test]
fn the_body_answers_a_changed_labels_commit_and_marks_it() {
    let body = about_body(&BuildLabel {
        commit: Some(HEAD),
        dirty: true,
    });
    assert_eq!(body["commit"], serde_json::json!(HEAD));
    assert_eq!(body["dirty"], serde_json::json!(true));
}

#[test]
fn the_body_answers_a_clean_labels_commit_and_does_not_mark_it() {
    let body = about_body(&BuildLabel {
        commit: Some(FULL),
        dirty: false,
    });
    assert_eq!(body["commit"], serde_json::json!(FULL));
    assert_eq!(body["dirty"], serde_json::json!(false));
}

/// The whole body, compared exactly. A string literal and a `json!` key get
/// no mutant, so only an exact comparison holds a name, a sentence, a key or
/// the order of the rows in place.
///
/// Each version is its constant's own `to_string()`, so the expected body
/// follows a bump. A swap between two constants that hold the same value gives
/// the same body, so this cannot see one; which row reads which constant is
/// held by reading [`about_body`].
///
/// `commit` and `dirty` are compared with the same [`build_label`] call the
/// route makes, so this always holds their keys but holds a value only when
/// the build gives it one other than `null` or `false`; CI's `test` job builds
/// with the commit under test (`.forgejo/workflows/ci.yml`). Which source wins
/// is held by the [`build_label`] cases above, and that the body answers the
/// label it is given by the [`about_body`] cases.
#[tokio::test]
async fn about_answers_the_commit_the_source_the_catalog_and_every_format() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_library().await;
    let body = crate::test_support::get_json(&router, "/api/about").await;
    let label = build_label(
        option_env!("TAIYAKU_COMMIT"),
        option_env!("TAIYAKU_GIT_HEAD"),
        option_env!("TAIYAKU_GIT_DIRTY"),
    );
    assert_eq!(
        body,
        serde_json::json!({
            "commit": label.commit,
            "dirty": label.dirty,
            "repository": "https://github.com/Davey-Hughes/taiyaku",
            "catalog": "/opds/v1.2/catalog",
            "formats": [
                {
                    "name": "taiyaku.json",
                    "version": taiyaku_core::VERSION.to_string(),
                    "description": "Sits beside a work’s archives and records how its editions correspond to each other.",
                },
                {
                    "name": "<stem>.library.json",
                    "version": taiyaku_library::librarydoc::VERSION.to_string(),
                    "description": "A work’s metadata, kept in a file so fixes made by hand outlive the database.",
                },
                {
                    "name": "<stem>.progress.json",
                    "version": taiyaku_library::progressdoc::VERSION.to_string(),
                    "description": "One reader’s place in a work, edition by edition.",
                },
                {
                    "name": "<stem>.truth.json",
                    "version": taiyaku_core::TRUTH_VERSION.to_string(),
                    "description": "Boxes a person drew around the text on pages, to score text detection against.",
                },
                {
                    "name": "<stem>.regions.json",
                    "version": taiyaku_core::REGIONS_VERSION.to_string(),
                    "description": "Where the text is on each page, as the regions command found it.",
                },
                {
                    "name": "<stem>.notes.json",
                    "version": taiyaku_library::NOTES_VERSION.to_string(),
                    "description": "Notes one reader left on a work’s pages.",
                },
                {
                    "name": "Database schema",
                    "version": taiyaku_library::schema::SCHEMA_VERSION.to_string(),
                    "description": "The layout of the SQLite database taiyaku keeps its library in.",
                },
                {
                    "name": "OPDS, with the Page Streaming Extension",
                    "version": crate::opds::VERSION.to_string(),
                    "description": "The catalog that reading apps browse and stream pages from.",
                },
                {
                    "name": "HTTP API",
                    "version": null,
                    "description": "The /api routes this web app uses to talk to the server.",
                },
                {
                    "name": "wasm module",
                    "version": null,
                    "description": "taiyaku’s core built for the browser, which the reader uses to match pages between editions.",
                },
            ],
        })
    );
}

/// **Every row says what its format is.** Read row by row, so a failure
/// names the row whose sentence is missing or empty; the exact comparison
/// above would show only that the bodies differ.
#[tokio::test]
async fn every_format_says_what_it_is() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_library().await;
    let body = crate::test_support::get_json(&router, "/api/about").await;
    let formats = body["formats"].as_array().expect("formats is a list");
    assert_eq!(formats.len(), 10);
    for format in formats {
        let description = format["description"].as_str();
        assert!(
            description.is_some_and(|text| !text.trim().is_empty()),
            "{}: {description:?}",
            format["name"],
        );
    }
}
