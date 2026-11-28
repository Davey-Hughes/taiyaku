use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::json;
use taiyaku_library::Library;
use taiyaku_library::account::Access;
use taiyaku_library::test_support::{
    Keepers, Kept, LAMP_ALONE, SECOND_LAMP, store_lamp_alone, store_second_lamp,
};

use super::{Tap, get_characters, looked_up};
use crate::ServeState;
use crate::auth::{Principal, Role};
use crate::test_support::{Fixture, body_string};

/// A book as a request names it: its work's slug and its edition's key.
struct Shelved {
    slug: String,
    key: String,
}

/// The keepers shelf: A, B and C each in a directory of its own, their logs
/// and the series stored, and their works served.
struct Shelf {
    state: Arc<ServeState>,
    keepers: Keepers,
    a: Shelved,
    b: Shelved,
}

/// [`taiyaku_library::test_support::keepers`] with all three books shelved on
/// `library`, in a tree under `dir`.
fn keepers_on(library: &Library, dir: &Path) -> Keepers {
    let tree = dir.join("tree");
    std::fs::create_dir_all(&tree).expect("creates the tree");
    taiyaku_library::test_support::keepers(library, &tree, &["A", "B", "C"])
}

/// The slug and the edition key `library` holds the file with digest `sha256`
/// under.
fn shelved(library: &Library, sha256: &str) -> Shelved {
    library
        .conn()
        .query_row(
            "SELECT work.slug, edition.key FROM work
               JOIN edition ON edition.work_id = work.id
               JOIN member ON member.edition_id = edition.id
              WHERE member.sha256 = ?1",
            [sha256],
            |row| {
                Ok(Shelved {
                    slug: row.get(0)?,
                    key: row.get(1)?,
                })
            },
        )
        .expect("the file's one edition")
}

/// A state serving `library`'s works, projected as
/// [`crate::test_support::misnamed`] projects them.
fn serving(dir: &Path, library: Arc<Library>) -> Arc<ServeState> {
    let works = crate::works::from_library(&library).expect("projects the shelved works");
    let mut state = crate::test_support::state(dir);
    state.library = library;
    state.works = crate::Works::fixed(works);
    Arc::new(state)
}

fn shelf(dir: &Path) -> Shelf {
    let library = crate::test_support::library();
    let keepers = keepers_on(&library, dir);
    let a = shelved(&library, &keepers.a.sha256);
    let b = shelved(&library, &keepers.b.sha256);
    Shelf {
        state: serving(dir, library),
        keepers,
        a,
        b,
    }
}

/// A library in a file under `dir`, which a further connection can open as it
/// cannot an in-memory one.
fn file_library(dir: &Path) -> Arc<Library> {
    let db = dir.join("db");
    std::fs::create_dir_all(&db).expect("the database's directory");
    Arc::new(Library::open(&db.join("library.db")).expect("opens"))
}

/// How long [`looked_up`] is given to answer beside a held connection: far
/// longer than a fold takes on a loaded machine, and short of nextest's kill
/// in `.config/nextest.toml`, so a fold that waits fails with its own message.
const ANSWERS_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

/// A work, slug `t`, of two book editions held as a scan holds them under
/// `dir`: `en`, the hub, [`LAMP_ALONE`] with its log stored, and `ko`,
/// [`SECOND_LAMP`] with no log stored, whose digest is answered.
fn two_books(dir: &Path) -> (ServeState, String) {
    let shelf = dir.to_string_lossy().into_owned();
    let en = taiyaku_read::test_support::epub_documents(dir, "en.epub", "", LAMP_ALONE);
    let ko = taiyaku_read::test_support::epub_documents(dir, "ko.epub", "", SECOND_LAMP);
    let (en, ko) = (
        crate::test_support::sha256_of(&en),
        crate::test_support::sha256_of(&ko),
    );
    let sidecar = taiyaku_core::Sidecar::from_json(&format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "T", "reading": "ltr" }},
          "editions": {{
            "en": {{ "label": "English", "members": [
              {{ "path": "{shelf}/en.epub", "sha256": "{en}", "pages": 2 }} ], "spreads": [] }},
            "ko": {{ "label": "Korean", "members": [
              {{ "path": "{shelf}/ko.epub", "sha256": "{ko}", "pages": 2 }} ], "spreads": [] }}
          }},
          "hub": "en",
          "alignments": []
        }}"#
    ))
    .expect("the work's sidecar");
    let mut state = crate::test_support::state(dir);
    crate::test_support::held_as_scanned(
        &state.library,
        dir,
        &sidecar,
        &[("en.epub", "epub-reflow"), ("ko.epub", "epub-reflow")],
    );
    state.works = crate::Works::fixed(vec![crate::Work::new(
        "t".into(),
        dir.join("taiyaku.json"),
        sidecar,
    )]);
    store_lamp_alone(&mut state.library.conn(), &en);
    (state, ko)
}

