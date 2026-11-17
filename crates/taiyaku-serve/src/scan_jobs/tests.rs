use std::cell::Cell;

use serde_json::json;
use taiyaku_library::{Library, derive, scan};

use taiyaku_library::progress::{Outcome, Step, Watch};

use super::{Reread, run_with};
use crate::test_support;

/// Two page directories, `a` walked before `b`, added to an in-memory library
/// and not yet scanned.
///
/// An in-memory library's working connection is its one shared connection, so
/// a test holds no guard of it while `run_with` runs: the closures read the
/// statuses from the connection they are handed.
fn two_dirs(
    tag: &str,
) -> (
    std::sync::Arc<Library>,
    test_support::Fixture,
    scan::Root,
    scan::Root,
) {
    let fixture = test_support::Fixture::new(tag);
    let library = test_support::library();
    let (a, b) = {
        let conn = library.conn();
        let add = |name: &str, series: &str| {
            let dir = fixture.path().join(name);
            test_support::write_pages(&dir.join(series).join(format!("{series} - v01")), 2);
            scan::add_root(&conn, &dir).expect("adds the directory")
        };
        (add("a", "Alpha"), add("b", "Beta"))
    };
    assert!(a.path < b.path, "sanity: a is walked first");
    (library, fixture, a, b)
}

/// `root`'s next walk is refused, as one of a directory scanned once and then
/// emptied on disk.
fn refuse_next_walk(library: &Library, root: &scan::Root) {
    let conn = library.conn();
    scan::scan_root(&conn, root).expect("the first scan");
    std::fs::remove_dir_all(&root.path).expect("empties the directory");
    std::fs::create_dir_all(&root.path).expect("makes it again");
}

fn status(conn: &rusqlite::Connection, root: &scan::Root) -> Option<String> {
    conn.query_row(
        "SELECT last_scan_status FROM root WHERE id = ?1",
        [root.id],
        |r| r.get(0),
    )
    .expect("reads the status")
}

fn statuses(library: &Library, a: &scan::Root, b: &scan::Root) -> (Option<String>, Option<String>) {
    let conn = library.conn();
    (status(&conn, a), status(&conn, b))
}

fn word(status: &str) -> Option<String> {
    Some(status.to_string())
}

/// O186: from each walk to the end of the one derive, a directory reads
/// `deriving`, so a stop anywhere in between leaves a status the next start
/// rewrites to `interrupted`.
#[test]
fn each_directory_reads_deriving_from_its_walk_until_its_derive_ends() {
    let (library, _fixture, a, b) = two_dirs("job-deriving");
    let job = crate::LibraryJob::default();
    let mut a_in_bs_walk = None;
    let mut in_the_derive = None;

    let reports = run_with(
        &library,
        &[a.clone(), b.clone()],
        Reread::Dir,
        &job,
        |conn, root| {
            if root.id == b.id {
                a_in_bs_walk = Some(status(conn, &a));
            }
            scan::scan_root(conn, root)
        },
        |conn| {
            in_the_derive = Some((status(conn, &a), status(conn, &b)));
            derive::derive(conn)
        },
    )
    .expect("the scan succeeds");

    assert_eq!(reports.len(), 2, "{reports:?}");
    // One assertion, so a failure shows which of the three moments is wrong.
    assert_eq!(
        (a_in_bs_walk, in_the_derive, statuses(&library, &a, &b)),
        (
            Some(word("deriving")),
            Some((word("deriving"), word("deriving"))),
            (word("ok"), word("ok"))
        ),
        "a while b is walked; a and b inside the derive; a and b once it has returned"
    );
}

/// A refused walk recorded nothing, so the derive that fails after it misses
/// nothing of its: only the walk that read `deriving` becomes `failed`.
#[test]
fn a_failed_derive_marks_failed_only_the_walks_that_read_deriving() {
    let (library, _fixture, a, b) = two_dirs("job-derive-fails");
    refuse_next_walk(&library, &a);
    let job = crate::LibraryJob::default();

    let why = run_with(
        &library,
        &[a.clone(), b.clone()],
        Reread::Dir,
        &job,
        scan::scan_root,
        |_| Err("the derive was told to fail".to_string()),
    )
    .expect_err("the derive fails");

    assert_eq!(
        why,
        format!(
            "{} was scanned, but deriving its structure afterward failed: \
             the derive was told to fail",
            a.path.display()
        )
    );
    assert_eq!(
        statuses(&library, &a, &b),
        (word("refused"), word("failed"))
    );
}

/// The derive's success marks `ok` only what read `deriving`.
#[test]
fn a_derive_that_succeeds_leaves_a_refused_walk_refused() {
    let (library, _fixture, a, b) = two_dirs("job-derive-succeeds");
    refuse_next_walk(&library, &a);
    let job = crate::LibraryJob::default();

    run_with(
        &library,
        &[a.clone(), b.clone()],
        Reread::Dir,
        &job,
        scan::scan_root,
        derive::derive,
    )
    .expect("the scan succeeds");

    assert_eq!(statuses(&library, &a, &b), (word("refused"), word("ok")));
}

/// What a batch's derive counts its calls with, before it derives for real.
fn counted(
    calls: &Cell<usize>,
    conn: &rusqlite::Connection,
) -> Result<derive::DeriveReport, String> {
    calls.set(calls.get() + 1);
    derive::derive(conn)
}

/// Spec §4, decision 9: one directory a scheduled scan cannot walk does not
/// hold back the others, or the derive.
#[test]
fn a_batch_walks_past_a_directory_it_cannot_walk() {
    let (library, _fixture, a, b) = two_dirs("batch-walk-fails");
    // Gone, so the walk fails for any uid: CI runs as root, where a `chmod`
    // refuses nothing.
    std::fs::remove_dir_all(&a.path).expect("deletes a's directory");
    let job = crate::LibraryJob::default();

    let reports =
        crate::test_support::run_unreserved(&library, &[a.clone(), b.clone()], Reread::Batch, &job)
            .expect("a batch answers each directory on its own slot");

    assert_eq!(reports.len(), 2, "{reports:?}");
    let a_said = reports[0]["error"]
        .as_str()
        .expect("a's slot holds an error");
    assert!(
        a_said.starts_with(&a.path.display().to_string()),
        "a's slot holds its own walk's error: {a_said}"
    );
    assert!(
        reports[1].get("scan").is_some() && reports[1].get("derive").is_some(),
        "b was walked and derived: {}",
        reports[1]
    );
    assert_eq!(statuses(&library, &a, &b), (word("failed"), word("ok")));
}

/// A batch's failed walk ends nothing, so what it walked before is still
/// derived and reads `ok`, unlike a directory scan's, which marks it `failed`.
#[test]
fn a_batch_keeps_what_it_walked_before_a_failed_walk() {
    let (library, _fixture, a, b) = two_dirs("batch-walk-fails-second");
    // Gone, for the reason `a_batch_walks_past_a_directory_it_cannot_walk` gives.
    std::fs::remove_dir_all(&b.path).expect("deletes b's directory");
    let job = crate::LibraryJob::default();

    let reports =
        crate::test_support::run_unreserved(&library, &[a.clone(), b.clone()], Reread::Batch, &job)
            .expect("a batch answers each directory on its own slot");

    assert_eq!(reports.len(), 2, "{reports:?}");
    assert!(
        reports[0].get("scan").is_some() && reports[0].get("derive").is_some(),
        "a was walked and derived: {}",
        reports[0]
    );
    let b_said = reports[1]["error"]
        .as_str()
        .expect("b's slot holds an error");
    assert!(
        b_said.starts_with(&b.path.display().to_string()),
        "b's slot holds its own walk's error: {b_said}"
    );
    assert_eq!(
        statuses(&library, &a, &b),
        (word("ok"), word("failed")),
        "a, walked before b's walk failed, is derived and reads ok"
    );
}

