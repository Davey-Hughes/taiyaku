/// The account these tests export and import as.
///
/// A named constant rather than a bare `1` at forty call sites, and
/// deliberately *not* a constant in `export.rs`: the one that used to live
/// there answered for every reader in the library, which is what spec A2
/// §5.1 removed. Here it means "whoever this test is", which is a fixture
/// detail rather than a rule.
const THE_READER: i64 = 1;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use taiyaku_core::{RegionMember, Sidecar, TruthDoc, TruthEdition};

/// A directory unique to one test, removed when the test ends.
///
/// No `tempfile` dependency, matching `taiyaku-serve/src/test_support.rs`'s
/// own `Fixture`: the workspace's dependency set is deliberately short
/// enough to quote in `docs/status.md`, and this is the whole of what that
/// crate would be used for here. `Drop` rather than an end-of-test cleanup
/// call for the same reason `Fixture` gives — the runs that leak are the
/// ones that panic before reaching their own last line.
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn tempdir() -> TempDir {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("taiyaku-test-export-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    TempDir { path }
}

fn doc(title: &str, sha256: &str) -> Sidecar {
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"{title}","reading":"rtl"}},
             "editions":{{"raw":{{"label":"R","members":[
               {{"path":"raw/v01.cbz","sha256":"{sha256}","pages":10}}]}}}},
             "hub":"raw","alignments":[]}}"#
    );
    Sidecar::from_json(&json).expect("parses")
}

/// A work with one edition, imported at a path inside `dir` — so exporting
/// it with `dir: None` still lands beside where it was "found" — plus a
/// note, a recorded position, a labelled (if empty) page and a hand-set
/// volume, so exporting it exercises all five documents rather than just the
/// two every work has.
///
/// The volume is what makes `librarydoc::of` answer a non-empty document, and
/// so what makes `<stem>.library.json` written rather than absent: a work
/// nobody has hand-edited has no metadata to snapshot, and export stopped
/// writing an empty one (review 3, Minor 12). One field is enough and is
/// honest — "this directory is volume 1" is exactly the kind of decision that
/// document exists to keep.
fn a_work_with_notes_and_progress(conn: &rusqlite::Connection, dir: &TempDir) -> i64 {
    let path = dir.path().join("taiyaku.json");
    let id = crate::import::sidecar(conn, &doc("T", "aa"), &path).expect("import");

    conn.execute("UPDATE work SET volume = 1.0 WHERE id = ?1", [id])
        .expect("a hand-set volume");

    crate::state::put_notes(
        conn,
        id,
        1,
        &crate::Notes {
            work: "t".to_string(),
            notes: vec![crate::Note {
                id: "01H".to_string(),
                edition: "raw".to_string(),
                slot: 0,
                region: None,
                color: None,
                body: "a note".to_string(),
                created: "2026-08-14T10:00:00Z".to_string(),
                updated: "2026-08-14T10:00:00Z".to_string(),
            }],
        },
    )
    .expect("put_notes");

    crate::state::put_progress(
        conn,
        id,
        1,
        &crate::Progress {
            edition: "raw".to_string(),
            slot: 3,
        },
    )
    .expect("put_progress");

    let mut pages = std::collections::BTreeMap::new();
    pages.insert(0, Vec::new());
    let mut editions = std::collections::BTreeMap::new();
    editions.insert(
        "raw".to_string(),
        TruthEdition {
            members: vec![RegionMember {
                sha256: "aa".to_string(),
            }],
            pages,
        },
    );
    crate::state::put_truth(
        conn,
        id,
        &TruthDoc {
            taiyaku_truth: taiyaku_core::TRUTH_VERSION,
            editions,
        },
    )
    .expect("put_truth");

    id
}

/// A work with none of the above: no notes, no progress, no truth.
fn a_bare_work(conn: &rusqlite::Connection, dir: &TempDir) -> i64 {
    let path = dir.path().join("taiyaku.json");
    crate::import::sidecar(conn, &doc("Bare", "bb"), &path).expect("import")
}

/// The first import's pass over the four sibling documents — what writes the
/// `migrated_document` markers `export::work` reads, and the only thing that
/// ever sets one to absorbed on the way *in*. Every test below that cares
/// about what an export may touch has to go through this, because the defect
/// it pins lives exactly in the seam between the two.
fn migrate_siblings(
    conn: &rusqlite::Connection,
    dir: &TempDir,
    id: i64,
) -> crate::migrate::SiblingMigration {
    crate::migrate::work(conn, id, THE_READER, &dir.path().join("taiyaku.json")).expect("migrates")
}

