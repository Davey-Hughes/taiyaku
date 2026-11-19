//! The fingerprint pass: in the background, the server reads each readable
//! file in a library with matching on, and stores its pages' fingerprints
//! under the file's content digest for content matching to compare.
//!
//! One pass at a time across the store, on threads of its own: a coordinator
//! lists the units due, again at each kick, and a few workers read them. It
//! takes no job lock and reserves no directory, so nothing a scan or a library
//! route does waits on it. A row is stored as each file is read, so a pass
//! stopped partway, by a cancel or a restart, resumes at the next file.

pub(crate) mod priority;
mod read;

use std::collections::{HashSet, VecDeque};
use std::io::Write as _;
#[cfg(test)]
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::Scope;
use std::time::{Duration, Instant};

use taiyaku_library::fingerprints::{self, Outcome, Row, Unit};
use taiyaku_library::progress::Step;

use crate::scan_jobs::{Job, JobEnded, panic_message};
use crate::{ServeState, about, fingerprint};
use read::{LargePages, Phase, Read};

/// How a pass runs.
pub(crate) struct PassSettings {
    /// Worker threads reading files side by side.
    pub(crate) workers: usize,
    /// How long after a reader is served a page no file starts.
    pub(crate) quiet: Duration,
    /// How often a waiting worker looks again.
    pub(crate) poll: Duration,
    /// The page size from which decodes run one at a time across the workers.
    pub(crate) large_page_bytes: usize,
    /// Asked before each walked page is read, with the file and the page's
    /// index: once for each page of each file read.
    #[cfg(test)]
    pub(crate) after_page: Option<PageHook>,
    /// Asked once a listing has found units due and the pass shows its job,
    /// before any is read.
    #[cfg(test)]
    pub(crate) after_listing: Option<Hook>,
    /// Asked with a file whose stamp has been checked, just before it is
    /// hashed.
    #[cfg(test)]
    pub(crate) before_hashing: Option<FileHook>,
    /// Asked with a file the pass has just hashed, before its stamp is taken
    /// again, and while a large file still hashes alone.
    #[cfg(test)]
    pub(crate) after_hashing: Option<FileHook>,
    /// Asked with a file whose read has ended in a row, before the row waits
    /// for the job lock.
    #[cfg(test)]
    pub(crate) after_read: Option<FileHook>,
    /// Asked as each worker stops, once it is counted out of the queue.
    #[cfg(test)]
    pub(crate) after_stopping: Option<Hook>,
    /// Asked with a file a worker has found it is not to read, before that is
    /// recorded.
    #[cfg(test)]
    pub(crate) after_passing_over: Option<FileHook>,
    /// Asked once a listing `relist_at_kicks` makes has found what is due,
    /// before that is queued.
    #[cfg(test)]
    pub(crate) after_relisting: Option<Hook>,
}

/// What [`PassSettings::after_page`] calls.
#[cfg(test)]
pub(crate) type PageHook = Arc<dyn Fn(&Path, usize) + Send + Sync>;

/// What [`PassSettings::after_listing`], [`PassSettings::after_stopping`] and
/// [`PassSettings::after_relisting`] call.
#[cfg(test)]
pub(crate) type Hook = Arc<dyn Fn() + Send + Sync>;

/// What [`PassSettings::before_hashing`], [`PassSettings::after_hashing`],
/// [`PassSettings::after_read`] and [`PassSettings::after_passing_over`] call.
#[cfg(test)]
pub(crate) type FileHook = Arc<dyn Fn(&Path) + Send + Sync>;

impl Default for PassSettings {
    fn default() -> Self {
        Self {
            // As many as content matching's timings were taken with.
            workers: 4,
            quiet: Duration::from_secs(30),
            poll: Duration::from_secs(1),
            large_page_bytes: fingerprint::LARGE_PAGE_BYTES,
            #[cfg(test)]
            after_page: None,
            #[cfg(test)]
            after_listing: None,
            #[cfg(test)]
            before_hashing: None,
            #[cfg(test)]
            after_hashing: None,
            #[cfg(test)]
            after_read: None,
            #[cfg(test)]
            after_stopping: None,
            #[cfg(test)]
            after_passing_over: None,
            #[cfg(test)]
            after_relisting: None,
        }
    }
}