/// A directory removed while the batch waited for the lock is answered alone,
/// in the directory route's words, and the others are still walked.
#[test]
fn a_batch_answers_a_directory_changed_while_it_waited_on_its_own_slot() {
    let (library, _fixture, a, b) = two_dirs("batch-dir-changed");
    let asked = [a.clone(), b.clone()];
    scan::remove_root(&library.conn(), a.id).expect("removes a from the store");
    let job = crate::LibraryJob::default();

    let reports = crate::test_support::run_unreserved(&library, &asked, Reread::Batch, &job)
        .expect("a batch answers each directory on its own slot");

    assert_eq!(reports.len(), 2, "{reports:?}");
    assert_eq!(
        reports[0],
        json!({
            "error": "the directory was removed or changed while the scan waited to start, \
                      so nothing was scanned"
        })
    );
    assert!(
        reports[1].get("scan").is_some() && reports[1].get("derive").is_some(),
        "b was walked and derived: {}",
        reports[1]
    );
    assert_eq!(status(&library.conn(), &b), word("ok"));
}

/// The derive is over the whole store, so one after the batch's last walk
/// covers every directory it walked.
#[test]
fn a_batch_derives_once_over_what_it_walked() {
    let (library, _fixture, a, b) = two_dirs("batch-derives-once");
    let job = crate::LibraryJob::default();
    let derives = Cell::new(0);

    let reports = run_with(
        &library,
        &[a.clone(), b.clone()],
        Reread::Batch,
        &job,
        scan::scan_root,
        |conn| counted(&derives, conn),
    )
    .expect("a batch answers each directory on its own slot");

    assert_eq!(derives.get(), 1, "the derive runs once for the batch");
    assert_eq!(reports.len(), 2, "{reports:?}");
    assert!(
        reports.iter().all(|report| report.get("derive").is_some()),
        "each walked directory carries the one derive: {reports:?}"
    );
}

/// The batch has no one directory to name, and nobody pressed for it.
#[test]
fn a_batch_says_a_failed_derive_without_naming_a_directory() {
    let (library, _fixture, a, b) = two_dirs("batch-derive-fails");
    let job = crate::LibraryJob::default();

    let reports = run_with(
        &library,
        &[a.clone(), b.clone()],
        Reread::Batch,
        &job,
        scan::scan_root,
        |_| Err("the rules could not be read".to_string()),
    )
    .expect("a batch answers each directory on its own slot");

    let said = json!({
        "error": "the directories a scheduled scan walked were scanned, but deriving their \
                  structure afterward failed: the rules could not be read"
    });
    assert_eq!(reports, [said.clone(), said]);
    assert_eq!(statuses(&library, &a, &b), (word("failed"), word("failed")));
}

/// Every slot already holds its own error, and nothing new was recorded.
#[test]
fn a_batch_that_walked_nothing_derives_nothing() {
    let (library, _fixture, a, b) = two_dirs("batch-walked-nothing");
    let asked = [a.clone(), b.clone()];
    {
        let conn = library.conn();
        scan::remove_root(&conn, a.id).expect("removes a from the store");
        scan::remove_root(&conn, b.id).expect("removes b from the store");
    }
    let job = crate::LibraryJob::default();
    let derives = Cell::new(0);

    let reports = run_with(
        &library,
        &asked,
        Reread::Batch,
        &job,
        scan::scan_root,
        |conn| counted(&derives, conn),
    )
    .expect("a batch answers each directory on its own slot");

    let changed = json!({
        "error": "the directory was removed or changed while the scan waited to start, \
                  so nothing was scanned"
    });
    assert_eq!(
        (derives.get(), reports),
        (0, vec![changed.clone(), changed]),
        "no derive, and each slot says its directory changed"
    );
}

/// A failed derive answers only the walked directories: one that changed
/// while the batch waited keeps its own words.
#[test]
fn a_batch_answers_a_failed_derive_only_on_what_it_walked() {
    let (library, _fixture, a, b) = two_dirs("batch-derive-fails-past-a-change");
    let asked = [a.clone(), b.clone()];
    scan::remove_root(&library.conn(), a.id).expect("removes a from the store");
    let job = crate::LibraryJob::default();

    let reports = run_with(
        &library,
        &asked,
        Reread::Batch,
        &job,
        scan::scan_root,
        |_| Err("the rules could not be read".to_string()),
    )
    .expect("a batch answers each directory on its own slot");

    assert_eq!(
        reports,
        [
            json!({
                "error": "the directory was removed or changed while the scan waited to start, \
                          so nothing was scanned"
            }),
            json!({
                "error": "the directories a scheduled scan walked were scanned, but deriving \
                          their structure afterward failed: the rules could not be read"
            })
        ]
    );
    assert_eq!(status(&library.conn(), &b), word("failed"));
}

/// A file-backed library with one page directory added and not yet scanned,
/// and the path of its store: the derive lease is a lock file only where the
/// store is a file.
fn one_dir_on_disk(
    tag: &str,
) -> (
    Library,
    test_support::Fixture,
    std::path::PathBuf,
    scan::Root,
) {
    let fixture = test_support::Fixture::new(tag);
    let db_dir = fixture.path().join("db");
    std::fs::create_dir_all(&db_dir).expect("makes the store's directory");
    let store = db_dir.join("library.db");
    let library = Library::open(&store).expect("opens a file-backed library");
    let dir = fixture.path().join("a");
    test_support::write_pages(&dir.join("Alpha").join("Alpha - v01"), 2);
    let root = scan::add_root(&library.conn(), &dir).expect("adds the directory");
    (library, fixture, store, root)
}

/// O51: a scan whose derive meets another derive's lease waits for it, reading
/// `waiting`, and derives once it holds the lease, reading `deriving`.
#[test]
fn a_scan_waits_for_another_derives_lease_and_derives_holding_its_own() {
    let (library, _fixture, store, a) = one_dir_on_disk("job-waits-for-the-lease");
    let elsewhere = Library::open(&store).expect("another connection, as another process");
    let held = elsewhere
        .derive_lease(taiyaku_library::lease::Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");

    let job_store = store.clone();
    let job_a = a.clone();
    let job = std::thread::spawn(move || {
        let watcher = Library::open(&job_store).expect("a third connection, to probe");
        let mut in_the_derive = None;
        let reports = run_with(
            &library,
            std::slice::from_ref(&job_a),
            Reread::Dir,
            &crate::LibraryJob::default(),
            scan::scan_root,
            |conn| {
                in_the_derive = Some((status(conn, &job_a), watcher.derive_running()));
                derive::derive(conn)
            },
        );
        (reports, in_the_derive, status(&library.conn(), &job_a))
    });

    let started = std::time::Instant::now();
    let mut seen = status(&elsewhere.conn(), &a);
    while seen.as_deref() != Some("waiting")
        && started.elapsed() < std::time::Duration::from_secs(10)
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
        seen = status(&elsewhere.conn(), &a);
    }
    assert_eq!(
        seen,
        word("waiting"),
        "a walked directory reads waiting while its derive waits for the lease"
    );
    assert!(!job.is_finished(), "the scan waits while the lease is held");

    drop(held);
    let (reports, in_the_derive, after) = job.join().expect("the scan did not panic");
    assert_eq!(reports.expect("the scan succeeds").len(), 1);
    assert_eq!(
        (in_the_derive, after),
        (Some((word("deriving"), true)), word("ok")),
        "inside the derive: deriving, and the lease held; after it: ok"
    );
}

