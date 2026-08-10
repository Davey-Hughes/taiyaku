//! Caching what decoding costs, keyed by content.
//!
//! Decoding dominates alignment by three orders of magnitude — roughly 12 ms per
//! page against 100 µs for an entire similarity matrix — and it produces the
//! same answer every time for the same bytes. Caching it turns re-solving from
//! seconds into microseconds, which is what makes tuning the solver bearable.
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
mod tests {
    use super::*;

    /// A member of three small pages as a directory, which `member_digest` and
    /// `read_edition` take just as they take an archive, and which a test can
    /// build without a zip writer.
    ///
    /// The pages differ from each other so the entry that gets cached holds
    /// three distinct hashes: an entry of three identical ones would be
    /// returned correctly by a cache that had mixed the order up.
    fn member_of_three_pages(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        for page in 0..3u32 {
            let image = image::GrayImage::from_fn(24, 32, |x, y| {
                image::Luma([((x * 7 + y * 31 + page * 53) % 200) as u8])
            });
            image.save(dir.join(format!("{page:03}.png"))).unwrap();
        }
    }

    /// A temporary root nobody else is using, removed first in case a previous
    /// run left one behind.
    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("taiyaku-test-cache-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// The whole point of the cache, and the only thing that proves the entry
    /// was found rather than silently rewritten: the second run reports itself
    /// as cached and returns what the first one decoded.
    ///
    /// This also pins where an entry is written. A path that did not depend on
    /// the digest — every member sharing one file, or every member missing its
    /// own — reads back as a permanent miss, which is a cache that is merely
    /// slow and so fails nothing else.
    #[test]
    fn a_second_run_over_unchanged_bytes_is_answered_from_the_cache() {
        let root = scratch("hit");
        let member = root.join("v01");
        member_of_three_pages(&member);

        let (cold, was_cached) = member_pages(&root.join("cache"), &member).expect("decodes");
        let (warm, was_cached_again) =
            member_pages(&root.join("cache"), &member).expect("reads the cache");

        let _ = std::fs::remove_dir_all(&root);
        assert!(!was_cached, "the first run has nothing to read");
        assert!(was_cached_again, "the second run must not decode again");
        assert_eq!(cold.len(), 3);
        let hashes = |pages: &[CachedPage]| pages.iter().map(|p| p.hash).collect::<Vec<_>>();
        assert_eq!(
            hashes(&cold),
            hashes(&warm),
            "the cached hashes are the decoded ones, in page order"
        );
        assert_eq!(
            hashes(&cold).len(),
            3,
            "three distinct pages, or the comparison above proves nothing"
        );
        assert_ne!(cold[0].hash, cold[1].hash);
    }

    /// An entry written before the hash changed is not comparable with one
    /// written after it, which is what `version` is for. Accepting it would
    /// align a volume against hashes from a different fingerprint.
    #[test]
    fn an_entry_from_another_cache_version_is_ignored_rather_than_trusted() {
        let root = scratch("version");
        let member = root.join("v01");
        member_of_three_pages(&member);
        let cache = root.join("cache");

        let (pages, _) = member_pages(&cache, &member).expect("decodes");
        // Rewrite the entry with everything intact except the version.
        let digest = taiyaku_read::member_digest(&member).unwrap();
        let stale = CachedMember {
            version: CACHE_VERSION + 1,
            sha256: digest.clone(),
            pages,
        };
        std::fs::write(
            entry_path(&cache, &digest),
            serde_json::to_string(&stale).unwrap(),
        )
        .unwrap();

        let (_, was_cached) = member_pages(&cache, &member).expect("decodes again");

        let _ = std::fs::remove_dir_all(&root);
        assert!(!was_cached, "an entry of another version is not a hit");
    }

    /// The entry names the bytes it describes, and the file it sits in is named
    /// for them too. Trusting an entry whose recorded digest disagrees with the
    /// member's would mean returning one archive's hashes for another.
    #[test]
    fn an_entry_whose_digest_disagrees_with_the_member_is_ignored() {
        let root = scratch("digest");
        let member = root.join("v01");
        member_of_three_pages(&member);
        let cache = root.join("cache");

        let (pages, _) = member_pages(&cache, &member).expect("decodes");
        let digest = taiyaku_read::member_digest(&member).unwrap();
        let mismatched = CachedMember {
            version: CACHE_VERSION,
            sha256: "not the digest of anything".to_string(),
            pages,
        };
        std::fs::write(
            entry_path(&cache, &digest),
            serde_json::to_string(&mismatched).unwrap(),
        )
        .unwrap();

        let (_, was_cached) = member_pages(&cache, &member).expect("decodes again");

        let _ = std::fs::remove_dir_all(&root);
        assert!(!was_cached, "an entry that names other bytes is not a hit");
    }
}
