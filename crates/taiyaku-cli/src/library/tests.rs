use super::*;
use taiyaku_library::Library;

/// A temp root nobody else is using, removed first in case a previous run
/// left one behind and created fresh — `cache/tests.rs`'s own `scratch`
/// helper, this module's copy of the same idiom.
///
/// **The process id is part of the name**, for the reason
/// `taiyaku-library/src/migrate/tests.rs`'s `scratch` sets out in full: `name`
/// alone separates tests within one run and nothing between runs, so a
/// `cargo test` overlapping a `cargo mutants` or a second terminal has two
/// processes sharing one directory, and either one's `remove_dir_all` deletes
/// the other's fixture mid-test. Review 3's Minor 13 added it there and to
/// `tests/cli.rs`'s `db_path`; this copy and `cache/tests.rs`'s were missed
/// (delta review M6).
///
/// **It buys the collision fix at the cost of accumulation, knowingly**, and
/// that is the trade every helper on this branch that took the pid has made —
/// `migrate/tests.rs`, `cache/tests.rs`, `score/tests.rs` and `tests/cli.rs`'s
/// `db_path` alike. A fixed name is reused, so `/tmp` holds one directory per
/// test forever; a per-run name is not, so it holds one per test **per run**.
/// `taiyaku-serve/src/test_support.rs`'s `Fixture` is the shape that answers
/// both — unique name *plus* a `Drop` that removes it, added there after 9,147
/// leaked directories were measured — and giving these five helpers the same
/// guard is a separate piece of work from the collision this comment is about.
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "taiyaku-test-library-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// A minimal, valid sidecar: one hub edition of one member. `content_key`
/// hashes the member's *declared* `sha256`, not its actual bytes, so no
/// archive needs to exist on disk for any test in this module — the same
/// fixture shape `taiyaku-library/src/import/tests.rs`'s own `doc` uses.
fn sidecar_json(hub_sha: &str) -> String {
    sidecar_json_titled("T", hub_sha)
}

/// `sidecar_json` with a title of the caller's choosing — so two works in one
/// library can be told apart by what their exported document *says* rather
/// than by which file it came from, which is the only way to see the `--dir`
/// collision below at all: the colliding files are byte-identical in
/// everything except the work they describe.
fn sidecar_json_titled(title: &str, hub_sha: &str) -> String {
    format!(
        r#"{{"taiyaku":1,"work":{{"title":"{title}","reading":"rtl"}},
             "editions":{{"raw":{{"label":"R","members":[
               {{"path":"raw/v01.cbz","sha256":"{hub_sha}","pages":10}}]}}}},
             "hub":"raw","alignments":[]}}"#
    )
}

fn write_fixture(dir: &Path, hub_sha: &str) -> PathBuf {
    write_fixture_titled(dir, "T", hub_sha)
}

/// The filename is always `taiyaku.json` — that is this project's documented
/// convention, and the reason a work's identity is its `unique_slug` rather
/// than its filename — so every fixture written by this helper shares a stem
/// with every other one.
fn write_fixture_titled(dir: &Path, title: &str, hub_sha: &str) -> PathBuf {
    let path = dir.join("taiyaku.json");
    std::fs::write(&path, sidecar_json_titled(title, hub_sha)).unwrap();
    path
}

/// `taiyaku.<suffix>` beside a fixture written by `write_fixture` — this
/// module's own copy of `migrate/tests.rs`'s identical helper, for the
/// sibling-migration tests below.
fn write_sibling(dir: &Path, suffix: &str, text: &str) {
    std::fs::write(dir.join(format!("taiyaku.{suffix}")), text).unwrap();
}

// -- db_path: pure, so every branch is testable with no real environment ---

#[test]
fn a_flag_pins_the_db_path_over_either_variable() {
    let path = db_path(
        Some("/explicit/db.sqlite"),
        Some(std::ffi::OsStr::new("/xdg")),
        Some(std::ffi::OsStr::new("/home")),
    )
    .expect("resolves");
    assert_eq!(path, PathBuf::from("/explicit/db.sqlite"));
}

#[test]
fn an_empty_flag_is_treated_as_absent_and_falls_through_to_the_variables() {
    // `--db ""` is a single unset shell variable away from what a caller
    // typed; pinning the empty path would resolve under the working
    // directory, which is exactly the defect this module exists to avoid.
    // `cache_root`'s own doc comment makes the identical argument for
    // `--cache`.
    let path = db_path(Some(""), Some(std::ffi::OsStr::new("/xdg")), None).expect("resolves");
    assert_eq!(path, PathBuf::from("/xdg/taiyaku/library.db"));
}

#[test]
fn xdg_data_home_wins_over_home_when_both_are_absolute() {
    let path = db_path(
        None,
        Some(std::ffi::OsStr::new("/xdg")),
        Some(std::ffi::OsStr::new("/home")),
    )
    .expect("resolves");
    assert_eq!(path, PathBuf::from("/xdg/taiyaku/library.db"));
}

#[test]
fn a_relative_xdg_data_home_is_ignored_in_favour_of_home() {
    let path = db_path(
        None,
        Some(std::ffi::OsStr::new("relative")),
        Some(std::ffi::OsStr::new("/home")),
    )
    .expect("resolves");
    assert_eq!(path, PathBuf::from("/home/.local/share/taiyaku/library.db"));
}

#[test]
fn neither_flag_nor_variable_is_a_refusal_not_a_relative_fallback() {
    let err = db_path(None, None, None).unwrap_err();
    assert!(err.contains("--db"), "{err}");
    assert!(err.contains("XDG_DATA_HOME"), "{err}");
    assert!(err.contains("HOME"), "{err}");
}

#[test]
fn resolve_db_path_creates_the_parent_directory() {
    let root = scratch("resolve-creates-parent");
    let db = root.join("nested").join("further").join("library.db");
    let resolved = resolve_db_path(Some(db.to_str().unwrap())).expect("resolves");
    assert_eq!(resolved, db);
    assert!(
        db.parent().unwrap().is_dir(),
        "the parent directory was created so rusqlite can open a file under it"
    );
}

// -- canonical_sidecar_path --------------------------------------------------

