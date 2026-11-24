use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use axum::extract::{Extension, Path as AxPath, State};
use axum::http::{Method, StatusCode};
use serde_json::json;
use taiyaku_library::account::Access;
use taiyaku_library::fingerprints::{self, Unit};
use taiyaku_library::progress::{Step, Watch};
use taiyaku_library::scan::{self, InLibrary, add_root_checked_in};
use taiyaku_library::{derive, libraries, matches};
use taiyaku_read::test_support::cbz_pages;

use crate::fingerprint;
use crate::fingerprint_pass;
use crate::match_pass;
use crate::scan_jobs::{self, Job, JobEnded, Reread, Running};
use crate::test_support::{self, HeldPass, SETTLE, one_worker};

fn principal(role: crate::auth::Role, access: Access) -> crate::auth::Principal {
    crate::auth::Principal {
        user_id: 1,
        role,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access,
    }
}

fn an_administrator() -> crate::auth::Principal {
    principal(crate::auth::Role::Admin, Access::Every)
}

/// A reader granted `libraries` alone.
fn a_reader_of(libraries: Vec<i64>) -> crate::auth::Principal {
    principal(
        crate::auth::Role::User,
        Access::Only {
            roots: Vec::new(),
            libraries,
        },
    )
}

/// The extensions a request by `principal` arrives with, signed in `by`.
fn signed_in(principal: crate::auth::Principal, by: crate::auth::By) -> axum::http::Extensions {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(by);
    extensions.insert(principal);
    extensions
}

fn as_admin() -> axum::http::Extensions {
    signed_in(an_administrator(), crate::auth::By::Cookie)
}

/// The feed's answer to a request arriving with `extensions`, parsed.
async fn fed(
    state: &Arc<crate::ServeState>,
    extensions: axum::http::Extensions,
) -> serde_json::Value {
    let response = super::feed(State(Arc::clone(state)), extensions).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    serde_json::from_str(&text).expect("a JSON body")
}

/// Cancel's answer to `principal`, signed in `by`, for job `job`.
async fn cancelled(
    state: &Arc<crate::ServeState>,
    principal: crate::auth::Principal,
    by: crate::auth::By,
    job: u64,
) -> (StatusCode, serde_json::Value) {
    let response = super::cancel(
        State(Arc::clone(state)),
        Extension(principal.clone()),
        signed_in(principal, by),
        AxPath(job),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, json)
}

