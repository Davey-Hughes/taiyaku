use super::*;
use rusqlite::OptionalExtension as _;

#[test]
fn migrating_twice_is_a_no_op() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("first migration");
    migrate(&conn).expect("second migration is a no-op");
}

/// A database written by a newer build is refused rather than opened, because
/// an older binary cannot know what it would be dropping.
#[test]
fn a_future_schema_version_is_refused_and_says_so() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("first migration");
    conn.execute(
        "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
        [],
    )
    .expect("forcing a future version");

    let refused = migrate(&conn).expect_err("a future version must be refused");
    assert!(
        refused.contains("99"),
        "the message names the version found: {refused}"
    );
    assert!(
        refused.contains("newer taiyaku"),
        "the message says what that means: {refused}"
    );
}

/// A corrupted version must not read as "fresh". Treating it as 0 would apply
/// migration 1 over a populated database, which fails part-way and leaves the
/// version still wrong.
#[test]
fn an_unparseable_schema_version_is_refused_rather_than_read_as_zero() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("first migration");
    conn.execute(
        "UPDATE meta SET value = 'one' WHERE key = 'schema_version'",
        [],
    )
    .expect("corrupting the version");

    let refused = migrate(&conn).expect_err("an unparseable version must be refused");
    assert!(
        refused.contains("one"),
        "the message names what it found: {refused}"
    );
}

/// **A migration that fails part-way must leave nothing behind, and the
/// database must still be openable afterwards.**
///
/// `execute_batch` opens no transaction of its own: every statement in the
/// file autocommits as it runs, so a failure at statement *n* leaves
/// statements 1..n committed. The version row is not advanced — which sounds
/// like the safe half, and is what makes the outcome unrecoverable rather
/// than merely untidy: the next open re-runs the same migration from
/// statement one, over tables that already exist, and fails there forever.
/// That is precisely the state `migrate`'s own refusal message says it will
/// not create.
///
/// Asserted here in both halves: nothing from the failed migration survives,
/// **and** a corrected migration then applies to the same connection.
#[test]
fn a_migration_that_fails_part_way_leaves_no_trace_and_can_be_retried() {
    let conn = Connection::open_in_memory().expect("in-memory connection");

    let refused = migrate_with(&conn, &["CREATE TABLE early (x); CREATE TABLE early (x);"])
        .expect_err("the repeated CREATE must fail");
    assert!(
        refused.contains("applying schema version 1"),
        "the message names the version it was applying: {refused}"
    );

    let names = table_names(&conn);
    assert!(
        !names.contains(&"early".to_string()),
        "the statement that ran before the failure must have rolled back: {names:?}"
    );
    let version: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .expect("reading the version");
    assert_eq!(
        version, None,
        "a migration that did not apply must not have recorded a version"
    );

    migrate_with(&conn, &["CREATE TABLE early (x);"])
        .expect("the same database migrates once the migration is corrected");
}

/// An older database's migrations each commit on their own, so the version
/// names exactly what is on disk: from version 1, migration 2's table stays,
/// migration 3's does not, and the version sits at 2 rather than at 1 or 3.
#[test]
fn an_older_database_keeps_each_migration_that_committed_before_a_later_one_failed() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate_with(&conn, &["CREATE TABLE first (x);"]).expect("the database is at version 1");

    let refused = migrate_with(
        &conn,
        &[
            "CREATE TABLE first (x);",
            "CREATE TABLE second (x);",
            "CREATE TABLE third (x); CREATE TABLE third (x);",
        ],
    )
    .expect_err("migration 3 must fail");
    assert!(
        refused.contains("applying schema version 3"),
        "the message names the version that failed: {refused}"
    );

    let names = table_names(&conn);
    assert!(
        names.contains(&"second".to_string()),
        "migration 2 committed and must survive: {names:?}"
    );
    assert!(
        !names.contains(&"third".to_string()),
        "migration 3 rolled back whole: {names:?}"
    );

    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .expect("the version row");
    assert_eq!(
        version, "2",
        "the version names the last migration that committed"
    );
}

/// **A fresh database is built in one transaction** (O269), so a later
/// migration failing takes the earlier ones with it: nothing of migration 1
/// stays, no version is recorded, and the next open starts again from an
/// empty file. Committed one at a time, the file would sit at version 1, which
/// every other open reads as an older database to back up first.
#[test]
fn a_fresh_database_whose_second_migration_fails_stays_at_version_zero() {
    let conn = Connection::open_in_memory().expect("in-memory connection");

    let refused = migrate_with(
        &conn,
        &[
            MIGRATIONS[0],
            "CREATE TABLE second (x); CREATE TABLE second (x);",
        ],
    )
    .expect_err("migration 2 must fail");
    assert!(
        refused.contains("applying schema version 2"),
        "the message names the version that failed: {refused}"
    );

    let names = table_names(&conn);
    assert_eq!(
        names,
        vec!["meta".to_string()],
        "migration 1 rolled back with migration 2, leaving only the version's table"
    );
    assert_eq!(
        version(&conn).expect("reading the version"),
        0,
        "a fresh database that failed to build is still fresh"
    );

    migrate_with(&conn, &[MIGRATIONS[0], "CREATE TABLE second (x);"])
        .expect("the same database builds once the migration is corrected");
    assert_eq!(version(&conn).expect("reading the version"), 2);
}

/// Two connections onto one real file, the way two processes reach one
/// `$XDG_DATA_HOME/taiyaku/library.db`. Plain `rusqlite` connections rather
/// than two `Library`s, because what is being reproduced happens *inside*
/// `Library::open`.
fn two_processes(label: &str) -> (crate::tests::TempDir, Connection, Connection) {
    let dir = crate::tests::tempdir(label);
    let db = dir.path().join("library.db");
    let first = Connection::open(&db).expect("the first process opens the file");
    let second = Connection::open(&db).expect("the second process opens the same file");
    (dir, first, second)
}

/// Two processes opening the same *fresh* library both read version 0 before
/// either has a transaction open — `serve` and `taiyaku library import` default
/// to the same file, so starting them together is ordinary rather than exotic —
/// and both then decide to apply migration 1. The second waits on the write
/// lock, gets it once the first has committed, and would run
/// `CREATE TABLE series` over a table that now exists:
///
/// ```text
/// taiyaku: opening the library: applying schema version 1: table series already exists
/// ```
///
/// That is the exact message this module's own doc names as the unrecoverable
/// state it exists to prevent, produced from a different cause. A reader who
/// hits it has every reason to believe their new library is beyond saving.
///
/// The interleaving is written out by hand rather than raced: the second
/// process reads the version, the first migrates, and only then does the second
/// act on what it read.
#[test]
fn a_migration_decided_before_another_process_committed_it_is_not_applied_twice() {
    let (_dir, first, second) = two_processes("schema-race");
    let migrations = ["CREATE TABLE series (id INTEGER PRIMARY KEY);"];

    // The second process looks first, at a database nobody has migrated.
    ensure_meta(&second).expect("meta");
    let seen = version(&second).expect("reading the version");
    assert_eq!(seen, 0, "a fresh database reads as version 0 to both");

    // The first process migrates while the second is still holding that answer.
    migrate_with(&first, &migrations).expect("the first process migrates");

    // And now the second acts on it.
    apply_pending(&second, &migrations, seen)
        .expect("the version has to be read again under the write lock");
    assert_eq!(
        version(&second).expect("reading the version again"),
        1,
        "one migration applied once, by whichever process got there first"
    );
    assert_eq!(
        table_names(&second)
            .iter()
            .filter(|n| *n == "series")
            .count(),
        1
    );
}

/// The same second read, catching the other thing that can have happened while
/// this process was deciding: a *newer* build migrated the file past what this
/// one knows. The refusal is the same one the first read gives, rather than a
/// silent "nothing pending, carry on" over a schema this build cannot read.
#[test]
fn a_database_migrated_past_this_build_while_it_waited_is_refused_rather_than_ignored() {
    let (_dir, first, second) = two_processes("schema-overtaken");

    ensure_meta(&second).expect("meta");
    let seen = version(&second).expect("reading the version");

    migrate_with(&first, &["CREATE TABLE a (x);", "CREATE TABLE b (x);"])
        .expect("a newer build migrates the file to version 2");

    let refused = apply_pending(&second, &["CREATE TABLE a (x);"], seen)
        .expect_err("this build knows one migration and the file is at two");
    assert!(
        refused.contains("newer taiyaku"),
        "the message says what happened: {refused}"
    );
}

/// A file database one migration short of this build, in WAL mode as every
/// open leaves one, so its bytes change only if a later open writes to it.
fn one_version_short(label: &str) -> (crate::tests::TempDir, std::path::PathBuf) {
    let dir = crate::tests::tempdir(label);
    let db = dir.path().join("library.db");
    let conn = Connection::open(&db).expect("a file database");
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("WAL mode");
    migrate_with(&conn, &MIGRATIONS[..SCHEMA_VERSION as usize - 1])
        .expect("migrated to one version short");
    drop(conn);
    (dir, db)
}

fn version_on_disk(db: &std::path::Path) -> u32 {
    version(&Connection::open(db).expect("a plain connection")).expect("the version")
}

fn backups_beside(db: &std::path::Path) -> Vec<crate::backup::Backup> {
    crate::backup::list(&crate::backup::directory_for(db)).expect("listing the backups")
}

/// The name the open mints is one `list` reads back as taken before this
/// build's version, and the copy holds the database as it stood (O263).
#[test]
fn an_open_that_migrates_backs_the_database_up_first_under_the_version_it_migrates_to() {
    let (_dir, db) = one_version_short("backup-before-migrating");

    crate::Library::open(&db).expect("the open migrates");

    let backups = backups_beside(&db);
    assert_eq!(backups.len(), 1, "the open took one backup: {backups:?}");
    let taken = &backups[0];
    assert!(
        taken.name.starts_with("library-")
            && taken
                .name
                .ends_with(&format!("-before-v{SCHEMA_VERSION}.db")),
        "named for the version the open migrates to: {}",
        taken.name
    );
    assert_eq!(
        taken.before,
        Some(SCHEMA_VERSION),
        "and listed as taken before that version"
    );
    let copy = crate::backup::directory_for(&db).join(&taken.name);
    assert_eq!(
        version_on_disk(&copy),
        SCHEMA_VERSION - 1,
        "the backup holds the database before the migration"
    );
    assert_eq!(
        version_on_disk(&db),
        SCHEMA_VERSION,
        "the database migrated"
    );
}

/// Two versions short, the version the open migrates to is this build's, not
/// the next one up: one short cannot tell the two apart.
#[test]
fn a_database_two_versions_short_is_backed_up_under_this_builds_version() {
    let dir = crate::tests::tempdir("backup-two-short");
    let db = dir.path().join("library.db");
    let conn = Connection::open(&db).expect("a file database");
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("WAL mode");
    migrate_with(&conn, &MIGRATIONS[..SCHEMA_VERSION as usize - 2])
        .expect("migrated to two versions short");
    drop(conn);

    crate::Library::open(&db).expect("the open migrates");

    let backups = backups_beside(&db);
    assert_eq!(backups.len(), 1, "the open took one backup: {backups:?}");
    assert!(
        backups[0]
            .name
            .ends_with(&format!("-before-v{SCHEMA_VERSION}.db")),
        "named for this build's version: {}",
        backups[0].name
    );
    assert_eq!(
        backups[0].before,
        Some(SCHEMA_VERSION),
        "and listed as taken before it"
    );
    let copy = crate::backup::directory_for(&db).join(&backups[0].name);
    assert_eq!(
        version_on_disk(&copy),
        SCHEMA_VERSION - 2,
        "the backup holds the database two versions short"
    );
}

#[test]
fn a_fresh_database_and_an_up_to_date_one_take_no_backup() {
    let dir = crate::tests::tempdir("backup-skipped");
    let db = dir.path().join("library.db");

    crate::Library::open(&db).expect("a fresh open");
    let backups = backups_beside(&db);
    assert!(
        backups.is_empty(),
        "a fresh database takes none: {backups:?}"
    );

    crate::Library::open(&db).expect("an open with nothing to migrate");
    let backups = backups_beside(&db);
    assert!(
        backups.is_empty(),
        "nor does one already at this build's version: {backups:?}"
    );
}

/// **Opens that arrive while another builds a fresh database all succeed and
/// take no backup** (O269). The builder is the open path itself, held by an
/// update hook at its second version row. Built migration by migration,
/// version 1 has committed there, so the other opens back up a part-built
/// store and two in one second collide on its name; built in one transaction,
/// they read version 0 and wait on the write lock. Holding the builder gives
/// the opens half a second to read the version mid-build, not milliseconds.
#[test]
fn opens_arriving_while_another_builds_a_fresh_database_all_succeed_and_take_no_backup() {
    const OPENS: usize = 3;
    let dir = crate::tests::tempdir("fresh-build-race");
    let db = dir.path().join("library.db");

    let builder = Connection::open(&db).expect("the builder opens the file");
    let (held, is_held) = std::sync::mpsc::channel::<()>();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let mut version_rows = 0;
    builder
        .update_hook(Some(
            move |_: rusqlite::hooks::Action, _: &str, table: &str, _: i64| {
                if table == "meta" {
                    version_rows += 1;
                    if version_rows == 2 {
                        let _ = held.send(());
                        // Ends early if the test thread has gone, so a failure
                        // there cannot leave the scope waiting on this thread.
                        let _ = released.recv();
                    }
                }
            },
        ))
        .expect("installing the hook");

    let barrier = std::sync::Barrier::new(OPENS);
    let outcomes = std::thread::scope(|scope| {
        let release = release;
        let building =
            scope.spawn(|| crate::Library::from_connection(builder, Some(db.clone())).map(|_| ()));
        is_held
            .recv()
            .expect("the builder reaches its second version row");
        let opens: Vec<_> = (0..OPENS)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    crate::Library::open(&db).map(|_| ())
                })
            })
            .collect();
        // Long enough for every open to read the version, well inside the
        // five seconds an open waits for the write lock.
        std::thread::sleep(std::time::Duration::from_millis(500));
        release.send(()).expect("releasing the builder");
        let mut outcomes = vec![building.join().expect("the builder did not panic")];
        outcomes.extend(
            opens
                .into_iter()
                .map(|o| o.join().expect("no open panicked")),
        );
        outcomes
    });

    for outcome in &outcomes {
        assert!(
            outcome.is_ok(),
            "every open of a fresh database must succeed: {outcomes:?}"
        );
    }
    let backups = backups_beside(&db);
    assert!(
        backups.is_empty(),
        "a fresh database has nothing to back up, however many opens build it: {backups:?}"
    );
    assert_eq!(version_on_disk(&db), SCHEMA_VERSION);
}

