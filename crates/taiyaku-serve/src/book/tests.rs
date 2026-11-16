use super::*;
use axum::response::Response;

fn edition(paths: &[&str]) -> taiyaku_core::Edition {
    let members: Vec<String> = paths
        .iter()
        .map(|path| format!(r#"{{ "path": "{path}", "sha256": "x", "pages": 1 }}"#))
        .collect();
    serde_json::from_str(&format!(
        r#"{{ "label": "e", "members": [{}], "spreads": [] }}"#,
        members.join(",")
    ))
    .expect("an edition")
}

#[test]
fn one_member_the_scan_read_as_a_book_is_a_book_and_nothing_else_is() {
    let books = HashSet::from(["en".to_string()]);
    assert!(is_book("en", &edition(&["Story/Story v01.zip"]), &books));
    assert!(!is_book("raw", &edition(&["Story/Story v01.epub"]), &books));
    assert!(!is_book("en", &edition(&["a.epub", "b.epub"]), &books));
}

/// A work of two book editions, `en` and `ko`, each an EPUB on disk, and a
/// comic edition, `raw`, with `hub` its hub, called as `principal`.
async fn book_of(dir: &std::path::Path, hub: &str, principal: crate::auth::Principal) -> Response {
    get(&books(dir, hub, false), principal).await
}

/// [`book_of`]'s work, slug `t`, held in the library as a scan holds it, so
/// it can be pinned and its books told; with `fr`, a third book edition, a
/// copy of `en`.
fn books(dir: &std::path::Path, hub: &str, fr: bool) -> Arc<crate::ServeState> {
    let shelf = dir.to_string_lossy().into_owned();
    taiyaku_read::test_support::epub(
        dir,
        "en.epub",
        &["CHAPTER ONE", "“Who?” he asked.", "Nobody."],
    );
    taiyaku_read::test_support::epub_with(
        dir,
        "ko.epub",
        "<dc:language>ko</dc:language>",
        &["1장", "“누구?”", "그가 물었다.", "아무도."],
    );
    taiyaku_read::test_support::cbz_pages(dir, "raw.cbz", 2, 0);
    let digest = |file: &str| crate::test_support::sha256_of(&dir.join(file));
    let (en, ko, raw) = (digest("en.epub"), digest("ko.epub"), digest("raw.cbz"));
    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "T", "reading": "ltr" }},
          "editions": {{
            "en": {{ "label": "English", "members": [
              {{ "path": "{shelf}/en.epub", "sha256": "{en}", "pages": 3 }} ], "spreads": [] }},
            "ko": {{ "label": "Korean", "members": [
              {{ "path": "{shelf}/ko.epub", "sha256": "{ko}", "pages": 4 }} ], "spreads": [] }},
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/raw.cbz", "sha256": "{raw}", "pages": 2 }} ], "spreads": [] }}{french}
          }},
          "hub": "{hub}",
          "alignments": []
        }}"#,
        french = if fr {
            format!(
                r#", "fr": {{ "label": "French", "members": [
                  {{ "path": "{shelf}/en.epub", "sha256": "{en}", "pages": 3 }} ], "spreads": [] }}"#
            )
        } else {
            String::new()
        },
    );
    let sidecar = Sidecar::from_json(&json).unwrap();
    let mut state = crate::test_support::state(dir);
    crate::test_support::held_as_scanned(
        &state.library,
        dir,
        &sidecar,
        &[
            ("en.epub", "epub-reflow"),
            ("ko.epub", "epub-reflow"),
            ("raw.cbz", "zip"),
        ],
    );
    state.works = crate::Works::fixed(vec![crate::Work::new(
        "t".into(),
        dir.join("taiyaku.json"),
        sidecar,
    )]);
    Arc::new(state)
}

async fn get(state: &Arc<crate::ServeState>, principal: crate::auth::Principal) -> Response {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(principal);
    get_book(
        State(Arc::clone(state)),
        AxPath("t".to_string()),
        extensions,
    )
    .await
}

async fn put(
    state: &Arc<crate::ServeState>,
    principal: crate::auth::Principal,
    body: &str,
) -> StatusCode {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(principal);
    extensions.insert(crate::auth::By::Cookie);
    put_pin(
        State(Arc::clone(state)),
        AxPath("t".to_string()),
        extensions,
        axum::body::Bytes::from(body.to_string()),
    )
    .await
    .status()
}