#[test]
fn a_nonexistent_path_is_refused_naming_it_exactly_as_typed() {
    let err = canonical_sidecar_path(Path::new("does-not-exist-anywhere.json")).unwrap_err();
    assert!(err.contains("does-not-exist-anywhere.json"), "{err}");
}

#[test]
fn two_spellings_of_the_same_file_canonicalise_identically() {
    let root = scratch("canon-two-spellings");
    let path = write_fixture(&root, "aa");
    let dotted = root.join(".").join("taiyaku.json");
    // `Path`'s own `PartialEq` compares normalised components, where a `.`
    // that is not the leading component is already discarded — so `assert_ne!`
    // on the `PathBuf`s themselves would pass regardless of whether this test
    // proves anything. What `find_by_path`'s `SELECT ... WHERE sidecar_path =
    // ?1` actually compares is the *string* SQLite stores, so that is what
    // must differ here.
    assert_ne!(
        path.to_string_lossy(),
        dotted.to_string_lossy(),
        "the two spellings must actually differ as strings, or this proves nothing"
    );

    let a = canonical_sidecar_path(&path).expect("canonicalises");
    let b = canonical_sidecar_path(&dotted).expect("canonicalises");
    assert_eq!(a, b, "both spellings name the same real file");
}

// -- merge_summary ------------------------------------------------------

#[test]
fn a_plain_import_with_no_merge_outcomes_summarises_to_nothing() {
    assert_eq!(merge_summary(&[]), "");
}

#[test]
fn merge_outcomes_are_counted_by_kind_and_the_three_counted_variants_are_summed() {
    let outcomes = vec![
        Merged::Added,
        Merged::Added,
        Merged::Replaced,
        Merged::CarriedHandRuns(3),
        Merged::CarriedHandRuns(2),
        Merged::DroppedCredits(5),
        Merged::DroppedHandRuns(4),
    ];
    assert_eq!(
        merge_summary(&outcomes),
        " (added=2 replaced=1 carried_hand_runs=5 dropped_credits=5 dropped_hand_runs=4)"
    );
}

// -- migration_summary ---------------------------------------------------

#[test]
fn a_migration_with_no_sibling_files_at_all_summarises_to_nothing() {
    assert_eq!(
        migration_summary(&taiyaku_library::migrate::SiblingMigration::default()),
        ""
    );
}

#[test]
fn a_migration_summary_names_every_document_that_had_a_sibling_file() {
    let report = taiyaku_library::migrate::SiblingMigration {
        progress: Some(taiyaku_library::migrate::ProgressOutcome::Applied),
        notes: Some(taiyaku_library::migrate::NotesOutcome::Applied { count: 3 }),
        truth: Some(taiyaku_library::migrate::TruthOutcome::AlreadyPresent),
        library: Some(taiyaku_library::migrate::LibraryOutcome::Malformed(
            "bad json".to_string(),
        )),
    };
    assert_eq!(
        migration_summary(&report),
        " (siblings: progress=migrated notes=migrated:3 truth=kept library=malformed)"
    );
}

/// `kept` and `unread` are different words because they are different facts,
/// and printing the first for the second is how whole-branch review 3's C1
/// stayed invisible: `kept` is documented as "the database already held state
/// for that document", and it was printed in the line immediately before an
/// export that deleted a `taiyaku.truth.json` the database had never held.
#[test]
fn a_document_neither_side_holds_is_reported_as_unread_rather_than_kept() {
    let report = taiyaku_library::migrate::SiblingMigration {
        progress: Some(taiyaku_library::migrate::ProgressOutcome::Unread),
        notes: Some(taiyaku_library::migrate::NotesOutcome::Unread),
        truth: Some(taiyaku_library::migrate::TruthOutcome::Unread),
        library: Some(taiyaku_library::migrate::LibraryOutcome::Unread),
    };
    assert_eq!(
        migration_summary(&report),
        " (siblings: progress=unread notes=unread truth=unread library=unread)"
    );
}

// -- command dispatch --------------------------------------------------

#[test]
fn library_refuses_an_unknown_subcommand() {
    let err = command(&["frobnicate".to_string()]).unwrap_err();
    assert!(err.contains("frobnicate"), "{err}");
    assert!(err.contains("import"), "{err}");
    assert!(err.contains("export"), "{err}");
}

#[test]
fn library_with_no_subcommand_is_a_refusal_not_a_panic() {
    let err = command(&[]).unwrap_err();
    assert!(err.contains("import"), "{err}");
    assert!(err.contains("export"), "{err}");
}

// -- import: argument validation, no filesystem or database touched -------

#[test]
fn import_refuses_more_than_one_path() {
    let err = import(&["a.json".to_string(), "b.json".to_string()]).unwrap_err();
    assert!(err.contains("usage: taiyaku library import"), "{err}");
}

#[test]
fn import_refuses_an_unknown_flag() {
    let err = import(&["a.json".to_string(), "--bogus".to_string()]).unwrap_err();
    assert!(err.contains("--bogus"), "{err}");
    assert!(err.contains("--db"), "{err}");
}

// -- export: argument validation, no filesystem or database touched -------

#[test]
fn export_refuses_all_together_with_a_work_name() {
    let err = export(&["some-work".to_string(), "--all".to_string()]).unwrap_err();
    assert!(err.contains("--all takes no work name"), "{err}");
}

#[test]
fn export_refuses_neither_a_work_name_nor_all() {
    let err = export(&[]).unwrap_err();
    assert!(err.contains("name exactly one work"), "{err}");
}

/// C2: `export::target_dir` defaults to `sidecar_path`'s own parent, so an
/// export with neither flag would silently overwrite the file a work was
/// imported from — reverting whatever a reader hand-corrected since. This
/// must refuse before any database is opened, matching the "no filesystem or
/// database touched" tests above it, and the message must name both ways
/// out so a caller does not have to guess which one they wanted.
///
/// `--db` is still passed, pointed at a scratch path nothing ever creates:
/// belt and braces against the refusal being reordered after
/// `resolve_db_path` later, which — unguarded, as this test caught once
/// already while red — reads `$XDG_DATA_HOME`/`$HOME` and creates a real
/// library there.
#[test]
fn export_refuses_without_dir_or_in_place_and_names_both_options() {
    let root = scratch("export-refuses-neither-flag");
    let db = root.join("unused.db");
    let err = export(&[
        "some-work".to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ])
    .unwrap_err();
    assert!(err.contains("--dir"), "{err}");
    assert!(err.contains("--in-place"), "{err}");
    assert!(
        !db.exists(),
        "the refusal must come before a database is ever opened"
    );
}

