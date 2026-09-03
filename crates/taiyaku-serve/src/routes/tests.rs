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
    crate::test_support::authenticating(super::build_router(state))
}

/// The three shapes a reader depends on: the work list to find a work, the
/// sidecar verbatim so the client can resolve without asking again, and a
/// page by index. A 404 for an edition that does not exist matters as much
/// as the successes — a reader that gets 200 and an empty body for a typo
/// shows a blank page and says nothing.
#[tokio::test]
async fn the_reader_can_list_a_work_read_its_sidecar_and_fetch_a_page() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-routes"));
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

    crate::test_support::cleanup("taiyaku-test-routes");
}

/// Task 14: `GET /api/works` answers objects rather than a bare list of ids.
#[tokio::test]
async fn listing_works_answers_objects_rather_than_bare_ids() {
    let (router, _state) = crate::test_support::router_over_a_library().await;
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
    let (router, _state) = crate::test_support::router_over_a_library().await;
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
    let (router, _state) = crate::test_support::router_over_a_library().await;
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
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-routes-list-no-row",
    ));
    let app = build_router(state);
    let body =
        crate::test_support::body_string(crate::test_support::get(&app, "/api/works").await).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(parsed[0]["slug"], "t");
    assert_eq!(parsed[0]["title"], "T");
    assert!(parsed[0]["series"].is_null());
    assert!(parsed[0]["volume"].is_null());
    crate::test_support::cleanup("taiyaku-test-routes-list-no-row");
}

/// `status.md` previously claimed no test dispatched an out-of-range
/// page index through the router — true of the `/api/…` route
/// specifically (the OPDS stream route had its own such test since Task
/// 6), but the sentence did not say so and so overclaimed. This closes
/// the actual gap: the fixture edition has 4 pages, so index 4 is one
/// past the end.
#[tokio::test]
async fn an_out_of_range_page_index_dispatched_through_the_router_names_the_length() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-routes-page-past-end",
    ));
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

    crate::test_support::cleanup("taiyaku-test-routes-page-past-end");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-routes-page-nan"));
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

    crate::test_support::cleanup("taiyaku-test-routes-page-nan");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-progress-api"));
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

    crate::test_support::cleanup("taiyaku-test-progress-api");
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
    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-progress-set",
    ));
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

    crate::test_support::cleanup("taiyaku-test-progress-set");
}

/// O3: the edition check was only half the guard. A position naming a real
/// edition and a slot past its end is stored garbage in the same way a
/// position naming no edition is — the reader is sent back somewhere that
/// cannot be resolved, and nothing says why. It is also the shape a bug in
/// a client produces, which is exactly when a server should refuse rather
/// than persist.
#[tokio::test]
async fn progress_past_the_end_of_the_edition_is_refused() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-progress-past-end"));
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

    crate::test_support::cleanup("taiyaku-test-progress-past-end");
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
    let state = crate::test_support::state("taiyaku-test-progress-spread-slot");
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

    crate::test_support::cleanup("taiyaku-test-progress-spread-slot");
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "4 pages with one spread is 5 slots, so slot 4 is the last one: {message}"
    );
}

