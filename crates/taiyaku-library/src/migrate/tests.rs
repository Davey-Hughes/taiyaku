/// The account these tests export and import as.
///
/// A named constant rather than a bare `1` at forty call sites, and
/// deliberately *not* a constant in `export.rs`: the one that used to live
/// there answered for every reader in the library, which is what spec A2
/// §5.1 removed. Here it means "whoever this test is", which is a fixture
/// detail rather than a rule.
const THE_READER: i64 = 1;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::{Library, Note, Progress, Region};

/// A temp root nobody else is using — `taiyaku-cli`'s own `library/tests.rs`
/// carries the identical idiom under the identical name, for the identical
/// reason: a real path on a real filesystem, since `migrate::work` stats
/// files rather than reading through an abstraction a test could fake.
///
/// **The process id is part of the name**, matching `export/tests.rs`'s and
/// `src/tests.rs`'s own `tempdir` helpers, which have carried one all along.
/// Without it the path is fixed, so two overlapping runs of this suite — a
/// `cargo test` while a `cargo mutants` or a second terminal is going, which
/// is how this crate is actually worked on — share one directory, and one
/// run's `remove_dir_all` deletes the other's fixture mid-test. The `name`
/// alone only separates tests within one run (whole-branch review 3, Minor
/// 13).
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "taiyaku-test-migrate-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// A work with one edition, `raw-mono`, imported at a **real** path under
/// `root` — `state/tests.rs`'s own `a_work`, except the sidecar path must
/// name a real file here, since `migrate::work` looks for siblings beside it
/// on an actual filesystem rather than being handed a `Connection` alone.
fn a_work(conn: &Connection, root: &Path) -> (i64, PathBuf) {
    let path = root.join("taiyaku.json");
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw-mono":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]}},
        "hub":"raw-mono","alignments":[]}"#;
    std::fs::write(&path, json).unwrap();
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id = crate::import::sidecar(conn, &doc, &path).expect("import");
    (work_id, path)
}

fn write_sibling(root: &Path, suffix: &str, text: &str) {
    std::fs::write(root.join(format!("taiyaku.{suffix}")), text).unwrap();
}

// -- nothing beside the sidecar at all -------------------------------------

#[test]
fn no_sibling_files_at_all_is_a_report_with_nothing_in_it() {
    let root = scratch("no-siblings");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert!(report.is_empty(), "{report:?}");
}

// -- progress ---------------------------------------------------------------

#[test]
fn a_progress_json_beside_the_sidecar_migrates_the_reading_position() {
    let root = scratch("progress-applies");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(&root, "progress.json", r#"{"edition":"raw-mono","slot":7}"#);

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.progress, Some(ProgressOutcome::Applied));

    let recovered = state::progress(&conn, work_id, THE_READER).expect("reads");
    assert_eq!(recovered.len(), 1, "a position was recorded");
    assert_eq!(recovered[0].edition, "raw-mono");
    assert_eq!(recovered[0].slot, 7);
}

/// The property this task added `progressdoc` for: a versioned document
/// carrying more than one position must land *every* position, not just the
/// first — `apply_progress` loops over `progressdoc::from_json`'s output, and
/// nothing before this test ran that loop over more than one element.
///
/// **And each position's own `updated`, which the loop used to drop.**
/// `apply_progress` called `state::put_progress`, which stamps
/// `datetime('now')`, so every reimported position landed in the same second
/// and `state::progress`'s `ORDER BY updated DESC, edition ASC` fell back to
/// alphabet — deciding by edition key which position `getProgress` resumes a
/// reader into. The two times below are minutes apart and in the opposite
/// order to the two edition keys, so a version that restamps them is red on
/// the order and on the times both.
#[test]
fn a_versioned_progress_json_migrates_every_position() {
    let root = scratch("progress-versioned");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(
        &root,
        "progress.json",
        r#"{"taiyaku_progress":1,"positions":[
             {"edition":"raw-mono","slot":7,"updated":"2026-08-27 09:00:00"},
             {"edition":"full-color","slot":33,"updated":"2026-08-27 09:24:41"}]}"#,
    );

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.progress, Some(ProgressOutcome::Applied));

    let recovered = state::progress(&conn, work_id, THE_READER).expect("reads");
    assert_eq!(
        recovered.len(),
        2,
        "both positions must have been recorded, got: {recovered:?}"
    );
    let by_edition: BTreeMap<&str, u32> = recovered
        .iter()
        .map(|p| (p.edition.as_str(), p.slot))
        .collect();
    assert_eq!(by_edition.get("raw-mono"), Some(&7));
    assert_eq!(by_edition.get("full-color"), Some(&33));
    assert_eq!(
        recovered
            .iter()
            .map(|p| (p.edition.as_str(), p.updated.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("full-color", "2026-08-27 09:24:41"),
            ("raw-mono", "2026-08-27 09:00:00"),
        ],
        "each position kept the time the document recorded, and the newer \
         one is the head a reader resumes into"
    );
}

