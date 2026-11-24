//! The works a server serves, kept in step with the library it serves them
//! from.
//!
//! **Why this is not a `Vec<Work>` built once.** `/api/works`, the reader's
//! routes and OPDS read the registry, while every `/api/library/*` route reads
//! the database and sees a scan the moment it commits. A list built at startup
//! would leave a volume a scan had just derived listed by the library and
//! refused by the reader until the process restarted (Part A
//! of `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`).
//!
//! **How it knows the library moved.** Migration 14 keeps a counter that
//! triggers bump on every write to a table a `Work` is projected from
//! ([`taiyaku_library::structure`]). A live registry remembers the value it
//! last read at and reads again when the counter differs. Triggers fire for
//! every connection in every process, so nothing that changes the library
//! has to remember to say so.
//!
//! **Two kinds.** [`Works::library`] is live: `serve` with no paths, the
//! whole library. [`Works::fixed`] never reads the library again: `serve
//! foo/taiyaku.json` serves the works it was handed (O61 is that mode's own
//! row), and so does every fixture that builds its works by hand.

// `eprintln!` panics when nothing reads stderr, and a panic here stops the
// refresher or fails the request a lookup miss is serving. Every line this
// module prints is written with `writeln!` and its error ignored instead.
#![deny(clippy::print_stderr)]

use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use rusqlite::Connection;
use taiyaku_core::Sidecar;
use taiyaku_library::Library;

use crate::Work;

/// Reads one work's document out of the library. `project::sidecar` in
/// production; a wrapper around it in the tests that need to see when a
/// refresh reads.
pub(crate) type Project<'a> = dyn Fn(&Connection, i64) -> Result<Sidecar, String> + 'a;

/// What [`Works::seen`] holds before anything has been read. No value of the
/// counter equals it, so the first refresh always reads.
const NEVER: i64 = i64::MIN;

/// One `work` row: as much of it as a [`Work`] is keyed and located by.
#[derive(Debug)]
pub(crate) struct WorkRow {
    pub(crate) id: i64,
    pub(crate) slug: String,
    sidecar_path: Option<String>,
    /// `work.missing_since` is set (O91).
    pub(crate) missing: bool,
}

impl WorkRow {
    /// Where the work's `taiyaku.json` is, or an empty path for a scanned
    /// work, which has none. [`Work::has_document`] reads that as "no
    /// document".
    pub(crate) fn path(&self) -> PathBuf {
        self.sidecar_path
            .as_deref()
            .map_or_else(PathBuf::new, PathBuf::from)
    }
}

/// Every `work` row, in id order.
///
/// # Errors
///
/// Returns an error if the rows cannot be read.
pub(crate) fn work_rows(conn: &Connection) -> Result<Vec<WorkRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id, slug, sidecar_path, missing_since IS NOT NULL FROM work ORDER BY id")
        .map_err(|e| format!("listing the library's works: {e}"))?;
    stmt.query_map([], |row| {
        Ok(WorkRow {
            id: row.get(0)?,
            slug: row.get(1)?,
            sidecar_path: row.get(2)?,
            missing: row.get(3)?,
        })
    })
    .map_err(|e| format!("listing the library's works: {e}"))?
    .collect::<rusqlite::Result<_>>()
    .map_err(|e| format!("reading the library's works: {e}"))
}

/// The works at one moment: in the order they were read, and by slug for a
/// lookup.
#[derive(Debug, Default)]
pub struct Snapshot {
    order: Vec<Arc<Work>>,
    by_slug: HashMap<String, Arc<Work>>,
}

impl Snapshot {
    fn new(order: Vec<Arc<Work>>) -> Self {
        let by_slug = order
            .iter()
            .map(|work| (work.id.clone(), Arc::clone(work)))
            .collect();
        Self { order, by_slug }
    }
}

impl std::ops::Deref for Snapshot {
    type Target = [Arc<Work>];

    fn deref(&self) -> &[Arc<Work>] {
        &self.order
    }
}