/// O51: a lease that cannot be held fails the derive, as a derive that failed
/// would: the walked directory reads `failed`, and the report says so.
#[test]
fn a_lease_that_cannot_be_held_fails_the_derive() {
    let (library, _fixture, store, a) = one_dir_on_disk("job-lease-fails");
    let lock = std::path::PathBuf::from(format!("{}.derive-lock", store.display()));
    std::fs::create_dir_all(&lock).expect("a directory where the lock file goes");

    let why = crate::test_support::run_unreserved(
        &library,
        std::slice::from_ref(&a),
        Reread::Dir,
        &crate::LibraryJob::default(),
    )
    .expect_err("the derive fails");

    let said = format!(
        "{} was scanned, but deriving its structure afterward failed: \
         opening the derive lock file {}: ",
        a.path.display(),
        lock.display()
    );
    assert!(why.starts_with(&said), "{why}");
    assert_eq!(status(&library.conn(), &a), word("failed"));
}

/// O51: a lock file the server cannot write holds no lease, so the scan
/// derives nothing, and its directory reads `failed` with the reason.
#[test]
fn a_scan_whose_lock_file_cannot_be_written_derives_nothing_and_says_why() {
    use std::os::unix::fs::PermissionsExt;

    let (library, _fixture, store, a) = one_dir_on_disk("job-lease-read-only");
    drop(
        library
            .derive_lease(taiyaku_library::lease::Wait::No)
            .expect("takes the lease")
            .expect("the lease is free"),
    );
    let lock = std::path::PathBuf::from(format!("{}.derive-lock", store.display()));
    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o444)).expect("chmod");
    if std::fs::OpenOptions::new().write(true).open(&lock).is_ok() {
        return;
    }
    let mut derived = false;

    let why = run_with(
        &library,
        std::slice::from_ref(&a),
        Reread::Dir,
        &crate::LibraryJob::default(),
        scan::scan_root,
        |conn| {
            derived = true;
            derive::derive(conn)
        },
    )
    .expect_err("the derive fails");

    assert_eq!(
        (derived, why, status(&library.conn(), &a)),
        (
            false,
            format!(
                "{} was scanned, but deriving its structure afterward failed: the derive lock \
                 file {} cannot be written by this process, so no derive can take its turn: \
                 chown it to the user taiyaku runs as, or make it writable (chmod u+w)",
                a.path.display(),
                lock.display()
            ),
            word("failed")
        )
    );
}

