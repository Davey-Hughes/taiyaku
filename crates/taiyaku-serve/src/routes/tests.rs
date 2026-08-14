use super::*;
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

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
    let back: crate::Progress = serde_json::from_slice(&bytes).unwrap();

    crate::test_support::cleanup("taiyaku-test-progress-api");
    assert_eq!(back.edition, "raw");
    assert_eq!(back.slot, 3);
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

/// O12: a progress read must not run on the async worker.
///
/// `get_page` has handed its work to `spawn_blocking` since M2 and these
/// two handlers did not, so a position read — a file open on the same disk
/// a page comes from, behind the same NFS mount — stalled a runtime thread
/// for its whole duration. With a `current_thread` runtime, which is what
/// this test pins deliberately, "a runtime thread" is the whole runtime.
///
/// Staged with a FIFO at the progress path, because opening one for
/// reading blocks until a writer arrives — a real block on the real code
/// path, rather than an injected one. If the read runs on the worker,
/// nothing else on that runtime runs at all, so the canary task never gets
/// scheduled; a watchdog OS thread then rescues the request and records
/// that it had to, so the failure is an assertion rather than a hung test
/// run.
///
/// **The write path gets the same treatment and is not measured here.**
/// `store` writes through a temp file whose name is unique per attempt
/// (see `atomic::temp_path`), so there is no path a test can pre-create
/// as a FIFO. It is covered by inspection and by sharing this one's
/// shape.
#[tokio::test(flavor = "current_thread")]
async fn a_progress_read_does_not_stall_the_runtime() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let state = crate::test_support::state("taiyaku-test-progress-blocking");
    let fifo = crate::progress::progress_path(&state.works[0]);
    let made = std::process::Command::new("mkfifo").arg(&fifo).status();
    if !matches!(&made, Ok(status) if status.success()) {
        crate::test_support::cleanup("taiyaku-test-progress-blocking");
        eprintln!(
            "NOT MEASURED: a_progress_read_does_not_stall_the_runtime \
                 (mkfifo unavailable: {made:?})"
        );
        return;
    }
    let app = build_router(Arc::new(state));

    let canary_ran = Arc::new(AtomicBool::new(false));
    let rescued_by_deadline = Arc::new(AtomicBool::new(false));

    let watchdog = {
        let fifo = fifo.clone();
        let canary_ran = Arc::clone(&canary_ran);
        let rescued_by_deadline = Arc::clone(&rescued_by_deadline);
        std::thread::spawn(move || {
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
            // Read *and* write. Opening a FIFO for writing alone blocks
            // until a reader arrives, which would make this rescue the
            // hang it exists to prevent; `O_RDWR` never blocks. Held
            // briefly so a reader that has not opened yet still finds a
            // writer, then dropped — the drop is what gives the read its
            // EOF and lets the request finish.
            let end = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&fifo);
            std::thread::sleep(std::time::Duration::from_millis(50));
            drop(end);
        })
    };

    // Spawned before the canary, so the runtime polls it first: by the
    // time the canary would be scheduled, a read on the worker has
    // already blocked.
    let request = tokio::spawn(
        app.oneshot(
            Request::get("/api/works/t/progress")
                .body(Body::empty())
                .unwrap(),
        ),
    );
    let canary = tokio::spawn({
        let canary_ran = Arc::clone(&canary_ran);
        async move { canary_ran.store(true, Ordering::SeqCst) }
    });

    request.await.unwrap().unwrap();
    canary.await.unwrap();
    watchdog.join().unwrap();

    crate::test_support::cleanup("taiyaku-test-progress-blocking");
    assert!(
        !rescued_by_deadline.load(Ordering::SeqCst),
        "the runtime polled nothing else for half a second: the progress read is \
             running on the async worker rather than behind spawn_blocking"
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
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            None,
            "{uri} answers a document this server rewrites; a freshness \
                 lifetime on it hands a reader back an edit they have already \
                 made"
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
        assert_eq!(
            revalidated.headers().get(header::CACHE_CONTROL),
            None,
            "{uri} shares `not_modified` with the page routes, and a \
                 client stores what a 304 tells it — a freshness lifetime \
                 here is worth exactly what one on the 200 would be"
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
        &state.works[0],
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
async fn a_successful_truth_write_returns_an_etag() {
    // O35: a PUT response with no ETag turns every later write into a
    // silent no-op. The notes route was fixed for exactly this; truth has
    // the same shape and must not repeat it.
    let (state, _fixture) = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let tag = etag_of(&router, "/api/works/w/truth").await;
    let body = taiyaku_core::TruthDoc::new().to_json();
    let response = router
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
    let ids: Vec<String> = serde_json::from_slice(&body).expect("a JSON array of ids");
    assert_eq!(ids, vec!["t".to_string()]);
}
