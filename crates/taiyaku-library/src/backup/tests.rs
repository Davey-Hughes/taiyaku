use super::*;
use crate::Library;

/// A scratch directory that cleans itself up, matching the shape the rest of
/// this crate's file-touching tests use.
fn scratch(name: &str) -> taiyaku_read::test_support::TempDir {
    taiyaku_read::test_support::TempDir::new(&format!("backup-{name}"))
}

/// PR2 §1.3: the directory is derived from the database path, because the
/// server has to be able to compute it in order to list it.
#[test]
fn the_backup_directory_sits_beside_the_database() {
    assert_eq!(
        directory_for(std::path::Path::new(
            "/home/x/.local/share/taiyaku/library.db"
        )),
        std::path::PathBuf::from("/home/x/.local/share/taiyaku/backups")
    );
}

/// **A backup restores.** Not "the file exists and is non-empty": the whole
/// point of `VACUUM INTO` over a file copy is that the snapshot is consistent,
/// and the only way to assert that is to open it and read something back.
#[test]
fn a_backup_opens_as_a_library_and_still_holds_the_work() {
    let dir = scratch("restores");
    let db = dir.join("library.db");
    let library = Library::open(&db).expect("opens a library");
    let work = {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Restored","reading":"rtl"},
            "editions":{"raw":{"label":"R","members":[
              {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
            .expect("imports")
    };

    let into = directory_for(&db);
    let taken = {
        let conn = library.conn();
        take(&conn, &into).expect("takes a backup")
    };

    assert!(
        taken.name.starts_with(PREFIX) && taken.name.ends_with(SUFFIX),
        "{taken:?}"
    );
    assert!(taken.bytes > 0, "{taken:?}");

    let restored = Library::open(&into.join(&taken.name)).expect("opens the backup");
    let title: String = restored
        .conn()
        .query_row("SELECT title FROM work WHERE id = ?1", [work], |r| r.get(0))
        .expect("reads the work back out of the backup");
    assert_eq!(title, "Restored");

    let _ = std::fs::remove_dir_all(&dir);
}

/// `list` answers newest first and reports each file's real size, so a screen
/// can render "3 backups, newest 2026-09-04" without a second stat call.
#[test]
fn listing_answers_newest_first_and_ignores_anything_else_in_the_directory() {
    let dir = scratch("listing");
    std::fs::write(dir.join("library-20260901T000000Z.db"), b"aaaa").expect("writes");
    std::fs::write(dir.join("library-20260903T000000Z.db"), b"bb").expect("writes");
    std::fs::write(dir.join("notes.txt"), b"not a backup").expect("writes");
    std::fs::create_dir(dir.join("library-20260902T000000Z.db.d")).expect("creates a decoy dir");

    let found = list(&dir).expect("lists");

    assert_eq!(
        found.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
        vec!["library-20260903T000000Z.db", "library-20260901T000000Z.db"],
        "newest first, and nothing that is not a backup"
    );
    assert_eq!(found[0].bytes, 2);
    assert_eq!(found[1].bytes, 4);
    assert_eq!(found[0].taken, "20260903T000000Z");

    let _ = std::fs::remove_dir_all(&dir);
}

/// A directory that does not exist yet is an empty listing, not an error: a
/// library that has never been backed up is the ordinary case, and the screen
/// that lists backups is the one an operator opens *before* taking the first.
#[test]
fn listing_a_directory_that_does_not_exist_yet_is_empty_rather_than_an_error() {
    let parent = scratch("absent");
    let dir = parent.join("never-made");
    assert_eq!(list(&dir).expect("lists"), Vec::new());
}

/// `take_named` refuses an existing target rather than overwriting it, and
/// PR2 §4 keeps that failure as itself rather than adding a suffix counter for
/// a case only a test loop produces.
#[test]
fn a_second_backup_in_the_same_second_is_refused_rather_than_overwriting() {
    let dir = scratch("collision");
    let db = dir.join("library.db");
    let library = Library::open(&db).expect("opens a library");
    let into = directory_for(&db);
    let conn = library.conn();

    let first = take(&conn, &into).expect("takes the first");
    let stand_in = b"stand-in for a same-second name";
    std::fs::write(into.join(&first.name), stand_in).expect("keeps the name taken");

    // Re-taking with the same name must not silently replace it. Rather than
    // race the clock, assert the property directly on the name that exists.
    let why = take_named(&conn, &into, &first.name).expect_err("refuses");
    assert!(
        matches!(why, TakeError::Exists(_)),
        "a name already taken is the caller's timing, not a fault: {why:?}"
    );
    assert!(
        why.to_string().contains(&first.name),
        "the message names the file: {why}"
    );
    assert_eq!(
        std::fs::read(into.join(&first.name)).expect("reads"),
        stand_in,
        "the existing file is untouched, not half-overwritten"
    );
    assert_eq!(
        std::fs::read_dir(&into)
            .expect("reads the directory")
            .count(),
        1,
        "and the refused take leaves nothing of its own beside it"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `take_named` creates the file before the copy, so a copy that fails would
/// leave it under a backup's name, listed as one.
#[test]
fn a_backup_whose_copy_fails_leaves_no_file_under_its_name() {
    let dir = scratch("failed-copy");
    let db = dir.join("library.db");
    let library = Library::open(&db).expect("opens a library");
    let into = directory_for(&db);
    let conn = library.conn();
    let name = "library-20261110T120000Z.db";

    // SQLite refuses a `VACUUM` inside a transaction, before it writes a page.
    conn.execute_batch("BEGIN").expect("begins");
    let why = take_named(&conn, &into, name).expect_err("the copy fails");
    conn.execute_batch("ROLLBACK").expect("rolls back");

    assert!(
        matches!(why, TakeError::Failed(_)),
        "a copy that fails is a fault: {why:?}"
    );
    assert!(
        why.to_string().contains("within a transaction"),
        "the copy's own error is the one returned: {why}"
    );
    assert!(
        std::fs::symlink_metadata(into.join(name)).is_err(),
        "nothing is left under the name"
    );
    assert!(
        list(&into).expect("lists").is_empty(),
        "and nothing lists as a backup"
    );
    assert_eq!(
        std::fs::read_dir(&into)
            .expect("reads the directory")
            .count(),
        0,
        "nor is the copy it began left anywhere in the directory"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// [`before_migration`] reads `Exists` as success and migrates, so a create
/// refused for any reason but the name being taken has to be a fault.
#[test]
fn a_create_refused_for_any_reason_but_a_taken_name_is_a_fault() {
    let name = "library-20261110T120000Z.db";
    let target = Path::new("/backups/library-20261110T120000Z.db");
    let error = |kind| std::io::Error::from(kind);

    let denied = create_refused(name, target, &error(std::io::ErrorKind::PermissionDenied));
    assert!(
        matches!(denied, TakeError::Failed(_)),
        "a create refused is a fault: {denied:?}"
    );
    assert!(
        denied.to_string().starts_with("creating /backups/library-"),
        "and names the file: {denied}"
    );

    let taken = create_refused(name, target, &error(std::io::ErrorKind::AlreadyExists));
    assert!(
        matches!(taken, TakeError::Exists(_)),
        "a name already taken is not: {taken:?}"
    );
}

/// A file the failed copy leaves behind lists as a backup, so the message
/// names a removal that failed, and only one that did.
#[test]
fn a_copy_that_fails_names_a_file_it_could_not_remove() {
    let name = "library-20261110T120000Z.db";
    let copy = "the copy's error";
    let error = |kind| Err(std::io::Error::from(kind));

    assert_eq!(
        copy_failed(name, &copy, &error(std::io::ErrorKind::PermissionDenied)).to_string(),
        format!(
            "writing the backup {name}: {copy}; the file it made could not be removed: {}",
            std::io::Error::from(std::io::ErrorKind::PermissionDenied)
        ),
    );
    for removed in [Ok(()), error(std::io::ErrorKind::NotFound)] {
        assert_eq!(
            copy_failed(name, &copy, &removed).to_string(),
            format!("writing the backup {name}: {copy}"),
            "nothing is left to name after {removed:?}"
        );
    }
}

/// PR2 §8: five backups, `keep: 2`, the three oldest go and **nothing else in
/// the directory does** — the unrelated file is the whole point of the
/// fixture, since a `prune` that globbed the directory would pass without it.
#[test]
fn pruning_keeps_the_newest_and_deletes_nothing_else_in_the_directory() {
    let dir = scratch("prune");
    for day in ["01", "02", "03", "04", "05"] {
        std::fs::write(dir.join(format!("library-202609{day}T000000Z.db")), b"x")
            .expect("writes a backup");
    }
    std::fs::write(dir.join("notes.txt"), b"keep me").expect("writes an unrelated file");
    std::fs::write(dir.join("library-.db"), b"keep me too").expect("writes a near-miss");

    let removed = prune(&dir, 2).expect("prunes");

    assert_eq!(
        removed,
        vec![
            "library-20260901T000000Z.db".to_string(),
            "library-20260902T000000Z.db".to_string(),
            "library-20260903T000000Z.db".to_string(),
        ],
        "oldest first, and it reports what it removed rather than a count"
    );
    assert_eq!(
        list(&dir).expect("lists").len(),
        2,
        "the two newest survive"
    );
    assert!(dir.join("notes.txt").exists(), "an unrelated file survives");
    assert!(
        dir.join("library-.db").exists(),
        "a name that is not a real backup name survives"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `keep` at or above the count deletes nothing, and `keep: 0` is a caller
/// asking for everything to go — which is allowed, said out loud, and is why
/// the CLI's flag is absent by default rather than defaulting to a number.
#[test]
fn pruning_more_than_there_are_deletes_nothing_and_zero_deletes_all() {
    let dir = scratch("prune-edges");
    for day in ["01", "02"] {
        std::fs::write(dir.join(format!("library-202609{day}T000000Z.db")), b"x")
            .expect("writes a backup");
    }

    assert!(prune(&dir, 5).expect("prunes").is_empty());
    assert_eq!(list(&dir).expect("lists").len(), 2);

    assert_eq!(prune(&dir, 0).expect("prunes").len(), 2);
    assert!(list(&dir).expect("lists").is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// O263: a backup taken before a migration is named for the version the
/// migration moves to, and `stamp_of` reads that version back out of the name.
#[test]
fn stamp_of_reads_the_version_a_backup_was_taken_before() {
    assert_eq!(
        stamp_of("library-20261110T120000Z-before-v19.db"),
        Some(("20261110T120000Z".to_string(), Some(19))),
        "a tagged name answers its stamp and its version"
    );
    assert_eq!(
        stamp_of("library-20261110T120000Z.db"),
        Some(("20261110T120000Z".to_string(), None)),
        "a plain name answers its stamp and no version"
    );
}

/// O263: one version has one spelling, so `N` is digits alone. This test and
/// the next two are split by the defect each catches, so a loop stopping at
/// one defect's row cannot hide another defect's.
#[test]
fn stamp_of_refuses_a_version_that_is_not_written_as_digits() {
    for name in [
        "library-20261110T120000Z-before-v.db",
        "library-20261110T120000Z-before-vx.db",
        "library-20261110T120000Z-before-v+5.db",
        "library-20261110T120000Z-before-v1/9.db",
    ] {
        assert_eq!(stamp_of(name), None, "`{name}` is not a backup name");
    }
}

/// O263: a leading zero would give one version a second spelling, and a
/// version past `u32` is not one the schema can reach.
#[test]
fn stamp_of_refuses_a_version_with_a_leading_zero_or_past_u32() {
    for name in [
        "library-20261110T120000Z-before-v019.db",
        "library-20261110T120000Z-before-v4294967296.db",
    ] {
        assert_eq!(stamp_of(name), None, "`{name}` is not a backup name");
    }
}

/// O263: nothing but one `-before-v<N>` sits between the stamp and `.db`, and
/// a name with a letter outside ASCII where the stamp ends is refused rather
/// than cut inside the letter.
#[test]
fn stamp_of_refuses_any_other_tail_after_the_stamp() {
    for name in [
        "library-20261110T120000Z-before-v19-before-v19.db",
        "library-20261110T120000Z-x-before-v19.db",
        "library-20261110T120000Z-after-v19.db",
        "library-20261110T120000é-before-v19.db",
    ] {
        assert_eq!(stamp_of(name), None, "`{name}` is not a backup name");
    }
}

/// O263: the Backup tab draws what `list` answers, so a backup taken before a
/// migration is listed beside the ones taken on request, carrying its version.
#[test]
fn listing_answers_a_backup_named_for_a_migration_with_its_version() {
    let dir = scratch("listing-before");
    std::fs::write(dir.join("library-20261110T120000Z-before-v19.db"), b"abc").expect("writes");
    std::fs::write(dir.join("library-20261109T120000Z.db"), b"x").expect("writes");

    assert_eq!(
        list(&dir).expect("lists"),
        vec![
            Backup {
                name: "library-20261110T120000Z-before-v19.db".to_string(),
                bytes: 3,
                taken: "20261110T120000Z".to_string(),
                before: Some(19),
            },
            Backup {
                name: "library-20261109T120000Z.db".to_string(),
                bytes: 1,
                taken: "20261109T120000Z".to_string(),
                before: None,
            },
        ],
        "both listed, newest first, and only the tagged one carries a version"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// O263: `take_named` handed a tagged name answers the version it holds, as
/// `list` would for the file it wrote.
#[test]
fn take_named_answers_the_version_a_tagged_name_holds() {
    let dir = scratch("take-named-before");
    let db = dir.join("library.db");
    let library = Library::open(&db).expect("opens a library");
    let into = directory_for(&db);
    let conn = library.conn();

    let taken =
        take_named(&conn, &into, "library-20261110T120000Z-before-v19.db").expect("takes it");
    assert_eq!(taken.before, Some(19), "{taken:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Writes a stand-in file under each backup name.
fn write_backups(dir: &std::path::Path, names: &[&str]) {
    for name in names {
        std::fs::write(dir.join(name), b"stand-in").expect("writes a backup");
    }
}

/// O263, spec decision 8: `--keep` neither counts nor deletes a backup taken
/// before a migration. With the tagged one newest, counting it would fill
/// `keep: 1` and both plain backups would go.
#[test]
fn pruning_neither_counts_nor_deletes_a_tagged_backup_that_is_newest() {
    let dir = scratch("prune-tagged-newest");
    write_backups(
        &dir,
        &[
            "library-20261101T000000Z.db",
            "library-20261102T000000Z.db",
            "library-20261103T000000Z-before-v19.db",
        ],
    );

    let removed = prune(&dir, 1).expect("prunes");

    assert_eq!(
        removed,
        vec!["library-20261101T000000Z.db".to_string()],
        "only the older plain backup goes"
    );
    assert!(
        dir.join("library-20261102T000000Z.db").exists(),
        "the newer plain backup is the one kept"
    );
    assert!(
        dir.join("library-20261103T000000Z-before-v19.db").exists(),
        "the tagged backup is kept"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// O263, spec decision 8: with the tagged one oldest, a `prune` that deleted
/// it would take it as the first past `keep`.
#[test]
fn pruning_neither_counts_nor_deletes_a_tagged_backup_that_is_oldest() {
    let dir = scratch("prune-tagged-oldest");
    write_backups(
        &dir,
        &[
            "library-20261031T000000Z-before-v19.db",
            "library-20261101T000000Z.db",
            "library-20261102T000000Z.db",
        ],
    );

    let removed = prune(&dir, 1).expect("prunes");

    assert_eq!(
        removed,
        vec!["library-20261101T000000Z.db".to_string()],
        "only the older plain backup goes"
    );
    assert!(
        dir.join("library-20261102T000000Z.db").exists(),
        "the newer plain backup is the one kept"
    );
    assert!(
        dir.join("library-20261031T000000Z-before-v19.db").exists(),
        "the tagged backup is kept"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A directory with no backups in it is not an error, for the same reason
/// `list` is not: the first run of a cron entry meets exactly this state.
#[test]
fn pruning_a_directory_that_does_not_exist_yet_removes_nothing() {
    let parent = scratch("prune-absent");
    let dir = parent.join("never-made");
    assert!(prune(&dir, 3).expect("prunes").is_empty());
}

/// **What a symlink in the backup directory costs, checked rather than
/// assumed** — `prune` deletes files, so what it can and cannot be aimed at
/// needs a test rather than a comment.
///
/// `DirEntry::metadata` does not traverse a symlink (unlike the free
/// `fs::metadata` it reads like), so a symlink named as a backup is not a
/// file, is never listed, and can never reach `prune` at all.
#[test]
fn pruning_a_symlinked_backup_is_left_alone_entirely() {
    let dir = scratch("prune-symlink");
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("creates the other directory");
    let target = elsewhere.join("precious.db");
    std::fs::write(&target, b"do not delete me").expect("writes the target");

    // Two real backups, so `keep: 1` displaces one of *them* and the symlink
    // is not merely spared by there being nothing to prune.
    for day in ["04", "05"] {
        std::fs::write(dir.join(format!("library-202609{day}T000000Z.db")), b"x")
            .expect("writes a backup");
    }
    let link = dir.join("library-20260901T000000Z.db");
    std::os::unix::fs::symlink(&target, &link)
        .expect("links a backup name at a file outside the directory");

    assert_eq!(
        list(&dir).expect("lists").len(),
        2,
        "the symlink is not a backup as far as `list` is concerned"
    );

    let removed = prune(&dir, 1).expect("prunes");

    assert_eq!(
        removed,
        vec!["library-20260904T000000Z.db".to_string()],
        "the oldest *real* backup goes, and the symlink is not a candidate"
    );
    assert!(link.exists(), "the link is untouched");
    assert_eq!(
        std::fs::read(&target).expect("the target survives"),
        b"do not delete me",
        "and so is what it points at"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **`take_named` is `pub`, so the shape check belongs on it.** Without one,
/// `take_named(conn, dir, "../../library.db")` joins straight onto the
/// directory and writes outside it — and the exclusive create's refusal of an
/// existing target does not hold for a target that does not exist yet.
#[test]
fn take_named_refuses_a_name_that_is_not_a_backup_name() {
    let dir = scratch("take-named-shape");
    let db = dir.join("library.db");
    let library = Library::open(&db).expect("opens a library");
    let into = directory_for(&db);
    let conn = library.conn();

    for name in ["../escaped.db", "library.db", "library-.db", "notes.txt"] {
        let why = take_named(&conn, &into, name).expect_err("refuses");
        assert!(
            why.to_string().contains("is not a backup name"),
            "`{name}`: {why}"
        );
    }
    assert!(
        !dir.join("escaped.db").exists(),
        "and nothing was written outside the backup directory"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **"Not found" is the only `read_dir` failure that means "no backups yet".**
/// A directory that exists and cannot be read, or a path that is a file, must
/// say so rather than answer an empty listing. That is the silence this
/// project keeps writing rules against.
#[test]
fn a_backup_path_that_is_not_a_directory_is_an_error_not_an_empty_listing() {
    let dir = scratch("list-not-a-directory");
    let impostor = dir.join("backups");
    std::fs::write(&impostor, b"this is a file, not a directory").expect("writes");

    let why = list(&impostor).expect_err("a file is not an empty directory");
    assert!(why.contains("backups"), "the message names the path: {why}");

    // And the case the guard is actually for still answers empty.
    assert!(
        list(&dir.join("never-made")).expect("lists").is_empty(),
        "a directory that does not exist is still an empty listing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// While a copy runs nothing is under the backup's name: a file there would
/// list as a backup and, before a migration, stand for the database while
/// holding part of it (O263). Another connection's exclusive lock holds the
/// copy at its start while the directory is read.
#[test]
fn nothing_is_under_a_backups_name_while_its_copy_runs() {
    let dir = scratch("copy-runs");
    let db = dir.join("library.db");
    Connection::open(&db)
        .expect("a file database")
        .execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);")
        .expect("something to copy");
    let into = directory_for(&db);
    let name = "library-20261110T120000Z.db";
    let holder = Connection::open(&db).expect("a second connection");
    holder
        .execute_batch("BEGIN EXCLUSIVE")
        .expect("holds the file");

    let copier = {
        let (db, into) = (db.clone(), into.clone());
        std::thread::spawn(move || {
            let conn = Connection::open(&db).expect("the copy's connection");
            conn.busy_timeout(std::time::Duration::from_secs(20))
                .expect("waits for the lock");
            take_named(&conn, &into, name)
        })
    };
    let started = std::time::Instant::now();
    while std::fs::read_dir(&into).map_or(true, |mut entries| entries.next().is_none()) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "the copy never began"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let named = std::fs::symlink_metadata(into.join(name)).is_ok();
    let listed = list(&into).expect("lists");
    holder.execute_batch("ROLLBACK").expect("lets the copy go");
    let taken = copier
        .join()
        .expect("the copy's thread")
        .expect("the copy finishes");

    assert!(
        !named && listed.is_empty(),
        "nothing is under the name, or listed, while the copy runs: {listed:?}"
    );
    assert_eq!(
        list(&into).expect("lists"),
        vec![taken],
        "and the whole copy is, once it is done"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A copy cut off before it reached its name leaves its stage, a dot name
/// `stamp_of` refuses: never listed, so never pruned or downloaded, and left
/// for a person to delete (O263).
#[test]
fn a_copy_left_unfinished_is_neither_listed_nor_pruned() {
    let dir = scratch("unfinished");
    let into = dir.join("backups");
    std::fs::create_dir_all(&into).expect("the backups directory");
    write_backups(&into, &["library-20261110T120000Z.db"]);
    let stage = into.join(".library-20261110T120001Z-before-v19.db.partial");
    std::fs::write(&stage, b"part of a copy").expect("a copy left unfinished");

    let listed: Vec<String> = list(&into)
        .expect("lists")
        .into_iter()
        .map(|backup| backup.name)
        .collect();
    assert_eq!(listed, ["library-20261110T120000Z.db"], "only the backup");
    assert_eq!(
        prune(&into, 0).expect("prunes"),
        ["library-20261110T120000Z.db"],
        "and only the backup goes"
    );
    assert!(stage.exists(), "the unfinished copy is left as it was");

    let _ = std::fs::remove_dir_all(&dir);
}

/// O104's ordering, for a backup: the copy is synced before it is renamed to
/// its name and the directory after, or a power cut could leave the name over
/// pages never written, which the next open would take as the backup (O263).
#[cfg(target_os = "linux")]
#[test]
fn a_backup_is_synced_before_its_rename_and_its_directory_after_it() {
    use taiyaku_read::test_support::{assert_replaced_durably, traced};

    const CHILD: &str = "TAIYAKU_TEST_BACKUP_SYNC_CHILD";
    let name = "library-20261110T120000Z-before-v19.db";
    if let Some(db) = std::env::var_os(CHILD) {
        let db = std::path::PathBuf::from(db);
        let conn = Connection::open(&db).expect("the traced copy's connection");
        // `VACUUM INTO` syncs its copy at this connection's level, so with it
        // off the only sync left is the one `take_named` makes itself.
        conn.pragma_update(None, "synchronous", "OFF")
            .expect("no sync from SQLite");
        take_named(&conn, &directory_for(&db), name).expect("the traced backup");
        return;
    }

    let temp = scratch("synced");
    let dir = std::fs::canonicalize(temp.path()).expect("the test directory resolves");
    let db = dir.join("library.db");
    drop(Library::open(&db).expect("a library"));

    let calls = traced(
        "backup::tests::a_backup_is_synced_before_its_rename_and_its_directory_after_it",
        (CHILD, db.as_os_str()),
        &dir.join("strace.log"),
    );

    let into = directory_for(&db);
    assert_eq!(
        list(&into).expect("lists").len(),
        1,
        "the traced child must actually have taken the backup"
    );
    assert_replaced_durably(&calls, &into.join(name), &into);
}
