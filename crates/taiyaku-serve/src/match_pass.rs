//! The match pass: in the background, the server looks up each file taking
//! part in content matching that it has not looked up yet, scores it against
//! the files the lookup leads to, and stores each pair from the floor up for
//! derive to read. Design:
//! `docs/superpowers/specs/2026-11-19-content-matching-volume-joins-design.md`,
//! §2.
//!
//! One pass at a time across the store, on threads of its own, as the
//! fingerprint pass runs: a coordinator lists the files due, and a few workers
//! look them up. A file's pairs and its looked-up mark are stored in one
//! transaction as its lookup ends, so a pass stopped partway, by a cancel or a
//! restart, resumes at the next file.
//!
//! The fingerprint pass starts it as it goes idle uncancelled. Where a listing
//! stored a pair at the bar, the pass derives once its lookups end, as a scan
//! derives: a pair joins only at a derive (spec §2, "After").

use std::collections::HashSet;
use std::io::Write as _;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError, mpsc};
use std::time::{Duration, Instant};

use taiyaku_core::Slot;
use taiyaku_library::matches::{self, BAR, FLOOR, NewPair, Participant};
use taiyaku_library::progress::Step;
use taiyaku_library::{derive, fingerprints, lease};

use crate::ServeState;
use crate::fingerprint::{self, PagePrint};
use crate::fingerprint_pass::priority;
use crate::scan_jobs::{Job, JobEnded, panic_message};
use crate::volume_match::{self, Entry, SCORER_VERSION, Slots};

/// How a pass runs.
pub(crate) struct PassSettings {
    /// Worker threads looking files up side by side.
    pub(crate) workers: usize,
    /// How often a worker, or the derive, waiting for the job lock looks
    /// again.
    pub(crate) poll: Duration,
    /// Asked with each digest once its lookup has been stored, or refused.
    #[cfg(test)]
    pub(crate) after_lookup: Option<DigestHook>,
    /// Asked with each pair as it is scored, the smaller digest first.
    #[cfg(test)]
    pub(crate) after_scoring: Option<PairHook>,
    /// Asked once each derive the pass runs has ended, and the feed's `ended`
    /// has moved.
    #[cfg(test)]
    pub(crate) after_derive: Option<Hook>,
}

/// What [`PassSettings::after_derive`] calls.
#[cfg(test)]
pub(crate) type Hook = Arc<dyn Fn() + Send + Sync>;

/// What [`PassSettings::after_lookup`] calls.
#[cfg(test)]
pub(crate) type DigestHook = Arc<dyn Fn(&str) + Send + Sync>;

/// What [`PassSettings::after_scoring`] calls.
#[cfg(test)]
pub(crate) type PairHook = Arc<dyn Fn(&str, &str) + Send + Sync>;

impl Default for PassSettings {
    fn default() -> Self {
        Self {
            // The fingerprint pass's number, so neither pass takes more of the
            // machine from its readers than the other.
            workers: 4,
            poll: Duration::from_secs(1),
            #[cfg(test)]
            after_lookup: None,
            #[cfg(test)]
            after_scoring: None,
            #[cfg(test)]
            after_derive: None,
        }
    }
}

impl PassSettings {
    #[cfg(test)]
    fn looked_up(&self, digest: &str) {
        if let Some(hook) = &self.after_lookup {
            hook(digest);
        }
    }

    #[cfg(not(test))]
    fn looked_up(&self, _: &str) {}

    #[cfg(test)]
    fn scoring(&self, pair: &(String, String)) {
        if let Some(hook) = &self.after_scoring {
            hook(&pair.0, &pair.1);
        }
    }

    #[cfg(not(test))]
    fn scoring(&self, _: &(String, String)) {}

    #[cfg(test)]
    fn derived(&self) {
        if let Some(hook) = &self.after_derive {
            hook();
        }
    }

    #[cfg(not(test))]
    fn derived(&self) {}
}

/// The match pass in [`ServeState`]: whether one runs, and its job.
#[derive(Default)]
pub struct Pass {
    /// `None` for a pass that never starts.
    settings: Option<PassSettings>,
    state: Mutex<State>,
    /// Where the pass's derives run: `None` for a pass that never starts, or
    /// one whose thread could not be started.
    deriver: Option<Deriver>,
}

/// A derive, as [`Deriver`] is handed it.
type Work = Box<dyn FnOnce() + Send>;

