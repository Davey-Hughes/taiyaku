use super::*;
use crate::test_support::z_with_a_spread;

/// A fresh in-memory library, good for nothing but bridging the fixture
/// `Work` this module's tests build — `progress/tests.rs`'s own `library()`,
/// duplicated rather than shared across a private module boundary neither
/// file crosses for anything else.
fn library() -> Library {
    taiyaku_library::Library::open_in_memory().expect("in-memory library")
}

fn note(id: &str, slot: u32, region: Option<Region>) -> Note {
    Note {
        text: None,
        id: id.into(),
        edition: "raw".into(),
        slot,
        region,
        color: None,
        body: "why is this page here".into(),
        created: "2026-08-05T10:00:00Z".into(),
        updated: "2026-08-05T10:00:00Z".into(),
        stale: false,
        at: None,
    }
}

/// The sidecar must not gain a notes field. It is an alignment document,
/// re-solving rewrites it, and prose in it would be noise in every diff.
#[test]
fn notes_are_written_to_the_library_not_into_the_sidecar() {
    let tmp = crate::test_support::Fixture::new("notes-beside");
    let work = crate::test_support::work(&tmp);
    let library = library();
    let sidecar_before = std::fs::read_to_string(&work.path).unwrap();

    store(
        &library,
        &work,
        1,
        &Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: "t".into(),
            notes: vec![note("01J8", 12, None)],
        },
        &work.sidecar(),
        crate::works::Mint::Bridge,
    )
    .unwrap();

    let sidecar_after = std::fs::read_to_string(&work.path).unwrap();
    let stored = load(&library, &work, 1, &work.sidecar()).unwrap();
    assert_eq!(sidecar_before, sidecar_after, "the sidecar must not change");
    assert_eq!(
        stored.map(|n| n.notes.len()),
        Some(1),
        "notes belong in the library"
    );
}

/// A note with a region and one without must both survive the trip, and
/// the one without must not acquire a `null` region on the way — absent
/// means the whole slot, and `"region": null` is a third spelling nothing
/// should have to handle.
#[test]
fn a_note_round_trips_with_and_without_a_region() {
    let tmp = crate::test_support::Fixture::new("notes-round-trip");
    let work = crate::test_support::work(&tmp);
    let library = library();
    let stored = Notes {
        taiyaku_notes: taiyaku_library::NOTES_VERSION,
        work: "t".into(),
        notes: vec![
            note(
                "01J8A",
                3,
                Some(Region {
                    x: 0.12,
                    y: 0.34,
                    w: 0.28,
                    h: 0.09,
                }),
            ),
            note("01J8B", 0, None),
        ],
    };

    store(
        &library,
        &work,
        1,
        &stored,
        &work.sidecar(),
        crate::works::Mint::Bridge,
    )
    .unwrap();
    let read_back = load(&library, &work, 1, &work.sidecar()).unwrap();
    // `notes::to_json`'s own doc comment is the invariant this is standing in
    // for: the region-less note must serialise with the field omitted, not
    // written as `null`, in exactly the bytes a client would be sent.
    let serialised = to_json(read_back.as_ref().expect("just stored")).unwrap();

    let mut stored = stored;
    stored.notes[0].at = Some(page_of("y", 0, 1));
    stored.notes[1].at = Some(page_of("x", 0, 0));
    assert_eq!(read_back, Some(stored), "each with the page its slot shows");
    assert!(
        !serialised.contains("null"),
        "a note with no region must omit the field, not write null: {serialised}"
    );
}

