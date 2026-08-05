//! Where you were, kept beside the sidecar rather than inside it.
//!
//! Position is server-side because it has to survive a different machine and
//! because a progress tracker needs to read it. It is recorded in **slots**,
//! not pages: a slot is the addressed unit everywhere else in the format, and
//! a page would lose which half of a joined spread was reached.
//!
//! It is a sibling file rather than a field in `taiyaku.json`, which stays a
//! pure alignment document that diffs meaningfully in git.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::works::Work;

/// How far through a work a reader got, and in which edition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub edition: String,
    pub slot: u32,
}

/// Where a work's progress file sits: beside its sidecar, same stem.
pub fn progress_path(work: &Work) -> PathBuf {
    work.sibling("progress.json")
}

/// What was recorded, or `None` when nothing ever was.
///
/// The distinction matters: "never opened" and "on the first page" are
/// different, and conflating them reopens a work at its cover forever.
pub fn load(work: &Work) -> Result<Option<Progress>, String> {
    let path = progress_path(work);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Record where the reader got to, atomically.
///
/// Write-and-rename rather than `fs::write`, which truncates first: a crash in
/// that window leaves a short file, and the next `load` answers 500 rather
/// than "nothing recorded" — the worse of the two wrong answers, since it
/// tells the reader the server is broken rather than that it has no position
/// to restore. A page turn writes this file, so the window is entered often.
///
/// The pattern is the one `stump::cache::store` already uses beside the same
/// sidecar, and now `crate::atomic::write` shared by both of them plus
/// `cover.rs`'s writer.
pub fn store(work: &Work, progress: &Progress) -> Result<(), String> {
    let path = progress_path(work);
    let text =
        serde_json::to_string_pretty(progress).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::atomic::write(&path, text.as_bytes(), "json")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A work with nothing recorded must be distinguishable from one recorded
    /// at slot 0 — "never opened" and "on the first page" are different, and a
    /// reader that conflates them reopens at the cover forever.
    #[test]
    fn absent_progress_is_not_the_same_as_slot_zero() {
        let work = crate::test_support::work("taiyaku-test-progress");

        let before = load(&work).unwrap();
        store(
            &work,
            &Progress {
                edition: "raw".into(),
                slot: 0,
            },
        )
        .unwrap();
        let after = load(&work).unwrap();

        crate::test_support::cleanup("taiyaku-test-progress");
        assert_eq!(before, None, "nothing recorded yet");
        assert_eq!(
            after,
            Some(Progress {
                edition: "raw".into(),
                slot: 0
            })
        );
    }

    /// The sidecar must not gain a progress field. It is an alignment document
    /// and a reading position churns it on every page turn.
    #[test]
    fn progress_is_written_beside_the_sidecar_not_into_it() {
        let work = crate::test_support::work("taiyaku-test-progress-beside");
        let sidecar_before = std::fs::read_to_string(&work.path).unwrap();

        store(
            &work,
            &Progress {
                edition: "raw".into(),
                slot: 12,
            },
        )
        .unwrap();

        let sidecar_after = std::fs::read_to_string(&work.path).unwrap();
        let beside = progress_path(&work);
        let exists = beside.exists();
        crate::test_support::cleanup("taiyaku-test-progress-beside");
        assert_eq!(sidecar_before, sidecar_after, "the sidecar must not change");
        assert!(exists, "progress belongs in {}", beside.display());
    }

    /// O2: a page turn must not be able to leave a short file behind.
    ///
    /// `fs::write` opens for truncation and then writes, so a crash in that
    /// window leaves a file that parses as nothing — and `load` answers a 500
    /// rather than "nothing recorded", which is the worse of the two possible
    /// wrong answers: the reader is told the server is broken rather than that
    /// it has no position to restore.
    ///
    /// The crash itself cannot be staged here, so this pins the mechanism that
    /// rules it out — the destination is *replaced* by a rename, never opened
    /// for truncation. Taking write permission off the existing file makes the
    /// difference observable: an in-place write needs it, a rename needs only
    /// a writable directory.
    #[test]
    fn progress_is_replaced_by_a_rename_rather_than_truncated_in_place() {
        let work = crate::test_support::work("taiyaku-test-progress-atomic");
        let path = progress_path(&work);
        store(
            &work,
            &Progress {
                edition: "raw".into(),
                slot: 1,
            },
        )
        .unwrap();

        // The precondition this test measures through. Under root, or on a
        // filesystem that ignores the mode bits, a read-only file accepts an
        // in-place write and the assertion below would pass without measuring
        // anything — so say so rather than report a green that means nothing.
        let mut mode = std::fs::metadata(&path).unwrap().permissions();
        mode.set_readonly(true);
        std::fs::set_permissions(&path, mode).unwrap();
        if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
            crate::test_support::cleanup("taiyaku-test-progress-atomic");
            eprintln!(
                "NOT MEASURED: progress_is_replaced_by_a_rename_rather_than_truncated_in_place \
                 (a read-only file accepts an in-place write here)"
            );
            return;
        }

        let stored = store(
            &work,
            &Progress {
                edition: "raw".into(),
                slot: 2,
            },
        );
        let after = load(&work);
        // Nothing may be left lying beside the sidecar. A rename that failed,
        // or a write that did, must clean up after itself rather than leave
        // one file per page turn in the reader's library directory.
        let strays: Vec<String> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(".tmp"))
            .collect();

        crate::test_support::cleanup("taiyaku-test-progress-atomic");
        stored.expect("a rename does not need the old file to be writable");
        assert_eq!(
            after.unwrap(),
            Some(Progress {
                edition: "raw".into(),
                slot: 2
            })
        );
        assert!(strays.is_empty(), "temp files left behind: {strays:?}");
    }
}
