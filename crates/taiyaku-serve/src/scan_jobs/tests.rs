use std::cell::Cell;

use serde_json::json;
use taiyaku_library::{Library, derive, scan};

use super::{Reread, run, run_with};
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

    let reports = run(
        &library,
        &[a.clone(), b.clone()],
        Reread::Batch,
        &job,
        &|_, _| {},
    )
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

    let reports = run(
        &library,
        &[a.clone(), b.clone()],
        Reread::Batch,
        &job,
        &|_, _| {},
    )
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

    let reports = run(&library, &asked, Reread::Batch, &job, &|_, _| {})
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

    let why = run(
        &library,
        std::slice::from_ref(&a),
        Reread::Dir,
        &crate::LibraryJob::default(),
        &|_, _| {},
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

/// O51: the job tells its caller that it holds the job lock, then when its
/// derive starts to wait for the lease and when it holds it, which the store,
/// whose write lock another derive may hold, cannot always be told.
#[test]
fn a_scan_says_when_it_waits_for_the_lease_and_when_it_holds_it() {
    let (library, _fixture, store, a) = one_dir_on_disk("job-says-it-waits");
    let elsewhere = Library::open(&store).expect("another connection, as another process");
    let held = elsewhere
        .derive_lease(taiyaku_library::lease::Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    let (told, heard) = std::sync::mpsc::channel::<bool>();

    let job = std::thread::spawn(move || {
        let told = std::sync::Mutex::new(told);
        super::run_watched(
            &library,
            std::slice::from_ref(&a),
            Reread::Dir,
            &crate::LibraryJob::default(),
            &|_, waiting| {
                let _ = told.lock().expect("one teller").send(waiting);
            },
            scan::scan_root,
            derive::derive,
        )
        .map(|reports| reports.len())
    });
    let locked = heard.recv_timeout(std::time::Duration::from_secs(10));
    let first = heard.recv_timeout(std::time::Duration::from_secs(10));
    let still_waiting = !job.is_finished();
    drop(held);
    let second = heard.recv_timeout(std::time::Duration::from_secs(10));

    assert_eq!(
        (locked, first, still_waiting, second),
        (Ok(false), Ok(true), true, Ok(false)),
        "holding the job lock; waiting while the lease is held elsewhere, then holding it"
    );
    assert_eq!(job.join().expect("the scan did not panic"), Ok(1));
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
        move |state, roots, waiting| {
            super::run_watched(
                &state.library,
                roots,
                Reread::Dir,
                &state.library_job,
                waiting,
                scan::scan_root,
                |conn| {
                    let _ = said.send(());
                    let _ = released.recv();
                    derive::derive(conn)
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
        move |state, roots, waiting| {
            super::run_watched(
                &state.library,
                roots,
                Reread::Dir,
                &state.library_job,
                waiting,
                |conn, root| {
                    let _ = walks.send(());
                    let _ = gone_on.recv();
                    scan::scan_root(conn, root)
                },
                derive::derive,
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