/// A colour is a name a client looks up, not a style it pastes into an
/// attribute — so it is held to the alphabet an edition key is, and the
/// refusal names what was wrong rather than quietly dropping the field.
#[test]
fn a_colour_must_be_a_name_rather_than_a_style() {
    let tmp = crate::test_support::Fixture::new("notes-colour");
    let work = crate::test_support::work(&tmp);
    let coloured = |value: &str| {
        let mut notes = on("raw", 0, None);
        notes.notes[0].color = Some(value.to_string());
        validate(&notes, &work, &empty("t"))
    };

    let plain = coloured("amber");
    let dashed = coloured("deep-sea-blue");
    let css = coloured("red; background: url(x)");
    let shouty = coloured("Amber");
    let empty = coloured("");

    plain.expect("an ordinary name");
    dashed.expect("dashes are how a two-word name is spelled here");
    assert!(matches!(css, Err(Invalid::Refused(_))), "not a style");
    assert!(
        matches!(shouty, Err(Invalid::Refused(_))),
        "one spelling only"
    );
    assert!(
        matches!(empty, Err(Invalid::Refused(_))),
        "absent, not blank"
    );
}

/// **One stale note must not brick a reader's notes.** A re-solve can
/// rename or drop an edition, or shrink one past a slot a note sits on,
/// and a client sends the whole document back on every write — so
/// re-checking an anchor nobody touched refuses every future write on that
/// work, including the write that would have removed the stale note.
#[test]
fn a_stale_anchor_already_stored_does_not_refuse_the_writes_that_follow_it() {
    let tmp = crate::test_support::Fixture::new("notes-stale");
    let work = crate::test_support::work(&tmp);
    let mut stale = on("gone-in-a-re-solve", 3, None);
    stale.notes[0].id = "01STALE".into();

    // As it stands today, refused — this write is what introduces it.
    let introducing = validate(&stale, &work, &empty("t"));

    // Already stored, and now a reader adds an ordinary note beside it.
    let mut both = stale.clone();
    both.notes.push(Note {
        text: None,
        id: "01NEW".into(),
        edition: "raw".into(),
        slot: 1,
        region: None,
        color: None,
        body: "an ordinary note".into(),
        created: "2026-08-05T10:00:00Z".into(),
        updated: "2026-08-05T10:00:00Z".into(),
        stale: false,
        at: None,
    });
    let alongside = validate(&both, &work, &stale);

    // And removing the stale one has to work, which is how a reader gets
    // out of it.
    let removing = validate(&empty("t"), &work, &stale);

    assert!(
        matches!(introducing, Err(Invalid::Missing(_))),
        "a new note on an edition this work does not have is still refused"
    );
    alongside.expect("a stale note already stored must not refuse the notes written beside it");
    removing.expect("and must not refuse its own removal");
}

/// The other half: grandfathering is for anchors that have not moved, not
/// for notes that carry a stale id. Dragging a stale note onto a slot of
/// an edition that still does not exist is this write's doing.
#[test]
fn moving_a_stale_note_is_refused_even_though_it_was_already_stored() {
    let tmp = crate::test_support::Fixture::new("notes-stale-moved");
    let work = crate::test_support::work(&tmp);
    let mut stored = on("gone-in-a-re-solve", 3, None);
    stored.notes[0].id = "01STALE".into();
    let mut moved = stored.clone();
    moved.notes[0].slot = 4;

    let refused = validate(&moved, &work, &stored);
    assert!(
        matches!(refused, Err(Invalid::Missing(_))),
        "moving a note onto a bad anchor is this write's doing"
    );
}

/// Grandfathering covers the anchor and nothing else: a colour cannot be
/// made stale by a re-solve, so a bad one is refused however long the note
/// has been stored.
#[test]
fn a_grandfathered_note_still_has_its_colour_checked() {
    let tmp = crate::test_support::Fixture::new("notes-stale-colour");
    let work = crate::test_support::work(&tmp);
    let stored = on("raw", 0, None);
    let mut recoloured = stored.clone();
    recoloured.notes[0].color = Some("NOT A NAME".into());

    let refused = validate(&recoloured, &work, &stored);
    assert!(matches!(refused, Err(Invalid::Refused(_))));
}