/// What one call to [`Works::refresh_if_moved`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Refresh {
    /// The counter had not moved, or the registry is fixed: nothing was read.
    Unmoved,
    /// Every work was read again. `kept` names each one whose rows would not
    /// project. It keeps the document it had or, if it is new, is not served
    /// until its rows do project.
    Refreshed { kept: Vec<String> },
}

/// The works a server serves. See the module's own doc comment.
#[derive(Debug)]
pub struct Works {
    held: RwLock<Arc<Snapshot>>,
    live: bool,
    /// The counter's value when [`Self::held`] was last read, or [`NEVER`].
    seen: AtomicI64,
    /// One refresh at a time. The refresher's tick and a lookup's miss can
    /// both ask at once, and the second only has to find the counter
    /// already caught up with.
    refreshing: Mutex<()>,
    /// The last complaint [`Self::report`] printed, as [`sameness`] keeps it,
    /// so a failure is said once rather than every tick.
    last_notice: Mutex<Option<String>>,
}

impl Works {
    fn empty(live: bool) -> Self {
        Self {
            held: RwLock::new(Arc::default()),
            live,
            seen: AtomicI64::new(NEVER),
            refreshing: Mutex::new(()),
            last_notice: Mutex::new(None),
        }
    }

    /// A registry that serves `works` and never reads the library again.
    #[must_use]
    pub fn fixed(works: Vec<Work>) -> Self {
        let registry = Self::empty(false);
        *registry
            .held
            .write()
            .unwrap_or_else(PoisonError::into_inner) =
            Arc::new(Snapshot::new(works.into_iter().map(Arc::new).collect()));
        registry
    }

    /// A live registry over every work in `library`.
    ///
    /// **A work whose rows will not project is left out and named on stderr**,
    /// the way a running refresh names one. Refusing the whole library would
    /// turn one bad row into a restart loop under an auto-deploy (spec §2.3).
    ///
    /// # Errors
    ///
    /// Returns an error only if the library cannot be read at all.
    pub fn library(library: &Library) -> Result<Self, String> {
        let works = Self::empty(true);
        works.refresh(library, &taiyaku_library::project::sidecar)?;
        Ok(works)
    }

    /// Whether this registry reads the library again when it moves.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// The works as they stand. Never reads the library: a read lock held
    /// only long enough to clone an `Arc`.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.held.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// The work `slug` names, if this registry holds it now. Never reads the
    /// library; `ServeState::find` is the lookup that refreshes on a miss.
    #[must_use]
    pub fn get(&self, slug: &str) -> Option<Arc<Work>> {
        self.snapshot().by_slug.get(slug).cloned()
    }

