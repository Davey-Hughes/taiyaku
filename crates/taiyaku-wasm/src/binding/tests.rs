use super::*;

#[test]
fn a_sidecar_parses_and_lists_its_editions() {
    let b = Binding::from_json(FIXTURE).expect("parses");
    let mut names = b.editions();
    names.sort();
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}

/// Refusal names the problem. A binding that answered `None` for malformed
/// input would make "no such work" and "this file is broken" the same
/// answer, and the browser is the worst place to tell them apart.
#[test]
fn malformed_json_is_refused_with_a_message() {
    let refused = Binding::from_json("{ not json").expect_err("must not invent a document");
    assert!(
        refused.contains("not a readable taiyaku.json"),
        "the refusal says what kind of file it failed to read: {refused}"
    );
    assert!(
        refused.contains("line") && refused.contains("column"),
        "and where — serde locates the fault, and a binding that dropped \
             that would leave a browser with nothing to act on: {refused}"
    );
}

/// Slot count, not page count: `b` has 8 pages of which one is a joined
/// spread, so it presents 9 slots. A reader sizing a filmstrip on pages
/// would come up one short on every spread.
#[test]
fn slot_count_counts_slots_rather_than_pages() {
    let b = Binding::from_json(FIXTURE).expect("parses");
    assert_eq!(b.slot_count("a"), Some(8));
    assert_eq!(b.slot_count("b"), Some(9));
    assert_eq!(b.slot_count("nosuch"), None);
}

/// The half travels in and out, and `via` says how the answer was found.
#[test]
fn a_slot_resolves_with_its_half_and_reports_the_path() {
    let b = Binding::from_json(FIXTURE).expect("parses");
    let got = b
        .resolve("a", "b", 3, None)
        .expect("a pairs with b directly");
    assert_eq!(got.page, 2);
    assert_eq!(got.half, Some(Half::Right), "run `1-3` ends on b's `2R`");
    assert_eq!(got.via, "direct");
}

/// Every slot of every ordered pair, through the binding and through the
/// core, must agree.
///
/// **What this catches, and what it does not.** `Binding::resolve` calls
/// `route` and `Route::resolve_slot` and copies the answer into
/// `Resolved`, so both sides of this comparison reach the same core
/// functions: a bug in spread arithmetic or run composition would be wrong
/// identically on both sides and this would stay green. What it does catch
/// is drift in the binding's own plumbing — a transposed `from`/`to`, a
/// dropped half, a wrong field copied into `Resolved`. It is not a value
/// compared with itself: the two sides parse the document separately and
/// build their own `Route`.
///
/// Exhaustive rather than sampled because the bug it *can* catch is a
/// single slot answering differently, which sampling finds only by luck.
#[test]
fn the_binding_agrees_with_the_core_on_every_slot() {
    let b = Binding::from_json(FIXTURE).expect("parses");
    let s = taiyaku_core::Sidecar::from_json(FIXTURE).expect("parses");
    let reading = s.work.reading;

    let mut compared = 0;
    for from in b.editions() {
        for to in b.editions() {
            let Some(r) = taiyaku_core::route(&s, &from, &to) else {
                continue;
            };
            for slot in s.editions[&from].slots(reading) {
                let native = r.resolve_slot(slot);
                let bound = b.resolve(&from, &to, slot.page, slot.half);
                assert_eq!(
                    native.map(|n| (n.page, n.half)),
                    bound.map(|x| (x.page, x.half)),
                    "{from} -> {to}, slot {slot:?} disagrees"
                );
                compared += 1;
            }
        }
    }
    // 8 slots in `a` and 9 in `b` — `b`'s page 2 is a spread — across four
    // ordered pairs, all of which have a route in a two-edition star.
    // Pinned rather than merely non-zero: an edit that narrowed the loop to
    // a subset of pairs would still compare *something* and stay green
    // while quietly dropping the "every ordered pair" claim above.
    assert_eq!(
        compared, 34,
        "the fixture's geometry changed, or the loop stopped covering it"
    );
}