/// Nothing recorded is `None`, not an empty document — whether a row exists
/// is a fact this layer reports honestly. Collapsing the two is the
/// *route*'s decision, made there and explained there.
#[test]
fn a_work_with_no_notes_recorded_loads_as_nothing_recorded() {
    let tmp = crate::test_support::Fixture::new("notes-absent");
    let work = crate::test_support::work(&tmp);
    let loaded = load(&library(), &work, 1, &work.sidecar());
    assert_eq!(loaded.unwrap(), None);
}

/// The bridge from a slug-keyed `Work` to the library's numeric `work.id`:
/// `progress/tests.rs`'s own bridging tests, mirrored here for notes. A work
/// this library has never heard of gets a row minted on its first write, and
/// a read never mints one at all.
#[test]
fn storing_notes_mints_exactly_one_bridge_row_for_a_work_the_library_never_imported() {
    let tmp = crate::test_support::Fixture::new("notes-bridge");
    let work = crate::test_support::work(&tmp);
    let library = library();

    assert_eq!(
        crate::works::find_row(&library.conn(), &work).unwrap(),
        None,
        "nothing has been written yet, so no row exists"
    );

    store(
        &library,
        &work,
        1,
        &on("raw", 0, None),
        &work.sidecar(),
        crate::works::Mint::Bridge,
    )
    .unwrap();
    let first_id = crate::works::find_row(&library.conn(), &work)
        .unwrap()
        .expect("a row now exists");

    store(
        &library,
        &work,
        1,
        &empty("t"),
        &work.sidecar(),
        crate::works::Mint::Bridge,
    )
    .unwrap();
    let second_id = crate::works::find_row(&library.conn(), &work)
        .unwrap()
        .expect("still there");

    assert_eq!(
        first_id, second_id,
        "the second write reuses the first's row"
    );
    assert_eq!(
        load(&library, &work, 1, &work.sidecar()).unwrap(),
        Some(empty("t")),
        "the second write replaced the first note with none"
    );
}

/// A read must never mint a bridge row as a side effect — `works::find_row`'s
/// own doc comment states the rule; this is what proves `notes::load`
/// actually honours it.
#[test]
fn loading_notes_for_an_unregistered_work_creates_no_row() {
    let tmp = crate::test_support::Fixture::new("notes-read-only");
    let work = crate::test_support::work(&tmp);
    let library = library();

    let loaded = load(&library, &work, 1, &work.sidecar()).unwrap();

    let row = crate::works::find_row(&library.conn(), &work).unwrap();
    assert_eq!(loaded, None);
    assert_eq!(row, None, "a GET must not write");
}

fn on(edition: &str, slot: u32, region: Option<Region>) -> Notes {
    Notes {
        taiyaku_notes: taiyaku_library::NOTES_VERSION,
        work: "t".into(),
        notes: vec![Note {
            text: None,
            id: "01J8".into(),
            edition: edition.into(),
            slot,
            region,
            color: None,
            body: "b".into(),
            created: "2026-08-05T10:00:00Z".into(),
            updated: "2026-08-05T10:00:00Z".into(),
            stale: false,
            at: None,
        }],
    }
}

/// A run of text starts at or before its end, an empty run being a caret, is
/// not also a region, and is on a book's text, never on a page.
#[test]
fn a_run_of_text_must_be_a_run_on_a_book_and_not_a_region_too() {
    let tmp = crate::test_support::Fixture::new("notes-text");
    let work = crate::test_support::work(&tmp);
    let mut book = (*work.sidecar()).clone();
    book.editions.get_mut("raw").unwrap().members.truncate(1);
    let books = std::collections::HashSet::from(["raw".to_string()]);
    let marking = |start, end, region| {
        let mut doc = on("raw", 0, region);
        doc.notes[0].text = Some(taiyaku_library::TextRange { start, end });
        validate_against(&doc, "t", &book, &empty("t"), &books)
    };
    let mut on_a_page = on("raw", 0, None);
    on_a_page.notes[0].text = Some(taiyaku_library::TextRange { start: 3, end: 9 });
    assert!(
        matches!(
            validate(&on_a_page, &work, &empty("t")),
            Err(Invalid::Refused(why)) if why.contains("is not a run")
        ),
        "a run on a comic's page"
    );
    let region = Region {
        x: 0.1,
        y: 0.1,
        w: 0.5,
        h: 0.5,
    };
    assert!(marking(3, 9, None).is_ok());
    assert!(marking(4, 4, None).is_ok());
    for refused in [marking(9, 3, None), marking(3, 9, Some(region))] {
        assert!(
            matches!(&refused, Err(Invalid::Refused(why)) if why.contains("is not a run")),
            "{refused:?}"
        );
    }
}

