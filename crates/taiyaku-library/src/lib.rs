//! The library store: many works, their editions and members, and the
//! per-person state that hangs off them.
//!
//! **A store and a projection, and deliberately nothing else.** Every rule
//! about what a re-solve does to a hand correction lives in `taiyaku-core`,
//! where it is already tested; this crate reaches it through
//! `Sidecar::merge_spoke` (see `import::reimport`) rather than reimplementing
//! it over rows. See `docs/superpowers/specs/2026-08-14-library-store-design.md`,
//! §4.2.

pub mod account;
pub mod backup;
pub mod book_pins;
pub mod browse;
pub mod comicinfo;
pub mod credential;
pub mod derive;
pub mod dictionary;
pub mod export;
// **Private, and that is the point.** `fold::absorb` deletes a `work` row, and
// `derive` is the only thing that has any business calling it —
// `derive::superseded` is what decides the pair. Narrowing the door is a
// mitigation rather than a guarantee, so `absorb` guards its own
// invariants regardless.
mod fold;
pub mod hand;
pub mod identity;
pub mod import;
pub mod lease;
pub mod libraries;
pub mod librarydoc;
pub mod migrate;
/// What a proposed rule list would do to the whole library, without doing it.
/// See `docs/superpowers/specs/2026-09-02-library-configuration-build-design.md`,
/// §2.
pub mod preview;
pub mod progressdoc;
pub mod project;
pub mod proposals;
pub mod reach;
pub mod relocate;
pub mod rules;
pub mod scan;
pub mod schedule;
pub mod schema;
pub mod stamp;
pub mod state;
pub mod structure;
pub mod watch;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Why a library or an account is not given a name [`has_no_address`] answers
/// true for, in the words the Configuration pages draw.
pub(crate) const NO_ADDRESS: &str = "An address cannot hold “.” or “..” as a name.";

/// Whether `name` is `.` or `..`, exactly.
///
/// **No page address can hold either.** A library's page and an account's are
/// at their name (`web/src/configAddress.ts`), and the router and the URL
/// parser collapse a `.` or `..` segment, `%2E` too, so a thing with either
/// name would have no page. A name holding dots beside anything else, such as
/// `...` or `a.b`, is an ordinary segment. Every write of a name a caller gave
/// to `library.name` or `user.name` asks this first.
///
/// **A library named for its directory is asked of the name it would be
/// given**, by `scan::add_root_checked_in` before the insert: the name is the
/// directory's last component stripped of white space, so a directory called
/// `. ` would make a library called `.`. The unchecked `scan::add_root` and
/// `scan::add_root_in` refuse it too, as they make the library through
/// `libraries::create`, which asks this of every name. A row that already
/// holds either name is left as it is.
pub(crate) fn has_no_address(name: &str) -> bool {
    matches!(name, "." | "..")
}

// Per-person state lives in this crate because it cannot depend on
// `taiyaku-serve` — the dependency points the other way. See
// `docs/superpowers/specs/2026-08-14-library-store-design.md`, §3.4 and §4.3.
//
// `TruthDoc`, `TruthEdition` and `TruthRegion` are not defined here: they
// live in `taiyaku-core`, which this crate depends on.

/// How far through a work a reader got, and in which edition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub edition: String,
    pub slot: u32,
}

