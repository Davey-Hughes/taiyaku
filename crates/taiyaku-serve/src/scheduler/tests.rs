use std::time::Instant;

use taiyaku_library::{derive, scan, schedule};

use super::Scheduler;
use crate::scan_jobs::Reread;
use crate::{ServeState, scan_jobs, test_support};

/// The clock a tick here is handed, unless it reads the store's own, so
/// `last_scan_at` decides what is due.
const NOW: &str = "2026-11-10 12:00:00";
/// Due under Automatic at [`NOW`], whose interval is 15 minutes.
const AN_HOUR_BEFORE: &str = "2026-11-10 11:00:00";

/// The fixture's two directories, A first by path.
fn a_and_b(state: &ServeState) -> (scan::Root, scan::Root) {
    let roots = scan::roots(&state.library.conn()).expect("lists the directories");
    let [a, b] = <[scan::Root; 2]>::try_from(roots).expect("the fixture holds two directories");
    assert!(a.path < b.path, "sanity: a is first by path");
    (a, b)
}

/// Set `root`'s stored interval (`None` is Automatic, 0 is Off) and when it
/// was last scanned.
fn schedule_of(state: &ServeState, root: &scan::Root, interval: Option<i64>, last_scan_at: &str) {
    state
        .library
        .conn()
        .execute(
            "UPDATE root SET scan_interval_secs = ?2, last_scan_at = ?3 WHERE id = ?1",
            rusqlite::params![root.id, interval, last_scan_at],
        )
        .expect("sets the schedule");
}

fn ids(roots: &[scan::Root]) -> Vec<i64> {
    roots.iter().map(|root| root.id).collect()
}

/// End the job reserved on `id` at `started` without a scan, as `spawn_job`
/// lands a report.
fn finish(state: &ServeState, id: i64, started: Instant) {
    scan_jobs::finish_reservation(
        &mut state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        id,
        started,
        serde_json::json!({}),
    );
}

/// The ids each call of a tick's `start` was handed, one entry per call.
fn started_ids(
    scheduler: &mut Scheduler,
    state: &ServeState,
    now: &str,
) -> Result<Vec<Vec<i64>>, String> {
    let mut calls = Vec::new();
    scheduler.tick(state, now, |roots, _| calls.push(ids(&roots)))?;
    Ok(calls)
}

/// Spec §4: the first tick is the startup scan.
#[test]
fn the_first_tick_takes_every_directory_that_is_not_off_due_or_not() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, NOW);
    schedule_of(&state, &b, Some(0), AN_HOUR_BEFORE);

    let calls = started_ids(&mut Scheduler::new(), &state, NOW).expect("the tick reads the store");

    assert_eq!(
        calls,
        [vec![a.id]],
        "a, not due, is scanned at startup, and b, which is Off, is not"
    );
}

/// Spec §4, "One job": the due directories are reserved under one start
/// instant, before the job starts, as no library's.
#[test]
fn a_tick_starts_its_due_directories_as_one_job() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, AN_HOUR_BEFORE);
    schedule_of(&state, &b, None, AN_HOUR_BEFORE);

    let mut calls = Vec::new();
    Scheduler { first: false }
        .tick(&state, NOW, |roots, started| {
            // Read when the job starts, so a reservation made only after it
            // reads as none.
            let scans = state
                .scans
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let held: Vec<_> = roots
                .iter()
                .map(|root| {
                    scans
                        .get(&root.id)
                        .map(|slot| (slot.started == started, slot.done, slot.library))
                })
                .collect();
            calls.push((ids(&roots), held));
        })
        .expect("the tick reads the store");

    let reserved = Some((true, false, None));
    assert_eq!(
        calls,
        [(vec![a.id, b.id], vec![reserved, reserved])],
        "one job of both, each slot reserved, not done, at the instant the job was handed, \
         with no library"
    );
}