impl PassSettings {
    #[cfg(test)]
    fn walked_page(&self, path: &Path, index: usize) {
        if let Some(hook) = &self.after_page {
            hook(path, index);
        }
    }

    #[cfg(not(test))]
    fn walked_page(&self, _: &std::path::Path, _: usize) {}

    #[cfg(test)]
    fn listed(&self) {
        if let Some(hook) = &self.after_listing {
            hook();
        }
    }

    #[cfg(not(test))]
    fn listed(&self) {}

    #[cfg(test)]
    fn hashing(&self, path: &Path) {
        if let Some(hook) = &self.before_hashing {
            hook(path);
        }
    }

    #[cfg(not(test))]
    fn hashing(&self, _: &std::path::Path) {}

    #[cfg(test)]
    fn hashed(&self, path: &Path) {
        if let Some(hook) = &self.after_hashing {
            hook(path);
        }
    }

    #[cfg(not(test))]
    fn hashed(&self, _: &std::path::Path) {}

    #[cfg(test)]
    fn read_ended(&self, path: &Path) {
        if let Some(hook) = &self.after_read {
            hook(path);
        }
    }

    #[cfg(not(test))]
    fn read_ended(&self, _: &std::path::Path) {}

    #[cfg(test)]
    fn stopped(&self) {
        if let Some(hook) = &self.after_stopping {
            hook();
        }
    }

    #[cfg(not(test))]
    fn stopped(&self) {}

    #[cfg(test)]
    fn passing_over(&self, path: &Path) {
        if let Some(hook) = &self.after_passing_over {
            hook(path);
        }
    }

    #[cfg(not(test))]
    fn passing_over(&self, _: &std::path::Path) {}

    #[cfg(test)]
    fn relisting(&self) {
        if let Some(hook) = &self.after_relisting {
            hook();
        }
    }

    #[cfg(not(test))]
    fn relisting(&self) {}
}

/// The fingerprint pass in [`ServeState`]: whether one runs, and its job.
pub struct Pass {
    /// `None` for a pass that never starts.
    settings: Option<PassSettings>,
    state: Mutex<State>,
    /// Signalled under `state` when a kick sets `again` and when a worker
    /// stops, so a coordinator waiting on its workers lists again at once.
    woken: Condvar,
    /// When a reader was last served, in milliseconds from `epoch`, plus one
    /// so that zero says never: a page request pays one store.
    reader: AtomicU64,
    epoch: Instant,
}

impl Default for Pass {
    fn default() -> Self {
        Self {
            settings: None,
            state: Mutex::default(),
            woken: Condvar::new(),
            reader: AtomicU64::new(0),
            epoch: Instant::now(),
        }
    }
}

#[derive(Default)]
struct State {
    /// A coordinator is starting or running.
    busy: bool,
    /// A kick came while busy, and since the job's cancel where it has one:
    /// the coordinator lists again, at once while its workers read, else
    /// before it ends.
    again: bool,
    /// The running pass's job, or the last one's.
    job: Option<Arc<Job>>,
}

impl std::fmt::Debug for Pass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pass")
            .field("live", &self.settings.is_some())
            .finish_non_exhaustive()
    }
}

impl Pass {
    /// A pass [`kick`] starts, as the server runs it.
    #[must_use]
    pub fn live() -> Self {
        Self::with(PassSettings::default())
    }

    pub(crate) fn with(settings: PassSettings) -> Self {
        Self {
            settings: Some(settings),
            ..Self::default()
        }
    }

    /// A page or a book was served to a reader now.
    pub(crate) fn reader_seen(&self) {
        self.mark_reader(Instant::now());
    }

    fn mark_reader(&self, at: Instant) {
        let millis = self.millis(at).saturating_add(1);
        self.reader.store(millis, Ordering::Relaxed);
    }

    /// How long before `now` a reader was last served, or `None` for never.
    fn since_reader(&self, now: Instant) -> Option<Duration> {
        let mark = self.reader.load(Ordering::Relaxed).checked_sub(1)?;
        Some(Duration::from_millis(self.millis(now).saturating_sub(mark)))
    }

    fn millis(&self, at: Instant) -> u64 {
        let since = at.saturating_duration_since(self.epoch).as_millis();
        u64::try_from(since).unwrap_or(u64::MAX)
    }

    /// The running pass's job, else the last one's.
    pub(crate) fn job(&self) -> Option<Arc<Job>> {
        locked(&self.state).job.clone()
    }