/// The thread that runs the pass's derives, never lowered. A derive holds the
/// job lock and the derive lease throughout: a pressed scan and a CLI derive
/// wait for them, and a rule save, a stamp, a move or a removal is refused.
/// Run on the coordinator, lowered to the idle class, each would wait on an
/// idle-class thread. A thread starts at its spawner's priority and cannot
/// raise its own, and the pass is kicked from the fingerprint pass's lowered
/// coordinator, so the one thread never lowered is this, spawned where the
/// pass is built, with the server.
struct Deriver {
    to: mpsc::Sender<Work>,
}

impl Deriver {
    /// The thread, at the calling thread's priority, or `None`, said.
    fn spawn() -> Option<Self> {
        let (to, from) = mpsc::channel::<Work>();
        let spawned = std::thread::Builder::new()
            .name("match-derive".to_owned())
            .spawn(move || {
                // Until the pass, and with it `to`, is dropped. A derive that
                // panics leaves the thread to run the next.
                for work in from {
                    if let Err(payload) = std::panic::catch_unwind(AssertUnwindSafe(work)) {
                        say(&format!(
                            "the derive panicked: {}",
                            panic_message(payload.as_ref())
                        ));
                    }
                }
            });
        match spawned {
            Ok(_) => Some(Self { to }),
            Err(e) => {
                say(&format!("could not start the derive's thread: {e}"));
                None
            }
        }
    }
}

#[derive(Default)]
struct State {
    /// A coordinator is starting or running.
    busy: bool,
    /// A kick came while busy, and since the job's cancel where it has one:
    /// the coordinator lists again before it ends.
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
            deriver: Deriver::spawn(),
            ..Self::default()
        }
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

/// Start a pass where none runs; where one does, have it list again before it
/// ends, so a file fingerprinted meanwhile is looked up by it rather than by
/// the next. An inert [`Pass`] starts nothing.
pub(crate) fn kick(state: &Arc<ServeState>) {
    let pass = &state.matches;
    if pass.settings.is_none() {
        return;
    }
    {
        let mut held = locked(&pass.state);
        if held.busy {
            held.again = true;
            return;
        }
        held.busy = true;
        held.again = false;
    }
    let coordinating = Arc::clone(state);
    let spawned = std::thread::Builder::new()
        .name("match-pass".to_owned())
        .spawn(move || coordinate(&coordinating));
    if let Err(e) = spawned {
        locked(&pass.state).busy = false;
        say(&format!("could not start: {e}"));
    }
}

/// The coordinator: list what is due, look it up, derive where a pair reached
/// the bar, list again at a kick, and end the job once a listing that no kick
/// has outdated is done.
fn coordinate(state: &Arc<ServeState>) {
    let pass = &state.matches;
    let mut ending = Ending {
        pass,
        job: None,
        armed: true,
    };
    let Some(settings) = &pass.settings else {
        return;
    };
    if let Err(why) = priority::lower() {
        say(&format!("looks up at the server's own priority: {why}"));
    }
    let counts = Mutex::new(Counts::default());
    loop {
        if let Some(listing) = listed(state) {
            let job = Arc::clone(ending.job.get_or_insert_with(|| started(state)));
            if !job.cancelling() {
                let looking = Looking {
                    state,
                    settings,
                    job: &job,
                    listing: &listing,
                    counts: &counts,
                    next: AtomicUsize::new(0),
                    at_bar: AtomicUsize::new(0),
                };
                looking.all();
                let at_bar = looking.at_bar.load(Ordering::SeqCst);
                if at_bar > 0 {
                    derive_aside(state, &job, at_bar);
                }
            }
        }
        match ending.what_next() {
            Next::Idle => return,
            Next::Again => {}
            Next::Fresh => *locked(&counts) = Counts::default(),
        }
    }
}

/// What a listing found: every file taking part as the lookup reads it, and
/// which of them are due.
struct Listing {
    /// By digest, as [`volume_match::leaders`] needs them.
    entries: Vec<Entry>,
    /// Each entry's slot table, in the same order.
    tables: Vec<Vec<Slot>>,
    /// Each entry's page count, in the same order.
    pages: Vec<u32>,
    /// The entries no lookup has marked, in order.
    due: Vec<usize>,
    /// Each pair stored, the smaller digest first, and each this pass has
    /// scored since, so that no pair is scored twice.
    scored: Mutex<HashSet<(String, String)>>,
}

