//! The scans the directory routes, the library routes and the scheduler start
//! in the background: one reservation in [`crate::ServeState::scans`], one job
//! on the blocking pool, and one report landed on each directory's own
//! reservation.
//!
//! **A scan of one directory is the scan of a library of one.** The directory route
//! reserves `std::slice::from_ref(&root)`, a library route reserves every
//! directory the library reaches ([`taiyaku_library::reach::walked_by`]), and
//! the scheduler's batch reserves what its tick takes: at startup every
//! directory not Off, then each one due, less any a scan already holds.
//! [`Reread`] is the only thing that tells them apart: in what the job reads
//! again under [`crate::ServeState::library_job`], in whether a directory that
//! changed or cannot be walked ends the job, and in the words of what it
//! answers where that fails.

use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use taiyaku_library::{derive, lease, libraries, scan};

use crate::ServeState;
use crate::config_api::finished_scan_json;

/// What a directory's report says of a scan that found its directory gone, or
/// another directory holding its id, once the scan could start.
pub(crate) const DIR_CHANGED: &str =
    "the directory was removed or changed while the scan waited to start, so nothing was scanned";

/// What each directory's report says of a scan that found the directories its
/// library shows changed: one added, removed or moved, or a pattern edit that
/// changed what the library reaches.
pub(crate) const CHANGED: &str = "the directories the library shows changed while the scan \
                                  waited to start, so nothing was scanned";

/// What a scan reads again once it holds the job lock, to know that the directories
/// it was asked to walk are still the ones it reserved.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Reread {
    /// Each directory asked for, which must still hold its id and its path.
    Dir,
    /// Library `id`, which must still reach the same directories
    /// ([`taiyaku_library::reach::walked_by`]), in the same number and order,
    /// with the same ids and paths.
    Library(i64),
    /// The scheduler's batch: each directory asked for, read again as
    /// [`Self::Dir`] reads it, and each answered on its own slot (spec §4,
    /// decision 9). Nobody pressed for it, so one directory that changed or
    /// cannot be walked must not hold back the others, or the derive: it is
    /// answered with its own error, a failed walk keeps the `failed` it
    /// records, and the one derive runs over what was walked, or not at all
    /// where nothing was.
    Batch,
}

impl Reread {
    /// Whether each directory is answered on its own slot, so that one that
    /// changed or cannot be walked is answered alone and the rest are walked
    /// and derived. Otherwise the first such directory ends the job, and one
    /// sentence answers every slot.
    fn answers_each_slot(self) -> bool {
        matches!(self, Self::Batch)
    }

    /// `asked` as the store holds it now, one entry per directory in order:
    /// the directory to walk, or the sentence its slot answers where it
    /// changed. The `Err` is the store's own words where a read failed, or,
    /// outside a batch, the sentence that ends a scan whose directories changed.
    ///
    /// Where the two readings agree the second is what is walked, with the
    /// settings the directories hold now.
    fn read_again(
        self,
        conn: &rusqlite::Connection,
        asked: &[scan::Root],
    ) -> Result<Vec<Result<scan::Root, String>>, String> {
        match self {
            Self::Dir | Self::Batch => {
                let held = scan::roots(conn)?;
                let now: Vec<Result<scan::Root, String>> = asked
                    .iter()
                    .map(|asked| {
                        held.iter()
                            .find(|now| now.id == asked.id && now.path == asked.path)
                            .cloned()
                            .ok_or_else(|| DIR_CHANGED.to_string())
                    })
                    .collect();
                match now.iter().find_map(|now| now.as_ref().err()) {
                    Some(changed) if !self.answers_each_slot() => Err(changed.clone()),
                    _ => Ok(now),
                }
            }
            Self::Library(id) => {
                if libraries::name_of(conn, id)?.is_none() {
                    return Err(CHANGED.to_string());
                }
                let now = taiyaku_library::reach::walked_by(&scan::roots(conn)?, id);
                let same = now.len() == asked.len()
                    && now
                        .iter()
                        .zip(asked)
                        .all(|(now, asked)| now.id == asked.id && now.path == asked.path);
                if same {
                    Ok(now.into_iter().map(Ok).collect())
                } else {
                    Err(CHANGED.to_string())
                }
            }
        }
    }

    /// What every directory's report says where the walk of `root` failed,
    /// or, for a batch, what `root`'s alone says.
    fn walk_failed(self, root: &scan::Root, why: &str) -> String {
        match self {
            Self::Dir | Self::Batch => why.to_string(),
            Self::Library(_) => format!("scanning {}: {why}", root.path.display()),
        }
    }