/// O51: a scan queued behind another job that cannot open its working
/// connection once it holds the job lock walks nothing, and its directory
/// reads its last scan's word again, not `waiting`, which every page would
/// read as queued until its next scan or start. Its last word is `refused`,
/// which only a walk writes and this path walks nothing, so a write-back of
/// any word of the job's own (`failed`, `ok`, `interrupted`) reads wrong.
/// Skipped where the log opens whatever its mode, as root, which is how CI
/// runs.
#[test]
fn a_queued_scan_that_cannot_open_its_connection_reads_its_last_word_again() {
    use std::os::unix::fs::PermissionsExt;

    let (library, _fixture, store, a) = one_dir_on_disk("job-queued-no-connection");
    library
        .conn()
        .execute(
            "UPDATE root SET last_scan_status = 'refused' WHERE id = ?1",
            [a.id],
        )
        .expect("sets its last word");
    let job = crate::LibraryJob::default();
    let held = job.lock().expect("the job lock is free");

    std::thread::scope(|scope| {
        let scan = scope.spawn(|| {
            run_with(
                &library,
                std::slice::from_ref(&a),
                Reread::Dir,
                &job,
                scan::scan_root,
                derive::derive,
            )
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while status(&library.conn(), &a).as_deref() != Some("waiting")
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            status(&library.conn(), &a),
            word("waiting"),
            "queued behind the job lock"
        );
        // The write-ahead log, not the store: a connection opened anew reuses
        // a store descriptor this process set aside, but opens its own log.
        // The shared connection keeps the log it holds open.
        let log = std::path::PathBuf::from(format!("{}-wal", store.display()));
        let mode = std::fs::metadata(&log).expect("the log").permissions();
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let opens_anyway = std::fs::File::open(&log).is_ok();
        drop(held);
        let outcome = scan.join().expect("the scan did not panic");
        std::fs::set_permissions(&log, mode).expect("chmod back");
        if opens_anyway {
            return;
        }
        let why = outcome.expect_err("the scan has no connection to walk on");
        assert!(why.starts_with("setting WAL: "), "{why}");
        assert_eq!(status(&library.conn(), &a), word("refused"));
    });
}

/// O51: the job says on its slot when its derive starts to wait for the lease
/// and when it holds it, which the store, whose write lock another derive may
/// hold, cannot always be told; and while it waits, it is in the step Waiting.
#[test]
fn a_scan_says_when_it_waits_for_the_lease_and_when_it_holds_it() {
    let (library, _fixture, store, a) = one_dir_on_disk("job-says-it-waits");
    let elsewhere = Library::open(&store).expect("another connection, as another process");
    let held = elsewhere
        .derive_lease(taiyaku_library::lease::Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    let started = std::time::Instant::now();
    let job = test_support::a_planted_job();
    let scans = a_registry_holding(&job, &a);
    let waits = || scans.lock().expect("the registry")[&a.id].waiting;

    std::thread::scope(|scope| {
        let scan =
            scope.spawn(|| a_job_of(&library, &a, &job, &scans).map(|reports| reports.len()));
        while !waits() && started.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let in_the_wait = (waits(), job.snapshot().step, scan.is_finished());
        drop(held);
        let ran = scan.join().expect("the scan did not panic");

        assert_eq!(
            (in_the_wait, ran, waits()),
            ((true, Step::Waiting, false), Ok(1), false),
            "(waiting, its step and whether it ended while the lease is held elsewhere), \
             what it answered, and waiting once it ended"
        );
    });
}

/// A registry holding `root`'s slot for `job`, not yet waiting.
fn a_registry_holding(
    job: &std::sync::Arc<super::Job>,
    root: &scan::Root,
) -> std::sync::Mutex<std::collections::HashMap<i64, crate::ScanProgress>> {
    std::sync::Mutex::new(std::collections::HashMap::from([(
        root.id,
        crate::ScanProgress {
            waiting: false,
            started: job.started,
            done: false,
            report: None,
            library: None,
            job: std::sync::Arc::clone(job),
        },
    )]))
}

/// `job`, a scan of `root` with the real walk and derive, saying on `scans`
/// whether it waits.
fn a_job_of(
    library: &Library,
    root: &scan::Root,
    job: &super::Job,
    scans: &std::sync::Mutex<std::collections::HashMap<i64, crate::ScanProgress>>,
) -> Result<Vec<serde_json::Value>, String> {
    super::run_watched(
        library,
        std::slice::from_ref(root),
        Reread::Dir,
        &crate::LibraryJob::default(),
        &super::Running::new(job, Some(scans)),
        scan::scan_root_watched,
        |conn, fresh, watch| {
            derive::derive_watched(conn, derive::Documents::Reconcile, fresh, watch)
        },
    )
}

/// Spec §7, "Cancel checks", and §10.5: a job cancelled while its derive
/// waits for the lease another process holds, as the CLI's derive can, ends
/// at once, not once that derive ends: it answers that it stopped while
/// waiting, and its directory reads `cancelled` and is paused.
#[test]
fn a_job_cancelled_while_it_waits_for_the_lease_ends_without_the_holder() {
    let (library, _fixture, store, a) = one_dir_on_disk("job-cancelled-on-the-lease");
    let elsewhere = Library::open(&store).expect("another connection, as another process");
    let held = elsewhere
        .derive_lease(taiyaku_library::lease::Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    let started = std::time::Instant::now();
    let job = test_support::a_planted_job();
    let scans = a_registry_holding(&job, &a);
    let waits = || scans.lock().expect("the registry")[&a.id].waiting;

    let (ended_while_held, ran) = std::thread::scope(|scope| {
        let scan = scope.spawn(|| a_job_of(&library, &a, &job, &scans));
        while !waits() && started.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        job.request_cancel();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !scan.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let ended_while_held = scan.is_finished();
        drop(held);
        (
            ended_while_held,
            scan.join().expect("the scan did not panic"),
        )
    });

    assert_eq!(
        (ended_while_held, ran),
        (true, Ok(vec![cancelled_in("waiting")])),
        "(whether it ended within two seconds, the lease still held elsewhere; \
         what it answered)"
    );
    let conn = library.conn();
    assert_eq!(
        (status(&conn, &a), paused(&conn, &a)),
        (word("cancelled"), true),
        "(its word, paused)"
    );
}

/// O51: a scan [`super::spawn`] started says it waits for the lease on the
/// reservation of each directory whose walk recorded something, where the
/// lists and the scan status read it, and on no other: a directory of the
/// batch it could not walk reads its own word, as it has nothing to derive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_spawned_scan_says_it_waits_on_what_it_walked_and_nothing_else() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let roots = scan::roots(&state.library.conn()).expect("the directories");
    let (gone, walked) = (roots[0].clone(), roots[1].clone());
    // Gone, for the reason `a_batch_walks_past_a_directory_it_cannot_walk` gives.
    std::fs::remove_dir_all(&gone.path).expect("deletes the first directory");
    let elsewhere = test_support::the_store_elsewhere(&state);
    let held = test_support::holding_the_lease(&elsewhere);
    let started = std::time::Instant::now();
    super::reserve(&state, &roots, started, None).expect("reserves");

    let job = super::spawn(
        std::sync::Arc::clone(&state),
        roots.clone(),
        started,
        Reread::Batch,
    );
    let waits = |root: &scan::Root| {
        state
            .scans
            .lock()
            .expect("the registry")
            .get(&root.id)
            .is_some_and(|progress| progress.waiting)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !waits(&walked) && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let seen = (waits(&gone), waits(&walked));
    drop(held);
    job.await.expect("the job ends");

    assert_eq!(
        seen,
        (false, true),
        "(the directory it could not walk, the one it walked) read waiting"
    );
}

/// O51: a walk the scan refused recorded nothing, so nothing of its waits for
/// the lease: while the batch's derive waits, its directory is sent `refused`,
/// the word that says to check the share is mounted, and not `waiting`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_spawned_scan_does_not_say_a_refused_walk_waits() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let roots = scan::roots(&state.library.conn()).expect("the directories");
    let (refused, walked) = (roots[0].clone(), roots[1].clone());
    // Emptied but kept, so its walk is refused rather than failed.
    std::fs::remove_dir_all(&refused.path).expect("empties the first directory");
    std::fs::create_dir_all(&refused.path).expect("makes it again");
    let elsewhere = test_support::the_store_elsewhere(&state);
    let held = test_support::holding_the_lease(&elsewhere);
    let started = std::time::Instant::now();
    super::reserve(&state, &roots, started, None).expect("reserves");

    let job = super::spawn(
        std::sync::Arc::clone(&state),
        roots.clone(),
        started,
        Reread::Batch,
    );
    let waits = |root: &scan::Root| {
        state
            .scans
            .lock()
            .expect("the registry")
            .get(&root.id)
            .is_some_and(|progress| progress.waiting)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !waits(&walked) && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let stored = status(&state.library.conn(), &refused);
    let sent = super::status_sent(
        &super::waiting_for_a_lock(&state),
        refused.id,
        stored.clone(),
    );
    let seen = (waits(&walked), stored, sent);
    drop(held);
    job.await.expect("the job ends");

    assert_eq!(
        seen,
        (true, word("refused"), word("refused")),
        "(the walked directory waits, the refused one's stored word, the word it is sent)"
    );
}

/// `root`'s stored word once it reads `wanted`, or the last word read when
/// ten seconds pass first.
async fn until_stored(
    state: &crate::ServeState,
    root: &scan::Root,
    wanted: &str,
) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let seen = status(&state.library.conn(), root);
        if seen.as_deref() == Some(wanted) || std::time::Instant::now() > deadline {
            return seen;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// A scan of `root` reserved and spawned as a route does, held inside its
/// derive, and so holding the job lock, until the sender is dropped.
fn a_scan_held_in_its_derive(
    state: &std::sync::Arc<crate::ServeState>,
    root: &scan::Root,
) -> (std::sync::mpsc::Sender<()>, tokio::task::JoinHandle<()>) {
    let started = std::time::Instant::now();
    super::reserve(state, std::slice::from_ref(root), started, None).expect("reserves it");
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (said, heard) = std::sync::mpsc::channel::<()>();
    let job = super::spawn_job(
        std::sync::Arc::clone(state),
        vec![root.clone()],
        started,
        move |state, roots, running| {
            super::run_watched(
                &state.library,
                roots,
                Reread::Dir,
                &state.library_job,
                running,
                |conn, root, _| scan::scan_root(conn, root).map(Outcome::Done),
                |conn, _, _| {
                    let _ = said.send(());
                    let _ = released.recv();
                    derive::derive(conn).map(Outcome::Done)
                },
            )
        },
    );
    heard
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the scan reaches its derive");
    (release, job)
}

/// A scan accepted while another scan holds the job lock has not started, so
/// it reads `waiting` in its own status and both lists, and in the store for
/// a restart to find, until it holds the lock; walking, it reads the word its
/// last scan left, and then `ok`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_queued_behind_another_reads_waiting_until_it_holds_the_job_lock() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let roots = scan::roots(&state.library.conn()).expect("the directories");
    let (first, second) = (roots[0].clone(), roots[1].clone());
    // Not `ok`, so the word read while it walks is told from the one it ends with.
    state
        .library
        .conn()
        .execute(
            "UPDATE root SET last_scan_status = 'failed' WHERE id = ?1",
            [second.id],
        )
        .expect("sets its last word");
    let (release_the_first, first_job) = a_scan_held_in_its_derive(&state, &first);

    let started = std::time::Instant::now();
    super::reserve(&state, std::slice::from_ref(&second), started, None).expect("reserves it");
    let accepted = test_support::last_scan_words(&router, second.id).await;
    let (walks, walking) = std::sync::mpsc::channel::<()>();
    let (go_on, gone_on) = std::sync::mpsc::channel::<()>();
    let second_job = super::spawn_job(
        std::sync::Arc::clone(&state),
        vec![second.clone()],
        started,
        move |state, roots, running| {
            super::run_watched(
                &state.library,
                roots,
                Reread::Dir,
                &state.library_job,
                running,
                |conn, root, _| {
                    let _ = walks.send(());
                    let _ = gone_on.recv();
                    scan::scan_root(conn, root).map(Outcome::Done)
                },
                |conn, _, _| derive::derive(conn).map(Outcome::Done),
            )
        },
    );
    let stored = until_stored(&state, &second, "waiting").await;
    let queued = test_support::last_scan_words(&router, second.id).await;
    drop(release_the_first);
    walking
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the second walks once the first has ended");
    let in_its_walk = test_support::last_scan_words(&router, second.id).await;
    drop(go_on);
    second_job.await.expect("the second ends");
    first_job.await.expect("the first ends");
    let after = test_support::last_scan_words(&router, second.id).await;

    assert_eq!(
        [accepted, queued, in_its_walk, after],
        [
            test_support::every_page_reads(true, "waiting"),
            test_support::every_page_reads(true, "waiting"),
            test_support::every_page_reads(true, "failed"),
            test_support::every_page_reads(false, "ok"),
        ],
        "(accepted, queued, walking, after)"
    );
    assert_eq!(stored, word("waiting"), "the store's word while it waits");
}

/// A job holding the store's write lock with the job lock, as a rules save
/// does through its derive, keeps a scan queued behind it from writing its
/// word to the store; the scan's reservation says it waits, and every page
/// reads that.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_queued_behind_a_job_writing_the_store_reads_waiting() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let second = scan::roots(&state.library.conn()).expect("the directories")[1].clone();
    let started = std::time::Instant::now();
    // While the lock is free, so that the job alone can say it waits.
    super::reserve(&state, std::slice::from_ref(&second), started, None).expect("reserves it");
    let held = test_support::holding_the_job(&state);
    let elsewhere = test_support::the_store_elsewhere(&state);
    elsewhere
        .conn()
        .execute_batch("BEGIN IMMEDIATE")
        .expect("holds the store's write lock");

    let job = super::spawn(
        std::sync::Arc::clone(&state),
        vec![second.clone()],
        started,
        Reread::Dir,
    );
    let waits = || {
        state
            .scans
            .lock()
            .expect("the registry")
            .get(&second.id)
            .is_some_and(|progress| progress.waiting)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !waits() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let queued = test_support::last_scan_words(&router, second.id).await;
    let stored = status(&state.library.conn(), &second);
    elsewhere
        .conn()
        .execute_batch("ROLLBACK")
        .expect("lets the store go");
    drop(held);
    job.await.expect("the job ends");
    let after = test_support::last_scan_words(&router, second.id).await;

    assert_eq!(
        [queued, after],
        [
            test_support::every_page_reads(true, "waiting"),
            test_support::every_page_reads(false, "ok"),
        ],
        "(queued, after)"
    );
    assert_eq!(
        stored,
        word("ok"),
        "the store still holds the last scan's word"
    );
}

/// A [`crate::ServeState`] over `library`, whose directories are in `fixture`.
fn state_over(
    library: std::sync::Arc<Library>,
    fixture: &test_support::Fixture,
) -> std::sync::Arc<crate::ServeState> {
    std::sync::Arc::new(crate::ServeState {
        library,
        ..test_support::state(fixture.path())
    })
}

/// Each step a job showed, with the directory it was on.
type Seen = Vec<(Step, Option<(usize, usize)>)>;

/// A watch that passes each report on to `inner`, then keeps `job`'s step and
/// directory wherever they changed.
struct Recording<'a> {
    inner: &'a dyn Watch,
    job: &'a super::Job,
    seen: &'a std::sync::Mutex<Seen>,
}

impl Watch for Recording<'_> {
    fn at(&self, step: Step, done: u64, of: Option<u64>) {
        self.inner.at(step, done, of);
        let now = self.job.snapshot();
        let mut seen = self.seen.lock().expect("one recorder");
        if seen.last() != Some(&(now.step, now.dir)) {
            seen.push((now.step, now.dir));
        }
    }

    fn cancelled(&self) -> bool {
        self.inner.cancelled()
    }
}

/// A watch that passes each report on to `inner`, and asks `job` to cancel
/// at the first report `when` names, by its step and count.
struct CancelWhen<'a> {
    inner: &'a dyn Watch,
    job: &'a super::Job,
    when: fn(Step, u64) -> bool,
}

