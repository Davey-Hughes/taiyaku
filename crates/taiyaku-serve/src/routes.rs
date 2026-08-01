//! HTTP. Everything interesting happens below this file.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Path as AxPath, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use crate::{ServeState, Work};

/// An error a client can act on, rather than a bare status.
///
/// Every failure here names the thing that was not found, because a reader
/// showing a blank page with no explanation is the failure mode this whole
/// project exists to avoid.
fn not_found(message: String) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

/// A URL that does not address anything, as against one addressing something
/// this server does not have.
///
/// Separate from `not_found` because they are genuinely different and a client
/// is entitled to tell them apart: `pages/4` names a page an edition does not
/// have, `pages/abc` does not name a page at all.
fn bad_request(message: String) -> Response {
    (StatusCode::BAD_REQUEST, message).into_response()
}

/// The page index a URL carries, or the refusal to send instead.
///
/// O5: the two routes addressed by page index — this module's `/api/…` route
/// and the OPDS stream — used to declare the index as `u32` in the extractor
/// and let axum's `Path` rejection answer a malformed one. That message is
/// phrased in terms of the Rust type it failed to parse into and the tuple
/// position it sat at ("Cannot parse value at index 2 with value `abc` to a
/// `u32`"), which names two things a client knows nothing about, and it was
/// the only message this server sent that this server had not written.
///
/// Shared rather than written twice, for the reason `ServeState::page` is
/// shared: the two routes are two spellings of one operation, and answering a
/// malformed index two different ways is the drift that starts the moment
/// there are two copies.
pub(crate) fn page_index(raw: &str, edition: &str) -> Result<u32, NotAPageIndex> {
    raw.parse::<u32>().map_err(|_| {
        NotAPageIndex(format!(
            "`{raw}` is not a page number; pages of `{edition}` are numbered from 0"
        ))
    })
}

/// A URL segment that was supposed to be a page index and was not.
///
/// A named type rather than a bare `Response`, for the same reason
/// `NoSuchWork` below is one: a `Response` is 128 bytes and
/// `Result<u32, Response>` trips `clippy::result_large_err`.
#[derive(Debug)]
pub(crate) struct NotAPageIndex(String);

impl IntoResponse for NotAPageIndex {
    fn into_response(self) -> Response {
        bad_request(self.0)
    }
}

/// A request naming a work this server does not hold.
///
/// A named type rather than a bare `Response` because a `Response` is 128
/// bytes and `Result<&Work, Response>` trips `clippy::result_large_err`. It
/// carries the status and the wording together, which is the point: one
/// answer to one question, wherever it is asked.
#[derive(Debug)]
struct NoSuchWork(String);

impl IntoResponse for NoSuchWork {
    fn into_response(self) -> Response {
        not_found(self.0)
    }
}

/// The work a route names, or the refusal to send instead.
///
/// Every route below is addressed by work, and M3 added more — a thumbnail, a
/// stream, a work feed. `ServeState::work_or_missing` is the one place that
/// decides the wording; this just wraps it in the `Response`-carrying type
/// `get_sidecar` — the one handler here that touches no disk, and so is the
/// one that still runs on the async worker — wants.
fn work<'a>(state: &'a ServeState, id: &str) -> Result<&'a Work, NoSuchWork> {
    state.work_or_missing(id).map_err(NoSuchWork)
}

/// Why a progress request could not be answered.
///
/// Two kinds, and they are not one status: a work this server does not hold,
/// or an edition or slot the work does not have, is the client's mistake and a
/// 404; a position file that will not read or write is this server's and a
/// 500. `get_page` needs no such type because both of its failures are 404s.
/// Progress is the one route where the distinction has to survive the trip
/// back out of `spawn_blocking`, which flattens a handler's failures into one
/// `Result` however different they were on the way in.
#[derive(Debug)]
enum NoProgress {
    NotFound(String),
    Failed(String),
}

impl IntoResponse for NoProgress {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => not_found(message),
            Self::Failed(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
        }
    }
}

async fn list_works(State(state): State<Arc<ServeState>>) -> Response {
    let ids: Vec<&str> = state.works.iter().map(|w| w.id.as_str()).collect();
    axum::Json(ids).into_response()
}

async fn get_sidecar(State(state): State<Arc<ServeState>>, AxPath(id): AxPath<String>) -> Response {
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    // Served verbatim from the parsed document rather than by re-reading the
    // file, so what the client resolves against is exactly what this server
    // loaded.
    (
        [(header::CONTENT_TYPE, "application/json")],
        found.sidecar.to_json(),
    )
        .into_response()
}

async fn get_page(
    State(state): State<Arc<ServeState>>,
    // `String` rather than `u32`, so a malformed index is refused by
    // `page_index` in this server's own words rather than by axum's `Path`
    // rejection in terms of a Rust type — O5.
    AxPath((id, edition, raw)): AxPath<(String, String, String)>,
) -> Response {
    let index = match page_index(&raw, &edition) {
        Ok(index) => index,
        Err(refusal) => return refusal.into_response(),
    };
    // Reading a page blocks: a zip decompression from disk, or an HTTP round
    // trip to Stump. On a tokio worker that stalls the whole worker, and this
    // corpus holds pages of 430 MB — with the prefetching the filmstrip and
    // swap both need, one reader would occupy several of them.
    //
    // `PageSource` stays synchronous deliberately. The boundary belongs here,
    // where there is exactly one of it, rather than in each implementation.
    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id)?;
        state.page(found, &edition, index)
    })
    .await;

    match fetched {
        Ok(Ok(page)) => ([(header::CONTENT_TYPE, page.content_type)], page.bytes).into_response(),
        Ok(Err(e)) => not_found(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("page read panicked: {e}"),
        )
            .into_response(),
    }
}

