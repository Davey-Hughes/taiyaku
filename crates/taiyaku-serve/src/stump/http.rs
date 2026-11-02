//! Reaching a Stump instance, and the seam that lets tests not.
//!
//! Every call goes through `Transport`, so the whole of `stump` is testable
//! against recorded responses and the suite does not depend on the NAS being
//! up. The one test that talks to a real instance is the gated equivalence
//! check.

use std::time::Duration;

/// One Stump instance, reachable.
///
/// The base URL is configured and never taken from a response. Stump's own
/// `ImageRef.url` emits its internally configured address, even on an
/// instance served publicly over https, so believing it would send every
/// page fetch somewhere unreachable.
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// POST a JSON document to a path below the base URL, and read the reply.
    fn post_json(&self, path: &str, body: &str) -> Result<Vec<u8>, String>;

    /// GET a path below the base URL as bytes, with its status and type.
    ///
    /// The status is returned rather than raised because a 404 from a page
    /// route is an answer — "this book has no page N" — and the caller says
    /// so better than a transport error could.
    fn get_bytes(&self, path: &str) -> Result<(u16, String, Vec<u8>), String>;
}

/// The real thing: `ureq`, blocking, with a bearer token.
///
/// Blocking is deliberate. `PageSource` is synchronous and the boundary to
/// async is `spawn_blocking` at the router, so a blocking client is the one
/// that does not need a runtime it cannot have.
#[derive(Debug)]
pub struct UreqTransport {
    base: String,
    key: String,
}

/// Pages in this corpus reach 430 MB, and `ureq` reads at most 10 MB unless
/// told otherwise — so a default read would truncate the largest pages into a
/// corrupt image rather than failing.
const MAX_PAGE_BYTES: u64 = 512 * 1024 * 1024;

/// How much of a GraphQL reply may be read.
///
/// Named rather than left inline in `post_json` for the reason `MAX_PAGE_BYTES`
/// is: it is a bound, and a bound nothing can name is a bound nothing can
/// assert. Much smaller than the page limit because these replies are library
/// metadata — ids, titles and page counts — not images.
const MAX_REPLY_BYTES: u64 = 16 * 1024 * 1024;

/// How long a page fetch may take before it is treated as a failure.
///
/// Paired with `composite::DOWN_COOLDOWN`, which must be **longer** than this.
/// The latch suppresses attempts only after a failure has returned, so with a
/// downed instance a reader meets one full timeout per cooldown; a cooldown
/// shorter than the timeout means it meets one almost continuously.
pub const GET_TIMEOUT_SECONDS: u64 = 20;

impl UreqTransport {
    /// `base` is the scheme, host and port, with no trailing slash.
    pub fn new(base: impl Into<String>, key: impl Into<String>) -> Self {
        let base = base.into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            key: key.into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.key)
    }
}

impl Transport for UreqTransport {
    fn post_json(&self, path: &str, body: &str) -> Result<Vec<u8>, String> {
        let mut response = ureq::post(self.url(path))
            .header("Authorization", self.bearer())
            .header("Content-Type", "application/json")
            .config()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .send(body)
            .map_err(|e| format!("stump: POST {path}: {e}"))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_REPLY_BYTES)
            .read_to_vec()
            .map_err(|e| format!("stump: POST {path}: reading reply: {e}"))
    }

    fn get_bytes(&self, path: &str) -> Result<(u16, String, Vec<u8>), String> {
        let mut response = ureq::get(self.url(path))
            .header("Authorization", self.bearer())
            .config()
            // A 404 from a page route is an answer, not a transport failure.
            .http_status_as_error(false)
            // Shorter than it looks like it should be, deliberately. This is
            // the longest a reader waits before the composite gives up and
            // reads the page off disk instead, and the request-path latch
            // only suppresses attempts once a failure has *returned* — so
            // this timeout, not the cooldown, is what bounds the worst stall
            // a downed Stump can inflict. The pages it fetches are ordinary
            // manga scans of a megabyte or two; the corpus's 430 MB pages are
            // read locally and never travel this path.
            .timeout_global(Some(Duration::from_secs(GET_TIMEOUT_SECONDS)))
            .build()
            .call()
            .map_err(|e| format!("stump: GET {path}: {e}"))?;

        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_PAGE_BYTES)
            .read_to_vec()
            .map_err(|e| format!("stump: GET {path}: reading body: {e}"))?;
        Ok((status, content_type, bytes))
    }
}

#[cfg(test)]
mod ureq_tests;

#[cfg(test)]
pub(crate) mod test_support;
