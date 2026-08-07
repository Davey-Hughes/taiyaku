//! Write a file so a crash cannot publish half of one.
//!
//! Five callers, counted rather than remembered: `progress::store` (O2),
//! `cover.rs`'s generated cover, `notes::store`, the sidecar rewritten by a
//! correction in `routes.rs`, and `taiyaku-cli`'s `write_sidecar`. The first
//! two carried their own copy of this and said so in a comment; a third copy
//! is where a comment asking whoever changes one to keep the others in step
//! stops being good enough.
//!
//! **This tally said "three" while it was already four, and the count was
//! raised to "four" while it was already five.** Both times the number was
//! written from memory of what this module was for rather than from the call
//! sites. `notes::store` was the one missed twice. If you add a caller, grep
//! for `atomic::write` and count what comes back.
//!
//! **O26 added a sixth command-line writer and the count did not move**, which
//! is the point of counting call sites rather than commands: `taiyaku credits
//! --write` and `taiyaku align --write` both go through one `write_sidecar`,
//! so there is one call site for the two of them. A tally of commands would
//! now be wrong.
//!
//! The last is in another crate, which is why this module is `pub`. It is not
//! a server concern that happens to be reachable — a sidecar is a sidecar
//! whoever rewrites it, and the alternative to exporting this was another copy
//! of write-and-rename in `taiyaku-cli`.

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
///
/// **Both errors name `path` — the file the caller asked for — and never the
/// temporary.** That was O29: the staging branch used to name the temporary,
/// which is a filename this module invents, that no caller typed, that is
/// deleted before the error is returned, and that therefore cannot be looked
/// for. It reached people two ways. `taiyaku align --write /nowhere/out.json`
/// answered `out.json.tmp.4041762.0: No such file or directory`, and — the
/// half the item had recorded as harmless — three of the four callers in this
/// crate put the string straight into a 500 body (`routes.rs`'s
/// `Failed(message)` arm), so `notes::store`, `progress::store` and the
/// sidecar rewrite leaked it to HTTP clients too. Only `cover.rs` discards it.
///
/// The distinction between the two failures is kept, because they are not the
/// same event — a full disk or a missing directory fails the staging write,
/// while a rename fails on a destination that cannot be replaced. It is
/// carried in words rather than by swapping in a path the caller cannot use.
///
/// `pub`, not `pub(crate)`: `taiyaku-cli`'s `write_sidecar` — the one writer
/// behind both `align --write` and `credits --write` — is the caller outside
/// this crate, and it is writing the very file the paragraph above names as the
/// worst thing to lose half of.
pub fn write(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String> {
    let temporary = temp_path(path, extension);
    if let Err(e) = std::fs::write(&temporary, bytes) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "{}: staging a temporary beside it: {e}",
            path.display()
        ));
    }
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!(
                "{}: renaming the temporary into place: {e}",
                path.display()
            ))
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
            .filter_map(|entry| entry.ok())
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
}
