//! The built app beside the API — spec `2026-10-04-day-to-day-deployment-design.md`
//! §3.1 and §3.5.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt as _;

use crate::routes::build_router_serving;
use crate::test_support::{Fixture, state};
use crate::web::{HASHED, REVALIDATE};

const INDEX: &str = "<!doctype html><title>the app</title>";
const SECRET: &str = "a file beside the app, never inside it";

/// A built app laid out the way Vite lays one out — `index.html` and
/// `label.html` at the top, content-hashed files under `assets/` — plus one
/// file *beside* the app directory that no request may ever reach.
fn built_app(tmp: &Fixture) -> PathBuf {
    let dist = tmp.join("dist");
    std::fs::create_dir_all(dist.join("assets")).unwrap();
    std::fs::write(dist.join("index.html"), INDEX).unwrap();
    std::fs::write(
        dist.join("label.html"),
        "<!doctype html><title>labeller</title>",
    )
    .unwrap();
    std::fs::write(dist.join("theme.js"), "document.documentElement").unwrap();
    std::fs::write(dist.join("assets/main-Ab12Cd34.js"), "console.log('app')").unwrap();
    std::fs::write(
        dist.join("assets/taiyaku_wasm_bg-Ef56Gh78.wasm"),
        b"\0asm\x01\0\0\0",
    )
    .unwrap();
    std::fs::write(tmp.join("secret.txt"), SECRET).unwrap();
    dist
}

/// One anonymous `GET` through the whole router, app included.
async fn get(tmp: &Fixture, dist: &Path, path: &str) -> (StatusCode, HeaderMap, String) {
    let router = build_router_serving(Arc::new(state(tmp)), Some(dist));
    let response = router
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .expect("answers");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("reads the body");
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}

/// A reload on any of the app's own routes gets `index.html`, and the app's
/// router takes it from there.
///
/// A library's page and an account's are each at its name, escaped where an
/// address needs it, so a path may hold any escape: `ServeDir` decodes one
/// before looking for a file, and `%2F` then reads as a separator, `%25` as a
/// bare `%`. Each still names no file, so each still gets the app.
#[tokio::test]
async fn a_deep_link_answers_the_app() {
    let tmp = Fixture::new("web-deep-link");
    let dist = built_app(&tmp);
    for path in [
        "/",
        "/read/some-work",
        "/series/3",
        "/config/users",
        "/config/new-library",
        "/config/libraries/Light%20novels",
        "/config/libraries/Manga%2FChapters/dirs/101",
        "/config/libraries/What%3F",
        "/config/libraries/No.%20%231",
        "/config/libraries/100%25",
        "/config/libraries/Text%20holding%20%252F",
        "/config/libraries/Tom%20%26%20Jerry",
        "/config/libraries/Tom%20&%20Jerry",
        "/config/libraries/%C3%89mile/dirs/107",
        "/config/libraries/2",
        "/config/new-account",
        "/config/users/Sam%20Jones",
        "/config/users/sam%2Fkim",
        "/config/users/new",
    ] {
        let (status, headers, body) = get(&tmp, &dist, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(body, INDEX, "{path}");
        let kind = headers[header::CONTENT_TYPE].to_str().unwrap();
        assert!(kind.starts_with("text/html"), "{path}: {kind}");
    }
}

/// A path under `/api` or `/opds` that no route matched is a plain 404 — never
/// the app, or a client asking for a route that does not exist would be handed
/// HTML with a 200. A path that only shares the letters is the app's.
#[tokio::test]
async fn the_api_keeps_its_own_404() {
    let tmp = Fixture::new("web-api-404");
    let dist = built_app(&tmp);
    for path in [
        "/api/nothing-here",
        "/api",
        "/api/",
        "/opds/nothing-here",
        "/opds",
    ] {
        let (status, _, body) = get(&tmp, &dist, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(body.is_empty(), "{path} answered with a body: {body}");
    }
    let (status, _, body) = get(&tmp, &dist, "/apiary").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, INDEX);
}

/// No spelling of a path leaves the app's directory. `ServeDir` hands a path it
/// refuses to the fallback rather than answering 404, so what is asserted is
/// the body, not the status.
#[tokio::test]
async fn a_path_outside_the_app_is_never_served() {
    let tmp = Fixture::new("web-traversal");
    let dist = built_app(&tmp);
    for path in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/..%2fsecret.txt",
        "/assets/../../secret.txt",
        "/assets/%2e%2e/%2e%2e/secret.txt",
    ] {
        let (_, _, body) = get(&tmp, &dist, path).await;
        assert!(
            !body.contains(SECRET),
            "{path} reached a file outside the app"
        );
    }
}

