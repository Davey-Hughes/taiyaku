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
//! **Read, refresh, raise.** A tick reads the signs ([`reading`]), then
//! refreshes the registry, then decides and raises ([`CliWatch::raise_for`]).
//! The refresh starts after the read, so the tick never raises `ended` for a
//! write its refresh was not asked to read: when the refresh succeeds, a page
//! that reads again after the raise is served at least what the look saw. A
//! write that lands after the read is the next tick's to raise, by
//! [`CliWatch::decide`]'s rules, and that tick's refresh reads it before that
//! raise. This server's own raises refresh first too: a scan job's end and the
//! match pass's derive read the works again before `ended` moves
//! ([`crate::registry::refresh_before_a_job_says_it_wrote`], O587).
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
        // `raise_for` sets this again from its own add, since a job of this
        // server's may end between the read and the raise; `now` is all this
        // function knows, and its tests drive it with no counter.
        self.ended_seen = now.ended.wrapping_add(u64::from(raise));
        self.held_seen = false;
        raise
    }

    /// Raise `state`'s feed's `ended` by one when [`Self::decide`] says, from
    /// `now`, that a scan another process ran has ended.
    ///
    /// **The count comes from the add.** The tick refreshes between its read of
    /// `now` and this raise, and a job of this server's may end in that gap and
    /// raise `ended` too. Counted from `now`, the watch would expect one less
    /// than `ended` holds, and the next look would take the difference for a
    /// job of this server's ending and pass over what it finds; counted from
    /// the add, the next look errs towards one more read.
    pub(crate) fn raise_for(&mut self, now: Reading, state: &ServeState) {
        if self.decide(now) {
            let raised_from = state.jobs_ended.ended.fetch_add(1, Ordering::SeqCst);
            self.ended_seen = raised_from.wrapping_add(1);
        }
    }
}

/// What `state` reads now, or `None` when the store refuses the read.
pub(crate) fn reading(state: &ServeState) -> Option<Reading> {
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
    // counted, and raises nothing. This server's own work costs an open page
    // one more read when one of its jobs ends, from its mark to its count,
    // across a look's reads; when its own lease is taken as the probe reads it;
    // when it writes outside any job; or when one of its jobs writes and ends
    // in the refresh after a read on which the tick raises, as its add counts
    // the end: the next look sees the write with `ended` equal to `ended_seen`.
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
