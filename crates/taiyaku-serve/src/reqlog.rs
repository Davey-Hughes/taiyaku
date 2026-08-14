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
use std::path::Path;
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
    ///
    /// # Panics
    ///
    /// Panics if serialising `self` to JSON fails. Every field of `Entry` is
    /// a `String`, a `u16`, a `u64`, or a `BTreeMap<String, String>`, none of
    /// which `serde_json` can fail to encode, so this is not reachable from
    /// any `Entry` this crate can construct; the `expect` exists because
    /// `Serialize::serialize` is fallible in general, not because failure is
    /// possible here.
    #[must_use]
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
    /// This request's own share of the in-flight count, captured at arrival —
    /// see the struct doc above.
    share: u64,
    counter: Arc<AtomicU64>,
    /// Carried from arrival rather than collected at `finish`, because a
    /// request's headers are an arrival-time fact exactly like `started_at`
    /// and `share` — and by the time the response comes back, the request
    /// they came from is gone.
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
    ///
    /// # Errors
    ///
    /// Returns an error, worded `--log-requests {path}: {e}`, if `path`
    /// cannot be opened for appending and cannot be created — most often
    /// because a parent directory does not exist, or because the process
    /// lacks permission to write there.
    pub fn new(path: &Path) -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
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
        let share = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        InFlight {
            started: Instant::now(),
            started_at: now_micros(),
            share,
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
    ///
    /// Takes `request` by value rather than `&InFlight`, though every field
    /// below is read by copy or clone and none is moved out: ownership is the
    /// mechanism that makes the doc comment above true. A reference would let
    /// `InFlight::drop` — and so the in-flight decrement — happen whenever the
    /// caller's own binding later goes out of scope, not at the point this
    /// function returns. Taking `request` by value moves it into this call so
    /// its `Drop` runs here, at the moment the response is actually recorded,
    /// which is what keeps the decrement synchronised with the write below
    /// rather than merely eventual.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "InFlight is consumed for its Drop timing, not its fields (see doc comment \
                  above): taking it by value is what makes the in-flight counter release \
                  exactly when this function returns, rather than whenever the caller's local \
                  variable happens to go out of scope. Tried a &InFlight signature first; it \
                  compiles, but silently detaches the decrement from the write it is meant to \
                  be synchronised with, which is the defect Drop was written to prevent."
    )]
    pub fn finish(&self, request: InFlight, method: &str, path: &str, status: u16, bytes: u64) {
        let entry = Entry {
            method: method.to_string(),
            path: path.to_string(),
            status,
            bytes,
            micros: u64::try_from(request.started.elapsed().as_micros()).unwrap_or(u64::MAX),
            started_at: request.started_at,
            in_flight: request.share,
            headers: request.headers.clone(),
        };
        let line = entry.to_json_line();
        let mut guard = self
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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

/// Microseconds since `UNIX_EPOCH`, narrowed from `Duration::as_micros`'s `u128`.
///
/// Saturates rather than panics, matching `finish`'s narrowing of
/// `Instant::elapsed()`'s `as_micros()` a few lines above
/// (`u64::try_from(..).unwrap_or(u64::MAX)`) — deliberately, even though the
/// two narrowings look like they should differ. `elapsed()` is measured
/// against a monotonic clock nothing outside the process can move, so
/// overflowing `u64` microseconds there needs 584,000 years of uptime. Wall
/// time carries no such guarantee: an operator's clock can be set to
/// anything, by a fat-fingered `date`, a stuck RTC, or a misbehaving NTP
/// source, and this function runs on every request's arrival
/// (`now_micros`, called from `begin`), not once at startup. A clock
/// misconfigured far enough into the future to overflow this narrowing must
/// not take the request-handling path down with it on every subsequent
/// request; a saturated, nonsensical `started_at` is a far better failure
/// than a panicking request log.
fn wall_clock_micros(d: u128) -> u64 {
    u64::try_from(d).unwrap_or(u64::MAX)
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| wall_clock_micros(d.as_micros()))
}

#[cfg(test)]
mod tests;
