use super::*;

/// The shell converts and does not decide. This pins the one piece of
/// judgement it does carry — how a half crosses the language boundary —
/// because a silent disagreement about `"L"` would show a reader the wrong
/// half of every spread.
#[test]
fn a_half_crosses_the_boundary_as_l_or_r() {
    assert_eq!(half_from_str(None), Ok(None));
    assert_eq!(half_from_str(Some("L")), Ok(Some(taiyaku_core::Half::Left)));
    assert_eq!(
        half_from_str(Some("R")),
        Ok(Some(taiyaku_core::Half::Right))
    );
    assert_eq!(half_to_str(Some(taiyaku_core::Half::Left)), Some("L"));
    assert_eq!(half_to_str(Some(taiyaku_core::Half::Right)), Some("R"));
    assert_eq!(half_to_str(None), None);
    assert!(
        half_from_str(Some("middle")).is_err(),
        "an unknown half is refused rather than silently read as a whole page"
    );
}

/// O240: an anchor's half crosses in the wire's spelling, so the web sends
/// what it is handed; a side swapped here would anchor every note made on a
/// spread to its other half.
#[test]
fn an_anchor_half_crosses_the_boundary_as_left_or_right() {
    assert_eq!(half_to_wire(Some(taiyaku_core::Half::Left)), Some("left"));
    assert_eq!(half_to_wire(Some(taiyaku_core::Half::Right)), Some("right"));
    assert_eq!(half_to_wire(None), None);
}

/// The shell forwards four positional arguments and copies three fields. The
/// agreement test in `binding` sits one layer below this and cannot see a
/// transposed `from`/`to` or a wrong field copied into `WasmSlot`.
///
/// `.ok()` rather than `.unwrap()` on the `Result`: formatting a `JsError`
/// off-target would call into JavaScript, and the happy path never
/// constructs one. Clippy's `ok_expect` does not know that, and its
/// suggested `.expect()` is the very call this test declines to make.
#[allow(clippy::ok_expect)]
#[test]
fn the_shell_forwards_its_arguments_and_copies_every_field() {
    let s = WasmSidecar::from_json(crate::binding::FIXTURE)
        .ok()
        .expect("the fixture parses");
    let got = s
        .resolve_slot("a", "b", 3, None)
        .ok()
        .expect("a half of `None` is accepted")
        .expect("slot 3 of `a` lands in `b`");
    assert_eq!(got.page, 2, "run `1-3` ends on b's page 2");
    assert_eq!(got.half, Some("R".to_string()), "and on its right half");
    assert_eq!(got.via, "direct", "a stored edge, walked as stored");
}

/// The three forwarders that carry a value straight across. `binding`'s own
/// tests pin what each answers, not that *this* layer asks the right question
/// with the right argument, which a constant in place of any of the three
/// would otherwise hide.
///
/// `editions` and `credits` are asserted against exact contents rather than
/// a length, and `credits` against a list that is neither empty nor `[0]`
/// nor `[1]`, because those are precisely the constants a stub returns.
#[allow(clippy::ok_expect)]
#[test]
fn the_value_forwarders_ask_the_binding_rather_than_answering_themselves() {
    let s = WasmSidecar::from_json(crate::binding::FIXTURE)
        .ok()
        .expect("the fixture parses");
    assert_eq!(s.editions(), vec!["a".to_string(), "b".to_string()]);
    assert_eq!(s.slot_count("a"), Some(8), "eight pages, no spreads");
    assert_eq!(
        s.slot_count("b"),
        Some(9),
        "eight pages, one of them joined, so nine slots"
    );
    assert_eq!(
        s.slot_count("no-such-edition"),
        None,
        "an unknown edition is absent, not zero"
    );

    let c = WasmSidecar::from_json(crate::binding::SIDECAR_WITH_CREDITS)
        .ok()
        .expect("the fixture parses");
    assert_eq!(c.credits("fan"), vec![3, 4]);
    assert!(
        c.credits("no-such-edition").is_empty(),
        "an unknown edition has no credit pages rather than an error"
    );
}