/// One labelled box, valid.
const A_TRUTH_DOCUMENT: &str = r#"{"taiyaku_truth":1,"editions":{"raw":{"members":[],"pages":{
    "0":[{"x":0.1,"y":0.2,"w":0.3,"h":0.05,"axis":"v","kind":"dialogue","ignore":false,"text":"labelled by hand"}]
}}}}"#;

/// **Whole-branch review 3, C1(b).** A `taiyaku.truth.json` with one box a
/// fraction off the page (`x + w = 1.15`) is `Malformed`, so it records no
/// marker and writes no rows — which is right, and is what makes fixing it
/// and importing again the documented way to recover (`migrate.rs`'s own doc,
/// design §4.3). One routine `export --in-place` then deleted it, because
/// `state::truth` is `None` and the file was therefore "stale". There was
/// nothing left to fix: one character typed into a labelling file plus one
/// export was the whole of it.
#[test]
fn a_malformed_sibling_survives_an_export_and_can_still_be_fixed_and_imported() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);
    let truth_path = dir.path().join("taiyaku.truth.json");
    let malformed = r#"{"taiyaku_truth":1,"editions":{"raw":{"members":[],"pages":{
        "0":[{"x":0.9,"y":0.2,"w":0.25,"h":0.05,"axis":"v","kind":"dialogue","ignore":false,"text":null}]
    }}}}"#;
    std::fs::write(&truth_path, malformed).expect("writing the malformed sibling");

    let report = migrate_siblings(&conn, &dir, id);
    assert!(
        matches!(
            report.truth,
            Some(crate::migrate::TruthOutcome::Malformed(_))
        ),
        "the fixture must really be unreadable, or this test proves nothing: {report:?}"
    );

    let exported = work(&conn, id, THE_READER, None).expect("export");
    assert!(
        exported.removed.is_empty(),
        "a file this library could not read is not a stale copy of anything it holds: {:?}",
        exported.removed
    );
    assert_eq!(
        exported.kept,
        vec![truth_path.clone()],
        "and it is named rather than passed over in silence"
    );
    assert_eq!(
        std::fs::read_to_string(&truth_path).expect("still there"),
        malformed,
        "byte for byte: an export must not rewrite it either"
    );

    // The documented recovery, still available: fix the file, import again.
    std::fs::write(&truth_path, A_TRUTH_DOCUMENT).expect("the reader fixes it by hand");
    let second = migrate_siblings(&conn, &dir, id);
    assert_eq!(
        second.truth,
        Some(crate::migrate::TruthOutcome::Applied { pages: 1, boxes: 1 })
    );
}

/// **Whole-branch review 3, C1(a) and C1(c).** A work imported with no
/// siblings has all four documents recorded as settled, so a file that turns
/// up afterwards is never read — copy the directory to a laptop, label it
/// there, copy it back; or restore one file from a backup while keeping the
/// database. Export then treated "the database holds no state for this
/// document" as "the file is stale", and the two documents whose state is an
/// `Option` were **deleted** while `notes.json` and `library.json` — always
/// written — were **overwritten with empty documents**, losing the volume,
/// the tags and every note.
#[test]
fn a_sibling_that_appeared_after_the_first_import_is_neither_removed_nor_emptied() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);
    let first = migrate_siblings(&conn, &dir, id);
    assert!(
        first.is_empty(),
        "nothing beside the sidecar at the first import: {first:?}"
    );

    let notes = r#"{"work":"bare","notes":[{"id":"01H1","edition":"raw","slot":0,"body":"written on the laptop","created":"2026-08-14T10:00:00Z","updated":"2026-08-14T10:00:00Z"}]}"#;
    let metadata =
        r#"{"taiyaku_library":1,"updated":"2026-08-14T11:00:00Z","volume":3.0,"tags":["seinen"]}"#;
    let restored = [
        ("taiyaku.library.json", metadata),
        ("taiyaku.notes.json", notes),
        ("taiyaku.truth.json", A_TRUTH_DOCUMENT),
    ];
    for (name, text) in restored {
        std::fs::write(dir.path().join(name), text).expect("the reader copies their work back");
    }

    let second = migrate_siblings(&conn, &dir, id);
    assert_eq!(
        (second.library, second.notes, second.truth),
        (
            Some(crate::migrate::LibraryOutcome::Unread),
            Some(crate::migrate::NotesOutcome::Unread),
            Some(crate::migrate::TruthOutcome::Unread),
        ),
        "settled before, so not read — and the database holds nothing for any of them"
    );

    let exported = work(&conn, id, THE_READER, None).expect("export");
    // The contents first, because they are the loss: `truth.json` was
    // deleted outright, and `notes.json` and `library.json` — always written
    // — came back holding `{"work": ..., "notes": []}` and a bare
    // `taiyaku_library`/`updated` pair.
    for (name, text) in restored {
        assert_eq!(
            std::fs::read_to_string(dir.path().join(name)).unwrap_or_default(),
            text,
            "`{name}` must be exactly as the reader left it"
        );
    }
    assert!(
        exported.removed.is_empty(),
        "nothing here was ever in the database to have gone stale: {:?}",
        exported.removed
    );
    assert_eq!(
        exported.kept,
        [
            "taiyaku.notes.json",
            "taiyaku.library.json",
            "taiyaku.truth.json"
        ]
        .iter()
        .map(|name| dir.path().join(name))
        .collect::<Vec<_>>(),
        "all three named — the ones this export would have written first, then \
         the ones it would have taken away, each group in the fixed order"
    );
    assert_eq!(
        exported.written,
        vec![dir.path().join("taiyaku.json")],
        "the sidecar itself is still written: it is what an import reads, and \
         `--in-place` writing over it is that flag's whole point"
    );
}

