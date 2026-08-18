//! The library store: many works, their editions and members, and the
//! per-person state that hangs off them.
//!
//! **A store and a projection, and deliberately nothing else.** Every rule
//! about what a re-solve does to a hand correction lives in `taiyaku-core`,
//! where it is already tested; this crate reaches it through
//! `Sidecar::merge_spoke` (see `import::reimport`) rather than reimplementing
//! it over rows. See `docs/superpowers/specs/2026-08-14-library-store-design.md`,
//! §4.2.

pub mod derive;
pub mod export;
pub mod identity;
pub mod import;
pub mod librarydoc;
pub mod migrate;
pub mod project;
pub mod proposals;
pub mod rules;
pub mod scan;
pub mod schema;
pub mod state;
pub mod watch;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

// Per-person state, moved down from `taiyaku-serve` because this crate
// cannot depend on the server — the dependency points the other way.
// `taiyaku-serve` re-exports each of these from its old module paths, so
// every existing consumer keeps compiling unchanged. See
// `docs/superpowers/specs/2026-08-14-library-store-design.md`, §3.4 and §4.3.
//
// `TruthDoc`, `TruthEdition` and `TruthRegion` do NOT move: they already
// live in `taiyaku-core`, which this crate depends on.

/// How far through a work a reader got, and in which edition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub edition: String,
    pub slot: u32,
}

/// A rectangle on one slot, in fractions of that slot's own width and height.
///
/// Fractions rather than pixels, and the whole marker arithmetic is scale-free
/// as a result — a tab or a tint tracks correctly at any fit scale with no
/// extra work.
///
/// **Not** `taiyaku_core::regions::Region`, which is a detected text region
/// rather than a note's anchor. The two share a name and nothing else; do not
/// merge them and do not `use` one where the other is meant.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One note.
///
/// `region` is optional and absent means the whole slot — a degenerate case
/// rather than a second kind of note, so nothing downstream needs two code
/// paths. `created`/`updated` are RFC 3339 strings minted by the client: the
/// server has no clock in the format, and a note written offline should carry
/// the time it was written rather than the time it reached a server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: String,
    pub edition: String,
    pub slot: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    /// Which colour a reader gave this note, as a name rather than as pixels.
    ///
    /// A **token**, never CSS: `taiyaku.json` carries correspondence and this
    /// document carries what a reader wrote, and neither is a stylesheet.
    /// What `amber` looks like is the reader's business, and a second reader
    /// — a native shell, a printout — is entitled to a different answer.
    ///
    /// Absent means the default, so every note written before colours existed
    /// stays valid and reads as it always did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub body: String,
    pub created: String,
    pub updated: String,
}

/// Every note on one work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
    pub work: String,
    pub notes: Vec<Note>,
}

/// **How long any of this waits for another process, in one place.**
///
/// **Two of this binary's own commands are expected to hold this file at
/// once.** `taiyaku serve`, `taiyaku library import` and `taiyaku library
/// export` all default to the same `$XDG_DATA_HOME/taiyaku/library.db`, and
/// `serve` writes a `progress` row on every page turn. Without a wait, an
/// import started while the server is running exits 1 on "database is
/// locked", and a reader's `PUT /progress` returns 500 and drops the position
/// rather than retrying.
///
/// Five seconds: longer than any single write this store performs — the
/// largest is one work's import, milliseconds — and short enough that a
/// genuinely stuck writer surfaces as an error someone can act on rather than
/// a server that looks hung. Set here rather than left to `rusqlite`, which
/// happens to set the same value on every connection: an inherited default is
/// not a decision, and this file is where the next person looks for the
/// decision.
///
/// [`set_wal`] waits the same five seconds by hand, because SQLite will not
/// wait them for it. One constant so the two cannot drift into two different
/// answers to "how long is this worth waiting for".
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// How long [`set_wal`] sleeps between attempts. Long enough that a first open
/// racing another first open costs at most one of these, short enough that
/// fifty of them fit inside [`BUSY_TIMEOUT`].
const WAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Whether SQLite refused because someone else holds a lock.
///
/// Both codes mean that and this crate treats them alike: `SQLITE_LOCKED` is
/// the same conflict inside one process (a second connection on the same
/// file, which is what a test — and a `serve` that also imports — produces),
/// `SQLITE_BUSY` between two.
fn is_busy(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::DatabaseBusy
                || err.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

/// `journal_mode = WAL`, retried while another connection is in the way.
///
/// **The busy timeout does not cover this one statement**, which is why it is
/// the only pragma here with a loop around it. Switching journal mode takes a
/// brief exclusive lock on the database, and SQLite runs no busy handler for
/// it: if any other connection holds the file, the pragma returns
/// `SQLITE_BUSY` on the spot. Reordering it after the timeout — done, above,
/// because that is where the decision belongs — does not change that.
///
/// It only ever bites on the **first** open of a fresh library: once the file
/// is in WAL, this pragma is a no-op that takes no lock at all. That is also
/// the one open every install performs, and `serve` plus `library import`
/// starting together is ordinary, so review 3 measured 14 failures across 48
/// concurrent first opens — half of them here, reported as
/// `opening the library: setting WAL: database is locked`, with nothing
/// anywhere saying that a retry recovers.
///
/// A thin wrapper over [`set_wal_within`] for the same reason
/// [`require_fts5`] wraps [`require_fts5_with_probe`]: the branch worth
/// pinning is the one that gives up, and a test cannot hold a lock for
/// [`BUSY_TIMEOUT`] without costing the suite five seconds.
fn set_wal(conn: &Connection) -> Result<(), String> {
    set_wal_within(conn, BUSY_TIMEOUT)
}

fn set_wal_within(conn: &Connection, budget: Duration) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match conn.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(e) if is_busy(&e) && started.elapsed() < budget => {
                std::thread::sleep(WAL_RETRY_INTERVAL.min(budget));
            }
            Err(e) => {
                return Err(format!(
                    "setting WAL: {e} (waited {:.0?} for another process to let go of the \
                     file; if one is genuinely stuck, closing it and opening this again is \
                     all that is needed — nothing has been written)",
                    started.elapsed()
                ));
            }
        }
    }
}