/// The schema a fresh database gets in one transaction (O269) is the one it
/// got migration by migration: every table, index, trigger and view as
/// `sqlite_master` holds it, every table's foreign keys, and how many rows
/// each table was seeded with. Foreign keys are off for both builds, set by
/// `without_foreign_keys` before any transaction opens, where the pragma
/// takes effect.
#[test]
fn a_fresh_database_built_in_one_transaction_matches_one_built_migration_by_migration() {
    fn shape(conn: &Connection) -> Vec<Vec<String>> {
        let mut shape = rows_of(
            conn,
            "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name",
        );
        for table in table_names(conn) {
            let keys = rows_of(
                conn,
                &format!("SELECT * FROM pragma_foreign_key_list('{table}') ORDER BY id, seq"),
            );
            shape.extend(keys.into_iter().map(|mut key| {
                key.insert(0, table.clone());
                key
            }));
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |row| {
                    row.get(0)
                })
                .expect("counting a table's rows");
            shape.push(vec![table, count.to_string()]);
        }
        shape
    }

    let whole = Connection::open_in_memory().expect("in-memory connection");
    migrate(&whole).expect("built in one transaction");

    let stepwise = Connection::open_in_memory().expect("in-memory connection");
    for applied in 1..=MIGRATIONS.len() {
        migrate_with(&stepwise, &MIGRATIONS[..applied]).expect("one migration more");
    }

    assert_eq!(version(&whole).expect("version"), SCHEMA_VERSION);
    assert_eq!(version(&stepwise).expect("version"), SCHEMA_VERSION);
    let (whole, stepwise) = (shape(&whole), shape(&stepwise));
    assert!(
        whole.iter().any(|row| row.len() == 9),
        "the comparison reaches at least one foreign key: {whole:?}"
    );
    assert_eq!(whole, stepwise);
}

/// `migrate` refuses a newer database, and nothing is written beside it first.
#[test]
fn a_newer_database_is_refused_with_no_backup_taken() {
    let dir = crate::tests::tempdir("backup-newer");
    let db = dir.path().join("library.db");
    crate::Library::open(&db).expect("a fresh open");
    Connection::open(&db)
        .expect("a plain connection")
        .execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
            [(SCHEMA_VERSION + 1).to_string()],
        )
        .expect("a newer build's version");

    let refused = crate::Library::open(&db).expect_err("a newer database is refused");
    assert!(
        refused.contains("newer taiyaku"),
        "the refusal is migrate's: {refused}"
    );
    let backups = backups_beside(&db);
    assert!(backups.is_empty(), "and no backup was taken: {backups:?}");
}

/// Without its backup the migration has no way back, so a backup that fails
/// refuses it, and a WAL-mode file is left byte for byte as it was (O263).
#[test]
fn a_backup_that_fails_refuses_the_migration_and_leaves_the_file_as_it_was() {
    let (_dir, db) = one_version_short("backup-fails");
    let as_it_was = std::fs::read(&db).expect("the database's bytes");
    std::fs::write(
        crate::backup::directory_for(&db),
        "a file where the directory goes",
    )
    .expect("the backups directory made a plain file");

    let refused = crate::Library::open(&db).expect_err("the open is refused");

    let canonical = db.canonicalize().expect("the path the open names");
    assert!(
        refused.starts_with("the backup library-")
            && refused.contains(&format!(
                "-before-v{SCHEMA_VERSION}.db could not be taken before migrating {}, \
                 so the database was not migrated: ",
                canonical.display()
            )),
        "the refusal names the backup and says the database was not migrated: {refused}"
    );
    assert!(
        std::fs::read(&db).expect("the database's bytes again") == as_it_was,
        "the database's bytes are as they were"
    );
    assert_eq!(
        version_on_disk(&db),
        SCHEMA_VERSION - 1,
        "the database was not migrated"
    );
}

/// A backup whose file cannot be created refuses the migration as well: only a
/// name already taken counts as success. `/proc/self` takes no new file
/// whatever the user, root included, so this needs no permission to deny.
#[cfg(target_os = "linux")]
#[test]
fn a_backup_whose_file_cannot_be_created_refuses_the_migration() {
    let (_dir, db) = one_version_short("backup-create-refused");
    let backups = crate::backup::directory_for(&db);
    std::os::unix::fs::symlink("/proc/self", &backups).expect("the backups directory linked");

    let conn = Connection::open(&db).expect("a plain connection");
    let name = format!("library-20261110T120000Z-before-v{SCHEMA_VERSION}.db");
    let why = crate::backup::take_named(&conn, &backups, &name).expect_err("the create is refused");
    assert!(
        matches!(why, crate::backup::TakeError::Failed(_)),
        "a create refused is a fault, not a name taken: {why:?}"
    );
    drop(conn);

    let refused = crate::Library::open(&db).expect_err("the open is refused");
    assert!(
        refused.contains("so the database was not migrated: creating "),
        "the refusal says why: {refused}"
    );
    assert_eq!(
        version_on_disk(&db),
        SCHEMA_VERSION - 1,
        "the database was not migrated"
    );
}

/// A second process opening the database in the same second took the same
/// backup, so finding it there is success, the migration runs, and the backup
/// already there is left alone.
#[test]
fn a_backup_already_under_the_name_is_success_and_the_migration_runs() {
    let (_dir, db) = one_version_short("backup-exists");
    let name = format!("library-20261110T120000Z-before-v{SCHEMA_VERSION}.db");
    let backups = crate::backup::directory_for(&db);
    std::fs::create_dir_all(&backups).expect("the backups directory");
    std::fs::write(backups.join(&name), "the other process's backup").expect("a backup there");

    let conn = Connection::open(&db).expect("a plain connection");
    crate::backup::before_migration(&conn, &db, &name).expect("a backup already there is success");
    migrate(&conn).expect("and the migration runs");

    assert_eq!(version(&conn).expect("the version"), SCHEMA_VERSION);
    assert_eq!(
        std::fs::read(backups.join(&name)).expect("the backup's bytes"),
        b"the other process's backup",
        "the backup already there is untouched"
    );
}

/// A copy cut off by a kill or a power cut can leave a file under a backup's
/// name, dated after the database's last write, that holds nothing. It is not
/// the backup, so the open takes a whole one of its own (O263).
#[test]
fn an_empty_file_under_a_backups_name_does_not_stand_for_the_database() {
    let (_dir, db) = one_version_short("backup-cut-copy");
    let backups = crate::backup::directory_for(&db);
    std::fs::create_dir_all(&backups).expect("the backups directory");
    let cut = format!("library-20261111T000000Z-before-v{SCHEMA_VERSION}.db");
    std::fs::write(backups.join(&cut), b"").expect("a cut copy under a backup's name");

    crate::Library::open(&db).expect("the open migrates");

    let taken: Vec<_> = backups_beside(&db)
        .into_iter()
        .filter(|backup| backup.name != cut)
        .collect();
    assert_eq!(
        taken.len(),
        1,
        "the open took a backup of its own: {taken:?}"
    );
    assert_eq!(
        version_on_disk(&backups.join(&taken[0].name)),
        SCHEMA_VERSION - 1,
        "a whole one, holding the database before the migration"
    );
}

/// Another open taking the same backup holds its unfinished copy beside the
/// name until it is whole, and one cut off leaves it there. Neither is the
/// backup, so the migration is refused rather than run without one (O263).
#[test]
fn a_backup_still_being_taken_refuses_the_migration() {
    let (_dir, db) = one_version_short("backup-being-taken");
    let name = format!("library-20261110T120000Z-before-v{SCHEMA_VERSION}.db");
    let backups = crate::backup::directory_for(&db);
    std::fs::create_dir_all(&backups).expect("the backups directory");
    std::fs::write(backups.join(format!(".{name}.partial")), b"part of a copy")
        .expect("another open's unfinished copy");

    let conn = Connection::open(&db).expect("a plain connection");
    let refused =
        crate::backup::before_migration(&conn, &db, &name).expect_err("the migration is refused");

    assert_eq!(
        refused,
        format!(
            "the backup {name} could not be taken before migrating {}, so the database was \
             not migrated: another open is taking it, or one cut off left it unfinished; an \
             open in a later second takes one of its own",
            db.display()
        )
    );
    assert!(
        backups_beside(&db).is_empty(),
        "nothing is listed as the backup"
    );
    drop(conn);
    assert_eq!(
        version_on_disk(&db),
        SCHEMA_VERSION - 1,
        "the database was not migrated"
    );
}

/// [`one_version_short`], with a trigger refusing the version row, so every
/// open's migration fails at its last step and rolls back, whatever the
/// migration is: nothing here names one, so a new migration needs no change.
fn one_whose_next_migration_fails(label: &str) -> (crate::tests::TempDir, std::path::PathBuf) {
    let (dir, db) = one_version_short(label);
    Connection::open(&db)
        .expect("a plain connection")
        .execute_batch(
            "CREATE TRIGGER refuse_the_next_version BEFORE UPDATE ON meta
             WHEN OLD.key = 'schema_version'
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .expect("a trigger refusing the next version");
    (dir, db)
}

/// An open whose migration fails, refused by the migration rather than by the
/// backup before it.
fn open_refused_by_its_migration(db: &std::path::Path) {
    let refused = crate::Library::open(db).expect_err("the migration fails");
    assert_eq!(
        refused,
        format!("recording schema version {SCHEMA_VERSION}: refused by the test"),
        "the migration refused the open"
    );
}

/// Waits until the clock has moved into a later second than when it was
/// called, so the next backup name differs from every one minted before.
fn wait_for_the_next_second() {
    let second = |t: std::time::SystemTime| {
        t.duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs()
    };
    let started = second(std::time::SystemTime::now());
    while second(std::time::SystemTime::now()) == started {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A migration that fails restarts a container on `unless-stopped` in a loop.
/// Each start falls in a later second and mints a new name, and nothing ever
/// deletes a `-before-v<N>` backup, so a backup per attempt would fill the
/// disk. The database has not changed since the first attempt's backup, so
/// that one stands for the second (O263).
#[test]
fn a_migration_that_keeps_failing_takes_one_backup_while_the_database_is_unchanged() {
    let (_dir, db) = one_whose_next_migration_fails("backup-restart-loop");

    open_refused_by_its_migration(&db);
    wait_for_the_next_second();
    open_refused_by_its_migration(&db);

    let backups = backups_beside(&db);
    assert_eq!(
        backups.len(),
        1,
        "the second attempt took no backup of its own: {backups:?}"
    );
    assert_eq!(backups[0].before, Some(SCHEMA_VERSION));
}

/// The README's restore moves a file into place, which keeps its older
/// modification time. Where a rename sets the change time, as this test's
/// temporary directory must, the skip counts it and backs up again (O263).
#[test]
fn a_database_moved_into_place_between_two_failed_migrations_is_backed_up_again() {
    let (dir, db) = one_whose_next_migration_fails("backup-moved-in");
    let restored = dir.path().join("restored.db");
    std::fs::copy(&db, &restored).expect("a copy to restore");
    Connection::open(&restored)
        .expect("the copy")
        .execute(
            "INSERT INTO meta (key, value) VALUES ('restored', 'by a move')",
            [],
        )
        .expect("a mark on the copy");

    open_refused_by_its_migration(&db);
    wait_for_the_next_second();
    std::fs::rename(&restored, &db).expect("the restore, by a move");
    open_refused_by_its_migration(&db);

    let backups = backups_beside(&db);
    assert_eq!(
        backups.len(),
        2,
        "the second attempt backed the restored database up: {backups:?}"
    );
    let newest = crate::backup::directory_for(&db).join(&backups[0].name);
    let restored: String = Connection::open(&newest)
        .expect("the newest backup")
        .query_row("SELECT value FROM meta WHERE key = 'restored'", [], |row| {
            row.get(0)
        })
        .expect("the newest backup is the restored database");
    assert_eq!(restored, "by a move");
}

/// The skip holds only while the database is unchanged: a write between two
/// failed attempts, checkpointed into the main file, is backed up again.
#[test]
fn a_database_written_between_two_failed_migrations_is_backed_up_again() {
    let (_dir, db) = one_whose_next_migration_fails("backup-written-between");

    open_refused_by_its_migration(&db);
    wait_for_the_next_second();
    Connection::open(&db)
        .expect("a plain connection")
        .execute(
            "INSERT INTO meta (key, value) VALUES ('written', 'between the attempts')",
            [],
        )
        .expect("a write between the attempts");
    open_refused_by_its_migration(&db);

    let backups = backups_beside(&db);
    assert_eq!(
        backups.len(),
        2,
        "the second attempt backed the changed database up: {backups:?}"
    );
    let newest = crate::backup::directory_for(&db).join(&backups[0].name);
    let written: String = Connection::open(&newest)
        .expect("the newest backup")
        .query_row("SELECT value FROM meta WHERE key = 'written'", [], |row| {
            row.get(0)
        })
        .expect("the write is in the newest backup");
    assert_eq!(written, "between the attempts");
}

/// The `-wal` counts as the database: a write still in it, with its writer
/// open so nothing has reached the main file, is backed up again too.
#[test]
fn a_write_still_in_the_wal_between_two_failed_migrations_is_backed_up_again() {
    let (_dir, db) = one_whose_next_migration_fails("backup-written-to-the-wal");

    open_refused_by_its_migration(&db);
    wait_for_the_next_second();
    let writer = Connection::open(&db).expect("a plain connection");
    writer
        .execute(
            "INSERT INTO meta (key, value) VALUES ('written', 'into the wal')",
            [],
        )
        .expect("a write between the attempts");
    open_refused_by_its_migration(&db);
    drop(writer);

    let backups = backups_beside(&db);
    assert_eq!(
        backups.len(),
        2,
        "the second attempt backed the changed database up: {backups:?}"
    );
}

/// Only a backup before the same version stands for an attempt: one taken
/// before migrating to another version, even after the database's last write,
/// does not.
#[test]
fn a_backup_before_another_version_does_not_stand_for_this_migration() {
    let (_dir, db) = one_version_short("backup-other-version");
    let backups = crate::backup::directory_for(&db);
    std::fs::create_dir_all(&backups).expect("the backups directory");
    let other = format!("library-20261110T120000Z-before-v{}.db", SCHEMA_VERSION - 1);
    std::fs::write(backups.join(&other), "an older migration's backup")
        .expect("a backup before another version");

    crate::Library::open(&db).expect("the open migrates");

    let taken: Vec<_> = backups_beside(&db)
        .into_iter()
        .filter(|backup| backup.before == Some(SCHEMA_VERSION))
        .collect();
    assert_eq!(
        taken.len(),
        1,
        "the open took its own backup before this version: {taken:?}"
    );
}

fn table_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("listing tables");
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .expect("mapping table names");
    rows.filter_map(Result::ok).collect()
}

#[test]
fn migration_one_creates_every_table_the_spec_names() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migration");
    let names = table_names(&conn);
    for expected in [
        "alignment",
        "collection",
        "collection_item",
        "credit",
        "edition",
        "external_id",
        "meta",
        "member",
        "migrated_document",
        "note",
        "page_flag",
        "person",
        "progress",
        "root",
        "run",
        "series",
        "series_tag",
        "series_title",
        "tag",
        "truth_box",
        "truth_member",
        "truth_page",
        "user",
        "work",
        "work_tag",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "missing table `{expected}`: {names:?}"
        );
    }
}

