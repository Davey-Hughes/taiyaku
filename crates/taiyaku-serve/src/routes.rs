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
/// Every route below is addressed by work, and M3 adds more — a thumbnail, a
/// correction, an OPDS tree — that will be too. One place decides what a
/// request for a work that does not exist gets back, so four routes cannot
/// drift into four different answers to the same question.
fn work<'a>(state: &'a ServeState, id: &str) -> Result<&'a Work, NoSuchWork> {
    state
        .work(id)
        .ok_or_else(|| NoSuchWork(format!("no work `{id}`")))
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
    AxPath((id, edition, index)): AxPath<(String, String, u32)>,
) -> Response {
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    match state.source.page(found, &edition, index) {
        // The type the source reported, not a guess: an `<img>` would sniff
        // octet-stream happily, but a direct navigation downloads it and an
        // OPDS client refuses it.
        Ok(page) => ([(header::CONTENT_TYPE, page.content_type)], page.bytes).into_response(),
        Err(e) => not_found(e),
    }
}

async fn get_progress(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
) -> Response {
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    match crate::progress::load(found) {
        // Nothing recorded is not an error: a work never opened has no
        // position, and 404 would say the work does not exist.
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Ok(Some(p)) => axum::Json(p).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn put_progress(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    axum::Json(progress): axum::Json<crate::Progress>,
) -> Response {
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    if !found.sidecar.editions.contains_key(&progress.edition) {
        return not_found(format!("no edition `{}` in work `{id}`", progress.edition));
    }
    match crate::progress::store(found, &progress) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// Build the router. Separate from `serve` so tests can drive it with
/// `oneshot` and never open a socket.
pub fn build_router(state: Arc<ServeState>) -> Router {
    Router::new()
        .route("/api/works", get(list_works))
        .route("/api/works/{work}", get(get_sidecar))
        .route("/api/works/{work}/{edition}/pages/{index}", get(get_page))
        .route(
            "/api/works/{work}/progress",
            get(get_progress).put(put_progress),
        )
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
                    .body(Body::from(r#"{"edition":"raw","slot":37}"#))
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
        assert_eq!(back.slot, 37);
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
}