/// An open library.
///
/// One connection behind a `Mutex` rather than a pool. The write volume this
/// serves is a reading position and the occasional note; a pool would add a
/// dependency and a failure mode to solve a contention problem nobody has.
///
/// **That argument is about one process.** Contention *between* processes is
/// real and expected — `serve` and `library import` share one default
/// database — and is handled where it has to be, on the connection: see the
/// busy timeout and transaction behaviour in [`Library::from_connection`].
#[derive(Debug)]
pub struct Library {
    conn: Mutex<Connection>,
}

impl Library {
    /// Open the library at `path`, creating and migrating it if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened — a missing parent
    /// directory or a permission refusal, with the message naming the path —
    /// or if applying the schema fails.
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_connection(conn)
    }

    /// An in-memory library, for tests.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite refuses the connection or the schema fails
    /// to apply.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("in memory: {e}"))?;
        Self::from_connection(conn)
    }

    fn from_connection(mut conn: Connection) -> Result<Self, String> {
        // Per connection, not per database, and off by default. See the test
        // in this module's `tests.rs`.
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("enabling foreign keys: {e}"))?;
        // **Before the pragma most likely to meet another process**, which is
        // the next one. See `set_wal` for why the timeout does not on its own
        // save that one, and `BUSY_TIMEOUT` for the five seconds.
        conn.pragma_update(
            None,
            "busy_timeout",
            i64::try_from(BUSY_TIMEOUT.as_millis()).unwrap_or(i64::MAX),
        )
        .map_err(|e| format!("setting the busy timeout: {e}"))?;
        set_wal(&conn)?;
        // Also per connection, and also off by default. Without it, the
        // implicit DELETE that `INSERT OR REPLACE` performs to resolve a
        // conflict does not fire DELETE triggers — so a REPLACE on
        // `series_title`, whose primary key is the composite
        // `(series_id, title)` rather than a rowid alias, leaves a phantom row
        // in `series_fts` pointing at the discarded rowid. The search index
        // then returns two hits for a title that exists once. `work` is immune
        // because its `id` IS a rowid alias and REPLACE overwrites in place,
        // which is exactly the kind of asymmetry that goes unnoticed.
        //
        // Turned on rather than documented as a rule for callers: a schema that
        // is only correct while every future writer avoids one SQL statement is
        // not correct, it is lucky. Recursion depth here is one — the FTS
        // triggers write through FTS5's command-row idiom, which fires no
        // further user triggers.
        conn.pragma_update(None, "recursive_triggers", "ON")
            .map_err(|e| format!("enabling recursive triggers: {e}"))?;
        // **And the timeout only reaches the writes it is for once the
        // transaction takes its lock at `BEGIN`.** Every write in this crate
        // opens with `unchecked_transaction`, whose default is `BEGIN
        // DEFERRED`: the transaction starts as a *reader* at its first SELECT
        // and asks to become a writer several statements later. SQLite runs no
        // busy handler for that promotion — two connections both holding read
        // transactions and both waiting to promote would deadlock, so it
        // returns SQLITE_BUSY at once — and under WAL, a snapshot the other
        // writer has moved past returns SQLITE_BUSY_SNAPSHOT, which waiting
        // cannot resolve at all. So a busy timeout on its own leaves exactly
        // the reproduced failure in place.
        //
        // `Immediate` for every transaction on this connection rather than at
        // each of the seven call sites (`state.rs` three times, one each in
        // `import.rs`, `librarydoc.rs`, `scan.rs` and `rules.rs`) a future
        // contributor might add a write to, and for the same reason
        // `recursive_triggers` is a pragma rather than a rule for callers:
        // all seven are writes, and a store that is only correct while every
        // future writer remembers to pass a behaviour is not correct, it is
        // lucky. `schema.rs`'s migration transaction, two lines below, is an
        // eighth `unchecked_transaction` and gets this same `Immediate`
        // behaviour too — it opens after this line, on this same connection
        // — but it sits outside the count above because it is not a call
        // site future code could add or forget: it runs once, here, inside
        // `from_connection`, before any caller has a `Connection` to write
        // through at all. The concurrent writers `schema.rs:159-161`'s own
        // comment depends on `Immediate` for are real and measured, not
        // hypothetical — `status.md`'s record of review 3's I1: 14 failures
        // in 48 concurrent first opens, each its own process racing this
        // exact migration, none in 40 runs after this connection-wide
        // setting closed it.
        // The cost is that a read-only `unchecked_transaction` would also take
        // the write lock; there is none, and one would be the wrong shape here
        // anyway — `Library` hands out one connection behind a `Mutex`.
        conn.set_transaction_behavior(rusqlite::TransactionBehavior::Immediate);
        require_fts5(&conn)?;
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// The connection, exclusively.
    ///
    /// A poisoned lock is taken rather than propagated, the same call
    /// `Work::sidecar` makes and for the same reason: the database behind it
    /// is whatever the last successful write left, and refusing every later
    /// request because one handler panicked is the worse answer.
    ///
    /// No `#[must_use]` here: `MutexGuard` already carries one, and stacking
    /// a second with no message is `clippy::double_must_use` — see
    /// `taiyaku-serve`'s `Work::notes_guard`, the same shape, without it.
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        match self.conn.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Refuse a SQLite that cannot do FTS5, before any schema is applied.
///
/// **This crate links the system `libsqlite3` rather than bundling one** — a
/// measured choice, 3.06 s against 1 m 43 s to build, with CI having no cargo
/// cache and a ten-minute job budget. The cost is that FTS5 stops being a
/// guarantee of the dependency and becomes a property of the host. Debian and
/// Arch both build it in; that is two distributions checked, not a promise
/// about every host this will ever run on.
///
/// Without this, the failure arrives as `no such module: fts5` from the middle
/// of a migration, on a database that is half-created and half-not. Probing
/// with a temporary virtual table is definitive where reading
/// `PRAGMA compile_options` is not: the pragma reports how the library was
/// configured, and this reports whether the statement actually works.
///
/// A thin wrapper over [`require_fts5_with_probe`], fixing the probe to the
/// real FTS5 module — see that function for why the probe is a parameter at
/// all rather than inlined here.
fn require_fts5(conn: &Connection) -> Result<(), String> {
    require_fts5_with_probe(
        conn,
        "CREATE VIRTUAL TABLE temp.taiyaku_fts5_probe USING fts5(probe)",
    )
}

