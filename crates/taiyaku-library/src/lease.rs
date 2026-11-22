//! The derive lease: one whole-store derive at a time, across processes (O51).
//!
//! **Why a lease and not the store's own write lock.** A plain derive commits
//! one work at a time, so the write lock is free between works and two derives
//! interleave, each writing works from the entries it read at its start. The
//! lease is held from before a derive's first read to after its last write.
//!
//! **Why a lock file and not a row in the store.** The hold is SQLite's own
//! `BEGIN IMMEDIATE` on `<store>.derive-lock`, an fcntl lock the kernel drops
//! with the process however it ends, so nothing goes stale and nothing needs
//! an expiry. A row would need a migration, and a holder killed while it held
//! the row would leave it held.
//!
//! `docs/status.md`'s O51 bullet, "two derives at once", says what holds it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags};

/// What [`crate::Library::derive_lease`] does while another holds the lease.
pub enum Wait<'a> {
    /// Answer `Ok(None)` after [`TRY`].
    No,
    /// Wait until it is free, calling back with the time waited so far after
    /// each [`SLICE`] it was still held, so a waiter can say it waits.
    Yes(&'a mut dyn FnMut(Duration)),
}

impl std::fmt::Debug for Wait<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::No => "No",
            Self::Yes(_) => "Yes",
        })
    }
}

/// How long [`Wait::No`] waits: longer than [`held_elsewhere`]'s hold of a few
/// microseconds, so a status read never refuses a press, and far shorter than
/// any derive.
const TRY: Duration = Duration::from_millis(100);

/// How long [`Wait::Yes`] waits between callbacks.
const SLICE: Duration = Duration::from_millis(1000);

/// How often an in-memory library's [`Wait::Yes`] looks at the flag again.
const FLAG_POLL: Duration = Duration::from_millis(10);

/// The lease, held until dropped.
///
/// The connection drops first, which releases the lock file; then the flag
/// clears. In between, [`crate::Library::derive_running`] still says running,
/// which only holds a button for one more status read.
#[must_use = "the lease is released as soon as it is dropped"]
pub struct DeriveLease<'a> {
    _hold: Option<Connection>,
    _flag: FlagGuard<'a>,
}

impl std::fmt::Debug for DeriveLease<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeriveLease").finish_non_exhaustive()
    }
}

impl DeriveLease<'_> {
    /// Run `work`, a derive or a job no derive may overlap, while this lease
    /// is held.
    ///
    /// **Every such job runs through this**, so a lease bound to `_`, which
    /// drops at once, leaves no name to run the job by and does not compile.
    pub fn holding<R>(&self, work: impl FnOnce() -> R) -> R {
        work()
    }
}

/// A [`crate::Library`]'s own flag, set while one of its leases lives.
struct FlagGuard<'a>(&'a AtomicBool);

impl Drop for FlagGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The lock file of the store at `store`.
pub(crate) fn lock_path(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_owned();
    name.push(".derive-lock");
    PathBuf::from(name)
}

/// [`crate::Library::derive_lease`] for an in-memory library, whose flag is the
/// whole lease: its one connection's mutex already serialises its work, and no
/// other process can open it.
pub(crate) fn take_flag<'a>(flag: &'a AtomicBool, mut wait: Wait<'_>) -> Option<DeriveLease<'a>> {
    let started = Instant::now();
    let mut next_call = SLICE;
    loop {
        if flag
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Some(DeriveLease {
                _hold: None,
                _flag: FlagGuard(flag),
            });
        }
        let waited = started.elapsed();
        match &mut wait {
            Wait::No if waited >= TRY => return None,
            Wait::Yes(call) if waited >= next_call => {
                call(waited);
                next_call += SLICE;
            }
            Wait::No | Wait::Yes(_) => {}
        }
        std::thread::sleep(FLAG_POLL);
    }
}

