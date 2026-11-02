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

/// A temporary root nobody else is using, removed when the test ends.
fn scratch(name: &str) -> taiyaku_read::test_support::TempDir {
    // O117's shared helper: unique to the process and the call, and removed
    // when the test ends.
    taiyaku_read::test_support::TempDir::new(&format!("cache-{name}"))
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