/// O12/O8: a progress read must not run on the async worker.
///
/// ***This test cannot observe the property the paragraphs below claim it
/// proves — O71, opened 2026-08-26. The prose is kept as written and marked
/// here rather than rewritten, because the reasoning in it is careful and is
/// exactly what made the gap invisible.*** What it misses is the router:
/// `auth::guard` resolves inside its own `spawn_blocking` and takes the mutex
/// wait with it, so by the time the handler under test touches SQLite the
/// reactor is already free and the canary runs whatever the handler does.
/// Proved rather than suspected — `routes::get_progress` was moved wholly
/// onto the async worker, the exact defect this names, and this test still
/// passed. Ten `routes.rs` handlers stand behind it with nothing else
/// guarding that property. The shape of the fix costs no new mechanism and is
/// already in the tree: `curation/tests.rs::no_curation_route_stalls_the_runtime`
/// calls handlers directly rather than through the router, and does have
/// teeth. **Do not model a new stall test on this one.**
///
/// Restores `a_progress_read_does_not_stall_the_runtime`, removed when Task
/// 13 moved progress off files and onto `taiyaku-library`'s SQLite
/// connection, on the premise — recorded then, and wrong — that "an
/// in-memory SQLite call has no comparable primitive to block a thread on
/// for a controlled duration." `Library::conn()` returns a
/// `MutexGuard<'_, Connection>` from a `Mutex<Connection>`
/// (`taiyaku-library/src/lib.rs`): a lock a spawned thread can hold for a
/// controlled span exactly as the old test held a FIFO open, and any
/// handler that calls `library.conn()` inside its `spawn_blocking` closure
/// blocks on it for as long as the guard is held. `get_progress` does
/// exactly that (`crate::progress::load` → `library.conn()`), so this is
/// O8's failure class — a blocking call inside an async handler stalling
/// the reactor — proven on the same code path, with the mutex standing in
/// for the FIFO as the controlled block.
///
/// **The block's release is gated on the canary, not a fixed sleep.** A
/// fixed sleep shorter than the watchdog's own deadline would let an
/// unwrapped handler's block happen to end before the deadline fires,
/// passing for the wrong reason — the same trap a fixed duration would set
/// here that the old FIFO test avoided by having its own watchdog decide
/// when to open the FIFO. Here the watchdog thread is the one holding
/// `library.conn()`: it waits for the canary or half a second, whichever
/// comes first, and only then drops the guard — so how long the block
/// actually lasts is decided by whether the reactor was ever free to run
/// anything else, which is the property under test, rather than by a
/// duration picked in advance.
#[tokio::test(flavor = "current_thread")]
async fn a_progress_read_does_not_stall_the_runtime() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let (router, state) = crate::test_support::router_over_a_library().await;

    // A position to read back, so the eventual read proves more than "no
    // content" — it proves the value written before the block still comes
    // back correctly once the block lifts.
    let put = router
        .clone()
        .oneshot(
            Request::put("/api/works/w/progress")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    let canary_ran = Arc::new(AtomicBool::new(false));
    let rescued_by_deadline = Arc::new(AtomicBool::new(false));
    let (lock_taken_tx, lock_taken_rx) = std::sync::mpsc::channel();

    let watchdog = {
        let library = Arc::clone(&state.library);
        let canary_ran = Arc::clone(&canary_ran);
        let rescued_by_deadline = Arc::clone(&rescued_by_deadline);
        std::thread::spawn(move || {
            let guard = library.conn();
            lock_taken_tx
                .send(())
                .expect("the test thread is still waiting on this channel");
            // Half a second for a task on an idle runtime to be polled
            // once. Passing costs the first 10 ms of it; only a failure
            // waits the whole way out.
            let mut waited = 0;
            while !canary_ran.load(Ordering::SeqCst) && waited < 50 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                waited += 1;
            }
            if !canary_ran.load(Ordering::SeqCst) {
                rescued_by_deadline.store(true, Ordering::SeqCst);
            }
            drop(guard);
        })
    };
    // Do not fire the request until the guard is confirmed held — otherwise
    // the request's own `spawn_blocking` closure could win the mutex first
    // and this would measure nothing.
    lock_taken_rx
        .recv()
        .expect("the watchdog thread is still running");

    // Spawned before the canary, so the runtime polls it first: by the time
    // the canary would be scheduled, a read behind `spawn_blocking` has
    // already blocked on the mutex.
    let request = tokio::spawn({
        let router = router.clone();
        async move {
            router
                .oneshot(
                    Request::get("/api/works/w/progress")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }
    });
    let canary = tokio::spawn({
        let canary_ran = Arc::clone(&canary_ran);
        async move { canary_ran.store(true, Ordering::SeqCst) }
    });

    let response = request.await.unwrap();
    canary.await.unwrap();
    watchdog.join().unwrap();

    assert!(
        !rescued_by_deadline.load(Ordering::SeqCst),
        "the runtime polled nothing else for half a second: the progress read is \
             running on the async worker rather than behind spawn_blocking"
    );

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let positions = doc["positions"].as_array().expect("positions is an array");
    assert_eq!(positions.len(), 1, "one write, one position");
    assert_eq!(positions[0]["edition"].as_str().unwrap(), "raw");
    assert_eq!(
        positions[0]["slot"].as_u64().unwrap(),
        1,
        "the value written before the block still comes back correctly"
    );
}

