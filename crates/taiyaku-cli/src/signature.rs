//! What a directory listing knows about two copies, before anything is
//! decoded, transferred or solved.
//!
//! This is step 1 of the duplicate protocol in `docs/status.md` §2.
//!
//! Called from `duplicates::resolve`, which groups copies by signature before
//! confirming a match with `content_digest` and fingerprinting one
//! representative per group.

use std::fmt::Write as _;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// What a directory listing knows about one copy of a volume.
///
/// Over *pages* rather than over files, so `docs/sidecar.md`'s "How a member
/// becomes pages" decides what counts and a stray `ComicInfo.xml` or
/// `Thumbs.db` does not move the answer. Over *uncompressed* sizes, so a
/// directory of loose images and a `.cbz` of the same scan sign identically.
///
/// `pages` and `bytes` ride along for a person to read and nothing prints
/// them; only `digest` decides. It subsumes both, being taken over the whole
/// sorted list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub pages: u32,
    pub bytes: u64,
    pub digest: String,
}

/// A page count, as `Signature.pages` stores it.
///
/// # Panics
///
/// Panics if `n` exceeds `u32::MAX`. It cannot here: `n` is `sizes.len()`, a
/// copy's page count across its members from
/// `taiyaku_read::member_page_sizes`; no real archive in this tool's corpus
/// runs anywhere near `u32::MAX` pages.
fn page_count(n: usize) -> u32 {
    u32::try_from(n).expect("page count exceeds u32::MAX")
}

impl Signature {
    /// Sign one copy, which may be several members, without reading a page.
    pub fn of(members: &[PathBuf]) -> Result<Self, String> {
        let mut sizes = Vec::new();
        for member in ordered(members) {
            sizes.extend(taiyaku_read::member_page_sizes(&member)?);
        }
        Ok(Self::from_sizes(&sizes))
    }

    /// The signature of a page-size list.
    ///
    /// Sorted here rather than by the caller, because a caller that forgot
    /// would get an answer that looked right and was packaging-dependent —
    /// which is the one failure this step cannot afford, since nothing
    /// downstream re-checks it.
    pub fn from_sizes(sizes: &[u64]) -> Self {
        let mut sorted = sizes.to_vec();
        sorted.sort_unstable();
        let mut hasher = Sha256::new();
        for size in &sorted {
            hasher.update(size.to_le_bytes());
        }
        // An intermediate binding: Rust's attribute grammar allows no attribute on a
        // struct-literal field, so `pages` is bound here rather than inline below.
        let pages = page_count(sizes.len());
        Self {
            pages,
            bytes: sizes.iter().sum(),
            digest: hex(hasher),
        }
    }
}

/// A hash of a copy's page bytes, in reading order: what *confirms* a signature
/// match rather than assuming it.
///
/// Not `taiyaku_read::member_digest`, which hashes an archive's file bytes and
/// a directory's page bytes. Those disagree for one scan filed two ways, which
/// is the case a signature match is most likely to be reporting, so using it
/// here would refuse to confirm exactly what it was called about. Hashing pages
/// makes the confirmation packaging-independent in the same way the signature
/// is.
///
/// One member is read and dropped at a time, so a copy costs its largest member
/// in memory rather than all of it.
pub fn content_digest(members: &[PathBuf]) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for member in ordered(members) {
        for page in taiyaku_read::read_edition(&[member])? {
            // Each page's length before its bytes, so the digest is over a page
            // sequence rather than over one concatenation: without it two copies
            // that split the same total run of bytes into different pages hash
            // alike, and this is the hash that *confirms* a signature match.
            hasher.update(page.bytes.len().to_le_bytes());
            hasher.update(&page.bytes);
        }
    }
    Ok(hex(hasher))
}

/// A copy's members in the order they read, which is not the order they were
/// typed: a shell glob yields `c10.5, c10, c1, c2`. Same funnel as
/// `split_editions` in `main.rs`, and for the same reason.
fn ordered(members: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted = members.to_vec();
    sorted.sort_by(|a, b| taiyaku_read::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    sorted
}

fn hex(hasher: Sha256) -> String {
    hasher.finalize().iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

#[cfg(test)]
mod tests;
