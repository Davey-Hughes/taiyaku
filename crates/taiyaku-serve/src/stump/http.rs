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
/// `ImageRef.url` emits its internally configured address — measured as
/// `http://stump.daveynet.xyz:10801/…` on an instance served publicly over
/// https — so believing it would send every page fetch somewhere unreachable.
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
            .limit(16 * 1024 * 1024)
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
pub(crate) mod test_support {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    use super::Transport;

    /// A `Transport` that answers from a table instead of a network.
    ///
    /// Records the requests it received — path *and* body for POSTs — so a
    /// test can assert both that a lookup was made once and cached rather
    /// than repeated on every page turn, and what was actually sent.
    #[derive(Debug, Default)]
    pub(crate) struct FakeTransport {
        pub posts: HashMap<String, String>,
        /// Per-path queues for `on_post_sequence`: each call to a queued path
        /// consumes the next reply, so one `FakeTransport` can describe a
        /// work whose members answer differently in turn (member A found,
        /// member B not, say) rather than being limited to one reply per
        /// path for the whole test.
        post_sequences: HashMap<String, Mutex<VecDeque<String>>>,
        pub gets: HashMap<String, (u16, String, Vec<u8>)>,
        pub seen: Mutex<Vec<String>>,
    }

    impl FakeTransport {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// Answer every POST to `path` with `reply`.
        pub(crate) fn on_post(mut self, path: &str, reply: &str) -> Self {
            self.posts.insert(path.to_string(), reply.to_string());
            self
        }

        /// Answer successive POSTs to `path` with each of `replies`, in
        /// order — the first call gets `replies[0]`, the second
        /// `replies[1]`, and so on. A call beyond the end of the queue is a
        /// test making more calls than it described, not a silently
        /// repeated reply, so it fails loudly instead.
        pub(crate) fn on_post_sequence(mut self, path: &str, replies: &[&str]) -> Self {
            self.post_sequences.insert(
                path.to_string(),
                Mutex::new(replies.iter().map(|r| r.to_string()).collect()),
            );
            self
        }

        pub(crate) fn on_get(mut self, path: &str, ct: &str, bytes: &[u8]) -> Self {
            self.gets
                .insert(path.to_string(), (200, ct.to_string(), bytes.to_vec()));
            self
        }

        pub(crate) fn on_get_status(mut self, path: &str, status: u16) -> Self {
            self.gets
                .insert(path.to_string(), (status, String::new(), Vec::new()));
            self
        }

        pub(crate) fn requests(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }

    impl Transport for FakeTransport {
        fn post_json(&self, path: &str, body: &str) -> Result<Vec<u8>, String> {
            let prefix = format!("POST {path} ");
            let mut seen = self.seen.lock().unwrap();
            seen.push(format!("{prefix}{body}"));
            let calls_to_path = seen
                .iter()
                .filter(|entry| entry.starts_with(&prefix))
                .count();
            drop(seen);

            if let Some(queue) = self.post_sequences.get(path) {
                let mut queue = queue.lock().unwrap();
                return match queue.pop_front() {
                    Some(reply) => Ok(reply.into_bytes()),
                    None => Err(format!(
                        "fake: POST {path} sequence exhausted after {calls_to_path} calls"
                    )),
                };
            }

            match self.posts.get(path) {
                Some(reply) => Ok(reply.clone().into_bytes()),
                None => Err(format!("fake has no reply for POST {path} with {body}")),
            }
        }

        fn get_bytes(&self, path: &str) -> Result<(u16, String, Vec<u8>), String> {
            self.seen.lock().unwrap().push(format!("GET {path}"));
            match self.gets.get(path) {
                Some((status, ct, bytes)) => Ok((*status, ct.clone(), bytes.clone())),
                None => Err(format!("fake has no reply for GET {path}")),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The gap the fix closes: a test could previously assert only that
        /// *some* POST hit a path, never what travelled in the body. A test
        /// deleting `variables` from a GraphQL request would have stayed
        /// green.
        #[test]
        fn post_json_records_the_body_sent() {
            let fake = FakeTransport::new().on_post("/api/graphql", "{}");

            fake.post_json("/api/graphql", r#"{"query":"x"}"#).unwrap();

            assert_eq!(
                fake.requests(),
                vec![r#"POST /api/graphql {"query":"x"}"#.to_string()]
            );
        }

        /// One path, two different answers in order — the shape Task 5's
        /// central test needs: member A found in Stump, member B not, from
        /// one `FakeTransport`.
        #[test]
        fn on_post_sequence_answers_successive_calls_in_order() {
            let fake = FakeTransport::new().on_post_sequence(
                "/api/graphql",
                &[
                    r#"{"data":{"media":{"nodes":[{"id":"a"}]}}}"#,
                    r#"{"data":{"media":{"nodes":[]}}}"#,
                ],
            );

            let first = fake.post_json("/api/graphql", "member-a").unwrap();
            let second = fake.post_json("/api/graphql", "member-b").unwrap();

            assert!(String::from_utf8(first).unwrap().contains("\"id\":\"a\""));
            assert!(String::from_utf8(second).unwrap().contains(r#""nodes":[]"#));
        }

        /// A queue running out must fail loudly, naming the path and how
        /// many calls were made, rather than silently repeating the last
        /// reply — a test that calls more times than it described should
        /// say so.
        #[test]
        fn an_exhausted_sequence_names_the_path_and_call_count() {
            let fake = FakeTransport::new().on_post_sequence("/api/graphql", &["{}"]);
            fake.post_json("/api/graphql", "one").unwrap();

            let failed = fake
                .post_json("/api/graphql", "two")
                .expect_err("the queue must run out, not repeat its last reply");

            assert!(
                failed.contains("/api/graphql"),
                "must name the path: {failed}"
            );
            assert!(
                failed.contains('2'),
                "must say how many calls were made: {failed}"
            );
        }
    }
}