/// One recorded position, as a reader of the store sees it.
///
/// **Separate from [`Progress`], which is the write shape.** `Progress` is the
/// body of `PUT /api/works/{work}/progress`, where `updated` is the store's to
/// set: putting it on `Progress` would ask a client to invent a timestamp the
/// server then ignores. Beside `at`, which only serve's read fills, the two
/// are one field apart and that field is the whole distinction.
///
/// **A sibling `taiyaku.progress.json` is the one place a time arrives from
/// outside the store**, and it arrives as this crate's own export of these
/// rows. `progressdoc::Recorded` carries it and `state::put_progress_at`
/// writes it, because a reimport that restamped every position with
/// `datetime('now')` would leave them all in one second and let alphabet
/// decide which edition a reader resumes into.
///
/// **The format is SQLite's `datetime('now')` — `"2026-08-27 16:08:33"`, no
/// `T` and no zone — where `Note`'s `created`/`updated` on the same API are
/// RFC 3339.** The clocks differ, which is why the formats do: a note's times
/// are minted by the client that wrote it, a position's by the server. Making
/// the two one format is a data migration rather than an edit. Every
/// comparison of this column is lexical — `state::progress`'s `ORDER BY`, and
/// `fold::move_positions`' `excluded.updated > progress.updated` — and
/// `'2026-08-27 23:59:59' > '2026-08-27T00:00:00Z'` is false, so a build that
/// began writing one format into a column already holding the other would
/// sort every same-day row by which format it was written in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub edition: String,
    pub slot: u32,
    pub updated: String,
    /// The page the row names, as stored, whether or not it is found: a wire
    /// field like [`Note::stale`], filled by `state::progress_anchored_in` for
    /// serve alone (O240). Every other read leaves it out, so an export
    /// carries none, and an import ignores it: a write derives the page from
    /// the slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<taiyaku_core::MemberPage>,
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

/// A run of a book block's text, from `start` up to `end`, in UTF-16 code
/// units as a browser counts a string, since a browser is what draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
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
    /// The run of a book block's text this note marks, a highlight; absent for
    /// a note on the whole block, and for every note on a page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextRange>,
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
    /// The page and half this note was written on are not in its edition's
    /// layout now, so `slot` is where it was stored and may show another page
    /// (O244).
    ///
    /// Set on a read's answer, never stored: the row keeps its `at_` columns,
    /// so the note reads unmarked again if its page comes back. False when
    /// absent and left out when false, so older documents read and write
    /// unchanged. A field because `Note` refuses unknown keys and a reader
    /// echoes what it read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
    /// The page the row names, as stored, whether or not it is found: a wire
    /// field like `stale`, filled by `state::notes_anchored_in` for serve
    /// alone, so a reader echoes the page it read (O240). Every other read
    /// leaves it out, so an export carries none, and an import ignores it: a
    /// write derives the page from the slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<taiyaku_core::MemberPage>,
}

impl Note {
    /// Whether two notes are anchored to exactly the same place: the same
    /// edition, slot, region and run of text.
    ///
    /// Not whether they are equal: a reader retyping a note's words leaves the
    /// anchor alone, and re-checking an anchor that has not moved is what
    /// makes the notes `PUT` refuse documents for a note nobody touched.
    #[must_use]
    pub fn same_anchor(&self, other: &Note) -> bool {
        self.edition == other.edition
            && self.slot == other.slot
            && self.region == other.region
            && self.text == other.text
    }
}

/// The notes document's format version this build writes and reads.
pub const NOTES_VERSION: u32 = 1;

fn notes_version() -> u32 {
    NOTES_VERSION
}

/// Every note on one work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
    /// The format version the document declares: absent reads as 1, the shape
    /// every notes document had before it was named, and it is always written.
    ///
    /// Only a newer version is refused ([`Notes::refuse_newer`]); whoever
    /// bumps [`NOTES_VERSION`] owns what reading a 1 means (`docs/sidecar.md`,
    /// "Forward compatibility").
    #[serde(default = "notes_version")]
    pub taiyaku_notes: u32,
    pub work: String,
    pub notes: Vec<Note>,
}

impl Notes {
    /// Refuse the notes document `text` when it declares a newer
    /// `taiyaku_notes` than this build reads, in
    /// [`taiyaku_core::refuse_newer`]'s words: the notes `PUT` and
    /// `library import` would otherwise store a newer document whose keys this
    /// build knows as though it were version 1. Asked of the raw JSON before it
    /// is parsed as [`Notes`], which refuses unknown keys, so a newer document
    /// that adds one is refused as newer; text that declares no version is left
    /// to the parse.
    ///
    /// # Errors
    ///
    /// Returns that refusal, naming both versions.
    pub fn refuse_newer(text: &[u8]) -> Result<(), String> {
        let Some(declared) = crate::migrate::declared_in(text, "taiyaku_notes") else {
            return Ok(());
        };
        // Past `u32` is newer than any build, so it is named at the largest.
        let declared = u32::try_from(declared).unwrap_or(u32::MAX);
        taiyaku_core::refuse_newer("taiyaku_notes", declared, NOTES_VERSION)
    }

