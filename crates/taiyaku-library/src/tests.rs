use super::*;

/// A directory unique to one test, removed when the test ends — the same
/// shape, and for the same reason, as `export/tests.rs`'s own: this
/// workspace's dependency list is short enough to quote in `docs/status.md`,
/// and a `tempfile` dependency would earn its place for about fifteen lines.
///
/// `pub(crate)` for `schema/tests.rs`, which needs two connections onto one
/// real file to reproduce two processes migrating together, and would
/// otherwise carry a fourth copy of these fifteen lines.
pub(crate) struct TempDir {
    pub(crate) path: std::path::PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn tempdir(label: &str) -> TempDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "taiyaku-test-lib-{label}-{}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("creating the test directory");
    TempDir { path }
}

/// **Two processes are expected to hold this file at once, and the second one
/// used to fail on the spot with "database is locked" rather than waiting.**
///
/// `taiyaku serve`, `taiyaku library import` and `taiyaku library export` all
/// default to the same `$XDG_DATA_HOME/taiyaku/library.db`, and `serve` writes
/// a `progress` row on every page turn. `Library`'s own doc argues one
/// connection behind a `Mutex` is enough — a correct argument about
/// contention *inside* one process that says nothing about the two-process
/// case the CLI plainly enables. Reproduced by review as
/// `taiyaku: updating work 2: database is locked`, exit 1.
///
/// **A busy timeout alone does not fix it**, which is the part worth writing
/// down. Every write in this crate opens its transaction with
/// `unchecked_transaction`, and that was `BEGIN DEFERRED`: the transaction
/// starts as a *reader* at its first `SELECT` — `import::sidecar`'s
/// `find_by_key`, here — and only becomes a writer several statements later.
/// SQLite does not run the busy handler for that promotion. It cannot: two
/// connections holding read transactions and both waiting to promote would
/// deadlock, so it returns `SQLITE_BUSY` immediately instead, and under WAL a
/// snapshot the other writer has already moved past returns
/// `SQLITE_BUSY_SNAPSHOT`, which no amount of waiting resolves. So the timeout
/// applies to the case the library never used.
///
/// Taking the write lock at `BEGIN` is what makes the wait possible, and that
/// is the connection-level `TransactionBehavior::Immediate` in
/// `from_connection`. This drives the real import path — the one review
/// reproduced — against a second library holding the write lock for 250 ms.
#[test]
fn an_import_waits_for_another_writer_rather_than_failing_on_the_lock() {
    let dir = tempdir("busy");
    let db = dir.path.join("library.db");
    let holder = Library::open(&db).expect("the first library opens");
    let waiter = Library::open(&db).expect("a second library opens on the same file");
    let doc = taiyaku_core::Sidecar::from_json(
        r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
            "editions":{"raw":{"label":"R","members":[
              {"path":"raw/v01.cbz","sha256":"3f9a","pages":10}]}},
            "hub":"raw","alignments":[]}"#,
    )
    .expect("the fixture parses");

    std::thread::scope(|scope| {
        let (locked, is_locked) = std::sync::mpsc::channel();
        scope.spawn(move || {
            let held = holder.conn();
            held.execute_batch("BEGIN IMMEDIATE")
                .expect("the first writer takes the write lock");
            // A real write, so the waiter's read snapshot is genuinely behind
            // the moment this commits — the `SQLITE_BUSY_SNAPSHOT` half.
            held.execute("INSERT INTO tag (name) VALUES ('held')", [])
                .expect("the first writer writes");
            locked.send(()).expect("announcing the lock");
            std::thread::sleep(std::time::Duration::from_millis(250));
            held.execute_batch("COMMIT")
                .expect("the first writer releases the lock");
        });
        is_locked.recv().expect("the write lock is held");

        let conn = waiter.conn();
        crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
            .expect("an import must wait for the lock rather than fail on it");
    });

    let conn = waiter.conn();
    let title: String = conn
        .query_row("SELECT title FROM work", [], |row| row.get(0))
        .expect("the work the waiting import wrote");
    assert_eq!(title, "T", "the waiting import must actually have landed");
}

