use super::*;

/// The property O2 established for progress and this now shares: the
/// destination is *replaced*, never opened for truncation, so a crash
/// mid-write cannot leave a short file where a whole one was.
///
/// Asserted by inode identity rather than by taking write permission off
/// the existing file, which was this test's first version. That approach
/// had a silent failure mode: on a filesystem that ignores the read-only
/// bit, or when the test runs as root, the permission-denied write this
/// test relies on to distinguish "replaced" from "truncated" simply
/// succeeds, so the earlier code printed a skip line and returned — but
/// libtest captures stdout for a passing test, so that line is invisible
/// under a plain `cargo test`, and the run reports an ordinary `ok`
/// indistinguishable from one that actually measured anything. A rename
/// gives the destination path a *different* inode; an in-place
/// truncating write keeps the same one. That distinction holds no matter
/// who is running the test or what the filesystem's permission
/// enforcement does, so there is nothing left to skip.
#[test]
fn the_destination_is_replaced_by_rename_not_truncation() {
    let dir = std::env::temp_dir().join("taiyaku-test-atomic-rename");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("thing.json");
    std::fs::write(&path, b"old").unwrap();

    #[cfg(unix)]
    let before = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(&path).unwrap().ino()
    };

    let wrote = write(&path, b"new", "json");

    // Portable: proven on every target, unix or not, independent of the
    // inode check below.
    let read_back = std::fs::read_to_string(&path).unwrap();

    #[cfg(unix)]
    let after = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(&path).unwrap().ino()
    };

    let _ = std::fs::remove_dir_all(&dir);
    wrote.expect("the write succeeds");
    assert_eq!(read_back, "new", "the destination must hold the new bytes");
    #[cfg(unix)]
    assert_ne!(
        before, after,
        "a rename replaces the destination with a different inode; an \
             in-place truncating write would have kept the same one"
    );
}

/// O29, the branch that carried it. The destination's parent does not
/// exist, so the *staging* write is what fails — the branch that used to
/// answer with the temporary's name.
///
/// The assertion is on the absence of `.tmp.` rather than on the exact
/// wording, because the point is not which sentence is printed but that no
/// invented filename appears. `temp_path` builds every temporary with that
/// infix, so it is the one substring that cannot appear by accident.
#[test]
fn a_failed_staging_names_the_destination_and_never_the_temporary() {
    let dir = std::env::temp_dir().join("taiyaku-test-atomic-o29-staging");
    let _ = std::fs::remove_dir_all(&dir);
    // Deliberately not created: the parent must be missing.
    let path = dir.join("out.json");

    let e = write(&path, b"{}", "json").expect_err("the parent does not exist");

    assert!(
        e.contains("out.json"),
        "the message must name the file the caller asked for: {e}"
    );
    assert!(
        !e.contains(".tmp."),
        "the message names a temporary the caller never typed: {e}"
    );
}

/// The other branch, which already named the destination and is pinned so
/// that a later edit cannot quietly give it the temporary instead.
///
/// A directory at the destination is what fails the rename while letting
/// the staging write succeed: the temporary is a sibling file, and a file
/// cannot be renamed onto a directory.
#[test]
fn a_failed_rename_names_the_destination_and_never_the_temporary() {
    let dir = std::env::temp_dir().join("taiyaku-test-atomic-o29-rename");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.json");
    // A directory where the file should go — staging succeeds, rename cannot.
    std::fs::create_dir(&path).unwrap();

    let outcome = write(&path, b"{}", "json");

    // Nothing left behind: the temp file is removed on this path too.
    let strays: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".tmp."))
        .collect();
    let _ = std::fs::remove_dir_all(&dir);

    let e = outcome.expect_err("a file cannot be renamed onto a directory");
    assert!(
        e.contains("out.json"),
        "the message must name the file the caller asked for: {e}"
    );
    assert!(
        !e.contains(".tmp."),
        "the message names a temporary the caller never typed: {e}"
    );
    assert!(
        strays.is_empty(),
        "a failed rename must not leave its temporary behind: {strays:?}"
    );
}

/// Unique per attempt, not a fixed name: two writers sharing one temp
/// path can interleave into publishing a half-written file, which is the
/// whole thing this exists to prevent.
#[test]
fn two_temp_paths_for_one_destination_differ() {
    let path = std::path::Path::new("/tmp/taiyaku-test-atomic/thing.json");
    assert_ne!(temp_path(path, "json"), temp_path(path, "json"));
}