    /// Every note without [`Note::stale`] and [`Note::at`], the two fields a
    /// read adds and a write never takes, so a document is compared or written
    /// out without them: a note a reader echoes would otherwise differ from
    /// the one held by them alone, and an exported file would carry what the
    /// store does not take back from it (O240, O244).
    pub fn clear_wire_fields(&mut self) {
        for note in &mut self.notes {
            note.stale = false;
            note.at = None;
        }
    }
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

/// Run `write` once on `conn`, refused at once rather than after the busy
/// timeout where another connection holds the store's write lock (O272):
/// `serve` has one shared connection, and a write a request did not ask for
/// must not hold every other request behind a derive's lock.
///
/// `None`, with `write` not run, where the timeout cannot be lowered. The
/// timeout goes back to `BUSY_TIMEOUT`, which every connection this crate
/// hands out carries, however `write` ends.
pub fn without_waiting<T>(conn: &Connection, write: impl FnOnce(&Connection) -> T) -> Option<T> {
    conn.busy_timeout(Duration::ZERO).ok()?;
    let _restore = RestoresTheWait(conn);
    Some(write(conn))
}

/// Gives a connection back [`BUSY_TIMEOUT`] when dropped, for
/// [`without_waiting`].
struct RestoresTheWait<'a>(&'a Connection);

impl Drop for RestoresTheWait<'_> {
    fn drop(&mut self) {
        let _ = self.0.busy_timeout(BUSY_TIMEOUT);
    }
}

/// `journal_mode = WAL`, retried while another connection is in the way.
///
/// **The busy timeout does not cover this one statement**, which is why it is
/// the only pragma here with a loop around it. Switching journal mode takes a
/// brief exclusive lock on the database, and SQLite runs no busy handler for
/// it: if any other connection holds the file, the pragma returns
/// `SQLITE_BUSY` on the spot.
///
/// It only ever bites on the **first** open of a fresh library: once the file
/// is in WAL, this pragma is a no-op that takes no lock at all. That is also
/// the one open every install performs, and `serve` plus `library import`
/// starting together is ordinary, so without the retry a first open that
/// collides would fail here, reported as
/// `opening the library: setting WAL: database is locked`.
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
/// One shared connection behind a `Mutex`; a save or a long job opens its own.
/// The write volume is a reading position and the occasional note; a pool would
/// add a dependency and a failure mode to solve a contention problem nobody has.
///
/// **That argument is about one process.** Contention *between* processes is
/// real and expected — `serve` and `library import` share one default
/// database — and is handled where it has to be, on the connection: see the
/// busy timeout and transaction behaviour in [`Library::configure`].
#[derive(Debug)]
pub struct Library {
    conn: Mutex<Connection>,
    /// Where this library was opened from, or `None` for `:memory:`.
    ///
    /// Held so [`Self::working`] can open a *second* connection to the same
    /// file rather than handing a long job the mutex every request needs. An
    /// in-memory database has no such file — a second connection to
    /// `:memory:` is a different, empty database — which is why [`Working`]
    /// has two shapes rather than this being an `Option<Connection>` that
    /// every caller of `working` would have to branch on for itself.
    path: Option<PathBuf>,
    /// Set while one of this library's own derive leases lives, so that
    /// [`Self::derive_running`] answers for its own process without a probe,
    /// and is the whole lease of an in-memory library.
    deriving: AtomicBool,
    /// Per library, not per process, so that two stores open in one process
    /// never order each other's saves.
    progress_order: state::ProgressOrder,
}