/// The wait is **bounded**, and by a value this crate states rather than one
/// it inherits.
///
/// Read back rather than assumed, because the effect above holds for any
/// timeout over 250 ms — including one that would hang a page turn for an
/// hour. Five seconds: longer than any single write this store performs (the
/// largest is one work's import, measured in milliseconds), short enough that
/// a genuinely stuck writer surfaces as an error a person can act on instead
/// of a server that appears to have hung.
///
/// **Stated plainly, since it is the kind of thing that looks like a guard and
/// is not**: `rusqlite` happens to set the same 5 000 ms on every connection
/// it opens (`inner_connection.rs`), so deleting the pragma below leaves this
/// green today. What it pins is the *value*, against a dependency that has
/// never promised it and against anyone setting a different one here. The
/// property that actually needed a failing test is the transaction behaviour,
/// which is the test above.
#[test]
fn every_connection_carries_the_busy_timeout() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let millis: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        millis, 5_000,
        "a busy writer waits five seconds before giving up"
    );
}

/// **Whole-branch review 3, I1, the other half of the first open.** Switching
/// a database into WAL takes a brief exclusive lock, and SQLite runs no busy
/// handler for it: while any other connection holds the file, the pragma
/// returns `SQLITE_BUSY` on the spot, however long the busy timeout is. So the
/// first open of a *fresh* library — the one open every install performs, with
/// `serve` and `library import` defaulting to the same file — failed outright
/// with `opening the library: setting WAL: database is locked`, one of the two
/// forms behind 14 failures in 48 concurrent first opens.
///
/// It is only ever the first: once the file is in WAL the pragma is a no-op
/// that takes no lock, which is why this test has to hold a lock on a database
/// that is *not* yet in WAL, exactly as a first open finds one.
///
/// Reordering the timeout ahead of the pragma — done, because that is where
/// the decision belongs — does not fix this. Waiting does.
#[test]
fn a_first_open_waits_for_the_file_rather_than_failing_to_set_wal() {
    let dir = tempdir("wal-busy");
    let db = dir.path.join("library.db");
    let holder = rusqlite::Connection::open(&db).expect("a plain connection opens the file");
    holder
        .execute_batch("CREATE TABLE seed (x)")
        .expect("a real file, in the rollback-journal mode a fresh database starts in");
    let mode: String = holder
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_ne!(
        mode.to_lowercase(),
        "wal",
        "the fixture must not already be in WAL, or this test proves nothing"
    );

    std::thread::scope(|scope| {
        let (locked, is_locked) = std::sync::mpsc::channel();
        scope.spawn(move || {
            holder
                .execute_batch("BEGIN IMMEDIATE; INSERT INTO seed VALUES (1);")
                .expect("the other process takes the write lock");
            locked.send(()).expect("announcing the lock");
            std::thread::sleep(std::time::Duration::from_millis(250));
            holder
                .execute_batch("COMMIT")
                .expect("the other process releases the lock");
        });
        is_locked.recv().expect("the write lock is held");

        Library::open(&db).expect("a first open must wait for the file rather than fail on it");
    });

    let library = Library::open(&db).expect("and the library is openable afterwards");
    let conn = library.conn();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        mode.to_lowercase(),
        "wal",
        "the wait ended in WAL, not in giving up quietly"
    );
}

