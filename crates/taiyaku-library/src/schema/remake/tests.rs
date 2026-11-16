use rusqlite::Connection;

use crate::schema::tests::rows_of;
use crate::schema::{MIGRATIONS, migrate, migrate_with, version};

/// A store at version 22, the last before the re-make, holding `rows`.
fn planted(rows: &str) -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..22]).expect("migrates to 22");
    conn.execute_batch(rows).expect("plants a schema-22 store");
    conn
}

fn slugs(conn: &Connection) -> Vec<(i64, String)> {
    let mut stmt = conn
        .prepare("SELECT id, slug FROM work ORDER BY id")
        .expect("reading the slugs");
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("reading the slugs")
        .collect::<rusqlite::Result<_>>()
        .expect("reading the slugs")
}

/// Each work's `(id, title, title_hand, title_derived)`.
fn titles(conn: &Connection) -> Vec<(i64, String, Option<String>, Option<String>)> {
    let mut stmt = conn
        .prepare("SELECT id, title, title_hand, title_derived FROM work ORDER BY id")
        .expect("reading the titles");
    stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })
    .expect("reading the titles")
    .collect::<rusqlite::Result<_>>()
    .expect("reading the titles")
}

/// The works `work_fts` finds for `query`, by id.
fn found(conn: &Connection, query: &str) -> Vec<i64> {
    let mut stmt = conn
        .prepare("SELECT rowid FROM work_fts WHERE work_fts MATCH ?1 ORDER BY rowid")
        .expect("searching work_fts");
    stmt.query_map([query], |row| row.get(0))
        .expect("searching work_fts")
        .collect::<rusqlite::Result<_>>()
        .expect("searching work_fts")
}

/// Every reading position and note, whole rows, in a fixed order.
fn positions_and_notes(conn: &Connection) -> Vec<Vec<Vec<String>>> {
    vec![
        rows_of(
            conn,
            "SELECT * FROM progress ORDER BY user_id, work_id, edition",
        ),
        rows_of(conn, "SELECT * FROM note ORDER BY id"),
    ]
}

fn slug_pairs(pairs: &[(i64, &str)]) -> Vec<(i64, String)> {
    pairs
        .iter()
        .map(|(id, slug)| (*id, (*slug).to_string()))
        .collect()
}

/// Four books under the old slug rule, one reader's positions on two of them
/// and two notes on a third. Every position and note field is set and differs
/// between rows, so a pass that wrote any of them would show.
const SHELF: &str = "
    INSERT INTO user (id, name, password_hash, every_library) VALUES (2, 'reader', 'x', 1);
    INSERT INTO series (id, sort_title) VALUES (1, 'berserk sort'), (2, 'One Piece'), (3, 'X');
    INSERT INTO series_title (series_id, title, kind, is_primary) VALUES (1, 'Berserk', 'romaji', 1);
    INSERT INTO work (id, content_key, slug, series_id, volume, title, title_derived, reading, hub)
    VALUES (1, 'k1', 'berserk----v01--2003--cbz', 1, 1, 'Berserk - v01 (2003).cbz',
            'Berserk - v01 (2003).cbz', 'rtl', 'raw'),
           (2, 'k2', 'one-piece---v085--2018--cbz', 2, 85, 'One Piece - v085 (2018).cbz',
            'One Piece - v085 (2018).cbz', 'rtl', 'raw'),
           (3, 'k3', 'x-old', 3, 10.5, 'X 10.5', 'X 10.5', 'rtl', 'raw'),
           (4, 'k4', 'look-back--2022--cbz', NULL, NULL, 'Look Back (2022).cbz',
            'Look Back (2022).cbz', 'rtl', 'raw');
    INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at, read_count,
                          at_sha256, at_nth, at_page, at_half)
    VALUES (2, 1, 'raw', 3, '2026-11-01 10:00:00', '2026-11-02 11:00:00', 2, 'aa11', 1, 4, 'left'),
           (2, 4, 'en', 7, '2026-11-03 12:00:00', '2026-11-04 13:00:00', 5, 'bb22', 2, 8, 'right');
    INSERT INTO note (user_id, id, work_id, edition, slot, rx, ry, rw, rh, color, body,
                      created, updated, at_sha256, at_nth, at_page, at_half)
    VALUES (2, 'n1', 2, 'raw', 3, 0.1, 0.2, 0.3, 0.4, 'yellow', 'first body',
            '2026-11-01T10:00:00Z', '2026-11-01T10:05:00Z', 'cc33', 3, 5, 'left'),
           (2, 'n2', 2, 'en', 7, 0.5, 0.6, 0.7, 0.8, 'blue', 'second body',
            '2026-11-05T10:00:00Z', '2026-11-05T10:05:00Z', 'dd44', 4, 9, 'right');
