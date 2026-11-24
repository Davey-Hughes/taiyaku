//! The watch for a scan another process ran: what each look decides, and
//! what looks over a real store raise.

use std::sync::atomic::Ordering;

use taiyaku_library::Library;
use taiyaku_library::lease::Wait;

use super::{CliWatch, Reading};
use crate::ServeState;
use crate::test_support::{Fixture, router_over_a_live_library};

type Marks = Vec<(i64, Option<String>, Option<String>)>;

/// One directory, last scanned `at`.
fn marks(at: &str) -> Marks {
    vec![(1, Some(at.to_owned()), Some("ok".to_owned()))]
}

const BEFORE: &str = "2026-11-24 10:00:00";
const AFTER: &str = "2026-11-24 10:05:00";

/// A reading with no job of this server's running and no lease held elsewhere.
fn idle(marks: Marks, ended: u64) -> Reading {
    Reading {
        marks,
        version: 0,
        elsewhere: false,
        busy: false,
        ended,
    }
}

/// A reading with the lease held elsewhere.
fn held(marks: Marks, ended: u64) -> Reading {
    Reading {
        elsewhere: true,
        ..idle(marks, ended)
    }
}

/// A watch that has taken its first look, at `BEFORE` with `ended` 0. A look
/// after a raise reads `ended` 1, the caller's raise.
fn watching() -> CliWatch {
    let mut watch = CliWatch::default();
    assert!(!watch.decide(idle(marks(BEFORE), 0)), "a first look raised");
    watch
}

#[test]
fn the_first_look_never_raises() {
    for (first, what) in [
        (idle(marks(AFTER), 0), "marks"),
        (held(Vec::new(), 0), "a lease held elsewhere"),
        (idle(Vec::new(), 3), "a moved ended"),
        (
            Reading {
                version: 3,
                ..idle(Vec::new(), 0)
            },
            "a moved version",
        ),
        (
            Reading {
                marks: marks(AFTER),
                version: 3,
                elsewhere: true,
                busy: true,
                ended: 3,
            },
            "all of them",
        ),
    ] {
        assert!(
            !CliWatch::default().decide(first),
            "a first look with {what} raised"
        );
    }
}

#[test]
fn marks_another_process_wrote_raise_once() {
    let mut watch = watching();
    assert!(
        watch.decide(idle(marks(AFTER), 0)),
        "marks written while the server ran nothing raised nothing"
    );
    assert!(
        !watch.decide(idle(marks(AFTER), 1)),
        "the same marks raised a second time"
    );
}

#[test]
fn marks_written_while_the_server_runs_a_job_raise_nothing() {
    let mut watch = watching();
    let busy = Reading {
        busy: true,
        ..idle(marks(AFTER), 0)
    };
    assert!(
        !watch.decide(busy),
        "marks the server's own job wrote raised"
    );
    assert!(
        !watch.decide(idle(marks(AFTER), 0)),
        "the server's own marks raised once its job was gone"
    );
}

#[test]
fn marks_and_ended_both_moved_raise_nothing() {
    let mut watch = watching();
    assert!(
        !watch.decide(idle(marks(AFTER), 1)),
        "marks a job of the server's own wrote and counted between two looks raised"
    );
}

/// What another process wrote, as the structure version reads it.
fn written(ended: u64) -> Reading {
    Reading {
        version: 1,
        ..idle(marks(BEFORE), ended)
    }
}

#[test]
fn a_structure_another_process_wrote_raises_once() {
    let mut watch = watching();
    assert!(
        watch.decide(written(0)),
        "a version moved while the server ran nothing raised nothing"
    );
    assert!(
        !watch.decide(written(1)),
        "the same version raised a second time"
    );
}

#[test]
fn a_structure_written_while_the_server_runs_a_job_raises_nothing() {
    let mut watch = watching();
    let busy = Reading {
        busy: true,
        ..written(0)
    };
    assert!(
        !watch.decide(busy),
        "a version the server's own job moved raised"
    );
    assert!(
        !watch.decide(written(0)),
        "the server's own version raised once its job was gone"
    );
}

#[test]
fn a_structure_and_ended_both_moved_raise_nothing() {
    let mut watch = watching();
    assert!(
        !watch.decide(written(1)),
        "a version a job of the server's own moved and counted between two looks raised"
    );
}

#[test]
fn a_lease_held_elsewhere_raises_once_when_it_is_let_go() {
    let mut watch = watching();
    assert!(
        !watch.decide(held(marks(BEFORE), 0)),
        "a lease still held raised"
    );
    assert!(
        watch.decide(idle(marks(BEFORE), 0)),
        "a lease let go raised nothing"
    );
    assert!(
        !watch.decide(idle(marks(BEFORE), 1)),
        "a lease let go raised a second time"
    );
}

#[test]
fn a_lease_held_across_looks_raises_once() {
    let mut watch = watching();
    for look in 1..=3 {
        assert!(
            !watch.decide(held(marks(BEFORE), 0)),
            "a lease still held raised at look {look}"
        );
    }
    assert!(
        watch.decide(idle(marks(BEFORE), 0)),
        "a lease let go raised nothing"
    );
    assert!(
        !watch.decide(idle(marks(BEFORE), 1)),
        "a lease let go raised a second time"
    );
}

/// A server that starts, or a watch reset after a panic, while another
/// process's derive holds the lease.
#[test]
fn a_lease_held_at_the_first_look_raises_once_when_it_is_let_go() {
    let mut watch = CliWatch::default();
    assert!(!watch.decide(held(marks(BEFORE), 0)), "a first look raised");
    assert!(
        watch.decide(idle(marks(BEFORE), 0)),
        "a lease held at the first look and let go raised nothing"
    );
}