/// `slotAt` and `indexOf` are inverses across the boundary, and each adds
/// its own conversion on the way — `slotAt` spells the half with
/// `half_to_str`, `indexOf` reads it back with `half_from_str`. Pinning
/// them against each other catches a stub in either direction and a half
/// that crosses as the wrong side, which is the failure that would show a
/// reader the opposite page of every spread.
///
/// Slot 2 of `b` is deliberate: it is the first half of the joined spread,
/// so the assertion is on an index that is neither `0` nor `1`.
#[allow(clippy::ok_expect)]
#[test]
fn slot_at_and_index_of_are_inverses_across_the_boundary() {
    let s = WasmSidecar::from_json(crate::binding::FIXTURE)
        .ok()
        .expect("the fixture parses");

    let at = s.slot_at("b", 2).expect("b presents nine slots");
    assert_eq!(at.page, 2, "the joined spread");
    assert_eq!(
        at.half,
        Some("R".to_string()),
        "right half first, reading rtl"
    );
    assert_eq!(
        s.index_of("b", at.page, at.half.clone())
            .ok()
            .expect("`R` is a half"),
        Some(2),
        "and back to the index it came from"
    );

    let other = s.slot_at("b", 3).expect("the spread's second half");
    assert_eq!((other.page, other.half.as_deref()), (2, Some("L")));

    assert!(s.slot_at("b", 999).is_none(), "past the end of the edition");
    assert!(
        s.slot_at("no-such-edition", 0).is_none(),
        "an unknown edition presents no slots"
    );

    // `index_of`'s refusal of an unknown half is *not* asserted here, and
    // the reason is a hazard rather than an oversight: the error arm builds
    // a `JsError`, and constructing one on a non-wasm target panics with
    // "cannot call wasm-bindgen imported functions on non-wasm targets" —
    // not only formatting it, as the note on
    // `the_shell_forwards_its_arguments_and_copies_every_field` suggests.
    // `a_half_crosses_the_boundary_as_l_or_r` pins that refusal one layer
    // down, in `half_from_str`, which is ordinary Rust and can be asked.
}

/// O240: `memberPage` copies all four fields of the binding's anchor. Slot 8
/// is the second copy's page 2, so `nth` and `page` are distinct and neither
/// is 0, and a field copied into the wrong place shows.
#[allow(clippy::ok_expect)]
#[test]
fn member_page_copies_every_field_across_the_boundary() {
    let s = WasmSidecar::from_json(crate::binding::TWICE_FIXTURE)
        .ok()
        .expect("the fixture parses");

    let second = s.member_page("twice", 8).expect("the last slot");
    assert_eq!(
        (
            second.sha256.as_str(),
            second.nth,
            second.page,
            second.half.as_deref()
        ),
        ("0a", 1, 2, None),
        "the second copy's page 2, a whole page"
    );
    let left = s.member_page("twice", 5).expect("the spread's second slot");
    assert_eq!(
        (
            left.sha256.as_str(),
            left.nth,
            left.page,
            left.half.as_deref()
        ),
        ("0b", 0, 1, Some("left")),
        "the spread's left half, second reading rtl"
    );
    assert!(s.member_page("twice", 9).is_none(), "past the end");
    assert!(
        s.member_page("no-such-edition", 0).is_none(),
        "an unknown edition has no pages"
    );
}

/// The one piece of judgement this shell adds for `chapterAt`: `usize`
/// does not cross the boundary, so `member` and `of` convert to `u32`.
/// `binding`'s own test pins the lookup; this pins the conversion, the
/// same division `a_half_crosses_the_boundary_as_l_or_r` draws for a half.
#[allow(clippy::ok_expect)]
#[test]
fn chapter_at_crosses_the_boundary_as_u32_fields() {
    let s = WasmSidecar::from_json(crate::binding::CHAPTERED_FIXTURE)
        .ok()
        .expect("the fixture parses");
    let at = s
        .chapter_at("volume", 0)
        .expect("volume slot 0 has a chapter");
    assert_eq!(at.member, 0);
    assert_eq!(at.of, 3);
    assert_eq!(at.path, "c001.cbz");
    assert!(
        s.chapter_at("volume", 999).is_none(),
        "past the end of the edition"
    );
}
