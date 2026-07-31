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

/// Record where the reader got to.
pub fn store(work: &Work, progress: &Progress) -> Result<(), String> {
    let path = progress_path(work);
    let text =
        serde_json::to_string_pretty(progress).map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
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
}