    /// Ask `job`, a pass's, to stop, and forget a kick it has not yet read: a
    /// cancel stops what was asked of the pass before it, so only a kick after
    /// it starts a fresh pass.
    pub(crate) fn cancel(&self, job: &Job) {
        let mut held = locked(&self.state);
        // A job is ended under this lock, so one not ended here is the running
        // pass's, and the kick forgotten is one it would have read.
        if job.ended().is_none() {
            held.again = false;
        }
        job.request_cancel();
    }

    /// Whether no pass runs once `within` has passed, or sooner.
    #[cfg(test)]
    pub(crate) fn settle(&self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            if !locked(&self.state).busy {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Start a pass where none runs. Where one does, wake it to list again at
/// once, so a file a scan has just found is read by it, ahead of what it has
/// still to read, rather than by the next. An inert [`Pass`] starts nothing.
pub(crate) fn kick(state: &Arc<ServeState>) {
    let pass = &state.fingerprints;
    if pass.settings.is_none() {
        return;
    }
    {
        let mut held = locked(&pass.state);
        if held.busy {
            held.again = true;
            pass.woken.notify_all();
            return;
        }
        held.busy = true;
        held.again = false;
    }
    let coordinating = Arc::clone(state);
    let spawned = std::thread::Builder::new()
        .name("fingerprint-pass".to_owned())
        .spawn(move || coordinate(&coordinating));
    if let Err(e) = spawned {
        locked(&pass.state).busy = false;
        say(&format!("could not start: {e}"));
    }
}

/// Start a pass as the server starts. The scheduler's startup scan passes
/// over a directory that is Off, so no scan's end would start one for it,
/// and a restart is no choice to stop.
pub(crate) fn at_startup(state: &Arc<ServeState>) {
    kick(state);
}

/// The coordinator: list what is due, read it, list again at each kick, and
/// end the job once a listing that no kick has outdated is done.
fn coordinate(state: &ServeState) {
    let pass = &state.fingerprints;
    let mut ending = Ending {
        pass,
        job: None,
        armed: true,
    };
    let Some(settings) = &pass.settings else {
        return;
    };
    if let Err(why) = priority::lower() {
        say(&format!("reads at the server's own priority: {why}"));
    }
    let build = about::this_build();
    let large = LargePages::new(settings.large_page_bytes);
    // By the stamp each unit was listed with, so one that stayed due after a
    // read was attempted (it changed with no scan since, or its store refused
    // it) is tried once a pass however often it is listed, while one a scan
    // walked again is read. A unit found not due, or whose digest
    // `record_digest` declined to write, had no read attempted: it leaves
    // through `passed_over`, or a directory covered again mid-pass would wait
    // for the next pass. A store error at either step leaves it tried.
    let mut tried: HashSet<Key> = HashSet::new();
    let passed_over = Mutex::new(Vec::new());
    let counts = Mutex::new(Counts::default());
    loop {
        let units = untried(state, build.as_deref(), &mut tried, &passed_over);
        if !units.is_empty() {
            let job = Arc::clone(ending.job.get_or_insert_with(|| started(state)));
            settings.listed();
            if !job.cancelling() {
                let reading = Reading {
                    state,
                    settings,
                    job: &job,
                    large: &large,
                    build: build.as_deref(),
                    counts: &counts,
                    passed_over: &passed_over,
                    queue: Mutex::default(),
                };
                reading.all(units, &mut tried);
            }
        }
        drop_unclaimed_once_free(state, settings.poll, ending.job.as_deref());
        match ending.what_next() {
            Next::Idle => return,
            Next::Again => {}
            Next::Fresh => {
                tried.clear();
                *locked(&counts) = Counts::default();
            }
        }
    }
}

/// Drop the rows no claimed unit holds once the job lock is free, as a row
/// waits to be stored. A cancel of `job` ends the wait with nothing dropped:
/// the next pass drops what this one leaves.
fn drop_unclaimed_once_free(state: &ServeState, poll: Duration, job: Option<&Job>) {
    while state.library_job.is_held() {
        if job.is_some_and(Job::cancelling) {
            return;
        }
        std::thread::sleep(poll);
    }
    let dropped = state
        .library
        .working()
        .and_then(|conn| fingerprints::drop_unclaimed(&conn));
    if let Err(why) = dropped {
        say(&why);
    }
}

/// What a worker waits for before it starts a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wait {
    /// A scan or another library job holds the library job lock.
    JobLock,
    /// A reader was served within the settings' quiet.
    Reader,
}

/// What `pass` waits for at `now`, the job lock first, or `None`. An inert
/// pass has no quiet, and waits for no reader.
pub(crate) fn waits(pass: &Pass, job_lock_held: bool, now: Instant) -> Option<Wait> {
    if job_lock_held {
        return Some(Wait::JobLock);
    }
    let quiet = pass.settings.as_ref()?.quiet;
    pass.since_reader(now)
        .filter(|since| *since < quiet)
        .map(|_| Wait::Reader)
}

/// Every unit due, or none where the store refuses the listing.
fn listed(state: &ServeState, build: Option<&str>) -> Vec<Unit> {
    let due = state
        .library
        .working()
        .and_then(|conn| fingerprints::due(&conn, fingerprint::VERSION, build));
    due.unwrap_or_else(|why| {
        say(&why);
        Vec::new()
    })
}

/// Every unit due that this pass has not tried, in the order listed. A unit
/// passed over since the last listing is untried again, and this listing alone
/// says whether it is due now.
fn untried(
    state: &ServeState,
    build: Option<&str>,
    tried: &mut HashSet<Key>,
    passed_over: &Mutex<Vec<Key>>,
) -> Vec<Unit> {
    for unit in locked(passed_over).drain(..) {
        tried.remove(&unit);
    }
    listed(state, build)
        .into_iter()
        .filter(|unit| !tried.contains(&key(unit)))
        .collect()
}

/// A unit as listed: its entry, and the size and mtime its walk recorded.
type Key = (i64, String, i64, i64);

fn key(unit: &Unit) -> Key {
    (unit.root_id, unit.rel_path.clone(), unit.size, unit.mtime)
}

/// The pass's job, made once something is due, so a pass with nothing to read
/// never shows: in the pass's state from now, for the feed, and naming the
/// libraries with matching on.
fn started(state: &ServeState) -> Arc<Job> {
    let job = Arc::new(Job::fingerprint(Instant::now()));
    // Best-effort, as a scan's own libraries are.
    if let Ok(conn) = state.library.working()
        && let Ok(coverage) = fingerprints::coverage(&conn)
    {
        *locked(&job.libraries) = coverage.libraries;
    }
    locked(&state.fingerprints.state).job = Some(Arc::clone(&job));
    job
}

/// Puts the pass back to idle however the coordinator leaves, a panic
/// included, and ends its job: a pass left busy would turn every later kick
/// into a wish for another look that no coordinator reads.
struct Ending<'a> {
    pass: &'a Pass,
    job: Option<Arc<Job>>,
    /// Cleared once the coordinator has gone idle itself, after which a kick
    /// may have started another pass whose state this must not touch.
    armed: bool,
}

/// What the coordinator does once a listing is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    /// Nothing: the job has ended and the pass is idle.
    Idle,
    /// List again for the same job: a kick came since the last listing.
    Again,
    /// List again for a job of its own: a kick came as a cancelled job wound
    /// down, and a cancel stops the pass it lands on, not the next one.
    Fresh,
}

