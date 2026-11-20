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

use std::cell::Cell;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use taiyaku_library::progress::{Outcome, Step, Watch};
use taiyaku_library::{derive, lease, libraries, reach, scan};

use crate::ServeState;
use crate::config_api::finished_scan_json;

/// How often a job queued behind another tries the job lock again, reading
/// its cancel flag between tries.
const JOB_LOCK_POLL: Duration = Duration::from_millis(100);

/// The next job's id. Ids start at 1 and are never reused in a process.
static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

/// What a job does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobKind {
    /// Scans the directories [`reserve`] reserved.
    Scan,
    /// The fingerprint pass ([`crate::fingerprint_pass`]), which reserves no
    /// directory.
    Fingerprint,
    /// The match pass ([`crate::match_pass`]), which reserves no directory.
    Match,
}

impl JobKind {
    /// The word the feed names the kind by.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Fingerprint => "fingerprint",
            Self::Match => "match",
        }
    }
}

/// One job: a scan, what [`reserve`] reserved, or a pass, fingerprint or match;
/// and where it is as it runs. Every directory's slot in
/// [`crate::ServeState::scans`] holds the same scan job.
#[derive(Debug)]
pub(crate) struct Job {
    /// Unique in this process, for a page to name the job it cancels.
    pub(crate) id: u64,
    /// A scan, the fingerprint pass or the match pass.
    pub(crate) kind: JobKind,
    /// The instant that names the job's reservations ([`finish_reservation`]).
    pub(crate) started: Instant,
    /// Each directory reserved, id and path, in the order reserved.
    pub(crate) dirs: Vec<(i64, PathBuf)>,
    /// The libraries whose pages show the job, id and name: empty until
    /// [`spawn_job`] reads them ([`held_libraries`]).
    pub(crate) libraries: Mutex<Vec<(i64, String)>>,
    cancel: AtomicBool,
    snapshot: Mutex<Snapshot>,
    ended: Mutex<Option<JobEnded>>,
    /// The server's count of ended jobs ([`crate::ServeState::jobs_ended`]),
    /// which [`Job::end`] raises.
    jobs_ended: Arc<AtomicU64>,
}

/// Where a job is: its step, that step's count, and the directory it walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) step: Step,
    /// The count so far; `None` while the job waits.
    pub(crate) done: Option<u64>,
    /// What the count runs to, where that is known before the step ends.
    pub(crate) of: Option<u64>,
    /// The directory walked, from 1, of the job's: set while listing and
    /// resolving, since hashing and writing cover every directory at once.
    pub(crate) dir: Option<(usize, usize)>,
}

/// How a job ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobEnded {
    /// It ran to its end, whatever each directory's report says.
    Finished,
    /// A cancel stopped it in this step.
    Cancelled(Step),
}

impl Job {
    /// A job of `dirs` reserved at `started`, waiting, with the next id,
    /// counted in `jobs_ended` once it ends.
    pub(crate) fn new(
        started: Instant,
        dirs: Vec<(i64, PathBuf)>,
        jobs_ended: Arc<AtomicU64>,
    ) -> Self {
        Self {
            id: NEXT_JOB.fetch_add(1, Ordering::SeqCst),
            kind: JobKind::Scan,
            started,
            dirs,
            libraries: Mutex::default(),
            cancel: AtomicBool::new(false),
            snapshot: Mutex::new(Snapshot {
                step: Step::Waiting,
                done: None,
                of: None,
                dir: None,
            }),
            ended: Mutex::new(None),
            jobs_ended,
        }
    }

    /// A fingerprint pass's job started at `started`, with the next id: no
    /// directory, counting files from none. Its ended count is its own, so the
    /// feed's `ended` counts scans alone.
    pub(crate) fn fingerprint(started: Instant) -> Self {
        Self {
            kind: JobKind::Fingerprint,
            snapshot: Mutex::new(Snapshot {
                step: Step::Fingerprinting,
                done: Some(0),
                of: None,
                dir: None,
            }),
            ..Self::new(started, Vec::new(), Arc::default())
        }
    }