/// The same refusal under `--all`: every work is a candidate for the same
/// silent overwrite, not just a single named one.
#[test]
fn export_all_refuses_without_dir_or_in_place_and_names_both_options() {
    let root = scratch("export-all-refuses-neither-flag");
    let db = root.join("unused.db");
    let err = export(&[
        "--all".to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ])
    .unwrap_err();
    assert!(err.contains("--dir"), "{err}");
    assert!(err.contains("--in-place"), "{err}");
    assert!(
        !db.exists(),
        "the refusal must come before a database is ever opened"
    );
}

// -- import/export, end to end over a real on-disk database ---------------

#[test]
fn export_writes_the_documents_for_a_previously_imported_work() {
    let root = scratch("export-roundtrip");
    let path = write_fixture(&root, "dd");
    let db = root.join("library.db");
    let out_dir = root.join("out");

    let library = Library::open(&db).expect("opens a real on-disk library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    import_canonical(&library, &canonical).expect("imports");
    drop(library);

    // `sidecar_json`'s title is "T", which `unique_slug` lowercases to "t"
    // with nothing else in a fresh library to collide against.
    let args = [
        "t".to_string(),
        "--dir".to_string(),
        out_dir.to_str().unwrap().to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    export(&args).expect("exports");

    // Under `--dir`, a work's documents land in a subdirectory named for its
    // slug — see `export::work`'s own doc comment, and the `--all` collision
    // test below for why.
    let work_dir = out_dir.join("t");
    assert!(
        work_dir.join("taiyaku.json").exists(),
        "the sidecar itself was exported"
    );
    assert!(
        work_dir.join("taiyaku.notes.json").exists(),
        "notes.json is always written, per export::work's own doc comment"
    );
}

/// **Whole-branch review 3's C1, end to end through the two commands that
/// produced it.** Import a work with no siblings; restore its
/// `taiyaku.truth.json` from a backup (or copy it back from the laptop it was
/// labelled on); import again — which is what `taiyaku serve <sidecar>...`
/// does on every start; export in place. The labelling was deleted, and the
/// import line before it said `truth=kept`, whose documented meaning is that
/// the database already held it. The database had never seen it.
#[test]
fn export_in_place_does_not_delete_a_sibling_the_database_never_absorbed() {
    let root = scratch("export-in-place-keeps-unabsorbed");
    let path = write_fixture(&root, "kk");
    let db = root.join("library.db");
    let truth_path = root.join("taiyaku.truth.json");

    let library = Library::open(&db).expect("opens a real on-disk library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (_id, _outcomes, first) = import_canonical(&library, &canonical).expect("imports");
    assert_eq!(
        migration_summary(&first),
        "",
        "nothing beside the sidecar on the first import"
    );

    let labelling = r#"{"taiyaku_truth":1,"editions":{"raw":{"members":[],"pages":{
        "3":[{"x":0.1,"y":0.2,"w":0.3,"h":0.05,"axis":"v","kind":"dialogue","ignore":false,"text":"hours of it"}]
    }}}}"#;
    std::fs::write(&truth_path, labelling).expect("the reader copies their labelling back");

    let (_id, _outcomes, second) = import_canonical(&library, &canonical).expect("imports again");
    let summary = migration_summary(&second);
    assert!(
        summary.contains("truth=unread"),
        "the import has to say the database holds nothing for it: {summary}"
    );
    drop(library);

    let args = [
        "t".to_string(),
        "--in-place".to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    export(&args).expect("exports in place");

    assert_eq!(
        std::fs::read_to_string(&truth_path).unwrap_or_default(),
        labelling,
        "the labelling must still be on disk, byte for byte"
    );
}