/// The other side of the same rule, and the one that keeps `--in-place` from
/// becoming a one-shot command: a document this export *wrote* came out of
/// these rows, so the next export overwrites it without ceremony — and the
/// next import still does not read it back over rows the reader has since
/// emptied, which is what `migrated_document` was added for (review 2, C3).
#[test]
fn a_document_this_export_wrote_is_this_librarys_from_then_on() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_work_with_notes_and_progress(&conn, &dir);
    let notes_path = dir.path().join("taiyaku.notes.json");
    let first_pass = migrate_siblings(&conn, &dir, id);
    assert!(first_pass.is_empty(), "no siblings yet: {first_pass:?}");

    let first = work(&conn, id, THE_READER, None).expect("first export");
    assert!(first.kept.is_empty(), "nothing was there to keep");
    assert!(first.written.contains(&notes_path));

    let second = work(&conn, id, THE_READER, None).expect("second export");
    assert!(
        second.kept.is_empty(),
        "an export is not one-shot: what it wrote from these rows, it may write again: {:?}",
        second.kept
    );
    assert!(second.written.contains(&notes_path));

    // The reader deletes their last note: `PUT /notes` with an empty document.
    crate::state::put_notes(
        &conn,
        id,
        THE_READER,
        &crate::Notes {
            work: "t".to_string(),
            notes: Vec::new(),
        },
    )
    .expect("the reader deletes their last note");

    let report = migrate_siblings(&conn, &dir, id);
    assert_eq!(
        report.notes,
        Some(crate::migrate::NotesOutcome::Unread),
        "the file an export wrote is not read back in"
    );
    assert!(
        crate::state::notes(&conn, id, THE_READER)
            .expect("reads")
            .notes
            .is_empty(),
        "the deleted note stays deleted"
    );
}

/// **`44187fd`'s purpose, through the route that makes it legitimate.** The
/// labelling was really migrated into rows, so the file beside the sidecar is
/// this library's copy of what the rows held; clearing every box and
/// exporting takes it away, which is what makes `--in-place` a snapshot
/// rather than an overlay.
#[test]
fn clearing_the_labelling_after_a_real_migration_still_removes_the_stale_document() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);
    let truth_path = dir.path().join("taiyaku.truth.json");
    std::fs::write(&truth_path, A_TRUTH_DOCUMENT).expect("a labelling file to migrate");

    let report = migrate_siblings(&conn, &dir, id);
    assert_eq!(
        report.truth,
        Some(crate::migrate::TruthOutcome::Applied { pages: 1, boxes: 1 }),
        "the labelling must really have been read into rows"
    );

    crate::state::put_truth(&conn, id, &TruthDoc::new()).expect("the reader clears every box");
    let exported = work(&conn, id, THE_READER, None).expect("export");
    assert_eq!(
        exported.removed,
        vec![truth_path.clone()],
        "the stale document is removed, and the removal is reported"
    );
    assert!(exported.kept.is_empty(), "{:?}", exported.kept);
    assert!(!truth_path.exists(), "really gone, not merely listed");
}

