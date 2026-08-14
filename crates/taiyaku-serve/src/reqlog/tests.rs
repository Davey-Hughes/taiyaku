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
        bytes: 1_180_617,
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
    let missing_dir = std::env::temp_dir().join("taiyaku-test-reqlog-missing-dir-does-not-exist");
    let _ = std::fs::remove_dir_all(&missing_dir);
    let path = missing_dir.join("nested").join("req.jsonl");

    let err = Log::new(&path).unwrap_err();

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
    let log = Log::new(&path).unwrap();

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
    assert_eq!(cancelled.share, 1);
    drop(cancelled);

    let after = log.begin(BTreeMap::new());
    assert_eq!(
        after.share, 1,
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
        Log::new(&std::env::temp_dir().join("taiyaku-test-reqlog-once.jsonl")).expect("opens");

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
