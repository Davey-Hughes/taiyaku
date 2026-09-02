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

/// A document whose work has been rekeyed under it is an **orphan**, and the
/// report has to carry the title out of it. The filename is a hash of content
/// that no longer exists, so a report naming only the path gives a person
/// nothing to act on — and since O80 closed, a title-only document carries no
/// series, no volume and no member digests to identify it by any other route.
///
/// **The same pass also closes the gap the rekey opened at the new
/// address.** The work's marker is still armed and its new `content_key`
/// now addresses a file that does not exist — a gap by this module's own
/// definition — so the loop that finds the orphan also finds and repairs
/// that gap in the same call to `reconcile`. This is store spec §4.4 holding
/// through a repack: the correction is not merely reported as stranded, it
/// is carried forward to where the work now lives.
#[test]
fn a_document_no_work_addresses_is_an_orphan_named_with_its_title() {
    let dir = crate::tests::tempdir("hand-reconcile-orphan");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    // The volume's bytes changed: a repack, or a page added. `import` follows
    // the row to the new key, and the document does not follow it.
    conn.execute(
        "UPDATE work SET content_key = 'moved' WHERE id = ?1",
        [work_id],
    )
    .expect("rekeying the work");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.orphans.len(), 1, "the stranded document is an orphan");
    assert_eq!(
        found.orphans[0].title.as_deref(),
        Some("Hand Set"),
        "the report must carry the title, because the filename is a hash of \
         content that no longer exists and nothing else identifies the work"
    );
    assert!(
        found.orphans[0]
            .path
            .to_string_lossy()
            .contains("abc123.library.json"),
        "named by the address it is actually at: {:?}",
        found.orphans[0].path
    );

    // The other half: the rekey also opened a gap at `moved.library.json`,
    // and the same pass closes it.
    assert_eq!(
        found.repaired, 1,
        "the correction is written at the new address, not merely reported \
         as stranded at the old one: {found:?}"
    );
    let (new_path, new_doc) = super::load(&conn, work_id)
        .expect("loading")
        .expect("the rekeyed work has a document at its new address");
    assert!(
        new_path.to_string_lossy().contains("moved.library.json"),
        "written under the work's new content_key: {new_path:?}"
    );
    assert_eq!(
        new_doc.title.as_deref(),
        Some("Hand Set"),
        "the hand-set title survives the repack, store spec §4.4: {new_doc:?}"
    );
}

/// **O82's claim, driven.** A correction made while the root could not take
/// the document is written on the next pass once it can. Before this, nothing
/// re-attempted the write and no command existed to sweep — measured on
/// 2026-08-31 by restoring write permission and running a full scan and
/// derive, after which the document still held the superseded title.
#[test]
fn a_gap_is_written_on_the_next_pass() {
    let dir = crate::tests::tempdir("hand-reconcile-retry");
    let root = dir.path.join("root");
    std::fs::create_dir(&root).expect("creating the root");
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root.to_string_lossy());

    // **A file where the directory must go, not `set_readonly`.** CI runs as
    // root and ignores the write bit; this fails for any uid. See
    // `a_root_that_cannot_take_the_document_still_arms_the_guard`.
    std::fs::write(root.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the directory must go");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    let saved = super::save(&conn, work_id).expect("saving must not fail");
    assert!(!saved.persisted, "the document could not reach the disk");

    // The obstruction is removed — the media became writable.
    std::fs::remove_file(root.join(".taiyaku")).expect("clearing the obstruction");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.repaired, 1, "the gap was closed: {found:?}");
    assert_eq!(found.unwritable.len(), 0, "and nothing was left unwritable");
    assert_eq!(found.orphans.len(), 0, "and nothing was orphaned");

    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    let text = std::fs::read_to_string(&path).expect("the document is on disk now");
    assert!(
        text.contains("Hand Set"),
        "and it carries the correction: {text}"
    );
}