/// Spec §4, "A busy directory": a due directory a press holds is passed over,
/// and the scan that held it writes the `last_scan_at` that satisfies its timer.
#[test]
fn a_tick_passes_over_a_directory_a_press_holds_and_the_press_satisfies_its_timer() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, AN_HOUR_BEFORE);
    schedule_of(&state, &b, None, AN_HOUR_BEFORE);
    let pressed = Instant::now();
    scan_jobs::reserve(
        &state,
        std::slice::from_ref(&a),
        pressed,
        Some(a.libraries[0]),
    )
    .expect("the press reserves a");

    let mut scheduler = Scheduler { first: false };
    let calls = started_ids(&mut scheduler, &state, NOW).expect("the tick reads the store");
    assert_eq!(calls, [vec![b.id]], "the tick starts b, and passes over a");

    let now = {
        let conn = state.library.conn();
        scan::scan_root(&conn, &a).expect("the press scans a");
        schedule::now(&conn).expect("reads the store's clock")
    };
    finish(&state, a.id, pressed);

    let calls = started_ids(&mut scheduler, &state, &now).expect("the tick reads the store");
    assert_eq!(
        calls,
        Vec::<Vec<i64>>::new(),
        "a was just scanned, so it is not due, and b's slot is still the batch's"
    );
}

/// Spec §4, "A change between ticks": each tick reads the schedule again.
#[test]
fn a_change_between_ticks_is_read_at_the_next_tick() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, AN_HOUR_BEFORE);
    schedule_of(&state, &b, None, NOW);

    let mut scheduler = Scheduler { first: false };

    // Each job ends without a scan, so a is still due by its last scan, and
    // only the Off below can be why the last tick passes over it.
    for said in [
        "the first tick starts a, which is due",
        "a's finished slot does not hold it, and a is still due",
    ] {
        let mut calls = Vec::new();
        scheduler
            .tick(&state, NOW, |roots, started| {
                calls.push((ids(&roots), started));
            })
            .expect("the tick reads the store");
        let jobs: Vec<Vec<i64>> = calls.iter().map(|(job, _)| job.clone()).collect();
        assert_eq!(jobs, [vec![a.id]], "{said}");
        finish(&state, a.id, calls[0].1);
    }
    let off = scan::RootPatch {
        scan_interval_secs: Some(Some(0)),
        ..scan::RootPatch::default()
    };
    assert!(
        scan::update_root(&state.library.conn(), a.id, &off).expect("sets a to Off"),
        "a's row is there to patch"
    );

    let calls = started_ids(&mut scheduler, &state, NOW).expect("the tick reads the store");
    assert_eq!(
        calls,
        Vec::<Vec<i64>>::new(),
        "a is Off now, so it is not scanned"
    );
}

/// The plan's own rule: the startup scan lasts until a tick has read the
/// store and reserved its batch or found nothing to start, so a first tick
/// whose read fails leaves the next tick the first.
#[test]
fn a_first_tick_that_cannot_read_the_store_leaves_the_next_tick_the_first() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, NOW);
    schedule_of(&state, &b, Some(0), NOW);
    let mut scheduler = Scheduler::new();

    state
        .library
        .conn()
        .execute_batch("ALTER TABLE library_root RENAME TO library_root_away")
        .expect("moves the table the read needs out of its way");
    let failed = started_ids(&mut scheduler, &state, NOW);
    state
        .library
        .conn()
        .execute_batch("ALTER TABLE library_root_away RENAME TO library_root")
        .expect("puts it back");
    assert!(failed.is_err(), "the read fails: {failed:?}");

    let calls = started_ids(&mut scheduler, &state, NOW).expect("the tick reads the store");
    assert_eq!(
        calls,
        [vec![a.id]],
        "the next tick is still the startup scan, so it takes a, which is not due"
    );
}

/// The plan's own rule, its other half: a first tick that reads the store and
/// finds nothing to start ends the startup scan, so a directory set to
/// Automatic later is scanned when it falls due, not at the next tick.
#[test]
fn a_first_tick_with_nothing_to_start_ends_the_startup_scan() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, Some(0), NOW);
    schedule_of(&state, &b, Some(0), NOW);
    let mut scheduler = Scheduler::new();

    let calls = started_ids(&mut scheduler, &state, NOW).expect("the tick reads the store");
    assert_eq!(
        calls,
        Vec::<Vec<i64>>::new(),
        "the fixture: both are Off, so the first tick starts nothing"
    );

    schedule_of(&state, &a, None, NOW);
    let calls = started_ids(&mut scheduler, &state, NOW).expect("the tick reads the store");
    assert_eq!(
        calls,
        Vec::<Vec<i64>>::new(),
        "a, Automatic now and scanned at NOW, is not due, and the startup scan is over"
    );
}