/// `require_fts5`'s refusal and message, run against a caller-supplied
/// `CREATE VIRTUAL TABLE temp.taiyaku_fts5_probe USING ...` statement rather
/// than a hardcoded one.
///
/// **Split out for exactly one reason: this host has FTS5**, so `require_fts5`
/// itself can never be driven down its refusal branch by any test running
/// here — CI's `rust:1-bookworm` and every workstation this has been built on
/// both have it. A test that cannot reach a branch cannot pin what that branch
/// does; see `tests.rs`'s `opening_refuses_a_sqlite_without_fts5_and_names_what_is_missing`,
/// which used to assert only that the host has FTS5 (true whether or not
/// `require_fts5` was ever called) rather than anything about the refusal.
/// Parameterising the probe statement lets a test hand in one that fails the
/// same way a real FTS5-less SQLite fails — `no such module` — without
/// needing such a host to exist. The caller's statement must still name the
/// table `temp.taiyaku_fts5_probe`, matching the `DROP` below, since only the
/// `USING` clause is the point of variation.
fn require_fts5_with_probe(conn: &Connection, create_probe_sql: &str) -> Result<(), String> {
    conn.execute_batch(create_probe_sql).map_err(|e| {
        format!(
            "this SQLite has no FTS5, which the library's search needs: {e}. \
             taiyaku links the system libsqlite3 rather than bundling one, so \
             install a build with FTS5 enabled (Debian's libsqlite3-0 and Arch's \
             sqlite both have it)"
        )
    })?;
    conn.execute_batch("DROP TABLE temp.taiyaku_fts5_probe")
        .map_err(|e| format!("dropping the FTS5 probe table: {e}"))
}

#[cfg(test)]
mod corpus;
#[cfg(test)]
mod tests;