#[test]
fn a_position_already_recorded_is_not_overwritten_by_the_file() {
    let root = scratch("progress-kept");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    // The reader has already moved on since whatever the file remembers.
    state::put_progress(
        &conn,
        work_id,
        THE_READER,
        &Progress {
            edition: "raw-mono".to_string(),
            slot: 42,
        },
    )
    .expect("recording the live position");
    write_sibling(&root, "progress.json", r#"{"edition":"raw-mono","slot":7}"#);

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.progress, Some(ProgressOutcome::AlreadyPresent));

    let recovered = state::progress(&conn, work_id, THE_READER).expect("reads");
    assert_eq!(recovered.len(), 1, "still recorded");
    assert_eq!(recovered[0].slot, 42, "the file must not have won");
}

#[test]
fn a_progress_json_that_does_not_parse_is_reported_and_skipped() {
    let root = scratch("progress-malformed");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(&root, "progress.json", "not json at all");

    let report = work(&conn, work_id, THE_READER, &path).expect("does not fail the import");
    match report.progress {
        Some(ProgressOutcome::Malformed(msg)) => assert!(!msg.is_empty()),
        other => panic!("expected Malformed, got {other:?}"),
    }
    assert_eq!(
        state::progress(&conn, work_id, THE_READER).expect("reads"),
        Vec::new(),
        "nothing was written from an unreadable file"
    );
}

// -- notes --------------------------------------------------------------

#[test]
fn a_notes_json_beside_the_sidecar_migrates_every_note_id_body_and_region() {
    let root = scratch("notes-applies");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(
        &root,
        "notes.json",
        r#"{"work":"t","notes":[
            {"id":"01H1","edition":"raw-mono","slot":0,"body":"whole slot","created":"2026-08-14T10:00:00Z","updated":"2026-08-14T10:00:00Z"},
            {"id":"01H2","edition":"raw-mono","slot":3,
             "region":{"x":0.1,"y":0.2,"w":0.3,"h":0.4},"color":"amber",
             "body":"on a panel","created":"2026-08-14T10:01:00Z","updated":"2026-08-14T10:01:00Z"}
        ]}"#,
    );

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.notes, Some(NotesOutcome::Applied { count: 2 }));

    let recovered = state::notes(&conn, work_id, THE_READER).expect("reads");
    assert_eq!(
        recovered.notes,
        vec![
            Note {
                id: "01H1".to_string(),
                edition: "raw-mono".to_string(),
                slot: 0,
                region: None,
                color: None,
                body: "whole slot".to_string(),
                created: "2026-08-14T10:00:00Z".to_string(),
                updated: "2026-08-14T10:00:00Z".to_string(),
            },
            Note {
                id: "01H2".to_string(),
                edition: "raw-mono".to_string(),
                slot: 3,
                region: Some(Region {
                    x: 0.1,
                    y: 0.2,
                    w: 0.3,
                    h: 0.4
                }),
                color: Some("amber".to_string()),
                body: "on a panel".to_string(),
                created: "2026-08-14T10:01:00Z".to_string(),
                updated: "2026-08-14T10:01:00Z".to_string(),
            },
        ],
        "every note's id, body and region must survive exactly"
    );
}

