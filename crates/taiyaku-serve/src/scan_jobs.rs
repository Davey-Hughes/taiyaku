//! The scans the directory routes and the library routes start in the
//! background: one reservation in [`crate::ServeState::scans`], one job on the
//! blocking pool, and one report landed on each directory's own reservation.
//!
//! **A scan of one directory is the scan of a library of one.** The directory route
//! reserves `std::slice::from_ref(&root)` and a library route reserves every
//! directory the library reaches ([`taiyaku_library::reach::walked_by`]);
//! [`Reread`] is the only thing that tells them apart, in what the
//! job reads again under [`crate::ServeState::library_job`] and in the words of
//! what it answers where that fails.

use std::sync::Arc;
use std::time::Instant;

use taiyaku_library::{derive, libraries, scan};

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
}

impl Reread {
    /// `asked` as the store holds it now, or the sentence that answers a scan
    /// whose directories are no longer what was asked for.
    ///
    /// Where the two readings agree the second is what is walked, with the
    /// settings the directories hold now.
    fn read_again(
        self,
        conn: &rusqlite::Connection,
        asked: &[scan::Root],
    ) -> Result<Vec<scan::Root>, String> {
        match self {
            Self::Dir => {
                let held = scan::roots(conn)?;
                asked
                    .iter()
                    .map(|asked| {
                        held.iter()
                            .find(|now| now.id == asked.id && now.path == asked.path)
                            .cloned()
                            .ok_or_else(|| DIR_CHANGED.to_string())
                    })
                    .collect()
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
                    Ok(now)
                } else {
                    Err(CHANGED.to_string())
                }
            }
        }
    }

    /// What every directory's report says where the walk of `root` failed.
    fn walk_failed(self, root: &scan::Root, why: &str) -> String {
        match self {
            Self::Dir => why.to_string(),
            Self::Library(_) => format!("scanning {}: {why}", root.path.display()),
        }
    }

    /// What every directory's report says where the derive after the walks of
    /// `roots` failed.
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
/// interleaving this closes. Unlike a rules save's `try_lock`, this blocks: a
/// scan already runs unwatched in the background, so there is nobody to answer
/// a refusal to, and the scan's status route is where a caller learns how long
/// the wait and the scan took together.
///
/// **The directories are read again once the lock is held**, as `reread` says:
/// `asked` is what the request found, and a directory removed while the scan
/// waited may have had its id taken by another. Where they differ nothing is
/// walked.
///
/// **A derive failure also corrects `last_scan_status`.** A walk writes `ok`
/// the moment it commits, before the derive starts, so each directory walked has
/// [`scan::mark_derive_failed`] applied where what it found was not derived. A
/// walk that fails ends the scan: no later directory is walked and nothing is
/// derived.
///
/// # Errors
///
/// Returns the sentence every directory's report then carries: `reread`'s where
/// the directories changed, the store's own words where a reading or the working
/// connection failed, and `reread`'s wording of a failed walk or derive.
pub(crate) fn run(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    reread: Reread,
    job: &crate::LibraryJob,
) -> Result<Vec<serde_json::Value>, String> {
    let _job = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let working = library.working()?;
    let roots = reread.read_again(&working, asked)?;
    let mut walked = Vec::with_capacity(roots.len());
    for root in &roots {
        match scan::scan_root(&working, root) {
            Ok(report) => walked.push(report),
            Err(why) => {
                for earlier in &roots[..walked.len()] {
                    let _ = scan::mark_derive_failed(&working, earlier);
                }
                return Err(reread.walk_failed(root, &why));
            }
        }
    }
    let derived = match derive::derive(&working) {
        Ok(report) => report,
        Err(why) => {
            for root in &roots {
                let _ = scan::mark_derive_failed(&working, root);
            }
            return Err(reread.derive_failed(&roots, &why));
        }
    };
    Ok(walked
        .iter()
        .map(|walk| finished_scan_json(walk, &derived))
        .collect())
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

/// The first of `roots` with a scan reserved and not yet done, if any.
pub(crate) fn a_scan_reserved_on<'a>(
    state: &ServeState,
    roots: &'a [scan::Root],
) -> Option<&'a scan::Root> {
    let scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    roots
        .iter()
        .find(|root| scans.get(&root.id).is_some_and(|progress| !progress.done))
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
    if let Some(busy) = roots
        .iter()
        .find(|root| scans.get(&root.id).is_some_and(|progress| !progress.done))
    {
        return Err(format!(
            "a scan of {} is already running",
            busy.path.display()
        ));
    }
    for root in roots {
        scans.insert(
            root.id,
            crate::ScanProgress {
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
    spawn_job(state, roots, started, move |state, roots| {
        run(&state.library, roots, reread, &state.library_job)
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
    F: FnOnce(&ServeState, &[scan::Root]) -> Result<Vec<serde_json::Value>, String>
        + Send
        + 'static,
{
    tokio::task::spawn_blocking(move || {
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&state, &roots)));
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