impl Library {
    /// Open the library at `path`, creating and migrating it if needed.
    ///
    /// The stored path is canonicalised — resolved to an absolute path with
    /// symlinks and `.`/`..` collapsed — rather than kept as given.
    /// [`Self::working`]'s whole guarantee is a *second* connection to the
    /// *same* file; a relative path stored verbatim would instead follow
    /// whatever the process's current directory happens to be when
    /// `working` is later called, which is not necessarily the directory it
    /// was in when `open` ran. Nothing outside the tests in this repository
    /// calls `std::env::set_current_dir`, so this is dormant — but
    /// `taiyaku-cli`'s `--db` flag reaches `open` with no absolute-path
    /// check the way its own `$XDG_DATA_HOME`/`$HOME` fallbacks have, so a
    /// relative path is reachable, and canonicalising here is cheaper than
    /// trusting every future caller to check.
    ///
    /// Canonicalising must happen *after* `Connection::open` succeeds, not
    /// before: `Connection::open` creates the file when it is missing, and a
    /// path to a file that does not exist yet cannot be canonicalised — the
    /// call would fail on the exact case (a brand new library) this is
    /// meant to handle.
    ///
    /// If canonicalising still fails once the file exists — a permission
    /// refusal on an ancestor directory, or the file being removed out from
    /// under this call between opening and canonicalising — this falls back
    /// to the path as given rather than turning a successful open into an
    /// error over a problem that is about to matter only if [`Self::working`]
    /// is ever called: `open` alone never opens a second connection, so
    /// there is no correctness reason to refuse here. The fallback is a
    /// `path.to_path_buf()` clone rather than a silent branch precisely so
    /// this comment has somewhere to explain it.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened (a missing parent directory
    /// or a permission refusal, naming the path), if the backup taken before a
    /// migration fails, leaving it unmigrated, or if applying the schema fails.
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        Self::from_connection(conn, Some(canonical))
    }

    /// An in-memory library, for tests.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite refuses the connection or the schema fails
    /// to apply.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("in memory: {e}"))?;
        Self::from_connection(conn, None)
    }

    fn from_connection(mut conn: Connection, path: Option<PathBuf>) -> Result<Self, String> {
        // Read before anything reads through `conn`: that first read makes a
        // `-wal` afresh, dated now, which would count every open as a change.
        let written = path.as_deref().and_then(backup::last_written);
        Self::configure(&mut conn)?;
        if let Some(db) = &path
            && let Some((current, target)) = schema::pending_migration(&conn)?
            && !backup::stands_for(db, current, target, written)
        {
            let name = backup::name_before_migration(&conn, target)?;
            backup::before_migration(&conn, db, &name)?;
        }
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
            deriving: AtomicBool::new(false),
            progress_order: state::ProgressOrder::default(),
        })
    }

    /// Everything a fresh connection needs before any query can safely touch
    /// it, short of applying the schema.
    ///
    /// Shared by [`Self::from_connection`], which follows this with
    /// [`schema::migrate`], and [`Self::working`], which does not: a second
    /// connection to an already-migrated file has nothing to migrate, and
    /// running `schema::migrate` again on a live database is not a risk
    /// worth taking just to keep one function's shape simpler.
    fn configure(conn: &mut Connection) -> Result<(), String> {
        // Per connection, not per database, and off by default. See the test
        // in this module's `tests.rs`.
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("enabling foreign keys: {e}"))?;
        // Every grant names it (`browse::member_under`).
        browse::register_walk_reaches(conn)
            .map_err(|e| format!("adding the grant's pattern test: {e}"))?;
        // **Before the pragma most likely to meet another process**, which is
        // the next one. See `set_wal` for why the timeout does not on its own
        // save that one, and `BUSY_TIMEOUT` for the five seconds.
        conn.pragma_update(
            None,
            "busy_timeout",
            i64::try_from(BUSY_TIMEOUT.as_millis()).unwrap_or(i64::MAX),
        )
        .map_err(|e| format!("setting the busy timeout: {e}"))?;
        set_wal(conn)?;
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
        // cannot resolve at all. So a busy timeout on its own leaves those
        // failures in place.
        //
        // `Immediate` for every transaction on this connection rather than at
        // each call site a future contributor might add a write to, and for
        // the same reason `recursive_triggers` is a pragma rather than a rule
        // for callers: a store that is only correct while every future writer
        // remembers to pass a behaviour is not correct, it is lucky.
        // `schema.rs`'s migration transaction gets this same `Immediate`
        // behaviour too, when this connection is the one
        // [`Self::from_connection`] is building: `schema::migrate` runs after
        // `configure` returns, on this same connection, before any caller has
        // it to write through at all. [`Self::working`]'s second connection
        // never runs a migration at all — the file it opens is already
        // migrated — but still gets `Immediate` here, because it is a writer
        // too and the same promotion deadlock applies to it. The concurrent
        // writers `schema::apply_pending`'s own doc depends on `Immediate` for
        // are real: concurrent first opens are each their own process racing
        // this exact migration. The cost is that a read-only
        // `unchecked_transaction` would also take the write lock; there is
        // none, and one would be the wrong shape here anyway — `Library` hands
        // out its shared connection behind a `Mutex`, and the long jobs on
        // [`Self::working`] are expected to run one at a time, beside readers'
        // saves ([`Self::saving`]), each one short write.
        //
        // **A contract on the caller, not a guarantee this type provides.**
        // Every call to `working` opens an independent connection with no
        // memory of any other, so nothing here stops two long jobs running at
        // once. A rule save's single `BEGIN IMMEDIATE` landing between two of
        // a scan's own per-work transactions would replace the rule table and
        // re-derive the whole library while the scan's already-compiled
        // `Engine` was still mid-pass under the rules that transaction just
        // replaced. `taiyaku-serve`'s `ServeState::library_job` is what makes
        // the premise true: a lock a scan's derive and a rule save's
        // `preview::apply` both take before ever calling `working`, so the two
        // genuinely run one at a time rather than merely being assumed to.
        conn.set_transaction_behavior(rusqlite::TransactionBehavior::Immediate);
        require_fts5(conn)
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

    /// A connection of its own for a reader's save: a position, notes, truth
    /// or a correction (O272). Opened per save as [`Self::working`] opens one,
    /// so each save waits for another connection's write lock on its own, up
    /// to the busy timeout, and holds up no read on [`Self::conn`].
    ///
    /// An in-memory library has one connection, so this is [`Self::conn`]
    /// there, and a caller holding `conn()` must not call it.
    ///
    /// # Errors
    ///
    /// As [`Self::working`].
    pub fn saving(&self) -> Result<Working<'_>, String> {
        self.working()
    }

    /// The order this library's progress saves arrived in, for
    /// [`state::put_progress_in_order`].
    #[must_use]
    pub fn progress_order(&self) -> &state::ProgressOrder {
        &self.progress_order
    }

    /// Where this library lives, or `None` if it is in memory.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// A connection for a job long enough that holding [`Self::conn`] would
    /// stall every request the server serves meanwhile — a rule preview that
    /// derives the whole library and rolls it back, or a scan that walks a
    /// filesystem tree.
    ///
    /// A file-backed library opens a genuine second connection to the same
    /// file, run through [`Self::configure`] but not `schema::migrate` — the
    /// file is already migrated by the time anyone asks for a working
    /// connection, and re-applying the schema is not a no-op worth risking
    /// here. An in-memory library has no file to open a second time — a
    /// second connection to `:memory:` is a different, empty database — so
    /// its "second" connection is the shared one, [`Self::conn`], the only
    /// connection such a library has ever had.
    ///
    /// Under WAL a second connection's write transaction leaves readers on
    /// the first reading the pre-transaction snapshot, and `busy_timeout`
    /// (set in [`Self::configure`]) governs a collision with a real writer.
    ///
    /// # Errors
    ///
    /// If a second connection cannot be opened against the file — the
    /// message names the path — or if opening succeeds but [`Self::configure`]
    /// fails on it, which fails the same way and for the same reasons a first
    /// open of that file would.
    pub fn working(&self) -> Result<Working<'_>, String> {
        match &self.path {
            Some(path) => {
                let mut conn = Connection::open(path).map_err(|e| {
                    format!("opening a second connection to {}: {e}", path.display())
                })?;
                Self::configure(&mut conn)?;
                Ok(Working::Owned(conn))
            }
            None => Ok(Working::Shared(self.conn())),
        }
    }

    /// The derive lease ([`lease`]): every whole-store derive takes it first,
    /// so that two never interleave, in this process or another (O51).
    ///
    /// # Errors
    ///
    /// If the lock file beside the store cannot be opened, written by this
    /// process, configured, initialised or held. `Ok(None)` only under
    /// [`lease::Wait::No`], where another holds the lease.
    pub fn derive_lease(
        &self,
        wait: lease::Wait<'_>,
    ) -> Result<Option<lease::DeriveLease<'_>>, String> {
        match &self.path {
            Some(path) => lease::take_file(path, &self.deriving, wait),
            None => Ok(lease::take_flag(&self.deriving, wait)),
        }
    }

    /// Whether a derive holds the lease now: this library's own, else one of
    /// another connection or process. A reading, stale as soon as it is taken.
    #[must_use]
    pub fn derive_running(&self) -> bool {
        self.deriving.load(std::sync::atomic::Ordering::SeqCst)
            || self.path.as_deref().is_some_and(lease::held_elsewhere)
    }
}