/// The other half of what progress has to get right. A slot means nothing
/// without the edition it is a slot of, so a body naming an edition this
/// work does not have is stored garbage: the reader is sent back to a
/// position in a document that cannot be resolved, and nothing anywhere
/// says why. Accepting it silently is worse than 404, which is why the
/// guard exists — and it was the one thing here with no test.
#[tokio::test]
async fn progress_for_an_edition_the_work_does_not_have_is_refused() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-progress-edition"));
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

    crate::test_support::cleanup("taiyaku-test-progress-edition");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-routes-etag-stale"));
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

    crate::test_support::cleanup("taiyaku-test-routes-etag-stale");
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
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-routes-etag-member",
    ));
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

    crate::test_support::cleanup("taiyaku-test-routes-etag-member");
    assert_ne!(
        tag0, tag1,
        "page 0 and page 1 share one member and its one sha256 — the \
             offset is the only thing that can tell them apart, and a tag \
             keyed on the member hash alone would make these equal"
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
    let mut state = crate::test_support::state("taiyaku-test-routes-reqlog");
    let (log, log_path) = crate::test_support::log("taiyaku-test-routes-reqlog");
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

    crate::test_support::cleanup("taiyaku-test-routes-reqlog");

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
    let mut state = crate::test_support::state("taiyaku-test-routes-reqlog-headers");
    let (log, log_path) = crate::test_support::log("taiyaku-test-routes-reqlog-headers");
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

    crate::test_support::cleanup("taiyaku-test-routes-reqlog-headers");

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
    let mut state = crate::test_support::state("taiyaku-test-routes-reqlog-query");
    let (log, log_path) = crate::test_support::log("taiyaku-test-routes-reqlog-query");
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

    crate::test_support::cleanup("taiyaku-test-routes-reqlog-query");

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
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-routes-sidecar-etag",
    ));
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

    crate::test_support::cleanup("taiyaku-test-routes-sidecar-etag");
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
    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-routes-correct",
    ));
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
    crate::test_support::cleanup("taiyaku-test-routes-correct");
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

    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-routes-correct-noop",
    ));
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
    crate::test_support::cleanup("taiyaku-test-routes-correct-noop");
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
    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-routes-correct-stale",
    ));
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
    crate::test_support::cleanup("taiyaku-test-routes-correct-stale");
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
    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-routes-correct-bare",
    ));
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
    crate::test_support::cleanup("taiyaku-test-routes-correct-bare");
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
    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-routes-correct-statuses",
    ));
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

    crate::test_support::cleanup("taiyaku-test-routes-correct-statuses");
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
    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-routes-correct-unpair",
    ));
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
    crate::test_support::cleanup("taiyaku-test-routes-correct-unpair");
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
    let state = Arc::new(crate::test_support::state_with_two_editions(
        "taiyaku-test-routes-correct-range",
    ));
    let router = build_router(state.clone());
    let tag = etag_of(&router, "/api/works/t").await;

    let (status, message) = correct_with_body(
        &router,
        "t",
        &tag,
        r#"{"from":"raw","to":"en","a":"2-3","b":"1"}"#,
    )
    .await;

    crate::test_support::cleanup("taiyaku-test-routes-correct-range");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-get"));
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

    crate::test_support::cleanup("taiyaku-test-notes-get");
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
    let (router, _state) = crate::test_support::router_over_a_library().await;
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
    let (router, _state) = crate::test_support::router_over_a_library().await;
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-304"));
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

    crate::test_support::cleanup("taiyaku-test-notes-304");
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
}

