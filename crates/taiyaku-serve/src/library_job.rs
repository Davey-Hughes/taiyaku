//! [`crate::ServeState::library_job`]'s lock and the flag beside it.
//!
//! A module of its own so that the raw mutex is private to it: a holder can
//! take the lock only through [`LibraryJob::lock`] or [`LibraryJob::try_lock`],
//! whose guard sets [`LibraryJob::is_held`] once the lock is taken and clears
//! it as it drops. The flag therefore says held for as long as a holder keeps
//! its guard, and misses a holder only in the instant after the lock is taken
//! and the instant before it is released.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

/// [`crate::ServeState::library_job`]'s lock, which these jobs take one at a
/// time across the whole store. A scan of any library (`scan_jobs::run`)
/// waits for it. A rule save (`config_api::save_rules`), a stamp
/// (`stamp::perform`), a move (`libraries_api::move_dir`) and a removal from a
/// library (`libraries_api::unlink_dir`) try it once and refuse in
/// `waits_for_the_job`'s words. A library's scan status reads
/// [`LibraryJob::is_held`] without taking the lock, so a directory's page
/// holds Move… and Remove from this library for as long as it is held.
#[derive(Debug, Default)]
pub struct LibraryJob {
    lock: Mutex<()>,
    held: AtomicBool,
}

/// The refusal of anything that asks for [`LibraryJob`]'s lock while it is
/// held: `to_do` is what was asked, in the words it is said in ("stamp
/// again").
///
/// It names no one holder because [`LibraryJob`] records only that the lock
/// is held, not who holds it: a sentence naming some of its takers is wrong
/// whenever one it leaves out holds the lock (O245).
pub(crate) fn waits_for_the_job(to_do: &str) -> String {
    format!("a scan or another library job is running; {to_do} once it has finished")
}

/// [`LibraryJob`]'s lock while held: it clears [`LibraryJob::is_held`] as it
/// is dropped, before the lock itself is released.
#[derive(Debug)]
pub struct JobGuard<'a> {
    _lock: MutexGuard<'a, ()>,
    held: &'a AtomicBool,
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.held.store(false, Ordering::SeqCst);
    }
}

impl LibraryJob {
    fn guard<'a>(&'a self, lock: MutexGuard<'a, ()>) -> JobGuard<'a> {
        self.held.store(true, Ordering::SeqCst);
        JobGuard {
            _lock: lock,
            held: &self.held,
        }
    }

    /// The lock, waiting for it, as [`std::sync::Mutex::lock`].
    ///
    /// # Errors
    ///
    /// As [`std::sync::Mutex::lock`]: poisoned, with the guard inside.
    pub fn lock(&self) -> std::sync::LockResult<JobGuard<'_>> {
        match self.lock.lock() {
            Ok(lock) => Ok(self.guard(lock)),
            Err(poisoned) => Err(PoisonError::new(self.guard(poisoned.into_inner()))),
        }
    }

    /// The lock if it is free, as [`std::sync::Mutex::try_lock`].
    ///
    /// # Errors
    ///
    /// As [`std::sync::Mutex::try_lock`]: held elsewhere, or poisoned with the
    /// guard inside.
    pub fn try_lock(&self) -> std::sync::TryLockResult<JobGuard<'_>> {
        match self.lock.try_lock() {
            Ok(lock) => Ok(self.guard(lock)),
            Err(TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
            Err(TryLockError::Poisoned(poisoned)) => Err(TryLockError::Poisoned(PoisonError::new(
                self.guard(poisoned.into_inner()),
            ))),
        }
    }

    /// Whether a holder panicked, as [`std::sync::Mutex::is_poisoned`].
    pub fn is_poisoned(&self) -> bool {
        self.lock.is_poisoned()
    }

    /// Whether something holds the lock now. A reading, stale as soon as it
    /// is taken, as any answer about a lock not held by the asker is.
    pub fn is_held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests;
