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
mod tests;
