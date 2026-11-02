//! Which Stump book each member is, remembered beside the sidecar.
//!
//! Keyed by the member's `sha256` rather than its path: two members with the
//! same hash are the same file, and a work that names one archive twice should
//! resolve it once.
//!
//! **Absence is recorded too.** Most members of most works in this library are
//! not in Stump at all — every raw scan, every fan translation — and a miss
//! that is not remembered is a GraphQL round trip on every page turn.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::works::Work;

/// Member `sha256` → the Stump book it is, or `None` for "Stump has not got it".
pub type Resolutions = HashMap<String, Option<String>>;

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    taiyaku_stump: u32,
    /// The instance these answers came from.
    ///
    /// Recorded so a cache written against one Stump is not silently believed
    /// against another, where the same ids mean different books or nothing.
    base: String,
    members: Resolutions,
}

/// Where a work's Stump mapping sits: beside its sidecar, same stem.
///
/// `taiyaku.json` stays a pure alignment document that diffs
/// meaningfully in git.
///
/// **The fourth copy of that arithmetic, and the last sibling file left.**
/// `taiyaku_core::TruthDoc::path_beside`, `RegionDoc::path_beside` and
/// `taiyaku_library::export::stem_of` are the other three, each naming the
/// others. A Stump resolution is a cache of another server's ids, not
/// something the library store holds, so it stays a file.
pub fn path_for(work: &Work) -> PathBuf {
    let stem = work.path.file_stem().map_or_else(
        || "taiyaku".to_string(),
        |s| s.to_string_lossy().to_string(),
    );
    work.path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(format!("{stem}.stump.json"))
}

/// What was resolved against this instance, or nothing.
///
/// Every failure is "nothing", deliberately: the cache is derived data that
/// can be rebuilt in one round trip per member, so refusing to start over a
/// truncated file would trade a recoverable state for an unrecoverable one.
pub fn load(work: &Work, base: &str) -> Resolutions {
    let Ok(text) = std::fs::read_to_string(path_for(work)) else {
        return Resolutions::new();
    };
    let Ok(document) = serde_json::from_str::<Document>(&text) else {
        return Resolutions::new();
    };
    if document.taiyaku_stump != 1 || document.base != base {
        return Resolutions::new();
    }
    document.members
}

/// Write the mapping, atomically.
///
/// Write-and-rename rather than `fs::write`, which truncates first: a crash
/// mid-write would otherwise leave a short file.
pub fn store(work: &Work, base: &str, members: &Resolutions) -> Result<(), String> {
    let document = Document {
        taiyaku_stump: 1,
        base: base.to_string(),
        members: members.clone(),
    };
    let text = serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?;
    let final_path = path_for(work);
    let temporary = final_path.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|e| format!("{}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, &final_path).map_err(|e| format!("{}: {e}", final_path.display()))
}

/// Throw away what this work resolved to, so the next start asks again.
///
/// O11's escape hatch. Nothing watches for the library changing on Stump's
/// side — a deliberate tradeoff, recorded as Risk 5 in the design, because a
/// remembered miss costs one query once instead of one per page turn — so a
/// book added to Stump after a work first resolved stays invisible until this
/// file goes away.
///
/// Destructive in exactly the way deleting it by hand is, and for the same
/// reason it is safe: these are derived answers, rebuildable in one query per
/// member.
///
/// A work with nothing recorded is not an error. `--refresh-stump` is typed
/// once and applies to every work the server was handed, most of which will
/// have no cache file; refusing to start because there was nothing to delete
/// would be the worse answer.
pub fn forget(work: &Work) -> Result<(), String> {
    let path = path_for(work);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests;
