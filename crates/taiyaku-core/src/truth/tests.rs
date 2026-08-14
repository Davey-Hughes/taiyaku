use super::*;

fn doc() -> TruthDoc {
    let mut pages = BTreeMap::new();
    pages.insert(
        7,
        vec![
            TruthRegion {
                x: 0.10,
                y: 0.20,
                w: 0.15,
                h: 0.30,
                axis: Some(Axis::Vertical),
                kind: Kind::Dialogue,
                ignore: false,
                text: None,
            },
            TruthRegion {
                x: 0.55,
                y: 0.40,
                w: 0.28,
                h: 0.12,
                axis: None,
                kind: Kind::Sfx,
                ignore: true,
                text: None,
            },
        ],
    );
    // Present and empty: somebody looked at page 8 and there was no text.
    pages.insert(8, Vec::new());
    let mut editions = BTreeMap::new();
    editions.insert(
        "raw-mono".to_string(),
        TruthEdition {
            members: vec![RegionMember {
                sha256: "aa".into(),
            }],
            pages,
        },
    );
    TruthDoc {
        taiyaku_truth: VERSION,
        editions,
    }
}

#[test]
fn a_document_survives_a_round_trip() {
    let there_and_back = TruthDoc::from_json(&doc().to_json()).unwrap();
    assert_eq!(there_and_back.taiyaku_truth, VERSION);
    let edition = &there_and_back.editions["raw-mono"];
    assert_eq!(edition.pages[&7].len(), 2);
    assert_eq!(edition.pages[&7][0].axis, Some(Axis::Vertical));
    assert_eq!(edition.pages[&7][0].kind, Kind::Dialogue);
    assert!(!edition.pages[&7][0].ignore);
    assert_eq!(edition.pages[&7][1].axis, None);
    assert!(edition.pages[&7][1].ignore);
}

#[test]
fn a_page_looked_at_and_a_page_never_opened_are_different() {
    let there_and_back = TruthDoc::from_json(&doc().to_json()).unwrap();
    let pages = &there_and_back.editions["raw-mono"].pages;
    assert!(pages.contains_key(&8), "page 8 was looked at");
    assert!(pages[&8].is_empty(), "and held no text");
    assert!(!pages.contains_key(&9), "page 9 was never opened");
}

#[test]
fn an_axis_may_be_null_because_a_radial_effect_has_no_reading_direction() {
    let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":null,"kind":"sfx"}]}}}}"#;
    let parsed = TruthDoc::from_json(json).unwrap();
    assert_eq!(parsed.editions["e"].pages[&3][0].axis, None);
}

#[test]
fn ignore_and_text_default_when_absent() {
    let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":"v","kind":"dialogue"}]}}}}"#;
    let region = &TruthDoc::from_json(json).unwrap().editions["e"].pages[&3][0];
    assert!(!region.ignore);
    assert_eq!(region.text, None);
}

#[test]
fn an_unknown_kind_is_refused_and_the_message_names_the_field() {
    let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":"v","kind":"balloon"}]}}}}"#;
    let err = TruthDoc::from_json(json).unwrap_err();
    // Naming the rejected value and a `Kind`-only variant, rather than
    // serde's generic "variant": `err.contains("variant")` alone passes
    // just as well when it is `axis` that is malformed, so it does not
    // establish that the *kind* was what was refused.
    assert!(err.contains("balloon"), "names the value it refused: {err}");
    assert!(
        err.contains("dialogue"),
        "names Kind's own variants, so this is Kind and not Axis: {err}"
    );
}

/// A drag that started or ended in the letterbox stores a box bigger than
/// the page, and the scorer divides by that inflated area — so a genuine
/// one-to-one demotes and its prediction turns spurious, with nothing
/// saying why. Refused on the way in, naming the field.
#[test]
fn a_box_off_the_page_is_refused_and_the_message_names_the_field() {
    // A drag that ran off the left edge: x negative.
    let left = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":-0.05,"y":0.1,"w":0.2,"h":0.2,
            "axis":"v","kind":"dialogue"}]}}}}"#;
    let err = TruthDoc::from_json(left).unwrap_err();
    assert!(
        err.contains("x = -0.05"),
        "names the field and value: {err}"
    );
    assert!(err.contains("off the page"), "{err}");

    // A drag that ran off the bottom: y and h are each inside 0..1, and
    // only their sum is not. A per-field check alone cannot see this one.
    let bottom = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.1,"y":0.8,"w":0.2,"h":0.5,
            "axis":"v","kind":"dialogue"}]}}}}"#;
    let err = TruthDoc::from_json(bottom).unwrap_err();
    assert!(err.contains("y + h"), "names the far edge: {err}");
    assert!(err.contains("page 3"), "and where it is: {err}");

    // A width past the page on its own.
    let wide = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.1,"w":1.4,"h":0.2,
            "axis":"v","kind":"dialogue"}]}}}}"#;
    let err = TruthDoc::from_json(wide).unwrap_err();
    assert!(err.contains("w = 1.4"), "{err}");
}

/// And a box that legitimately touches an edge still parses. A clamped
/// drag stores `w = far - x`, and `x + w` need not round back to exactly
/// `far` — refusing that would reject the boxes the labeller writes for
/// text bleeding to the page edge, which is the very case the clamp exists
/// for.
#[test]
fn a_box_flush_against_the_page_edge_is_not_refused() {
    let flush = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":1.0,"h":1.0,
            "axis":"v","kind":"dialogue"},
            {"x":0.7,"y":0.7,"w":0.30000000000000004,"h":0.3,
            "axis":"v","kind":"dialogue"}]}}}}"#;
    let parsed = TruthDoc::from_json(flush).expect("a box on the edge is on the page");
    assert_eq!(parsed.editions["e"].pages[&3].len(), 2);
}

#[test]
fn a_replaced_archive_makes_a_document_stale() {
    let edition = &doc().editions["raw-mono"];
    assert!(!edition.is_stale_for(&["aa".to_string()]));
    assert!(edition.is_stale_for(&["bb".to_string()]), "content changed");
    assert!(edition.is_stale_for(&[]), "member count changed");
}

#[test]
fn the_document_sits_beside_the_sidecar_keeping_a_dotted_stem() {
    assert_eq!(
        TruthDoc::path_beside(Path::new("/c/taiyaku.json")),
        Path::new("/c/taiyaku.truth.json")
    );
    assert_eq!(
        TruthDoc::path_beside(Path::new("/c/go.between.json")),
        Path::new("/c/go.between.truth.json"),
        "set_extension twice would eat the dot"
    );
}
