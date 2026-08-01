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
    let stem = work
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "taiyaku".to_string());
    work.path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(format!("{stem}.progress.json"))
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

/// A temp path unique to this write attempt.
///
/// The rename below protects a *reader* from ever seeing a half-written
/// position; it does nothing for two *writers* sharing one temp name. Progress
/// is the one thing here a person can write from two devices at once — a phone
/// and a tablet on the same work — and with a fixed name their two attempts
/// interleave: one truncates the temp file the other has already written and
/// is about to rename, and what gets published is the empty file this whole
/// function exists to prevent. A process id plus a monotonic per-process
/// counter rules that out without a new dependency.
///
/// The same reasoning, and the same shape, as `cover::unique_temp_path`. They
/// are not shared yet.
fn unique_temp_path(path: &std::path::Path) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    path.with_extension(format!("json.tmp.{pid}.{n}"))
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
/// sidecar.
pub fn store(work: &Work, progress: &Progress) -> Result<(), String> {
    let path = progress_path(work);
    let text =
        serde_json::to_string_pretty(progress).map_err(|e| format!("{}: {e}", path.display()))?;
    let temporary = unique_temp_path(&path);
    std::fs::write(&temporary, text).map_err(|e| format!("{}: {e}", temporary.display()))?;
    match std::fs::rename(&temporary, &path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Nothing else will ever collect this one: its name is unique to
            // this attempt, so a leftover accumulates one file per failure in
            // the reader's own library directory, beside the sidecar.
            let _ = std::fs::remove_file(&temporary);
            Err(format!("{}: {e}", path.display()))
        }
    }
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