fn keys_of(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

/// Library A holding `x`, and library B holding `x/inner`, which A reaches,
/// over an in-memory store: A's id, B's id, and the two directories, `x`
/// first.
struct Nested {
    state: Arc<crate::ServeState>,
    _fixture: test_support::Fixture,
    a: i64,
    b: i64,
    roots: Vec<scan::Root>,
}

fn nested(tag: &str) -> Nested {
    let fixture = test_support::Fixture::new(tag);
    let x = fixture.path().join("x");
    let inner = x.join("inner");
    test_support::write_pages(&inner.join("Inner").join("Inner - v01"), 2);
    let state = Arc::new(test_support::state(fixture.path()));
    let (a, b, roots) = {
        let conn = state.library.conn();
        add_root_checked_in(&conn, &x, InLibrary::Named("A")).expect("A holds x");
        add_root_checked_in(&conn, &inner, InLibrary::Named("B")).expect("B holds x/inner");
        let id = |name: &str| {
            taiyaku_library::libraries::id_of(&conn, name)
                .expect("reads")
                .expect("the library")
        };
        (
            id("A"),
            id("B"),
            scan::roots(&conn).expect("the directories"),
        )
    };
    assert_eq!(roots.len(), 2, "the fixture: two directories");
    assert!(roots[0].path < roots[1].path, "the fixture: x first");
    Nested {
        state,
        _fixture: fixture,
        a,
        b,
        roots,
    }
}

impl Nested {
    /// A job of both directories, reserved and not run, holding the
    /// libraries a spawned job reads.
    fn a_job(&self) -> Arc<Job> {
        let job =
            scan_jobs::reserve(&self.state, &self.roots, Instant::now(), None).expect("reserves");
        let held = scan_jobs::held_libraries(&self.state.library.conn(), &self.roots)
            .expect("reads the libraries");
        assert_eq!(
            held,
            [(self.a, "A".to_string()), (self.b, "B".to_string())],
            "the fixture: the job shows on A's page and B's"
        );
        *job.libraries.lock().expect("unpoisoned") = held;
        job
    }

    fn path(&self, at: usize) -> String {
        self.roots[at].path.to_string_lossy().into_owned()
    }
}

/// Spec §7, "The feed", the admin form: each running job with its step,
/// counts, directory, directories, libraries, elapsed time and cancel flag.
#[tokio::test]
async fn the_feed_answers_an_admin_each_running_job_with_its_step_counts_dirs_and_libraries() {
    let nested = nested("feed-admin-form");
    let job = nested.a_job();
    let running = Running::new(&job, None);
    running.at_dir(2, 5);
    running.at(Step::Resolving, 120, Some(2097));

    let body = fed(&nested.state, as_admin()).await;

    assert_eq!(
        keys_of(&body),
        ["ended", "jobs", "libraries", "written"],
        "{body}"
    );
    assert!(body["ended"].is_u64(), "{body}");
    assert_eq!(
        body["libraries"],
        json!([
            { "id": nested.a, "step": "resolving" },
            { "id": nested.b, "step": "resolving" },
        ]),
        "{body}"
    );
    let jobs = body["jobs"].as_array().expect("a jobs array");
    assert_eq!(jobs.len(), 1, "{body}");
    let mut sent = jobs[0].clone();
    let elapsed = sent["elapsed_secs"]
        .as_f64()
        .expect("elapsed_secs, in seconds");
    assert!((0.0..60.0).contains(&elapsed), "{body}");
    sent.as_object_mut()
        .expect("an object")
        .remove("elapsed_secs");
    assert_eq!(
        sent,
        json!({
            "id": job.id,
            "kind": "scan",
            "step": "resolving",
            "done": 120,
            "of": 2097,
            "dir": { "at": 2, "of": 5 },
            "dirs": [
                { "id": nested.roots[0].id, "path": nested.path(0) },
                { "id": nested.roots[1].id, "path": nested.path(1) },
            ],
            "libraries": [
                { "id": nested.a, "name": "A" },
                { "id": nested.b, "name": "B" },
            ],
            "cancelling": false,
            "ready": null,
        }),
    );
}

/// Spec §3, Goal 2, and the feed's nulls: no count while a job waits or
/// finishes, no total while it lists, and none while hashing sizes folders,
/// before the total is known.
#[tokio::test]
async fn a_step_with_no_count_or_no_total_yet_sends_null() {
    let nested = nested("feed-nulls");
    let job = nested.a_job();
    let running = Running::new(&job, None);
    let counts = async || {
        let body = fed(&nested.state, as_admin()).await;
        let sent = &body["jobs"][0];
        (
            sent["step"].clone(),
            sent["done"].clone(),
            sent["of"].clone(),
            sent["dir"].clone(),
        )
    };

    let waiting = counts().await;
    running.at_dir(1, 2);
    running.at(Step::Listing, 7, None);
    let listing = counts().await;
    running.at(Step::Hashing, 0, None);
    let sizing = counts().await;
    running.at(Step::Finishing, 0, None);
    let finishing = counts().await;

    let null = serde_json::Value::Null;
    assert_eq!(
        waiting,
        (json!("waiting"), null.clone(), null.clone(), null.clone())
    );
    assert_eq!(
        listing,
        (
            json!("listing"),
            json!(7),
            null.clone(),
            json!({ "at": 1, "of": 2 })
        )
    );
    assert_eq!(
        sizing,
        (json!("hashing"), json!(0), null.clone(), null.clone())
    );
    assert_eq!(
        finishing,
        (json!("finishing"), null.clone(), null.clone(), null)
    );
}

/// Design O314 §7, "The report and the feed", and §8 test 12: an admin's job
/// carries the volumes its passes have written, null until one has, beside
/// the step it shows; every answer's `written`, a reader's too, moves as the
/// count rises.
#[tokio::test]
async fn a_jobs_ready_count_is_null_until_a_pass_writes_and_then_sent_to_an_admin() {
    let nested = nested("feed-ready");
    let job = nested.a_job();
    let running = Running::new(&job, None);
    let reader = || signed_in(a_reader_of(vec![nested.a]), crate::auth::By::Cookie);
    running.at(Step::Hashing, 1_024, Some(4_096));
    let before = (
        fed(&nested.state, as_admin()).await,
        fed(&nested.state, reader()).await,
    );
    running.written(41);
    let after = (
        fed(&nested.state, as_admin()).await,
        fed(&nested.state, reader()).await,
    );

    let sent = |(admin, reader): &(serde_json::Value, serde_json::Value)| {
        (
            admin["jobs"][0]["step"].clone(),
            admin["jobs"][0]["ready"].clone(),
            admin["written"].clone(),
            reader["written"].clone(),
        )
    };
    assert_eq!(
        sent(&before),
        (
            json!("hashing"),
            serde_json::Value::Null,
            json!(0),
            json!(0)
        ),
        "(step, count, admin's marker, reader's marker) before a pass: {before:?}"
    );
    assert_eq!(
        sent(&after),
        (json!("hashing"), json!(41), json!(1), json!(1)),
        "(step, count, admin's marker, reader's marker) once a pass told 41: {after:?}"
    );
}

/// Spec §7, "The feed", the reader form: only the granted libraries a job
/// holds, each with its step, and no count and no path. B is nested in A's
/// directory, so the job shows on both pages, and a reader granted B alone
/// sees B.
#[tokio::test]
async fn the_feed_answers_a_reader_the_granted_libraries_a_job_holds_with_the_step_alone() {
    let nested = nested("feed-reader-form");
    let job = nested.a_job();
    Running::new(&job, None).at(Step::Hashing, 1_048_576, Some(5_368_709_120));

    let body = fed(
        &nested.state,
        signed_in(a_reader_of(vec![nested.b]), crate::auth::By::Cookie),
    )
    .await;

    assert_eq!(keys_of(&body), ["ended", "libraries", "written"], "{body}");
    assert_eq!(
        body["libraries"],
        json!([{ "id": nested.b, "step": "hashing" }]),
        "{body}"
    );
    let text = body.to_string();
    for at in 0..2 {
        assert!(
            !text.contains(&nested.path(at)),
            "a reader is sent no path: {text}"
        );
    }
}

/// An administrator's API key is a device credential, and
/// `session::administers` does not admit it: it gets the reader form, every
/// library listed, as an administrator is granted each.
#[tokio::test]
async fn an_admins_device_key_gets_the_reader_form() {
    let nested = nested("feed-admin-key");
    let job = nested.a_job();
    Running::new(&job, None).at(Step::Writing, 3, Some(9));

    let body = fed(
        &nested.state,
        signed_in(an_administrator(), crate::auth::By::Key),
    )
    .await;

    assert_eq!(keys_of(&body), ["ended", "libraries", "written"], "{body}");
    assert_eq!(
        body["libraries"],
        json!([
            { "id": nested.a, "step": "writing" },
            { "id": nested.b, "step": "writing" },
        ]),
        "{body}"
    );
}

/// Each library shows the step of the lowest-id job holding it: two jobs,
/// one per directory, both shown on A's page.
#[tokio::test]
async fn a_library_two_jobs_hold_shows_the_step_of_the_lower_id() {
    let nested = nested("feed-two-jobs");
    let first = scan_jobs::reserve(&nested.state, &nested.roots[..1], Instant::now(), None)
        .expect("reserves x");
    let second = scan_jobs::reserve(&nested.state, &nested.roots[1..], Instant::now(), None)
        .expect("reserves x/inner");
    *first.libraries.lock().expect("unpoisoned") = vec![(nested.a, "A".to_string())];
    *second.libraries.lock().expect("unpoisoned") =
        vec![(nested.a, "A".to_string()), (nested.b, "B".to_string())];
    assert!(first.id < second.id, "the fixture: ids rise");
    Running::new(&first, None).at(Step::Hashing, 1, Some(2));
    Running::new(&second, None).at(Step::Writing, 1, Some(2));

    let body = fed(&nested.state, as_admin()).await;

    assert_eq!(
        body["libraries"],
        json!([
            { "id": nested.a, "step": "hashing" },
            { "id": nested.b, "step": "writing" },
        ]),
        "{body}"
    );
    let ids: Vec<u64> = body["jobs"]
        .as_array()
        .expect("a jobs array")
        .iter()
        .map(|sent| sent["id"].as_u64().expect("an id"))
        .collect();
    assert_eq!(ids, [first.id, second.id], "by id, each once");
}

/// An answer's top-level `libraries` and each job's own come from one read:
/// a job whose libraries are filled in while the feed answers, as a spawned
/// job's are, is in the top-level list exactly when its own list names it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_names_the_same_libraries_at_the_top_as_in_each_job() {
    let nested = nested("feed-one-read");
    let job =
        scan_jobs::reserve(&nested.state, &nested.roots, Instant::now(), None).expect("reserves");
    let stop = Arc::new(AtomicBool::new(false));
    let flipping = {
        let job = Arc::clone(&job);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut filled = false;
            while !stop.load(Ordering::SeqCst) {
                filled = !filled;
                *job.libraries.lock().expect("unpoisoned") = if filled {
                    vec![(70, "Manga".to_string())]
                } else {
                    Vec::new()
                };
            }
        })
    };

    // Collected rather than asserted in the loop, so a contradiction still
    // stops the thread above instead of leaving it spinning for the rest of
    // the run.
    let mut checked = 0;
    let mut contradicting = Vec::new();
    for _ in 0..2000 {
        let body = fed(&nested.state, as_admin()).await;
        if !lists(&body, &job) {
            continue;
        }
        let top: Vec<i64> = body["libraries"]
            .as_array()
            .expect("a libraries array")
            .iter()
            .map(|held| held["id"].as_i64().expect("an id"))
            .collect();
        let in_jobs: std::collections::BTreeSet<i64> = body["jobs"]
            .as_array()
            .expect("a jobs array")
            .iter()
            .filter(|sent| sent["kind"] == "scan")
            .flat_map(|sent| sent["libraries"].as_array().expect("a libraries array"))
            .map(|library| library["id"].as_i64().expect("an id"))
            .collect();
        if top != in_jobs.into_iter().collect::<Vec<_>>() {
            contradicting.push(body);
        }
        checked += 1;
    }
    stop.store(true, Ordering::SeqCst);
    flipping.join().expect("the flipping thread ends");

    assert!(
        contradicting.is_empty(),
        "{} of {checked} answers name other libraries at the top than in their scan jobs, the first: {}",
        contradicting.len(),
        contradicting[0]
    );
    assert_eq!(checked, 2000, "answers that list the job, each checked");
}

