//! What a client asked for, and when.
//!
//! Automated UI testing of Panels is a dead end — App Store builds are
//! FairPlay-encrypted, so the Simulator cannot install them — so the request
//! log is the instrument instead. Prefetch depth, page-turn concurrency,
//! requested image dimensions, re-request patterns and how a client walks a
//! feed are all visible in what a client asks for and when — literally,
//! since every line carries its own arrival time.
//!
//! **It measures how Panels fetches, not how Panels reads.** Gestures,
//! transitions, zoom and spread handling are invisible here and want a person
//! writing down what the app gets right. That is a later task.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// The request headers a line records, and the only ones it records.
///
/// An allowlist rather than everything a client sends. Everything would grow
/// each line without bound and would put a `Cookie` or an `Authorization`
/// into a file that gets read by scripts and pasted into issues; these five
/// are the ones that answer questions this log exists to ask:
///
/// - `if-none-match` and `if-modified-since` — whether a client would use a
///   cache header at all, which is the whole of O1. An `ETag` is a line of
///   code; whether it buys anything is the part that needs a real session
///   with a real client to answer.
/// - `range` — whether a client fetches pages in pieces, which matters on a
///   corpus with 430 MB pages.
/// - `accept` — what a client will take, and so whether serving anything
///   other than the stored bytes could help.
/// - `user-agent` — which client produced the session, since a log is worth
///   little if you cannot tell whose behaviour it recorded.
///
/// Lower-case, because HTTP header names are case-insensitive and `HeaderMap`
/// stores them lower-cased; a lookup for `If-None-Match` finds nothing.
pub const LOGGED_HEADERS: [&str; 5] = [
    "if-none-match",
    "if-modified-since",
    "range",
    "accept",
    "user-agent",
];

/// One request, as a line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub method: String,
    /// The full path *and* query string
    /// (`request.uri().path_and_query()`), not path alone. This server has
    /// no resize support, which makes a client's `?width=…` more important
    /// to see, not less — logging path alone would make it look like no
    /// client ever asks for one.
    pub path: String,
    pub status: u16,
    pub bytes: u64,
    pub micros: u64,
    /// Unix microseconds when the request arrived.
    ///
    /// Lines are appended in *completion* order, so adjacency in the file is
    /// not arrival order. This is what lets a re-request 200ms later (no
    /// caching) be told apart from one 20 minutes later (expiry), and lets
    /// page-turn cadence and burst boundaries be reconstructed at all —
    /// together with `micros` and `in_flight`, the whole in-flight timeline
    /// is recoverable offline.
    pub started_at: u64,
    /// Requests in flight at the instant this one arrived, counting itself.
    ///
    /// 1 means it arrived alone; the peak of a burst is `max` over the
    /// burst. In-flight only rises at arrivals, so a burst's peak is
    /// attained right after some arrival, and that arrival's own snapshot
    /// equals the peak exactly — nothing needs to be tracked separately to
    /// recover it.
    pub in_flight: u64,
    /// Those of `LOGGED_HEADERS` the request actually carried.
    ///
    /// Always emitted, empty object and all, so every line has the same
    /// shape and a `jq` expression does not have to guard on the key's
    /// existence as well as the header's.
    pub headers: BTreeMap<String, String>,
}

impl Entry {
    /// One JSON object on one line, so a summary is `jq` rather than a
    /// parser.
    ///
    /// Escaping is `serde_json`'s, not hand-rolled: nothing upstream
    /// promises a path or method never contains a quote, a backslash or a
    /// control character, and a log line that is not valid JSON defeats the
    /// one thing this format exists to guarantee.
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("Entry always serialises") + "\n"
    }
}