/// `member.ordinal` is not decoration. `sidecar.md`'s "How a member becomes
/// pages" is normative: an edition's page sequence is its members concatenated
/// *in the order given*, and a consumer that reorders them disagrees with every
/// run in the document while `sha256` and `pages` still match. The unique
/// constraint is what makes that unrepresentable rather than merely documented.
#[test]
fn two_members_cannot_share_an_ordinal_within_an_edition() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, 't', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
         INSERT INTO member (edition_id, ordinal, path, sha256, pages)
             VALUES (1, 0, 'a.cbz', 'aa', 10);",
    )
    .expect("first member");

    let refused = conn.execute(
        "INSERT INTO member (edition_id, ordinal, path, sha256, pages)
         VALUES (1, 0, 'b.cbz', 'bb', 10)",
        [],
    );
    assert!(
        refused.is_err(),
        "a second member at ordinal 0 must be refused"
    );
}

/// **The FTS index must survive a rename.** A stale index would match the old
/// name and miss the new one — a wrong search result, with no error anywhere.
/// Both tables are asserted here because each has triggers of its own.
#[test]
fn renaming_a_title_updates_the_search_index_for_both_tables() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO series_title (series_id, title) VALUES (1, 'Old Series');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, 'Old Work', 'rtl', 'raw');
         UPDATE series_title SET title = 'New Series' WHERE series_id = 1;
         UPDATE work SET title = 'New Work' WHERE id = 1;",
    )
    .expect("insert then rename");

    let hits = |table: &str, term: &str| -> i64 {
        conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE {table} MATCH ?1"),
            [term],
            |row| row.get(0),
        )
        .expect("fts query")
    };

    assert_eq!(
        hits("series_fts", "New"),
        1,
        "the new series title must be findable"
    );
    assert_eq!(
        hits("series_fts", "Old"),
        0,
        "the old series title must be gone"
    );
    assert_eq!(
        hits("work_fts", "New"),
        1,
        "the new work title must be findable"
    );
    assert_eq!(
        hits("work_fts", "Old"),
        0,
        "the old work title must be gone"
    );
}

#[test]
fn deleting_a_work_takes_its_editions_and_members_with_it() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, 't', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
         INSERT INTO member (edition_id, ordinal, path, sha256, pages)
             VALUES (1, 0, 'a.cbz', 'aa', 10);
         DELETE FROM work WHERE id = 1;",
    )
    .expect("cascade");

    let members: i64 = conn
        .query_row("SELECT count(*) FROM member", [], |row| row.get(0))
        .expect("counting members");
    assert_eq!(members, 0, "the member should have gone with its work");
}

#[test]
fn migration_three_creates_the_scanner_tables_and_seeds_the_default_rules() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migration");

    let names = table_names(&conn);
    for expected in ["dir_state", "proposal", "rule", "scan_entry"] {
        assert!(
            names.contains(&expected.to_string()),
            "missing table `{expected}`: {names:?}"
        );
    }

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rules, 4, "§4's four defaults, seeded as ordinary rows");

    let kinds: Vec<String> = conn
        .prepare("SELECT kind FROM rule ORDER BY ordinal")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        kinds,
        ["regex", "builtin-volume", "builtin-chapter", "regex"]
    );
}

/// `scan_entry`'s `UNIQUE (root_id, rel_path)` — one row per path per root,
/// whatever changes about the file at it.
///
/// `scan.rs` never relies on it: `record` chooses `INSERT` or `UPDATE` from
/// `held_for`'s own `(root_id, rel_path)` lookup, so a file that merely changed
/// size takes the `UPDATE` branch and no duplicate is ever offered to SQLite.
/// That makes this an invariant nothing in the crate currently enforces from
/// the other side — which is a reason to pin it here, not a reason it does not
/// matter: it is what a second writer, or a future upsert written as
/// `ON CONFLICT`, would be relying on, and it is the constraint that makes
/// `forget_unseen`'s `held` map a complete picture of a root.
///
/// Migration 3 is applied and this file is append-only, so the constraint
/// can only be checked, never corrected.
#[test]
fn one_scan_entry_row_per_path_per_root_whatever_changes_about_the_file() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys");
    migrate(&conn).expect("migration");

    conn.execute("INSERT INTO root (id, path) VALUES (1, '/library')", [])
        .expect("root");
    conn.execute(
        "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
         VALUES (1, 'Series/v01.cbz', 'file', 'zip', 100, 1)",
        [],
    )
    .expect("the first row");

    let again = conn.execute(
        "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
         VALUES (1, 'Series/v01.cbz', 'file', 'zip', 200, 2)",
        [],
    );
    let err = again.expect_err("a second row for the same path must be refused");
    assert!(
        err.to_string().to_lowercase().contains("unique"),
        "refused by the UNIQUE constraint rather than by something else: {err}"
    );

    // The same path under a *different* root is a different fact and must
    // still be admitted — a key of `rel_path` alone would refuse it.
    conn.execute("INSERT INTO root (id, path) VALUES (2, '/second')", [])
        .expect("second root");
    conn.execute(
        "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
         VALUES (2, 'Series/v01.cbz', 'file', 'zip', 100, 1)",
        [],
    )
    .expect("the same path under another root is its own row");

    let rows: i64 = conn
        .query_row("SELECT count(*) FROM scan_entry", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows, 2);
}

#[test]
fn a_dismissed_proposal_is_not_raised_again_when_its_target_is_null() {
    // SQLite treats NULLs as distinct in a UNIQUE constraint, so a plain
    // `UNIQUE (kind, subject_id, target_id)` would let every rescan insert a
    // second `attach-chapters` row for the same directory — and §6's whole
    // argument is that dismissal has to persist or the inbox becomes noise.
    // The expression index is what closes that.
    let conn = Connection::open_in_memory().expect("in-memory connection");
    // `migrate` does not set this and the insert below crosses a foreign key.
    // Same line, for the same reason, as `two_members_cannot_share_an_ordinal
    // _within_an_edition` above.
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys");
    migrate(&conn).expect("migration");
    conn.execute(
        "INSERT INTO series (id, sort_title) VALUES (1, 'Series')",
        [],
    )
    .expect("series");
    let insert =
        "INSERT OR IGNORE INTO proposal (kind, subject_id, target_id, score, reason, state)
                  VALUES ('attach-chapters', 1, NULL, 0.5, 'because', ?1)";
    conn.execute(insert, ["dismissed"]).expect("first");
    conn.execute(insert, ["open"]).expect("second");

    let (count, state): (i64, String) = conn
        .query_row(
            "SELECT count(*), max(state) FROM proposal WHERE kind = 'attach-chapters'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("select");
    assert_eq!(count, 1);
    assert_eq!(state, "dismissed", "the dismissal survived the rescan");
}

/// **Migration 4 rebuilds `user`, and the rebuild must not take the library's
/// notes with it.**
///
/// `password_hash` goes NULL -> NOT NULL, which SQLite cannot do in place, so
/// migration 4 is the documented twelve-step rebuild: create, copy, drop,
/// rename. `progress` and `note` both `REFERENCES user(id) ON DELETE CASCADE`,
/// and `DROP TABLE` on a parent performs an implicit `DELETE` that fires those
/// cascades — so the rebuild written the obvious way copies the `user` rows
/// correctly and deletes every note and every reading position in the library.
///
/// The pragma that would prevent it cannot live in the migration. `PRAGMA
/// foreign_keys` is a **no-op inside a transaction** — it does not error and
/// does not warn, it simply does not apply — and [`apply_pending`] runs every
/// migration inside one. See `docs/notes.md`.
///
/// This test is why `migrate_with` disables foreign keys around the whole run
/// rather than inside it.
#[test]
fn migration_4_rebuilds_user_without_taking_the_notes_with_it() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");

    // Stop at version 3, then plant the state a real library would hold: a
    // work, a note and a reading position, all hanging off the seeded user.
    migrate_with(&conn, &MIGRATIONS[..3]).expect("migrates to 3");
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub)
         VALUES ('k', 's', 'T', 'rtl', 'raw')",
        [],
    )
    .expect("work");
    let work_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('n1', 1, ?1, 'raw', 0, 'written before there was a login', 'now', 'now')",
        [work_id],
    )
    .expect("note");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 42)",
        [work_id],
    )
    .expect("progress");

    migrate(&conn).expect("migrates to 4");

    let notes: i64 = conn
        .query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))
        .expect("counts notes");
    let positions: i64 = conn
        .query_row("SELECT COUNT(*) FROM progress", [], |row| row.get(0))
        .expect("counts positions");
    assert_eq!(notes, 1, "migration 4 cascaded the notes away");
    assert_eq!(
        positions, 1,
        "migration 4 cascaded the reading positions away"
    );

    // The id is what spec A2 §5 turns on: promotion must reach this row rather
    // than insert beside it, and it cannot if the rebuild moved it.
    let (id, hash): (i64, String) = conn
        .query_row("SELECT id, password_hash FROM user", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("reads the user");
    assert_eq!(id, 1, "the seeded user's id did not survive the rebuild");
    assert_eq!(
        hash, "!",
        "a NULL password_hash must become the locked placeholder, which is not \
         a well-formed PHC string and so verifies against nothing"
    );

    // And the children still join to it, which a rebuild that left them
    // pointing at the dropped table would fail.
    let joined: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM note JOIN user ON user.id = note.user_id",
            [],
            |row| row.get(0),
        )
        .expect("joins");
    assert_eq!(joined, 1, "note no longer joins to user");
}

/// The four tables migration 4 adds, and the two columns it adds to `user`.
#[test]
fn migration_4_creates_every_table_spec_a2_names() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate_with(&conn, &MIGRATIONS[..4]).expect("migrates to 4");
    let names = table_names(&conn);
    for expected in ["session", "api_key", "user_root", "login_attempt"] {
        assert!(
            names.iter().any(|name| name == expected),
            "migration 4 did not create `{expected}`; it created {names:?}"
        );
    }
    let capabilities: (i64, i64) = conn
        .query_row(
            "SELECT can_download, can_stream FROM user WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("reads the capability columns");
    assert_eq!(
        capabilities,
        (1, 1),
        "both capabilities default on, per spec A2 §3"
    );
}

/// **Migration 5 rebuilds `note`, and the rows and the index both have to
/// survive it.**
///
/// Migration 4's lesson applied to a second rebuild: what a `DROP TABLE` takes
/// away is not only what the migration mentions. Nothing REFERENCES `note`, so
/// there is no cascade here — but `DROP TABLE` still takes the table's own
/// indexes, silently, and a notes read that scans is correct and slower with
/// nothing able to tell the difference.
#[test]
fn migration_5_rebuilds_note_keeping_the_rows_and_the_index() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on");

    migrate_with(&conn, &MIGRATIONS[..4]).expect("migrates to 4");
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub)
         VALUES ('k', 's', 'T', 'rtl', 'raw')",
        [],
    )
    .expect("work");
    let work_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('01HKEEP', 1, ?1, 'raw', 3, 'a note from before', 'then', 'then')",
        [work_id],
    )
    .expect("note");

    migrate(&conn).expect("migrates to 5");

    let (id, slot, body): (String, i64, String) = conn
        .query_row("SELECT id, slot, body FROM note", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .expect("the note survived");
    assert_eq!(id, "01HKEEP");
    assert_eq!(slot, 3, "a column was dropped or reordered by the copy");
    assert_eq!(body, "a note from before");

    let indexes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
              WHERE type = 'index' AND tbl_name = 'note' AND name = 'note_by_work'",
            [],
            |row| row.get(0),
        )
        .expect("counts");
    assert_eq!(
        indexes, 1,
        "DROP TABLE took `note_by_work` with it and nothing put it back"
    );

    // And the new key is the pair, which is the whole point of the rebuild.
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'bob', '!', 'user')",
        [],
    )
    .expect("second user");
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('01HKEEP', 2, ?1, 'raw', 0, \"bob's, same id\", 'then', 'then')",
        [work_id],
    )
    .expect("two readers may hold one id");
}

/// **The role column fails closed and refuses a third word.**
///
/// Migration 1 wrote `DEFAULT 'admin'` when there was one account and no login.
/// An insert omitting the column must not mint an administrator, and a typo
/// must not store a role that is neither, which `Principal::from_account`'s
/// `== "admin"` would read as a reader without saying so.
#[test]
fn a_role_defaults_to_reader_and_refuses_anything_else() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates");

    conn.execute(
        "INSERT INTO user (name, password_hash) VALUES ('omitted', '!')",
        [],
    )
    .expect("inserts");
    let role: String = conn
        .query_row("SELECT role FROM user WHERE name = 'omitted'", [], |row| {
            row.get(0)
        })
        .expect("reads");
    assert_eq!(role, "user", "an omitted role minted an administrator");

    assert!(
        conn.execute(
            "INSERT INTO user (name, password_hash, role) VALUES ('typo', '!', 'administrator')",
            [],
        )
        .is_err(),
        "a role that is neither word was stored"
    );

    // And the row migration 1 seeded is still an admin, because it names one.
    let seeded: String = conn
        .query_row("SELECT role FROM user WHERE id = 1", [], |row| row.get(0))
        .expect("reads");
    assert_eq!(seeded, "admin");
}

/// Migration 6 adds `member.root_id` and leaves every existing member NULL.
///
/// **NULL is load-bearing rather than incidental.** An imported work sits
/// under no root at all, and spec C1 §5.1's filter reads a NULL member as
/// belonging to no root — which is what lets `member.root_id IN (…)` exclude
/// it with no special case in the SQL. A `NOT NULL DEFAULT 0` would have made
/// every pre-existing member claim root 0, which no row references.
#[test]
fn migration_6_adds_member_root_id_leaving_existing_members_null() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates");

    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]}},
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json")).expect("imports");

    let root_id: Option<i64> = conn
        .query_row("SELECT root_id FROM member LIMIT 1", [], |row| row.get(0))
        .expect("reading the new column");
    assert_eq!(root_id, None, "an imported member belongs to no root");
}

