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
/// **Nothing tested any scanner constraint**; the test above checks table
/// names and a rule count. Widening this key — to include `size`, say —
/// leaves every test in the workspace green, because `scan.rs` never relies
/// on it: `record` chooses `INSERT` or `UPDATE` from `held_for`'s own
/// `(root_id, rel_path)` lookup, so a file that merely changed size takes
/// the `UPDATE` branch either way and no duplicate is ever offered to
/// SQLite. That makes this an invariant nothing in the crate currently
/// enforces from the other side — which is a reason to pin it here, not a
/// reason it does not matter: it is what a second writer, or a future upsert
/// written as `ON CONFLICT`, would be relying on, and it is the constraint
/// that makes `forget_unseen`'s `held` map a complete picture of a root.
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
/// migration inside one. Measured on 2026-08-18 against a two-table
/// reproduction of this pair: notes surviving, 0. See `docs/notes.md`.
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
    migrate(&conn).expect("migrates");
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
/// Migration 1 wrote `DEFAULT 'admin'` when there was one account and no login,
/// and migration 4's rebuild carried it forward — so an insert omitting the
/// column minted an administrator, and a typo stored a role that is neither,
/// which `Principal::from_account`'s `== "admin"` reads as a reader without
/// saying so.
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