    /// How many works are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    /// Whether none are.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snapshot().is_empty()
    }

    /// Read every work again if the library's structure has moved since the
    /// last read. A fixed registry answers [`Refresh::Unmoved`] at once.
    ///
    /// **Blocks: it reads the database.** Call it from the blocking pool or
    /// a thread of its own, never from a runtime worker.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be read at all. A work whose
    /// rows will not project is not an error here: it is named in
    /// [`Refresh::Refreshed`]'s `kept` and keeps the document it had.
    ///
    /// **It prints to stderr** when what it found differs from what it last
    /// printed: a failure once, and the recovery once.
    pub fn refresh_if_moved(&self, library: &Library) -> Result<Refresh, String> {
        self.refresh_if_moved_with(library, &taiyaku_library::project::sidecar)
    }

    /// [`Self::refresh_if_moved`], reading each work through `project`.
    pub(crate) fn refresh_if_moved_with(
        &self,
        library: &Library,
        project: &Project<'_>,
    ) -> Result<Refresh, String> {
        if !self.live {
            return Ok(Refresh::Unmoved);
        }
        self.refresh(library, project)
    }

    /// One refresh, reported to stderr before the next may begin, so reports
    /// reach the terminal in the order the refreshes ran. A report made after
    /// `refreshing` was released could land behind a later refresh's and
    /// leave [`Self::last_notice`] on an older failure.
    ///
    /// A refresh that fails marks the registry unread ([`NEVER`]), so the next
    /// one reads again whatever the counter says and its outcome raises or
    /// clears the complaint. Without that, a failure before the counter was
    /// read would be followed by `Unmoved`, which clears nothing, and the
    /// terminal would stay on the failure until the next structural write.
    fn refresh(&self, library: &Library, project: &Project<'_>) -> Result<Refresh, String> {
        let _one_at_a_time = self
            .refreshing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let outcome = self.read_all(library, project);
        if outcome.is_err() {
            self.seen.store(NEVER, Ordering::Release);
        }
        self.report(&outcome);
        outcome
    }

    /// Read every work again if the counter has moved. The caller holds
    /// `refreshing`.
    fn read_all(&self, library: &Library, project: &Project<'_>) -> Result<Refresh, String> {
        let conn = library.working()?;
        // **Read before the works, never after.** A write that commits while
        // the works below are being read leaves the counter ahead of what is
        // recorded as seen, and the next refresh reads again. Read
        // afterwards, and that write would be recorded as seen without ever
        // having been read.
        let now = taiyaku_library::structure::version(&conn)?;
        if self.seen.load(Ordering::Acquire) == now {
            return Ok(Refresh::Unmoved);
        }
        let before = self.snapshot();
        let mut order = Vec::new();
        let mut kept = Vec::new();
        for row in work_rows(&conn)? {
            let path = row.path();
            let held = before
                .by_slug
                .get(&row.slug)
                .filter(|held| held.path == path);
            let read = match held {
                // **The same `Work`, read again under its own lock**, so its
                // notes lock and its correction lock stay the ones every
                // request in flight already holds (`Work::reproject`).
                Some(held) => held
                    .reproject(|| read_one(&conn, project, row.id))
                    .map(|()| Arc::clone(held)),
                // New, or its document moved. `Work::path` is fixed for a
                // `Work`'s life, so a moved document is a new `Work`.
                None => read_one(&conn, project, row.id)
                    .map(|sidecar| Arc::new(Work::new(row.slug.clone(), path, sidecar))),
            };
            // The mark is set on whichever `Work` is served, held or new: it
            // comes from the row, which reads even when the document will not.
            match read {
                Ok(work) => {
                    work.set_missing(row.missing);
                    order.push(work);
                }
                Err(why) => {
                    kept.push(format!("`{}`: {why}", row.slug));
                    if let Some(previous) = before.by_slug.get(&row.slug) {
                        previous.set_missing(row.missing);
                        order.push(Arc::clone(previous));
                    }
                }
            }
        }
        *self.held.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(Snapshot::new(order));
        self.seen.store(now, Ordering::Release);
        Ok(Refresh::Refreshed { kept })
    }

    /// Print what one refresh means to whoever is watching the server's
    /// stderr: once per change, never once per tick. Called by [`Self::refresh`]
    /// while it holds `refreshing`.
    fn report(&self, outcome: &Result<Refresh, String>) {
        let Some(now) = complaint(outcome) else {
            return;
        };
        let mut last = self
            .last_notice
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(line) = notice(&mut last, now) {
            // Not `eprintln!`, which panics when nothing reads stderr, out
            // through whoever asked for this refresh.
            let _ = writeln!(std::io::stderr(), "{line}");
        }
    }
}

/// How often a live registry asks whether the library has moved, and looks for
/// a scan another process ran ([`crate::cli_watch`]). The question is one
/// connection opened and a one-row read, and the look a second connection, a
/// read of every directory's last scan and of the structure version, and a
/// probe of the derive lock file. At this rate a volume a scan derives is
/// listed even by the routes that never name a work, `/api/works` and the OPDS
/// feed, and an open page reads again soon after a `library scan` ends.
pub(crate) const REFRESH_EVERY: Duration = Duration::from_secs(2);

