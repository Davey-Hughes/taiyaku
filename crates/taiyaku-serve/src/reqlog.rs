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
//! writing down what the app gets right.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
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
/// stores them lower-cased.
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
    /// (`request.uri().path_and_query()`), not path alone. Logging path alone
    /// would make it look like no client ever asks for a `?width=…`.
    pub path: String,
    pub status: u16,
    /// Body bytes handed to hyper, which is not what the client received:
    /// hyper drops the body once its last bytes are in its write buffer,
    /// several hundred KiB, and the kernel's socket buffers hold more. A
    /// download smaller than those can read `complete` though its client
    /// left, and a `cut` line's count is at least what arrived (O68).
    pub bytes: u64,
    /// How the body ended. See [`Outcome`].
    pub outcome: Outcome,
    /// Microseconds from arrival to the response head.
    pub micros: u64,
    /// Microseconds from arrival to the line: the body's end, or the moment
    /// it was dropped.
    pub transfer_micros: u64,
    /// Unix microseconds when the request arrived.
    ///
    /// Lines are appended in the order bodies end, so adjacency in the file is
    /// not arrival order. This is what lets a re-request 200ms later (no
    /// caching) be told apart from one 20 minutes later (expiry), and lets
    /// page-turn cadence and burst boundaries be reconstructed at all —
    /// together with `transfer_micros` and `in_flight`, the whole in-flight
    /// timeline is recoverable offline.
    pub started_at: u64,
    /// Requests in flight at the instant this one arrived, counting itself. A
    /// request is in flight from its arrival until its body ends or is dropped.
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

/// How a response body ended, which is what tells a finished download from
/// an abandoned one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Every byte it promised was handed over, or it promised none and
    /// reached its end. A HEAD, a 204 and a 304 send no body, and are this.
    Complete,
    /// Dropped before its end: the client went away, or the server stopped.
    Cut,
    /// Ended cleanly with fewer bytes than its `Content-Length`, as a
    /// download does when its producer gives up without its error landing.
    Short,
    /// The body yielded an error, and the response stopped there.
    Error,
}

/// What a body sent, as the counting body saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sent {
    pub bytes: u64,
    pub outcome: Outcome,
    /// [`Entry::micros`], read when the head was ready.
    pub micros: u64,
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
/// that cancels a prefetch before its response is ready drops the response
/// future, which never runs it. `Drop` is the one place a departure is
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

