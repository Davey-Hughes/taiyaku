//! Caching what decoding costs, keyed by content.
//!
//! Decoding dominates alignment by orders of magnitude, and it produces the
//! same answer every time for the same bytes. Caching it makes re-solving
//! cheap, which is what makes tuning the solver bearable.
//!
//! The key is the member's SHA-256, so the cache invalidates correctly when an
//! archive is re-downloaded and is shared between every pairing that uses it.
//! Fingerprints are per-member, not per-pair: five editions of one volume means
//! five cached entries, after which all twenty pairings are free.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use taiyaku_core::PageHash;

use crate::fingerprint;

/// What decoding a page tells us, stored so it never has to be decoded again.
///
/// Dimensions are kept alongside the hashes because whether a page is a joined
/// spread is decided relative to its edition, and that decision may change
/// while the pixels do not. Half hashes are stored for every landscape page
/// regardless, so a later change of mind about the spread rule costs nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPage {
    pub width: u32,
    pub height: u32,
    pub hash: PageHash,
    /// Left and right halves, present for any landscape page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halves: Option<(PageHash, PageHash)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMember {
    /// Format of the cache entry itself, so a change of hash can invalidate.
    pub version: u32,
    pub sha256: String,
    pub pages: Vec<CachedPage>,
}

/// Bumped when the stored hashes would no longer be comparable.
const CACHE_VERSION: u32 = 1;

fn entry_path(root: &Path, digest: &str) -> PathBuf {
    root.join(format!("{digest}.json"))
}

/// Fingerprint a member, reading from the cache when the bytes are unchanged.
///
/// Returns the pages and whether the work was avoided, so callers can report
/// honestly rather than implying a cold run was fast.
pub fn member_pages(root: &Path, member: &Path) -> Result<(Vec<CachedPage>, bool), String> {
    let digest = taiyaku_read::member_digest(member)?;
    let path = entry_path(root, &digest);

    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(entry) = serde_json::from_str::<CachedMember>(&text)
        && entry.version == CACHE_VERSION
        && entry.sha256 == digest
    {
        return Ok((entry.pages, true));
    }

    let decoded =
        fingerprint::describe_pages(&taiyaku_read::read_edition(&[member.to_path_buf()])?)?;
    let entry = CachedMember {
        version: CACHE_VERSION,
        sha256: digest,
        pages: decoded.clone(),
    };
    // A cache that cannot be written is a slow cache, not a broken one.
    if std::fs::create_dir_all(root).is_ok() {
        let _ = std::fs::write(&path, serde_json::to_string(&entry).unwrap_or_default());
    }
    Ok((decoded, false))
}

#[cfg(test)]
mod tests;
