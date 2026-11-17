use std::path::Path;

use rusqlite::Connection;

use super::*;
use crate::Library;

/// A directory at `dir/name`, added, with its stored interval and its last
/// scan put in place by SQL.
fn dir_with(
    conn: &Connection,
    dir: &Path,
    name: &str,
    stored: Option<i64>,
    last_scan_at: Option<&str>,
) -> scan::Root {
    let path = dir.join(name);
    std::fs::create_dir_all(&path).expect("makes the directory");
    let root = scan::add_root(conn, &path).expect("adds the directory");
    conn.execute(
        "UPDATE root SET scan_interval_secs = ?2, last_scan_at = ?3 WHERE id = ?1",
        rusqlite::params![root.id, stored, last_scan_at],
    )
    .expect("puts the schedule in place");
    root
}

/// `None` where `scheduled` does not list directory `id` at `now`, else
/// whether it is due.
fn due_at(conn: &Connection, now: &str, id: i64) -> Option<bool> {
    scheduled(conn, now)
        .expect("reads the schedule")
        .into_iter()
        .find(|one| one.root.id == id)
        .map(|one| one.due)
}

#[test]
fn an_automatic_directory_never_scanned_is_listed_and_due() {
    let dir = crate::tests::tempdir("schedule-automatic-never");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = dir_with(&conn, dir.path(), "a", None, None);

    assert_eq!(due_at(&conn, "2026-11-10 12:00:00", root.id), Some(true));
}

#[test]
fn an_automatic_directory_is_due_fifteen_minutes_after_its_last_scan() {
    let dir = crate::tests::tempdir("schedule-automatic-after");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = dir_with(&conn, dir.path(), "a", None, Some("2026-11-10 12:00:00"));

    assert_eq!(
        due_at(&conn, "2026-11-10 12:14:59", root.id),
        Some(false),
        "a second before the unwatched interval has passed"
    );
    assert_eq!(
        due_at(&conn, "2026-11-10 12:15:00", root.id),
        Some(true),
        "the unwatched interval to the second"
    );
}

#[test]
fn an_hourly_directory_is_due_an_hour_after_its_last_scan() {
    let dir = crate::tests::tempdir("schedule-hourly");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = dir_with(
        &conn,
        dir.path(),
        "a",
        Some(3600),
        Some("2026-11-10 12:00:00"),
    );

    assert_eq!(due_at(&conn, "2026-11-10 12:59:59", root.id), Some(false));
    assert_eq!(due_at(&conn, "2026-11-10 13:00:00", root.id), Some(true));
}

#[test]
fn an_off_directory_is_not_listed() {
    let dir = crate::tests::tempdir("schedule-off");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let off = dir_with(&conn, dir.path(), "off", Some(0), None);
    let automatic = dir_with(&conn, dir.path(), "automatic", None, None);

    assert_eq!(
        due_at(&conn, "2026-11-10 12:00:00", off.id),
        None,
        "0 is Off, never scanned or not"
    );
    assert_eq!(
        due_at(&conn, "2026-11-10 12:00:00", automatic.id),
        Some(true),
        "the directory beside it is listed"
    );
}

/// A cancelled job's directories wait for Scan to be pressed, so the
/// scheduler passes a paused one over though it is due, and lists the one
/// beside it.
#[test]
fn a_paused_directory_is_not_scheduled_though_never_scanned_and_due() {
    let dir = crate::tests::tempdir("schedule-paused");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let paused = dir_with(&conn, dir.path(), "paused", None, None);
    let beside = dir_with(&conn, dir.path(), "beside", None, None);

    scan::pause(&conn, std::slice::from_ref(&paused)).expect("pauses it");

    let now = "2026-11-10 12:00:00";
    assert_eq!(
        [due_at(&conn, now, paused.id), due_at(&conn, now, beside.id)],
        [None, Some(true)],
        "(paused, beside)"
    );
}

/// The pause leaves the schedule setting alone, so once unpaused the
/// directory is listed exactly as before it.
#[test]
fn a_paused_directory_is_scheduled_again_once_unpaused() {
    let dir = crate::tests::tempdir("schedule-unpaused");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = dir_with(&conn, dir.path(), "a", Some(3600), None);
    let listed = |conn: &Connection| scheduled(conn, "2026-11-10 12:00:00").expect("reads");
    let before = listed(&conn);
    let roots = std::slice::from_ref(&root);

    scan::pause(&conn, roots).expect("pauses it");
    let paused = listed(&conn);
    scan::unpause(&conn, roots).expect("unpauses it");

    assert!(
        matches!(&before[..], [one] if one.due && one.root.scan_interval_secs == Some(3600)),
        "sanity: listed hourly and due before the pause: {before:?}"
    );
    assert_eq!(paused, [], "sanity: not listed while paused");
    assert_eq!(listed(&conn), before);
}