/// And a removal gives the claim up again. Afterwards this library holds
/// nothing of that document on either side, so a file that turns up at that
/// path later is once more something it has never seen — the same conclusion
/// as the appeared-after-import case above, reached from the other direction,
/// and the reason `record_absorbed` is called with `false` there.
#[test]
fn a_sibling_that_returns_after_a_removal_is_not_removed_a_second_time() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);
    let truth_path = dir.path().join("taiyaku.truth.json");
    std::fs::write(&truth_path, A_TRUTH_DOCUMENT).expect("a labelling file to migrate");
    migrate_siblings(&conn, &dir, id);
    crate::state::put_truth(&conn, id, &TruthDoc::new()).expect("the reader clears every box");
    assert_eq!(
        work(&conn, id, THE_READER, None).expect("export").removed,
        vec![truth_path.clone()],
        "the setup must really remove it"
    );

    // Restored from a backup made before the labelling was cleared.
    std::fs::write(&truth_path, A_TRUTH_DOCUMENT).expect("the reader puts one back");
    let exported = work(&conn, id, THE_READER, None).expect("second export");
    assert!(
        exported.removed.is_empty(),
        "this library holds nothing of that document to call the file stale against: {:?}",
        exported.removed
    );
    assert_eq!(exported.kept, vec![truth_path.clone()]);
    assert_eq!(
        std::fs::read_to_string(&truth_path).expect("still there"),
        A_TRUTH_DOCUMENT
    );
}

/// `--dir` is exempt from all of the above: those paths are a destination a
/// caller named for output, not the reader's own documents, and the markers
/// describe the siblings beside the sidecar. Exporting the same work twice
/// into one directory must therefore rewrite it, marker or no marker.
#[test]
fn a_dir_export_writes_over_its_own_output_rather_than_keeping_it() {
    let dir = tempdir();
    let out = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_work_with_notes_and_progress(&conn, &dir);
    migrate_siblings(&conn, &dir, id);

    let first = work(&conn, id, THE_READER, Some(out.path())).expect("first export");
    assert!(first.kept.is_empty());
    let second = work(&conn, id, THE_READER, Some(out.path())).expect("second export");
    assert!(
        second.kept.is_empty(),
        "a --dir export has nothing to preserve: {:?}",
        second.kept
    );
    assert_eq!(
        first.written, second.written,
        "and writes the same five paths both times"
    );
}

/// Five documents, atomically, and nothing half-written if one fails.
#[test]
fn exporting_writes_five_documents_beside_the_sidecar() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_work_with_notes_and_progress(&conn, &dir);

    let written = work(&conn, id, THE_READER, None).expect("export").written;
    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert_eq!(
        names,
        vec![
            "taiyaku.json",
            "taiyaku.library.json",
            "taiyaku.notes.json",
            "taiyaku.progress.json",
            "taiyaku.truth.json",
        ]
    );
}

/// Export and import are inverses. The exported bytes are exactly what a
/// projection produces, which is what makes "the sidecar remains the
/// interchange format" true rather than nominal.
#[test]
fn an_exported_sidecar_is_the_projection_byte_for_byte() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_work_with_notes_and_progress(&conn, &dir);

    let expected = crate::project::sidecar(&conn, id)
        .expect("projection")
        .to_json();
    work(&conn, id, THE_READER, Some(dir.path())).expect("export");

    // `dir/<slug>/`, not `dir/` — see `work`'s doc comment.
    let written =
        std::fs::read_to_string(dir.path().join("t").join("taiyaku.json")).expect("reads");
    assert_eq!(written, expected);
}

/// A work nobody has annotated still exports a notes document, because
/// `getNotes` answers one for such a work and the reader has no
/// empty-versus-absent case to carry either way (`startup.ts`).
#[test]
fn a_work_with_no_notes_exports_an_empty_notes_document() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);

    work(&conn, id, THE_READER, Some(dir.path())).expect("export");
    let text =
        std::fs::read_to_string(dir.path().join("bare").join("taiyaku.notes.json")).expect("reads");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(doc["notes"].as_array().map(Vec::len), Some(0));
}

/// A work with no recorded position, no labelling and no hand-fixed metadata
/// must not export `taiyaku.progress.json`, `taiyaku.truth.json` or
/// `taiyaku.library.json` at all — all three are genuinely absent rather than
/// empty, unlike notes, which has a meaningful empty form because
/// `startup.ts`'s `getNotes` reads one. `taiyaku-serve`'s own
/// `progress::load`/`truth::load` already treat "file absent" as the encoding
/// of `None`; writing an empty placeholder would assert a fact ("this work has
/// a position", "someone opened the labelling tool here", "someone filed this
/// work") that is not true.
///
/// **`taiyaku.library.json` is the one this branch moved** (review 3, Minor
/// 12): it used to be written unconditionally, so every `--in-place` export
/// left a `{"taiyaku_library": 1, "updated": "..."}` in every work directory
/// in the reader's library, whose only field changed on every run.
#[test]
fn a_bare_work_exports_only_the_two_documents_that_always_exist() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);

    let written = work(&conn, id, THE_READER, Some(dir.path()))
        .expect("export")
        .written;
    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["taiyaku.json", "taiyaku.notes.json"],
        "metadata, progress and truth are absent, not empty"
    );
    let work_dir = dir.path().join("bare");
    assert!(work_dir.join("taiyaku.json").is_file(), "the two that do");
    assert!(!work_dir.join("taiyaku.library.json").exists());
    assert!(!work_dir.join("taiyaku.progress.json").exists());
    assert!(!work_dir.join("taiyaku.truth.json").exists());
}

