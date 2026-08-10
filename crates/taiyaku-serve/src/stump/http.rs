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
mod ureq_tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};

    use super::*;

    /// What one request looked like on the wire.
    struct Captured {
        line: String,
        headers: Vec<String>,
        body: String,
    }

    /// A one-shot HTTP server on a loopback port, answering `reply` and handing
    /// back what it was sent.
    ///
    /// Hand-rolled on `TcpListener` rather than pulled in as a dependency:
    /// what is under test is the four lines of `ureq` configuration in this
    /// file, and a test server is 30 lines of `write!`. `UreqTransport` is the
    /// one thing in `stump` that talks to the outside world, and it is exactly
    /// the thing `Transport` exists to let every other test avoid — so nothing
    /// offline exercised it, and every one of its methods could be replaced by
    /// a constant unnoticed.
    fn serve_once(
        status: u16,
        content_type: &str,
        body: &[u8],
    ) -> (String, std::sync::mpsc::Receiver<Captured>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        let status_line = format!("HTTP/1.1 {status} X");
        let ct = content_type.to_string();
        let body = body.to_vec();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("one connection");
            let captured = read_request(&stream);
            let mut stream = stream;
            let _ = write!(
                stream,
                "{status_line}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
            let _ = stream.flush();
            let _ = tx.send(captured);
        });

        (format!("http://127.0.0.1:{port}"), rx)
    }

    fn read_request(stream: &TcpStream) -> Captured {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();

        let mut headers = Vec::new();
        let mut length = 0usize;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            let header = header.trim_end().to_string();
            if header.is_empty() {
                break;
            }
            if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap_or(0);
            }
            headers.push(header);
        }

        let mut body = vec![0u8; length];
        if length > 0 {
            reader.read_exact(&mut body).unwrap();
        }
        Captured {
            line: line.trim_end().to_string(),
            headers,
            body: String::from_utf8_lossy(&body).to_string(),
        }
    }

    fn header<'a>(c: &'a Captured, name: &str) -> Option<&'a str> {
        let name = format!("{}:", name.to_ascii_lowercase());
        c.headers
            .iter()
            .find(|h| h.to_ascii_lowercase().starts_with(&name))
            .map(|h| h[name.len()..].trim())
    }

    /// The two limits are bounds on how much of a reply is read, and a bound
    /// nothing asserts is a bound every arithmetic mutation of it survives.
    ///
    /// The page limit has to clear this corpus's largest pages, which is what
    /// the comment above it claims and what `ureq`'s 10 MB default does not.
    #[test]
    fn the_read_limits_are_the_sizes_the_comments_claim() {
        assert_eq!(MAX_PAGE_BYTES, 536_870_912, "512 MiB");
        assert_eq!(MAX_REPLY_BYTES, 16_777_216, "16 MiB");
        // Through bindings rather than as literal constant expressions:
        // `assert!` on a wholly-constant condition is `clippy::assertions_on_constants`,
        // and its suggested `const { assert!(…) }` would move the check to
        // compile time — which sounds better and is worse here, because a
        // mutated constant would then fail to *build* and be reported as an
        // unviable mutant rather than a caught one.
        let largest_corpus_page: u64 = 430 * 1024 * 1024;
        let (page_limit, reply_limit) = (MAX_PAGE_BYTES, MAX_REPLY_BYTES);
        assert!(
            page_limit > largest_corpus_page,
            "the corpus reaches 430 MB a page; ureq's 10 MB default would truncate one"
        );
        assert!(reply_limit < page_limit, "a metadata reply is not an image");
    }

    /// `url` joins, `bearer` spells the header. Both are one line, both were
    /// replaceable by `String::new()` unnoticed, and a wrong `url` sends every
    /// request to the wrong place while a wrong `bearer` fails every one.
    ///
    /// The trailing-slash trim is the part with judgement in it: `new` takes a
    /// configured base that a person typed, and paths already start with `/`.
    #[test]
    fn the_base_url_is_joined_without_doubling_the_slash() {
        let t = UreqTransport::new("http://stump.example:10801/", "sekrit");
        assert_eq!(
            t.url("/api/graphql"),
            "http://stump.example:10801/api/graphql"
        );
        assert_eq!(t.bearer(), "Bearer sekrit");

        let untrimmed = UreqTransport::new("http://stump.example:10801", "k");
        assert_eq!(
            untrimmed.url("/x"),
            "http://stump.example:10801/x",
            "a base with no trailing slash joins the same way"
        );
    }

    /// What actually goes out on the wire, against a real socket.
    ///
    /// `post_json` could return `Ok(vec![])` and no offline test noticed,
    /// because everything else in `stump` runs against `FakeTransport`. This
    /// asserts the four things the configuration is for: the path is below the
    /// base, the bearer token is attached, the body is sent as typed, and the
    /// reply is handed back rather than invented.
    #[test]
    fn post_json_sends_the_body_with_a_bearer_and_returns_the_reply() {
        let (base, rx) = serve_once(200, "application/json", br#"{"data":{"ok":true}}"#);
        let t = UreqTransport::new(base, "sekrit");

        let got = t
            .post_json("/api/graphql", r#"{"query":"x"}"#)
            .expect("the local server answers");

        assert_eq!(got, br#"{"data":{"ok":true}}"#.to_vec());
        let sent = rx.recv().expect("the server captured a request");
        assert_eq!(sent.line, "POST /api/graphql HTTP/1.1");
        assert_eq!(sent.body, r#"{"query":"x"}"#);
        assert_eq!(header(&sent, "authorization"), Some("Bearer sekrit"));
        assert_eq!(header(&sent, "content-type"), Some("application/json"));
    }

    /// A page fetch returns its status, its type and its bytes — all three,
    /// and the status *as a value*.
    #[test]
    fn get_bytes_returns_the_status_content_type_and_body() {
        let (base, rx) = serve_once(200, "image/jpeg", &[0xFF, 0xD8, 0xFF, 0xE0]);
        let t = UreqTransport::new(base, "k");

        let (status, content_type, bytes) = t.get_bytes("/api/book/1/page/2").expect("answered");

        assert_eq!(status, 200);
        assert_eq!(content_type, "image/jpeg");
        assert_eq!(bytes, vec![0xFF, 0xD8, 0xFF, 0xE0]);
        let sent = rx.recv().expect("captured");
        assert_eq!(sent.line, "GET /api/book/1/page/2 HTTP/1.1");
        assert_eq!(header(&sent, "authorization"), Some("Bearer k"));
    }

    /// The one piece of judgement in `get_bytes`'s configuration:
    /// `http_status_as_error(false)`, so a 404 from a page route comes back as
    /// the number 404 rather than as a transport error. The caller says "this
    /// book has no page N" better than an error string could — and without
    /// this, `composite` would read a missing page as Stump being *down* and
    /// trip the cooldown latch.
    #[test]
    fn a_404_is_an_answer_rather_than_a_transport_error() {
        let (base, _rx) = serve_once(404, "text/plain", b"no such page");
        let t = UreqTransport::new(base, "k");

        let (status, _, body) = t
            .get_bytes("/api/book/1/page/999")
            .expect("a 404 is returned, not raised");

        assert_eq!(status, 404);
        assert_eq!(body, b"no such page".to_vec());
    }

    /// A reply with no `Content-Type` gets the generic one rather than an
    /// empty string, which would reach a browser as a header it cannot use.
    #[test]
    fn a_reply_with_no_content_type_falls_back_to_octet_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&stream);
            let _ = stream
                .write_all(b"HTTP/1.1 200 X\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi");
        });

        let t = UreqTransport::new(format!("http://127.0.0.1:{port}"), "k");
        let (_, content_type, _) = t.get_bytes("/x").expect("answered");

        assert_eq!(content_type, "application/octet-stream");
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
