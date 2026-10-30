use super::*;
use crate::{Half, PageRange};

/// A slot with no half names the *first* half of its page in reading
/// order. That is the rule which makes `resolve` correct as a call to
/// `resolve_slot`, and it is a real choice rather than a formality: under
/// `rtl` the right half is the earlier page, so en-mono's spread at page 4
/// must answer for `4R`, and `4R` and `4L` do not agree here.
#[test]
fn a_slot_with_no_half_names_the_first_half_in_reading_order() {
    let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");
    let r = route(&s, "en-mono", "raw-mono").expect("a route exists");

    let whole = r.resolve_slot(Slot {
        page: 4,
        half: None,
    });
    let right = r.resolve_slot(Slot {
        page: 4,
        half: Some(Half::Right),
    });
    let left = r.resolve_slot(Slot {
        page: 4,
        half: Some(Half::Left),
    });

    assert_eq!(
        whole, right,
        "rtl makes the right half the earlier page, so it is the one a \
             bare page reference names"
    );
    assert_ne!(
        whole, left,
        "and the halves disagree here, which is what stops this asserting \
             a formality"
    );
}

/// The reason `resolve_slot` exists. `en-mono` page 4 is a joined spread,
/// and the stored run `"a": "2-5", "b": "1-4R"` ends on its *right* half —
/// so read backwards, `4R` has a counterpart and `4L` falls between two
/// runs and has none. A reader stopped on the left half that swapped to
/// "page 4" would be shown the right half's counterpart and silently move.
#[test]
fn the_halves_of_a_spread_resolve_separately() {
    let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");
    let r = route(&s, "en-mono", "raw-mono").expect("a route exists");

    assert_eq!(
        r.resolve_slot(Slot {
            page: 4,
            half: Some(Half::Right)
        }),
        Some(Slot {
            page: 5,
            half: None
        }),
        "the right half ends run `1-4R`, so it pairs with raw 5"
    );
    assert_eq!(
        r.resolve_slot(Slot {
            page: 4,
            half: Some(Half::Left)
        }),
        None,
        "the left half sits between two runs and pairs with nothing"
    );
}

/// A slot naming a page the edition does not have is `None`, not a clamp
/// and not a panic — the same choice `Edition::locate` makes.
#[test]
fn a_slot_past_the_end_resolves_to_nothing() {
    let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");
    let r = route(&s, "raw-mono", "en-mono").expect("a route exists");
    let past = s
        .editions
        .get("raw-mono")
        .expect("the fixture's hub")
        .total_pages();
    assert_eq!(
        r.resolve_slot(Slot {
            page: past,
            half: None
        }),
        None
    );
}

/// The reader's whole job, done from the stored document with no archives
/// and no solving. These expectations come from the aligner's own output on
/// Go-Toubun volume 1, so agreement means the file carries what was solved.
#[test]
fn a_page_resolves_through_a_stored_sidecar() {
    let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");

    // Ordinary page inside the long final run. Raw 100 lands on English 94,
    // not 99: five of the seven spreads sit before it, and each absorbs a
    // page. Working that out by eye gets it wrong, which is the point.
    assert_eq!(
        resolve(&s, "raw-mono", "en-mono", 100),
        Some(Slot {
            page: 94,
            half: None
        })
    );
    // Raw page 47 lands on the right half of the spread at English page 45.
    // Getting the slot arithmetic wrong here shows the wrong half.
    assert_eq!(
        resolve(&s, "raw-mono", "en-mono", 47),
        Some(Slot {
            page: 45,
            half: Some(Half::Right)
        })
    );
    assert_eq!(
        resolve(&s, "raw-mono", "en-mono", 48),
        Some(Slot {
            page: 45,
            half: Some(Half::Left)
        })
    );
    // Page 1 is the blank the solver refused to pair.
    assert_eq!(resolve(&s, "raw-mono", "en-mono", 1), None);
    // Beyond the volume.
    assert_eq!(resolve(&s, "raw-mono", "en-mono", 9999), None);
    assert_eq!(
        route(&s, "raw-mono", "en-mono").map(|r| r.via),
        Some(Via::Direct)
    );
}