/// **A gap the marker names but the projection does not.** A work whose
/// marker is armed while it carries no hand-set field at all reproduces the
/// state a hand edit withdrawn outside `save` would leave behind:
/// `librarydoc::of` projects the same empty document
/// `of_is_empty_for_a_work_nothing_has_been_applied_to` pins, so the gap
/// loop's call to `save` takes its remove-file branch, finds nothing to
/// remove (`ErrorKind::NotFound`), and answers `persisted: true` for a pass
/// that wrote no file at all. Before this fix that counted as a repair;
/// asserting `repaired == 0` is the fix.
#[test]
fn a_gap_whose_document_projects_empty_is_not_counted_as_repaired() {
    let dir = crate::tests::tempdir("hand-reconcile-empty-gap");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    // Armed directly, without ever calling `save` — the marker alone, with
    // nothing behind it.
    crate::migrate::record_absorbed(&conn, work_id, crate::migrate::LIBRARY, true)
        .expect("arming the marker");
    assert!(
        crate::librarydoc::of(&conn, work_id)
            .expect("projecting")
            .is_empty(),
        "the fixture only demonstrates the finding if the projection is empty"
    );

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(
        found.repaired, 0,
        "nothing was written for an empty projection: {found:?}"
    );
}

/// A root that stays unwritable reports the same failure every pass, naming
/// the path. Reported, never raised: a scan must not die because one root of
/// several cannot take a document.
#[test]
fn a_gap_that_cannot_be_written_is_named_and_does_not_fail_the_pass() {
    let dir = crate::tests::tempdir("hand-reconcile-still-unwritable");
    let root = dir.path.join("root");
    std::fs::create_dir(&root).expect("creating the root");
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root.to_string_lossy());

    std::fs::write(root.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the directory must go");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving must not fail");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling must not fail");

    assert_eq!(found.repaired, 0, "nothing could be written");
    assert_eq!(found.unwritable.len(), 1, "and the failure is named");
    assert!(
        found.unwritable[0].contains(&root.to_string_lossy().to_string()),
        "naming the path that could not be written: {:?}",
        found.unwritable[0]
    );
}

/// **The control.** A library in which nothing has gone wrong must report
/// nothing at all — without this, a `reconcile` that returned every work as a
/// gap, or rewrote every document on every pass, would still pass the three
/// tests above.
#[test]
fn a_library_with_nothing_wrong_reports_nothing() {
    let dir = crate::tests::tempdir("hand-reconcile-clean");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.orphans.len(), 0, "nothing orphaned");
    assert_eq!(found.stale.len(), 0, "nothing stale");
    assert_eq!(found.repaired, 0, "nothing needed repair");
    assert_eq!(found.unwritable.len(), 0, "nothing failed to write");
}