/// **The whole of I1 as it was reproduced**: several processes opening one
/// fresh library at the same moment, which is what `serve` and `taiyaku
/// library import` sharing `$XDG_DATA_HOME/taiyaku/library.db` do on the very
/// first run. Review 3 measured 14 failures across 48 such opens, in the two
/// forms the two tests around this one pin one at a time — `setting WAL:
/// database is locked` and `applying schema version 1: table series already
/// exists`. Neither is reachable on an existing library; it is only ever the
/// first open, which is the one every install performs.
///
/// Threads rather than processes: each carries its own `Connection`, which is
/// the unit SQLite locks against, and a test that spawns four copies of the
/// binary would be measuring the harness.
#[test]
fn several_first_opens_of_one_fresh_library_at_once_all_succeed() {
    let dir = tempdir("first-open-race");
    let db = dir.path.join("library.db");

    std::thread::scope(|scope| {
        let opens: Vec<_> = (0..4)
            .map(|n| {
                let db = db.clone();
                scope.spawn(move || Library::open(&db).map(|_| n))
            })
            .collect();
        for open in opens {
            match open.join().expect("no thread panicked") {
                Ok(_) => {}
                Err(e) => panic!("every concurrent first open must succeed: {e}"),
            }
        }
    });

    let library = Library::open(&db).expect("and the file is a working library afterwards");
    let conn = library.conn();
    let version: u32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("the version row")
        .parse()
        .expect("a version number");
    assert_eq!(
        version,
        crate::schema::SCHEMA_VERSION,
        "migrated exactly once, by whichever open got there first"
    );
}

/// The wait is bounded, and what it says when it runs out is the point.
///
/// The reproduced message — `setting WAL: database is locked` — named a
/// condition and offered nothing, on a database that is fine. `set_wal_within`
/// is split out from `set_wal` for the same reason `require_fts5_with_probe`
/// is split from `require_fts5`: the giving-up branch is unreachable from a
/// test that is not willing to hold a lock for the full five seconds.
#[test]
fn giving_up_on_wal_says_how_long_it_waited_and_that_nothing_was_lost() {
    let dir = tempdir("wal-gives-up");
    let db = dir.path.join("library.db");
    let holder = rusqlite::Connection::open(&db).expect("a plain connection opens the file");
    holder
        .execute_batch("CREATE TABLE seed (x); BEGIN IMMEDIATE; INSERT INTO seed VALUES (1);")
        .expect("the other process takes the write lock and keeps it");

    let waiter = rusqlite::Connection::open(&db).expect("a second connection opens");
    let refused = crate::set_wal_within(&waiter, std::time::Duration::from_millis(50))
        .expect_err("a lock nobody releases must end in an error, not a hang");
    assert!(
        refused.starts_with("setting WAL: "),
        "the message keeps the step it failed at: {refused}"
    );
    assert!(
        refused.contains("nothing has been written"),
        "and says the library is not damaged, which is what the bare SQLite \
         message left a reader to guess: {refused}"
    );
}

/// `PRAGMA foreign_keys` is a property of the **connection**, not of the
/// database, and SQLite leaves it off by default for backward compatibility.
/// A `Library::open` that forgets it turns every `REFERENCES` in the schema
/// into a comment — silently, with every constraint appearing to be there.
#[test]
fn every_connection_enforces_foreign_keys() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let on: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        on, 1,
        "foreign key enforcement must be on for this connection"
    );
}

/// FTS5 is a property of the host's SQLite build, not of the crate, because
/// this links the system library rather than bundling one. Debian and Arch both
/// build it in; "both distros I checked" is not "every host". Without this
/// check the failure surfaces as Task 3's migration dying part-way with
/// `no such module: fts5`, on a half-created schema.
///
/// This host has FTS5 — CI's rust:1-bookworm and this workstation both do —
/// so there is no way to reach `require_fts5`'s refusal through the real
/// probe statement. What used to sit here instead ran its *own* ad hoc probe
/// against `Library::open_in_memory()`'s connection and asserted that FTS5
/// worked — which is true regardless of whether `require_fts5` ever ran,
/// since the host has it either way, so deleting the call to `require_fts5`
/// out of `from_connection` left that test green. Neither half of the name
/// — "refuses", "names what is missing" — was exercised.
///
/// This calls `require_fts5` itself, through `require_fts5_with_probe`,
/// handing it a probe statement engineered to fail the exact way a real
/// FTS5-less SQLite fails ("no such module") without needing a host that
/// actually lacks FTS5. It runs the same `execute_batch`,
/// same `map_err`, same message-building the real call site
/// (`from_connection`, `lib.rs`) uses — only the SQL differs — so gutting
/// `require_fts5_with_probe`'s refusal (for instance, ignoring the `Err` and
/// returning `Ok(())` regardless) fails this test — checked by that exact
/// sabotage.
///
/// This pins the refusal and its message. That `from_connection` *calls*
/// `require_fts5` at all is a separate claim, pinned separately by
/// `from_connection_runs_the_fts5_probe_before_applying_the_schema` below.
#[test]
fn opening_refuses_a_sqlite_without_fts5_and_names_what_is_missing() {
    let library = Library::open_in_memory().expect("this host has FTS5");
    let conn = library.conn();
    let err = require_fts5_with_probe(
        &conn,
        "CREATE VIRTUAL TABLE temp.taiyaku_fts5_probe USING fts5_module_that_does_not_exist(probe)",
    )
    .expect_err("a probe naming a nonexistent module must fail exactly as a missing FTS5 would");
    assert!(
        err.contains("FTS5"),
        "the message must name what is missing: {err}"
    );
    assert!(
        err.contains("libsqlite3") || err.to_lowercase().contains("install"),
        "the message must suggest a fix: {err}"
    );
}