/// C2's fix, proven the way the reviewer reproduced the defect: a hand edit
/// made to the sidecar after import is gone once `--in-place` exports the
/// database's copy back over it. That is `--in-place`'s whole point — export
/// intentionally reverting a stale file is the fix working, not a new bug —
/// so this pins the flag actually reaches `target_dir`'s default-to-parent
/// behaviour rather than the refusal added above.
#[test]
fn export_in_place_writes_over_the_sidecar_it_was_imported_from() {
    let root = scratch("export-in-place");
    let path = write_fixture(&root, "hh");
    let db = root.join("library.db");

    let library = Library::open(&db).expect("opens a real on-disk library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    import_canonical(&library, &canonical).expect("imports");
    drop(library);

    // A hand edit to the file the database no longer knows about — the same
    // shape as the reviewer's repro (a title, a `spreads` list).
    std::fs::write(
        &path,
        sidecar_json("hh").replace("\"T\"", "\"HAND EDITED\""),
    )
    .expect("hand-edits the source file");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("HAND EDITED"),
        "the hand edit must actually be on disk, or this test proves nothing"
    );

    let args = [
        "t".to_string(),
        "--in-place".to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    export(&args).expect("exports in place");

    let after = std::fs::read_to_string(&path).expect("reads the sidecar back");
    assert!(
        !after.contains("HAND EDITED"),
        "--in-place must write the database's copy over the source file: {after}"
    );
}

/// The unchanged case: `--dir` still lands the five documents in the given
/// directory and never touches the source sidecar, exactly as before this
/// fix — the guard above only fires when neither flag is given.
#[test]
fn export_with_dir_never_touches_the_source_sidecar() {
    let root = scratch("export-dir-unchanged");
    let path = write_fixture(&root, "ii");
    let db = root.join("library.db");
    let out_dir = root.join("out");

    let library = Library::open(&db).expect("opens a real on-disk library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    import_canonical(&library, &canonical).expect("imports");
    drop(library);

    let before = std::fs::read_to_string(&path).expect("reads the source");

    let args = [
        "t".to_string(),
        "--dir".to_string(),
        out_dir.to_str().unwrap().to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    export(&args).expect("exports");

    assert!(out_dir.join("t").join("taiyaku.json").exists());
    let after = std::fs::read_to_string(&path).expect("reads the source again");
    assert_eq!(before, after, "--dir must not write back over the source");
}

#[test]
fn export_refuses_an_unknown_work_slug() {
    let root = scratch("export-unknown-slug");
    let db = root.join("library.db");
    drop(Library::open(&db).expect("opens"));

    let args = [
        "nope".to_string(),
        "--in-place".to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    let err = export(&args).unwrap_err();
    assert!(err.contains("no work `nope`"), "{err}");
}

#[test]
fn export_all_covers_every_work_in_the_library() {
    let root = scratch("export-all");
    let a = write_fixture(&root, "ee");
    let sub = root.join("second");
    std::fs::create_dir_all(&sub).unwrap();
    let b = write_fixture(&sub, "ff");
    let db = root.join("library.db");

    let library = Library::open(&db).expect("opens");
    import_canonical(&library, &canonical_sidecar_path(&a).unwrap()).expect("imports a");
    import_canonical(&library, &canonical_sidecar_path(&b).unwrap()).expect("imports b");
    drop(library);

    // No --dir, so this is exactly the in-place case C2 gates: each work
    // exports beside its own sidecar_path. --in-place says that overwrite is
    // intended here. (Under one shared --dir the two fixtures share the
    // filename `taiyaku.json`; the test below is what pins that case.)
    let args = [
        "--all".to_string(),
        "--in-place".to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    export(&args).expect("exports both");

    assert!(a.parent().unwrap().join("taiyaku.notes.json").exists());
    assert!(b.parent().unwrap().join("taiyaku.notes.json").exists());
}

/// **Minor 1 of whole-branch review 2, closed: one bad work under `--all`
/// does not strand the works after it.** The loop used to propagate with `?`,
/// so a work that could not be exported — a NULL `sidecar_path` under
/// `--in-place`, which `clear_stale_path_claim` can produce — left the works
/// before it written over their sources and the works after it neither
/// written nor named.
///
/// `zeta` is chosen so the bad work sorts *first* by `work.id` (it is imported
/// first), which is the ordering that made the old behaviour worst: nothing
/// after it ran at all. The assertions are on the payload — the good work's
/// documents on disk, the bad work's slug in the error — rather than on the
/// call merely returning `Err`, since returning `Err` is exactly what it did
/// before.
#[test]
fn export_all_writes_what_it_can_and_names_the_work_it_could_not() {
    let root = scratch("export-all-partial");
    let bad_dir = root.join("bad");
    let good_dir = root.join("good");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::create_dir_all(&good_dir).unwrap();
    let bad = write_fixture_titled(&bad_dir, "Zeta", "z155");
    let good = write_fixture_titled(&good_dir, "Gamma", "g155");
    let db = root.join("library.db");

    let library = Library::open(&db).expect("opens");
    import_canonical(&library, &canonical_sidecar_path(&bad).unwrap()).expect("imports the bad");
    import_canonical(&library, &canonical_sidecar_path(&good).unwrap()).expect("imports the good");
    // The shape `clear_stale_path_claim` leaves behind: a work whose row no
    // longer names any file, so `--in-place` has nowhere to write it.
    {
        let conn = library.conn();
        conn.execute(
            "UPDATE work SET sidecar_path = NULL WHERE slug = 'zeta'",
            [],
        )
        .expect("clearing the path claim");
    }
    drop(library);

    let args = [
        "--all".to_string(),
        "--in-place".to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    let err = export(&args).expect_err("a work that could not be exported must exit non-zero");

    assert!(
        err.contains("zeta"),
        "the refusal names the work that failed: {err}"
    );
    assert!(
        good_dir.join("taiyaku.notes.json").is_file(),
        "and the work after it was still written, rather than stranded by the \
         one before it"
    );
    assert!(
        !bad_dir.join("taiyaku.notes.json").exists(),
        "the failing work wrote nothing, since it had nowhere to write"
    );
}

/// **The C1 defect of whole-branch review 2, closed.** Every work in a real
/// library keeps its sidecar at `<dir>/taiyaku.json`, so a flat `--dir` gave
/// every work the same five filenames and each one silently overwrote the
/// one before it: three works in, three files out, the summary line claiming
/// nine documents across three works, and the files holding only the third.
/// `atomic::write` makes each write atomic, so there was not even a
/// truncated file to notice.
///
/// A per-work subdirectory named for the work's `slug` — unique by schema —
/// is what closes it, and it keeps the property the round trip needs: the
/// five documents of one work stay siblings of each other, sharing the stem
/// `migrate.rs` finds them by on the way back in.
#[test]
fn export_all_into_one_dir_keeps_every_work_rather_than_overwriting() {
    let root = scratch("export-all-dir-collision");
    let a_dir = root.join("first");
    let b_dir = root.join("second");
    std::fs::create_dir_all(&a_dir).unwrap();
    std::fs::create_dir_all(&b_dir).unwrap();
    let a = write_fixture_titled(&a_dir, "Alpha", "aa55");
    let b = write_fixture_titled(&b_dir, "Beta", "bb55");
    let db = root.join("library.db");
    let out = root.join("backup");

    let library = Library::open(&db).expect("opens");
    import_canonical(&library, &canonical_sidecar_path(&a).unwrap()).expect("imports a");
    import_canonical(&library, &canonical_sidecar_path(&b).unwrap()).expect("imports b");
    drop(library);

    // Neither work has been hand-edited, so neither exports a
    // `taiyaku.library.json` — but the *second* one has a volume, which is
    // what makes the tally below an arithmetic claim rather than a multiple of
    // the work count: two documents from one work and three from the other,
    // which `+=` and `*=` disagree about and two equal works would not.
    {
        let library = Library::open(&db).expect("reopens");
        let conn = library.conn();
        conn.execute("UPDATE work SET volume = 2.0 WHERE slug = 'beta'", [])
            .expect("the reader files Beta as volume 2");
    }

    let args = [
        "--all".to_string(),
        "--dir".to_string(),
        out.to_str().unwrap().to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    let library = Library::open(&db).expect("reopens");
    let conn = library.conn();
    let targets = all_work_ids(&conn).expect("lists both works");
    let total = export_targets(&conn, &targets, Some(&out)).expect("exports both");
    drop(conn);
    drop(library);
    // The same command through its real entry point, over the same directory,
    // so the argument parsing above the tally is exercised too.
    export(&args).expect("exports both again");

    // Both works are on disk, each in its own subdirectory, each holding the
    // document it was imported from rather than the last work to be written.
    for (slug, title) in [("alpha", "Alpha"), ("beta", "Beta")] {
        let sidecar = out.join(slug).join("taiyaku.json");
        let text = std::fs::read_to_string(&sidecar)
            .unwrap_or_else(|e| panic!("{}: {e}", sidecar.display()));
        let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(
            doc["work"]["title"].as_str(),
            Some(title),
            "`{slug}` must hold its own work, not whichever work was exported last"
        );
        // The siblings landed beside it, not one directory up: `migrate.rs`
        // finds them by stem arithmetic on the sidecar's own path.
        assert!(out.join(slug).join("taiyaku.notes.json").is_file());
    }
    assert!(
        !out.join("alpha").join("taiyaku.library.json").exists(),
        "nothing about Alpha has been hand-fixed, so there is nothing to file"
    );
    assert!(
        out.join("beta").join("taiyaku.library.json").is_file(),
        "and Beta's volume is"
    );

    // **Whole-branch review 3, Minor 11.** The count the summary line reports
    // is the count on disk, and it is a sum rather than a product: `+=` → `*=`
    // read "exported 0 document(s)" on every run and passed the whole suite,
    // because no fixture had enough works and documents to tell them apart.
    let mut on_disk: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&out).expect("reads the backup directory") {
        let dir = entry.expect("entry").path();
        for file in std::fs::read_dir(&dir).expect("reads a work directory") {
            on_disk.push(file.expect("entry").path());
        }
    }
    assert_eq!(
        on_disk.len(),
        5,
        "two documents for Alpha and three for Beta, all five distinct: {on_disk:?}"
    );
    assert_eq!(
        total,
        on_disk.len(),
        "and the figure the command prints is that one, not a product of it"
    );
}

// -- the placeholder-path hazard (Task 13 -> Task 15), tested explicitly --

/// The bridge row exactly as `taiyaku-serve`'s `works::row_id_for_write`
/// mints one. That function is `pub(crate)` to `taiyaku-serve` and
/// unreachable from here, so this replicates its one `INSERT` rather than
/// calling it — same columns, same sentinel `content_key`, same
/// uncanonicalised `sidecar_path`.
fn insert_placeholder_row(library: &Library, slug: &str, sidecar_path: &Path) -> i64 {
    let conn = library.conn();
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub, sidecar_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            format!("bridge:{slug}"),
            slug,
            "T",
            "rtl",
            "raw",
            sidecar_path.to_string_lossy(),
        ],
    )
    .expect("inserting the placeholder row");
    conn.last_insert_rowid()
}

fn work_count(library: &Library) -> i64 {
    library
        .conn()
        .query_row("SELECT COUNT(*) FROM work", [], |row| row.get(0))
        .expect("counting works")
}

/// The hazard, closed: a placeholder row minted by a serve session (a
/// reading position recorded before anything was imported) is *reused*, not
/// orphaned, when the same file is later imported under a different — but
/// equivalent — spelling. This is the property the whole module exists for.
#[test]
fn importing_the_same_file_under_a_different_spelling_reuses_the_placeholder_row() {
    let root = scratch("hazard-reuse");
    let path = write_fixture(&root, "bb");
    let dotted = root.join(".").join("taiyaku.json");

    let library = Library::open_in_memory().expect("library");
    let placeholder_id = insert_placeholder_row(&library, "bridge-slug", &path);

    // The reader's position: exactly what must not be lost.
    let conn = library.conn();
    taiyaku_library::state::put_progress(
        &conn,
        placeholder_id,
        1,
        &taiyaku_library::Progress {
            edition: "raw".to_string(),
            slot: 7,
        },
    )
    .expect("recording a reading position against the placeholder");
    drop(conn);

    let canonical = canonical_sidecar_path(&dotted).expect("canonicalises");
    let (work_id, _outcomes, _migration) = import_canonical(&library, &canonical).expect("imports");

    assert_eq!(
        work_id, placeholder_id,
        "the same real file must reconcile onto the placeholder's row, not mint a second one"
    );
    assert_eq!(work_count(&library), 1, "no second work was minted");

    let recovered = taiyaku_library::state::progress(&library.conn(), work_id, 1)
        .expect("reading progress back")
        .expect("the reading position survived the import");
    assert_eq!(
        recovered.slot, 7,
        "the reader's position is exactly what was recorded, not reset by the merge"
    );
}

/// The discriminating half: without this module's canonicalisation, the
/// identical scenario orphans the placeholder — a second work is minted and
/// the reading position recorded above becomes unreachable through it (its
/// row is still there, but nothing an ordinary caller does ever looks at it
/// again). Calls `taiyaku_library::import::sidecar` directly, bypassing
/// `canonical_sidecar_path`, to show the defect this module's decision
/// closes is real rather than hypothetical.
#[test]
fn without_canonicalisation_the_same_scenario_would_orphan_the_placeholder() {
    let root = scratch("hazard-without-fix");
    let path = write_fixture(&root, "cc");
    let dotted = root.join(".").join("taiyaku.json");
    assert_ne!(
        path.to_string_lossy(),
        dotted.to_string_lossy(),
        "the two spellings must actually differ as strings"
    );

    let library = Library::open_in_memory().expect("library");
    let placeholder_id = insert_placeholder_row(&library, "bridge-slug-2", &path);

    let text = std::fs::read_to_string(&dotted).unwrap();
    let doc = Sidecar::from_json(&text).expect("parses");
    let conn = library.conn();
    let work_id =
        taiyaku_library::import::sidecar(&conn, &doc, &dotted).expect("imports (uncanonicalised)");
    drop(conn);

    assert_ne!(
        work_id, placeholder_id,
        "a differently-spelled but identical path misses the placeholder by string comparison \
         and mints a second row — the defect canonicalisation exists to prevent"
    );
    assert_eq!(
        work_count(&library),
        2,
        "both the placeholder and the freshly-imported row now exist, and the reading \
         position recorded against the first is unreachable through the second"
    );
}

// -- sibling migration on first import (spec C3-fix) -----------------------
//
// `.superpowers/sdd/lib-fix-c3-context.md`: Task 13 moved reading progress,
// notes and labelling truth into rows and nothing ever read the sibling
// files back. These tests go through `import_canonical` itself — the wiring,
// not just `taiyaku_library::migrate::work`'s own unit tests
// (`taiyaku-library/src/migrate/tests.rs`) — because the defect was never in
// that function's logic; it was that nothing called it.

fn notes_fixture() -> &'static str {
    r#"{"work":"t","notes":[
        {"id":"01H1","edition":"raw","slot":0,"body":"a note","created":"2026-08-14T10:00:00Z","updated":"2026-08-14T10:00:00Z"}
    ]}"#
}

fn truth_fixture() -> &'static str {
    // Page 0 examined and found empty, page 1 carrying a box — the
    // distinction `truth_page` exists for (schema/001_initial.sql).
    r#"{"taiyaku_truth":1,"editions":{"raw":{"members":[],"pages":{
        "0":[],
        "1":[{"x":0.1,"y":0.2,"w":0.3,"h":0.05,"axis":"v","kind":"dialogue","ignore":false,"text":null}]
    }}}}"#
}