/// Statements run on the traced connection.
static STATEMENTS: AtomicUsize = AtomicUsize::new(0);

fn counting(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(..) = event {
        STATEMENTS.fetch_add(1, Ordering::SeqCst);
    }
}

/// Spec §7, "The store cost of a poll", and §8: the handler reads memory
/// alone, in each form. An in-memory store has one connection, which every
/// read and write goes through, so a trace on it sees any statement.
#[tokio::test]
async fn the_feeds_handler_runs_no_statement() {
    let nested = nested("feed-no-statement");
    let job = nested.a_job();
    Running::new(&job, None).at(Step::Hashing, 1, Some(2));
    nested
        .state
        .library
        .conn()
        .trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(counting),
        )
        .expect("installs the trace");
    nested
        .state
        .library
        .conn()
        .query_row("SELECT 1", [], |_| Ok(()))
        .expect("a control statement");
    let control = STATEMENTS.swap(0, Ordering::SeqCst);

    let admin = fed(&nested.state, as_admin()).await;
    let reader = fed(
        &nested.state,
        signed_in(a_reader_of(vec![nested.b]), crate::auth::By::Cookie),
    )
    .await;
    let statements = STATEMENTS.load(Ordering::SeqCst);
    nested
        .state
        .library
        .conn()
        .trace_v2(
            rusqlite::trace::TraceEventCodes::empty(),
            None::<fn(rusqlite::trace::TraceEvent<'_>)>,
        )
        .expect("clears the trace");

    assert_eq!(control, 1, "the fixture: the trace counts a statement");
    assert_eq!(
        (
            admin["jobs"].as_array().map(Vec::len),
            reader["libraries"].as_array().map(Vec::len)
        ),
        (Some(1), Some(1)),
        "the fixture: each form had a job to send"
    );
    assert_eq!(statements, 0, "the feed ran a statement");
}