/// Every refusal, and the one acceptance that proves the guard is not
/// spelled "refuse everything".
#[test]
fn a_note_must_name_a_slot_of_an_edition_this_work_has() {
    let tmp = crate::test_support::Fixture::new("notes-validate");
    let work = crate::test_support::work(&tmp);
    // The fixture is one edition, `raw`, of four pages and no spreads —
    // four slots, so slot 3 is the last and slot 4 is one past the end.
    let accepted = validate(&on("raw", 3, None), &work, &empty("t"));
    let unknown_edition = validate(&on("nope", 0, None), &work, &empty("t"));
    let past_the_end = validate(&on("raw", 4, None), &work, &empty("t"));

    accepted.expect("the last slot of a real edition is a real anchor");
    let message = match unknown_edition {
        Err(Invalid::Missing(m)) => m,
        other => panic!("an unknown edition is a 404's shape: {other:?}"),
    };
    assert!(message.contains("nope"), "name it: {message}");
    let message = match past_the_end {
        Err(Invalid::Missing(m)) => m,
        other => panic!("a slot past the end is a 404's shape: {other:?}"),
    };
    assert!(
        message.contains('4') && message.contains("slot"),
        "say how many slots there are: {message}"
    );
}

/// Counted in slots, not pages — a joined spread is one page and two
/// slots, so checking against `total_pages()` would refuse a real anchor
/// on the last slot of every spread-joining edition. O3 learned this on
/// progress; a note is anchored the same way.
#[test]
fn the_range_check_counts_slots_not_pages() {
    let tmp = crate::test_support::Fixture::new("notes-spread-slot");
    let built = crate::test_support::work(&tmp);
    // Rewrite the fixture's sidecar so page 0 is a joined spread: 4 pages,
    // 5 slots, and slot 4 must be accepted.
    let json = std::fs::read_to_string(&built.path)
        .unwrap()
        .replace(r#""spreads": []"#, r#""spreads": [0]"#);
    let work = crate::works::Work::new(
        built.id.clone(),
        built.path.clone(),
        taiyaku_core::Sidecar::from_json(&json).unwrap(),
    );

    let last = validate(&on("raw", 4, None), &work, &empty("t"));
    let past = validate(&on("raw", 5, None), &work, &empty("t"));

    last.expect("4 pages with page 0 joined is 5 slots, and slot 4 is real");
    assert!(past.is_err(), "slot 5 is past the end even so");
}

/// A region outside the slot would draw a marker off the page. A
/// zero-sized one is not nothing — it is a point, the same degenerate case
/// an absent region already is for the whole slot.
#[test]
fn a_region_must_lie_within_the_slot_and_have_a_size() {
    let tmp = crate::test_support::Fixture::new("notes-region");
    let work = crate::test_support::work(&tmp);
    let ok = Region {
        x: 0.1,
        y: 0.1,
        w: 0.5,
        h: 0.5,
    };
    let full = Region {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };
    let over = Region {
        x: 0.8,
        y: 0.1,
        w: 0.5,
        h: 0.2,
    };
    let point = Region {
        x: 0.1,
        y: 0.1,
        w: 0.0,
        h: 0.0,
    };
    let negative = Region {
        x: -0.1,
        y: 0.1,
        w: 0.2,
        h: 0.2,
    };

    let results = [
        validate(&on("raw", 0, Some(ok)), &work, &empty("t")),
        validate(&on("raw", 0, Some(full)), &work, &empty("t")),
        validate(&on("raw", 0, Some(over)), &work, &empty("t")),
        validate(&on("raw", 0, Some(point)), &work, &empty("t")),
        validate(&on("raw", 0, Some(negative)), &work, &empty("t")),
    ];

    results[0].as_ref().expect("an ordinary region");
    results[1]
        .as_ref()
        .expect("the whole slot, stated explicitly");
    assert!(
        matches!(results[2], Err(Invalid::Refused(_))),
        "runs off the edge"
    );
    results[3]
        .as_ref()
        .expect("zero-sized is a point — a reader marking a spot, not an area");
    assert!(
        matches!(results[4], Err(Invalid::Refused(_))),
        "negative origin"
    );
}

/// Two notes sharing an id makes every later edit ambiguous — which of the
/// two did the reader mean? — so it is refused at the boundary rather than
/// discovered by a client that edits the wrong one.
#[test]
fn two_notes_cannot_share_an_id() {
    let tmp = crate::test_support::Fixture::new("notes-dupe-id");
    let work = crate::test_support::work(&tmp);
    let mut notes = on("raw", 0, None);
    notes.notes.push(notes.notes[0].clone());

    let refused = validate(&notes, &work, &empty("t"));

    let message = match refused {
        Err(Invalid::Refused(m)) => m,
        other => panic!("a duplicate id is malformed, not missing: {other:?}"),
    };
    assert!(message.contains("01J8"), "name the id: {message}");
}

/// The document says which work it belongs to, and a mismatch means a
/// client is about to write one work's notes over another's — the one
/// mistake here that destroys notes rather than refusing them.
#[test]
fn a_document_naming_a_different_work_is_refused() {
    let tmp = crate::test_support::Fixture::new("notes-wrong-work");
    let work = crate::test_support::work(&tmp);
    let mut notes = on("raw", 0, None);
    notes.work = "some-other-work".into();

    let refused = validate(&notes, &work, &empty("t"));

    let message = match refused {
        Err(Invalid::Refused(m)) => m,
        other => panic!("{other:?}"),
    };
    assert!(
        message.contains("some-other-work") && message.contains('t'),
        "name both, or nobody can tell which is wrong: {message}"
    );
}

/// `empty` is what the route hands a client that has never written a note,
/// and it must be a document about *this* work — a client writes it
/// straight back, and `validate` refuses a document naming another work.
#[test]
fn an_empty_document_names_the_work_it_is_for() {
    assert_eq!(
        empty("go-toubun-v01"),
        Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: "go-toubun-v01".into(),
            notes: Vec::new()
        }
    );
}