/// Migration 20 adds `work.missing_since` and leaves every existing work
/// NULL, which reads as present: a store migrating has not been asked yet
/// whether any directory still holds its works, and a work marked by the
/// migration would vanish from every listing before a scan could say so.
#[test]
fn migration_20_adds_missing_since_leaving_existing_works_present() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate_with(&conn, &MIGRATIONS[..19]).expect("migrates to 19");
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (7, 'k', 'a-work', 'A Work', 'rtl', 'raw')",
        [],
    )
    .expect("seeds a schema-19 work");

    migrate(&conn).expect("migrates to 20");

    let mark: Option<String> = conn
        .query_row("SELECT missing_since FROM work WHERE id = 7", [], |row| {
            row.get(0)
        })
        .expect("reading the new column");
    assert_eq!(mark, None, "an existing work is present");
}

/// The version and the migration count are one fact, and this is the refusal
/// that depends on it staying true.
#[test]
fn the_schema_version_is_twenty_one() {
    assert_eq!(SCHEMA_VERSION, 21);
    assert_eq!(
        MIGRATIONS.len(),
        SCHEMA_VERSION as usize,
        "the version and the migration count are one fact"
    );
}

/// Migration 010's two `UPDATE … SET *_derived = *` statements are what
/// `librarydoc::apply`'s `COALESCE(?2, title_derived)` depends on to be safe on
/// every pre-010 row, so this test runs them over a row that already exists
/// rather than over an empty table. `title_hand`/`label_hand` reading `None`
/// shows nothing about them: a plain `INSERT` leaves those NULL whether or not
/// migration 010 ever ran.
///
/// `migrate_with(&conn, &MIGRATIONS[..9])` is the established pattern for
/// this in the same file (`migration_7_backfills_the_root_of_a_member_under_
/// one`): stop one migration short, seed a row in the shape the *previous*
/// schema version actually left it, then apply the migration under test and
/// check what it did to a real row rather than a fresh one.
#[test]
fn migration_ten_backfills_derived_columns_and_leaves_hand_ones_null() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    let upto: Vec<&str> = MIGRATIONS[..9].to_vec();
    migrate_with(&conn, &upto).expect("migrates to 9, before title_hand/title_derived exist");

    conn.execute_batch(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 'w', 'Derived Title', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label)
         VALUES (1, 1, 'en', 'Derived Label');",
    )
    .expect("seeding a pre-010 row, the shape migration 010 actually runs over");

    migrate(&conn).expect("applies migration 10");

    let (title_hand, title_derived): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT title_hand, title_derived FROM work WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reading work's hand and derived titles");
    let (label_hand, label_derived): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT label_hand, label_derived FROM edition WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reading edition's hand and derived labels");

    assert_eq!(title_hand, None, "a pre-010 row never had a hand-set title");
    assert_eq!(label_hand, None, "a pre-010 row never had a hand-set label");
    assert_eq!(
        title_derived,
        Some("Derived Title".to_string()),
        "the backfill states a truth rather than guessing one (010's own \
         comment): every title in a pre-010 database is the derived one, so \
         the migration must copy the existing title across, not leave the \
         new column NULL"
    );
    assert_eq!(
        label_derived,
        Some("Derived Label".to_string()),
        "same backfill, same reason, for edition.label"
    );
}

/// Migration 7 backfills the root of every member a scan produced, which
/// migration 6 left NULL.
///
/// **The bug this closes blanks a library rather than leaking one.**
#[test]
fn migration_7_backfills_the_root_of_a_member_under_one() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    // Stop one migration short, so the rows exist in the state 006 left them.
    let upto: Vec<&str> = MIGRATIONS[..6].to_vec();
    migrate_with(&conn, &upto).expect("migrates to 6");

    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, '/shelf'), (2, '/shelf/inner')",
        [],
    )
    .expect("adds two roots, one nested in the other");
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 's', 'T', 'rtl', 'raw')",
        [],
    )
    .expect("a work");
    conn.execute(
        "INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'R')",
        [],
    )
    .expect("an edition");
    conn.execute(
        "INSERT INTO member (id, edition_id, ordinal, path, sha256, pages) VALUES
           (1, 1, 0, '/shelf/Series - v01', 'a', 2),
           (2, 1, 1, '/shelf/inner/Series - v02', 'b', 2),
           (3, 1, 2, 'raw/v01.cbz', 'c', 2)",
        [],
    )
    .expect("three members: one under each root, one relative");

    migrate(&conn).expect("applies migration 7");

    let roots: Vec<Option<i64>> = {
        let mut stmt = conn
            .prepare("SELECT root_id FROM member ORDER BY id")
            .expect("prepares");
        stmt.query_map([], |row| row.get(0))
            .expect("reads")
            .collect::<rusqlite::Result<_>>()
            .expect("reads")
    };
    assert_eq!(
        roots,
        vec![Some(1), Some(2), None],
        "the nested root wins for its own member, and a relative path \
         (an imported work) stays NULL"
    );
}

/// Migration 8 makes a Japanese title findable by a substring of it, which is
/// what the default tokeniser could not do at any query length.
///
/// **`の花嫁` rather than `花嫁`.** Trigram indexes three-character windows, so
/// a two-character query matches nothing however correct the index is;
/// `browse::search` answers those with `LIKE` instead, and this test is about
/// the index rather than about that fallback.
#[test]
fn migration_8_finds_a_japanese_title_by_its_middle() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'gotoubun');
         INSERT INTO series_title (series_id, title) VALUES (1, '五等分の花嫁');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, '小林さんちのメイドラゴン 第1巻', 'rtl', 'raw');",
    )
    .expect("two real titles");

    let hits = |table: &str, term: &str| -> i64 {
        conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE {table} MATCH ?"),
            [format!("\"{term}\"")],
            |row| row.get(0),
        )
        .expect("fts query")
    };

    assert_eq!(
        hits("series_fts", "の花嫁"),
        1,
        "a substring from the middle of the run must match"
    );
    assert_eq!(
        hits("series_fts", "五等分"),
        1,
        "and so must one from the start"
    );
    assert_eq!(
        hits("work_fts", "メイドラゴン"),
        1,
        "the work index is tokenised the same way"
    );
    assert_eq!(
        hits("series_fts", "花婿"),
        0,
        "a substring that is not there still matches nothing"
    );
}

/// The rebuild is not the end of it: the triggers must still be writing into
/// the tables migration 8 replaced.
///
/// **This is the failure the migration would otherwise hide.** `'rebuild'`
/// populates the index once, so every test that inserts before migrating
/// passes whether or not a trigger ever fires again. A row written *after*
/// the migration is the only one that proves it.
#[test]
fn migration_8_leaves_the_triggers_writing_into_the_new_tables() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'kaguya');
         INSERT INTO series_title (series_id, title) VALUES (1, 'かぐや様');
         UPDATE series_title SET title = 'ぼっち・ざ・ろっく!' WHERE series_id = 1;",
    )
    .expect("insert then rename, both after the migration");

    let hits = |term: &str| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM series_fts WHERE series_fts MATCH ?",
            [format!("\"{term}\"")],
            |row| row.get(0),
        )
        .expect("fts query")
    };

    assert_eq!(
        hits("ざ・ろっ"),
        1,
        "the update trigger wrote the new title"
    );
    assert_eq!(hits("かぐや様"), 0, "and deleted the old one");
}

/// `'rebuild'` is the one statement in migration 8 that the two tests above
/// cannot exercise: both of them call `migrate` on an empty database and
/// insert their rows afterward, so the triggers alone populate the trigram
/// tables and `'rebuild'` runs over zero content rows either way. A real
/// library is never empty when 008 runs — it is a version-7 database that
/// already holds titles indexed by `unicode61`, and `'rebuild'` is what
/// carries them into the replacement tables. This test is that database.
///
/// **Stop at migration 7, write the titles while the tables are still
/// `unicode61`, then apply 008.** If `'rebuild'` were missing or broken,
/// `series_fts` and `work_fts` would come out empty and stay that way until
/// something next wrote to the row — every already-imported title in a real
/// library would silently drop out of search. Deleting the two `'rebuild'`
/// statements from `008_trigram_fts.sql` must fail this test alone; the two
/// above populate after migrating and so cannot see the gap.
#[test]
fn migration_8_rebuild_reindexes_titles_that_predate_it() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    // Stop one migration short, so the rows are written while series_fts and
    // work_fts are still the unicode61 tables 001 created.
    let upto: Vec<&str> = MIGRATIONS[..7].to_vec();
    migrate_with(&conn, &upto).expect("migrates to 7");

    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'gotoubun');
         INSERT INTO series_title (series_id, title) VALUES (1, '五等分の花嫁');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, '小林さんちのメイドラゴン 第1巻', 'rtl', 'raw');",
    )
    .expect("two real titles, written before migration 8 exists");

    migrate(&conn).expect("applies migration 8 over a database that already holds titles");

    let hits = |table: &str, term: &str| -> i64 {
        conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE {table} MATCH ?"),
            [format!("\"{term}\"")],
            |row| row.get(0),
        )
        .expect("fts query")
    };

    assert_eq!(
        hits("series_fts", "の花嫁"),
        1,
        "rebuild must have reindexed the title that already existed"
    );
    assert_eq!(
        hits("work_fts", "メイドラゴン"),
        1,
        "and the work index too"
    );
}

/// Two editions of one work, both with a position, at the new key.
///
/// The narrow key made this shape unreachable before the migration, so the
/// rows are inserted after it has run — what is under test is that the wider
/// key admits them at all. The sibling test,
/// `migration_9_rebuild_carries_a_pre_existing_position_across_intact`, is
/// what checks the other half: that a row written under the *old* key
/// survives the rebuild.
#[test]
fn migration_9_lets_one_work_hold_a_position_per_edition() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    migrate(&conn).expect("migrating");

    conn.execute(
        "INSERT INTO user (id, name, password_hash) VALUES (2, 'reader', '!')",
        [],
    )
    .expect("user");
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 's', 't', 'rtl', 'raw')",
        [],
    )
    .expect("work");

    for edition in ["raw", "full-color"] {
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (2, 1, ?1, 7)",
            [edition],
        )
        .expect("a position per edition");
    }

    let held: i64 = conn
        .query_row("SELECT count(*) FROM progress WHERE work_id = 1", [], |r| {
            r.get(0)
        })
        .expect("counting");
    assert_eq!(held, 2, "both editions' positions must stand");
}

/// **The migration's stated risk, actually exercised.**
/// `migration_4_rebuilds_user_without_taking_the_notes_with_it`,
/// `migration_5_rebuilds_note_keeping_the_rows_and_the_index` and
/// `migration_8_rebuild_reindexes_titles_that_predate_it` each plant a row at
/// the prior version and migrate it forward.
/// `migration_9_lets_one_work_hold_a_position_per_edition` does not: it
/// migrates a fresh database straight to version 9 and only inserts afterwards,
/// which shows the new key admits two editions but never shows that a row
/// written under the *old* `(user_id, work_id)` key — the shape every real
/// library holds before this migration ever runs — survives the `progress_new`
/// copy intact.
///
/// So this test stops at 8, writes a row the old key still accepts, with
/// `completed_at` and `read_count` both set, then runs the full migration and
/// checks all four columns came across unchanged.
#[test]
fn migration_9_rebuild_carries_a_pre_existing_position_across_intact() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");

    migrate_with(&conn, &MIGRATIONS[..8]).expect("migrates to 8");
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub)
         VALUES ('k', 's', 'T', 'rtl', 'raw')",
        [],
    )
    .expect("work");
    let work_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, completed_at, read_count)
         VALUES (1, ?1, 'raw-mono', 42, '2026-03-01T00:00:00Z', 2)",
        [work_id],
    )
    .expect("a reading position written under the old (user_id, work_id) key");

    migrate(&conn).expect("migrates to 9");

    let (edition, slot, completed_at, read_count): (String, i64, Option<String>, i64) = conn
        .query_row(
            "SELECT edition, slot, completed_at, read_count FROM progress
             WHERE user_id = 1 AND work_id = ?1",
            [work_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("the pre-009 row survived the rebuild");
    assert_eq!(
        edition, "raw-mono",
        "the rebuild must not drop or reorder a column"
    );
    assert_eq!(slot, 42);
    assert_eq!(
        completed_at.as_deref(),
        Some("2026-03-01T00:00:00Z"),
        "a finished-once fact must not be lost by the copy"
    );
    assert_eq!(read_count, 2);
}

/// Spec C3 §6's capability arrives switched **off** for every account that
/// already exists, including the administrator migration 1 seeds.
///
/// `can_download` and `can_stream` default to 1 because they widen what an
/// account may read and every account was already doing both. This one
/// decides whether an account may rewrite archives taiyaku does not own, and
/// no account anywhere was already doing that — so defaulting it on would
/// hand every library's administrators a power none of them asked for, on an
/// upgrade none of them read. See
/// `docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md` §1.1.
#[test]
fn migration_13_gives_every_existing_account_can_stamp_off() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates to 13");

    let can_stamp: i64 = conn
        .query_row("SELECT can_stamp FROM user WHERE id = 1", [], |row| {
            row.get(0)
        })
        .expect("the seeded account carries the column");

    assert_eq!(can_stamp, 0, "the seeded administrator may not stamp");
}

