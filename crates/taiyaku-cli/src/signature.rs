//! What a directory listing knows about two copies, before anything is
//! decoded, transferred or solved.
//!
//! This is step 1 of the duplicate protocol in `docs/status.md` §2. It settled
//! 3 of the 19 duplicated One Piece volume-editions without transferring
//! anything, which is the reason it runs first.
//!
//! Called from `duplicates::resolve`, which groups copies by signature before
//! confirming a match with `content_digest` and fingerprinting one
//! representative per group.

use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// What a directory listing knows about one copy of a volume.
///
/// Over *pages* rather than over files, so `docs/sidecar.md`'s "How a member
/// becomes pages" decides what counts and a stray `ComicInfo.xml` or
/// `Thumbs.db` does not move the answer. Over *uncompressed* sizes, so a
/// directory of loose images and a `.cbz` of the same scan sign identically.
///
/// `pages` and `bytes` ride along for a person to read and nothing prints them
/// today; only `digest` decides. It subsumes both, being taken over the whole
/// sorted list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub pages: u32,
    pub bytes: u64,
    pub digest: String,
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
        Self {
            pages: sizes.len() as u32,
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
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// A repacker writes entries in whatever order it likes. Sorting the size
    /// list is what makes the signature independent of that, and it is the one
    /// thing about a re-packaging that always changes.
    #[test]
    fn the_order_pages_were_stored_in_does_not_change_the_signature() {
        let one = Signature::from_sizes(&[300, 100, 200]);
        let other = Signature::from_sizes(&[100, 200, 300]);

        assert_eq!(one.digest, other.digest);
        assert_eq!(one.pages, 3);
        assert_eq!(one.bytes, 600);
    }

    /// And it must still be able to tell two scans apart. One page differing by
    /// a single byte is the smallest difference there is.
    #[test]
    fn one_page_of_a_different_size_changes_the_signature() {
        let one = Signature::from_sizes(&[100, 200, 300]);
        let other = Signature::from_sizes(&[100, 200, 301]);

        assert_ne!(one.digest, other.digest);
    }

    /// The property the whole step rests on: the same scan filed two ways is
    /// one signature. Without this, repackaging defeats step 1 and every set
    /// pays for a full alignment.
    #[test]
    fn a_directory_and_an_archive_of_one_scan_sign_identically() {
        let (dir, archive) = same_pages_two_ways("sig");

        let from_dir = Signature::of(std::slice::from_ref(&dir)).unwrap();
        let from_archive = Signature::of(std::slice::from_ref(&archive)).unwrap();

        clean_up(&dir, &archive);
        assert_eq!(from_dir, from_archive);
    }

    /// The confirmation has to be packaging-independent for the same reason the
    /// signature is. `taiyaku_read::member_digest` is not: it hashes an
    /// archive's file bytes and a directory's page bytes, so it would report
    /// two different answers for one scan and refuse to confirm precisely the
    /// case it was called about.
    #[test]
    fn the_content_digest_sees_through_packaging_too() {
        let (dir, archive) = same_pages_two_ways("content");

        let from_dir = content_digest(std::slice::from_ref(&dir)).unwrap();
        let from_archive = content_digest(std::slice::from_ref(&archive)).unwrap();

        clean_up(&dir, &archive);
        assert_eq!(from_dir, from_archive);
    }

    #[test]
    fn different_page_bytes_produce_a_different_content_digest() {
        let (dir, archive) = same_pages_two_ways("differs");
        std::fs::write(dir.join("p2.jpg"), b"changed").unwrap();

        let from_dir = content_digest(std::slice::from_ref(&dir)).unwrap();
        let from_archive = content_digest(std::slice::from_ref(&archive)).unwrap();

        clean_up(&dir, &archive);
        assert_ne!(from_dir, from_archive);
    }

    /// One scan as a directory of loose images and as a cbz storing the same
    /// entries in a different order.
    fn same_pages_two_ways(tag: &str) -> (PathBuf, PathBuf) {
        use std::io::Write;

        let dir = std::env::temp_dir().join(format!("taiyaku-test-{tag}-dir"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for entry in ["p1.jpg", "p2.jpg", "p10.jpg"] {
            std::fs::write(dir.join(entry), entry.as_bytes()).unwrap();
        }

        let archive = std::env::temp_dir().join(format!("taiyaku-test-{tag}.cbz"));
        let _ = std::fs::remove_file(&archive);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        for entry in ["p10.jpg", "p2.jpg", "p1.jpg"] {
            zip.start_file(entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(entry.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        (dir, archive)
    }

    fn clean_up(dir: &Path, archive: &Path) {
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_file(archive);
    }
}
