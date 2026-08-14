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

/// O2: a page turn must not be able to leave a short file behind.
///
/// `fs::write` opens for truncation and then writes, so a crash in that
/// window leaves a file that parses as nothing — and `load` answers a 500
/// rather than "nothing recorded", which is the worse of the two possible
/// wrong answers: the reader is told the server is broken rather than that
/// it has no position to restore.
///
/// The crash itself cannot be staged here, so this pins the mechanism that
/// rules it out — the destination is *replaced* by a rename, never opened
/// for truncation. Taking write permission off the existing file makes the
/// difference observable: an in-place write needs it, a rename needs only
/// a writable directory.
#[test]
fn progress_is_replaced_by_a_rename_rather_than_truncated_in_place() {
    let work = crate::test_support::work("taiyaku-test-progress-atomic");
    let path = progress_path(&work);
    store(
        &work,
        &Progress {
            edition: "raw".into(),
            slot: 1,
        },
    )
    .unwrap();

    // The precondition this test measures through. Under root, or on a
    // filesystem that ignores the mode bits, a read-only file accepts an
    // in-place write and the assertion below would pass without measuring
    // anything — so say so rather than report a green that means nothing.
    let mut mode = std::fs::metadata(&path).unwrap().permissions();
    mode.set_readonly(true);
    std::fs::set_permissions(&path, mode).unwrap();
    if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
        crate::test_support::cleanup("taiyaku-test-progress-atomic");
        eprintln!(
            "NOT MEASURED: progress_is_replaced_by_a_rename_rather_than_truncated_in_place \
                 (a read-only file accepts an in-place write here)"
        );
        return;
    }

    let stored = store(
        &work,
        &Progress {
            edition: "raw".into(),
            slot: 2,
        },
    );
    let after = load(&work);
    // Nothing may be left lying beside the sidecar. A rename that failed,
    // or a write that did, must clean up after itself rather than leave
    // one file per page turn in the reader's library directory.
    let strays: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".tmp"))
        .collect();

    crate::test_support::cleanup("taiyaku-test-progress-atomic");
    stored.expect("a rename does not need the old file to be writable");
    assert_eq!(
        after.unwrap(),
        Some(Progress {
            edition: "raw".into(),
            slot: 2
        })
    );
    assert!(strays.is_empty(), "temp files left behind: {strays:?}");
}

/// `NotFound` means "never opened"; any other read failure must not be
/// reported as that. The guard was replaceable by `true` — and the whole
/// point of `absent_progress_is_not_the_same_as_slot_zero` is that
/// "nothing recorded" is a meaningful answer, so handing it back for an
/// unreadable file reopens the reader at the cover and then overwrites
/// whatever was there.
#[test]
fn an_unreadable_progress_file_is_an_error_rather_than_never_opened() {
    let work = crate::test_support::work("taiyaku-test-progress-unreadable");
    std::fs::create_dir_all(progress_path(&work)).unwrap();

    let result = load(&work);

    crate::test_support::cleanup("taiyaku-test-progress-unreadable");
    assert!(
        result.is_err(),
        "must not read as `never opened`: {result:?}"
    );
}
