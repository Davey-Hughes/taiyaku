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
/// Reproduced by review as a real database that could never be opened again.
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

/// Each migration commits on its own, so the version names exactly what is on
/// disk: migration 1's tables stay, migration 2's do not, and the version sits
/// at 1 rather than at 0 or 2.
#[test]
fn a_later_migration_failing_leaves_the_earlier_one_committed() {
    let conn = Connection::open_in_memory().expect("in-memory connection");

    let refused = migrate_with(
        &conn,
        &[
            "CREATE TABLE first (x);",
            "CREATE TABLE second (x); CREATE TABLE second (x);",
        ],
    )
    .expect_err("migration 2 must fail");
    assert!(
        refused.contains("applying schema version 2"),
        "the message names the version that failed: {refused}"
    );

    let names = table_names(&conn);
    assert!(
        names.contains(&"first".to_string()),
        "migration 1 committed and must survive: {names:?}"
    );
    assert!(
        !names.contains(&"second".to_string()),
        "migration 2 rolled back whole: {names:?}"
    );

    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .expect("the version row");
    assert_eq!(
        version, "1",
        "the version names the last migration that committed"
    );
}

/// Two connections onto one real file, the way two processes reach one
/// `$XDG_DATA_HOME/taiyaku/library.db`. Plain `rusqlite` connections rather
/// than two `Library`s, because what is being reproduced happens *inside*
/// `Library::open`.
fn two_processes(label: &str) -> (crate::tests::TempDir, Connection, Connection) {
    let dir = crate::tests::tempdir(label);
    let db = dir.path.join("library.db");
    let first = Connection::open(&db).expect("the first process opens the file");
    let second = Connection::open(&db).expect("the second process opens the same file");
    (dir, first, second)
}

/// **Whole-branch review 3, I1.** Two processes opening the same *fresh*
/// library both read version 0 before either has a transaction open — `serve`
/// and `taiyaku library import` default to the same file, so starting them
/// together is ordinary rather than exotic — and both then decide to apply
/// migration 1. The second waits on the write lock, gets it once the first has
/// committed, and used to run `CREATE TABLE series` over a table that now
/// exists:
///
/// ```text
/// taiyaku: opening the library: applying schema version 1: table series already exists
/// ```
///
/// That is the exact message this module's own doc names as the unrecoverable
/// state it exists to prevent, produced from a different cause, at 14 failures
/// in 48 concurrent first opens. A reader who hits it has every reason to
/// believe their new library is beyond saving.
///
/// The interleaving is written out by hand rather than raced: the second
/// process reads the version, the first migrates, and only then does the
/// second act on what it read. `661cab2` made each migration atomic and that
/// does not close this — the decision was taken before the transaction began.
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

/// **The FTS index must survive a rename, and nothing tested that.** An
/// earlier draft of this schema gave `work` all three lifecycle triggers and
/// `series_title` only two, so renaming a title left the index matching the old
/// name and missing the new one — a wrong search result, with no error
/// anywhere. Both tables are asserted here because the asymmetry is precisely
/// what went unnoticed.
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