/// The paths a two-edition star can produce, named the way JS will branch
/// on them. `Via::Hub` needs three editions, so it is not reachable from
/// this fixture and is not asserted here.
///
/// **Slot 1, not slot 0, for the `a -> b` case.** The fixture's first run
/// starts at `a`'s slot 1, so slot 0 is unpaired and `a -> b` answers
/// `None` for it — `via` is a property of the path, but this call can only
/// report one when the slot actually lands somewhere.
#[test]
fn each_path_reports_its_own_word() {
    let b = Binding::from_json(FIXTURE).expect("parses");
    assert_eq!(
        b.resolve("a", "a", 0, None).expect("identity").via,
        "identity"
    );
    assert_eq!(b.resolve("a", "b", 1, None).expect("stored").via, "direct");
    assert_eq!(
        b.resolve("b", "a", 0, None).expect("reversed").via,
        "inverted"
    );
}

/// The fourth `via` word, which two editions cannot reach. `b` and `c` have
/// no stored edge, so the answer composes through the hub — and a reader is
/// entitled to know that, because a composed correspondence is weaker than
/// a stored one.
#[test]
fn a_spoke_to_spoke_lookup_reports_the_hub() {
    let b = Binding::from_json(STAR_FIXTURE).expect("parses");
    let got = b.resolve("b", "c", 0, None).expect("composes through `a`");
    assert_eq!(got.via, "hub");
    assert_eq!(
        got.page, 0,
        "the star's runs are identity, so the page holds"
    );
}

/// `b`'s page 2 is a joined spread, so its slot sequence is
/// 0, 1, 2R, 2L, 3, 4, 5, 6, 7 under `rtl` — nine slots for eight pages.
/// Index 2 and index 3 are the two halves of one page, which is the whole
/// reason a flat index cannot be a page number.
#[test]
fn slot_at_walks_a_spread_as_two_slots() {
    let b = Binding::from_json(FIXTURE).unwrap();
    assert_eq!(
        b.slot_at("b", 1),
        Some(Slot {
            page: 1,
            half: None
        })
    );
    assert_eq!(
        b.slot_at("b", 2),
        Some(Slot {
            page: 2,
            half: Some(Half::Right)
        })
    );
    assert_eq!(
        b.slot_at("b", 3),
        Some(Slot {
            page: 2,
            half: Some(Half::Left)
        })
    );
    assert_eq!(
        b.slot_at("b", 4),
        Some(Slot {
            page: 3,
            half: None
        })
    );
}

#[test]
fn slot_at_refuses_an_index_past_the_end_and_an_unknown_edition() {
    let b = Binding::from_json(FIXTURE).unwrap();
    assert_eq!(b.slot_count("b"), Some(9));
    assert_eq!(b.slot_at("b", 9), None);
    assert_eq!(b.slot_at("nope", 0), None);
}

/// The two are inverses over every slot of every edition. This is the
/// property the reader depends on: a swap converts index → pair → resolve →
/// pair → index, and a mismatch anywhere silently moves the reader.
#[test]
fn index_of_inverts_slot_at_over_every_slot() {
    let b = Binding::from_json(FIXTURE).unwrap();
    for edition in ["a", "b"] {
        let count = b.slot_count(edition).unwrap();
        for index in 0..count {
            let slot = b.slot_at(edition, index).unwrap();
            assert_eq!(
                b.index_of(edition, slot),
                Some(index),
                "{edition} slot {index}"
            );
        }
    }
}

/// The binding takes a flat index, not a `Slot` — `chapter_at` converts
/// through `slot_at` first, the same as `resolve` converts through
/// `route`.
#[test]
fn chapter_at_crosses_the_boundary_as_an_index_and_a_label() {
    let b = Binding::from_json(CHAPTERED_FIXTURE).unwrap();
    let at = b.chapter_at("volume", 0).unwrap();
    assert_eq!((at.member, at.of), (0, 3));
    assert_eq!(at.path, "c001.cbz");
    assert_eq!(b.chapter_at("volume", 999), None);
    assert_eq!(b.chapter_at("nope", 0), None);
}

