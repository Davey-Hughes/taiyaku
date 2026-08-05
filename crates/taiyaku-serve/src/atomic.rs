//! Write a file so a crash cannot publish half of one.
//!
//! Three callers now: the reading position (O2), a generated cover, and a
//! sidecar rewritten by a correction. The first two carried their own copy of
//! this and said so in a comment; a third copy is where a comment asking
//! whoever changes one to keep the others in step stops being good enough.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A temp path nothing else will ever pick.
///
/// A process id plus a monotonic per-process counter. Two devices writing one
/// work through one server is two requests in one process, so the counter
/// carries that case; two servers over one directory is what the pid carries.
pub(crate) fn temp_path(path: &Path, extension: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    path.with_extension(format!("{extension}.tmp.{pid}.{n}"))
}

/// Write `bytes` to `path`, atomically.
///
/// `fs::write` truncates first, and a crash in that window leaves a short file
/// where a whole one was — for progress that answers 500 instead of "nothing
/// recorded", and for a sidecar it destroys the alignment of every edition in
/// the work.
///
/// A failed write or a failed rename each leave a temp file nothing else will
/// ever collect, since its name is unique to this attempt, so both branches
/// remove it rather than accumulating one file per failure in the reader's
/// own library directory. `cover.rs`'s writer, before this module existed,
/// did this unconditionally for the same reason; both failure paths now get
/// the same treatment here.
pub(crate) fn write(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String> {
    let temporary = temp_path(path, extension);
    if let Err(e) = std::fs::write(&temporary, bytes) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{}: {e}", temporary.display()));
    }
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!("{}: {e}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
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

    /// Unique per attempt, not a fixed name: two writers sharing one temp
    /// path can interleave into publishing a half-written file, which is the
    /// whole thing this exists to prevent.
    #[test]
    fn two_temp_paths_for_one_destination_differ() {
        let path = std::path::Path::new("/tmp/taiyaku-test-atomic/thing.json");
        assert_ne!(temp_path(path, "json"), temp_path(path, "json"));
    }
}
