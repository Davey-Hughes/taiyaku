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
/// sixty-odd `Request::get(...)` calls below: the header is the same on all of
/// them, and threading it through by hand would put the interesting part of
/// each test one line further from its assertion. The guard itself is asserted
/// over in `auth::tests`, which calls the real `build_router` directly.
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

/// Task 14: `GET /api/works` answers objects rather than a bare list of ids.
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

/// A work loaded the pre-Task-15 way — `load_works`, paired with an empty
/// library and no row at all (`test_support::state`) — still answers a slug
/// and a title; only `series` and `volume` degrade, and to the same `null` a
/// row nothing has scanned yet would answer. `taiyaku serve <sidecar>...`
/// itself no longer takes this path since Task 15 (it imports first, so a
/// served work always has a row) — but `load_works` is still public, still
/// used directly by this crate's own fixtures and by the corpus-gated tests
/// in `corpus.rs`, and must not regress to a null title for whoever still
/// calls it that way.
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

/// `status.md` previously claimed no test dispatched an out-of-range
/// page index through the router — true of the `/api/…` route
/// specifically (the OPDS stream route had its own such test since Task
/// 6), but the sentence did not say so and so overclaimed. This closes
/// the actual gap: the fixture edition has 4 pages, so index 4 is one
/// past the end.
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

/// O5: the one failure on this route that did not say what was wrong.
///
/// An index past the end names the edition and its length; an index that
/// is not a number at all fell through to axum's `Path` rejection, which
/// answers a 400 phrased in terms of the Rust type it failed to parse
/// into. Comprehensible, and the only message in this server written by
/// something other than this server.
///
/// The status stays 400 rather than becoming the 404 the sibling failure
/// gives, deliberately: `pages/4` names a page this edition does not have,
/// and `pages/abc` does not name a page at all. Those are different
/// things and a client is entitled to tell them apart. What they now share
/// is that the message comes from here and says what was expected.
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
                // so 3 is its last slot. It used to be 37, which named a
                // position the work has never had — harmless while
                // nothing checked, and a lie about what round-trips once
                // something does.
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
    // slot and updated — no other top-level field, and no field leaking
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
            "updated".to_string()
        ]),
        "each position must have exactly edition, slot and updated fields"
    );

    assert_eq!(entry["edition"].as_str().unwrap(), "raw");
    assert_eq!(entry["slot"].as_u64().unwrap(), 3);
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

/// O3: the edition check was only half the guard. A position naming a real
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
// `works.rs`'s `a_replaced_sidecar_is_what_the_next_reader_sees`.
#[allow(clippy::result_large_err)]
#[tokio::test]
async fn the_last_slot_of_a_spread_carrying_edition_is_in_range() {
    let tmp = crate::test_support::Fixture::new("progress-spread-slot");
    let state = crate::test_support::state(&tmp);
    state.works[0]
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
/// **An empty grant, and that is chosen.** For a principal carrying roots,
/// `get_page`, `get_thumb` and `patch_alignments` wait on the library's mutex
/// first for their grant check, and the watchdog measures a call's first wait,
/// so it would measure that check rather than the closures this test is for.
/// The check has its own test, `no_grant_check_stalls_the_runtime`, since O115.
/// Until O115 closed it ran on the async worker, and `docs/notes.md`, "O71",
/// records those stalls measured.
fn as_the_fixture_account() -> axum::http::Extensions {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::Admin,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        roots: Vec::new(),
    });
    extensions
}