/// Three editions in a star: hub `a`, spokes `b` and `c`, and only the two hub
/// edges stored. `b -> c` is the question a reader with three editions asks.
const STAR: &str = r#"{
      "taiyaku": 1,
      "work": { "title": "T", "reading": "rtl" },
      "editions": {
        "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
        "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 30 }] },
        "c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] }
      },
      "hub": "a",
      "alignments": [
        { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-29", "b": "0-29" }] },
        { "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] }
      ]
    }"#;

#[test]
fn two_spokes_resolve_through_the_hub() {
    let s = Sidecar::from_json(STAR).expect("parses");

    // b 10 -> a 10 -> c 8. The first leg is the stored a->b edge inverted;
    // the second is stored in the direction needed.
    assert_eq!(
        resolve(&s, "b", "c", 10),
        Some(Slot {
            page: 8,
            half: None
        })
    );
    // And back, which crosses the same two edges the other way round.
    assert_eq!(
        resolve(&s, "c", "b", 8),
        Some(Slot {
            page: 10,
            half: None
        })
    );
    assert_eq!(
        route(&s, "b", "c").map(|r| r.via),
        Some(Via::Hub("a".into()))
    );
    // Composing two constant offsets is one constant offset, so the
    // pairing stays a single run rather than fragmenting into 28. The
    // second run is the gap at the head, for the two pages the hub has no
    // pairing for — see the next test.
    let composed = route(&s, "b", "c").expect("routes");
    assert_eq!(
        composed.runs().iter().filter(|r| r.b.is_some()).count(),
        1,
        "two constant offsets must not fragment: {:?}",
        composed.runs()
    );
    assert_eq!(composed.runs().len(), 2);
}

/// The cost hub-and-spoke carries, at the sidecar layer. A page that exists in
/// a spoke but not in the hub has nowhere to land, so the chain ends there —
/// and the reader sees "no counterpart" rather than the wrong page, which is
/// the safe direction.
#[test]
fn a_page_landing_on_a_hub_gap_has_no_counterpart() {
    let s = Sidecar::from_json(STAR).expect("parses");

    // a has no pairing for pages 0 and 1, so b 0 and b 1 reach the hub and
    // stop. c does have pages there; a direct b->c edge would find them.
    assert_eq!(resolve(&s, "b", "c", 0), None);
    assert_eq!(resolve(&s, "b", "c", 1), None);
    assert_eq!(
        resolve(&s, "b", "c", 2),
        Some(Slot {
            page: 0,
            half: None
        })
    );
}

/// A direct edge layered over the star wins, which is the mitigation
/// `sidecar.md` designs for the pair where composition demonstrably loses.
#[test]
fn a_direct_edge_is_preferred_to_the_hub() {
    let with_edge = STAR.replace(
        r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] }"#,
        r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] },
               { "from": "b", "to": "c", "by": "hand", "runs": [{ "a": "0-29", "b": "0-29" }] }"#,
    );
    let s = Sidecar::from_json(&with_edge).expect("parses");

    assert_eq!(route(&s, "b", "c").map(|r| r.via), Some(Via::Direct));
    // The direct edge reaches the pages the hub gap swallowed.
    assert_eq!(
        resolve(&s, "b", "c", 0),
        Some(Slot {
            page: 0,
            half: None
        })
    );
}

/// An edition nothing aligns to is unreachable, and saying so is the whole
/// point — a partial answer here would be a wrong page.
#[test]
fn a_missing_leg_has_no_route() {
    let lonely = STAR.replace(
        r#""c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] }"#,
        r#""c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] },
               "d": { "label": "D", "members": [{ "path": "d.cbz", "sha256": "03", "pages": 30 }] }"#,
    );
    let s = Sidecar::from_json(&lonely).expect("parses");

    assert!(route(&s, "b", "d").is_none());
    assert!(route(&s, "d", "a").is_none());
}