/// [`two_books`], `ko` filed under a directory of its own, whose id is
/// answered.
fn two_books_apart(dir: &Path) -> (Arc<ServeState>, i64) {
    let (state, _) = two_books(dir);
    let elsewhere = {
        let conn = state.library.conn();
        conn.execute("INSERT INTO root (path) VALUES ('/elsewhere')", [])
            .expect("a second directory");
        let elsewhere = conn.last_insert_rowid();
        conn.execute(
            "UPDATE member SET root_id = ?1 WHERE edition_id = \
             (SELECT id FROM edition WHERE key = 'ko')",
            [elsewhere],
        )
        .expect("files ko under it");
        elsewhere
    };
    (Arc::new(state), elsewhere)
}

/// A reader who may stream and sees every library.
fn reader() -> Principal {
    Principal {
        user_id: 1,
        role: Role::Admin,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access: Access::Every,
    }
}

/// [`get_characters`] for `book` with `query`, asked by `principal`.
async fn ask(
    state: &Arc<ServeState>,
    principal: Principal,
    book: &Shelved,
    query: &[(&str, &str)],
) -> Response {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(principal);
    get_characters(
        State(Arc::clone(state)),
        AxPath((book.slug.clone(), book.key.clone())),
        Query(
            query
                .iter()
                .map(|&(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        ),
        extensions,
    )
    .await
}

async fn json_of(response: Response) -> serde_json::Value {
    let body = body_string(response).await;
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}: {body}"))
}

/// The answer to `query` on `book`, asked by `principal`: a 200's body, which a
/// test that reads the answer needs. The tests of a refusal assert its
/// status themselves.
async fn answer_as(
    state: &Arc<ServeState>,
    principal: Principal,
    book: &Shelved,
    query: &[(&str, &str)],
) -> serde_json::Value {
    let response = ask(state, principal, book, query).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{query:?}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{query:?}: {e}: {body}"))
}

/// [`answer_as`] for [`reader`].
async fn answer(
    state: &Arc<ServeState>,
    book: &Shelved,
    query: &[(&str, &str)],
) -> serde_json::Value {
    answer_as(state, reader(), book, query).await
}

/// The names of the characters `answer` sends, in its order.
fn names(answer: &serde_json::Value) -> Vec<&str> {
    answer["characters"]
        .as_array()
        .unwrap_or_else(|| panic!("a list of characters: {answer}"))
        .iter()
        .map(|card| card["name"]["text"].as_str().expect("a name"))
        .collect()
}

const NO_ONE: [&str; 0] = [];

/// The status and the body of a request on A for [`reader`].
async fn refusal(shelf: &Shelf, query: &[(&str, &str)]) -> (StatusCode, String) {
    let response = ask(&shelf.state, reader(), &shelf.a, query).await;
    (response.status(), body_string(response).await)
}

#[tokio::test]
async fn the_chapter_being_read_is_left_out_to_its_last_block_and_back_from_the_next() {
    let tmp = Fixture::new("characters-cut");
    let shelf = shelf(&tmp);

    let end_of_c2 = answer(
        &shelf.state,
        &shelf.a,
        &[("block", "6"), ("text", "Mrs. Vell said nothing.")],
    )
    .await;
    assert_eq!(
        names(&end_of_c2),
        NO_ONE,
        "Mara comes in c2, still being read"
    );

    let start_of_c3 = answer(
        &shelf.state,
        &shelf.a,
        &[("block", "7"), ("text", "Mara Vell came back.")],
    )
    .await;
    assert_eq!(names(&start_of_c3), ["Mara Vell"], "c2 is finished");

    // Block 6 again, after block 7, so an answer kept from the first ask, or
    // a cut kept from block 7's, would show here.
    let ned = answer(
        &shelf.state,
        &shelf.a,
        &[
            ("block", "6"),
            ("text", "Ned Reed knew his cousin at once."),
        ],
    )
    .await;
    assert_eq!(names(&ned), ["Ned Reed"]);
    let related: Vec<&str> = ned["characters"][0]["relations"]
        .as_array()
        .unwrap_or_else(|| panic!("Ned's relations: {ned}"))
        .iter()
        .map(|relation| relation["with"]["text"].as_str().expect("a name"))
        .collect();
    assert_eq!(related, ["Tom Field"], "his cousin is c2's");
}

#[tokio::test]
async fn the_card_names_haiyakus_titles_not_the_works() {
    let tmp = Fixture::new("characters-titles");
    let shelf = shelf(&tmp);
    let work = shelf.state.works.get(&shelf.a.slug).expect("A's work");
    assert_eq!(
        work.sidecar().work.title,
        "Keepers One",
        "the work is titled unlike its log"
    );

    let reeds = answer(
        &shelf.state,
        &shelf.a,
        &[("block", "9"), ("text", "Reeds put out the lamp.")],
    )
    .await;
    assert_eq!(
        names(&reeds),
        ["Ned Reed", "Ada Reed"],
        "most recently seen first"
    );
    assert_eq!(reeds["characters"][0]["first"]["book"], "The Lamp Keepers");
}

#[tokio::test]
async fn earlier_reaches_the_series_and_zero_keeps_to_the_book() {
    let tmp = Fixture::new("characters-earlier");
    let shelf = shelf(&tmp);
    let tap = [("block", "1"), ("text", "Ned Reed wrote to Wren Hollis.")];
    let with = |more: &[(&'static str, &'static str)]| [&tap[..], more].concat();

    let on = answer(&shelf.state, &shelf.b, &with(&[("earlier", "1")])).await;
    assert_eq!(names(&on), ["Ned Reed"], "Ned is A's");
    assert_eq!(
        on["scope"],
        json!({"series": "keepers", "order": "printed"})
    );

    let off = answer(&shelf.state, &shelf.b, &with(&[("earlier", "0")])).await;
    assert_eq!(names(&off), NO_ONE, "B alone has not met Ned by its c2");

    let unsaid = answer(&shelf.state, &shelf.b, &tap).await;
    assert_eq!(names(&unsaid), ["Ned Reed"], "earlier is on unless it is 0");

    let author = answer(&shelf.state, &shelf.b, &with(&[("order", "author")])).await;
    assert_eq!(names(&author), NO_ONE, "the author's order reads B first");

    let gone = answer(
        &shelf.state,
        &shelf.b,
        &with(&[("series", "keepers"), ("order", "gone")]),
    )
    .await;
    assert_eq!(
        names(&gone),
        ["Ned Reed"],
        "an order the series does not hold falls back to its first"
    );
}

#[tokio::test]
async fn a_restricted_account_outside_the_earlier_book_gets_the_book_alone_and_is_told() {
    let tmp = Fixture::new("characters-narrowed");
    let shelf = shelf(&tmp);
    let root = |kept: &Kept| kept.root.expect("shelved");
    let granted = |roots| Principal {
        role: Role::User,
        access: Access::roots(roots),
        ..reader()
    };
    let tap = [("block", "1"), ("text", "Ned Reed wrote to Wren Hollis.")];

    let b_alone = answer_as(
        &shelf.state,
        granted(vec![root(&shelf.keepers.b)]),
        &shelf.b,
        &tap,
    )
    .await;
    assert_eq!(names(&b_alone), NO_ONE, "A is outside the grant");
    assert_eq!(b_alone["narrowed"], "not-visible");

    let both = answer_as(
        &shelf.state,
        granted(vec![root(&shelf.keepers.a), root(&shelf.keepers.b)]),
        &shelf.b,
        &tap,
    )
    .await;
    assert_eq!(names(&both), ["Ned Reed"]);
}

#[tokio::test]
async fn an_unknown_book_is_not_known() {
    let tmp = Fixture::new("characters-unknown");
    let library = crate::test_support::library();
    keepers_on(&library, &tmp);
    let folder = tmp.join("tree").join("Stranger");
    std::fs::create_dir_all(&folder).expect("creates the stranger's directory");
    let file = taiyaku_read::test_support::epub(&folder, "Stranger v01.epub", &["One."]);
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &folder).expect("adds the stranger");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the stranger");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let stranger = shelved(&library, &crate::test_support::sha256_of(&file));
    let state = serving(&tmp, library);

    let response = ask(
        &state,
        reader(),
        &stranger,
        &[("block", "0"), ("text", "One.")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_of(response).await,
        json!({"known": false, "scope": null, "narrowed": null, "characters": []})
    );
}

#[tokio::test]
async fn a_block_past_the_end_and_a_bad_query_are_400() {
    let tmp = Fixture::new("characters-400");
    let shelf = shelf(&tmp);
    // 𠮷 is two UTF-16 units, one character and four bytes, so a text counted
    // in characters or in bytes is off the bound one way or the other.
    let units = |count: usize| format!("𠮷{}", "a".repeat(count - 2));
    let (longest, longer) = (units(128), units(129));
    let bad = StatusCode::BAD_REQUEST;

    let past = refusal(&shelf, &[("block", "11"), ("text", "Afterword.")]).await;
    assert_eq!(past.0, bad, "a block past the end: {}", past.1);
    let no_number = refusal(&shelf, &[("block", "x"), ("text", "Afterword.")]).await;
    assert_eq!(
        no_number.0, bad,
        "a block that is no number: {}",
        no_number.1
    );
    let no_block = refusal(&shelf, &[("text", "Afterword.")]).await;
    assert_eq!(no_block.0, bad, "no block: {}", no_block.1);
    let no_text = refusal(&shelf, &[("block", "9")]).await;
    assert_eq!(no_text.0, bad, "no text: {}", no_text.1);
    let empty = refusal(&shelf, &[("block", "9"), ("text", "")]).await;
    assert_eq!(empty.0, bad, "an empty text: {}", empty.1);
    let over = refusal(&shelf, &[("block", "9"), ("text", &longer)]).await;
    assert_eq!(over.0, bad, "a text of 129 UTF-16 units: {}", over.1);
    assert!(
        over.1.contains("128"),
        "the refusal names the bound: {}",
        over.1
    );
    let neither = refusal(
        &shelf,
        &[("block", "9"), ("text", "Reeds"), ("earlier", "2")],
    )
    .await;
    assert_eq!(neither.0, bad, "earlier neither 0 nor 1: {}", neither.1);

    let at_the_bound = refusal(&shelf, &[("block", "9"), ("text", &longest)]).await;
    assert_eq!(
        at_the_bound.0,
        StatusCode::OK,
        "a text of 128 UTF-16 units: {}",
        at_the_bound.1
    );
}

#[tokio::test]
async fn the_text_before_the_tap_reaches_the_look_up_and_start_is_sent_unless_0() {
    let tmp = Fixture::new("characters-before");
    let shelf = shelf(&tmp);
    let tap = [("block", "9"), ("text", "Reed put out the lamp.")];
    let with = |before: &'static str| [&tap[..], &[("before", before)]].concat();

    let unsaid = answer(&shelf.state, &shelf.a, &tap).await;
    assert_eq!(names(&unsaid), ["Ned Reed", "Ada Reed"]);
    assert_eq!(
        (unsaid.get("start"), &unsaid["length"]),
        (None, &json!(4)),
        "a name that starts where the text does sends no start"
    );
    let empty = answer(&shelf.state, &shelf.a, &with("")).await;
    assert_eq!(empty, unsaid, "an empty `before` is none");

    let after = answer(&shelf.state, &shelf.a, &with("Ned ")).await;
    assert_eq!(names(&after), ["Ned Reed"]);
    assert_eq!((&after["start"], &after["length"]), (&json!(-4), &json!(8)));
}

#[tokio::test]
async fn a_before_over_its_bound_is_400() {
    let tmp = Fixture::new("characters-before-400");
    let shelf = shelf(&tmp);
    // Counted as `text` is: 𠮷 is two UTF-16 units, one character and four
    // bytes.
    let units = |count: usize| format!("𠮷{}", "a".repeat(count - 2));
    let (longest, longer) = (units(64), units(65));

    let over = refusal(
        &shelf,
        &[("block", "9"), ("text", "Reeds"), ("before", &longer)],
    )
    .await;
    assert_eq!(
        over.0,
        StatusCode::BAD_REQUEST,
        "a `before` of 65 UTF-16 units: {}",
        over.1
    );
    assert!(
        over.1.contains("`before`") && over.1.contains("64"),
        "the refusal names the key and the bound: {}",
        over.1
    );
    let at_the_bound = refusal(
        &shelf,
        &[("block", "9"), ("text", "Reeds"), ("before", &longest)],
    )
    .await;
    assert_eq!(
        at_the_bound.0,
        StatusCode::OK,
        "a `before` of 64 UTF-16 units: {}",
        at_the_bound.1
    );
}

#[tokio::test]
async fn the_skipped_afterword_is_known_with_no_characters() {
    let tmp = Fixture::new("characters-afterword");
    let shelf = shelf(&tmp);

    let response = ask(
        &shelf.state,
        reader(),
        &shelf.a,
        &[("block", "10"), ("text", "Afterword.")],
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the edition's last block"
    );
    assert_eq!(
        json_of(response).await,
        json!({"known": true, "scope": null, "narrowed": null, "characters": []})
    );
}

#[tokio::test]
async fn a_hidden_edition_of_a_visible_work_is_404() {
    let tmp = Fixture::new("characters-hidden-edition");
    let (state, elsewhere) = two_books_apart(&tmp);
    // `ko` alone is under the directory the account is granted, so the work
    // is seen and `en`, a book haiyaku holds, is not.
    let principal = Principal {
        role: Role::User,
        access: Access::roots(vec![elsewhere]),
        ..reader()
    };
    let en = Shelved {
        slug: "t".to_owned(),
        key: "en".to_owned(),
    };
    let tap = [
        ("block", "1"),
        ("text", "Iris Moor left."),
        ("earlier", "0"),
    ];

    let hidden = ask(&state, principal.clone(), &en, &tap).await;
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        names(&answer(&state, &en, &tap).await),
        ["Iris Moor"],
        "the 404 is the grant's"
    );
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(principal);
    let seen = crate::book::get_book(
        State(Arc::clone(&state)),
        AxPath("t".to_owned()),
        extensions,
    )
    .await;
    assert_eq!(
        seen.status(),
        StatusCode::OK,
        "the work itself is seen, by `ko`"
    );
}