/// Whether `body`'s jobs name `job`.
fn lists(body: &serde_json::Value, job: &Job) -> bool {
    body["jobs"]
        .as_array()
        .expect("a jobs array")
        .iter()
        .any(|sent| sent["id"] == job.id)
}

fn ended(body: &serde_json::Value) -> u64 {
    body["ended"].as_u64().expect("an ended marker")
}

/// Spec §10.4: `ended` moves when a job ends, not when it is reserved or
/// starts, so a page learns of a job that started and ended between two of
/// its reads. The count is this server's, so each move is exact.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_feeds_ended_marker_moves_when_a_job_ends() {
    let nested = nested("feed-ended-marker");
    let before = fed(&nested.state, as_admin()).await;
    let started = Instant::now();
    let job = scan_jobs::reserve(&nested.state, &nested.roots, started, None).expect("reserves");
    let (entered, entering) = std::sync::mpsc::channel::<()>();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let spawned = scan_jobs::spawn_job(
        Arc::clone(&nested.state),
        nested.roots.clone(),
        started,
        move |_, roots, _| {
            entered.send(()).expect("the test waits");
            let _ = released.recv();
            Ok(vec![json!({}); roots.len()])
        },
    );
    tokio::task::spawn_blocking(move || entering.recv())
        .await
        .expect("waits")
        .expect("the job started");
    let running = fed(&nested.state, as_admin()).await;
    release.send(()).expect("the job waits");
    spawned.await.expect("the job ends");
    let after = fed(&nested.state, as_admin()).await;

    let started = Instant::now();
    let brief = scan_jobs::reserve(&nested.state, &nested.roots, started, None).expect("reserves");
    scan_jobs::spawn_job(
        Arc::clone(&nested.state),
        nested.roots.clone(),
        started,
        |_, roots, _| Ok(vec![json!({}); roots.len()]),
    )
    .await
    .expect("the job ends");
    let later = fed(&nested.state, as_admin()).await;

    assert!(lists(&running, &job), "the fixture: {running}");
    assert_eq!(
        [
            ended(&before),
            ended(&running),
            ended(&after),
            ended(&later)
        ],
        [0, 0, 1, 2],
        "(before the job, while it ran, once it ended, once a job started and \
         ended between two reads): {before} {running} {after} {later}"
    );
    assert!(!lists(&after, &job), "{after}");
    assert!(!lists(&later, &brief), "{later}");
}