/// A connection to run a long job or a save on: a fresh one where
/// [`Library`] has a file to open a second connection against, the same
/// shared one where it does not.
///
/// **One accessor, two shapes, so no caller of [`Library::working`] carries
/// two code paths.** The alternative was `Library::working` returning
/// `Option<Connection>` and leaving each caller to fall back to
/// `library.conn()` for itself — two behaviours written out at every call
/// site rather than once here, which is the arrangement this codebase keeps
/// finding drifts (see `Working`'s sibling reasoning on `path`, above).
#[derive(Debug)]
pub enum Working<'a> {
    Owned(Connection),
    Shared(MutexGuard<'a, Connection>),
}

impl std::ops::Deref for Working<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            Self::Owned(conn) => conn,
            Self::Shared(held) => held,
        }
    }
}

/// A read transaction on `conn`, so that the statements read under it come
/// from one committed state, never from both sides of a write committing
/// between two of them; `None` where `conn` is already in a transaction,
/// which reads one state already.
///
/// **`BEGIN DEFERRED`, spelled out**: [`Library::configure`] makes every
/// `unchecked_transaction` `IMMEDIATE`, which would take the write lock for a
/// read and hold up every writer meanwhile.
///
/// # Errors
///
/// If SQLite refuses the `BEGIN`.
pub fn one_snapshot(conn: &Connection) -> rusqlite::Result<Option<rusqlite::Transaction<'_>>> {
    if !conn.is_autocommit() {
        return Ok(None);
    }
    rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Deferred).map(Some)
}

