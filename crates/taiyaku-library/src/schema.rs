//! Migrations, versioned in `meta`.
//!
//! Applied in order, an older database's each inside its own transaction, a
//! fresh database's all inside one (O269, see [`apply_pending`]), with each
//! version row written after its migration — so a migration that fails leaves
//! the database exactly as that transaction found it: no half-applied schema,
//! the version where it was, and the same file openable again once the
//! migration is corrected.
//!
//! **The version not advancing is not on its own the safe outcome**, which is
//! why the transaction matters. A half-applied migration whose version stayed
//! put is re-run from statement one on the next open, over tables that
//! already exist, and fails there every time. See `tests.rs`'s
//! `a_migration_that_fails_part_way_leaves_no_trace_and_can_be_retried`, and
//! `migrate`'s own refusal message, which names that state as the one it
//! declines to create.
//!
//! **A transaction per migration is not on its own enough either**, because
//! the decision to run one is taken before the transaction opens. Two
//! processes reaching a fresh file together — `serve` and `library import`
//! share one by default — both read version 0 and both decide to apply
//! migration 1, and the second would produce, from a different cause entirely,
//! that same "over tables that already exist" failure. The version is therefore
//! read a second time under the write lock; see [`apply_pending`].

use rusqlite::Connection;

/// The schema version this build expects.
pub const SCHEMA_VERSION: u32 = 21;

/// Every migration, in order. Index 0 takes an empty database to version 1.
///
/// **Append, never edit.** A database already at version 1 is never shown
/// migration 1 again, so a change made there reaches new databases only and
/// the two diverge silently. `002` adds one column to `migrated_document`
/// rather than changing the table `001` creates, for exactly that reason.
const MIGRATIONS: [&str; 21] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
    include_str!("schema/004_auth.sql"),
    include_str!("schema/005_note_id_per_user.sql"),
    include_str!("schema/006_member_root.sql"),
    include_str!("schema/007_backfill_member_root.sql"),
    include_str!("schema/008_trigram_fts.sql"),
    include_str!("schema/009_progress_per_edition.sql"),
    include_str!("schema/010_hand_metadata.sql"),
    include_str!("schema/011_hand_provenance.sql"),
    include_str!("schema/012_root_keeps_documents.sql"),
    include_str!("schema/013_can_stamp.sql"),
    include_str!("schema/014_structure_version.sql"),
    include_str!("schema/015_libraries.sql"),
    include_str!("schema/016_library_names.sql"),
    include_str!("schema/017_every_library.sql"),
    include_str!("schema/018_overlapping_libraries.sql"),
    include_str!("schema/019_library_links.sql"),
    include_str!("schema/020_work_missing_since.sql"),
    include_str!("schema/021_anchors.sql"),
];

/// The version a database reaches is the number of migrations applied to it,
/// so the two figures are one fact written twice and drift is a compile error
/// rather than a wrong refusal message.
const _: () = assert!(MIGRATIONS.len() == SCHEMA_VERSION as usize);

/// Bring `conn` up to `SCHEMA_VERSION`.
///
/// # Errors
///
/// Returns an error if a migration fails to apply, naming the version it was
/// applying, or if the database reports a version this build does not know —
/// which means a newer taiyaku wrote it, and running an older binary against
/// it would corrupt what it does not understand.
pub fn migrate(conn: &Connection) -> Result<(), String> {
    migrate_with(conn, &MIGRATIONS)
}

/// `(current, SCHEMA_VERSION)` when [`migrate`] would change a database that
/// holds something, which the open backs up first (O263). `None` for a fresh
/// one, one at this build's version, and a newer one, which `migrate` refuses
/// before anything is written beside it. A fresh database another open is
/// building still reads as fresh, because [`apply_pending`] commits its whole
/// build at once.
pub(crate) fn pending_migration(conn: &Connection) -> Result<Option<(u32, u32)>, String> {
    ensure_meta(conn)?;
    let current = version(conn)?;
    Ok((0 < current && current < SCHEMA_VERSION).then_some((current, SCHEMA_VERSION)))
}