/// The same rule on the other axis.
/// `a_region_must_lie_within_the_slot_and_have_a_size` overflows in x only
/// (`x: 0.8, w: 0.5`), so it never asks the `y + h` half of the bound. A
/// marker can run off the bottom of a page as easily as off the side.
#[test]
fn a_region_that_runs_off_the_bottom_is_refused_like_one_that_runs_off_the_side() {
    let tmp = crate::test_support::Fixture::new("notes-region-y");
    let work = crate::test_support::work(&tmp);
    let over_y = Region {
        x: 0.1,
        y: 0.8,
        w: 0.2,
        h: 0.5,
    };
    let result = validate(&on("raw", 0, Some(over_y)), &work, &empty("t"));

    assert!(
        matches!(result, Err(Invalid::Refused(_))),
        "y + h is 1.3, which is off the page: {result:?}"
    );
}

/// `sha256`'s copy `nth`, its whole `page`.
fn page_of(sha256: &str, nth: u32, page: u32) -> taiyaku_core::MemberPage {
    taiyaku_core::MemberPage {
        sha256: sha256.into(),
        nth,
        page,
        half: None,
    }
}

/// `raw` of X, Y, X again (one archive standing twice) and H, two pages
/// each, left to right, and the view of an account sent Y and the second X.
fn x_twice() -> (std::sync::Arc<Sidecar>, View) {
    let sidecar = std::sync::Arc::new(
        Sidecar::from_json(
            r#"{
              "taiyaku": 1,
              "work": { "title": "T", "reading": "ltr" },
              "editions": {
                "raw": { "label": "Raw", "members": [
                  { "path": "x1.cbz", "sha256": "x", "pages": 2 },
                  { "path": "y.cbz", "sha256": "y", "pages": 2 },
                  { "path": "x2.cbz", "sha256": "x", "pages": 2 },
                  { "path": "h.cbz", "sha256": "h", "pages": 2 }
                ], "spreads": [] }
              },
              "hub": "raw",
              "alignments": []
            }"#,
        )
        .unwrap(),
    );
    let seen = |sha256: &str, path: &str, visible| taiyaku_library::browse::SeenMember {
        sha256: sha256.into(),
        path: path.into(),
        visible,
    };
    let flags = std::collections::HashMap::from([(
        "raw".to_string(),
        vec![
            seen("x", "x1.cbz", false),
            seen("y", "y.cbz", true),
            seen("x", "x2.cbz", true),
            seen("h", "h.cbz", false),
        ],
    )]);
    let view = View::build(std::sync::Arc::clone(&sidecar), Some(&flags)).expect("raw is sent");
    (sidecar, view)
}