    /// What every walked directory's report says where the derive after the
    /// walks of `roots` failed.
    fn derive_failed(self, roots: &[scan::Root], why: &str) -> String {
        match self {
            Self::Dir => format!(
                "{} was scanned, but deriving its structure afterward failed: {why}",
                roots
                    .first()
                    .map_or_else(String::new, |root| root.path.display().to_string())
            ),
            Self::Library(_) => format!(
                "the directories the library shows were scanned, but deriving its structure \
                 afterward failed: {why}"
            ),
            Self::Batch => format!(
                "the directories a scheduled scan walked were scanned, but deriving their \
                 structure afterward failed: {why}"
            ),
        }
    }
}

/// Walk each of `asked` in turn and derive once: one report per directory, in
/// order, each its own walk with the one derive.
///
/// **Opens [`taiyaku_library::Library::working`].** A scan is the
/// longest thing this server does to a library, and holding the shared
/// connection's mutex for its length would stall every other request. Where
/// the library is file-backed `working` is a second connection, so an ordinary
/// request proceeds the whole time this runs.
///
/// **Holds `job`, [`crate::ServeState::library_job`], for its whole length,
/// walks and derive both.** See that field's own doc comment for the
/// interleaving this closes. Unlike a rules save's `try_lock`, this blocks,
/// reading `waiting` meanwhile ([`the_job_lock`]): a scan runs unwatched in
/// the background, so there is nobody to answer a refusal to, and its status
/// route says how long the wait and the scan took together.
///
/// **The directories are read again once the lock is held**, as `reread` says:
/// `asked` is what the request found, and a directory removed while the scan
/// waited may have had its id taken by another. Where they differ nothing is
/// walked; a batch walks the others ([`Reread::Batch`]).
///
/// **A walked directory reads `deriving` until the derive returns, or `waiting`
/// while it waits for the lease** (O186), and a refused walk keeps `refused`; a
/// stop between leaves what the next start rewrites to `interrupted`. A failed
/// derive turns each to `failed` ([`scan::mark_derive_failed`]). So does a failed
/// walk outside a batch, which ends the scan: nothing later is walked or derived.
///
/// # Errors
///
/// Returns the sentence every directory's report then carries: the store's own
/// words where a reading or the working connection failed and, outside a batch,
/// `reread`'s where the directories changed or a walk or the derive failed.
pub(crate) fn run(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    reread: Reread,
    job: &crate::LibraryJob,
    waiting: &dyn Fn(&[scan::Root], bool),
) -> Result<Vec<serde_json::Value>, String> {
    run_watched(
        library,
        asked,
        reread,
        job,
        waiting,
        scan::scan_root,
        derive::derive,
    )
}

/// [`run_watched`], telling no one that it waits, for the tests that do not
/// ask.
#[cfg(test)]
pub(crate) fn run_with(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    reread: Reread,
    job: &crate::LibraryJob,
    walk: impl FnMut(&rusqlite::Connection, &scan::Root) -> Result<scan::ScanReport, String>,
    derive: impl FnOnce(&rusqlite::Connection) -> Result<derive::DeriveReport, String>,
) -> Result<Vec<serde_json::Value>, String> {
    run_watched(library, asked, reread, job, &|_, _| {}, walk, derive)
}