fn reader() -> crate::auth::Principal {
    crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::Admin,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access: taiyaku_library::account::Access::Every,
    }
}

async fn json_of(response: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn a_book_is_sent_as_blocks_with_the_hubs_alignment_to_each_edition() {
    let tmp = crate::test_support::Fixture::new("book-route");
    let response = book_of(&tmp, "en", reader()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let book = json_of(response).await;

    assert_eq!(book["title"], "T");
    assert_eq!(book["hub"], "en");
    let editions = book["editions"].as_object().unwrap();
    assert_eq!(
        editions.keys().collect::<Vec<_>>(),
        ["en", "ko"],
        "the comic is no book"
    );
    assert_eq!(book["editions"]["ko"]["label"], "Korean");
    assert_eq!(book["editions"]["ko"]["language"], "ko");
    assert!(
        book["editions"]["en"]
            .as_object()
            .unwrap()
            .get("language")
            .is_some_and(serde_json::Value::is_null),
        "a book that declares no language says so"
    );
    assert_eq!(
        book["editions"]["en"]["blocks"][1]["text"],
        "“Who?” he asked."
    );
    assert_eq!(book["editions"]["en"]["blocks"][1]["heading"], false);
    assert!(
        book["editions"]["en"]["blocks"][1]
            .as_object()
            .unwrap()
            .get("line")
            .is_some_and(serde_json::Value::is_null),
        "a block the book prints no number for says so"
    );

    let alignments = book["alignments"].as_array().unwrap();
    assert_eq!(alignments.len(), 1);
    assert_eq!(alignments[0]["to"], "ko");
    let links: Vec<[usize; 4]> = serde_json::from_value(alignments[0]["links"].clone()).unwrap();
    assert_eq!(links.first().map(|l| (l[0], l[2])), Some((0, 0)));
    assert_eq!(
        links.last().map(|l| (l[1], l[3])),
        Some((3, 4)),
        "{links:?}"
    );
    assert!(
        links
            .windows(2)
            .all(|w| w[0][1] == w[1][0] && w[0][3] == w[1][2]),
        "{links:?}"
    );
}

#[tokio::test]
async fn the_hub_is_the_works_where_it_is_a_book_and_the_first_book_where_not() {
    for (hub, book_hub) in [("ko", "ko"), ("raw", "en")] {
        let tmp = crate::test_support::Fixture::new("book-route-hub");
        let book = json_of(book_of(&tmp, hub, reader()).await).await;
        assert_eq!(book["hub"], book_hub, "the work's hub is {hub}");
        let alignments = book["alignments"].as_array().unwrap();
        assert_eq!(alignments.len(), 1, "{book}");
        assert_ne!(alignments[0]["to"], book_hub);
    }
}

#[tokio::test]
async fn an_account_that_may_not_stream_is_refused_the_text() {
    let tmp = crate::test_support::Fixture::new("book-route-no-stream");
    let principal = crate::auth::Principal {
        can_stream: false,
        role: crate::auth::Role::User,
        ..reader()
    };
    assert_eq!(
        book_of(&tmp, "en", principal).await.status(),
        StatusCode::FORBIDDEN
    );
}

/// [`get_book`] for the work `pick` chooses, the book's or the comic's, of
/// [`crate::test_support::misnamed`]'s library.
async fn misnamed_book(pick: fn(String, String) -> String) -> Response {
    let tmp = crate::test_support::Fixture::new("book-route-misnamed");
    let (state, book, comic) = crate::test_support::misnamed(&tmp);
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(reader());
    get_book(State(state), AxPath(pick(book, comic)), extensions).await
}

#[tokio::test]
async fn a_reflowable_epub_named_zip_is_read_as_a_book() {
    let response = misnamed_book(|book, _| book).await;
    assert_eq!(response.status(), StatusCode::OK);
    let story = json_of(response).await;
    let hub = story["hub"].as_str().expect("a book, as the scan read it");
    assert_eq!(story["editions"][hub]["blocks"][1]["text"], "Two.");
}

#[tokio::test]
async fn a_comic_archive_named_epub_is_no_book() {
    let response = misnamed_book(|_, comic| comic).await;
    assert_eq!(response.status(), StatusCode::OK, "not read as a book");
    let comic = json_of(response).await;
    assert_eq!(comic["hub"], serde_json::Value::Null, "{comic}");
}

#[tokio::test]
async fn a_work_with_no_book_edition_is_a_book_of_none() {
    let tmp = crate::test_support::Fixture::new("book-route-comic");
    let state = crate::test_support::state(&tmp);
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(reader());
    let response = get_book(State(Arc::new(state)), AxPath("t".to_string()), extensions).await;
    assert_eq!(response.status(), StatusCode::OK);
    let book = json_of(response).await;
    assert_eq!(book["hub"], serde_json::Value::Null);
    assert_eq!(book["editions"], serde_json::json!({}));
}

#[tokio::test]
async fn a_pin_starts_a_link_the_book_route_sends_and_unpinning_takes_it_away() {
    let tmp = crate::test_support::Fixture::new("book-pin");
    let state = books(&tmp, "en", false);
    let links = |book: &serde_json::Value| -> Vec<[usize; 4]> {
        serde_json::from_value(book["alignments"][0]["links"].clone()).unwrap()
    };
    let before = links(&json_of(get(&state, reader()).await).await);
    assert!(!before.iter().any(|l| l[0] == 2 && l[2] == 1), "{before:?}");

    let pin = r#"{"edition": "ko", "hub": "en", "hub_block": 2, "block": 1}"#;
    assert_eq!(put(&state, reader(), pin).await, StatusCode::NO_CONTENT);
    let book = json_of(get(&state, reader()).await).await;
    assert!(
        links(&book).iter().any(|l| l[0] == 2 && l[2] == 1),
        "{book}"
    );
    assert_eq!(book["alignments"][0]["pins"], serde_json::json!([[2, 1]]));

    let unpin = r#"{"edition": "ko", "hub": "en", "hub_block": 2, "block": null}"#;
    assert_eq!(put(&state, reader(), unpin).await, StatusCode::NO_CONTENT);
    let book = json_of(get(&state, reader()).await).await;
    assert_eq!(links(&book), before);
    assert_eq!(book["alignments"][0]["pins"], serde_json::json!([]));
}

#[tokio::test]
async fn a_pin_is_an_administrators_and_joins_a_hub_block_to_another_book_editions() {
    let tmp = crate::test_support::Fixture::new("book-pin-refused");
    let state = books(&tmp, "en", false);
    let user = crate::auth::Principal {
        role: crate::auth::Role::User,
        ..reader()
    };
    let pin = |edition: &str, hub_block: u32, block: u32| {
        format!(
            r#"{{"edition": "{edition}", "hub": "en", "hub_block": {hub_block}, "block": {block}}}"#
        )
    };
    assert_eq!(
        put(&state, user, &pin("ko", 1, 1)).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(put(&state, reader(), "{}").await, StatusCode::BAD_REQUEST);
    assert_eq!(
        put(&state, reader(), &pin("raw", 1, 1)).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        put(&state, reader(), &pin("fr", 1, 1)).await,
        StatusCode::NOT_FOUND
    );
    for past in [pin("ko", 3, 1), pin("ko", 1, 4), pin("en", 1, 1)] {
        assert_eq!(
            put(&state, reader(), &past).await,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{past}"
        );
    }
    assert_eq!(
        put(&state, reader(), &pin("ko", 2, 3)).await,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn a_pin_holds_on_its_own_edition_alone() {
    let tmp = crate::test_support::Fixture::new("book-pin-own");
    let state = books(&tmp, "en", true);
    let pin = r#"{"edition": "ko", "hub": "en", "hub_block": 2, "block": 1}"#;
    assert_eq!(put(&state, reader(), pin).await, StatusCode::NO_CONTENT);
    let book = json_of(get(&state, reader()).await).await;
    let pins_to = |to: &str| {
        book["alignments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["to"] == to)
            .map(|a| a["pins"].clone())
    };
    assert_eq!(pins_to("ko"), Some(serde_json::json!([[2, 1]])));
    assert_eq!(pins_to("fr"), Some(serde_json::json!([])));
}

#[tokio::test]
async fn a_pin_out_of_order_with_an_earlier_one_is_not_sent_as_holding() {
    let tmp = crate::test_support::Fixture::new("book-pin-held");
    let state = books(&tmp, "en", false);
    {
        let conn = state.library.conn();
        for (hub_block, block) in [(1, 2), (2, 1)] {
            conn.execute(
                "INSERT INTO book_pin (work_id, edition, hub_block, block) VALUES (1, 'ko', ?1, ?2)",
                rusqlite::params![hub_block, block],
            )
            .unwrap();
        }
    }
    let book = json_of(get(&state, reader()).await).await;
    assert_eq!(book["alignments"][0]["pins"], serde_json::json!([[1, 2]]));
}

#[tokio::test]
async fn pins_hold_only_against_the_works_own_hub() {
    let tmp = crate::test_support::Fixture::new("book-pin-hub");
    let state = books(&tmp, "en", true);
    let pin = r#"{"edition": "ko", "hub": "en", "hub_block": 2, "block": 1}"#;
    assert_eq!(put(&state, reader(), pin).await, StatusCode::NO_CONTENT);
    let work = state.works.get("t").unwrap();
    let mut seen = (*work.sidecar()).clone();
    seen.editions.remove("en");
    seen.hub = "fr".into();
    let (pins, books) = {
        let conn = state.library.conn();
        (
            pins_of(&conn, &work).unwrap(),
            books_of(&conn, &work).unwrap(),
        )
    };
    let book = document(&work, &seen, "en", &pins, &books).unwrap();
    assert_eq!(book["hub"], "fr");
    let to_ko = book["alignments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["to"] == "ko")
        .unwrap();
    assert_eq!(to_ko["pins"], serde_json::json!([]), "{book}");
}

#[tokio::test]
async fn each_block_is_sent_with_its_sentences_and_each_alignment_pairs_them() {
    let tmp = crate::test_support::Fixture::new("book-sentences");
    let book = json_of(book_of(&tmp, "en", reader()).await).await;

    assert_eq!(
        book["editions"]["en"]["blocks"][1]["sentences"],
        serde_json::json!([[0, 16]]),
        "“Who?” he asked. is one sentence, sixteen units long"
    );
    let sentences: Vec<[usize; 4]> =
        serde_json::from_value(book["alignments"][0]["sentences"].clone()).unwrap();
    assert_eq!(sentences.first().map(|l| (l[0], l[2])), Some((0, 0)));
    assert_eq!(
        sentences.last().map(|l| (l[1], l[3])),
        Some((3, 4)),
        "every sentence of either book, the one a block holds each: {sentences:?}"
    );
    assert!(
        sentences
            .windows(2)
            .all(|w| w[0][1] == w[1][0] && w[0][3] == w[1][2]),
        "{sentences:?}"
    );
}

/// A work, slug `t`, of a book edition for each of `editions`, `{key}.epub` in
/// `dir` holding its paragraphs, the first edition its hub, held in the
/// library as a scan holds it: each member named by its file's digest as the
/// file stands now, and its block count.
fn shelf(dir: &std::path::Path, editions: &[(&str, &[&str])]) -> Arc<crate::ServeState> {
    let shelf = dir.to_string_lossy();
    let mut listed = Vec::new();
    let mut walked = Vec::new();
    for (key, paragraphs) in editions {
        let path = taiyaku_read::test_support::epub(dir, &format!("{key}.epub"), paragraphs);
        listed.push(format!(
            r#""{key}": {{ "label": "{key}", "members": [
              {{ "path": "{shelf}/{key}.epub", "sha256": "{}", "pages": {} }} ],
              "spreads": [] }}"#,
            crate::test_support::sha256_of(&path),
            paragraphs.len(),
        ));
        walked.push(format!("{key}.epub"));
    }
    let sidecar = Sidecar::from_json(&format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "T", "reading": "ltr" }},
          "editions": {{ {} }},
          "hub": "{}",
          "alignments": []
        }}"#,
        listed.join(","),
        editions[0].0,
    ))
    .unwrap();
    let mut state = crate::test_support::state(dir);
    let walked: Vec<(&str, &str)> = walked
        .iter()
        .map(|file| (file.as_str(), "epub-reflow"))
        .collect();
    crate::test_support::held_as_scanned(&state.library, dir, &sidecar, &walked);
    state.works = crate::Works::fixed(vec![crate::Work::new(
        "t".into(),
        dir.join("taiyaku.json"),
        sidecar,
    )]);
    Arc::new(state)
}