/// The sign-in screen is part of the app and has to load before there is any
/// credential, so the app asks for none — and the API behind it still does.
#[tokio::test]
async fn the_app_needs_no_credential_and_the_api_still_does() {
    let tmp = Fixture::new("web-anonymous");
    let dist = built_app(&tmp);
    for path in ["/", "/label.html", "/assets/main-Ab12Cd34.js"] {
        let (status, _, _) = get(&tmp, &dist, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
    }
    let (status, _, _) = get(&tmp, &dist, "/api/me").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// A content-hashed name never changes meaning, so it may be kept a year;
/// everything else — `index.html` above all — is asked for again, which is how
/// a phone picks up a new deployment. Neither is the guarded routes'
/// `private, no-store`.
#[tokio::test]
async fn hashed_assets_are_immutable_and_the_rest_revalidates() {
    let tmp = Fixture::new("web-cache");
    let dist = built_app(&tmp);
    let (status, headers, _) = get(&tmp, &dist, "/assets/main-Ab12Cd34.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], HASHED);
    for path in ["/", "/label.html", "/read/some-work"] {
        let (_, headers, _) = get(&tmp, &dist, path).await;
        assert_eq!(headers[header::CACHE_CONTROL], REVALIDATE, "{path}");
    }
}

/// A hashed name that does not exist is a 404 with no caching instruction.
/// Answering it with `index.html` would hand a browser HTML under a name it may
/// keep for a year.
#[tokio::test]
async fn a_missing_asset_is_a_404_and_is_not_cached() {
    let tmp = Fixture::new("web-missing-asset");
    let dist = built_app(&tmp);
    let (status, headers, body) = get(&tmp, &dist, "/assets/gone-Zz99Yy88.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(body, INDEX);
    assert!(!headers.contains_key(header::CACHE_CONTROL));
}

/// `WebAssembly.instantiateStreaming` refuses a module served as anything but
/// `application/wasm`, and the reader's alignment code is that module.
#[tokio::test]
async fn wasm_is_served_as_application_wasm() {
    let tmp = Fixture::new("web-wasm");
    let dist = built_app(&tmp);
    let (status, headers, _) = get(&tmp, &dist, "/assets/taiyaku_wasm_bg-Ef56Gh78.wasm").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/wasm");
}

/// O277: the app's pages, its scripts, a deep link and both kinds of 404 carry
/// the four security headers. The pages need the policy most, and they are
/// served by the fallback, which a layer applied too early would not wrap.
#[tokio::test]
async fn every_answer_the_app_serves_carries_the_security_headers() {
    let tmp = Fixture::new("web-security-headers");
    let dist = built_app(&tmp);
    for (path, expected) in [
        ("/", StatusCode::OK),
        ("/label.html", StatusCode::OK),
        ("/theme.js", StatusCode::OK),
        ("/assets/main-Ab12Cd34.js", StatusCode::OK),
        ("/read/some-work", StatusCode::OK),
        ("/assets/gone-Zz99Yy88.js", StatusCode::NOT_FOUND),
        ("/api/nothing-here", StatusCode::NOT_FOUND),
    ] {
        let (status, headers, _) = get(&tmp, &dist, path).await;
        assert_eq!(status, expected, "{path}");
        crate::test_support::assert_the_security_headers(&headers, path);
    }
}

/// `--log-requests` sees what the app served, not only the API.
#[tokio::test]
async fn the_request_log_records_what_the_app_served() {
    let tmp = Fixture::new("web-log");
    let dist = built_app(&tmp);
    let log_path = tmp.join("requests.jsonl");
    let mut served = state(&tmp);
    served.log = Some(Arc::new(
        crate::reqlog::Log::new(&log_path).expect("opens the log"),
    ));
    let response = build_router_serving(Arc::new(served), Some(&dist))
        .oneshot(Request::get("/read/some-work").body(Body::empty()).unwrap())
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
    let logged = std::fs::read_to_string(&log_path).expect("the log was written");
    assert!(logged.contains("\"path\":\"/read/some-work\""), "{logged}");
}