/// A schema-14 library for migration 15 to file, chosen so each backfill rule
/// gives a different answer from the rule it could be mistaken for.
///
/// - Roots 1 and 2 share the last component `Volumes`, so both are named by
///   their whole path; root 3's `Chapters` is not shared.
/// - Roots 4 and 5 share the last component `Comics` only once each is
///   trimmed of spaces, so both are named by their whole path, trimmed.
///   Root 6's last component is only spaces, so it is named by its whole
///   path, trimmed.
/// - Work 20's hub (`raw`) is under root 2 and its `en` edition under root 1:
///   the hub rule says root 2's library, the lowest-root rule says root 1's.
/// - Work 40 has a value in every column, each different from the others,
///   so a rebuild that copied one column into another would show.
/// - Work 60's hub member is under no root (an imported edition) and its
///   `scan` edition is under root 1: only the fallback files it.
/// - Work 50 is wholly imported and stays in no library.
/// - Series 2 holds work 20 (root 2) and works 30 and 40 (root 3): its
///   first work's library and its lowest library are root 2's, and the
///   library most of its works are in is root 3's.
/// - Series 3 holds nothing.
/// - Series 4 holds work 70 (root 3) and work 80 (root 1), a tie: its first
///   work's library is root 3's, and the lower library id is root 1's.
const SCHEMA_14_LIBRARY: &str = "
    INSERT INTO user (id, name, password_hash, role) VALUES (2, 'reader', '!', 'user');
    INSERT INTO root (id, path) VALUES
      (1, '/srv/a/Volumes'), (2, '/srv/b/Volumes'), (3, '/srv/Chapters'),
      (4, '/srv/ Comics '), (5, '/srv/old/Comics '), (6, '/srv/   ');
    INSERT INTO series (id, sort_title) VALUES
      (1, 'Alpha'), (2, 'Beta'), (3, 'Orphan'), (4, 'Delta');
    INSERT INTO work (id, content_key, slug, series_id, volume, title, reading, hub, sidecar_path)
    VALUES (10, 'k-alpha-1', 'alpha-v01', 1, 1, 'Alpha v01', 'rtl', 'raw', NULL),
           (20, 'k-beta-1',  'beta-v01',  2, 1, 'Beta v01',  'rtl', 'raw', NULL),
           (30, 'k-beta-2',  'beta-v02',  2, 2, 'Beta v02',  'rtl', 'raw', NULL),
           (50, 'k-imported', 'imported', NULL, NULL, 'Imported', 'ltr', 'raw',
                '/docs/imported/taiyaku.json'),
           (60, 'k-merged',  'merged',    1, 2, 'Alpha v02', 'rtl', 'raw',
                '/docs/merged/taiyaku.json'),
           (70, 'k-delta-1', 'delta-v01', 4, 1, 'Delta v01', 'rtl', 'raw', NULL),
           (80, 'k-delta-2', 'delta-v02', 4, 2, 'Delta v02', 'rtl', 'raw', NULL);
    INSERT INTO work (id, content_key, slug, series_id, volume, title, reading, hub,
                      sidecar_path, sidecar_mtime, imported, title_hand, title_derived,
                      volume_hand, series_id_hand)
    VALUES (40, 'k-beta-3', 'beta-v03', 2, 3, 'Beta v03', 'rtl', 'raw',
            '/docs/beta-v03/taiyaku.json', 1700000000, '2026-01-02 03:04:05',
            'Beta, volume three', 'Beta v03 (derived)', 4.5, 1);
    INSERT INTO edition (id, work_id, key, label) VALUES
      (100, 10, 'raw', 'raw'),
      (200, 20, 'raw', 'raw'), (201, 20, 'en', 'en'),
      (300, 30, 'raw', 'raw'),
      (400, 40, 'raw', 'raw'),
      (500, 50, 'raw', 'raw'),
      (600, 60, 'raw', 'raw'), (601, 60, 'scan', 'scan'),
      (700, 70, 'raw', 'raw'),
      (800, 80, 'raw', 'raw');
    INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id) VALUES
      (1, 100, 0, '/srv/a/Volumes/Alpha/v01.cbz',    'a1',  10, 1),
      (2, 200, 0, '/srv/b/Volumes/Beta/v01.cbz',     'b1',  10, 2),
      (3, 201, 0, '/srv/a/Volumes/Beta/v01-en.cbz',  'b1e', 10, 1),
      (4, 300, 0, '/srv/Chapters/Beta/v02.cbz',      'b2',  10, 3),
      (5, 400, 0, '/srv/Chapters/Beta/v03.cbz',      'b3',  10, 3),
      (6, 500, 0, '/docs/imported/v.cbz',            'i1',  10, NULL),
      (7, 600, 0, '/docs/merged/v.cbz',              'm1',  10, NULL),
      (8, 601, 0, '/srv/a/Volumes/Alpha/v02.cbz',    'm1s', 10, 1),
      (9, 700, 0, '/srv/Chapters/Delta/v01.cbz',     'd1',  10, 3),
      (10, 800, 0, '/srv/a/Volumes/Delta/v02.cbz',   'd2',  10, 1);
    INSERT INTO progress (user_id, work_id, edition, slot) VALUES
      (1, 10, 'raw', 3), (1, 20, 'en', 4), (2, 50, 'raw', 5);
    INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
    VALUES (1, 'n1', 30, 'raw', 2, 'hello', 'now', 'now');
    INSERT INTO user_root (user_id, root_id) VALUES (2, 1), (2, 3);
";

/// [`SCHEMA_14_LIBRARY`] at version 14, foreign keys on as `Library::open`
/// sets them.
fn a_schema_14_library() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..14]).expect("migrates to 14");
    conn.execute_batch(SCHEMA_14_LIBRARY)
        .expect("seeds a schema-14 library");
    conn
}

/// Every row of `sql`'s answer, each column rendered as text, so two reads
/// can be compared whole.
fn rows_of(conn: &Connection, sql: &str) -> Vec<Vec<String>> {
    let mut stmt = conn.prepare(sql).expect(sql);
    let width = stmt.column_count();
    stmt.query_map([], |row| {
        (0..width)
            .map(|i| {
                let value: rusqlite::types::Value = row.get(i)?;
                Ok(format!("{value:?}"))
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .expect(sql)
    .collect::<rusqlite::Result<Vec<_>>>()
    .expect(sql)
}

/// The library each named thing is filed under, by name; `None` for none.
fn library_names(conn: &Connection, sql: &str) -> Vec<(i64, Option<String>)> {
    let mut stmt = conn.prepare(sql).expect(sql);
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect(sql)
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect(sql)
}

#[test]
fn migration_15_files_every_root_under_a_library_named_for_its_dir() {
    let conn = a_schema_14_library();
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT r.id, l.name FROM root r LEFT JOIN library l ON l.id = r.library_id ORDER BY r.id"
        ),
        vec![
            (1, Some("/srv/a/Volumes".to_string())),
            (2, Some("/srv/b/Volumes".to_string())),
            (3, Some("Chapters".to_string())),
            (4, Some("/srv/ Comics".to_string())),
            (5, Some("/srv/old/Comics".to_string())),
            (6, Some("/srv/".to_string())),
        ],
        "two directories called Volumes are named by their whole paths; Chapters by its own; \
         two called Comics once trimmed by their whole paths, trimmed; and a directory named \
         only spaces by its whole path, trimmed"
    );
}

/// **Two directories whose paths differ only in surrounding spaces keep their
/// paths as they are.** Both share the leaf `Extra`, so both fall back to the
/// whole path, and the trimmed paths are the same, so trimming them would
/// collide on `library.name` and the migration, and the database, would not
/// open.
#[test]
fn migration_15_keeps_whole_paths_that_trimming_would_make_the_same() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..14]).expect("migrates to 14");
    conn.execute_batch(
        "INSERT INTO root (id, path) VALUES (1, '/srv/a/Extra'), (2, '/srv/a/Extra ');",
    )
    .expect("two directories whose paths differ only in a trailing space");
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15 without a name collision");
    assert_eq!(
        library_names(
            &conn,
            "SELECT r.id, l.name FROM root r LEFT JOIN library l ON l.id = r.library_id ORDER BY r.id"
        ),
        vec![
            (1, Some("/srv/a/Extra".to_string())),
            (2, Some("/srv/a/Extra ".to_string())),
        ]
    );
}

/// A library's id is never given to a later library. The backfill numbers
/// each library after its root, and a library made afterwards is numbered
/// after the highest of those, though that library has since gone and a
/// lower number is free.
#[test]
fn migration_15_never_gives_a_librarys_id_to_another() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..14]).expect("migrates to 14");
    conn.execute_batch("INSERT INTO root (id, path) VALUES (1, '/srv/manga'), (5, '/srv/comics');")
        .expect("two roots, the second numbered past a gap");
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");
    let ids = |conn: &Connection| -> Vec<i64> {
        conn.prepare("SELECT id FROM library ORDER BY id")
            .expect("prepares")
            .query_map([], |r| r.get(0))
            .expect("reads")
            .collect::<rusqlite::Result<_>>()
            .expect("reads")
    };
    assert_eq!(
        ids(&conn),
        vec![1, 5],
        "the fixture: each library takes its root's id"
    );

    conn.execute("DELETE FROM root WHERE id = 5", [])
        .expect("the last directory of library 5 goes, and the library with it");
    assert_eq!(ids(&conn), vec![1], "the fixture: library 5 is gone");
    conn.execute("INSERT INTO root (path) VALUES ('/srv/zines')", [])
        .expect("a new directory, filed under a library of its own");

    assert_eq!(
        ids(&conn),
        vec![1, 6],
        "the new library is numbered after 5, which no library holds any more"
    );
}

#[test]
fn migration_15_files_each_work_under_its_hub_editions_library() {
    let conn = a_schema_14_library();
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT w.id, l.name FROM work w LEFT JOIN library l ON l.id = w.library_id ORDER BY w.id"
        ),
        vec![
            (10, Some("/srv/a/Volumes".to_string())),
            (20, Some("/srv/b/Volumes".to_string())),
            (30, Some("Chapters".to_string())),
            (40, Some("Chapters".to_string())),
            (50, None),
            (60, Some("/srv/a/Volumes".to_string())),
            (70, Some("Chapters".to_string())),
            (80, Some("/srv/a/Volumes".to_string())),
        ],
        "20 follows its hub, not its lowest root; 60 falls back to its scanned edition; \
         50 is imported and in no library"
    );
}

#[test]
fn migration_15_files_each_series_where_most_of_its_works_are() {
    let conn = a_schema_14_library();
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT s.id, l.name FROM series s LEFT JOIN library l ON l.id = s.library_id ORDER BY s.id"
        ),
        vec![
            (1, Some("/srv/a/Volumes".to_string())),
            (2, Some("Chapters".to_string())),
            (3, None),
            (4, Some("/srv/a/Volumes".to_string())),
        ],
        "Beta's first work and its lowest library are /srv/b/Volumes; two of its three \
         works are in Chapters; Delta's two works tie, and the lower library id wins over \
         its first work's"
    );
}

/// Every column `work` had at version 14 is read from the table itself rather
/// than listed here, so none can be left out of the comparison.
#[test]
fn migration_15_keeps_every_work_position_and_note_where_it_was() {
    let conn = a_schema_14_library();
    let columns: String = conn
        .query_row(
            "SELECT group_concat(name, ', ') FROM pragma_table_info('work')",
            [],
            |r| r.get(0),
        )
        .expect("work's columns at version 14");
    let works = format!("SELECT {columns} FROM work ORDER BY id");
    let works = works.as_str();
    let progress = "SELECT user_id, work_id, edition, slot FROM progress ORDER BY user_id, work_id";
    let notes = "SELECT user_id, id, work_id, edition, slot, body FROM note ORDER BY id";
    let before = (
        rows_of(&conn, works),
        rows_of(&conn, progress),
        rows_of(&conn, notes),
    );
    let work_40 = rows_of(&conn, &format!("SELECT {columns} FROM work WHERE id = 40"));
    assert!(
        !work_40[0].iter().any(|value| value == "Null"),
        "the fixture's work 40 must fill every column for a swap to show: {work_40:?}"
    );

    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");

    assert_eq!(
        rows_of(&conn, works),
        before.0,
        "the rebuild changed a work"
    );
    assert_eq!(
        rows_of(&conn, progress),
        before.1,
        "a reading position moved or went"
    );
    assert_eq!(rows_of(&conn, notes), before.2, "a note moved or went");
}

#[test]
fn migration_15_moves_each_root_grant_to_that_roots_library() {
    let conn = a_schema_14_library();
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT ul.user_id, l.name FROM user_library ul JOIN library l ON l.id = ul.library_id \
             ORDER BY l.name"
        ),
        vec![
            (2, Some("/srv/a/Volumes".to_string())),
            (2, Some("Chapters".to_string())),
        ]
    );
    assert!(
        !table_names(&conn).iter().any(|name| name == "user_root"),
        "user_root is gone"
    );
}

/// `DROP TABLE work` takes migration 001's search triggers and migration 014's
/// counter triggers with it. `structure::tests` checks the counter's on a
/// fresh database; this checks both over rows that predate the rebuild.
#[test]
fn migration_15_leaves_search_and_the_structure_counter_working_on_work() {
    let conn = a_schema_14_library();
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");

    let matching = |term: &str| -> Vec<i64> {
        let mut stmt = conn
            .prepare("SELECT rowid FROM work_fts WHERE work_fts MATCH ?1 ORDER BY rowid")
            .expect("prepares");
        stmt.query_map([term], |r| r.get(0))
            .expect("searches")
            .collect::<rusqlite::Result<_>>()
            .expect("reads")
    };
    assert_eq!(
        matching("Beta"),
        vec![20, 30, 40],
        "the index survived the rebuild"
    );

    let before = crate::structure::version(&conn).expect("reads the counter");
    conn.execute("UPDATE work SET title = 'Gamma v02' WHERE id = 30", [])
        .expect("renames a work");
    assert_eq!(matching("Gamma"), vec![30], "work_au is back");
    assert!(
        crate::structure::version(&conn).expect("reads the counter") > before,
        "structure_work_upd is back"
    );
    conn.execute(
        "INSERT INTO work_fts (work_fts) VALUES ('integrity-check')",
        [],
    )
    .expect("the search index agrees with work");
}

/// Every index and trigger `work` had before the rebuild is back, word for
/// word. The case above exercises two of the triggers by what they do; this
/// pins all of them, and the index, by their definitions. Automatic indexes
/// have no `sql` and are left out: the constraints they serve are the ones
/// the rebuild changes.
#[test]
fn migration_15_recreates_every_index_and_trigger_work_had() {
    let conn = a_schema_14_library();
    let objects = "SELECT type, name, sql FROM sqlite_master \
                   WHERE tbl_name = 'work' AND type IN ('index', 'trigger') AND sql IS NOT NULL \
                   ORDER BY name";
    let before = rows_of(&conn, objects);

    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");

    assert_eq!(
        rows_of(&conn, objects),
        before,
        "the rebuild lost or changed an index or trigger on work"
    );
}

/// Ids for the trigger cases, with library ids that differ from root ids so a
/// statement joining the wrong pair cannot pass by coincidence. In the shape of
/// versions 15 to 18, where a directory names its library in `root.library_id`;
/// [`TWO_LINKED_LIBRARIES`] is the same store as a link.
const TWO_DIR_LIBRARY: &str = "
    INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
    INSERT INTO root (id, path, library_id) VALUES
      (7, '/srv/manga', 70), (8, '/srv/raw', 70), (9, '/srv/other', 80);
";