async fn get_progress(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
) -> Response {
    // O12: a file read, on the same disk a page comes from and behind the same
    // NFS mount. `get_page`'s reasoning above applies to it unchanged — these
    // two handlers simply did not get it when M2 wrote them.
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoProgress::NotFound)?;
        crate::progress::load(found).map_err(NoProgress::Failed)
    })
    .await;

    match loaded {
        // Nothing recorded is not an error: a work never opened has no
        // position, and 404 would say the work does not exist.
        Ok(Ok(None)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Some(p))) => axum::Json(p).into_response(),
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("progress read panicked: {e}"),
        )
            .into_response(),
    }
}

async fn put_progress(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    axum::Json(progress): axum::Json<crate::Progress>,
) -> Response {
    // Validation moves in here with the write rather than staying outside it.
    // It reads the in-memory sidecar and blocks on nothing, but splitting the
    // handler across the boundary would mean looking the work up twice — once
    // to validate and once inside the closure, which cannot borrow across it.
    let stored = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoProgress::NotFound)?;
        let edition = found
            .sidecar
            .editions
            .get(&progress.edition)
            .ok_or_else(|| {
                NoProgress::NotFound(format!("no edition `{}` in work `{id}`", progress.edition))
            })?;
        // Counted in slots rather than pages, because that is what a position
        // is: `progress.rs` records slots so a spread cannot lose which half
        // was reached, and an edition that joins spreads has more slots than
        // pages. Checking against `total_pages()` would refuse the last slot
        // of every such edition — One Piece v092 is spreads throughout.
        let slots = edition.slots(found.sidecar.work.reading).len() as u32;
        if progress.slot >= slots {
            return Err(NoProgress::NotFound(format!(
                "`{}` has {slots} slots in work `{id}`; there is no slot {}",
                progress.edition, progress.slot
            )));
        }
        crate::progress::store(found, &progress).map_err(NoProgress::Failed)
    })
    .await;

    match stored {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("progress write panicked: {e}"),
        )
            .into_response(),
    }
}

/// Build the router. Separate from `serve` so tests can drive it with
/// `oneshot` and never open a socket.
pub fn build_router(state: Arc<ServeState>) -> Router {
    let log = state.log.clone();
    Router::new()
        .route("/api/works", get(list_works))
        .route("/api/works/{work}", get(get_sidecar))
        .route("/api/works/{work}/{edition}/pages/{index}", get(get_page))
        .route(
            "/api/works/{work}/progress",
            get(get_progress).put(put_progress),
        )
        .merge(crate::opds::routes())
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let log = log.clone();
                async move {
                    let Some(log) = log else {
                        return next.run(request).await;
                    };
                    let method = request.method().to_string();
                    // Path *and* query string: a client asking for
                    // `?width=1536` is invisible if only `path()` is
                    // logged, and this server's total absence of resize
                    // support makes that more important to catch, not less.
                    let path = request
                        .uri()
                        .path_and_query()
                        .map(|pq| pq.as_str())
                        .unwrap_or_else(|| request.uri().path())
                        .to_string();
                    // Read here, before the request is consumed: by the time
                    // a response exists, what it was asked with is gone.
                    // Filtered to `LOGGED_HEADERS` at this end rather than
                    // in `reqlog`, which deliberately knows nothing about
                    // `http` types — see `Log::begin`.
                    let headers = crate::reqlog::LOGGED_HEADERS
                        .iter()
                        .filter_map(|name| {
                            let value = request.headers().get(*name)?.to_str().ok()?;
                            Some(((*name).to_string(), value.to_string()))
                        })
                        .collect();
                    let started = log.begin(headers);
                    let response = next.run(request).await;
                    let status = response.status().as_u16();
                    // The body's own known length, not the Content-Length
                    // header: axum sets that header when it serialises the
                    // response, which is after this middleware has already
                    // run — so reading the header here reports 0 for every
                    // request, and bytes-per-page is one of the numbers this
                    // log exists to produce.
                    let bytes = {
                        use axum::body::HttpBody;
                        response.body().size_hint().exact().unwrap_or(0)
                    };
                    log.finish(started, &method, &path, status, bytes);
                    response
                }
            },
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
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
    #[tokio::test]
    async fn the_last_slot_of_a_spread_carrying_edition_is_in_range() {
        let mut state = crate::test_support::state("taiyaku-test-progress-spread-slot");
        state.works[0]
            .sidecar
            .editions
            .get_mut("raw")
            .expect("the fixture defines `raw`")
            .spreads = vec![0];
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
    /// (see `progress::unique_temp_path`), so there is no path a test can
    /// pre-create as a FIFO. It is covered by inspection and by sharing this
    /// one's shape.
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
}