fn library_fixture() -> &'static str {
    r#"{"taiyaku_library":1,"updated":"2026-08-14T11:00:00Z","volume":3.0,"tags":["seinen"]}"#
}

#[test]
fn importing_a_work_with_all_four_siblings_present_migrates_every_one() {
    let root = scratch("siblings-all-four");
    let path = write_fixture(&root, "aa11");
    write_sibling(&root, "progress.json", r#"{"edition":"raw","slot":7}"#);
    write_sibling(&root, "notes.json", notes_fixture());
    write_sibling(&root, "truth.json", truth_fixture());
    write_sibling(&root, "library.json", library_fixture());

    let library = Library::open_in_memory().expect("library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (work_id, _outcomes, migration) = import_canonical(&library, &canonical).expect("imports");

    // The report says so, in the fixed-field style the printed summary uses.
    let summary = migration_summary(&migration);
    assert!(summary.contains("progress=migrated"), "{summary}");
    assert!(summary.contains("notes=migrated:1"), "{summary}");
    assert!(summary.contains("truth=migrated:2p/1b"), "{summary}");
    assert!(summary.contains("library=migrated"), "{summary}");

    // And the rows actually hold it.
    let conn = library.conn();
    let progress = taiyaku_library::state::progress(&conn, work_id, 1)
        .expect("reads")
        .expect("migrated");
    assert_eq!(progress.slot, 7);

    let notes = taiyaku_library::state::notes(&conn, work_id, 1).expect("reads");
    assert_eq!(notes.notes.len(), 1);
    assert_eq!(notes.notes[0].id, "01H1");
    assert_eq!(notes.notes[0].body, "a note");

    let truth = taiyaku_library::state::truth(&conn, work_id)
        .expect("reads")
        .expect("migrated");
    let edition = truth.editions.get("raw").expect("edition present");
    assert!(
        edition.pages.contains_key(&0),
        "the examined-and-empty page must survive as present, not absent"
    );
    assert_eq!(edition.pages.get(&0), Some(&Vec::new()));
    assert_eq!(edition.pages.get(&1).map(Vec::len), Some(1));

    let meta = taiyaku_library::librarydoc::of(&conn, work_id).expect("reads");
    assert_eq!(meta.volume, Some(3.0));
    assert_eq!(meta.tags, vec!["seinen".to_string()]);
}

#[test]
fn a_second_import_does_not_overwrite_state_changed_since_the_first() {
    let root = scratch("siblings-second-import-kept");
    let path = write_fixture(&root, "bb22");
    write_sibling(&root, "progress.json", r#"{"edition":"raw","slot":7}"#);
    write_sibling(&root, "notes.json", notes_fixture());
    write_sibling(&root, "truth.json", truth_fixture());
    write_sibling(&root, "library.json", library_fixture());

    let library = Library::open_in_memory().expect("library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (work_id, _outcomes, first) = import_canonical(&library, &canonical).expect("first import");
    assert!(
        !migration_summary(&first).is_empty(),
        "the first import must actually have migrated something, or this test proves nothing"
    );

    // The reader — and a hand edit — move on before the library is reopened.
    {
        let conn = library.conn();
        taiyaku_library::state::put_progress(
            &conn,
            work_id,
            1,
            &taiyaku_library::Progress {
                edition: "raw".to_string(),
                slot: 99,
            },
        )
        .expect("live progress");
        taiyaku_library::state::put_notes(
            &conn,
            work_id,
            1,
            &taiyaku_library::Notes {
                work: "t".to_string(),
                notes: vec![taiyaku_library::Note {
                    id: "live".to_string(),
                    edition: "raw".to_string(),
                    slot: 0,
                    region: None,
                    color: None,
                    body: "written since".to_string(),
                    created: "2026-08-15T00:00:00Z".to_string(),
                    updated: "2026-08-15T00:00:00Z".to_string(),
                }],
            },
        )
        .expect("live notes");
    }

    let (_work_id, _outcomes, second) =
        import_canonical(&library, &canonical).expect("second import");
    let summary = migration_summary(&second);
    assert!(summary.contains("progress=kept"), "{summary}");
    assert!(summary.contains("notes=kept"), "{summary}");
    assert!(summary.contains("truth=kept"), "{summary}");
    assert!(summary.contains("library=kept"), "{summary}");

    let conn = library.conn();
    let progress = taiyaku_library::state::progress(&conn, work_id, 1)
        .expect("reads")
        .expect("still there");
    assert_eq!(progress.slot, 99, "the file must not have won");

    let notes = taiyaku_library::state::notes(&conn, work_id, 1).expect("reads");
    assert_eq!(notes.notes.len(), 1);
    assert_eq!(notes.notes[0].id, "live", "the file must not have won");
}

/// **The C3 defect of whole-branch review 2, through the wiring `serve`
/// actually uses.** `taiyaku serve <sidecar>...` calls `import_canonical` for
/// every path on every start, so a migration that decided "already migrated?"
/// by asking whether the database currently holds anything undid a reader's
/// deletion at every restart, forever: delete your last note and clear your
/// labelling, and both came back — with `export --in-place` unable to remove
/// the stale files, because it does not write a document whose state is
/// `None`.
#[test]
fn a_deletion_made_since_the_first_import_survives_every_restart() {
    let root = scratch("siblings-deletion-sticks");
    let path = write_fixture(&root, "ee55");
    write_sibling(&root, "notes.json", notes_fixture());
    write_sibling(&root, "truth.json", truth_fixture());

    let library = Library::open_in_memory().expect("library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (work_id, _outcomes, first) = import_canonical(&library, &canonical).expect("first import");
    let summary = migration_summary(&first);
    assert!(summary.contains("notes=migrated:1"), "{summary}");
    assert!(summary.contains("truth=migrated:2p/1b"), "{summary}");

    // The reader deletes their last note and clears every labelled page —
    // `PUT /notes` and `PUT /truth` with empty documents.
    {
        let conn = library.conn();
        taiyaku_library::state::put_notes(
            &conn,
            work_id,
            1,
            &taiyaku_library::Notes {
                work: "t".to_string(),
                notes: Vec::new(),
            },
        )
        .expect("deletes the last note");
        taiyaku_library::state::put_truth(&conn, work_id, &taiyaku_core::TruthDoc::new())
            .expect("clears the labelling");
    }

    // Two more starts, because "it comes back at every restart" was the
    // shape of the defect: once is not evidence that it stays away.
    for start in 1..=2 {
        import_canonical(&library, &canonical).expect("a later start");
        let conn = library.conn();
        assert!(
            taiyaku_library::state::notes(&conn, work_id, 1)
                .expect("reads")
                .notes
                .is_empty(),
            "start {start}: the deleted note came back"
        );
        assert!(
            taiyaku_library::state::truth(&conn, work_id)
                .expect("reads")
                .is_none(),
            "start {start}: the cleared labelling came back"
        );
    }
}

#[test]
fn a_malformed_sibling_is_reported_and_skipped_and_the_sidecar_still_imports() {
    let root = scratch("siblings-malformed");
    let path = write_fixture(&root, "cc33");
    write_sibling(&root, "notes.json", "{ this is not valid json");
    write_sibling(&root, "progress.json", r#"{"edition":"raw","slot":7}"#);

    let library = Library::open_in_memory().expect("library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (work_id, _outcomes, migration) =
        import_canonical(&library, &canonical).expect("the import itself must still succeed");

    match &migration.notes {
        Some(taiyaku_library::migrate::NotesOutcome::Malformed(msg)) => assert!(!msg.is_empty()),
        other => panic!("expected notes to be reported malformed, got {other:?}"),
    }
    let summary = migration_summary(&migration);
    assert!(summary.contains("notes=malformed"), "{summary}");
    // The good sibling beside the bad one still migrated — one bad file does
    // not block the rest of this work's own migration, let alone the library.
    assert!(summary.contains("progress=migrated"), "{summary}");

    assert_eq!(work_count(&library), 1, "the sidecar itself still imported");
    let conn = library.conn();
    assert!(
        taiyaku_library::state::notes(&conn, work_id, 1)
            .expect("reads")
            .notes
            .is_empty(),
        "nothing was written from the unreadable notes file"
    );
}

/// **Whole-branch review 3, Minor 10.** The `=malformed` token in the summary
/// line names the document and stops there — no colon, no byte offset, no
/// sentence — deliberately, because a fixed-field line is no place for one.
/// This is the sentence, and it was pinned by nothing: the function printed
/// directly, so replacing its body with `()` passed the whole suite.
///
/// It is the only message that tells a reader a sibling document was not read,
/// and it is what makes the C1 recovery path a recovery: a malformed
/// `taiyaku.truth.json` is left on disk *so that* fixing it and importing
/// again works, and nothing else says there is something to fix.
///
/// All four documents malformed at once, because the order is part of the
/// claim — a reader scanning four lines should meet them in the order
/// `migrate::work` considered them — and because three of the four arms had
/// never been reached by any test at all.
#[test]
fn every_malformed_sibling_gets_a_sentence_naming_the_file_and_the_reason() {
    let root = scratch("siblings-malformed-sentences");
    let path = write_fixture(&root, "ee55");
    write_sibling(&root, "progress.json", "{ not a progress document");
    write_sibling(&root, "notes.json", "{ not a notes document");
    write_sibling(&root, "truth.json", "{ not a truth document");
    write_sibling(&root, "library.json", "{ not a library document");

    let library = Library::open_in_memory().expect("library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (_work_id, _outcomes, migration) =
        import_canonical(&library, &canonical).expect("the import itself must still succeed");

    let lines = malformed_sibling_warnings(&canonical, &migration);
    let kinds: Vec<&str> = lines
        .iter()
        .map(|l| {
            l.split(" not migrated: ")
                .next()
                .and_then(|head| head.rsplit(": ").next())
                .expect("every line names which document it is about")
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["progress", "notes", "truth", "library metadata"],
        "one line per unreadable document, in the order they were considered: {lines:?}"
    );
    for line in &lines {
        assert!(
            line.starts_with("taiyaku: "),
            "the message names the tool, the way every other stderr line does: {line}"
        );
        assert!(
            line.contains(&canonical.display().to_string()),
            "and the sidecar whose siblings these are: {line}"
        );
        assert!(
            line.contains("expected value") || line.contains("key must be a string"),
            "and carries serde's own reason through, which is the part a reader \
             acts on: {line}"
        );
    }
}

/// The other half: nothing malformed, nothing said. A reader importing a
/// perfectly good work must not be handed four lines about it.
#[test]
fn a_work_whose_siblings_all_read_cleanly_produces_no_warning_at_all() {
    let root = scratch("siblings-no-warnings");
    let path = write_fixture(&root, "ff66");
    write_sibling(&root, "progress.json", r#"{"edition":"raw","slot":7}"#);
    write_sibling(&root, "notes.json", notes_fixture());

    let library = Library::open_in_memory().expect("library");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (_work_id, _outcomes, migration) = import_canonical(&library, &canonical).expect("imports");

    assert!(
        migration_summary(&migration).contains("progress=migrated"),
        "the fixture must really have siblings, or this test proves nothing"
    );
    assert!(
        malformed_sibling_warnings(&canonical, &migration).is_empty(),
        "a good import says nothing on stderr about its siblings"
    );
}

#[test]
fn round_trip_export_wipe_the_database_and_reimport_recovers_the_same_state() {
    let root = scratch("siblings-round-trip");
    let path = write_fixture(&root, "dd44");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");

    // A library with real reader activity against it — no sibling files
    // involved yet, all of it recorded live, the way a running server would.
    let first_db = root.join("first.db");
    let library_a = Library::open(&first_db).expect("opens");
    let (work_id_a, _outcomes, _migration) =
        import_canonical(&library_a, &canonical).expect("imports");
    {
        let conn = library_a.conn();
        taiyaku_library::state::put_progress(
            &conn,
            work_id_a,
            1,
            &taiyaku_library::Progress {
                edition: "raw".to_string(),
                slot: 4,
            },
        )
        .expect("progress");
        taiyaku_library::state::put_notes(
            &conn,
            work_id_a,
            1,
            &taiyaku_library::Notes {
                work: "t".to_string(),
                notes: vec![taiyaku_library::Note {
                    id: "01H9".to_string(),
                    edition: "raw".to_string(),
                    slot: 2,
                    region: None,
                    color: None,
                    body: "round trip me".to_string(),
                    created: "2026-08-14T10:00:00Z".to_string(),
                    updated: "2026-08-14T10:00:00Z".to_string(),
                }],
            },
        )
        .expect("notes");
        let truth = taiyaku_core::TruthDoc::from_json(truth_fixture()).expect("parses");
        taiyaku_library::state::put_truth(&conn, work_id_a, &truth).expect("truth");
        let meta =
            taiyaku_library::librarydoc::LibraryDoc::from_json(library_fixture()).expect("parses");
        taiyaku_library::librarydoc::apply(&conn, work_id_a, &meta).expect("library metadata");
    }

    // Export: the five documents land beside the sidecar.
    {
        let conn = library_a.conn();
        taiyaku_library::export::work(&conn, work_id_a, None).expect("exports");
    }
    drop(library_a);
    std::fs::remove_file(&first_db).expect("wiping the database");

    // A brand-new, empty library — nothing recorded against this work at
    // all — re-imports the same canonical sidecar path.
    let second_db = root.join("second.db");
    let library_b = Library::open(&second_db).expect("opens fresh");
    let (work_id_b, _outcomes, migration) =
        import_canonical(&library_b, &canonical).expect("reimports");
    assert!(
        !migration_summary(&migration).is_empty(),
        "the exported siblings must actually have been read back, or this test proves nothing"
    );

    let conn = library_b.conn();
    let progress = taiyaku_library::state::progress(&conn, work_id_b, 1)
        .expect("reads")
        .expect("recovered");
    assert_eq!(progress.slot, 4, "the reading position round-tripped");

    let notes = taiyaku_library::state::notes(&conn, work_id_b, 1).expect("reads");
    assert_eq!(notes.notes.len(), 1);
    assert_eq!(notes.notes[0].body, "round trip me");

    let truth = taiyaku_library::state::truth(&conn, work_id_b)
        .expect("reads")
        .expect("recovered");
    let edition = truth.editions.get("raw").expect("edition present");
    assert_eq!(
        edition.pages.get(&0),
        Some(&Vec::new()),
        "the examined-and-empty page round-tripped as present and empty, not absent"
    );
    assert_eq!(edition.pages.get(&1).map(Vec::len), Some(1));

    let meta = taiyaku_library::librarydoc::of(&conn, work_id_b).expect("reads");
    assert_eq!(meta.volume, Some(3.0), "the volume round-tripped");
    assert_eq!(meta.tags, vec!["seinen".to_string()]);
}

/// **The C2 defect of whole-branch review 2, end to end, exactly as it was
/// reproduced.** Labels are drawn against archive `aaaaaaaa` and recorded in
/// `taiyaku.truth.json`; the archive is then replaced (same path, different
/// bytes) and the work reimported; the export must still say `aaaaaaaa`.
///
/// Before the fix it said `bbbbbbbb`: `state::truth` re-projected `members`
/// from whatever `member` rows existed at read time, so the exported document
/// asserted provenance the labelling had never seen — and under `--in-place`
/// wrote that falsehood over the reader's only copy of it. Two things ride on
/// this list being true: `taiyaku score`'s refusal to score labels against
/// archives that have since changed, and the Labeller's own view of the same
/// document through `GET /api/works/{w}/truth`.
#[test]
fn a_reimport_that_changes_the_archives_does_not_rewrite_what_the_labels_were_drawn_against() {
    let root = scratch("truth-provenance-survives-reimport");
    let path = write_fixture(&root, "aaaaaaaa");
    write_sibling(
        &root,
        "truth.json",
        r#"{"taiyaku_truth":1,"editions":{"raw":{
             "members":[{"sha256":"aaaaaaaa"}],
             "pages":{"2":[{"x":0.1,"y":0.2,"w":0.3,"h":0.05,"axis":"v",
                            "kind":"dialogue","ignore":false,
                            "text":"hand-labelled against archive AAAA"}]}}}}"#,
    );
    let db = root.join("library.db");
    let out = root.join("out");

    let library = Library::open(&db).expect("opens");
    let canonical = canonical_sidecar_path(&path).expect("canonicalises");
    let (_work_id, _outcomes, migration) = import_canonical(&library, &canonical).expect("imports");
    assert!(
        migration_summary(&migration).contains("truth=migrated"),
        "the labels must actually have been migrated, or this test proves nothing"
    );

    // The archive is replaced under the same path: the sidecar now declares a
    // different digest for the same member, and the work is reimported.
    std::fs::write(&path, sidecar_json("bbbbbbbb")).expect("rewrites the sidecar");
    import_canonical(&library, &canonical).expect("reimports");
    assert_eq!(
        work_count(&library),
        1,
        "the reimport reconciled onto the same work, or the scenario is not the one under test"
    );
    drop(library);

    let args = [
        "t".to_string(),
        "--dir".to_string(),
        out.to_str().unwrap().to_string(),
        "--db".to_string(),
        db.to_str().unwrap().to_string(),
    ];
    export(&args).expect("exports");

    let text = std::fs::read_to_string(out.join("t").join("taiyaku.truth.json"))
        .expect("reads the exported truth document");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(
        doc["editions"]["raw"]["members"][0]["sha256"].as_str(),
        Some("aaaaaaaa"),
        "the exported document must say which archives the boxes were drawn \
         against, not which archives happen to be on disk now: {text}"
    );
}
