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

/// O104. A rename replaces the destination atomically only for bytes that
/// have reached the disk. Without a sync first, the rename can be journalled
/// before the data it points at is written — XFS, which the NAS's array disks
/// run, has no equivalent of ext4's `auto_da_alloc` to force it — and a crash
/// in that window leaves the destination naming data that never existed,
/// with the old file already unlinked. The module doc's promise, "a crash
/// cannot publish half of one", did not hold.
///
/// So the temporary must be synced **before** the rename, and the directory
/// **after** it, or the rename itself may not survive the crash either.
/// Asserted on the syscalls the real function makes, because nothing else a
/// test can read tells a synced file from an unsynced one.
#[cfg(target_os = "linux")]
#[test]
fn the_temporary_is_synced_before_the_rename_and_the_directory_after_it() {
    use crate::test_support::{assert_replaced_durably, traced};

    const CHILD: &str = "TAIYAKU_TEST_ATOMIC_O104_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        write(std::path::Path::new(&path), b"new", "json").expect("the traced write");
        return;
    }

    // Per process, since the traced child widens the window in which two
    // concurrent runs — cargo-mutants' jobs, a second worktree — would
    // delete each other's directory.
    let dir = std::env::temp_dir().join(format!(
        "taiyaku-test-atomic-o104-sync-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let path = dir.join("thing.json");
    std::fs::write(&path, b"old").unwrap();

    let calls = traced(
        "atomic::tests::the_temporary_is_synced_before_the_rename_and_the_directory_after_it",
        (CHILD, path.as_os_str()),
        &dir.join("strace.log"),
    );
    let read_back = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        read_back, "new",
        "the traced child must actually have written"
    );
    assert_replaced_durably(&calls, &path, &dir);
}

/// O104's directory sync, for a destination named with no directory at all.
/// `Path::new("thing.json").parent()` is `Some("")` rather than `None`, and
/// opening `""` fails — so without the fallback to the current directory,
/// `taiyaku align --write out.json` would write the sidecar and then report
/// that it had not.
///
/// Through strace rather than an `is_ok()`, because an `is_ok()` also passes
/// for a fallback that syncs the wrong directory. The child changes into the
/// test directory first, which is safe only because it is a process running
/// this one test and nothing else.
#[cfg(target_os = "linux")]
#[test]
fn a_bare_filename_syncs_the_directory_it_was_written_into() {
    use crate::test_support::{Call, traced};

    const CHILD: &str = "TAIYAKU_TEST_ATOMIC_O104_BARE_CHILD";
    if let Some(dir) = std::env::var_os(CHILD) {
        std::env::set_current_dir(&dir).expect("the child enters the test directory");
        write(std::path::Path::new("thing.json"), b"new", "json").expect("the traced write");
        return;
    }

    let dir = std::env::temp_dir().join(format!(
        "taiyaku-test-atomic-o104-bare-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();

    let calls = traced(
        "atomic::tests::a_bare_filename_syncs_the_directory_it_was_written_into",
        (CHILD, dir.as_os_str()),
        &dir.join("strace.log"),
    );
    let read_back = std::fs::read_to_string(dir.join("thing.json")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        read_back, "new",
        "the traced child must actually have written"
    );
    let renamed = calls
        .iter()
        .position(|c| matches!(c, Call::Rename { to, .. } if to.as_os_str() == "thing.json"))
        .unwrap_or_else(|| panic!("nothing was renamed onto the bare name: {calls:?}"));
    assert!(
        calls[renamed + 1..]
            .iter()
            .any(|c| matches!(c, Call::Sync(p) if *p == dir)),
        "{} was not synced after a rename onto a bare filename inside it: {calls:?}",
        dir.display()
    );
}