/// Every file taking part, where any is due a lookup; `None` where none is
/// or the store refuses a read. Each file's prints are read into slots and
/// let go, so a pass holds slots, not blobs.
fn listed(state: &ServeState) -> Option<Listing> {
    let read = state.library.working().and_then(|conn| {
        Ok((
            matches::participants(&conn, fingerprint::VERSION)?,
            matches::looked_up(&conn)?,
            matches::scored(&conn)?,
        ))
    });
    let (taking_part, looked_up, scored) = read.map_err(|why| say(&why)).ok()?;
    let mut listing = Listing {
        entries: Vec::with_capacity(taking_part.len()),
        tables: Vec::with_capacity(taking_part.len()),
        pages: Vec::with_capacity(taking_part.len()),
        due: Vec::new(),
        scored: Mutex::new(scored),
    };
    for participant in taking_part {
        let Some(prints) = prints_of(&participant) else {
            continue;
        };
        let Slots { hashes, table } = volume_match::slots(&prints, participant.reading);
        if !looked_up.contains(&participant.sha256) {
            listing.due.push(listing.entries.len());
        }
        listing.tables.push(table);
        listing
            .pages
            .push(u32::try_from(prints.len()).unwrap_or(u32::MAX));
        listing.entries.push(Entry {
            key: participant.sha256,
            slots: hashes,
            libraries: participant.libraries,
            series: participant.series_id,
            volume: Some(participant.volume),
        });
    }
    (!listing.due.is_empty()).then_some(listing)
}

/// `participant`'s pages, or `None`, said, where its blob does not decode
/// or holds a page with no print: a whole read hashes every page, so either
/// is a store this build cannot read, and the file takes no part.
fn prints_of(participant: &Participant) -> Option<Vec<PagePrint>> {
    let digest = &participant.sha256;
    let pages = fingerprint::decode(&participant.prints)
        .map_err(|why| {
            say(&format!(
                "{digest}'s prints do not decode, so it takes no part: {why}"
            ))
        })
        .ok()?;
    let prints: Option<Vec<PagePrint>> = pages.into_iter().collect();
    if prints.is_none() {
        say(&format!(
            "{digest} was read whole but a page has no print, so it takes no part"
        ));
    }
    prints
}

/// The pass's job, made once something is due, so a pass with nothing to look
/// up never shows: in the pass's state from now, for the feed, and naming the
/// libraries with matching on.
fn started(state: &ServeState) -> Arc<Job> {
    let job = Arc::new(Job::matching(Instant::now()));
    // Best-effort, as the fingerprint pass's are.
    if let Ok(conn) = state.library.working()
        && let Ok(coverage) = fingerprints::coverage(&conn)
    {
        *locked(&job.libraries) = coverage.libraries;
    }
    locked(&state.matches.state).job = Some(Arc::clone(&job));
    job
}

/// A job's count: the files workers have finished with, whatever came of each,
/// of those due in every listing so far.
#[derive(Debug, Default)]
struct Counts {
    done: u64,
    of: u64,
}

/// The lookups of one listing: what every worker shares with the coordinator.
struct Looking<'a> {
    state: &'a ServeState,
    settings: &'a PassSettings,
    job: &'a Job,
    listing: &'a Listing,
    /// The job's count, across every listing of its pass.
    counts: &'a Mutex<Counts>,
    /// The next of `listing.due` a worker takes.
    next: AtomicUsize,
    /// The pairs from [`BAR`] up that this listing's lookups stored.
    at_bar: AtomicUsize,
}