#[test]
fn index_of_refuses_a_slot_the_edition_does_not_present() {
    let b = Binding::from_json(FIXTURE).unwrap();
    // `a` has no spreads, so it presents no halved slot at all.
    assert_eq!(
        b.index_of(
            "a",
            Slot {
                page: 0,
                half: Some(Half::Right)
            }
        ),
        None
    );
    assert_eq!(
        b.index_of(
            "b",
            Slot {
                page: 99,
                half: None
            }
        ),
        None
    );
}

/// O14's own number, reproduced. `route.rs` says a reader paging a
/// 195-page volume would pay for a route rebuild 195 times without a
/// memo; this resolves every slot of `raw-mono` against `en-mono` — the
/// real 195-slot edition — and checks that `route_builds` stops counting
/// after the first. Before `resolve` routed through `cached_route`, this
/// assertion failed with `route_builds() == 195`; it is the test that
/// pinned the memo, not one added after the fact.
#[test]
fn resolving_every_slot_of_one_pair_builds_the_route_once() {
    let b = Binding::from_json(REAL_FIXTURE).expect("parses");
    let count = b.slot_count("raw-mono").expect("raw-mono exists");
    assert_eq!(count, 195, "the fixture's own claim about its size");
    for slot in 0..count {
        b.resolve("raw-mono", "en-mono", slot, None);
    }
    assert_eq!(
        b.route_builds(),
        1,
        "195 resolves of one pair must build the route once, not 195 times"
    );
}

/// The realistic worst case, not a synthetic one: `raw-mono` slot 1 has
/// no counterpart in `en-mono` at all (the fixture's run `"a": "1"` with
/// no `b` side) — the same gap `docs/status.md`'s O14 entry cites. Its
/// neighbours, slots 0 and 2, do resolve, so `web/src/nearest.ts`'s
/// `nearestPaired` finds both in one step each way: two resolves for the
/// scan. Before `App.tsx`'s gap branch was changed to reuse what the scan
/// already found, it resolved both offers a second time through
/// `resolveOffered` — two more calls, same pair. All four are reproduced
/// here directly against `Binding`, since `nearest.ts`'s own tests use a
/// fake resolver and cannot observe a Rust-side rebuild.
#[test]
fn a_gap_scan_and_its_former_second_pass_build_the_route_once() {
    let b = Binding::from_json(REAL_FIXTURE).expect("parses");

    // The scan: nearestPaired checking slot 0 (before) and slot 2 (after)
    // from the gap at slot 1.
    assert!(b.resolve("raw-mono", "en-mono", 0, None).is_some());
    assert!(b.resolve("raw-mono", "en-mono", 2, None).is_some());
    // The second pass App.tsx no longer makes — kept here to show the
    // memo would have absorbed it even if it still did.
    assert!(b.resolve("raw-mono", "en-mono", 0, None).is_some());
    assert!(b.resolve("raw-mono", "en-mono", 2, None).is_some());

    assert_eq!(
        b.route_builds(),
        1,
        "four resolves of one pair, from a scan and its former repeat, \
             must build the route once"
    );
}

/// An unknown edition answers empty, not an error: the reader asks for
/// every edition it draws, and a missing list is not a fault.
#[test]
fn credits_crosses_the_boundary_as_page_indices() {
    let binding = Binding::from_json(SIDECAR_WITH_CREDITS).expect("parses");
    assert_eq!(binding.credits("fan"), vec![3, 4]);
    assert!(
        binding.credits("no-such-edition").is_empty(),
        "an unknown edition answers empty, not an error: the reader asks for \
             every edition it draws and a missing list is not a fault"
    );
}
