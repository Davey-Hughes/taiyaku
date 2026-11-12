//! The directory scans the server answers for without a press (spec
//! `docs/superpowers/specs/2026-11-10-pre-deploy-design.md`, §4): at its
//! start, the scans an earlier process cut as they waited or derived; then each
//! directory's scan on its schedule, every one not Off once at startup, as one
//! job per tick (O51).

use std::io::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use taiyaku_library::{scan, schedule};

use crate::ServeState;
use crate::scan_jobs::{self, Reread};

/// How often the scheduler reads the schedule. A `PATCH`, an add or a removal
/// is read at the next tick, so nothing has to wake it.
pub(crate) const TICK: Duration = Duration::from_secs(30);

// A directory is scanned up to a tick after it falls due; a tick as long as
// the shortest interval would let that lateness reach a whole interval.
const _: () = assert!(TICK.as_secs() < schedule::MIN_INTERVAL_SECS.cast_unsigned());

/// Rewrite every directory an earlier process left `waiting` or `deriving` to
/// `interrupted` (O186), and say so on stderr where one was.
///
/// **`serve` calls it before the bind.** Only this server writes either word,
/// and until it listens no scan of its own has begun, so each is a scan that a
/// stop, a panic or a refused write (O272) left without its last word; later,
/// it could rewrite a running scan's.
///
/// Never fatal, like the session sweeps beside its call. Not `eprintln!`,
/// which panics when nothing reads stderr.
pub(crate) fn interrupt_cut_derives(library: &taiyaku_library::Library) {
    let line = match taiyaku_library::scan::interrupt_derives(&library.conn()) {
        Ok(0) => return,
        Ok(marked) => {
            format!(
                "marked {marked} directory scan(s) cut while waiting or deriving as interrupted"
            )
        }
        Err(why) => {
            format!(
                "could not mark directory scans cut while waiting or deriving as interrupted: {why}"
            )
        }
    };
    let _ = writeln!(std::io::stderr(), "{line}");
}

/// What one tick takes, carried from tick to tick.
pub(crate) struct Scheduler {
    /// Whether the next tick is the startup scan, which takes every directory
    /// that is not Off, due or not (spec §4). It stays set until a tick has
    /// read the store and either reserved its batch or found nothing to start,
    /// so a failed read, or a batch a press's reservation refused, is tried
    /// again whole at the next tick.
    first: bool,
}

impl Scheduler {
    pub(crate) fn new() -> Self {
        Self { first: true }
    }

    /// Reserve, as one batch under no library, every directory this tick
    /// takes whose slot no scan holds, sorted by path, and hand the batch and
    /// its start instant to `start`. Nothing is started where none is left,
    /// or where [`scan_jobs::reserve`] refuses the batch: a press raced the
    /// tick, and the next tick takes it again.
    ///
    /// `now` is the store's clock in its `datetime()` shape
    /// ([`schedule::now`]); a test hands its own.
    ///
    /// # Errors
    ///
    /// The store's own words where the schedule could not be read.
    pub(crate) fn tick(
        &mut self,
        state: &ServeState,
        now: &str,
        mut start: impl FnMut(Vec<scan::Root>, Instant),
    ) -> Result<(), String> {
        // The guard drops with this statement: a job's `Library::working`
        // takes the same mutex where the library is in memory.
        let listed = schedule::scheduled(&state.library.conn(), now)?;
        let taken = listed
            .into_iter()
            .filter(|listed| self.first || listed.due)
            .map(|listed| listed.root)
            .collect();
        // A directory a scan holds is passed over, and that scan writes the
        // `last_scan_at` that satisfies its timer (spec §4).
        let batch = scan_jobs::not_held(state, taken);
        if batch.is_empty() {
            self.first = false;
            return Ok(());
        }
        let started = Instant::now();
        if scan_jobs::reserve(state, &batch, started, None).is_err() {
            return Ok(());
        }
        self.first = false;
        start(batch, started);
        Ok(())
    }
}

/// Run a [`Scheduler`]'s tick every `every`, the first at once, each on the
/// blocking pool since it reads the store, and start each batch with
/// [`scan_jobs::spawn`] as a [`Reread::Batch`].
///
/// **`serve` spawns it after the bind**, so the server answers while the
/// startup scan runs, **and after [`interrupt_cut_derives`]**, which would
/// otherwise rewrite this batch's `deriving` to `interrupted` (O186).
pub(crate) fn spawn(state: Arc<ServeState>, every: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(every);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut scheduler = Scheduler::new();
        loop {
            ticks.tick().await;
            let ticking = Arc::clone(&state);
            let ticked = tokio::task::spawn_blocking(move || {
                let mut scheduler = scheduler;
                // Read apart from the tick, whose own read takes the guard.
                let now = schedule::now(&ticking.library.conn());
                let outcome = now.and_then(|now| {
                    scheduler.tick(&ticking, &now, |roots, started| {
                        scan_jobs::spawn(Arc::clone(&ticking), roots, started, Reread::Batch);
                    })
                });
                (scheduler, outcome)
            })
            .await;
            let line = match ticked {
                Ok((kept, outcome)) => {
                    scheduler = kept;
                    outcome
                        .err()
                        .map(|why| format!("taiyaku: a scheduled scan could not start: {why}"))
                }
                Err(panicked) => {
                    // The panic took the scheduler with it, so the next tick
                    // is a startup tick.
                    scheduler = Scheduler::new();
                    Some(format!("taiyaku: a scheduled scan panicked: {panicked}"))
                }
            };
            if let Some(line) = line {
                // Not `eprintln!`, which panics when nothing reads stderr.
                let _ = writeln!(std::io::stderr(), "{line}");
            }
        }
    })
}

#[cfg(test)]
mod tests;