/// Spec §7, "Cancel endpoint": pressing it twice is harmless.
#[tokio::test]
async fn cancel_pressed_twice_answers_cancelling_both_times() {
    let nested = nested("cancel-twice");
    let job = nested.a_job();

    let first = cancelled(
        &nested.state,
        an_administrator(),
        crate::auth::By::Cookie,
        job.id,
    )
    .await;
    let second = cancelled(
        &nested.state,
        an_administrator(),
        crate::auth::By::Cookie,
        job.id,
    )
    .await;

    let answer = (
        StatusCode::ACCEPTED,
        json!({ "job": job.id, "cancelling": true }),
    );
    assert_eq!(first, answer);
    assert_eq!(second, answer);
    assert!(job.cancelling(), "the job was asked to stop");
    assert!(
        fed(&nested.state, as_admin()).await["jobs"][0]["cancelling"] == true,
        "the feed says it is cancelling"
    );
}

/// Spec §7, "Cancel endpoint": a job that has ended answers how it ended,
/// and is not asked to stop. Each job holds a directory of its own, as a
/// later job of the same directory takes its slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_an_ended_job_answers_how_it_ended() {
    let nested = nested("cancel-ended");
    let (x, inner) = (nested.roots[..1].to_vec(), nested.roots[1..].to_vec());
    let started = Instant::now();
    let finished = scan_jobs::reserve(&nested.state, &x, started, None).expect("reserves");
    scan_jobs::spawn_job(Arc::clone(&nested.state), x, started, |_, roots, _| {
        Ok(vec![json!({}); roots.len()])
    })
    .await
    .expect("the job ends");

    // Cancelled while it waits behind a holder of the job lock.
    let held = test_support::holding_the_job(&nested.state);
    let started = Instant::now();
    let stopped = scan_jobs::reserve(&nested.state, &inner, started, None).expect("reserves");
    stopped.request_cancel();
    scan_jobs::spawn(Arc::clone(&nested.state), inner, started, Reread::Dir)
        .await
        .expect("the job ends");
    drop(held);

    let admin = |id| {
        cancelled(
            &nested.state,
            an_administrator(),
            crate::auth::By::Cookie,
            id,
        )
    };
    assert_eq!(
        admin(finished.id).await,
        (
            StatusCode::OK,
            json!({ "job": finished.id, "ended": "finished" })
        )
    );
    assert!(!finished.cancelling(), "an ended job is not asked to stop");
    assert_eq!(
        admin(stopped.id).await,
        (
            StatusCode::OK,
            json!({ "job": stopped.id, "ended": "cancelled", "step": "waiting" })
        )
    );
}

