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

/// `VACUUM INTO` refuses an existing target rather than overwriting it, and
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

    let _ = std::fs::remove_dir_all(&dir);
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
/// directory and writes outside it — and `VACUUM INTO`'s refusal of an
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
