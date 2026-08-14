use super::*;

/// A URL has to name a work, and the sidecar carries no id — only a title,
/// which is neither unique nor stable. The filename is, so the id comes
/// from there, slugified so it survives a URL without escaping.
#[test]
fn a_work_is_named_by_its_sidecar_filename() {
    assert_eq!(
        work_id(Path::new("/library/Go-Toubun no Hanayome.taiyaku.json")),
        "go-toubun-no-hanayome-taiyaku"
    );
    assert_eq!(work_id(Path::new("taiyaku.json")), "taiyaku");
}

/// The id comes from the filename, so two sidecars in two directories with
/// the same filename — `taiyaku.json` beside each work is the convention
/// this project documents — are one id. Serving both would make one of
/// them permanently unreachable at every route, with nothing anywhere
/// saying which one won or that a choice was made at all.
///
/// O4: the code has refused this since M2 and nothing exercised it, so it
/// was correct by inspection only. The message has to name **both** paths,
/// because the whole point is that the person running the server cannot
/// otherwise tell which two files collided.
#[test]
fn two_sidecars_claiming_one_work_id_are_refused_rather_than_shadowed() {
    let first = crate::test_support::work("taiyaku-test-works-clash-first");
    let second = crate::test_support::work("taiyaku-test-works-clash-second");

    let refused = load_works(&[first.path.clone(), second.path.clone()]);

    crate::test_support::cleanup("taiyaku-test-works-clash-first");
    crate::test_support::cleanup("taiyaku-test-works-clash-second");

    let message = refused.expect_err("two files naming one work must not load");
    assert!(
        message.contains(&first.path.display().to_string()),
        "must name the sidecar already holding the id: {message}"
    );
    assert!(
        message.contains(&second.path.display().to_string()),
        "must name the sidecar that collided with it: {message}"
    );
    assert!(
        message.contains("taiyaku"),
        "must name the id they both claim: {message}"
    );
}

/// The other half of the same guard, so a refusal cannot be spelled
/// "refuse everything": two sidecars whose filenames differ load together
/// and keep their own ids.
#[test]
fn two_differently_named_sidecars_load_side_by_side() {
    let first = crate::test_support::work("taiyaku-test-works-distinct-first");
    let second = crate::test_support::work("taiyaku-test-works-distinct-second");
    let renamed = second.path.with_file_name("other.taiyaku.json");
    std::fs::rename(&second.path, &renamed).unwrap();

    let loaded = load_works(&[first.path.clone(), renamed]);

    crate::test_support::cleanup("taiyaku-test-works-distinct-first");
    crate::test_support::cleanup("taiyaku-test-works-distinct-second");

    let works = loaded.expect("distinct filenames are distinct works");
    let ids: Vec<&str> = works.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["taiyaku", "other-taiyaku"]);
}

/// The property the correction route rests on: what a later reader gets
/// back is what the last write put in, without a restart.
// `update`'s own `#[allow(clippy::result_large_err)]` does not reach a
// closure written at the call site — clippy checks each closure as its
// own item — so the explicit `Result<(), Response>` below needs the same
// allow, for the same reason.
#[allow(clippy::result_large_err)]
#[test]
fn a_replaced_sidecar_is_what_the_next_reader_sees() {
    let work = crate::test_support::work("taiyaku-test-works-replace");
    let before = work.sidecar();

    let swapped: Result<(), axum::response::Response> = work.update(|current| {
        let mut next = (**current).clone();
        next.work.title = "Renamed".into();
        Ok((std::sync::Arc::new(next), ()))
    });

    let after = work.sidecar();
    crate::test_support::cleanup("taiyaku-test-works-replace");
    swapped.expect("the update runs");
    assert_eq!(before.work.title, "T");
    assert_eq!(after.work.title, "Renamed");
}