/// [`crate::Library::derive_lease`] for the store at `store`: the lock file's
/// connection inside `BEGIN IMMEDIATE`, with `flag` set.
///
/// **No [`crate::Library::configure`].** The lock file holds no data: it needs
/// no WAL, whose `-shm` a probe would have to map, and no FTS5.
pub(crate) fn take_file<'a>(
    store: &Path,
    flag: &'a AtomicBool,
    mut wait: Wait<'_>,
) -> Result<Option<DeriveLease<'a>>, String> {
    let path = lock_path(store);
    let fail = |doing: &str, e: rusqlite::Error| {
        format!("{doing} the derive lock file {}: {e}", path.display())
    };
    let created = !path.exists();
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| fail("opening", e))?;
    // SQLite opens a file it cannot write read-only without saying so, and a
    // read-only `BEGIN IMMEDIATE` takes no write lock: such a lease would
    // exclude nothing. The hold below refuses any lease without the write
    // lock; this refusal comes first to name the usual cause and its fix.
    if conn
        .is_readonly(rusqlite::MAIN_DB)
        .map_err(|e| fail("opening", e))?
    {
        return Err(format!(
            "the derive lock file {} cannot be written by this process, so no derive can \
             take its turn: chown it to the user taiyaku runs as, or make it writable \
             (chmod u+w)",
            path.display()
        ));
    }
    if created {
        take_the_stores_owner(store, &path);
    }
    let budget = match wait {
        Wait::No => TRY,
        Wait::Yes(_) => SLICE,
    };
    conn.busy_timeout(budget)
        .map_err(|e| fail("setting the busy timeout of", e))?;
    initialise(&conn).map_err(|e| fail("initialising", e))?;
    let started = Instant::now();
    loop {
        match conn.execute_batch("BEGIN IMMEDIATE") {
            Ok(()) => {
                // SQLite also reads a file as read-only from its header, which
                // `initialise` reads after the check above, and there
                // `BEGIN IMMEDIATE` begins a read transaction and answers OK.
                let state = conn
                    .transaction_state(Some(rusqlite::MAIN_DB))
                    .map_err(|e| fail("holding", e))?;
                if state != rusqlite::TransactionState::Write {
                    return Err(format!(
                        "the derive lock file {} took no write lock, so no derive can \
                         take its turn: SQLite reads it as read-only, as it does a \
                         file whose header is damaged. It holds no data, so remove \
                         it: the next derive makes it afresh",
                        path.display()
                    ));
                }
                flag.store(true, Ordering::SeqCst);
                return Ok(Some(DeriveLease {
                    _hold: Some(conn),
                    _flag: FlagGuard(flag),
                }));
            }
            Err(e) if crate::is_busy(&e) => match &mut wait {
                Wait::No => return Ok(None),
                Wait::Yes(call) => call(started.elapsed()),
            },
            Err(e) => return Err(fail("holding", e)),
        }
    }
}

/// Give a new lock file the store's owner and mode, as SQLite gives the
/// store's `-wal`, so that a root `library scan` on the host leaves a file the
/// server can still write. Best-effort: a process that may not do either keeps
/// what it made, and [`take_file`] refuses it there if it cannot be written.
fn take_the_stores_owner(store: &Path, lock: &Path) {
    use std::os::unix::fs::MetadataExt;

    let Ok(stores) = std::fs::metadata(store) else {
        return;
    };
    let _ = std::os::unix::fs::chown(lock, Some(stores.uid()), Some(stores.gid()));
    let _ = std::fs::set_permissions(lock, stores.permissions());
}

/// Give a new lock file its one page, once, so that a hold has a database to
/// lock and writes nothing.
///
/// A busy answer means another holds it, and a holder initialised it first,
/// so the hold that follows waits as it would have.
fn initialise(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != 0 {
        return Ok(());
    }
    match conn.pragma_update(None, "user_version", 1) {
        Err(e) if crate::is_busy(&e) => Ok(()),
        done => done,
    }
}

/// Whether another connection holds the lease on the store at `store`.
///
/// **Safe to call on every status read**: it never waits, never creates the
/// file, and writes nothing, so it cannot hold up a derive or a press. Any
/// error reads as not running, as does a lock file this process cannot write,
/// where it cannot tell: a page's status read stays whole, and the press it
/// would hold meets [`take_file`]'s refusal, which names the file.
pub(crate) fn held_elsewhere(store: &Path) -> bool {
    let Ok(conn) = Connection::open_with_flags(
        lock_path(store),
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return false;
    };
    if conn.busy_timeout(Duration::ZERO).is_err() {
        return false;
    }
    match conn.execute_batch("BEGIN IMMEDIATE") {
        Ok(()) => {
            let _ = conn.execute_batch("ROLLBACK");
            false
        }
        Err(e) => crate::is_busy(&e),
    }
}

#[cfg(test)]
mod tests;