impl InFlight {
    /// Microseconds since the request arrived, saturating.
    #[must_use]
    pub fn micros(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Where request lines are written.
///
/// O7: lines go to the file and nowhere else. An in-memory copy of every
/// entry would grow for the life of the process, on a log that exists
/// precisely to be left running, and the actual consumer,
/// `scripts/request-summary.sh`, reads the JSONL on disk. The tests that look
/// at what was logged read that file too, so they exercise the branch a
/// person's measurement actually depends on.
#[derive(Debug)]
pub struct Log {
    in_flight: Arc<AtomicU64>,
    /// Shared with the writer thread, and written here directly once that
    /// thread is gone, so a line finished after [`Log::close`] still lands.
    out: Arc<Out>,
    /// `None` once [`Log::close`] has run, whether or not the queue drained.
    writer: Mutex<Option<Writer>>,
    /// Set by a [`Log::close`] that gave up: the thread it left, which may be
    /// stuck on a disk that hangs, disconnects this as it returns.
    abandoned: Mutex<Option<mpsc::Receiver<()>>>,
}

/// The thread that writes the lines, and the queue that feeds it. O123: a line
/// is written here, never by the async worker that served the request.
///
/// **A `std::thread`, not `spawn_blocking`.** Blocking-pool tasks run in no
/// set order, so lines would land out of the order they were finished in,
/// and tokio shuts down a blocking task that has not started rather than
/// running it, so the lines queued at a stop would be lost. One thread and one
/// channel keep the order and let [`Log::close`] drain the queue.
#[derive(Debug)]
struct Writer {
    /// Holds at most [`QUEUE_LINES`].
    lines: mpsc::SyncSender<Message>,
    /// Disconnects as the thread returns or unwinds, which [`Log::close`] can
    /// wait on with a deadline, as it cannot on a join.
    gone: mpsc::Receiver<()>,
}

/// How many finished lines may wait for the writer. A line finished while the
/// queue is full is dropped and counted rather than held, so a disk that hangs
/// while the server runs costs lines, not memory that grows for as long as it
/// hangs (O7), and no worker waits on it. Large enough that a hang of minutes
/// at the request rates a measurement session sees loses nothing (`notes.md`,
/// "O68 and O123").
const QUEUE_LINES: usize = 4096;

#[derive(Debug)]
enum Message {
    Line(String),
    /// Answered once every line sent before it is on disk.
    #[cfg(test)]
    Flush(mpsc::Sender<()>),
}

/// The destination file, its warn-once flag, and the lines a full queue
/// dropped.
#[derive(Debug)]
struct Out {
    /// Opened once, at construction, rather than reopened on every write: a
    /// destination that cannot be opened is a startup failure the caller can
    /// act on, not a silently empty file discovered after an hour of reading
    /// on an iPad.
    file: Mutex<BufWriter<File>>,
    /// Set after the first failed write, so a struggling disk earns one
    /// message on stderr rather than one per request.
    write_failed: AtomicBool,
    /// Lines dropped because the queue was full, since the writer last said
    /// how many: once each time it catches up, not once a line.
    lost: AtomicU64,
}

impl Out {
    fn lock(&self) -> MutexGuard<'_, BufWriter<File>> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, file: &mut BufWriter<File>, line: &str) {
        if let Err(e) = file.write_all(line.as_bytes()) {
            self.failed(&e);
        }
    }

    fn flush(&self, file: &mut BufWriter<File>) {
        if let Err(e) = file.flush() {
            self.failed(&e);
        }
    }

    /// `writeln!` and not `eprintln!`, which panics when stderr is closed: a
    /// panic here would end the writer thread.
    fn failed(&self, e: &std::io::Error) {
        if self.first_write_failure() {
            let _ = writeln!(std::io::stderr(), "taiyaku: request log write failed: {e}");
        }
    }

    /// Says on `stderr` how many lines a full queue dropped since it last
    /// said, if any. `writeln!` for the reason [`Out::failed`] gives. Takes
    /// the stream so a test can read what it says.
    fn say_lost(&self, stderr: &mut impl std::io::Write) {
        let lost = self.lost.swap(0, Ordering::SeqCst);
        if lost > 0 {
            let _ = writeln!(
                stderr,
                "taiyaku: the request log fell behind its disk and lost {lost} lines"
            );
        }
    }

    /// True once, for the first write failure only.
    ///
    /// A full disk fails *every* subsequent write, and a line of stderr per
    /// request would bury whatever the operator was actually reading. Named
    /// rather than left inline as `!self.write_failed.swap(true, …)` because
    /// the `!` is the whole behaviour, and the difference between "warn once"
    /// and "warn on every failure after the first" is not something a test of
    /// the log's *contents* can see.
    fn first_write_failure(&self) -> bool {
        !self.write_failed.swap(true, Ordering::SeqCst)
    }
}

/// The writer thread's loop: everything queued is written, and the file is
/// flushed whenever the queue runs dry, so a quiet server's lines reach the
/// disk one by one and a busy one's go in batches; then it says how many lines
/// a full queue dropped meanwhile. A kill loses what was still queued and what
/// sat unflushed. Returns once every sender is gone and the queue is empty.
fn write_lines(queue: &mpsc::Receiver<Message>, out: &Out) {
    while let Ok(first) = queue.recv() {
        let mut file = out.lock();
        let mut next = Some(first);
        while let Some(message) = next {
            match message {
                Message::Line(line) => out.write(&mut file, &line),
                #[cfg(test)]
                Message::Flush(done) => {
                    out.flush(&mut file);
                    let _ = done.send(());
                }
            }
            next = queue.try_recv().ok();
        }
        out.flush(&mut file);
        drop(file);
        out.say_lost(&mut std::io::stderr());
    }
}

impl Log {
    /// `path` is where lines are appended. Opens (or creates) the file
    /// immediately, so a bad path is a startup error the caller can act on
    /// rather than an hour of silently lost measurement.
    ///
    /// Not optional: a `Log` with nowhere to write would record nothing
    /// anywhere, and `serve` already expresses "not logging" as no `Log` at
    /// all rather than an inert one.
    ///
    /// # Errors
    ///
    /// Returns an error, worded `--log-requests {path}: {e}`, if `path`
    /// cannot be opened for appending and cannot be created — most often
    /// because a parent directory does not exist, or because the process
    /// lacks permission to write there — or if the thread that writes the
    /// lines cannot be started.
    pub fn new(path: &Path) -> Result<Self, String> {
        let named = |e: std::io::Error| format!("--log-requests {}: {e}", path.display());
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(named)?;
        let out = Arc::new(Out {
            file: Mutex::new(BufWriter::new(file)),
            write_failed: AtomicBool::new(false),
            lost: AtomicU64::new(0),
        });
        let (lines, queue) = mpsc::sync_channel(QUEUE_LINES);
        let (going, gone) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("taiyaku-reqlog".into())
            .spawn({
                let out = Arc::clone(&out);
                move || {
                    let _going = going;
                    write_lines(&queue, &out);
                }
            })
            .map_err(named)?;
        Ok(Self {
            in_flight: Arc::new(AtomicU64::new(0)),
            out,
            abandoned: Mutex::new(None),
            writer: Mutex::new(Some(Writer { lines, gone })),
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

    /// A request's body ended, or was dropped, and the request is recorded.
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
    /// its `Drop` runs here, once the line is handed to the writer, which is
    /// what keeps the decrement synchronised with the line rather than merely
    /// eventual.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "InFlight is consumed for its Drop timing, not its fields (see doc comment \
                  above): taking it by value is what makes the in-flight counter release \
                  exactly when this function returns, rather than whenever the caller's local \
                  variable happens to go out of scope. Tried a &InFlight signature first; it \
                  compiles, but silently detaches the decrement from the line it is meant to \
                  be synchronised with, which is the defect Drop was written to prevent."
    )]
    pub fn finish(&self, request: InFlight, method: &str, path: &str, status: u16, sent: Sent) {
        let entry = Entry {
            method: method.to_string(),
            path: path.to_string(),
            status,
            bytes: sent.bytes,
            outcome: sent.outcome,
            micros: sent.micros,
            transfer_micros: request.micros(),
            started_at: request.started_at,
            in_flight: request.share,
            headers: request.headers.clone(),
        };
        self.write(entry.to_json_line());
    }

