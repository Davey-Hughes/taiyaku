//! Migration 14's triggers: one case per table and statement kind, and the
//! two per-reader writes that must not move the counter.

use rusqlite::Connection;

use super::version;

/// One row in each of the six tables `project::sidecar` reads, each
/// satisfying the foreign keys of the rows before it.
const SEED: &str = "
    INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k1', 's1', 'T', 'rtl', 'raw');
    INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
    INSERT INTO member (id, edition_id, ordinal, path, sha256, pages)
         VALUES (1, 1, 0, '/x/a.cbz', 'aa', 10);
    INSERT INTO page_flag (edition_id, kind, page) VALUES (1, 'spread', 3);
    INSERT INTO alignment (id, work_id, from_key, to_key, by)
         VALUES (1, 1, 'raw', 'en', 'solver');
    INSERT INTO run (id, alignment_id, ordinal, a_start_page, a_end_page)
         VALUES (1, 1, 0, 0, 9);
";

/// Per table, an insert, an update and a delete that each change exactly one
/// row of [`SEED`]'s database.
const CASES: [(&str, [&str; 3]); 6] = [
    (
        "work",
        [
            "INSERT INTO work (id, content_key, slug, title, reading, hub) \
             VALUES (2, 'k2', 's2', 'U', 'ltr', 'raw')",
            "UPDATE work SET title = 'Retitled' WHERE id = 1",
            "DELETE FROM work WHERE id = 1",
        ],
    ),
    (
        "edition",
        [
            "INSERT INTO edition (id, work_id, key, label) VALUES (2, 1, 'en', 'English')",
            "UPDATE edition SET label = 'Relabelled' WHERE id = 1",
            "DELETE FROM edition WHERE id = 1",
        ],
    ),
    (
        "member",
        [
            "INSERT INTO member (id, edition_id, ordinal, path, sha256, pages) \
             VALUES (2, 1, 1, '/x/b.cbz', 'bb', 12)",
            "UPDATE member SET pages = 11 WHERE id = 1",
            "DELETE FROM member WHERE id = 1",
        ],
    ),
    (
        "page_flag",
        [
            "INSERT INTO page_flag (edition_id, kind, page) VALUES (1, 'credit', 9)",
            "UPDATE page_flag SET page = 4 WHERE edition_id = 1 AND kind = 'spread'",
            "DELETE FROM page_flag WHERE edition_id = 1",
        ],
    ),
    (
        "alignment",
        [
            "INSERT INTO alignment (id, work_id, from_key, to_key, by) \
             VALUES (2, 1, 'raw', 'fan', 'hand')",
            "UPDATE alignment SET verdict = 'sound' WHERE id = 1",
            "DELETE FROM alignment WHERE id = 1",
        ],
    ),
    (
        "run",
        [
            "INSERT INTO run (id, alignment_id, ordinal, a_start_page, a_end_page) \
             VALUES (2, 1, 1, 10, 19)",
            "UPDATE run SET margin = 0.5 WHERE id = 1",
            "DELETE FROM run WHERE id = 1",
        ],
    ),
];

/// A migrated database holding [`SEED`]. A bare connection, so foreign keys
/// are off and a delete removes exactly the row it names, with no cascade
/// moving the counter on its behalf.
fn seeded() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    crate::schema::migrate(&conn).expect("migrates");
    conn.execute_batch(SEED).expect("seeds one row per table");
    conn
}

/// **Every statement kind on every table a served work is built from moves
/// the counter.** One fresh database per case, so no case can pass on a
/// movement an earlier one caused, and every miss is listed at once rather
/// than the first one only.
#[test]
fn every_write_to_a_table_a_work_is_built_from_moves_the_counter() {
    let mut unmoved = Vec::new();
    for (table, statements) in CASES {
        for (kind, sql) in ["insert", "update", "delete"].into_iter().zip(statements) {
            let conn = seeded();
            let before = version(&conn).expect("reads the counter");
            let changed = conn.execute(sql, []).expect(sql);
            assert_eq!(
                changed, 1,
                "`{sql}` must change exactly one row, or the case tests nothing"
            );
            if version(&conn).expect("reads the counter") <= before {
                unmoved.push(format!("{kind} on {table}"));
            }
        }
    }
    assert!(
        unmoved.is_empty(),
        "the counter did not move for: {unmoved:?}"
    );
}

/// **A reading position or a note does not move it.** Both are written on a
/// page turn or a keystroke, and a counter that moved for them would make a
/// server re-read every work of the library over and over while somebody
/// read.
#[test]
fn a_reading_position_or_a_note_does_not_move_the_counter() {
    let conn = seeded();
    let before = version(&conn).expect("reads the counter");
    for sql in [
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, 1, 'raw', 5)",
        "UPDATE progress SET slot = 6 WHERE user_id = 1 AND work_id = 1",
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated) \
         VALUES (1, 'n1', 1, 'raw', 2, 'hello', 'now', 'now')",
        "UPDATE note SET body = 'edited' WHERE id = 'n1'",
        "DELETE FROM note WHERE id = 'n1'",
        "DELETE FROM progress WHERE user_id = 1",
    ] {
        assert_eq!(
            conn.execute(sql, []).expect(sql),
            1,
            "`{sql}` must change one row"
        );
    }
    assert_eq!(version(&conn).expect("reads the counter"), before);
}
