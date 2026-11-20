use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};
use std::time::{Duration, Instant};

use taiyaku_library::fingerprints::{self, Outcome, Row};
use taiyaku_library::progress::Step;
use taiyaku_library::{Library, derive, libraries, matches, scan};
use taiyaku_read::test_support::cbz_pages;

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::priority::tests::{io_class, stat_field};
use super::*;
use crate::scan_jobs::{Job, JobEnded, JobKind};
use crate::test_support::{
    self, Fixture, HeldPass, POLL, SETTLE, digest_at, gate, hooked, one_worker,
};
use crate::{ServeState, fingerprint};

/// A library over `<fixture>/lib` holding each of `files`, a cbz of three
/// pages unlike every other's, walked and derived: the derive takes a
/// volume's digest and no chapter's.
fn scanned(tag: &str, files: &[&str]) -> (Fixture, Arc<Library>, scan::Root) {
    let fixture = Fixture::new(tag);
    let top = fixture.join("lib");
    write_volumes(&top, files, 0);
    let library = test_support::library();
    let root = {
        let conn = library.conn();
        let root = scan::add_root(&conn, &top).expect("adds the directory");
        scan::scan_root(&conn, &root).expect("walks it");
        derive::derive(&conn).expect("derives");
        root
    };
    (fixture, library, root)
}

/// Each of `files` under `top`, a cbz of three pages unlike every other's,
/// numbered on from `from`, so that files written apart differ too.
fn write_volumes(top: &Path, files: &[&str], from: u32) {
    for (n, rel_path) in (from..).zip(files) {
        let path = top.join(rel_path);
        let dir = path.parent().expect("a file under the tree");
        std::fs::create_dir_all(dir).expect("makes its directory");
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a name");
        cbz_pages(dir, name, 3, n * 10);
    }
}

/// A state over `library` whose pass runs as `settings` say.
fn state_of(library: Arc<Library>, fixture: &Fixture, settings: PassSettings) -> Arc<ServeState> {
    Arc::new(ServeState {
        library,
        fingerprints: Arc::new(Pass::with(settings)),
        ..test_support::state(fixture)
    })
}

/// The default settings, polling every [`POLL`].
fn polling() -> PassSettings {
    PassSettings {
        poll: POLL,
        ..PassSettings::default()
    }
}

/// The pass's job, once it shows: once the coordinator has listed.
fn shown(state: &ServeState) -> Arc<Job> {
    let deadline = Instant::now() + SETTLE;
    loop {
        if let Some(job) = state.fingerprints.job() {
            return job;
        }
        assert!(Instant::now() < deadline, "the pass showed no job");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Kick the pass and wait for it to end.
fn kicked(state: &Arc<ServeState>) {
    kick(state);
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
}

/// The row stored for `rel_path`'s digest, if it has a digest and a row.
fn row_of(library: &Library, root: &scan::Root, rel_path: &str) -> Option<Row> {
    let digest = digest_at(library, root.id, rel_path)?;
    fingerprints::read(&library.conn(), &digest).expect("reads the store")
}

/// The outcome stored for `rel_path`, if any.
fn outcome_of(library: &Library, root: &scan::Root, rel_path: &str) -> Option<Outcome> {
    row_of(library, root, rel_path).map(|row| row.outcome)
}

/// The pass's job, which it must have.
fn the_job(state: &ServeState) -> Arc<Job> {
    state.fingerprints.job().expect("the pass showed a job")
}

/// The job's `(done, of)`.
fn counts(job: &Job) -> (Option<u64>, Option<u64>) {
    let snapshot = job.snapshot();
    (snapshot.done, snapshot.of)
}

const VOLUMES: [&str; 3] = [
    "Alpha/Alpha v01.cbz",
    "Beta/Beta v01.cbz",
    "Gamma/Gamma v01.cbz",
];
const CHAPTER: &str = "Kaguya/Kaguya - c001.cbz";

/// The chapter has no digest until the pass takes one, so it is read only if
/// the pass hashes it and records it as the derive would.
#[test]
fn a_pass_fingerprints_every_due_unit_and_ends_counted() {
    let files = [VOLUMES[0], VOLUMES[1], VOLUMES[2], CHAPTER];
    let (fixture, library, root) = scanned("pass-every-unit", &files);
    assert_eq!(
        digest_at(&library, root.id, CHAPTER),
        None,
        "the fixture: no derive hashes a chapter"
    );
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());

    kicked(&state);

    for rel_path in files {
        let row = row_of(&library, &root, rel_path)
            .unwrap_or_else(|| panic!("{rel_path} has a digest and a row"));
        assert_eq!(row.outcome, Outcome::Whole, "{rel_path} read whole");
        assert_eq!(row.pages, 3, "{rel_path}'s pages");
    }
    let hashed = taiyaku_read::member_digest(&root.path.join(CHAPTER)).expect("hashes the chapter");
    assert_eq!(
        digest_at(&library, root.id, CHAPTER),
        Some(hashed),
        "the chapter's own digest"
    );
    let job = the_job(&state);
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&job), (Some(4), Some(4)), "(done, of)");
    assert_eq!(job.snapshot().step, Step::Fingerprinting);
    assert_eq!(
        (job.kind, job.kind.as_str()),
        (JobKind::Fingerprint, "fingerprint")
    );
    assert!(job.dirs.is_empty(), "the pass holds no directory");
    let coverage = fingerprints::coverage(&library.conn()).expect("reads the coverage");
    assert_eq!(
        *job.libraries.lock().expect("the job's libraries"),
        coverage.libraries,
        "the libraries with matching on"
    );
}

#[test]
fn a_pass_with_nothing_due_shows_no_job() {
    let (fixture, library, _root) = scanned("pass-nothing-due", &VOLUMES[..2]);
    {
        let conn = library.conn();
        let due = fingerprints::due(&conn, fingerprint::VERSION, None).expect("lists what is due");
        assert_eq!(due.len(), 2, "the fixture: both volumes are due");
        for unit in due {
            fingerprints::store(
                &conn,
                &Row {
                    sha256: unit.sha256.expect("a volume's digest"),
                    version: fingerprint::VERSION,
                    outcome: Outcome::Whole,
                    reason: None,
                    build: None,
                    pages: 3,
                    prints: vec![0; 3],
                },
            )
            .expect("stores a row");
        }
    }
    let state = state_of(library, &fixture, PassSettings::default());

    kicked(&state);

    assert!(state.fingerprints.job().is_none(), "no job shows");
}

/// One worker, so the first volume is read and stored before the second
/// starts; the cancel lands as the second's first walked page is asked
/// about. The second pass's count is one: a pass that listed the stored
/// volume again would count it, even where it then passed it over.
#[test]
fn a_cancelled_pass_resumes_without_reading_what_it_stored() {
    let (fixture, library, root) = scanned("pass-resumes", &VOLUMES[..2]);
    let order: Vec<String> = fingerprints::due(&library.conn(), fingerprint::VERSION, None)
        .expect("lists what is due")
        .into_iter()
        .map(|unit| unit.rel_path)
        .collect();
    let [first, second] = <[String; 2]>::try_from(order).expect("the fixture: two units due");
    let (first, second) = (first.as_str(), second.as_str());
    let read: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let armed = Arc::new(AtomicBool::new(true));
    let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
    let settings = hooked(
        PassSettings {
            workers: 1,
            ..PassSettings::default()
        },
        {
            let (read, armed, served) =
                (Arc::clone(&read), Arc::clone(&armed), Arc::clone(&served));
            move |path, index| {
                if index != 0 {
                    return;
                }
                let mut read = read.lock().expect("one recorder");
                read.push(path.to_owned());
                if read.len() == 2 && armed.swap(false, SeqCst) {
                    let state = served.get().and_then(Weak::upgrade).expect("the state");
                    the_job(&state).request_cancel();
                }
            }
        },
    );
    let state = state_of(Arc::clone(&library), &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kicked(&state);

    let cancelled = the_job(&state);
    assert_eq!(
        cancelled.ended(),
        Some(JobEnded::Cancelled(Step::Fingerprinting))
    );
    assert_eq!(outcome_of(&library, &root, first), Some(Outcome::Whole));
    assert_eq!(outcome_of(&library, &root, second), None, "not stored");
    assert_eq!(
        *read.lock().expect("one recorder"),
        [root.path.join(first), root.path.join(second)],
        "the files the first pass read"
    );

    read.lock().expect("one recorder").clear();
    kicked(&state);

    let resumed = the_job(&state);
    assert_ne!(resumed.id, cancelled.id, "a pass of its own");
    assert_eq!(resumed.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&resumed), (Some(1), Some(1)), "(done, of)");
    assert_eq!(outcome_of(&library, &root, second), Some(Outcome::Whole));
    assert_eq!(
        *read.lock().expect("one recorder"),
        [root.path.join(second)],
        "the files the second pass read"
    );
}

/// The scheduler's startup scan takes no directory that is Off, so only the
/// pass's own start at startup reads them.
#[test]
fn at_startup_starts_a_pass_with_every_directory_off() {
    let (fixture, library, root) = scanned("pass-at-startup", &VOLUMES[..2]);
    library
        .conn()
        .execute("UPDATE root SET scan_interval_secs = 0", [])
        .expect("turns every directory Off");
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());
    let mut started = Vec::new();
    crate::scheduler::Scheduler::new()
        .tick(&state, "2026-11-10 12:00:00", |roots, _| {
            started.push(roots)
        })
        .expect("the startup tick");
    assert!(started.is_empty(), "the startup scan starts nothing");

    at_startup(&state);
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    for rel_path in &VOLUMES[..2] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

/// The second kick finds nothing due, and still drops the row no entry holds.
#[test]
fn every_pass_ends_by_dropping_rows_no_claim_holds() {
    let (fixture, library, root) = scanned("pass-drops-rows", &VOLUMES[..1]);
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());
    kicked(&state);
    let first = the_job(&state);
    let orphan = "0".repeat(64);
    fingerprints::store(
        &library.conn(),
        &Row {
            sha256: orphan.clone(),
            version: fingerprint::VERSION,
            outcome: Outcome::Whole,
            reason: None,
            build: None,
            pages: 1,
            prints: vec![0],
        },
    )
    .expect("plants a row");

    kicked(&state);

    assert_eq!(the_job(&state).id, first.id, "nothing was due: no new job");
    assert_eq!(
        fingerprints::read(&library.conn(), &orphan).expect("reads the store"),
        None,
        "the row no entry holds is gone"
    );
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        Some(Outcome::Whole),
        "the volume's row stays"
    );
}

/// Review Focus 1. Four workers, so the other volume is read beside the one
/// that panics.
#[test]
fn a_panic_in_one_file_ends_nothing() {
    let (fixture, library, root) = scanned("pass-panics", &VOLUMES[..2]);
    let [panics, reads] = [VOLUMES[0], VOLUMES[1]];
    let settings = hooked(PassSettings::default(), move |path, _| {
        if path.ends_with(panics) {
            panic!("a decoder bug");
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let first = the_job(&state);
    assert_eq!(first.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&first), (Some(2), Some(2)), "(done, of)");
    let panicked = row_of(&library, &root, panics).expect("the file that panicked has a row");
    assert_eq!(panicked.outcome, Outcome::Unreadable);
    assert!(
        panicked
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("a decoder bug")),
        "its reason holds the panic's words: {:?}",
        panicked.reason
    );
    assert_eq!(outcome_of(&library, &root, reads), Some(Outcome::Whole));

    let digest = digest_at(&library, root.id, reads).expect("a volume's digest");
    library
        .conn()
        .execute("DELETE FROM fingerprint WHERE sha256 = ?1", [&digest])
        .expect("makes it due again");
    kicked(&state);

    assert_ne!(the_job(&state).id, first.id, "a later kick starts a pass");
    assert_eq!(outcome_of(&library, &root, reads), Some(Outcome::Whole));
}

/// Read from the worker's own thread, as `priority`'s test reads its own.
#[test]
fn a_worker_runs_niced_and_idle() {
    for tool in ["renice", "ionice"] {
        if std::process::Command::new(tool)
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("NOT MEASURED: a_worker_runs_niced_and_idle (no `{tool}` on PATH)");
            return;
        }
    }
    if stat_field(19) == "19" || io_class() == "idle" {
        eprintln!("NOT MEASURED: a_worker_runs_niced_and_idle (this thread is already lowered)");
        return;
    }
    let (fixture, library, _root) = scanned("pass-niced", &VOLUMES[..1]);
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let settings = hooked(PassSettings::default(), {
        let seen = Arc::clone(&seen);
        move |_, _| {
            seen.lock()
                .expect("one recorder")
                .push((stat_field(19), io_class()));
        }
    });
    let state = state_of(library, &fixture, settings);

    kicked(&state);

    let seen = seen.lock().expect("one recorder");
    assert_eq!(seen.len(), 3, "one reading per walked page");
    for (nice, class) in seen.iter() {
        assert_eq!(
            (nice.as_str(), class.as_str()),
            ("19", "idle"),
            "(niceness, I/O class)"
        );
    }
}