    /// Hands `line` to the writer thread, or writes it here: after
    /// [`Log::close`], which is where the bodies dropped as the runtime shuts
    /// down end up, or if the thread has died. A line finished while the queue
    /// is full is dropped and counted, and the writer says how many once it
    /// catches up. While a thread that a close gave up on still runs, the line
    /// is dropped, so no worker waits behind it on a disk that hangs; the close
    /// said on stderr that lines may be lost.
    fn write(&self, line: String) {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let line = match writer.as_ref() {
            Some(writer) => match writer.lines.try_send(Message::Line(line)) {
                Ok(()) => return,
                Err(mpsc::TrySendError::Full(_)) => {
                    self.out.lost.fetch_add(1, Ordering::SeqCst);
                    return;
                }
                Err(mpsc::TrySendError::Disconnected(Message::Line(line))) => line,
                #[cfg(test)]
                Err(mpsc::TrySendError::Disconnected(Message::Flush(_))) => return,
            },
            None => line,
        };
        drop(writer);
        if self.abandoned_thread_runs() {
            return;
        }
        let mut file = self.out.lock();
        self.out.write(&mut file, &line);
        self.out.flush(&mut file);
    }

    /// Writes every line already finished, flushes, and stops the writer
    /// thread, waiting at most [`crate::REQUEST_LOG_DRAIN`]; a line finished
    /// afterwards is written by its caller. `serve` calls this as it returns,
    /// and `Drop` calls it for every other way a `Log` goes away.
    ///
    /// Holds the queue's lock until the thread has drained, so a line
    /// finished meanwhile waits and lands after the queued ones, in order.
    pub fn close(&self) {
        self.close_within(crate::REQUEST_LOG_DRAIN);
    }

    /// [`Log::close`], waiting at most `limit`. A disk that hangs costs the
    /// lines still queued, not a stop that never ends. True if the thread
    /// drained the queue.
    pub(crate) fn close_within(&self, limit: std::time::Duration) -> bool {
        let mut writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(Writer { lines, gone }) = writer.take() else {
            return true;
        };
        drop(lines);
        if let Err(mpsc::RecvTimeoutError::Timeout) = gone.recv_timeout(limit) {
            *self.abandoned() = Some(gone);
            let _ = writeln!(
                std::io::stderr(),
                "taiyaku: the request log was still writing after {limit:?}; the lines not yet \
                 written, and those finished until it is done, may be lost"
            );
            return false;
        }
        true
    }

    fn abandoned(&self) -> MutexGuard<'_, Option<mpsc::Receiver<()>>> {
        self.abandoned
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// True while the thread a [`Log::close`] gave up on has not returned.
    fn abandoned_thread_runs(&self) -> bool {
        let mut abandoned = self.abandoned();
        match abandoned.as_ref().map(mpsc::Receiver::try_recv) {
            None => false,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                *abandoned = None;
                false
            }
            Some(_) => true,
        }
    }

    /// The file, held, so the writer can write nothing until the guard goes.
    #[cfg(test)]
    pub(crate) fn hold_file(&self) -> MutexGuard<'_, BufWriter<File>> {
        self.out.lock()
    }

    /// Returns once every line finished before this call is on disk, so a
    /// test can read the file.
    #[cfg(test)]
    pub(crate) fn flush(&self) {
        let (done, flushed) = mpsc::channel();
        {
            let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(writer) = writer.as_ref() else {
                return;
            };
            if writer.lines.send(Message::Flush(done)).is_err() {
                return;
            }
        }
        let _ = flushed.recv();
    }
}

impl Drop for Log {
    fn drop(&mut self) {
        self.close();
    }
}

/// Microseconds since `UNIX_EPOCH`, narrowed from `Duration::as_micros`'s `u128`.
///
/// Saturates rather than panics, matching `InFlight::micros`'s narrowing of
/// `Instant::elapsed()`'s `as_micros()` above
/// (`u64::try_from(..).unwrap_or(u64::MAX)`) — deliberately, even though the
/// two narrowings look like they should differ. `elapsed()` is measured against
/// a monotonic clock nothing outside the process can move, so overflowing `u64`
/// microseconds there needs 584,000 years of uptime. Wall time carries no such
/// guarantee: an operator's clock can be set to anything, by a fat-fingered
/// `date`, a stuck RTC, or a misbehaving NTP source, and this function runs on
/// every request's arrival (`now_micros`, called from `begin`), not once at
/// startup. A clock misconfigured far enough into the future to overflow this
/// narrowing must not take the request-handling path down with it on every
/// subsequent request; a saturated, nonsensical `started_at` is a far better
/// failure than a panicking request log.
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