/// Every `every`, ask whether `state`'s library has moved, and read its works
/// again when it has; then look for a scan another process ran, by every
/// directory's last scan, the structure version and the derive lease, with one
/// [`crate::cli_watch::CliWatch`] kept from tick to tick. Both on the blocking
/// pool, because both read the database.
///
/// `serve` spawns this for a live registry only and lets it run for the
/// runtime's life. It answers the task so that a test can stop it.
pub(crate) fn spawn_refresher(
    state: Arc<crate::ServeState>,
    every: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(every);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut watch = crate::cli_watch::CliWatch::default();
        loop {
            ticks.tick().await;
            let asking = Arc::clone(&state);
            let mut watching = std::mem::take(&mut watch);
            match tokio::task::spawn_blocking(move || {
                // The refresh reports itself, in its own order (`Works::refresh`).
                let _ = asking.works.refresh_if_moved(&asking.library);
                watching.look(&asking);
                watching
            })
            .await
            {
                Ok(looked) => watch = looked,
                // The watch is lost with the panic, and its fresh one's first
                // look raises nothing.
                Err(panicked) => {
                    // Not `eprintln!`, which panics when nothing reads stderr,
                    // and a panic here would end the refresher for good.
                    let _ = writeln!(
                        std::io::stderr(),
                        "taiyaku: the refresher panicked: {panicked}"
                    );
                }
            }
        }
    })
}

/// One work's document, read inside a read transaction of its own, so it
/// comes from one committed state of the library rather than from both
/// sides of a write that landed between two of `project`'s queries.
///
/// **`BEGIN DEFERRED`, spelled out.** `Library::configure` makes every
/// `unchecked_transaction` on its connections `IMMEDIATE`, which takes the
/// write lock. That is right for the writers it was set for; for a read it
/// would make every refresh wait out, and hold up, whichever writer was
/// running.
fn read_one(conn: &Connection, project: &Project<'_>, id: i64) -> Result<Sidecar, String> {
    let read = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Deferred)
        .map_err(|e| format!("starting a read of work {id}: {e}"))?;
    let reading: &Connection = &read;
    project(reading, id)
}

/// What a refresh's outcome says: `None` when it read nothing, so it can
/// neither raise a complaint nor clear one; `Some(None)` when it read every
/// work cleanly; `Some(Some(why))` otherwise.
fn complaint(outcome: &Result<Refresh, String>) -> Option<Option<String>> {
    match outcome {
        Ok(Refresh::Unmoved) => None,
        Ok(Refresh::Refreshed { kept }) if kept.is_empty() => Some(None),
        Ok(Refresh::Refreshed { kept }) => Some(Some(format!(
            "these works could not be read, and each is served as it last was, or not at all \
             if it never was: {}",
            kept.join("; ")
        ))),
        Err(why) => Some(Some(why.clone())),
    }
}

/// The line to print for `now`, given that `last` was the last thing
/// printed, as [`sameness`] keeps it, or nothing when it would only repeat
/// it.
fn notice(last: &mut Option<String>, now: Option<String>) -> Option<String> {
    let key = now.as_deref().map(sameness);
    if *last == key {
        return None;
    }
    let line = match now {
        Some(why) => format!("taiyaku: could not refresh the served works: {why}"),
        None => "taiyaku: the served works are being refreshed again".to_string(),
    };
    *last = key;
    Some(line)
}

/// What one complaint is compared with the last by: its words without the
/// figure after `(waited `. A failure to open the library says how long it
/// waited for another process (`taiyaku_library`'s `set_wal_within`), and
/// that figure changes from one attempt to the next, so a library that stays
/// unopenable would otherwise be a new failure, printed again, every tick.
/// The line printed keeps the figure.
fn sameness(why: &str) -> String {
    const WAITED: &str = "(waited ";
    let Some((before, figure_on)) = why.split_once(WAITED) else {
        return why.to_string();
    };
    let after = figure_on.split_once(' ').map_or("", |(_, after)| after);
    format!("{before}{WAITED}{after}")
}

#[cfg(test)]
mod tests;