/// O240: on a narrowed edition the copy is counted among the members sent,
/// so the second X, the only one sent, is copy 0 on the wire.
#[test]
fn a_narrowed_editions_anchor_counts_the_members_the_account_is_sent() {
    let (sidecar, view) = x_twice();
    assert_eq!(
        view.slot_from_full("raw", 5),
        Some(FromFull::Visible(3)),
        "the fixture: the second X's page 1 is sent as slot 3"
    );

    assert_eq!(
        view.anchor_sent("raw", 3, Some(&page_of("x", 1, 1)), true),
        Some(page_of("x", 0, 1))
    );
    assert_eq!(
        [
            view.slot_of_anchor("raw", &page_of("x", 0, 1)),
            view.slot_of_anchor("raw", &page_of("x", 1, 1)),
        ],
        [Some(taiyaku_core::SlotOf::Exact(3)), None],
        "the account's document holds X once, its page 1 at slot 3"
    );
    let stored = Notes {
        taiyaku_notes: taiyaku_library::NOTES_VERSION,
        work: "t".into(),
        notes: vec![Note {
            at: Some(page_of("x", 1, 1)),
            ..note("n", 5, None)
        }],
    };
    assert_eq!(
        as_seen(&stored, &view).notes,
        [Note {
            slot: 3,
            at: Some(page_of("x", 0, 1)),
            ..note("n", 5, None)
        }]
    );

    assert_eq!(
        view.anchor_sent("raw", 3, Some(&page_of("h", 0, 9)), false),
        None,
        "H is not sent, so neither is its digest"
    );
    assert_eq!(
        view.anchor_sent("raw", 3, Some(&page_of("x", 0, 9)), false),
        Some(page_of("x", 0, 9)),
        "X's digest is a sent member's too"
    );

    let every = View::build(sidecar, None).expect("Access::Every sees the work");
    for resolves in [true, false] {
        for stored in [page_of("x", 1, 1), page_of("h", 0, 9)] {
            assert_eq!(
                every.anchor_sent("raw", 5, Some(&stored), resolves),
                Some(stored.clone()),
                "Access::Every, resolving {resolves}"
            );
        }
    }
}

/// A page whose half the layout no longer holds is found as the page alone,
/// not as nothing, so a write can tell it from a page that is gone. Read
/// right to left, page 1's first slot is its right half.
#[test]
fn a_half_the_layout_no_longer_holds_is_found_on_its_page_only() {
    let view = z_with_a_spread();
    let at = |page, half| taiyaku_core::MemberPage {
        half,
        ..page_of("z", 0, page)
    };
    let left = Some(taiyaku_core::Half::Left);

    assert_eq!(
        view.slot_of_anchor("raw", &at(1, left)),
        Some(taiyaku_core::SlotOf::Exact(2)),
        "page 1's left half"
    );
    assert_eq!(
        view.slot_of_anchor("raw", &at(0, left)),
        Some(taiyaku_core::SlotOf::PageOnly(0)),
        "page 0 is not a spread"
    );
    assert_eq!(
        view.slot_of_anchor("raw", &at(1, None)),
        Some(taiyaku_core::SlotOf::PageOnly(1)),
        "page 1 is"
    );
}

