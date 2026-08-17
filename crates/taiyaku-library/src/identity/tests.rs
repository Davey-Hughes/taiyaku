use super::*;
use taiyaku_core::Sidecar;

fn sidecar_json(hub_members: &[(&str, &str)], extra_edition: bool) -> String {
    let members: Vec<String> = hub_members
        .iter()
        .map(|(path, sha)| format!(r#"{{"path":"{path}","sha256":"{sha}","pages":10}}"#))
        .collect();
    let extra = if extra_edition {
        r#","en":{"label":"En","members":[{"path":"e.cbz","sha256":"ee","pages":10}]}"#
    } else {
        ""
    };
    format!(
        r#"{{"taiyaku":1,"work":{{"title":"T","reading":"rtl"}},
            "editions":{{"raw":{{"label":"Raw","members":[{}]}}{}}},
            "hub":"raw","alignments":[]}}"#,
        members.join(","),
        extra
    )
}

/// The property the whole design rests on. Adding an edition must NOT change a
/// work's identity — a rescan would otherwise see the work vanish and a new one
/// appear, orphaning its progress and its notes.
#[test]
fn adding_an_edition_does_not_change_the_key() {
    let one = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa")], false)).expect("parses");
    let two = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa")], true)).expect("parses");

    assert_eq!(
        content_key(&one).expect("key"),
        content_key(&two).expect("key"),
        "a second edition must not re-identify the work"
    );
}

#[test]
fn replacing_a_hub_archive_does_change_the_key() {
    let before = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa")], false)).expect("parses");
    let after = Sidecar::from_json(&sidecar_json(&[("a.cbz", "bb")], false)).expect("parses");

    assert_ne!(
        content_key(&before).expect("key"),
        content_key(&after).expect("key"),
        "different bytes are a different work"
    );
}

/// Member order is normative, so two works whose hub holds the same archives in
/// different orders are different works.
#[test]
fn member_order_is_part_of_the_key() {
    let forward =
        Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa"), ("b.cbz", "bb")], false)).expect("p");
    let backward =
        Sidecar::from_json(&sidecar_json(&[("b.cbz", "bb"), ("a.cbz", "aa")], false)).expect("p");

    assert_ne!(
        content_key(&forward).expect("key"),
        content_key(&backward).expect("key")
    );
}

#[test]
fn a_hub_that_names_no_edition_is_an_error_naming_the_hub() {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
                   "editions":{"raw":{"label":"R","members":[]}},
                   "hub":"missing","alignments":[]}"#;
    let sidecar = Sidecar::from_json(json).expect("parses");
    let refused = content_key(&sidecar).expect_err("a hub naming nothing must fail");
    assert!(
        refused.contains("missing"),
        "the message names the hub: {refused}"
    );
}

/// **The separator is what makes the hash a hash of a *list*.**
///
/// Without it, the digest is taken over the members' hex digits run together,
/// so two members `aa` and `bb` hash identically to one member `aabb` — two
/// different works, one identity, and whichever is imported second silently
/// lands on the first's row, taking over its notes and its reading position.
/// The `hasher.update(b"\n")` is the whole of the defence and no test observed
/// it: deleting it survived the suite (whole-branch review 2, I6).
///
/// Hex digests are fixed-width today, which makes this unreachable in
/// practice *today*; `sha256` in a sidecar is a string this crate never
/// validates the length of, and a truncated or prefixed digest is exactly the
/// kind of hand edit the format tolerates elsewhere. The comment at the
/// `update` says it does not rely on fixed width, so neither does this.
#[test]
fn the_member_separator_stops_two_hashes_concatenating_into_one() {
    let split = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa"), ("b.cbz", "bb")], false))
        .expect("parses");
    let joined = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aabb")], false)).expect("parses");

    assert_ne!(
        content_key(&split).expect("key"),
        content_key(&joined).expect("key"),
        "two members must not hash the same as one member whose digest is \
         their two concatenated"
    );
}