/// `migrate`'s whole body, over a caller-supplied migration list rather than
/// the built-in one.
///
/// **Split out for the same reason as `require_fts5_with_probe`**: the property
/// worth pinning is what happens when a migration *fails*, and every migration
/// in `MIGRATIONS` has to keep succeeding. A test cannot corrupt a real file
/// without breaking every other test in the crate, so it hands in its own
/// two-statement migration instead — same loop, same transaction, same error
/// strings, only the SQL differs. `known` is the list's length rather than
/// [`SCHEMA_VERSION`] so the "written by a newer taiyaku" refusal stays
/// coherent for whatever list it was given; the two agree for the real call by
/// the `const` assertion above.
fn migrate_with(conn: &Connection, migrations: &[&str]) -> Result<(), String> {
    let known = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    ensure_meta(conn)?;
    let current = version(conn)?;
    if current > known {
        return Err(written_by_a_newer_taiyaku(current, known));
    }
    without_foreign_keys(conn, || apply_pending(conn, migrations, current))
}

/// Foreign keys, off for the duration of the migration run and restored after.
///
/// **This cannot live inside a migration, which is the whole reason it is
/// here.** `PRAGMA foreign_keys` is a no-op within a transaction — SQLite
/// neither errors nor warns, it simply does not apply it — and [`apply_pending`]
/// runs every migration inside one. So a migration that rebuilds a table runs
/// with foreign keys ON however loudly its own SQL asks otherwise, and `DROP
/// TABLE` on a parent performs an implicit DELETE that fires `ON DELETE
/// CASCADE` into its children.
///
/// Migration 4 rebuilds `user`; `progress` and `note` reference it, and they
/// hold every note and every reading position in the library.
/// `schema::tests::migration_4_rebuilds_user_without_taking_the_notes_with_it`
/// fails the moment this wrapper is removed.
///
/// **What disabling them costs is paid back in [`apply_one`]**, which runs
/// `PRAGMA foreign_key_check` after each migration, inside its transaction:
/// nothing enforces the constraints during the run, so the enforcement that
/// would have happened continuously happens once, before its version row.
///
/// The restored value is read rather than assumed `ON`, because `Library::open`
/// sets it and a caller holding a bare `Connection` — which is what this
/// module's own tests use — may not have.
fn without_foreign_keys<T>(
    conn: &Connection,
    body: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let was: bool = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|e| format!("reading the foreign_keys pragma: {e}"))?;
    conn.pragma_update(None, "foreign_keys", "OFF")
        .map_err(|e| format!("disabling foreign keys for the migration: {e}"))?;
    let outcome = body();
    // Restored on both paths. A failed migration that left foreign keys off
    // would hand the caller a `Connection` that silently accepts orphans for
    // the rest of its life — a worse outcome than the migration failure that
    // produced it, and a quieter one.
    conn.pragma_update(None, "foreign_keys", if was { "ON" } else { "OFF" })
        .map_err(|e| format!("restoring the foreign_keys pragma: {e}"))?;
    outcome
}

/// The one table this module needs before it can ask anything. `IF NOT
/// EXISTS` rather than guarded by a version, because the version lives in it.
fn ensure_meta(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    )
    .map_err(|e| format!("creating meta: {e}"))
}

/// The version `meta` records, or 0 for a database nobody has migrated.
///
/// Takes a `&Connection` rather than the `Library`'s own so it can be called
/// with a `Transaction`'s deref inside [`apply_pending`]'s loop — the whole
/// point of the second read, see there.
pub(crate) fn version(conn: &Connection) -> Result<u32, String> {
    match conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        // No row at all: a database nobody has migrated. Genuinely version 0.
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(format!("reading the schema version: {e}")),
        Ok(text) => text.trim().parse().map_err(|_| {
            format!(
                "`meta.schema_version` holds {text:?}, which is not a version number. \
                 Refusing to migrate rather than treating it as a fresh database and \
                 applying migration 1 over tables that already exist."
            )
        }),
    }
}

fn written_by_a_newer_taiyaku(current: u32, known: u32) -> String {
    format!(
        "this database is at schema version {current} and this build knows {known}; \
         it was written by a newer taiyaku"
    )
}

