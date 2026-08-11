//! Which path a lookup walks between two editions, and the answer it gives.
//!
//! `sidecar.rs` owns the document; this owns what can be asked of one. The two
//! were the same file until composition made routing a subject of its own.

use crate::{Run, Sidecar, Slot, counterpart};

/// Which path a lookup walked.
///
/// Reported rather than hidden because a composed answer is weaker than a
/// direct one — measured at about 1.7% of pages lost to hub gaps — and a reader
/// that cannot tell them apart has been told less than the document knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    /// The same edition on both sides.
    Identity,
    /// An alignment stored in the direction asked for.
    Direct,
    /// An alignment stored the other way round, reversed.
    Inverted,
    /// Two alignments chained through the named hub.
    Hub(String),
}

/// Reverse an alignment: B→A from A→B.
///
/// Paired runs swap sides. Gaps drop, and that is not a loss: a gap records
/// pages of A with no counterpart in B, which says nothing whatever about B.
///
/// This is what lets a sidecar store N-1 edges rather than 2(N-1). It rests on
/// a measurement rather than an assumption — the solver is symmetric on the
/// pairing relation, checked across five pairs in two series including the
/// widest length mismatch in the corpus (195 slots against 240), where every
/// paired run is identical to its transpose. `calibrate` is asymmetric on
/// paper, since `signal` is a median of per-A-page maxima, and it does not
/// reach the DP's output on any material measured. See `notes.md`.
#[must_use]
pub fn invert(runs: &[Run]) -> Vec<Run> {
    let mut inverted: Vec<Run> = runs
        .iter()
        .filter_map(|run| run.b.map(|b| Run::paired(b, run.a)))
        .collect();
    // The solver is monotonic, so swapping a sorted list yields a sorted one —
    // but a hand-edited document need not be monotonic and nothing in the
    // format enforces it. Unsorted, `compress` emits one run per page, which
    // would make a sound composed alignment read as confetti.
    inverted.sort_by_key(|run| run.a.start);
    inverted
}

/// The correspondence between two editions, in slot indices, and how it was
/// found.
///
/// An object rather than a bare function because composing is not free: it
/// reads two alignments, builds two slot tables and chains them. At one page
/// that is nothing; a reader paging through a 195-page volume would pay it 195
/// times. Building it once is the difference between a lookup and a scan.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub via: Via,
    runs: Vec<Run>,
    from_slots: Vec<Slot>,
    to_slots: Vec<Slot>,
}

impl Route {
    /// Where a slot of the `from` edition lands.
    ///
    /// A reader stopped on the left half of a joined spread has to swap to the
    /// counterpart of *that half*. Resolving its page instead names the first
    /// half, which is a different slot and, where a run ends mid-spread, a
    /// different answer — so a reader doing that moves a page without being
    /// asked to. `notes.md`, "swapping must not round-trip through the
    /// alignment", is the same failure reached by a different route.
    #[must_use]
    pub fn resolve_slot(&self, slot: Slot) -> Option<Slot> {
        let here =
            crate::sidecar::slot_position(crate::sidecar::slot_index(&self.from_slots, slot)?);
        let there = counterpart(&self.runs, here)?;
        self.to_slots.get(there as usize).copied()
    }

    /// Where a page of the `from` edition lands.
    ///
    /// A page reference with no half names the first slot of that page, which
    /// is what makes `47` mean the same thing whether page 47 is whole or a
    /// spread. The answer carries a half because that is what a reader has to
    /// display — possibly one side of a joined spread.
    #[must_use]
    pub fn resolve(&self, page: u32) -> Option<Slot> {
        self.resolve_slot(Slot { page, half: None })
    }

    /// The runs this route walks, in slot indices.
    ///
    /// Exposed because a caller reporting how far the answer can be trusted has
    /// to judge what will actually be walked, which for a composed route is
    /// neither of the stored run lists.
    #[must_use]
    pub fn runs(&self) -> &[Run] {
        &self.runs
    }
}