#[test]
fn a_stored_interval_under_the_minimum_is_read_as_the_minimum() {
    let dir = crate::tests::tempdir("schedule-under-minimum");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = dir_with(
        &conn,
        dir.path(),
        "a",
        Some(60),
        Some("2026-11-10 12:00:00"),
    );

    assert_eq!(
        due_at(&conn, "2026-11-10 12:01:00", root.id),
        Some(false),
        "60 s stored is scanned at the 5-minute minimum"
    );
    assert_eq!(due_at(&conn, "2026-11-10 12:05:00", root.id), Some(true));
}

#[test]
fn a_negative_interval_is_read_as_off() {
    let dir = crate::tests::tempdir("schedule-negative");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let negative = dir_with(&conn, dir.path(), "negative", Some(-1), None);
    let automatic = dir_with(&conn, dir.path(), "automatic", None, None);

    assert_eq!(due_at(&conn, "2026-11-10 12:00:00", negative.id), None);
    assert_eq!(
        due_at(&conn, "2026-11-10 12:00:00", automatic.id),
        Some(true),
        "the directory beside it is listed"
    );
}

#[test]
fn the_schedule_lists_directories_by_path_not_by_id() {
    let dir = crate::tests::tempdir("schedule-order");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let z = dir_with(&conn, dir.path(), "z", None, None);
    let b = dir_with(&conn, dir.path(), "b", None, None);
    assert!(z.id < b.id, "the fixture adds z first");

    let ids: Vec<i64> = scheduled(&conn, "2026-11-10 12:00:00")
        .expect("reads the schedule")
        .iter()
        .map(|one| one.root.id)
        .collect();
    assert_eq!(ids, vec![b.id, z.id], "…/b before …/z");
}

#[test]
fn next_scan_at_is_the_last_scan_plus_the_interval_scanned_at() {
    for (last, stored, next) in [
        (
            Some("2026-11-10 12:00:00"),
            None,
            Some("2026-11-10 12:15:00"),
        ),
        (None, None, None),
        (Some("2026-11-10 12:00:00"), Some(0), None),
        (
            Some("2026-12-31 23:55:00"),
            Some(900),
            Some("2027-01-01 00:10:00"),
        ),
        (
            Some("2028-02-28 23:55:00"),
            Some(600),
            Some("2028-02-29 00:05:00"),
        ),
        (
            Some("2100-02-28 23:55:00"),
            Some(600),
            Some("2100-03-01 00:05:00"),
        ),
        (
            Some("2026-11-10 12:00:00"),
            Some(60),
            Some("2026-11-10 12:05:00"),
        ),
        (Some("not a time"), None, None),
    ] {
        assert_eq!(
            next_scan_at(last, stored).as_deref(),
            next,
            "{last:?} every {stored:?}"
        );
    }
}

/// An interval that would carry the time past the shape's last year is never
/// due, rather than read as a stamp that does not parse, which the scheduler
/// counts due at every tick.
#[test]
fn next_scan_at_past_the_last_year_it_can_write_is_that_years_last_second() {
    for (last, stored) in [
        ("9999-12-31 23:55:00", 600),
        ("2026-11-10 12:00:00", 1_000_000_000_000),
        ("2026-11-10 12:00:00", i64::MAX - 24 * 60 * 60),
        ("2026-11-10 12:00:00", i64::MAX),
    ] {
        assert_eq!(
            next_scan_at(Some(last), Some(stored)).as_deref(),
            Some("9999-12-31 23:59:59"),
            "{last} every {stored}"
        );
    }
}

/// The saturated time compares after any `now` the store's clock writes, so
/// the scheduler reads the directory as not due.
#[test]
fn a_directory_scanned_at_an_interval_past_the_last_year_is_not_due() {
    let dir = crate::tests::tempdir("schedule-past-the-last-year");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = dir_with(
        &conn,
        dir.path(),
        "a",
        Some(1_000_000_000_000),
        Some("2026-11-10 12:00:00"),
    );

    assert_eq!(due_at(&conn, "2026-11-10 12:15:00", root.id), Some(false));
}

#[test]
fn the_stores_clock_reads_in_the_shape_the_rule_parses() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let now = now(&conn).expect("reads the store's clock");

    assert!(next_scan_at(Some(&now), Some(300)).is_some(), "{now}");
}