#[test]
fn notes_already_held_are_not_overwritten_by_the_file() {
    let root = scratch("notes-kept");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    state::put_notes(
        &conn,
        work_id,
        THE_READER,
        &Notes {
            work: "t".to_string(),
            notes: vec![Note {
                id: "live".to_string(),
                edition: "raw-mono".to_string(),
                slot: 0,
                region: None,
                color: None,
                body: "written since".to_string(),
                created: "2026-08-15T00:00:00Z".to_string(),
                updated: "2026-08-15T00:00:00Z".to_string(),
            }],
        },
    )
    .expect("recording a live note");
    write_sibling(
        &root,
        "notes.json",
        r#"{"work":"t","notes":[{"id":"stale","edition":"raw-mono","slot":0,"body":"old","created":"2026-08-01T00:00:00Z","updated":"2026-08-01T00:00:00Z"}]}"#,
    );

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.notes, Some(NotesOutcome::AlreadyPresent));

    let recovered = state::notes(&conn, work_id, THE_READER).expect("reads");
    assert_eq!(recovered.notes.len(), 1);
    assert_eq!(recovered.notes[0].id, "live", "the file must not have won");
}

/// **The C3 defect of whole-branch review 2, closed.** Deciding "has this
/// already been migrated?" by asking whether the database currently holds
/// anything answers a different question. An emptied live state is
/// indistinguishable from "never migrated", so the reader's own deletion was
/// undone by the next import — and `taiyaku serve` imports every path it is
/// given on **every start**, so it recurred at every restart, forever, with
/// no way to make the deletion stick (`export --in-place` never removes a
/// sibling file it has nothing to write to).
#[test]
fn deleting_the_last_note_is_not_undone_by_the_next_import() {
    let root = scratch("notes-deleted-stays-deleted");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(
        &root,
        "notes.json",
        r#"{"work":"t","notes":[{"id":"01H1","edition":"raw-mono","slot":0,"body":"a reader's note","created":"2026-08-14T10:00:00Z","updated":"2026-08-14T10:00:00Z"}]}"#,
    );

    let first = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(first.notes, Some(NotesOutcome::Applied { count: 1 }));

    // The reader deletes their last note: `PUT /notes` with an empty
    // document, which is exactly this.
    state::put_notes(
        &conn,
        work_id,
        THE_READER,
        &Notes {
            work: "t".to_string(),
            notes: Vec::new(),
        },
    )
    .expect("the reader deletes their last note");

    let second = work(&conn, work_id, THE_READER, &path).expect("migrates again");
    assert_eq!(
        second.notes,
        Some(NotesOutcome::Unread),
        "this document was migrated once; the file is history and is not read \
         again — and `Unread` rather than `AlreadyPresent`, because the \
         database now holds no note at all, which is the fact `kept` used to \
         hide (review 3, C1)"
    );
    assert!(
        state::notes(&conn, work_id, THE_READER)
            .expect("reads")
            .notes
            .is_empty(),
        "the deleted note must stay deleted — the sibling file is still on disk, \
         untouched, and `migrate.rs` deliberately never removes it"
    );
}

/// The same defect through the document that matters most: clearing every
/// labelled page (`PUT /truth` with an empty document) must stick. Labelling
/// is hours of irreplaceable human work in both directions — resurrecting 75
/// pages of deleted boxes is as wrong as losing them.
#[test]
fn clearing_every_labelled_page_is_not_undone_by_the_next_import() {
    let root = scratch("truth-cleared-stays-cleared");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(
        &root,
        "truth.json",
        r#"{"taiyaku_truth":1,"editions":{"raw-mono":{"members":[],"pages":{
            "0":[],
            "3":[{"x":0.1,"y":0.2,"w":0.3,"h":0.05,"axis":"v","kind":"dialogue","ignore":false,"text":null}]
        }}}}"#,
    );

    let first = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(
        first.truth,
        Some(TruthOutcome::Applied { pages: 2, boxes: 1 })
    );

    state::put_truth(&conn, work_id, &taiyaku_core::TruthDoc::new())
        .expect("the reader clears every labelled page");

    let second = work(&conn, work_id, THE_READER, &path).expect("migrates again");
    assert_eq!(
        second.truth,
        Some(TruthOutcome::Unread),
        "left unread, and the database holds nothing for it either"
    );
    assert!(
        state::truth(&conn, work_id).expect("reads").is_none(),
        "the cleared labelling must stay cleared"
    );
}