/// Find the path from one edition to another, or `None` when there is none.
///
/// Order: identity, a stored edge in the direction asked for, the same edge
/// reversed, then two edges chained through the hub.
///
/// **A stored edge is the whole answer, including when it is there and cannot
/// be read.** Falling through to the hub in that case would answer a different
/// question than the one asked — not "where does this edge send the page" but
/// "where does some other path send it". `Sidecar::trust` makes the same
/// choice for the same reason.
#[must_use]
pub fn route(sidecar: &Sidecar, from: &str, to: &str) -> Option<Route> {
    // Existence is checked before identity: `route("nosuch", "nosuch")` must be
    // `None` rather than a confident identity over an edition that is not here.
    let reading = sidecar.work.reading;
    let from_slots = sidecar.editions.get(from)?.slots(reading);
    let to_slots = sidecar.editions.get(to)?.slots(reading);

    if from == to {
        // An edition corresponds to itself as a property of the edition, not of
        // a run list that need not exist — the same rule `trust` applies.
        let runs = match from_slots.len() {
            0 => Vec::new(),
            n => {
                // n > 0 in this arm (0 is handled above), so slot_position(n) - 1
                // cannot underflow.
                let last = crate::sidecar::slot_position(n) - 1;
                vec![Run::paired(
                    crate::PageRange::new(0, last),
                    crate::PageRange::new(0, last),
                )]
            }
        };
        return Some(Route {
            via: Via::Identity,
            runs,
            from_slots,
            to_slots,
        });
    }

    if let Some(edge) = sidecar.edge_between(from, to) {
        let stored = sidecar.runs_of(edge).ok()?;
        let (runs, via) = if edge.from == from {
            (stored, Via::Direct)
        } else {
            (invert(&stored), Via::Inverted)
        };
        return Some(Route {
            via,
            runs,
            from_slots,
            to_slots,
        });
    }

    // No stored edge, so chain two through the hub. This is the topology
    // `sidecar.md` specifies and `topology` measured: 0.00% disagreement across
    // 4,815 page comparisons, about 1.7% of pages lost to hub gaps.
    if !sidecar.editions.contains_key(&sidecar.hub) {
        return None;
    }
    let first = leg(sidecar, from, &sidecar.hub)?;
    let second = leg(sidecar, &sidecar.hub, to)?;
    Some(Route {
        via: Via::Hub(sidecar.hub.clone()),
        runs: crate::compose(&first, &second),
        from_slots,
        to_slots,
    })
}

/// One hop of a composition: a stored edge, oriented the way the caller needs.
///
/// A star stores `hub -> spoke` edges, so the first leg of every spoke-to-spoke
/// lookup is an inversion. That costs nothing the solver would not have
/// produced anyway — see `invert`.
fn leg(sidecar: &Sidecar, from: &str, to: &str) -> Option<Vec<Run>> {
    let edge = sidecar.edge_between(from, to)?;
    let stored = sidecar.runs_of(edge).ok()?;
    Some(if edge.from == from {
        stored
    } else {
        invert(&stored)
    })
}

/// Resolve a page of one edition to its counterpart in another, from a stored
/// sidecar alone.
///
/// This is the whole reader API in one call. A caller resolving more than one
/// page should hold a `Route` instead, which builds the path once.
#[must_use]
pub fn resolve(sidecar: &Sidecar, from: &str, to: &str, page: u32) -> Option<Slot> {
    route(sidecar, from, to)?.resolve(page)
}

#[cfg(test)]
mod tests {
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

    /// Three editions in a star: hub `a`, spokes `b` and `c`, and only the two
    /// hub edges stored. `b -> c` is the question a reader with three editions
    /// asks, and until now the document could not answer it.
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

    /// The cost hub-and-spoke is measured to carry, at the sidecar layer: about
    /// 1.7% of pages. A page that exists in a spoke but not in the hub has
    /// nowhere to land, so the chain ends there — and the reader sees "no
    /// counterpart" rather than the wrong page, which is the safe direction.
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

    /// The behaviour change this work turns on. `resolve` previously answered
    /// only in the stored direction, so a reader holding a two-edition document
    /// could go raw→en and not back. The relation is symmetric — measured, not
    /// assumed — so the document already carried this answer and the library
    /// declined to give it.
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

    /// An edition corresponds to itself, and `trust` has always said so. Before
    /// this, `resolve` disagreed unless a self-alignment happened to be stored.
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
}