/// Each slot's start instant and whether it is done, in `roots`' order.
fn slots(state: &ServeState, roots: &[&scan::Root]) -> Vec<Option<(Instant, bool)>> {
    let scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    roots
        .iter()
        .map(|root| scans.get(&root.id).map(|slot| (slot.started, slot.done)))
        .collect()
}

/// `spawn` carries its `Scheduler` from tick to tick, so only its first tick
/// is the startup scan. The startup batch writes each directory's
/// `last_scan_at`, so under Automatic neither is due again for 15 minutes.
/// Clearing b's makes b due, so the tick that takes b again is a later one; a
/// startup tick would take a with it, in the same reservation.
#[tokio::test]
async fn spawn_runs_the_startup_scan_once_and_keeps_its_scheduler_between_ticks() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    let every = std::time::Duration::from_millis(20);
    let ticking = super::spawn(std::sync::Arc::clone(&state), every);

    let mut startup = None;
    for _ in 0..500 {
        let now = slots(&state, &[&a, &b]);
        if now.iter().all(|slot| slot.is_some_and(|(_, done)| done)) {
            startup = Some(now);
            break;
        }
        tokio::time::sleep(every).await;
    }
    let startup = startup.expect("the startup scan never finished both directories");
    state
        .library
        .conn()
        .execute("UPDATE root SET last_scan_at = NULL WHERE id = ?1", [b.id])
        .expect("makes b due");

    let mut later = None;
    for _ in 0..500 {
        let now = slots(&state, &[&a, &b]);
        if now[1].map(|(started, _)| started) != startup[1].map(|(started, _)| started) {
            later = Some(now);
            break;
        }
        tokio::time::sleep(every).await;
    }
    ticking.abort();
    let later = later.expect("no tick took b again once it was due");

    assert_eq!(
        later[0], startup[0],
        "the tick that took b again, now due, left a alone, so it was not a startup tick"
    );
}