// --- over a store ------------------------------------------------------------

fn ended(state: &ServeState) -> u64 {
    state.jobs_ended.ended.load(Ordering::SeqCst)
}

/// A second `Library` over the fixture's store, which is another process as
/// far as the state's own connections and lease can tell.
fn another_handle(fixture: &Fixture) -> Library {
    Library::open(&fixture.path().join("db").join("library.db"))
        .expect("a second library over the same file")
}

/// Write the fixture's one directory's marks as a command-line walk does, with
/// no lease. A time of its own, since `datetime('now')` reads the same as the
/// fixture's own scan within a second.
fn mark_a_scan(library: &Library, at: &str) {
    let changed = library
        .conn()
        .execute(
            "UPDATE root SET last_scan_at = ?1, last_scan_status = 'ok' \
             WHERE id = (SELECT min(id) FROM root)",
            [at],
        )
        .expect("writes a directory's marks");
    assert_eq!(changed, 1, "the fixture has one directory");
}

#[test]
fn a_root_scanned_from_another_library_handle_moves_ended() {
    let (_router, state, fixture) = router_over_a_live_library();
    let mut watch = CliWatch::default();
    watch.look(&state);
    let before = ended(&state);

    mark_a_scan(&another_handle(&fixture), "2030-01-01 00:00:00");
    watch.look(&state);
    assert_eq!(
        ended(&state),
        before + 1,
        "a walk another process ended did not move ended by one"
    );
    watch.look(&state);
    assert_eq!(
        ended(&state),
        before + 1,
        "a look with nothing new moved ended"
    );
}

/// A command-line scan of several directories ends a walk at each. One that
/// ends the look after another's raise is still another process's.
#[test]
fn a_walk_that_ends_the_look_after_a_raise_moves_ended_again() {
    let (_router, state, fixture) = router_over_a_live_library();
    let other = another_handle(&fixture);
    let mut watch = CliWatch::default();
    watch.look(&state);
    let before = ended(&state);

    mark_a_scan(&other, "2030-01-01 00:00:00");
    watch.look(&state);
    mark_a_scan(&other, "2030-01-01 00:00:01");
    watch.look(&state);
    assert_eq!(
        ended(&state),
        before + 2,
        "the watch's own raise hid the walk that ended after it"
    );
}

/// A command-line scan writes its walk's marks, then takes the lease to
/// derive. A look between the two raises for the walk; a derive that then
/// takes and lets go of the lease between two looks is seen by what it wrote.
#[test]
fn a_derive_after_a_walk_already_raised_for_moves_ended_again() {
    let (_router, state, fixture) = router_over_a_live_library();
    let other = another_handle(&fixture);
    let mut watch = CliWatch::default();
    watch.look(&state);
    let before = ended(&state);

    mark_a_scan(&other, "2030-01-01 00:00:00");
    watch.look(&state);
    assert_eq!(ended(&state), before + 1, "the walk raised nothing");
    {
        let lease = other
            .derive_lease(Wait::No)
            .expect("takes the lease")
            .expect("nothing else holds it");
        let changed = lease.holding(|| {
            other
                .conn()
                .execute(
                    "UPDATE work SET title = title || '' WHERE id = (SELECT min(id) FROM work)",
                    [],
                )
                .expect("writes a work")
        });
        assert_eq!(changed, 1, "the fixture has a work");
    }
    watch.look(&state);
    assert_eq!(
        ended(&state),
        before + 2,
        "a derive that followed a raised walk and fit between two looks went unnoticed"
    );
}

#[test]
fn a_lease_another_handle_holds_moves_ended_when_dropped() {
    let (_router, state, fixture) = router_over_a_live_library();
    let mut watch = CliWatch::default();
    watch.look(&state);
    let before = ended(&state);

    let other = another_handle(&fixture);
    let lease = other
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("nothing else holds it");
    assert!(
        state.library.derive_held_elsewhere(),
        "another handle's lease did not read as held elsewhere"
    );
    watch.look(&state);
    assert_eq!(ended(&state), before, "a lease still held moved ended");

    drop(lease);
    watch.look(&state);
    assert_eq!(
        ended(&state),
        before + 1,
        "a lease let go did not move ended by one"
    );
    watch.look(&state);
    assert_eq!(
        ended(&state),
        before + 1,
        "a lease let go moved ended twice"
    );
}

#[test]
fn the_servers_own_lease_is_not_elsewhere() {
    let (_router, state, fixture) = router_over_a_live_library();
    let mut watch = CliWatch::default();
    watch.look(&state);
    let before = ended(&state);

    let lease = state
        .library
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("nothing else holds it");
    assert!(
        another_handle(&fixture).derive_held_elsewhere(),
        "the lease is not on the lock file, where another handle's probe would read it"
    );
    assert!(
        !state.library.derive_held_elsewhere(),
        "the server's own lease read as held elsewhere"
    );
    watch.look(&state);
    watch.look(&state);
    drop(lease);
    watch.look(&state);
    assert_eq!(
        ended(&state),
        before,
        "the server's own lease, held or let go, moved ended"
    );
}

#[test]
fn a_store_no_derive_has_touched_raises_nothing() {
    let (_router, state, fixture) = router_over_a_live_library();
    let lock = fixture.path().join("db").join("library.db.derive-lock");
    assert!(!lock.exists(), "the fixture took no lease");
    let before = ended(&state);
    let mut watch = CliWatch::default();
    for _ in 0..3 {
        watch.look(&state);
    }
    assert_eq!(ended(&state), before, "a store nobody scanned moved ended");
    assert!(!lock.exists(), "a look made the derive lock file");
}