impl Watch for CancelWhen<'_> {
    fn at(&self, step: Step, done: u64, of: Option<u64>) {
        self.inner.at(step, done, of);
        if (self.when)(step, done) {
            self.job.request_cancel();
        }
    }

    fn cancelled(&self) -> bool {
        self.inner.cancelled()
    }
}

/// A job of `asked`, as `reread` says, cancelled at the first count of
/// hashing after one digest.
fn cancelled_after_one_digest(
    library: &Library,
    asked: &[scan::Root],
    reread: Reread,
) -> Result<Vec<serde_json::Value>, String> {
    let job = super::Job::new(
        std::time::Instant::now(),
        Vec::new(),
        std::sync::Arc::default(),
    );
    super::run_watched(
        library,
        asked,
        reread,
        &crate::LibraryJob::default(),
        &super::Running::new(&job, None),
        scan::scan_root_watched,
        |conn, fresh, watch| {
            derive::derive_watched(
                conn,
                derive::Documents::Reconcile,
                fresh,
                &CancelWhen {
                    inner: watch,
                    job: &job,
                    when: |step, done| step == Step::Hashing && done > 0,
                },
            )
        },
    )
}

/// A job of `asked` that asks itself to cancel as each walk returns.
fn cancelled_after_the_first_walk(
    library: &Library,
    asked: &[scan::Root],
    walked: &mut Vec<i64>,
) -> Result<Vec<serde_json::Value>, String> {
    let job = super::Job::new(
        std::time::Instant::now(),
        Vec::new(),
        std::sync::Arc::default(),
    );
    super::run_watched(
        library,
        asked,
        Reread::Dir,
        &crate::LibraryJob::default(),
        &super::Running::new(&job, None),
        |conn, root, watch| {
            walked.push(root.id);
            let walk = scan::scan_root_watched(conn, root, watch);
            job.request_cancel();
            walk
        },
        |conn, fresh, watch| {
            derive::derive_watched(conn, derive::Documents::Reconcile, fresh, watch)
        },
    )
}

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect("counts")
}

const DIGESTS: &str = "SELECT count(*) FROM scan_entry WHERE sha256 IS NOT NULL";

fn last_scan_at(conn: &rusqlite::Connection, root: &scan::Root) -> Option<String> {
    conn.query_row(
        "SELECT last_scan_at FROM root WHERE id = ?1",
        [root.id],
        |r| r.get(0),
    )
    .expect("reads when it was last scanned")
}

