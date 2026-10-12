//! A counting semaphore, for work this server bounds by how many run at once
//! rather than by how many are asked for.
//!
//! A module of its own so that more than one caller can hold one. What a
//! caller bounds, and why its number is what it is, is said where that caller
//! builds its own: `cover.rs`'s `GENERATION_LIMIT` is one.
//!
//! `download.rs` does not use this. Its handlers are `async fn`s with a
//! runtime to await on, so it holds a `tokio::sync::Semaphore`. This one is
//! for code already on the blocking pool, where waiting is a parked thread.

use std::sync::{Condvar, Mutex};

/// A minimal counting semaphore, built on `Mutex` and `Condvar` so this stays
/// dependency-free.
#[derive(Debug)]
pub(crate) struct Semaphore {
    available: Mutex<usize>,
    released: Condvar,
}

impl Semaphore {
    pub(crate) const fn new(permits: usize) -> Self {
        Self {
            available: Mutex::new(permits),
            released: Condvar::new(),
        }
    }

    /// Block until a permit is free, then hold it until the returned guard
    /// drops.
    pub(crate) fn acquire(&self) -> SemaphoreGuard<'_> {
        let mut available = self
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *available == 0 {
            available = self
                .released
                .wait(available)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *available -= 1;
        SemaphoreGuard { semaphore: self }
    }
}

#[derive(Debug)]
pub(crate) struct SemaphoreGuard<'a> {
    semaphore: &'a Semaphore,
}

impl Drop for SemaphoreGuard<'_> {
    fn drop(&mut self) {
        let mut available = self
            .semaphore
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *available += 1;
        drop(available);
        self.semaphore.released.notify_one();
    }
}

#[cfg(test)]
mod tests;