/// **The third state, and the one neither O81 nor O82 named.** A document that
/// exists at the right address and disagrees with the database is neither a
/// gap nor an orphan, so a sweep counting only presence walks straight past
/// it — and a rebuild then reads the older title back over the newer one.
/// Measured on 2026-08-31; see `docs/notes.md`.
#[test]
fn a_document_that_disagrees_with_the_database_is_stale_and_is_not_rewritten() {
    let dir = crate::tests::tempdir("hand-reconcile-stale");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("First Correction".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    // A second edit that never reached the disk — the read-only-root case,
    // reproduced by editing the row directly rather than by permissions.
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Second Correction".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    let before = std::fs::read_to_string(&path).expect("reading the document");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.stale.len(), 1, "the disagreement is seen: {found:?}");
    assert_eq!(found.repaired, 0, "a stale document is not a gap");
    assert_eq!(
        std::fs::read_to_string(&path).expect("re-reading"),
        before,
        "a stale document is reported and touched by nothing — the database \
         is not automatically right, since O44 records this as the one \
         document this project designs for hand-editing"
    );
}

/// **The control that pins the `updated` normalisation.** `librarydoc::of`
/// stamps `updated` with the time the projection was taken and nothing in the
/// schema stores it, so a comparison that did not normalise it would call
/// every reachable document stale — including one written a moment ago.
#[test]
fn a_document_that_agrees_is_not_stale_despite_a_newer_updated_stamp() {
    let dir = crate::tests::tempdir("hand-reconcile-agrees");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    // Rewrite the file with a deliberately older stamp and the same content.
    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    let mut on_disk =
        crate::librarydoc::LibraryDoc::from_json(&std::fs::read_to_string(&path).expect("reading"))
            .expect("parsing");
    on_disk.updated = "2001-01-01T00:00:00Z".to_string();
    std::fs::write(&path, on_disk.to_json()).expect("rewriting with an older stamp");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(
        found.stale.len(),
        0,
        "only the content decides staleness, never the stamp: {found:?}"
    );
}

/// A file at a work's own address that cannot be *parsed* is not a
/// disagreement — it is already `DeriveReport::unreadable_hand`'s, which the
/// read-back populates, and this must not duplicate it. Checked by hand:
/// turning the parse guard's `let … else { continue }` in `reconcile` into
/// `found.stale.push(path.display().to_string())` makes this test fail;
/// reverted after confirming that.
///
/// **This exercises only the parse guard, not the read guard.** `b"not json
/// at all"` is valid UTF-8, so `std::fs::read_to_string` always succeeds
/// against it and only `LibraryDoc::from_json` can fail — a mutant that
/// turns the *read* guard's `continue` into a `stale` push survives this
/// test. See `a_document_that_cannot_be_read_is_not_reported_stale` for that
/// guard's own coverage.
#[test]
fn an_unparseable_document_is_neither_stale_nor_an_orphan() {
    let dir = crate::tests::tempdir("hand-reconcile-unparseable");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    // The document is corrupted in place, at its own addressed path — not
    // moved, not missing, just unreadable as JSON.
    std::fs::write(&path, b"not json at all").expect("corrupting the document in place");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling must not fail");

    assert_eq!(
        found.stale.len(),
        0,
        "a file that fails to parse is not a hand disagreement: {found:?}"
    );
    // Structurally guaranteed here, not exercised: the path is addressed, so
    // `reconcile`'s `if let Some(work_id)` arm always `continue`s — whichever
    // guard fires, or neither — before the orphan-push arm below it is ever
    // reached. Kept to document that an unreadable document at an addressed
    // path stays "owed", not to test behaviour that cannot vary.
    assert_eq!(
        found.orphans.len(),
        0,
        "the address is still owed by this work; it just cannot be read \
         back, which is not the same thing as unreachable: {found:?}"
    );
}

/// The read guard's own coverage. [`an_unparseable_document_is_neither_stale_
/// nor_an_orphan`] cannot reach it: valid UTF-8 that fails to *parse* still
/// succeeds at `std::fs::read_to_string`, so a mutant turning that test's
/// read guard into a `stale` push survives it. A **directory** planted at the
/// addressed path fails `read_to_string` with `Is a directory` for any uid,
/// while `reconcile`'s `.ends_with(".library.json")` check still finds it in
/// the listing — that check is on the name, not the file type.
///
/// **Not `set_readonly`**, per this codebase's own rule against it in tests:
/// `a_root_that_cannot_take_the_document_still_arms_the_guard`'s comment
/// records why — CI runs as root, which ignores the write bit, so a
/// read-only permission bit passes locally and fails on the forge. A
/// directory has no such escape: reading one fails for every uid, root
/// included.
///
/// Checked by hand: turning the read guard's `let … else { continue }` in
/// `reconcile` into `found.stale.push(path.display().to_string())` makes
/// this test fail; reverted after confirming that.
#[test]
fn a_document_that_cannot_be_read_is_not_reported_stale() {
    let dir = crate::tests::tempdir("hand-reconcile-unreadable");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    // The document `save` just wrote is replaced with a directory of the
    // same name — not moved, not missing, just unreadable as a file.
    std::fs::remove_file(&path).expect("clearing the document `save` wrote");
    std::fs::create_dir(&path).expect("planting a directory where the document must go");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling must not fail");

    assert_eq!(
        found.stale.len(),
        0,
        "a file that fails to read is not a hand disagreement: {found:?}"
    );
}

/// **Finding 2.** `librarydoc::of` projects `Comicinfo` and `Filename`
/// chapters into `members`, not only `Hand` ones, and `derive::
/// write_chapters` writes a fresh `Filename` chapter from a member's name on
/// every scan. A work with an unrelated hand edit whose document already
/// agrees with the database must not turn `stale` the moment the library
/// gains a chapter nobody touched by hand.
///
/// Checked by hand: removing `reconcile`'s `not_filename` filter makes this
/// fail, because the document on disk (saved before the chapter existed) then
/// disagrees with the freshly projected one on `members` alone; the filter
/// was restored after confirming that.
#[test]
fn a_new_filename_chapter_does_not_make_a_hand_edit_stale() {
    let dir = crate::tests::tempdir("hand-reconcile-new-chapter");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    // The library gains a chapter — `derive::write_chapters` naming a member
    // from its filename on a later scan, not a person editing the document.
    conn.execute(
        "UPDATE member SET chapter_declared = 1.0, chapter_source = 'filename'
          WHERE id = 1",
        [],
    )
    .expect("simulating a scan deriving a filename chapter");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(
        found.stale.len(),
        0,
        "a filename-derived chapter is derivation's own churn, not a \
         person's disagreement: {found:?}"
    );
}