/// [`rusqlite::Transaction`]'s own shape — `Deref` to [`Connection`],
/// `commit`, rollback on drop — open exactly as `Connection::
/// unchecked_transaction` already does wherever nothing is open yet, and as
/// a nested `SAVEPOINT` wherever something already is.
///
/// **Why this type exists at all.** A bare `BEGIN` — what
/// `unchecked_transaction` sends — is refused outright the moment SQLite
/// already holds an open transaction; `derive.rs`'s own doc records exactly
/// this, on why [`crate::derive::derive`] holds no transaction of its own.
/// So `import::scanned_inserting` (through `import_with`, called once per
/// work) and [`crate::fold::absorb`] (once per fold), each of which opens one
/// of its own, would be uncomposable with a caller that wraps a whole
/// derivation in a transaction to roll it all back at once —
/// [`crate::preview::preview`]'s entire mechanism. A plain
/// `unchecked_transaction` inside a derive pass would fail on the spot with
/// "cannot start a transaction within a transaction" the moment a preview
/// wrapped it, on any library holding at least one work.
///
/// **Why not always open a `SAVEPOINT`, and skip the branch.** `SAVEPOINT`
/// nests unconditionally, so that alone would fix the failure above — but at
/// the cost of a regression for every caller that reaches this with nothing
/// open yet. A `SAVEPOINT` that starts a transaction implicitly (nothing
/// open) takes SQLite's own `BEGIN DEFERRED` locking — no lock until the first
/// write — where every connection this crate hands out is configured
/// `TransactionBehavior::Immediate` for the opposite reason:
/// [`Library::configure`]'s own comment records a promotion deadlock
/// `busy_timeout` cannot help with, closed by taking the write lock at `BEGIN`
/// rather than several statements later. Reintroducing deferred locking under
/// a name that looks like a drop-in replacement would undo that with nothing
/// to catch it. So the choice is read off what the connection is already
/// doing — [`Connection::is_autocommit`] is false only when something has
/// already opened a transaction this one must nest inside instead of
/// colliding with — a caller wrapping a whole derive, as
/// [`crate::preview::preview`] does, or `derive::write_work`'s own scope for
/// one work, which every import and `librarydoc::apply` inside a derive nests
/// in (O210) — and every other call site keeps the `BEGIN`/lock behaviour
/// of `unchecked_transaction`.
///
/// **The cost `preview` pays for that outer transaction is real.** Because
/// `preview` holds a `BEGIN IMMEDIATE` for a whole library-wide derivation
/// rather than per work, every `Nested` scope opened underneath it takes the
/// `SAVEPOINT` branch, and the write lock the outer `BEGIN IMMEDIATE` holds
/// stays taken for as long as that entire derivation runs: exactly the span
/// `Library::working`'s second connection exists to let a long job occupy
/// without starving every other request. See [`crate::preview::preview`]'s own
/// doc for why that trade is made on purpose.
pub(crate) struct Nested<'a> {
    conn: &'a Connection,
    label: &'static str,
    /// `Some` for the ordinary, unnested case: a real
    /// [`rusqlite::Transaction`], whose own `Drop` already rolls back an
    /// early return exactly as every caller of `unchecked_transaction` relies
    /// on. Taken with `Option::take` in [`Self::commit`] rather than moved out
    /// by a pattern match, which this type's own `Drop` impl below would
    /// refuse to compile.
    ///
    /// **Not what `Drop` below checks.** `commit` takes this to `None` on
    /// *both* branches — the real-transaction case takes it to run the real
    /// `COMMIT`, and it was already `None` on the `SAVEPOINT` case — so
    /// `tx.is_none()` is true at drop time regardless of which branch this
    /// scope took. `savepoint_open`, not this field, is what tells `drop`
    /// whether a `SAVEPOINT` named `label` actually exists to undo.
    tx: Option<rusqlite::Transaction<'a>>,
    /// Set in [`Self::open`]'s `SAVEPOINT` branch and never after: whether
    /// this scope actually opened `SAVEPOINT {label}` and therefore owns one
    /// to release or roll back. `false` for the ordinary, unnested case,
    /// where [`Self::commit`]'s real `COMMIT` and `rusqlite::Transaction`'s
    /// own `Drop` already do the whole job and there is no savepoint of this
    /// name for [`Self::drop`] to touch.
    savepoint_open: bool,
    /// Only meaningful while `savepoint_open` is `true`: `false` until
    /// [`Self::commit`] releases it — checked by [`Self::drop`] so an early
    /// return rolls back only this scope, not the caller's.
    savepoint_committed: bool,
}