/// **Whole-branch review 3, Minor 12, in place**: the case the finding was
/// observed on. A reader with a real library runs `export --all --in-place` to
/// bring their sibling documents up to date, and every work that has never
/// been hand-edited — which is all of them, since nothing sets this metadata
/// yet — got a new file holding nothing but a version and a timestamp, and a
/// fresh timestamp on every run after that.
///
/// Asserted across two exports, because "it is created" and "it changes every
/// time" are two complaints and the second is the one that makes a library
/// permanently dirty.
#[test]
fn an_in_place_export_creates_no_library_document_for_a_work_nobody_has_edited() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);
    let metadata_path = dir.path().join("taiyaku.library.json");

    let first = work(&conn, id, THE_READER, None).expect("first export");
    assert!(
        !metadata_path.exists(),
        "nothing about this work has been hand-fixed, so there is nothing to file"
    );
    assert!(!first.written.contains(&metadata_path));
    assert!(
        first.removed.is_empty(),
        "and nothing was there to take away either: {:?}",
        first.removed
    );

    let second = work(&conn, id, THE_READER, None).expect("second export");
    assert!(
        !metadata_path.exists(),
        "and a second export does not create one to churn its `updated` field"
    );
    assert_eq!(first.written, second.written);
}

/// The other side of the same decision, and the reason it is `is_empty`
/// rather than a new flag: the moment there *is* something to say, the
/// document is written, and clearing it again takes the file away — the
/// snapshot rule `<stem>.truth.json` already lives by.
#[test]
fn hand_fixed_metadata_is_written_and_clearing_it_removes_the_document() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);
    let metadata_path = dir.path().join("taiyaku.library.json");

    conn.execute("UPDATE work SET volume = 3.0 WHERE id = ?1", [id])
        .expect("the reader files this directory as volume 3");
    let written = work(&conn, id, THE_READER, None).expect("export").written;
    assert!(
        written.contains(&metadata_path),
        "a decision worth keeping is worth writing down: {written:?}"
    );
    let text = std::fs::read_to_string(&metadata_path).expect("reads");
    assert!(text.contains("\"volume\""), "{text}");

    conn.execute("UPDATE work SET volume = NULL WHERE id = ?1", [id])
        .expect("and then changes their mind");
    let second = work(&conn, id, THE_READER, None).expect("second export");
    assert_eq!(
        second.removed,
        vec![metadata_path.clone()],
        "a snapshot of nothing is no file, and the removal is reported"
    );
    assert!(!metadata_path.exists(), "really gone, not merely listed");
}

/// **Whole-branch review 3, Minor 5.** The removal loop's `NotFound` guard is
/// the difference between "most works never had this document" and "this
/// document is still there, asserting state the database no longer holds".
/// Replacing the guard with `true` — swallowing every removal error — passed
/// the whole suite, and `work`'s own `# Errors` clause promises the opposite.
///
/// A directory at the path is the cheapest way to make `remove_file` fail for
/// a reason that is not absence, and it is not contrived: an interrupted
/// `rsync`, or a reader who made one by hand, produces exactly this. It also
/// slips past the `kept` guard above on its own terms — `path.is_file()` is
/// false for a directory — so this reaches the removal with nothing else
/// changed.
#[test]
fn a_stale_document_that_cannot_be_removed_fails_the_export_rather_than_passing() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);
    let truth_path = dir.path().join("taiyaku.truth.json");
    std::fs::create_dir(&truth_path).expect("something that is not a file at that path");

    let err = work(&conn, id, THE_READER, None)
        .expect_err("a removal that failed is not a snapshot taken");
    assert!(
        err.contains("removing the stale"),
        "the message names the step it failed at: {err}"
    );
    assert!(
        err.contains("taiyaku.truth.json"),
        "and the document it could not take away: {err}"
    );
    assert!(
        truth_path.is_dir(),
        "and it really was still there, so the error is about the world rather \
         than about an unlucky race"
    );
}