/// "Migrated once" is a property of the work and the document, not of the
/// file's existence: a sibling that appears *after* the first import is not
/// read either. This is the same rule seen from the other side, and it is
/// what closes the route the two tests above do not — import a work with no
/// siblings at all, `export --in-place` to write them, delete the note, and
/// without this the deletion would come back at the next start.
///
/// It is also the boundary of what this module promises: a one-way migration
/// for an existing install's first import, never a sync. Losing the database
/// is what makes a sibling file authoritative again, and that wipes these
/// markers with everything else.
#[test]
fn a_sibling_written_after_the_first_import_is_not_migrated_later() {
    let root = scratch("sibling-appears-later");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);

    let first = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert!(
        first.is_empty(),
        "nothing beside the sidecar yet: {first:?}"
    );

    write_sibling(
        &root,
        "notes.json",
        r#"{"work":"t","notes":[{"id":"01H1","edition":"raw-mono","slot":0,"body":"exported, then deleted","created":"2026-08-14T10:00:00Z","updated":"2026-08-14T10:00:00Z"}]}"#,
    );

    let second = work(&conn, work_id, THE_READER, &path).expect("migrates again");
    assert_eq!(
        second.notes,
        Some(NotesOutcome::Unread),
        "neither side holds this document: the file was never read, and the \
         rows are empty"
    );
    assert!(
        state::notes(&conn, work_id, THE_READER)
            .expect("reads")
            .notes
            .is_empty(),
        "rows are current and files are history, whenever the file appeared"
    );
    assert!(
        !crate::migrate::absorbed(&conn, work_id, crate::migrate::NOTES).expect("marker"),
        "and it is not this library's to overwrite or remove either — see \
         `export::work`'s own tests for the other half"
    );
}

/// The one outcome that is *not* recorded as migrated, and the module doc
/// says why: a file that could not be read has not been migrated, and fixing
/// it and importing again is the documented way to recover. Nothing else in
/// this module would let a reader retry.
#[test]
fn a_sibling_that_was_malformed_is_migrated_once_it_is_fixed() {
    let root = scratch("malformed-then-fixed");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(&root, "notes.json", "{ this is not valid json");

    let first = work(&conn, work_id, THE_READER, &path).expect("does not fail the import");
    assert!(matches!(first.notes, Some(NotesOutcome::Malformed(_))));

    write_sibling(
        &root,
        "notes.json",
        r#"{"work":"t","notes":[{"id":"01H1","edition":"raw-mono","slot":0,"body":"fixed by hand","created":"2026-08-14T10:00:00Z","updated":"2026-08-14T10:00:00Z"}]}"#,
    );

    let second = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(second.notes, Some(NotesOutcome::Applied { count: 1 }));
    assert_eq!(
        state::notes(&conn, work_id, THE_READER)
            .expect("reads")
            .notes
            .len(),
        1
    );
}

#[test]
fn a_notes_json_that_does_not_parse_is_reported_and_skipped() {
    let root = scratch("notes-malformed");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(&root, "notes.json", "{ this is not valid json");

    let report = work(&conn, work_id, THE_READER, &path).expect("does not fail the import");
    match report.notes {
        Some(NotesOutcome::Malformed(msg)) => assert!(!msg.is_empty()),
        other => panic!("expected Malformed, got {other:?}"),
    }
    assert!(
        state::notes(&conn, work_id, THE_READER)
            .expect("reads")
            .notes
            .is_empty(),
        "nothing was written from an unreadable file"
    );
}

// -- truth ------------------------------------------------------------------

/// **The test that matters most in this module.** A page that was examined
/// and found to hold no text must migrate as a present, empty key — not as
/// an absent one, which would read back as "nobody ever looked". That
/// distinction is exactly what `truth_page` exists for
/// (`schema/001_initial.sql`'s own comment: "across 75 pages of manual work
/// that difference is the progress tracker"), and Task 9 fixed the row-store
/// side of it; this is the read-back side.
#[test]
fn a_truth_json_beside_the_sidecar_migrates_a_page_examined_and_found_empty() {
    let root = scratch("truth-applies");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(
        &root,
        "truth.json",
        r#"{"taiyaku_truth":1,"editions":{"raw-mono":{"members":[],"pages":{
            "0":[],
            "3":[{"x":0.1,"y":0.2,"w":0.3,"h":0.05,"axis":"v","kind":"dialogue","ignore":false,"text":null}]
        }}}}"#,
    );

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(
        report.truth,
        Some(TruthOutcome::Applied { pages: 2, boxes: 1 })
    );

    let recovered = state::truth(&conn, work_id)
        .expect("reads")
        .expect("something was labelled");
    let edition = recovered
        .editions
        .get("raw-mono")
        .expect("the edition survived");
    assert!(
        edition.pages.contains_key(&0),
        "page 0 must be present, not absent — it was examined"
    );
    assert_eq!(
        edition.pages.get(&0),
        Some(&Vec::new()),
        "page 0 was found empty, not never-opened"
    );
    assert_eq!(
        edition.pages.get(&3).map(Vec::len),
        Some(1),
        "the labelled page still carries its box"
    );
}

