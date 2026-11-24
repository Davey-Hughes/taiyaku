//! The scan feed's `ended` for a scan another process ran. A `library scan`
//! from the command line writes the store this server reads, and an open page
//! learns of it only by reading again, which it does when `ended` moves (O521).
//!
//! **Three signs, read on the registry's refresher tick.** A walk writes each
//! directory's `last_scan_at` and `last_scan_status` and takes no lease; a
//! derive holds the derive lease from its first read to its last write; and
//! every write to what a served work is built from moves the store's structure
//! version, whichever process makes it. Each raises `ended` once: marks or a
//! version that change while this server runs no job of its own and its
//! `ended` stays put, or a lease another process held let go.
//!
//! **The version sees a derive that no look saw hold the lease**, one that
//! takes it and lets it go between two looks after its walk's marks raised
//! already. It moves for this server's own writes outside a job as well, and
//! each look that sees one costs an open page one more read.
//!
//! **Re-read only.** Nothing is drawn while the other process runs: it is no
//! job of this server's, so the feed lists nothing for it, and a page only
//! reads again once one of the signs has moved.

use std::sync::atomic::Ordering;

use taiyaku_library::schedule::ScanMark;

use crate::ServeState;

/// What one look reads.
#[derive(Debug)]
pub(crate) struct Reading {
    /// Every directory's `(id, last_scan_at, last_scan_status)`, by id.
    pub marks: Vec<ScanMark>,
    /// The store's structure version (`structure::version`), which every
    /// write to what a served work is built from moves, from any process.
    pub version: i64,
    /// Whether a connection other than this server's own holds the derive
    /// lease.
    pub elsewhere: bool,
    /// Whether one of this server's own jobs is running.
    pub busy: bool,
    /// The feed's `ended`.
    pub ended: u64,
}

/// What the looks before this one saw, kept by the refresher from one tick to
/// the next.
#[derive(Debug, Default)]
pub(crate) struct CliWatch {
    looked: bool,
    marks_seen: Vec<ScanMark>,
    version_seen: i64,
    ended_seen: u64,
    held_seen: bool,
}

impl CliWatch {
    /// Whether `now` says that a scan another process ran has ended since the
    /// last look; the caller raises `ended` by one when it does.
    pub(crate) fn decide(&mut self, now: Reading) -> bool {
        // A first look has nothing to compare with, and a page that opened
        // before it read what the store held then.
        if !self.looked {
            self.looked = true;
            self.marks_seen = now.marks;
            self.version_seen = now.version;
            self.ended_seen = now.ended;
            self.held_seen = now.elsewhere;
            return false;
        }
        // A derive writes while it holds the lease, and the look that sees the
        // lease let go raises whatever the marks and the version say, so a
        // look while it is held only notes the hold. A derive that no look
        // sees hold the lease is seen by the version its writes moved.
        if now.elsewhere {
            self.held_seen = true;
            return false;
        }
        // Marks or a version that change while this server runs a job, or as
        // its `ended` moves, may be its own job's, whose end raises `ended`
        // itself.
        let moved = now.marks != self.marks_seen || now.version != self.version_seen;
        let raise = self.held_seen || (moved && !now.busy && now.ended == self.ended_seen);
        self.marks_seen = now.marks;
        self.version_seen = now.version;
        // The caller's raise is no job of this server's ending, so the next
        // look must not read it as one and pass over what it finds.
        self.ended_seen = now.ended.wrapping_add(u64::from(raise));
        self.held_seen = false;
        raise
    }

    /// Read `state`, and raise its feed's `ended` by one when
    /// [`Self::decide`] says a scan another process ran has ended.
    ///
    /// **A read that fails is no look**: it changes nothing and prints
    /// nothing, since the refresh on the same tick reports a store it cannot
    /// read.
    pub(crate) fn look(&mut self, state: &ServeState) {
        let Some(now) = reading(state) else {
            return;
        };
        if self.decide(now) {
            state.jobs_ended.ended.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// What `state` reads now, or `None` when the store refuses the read.
fn reading(state: &ServeState) -> Option<Reading> {
    let (marks, version) = {
        let working = state.library.working().ok()?;
        (
            taiyaku_library::schedule::scan_marks(&working).ok()?,
            taiyaku_library::structure::version(&working).ok()?,
        )
    };
    let elsewhere = state.library.derive_held_elsewhere();
    let busy = !crate::scan_feed::running_jobs(state).is_empty();
    // Read last: a job is marked ended before it is counted, so a job of this
    // server's that wrote what was read above reads here as running or as
    // counted, and raises nothing. A raise for this server's own work needs
    // one of its jobs to end, from its mark to its count, across a look's
    // reads; its own lease to be taken as the probe reads it
    // (`Library::derive_held_elsewhere`); or a write of its own outside any
    // job.
    let ended = state.jobs_ended.ended.load(Ordering::SeqCst);
    Some(Reading {
        marks,
        version,
        elsewhere,
        busy,
        ended,
    })
}

#[cfg(test)]
mod tests;