/// A stored edge that cannot be read is the answer, not a reason to go
/// looking for another path. `trust` makes the same choice: composing here
/// would answer a question the caller did not ask.
#[test]
fn an_unreadable_direct_edge_does_not_fall_through_to_the_hub() {
    let broken = STAR.replace(
        r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] }"#,
        r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] },
               { "from": "b", "to": "c", "by": "hand", "runs": [{ "a": "0-1", "b": "0-9" }] }"#,
    );
    let s = Sidecar::from_json(&broken).expect("parses");

    assert!(route(&s, "b", "c").is_none());
}

/// A stored edge answers in both directions. The relation is symmetric, so the
/// document already carries the reverse answer.
#[test]
fn a_stored_edge_answers_in_both_directions() {
    let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");

    // The exact inverse of the forward case: raw 47 lands on 45R, so
    // English page 45 — whose first slot is 45R — lands back on raw 47.
    assert_eq!(
        resolve(&s, "en-mono", "raw-mono", 45),
        Some(Slot {
            page: 47,
            half: None
        })
    );
    assert_eq!(
        resolve(&s, "en-mono", "raw-mono", 0),
        Some(Slot {
            page: 0,
            half: None
        })
    );
    assert_eq!(
        route(&s, "en-mono", "raw-mono").map(|r| r.via),
        Some(Via::Inverted)
    );
    // Raw page 1 is a gap, so nothing in English reaches it. Inverting must
    // not invent a counterpart for it.
    assert!(
        !(0..187).any(|p| resolve(&s, "en-mono", "raw-mono", p)
            == Some(Slot {
                page: 1,
                half: None
            })),
        "a dropped gap must not become a pairing"
    );
}

/// An edition corresponds to itself, and `trust` says so. `resolve` must agree
/// without a self-alignment stored.
#[test]
fn an_edition_resolves_to_itself() {
    let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");

    assert_eq!(
        resolve(&s, "raw-mono", "raw-mono", 100),
        Some(Slot {
            page: 100,
            half: None
        })
    );
    // A spread page answers with its first slot, exactly as every other
    // lookup does — English page 45 is a spread, and `45` names 45R.
    assert_eq!(
        resolve(&s, "en-mono", "en-mono", 45),
        Some(Slot {
            page: 45,
            half: Some(Half::Right)
        })
    );
    assert_eq!(
        route(&s, "raw-mono", "raw-mono").map(|r| r.via),
        Some(Via::Identity)
    );
    // Identity is a property of the edition, but the edition still has to
    // exist.
    assert!(route(&s, "nosuch", "nosuch").is_none());
}

/// The ordinary case: a paired run reverses, and reversing twice is the
/// identity. If this were lossy the star would need both directions stored.
#[test]
fn inverting_swaps_the_sides_and_round_trips() {
    let runs = vec![
        Run::paired(PageRange::new(0, 3), PageRange::new(10, 13)),
        Run::paired(PageRange::new(4, 9), PageRange::new(20, 25)),
    ];

    let back = invert(&runs);

    assert_eq!(
        back,
        vec![
            Run::paired(PageRange::new(10, 13), PageRange::new(0, 3)),
            Run::paired(PageRange::new(20, 25), PageRange::new(4, 9)),
        ]
    );
    assert_eq!(invert(&back), runs, "inverting twice must change nothing");
}

/// A gap is one-sided by construction. Carrying it across would claim B has
/// pages the alignment never mentioned.
#[test]
fn inverting_drops_gaps() {
    let runs = vec![
        Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
        Run::gap(PageRange::new(1, 1)),
        Run::paired(PageRange::new(2, 2), PageRange::new(1, 1)),
    ];

    let back = invert(&runs);

    assert_eq!(
        back,
        vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::paired(PageRange::new(1, 1), PageRange::new(2, 2)),
        ]
    );
}

/// A hand-edited file can list runs in any order. Left unsorted, the
/// composed output compresses into one run per page.
#[test]
fn inverting_sorts_by_the_new_source_side() {
    let runs = vec![
        Run::paired(PageRange::new(0, 1), PageRange::new(8, 9)),
        Run::paired(PageRange::new(2, 3), PageRange::new(0, 1)),
    ];

    let back = invert(&runs);

    assert_eq!(back[0].a, PageRange::new(0, 1));
    assert_eq!(back[1].a, PageRange::new(8, 9));
}