/// A work this server does not hold says so, in the same words every other
/// route addressed by work uses.
#[tokio::test]
async fn notes_for_an_unknown_work_are_a_404_naming_it() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-no-work"));
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

    crate::test_support::cleanup("taiyaku-test-notes-no-work");
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("nope"), "the refusal must name it: {body}");
}

/// The notes a work already holds are what it serves, and the validator
/// moves with them — a tag that did not change when the document did would
/// let a stale client write over a note it never saw.
#[tokio::test]
async fn stored_notes_are_served_and_change_the_validator() {
    let state = crate::test_support::state("taiyaku-test-notes-stored");
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

    crate::test_support::cleanup("taiyaku-test-notes-stored");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-put"));
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

    crate::test_support::cleanup("taiyaku-test-notes-put");
    assert_ne!(fresh, tag, "the document changed, so the tag must have");
    assert_eq!(parsed.notes.len(), 1);
    assert_eq!(parsed.notes[0].body, "first");
}

/// The lost-update case itself, which is the reason any of this exists:
/// two devices read the same document, both write, and the second must be
/// told rather than quietly replacing the first reader's note.
#[tokio::test]
async fn a_second_writer_holding_the_same_tag_is_refused_rather_than_winning() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-race"));
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

    crate::test_support::cleanup("taiyaku-test-notes-race");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-bad-slot"));
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

    crate::test_support::cleanup("taiyaku-test-notes-bad-slot");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-concurrent"));
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
            crate::test_support::cleanup("taiyaku-test-notes-concurrent");
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

    crate::test_support::cleanup("taiyaku-test-notes-concurrent");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-notes-garbage"));
    let app = build_router(state);
    let tag = etag_of(&app, "/api/works/t/notes").await;

    let (status, message) = write_notes(&app, Some(&tag), "{\"nope\":1}".to_string()).await;

    crate::test_support::cleanup("taiyaku-test-notes-garbage");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-routes-unknown"));
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::get("/api/works/nope/raw/pages/0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    crate::test_support::cleanup("taiyaku-test-routes-unknown");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-routes-list"));
    let app = build_router(state);

    let response = app
        .oneshot(Request::get("/api/works").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    crate::test_support::cleanup("taiyaku-test-routes-list");
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
    let state = crate::test_support::state("taiyaku-test-routes-list-db-broken");
    state
        .library
        .conn()
        .execute("DROP TABLE work", [])
        .expect("drop the table `summary` queries");
    let app = build_router(Arc::new(state));

    let response = crate::test_support::get(&app, "/api/works").await;

    crate::test_support::cleanup("taiyaku-test-routes-list-db-broken");
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

    let state = crate::test_support::state("taiyaku-test-two-readers");
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

    crate::test_support::cleanup("taiyaku-test-two-readers");
}

/// The same, for notes: two readers annotate one work and neither sees the
/// other's.
#[tokio::test]
async fn two_readers_hold_separate_notes_on_one_work() {
    use base64::Engine as _;

    let state = crate::test_support::state("taiyaku-test-two-readers-notes");
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

    crate::test_support::cleanup("taiyaku-test-two-readers-notes");
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
fn router_with_two_works_one_withheld(dir: &str) -> (axum::Router, Arc<ServeState>) {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
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
    let (router, _state) =
        router_with_two_works_one_withheld("taiyaku-test-routes-grant-works-list");

    let response = crate::test_support::get(&router, "/api/works").await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    crate::test_support::cleanup("taiyaku-test-routes-grant-works-list");
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
fn router_over_a_scanned_work_under_a_withheld_root(dir: &str) -> (axum::Router, Arc<ServeState>) {
    let fixture_root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&fixture_root);
    std::fs::create_dir_all(&fixture_root).unwrap();
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
    let (router, state) = router_over_a_scanned_work_under_a_withheld_root(
        "taiyaku-test-routes-grant-alignments-write",
    );
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

    crate::test_support::cleanup("taiyaku-test-routes-grant-alignments-write");
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