/// `root`'s stored word once it reads `wanted`, or the last word read when
/// ten seconds pass first.
async fn until_stored(state: &ServeState, root: &scan::Root, wanted: &str) -> Option<String> {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let seen = stored(state, root);
        if seen.as_deref() == Some(wanted) || Instant::now() > deadline {
            return seen;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

fn stored(state: &ServeState, root: &scan::Root) -> Option<String> {
    state
        .library
        .conn()
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("reads the status")
}

/// A startup scan queued behind another startup scan (a tick after a panic
/// is a startup tick, and takes a directory taken off Off meanwhile) has not
/// started: every page reads `waiting` for its directory until the first
/// ends, then `ok`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_startup_scan_queued_behind_another_reads_waiting_until_it_runs() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, NOW);
    schedule_of(&state, &b, Some(0), NOW);
    let (said, heard) = std::sync::mpsc::channel::<()>();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let mut released = Some(released);
    let mut first = Vec::new();
    Scheduler::new()
        .tick(&state, NOW, |roots, started| {
            let (said, released) = (said.clone(), released.take().expect("one batch"));
            first.push(scan_jobs::spawn_job(
                std::sync::Arc::clone(&state),
                roots,
                started,
                move |state, roots, running| {
                    scan_jobs::run_watched(
                        &state.library,
                        roots,
                        Reread::Batch,
                        &state.library_job,
                        running,
                        |conn, root, _| {
                            scan::scan_root(conn, root)
                                .map(taiyaku_library::progress::Outcome::Done)
                        },
                        |conn, _, _| {
                            let _ = said.send(());
                            let _ = released.recv();
                            taiyaku_library::derive::derive(conn)
                                .map(taiyaku_library::progress::Outcome::Done)
                        },
                    )
                },
            ));
        })
        .expect("the first startup tick");
    heard
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the first startup scan derives");
    schedule_of(&state, &b, None, NOW);

    let mut second = Vec::new();
    Scheduler::new()
        .tick(&state, NOW, |roots, started| {
            second.push(scan_jobs::spawn(
                std::sync::Arc::clone(&state),
                roots,
                started,
                Reread::Batch,
            ));
        })
        .expect("the second startup tick");
    let queued = test_support::last_scan_words(&router, b.id).await;
    drop(release);
    for job in first.into_iter().chain(second) {
        job.await.expect("a startup scan ends");
    }
    let after = test_support::last_scan_words(&router, b.id).await;

    assert_eq!(
        [queued, after],
        [
            test_support::every_page_reads(true, "waiting"),
            test_support::every_page_reads(false, "ok"),
        ],
        "(queued, after)"
    );
}

/// A stop while a scan waits for the job lock leaves its `waiting` stored,
/// which the next start reads as `interrupted` (O186), and that start's
/// scan of the directory heals it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_cut_while_queued_reads_interrupted_until_the_next_startup_scan() {
    let (_router, state, fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let (_, b) = a_and_b(&state);
    let held = test_support::holding_the_job(&state);
    let started = Instant::now();
    scan_jobs::reserve(&state, std::slice::from_ref(&b), started, None).expect("reserves b");
    let cut = scan_jobs::spawn(
        std::sync::Arc::clone(&state),
        vec![b.clone()],
        started,
        Reread::Dir,
    );
    let queued = until_stored(&state, &b, "waiting").await;

    // The next process: its own state over the same store, whose job lock the
    // cut scan never takes.
    let next = std::sync::Arc::new(ServeState {
        library: std::sync::Arc::new(test_support::the_store_elsewhere(&state)),
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("next-covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        fingerprints: Default::default(),
        dirs: Vec::new(),
    });
    super::interrupt_cut_derives(&next.library);
    let at_start = stored(&next, &b);
    let now = schedule::now(&next.library.conn()).expect("the store's clock");
    let mut startup = Vec::new();
    Scheduler::new()
        .tick(&next, &now, |roots, started| {
            startup.push(scan_jobs::spawn(
                std::sync::Arc::clone(&next),
                roots,
                started,
                Reread::Batch,
            ));
        })
        .expect("the startup tick");
    for job in startup {
        job.await.expect("the startup scan ends");
    }
    let healed = stored(&next, &b);
    drop(held);
    cut.await.expect("the cut scan's thread ends");

    assert_eq!(
        (queued, at_start, healed),
        (
            Some("waiting".to_string()),
            Some("interrupted".to_string()),
            Some("ok".to_string())
        ),
        "(queued, at the next start, after its startup scan)"
    );
}

/// Spec §3, Goal 5: the startup scan, which takes every directory not Off,
/// passes over one a cancel paused.
#[test]
fn the_first_tick_after_a_start_passes_over_a_paused_directory() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, NOW);
    schedule_of(&state, &b, None, NOW);
    scan::pause(&state.library.conn(), std::slice::from_ref(&a)).expect("pauses a");

    let calls = started_ids(&mut Scheduler::new(), &state, NOW).expect("the tick reads the store");

    assert_eq!(calls, [vec![b.id]], "b alone; a is paused");
}

/// Spec §3, Goal 5: a later tick passes over a paused directory however due.
#[test]
fn a_later_tick_passes_over_a_paused_directory_that_is_due() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, AN_HOUR_BEFORE);
    schedule_of(&state, &b, None, AN_HOUR_BEFORE);
    scan::pause(&state.library.conn(), std::slice::from_ref(&a)).expect("pauses a");

    let calls = started_ids(&mut Scheduler { first: false }, &state, NOW)
        .expect("the tick reads the store");

    assert_eq!(calls, [vec![b.id]], "b alone; a is paused though due");
}