/// A work whose `sidecar_path` is NULL — one that arrived by a route that
/// never had a path — has no directory to default to when `dir` is not
/// given. That is refused with an error naming the work, not a panic and
/// not a silent no-op.
///
/// There is no importer that leaves `sidecar_path` NULL from the start, so
/// this reaches the state the column allows the direct way: import
/// normally, then clear it, the same shape `import.rs`'s own
/// `clear_stale_path_claim` produces when a path changes hands.
#[test]
fn exporting_a_work_with_no_sidecar_path_and_no_dir_is_a_named_error() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = crate::import::sidecar(&conn, &doc("Orphan", "cc"), Path::new("/x/taiyaku.json"))
        .expect("import");
    conn.execute("UPDATE work SET sidecar_path = NULL WHERE id = ?1", [id])
        .expect("clear the path");

    let err = work(&conn, id, THE_READER, None).expect_err("nowhere to default to");
    assert!(
        err.contains("orphan"),
        "the error must name the work by slug rather than failing obscurely: {err}"
    );
}

/// The same NULL `sidecar_path`, but with `dir` given: there is still no
/// filename to take a stem from, so the fallback is the same `taiyaku` stem
/// a sidecar with no stem at all gets (`stem_of`'s own convention).
#[test]
fn exporting_a_work_with_no_sidecar_path_but_a_dir_falls_back_to_the_taiyaku_stem() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = crate::import::sidecar(&conn, &doc("Orphan", "dd"), Path::new("/x/taiyaku.json"))
        .expect("import");
    conn.execute("UPDATE work SET sidecar_path = NULL WHERE id = ?1", [id])
        .expect("clear the path");

    let written = work(&conn, id, THE_READER, Some(dir.path()))
        .expect("export")
        .written;
    assert!(written.iter().any(|p| p.ends_with("taiyaku.json")));
    assert!(dir.path().join("orphan").join("taiyaku.json").exists());
}

/// **The C1 defect of whole-branch review 2, at this module's own level.**
/// Every work in a real library keeps its sidecar at `<dir>/taiyaku.json`, so
/// the stem is the same string for every work and a flat `dir` gave two works
/// the same five paths — the second silently overwriting the first, with
/// `atomic::write` making each write clean enough that nothing looked wrong.
/// The per-work subdirectory named for `slug` is what keeps them apart, and
/// `slug` is `UNIQUE` in the schema, so no two works can land in one.
#[test]
fn two_works_whose_sidecars_share_a_filename_do_not_collide_under_one_dir() {
    let a_dir = tempdir();
    let b_dir = tempdir();
    let out = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();

    // The convention: both sidecars are named `taiyaku.json`, in different
    // directories, exactly as every work in this project's corpus is.
    let a = crate::import::sidecar(
        &conn,
        &doc("Alpha", "a1"),
        &a_dir.path().join("taiyaku.json"),
    )
    .expect("import a");
    let b = crate::import::sidecar(
        &conn,
        &doc("Beta", "b1"),
        &b_dir.path().join("taiyaku.json"),
    )
    .expect("import b");

    let written_a = work(&conn, a, THE_READER, Some(out.path()))
        .expect("export a")
        .written;
    let written_b = work(&conn, b, THE_READER, Some(out.path()))
        .expect("export b")
        .written;
    assert!(
        written_a.iter().all(|p| !written_b.contains(p)),
        "no path may be written twice: {written_a:?} against {written_b:?}"
    );

    for (slug, title) in [("alpha", "Alpha"), ("beta", "Beta")] {
        let text = std::fs::read_to_string(out.path().join(slug).join("taiyaku.json"))
            .unwrap_or_else(|e| panic!("{slug}/taiyaku.json: {e}"));
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(
            parsed["work"]["title"].as_str(),
            Some(title),
            "`{slug}` must hold its own work, not the last one exported"
        );
    }
}

/// **Clearing your labelling and exporting takes the old document away.**
/// `taiyaku.truth.json` is written only when `state::truth` is `Some`, and
/// leaving a stale one behind made `--in-place` an overlay rather than a
/// snapshot: the directory afterwards asserted boxes the reader had deleted.
/// The C3 fix makes that file inert against *this* library — `migrate.rs`
/// records the document as considered, so a later import will not read it back
/// — but the five documents are also how a library travels, and copying this
/// directory to another machine would resurrect the labelling there.
///
/// Cleared through `put_truth` with an empty document, which is the path the
/// labeller actually takes (it sends the whole document on every write), not
/// by deleting rows behind the API's back.
#[test]
fn clearing_the_labelling_and_exporting_removes_the_stale_truth_document() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_work_with_notes_and_progress(&conn, &dir);

    let first = work(&conn, id, THE_READER, None).expect("first export");
    let truth_path = dir.path().join("taiyaku.truth.json");
    assert!(
        first.written.contains(&truth_path),
        "the fixture must actually write one, or this test proves nothing"
    );
    assert!(first.removed.is_empty(), "nothing stale on a first export");

    crate::state::put_truth(
        &conn,
        id,
        &TruthDoc {
            taiyaku_truth: taiyaku_core::TRUTH_VERSION,
            editions: std::collections::BTreeMap::new(),
        },
    )
    .expect("the labeller clears every box");
    assert!(
        crate::state::truth(&conn, id).expect("reads").is_none(),
        "clearing must really leave no truth rows"
    );

    let second = work(&conn, id, THE_READER, None).expect("second export");
    assert_eq!(
        second.removed,
        vec![truth_path.clone()],
        "the stale document is removed, and the removal is reported"
    );
    assert!(
        !truth_path.exists(),
        "and it is really gone from the directory, not merely listed"
    );
    assert!(
        !second.written.contains(&truth_path),
        "a removed document is not also a written one"
    );
    assert!(
        dir.path().join("taiyaku.progress.json").is_file(),
        "the progress document is untouched: its state is still Some"
    );
}