/// Spec gap 8 over the whole work: a page not found is sent as stored unless
/// its digest is a member's the account is not sent, in any edition, and no
/// member's it is sent. `raw` is sent X, not H; `en` (E) not at all; `fr`
/// whole, with H filed under it too. A row naming no page is sent none.
#[test]
fn a_digest_is_sent_only_where_a_member_sent_carries_it() {
    let sidecar = std::sync::Arc::new(
        Sidecar::from_json(
            r#"{
              "taiyaku": 1,
              "work": { "title": "T", "reading": "ltr" },
              "editions": {
                "raw": { "label": "Raw", "members": [
                  { "path": "x.cbz", "sha256": "x", "pages": 2 },
                  { "path": "h.cbz", "sha256": "h", "pages": 2 }
                ], "spreads": [] },
                "en": { "label": "En", "members": [
                  { "path": "e.cbz", "sha256": "e", "pages": 2 }
                ], "spreads": [] },
                "fr": { "label": "Fr", "members": [
                  { "path": "f.cbz", "sha256": "f", "pages": 2 },
                  { "path": "fr-h.cbz", "sha256": "h", "pages": 2 }
                ], "spreads": [] }
              },
              "hub": "raw",
              "alignments": []
            }"#,
        )
        .unwrap(),
    );
    let seen = |edition: &str, visible: &[bool]| {
        let members = &sidecar.editions[edition].members;
        let flags = members.iter().zip(visible);
        let flags = flags.map(|(m, &visible)| taiyaku_library::browse::SeenMember {
            sha256: m.sha256.clone(),
            path: m.path.clone(),
            visible,
        });
        (edition.to_string(), flags.collect())
    };
    let flags = std::collections::HashMap::from([
        seen("raw", &[true, false]),
        seen("en", &[false]),
        seen("fr", &[true, true]),
    ]);
    let view = View::build(std::sync::Arc::clone(&sidecar), Some(&flags)).expect("raw and fr");
    assert!(
        view.narrowed("raw").is_some() && view.sees_whole("fr") && !view.sees_whole("en"),
        "the fixture: raw narrowed, fr whole, en hidden"
    );
    let sent = |edition, stored: &taiyaku_core::MemberPage| {
        view.anchor_sent(edition, 0, Some(stored), false)
    };

    assert_eq!(sent("raw", &page_of("e", 0, 9)), None, "E only in en");
    assert_eq!(sent("fr", &page_of("e", 0, 9)), None, "fr is sent whole");
    assert_eq!(
        sent("raw", &page_of("h", 0, 9)),
        Some(page_of("h", 0, 9)),
        "fr's H is sent"
    );
    assert_eq!(
        sent("fr", &page_of("gone", 0, 0)),
        Some(page_of("gone", 0, 0)),
        "no member's digest"
    );
    assert_eq!(view.anchor_sent("raw", 0, None, true), None, "no page");

    let every = View::build(sidecar, None).expect("Access::Every sees the work");
    assert_eq!(
        every.anchor_sent("fr", 0, Some(&page_of("e", 0, 9)), false),
        Some(page_of("e", 0, 9)),
        "Access::Every is sent every member"
    );
}