/// A directory added with no library is filed under one of its own, named in
/// Rust by `scan::own_library_name` and linked in the add's transaction.
#[test]
fn a_root_written_without_a_library_is_filed_under_one_of_its_own() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates");
    for path in ["/x/Extras", "/y/Extras", "/"] {
        crate::scan::insert_root(
            &conn,
            std::path::Path::new(path),
            crate::scan::InLibrary::Own,
        )
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    }
    assert_eq!(
        library_names(
            &conn,
            "SELECT r.id, l.name FROM root r LEFT JOIN library_root lr ON lr.root_id = r.id
               LEFT JOIN library l ON l.id = lr.library_id ORDER BY r.id"
        ),
        vec![
            (1, Some("Extras".to_string())),
            (2, Some("/y/Extras".to_string())),
            (3, Some("/".to_string())),
        ],
        "a taken name falls back to the whole path, and so does a path with no last component"
    );
}

/// The add strips every character `libraries::trim_name` strips, each on its
/// own at both ends of a directory's name, and keeps one that is not white
/// space. A name taken in another case is taken.
#[test]
fn a_root_written_without_a_library_is_named_without_white_space_or_a_case_clash() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates");
    let add = |path: &str| {
        crate::scan::insert_root(
            &conn,
            std::path::Path::new(path),
            crate::scan::InLibrary::Own,
        )
        .unwrap_or_else(|e| panic!("a root at {path:?}: {e}"));
    };
    let name_of_root = |path: &str| -> String {
        conn.query_row(
            "SELECT l.name FROM root r JOIN library_root lr ON lr.root_id = r.id
               JOIN library l ON l.id = lr.library_id WHERE r.path = ?1",
            [path],
            |row| row.get(0),
        )
        .expect("the root has a library")
    };

    let spaces = (0..=0x10_FFFF_u32)
        .filter_map(char::from_u32)
        .filter(|c| c.is_whitespace());
    for (n, c) in spaces.enumerate() {
        let path = format!("/w{n}/{c}Leaf{n}{c}");
        add(&path);
        assert_eq!(
            name_of_root(&path),
            format!("Leaf{n}"),
            "U+{:04X}",
            u32::from(c)
        );

        // A component that is only the character is none, so the whole path
        // names the library, stripped the same way.
        let bare = format!("/b{n}/{c}");
        add(&bare);
        assert_eq!(
            name_of_root(&bare),
            format!("/b{n}/"),
            "U+{:04X}",
            u32::from(c)
        );
    }

    add("/z/\u{200B}Zero\u{200B}");
    assert_eq!(
        name_of_root("/z/\u{200B}Zero\u{200B}"),
        "\u{200B}Zero\u{200B}"
    );

    add("/x/Extras");
    add("/y/EXTRAS");
    assert_eq!(name_of_root("/y/EXTRAS"), "/y/EXTRAS");
}

/// **Migration 16 brings the names already stored under both rules.** A name
/// that is an older library's once stripped and folded keeps its id beside
/// it; every other name is stripped; one that is only white space is left.
/// `' Manga '` would collide with `'Manga'` on `library.name`'s own UNIQUE if
/// it were stripped first, so the order of the two steps is held too.
#[test]
fn migration_16_strips_names_and_keeps_an_id_beside_a_name_another_holds() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES
           (1, 'Manga'), (2, ' Manga '), (3, 'MANGA'), (4, 'Comics' || char(9)), (5, char(9)), (6, ' ');",
    )
    .expect("six libraries");

    migrate(&conn).expect("migrates to 16");

    assert_eq!(
        library_names(&conn, "SELECT id, name FROM library ORDER BY id"),
        vec![
            (1, Some("Manga".to_string())),
            (2, Some("Manga (2)".to_string())),
            (3, Some("MANGA (3)".to_string())),
            (4, Some("Comics".to_string())),
            (5, Some("\t".to_string())),
            (6, Some(" ".to_string())),
        ]
    );
    let refused = conn
        .execute("INSERT INTO library (name) VALUES ('COMICS')", [])
        .expect_err("one name in another case");
    assert!(refused.to_string().contains("UNIQUE"), "{refused}");
}

/// **Migration 17 changes no account's view.** An account holding a grant
/// row is set to "only these" and still sees its directories; one holding none
/// stays "every library". An administrator's rows count like anyone's, so a
/// demotion still restores them.
#[test]
fn migration_17_restricts_exactly_the_accounts_that_hold_a_grant() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..16]).expect("migrates to 16");
    conn.execute_batch(TWO_DIR_LIBRARY).expect("seeds");
    conn.execute_batch(
        "INSERT INTO user (id, name, password_hash, role) VALUES
           (2, 'granted', 'x', 'user'), (3, 'open', 'x', 'user'), (4, 'boss', 'x', 'admin');
         INSERT INTO user_library (user_id, library_id) VALUES (2, 70), (4, 80);",
    )
    .expect("an account with a grant, one without, and an administrator with one");

    migrate(&conn).expect("migrates to 17");

    assert_eq!(
        rows_of(
            &conn,
            "SELECT id, every_library FROM user WHERE id IN (2, 3, 4) ORDER BY id"
        ),
        vec![
            vec!["Integer(2)".to_string(), "Integer(0)".to_string()],
            vec!["Integer(3)".to_string(), "Integer(1)".to_string()],
            vec!["Integer(4)".to_string(), "Integer(0)".to_string()],
        ]
    );
    assert_eq!(
        crate::account::access_for(&conn, 2).expect("reads"),
        crate::account::Access::Only {
            roots: vec![7, 8],
            libraries: vec![70],
        },
        "the granted account still sees Shelf's two directories"
    );
    assert_eq!(
        crate::account::access_for(&conn, 3).expect("reads"),
        crate::account::Access::Every,
        "the account with no grant still sees every library"
    );
    let widened = conn
        .execute("UPDATE user SET every_library = 2 WHERE id = 3", [])
        .expect_err("the column holds 0 or 1 and nothing else");
    assert!(
        widened.to_string().contains("CHECK constraint failed"),
        "{widened}"
    );
}

#[test]
fn a_library_goes_with_its_last_root_and_takes_its_grants_with_it() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(TWO_LINKED_LIBRARIES).expect("seeds");
    conn.execute(
        "INSERT INTO user_library (user_id, library_id) VALUES (1, 70)",
        [],
    )
    .expect("grants Shelf");

    conn.execute("DELETE FROM root WHERE id = 7", [])
        .expect("removes one directory");
    let shelf = |conn: &Connection| -> (i64, i64) {
        conn.query_row(
            "SELECT (SELECT count(*) FROM library WHERE id = 70),
                    (SELECT count(*) FROM user_library WHERE library_id = 70)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reads")
    };
    assert_eq!(
        shelf(&conn),
        (1, 1),
        "a library with a directory left keeps itself and its grant"
    );

    conn.execute("DELETE FROM root WHERE id = 8", [])
        .expect("removes the last directory");
    assert_eq!(
        shelf(&conn),
        (0, 0),
        "the library and its grant went with its last directory"
    );
}

#[test]
fn a_library_goes_when_its_last_root_moves_away_and_its_works_stay() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(TWO_LINKED_LIBRARIES).expect("seeds");
    conn.execute_batch(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 's', 'T', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (10, 1, 'raw', 'Raw');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (100, 10, 0, '/srv/other/v01.cbz', 'h', 1, 9);",
    )
    .expect("a work under Other's directory");

    conn.execute(
        "UPDATE library_root SET library_id = 70 WHERE library_id = 80 AND root_id = 9",
        [],
    )
    .expect("moves Other's one directory into Shelf");

    let (libraries, works, held): (i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM library WHERE id = 80),
                    (SELECT count(*) FROM work WHERE id = 1),
                    (SELECT count(*) FROM member_root WHERE member_id = 100 AND root_id = 9)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("reads");
    assert_eq!(libraries, 0, "Other had no directory left");
    assert_eq!(works, 1, "its work is kept");
    assert_eq!(held, 1, "and still held under the directory that moved");
}

/// [`TWO_LINKED_LIBRARIES`], two accounts, and `grants` as `(user_id,
/// library_id)` rows. Each granted account is set to "only these", as
/// [`crate::account::grant_library`] sets it.
fn libraries_granted(grants: &str) -> crate::Library {
    let library = crate::Library::open_in_memory().expect("library");
    {
        let conn = library.conn();
        conn.execute_batch(TWO_LINKED_LIBRARIES).expect("seeds");
        conn.execute_batch(&format!(
            "INSERT INTO user (id, name, password_hash) VALUES (2, 'a', 'x'), (3, 'b', 'x');
             INSERT INTO user_library (user_id, library_id) VALUES {grants};
             UPDATE user SET every_library = 0 WHERE id IN (SELECT user_id FROM user_library);"
        ))
        .expect("seeds the grants");
    }
    library
}

/// **Nothing in the store keeps an only grant's last directory.** Deleting,
/// moving away or unlinking `Other`'s one directory succeeds while account 2
/// is granted `Other` and nothing else, administrator or not: the library and
/// its grant go, and the account is left seeing no library, never every
/// library. Account 3 keeps its grant of `Shelf`. An unlink needs the
/// directory linked to `Shelf` first, since a directory keeps one library.
#[test]
fn the_last_root_of_an_accounts_only_library_can_be_deleted_or_moved_away() {
    for role in ["user", "admin"] {
        for (first, statement) in [
            ("", "DELETE FROM root WHERE id = 9"),
            (
                "",
                "UPDATE library_root SET library_id = 70 WHERE library_id = 80 AND root_id = 9",
            ),
            (
                "INSERT INTO library_root (library_id, root_id) VALUES (70, 9)",
                "DELETE FROM library_root WHERE library_id = 80 AND root_id = 9",
            ),
        ] {
            let library = libraries_granted("(2, 80), (3, 70)");
            let conn = library.conn();
            conn.execute("UPDATE user SET role = ?1 WHERE id = 2", [role])
                .expect("sets account 2's role");
            conn.execute_batch(first)
                .unwrap_or_else(|e| panic!("{role}, {first}: {e}"));

            let changed = conn
                .execute(statement, [])
                .unwrap_or_else(|e| panic!("{role}, {statement}: {e}"));

            assert_eq!(changed, 1, "{role}, {statement}");
            assert_eq!(
                rows_of(
                    &conn,
                    "SELECT (SELECT count(*) FROM library WHERE id = 80),
                            (SELECT count(*) FROM user_library WHERE user_id = 2)"
                ),
                vec![vec!["Integer(0)".to_string(), "Integer(0)".to_string()]],
                "{role}, {statement}: Other and its grant went with its last directory"
            );
            assert_eq!(
                crate::account::access_for(&conn, 2).expect("reads"),
                crate::account::Access::roots(Vec::new()),
                "{role}, {statement}: the account sees no library"
            );
            assert_eq!(
                crate::account::libraries_for(&conn, 3).expect("reads"),
                vec![70],
                "{role}, {statement}: account 3 keeps its grant of Shelf"
            );
        }
    }
}

#[test]
fn a_content_key_and_a_slug_are_each_unique_across_the_store() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys");
    migrate(&conn).expect("migrates");
    let insert = |key: &str, slug: &str| {
        conn.execute(
            "INSERT INTO work (content_key, slug, title, reading, hub)
             VALUES (?1, ?2, 'T', 'rtl', 'raw')",
            rusqlite::params![key, slug],
        )
    };
    insert("same", "first").expect("the first work of this content");
    let twice = insert("same", "second").expect_err("the same content is one work");
    assert!(twice.to_string().contains("work.content_key"), "{twice}");
    let slug = insert("fresh", "first").expect_err("a slug names one work");
    assert!(slug.to_string().contains("work.slug"), "{slug}");
}

/// A store at version 17 holding two copies of one volume, filed under
/// libraries A and B, in two series of one title. Each copy holds one of
/// everything that names a work or its edition, and each series one of
/// everything that names a series, so the rows of the copy that goes stand
/// beside the rows of the one that stays. Work 3 has its own content and is
/// placed in series 2 by hand, and series 1's cover is the younger copy.
const SCHEMA_17_COPIES: &str = "
    INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');
    INSERT INTO root (id, path, library_id) VALUES (7, '/srv/a', 70), (8, '/srv/b', 80);
    INSERT INTO series (id, sort_title, library_id) VALUES (1, 'Foo', 70), (2, 'Foo', 80);
    INSERT INTO series_title (series_id, title, kind, is_primary) VALUES
      (1, 'Foo', 'english', 1), (2, 'Foo', 'english', 1), (2, 'Fuu', 'alt', 0);
    INSERT INTO tag (id, name) VALUES (1, 'drama');
    INSERT INTO series_tag (series_id, tag_id) VALUES (1, 1), (2, 1);
    INSERT INTO external_id (series_id, provider, external_id) VALUES
      (1, 'anilist', '1'), (2, 'anilist', '2');
    INSERT INTO proposal (id, kind, subject_id, target_id, score, reason) VALUES
      (1, 'same-series', 2, NULL, 1.0, 'names series 2 as its subject'),
      (2, 'edition-of', 1, 2, 1.0, 'names series 2 as its target'),
      (3, 'same-series', 1, NULL, 1.0, 'names series 1 alone');
    INSERT INTO work (id, content_key, slug, series_id, series_id_hand, title, reading, hub,
                      library_id) VALUES
      (1, 'same', 'foo-a', 1, NULL, 'Foo', 'rtl', 'raw', 70),
      (2, 'same', 'foo-b', 2, NULL, 'Foo', 'rtl', 'raw', 80),
      (3, 'own', 'foo-2', 2, 2, 'Foo 2', 'rtl', 'raw', 80);
    UPDATE series SET cover_work_id = 2 WHERE id = 1;
    INSERT INTO edition (id, work_id, key, label) VALUES (10, 1, 'raw', 'Raw'), (20, 2, 'raw', 'Raw');
    INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id) VALUES
      (100, 10, 0, '/srv/a/Foo/v01.cbz', 'h', 10, 7),
      (200, 20, 0, '/srv/b/Foo/v01.cbz', 'h', 10, 8);
    INSERT INTO page_flag (edition_id, kind, page) VALUES (10, 'spread', 3), (20, 'spread', 3);
    INSERT INTO alignment (id, work_id, from_key, to_key, by) VALUES
      (11, 1, 'raw', 'en', 'solver'), (21, 2, 'raw', 'en', 'solver');
    INSERT INTO run (alignment_id, ordinal, a_start_page, a_end_page) VALUES (11, 0, 1, 9), (21, 0, 1, 9);
    INSERT INTO truth_page (work_id, edition, page) VALUES (1, 'raw', 2), (2, 'raw', 2);
    INSERT INTO truth_box (work_id, edition, page, x, y, w, h, kind) VALUES
      (1, 'raw', 2, 0, 0, 1, 1, 'text'), (2, 'raw', 2, 0, 0, 1, 1, 'text');
    INSERT INTO truth_member (work_id, edition, ordinal, sha256) VALUES (1, 'raw', 0, 'h'), (2, 'raw', 0, 'h');
    INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, 1, 'raw', 3), (1, 2, 'raw', 5);
    INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated) VALUES
      (1, 'n1', 1, 'raw', 2, 'kept', 'now', 'now'), (1, 'n2', 2, 'raw', 2, 'gone', 'now', 'now');
    INSERT INTO work_tag (work_id, tag_id) VALUES (1, 1), (2, 1);
    INSERT INTO person (id, name) VALUES (1, 'Author');
    INSERT INTO credit (person_id, role, work_id, edition_id, series_id) VALUES
      (1, 'author', 1, NULL, NULL), (1, 'author', 2, NULL, NULL),
      (1, 'translator', NULL, 10, NULL), (1, 'translator', NULL, 20, NULL),
      (1, 'artist', NULL, NULL, 1), (1, 'artist', NULL, NULL, 2);
    INSERT INTO collection (id, name) VALUES (1, 'Shelf');
    INSERT INTO collection_item (collection_id, ordinal, work_id, series_id) VALUES
      (1, 0, 1, NULL), (1, 1, 2, NULL), (1, 2, NULL, 1), (1, 3, NULL, 2);
    INSERT INTO migrated_document (work_id, document) VALUES (1, 'library'), (2, 'library');
