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
            Mutex::new(replies.iter().map(ToString::to_string).collect()),
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

    /// What travelled in the body is recorded, not only the path it went to.
    #[test]
    fn post_json_records_the_body_sent() {
        let fake = FakeTransport::new().on_post("/api/graphql", "{}");

        fake.post_json("/api/graphql", r#"{"query":"x"}"#).unwrap();

        assert_eq!(
            fake.requests(),
            vec![r#"POST /api/graphql {"query":"x"}"#.to_string()]
        );
    }

    /// One path, two different answers in order: member A found in Stump,
    /// member B not, from one `FakeTransport`.
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
