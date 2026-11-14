use super::*;
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

/// **`build_router`, shadowed, so every test in this module authenticates.**
///
/// Spec A2 §9 refuses a `--no-auth` flag and a dev bypass: "A bypass flag would
/// be the first real footgun on an internet-facing server, and the convenience
/// it buys is a login screen once a month." A test harness is not a bypass, so
/// this is not one either — it gives the fixture's seeded account a real
/// password and then presents it, through the real guard, on every request.
///
/// Shadowing the glob-imported `super::build_router` rather than editing the
/// `Request::get(...)` calls below: the header is the same on all of them, and
/// threading it through by hand would put the interesting part of each test
/// one line further from its assertion. The guard itself is asserted over in
/// `auth::tests`, which calls the real `build_router` directly.
fn build_router(state: Arc<crate::ServeState>) -> axum::Router {
    let library = Arc::clone(&state.library);
    crate::test_support::authenticating(super::build_router(state), &library)
}

/// The three shapes a reader depends on: the work list to find a work, the
/// sidecar verbatim so the client can resolve without asking again, and a
/// page by index. A 404 for an edition that does not exist matters as much
/// as the successes — a reader that gets 200 and an empty body for a typo
/// shows a blank page and says nothing.
#[tokio::test]
async fn the_reader_can_list_a_work_read_its_sidecar_and_fetch_a_page() {
    let tmp = crate::test_support::Fixture::new("routes");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let list = app
        .clone()
        .oneshot(Request::get("/api/works").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);

    let sidecar = app
        .clone()
        .oneshot(Request::get("/api/works/t").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(sidecar.status(), StatusCode::OK);

    let page = app
        .clone()
        .oneshot(
            Request::get("/api/works/t/raw/pages/2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(
        page.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/jpeg",
        "a page has to say what it is, or only an <img> can use it"
    );
    let bytes = axum::body::to_bytes(page.into_body(), 1 << 20)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"b1.jpg", "page 2 is the second member's first");

    let missing = app
        .oneshot(
            Request::get("/api/works/t/nope/pages/0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// `GET /api/works` answers objects rather than a bare list of ids.
#[tokio::test]
async fn listing_works_answers_objects_rather_than_bare_ids() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_library().await;
    let body =
        crate::test_support::body_string(crate::test_support::get(&router, "/api/works").await)
            .await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    let first = &parsed[0];
    assert!(first["slug"].is_string());
    assert!(first["title"].is_string());
    assert!(first["editions"].is_array());
}

/// `series` and `volume` are answered as `null` — a present key whose value
/// is `null` — rather than being left out of the object. Spec B's scanner is
/// what would give either a value; nothing here invents one. Driven over
/// `router_over_a_library`, the one fixture with a real library row: `w`'s
/// row is inserted by `import::sidecar`, which never sets `series_id` or
/// `volume`, so even a row that exists says nothing about either.
#[tokio::test]
async fn series_and_volume_answer_null_rather_than_being_omitted() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_library().await;
    let body =
        crate::test_support::body_string(crate::test_support::get(&router, "/api/works").await)
            .await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    let first = &parsed[0];
    assert!(
        first.get("series").is_some_and(serde_json::Value::is_null),
        "series must be a present, null field: {first}"
    );
    assert!(
        first.get("volume").is_some_and(serde_json::Value::is_null),
        "volume must be a present, null field: {first}"
    );
}

/// The cover is a URL, not bytes: page 0 of the hub edition, through the
/// thumbnail route that already exists — no new route, and nothing here
/// decodes or generates an image just to build a listing.
#[tokio::test]
async fn the_cover_url_addresses_page_zero_of_the_hub_edition() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_library().await;
    let body =
        crate::test_support::body_string(crate::test_support::get(&router, "/api/works").await)
            .await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(parsed[0]["cover"], "/api/works/w/raw/pages/0/thumb");
}

/// A slug and an edition key outside ASCII are percent-encoded in the cover
/// URL, as `encodeURIComponent` would, so the address the thumbnail route is
/// asked for is the one it names (O262).
#[tokio::test]
async fn the_cover_url_encodes_a_slug_and_an_edition_key_outside_ascii() {
    let (router, _state, _fixture) =
        crate::test_support::router_over_two_scanned_volumes_with_a_non_ascii_title();
    let body =
        crate::test_support::body_string(crate::test_support::get(&router, "/api/works").await)
            .await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    let first = parsed
        .as_array()
        .expect("an array")
        .iter()
        .find(|work| work["slug"] == "kabushikigaisha-magi-lumière-v01")
        .expect("the first volume is listed");
    assert_eq!(
        first["cover"],
        "/api/works/kabushikigaisha-magi-lumi%C3%A8re-v01/%C3%A9dition/pages/0/thumb"
    );

    let thumb = crate::test_support::get(
        &router,
        first["cover"].as_str().expect("the cover is a string"),
    )
    .await;
    assert_eq!(
        thumb.status(),
        axum::http::StatusCode::OK,
        "the thumbnail route answers the address the cover names"
    );
}

/// A work with no library row at all, paired with an empty library
/// (`test_support::state`), answers a slug and a title; only `series` and
/// `volume` degrade, and to the same `null` a row nothing has scanned yet would
/// answer. `load_works` is public and builds works this way, as the
/// corpus-gated tests in `corpus.rs` use it, and must not regress to a null
/// title for whoever calls it that way.
#[tokio::test]
async fn a_work_with_no_library_row_still_lists_with_a_title() {
    let tmp = crate::test_support::Fixture::new("routes-list-no-row");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);
    let body =
        crate::test_support::body_string(crate::test_support::get(&app, "/api/works").await).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(parsed[0]["slug"], "t");
    assert_eq!(parsed[0]["title"], "T");
    assert!(parsed[0]["series"].is_null());
    assert!(parsed[0]["volume"].is_null());
}

/// An out-of-range page index dispatched through the router, on the `/api/…`
/// route specifically: the fixture edition has 4 pages, so index 4 is one past
/// the end.
#[tokio::test]
async fn an_out_of_range_page_index_dispatched_through_the_router_names_the_length() {
    let tmp = crate::test_support::Fixture::new("routes-page-past-end");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/t/raw/pages/4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body).to_string();

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(message.contains("raw"), "must name the edition: {message}");
    assert!(
        message.contains('4'),
        "must say how many pages it has: {message}"
    );
}

/// O5: a non-numeric page index is refused in this server's own words.
///
/// An index past the end names the edition and its length. A bare axum `Path`
/// rejection would answer a 400 phrased in terms of the Rust type it failed to
/// parse into.
///
/// The status stays 400 rather than becoming the 404 the sibling failure
/// gives, deliberately: `pages/4` names a page this edition does not have,
/// and `pages/abc` does not name a page at all. Those are different
/// things and a client is entitled to tell them apart. What they share is
/// that the message comes from here and says what was expected.
#[tokio::test]
async fn a_page_index_that_is_not_a_number_is_refused_in_this_server_s_own_words() {
    let tmp = crate::test_support::Fixture::new("routes-page-nan");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/t/raw/pages/abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body).to_string();

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        message.contains("abc"),
        "must quote what was sent: {message}"
    );
    assert!(
        message.contains("raw"),
        "must name the edition it was sent to: {message}"
    );
    assert!(
        !message.contains("u32"),
        "must not be phrased in terms of a Rust type: {message}"
    );
}

/// A reader reopening a work asks one question and gets one answer, so a
/// work never opened returns 204 rather than 404 — there is nothing wrong,
/// there is simply nothing yet.
#[tokio::test]
async fn progress_round_trips_through_the_api() {
    let tmp = crate::test_support::Fixture::new("progress-api");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let empty = app
        .clone()
        .oneshot(
            Request::get("/api/works/t/progress")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::NO_CONTENT);

    let put = app
        .clone()
        .oneshot(
            Request::put("/api/works/t/progress")
                .header("content-type", "application/json")
                // The fixture's `raw` edition is 4 pages and no spreads,
                // so 3 is its last slot.
                .body(Body::from(r#"{"edition":"raw","slot":3}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    let got = app
        .oneshot(
            Request::get("/api/works/t/progress")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(got.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(got.into_body(), 1 << 16)
        .await
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // Pin the exact shape at both levels: the top-level object carries
    // exactly `positions`, and each element of it carries exactly edition,
    // slot, updated and at — no other top-level field, and no field leaking
    // into (or missing from) an element.
    let top_keys: std::collections::HashSet<_> = doc
        .as_object()
        .expect("response body is a JSON object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        top_keys,
        std::collections::HashSet::from_iter(vec!["positions".to_string()]),
        "progress response must have exactly a `positions` field, no other top-level fields"
    );
    let positions = doc["positions"].as_array().expect("positions is an array");
    assert_eq!(positions.len(), 1, "one write, one position");
    let entry = positions[0].as_object().expect("a position is an object");
    let keys: std::collections::HashSet<_> = entry.keys().cloned().collect();
    assert_eq!(
        keys,
        std::collections::HashSet::from_iter(vec![
            "edition".to_string(),
            "slot".to_string(),
            "updated".to_string(),
            "at".to_string()
        ]),
        "each position must have exactly edition, slot, updated and at fields"
    );

    assert_eq!(entry["edition"].as_str().unwrap(), "raw");
    assert_eq!(entry["slot"].as_u64().unwrap(), 3);
    assert_eq!(
        entry["at"],
        serde_json::json!({ "sha256": "y", "nth": 0, "page": 1 }),
        "slot 3 is c2's second page"
    );
}

/// Every edition the reader holds a position in comes back, newest first.
///
/// Built on `progress_round_trips_through_the_api`'s harness directly above,
/// but on `state_with_two_editions` rather than `state`: the plain `state`
/// fixture's work only declares the `raw` edition, and a `PUT` to an edition
/// it does not have gets a 404 from `put_progress`'s own edition check —
/// proving nothing about ordering. `state_with_two_editions` adds `en` over
/// the same two archive members, so both editions' slots are real ones.
///
/// **Chosen so no other plausible ordering produces the same answer.** `en`
/// is written first and `raw` second, so "first written first" would answer
/// `[en, raw]`; `en` also sorts before `raw`, so "alphabetically first"
/// would answer `[en, raw]` too. Only the real rule — most recently
/// `updated` first — answers `[raw, en]`. `updated` is SQLite's
/// `datetime('now')`, one-second resolution (see `taiyaku-library`'s
/// `positions_come_back_newest_first`), so the two writes' timestamps are
/// forced apart directly in the store rather than trusted to land in
/// different seconds.
#[tokio::test]
async fn every_edition_the_reader_has_a_position_in_comes_back_newest_first() {
    let tmp = crate::test_support::Fixture::new("progress-set");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let library = state.library.clone();
    let app = build_router(state);

    for (edition, slot) in [("en", 1), ("raw", 3)] {
        let put = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/progress")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"edition":"{edition}","slot":{slot}}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::NO_CONTENT, "writing {edition}");
    }

    // Back-date `en`'s position so it cannot tie with `raw`'s — both were
    // just written and `datetime('now')` cannot tell them apart within the
    // same second.
    library
        .conn()
        .execute(
            "UPDATE progress SET updated = '2020-01-01 00:00:00'
              WHERE work_id = (SELECT id FROM work WHERE slug = 't') AND edition = 'en'",
            [],
        )
        .expect("backdating en's position");

    let got = app
        .clone()
        .oneshot(
            Request::get("/api/works/t/progress")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(got.status(), StatusCode::OK);
    let body = axum::body::to_bytes(got.into_body(), 1 << 16)
        .await
        .expect("reading the body");
    let doc: serde_json::Value = serde_json::from_slice(&body).expect("json");

    let positions = doc["positions"].as_array().expect("positions is an array");
    let editions: Vec<&str> = positions
        .iter()
        .map(|p| p["edition"].as_str().expect("an edition"))
        .collect();
    assert_eq!(editions.len(), 2, "both positions, not the newest only");
    assert_eq!(
        editions,
        vec!["raw", "en"],
        "raw is the newest position and must come first — not alphabetically \
         first (`en`) and not the position written first (`en`): got {editions:?}"
    );
}

/// O3: the edition check is only half the guard. A position naming a real
/// edition and a slot past its end is stored garbage in the same way a
/// position naming no edition is — the reader is sent back somewhere that
/// cannot be resolved, and nothing says why. It is also the shape a bug in
/// a client produces, which is exactly when a server should refuse rather
/// than persist.
#[tokio::test]
async fn progress_past_the_end_of_the_edition_is_refused() {
    let tmp = crate::test_support::Fixture::new("progress-past-end");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    // 4 pages, no spreads, so 4 slots — 0 to 3. Slot 4 is one past.
    let put = app
        .clone()
        .oneshot(
            Request::put("/api/works/t/progress")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":4}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = put.status();
    let body = axum::body::to_bytes(put.into_body(), 1 << 16)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body).to_string();

    // Nothing was written, so reopening still says there is no position
    // rather than restoring one that cannot be resolved.
    let after = app
        .oneshot(
            Request::get("/api/works/t/progress")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(message.contains("raw"), "must name the edition: {message}");
    assert!(
        message.contains('4'),
        "must say how many slots it has: {message}"
    );
    assert_eq!(after.status(), StatusCode::NO_CONTENT);
}

/// A position is recorded in **slots**, not pages — `progress.rs` says so
/// and the design turns on it, because a page would lose which half of a
/// joined spread was reached. So the range this checks against is the slot
/// count, and an edition that joins spreads has *more* slots than pages:
/// this one is 4 pages with page 0 a spread, so 5 slots.
///
/// Range-checking against `total_pages()` would pass every other test in
/// this file and refuse the last slot of every spread-carrying edition in
/// the corpus — including One Piece v092, which is spreads throughout.
// The fixture mutation below goes through `Work::update`, whose closure
// is checked by clippy as its own item — see the same allow on
// `works/tests.rs`'s `a_replaced_sidecar_is_what_the_next_reader_sees`.
#[allow(clippy::result_large_err)]
#[tokio::test]
async fn the_last_slot_of_a_spread_carrying_edition_is_in_range() {
    let tmp = crate::test_support::Fixture::new("progress-spread-slot");
    let state = crate::test_support::state(&tmp);
    state.works.snapshot()[0]
        .update(|current| {
            let mut next = (**current).clone();
            next.editions
                .get_mut("raw")
                .expect("the fixture defines `raw`")
                .spreads = vec![0];
            Ok((std::sync::Arc::new(next), ()))
        })
        .expect("the fixture update always succeeds");
    let app = build_router(Arc::new(state));

    let put = app
        .oneshot(
            Request::put("/api/works/t/progress")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":4}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = put.status();
    let body = axum::body::to_bytes(put.into_body(), 1 << 16)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body).to_string();

    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "4 pages with one spread is 5 slots, so slot 4 is the last one: {message}"
    );
}

/// The extensions `auth::guard` leaves on a request from the fixture's
/// account, for a handler called directly.
///
/// **`Access::Every`, and that is chosen.** For a restricted principal, a
/// handler that asks a grant waits on the library's mutex first for that
/// check, and the watchdog measures a call's first wait, so it would measure
/// the check rather than the closures this test is for. The check has its own
/// test, `no_grant_check_stalls_the_runtime`.
///
/// **By a browser session**, for both shared-document writes' gate (O58).
fn as_the_fixture_account() -> axum::http::Extensions {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::Admin,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access: Access::Every,
    });
    extensions.insert(crate::auth::By::Cookie);
    extensions
}

/// **No `/api/works` route does its blocking work on the async worker** — O12's
/// property, held for all ten handlers that have a closure to hold it in.
///
/// Every handler here that reads or writes the library, or reads a page, does
/// it inside `spawn_blocking`, because on a tokio worker that work stalls the
/// whole worker — on this test's `current_thread` runtime, every other
/// request: `serve_page`'s comment gives the reasoning.
///
/// **So each handler is called directly**, through
/// [`crate::test_support::the_runtime_stalls_on`], as
/// `curation::tests::no_curation_route_stalls_the_runtime` calls its six.
/// Driven through the router, `auth::guard`'s own `spawn_blocking` would take
/// the mutex wait before any handler ran, so a handler whose closure sat on the
/// async worker would still pass. The guard is what
/// `auth::tests::the_guard_does_not_stall_the_runtime` sees. Every handler is
/// driven rather than one, because a rule that holds for most of a module is a
/// rule the next handler is written against wrongly. `get_sidecar` is not here
/// because it has no closure: it answers from the sidecar already in memory.
///
/// **The page routes need a source that waits.** `get_page` and `get_thumb`
/// never touch the library for an unrestricted reader, so the watchdog's mutex
/// does not reach what blocks in them; the fixture reads pages through
/// `test_support::PagesWaitOnTheLibrary`, which waits on that mutex first.
///
/// **Every call is asserted to succeed, not only not to stall**, because a
/// handler that refused before reaching its closure — a stale `If-Match`, a
/// body it would not parse — would be free for the wrong reason. Ordered so
/// each can: the progress write before its read, each document read before
/// the write that sends its `ETag` back. The progress read also asserts that
/// the position written comes back.
#[tokio::test(flavor = "current_thread")]
async fn no_works_route_stalls_the_runtime() {
    use crate::test_support::{body_string, the_runtime_stalls_on};
    use axum::body::Bytes;
    use axum::extract::{Path as AxPath, Query, State};
    use axum::http::HeaderMap;

    let (state, _fixture) = crate::test_support::state_whose_pages_wait_on_the_library();
    let library = Arc::clone(&state.library);
    let w = || AxPath("w".to_string());
    let page = || AxPath(("w".to_string(), "raw".to_string(), "0".to_string()));
    let tag_of = |response: &Response| {
        response
            .headers()
            .get(header::ETAG)
            .expect("an ETag")
            .to_str()
            .expect("an ETag is text")
            .to_string()
    };
    let if_match = |tag: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_MATCH,
            tag.parse().expect("an ETag is a header value"),
        );
        headers
    };

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        list_works(State(Arc::clone(&state)), as_the_fixture_account()),
    )
    .await;
    assert!(
        !stalled,
        "list_works (GET /api/works) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let position: crate::progress::Body =
        serde_json::from_str(r#"{"edition":"raw","slot":1}"#).expect("a position");
    let (stalled, response) = the_runtime_stalls_on(
        &library,
        put_progress(
            State(Arc::clone(&state)),
            as_the_fixture_account(),
            w(),
            axum::Json(position),
        ),
    )
    .await;
    assert!(
        !stalled,
        "put_progress (PUT /api/works/{{work}}/progress) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_progress(State(Arc::clone(&state)), as_the_fixture_account(), w()),
    )
    .await;
    assert!(
        !stalled,
        "get_progress (GET /api/works/{{work}}/progress) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);
    let doc: serde_json::Value =
        serde_json::from_str(&body_string(response).await).expect("a progress document");
    let positions = doc["positions"].as_array().expect("positions is an array");
    assert_eq!(positions.len(), 1, "one write, one position");
    assert_eq!(positions[0]["edition"].as_str(), Some("raw"));
    assert_eq!(
        positions[0]["slot"].as_u64(),
        Some(1),
        "the position written above is the one that comes back"
    );

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_notes(
            State(Arc::clone(&state)),
            as_the_fixture_account(),
            w(),
            HeaderMap::new(),
        ),
    )
    .await;
    assert!(
        !stalled,
        "get_notes (GET /api/works/{{work}}/notes) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);
    let tag = tag_of(&response);
    let notes = body_string(response).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        put_notes(
            State(Arc::clone(&state)),
            as_the_fixture_account(),
            w(),
            if_match(&tag),
            Bytes::from(notes),
        ),
    )
    .await;
    assert!(
        !stalled,
        "put_notes (PUT /api/works/{{work}}/notes) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_truth(
            State(Arc::clone(&state)),
            as_the_fixture_account(),
            w(),
            HeaderMap::new(),
        ),
    )
    .await;
    assert!(
        !stalled,
        "get_truth (GET /api/works/{{work}}/truth) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);
    let tag = tag_of(&response);
    let truth = body_string(response).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        put_truth(
            State(Arc::clone(&state)),
            as_the_fixture_account(),
            w(),
            if_match(&tag),
            Bytes::from(truth),
        ),
    )
    .await;
    assert!(
        !stalled,
        "put_truth (PUT /api/works/{{work}}/truth) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_page(
            State(Arc::clone(&state)),
            page(),
            Query(PageQuery { w: None }),
            HeaderMap::new(),
            as_the_fixture_account(),
        ),
    )
    .await;
    assert!(
        !stalled,
        "get_page (GET /api/works/{{work}}/{{edition}}/pages/{{index}}) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_thumb(
            State(Arc::clone(&state)),
            page(),
            Query(Vec::new()),
            HeaderMap::new(),
            as_the_fixture_account(),
        ),
    )
    .await;
    assert!(
        !stalled,
        "get_thumb (GET /api/works/{{work}}/{{edition}}/pages/{{index}}/thumb) ran on the async \
         worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    // O133: a library cover — the tile route at a cover size — takes the other
    // branch, through `serve_page`, and is driven on its own.
    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_thumb(
            State(Arc::clone(&state)),
            page(),
            Query(vec![("w".to_string(), "256".to_string())]),
            HeaderMap::new(),
            as_the_fixture_account(),
        ),
    )
    .await;
    assert!(
        !stalled,
        "get_thumb at a cover size (GET …/pages/0/thumb?w=256) ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let tag = sidecar_etag(
        &state
            .find_blocking("w")
            .expect("the fixture work")
            .sidecar(),
    );
    let (stalled, response) = the_runtime_stalls_on(
        &library,
        patch_alignments(
            State(Arc::clone(&state)),
            w(),
            if_match(&tag),
            as_the_fixture_account(),
            Bytes::from_static(br#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#),
        ),
    )
    .await;
    assert!(
        !stalled,
        "patch_alignments (PATCH /api/works/{{work}}/alignments) ran on the async worker"
    );
    let status = response.status();
    assert_eq!(status, StatusCode::OK, "{}", body_string(response).await);
}

/// The extensions `auth::guard` leaves on a request from a reader granted
/// `roots`, for a handler called directly: the fixture account as
/// [`crate::test_support::grant_root`] leaves it, demoted, because an
/// administrator's access is always `Every` (O86).
fn as_a_reader_granted(roots: Vec<i64>) -> axum::http::Extensions {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::User,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access: Access::roots(roots),
    });
    extensions
}

/// `response`'s status, or a panic naming `handler` and quoting the body.
async fn assert_answered(handler: &str, response: Response, expected: StatusCode) {
    let status = response.status();
    assert_eq!(
        status,
        expected,
        "{handler}: {}",
        crate::test_support::body_string(response).await
    );
}

/// **No grant check stalls the runtime** — O115: the property
/// [`no_works_route_stalls_the_runtime`] holds for a handler's own work, held
/// for the check that runs before it.
///
/// For a reader whose grant names roots, `view::of_edition` and
/// `view::of_work` read the library to answer. The test above
/// drives `Access::Every`, which never reaches the library, so it cannot see
/// either check's read.
///
/// **Every handler that calls either check.** `view::of_edition` is
/// `pub(crate)`, and `opds::stream_page`, `opds::cover`, `download::edition`
/// and `download::member` call it too. `opds::work_feed` asks
/// `view::of_work`, and
/// [`neither_opds_feed_asks_a_grant_on_the_async_worker`] holds it
/// beside `opds::works`.
///
/// **Progress and notes, read and write, and the truth read** ask
/// `view::of_work` first. The truth write and the correction, an
/// administrator's (O58), ask none.
///
/// **The watchdog measures each call's first wait on the library's mutex**:
/// [`crate::test_support::the_runtime_stalls_on`] holds it once and lets it go
/// when the canary runs. For every handler here that first wait is the grant
/// check, and a second wait on the async worker after it would pass.
///
/// **Every call is asserted to succeed**, because a handler refused before its
/// check reached the library would be free for the wrong reason. Root 1 holds
/// every member of the fixture's work. The stalls are collected rather than
/// asserted one at a time, so a failing run names every handler that stalled.
///
/// **Then every handler again for a reader granted nothing, each asserted to
/// answer 404.** `Only` with no directory is asked of the library like any other
/// restricted access, so the watchdog still measures the check, and the 404
/// is what that check answers. The writes are here because no other test
/// sends one with this access.
#[tokio::test(flavor = "current_thread")]
async fn no_grant_check_stalls_the_runtime() {
    use crate::test_support::the_runtime_stalls_on;
    use axum::body::Bytes;
    use axum::extract::{Path as AxPath, Query, State};
    use axum::http::HeaderMap;

    let (state, _fixture) = crate::test_support::state_whose_grant_waits_on_the_library();
    let library = Arc::clone(&state.library);
    let granted = || as_a_reader_granted(vec![1]);
    let w = || AxPath("w".to_string());
    let edition = || AxPath(("w".to_string(), "raw".to_string()));
    let page = || AxPath(("w".to_string(), "raw".to_string(), "0".to_string()));
    let mut stalled_on = Vec::new();

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_sidecar(State(Arc::clone(&state)), w(), HeaderMap::new(), granted()),
    )
    .await;
    if stalled {
        stalled_on.push("get_sidecar");
    }
    assert_answered("get_sidecar", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_page(
            State(Arc::clone(&state)),
            page(),
            Query(PageQuery { w: None }),
            HeaderMap::new(),
            granted(),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("get_page");
    }
    assert_answered("get_page", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_thumb(
            State(Arc::clone(&state)),
            page(),
            Query(Vec::new()),
            HeaderMap::new(),
            granted(),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("get_thumb");
    }
    assert_answered("get_thumb", response, StatusCode::OK).await;

    // This watchdog sees a call's first wait, which for a reader with roots is
    // `view::of_edition`'s, so this re-proves only the grant check on the sized
    // branch; `no_works_route_stalls_the_runtime`, with `Access::Every`, is the
    // test that sees past it into `serve_page`.
    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_thumb(
            State(Arc::clone(&state)),
            page(),
            Query(vec![("w".to_string(), "256".to_string())]),
            HeaderMap::new(),
            granted(),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("get_thumb at a cover size");
    }
    assert_answered("get_thumb at a cover size", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        crate::opds::stream_page(
            State(Arc::clone(&state)),
            page(),
            HeaderMap::new(),
            granted(),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("opds::stream_page");
    }
    assert_answered("opds::stream_page", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        crate::opds::cover(State(Arc::clone(&state)), edition(), granted()),
    )
    .await;
    if stalled {
        stalled_on.push("opds::cover");
    }
    assert_answered("opds::cover", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        crate::download::edition(
            State(Arc::clone(&state)),
            edition(),
            HeaderMap::new(),
            granted(),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("download::edition");
    }
    assert_answered("download::edition", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        crate::download::member(
            State(Arc::clone(&state)),
            page(),
            HeaderMap::new(),
            granted(),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("download::member");
    }
    assert_answered("download::member", response, StatusCode::OK).await;

    // O60: progress, notes and the truth read ask the grant too. Ordered as
    // `no_works_route_stalls_the_runtime` orders them, so each can succeed:
    // the progress write before its read, the notes read before the write
    // that sends its `ETag` back.
    let tag_of = |response: &Response| {
        response
            .headers()
            .get(header::ETAG)
            .expect("an ETag")
            .to_str()
            .expect("an ETag is text")
            .to_string()
    };
    let with_tag = |tag: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_MATCH,
            tag.parse().expect("an ETag is a header value"),
        );
        headers
    };

    let position: crate::progress::Body =
        serde_json::from_str(r#"{"edition":"raw","slot":1}"#).expect("a position");
    let (stalled, response) = the_runtime_stalls_on(
        &library,
        put_progress(
            State(Arc::clone(&state)),
            granted(),
            w(),
            axum::Json(position),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("put_progress");
    }
    assert_answered("put_progress", response, StatusCode::NO_CONTENT).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_progress(State(Arc::clone(&state)), granted(), w()),
    )
    .await;
    if stalled {
        stalled_on.push("get_progress");
    }
    assert_answered("get_progress", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_notes(State(Arc::clone(&state)), granted(), w(), HeaderMap::new()),
    )
    .await;
    if stalled {
        stalled_on.push("get_notes");
    }
    assert_eq!(response.status(), StatusCode::OK, "get_notes");
    let tag = tag_of(&response);
    let notes = crate::test_support::body_string(response).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        put_notes(
            State(Arc::clone(&state)),
            granted(),
            w(),
            with_tag(&tag),
            Bytes::from(notes),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("put_notes");
    }
    assert_answered("put_notes", response, StatusCode::OK).await;

    let (stalled, response) = the_runtime_stalls_on(
        &library,
        get_truth(State(Arc::clone(&state)), granted(), w(), HeaderMap::new()),
    )
    .await;
    if stalled {
        stalled_on.push("get_truth");
    }
    assert_answered("get_truth", response, StatusCode::OK).await;

    // Every handler again, for a reader granted nothing. Each request is the
    // one granted root 1 just answered, with the validators it holds now, so
    // the 404 is the access's: the hidden-work refusal comes before any
    // precondition or body is read.
    let nothing = || as_a_reader_granted(Vec::new());
    let response = get_notes(State(Arc::clone(&state)), granted(), w(), HeaderMap::new()).await;
    let notes_tag = tag_of(&response);
    let notes = crate::test_support::body_string(response).await;
    let position: crate::progress::Body =
        serde_json::from_str(r#"{"edition":"raw","slot":1}"#).expect("a position");
    type Call = std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>;
    let calls: Vec<(&str, Call)> = vec![
        (
            "get_sidecar, granted nothing",
            Box::pin(get_sidecar(
                State(Arc::clone(&state)),
                w(),
                HeaderMap::new(),
                nothing(),
            )),
        ),
        (
            "get_page, granted nothing",
            Box::pin(get_page(
                State(Arc::clone(&state)),
                page(),
                Query(PageQuery { w: None }),
                HeaderMap::new(),
                nothing(),
            )),
        ),
        (
            "get_thumb, granted nothing",
            Box::pin(get_thumb(
                State(Arc::clone(&state)),
                page(),
                Query(Vec::new()),
                HeaderMap::new(),
                nothing(),
            )),
        ),
        (
            "get_thumb at a cover size, granted nothing",
            Box::pin(get_thumb(
                State(Arc::clone(&state)),
                page(),
                Query(vec![("w".to_string(), "256".to_string())]),
                HeaderMap::new(),
                nothing(),
            )),
        ),
        (
            "opds::stream_page, granted nothing",
            Box::pin(crate::opds::stream_page(
                State(Arc::clone(&state)),
                page(),
                HeaderMap::new(),
                nothing(),
            )),
        ),
        (
            "opds::cover, granted nothing",
            Box::pin(crate::opds::cover(
                State(Arc::clone(&state)),
                edition(),
                nothing(),
            )),
        ),
        (
            "download::edition, granted nothing",
            Box::pin(crate::download::edition(
                State(Arc::clone(&state)),
                edition(),
                HeaderMap::new(),
                nothing(),
            )),
        ),
        (
            "download::member, granted nothing",
            Box::pin(crate::download::member(
                State(Arc::clone(&state)),
                page(),
                HeaderMap::new(),
                nothing(),
            )),
        ),
        (
            "put_progress, granted nothing",
            Box::pin(put_progress(
                State(Arc::clone(&state)),
                nothing(),
                w(),
                axum::Json(position),
            )),
        ),
        (
            "get_progress, granted nothing",
            Box::pin(get_progress(State(Arc::clone(&state)), nothing(), w())),
        ),
        (
            "get_notes, granted nothing",
            Box::pin(get_notes(
                State(Arc::clone(&state)),
                nothing(),
                w(),
                HeaderMap::new(),
            )),
        ),
        (
            "put_notes, granted nothing",
            Box::pin(put_notes(
                State(Arc::clone(&state)),
                nothing(),
                w(),
                with_tag(&notes_tag),
                Bytes::from(notes),
            )),
        ),
        (
            "get_truth, granted nothing",
            Box::pin(get_truth(
                State(Arc::clone(&state)),
                nothing(),
                w(),
                HeaderMap::new(),
            )),
        ),
    ];
    let mut wrong = Vec::new();
    for (handler, call) in calls {
        let (stalled, response) = the_runtime_stalls_on(&library, call).await;
        if stalled {
            stalled_on.push(handler);
        }
        let status = response.status();
        if status != StatusCode::NOT_FOUND {
            // Lossy, because a page or a tile is not text.
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .expect("reads the body");
            wrong.push(format!(
                "{handler}: answered {status}, not 404: {}",
                String::from_utf8_lossy(&bytes)
            ));
        }
    }

    assert!(
        stalled_on.is_empty(),
        "these asked the library on the async worker: {stalled_on:?}"
    );
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// **Neither OPDS feed asks a grant on the async worker** — O119: the property
/// [`no_grant_check_stalls_the_runtime`] holds for the handlers it drives,
/// held for the two OPDS feeds.
///
/// **Each access, because they take different paths**: one that names
/// directories is asked of the library, and `Every` is answered without it.
/// `Only` with no directory is asked of the library too, and must find nothing
/// there: the works feed answers 200 listing no work, and the work's feed the
/// 404 a work outside the grant gets. Every other call is asserted to answer
/// 200, so neither feed is free for having been refused, and the stalls are
/// collected so a failing run names every call that stalled.
/// The watchdog sees a call's first wait on the library's mutex, and in both
/// feeds that wait is the grant check: nothing after it takes the library.
#[tokio::test(flavor = "current_thread")]
async fn neither_opds_feed_asks_a_grant_on_the_async_worker() {
    use crate::test_support::the_runtime_stalls_on;
    use axum::extract::{Path as AxPath, State};

    let (state, _fixture) = crate::test_support::state_whose_grant_waits_on_the_library();
    let library = Arc::clone(&state.library);
    let mut stalled_on = Vec::new();

    for (grant, principal, sees) in [
        ("granted root 1", as_a_reader_granted(vec![1]), true),
        ("every library", as_the_fixture_account(), true),
        ("granted nothing", as_a_reader_granted(Vec::new()), false),
    ] {
        let call = format!("opds::works, {grant}");
        let (stalled, response) = the_runtime_stalls_on(
            &library,
            crate::opds::works(State(Arc::clone(&state)), principal.clone()),
        )
        .await;
        if stalled {
            stalled_on.push(call.clone());
        }
        let status = response.status();
        let body = crate::test_support::body_string(response).await;
        assert_eq!(status, StatusCode::OK, "{call}: {body}");
        assert_eq!(body.contains("<entry>"), sees, "{call}: {body}");

        let call = format!("opds::work_feed, {grant}");
        let (stalled, response) = the_runtime_stalls_on(
            &library,
            crate::opds::work_feed(
                State(Arc::clone(&state)),
                AxPath("w".to_string()),
                principal,
            ),
        )
        .await;
        if stalled {
            stalled_on.push(call.clone());
        }
        let expected = if sees {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        assert_answered(&call, response, expected).await;
    }

    assert!(
        stalled_on.is_empty(),
        "these asked the library on the async worker: {stalled_on:?}"
    );
}

/// **A hidden work is refused before its validator is answered.** `get_page`,
/// `get_sidecar` and `opds::stream_page` each answer a matching
/// `If-None-Match` with a 304 before reading anything, and each asks the grant
/// first, so a reader granted a root holding nothing of the work is told the
/// work does not exist rather than that the tag it sent is current. That order
/// is what this holds.
///
/// **Granted root 1, which holds the work, the same requests answer 304.**
/// That half shows the tags are the real ones: a tag that matched nothing
/// would be refused by a hidden work whichever order a handler asked in.
#[tokio::test]
async fn a_hidden_work_is_refused_before_its_validator_is_answered() {
    use axum::extract::{Path as AxPath, Query, State};
    use axum::http::HeaderMap;

    let (state, _fixture) = crate::test_support::state_whose_grant_waits_on_the_library();
    let found = state.find_blocking("w").expect("the fixture work");
    let whole = crate::view::EditionView::build(Arc::clone(&found), "raw", None)
        .expect("an edition is never hidden from `Access::Every`");
    let (sha256, offset) = whole
        .locate(0)
        .map(|(member, offset)| (member.sha256.clone(), offset))
        .expect("page 0 of `raw`");
    let page_tag = page_etag(&sha256, offset, None);
    let sidecar_tag = sidecar_etag(&found.sidecar());
    let if_none_match = |tag: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            tag.parse().expect("an ETag is a header value"),
        );
        headers
    };
    let page = || AxPath(("w".to_string(), "raw".to_string(), "0".to_string()));

    for (root, expected) in [(1, StatusCode::NOT_MODIFIED), (2, StatusCode::NOT_FOUND)] {
        let response = get_page(
            State(Arc::clone(&state)),
            page(),
            Query(PageQuery { w: None }),
            if_none_match(&page_tag),
            as_a_reader_granted(vec![root]),
        )
        .await;
        assert_answered(
            &format!("get_page, granted root {root}"),
            response,
            expected,
        )
        .await;

        let response = get_sidecar(
            State(Arc::clone(&state)),
            AxPath("w".to_string()),
            if_none_match(&sidecar_tag),
            as_a_reader_granted(vec![root]),
        )
        .await;
        assert_answered(
            &format!("get_sidecar, granted root {root}"),
            response,
            expected,
        )
        .await;

        let response = crate::opds::stream_page(
            State(Arc::clone(&state)),
            page(),
            if_none_match(&page_tag),
            as_a_reader_granted(vec![root]),
        )
        .await;
        assert_answered(
            &format!("opds::stream_page, granted root {root}"),
            response,
            expected,
        )
        .await;
    }
}

/// The other half of what progress has to get right. A slot means nothing
/// without the edition it is a slot of, so a body naming an edition this
/// work does not have is stored garbage: the reader is sent back to a
/// position in a document that cannot be resolved, and nothing anywhere
/// says why. Accepting it silently is worse than 404, which is why the
/// guard exists.
#[tokio::test]
async fn progress_for_an_edition_the_work_does_not_have_is_refused() {
    let tmp = crate::test_support::Fixture::new("progress-edition");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let put = app
        .clone()
        .oneshot(
            Request::put("/api/works/t/progress")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"nope","slot":37}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(put.into_body(), 1 << 16)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body).to_string();

    // Nothing was written, so a reader that reopens the work still gets
    // 204 rather than a position it cannot resolve.
    let after = app
        .oneshot(
            Request::get("/api/works/t/progress")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(message.contains("nope"), "must name the edition: {message}");
    assert_eq!(after.status(), StatusCode::NO_CONTENT);
}

/// O1: a page's `ETag` is checkable before anything expensive
/// runs, and a client that already holds the bytes gets told so with no
/// body at all — sending one would defeat the entire point of a `304`.
#[tokio::test]
async fn a_page_carries_an_etag_and_a_matching_request_is_answered_304() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let first = app
        .clone()
        .oneshot(
            Request::get("/api/works/w/a/pages/0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let tag = first.headers()[header::ETAG].clone();

    let second = app
        .oneshot(
            Request::get("/api/works/w/a/pages/0")
                .header(header::IF_NONE_MATCH, tag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
    let body = axum::body::to_bytes(second.into_body(), 1 << 16)
        .await
        .unwrap();
    assert!(
        body.is_empty(),
        "a 304 must carry no body — that is the entire point of sending one"
    );
}

/// The reading-size parameter, over a fixture page of 16x16: `?w=8` is
/// smaller than the page and must resize, and the output is declared JPEG
/// whatever the archive held (this fixture holds PNG).
#[tokio::test]
async fn a_width_smaller_than_the_page_resizes_it_and_declares_jpeg() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/w/a/pages/0?w=8")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let decoded = image::load_from_memory(&body).expect("a decodable image");
    assert_eq!(
        (decoded.width().max(decoded.height())),
        8,
        "the long edge must be what was asked for"
    );
}

/// The half that makes this safe on every edition: a page already within
/// the requested edge is handed back untouched rather than re-encoded, so
/// asking for a size no larger than the source costs nothing and loses no
/// quality. On the real corpus this is `raw-mono` and `en-fan`, which are
/// frequently smaller than a viewport asks for.
#[tokio::test]
async fn a_width_larger_than_the_page_serves_the_original_bytes_untouched() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let plain = app
        .clone()
        .oneshot(
            Request::get("/api/works/w/a/pages/0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let plain_type = plain.headers()[header::CONTENT_TYPE].clone();
    let plain_bytes = axum::body::to_bytes(plain.into_body(), 1 << 20)
        .await
        .unwrap();

    let wide = app
        .oneshot(
            Request::get("/api/works/w/a/pages/0?w=700")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wide.status(), StatusCode::OK);
    assert_eq!(wide.headers()[header::CONTENT_TYPE], plain_type);
    let wide_bytes = axum::body::to_bytes(wide.into_body(), 1 << 20)
        .await
        .unwrap();
    assert_eq!(
        wide_bytes, plain_bytes,
        "a page already small enough must not be re-encoded"
    );
}

/// The trap this parameter introduces. One page has a representation per
/// size, so a tag that named only the member and offset would let a client
/// holding the small one be answered 304 for the large one and go on drawing
/// a soft page with nothing to point at.
#[tokio::test]
async fn a_validator_from_one_size_does_not_satisfy_a_request_for_another() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let small = app
        .clone()
        .oneshot(
            Request::get("/api/works/w/a/pages/0?w=8")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let small_tag = small.headers()[header::ETAG].clone();

    let full = app
        .clone()
        .oneshot(
            Request::get("/api/works/w/a/pages/0")
                .header(header::IF_NONE_MATCH, small_tag.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        full.status(),
        StatusCode::OK,
        "the full-size page must not be short-circuited by a resized page's tag"
    );
    assert_ne!(full.headers()[header::ETAG], small_tag);

    // And the same size still validates, so the parameter does not cost the
    // conditional request that O1 provides.
    let again = app
        .oneshot(
            Request::get("/api/works/w/a/pages/0?w=8")
                .header(header::IF_NONE_MATCH, small_tag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
}

/// O106: a resized page's tag is its cache key in quotes, so that bumping
/// `PAGE_FORMAT_VERSION` cannot regenerate every cached page while every
/// browser keeps validating against its old copy and gets a `304` for it.
///
/// Compared with the file name, not with a typed-out string, so that a
/// parameter later added to one side must reach the other.
#[test]
fn a_resized_page_tag_is_the_cache_key_it_was_rendered_under() {
    let path = crate::cover::page_cache_path(std::path::Path::new("/cache"), "abc", 3, 800);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
    assert_eq!(page_etag("abc", 3, Some(800)), format!("\"{stem}\""));
}

/// O106, from the client's side: a tag issued before the format version was
/// part of it, `"<sha256>-p<offset>-r<edge>"`, must not validate. Answering it
/// with `304` would leave a browser that holds it validating against a
/// rendering from before the format version.
///
/// The old tag is built from the full-size tag, `"<sha256>-p<offset>"`. The
/// first assertion checks that the full-size tag has that shape, so the old
/// tag below is built from a real full-size tag.
#[tokio::test]
async fn a_resized_page_tag_from_before_the_format_version_is_not_honoured() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let full = etag_of(&app, "/api/works/w/a/pages/0").await;
    assert!(full.ends_with("-p0\""), "the full-size tag's shape: {full}");
    let old = format!("{}-r8\"", full.trim_end_matches('"'));

    let response = app
        .oneshot(
            Request::get("/api/works/w/a/pages/0?w=8")
                .header(header::IF_NONE_MATCH, old.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{old} names a rendering from before the format version was in the tag"
    );
    assert_ne!(response.headers()[header::ETAG], old.as_str());
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    assert!(
        body.starts_with(b"\xff\xd8\xff"),
        "the page itself, resized"
    );
}

/// O106: a resized page's tag is built from the clamped width, which is the
/// width its cache file is written under. Built from `?w=` as sent, a request
/// above `MAX_FIT_LONG_EDGE` would be served the ceiling's rendering under a
/// tag of its own, and raising the ceiling would change what that request is
/// served and leave its tag alone. The tag-equals-key test calls `page_etag`
/// with literal arguments and never goes through `get_page`.
#[tokio::test]
async fn a_width_above_the_ceiling_carries_the_ceilings_tag() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);
    let ceiling = crate::cover::MAX_FIT_LONG_EDGE;

    let at = etag_of(&app, &format!("/api/works/w/a/pages/0?w={ceiling}")).await;
    let above = etag_of(&app, &format!("/api/works/w/a/pages/0?w={}", ceiling + 1)).await;

    assert!(
        at.contains(&format!("-r{ceiling}v")),
        "the ceiling's own tag: {at}"
    );
    assert_eq!(above, at, "a width above the ceiling is served the ceiling");
}

/// A width outside the sensible range is clamped rather than refused: a
/// hand-typed URL or a client bug should still draw a page. The ceiling is
/// what stops one cache entry per pixel value.
#[tokio::test]
async fn an_absurd_width_is_clamped_rather_than_refused() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    for query in ["?w=0", "?w=1", "?w=999999", "?w=4096"] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/api/works/w/a/pages/0{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "for {query}");
    }
}

/// A validator naming a *different* page must not short-circuit this
/// one — `If-None-Match` says "I already have the page this tag names",
/// and a stale tag names something else entirely.
#[tokio::test]
async fn a_stale_validator_gets_the_page_rather_than_a_304() {
    let tmp = crate::test_support::Fixture::new("routes-etag-stale");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let first = app
        .clone()
        .oneshot(
            Request::get("/api/works/t/raw/pages/0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let stale_tag = first.headers()[header::ETAG].clone();

    let second = app
        .oneshot(
            Request::get("/api/works/t/raw/pages/2")
                .header(header::IF_NONE_MATCH, stale_tag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = second.status();
    let bytes = axum::body::to_bytes(second.into_body(), 1 << 16)
        .await
        .unwrap();

    assert_eq!(
        status,
        StatusCode::OK,
        "a tag naming a different page must not answer 304"
    );
    assert_eq!(
        &bytes[..],
        b"b1.jpg",
        "the actual page must still be served, not an empty 304 body"
    );
}

/// A member has one `sha256` and, in this fixture, two pages. This test would
/// catch keying an `ETag` on the member hash alone, which would answer every
/// page after the first with the first page's own tag.
#[tokio::test]
async fn two_pages_of_one_member_have_different_etags() {
    let tmp = crate::test_support::Fixture::new("routes-etag-member");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    // Pages 0 and 1 of `raw` are both `c1.cbz`'s pages (see
    // `test_support::work`) — one member, two offsets.
    let page0 = app
        .clone()
        .oneshot(
            Request::get("/api/works/t/raw/pages/0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page0.status(), StatusCode::OK);
    let tag0 = page0.headers()[header::ETAG].clone();

    let page1 = app
        .oneshot(
            Request::get("/api/works/t/raw/pages/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page1.status(), StatusCode::OK);
    let tag1 = page1.headers()[header::ETAG].clone();

    assert_ne!(
        tag0, tag1,
        "page 0 and page 1 share one member and its one sha256 — the \
             offset is the only thing that can tell them apart, and a tag \
             keyed on the member hash alone would make these equal"
    );
}

/// A GET through `router`: its status, headers and body, sent with an
/// `If-None-Match` when there is a tag to send.
async fn fetch(
    router: &axum::Router,
    uri: &str,
    if_none_match: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, axum::body::Bytes) {
    let mut request = Request::get(uri);
    if let Some(tag) = if_none_match {
        request = request.header(header::IF_NONE_MATCH, tag);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, headers, body)
}

/// O108: a page's tag names its member and its offset within that member,
/// not its index in the edition, at full size and resized alike.
///
/// Page 2 of `ab` and page 0 of `b` are one page of one archive (see
/// `test_support::state_with_a_shared_member`), so a client holding either
/// holds the other. Page 2 of `b` is a different page of that archive, at the
/// index that page 2 of `ab` has. A tag built from the index would answer that
/// page's tag with `304`, and the client would go on drawing the wrong picture.
#[tokio::test]
async fn a_page_tag_names_its_offset_in_its_member_not_its_index_in_the_edition() {
    let (state, _fixture) = crate::test_support::state_with_a_shared_member();
    let app = build_router(state);

    for size in ["", "?w=8"] {
        let same_page = etag_of(&app, &format!("/api/works/m/b/pages/0{size}")).await;
        let same_index = etag_of(&app, &format!("/api/works/m/b/pages/2{size}")).await;
        let uri = format!("/api/works/m/ab/pages/2{size}");

        let (status, _, body) = fetch(&app, &uri, Some(&same_index)).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{uri} sent page 2 of `b`'s tag {same_index}, which is a different page"
        );
        assert_eq!(crate::test_support::colour_of(&body), "red", "{uri}");

        let (status, _, _) = fetch(&app, &uri, Some(&same_page)).await;
        assert_eq!(
            status,
            StatusCode::NOT_MODIFIED,
            "{uri} sent page 0 of `b`'s tag {same_page}, which is the same page"
        );
    }
}

/// O108: a reading-sized page is cached under its member's hash and its
/// offset within that member.
///
/// Page 2 of `b` is rendered first. A cache keyed by index would then already
/// hold a file under page 2 of `ab`'s key, and would serve it: `b.cbz`'s third
/// page, blue, in place of its first, red. The cache files are read afterwards,
/// so the test says where each rendering went as well as what the reader got.
#[tokio::test]
async fn a_resized_page_is_cached_under_its_offset_in_its_member_not_its_index() {
    let (state, _fixture) = crate::test_support::state_with_a_shared_member();
    let dir = state.thumb_cache_dir();
    let app = build_router(state);
    let colour_of = crate::test_support::colour_of;

    let (_, _, third) = fetch(&app, "/api/works/m/b/pages/2?w=8", None).await;
    assert_eq!(
        colour_of(&third),
        "blue",
        "page 2 of `b` is b.cbz's third page"
    );
    let (status, headers, first) = fetch(&app, "/api/works/m/ab/pages/2?w=8", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg", "resized");
    assert_eq!(
        colour_of(&first),
        "red",
        "page 2 of `ab` is b.cbz's first page"
    );

    for (offset, colour) in [(0, "red"), (2, "blue")] {
        let path = crate::cover::page_cache_path(&dir, "sha-b", offset, 8);
        let cached = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(colour_of(&cached), colour, "{}", path.display());
    }
}

/// O108: a filmstrip tile is cached under its member's hash and its offset
/// within that member, for the reason the resized page's test above gives.
///
/// It also asks for page 0 of `ab`, which is in `a.cbz`. Every other page it
/// asks for is in `b.cbz`, the last member of both editions, so without that
/// request a hash taken from the last member would pass every test.
#[tokio::test]
async fn a_tile_is_cached_under_its_offset_in_its_member_not_its_index() {
    let (state, _fixture) = crate::test_support::state_with_a_shared_member();
    let dir = state.thumb_cache_dir();
    let app = build_router(state);
    let colour_of = crate::test_support::colour_of;

    let (_, _, third) = fetch(&app, "/api/works/m/b/pages/2/thumb", None).await;
    assert_eq!(
        colour_of(&third),
        "blue",
        "page 2 of `b` is b.cbz's third page"
    );
    let (status, _, first) = fetch(&app, "/api/works/m/ab/pages/2/thumb", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        colour_of(&first),
        "red",
        "page 2 of `ab` is b.cbz's first page"
    );
    let (_, _, other_member) = fetch(&app, "/api/works/m/ab/pages/0/thumb", None).await;
    assert_eq!(
        colour_of(&other_member),
        "grey",
        "page 0 of `ab` is a.cbz's first page"
    );

    for (sha, offset, colour) in [
        ("sha-b", 0, "red"),
        ("sha-b", 2, "blue"),
        ("sha-a", 0, "grey"),
    ] {
        let path = crate::cover::thumb_cache_path(&dir, sha, offset, crate::cover::THUMB_LONG_EDGE);
        let cached = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(colour_of(&cached), colour, "{}", path.display());
    }
}

/// O108: a width above the ceiling is resized to the ceiling, cached under
/// the ceiling, and tagged with that cache file's name.
///
/// `a_width_above_the_ceiling_carries_the_ceilings_tag` pins the tag, which
/// `get_page` builds before it reads anything. The resize and its cache file
/// come later, and given the width as sent they would still pass every other
/// test, because a 16×16 page fits inside the ceiling and inside any width
/// above it, and is served untouched either way. Page 1 of `ab` is wider than
/// the ceiling and narrower than the width asked for here, so only the clamped
/// width resizes it.
#[tokio::test]
async fn a_width_above_the_ceiling_is_resized_and_cached_at_the_ceiling() {
    let (state, _fixture) = crate::test_support::state_with_a_shared_member();
    let dir = state.thumb_cache_dir();
    let app = build_router(state);
    let ceiling = crate::cover::MAX_FIT_LONG_EDGE;

    let uri = format!("/api/works/m/ab/pages/1?w={}", ceiling + 8);
    let (status, headers, body) = fetch(&app, &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "image/jpeg",
        "{uri} must be resized to the ceiling, not served whole"
    );
    let decoded = image::load_from_memory(&body).expect("a decodable page");
    assert_eq!(decoded.width().max(decoded.height()), ceiling, "{uri}");

    let path = crate::cover::page_cache_path(&dir, "sha-a", 1, ceiling);
    let cached = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        cached, body,
        "the page served is the one cached at the ceiling"
    );
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
    assert_eq!(headers[header::ETAG], format!("\"{stem}\"").as_str());
}

/// O108: a page that already fits inside `?w=` is served its own bytes, read by
/// its index in the edition.
///
/// The read that serves such a page takes the index where the tag and the cache
/// key take the offset. The page is served from the fetch that found it fits,
/// and this pins that fetch to the index. A width of 16 is the page's own size.
#[tokio::test]
async fn a_page_that_already_fits_the_width_is_read_by_its_index() {
    let (state, _fixture) = crate::test_support::state_with_a_shared_member();
    let app = build_router(state);

    let uri = "/api/works/m/ab/pages/2?w=16";
    let (status, headers, body) = fetch(&app, uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "image/png",
        "{uri} fits, so it is served the original bytes"
    );
    assert_eq!(
        crate::test_support::colour_of(&body),
        "red",
        "page 2 of `ab` is b.cbz's first page"
    );
}

/// O107: a page that already fits `?w=` is read out of its archive once.
///
/// Reading it twice, once to learn that it fits and again to serve it, serves
/// the same bytes both times, so no assertion on the response could see the
/// second read. This counts the reads through `CountingSource`.
///
/// The four requests are a full-size page, a page that fits, a resize and a
/// cache hit. They run here in that order and must read once, once, once and
/// not at all. Each read is `b.cbz`'s first page, because page 2 of `ab` is
/// that page.
#[tokio::test]
async fn a_page_that_already_fits_the_width_is_read_once() {
    let (state, _fixture, reads) = crate::test_support::state_with_a_shared_member_and_its_reads();
    let app = build_router(state);
    let taken = || {
        std::mem::take(
            &mut *reads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    };
    let first_page_of_b = [("b.cbz".to_string(), 0)];

    let (status, headers, original) = fetch(&app, "/api/works/m/ab/pages/2", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/png");
    assert_eq!(taken(), first_page_of_b, "a full-size page is read once");

    let (status, headers, body) = fetch(&app, "/api/works/m/ab/pages/2?w=16", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "image/png",
        "a page that fits is served with the type its archive gave it"
    );
    assert_eq!(body, original, "and the bytes");
    assert_eq!(
        taken(),
        first_page_of_b,
        "a page that already fits is read once"
    );

    let (status, headers, resized) = fetch(&app, "/api/works/m/ab/pages/2?w=8", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(taken(), first_page_of_b, "a resize reads its page once");

    let (status, _, again) = fetch(&app, "/api/works/m/ab/pages/2?w=8", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again, resized);
    assert_eq!(
        taken(),
        Vec::<(String, u32)>::new(),
        "a cache hit reads nothing"
    );
}

#[tokio::test]
async fn a_thumb_route_answers_a_jpeg_and_404s_a_page_that_does_not_exist() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/works/w/a/pages/0/thumb")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/jpeg");

    let missing = app
        .oneshot(
            Request::builder()
                .uri("/api/works/w/a/pages/9999/thumb")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        crate::test_support::body_string(missing).await,
        "`a` has 1 pages; there is no page 9999"
    );
}

/// O1: every route whose bytes are fixed by a member's content says how
/// long they may be reused without asking.
///
/// An `ETag` alone never removes the round trip, which is the part a reader
/// waits on. All four byte routes are checked rather than only the `/api/…`
/// one, because Panels reaches pages through the OPDS stream, and the cover is
/// the only route a cold grid view touches at all.
///
/// The `304` is checked for the same header, and it is the case most
/// easily left out: RFC 9111 §4.3.4 has a client refresh its stored
/// response from the `304`'s headers, so omitting it here would leave the
/// entry exactly as stale as it was and cost a round trip on the very
/// next read.
///
/// **Every address here is one without a `?v=`**, and the claim is about those.
/// A cover address whose `v` names its bytes does name them, and is the one
/// answer kept for good:
/// `a_cover_whose_version_matches_is_kept_for_good_on_the_200_and_the_304`.
#[tokio::test]
async fn every_route_serving_a_member_s_bytes_makes_the_client_revalidate() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    for uri in [
        "/api/works/w/a/pages/0",
        "/api/works/w/a/pages/0/thumb",
        "/opds/v1.2/works/w/a/pages/0",
        "/opds/v1.2/works/w/a/cover",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        let policy = response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert_eq!(
            policy, PAGE_CACHE_CONTROL,
            "{uri} serves a member's bytes and must state a caching policy"
        );
        // The property, not just the constant. Asserting only that the
        // header equals `PAGE_CACHE_CONTROL` passes for *any* value that
        // constant takes, so it could not catch a long `max-age`: a work id is
        // the sidecar's filename stem, which every sidecar in the corpus
        // shares, so one URL can name two volumes.
        assert!(
            policy.contains("no-cache"),
            "{uri} must be revalidated rather than assumed fresh: a work id \
                 is not unique, so this URL does not name these bytes — it was \
                 measured serving two different volumes' page 10 under two \
                 different ETags. Got: {policy}"
        );
    }

    // Both routes that carry a validator, revalidated — not just the `/api/…`
    // one. Panels reaches pages through the OPDS stream, so a `304` asserted on
    // `/api/…` alone would miss the route Panels uses.
    for uri in ["/api/works/w/a/pages/0", "/opds/v1.2/works/w/a/pages/0"] {
        let first = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let tag = first.headers()[header::ETAG].clone();

        let revalidated = app
            .clone()
            .oneshot(
                Request::get(uri)
                    .header(header::IF_NONE_MATCH, &tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED, "{uri}");
        assert_eq!(
            revalidated.headers().get(header::CACHE_CONTROL),
            Some(&header::HeaderValue::from_static(PAGE_CACHE_CONTROL)),
            "{uri}: a 304 that omits it buys one response and no more — \
                 the client stores the same stale entry and asks again next \
                 read"
        );
    }
}

/// The other half, and the half that has something to fail against: no
/// document a client can change is given a freshness lifetime.
///
/// A page is fixed by a member's `sha256`. A sidecar is rewritten by
/// `PATCH /alignments`, notes by `PUT /notes`, progress by
/// `PUT /progress`, and every feed restates whatever those hold — so a
/// client told it may reuse any of them for a day would show a correction
/// it had just made being undone, or lose a note to a second device.
///
/// This is what fails the day someone reaches for a blanket
/// `SetResponseHeaderLayer` on the router rather than the four routes
/// where the claim is true.
///
/// **Both statuses are checked, and the second one is why this test exists in
/// the shape it does.** `not_modified` is shared by `get_page`, `get_sidecar`,
/// `get_notes` and the OPDS stream, and a client caches what the `304` says
/// (RFC 9111 §4.3.4), so a lifetime on a shared `304` would be worth exactly as
/// much as one on the `200`, and a test that looked only at the `200`s would
/// not see it.
#[tokio::test]
async fn no_document_a_client_can_change_is_given_a_freshness_lifetime() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    for uri in [
        "/api/works",
        "/api/works/w",
        "/api/works/w/progress",
        "/api/works/w/notes",
        "/opds/v1.2/catalog",
        "/opds/v1.2/works",
        "/opds/v1.2/works/w",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        // **No freshness lifetime, which is not the same as no header.** These
        // documents are per-reader: a proxy in front — which spec A2 §11 puts
        // there — would store one reader's notes and replay them to another,
        // and RFC 9111's bar on caching authenticated responses keys on
        // `Authorization`, which the browser path never sends. `no-store` grants
        // no lifetime *and* forbids the store, so it satisfies this test's own
        // stated reason more strongly than absence would.
        let directive = response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok());
        assert!(
            // `is_some_and`, not `is_none_or`, so that a route which sets no
            // directive at all fails.
            directive.is_some_and(|d| d.contains("no-store") || d.contains("no-cache")),
            "{uri} answers a document this server rewrites; a freshness \
                 lifetime on it hands a reader back an edit they have already \
                 made — got {directive:?}"
        );
        assert!(
            directive.is_none_or(|d| !d.contains("max-age")),
            "{uri} was given a max-age: {directive:?}"
        );
    }

    // The two that carry a validator, revalidated. A `304` is where a
    // client refreshes what it believes about an entry, so a mutable
    // document's `304` has to be as silent about freshness as its `200`.
    for uri in ["/api/works/w", "/api/works/w/notes"] {
        let first = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let tag = first.headers()[header::ETAG].clone();

        let revalidated = app
            .clone()
            .oneshot(
                Request::get(uri)
                    .header(header::IF_NONE_MATCH, &tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED, "{uri}");
        // The 304 too: a client stores what a `304` tells it (RFC 9111
        // §4.3.4), so a lifetime here is worth exactly what one on the 200
        // would be — and a *shared* cache storing a per-reader 304 is the same
        // replay the 200 would be. The layer covers both because it wraps the
        // response rather than the handler.
        let revalidated_directive = revalidated
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok());
        assert!(
            revalidated_directive.is_some_and(|d| d.contains("no-store") || d.contains("no-cache")),
            "{uri} shares `not_modified` with the page routes, and a \
                 client stores what a 304 tells it — a freshness lifetime \
                 here is worth exactly what one on the 200 would be; got \
                 {revalidated_directive:?}"
        );
    }
}

/// O277: with no `--web`, every kind of answer carries the four security
/// headers — a refusal by the guard, a guarded route reached by a session and
/// by a key, an open route's refusal, and axum's own 404. A browser that opens
/// an API or OPDS address directly is held to the same policy as the app.
///
/// Each status is asserted too, so a case that stopped reaching its route is
/// seen rather than passing on whatever answered instead.
#[tokio::test]
async fn api_and_opds_answers_carry_the_security_headers() {
    let tmp = crate::test_support::Fixture::new("routes-security-headers");
    let state = Arc::new(crate::test_support::state(&tmp));
    let library = Arc::clone(&state.library);
    let anonymous = super::build_router(state.clone());
    let by_session =
        crate::test_support::authenticating(super::build_router(state.clone()), &library);
    let by_key = crate::test_support::key_authenticating(super::build_router(state));

    let wrong_password = Request::post("/api/session")
        .header("content-type", "application/json")
        // Without a matching `host` and `origin`, `refuse_cross_site`
        // answers 403 before the password is read.
        .header("host", "test")
        .header("origin", "http://test")
        .body(Body::from(format!(
            r#"{{"name":"{}","password":"not the password"}}"#,
            crate::test_support::FIXTURE_NAME
        )))
        .unwrap();
    for (what, app, request, expected) in [
        (
            "an anonymous GET /api/me",
            &anonymous,
            Request::get("/api/me").body(Body::empty()).unwrap(),
            StatusCode::UNAUTHORIZED,
        ),
        (
            "GET /api/me by a session",
            &by_session,
            Request::get("/api/me").body(Body::empty()).unwrap(),
            StatusCode::OK,
        ),
        (
            "the OPDS catalog by a key",
            &by_key,
            Request::get("/opds/v1.2/catalog")
                .body(Body::empty())
                .unwrap(),
            StatusCode::OK,
        ),
        (
            "a login with a wrong password",
            &anonymous,
            wrong_password,
            StatusCode::UNAUTHORIZED,
        ),
        (
            "a path no route matched",
            &anonymous,
            Request::get("/nothing-here").body(Body::empty()).unwrap(),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected, "{what}");
        crate::test_support::assert_the_security_headers(response.headers(), what);
    }
}

/// A real request, through the real router, has to produce a real log
/// line — and that line's `bytes` has to be non-zero.
///
/// This is the case the `CONTENT_LENGTH` mistake would pass silently: a
/// unit test on `Log` alone never sends a response through axum, so it
/// cannot see that the header is not yet set when the middleware reads
/// it. Only driving the middleware through a page fetch — a body with
/// real bytes in it — proves the log records what a client actually
/// received rather than 0 for every request.
#[tokio::test]
async fn a_page_fetch_through_the_router_logs_a_line_with_real_byte_count() {
    let tmp = crate::test_support::Fixture::new("routes-reqlog");
    let mut state = crate::test_support::state(&tmp);
    let tmp2 = crate::test_support::Fixture::new("routes-reqlog");
    let (log, log_path) = crate::test_support::log(&tmp2);
    state.log = Some(log);
    let app = build_router(Arc::new(state));

    let page = app
        .oneshot(
            Request::get("/api/works/t/raw/pages/2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(page.into_body(), 1 << 20)
        .await
        .unwrap();

    let entries = crate::test_support::logged_lines(&log_path);
    let _ = std::fs::remove_file(&log_path);
    assert_eq!(entries.len(), 1, "exactly one request went through");
    let entry = &entries[0];
    assert_eq!(entry["method"], "GET");
    assert_eq!(entry["path"], "/api/works/t/raw/pages/2");
    assert_eq!(entry["status"], 200);
    assert_eq!(
        entry["bytes"],
        bytes.len() as u64,
        "the logged size must match the bytes the client actually received"
    );
    assert!(
        entry["bytes"].as_u64().unwrap() > 0,
        "a page is never a zero-byte response"
    );
    assert_eq!(entry["in_flight"], 1, "a lone request arrived alone");
}

/// O1's question — does a client ever send `If-None-Match`? — cannot be
/// answered from a log that records no request headers. An `ETag` is a line of
/// code; knowing whether it would buy anything is the part that needs evidence,
/// and the evidence has to come from a real session with a real client.
///
/// The set logged is an allowlist, so this pins both halves: a header on
/// it survives into the line, and one that is not on it does not. A log
/// that swept up everything a client sends would grow unbounded per line
/// and would put a `Cookie` or an `Authorization` into a file that gets
/// pasted into issues.
#[tokio::test]
async fn a_conditional_request_header_is_logged_and_an_unlisted_one_is_not() {
    let tmp = crate::test_support::Fixture::new("routes-reqlog-headers");
    let mut state = crate::test_support::state(&tmp);
    let tmp2 = crate::test_support::Fixture::new("routes-reqlog-headers");
    let (log, log_path) = crate::test_support::log(&tmp2);
    state.log = Some(log);
    let app = build_router(Arc::new(state));

    let page = app
        .oneshot(
            Request::get("/api/works/t/raw/pages/2")
                .header("if-none-match", "\"abc123\"")
                .header("user-agent", "Panels/1.0")
                .header("cookie", "session=secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);

    let entries = crate::test_support::logged_lines(&log_path);
    let _ = std::fs::remove_file(&log_path);
    assert_eq!(entries.len(), 1);
    let headers = &entries[0]["headers"];
    assert_eq!(
        headers["if-none-match"], "\"abc123\"",
        "the header O1 turns on has to survive into the line"
    );
    assert_eq!(headers["user-agent"], "Panels/1.0", "which client asked");
    assert!(
        headers.get("cookie").is_none(),
        "an unlisted header must not be recorded: {headers}"
    );
}

/// A client asking for `?width=1536` has to be visible in the log: logging
/// `path()` alone would make it look like no client ever asks for one.
#[tokio::test]
async fn a_request_with_a_query_string_is_logged_with_it() {
    let tmp = crate::test_support::Fixture::new("routes-reqlog-query");
    let mut state = crate::test_support::state(&tmp);
    let tmp2 = crate::test_support::Fixture::new("routes-reqlog-query");
    let (log, log_path) = crate::test_support::log(&tmp2);
    state.log = Some(log);
    let app = build_router(Arc::new(state));

    let page = app
        .oneshot(
            Request::get("/api/works/t/raw/pages/2?width=1536")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);

    let entries = crate::test_support::logged_lines(&log_path);
    let _ = std::fs::remove_file(&log_path);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]["path"], "/api/works/t/raw/pages/2?width=1536",
        "the query string must survive into the logged path"
    );
}

/// `if_match_hits`'s reason to exist: absent, matching and stale are
/// three different answers, not two. `None` becomes a 428 and `Some(false)` a
/// 412 — collapsing the first into the second would tell a client that sent no
/// validator it lost a race it never entered.
#[test]
fn if_match_distinguishes_absent_from_matching_from_stale() {
    let none_sent = HeaderMap::new();
    assert_eq!(
        if_match_hits(&none_sent, "\"abc\""),
        None,
        "no header at all is not the same question as a stale one"
    );

    let mut matching = HeaderMap::new();
    matching.insert(header::IF_MATCH, "\"abc\"".parse().unwrap());
    assert_eq!(if_match_hits(&matching, "\"abc\""), Some(true));

    let mut stale = HeaderMap::new();
    stale.insert(header::IF_MATCH, "\"old\"".parse().unwrap());
    assert_eq!(if_match_hits(&stale, "\"abc\""), Some(false));
}

/// The validator the correction path writes against. It must change when
/// the document does, or `If-Match` guards nothing.
// `update`'s own `#[allow(clippy::result_large_err)]` does not reach a
// closure written at the call site — clippy checks each closure as its
// own item — so the explicit closure here needs the same allow, for the
// same reason `works.rs`'s and this file's other `update` calls do.
#[allow(clippy::result_large_err)]
#[tokio::test]
async fn the_sidecar_carries_an_etag_that_tracks_its_content() {
    let tmp = crate::test_support::Fixture::new("routes-sidecar-etag");
    let state = Arc::new(crate::test_support::state(&tmp));
    let router = build_router(state.clone());

    let first = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/works/t")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let tag = first.headers().get(axum::http::header::ETAG).cloned();
    state.works.snapshot()[0]
        .update(|current| {
            let mut next = (**current).clone();
            next.work.title = "Different".into();
            Ok((std::sync::Arc::new(next), ()))
        })
        .expect("updates");
    let second = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/works/t")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let tag = tag.expect("a sidecar response carries an ETag");
    assert_ne!(
        tag,
        *second.headers().get(axum::http::header::ETAG).unwrap(),
        "a changed document must not validate against the old tag"
    );
}

async fn etag_of(router: &axum::Router, uri: &str) -> String {
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    response
        .headers()
        .get(axum::http::header::ETAG)
        .expect("an ETag")
        .to_str()
        .unwrap()
        .to_string()
}

async fn correct(router: &axum::Router, tag: &str, body: &str) -> axum::http::StatusCode {
    router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri("/api/works/t/alignments")
                .header("if-match", tag)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// The same request, keeping the body as well as the status.
///
/// A refusal that names what was missing is a constraint this project
/// holds every route to, so the wording is worth asserting rather than
/// only the number — `correct` above stays for the cases where the status
/// is the whole claim.
async fn correct_with_body(
    router: &axum::Router,
    work: &str,
    tag: &str,
    body: &str,
) -> (axum::http::StatusCode, String) {
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri(format!("/api/works/{work}/alignments"))
                .header("if-match", tag)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Correct a pairing, and have the document say so afterwards.
#[tokio::test]
async fn a_correction_is_written_and_read_back() {
    let tmp = crate::test_support::Fixture::new("routes-correct");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state.clone());
    let tag = etag_of(&router, "/api/works/t").await;

    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri("/api/works/t/alignments")
                .header("if-match", &tag)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let fresh = response.headers().get(axum::http::header::ETAG).cloned();
    // The body is the point of answering 200 rather than 204: the client
    // refreshes its resolver from this rather than making a second request
    // that could already be out of date again. Without this assertion,
    // replacing the whole response with a bare `StatusCode::OK` would
    // delete the round-trip-refresh feature and leave every other check in
    // this test green.
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let on_disk = std::fs::read_to_string(&state.works.snapshot()[0].path).unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_ne!(
        fresh,
        Some(tag.parse().unwrap()),
        "the ETag moves with the write"
    );
    let returned = taiyaku_core::Sidecar::from_json(&String::from_utf8_lossy(&body))
        .expect("the response body is the updated document");
    assert_eq!(
        returned
            .alignments
            .iter()
            .flat_map(|a| &a.runs)
            .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
            .count(),
        2,
        "the document handed back already carries the correction"
    );
    assert!(
        on_disk.contains("\"hand\""),
        "the file records who decided: {on_disk}"
    );
    assert_eq!(
        state.works.snapshot()[0].sidecar().alignments[0]
            .runs
            .iter()
            .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
            .count(),
        2,
        "the slot asserted and the slot displaced, in memory as well as on disk"
    );
}

/// How many runs in `work_id`'s **rows** were decided by hand.
///
/// Read through `taiyaku_library::project::sidecar` rather than off the `run`
/// table directly: the projection is what `taiyaku serve` starts from and what
/// `taiyaku library export` writes out, so a count taken here is a count of
/// what a reader would actually get back.
fn hand_runs_in_rows(library: &taiyaku_library::Library, work_id: i64) -> usize {
    let conn = library.conn();
    taiyaku_library::project::sidecar(&conn, work_id)
        .expect("projecting the corrected work")
        .alignments
        .iter()
        .flat_map(|a| &a.runs)
        .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
        .count()
}

/// **O43: a hand correction reaches the library, not only the file.**
///
/// `serve` answers every request from the rows, so a correction the database
/// has never heard of is a correction the next reader does not see — the file
/// and the database disagree until something restarts and re-imports. The
/// assertion is on the projection, immediately after the request, with nothing
/// restarted in between.
#[tokio::test]
async fn a_correction_reaches_the_library_rows_without_a_restart() {
    let (router, state, work_id, _fixture) =
        crate::test_support::router_over_a_library_with_two_editions();
    assert_eq!(
        hand_runs_in_rows(&state.library, work_id),
        0,
        "the imported document is all solver runs, so the count below is the correction's"
    );

    let tag = etag_of(&router, "/api/works/w").await;
    let (status, body) = correct_with_body(
        &router,
        "w",
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");

    assert_eq!(
        hand_runs_in_rows(&state.library, work_id),
        2,
        "the slot asserted and the slot displaced are in the rows the next request reads from"
    );
}

/// The other half of the round trip: what the rows hold now must also be what
/// a restart leaves behind.
///
/// A restart is `taiyaku-cli`'s `serve_work` — `import::reimport` over the
/// sidecar file — so this drives exactly that, from the bytes the handler
/// wrote, and asserts the hand runs are still there afterwards. Both sides
/// carry the correction by then, which is the point: the recovery path and the
/// live write agree rather than one overwriting the other.
#[tokio::test]
async fn a_correction_survives_the_reimport_a_restart_performs() {
    let (router, state, work_id, _fixture) =
        crate::test_support::router_over_a_library_with_two_editions();
    let tag = etag_of(&router, "/api/works/w").await;
    let (status, body) = correct_with_body(
        &router,
        "w",
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");

    let path = state.works.snapshot()[0].path.clone();
    let text = std::fs::read_to_string(&path).expect("the handler wrote the file");
    let doc = taiyaku_core::Sidecar::from_json(&text).expect("the written file parses");
    {
        let conn = state.library.conn();
        taiyaku_library::import::reimport(&conn, &doc, &path).expect("reimport");
    }

    assert_eq!(
        hand_runs_in_rows(&state.library, work_id),
        2,
        "a restart re-importing the file keeps the correction rather than replacing it"
    );
}

/// [`router_over_a_library_with_two_editions`]'s work with its `taiyaku.json`
/// rewritten as a newer taiyaku would leave it: `taiyaku` at `declared` and a
/// field this build has no name for. Answers the correction's status and body,
/// the file's text afterwards, and the work's projected rows before and after.
///
/// [`router_over_a_library_with_two_editions`]: crate::test_support::router_over_a_library_with_two_editions
async fn correct_over_a_newer_sidecar(
    declared: u64,
) -> (axum::http::StatusCode, String, String, String, String) {
    let (router, state, work_id, _fixture) =
        crate::test_support::router_over_a_library_with_two_editions();
    let path = state.works.snapshot()[0].path.clone();
    let mut newer: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the fixture's sidecar"))
            .expect("the fixture's sidecar is JSON");
    newer["taiyaku"] = serde_json::json!(declared);
    newer["from_the_future"] = serde_json::json!(true);
    std::fs::write(&path, newer.to_string()).expect("a newer taiyaku's sidecar");
    let rows = || {
        let conn = state.library.conn();
        taiyaku_library::project::sidecar(&conn, work_id)
            .expect("projecting the work")
            .to_json()
    };
    let rows_before = rows();

    let tag = etag_of(&router, "/api/works/w").await;
    let (status, body) = correct_with_body(
        &router,
        "w",
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
    )
    .await;

    let file = std::fs::read_to_string(&path).expect("the sidecar is still there");
    (status, body, file, rows_before, rows())
}

/// **O264: the correction route is a writer of `taiyaku.json` too.** Writing
/// a newer taiyaku's sidecar back from this build's projection would drop
/// every field it has no name for and stamp it version 1, so the route refuses
/// in the stamp's words before the file write, and so before the rows. Any
/// account granted the work can call it, so the refusal names no server path.
#[tokio::test]
async fn a_correction_over_a_sidecar_a_newer_taiyaku_wrote_is_refused_and_changes_nothing() {
    let (status, body, file, rows_before, rows_after) = correct_over_a_newer_sidecar(2).await;

    assert_eq!(status, axum::http::StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body,
        "this document declares taiyaku format version 2, and this build reads 1. \
         Refusing to read it rather than dropping the fields it does not understand and \
         writing them back out as version 1 — upgrade taiyaku instead",
        "refuse_newer's sentence alone, with no server path in it"
    );
    let file: serde_json::Value = serde_json::from_str(&file).expect("the file is still JSON");
    assert_eq!(
        (&file["taiyaku"], &file["from_the_future"]),
        (&serde_json::json!(2), &serde_json::json!(true)),
        "the file is as the newer taiyaku left it, version and unknown field both"
    );
    assert_eq!(rows_after, rows_before, "and the rows are as they were");
}

/// A version past `u32` is newer than any build, not version 0.
#[tokio::test]
async fn a_correction_over_a_sidecar_declaring_a_version_past_u32_is_refused_too() {
    let (status, body, file, _, _) = correct_over_a_newer_sidecar(4_294_967_296).await;

    assert_eq!(status, axum::http::StatusCode::CONFLICT, "{body}");
    assert!(
        file.contains("4294967296"),
        "the file still declares the version it was written with: {file}"
    );
}

/// `Corrected::AlreadyTrue` — asserting a pairing the document already
/// holds — must not touch the file at all: `patch_alignments` only calls
/// `atomic::write` when `what != AlreadyTrue`. A write neither read nor
/// followed by a change is exactly the crash window `atomic::write`
/// exists to close, so skipping it here is not an optimisation, it is one
/// fewer moment a concurrent reader could see a temp-file rename that
/// says nothing new.
///
/// Proven by inode identity rather than by content or mtime, for the
/// reason `atomic.rs`'s own
/// `the_destination_is_replaced_by_rename_not_truncation` gives: an
/// `AlreadyTrue` correction writes back byte-identical JSON, so content
/// alone cannot tell "skipped" from "wrote the same bytes again", and mtime
/// cannot either, because two `atomic::write` calls microseconds apart can
/// get the identical mtime. A rename always replaces the destination with a
/// fresh inode, whether or not the bytes changed, so the inode is the one
/// signal that distinguishes "no write happened" from "a write happened that
/// changed nothing".
#[tokio::test]
async fn a_no_op_correction_does_not_touch_the_file() {
    use std::os::unix::fs::MetadataExt;

    let tmp = crate::test_support::Fixture::new("routes-correct-noop");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state.clone());
    let first_tag = etag_of(&router, "/api/works/t").await;
    let first_status = correct(
        &router,
        &first_tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
    )
    .await;
    assert_eq!(
        first_status,
        axum::http::StatusCode::OK,
        "the setup write lands"
    );

    let before_bytes = std::fs::read(&state.works.snapshot()[0].path).unwrap();
    let before_ino = std::fs::metadata(&state.works.snapshot()[0].path)
        .unwrap()
        .ino();
    let second_tag = etag_of(&router, "/api/works/t").await;

    // The same assertion again: the document already says slot 2 pairs
    // with slot 1, so this is `Corrected::AlreadyTrue`.
    let second_status = correct(
        &router,
        &second_tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
    )
    .await;

    let after_bytes = std::fs::read(&state.works.snapshot()[0].path).unwrap();
    let after_ino = std::fs::metadata(&state.works.snapshot()[0].path)
        .unwrap()
        .ino();
    assert_eq!(
        second_status,
        axum::http::StatusCode::OK,
        "a no-op is still answered 200"
    );
    assert_eq!(
        before_bytes, after_bytes,
        "a no-op correction must leave the file's bytes exactly as they were"
    );
    assert_eq!(
        before_ino, after_ino,
        "identical bytes are not proof nothing was written — a rename always gives the \
             destination a fresh inode, whether or not the bytes it carries changed, so the \
             inode is what actually distinguishes a skipped write from a redundant one"
    );
}

/// The lost-update hole the design names. Two devices read one document,
/// both correct it; the second must be told rather than silently winning.
#[tokio::test]
async fn a_correction_against_a_stale_validator_is_refused() {
    let tmp = crate::test_support::Fixture::new("routes-correct-stale");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state.clone());
    let stale_etag = etag_of(&router, "/api/works/t").await;
    correct(
        &router,
        &stale_etag,
        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
    )
    .await;

    let (second, message) = correct_with_body(
        &router,
        "t",
        &stale_etag,
        r#"{"from":"raw","to":"en","a":"3","b":"0"}"#,
    )
    .await;

    let after = state.works.snapshot()[0].sidecar();
    let current = sidecar_etag(&after);
    assert_eq!(second, axum::http::StatusCode::PRECONDITION_FAILED);
    // Telling a client only that it lost is not enough to let it recover:
    // the message carries the validator it should have sent, so the next
    // attempt can be made without a second round trip to find out.
    assert!(
        message.contains(&current),
        "the refusal must carry the ETag the client now needs, got: {message}"
    );
    // The first correction only reassigns slots 1 and 2 (the one asserted
    // and the one it displaced); slot 3 is untouched by it. If the second,
    // stale-validator correction had wrongly landed, slot 3 would resolve
    // to `en`'s slot 0 instead — so slot 3 still holding the identity
    // pairing from before either correction is exactly what proves the
    // refused request never applied.
    assert_eq!(
        taiyaku_core::resolve(&after, "raw", "en", 3),
        Some(taiyaku_core::Slot {
            page: 3,
            half: None
        }),
        "the refused correction must not have landed"
    );
}

/// A client that sent no validator has not lost a race, it has not entered
/// one — and telling it 412 would send it looking for a conflict that does
/// not exist.
#[tokio::test]
async fn a_correction_with_no_validator_at_all_says_which_header_is_missing() {
    let tmp = crate::test_support::Fixture::new("routes-correct-bare");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state);

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri("/api/works/t/alignments")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(status, axum::http::StatusCode::PRECONDITION_REQUIRED);
    assert!(
        String::from_utf8_lossy(&body).contains("If-Match"),
        "must name the header a client has to send"
    );
}

/// Each row of this route's status table, pinned.
///
/// The mapping from `NotCorrected` to a status is hand-written, and an
/// exhaustive `match` only protects against a *new variant* being dropped
/// — it says nothing about an existing one being given the wrong status.
/// Changing `SameEdition => BAD_REQUEST` to `NOT_FOUND` compiles, and
/// without this test the suite stays green. `Sidecar::correct`'s own tests pin
/// which variant it returns; which status it becomes is this route's to
/// answer, and so this route's to test.
///
/// `Unreadable` (409) is deliberately absent: producing it needs a stored
/// edge whose spans do not resolve, which this fixture cannot express
/// without hand-writing a broken document, and contorting the fixture to
/// reach one status would cost more than it pins.
#[tokio::test]
async fn each_refusal_answers_the_status_its_kind_calls_for() {
    let tmp = crate::test_support::Fixture::new("routes-correct-statuses");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state.clone());
    let tag = etag_of(&router, "/api/works/t").await;

    let cases = [
        // An edition cannot be its own counterpart — the request is
        // coherent JSON naming an incoherent operation.
        (
            r#"{"from":"raw","to":"raw","a":"2","b":"1"}"#,
            axum::http::StatusCode::BAD_REQUEST,
            "own counterpart",
        ),
        // Names an edition this document does not define.
        (
            r#"{"from":"raw","to":"nosuch","a":"2","b":"1"}"#,
            axum::http::StatusCode::NOT_FOUND,
            "nosuch",
        ),
        // Names a slot past the end of a four-page edition.
        (
            r#"{"from":"raw","to":"en","a":"99","b":"1"}"#,
            axum::http::StatusCode::NOT_FOUND,
            "99",
        ),
        // Not a correction at all.
        (
            r#"{"nonsense":true}"#,
            axum::http::StatusCode::BAD_REQUEST,
            "not a correction",
        ),
        // The field name is misspelled. Without `deny_unknown_fields` this
        // would deserialise to `b: None`, unpair slot 2, and answer 200 —
        // a destructive edit the client never asked for and is not told
        // about. See `Correction`'s own doc comment.
        (
            r#"{"from":"raw","to":"en","a":"2","bb":"1"}"#,
            axum::http::StatusCode::BAD_REQUEST,
            "not a correction",
        ),
        // `b` omitted entirely. The other half of the same rule: absence must
        // not spell "unpair", so this is refused rather than quietly taking a
        // pairing away. The refusal is this server's own and names the field;
        // `patch_alignments` says why serde's "missing field `b`" would not do.
        (
            r#"{"from":"raw","to":"en","a":"2"}"#,
            axum::http::StatusCode::BAD_REQUEST,
            "needs `b`",
        ),
    ];

    let mut answered = Vec::new();
    for (body, _, _) in &cases {
        answered.push(correct_with_body(&router, "t", &tag, body).await);
    }
    // A work this server does not hold, refused with a 404 that names it.
    let missing = correct_with_body(
        &router,
        "nosuchwork",
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
    )
    .await;
    let unchanged = state.works.snapshot()[0].sidecar().alignments[0].runs.len();

    for ((body, want, names), (got, message)) in cases.iter().zip(&answered) {
        assert_eq!(got, want, "for {body}: {message}");
        assert!(
            message.to_lowercase().contains(&names.to_lowercase()),
            "the refusal for {body} must name what was wrong, got: {message}"
        );
    }
    assert_eq!(missing.0, axum::http::StatusCode::NOT_FOUND);
    assert!(
        missing.1.contains("nosuchwork"),
        "must name the work: {}",
        missing.1
    );
    assert_eq!(
        unchanged, 1,
        "not one of these refusals may have changed the document"
    );
}

/// The unpair path, end to end.
///
/// `"b": null` is the reason `b` is nullable rather than simply a `String`.
/// Nothing in `web/` can send it — the reader has no "this page pairs with
/// nothing" gesture — so without this test the whole justification for keeping
/// the field nullable would rest on a path nothing exercises.
#[tokio::test]
async fn an_explicit_null_unpairs_the_slot() {
    let tmp = crate::test_support::Fixture::new("routes-correct-unpair");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state.clone());
    let tag = etag_of(&router, "/api/works/t").await;
    let before = taiyaku_core::resolve(&state.works.snapshot()[0].sidecar(), "raw", "en", 2);

    let (status, _) = correct_with_body(
        &router,
        "t",
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":null}"#,
    )
    .await;

    let after = state.works.snapshot()[0].sidecar();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(
        before.is_some(),
        "slot 2 must start out paired, or this asserts nothing"
    );
    assert_eq!(
        taiyaku_core::resolve(&after, "raw", "en", 2),
        None,
        "the slot now pairs with nothing"
    );
    assert_eq!(
        after.alignments[0]
            .runs
            .iter()
            .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
            .count(),
        1,
        "an unpairing displaces nobody, so it is the only hand run"
    );
}

/// A span is the format's own notation and `45R-46L` is a legal one — but
/// a correction addresses one slot, and taking the start of a range would
/// silently correct something other than what was asked.
#[tokio::test]
async fn a_correction_naming_a_range_rather_than_a_slot_is_refused() {
    let tmp = crate::test_support::Fixture::new("routes-correct-range");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state.clone());
    let tag = etag_of(&router, "/api/works/t").await;

    let (status, message) = correct_with_body(
        &router,
        "t",
        &tag,
        r#"{"from":"raw","to":"en","a":"2-3","b":"1"}"#,
    )
    .await;

    assert!(
        message.contains("2-3") && message.contains("range"),
        "the refusal must name the span it would not take: {message}"
    );
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
}

/// A work nobody has annotated still answers with a document, not a 204.
///
/// A `PUT` requires `If-Match`, so a client that has never seen a document
/// would have no validator to send and no way to write its first note
/// conditionally. Synthesising an empty one — with an `ETag` over exactly
/// the bytes sent — makes the first write an ordinary conditional write
/// like every later one.
#[tokio::test]
async fn a_work_with_no_notes_answers_an_empty_document_with_a_validator() {
    let tmp = crate::test_support::Fixture::new("notes-get");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/t/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let tag = response
        .headers()
        .get(header::ETAG)
        .expect("a document a client may write against carries a validator")
        .to_str()
        .unwrap()
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    assert_eq!(tag, notes_etag(&body), "the tag must be over what was sent");
    let parsed: crate::notes::Notes = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed.work, "t");
    assert!(parsed.notes.is_empty());
}

/// `serve/truth.rs:3` states the rule: one serialisation shared by the
/// projection, the response body and the `ETag` over that body, so a validator
/// can never be computed over bytes a client was not sent. Driven over
/// `router_over_a_library` rather than `test_support::state`, because that is
/// the path of a work with a real row in `state.library`, reached through
/// `works::find_row` rather than through `works::row_id_for_write` minting one.
#[tokio::test]
async fn a_notes_etag_is_computed_over_the_bytes_that_were_sent() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_library().await;
    let response = crate::test_support::get(&router, "/api/works/w/notes").await;
    let etag = response.headers().get("etag").expect("an etag").clone();
    let body = crate::test_support::body_string(response).await;

    let recomputed = crate::notes::etag_for(&body);
    assert_eq!(etag, recomputed);
}

/// The identical invariant on truth's side of `notes_etag`'s shared
/// implementation — `truth.rs`'s own doc comment claims `notes.rs`'s rule
/// rather than a second one, so this is the same test, over `/truth`.
#[tokio::test]
async fn a_truth_etag_is_computed_over_the_bytes_that_were_sent() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_library().await;
    let response = crate::test_support::get(&router, "/api/works/w/truth").await;
    let etag = response.headers().get("etag").expect("an etag").clone();
    let body = crate::test_support::body_string(response).await;

    let recomputed = crate::notes::etag_for(&body);
    assert_eq!(etag, recomputed);
}

/// The same validator answers `304` on the next load, which is what keeps
/// a per-page-load read of a document that rarely changes off the wire.
#[tokio::test]
async fn a_matching_if_none_match_answers_304_for_notes() {
    let tmp = crate::test_support::Fixture::new("notes-304");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);
    let tag = etag_of(&app, "/api/works/t/notes").await;

    let again = app
        .oneshot(
            Request::get("/api/works/t/notes")
                .header(header::IF_NONE_MATCH, &tag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
}

/// A work this server does not hold says so, in the same words every other
/// route addressed by work uses.
#[tokio::test]
async fn notes_for_an_unknown_work_are_a_404_naming_it() {
    let tmp = crate::test_support::Fixture::new("notes-no-work");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/nope/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("nope"), "the refusal must name it: {body}");
}

/// The notes a work already holds are what it serves, and the validator
/// moves with them — a tag that did not change when the document did would
/// let a stale client write over a note it never saw.
#[tokio::test]
async fn stored_notes_are_served_and_change_the_validator() {
    let tmp = crate::test_support::Fixture::new("notes-stored");
    let state = crate::test_support::state(&tmp);
    crate::notes::store(
        &state.library,
        &state.works.snapshot()[0],
        1,
        &crate::notes::Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: "t".into(),
            notes: vec![crate::notes::Note {
                id: "01J8".into(),
                edition: "raw".into(),
                slot: 2,
                region: None,
                color: None,
                body: "kept".into(),
                created: "2026-08-05T10:00:00Z".into(),
                updated: "2026-08-05T10:00:00Z".into(),
                stale: false,
                at: None,
            }],
        },
        &state.works.snapshot()[0].sidecar(),
        crate::works::Mint::Bridge,
    )
    .unwrap();
    let empty_tag = notes_etag(&crate::notes::to_json(&crate::notes::empty("t")).unwrap());
    let app = build_router(Arc::new(state));

    let response = app
        .oneshot(
            Request::get("/api/works/t/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let tag = response
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(parsed.notes.len(), 1);
    assert_eq!(parsed.notes[0].body, "kept");
    assert_ne!(
        tag, empty_tag,
        "a document with a note in it must not validate as an empty one"
    );
}

/// One note, as a client sends it.
fn one_note(id: &str, body: &str) -> String {
    format!(
        r#"{{"work":"t","notes":[{{"id":"{id}","edition":"raw","slot":1,"body":"{body}",
               "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}]}}"#
    )
}

async fn write_notes(
    router: &axum::Router,
    tag: Option<&str>,
    body: String,
) -> (StatusCode, String) {
    let mut request =
        Request::put("/api/works/t/notes").header(header::CONTENT_TYPE, "application/json");
    if let Some(tag) = tag {
        request = request.header(header::IF_MATCH, tag);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// The three answers `If-Match` has, on the one route where getting them
/// wrong loses prose a person typed: no header at all is 428 (you have not
/// entered a race), a stale one is 412 (you lost it), a current one writes.
#[tokio::test]
async fn writing_notes_needs_a_current_if_match() {
    let tmp = crate::test_support::Fixture::new("notes-put");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let (unconditional, _) = write_notes(&app, None, one_note("01J8", "first")).await;
    assert_eq!(unconditional, StatusCode::PRECONDITION_REQUIRED);

    let (stale_status, _) =
        write_notes(&app, Some("\"nonsense\""), one_note("01J8", "first")).await;
    assert_eq!(stale_status, StatusCode::PRECONDITION_FAILED);

    let tag = etag_of(&app, "/api/works/t/notes").await;
    let (written, _) = write_notes(&app, Some(&tag), one_note("01J8", "first")).await;
    assert_eq!(written, StatusCode::OK);

    let fresh = etag_of(&app, "/api/works/t/notes").await;
    let read_back = app
        .oneshot(
            Request::get("/api/works/t/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(read_back.into_body(), 1 << 20)
        .await
        .unwrap();
    let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

    assert_ne!(fresh, tag, "the document changed, so the tag must have");
    assert_eq!(parsed.notes.len(), 1);
    assert_eq!(parsed.notes[0].body, "first");
}

/// The lost-update case itself, which is the reason any of this exists:
/// two devices read the same document, both write, and the second must be
/// told rather than quietly replacing the first reader's note.
#[tokio::test]
async fn a_second_writer_holding_the_same_tag_is_refused_rather_than_winning() {
    let tmp = crate::test_support::Fixture::new("notes-race");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);
    let shared = etag_of(&app, "/api/works/t/notes").await;

    let (first, _) = write_notes(&app, Some(&shared), one_note("01J8A", "device one")).await;
    assert_eq!(first, StatusCode::OK);

    let (second, message) = write_notes(&app, Some(&shared), one_note("01J8B", "device two")).await;

    let survived = app
        .oneshot(
            Request::get("/api/works/t/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(survived.into_body(), 1 << 20)
        .await
        .unwrap();
    let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(second, StatusCode::PRECONDITION_FAILED);
    assert!(
        message.contains("ETag is now"),
        "a loser needs the current validator to retry against: {message}"
    );
    assert_eq!(parsed.notes[0].body, "device one", "the first write stands");
}

/// The validation is not merely unit-tested: a bad anchor dispatched
/// through the router comes back as a refusal a client can read, and with
/// the status that tells it apart from a malformed body. A guard nothing
/// calls is the shape O4 records — correct by inspection only.
#[tokio::test]
async fn a_note_on_a_slot_that_does_not_exist_is_refused_through_the_router() {
    let tmp = crate::test_support::Fixture::new("notes-bad-slot");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);
    let tag = etag_of(&app, "/api/works/t/notes").await;

    let (status, message) = write_notes(
        &app,
        Some(&tag),
        r#"{"work":"t","notes":[{"id":"01J8","edition":"raw","slot":99,
               "body":"b","created":"c","updated":"u"}]}"#
            .to_string(),
    )
    .await;

    let (duplicate, dupe_message) = write_notes(
        &app,
        Some(&tag),
        r#"{"work":"t","notes":[
               {"id":"01J8","edition":"raw","slot":0,"body":"a","created":"c","updated":"u"},
               {"id":"01J8","edition":"raw","slot":1,"body":"b","created":"c","updated":"u"}]}"#
            .to_string(),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        message.contains("99") && message.contains("slots"),
        "the refusal must say what was wrong: {message}"
    );
    assert_eq!(
        duplicate,
        StatusCode::BAD_REQUEST,
        "a malformed document is not a missing one"
    );
    assert!(dupe_message.contains("01J8"), "name the id: {dupe_message}");
}

/// A body with two faulty notes is refused for the first one it sends, as
/// `notes::validate_against` reads the body, and a body it accepts is stored
/// and answered in id order, the order the next read sends.
#[tokio::test]
async fn a_notes_body_is_checked_in_its_order_and_answered_in_id_order() {
    let tmp = crate::test_support::Fixture::new("notes-two-faults");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);
    let tag = etag_of(&app, "/api/works/t/notes").await;

    let (status, message) = write_notes(
        &app,
        Some(&tag),
        r#"{"work":"t","notes":[
               {"id":"01B","edition":"nope","slot":0,"body":"b","created":"c","updated":"u"},
               {"id":"01A","edition":"raw","slot":0,"color":"Red","body":"a","created":"c",
                "updated":"u"}]}"#
            .to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{message}");
    assert!(
        message.contains("`01B`") && message.contains("`nope`"),
        "refused for the note sent first: {message}"
    );

    let (status, answered) = write_notes(
        &app,
        Some(&tag),
        r#"{"work":"t","notes":[
               {"id":"01B","edition":"raw","slot":1,"body":"b","created":"c","updated":"u"},
               {"id":"01A","edition":"raw","slot":0,"body":"a","created":"c","updated":"u"}]}"#
            .to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    let ids: Vec<String> = serde_json::from_str::<crate::notes::Notes>(&answered)
        .expect("notes")
        .notes
        .into_iter()
        .map(|note| note.id)
        .collect();
    assert_eq!(ids, ["01A", "01B"], "answered in id order");
}

/// The lost-update guard under a genuine race, not a sequence.
///
/// `a_second_writer_holding_the_same_tag_is_refused_rather_than_winning`
/// sends one write and then the other, which is the case a `RwLock` would
/// also pass without holding anything across the read-modify-write.
/// `Work::notes_guard` exists for the interleaving that test cannot
/// produce: two writes *in flight together*, both having read the same
/// document and both finding their validator current.
///
/// **Raced many times, deliberately.** A single race is not a reliable
/// detector: with the guard removed, two runs in five happened to serialise
/// and pass. One round is therefore a test that can miss the very defect it
/// exists for, which is worse than no test — it reports green for a reason it
/// never checked. Twenty rounds put the chance of every one of them
/// serialising at roughly `0.4^20`.
///
/// It cannot fail *falsely*, whatever the interleaving: exactly one of two
/// writes can be holding the current tag when it compares, so the
/// assertion is on the pair of outcomes rather than on which request won.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_writes_in_flight_together_leave_exactly_one_winner() {
    let tmp = crate::test_support::Fixture::new("notes-concurrent");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    for round in 0..20 {
        let shared = etag_of(&app, "/api/works/t/notes").await;
        let first = tokio::spawn({
            let app = app.clone();
            let tag = shared.clone();
            let body = one_note(&format!("01RACEA{round}"), "device one");
            async move { write_notes(&app, Some(&tag), body).await }
        });
        let second = tokio::spawn({
            let app = app.clone();
            let tag = shared.clone();
            let body = one_note(&format!("01RACEB{round}"), "device two");
            async move { write_notes(&app, Some(&tag), body).await }
        });
        let (a, b) = (first.await.unwrap(), second.await.unwrap());

        let mut statuses = [a.0, b.0];
        statuses.sort_by_key(StatusCode::as_u16);
        if statuses != [StatusCode::OK, StatusCode::PRECONDITION_FAILED] {
            panic!(
                "round {round}: {statuses:?} — one write must land and the other be told;                      two 200s is a lost note"
            );
        }
    }

    let survived = app
        .oneshot(
            Request::get("/api/works/t/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(survived.into_body(), 1 << 20)
        .await
        .unwrap();
    let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(
        parsed.notes.len(),
        1,
        "each round replaces the document with one note, so the last winner's is what stands"
    );
}

/// A body this server will not parse is answered in this server's own
/// words, not serde's — the same rule as O5's for a malformed page index,
/// and `patch_alignments` follows it for a malformed correction.
#[tokio::test]
async fn a_body_that_is_not_notes_is_refused_in_this_servers_own_words() {
    let tmp = crate::test_support::Fixture::new("notes-garbage");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);
    let tag = etag_of(&app, "/api/works/t/notes").await;

    let (status, message) = write_notes(&app, Some(&tag), "{\"nope\":1}".to_string()).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        message.contains("these are not notes"),
        "the refusal must be this server's own: {message}"
    );
}

/// The notes document names its format version, so a newer taiyaku's reader
/// can tell what wrote it (pre-deploy spec §3, decision 5).
#[tokio::test]
async fn the_notes_document_declares_taiyaku_notes_1() {
    let tmp = crate::test_support::Fixture::new("notes-version");
    let app = build_router(Arc::new(crate::test_support::state(&tmp)));
    let tag = etag_of(&app, "/api/works/t/notes").await;
    let (written, _) = write_notes(&app, Some(&tag), one_note("01J8", "kept")).await;
    assert_eq!(written, StatusCode::OK, "the fixture's note is written");

    let (status, _, body) = fetch(&app, "/api/works/t/notes", None).await;

    assert_eq!(status, StatusCode::OK);
    let doc: serde_json::Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(
        doc["taiyaku_notes"],
        serde_json::json!(1),
        "the version is written: {doc}"
    );
}

/// A notes body written before the version was named has no `taiyaku_notes`,
/// and is read as version 1 (pre-deploy spec §3, decision 5).
#[tokio::test]
async fn a_notes_body_naming_no_version_is_read_as_version_1() {
    let tmp = crate::test_support::Fixture::new("notes-no-version");
    let app = build_router(Arc::new(crate::test_support::state(&tmp)));
    let tag = etag_of(&app, "/api/works/t/notes").await;
    let body = r#"{"work":"t","notes":[{"id":"01J8","edition":"raw","slot":1,"body":"unversioned",
        "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}]}"#;

    let (status, message) = write_notes(&app, Some(&tag), body.to_string()).await;

    assert_eq!(status, StatusCode::OK, "{message}");
}

/// A notes body from a newer taiyaku may mean what this build cannot read, so
/// it is refused before anything is written (pre-deploy spec §3, decision 5).
/// Sent with a current tag, so only the version can refuse it.
#[tokio::test]
async fn a_notes_body_declaring_a_newer_version_is_refused_and_nothing_is_written() {
    let tmp = crate::test_support::Fixture::new("notes-newer");
    let app = build_router(Arc::new(crate::test_support::state(&tmp)));
    let first = etag_of(&app, "/api/works/t/notes").await;
    let (written, _) = write_notes(&app, Some(&first), one_note("01J8", "kept")).await;
    assert_eq!(written, StatusCode::OK, "the fixture's note is written");
    let (_, _, before) = fetch(&app, "/api/works/t/notes", None).await;
    let tag = etag_of(&app, "/api/works/t/notes").await;
    let newer = r#"{"taiyaku_notes":2,"work":"t","notes":[{"id":"01J9","edition":"raw","slot":1,
        "body":"from a newer taiyaku","created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}]}"#;

    let (status, message) = write_notes(&app, Some(&tag), newer.to_string()).await;

    assert_eq!(
        (status, message.as_str()),
        (
            StatusCode::BAD_REQUEST,
            "this document declares taiyaku_notes format version 2, and this build reads 1. \
             Refusing to read it rather than dropping the fields it does not understand and \
             writing them back out as version 1 — upgrade taiyaku instead"
        )
    );
    let (_, _, after) = fetch(&app, "/api/works/t/notes", None).await;
    assert_eq!(
        String::from_utf8_lossy(&after),
        String::from_utf8_lossy(&before),
        "the stored notes are unchanged"
    );
}

/// A newer notes body may add a key this build has no name for; it is still
/// refused as newer, not as not notes, because the version is asked before
/// the shape (O264).
#[tokio::test]
async fn a_newer_notes_body_with_a_key_this_build_does_not_know_is_refused_as_newer() {
    let tmp = crate::test_support::Fixture::new("notes-newer-new-key");
    let app = build_router(Arc::new(crate::test_support::state(&tmp)));
    let tag = etag_of(&app, "/api/works/t/notes").await;
    let newer = r#"{"taiyaku_notes":2,"work":"t","notes":[{"id":"01J9","edition":"raw","slot":1,
        "ink":"red","body":"b","created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}]}"#;

    let (status, message) = write_notes(&app, Some(&tag), newer.to_string()).await;

    assert_eq!(
        (status, message.as_str()),
        (
            StatusCode::BAD_REQUEST,
            "this document declares taiyaku_notes format version 2, and this build reads 1. \
             Refusing to read it rather than dropping the fields it does not understand and \
             writing them back out as version 1 — upgrade taiyaku instead"
        )
    );
}

/// At this build's own version an unknown key is still not notes: asking the
/// version first refuses only what is newer.
#[tokio::test]
async fn a_version_1_notes_body_with_a_key_this_build_does_not_know_is_not_notes() {
    let tmp = crate::test_support::Fixture::new("notes-v1-new-key");
    let app = build_router(Arc::new(crate::test_support::state(&tmp)));
    let tag = etag_of(&app, "/api/works/t/notes").await;
    let body = r#"{"taiyaku_notes":1,"work":"t","notes":[{"id":"01J9","edition":"raw","slot":1,
        "ink":"red","body":"b","created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}]}"#;

    let (status, message) = write_notes(&app, Some(&tag), body.to_string()).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{message}");
    assert!(
        message.starts_with("these are not notes: unknown field `ink`"),
        "{message}"
    );
}

/// A work nobody has labelled yet still answers with a document, not a
/// 404 or a 204 — the same reasoning `get_notes`'s empty-document test
/// carries, so a client's first write has something to be conditional on.
#[tokio::test]
async fn a_work_nobody_has_labelled_answers_an_empty_document_with_a_tag() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/works/w/truth")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().contains_key(header::ETAG),
        "a client's first write needs a validator to be conditional on"
    );
}

#[tokio::test]
async fn writing_truth_without_if_match_is_refused() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let body = taiyaku_core::TruthDoc::new().to_json();
    let response = router
        .oneshot(
            Request::put("/api/works/w/truth")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
}

#[tokio::test]
async fn a_successful_truth_write_stores_the_document_and_returns_an_etag() {
    // O35: a PUT response with no ETag turns every later write into a
    // silent no-op, and truth's must carry one.
    //
    // **And the document is read back**: a status and a header alone are
    // indistinguishable from what a no-op produces. This is the only save path
    // the labelling tool has: a regression here loses hand work behind a 200
    // OK. Assert the payload, not the status.
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let tag = etag_of(&router, "/api/works/w/truth").await;
    let body = r#"{"taiyaku_truth":1,"editions":{"a":{
        "members":[{"sha256":"real"}],
        "pages":{"0":[{"x":0.1,"y":0.2,"w":0.3,"h":0.4,"axis":"v",
                       "kind":"dialogue","ignore":false,"text":"ハロー"}],
                 "1":[]}}}}"#;
    let response = router
        .clone()
        .oneshot(
            Request::put("/api/works/w/truth")
                .header("content-type", "application/json")
                .header("if-match", tag)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().contains_key(header::ETAG),
        "O35: without this every later write is a silent no-op"
    );

    let got = router
        .oneshot(
            Request::get("/api/works/w/truth")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(got.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(got.into_body(), 1 << 16)
        .await
        .expect("body");
    let back: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    let edition = &back["editions"]["a"];
    assert_eq!(
        edition["members"][0]["sha256"].as_str(),
        Some("real"),
        "the provenance the boxes were drawn against came back: {back}"
    );
    let region = &edition["pages"]["0"][0];
    assert_eq!(region["x"].as_f64(), Some(0.1));
    assert_eq!(region["h"].as_f64(), Some(0.4));
    assert_eq!(region["kind"].as_str(), Some("dialogue"));
    assert_eq!(region["axis"].as_str(), Some("v"));
    assert_eq!(region["text"].as_str(), Some("ハロー"));
    assert_eq!(
        edition["pages"]["1"].as_array().map(Vec::len),
        Some(0),
        "page 1 was examined and found empty; a present, empty key is the \
         labelling progress tracker, not a page nobody opened"
    );
}

#[tokio::test]
async fn a_stale_validator_is_refused_with_the_current_tag() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let body = taiyaku_core::TruthDoc::new().to_json();
    let response = router
        .oneshot(
            Request::put("/api/works/w/truth")
                .header("content-type", "application/json")
                .header("if-match", "\"not-the-current-tag\"")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
}

#[tokio::test]
async fn a_malformed_truth_body_is_refused_in_this_servers_own_words() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let tag = etag_of(&router, "/api/works/w/truth").await;
    let response = router
        .oneshot(
            Request::put("/api/works/w/truth")
                .header("content-type", "application/json")
                .header("if-match", tag)
                .body(Body::from("{\"nonsense\":true}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    // The status alone does not establish what this test is named for.
    // `{"nonsense":true}` is valid JSON missing a required field, so
    // axum's default `Json` extractor would answer 422 rather than 400 —
    // but a 400 carrying axum's wording would still pass a status-only
    // check, and O5's rule is about the words, not the number.
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body);
    assert!(
        message.contains("not a readable truth document"),
        "the refusal must be this server's own: {message}"
    );
}

/// A labeller on a newer client sends a document this build would store
/// without the fields it has no name for, so it is refused in the sidecar's
/// words (O264).
#[tokio::test]
async fn a_truth_body_from_a_newer_taiyaku_is_refused_in_the_sidecars_words() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let tag = etag_of(&router, "/api/works/w/truth").await;
    let newer = taiyaku_core::TRUTH_VERSION + 1;
    let reads = taiyaku_core::TRUTH_VERSION;
    let response = router
        .oneshot(
            Request::put("/api/works/w/truth")
                .header("content-type", "application/json")
                .header("if-match", tag)
                .body(Body::from(format!(
                    r#"{{"taiyaku_truth":{newer},"editions":{{}}}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body);
    assert!(
        message.contains(&format!(
            "this document declares taiyaku_truth format version {newer}, and this build reads \
             {reads}. Refusing to read it rather than dropping the fields it does not understand \
             and writing them back out as version {reads} — upgrade taiyaku instead"
        )),
        "{message}"
    );
}

/// `?w=1` must mean "no resize", not "resize to one pixel".
///
/// A filter that admitted 1 would re-encode a 16-pixel page down to one pixel
/// and hand it to the reader as a JPEG, because `cover::fits_within` calls a
/// page fitting only when it is already within the requested edge.
#[tokio::test]
async fn a_width_of_one_is_absent_rather_than_a_one_pixel_page() {
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/w/a/pages/0?w=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let decoded = image::load_from_memory(&body).expect("a decodable image");
    assert_eq!(
        (decoded.width(), decoded.height()),
        (16, 16),
        "the page untouched, not a one-pixel rendering of it"
    );
}

/// Naming an unknown work must be a 404 that says so. `NoSuchWork`'s
/// `into_response` could return `Response::default()` — an empty 200 —
/// with the suite green, because every test that asks for a missing work
/// went through a route that builds its own refusal, and the ones that go
/// through this impl only ever asked for works that exist.
///
/// An empty 200 is the worst of the available answers: the reader's client
/// cannot tell "no such work" from "a work with nothing in it".
#[tokio::test]
async fn an_unknown_work_is_a_404_that_names_it() {
    let tmp = crate::test_support::Fixture::new("routes-unknown");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/nope/raw/pages/0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("nope"),
        "must name the work asked for: {text}"
    );
}

/// The work list must contain the works. `list_works` could return
/// `Response::default()` — an empty 200 — and
/// `the_reader_can_list_a_work_read_its_sidecar_and_fetch_a_page` would
/// still pass, because it asserts the status and never looks at the body.
/// A reader given an empty list shows an empty library.
#[tokio::test]
async fn the_work_list_names_every_work_rather_than_being_merely_ok() {
    let tmp = crate::test_support::Fixture::new("routes-list");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = build_router(state);

    let response = app
        .oneshot(Request::get("/api/works").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    // A JSON array of objects rather than of bare ids — `slug` is this
    // object's `id`.
    let listed: Vec<serde_json::Value> = serde_json::from_slice(&body).expect("a JSON array");
    let slugs: Vec<&str> = listed
        .iter()
        .map(|w| w["slug"].as_str().expect("slug is a string"))
        .collect();
    assert_eq!(slugs, vec!["t"]);
}

/// `works::summary`'s `SELECT` against `state.library` can fail — a locked
/// or corrupted database — and `list_works` answers `500` rather than
/// propagating a panic through `spawn_blocking`. Forced by dropping the very
/// table the query reads, since nothing else in this fixture can make
/// SQLite refuse it.
#[tokio::test]
async fn a_broken_library_connection_answers_500_rather_than_panicking() {
    let tmp = crate::test_support::Fixture::new("routes-list-db-broken");
    let state = crate::test_support::state(&tmp);
    state
        .library
        .conn()
        .execute("DROP TABLE work", [])
        .expect("drop the table `summary` queries");
    let app = build_router(Arc::new(state));

    let response = crate::test_support::get(&app, "/api/works").await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// **Two readers hold two positions in one work.**
///
/// Spec A2 §5.1 records the shape of the problem: `progress` and `note` carry
/// `user_id` in their primary key, so the database can hold two readers.
/// `progress`'s key is `(user_id, work_id, edition)`.
#[tokio::test]
async fn two_readers_hold_separate_positions_in_one_work() {
    use base64::Engine as _;

    let tmp = crate::test_support::Fixture::new("two-readers");
    let state = crate::test_support::state(&tmp);
    {
        let conn = state.library.conn();
        let phc = taiyaku_library::credential::hash_password("bobs-password").expect("hashes");
        conn.execute(
            "INSERT INTO user (name, password_hash, role) VALUES ('bob', ?1, 'user')",
            [&phc],
        )
        .expect("adds bob");
    }
    let state = Arc::new(state);

    let basic = |name: &str, password: &str| {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{name}:{password}").as_bytes());
        format!("Basic {encoded}")
    };
    let alice = basic(
        crate::test_support::FIXTURE_NAME,
        crate::test_support::FIXTURE_PASSWORD,
    );
    let bob = basic("bob", "bobs-password");

    // The fixture edition is four pages, so 1 and 3 are both real slots
    // and distinguishable.
    for (credential, slot) in [(&alice, 1), (&bob, 3)] {
        let put = build_router(state.clone())
            .oneshot(
                Request::put("/api/works/t/progress")
                    .header("authorization", credential)
                    .header("content-type", "application/json")
                    .body(Body::from(format!(r#"{{"edition":"raw","slot":{slot}}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::NO_CONTENT);
    }

    for (credential, expected) in [(&alice, 1), (&bob, 3)] {
        let response = build_router(state.clone())
            .oneshot(
                Request::get("/api/works/t/progress")
                    .header("authorization", credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 1 << 16)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            body.contains(&format!("\"slot\":{expected}")),
            "expected slot {expected}, read {body}"
        );
    }
}

/// The same, for notes: two readers annotate one work and neither sees the
/// other's.
#[tokio::test]
async fn two_readers_hold_separate_notes_on_one_work() {
    use base64::Engine as _;

    let tmp = crate::test_support::Fixture::new("two-readers-notes");
    let state = crate::test_support::state(&tmp);
    {
        let conn = state.library.conn();
        let phc = taiyaku_library::credential::hash_password("bobs-password").expect("hashes");
        conn.execute(
            "INSERT INTO user (name, password_hash, role) VALUES ('bob', ?1, 'user')",
            [&phc],
        )
        .expect("adds bob");
    }
    let state = Arc::new(state);

    let basic = |name: &str, password: &str| {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{name}:{password}").as_bytes());
        format!("Basic {encoded}")
    };
    let alice = basic(
        crate::test_support::FIXTURE_NAME,
        crate::test_support::FIXTURE_PASSWORD,
    );
    let bob = basic("bob", "bobs-password");

    let note = |id: &str, body: &str| {
        format!(
            r#"{{"work":"t","notes":[{{"id":"{id}","edition":"raw","slot":0,
               "body":"{body}","created":"2026-08-18T00:00:00Z",
               "updated":"2026-08-18T00:00:00Z"}}]}}"#
        )
    };

    // A notes write carries `If-Match` over the reader's *own* notes, so each
    // one reads its validator first. That is itself part of what this test
    // pins: two readers of one work hold two documents and therefore two
    // ETags, and bob's write must not be refused as stale because alice wrote
    // hers first.
    for (credential, id, text) in [(&alice, "01HALICE", "alice's"), (&bob, "01HBOBBB", "bob's")] {
        let current = build_router(state.clone())
            .oneshot(
                Request::get("/api/works/t/notes")
                    .header("authorization", credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tag = current
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .expect("an ETag on the reader's own notes")
            .to_string();

        let put = build_router(state.clone())
            .oneshot(
                Request::put("/api/works/t/notes")
                    .header("authorization", credential)
                    .header("content-type", "application/json")
                    .header("if-match", &tag)
                    .body(Body::from(note(id, text)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK, "writing {text}");
    }

    for (credential, mine, theirs) in [(&alice, "alice's", "bob's"), (&bob, "bob's", "alice's")] {
        let response = build_router(state.clone())
            .oneshot(
                Request::get("/api/works/t/notes")
                    .header("authorization", credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 1 << 16)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains(mine), "{mine} is missing from {body}");
        assert!(
            !body.contains(theirs),
            "{theirs} leaked into another reader's notes: {body}"
        );
    }
}

/// A correction on a work with **no document** writes rows and no file.
///
/// Spec C1 §5.3. The assertion that matters is the second one: the server must
/// not mint a `taiyaku.json` into a tree spec B took care never to open for
/// writing.
#[tokio::test]
async fn correcting_a_work_with_no_document_writes_rows_and_no_file() {
    let (router, state, fixture) = crate::test_support::router_over_a_scanned_work();
    let slug = state.works.snapshot()[0].id.clone();
    assert!(
        !state.works.snapshot()[0].has_document(),
        "the fixture is only meaningful for a work with no document"
    );

    let before = std::fs::read_dir(fixture.path())
        .expect("reads the fixture directory")
        .count();

    let tag = etag_of(&router, &format!("/api/works/{slug}")).await;
    let (status, body) = correct_with_body(
        &router,
        &slug,
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let after = std::fs::read_dir(fixture.path())
        .expect("reads the fixture directory")
        .count();
    assert_eq!(before, after, "no taiyaku.json appeared beside the work");

    let conn = state.library.conn();
    let hand: i64 = conn
        .query_row("SELECT COUNT(*) FROM run WHERE by = 'hand'", [], |row| {
            row.get(0)
        })
        .expect("counting hand-corrected runs");
    assert!(hand > 0, "the correction reached the rows");
}

// ---------------------------------------------------------------------------
// The grant on `list_works` (`GET /api/works`): spec C1's per-root visibility,
// which also reaches the page routes (`view::of_edition`) and the OPDS
// mirror (`opds::visible_editions_by_work`).
//
// Mirrors `opds::tests::router_with_two_works_one_withheld` exactly: two
// works imported through `taiyaku_library::import::scanned` into a real
// library database (a `work`/`edition`/`member` row is what
// `browse::visible_editions` reads — an in-memory-only `Work` cannot
// exercise the grant), filed under two different roots, with the fixture
// account (`user_id` 1) granted only one of them.
// ---------------------------------------------------------------------------

/// A router over **two** works, one filed under the root the fixture account
/// holds and one under a root it does not.
fn router_with_two_works_one_withheld(dir: &std::path::Path) -> (axum::Router, Arc<ServeState>) {
    let root = dir;
    let shelf = root.to_string_lossy().into_owned();

    let visible_json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Visible Work", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/v1.cbz", "sha256": "v", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );
    let hidden_json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Hidden Work", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/h1.cbz", "sha256": "h", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let visible_doc =
            taiyaku_core::Sidecar::from_json(&visible_json).expect("visible sidecar parses");
        let visible_id = taiyaku_library::import::scanned(&conn, &visible_doc, None)
            .expect("import the visible work");
        let hidden_doc =
            taiyaku_core::Sidecar::from_json(&hidden_json).expect("hidden sidecar parses");
        let hidden_id = taiyaku_library::import::scanned(&conn, &hidden_doc, None)
            .expect("import the hidden work");

        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld')",
            [],
        )
        .expect("two roots");
        conn.execute(
            "UPDATE member SET root_id = 1 WHERE edition_id IN \
             (SELECT id FROM edition WHERE work_id = ?1)",
            [visible_id],
        )
        .expect("files the visible work under the held root");
        conn.execute(
            "UPDATE member SET root_id = 2 WHERE edition_id IN \
             (SELECT id FROM edition WHERE work_id = ?1)",
            [hidden_id],
        )
        .expect("files the hidden work under the withheld root");
        crate::test_support::grant_root(&conn, 1);
    }
    let works = crate::works::from_library(&library).expect("projects the imported works");
    let state = Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = build_router(state.clone());
    (router, state)
}

/// `GET /api/works` must answer the same grant `opds::works` and the page
/// routes do: a restricted account is not told the id, title, series or volume
/// of a work outside every root it holds. This mirrors
/// `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` for the
/// JSON route.
#[tokio::test]
async fn the_work_list_does_not_name_a_work_outside_the_grant() {
    let tmp = crate::test_support::Fixture::new("routes-grant-works-list");
    let (router, _state) = router_with_two_works_one_withheld(&tmp);

    let response = crate::test_support::get(&router, "/api/works").await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("Visible Work"),
        "the granted work must still be listed: {body}"
    );
    assert!(
        !body.contains("Hidden Work"),
        "a work outside the grant must not be named in the work list: {body}"
    );
}

/// How [`an_account_granted_only_a_library_with_no_dir_sees_nothing`]
/// counts what a route's answer shows.
enum Shows {
    /// The elements of a JSON body's lists: the body itself when it is one,
    /// or every list among an object's fields.
    Lists,
    /// An OPDS feed's entries.
    Entries,
    /// One thing: a page, a tile, a document, a download, a redirect, or the
    /// 204 that says the work is there and holds no position.
    Itself,
}

/// What `response` shows of the library, counted as `shows` says. The 404 a
/// work outside the grant gets shows nothing. Any other status panics, so a
/// refusal for some other reason cannot pass as seeing nothing.
async fn what_it_shows(call: &str, response: Response, shows: &Shows) -> usize {
    let status = response.status();
    // Lossy, because a page or a tile is not text.
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("reads the body");
    let body = String::from_utf8_lossy(&bytes);
    match status {
        StatusCode::NOT_FOUND => 0,
        StatusCode::OK | StatusCode::NO_CONTENT | StatusCode::TEMPORARY_REDIRECT => match shows {
            Shows::Itself => 1,
            Shows::Entries => body.matches("<entry>").count(),
            Shows::Lists => {
                let json: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{call}: {e}: {body}"));
                let length = |value: &serde_json::Value| value.as_array().map_or(0, Vec::len);
                match &json {
                    serde_json::Value::Object(fields) => fields.values().map(length).sum(),
                    list => length(list),
                }
            }
        },
        other => panic!("{call} answered {other}: {body}"),
    }
}

/// The `GET` [`an_account_granted_only_a_library_with_no_dir_sees_nothing`]
/// sends for a route [`grant_postures`] says reads the grant, and how it counts
/// the answer.
///
/// **Classified by the path's shape rather than listed**, so a route added to
/// the table is driven with no edit here: an `/api/` path with no parameter is
/// a listing, an OPDS path below no edition is a feed, and any other path
/// names one thing. A route the shape misreads fails one of the test's two
/// passes, a route a `GET` cannot ask answers 405 and fails both, and a path
/// parameter this does not fill panics, naming the route.
fn get_for(pattern: &str, held_library: i64) -> (String, Shows) {
    let shows = if pattern.starts_with("/opds/") && !pattern.contains("{edition}") {
        Shows::Entries
    } else if pattern.starts_with("/api/") && !pattern.contains('{') {
        Shows::Lists
    } else {
        Shows::Itself
    };
    let uri = pattern
        .replace("{work}", "w")
        .replace("{slug}", "w")
        .replace("{edition}", "raw")
        .replace("{index}", "0")
        .replace("{ordinal}", "0")
        .replace("{number}", "1")
        .replace("{id}", "1")
        .replace("{library}", &held_library.to_string());
    assert!(
        !uri.contains('{'),
        "{pattern}: a path parameter this test does not fill"
    );
    // The search answers a query, and `W` is the fixture work's title.
    let uri = if pattern == "/api/library/search" {
        format!("{uri}?q=W")
    } else {
        uri
    };
    (uri, shows)
}

/// **An account granted only a library with no directory sees nothing**, through
/// the real guard, on every `GET` route [`grant_postures`] says reads the
/// grant (spec §3), each asked as [`get_for`] says. The grant is written by
/// hand, as `account::tests` writes it, and `access_for` reads it as
/// `Access::Only` with no directory, which the guard puts on every request.
///
/// **The same requests from the same account granted the work's own library
/// show the work.** That pass is what shows each empty answer is the access's,
/// not a fixture holding nothing or a broken route, and it takes the same
/// restricted path, so neither pass is answered by `Every`'s shortcut in
/// `on_the_grant`.
#[tokio::test]
async fn an_account_granted_only_a_library_with_no_dir_sees_nothing() {
    let (state, _fixture) = crate::test_support::state_whose_grant_waits_on_the_library();
    let held_library: i64 = {
        let conn = state.library.conn();
        conn.execute_batch(
            "INSERT INTO series (id, sort_title) VALUES (1, 'W');
             INSERT INTO series_title (series_id, title, kind, is_primary)
             VALUES (1, 'W', 'english', 1);
             UPDATE work SET series_id = 1, volume = 1.0;
             INSERT INTO library (id, name) VALUES (90, 'Directoryless');",
        )
        .expect("puts the work in a series, beside a library with no directory");
        crate::test_support::grant_root(&conn, 1);
        crate::test_support::library_of_root(&conn, 1)
    };
    let router =
        crate::test_support::authenticating(build_router(Arc::clone(&state)), &state.library);
    let routes: Vec<(String, Shows)> = grant_postures()
        .into_iter()
        .filter(|(_, posture)| !matches!(posture, GrantPosture::GrantIndependent { .. }))
        .map(|(pattern, _)| get_for(pattern, held_library))
        .collect();

    let mut wrong = Vec::new();
    for (grant, sees) in [
        ("granted the work's library", true),
        ("granted only a library with no directory", false),
    ] {
        if !sees {
            state
                .library
                .conn()
                .execute_batch(
                    "INSERT INTO user_library (user_id, library_id) VALUES (1, 90);
                     DELETE FROM user_library WHERE user_id = 1 AND library_id <> 90;",
                )
                .expect("leaves the account granted the directoryless library alone");
        }
        for (uri, shows) in &routes {
            let response = crate::test_support::get(&router, uri).await;
            let shown = what_it_shows(uri, response, shows).await;
            if (shown > 0) != sees {
                wrong.push(format!("GET {uri}, {grant}: shows {shown}"));
            }
        }
    }
    // Collected, so a failing run names every route that answered wrongly.
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// **A request with no principal sees nothing.** The guard puts a principal on
/// every guarded route or refuses the request, so a handler without one was
/// reached some other way, a direct call or a routing mistake, and must see
/// nothing rather than everything. The handlers that read the access list
/// nothing, one library's feed answers the 404 an ungranted library gets, and
/// a handler that asks the grant answers 401.
///
/// **The same calls as the fixture's administrator show the work**, so each
/// empty answer is the missing principal's.
#[tokio::test]
async fn a_request_with_no_principal_sees_nothing() {
    use axum::extract::{Path as AxPath, State};
    use axum::http::HeaderMap;

    let (state, _fixture) = crate::test_support::state_whose_grant_waits_on_the_library();
    let held_library = crate::test_support::library_of_root(&state.library.conn(), 1);

    let mut wrong = Vec::new();
    for (who, extensions, sees) in [
        ("as the administrator", as_the_fixture_account(), true),
        ("with no principal", axum::http::Extensions::new(), false),
    ] {
        let shown = [
            (
                "list_works",
                what_it_shows(
                    "list_works",
                    list_works(State(Arc::clone(&state)), extensions.clone()).await,
                    &Shows::Lists,
                )
                .await,
            ),
            (
                "opds::libraries",
                what_it_shows(
                    "opds::libraries",
                    crate::opds::libraries(State(Arc::clone(&state)), extensions.clone()).await,
                    &Shows::Entries,
                )
                .await,
            ),
            (
                "opds::library_works",
                what_it_shows(
                    "opds::library_works",
                    crate::opds::library_works(
                        State(Arc::clone(&state)),
                        AxPath(held_library),
                        extensions.clone(),
                    )
                    .await,
                    &Shows::Entries,
                )
                .await,
            ),
        ];
        for (handler, shown) in shown {
            if (shown > 0) != sees {
                wrong.push(format!("{handler}, {who}: shows {shown}"));
            }
        }

        let response = get_sidecar(
            State(Arc::clone(&state)),
            AxPath("w".to_string()),
            HeaderMap::new(),
            extensions,
        )
        .await;
        let expected = if sees {
            StatusCode::OK
        } else {
            StatusCode::UNAUTHORIZED
        };
        if response.status() != expected {
            wrong.push(format!(
                "get_sidecar, {who}: answered {}, not {expected}",
                response.status()
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// One work with an edition in each of two libraries, read by a reader granted
/// the hub directory's library alone. The work is listed to them with its hub
/// edition `en` alone. The edition `raw`, under the other library's directory,
/// is not named in the list, the sidecar or the OPDS feed, and its pages,
/// thumbnail and downloads answer as a work that does not exist, here and
/// under OPDS.
#[tokio::test]
async fn a_reader_is_refused_the_edition_a_work_has_under_a_library_they_are_not_granted() {
    let (router, _state, _fixture, slug) =
        crate::test_support::router_over_a_work_whose_editions_sit_in_two_libraries();
    let json = |body: String| -> serde_json::Value { serde_json::from_str(&body).expect("json") };
    let get = |path: String| {
        let router = router.clone();
        async move {
            let response = crate::test_support::get(&router, &path).await;
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap();
            (status, String::from_utf8_lossy(&bytes).into_owned())
        }
    };

    let (status, listed) = get("/api/works".to_string()).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        json(listed),
        serde_json::json!([{
            "slug": slug,
            "title": "Series - v01",
            "series": "Series",
            "volume": 1.0,
            "editions": ["en"],
            "cover": format!("/api/works/{slug}/en/pages/0/thumb"),
        }]),
        "the one work, with the granted library's edition alone"
    );

    let (status, sidecar) = get(format!("/api/works/{slug}")).await;
    assert_eq!(status, StatusCode::OK, "{sidecar}");
    let editions: Vec<String> = json(sidecar)["editions"]
        .as_object()
        .expect("editions")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        editions,
        ["en"],
        "the sidecar names no edition outside the grant"
    );

    let (status, feed) = get(format!("/opds/v1.2/works/{slug}")).await;
    assert_eq!(status, StatusCode::OK, "{feed}");
    assert!(feed.contains("/en/"), "{feed}");
    assert!(
        !feed.contains("/raw/"),
        "the feed names the edition outside the grant: {feed}"
    );

    let (status, page) = get(format!("/api/works/{slug}/en/pages/0")).await;
    assert_eq!(status, StatusCode::OK, "the granted edition's page: {page}");

    for path in [
        format!("/api/works/{slug}/raw/pages/0"),
        format!("/api/works/{slug}/raw/pages/0/thumb"),
        format!("/api/works/{slug}/raw/download"),
        format!("/api/works/{slug}/raw/members/0/download"),
        format!("/opds/v1.2/works/{slug}/raw/pages/0"),
        format!("/opds/v1.2/works/{slug}/raw/cover"),
        format!("/opds/v1.2/works/{slug}/raw/download"),
    ] {
        let (status, body) = get(path.clone()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert_eq!(body, format!("no work `{slug}`"), "{path}");
    }
}

/// Spec §7, "Access", through the routes: the same work read by a reader
/// granted the library holding its edition `raw` and not its hub `en`. The
/// work is listed to them with `raw` alone, whose page and download they are
/// served; `en`'s page and download answer as a work that does not exist.
#[tokio::test]
async fn a_reader_granted_the_library_without_the_hub_is_served_the_work_through_its_other_edition()
{
    let (router, _state, _fixture, slug) =
        crate::test_support::router_over_a_work_granted_through_its_other_edition();
    let get = |path: String| {
        let router = router.clone();
        async move {
            let response = crate::test_support::get(&router, &path).await;
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap();
            (status, String::from_utf8_lossy(&bytes).into_owned())
        }
    };

    let (status, listed) = get("/api/works".to_string()).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let listed: serde_json::Value = serde_json::from_str(&listed).expect("json");
    assert_eq!(
        listed
            .as_array()
            .expect("a list")
            .iter()
            .map(|w| (w["slug"].clone(), w["editions"].clone()))
            .collect::<Vec<_>>(),
        [(serde_json::json!(slug), serde_json::json!(["raw"]))],
        "the one work, with the granted library's edition alone"
    );
    for path in [
        format!("/api/works/{slug}/raw/pages/0"),
        format!("/api/works/{slug}/raw/download"),
    ] {
        let (status, body) = get(path.clone()).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
    for path in [
        format!("/api/works/{slug}/en/pages/0"),
        format!("/api/works/{slug}/en/download"),
    ] {
        let (status, body) = get(path.clone()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert_eq!(body, format!("no work `{slug}`"), "{path}");
    }
}

/// Spec §7, "Access": a copy held under a directory of each of two libraries
/// is one member, and a reader granted the library whose directory does not
/// hold its primary copy is listed the work and served its page and download.
#[tokio::test]
async fn a_reader_granted_the_library_of_a_copys_second_directory_is_served_its_page_and_download()
{
    let (router, _state, _fixture, slug) =
        crate::test_support::router_over_a_copy_granted_through_its_second_directory();
    let get = |path: String| {
        let router = router.clone();
        async move {
            let response = crate::test_support::get(&router, &path).await;
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap();
            (status, String::from_utf8_lossy(&bytes).into_owned())
        }
    };

    let (status, listed) = get("/api/works".to_string()).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let listed: serde_json::Value = serde_json::from_str(&listed).expect("json");
    assert_eq!(
        listed
            .as_array()
            .expect("a list")
            .iter()
            .map(|w| (w["slug"].clone(), w["editions"].clone()))
            .collect::<Vec<_>>(),
        [(serde_json::json!(slug), serde_json::json!(["raw"]))],
        "the copy's work is listed"
    );
    for path in [
        format!("/api/works/{slug}/raw/pages/0"),
        format!("/api/works/{slug}/raw/download"),
    ] {
        let (status, body) = get(path.clone()).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
}

/// A reader granted the library `Shelf` is listed the work under each of its
/// two directories and may open each and ask for its pages. The work under
/// `Other` is not listed, and its own address and its pages answer as a work
/// that does not exist.
#[tokio::test]
async fn the_work_list_names_every_dir_of_a_granted_library() {
    let tmp = crate::test_support::Fixture::new("routes-granted-library");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let slug_of = |title: &str| -> String {
        state
            .library
            .conn()
            .query_row("SELECT slug FROM work WHERE title = ?1", [title], |r| {
                r.get(0)
            })
            .expect("the work's slug")
    };

    let response = crate::test_support::get(&router, "/api/works").await;
    let status = response.status();
    let listed = crate::test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        !listed.contains("Other Library Work"),
        "a work of a library that was not granted is listed: {listed}"
    );

    for title in ["First Directory Work", "Second Directory Work"] {
        assert!(
            listed.contains(title),
            "a work under a directory of the granted library is missing: {title}: {listed}"
        );
        let slug = slug_of(title);
        let work = crate::test_support::get(&router, &format!("/api/works/{slug}")).await;
        assert_eq!(work.status(), StatusCode::OK, "{title}");
        // The fixture's member files are not on disk, so a page is a 404
        // either way. The grant's refusal names the work and nothing else.
        let page =
            crate::test_support::get(&router, &format!("/api/works/{slug}/raw/pages/0")).await;
        assert_ne!(
            crate::test_support::body_string(page).await,
            format!("no work `{slug}`"),
            "{title}: its pages were refused by the grant"
        );
    }

    let other = slug_of("Other Library Work");
    for path in [
        format!("/api/works/{other}"),
        format!("/api/works/{other}/raw/pages/0"),
    ] {
        let response = crate::test_support::get(&router, &path).await;
        let status = response.status();
        let body = crate::test_support::body_string(response).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert_eq!(body, format!("no work `{other}`"), "{path}");
    }
}

// ---------------------------------------------------------------------------
// A missing work (O91): `/api/works` leaves it out for every account, and the
// reader's read of it says why it will not open, to an account that may see it.
// ---------------------------------------------------------------------------

/// The slugs `/api/works` lists.
async fn listed_slugs(router: &axum::Router) -> Vec<String> {
    crate::test_support::get_json(router, "/api/works")
        .await
        .as_array()
        .expect("a list")
        .iter()
        .map(|work| work["slug"].as_str().expect("a slug").to_string())
        .collect()
}

/// An account that sees every library is not listed a missing work, which
/// `browse::visible_editions` never filters for it: only the registry's mark
/// can.
#[tokio::test]
async fn the_work_list_leaves_a_missing_work_out_for_an_account_that_sees_every_library() {
    let (router, state, _fixture, [present, missing, _]) =
        crate::test_support::router_over_a_live_library_of_two_directories();
    crate::test_support::mark_missing(&state, &missing);

    let listed = listed_slugs(&router).await;
    assert!(listed.contains(&present), "the present work: {listed:?}");
    assert!(
        !listed.contains(&missing),
        "a missing work is listed to an account that sees every library: {listed:?}"
    );
}

/// A reader granted the directory a missing work is under is not listed it
/// either.
#[tokio::test]
async fn the_work_list_leaves_a_missing_work_out_under_a_grant() {
    let (router, state, _fixture, [present, missing, withheld]) =
        crate::test_support::router_over_a_live_library_of_two_directories();
    crate::test_support::grant_the_scanned_directory(&state);
    crate::test_support::mark_missing(&state, &missing);

    let listed = listed_slugs(&router).await;
    assert!(listed.contains(&present), "the present work: {listed:?}");
    assert!(!listed.contains(&withheld), "the grant holds: {listed:?}");
    assert!(
        !listed.contains(&missing),
        "a missing work is listed under a grant that reaches it: {listed:?}"
    );
}

/// `GET /api/works/{work}` of a missing work answers 404 with JSON saying its
/// files are missing, to an administrator and to a reader whose grant reaches
/// it, and to one revalidating the copy it read while the work was present,
/// whose tag the mark leaves unchanged. A work the grant hides answers as one
/// that does not exist, missing or not, so the reason never tells a reader of
/// a work outside their grant.
#[tokio::test]
async fn the_reader_s_read_of_a_missing_work_says_its_files_are_missing() {
    let (router, state, _fixture, [present, missing, withheld]) =
        crate::test_support::router_over_a_live_library_of_two_directories();
    let uri = format!("/api/works/{missing}");
    let fresh = crate::test_support::get(&router, &uri).await;
    assert_eq!(
        fresh.status(),
        StatusCode::OK,
        "the fixture: present at first"
    );
    let tag = fresh.headers()[header::ETAG].clone();
    crate::test_support::mark_missing(&state, &missing);
    crate::test_support::mark_missing(&state, &withheld);
    let read = |slug: String| {
        let router = router.clone();
        async move {
            let response = crate::test_support::get(&router, &format!("/api/works/{slug}")).await;
            let status = response.status();
            let kind = response
                .headers()
                .get(header::CONTENT_TYPE)
                .map(|value| value.to_str().expect("ascii").to_string());
            (
                status,
                kind,
                crate::test_support::body_string(response).await,
            )
        }
    };
    let says_missing = |slug: &str| {
        serde_json::json!({
            "error": format!(
                "the files of work `{slug}` are missing from every directory that held them; \
                 it opens again once a scan finds them"
            ),
            "missing": true,
        })
    };

    let (status, kind, body) = read(missing.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an administrator: {body}");
    assert_eq!(kind.as_deref(), Some("application/json"), "{body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("a JSON body"),
        says_missing(&missing),
        "an administrator"
    );
    let (status, _, body) = read(present.clone()).await;
    assert_eq!(status, StatusCode::OK, "the present work: {body}");
    let revalidated = router
        .clone()
        .oneshot(
            Request::get(&uri)
                .header(header::IF_NONE_MATCH, tag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = revalidated.status();
    let body = crate::test_support::body_string(revalidated).await;
    assert_eq!(
        (
            status,
            serde_json::from_str::<serde_json::Value>(&body).ok()
        ),
        (StatusCode::NOT_FOUND, Some(says_missing(&missing))),
        "revalidating the copy read while it was present: {body}"
    );

    crate::test_support::grant_the_scanned_directory(&state);
    let (status, _, body) = read(missing.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a granted reader: {body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("a JSON body"),
        says_missing(&missing),
        "a reader whose grant reaches the work"
    );
    let (status, _, body) = read(withheld.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body,
        format!("no work `{withheld}`"),
        "a work the grant hides answers as one that does not exist, missing or not"
    );
}

// ---------------------------------------------------------------------------
// A restricted account's correction: shared `alignment`/`run` rows are an
// administrator's to write (O58), whatever the account's grant holds.
// ---------------------------------------------------------------------------

/// A router over a work with two editions and a solved alignment edge
/// between them — `test_support::router_over_a_scanned_work`'s exact shape,
/// since a correction needs an edge to address — but filed entirely under a
/// root the fixture account is **not** granted, mirroring
/// `opds::tests::router_with_a_real_page_under_a_withheld_root`'s root split.
fn router_over_a_scanned_work_under_a_withheld_root(
    dir: &std::path::Path,
) -> (axum::Router, Arc<ServeState>) {
    let fixture_root = dir;
    let shelf = fixture_root.to_string_lossy().into_owned();

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Withheld", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/raw/c1.cbz", "sha256": "x", "pages": 2 }},
              {{ "path": "{shelf}/raw/c2.cbz", "sha256": "y", "pages": 2 }}
            ], "spreads": [] }},
            "en": {{ "label": "En", "members": [
              {{ "path": "{shelf}/en/d1.cbz", "sha256": "p", "pages": 2 }},
              {{ "path": "{shelf}/en/d2.cbz", "sha256": "q", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": [
            {{ "from": "raw", "to": "en", "by": "solver", "runs": [{{ "a": "0-3", "b": "0-3" }}] }}
          ]
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        let work_id =
            taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");

        // A scan cannot know the edge; write the rows a solve would leave,
        // the same way `test_support::router_over_a_scanned_work` does —
        // otherwise the correction below answers 404 `NoPath` and this
        // fixture cannot exercise the write at all.
        conn.execute(
            "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, 'raw', 'en', 'solver')",
            [work_id],
        )
        .expect("writes the solved edge");
        let alignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO run (alignment_id, ordinal, a_start_page, a_end_page, b_start_page, b_end_page)
             VALUES (?1, 0, 0, 3, 0, 3)",
            [alignment_id],
        )
        .expect("writes the run");

        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld')",
            [],
        )
        .expect("two roots");
        conn.execute("UPDATE member SET root_id = 2", [])
            .expect("files every member under the withheld root");
        crate::test_support::grant_root(&conn, 1);
    }
    let works = crate::works::from_library(&library).expect("projects the imported work");
    let state = Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: fixture_root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = build_router(state.clone());
    (router, state)
}

/// A restricted account's correction to a work outside every root it holds
/// is refused by the administrator's gate (O58) and writes no shared
/// `alignment`/`run` rows — library truth every other reader sees.
///
/// The rows check is a backstop: gate or no gate, the grant refuses this
/// account before any write, so only the status and body tell the gate's
/// refusal from the grant's.
#[tokio::test]
async fn a_restricted_account_cannot_patch_alignments_on_a_work_in_a_withheld_root() {
    let tmp = crate::test_support::Fixture::new("routes-grant-alignments-write");
    let (router, state) = router_over_a_scanned_work_under_a_withheld_root(&tmp);
    let slug = state.works.snapshot()[0].id.clone();
    // Computed directly from the state the running server holds, rather than
    // fetched through `GET /api/works/{slug}` — that route would itself answer
    // 404 to this restricted account, which is exactly what would make an
    // attacker compute this offline instead.
    let tag = super::sidecar_etag(&state.works.snapshot()[0].sidecar());

    let (status, body) = correct_with_body(
        &router,
        &slug,
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1","note":"leaked"}"#,
    )
    .await;

    let hand_writes: i64 = {
        let conn = state.library.conn();
        conn.query_row("SELECT COUNT(*) FROM run WHERE by = 'hand'", [], |row| {
            row.get(0)
        })
        .expect("counting hand-corrected runs")
    };

    assert_eq!(
        (status, body.as_str()),
        (StatusCode::FORBIDDEN, "this route is an administrator's"),
        "a restricted account must not be able to correct a work outside every \
         root it holds"
    );
    assert_eq!(
        hand_writes, 0,
        "no correction may reach the shared rows for a work outside the grant, \
         whatever the response said"
    );
}

/// O134: the rule `refuse_if_cannot_stream` refuses on, read as a question.
///
/// **No `Principal` means yes**, and that is the branch this pins: a server
/// configured without auth puts none in the extensions and every route is open
/// there. `get_thumb` asks this before honouring a `?w=`, so a `false` here
/// would drop every tile in that configuration to 160 px silently — no test
/// that signs in can reach it, and a soft tile looks like a tile.
#[test]
fn can_stream_is_true_when_no_principal_is_present_and_follows_the_flag_when_one_is() {
    let empty = axum::http::Extensions::new();
    assert!(can_stream(&empty), "no principal at all may stream");

    let mut may = axum::http::Extensions::new();
    may.insert(crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::User,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access: Access::Every,
    });
    assert!(can_stream(&may));

    let mut may_not = axum::http::Extensions::new();
    may_not.insert(crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::User,
        can_download: true,
        can_stream: false,
        can_stamp: false,
        access: Access::Every,
    });
    assert!(!can_stream(&may_not));
    assert!(
        refuse_if_cannot_stream(&may_not).is_some(),
        "and the refusal still refuses on exactly that"
    );
}

/// The account `refuse_if_cannot_stream` refuses: may browse and download, not
/// read pages here. User 1 is made an ordinary user first, because a capability
/// is withheld from a user, never from an admin.
fn without_can_stream(state: &ServeState) {
    let conn = state.library.conn();
    conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
        .expect("user 1 is an ordinary user");
    conn.execute("UPDATE user SET can_stream = 0", [])
        .expect("withholds can_stream");
}

/// An image's long edge, in pixels.
fn long_edge_of(body: &[u8]) -> u32 {
    let image = image::load_from_memory(body).expect("a decodable image");
    image.width().max(image.height())
}

/// O133: a library cover is page 0 at 256, 512 or 768 px, and it must reach
/// an account that may download but not read here — `refuse_if_cannot_stream`'s
/// doc says why a download-only account still sees its shelf. A cover drawn
/// through the page route, which refuses that account, would leave every tile
/// on its shelf a placeholder. The page route itself refuses it, and that half
/// is asserted too.
#[tokio::test]
async fn a_download_only_account_sees_covers_at_every_size_and_still_no_pages() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    without_can_stream(&state);
    let app = crate::test_support::authenticating(build_router(state.clone()), &state.library);

    for edge in crate::cover::COVER_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/0/thumb?w={edge}");
        let (status, headers, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(long_edge_of(&body), edge, "{uri}: page 0 at {edge} px");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).expect("a content type"),
            "image/jpeg",
            "{uri}"
        );
    }

    let (status, _, _) = fetch(&app, "/api/works/w/a/pages/0?w=256", None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the page route still refuses to show this account a page"
    );
}

/// A `w` may also be a tile rung (O134), on any page, for a caller that may
/// stream. This test's account may not — `without_can_stream` — and none of
/// these sizes is a rung anyway, so every one of them falls through to the
/// plain tile: page 0, at one of `cover::COVER_LONG_EDGES`, is the only sized
/// answer this account gets.
#[tokio::test]
async fn the_tile_route_sizes_page_zero_to_a_cover_size_and_refuses_the_rest_a_size() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    without_can_stream(&state);
    let app = crate::test_support::authenticating(build_router(state.clone()), &state.library);

    for uri in [
        "/api/works/w/a/pages/1/thumb?w=768",
        "/api/works/w/a/pages/0/thumb?w=700",
        "/api/works/w/a/pages/0/thumb?w=1000",
        "/api/works/w/a/pages/0/thumb?w=abc",
        "/api/works/w/a/pages/0/thumb",
    ] {
        let (status, _, body) = fetch(&app, uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(long_edge_of(&body), 160, "{uri} is the filmstrip tile");
    }
}

/// O134: any page, at any rung of `cover::TILE_LONG_EDGES`, for a caller that
/// may stream — this fixture's router has no `Principal`, which `can_stream`
/// answers `true` for.
///
/// **Both loops, and page 1 first**, because the two are different claims.
/// Page 1 is the plain one: a page `cover_edge` never touches, sized by
/// `tile_edge` alone. Page 0 is the composition — it is the one page that is
/// both a cover and a tile, and a rung there must still reach `tile_edge`
/// rather than the cover branch that runs ahead of it.
///
/// The pages are 600×900 (`test_support::state_with_tall_pages`), so every
/// rung is a real downward resize and the served image says which. **On page 0
/// the size and the content type cannot tell the two branches apart**: the
/// fixture is one flat colour, where the tile's fast filter and `serve_page`'s
/// quality filter encode byte-identically at the same target. The `ETag` is
/// what separates them — `serve_page` always sends one and a tile never does
/// (`get_thumb`'s own doc says why) — so that assertion is the one carrying
/// page 0's half of this test, and the other two are its symmetry with page 1.
#[tokio::test]
async fn the_tile_route_sizes_any_page_to_a_tile_rung() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);

    for edge in crate::cover::TILE_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/1/thumb?w={edge}");
        let (status, headers, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(long_edge_of(&body), edge, "{uri}: page 1 at {edge} px");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).expect("a content type"),
            "image/jpeg",
            "{uri}: a tile is always JPEG"
        );
    }

    // Page 0 too, at a tile rung: this test's name says "any page", and page 0
    // is the one page `cover_edge` also looks at. `cover_edge` matches page 0
    // only at a `cover::COVER_LONG_EDGES` size, and those are disjoint from
    // `cover::TILE_LONG_EDGES` (asserted at compile time beside the latter),
    // so a tile rung here still falls through to `tile_edge` — a rung on page
    // 0 is a tile, not a cover.
    //
    // The `ETag` check is load-bearing, not decorative: on this fixture a
    // page 0 wrongly routed through `cover_edge` comes back at the same long
    // edge and the same `image/jpeg` content type as the tile does — both
    // resize a same-color page to the same box and encode it the same way,
    // so those two checks alone pass whichever branch served it. `get_thumb`'s
    // doc says the tile sends no `ETag`; `serve_page`, which is what
    // `cover_edge` reaches, always sends one. That header is what actually
    // tells the two branches apart here.
    for edge in crate::cover::TILE_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/0/thumb?w={edge}");
        let (status, headers, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(long_edge_of(&body), edge, "{uri}: page 0 at {edge} px");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).expect("a content type"),
            "image/jpeg",
            "{uri}: a tile is always JPEG, page 0 included"
        );
        assert!(
            headers.get(header::ETAG).is_none(),
            "{uri}: a tile carries no ETag; an ETag here means page 0 was served as a cover"
        );
    }
}

/// O134's line: a sharper tile is for a reader, and an account that may browse
/// and download but not read pages is not one.
///
/// It is answered with the 160 px tile rather than a `403` — the same fallback
/// `cover_edge` makes for a `w` this server does not recognise — so such an
/// account's strip is soft rather than broken. It costs that account nothing
/// real: the page route refuses it every page, asserted below, so there is
/// nothing behind the strip to be sharp about.
#[tokio::test]
async fn a_download_only_account_gets_the_unsized_tile_whatever_rung_it_asks_for() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    without_can_stream(&state);
    let app = crate::test_support::authenticating(build_router(state.clone()), &state.library);

    for edge in crate::cover::TILE_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/1/thumb?w={edge}");
        let (status, _, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(
            long_edge_of(&body),
            crate::cover::THUMB_LONG_EDGE,
            "{uri}: the tile, not the rung"
        );
    }

    let (status, _, _) = fetch(&app, "/api/works/w/a/pages/1?w=480", None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the page route still refuses this account a page"
    );
}

/// O134's fallback, for a caller that *may* stream: a `w` this server makes no
/// tile at is the 160 px tile — not a `400`, and not a resize to whatever was
/// asked for.
///
/// **The neighbouring download-only test cannot stand in for this one.** There
/// `can_stream` short-circuits the whole branch before `tile_edge`'s answer
/// matters, so it would pass against a `tile_edge` that accepted every number.
/// Here nothing short-circuits, so it is `tile_edge` itself being pinned —
/// including that a *cover* size is not a tile size on a page that is not
/// page 0.
#[tokio::test]
async fn a_w_that_is_not_a_tile_rung_is_the_tile() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);

    for uri in [
        "/api/works/w/a/pages/1/thumb?w=768",
        "/api/works/w/a/pages/1/thumb?w=512",
        "/api/works/w/a/pages/1/thumb?w=321",
        "/api/works/w/a/pages/1/thumb?w=abc",
        "/api/works/w/a/pages/1/thumb?w=",
        "/api/works/w/a/pages/1/thumb",
    ] {
        let (status, _, body) = fetch(&app, uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(
            long_edge_of(&body),
            crate::cover::THUMB_LONG_EDGE,
            "{uri}: not a rung, so the tile"
        );
    }
}

/// A sized cover carries the page route's validator, so a grid visited again
/// answers `304` for each cover rather than sending them all again.
#[tokio::test]
async fn a_sized_cover_answers_its_own_validator_with_a_304() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);

    let uri = "/api/works/w/a/pages/0/thumb?w=512";
    let (status, headers, _) = fetch(&app, uri, None).await;
    assert_eq!(status, StatusCode::OK);
    let tag = headers
        .get(header::ETAG)
        .expect("a sized cover carries an ETag")
        .to_str()
        .expect("ascii")
        .to_owned();
    let (again, _, body) = fetch(&app, uri, Some(&tag)).await;
    assert_eq!(again, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty(), "a 304 sends no body");

    // A `w` given twice is read, not refused: the first one decides.
    let (status, _, body) = fetch(&app, "/api/works/w/a/pages/0/thumb?w=768&w=256", None).await;
    assert_eq!(status, StatusCode::OK);
    let decoded = image::load_from_memory(&body).expect("a decodable image");
    assert_eq!(
        decoded.width().max(decoded.height()),
        768,
        "the first `w` decides the size"
    );
}

/// An answer's `Cache-Control`, or `""` when it has none.
fn cache_control(headers: &axum::http::HeaderMap) -> &str {
    headers
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

/// That `policy` keeps an answer for good in the reader's browser alone: what
/// `COVER_CACHE_CONTROL` must say, checked as directives rather than against
/// the constant, which a comparison with itself can never catch changing.
/// `private` because a cover is answered only to an account whose grants show
/// it, so a shared cache must not hold it (spec
/// `2026-10-28-loading-library-design.md` §4.5).
fn assert_kept_for_good_privately(policy: &str, what: &str) {
    let directives: Vec<&str> = policy.split(',').map(str::trim).collect();
    for present in ["private", "immutable", "max-age=31536000"] {
        assert!(
            directives.contains(&present),
            "{what}: `{present}` missing from {policy:?}"
        );
    }
    for absent in ["public", "no-cache", "no-store"] {
        assert!(
            !directives.contains(&absent),
            "{what}: `{absent}` in {policy:?}"
        );
    }
}

/// A cover address whose `?v=` names the bytes it serves is kept for good, at
/// every cover size, with the `ETag` and the bytes the same address sends
/// without a `v`.
///
/// **The `304` too** (O1): RFC 9111 §4.3.4 has the client take a
/// `304`'s headers into its stored answer, so a `304` sending
/// `PAGE_CACHE_CONTROL` would put a kept cover back to asking on every visit.
/// The addresses are in the shape the lists carry, `v` first and the client's
/// `w` after it. The header is compared with `COVER_CACHE_CONTROL` and its
/// directives are checked too, since the comparison alone passes whatever the
/// constant says.
#[tokio::test]
async fn a_cover_whose_version_matches_is_kept_for_good_on_the_200_and_the_304() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);
    let v = crate::cover::cover_version("tall", 0);

    for edge in crate::cover::COVER_LONG_EDGES {
        let (_, plain_headers, plain_body) = fetch(
            &app,
            &format!("/api/works/w/a/pages/0/thumb?w={edge}"),
            None,
        )
        .await;
        let uri = format!("/api/works/w/a/pages/0/thumb?v={v}&w={edge}");
        let (status, headers, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(cache_control(&headers), COVER_CACHE_CONTROL, "{uri}");
        assert_kept_for_good_privately(cache_control(&headers), &uri);
        let tag = headers
            .get(header::ETAG)
            .expect("a cover carries an ETag")
            .to_str()
            .expect("ascii")
            .to_owned();
        assert_eq!(
            Some(tag.as_str()),
            plain_headers
                .get(header::ETAG)
                .and_then(|value| value.to_str().ok()),
            "{uri}: the ETag the address sends without a v"
        );
        assert_eq!(body, plain_body, "{uri}: the same bytes");
        assert_eq!(long_edge_of(&body), edge, "{uri}: page 0 at {edge} px");

        let (again, headers, body) = fetch(&app, &uri, Some(&tag)).await;
        assert_eq!(again, StatusCode::NOT_MODIFIED, "{uri}");
        assert!(body.is_empty(), "{uri}: a 304 sends no body");
        assert_eq!(
            cache_control(&headers),
            COVER_CACHE_CONTROL,
            "{uri}: the 304's headers replace the kept answer's"
        );
        assert_kept_for_good_privately(cache_control(&headers), &format!("{uri}, the 304"));
        assert_eq!(headers[header::ETAG], tag.as_str(), "{uri}");
    }
}

/// A `v` that does not name what the route serves (the image changed after the
/// list that carried the address was read) is answered with the current image
/// under `PAGE_CACHE_CONTROL`, on the `200` and on the `304`, so no address is
/// kept for good over bytes other than the ones it names.
///
/// Another member's version, this member's at another offset, the right one
/// with more after it, an empty one and one that is not a version at all; and
/// the right one given second, since the first `v` decides as the first `w`
/// does.
#[tokio::test]
async fn a_cover_whose_version_does_not_match_is_the_current_image_revalidated() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);
    let (_, plain_headers, plain_body) =
        fetch(&app, "/api/works/w/a/pages/0/thumb?w=512", None).await;
    let current = plain_headers[header::ETAG]
        .to_str()
        .expect("ascii")
        .to_owned();
    let v = crate::cover::cover_version("tall", 0);
    let stale = crate::cover::cover_version(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        0,
    );
    let other_offset = crate::cover::cover_version("tall", 1);

    for query in [
        format!("v={stale}"),
        format!("v={other_offset}"),
        format!("v={v}0"),
        "v=".to_owned(),
        "v=abc".to_owned(),
        format!("v={stale}&v={v}"),
    ] {
        let uri = format!("/api/works/w/a/pages/0/thumb?{query}&w=512");
        let (status, headers, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(cache_control(&headers), PAGE_CACHE_CONTROL, "{uri}");
        assert_eq!(headers[header::ETAG], current.as_str(), "{uri}");
        assert_eq!(body, plain_body, "{uri}: the current image");

        let (again, headers, _) = fetch(&app, &uri, Some(&current)).await;
        assert_eq!(again, StatusCode::NOT_MODIFIED, "{uri}");
        assert_eq!(
            cache_control(&headers),
            PAGE_CACHE_CONTROL,
            "{uri}: a 304 must not make the address kept for good either"
        );
    }
}

/// With no `v`, a cover is revalidated as every page is: any cover address
/// without a version, the `307`'s target at a cover size among them, is sent
/// `PAGE_CACHE_CONTROL` on both.
#[tokio::test]
async fn a_cover_without_a_version_is_revalidated_on_the_200_and_the_304() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);

    for edge in crate::cover::COVER_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/0/thumb?w={edge}");
        let (status, headers, _) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(cache_control(&headers), PAGE_CACHE_CONTROL, "{uri}");
        let tag = headers[header::ETAG].to_str().expect("ascii").to_owned();

        let (again, headers, _) = fetch(&app, &uri, Some(&tag)).await;
        assert_eq!(again, StatusCode::NOT_MODIFIED, "{uri}");
        assert_eq!(cache_control(&headers), PAGE_CACHE_CONTROL, "{uri}");
    }
}

/// `v` is a cover's alone: page 0 at one of `cover::COVER_LONG_EDGES` on the
/// tile route. Anywhere else it is ignored, and the answer is the one the same
/// address gives without it, headers and bytes alike.
///
/// Each `v` here is the one that address's own page would have, so a route
/// that honoured a matching `v` anywhere but a cover would be caught, not only
/// one that honoured any `v`.
#[tokio::test]
async fn a_version_anywhere_but_a_cover_is_ignored() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);
    let v0 = crate::cover::cover_version("tall", 0);
    let v1 = crate::cover::cover_version("tall", 1);

    for (plain, with_v) in [
        (
            "/api/works/w/a/pages/0/thumb",
            format!("/api/works/w/a/pages/0/thumb?v={v0}"),
        ),
        (
            "/api/works/w/a/pages/0/thumb?w=320",
            format!("/api/works/w/a/pages/0/thumb?v={v0}&w=320"),
        ),
        (
            "/api/works/w/a/pages/0/thumb?w=700",
            format!("/api/works/w/a/pages/0/thumb?v={v0}&w=700"),
        ),
        (
            "/api/works/w/a/pages/1/thumb?w=256",
            format!("/api/works/w/a/pages/1/thumb?v={v1}&w=256"),
        ),
        (
            "/api/works/w/a/pages/0?w=256",
            format!("/api/works/w/a/pages/0?v={v0}&w=256"),
        ),
        (
            "/api/works/w/a/pages/0",
            format!("/api/works/w/a/pages/0?v={v0}"),
        ),
    ] {
        let (plain_status, plain_headers, plain_body) = fetch(&app, plain, None).await;
        let (status, headers, body) = fetch(&app, &with_v, None).await;
        assert_eq!(status, StatusCode::OK, "{with_v}");
        assert_eq!(cache_control(&headers), PAGE_CACHE_CONTROL, "{with_v}");
        assert_eq!(status, plain_status, "{with_v}");
        assert_eq!(headers, plain_headers, "{with_v}: the answer {plain} gives");
        assert_eq!(body, plain_body, "{with_v}: the bytes {plain} gives");
    }
}

/// O133: an account that may download but not read here sees its shelf's
/// covers, and with a matching `v` keeps them as any account does, at every
/// cover size. The page route still refuses it with a `v`, as it does without
/// one (`a_download_only_account_sees_covers_at_every_size_and_still_no_pages`).
#[tokio::test]
async fn a_download_only_account_keeps_its_covers_with_a_matching_version_and_still_no_pages() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    without_can_stream(&state);
    let app = crate::test_support::authenticating(build_router(state.clone()), &state.library);
    let v = crate::cover::cover_version("tall", 0);

    for edge in crate::cover::COVER_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/0/thumb?v={v}&w={edge}");
        let (status, headers, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(long_edge_of(&body), edge, "{uri}: page 0 at {edge} px");
        assert_eq!(cache_control(&headers), COVER_CACHE_CONTROL, "{uri}");
    }

    let (status, _, _) = fetch(&app, &format!("/api/works/w/a/pages/0?v={v}&w=256"), None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the page route still refuses to show this account a page"
    );
}

// ---------------------------------------------------------------------------
// O60: progress, notes and truth ask the grant. A work outside it is answered
// on those routes as a slug no work holds is.
// ---------------------------------------------------------------------------

/// One request through a fixture router, answered as its status and its body.
///
/// `if_match` is what a document write sends and `body` what any write does;
/// a plain read sends neither. The answer's body is read as text whatever it
/// holds, so a route that answers bytes can be asked too.
async fn answered(
    router: &axum::Router,
    method: axum::http::Method,
    uri: &str,
    if_match: Option<&str>,
    body: Option<&str>,
) -> (StatusCode, String) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(tag) = if_match {
        request = request.header(header::IF_MATCH, tag);
    }
    let body = match body {
        Some(text) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(text.to_string())
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// One of the five requests [`the_five_requests`] builds.
struct Asked {
    handler: &'static str,
    method: axum::http::Method,
    uri: String,
    if_match: Option<String>,
    body: Option<String>,
    /// What a caller who may see the work is answered.
    seen: StatusCode,
}

/// The requests O60 names that an account a grant limits may make, against
/// `slug`, in an order each can succeed in. Not the truth write: O58 makes it
/// an administrator's, and no grant limits an administrator.
///
/// The tag is the one a work nobody has written on answers with. Every
/// request here is one the server carries out, so each write carries the tag
/// it needs; the writes that fail a check of their own, a missing or stale tag
/// among them, are sent by
/// [`a_write_to_a_hidden_work_is_refused_before_its_own_checks_are_made`].
fn the_five_requests(slug: &str) -> Vec<Asked> {
    use axum::http::Method;

    let notes_tag = notes_etag(&crate::notes::to_json(&crate::notes::empty(slug)).unwrap());
    let note = format!(
        r#"{{"work":"{slug}","notes":[{{"id":"01J8","edition":"raw","slot":1,"body":"kept",
               "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}]}}"#
    );
    vec![
        Asked {
            handler: "put_progress",
            method: Method::PUT,
            uri: format!("/api/works/{slug}/progress"),
            if_match: None,
            body: Some(r#"{"edition":"raw","slot":1}"#.to_string()),
            seen: StatusCode::NO_CONTENT,
        },
        Asked {
            handler: "get_progress",
            method: Method::GET,
            uri: format!("/api/works/{slug}/progress"),
            if_match: None,
            body: None,
            seen: StatusCode::OK,
        },
        Asked {
            handler: "get_notes",
            method: Method::GET,
            uri: format!("/api/works/{slug}/notes"),
            if_match: None,
            body: None,
            seen: StatusCode::OK,
        },
        Asked {
            handler: "put_notes",
            method: Method::PUT,
            uri: format!("/api/works/{slug}/notes"),
            if_match: Some(notes_tag),
            body: Some(note),
            seen: StatusCode::OK,
        },
        Asked {
            handler: "get_truth",
            method: Method::GET,
            uri: format!("/api/works/{slug}/truth"),
            if_match: None,
            body: None,
            seen: StatusCode::OK,
        },
    ]
}

/// **O60: a work outside the grant is a slug no work holds**, on the five
/// handlers that read and write progress and notes and read truth.
///
/// Each request is sent twice by one account, granted `Shelf` alone: against
/// the work under `Other`, and against a slug no work holds. The two answers
/// are compared with each other, status and body, with the slug each names
/// put aside. Neither is compared with a sentence written here.
///
/// **Then the grant is lifted and the same five are sent again**, against the
/// same work, and each is carried out. That half is what shows the requests
/// are ones this server accepts: five malformed requests would be refused
/// alike for any slug. Between the two halves the two documents written are
/// read, and nothing a refused write carried is in them.
#[tokio::test]
async fn progress_notes_and_truth_answer_a_hidden_work_as_a_slug_no_work_holds() {
    use axum::http::Method;

    let tmp = crate::test_support::Fixture::new("o60-hidden-work");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let hidden: String = state
        .library
        .conn()
        .query_row(
            "SELECT slug FROM work WHERE title = 'Other Library Work'",
            [],
            |row| row.get(0),
        )
        .expect("the slug of the work under `Other`");
    let unknown = "no-work-is-called-this";

    for (asked, nowhere) in the_five_requests(&hidden)
        .into_iter()
        .zip(the_five_requests(unknown))
    {
        let (status, body) = answered(
            &router,
            asked.method.clone(),
            &asked.uri,
            asked.if_match.as_deref(),
            asked.body.as_deref(),
        )
        .await;
        let (no_status, no_body) = answered(
            &router,
            nowhere.method.clone(),
            &nowhere.uri,
            nowhere.if_match.as_deref(),
            nowhere.body.as_deref(),
        )
        .await;
        assert_eq!(
            (status, body.replace(&hidden, "{work}")),
            (no_status, no_body.replace(unknown, "{work}")),
            "{}: a work outside the grant, left, and a slug no work holds, right",
            asked.handler
        );
        assert_eq!(status, StatusCode::NOT_FOUND, "{}: {body}", asked.handler);
    }

    let lifted = taiyaku_library::account::set_every_library(&state.library.conn(), 1)
        .expect("lifts the grant");
    assert!(lifted, "the account sees every library from here");

    let read = |document: &str| format!("/api/works/{hidden}/{document}");
    let (status, body) = answered(&router, Method::GET, &read("progress"), None, None).await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "the refused position write reached the store: {body}"
    );
    let (status, notes) = answered(&router, Method::GET, &read("notes"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{notes}");
    assert!(
        !notes.contains("01J8"),
        "the refused note reached the store: {notes}"
    );

    for asked in the_five_requests(&hidden) {
        let (status, body) = answered(
            &router,
            asked.method.clone(),
            &asked.uri,
            asked.if_match.as_deref(),
            asked.body.as_deref(),
        )
        .await;
        assert_eq!(
            status, asked.seen,
            "{}, for an account with no limits: {body}",
            asked.handler
        );
    }
}

/// **Every guarded route that names a work answers a hidden one as it answers
/// a slug no work holds**, walked from `guarded_routes()` and not from a list
/// of routes somebody remembered.
///
/// Five methods on every path, each with no body and with `{}`. A
/// `MethodRouter` cannot be asked which methods it registers, and a method it
/// does not is answered 405 for any slug, which the comparison finds equal.
/// A body the handler will not parse is part of what O60 decided: it is
/// answered alike for both.
///
/// **And on each path but the correction's some request must tell a granted
/// work from a hidden one.** Without that a path could pass by never reaching
/// its handler, and the fixture would be one where right and wrong answer the
/// same. The granted work is asked last, so nothing it does can change what
/// the other two are answered.
///
/// The count is typed, as `auth::tests::every_route_refuses_an_anonymous_request`
/// types its list. A route added later is walked without being named here;
/// the count notices a path that stopped carrying `{work}`, and one that
/// started.
#[tokio::test]
async fn every_route_naming_a_work_answers_a_hidden_one_as_a_slug_no_work_holds() {
    use axum::http::Method;

    let tmp = crate::test_support::Fixture::new("o60-walk");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let slug_of = |title: &str| -> String {
        state
            .library
            .conn()
            .query_row("SELECT slug FROM work WHERE title = ?1", [title], |row| {
                row.get(0)
            })
            .expect("the work's slug")
    };
    let granted = slug_of("First Directory Work");
    let hidden = slug_of("Other Library Work");
    let unknown = "no-work-is-called-this".to_string();
    let concrete = |pattern: &str, work: &str| -> String {
        let path = pattern
            .replace("{work}", work)
            .replace("{edition}", "raw")
            .replace("{index}", "0")
            .replace("{ordinal}", "0")
            .replace("{number}", "0");
        assert!(
            !path.contains('{'),
            "{pattern} carries a placeholder this walk has no value for: {path}"
        );
        path
    };

    let mut walked = 0;
    for (pattern, _) in guarded_routes() {
        if !pattern.contains("{work}") {
            continue;
        }
        walked += 1;
        let mut told_apart = false;
        for method in [
            Method::GET,
            Method::PUT,
            Method::PATCH,
            Method::POST,
            Method::DELETE,
        ] {
            for body in [None, Some("{}")] {
                let mut said = Vec::new();
                for work in [&hidden, &unknown, &granted] {
                    let (status, text) = answered(
                        &router,
                        method.clone(),
                        &concrete(pattern, work),
                        None,
                        body,
                    )
                    .await;
                    said.push((status, text.replace(work.as_str(), "{work}")));
                }
                assert_eq!(
                    said[0], said[1],
                    "{method} {pattern}, body {body:?}: a work outside the grant, left, and a \
                     slug no work holds, right"
                );
                told_apart |= said[2] != said[0];
            }
        }
        // O58: the correction is an administrator's, and an administrator
        // holds no grant, so no request from this account reaches its grant.
        let asks_this_account_the_grant = pattern != "/api/works/{work}/alignments";
        assert!(
            told_apart || !asks_this_account_the_grant,
            "{pattern}: no request told a granted work from a hidden one, so this fixture \
             cannot show the route asks the grant at all"
        );
    }
    assert_eq!(
        walked, 13,
        "the guarded paths that carry `{{work}}`: a route that gains or loses one changes \
         this count"
    );
}

/// **Nothing stored is deleted** (spec 2026-10-11 §2.1). A position and a note
/// written while the account could see the work are refused it once the grant
/// is narrowed, are still in the store, and are served again when the grant
/// is widened.
///
/// Narrowed means granted `Other` instead of `Shelf`, so the account still
/// sees a library and is refused only what the grant leaves out.
#[tokio::test]
async fn a_position_and_a_note_outlive_the_grant_and_come_back_with_it() {
    use axum::http::Method;

    let tmp = crate::test_support::Fixture::new("o60-outlives-the-grant");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let slug: String = state
        .library
        .conn()
        .query_row(
            "SELECT slug FROM work WHERE title = 'First Directory Work'",
            [],
            |row| row.get(0),
        )
        .expect("the slug of a work under `Shelf`");
    let progress = format!("/api/works/{slug}/progress");
    let notes = format!("/api/works/{slug}/notes");
    let held = |table: &str| -> i64 {
        state
            .library
            .conn()
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE user_id = 1"),
                [],
                |row| row.get(0),
            )
            .expect("counts the account's rows")
    };

    let (status, body) = answered(
        &router,
        Method::PUT,
        &progress,
        None,
        Some(r#"{"edition":"raw","slot":1}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let tag = etag_of(&router, &notes).await;
    let note = format!(
        r#"{{"work":"{slug}","notes":[{{"id":"01J8","edition":"raw","slot":1,"body":"kept",
               "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}]}}"#
    );
    let (status, body) = answered(&router, Method::PUT, &notes, Some(&tag), Some(&note)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    {
        let conn = state.library.conn();
        conn.execute("DELETE FROM user_library WHERE user_id = 1", [])
            .expect("takes `Shelf` away");
        crate::test_support::grant_root(&conn, 3);
    }
    for uri in [&progress, &notes] {
        let (status, body) = answered(&router, Method::GET, uri, None, None).await;
        assert_eq!(
            (status, body),
            (StatusCode::NOT_FOUND, format!("no work `{slug}`")),
            "GET {uri} while the grant is narrowed"
        );
    }
    assert_eq!(held("progress"), 1, "the position is still in the store");
    assert_eq!(held("note"), 1, "the note is still in the store");

    crate::test_support::grant_root(&state.library.conn(), 1);
    let (status, body) = answered(&router, Method::GET, &progress, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let doc: serde_json::Value = serde_json::from_str(&body).expect("a progress document");
    assert_eq!(
        doc["positions"][0]["edition"].as_str(),
        Some("raw"),
        "{body}"
    );
    assert_eq!(doc["positions"][0]["slot"].as_u64(), Some(1), "{body}");
    let (status, body) = answered(&router, Method::GET, &notes, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("kept"), "the note came back: {body}");
}

/// **A document that does not parse is refused as that, whatever work it
/// names**: a notes write to a granted work, a hidden one and a slug no work
/// holds is answered 400 in the same words each time.
///
/// The notes write parses its body before it looks the work up, where the
/// grant is asked, and a notes body lends the grant nothing. The truth write
/// refuses this account before its body is read, alike for all three: it is an
/// administrator's (O58).
#[tokio::test]
async fn a_document_that_does_not_parse_is_refused_before_the_grant_is_asked() {
    use axum::http::Method;

    let tmp = crate::test_support::Fixture::new("o60-malformed");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let slug_of = |title: &str| -> String {
        state
            .library
            .conn()
            .query_row("SELECT slug FROM work WHERE title = ?1", [title], |row| {
                row.get(0)
            })
            .expect("the work's slug")
    };
    let granted = slug_of("First Directory Work");
    let hidden = slug_of("Other Library Work");
    let (status, body) = answered(
        &router,
        Method::GET,
        &format!("/api/works/{hidden}/notes"),
        None,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the fixture: `{hidden}` is hidden from the account: {body}"
    );

    for (document, refused) in [
        ("notes", StatusCode::BAD_REQUEST),
        ("truth", StatusCode::FORBIDDEN),
    ] {
        let mut said = Vec::new();
        for slug in [granted.as_str(), hidden.as_str(), "no-work-is-called-this"] {
            let (status, body) = answered(
                &router,
                Method::PUT,
                &format!("/api/works/{slug}/{document}"),
                None,
                Some("not a document"),
            )
            .await;
            assert_eq!(status, refused, "PUT {document} on `{slug}`: {body}");
            said.push(body);
        }
        assert!(
            said.iter().all(|body| body == &said[0]),
            "PUT {document}: the three refusals differ: {said:?}"
        );
        if document == "truth" {
            assert_eq!(
                said[0], "this route is an administrator's",
                "PUT truth is refused by the administrator's gate"
            );
        }
    }
}

/// **A position on an edition outside the grant is refused as one on an
/// edition the work does not have**: same status, same words, with the
/// edition each names put aside. The work has an edition in each of two
/// libraries, and the account is granted one: it sees `en` and not `raw`.
///
/// A position on `en` is stored, which shows the route is not refusing this
/// reader the work, and `en`'s is the only one the store then holds.
#[tokio::test]
async fn a_position_on_an_edition_outside_the_grant_is_refused_as_one_the_work_does_not_have() {
    use axum::http::Method;

    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_work_whose_editions_sit_in_two_libraries();
    let uri = format!("/api/works/{slug}/progress");
    let position = |edition: &str| format!(r#"{{"edition":"{edition}","slot":0}}"#);

    let (kept_status, kept) =
        answered(&router, Method::PUT, &uri, None, Some(&position("raw"))).await;
    let (absent_status, absent) =
        answered(&router, Method::PUT, &uri, None, Some(&position("nope"))).await;
    assert_eq!(
        (kept_status, kept.replace("`raw`", "`{edition}`")),
        (absent_status, absent.replace("`nope`", "`{edition}`")),
        "an edition outside the grant, left, and one the work does not have, right"
    );
    assert_eq!(kept_status, StatusCode::NOT_FOUND, "{kept}");

    let (status, body) = answered(&router, Method::PUT, &uri, None, Some(&position("en"))).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    let found = state.find_blocking(&slug).expect("the work");
    let held = crate::progress::load(&state.library, &found, 1, &found.sidecar())
        .expect("reads the positions");
    let editions: Vec<&str> = held.iter().map(|p| p.edition.as_str()).collect();
    assert_eq!(editions, ["en"], "the positions the store holds");
}

/// **A position read leaves out an edition outside the grant.** The position
/// on `raw` is written straight into the store, as one held from before the
/// grant was narrowed would be. Alone, it is answered 204, nothing recorded.
/// Beside one on `en`, only `en`'s is named.
#[tokio::test]
async fn a_position_read_leaves_out_an_edition_outside_the_grant() {
    use axum::http::Method;

    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_work_whose_editions_sit_in_two_libraries();
    let uri = format!("/api/works/{slug}/progress");
    let found = state.find_blocking(&slug).expect("the work");
    let hold = |edition: &str| {
        crate::progress::store(
            &state.library,
            &found,
            1,
            &crate::Progress {
                edition: edition.to_string(),
                slot: 1,
            },
            &found.sidecar(),
            taiyaku_library::state::Arrival::next(),
            crate::works::Mint::Bridge,
        )
        .expect("stores a position");
    };

    hold("raw");
    let (status, body) = answered(&router, Method::GET, &uri, None, None).await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "the one position held is on an edition outside the grant: {body}"
    );

    hold("en");
    let (status, body) = answered(&router, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let doc: serde_json::Value = serde_json::from_str(&body).expect("a progress document");
    let editions: Vec<&str> = doc["positions"]
        .as_array()
        .expect("positions is an array")
        .iter()
        .map(|p| p["edition"].as_str().expect("an edition key"))
        .collect();
    assert_eq!(editions, ["en"], "{body}");
}

/// **A new note on an edition outside the grant is refused as one on an
/// edition the work does not have**, and a note the reader already holds
/// there is neither sent nor lost (O191, spec §3.8).
///
/// The held note is written straight into the store, as one made before the
/// grant was narrowed would be. The read leaves it out, and a write of what
/// was read, with a note on `en` added, keeps it as it was. A client that read
/// it before it was left out sends it back unchanged, and that is kept too.
#[tokio::test]
async fn a_new_note_on_an_edition_outside_the_grant_is_refused_and_a_held_one_is_kept() {
    use axum::http::Method;

    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_work_whose_editions_sit_in_two_libraries();
    let uri = format!("/api/works/{slug}/notes");
    let note = |id: &str, edition: &str| {
        format!(
            r#"{{"id":"{id}","edition":"{edition}","slot":0,"body":"b",
                 "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}"#
        )
    };
    let doc = |notes: &[String]| format!(r#"{{"work":"{slug}","notes":[{}]}}"#, notes.join(","));

    let tag = etag_of(&router, &uri).await;
    let (kept_status, kept) = answered(
        &router,
        Method::PUT,
        &uri,
        Some(&tag),
        Some(&doc(&[note("01NEW", "raw")])),
    )
    .await;
    let (absent_status, absent) = answered(
        &router,
        Method::PUT,
        &uri,
        Some(&tag),
        Some(&doc(&[note("01NEW", "nope")])),
    )
    .await;
    assert_eq!(
        (kept_status, kept.replace("`raw`", "`{edition}`")),
        (absent_status, absent.replace("`nope`", "`{edition}`")),
        "an edition outside the grant, left, and one the work does not have, right"
    );
    assert_eq!(kept_status, StatusCode::NOT_FOUND, "{kept}");

    let found = state.find_blocking(&slug).expect("the work");
    crate::notes::store(
        &state.library,
        &found,
        1,
        &crate::notes::Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: slug.clone(),
            notes: vec![crate::notes::Note {
                id: "01HELD".into(),
                edition: "raw".into(),
                slot: 0,
                region: None,
                color: None,
                body: "b".into(),
                created: "2026-08-05T10:00:00Z".into(),
                updated: "2026-08-05T10:00:00Z".into(),
                stale: false,
                at: None,
            }],
        },
        &found.sidecar(),
        crate::works::Mint::Bridge,
    )
    .expect("stores the note held from before");

    let (status, body) = answered(&router, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sent: crate::notes::Notes = serde_json::from_str(&body).expect("a notes document");
    assert_eq!(
        sent.notes,
        [],
        "the note on the edition outside the grant is not sent: {body}"
    );
    let ids = || -> Vec<(String, String)> {
        crate::notes::load(&state.library, &found, 1, &found.sidecar())
            .expect("reads the notes")
            .expect("notes are held")
            .notes
            .into_iter()
            .map(|note| (note.id, note.edition))
            .collect()
    };
    let held = |id: &str, edition: &str| (id.to_string(), edition.to_string());

    let tag = etag_of(&router, &uri).await;
    let (status, body) = answered(
        &router,
        Method::PUT,
        &uri,
        Some(&tag),
        Some(&doc(&[note("01OWN", "en")])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(), [held("01HELD", "raw"), held("01OWN", "en")]);

    let tag = etag_of(&router, &uri).await;
    let (status, body) = answered(
        &router,
        Method::PUT,
        &uri,
        Some(&tag),
        Some(&doc(&[note("01HELD", "raw"), note("01OWN", "en")])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(), [held("01HELD", "raw"), held("01OWN", "en")]);
}

/// `notes` without the page each row names, which a note these tests build
/// carries none of.
fn unanchored(mut notes: crate::notes::Notes) -> crate::notes::Notes {
    for note in &mut notes.notes {
        note.at = None;
    }
    notes
}

/// **A notes write read before the account's grant widened is refused with
/// 412, and stores nothing** (spec §3.8, decision 11). Under the stored
/// document's tag alone it would pass `If-Match`, and `01HELD`, now seen and
/// not in the body, would be deleted. A read conditioned on the old tag is
/// sent the document, not 304.
#[tokio::test]
async fn a_notes_write_read_before_the_grant_widened_is_refused() {
    use axum::http::Method;

    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_work_whose_editions_sit_in_two_libraries();
    let uri = format!("/api/works/{slug}/notes");
    let note = |id: &str, edition: &str| crate::notes::Note {
        id: id.into(),
        edition: edition.into(),
        slot: 0,
        region: None,
        color: None,
        body: "b".into(),
        created: "2026-08-05T10:00:00Z".into(),
        updated: "2026-08-05T10:00:00Z".into(),
        stale: false,
        at: None,
    };
    let found = state.find_blocking(&slug).expect("the work");
    let held = crate::notes::Notes {
        taiyaku_notes: taiyaku_library::NOTES_VERSION,
        work: slug.clone(),
        notes: vec![note("01HELD", "raw"), note("01OWN", "en")],
    };
    crate::notes::store(
        &state.library,
        &found,
        1,
        &held,
        &found.sidecar(),
        crate::works::Mint::Bridge,
    )
    .expect("stores the notes");

    let tag = etag_of(&router, &uri).await;
    let (status, body) = answered(&router, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut sent: crate::notes::Notes = serde_json::from_str(&body).expect("a notes document");
    assert_eq!(
        unanchored(sent.clone()).notes,
        [note("01OWN", "en")],
        "the fixture: `raw` is hidden"
    );

    taiyaku_library::account::grant_library(&state.library.conn(), 1, 90)
        .expect("grants `Other`, which holds `raw`");
    sent.notes.insert(0, note("01NEW", "en"));
    let (status, body) = answered(
        &router,
        Method::PUT,
        &uri,
        Some(&tag),
        Some(&serde_json::to_string(&sent).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED, "{body}");
    assert_eq!(
        unanchored(
            crate::notes::load(&state.library, &found, 1, &found.sidecar())
                .expect("reads the notes")
                .expect("notes are held")
        ),
        held,
        "nothing is stored"
    );
    let (status, body) = answered_if_none_match(&router, &uri, &tag).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a read under the old tag is sent: {body}"
    );
}

/// `request`'s status, `ETag` and body, as the router answers it.
async fn answered_with_tag(
    router: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, Option<String>, String) {
    let response = router.clone().oneshot(request).await.expect("answers");
    let tag = response
        .headers()
        .get(header::ETAG)
        .map(|tag| tag.to_str().expect("an ETag is text").to_string());
    let status = response.status();
    (
        status,
        tag,
        crate::test_support::body_string(response).await,
    )
}

/// A `GET` of `uri`, with `authorization` as its credential when given; with
/// none the router's own session answers it.
fn get_as(uri: &str, authorization: Option<&str>) -> Request<Body> {
    let mut request = Request::get(uri);
    if let Some(credential) = authorization {
        request = request.header(header::AUTHORIZATION, credential);
    }
    request.body(Body::empty()).unwrap()
}

/// A router signed in as a new `user` that sees every library, so a refusal
/// it gets is its role's and not its grant's.
fn a_user_who_sees_every_library(state: &Arc<ServeState>) -> axum::Router {
    let reader = {
        let conn = state.library.conn();
        let id = taiyaku_library::account::add(&conn, "reader", "pw", "user").expect("adds");
        taiyaku_library::account::set_every_library(&conn, id).expect("sees every library");
        let every: bool = conn
            .query_row(
                "SELECT every_library FROM user WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .expect("reads the account");
        assert!(every, "the fixture: `reader` sees every library");
        id
    };
    crate::test_support::authenticating_as(
        crate::build_router(state.clone()),
        &state.library,
        reader,
    )
}

/// **Only an administrator, and never by a key, writes a work's truth
/// document** (O58). Every account that may open the work is served it, so a
/// write changes what every reader is shown.
///
/// The administrator's own API key reads it and is refused the write for its
/// credential. A `user` that sees every library reads it and is refused for
/// its role, so the refusal is not the grant's. Neither refusal moves the
/// store, and the administrator's browser session then writes the same body.
#[tokio::test]
async fn only_an_administrators_browser_session_writes_the_truth_document() {
    let (router, state, _fixture) = crate::test_support::router_over_a_live_library();
    let slug = state.works.snapshot()[0].id.clone();
    let found = state.find_blocking(&slug).expect("the work");
    let uri = format!("/api/works/{slug}/truth");
    let edition = found
        .sidecar()
        .editions
        .keys()
        .next()
        .expect("the work has an edition")
        .clone();
    let mut labelled = taiyaku_core::TruthDoc::new();
    labelled.editions.insert(
        edition,
        taiyaku_core::TruthEdition {
            members: Vec::new(),
            pages: std::collections::BTreeMap::from([(
                0,
                vec![taiyaku_core::TruthRegion {
                    x: 0.1,
                    y: 0.2,
                    w: 0.3,
                    h: 0.4,
                    axis: None,
                    kind: taiyaku_core::Kind::Dialogue,
                    ignore: false,
                    text: Some("labelled".into()),
                }],
            )]),
        },
    );
    let body = labelled.to_json();
    let read = |authorization: Option<&str>| get_as(&uri, authorization);
    let write = |authorization: Option<&str>, tag: &str| {
        let mut request = Request::put(&uri)
            .header(header::IF_MATCH, tag)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(credential) = authorization {
            request = request.header(header::AUTHORIZATION, credential);
        }
        request.body(Body::from(body.clone())).unwrap()
    };
    let stored = || {
        crate::truth::load(&state.library, &found)
            .expect("reads the truth")
            .map(|doc| doc.to_json())
    };
    let before = stored();

    let key = crate::test_support::fixture_authorization();
    let (status, tag, said) = answered_with_tag(&router, read(Some(&key))).await;
    assert_eq!(status, StatusCode::OK, "the key's read: {said}");
    let tag = tag.expect("the key's read carries an ETag");
    let (status, _, said) = answered_with_tag(&router, write(Some(&key), &tag)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the key's write: {said}");
    assert!(
        said.contains("API key"),
        "the key's write is refused for its credential: {said}"
    );
    assert_eq!(stored(), before, "the key's write moved the store");

    let readers = a_user_who_sees_every_library(&state);
    let (status, tag, said) = answered_with_tag(&readers, read(None)).await;
    assert_eq!(status, StatusCode::OK, "the user's read: {said}");
    let tag = tag.expect("the user's read carries an ETag");
    let (status, _, said) = answered_with_tag(&readers, write(None, &tag)).await;
    assert_eq!(
        (status, said.as_str()),
        (StatusCode::FORBIDDEN, "this route is an administrator's"),
        "the user's write"
    );
    assert_eq!(stored(), before, "the user's write moved the store");

    let (status, tag, said) = answered_with_tag(&router, read(None)).await;
    assert_eq!(status, StatusCode::OK, "the administrator's read: {said}");
    let tag = tag.expect("the administrator's read carries an ETag");
    let (status, _, said) = answered_with_tag(&router, write(None, &tag)).await;
    assert_eq!(status, StatusCode::OK, "the administrator's write: {said}");
    assert_eq!(
        stored(),
        Some(body.clone()),
        "the administrator's write is stored"
    );
}

/// **Only an administrator, and never by a key, corrects a work's
/// alignments** (O58). An alignment is shared: every account that opens the
/// work is served it.
///
/// The administrator's own API key reads the work and is refused the
/// correction for its credential. A `user` that sees every library is refused
/// for its role, so the refusal is not the grant's. Neither refusal moves the
/// `run` rows, and the administrator's browser session then makes the same
/// correction.
#[tokio::test]
async fn only_an_administrators_browser_session_corrects_an_alignment() {
    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_live_library_with_an_edge();
    let uri = format!("/api/works/{slug}");
    let read = |authorization: Option<&str>| get_as(&uri, authorization);
    let correct = |authorization: Option<&str>, tag: &str| {
        let mut request = Request::patch(format!("{uri}/alignments"))
            .header(header::IF_MATCH, tag)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(credential) = authorization {
            request = request.header(header::AUTHORIZATION, credential);
        }
        request
            .body(Body::from(r#"{"from":"raw","to":"en","a":"1","b":"0"}"#))
            .unwrap()
    };
    // A side a run pairs with nothing is NULL.
    type Run = (Option<i64>, Option<i64>, Option<i64>, Option<i64>);
    let runs = || -> Vec<Run> {
        let conn = state.library.conn();
        let mut statement = conn
            .prepare(
                "SELECT a_start_page, a_end_page, b_start_page, b_end_page FROM run \
                 ORDER BY alignment_id, ordinal",
            )
            .expect("prepares");
        statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .expect("reads the runs")
            .collect::<Result<_, _>>()
            .expect("reads the runs")
    };
    let before = runs();

    let key = crate::test_support::fixture_authorization();
    let (status, tag, said) = answered_with_tag(&router, read(Some(&key))).await;
    assert_eq!(status, StatusCode::OK, "the key's read: {said}");
    let tag = tag.expect("the key's read carries an ETag");
    let (status, _, said) = answered_with_tag(&router, correct(Some(&key), &tag)).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the key's correction: {said}"
    );
    assert!(
        said.contains("API key"),
        "the key's correction is refused for its credential: {said}"
    );
    assert_eq!(runs(), before, "the key's correction moved the runs");

    let readers = a_user_who_sees_every_library(&state);
    let (status, tag, said) = answered_with_tag(&readers, read(None)).await;
    assert_eq!(status, StatusCode::OK, "the user's read: {said}");
    let tag = tag.expect("the user's read carries an ETag");
    let (status, _, said) = answered_with_tag(&readers, correct(None, &tag)).await;
    assert_eq!(
        (status, said.as_str()),
        (StatusCode::FORBIDDEN, "this route is an administrator's"),
        "the user's correction"
    );
    assert_eq!(runs(), before, "the user's correction moved the runs");

    let (status, tag, said) = answered_with_tag(&router, read(None)).await;
    assert_eq!(status, StatusCode::OK, "the administrator's read: {said}");
    let tag = tag.expect("the administrator's read carries an ETag");
    let (status, _, said) = answered_with_tag(&router, correct(None, &tag)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the administrator's correction: {said}"
    );
    assert_ne!(
        runs(),
        before,
        "the administrator's correction is stored in the runs"
    );
}

/// How many `work` rows the store holds: a write that minted one moves it.
fn work_rows(state: &ServeState) -> i64 {
    state
        .library
        .conn()
        .query_row("SELECT count(*) FROM work", [], |row| row.get(0))
        .expect("counts the works")
}

/// `slug`'s row, deleted behind a live registry's back (O184). Nothing
/// refreshes after it and a lookup that finds the work does not refresh, so
/// the next write reaches the store with the work held and its row gone.
fn delete_the_row_of(state: &ServeState, slug: &str) {
    assert!(state.works.is_live(), "the fixture: a live registry");
    let deleted = state
        .library
        .conn()
        .execute("DELETE FROM work WHERE slug = ?1", [slug])
        .expect("deletes the work's row");
    assert_eq!(deleted, 1, "the fixture: `{slug}` had a row");
    assert!(
        state.works.get(slug).is_some(),
        "the registry still holds `{slug}`"
    );
}

/// The live fixture's work and the key of its first edition.
fn the_live_work(state: &ServeState) -> (String, String) {
    let work = Arc::clone(&state.works.snapshot()[0]);
    let edition = work
        .sidecar()
        .editions
        .keys()
        .next()
        .expect("the work has an edition")
        .clone();
    (work.id.clone(), edition)
}

/// **A position saved for a work whose row is gone answers 404 from a live
/// registry and mints no row** (O184): a minted `bridge:` row would bring the
/// deleted work back, with no editions, at the next refresh. Sent as the
/// administrator, `Access::Every`, since under a grant the view finds no
/// visible member for the deleted row and refuses before the store.
#[tokio::test]
async fn a_position_saved_after_its_works_row_is_gone_answers_404_and_mints_no_row() {
    let (router, state, _fixture) = crate::test_support::router_over_a_live_library();
    let (slug, edition) = the_live_work(&state);
    delete_the_row_of(&state, &slug);
    let rows = work_rows(&state);

    let request = Request::put(format!("/api/works/{slug}/progress"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"edition":"{edition}","slot":0}}"#)))
        .unwrap();
    let (status, _, said) = answered_with_tag(&router, request).await;

    assert_eq!(
        (status, said),
        (StatusCode::NOT_FOUND, format!("no work `{slug}`"))
    );
    assert_eq!(work_rows(&state), rows, "the save minted a work row");
}

/// As [`a_position_saved_after_its_works_row_is_gone_answers_404_and_mints_no_row`],
/// for a note.
#[tokio::test]
async fn notes_saved_after_their_works_row_is_gone_answer_404_and_mint_no_row() {
    let (router, state, _fixture) = crate::test_support::router_over_a_live_library();
    let (slug, edition) = the_live_work(&state);
    let uri = format!("/api/works/{slug}/notes");
    delete_the_row_of(&state, &slug);
    let (status, tag, said) = answered_with_tag(&router, get_as(&uri, None)).await;
    assert_eq!(status, StatusCode::OK, "the notes read: {said}");
    let tag = tag.expect("the notes read carries an ETag");
    let rows = work_rows(&state);

    let request = Request::put(&uri)
        .header(header::IF_MATCH, tag)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"work":"{slug}","notes":[{{"id":"01J8","edition":"{edition}","slot":0,
               "body":"kept","created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}]}}"#
        )))
        .unwrap();
    let (status, _, said) = answered_with_tag(&router, request).await;

    assert_eq!(
        (status, said),
        (StatusCode::NOT_FOUND, format!("no work `{slug}`"))
    );
    assert_eq!(work_rows(&state), rows, "the save minted a work row");
}

/// As [`a_position_saved_after_its_works_row_is_gone_answers_404_and_mints_no_row`],
/// for a truth document.
#[tokio::test]
async fn truth_saved_after_its_works_row_is_gone_answers_404_and_mints_no_row() {
    let (router, state, _fixture) = crate::test_support::router_over_a_live_library();
    let (slug, edition) = the_live_work(&state);
    let uri = format!("/api/works/{slug}/truth");
    delete_the_row_of(&state, &slug);
    let (status, tag, said) = answered_with_tag(&router, get_as(&uri, None)).await;
    assert_eq!(status, StatusCode::OK, "the truth read: {said}");
    let tag = tag.expect("the truth read carries an ETag");
    let rows = work_rows(&state);
    let mut looked_at = taiyaku_core::TruthDoc::new();
    looked_at.editions.insert(
        edition,
        taiyaku_core::TruthEdition {
            members: Vec::new(),
            pages: std::collections::BTreeMap::from([(0, Vec::new())]),
        },
    );

    let request = Request::put(&uri)
        .header(header::IF_MATCH, tag)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(looked_at.to_json()))
        .unwrap();
    let (status, _, said) = answered_with_tag(&router, request).await;

    assert_eq!(
        (status, said),
        (StatusCode::NOT_FOUND, format!("no work `{slug}`"))
    );
    assert_eq!(work_rows(&state), rows, "the save minted a work row");
}

/// **A correction asking a change of a work whose row is gone answers 404,
/// mints no row and leaves its `taiyaku.json` as it was** (O184). The row
/// is resolved first, or the file would keep a correction no row holds.
///
/// The control is the same correction with the row in place: it writes the
/// file, so the body asks a change (`raw` 1 to `en` 0, where the edge pairs 1
/// with 1) and the work has a file to write.
#[tokio::test]
async fn a_correction_after_its_works_row_is_gone_answers_404_and_writes_no_row_or_file() {
    let correct = |slug: &str, tag: &str| {
        Request::patch(format!("/api/works/{slug}/alignments"))
            .header(header::IF_MATCH, tag)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"from":"raw","to":"en","a":"1","b":"0"}"#))
            .unwrap()
    };
    let tag_of = |router: &axum::Router, slug: &str| {
        let request = get_as(&format!("/api/works/{slug}"), None);
        let router = router.clone();
        async move {
            let (status, tag, said) = answered_with_tag(&router, request).await;
            assert_eq!(status, StatusCode::OK, "the work's read: {said}");
            tag.expect("the work's read carries an ETag")
        }
    };
    {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_a_document();
        let file = state.find_blocking(&slug).expect("the work").path.clone();
        let before = std::fs::read_to_string(&file).expect("reads the document");
        let tag = tag_of(&router, &slug).await;
        let (status, _, said) = answered_with_tag(&router, correct(&slug, &tag)).await;
        assert_eq!(status, StatusCode::OK, "the control's correction: {said}");
        assert_ne!(
            std::fs::read_to_string(&file).expect("reads the document"),
            before,
            "the control's correction writes the file"
        );
    }

    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_live_library_with_a_document();
    let runs: Vec<(i64, i64, i64, i64)> = {
        let conn = state.library.conn();
        let mut statement = conn
            .prepare("SELECT a_start_page, a_end_page, b_start_page, b_end_page FROM run")
            .expect("prepares");
        statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .expect("reads the runs")
            .collect::<Result<_, _>>()
            .expect("reads the runs")
    };
    assert_eq!(
        runs,
        vec![(0, 3, 0, 3)],
        "the fixture's edge pairs `raw` 0-3 with `en` 0-3"
    );
    let file = state.find_blocking(&slug).expect("the work").path.clone();
    let before = std::fs::read_to_string(&file).expect("reads the document");
    let tag = tag_of(&router, &slug).await;
    delete_the_row_of(&state, &slug);
    let rows = work_rows(&state);

    let (status, _, said) = answered_with_tag(&router, correct(&slug, &tag)).await;

    assert_eq!(
        (status, said),
        (StatusCode::NOT_FOUND, format!("no work `{slug}`"))
    );
    assert_eq!(work_rows(&state), rows, "the correction minted a work row");
    assert_eq!(
        std::fs::read_to_string(&file).expect("reads the document"),
        before,
        "the correction wrote the file"
    );
}

/// **A truth read leaves out an edition the account cannot see** (O191), and
/// every truth write the account sends is refused by the administrator's gate
/// (O58) and stores nothing: one changing the hidden edition's entry, one
/// naming an edition the work does not have, and one changing only what it
/// was sent.
#[tokio::test]
async fn a_truth_read_leaves_out_an_edition_the_account_cannot_see_and_a_write_is_refused() {
    use axum::http::Method;

    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_work_whose_editions_sit_in_two_libraries();
    let uri = format!("/api/works/{slug}/truth");
    let found = state.find_blocking(&slug).expect("the work");
    let entry = |page: u32| taiyaku_core::TruthEdition {
        members: Vec::new(),
        pages: std::collections::BTreeMap::from([(page, Vec::new())]),
    };
    let mut held = taiyaku_core::TruthDoc::new();
    held.editions.insert("raw".into(), entry(0));
    held.editions.insert("en".into(), entry(0));
    crate::truth::store(&state.library, &found, &held, crate::works::Mint::Bridge)
        .expect("stores the truth");
    let stored = || {
        crate::truth::load(&state.library, &found)
            .expect("reads the truth")
            .expect("truth is held")
            .to_json()
    };
    let before = stored();

    let tag = etag_of(&router, &uri).await;
    let (status, body) = answered(&router, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut sent = taiyaku_core::TruthDoc::from_json(&body).expect("a truth document");
    assert_eq!(
        sent.editions.keys().collect::<Vec<_>>(),
        ["en"],
        "the fixture: `raw` is hidden, and its entry is not sent"
    );

    let changed = |key: &str| {
        let mut doc = sent.clone();
        doc.editions.insert(key.into(), entry(1));
        doc.to_json()
    };
    let (raw, zz) = (changed("raw"), changed("zz"));
    sent.editions
        .get_mut("en")
        .expect("en")
        .pages
        .insert(1, Vec::new());
    for (label, body) in [
        ("a hidden edition's entry changed", raw),
        ("an edition the work does not have", zz),
        ("en's entry changed", sent.to_json()),
    ] {
        let (status, said) = answered(&router, Method::PUT, &uri, Some(&tag), Some(&body)).await;
        assert_eq!(
            (status, said.as_str()),
            (StatusCode::FORBIDDEN, "this route is an administrator's"),
            "{label}"
        );
        assert_eq!(stored(), before, "{label}: nothing is stored");
    }
}

/// **A position on an edition the work no longer has is sent only to an
/// account with no limits.** A re-solve can rename an edition's key away and
/// leave the reader's position under the old one.
///
/// Restricted, the read names `en` and not `gone`: the visible set holds only
/// editions the work has now, so a key outside it is left out with the rest.
/// Unrestricted, once the account is set to every library, the same read
/// names `gone` as well.
#[tokio::test]
async fn a_position_on_an_edition_the_work_no_longer_has_is_sent_only_to_an_account_with_no_limits()
{
    use axum::http::Method;

    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_work_whose_editions_sit_in_two_libraries();
    let uri = format!("/api/works/{slug}/progress");
    let found = state.find_blocking(&slug).expect("the work");
    for edition in ["gone", "en"] {
        crate::progress::store(
            &state.library,
            &found,
            1,
            &crate::Progress {
                edition: edition.to_string(),
                slot: 0,
            },
            &found.sidecar(),
            taiyaku_library::state::Arrival::next(),
            crate::works::Mint::Bridge,
        )
        .expect("stores a position");
    }
    let named = |body: &str| -> Vec<String> {
        let doc: serde_json::Value = serde_json::from_str(body).expect("a progress document");
        let mut editions: Vec<String> = doc["positions"]
            .as_array()
            .expect("positions is an array")
            .iter()
            .map(|p| p["edition"].as_str().expect("an edition key").to_string())
            .collect();
        editions.sort();
        editions
    };

    let (status, body) = answered(&router, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(named(&body), ["en"], "restricted: {body}");

    let lifted = taiyaku_library::account::set_every_library(&state.library.conn(), 1)
        .expect("lifts the grant");
    assert!(lifted, "the account sees every library from here");

    let (status, body) = answered(&router, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(named(&body), ["en", "gone"], "unrestricted: {body}");
}

/// One write whose own checks could answer before the grant does. `body` is
/// built from the path's slug and handed the slug of a work the account may
/// see, for the request whose body names another work.
struct OwnCheck {
    label: &'static str,
    document: &'static str,
    if_match: Tag,
    body: fn(&str, &str) -> String,
    /// What a work the account may see answers it. Never `no work`.
    on_a_granted_work: StatusCode,
}

/// The `If-Match` an [`OwnCheck`] carries.
#[derive(Debug, Clone, Copy)]
enum Tag {
    Absent,
    Stale,
    /// The notes tag the account is sent. To a hidden work, the one an
    /// unwritten work answers `Access::Every` with, which anyone can compute;
    /// to a granted one, read first, as an account with grant limits is sent
    /// a tag naming its numbering (spec §3.8, decision 11).
    Current,
}

/// The writes [`a_write_to_a_hidden_work_is_refused_before_its_own_checks_are_made`]
/// sends: sent to the work outside the grant, each would fail one check the
/// handler makes after the work is found. The truth write is not one: an
/// account a grant limits is refused it for its role first (O58).
fn writes_that_fail_their_own_checks() -> Vec<OwnCheck> {
    vec![
        OwnCheck {
            label: "progress, an edition the work lacks",
            document: "progress",
            if_match: Tag::Absent,
            body: |_, _| r#"{"edition":"nope","slot":0}"#.to_string(),
            on_a_granted_work: StatusCode::NOT_FOUND,
        },
        OwnCheck {
            label: "progress, a slot past the end",
            document: "progress",
            if_match: Tag::Absent,
            body: |_, _| r#"{"edition":"raw","slot":99}"#.to_string(),
            on_a_granted_work: StatusCode::NOT_FOUND,
        },
        OwnCheck {
            label: "notes, no If-Match",
            document: "notes",
            if_match: Tag::Absent,
            body: |slug, _| format!(r#"{{"work":"{slug}","notes":[]}}"#),
            on_a_granted_work: StatusCode::PRECONDITION_REQUIRED,
        },
        OwnCheck {
            label: "notes, a stale If-Match",
            document: "notes",
            if_match: Tag::Stale,
            body: |slug, _| format!(r#"{{"work":"{slug}","notes":[]}}"#),
            on_a_granted_work: StatusCode::PRECONDITION_FAILED,
        },
        OwnCheck {
            label: "notes, a body that names the granted work",
            document: "notes",
            if_match: Tag::Current,
            body: |_, granted| format!(r#"{{"work":"{granted}","notes":[]}}"#),
            on_a_granted_work: StatusCode::OK,
        },
        OwnCheck {
            label: "notes, the current tag and a note on an edition the work lacks",
            document: "notes",
            if_match: Tag::Current,
            body: |slug, _| {
                format!(
                    r#"{{"work":"{slug}","notes":[{{"id":"01J8","edition":"nope","slot":1,
                       "body":"kept","created":"2026-08-05T10:00:00Z",
                       "updated":"2026-08-05T10:00:00Z"}}]}}"#
                )
            },
            on_a_granted_work: StatusCode::NOT_FOUND,
        },
    ]
}

/// **A write to a work outside the grant is refused before any check the
/// handler makes once it has found the work** (O60): by the grant, and not by
/// the edition, the slot, the precondition or the work the body names.
///
/// [`the_five_requests`] sends each write once, fully valid, so a grant check
/// that slid later in a handler would still refuse it with the same 404. Here
/// each write is one its own checks would answer, sent to a work outside the
/// grant and to a slug no work holds, and the two answers are compared with
/// each other, the slug put aside. A later grant check would tell the first
/// something the second is never told: an edition's name, a slot count, the
/// `ETag` of a document the caller may not see.
///
/// **Then each is sent to a work the account may see**, and answered on its
/// merits and not as a missing work. That is what shows these are requests the
/// server judges, rather than ones refused alike for any slug.
#[tokio::test]
async fn a_write_to_a_hidden_work_is_refused_before_its_own_checks_are_made() {
    use axum::http::Method;

    let tmp = crate::test_support::Fixture::new("o60-before-its-own-checks");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let slug_of = |title: &str| -> String {
        state
            .library
            .conn()
            .query_row("SELECT slug FROM work WHERE title = ?1", [title], |row| {
                row.get(0)
            })
            .expect("the work's slug")
    };
    let granted = slug_of("First Directory Work");
    let hidden = slug_of("Other Library Work");
    let unknown = "no-work-is-called-this";

    for case in writes_that_fail_their_own_checks() {
        let mut said = Vec::new();
        for slug in [hidden.as_str(), unknown] {
            let if_match = match case.if_match {
                Tag::Absent => None,
                Tag::Stale => Some("\"stale\"".to_string()),
                Tag::Current => Some(notes_etag(
                    &crate::notes::to_json(&crate::notes::empty(slug)).unwrap(),
                )),
            };
            let (status, body) = answered(
                &router,
                Method::PUT,
                &format!("/api/works/{slug}/{}", case.document),
                if_match.as_deref(),
                Some(&(case.body)(slug, &granted)),
            )
            .await;
            said.push((status, body.replace(slug, "{work}")));
        }
        assert_eq!(
            said[0], said[1],
            "PUT {}: a work outside the grant, left, and a slug no work holds, right",
            case.label
        );
        assert_eq!(said[0].0, StatusCode::NOT_FOUND, "PUT {}", case.label);

        let uri = format!("/api/works/{granted}/{}", case.document);
        let if_match = match case.if_match {
            Tag::Absent => None,
            Tag::Stale => Some("\"stale\"".to_string()),
            Tag::Current => Some(etag_of(&router, &uri).await),
        };
        let (status, body) = answered(
            &router,
            Method::PUT,
            &uri,
            if_match.as_deref(),
            Some(&(case.body)(&granted, &granted)),
        )
        .await;
        assert_eq!(
            status, case.on_a_granted_work,
            "PUT {}, to a work the account may see: {body}",
            case.label
        );
        assert!(
            !body.starts_with("no work `"),
            "PUT {}, to a work the account may see, was answered as a missing work: {body}",
            case.label
        );
    }
}

/// **The work a notes body names is the work its path names** (O60). The
/// grant is asked about the path's work, so a body that names a different one
/// must not move the write there.
///
/// As the account granted `Shelf` alone, a well-formed document naming the
/// work under `Other` is sent to a work under `Shelf`, with that work's current
/// tag so the precondition is met. It is refused as a body that does not belong
/// to the path. Then the grant is lifted and both works' notes are read: the
/// note was stored under neither.
#[tokio::test]
async fn a_notes_body_naming_another_work_is_refused_and_stored_under_neither() {
    use axum::http::Method;

    let tmp = crate::test_support::Fixture::new("o60-body-names-another-work");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let slug_of = |title: &str| -> String {
        state
            .library
            .conn()
            .query_row("SELECT slug FROM work WHERE title = ?1", [title], |row| {
                row.get(0)
            })
            .expect("the work's slug")
    };
    let granted = slug_of("First Directory Work");
    let hidden = slug_of("Other Library Work");
    let granted_notes = format!("/api/works/{granted}/notes");
    let hidden_notes = format!("/api/works/{hidden}/notes");
    let body = format!(
        r#"{{"work":"{hidden}","notes":[{{"id":"01J8","edition":"raw","slot":1,"body":"misplaced",
               "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}]}}"#
    );

    let tag = etag_of(&router, &granted_notes).await;
    let (status, text) = answered(
        &router,
        Method::PUT,
        &granted_notes,
        Some(&tag),
        Some(&body),
    )
    .await;
    assert_eq!(
        (status, text),
        (
            StatusCode::BAD_REQUEST,
            format!("these notes name the work `{hidden}`, but this is `{granted}`")
        ),
        "a body naming another work, sent to a work the account may see"
    );

    taiyaku_library::account::set_every_library(&state.library.conn(), 1).expect("lifts the grant");
    for uri in [&granted_notes, &hidden_notes] {
        let (status, text) = answered(&router, Method::GET, uri, None, None).await;
        assert_eq!(status, StatusCode::OK, "GET {uri}: {text}");
        assert!(
            !text.contains("misplaced"),
            "the note a body sent to the wrong work carried reached the store: GET {uri}: {text}"
        );
    }
}

/// One `GET` carrying `If-None-Match`, answered as its status and its body.
async fn answered_if_none_match(
    router: &axum::Router,
    uri: &str,
    if_none_match: &str,
) -> (StatusCode, String) {
    let request = Request::builder()
        .uri(uri)
        .header(header::IF_NONE_MATCH, if_none_match)
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// **A conditional read of a work outside the grant is refused as a read of a
/// slug no work holds** (O60), before the tag is compared.
///
/// An unwritten work's stored-document tag is one anyone can compute, and a
/// comparison made above the grant, without the account's view, would hit
/// it: a hidden work would answer 304 where an unknown slug answers 404. Each
/// read is sent under it to both, and the answers compared, the slug put aside.
#[tokio::test]
async fn a_conditional_read_of_a_hidden_work_is_refused_as_a_slug_no_work_holds() {
    let tmp = crate::test_support::Fixture::new("o60-conditional-read");
    let (router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let slug_of = |title: &str| -> String {
        state
            .library
            .conn()
            .query_row("SELECT slug FROM work WHERE title = ?1", [title], |row| {
                row.get(0)
            })
            .expect("the work's slug")
    };
    let granted = slug_of("First Directory Work");
    let hidden = slug_of("Other Library Work");
    let unknown = "no-work-is-called-this";

    // The tag a work nobody has written on answers with, for each document.
    let tag = |document: &str, slug: &str| -> String {
        if document == "notes" {
            notes_etag(&crate::notes::to_json(&crate::notes::empty(slug)).unwrap())
        } else {
            notes_etag(&crate::truth::to_json(&taiyaku_core::TruthDoc::new()))
        }
    };
    for document in ["notes", "truth"] {
        let mut said = Vec::new();
        for slug in [hidden.as_str(), unknown] {
            let (status, body) = answered_if_none_match(
                &router,
                &format!("/api/works/{slug}/{document}"),
                &tag(document, slug),
            )
            .await;
            said.push((status, body.replace(slug, "{work}")));
        }
        assert_eq!(
            said[0], said[1],
            "conditional GET {document}: a work outside the grant, left, and a slug no work \
             holds, right"
        );
        assert_eq!(
            said[0].0,
            StatusCode::NOT_FOUND,
            "conditional GET {document}"
        );

        // A work the account may see answers 304 under the tag it is sent,
        // which names its view (spec §3.8, decision 11), not the stored
        // document's.
        let uri = format!("/api/works/{granted}/{document}");
        let sent = etag_of(&router, &uri).await;
        assert_ne!(
            sent,
            tag(document, &granted),
            "{document}: an account with grant limits is sent a tag naming its view"
        );
        let (status, body) = answered_if_none_match(&router, &uri, &sent).await;
        assert_eq!(
            status,
            StatusCode::NOT_MODIFIED,
            "conditional GET {document}, of a work the account may see: {body}"
        );
    }

    // Lifted, the grant leaves the stored document's tag, so the tag the
    // hidden half sends is one the server hits.
    taiyaku_library::account::set_every_library(&state.library.conn(), 1).expect("lifts the grant");
    for document in ["notes", "truth"] {
        let uri = format!("/api/works/{granted}/{document}");
        let (status, body) = answered_if_none_match(&router, &uri, &tag(document, &granted)).await;
        assert_eq!(
            status,
            StatusCode::NOT_MODIFIED,
            "conditional GET {document} under the stored document's tag, the grant lifted: {body}"
        );
    }
}

/// Spec §3.8's narrowing, over
/// [`test_support::router_over_an_edition_merged_from_two_libraries`].
mod narrowing {
    use super::*;
    use crate::test_support::{self, EditionMergedFromTwoLibraries, Reader};
    use std::collections::{BTreeSet, HashMap};
    use taiyaku_core::{Member, Sidecar};
    use taiyaku_library::browse::SeenMember;

    /// `GET uri` as `reader`, with `If-None-Match` when given: the status, the
    /// `ETag` and the body.
    async fn fetched(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        uri: &str,
        if_none_match: Option<&str>,
    ) -> (StatusCode, Option<String>, String) {
        let mut request = Request::get(uri);
        if let Some(tag) = if_none_match {
            request = request.header(header::IF_NONE_MATCH, tag);
        }
        sent(merged, reader, request.body(Body::empty()).unwrap()).await
    }

    async fn sent(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        request: Request<Body>,
    ) -> (StatusCode, Option<String>, String) {
        let response = merged.send(reader, request).await;
        (
            response.status(),
            etag_of(&response),
            String::from_utf8_lossy(response.body()).into_owned(),
        )
    }

    fn etag_of<B>(response: &axum::http::Response<B>) -> Option<String> {
        response
            .headers()
            .get(header::ETAG)
            .map(|tag| tag.to_str().unwrap().to_string())
    }

    /// `reader`'s work document and its `ETag`.
    async fn document(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
    ) -> (serde_json::Value, String) {
        let (status, tag, body) =
            fetched(merged, reader, &format!("/api/works/{}", merged.slug), None).await;
        assert_eq!(status, StatusCode::OK, "{reader:?}'s document: {body}");
        (
            serde_json::from_str(&body).expect("the document is JSON"),
            tag.expect("the document has an ETag"),
        )
    }

    /// Each member of `doc`'s `edition`: its file name and page count.
    fn members(doc: &serde_json::Value, edition: &str) -> Vec<(String, u64)> {
        doc["editions"][edition]["members"]
            .as_array()
            .unwrap_or_else(|| panic!("no edition `{edition}` in {doc}"))
            .iter()
            .map(|member| {
                let path = member["path"].as_str().expect("a path");
                let file = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("a file name");
                (file.to_string(), member["pages"].as_u64().expect("pages"))
            })
            .collect()
    }

    fn files(named: &[(&str, u64)]) -> Vec<(String, u64)> {
        named
            .iter()
            .map(|(file, pages)| ((*file).to_string(), *pages))
            .collect()
    }

    fn editions(doc: &serde_json::Value) -> Vec<String> {
        doc["editions"]
            .as_object()
            .expect("an editions object")
            .keys()
            .cloned()
            .collect()
    }

    fn alignments(doc: &serde_json::Value) -> BTreeSet<(String, String)> {
        doc["alignments"]
            .as_array()
            .expect("an alignments array")
            .iter()
            .map(|a| {
                (
                    a["from"].as_str().expect("from").to_string(),
                    a["to"].as_str().expect("to").to_string(),
                )
            })
            .collect()
    }

    fn pair(from: &str, to: &str) -> (String, String) {
        (from.to_string(), to.to_string())
    }

    /// `members` as the store reads them, in this order, each flagged as
    /// `visible` says.
    fn read_as(members: &[&Member], visible: &[bool]) -> Vec<SeenMember> {
        assert_eq!(members.len(), visible.len(), "one flag per member");
        members
            .iter()
            .zip(visible)
            .map(|(member, &visible)| SeenMember {
                sha256: member.sha256.clone(),
                path: member.path.clone(),
                visible,
            })
            .collect()
    }

    /// Flags for `sidecar`: `raw` as given, and `en` and `fr` visible and read
    /// of their own members.
    fn with_raw(sidecar: &Sidecar, raw: Vec<SeenMember>) -> HashMap<String, Vec<SeenMember>> {
        let whole = |key: &str| {
            let members: Vec<&Member> = sidecar.editions[key].members.iter().collect();
            read_as(&members, &vec![true; members.len()])
        };
        HashMap::from([
            ("raw".to_string(), raw),
            ("en".to_string(), whole("en")),
            ("fr".to_string(), whole("fr")),
        ])
    }

    fn the_work(merged: &EditionMergedFromTwoLibraries) -> Arc<crate::Work> {
        merged.state.find_blocking(&merged.slug).expect("the work")
    }

    fn the_sidecar(merged: &EditionMergedFromTwoLibraries) -> Arc<Sidecar> {
        the_work(merged).sidecar()
    }

    /// Whether a page route sees `raw` under `flags`.
    fn a_page_of_raw_is_sent(
        merged: &EditionMergedFromTwoLibraries,
        flags: &HashMap<String, Vec<SeenMember>>,
    ) -> bool {
        crate::view::EditionView::build(the_work(merged), "raw", Some(flags)).is_some()
    }

    /// The fixture's shape, from the administrator's document, which every
    /// test below states first: `raw` is part1, part2 and part3 (9 pages), `en`
    /// 5 pages, `fr` 6, and both seeded alignments are there. Answers the
    /// document.
    async fn the_administrators_document(
        merged: &EditionMergedFromTwoLibraries,
    ) -> serde_json::Value {
        let (doc, _) = document(merged, Reader::Administrator).await;
        assert_eq!(editions(&doc), ["en", "fr", "raw"], "{doc}");
        assert_eq!(
            members(&doc, "raw"),
            files(&[
                ("Foo v01 part1.cbz", 2),
                ("Foo v01 part2.cbz", 3),
                ("Foo v01 part3.cbz", 4),
            ]),
            "the fixture: raw's hidden-to-Manga member is not the last"
        );
        assert_eq!(members(&doc, "en"), files(&[("Foo v01.cbz", 5)]));
        assert_eq!(members(&doc, "fr"), files(&[("Foo v01.cbz", 6)]));
        assert_eq!(
            alignments(&doc),
            BTreeSet::from([pair("raw", "en"), pair("en", "fr")])
        );
        doc
    }

    #[tokio::test]
    async fn the_merged_edition_fixture_has_the_shape_its_tests_name() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let doc = the_administrators_document(&merged).await;
        assert_eq!(doc["hub"], "en", "the derive makes `en` the hub: {doc}");
    }

    /// The store's answer, which narrowing reads: one flag per member in
    /// ordinal order, for each grant, read of the members the sidecar lists.
    #[test]
    fn each_grant_is_flagged_the_members_of_the_merged_edition_it_sees() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let sidecar = the_sidecar(&merged);
        let conn = merged.state.library.conn();
        let flags = |reader: Reader| -> HashMap<String, Vec<bool>> {
            let access = taiyaku_library::account::access_for(&conn, merged.user_id(reader))
                .expect("reads the access");
            assert_ne!(access, Access::Every, "{reader:?} is restricted");
            let read = taiyaku_library::browse::visible_members(&conn, &access, &merged.slug, None)
                .expect("reads the flags");
            read.into_iter()
                .map(|(key, seen)| {
                    let listed: Vec<&Member> = sidecar.editions[&key].members.iter().collect();
                    let visible: Vec<bool> = seen.iter().map(|seen| seen.visible).collect();
                    assert_eq!(
                        seen,
                        read_as(&listed, &visible),
                        "{reader:?}: `{key}` read of the sidecar's members, in its order"
                    );
                    (key, visible)
                })
                .collect()
        };
        let of = |raw: &[bool], en: bool, fr: bool| {
            HashMap::from([
                ("raw".to_string(), raw.to_vec()),
                ("en".to_string(), vec![en]),
                ("fr".to_string(), vec![fr]),
            ])
        };
        assert_eq!(
            flags(Reader::MangaOnly),
            of(&[true, false, true], true, true)
        );
        assert_eq!(
            flags(Reader::KidsOnly),
            of(&[false, true, false], false, false)
        );
        assert_eq!(flags(Reader::Both), of(&[true, true, true], true, true));
    }

    /// The flags and the sidecar are two reads, so flags that do not number an
    /// edition's members hide it rather than pairing a member with another's
    /// flag.
    #[tokio::test]
    async fn flags_that_do_not_number_an_editions_members_hide_it() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let sidecar = the_sidecar(&merged);
        let listed: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
        for raw in [
            read_as(&listed[..2], &[true, true]),
            read_as(&[listed[0], listed[1], listed[2], listed[2]], &[true; 4]),
        ] {
            let flags = with_raw(&sidecar, raw.clone());
            let view = crate::view::View::build(Arc::clone(&sidecar), Some(&flags))
                .expect("en and fr are visible");
            assert_eq!(
                view.sidecar().editions.keys().collect::<Vec<_>>(),
                ["en", "fr"],
                "{} flags for raw's 3 members",
                raw.len()
            );
            assert!(
                !a_page_of_raw_is_sent(&merged, &flags),
                "{} flags for raw's 3 members, to a page route",
                raw.len()
            );
        }
    }

    /// The sidecar can be the registry's copy from before a scan that moved a
    /// member to another ordinal and kept the count, so flags read of other
    /// members, or of these in another order, hide the edition too, whether
    /// they would narrow it or send it whole.
    #[tokio::test]
    async fn flags_read_of_other_members_hide_the_edition() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let sidecar = the_sidecar(&merged);
        let listed: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
        assert_ne!(
            listed[0].sha256, listed[2].sha256,
            "the fixture: part1 and part3 are different files"
        );
        let renamed = Member {
            path: format!("{}.moved", listed[2].path),
            ..listed[2].clone()
        };
        let replaced = Member {
            sha256: "0".repeat(64),
            ..listed[0].clone()
        };
        for (case, members) in [
            (
                "part1 and part3 exchanged",
                [listed[2], listed[1], listed[0]],
            ),
            ("part3 at another path", [listed[0], listed[1], &renamed]),
            (
                "part1 replaced at its path",
                [&replaced, listed[1], listed[2]],
            ),
        ] {
            for visible in [[true, false, true], [true, true, true]] {
                let flags = with_raw(&sidecar, read_as(&members, &visible));
                let view = crate::view::View::build(Arc::clone(&sidecar), Some(&flags))
                    .expect("en and fr are visible");
                assert_eq!(
                    view.sidecar().editions.keys().collect::<Vec<_>>(),
                    ["en", "fr"],
                    "{case}, flagged {visible:?}"
                );
                assert!(
                    !a_page_of_raw_is_sent(&merged, &flags),
                    "{case}, flagged {visible:?}, to a page route"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_narrowed_editions_pages_translate_to_the_full_numbering() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let work = the_work(&merged);
        let sidecar = work.sidecar();
        let listed: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
        let flags = with_raw(&sidecar, read_as(&listed, &[true, false, true]));
        let view = crate::view::View::build(Arc::clone(&sidecar), Some(&flags)).expect("visible");
        assert!(view.narrowed("raw").is_some());
        assert!(view.narrowed("en").is_none(), "en is sent whole");
        let edition =
            |key: &str| crate::view::EditionView::build(Arc::clone(&work), key, Some(&flags));
        let raw = edition("raw").expect("raw is visible");
        assert_eq!(
            (0..7)
                .map(|page| raw.page_to_full(page))
                .collect::<Vec<_>>(),
            [
                Ok(0),
                Ok(1),
                Ok(5),
                Ok(6),
                Ok(7),
                Ok(8),
                Err("`raw` has 6 pages; there is no page 6".to_string())
            ]
        );
        assert_eq!(
            edition("en").expect("en is visible").page_to_full(3),
            Ok(3),
            "a whole edition's page is itself"
        );
        assert!(edition("de").is_none(), "an edition it is not sent");
    }

    async fn page(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        page: &str,
        if_none_match: Option<&str>,
    ) -> axum::http::Response<Vec<u8>> {
        let uri = format!("/api/works/{}/raw/pages/{page}", merged.slug);
        let response = merged.get(reader, &uri, if_none_match).await;
        assert!(
            [StatusCode::OK, StatusCode::NOT_MODIFIED].contains(&response.status()),
            "{reader:?}'s {uri}: {}",
            String::from_utf8_lossy(response.body())
        );
        response
    }

    /// Manga's page 2 is full page 5, part3's first: the same bytes under the
    /// same `ETag`, so either account's copy revalidates the other's.
    #[tokio::test]
    async fn a_narrowed_page_is_the_full_page_it_translates_to() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let full = page(&merged, Reader::Administrator, "5", None).await;
        assert!(
            full.body() != page(&merged, Reader::Administrator, "2", None).await.body(),
            "the fixture: pages 2 and 5 differ"
        );
        let narrowed = page(&merged, Reader::MangaOnly, "2", None).await;
        assert!(narrowed.body() == full.body(), "Manga's page 2 is page 5");
        assert_eq!(etag_of(&narrowed), etag_of(&full));
        let past = merged
            .get(
                Reader::MangaOnly,
                &format!("/api/works/{}/raw/pages/6", merged.slug),
                None,
            )
            .await;
        assert_eq!(past.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            String::from_utf8_lossy(past.body()),
            "`raw` has 6 pages; there is no page 6"
        );
        let revalidated = page(&merged, Reader::MangaOnly, "2", etag_of(&full).as_deref()).await;
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED);
    }

    /// Manga's thumb of page 2 is page 5's. Asked first, on a cold cache: a
    /// thumb kept under one page's member and drawn from another's would be
    /// the answer the administrator is then given for one of the two.
    #[tokio::test]
    async fn a_narrowed_thumb_is_the_full_pages_thumb() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let narrowed = page(&merged, Reader::MangaOnly, "2/thumb", None).await;
        let full = page(&merged, Reader::Administrator, "5/thumb", None).await;
        let hidden = page(&merged, Reader::Administrator, "2/thumb", None).await;
        assert!(
            full.body() != hidden.body(),
            "the thumbs of pages 2 and 5 differ: neither was kept under the other's member"
        );
        assert!(
            narrowed.body() == full.body(),
            "Manga's thumb of page 2 is page 5's"
        );
    }

    #[tokio::test]
    async fn kids_alones_first_page_is_part2s_first() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let full = page(&merged, Reader::Administrator, "2", None).await;
        assert!(
            full.body() != page(&merged, Reader::Administrator, "0", None).await.body(),
            "the fixture: pages 0 and 2 differ"
        );
        let narrowed = page(&merged, Reader::KidsOnly, "0", None).await;
        assert!(narrowed.body() == full.body(), "Kids' page 0 is page 2");
        assert_eq!(etag_of(&narrowed), etag_of(&full));
    }

    /// The series cover is the thumb of the edition Kids sees, `raw`, which
    /// is drawn from Kids' page 0, at the tile's size and at a cover's.
    #[tokio::test]
    async fn kids_alones_series_cover_is_part2s_first_page() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let series: i64 = merged
            .state
            .library
            .conn()
            .query_row("SELECT series_id FROM work", [], |row| row.get(0))
            .expect("the fixture: one work, in one series");
        let tile = page(&merged, Reader::Administrator, "2/thumb", None).await;
        assert!(
            tile.body()
                != page(&merged, Reader::Administrator, "0/thumb", None)
                    .await
                    .body(),
            "the fixture: the thumbs of pages 0 and 2 differ"
        );
        // A 16-pixel page fits a cover's size, so the cover is the page itself.
        let full = page(&merged, Reader::Administrator, "2", None).await;
        for (query, expected) in [("", &tile), ("?w=256", &full)] {
            let redirect = merged
                .get(
                    Reader::KidsOnly,
                    &format!("/api/library/series/{series}/cover{query}"),
                    None,
                )
                .await;
            assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT, "{query}");
            let location = redirect.headers()[header::LOCATION].to_str().unwrap();
            let cover = merged.get(Reader::KidsOnly, location, None).await;
            assert_eq!(cover.status(), StatusCode::OK, "{location}");
            assert!(
                cover.body() == expected.body(),
                "{location}: Kids' series cover is part2's first page"
            );
        }
    }

    #[tokio::test]
    async fn manga_alone_is_sent_raw_as_part1_and_part3() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let whole = as_file_names(&the_administrators_document(&merged).await);
        let (doc, _) = document(&merged, Reader::MangaOnly).await;
        assert_eq!(
            members(&doc, "raw"),
            files(&[("Foo v01 part1.cbz", 2), ("Foo v01 part3.cbz", 4)]),
            "{doc}"
        );
        assert_eq!(
            doc["editions"]["en"], whole["editions"]["en"],
            "en is whole"
        );
        assert_eq!(
            doc["editions"]["fr"], whole["editions"]["fr"],
            "fr is whole"
        );
        assert_eq!(
            alignments(&doc),
            BTreeSet::from([pair("en", "fr")]),
            "the alignment touching the narrowed raw is left out"
        );
    }

    #[tokio::test]
    async fn kids_alone_is_sent_raw_as_part2_and_nothing_else() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let (doc, _) = document(&merged, Reader::KidsOnly).await;
        assert_eq!(editions(&doc), ["raw"], "{doc}");
        assert_eq!(members(&doc, "raw"), files(&[("Foo v01 part2.cbz", 3)]));
        assert_eq!(alignments(&doc), BTreeSet::new());
        assert_eq!(doc["hub"], "raw", "the hub moves to the one edition sent");
    }

    #[tokio::test]
    async fn both_libraries_are_sent_raw_whole() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let whole = the_administrators_document(&merged).await;
        let (doc, _) = document(&merged, Reader::Both).await;
        assert_eq!(
            doc,
            as_file_names(&whole),
            "an account granted every library holding it"
        );
    }

    /// The tag is over the document sent, so an account narrowed out of part
    /// of it never revalidates against another's copy.
    #[tokio::test]
    async fn a_narrowed_document_has_its_own_etag() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let uri = format!("/api/works/{}", merged.slug);
        let (_, manga) = document(&merged, Reader::MangaOnly).await;
        let (_, admin) = document(&merged, Reader::Administrator).await;
        assert_ne!(manga, admin);
        for (reader, own, other) in [
            (Reader::MangaOnly, &manga, &admin),
            (Reader::Administrator, &admin, &manga),
        ] {
            let (status, _, _) = fetched(&merged, reader, &uri, Some(own)).await;
            assert_eq!(status, StatusCode::NOT_MODIFIED, "{reader:?}, its own tag");
            let (status, _, _) = fetched(&merged, reader, &uri, Some(other)).await;
            assert_eq!(status, StatusCode::OK, "{reader:?}, the other's tag");
        }
    }

    /// `doc` with each member's `path` as its file name, which is what an
    /// account the gate does not admit is sent of it (O280).
    fn as_file_names(doc: &serde_json::Value) -> serde_json::Value {
        let mut doc = doc.clone();
        let editions = doc["editions"].as_object_mut().expect("an editions object");
        for edition in editions.values_mut() {
            for member in edition["members"].as_array_mut().expect("a members array") {
                let file = std::path::Path::new(member["path"].as_str().expect("a path"))
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("a file name")
                    .to_string();
                member["path"] = file.into();
            }
        }
        doc
    }

    /// Each member's `path` in `doc`'s `edition` as the body spells it, not
    /// through [`members`], which maps it to its file name.
    fn sent_paths(doc: &serde_json::Value, edition: &str) -> Vec<String> {
        doc["editions"][edition]["members"]
            .as_array()
            .unwrap_or_else(|| panic!("no edition `{edition}` in {doc}"))
            .iter()
            .map(|member| member["path"].as_str().expect("a path").to_string())
            .collect()
    }

    fn paths_by_edition(
        doc: &serde_json::Value,
    ) -> std::collections::BTreeMap<String, Vec<String>> {
        editions(doc)
            .into_iter()
            .map(|key| {
                let paths = sent_paths(doc, &key);
                (key, paths)
            })
            .collect()
    }

    fn named(editions: &[(&str, &[&str])]) -> std::collections::BTreeMap<String, Vec<String>> {
        editions
            .iter()
            .map(|(key, paths)| {
                let paths = paths.iter().map(|path| (*path).to_string()).collect();
                ((*key).to_string(), paths)
            })
            .collect()
    }

    /// `GET uri` with the fixture administrator's API key, and with
    /// `If-None-Match` when given.
    async fn fetched_by_key(
        merged: &EditionMergedFromTwoLibraries,
        uri: &str,
        if_none_match: Option<&str>,
    ) -> (StatusCode, Option<String>, String) {
        let mut request = Request::get(uri);
        if let Some(tag) = if_none_match {
            request = request.header(header::IF_NONE_MATCH, tag);
        }
        answered_with_tag(
            &test_support::key_authenticating(crate::build_router(merged.state.clone())),
            request.body(Body::empty()).unwrap(),
        )
        .await
    }

    /// Every edition a `user` is sent names each member by its file name,
    /// and nothing of where the server keeps it (O280).
    #[tokio::test]
    async fn a_readers_document_names_each_member_by_its_file_name() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let whole = the_administrators_document(&merged).await;
        let part1 = sent_paths(&whole, "raw").remove(0);
        let part1 = std::path::Path::new(&part1);
        assert!(
            part1.is_absolute(),
            "the fixture stores a scanned member's whole path: {}",
            part1.display()
        );
        let dir = part1
            .parent()
            .and_then(std::path::Path::parent)
            .expect("part1 sits two levels below the fixture's directory");
        let dir_name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the fixture's directory has a name");
        let dir = dir.display().to_string();
        let uri = format!("/api/works/{}", merged.slug);
        let readers = [Reader::MangaOnly, Reader::KidsOnly, Reader::Both];
        let mut sent = Vec::new();
        let mut told_the_directory = Vec::new();
        for reader in readers {
            let (status, _, body) = fetched(&merged, reader, &uri, None).await;
            assert_eq!(status, StatusCode::OK, "{reader:?}: {body}");
            let doc: serde_json::Value = serde_json::from_str(&body).expect("the document is JSON");
            sent.push((reader, paths_by_edition(&doc)));
            if body.contains(&dir) || body.contains(dir_name) {
                told_the_directory.push(reader);
            }
        }
        assert_eq!(
            sent,
            [
                (
                    Reader::MangaOnly,
                    named(&[
                        ("en", &["Foo v01.cbz"]),
                        ("fr", &["Foo v01.cbz"]),
                        ("raw", &["Foo v01 part1.cbz", "Foo v01 part3.cbz"]),
                    ]),
                ),
                (Reader::KidsOnly, named(&[("raw", &["Foo v01 part2.cbz"])])),
                (
                    Reader::Both,
                    named(&[
                        ("en", &["Foo v01.cbz"]),
                        ("fr", &["Foo v01.cbz"]),
                        (
                            "raw",
                            &[
                                "Foo v01 part1.cbz",
                                "Foo v01 part2.cbz",
                                "Foo v01 part3.cbz",
                            ],
                        ),
                    ]),
                ),
            ]
        );
        assert_eq!(
            told_the_directory,
            [],
            "the readers sent {dir} or {dir_name}"
        );
    }

    /// An administrator by browser session or by password is sent where the
    /// server keeps each member, the same bytes either way.
    #[tokio::test]
    async fn the_administrator_by_session_or_password_is_sent_the_stored_paths() {
        use base64::Engine as _;
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let uri = format!("/api/works/{}", merged.slug);
        let (status, _, by_session) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "by session: {by_session}");
        let basic = base64::engine::general_purpose::STANDARD.encode(
            format!(
                "{}:{}",
                test_support::FIXTURE_NAME,
                test_support::FIXTURE_PASSWORD
            )
            .as_bytes(),
        );
        let (status, _, by_password) = answered_with_tag(
            &crate::build_router(merged.state.clone()),
            Request::get(&uri)
                .header(header::AUTHORIZATION, format!("Basic {basic}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "by password: {by_password}");
        for (by, body) in [("session", &by_session), ("password", &by_password)] {
            let doc: serde_json::Value = serde_json::from_str(body).expect("the document is JSON");
            let raw = sent_paths(&doc, "raw");
            assert_eq!(raw.len(), 3, "by {by}: {raw:?}");
            for (path, end) in raw.iter().zip([
                "m/Foo v01 part1.cbz",
                "k/Foo v01 part2.cbz",
                "m/Foo v01 part3.cbz",
            ]) {
                let path = std::path::Path::new(path);
                assert!(path.is_absolute(), "by {by}: {}", path.display());
                assert!(path.ends_with(end), "by {by}: {} for {end}", path.display());
            }
        }
        assert_eq!(
            by_session, by_password,
            "the session's bytes and the password's"
        );
    }

    /// The gate admits no key, so an administrator's key is sent file names,
    /// the same document as an account granted every library holding the work.
    #[tokio::test]
    async fn an_administrators_key_is_sent_file_names() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let uri = format!("/api/works/{}", merged.slug);
        let (status, _, by_key) = fetched_by_key(&merged, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{by_key}");
        let doc: serde_json::Value = serde_json::from_str(&by_key).expect("the document is JSON");
        assert_eq!(editions(&doc), ["en", "fr", "raw"], "{doc}");
        for key in editions(&doc) {
            for path in sent_paths(&doc, &key) {
                let file = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("a file name");
                assert_eq!(path, file, "a member of `{key}`, by key");
            }
        }
        let (status, _, both) = fetched(&merged, Reader::Both, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{both}");
        assert_eq!(by_key, both, "the key's bytes and Both's");
    }

    /// Each caller's `ETag` is the SHA-256 of the bytes it is sent, so here,
    /// where the stored paths are whole, neither copy revalidates the other.
    #[tokio::test]
    async fn each_document_is_tagged_over_the_bytes_it_is_sent() {
        use sha2::{Digest, Sha256};
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let uri = format!("/api/works/{}", merged.slug);
        let over = |body: &str| {
            let hex: String = Sha256::digest(body.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            format!("\"{hex}\"")
        };
        let (status, both_tag, both) = fetched(&merged, Reader::Both, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{both}");
        let (status, admin_tag, admin) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{admin}");
        let (status, key_tag, by_key) = fetched_by_key(&merged, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{by_key}");
        for (who, tag, body) in [
            ("Both", &both_tag, &both),
            ("the administrator", &admin_tag, &admin),
            ("the key", &key_tag, &by_key),
        ] {
            assert_eq!(
                tag.as_deref(),
                Some(over(body).as_str()),
                "{who}'s tag, over the bytes it is sent"
            );
        }
        let both_tag = both_tag.expect("Both's tag");
        let admin_tag = admin_tag.expect("the administrator's tag");
        let key_tag = key_tag.expect("the key's tag");

        let (status, _, _) = fetched(&merged, Reader::Both, &uri, Some(&both_tag)).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED, "Both, its own tag");
        let (status, _, _) = fetched(&merged, Reader::Administrator, &uri, Some(&admin_tag)).await;
        assert_eq!(
            status,
            StatusCode::NOT_MODIFIED,
            "the administrator, its own tag"
        );
        let (status, _, _) = fetched_by_key(&merged, &uri, Some(&key_tag)).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED, "the key, its own tag");

        let (status, _, _) = fetched(&merged, Reader::Both, &uri, Some(&admin_tag)).await;
        assert_eq!(status, StatusCode::OK, "Both, the administrator's tag");
        let (status, _, _) = fetched(&merged, Reader::Administrator, &uri, Some(&both_tag)).await;
        assert_eq!(status, StatusCode::OK, "the administrator, Both's tag");
    }

    #[test]
    fn with_file_names_keeps_only_each_paths_last_part() {
        let given = Sidecar::from_json(
            r#"{
              "taiyaku": 1,
              "work": { "title": "Foo", "reading": "rtl" },
              "editions": {
                "en": { "label": "English", "members": [
                  { "path": "/srv/Manga/Foo en/Foo v01.cbz", "sha256": "sha-of-en-v01", "pages": 4 }
                ], "spreads": [2] },
                "raw": { "label": "Raw", "members": [
                  { "path": "/srv/Manga/Foo/Foo v01.cbz", "sha256": "sha-of-v01", "pages": 4 },
                  { "path": "../../Manga/Foo v02.cbz", "sha256": "sha-of-v02", "pages": 3 },
                  { "path": "Foo v03.cbz", "sha256": "sha-of-v03", "pages": 2 },
                  { "path": "/", "sha256": "sha-of-a-root", "pages": 1 },
                  { "path": "..", "sha256": "sha-of-a-parent", "pages": 1 }
                ], "spreads": [] }
              },
              "hub": "en",
              "alignments": [
                { "from": "raw", "to": "en", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] }
              ]
            }"#,
        )
        .expect("the literal parses");
        assert_eq!(
            given.alignments.len(),
            1,
            "the literal carries an alignment"
        );

        let sent = with_file_names(&given);
        let paths = |doc: &Sidecar| -> std::collections::BTreeMap<String, Vec<String>> {
            doc.editions
                .iter()
                .map(|(key, edition)| {
                    let paths = edition.members.iter().map(|member| member.path.clone());
                    (key.clone(), paths.collect())
                })
                .collect()
        };
        assert_eq!(
            paths(&sent),
            named(&[
                ("en", &["Foo v01.cbz"]),
                (
                    "raw",
                    &["Foo v01.cbz", "Foo v02.cbz", "Foo v03.cbz", "", ""]
                ),
            ])
        );
        let blanked = |doc: &Sidecar| {
            let mut doc = doc.clone();
            for edition in doc.editions.values_mut() {
                for member in &mut edition.members {
                    member.path.clear();
                }
            }
            doc.to_json()
        };
        assert_eq!(
            blanked(&sent),
            blanked(&given),
            "every field but the paths is as given"
        );
    }

    /// A note `id` on `edition`'s `slot`, its words its own.
    fn note(id: &str, edition: &str, slot: u32) -> crate::notes::Note {
        crate::notes::Note {
            id: id.into(),
            edition: edition.into(),
            slot,
            region: None,
            color: None,
            body: format!("{id}'s words"),
            created: "2026-11-03T10:00:00Z".into(),
            updated: "2026-11-03T10:00:00Z".into(),
            stale: false,
            at: None,
        }
    }

    /// Manga's notes as the store holds them, in the full numbering: n1 on
    /// `raw`'s page 1 (part1), n2 on page 3 (part2, which Manga does not see),
    /// n3 on page 6 (part3's second, Manga's page 3), and n4 on `en`.
    fn mangas_notes(merged: &EditionMergedFromTwoLibraries) -> crate::notes::Notes {
        crate::notes::Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: merged.slug.clone(),
            notes: vec![
                note("n1", "raw", 1),
                note("n2", "raw", 3),
                note("n3", "raw", 6),
                note("n4", "en", 0),
            ],
        }
    }

    fn hold_notes(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        notes: &crate::notes::Notes,
    ) {
        crate::notes::store(
            &merged.state.library,
            &the_work(merged),
            merged.user_id(reader),
            notes,
            &the_sidecar(merged),
            crate::works::Mint::Bridge,
        )
        .expect("stores the notes");
    }

    fn held_notes(merged: &EditionMergedFromTwoLibraries, reader: Reader) -> crate::notes::Notes {
        crate::notes::load(
            &merged.state.library,
            &the_work(merged),
            merged.user_id(reader),
            &the_sidecar(merged),
        )
        .expect("reads the notes")
        .expect("notes are held")
    }

    /// `reader`'s notes read in the store's own numbering, whatever document
    /// the server holds: what is stored rather than what a reader is sent.
    fn stored_notes(merged: &EditionMergedFromTwoLibraries, reader: Reader) -> crate::notes::Notes {
        let work = the_work(merged);
        let conn = merged.state.library.conn();
        let work_id = crate::works::find_row(&conn, &work)
            .expect("finds the work's row")
            .expect("the work has a row");
        taiyaku_library::state::notes(&conn, work_id, merged.user_id(reader))
            .expect("reads the notes")
    }

    /// Each note of a notes body: its id, edition and slot.
    fn anchors(body: &str) -> Vec<(String, String, u32)> {
        let doc: crate::notes::Notes = serde_json::from_str(body).expect("a notes document");
        doc.notes
            .into_iter()
            .map(|note| (note.id, note.edition, note.slot))
            .collect()
    }

    fn anchor(id: &str, edition: &str, slot: u32) -> (String, String, u32) {
        (id.to_string(), edition.to_string(), slot)
    }

    async fn put(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        uri: &str,
        if_match: Option<&str>,
        body: String,
    ) -> (StatusCode, Option<String>, String) {
        let mut request = Request::put(uri).header(header::CONTENT_TYPE, "application/json");
        if let Some(tag) = if_match {
            request = request.header(header::IF_MATCH, tag);
        }
        sent(merged, reader, request.body(Body::from(body)).unwrap()).await
    }

    fn notes_uri(merged: &EditionMergedFromTwoLibraries) -> String {
        format!("/api/works/{}/notes", merged.slug)
    }

    fn progress_uri(merged: &EditionMergedFromTwoLibraries) -> String {
        format!("/api/works/{}/progress", merged.slug)
    }

    /// The tag names the stored document and Manga's numbering of it (spec
    /// §3.8, decision 11): neither the body's nor the stored document's alone,
    /// and the same for every read under the same view, so a write with it is
    /// accepted.
    #[tokio::test]
    async fn manga_alone_is_sent_its_notes_on_the_pages_it_sees_under_a_tag_naming_its_numbering() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(&merged, Reader::MangaOnly, &mangas_notes(&merged));
        let uri = notes_uri(&merged);

        let (status, tag, body) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            anchors(&body),
            [
                anchor("n1", "raw", 1),
                anchor("n3", "raw", 3),
                anchor("n4", "en", 0)
            ],
            "n2 is on part2, which Manga does not see; n3 is on its page 3: {body}"
        );
        let tag = tag.expect("a notes read carries a tag");
        let stored = crate::notes::to_json(&held_notes(&merged, Reader::MangaOnly)).unwrap();
        assert_ne!(tag, notes_etag(&stored), "not the stored document's alone");
        assert_ne!(tag, notes_etag(&body), "not the body's");

        let (_, again, _) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        assert_eq!(
            again.as_deref(),
            Some(tag.as_str()),
            "the same view, the same tag"
        );
        let (status, _, _) = fetched(&merged, Reader::MangaOnly, &uri, Some(&tag)).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        let (status, _, answered) = put(&merged, Reader::MangaOnly, &uri, Some(&tag), body).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            unanchored(held_notes(&merged, Reader::MangaOnly)),
            mangas_notes(&merged)
        );
    }

    /// A write Manga read while `raw` was narrowed is refused once it sees
    /// `raw` whole, and stores nothing: applied in the whole numbering it
    /// would put n3 on part2's page and delete n2 (spec §3.8, decision 11).
    #[tokio::test]
    async fn a_notes_write_manga_read_before_it_was_granted_kids_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(&merged, Reader::MangaOnly, &mangas_notes(&merged));
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::MangaOnly, &uri, None).await;

        taiyaku_library::account::grant_library(
            &merged.state.library.conn(),
            merged.user_id(Reader::MangaOnly),
            80,
        )
        .expect("grants Kids");
        let (whole, _) = document(&merged, Reader::MangaOnly).await;
        assert_eq!(
            members(&whole, "raw").len(),
            3,
            "the fixture: Manga now sees `raw` whole: {whole}"
        );

        let (status, _, answered) =
            put(&merged, Reader::MangaOnly, &uri, tag.as_deref(), body).await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED, "{answered}");
        assert_eq!(
            unanchored(held_notes(&merged, Reader::MangaOnly)),
            mangas_notes(&merged),
            "nothing is stored"
        );
    }

    /// The notes tag names a narrowed edition's visible members, so a scan
    /// that replaces one, with `raw` still narrowed to two, changes it, and
    /// so does seeing another two of its three; the same view gives the same
    /// tag (spec §3.8, decision 11).
    #[tokio::test]
    async fn the_notes_tag_names_a_narrowed_editions_visible_members() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let tag = |sidecar: &Arc<Sidecar>, raw: &[bool]| {
            let listed: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
            let flags = with_raw(sidecar, read_as(&listed, raw));
            crate::view::View::build(Arc::clone(sidecar), Some(&flags))
                .expect("visible")
                .etag_naming_numbering("{}")
        };
        let sidecar = the_sidecar(&merged);
        let mut rescanned = (*sidecar).clone();
        rescanned.editions.get_mut("raw").expect("raw").members[2].sha256 = "replaced".into();
        let mangas = [true, false, true];
        assert_eq!(
            tag(&sidecar, &mangas),
            tag(&sidecar, &mangas),
            "the same view, the same tag"
        );
        assert_ne!(
            tag(&sidecar, &mangas),
            tag(&Arc::new(rescanned), &mangas),
            "part3 replaced"
        );
        assert_ne!(
            tag(&sidecar, &mangas),
            tag(&sidecar, &[true, true, false]),
            "two of three either way, but part2 seen in place of part3"
        );
        // Part2 is hidden from Manga and before part3, so what Manga is sent
        // is the same in both, and only where part3 sits in full moves.
        let mut resized = (*sidecar).clone();
        resized.editions.get_mut("raw").expect("raw").members[1].pages = 1;
        assert_ne!(
            tag(&sidecar, &mangas),
            tag(&Arc::new(resized), &mangas),
            "part2 resized, moving part3's pages"
        );
        let mut spread = (*sidecar).clone();
        spread.editions.get_mut("raw").expect("raw").spreads.push(3);
        assert_ne!(
            tag(&sidecar, &mangas),
            tag(&Arc::new(spread), &mangas),
            "a spread on part2's page 3, moving part3's slots and none of its pages"
        );
    }

    /// The notes tag is keyed by every member the account is not sent: part2,
    /// hidden in a narrowed `raw`, and the member of `fr` when `fr` is hidden.
    /// So replacing either by a file of as many pages, which moves no page the
    /// account is sent, changes it (spec §3.8, decision 11).
    #[tokio::test]
    async fn the_notes_tag_is_keyed_by_the_members_the_account_is_not_sent() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let tag = |sidecar: &Sidecar, raw: &[bool], fr: bool| {
            let listed: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
            let mut flags = with_raw(sidecar, read_as(&listed, raw));
            if !fr {
                flags.remove("fr");
            }
            crate::view::View::build(Arc::new(sidecar.clone()), Some(&flags))
                .expect("visible")
                .etag_naming_numbering("{}")
        };
        let sidecar = (*the_sidecar(&merged)).clone();
        let replaced = |edition: &str, member: usize| {
            let mut rescanned = sidecar.clone();
            let edition = rescanned.editions.get_mut(edition).expect("the edition");
            edition.members[member].sha256 = "replaced".into();
            rescanned
        };
        let mangas = [true, false, true];
        assert_ne!(
            tag(&sidecar, &mangas, true),
            tag(&replaced("raw", 1), &mangas, true),
            "part2 replaced"
        );
        let whole = [true, true, true];
        assert_ne!(
            tag(&sidecar, &whole, false),
            tag(&replaced("fr", 0), &whole, false),
            "fr's member replaced, with `raw` sent whole and `fr` hidden"
        );
    }

    /// A page route reads from the sidecar its flags were checked against: a
    /// refresh landing between the check and the read, here reversing `raw`'s
    /// members so that part2 holds full page 5, does not send Manga part2's
    /// page as its page 2, and `en`, sent whole, keeps the one member it was
    /// checked with when the refresh adds part2 to it (spec §3.8, fail closed).
    #[tokio::test]
    async fn a_page_is_read_in_the_sidecar_its_flags_were_checked_against() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let uri = format!("/api/works/{}/raw/pages/5", merged.slug);
        let page_5 = merged.get(Reader::Administrator, &uri, None).await;
        assert_eq!(page_5.status(), StatusCode::OK, "{uri}");

        let sidecar = the_sidecar(&merged);
        let raw: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
        let flags = with_raw(&sidecar, read_as(&raw, &[true, false, true]));
        let checked = |edition: &str| {
            crate::view::EditionView::build(the_work(&merged), edition, Some(&flags))
                .expect("Manga sees it")
        };
        let (raw_view, en_view) = (checked("raw"), checked("en"));

        let mut refreshed = (*sidecar).clone();
        let editions = &mut refreshed.editions;
        editions.get_mut("raw").expect("raw").members.reverse();
        let part2 = editions["raw"].members[1].clone();
        editions.get_mut("en").expect("en").members.push(part2);
        the_work(&merged)
            .reproject(|| Ok(refreshed))
            .expect("refreshes");
        assert_eq!(
            the_sidecar(&merged).editions["raw"]
                .locate(5)
                .map(|(member, _)| member),
            Some(1),
            "the fixture: in the refreshed sidecar, full page 5 is part2's"
        );

        let served = serve_page(
            Arc::clone(&merged.state),
            raw_view,
            2,
            None,
            None,
            &axum::http::HeaderMap::new(),
        )
        .await;
        assert_eq!(served.status(), StatusCode::OK);
        let body = axum::body::to_bytes(served.into_body(), usize::MAX)
            .await
            .expect("reads the page");
        assert!(
            body[..] == page_5.body()[..],
            "Manga's page 2 is part3's first, as the flags were checked"
        );
        assert_eq!(
            en_view.edition().map(|edition| edition.members.len()),
            Some(1),
            "`en` as it was checked"
        );
    }

    /// A write Manga read before a scan resized part2, which it does not see
    /// and which comes before part3, is refused and stores nothing. The
    /// resize itself moves n3's slot, part3's second page, from 6 to 4, and
    /// leaves n2 `stale` at its stored 3, now part3's first page, which Manga
    /// is shown (O244): applied, the write would delete n2. The
    /// administrator's tag is still the stored document's, as read in the
    /// document the server now holds.
    #[tokio::test]
    async fn a_notes_write_manga_read_before_a_hidden_member_was_resized_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(&merged, Reader::MangaOnly, &mangas_notes(&merged));
        hold_notes(&merged, Reader::Administrator, &mangas_notes(&merged));
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::MangaOnly, &uri, None).await;

        let mut rescanned = (*the_sidecar(&merged)).clone();
        rescanned.editions.get_mut("raw").expect("raw").members[1].pages = 1;
        the_work(&merged)
            .reproject(|| Ok(rescanned))
            .expect("holds the rescanned document");
        let (doc, _) = document(&merged, Reader::MangaOnly).await;
        assert_eq!(
            members(&doc, "raw"),
            files(&[("Foo v01 part1.cbz", 2), ("Foo v01 part3.cbz", 4)]),
            "the fixture: what Manga is sent of `raw` has not changed: {doc}"
        );

        let (status, _, answered) =
            put(&merged, Reader::MangaOnly, &uri, tag.as_deref(), body).await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED, "{answered}");
        assert_eq!(
            stored_notes(&merged, Reader::MangaOnly),
            mangas_notes(&merged),
            "nothing is stored"
        );
        let (_, after, _) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let stored = crate::notes::to_json(&held_notes(&merged, Reader::Administrator)).unwrap();
        assert_eq!(after, Some(notes_etag(&stored)), "the administrator's tag");
    }

    /// A layout change that moves a note's slot or marks it `stale` changes
    /// the notes tag of each account holding that note (O244), so a write read
    /// before it is refused and takes O240's merge path. part2 is resized to
    /// one page in the document the server holds: n3, on part3's second page,
    /// goes from slot 6 to 4, and n2, on part2's second page, is `stale` at its
    /// stored 3. The administrator is sent every edition whole, so its tag is
    /// over the notes alone and changes because n3 moved and n2 is marked;
    /// Manga's names its numbering besides.
    #[tokio::test]
    async fn a_resize_that_moves_a_note_changes_every_accounts_notes_tag() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(&merged, Reader::MangaOnly, &mangas_notes(&merged));
        hold_notes(&merged, Reader::Administrator, &mangas_notes(&merged));
        let uri = notes_uri(&merged);
        let (_, administrators, before) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let (_, mangas, _) = fetched(&merged, Reader::MangaOnly, &uri, None).await;

        let mut rescanned = (*the_sidecar(&merged)).clone();
        rescanned.editions.get_mut("raw").expect("raw").members[1].pages = 1;
        the_work(&merged)
            .reproject(|| Ok(rescanned))
            .expect("holds the rescanned document");

        let (_, administrators_now, after) =
            fetched(&merged, Reader::Administrator, &uri, None).await;
        let (_, mangas_now, _) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        let placed = |body: &str| {
            serde_json::from_str::<crate::notes::Notes>(body)
                .expect("a notes document")
                .notes
                .into_iter()
                .map(|note| (note.id, note.slot, note.stale))
                .collect::<Vec<_>>()
        };
        let on = |id: &str, slot: u32, stale: bool| (id.to_string(), slot, stale);
        assert_eq!(
            (placed(&before), placed(&after)),
            (
                vec![
                    on("n1", 1, false),
                    on("n2", 3, false),
                    on("n3", 6, false),
                    on("n4", 0, false)
                ],
                vec![
                    on("n1", 1, false),
                    on("n2", 3, true),
                    on("n3", 4, false),
                    on("n4", 0, false)
                ],
            ),
            "the fixture: the administrator is sent n3 moved and n2 stale"
        );
        assert!(administrators.is_some(), "a notes read carries a tag");
        assert_ne!(
            administrators_now, administrators,
            "the administrator's tag"
        );
        assert_ne!(
            mangas_now, mangas,
            "Manga's tag, which also names the numbering the resize changed"
        );
        let (status, _, answered) = put(
            &merged,
            Reader::Administrator,
            &uri,
            administrators.as_deref(),
            before,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::PRECONDITION_FAILED,
            "the administrator's write read before the resize: {answered}"
        );
    }

    /// Manga alone cannot find its notes tag by searching what it is sent: the
    /// hidden slots before each member it sees are a few small numbers, so a
    /// tag over them and what it is sent alone would give part2's size away
    /// (spec §3.8, decision 11). Each guess has `View::etag_naming_numbering`'s
    /// layout over the work document and the notes body, each member's path as
    /// stored (the tag hashes it: the strongest guess), whichever editions are
    /// narrowed, and with and without raw's count of hidden members after them,
    /// as a tag naming what is hidden by anything but its hashes would.
    #[tokio::test]
    async fn manga_alones_notes_tag_cannot_be_found_from_what_it_is_sent() {
        use sha2::{Digest, Sha256};
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let whole = the_administrators_document(&merged).await;
        let (doc, _) = document(&merged, Reader::MangaOnly).await;
        assert_eq!(
            members(&doc, "raw"),
            files(&[("Foo v01 part1.cbz", 2), ("Foo v01 part3.cbz", 4)]),
            "the fixture: part2, hidden, sits between the two Manga sees: {doc}"
        );
        assert_eq!(doc["editions"]["raw"]["spreads"], serde_json::json!([]));
        let (status, tag, body) =
            fetched(&merged, Reader::MangaOnly, &notes_uri(&merged), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let tag = tag.expect("a notes read carries a tag");

        let mut keys = editions(&doc);
        keys.sort();
        let stored = |key: &str, sha256: &str| -> String {
            whole["editions"][key]["members"]
                .as_array()
                .expect("members")
                .iter()
                .find(|member| member["sha256"] == sha256)
                .and_then(|member| member["path"].as_str())
                .unwrap_or_else(|| panic!("`{key}` stores no member {sha256}"))
                .to_string()
        };
        let guess = |narrowed: usize, hidden: [u64; 2], counted: Option<u64>| {
            let mut hasher = Sha256::new();
            let mut field = |bytes: &[u8]| {
                hasher.update((bytes.len() as u64).to_le_bytes());
                hasher.update(bytes);
            };
            for (at, key) in keys.iter().enumerate() {
                field(key.as_bytes());
                if narrowed & (1 << at) == 0 {
                    field(&0_u64.to_le_bytes());
                    continue;
                }
                let sent = doc["editions"][key]["members"].as_array().expect("members");
                field(&(sent.len() as u64).to_le_bytes());
                let mut pages = Vec::new();
                for member in sent {
                    let sha256 = member["sha256"].as_str().expect("a sha256");
                    field(sha256.as_bytes());
                    field(stored(key, sha256).as_bytes());
                    pages.push(member["pages"].as_u64().expect("pages"));
                }
                // Raw's guesses are the hidden pages before each member; any
                // other edition is guessed narrowed to itself.
                let mut full = Vec::new();
                let mut at_full = 0;
                for (index, count) in pages.iter().enumerate() {
                    if key == "raw" {
                        at_full += hidden.get(index).copied().unwrap_or(0);
                    }
                    for _ in 0..*count {
                        full.push(at_full);
                        at_full += 1;
                    }
                }
                field(&(full.len() as u64).to_le_bytes());
                for slot in full {
                    field(&slot.to_le_bytes());
                }
            }
            if let Some(count) = counted {
                field(b"raw");
                field(&count.to_le_bytes());
            }
            field(body.as_bytes());
            crate::notes::quoted_hex(&hasher.finalize())
        };

        let mut found = Vec::new();
        let counts = [None, Some(0), Some(1), Some(2), Some(3)];
        for narrowed in 0..1_usize << keys.len() {
            for before_part1 in 0..40 {
                for before_part3 in 0..40 {
                    for counted in counts {
                        if guess(narrowed, [before_part1, before_part3], counted) == tag {
                            found.push((narrowed, before_part1, before_part3, counted));
                        }
                    }
                }
            }
        }
        assert_eq!(found, [], "a guess at part2's 3 pages matched the tag");
    }

    /// `Access::Every`'s notes tag is the stored document's, on a read and on
    /// a write's answer: it is never narrowed, so it has no numbering to name.
    #[tokio::test]
    async fn the_administrators_notes_tag_is_the_stored_documents() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(&merged, Reader::Administrator, &mangas_notes(&merged));
        let uri = notes_uri(&merged);
        let stored = || crate::notes::to_json(&held_notes(&merged, Reader::Administrator)).unwrap();

        let (_, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(body, stored(), "sent the stored document");
        assert_eq!(tag, Some(notes_etag(&stored())));

        let mut doc: crate::notes::Notes = serde_json::from_str(&body).expect("notes");
        doc.notes.push(note("n5", "raw", 2));
        let (status, answered_tag, answered) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(answered, stored(), "answered the stored document");
        assert_eq!(answered_tag, Some(notes_etag(&stored())));
    }

    #[tokio::test]
    async fn manga_alones_notes_write_is_stored_translated_and_keeps_the_note_it_is_not_sent() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(&merged, Reader::MangaOnly, &mangas_notes(&merged));
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        let mut doc: crate::notes::Notes = serde_json::from_str(&body).expect("notes");
        doc.notes.push(note("n5", "raw", 2));

        let (status, answered_tag, answered) = put(
            &merged,
            Reader::MangaOnly,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            unanchored(held_notes(&merged, Reader::MangaOnly)),
            crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![
                    note("n1", "raw", 1),
                    note("n2", "raw", 3),
                    note("n3", "raw", 6),
                    note("n4", "en", 0),
                    note("n5", "raw", 5),
                ],
            },
            "n2 kept as it was, n3 and n5 in the full numbering"
        );
        let (_, next_tag, next) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        assert_eq!(
            answered_tag, next_tag,
            "the answer's ETag is the next read's"
        );
        assert_eq!(answered, next, "the answer is what Manga is sent next");
    }

    #[tokio::test]
    async fn a_note_given_the_id_of_one_manga_alone_is_not_sent_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(&merged, Reader::MangaOnly, &mangas_notes(&merged));
        let uri = notes_uri(&merged);
        let (_, tag, _) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        let doc = crate::notes::Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: merged.slug.clone(),
            notes: vec![
                note("n1", "raw", 1),
                note("n2", "raw", 0),
                note("n3", "raw", 3),
                note("n4", "en", 0),
            ],
        };

        let (status, _, body) = put(
            &merged,
            Reader::MangaOnly,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(body.contains("`n2`"), "the refusal names the note: {body}");
        assert_eq!(
            unanchored(held_notes(&merged, Reader::MangaOnly)),
            mangas_notes(&merged),
            "nothing is stored"
        );
    }

    /// Kids alone is not sent `en`, so its note k3 there reaches `as_stored`
    /// unchanged and is compared with the note held. k3's page is lost, so it
    /// is read `stale` (O244). A write that echoes `"stale": true` and one
    /// that leaves it out both send k3 as it was: each is accepted and the
    /// store answers k3 as before. Not a hidden page of `raw`: Kids'
    /// numbering cannot name one, so such a note is placed elsewhere and
    /// refused whatever `stale` says.
    #[tokio::test]
    async fn kids_alone_echoing_or_dropping_stale_on_a_note_it_is_not_sent_is_accepted() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::KidsOnly,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![
                    note("k1", "raw", 2),
                    note("k2", "raw", 4),
                    note("k3", "en", 1),
                ],
            },
        );
        merged
            .state
            .library
            .conn()
            .execute(
                "UPDATE note SET at_sha256 = 'held-by-no-member' \
                  WHERE id = 'k3' AND user_id = ?1",
                [merged.user_id(Reader::KidsOnly)],
            )
            .expect("loses k3's page");
        let k3 = |stale: bool| crate::notes::Note {
            stale,
            ..note("k3", "en", 1)
        };
        let held_k3 = || {
            stored_notes(&merged, Reader::KidsOnly)
                .notes
                .into_iter()
                .find(|note| note.id == "k3")
        };
        assert_eq!(held_k3(), Some(k3(true)), "the fixture: k3 is read stale");
        let uri = notes_uri(&merged);

        for stale in [true, false] {
            let (_, tag, body) = fetched(&merged, Reader::KidsOnly, &uri, None).await;
            assert_eq!(
                anchors(&body),
                [anchor("k1", "raw", 0), anchor("k2", "raw", 2)],
                "the fixture: Kids is sent its notes on part2 alone"
            );
            let mut doc: crate::notes::Notes = serde_json::from_str(&body).expect("notes");
            doc.notes.push(k3(stale));
            let sent = serde_json::to_string(&doc).unwrap();
            assert_eq!(
                sent.contains(r#""stale":true"#),
                stale,
                "the fixture: {sent}"
            );

            let (status, _, answered) =
                put(&merged, Reader::KidsOnly, &uri, tag.as_deref(), sent).await;
            assert_eq!(status, StatusCode::OK, "`stale` sent {stale}: {answered}");
            assert_eq!(
                held_k3(),
                Some(k3(true)),
                "k3 as before, `stale` sent {stale}"
            );
        }
    }

    /// While a note's page is gone, a write is checked against the tag the
    /// read gave, over a document naming the note `stale`, and is answered
    /// with what the next read sends: the note still `stale`, as the store
    /// answers it (O244).
    #[tokio::test]
    async fn a_notes_write_over_a_stale_note_is_answered_what_the_next_read_sends() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n2", "raw", 3)],
            },
        );
        let page = |file: &str, page| taiyaku_core::MemberPage {
            sha256: digest(&merged, file),
            nth: 0,
            page,
            half: None,
        };
        let (part1, part2) = (
            page("m/Foo v01 part1.cbz", 0),
            page("k/Foo v01 part2.cbz", 1),
        );
        remove_kids_directory(&merged);
        reread(&merged);
        let n2 = crate::notes::Note {
            stale: true,
            at: Some(part2),
            ..note("n2", "raw", 3)
        };
        let n5 = crate::notes::Note {
            at: Some(part1),
            ..note("n5", "raw", 0)
        };
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let mut doc: crate::notes::Notes = serde_json::from_str(&body).expect("notes");
        assert_eq!(
            doc.notes,
            std::slice::from_ref(&n2),
            "the fixture: part2 is gone, so n2 is sent stale: {body}"
        );
        doc.notes.push(note("n5", "raw", 0));

        let (status, answered_tag, answered) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        let (_, next_tag, next) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert!(answered_tag.is_some(), "a notes write carries a tag");
        assert_eq!(
            answered_tag, next_tag,
            "the answer's ETag is the next read's"
        );
        assert_eq!(answered, next, "the answer is what is sent next");
        let answered: crate::notes::Notes = serde_json::from_str(&answered).expect("notes");
        assert_eq!(answered.notes, [n2, n5], "n2 is answered stale");
    }

    fn hold_position(merged: &EditionMergedFromTwoLibraries, reader: Reader, slot: u32) {
        crate::progress::store(
            &merged.state.library,
            &the_work(merged),
            merged.user_id(reader),
            &crate::Progress {
                edition: "raw".into(),
                slot,
            },
            &the_sidecar(merged),
            taiyaku_library::state::Arrival::next(),
            crate::works::Mint::Bridge,
        )
        .expect("stores a position");
    }

    /// `reader`'s positions as it is sent them: each edition and slot, or
    /// `None` for a 204.
    async fn positions(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
    ) -> Option<Vec<(String, u64)>> {
        let (status, _, body) = fetched(merged, reader, &progress_uri(merged), None).await;
        if status == StatusCode::NO_CONTENT {
            return None;
        }
        assert_eq!(status, StatusCode::OK, "{body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("a progress document");
        Some(
            doc["positions"]
                .as_array()
                .expect("positions is an array")
                .iter()
                .map(|p| {
                    (
                        p["edition"].as_str().expect("an edition").to_string(),
                        p["slot"].as_u64().expect("a slot"),
                    )
                })
                .collect(),
        )
    }

    #[tokio::test]
    async fn a_position_on_a_hidden_page_reads_as_the_nearest_page_seen_before_it() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_position(&merged, Reader::MangaOnly, 3);
        assert_eq!(
            positions(&merged, Reader::MangaOnly).await,
            Some(vec![("raw".to_string(), 1)]),
            "full page 3 is part2's; Manga's nearest before it is page 1"
        );
    }

    #[tokio::test]
    async fn a_position_before_every_page_kids_sees_reads_as_its_first() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_position(&merged, Reader::KidsOnly, 0);
        assert_eq!(
            positions(&merged, Reader::KidsOnly).await,
            Some(vec![("raw".to_string(), 0)]),
            "Kids sees no page before part2's, so its first"
        );
        hold_position(&merged, Reader::KidsOnly, 1);
        assert_eq!(
            positions(&merged, Reader::KidsOnly).await,
            Some(vec![("raw".to_string(), 0)]),
            "full page 1 is part1's, before any page Kids sees"
        );
    }

    #[tokio::test]
    async fn a_position_past_a_narrowed_editions_end_is_not_sent() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_position(&merged, Reader::MangaOnly, 9);
        assert_eq!(positions(&merged, Reader::MangaOnly).await, None);
    }

    #[tokio::test]
    async fn a_position_manga_alone_writes_is_stored_in_the_full_numbering() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let uri = progress_uri(&merged);
        let (status, _, body) = put(
            &merged,
            Reader::MangaOnly,
            &uri,
            None,
            r#"{"edition":"raw","slot":2}"#.to_string(),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let held = crate::progress::load(
            &merged.state.library,
            &the_work(&merged),
            merged.user_id(Reader::MangaOnly),
            &the_sidecar(&merged),
        )
        .expect("reads the positions");
        let held: Vec<(&str, u32)> = held.iter().map(|p| (p.edition.as_str(), p.slot)).collect();
        assert_eq!(held, [("raw", 5)], "Manga's page 2 is full page 5");
        assert_eq!(
            positions(&merged, Reader::MangaOnly).await,
            Some(vec![("raw".to_string(), 2)])
        );

        let (status, _, body) = put(
            &merged,
            Reader::MangaOnly,
            &uri,
            None,
            r#"{"edition":"raw","slot":6}"#.to_string(),
        )
        .await;
        assert_eq!(
            (status, body.as_str()),
            (
                StatusCode::NOT_FOUND,
                format!(
                    "`raw` has 6 slots in work `{}`; there is no slot 6",
                    merged.slug
                )
                .as_str()
            ),
            "checked against the six pages Manga sees"
        );
    }

    /// A stored row's slot, and the digest, occurrence and page it is
    /// anchored to.
    type Anchored = (u32, Option<String>, Option<u32>, Option<u32>);

    /// `reader`'s rows in `table` as stored, by slot.
    fn anchored(
        merged: &EditionMergedFromTwoLibraries,
        table: &str,
        reader: Reader,
    ) -> Vec<Anchored> {
        merged
            .state
            .library
            .conn()
            .prepare(&format!(
                "SELECT slot, at_sha256, at_nth, at_page FROM {table}
                  WHERE user_id = ?1 ORDER BY slot"
            ))
            .and_then(|mut stmt| {
                stmt.query_map([merged.user_id(reader)], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .collect()
            })
            .expect("reads the anchored rows")
    }

    /// O272 under a narrowed view: over a store in a file, Kids alone's
    /// position and note are written on the save's own connection in the full
    /// numbering, anchored to the part2 pages Kids read them on (O244), and
    /// the notes write is answered with what the next read sends.
    #[tokio::test]
    async fn kids_alones_saves_to_a_store_in_a_file_are_anchored_to_the_pages_it_read() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries_in_a_file();
        assert!(
            merged.state.library.path().is_some(),
            "the fixture: a store in a file, so a save has a connection of its own"
        );
        the_administrators_document(&merged).await;
        let part2: String = merged
            .state
            .library
            .conn()
            .query_row(
                "SELECT sha256 FROM member WHERE path LIKE '%part2.cbz'",
                [],
                |r| r.get(0),
            )
            .expect("part2's digest");

        let (status, _, body) = put(
            &merged,
            Reader::KidsOnly,
            &progress_uri(&merged),
            None,
            r#"{"edition":"raw","slot":1}"#.to_string(),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let uri = notes_uri(&merged);
        let (_, tag, _) = fetched(&merged, Reader::KidsOnly, &uri, None).await;
        let notes = crate::notes::Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: merged.slug.clone(),
            notes: vec![note("k", "raw", 2)],
        };
        let (status, answered_tag, answered) = put(
            &merged,
            Reader::KidsOnly,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&notes).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");

        assert_eq!(
            anchored(&merged, "progress", Reader::KidsOnly),
            [(3, Some(part2.clone()), Some(0), Some(1))],
            "Kids' slot 1 is full slot 3, part2's page 1"
        );
        assert_eq!(
            anchored(&merged, "note", Reader::KidsOnly),
            [(4, Some(part2), Some(0), Some(2))],
            "Kids' slot 2 is full slot 4, part2's page 2"
        );
        let (_, next_tag, next) = fetched(&merged, Reader::KidsOnly, &uri, None).await;
        assert!(answered_tag.is_some(), "a notes write carries a tag");
        assert_eq!(
            (answered_tag, answered),
            (next_tag, next),
            "the answer and its ETag are the next read's"
        );
    }

    /// The administrator's position at `slot` of `raw`, written through the
    /// route.
    async fn put_raw_as_the_administrator(merged: &EditionMergedFromTwoLibraries, slot: u32) {
        let (status, _, body) = put(
            merged,
            Reader::Administrator,
            &progress_uri(merged),
            None,
            format!(r#"{{"edition":"raw","slot":{slot}}}"#),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    }

    /// Kids' directory `k` (part2) removed from the store. The server's copy
    /// of the work stays as it was until [`reread`].
    fn remove_kids_directory(merged: &EditionMergedFromTwoLibraries) {
        let conn = merged.state.library.conn();
        let kids: i64 = conn
            .query_row("SELECT id FROM root WHERE path LIKE '%/k'", [], |r| {
                r.get(0)
            })
            .expect("Kids' directory");
        taiyaku_library::scan::remove_root(&conn, kids).expect("removes k");
    }

    /// The server's copy of the work re-read from the store, as its
    /// refresher re-reads it.
    fn reread(merged: &EditionMergedFromTwoLibraries) {
        let conn = merged.state.library.conn();
        let work_id: i64 = conn
            .query_row("SELECT id FROM work", [], |r| r.get(0))
            .expect("the one work");
        the_work(merged)
            .reproject(|| taiyaku_library::project::sidecar(&conn, work_id))
            .expect("re-reads the work");
    }

    /// O244: a position on part3's second page (raw slot 6) is read on that
    /// page after Kids' directory, which holds part2, is removed and the
    /// server has re-read the work: part3's pages move down three slots.
    #[tokio::test]
    async fn a_position_stays_on_its_page_when_a_directory_before_it_goes() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        put_raw_as_the_administrator(&merged, 6).await;

        remove_kids_directory(&merged);
        reread(&merged);

        let (doc, _) = document(&merged, Reader::Administrator).await;
        assert_eq!(
            members(&doc, "raw"),
            files(&[("Foo v01 part1.cbz", 2), ("Foo v01 part3.cbz", 4)]),
            "the fixture: the document sent is the store's, without part2"
        );
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("raw".to_string(), 3)]),
            "part3's second page, after part1's two"
        );
    }

    /// O244: until the server re-reads the work, a position is read in the
    /// numbering of the document it still sends, so the slot names the page
    /// the position was written on in that document.
    #[tokio::test]
    async fn a_position_is_read_in_the_numbering_the_server_sends_until_it_re_reads_the_work() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        put_raw_as_the_administrator(&merged, 6).await;

        remove_kids_directory(&merged);
        let (doc, _) = document(&merged, Reader::Administrator).await;
        assert_eq!(
            members(&doc, "raw"),
            files(&[
                ("Foo v01 part1.cbz", 2),
                ("Foo v01 part2.cbz", 3),
                ("Foo v01 part3.cbz", 4),
            ]),
            "the fixture: the document sent still holds part2"
        );
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("raw".to_string(), 6)]),
            "part3's second page in the document sent, not part2's second (3)"
        );

        reread(&merged);
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("raw".to_string(), 3)]),
            "part3's second page once the document sent is the store's"
        );
    }

    /// O244: a page turn before the server re-reads the work names the page
    /// in the document the reader holds, so it is found again once the server
    /// has re-read it.
    #[tokio::test]
    async fn a_page_turn_before_the_server_re_reads_the_work_keeps_its_page() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;

        remove_kids_directory(&merged);
        put_raw_as_the_administrator(&merged, 7).await;

        reread(&merged);
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("raw".to_string(), 4)]),
            "part3's third page, slot 7 in the document the page was turned in"
        );
    }

    /// O244: until the server re-reads the work, a note is read in the
    /// numbering of the document it still sends, as a position is.
    #[tokio::test]
    async fn a_note_is_read_in_the_numbering_the_server_sends_until_it_re_reads_the_work() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n", "raw", 6)],
            },
        );
        let uri = notes_uri(&merged);

        remove_kids_directory(&merged);
        let (_, _, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(
            anchors(&body),
            vec![anchor("n", "raw", 6)],
            "part3's second page in the document sent, not part2's second (3)"
        );

        reread(&merged);
        let (_, _, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(
            anchors(&body),
            vec![anchor("n", "raw", 3)],
            "part3's second page once the document sent is the store's"
        );
    }

    /// O244: a notes write before the server re-reads the work is read,
    /// tagged and anchored in the document the reader holds, so its notes are
    /// found again once the server has re-read it.
    #[tokio::test]
    async fn a_notes_write_before_the_server_re_reads_the_work_keeps_its_pages() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n1", "raw", 6)],
            },
        );
        let uri = notes_uri(&merged);

        remove_kids_directory(&merged);
        let (_, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let mut doc: crate::notes::Notes = serde_json::from_str(&body).expect("notes");
        doc.notes.push(note("n2", "raw", 7));
        let (status, _, body) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        reread(&merged);
        let (_, _, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(
            anchors(&body),
            vec![anchor("n1", "raw", 3), anchor("n2", "raw", 4)],
            "part3's second and third pages, slots 6 and 7 in the document written in"
        );
    }

    /// One box, at `x` across its page.
    fn a_box(x: f64) -> taiyaku_core::TruthRegion {
        taiyaku_core::TruthRegion {
            x,
            y: 0.1,
            w: 0.2,
            h: 0.2,
            axis: None,
            kind: taiyaku_core::Kind::Dialogue,
            ignore: false,
            text: None,
        }
    }

    /// The truth the store holds, in the full numbering: `raw` labelled on
    /// page 3 (part2, which Manga does not see) and page 5 (part3's first,
    /// Manga's page 2), and `en` on page 0.
    fn the_truth(merged: &EditionMergedFromTwoLibraries) -> taiyaku_core::TruthDoc {
        let sidecar = the_sidecar(merged);
        let entry = |key: &str, pages: &[u32]| taiyaku_core::TruthEdition {
            members: sidecar.editions[key]
                .members
                .iter()
                .map(|member| taiyaku_core::RegionMember {
                    sha256: member.sha256.clone(),
                })
                .collect(),
            pages: pages.iter().map(|&page| (page, vec![a_box(0.1)])).collect(),
        };
        let mut doc = taiyaku_core::TruthDoc::new();
        doc.editions.insert("raw".into(), entry("raw", &[3, 5]));
        doc.editions.insert("en".into(), entry("en", &[0]));
        doc
    }

    fn hold_truth(merged: &EditionMergedFromTwoLibraries, doc: &taiyaku_core::TruthDoc) {
        crate::truth::store(
            &merged.state.library,
            &the_work(merged),
            doc,
            crate::works::Mint::Bridge,
        )
        .expect("stores the truth");
    }

    fn held_truth(merged: &EditionMergedFromTwoLibraries) -> String {
        crate::truth::to_json(
            &crate::truth::load(&merged.state.library, &the_work(merged))
                .expect("reads the truth")
                .expect("truth is held"),
        )
    }

    fn truth_uri(merged: &EditionMergedFromTwoLibraries) -> String {
        format!("/api/works/{}/truth", merged.slug)
    }

    fn truth_editions(body: &str) -> Vec<String> {
        taiyaku_core::TruthDoc::from_json(body)
            .expect("a truth document")
            .editions
            .into_keys()
            .collect()
    }

    /// Manga alone is sent `en`'s truth and not `raw`'s, whose entry is in a
    /// numbering it is not sent, under a tag naming its view; the
    /// administrator's read is the stored document under its own tag.
    #[tokio::test]
    async fn manga_alone_reads_truth_for_en_alone_under_its_numberings_etag() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_truth(&merged, &the_truth(&merged));
        let stored = held_truth(&merged);
        let uri = truth_uri(&merged);

        let (status, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body, stored,
            "the administrator is sent the stored document"
        );
        assert_eq!(
            tag,
            Some(notes_etag(&stored)),
            "under the stored document's tag"
        );

        let (status, tag, body) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(truth_editions(&body), ["en"], "{body}");
        let sidecar = the_sidecar(&merged);
        let listed: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
        let flags = with_raw(&sidecar, read_as(&listed, &[true, false, true]));
        let mangas = crate::view::View::build(Arc::clone(&sidecar), Some(&flags))
            .expect("visible")
            .etag_naming_numbering(&stored);
        assert_eq!(tag, Some(mangas.clone()), "the tag names Manga's view");
        let (status, _, _) = fetched(&merged, Reader::MangaOnly, &uri, Some(&mangas)).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED, "Manga's tag hits");
    }

    /// The view of an account whose flags for `raw`'s members are `raw`, with
    /// `en` and `fr` whole.
    fn viewing_raw(merged: &EditionMergedFromTwoLibraries, raw: &[bool]) -> crate::view::View {
        let sidecar = the_sidecar(merged);
        let listed: Vec<&Member> = sidecar.editions["raw"].members.iter().collect();
        let flags = with_raw(&sidecar, read_as(&listed, raw));
        crate::view::View::build(Arc::clone(&sidecar), Some(&flags)).expect("en is visible")
    }

    /// `truth::as_stored` called directly: only an administrator writes truth
    /// (O58) and is never narrowed, so no route reaches its narrowed path,
    /// kept for a later per-account capability to label (the go-public
    /// design's §5, decision 6). Answers the document to store, or the
    /// refusal's kind and words.
    fn written(
        merged: &EditionMergedFromTwoLibraries,
        view: &crate::view::View,
        incoming: taiyaku_core::TruthDoc,
    ) -> Result<String, (&'static str, String)> {
        crate::truth::as_stored(incoming, &the_truth(merged), view, &merged.slug)
            .map(|doc| doc.to_json())
            .map_err(|why| match why {
                crate::truth::Unwritable::Missing(said) => ("missing", said),
                crate::truth::Unwritable::PartlyVisible(said) => ("partly visible", said),
            })
    }

    /// `doc` with a box added to `en`'s page 4.
    fn with_ens_page_4_labelled(mut doc: taiyaku_core::TruthDoc) -> taiyaku_core::TruthDoc {
        doc.editions
            .get_mut("en")
            .expect("en")
            .pages
            .insert(4, vec![a_box(0.5)]);
        doc
    }

    /// Manga alone writes back what it was sent with `en` changed: the change
    /// is taken, and `raw`'s entry, which it was not sent, kept.
    #[tokio::test]
    async fn manga_alones_truth_keeps_raws_entry_as_stored() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let held = the_truth(&merged);
        let view = viewing_raw(&merged, &[true, false, true]);
        let sent = crate::truth::as_seen(&held, &view);
        assert_eq!(
            sent.editions.keys().collect::<Vec<_>>(),
            ["en"],
            "the fixture: raw is narrowed, and its entry is not sent"
        );

        let doc = with_ens_page_4_labelled(sent);
        let mut expected = held.clone();
        expected
            .editions
            .insert("en".into(), doc.editions["en"].clone());
        assert_eq!(
            written(&merged, &view, doc),
            Ok(expected.to_json()),
            "en's change taken and raw's entry kept"
        );
    }

    /// With `raw` hidden, a changed entry for it is refused in the words, and
    /// the shape, an edition the work does not have is refused in (O60), and
    /// a write of what was sent keeps raw's entry.
    #[tokio::test]
    async fn a_hidden_raws_truth_entry_is_refused_as_one_the_work_does_not_have() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let held = the_truth(&merged);
        let view = viewing_raw(&merged, &[false, false, false]);
        let visible = view.visible().expect("Manga is limited");
        assert!(!visible.contains("raw"), "the fixture: raw is hidden");
        let sent = crate::truth::as_seen(&held, &view);
        assert_eq!(sent.editions.keys().collect::<Vec<_>>(), ["en"]);

        let mut changed = held.editions["raw"].clone();
        changed.pages.insert(0, vec![a_box(0.5)]);
        let refused = |key: &str| {
            let mut doc = sent.clone();
            doc.editions.insert(key.into(), changed.clone());
            written(&merged, &view, doc)
                .map_err(|(kind, said)| (kind, said.replace(&format!("`{key}`"), "`{edition}`")))
        };
        let words = Err((
            "missing",
            format!(
                "this truth document names the edition `{{edition}}`, which work `{}` does \
                 not have",
                merged.slug
            ),
        ));
        assert_eq!(refused("raw"), words, "hidden raw's entry changed");
        assert_eq!(refused("zz"), words, "an edition the work does not have");

        let mut expected = held.clone();
        let doc = with_ens_page_4_labelled(sent);
        expected
            .editions
            .insert("en".into(), doc.editions["en"].clone());
        assert_eq!(
            written(&merged, &view, doc),
            Ok(expected.to_json()),
            "en's change taken and hidden raw's entry kept"
        );
    }

    /// A write changing `raw`'s entry, a page or its members, is refused, as
    /// Manga sees `raw` only in part; one sending the stored entry back as it
    /// was, as a client that read it before the grant narrowed does, is taken.
    #[tokio::test]
    async fn manga_alones_truth_changing_raws_entry_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let held = the_truth(&merged);
        let view = viewing_raw(&merged, &[true, false, true]);
        assert!(
            view.narrowed("raw").is_some(),
            "the fixture: raw is narrowed"
        );

        let words = Err((
            "partly visible",
            format!(
                "`raw` of `{}` is only partly visible to this account, so its ground truth \
                 cannot be written here",
                merged.slug
            ),
        ));
        let mut a_page = held.clone();
        let raw = a_page.editions.get_mut("raw").expect("raw");
        raw.pages.insert(0, vec![a_box(0.5)]);
        assert_eq!(written(&merged, &view, a_page), words, "a page labelled");
        let mut members = held.clone();
        members.editions.get_mut("raw").expect("raw").members.pop();
        assert_eq!(written(&merged, &view, members), words, "a member left out");

        assert_eq!(
            written(&merged, &view, held.clone()),
            Ok(held.to_json()),
            "raw's entry as it was"
        );
    }

    /// The digest of the one member whose path ends `/{file}`, `file` naming
    /// its directory too, as `en` and `fr` share a file name.
    fn digest(merged: &EditionMergedFromTwoLibraries, file: &str) -> String {
        let digests: Vec<String> = merged
            .state
            .library
            .conn()
            .prepare("SELECT sha256 FROM member WHERE path LIKE '%/' || ?1")
            .and_then(|mut stmt| stmt.query_map([file], |r| r.get(0))?.collect())
            .unwrap_or_else(|e| panic!("{file}'s digest: {e}"));
        assert_eq!(digests.len(), 1, "one member is `{file}`: {digests:?}");
        digests.into_iter().next().expect("one digest")
    }

    /// Each note of a notes body as sent, by id.
    fn sent_notes(body: &str) -> HashMap<String, serde_json::Value> {
        let doc: serde_json::Value = serde_json::from_str(body).expect("a notes document");
        doc["notes"]
            .as_array()
            .expect("a notes array")
            .iter()
            .map(|note| {
                (
                    note["id"].as_str().expect("an id").to_string(),
                    note.clone(),
                )
            })
            .collect()
    }

    /// O240: a note is sent the page it stands on, part3's second, beside
    /// its slot: whole, and to Manga alone at its slot 3.
    #[tokio::test]
    async fn the_notes_read_sends_each_notes_page() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part3 = serde_json::json!({
            "sha256": digest(&merged, "m/Foo v01 part3.cbz"),
            "nth": 0,
            "page": 1,
        });
        let uri = notes_uri(&merged);
        for (reader, slot) in [(Reader::Administrator, 6), (Reader::MangaOnly, 3)] {
            hold_notes(
                &merged,
                reader,
                &crate::notes::Notes {
                    taiyaku_notes: taiyaku_library::NOTES_VERSION,
                    work: merged.slug.clone(),
                    notes: vec![note("n", "raw", 6)],
                },
            );
            let (status, _, body) = fetched(&merged, reader, &uri, None).await;
            assert_eq!(status, StatusCode::OK, "{reader:?}: {body}");
            assert_eq!(anchors(&body), [anchor("n", "raw", slot)], "{reader:?}");
            assert_eq!(sent_notes(&body)["n"]["at"], part3, "{reader:?}: {body}");
        }
    }

    /// Spec §6 decision 5: a `stale` note is sent the page its row names, as
    /// stored, though Kids alone's `raw` is narrowed and its slot translated.
    #[tokio::test]
    async fn a_stale_notes_read_sends_its_stored_anchor() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::KidsOnly,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("k1", "raw", 2)],
            },
        );
        merged
            .state
            .library
            .conn()
            .execute(
                "UPDATE note SET at_sha256 = 'held-by-no-member' \
                  WHERE id = 'k1' AND user_id = ?1",
                [merged.user_id(Reader::KidsOnly)],
            )
            .expect("loses k1's page");

        let (status, _, body) = fetched(&merged, Reader::KidsOnly, &notes_uri(&merged), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(anchors(&body), [anchor("k1", "raw", 0)], "{body}");
        let k1 = &sent_notes(&body)["k1"];
        assert_eq!(k1["stale"], true, "the fixture: k1's page is gone: {body}");
        assert_eq!(
            k1["at"],
            serde_json::json!({ "sha256": "held-by-no-member", "nth": 0, "page": 0 }),
            "{body}"
        );
    }

    /// Spec gap 8: a `stale` note whose row names part2, a member Manga
    /// alone is not sent, is sent no page, as the digest would hand Manga a
    /// key its notes tag is hashed under. Page 9 is past part2's end.
    #[tokio::test]
    async fn a_stale_note_on_a_hidden_member_is_sent_no_anchor() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::MangaOnly,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n", "raw", 1)],
            },
        );
        let part2 = digest(&merged, "k/Foo v01 part2.cbz");
        merged
            .state
            .library
            .conn()
            .execute(
                "UPDATE note SET at_sha256 = ?1, at_nth = 0, at_page = 9 \
                  WHERE id = 'n' AND user_id = ?2",
                rusqlite::params![part2, merged.user_id(Reader::MangaOnly)],
            )
            .expect("names part2's page 9");

        let (status, _, body) =
            fetched(&merged, Reader::MangaOnly, &notes_uri(&merged), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(anchors(&body), [anchor("n", "raw", 1)], "{body}");
        let n = &sent_notes(&body)["n"];
        assert_eq!(n["stale"], true, "the fixture: n's page is gone: {body}");
        assert_eq!(n.get("at"), None, "{body}");
        assert!(!body.contains(&part2), "part2's digest is not sent: {body}");
    }

    /// The notes body `reader` is sent over its one note `n` on `edition`'s
    /// `slot`, its row naming `page` of copy 0 of the member of digest
    /// `sha256`, or no page.
    async fn sent_naming(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        (edition, slot): (&str, u32),
        page: Option<(&str, u32)>,
    ) -> String {
        hold_notes(
            merged,
            reader,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n", edition, slot)],
            },
        );
        let (sha256, page) = page.unzip();
        merged
            .state
            .library
            .conn()
            .execute(
                "UPDATE note SET at_sha256 = ?1, at_nth = ?2, at_page = ?3, at_half = NULL \
                  WHERE id = 'n' AND user_id = ?4",
                rusqlite::params![sha256, sha256.map(|_| 0), page, merged.user_id(reader)],
            )
            .expect("names the page");
        let (status, _, body) = fetched(merged, reader, &notes_uri(merged), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// Spec gap 8 across editions: Kids alone is not sent `en`, so a `stale`
    /// note on `raw` naming `en`'s member is sent no page.
    #[tokio::test]
    async fn a_stale_note_naming_an_edition_kids_is_not_sent_is_sent_no_anchor() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let en = digest(&merged, "m-en/Foo v01.cbz");
        let body = sent_naming(&merged, Reader::KidsOnly, ("raw", 2), Some((&en, 0))).await;
        assert_eq!(anchors(&body), [anchor("n", "raw", 0)], "{body}");
        let n = &sent_notes(&body)["n"];
        assert_eq!(n["stale"], true, "the fixture: n's page is gone: {body}");
        assert_eq!(n.get("at"), None, "{body}");
        assert!(!body.contains(&en), "en's digest is not sent: {body}");
    }

    /// Spec gap 8 on an edition sent whole: Manga alone sees `en` whole, and a
    /// `stale` note there naming part2, which it is not sent, is sent no page.
    #[tokio::test]
    async fn a_stale_note_on_an_edition_sent_whole_naming_a_hidden_member_is_sent_no_anchor() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part2 = digest(&merged, "k/Foo v01 part2.cbz");
        let body = sent_naming(&merged, Reader::MangaOnly, ("en", 1), Some((&part2, 9))).await;
        assert_eq!(anchors(&body), [anchor("n", "en", 1)], "{body}");
        let n = &sent_notes(&body)["n"];
        assert_eq!(n["stale"], true, "the fixture: n's page is gone: {body}");
        assert_eq!(n.get("at"), None, "{body}");
        assert!(!body.contains(&part2), "part2's digest is not sent: {body}");
    }

    /// A row naming no page is sent none, narrowed or not, as a position is:
    /// the page its slot shows is not a page it was written with.
    #[tokio::test]
    async fn an_anchorless_note_on_a_narrowed_edition_is_sent_no_anchor() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let body = sent_naming(&merged, Reader::MangaOnly, ("raw", 6), None).await;
        assert_eq!(anchors(&body), [anchor("n", "raw", 3)], "{body}");
        let n = &sent_notes(&body)["n"];
        assert_eq!(n.get("stale"), None, "the fixture: n is not stale: {body}");
        assert_eq!(n.get("at"), None, "{body}");
    }

    /// A note's or a position's row: its slot and its four `at_` columns,
    /// as stored.
    type Row = (
        u32,
        Option<String>,
        Option<u32>,
        Option<u32>,
        Option<String>,
    );

    /// `reader`'s row for note `id`, read by SQL, or `None` where it has none.
    fn row(merged: &EditionMergedFromTwoLibraries, reader: Reader, id: &str) -> Option<Row> {
        use rusqlite::OptionalExtension as _;
        merged
            .state
            .library
            .conn()
            .query_row(
                "SELECT slot, at_sha256, at_nth, at_page, at_half FROM note \
                  WHERE id = ?1 AND user_id = ?2",
                rusqlite::params![id, merged.user_id(reader)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .unwrap_or_else(|e| panic!("{id}'s row: {e}"))
    }

    /// A row on `slot`, anchored on `page` of copy 0 of the member `sha256`.
    fn on(slot: u32, sha256: &str, page: u32) -> Option<Row> {
        Some((slot, Some(sha256.to_string()), Some(0), Some(page), None))
    }

    /// `page` of copy 0 of the member `sha256`, whole.
    fn page_of(sha256: &str, page: u32) -> taiyaku_core::MemberPage {
        taiyaku_core::MemberPage {
            sha256: sha256.into(),
            nth: 0,
            page,
            half: None,
        }
    }

    /// `body`, a notes document as read, with `note` added.
    fn with(body: &str, note: crate::notes::Note) -> String {
        let mut doc: crate::notes::Notes = serde_json::from_str(body).expect("notes");
        doc.notes.push(note);
        serde_json::to_string(&doc).unwrap()
    }

    /// The refusal of note `id`'s page on `raw`, written out.
    fn refused_page(merged: &EditionMergedFromTwoLibraries, id: &str) -> String {
        format!(
            "note `{id}` names a page that `raw` of work `{}` does not have now. Reload the \
             page to see the notes as they are.",
            merged.slug
        )
    }

    /// O240: once Kids' directory is gone, slot 5 shows part3's fourth page,
    /// where in the numbering the note was written in it showed part3's
    /// first. Its `at` places it on that first page, slot 2.
    #[tokio::test]
    async fn a_note_written_with_an_old_slot_lands_on_the_page_its_at_names() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part3 = digest(&merged, "m/Foo v01 part3.cbz");
        remove_kids_directory(&merged);
        reread(&merged);
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;

        let n = crate::notes::Note {
            at: Some(page_of(&part3, 0)),
            ..note("n", "raw", 5)
        };
        let (status, _, answered) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            with(&body, n),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            row(&merged, Reader::Administrator, "n"),
            on(2, &part3, 0),
            "part3's first page"
        );
    }

    /// Spec §6 decision 5: part2 is no member Manga alone is sent, so a new
    /// note on its page is refused, though the work holds it.
    #[tokio::test]
    async fn a_note_anchored_on_a_page_the_account_is_not_sent_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part2 = digest(&merged, "k/Foo v01 part2.cbz");
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::MangaOnly, &uri, None).await;
        let before = stored_notes(&merged, Reader::MangaOnly);

        let m = crate::notes::Note {
            at: Some(page_of(&part2, 0)),
            ..note("m", "raw", 0)
        };
        let (status, _, answered) = put(
            &merged,
            Reader::MangaOnly,
            &uri,
            tag.as_deref(),
            with(&body, m),
        )
        .await;
        assert_eq!(
            (status, answered),
            (StatusCode::NOT_FOUND, refused_page(&merged, "m"))
        );
        assert_eq!(
            stored_notes(&merged, Reader::MangaOnly),
            before,
            "the store is unchanged"
        );
    }

    /// Part1 has two pages, so its page 9 is no page: refused, not read by
    /// the slot sent beside it.
    #[tokio::test]
    async fn an_at_past_its_members_end_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part1 = digest(&merged, "m/Foo v01 part1.cbz");
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;

        let n = crate::notes::Note {
            at: Some(page_of(&part1, 9)),
            ..note("n", "raw", 0)
        };
        let (status, _, answered) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            with(&body, n),
        )
        .await;
        assert_eq!(
            (status, answered),
            (StatusCode::NOT_FOUND, refused_page(&merged, "n"))
        );
        assert_eq!(row(&merged, Reader::Administrator, "n"), None);
    }

    /// Spec §6 decision 8: an edit keeps a `stale` note's anchor. Kids alone
    /// changes k1's words alone and sends back the page it read, which no
    /// member holds: k1 stays `stale` on its stored slot and page.
    #[tokio::test]
    async fn an_edited_stale_note_keeps_its_stored_anchor() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::KidsOnly,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("k1", "raw", 2)],
            },
        );
        merged
            .state
            .library
            .conn()
            .execute(
                "UPDATE note SET at_sha256 = 'held-by-no-member' \
                  WHERE id = 'k1' AND user_id = ?1",
                [merged.user_id(Reader::KidsOnly)],
            )
            .expect("loses k1's page");
        let held = row(&merged, Reader::KidsOnly, "k1");
        assert_eq!(held, on(2, "held-by-no-member", 0), "the fixture");
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::KidsOnly, &uri, None).await;
        let mut doc: crate::notes::Notes = serde_json::from_str(&body).expect("notes");
        assert_eq!(
            doc.notes,
            [crate::notes::Note {
                stale: true,
                at: Some(page_of("held-by-no-member", 0)),
                ..note("k1", "raw", 0)
            }],
            "the fixture: k1 is sent stale, on Kids' slot 0"
        );
        doc.notes[0].body = "k1's new words".into();

        let (status, _, answered) = put(
            &merged,
            Reader::KidsOnly,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            stored_notes(&merged, Reader::KidsOnly).notes,
            [crate::notes::Note {
                body: "k1's new words".into(),
                stale: true,
                ..note("k1", "raw", 2)
            }]
        );
        assert_eq!(
            row(&merged, Reader::KidsOnly, "k1"),
            held,
            "its slot and page as stored"
        );
    }

    /// Review Focus 2: Kids alone is not sent `en`, and echoes k3 there with
    /// the page its row names, then with another, as a tab that read it
    /// before a stamp re-keyed the row would. `at` is cleared on both sides
    /// before `as_stored` compares k3 with the note held, so each is kept.
    #[tokio::test]
    async fn an_echo_of_a_note_the_account_is_not_sent_carrying_an_at_is_accepted() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::KidsOnly,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("k1", "raw", 2), note("k3", "en", 1)],
            },
        );
        let held = row(&merged, Reader::KidsOnly, "k3");
        let Some((1, Some(sha256), Some(nth), Some(page), None)) = held.clone() else {
            panic!("the fixture: k3 is anchored on a whole page of en: {held:?}");
        };
        let stored = taiyaku_core::MemberPage {
            sha256,
            nth,
            page,
            half: None,
        };
        let uri = notes_uri(&merged);

        for at in [
            stored.clone(),
            taiyaku_core::MemberPage {
                sha256: "re-keyed-since".into(),
                ..stored
            },
        ] {
            let (_, tag, body) = fetched(&merged, Reader::KidsOnly, &uri, None).await;
            assert_eq!(
                anchors(&body),
                [anchor("k1", "raw", 0)],
                "the fixture: k3 is not sent"
            );
            let k3 = crate::notes::Note {
                at: Some(at.clone()),
                ..note("k3", "en", 1)
            };
            let (status, _, answered) = put(
                &merged,
                Reader::KidsOnly,
                &uri,
                tag.as_deref(),
                with(&body, k3),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{at:?}: {answered}");
            assert_eq!(row(&merged, Reader::KidsOnly, "k3"), held, "{at:?}");
            assert!(
                stored_notes(&merged, Reader::KidsOnly)
                    .notes
                    .contains(&note("k3", "en", 1)),
                "k3 as it was, {at:?}"
            );
        }
    }

    /// Spec §6 decision 5: a body without `at` is read by its slot. Once
    /// Kids' directory is gone, slot 2 shows part3's first page.
    #[tokio::test]
    async fn a_note_sent_without_an_at_is_read_by_its_slot() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part3 = digest(&merged, "m/Foo v01 part3.cbz");
        remove_kids_directory(&merged);
        reread(&merged);
        let uri = notes_uri(&merged);
        let (_, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;

        let (status, _, answered) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            with(&body, note("n", "raw", 2)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(row(&merged, Reader::Administrator, "n"), on(2, &part3, 0));
    }

    /// Spec §6 decision 4: n is read on part3's second page, slot 6, and
    /// Kids' directory then goes. After the 412 the tab sends n as it read
    /// it, slot 6 and all, past the six slots `raw` has now. Its `at` keeps
    /// it on part3's second page, now slot 3.
    #[tokio::test]
    async fn an_untouched_note_echoed_after_a_renumbering_keeps_its_page() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n", "raw", 6)],
            },
        );
        let part3 = digest(&merged, "m/Foo v01 part3.cbz");
        let uri = notes_uri(&merged);
        let (_, read_tag, read) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(
            sent_notes(&read)["n"]["at"],
            serde_json::json!({ "sha256": part3, "nth": 0, "page": 1 }),
            "the fixture: {read}"
        );
        remove_kids_directory(&merged);
        reread(&merged);

        let (status, _, _) = put(
            &merged,
            Reader::Administrator,
            &uri,
            read_tag.as_deref(),
            read.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED, "the old tag");
        let (_, tag, _) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let mut by_slot: crate::notes::Notes = serde_json::from_str(&read).expect("notes");
        by_slot.clear_wire_fields();
        let (status, _, _) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&by_slot).unwrap(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "the fixture: by its slot alone, n is past raw's end"
        );

        let (status, _, answered) =
            put(&merged, Reader::Administrator, &uri, tag.as_deref(), read).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            row(&merged, Reader::Administrator, "n"),
            on(3, &part3, 1),
            "part3's second page"
        );
    }

    /// The owner's ruling "Stamp remembers fingerprints": a stamp of part3
    /// re-keys n2's row to part3's new digest, and the tab, after the stamp's
    /// 412, sends n2 as it read it, naming the old one. That is forwarded to
    /// the new digest, n2's stored anchor, so n2 is kept as held, and n1
    /// beside it.
    #[tokio::test]
    async fn untouched_notes_echoed_after_a_stamp_are_kept() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n1", "raw", 1), note("n2", "raw", 6)],
            },
        );
        let part1 = digest(&merged, "m/Foo v01 part1.cbz");
        let uri = notes_uri(&merged);
        let (_, read_tag, read) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let (_, new) = stamp_part3(&merged);
        let (_, _, fresh) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(
            anchors(&fresh),
            [anchor("n1", "raw", 1), anchor("n2", "raw", 6)],
            "the fixture: {fresh}"
        );
        assert_eq!(
            sent_notes(&fresh)["n2"]["at"],
            serde_json::json!({ "sha256": new, "nth": 0, "page": 1 }),
            "the fixture: n2 is sent the new digest"
        );
        let stamped = row(&merged, Reader::Administrator, "n2");
        assert_eq!(stamped, on(6, &new, 1), "the fixture");

        let (status, _, _) = put(
            &merged,
            Reader::Administrator,
            &uri,
            read_tag.as_deref(),
            read.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED, "the stamp's 412");
        let (_, tag, _) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let (status, _, answered) =
            put(&merged, Reader::Administrator, &uri, tag.as_deref(), read).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            row(&merged, Reader::Administrator, "n2"),
            stamped,
            "n2's row as the stamp left it"
        );
        assert_eq!(row(&merged, Reader::Administrator, "n1"), on(1, &part1, 1));
        assert!(!answered.contains("stale"), "nothing is stale: {answered}");
    }

    /// The owner's ruling "Server keeps echoes" stops at a note another
    /// device moved: tab 2 marks a region on n, whose page is gone, which
    /// anchors it on part1's second page. Tab 1, after the 412, sends n as it
    /// read it, and is refused in words that say to reload.
    #[tokio::test]
    async fn a_stale_note_another_device_moved_is_refused_with_reload_words() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n", "raw", 1)],
            },
        );
        merged
            .state
            .library
            .conn()
            .execute(
                "UPDATE note SET at_sha256 = 'held-by-no-member' \
                  WHERE id = 'n' AND user_id = ?1",
                [merged.user_id(Reader::Administrator)],
            )
            .expect("loses n's page");
        let part1 = digest(&merged, "m/Foo v01 part1.cbz");
        let uri = notes_uri(&merged);
        let (_, first_tag, first) = fetched(&merged, Reader::Administrator, &uri, None).await;
        assert_eq!(sent_notes(&first)["n"]["stale"], true, "the fixture");

        let (_, tag, body) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let region = crate::notes::Region {
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
        };
        let mut doc: crate::notes::Notes = serde_json::from_str(&body).expect("notes");
        doc.notes = vec![crate::notes::Note {
            region: Some(region),
            at: Some(page_of(&part1, 1)),
            ..note("n", "raw", 1)
        }];
        let (status, _, answered) = put(
            &merged,
            Reader::Administrator,
            &uri,
            tag.as_deref(),
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "tab 2's move: {answered}");

        let (status, _, _) = put(
            &merged,
            Reader::Administrator,
            &uri,
            first_tag.as_deref(),
            first.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED, "tab 1's old tag");
        let (_, tag, _) = fetched(&merged, Reader::Administrator, &uri, None).await;
        let (status, _, answered) =
            put(&merged, Reader::Administrator, &uri, tag.as_deref(), first).await;
        assert_eq!(
            (status, answered),
            (StatusCode::NOT_FOUND, refused_page(&merged, "n"))
        );
        assert_eq!(
            stored_notes(&merged, Reader::Administrator).notes,
            [crate::notes::Note {
                region: Some(region),
                ..note("n", "raw", 1)
            }],
            "the store holds tab 2's n"
        );
        assert_eq!(
            row(&merged, Reader::Administrator, "n"),
            on(1, &part1, 1),
            "on part1's second page"
        );
    }

    /// Part3 stamped as `stamp::record_rows` stamps it: a new digest on its
    /// member, every row anchored on the old one moved to it, and the move
    /// remembered; then the server re-reads the work. Answers the old digest
    /// and the new.
    fn stamp_part3(merged: &EditionMergedFromTwoLibraries) -> (String, String) {
        let old = digest(merged, "m/Foo v01 part3.cbz");
        // Hashed from the old, so a second stamp names a third digest.
        let new = crate::notes::etag_for(&old).trim_matches('"').to_string();
        {
            let conn = merged.state.library.conn();
            let stamped = conn
                .execute(
                    "UPDATE member SET sha256 = ?1 WHERE path LIKE '%/m/Foo v01 part3.cbz'",
                    [&new],
                )
                .expect("stamps part3");
            assert_eq!(stamped, 1, "the fixture: one member is part3");
            for table in ["note", "progress"] {
                conn.execute(
                    &format!("UPDATE {table} SET at_sha256 = ?1 WHERE at_sha256 = ?2"),
                    [&new, &old],
                )
                .expect("moves the rows on part3 to its new digest");
            }
            let work_id: i64 = conn
                .query_row("SELECT id FROM work", [], |r| r.get(0))
                .expect("the one work");
            taiyaku_library::stamp::remember_moves(
                &conn,
                work_id,
                "raw",
                &std::collections::BTreeMap::from([((old.clone(), 0), (new.clone(), 0))]),
            )
            .expect("remembers the move");
        }
        reread(merged);
        (old, new)
    }

    /// The administrator holds n2 on part3's second page, raw slot 6, and a
    /// tab reads it: the tab's body, n2 naming part3's digest as it is now.
    async fn n2_as_read(merged: &EditionMergedFromTwoLibraries) -> String {
        hold_notes(
            merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n2", "raw", 6)],
            },
        );
        let part3 = digest(merged, "m/Foo v01 part3.cbz");
        let (_, _, read) = fetched(merged, Reader::Administrator, &notes_uri(merged), None).await;
        assert_eq!(
            sent_notes(&read)["n2"]["at"],
            serde_json::json!({ "sha256": part3, "nth": 0, "page": 1 }),
            "the fixture: {read}"
        );
        read
    }

    /// `body` written as `reader` under the tag a read gives now, as a tab
    /// writes after a 412: the status and the answer.
    async fn written_now(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        body: String,
    ) -> (StatusCode, String) {
        let uri = notes_uri(merged);
        let (_, tag, _) = fetched(merged, reader, &uri, None).await;
        let (status, _, answered) = put(merged, reader, &uri, tag.as_deref(), body).await;
        (status, answered)
    }

    /// The owner's ruling "Stamp remembers fingerprints": an edit keeps the
    /// `at` it was read with, so a tab from before part3's stamp sends n2's
    /// new words on the old digest, which is forwarded to the new one.
    #[tokio::test]
    async fn a_body_edit_from_before_a_stamp_saves() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (_, new) = stamp_part3(&merged);

        let mut doc: crate::notes::Notes = serde_json::from_str(&read).expect("notes");
        doc.notes[0].body = "n2's new words".into();
        let (status, answered) = written_now(
            &merged,
            Reader::Administrator,
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            stored_notes(&merged, Reader::Administrator).notes,
            [crate::notes::Note {
                body: "n2's new words".into(),
                ..note("n2", "raw", 6)
            }]
        );
        assert_eq!(row(&merged, Reader::Administrator, "n2"), on(6, &new, 1));
    }

    /// As [`a_body_edit_from_before_a_stamp_saves`], for n2's colour.
    #[tokio::test]
    async fn a_colour_edit_from_before_a_stamp_saves() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (_, new) = stamp_part3(&merged);

        let mut doc: crate::notes::Notes = serde_json::from_str(&read).expect("notes");
        doc.notes[0].color = Some("yellow".into());
        let (status, answered) = written_now(
            &merged,
            Reader::Administrator,
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            stored_notes(&merged, Reader::Administrator).notes,
            [crate::notes::Note {
                color: Some("yellow".into()),
                ..note("n2", "raw", 6)
            }]
        );
        assert_eq!(row(&merged, Reader::Administrator, "n2"), on(6, &new, 1));
    }

    /// A new note takes the page the tab shows, from its sidecar read
    /// before part3's stamp: part3's first page, by the old digest.
    #[tokio::test]
    async fn a_new_note_from_before_a_stamp_saves() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (old, new) = stamp_part3(&merged);

        let m = crate::notes::Note {
            at: Some(page_of(&old, 0)),
            ..note("m", "raw", 5)
        };
        let (status, answered) = written_now(&merged, Reader::Administrator, with(&read, m)).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(row(&merged, Reader::Administrator, "m"), on(5, &new, 0));
    }

    /// A region marked re-anchors n2 from the tab's sidecar read before
    /// part3's stamp: part3's second page, by the old digest.
    #[tokio::test]
    async fn a_region_move_from_before_a_stamp_saves() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (old, new) = stamp_part3(&merged);

        let region = crate::notes::Region {
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
        };
        let mut doc: crate::notes::Notes = serde_json::from_str(&read).expect("notes");
        doc.notes = vec![crate::notes::Note {
            region: Some(region),
            at: Some(page_of(&old, 1)),
            ..note("n2", "raw", 6)
        }];
        let (status, answered) = written_now(
            &merged,
            Reader::Administrator,
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            stored_notes(&merged, Reader::Administrator).notes,
            [crate::notes::Note {
                region: Some(region),
                ..note("n2", "raw", 6)
            }]
        );
        assert_eq!(row(&merged, Reader::Administrator, "n2"), on(6, &new, 1));
    }

    /// Manga alone reads n2 at its slot 3. Part3 is then stamped and Manga
    /// granted Kids, so n2 is slot 6. A grant, not a removal: a removal
    /// derives, which puts the old digest back (O270). Forwarded, n2's old
    /// digest finds the held note; unforwarded, its slot is not the one
    /// held.
    #[tokio::test]
    async fn an_untouched_note_after_a_stamp_and_a_renumbering_saves() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        hold_notes(
            &merged,
            Reader::MangaOnly,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n2", "raw", 6)],
            },
        );
        let part3 = digest(&merged, "m/Foo v01 part3.cbz");
        let (_, _, read) = fetched(&merged, Reader::MangaOnly, &notes_uri(&merged), None).await;
        assert_eq!(anchors(&read), [anchor("n2", "raw", 3)], "the fixture");
        assert_eq!(
            sent_notes(&read)["n2"]["at"],
            serde_json::json!({ "sha256": part3, "nth": 0, "page": 1 }),
            "the fixture: {read}"
        );
        let (_, new) = stamp_part3(&merged);
        taiyaku_library::account::grant_library(
            &merged.state.library.conn(),
            merged.user_id(Reader::MangaOnly),
            80,
        )
        .expect("grants Kids");
        let (whole, _) = document(&merged, Reader::MangaOnly).await;
        assert_eq!(
            members(&whole, "raw").len(),
            3,
            "the fixture: Manga now sees `raw` whole: {whole}"
        );

        let (status, answered) = written_now(&merged, Reader::MangaOnly, read).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(row(&merged, Reader::MangaOnly, "n2"), on(6, &new, 1));
    }

    /// A second stamp of part3 redirects the first's alias, so a tab from
    /// before both forwards in one step to the digest part3 holds now.
    #[tokio::test]
    async fn a_body_edit_from_before_two_stamps_saves() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (_, mid) = stamp_part3(&merged);
        let (was, new) = stamp_part3(&merged);
        assert_eq!(was, mid, "the fixture: the second stamp moves the first's");

        let mut doc: crate::notes::Notes = serde_json::from_str(&read).expect("notes");
        doc.notes[0].body = "n2's new words".into();
        let (status, answered) = written_now(
            &merged,
            Reader::Administrator,
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            stored_notes(&merged, Reader::Administrator).notes,
            [crate::notes::Note {
                body: "n2's new words".into(),
                ..note("n2", "raw", 6)
            }]
        );
        assert_eq!(row(&merged, Reader::Administrator, "n2"), on(6, &new, 1));
    }

    /// Only a digest a stamp moved is forwarded: one nothing names is
    /// refused, though an alias of another digest on its occurrence stands.
    #[tokio::test]
    async fn a_digest_no_stamp_recorded_is_still_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (_, new) = stamp_part3(&merged);

        let m = crate::notes::Note {
            at: Some(page_of("never-stamped", 0)),
            ..note("m", "raw", 5)
        };
        let (status, answered) = written_now(&merged, Reader::Administrator, with(&read, m)).await;
        assert_eq!(
            (status, answered),
            (StatusCode::NOT_FOUND, refused_page(&merged, "m"))
        );
        assert_eq!(row(&merged, Reader::Administrator, "m"), None);
        assert_eq!(
            row(&merged, Reader::Administrator, "n2"),
            on(6, &new, 1),
            "the store is unchanged"
        );
    }

    /// Review Focus 9, O270: a derive before the next scan puts part3's
    /// old digest back on its member, and the alias stays. A new note on
    /// the old digest is on a page the document sent shows, so it is placed
    /// there, not forwarded off it.
    #[tokio::test]
    async fn a_digest_the_member_still_shows_is_not_forwarded() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (old, new) = stamp_part3(&merged);
        {
            let conn = merged.state.library.conn();
            let restored = conn
                .execute(
                    "UPDATE member SET sha256 = ?1 WHERE path LIKE '%/m/Foo v01 part3.cbz'",
                    [&old],
                )
                .expect("puts part3's old digest back");
            assert_eq!(restored, 1, "the fixture: one member is part3");
            let work_id: i64 = conn
                .query_row("SELECT id FROM work", [], |r| r.get(0))
                .expect("the one work");
            assert_eq!(
                taiyaku_library::stamp::alias_of(&conn, work_id, "raw", &old, 0),
                Ok(Some((new, 0))),
                "the fixture: the alias stays"
            );
        }
        reread(&merged);
        let (doc, _) = document(&merged, Reader::Administrator).await;
        assert_eq!(
            doc["editions"]["raw"]["members"][2]["sha256"], old,
            "the fixture: part3 is sent by its old digest: {doc}"
        );

        let m = crate::notes::Note {
            at: Some(page_of(&old, 0)),
            ..note("m", "raw", 5)
        };
        let (status, answered) = written_now(&merged, Reader::Administrator, with(&read, m)).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(row(&merged, Reader::Administrator, "m"), on(5, &old, 0));
    }

    /// The owner's ruling "Server keeps echoes", where nothing forwards: a
    /// fold or an edition rename leaves no alias of part3's stamp. The tab's
    /// untouched n2, naming the old digest, is kept as held; its own edit is
    /// refused in words that say to reload.
    #[tokio::test]
    async fn an_untouched_echo_no_alias_forwards_is_kept_and_an_edit_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (_, new) = stamp_part3(&merged);
        let forgotten = merged
            .state
            .library
            .conn()
            .execute("DELETE FROM stamp_alias", [])
            .expect("forgets the stamp's aliases");
        assert_eq!(forgotten, 1, "the fixture: the stamp's one alias");

        let (status, answered) = written_now(&merged, Reader::Administrator, read.clone()).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            row(&merged, Reader::Administrator, "n2"),
            on(6, &new, 1),
            "n2's row as the stamp left it"
        );

        let mut doc: crate::notes::Notes = serde_json::from_str(&read).expect("notes");
        doc.notes[0].body = "n2's new words".into();
        let (status, answered) = written_now(
            &merged,
            Reader::Administrator,
            serde_json::to_string(&doc).unwrap(),
        )
        .await;
        assert_eq!(
            (status, answered),
            (StatusCode::NOT_FOUND, refused_page(&merged, "n2"))
        );
        assert_eq!(
            stored_notes(&merged, Reader::Administrator).notes,
            [note("n2", "raw", 6)],
            "n2's words as held"
        );
    }

    /// An alias read that fails is the library's failure, answered without
    /// the store's words, and nothing is written.
    #[tokio::test]
    async fn a_failed_alias_read_is_answered_as_the_librarys_failure() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let read = n2_as_read(&merged).await;
        let (_, new) = stamp_part3(&merged);
        merged
            .state
            .library
            .conn()
            .execute("DROP TABLE stamp_alias", [])
            .expect("fails every alias read");

        let (status, answered) = written_now(&merged, Reader::Administrator, read).await;
        assert_eq!(
            (status, answered.as_str()),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "the library could not answer this request; the server's log says why"
            )
        );
        assert_eq!(row(&merged, Reader::Administrator, "n2"), on(6, &new, 1));
    }

    /// Pre-deploy decisions 2 and 3: part3 is made a copy of part1, so `raw`
    /// holds one digest twice, and n is written on the second copy. A stamp of
    /// the first alone moves n's row onto the pair the first copy held before,
    /// and the second copy is then lost. n's echo names its own stored anchor,
    /// which is not forwarded onto the stamped copy: n stays where it was.
    #[tokio::test]
    async fn a_lost_copys_stale_note_is_not_forwarded_onto_the_stamped_copy() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let copied = digest(&merged, "m/Foo v01 part1.cbz");
        let set_part = |file: &str, sha256: &str| {
            let set = merged
                .state
                .library
                .conn()
                .execute(
                    "UPDATE member SET sha256 = ?1 WHERE path LIKE '%/m/Foo v01 ' || ?2",
                    [sha256, file],
                )
                .expect("sets a member's digest");
            assert_eq!(set, 1, "the fixture: one member is {file}");
        };
        set_part("part3.cbz", &copied);
        reread(&merged);
        hold_notes(
            &merged,
            Reader::Administrator,
            &crate::notes::Notes {
                taiyaku_notes: taiyaku_library::NOTES_VERSION,
                work: merged.slug.clone(),
                notes: vec![note("n", "raw", 5)],
            },
        );
        assert_eq!(
            row(&merged, Reader::Administrator, "n"),
            Some((5, Some(copied.clone()), Some(1), Some(0), None)),
            "the fixture: n on the second copy's first page"
        );

        let stamped = "5a".repeat(32);
        set_part("part1.cbz", &stamped);
        {
            let conn = merged.state.library.conn();
            conn.execute("UPDATE note SET at_nth = 0 WHERE id = 'n'", [])
                .expect("moves n as the stamp's re-keying does");
            let work_id: i64 = conn
                .query_row("SELECT id FROM work", [], |r| r.get(0))
                .expect("the one work");
            taiyaku_library::stamp::remember_moves(
                &conn,
                work_id,
                "raw",
                &std::collections::BTreeMap::from([
                    ((copied.clone(), 0), (stamped, 0)),
                    ((copied.clone(), 1), (copied.clone(), 0)),
                ]),
            )
            .expect("remembers the stamp's moves");
        }
        set_part("part3.cbz", "lost-since");
        reread(&merged);
        let held = row(&merged, Reader::Administrator, "n");
        assert_eq!(
            held,
            on(5, &copied, 0),
            "the fixture: n's row names the first copy's old pair"
        );
        let (_, _, read) = fetched(&merged, Reader::Administrator, &notes_uri(&merged), None).await;
        assert_eq!(sent_notes(&read)["n"]["stale"], true, "the fixture: {read}");

        let (status, answered) = written_now(&merged, Reader::Administrator, read).await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(
            row(&merged, Reader::Administrator, "n"),
            held,
            "its slot and page as stored"
        );
    }

    /// The refusal of a position's page on `raw`, written out.
    fn refused_position(merged: &EditionMergedFromTwoLibraries) -> String {
        format!(
            "this position names a page that `raw` of work `{}` does not have",
            merged.slug
        )
    }

    /// `reader`'s position on `edition` as stored: its slot and its four
    /// `at_` columns, read by SQL, or `None` where it has none.
    fn position_row(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        edition: &str,
    ) -> Option<Row> {
        use rusqlite::OptionalExtension as _;
        merged
            .state
            .library
            .conn()
            .query_row(
                "SELECT slot, at_sha256, at_nth, at_page, at_half FROM progress \
                  WHERE edition = ?1 AND user_id = ?2",
                rusqlite::params![edition, merged.user_id(reader)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .unwrap_or_else(|e| panic!("the position on {edition}: {e}"))
    }

    /// `body` written as `reader`'s position: the status and the answer.
    async fn put_position(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        body: &serde_json::Value,
    ) -> (StatusCode, String) {
        let (status, _, answered) = put(
            merged,
            reader,
            &progress_uri(merged),
            None,
            body.to_string(),
        )
        .await;
        (status, answered)
    }

    /// `reader`'s positions as it is sent them, whole.
    async fn sent_positions(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
    ) -> (Vec<serde_json::Value>, String) {
        let (status, _, body) = fetched(merged, reader, &progress_uri(merged), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("a progress document");
        let positions = doc["positions"].as_array().expect("an array").clone();
        (positions, body)
    }

    /// Settled call 9a: once Kids' directory is gone, slot 5 shows part3's
    /// fourth page, where in the numbering the turn was made in it showed
    /// part3's first. Its `at` places it on that first page, slot 2.
    #[tokio::test]
    async fn a_position_written_with_an_old_slot_lands_on_the_page_its_at_names() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part3 = digest(&merged, "m/Foo v01 part3.cbz");
        remove_kids_directory(&merged);
        reread(&merged);

        let (status, answered) = put_position(
            &merged,
            Reader::Administrator,
            &serde_json::json!({
                "edition": "raw",
                "slot": 5,
                "at": { "sha256": part3, "nth": 0, "page": 0 }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("raw".to_string(), 2)]),
            "part3's first page, after part1's two"
        );
    }

    /// Spec §6 decision 5: part2 is not in the document Manga alone is sent,
    /// so a position on one of its pages is refused, and nothing is written.
    #[tokio::test]
    async fn a_position_anchored_on_a_page_the_account_is_not_sent_is_refused() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part2 = digest(&merged, "k/Foo v01 part2.cbz");

        let (status, answered) = put_position(
            &merged,
            Reader::MangaOnly,
            &serde_json::json!({
                "edition": "raw",
                "slot": 1,
                "at": { "sha256": part2, "nth": 0, "page": 1 }
            }),
        )
        .await;
        assert_eq!(
            (status, answered),
            (StatusCode::NOT_FOUND, refused_position(&merged))
        );
        assert_eq!(position_row(&merged, Reader::MangaOnly, "raw"), None);
    }

    /// Settled call 16: a tab from before part3's stamp turns to part3's
    /// second page, naming the digest the stamp replaced, which is forwarded
    /// to the new one.
    #[tokio::test]
    async fn a_page_turn_from_before_a_stamp_saves() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let (old, new) = stamp_part3(&merged);

        let (status, answered) = put_position(
            &merged,
            Reader::Administrator,
            &serde_json::json!({
                "edition": "raw",
                "slot": 6,
                "at": { "sha256": old, "nth": 0, "page": 1 }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        assert_eq!(
            position_row(&merged, Reader::Administrator, "raw"),
            on(6, &new, 1)
        );
    }

    /// As [`a_page_turn_from_before_a_stamp_saves`], by a reader already on
    /// part3's first page, a row the stamp re-keyed: a turn naming another
    /// page is still forwarded.
    #[tokio::test]
    async fn a_page_turn_from_before_a_stamp_saves_over_a_row_held_on_the_edition() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        put_raw_as_the_administrator(&merged, 5).await;
        let (old, new) = stamp_part3(&merged);
        assert_eq!(
            position_row(&merged, Reader::Administrator, "raw"),
            on(5, &new, 0),
            "the fixture: the row is on the new digest"
        );

        let (status, answered) = put_position(
            &merged,
            Reader::Administrator,
            &serde_json::json!({
                "edition": "raw",
                "slot": 6,
                "at": { "sha256": old, "nth": 0, "page": 1 }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        assert_eq!(
            position_row(&merged, Reader::Administrator, "raw"),
            on(6, &new, 1)
        );
    }

    /// Settled call 9b: the administrator stands on part2's second page, and
    /// Kids' directory goes. A turn naming that page leaves the row as it is:
    /// written, it would be anchored on the page its slot shows now.
    #[tokio::test]
    async fn a_position_naming_its_own_lost_page_is_left_as_it_is() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part2 = digest(&merged, "k/Foo v01 part2.cbz");
        put_raw_as_the_administrator(&merged, 3).await;
        remove_kids_directory(&merged);
        reread(&merged);
        let held = position_row(&merged, Reader::Administrator, "raw");
        assert_eq!(
            held,
            on(3, &part2, 1),
            "the fixture: on part2's second page"
        );

        let (status, answered) = put_position(
            &merged,
            Reader::Administrator,
            &serde_json::json!({
                "edition": "raw",
                "slot": 3,
                "at": { "sha256": part2, "nth": 0, "page": 1 }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        assert_eq!(
            position_row(&merged, Reader::Administrator, "raw"),
            held,
            "its slot and page as stored"
        );
    }

    /// Settled call 9d: a turn with no `at` is read by its slot in the
    /// document sent, and range-checked there in O3's words.
    #[tokio::test]
    async fn a_position_sent_without_an_at_is_read_by_its_slot() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        remove_kids_directory(&merged);
        reread(&merged);

        let (status, answered) = put_position(
            &merged,
            Reader::Administrator,
            &serde_json::json!({ "edition": "raw", "slot": 2 }),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("raw".to_string(), 2)])
        );

        let (status, answered) = put_position(
            &merged,
            Reader::Administrator,
            &serde_json::json!({ "edition": "raw", "slot": 99 }),
        )
        .await;
        assert_eq!(
            (status, answered),
            (
                StatusCode::NOT_FOUND,
                format!(
                    "`raw` has 6 slots in work `{}`; there is no slot 99",
                    merged.slug
                )
            )
        );
    }

    /// Settled call 10: a position is sent the page it stands on; one on a
    /// page Manga alone is not sent, its fallback's page; one whose page is
    /// gone, the page as stored beside its clamped slot; and one naming a
    /// member Manga alone is not sent, no page (spec gap 8).
    #[tokio::test]
    async fn the_progress_read_sends_each_positions_page() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part2 = digest(&merged, "k/Foo v01 part2.cbz");
        let part3 = digest(&merged, "m/Foo v01 part3.cbz");

        put_raw_as_the_administrator(&merged, 6).await;
        let (sent, body) = sent_positions(&merged, Reader::Administrator).await;
        assert_eq!(
            (&sent[0]["slot"], &sent[0]["at"]),
            (
                &serde_json::json!(6),
                &serde_json::json!({ "sha256": part3, "nth": 0, "page": 1 })
            ),
            "part3's second page: {body}"
        );

        let part1 = digest(&merged, "m/Foo v01 part1.cbz");
        hold_position(&merged, Reader::MangaOnly, 3);
        let (sent, body) = sent_positions(&merged, Reader::MangaOnly).await;
        assert_eq!(
            (&sent[0]["slot"], &sent[0]["at"]),
            (
                &serde_json::json!(1),
                &serde_json::json!({ "sha256": part1, "nth": 0, "page": 1 })
            ),
            "part2's second page is hidden; its fallback is part1's second: {body}"
        );
        assert!(!body.contains(&part2), "part2's digest is not sent: {body}");

        hold_position(&merged, Reader::MangaOnly, 1);
        let changed = merged
            .state
            .library
            .conn()
            .execute(
                "UPDATE progress SET at_sha256 = ?1, at_page = 9 WHERE user_id = ?2",
                rusqlite::params![part2, merged.user_id(Reader::MangaOnly)],
            )
            .expect("names part2's page 9");
        assert_eq!(changed, 1, "the fixture: Manga's one position");
        let (sent, body) = sent_positions(&merged, Reader::MangaOnly).await;
        assert_eq!(sent[0]["slot"], 1, "the fixture: {body}");
        assert_eq!(sent[0].get("at"), None, "{body}");
        assert!(!body.contains(&part2), "part2's digest is not sent: {body}");

        put_raw_as_the_administrator(&merged, 3).await;
        remove_kids_directory(&merged);
        reread(&merged);
        let (sent, body) = sent_positions(&merged, Reader::Administrator).await;
        assert_eq!(
            (&sent[0]["slot"], &sent[0]["at"]),
            (
                &serde_json::json!(3),
                &serde_json::json!({ "sha256": part2, "nth": 0, "page": 1 })
            ),
            "part2's second page, as stored: {body}"
        );
    }

    /// Spec gap 4: a turn back to the page a row already names is written,
    /// so the edition turned in last is the one resumed. The rows' times
    /// are set apart by SQL, as writes in one second tie.
    #[tokio::test]
    async fn a_position_turned_back_to_in_one_edition_is_the_one_resumed() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let part3 = digest(&merged, "m/Foo v01 part3.cbz");
        let turn = serde_json::json!({
            "edition": "raw",
            "slot": 6,
            "at": { "sha256": part3, "nth": 0, "page": 1 }
        });
        for body in [&turn, &serde_json::json!({ "edition": "en", "slot": 0 })] {
            let (status, answered) = put_position(&merged, Reader::Administrator, body).await;
            assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        }
        for (edition, updated) in [
            ("raw", "2026-01-01 00:00:00"),
            ("en", "2026-01-02 00:00:00"),
        ] {
            let set = merged
                .state
                .library
                .conn()
                .execute(
                    "UPDATE progress SET updated = ?1 WHERE edition = ?2 AND user_id = ?3",
                    rusqlite::params![updated, edition, merged.user_id(Reader::Administrator)],
                )
                .expect("sets the row's time");
            assert_eq!(set, 1, "the fixture: one row on {edition}");
        }
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("en".to_string(), 0), ("raw".to_string(), 6)]),
            "the fixture: en is the newer"
        );

        let (status, answered) = put_position(&merged, Reader::Administrator, &turn).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        assert_eq!(
            positions(&merged, Reader::Administrator).await,
            Some(vec![("raw".to_string(), 6), ("en".to_string(), 0)]),
            "raw, turned in last"
        );
    }

    /// As [`a_lost_copys_stale_note_is_not_forwarded_onto_the_stamped_copy`],
    /// for the administrator's position on the second copy's first page: a
    /// turn re-sending the page its row stores is not forwarded onto the
    /// stamped copy, and the row stays as it is.
    #[tokio::test]
    async fn a_lost_copys_position_is_not_forwarded_onto_the_stamped_copy() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        the_administrators_document(&merged).await;
        let copied = digest(&merged, "m/Foo v01 part1.cbz");
        let set_part = |file: &str, sha256: &str| {
            let set = merged
                .state
                .library
                .conn()
                .execute(
                    "UPDATE member SET sha256 = ?1 WHERE path LIKE '%/m/Foo v01 ' || ?2",
                    [sha256, file],
                )
                .expect("sets a member's digest");
            assert_eq!(set, 1, "the fixture: one member is {file}");
        };
        set_part("part3.cbz", &copied);
        reread(&merged);
        put_raw_as_the_administrator(&merged, 5).await;
        assert_eq!(
            position_row(&merged, Reader::Administrator, "raw"),
            Some((5, Some(copied.clone()), Some(1), Some(0), None)),
            "the fixture: on the second copy's first page"
        );

        set_part("part1.cbz", &"5a".repeat(32));
        {
            let conn = merged.state.library.conn();
            conn.execute(
                "UPDATE progress SET at_nth = 0 WHERE user_id = ?1",
                [merged.user_id(Reader::Administrator)],
            )
            .expect("moves the row as the stamp's re-keying does");
            let work_id: i64 = conn
                .query_row("SELECT id FROM work", [], |r| r.get(0))
                .expect("the one work");
            taiyaku_library::stamp::remember_moves(
                &conn,
                work_id,
                "raw",
                &std::collections::BTreeMap::from([
                    ((copied.clone(), 0), ("5a".repeat(32), 0)),
                    ((copied.clone(), 1), (copied.clone(), 0)),
                ]),
            )
            .expect("remembers the stamp's moves");
        }
        set_part("part3.cbz", "lost-since");
        reread(&merged);
        let held = position_row(&merged, Reader::Administrator, "raw");
        assert_eq!(
            held,
            on(5, &copied, 0),
            "the fixture: the row names the first copy's old pair"
        );
        let (sent, body) = sent_positions(&merged, Reader::Administrator).await;
        let stored = serde_json::json!({ "sha256": copied, "nth": 0, "page": 0 });
        assert_eq!(sent[0]["at"], stored, "the fixture: {body}");

        let (status, answered) = put_position(
            &merged,
            Reader::Administrator,
            &serde_json::json!({ "edition": "raw", "slot": 5, "at": stored }),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answered}");
        assert_eq!(
            position_row(&merged, Reader::Administrator, "raw"),
            held,
            "its slot and page as stored"
        );
    }
}

/// O272: a reader's save waiting for the store's write lock, which a scan's
/// derive holds work by work, stalls only itself. Each test sends its saves
/// while another connection holds `BEGIN IMMEDIATE`.
mod a_save_waiting_for_the_write_lock {
    use super::*;
    use axum::http::Method;
    use std::time::{Duration, Instant};

    /// Well under the store's busy timeout (5 s), which a read queued behind
    /// the waiting save would take.
    const NO_WAIT: Duration = Duration::from_secs(1);

    /// Long enough for a save to pass the guard and reach its write.
    const IN_FLIGHT: Duration = Duration::from_millis(500);

    /// About one busy timeout (5 s): a save that waited for the lock alone,
    /// not the 10 s of one that waited out another save's timeout first.
    const ITS_OWN_WAIT: std::ops::Range<Duration> = Duration::from_secs(4)..Duration::from_secs(7);

    /// Another connection to the store, holding its write lock until it rolls
    /// back or drops.
    fn the_lock_held(state: &crate::ServeState) -> rusqlite::Connection {
        let holder = rusqlite::Connection::open(state.library.path().expect("a store in a file"))
            .expect("another connection");
        holder
            .execute_batch("BEGIN IMMEDIATE")
            .expect("takes the write lock");
        holder
    }

    /// `request`'s status and body, and how long it took to answer.
    async fn timed(router: Router, request: Request<Body>) -> (StatusCode, String, Duration) {
        let started = Instant::now();
        let response = router.oneshot(request).await.expect("answers");
        let status = response.status();
        let body = crate::test_support::body_string(response).await;
        (status, body, started.elapsed())
    }

    struct Waited {
        read: StatusCode,
        took: Duration,
        /// Whether the save was still out when the read was answered. A save
        /// that had already finished leaves the read proving nothing.
        waiting: bool,
        saved: StatusCode,
        body: String,
    }

    /// Sends `save` while another connection holds the write lock, then a
    /// `GET /api/works` once the save is in flight, timed; then lets the lock
    /// go and waits for the save's answer.
    async fn a_read_while(
        state: &crate::ServeState,
        router: &Router,
        save: Request<Body>,
    ) -> Waited {
        let holder = the_lock_held(state);
        let saving = tokio::spawn(router.clone().oneshot(save));
        tokio::time::sleep(IN_FLIGHT).await;

        let started = Instant::now();
        let read = router
            .clone()
            .oneshot(
                Request::get("/api/works")
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("answers")
            .status();
        let took = started.elapsed();
        let waiting = !saving.is_finished();
        holder.execute_batch("ROLLBACK").expect("releases the lock");

        let saved = saving.await.expect("the save ran").expect("answers");
        let status = saved.status();
        let body = crate::test_support::body_string(saved).await;
        Waited {
            read,
            took,
            waiting,
            saved: status,
            body,
        }
    }

    fn sent(method: Method, uri: String, tag: Option<String>, body: String) -> Request<Body> {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        let request = match tag {
            Some(tag) => request.header(header::IF_MATCH, tag),
            None => request,
        };
        request.body(Body::from(body)).expect("a request")
    }

    /// `uri`'s `ETag` and body, read before the lock is taken.
    async fn read(router: &Router, uri: &str) -> (String, String) {
        let response = crate::test_support::get(router, uri).await;
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        let tag = response.headers()[header::ETAG]
            .to_str()
            .expect("an ETag is text")
            .to_owned();
        (tag, crate::test_support::body_string(response).await)
    }

    /// The read answered quickly while the save waited, and the save landed
    /// with `saved` once the lock was free.
    fn quick(waited: &Waited, saved: StatusCode) {
        assert!(waited.took < NO_WAIT, "the read took {:?}", waited.took);
        assert!(
            waited.waiting,
            "the save had its answer before the read: {} {}",
            waited.saved, waited.body
        );
        assert_eq!(
            (waited.read, waited.saved),
            (StatusCode::OK, saved),
            "(the read, the save): {}",
            waited.body
        );
    }

    #[tokio::test]
    async fn a_read_is_answered_while_a_progress_save_waits() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();
        let save = sent(
            Method::PUT,
            format!("/api/works/{slug}/progress"),
            None,
            r#"{"edition":"raw","slot":1}"#.to_string(),
        );

        let waited = a_read_while(&state, &router, save).await;

        quick(&waited, StatusCode::NO_CONTENT);
        // Read back on the shared connection, which sees the save's commit.
        let response =
            crate::test_support::get(&router, &format!("/api/works/{slug}/progress")).await;
        let positions = crate::test_support::body_string(response).await;
        assert!(positions.contains(r#""slot":1"#), "{positions}");
    }

    #[tokio::test]
    async fn a_read_is_answered_while_a_notes_save_waits() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();
        let uri = format!("/api/works/{slug}/notes");
        let (tag, notes) = read(&router, &uri).await;

        let waited = a_read_while(&state, &router, sent(Method::PUT, uri, Some(tag), notes)).await;

        quick(&waited, StatusCode::OK);
    }

    #[tokio::test]
    async fn a_read_is_answered_while_a_truth_save_waits() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();
        let uri = format!("/api/works/{slug}/truth");
        let (tag, truth) = read(&router, &uri).await;

        let waited = a_read_while(&state, &router, sent(Method::PUT, uri, Some(tag), truth)).await;

        quick(&waited, StatusCode::OK);
    }

    /// A correction holds its work's document while it writes, and
    /// `list_works` reads every work's document on the shared connection, so
    /// the correction must not wait for the lock with the document held.
    #[tokio::test]
    async fn a_read_is_answered_while_a_correction_waits() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();
        let uri = format!("/api/works/{slug}");
        let (tag, _) = read(&router, &uri).await;
        let save = sent(
            Method::PATCH,
            format!("{uri}/alignments"),
            Some(tag.clone()),
            r#"{"from":"raw","to":"en","a":"2","b":"1"}"#.to_string(),
        );

        let waited = a_read_while(&state, &router, save).await;

        quick(&waited, StatusCode::OK);
        assert_ne!(read(&router, &uri).await.0, tag, "nothing was corrected");
    }

    /// Two saves waiting at once each wait for the lock alone, so each fails
    /// after its own busy timeout, and a read sent at any point meanwhile is
    /// answered at once. The one test here that waits a busy timeout out.
    #[tokio::test]
    async fn two_waiting_saves_each_wait_only_for_the_lock() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();
        let put = |slot: u32| {
            sent(
                Method::PUT,
                format!("/api/works/{slug}/progress"),
                None,
                format!(r#"{{"edition":"raw","slot":{slot}}}"#),
            )
        };
        let holder = the_lock_held(&state);
        let first = tokio::spawn(timed(router.clone(), put(0)));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let second = tokio::spawn(timed(router.clone(), put(1)));

        let mut slowest = Duration::ZERO;
        while !(first.is_finished() && second.is_finished()) {
            tokio::time::sleep(IN_FLIGHT).await;
            let works = Request::get("/api/works")
                .body(Body::empty())
                .expect("a request");
            let (status, _, took) = timed(router.clone(), works).await;
            assert_eq!(status, StatusCode::OK, "a read while the saves wait");
            slowest = slowest.max(took);
        }
        holder.execute_batch("ROLLBACK").expect("releases the lock");

        for (which, save) in [("first", first), ("second", second)] {
            let (status, body, took) = save.await.expect("the save ran");
            assert!(
                ITS_OWN_WAIT.contains(&took),
                "the {which} save answered after {took:?}: {status} {body}"
            );
            assert_eq!(
                status,
                StatusCode::INTERNAL_SERVER_ERROR,
                "the {which} save"
            );
            // The lock is named in the log alone (O267): the wait above is
            // what shows the save failed on it.
            assert_eq!(
                body, "the library could not answer this request; the server's log says why",
                "the {which} save"
            );
        }
        assert!(slowest < NO_WAIT, "the slowest read took {slowest:?}");
    }

    /// A stale `If-Match` is refused before the correction waits for the lock.
    #[tokio::test]
    async fn a_stale_correction_is_refused_without_waiting_for_the_lock() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();
        let holder = the_lock_held(&state);

        let stale = sent(
            Method::PATCH,
            format!("/api/works/{slug}/alignments"),
            Some(r#""not this work's tag""#.to_string()),
            r#"{"from":"raw","to":"en","a":"2","b":"1"}"#.to_string(),
        );
        let (status, body, took) = timed(router.clone(), stale).await;
        holder.execute_batch("ROLLBACK").expect("releases the lock");

        assert!(took < NO_WAIT, "the refusal took {took:?}: {status} {body}");
        assert_eq!(status, StatusCode::PRECONDITION_FAILED, "{body}");
        assert!(
            body.starts_with("this work has changed since you read it; its ETag is now "),
            "{body}"
        );
    }

    /// Two corrections sent with one tag, both waiting for the lock: the one
    /// that gets it first lands, and the other is refused by the tag it is
    /// checked against once it holds the lock, not let through by the check
    /// it passed before.
    #[tokio::test]
    async fn of_two_corrections_from_one_tag_the_second_to_the_lock_is_refused() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();
        let uri = format!("/api/works/{slug}");
        let (tag, _) = read(&router, &uri).await;
        let correction = || {
            sent(
                Method::PATCH,
                format!("{uri}/alignments"),
                Some(tag.clone()),
                r#"{"from":"raw","to":"en","a":"2","b":"1"}"#.to_string(),
            )
        };
        let holder = the_lock_held(&state);
        let first = tokio::spawn(timed(router.clone(), correction()));
        let second = tokio::spawn(timed(router.clone(), correction()));
        tokio::time::sleep(IN_FLIGHT).await;
        let waiting = !(first.is_finished() || second.is_finished());
        holder.execute_batch("ROLLBACK").expect("releases the lock");

        let mut answered = Vec::new();
        for save in [first, second] {
            let (status, body, _) = save.await.expect("the correction ran");
            answered.push((status, body));
        }
        assert!(
            waiting,
            "a correction had its answer before the lock went: {answered:?}"
        );
        answered.sort_by_key(|(status, _)| *status);
        let statuses: Vec<StatusCode> = answered.iter().map(|(status, _)| *status).collect();
        assert_eq!(
            statuses,
            [StatusCode::OK, StatusCode::PRECONDITION_FAILED],
            "{answered:?}"
        );
    }

    /// Between two of one reader's saves, longer than SQLite's busy handler
    /// takes to back off to its longest sleep.
    const APART: Duration = Duration::from_millis(300);

    /// From the last save to the lock's release: short enough that the last
    /// save is still in the busy handler's short sleeps and so polls first.
    const RELEASED_AFTER: Duration = Duration::from_millis(30);

    /// Sends a progress save on `raw` for each of `slots`, [`APART`], while
    /// another connection holds the write lock, lets the lock go
    /// [`RELEASED_AFTER`] the last, and answers each save's status, and the
    /// positions stored then as `(edition, slot)`.
    async fn saved_in_turn(
        state: &crate::ServeState,
        router: &Router,
        slug: &str,
        slots: &[u32],
    ) -> (Vec<StatusCode>, Vec<(String, u64)>) {
        let holder = the_lock_held(state);
        let mut saves = Vec::new();
        for (sent_before, slot) in slots.iter().enumerate() {
            if sent_before > 0 {
                tokio::time::sleep(APART).await;
            }
            let save = sent(
                Method::PUT,
                format!("/api/works/{slug}/progress"),
                None,
                format!(r#"{{"edition":"raw","slot":{slot}}}"#),
            );
            saves.push(tokio::spawn(timed(router.clone(), save)));
        }
        tokio::time::sleep(RELEASED_AFTER).await;
        let waiting = saves.iter().all(|save| !save.is_finished());
        holder.execute_batch("ROLLBACK").expect("releases the lock");

        let mut answered = Vec::new();
        for save in saves {
            let (status, body, _) = save.await.expect("the save ran");
            answered.push((status, body));
        }
        assert!(
            waiting,
            "a save had its answer before the lock went: {answered:?}"
        );
        let statuses = answered.into_iter().map(|(status, _)| status).collect();
        let response =
            crate::test_support::get(router, &format!("/api/works/{slug}/progress")).await;
        let body = crate::test_support::body_string(response).await;
        let held: serde_json::Value = serde_json::from_str(&body).expect("positions");
        let positions = held["positions"]
            .as_array()
            .expect("a list of positions")
            .iter()
            .map(|position| {
                (
                    position["edition"].as_str().expect("an edition").to_owned(),
                    position["slot"].as_u64().expect("a slot"),
                )
            })
            .collect();
        (statuses, positions)
    }

    /// One reader's two saves, both waiting for the lock: the one that arrived
    /// later is the position stored, whichever of the two takes the freed
    /// lock first.
    #[tokio::test]
    async fn of_one_readers_two_waiting_saves_the_later_arrival_is_stored() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();

        let (statuses, positions) = saved_in_turn(&state, &router, &slug, &[0, 1]).await;

        assert_eq!(statuses, [StatusCode::NO_CONTENT; 2]);
        assert_eq!(positions, [("raw".to_string(), 1)], "the stored position");
    }

    /// Three of one reader's saves queued behind the lock in turn end at the
    /// last one's slot.
    #[tokio::test]
    async fn three_waiting_saves_end_at_the_last_ones_slot() {
        let (router, state, _fixture, slug) =
            crate::test_support::router_over_a_live_library_with_an_edge();

        let (statuses, positions) = saved_in_turn(&state, &router, &slug, &[0, 1, 2]).await;

        assert_eq!(statuses, [StatusCode::NO_CONTENT; 3]);
        assert_eq!(positions, [("raw".to_string(), 2)], "the stored position");
    }
}

/// O267: a page whose file cannot be read is answered by a sentence naming it,
/// and the error, which names the server's path, goes to the log alone.
mod answered_without_a_server_path {
    use super::*;
    use crate::test_support::{self, Reader};

    /// `uri`'s status and body, and the fixture's directory, which the body
    /// must not hold.
    async fn unreadable(uri: &str) -> (StatusCode, String, String) {
        let (router, state, fixture) =
            test_support::router_over_a_scanned_cbz_that_is_not_an_archive();
        assert_eq!(
            state.works.snapshot()[0].id,
            "scanned-v01",
            "the fixture's slug"
        );
        let response = test_support::get(&router, uri).await;
        let status = response.status();
        let body = test_support::body_string(response).await;
        (status, body, fixture.path().display().to_string())
    }

    #[tokio::test]
    async fn a_page() {
        let (status, body, dir) = unreadable("/api/works/scanned-v01/raw/pages/0").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            body,
            "page 0 of `raw` of `scanned-v01` could not be read; the server's log says why"
        );
        assert!(!body.contains(&dir), "{body}");
    }

    #[tokio::test]
    async fn a_tile() {
        let (status, body, dir) = unreadable("/api/works/scanned-v01/raw/pages/0/thumb").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            body,
            "page 0 of `raw` of `scanned-v01` could not be read; the server's log says why"
        );
        assert!(!body.contains(&dir), "{body}");
    }

    /// The cover size, which `serve_page` answers rather than the tile's branch.
    #[tokio::test]
    async fn a_cover_sized_thumb() {
        let (status, body, dir) =
            unreadable("/api/works/scanned-v01/raw/pages/0/thumb?w=256").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            body,
            "page 0 of `raw` of `scanned-v01` could not be read; the server's log says why"
        );
        assert!(!body.contains(&dir), "{body}");
    }

    /// Named as the account asked for it: Manga alone's page 2 is `raw`'s
    /// page 5, part3's first.
    #[tokio::test]
    async fn a_narrowed_page_by_the_number_asked() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let work = merged.state.works.get(&merged.slug).expect("the work");
        let part3 = std::path::PathBuf::from(
            &work.sidecar().editions["raw"]
                .members
                .iter()
                .find(|member| member.path.ends_with("part3.cbz"))
                .expect("part3")
                .path,
        );
        assert!(part3.is_file(), "the fixture's part3: {part3:?}");
        std::fs::write(&part3, b"not an archive").expect("overwrites part3");
        let dir = part3
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the tree");
        let slug = &merged.slug;
        for uri in [
            format!("/api/works/{slug}/raw/pages/2"),
            format!("/api/works/{slug}/raw/pages/2/thumb"),
            format!("/opds/v1.2/works/{slug}/raw/pages/2"),
        ] {
            let response = merged.get(Reader::MangaOnly, &uri, None).await;
            let body = String::from_utf8_lossy(response.body()).into_owned();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}: {body}");
            assert_eq!(
                body,
                format!("page 2 of `raw` of `{slug}` could not be read; the server's log says why"),
                "{uri}"
            );
            assert!(!body.contains(&dir.display().to_string()), "{uri}: {body}");
        }
    }

    /// An edition the work does not have, asked for by a `user` that sees
    /// every library: its view is the edition whole, present or not, so
    /// `locate` is what refuses, and every route answers its words.
    #[tokio::test]
    async fn an_edition_the_work_does_not_have() {
        let tmp = test_support::Fixture::new("no-such-edition");
        let state = Arc::new(test_support::state(&tmp));
        let router = a_user_who_sees_every_library(&state);
        let dir = tmp.path().display().to_string();
        let uris = [
            "/api/works/t/nope/pages/0",
            "/api/works/t/nope/pages/0/thumb",
            "/api/works/t/nope/pages/0/thumb?w=256",
            "/opds/v1.2/works/t/nope/pages/0",
            "/opds/v1.2/works/t/nope/cover",
        ];
        let mut answered = Vec::new();
        for uri in uris {
            let response = test_support::get(&router, uri).await;
            let status = response.status();
            answered.push((uri, status, test_support::body_string(response).await));
        }
        let refused = uris.map(|uri| {
            (
                uri,
                StatusCode::NOT_FOUND,
                "no edition `nope` in `t`".to_string(),
            )
        });
        assert_eq!(answered, refused);
        assert!(
            answered.iter().all(|(_, _, body)| !body.contains(&dir)),
            "{answered:?}"
        );
    }

    /// The log line is the answer, then the error it left out, path and all.
    #[test]
    fn the_log_line() {
        let mut out = Vec::new();
        log_line(
            &mut out,
            "page 0 of `raw` of `w` could not be read",
            "/srv/manga/w.cbz: invalid Zip archive",
        );
        assert_eq!(
            String::from_utf8(out).expect("text"),
            "taiyaku: page 0 of `raw` of `w` could not be read: /srv/manga/w.cbz: invalid Zip archive\n"
        );
    }

    /// The store's directory renamed under a running router: the shared
    /// connection keeps the file it has open, and a save's own connection
    /// (`Library::saving`) cannot open one, with an error naming the store.
    fn move_the_store(fixture: &test_support::Fixture) {
        let db = fixture.path().join("db");
        assert!(
            db.join("library.db").is_file(),
            "the fixture's store: {db:?}"
        );
        std::fs::rename(&db, fixture.path().join("db-moved")).expect("moves the store");
    }

    #[tokio::test]
    async fn a_position_the_store_cannot_save() {
        let (router, state, fixture) = test_support::router_over_a_file_backed_library();
        let slug = state.works.snapshot()[0].id.clone();
        move_the_store(&fixture);
        let response = test_support::send_json(
            &router,
            axum::http::Method::PUT,
            &format!("/api/works/{slug}/progress"),
            &serde_json::json!({ "edition": "raw", "slot": 0 }),
        )
        .await;
        let status = response.status();
        let body = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert_eq!(
            body,
            "the library could not answer this request; the server's log says why"
        );
        assert!(
            !body.contains(&fixture.path().display().to_string()),
            "{body}"
        );
    }

    /// Its `If-Match` is read before the store moves, so the write gets as far
    /// as the save.
    #[tokio::test]
    async fn notes_the_store_cannot_save() {
        let (router, state, fixture) = test_support::router_over_a_file_backed_library();
        let slug = state.works.snapshot()[0].id.clone();
        let uri = format!("/api/works/{slug}/notes");
        let tag = etag_of(&router, &uri).await;
        move_the_store(&fixture);
        let response = router
            .clone()
            .oneshot(
                Request::put(&uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, &tag)
                    .body(Body::from(
                        serde_json::json!({ "work": slug, "notes": [] }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert_eq!(
            body,
            "the library could not answer this request; the server's log says why"
        );
        assert!(
            !body.contains(&fixture.path().display().to_string()),
            "{body}"
        );
    }
}