fn paused(conn: &rusqlite::Connection, root: &scan::Root) -> bool {
    conn.query_row(
        "SELECT paused_at IS NOT NULL FROM root WHERE id = ?1",
        [root.id],
        |r| r.get(0),
    )
    .expect("reads whether it is paused")
}

fn both_paused(library: &Library, a: &scan::Root, b: &scan::Root) -> (bool, bool) {
    let conn = library.conn();
    (paused(&conn, a), paused(&conn, b))
}

/// `root`'s slot in `state`'s registry: whether it is done, and its report.
fn slot(state: &crate::ServeState, root: &scan::Root) -> (bool, Option<serde_json::Value>) {
    let scans = state.scans.lock().expect("the registry");
    let slot = &scans[&root.id];
    (slot.done, slot.report.clone())
}

fn cancelled_in(step: &str) -> serde_json::Value {
    json!({ "cancelled": { "step": step } })
}

/// Spec §3, Goal 2: a job shows its step, and while it walks, which of its
/// directories it is on, from its reservation to its end; and the libraries
/// whose pages show it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_reports_its_directory_and_step_as_it_goes() {
    let (library, fixture, a, b) = two_dirs("job-reports-its-step");
    let state = state_over(library, &fixture);
    let roots = vec![a.clone(), b.clone()];
    let started = std::time::Instant::now();
    let job = super::reserve(&state, &roots, started, None).expect("reserves");
    let reserved = job.snapshot();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(vec![(reserved.step, reserved.dir)]));
    let ended_before = state.jobs_ended.load(std::sync::atomic::Ordering::SeqCst);

    let recorder = (std::sync::Arc::clone(&job), std::sync::Arc::clone(&seen));
    super::spawn_job(
        std::sync::Arc::clone(&state),
        roots,
        started,
        move |state, roots, running| {
            let (job, seen) = (&*recorder.0, &*recorder.1);
            super::run_watched(
                &state.library,
                roots,
                Reread::Dir,
                &state.library_job,
                running,
                |conn, root, watch| {
                    scan::scan_root_watched(
                        conn,
                        root,
                        &Recording {
                            inner: watch,
                            job,
                            seen,
                        },
                    )
                },
                |conn, fresh, watch| {
                    derive::derive_watched(
                        conn,
                        derive::Documents::Reconcile,
                        fresh,
                        &Recording {
                            inner: watch,
                            job,
                            seen,
                        },
                    )
                },
            )
        },
    )
    .await
    .expect("the job ends");

    let seen = seen.lock().expect("the record").clone();
    assert_eq!(
        seen,
        [
            (Step::Waiting, None),
            (Step::Listing, Some((1, 2))),
            (Step::Resolving, Some((1, 2))),
            (Step::Listing, Some((2, 2))),
            (Step::Resolving, Some((2, 2))),
            (Step::Hashing, None),
            (Step::Writing, None),
            (Step::Finishing, None),
        ],
        "each step and directory, in order"
    );
    assert_eq!(
        (
            job.ended(),
            ended_before,
            state.jobs_ended.load(std::sync::atomic::Ordering::SeqCst)
        ),
        (Some(super::JobEnded::Finished), 0, 1),
        "(how it ended, the server's count of ended jobs once it was reserved, once it ended)"
    );
    assert_eq!(
        (job.dirs.clone(), job.id == test_support::a_planted_job().id),
        (vec![(a.id, a.path.clone()), (b.id, b.path.clone())], false),
        "(its directories, whether a later job took its id)"
    );
    assert_eq!(
        *job.libraries.lock().expect("its libraries"),
        [
            (a.libraries[0], "a".to_string()),
            (b.libraries[0], "b".to_string())
        ],
        "the library each directory is in"
    );
}

/// Spec §7, "Cancel checks", and §10.5: a job cancelled while it waits for
/// the job lock never starts, and ends at once, not after the holder:
/// nothing walked, each directory `cancelled` and paused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_cancelled_while_it_waits_for_the_job_lock_ends_without_the_holder() {
    let (library, fixture, a, b) = two_dirs("job-cancelled-waiting");
    let state = state_over(library, &fixture);
    let held = test_support::holding_the_job(&state);
    let roots = vec![a.clone(), b.clone()];
    let started = std::time::Instant::now();
    let job = super::reserve(&state, &roots, started, None).expect("reserves");
    let spawned = super::spawn(std::sync::Arc::clone(&state), roots, started, Reread::Dir);
    let queued = until_stored(&state, &a, "waiting").await;

    job.request_cancel();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !slot(&state, &a).0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let in_time = (slot(&state, &a).0, state.library_job.is_held());
    let reports = (slot(&state, &a).1, slot(&state, &b).1);
    drop(held);
    spawned.await.expect("the job ends");

    assert_eq!(queued, word("waiting"), "queued behind the holder");
    assert_eq!(
        in_time,
        (true, true),
        "(its slot is done within two seconds, the holder still holds the lock)"
    );
    assert_eq!(
        reports,
        (Some(cancelled_in("waiting")), Some(cancelled_in("waiting")))
    );
    let conn = state.library.conn();
    assert_eq!(
        (
            count(&conn, "SELECT count(*) FROM scan_entry"),
            (status(&conn, &a), status(&conn, &b)),
            (paused(&conn, &a), paused(&conn, &b)),
        ),
        (0, (word("cancelled"), word("cancelled")), (true, true)),
        "(rows, statuses, paused)"
    );
    assert_eq!(job.ended(), Some(super::JobEnded::Cancelled(Step::Waiting)));
}

/// Spec §5, §7 and §10.2: a first scan cancelled while it hashes keeps the
/// digests it computed and writes no work; each directory still reads
/// Never, says `cancelled` and is paused.
#[test]
fn a_cancelled_first_scan_reads_never_cancelled_and_paused_with_its_digests_kept() {
    let (library, _fixture, a, b) = two_dirs("job-cancelled-hashing");

    let reports = cancelled_after_one_digest(&library, &[a.clone(), b.clone()], Reread::Dir);

    assert_eq!(
        reports,
        Ok(vec![cancelled_in("hashing"), cancelled_in("hashing")])
    );
    let conn = library.conn();
    let of = |root: &scan::Root| {
        (
            last_scan_at(&conn, root),
            status(&conn, root),
            paused(&conn, root),
        )
    };
    assert_eq!(
        (of(&a), of(&b)),
        (
            (None, word("cancelled"), true),
            (None, word("cancelled"), true)
        ),
        "(last scan, status, paused) of each"
    );
    assert_eq!(
        (
            count(&conn, DIGESTS),
            count(&conn, "SELECT count(*) FROM work")
        ),
        (1, 0),
        "(digests, works)"
    );
}

/// `root`'s entries that hold a digest.
fn digests_in(conn: &rusqlite::Connection, root: &scan::Root) -> i64 {
    count(
        conn,
        &format!(
            "SELECT count(*) FROM scan_entry WHERE root_id = {} AND sha256 IS NOT NULL",
            root.id
        ),
    )
}

/// The members held under `root`.
fn member_roots_in(conn: &rusqlite::Connection, root: &scan::Root) -> i64 {
    count(
        conn,
        &format!(
            "SELECT count(*) FROM member_root WHERE root_id = {}",
            root.id
        ),
    )
}

