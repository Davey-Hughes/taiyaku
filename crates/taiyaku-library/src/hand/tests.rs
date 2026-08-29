use rusqlite::Connection;

/// A migrated database holding one scanned work: a hub edition `en` with two
/// members under two different roots, plus a second, non-hub edition `raw`
/// with a member under a third root.
///
/// **Strengthened from the brief's one-edition, one-member fixture.** That
/// fixture cannot fail for the reason `the_address_is_a_dotfile_directory_
/// under_the_hub_editions_root`'s name claims: with a single edition and a
/// single member, "the hub edition's first member" and "any member at all"
/// name the same row, so a query that dropped `edition.key = work.hub` or
/// reversed `ORDER BY member.ordinal` still passed against it (checked by
/// hand, mutating `path_for`'s SQL and re-running against the original
/// fixture — both mutants survived). Three roots make the three candidates
/// resolve to three different paths, so picking the wrong one is visible:
/// `/wrong-order` is the hub edition's *second* member by ordinal, and
/// `/other-edition` belongs to the non-hub edition entirely. Only
/// `/library`, the hub edition's first member, is the right answer.
///
/// The non-hub edition's member carries ordinal `-1` — lower than either of
/// the hub edition's, rather than tied with the first at `0` — so that
/// dropping the `edition.key = work.hub` filter picks it *deterministically*
/// first once ordered, rather than winning only because SQLite's sort
/// happened to keep the tie in insertion order. Checked by hand: with ordinal
/// `0` on both, the same mutant still passed.
fn a_scanned_work(conn: &Connection) -> i64 {
    crate::schema::migrate(conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO root (id, path) VALUES
           (1, '/library'),
           (2, '/wrong-order'),
           (3, '/other-edition');
         INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'abc123', 'w', 'Derived', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES
           (1, 1, 'en', 'en'),
           (2, 1, 'raw', 'raw');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id) VALUES
           (1, 1, 0, '/library/v01.cbz', 'deadbeef', 100, 1),
           (2, 1, 1, '/wrong-order/v02.cbz', 'beefdead', 100, 2),
           (3, 2, -1, '/other-edition/v01.cbz', 'cafebabe', 100, 3);",
    )
    .expect("seeding");
    1
}

#[test]
fn the_address_is_a_dotfile_directory_under_the_hub_editions_root() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work(&conn);

    let path = super::path_for(&conn, work_id)
        .expect("resolving")
        .expect("a scanned work has a root");

    assert_eq!(
        path,
        std::path::Path::new("/library/.taiyaku/abc123.library.json"),
        "the address is <root>/.taiyaku/<content_key>.library.json — a dotted \
         directory so `scan.rs` skips it without an exclusion list, and keyed \
         by content so a rename or a remount does not move it"
    );
}

#[test]
fn a_work_with_no_rooted_member_has_no_address() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'abc123', 'w', 'Derived', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');",
    )
    .expect("seeding");

    assert_eq!(
        super::path_for(&conn, 1).expect("resolving"),
        None,
        "an imported work keeps its sidecar's directory and does not want this \
         address; only a scanned work, whose `sidecar_path` is NULL by design, \
         has nowhere else to put the document"
    );
}

fn a_scanned_work_rooted_at(conn: &Connection, root: &str) -> i64 {
    crate::schema::migrate(conn).expect("migrating");
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, ?1)",
        rusqlite::params![root],
    )
    .expect("seeding the root");
    conn.execute_batch(
        // `title_derived` set alongside `title`, matching every production
        // writer of this row (migration 010's own backfill, `import::
        // insert_work`, …) rather than the NULL a hand-typed `INSERT` would
        // otherwise leave — `librarydoc::apply`'s `COALESCE(?2, title_derived)`
        // needs it populated the moment a caller withdraws a hand-set title
        // back to `None`, or the write fails `work.title`'s own `NOT NULL`.
        "INSERT INTO work (id, content_key, slug, title, title_derived, reading, hub)
         VALUES (1, 'abc123', 'w', 'Derived', 'Derived', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (1, 1, 0, 'v01.cbz', 'deadbeef', 100, 1);",
    )
    .expect("seeding");
    1
}

#[test]
fn saving_writes_the_document_and_records_the_marker() {
    let dir = crate::tests::tempdir("hand-save");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let saved = super::save(&conn, work_id).expect("saving");

    assert!(saved.persisted, "a writable root takes the document");
    let path = saved.path.expect("a scanned work has an address");
    let text = std::fs::read_to_string(&path).expect("reading the document back");
    assert!(
        text.contains("Hand Set"),
        "asserted on the file rather than on the call, per C3 §8: {text}"
    );
    assert!(
        crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "the marker is what arms `derive`'s guard; without it the next scan \
         reverts the edit"
    );
}