";

/// Every table that names a work, its edition or its alignment, with the ids
/// it names, so the copy that stays and the one that goes read side by side.
const ROWS_NAMING_A_WORK: &str = "
    SELECT 'work', group_concat(id) FROM (SELECT id FROM work ORDER BY id)
    UNION ALL SELECT 'edition of work', group_concat(work_id) FROM (SELECT work_id FROM edition ORDER BY 1)
    UNION ALL SELECT 'member of edition', group_concat(edition_id) FROM (SELECT edition_id FROM member ORDER BY 1)
    UNION ALL SELECT 'page_flag of edition', group_concat(edition_id) FROM (SELECT edition_id FROM page_flag ORDER BY 1)
    UNION ALL SELECT 'credit of edition', group_concat(edition_id) FROM (SELECT edition_id FROM credit WHERE edition_id IS NOT NULL ORDER BY 1)
    UNION ALL SELECT 'credit of work', group_concat(work_id) FROM (SELECT work_id FROM credit WHERE work_id IS NOT NULL ORDER BY 1)
    UNION ALL SELECT 'alignment of work', group_concat(work_id) FROM (SELECT work_id FROM alignment ORDER BY 1)
    UNION ALL SELECT 'run of alignment', group_concat(alignment_id) FROM (SELECT alignment_id FROM run ORDER BY 1)
    UNION ALL SELECT 'truth_page of work', group_concat(work_id) FROM (SELECT work_id FROM truth_page ORDER BY 1)
    UNION ALL SELECT 'truth_box of work', group_concat(work_id) FROM (SELECT work_id FROM truth_box ORDER BY 1)
    UNION ALL SELECT 'truth_member of work', group_concat(work_id) FROM (SELECT work_id FROM truth_member ORDER BY 1)
    UNION ALL SELECT 'progress of work', group_concat(work_id) FROM (SELECT work_id FROM progress ORDER BY 1)
    UNION ALL SELECT 'note of work', group_concat(work_id) FROM (SELECT work_id FROM note ORDER BY 1)
    UNION ALL SELECT 'work_tag of work', group_concat(work_id) FROM (SELECT work_id FROM work_tag ORDER BY 1)
    UNION ALL SELECT 'collection_item of work', group_concat(work_id) FROM (SELECT work_id FROM collection_item WHERE work_id IS NOT NULL ORDER BY 1)
    UNION ALL SELECT 'migrated_document of work', group_concat(work_id) FROM (SELECT work_id FROM migrated_document ORDER BY 1)
";

/// Every table that names a series, with the ids it names.
const ROWS_NAMING_A_SERIES: &str = "
    SELECT 'series', group_concat(id) FROM (SELECT id FROM series ORDER BY id)
    UNION ALL SELECT 'series_title of series', group_concat(series_id) FROM (SELECT series_id FROM series_title ORDER BY 1)
    UNION ALL SELECT 'series_tag of series', group_concat(series_id) FROM (SELECT series_id FROM series_tag ORDER BY 1)
    UNION ALL SELECT 'external_id of series', group_concat(series_id) FROM (SELECT series_id FROM external_id ORDER BY 1)
    UNION ALL SELECT 'credit of series', group_concat(series_id) FROM (SELECT series_id FROM credit WHERE series_id IS NOT NULL ORDER BY 1)
    UNION ALL SELECT 'collection_item of series', group_concat(series_id) FROM (SELECT series_id FROM collection_item WHERE series_id IS NOT NULL ORDER BY 1)
    UNION ALL SELECT 'proposal', group_concat(id) FROM (SELECT id FROM proposal ORDER BY id)
";

/// `sql`'s `(label, ids)` rows, an empty table reading as an empty string.
fn census(conn: &Connection, sql: &str) -> Vec<(String, String)> {
    let mut stmt = conn.prepare(sql).expect(sql);
    stmt.query_map([], |row| {
        Ok((
            row.get(0)?,
            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        ))
    })
    .expect(sql)
    .collect::<rusqlite::Result<Vec<_>>>()
    .expect(sql)
}

/// `labels` paired with `ids`, in order, for comparing with a [`census`].
fn expected(labels: &[&str], ids: &[&str]) -> Vec<(String, String)> {
    assert_eq!(labels.len(), ids.len(), "one id list per label");
    labels
        .iter()
        .zip(ids)
        .map(|(label, ids)| ((*label).to_string(), (*ids).to_string()))
        .collect()
}

const WORK_LABELS: [&str; 16] = [
    "work",
    "edition of work",
    "member of edition",
    "page_flag of edition",
    "credit of edition",
    "credit of work",
    "alignment of work",
    "run of alignment",
    "truth_page of work",
    "truth_box of work",
    "truth_member of work",
    "progress of work",
    "note of work",
    "work_tag of work",
    "collection_item of work",
    "migrated_document of work",
];

const SERIES_LABELS: [&str; 7] = [
    "series",
    "series_title of series",
    "series_tag of series",
    "external_id of series",
    "credit of series",
    "collection_item of series",
    "proposal",
];

/// [`SCHEMA_17_COPIES`] at version 17, foreign keys on as `Library::open`
/// sets them.
fn a_schema_17_store_with_copies() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..17]).expect("migrates to 17");
    conn.execute_batch(SCHEMA_17_COPIES)
        .expect("seeds two copies of one volume");
    conn
}

/// **Migration 18 keeps the oldest copy and takes the rest whole.** Of the
/// two works of one content key, work 1 stands with every row naming it, and
/// work 2 goes with every row naming it, its edition or its alignment. Of the
/// two series titled `Foo`, series 1 stands and series 2 goes with its titles,
/// tags, external ids, credits, collection place and both proposals naming it.
#[test]
fn migration_18_keeps_the_oldest_copy_of_a_work_and_of_a_series_and_takes_the_rest() {
    let conn = a_schema_17_store_with_copies();
    assert_eq!(
        census(&conn, ROWS_NAMING_A_WORK),
        expected(
            &WORK_LABELS,
            &[
                "1,2,3", "1,2", "10,20", "10,20", "10,20", "1,2", "1,2", "11,21", "1,2", "1,2",
                "1,2", "1,2", "1,2", "1,2", "1,2", "1,2",
            ]
        ),
        "the fixture: each copy holds one row of every table naming a work"
    );
    assert_eq!(
        census(&conn, ROWS_NAMING_A_SERIES),
        expected(
            &SERIES_LABELS,
            &["1,2", "1,2,2", "1,2", "1,2", "1,2", "1,2", "1,2,3"]
        ),
        "the fixture: each series holds a row of every table naming a series, series 2 two titles"
    );

    migrate(&conn).expect("migrates to 18");

    assert_eq!(
        census(&conn, ROWS_NAMING_A_WORK),
        expected(
            &WORK_LABELS,
            &[
                "1,3", "1", "10", "10", "10", "1", "1", "11", "1", "1", "1", "1", "1", "1", "1",
                "1",
            ]
        ),
        "work 1 stands with its rows, and work 2 went with every row naming it"
    );
    assert_eq!(
        census(&conn, ROWS_NAMING_A_SERIES),
        expected(&SERIES_LABELS, &["1", "1", "1", "1", "1", "1", "3"]),
        "series 1 stands with its rows, and series 2 went with every row naming it"
    );
}

/// **A work of the series that went is moved to the one kept**, by both its
/// derived and its hand placement, and a cover naming the copy that went is
/// cleared: `series.cover_work_id` has no foreign key to refuse it.
#[test]
fn migration_18_moves_works_to_the_kept_series_and_clears_a_cover_that_went() {
    let conn = a_schema_17_store_with_copies();

    migrate(&conn).expect("migrates to 18");

    assert_eq!(
        rows_of(
            &conn,
            "SELECT series_id, series_id_hand FROM work WHERE id = 3"
        ),
        vec![vec!["Integer(1)".to_string(), "Integer(1)".to_string()]],
        "work 3 is in series 1, by derivation and by hand"
    );
    assert_eq!(
        rows_of(&conn, "SELECT cover_work_id FROM series WHERE id = 1"),
        vec![vec!["Null".to_string()]],
        "series 1's cover named the copy that went"
    );
}

/// **The store migration 18 leaves opens with its constraints met.** No
/// foreign key is left dangling, the search index holds the kept works and
/// agrees with `work`, and a second work of a kept content key is refused.
#[test]
fn migration_18_leaves_no_orphan_a_search_index_of_the_kept_works_and_one_work_per_content() {
    let conn = a_schema_17_store_with_copies();

    migrate(&conn).expect("migrates to 18");

    assert_eq!(
        rows_of(&conn, "SELECT * FROM pragma_foreign_key_check"),
        Vec::<Vec<String>>::new(),
        "no row names one that went"
    );
    let matching = {
        let mut stmt = conn
            .prepare("SELECT rowid FROM work_fts WHERE work_fts MATCH 'Foo' ORDER BY rowid")
            .expect("prepares");
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .expect("searches")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("reads")
    };
    assert_eq!(
        matching,
        vec![1, 3],
        "the search index holds the kept works"
    );
    conn.execute(
        "INSERT INTO work_fts (work_fts) VALUES ('integrity-check')",
        [],
    )
    .expect("the search index agrees with work");

    let twice = conn
        .execute(
            "INSERT INTO work (content_key, slug, title, reading, hub)
             VALUES ('same', 'foo-again', 'Foo', 'rtl', 'raw')",
            [],
        )
        .expect_err("a second work of work 1's content");
    assert!(twice.to_string().contains("work.content_key"), "{twice}");
}

/// Every index and trigger `work` had at version 17 is back after 18's
/// rebuild, word for word, as at version 15.
#[test]
fn migration_18_recreates_every_index_and_trigger_work_had() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate_with(&conn, &MIGRATIONS[..17]).expect("migrates to 17");
    let objects = "SELECT type, name, sql FROM sqlite_master \
                   WHERE tbl_name = 'work' AND type IN ('index', 'trigger') AND sql IS NOT NULL \
                   ORDER BY name";
    let before = rows_of(&conn, objects);
    assert_eq!(before.len(), 7, "the index and six triggers: {before:?}");

    migrate(&conn).expect("migrates to 18");

    assert_eq!(
        rows_of(&conn, objects),
        before,
        "the rebuild lost or changed an index or trigger on work"
    );
}

/// The `(member_id, root_id)` rows of `member_root`.
const MEMBER_ROOTS: &str = "SELECT member_id, root_id FROM member_root ORDER BY member_id, root_id";

/// **A member's directories are a set that holds its `root_id`.** Migration
/// 18 fills it from the members it finds, a member under no directory holding
/// none; afterwards an insert adds the directory and a move replaces it. A
/// removed directory takes its rows and leaves the member, including a row no
/// `root_id` names, which only the cascade reaches; a deleted member takes its
/// rows.
#[test]
fn member_root_holds_each_members_directory_whoever_writes_it() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..17]).expect("migrates to 17");
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES (70, 'A');
         INSERT INTO root (id, path, library_id) VALUES (7, '/srv/a', 70), (8, '/srv/b', 70);
         INSERT INTO work (id, content_key, slug, title, reading, hub) VALUES (1, 'k', 's', 'T', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (10, 1, 'raw', 'Raw');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id) VALUES
           (100, 10, 0, '/srv/a/v01.cbz', 'a', 1, 7),
           (101, 10, 1, '/docs/v02.cbz', 'b', 1, NULL);",
    )
    .expect("a member under a directory and one under none");

    migrate(&conn).expect("migrates to 18");
    assert_eq!(
        rows_of(&conn, MEMBER_ROOTS),
        vec![vec!["Integer(100)".to_string(), "Integer(7)".to_string()]],
        "the migration filled the set from member.root_id"
    );

    conn.execute(
        "INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (102, 10, 2, '/srv/b/v03.cbz', 'c', 1, 8)",
        [],
    )
    .expect("inserts a member under /srv/b");
    conn.execute(
        "INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (103, 10, 3, '/docs/v04.cbz', 'd', 1, NULL)",
        [],
    )
    .expect("inserts a member under no directory");
    assert_eq!(
        rows_of(&conn, MEMBER_ROOTS),
        vec![
            vec!["Integer(100)".to_string(), "Integer(7)".to_string()],
            vec!["Integer(102)".to_string(), "Integer(8)".to_string()],
        ],
        "an inserted member's directory joins the set, and none joins for none"
    );

    conn.execute("UPDATE member SET root_id = 7 WHERE id = 102", [])
        .expect("moves member 102 to /srv/a");
    assert_eq!(
        rows_of(&conn, MEMBER_ROOTS),
        vec![
            vec!["Integer(100)".to_string(), "Integer(7)".to_string()],
            vec!["Integer(102)".to_string(), "Integer(7)".to_string()],
        ],
        "the new directory replaced the old one"
    );

    conn.execute(
        "INSERT INTO member_root (member_id, root_id) VALUES (100, 8)",
        [],
    )
    .expect("a second copy of member 100, under /srv/b");
    conn.execute("DELETE FROM root WHERE id = 8", [])
        .expect("removes /srv/b");
    assert_eq!(
        rows_of(&conn, MEMBER_ROOTS),
        vec![
            vec!["Integer(100)".to_string(), "Integer(7)".to_string()],
            vec!["Integer(102)".to_string(), "Integer(7)".to_string()],
        ],
        "the removed directory holds nothing, though no member's root_id named it"
    );

    conn.execute("DELETE FROM member WHERE id = 102", [])
        .expect("deletes member 102");
    assert_eq!(
        rows_of(&conn, MEMBER_ROOTS),
        vec![vec!["Integer(100)".to_string(), "Integer(7)".to_string()]],
        "a deleted member's directories went with it"
    );

    conn.execute("DELETE FROM root WHERE id = 7", [])
        .expect("removes /srv/a");
    assert_eq!(
        rows_of(&conn, MEMBER_ROOTS),
        Vec::<Vec<String>>::new(),
        "the removed directory holds nothing"
    );
    assert_eq!(
        rows_of(&conn, "SELECT count(*) FROM member"),
        vec![vec!["Integer(3)".to_string()]],
        "and no member went with a directory"
    );
}