";

/// O282: a book with a series and a volume is re-made to `<series>-v<volume>`
/// under the name the Series page shows, and one without both keeps the
/// title rule. Positions and notes key on `work_id`, so not one of them moves.
#[test]
fn the_remake_gives_each_work_its_series_and_volume_and_moves_no_position_or_note() {
    let conn = planted(SHELF);
    let before = positions_and_notes(&conn);
    assert!(
        before.iter().all(|rows| rows.len() == 2),
        "the store holds two positions and two notes: {before:?}"
    );

    // Only through the re-make: a later migration may add a column to these
    // rows, which is not the re-make moving them.
    migrate_with(&conn, &MIGRATIONS[..23]).expect("the re-make finishes");

    assert_eq!(
        slugs(&conn),
        slug_pairs(&[
            (1, "berserk-v01"),
            (2, "one-piece-v85"),
            (3, "x-v10-5"),
            (4, "look-back-2022"),
        ])
    );
    assert_eq!(
        positions_and_notes(&conn),
        before,
        "every position and note is as it was, row for row"
    );
    assert_eq!(version(&conn).expect("reads the version"), 23);
}

/// A slug the re-make gives may be one another work holds until its own turn:
/// work 10 holds `foo-v01` and work 9 is due it. Work 9 sorts between the
/// others by old slug and by title, and the parked slugs sort `~10` < `~11` <
/// `~9`, so neither direction of any of the three gives these slugs, and
/// neither does `id DESC` or a mint without the park.
#[test]
fn a_new_slug_may_be_another_works_old_one() {
    let conn = planted(
        "INSERT INTO series (id, sort_title) VALUES (1, 'Foo');
         INSERT INTO work (id, content_key, slug, series_id, volume, title, title_derived,
                           reading, hub)
         VALUES (9, 'k9', 'ggg', 1, 1, 'Gg', 'Gg', 'rtl', 'raw'),
                (10, 'k10', 'foo-v01', NULL, NULL, 'Foo v01', 'Foo v01', 'rtl', 'raw'),
                (11, 'k11', 'zzz', 1, 1, 'Zz', 'Zz', 'rtl', 'raw');",
    );

    migrate(&conn).expect("the re-make finishes over a slug another work holds");

    assert_eq!(
        slugs(&conn),
        slug_pairs(&[(9, "foo-v01"), (10, "foo-v01-2"), (11, "foo-v01-3")]),
        "the earliest-added work takes the plain slug"
    );
}

/// The re-make is a migration step, so a store already re-made is not re-made
/// again: a book renumbered afterwards keeps its slug.
#[test]
fn the_remake_runs_once() {
    let conn = planted(SHELF);
    migrate(&conn).expect("the re-make finishes");
    assert_eq!(slugs(&conn)[0], (1, "berserk-v01".to_string()));

    conn.execute("UPDATE work SET volume = 2 WHERE id = 1", [])
        .expect("renumbers Berserk v01 by hand");
    migrate(&conn).expect("the next open");

    assert_eq!(
        slugs(&conn)[0],
        (1, "berserk-v01".to_string()),
        "the next open re-makes nothing"
    );
}