/// The other half of the same rule: a work that never had either optional
/// document reports no removals. A `remove_file` on a path that is not there
/// is the ordinary case — most works have neither — and must not be reported
/// as a deletion or fail the export.
#[test]
fn a_bare_work_reports_no_removals_for_documents_that_were_never_there() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);

    let exported = work(&conn, id, THE_READER, Some(dir.path())).expect("export");
    assert!(
        exported.removed.is_empty(),
        "nothing was there to remove: {:?}",
        exported.removed
    );
}

/// A note for `user_id`, on the fixture work's own edition.
fn note_for(conn: &rusqlite::Connection, work_id: i64, user_id: i64, id: &str, body: &str) {
    crate::state::put_notes(
        conn,
        work_id,
        user_id,
        &crate::Notes {
            work: "t".to_string(),
            notes: vec![crate::Note {
                id: id.to_string(),
                edition: "raw".to_string(),
                slot: 0,
                region: None,
                color: None,
                body: body.to_string(),
                created: "2026-08-18T00:00:00Z".to_string(),
                updated: "2026-08-18T00:00:00Z".to_string(),
            }],
        },
    )
    .expect("put_notes");
}

fn add_bob(conn: &rusqlite::Connection) {
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'bob', '!', 'user')",
        [],
    )
    .expect("adds bob");
}

/// **§5.1, decided 2026-08-18.** One work, two readers with notes, one export.
///
/// The file carries one set — that is what a sidecar is, and versioning the
/// format to hold several was declined — so the report names the reader whose
/// notes are *not* in it. Nothing is dropped in silence, which is the rule
/// `suppressed`, `rekeyed`, `merged` and `kept_editions` were all built on: any
/// outcome that changes what a person sees and cannot be inferred from the other
/// counts gets a field.
#[test]
fn an_export_names_the_readers_it_did_not_write() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let dir = tempdir();
    let work_id = crate::import::sidecar(&conn, &doc("T", "aa"), &dir.path().join("taiyaku.json"))
        .expect("import");
    add_bob(&conn);
    note_for(&conn, work_id, 1, "01HALICE", "alices-note");
    note_for(&conn, work_id, 2, "01HBOBBB", "bobs-note");

    let out = tempdir();
    let exported = work(&conn, work_id, 1, Some(out.path())).expect("exports");

    assert_eq!(
        exported.other_users,
        vec!["bob".to_string()],
        "the export did not name the reader whose notes it left behind"
    );

    let written = exported
        .written
        .iter()
        .find(|path| path.ends_with("taiyaku.notes.json"))
        .expect("notes were written");
    let text = std::fs::read_to_string(written).expect("reads back");
    assert!(
        text.contains("alices-note"),
        "the requesting reader's notes are missing: {text}"
    );
    assert!(
        !text.contains("bobs-note"),
        "a sidecar carries one reader's notes: {text}"
    );
}

/// Exporting *as* the second reader writes theirs and names the first, so the
/// rule is about who asked rather than about who happens to be user 1.
#[test]
fn the_export_follows_whoever_asked_for_it() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let dir = tempdir();
    let work_id = crate::import::sidecar(&conn, &doc("T", "aa"), &dir.path().join("taiyaku.json"))
        .expect("import");
    add_bob(&conn);
    note_for(&conn, work_id, 1, "01HALICE", "alices-note");
    note_for(&conn, work_id, 2, "01HBOBBB", "bobs-note");

    let out = tempdir();
    let exported = work(&conn, work_id, 2, Some(out.path())).expect("exports");

    assert_eq!(exported.other_users, vec!["local".to_string()]);
    let written = exported
        .written
        .iter()
        .find(|path| path.ends_with("taiyaku.notes.json"))
        .expect("notes were written");
    let text = std::fs::read_to_string(written).expect("reads back");
    assert!(text.contains("bobs-note"), "{text}");
    assert!(!text.contains("alices-note"), "{text}");
}

