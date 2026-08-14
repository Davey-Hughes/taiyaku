use super::*;

fn work(dir: &str) -> Work {
    crate::test_support::work(dir)
}

/// O11: nothing watches for the library changing on Stump's side, so a
/// book added there after a work's first resolution stays invisible until
/// this file goes away. That was true before and stays true — it is the
/// deliberate tradeoff the design records as Risk 5, and the reason for it
/// (a miss costing one query once rather than one per page turn) has not
/// changed. What was wrong is that the only way to act on it was to know,
/// from reading the source, that a file called `taiyaku.stump.json` exists
/// and can be deleted.
///
/// So this is the manual workaround made into an operation. Deliberately
/// destructive in exactly the way deleting the file by hand is: the
/// answers are derived data, rebuildable in one query per member, and the
/// whole point is to stop believing them.
#[test]
fn forgetting_a_work_makes_the_next_run_ask_stump_again() {
    let work = work("taiyaku-test-stump-cache-forget");
    let mut resolutions = Resolutions::new();
    resolutions.insert("hash-of-en-mono".into(), Some("a8ed51d8".into()));
    store(&work, "http://nas:10801", &resolutions).unwrap();

    let before = load(&work, "http://nas:10801");
    forget(&work).unwrap();
    let after = load(&work, "http://nas:10801");
    let gone = !path_for(&work).exists();

    crate::test_support::cleanup("taiyaku-test-stump-cache-forget");
    assert_eq!(before.len(), 1, "the resolution was there to forget");
    assert!(after.is_empty(), "and is not there afterwards: {after:?}");
    assert!(gone, "the file itself is removed, not emptied");
}

/// Forgetting a work that never resolved anything is not an error. The
/// flag is a thing a person types before starting a server, across every
/// work it was handed, and most of them will not have a cache file — one
/// refusing to start the server because there was nothing to delete would
/// be a worse answer than doing nothing.
#[test]
fn forgetting_a_work_with_nothing_recorded_is_not_an_error() {
    let work = work("taiyaku-test-stump-cache-forget-absent");

    let forgotten = forget(&work);

    crate::test_support::cleanup("taiyaku-test-stump-cache-forget-absent");
    assert!(forgotten.is_ok(), "{forgotten:?}");
}

/// A resolution survives a restart, including a recorded absence — which
/// is the common case here and the one worth not re-asking about.
#[test]
fn resolutions_round_trip_beside_the_sidecar() {
    let work = work("taiyaku-test-stump-cache");
    let mut resolutions = Resolutions::new();
    resolutions.insert("hash-of-en-mono".into(), Some("a8ed51d8".into()));
    resolutions.insert("hash-of-raw-mono".into(), None);

    store(&work, "http://nas:10801", &resolutions).unwrap();
    let back = load(&work, "http://nas:10801");

    let path = path_for(&work);
    let existed = path.exists();
    crate::test_support::cleanup("taiyaku-test-stump-cache");

    assert!(existed, "must write beside the sidecar: {}", path.display());
    assert_eq!(
        back.get("hash-of-en-mono"),
        Some(&Some("a8ed51d8".to_string()))
    );
    assert_eq!(
        back.get("hash-of-raw-mono"),
        Some(&None),
        "an absence must be remembered, or every page turn asks again"
    );
}

/// A cache written against one instance says nothing about another. Book
/// ids are that instance's, so believing them would serve pages from
/// whatever happens to share an id — or 404 on everything.
#[test]
fn a_cache_from_a_different_instance_is_ignored() {
    let work = work("taiyaku-test-stump-cache-base");
    let mut resolutions = Resolutions::new();
    resolutions.insert("hash".into(), Some("id-on-the-old-server".into()));
    store(&work, "http://old:10801", &resolutions).unwrap();

    let back = load(&work, "http://new:10801");

    crate::test_support::cleanup("taiyaku-test-stump-cache-base");
    assert!(back.is_empty(), "a different base must not be trusted");
}

/// Nothing cached yet is not an error, and neither is a corrupt file: both
/// mean "resolve again". A server that refused to start because a cache it
/// can rebuild was truncated would be worse than one that rebuilds it.
#[test]
fn a_missing_or_unreadable_cache_is_empty_rather_than_fatal() {
    let work = work("taiyaku-test-stump-cache-bad");
    assert!(
        load(&work, "http://nas:10801").is_empty(),
        "nothing written yet"
    );

    std::fs::write(path_for(&work), b"{ truncated").unwrap();
    let back = load(&work, "http://nas:10801");

    crate::test_support::cleanup("taiyaku-test-stump-cache-bad");
    assert!(
        back.is_empty(),
        "a corrupt cache rebuilds rather than refusing to start"
    );
}

/// `forget` treats a missing cache file as success — deleting nothing is
/// the right answer to "make sure this is gone". Any *other* failure is
/// not: a cache file that cannot be removed means the next start reads a
/// stale resolution, and reporting that as `Ok(())` is how it goes
/// unnoticed. The guard could be replaced by `true` with the suite green.
#[test]
fn a_cache_file_that_cannot_be_removed_is_an_error_rather_than_success() {
    let work = work("taiyaku-test-stump-forget-unremovable");
    // `remove_file` on a directory fails with something that is not
    // `NotFound`, which is the distinction under test.
    std::fs::create_dir_all(path_for(&work)).unwrap();

    let result = forget(&work);

    let _ =
        std::fs::remove_dir_all(std::env::temp_dir().join("taiyaku-test-stump-forget-unremovable"));
    assert!(result.is_err(), "must not report success: {result:?}");
}
