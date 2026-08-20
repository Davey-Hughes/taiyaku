use super::*;

/// A fresh in-memory library, good for nothing but bridging the fixture
/// `Work` this module's tests build — `progress/tests.rs`'s own `library()`,
/// duplicated rather than shared across a private module boundary neither
/// file crosses for anything else.
fn library() -> Library {
    taiyaku_library::Library::open_in_memory().expect("in-memory library")
}

fn note(id: &str, slot: u32, region: Option<Region>) -> Note {
    Note {
        id: id.into(),
        edition: "raw".into(),
        slot,
        region,
        color: None,
        body: "why is this page here".into(),
        created: "2026-08-05T10:00:00Z".into(),
        updated: "2026-08-05T10:00:00Z".into(),
    }
}

/// The sidecar must not gain a notes field. It is an alignment document,
/// re-solving rewrites it, and prose in it would be noise in every diff.
#[test]
fn notes_are_written_to_the_library_not_into_the_sidecar() {
    let work = crate::test_support::work("taiyaku-test-notes-beside");
    let library = library();
    let sidecar_before = std::fs::read_to_string(&work.path).unwrap();

    store(
        &library,
        &work,
        1,
        &Notes {
            work: "t".into(),
            notes: vec![note("01J8", 12, None)],
        },
    )
    .unwrap();

    let sidecar_after = std::fs::read_to_string(&work.path).unwrap();
    let stored = load(&library, &work, 1).unwrap();
    crate::test_support::cleanup("taiyaku-test-notes-beside");
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
    let work = crate::test_support::work("taiyaku-test-notes-round-trip");
    let library = library();
    let stored = Notes {
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

    store(&library, &work, 1, &stored).unwrap();
    let read_back = load(&library, &work, 1).unwrap();
    // The projection, not a second copy re-parsed from disk — there is no
    // disk any more. `notes::to_json`'s own doc comment is the invariant
    // this is standing in for: the region-less note must serialise with the
    // field omitted, not written as `null`, in exactly the bytes a client
    // would be sent.
    let serialised = to_json(read_back.as_ref().expect("just stored")).unwrap();

    crate::test_support::cleanup("taiyaku-test-notes-round-trip");
    assert_eq!(read_back, Some(stored));
    assert!(
        !serialised.contains("null"),
        "a note with no region must omit the field, not write null: {serialised}"
    );
}

// `a_notes_file_with_an_unknown_field_is_refused_rather_than_silently_dropped`
// lived here through Task 12: a notes file hand-edited (or written by a
// later build) with a field this build does not know was refused rather than
// silently dropped, because `Notes`/`Note` both `deny_unknown_fields`. Task
// 13 removed the file its scenario hand-edited — there is no longer a JSON
// document on disk a stray key could be added to. The row-store analogue of
// "malformed foreign data" is a `note` row whose region has only some of its
// four columns set, which cannot come from this crate's own `put_notes` and
// is refused by `taiyaku_library::state::notes` — covered in that crate by
// `state::tests::a_partially_null_region_is_refused_rather_than_guessed_at`,
// not duplicated here.

/// A colour is a name a client looks up, not a style it pastes into an
/// attribute — so it is held to the alphabet an edition key is, and the
/// refusal names what was wrong rather than quietly dropping the field.
#[test]
fn a_colour_must_be_a_name_rather_than_a_style() {
    let work = crate::test_support::work("taiyaku-test-notes-colour");
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
    crate::test_support::cleanup("taiyaku-test-notes-colour");

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
/// Measured before the fix: adding an ordinary note beside a stale one
/// answered 404.
#[test]
fn a_stale_anchor_already_stored_does_not_refuse_the_writes_that_follow_it() {
    let work = crate::test_support::work("taiyaku-test-notes-stale");
    let mut stale = on("gone-in-a-re-solve", 3, None);
    stale.notes[0].id = "01STALE".into();

    // As it stands today, refused — this write is what introduces it.
    let introducing = validate(&stale, &work, &empty("t"));

    // Already stored, and now a reader adds an ordinary note beside it.
    let mut both = stale.clone();
    both.notes.push(Note {
        id: "01NEW".into(),
        edition: "raw".into(),
        slot: 1,
        region: None,
        color: None,
        body: "an ordinary note".into(),
        created: "2026-08-05T10:00:00Z".into(),
        updated: "2026-08-05T10:00:00Z".into(),
    });
    let alongside = validate(&both, &work, &stale);

    // And removing the stale one has to work, which is how a reader gets
    // out of it.
    let removing = validate(&empty("t"), &work, &stale);

    crate::test_support::cleanup("taiyaku-test-notes-stale");
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
    let work = crate::test_support::work("taiyaku-test-notes-stale-moved");
    let mut stored = on("gone-in-a-re-solve", 3, None);
    stored.notes[0].id = "01STALE".into();
    let mut moved = stored.clone();
    moved.notes[0].slot = 4;

    let refused = validate(&moved, &work, &stored);
    crate::test_support::cleanup("taiyaku-test-notes-stale-moved");
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
    let work = crate::test_support::work("taiyaku-test-notes-stale-colour");
    let stored = on("raw", 0, None);
    let mut recoloured = stored.clone();
    recoloured.notes[0].color = Some("NOT A NAME".into());

    let refused = validate(&recoloured, &work, &stored);
    crate::test_support::cleanup("taiyaku-test-notes-stale-colour");
    assert!(matches!(refused, Err(Invalid::Refused(_))));
}

/// Nothing recorded is `None`, not an empty document — whether a row exists
/// is a fact this layer reports honestly. Collapsing the two is the
/// *route*'s decision, made there and explained there.
#[test]
fn a_work_with_no_notes_recorded_loads_as_nothing_recorded() {
    let work = crate::test_support::work("taiyaku-test-notes-absent");
    let loaded = load(&library(), &work, 1);
    crate::test_support::cleanup("taiyaku-test-notes-absent");
    assert_eq!(loaded.unwrap(), None);
}

/// The bridge from a slug-keyed `Work` to the library's numeric `work.id` —
/// the real work of Task 13, and `progress/tests.rs`'s own bridging tests,
/// mirrored here for notes. A work this library has never heard of gets a
/// row minted on its first write, and a read never mints one at all.
#[test]
fn storing_notes_mints_exactly_one_bridge_row_for_a_work_the_library_never_imported() {
    let work = crate::test_support::work("taiyaku-test-notes-bridge");
    let library = library();

    assert_eq!(
        crate::works::find_row(&library.conn(), &work).unwrap(),
        None,
        "nothing has been written yet, so no row exists"
    );

    store(&library, &work, 1, &on("raw", 0, None)).unwrap();
    let first_id = crate::works::find_row(&library.conn(), &work)
        .unwrap()
        .expect("a row now exists");

    store(&library, &work, 1, &empty("t")).unwrap();
    let second_id = crate::works::find_row(&library.conn(), &work)
        .unwrap()
        .expect("still there");

    crate::test_support::cleanup("taiyaku-test-notes-bridge");
    assert_eq!(
        first_id, second_id,
        "the second write reuses the first's row"
    );
    assert_eq!(
        load(&library, &work, 1).unwrap(),
        Some(empty("t")),
        "the second write replaced the first note with none"
    );
}

/// A read must never mint a bridge row as a side effect — `works::find_row`'s
/// own doc comment states the rule; this is what proves `notes::load`
/// actually honours it.
#[test]
fn loading_notes_for_an_unregistered_work_creates_no_row() {
    let work = crate::test_support::work("taiyaku-test-notes-read-only");
    let library = library();

    let loaded = load(&library, &work, 1).unwrap();

    let row = crate::works::find_row(&library.conn(), &work).unwrap();
    crate::test_support::cleanup("taiyaku-test-notes-read-only");
    assert_eq!(loaded, None);
    assert_eq!(row, None, "a GET must not write");
}

fn on(edition: &str, slot: u32, region: Option<Region>) -> Notes {
    Notes {
        work: "t".into(),
        notes: vec![Note {
            id: "01J8".into(),
            edition: edition.into(),
            slot,
            region,
            color: None,
            body: "b".into(),
            created: "2026-08-05T10:00:00Z".into(),
            updated: "2026-08-05T10:00:00Z".into(),
        }],
    }
}

/// Every refusal, and the one acceptance that proves the guard is not
/// spelled "refuse everything".
#[test]
fn a_note_must_name_a_slot_of_an_edition_this_work_has() {
    let work = crate::test_support::work("taiyaku-test-notes-validate");
    // The fixture is one edition, `raw`, of four pages and no spreads —
    // four slots, so slot 3 is the last and slot 4 is one past the end.
    let accepted = validate(&on("raw", 3, None), &work, &empty("t"));
    let unknown_edition = validate(&on("nope", 0, None), &work, &empty("t"));
    let past_the_end = validate(&on("raw", 4, None), &work, &empty("t"));
    crate::test_support::cleanup("taiyaku-test-notes-validate");

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
    let built = crate::test_support::work("taiyaku-test-notes-spread-slot");
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

    crate::test_support::cleanup("taiyaku-test-notes-spread-slot");
    last.expect("4 pages with page 0 joined is 5 slots, and slot 4 is real");
    assert!(past.is_err(), "slot 5 is past the end even so");
}

/// A region outside the slot would draw a marker off the page. A
/// zero-sized one is not nothing — it is a point, the same degenerate case
/// an absent region already is for the whole slot.
#[test]
fn a_region_must_lie_within_the_slot_and_have_a_size() {
    let work = crate::test_support::work("taiyaku-test-notes-region");
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
    crate::test_support::cleanup("taiyaku-test-notes-region");

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
    let work = crate::test_support::work("taiyaku-test-notes-dupe-id");
    let mut notes = on("raw", 0, None);
    notes.notes.push(notes.notes[0].clone());

    let refused = validate(&notes, &work, &empty("t"));
    crate::test_support::cleanup("taiyaku-test-notes-dupe-id");

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
    let work = crate::test_support::work("taiyaku-test-notes-wrong-work");
    let mut notes = on("raw", 0, None);
    notes.work = "some-other-work".into();

    let refused = validate(&notes, &work, &empty("t"));
    crate::test_support::cleanup("taiyaku-test-notes-wrong-work");

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
            work: "go-toubun-v01".into(),
            notes: Vec::new()
        }
    );
}