#[tokio::test]
async fn cancelling_an_unknown_job_is_404() {
    let nested = nested("cancel-unknown");
    let job = nested.a_job();

    let (status, body) = cancelled(
        &nested.state,
        an_administrator(),
        crate::auth::By::Cookie,
        job.id + 1,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(keys_of(&body), ["error"], "{body}");
    assert!(!job.cancelling(), "no other job is asked to stop");
}

/// Cancel is an administrator's, and a refused press asks nothing.
#[tokio::test]
async fn a_reader_cannot_cancel() {
    let nested = nested("cancel-reader");
    let job = nested.a_job();

    let (status, body) = cancelled(
        &nested.state,
        a_reader_of(vec![nested.a, nested.b]),
        crate::auth::By::Cookie,
        job.id,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!job.cancelling(), "the job was not asked to stop");
}

/// Libraries `Manga` and `Comics`, each with matching on and a directory of
/// its own holding one volume, walked and derived. No pass has run.
struct Matched {
    /// The store, served with a pass that never starts.
    store: crate::ServeState,
    fixture: test_support::Fixture,
    manga: i64,
    comics: i64,
    roots: Vec<scan::Root>,
    /// Every unit due.
    due: Vec<Unit>,
}

fn matched(tag: &str) -> Matched {
    let fixture = test_support::Fixture::new(tag);
    let library = test_support::library();
    let (manga, comics, roots) = {
        let conn = library.conn();
        for (seed, name) in [(0, "Manga"), (10, "Comics")] {
            let top = fixture.join(name.to_lowercase());
            let series = top.join(name);
            std::fs::create_dir_all(&series).expect("makes the series directory");
            cbz_pages(&series, &format!("{name} v01.cbz"), 3, seed);
            add_root_checked_in(&conn, &top, InLibrary::Named(name)).expect("adds the directory");
        }
        let roots = scan::roots(&conn).expect("the directories");
        for root in &roots {
            scan::scan_root(&conn, root).expect("walks it");
        }
        derive::derive(&conn).expect("derives");
        let id = |name: &str| {
            libraries::id_of(&conn, name)
                .expect("reads")
                .expect("the library")
        };
        (id("Manga"), id("Comics"), roots)
    };
    let due =
        fingerprints::due(&library.conn(), fingerprint::VERSION, None).expect("lists what is due");
    assert_eq!(due.len(), 2, "the fixture: each library's volume is due");
    let store = crate::ServeState {
        library,
        ..test_support::state(fixture.path())
    };
    Matched {
        store,
        fixture,
        manga,
        comics,
        roots,
        due,
    }
}

impl Matched {
    /// The directory `library` holds.
    fn root_of(&self, library: i64) -> scan::Root {
        self.roots
            .iter()
            .find(|root| root.libraries.contains(&library))
            .cloned()
            .expect("the library's directory")
    }

    /// A state over the store whose pass reads on one worker, not yet kicked.
    fn with_a_pass(&self) -> Arc<crate::ServeState> {
        test_support::with_a_pass(&self.store, &self.fixture, one_worker()).1
    }

    /// A state over the store whose pass is held in its first file.
    fn held(&self) -> HeldPass {
        HeldPass::over(&self.store, &self.fixture)
    }
}

/// `state`'s pass's job, which it must have.
fn pass_of(state: &crate::ServeState) -> Arc<Job> {
    state.fingerprints.job().expect("the pass showed a job")
}

fn settled(state: &crate::ServeState) {
    assert!(state.fingerprints.settle(SETTLE), "the pass ends");
}

/// Plan, "Feed shape": the pass is a job of its own kind beside a scan job.
/// Both libraries have matching on and the scan holds Comics alone, so a held
/// list that took the pass would add Manga to it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_admin_sees_the_pass_as_a_job_of_its_own_kind() {
    let matched = matched("feed-pass-kind");
    let held = matched.held();
    let state = Arc::clone(&held.state);
    let pass = pass_of(&state);
    let comics = vec![matched.root_of(matched.comics)];
    let started = Instant::now();
    let scan = scan_jobs::reserve(&state, &comics, started, None).expect("reserves");
    let (entered, entering) = mpsc::channel::<()>();
    let (release_scan, released) = mpsc::channel::<()>();
    let spawned = scan_jobs::spawn_job(Arc::clone(&state), comics, started, move |_, roots, _| {
        entered.send(()).expect("the test waits");
        let _ = released.recv();
        Ok(vec![json!({}); roots.len()])
    });
    tokio::task::spawn_blocking(move || entering.recv())
        .await
        .expect("waits")
        .expect("the scan started");

    let body = fed(&state, as_admin()).await;
    release_scan.send(()).expect("the scan waits");
    spawned.await.expect("the scan ends");
    held.let_go();

    assert_eq!(
        body["libraries"],
        json!([{ "id": matched.comics, "step": "waiting" }]),
        "{body}"
    );
    let jobs = body["jobs"].as_array().expect("a jobs array");
    let sent = |id: u64| {
        jobs.iter()
            .find(|job| job["id"] == id)
            .unwrap_or_else(|| panic!("job {id} is sent: {body}"))
    };
    assert_eq!(jobs.len(), 2, "{body}");
    let sent_pass = sent(pass.id);
    assert_eq!(
        keys_of(sent_pass),
        [
            "cancelling",
            "dir",
            "dirs",
            "done",
            "elapsed_secs",
            "id",
            "kind",
            "libraries",
            "of",
            "ready",
            "step"
        ],
        "{body}"
    );
    assert_eq!(sent_pass["kind"], "fingerprint", "{body}");
    assert_eq!(sent_pass["step"], "fingerprinting", "{body}");
    assert_eq!(sent_pass["done"], 0, "{body}");
    assert_eq!(sent_pass["of"], matched.due.len(), "{body}");
    assert_eq!(sent_pass["dir"], serde_json::Value::Null, "{body}");
    assert_eq!(sent_pass["dirs"], json!([]), "{body}");
    assert_eq!(
        sent_pass["libraries"],
        json!([
            { "id": matched.comics, "name": "Comics" },
            { "id": matched.manga, "name": "Manga" },
        ]),
        "{body}"
    );
    assert!(sent_pass["elapsed_secs"].is_f64(), "{body}");
    assert_eq!(sent_pass["cancelling"], false, "{body}");
    assert_eq!(sent(scan.id)["kind"], "scan", "{body}");
}

/// Global Constraints, "Nothing a reader sees changes": a reader granted both
/// libraries the pass reads is sent neither while it reads.
#[tokio::test]
async fn a_reader_is_sent_nothing_of_the_pass() {
    let matched = matched("feed-pass-reader");
    let held = matched.held();
    let admin = fed(&held.state, as_admin()).await;
    let reader = fed(
        &held.state,
        signed_in(
            a_reader_of(vec![matched.manga, matched.comics]),
            crate::auth::By::Cookie,
        ),
    )
    .await;
    held.let_go();

    assert_eq!(
        admin["jobs"].as_array().map(Vec::len),
        Some(1),
        "the fixture: the pass is in the feed: {admin}"
    );
    assert_eq!(
        keys_of(&reader),
        ["ended", "libraries", "written"],
        "{reader}"
    );
    assert_eq!(reader["libraries"], json!([]), "{reader}");
}