/// `root`'s pause, last scan and stored word.
fn pause_and_last_scan(
    state: &ServeState,
    root: &scan::Root,
) -> (bool, Option<String>, Option<String>) {
    state
        .library
        .conn()
        .query_row(
            "SELECT paused_at IS NOT NULL, last_scan_at, last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("reads the directory")
}

/// Spec §7, "Pause": a directory that a cancelled job pauses after a tick
/// read the schedule, as one can before the batch holds the job lock, is not
/// walked: the batch answers it on its own slot, leaves its pause and its
/// last scan as they are, and walks the rest.
#[test]
fn a_batch_does_not_walk_a_directory_paused_after_its_tick_read_the_schedule() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    // Dated, so a walk of a would move its last scan.
    schedule_of(&state, &a, None, AN_HOUR_BEFORE);
    schedule_of(&state, &b, None, AN_HOUR_BEFORE);
    let mut taken = Vec::new();
    Scheduler::new()
        .tick(&state, NOW, |roots, _| taken = roots)
        .expect("the tick reads the store");
    assert_eq!(
        ids(&taken),
        [a.id, b.id],
        "sanity: the startup tick takes both"
    );
    scan::pause(&state.library.conn(), std::slice::from_ref(&a)).expect("pauses a");
    let job = test_support::a_planted_job();
    let mut walked = Vec::new();

    let reports = scan_jobs::run_watched(
        &state.library,
        &taken,
        Reread::Batch,
        &state.library_job,
        &scan_jobs::Running::new(&job, None),
        |conn, root, watch| {
            walked.push(root.id);
            scan::scan_root_watched(conn, root, watch)
        },
        |conn, fresh, watch| {
            derive::derive_watched(conn, derive::Documents::Reconcile, fresh, watch)
        },
    )
    .expect("the batch answers");

    assert_eq!(
        (walked, &reports[0], reports[1].get("error")),
        (
            vec![b.id],
            &serde_json::json!({ "error": scan_jobs::PAUSED }),
            None
        ),
        "(the walks entered, a's slot, b's error)"
    );
    assert_eq!(
        pause_and_last_scan(&state, &a),
        (
            true,
            Some(AN_HOUR_BEFORE.to_string()),
            Some("ok".to_string())
        ),
        "a's (pause, last scan, word)"
    );
}

/// Spec §9, O320: the batch's one derive freezes a directory paused after its
/// tick read the schedule, so a file the directory's last walk found waits
/// for Scan: no digest, no member.
#[test]
fn a_batch_hashes_none_of_a_directory_paused_after_its_tick_read_the_schedule() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (a, b) = a_and_b(&state);
    schedule_of(&state, &a, None, AN_HOUR_BEFORE);
    schedule_of(&state, &b, None, AN_HOUR_BEFORE);
    let mut taken = Vec::new();
    Scheduler::new()
        .tick(&state, NOW, |roots, _| taken = roots)
        .expect("the tick reads the store");
    assert_eq!(
        ids(&taken),
        [a.id, b.id],
        "sanity: the startup tick takes both"
    );
    let gamma = "Gamma/Gamma - v01";
    let gamma_path = a.path.join(gamma).display().to_string();
    test_support::write_pages(&a.path.join(gamma), 2);
    scan::scan_root(&state.library.conn(), &a).expect("walks a");
    scan::pause(&state.library.conn(), std::slice::from_ref(&a)).expect("pauses a");
    let gamma_of = |state: &ServeState| -> (Option<String>, i64) {
        let conn = state.library.conn();
        let digest = conn
            .query_row(
                "SELECT sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![a.id, gamma],
                |r| r.get(0),
            )
            .expect("a's walk recorded Gamma");
        let members = conn
            .query_row(
                "SELECT count(*) FROM member WHERE path = ?1",
                [&gamma_path],
                |r| r.get(0),
            )
            .expect("counts Gamma's members");
        (digest, members)
    };
    assert_eq!(
        gamma_of(&state),
        (None, 0),
        "the fixture: Gamma walked, not hashed"
    );
    let job = test_support::a_planted_job();

    scan_jobs::run_watched(
        &state.library,
        &taken,
        Reread::Batch,
        &state.library_job,
        &scan_jobs::Running::new(&job, None),
        scan::scan_root_watched,
        |conn, fresh, watch| {
            derive::derive_watched(conn, derive::Documents::Reconcile, fresh, watch)
        },
    )
    .expect("the batch answers");

    assert_eq!(gamma_of(&state), (None, 0), "Gamma's (digest, members)");
}