#[test]
fn an_unedited_work_has_nothing_to_remove_and_that_is_not_a_failure() {
    // The ordinary case: every work that has never been hand-edited. No
    // `librarydoc::apply` at all, so `librarydoc::of` projects the empty
    // document and `save` takes the remove branch — but there was never a
    // document on disk to begin with, so `remove_file` finds nothing there.
    // That is not a removal failure; the disk already reflects the database
    // (there is nothing to write), and there is nothing to report.
    let dir = crate::tests::tempdir("hand-save-no-file");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let saved = super::save(&conn, work_id).expect("saving");

    assert!(
        saved.persisted,
        "no file ever existed to remove: the disk already reflects the \
         database, and that is not a removal failure to report"
    );
    assert_eq!(
        saved.why, None,
        "nothing went wrong, so there is nothing to report"
    );
    // **I2.** `record_absorbed(…, !doc.is_empty())` must not arm the guard
    // for a work nothing has ever hand-edited — mutating `!doc.is_empty()`
    // to `true` would freeze derivation out of every scanned work's
    // `series_id`/`volume`/`edition.kind`/`edition.language` the moment
    // anything ever called `save` on it at all, hand edit or not. Nothing
    // above named the marker, only `persisted`/`why`.
    assert!(
        !crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "an empty document has nothing to protect; arming the guard for it \
         anyway would freeze this work out of derivation for no edit at all"
    );
}

/// The test above takes the remove branch, but the file was never there to
/// begin with — `remove_file` answers `NotFound` either way, and mutating
/// `save`'s `doc.is_empty()` match guard to `false` still passes it, because
/// the write branch it falls through to also answers `persisted: true` for
/// an empty document with nowhere to land. This is the case that guard
/// actually protects: a document that *was* written, then hand-cleared back
/// to nothing.
#[test]
fn a_hand_document_withdrawn_removes_the_file_it_left_behind() {
    let dir = crate::tests::tempdir("hand-save-withdraw");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    let first = super::save(&conn, work_id).expect("saving the hand-set title");
    let path = first.path.expect("a scanned work has an address");
    assert!(
        path.is_file(),
        "the document must actually land, or withdrawing it proves nothing"
    );

    // The edit is withdrawn — every field cleared back to what a person
    // clearing the metadata editor would leave, which projects exactly as
    // empty as a work nothing has ever touched.
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = None;
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying the withdrawal");
    assert!(
        crate::librarydoc::of(&conn, work_id)
            .expect("projecting")
            .is_empty(),
        "the document must actually be empty, or this test exercises the \
         write branch by accident rather than the remove branch it means to"
    );

    let second = super::save(&conn, work_id).expect("saving the withdrawal");

    assert!(
        second.persisted,
        "removing a file that exists is not a durability failure"
    );
    assert!(
        !path.exists(),
        "a document that projects empty must remove the file rather than \
         write an empty one over it: mutating the `doc.is_empty()` guard to \
         `false` takes the write branch instead and leaves the old content \
         replaced with a near-empty document rather than gone"
    );
}

/// [`a_hand_document_withdrawn_removes_the_file_it_left_behind`]'s remove
/// branch, but the removal itself fails for a reason that is not "nothing
/// there" — mutating `save`'s `e.kind() == std::io::ErrorKind::NotFound`
/// guard to `true` folds this into the ordinary no-file-to-remove case and
/// reports `persisted: true` for a write that never happened.
#[test]
fn a_removal_failure_that_is_not_notfound_is_reported_not_swallowed() {
    let dir = crate::tests::tempdir("hand-save-remove-fails");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    let first = super::save(&conn, work_id).expect("saving the hand-set title");
    let path = first.path.expect("a scanned work has an address");
    assert!(
        path.is_file(),
        "the document must actually land, or this test cannot force its \
         removal to fail"
    );

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = None;
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying the withdrawal");

    // **A directory in the document's place, not a read-only parent.** This
    // read `set_readonly` on the parent until 2026-08-29, and that premise
    // does not hold for every process: **CI runs as root, which ignores the
    // write bit**, so the removal succeeded there and the failure this test
    // is named for never happened — the forge failed on it while every local
    // run passed. `remove_file` on a directory is `EISDIR` for any uid, which
    // is a removal failure that is not `NotFound`, which is exactly the arm
    // under test.
    std::fs::remove_file(&path).expect("clearing the document to put a directory in its place");
    std::fs::create_dir(&path).expect("a directory where the document was");

    let saved = super::save(&conn, work_id).expect("saving must not fail here");

    assert!(
        !saved.persisted,
        "a removal that fails for a reason other than \"nothing there\" is a \
         real durability failure and must be reported, not folded into the \
         ordinary nothing-to-remove case"
    );
    let why = saved.why.as_deref().expect(
        "a real removal failure records why, not just false — mutating the \
         `NotFound` guard to `true` swallows it here",
    );
    assert!(
        why.contains(&path.to_string_lossy().to_string()),
        "the reason should name the path that could not be removed: {why}"
    );
    assert!(
        path.exists(),
        "the path must still be there: the removal genuinely failed rather \
         than quietly succeeding"
    );
}