impl Looking<'_> {
    /// Look up every file due on the settings' workers, in order; return once
    /// every worker has stopped.
    fn all(&self) {
        {
            let mut counts = locked(self.counts);
            counts.of += self.listing.due.len() as u64;
            self.job.set_count(Step::Matching, counts.done, counts.of);
        }
        let workers = self.settings.workers.max(1).min(self.listing.due.len());
        std::thread::scope(|scope| {
            for n in 0..workers {
                let spawned = std::thread::Builder::new()
                    .name(format!("match-{n}"))
                    .spawn_scoped(scope, || self.work());
                if let Err(e) = spawned {
                    say(&format!("could not start a worker: {e}"));
                    break;
                }
            }
        });
    }

    /// One worker: take the next file due until none is left or the job
    /// cancels.
    fn work(&self) {
        // Its own, as each thread's priority is: inherited from the
        // coordinator here, but not from whatever thread spawns it next.
        let _ = priority::lower();
        loop {
            // Between files, so a cancel leaves the next file due.
            if self.job.cancelling() {
                return;
            }
            let at = self.next.fetch_add(1, Ordering::SeqCst);
            let Some(&query) = self.listing.due.get(at) else {
                return;
            };
            self.one(query);
            // Counted under the lock, so no count lands after a later one.
            let mut counts = locked(self.counts);
            counts.done += 1;
            self.job.set_count(Step::Matching, counts.done, counts.of);
        }
    }

    /// Look `query` up: score it against each of its leaders, and store the
    /// pairs from the floor up with its mark once the job lock is free, a
    /// cancel or not: a cancel stops new work, and the next pass would only
    /// score the file again.
    fn one(&self, query: usize) {
        let entries = &self.listing.entries;
        let found: Vec<NewPair> = volume_match::leaders(entries, query)
            .into_iter()
            .filter_map(|leader| self.pair(query, leader))
            .collect();
        self.until_the_lock_is_free();
        let digest = &entries[query].key;
        let stored = self.state.library.working().and_then(|conn| {
            matches::store(&conn, &found, digest, fingerprint::VERSION, SCORER_VERSION)
        });
        match stored {
            Ok(()) => {
                let at_bar = found.iter().filter(|pair| pair.v >= BAR).count();
                self.at_bar.fetch_add(at_bar, Ordering::SeqCst);
            }
            Err(why) => say(&why),
        }
        self.settings.looked_up(digest);
    }

    /// `query` and `leader` scored, as [`matches::store`] takes them, where
    /// no stored pair and no pair this pass has scored is the two and they
    /// reach the floor. Named by the side that went first, the shorter, which
    /// is the side the solver's runs start from.
    fn pair(&self, query: usize, leader: usize) -> Option<NewPair> {
        let listing = self.listing;
        let (q, l) = (&listing.entries[query], &listing.entries[leader]);
        let named = if q.key < l.key {
            (q.key.clone(), l.key.clone())
        } else {
            (l.key.clone(), q.key.clone())
        };
        if !locked(&listing.scored).insert(named.clone()) {
            return None;
        }
        self.settings.scoring(&named);
        let slots = |at: usize| Slots {
            hashes: listing.entries[at].slots.clone(),
            table: listing.tables[at].clone(),
        };
        let scored = volume_match::score((&q.key, &slots(query)), (&l.key, &slots(leader)));
        if scored.v < FLOOR {
            return None;
        }
        let (a, b) = if scored.swapped {
            (leader, query)
        } else {
            (query, leader)
        };
        Some(NewPair {
            sha_a: listing.entries[a].key.clone(),
            sha_b: listing.entries[b].key.clone(),
            v: scored.v,
            long_paired: scored.long_paired,
            a_slots: scored.a_slots,
            b_slots: scored.b_slots,
            a_pages: listing.pages[a],
            b_pages: listing.pages[b],
            floor: scored.floor,
            runs: volume_match::page_runs(&scored.runs, &listing.tables[a], &listing.tables[b]),
        })
    }

    /// Wait, looking again every poll, until the job lock is free, a cancel or
    /// not. A rule save's derive writes the whole store under the lock, and a
    /// write beside it would wait on the store's own lock or fail.
    fn until_the_lock_is_free(&self) {
        while self.state.library_job.is_held() {
            std::thread::sleep(self.settings.poll);
        }
    }
}

/// [`derive_after`] on the pass's [`Deriver`], returning once it has ended,
/// so the coordinator lists again only after the derive.
fn derive_aside(state: &Arc<ServeState>, job: &Arc<Job>, at_bar: usize) {
    let (deriving, job) = (Arc::clone(state), Arc::clone(job));
    let (done, ended) = mpsc::channel::<()>();
    let work: Work = Box::new(move || {
        derive_after(&deriving, &job, at_bar);
        let _ = done.send(());
    });
    let sent = match &state.matches.deriver {
        Some(deriver) => deriver.to.send(work).is_ok(),
        None => false,
    };
    if !sent {
        say(&format!(
            "stored {at_bar} {} at the bar, but no thread runs the pass's derive, so they \
             join at the next derive",
            pairs(at_bar)
        ));
        return;
    }
    // An error is a derive that panicked, which its thread has said.
    let _ = ended.recv();
}

