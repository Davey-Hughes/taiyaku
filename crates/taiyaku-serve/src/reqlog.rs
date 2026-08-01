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

use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

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
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Where request lines accumulate.
#[derive(Debug)]
pub struct Log {
    entries: Mutex<Vec<Entry>>,
    in_flight: Arc<AtomicU64>,
    /// Opened once, at construction, rather than reopened on every write: a
    /// destination that cannot be opened is a startup failure the caller can
    /// act on, not a silently empty file discovered after an hour of reading
    /// on an iPad.
    file: Option<Mutex<File>>,
    /// Set after the first failed write, so a struggling disk earns one
    /// message on stderr rather than one per request.
    write_failed: AtomicBool,
}

impl Log {
    /// `path` is where lines are appended, or `None` to keep them in memory
    /// only. Opens (or creates) the file immediately, so a bad path is a
    /// startup error the caller can act on rather than an hour of silently
    /// lost measurement.
    pub fn new(path: Option<PathBuf>) -> Result<Self, String> {
        let file = match &path {
            Some(path) => Some(Mutex::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| format!("--log-requests {}: {e}", path.display()))?,
            )),
            None => None,
        };
        Ok(Self {
            entries: Mutex::new(Vec::new()),
            in_flight: Arc::new(AtomicU64::new(0)),
            file,
            write_failed: AtomicBool::new(false),
        })
    }

    /// A request arrived. The returned `InFlight` carries the count at this
    /// instant, including itself.
    pub fn begin(&self) -> InFlight {
        let in_flight = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        InFlight {
            started: Instant::now(),
            started_at: now_micros(),
            in_flight,
            counter: Arc::clone(&self.in_flight),
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
        };
        if let Some(file) = &self.file {
            let line = entry.to_json_line();
            let mut guard = file.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = guard.write_all(line.as_bytes())
                && !self.write_failed.swap(true, Ordering::SeqCst)
            {
                eprintln!("taiyaku: request log write failed: {e}");
            }
        }
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entry);
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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
        let log = Log::new(None).unwrap();

        let first = log.begin();
        let second = log.begin();
        log.finish(second, "GET", "/b", 200, 20);
        log.finish(first, "GET", "/a", 200, 10);

        let entries = log.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "/b");
        assert_eq!(
            entries[0].in_flight, 2,
            "arrived while one other request was already in flight"
        );
        assert_eq!(entries[1].path, "/a");
        assert_eq!(entries[1].in_flight, 1, "arrived alone");
        assert_eq!(entries[1].bytes, 10);
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

        let err = Log::new(Some(path)).unwrap_err();

        assert!(!err.is_empty(), "the failure has to say something");
    }

    /// The only artefact `scripts/request-summary.sh` actually consumes is
    /// the file `Log` writes — every other test here uses `Log::new(None)`,
    /// so deleting the whole file-writing branch would leave the suite
    /// green. Writes to a real path under `std::env::temp_dir()` (this
    /// crate's established pattern for on-disk tests; no socket, no sleep)
    /// and reads the file back, checking each line is valid JSON carrying
    /// every key the summary script reads.
    #[test]
    fn entries_written_to_disk_are_valid_json_lines_with_the_expected_keys() {
        let path = std::env::temp_dir().join("taiyaku-test-reqlog-disk.jsonl");
        let _ = std::fs::remove_file(&path);
        let log = Log::new(Some(path.clone())).unwrap();

        let a = log.begin();
        log.finish(a, "GET", "/api/works/t/raw/pages/2", 200, 1234);
        let b = log.begin();
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
        let log = Log::new(None).unwrap();

        let cancelled = log.begin();
        assert_eq!(cancelled.in_flight, 1);
        drop(cancelled);

        let after = log.begin();
        assert_eq!(
            after.in_flight, 1,
            "the cancelled request must not still be counted as in flight"
        );
        log.finish(after, "GET", "/a", 200, 1);

        assert_eq!(
            log.entries().len(),
            1,
            "the cancelled request must not have produced an entry"
        );
    }
}