impl Ending<'_> {
    /// What comes next, decided under the one lock, so a kick either finds the
    /// pass busy and is read here, or finds it idle and starts another. With
    /// no kick since the last listing the job ends and the pass goes idle;
    /// with one it lists again, after ending a cancelling job.
    fn what_next(&mut self) -> Next {
        let mut held = locked(&self.pass.state);
        // Under the lock [`Pass::cancel`] takes, so a cancel lands wholly
        // before this decision, and the job ends cancelled, or finds it ended.
        let cancelling = self.job.as_ref().is_some_and(|job| job.cancelling());
        if held.again {
            held.again = false;
            if !cancelling {
                return Next::Again;
            }
            if let Some(job) = self.job.take() {
                job.end(JobEnded::Cancelled(Step::Fingerprinting));
            }
            return Next::Fresh;
        }
        if let Some(job) = self.job.take() {
            job.end(if cancelling {
                JobEnded::Cancelled(Step::Fingerprinting)
            } else {
                JobEnded::Finished
            });
        }
        held.busy = false;
        self.armed = false;
        Next::Idle
    }
}

impl Drop for Ending<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut held = locked(&self.pass.state);
        if let Some(job) = self.job.take()
            && job.ended().is_none()
        {
            job.end(if job.cancelling() {
                JobEnded::Cancelled(Step::Fingerprinting)
            } else {
                JobEnded::Finished
            });
        }
        held.again = false;
        held.busy = false;
    }
}