/// [`run`]'s whole body, over a caller-supplied walk and derive, telling
/// `waiting` when it waits (`true`) and when it no longer does (`false`):
/// with every directory of `asked` for the job lock, and with the directories
/// whose walks recorded something (not refused) for the derive lease.
///
/// **Split out as `schema::migrate_with` is**: what is worth pinning is the
/// status each directory reads *during* the job, which only a walk or derive
/// of the test's own can read, and a derive that fails, which the real one
/// cannot be made to do without breaking the store.
pub(crate) fn run_watched(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    reread: Reread,
    job: &crate::LibraryJob,
    waiting: &dyn Fn(&[scan::Root], bool),
    mut walk: impl FnMut(&rusqlite::Connection, &scan::Root) -> Result<scan::ScanReport, String>,
    derive: impl FnOnce(&rusqlite::Connection) -> Result<derive::DeriveReport, String>,
) -> Result<Vec<serde_json::Value>, String> {
    let (_job, queued) = the_job_lock(library, asked, job, waiting);
    let working = match library.working() {
        Ok(working) => working,
        Err(why) => {
            // Nothing is walked, so each reads its last scan's word again, as
            // one `read_again` refuses does; left `waiting`, every page would
            // read it as queued until its next scan or the server's next start.
            let _ = scan::unmark_queued(&library.conn(), &queued);
            return Err(why);
        }
    };
    let _ = scan::unmark_queued(&working, &queued);
    let roots = reread.read_again(&working, asked)?;
    // One per directory asked for, in order: its walk's report, or the error
    // its slot answers.
    let mut slots = Vec::with_capacity(roots.len());
    let mut walked = Vec::with_capacity(roots.len());
    // The walks that recorded something, which the derive is for: a refused
    // walk recorded nothing, so its directory never waits for the lease and
    // keeps reading `refused`, the word that says to check the share.
    let mut recorded = Vec::with_capacity(roots.len());
    // Every status write below is best-effort, as `scan_root`'s own is: a
    // walk or derive that succeeded is not reported failed because a
    // bookkeeping write did not land, and one that failed keeps its own error.
    for root in roots {
        let root = match root {
            Ok(root) => root,
            Err(changed) => {
                slots.push(Err(changed));
                continue;
            }
        };
        match walk(&working, &root) {
            Ok(report) => {
                let _ = scan::mark_deriving(&working, &root);
                if report.refused.is_none() {
                    recorded.push(root.clone());
                }
                slots.push(Ok(report));
                walked.push(root);
            }
            Err(why) if reread.answers_each_slot() => {
                slots.push(Err(reread.walk_failed(&root, &why)));
            }
            Err(why) => {
                for earlier in &walked {
                    let _ = scan::mark_derive_failed(&working, earlier);
                }
                return Err(reread.walk_failed(&root, &why));
            }
        }
    }
    if walked.is_empty() && reread.answers_each_slot() {
        // No walk succeeded, so nothing is derived, and every slot already
        // holds its own error. A walk that failed after its commit leaves its
        // rows to the next derive.
        return Ok(slots
            .into_iter()
            .filter_map(Result::err)
            .map(|why| serde_json::json!({ "error": why }))
            .collect());
    }
    let lease = lease_for_the_derive(library, &working, &recorded, waiting);
    let outcome = match &lease {
        Ok(lease) => lease.holding(|| derive(&working)),
        Err(why) => Err(why.clone()),
    };
    let derived = match outcome {
        Ok(report) => {
            let _ = scan::mark_derived(&working, &walked);
            Ok(report)
        }
        Err(why) => {
            for root in &walked {
                let _ = scan::mark_derive_failed(&working, root);
            }
            let said = reread.derive_failed(&walked, &why);
            if reread.answers_each_slot() {
                // Answered on each walked slot alone, below.
                Err(said)
            } else {
                // Ends the job: the one sentence answers every slot.
                return Err(said);
            }
        }
    };
    Ok(slots
        .into_iter()
        .map(|slot| match (slot, &derived) {
            (Ok(walk), Ok(derived)) => finished_scan_json(&walk, derived),
            (Ok(_), Err(said)) => serde_json::json!({ "error": said }),
            (Err(why), _) => serde_json::json!({ "error": why }),
        })
        .collect())
}

/// The job lock, waiting for it where another job holds it, and the
/// directories the store marked `waiting` meanwhile, for the caller to write
/// back ([`scan::unmark_queued`]) before its walk.
///
/// **While it waits, every directory of `asked` reads `waiting`**, on its
/// reservation and in the store, which a restart reads as `interrupted`
/// (O186). The store is marked on a connection dropped before the wait: an
/// in-memory library's is the holder's own. The reservation is cleared once
/// the lock is held, waited for or not, since [`reserve`] marks it too.
fn the_job_lock<'a>(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    job: &'a crate::LibraryJob,
    waiting: &dyn Fn(&[scan::Root], bool),
) -> (crate::JobGuard<'a>, Vec<scan::Queued>) {
    let (held, queued) = match job.try_lock() {
        Ok(held) => (held, Vec::new()),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => (poisoned.into_inner(), Vec::new()),
        Err(std::sync::TryLockError::WouldBlock) => {
            waiting(asked, true);
            let queued = library
                .working()
                .map(|conn| scan::mark_queued(&conn, asked))
                .unwrap_or_default();
            let held = job
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (held, queued)
        }
    };
    waiting(asked, false);
    (held, queued)
}