/// `ended` tells a library's page that a scan it showed has ended, and a pass
/// is no scan, so its end leaves `ended` where it was.
#[tokio::test]
async fn ended_stays_put_when_a_pass_ends() {
    let matched = matched("feed-pass-ended");
    let state = matched.with_a_pass();
    let before = ended(&fed(&state, as_admin()).await);

    fingerprint_pass::kick(&state);
    settled(&state);
    let after = ended(&fed(&state, as_admin()).await);

    assert_eq!(
        pass_of(&state).ended(),
        Some(JobEnded::Finished),
        "the fixture: a pass ran to its end"
    );
    assert_eq!(after, before);
}

/// Spec §7, "Cancel endpoint", for the pass: asked while it reads, it stops
/// with nothing stored, then answers how it ended; the next pass is its own
/// and reads every unit.
#[tokio::test]
async fn cancel_stops_the_pass_and_then_says_how_it_ended() {
    let matched = matched("cancel-pass");
    let held = matched.held();
    let state = Arc::clone(&held.state);
    let pass = pass_of(&state);
    let admin = || cancelled(&state, an_administrator(), crate::auth::By::Cookie, pass.id);

    let asked = admin().await;
    held.let_go();
    let answered = admin().await;
    let stored: Vec<bool> = matched
        .due
        .iter()
        .map(|unit| test_support::fingerprinted(&state.library, unit))
        .collect();
    fingerprint_pass::kick(&state);
    settled(&state);
    let next = pass_of(&state);

    assert_eq!(
        asked,
        (
            StatusCode::ACCEPTED,
            json!({ "job": pass.id, "cancelling": true })
        )
    );
    assert_eq!(
        answered,
        (
            StatusCode::OK,
            json!({ "job": pass.id, "ended": "cancelled", "step": "fingerprinting" })
        )
    );
    assert_eq!(stored, [false, false], "the cancelled pass stored nothing");
    assert_ne!(next.id, pass.id, "a pass of its own");
    assert_eq!(next.ended(), Some(JobEnded::Finished));
    for unit in &matched.due {
        assert!(
            test_support::fingerprinted(&state.library, unit),
            "{} is read",
            unit.rel_path
        );
    }
}

/// A pass that has ended leaves the feed, or the web would show its line and
/// Cancel for good; Cancel still answers how it ended.
#[tokio::test]
async fn an_ended_pass_leaves_the_feed() {
    let matched = matched("feed-pass-leaves");
    let state = matched.with_a_pass();
    fingerprint_pass::kick(&state);
    settled(&state);
    let pass = pass_of(&state);

    let body = fed(&state, as_admin()).await;
    let answer = cancelled(&state, an_administrator(), crate::auth::By::Cookie, pass.id).await;

    assert_eq!(
        pass.ended(),
        Some(JobEnded::Finished),
        "the fixture: the pass has ended"
    );
    assert_eq!(body["jobs"], json!([]), "{body}");
    assert_eq!(
        answer,
        (
            StatusCode::OK,
            json!({ "job": pass.id, "ended": "finished" })
        )
    );
    assert!(!pass.cancelling(), "an ended pass is not asked to stop");
}