/// "pair" or "pairs", for `count` of them.
fn pairs(count: usize) -> &'static str {
    if count == 1 { "pair" } else { "pairs" }
}

/// Derive the whole store, where this listing stored `at_bar` pairs from the
/// bar up, once the job lock and the derive lease are held, as a scan's derive
/// takes them: a derive beside another's, or beside a rule save's, would write
/// works from a store the other is rewriting. Each wait reads `job`'s cancel
/// between tries, so a cancel during either ends the job with no derive, and
/// the pairs wait for the next. A cancel during the derive is read once it
/// has ended. The feed's `ended` moves after it, so an open Library view reads
/// the works it joined.
fn derive_after(state: &ServeState, job: &Job, at_bar: usize) {
    let Some(settings) = &state.matches.settings else {
        return;
    };
    let Some(derived) = derived(state, settings.poll, job) else {
        return;
    };
    state.jobs_ended.ended.fetch_add(1, Ordering::SeqCst);
    settings.derived();
    say(&derived_line(at_bar, &derived));
}

/// What the pass says once a derive after storing `at_bar` pairs from the
/// bar up has ended: what it derived and how many files stand joined by
/// content, the count an admin reads to see the pairs did join; or why it
/// failed.
fn derived_line(at_bar: usize, derived: &Result<derive::DeriveReport, String>) -> String {
    let pairs = pairs(at_bar);
    match derived {
        Ok(report) => format!(
            "stored {at_bar} {pairs} at the bar, then derived {} series, {} works, {} editions \
             and {} members; {} {} joined by content",
            report.series,
            report.works,
            report.editions,
            report.members,
            report.joined,
            if report.joined == 1 { "file" } else { "files" }
        ),
        Err(why) => format!("stored {at_bar} {pairs} at the bar, then the derive failed: {why}"),
    }
}

/// The derive's report, run under the job lock and the lease; `None` where
/// `job` was cancelled first, or the store or the lease refused, said.
fn derived(
    state: &ServeState,
    poll: Duration,
    job: &Job,
) -> Option<Result<derive::DeriveReport, String>> {
    let _job_lock = the_job_lock(state, poll, job)?;
    let working = state.library.working().map_err(|why| say(&why)).ok()?;
    let lease = the_lease(&state.library, job)
        .map_err(|why| say(&why))
        .ok()??;
    Some(lease.holding(|| derive::derive_with(&working, derive::Documents::Reconcile)))
}

/// The job lock, waiting for it where another job holds it, looking again
/// every `poll`; `None` once `job` is cancelled. Polled, never a blocking
/// lock, so a cancel ends the wait at once rather than once the holder lets go.
fn the_job_lock<'a>(
    state: &'a ServeState,
    poll: Duration,
    job: &Job,
) -> Option<crate::JobGuard<'a>> {
    loop {
        if job.cancelling() {
            return None;
        }
        match state.library_job.try_lock() {
            Ok(held) => return Some(held),
            Err(TryLockError::Poisoned(poisoned)) => return Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => {}
        }
        std::thread::sleep(poll);
    }
}

/// The derive lease, waiting for it where another derive holds it, in this
/// process or another; `None` once `job` is cancelled. Each try waits the
/// lease's own short budget, so the cancel is read between tries.
fn the_lease<'a>(
    library: &'a taiyaku_library::Library,
    job: &Job,
) -> Result<Option<lease::DeriveLease<'a>>, String> {
    loop {
        if job.cancelling() {
            return Ok(None);
        }
        if let Some(lease) = library.derive_lease(lease::Wait::No)? {
            return Ok(Some(lease));
        }
    }
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

/// What the coordinator does once a listing is looked up.
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
    /// pass busy and is read here, or finds it idle and starts another.
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
                job.end(JobEnded::Cancelled(Step::Matching));
            }
            return Next::Fresh;
        }
        if let Some(job) = self.job.take() {
            job.end(if cancelling {
                JobEnded::Cancelled(Step::Matching)
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
                JobEnded::Cancelled(Step::Matching)
            } else {
                JobEnded::Finished
            });
        }
        held.again = false;
        held.busy = false;
    }
}

/// `what` on stderr, as the pass's. Not `eprintln!`, which panics when
/// nothing reads stderr.
fn say(what: &str) {
    let _ = writeln!(std::io::stderr(), "taiyaku: the match pass: {what}");
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
