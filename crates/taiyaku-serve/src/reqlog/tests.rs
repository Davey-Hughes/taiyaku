use super::*;

/// Longer than any wait here should take, short of the test harness's kill.
const LONG: std::time::Duration = std::time::Duration::from_secs(10);

/// Finishes `count` requests, for `/0` onwards.
fn queue(log: &Log, count: usize) {
    for n in 0..count {
        let path = format!("/{n}");
        log.finish(log.begin(BTreeMap::new()), "GET", &path, 200, sent(1));
    }
}

/// [`queue`], on a thread of its own, for a test that holds the file. A line
/// written by its caller would wait on that file, so the test fails here with
/// a message rather than deadlock on its own lock.
fn queue_while_held(log: &Arc<Log>, count: usize) {
    let (queued, done) = std::sync::mpsc::channel();
    std::thread::spawn({
        let log = Arc::clone(log);
        move || {
            queue(&log, count);
            // Before the signal, so the test's own handle is the last when it
            // drops the log.
            drop(log);
            let _ = queued.send(());
        }
    });
    done.recv_timeout(LONG).expect(
        "queueing waited on the held file: a line was written by its caller, or waited for \
         room in the queue",
    );
}

/// The paths [`queue`] gives `count` requests, in order.
fn queued(count: usize) -> Vec<String> {
    (0..count).map(|n| format!("/{n}")).collect()
}

/// The paths of the lines in the file at `path`, read as it is, with no flush.
fn paths(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            value["path"].as_str().unwrap().to_string()
        })
        .collect()
}