impl<'a> Nested<'a> {
    /// Open a nested scope named `label` — a name for the `SAVEPOINT` case's
    /// SQL and for the messages below, not a uniqueness SQLite enforces on
    /// its own. Distinct call sites should still pass distinct labels: two
    /// unrelated scopes sharing one name would make a stray `ROLLBACK TO` or
    /// `RELEASE` from a fixture bug ambiguous about which one it meant.
    ///
    /// # Errors
    ///
    /// If SQLite refuses the `BEGIN` or the `SAVEPOINT`.
    pub(crate) fn open(conn: &'a Connection, label: &'static str) -> rusqlite::Result<Self> {
        if conn.is_autocommit() {
            let tx = conn.unchecked_transaction()?;
            Ok(Self {
                conn,
                label,
                tx: Some(tx),
                savepoint_open: false,
                savepoint_committed: false,
            })
        } else {
            conn.execute_batch(&format!("SAVEPOINT {label}"))?;
            Ok(Self {
                conn,
                label,
                tx: None,
                savepoint_open: true,
                savepoint_committed: false,
            })
        }
    }

    /// Commit this scope: a real `COMMIT` where nothing else was open, a
    /// `RELEASE SAVEPOINT` — folding this scope's writes into the caller's
    /// still-open transaction, not onto disk — where something was.
    ///
    /// # Errors
    ///
    /// If SQLite refuses the commit or the release.
    pub(crate) fn commit(mut self) -> rusqlite::Result<()> {
        if let Some(tx) = self.tx.take() {
            tx.commit()
        } else {
            self.conn
                .execute_batch(&format!("RELEASE SAVEPOINT {}", self.label))?;
            self.savepoint_committed = true;
            Ok(())
        }
    }
}

