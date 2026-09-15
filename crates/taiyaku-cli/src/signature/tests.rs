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
    let (_scratch, dir, archive) = same_pages_two_ways("sig");

    let from_dir = Signature::of(std::slice::from_ref(&dir)).unwrap();
    let from_archive = Signature::of(std::slice::from_ref(&archive)).unwrap();

    assert_eq!(from_dir, from_archive);
}

/// The confirmation has to be packaging-independent for the same reason the
/// signature is. `taiyaku_read::member_digest` is not: it hashes an
/// archive's file bytes and a directory's page bytes, so it would report
/// two different answers for one scan and refuse to confirm precisely the
/// case it was called about.
#[test]
fn the_content_digest_sees_through_packaging_too() {
    let (_scratch, dir, archive) = same_pages_two_ways("content");

    let from_dir = content_digest(std::slice::from_ref(&dir)).unwrap();
    let from_archive = content_digest(std::slice::from_ref(&archive)).unwrap();

    assert_eq!(from_dir, from_archive);
}

#[test]
fn different_page_bytes_produce_a_different_content_digest() {
    let (_scratch, dir, archive) = same_pages_two_ways("differs");
    std::fs::write(dir.join("p2.jpg"), b"changed").unwrap();

    let from_dir = content_digest(std::slice::from_ref(&dir)).unwrap();
    let from_archive = content_digest(std::slice::from_ref(&archive)).unwrap();

    assert_ne!(from_dir, from_archive);
}

/// One scan as a directory of loose images and as a cbz storing the same
/// entries in a different order.
fn same_pages_two_ways(tag: &str) -> (taiyaku_read::test_support::TempDir, PathBuf, PathBuf) {
    use std::io::Write;

    let scratch = taiyaku_read::test_support::TempDir::new(&format!("signature-{tag}"));
    let dir = scratch.join("dir");
    std::fs::create_dir_all(&dir).unwrap();
    for entry in ["p1.jpg", "p2.jpg", "p10.jpg"] {
        std::fs::write(dir.join(entry), entry.as_bytes()).unwrap();
    }

    let archive = scratch.join("pages.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    for entry in ["p10.jpg", "p2.jpg", "p1.jpg"] {
        zip.start_file(entry, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(entry.as_bytes()).unwrap();
    }
    zip.finish().unwrap();

    (scratch, dir, archive)
}