/// Each report `watch` was given at `step`, in order.
fn reports_at(
    watch: &taiyaku_library::test_support::Recording,
    step: Step,
) -> Vec<taiyaku_library::test_support::Report> {
    watch
        .reports()
        .into_iter()
        .filter(|(at, _, _)| *at == step)
        .collect()
}

/// `a` with a second volume, `Alpha - v02` (3 pages), its first import
/// cancelled after one digest: one of its files hashed, none written, paused.
fn a_cancelled_first_import(library: &Library, fixture: &test_support::Fixture, a: &scan::Root) {
    test_support::write_pages(
        &fixture.path().join("a").join("Alpha").join("Alpha - v02"),
        3,
    );
    cancelled_after_one_digest(library, std::slice::from_ref(a), Reread::Dir)
        .expect("the job answers");
    let conn = library.conn();
    assert_eq!(
        (
            digests_in(&conn, a),
            member_roots_in(&conn, a),
            paused(&conn, a)
        ),
        (1, 0, true),
        "the fixture: a's first import hashed one file, wrote nothing and paused it"
    );
}

/// Spec §9, O320: another library's scan freezes a directory a cancel paused,
/// so it hashes none of it (its Hashing total is the other library's bytes
/// alone) and writes none of its works; a pressed Scan of it writes them all.
#[test]
fn a_cancelled_first_import_is_hashed_by_no_other_librarys_scan_and_whole_by_its_own() {
    let (library, fixture, a, b) = two_dirs("job-frozen-other-library");
    a_cancelled_first_import(&library, &fixture, &a);
    let b_bytes: u64 = std::fs::read_dir(fixture.path().join("b").join("Beta").join("Beta - v01"))
        .expect("lists b's volume")
        .map(|page| page.expect("a page").metadata().expect("its size").len())
        .sum();
    let watch = taiyaku_library::test_support::Recording::default();
    let job = super::Job::new(
        std::time::Instant::now(),
        Vec::new(),
        std::sync::Arc::default(),
    );

    super::run_watched(
        &library,
        std::slice::from_ref(&b),
        Reread::Library(b.libraries[0]),
        &crate::LibraryJob::default(),
        &super::Running::new(&job, None),
        scan::scan_root_watched,
        |conn, fresh, _| derive::derive_watched(conn, derive::Documents::Reconcile, fresh, &watch),
    )
    .expect("b's library's scan answers");

    assert_eq!(
        reports_at(&watch, Step::Hashing).last(),
        Some(&(Step::Hashing, b_bytes, Some(b_bytes))),
        "b's bytes hashed of b's bytes"
    );
    {
        let conn = library.conn();
        assert_eq!(
            (digests_in(&conn, &a), member_roots_in(&conn, &a)),
            (1, 0),
            "a's (digests, members) as its cancel left them"
        );
    }
    let job = super::Job::new(
        std::time::Instant::now(),
        Vec::new(),
        std::sync::Arc::default(),
    );
    super::run(
        &library,
        std::slice::from_ref(&a),
        Reread::Dir,
        &crate::LibraryJob::default(),
        &super::Running::new(&job, None),
    )
    .expect("a's scan answers");
    let conn = library.conn();
    assert_eq!(
        (member_roots_in(&conn, &a), paused(&conn, &a)),
        (2, false),
        "after a's own Scan: (a's members, paused)"
    );
}

/// Spec §7, "Who is frozen": a pressed Scan reads what it covers fresh, so a
/// directory whose unpause the store refused, as a rules save holding the
/// write lock refuses it, is still hashed and written whole.
#[test]
fn a_pressed_scan_whose_unpause_is_refused_still_reads_its_directory_fresh() {
    let (library, fixture, a, _b) = two_dirs("job-unpause-refused");
    a_cancelled_first_import(&library, &fixture, &a);
    library
        .conn()
        .execute_batch(
            "CREATE TRIGGER refuse_unpause BEFORE UPDATE OF paused_at ON root
             WHEN NEW.paused_at IS NULL
             BEGIN SELECT RAISE(ABORT, 'the store refuses the write, as one a rules save holds'); END;",
        )
        .expect("refuses every unpause");
    let job = super::Job::new(
        std::time::Instant::now(),
        Vec::new(),
        std::sync::Arc::default(),
    );

    let errors = super::run(
        &library,
        std::slice::from_ref(&a),
        Reread::Dir,
        &crate::LibraryJob::default(),
        &super::Running::new(&job, None),
    )
    .map(|slots| {
        slots
            .iter()
            .map(|slot| slot.get("error").cloned())
            .collect::<Vec<_>>()
    });

    let conn = library.conn();
    assert_eq!(
        (
            errors,
            paused(&conn, &a),
            member_roots_in(&conn, &a),
            digests_in(&conn, &a)
        ),
        (Ok(vec![None]), true, 2, 2),
        "(each slot's error; still paused, so the unpause was refused; all of a written and \
         hashed)"
    );
}

/// Spec §7: a cancel writes back the last scan each directory had before
/// the job, which its walk replaced.
#[test]
fn a_cancelled_scan_puts_back_the_last_scan_a_directory_had() {
    const BEFORE: &str = "2026-11-01 00:00:00";
    let (library, _fixture, a, b) = two_dirs("job-cancelled-rescan");
    {
        let conn = library.conn();
        scan::scan_root(&conn, &a).expect("a's earlier scan");
        conn.execute(
            "UPDATE root SET last_scan_at = ?2 WHERE id = ?1",
            rusqlite::params![a.id, BEFORE],
        )
        .expect("dates it");
    }

    cancelled_after_one_digest(&library, &[a.clone(), b.clone()], Reread::Dir)
        .expect("the job answers");

    let conn = library.conn();
    assert_eq!(
        (last_scan_at(&conn, &a), last_scan_at(&conn, &b)),
        (Some(BEFORE.to_string()), None)
    );
}

/// Spec §7, "Cancel checks": a cancel between directories walks no more, so
/// the directory not yet walked records nothing.
#[test]
fn a_job_cancelled_between_directories_walks_no_more() {
    let (library, _fixture, a, b) = two_dirs("job-cancelled-between");
    let mut walked = Vec::new();

    let reports = cancelled_after_the_first_walk(&library, &[a.clone(), b.clone()], &mut walked);

    let conn = library.conn();
    let b_rows = count(
        &conn,
        &format!("SELECT count(*) FROM scan_entry WHERE root_id = {}", b.id),
    );
    assert_eq!(
        (walked, b_rows),
        (vec![a.id], 0),
        "(the walks entered, b's rows)"
    );
    assert_eq!(
        reports,
        Ok(vec![cancelled_in("resolving"), cancelled_in("resolving")])
    );
    assert_eq!(
        (status(&conn, &a), status(&conn, &b)),
        (word("cancelled"), word("cancelled"))
    );
}