/// The reading of one listing and of every listing `relist_at_kicks` adds to
/// it: what every worker shares with the coordinator.
struct Reading<'a> {
    state: &'a ServeState,
    settings: &'a PassSettings,
    job: &'a Job,
    large: &'a LargePages,
    build: Option<&'a str>,
    /// The job's count, across every listing of its pass.
    counts: &'a Mutex<Counts>,
    /// The units found not due, or whose digest `record_digest` declined to
    /// write, since the last listing, across every listing of the pass.
    passed_over: &'a Mutex<Vec<Key>>,
    queue: Mutex<Queue>,
}

/// A job's count: the units workers have finished with, whatever came of each,
/// of those queued by every listing so far. A unit a worker drops at a cancel
/// is not finished with.
#[derive(Debug, Default)]
struct Counts {
    done: u64,
    of: u64,
}

/// The units listed and not yet taken, and who is left to take them.
#[derive(Debug, Default)]
struct Queue {
    /// The next first.
    units: VecDeque<Unit>,
    /// Workers started and not yet stopped.
    workers: usize,
}

impl Reading<'_> {
    /// Read `units` on the settings' workers, one queue between them, and at
    /// each kick meanwhile list again and queue what is new first; return once
    /// every worker has stopped.
    fn all(&self, units: Vec<Unit>, tried: &mut HashSet<Key>) {
        std::thread::scope(|scope| {
            self.queue_first(scope, units, tried);
            self.relist_at_kicks(scope, tried);
        });
    }

    /// Put `units` at the front of the queue in their order, count them, and
    /// start workers up to the settings' number. Under the queue's lock, so no
    /// worker takes a unit before it is counted, and none stops unseen between
    /// the push and the count of who is left to read it.
    fn queue_first<'scope>(
        &'scope self,
        scope: &'scope Scope<'scope, '_>,
        units: Vec<Unit>,
        tried: &mut HashSet<Key>,
    ) {
        if units.is_empty() {
            return;
        }
        tried.extend(units.iter().map(key));
        let mut queue = locked(&self.queue);
        {
            let mut counts = locked(self.counts);
            counts.of += units.len() as u64;
            self.job.set_count(counts.done, counts.of);
        }
        for unit in units.into_iter().rev() {
            queue.units.push_front(unit);
        }
        while queue.workers < self.settings.workers.max(1) {
            let spawned = std::thread::Builder::new()
                .name(format!("fingerprint-{}", queue.workers))
                .spawn_scoped(scope, || self.work());
            if let Err(e) = spawned {
                say(&format!("could not start a worker: {e}"));
                break;
            }
            queue.workers += 1;
        }
    }

    /// Until every worker has stopped: at each kick, list again and queue what
    /// is new first, so a volume a scan adds mid-pass is read next rather than
    /// after the backlog. A cancelling job lists nothing, and its kick is left
    /// for [`Ending::what_next`]. Nothing else lists here, and at most twice
    /// for a kick, so however a listing and a worker disagree about a unit it
    /// is not queued again without limit, and the pass ends.
    fn relist_at_kicks<'scope>(
        &'scope self,
        scope: &'scope Scope<'scope, '_>,
        tried: &mut HashSet<Key>,
    ) {
        let pass = &self.state.fingerprints;
        let mut held = locked(&pass.state);
        let mut answered_a_kick = false;
        loop {
            if held.again && !self.job.cancelling() {
                held.again = false;
                answered_a_kick = true;
                drop(held);
                self.relist(scope, tried);
                held = locked(&pass.state);
            } else if locked(&self.queue).workers > 0 {
                held = pass
                    .woken
                    .wait(held)
                    .unwrap_or_else(PoisonError::into_inner);
            } else if answered_a_kick
                && !self.job.cancelling()
                && !locked(self.passed_over).is_empty()
            {
                // A worker can pass a unit over just after a kick's listing
                // has run, and no later kick need come to list it.
                answered_a_kick = false;
                drop(held);
                self.relist(scope, tried);
                held = locked(&pass.state);
            } else {
                return;
            }
        }
    }

    /// List again and queue first what this pass has not tried.
    fn relist<'scope>(&'scope self, scope: &'scope Scope<'scope, '_>, tried: &mut HashSet<Key>) {
        let units = untried(self.state, self.build, tried, self.passed_over);
        self.settings.relisting();
        self.queue_first(scope, units, tried);
    }

    /// One worker: take units from the front until the queue is empty or the
    /// job cancels.
    fn work(&self) {
        // Its own, as each thread's priority is: inherited from the
        // coordinator here, but not from whatever thread spawns it next.
        let _ = priority::lower();
        let mut stopping = Stopping {
            reading: self,
            counted_out: false,
        };
        let pass = &self.state.fingerprints;
        loop {
            let unit = {
                let mut queue = locked(&self.queue);
                let Some(unit) = queue.units.pop_front() else {
                    // Under the lock that queues, so a unit queued after this
                    // look finds the worker gone and starts another.
                    queue.workers -= 1;
                    stopping.counted_out = true;
                    return;
                };
                unit
            };
            // Between files, so that neither a library job nor a reader shares
            // the disk with the pass; taken first, so a worker with nothing
            // left waits for nothing. A cancel meanwhile leaves the unit due.
            if !self
                .waited(|| waits(pass, self.state.library_job.is_held(), Instant::now()).is_some())
            {
                return;
            }
            self.one(&unit);
            // Counted under the lock, so no count lands after a later one.
            let mut counts = locked(self.counts);
            counts.done += 1;
            self.job.set_count(counts.done, counts.of);
        }
    }

    /// Read `unit` and store what came of it. Nothing is stored, and it stays
    /// due, where it is no longer due, its digest cannot be taken or written,
    /// it changed while it was read, or a cancel stopped the read. A read that
    /// has ended is stored once the job lock is free, cancel or not: a cancel
    /// stops new work, and the next pass would only read the file again.
    fn one(&self, unit: &Unit) {
        let library = &self.state.library;
        let due = library.working().and_then(|conn| {
            // Read as each file's own check reads it, so a library switched off
            // mid-pass leaves the job's list as its files leave the pass.
            *locked(&self.job.libraries) = fingerprints::coverage(&conn)?.libraries;
            fingerprints::still_due(&conn, unit, fingerprint::VERSION, self.build)
        });
        match due {
            Ok(true) => {}
            Ok(false) => {
                self.pass_over(unit);
                return;
            }
            Err(why) => {
                say(&why);
                return;
            }
        }
        let digest = match &unit.sha256 {
            Some(digest) => digest.clone(),
            None => match self.digest_of(unit) {
                Some(digest) => digest,
                None => return,
            },
        };
        let mut between = |phase, index| {
            if phase == Phase::Walk {
                self.settings.walked_page(&unit.path, index);
            }
            !self.job.cancelling()
        };
        // A decoder bug, or a page crafted to find one, ends this file, never
        // the pass: it is stored unreadable for this build.
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read::read_unit(unit, &digest, self.build, self.large, &mut between)
        }));
        let row = match read {
            Ok(Read::Stored(row)) => row,
            Ok(Read::Gone | Read::Stopped) => return,
            // Every pass reads such a folder and stores nothing, until its own
            // mtime moves or what ended its read is mended (O347); said, or the
            // cost never shows.
            Ok(Read::NotItsDigest(failed)) => {
                let path = unit.path.display();
                say(&match failed {
                    Some(why) => format!(
                        "{path}: it could not be read whole, so nothing is stored, and each \
                         pass reads it again: {why}"
                    ),
                    None => format!(
                        "{path}: its pages do not hash to the digest its scan took, as after a \
                         page is rewritten in place, so nothing is stored, and each pass reads \
                         it again until the folder's own mtime moves"
                    ),
                });
                return;
            }
            // A file that moved as it was read may have panicked on bytes its
            // digest does not name: it stays due, as a changed file does.
            Err(_) if fingerprints::stamp_of(&unit.path) != Some((unit.size, unit.mtime)) => {
                return;
            }
            Err(payload) => Row {
                sha256: digest,
                version: fingerprint::VERSION,
                outcome: Outcome::Unreadable,
                reason: Some(format!(
                    "reading it panicked: {}",
                    panic_message(payload.as_ref())
                )),
                build: self.build.map(str::to_owned),
                pages: 0,
                prints: Vec::new(),
            },
        };
        self.settings.read_ended(&unit.path);
        self.until_the_lock_is_free();
        if let Err(why) = library
            .working()
            .and_then(|conn| fingerprints::store(&conn, &row))
        {
            say(&why);
        }
    }

    /// Wait while `busy` holds, looking again every poll: true once it does
    /// not, false where the job cancels first.
    fn waited(&self, busy: impl Fn() -> bool) -> bool {
        loop {
            if self.job.cancelling() {
                return false;
            }
            if !busy() {
                return true;
            }
            std::thread::sleep(self.settings.poll);
        }
    }

    /// Wait, looking again every poll, until the job lock is free, a cancel or
    /// not. A rule save's derive writes the whole store under the lock, and a
    /// write beside it would wait on the store's own lock or fail.
    fn until_the_lock_is_free(&self) {
        while self.state.library_job.is_held() {
            std::thread::sleep(self.settings.poll);
        }
    }

    /// `unit`'s digest, taken as the derive takes one and written onto its
    /// entry; `None` where it changed before or while it was hashed, it could
    /// not be hashed, the store refused the digest, or it is no longer due: its
    /// directory is not covered now, or the digest has a row this build keeps,
    /// as a copy read earlier in the pass has.
    fn digest_of(&self, unit: &Unit) -> Option<String> {
        let stamp = Some((unit.size, unit.mtime));
        if fingerprints::stamp_of(&unit.path) != stamp {
            return None;
        }
        // Hashing holds a file's whole bytes in memory, so one as large as a
        // large page hashes alone, as a large page decodes alone. A folder is
        // held whole too, every page at once, and its recorded size is its
        // directory's, which bounds nothing: a folder always hashes alone.
        let size = if unit.folder {
            usize::MAX
        } else {
            usize::try_from(unit.size).unwrap_or(usize::MAX)
        };
        let digest = self
            .large
            .run(size, || {
                self.settings.hashing(&unit.path);
                let digest = taiyaku_read::member_digest(&unit.path);
                if digest.is_ok() {
                    self.settings.hashed(&unit.path);
                }
                digest
            })
            .map_err(|why| say(&why))
            .ok()?;
        // Bytes that moved while they were hashed give a digest of neither
        // content, which no entry may be given.
        if fingerprints::stamp_of(&unit.path) != stamp {
            return None;
        }
        self.until_the_lock_is_free();
        let hashed = Unit {
            sha256: Some(digest.clone()),
            ..unit.clone()
        };
        let due = {
            let conn = self.state.library.working().map_err(|why| say(&why)).ok()?;
            fingerprints::record_digest(&conn, unit, &digest)
                .map_err(|why| say(&why))
                .ok()?
                && fingerprints::still_due(&conn, &hashed, fingerprint::VERSION, self.build)
                    .map_err(|why| say(&why))
                    .ok()?
        };
        if !due {
            self.pass_over(unit);
        }
        due.then_some(digest)
    }

    /// Leave `unit` for the next listing to judge: it was found not due, or
    /// `record_digest` declined to write its digest, so no read of it was
    /// attempted. A directory covered again by then has it read, and a unit a
    /// copy's row has settled is simply not listed.
    fn pass_over(&self, unit: &Unit) {
        self.settings.passing_over(&unit.path);
        locked(self.passed_over).push(key(unit));
    }
}

/// A worker's leaving, however it leaves, a panic included: counted out of the
/// queue unless it already was, and the coordinator woken to see it go.
struct Stopping<'r, 'a> {
    reading: &'r Reading<'a>,
    /// Counted out already, by the worker that found the queue empty.
    counted_out: bool,
}

impl Drop for Stopping<'_, '_> {
    fn drop(&mut self) {
        if !self.counted_out {
            locked(&self.reading.queue).workers -= 1;
        }
        let pass = &self.reading.state.fingerprints;
        {
            let _held = locked(&pass.state);
            pass.woken.notify_all();
        }
        self.reading.settings.stopped();
    }
}

/// `what` on stderr, as the pass's. Not `eprintln!`, which panics when
/// nothing reads stderr.
fn say(what: &str) {
    let _ = writeln!(std::io::stderr(), "taiyaku: the fingerprint pass: {what}");
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