impl std::ops::Deref for Nested<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl Drop for Nested<'_> {
    /// The real-transaction case needs nothing here: `commit`'s `Some(tx)`
    /// arm hands the `Transaction` to its own `commit`, which runs a real
    /// `COMMIT` and consumes it outright, and an uncommitted `Transaction`
    /// left in `self.tx` at drop time rolls back on its own — through that
    /// value's own `Drop`, which fires on this struct's other fields
    /// immediately after this method returns. Only the `SAVEPOINT` case —
    /// `savepoint_open` true — is this type's own to undo. Checking
    /// `savepoint_open` rather than `self.tx.is_none()` is the point: `commit`
    /// takes `tx` to `None` on *both* branches, so `tx.is_none()` alone cannot
    /// tell a committed real transaction apart from an open savepoint, and
    /// reading it that way would run `ROLLBACK TO SAVEPOINT` against a
    /// connection with no such savepoint after every ordinary commit, each
    /// swallowed by the `let _ =` below and each a wasted parse-and-step.
    ///
    /// Both statements together, best-effort: a `ROLLBACK TO` leaves the
    /// savepoint itself still open (unlike a transaction's plain `ROLLBACK`,
    /// which ends it — [`rusqlite::Savepoint`]'s own doc notes the same
    /// asymmetry), so the `RELEASE` after it is what actually closes this
    /// scope. A `Drop` cannot propagate an error, the same constraint
    /// `rusqlite::Transaction`'s own `Drop` accepts. `savepoint_open` being
    /// accurate means this statement is only ever attempted where a savepoint
    /// of this name genuinely exists, so a failure here is a sign this type's
    /// own bookkeeping broke, not a routine result of an ordinary commit. Kept
    /// as `let _ =` rather than a `debug_assert!` on the result: a `Drop` runs
    /// during unwinding too, and panicking a second time there aborts the
    /// process instead of reporting the first panic — see
    /// `tests.rs`'s `committing_the_real_transaction_branch_sends_no_savepoint_statement`,
    /// which pins this by watching the SQL itself rather than by making a
    /// failure here fatal.
    fn drop(&mut self) {
        if self.savepoint_open && !self.savepoint_committed {
            let _ = self.conn.execute_batch(&format!(
                "ROLLBACK TO SAVEPOINT {0}; RELEASE SAVEPOINT {0}",
                self.label
            ));
        }
    }
}

/// Refuse a SQLite that cannot do FTS5, before any schema is applied.
///
/// **This crate links the system `libsqlite3` rather than bundling one** — a
/// choice made on build time, with CI having no cargo cache and a ten-minute
/// job budget. The cost is that FTS5 stops being a guarantee of the dependency
/// and becomes a property of the host. Debian and Arch both build it in; that
/// is two distributions checked, not a promise about every host this will ever
/// run on.
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
/// does; see `tests.rs`'s
/// `opening_refuses_a_sqlite_without_fts5_and_names_what_is_missing`.
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
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod test_support;
#[cfg(test)]
mod tests;
