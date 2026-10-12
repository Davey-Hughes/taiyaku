use super::*;
use std::sync::atomic::Ordering;

/// Important 6: two permits must allow two acquisitions to be held at
/// once — proving the semaphore does not serialise its callers down to
/// one, which would defeat the point of choosing more than one permit.
#[test]
fn a_semaphore_admits_its_configured_number_of_permits_at_once() {
    use std::sync::Barrier;
    use std::sync::atomic::AtomicUsize;

    let sem = Semaphore::new(2);
    let max_seen = AtomicUsize::new(0);
    let inside = AtomicUsize::new(0);
    // Sized to exactly the permit count: this can only complete if both
    // permits are held at the same instant.
    let both_held = Barrier::new(2);

    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let _permit = sem.acquire();
                let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                both_held.wait();
                inside.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });

    assert_eq!(
        max_seen.load(Ordering::SeqCst),
        2,
        "two permits must be usable at the same time"
    );
}

/// Acquiring takes a permit, and the count says so.
///
/// The direct, single-threaded half of the property the test below tries
/// to observe through two threads. It exists because that one **is** timing
/// dependent, despite having said otherwise for as long as it existed:
/// mutation testing flipped `*available -= 1` to `*available /= 1` — a
/// permit count that never decreases — and caught it on two runs and
/// missed it on a third. Nothing about the code changed between them.
///
/// With the count asserted here, that mutant dies the same way every run.
#[test]
fn acquiring_a_permit_decrements_the_available_count() {
    let sem = Semaphore::new(2);
    assert_eq!(*sem.available.lock().unwrap(), 2, "both permits free");

    let first = sem.acquire();
    assert_eq!(*sem.available.lock().unwrap(), 1, "one taken");

    let second = sem.acquire();
    assert_eq!(*sem.available.lock().unwrap(), 0, "both taken");

    drop(second);
    assert_eq!(*sem.available.lock().unwrap(), 1, "and released on drop");
    drop(first);
    assert_eq!(*sem.available.lock().unwrap(), 2);
}

/// Important 6: a third acquisition against a one-permit semaphore must
/// not succeed until the held permit is released — the property that
/// actually bounds what runs at once rather than merely allowing some.
///
/// **This test is timing dependent, and the comment here used to claim it
/// was not.** The claim was that the second thread's `acquire()` can only
/// return after `Drop` runs on the first thread's guard, which is strictly
/// after `released.store(true, ..)` — true while `acquire` blocks, and the
/// whole point is that a broken `acquire` does not. Once it returns early
/// there is no ordering at all between that store and this load, so the
/// mutant that stops permits decrementing is caught or missed by
/// scheduling. It was observed doing both.
///
/// Kept, because the blocking property is worth stating and this is the
/// only test that states it. What makes the mutant die reliably is
/// `acquiring_a_permit_decrements_the_available_count` above.
#[test]
fn a_second_acquisition_waits_until_the_held_permit_is_released() {
    use std::sync::Barrier;
    use std::sync::atomic::AtomicBool;

    let sem = Semaphore::new(1);
    let released = AtomicBool::new(false);
    let observed_after_release = AtomicBool::new(false);
    let holder_ready = Barrier::new(2);

    std::thread::scope(|scope| {
        scope.spawn(|| {
            let _permit = sem.acquire();
            holder_ready.wait();
            // The other thread is now blocked in `acquire()`. Releasing
            // happens when `_permit` drops, at the end of this closure —
            // strictly after this store, in program order.
            released.store(true, Ordering::SeqCst);
        });

        scope.spawn(|| {
            holder_ready.wait();
            let _permit = sem.acquire();
            observed_after_release.store(released.load(Ordering::SeqCst), Ordering::SeqCst);
        });
    });

    assert!(
        observed_after_release.load(Ordering::SeqCst),
        "the second acquire must not succeed before the first permit was released"
    );
}