/// The guard above is only worth anything if `from_connection` actually runs
/// it, and that is the half that had never been pinned: on a host with FTS5,
/// deleting `require_fts5(&conn)?;` changes nothing `open_in_memory` returns,
/// so the probe can be removed entirely and every other test stays green.
///
/// It can be pinned without a host that lacks FTS5, and without widening the
/// public constructors. `from_connection` is private but reachable from this
/// module, and the probe names a specific table — `temp.taiyaku_fts5_probe`.
/// Occupying that name before opening makes the probe's `CREATE VIRTUAL
/// TABLE` fail on a host that has FTS5, which is exactly the branch that
/// otherwise cannot be reached.
///
/// So the assertion is about **sequencing, not about FTS5**: the schema
/// migration never touches `temp`, so with the probe wired the open fails
/// with the FTS5 refusal, and with the probe deleted the migration runs to
/// completion and the open succeeds. Verified by that exact deletion.
///
/// Asserting the refusal's own wording rather than merely that opening failed
/// is what keeps it honest — a migration that failed for some unrelated
/// reason would also be an `Err`, and would pass a test that only checked for
/// one.
#[test]
fn from_connection_runs_the_fts5_probe_before_applying_the_schema() {
    let conn = Connection::open_in_memory().expect("an in-memory connection opens");
    conn.execute_batch("CREATE TABLE temp.taiyaku_fts5_probe (occupied)")
        .expect("the probe's table name is free until the probe takes it");

    let err = Library::from_connection(conn)
        .expect_err("the probe cannot create its table, so opening must refuse");

    assert!(
        err.contains("which the library's search needs"),
        "opening must fail through require_fts5's refusal, not somewhere later: {err}"
    );
}

/// `INSERT OR REPLACE` on `series_title` must not leave a phantom row in the
/// search index.
///
/// Its primary key is the composite `(series_id, title)`, not a rowid alias, so
/// REPLACE resolves a conflict by deleting and re-inserting with a **fresh
/// rowid** — and that implicit delete fires no DELETE trigger unless
/// `recursive_triggers` is on. Without the pragma this returns 2 for a title
/// that exists once. `work` is immune, because its `id` IS a rowid alias and
/// REPLACE overwrites in place; that asymmetry is why this is asserted rather
/// than assumed.
#[test]
fn replacing_a_series_title_leaves_no_phantom_in_the_search_index() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO series_title (series_id, title) VALUES (1, 'Frieren');
         INSERT OR REPLACE INTO series_title (series_id, title, kind)
             VALUES (1, 'Frieren', 'romaji');",
    )
    .expect("insert then replace");

    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM series_fts WHERE series_fts MATCH 'Frieren'",
            [],
            |row| row.get(0),
        )
        .expect("fts query");
    assert_eq!(hits, 1, "a title that exists once must match once");
}

#[test]
fn a_fresh_database_records_its_schema_version() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let version: u32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| {
                let text: String = row.get(0)?;
                Ok(text.parse().unwrap_or(0))
            },
        )
        .expect("meta carries a schema version");
    assert_eq!(version, crate::schema::SCHEMA_VERSION);
}