/// Plan, "Feed job": the match pass is a job of its own kind, sent as the
/// plan's shape has it, and holds no library, so the held list is what it was
/// before the pass. Its Cancel, pressed through the route after a kick, stops
/// the pass it lands on: no pass of its own starts, and the volume the pass had
/// not reached is not looked up.
#[tokio::test]
async fn the_feed_shows_the_match_pass_and_its_cancel_stops_it() {
    let matched = matched("feed-match-pass");
    let fingerprinting = matched.with_a_pass();
    fingerprint_pass::kick(&fingerprinting);
    settled(&fingerprinting);
    let (hold, arrived, release) = test_support::gate();
    let armed = AtomicBool::new(true);
    let settings = match_pass::PassSettings {
        workers: 1,
        after_lookup: Some(Arc::new(move |_: &str| {
            if armed.swap(false, Ordering::SeqCst) {
                hold();
            }
        })),
        ..match_pass::PassSettings::default()
    };
    let state = test_support::with_a_match_pass(
        Arc::clone(&matched.store.library),
        &matched.fixture,
        settings,
    );
    let router =
        test_support::authenticating(crate::build_router(Arc::clone(&state)), &state.library);
    let before = fed(&state, as_admin()).await;

    match_pass::kick(&state);
    arrived
        .recv_timeout(SETTLE)
        .expect("the pass has looked up its first volume");
    let job = state.matches.job().expect("the pass shows a job");
    let body = fed(&state, as_admin()).await;
    match_pass::kick(&state);
    let pressed = test_support::send_json(
        &router,
        Method::POST,
        &format!("/api/library/scans/{}/cancel", job.id),
        &json!({}),
    )
    .await
    .status();
    release.send(()).expect("the worker waits for the word");
    assert!(state.matches.settle(SETTLE), "the pass ends");
    let answered = cancelled(&state, an_administrator(), crate::auth::By::Cookie, job.id).await;
    let looked_up = matches::looked_up(&state.library.conn()).expect("reads the lookups");

    assert_eq!(
        before["libraries"],
        json!([]),
        "the fixture: nothing holds a library"
    );
    assert_eq!(body["libraries"], before["libraries"], "{body}");
    let jobs = body["jobs"].as_array().expect("a jobs array");
    assert_eq!(jobs.len(), 1, "{body}");
    let mut sent = jobs[0].clone();
    assert!(sent["elapsed_secs"].is_f64(), "{body}");
    sent.as_object_mut()
        .expect("an object")
        .remove("elapsed_secs");
    assert_eq!(
        sent,
        json!({
            "id": job.id,
            "kind": "match",
            "step": "matching",
            "done": 0,
            "of": 2,
            "dir": null,
            "dirs": [],
            "libraries": [
                { "id": matched.comics, "name": "Comics" },
                { "id": matched.manga, "name": "Manga" },
            ],
            "cancelling": false,
            "ready": null,
        }),
    );
    assert_eq!(pressed, StatusCode::ACCEPTED);
    assert_eq!(job.ended(), Some(JobEnded::Cancelled(Step::Matching)));
    assert_eq!(
        answered,
        (
            StatusCode::OK,
            json!({ "job": job.id, "ended": "cancelled", "step": "matching" })
        )
    );
    assert_eq!(
        state.matches.job().map(|job| job.id),
        Some(job.id),
        "no pass of its own"
    );
    assert_eq!(looked_up.len(), 1, "one volume looked up: {looked_up:?}");
}

/// Design O314 §5, ruling 9, "a directory you cancelled stays paused": a
/// cancel the route accepted clears the job's held mark at once, so a stop
/// before the job reads the flag and writes its pause leaves nothing the next
/// start takes up. The job here is held inside its walk, past its mark and
/// before its next read of the flag; once let go, it writes the pause itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_accepted_before_a_stop_leaves_nothing_the_next_start_takes_up() {
    let (_router, state, fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let roots = scan::roots(&state.library.conn()).expect("the directories");
    state
        .library
        .conn()
        .execute("UPDATE root SET scan_interval_secs = 0", [])
        .expect("sets both Off");
    let b = roots[1].clone();
    let held = |state: &crate::ServeState| -> (bool, bool) {
        state
            .library
            .conn()
            .query_row(
                "SELECT scan_held_at IS NOT NULL, paused_at IS NOT NULL FROM root WHERE id = ?1",
                [b.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("reads the directory")
    };
    let started = Instant::now();
    let job =
        scan_jobs::reserve(&state, std::slice::from_ref(&b), started, None).expect("reserves b");
    let (walking, walks) = mpsc::channel::<()>();
    let (release, released) = mpsc::channel::<()>();
    let running = scan_jobs::spawn_job(
        Arc::clone(&state),
        vec![b.clone()],
        started,
        move |state, roots, running| {
            scan_jobs::run_watched(
                &state.library,
                roots,
                Reread::Dir,
                &state.library_job,
                running,
                |conn, root, watch| {
                    let _ = walking.send(());
                    let _ = released.recv();
                    scan::scan_root_watched(conn, root, watch)
                },
                scan_jobs::job_derive,
            )
        },
    );
    walks
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the job walks b");
    let before = held(&state);

    let answer = cancelled(&state, an_administrator(), crate::auth::By::Cookie, job.id).await;
    let accepted = held(&state);
    // The stop: the next process, over the same store, while this one's job
    // has not yet read the flag.
    let next = Arc::new(crate::ServeState {
        library: Arc::new(test_support::the_store_elsewhere(&state)),
        cache_dir: fixture.join("next-covers"),
        ..test_support::state(fixture.path())
    });
    scan::interrupt_derives(&next.library.conn()).expect("rewrites the cut words");
    let now = taiyaku_library::schedule::now(&next.library.conn()).expect("the store's clock");
    let mut taken_up = Vec::new();
    crate::scheduler::Scheduler::new()
        .tick(&next, &now, |roots, _| {
            taken_up.push(roots.iter().map(|root| root.id).collect::<Vec<_>>());
        })
        .expect("the startup tick");
    drop(release);
    running.await.expect("the job ends");

    assert_eq!(
        answer,
        (
            StatusCode::ACCEPTED,
            json!({ "job": job.id, "cancelling": true })
        )
    );
    assert_eq!(
        (before, accepted, taken_up, held(&state)),
        (
            (true, false),
            (false, false),
            Vec::<Vec<i64>>::new(),
            (false, true)
        ),
        "(b held and paused: walking, once the cancel is accepted; what the next start takes up; \
         b once the job has read the flag)"
    );
}
