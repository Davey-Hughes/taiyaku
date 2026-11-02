//! The built web app, served beside the API when `taiyaku serve --web DIR` is
//! given — spec `2026-10-04-day-to-day-deployment-design.md` §3.1.
//!
//! **Every path no route matched is the app's**, so a reload on a deep link
//! such as `/read/some-work` gets `index.html`, and the app's own router takes
//! it from there. Two exceptions keep the API's answers its own. A path under
//! `/api` or `/opds` that no route matched is the plain 404, never the app.
//! And a missing file under `/assets/` is a 404 too:
//! Vite names those files by content hash, and a browser may keep whatever such
//! a name answers for a year.
//!
//! **No credential is asked for here.** The sign-in screen is part of the app
//! and has to load before there is anything to sign in with. Everything the
//! app then asks for goes through `/api`, behind the guard.
//!
//! `tower-http`'s `ServeDir` does the serving — content types, ranges,
//! `If-Modified-Since`, and refusing a path that would leave the directory — so
//! none of that is written here.

use std::path::Path;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tower::ServiceExt as _;
use tower_http::services::{ServeDir, ServeFile};

/// `Cache-Control` for a file under `/assets/`, whose name carries its content
/// hash and so never changes meaning.
pub(crate) const HASHED: &str = "public, max-age=31536000, immutable";

/// `Cache-Control` for everything else the app serves, `index.html` above all:
/// a phone has to ask again to pick up a new deployment.
pub(crate) const REVALIDATE: &str = "no-cache";

/// Whether `path` belongs to the API, whose unmatched paths stay 404.
///
/// A segment boundary rather than a bare prefix, so `/apiary` is the app's.
pub(crate) fn is_api(path: &str) -> bool {
    ["/api", "/opds"].iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// The two ways a path reaches the built app's directory.
#[derive(Clone, Debug)]
pub(crate) struct Web {
    /// Everything but `/assets/`: the file if there is one, else `index.html`.
    app: ServeDir<ServeFile>,
    /// `/assets/`: the file if there is one, else a 404 — never `index.html`.
    assets: ServeDir,
}

impl Web {
    pub(crate) fn new(dir: &Path) -> Self {
        Self {
            app: ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))),
            assets: ServeDir::new(dir),
        }
    }

    /// Answer one request that no API route matched.
    pub(crate) async fn answer(self, request: Request) -> Response {
        let path = request.uri().path();
        if is_api(path) {
            return StatusCode::NOT_FOUND.into_response();
        }
        let hashed = path.starts_with("/assets/");
        let served = if hashed {
            self.assets.oneshot(request).await
        } else {
            self.app.oneshot(request).await
        };
        let mut response = match served {
            Ok(response) => response.map(Body::new),
            Err(never) => match never {},
        };
        let status = response.status();
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            let policy = if hashed { HASHED } else { REVALIDATE };
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static(policy));
        }
        response
    }
}

#[cfg(test)]
mod tests;
