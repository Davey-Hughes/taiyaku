use super::*;

/// A guard from `try_lock`, as a rule save, a stamp and a move take one, says
/// the lock is held for as long as it lives, which a library's scan status
/// reads as `moves_held`, and free once it is dropped.
#[test]
fn a_try_lock_guard_says_the_lock_is_held_until_it_drops() {
    let job = LibraryJob::default();
    assert!(!job.is_held(), "free to begin with");
    let guard = job.try_lock().expect("the lock is free");
    assert!(job.is_held(), "held while the guard lives");
    assert!(
        matches!(job.try_lock(), Err(TryLockError::WouldBlock)),
        "a second holder is refused"
    );
    assert!(job.is_held(), "a refused try_lock leaves it held");
    drop(guard);
    assert!(!job.is_held(), "free once the guard drops");
}