#[tokio::test]
async fn the_route_reads_the_edition_its_path_names() {
    let tmp = Fixture::new("characters-edition-named");
    let (state, ko) = two_books(&tmp);
    store_second_lamp(&mut state.library.conn(), &ko);
    let state = Arc::new(state);
    let edition = |key: &str| Shelved {
        slug: "t".to_owned(),
        key: key.to_owned(),
    };
    // The same tap on each edition: `ko` is not the hub, so an answer read
    // from the hub's book would name no one there.
    let tap = [("block", "1"), ("text", "Wren Hollis."), ("earlier", "0")];

    assert_eq!(
        names(&answer(&state, &edition("ko"), &tap).await),
        ["Wren Hollis"]
    );
    assert_eq!(
        names(&answer(&state, &edition("en"), &tap).await),
        NO_ONE,
        "`en`'s book has met no Wren Hollis"
    );
}

#[tokio::test]
async fn an_account_that_may_not_stream_is_refused() {
    let tmp = Fixture::new("characters-no-stream");
    let shelf = shelf(&tmp);
    let principal = Principal {
        can_stream: false,
        role: Role::User,
        ..reader()
    };

    let response = ask(
        &shelf.state,
        principal,
        &shelf.a,
        &[("block", "9"), ("text", "Reeds put out the lamp.")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_edition_that_is_no_book_is_404() {
    let tmp = Fixture::new("characters-comic");
    let (state, _, comic) = crate::test_support::misnamed(&tmp);
    let edition = Shelved {
        slug: comic,
        key: "raw".to_owned(),
    };

    let response = ask(
        &state,
        reader(),
        &edition,
        &[("block", "0"), ("text", "One.")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_missing_work_says_its_files_are_missing() {
    let tmp = Fixture::new("characters-missing");
    let shelf = shelf(&tmp);
    shelf
        .state
        .works
        .get(&shelf.a.slug)
        .expect("A's work")
        .set_missing(true);

    let response = ask(
        &shelf.state,
        reader(),
        &shelf.a,
        &[("block", "9"), ("text", "Reeds put out the lamp.")],
    )
    .await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let said: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    assert_eq!(said["missing"], true, "{body}");
}

#[tokio::test]
async fn with_cards_off_the_route_reads_nothing() {
    let tmp = Fixture::new("characters-cards-off");
    let db = tmp.join("db");
    std::fs::create_dir_all(&db).expect("the database's directory");
    let open = || Arc::new(Library::open(&db.join("library.db")).expect("opens"));
    let keepers = {
        let first = open();
        let keepers = keepers_on(&first, &tmp);
        first
            .conn()
            .execute("UPDATE haiyaku_schema SET version = 3", [])
            .expect("a later haiyaku's version");
        keepers
    };
    let library = open();
    assert!(library.cards_off().is_some(), "cards are off");
    assert_eq!(
        taiyaku_library::characters::holds(&library.conn(), &keepers.a.sha256),
        Ok(true),
        "haiyaku's tables still answer for A, so only cards being off keeps them unread"
    );
    let a = shelved(&library, &keepers.a.sha256);
    let state = serving(&tmp, library);

    let response = ask(
        &state,
        reader(),
        &a,
        &[("block", "9"), ("text", "Reeds put out the lamp.")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_of(response).await,
        json!({"known": false, "scope": null, "narrowed": null, "characters": []})
    );
}

#[tokio::test]
async fn a_haiyaku_read_that_fails_is_500() {
    let tmp = Fixture::new("characters-unread");
    let shelf = shelf(&tmp);
    shelf
        .state
        .library
        .conn()
        .execute_batch("DROP TABLE haiyaku_order_book")
        .expect("drops a table no other table references");

    let response = ask(
        &shelf.state,
        reader(),
        &shelf.a,
        &[("block", "9"), ("text", "Reeds put out the lamp.")],
    )
    .await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(
        body,
        format!(
            "the characters of `{}` of `{}` could not be read; the server's log says why",
            shelf.a.key, shelf.a.slug
        )
    );
}

/// [`looked_up`], the route's blocking half, and not the handler: the
/// handler's edition check reads on the shared connection before the fold, as
/// the book and image routes' does, so it would wait for a lock held through
/// a whole request whatever the fold reads on.
#[test]
fn the_fold_answers_while_another_holds_the_shared_connection() {
    let tmp = Fixture::new("characters-own-connection");
    let library = file_library(&tmp);
    let keepers = keepers_on(&library, &tmp);
    let b = shelved(&library, &keepers.b.sha256);
    let state = serving(&tmp, library);
    let work = state.works.get(&b.slug).expect("B's work");
    let sidecar = work.sidecar();
    let edition = sidecar.editions.get(&b.key).expect("B's edition");
    let [path] = <[PathBuf; 1]>::try_from(crate::download::member_paths(&work, edition))
        .expect("B's one file");
    let tap = Tap {
        block: 1,
        text: "Ned Reed wrote to Wren Hollis.".to_owned(),
        before: String::new(),
        earlier: true,
        series: None,
        order: None,
    };

    let (answers, answered) = std::sync::mpsc::channel();
    let held = state.library.conn();
    let asking = std::thread::spawn({
        let state = Arc::clone(&state);
        let sha256 = keepers.b.sha256.clone();
        move || {
            let _ = answers.send(looked_up(&state, &sha256, &path, &tap, &Access::Every));
        }
    });
    let looked = answered.recv_timeout(ANSWERS_WITHIN);
    // Let go before anything is asserted, so a fold that waited for the
    // connection ends, and its thread with it, rather than outliving the test.
    drop(held);
    asking.join().expect("the asking thread ends");

    let answer = looked
        .expect("the fold answers while another holds the shared connection")
        .expect("the fold reads")
        .expect("block 1 is in B");
    let answer = serde_json::to_value(answer).expect("an answer is JSON");
    assert_eq!(
        names(&answer),
        ["Ned Reed"],
        "Ned is A's, folded through the series"
    );
}

#[tokio::test]
async fn a_connection_the_fold_cannot_open_is_500_and_its_path_is_not_sent() {
    let tmp = Fixture::new("characters-no-connection");
    let library = file_library(&tmp);
    let keepers = keepers_on(&library, &tmp);
    let a = shelved(&library, &keepers.a.sha256);
    let state = serving(&tmp, library);
    // A directory where the database was: the shared connection has the file
    // open and reads on, so the edition check passes, and no further
    // connection opens.
    let db = tmp.join("db").join("library.db");
    std::fs::rename(&db, tmp.join("db").join("library.aside")).expect("moves the database aside");
    std::fs::create_dir(&db).expect("puts a directory where it was");

    let response = ask(
        &state,
        reader(),
        &a,
        &[("block", "9"), ("text", "Reeds put out the lamp.")],
    )
    .await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(
        body,
        format!(
            "the characters of `{}` of `{}` could not be read; the server's log says why",
            a.key, a.slug
        )
    );
}