/// The chapter is hashed by the pass, and grows by a byte once hashed, so the
/// digest taken is of bytes the file no longer holds.
#[test]
fn a_file_changed_once_hashed_is_given_no_digest() {
    let (fixture, library, root) = scanned("pass-changed-once-hashed", &[CHAPTER]);
    let due =
        fingerprints::due(&library.conn(), fingerprint::VERSION, None).expect("lists what is due");
    assert_eq!(
        due.iter()
            .map(|unit| unit.rel_path.as_str())
            .collect::<Vec<_>>(),
        [CHAPTER],
        "the fixture: the chapter is due"
    );
    let hashed: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let settings = PassSettings {
        after_hashing: Some(Arc::new({
            let hashed = Arc::clone(&hashed);
            move |path: &Path| {
                hashed.lock().expect("one recorder").push(path.to_owned());
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(path)
                    .and_then(|mut file| file.write_all(b"x"))
                    .expect("grows the file");
            }
        })),
        ..PassSettings::default()
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    assert_eq!(
        *hashed.lock().expect("one recorder"),
        [root.path.join(CHAPTER)],
        "the files the pass hashed"
    );
    assert_eq!(
        digest_at(&library, root.id, CHAPTER),
        None,
        "no digest recorded"
    );
}

/// Review Focus 1, outside any file: the coordinator itself panics once its
/// first listing shows a job, before anything is read.
#[test]
fn a_panic_in_the_coordinator_leaves_a_later_kick_to_start_a_pass() {
    let (fixture, library, root) = scanned("pass-coordinator-panics", &VOLUMES[..1]);
    let armed = Arc::new(AtomicBool::new(true));
    let settings = PassSettings {
        after_listing: Some(Arc::new({
            let armed = Arc::clone(&armed);
            move || {
                if armed.swap(false, SeqCst) {
                    panic!("a coordinator bug");
                }
            }
        })),
        ..PassSettings::default()
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let first = the_job(&state);
    assert_eq!(first.ended(), Some(JobEnded::Finished), "the job is ended");
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        None,
        "nothing was read"
    );

    kicked(&state);

    assert_ne!(the_job(&state).id, first.id, "a later kick starts a pass");
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        Some(Outcome::Whole)
    );
}