/// O284: a derived title drops one archive extension, a document's included;
/// a hand title shows as typed. The title's search index follows it. A name
/// that is only an extension keeps it, and a row with no derived title is
/// passed over. The titles go before the mint, so `Two.cbz.cbz`'s slug is
/// `two`, from the title it shows, and not `two-cbz`.
#[test]
fn a_derived_title_loses_its_extension_and_no_other_title_does() {
    let conn = planted(
        "INSERT INTO work (id, content_key, slug, title, title_hand, title_derived,
                           sidecar_path, reading, hub)
         VALUES (1, 'k1', 's1', 'Berserk - v01 (2003).cbz', NULL, 'Berserk - v01 (2003).cbz',
                 NULL, 'rtl', 'raw'),
                (2, 'k2', 's2', 'Mine.cbz', 'Mine.cbz', 'Foo v01.CBZ', NULL, 'rtl', 'raw'),
                (3, 'k3', 's3', 'Doc.cbz', NULL, 'Doc.cbz', '/x/taiyaku.json', 'rtl', 'raw'),
                (4, 'k4', 's4', 'Two.cbz.cbz', NULL, 'Two.cbz.cbz', NULL, 'rtl', 'raw'),
                (5, 'k5', 's5', '.cbz', NULL, '.cbz', NULL, 'rtl', 'raw'),
                (6, 'k6', 's6', 'Kept.cbz', 'Kept.cbz', NULL, NULL, 'rtl', 'raw');",
    );
    assert_eq!(
        found(&conn, "cbz"),
        [1, 2, 3, 4, 5, 6],
        "the index holds every title before the re-make"
    );

    migrate(&conn).expect("the re-make finishes");

    let some = |s: &str| Some(s.to_string());
    assert_eq!(
        titles(&conn),
        [
            (
                1,
                "Berserk - v01 (2003)".to_string(),
                None,
                some("Berserk - v01 (2003)")
            ),
            (2, "Mine.cbz".to_string(), some("Mine.cbz"), some("Foo v01")),
            (3, "Doc".to_string(), None, some("Doc")),
            (4, "Two.cbz".to_string(), None, some("Two.cbz")),
            (5, ".cbz".to_string(), None, some(".cbz")),
            (6, "Kept.cbz".to_string(), some("Kept.cbz"), None),
        ]
    );
    assert_eq!(
        slugs(&conn)[3],
        (4, "two".to_string()),
        "the slug is minted from the stripped title"
    );
    assert_eq!(
        found(&conn, "Berserk"),
        [1],
        "the index finds the new title"
    );
    assert_eq!(
        found(&conn, "cbz"),
        [2, 4, 5, 6],
        "and no longer finds the extensions the re-make dropped"
    );
}

/// A `bridge:` row is found by its slug on every write, so it keeps its slug
/// and its title, and a re-made work counts its slug as taken.
#[test]
fn a_bridge_row_keeps_its_slug() {
    let conn = planted(
        "INSERT INTO work (id, content_key, slug, title, title_derived, sidecar_path,
                           reading, hub)
         VALUES (1, 'bridge:foo-json', 'foo-json', 'Foo.cbz', 'Foo.cbz', '/x/foo.json',
                 'rtl', 'raw'),
                (2, 'k2', 'old-2', 'Foo', 'Foo', NULL, 'rtl', 'raw'),
                (3, 'k3', 'old-3', 'Foo Json', 'Foo Json', NULL, 'rtl', 'raw');",
    );

    migrate(&conn).expect("the re-make finishes");

    assert_eq!(
        slugs(&conn),
        slug_pairs(&[(1, "foo-json"), (2, "foo"), (3, "foo-json-2")])
    );
    assert_eq!(
        titles(&conn)[0],
        (1, "Foo.cbz".to_string(), None, Some("Foo.cbz".to_string())),
        "the bridge row's title is the sidecar's, as the fixed registry serves it"
    );
}
