use super::{from_json, to_json};
use crate::Position;

/// The legacy form, byte for byte as it exists on disk.
///
/// **Over the real corpus document rather than a fixture**, because a fixture
/// for this would be written by the same hand as the reader of it and would
/// render right and wrong identically. `corpus/gotoubun-v01/taiyaku.progress.json`
/// is `{"edition": "raw-mono", "slot": 193}`, and `taiyaku serve <sidecar>…`
/// runs `migrate::apply_progress` over documents in exactly that shape at
/// every start.
#[test]
fn a_bare_edition_and_slot_reads_as_one_position() {
    let held = from_json("{\n  \"edition\": \"raw-mono\",\n  \"slot\": 193\n}")
        .expect("the legacy form must still read");
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].place.edition, "raw-mono");
    assert_eq!(held[0].place.slot, 193);
    assert_eq!(
        held[0].updated, None,
        "this form records no time, and inventing one here would be this \
         module deciding what `datetime('now')` is"
    );
}

#[test]
fn the_versioned_form_reads_every_position() {
    let held = from_json(
        r#"{"taiyaku_progress":1,"positions":[
             {"edition":"raw","slot":11,"updated":"2026-08-27 09:00:00"},
             {"edition":"full-color","slot":33,"updated":"2026-08-27 09:24:41"}]}"#,
    )
    .expect("the versioned form");
    assert_eq!(
        held.iter()
            .map(|p| p.place.edition.as_str())
            .collect::<Vec<_>>(),
        vec!["raw", "full-color"]
    );
    assert_eq!(held[1].place.slot, 33);
    // **Each position's own time, carried rather than dropped.** Two
    // different times, so a reader that returned either one for both, or the
    // first for both, is red here — and it is `state::put_progress_at` that
    // then writes them, which is what keeps the resume order a reader's own
    // rather than alphabetical.
    assert_eq!(
        held.iter()
            .map(|p| p.updated.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("2026-08-27 09:00:00"), Some("2026-08-27 09:24:41")]
    );
}

/// A version this build does not know is refused rather than guessed at.
#[test]
fn an_unknown_version_is_refused_by_name() {
    let why = from_json(r#"{"taiyaku_progress":2,"positions":[]}"#)
        .expect_err("a newer document must not be read as though it were this one");
    assert!(
        why.contains("taiyaku_progress 2 document"),
        "the refusal must name the version it found, not just any message \
         containing a 2, got: {why}"
    );
}

#[test]
fn export_writes_the_versioned_form_only() {
    let json = to_json(&[Position {
        edition: "raw".into(),
        slot: 11,
        updated: "2026-08-27 09:00:00".into(),
    }])
    .expect("serialising");
    assert!(json.contains("\"taiyaku_progress\": 1"), "got: {json}");
    assert!(json.contains("\"positions\""), "got: {json}");
}