/// **No `/api/works` route does its blocking work on the async worker** — O12's
/// property, held for all ten handlers that have a closure to hold it in.
///
/// Every handler here that reads or writes the library, or reads a page, does
/// it inside `spawn_blocking`, because on the async worker that work stalls
/// every other request the runtime is serving: `get_page`'s comment gives the
/// reasoning and the others cite it. **Until O71 closed, 2026-09-14, the one
/// test behind that was `a_progress_read_does_not_stall_the_runtime`, and it
/// measured something else.** It drove `GET /progress` through the router, so
/// `auth::guard`'s own `spawn_blocking` took the mutex wait before any handler
/// ran: with `get_progress`'s closure moved onto the async worker, the whole
/// workspace still passed. It is `auth::tests::the_guard_does_not_stall_the_runtime`
/// now, because the guard is what it does see.
///
/// **So each handler is called directly**, through
/// [`crate::test_support::the_runtime_stalls_on`], as
/// `curation::tests::no_curation_route_stalls_the_runtime` calls its six — and
/// for that test's reason, every handler rather than the one the old test
/// named: a rule that holds for most of a module is a rule the next handler is
/// written against wrongly. `get_sidecar` is not here because it has no
/// closure: it answers from the sidecar already in memory.
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
/// the position written comes back, which is what the old test asserted of its
/// own read.
///
/// Each handler's closure was moved onto the async worker by hand, one at a
/// time, and this failed naming that handler; `docs/notes.md`, "O71", has
/// the line each failed at.
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

    let position: crate::Progress =
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
        get_truth(State(Arc::clone(&state)), w(), HeaderMap::new()),
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
        get_thumb(State(Arc::clone(&state)), page(), as_the_fixture_account()),
    )
    .await;
    assert!(
        !stalled,
        "get_thumb (GET /api/works/{{work}}/{{edition}}/pages/{{index}}/thumb) ran on the async \
         worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let tag = sidecar_etag(
        &state
            .work_or_missing("w")
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
/// administrator's grant is always empty (O86).
fn as_a_reader_granted(roots: Vec<i64>) -> axum::http::Extensions {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::User,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        roots,
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
/// For a reader whose grant names roots, `refuse_if_hidden` and
/// `visible_editions_or_refuse` read the library to answer. Until O115 closed,
/// 2026-09-15, both took `state.library.conn()` in the handler itself, on the
/// async worker, before any `spawn_blocking`. The test above drives an empty
/// grant, which never reaches the library, so it could not see that.
///
/// **Every handler that calls either check, not the four O115 named.** The
/// item listed `get_page`, `get_thumb`, `get_sidecar` and `patch_alignments`.
/// `refuse_if_hidden` is `pub(crate)`, and `opds::stream_page`, `opds::cover`,
/// `download::edition` and `download::member` call it the same way.
///
/// **The watchdog measures each call's first wait on the library's mutex**:
/// [`crate::test_support::the_runtime_stalls_on`] holds it once and lets it go
/// when the canary runs. For every handler here that first wait is the grant
/// check, and a second wait on the async worker after it would pass. That is
/// why `patch_alignments` asks its three questions, the work and then each
/// side of the correction, in one trip rather than three.
///
/// **Every call is asserted to succeed**, because a handler refused before its
/// check reached the library would be free for the wrong reason. Root 1 holds
/// every member of the fixture's work. The stalls are collected rather than
/// asserted one at a time, so a failing run names every handler that stalled.
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
        get_thumb(State(Arc::clone(&state)), page(), granted()),
    )
    .await;
    if stalled {
        stalled_on.push("get_thumb");
    }
    assert_answered("get_thumb", response, StatusCode::OK).await;

    let tag = sidecar_etag(
        &state
            .work_or_missing("w")
            .expect("the fixture work")
            .sidecar(),
    );
    let mut if_match = HeaderMap::new();
    if_match.insert(
        header::IF_MATCH,
        tag.parse().expect("an ETag is a header value"),
    );
    let (stalled, response) = the_runtime_stalls_on(
        &library,
        patch_alignments(
            State(Arc::clone(&state)),
            w(),
            if_match,
            granted(),
            Bytes::from_static(br#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#),
        ),
    )
    .await;
    if stalled {
        stalled_on.push("patch_alignments");
    }
    assert_answered("patch_alignments", response, StatusCode::OK).await;

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

    assert!(
        stalled_on.is_empty(),
        "for a reader whose grant names roots, these asked it on the async worker: {stalled_on:?}"
    );
}

/// **A hidden work is refused before its validator is answered.** `get_page`,
/// `get_sidecar` and `opds::stream_page` each answer a matching
/// `If-None-Match` with a 304 before reading anything, and each asks the grant
/// first, so a reader granted a root holding nothing of the work is told the
/// work does not exist rather than that the tag it sent is current. O115 moved
/// that check off the async worker; this holds the order it had to keep.
///
/// **Granted root 1, which holds the work, the same requests answer 304.**
/// That half shows the tags are the real ones: a tag that matched nothing
/// would be refused by a hidden work whichever order a handler asked in.
#[tokio::test]
async fn a_hidden_work_is_refused_before_its_validator_is_answered() {
    use axum::extract::{Path as AxPath, Query, State};
    use axum::http::HeaderMap;

    let (state, _fixture) = crate::test_support::state_whose_grant_waits_on_the_library();
    let found = state.work_or_missing("w").expect("the fixture work");
    let (sha256, offset) = locate_member(found, "raw", 0).expect("page 0 of `raw`");
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

/// **A hidden work is refused before its correction is read.**
/// `patch_alignments` parses the body before it asks the grant, to lend the
/// check the two editions a correction names, and refuses a body that did not
/// parse only once the grant has answered. So a reader granted a root holding
/// nothing of the work is told the work does not exist rather than that its
/// body is malformed. Granted root 1, which holds the work, the same body is
/// refused as malformed, which shows the body is one the handler refuses.
#[tokio::test]
async fn a_hidden_work_is_refused_before_its_correction_is_read() {
    use axum::body::Bytes;
    use axum::extract::{Path as AxPath, State};
    use axum::http::HeaderMap;

    let (state, _fixture) = crate::test_support::state_whose_grant_waits_on_the_library();
    for (root, expected) in [(1, StatusCode::BAD_REQUEST), (2, StatusCode::NOT_FOUND)] {
        let response = patch_alignments(
            State(Arc::clone(&state)),
            AxPath("w".to_string()),
            HeaderMap::new(),
            as_a_reader_granted(vec![root]),
            Bytes::from_static(b"not a correction"),
        )
        .await;
        assert_answered(
            &format!("patch_alignments, granted root {root}"),
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
/// guard exists — and it was the one thing here with no test.
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

/// O1, shipped: a page's `ETag` is checkable before anything expensive
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

/// The trap this parameter introduces. One page now has a representation
/// per size, so a tag that named only the member and offset would let a
/// client holding the small one be answered 304 for the large one and go
/// on drawing a soft page with nothing to point at.
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

    // And the same size still validates, so the parameter has not cost the
    // conditional request that O1 shipped.
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

/// O106: a resized page's tag is its cache key in quotes. Both used to be
/// built separately, and they drifted apart. The cache file name has carried
/// `PAGE_FORMAT_VERSION` since #35, and the tag never did. So bumping the
/// version would regenerate every cached page while every browser kept
/// validating against its old copy and got a `304` for it.
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
/// part of it must not validate. Until O106 a resized page's tag was
/// `"<sha256>-p<offset>-r<edge>"`, and every browser that has read a page at
/// a size is holding one. Under O105 the bytes behind it could have been an
/// empty file. Answering that tag with `304` would keep such a browser drawing
/// nothing after the server was fixed.
///
/// The old tag is built from the full-size tag, `"<sha256>-p<offset>"`, which
/// O106 does not change. The first assertion checks that the full-size tag
/// still has that shape, so the old tag below is the one browsers really hold.
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

/// O106 review: a resized page's tag is built from the clamped width, which
/// is the width its cache file is written under. Built from `?w=` as sent, a
/// request above `MAX_FIT_LONG_EDGE` would be served the ceiling's rendering
/// under a tag of its own. Raising the ceiling would then change what that
/// request is served and leave its tag alone, which is this item's defect
/// again. Nothing tested it: the tag-equals-key test calls `page_etag` with
/// literal arguments and never goes through `get_page`.
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

/// A member has one `sha256` and, in this fixture, two pages. The third
/// test the brief calls for: this is the one that would catch keying an
/// `ETag` on the member hash alone, which would answer every page after
/// the first with the first page's own tag.
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
/// request a hash taken from the last member passed every test. The review of
/// this branch found that.
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
        let path = crate::cover::thumb_cache_path(&dir, sha, offset);
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

/// O108's review: a page that already fits inside `?w=` is served its own
/// bytes, read by its index in the edition.
///
/// The read that serves such a page takes the index where the tag and the cache
/// key take the offset. Every other resized request in these tests is smaller
/// than its page. So when `get_page` still read a page that fits a second time
/// in an arm of its own, reading by the offset there served page 0 of `ab`,
/// grey, with every test green, and the review of O108's branch found it.
/// Since O107 there is no second read: the page is served from the fetch that
/// found it fits, and this still pins that fetch to the index. A width of 16
/// is the page's own size.
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
/// It used to be read twice. `cached_or_generated_page` read the page to learn
/// that it fit and dropped the bytes, and `get_page` read it again to serve it.
/// Both reads served the same bytes, so no assertion on the response could see
/// the second one. This counts the reads through `CountingSource`.
///
/// The four requests are the four `strace` counted on the real corpus before
/// the fix: one archive open at full size, two for a page that fits, one for a
/// resize and none for a cache hit. They run here in that order and must read
/// once, once, once and not at all. Each read is `b.cbz`'s first page, because
/// page 2 of `ab` is that page.
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
}

/// O1: every route whose bytes are fixed by a member's content says how
/// long they may be reused without asking.
///
/// The `ETag` shipped before this and shrank a repeat to about 300 bytes;
/// it never removed the round trip, which is the part a reader waits on.
/// All four byte routes are checked rather than the one this was first
/// noticed on, because O5 already recorded once what fixing only the
/// `/api/…` route is worth: Panels reaches pages through the OPDS stream,
/// and the cover is the only route a cold grid view touches at all.
///
/// The `304` is checked for the same header, and it is the case most
/// easily left out: RFC 9111 §4.3.4 has a client refresh its stored
/// response from the `304`'s headers, so omitting it here would leave the
/// entry exactly as stale as it was and cost a round trip on the very
/// next read.
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
        // constant takes, so it could not have caught what it was there
        // to catch: this was `max-age=86400`, and a work id is the
        // sidecar's filename stem, which every sidecar in the corpus
        // shares. One URL, two volumes, and a day before the client asks.
        assert!(
            policy.contains("no-cache"),
            "{uri} must be revalidated rather than assumed fresh: a work id \
                 is not unique, so this URL does not name these bytes — it was \
                 measured serving two different volumes' page 10 under two \
                 different ETags. Got: {policy}"
        );
    }

    // Both routes that carry a validator, revalidated — not just the
    // `/api/…` one. O5 already recorded once what checking only that one
    // is worth: Panels reaches pages through the OPDS stream, so a `304`
    // asserted only here would be asserted on the route nothing uses.
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
/// **Both statuses are checked, and the second one is why this test
/// exists in the shape it does.** The first version of this looked only
/// at the `200`s and passed while the server was actively telling clients
/// they could reuse a sidecar for a day: `not_modified` is shared by
/// `get_page`, `get_sidecar`, `get_notes` and the OPDS stream, so putting
/// the page constant inside it leaked a freshness lifetime onto the `304`
/// of every route that shares it. A client caches what the `304` says
/// (RFC 9111 §4.3.4), so the leak was worth exactly as much as putting it
/// on the `200` would have been, and no test saw it.
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
        // **No freshness lifetime, which is not the same as no header.** This
        // asserted the header's absence, and absence was the wrong shape once
        // these documents became per-reader: a proxy in front — which spec A2
        // §11 puts there — would store one reader's notes and replay them to
        // another, and RFC 9111's bar on caching authenticated responses keys on
        // `Authorization`, which the browser path never sends. `no-store` grants
        // no lifetime *and* forbids the store, so it satisfies this test's own
        // stated reason more strongly than absence did. Found by the second
        // whole-branch review.
        let directive = response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok());
        assert!(
            // `is_some_and`, not `is_none_or`: the first attempt at this was
            // loosened to tolerate absence, which made it pass for the very
            // routes that had been missed — `/api/works/w/notes` among them.
            // Found by the third whole-branch review.
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
        // The 304 too, and for the reason this assertion always gave: a client
        // stores what a `304` tells it (RFC 9111 §4.3.4), so a lifetime here is
        // worth exactly what one on the 200 would be — and a *shared* cache
        // storing a per-reader 304 is the same replay the 200 would be. The
        // layer covers both because it wraps the response rather than the
        // handler.
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
/// answered from a log that records no request headers, and this one
/// recorded none. An `ETag` is a line of code; knowing whether it would
/// buy anything is the part that needs evidence, and the evidence has to
/// come from a real session with a real client.
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

/// A client asking for `?width=1536` has to be visible in the log: this
/// server serves no resized images at all, which makes it *more*
/// important to see the parameter, not less — logging `path()` alone
/// would make it look like no client ever asks for one, when the truth
/// is only that nothing here answers it yet.
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
/// three different answers, not two. Task 6 turns `None` into a 428 and
/// `Some(false)` into a 412 — collapsing the first into the second would
/// tell a client that sent no validator it lost a race it never entered.
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
    state.works[0]
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

/// The milestone in one test: correct a pairing, and have the document say
/// so afterwards.
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
    let on_disk = std::fs::read_to_string(&state.works[0].path).unwrap();
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
        state.works[0].sidecar().alignments[0]
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

    let path = state.works[0].path.clone();
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
/// alone cannot tell "skipped" from "wrote the same bytes again", and this
/// filesystem was measured to give two `atomic::write` calls microseconds
/// apart the identical mtime down to the nanosecond — so mtime cannot
/// either. A rename always replaces the destination with a fresh inode,
/// whether or not the bytes changed, so the inode is the one signal that
/// distinguishes "no write happened" from "a write happened that changed
/// nothing".
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

    let before_bytes = std::fs::read(&state.works[0].path).unwrap();
    let before_ino = std::fs::metadata(&state.works[0].path).unwrap().ino();
    let second_tag = etag_of(&router, "/api/works/t").await;

    // The same assertion again: the document already says slot 2 pairs
    // with slot 1, so this is `Corrected::AlreadyTrue`.
    let second_status = correct(
        &router,
        &second_tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
    )
    .await;

    let after_bytes = std::fs::read(&state.works[0].path).unwrap();
    let after_ino = std::fs::metadata(&state.works[0].path).unwrap().ino();
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

    let after = state.works[0].sidecar();
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
/// without this test the suite stays green. Task 2's own tests pin which
/// variant `Sidecar::correct` returns; which status it becomes is this
/// route's to answer, and so this route's to test.
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
        // `b` omitted entirely. The other half of the same rule: absence
        // must not spell "unpair", so this is refused rather than quietly
        // taking a pairing away. The refusal names the field, because
        // serde's own "missing field `b`" is the most useful thing a
        // client could be told here.
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
    // A work this server does not hold, which is refused before the body
    // is ever looked at.
    let missing = correct_with_body(
        &router,
        "nosuchwork",
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
    )
    .await;
    let unchanged = state.works[0].sidecar().alignments[0].runs.len();

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
/// `"b": null` is the one thing an absent `b` used to mean, and it is the
/// reason `b` is nullable rather than simply a `String`. Nothing in `web/`
/// can send it yet — the reader has no "this page pairs with nothing"
/// gesture — so without this test the whole justification for keeping the
/// field nullable would rest on a path nothing exercises.
#[tokio::test]
async fn an_explicit_null_unpairs_the_slot() {
    let tmp = crate::test_support::Fixture::new("routes-correct-unpair");
    let state = Arc::new(crate::test_support::state_with_two_editions(&tmp));
    let router = build_router(state.clone());
    let tag = etag_of(&router, "/api/works/t").await;
    let before = taiyaku_core::resolve(&state.works[0].sidecar(), "raw", "en", 2);

    let (status, _) = correct_with_body(
        &router,
        "t",
        &tag,
        r#"{"from":"raw","to":"en","a":"2","b":null}"#,
    )
    .await;

    let after = state.works[0].sidecar();
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

/// `serve/truth.rs:3` states the rule: one serialisation shared by the file,
/// the response body and the `ETag` over that body, so a validator can never
/// be computed over bytes a client was not sent. Task 13 makes it one
/// serialisation shared by the PROJECTION, the body and the validator — the
/// same rule with one fewer writer. Driven over `router_over_a_library`
/// rather than `test_support::state`, because this is the path Task 13
/// actually rewired: a work with a real row in `state.library`, reached
/// through `works::find_row` rather than through `works::row_id_for_write`
/// minting one.
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
        &state.works[0],
        1,
        &crate::notes::Notes {
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
            }],
        },
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
/// detector: measured by removing the guard and running this five times,
/// two of those runs still happened to serialise and passed. One round is
/// therefore a test that can miss the very defect it exists for, which is
/// worse than no test — it reports green for a reason it never checked.
/// Twenty rounds put the chance of every one of them serialising at
/// roughly `0.4^20`, and the same sabotage now fails immediately.
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
/// words, not serde's — the same rule O5 established for a malformed page
/// index and `patch_alignments` already follows for a malformed
/// correction.
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
    // silent no-op. The notes route was fixed for exactly this; truth has
    // the same shape and must not repeat it.
    //
    // **And the document is read back**, which is the half this test used to
    // be missing. It sent an empty `TruthDoc::new()` and asserted the status
    // and the header only — a body indistinguishable from what a no-op
    // produces — so `truth::store`'s entire body could be replaced with
    // `Ok(())` and all 609 tests in this crate and `taiyaku-cli` stayed green
    // (whole-branch review 2, I4). This is the only save path the labelling
    // tool has: a regression here loses 75 pages of hand work behind a 200
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

/// `?w=1` must mean "no resize", not "resize to one pixel".
///
/// The filter that does it could be `>= 1` — admitting 1 — with every
/// other test still green, because no test asked for a width of 1 and the
/// comment above the filter claimed `fit_at` would decline it anyway. It
/// does not: `fit_at` returns `None` only when the page is *already*
/// within the requested edge, so at `w=1` a 16-pixel page is re-encoded
/// down to one pixel and handed to the reader as a JPEG.
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
    // Task 14: a JSON array of objects rather than of bare ids — `slug` is
    // this object's `id`.
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

/// **Two readers hold two positions in one work**, which the constant that
/// answered user 1 for both could not express.
///
/// Spec A2 §5.1 records the shape of the problem: `progress` and `note` have
/// carried `user_id` in their primary key since migration 1 — `progress`'s is
/// `(user_id, work_id, edition)` since migration 9 — so the database could
/// always hold two readers, and it was the server that could only ask about
/// one.
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
    let slug = state.works[0].id.clone();
    assert!(
        !state.works[0].has_document(),
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
// The grant gap: `list_works` (`GET /api/works`) applied none of spec C1's
// per-root visibility — it iterates `state.works` and returns every work the
// server holds, with no reference to `Principal.roots` at all. The
// whole-branch review closed the equivalent hole on the page routes
// (`refuse_if_hidden`) and commit 5d41bd6 closed it on the OPDS mirror
// (`opds::visible_slugs`); this is the last route in that class.
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
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = build_router(state.clone());
    (router, state)
}

/// **The gap itself.** `GET /api/works` must answer the same grant `opds::works`
/// and the page routes already do: a restricted account learns the id, title,
/// series and volume of a work outside every root it holds, and this asserts
/// it does not, mirroring `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant`
/// for the JSON route.
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

// ---------------------------------------------------------------------------
// The write leak: `patch_alignments` resolved its target by slug alone
// (`state.work_or_missing`), consulting no `Extensions` and no `Principal` at
// all — unlike every other route in this file. A restricted account could
// therefore both read the sidecar this handler returns and write shared
// `alignment`/`run` rows for a work outside every root it held. This is worse
// than the six read leaks: an alignment is shared library truth rather than
// per-account state, so O60's "nothing leaks" reasoning for progress, notes
// and truth does not carry to it.
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
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: fixture_root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = build_router(state.clone());
    (router, state)
}

/// **The write leak itself.** Before the fix, `patch_alignments` asked
/// nothing about the grant, so a restricted account could write shared
/// `alignment`/`run` rows — library truth every other reader sees — for a
/// work outside every root it holds.
///
/// Both the response and the rows are checked, not only the status: a 404
/// that still let the write through, or a 200 that happened not to change
/// anything, would each be a colour change rather than evidence that the
/// grant was actually consulted.
#[tokio::test]
async fn a_restricted_account_cannot_patch_alignments_on_a_work_in_a_withheld_root() {
    let tmp = crate::test_support::Fixture::new("routes-grant-alignments-write");
    let (router, state) = router_over_a_scanned_work_under_a_withheld_root(&tmp);
    let slug = state.works[0].id.clone();
    // Computed directly from the state the running server holds, rather than
    // fetched through `GET /api/works/{slug}` — that route is already fixed
    // and would itself answer 404 to this restricted account, which is
    // exactly what would make an attacker compute this offline instead.
    let tag = super::sidecar_etag(&state.works[0].sidecar());

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
        status,
        StatusCode::NOT_FOUND,
        "a restricted account must not be able to correct a work outside every \
         root it holds: {body}"
    );
    assert_eq!(
        body,
        format!("no work `{slug}`"),
        "refused by the grant, in `get_sidecar`'s own words, not by some other 404"
    );
    assert_eq!(
        hand_writes, 0,
        "no correction may reach the shared rows for a work outside the grant, \
         whatever the response said"
    );
}