/// The derive lease for the derive after the walks of `recorded`, waiting for
/// it where another derive holds it.
///
/// **While it waits, the directories of `recorded` read `waiting`**, and `deriving`
/// again once it is held, so a page tells a scan held up by another derive
/// from one deriving. The wait has no end: a derive elsewhere always ends, and
/// a scan's own status route says how long it has run.
///
/// Taken after the job lock and with no store transaction open, so nothing
/// that waits for either is waited for here. An error fails the derive (O51).
fn lease_for_the_derive<'a>(
    library: &'a taiyaku_library::Library,
    conn: &rusqlite::Connection,
    recorded: &[scan::Root],
    waiting: &dyn Fn(&[scan::Root], bool),
) -> Result<lease::DeriveLease<'a>, String> {
    if let Some(lease) = library.derive_lease(lease::Wait::No)? {
        return Ok(lease);
    }
    waiting(recorded, true);
    let _ = scan::mark_waiting(conn, recorded);
    // Not `eprintln!`, which panics when nothing reads stderr.
    let _ = writeln!(
        std::io::stderr(),
        "taiyaku: a scan is waiting for another derive of this library to finish"
    );
    loop {
        if let Some(lease) =
            library.derive_lease(lease::Wait::Yes(&mut |_: std::time::Duration| {}))?
        {
            let _ = scan::mark_waited(conn, recorded);
            waiting(recorded, false);
            return Ok(lease);
        }
    }
}

/// The directories whose scan, reserved and not yet done, waits for the job
/// lock or the derive lease: what [`status_sent`] reads `waiting` from.
pub(crate) fn waiting_for_a_lock(state: &ServeState) -> std::collections::HashSet<i64> {
    let scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    scans
        .iter()
        .filter(|(_, progress)| !progress.done && progress.waiting)
        .map(|(id, _)| *id)
        .collect()
}

/// The last scan status a page is sent for directory `id`, given the store's,
/// `stored`: `waiting` where `id` is one of `waiting`
/// ([`waiting_for_a_lock`]), else the store's word.
///
/// **The store's word lags while a holder in this process derives** (O51): a
/// preview, a save or a removal holds the store's write lock for its whole
/// derive, so the job's `scan::mark_waiting` or `mark_queued` waits behind
/// it. The status route and both lists read it here, so a directory's page
/// and both lists say the same; an add's answer sends the store's word.
pub(crate) fn status_sent(
    waiting: &std::collections::HashSet<i64>,
    id: i64,
    stored: Option<String>,
) -> Option<String> {
    if waiting.contains(&id) {
        Some("waiting".to_string())
    } else {
        stored
    }
}

/// What a caught panic said, or that it said nothing readable.
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "the scan panicked with a payload that names no message".to_string())
}

/// What each of `count` directories' registry entry then holds: `outcome`'s
/// reports, or, where it failed or panicked, the same `{"error": …}` for every
/// directory, so none is left running for good.
pub(crate) fn reports_of(
    count: usize,
    outcome: std::thread::Result<Result<Vec<serde_json::Value>, String>>,
) -> Vec<serde_json::Value> {
    let why = match outcome {
        Ok(Ok(reports)) => return reports,
        Ok(Err(why)) => why,
        Err(payload) => format!("the scan panicked: {}", panic_message(payload.as_ref())),
    };
    (0..count)
        .map(|_| serde_json::json!({ "error": why }))
        .collect()
}

/// Whether `scans` holds a scan of `root` reserved and not yet done.
fn held(scans: &std::collections::HashMap<i64, crate::ScanProgress>, root: &scan::Root) -> bool {
    scans.get(&root.id).is_some_and(|progress| !progress.done)
}

/// The first of `roots` with a scan reserved and not yet done, if any.
pub(crate) fn a_scan_reserved_on<'a>(
    state: &ServeState,
    roots: &'a [scan::Root],
) -> Option<&'a scan::Root> {
    let scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    roots.iter().find(|root| held(&scans, root))
}

/// `roots` less each one with a scan reserved and not yet done.
///
/// A check only: a press may reserve one of the rest before the caller does,
/// and [`reserve`] then refuses them all.
pub(crate) fn not_held(state: &ServeState, roots: Vec<scan::Root>) -> Vec<scan::Root> {
    let scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    roots
        .into_iter()
        .filter(|root| !held(&scans, root))
        .collect()
}

/// The refusal of a removal that a scan of `root` holds up. `what` is what
/// was to be removed.
pub(crate) fn removal_waits_for(root: &scan::Root, what: &str) -> String {
    waits_for(root, &format!("remove {what}"))
}

