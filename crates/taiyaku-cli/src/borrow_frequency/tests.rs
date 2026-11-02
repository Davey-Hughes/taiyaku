use super::*;

/// Two editions where `b` has no counterpart for the hub's slot 2, and
/// nothing else to lend it either.
///
/// Hand-written JSON rather than a corpus volume, so the port is checked
/// without the archives and this test runs in the ordinary gate.
fn two_editions() -> String {
    r#"{
          "taiyaku": 1,
          "work": { "title": "fixture", "reading": "rtl" },
          "hub": "a",
          "editions": {
            "a": { "label": "a", "members": [{ "path": "a", "sha256": "aa", "pages": 4 }] },
            "b": { "label": "b", "members": [{ "path": "b", "sha256": "bb", "pages": 4 }] }
          },
          "alignments": [{
            "from": "a", "to": "b", "by": "solver",
            "solver": { "hash": "dhash-16", "floor": 0.5 },
            "quality": { "verdict": "sound", "slots": 4, "paired": 3, "avg_run": 3.0 },
            "runs": [{ "a": "0-1", "b": "0-1" }, { "a": "2" }, { "a": "3", "b": "2" }]
          }]
        }"#
    .to_string()
}

/// The same shape as `two_editions`, plus `c`, which pairs to every `a`
/// slot including 2 — the one `b` lacks. What lets this fixture exercise
/// `lender_for`'s third-edition arm without `TAIYAKU_CORPUS`.
fn three_editions() -> String {
    r#"{
          "taiyaku": 1,
          "work": { "title": "fixture", "reading": "rtl" },
          "hub": "a",
          "editions": {
            "a": { "label": "a", "members": [{ "path": "a", "sha256": "aa", "pages": 4 }] },
            "b": { "label": "b", "members": [{ "path": "b", "sha256": "bb", "pages": 4 }] },
            "c": { "label": "c", "members": [{ "path": "c", "sha256": "cc", "pages": 4 }] }
          },
          "alignments": [{
            "from": "a", "to": "b", "by": "solver",
            "solver": { "hash": "dhash-16", "floor": 0.5 },
            "quality": { "verdict": "sound", "slots": 4, "paired": 3, "avg_run": 3.0 },
            "runs": [{ "a": "0-1", "b": "0-1" }, { "a": "2" }, { "a": "3", "b": "2" }]
          }, {
            "from": "a", "to": "c", "by": "solver",
            "solver": { "hash": "dhash-16", "floor": 0.5 },
            "quality": { "verdict": "sound", "slots": 4, "paired": 4, "avg_run": 4.0 },
            "runs": [{ "a": "0-3", "b": "0-3" }]
          }]
        }"#
    .to_string()
}

#[test]
fn ordered_editions_puts_the_hub_first_then_sorts() {
    let editions = vec!["c".to_string(), "a".to_string(), "b".to_string()];
    assert_eq!(ordered_editions("b", &editions), vec!["b", "a", "c"]);
}

#[test]
fn a_paired_slot_is_at_and_an_unpaired_one_with_no_third_edition_is_a_gap() {
    let json = two_editions();
    let b = Binding::from_json(&json).expect("fixture parses");
    let order = ordered_editions("a", &b.editions());
    // Slot 0 pairs, so the reader sees a real `b` page.
    assert_eq!(shown_at(&b, &order, "a", 0, "b"), Shown::At(0));
    // Slot 2 does not, and with only two editions there is nothing to lend it:
    // the authority does not lend.
    assert_eq!(shown_at(&b, &order, "a", 2, "b"), Shown::Gap);
}

#[test]
fn an_unpaired_slot_borrows_a_third_edition_where_one_resolves() {
    let json = three_editions();
    let b = Binding::from_json(&json).expect("fixture parses");
    let order = ordered_editions("a", &b.editions());
    // Slot 2 has no `b` counterpart but does have a `c` one, so `c` lends it
    // rather than the authority — `a` is skipped like `b` is for `viewing`, and
    // there is no fallback beyond it.
    assert_eq!(
        shown_at(&b, &order, "a", 2, "b"),
        Shown::Borrowed("c".to_string())
    );
}