/// Pre-deploy decision 6 (O240): a note is never moved to a page it was not
/// written on, so page 0's left half, which the layout does not hold, places
/// no new note. A held note anchored there, its words changed, keeps its
/// stored slot whatever slot it is sent at, as its `at` is the one stored.
#[test]
fn a_note_on_a_half_the_layout_no_longer_holds_is_not_placed() {
    let view = z_with_a_spread();
    let gone = taiyaku_core::MemberPage {
        half: Some(taiyaku_core::Half::Left),
        ..page_of("z", 0, 0)
    };
    assert_eq!(
        view.slot_of_anchor("raw", &gone),
        Some(taiyaku_core::SlotOf::PageOnly(0)),
        "the fixture: page 0 is there and its left half is not"
    );
    let document = |notes: Vec<Note>| Notes {
        taiyaku_notes: taiyaku_library::NOTES_VERSION,
        work: "t".into(),
        notes,
    };

    let mut new = document(vec![Note {
        at: Some(gone.clone()),
        ..note("n", 0, None)
    }]);
    match placed(&mut new, &document(Vec::new()), &view, "t") {
        Err(Invalid::Missing(why)) => assert_eq!(
            why,
            "note `n` names a page that `raw` of work `t` does not have now. Reload the page \
             to see the notes as they are."
        ),
        other => panic!("a new note on a gone half is refused: {other:?}"),
    }

    let held = Note {
        stale: true,
        at: Some(gone),
        ..note("n", 1, None)
    };
    let mut edited = document(vec![Note {
        slot: 0,
        body: "new words".into(),
        ..held.clone()
    }]);
    placed(&mut edited, &document(vec![held]), &view, "t").expect("an edit keeps its anchor");
    assert_eq!(edited.notes[0].slot, 1, "its stored slot");
}

/// The owner's ruling "Server keeps echoes" keeps an echo only where it is
/// the held note as the account is sent it, `at` and `stale` aside. Over
/// [`x_twice`], n is held on the second X's page 1, stored slot 5 and sent
/// as slot 3. Its echo names that page under a digest no member carries, as
/// a tab that read it before a stamp would, so neither equality nor `slot_of`
/// places it: each field below decides alone.
#[test]
fn an_echo_is_kept_only_where_it_is_the_held_note_as_sent() {
    let (_, view) = x_twice();
    let document = |notes: Vec<Note>| Notes {
        taiyaku_notes: taiyaku_library::NOTES_VERSION,
        work: "t".into(),
        notes,
    };
    let held = Note {
        at: Some(page_of("x-stamped", 0, 1)),
        ..note("n", 5, None)
    };
    let echo = Note {
        slot: 3,
        stale: true,
        at: Some(page_of("x-before-the-stamp", 0, 1)),
        ..held.clone()
    };
    let current = document(vec![held]);

    let mut kept = document(vec![echo.clone()]);
    placed(&mut kept, &current, &view, "t").expect("untouched, at the slot it is sent at");
    assert_eq!(kept.notes, std::slice::from_ref(&echo), "kept as it came");

    let changed = [
        (
            "its body",
            Note {
                body: "a body edit".into(),
                ..echo.clone()
            },
        ),
        (
            "its colour",
            Note {
                color: Some("amber".into()),
                ..echo.clone()
            },
        ),
        (
            "its region",
            Note {
                region: Some(Region {
                    x: 0.1,
                    y: 0.1,
                    w: 0.2,
                    h: 0.2,
                }),
                ..echo.clone()
            },
        ),
        (
            "when it was made",
            Note {
                created: "2026-11-01T10:00:00Z".into(),
                ..echo.clone()
            },
        ),
        (
            "when it was changed",
            Note {
                updated: "2026-11-01T10:00:00Z".into(),
                ..echo.clone()
            },
        ),
        ("its slot", Note { slot: 2, ..echo }),
    ];
    for (what, note) in changed {
        let mut sent = document(vec![note]);
        match placed(&mut sent, &current, &view, "t") {
            Err(Invalid::Missing(why)) => assert_eq!(
                why,
                "note `n` names a page that `raw` of work `t` does not have now. Reload the \
                 page to see the notes as they are.",
                "{what}"
            ),
            other => panic!("an echo changing {what} is refused: {other:?}"),
        }
    }
}