/// The refusal of anything a scan of `root` holds up: `to_do` is what was
/// asked, in the words it is said in ("move the directory").
pub(crate) fn waits_for(root: &scan::Root, to_do: &str) -> String {
    format!(
        "a scan of {} is running; {to_do} once it has finished",
        root.path.display()
    )
}

/// Reserve the registry slot of each of `roots` for one scan that started at
/// `started` at the request of `library`, where one asked, or answer the
/// sentence that refuses it, naming a directory already being scanned.
///
/// The check and the reservations are one lock: a refusal reserves nothing,
/// and two requests racing for the same directories cannot both find them free.
pub(crate) fn reserve(
    state: &ServeState,
    roots: &[scan::Root],
    started: Instant,
    library: Option<i64>,
) -> Result<(), String> {
    let mut scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(busy) = roots.iter().find(|root| held(&scans, root)) {
        return Err(format!(
            "a scan of {} is already running",
            busy.path.display()
        ));
    }
    // A scan accepted while the job lock is held waits from now, and reads so
    // until its job holds the lock (`the_job_lock`).
    let waiting = state.library_job.is_held();
    for root in roots {
        scans.insert(
            root.id,
            crate::ScanProgress {
                waiting,
                started,
                done: false,
                report: None,
                library,
            },
        );
    }
    Ok(())
}

/// Mark the scan reserved on `id` at `started` done, with `report`.
///
/// **Only that reservation is replaced**, which its start instant names: a
/// removal clears the entry, and a scan of a directory that took the freed id
/// holds an entry of its own. A report with no reservation of its own to land
/// on is dropped, since nothing is waiting for it.
pub(crate) fn finish_reservation(
    scans: &mut std::collections::HashMap<i64, crate::ScanProgress>,
    id: i64,
    started: Instant,
    report: serde_json::Value,
) {
    if let Some(entry) = scans.get_mut(&id)
        && entry.started == started
    {
        *entry = crate::ScanProgress {
            waiting: false,
            started,
            done: true,
            report: Some(report),
            library: entry.library,
        };
    }
}

/// Scan the directories `reserve` reserved at `started`, as `reread` says, on the
/// blocking pool.
///
/// Nothing awaits the job, so [`spawn_job`] catches a panic in it: left to
/// unwind it would leave every entry at `done: false` for good, and every
/// later scan of those directories refused as already running for the rest of
/// the process.
pub(crate) fn spawn(
    state: Arc<ServeState>,
    roots: Vec<scan::Root>,
    started: Instant,
    reread: Reread,
) -> tokio::task::JoinHandle<()> {
    spawn_job(state, roots, started, move |state, roots, waiting| {
        run(&state.library, roots, reread, &state.library_job, waiting)
    })
}

/// [`spawn`] with the job given: run `job` on the blocking pool, and when it
/// ends, however it ends, mark every one of `roots` done with its report, or
/// with an error where the job answered none for it.
///
/// A reservation is identified by its `started`: an entry that another scan
/// started, or that a removal cleared, is left as it is.
pub(crate) fn spawn_job<F>(
    state: Arc<ServeState>,
    roots: Vec<scan::Root>,
    started: Instant,
    job: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnOnce(
            &ServeState,
            &[scan::Root],
            &dyn Fn(&[scan::Root], bool),
        ) -> Result<Vec<serde_json::Value>, String>
        + Send
        + 'static,
{
    tokio::task::spawn_blocking(move || {
        // Marks this job's own reservations, as `finish_reservation` does, of
        // the directories the job names: each one asked for while it waits
        // for the job lock, and for the derive lease only those whose walk
        // recorded something, since one it could not walk, or whose walk was
        // refused, has nothing to derive.
        let waiting = |recorded: &[scan::Root], waiting: bool| {
            let mut scans = state
                .scans
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for root in recorded {
                if let Some(entry) = scans.get_mut(&root.id)
                    && entry.started == started
                {
                    entry.waiting = waiting;
                }
            }
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job(&state, &roots, &waiting)
        }));
        let mut reports = reports_of(roots.len(), outcome).into_iter();
        let mut scans = state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for root in &roots {
            let report = reports.next().unwrap_or_else(
                || serde_json::json!({ "error": "the scan ended without a report for this directory" }),
            );
            finish_reservation(&mut scans, root.id, started, report);
        }
    })
}

#[cfg(test)]
mod tests;
