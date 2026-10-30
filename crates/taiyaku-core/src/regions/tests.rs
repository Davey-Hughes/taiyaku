use super::*;

fn doc() -> RegionDoc {
    let mut pages = BTreeMap::new();
    pages.insert(
        4,
        vec![Region {
            x: 0.12,
            y: 0.08,
            w: 0.11,
            h: 0.31,
            axis: Axis::Vertical,
        }],
    );
    pages.insert(5, Vec::new());
    let mut editions = BTreeMap::new();
    editions.insert(
        "raw-mono".to_string(),
        RegionEdition {
            members: vec![RegionMember {
                sha256: "3f9a".to_string(),
            }],
            pages,
        },
    );
    RegionDoc {
        taiyaku_regions: VERSION,
        editions,
    }
}

#[test]
fn a_document_round_trips_through_json() {
    let parsed = RegionDoc::from_json(&doc().to_json()).expect("parses");
    assert_eq!(
        parsed.editions["raw-mono"].pages[&4][0].axis,
        Axis::Vertical
    );
    assert_eq!(parsed.editions["raw-mono"].pages[&4][0].x, 0.12);
    assert_eq!(parsed.editions["raw-mono"].members[0].sha256, "3f9a");
}

/// The distinction is load-bearing and is why `pages` is a map rather than
/// a list: a page that was looked at and holds no text is a different fact
/// from a page nothing has looked at, and only the first can be stated.
#[test]
fn a_page_detected_empty_is_not_a_page_never_detected() {
    let parsed = RegionDoc::from_json(&doc().to_json()).expect("parses");
    let pages = &parsed.editions["raw-mono"].pages;
    assert!(pages.contains_key(&5), "5 was detected and found nothing");
    assert!(pages[&5].is_empty());
    assert!(!pages.contains_key(&6), "6 was never detected");
}

#[test]
fn an_axis_is_one_character_in_the_file() {
    assert!(doc().to_json().contains("\"axis\": \"v\""));
}

/// Page keys are integers in Rust and strings in JSON.
///
/// **This does not discriminate a `u32` key from a `String` one**:
/// `serde_json` stringifies any map key, so a `BTreeMap<String, _>` emits the
/// identical bytes. That property is a serde guarantee rather than something
/// this crate can break. What it still catches is a change of *shape* — pages
/// emitted as an array of pairs would write `4`, unquoted, and fail here.
#[test]
fn page_keys_are_strings_in_the_file() {
    assert!(doc().to_json().contains("\"4\""));
}

#[test]
fn a_replaced_archive_makes_an_edition_stale() {
    let edition = &doc().editions["raw-mono"];
    assert!(!edition.is_stale_for(&["3f9a".to_string()]));
    assert!(edition.is_stale_for(&["c104".to_string()]));
    assert!(
        edition.is_stale_for(&["3f9a".to_string(), "c104".to_string()]),
        "a member added to the edition is a change too"
    );
}

/// The same rule `sidecar.md` states for the sidecar: order is page order,
/// so a reordering is a different edition and not the same one.
#[test]
fn member_order_is_part_of_the_identity() {
    let mut two = doc();
    let edition = two.editions.get_mut("raw-mono").expect("present");
    edition.members = vec![
        RegionMember {
            sha256: "a".to_string(),
        },
        RegionMember {
            sha256: "b".to_string(),
        },
    ];
    assert!(!edition.is_stale_for(&["a".to_string(), "b".to_string()]));
    assert!(edition.is_stale_for(&["b".to_string(), "a".to_string()]));
}

#[test]
fn the_document_sits_beside_the_sidecar_on_its_stem() {
    assert_eq!(
        RegionDoc::path_beside(Path::new("/w/taiyaku.json")),
        PathBuf::from("/w/taiyaku.regions.json")
    );
    assert_eq!(
        RegionDoc::path_beside(Path::new("gotoubun.json")),
        PathBuf::from("gotoubun.regions.json")
    );
    // A stem containing a dot keeps it. `set_extension` twice does not:
    // the first call strips to `go.between`, whose own extension is then
    // `between`, which the second call replaces.
    assert_eq!(
        RegionDoc::path_beside(Path::new("/w/go.between.json")),
        PathBuf::from("/w/go.between.regions.json")
    );
}

#[test]
fn an_unreadable_document_says_what_it_is() {
    let err = RegionDoc::from_json("{").expect_err("refuses");
    assert!(err.contains("regions"), "names the document: {err}");
}