/// `table`'s column names.
fn columns_of(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")
        .expect("prepares");
    stmt.query_map([table], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("reads")
}

/// **A library no longer decides what a work or a series is**: in a fresh
/// store neither table has a `library_id` to file it by.
#[test]
fn a_fresh_store_files_no_work_or_series_under_a_library() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates");
    assert_eq!(version(&conn).expect("reads the version"), SCHEMA_VERSION);
    for table in ["work", "series"] {
        let columns = columns_of(&conn, table);
        assert!(!columns.is_empty(), "{table} exists");
        assert!(
            !columns.iter().any(|c| c == "library_id"),
            "{table} has no library: {columns:?}"
        );
    }
}

/// A store at version 18: Shelf holding two directories and Other one, two
/// accounts limited to their grants, and a reading position under Other's
/// directory.
const SCHEMA_18_LIBRARIES: &str = "
    INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
    INSERT INTO root (id, path, library_id) VALUES
      (7, '/srv/manga', 70), (8, '/srv/raw', 70), (9, '/srv/other', 80);
    INSERT INTO user (id, name, password_hash, every_library) VALUES
      (2, 'a', 'x', 0), (3, 'b', 'x', 0);
    INSERT INTO user_library (user_id, library_id) VALUES (2, 80), (3, 70), (3, 80);
    INSERT INTO work (id, content_key, slug, title, reading, hub)
    VALUES (1, 'k', 's', 'T', 'rtl', 'raw');
    INSERT INTO edition (id, work_id, key, label) VALUES (10, 1, 'raw', 'Raw');
    INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
    VALUES (100, 10, 0, '/srv/other/v01.cbz', 'h', 1, 9);
    INSERT INTO progress (user_id, work_id, edition, slot) VALUES (2, 1, 'raw', 3);
";

/// Every link, as `(library, directory)`.
const LINKS: &str = "SELECT library_id, root_id FROM library_root ORDER BY library_id, root_id";

/// `rows_of`'s rendering of `(library, directory)` pairs.
fn links(pairs: &[(i64, i64)]) -> Vec<Vec<String>> {
    pairs
        .iter()
        .map(|(library, root)| vec![format!("Integer({library})"), format!("Integer({root})")])
        .collect()
}

/// **Migration 19 makes a directory's library a link**: one `library_root`
/// row for each value `root.library_id` held, and the column gone with the
/// three triggers that wrote or read it. The triggers left on the link are
/// the three that keep a directory in a library and a library in a
/// directory. Grants and positions are as they were.
#[test]
fn migration_19_links_each_directory_to_its_library_and_drops_the_column() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..18]).expect("migrates to 18");
    conn.execute_batch(SCHEMA_18_LIBRARIES)
        .expect("seeds a schema-18 store");
    let grants = "SELECT user_id, library_id FROM user_library ORDER BY user_id, library_id";
    let positions =
        "SELECT user_id, work_id, edition, slot FROM progress ORDER BY user_id, work_id";
    let (grants_before, positions_before) = (rows_of(&conn, grants), rows_of(&conn, positions));

    migrate_with(&conn, &MIGRATIONS[..19]).expect("migrates to 19");

    assert_eq!(version(&conn).expect("reads the version"), 19);
    assert_eq!(
        rows_of(&conn, LINKS),
        links(&[(70, 7), (70, 8), (80, 9)]),
        "one link per directory, to the library its column named"
    );
    let columns = columns_of(&conn, "root");
    assert!(
        columns.iter().any(|c| c == "path") && !columns.iter().any(|c| c == "library_id"),
        "root keeps its row and loses the column: {columns:?}"
    );
    assert_eq!(
        rows_of(&conn, grants),
        grants_before,
        "the grants are as they were"
    );
    assert_eq!(
        rows_of(&conn, positions),
        positions_before,
        "the positions are as they were"
    );
    assert_eq!(
        rows_of(
            &conn,
            "SELECT name, tbl_name FROM sqlite_master
              WHERE type = 'trigger' AND (tbl_name IN ('root', 'library_root')
                    OR name IN ('root_gets_a_library', 'library_emptied_by_delete'))
              ORDER BY name"
        ),
        [
            ("library_emptied_by_move", "library_root"),
            ("library_emptied_by_unlink", "library_root"),
            ("root_keeps_a_library", "library_root"),
        ]
        .iter()
        .map(|(name, table)| vec![format!("Text({name:?})"), format!("Text({table:?})")])
        .collect::<Vec<_>>(),
        "no trigger is left on root, and the link has the three"
    );
}

/// A store at version 20 with one reading position and one note, every
/// column given a value so a column the migration disturbed would show.
const SCHEMA_20_POSITION_AND_NOTE: &str = "
    INSERT INTO user (id, name, password_hash, every_library) VALUES (2, 'a', 'x', 1);
    INSERT INTO work (id, content_key, slug, title, reading, hub)
    VALUES (1, 'k', 's', 'T', 'rtl', 'raw');
    INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at, read_count)
    VALUES (2, 1, 'raw', 3, '2026-11-01 10:00:00', '2026-11-02 11:00:00', 2);
    INSERT INTO note (user_id, id, work_id, edition, slot, rx, ry, rw, rh, color, body,
                      created, updated)
    VALUES (2, 'n1', 1, 'raw', 4, 0.1, 0.2, 0.3, 0.4, 'yellow', 'a body',
            '2026-11-01T10:00:00Z', '2026-11-01T10:05:00Z');
";

/// **Migration 21 gives a reading position and a note a page beside the
/// slot** (O244): four columns at the end of `progress` and of `note`, NULL
/// on the rows already there, which read otherwise as they were. ADD COLUMN
/// rebuilds neither table, so `note_by_work` stands, and `at_half` takes
/// only the two halves.
#[test]
fn migration_21_gives_progress_and_note_a_page_beside_the_slot() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..20]).expect("migrates to 20");
    conn.execute_batch(SCHEMA_20_POSITION_AND_NOTE)
        .expect("seeds a schema-20 store");
    let before: Vec<_> = ["progress", "note"]
        .iter()
        .map(|table| rows_of(&conn, &format!("SELECT * FROM {table}")))
        .collect();

    migrate(&conn).expect("migrates to 21");

    assert_eq!(version(&conn).expect("reads the version"), 21);
    for (table, before) in ["progress", "note"].iter().zip(before) {
        let columns = columns_of(&conn, table);
        assert_eq!(
            columns[columns.len().saturating_sub(4)..],
            ["at_sha256", "at_nth", "at_page", "at_half"],
            "{table} ends with the page's four columns: {columns:?}"
        );
        let with_no_page: Vec<Vec<String>> = before
            .into_iter()
            .map(|mut row| {
                row.extend(std::iter::repeat_n("Null".to_string(), 4));
                row
            })
            .collect();
        assert_eq!(
            rows_of(&conn, &format!("SELECT * FROM {table}")),
            with_no_page,
            "{table}'s row is as it was, naming no page"
        );
        let refused = conn
            .execute(&format!("UPDATE {table} SET at_half = 'middle'"), [])
            .expect_err(&format!("{table} refuses a half that is not one"));
        assert!(
            refused.to_string().contains("CHECK constraint failed"),
            "{table} refuses it by its CHECK: {refused}"
        );
        assert_eq!(
            conn.execute(&format!("UPDATE {table} SET at_half = 'left'"), []),
            Ok(1),
            "{table} takes a half"
        );
        assert_eq!(
            conn.execute(&format!("UPDATE {table} SET at_half = 'right'"), []),
            Ok(1),
            "{table} takes the other half"
        );
    }
    assert_eq!(
        rows_of(
            &conn,
            "SELECT tbl_name FROM sqlite_master WHERE type = 'index' AND name = 'note_by_work'"
        ),
        vec![vec!["Text(\"note\")".to_string()]],
        "note_by_work stands"
    );
}

/// [`TWO_DIR_LIBRARY`] at the current version: Shelf holding `/srv/manga` and
/// `/srv/raw`, and Other holding `/srv/other`, each through its link.
const TWO_LINKED_LIBRARIES: &str = "
    INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
    INSERT INTO root (id, path) VALUES (7, '/srv/manga'), (8, '/srv/raw'), (9, '/srv/other');
    INSERT INTO library_root (library_id, root_id) VALUES (70, 7), (70, 8), (80, 9);
";

/// [`TWO_LINKED_LIBRARIES`] with `/srv/other` linked to Shelf as well, and two
/// accounts: 2 granted Other alone, 3 granted Shelf alone.
fn other_shared_with_shelf() -> crate::Library {
    let library = crate::Library::open_in_memory().expect("library");
    {
        let conn = library.conn();
        conn.execute_batch(TWO_LINKED_LIBRARIES).expect("seeds");
        conn.execute_batch(
            "INSERT INTO library_root (library_id, root_id) VALUES (70, 9);
             INSERT INTO user (id, name, password_hash, every_library) VALUES
               (2, 'a', 'x', 0), (3, 'b', 'x', 0);
             INSERT INTO user_library (user_id, library_id) VALUES (2, 80), (3, 70);",
        )
        .expect("links /srv/other to Shelf and grants each library");
    }
    library
}

/// `(libraries, grants)` of library `id`: 1 and its grant count while it
/// stands, 0 and 0 once it has gone.
fn library_and_grants(conn: &Connection, id: i64) -> (i64, i64) {
    conn.query_row(
        "SELECT (SELECT count(*) FROM library WHERE id = ?1),
                (SELECT count(*) FROM user_library WHERE library_id = ?1)",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .expect("reads")
}

/// **A directory keeps at least one library**: its last link cannot be
/// deleted while the directory stands, whether its library has another
/// directory (`/srv/manga`) or none (`/srv/other`).
#[test]
fn a_directorys_last_link_cannot_be_unlinked() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(TWO_LINKED_LIBRARIES).expect("seeds");

    for statement in [
        "DELETE FROM library_root WHERE library_id = 80 AND root_id = 9",
        "DELETE FROM library_root WHERE library_id = 70 AND root_id = 7",
    ] {
        let refused = conn.execute(statement, []).expect_err(statement);
        assert!(
            refused
                .to_string()
                .contains("a directory keeps at least one library"),
            "{statement}: {refused}"
        );
    }
    assert_eq!(
        rows_of(&conn, LINKS),
        links(&[(70, 7), (70, 8), (80, 9)]),
        "every link stands"
    );
}

/// A directory linked to a second library can be unlinked from the first,
/// and the first, left with no directory, goes with its grants. The directory
/// stays, in the second.
#[test]
fn a_directory_linked_elsewhere_is_unlinked_and_takes_an_emptied_library_with_it() {
    let library = other_shared_with_shelf();
    let conn = library.conn();

    conn.execute(
        "DELETE FROM library_root WHERE library_id = 80 AND root_id = 9",
        [],
    )
    .expect("unlinks /srv/other from Other");

    assert_eq!(
        library_and_grants(&conn, 80),
        (0, 0),
        "Other went, and its grant"
    );
    assert_eq!(
        library_and_grants(&conn, 70),
        (1, 1),
        "Shelf stays, and its grant"
    );
    assert_eq!(
        rows_of(&conn, LINKS),
        links(&[(70, 7), (70, 8), (70, 9)]),
        "/srv/other is Shelf's"
    );
}

/// **A library cannot be deleted from under a directory it alone holds**: the
/// link the delete cascades to is that directory's last.
#[test]
fn a_library_cannot_be_deleted_while_it_alone_holds_a_directory() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(TWO_LINKED_LIBRARIES).expect("seeds");

    let refused = conn
        .execute("DELETE FROM library WHERE id = 80", [])
        .expect_err("Other alone holds /srv/other");

    assert!(
        refused
            .to_string()
            .contains("a directory keeps at least one library"),
        "{refused}"
    );
    assert_eq!(
        rows_of(&conn, LINKS),
        links(&[(70, 7), (70, 8), (80, 9)]),
        "Other and its link stand"
    );
}

/// A library whose every directory another library also holds can be
/// deleted, and its directories stay in the other.
#[test]
fn a_library_whose_directories_are_all_linked_elsewhere_is_deleted_and_they_stay() {
    let library = other_shared_with_shelf();
    let conn = library.conn();

    conn.execute("DELETE FROM library WHERE id = 80", [])
        .expect("deletes Other");

    assert_eq!(
        library_and_grants(&conn, 80),
        (0, 0),
        "Other went, and its grant"
    );
    assert_eq!(
        rows_of(&conn, LINKS),
        links(&[(70, 7), (70, 8), (70, 9)]),
        "/srv/other stays, in Shelf"
    );
    assert_eq!(
        rows_of(&conn, "SELECT count(*) FROM root WHERE id = 9"),
        vec![vec!["Integer(1)".to_string()]],
        "and the directory's row stands"
    );
}

/// Deleting a directory two libraries hold, the last of one of them, takes
/// that library with its grants and leaves the other with its own.
#[test]
fn a_directory_shared_by_two_libraries_takes_the_one_it_was_the_last_of() {
    let library = other_shared_with_shelf();
    let conn = library.conn();

    conn.execute("DELETE FROM root WHERE id = 9", [])
        .expect("deletes /srv/other");

    assert_eq!(
        library_and_grants(&conn, 80),
        (0, 0),
        "Other went, and its grant"
    );
    assert_eq!(
        library_and_grants(&conn, 70),
        (1, 1),
        "Shelf stays, and its grant"
    );
    assert_eq!(
        rows_of(&conn, LINKS),
        links(&[(70, 7), (70, 8)]),
        "Shelf keeps its own two"
    );
}