/// While one is held, [`read`]'s cache is not emptied at [`KEPT`]
/// ([`kept_whole`]).
struct KeptWhole;

impl KeptWhole {
    fn hold() -> Self {
        KEEPING_WHOLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self
    }
}

impl Drop for KeptWhole {
    fn drop(&mut self) {
        KEEPING_WHOLE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[tokio::test]
async fn a_book_is_read_once_for_its_files_hash_and_again_for_a_new_one() {
    // The file is changed under the hash the scan recorded, so a second read
    // of it would send the changed text. The cache is held whole, so that
    // other tests reading books in the same process cannot empty it between
    // the requests and have the file read again.
    let _whole = KeptWhole::hold();
    let tmp = crate::test_support::Fixture::new("book-read-once");
    let first_block = |book: serde_json::Value| book["editions"]["en"]["blocks"][0]["text"].clone();
    let state = shelf(&tmp, &[("en", &["As first read."])]);
    assert_eq!(
        first_block(json_of(get(&state, reader()).await).await),
        "As first read."
    );

    taiyaku_read::test_support::epub(&tmp, "en.epub", &["As changed."]);
    assert_eq!(
        first_block(json_of(get(&state, reader()).await).await),
        "As first read.",
        "a second request reads no file for a hash already read"
    );
    assert_eq!(
        first_block(json_of(get(&shelf(&tmp, &[("en", &["As changed."])]), reader()).await).await),
        "As changed.",
        "a new hash is read afresh"
    );
}

/// A pin of `edition`'s block 1 to the hub's block 1, which the three
/// editions of [`three_pinned`] would link without it.
async fn pin_one_to_one(state: &Arc<crate::ServeState>, edition: &str) {
    let pin = format!(r#"{{"edition": "{edition}", "hub": "en", "hub_block": 1, "block": 1}}"#);
    assert_eq!(put(state, reader(), &pin).await, StatusCode::NO_CONTENT);
}

/// The pins on [`three_pinned`]'s work, as `(edition, hub_block, block)`.
fn pins_held(state: &Arc<crate::ServeState>) -> Vec<(String, u32, u32)> {
    let work = state.works.get("t").unwrap();
    pins_of(&state.library.conn(), &work)
        .unwrap()
        .into_iter()
        .map(|pin| (pin.edition, pin.hub_block, pin.block))
        .collect()
}

/// A work of three book editions, `en` its hub, with `ja` and `ko` pinned to
/// it, which no request has read yet. Every block names `tag`, so no other
/// call's read of the same text can be cached: a book read once is served by
/// its digest.
async fn three_pinned(dir: &std::path::Path, tag: &str) -> Arc<crate::ServeState> {
    let texts = |blocks: [&str; 3]| blocks.map(|block| format!("{block} ({tag})"));
    let (en, ja, ko) = (
        texts(["The hub's first.", "The hub's second.", "Its third."]),
        texts(["残る版の一。", "残る版の二。", "その三。"]),
        texts(["사라질 판의 하나.", "사라질 판의 둘.", "그 셋."]),
    );
    let state = shelf(
        dir,
        &[
            ("en", &en.each_ref().map(String::as_str)),
            ("ja", &ja.each_ref().map(String::as_str)),
            ("ko", &ko.each_ref().map(String::as_str)),
        ],
    );
    pin_one_to_one(&state, "ja").await;
    pin_one_to_one(&state, "ko").await;
    state
}

/// [`three_pinned`]'s book with the files of `gone` removed and no scan
/// since, before any request has read them.
async fn file_gone(dir: &std::path::Path, gone: &[&str]) -> Response {
    let state = three_pinned(dir, &gone.join("+")).await;
    for key in gone {
        std::fs::remove_file(dir.join(format!("{key}.epub"))).unwrap();
    }
    get(&state, reader()).await
}

#[tokio::test]
async fn an_edition_whose_file_went_since_the_scan_is_left_out_with_its_pins() {
    let tmp = crate::test_support::Fixture::new("book-file-gone");
    let response = file_gone(&tmp, &["ko"]).await;
    assert_eq!(response.status(), StatusCode::OK, "the rest still opens");
    let book = json_of(response).await;
    assert_eq!(book["hub"], "en");
    assert_eq!(
        book["editions"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["en", "ja"]
    );
    let alignments = book["alignments"].as_array().unwrap();
    assert_eq!(alignments.len(), 1, "{book}");
    assert_eq!(alignments[0]["to"], "ja");
    assert_eq!(
        alignments[0]["pins"],
        serde_json::json!([[1, 1]]),
        "a pin of an edition read holds as it did"
    );
}

#[tokio::test]
async fn a_hub_whose_file_went_since_the_scan_leaves_the_book_to_the_first_edition_read() {
    let tmp = crate::test_support::Fixture::new("book-hub-file-gone");
    let response = file_gone(&tmp, &["en"]).await;
    assert_eq!(response.status(), StatusCode::OK, "the rest still opens");
    let book = json_of(response).await;
    assert_eq!(book["hub"], "ja", "{book}");
    let alignments = book["alignments"].as_array().unwrap();
    assert_eq!(alignments.len(), 1, "{book}");
    assert_eq!(alignments[0]["to"], "ko");
    let links: Vec<[usize; 4]> = serde_json::from_value(alignments[0]["links"].clone()).unwrap();
    assert!(
        links.iter().any(|l| l[0] == 1 && l[2] == 1),
        "a pin of 1 to 1 would hold here: {links:?}"
    );
    assert_eq!(
        alignments[0]["pins"],
        serde_json::json!([]),
        "the pins count the blocks of the hub that was not read: {book}"
    );
}

#[tokio::test]
async fn a_book_none_of_whose_files_can_be_read_is_not_found() {
    let tmp = crate::test_support::Fixture::new("book-files-gone");
    let response = file_gone(&tmp, &["en", "ja", "ko"]).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "unreadable, not a book of none"
    );
}

#[tokio::test]
async fn an_edition_whose_file_is_there_but_does_not_read_closes_the_book() {
    // A file that is there may read on a retry, as one on a share that
    // failed for a moment does; leaving it out would hand the book to another
    // hub until then.
    for spoiled in ["en", "ko"] {
        let tmp = crate::test_support::Fixture::new(&format!("book-file-spoiled-{spoiled}"));
        let state = three_pinned(&tmp, &format!("{spoiled} spoiled")).await;
        std::fs::write(tmp.join(format!("{spoiled}.epub")), "no zip at all").unwrap();
        let response = get(&state, reader()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{spoiled}");
    }
}

#[tokio::test]
async fn a_pin_counted_in_a_hub_sent_in_place_of_the_works_own_is_refused() {
    let tmp = crate::test_support::Fixture::new("book-hub-file-gone-pinned");
    let state = three_pinned(&tmp, "pinned while en is gone").await;
    std::fs::remove_file(tmp.join("en.epub")).unwrap();
    let book = json_of(get(&state, reader()).await).await;
    assert_eq!(book["hub"], "ja", "{book}");
    let held = pins_held(&state);
    assert_eq!(held.len(), 2, "{held:?}");

    // What the reader asks of the book it was sent, counted in `ja`'s blocks.
    // Counted in `en`'s, the first crosses `ko`'s pin and the second names
    // it, so either would take it away.
    for request in [
        r#"{"edition": "ko", "hub": "ja", "hub_block": 2, "block": 0}"#,
        r#"{"edition": "ko", "hub": "ja", "hub_block": 1, "block": null}"#,
    ] {
        assert_eq!(
            put(&state, reader(), request).await,
            StatusCode::CONFLICT,
            "{request}"
        );
    }
    assert_eq!(pins_held(&state), held, "the pins on `en` are as they were");
}

/// A library of one book in each of `keys`, `Rowling/Stone [key]/Stone
/// [key].epub`, keyed by a rule as editions of one work and derived, its hub
/// `keys[0]`, the key that sorts first, and the `ko` edition's block 1 pinned
/// to the hub's block 1; then the hub's file removed, the directory scanned
/// and the library derived again. Answers the works the library then
/// projects, and the one work's slug.
fn hub_gone(dir: &std::path::Path, keys: &[&str]) -> (Arc<crate::ServeState>, String) {
    let tree = dir.join("tree");
    for key in keys {
        let folder = tree.join(format!("Rowling/Stone [{key}]"));
        std::fs::create_dir_all(&folder).unwrap();
        let blocks = match *key {
            "ko" => ["1장", "“누구?”", "아무도."],
            "ja" => ["第一章", "「誰？」", "誰も。"],
            _ => ["CHAPTER ONE", "“Who?”", "Nobody."],
        };
        taiyaku_read::test_support::epub(&folder, &format!("Stone [{key}].epub"), &blocks);
    }
    let library = crate::test_support::library();
    let slug = {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &tree).unwrap();
        taiyaku_library::scan::scan_root(&conn, &root).unwrap();
        conn.execute(
            "INSERT INTO rule (ordinal, kind, pattern, match_on)
             VALUES (99, 'regex', '^Rowling/(?<title>Stone) \\[(?<edition>[a-z]+)\\]/', 'path')",
            [],
        )
        .unwrap();
        taiyaku_library::derive::derive(&conn).unwrap();
        let (work_id, hub, slug): (i64, String, String) = conn
            .query_row("SELECT id, hub, slug FROM work", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .expect("one work of every translation");
        assert_eq!(hub, keys[0]);
        book_pins::pin(
            &conn,
            work_id,
            &book_pins::Pin {
                edition: "ko".to_string(),
                hub_block: 1,
                block: 1,
            },
        )
        .unwrap();

        std::fs::remove_dir_all(tree.join(format!("Rowling/Stone [{}]", keys[0]))).unwrap();
        taiyaku_library::scan::scan_root(&conn, &root).unwrap();
        taiyaku_library::derive::derive(&conn).unwrap();
        slug
    };
    let works = crate::works::from_library(&library).unwrap();
    let mut state = crate::test_support::state(dir);
    state.library = library;
    state.works = crate::Works::fixed(works);
    (Arc::new(state), slug)
}

async fn get_slug(state: &Arc<crate::ServeState>, slug: &str) -> serde_json::Value {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(reader());
    let response = get_book(
        State(Arc::clone(state)),
        AxPath(slug.to_string()),
        extensions,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_of(response).await
}

#[tokio::test]
async fn a_book_whose_hub_file_went_is_sent_from_the_translation_left() {
    let tmp = crate::test_support::Fixture::new("book-hub-gone");
    let (state, slug) = hub_gone(&tmp, &["en", "ko"]);
    let work = state.works.get(&slug).unwrap();
    assert_eq!(
        work.sidecar().hub,
        "en",
        "the work keeps the hub whose file went"
    );

    let book = get_slug(&state, &slug).await;
    assert_eq!(book["hub"], "ko", "{book}");
    assert_eq!(
        book["editions"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["ko"],
        "the edition whose file went is no book"
    );
    assert_eq!(book["editions"]["ko"]["blocks"][1]["text"], "“누구?”");
    assert_eq!(book["alignments"], serde_json::json!([]), "{book}");
}

#[tokio::test]
async fn a_pin_to_a_hub_whose_file_went_holds_between_no_two_translations_left() {
    let tmp = crate::test_support::Fixture::new("book-hub-gone-pinned");
    let (state, slug) = hub_gone(&tmp, &["en", "ja", "ko"]);

    let book = get_slug(&state, &slug).await;
    assert_eq!(book["hub"], "ja", "the first book left: {book}");
    let alignments = book["alignments"].as_array().unwrap();
    assert_eq!(alignments.len(), 1, "{book}");
    assert_eq!(alignments[0]["to"], "ko");
    let links: Vec<[usize; 4]> = serde_json::from_value(alignments[0]["links"].clone()).unwrap();
    assert!(
        links.iter().any(|l| l[0] == 1 && l[2] == 1),
        "a pin of 1 to 1 would hold here: {links:?}"
    );
    assert_eq!(alignments[0]["pins"], serde_json::json!([]), "{book}");

    let mut extensions = axum::http::Extensions::new();
    extensions.insert(reader());
    extensions.insert(crate::auth::By::Cookie);
    let pin = r#"{"edition": "ko", "hub": "ja", "hub_block": 1, "block": 1}"#;
    assert_eq!(
        put_pin(State(state), AxPath(slug), extensions, pin.into())
            .await
            .status(),
        StatusCode::NOT_FOUND,
        "a pin counts the blocks of the work's own hub, which is no book now"
    );
}

#[test]
fn a_sentence_is_counted_in_utf16_units_as_the_browser_counts_it() {
    // An emoji is four bytes, one character and two units.
    let text = "A😀. B.";
    assert_eq!(
        super::utf16_ranges(text, &taiyaku_core::sentences(text)),
        [[0, 4], [5, 7]]
    );
}