/// A request in progress: its start time, and its own share of the
/// in-flight count.
///
/// Holds the count rather than leaving `Log::finish` to release it, because
/// `finish` is not the only way a request stops being in flight — a client
/// cancelling a prefetch on a fast page turn, or the response future simply
/// being dropped, never runs it. `Drop` is the one place a departure is
/// guaranteed to be seen, so it is the one place that decrements, whether
/// the request finished or was abandoned.
#[derive(Debug)]
pub struct InFlight {
    started: Instant,
    started_at: u64,
    in_flight: u64,
    counter: Arc<AtomicU64>,
    /// Carried from arrival rather than collected at `finish`, because a
    /// request's headers are an arrival-time fact exactly like `started_at`
    /// and `in_flight` — and by the time the response comes back, the
    /// request they came from is gone.
    headers: BTreeMap<String, String>,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Where request lines are written.
///
/// O7: lines go to the file and nowhere else. Every entry used to be kept in
/// a `Vec` behind a mutex as well, for the life of the process — a server that
/// stays up for a reading session, on a log that exists precisely to be left
/// running. Nothing in production ever read it back: `entries()` was called
/// only from tests, and `scripts/request-summary.sh`, the actual consumer,
/// reads the JSONL on disk. So the growth was real and bought nothing. The
/// tests that read it now read the file, which has the second benefit of
/// meaning every test here exercises the branch a person's measurement
/// actually depends on.
#[derive(Debug)]
pub struct Log {
    in_flight: Arc<AtomicU64>,
    /// Opened once, at construction, rather than reopened on every write: a
    /// destination that cannot be opened is a startup failure the caller can
    /// act on, not a silently empty file discovered after an hour of reading
    /// on an iPad.
    file: Mutex<File>,
    /// Set after the first failed write, so a struggling disk earns one
    /// message on stderr rather than one per request.
    write_failed: AtomicBool,
}

impl Log {
    /// `path` is where lines are appended. Opens (or creates) the file
    /// immediately, so a bad path is a startup error the caller can act on
    /// rather than an hour of silently lost measurement.
    ///
    /// Not optional: a `Log` with nowhere to write would now record nothing
    /// anywhere, and `serve` already expresses "not logging" as no `Log` at
    /// all rather than an inert one.
    pub fn new(path: PathBuf) -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("--log-requests {}: {e}", path.display()))?;
        Ok(Self {
            in_flight: Arc::new(AtomicU64::new(0)),
            file: Mutex::new(file),
            write_failed: AtomicBool::new(false),
        })
    }

    /// A request arrived. The returned `InFlight` carries the count at this
    /// instant, including itself, and the headers it came with.
    ///
    /// `headers` is already filtered to `LOGGED_HEADERS` by the caller: this
    /// module takes `&str` for the method and the path rather than `http`
    /// types, and takes an extracted map here for the same reason — what a
    /// line contains is decided in one place, and it is not this one's job to
    /// know what an `http::HeaderMap` is.
    pub fn begin(&self, headers: BTreeMap<String, String>) -> InFlight {
        let in_flight = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        InFlight {
            started: Instant::now(),
            started_at: now_micros(),
            in_flight,
            counter: Arc::clone(&self.in_flight),
            headers,
        }
    }

    /// A request finished normally and is recorded.
    ///
    /// Never called for a cancelled request — there is nothing to record for
    /// one that never produced a response. `request`'s share of the
    /// in-flight count is released by `InFlight`'s `Drop`, once, when this
    /// function returns; that is the same path a cancellation takes, so
    /// there is exactly one decrement site rather than one here and another
    /// in `Drop`.
    pub fn finish(&self, request: InFlight, method: &str, path: &str, status: u16, bytes: u64) {
        let entry = Entry {
            method: method.to_string(),
            path: path.to_string(),
            status,
            bytes,
            micros: request.started.elapsed().as_micros() as u64,
            started_at: request.started_at,
            in_flight: request.in_flight,
            headers: request.headers.clone(),
        };
        let line = entry.to_json_line();
        let mut guard = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = guard.write_all(line.as_bytes())
            && self.first_write_failure()
        {
            eprintln!("taiyaku: request log write failed: {e}");
        }
    }

    /// True once, for the first write failure only.
    ///
    /// A full disk fails *every* subsequent write, and a line of stderr per
    /// request would bury whatever the operator was actually reading. Named
    /// rather than left inline as `!self.write_failed.swap(true, …)` because
    /// the `!` is the whole behaviour and, inline, deleting it was invisible
    /// to the suite — the difference between "warn once" and "warn on every
    /// failure after the first" is not something a test of the log's
    /// *contents* can see.
    fn first_write_failure(&self) -> bool {
        !self.write_failed.swap(true, Ordering::SeqCst)
    }
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The log has to record concurrency, not just order: a client fetching
    /// six pages at once and one fetching six in a row produce the same list
    /// of paths and mean entirely different things.
    ///
    /// Each entry gets its *own* arrival snapshot, not the burst's peak: the
    /// second request arrived while one other was in flight (2, counting
    /// itself), the first arrived alone (1). A constant function cannot
    /// fake this — the two entries must differ.
    #[test]
    fn a_log_records_what_was_in_flight_alongside_it() {
        let (log, path) = crate::test_support::log("taiyaku-test-reqlog-in-flight");

        let first = log.begin(BTreeMap::new());
        let second = log.begin(BTreeMap::new());
        log.finish(second, "GET", "/b", 200, 20);
        log.finish(first, "GET", "/a", 200, 10);

        let entries = crate::test_support::logged_lines(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["path"], "/b");
        assert_eq!(
            entries[0]["in_flight"], 2,
            "arrived while one other request was already in flight"
        );
        assert_eq!(entries[1]["path"], "/a");
        assert_eq!(entries[1]["in_flight"], 1, "arrived alone");
        assert_eq!(entries[1]["bytes"], 10);
    }

    /// Written as JSON lines so a summary script is `jq`, not a parser.
    ///
    /// Pins every key, not just two: a rename of any field is a break in
    /// `scripts/request-summary.sh`, and only asserting `bytes` and
    /// `in_flight` (say) would let `micros` or `started_at` drift unnoticed.
    #[test]
    fn an_entry_serialises_as_one_json_line() {
        let entry = Entry {
            method: "GET".into(),
            path: "/opds/v1.2/works/t/raw/pages/7".into(),
            status: 200,
            bytes: 1180617,
            micros: 4200,
            started_at: 1_753_900_000_000_000,
            in_flight: 3,
            headers: BTreeMap::from([("if-none-match".to_string(), "\"abc123\"".to_string())]),
        };

        let line = entry.to_json_line();

        assert!(line.ends_with('\n'), "one entry, one line");
        assert!(
            !line[..line.len() - 1].contains('\n'),
            "and no more: {line}"
        );

        let value: serde_json::Value =
            serde_json::from_str(line.trim_end()).expect("must parse as JSON");
        assert_eq!(value["method"], "GET");
        assert_eq!(value["path"], "/opds/v1.2/works/t/raw/pages/7");
        assert_eq!(value["status"], 200);
        assert_eq!(value["bytes"], 1_180_617);
        assert_eq!(value["micros"], 4200);
        assert_eq!(value["started_at"], 1_753_900_000_000_000_i64);
        assert_eq!(value["in_flight"], 3);
        assert_eq!(value["headers"]["if-none-match"], "\"abc123\"");
    }

    /// A quote is not the only character a hand-built JSON line has to
    /// survive: nothing upstream promises a path never carries a backslash
    /// or a control character, and `to_json_line` delegates to `serde_json`
    /// precisely so none of them can produce a line the summary script's
    /// `jq` calls choke on.
    #[test]
    fn to_json_line_survives_backslashes_and_control_characters_in_the_path() {
        let entry = Entry {
            method: "GET".into(),
            path: "/weird\\path\twith\na newline and a \"quote\"".into(),
            status: 200,
            bytes: 1,
            micros: 1,
            started_at: 1,
            in_flight: 1,
            headers: BTreeMap::new(),
        };

        let line = entry.to_json_line();

        let value: serde_json::Value =
            serde_json::from_str(line.trim_end()).expect("must still parse as JSON");
        assert_eq!(
            value["path"],
            "/weird\\path\twith\na newline and a \"quote\""
        );
    }

    /// `Log::new` opens (or creates) the destination immediately, rather
    /// than deferring to the first write, so a directory that does not
    /// exist is a startup error rather than a silently empty file after an
    /// hour of reading on an iPad.
    #[test]
    fn opening_the_log_fails_loudly_when_the_directory_does_not_exist() {
        let missing_dir =
            std::env::temp_dir().join("taiyaku-test-reqlog-missing-dir-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing_dir);
        let path = missing_dir.join("nested").join("req.jsonl");

        let err = Log::new(path).unwrap_err();

        assert!(!err.is_empty(), "the failure has to say something");
    }

    /// The file `Log` writes is the only artefact anything outside this crate
    /// consumes, and `scripts/request-summary.sh` reads it with `jq` — so this
    /// pins that every key that script names is present on every line.
    ///
    /// It used to carry a second job: every other test read `Log::entries()`
    /// instead, so deleting the whole file-writing branch would have left the
    /// suite green and this was the only thing standing in the way. O7 removed
    /// the in-memory copy, so that job is now shared by every test here, and
    /// what is left is the narrower and more useful claim about the keys.
    #[test]
    fn entries_written_to_disk_are_valid_json_lines_with_the_expected_keys() {
        let path = std::env::temp_dir().join("taiyaku-test-reqlog-disk.jsonl");
        let _ = std::fs::remove_file(&path);
        let log = Log::new(path.clone()).unwrap();

        let a = log.begin(BTreeMap::new());
        log.finish(a, "GET", "/api/works/t/raw/pages/2", 200, 1234);
        let b = log.begin(BTreeMap::new());
        log.finish(b, "GET", "/api/works/t/raw/pages/3?width=1536", 200, 4321);

        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "one line per finished request");
        for line in lines {
            let value: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("line must parse as JSON: {e}: {line}"));
            for key in [
                "method",
                "path",
                "status",
                "bytes",
                "micros",
                "started_at",
                "in_flight",
                "headers",
            ] {
                assert!(value.get(key).is_some(), "missing key `{key}` in {line}");
            }
        }
    }

    /// Panels cancelling in-flight prefetches on a fast page turn is exactly
    /// the traffic this log exists to measure, so it is the likely case, not
    /// the exotic one. `finish` is never reached for a cancelled request —
    /// `Drop` is what must release its share of the in-flight count, and it
    /// must do so without inventing an entry for a request that produced no
    /// response.
    #[test]
    fn dropping_an_in_flight_request_without_finishing_it_decrements_but_records_nothing() {
        let (log, path) = crate::test_support::log("taiyaku-test-reqlog-cancelled");

        let cancelled = log.begin(BTreeMap::new());
        assert_eq!(cancelled.in_flight, 1);
        drop(cancelled);

        let after = log.begin(BTreeMap::new());
        assert_eq!(
            after.in_flight, 1,
            "the cancelled request must not still be counted as in flight"
        );
        log.finish(after, "GET", "/a", 200, 1);

        let entries = crate::test_support::logged_lines(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            entries.len(),
            1,
            "the cancelled request must not have produced an entry"
        );
    }

    /// Warn once, not once per request. A full disk fails every write, and the
    /// `!` that makes this fire a single time was invisible to every test:
    /// they read the log's *contents*, and the difference is on stderr.
    #[test]
    fn only_the_first_write_failure_is_reported() {
        let log =
            Log::new(std::env::temp_dir().join("taiyaku-test-reqlog-once.jsonl")).expect("opens");

        assert!(log.first_write_failure(), "the first failure is reported");
        assert!(
            !log.first_write_failure(),
            "and no later one is, or a full disk buries the terminal"
        );
        assert!(!log.first_write_failure());

        let _ = std::fs::remove_file(std::env::temp_dir().join("taiyaku-test-reqlog-once.jsonl"));
    }

    /// The timestamp is what orders a session, and `now_micros` could return
    /// `0` or `1` for every entry with nothing noticing — every line would
    /// claim 1970 and no measurement drawn from the log would mean anything.
    #[test]
    fn the_clock_reads_a_real_time_that_does_not_go_backwards() {
        // 2020-01-01 in microseconds since the epoch. Any plausible reading is
        // far above it, and both `0` and `1` are far below.
        const Y2020_MICROS: u64 = 1_577_836_800_000_000;

        let first = now_micros();
        let second = now_micros();

        assert!(
            first > Y2020_MICROS,
            "a real wall clock, not 0 or 1: {first}"
        );
        assert!(second >= first, "and it does not go backwards");
    }
}
