use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;

use super::*;
use crate::scan::tests::{
    READABLE_RAR, rar_fixture, write_archive, write_archive_labelled, write_pages,
};
use crate::test_support::library_of_root;
use crate::{Library, derive, libraries, scan};

/// The layout version these tests store and ask under.
const VERSION: u32 = 1;

/// `dir` added in a library of its own, read back so its libraries are
/// filled in, and walked.
fn scanned(conn: &Connection, dir: &Path) -> scan::Root {
    let added = scan::add_root(conn, dir).expect("adds the directory");
    let root = scan::roots(conn)
        .expect("lists the directories")
        .into_iter()
        .find(|root| root.id == added.id)
        .expect("the directory just added");
    scan::scan_root(conn, &root).expect("walks it");
    root
}

/// The units due under `build`, by path.
fn due_by_path(conn: &Connection, build: Option<&str>) -> BTreeMap<String, Unit> {
    due(conn, VERSION, build)
        .expect("lists what is due")
        .into_iter()
        .map(|unit| (unit.rel_path.clone(), unit))
        .collect()
}

fn digest_at(conn: &Connection, root_id: i64, rel_path: &str) -> Option<String> {
    conn.query_row(
        "SELECT sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root_id, rel_path],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("{rel_path} has a row: {e}"))
}

/// The `(size, mtime)` the walk recorded for `rel_path`.
fn recorded(conn: &Connection, root_id: i64, rel_path: &str) -> (i64, i64) {
    conn.query_row(
        "SELECT size, mtime FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root_id, rel_path],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap_or_else(|e| panic!("{rel_path} has a row: {e}"))
}

fn set_entry(conn: &Connection, root_id: i64, rel_path: &str, assignment: &str) {
    conn.execute(
        &format!("UPDATE scan_entry SET {assignment} WHERE root_id = ?1 AND rel_path = ?2"),
        rusqlite::params![root_id, rel_path],
    )
    .unwrap_or_else(|e| panic!("sets {assignment} on {rel_path}: {e}"));
}

fn a_row(sha256: &str, version: u32, outcome: Outcome, build: Option<&str>) -> Row {
    Row {
        sha256: sha256.to_string(),
        version,
        outcome,
        reason: None,
        build: build.map(str::to_string),
        pages: 3,
        prints: vec![0; 3],
    }
}

fn switch(conn: &Connection, library: i64, on: bool) {
    libraries::set_match_by_content(conn, library, on).expect("switches the library");
}

/// The EPUB and the empty chapter folder are claimed, so only the due rule's
/// own filters keep them out: the EPUB by its format, the folder by its page
/// count. A RAR is due in a build that reads RAR, and refused by its format
/// in one that does not.
#[test]
fn due_takes_paged_units_of_covered_roots() {
    let tree = crate::tests::tempdir("fingerprints-due");
    let top = tree.path();
    write_archive(&top.join("Alpha").join("Alpha v01.cbz"), 3);
    write_archive(&top.join("Kaguya").join("Kaguya - c001.cbz"), 2);
    write_pages(&top.join("Beta").join("Beta v01"), 2);
    std::fs::create_dir_all(top.join("Story")).expect("mkdir");
    taiyaku_read::test_support::epub(&top.join("Story"), "Story v01.epub", &["One."]);
    std::fs::create_dir_all(top.join("Gamma").join("Gamma - c001")).expect("mkdir");
    std::fs::create_dir_all(top.join("Delta")).expect("mkdir");
    std::fs::copy(
        rar_fixture(READABLE_RAR).path,
        top.join("Delta").join("Delta v01.cbr"),
    )
    .expect("copies the fixture");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, top);
    derive::derive(&conn).expect("derives");
    let claimed: Vec<String> = derive::claimed_units(&conn)
        .expect("lists the claims")
        .into_iter()
        .map(|unit| unit.rel_path)
        .collect();
    for rel_path in ["Story/Story v01.epub", "Gamma/Gamma - c001"] {
        assert!(
            claimed.iter().any(|claim| claim == rel_path),
            "the fixture: the derive claims {rel_path}"
        );
    }

    let due = due_by_path(&conn, Some("a"));

    let volume = due
        .get("Alpha/Alpha v01.cbz")
        .expect("an archive volume is due");
    assert!(volume.sha256.is_some(), "with the digest the derive took");
    assert_eq!(
        volume.sha256,
        digest_at(&conn, root.id, "Alpha/Alpha v01.cbz")
    );
    assert_eq!(
        (volume.root_id, volume.folder, &volume.path),
        (root.id, false, &root.path.join("Alpha/Alpha v01.cbz"))
    );
    let chapter = due
        .get("Kaguya/Kaguya - c001.cbz")
        .expect("a chapter archive is due");
    assert_eq!(
        chapter.sha256, None,
        "with no digest: no derive hashes a chapter"
    );
    let folder = due.get("Beta/Beta v01").expect("a folder volume is due");
    assert!(folder.folder, "as a folder");
    assert!(
        !due.contains_key("Story/Story v01.epub"),
        "a reflowable EPUB has no page to read"
    );
    assert!(
        !due.contains_key("Gamma/Gamma - c001"),
        "a folder of no page has nothing to read"
    );
    let reads_rar = Format::Rar.is_readable();
    let rar = due.get("Delta/Delta v01.cbr");
    assert_eq!(rar.is_some(), reads_rar, "a RAR is read where RAR is read");
    if let Some(rar) = rar {
        assert!(rar.sha256.is_some(), "with the digest the derive took");
        assert_eq!(rar.sha256, digest_at(&conn, root.id, "Delta/Delta v01.cbr"));
    }
    assert_eq!(
        due.len(),
        3 + usize::from(reads_rar),
        "and nothing else is due"
    );
}

/// Directory D is in libraries A and B, and Inner, nested in D, is in C alone.
#[test]
fn a_directory_is_covered_when_any_library_with_it_matches() {
    let outer = crate::tests::tempdir("fingerprints-covered");
    std::fs::create_dir_all(outer.path().join("Inner")).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let shared = scan::add_root(&conn, outer.path()).expect("adds D");
    let a = library_of_root(&conn, shared.id);
    conn.execute("INSERT INTO library (name) VALUES ('B')", [])
        .expect("makes B");
    let b = conn.last_insert_rowid();
    crate::test_support::link(&conn, b, shared.id);
    let inner = scan::add_root(&conn, &outer.path().join("Inner")).expect("adds Inner");
    let c = library_of_root(&conn, inner.id);
    libraries::rename(&conn, a, "Zeta").expect("renames A");
    let name = |id: i64| {
        libraries::name_of(&conn, id)
            .expect("reads")
            .expect("named")
    };
    assert_eq!(
        name(c),
        "Inner",
        "the fixture: C is named for its directory"
    );
    assert!(
        a < b && b < c,
        "the fixture: by id, A, B, C; by name, B, C, A"
    );

    let all_on = coverage(&conn).expect("reads the coverage");
    assert_eq!(
        all_on.libraries,
        vec![(b, name(b)), (c, name(c)), (a, name(a))],
        "by name"
    );

    switch(&conn, b, false);
    switch(&conn, c, false);
    let one_on = coverage(&conn).expect("reads the coverage");

    assert!(
        one_on.roots.contains(&shared.id),
        "D: A matches, though B does not"
    );
    assert!(
        one_on.roots.contains(&inner.id),
        "Inner: A reaches it, though C, its own library, does not match"
    );
    assert_eq!(one_on.libraries, vec![(a, name(a))], "A alone matches");

    switch(&conn, a, false);
    let none_on = coverage(&conn).expect("reads the coverage");

    assert!(
        !none_on.roots.contains(&shared.id),
        "D: no library with it matches"
    );
    assert!(
        !none_on.roots.contains(&inner.id),
        "Inner: nor one reaching it"
    );
    assert_eq!(none_on.libraries, Vec::new(), "no library matches");
}

#[test]
fn a_paused_directory_has_nothing_due() {
    let tree = crate::tests::tempdir("fingerprints-paused");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    write_archive(&tree.path().join("Kaguya").join("Kaguya - c001.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let before = due(&conn, VERSION, None).expect("lists");
    assert_eq!(before.len(), 2, "the fixture: two units due");

    scan::pause(&conn, std::slice::from_ref(&root)).expect("pauses");
    assert_eq!(
        due(&conn, VERSION, None).expect("lists"),
        Vec::new(),
        "a paused directory's units wait"
    );

    scan::unpause(&conn, std::slice::from_ref(&root)).expect("unpauses");
    assert_eq!(
        due(&conn, VERSION, None).expect("lists"),
        before,
        "and are due again once it is unpaused"
    );
}

#[test]
fn stored_rows_decide_what_is_due() {
    let tree = crate::tests::tempdir("fingerprints-stored");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let sha256 = digest_at(&conn, root.id, "Alpha/Alpha v01.cbz").expect("the derive's digest");
    let is_due =
        |build: Option<&str>| due_by_path(&conn, build).contains_key("Alpha/Alpha v01.cbz");
    let stored = |version: u32, outcome: Outcome, build: Option<&str>| {
        store(&conn, &a_row(&sha256, version, outcome, build)).expect("stores");
    };
    assert!(is_due(Some("a")), "no row yet");

    stored(VERSION, Outcome::Whole, Some("a"));
    assert!(!is_due(Some("a")), "read whole by this build");
    assert!(!is_due(Some("b")), "read whole by another build");

    stored(VERSION - 1, Outcome::Whole, Some("a"));
    assert!(is_due(Some("a")), "a row of an older layout");

    for outcome in [Outcome::Partial, Outcome::Unreadable] {
        stored(VERSION, outcome, Some("a"));
        assert!(!is_due(Some("a")), "{outcome:?}, by this build");
        assert!(is_due(Some("b")), "{outcome:?}, by another build");
    }

    stored(VERSION, Outcome::Partial, None);
    assert!(!is_due(None), "partial by no build, asked by none");
    assert!(is_due(Some("b")), "partial by no build, asked by a build");
}

/// Alpha and Beta hold one archive's bytes; Gamma holds its own.
#[test]
fn twins_are_one_unit_and_the_newest_comes_first() {
    let tree = crate::tests::tempdir("fingerprints-twins");
    let top = tree.path();
    write_archive_labelled(&top.join("Alpha").join("Alpha v01.cbz"), 3, "jpg", "twin");
    write_archive_labelled(&top.join("Beta").join("Beta v01.cbz"), 3, "jpg", "twin");
    write_archive(&top.join("Gamma").join("Gamma v01.cbz"), 4);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, top);
    derive::derive(&conn).expect("derives");
    let twin = digest_at(&conn, root.id, "Alpha/Alpha v01.cbz");
    assert!(twin.is_some(), "the fixture: the derive took the digest");
    assert_eq!(
        digest_at(&conn, root.id, "Beta/Beta v01.cbz"),
        twin,
        "the fixture: twins"
    );
    let first_seen = |rel_path: &str, at: &str| {
        set_entry(&conn, root.id, rel_path, &format!("first_seen = '{at}'"));
    };
    let order = || -> Vec<String> {
        due(&conn, VERSION, None)
            .expect("lists")
            .into_iter()
            .map(|unit| unit.rel_path)
            .collect()
    };

    first_seen("Alpha/Alpha v01.cbz", "2020-01-01 00:00:00");
    first_seen("Gamma/Gamma v01.cbz", "2021-01-01 00:00:00");
    assert_eq!(
        order(),
        vec!["Beta/Beta v01.cbz", "Gamma/Gamma v01.cbz"],
        "Beta, the newer twin, stands for both, ahead of an older file"
    );

    first_seen("Beta/Beta v01.cbz", "2019-01-01 00:00:00");
    first_seen("Alpha/Alpha v01.cbz", "2022-01-01 00:00:00");
    assert_eq!(
        order(),
        vec!["Alpha/Alpha v01.cbz", "Gamma/Gamma v01.cbz"],
        "Alpha, now the newer twin, stands for both"
    );
}

/// `Alpha v01.cbz` is walked first and sorts first as text, since a space is
/// below `/`; as a path, `Alpha/` comes first. So no row order gives the
/// path order by accident.
#[test]
fn units_first_seen_together_come_in_path_order() {
    let tree = crate::tests::tempdir("fingerprints-path-order");
    write_archive(&tree.path().join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    write_archive(&tree.path().join("Alpha").join("Alpha v02.cbz"), 4);
    scan::scan_root(&conn, &root).expect("walks it again");
    conn.execute(
        "UPDATE scan_entry SET first_seen = '2020-01-01 00:00:00' WHERE root_id = ?1",
        [root.id],
    )
    .expect("one age for every entry");

    let order: Vec<String> = due(&conn, VERSION, None)
        .expect("lists")
        .into_iter()
        .map(|unit| unit.rel_path)
        .collect();

    assert_eq!(
        order,
        vec!["Alpha/Alpha v02.cbz", "Alpha v01.cbz"],
        "by path, a folder's files before a name it prefixes"
    );
}

#[test]
fn still_due_sees_a_switch_and_a_stored_twin() {
    let tree = crate::tests::tempdir("fingerprints-still-due");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let unit = due(&conn, VERSION, Some("a"))
        .expect("lists")
        .into_iter()
        .next()
        .expect("the volume is due");
    let still = || still_due(&conn, &unit, VERSION, Some("a")).expect("asks");
    let switched = library_of_root(&conn, root.id);
    assert!(still(), "a fresh unit");

    switch(&conn, switched, false);
    assert!(!still(), "its library switched off");

    switch(&conn, switched, true);
    let sha256 = unit.sha256.as_deref().expect("the derive's digest");
    store(&conn, &a_row(sha256, VERSION, Outcome::Whole, Some("a"))).expect("stores");
    assert!(!still(), "a row its twin stored stands for it");
}

/// Four chapter archives, which no derive hashes.
#[test]
fn a_digest_is_written_back_only_where_empty_and_unchanged() {
    let tree = crate::tests::tempdir("fingerprints-record");
    for n in 1..=4 {
        write_archive(
            &tree
                .path()
                .join("Kaguya")
                .join(format!("Kaguya - c00{n}.cbz")),
            2,
        );
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let due = due_by_path(&conn, None);
    let units: Vec<&Unit> = (1..=4)
        .map(|n| {
            due.get(&format!("Kaguya/Kaguya - c00{n}.cbz"))
                .expect("due")
        })
        .collect();
    for unit in &units {
        assert_eq!(
            unit.sha256, None,
            "the fixture: {} has no digest",
            unit.rel_path
        );
    }

    let empty = units[0];
    assert!(
        record_digest(&conn, empty, "one").expect("writes"),
        "empty, unchanged"
    );
    assert_eq!(
        digest_at(&conn, root.id, &empty.rel_path).as_deref(),
        Some("one")
    );

    let taken = units[1];
    set_entry(&conn, root.id, &taken.rel_path, "sha256 = 'taken since'");
    assert!(!record_digest(&conn, taken, "two").expect("asks"), "taken");
    assert_eq!(
        digest_at(&conn, root.id, &taken.rel_path).as_deref(),
        Some("taken since"),
        "a digest taken since stands"
    );

    for (unit, moved) in [
        (units[2], "size = size + 1"),
        (units[3], "mtime = mtime + 1"),
    ] {
        set_entry(&conn, root.id, &unit.rel_path, moved);
        assert!(
            !record_digest(&conn, unit, "moved").expect("asks"),
            "{}: {moved}",
            unit.rel_path
        );
        assert_eq!(
            digest_at(&conn, root.id, &unit.rel_path),
            None,
            "{}",
            unit.rel_path
        );
    }
}

/// Four chapter archives, which no derive hashes, so each digest is set by
/// hand. An entry moved since holds a digest too, or a test that ignored the
/// stamp would still find none there; and one entry is given a stamp no other
/// has, so a lookup that ignored the path or the directory would find it.
#[test]
fn a_recorded_digest_is_answered_only_while_the_entry_is_unchanged() {
    let tree = crate::tests::tempdir("fingerprints-recorded");
    for n in 1..=4 {
        write_archive(
            &tree
                .path()
                .join("Kaguya")
                .join(format!("Kaguya - c00{n}.cbz")),
            2,
        );
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let due = due_by_path(&conn, None);
    let units: Vec<&Unit> = (1..=4)
        .map(|n| {
            due.get(&format!("Kaguya/Kaguya - c00{n}.cbz"))
                .expect("due")
        })
        .collect();
    for unit in &units {
        assert_eq!(
            unit.sha256, None,
            "the fixture: {} has no digest",
            unit.rel_path
        );
    }

    assert_eq!(
        recorded_digest(&conn, units[0]).expect("reads"),
        None,
        "an entry holding none"
    );

    set_entry(
        &conn,
        root.id,
        &units[0].rel_path,
        "sha256 = 'held here', size = 7, mtime = 7",
    );
    let here = Unit {
        size: 7,
        mtime: 7,
        ..units[0].clone()
    };
    assert_eq!(
        recorded_digest(&conn, &here).expect("reads").as_deref(),
        Some("held here"),
        "the fixture: the one entry of that stamp"
    );
    for (elsewhere, what) in [
        (
            Unit {
                rel_path: "Kaguya/Kaguya - c009.cbz".to_owned(),
                ..here.clone()
            },
            "another path",
        ),
        (
            Unit {
                root_id: root.id + 1,
                ..here.clone()
            },
            "another directory",
        ),
    ] {
        assert_eq!(
            recorded_digest(&conn, &elsewhere).expect("reads"),
            None,
            "{what}"
        );
    }

    let taken = units[1];
    set_entry(&conn, root.id, &taken.rel_path, "sha256 = 'taken since'");
    assert_eq!(
        recorded_digest(&conn, taken).expect("reads").as_deref(),
        Some("taken since"),
        "a digest taken since the unit was listed"
    );

    for (unit, moved) in [
        (units[2], "size = size + 1"),
        (units[3], "mtime = mtime + 1"),
    ] {
        set_entry(
            &conn,
            root.id,
            &unit.rel_path,
            &format!("sha256 = 'other bytes', {moved}"),
        );
        assert_eq!(
            recorded_digest(&conn, unit).expect("reads"),
            None,
            "{}: {moved}",
            unit.rel_path
        );
    }
}

/// A paused directory's derive passes an entry whose digest its works hold,
/// so a digest written there could add a copy the pause holds back.
#[test]
fn a_digest_is_not_written_back_in_a_paused_directory() {
    let tree = crate::tests::tempdir("fingerprints-record-paused");
    write_archive(&tree.path().join("Kaguya").join("Kaguya - c001.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let unit = due(&conn, VERSION, None)
        .expect("lists")
        .into_iter()
        .next()
        .expect("the chapter is due");
    assert_eq!(unit.sha256, None, "the fixture: no digest yet");

    scan::pause(&conn, std::slice::from_ref(&root)).expect("pauses");
    assert!(
        !record_digest(&conn, &unit, "paused").expect("asks"),
        "a paused directory's entry"
    );
    assert_eq!(
        digest_at(&conn, root.id, &unit.rel_path),
        None,
        "is left as it was"
    );

    scan::unpause(&conn, std::slice::from_ref(&root)).expect("unpauses");
    assert!(
        record_digest(&conn, &unit, "unpaused").expect("writes"),
        "the same entry, unpaused"
    );
    assert_eq!(
        digest_at(&conn, root.id, &unit.rel_path).as_deref(),
        Some("unpaused")
    );
}

#[test]
fn stamp_of_matches_what_the_walk_recorded() {
    let tree = crate::tests::tempdir("fingerprints-stamp");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    write_pages(&tree.path().join("Beta").join("Beta v01"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    let due = due_by_path(&conn, None);

    for rel_path in ["Alpha/Alpha v01.cbz", "Beta/Beta v01"] {
        let walked = recorded(&conn, root.id, rel_path);
        assert_eq!(
            stamp_of(&root.path.join(rel_path)),
            Some(walked),
            "{rel_path}"
        );
        let unit = due.get(rel_path).expect("due");
        assert_eq!((unit.size, unit.mtime), walked, "{rel_path}'s unit");
    }

    let file = root.path.join("Alpha/Alpha v01.cbz");
    std::fs::write(&file, b"other bytes, of another length").expect("rewrites");
    assert_ne!(
        stamp_of(&file),
        Some(recorded(&conn, root.id, "Alpha/Alpha v01.cbz")),
        "a rewritten file's stamp moves"
    );
    assert_eq!(
        stamp_of(&root.path.join("gone.cbz")),
        None,
        "a path not there"
    );
}

#[test]
fn store_replaces_an_older_row() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let older = Row {
        sha256: "d".to_string(),
        version: 1,
        outcome: Outcome::Partial,
        reason: Some("page 2 did not decode".to_string()),
        build: Some("a".to_string()),
        pages: 3,
        prints: vec![1, 0, 2],
    };
    store(&conn, &older).expect("stores");
    assert_eq!(read(&conn, "d"), Ok(Some(older)), "read back as written");

    let newer = Row {
        sha256: "d".to_string(),
        version: 2,
        outcome: Outcome::Whole,
        reason: None,
        build: Some("b".to_string()),
        pages: 4,
        prints: vec![2; 9],
    };
    store(&conn, &newer).expect("replaces");
    assert_eq!(read(&conn, "d"), Ok(Some(newer)), "every column replaced");
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM fingerprint", [], |r| r.get(0))
        .expect("counts");
    assert_eq!(rows, 1, "in place");

    let unstamped = Row {
        sha256: "e".to_string(),
        version: 1,
        outcome: Outcome::Unreadable,
        reason: Some("not an archive".to_string()),
        build: None,
        pages: 0,
        prints: Vec::new(),
    };
    store(&conn, &unstamped).expect("stores");
    assert_eq!(read(&conn, "e"), Ok(Some(unstamped)), "no build, no prints");
    assert_eq!(read(&conn, "absent"), Ok(None), "no row");
}

/// One directory matching with a volume folder of two chapter folders, and
/// one switched off.
#[test]
fn rows_no_claim_holds_are_dropped() {
    let on = crate::tests::tempdir("fingerprints-drop-on");
    write_archive(&on.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let volume = on.path().join("Beta").join("Beta - v01");
    write_pages(&volume.join("c001"), 2);
    write_pages(&volume.join("c002"), 2);
    let off = crate::tests::tempdir("fingerprints-drop-off");
    write_archive(&off.path().join("Gamma").join("Gamma v01.cbz"), 4);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let on_root = scanned(&conn, on.path());
    let off_root = scanned(&conn, off.path());
    derive::derive(&conn).expect("derives");
    switch(&conn, library_of_root(&conn, off_root.id), false);
    set_entry(
        &conn,
        on_root.id,
        "Beta/Beta - v01/c001",
        "sha256 = 'nested'",
    );
    set_entry(
        &conn,
        on_root.id,
        "Alpha/Alpha v01.cbz",
        "held_sha256 = 'held'",
    );
    let matched = digest_at(&conn, on_root.id, "Alpha/Alpha v01.cbz").expect("a digest");
    let switched_off = digest_at(&conn, off_root.id, "Gamma/Gamma v01.cbz").expect("a digest");
    for sha256 in ["held by none", "nested", "held", &matched, &switched_off] {
        store(&conn, &a_row(sha256, VERSION, Outcome::Whole, None)).expect("stores");
    }

    let dropped = drop_unclaimed(&conn).expect("drops");

    assert_eq!(read(&conn, "held by none"), Ok(None), "no entry holds it");
    assert_eq!(
        read(&conn, "nested"),
        Ok(None),
        "only a chapter folder its volume speaks for holds it"
    );
    for (kept, why) in [
        (matched.as_str(), "a claimed file's"),
        (
            switched_off.as_str(),
            "a claimed file's in a directory switched off",
        ),
        ("held", "a paused directory's held reading's"),
    ] {
        assert!(
            read(&conn, kept).expect("reads").is_some(),
            "{why} row stays"
        );
    }
    assert_eq!(dropped, 2, "the two rows no claim holds");
}

/// The drop after an ordinary pass: every row is held by a claimed file or a
/// held reading, so it asks for no write. Another connection holds the
/// store's write lock and this one waits for none, so a write asked for is
/// refused at once.
#[test]
fn a_drop_with_nothing_to_drop_takes_no_write_lock() {
    let tree = crate::tests::tempdir("fingerprints-drop-no-write");
    let top = tree.path().join("lib");
    write_archive(&top.join("Alpha").join("Alpha v01.cbz"), 3);
    let db = tree.path().join("library.db");
    let library = Library::open(&db).expect("creates the store");
    let conn = library.conn();
    let root = scanned(&conn, &top);
    derive::derive(&conn).expect("derives");
    set_entry(
        &conn,
        root.id,
        "Alpha/Alpha v01.cbz",
        "held_sha256 = 'held'",
    );
    let matched = digest_at(&conn, root.id, "Alpha/Alpha v01.cbz").expect("a digest");
    for sha256 in [matched.as_str(), "held"] {
        store(&conn, &a_row(sha256, VERSION, Outcome::Whole, None)).expect("stores");
    }
    conn.busy_timeout(std::time::Duration::ZERO)
        .expect("waits for no lock");
    let holder = Connection::open(&db).expect("another connection");
    holder
        .execute_batch("BEGIN IMMEDIATE")
        .expect("takes the write lock");

    let dropped = drop_unclaimed(&conn);

    holder.execute_batch("ROLLBACK").expect("releases the lock");
    assert_eq!(dropped, Ok(0), "nothing to drop, and no write asked for");
    for (kept, why) in [
        (matched.as_str(), "a claimed file's"),
        ("held", "a held reading's"),
    ] {
        assert!(
            read(&conn, kept).expect("reads").is_some(),
            "{why} row stays"
        );
    }
}

/// A reading held after the drop's snapshot, as a pause between the snapshot
/// and the write holds one, keeps the row it names: the write reads the held
/// readings again. A row no reading holds still goes.
#[test]
fn a_reading_held_after_the_snapshot_keeps_its_row() {
    let tree = crate::tests::tempdir("fingerprints-drop-held-later");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    for sha256 in ["held later", "never held"] {
        store(&conn, &a_row(sha256, VERSION, Outcome::Whole, None)).expect("stores");
    }
    let mut unclaimed = unclaimed(&conn).expect("reads the snapshot");
    unclaimed.sort();
    assert_eq!(
        unclaimed,
        ["held later", "never held"],
        "the fixture: no claim or reading holds either"
    );
    set_entry(
        &conn,
        root.id,
        "Alpha/Alpha v01.cbz",
        "held_sha256 = 'held later'",
    );

    let dropped = drop_unless_held(&conn, &unclaimed);

    assert_eq!(dropped, Ok(1), "the row no reading holds");
    assert!(
        read(&conn, "held later").expect("reads").is_some(),
        "the row the reading now holds stays"
    );
    assert_eq!(read(&conn, "never held"), Ok(None), "the other goes");
}

/// A re-count keeps a file's digest, so only the count can say that the row
/// under it numbers other pages, and it says so to a build with no stamp too.
#[test]
fn a_row_numbering_other_pages_than_the_files_count_is_due_whatever_its_build() {
    const PATH: &str = "Alpha/Alpha v01.cbz";
    let tree = crate::tests::tempdir("fingerprints-other-pages");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let sha256 = digest_at(&conn, root.id, PATH).expect("the derive's digest");
    let unit = due_by_path(&conn, None).remove(PATH).expect("no row yet");

    for outcome in [Outcome::Whole, Outcome::Partial] {
        let row = Row {
            pages: 2,
            ..a_row(&sha256, VERSION, outcome, None)
        };
        store(&conn, &row).expect("stores");
        assert!(
            due_by_path(&conn, None).contains_key(PATH),
            "{outcome:?} is listed"
        );
        assert!(
            still_due(&conn, &unit, VERSION, None).expect("asks"),
            "{outcome:?} is still due as the pass reaches it"
        );

        store(&conn, &a_row(&sha256, VERSION, outcome, None)).expect("stores");
        assert!(
            !due_by_path(&conn, None).contains_key(PATH),
            "{outcome:?} at its count"
        );
        assert!(
            !still_due(&conn, &unit, VERSION, None).expect("asks"),
            "{outcome:?} at its count is settled as the pass reaches it"
        );
    }
}

/// D8: a re-count keeps a RAR's digest as it keeps a zip's, so a RAR row
/// numbering other pages than this reader counts in the file is due again, in
/// a build that reads RAR, and settled once a read brings it to the count. A
/// build without a RAR backend reads no RAR.
#[test]
fn a_rar_row_numbering_other_pages_than_the_files_count_is_due() {
    const PATH: &str = "Alpha/Alpha v01.cbz";
    let fixture = rar_fixture(READABLE_RAR);
    let tree = crate::tests::tempdir("fingerprints-rar-other-pages");
    std::fs::create_dir_all(tree.path().join("Alpha")).expect("mkdir");
    std::fs::copy(&fixture.path, tree.path().join(PATH)).expect("copies the fixture");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    if !Format::Rar.is_readable() {
        assert!(
            !due_by_path(&conn, None).contains_key(PATH),
            "a build without a RAR backend reads no RAR"
        );
        return;
    }
    let sha256 = digest_at(&conn, root.id, PATH).expect("the derive's digest");
    let unit = due_by_path(&conn, None).remove(PATH).expect("no row yet");
    let pages = u32::try_from(fixture.pages.len()).expect("a few pages");
    let numbering = |pages: u32| Row {
        pages,
        ..a_row(&sha256, VERSION, Outcome::Whole, None)
    };

    store(&conn, &numbering(pages - 1)).expect("stores");
    assert!(due_by_path(&conn, None).contains_key(PATH), "it is listed");
    assert!(
        still_due(&conn, &unit, VERSION, None).expect("asks"),
        "it is still due as the pass reaches it"
    );

    store(&conn, &numbering(pages)).expect("stores");
    assert!(!due_by_path(&conn, None).contains_key(PATH), "at its count");
    assert!(
        !still_due(&conn, &unit, VERSION, None).expect("asks"),
        "at its count it is settled as the pass reaches it"
    );
}

/// Each count a read of the file cannot bring a row to: were any of these due,
/// every pass would read the file and store the same row again.
#[test]
fn a_row_no_read_would_renumber_stays_settled() {
    const ARCHIVE: &str = "Alpha/Alpha v01.cbz";
    const FOLDER: &str = "Beta/Beta v01";
    let tree = crate::tests::tempdir("fingerprints-no-renumbering");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    write_pages(&tree.path().join("Beta").join("Beta v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let archive = digest_at(&conn, root.id, ARCHIVE).expect("the archive's digest");
    let folder = digest_at(&conn, root.id, FOLDER).expect("the folder's digest");
    let short = |sha256: &str, outcome| Row {
        pages: 2,
        ..a_row(sha256, VERSION, outcome, None)
    };

    store(&conn, &short(&archive, Outcome::Unreadable)).expect("stores");
    store(&conn, &short(&folder, Outcome::Whole)).expect("stores");
    assert_eq!(
        due_by_path(&conn, None).len(),
        0,
        "an unreadable row holds no prints to renumber, and a folder's walk leaves out \
         what its patterns exclude where a read takes it in"
    );

    store(&conn, &short(&archive, Outcome::Whole)).expect("stores");
    set_entry(&conn, root.id, ARCHIVE, "counted_by = NULL");
    assert_eq!(
        due_by_path(&conn, None).len(),
        0,
        "a count another reader took says nothing until a scan counts it again"
    );

    let signature = Format::Zip
        .token()
        .and_then(taiyaku_read::reader_signature)
        .expect("a zip has a signature");
    set_entry(
        &conn,
        root.id,
        ARCHIVE,
        &format!("counted_by = '{signature}'"),
    );
    assert!(
        due_by_path(&conn, None).contains_key(ARCHIVE),
        "the fixture: under this reader's count the short row is due"
    );
}

/// A re-count can lower a file's count, and a row numbering more pages than the
/// file holds is as far from it as one numbering fewer.
#[test]
fn a_row_numbering_more_pages_than_the_files_count_is_due() {
    const PATH: &str = "Alpha/Alpha v01.cbz";
    let tree = crate::tests::tempdir("fingerprints-more-pages");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let sha256 = digest_at(&conn, root.id, PATH).expect("the derive's digest");
    let unit = due_by_path(&conn, None).remove(PATH).expect("no row yet");

    let row = Row {
        pages: 4,
        ..a_row(&sha256, VERSION, Outcome::Whole, None)
    };
    store(&conn, &row).expect("stores");

    assert!(due_by_path(&conn, None).contains_key(PATH), "it is listed");
    assert!(
        still_due(&conn, &unit, VERSION, None).expect("asks"),
        "it is still due as the pass reaches it"
    );
}

/// A count another reader took says nothing of the pages this reader numbers,
/// so the row stays settled until a scan counts the file again.
#[test]
fn a_row_under_a_count_another_reader_took_stays_settled() {
    const PATH: &str = "Alpha/Alpha v01.cbz";
    let tree = crate::tests::tempdir("fingerprints-another-readers-count");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned(&conn, tree.path());
    derive::derive(&conn).expect("derives");
    let sha256 = digest_at(&conn, root.id, PATH).expect("the derive's digest");
    let unit = due_by_path(&conn, None).remove(PATH).expect("no row yet");
    let row = Row {
        pages: 2,
        ..a_row(&sha256, VERSION, Outcome::Whole, None)
    };
    store(&conn, &row).expect("stores");
    assert!(
        due_by_path(&conn, None).contains_key(PATH),
        "the fixture: under this reader's count the short row is due"
    );

    set_entry(&conn, root.id, PATH, "counted_by = 'another reader'");

    assert!(
        !due_by_path(&conn, None).contains_key(PATH),
        "it is not listed"
    );
    assert!(
        !still_due(&conn, &unit, VERSION, None).expect("asks"),
        "it is settled as the pass reaches it"
    );
}