/// The coordinator panics once its first listing shows a job, as Cancel is
/// pressed on that job: the job ends cancelled, as a pass that cancels does.
#[test]
fn a_panic_in_the_coordinator_of_a_cancelling_pass_ends_its_job_cancelled() {
    let (fixture, library, _root) = scanned("pass-coordinator-panics-cancelling", &VOLUMES[..1]);
    let served: Served = Arc::default();
    let settings = PassSettings {
        after_listing: Some(Arc::new({
            let served = Arc::clone(&served);
            move || {
                the_job(&served_state(&served)).request_cancel();
                panic!("a coordinator bug");
            }
        })),
        ..PassSettings::default()
    };
    let state = state_of(library, &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kicked(&state);

    assert_eq!(
        the_job(&state).ended(),
        Some(JobEnded::Cancelled(Step::Fingerprinting))
    );
}

/// A mark between two whole milliseconds, so a reading rounded the wrong way
/// shows at the quiet's edge.
#[test]
fn waits_names_the_job_lock_then_a_reader_then_nothing() {
    let pass = Pass::with(PassSettings::default());
    let quiet = Duration::from_secs(30);
    assert_eq!(
        pass.settings.as_ref().map(|settings| settings.quiet),
        Some(quiet),
        "the quiet a reader is given"
    );
    let t = pass.epoch + Duration::from_micros(2_500);
    let ms = Duration::from_millis(1);
    assert_eq!(waits(&pass, false, t), None, "never marked");
    assert_eq!(
        waits(&pass, true, t),
        Some(Wait::JobLock),
        "held, never marked"
    );

    pass.mark_reader(t);

    for at in [t, t + quiet - ms, t + quiet] {
        assert_eq!(
            waits(&pass, true, at),
            Some(Wait::JobLock),
            "held, at {at:?}"
        );
    }
    assert_eq!(waits(&pass, false, t), Some(Wait::Reader), "at the mark");
    assert_eq!(
        waits(&pass, false, t + quiet - ms),
        Some(Wait::Reader),
        "a millisecond short of the quiet"
    );
    assert_eq!(
        waits(&pass, false, t + quiet),
        None,
        "once the quiet has passed"
    );
}

/// The job shows once the pass has listed, so the three polls are counted
/// from a pass with its units in hand.
#[test]
fn no_file_starts_while_the_job_lock_is_held() {
    let (fixture, library, root) = scanned("pass-waits-for-a-job", &VOLUMES[..2]);
    let pages = Arc::new(AtomicUsize::new(0));
    let settings = hooked(polling(), {
        let pages = Arc::clone(&pages);
        move |_, _| {
            pages.fetch_add(1, SeqCst);
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);
    let held = state.library_job.lock().expect("the job lock");

    kick(&state);
    shown(&state);
    std::thread::sleep(POLL * 3);

    assert_eq!(pages.load(SeqCst), 0, "pages read while the lock is held");
    for rel_path in &VOLUMES[..2] {
        assert_eq!(outcome_of(&library, &root, rel_path), None, "{rel_path}");
    }
    drop(held);
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    for rel_path in &VOLUMES[..2] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

/// Each walked page's instant is kept, so a page read inside the quiet shows
/// however late the test looks.
#[test]
fn no_file_starts_within_the_quiet_after_a_page() {
    let (fixture, library, root) = scanned("pass-waits-for-a-reader", &VOLUMES[..2]);
    let read_at: Arc<Mutex<Vec<Instant>>> = Arc::default();
    let settings = hooked(
        PassSettings {
            quiet: Duration::from_millis(300),
            ..polling()
        },
        {
            let read_at = Arc::clone(&read_at);
            move |_, _| read_at.lock().expect("one recorder").push(Instant::now())
        },
    );
    let state = state_of(Arc::clone(&library), &fixture, settings);
    let served = Instant::now();
    state.fingerprints.reader_seen();

    kicked(&state);

    let read_at = read_at.lock().expect("one recorder");
    assert_eq!(read_at.len(), 6, "every page of both volumes was read");
    for (n, at) in read_at.iter().enumerate() {
        assert!(
            *at >= served + Duration::from_millis(200),
            "walked page {n} was read {:?} after a page was served",
            at.duration_since(served)
        );
    }
    for rel_path in &VOLUMES[..2] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

/// Review Focus 5. The worker stops before the volume's last page until the
/// test holds the job lock, so the read ends with the lock held and only the
/// wait stands between it and the store.
#[test]
fn a_row_waits_for_the_job_lock() {
    let (fixture, library, root) = scanned("pass-row-waits", &VOLUMES[..1]);
    let (reached, at_last_page) = mpsc::channel::<()>();
    let (go, gone) = mpsc::channel::<()>();
    let gone = Mutex::new(gone);
    let settings = hooked(
        PassSettings {
            workers: 1,
            ..polling()
        },
        move |_, index| {
            if index == 2 {
                reached.send(()).expect("the test listens");
                gone.lock()
                    .expect("one worker")
                    .recv()
                    .expect("the test holds the lock");
            }
        },
    );
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    at_last_page
        .recv_timeout(SETTLE)
        .expect("the worker reaches the last page");
    let held = state.library_job.lock().expect("the job lock");
    go.send(()).expect("the worker waits for the word");
    std::thread::sleep(POLL * 3);

    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        None,
        "no row while the lock is held"
    );
    drop(held);
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        Some(Outcome::Whole)
    );
}

/// A cancel reaches a worker waiting for the lock, so the pass ends with the
/// lock still held.
#[test]
fn a_cancel_ends_a_pass_that_waits() {
    let (fixture, library, root) = scanned("pass-cancel-waiting", &VOLUMES[..1]);
    let state = state_of(Arc::clone(&library), &fixture, polling());
    let held = state.library_job.lock().expect("the job lock");

    kick(&state);
    let job = shown(&state);
    job.request_cancel();

    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    assert_eq!(job.ended(), Some(JobEnded::Cancelled(Step::Fingerprinting)));
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        None,
        "nothing stored"
    );
    drop(held);
}

/// Through the router, as a reader asks: the misnamed fixture's comic for the
/// page routes, its book for the book's.
#[tokio::test]
async fn each_reading_route_marks_a_reader() {
    let tmp = Fixture::new("pass-reading-routes");
    let (state, book, comic) = test_support::misnamed(&tmp);
    let router = test_support::authenticating(
        crate::routes::build_router(Arc::clone(&state)),
        &state.library,
    );
    let mark = &state.fingerprints.reader;
    let routes = [
        format!("/api/works/{comic}/raw/pages/0"),
        format!("/api/works/{comic}/raw/pages/0/thumb"),
        format!("/api/works/{book}/book"),
        format!("/api/works/{book}/raw/book/images/0"),
        format!("/api/works/{comic}/raw/download"),
        format!("/api/works/{comic}/raw/members/0/download"),
        format!("/opds/v1.2/works/{comic}/raw/pages/0"),
        format!("/opds/v1.2/works/{comic}/raw/cover"),
    ];
    for route in routes {
        mark.store(0, SeqCst);
        let status = test_support::get(&router, &route).await.status();
        assert_ne!(
            mark.load(SeqCst),
            0,
            "{route} answered {status} and marked no reader"
        );
    }

    mark.store(0, SeqCst);
    let status = test_support::get(&router, "/api/library/series")
        .await
        .status();
    assert_eq!(status, axum::http::StatusCode::OK, "the series list");
    assert_eq!(mark.load(SeqCst), 0, "the series list marks no reader");
}

/// One worker, so the second volume is taken once the first is stored, and a
/// reader is served as the first volume's last page is asked about. A worker
/// that waited once, before its first unit, and then read the queue through
/// would start the second volume inside the quiet.
#[test]
fn each_file_waits_out_a_reader_served_during_the_one_before() {
    let (fixture, library, root) = scanned("pass-waits-between-files", &VOLUMES[..2]);
    let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
    let marked: Arc<OnceLock<(PathBuf, Instant)>> = Arc::default();
    let walked: Arc<Mutex<Vec<(PathBuf, Instant)>>> = Arc::default();
    let settings = hooked(
        PassSettings {
            workers: 1,
            quiet: Duration::from_millis(300),
            ..polling()
        },
        {
            let (served, marked, walked) = (
                Arc::clone(&served),
                Arc::clone(&marked),
                Arc::clone(&walked),
            );
            move |path, index| {
                walked
                    .lock()
                    .expect("one recorder")
                    .push((path.to_owned(), Instant::now()));
                if index == 2 && marked.get().is_none() {
                    let state = served.get().and_then(Weak::upgrade).expect("the state");
                    let at = Instant::now();
                    state.fingerprints.reader_seen();
                    marked.set((path.to_owned(), at)).expect("set once");
                }
            }
        },
    );
    let state = state_of(Arc::clone(&library), &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kicked(&state);

    let (first, mark) = marked.get().expect("a reader was served");
    let walked = walked.lock().expect("one recorder");
    let second: Vec<Instant> = walked
        .iter()
        .filter(|(path, _)| path != first)
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(second.len(), 3, "the second volume's walked pages");
    for (n, at) in second.iter().enumerate() {
        assert!(
            *at >= *mark + Duration::from_millis(200),
            "the second volume's page {n} was read {:?} after a reader was served",
            at.duration_since(*mark)
        );
    }
    for rel_path in &VOLUMES[..2] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

/// Review Focus 5, for the digest: the lock is taken as the chapter is hashed,
/// so only the wait stands between the digest and its write.
#[test]
fn a_digest_waits_for_the_job_lock() {
    let (fixture, library, root) = scanned("pass-digest-waits", &[CHAPTER]);
    let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
    let held: Arc<Mutex<Option<test_support::HeldJob>>> = Arc::default();
    let (hashed, has_hashed) = mpsc::channel::<()>();
    let settings = PassSettings {
        after_hashing: Some(Arc::new({
            let (served, held) = (Arc::clone(&served), Arc::clone(&held));
            move |_: &Path| {
                let state = served.get().and_then(Weak::upgrade).expect("the state");
                *held.lock().expect("one holder") = Some(test_support::holding_the_job(&state));
                hashed.send(()).expect("the test listens");
            }
        })),
        ..polling()
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kick(&state);
    has_hashed
        .recv_timeout(SETTLE)
        .expect("the pass hashes the chapter");
    std::thread::sleep(POLL * 3);

    assert_eq!(
        digest_at(&library, root.id, CHAPTER),
        None,
        "no digest while the lock is held"
    );
    let holder = held.lock().expect("one holder").take();
    drop(holder);
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    let digest = taiyaku_read::member_digest(&root.path.join(CHAPTER)).expect("hashes the chapter");
    assert_eq!(digest_at(&library, root.id, CHAPTER), Some(digest));
}

/// A reader is served as the only volume's last page is asked about, with a
/// quiet longer than any wait here: once the volume is stored the worker finds
/// nothing left, and the pass ends without waiting the quiet out.
#[test]
fn a_worker_with_nothing_left_waits_for_nothing() {
    let (fixture, library, root) = scanned("pass-nothing-left", &VOLUMES[..1]);
    let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
    let settings = hooked(
        PassSettings {
            workers: 1,
            quiet: Duration::from_secs(600),
            ..polling()
        },
        {
            let served = Arc::clone(&served);
            move |_, index| {
                if index == 2 {
                    let state = served.get().and_then(Weak::upgrade).expect("the state");
                    state.fingerprints.reader_seen();
                }
            }
        },
    );
    let state = state_of(Arc::clone(&library), &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kicked(&state);

    assert_eq!(the_job(&state).ended(), Some(JobEnded::Finished));
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        Some(Outcome::Whole)
    );
}

/// One worker and two volumes; the cancel lands once the first volume's read
/// has ended in a row, before the row is written, with the lock free.
#[test]
fn a_cancel_keeps_a_read_that_has_ended() {
    let (fixture, library, root) = scanned("pass-cancel-keeps-a-read", &VOLUMES[..2]);
    let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
    let ended: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let walked: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let settings = PassSettings {
        after_read: Some(Arc::new({
            let (served, ended) = (Arc::clone(&served), Arc::clone(&ended));
            move |path: &Path| {
                ended.lock().expect("one recorder").push(path.to_owned());
                let state = served.get().and_then(Weak::upgrade).expect("the state");
                the_job(&state).request_cancel();
            }
        })),
        ..hooked(
            PassSettings {
                workers: 1,
                ..polling()
            },
            {
                let walked = Arc::clone(&walked);
                move |path, index| {
                    if index == 0 {
                        walked.lock().expect("one recorder").push(path.to_owned());
                    }
                }
            },
        )
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kicked(&state);

    assert_eq!(
        the_job(&state).ended(),
        Some(JobEnded::Cancelled(Step::Fingerprinting))
    );
    let ended = ended.lock().expect("one recorder");
    let [first] = ended.as_slice() else {
        panic!("one read ended: {ended:?}");
    };
    assert_eq!(
        *walked.lock().expect("one recorder"),
        std::slice::from_ref(first),
        "the files whose pages were walked"
    );
    for rel_path in &VOLUMES[..2] {
        let stored = (root.path.join(rel_path) == *first).then_some(Outcome::Whole);
        assert_eq!(outcome_of(&library, &root, rel_path), stored, "{rel_path}");
    }
}

/// Each file whose first walked page was asked about, in the order asked.
type Walked = Arc<Mutex<Vec<PathBuf>>>;

/// `settings` recording each file as its first walked page is asked about,
/// then asking `then` with it.
fn recording(
    settings: PassSettings,
    then: impl Fn(&Path) + Send + Sync + 'static,
) -> (PassSettings, Walked) {
    let walked: Walked = Arc::default();
    let settings = hooked(settings, {
        let walked = Arc::clone(&walked);
        move |path, index| {
            if index == 0 {
                walked.lock().expect("one recorder").push(path.to_owned());
                then(path);
            }
        }
    });
    (settings, walked)
}

/// Where a hook finds the state, set once the state is made.
type Served = Arc<OnceLock<Weak<ServeState>>>;

fn served_state(served: &Served) -> Arc<ServeState> {
    served.get().and_then(Weak::upgrade).expect("the state")
}

/// Wait until `holds` answers true, failing as `what` once [`SETTLE`] has
/// passed.
fn until(what: &str, holds: impl Fn() -> bool) {
    let deadline = Instant::now() + SETTLE;
    while !holds() {
        assert!(Instant::now() < deadline, "{what}: not within {SETTLE:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The paths of the units due, in the order the pass lists them.
fn listed_order(library: &Library) -> Vec<String> {
    let build = about::this_build();
    fingerprints::due(&library.conn(), fingerprint::VERSION, build.as_deref())
        .expect("lists what is due")
        .into_iter()
        .map(|unit| unit.rel_path)
        .collect()
}

/// Date `files`' entries a second apart, the first newest, and check that the
/// pass lists them in that order. The listing takes the newest `first_seen`
/// first, and a scan dates each entry to the second it is written, so a scan
/// that crosses a second would list a later file first.
fn listed_in_order(library: &Library, root: &scan::Root, files: &[&str]) {
    {
        let conn = library.conn();
        for (rel_path, second) in files.iter().zip((1..=files.len()).rev()) {
            conn.execute(
                "UPDATE scan_entry SET first_seen = ?3 WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![root.id, rel_path, format!("2026-11-17 10:00:{second:02}")],
            )
            .expect("dates the entry");
        }
    }
    assert_eq!(
        listed_order(library),
        files,
        "the fixture: the listing's order"
    );
}

/// `path` one byte longer, as a file still being copied in grows.
fn grow(path: &Path) {
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(b"x"))
        .expect("grows the file");
}

/// What [`two_libraries`] answers.
struct TwoLibraries {
    fixture: Fixture,
    library: Arc<Library>,
    /// Library `A`, as a job names it.
    a: (i64, String),
    /// Library `B`'s id.
    b: i64,
    /// B's own directory, which A does not hold.
    b_dir: scan::Root,
    /// Each unit's directory and path, in the order the pass lists them: A's
    /// two, B's two, then the one in the directory both libraries hold.
    units: [(scan::Root, &'static str); 5],
}

/// Library `A` with a directory of two volumes, `B` with a directory of two
/// more, and a third directory, of one volume, in both; walked, derived, and
/// dated so that the pass lists A's first and the shared one last.
fn two_libraries(tag: &str) -> TwoLibraries {
    let fixture = Fixture::new(tag);
    let library = test_support::library();
    let dirs: [(&str, &str, &[&'static str]); 3] = [
        ("a", "A", &["Alpha/Alpha v01.cbz", "Beta/Beta v01.cbz"]),
        ("b", "B", &["Gamma/Gamma v01.cbz", "Delta/Delta v01.cbz"]),
        ("ab", "A", &["Epsilon/Epsilon v01.cbz"]),
    ];
    let (units, a, b, b_dir) = {
        let conn = library.conn();
        let mut units = Vec::new();
        let mut roots = Vec::new();
        for (n, (dir, in_library, files)) in (0u32..).zip(dirs) {
            let top = fixture.join(dir);
            write_volumes(&top, files, n * 2);
            let root = scan::add_root_in(&conn, &top, scan::InLibrary::Named(in_library))
                .unwrap_or_else(|e| panic!("adds {dir} to {in_library}: {e}"));
            units.extend(files.iter().map(|rel_path| (root.clone(), *rel_path)));
            roots.push(root);
        }
        let id = |name: &str| {
            libraries::id_of(&conn, name)
                .expect("reads the libraries")
                .unwrap_or_else(|| panic!("library {name} was made"))
        };
        let (a, b) = (id("A"), id("B"));
        test_support::link(&conn, b, roots[2].id);
        for root in &roots {
            scan::scan_root(&conn, root).expect("walks it");
        }
        derive::derive(&conn).expect("derives");
        for ((root, rel_path), second) in units.iter().zip(["05", "04", "03", "02", "01"]) {
            conn.execute(
                "UPDATE scan_entry SET first_seen = ?3 WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![root.id, rel_path, format!("2026-11-17 10:00:{second}")],
            )
            .expect("dates the entry");
        }
        (units, a, b, roots[1].clone())
    };
    let units = <[(scan::Root, &str); 5]>::try_from(units).expect("five units");
    let expected: Vec<String> = units
        .iter()
        .map(|(_, rel_path)| (*rel_path).to_owned())
        .collect();
    assert_eq!(
        listed_order(&library),
        expected,
        "the fixture: the listing's order"
    );
    let coverage = fingerprints::coverage(&library.conn()).expect("reads the coverage");
    let a = (a, "A".to_owned());
    assert_eq!(
        coverage.libraries,
        [a.clone(), (b, "B".to_owned())],
        "the fixture: both libraries match, and no other"
    );
    TwoLibraries {
        fixture,
        library,
        a,
        b,
        b_dir,
        units,
    }
}

/// Run a pass over `two` on one worker, asking `stop` once as A's first
/// volume's first page is asked about, and check that it read and stored every
/// volume but B's two; answer its job.
fn a_pass_stopping_b(two: &TwoLibraries, stop: impl Fn() + Send + Sync + 'static) -> Arc<Job> {
    let armed = Arc::new(AtomicBool::new(true));
    let (settings, walked) = recording(one_worker(), move |_| {
        if armed.swap(false, SeqCst) {
            stop();
        }
    });
    let state = state_of(Arc::clone(&two.library), &two.fixture, settings);

    kicked(&state);

    let [a1, a2, b1, b2, both] = &two.units;
    let path = |(root, rel_path): &(scan::Root, &str)| root.path.join(rel_path);
    assert_eq!(
        *walked.lock().expect("one recorder"),
        [path(a1), path(a2), path(both)],
        "the files read"
    );
    for (root, rel_path) in [a1, a2, both] {
        assert_eq!(
            outcome_of(&two.library, root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
    for (root, rel_path) in [b1, b2] {
        assert_eq!(outcome_of(&two.library, root, rel_path), None, "{rel_path}");
    }
    let job = the_job(&state);
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    job
}

/// The directory both libraries hold is still covered through A.
#[test]
fn turning_a_library_off_stops_its_files_mid_pass() {
    let two = two_libraries("pass-switched-off");
    let (library, b) = (Arc::clone(&two.library), two.b);
    let job = a_pass_stopping_b(&two, move || {
        libraries::set_match_by_content(&library.conn(), b, false).expect("switches B off");
    });

    assert_eq!(
        *job.libraries.lock().expect("the job's libraries"),
        std::slice::from_ref(&two.a),
        "the libraries with matching on"
    );
}

#[test]
fn a_directory_paused_mid_pass_gets_no_new_files() {
    let two = two_libraries("pass-paused");
    let (library, b_dir) = (Arc::clone(&two.library), two.b_dir.clone());
    a_pass_stopping_b(&two, move || {
        scan::pause(&library.conn(), std::slice::from_ref(&b_dir)).expect("pauses B's directory");
    });
}

/// The fixture's one library, switched `on` or off.
fn switch_the_library(library: &Library, on: bool) {
    let conn = library.conn();
    let listed = libraries::list(&conn).expect("reads the libraries");
    let [only] = listed.as_slice() else {
        panic!("the fixture: one library, not {}", listed.len());
    };
    libraries::set_match_by_content(&conn, only.id, on).expect("switches the library");
}

fn library_off(library: &Library, _: &scan::Root) {
    switch_the_library(library, false);
}

fn library_on(library: &Library, _: &scan::Root) {
    switch_the_library(library, true);
}

fn directory_paused(library: &Library, root: &scan::Root) {
    scan::pause(&library.conn(), std::slice::from_ref(root)).expect("pauses the directory");
}

fn directory_unpaused(library: &Library, root: &scan::Root) {
    scan::unpause(&library.conn(), std::slice::from_ref(root)).expect("unpauses the directory");
}

/// Three volumes on two workers, each held at the first page of its first
/// file. `off` lands with both held, and the second worker, let go, passes
/// the third volume over. `on` and a kick follow, and the third volume is read
/// while the first worker is still held, so by this pass and no later one.
fn the_third_volume_is_read_once_covered_again(
    tag: &str,
    off: fn(&Library, &scan::Root),
    on: fn(&Library, &scan::Root),
) {
    let (fixture, library, root) = scanned(tag, &VOLUMES);
    listed_in_order(&library, &root, &VOLUMES);
    let (hold_first, first_held, release_first) = gate();
    let (hold_second, second_held, release_second) = gate();
    let started = AtomicUsize::new(0);
    let settings = hooked(
        PassSettings {
            workers: 2,
            ..polling()
        },
        move |_, index| {
            if index != 0 {
                return;
            }
            match started.fetch_add(1, SeqCst) {
                0 => hold_first(),
                1 => hold_second(),
                _ => {}
            }
        },
    );
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    first_held.recv_timeout(SETTLE).expect("one worker is held");
    second_held
        .recv_timeout(SETTLE)
        .expect("the other worker is held");
    let job = the_job(&state);

    off(&library, &root);
    release_second.send(()).expect("the second worker goes on");
    until("the third volume is passed over", || {
        counts(&job).0 == Some(2)
    });
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[2]),
        None,
        "the fixture: the third volume was not read"
    );

    on(&library, &root);
    kick(&state);
    until("the third volume is read by this pass", || {
        outcome_of(&library, &root, VOLUMES[2]).is_some()
    });
    release_first.send(()).expect("the first worker goes on");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(the_job(&state).id, job.id, "one pass read them all");
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(
        listed_order(&library),
        Vec::<String>::new(),
        "nothing is due"
    );
    assert_eq!(
        counts(&job),
        (Some(4), Some(4)),
        "(done, of): the third volume is queued once more"
    );
}

#[test]
fn a_file_passed_over_while_its_library_is_off_is_read_once_it_is_on_again() {
    the_third_volume_is_read_once_covered_again("pass-over-switch", library_off, library_on);
}

#[test]
fn a_file_passed_over_while_its_directory_is_paused_is_read_once_it_is_unpaused() {
    the_third_volume_is_read_once_covered_again(
        "pass-over-pause",
        directory_paused,
        directory_unpaused,
    );
}

/// A chapter has no digest until the pass takes one, so `off` can land after
/// its check and before its digest is written. One worker is held at the
/// volume, which keeps the pass running while `on` and a kick arrive. The
/// other is held once it has hashed the chapter, so `off` lands with both
/// held, whichever unit is listed first and whichever worker starts first.
/// Were `off` to land before the volume's worker had checked the volume, that
/// worker would pass the volume over and never reach its hold. A worker held
/// in its first unit takes no second, so the two workers take one each.
fn a_chapter_losing_coverage_as_it_is_hashed_is_read_once_covered_again(
    tag: &str,
    off: fn(&Library, &scan::Root),
    on: fn(&Library, &scan::Root),
) {
    let (fixture, library, root) = scanned(tag, &[VOLUMES[0], CHAPTER]);
    let (hold, held, release) = gate();
    let (hold_hashed, hashed, release_hashed) = gate();
    let volume = root.path.join(VOLUMES[0]);
    let armed = AtomicBool::new(true);
    let settings = PassSettings {
        after_hashing: Some(Arc::new(move |_| {
            if armed.swap(false, SeqCst) {
                hold_hashed();
            }
        })),
        ..hooked(
            PassSettings {
                workers: 2,
                ..polling()
            },
            move |path, index| {
                if index == 0 && path == volume {
                    hold();
                }
            },
        )
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    held.recv_timeout(SETTLE)
        .expect("a worker holds the volume");
    hashed
        .recv_timeout(SETTLE)
        .expect("the other worker has hashed the chapter");
    let job = the_job(&state);
    off(&library, &root);
    release_hashed
        .send(())
        .expect("the chapter's worker goes on");
    until("the chapter is passed over", || counts(&job).0 == Some(1));
    assert_eq!(
        outcome_of(&library, &root, CHAPTER),
        None,
        "the fixture: the chapter was not read"
    );

    on(&library, &root);
    kick(&state);
    until("the chapter is read by this pass", || {
        outcome_of(&library, &root, CHAPTER).is_some()
    });
    release.send(()).expect("the volume's worker goes on");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(
        listed_order(&library),
        Vec::<String>::new(),
        "nothing is due"
    );
}

/// The pause refuses the digest the pass has just taken.
#[test]
fn a_chapter_refused_its_digest_by_a_pause_is_read_once_it_is_unpaused() {
    a_chapter_losing_coverage_as_it_is_hashed_is_read_once_covered_again(
        "pass-over-digest-refused",
        directory_paused,
        directory_unpaused,
    );
}

/// The digest is written, and the check that follows it finds the library off.
#[test]
fn a_chapter_whose_library_goes_off_as_it_is_hashed_is_read_once_it_is_on_again() {
    a_chapter_losing_coverage_as_it_is_hashed_is_read_once_covered_again(
        "pass-over-digest-unticked",
        library_off,
        library_on,
    );
}

/// A pass with a worker that has found the third volume not due and has not
/// yet said so, coverage back, and a kick's listing done, which could not queue
/// that volume: the new volume it queued shows that it ran.
struct BehindAListing {
    /// Holds the library's files for as long as the pass reads them.
    _fixture: Fixture,
    library: Arc<Library>,
    root: scan::Root,
    state: Arc<ServeState>,
    job: Arc<Job>,
    /// Lets the worker held first go on.
    release_first: mpsc::Sender<()>,
    /// Lets the worker that passed the third volume over say so.
    release_over: mpsc::Sender<()>,
    /// The volume the kick's listing queued.
    added: &'static str,
}

fn behind_a_kicks_listing(tag: &str) -> BehindAListing {
    behind_a_kicks_listing_with(tag, |_, _| {}, |_| {})
}

/// As [`behind_a_kicks_listing`], asking `listed_again` with the number of each
/// listing `relist_at_kicks` makes, the kick's being the first and the closing
/// listing the second, and `passed_again` at every pass-over after the one held.
fn behind_a_kicks_listing_with(
    tag: &str,
    listed_again: fn(&Library, usize),
    passed_again: fn(&Library),
) -> BehindAListing {
    let (fixture, library, root) = scanned(tag, &VOLUMES);
    listed_in_order(&library, &root, &VOLUMES);
    let (hold_first, first_held, release_first) = gate();
    let (hold_second, second_held, release_second) = gate();
    let (hold_over, over_held, release_over) = gate();
    let started = AtomicUsize::new(0);
    let armed = AtomicBool::new(true);
    let listings = AtomicUsize::new(0);
    let settings = PassSettings {
        after_passing_over: Some(Arc::new({
            let library = Arc::clone(&library);
            move |_| {
                if armed.swap(false, SeqCst) {
                    hold_over();
                } else {
                    passed_again(&library);
                }
            }
        })),
        after_relisting: Some(Arc::new({
            let library = Arc::clone(&library);
            move || listed_again(&library, listings.fetch_add(1, SeqCst) + 1)
        })),
        ..hooked(
            PassSettings {
                workers: 2,
                ..polling()
            },
            move |_, index| {
                if index != 0 {
                    return;
                }
                match started.fetch_add(1, SeqCst) {
                    0 => hold_first(),
                    1 => hold_second(),
                    _ => {}
                }
            },
        )
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    first_held.recv_timeout(SETTLE).expect("one worker is held");
    second_held
        .recv_timeout(SETTLE)
        .expect("the other worker is held");
    let job = the_job(&state);

    switch_the_library(&library, false);
    release_second.send(()).expect("the second worker goes on");
    over_held
        .recv_timeout(SETTLE)
        .expect("the second worker finds the third volume not due");

    switch_the_library(&library, true);
    let added = "Delta/Delta v01.cbz";
    write_volumes(&root.path, &[added], 3);
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks the new volume");
        derive::derive(&conn).expect("derives");
    }
    kick(&state);
    until("the kick's listing queues the new volume", || {
        counts(&job).1 == Some(4)
    });
    BehindAListing {
        _fixture: fixture,
        library,
        root,
        state,
        job,
        release_first,
        release_over,
        added,
    }
}

#[test]
fn a_file_passed_over_behind_a_kicks_listing_is_read_before_the_pass_ends() {
    let behind = behind_a_kicks_listing("pass-over-behind-a-listing");
    let (library, root, job) = (&behind.library, &behind.root, &behind.job);

    behind.release_over.send(()).expect("the worker goes on");
    until("the new volume is read", || {
        outcome_of(library, root, behind.added).is_some()
    });
    behind.release_first.send(()).expect("the worker goes on");
    assert!(behind.state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(the_job(&behind.state).id, job.id, "one pass read them all");
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(
        outcome_of(library, root, VOLUMES[2]),
        Some(Outcome::Whole),
        "the third volume"
    );
    assert_eq!(
        listed_order(library),
        Vec::<String>::new(),
        "nothing is due"
    );
}

/// A cancel stops what was asked of the pass, so the unit passed over behind
/// the listing waits for the next pass with the rest.
#[test]
fn a_cancelled_pass_lists_nothing_for_a_file_passed_over_behind_a_kicks_listing() {
    let behind = behind_a_kicks_listing("pass-over-behind-a-cancel");
    let (library, root, job) = (&behind.library, &behind.root, &behind.job);

    behind.state.fingerprints.cancel(job);
    behind.release_over.send(()).expect("the worker goes on");
    behind.release_first.send(()).expect("the worker goes on");
    assert!(behind.state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(job.ended(), Some(JobEnded::Cancelled(Step::Fingerprinting)));
    assert_eq!(
        counts(job).1,
        Some(4),
        "of: nothing is queued after the cancel"
    );
    assert_eq!(outcome_of(library, root, VOLUMES[2]), None, "not read");
}

/// The closing listing queues the third volume, the library goes off before a
/// worker looks at it, and is back on as the worker passes it over again.
/// Nothing has kicked since, so nothing lists it a third time.
#[test]
fn a_kick_pays_for_one_closing_listing() {
    let behind = behind_a_kicks_listing_with(
        "pass-over-one-closing-listing",
        |library, listing| {
            if listing == 2 {
                switch_the_library(library, false);
            }
        },
        |library| switch_the_library(library, true),
    );
    let (library, root, job) = (&behind.library, &behind.root, &behind.job);

    behind.release_over.send(()).expect("the worker goes on");
    until("the new volume is read", || {
        outcome_of(library, root, behind.added).is_some()
    });
    behind.release_first.send(()).expect("the worker goes on");
    assert!(behind.state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(outcome_of(library, root, VOLUMES[2]), None, "not read");
    assert_eq!(
        listed_order(library),
        [VOLUMES[2]],
        "left for the next pass"
    );
    assert_eq!(counts(job), (Some(5), Some(5)), "(done, of)");
}

/// Kick the held pass `times` times, each kick's listing made before the next
/// kick, so that no two kicks are read as one.
fn kicked_again(state: &Arc<ServeState>, listings: &AtomicUsize, times: usize) {
    for _ in 0..times {
        let before = listings.load(SeqCst);
        kick(state);
        until("the kick's listing is made", || {
            listings.load(SeqCst) > before
        });
    }
}

/// A volume held at its first page keeps the pass running while a chapter's
/// attempt ends with nothing stored, as `arrange` makes it, and three kicks
/// follow. The attempt was made, so no listing queues the chapter again.
fn the_chapter_is_queued_once_however_often_the_pass_is_kicked(
    tag: &str,
    arrange: impl FnOnce(&Library, &Path, PassSettings) -> PassSettings,
) {
    let (fixture, library, root) = scanned(tag, &[VOLUMES[0], CHAPTER]);
    let (hold, held, release) = gate();
    let volume = root.path.join(VOLUMES[0]);
    let listings = Arc::new(AtomicUsize::new(0));
    let settings = PassSettings {
        after_relisting: Some(Arc::new({
            let listings = Arc::clone(&listings);
            move || {
                listings.fetch_add(1, SeqCst);
            }
        })),
        ..hooked(
            PassSettings {
                workers: 2,
                ..polling()
            },
            move |path, index| {
                if index == 0 && path == volume {
                    hold();
                }
            },
        )
    };
    let settings = arrange(&library, &root.path.join(CHAPTER), settings);
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    held.recv_timeout(SETTLE)
        .expect("a worker holds the volume");
    let job = the_job(&state);
    until("the chapter's attempt ends", || counts(&job).0 == Some(1));
    kicked_again(&state, &listings, 3);
    release.send(()).expect("the volume's worker goes on");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&job), (Some(2), Some(2)), "(done, of): queued once");
    assert_eq!(listed_order(&library), [CHAPTER], "the chapter stays due");
}

/// The file goes between the check of its stamp and its hash, which no mode
/// bits could do for a test run as root.
#[test]
fn a_file_that_fails_to_hash_is_queued_once_however_often_the_pass_is_kicked() {
    the_chapter_is_queued_once_however_often_the_pass_is_kicked(
        "tried-once-hash-fails",
        |_, _, settings| PassSettings {
            before_hashing: Some(Arc::new(|path: &Path| {
                std::fs::remove_file(path).expect("removes the file");
            })),
            ..settings
        },
    );
}

#[test]
fn a_file_changed_before_its_hash_is_queued_once_however_often_the_pass_is_kicked() {
    the_chapter_is_queued_once_however_often_the_pass_is_kicked(
        "tried-once-changed-before",
        |_, chapter, settings| {
            grow(chapter);
            settings
        },
    );
}

#[test]
fn a_file_changed_as_it_is_hashed_is_queued_once_however_often_the_pass_is_kicked() {
    the_chapter_is_queued_once_however_often_the_pass_is_kicked(
        "tried-once-changed-during",
        |_, _, settings| PassSettings {
            after_hashing: Some(Arc::new(|path: &Path| grow(path))),
            ..settings
        },
    );
}

#[test]
fn a_file_whose_row_is_refused_is_queued_once_however_often_the_pass_is_kicked() {
    the_chapter_is_queued_once_however_often_the_pass_is_kicked(
        "tried-once-row-refused",
        |library, chapter, settings| {
            let refused = taiyaku_read::member_digest(chapter).expect("hashes the chapter");
            library
                .conn()
                .execute_batch(&format!(
                    "CREATE TEMP TRIGGER refuse_one BEFORE INSERT ON fingerprint
                      WHEN NEW.sha256 = '{refused}' BEGIN SELECT RAISE(ABORT, 'refused'); END"
                ))
                .expect("plants the refusal");
            settings
        },
    );
}

/// The chapter is passed over once, then queued again and attempted: its stamp
/// moved with no scan since. Being passed over earlier buys it no second
/// attempt at the kicks that follow.
#[test]
fn a_file_passed_over_then_attempted_is_queued_no_more() {
    let files = [VOLUMES[0], VOLUMES[1], CHAPTER];
    let (fixture, library, root) = scanned("pass-over-then-tried", &files);
    listed_in_order(&library, &root, &files);
    grow(&root.path.join(CHAPTER));
    let (hold_first, first_held, release_first) = gate();
    let (hold_second, second_held, release_second) = gate();
    let started = AtomicUsize::new(0);
    let listings = Arc::new(AtomicUsize::new(0));
    let settings = PassSettings {
        after_relisting: Some(Arc::new({
            let listings = Arc::clone(&listings);
            move || {
                listings.fetch_add(1, SeqCst);
            }
        })),
        ..hooked(
            PassSettings {
                workers: 2,
                ..polling()
            },
            move |_, index| {
                if index != 0 {
                    return;
                }
                match started.fetch_add(1, SeqCst) {
                    0 => hold_first(),
                    1 => hold_second(),
                    _ => {}
                }
            },
        )
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    first_held.recv_timeout(SETTLE).expect("one worker is held");
    second_held
        .recv_timeout(SETTLE)
        .expect("the other worker is held");
    let job = the_job(&state);
    switch_the_library(&library, false);
    release_second.send(()).expect("the second worker goes on");
    until("the chapter is passed over", || counts(&job).0 == Some(2));

    switch_the_library(&library, true);
    kicked_again(&state, &listings, 1);
    until("the chapter's attempt ends", || {
        counts(&job) == (Some(3), Some(4))
    });
    kicked_again(&state, &listings, 3);
    release_first.send(()).expect("the first worker goes on");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(
        counts(&job),
        (Some(4), Some(4)),
        "(done, of): queued again once, after the pass-over"
    );
    assert_eq!(listed_order(&library), [CHAPTER], "the chapter stays due");
}

/// The library goes off once the pass has listed and is back on as the last
/// file is passed over, and nothing kicks: every listing answers a kick, so
/// nothing is queued twice and the pass ends with its files left for the next.
#[test]
fn a_file_passed_over_is_queued_again_only_at_a_kick() {
    let (fixture, library, _root) = scanned("pass-over-no-kick", &VOLUMES);
    let (settings, walked) = recording(one_worker(), |_| {});
    let armed = AtomicBool::new(true);
    let passed = AtomicUsize::new(0);
    let settings = PassSettings {
        after_listing: Some(Arc::new({
            let library = Arc::clone(&library);
            move || {
                if armed.swap(false, SeqCst) {
                    switch_the_library(&library, false);
                }
            }
        })),
        after_passing_over: Some(Arc::new({
            let library = Arc::clone(&library);
            move |_| {
                if passed.fetch_add(1, SeqCst) + 1 == VOLUMES.len() {
                    switch_the_library(&library, true);
                }
            }
        })),
        ..settings
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let job = the_job(&state);
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&job), (Some(3), Some(3)), "(done, of): queued once");
    assert!(
        walked.lock().expect("one recorder").is_empty(),
        "nothing was read"
    );
    assert_eq!(listed_order(&library).len(), 3, "all three stay due");

    kicked(&state);

    assert_ne!(the_job(&state).id, job.id, "a pass of its own");
    assert_eq!(
        listed_order(&library),
        Vec::<String>::new(),
        "nothing is due"
    );
}

/// One worker, held at the first volume's first page while a scan adds a
/// volume and kicks, as a scan's end does; the job counts the new volume
/// before the worker goes on.
#[test]
fn a_scan_ending_mid_pass_puts_its_new_files_first() {
    let (fixture, library, root) = scanned("pass-new-files-first", &VOLUMES);
    let order = listed_order(&library);
    let (hold, arrived, release) = gate();
    let armed = Arc::new(AtomicBool::new(true));
    let (settings, walked) = recording(one_worker(), move |_| {
        if armed.swap(false, SeqCst) {
            hold();
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    arrived
        .recv_timeout(SETTLE)
        .expect("the worker reaches the first volume");
    let added = "Delta/Delta v01.cbz";
    write_volumes(&root.path, &[added], 3);
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks the new volume");
        derive::derive(&conn).expect("derives");
    }
    kick(&state);
    let job = the_job(&state);
    until("the job counts the new volume", || {
        counts(&job).1 == Some(4)
    });
    release.send(()).expect("the worker waits for the word");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    let expected = [
        order[0].as_str(),
        added,
        order[1].as_str(),
        order[2].as_str(),
    ];
    let walked = walked.lock().expect("one recorder");
    assert_eq!(walked.len(), expected.len(), "the files read: {walked:?}");
    for (n, rel_path) in expected.iter().enumerate() {
        assert_eq!(walked[n], root.path.join(rel_path), "read {n}");
    }
    for rel_path in expected {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&job), (Some(4), Some(4)), "(done, of)");
}

/// Two directories, each in a library of its own, hold the same bytes, so the
/// derive gives both entries one digest.
#[test]
fn twins_are_read_once() {
    let fixture = Fixture::new("pass-twins");
    let (one, two) = (fixture.join("one"), fixture.join("two"));
    write_volumes(&one, &VOLUMES[..1], 0);
    std::fs::create_dir_all(two.join("Alpha")).expect("makes its directory");
    std::fs::copy(one.join(VOLUMES[0]), two.join(VOLUMES[0])).expect("copies the volume");
    let library = test_support::library();
    let roots = {
        let conn = library.conn();
        let roots = [&one, &two].map(|dir| {
            let root = scan::add_root(&conn, dir).expect("adds the directory");
            scan::scan_root(&conn, &root).expect("walks it");
            root
        });
        derive::derive(&conn).expect("derives");
        roots
    };
    let digests = roots
        .each_ref()
        .map(|root| digest_at(&library, root.id, VOLUMES[0]));
    assert!(
        digests[0].is_some() && digests[0] == digests[1],
        "the fixture: one digest for both: {digests:?}"
    );
    let (settings, walked) = recording(one_worker(), |_| {});
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let walked = walked.lock().expect("one recorder");
    assert_eq!(walked.len(), 1, "the files read: {walked:?}");
    assert!(
        roots
            .iter()
            .any(|root| walked[0] == root.path.join(VOLUMES[0])),
        "a twin was read: {walked:?}"
    );
    let shared = digests[0].as_deref().expect("a digest");
    let row = fingerprints::read(&library.conn(), shared)
        .expect("reads the store")
        .expect("the shared digest has a row");
    assert_eq!(row.outcome, Outcome::Whole);
}

/// Review Focus 4. One worker. Two volumes change as each read of them walks
/// its first page, and the pass is kicked then, as a scan's end would: an
/// archive grows by a byte, as a file still being copied in does, and a
/// folder's first page grows in place, which moves no stamp of the folder's.
/// Nothing walks either again, so each keeps the stamp it was listed with.
#[test]
fn a_file_that_changes_on_every_read_is_tried_once_per_pass() {
    let (fixture, library, root) = scanned("pass-changes-every-read", &VOLUMES);
    let folder = "Delta/Delta v01";
    test_support::write_pages(&root.path.join(folder), 3);
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks the folder");
        derive::derive(&conn).expect("derives");
    }
    let changing = [VOLUMES[0], folder];
    assert!(
        listed_order(&library)
            .iter()
            .any(|rel_path| rel_path == folder),
        "the fixture: the folder is due"
    );
    let served: Served = Arc::default();
    let (settings, walked) = recording(one_worker(), {
        let served = Arc::clone(&served);
        let (archive, folder) = (root.path.join(VOLUMES[0]), root.path.join(folder));
        move |path| {
            if path == archive.as_path() {
                grow(path);
            } else if path == folder.as_path() {
                grow(&folder.join("000.jpg"));
            } else {
                return;
            }
            kick(&served_state(&served));
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kick(&state);
    assert!(
        state.fingerprints.settle(Duration::from_secs(5)),
        "the pass ends"
    );

    let walked = walked.lock().expect("one recorder");
    for rel_path in changing {
        let path = root.path.join(rel_path);
        assert_eq!(
            walked.iter().filter(|read| **read == path).count(),
            1,
            "reads of {rel_path}: {walked:?}"
        );
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            None,
            "{rel_path} has no row"
        );
    }
    for rel_path in &VOLUMES[1..] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
    assert_eq!(
        counts(&the_job(&state)),
        (Some(4), Some(4)),
        "(done, of): each unit taken once"
    );
}

/// Review Focus 5. One worker, and the refused volume is listed first, so a
/// refusal that ended the worker would leave the other two unread.
#[test]
fn a_refused_write_leaves_its_file_due_and_the_pass_goes_on() {
    let (fixture, library, root) = scanned("pass-refused-write", &VOLUMES);
    let order = listed_order(&library);
    let refused = digest_at(&library, root.id, &order[0]).expect("a volume's digest");
    library
        .conn()
        .execute_batch(&format!(
            "CREATE TEMP TRIGGER refuse_one BEFORE INSERT ON fingerprint
              WHEN NEW.sha256 = '{refused}' BEGIN SELECT RAISE(ABORT, 'refused'); END"
        ))
        .expect("plants the refusal");
    let state = state_of(Arc::clone(&library), &fixture, one_worker());

    kicked(&state);

    assert_eq!(the_job(&state).ended(), Some(JobEnded::Finished));
    assert_eq!(
        outcome_of(&library, &root, &order[0]),
        None,
        "the refused volume has no row"
    );
    assert_eq!(
        listed_order(&library),
        [order[0].clone()],
        "the refused volume alone is still due"
    );
    for rel_path in &order[1..] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

#[test]
fn four_workers_store_every_unit_once() {
    let names = [
        "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta",
    ]
    .map(|series| format!("{series}/{series} v01.cbz"));
    let files: Vec<&str> = names.iter().map(String::as_str).collect();
    let (fixture, library, root) = scanned("pass-four-workers", &files);
    let (settings, walked) = recording(PassSettings::default(), |_| {});
    assert_eq!(settings.workers, 4, "the default workers");
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    for rel_path in &files {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
    let job = the_job(&state);
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&job), (Some(8), Some(8)), "(done, of)");
    let mut walked = walked.lock().expect("one recorder").clone();
    walked.sort();
    let mut expected: Vec<PathBuf> = files
        .iter()
        .map(|rel_path| root.path.join(rel_path))
        .collect();
    expected.sort();
    assert_eq!(walked, expected, "each file read once");
}

/// One worker, held at the first volume's first page while the test cancels
/// the pass and kicks it: a cancel stops the pass it lands on and no later
/// one, so the kick starts a pass that reads the volume the cancel stopped.
#[test]
fn a_kick_as_a_cancelled_pass_winds_down_starts_a_fresh_pass() {
    let (fixture, library, root) = scanned("pass-kick-after-cancel", &VOLUMES[..2]);
    let order = listed_order(&library);
    let (hold, arrived, release) = gate();
    let armed = Arc::new(AtomicBool::new(true));
    let (settings, walked) = recording(one_worker(), move |_| {
        if armed.swap(false, SeqCst) {
            hold();
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    arrived
        .recv_timeout(SETTLE)
        .expect("the worker reaches the first volume");
    let cancelled = the_job(&state);
    cancelled.request_cancel();
    kick(&state);
    release.send(()).expect("the worker waits for the word");
    assert!(state.fingerprints.settle(SETTLE), "the passes end");

    assert_eq!(
        cancelled.ended(),
        Some(JobEnded::Cancelled(Step::Fingerprinting))
    );
    let fresh = the_job(&state);
    assert_ne!(fresh.id, cancelled.id, "a pass of its own");
    assert_eq!(fresh.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&fresh), (Some(2), Some(2)), "(done, of)");
    let [first, second] = [&order[0], &order[1]].map(|rel_path| root.path.join(rel_path));
    assert_eq!(
        *walked.lock().expect("one recorder"),
        [first.clone(), first, second],
        "the files read"
    );
    for rel_path in &order {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

/// One worker. As the first volume's first page is asked about, a scan finds
/// it replaced by a volume of four pages, derives and kicks; the read under
/// way stores nothing, and the replacement is read in the same pass.
#[test]
fn a_file_a_scan_walks_again_mid_pass_is_read_in_that_pass() {
    let (fixture, library, root) = scanned("pass-walked-again", &VOLUMES[..2]);
    let order = listed_order(&library);
    let (replaced, other) = (root.path.join(&order[0]), root.path.join(&order[1]));
    let served: Served = Arc::default();
    let armed = Arc::new(AtomicBool::new(true));
    let (settings, walked) = recording(one_worker(), {
        let (served, library, root, replaced) = (
            Arc::clone(&served),
            Arc::clone(&library),
            root.clone(),
            replaced.clone(),
        );
        move |path| {
            if path != replaced.as_path() || !armed.swap(false, SeqCst) {
                return;
            }
            let dir = path.parent().expect("a volume's directory");
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a name");
            cbz_pages(dir, name, 4, 90);
            {
                let conn = library.conn();
                scan::scan_root(&conn, &root).expect("walks it again");
                derive::derive(&conn).expect("derives");
            }
            kick(&served_state(&served));
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);
    served.set(Arc::downgrade(&state)).expect("set once");

    kicked(&state);

    let walked = walked.lock().expect("one recorder");
    assert_eq!(walked.first(), Some(&replaced), "read first: {walked:?}");
    assert_eq!(
        walked.iter().filter(|path| **path == replaced).count(),
        2,
        "reads of the replaced volume: {walked:?}"
    );
    assert_eq!(
        walked.iter().filter(|path| **path == other).count(),
        1,
        "reads of the other: {walked:?}"
    );
    let row = row_of(&library, &root, &order[0]).expect("the replacement's digest has a row");
    assert_eq!(
        (row.outcome, row.pages),
        (Outcome::Whole, 4),
        "the replacement, read whole"
    );
    assert_eq!(outcome_of(&library, &root, &order[1]), Some(Outcome::Whole));
    let job = the_job(&state);
    assert_eq!(job.ended(), Some(JobEnded::Finished));
    assert_eq!(counts(&job), (Some(3), Some(3)), "(done, of)");
}

/// The volume grows by a byte as its first walked page is asked about, then
/// its read panics: the panic may come of bytes its digest does not name.
#[test]
fn a_panic_on_a_file_that_changed_stores_nothing() {
    let (fixture, library, root) = scanned("pass-panic-after-change", &VOLUMES[..1]);
    let settings = hooked(PassSettings::default(), |path, index| {
        if index == 0 {
            grow(path);
            panic!("a decoder bug");
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    assert_eq!(the_job(&state).ended(), Some(JobEnded::Finished));
    assert_eq!(
        outcome_of(&library, &root, VOLUMES[0]),
        None,
        "nothing stored"
    );
    assert_eq!(listed_order(&library), [VOLUMES[0]], "still due");
}

/// Four chapters, which no derive hashes, on the default four workers, with
/// every file large: each hash holds its hook a while, so two hashes at once
/// would meet there.
#[test]
fn a_large_file_is_hashed_alone() {
    let chapters = [
        "Kaguya/Kaguya - c001.cbz",
        "Mob/Mob - c001.cbz",
        "Nana/Nana - c001.cbz",
        "Orb/Orb - c001.cbz",
    ];
    let (fixture, library, root) = scanned("pass-hashes-alone", &chapters);
    for rel_path in chapters {
        assert_eq!(
            digest_at(&library, root.id, rel_path),
            None,
            "the fixture: {rel_path} has no digest"
        );
    }
    let inside = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let hashed = Arc::new(AtomicUsize::new(0));
    let settings = PassSettings {
        large_page_bytes: 1,
        after_hashing: Some(Arc::new({
            let (inside, most, hashed) =
                (Arc::clone(&inside), Arc::clone(&most), Arc::clone(&hashed));
            move |_: &Path| {
                let now = inside.fetch_add(1, SeqCst) + 1;
                most.fetch_max(now, SeqCst);
                std::thread::sleep(Duration::from_millis(100));
                inside.fetch_sub(1, SeqCst);
                hashed.fetch_add(1, SeqCst);
            }
        })),
        ..PassSettings::default()
    };
    assert_eq!(settings.workers, 4, "the default workers");
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    assert_eq!(hashed.load(SeqCst), 4, "the files hashed");
    assert_eq!(most.load(SeqCst), 1, "the hashes under way at once");
    for rel_path in chapters {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

/// Nothing is due on the second kick, so the drop that ends every pass is its
/// only write.
#[test]
fn the_closing_drop_waits_for_the_job_lock() {
    let (fixture, library, _root) = scanned("pass-drop-waits", &VOLUMES[..1]);
    let state = state_of(Arc::clone(&library), &fixture, polling());
    kicked(&state);
    let orphan = "0".repeat(64);
    fingerprints::store(
        &library.conn(),
        &Row {
            sha256: orphan.clone(),
            version: fingerprint::VERSION,
            outcome: Outcome::Whole,
            reason: None,
            build: None,
            pages: 1,
            prints: vec![0],
        },
    )
    .expect("plants a row");
    let held = state.library_job.lock().expect("the job lock");

    kick(&state);

    assert!(
        !state.fingerprints.settle(Duration::from_secs(2)),
        "the pass waits for the lock"
    );
    assert!(
        fingerprints::read(&library.conn(), &orphan)
            .expect("reads the store")
            .is_some(),
        "the row no entry holds stays while the lock is held"
    );
    drop(held);
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    assert_eq!(
        fingerprints::read(&library.conn(), &orphan).expect("reads the store"),
        None,
        "dropped once the lock is free"
    );
}

/// Two workers and three chapters, which no derive hashes, listed in this
/// order: an archive at the large-page size, whose hash holds the lock in its
/// hook until the test lets it go; a small archive, whose hook waits until the
/// lock is held; and a folder, whose recorded size is its directory's, below
/// the large-page size. So the folder is taken once the small archive is
/// stored, with the lock held.
#[test]
fn a_folder_is_hashed_alone_whatever_its_size() {
    let small = "Kaguya/Kaguya - c001.cbz";
    let (fixture, library, root) = scanned("pass-folder-hashes-alone", &[small]);
    let (big, folder) = ("Big/Big - c001.cbz", "Nana/Nana - c001");
    let big_dir = root.path.join("Big");
    std::fs::create_dir_all(&big_dir).expect("makes its directory");
    cbz_pages(&big_dir, "Big - c001.cbz", 200, 50);
    test_support::write_pages(&root.path.join(folder), 3);
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks the new chapters");
        derive::derive(&conn).expect("derives");
        for (rel_path, second) in [(big, "03"), (small, "02"), (folder, "01")] {
            conn.execute(
                "UPDATE scan_entry SET first_seen = ?3 WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![root.id, rel_path, format!("2026-11-17 10:00:{second}")],
            )
            .expect("dates the entry");
        }
    }
    let due =
        fingerprints::due(&library.conn(), fingerprint::VERSION, None).expect("lists what is due");
    assert_eq!(
        due.iter()
            .map(|unit| unit.rel_path.as_str())
            .collect::<Vec<_>>(),
        [big, small, folder],
        "the fixture: the listing's order"
    );
    for unit in &due {
        assert_eq!(
            unit.sha256, None,
            "the fixture: {} has no digest",
            unit.rel_path
        );
    }
    let size_of = |n: usize| usize::try_from(due[n].size).expect("a size");
    let threshold = size_of(0);
    assert!(
        size_of(1) < threshold && size_of(2) < threshold,
        "the fixture: the small archive and the folder are recorded below the large-page size"
    );
    let (hold, arrived, release) = gate();
    let holding = Arc::new(AtomicBool::new(false));
    let hashed: Walked = Arc::default();
    let settings = PassSettings {
        workers: 2,
        large_page_bytes: threshold,
        after_hashing: Some(Arc::new({
            let (hashed, holding) = (Arc::clone(&hashed), Arc::clone(&holding));
            let (big, small) = (root.path.join(big), root.path.join(small));
            move |path: &Path| {
                hashed.lock().expect("one recorder").push(path.to_owned());
                if path == big.as_path() {
                    holding.store(true, SeqCst);
                    hold();
                } else if path == small.as_path() {
                    until("the big chapter's hash holds the lock", || {
                        holding.load(SeqCst)
                    });
                }
            }
        })),
        ..polling()
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);
    let was_hashed = |rel_path: &str| {
        hashed
            .lock()
            .expect("one recorder")
            .contains(&root.path.join(rel_path))
    };

    kick(&state);
    arrived
        .recv_timeout(SETTLE)
        .expect("the big chapter's hash holds the lock");
    until("the small chapter is stored", || {
        row_of(&library, &root, small).is_some()
    });
    std::thread::sleep(POLL * 4);

    assert!(
        !was_hashed(folder),
        "the folder is hashed while the lock is held"
    );
    release.send(()).expect("the worker waits for the word");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    for rel_path in [big, small, folder] {
        assert!(was_hashed(rel_path), "{rel_path} was hashed");
        assert!(
            row_of(&library, &root, rel_path).is_some(),
            "{rel_path} has a digest and a row"
        );
    }
}

/// One worker. The coordinator is held once its listing shows a job, before
/// anything is read, while the test kicks and then presses Cancel: a cancel
/// stops what was asked of the pass before it, so the pass ends having read
/// nothing and no pass of its own starts.
#[tokio::test]
async fn a_kick_before_a_cancel_starts_no_fresh_pass() {
    let (fixture, library, root) = scanned("pass-kick-before-cancel", &VOLUMES[..2]);
    let (hold, arrived, release) = gate();
    let armed = AtomicBool::new(true);
    let settings = PassSettings {
        after_listing: Some(Arc::new(move || {
            if armed.swap(false, SeqCst) {
                hold();
            }
        })),
        ..one_worker()
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);
    let router =
        test_support::authenticating(crate::build_router(Arc::clone(&state)), &state.library);

    kick(&state);
    arrived
        .recv_timeout(SETTLE)
        .expect("the coordinator has listed");
    let cancelled = the_job(&state);
    kick(&state);
    let pressed = test_support::send_json(
        &router,
        Method::POST,
        &format!("/api/library/scans/{}/cancel", cancelled.id),
        &json!({}),
    )
    .await
    .status();
    release
        .send(())
        .expect("the coordinator waits for the word");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    assert_eq!(pressed, StatusCode::ACCEPTED);
    assert_eq!(
        cancelled.ended(),
        Some(JobEnded::Cancelled(Step::Fingerprinting))
    );
    assert_eq!(the_job(&state).id, cancelled.id, "no pass of its own");
    for rel_path in &VOLUMES[..2] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            None,
            "{rel_path} is not read"
        );
    }
}

/// Two workers and one volume: one worker is held at the volume's first page
/// while the other finds the queue empty and stops. A scan then adds a volume
/// and kicks, and the new volume is stored while the first is still held, so
/// the relist started a worker in place of the one that stopped.
#[test]
fn a_relist_starts_a_worker_for_one_that_has_stopped() {
    let (fixture, library, root) = scanned("pass-relist-tops-up", &VOLUMES[..1]);
    let (hold, arrived, release) = gate();
    let armed = AtomicBool::new(true);
    let (stop, stopped) = mpsc::channel::<()>();
    let settings = PassSettings {
        after_stopping: Some(Arc::new(move || {
            let _ = stop.send(());
        })),
        ..hooked(
            PassSettings {
                workers: 2,
                ..polling()
            },
            move |_, index| {
                if index == 0 && armed.swap(false, SeqCst) {
                    hold();
                }
            },
        )
    };
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kick(&state);
    arrived
        .recv_timeout(SETTLE)
        .expect("a worker reaches the volume");
    stopped
        .recv_timeout(SETTLE)
        .expect("the other worker stops");
    let added = "Delta/Delta v01.cbz";
    write_volumes(&root.path, &[added], 3);
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks the new volume");
        derive::derive(&conn).expect("derives");
    }
    kick(&state);
    until("the new volume is stored while the first is held", || {
        outcome_of(&library, &root, added).is_some()
    });
    release.send(()).expect("the worker waits for the word");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");

    for rel_path in [VOLUMES[0], added] {
        assert_eq!(
            outcome_of(&library, &root, rel_path),
            Some(Outcome::Whole),
            "{rel_path}"
        );
    }
}

/// The status and body that `method` on `path` with `body` answers.
async fn sent(
    router: &axum::Router,
    method: Method,
    path: &str,
    body: &serde_json::Value,
) -> (StatusCode, String) {
    let response = test_support::send_json(router, method, path, body).await;
    let status = response.status();
    (status, test_support::body_string(response).await)
}

/// Global Constraints: the pass takes no job lock, so a rules save, a stamp, a
/// Move and a Remove from a library each go through while it is held inside a
/// file. Each is set up as its own route's tests set it up: the save and the
/// stamp over the stamp tests' archive, which has something to stamp; the Move
/// and the Remove over two libraries of a directory each, and a third library
/// sharing root-a's directory to be removed from.
#[tokio::test]
async fn rules_stamp_move_and_remove_go_through_while_a_pass_reads() {
    let (_, archive, archive_fixture) = test_support::router_over_a_scanned_cbz();
    let slug: String = {
        let conn = archive.library.conn();
        conn.execute("UPDATE user SET can_stamp = 1 WHERE id = 1", [])
            .expect("grants stamping");
        conn.query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
            .expect("the fixture's one work")
    };
    let held = HeldPass::over(&archive, &archive_fixture);
    let rules = test_support::get_json(&held.router, "/api/library/rules").await["rules"].clone();
    let saved = sent(
        &held.router,
        Method::PUT,
        "/api/library/rules",
        &json!({ "rules": rules }),
    )
    .await;
    let stamp = format!("/api/library/works/{slug}/stamp-comicinfo");
    let planned = test_support::get_json(&held.router, &stamp).await;
    let stamped = sent(&held.router, Method::POST, &stamp, &planned).await;
    held.let_go();

    let (router, two, two_fixture) = test_support::state_with_two_scanned_roots();
    let listed = test_support::get_json(&router, "/api/library/libraries").await;
    let library = |name: &str| {
        listed["libraries"]
            .as_array()
            .expect("a libraries array")
            .iter()
            .find(|library| library["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("no library called {name}: {listed}"))
    };
    let (a, b) = (library("root-a"), library("root-b"));
    let own = two_fixture.join("Own");
    test_support::write_pages(&own.join("Gamma").join("Gamma - v01"), 2);
    let (status, kids) = sent(
        &router,
        Method::POST,
        "/api/library/libraries",
        &json!({ "name": "Kids", "dirs": [a["dirs"][0]["path"], own.to_string_lossy()] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the fixture: Kids is made: {kids}");
    let kids: serde_json::Value = serde_json::from_str(&kids).expect("JSON");
    let held = HeldPass::over(&two, &two_fixture);
    let moved = sent(
        &held.router,
        Method::POST,
        &format!("/api/library/roots/{}/move", b["dirs"][0]["id"]),
        &json!({ "from": b["id"], "library": a["id"] }),
    )
    .await;
    let removed = sent(
        &held.router,
        Method::DELETE,
        &format!(
            "/api/library/libraries/{}/dirs/{}",
            kids["library"]["id"], a["dirs"][0]["id"]
        ),
        &json!({}),
    )
    .await;
    held.let_go();

    assert!(
        !planned["archives"].as_array().expect("an array").is_empty(),
        "the fixture: the stamp has something to write: {planned}"
    );
    let answers = [
        ("a rules save", saved),
        ("a stamp", stamped),
        ("a Move", moved),
        ("a Remove", removed),
    ];
    let statuses: Vec<(&str, StatusCode)> = answers
        .iter()
        .map(|(what, (status, _))| (*what, *status))
        .collect();
    assert_eq!(
        statuses,
        [
            ("a rules save", StatusCode::OK),
            ("a stamp", StatusCode::OK),
            ("a Move", StatusCode::OK),
            ("a Remove", StatusCode::OK),
        ],
        "{answers:?}"
    );
}

/// The pass the server builds is set to four workers and a 30 s quiet, and a
/// reader just seen makes it wait; its `Debug` says it is live, and a default
/// pass's says it is inert. Every other test builds its pass from settings of
/// its own.
#[test]
fn the_servers_pass_is_set_to_four_workers_and_a_30_s_quiet() {
    let live = Pass::live();
    let settings = live.settings.as_ref().expect("the server's pass is live");
    assert_eq!(settings.workers, 4);
    assert_eq!(settings.quiet, Duration::from_secs(30));
    live.reader_seen();
    assert_eq!(waits(&live, false, Instant::now()), Some(Wait::Reader));
    for (pass, said) in [(&live, "live: true"), (&Pass::default(), "live: false")] {
        let debug = format!("{pass:?}");
        assert!(debug.contains(said), "{debug}");
    }
}

/// The page hook is asked once for each walked page, in order, and for no
/// page the border sample reads: a file of 26 pages samples 13, so a hook
/// asked at the sample sees half its pages. Other tests count walked pages
/// through it.
#[test]
fn the_page_hook_is_asked_for_each_walked_page_once_and_for_no_sampled_one() {
    let fixture = Fixture::new("pass-page-hook");
    let top = fixture.join("lib");
    let path = top.join(VOLUMES[0]);
    let dir = path.parent().expect("a file under the tree");
    std::fs::create_dir_all(dir).expect("makes its directory");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a name");
    cbz_pages(dir, name, 26, 0);
    let library = test_support::library();
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, &top).expect("adds the directory");
        scan::scan_root(&conn, &root).expect("walks it");
        derive::derive(&conn).expect("derives");
    }
    let asked: Arc<Mutex<Vec<usize>>> = Arc::default();
    let settings = hooked(one_worker(), {
        let asked = Arc::clone(&asked);
        move |_, index| asked.lock().expect("one recorder").push(index)
    });
    let state = state_of(library, &fixture, settings);

    kicked(&state);

    assert_eq!(
        *asked.lock().expect("one recorder"),
        (0..26).collect::<Vec<_>>()
    );
}

/// Two chapters of the same bytes, neither hashed when the pass lists them, so
/// both are listed: the first is read and stored, and the second, once hashed,
/// is found to hold the first's row and is not read again.
#[test]
fn a_copy_hashed_mid_pass_is_not_read_again_once_its_twin_is_stored() {
    const COPY: &str = "Kaguya/Kaguya - c002.cbz";
    let fixture = Fixture::new("pass-twin-hashed");
    let top = fixture.join("lib");
    write_volumes(&top, &[CHAPTER], 0);
    std::fs::copy(top.join(CHAPTER), top.join(COPY)).expect("copies the chapter");
    let library = test_support::library();
    let root = {
        let conn = library.conn();
        let root = scan::add_root(&conn, &top).expect("adds the directory");
        scan::scan_root(&conn, &root).expect("walks it");
        derive::derive(&conn).expect("derives");
        root
    };
    let due =
        fingerprints::due(&library.conn(), fingerprint::VERSION, None).expect("lists what is due");
    assert_eq!(
        due.iter()
            .map(|unit| (unit.rel_path.as_str(), unit.sha256.is_some()))
            .collect::<Vec<_>>(),
        [(CHAPTER, false), (COPY, false)],
        "the fixture: both copies are due, neither hashed"
    );
    let walked: Arc<Mutex<Vec<(PathBuf, usize)>>> = Arc::default();
    let settings = hooked(one_worker(), {
        let walked = Arc::clone(&walked);
        move |path, index| {
            walked
                .lock()
                .expect("one recorder")
                .push((path.to_owned(), index));
        }
    });
    let state = state_of(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let walked = walked.lock().expect("one recorder");
    let first = walked
        .first()
        .map(|(path, _)| path.clone())
        .expect("a file was read");
    assert_eq!(
        *walked,
        (0..3).map(|i| (first.clone(), i)).collect::<Vec<_>>(),
        "one copy's pages, each once"
    );
    let digests = [CHAPTER, COPY].map(|rel_path| digest_at(&library, root.id, rel_path));
    assert!(digests[0].is_some(), "the copies are hashed");
    assert_eq!(digests[0], digests[1], "the copies are hashed alike");
}

/// What the pass cannot do is said on stderr, where a host's log keeps it.
/// Read from a child run of this test, since the line goes to the process's
/// own stderr, which the harness does not capture.
#[test]
fn what_the_pass_cannot_do_is_said_on_stderr() {
    const CHILD: &str = "TAIYAKU_TEST_PASS_SAYS";
    if std::env::var_os(CHILD).is_some() {
        say("the store refused a row");
        return;
    }
    let ran =
        std::process::Command::new(std::env::current_exe().expect("the test binary's own path"))
            .args([
                "--exact",
                "fingerprint_pass::tests::what_the_pass_cannot_do_is_said_on_stderr",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .expect("runs this test as a child");
    assert!(ran.status.success(), "the child passes");
    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line == "taiyaku: the fingerprint pass: the store refused a row"),
        "the child's stderr: {stderr}"
    );
}

/// Two volume folders whose pages do not hash to the digests their scans took:
/// one with a page rewritten in place, which moves no stamp of the folder's,
/// and one with a page replaced by a link to nothing, its stamp put back, so
/// its walk ends on a read error. Each pass reads both and stores nothing, and
/// says so once for each: the rewrite for the first, the error for the second.
/// Read from a child run of this test, as above, which runs two passes.
#[test]
fn a_folder_whose_pages_do_not_hash_to_its_digest_is_said_once_a_pass() {
    const CHILD: &str = "TAIYAKU_TEST_PASS_SAYS_NOT_ITS_DIGEST";
    const REWRITTEN: &str = "Delta/Delta v01";
    const BROKEN: &str = "Epsilon/Epsilon v01";
    if std::env::var_os(CHILD).is_some() {
        let (fixture, library, root) = scanned("pass-says-not-its-digest", &VOLUMES[..1]);
        for folder in [REWRITTEN, BROKEN] {
            test_support::write_pages(&root.path.join(folder), 3);
        }
        {
            let conn = library.conn();
            scan::scan_root(&conn, &root).expect("walks the folders");
            derive::derive(&conn).expect("derives");
        }
        for folder in [REWRITTEN, BROKEN] {
            assert!(
                digest_at(&library, root.id, folder).is_some(),
                "the fixture: the derive took {folder}'s digest"
            );
        }
        grow(&root.path.join(REWRITTEN).join("000.jpg"));
        let broken = root.path.join(BROKEN);
        let stamp = fingerprints::stamp_of(&broken);
        let modified = std::fs::metadata(&broken)
            .and_then(|meta| meta.modified())
            .expect("the folder's mtime");
        let page = broken.join("001.jpg");
        std::fs::remove_file(&page).expect("removes the page");
        std::os::unix::fs::symlink(broken.join("nowhere"), &page)
            .expect("links the page to nothing");
        std::fs::File::open(&broken)
            .and_then(|dir| dir.set_modified(modified))
            .expect("puts the folder's mtime back");
        assert_eq!(
            fingerprints::stamp_of(&broken),
            stamp,
            "the fixture: the folder's own stamp is as scanned"
        );
        let state = state_of(Arc::clone(&library), &fixture, polling());
        kicked(&state);
        kicked(&state);
        for folder in [REWRITTEN, BROKEN] {
            assert_eq!(
                outcome_of(&library, &root, folder),
                None,
                "nothing is stored for {folder}"
            );
        }
        return;
    }
    let ran =
        std::process::Command::new(std::env::current_exe().expect("the test binary's own path"))
            .args([
                "--exact",
                "fingerprint_pass::tests::a_folder_whose_pages_do_not_hash_to_its_digest_is_said_once_a_pass",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .expect("runs this test as a child");
    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(ran.status.success(), "the child passes: {stderr}");
    let said = |words: &[&str]| {
        stderr
            .lines()
            .filter(|line| {
                line.starts_with("taiyaku: the fingerprint pass: ")
                    && words.iter().all(|word| line.contains(word))
            })
            .count()
    };
    assert_eq!(
        said(&["Delta v01: its pages do not hash to the digest its scan took"]),
        2,
        "the rewrite, once in each pass; the child's stderr: {stderr}"
    );
    assert_eq!(
        said(&[
            "Epsilon v01: it could not be read whole, so nothing is stored",
            "001.jpg: ",
        ]),
        2,
        "the read error, once in each pass; the child's stderr: {stderr}"
    );
    assert_eq!(
        said(&["Epsilon v01: its pages do not hash"]),
        0,
        "no rewrite said of the folder that could not be read; the child's stderr: {stderr}"
    );
}

/// A cbz at `path` holding `pages`, each entry's bytes as given.
fn cbz_holding(path: &Path, pages: &[(&str, &[u8])]) {
    use std::io::Write as _;
    let dir = path.parent().expect("a file under the tree");
    std::fs::create_dir_all(dir).expect("makes its directory");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).expect("creates the cbz"));
    for (name, bytes) in pages {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("starts an entry");
        zip.write_all(bytes).expect("writes an entry");
    }
    zip.finish().expect("finishes the cbz");
}

/// A library over `<fixture>/lib` holding one volume of `pages`, walked and
/// derived.
fn scanned_volume(tag: &str, pages: &[(&str, &[u8])]) -> (Fixture, Arc<Library>, scan::Root) {
    let fixture = Fixture::new(tag);
    let top = fixture.join("lib");
    cbz_holding(&top.join(VOLUMES[0]), pages);
    let library = test_support::library();
    let root = {
        let conn = library.conn();
        let root = scan::add_root(&conn, &top).expect("adds the directory");
        scan::scan_root(&conn, &root).expect("walks it");
        derive::derive(&conn).expect("derives");
        root
    };
    (fixture, library, root)
}

/// `image` knows JPEG XL only through the hooks `serve` installs before any
/// pass starts, and a pass decodes a page by guessing its format, so without
/// them every page of this volume would be stored as undecoded.
#[test]
fn a_pass_prints_every_page_of_a_volume_of_jpeg_xl_pages() {
    crate::jxl::register();
    let pages = [
        ("000.jxl", test_support::jxl_fixture("lossy.jxl")),
        ("001.jxl", test_support::jxl_fixture("grey.jxl")),
    ];
    let (fixture, library, root) = scanned_volume("pass-jxl-pages", &pages);
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());

    kicked(&state);

    let row = row_of(&library, &root, VOLUMES[0]).expect("the volume has a row");
    assert_eq!(row.outcome, Outcome::Whole, "its reason: {:?}", row.reason);
    assert_eq!(row.pages, 2, "both pages");
    let prints = fingerprint::decode(&row.prints).expect("the row's prints decode");
    assert_eq!(prints.len(), 2, "one record per page");
    assert!(
        prints.iter().all(Option::is_some),
        "a print of each page: {prints:?}"
    );
    assert_ne!(prints[0], prints[1], "each page's own print");
}

/// A re-count keeps a file's digest, and a row is keyed by that digest, so the
/// row made under the old page numbers stands until a read replaces it: it
/// numbers fewer pages than the entry now counts, and it is partial by another
/// build.
#[test]
fn a_mixed_volumes_row_is_read_again_under_the_pages_its_re_count_numbers() {
    crate::jxl::register();
    let pages = [
        ("000.jpg", test_support::jxl_fixture("recompressed.jpg")),
        ("001.jxl", test_support::jxl_fixture("lossy.jxl")),
        ("002.jpg", test_support::jxl_fixture("trailing.jpg")),
    ];
    let (fixture, library, root) = scanned_volume("pass-mixed-recount", &pages);
    let digest = digest_at(&library, root.id, VOLUMES[0]).expect("the derive took its digest");
    let earlier =
        fingerprint::describe_page(&image::GrayImage::new(8, 8), fingerprint::Border::default());
    let count = |library: &Library| -> i64 {
        library
            .conn()
            .query_row(
                "SELECT pages FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![root.id, VOLUMES[0]],
                |r| r.get(0),
            )
            .expect("reads the entry's count")
    };
    {
        let conn = library.conn();
        conn.execute(
            "UPDATE scan_entry SET pages = 2, counted_by = NULL
              WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, VOLUMES[0]],
        )
        .expect("leaves the entry as a build that did not count JPEG XL did");
        fingerprints::store(
            &conn,
            &Row {
                sha256: digest.clone(),
                version: fingerprint::VERSION,
                outcome: Outcome::Partial,
                reason: Some("001.jxl is of a type the reader does not read".to_owned()),
                build: Some("a build before JPEG XL".to_owned()),
                pages: 2,
                prints: fingerprint::encode(&[Some(earlier), Some(earlier)]),
            },
        )
        .expect("stores that build's row");
        scan::scan_root(&conn, &root).expect("walks it again");
    }
    assert_eq!(count(&library), 3, "the re-count takes in the JPEG XL page");
    assert_eq!(
        digest_at(&library, root.id, VOLUMES[0]),
        Some(digest.clone()),
        "the re-count keeps the digest the row is keyed by"
    );
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());

    kicked(&state);

    let row = fingerprints::read(&library.conn(), &digest)
        .expect("reads the row")
        .expect("the row stands");
    assert_eq!(row.outcome, Outcome::Whole, "its reason: {:?}", row.reason);
    assert_eq!(row.pages, 3, "the page the re-count took in");
    let prints = fingerprint::decode(&row.prints).expect("the row's prints decode");
    assert_eq!(prints.len(), 3, "one record per page as now numbered");
    assert!(
        prints
            .iter()
            .all(|print| print.is_some() && *print != Some(earlier)),
        "every page read again: {prints:?}"
    );
}

/// `folder`'s row, under the stamp the folder holds now: what a walk by a reader
/// that passed over the page just added left there.
fn stamped_as_walked(library: &Library, root: &scan::Root, folder: &str) {
    let (size, mtime) =
        fingerprints::stamp_of(&root.path.join(folder)).expect("reads the folder's stamp");
    library
        .conn()
        .execute(
            "UPDATE scan_entry SET size = ?1, mtime = ?2 WHERE root_id = ?3 AND rel_path = ?4",
            rusqlite::params![size, mtime, root.id, folder],
        )
        .expect("records the stamp");
}

/// A row as a build that read only `pages` of a file left it.
fn earlier_row(sha256: &str, outcome: Outcome, pages: u32) -> Row {
    let print =
        fingerprint::describe_page(&image::GrayImage::new(8, 8), fingerprint::Border::default());
    Row {
        sha256: sha256.to_owned(),
        version: fingerprint::VERSION,
        outcome,
        reason: (outcome != Outcome::Whole).then(|| "the reader does not read 001.jxl".to_owned()),
        build: Some("a build before JPEG XL".to_owned()),
        pages,
        prints: fingerprint::encode(&vec![Some(print); pages as usize]),
    }
}

/// What the pass still has to read under this build.
fn still_listed(library: &Library) -> Vec<String> {
    fingerprints::due(
        &library.conn(),
        fingerprint::VERSION,
        crate::about::this_build().as_deref(),
    )
    .expect("lists what is due")
    .into_iter()
    .map(|unit| unit.rel_path)
    .collect()
}

const MIXED_FOLDER: &str = "Delta/Delta v01";

/// A library holding [`MIXED_FOLDER`]'s two JPEG pages, walked and derived, then
/// the JPEG XL page between them, which a build that did not read it passed
/// over: so the row holds that build's digest, count and stamp.
fn mixed_folder_as_an_earlier_build_left_it(
    fixture: &Fixture,
    twin: Option<&str>,
) -> (Arc<Library>, scan::Root, String) {
    let top = fixture.join("lib");
    let dir = top.join(MIXED_FOLDER);
    std::fs::create_dir_all(&dir).expect("makes the folder");
    let jpegs = [
        ("000.jpg", test_support::jxl_fixture("recompressed.jpg")),
        ("002.jpg", test_support::jxl_fixture("trailing.jpg")),
    ];
    for (name, bytes) in jpegs {
        std::fs::write(dir.join(name), bytes).expect("writes a page");
        if let Some(twin) = twin {
            std::fs::create_dir_all(top.join(twin)).expect("makes the twin");
            std::fs::write(top.join(twin).join(name), bytes).expect("writes a page");
        }
    }
    let library = test_support::library();
    let root = {
        let conn = library.conn();
        let root = scan::add_root(&conn, &top).expect("adds the directory");
        scan::scan_root(&conn, &root).expect("walks it");
        derive::derive(&conn).expect("derives");
        root
    };
    let earlier = digest_at(&library, root.id, MIXED_FOLDER).expect("the derive took a digest");
    std::fs::write(dir.join("001.jxl"), test_support::jxl_fixture("lossy.jxl"))
        .expect("writes the JPEG XL page");
    stamped_as_walked(&library, &root, MIXED_FOLDER);
    (library, root, earlier)
}

/// The digest an earlier build took hashed the JPEG pages alone, and a read
/// now hashes all three, so a row under it could never be stored.
#[test]
fn a_mixed_folder_an_earlier_build_read_is_read_under_the_pages_it_now_numbers() {
    crate::jxl::register();
    let fixture = Fixture::new("pass-mixed-folder");
    let (library, root, earlier) = mixed_folder_as_an_earlier_build_left_it(&fixture, None);
    fingerprints::store(&library.conn(), &earlier_row(&earlier, Outcome::Partial, 2))
        .expect("stores that build's row");
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks it again");
        derive::derive(&conn).expect("derives again");
    }
    let digest = digest_at(&library, root.id, MIXED_FOLDER).expect("the digest is taken again");
    assert_ne!(digest, earlier, "the digest takes in the JPEG XL page");
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());

    kicked(&state);

    let row = fingerprints::read(&library.conn(), &digest)
        .expect("reads the row")
        .expect("the read is stored under the digest it hashes to");
    assert_eq!(row.outcome, Outcome::Whole, "its reason: {:?}", row.reason);
    assert_eq!(row.pages, 3, "every page the entry numbers");
    assert_eq!(
        still_listed(&library),
        Vec::<String>::new(),
        "nothing is left due"
    );
    assert_eq!(
        fingerprints::read(&library.conn(), &earlier).expect("reads the store"),
        None,
        "no file holds the earlier row's digest now"
    );
}

/// An earlier build's digest ignored the JPEG XL page, so the folder shared it
/// with a copy of its JPEG pages, whose whole row spoke for both.
#[test]
fn a_mixed_folder_sharing_a_twins_whole_row_is_read_under_its_own_digest() {
    crate::jxl::register();
    const TWIN: &str = "Eta/Eta v01";
    let fixture = Fixture::new("pass-mixed-twin");
    let (library, root, earlier) = mixed_folder_as_an_earlier_build_left_it(&fixture, Some(TWIN));
    assert_eq!(
        digest_at(&library, root.id, TWIN).as_deref(),
        Some(earlier.as_str()),
        "the fixture: the two share a digest"
    );
    fingerprints::store(&library.conn(), &earlier_row(&earlier, Outcome::Whole, 2))
        .expect("stores the twin's row");
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks it again");
        derive::derive(&conn).expect("derives again");
    }
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());

    kicked(&state);

    let row = row_of(&library, &root, MIXED_FOLDER).expect("the folder has a row of its own");
    assert_eq!(row.outcome, Outcome::Whole, "its reason: {:?}", row.reason);
    assert_eq!(row.pages, 3, "every page the entry numbers");
    let twin = row_of(&library, &root, TWIN).expect("the twin keeps its row");
    assert_eq!(
        (twin.sha256.as_str(), twin.pages),
        (earlier.as_str(), 2),
        "the twin's digest and row stand"
    );
    assert_eq!(
        still_listed(&library),
        Vec::<String>::new(),
        "nothing is left due"
    );
}

/// The row is this very build's, so no build stamp marks it stale: only its
/// count against the re-count does.
#[test]
fn a_row_of_this_build_numbering_fewer_pages_than_the_re_count_is_read_again() {
    crate::jxl::register();
    let pages = [
        ("000.jpg", test_support::jxl_fixture("recompressed.jpg")),
        ("001.jxl", test_support::jxl_fixture("lossy.jxl")),
        ("002.jpg", test_support::jxl_fixture("trailing.jpg")),
    ];
    let (fixture, library, root) = scanned_volume("pass-other-pages", &pages);
    let digest = digest_at(&library, root.id, VOLUMES[0]).expect("the derive took its digest");
    {
        let conn = library.conn();
        conn.execute(
            "UPDATE scan_entry SET pages = 2, counted_by = NULL
              WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, VOLUMES[0]],
        )
        .expect("leaves the count a build that did not read JPEG XL took");
        fingerprints::store(
            &conn,
            &Row {
                build: crate::about::this_build(),
                ..earlier_row(&digest, Outcome::Partial, 2)
            },
        )
        .expect("stores the row");
        scan::scan_root(&conn, &root).expect("counts it again");
    }
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());

    kicked(&state);

    let row = fingerprints::read(&library.conn(), &digest)
        .expect("reads the row")
        .expect("the row stands");
    assert_eq!(row.outcome, Outcome::Whole, "its reason: {:?}", row.reason);
    assert_eq!(row.pages, 3, "the page the re-count took in");
}

/// Every outcome a read can store, and a folder whose walk leaves out a page its
/// read takes in: one pass settles them all, so the next has nothing to read.
#[test]
fn no_row_a_pass_stores_is_due_for_the_next() {
    crate::jxl::register();
    const FOLDER: &str = "Delta/Delta v01";
    let jpeg = test_support::jxl_fixture("recompressed.jpg");
    let jxl = test_support::jxl_fixture("lossy.jxl");
    let fixture = Fixture::new("pass-settles");
    let top = fixture.join("lib");
    cbz_holding(
        &top.join("Alpha/Alpha v01.cbz"),
        &[("000.jpg", jpeg), ("001.jxl", jxl)],
    );
    cbz_holding(
        &top.join("Beta/Beta v01.cbz"),
        &[("000.jpg", jpeg), ("001.jpg", b"no image".as_slice())],
    );
    cbz_holding(
        &top.join("Gamma/Gamma v01.cbz"),
        &[("000.jpg", b"no image".as_slice())],
    );
    std::fs::create_dir_all(top.join(FOLDER)).expect("makes the folder");
    for name in ["000.jpg", "001.jpg", "credit.jpg"] {
        std::fs::write(top.join(FOLDER).join(name), jpeg).expect("writes a page");
    }
    let library = test_support::library();
    let root = {
        let conn = library.conn();
        let added = scan::add_root(&conn, &top).expect("adds the directory");
        conn.execute(
            "UPDATE root SET exclude_globs = 'credit.jpg' WHERE id = ?1",
            [added.id],
        )
        .expect("excludes a page from the walk");
        let root = scan::roots(&conn)
            .expect("lists the directories")
            .into_iter()
            .find(|root| root.id == added.id)
            .expect("the directory just added");
        scan::scan_root(&conn, &root).expect("walks it");
        derive::derive(&conn).expect("derives");
        root
    };
    let state = state_of(Arc::clone(&library), &fixture, PassSettings::default());

    kicked(&state);

    assert_eq!(
        [
            "Alpha/Alpha v01.cbz",
            "Beta/Beta v01.cbz",
            "Gamma/Gamma v01.cbz",
            FOLDER
        ]
        .map(|rel_path| outcome_of(&library, &root, rel_path)),
        [
            Some(Outcome::Whole),
            Some(Outcome::Partial),
            Some(Outcome::Unreadable),
            Some(Outcome::Whole),
        ],
        "the fixture: one of each outcome"
    );
    assert_eq!(
        row_of(&library, &root, FOLDER).map(|row| row.pages),
        Some(3),
        "the fixture: the read takes in the page the walk's total of 2 leaves out"
    );
    assert_eq!(
        still_listed(&library),
        Vec::<String>::new(),
        "nothing is left due"
    );
    let next = state_of(Arc::clone(&library), &fixture, PassSettings::default());
    kicked(&next);
    assert!(
        next.fingerprints.job().is_none(),
        "the next pass reads nothing"
    );
}

/// A state over `library` whose fingerprint pass runs as `settings` say and
/// whose match pass runs too, so a pass that goes idle has one to start.
fn chained(library: Arc<Library>, fixture: &Fixture, settings: PassSettings) -> Arc<ServeState> {
    Arc::new(ServeState {
        library,
        fingerprints: Arc::new(Pass::with(settings)),
        matches: Arc::new(crate::match_pass::Pass::with(
            crate::match_pass::PassSettings {
                poll: POLL,
                ..crate::match_pass::PassSettings::default()
            },
        )),
        ..test_support::state(fixture)
    })
}

/// Every digest the match pass has looked up.
fn looked_up(library: &Library) -> HashSet<String> {
    matches::looked_up(&library.conn()).expect("reads the lookups")
}

/// The digests of `rel_paths` in `root`.
fn digests_of(library: &Library, root: &scan::Root, rel_paths: &[&str]) -> HashSet<String> {
    rel_paths
        .iter()
        .map(|rel_path| {
            digest_at(library, root.id, rel_path).unwrap_or_else(|| panic!("{rel_path}'s digest"))
        })
        .collect()
}

/// Spec §2, "When": the match pass follows a fingerprint pass that ends
/// uncancelled, and a cancel stops the chain with the pass it lands on. The
/// first pass reads two volumes, which the match pass after it looks up. A
/// third volume is then due and both lookups are forgotten, so a match pass
/// started after the cancelled second pass would look both up again.
#[test]
fn a_finished_fingerprint_pass_starts_a_match_pass_and_a_cancelled_one_does_not() {
    let (fixture, library, root) = scanned("pass-chain", &VOLUMES[..2]);
    let (hold, arrived, release) = gate();
    let listings = AtomicUsize::new(0);
    let settings = PassSettings {
        after_listing: Some(Arc::new(move || {
            if listings.fetch_add(1, SeqCst) == 1 {
                hold();
            }
        })),
        ..polling()
    };
    let state = chained(Arc::clone(&library), &fixture, settings);
    let read = digests_of(&library, &root, &VOLUMES[..2]);

    kicked(&state);
    assert!(state.matches.settle(SETTLE), "the match pass ends");
    let finished = the_job(&state);
    let first = state.matches.job();
    let first_looked_up = looked_up(&library);

    write_volumes(&root.path, &VOLUMES[2..], 2);
    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("walks it again");
        derive::derive(&conn).expect("derives");
        conn.execute("DELETE FROM match_lookup", [])
            .expect("forgets the lookups");
    }
    kick(&state);
    arrived
        .recv_timeout(SETTLE)
        .expect("the second pass has listed");
    let cancelled = the_job(&state);
    state.fingerprints.cancel(&cancelled);
    release
        .send(())
        .expect("the coordinator waits for the word");
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    assert!(state.matches.settle(SETTLE), "the match pass is idle");

    assert_eq!(
        finished.ended(),
        Some(JobEnded::Finished),
        "the fixture: the first pass ran to its end"
    );
    let first = first.expect("a finished pass starts a match pass");
    assert_eq!(
        first.ended(),
        Some(JobEnded::Finished),
        "which ran to its end"
    );
    assert_eq!(first_looked_up, read, "and looked up both volumes");
    assert_eq!(
        cancelled.ended(),
        Some(JobEnded::Cancelled(Step::Fingerprinting)),
        "the fixture: the second pass was cancelled"
    );
    assert_eq!(
        state.matches.job().map(|job| job.id),
        Some(first.id),
        "a cancelled pass starts no match pass"
    );
    assert_eq!(looked_up(&library), HashSet::new(), "nothing is looked up");
}

/// Spec §2, "When", at startup: with nothing due a fingerprint, the pass
/// started at startup lists nothing and goes idle, and the match pass it starts
/// looks up the two volumes an earlier pass read and no match pass looked up.
#[test]
fn startup_resumes_matching_after_fingerprinting() {
    let (fixture, library, root) = scanned("pass-startup-matching", &VOLUMES[..2]);
    kicked(&state_of(Arc::clone(&library), &fixture, polling()));
    let due = fingerprints::due(&library.conn(), fingerprint::VERSION, None).expect("lists");
    assert!(due.is_empty(), "the fixture: nothing is due a fingerprint");
    assert_eq!(
        looked_up(&library),
        HashSet::new(),
        "the fixture: nothing is looked up"
    );
    let state = chained(Arc::clone(&library), &fixture, polling());

    at_startup(&state);
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
    assert!(state.matches.settle(SETTLE), "the match pass ends");

    assert!(
        state.fingerprints.job().is_none(),
        "the fixture: the pass had nothing to read"
    );
    assert_eq!(
        looked_up(&library),
        digests_of(&library, &root, &VOLUMES[..2]),
        "both volumes are looked up"
    );
}