/// Spec §7, "Cancel checks": a cancel between the entries a walk resolves
/// stops that walk before it records anything, and the job with it: no
/// later directory is walked, and every slot says the job stopped resolving.
#[test]
fn a_job_cancelled_while_a_walk_resolves_records_nothing() {
    let (library, _fixture, a, b) = two_dirs("job-cancelled-resolving");
    let job = test_support::a_planted_job();
    let mut walked = Vec::new();

    let reports = super::run_watched(
        &library,
        &[a.clone(), b.clone()],
        Reread::Dir,
        &crate::LibraryJob::default(),
        &super::Running::new(&job, None),
        |conn, root, watch| {
            walked.push(root.id);
            scan::scan_root_watched(
                conn,
                root,
                &CancelWhen {
                    inner: watch,
                    job: &job,
                    when: |step, done| step == Step::Resolving && done >= 1,
                },
            )
        },
        |conn, fresh, watch| {
            derive::derive_watched(conn, derive::Documents::Reconcile, fresh, watch)
        },
    );

    assert_eq!(
        reports,
        Ok(vec![cancelled_in("resolving"), cancelled_in("resolving")])
    );
    let rows = count(&library.conn(), "SELECT count(*) FROM scan_entry");
    assert_eq!(
        (walked, rows, statuses(&library, &a, &b)),
        (vec![a.id], 0, (word("cancelled"), word("cancelled"))),
        "(the walks entered, rows, statuses)"
    );
}

/// Spec §7, "Pause": every directory of a cancelled job is paused, one whose
/// walk was refused among them, though `refused` stays its word and its slot
/// keeps the refusal; only the directory not finished answers cancelled.
#[test]
fn a_cancelled_job_pauses_a_directory_whose_walk_was_refused() {
    let (library, _fixture, a, b) = two_dirs("job-cancelled-refused");
    refuse_next_walk(&library, &a);

    let reports =
        cancelled_after_the_first_walk(&library, &[a.clone(), b.clone()], &mut Vec::new())
            .expect("the job answers");

    let refusal = reports[0]["error"].as_str().unwrap_or_default();
    assert!(
        refusal.starts_with(&format!(
            "{} scanned empty while the library holds",
            a.path.display()
        )),
        "a's slot keeps its refusal: {}",
        reports[0]
    );
    assert_eq!(reports[1], cancelled_in("listing"), "b's slot");
    assert_eq!(
        (statuses(&library, &a, &b), both_paused(&library, &a, &b)),
        ((word("refused"), word("cancelled")), (true, true)),
        "(statuses, paused)"
    );
}

/// Spec §7, "Pause": a pressed scan, of a directory or of a library, clears
/// the pause of each directory it covers, and of no other.
#[test]
fn a_pressed_scan_clears_the_pause_of_what_it_covers() {
    let (library, _fixture, a, b) = two_dirs("job-pressed-unpauses");
    scan::pause(&library.conn(), &[a.clone(), b.clone()]).expect("pauses both");
    let job = crate::LibraryJob::default();

    run_with(
        &library,
        std::slice::from_ref(&a),
        Reread::Dir,
        &job,
        scan::scan_root,
        derive::derive,
    )
    .expect("the directory's scan");
    let after_the_directory = both_paused(&library, &a, &b);
    run_with(
        &library,
        std::slice::from_ref(&b),
        Reread::Library(b.libraries[0]),
        &job,
        scan::scan_root,
        derive::derive,
    )
    .expect("the library's scan");

    assert_eq!(
        (after_the_directory, both_paused(&library, &a, &b)),
        ((false, true), (false, false)),
        "(after a's scan, after b's library's scan)"
    );
}

/// Spec §7, "Pause": nobody pressed for a scheduled batch, so it clears no
/// pause.
#[test]
fn a_scheduled_batch_leaves_a_pause() {
    let (library, _fixture, a, b) = two_dirs("job-batch-keeps-the-pause");
    scan::pause(&library.conn(), std::slice::from_ref(&a)).expect("pauses a");

    run_with(
        &library,
        &[a.clone(), b.clone()],
        Reread::Batch,
        &crate::LibraryJob::default(),
        scan::scan_root,
        derive::derive,
    )
    .expect("the batch");

    assert_eq!(both_paused(&library, &a, &b), (true, false));
}

/// Spec §3, Goal 3, and §7, "Pause": a directory two libraries share is
/// shown by both while a job holds it, and a cancelled scan of either
/// library pauses it.
#[test]
fn a_cancelled_library_scan_pauses_a_directory_another_library_shares() {
    let (library, _fixture, a, b) = two_dirs("job-cancelled-shared");
    let (scanned, other) = (a.libraries[0], b.libraries[0]);
    library
        .conn()
        .execute(
            "INSERT INTO library_root (library_id, root_id) VALUES (?1, ?2)",
            [scanned, b.id],
        )
        .expect("puts b in a's library too");

    let reports =
        cancelled_after_one_digest(&library, &[a.clone(), b.clone()], Reread::Library(scanned));

    assert_eq!(
        reports,
        Ok(vec![cancelled_in("hashing"), cancelled_in("hashing")])
    );
    assert_eq!(both_paused(&library, &a, &b), (true, true));
    assert_eq!(
        super::held_libraries(&library.conn(), std::slice::from_ref(&b)),
        Ok(vec![(scanned, "a".to_string()), (other, "b".to_string())])
    );
}

/// Spec §3, Goal 3: a job holding a directory nested in another library's
/// directory, which that library reaches, is shown by both libraries.
#[test]
fn held_libraries_names_the_outer_library_that_reaches_a_nested_directory() {
    let fixture = test_support::Fixture::new("held-nested");
    let library = test_support::library();
    let conn = library.conn();
    let outer_dir = fixture.path().join("outer");
    let inner_dir = outer_dir.join("inner");
    test_support::write_pages(&inner_dir.join("Inner").join("Inner - v01"), 2);
    let outer = scan::add_root_in(&conn, &outer_dir, scan::InLibrary::Named("Outer"))
        .expect("adds the outer directory");
    let inner = scan::add_root_in(&conn, &inner_dir, scan::InLibrary::Named("Inner"))
        .expect("adds the nested directory");

    assert_eq!(
        super::held_libraries(&conn, std::slice::from_ref(&inner)),
        Ok(vec![
            (outer.libraries[0], "Outer".to_string()),
            (inner.libraries[0], "Inner".to_string())
        ])
    );
}

/// Spec §7, "Pause": an accepted press clears the pause again once it holds
/// the job lock, so a press whose first clearing was refused while it
/// waited, as a rules save's long write can refuse it, still runs and ends
/// unpaused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pressed_scan_clears_its_pause_again_once_it_holds_the_job_lock() {
    let (library, fixture, a, _b) = two_dirs("job-unpauses-again");
    let state = state_over(library, &fixture);
    scan::pause(&state.library.conn(), std::slice::from_ref(&a)).expect("pauses a");
    let held = test_support::holding_the_job(&state);
    let started = std::time::Instant::now();
    super::reserve(&state, std::slice::from_ref(&a), started, None).expect("reserves");
    let spawned = super::spawn(
        std::sync::Arc::clone(&state),
        vec![a.clone()],
        started,
        Reread::Dir,
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while paused(&state.library.conn(), &a) && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let cleared_before_the_lock = !paused(&state.library.conn(), &a);
    // Stands in for a first clearing the store refused: the lock is still
    // held, so the job has not passed it.
    scan::pause(&state.library.conn(), std::slice::from_ref(&a)).expect("pauses a again");
    drop(held);
    spawned.await.expect("the job ends");

    let conn = state.library.conn();
    assert_eq!(
        (
            cleared_before_the_lock,
            paused(&conn, &a),
            status(&conn, &a)
        ),
        (true, false, word("ok")),
        "(cleared before the lock, paused once it ended, its word)"
    );
}