    /// A match pass's job started at `started`, with the next id: no directory,
    /// counting files from none. Its ended count is its own, as a fingerprint
    /// pass's is.
    pub(crate) fn matching(started: Instant) -> Self {
        Self {
            kind: JobKind::Match,
            snapshot: Mutex::new(Snapshot {
                step: Step::Matching,
                done: Some(0),
                of: None,
                dir: None,
            }),
            ..Self::new(started, Vec::new(), Arc::default())
        }
    }

    /// A pass's count in its one `step`: `done` units finished with, whatever
    /// came of each, of the `of` queued so far.
    pub(crate) fn set_count(&self, step: Step, done: u64, of: u64) {
        *locked(&self.snapshot) = Snapshot {
            step,
            done: Some(done),
            of: Some(of),
            dir: None,
        };
    }

    /// Ask the job to stop at its next check. Asking twice is asking once.
    pub(crate) fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Whether the job has been asked to stop.
    pub(crate) fn cancelling(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        *locked(&self.snapshot)
    }

    /// How the job ended, once every one of its slots holds its report.
    pub(crate) fn ended(&self) -> Option<JobEnded> {
        *locked(&self.ended)
    }

    /// Mark the job ended, then count it: whoever reads the count raised
    /// reads the job ended.
    pub(crate) fn end(&self, ended: JobEnded) {
        *locked(&self.ended) = Some(ended);
        self.jobs_ended.fetch_add(1, Ordering::SeqCst);
    }
}

/// `mutex`'s value, whether or not a holder panicked: every value guarded
/// here is whole between statements.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a job reports through as it runs: each step and count on its [`Job`],
/// cheaply, since the walk and the derive report per file, entry and digest;
/// and on each of its slots in a registry, whether it waits.
#[derive(Debug)]
pub(crate) struct Running<'a> {
    job: &'a Job,
    /// The registry whose slots reserved at the job's instant say it waits;
    /// `None` for a job no slot was reserved for.
    scans: Option<&'a Mutex<HashMap<i64, crate::ScanProgress>>>,
    /// The step a cancel stopped the job in, once one has.
    stopped: Cell<Option<Step>>,
}

impl<'a> Running<'a> {
    pub(crate) fn new(
        job: &'a Job,
        scans: Option<&'a Mutex<HashMap<i64, crate::ScanProgress>>>,
    ) -> Self {
        Self {
            job,
            scans,
            stopped: Cell::new(None),
        }
    }

    /// Mark each of `roots`' slot as waiting (`true`) or not: for the job
    /// lock, every directory asked; for the derive lease, those whose walks
    /// recorded something. While it waits, the job is in the step Waiting.
    pub(crate) fn waiting(&self, roots: &[scan::Root], waiting: bool) {
        if let Some(scans) = self.scans {
            let mut scans = locked(scans);
            for root in roots {
                if let Some(entry) = scans.get_mut(&root.id)
                    && entry.started == self.job.started
                {
                    entry.waiting = waiting;
                }
            }
        }
        if waiting {
            *locked(&self.job.snapshot) = Snapshot {
                step: Step::Waiting,
                done: None,
                of: None,
                dir: None,
            };
        }
    }

    /// The job starts to walk directory `at`, from 1, of `of`: listing it,
    /// with nothing found yet, so no count of the last directory's shows
    /// against this one.
    pub(crate) fn at_dir(&self, at: usize, of: usize) {
        *locked(&self.job.snapshot) = Snapshot {
            step: Step::Listing,
            done: Some(0),
            of: None,
            dir: Some((at, of)),
        };
    }
}