#[test]
fn a_root_that_cannot_take_the_document_still_arms_the_guard() {
    let dir = crate::tests::tempdir("hand-save-unwritable");
    let root = dir.path.join("unwritable");
    std::fs::create_dir(&root).expect("creating the root");
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root.to_string_lossy());

    // Without an actual edit, `librarydoc::of` projects empty and `save`
    // takes its remove-file branch instead of its write branch — recording
    // `absorbed: false`, since there is then nothing to protect.
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    // **A file where the directory must go, not a read-only root.** This test
    // used `set_readonly` on `root` until 2026-08-29, and that premise does
    // not hold for every process: **CI runs as root, which ignores the write
    // bit**, so `create_dir_all` succeeded there, the document reached the
    // disk, and the assertion below failed on the forge while passing for
    // every unprivileged developer. A plain file at `<root>/.taiyaku` makes
    // `create_dir_all` fail with `AlreadyExists` (`File exists`, os error 17)
    // for *any* uid, which is the same branch of `save` without the
    // privilege dependency.
    std::fs::write(root.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the directory must go");

    let saved = super::save(&conn, work_id).expect("saving must not fail here");

    assert!(!saved.persisted, "the document did not reach the disk");
    let why = saved
        .why
        .as_deref()
        .expect("a real write failure records why, not just false");
    assert!(
        why.contains(&root.to_string_lossy().to_string()),
        "the reason should name the path that could not be written, per \
         atomic::write's own O29 comment (staging/rename each name `path`, \
         never the temporary): {why}"
    );
    assert!(
        crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "an edit onto media that cannot take the document is still an edit: \
         the row is written and the guard is armed, and only durability is lost"
    );
}

/// **I4.** A work with no rooted member at all — [`a_work_with_no_rooted_
/// member_has_no_address`]'s own fixture, an imported/sidecar work — has
/// nowhere for [`save`] to write. That must not mean the edit is unprotected:
/// `derive::write_work`'s guard reads this same marker before it overwrites
/// `work.series_id`, `work.volume`, `edition.kind` and `edition.language`, so
/// an edit that never arms it would be silently reverted the next time
/// anything scans this library.
#[test]
fn saving_a_work_with_no_address_still_arms_the_guard() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'abc123', 'w', 'Derived', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');",
    )
    .expect("seeding");
    let work_id = 1;
    assert_eq!(
        super::path_for(&conn, work_id).expect("resolving"),
        None,
        "this work must really have no address, or this test proves nothing"
    );

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let saved = super::save(&conn, work_id).expect("saving must not fail here");

    assert_eq!(
        saved.path, None,
        "there is still nowhere to write the document"
    );
    assert!(
        !saved.persisted,
        "nothing durable happened: there was nowhere to write"
    );
    assert_eq!(
        saved.why, None,
        "there is no address to have failed against, so there is nothing to name"
    );
    assert!(
        crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "an edit against a work with no address is still an edit: without the \
         marker, `derive`'s guard never engages and the next scan overwrites \
         it right back"
    );
}

#[test]
fn a_rescan_with_no_database_recovers_the_hand_set_title() {
    let dir = crate::tests::tempdir("hand-load-rescan");
    let root = dir.path.to_string_lossy().to_string();

    // First database: a person corrects the title, and it is saved.
    let first = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&first, &root);
    let mut doc = crate::librarydoc::of(&first, work_id).expect("projecting");
    doc.title = Some("Go-Toubun no Hanayome, volume 1".to_string());
    crate::librarydoc::apply(&first, work_id, &doc).expect("applying");
    super::save(&first, work_id).expect("saving");
    drop(first);

    // The database is lost. A second one is built from the same root, and the
    // work is minted afresh with the title derivation gives it.
    let second = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&second, &root);

    let (found_path, found) = super::load(&second, work_id)
        .expect("loading")
        .expect("the document is still beside the library");

    assert_eq!(
        found_path,
        std::path::Path::new(&root)
            .join(".taiyaku")
            .join("abc123.library.json"),
        "the path load hands back must be the same address it read from"
    );
    assert_eq!(
        found.title.as_deref(),
        Some("Go-Toubun no Hanayome, volume 1"),
        "losing the database must cost a rescan, not the organisational work \
         (store spec §4.4)"
    );
}