/// [`paths`], once the file holds `count` lines or [`LONG`] has passed.
fn on_disk_within(path: &std::path::Path, count: usize) -> Vec<String> {
    let deadline = Instant::now() + LONG;
    loop {
        let written = paths(path);
        if written.len() >= count || Instant::now() >= deadline {
            return written;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A body read to its end, `bytes` long, for a test of the line rather than
/// of the counting body.
fn sent(bytes: u64) -> Sent {
    Sent {
        bytes,
        outcome: Outcome::Complete,
        micros: 0,
    }
}

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
    let tmp = crate::test_support::Fixture::new("reqlog-in-flight");
    let (log, path) = crate::test_support::log(&tmp);

    let first = log.begin(BTreeMap::new());
    let second = log.begin(BTreeMap::new());
    log.finish(second, "GET", "/b", 200, sent(20));
    log.finish(first, "GET", "/a", 200, sent(10));

    let entries = crate::test_support::logged_lines(&log, &path);
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
/// Pins every key, not just two: a rename of a field the script reads is a
/// break in `scripts/request-summary.sh`, and only asserting `bytes` and
/// `in_flight` (say) would let `micros` or `started_at` drift unnoticed.
#[test]
fn an_entry_serialises_as_one_json_line() {
    let entry = Entry {
        method: "GET".into(),
        path: "/opds/v1.2/works/t/raw/pages/7".into(),
        status: 200,
        bytes: 1_180_617,
        outcome: Outcome::Cut,
        micros: 4200,
        transfer_micros: 9100,
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
    assert_eq!(value["outcome"], "cut");
    assert_eq!(value["micros"], 4200);
    assert_eq!(value["transfer_micros"], 9100);
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
        outcome: Outcome::Complete,
        micros: 1,
        transfer_micros: 1,
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
    let scratch = crate::test_support::Fixture::new("reqlog-missing-dir");
    let missing_dir = scratch.join("does-not-exist");
    let path = missing_dir.join("nested").join("req.jsonl");

    let err = Log::new(&path).unwrap_err();

    assert!(!err.is_empty(), "the failure has to say something");
}

/// The file `Log` writes is the only artefact anything outside this crate
/// consumes, and `scripts/request-summary.sh` reads it with `jq` — so this
/// pins that every key that script names is present on every line.
#[test]
fn entries_written_to_disk_are_valid_json_lines_with_the_expected_keys() {
    let dir = crate::test_support::Fixture::new("reqlog-disk");
    let path = dir.join("reqlog-disk.jsonl");
    let log = Log::new(&path).unwrap();

    let a = log.begin(BTreeMap::new());
    log.finish(a, "GET", "/api/works/t/raw/pages/2", 200, sent(1234));
    let b = log.begin(BTreeMap::new());
    log.finish(
        b,
        "GET",
        "/api/works/t/raw/pages/3?width=1536",
        200,
        sent(4321),
    );
    log.flush();

    let contents = std::fs::read_to_string(&path).unwrap();

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
            "outcome",
            "micros",
            "transfer_micros",
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
    let tmp = crate::test_support::Fixture::new("reqlog-cancelled");
    let (log, path) = crate::test_support::log(&tmp);

    let cancelled = log.begin(BTreeMap::new());
    assert_eq!(cancelled.share, 1);
    drop(cancelled);

    let after = log.begin(BTreeMap::new());
    assert_eq!(
        after.share, 1,
        "the cancelled request must not still be counted as in flight"
    );
    log.finish(after, "GET", "/a", 200, sent(1));

    let entries = crate::test_support::logged_lines(&log, &path);
    assert_eq!(
        entries.len(),
        1,
        "the cancelled request must not have produced an entry"
    );
}

/// Warn once, not once per request. A full disk fails every write, and the
/// `!` that makes this fire a single time makes a difference on stderr, not
/// in the log's *contents*.
#[test]
fn only_the_first_write_failure_is_reported() {
    let dir = crate::test_support::Fixture::new("reqlog-once");
    let log = Log::new(&dir.join("reqlog-once.jsonl")).expect("opens");

    assert!(
        log.out.first_write_failure(),
        "the first failure is reported"
    );
    assert!(
        !log.out.first_write_failure(),
        "and no later one is, or a full disk buries the terminal"
    );
    assert!(!log.out.first_write_failure());
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

/// O123: a stop loses no line. `close` does not return while a line is
/// queued, leaves every queued line on disk in the order it was finished,
/// and a line finished after it still lands, which is where a body dropped
/// as the runtime shuts down ends up.
///
/// The test holds the file's lock while it queues and while `close` runs, so
/// the writer cannot have written a line yet, and a `close` that returns
/// without waiting for it fails here every time.
#[test]
fn closing_the_log_writes_every_queued_line_and_a_later_one_still_lands() {
    const QUEUED: usize = 1000;
    let tmp = crate::test_support::Fixture::new("reqlog-close");
    let (log, path) = crate::test_support::log(&tmp);

    let held = log.out.lock();
    queue_while_held(&log, QUEUED);
    let (closed, returned) = std::sync::mpsc::channel();
    let closing = std::thread::spawn({
        let log = Arc::clone(&log);
        move || {
            let drained = log.close_within(LONG);
            let _ = closed.send(());
            drained
        }
    });
    assert!(
        returned
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_err(),
        "close returned while every line was still waiting to be written"
    );
    drop(held);
    assert!(closing.join().unwrap(), "the writer drained the queue");

    assert_eq!(paths(&path), queued(QUEUED), "every queued line, in order");

    log.finish(log.begin(BTreeMap::new()), "GET", "/after", 200, sent(1));
    let written = paths(&path);
    assert_eq!(
        written.len(),
        QUEUED + 1,
        "a line finished after close lands"
    );
    assert_eq!(written.last().map(String::as_str), Some("/after"));
}

/// O123: a line finished while `close` drains lands after every line queued
/// before it, because `close` keeps the queue until the writer has drained
/// it. The test holds the file, so the writer cannot drain until it lets go.
#[test]
fn a_line_finished_while_close_drains_lands_after_every_queued_one() {
    const QUEUED: usize = 100;
    let tmp = crate::test_support::Fixture::new("reqlog-close-order");
    let (log, path) = crate::test_support::log(&tmp);

    let held = log.out.lock();
    queue_while_held(&log, QUEUED);
    let closing = std::thread::spawn({
        let log = Arc::clone(&log);
        move || log.close_within(LONG)
    });
    let deadline = Instant::now() + LONG;
    while log.writer.try_lock().is_ok() {
        assert!(Instant::now() < deadline, "close never took the queue");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        matches!(
            log.writer.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ),
        "close let go of the queue before the writer drained it"
    );
    let late = std::thread::spawn({
        let log = Arc::clone(&log);
        move || log.finish(log.begin(BTreeMap::new()), "GET", "/late", 200, sent(1))
    });
    drop(held);
    assert!(closing.join().unwrap(), "the writer drained the queue");
    late.join().unwrap();

    let mut expected = queued(QUEUED);
    expected.push("/late".into());
    assert_eq!(paths(&path), expected);
}

/// O123: a `Log` that goes away without `close` still writes every queued
/// line before its drop returns.
#[test]
fn dropping_the_log_writes_every_queued_line_before_it_returns() {
    const QUEUED: usize = 100;
    let tmp = crate::test_support::Fixture::new("reqlog-drop");
    let (log, path) = crate::test_support::log(&tmp);

    let out = Arc::clone(&log.out);
    let held = out.lock();
    queue_while_held(&log, QUEUED);
    let (dropped, returned) = std::sync::mpsc::channel();
    let dropping = std::thread::spawn(move || {
        drop(log);
        let _ = dropped.send(());
    });
    assert!(
        returned
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err(),
        "the drop returned while every line was still waiting to be written"
    );
    drop(held);
    dropping.join().unwrap();
    assert_eq!(on_disk_within(&path, QUEUED), queued(QUEUED));
}

/// O123: a quiet server's line reaches the disk with nothing asking for it,
/// as a person tailing the log during a session expects.
#[test]
fn a_finished_line_reaches_the_disk_by_itself() {
    let tmp = crate::test_support::Fixture::new("reqlog-by-itself");
    let (log, path) = crate::test_support::log(&tmp);

    log.finish(log.begin(BTreeMap::new()), "GET", "/0", 200, sent(1));

    assert_eq!(on_disk_within(&path, 1), queued(1));
}

/// O123: a `close` whose writer cannot write gives up at its limit and says
/// so, rather than holding a stop for as long as the disk hangs; a line
/// finished then is dropped rather than wait on the file; the writer still
/// writes what it held once the disk answers; and once it has returned, a
/// line finished later lands again.
#[test]
fn a_close_whose_writer_is_stuck_gives_up_at_its_limit() {
    let tmp = crate::test_support::Fixture::new("reqlog-stuck");
    let (log, path) = crate::test_support::log(&tmp);

    let out = Arc::clone(&log.out);
    let held = out.lock();
    queue_while_held(&log, 1);
    let (closed, returned) = std::sync::mpsc::channel();
    let closing = std::thread::spawn({
        let log = Arc::clone(&log);
        move || {
            let drained = log.close_within(std::time::Duration::from_millis(50));
            let _ = closed.send(drained);
        }
    });
    let drained = returned
        .recv_timeout(LONG)
        .expect("close waited past its limit for a writer that could not write");
    assert!(!drained, "and says the writer did not drain");
    let (finished, finishing) = std::sync::mpsc::channel();
    let late = std::thread::spawn({
        let log = Arc::clone(&log);
        move || {
            log.finish(log.begin(BTreeMap::new()), "GET", "/late", 200, sent(1));
            let _ = finished.send(());
        }
    });
    let waited = finishing.recv_timeout(LONG).is_err();
    drop(held);
    closing.join().unwrap();
    late.join().unwrap();
    assert!(
        !waited,
        "a line finished after the close waited on the file"
    );
    assert_eq!(on_disk_within(&path, 1), queued(1));

    let deadline = Instant::now() + LONG;
    while let Some(Err(std::sync::mpsc::TryRecvError::Empty)) = log
        .abandoned()
        .as_ref()
        .map(std::sync::mpsc::Receiver::try_recv)
    {
        assert!(Instant::now() < deadline, "the writer never returned");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    log.finish(log.begin(BTreeMap::new()), "GET", "/later", 200, sent(1));
    assert_eq!(
        paths(&path),
        ["/0", "/later"],
        "a line finished once the writer had returned was not written"
    );
}

/// O123 and O7: while the disk hangs, at most [`QUEUE_LINES`] finished lines
/// wait in memory. One finished while the queue is full is dropped and
/// counted, without its caller waiting, and the writer takes the count to say
/// on stderr once it has caught up. The test holds the file, so the writer can
/// take at most one line before it waits on it.
#[test]
fn a_line_finished_while_the_queue_is_full_is_dropped_and_counted() {
    const OVER: usize = 10;
    let tmp = crate::test_support::Fixture::new("reqlog-full");
    let (log, path) = crate::test_support::log(&tmp);

    let held = log.out.lock();
    queue_while_held(&log, QUEUE_LINES + OVER);
    let lost = usize::try_from(log.out.lost.load(Ordering::SeqCst)).unwrap();
    drop(held);

    assert!(
        lost >= OVER - 1,
        "{lost} lines dropped: more than {QUEUE_LINES} waited for a disk that could not write"
    );
    let kept = QUEUE_LINES + OVER - lost;
    let written = on_disk_within(&path, kept);
    assert_eq!(written.len(), kept, "every line not dropped is written");
    assert_eq!(
        written[..QUEUE_LINES],
        queued(QUEUE_LINES),
        "the lines kept are the first, in order"
    );
    let deadline = Instant::now() + LONG;
    while log.out.lost.load(Ordering::SeqCst) != 0 {
        assert!(
            Instant::now() < deadline,
            "the writer caught up and did not say how many lines were lost"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// O123: the writer's report of lines a full queue dropped gives their count,
/// once, and says nothing when none were dropped.
#[test]
fn the_loss_report_gives_the_count_once_and_nothing_when_no_line_was_lost() {
    let tmp = crate::test_support::Fixture::new("reqlog-say-lost");
    let (log, _path) = crate::test_support::log(&tmp);
    let said = |log: &Log| {
        let mut stderr = Vec::new();
        log.out.say_lost(&mut stderr);
        String::from_utf8(stderr).unwrap()
    };

    assert_eq!(said(&log), "", "reported lost lines when none were lost");
    log.out.lost.store(7, Ordering::SeqCst);
    assert_eq!(
        said(&log),
        "taiyaku: the request log fell behind its disk and lost 7 lines\n"
    );
    assert_eq!(said(&log), "", "reported the same lost lines twice");
}

/// O123: a logged request's line is written by no runtime worker. The test
/// runs itself again as a child under `strace`, as O120's does: the child
/// serves one page through the logging router on a current-thread runtime,
/// whose async worker is the thread calling `block_on`, and writes that
/// thread's id down. The parent requires that something wrote the log file
/// and that the async worker never did.
#[cfg(target_os = "linux")]
#[test]
fn a_logged_request_is_not_written_by_the_async_worker() {
    const CHILD: &str = "TAIYAKU_TEST_REQLOG_O123_CHILD";
    if let Some(dir) = std::env::var_os(CHILD) {
        serve_one_logged_page(std::path::Path::new(&dir));
        return;
    }

    let scratch = crate::test_support::Fixture::new("reqlog-o123-write");
    // Canonical, because strace prints the descriptor's path that way.
    let dir = std::fs::canonicalize(&scratch).unwrap();
    let trace = taiyaku_read::test_support::strace(
        "reqlog::tests::a_logged_request_is_not_written_by_the_async_worker",
        (CHILD, dir.as_os_str()),
        "write,writev,pwrite64,pwritev",
        &dir.join("strace.log"),
    );
    let threads = std::fs::read_to_string(dir.join("threads")).expect("the child wrote its thread");
    let async_worker = threads
        .lines()
        .find_map(|line| line.strip_prefix("async "))
        .expect("the child named its async worker");
    let target = format!("<{}>", dir.join("requests.jsonl").display());
    let by: Vec<&str> = trace
        .lines()
        .filter_map(|line| {
            let (thread, call) = line.split_once(' ')?;
            let descriptor = call.trim_start().split(',').next()?;
            descriptor.ends_with(&target).then_some(thread)
        })
        .collect();

    assert!(
        !by.is_empty(),
        "nothing wrote the log, so which thread did proves nothing:\n{trace}"
    );
    assert!(
        !by.contains(&async_worker),
        "the async worker {async_worker} wrote the log; the writers were {by:?}"
    );
}

/// The child half of [`a_logged_request_is_not_written_by_the_async_worker`].
#[cfg(target_os = "linux")]
fn serve_one_logged_page(dir: &std::path::Path) {
    use tower::ServiceExt as _;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime");
    let (log, path) = crate::test_support::log(dir);
    let mut state = crate::test_support::state(dir);
    state.log = Some(Arc::clone(&log));
    let state = Arc::new(state);
    let app =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let async_worker = crate::test_support::this_thread();
    runtime.block_on(async {
        let page = app
            .oneshot(
                axum::http::Request::get("/api/works/t/raw/pages/2")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), axum::http::StatusCode::OK);
        axum::body::to_bytes(page.into_body(), 1 << 20)
            .await
            .unwrap();
    });
    assert_eq!(
        crate::test_support::logged_lines(&log, &path).len(),
        1,
        "the page was logged"
    );
    std::fs::write(dir.join("threads"), format!("async {async_worker}\n")).unwrap();
}