impl Watch for Running<'_> {
    /// The latest report, whatever came before: a walk that walks again
    /// counts from 0 again.
    fn at(&self, step: Step, done: u64, of: Option<u64>) {
        let mut snapshot = locked(&self.job.snapshot);
        let dir = match step {
            Step::Listing | Step::Resolving => snapshot.dir,
            Step::Waiting
            | Step::Hashing
            | Step::Writing
            | Step::Finishing
            | Step::Fingerprinting
            | Step::Matching => None,
        };
        *snapshot = Snapshot {
            step,
            done: Some(done),
            of,
            dir,
        };
    }

    fn cancelled(&self) -> bool {
        self.job.cancelling()
    }
}

/// What a directory's report says of a scan that found its directory gone, or
/// another directory holding its id, once the scan could start.
pub(crate) const DIR_CHANGED: &str =
    "the directory was removed or changed while the scan waited to start, so nothing was scanned";

/// What each directory's report says of a scan that found the directories its
/// library shows changed: one added, removed or moved, or a pattern edit that
/// changed what the library reaches.
pub(crate) const CHANGED: &str = "the directories the library shows changed while the scan \
                                  waited to start, so nothing was scanned";

/// What a directory's report says of a scheduled scan that found it paused
/// once the scan could start.
pub(crate) const PAUSED: &str =
    "the directory was paused while the scheduled scan waited to start, so it was not scanned";

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
    /// where nothing was. One paused since the tick read the schedule is
    /// answered so too, unwalked ([`PAUSED`]).
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

    /// Whether someone pressed Scan for this job, so that it clears the
    /// pause of what it covers. The scheduler never runs a paused directory,
    /// so its batch leaves every pause as it is (spec §7, "Pause").
    fn pressed(self) -> bool {
        !matches!(self, Self::Batch)
    }

    /// `asked` as the store holds it now, one entry per directory in order:
    /// the directory to walk, or the sentence its slot answers where it
    /// changed or, in a batch, is paused. The `Err` is the store's own words
    /// where a read failed, or, outside a batch, the sentence that ends a scan
    /// whose directories changed.
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
                // A tick reads the schedule before it reserves its batch, so a
                // job cancelled in between may have paused one of the batch's
                // directories, which the scheduler never runs. That job writes
                // its pause before its slots are marked done, and the batch can
                // reserve them only after, so this read, later than the
                // reservation, sees the pause.
                let paused = if self.pressed() {
                    std::collections::HashSet::new()
                } else {
                    scan::paused(conn)?
                };
                let now: Vec<Result<scan::Root, String>> = asked
                    .iter()
                    .map(|asked| {
                        let now = held
                            .iter()
                            .find(|now| now.id == asked.id && now.path == asked.path)
                            .cloned()
                            .ok_or_else(|| DIR_CHANGED.to_string())?;
                        if paused.contains(&now.id) {
                            Err(PAUSED.to_string())
                        } else {
                            Ok(now)
                        }
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
/// **Holds `job_lock`, [`crate::ServeState::library_job`], for its whole
/// length, walks and derive both.** See that field's own doc comment for the
/// interleaving this closes. Unlike a rules save's `try_lock`, this waits,
/// saying so through `running` meanwhile ([`the_job_lock`]): a scan runs
/// unwatched in the background, so there is nobody to answer a refusal to,
/// and its status route says how long the wait and the scan took together.
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
    job_lock: &crate::LibraryJob,
    running: &Running<'_>,
) -> Result<Vec<serde_json::Value>, String> {
    run_watched(
        library,
        asked,
        reread,
        job_lock,
        running,
        scan::scan_root_watched,
        |conn, fresh, watch| {
            derive::derive_watched(conn, derive::Documents::Reconcile, fresh, watch)
        },
    )
}

/// [`run_watched`] over a walk and derive that neither report nor stop, as a
/// job no slot was reserved for, for the tests that do not watch.
#[cfg(test)]
pub(crate) fn run_with(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    reread: Reread,
    job_lock: &crate::LibraryJob,
    mut walk: impl FnMut(&rusqlite::Connection, &scan::Root) -> Result<scan::ScanReport, String>,
    derive: impl FnOnce(&rusqlite::Connection) -> Result<derive::DeriveReport, String>,
) -> Result<Vec<serde_json::Value>, String> {
    let job = Job::new(Instant::now(), dirs_of(asked), Arc::default());
    run_watched(
        library,
        asked,
        reread,
        job_lock,
        &Running::new(&job, None),
        |conn, root, _| walk(conn, root).map(Outcome::Done),
        |conn, _, _| derive(conn).map(Outcome::Done),
    )
}

/// [`run`]'s whole body, over a caller-supplied walk and derive, each handed
/// `running` to report through and to read the cancel flag from, and the
/// derive the ids of the directories it reads fresh.
///
/// **Split out as `schema::migrate_with` is**: what is worth pinning is the
/// status each directory reads *during* the job, which only a walk or derive
/// of the test's own can read, and a derive that fails, which the real one
/// cannot be made to do without breaking the store.
pub(crate) fn run_watched(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    reread: Reread,
    job_lock: &crate::LibraryJob,
    running: &Running<'_>,
    mut walk: impl FnMut(
        &rusqlite::Connection,
        &scan::Root,
        &dyn Watch,
    ) -> Result<Outcome<scan::ScanReport>, String>,
    derive: impl FnOnce(
        &rusqlite::Connection,
        &[i64],
        &dyn Watch,
    ) -> Result<Outcome<derive::DeriveReport>, String>,
) -> Result<Vec<serde_json::Value>, String> {
    if reread.pressed() {
        // Accepted, so it clears the pause before it waits: a cancel while it
        // waits pauses again. Best-effort, as `mark_queued` is.
        if let Ok(conn) = library.working() {
            let _ = scan::unpause(&conn, asked);
        }
    }
    let (held, queued) = the_job_lock(library, asked, job_lock, running);
    let Some(_held) = held else {
        // Nothing was walked, so each directory's last scan is as read here.
        if let Ok(conn) = library.working() {
            let _ = scan::unmark_queued(&conn, &queued);
            let unfinished = scan::last_scans(&conn, asked).unwrap_or_default();
            return Ok(cancel(
                &conn,
                asked,
                &unfinished,
                Vec::new(),
                running,
                Step::Waiting,
            ));
        }
        running.stopped.set(Some(Step::Waiting));
        return Ok(cancelled_reports(asked.len(), Vec::new(), Step::Waiting));
    };
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
    if reread.pressed() {
        // Again, once held: a holder's write that outlasts the busy timeout,
        // as a rules save's derive can, refuses the clearing before the wait.
        let _ = scan::unpause(&working, asked);
    }
    let roots = reread.read_again(&working, asked)?;
    // Read before any walk writes its own: a cancel writes each back.
    let kept: Vec<scan::Root> = roots.iter().filter_map(|root| root.clone().ok()).collect();
    let mut unfinished = scan::last_scans(&working, &kept)?;
    let of = roots.len();
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
    for (at, root) in roots.into_iter().enumerate() {
        let root = match root {
            Ok(root) => root,
            Err(changed) => {
                slots.push(Err(changed));
                continue;
            }
        };
        // A walk with nothing new to read never reads the flag itself.
        if running.cancelled() {
            let step = running.job.snapshot().step;
            return Ok(cancel(&working, asked, &unfinished, slots, running, step));
        }
        running.at_dir(at + 1, of);
        match walk(&working, &root, running) {
            Ok(Outcome::Done(report)) => {
                let _ = scan::mark_deriving(&working, &root);
                if report.refused.is_none() {
                    recorded.push(root.clone());
                } else {
                    // A refusal is the walk's last word, kept as it is.
                    unfinished.retain(|unfinished| unfinished.root.id != root.id);
                }
                slots.push(Ok(report));
                walked.push(root);
            }
            Ok(Outcome::Cancelled(step)) => {
                return Ok(cancel(&working, asked, &unfinished, slots, running, step));
            }
            Err(why) if reread.answers_each_slot() => {
                // Its `failed` is its last word, kept as it is.
                unfinished.retain(|unfinished| unfinished.root.id != root.id);
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
    let lease = match lease_for_the_derive(library, &working, &recorded, running) {
        Ok(Some(lease)) => Ok(lease),
        Ok(None) => {
            let step = running.job.snapshot().step;
            return Ok(cancel(&working, asked, &unfinished, slots, running, step));
        }
        Err(why) => Err(why),
    };
    // A pressed job reads what it covers fresh, paused or not: its unpause is
    // best-effort, and a directory left paused would otherwise be frozen by
    // the very Scan pressed for it, nothing hashed or written (O320). A batch
    // runs no paused directory, so it has none to read fresh.
    let fresh: Vec<i64> = if reread.pressed() {
        asked.iter().map(|root| root.id).collect()
    } else {
        Vec::new()
    };
    let outcome = match &lease {
        Ok(lease) => lease.holding(|| derive(&working, &fresh, running)),
        Err(why) => Err(why.clone()),
    };
    let derived = match outcome {
        Ok(Outcome::Done(report)) => {
            let _ = scan::mark_derived(&working, &walked);
            Ok(report)
        }
        Ok(Outcome::Cancelled(step)) => {
            return Ok(cancel(&working, asked, &unfinished, slots, running, step));
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

/// End a job cancelled in `step` (spec §5, "What a cancel keeps and loses"):
/// mark each of `unfinished` `cancelled` with the last scan it had, so a
/// directory never scanned still reads Never, and pause every one of `asked`
/// until a pressed scan covers it. What was written stays; the derive's tail
/// is left to the next derive.
///
/// Best-effort, as every status write of the job is.
fn cancel(
    conn: &rusqlite::Connection,
    asked: &[scan::Root],
    unfinished: &[scan::Unfinished],
    slots: Vec<Result<scan::ScanReport, String>>,
    running: &Running<'_>,
    step: Step,
) -> Vec<serde_json::Value> {
    let _ = scan::mark_cancelled(conn, unfinished);
    let _ = scan::pause(conn, asked);
    running.stopped.set(Some(step));
    cancelled_reports(asked.len(), slots, step)
}

/// What each of `count` directories answers of a job cancelled in `step`,
/// given the `slots` answered so far, in order: a slot already answered keeps
/// its error or its walk's refusal, the job's last word on it, and every
/// other says where the job stopped. The refusal is sent as an error, a
/// shape a page already shows, since no derive ran to complete a finished one.
fn cancelled_reports(
    count: usize,
    slots: Vec<Result<scan::ScanReport, String>>,
    step: Step,
) -> Vec<serde_json::Value> {
    let mut slots = slots.into_iter();
    (0..count)
        .map(|_| match slots.next() {
            Some(
                Err(why)
                | Ok(scan::ScanReport {
                    refused: Some(why), ..
                }),
            ) => serde_json::json!({ "error": why }),
            Some(Ok(_)) | None => serde_json::json!({ "cancelled": { "step": step.as_str() } }),
        })
        .collect()
}

/// The job lock, waiting for it where another job holds it, or `None` where
/// the job was cancelled first; and the directories the store marked `waiting`
/// meanwhile, for the caller to write back ([`scan::unmark_queued`]).
///
/// **While it waits, every directory of `asked` reads `waiting`**, on its
/// reservation and in the store, which a restart reads as `interrupted`
/// (O186). The store is marked on a connection dropped before the wait: an
/// in-memory library's is the holder's own. The reservation is cleared once
/// the lock is held, waited for or not, since [`reserve`] marks it too.
fn the_job_lock<'a>(
    library: &taiyaku_library::Library,
    asked: &[scan::Root],
    job_lock: &'a crate::LibraryJob,
    running: &Running<'_>,
) -> (Option<crate::JobGuard<'a>>, Vec<scan::Queued>) {
    let mut queued = None;
    let held = loop {
        match job_lock.try_lock() {
            Ok(held) => break Some(held),
            Err(TryLockError::Poisoned(poisoned)) => break Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => {}
        }
        if queued.is_none() {
            running.waiting(asked, true);
            queued = Some(
                library
                    .working()
                    .map(|conn| scan::mark_queued(&conn, asked))
                    .unwrap_or_default(),
            );
        }
        // Polled, never a blocking `lock`: a job cancelled while another holds
        // the lock ends at once, not once the holder lets go.
        if running.cancelled() {
            break None;
        }
        std::thread::sleep(JOB_LOCK_POLL);
    };
    running.waiting(asked, false);
    (held, queued.unwrap_or_default())
}

/// The derive lease for the derive after the walks of `recorded`, waiting for
/// it where another derive holds it; `None` where the job was cancelled first.
///
/// **While it waits, the directories of `recorded` read `waiting`**, and `deriving`
/// again once it is held, so a page tells a scan held up by another derive
/// from one deriving. The wait has no end but a cancel: a derive elsewhere
/// always ends, and a scan's own status route says how long it has run.
///
/// Taken after the job lock and with no store transaction open, so nothing
/// that waits for either is waited for here. An error fails the derive (O51).
fn lease_for_the_derive<'a>(
    library: &'a taiyaku_library::Library,
    conn: &rusqlite::Connection,
    recorded: &[scan::Root],
    running: &Running<'_>,
) -> Result<Option<lease::DeriveLease<'a>>, String> {
    if let Some(lease) = library.derive_lease(lease::Wait::No)? {
        return Ok(Some(lease));
    }
    running.waiting(recorded, true);
    let _ = scan::mark_waiting(conn, recorded);
    // Not `eprintln!`, which panics when nothing reads stderr.
    let _ = writeln!(
        std::io::stderr(),
        "taiyaku: a scan is waiting for another derive of this library to finish"
    );
    // Each try waits the lease's own short budget, so the flag is read
    // between tries.
    loop {
        if running.cancelled() {
            return Ok(None);
        }
        if let Some(lease) = library.derive_lease(lease::Wait::No)? {
            let _ = scan::mark_waited(conn, recorded);
            running.waiting(recorded, false);
            return Ok(Some(lease));
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
/// `started` at the request of `library`, where one asked, and answer the job
/// every slot holds, waiting; or answer the sentence that refuses it, naming
/// a directory already being scanned.
///
/// The check and the reservations are one lock: a refusal reserves nothing,
/// and two requests racing for the same directories cannot both find them free.
pub(crate) fn reserve(
    state: &ServeState,
    roots: &[scan::Root],
    started: Instant,
    library: Option<i64>,
) -> Result<Arc<Job>, String> {
    let mut scans = locked(&state.scans);
    if let Some(busy) = roots.iter().find(|root| held(&scans, root)) {
        return Err(format!(
            "a scan of {} is already running",
            busy.path.display()
        ));
    }
    // A scan accepted while the job lock is held waits from now, and reads so
    // until its job holds the lock (`the_job_lock`).
    let waiting = state.library_job.is_held();
    let job = Arc::new(Job::new(
        started,
        dirs_of(roots),
        Arc::clone(&state.jobs_ended),
    ));
    for root in roots {
        scans.insert(
            root.id,
            crate::ScanProgress {
                waiting,
                started,
                done: false,
                report: None,
                library,
                job: Arc::clone(&job),
            },
        );
    }
    Ok(job)
}

/// Each of `roots`' id and path, as a [`Job`] holds them.
fn dirs_of(roots: &[scan::Root]) -> Vec<(i64, PathBuf)> {
    roots
        .iter()
        .map(|root| (root.id, root.path.clone()))
        .collect()
}

/// Mark the scan reserved on `id` at `started` done, with `report`.
///
/// **Only that reservation is replaced**, which its start instant names: a
/// removal clears the entry, and a scan of a directory that took the freed id
/// holds an entry of its own. A report with no reservation of its own to land
/// on is dropped, since nothing is waiting for it.
pub(crate) fn finish_reservation(
    scans: &mut HashMap<i64, crate::ScanProgress>,
    id: i64,
    started: Instant,
    report: serde_json::Value,
) {
    if let Some(entry) = scans.get_mut(&id)
        && entry.started == started
    {
        entry.waiting = false;
        entry.done = true;
        entry.report = Some(report);
    }
}

/// The libraries whose pages show a job holding `roots`, id and name,
/// ascending by id: each library whose directories include one of them
/// ([`reach::libraries_reaching`]), an outer library a nested one reaches
/// among them.
///
/// # Errors
///
/// The store's own words where a read failed.
pub(crate) fn held_libraries(
    conn: &rusqlite::Connection,
    roots: &[scan::Root],
) -> Result<Vec<(i64, String)>, String> {
    let all = scan::roots(conn)?;
    let ids: Vec<i64> = roots.iter().map(|root| root.id).collect();
    let mut held = Vec::new();
    for id in reach::libraries_reaching(&all, &ids) {
        if let Some(name) = libraries::name_of(conn, id)? {
            held.push((id, name));
        }
    }
    Ok(held)
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
    spawn_job(state, roots, started, move |state, roots, running| {
        run(&state.library, roots, reread, &state.library_job, running)
    })
}

/// [`spawn`] with the job given: run `job` on the blocking pool over the
/// [`Job`] reserved at `started`, and when it ends, however it ends, mark
/// every one of `roots` done with its report, or with an error where the job
/// answered none for it; then mark the [`Job`] ended.
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
    F: FnOnce(&ServeState, &[scan::Root], &Running<'_>) -> Result<Vec<serde_json::Value>, String>
        + Send
        + 'static,
{
    tokio::task::spawn_blocking(move || {
        let reserved = reserved_job(&state, &roots, started);
        // Best-effort: a job whose libraries cannot be read still runs, and
        // shows on its directories' pages alone.
        if let Ok(conn) = state.library.working()
            && let Ok(held) = held_libraries(&conn, &roots)
        {
            *locked(&reserved.libraries) = held;
        }
        let running = Running::new(&reserved, Some(&state.scans));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job(&state, &roots, &running)
        }));
        let mut reports = reports_of(roots.len(), outcome).into_iter();
        {
            let mut scans = locked(&state.scans);
            for root in &roots {
                let report = reports.next().unwrap_or_else(
                    || serde_json::json!({ "error": "the scan ended without a report for this directory" }),
                );
                finish_reservation(&mut scans, root.id, started, report);
            }
        }
        let ended = running
            .stopped
            .get()
            .map_or(JobEnded::Finished, JobEnded::Cancelled);
        reserved.end(ended);
        // A cancel is a choice to stop, so only a scan that ran to its end
        // starts a pass over what it found.
        if ended == JobEnded::Finished {
            crate::fingerprint_pass::kick(&state);
        }
    })
}

/// The [`Job`] reserved at `started` on one of `roots`, or, where a removal
/// cleared every slot first, a job of their own that no page shows.
fn reserved_job(state: &ServeState, roots: &[scan::Root], started: Instant) -> Arc<Job> {
    let scans = locked(&state.scans);
    roots
        .iter()
        .find_map(|root| {
            scans
                .get(&root.id)
                .filter(|entry| entry.started == started)
                .map(|entry| Arc::clone(&entry.job))
        })
        .unwrap_or_else(|| {
            Arc::new(Job::new(
                started,
                dirs_of(roots),
                Arc::clone(&state.jobs_ended),
            ))
        })
}

#[cfg(test)]
mod tests;