#[test]
fn truth_already_held_is_not_overwritten_by_the_file() {
    let root = scratch("truth-kept");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    let live = taiyaku_core::TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions: BTreeMap::from([(
            "raw-mono".to_string(),
            taiyaku_core::TruthEdition {
                members: Vec::new(),
                pages: BTreeMap::from([(9, Vec::new())]),
            },
        )]),
    };
    state::put_truth(&conn, work_id, &live).expect("recording live truth");
    write_sibling(
        &root,
        "truth.json",
        r#"{"taiyaku_truth":1,"editions":{"raw-mono":{"members":[],"pages":{"0":[]}}}}"#,
    );

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.truth, Some(TruthOutcome::AlreadyPresent));

    let recovered = state::truth(&conn, work_id)
        .expect("reads")
        .expect("still there");
    let edition = recovered.editions.get("raw-mono").unwrap();
    assert!(
        edition.pages.contains_key(&9) && !edition.pages.contains_key(&0),
        "the file must not have won: {edition:?}"
    );
}

#[test]
fn a_truth_json_that_does_not_parse_is_reported_and_skipped() {
    let root = scratch("truth-malformed");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(&root, "truth.json", "{ not json");

    let report = work(&conn, work_id, THE_READER, &path).expect("does not fail the import");
    match report.truth {
        Some(TruthOutcome::Malformed(msg)) => assert!(!msg.is_empty()),
        other => panic!("expected Malformed, got {other:?}"),
    }
    assert!(
        state::truth(&conn, work_id).expect("reads").is_none(),
        "nothing was written from an unreadable file — TruthDoc has no PartialEq, see state/tests.rs"
    );
}

// -- library.json -------------------------------------------------------

#[test]
fn a_library_json_beside_the_sidecar_migrates_hand_fixed_metadata() {
    let root = scratch("library-applies");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(
        &root,
        "library.json",
        r#"{"taiyaku_library":1,"updated":"2026-08-14T11:00:00Z","volume":3.0,"tags":["seinen"]}"#,
    );

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.library, Some(LibraryOutcome::Applied));

    let recovered = librarydoc::of(&conn, work_id).expect("reads");
    assert_eq!(recovered.volume, Some(3.0));
    assert_eq!(recovered.tags, vec!["seinen".to_string()]);
}

#[test]
fn library_metadata_already_held_is_not_overwritten_by_the_file() {
    let root = scratch("library-kept");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    let live = librarydoc::LibraryDoc {
        taiyaku_library: librarydoc::VERSION,
        updated: "2026-08-15T00:00:00Z".to_string(),
        series: librarydoc::SeriesInfo::default(),
        volume: Some(9.0),
        title: None,
        editions: BTreeMap::new(),
        members: Vec::new(),
        tags: Vec::new(),
        external: BTreeMap::new(),
    };
    librarydoc::apply(&conn, work_id, &live).expect("recording live metadata");
    write_sibling(
        &root,
        "library.json",
        r#"{"taiyaku_library":1,"updated":"2026-08-14T11:00:00Z","volume":1.0}"#,
    );

    let report = work(&conn, work_id, THE_READER, &path).expect("migrates");
    assert_eq!(report.library, Some(LibraryOutcome::AlreadyPresent));

    let recovered = librarydoc::of(&conn, work_id).expect("reads");
    assert_eq!(recovered.volume, Some(9.0), "the file must not have won");
}

#[test]
fn a_library_json_that_does_not_parse_is_reported_and_skipped() {
    let root = scratch("library-malformed");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, path) = a_work(&conn, &root);
    write_sibling(&root, "library.json", "{ nope");

    let report = work(&conn, work_id, THE_READER, &path).expect("does not fail the import");
    match report.library {
        Some(LibraryOutcome::Malformed(msg)) => assert!(!msg.is_empty()),
        other => panic!("expected Malformed, got {other:?}"),
    }
    assert!(
        librarydoc::of(&conn, work_id).expect("reads").is_empty(),
        "nothing was written from an unreadable file"
    );
}