/// The same rule on the other axis. `a_region_must_lie_within_the_slot_and_have_a_size`
/// overflows in x only — `x: 0.8, w: 0.5` — so the `y + h` half of the
/// bound was never asked, and both arithmetic mutations of it survived. A
/// marker can run off the bottom of a page as easily as off the side.
#[test]
fn a_region_that_runs_off_the_bottom_is_refused_like_one_that_runs_off_the_side() {
    let work = crate::test_support::work("taiyaku-test-notes-region-y");
    let over_y = Region {
        x: 0.1,
        y: 0.8,
        w: 0.2,
        h: 0.5,
    };
    let result = validate(&on("raw", 0, Some(over_y)), &work, &empty("t"));
    crate::test_support::cleanup("taiyaku-test-notes-region-y");

    assert!(
        matches!(result, Err(Invalid::Refused(_))),
        "y + h is 1.3, which is off the page: {result:?}"
    );
}

// `an_unreadable_notes_file_is_an_error_rather_than_nothing_recorded` staged
// an unreadable `taiyaku.notes.json` (a directory sitting where the file
// belonged) and asserted `load` did not read it as "nothing recorded". Task
// 13 removed the file this staged; the row-store equivalent of "storage is
// broken" is a SQLite failure, which `notes::load` already reports as `Err`
// rather than `Ok(None)` by construction — it only ever returns `Ok(None)`
// from `works::find_row` answering `None`, never from an error swallowed —
// and there is no equivalent of "make a directory unreadable" to stage
// against an in-memory database from a test in this crate.