/// Apply every migration after `current`, a fresh database's all at once.
///
/// **`current` is a hint, not the decision.** It was read before any
/// transaction was open, and on a fresh database two processes reach it at the
/// same moment: `serve` and `library import` default to the same file, so
/// starting them together is ordinary. Both read 0, both decide to apply
/// migration 1, the second waits on the write lock, gets it once the first has
/// committed, and would run `CREATE TABLE` over tables that now exist —
/// failing with the exact message this module's own doc names as the
/// unrecoverable state it exists to prevent, and telling a reader their new
/// library is beyond saving when a retry would have opened it.
///
/// A transaction per migration does not close that on its own: the *decision*
/// would be taken before the transaction began. So the version is read a
/// second time, under the write lock `BEGIN IMMEDIATE` took, and that read
/// decides: at 0 every migration runs in this one transaction (O269), so no
/// other open sees a part-built database, takes it for older and backs it up.
fn apply_pending(conn: &Connection, migrations: &[&str], current: u32) -> Result<(), String> {
    let known = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    for (index, sql) in migrations.iter().enumerate() {
        let target = u32::try_from(index).unwrap_or(u32::MAX) + 1;
        // **A fast path, not the decision.** The recheck below, under the
        // write lock, is what makes the skip correct; `version(&tx)` can only
        // be at or ahead of the `current` read before any transaction opened,
        // so anything this line skips that one would skip too. What it buys
        // is that an ordinary open of an up-to-date database takes no write
        // lock at all, rather than opening and rolling back one transaction
        // per already-applied migration on every `Library::open` — which
        // matters because `serve` and `library import` share a file by
        // default. Deleting it changes no answer and no test, deliberately:
        // it is a cost guard, and the correctness is the read under the lock.
        if target <= current {
            continue;
        }
        // One transaction around the whole migration and the version row it
        // ends with. `execute_batch` opens none of its own — every statement
        // in the file autocommits as it runs — so without this a failure at
        // statement *n* would leave statements 1..n on disk with the version
        // still where it started, and the next open would re-run the migration
        // from statement one over tables that already existed. Unrecoverable by
        // hand, and exactly the state the unparseable-version refusal above
        // exists to avoid. `unchecked_transaction` rather than `transaction`
        // because `conn` is shared by reference here, the same call
        // `import::sidecar` and `state::put_truth` make; the guard rolls back
        // on drop, so the `?` below is the rollback.
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("starting the transaction for schema version {target}: {e}"))?;
        // Under the write lock, so this is the authoritative answer rather
        // than the one from before it was taken. A `continue` here drops the
        // guard, which rolls back a transaction that has done nothing.
        let applied = version(&tx)?;
        if applied > known {
            return Err(written_by_a_newer_taiyaku(applied, known));
        }
        if target <= applied {
            continue;
        }
        if applied == 0 {
            for (target, sql) in (1..=known).zip(migrations) {
                apply_one(&tx, target, sql)?;
            }
            return tx
                .commit()
                .map_err(|e| format!("committing schema versions 1 to {known}: {e}"));
        }
        apply_one(&tx, target, sql)?;
        tx.commit()
            .map_err(|e| format!("committing schema version {target}: {e}"))?;
    }
    Ok(())
}

/// One migration, the foreign-key check after it and its version row, inside
/// the caller's transaction, which commits them or rolls them back on drop.
fn apply_one(tx: &Connection, target: u32, sql: &str) -> Result<(), String> {
    tx.execute_batch(sql)
        .map_err(|e| format!("applying schema version {target}: {e}"))?;
    // **`without_foreign_keys` turned them off around this whole run**, so
    // nothing enforced them while the migration ran. The enforcement that
    // would otherwise have happened continuously happens once, here, inside
    // the same transaction — a migration that left an orphan rolls back
    // rather than committing a database that the next `Library::open`, which
    // sets `foreign_keys = ON`, would enforce constraints over and fail on.
    //
    // Inside the transaction and before the version row, so the rollback is
    // the same one every other failure in `apply_pending` takes.
    let orphans: i64 = tx
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("checking foreign keys after schema version {target}: {e}"))?;
    if orphans > 0 {
        return Err(format!(
            "schema version {target} left {orphans} row(s) violating a foreign key; \
             rolling back rather than committing a database that cannot be reopened"
        ));
    }
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [target.to_string()],
    )
    .map(|_| ())
    .map_err(|e| format!("recording schema version {target}: {e}"))
}

#[cfg(test)]
mod tests;