/// The ordinary single-reader library reports nobody, so an export does not
/// grow a line telling a person about a situation they are not in.
#[test]
fn a_single_reader_export_names_nobody() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let dir = tempdir();
    let work_id = crate::import::sidecar(&conn, &doc("T", "aa"), &dir.path().join("taiyaku.json"))
        .expect("import");
    note_for(&conn, work_id, 1, "01HONLY", "only-mine");

    let out = tempdir();
    let exported = work(&conn, work_id, 1, Some(out.path())).expect("exports");
    assert!(exported.other_users.is_empty());
}

/// A second reader holding only a *position* counts too: `progress` is per-user
/// exactly as `note` is, and a reading position left behind is as invisible as a
/// note left behind.
#[test]
fn a_reader_with_only_a_position_is_named_as_well() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let dir = tempdir();
    let work_id = crate::import::sidecar(&conn, &doc("T", "aa"), &dir.path().join("taiyaku.json"))
        .expect("import");
    add_bob(&conn);
    crate::state::put_progress(
        &conn,
        work_id,
        2,
        &crate::Progress {
            edition: "raw".to_string(),
            slot: 7,
        },
    )
    .expect("put_progress");

    let out = tempdir();
    let exported = work(&conn, work_id, 1, Some(out.path())).expect("exports");
    assert_eq!(exported.other_users, vec!["bob".to_string()]);
}

/// **The cause the `--in-place --user` refusal only papered over.**
///
/// `absorbed` is keyed `(work_id, document)` with no user, so an in-place export
/// as reader 1 — the *default*, needing no flag at all — found no notes and no
/// position for them, marked both documents stale, and had the "never absorbed"
/// guard satisfied by reader 2's own import. Reader 2's `taiyaku.notes.json` was
/// overwritten with an empty document and their `taiyaku.progress.json` deleted.
///
/// Found by the second whole-branch review, which reproduced it end to end.
#[test]
fn an_in_place_export_does_not_delete_another_readers_documents() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let dir = tempdir();
    let sidecar = dir.path().join("taiyaku.json");
    let work_id = crate::import::sidecar(&conn, &doc("T", "aa"), &sidecar).expect("import");
    add_bob(&conn);

    // Only bob reads this work — the ordinary case for a shared library where
    // one person follows one series.
    note_for(&conn, work_id, 2, "01HBOBBB", "bobs-note");
    crate::state::put_progress(
        &conn,
        work_id,
        2,
        &crate::Progress {
            edition: "raw".to_string(),
            slot: 4,
        },
    )
    .expect("bob's position");

    // Bob's own documents, beside the sidecar, as an export would have left them.
    let exported = work(&conn, work_id, 2, None).expect("bob exports in place");
    let notes_path = dir.path().join("taiyaku.notes.json");
    let progress_path = dir.path().join("taiyaku.progress.json");
    assert!(exported.written.contains(&notes_path));
    assert!(progress_path.is_file());

    // Now the default in-place export, as reader 1, with no flag at all.
    let as_alice = work(&conn, work_id, 1, None).expect("exports");

    assert!(
        progress_path.is_file(),
        "reader 1's export deleted reader 2's reading position from their own library"
    );
    let left = std::fs::read_to_string(&notes_path).expect("reads");
    assert!(
        left.contains("bobs-note"),
        "reader 1's export overwrote reader 2's notes with an empty document: {left}"
    );
    assert_eq!(
        as_alice.other_users,
        vec!["bob".to_string()],
        "and it still says whose state it did not write"
    );
}

/// The documents that are not per-user are decided by the marker alone — there
/// is no "whose truth" to ask about, and treating them per-user would make an
/// export decline to write a work's own metadata.
#[test]
fn a_document_that_is_not_per_user_is_settled_by_the_marker_alone() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let dir = tempdir();
    let work_id = crate::import::sidecar(&conn, &doc("T", "aa"), &dir.path().join("taiyaku.json"))
        .expect("import");
    add_bob(&conn);
    note_for(&conn, work_id, 2, "01HBOBBB", "bobs-note");

    // Alice holds no notes, so `notes` is bob's and must not be touched — but
    // `truth` and `library` are the library's, not bob's.
    assert!(!crate::migrate::absorbed_for(&conn, work_id, "notes", 1).expect("reads"));
    crate::migrate::record_absorbed(&conn, work_id, "truth", true).expect("marks");
    assert!(
        crate::migrate::absorbed_for(&conn, work_id, "truth", 1).expect("reads"),
        "ground truth is library-wide, so no user owns it"
    );
}
