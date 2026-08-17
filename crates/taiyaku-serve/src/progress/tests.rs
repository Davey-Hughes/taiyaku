use super::*;

/// A fresh in-memory library, good for nothing but bridging the fixture
/// `Work` this module's tests build. `test_support::library()` is the same
/// helper `ServeState` fixtures use; called directly here since these tests
/// exercise `progress::load`/`store` without a router.
fn library() -> Library {
    taiyaku_library::Library::open_in_memory().expect("in-memory library")
}

/// A work with nothing recorded must be distinguishable from one recorded
/// at slot 0 — "never opened" and "on the first page" are different, and a
/// reader that conflates them reopens at the cover forever.
#[test]
fn absent_progress_is_not_the_same_as_slot_zero() {
    let work = crate::test_support::work("taiyaku-test-progress");
    let library = library();

    let before = load(&library, &work).unwrap();
    store(
        &library,
        &work,
        &Progress {
            edition: "raw".into(),
            slot: 0,
        },
    )
    .unwrap();
    let after = load(&library, &work).unwrap();

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
fn progress_is_written_to_the_library_not_into_the_sidecar() {
    let work = crate::test_support::work("taiyaku-test-progress-beside");
    let library = library();
    let sidecar_before = std::fs::read_to_string(&work.path).unwrap();

    store(
        &library,
        &work,
        &Progress {
            edition: "raw".into(),
            slot: 12,
        },
    )
    .unwrap();

    let sidecar_after = std::fs::read_to_string(&work.path).unwrap();
    crate::test_support::cleanup("taiyaku-test-progress-beside");
    assert_eq!(sidecar_before, sidecar_after, "the sidecar must not change");
}

/// The bridge from a slug-keyed `Work` to the library's numeric `work.id` —
/// the real work of Task 13. A work this library has never heard of gets a
/// row minted for it on its first write, and every read and write after that
/// resolves back onto that same row rather than a fresh one each time.
#[test]
fn storing_progress_mints_exactly_one_bridge_row_for_a_work_the_library_never_imported() {
    let work = crate::test_support::work("taiyaku-test-progress-bridge");
    let library = library();

    assert_eq!(
        crate::works::find_row(&library.conn(), &work).unwrap(),
        None,
        "nothing has been written yet, so no row exists"
    );

    store(
        &library,
        &work,
        &Progress {
            edition: "raw".into(),
            slot: 1,
        },
    )
    .unwrap();
    let first_id = crate::works::find_row(&library.conn(), &work)
        .unwrap()
        .expect("a row now exists");

    store(
        &library,
        &work,
        &Progress {
            edition: "raw".into(),
            slot: 2,
        },
    )
    .unwrap();
    let second_id = crate::works::find_row(&library.conn(), &work)
        .unwrap()
        .expect("still there");

    crate::test_support::cleanup("taiyaku-test-progress-bridge");
    assert_eq!(
        first_id, second_id,
        "the second write reuses the first's row"
    );
    assert_eq!(
        load(&library, &work).unwrap(),
        Some(Progress {
            edition: "raw".into(),
            slot: 2
        })
    );
}

/// A read must never mint a bridge row as a side effect — `works::find_row`'s
/// own doc comment states the rule; this is what proves `progress::load`
/// actually honours it rather than merely being written next to it.
#[test]
fn loading_progress_for_an_unregistered_work_creates_no_row() {
    let work = crate::test_support::work("taiyaku-test-progress-read-only");
    let library = library();

    let loaded = load(&library, &work).unwrap();

    let row = crate::works::find_row(&library.conn(), &work).unwrap();
    crate::test_support::cleanup("taiyaku-test-progress-read-only");
    assert_eq!(loaded, None);
    assert_eq!(row, None, "a GET must not write");
}
