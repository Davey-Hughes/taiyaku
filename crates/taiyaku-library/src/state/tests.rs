use std::path::Path;

use super::*;
use crate::{Library, Note, Notes};

/// A work with one edition, `raw-mono`, and one member — enough to hang
/// progress, notes and truth rows off, and to give `truth_members_for`
/// something real to project.
fn a_work(conn: &Connection) -> i64 {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw-mono":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]}},
        "hub":"raw-mono","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    crate::import::sidecar(conn, &doc, Path::new("/x/taiyaku.json")).expect("import")
}

/// `(id, body, updated)` triples into a `Notes` document. `edition`/`slot`
/// are fixed and `region`/`color` absent throughout: nothing in this module's
/// merge rule looks at them, only `id` and `updated`.
fn notes_doc(entries: &[(&str, &str, &str)]) -> Notes {
    Notes {
        work: "t".to_string(),
        notes: entries
            .iter()
            .map(|(id, body, updated)| Note {
                id: (*id).to_string(),
                edition: "raw-mono".to_string(),
                slot: 0,
                region: None,
                color: None,
                body: (*body).to_string(),
                created: (*updated).to_string(),
                updated: (*updated).to_string(),
            })
            .collect(),
    }
}

/// The rule from §4.3: union by ULID, and per note the later `updated` wins.
#[test]
fn merging_notes_unions_by_id_and_takes_the_later_update() {
    let held = notes_doc(&[
        ("01H", "held", "2026-08-14T10:00:00Z"),
        ("01J", "only in db", "2026-08-14T10:00:00Z"),
    ]);
    let incoming = notes_doc(&[
        ("01H", "newer", "2026-08-14T11:00:00Z"),
        ("01K", "only on disk", "2026-08-14T09:00:00Z"),
    ]);

    let merged = merge_notes(&held, &incoming);
    let ids: Vec<&str> = merged.notes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["01H", "01J", "01K"], "the union, in id order");
    assert_eq!(merged.notes[0].body, "newer", "the later update wins");
}

/// An older incoming note must not overwrite a newer held one, the direction
/// `merging_notes_unions_by_id_and_takes_the_later_update` does not exercise:
/// its one shared id (`01H`) always has the incoming side newer.
#[test]
fn merging_notes_keeps_the_held_note_when_it_is_newer() {
    let held = notes_doc(&[("01H", "held, and later", "2026-08-14T12:00:00Z")]);
    let incoming = notes_doc(&[("01H", "incoming, and older", "2026-08-14T09:00:00Z")]);

    let merged = merge_notes(&held, &incoming);
    assert_eq!(merged.notes.len(), 1);
    assert_eq!(merged.notes[0].body, "held, and later");
}

/// The tie itself: `merge_notes`'s doc comment says an exact match on
/// `updated` keeps `held`, deterministically — `held_is_newer` in the
/// implementation is `>=`, not `>`. Neither of the two tests above exercises
/// this: one has the incoming side strictly newer, the other has the held
/// side strictly newer.
#[test]
fn merging_notes_keeps_the_held_note_on_an_exact_tie() {
    let held = notes_doc(&[("01H", "held", "2026-08-14T10:00:00Z")]);
    let incoming = notes_doc(&[("01H", "incoming", "2026-08-14T10:00:00Z")]);

    let merged = merge_notes(&held, &incoming);
    assert_eq!(merged.notes.len(), 1);
    assert_eq!(
        merged.notes[0].body, "held",
        "a tie on `updated` keeps held"
    );
}

#[test]
fn a_whole_slot_note_round_trips_with_no_region() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = notes_doc(&[("01H", "a note", "2026-08-14T10:00:00Z")]);
    put_notes(&conn, work_id, 1, &doc).expect("write");
    let back = notes(&conn, work_id, 1).expect("read");
    assert_eq!(
        back.notes[0].region, None,
        "all four columns NULL is no region"
    );
}

/// The companion the above test's name promises but does not itself cover: a
/// note anchored to part of a slot must round-trip its region, not just its
/// absence.
#[test]
fn a_note_with_a_region_round_trips() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = Notes {
        work: "t".to_string(),
        notes: vec![Note {
            id: "01H".to_string(),
            edition: "raw-mono".to_string(),
            slot: 7,
            region: Some(Region {
                x: 0.1,
                y: 0.2,
                w: 0.3,
                h: 0.4,
            }),
            color: Some("amber".to_string()),
            body: "a boxed note".to_string(),
            created: "2026-08-14T10:00:00Z".to_string(),
            updated: "2026-08-14T10:00:00Z".to_string(),
        }],
    };

    put_notes(&conn, work_id, 1, &doc).expect("write");
    let back = notes(&conn, work_id, 1).expect("read");

    assert_eq!(back.notes.len(), 1);
    assert_eq!(
        back.notes[0].region,
        Some(Region {
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4
        })
    );
    assert_eq!(back.notes[0].color.as_deref(), Some("amber"));
    assert_eq!(back.notes[0].slot, 7);
}

/// The invariant `region_from_columns` polices: `put_notes` can never write a
/// partial region, but a hand edit of the database could leave one, and
/// `notes` must refuse to silently guess at it rather than reading it back as
/// either "no region" or a made-up one.
#[test]
fn a_partially_null_region_is_refused_rather_than_guessed_at() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, rx, ry, rw, rh, body, created, updated)
         VALUES ('01H', 1, ?1, 'raw-mono', 0, 0.1, 0.2, NULL, NULL, 'b', 't', 't')",
        [work_id],
    )
    .expect("hand-inserted, bypassing put_notes");

    let result = notes(&conn, work_id, 1);
    assert!(
        result.is_err(),
        "a region with only rx/ry set must not read as anything"
    );
}

/// Two readers' notes on the same work must not leak into each other.
#[test]
fn notes_are_isolated_per_user() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    // Migration 1 inserts only user 1; a second row for this test alone.
    // `password_hash` is NOT NULL since migration 4, and `'!'` is the locked
    // placeholder that migration gives the seeded row — a value no password
    // verifies against, which is all this test needs from an account it never
    // logs in as.
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'admin')",
        [],
    )
    .expect("second user");

    put_notes(
        &conn,
        work_id,
        1,
        &notes_doc(&[("01H", "mine", "2026-08-14T10:00:00Z")]),
    )
    .expect("write for user 1");
    put_notes(
        &conn,
        work_id,
        2,
        &notes_doc(&[("01K", "theirs", "2026-08-14T10:00:00Z")]),
    )
    .expect("write for user 2");

    let first = notes(&conn, work_id, 1).expect("read for user 1");
    let second = notes(&conn, work_id, 2).expect("read for user 2");
    assert_eq!(first.notes.len(), 1);
    assert_eq!(first.notes[0].id, "01H");
    assert_eq!(second.notes.len(), 1);
    assert_eq!(second.notes[0].id, "01K");
}

/// A second `put_notes` call for the same reader must replace the first
/// wholesale, not accumulate — the row-store equivalent of the old file's
/// `store` overwriting the whole document.
#[test]
fn a_second_put_notes_replaces_the_first_rather_than_accumulating() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    put_notes(
        &conn,
        work_id,
        1,
        &notes_doc(&[("01H", "first", "2026-08-14T10:00:00Z")]),
    )
    .expect("first write");
    put_notes(
        &conn,
        work_id,
        1,
        &notes_doc(&[("01K", "second", "2026-08-14T11:00:00Z")]),
    )
    .expect("second write");

    let back = notes(&conn, work_id, 1).expect("read");
    let ids: Vec<&str> = back.notes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["01K"], "01H must be gone, not merged with 01K");
}

#[test]
fn progress_round_trips() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    put_progress(
        &conn,
        work_id,
        1,
        &crate::Progress {
            edition: "raw-mono".into(),
            slot: 193,
        },
    )
    .expect("write");
    let back = progress(&conn, work_id, 1).expect("read").expect("some");
    assert_eq!(back.slot, 193);
}

/// "Never opened" and "on the first page" must stay distinguishable — the
/// same rule `taiyaku-serve`'s file-backed `progress::load` was written to
/// keep (O3).
#[test]
fn progress_is_none_when_nothing_recorded() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let back = progress(&conn, work_id, 1).expect("read");
    assert_eq!(back, None);
}

/// The later write must win outright, not add a second row: `progress`'s
/// primary key is `(user_id, work_id)`, so this also pins that `put_progress`
/// is an upsert rather than an insert that fails the second time.
#[test]
fn a_second_put_progress_overwrites_the_first() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "raw-mono".into(),
            slot: 0,
        },
    )
    .expect("first write");
    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "raw-mono".into(),
            slot: 12,
        },
    )
    .expect("second write");

    let back = progress(&conn, work_id, 1).expect("read").expect("some");
    assert_eq!(back.slot, 12);
}

/// **A page turn must not reset the reading history**, which is the one
/// property standing between `put_progress` and the two columns
/// `001_initial.sql` describes as "behavioural data with NOTHING to backfill".
///
/// `progress` is `WITHOUT ROWID` with `PRIMARY KEY (user_id, work_id)`, so
/// `INSERT OR REPLACE` would be the obvious way to write it and would clear
/// `completed_at` and `read_count` on **every page turn** — a reader who
/// finished a volume in March would lose that fact the next time they opened
/// it, silently and permanently. `ON CONFLICT … DO UPDATE` naming three
/// columns is what keeps them; nothing observed that until this test, so the
/// swap survived the whole suite (whole-branch review 2, I6).
///
/// The two columns are written here by hand because nothing in this module's
/// interface manages them yet — which is exactly why the guard is needed
/// before something does.
#[test]
fn put_progress_leaves_completed_at_and_read_count_alone() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "raw-mono".into(),
            slot: 0,
        },
    )
    .expect("first write");
    conn.execute(
        "UPDATE progress SET completed_at = '2026-03-01T00:00:00Z', read_count = 2
         WHERE user_id = 1 AND work_id = ?1",
        [work_id],
    )
    .expect("a finished-once, now-re-reading row");

    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "raw-mono".into(),
            slot: 7,
        },
    )
    .expect("a page turn");

    let (edition, slot, completed_at, read_count): (String, u32, Option<String>, i64) = conn
        .query_row(
            "SELECT edition, slot, completed_at, read_count FROM progress
             WHERE user_id = 1 AND work_id = ?1",
            [work_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("the progress row");

    assert_eq!(slot, 7, "the page turn is recorded");
    assert_eq!(edition, "raw-mono");
    assert_eq!(
        completed_at.as_deref(),
        Some("2026-03-01T00:00:00Z"),
        "the date this work was finished must survive a page turn"
    );
    assert_eq!(read_count, 2, "and so must the number of times it was read");
}

#[test]
fn truth_is_none_when_nothing_recorded() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let back = truth(&conn, work_id).expect("read");
    assert!(
        back.is_none(),
        "TruthDoc has no PartialEq; assert on the variant"
    );
}

/// One box with every field populated — `axis`, `kind`, `ignore` and `text` —
/// on the real `raw-mono` edition `a_work` creates, alongside the member
/// digests the labelling was drawn against.
#[test]
fn truth_round_trips_a_box_and_the_members_it_was_drawn_against() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let mut editions = BTreeMap::new();
    editions.insert(
        "raw-mono".to_string(),
        TruthEdition {
            members: vec![RegionMember {
                sha256: "deadbeef".to_string(),
            }],
            pages: BTreeMap::from([(
                3,
                vec![TruthRegion {
                    x: 0.1,
                    y: 0.2,
                    w: 0.3,
                    h: 0.4,
                    axis: Some(Axis::Vertical),
                    kind: Kind::Dialogue,
                    ignore: true,
                    text: Some("こんにちは".to_string()),
                }],
            )]),
        },
    );
    let doc = TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions,
    };

    put_truth(&conn, work_id, &doc).expect("write");
    let back = truth(&conn, work_id).expect("read").expect("some");

    let edition = back.editions.get("raw-mono").expect("the edition");
    assert_eq!(
        edition.members,
        vec![RegionMember {
            sha256: "deadbeef".to_string()
        }],
        "the digests written with the document come back, unchanged"
    );
    let boxes = edition.pages.get(&3).expect("page 3");
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].axis, Some(Axis::Vertical));
    assert_eq!(boxes[0].kind, Kind::Dialogue);
    assert!(boxes[0].ignore);
    assert_eq!(boxes[0].text.as_deref(), Some("こんにちは"));
}

/// **The C2 defect of whole-branch review 2, closed.** `TruthEdition::members`
/// is the record of *which archives the boxes were drawn against*, and
/// `TruthEdition::is_stale_for` — the guard `taiyaku score` and
/// `score_corpus` both refuse on — compares it element-wise against the
/// digests of the archives in hand now. Re-projecting `members` from
/// whatever `member` rows exist at read time made that comparison compare a
/// list against itself: it could never report stale again, and an export
/// after a reimport wrote a `taiyaku.truth.json` asserting provenance that
/// was simply false, over the reader's only copy.
///
/// So the archives are replaced here the way a reimport replaces them — same
/// edition, different bytes — and the stored provenance must not follow.
#[test]
fn truth_keeps_the_provenance_it_was_written_with_when_the_archives_change() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let doc = TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions: BTreeMap::from([(
            "raw-mono".to_string(),
            TruthEdition {
                members: vec![RegionMember {
                    sha256: "aaaaaaaa".to_string(),
                }],
                pages: BTreeMap::from([(2, Vec::new())]),
            },
        )]),
    };
    put_truth(&conn, work_id, &doc).expect("write");

    // The archives are replaced under the same edition key — what
    // `import::replace_editions` does on every reimport.
    conn.execute("UPDATE member SET sha256 = 'bbbbbbbb'", [])
        .expect("the archives change");

    let back = truth(&conn, work_id).expect("read").expect("some");
    let edition = &back.editions["raw-mono"];
    assert_eq!(
        edition.members,
        vec![RegionMember {
            sha256: "aaaaaaaa".to_string()
        }],
        "the labels were drawn against `aaaaaaaa`; nothing that happened to \
         the `member` table afterwards can make that untrue"
    );
    assert!(
        edition.is_stale_for(&["bbbbbbbb".to_string()]),
        "the whole point of storing it: the guard that refuses to score labels \
         against archives that have since changed must be able to fire"
    );
}

/// **Closes the gap the previous version of this test documented.**
/// `TruthEdition::pages`'s own doc comment says an absent page key means
/// "never looked at" and a present-but-empty one means "looked at, found
/// nothing" — and calls that the labelling progress tracker. `truth_page`
/// exists precisely so a page with zero boxes still writes a row: `truth`
/// seeds `pages` from `truth_page`, then attaches boxes from `truth_box`, so
/// an examined-and-empty page comes back as a present key with an empty
/// vector rather than vanishing into "nobody has looked".
#[test]
fn a_page_looked_at_and_found_empty_round_trips_as_present_and_empty() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let mut editions = BTreeMap::new();
    editions.insert(
        "raw-mono".to_string(),
        TruthEdition {
            members: Vec::new(),
            pages: BTreeMap::from([(5, Vec::new())]), // looked at page 5, found nothing
        },
    );
    let doc = TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions,
    };

    put_truth(&conn, work_id, &doc).expect("write");
    let back = truth(&conn, work_id).expect("read").expect("some");

    let pages = &back.editions["raw-mono"].pages;
    let boxes = pages.get(&5).expect(
        "page 5 must be a present key — it was looked at, even though nothing was found there",
    );
    assert!(
        boxes.is_empty(),
        "page 5 was found empty; it must not carry a phantom box"
    );
}

/// The composite FK on `truth_box(work_id, edition, page)` is what turns
/// "`put_truth` always writes the page row before its boxes" from a write-order
/// convention this module happens to honour into something the database
/// itself refuses to violate. A box inserted directly, bypassing `put_truth`
/// entirely, on a page nobody has recorded looking at must be rejected by the
/// connection — not merely unreachable through this module's own functions.
#[test]
fn a_truth_box_on_an_unrecorded_page_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let result = conn.execute(
        "INSERT INTO truth_box (work_id, edition, page, x, y, w, h, kind)
         VALUES (?1, 'raw-mono', 3, 0.0, 0.0, 1.0, 1.0, 'other')",
        [work_id],
    );

    assert!(
        result.is_err(),
        "a truth_box row on a page with no truth_page row must be refused by \
         the FK, not merely avoided by put_truth's write order"
    );
}

/// A second `put_truth` must replace the first wholesale — §4.3's
/// whole-document rule, applied here to the write this module actually does
/// rather than to which of two documents to keep.
#[test]
fn a_second_put_truth_replaces_the_first_rather_than_accumulating() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let a_box = || TruthRegion {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
        axis: None,
        kind: Kind::Other,
        ignore: false,
        text: None,
    };
    let doc_with_page = |page: u32| TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions: BTreeMap::from([(
            "raw-mono".to_string(),
            TruthEdition {
                members: Vec::new(),
                pages: BTreeMap::from([(page, vec![a_box()])]),
            },
        )]),
    };

    put_truth(&conn, work_id, &doc_with_page(0)).expect("first write");
    put_truth(&conn, work_id, &doc_with_page(9)).expect("second write");

    let back = truth(&conn, work_id).expect("read").expect("some");
    let pages: Vec<&u32> = back.editions["raw-mono"].pages.keys().collect();
    assert_eq!(
        pages,
        vec![&9],
        "page 0's box must be gone, not merged with page 9's"
    );
}

/// An edition the sidecar no longer holds — the shape a reimport that drops
/// or renames an edition leaves behind, since `put_truth` never clears truth
/// rows when `import::replace_editions` deletes and reinserts editions. The
/// labels are still there and so is the provenance they were drawn against;
/// nothing here consults `edition` or `member` at all, which is what lets a
/// caller ask `is_stale_for` about labels whose archives are gone.
#[test]
fn truth_keeps_an_edition_and_its_provenance_after_the_edition_row_is_gone() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let doc = TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions: BTreeMap::from([(
            "renamed-away".to_string(),
            TruthEdition {
                members: vec![RegionMember {
                    sha256: "cafebabe".to_string(),
                }],
                pages: BTreeMap::from([(0, Vec::new())]),
            },
        )]),
    };
    put_truth(&conn, work_id, &doc).expect("write");

    let back = truth(&conn, work_id).expect("read").expect("some");
    let edition = back
        .editions
        .get("renamed-away")
        .expect("the edition key survives even though the edition row does not");
    assert_eq!(
        edition.members,
        vec![RegionMember {
            sha256: "cafebabe".to_string()
        }],
        "no `edition` row of this name has ever existed in this work, and the \
         provenance is still exactly what was labelled against"
    );
}

/// Both axes and all six kinds, written and read back — the closed sets
/// `axis_text`/`kind_text` and `parse_axis`/`parse_kind` encode, in the one
/// place where a mismatch between the two halves would be silent.
///
/// A box stored as the wrong kind is not a crash: it is a labelled dialogue
/// balloon that scores as an SFX. Only `Dialogue` and `Vertical` were ever
/// written by a test before this, so swapping any of the other five strings
/// for another changed nothing anybody could see.
#[test]
fn every_axis_and_every_kind_round_trips_through_the_rows() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let cases = [
        (Some(Axis::Vertical), Kind::Dialogue),
        (Some(Axis::Horizontal), Kind::Narration),
        (None, Kind::Sfx),
        (Some(Axis::Vertical), Kind::Logo),
        (Some(Axis::Horizontal), Kind::Credits),
        (None, Kind::Other),
    ];
    let pages: BTreeMap<u32, Vec<TruthRegion>> = cases
        .iter()
        .enumerate()
        .map(|(page, (axis, kind))| {
            (
                u32::try_from(page).expect("six pages"),
                vec![TruthRegion {
                    x: 0.0,
                    y: 0.0,
                    w: 0.5,
                    h: 0.5,
                    axis: *axis,
                    kind: *kind,
                    ignore: false,
                    text: None,
                }],
            )
        })
        .collect();
    let doc = TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions: BTreeMap::from([(
            "raw-mono".to_string(),
            TruthEdition {
                members: Vec::new(),
                pages,
            },
        )]),
    };

    put_truth(&conn, work_id, &doc).expect("write");
    let back = truth(&conn, work_id).expect("read").expect("some");
    let pages = &back.editions["raw-mono"].pages;

    for (page, (axis, kind)) in cases.iter().enumerate() {
        let page = u32::try_from(page).expect("six pages");
        let region = &pages.get(&page).expect("the page")[0];
        assert_eq!(region.axis, *axis, "page {page}'s axis");
        assert_eq!(region.kind, *kind, "page {page}'s kind");
    }
}

/// An edition whose provenance was recorded before any page was opened is a
/// present, empty-paged key — not a document that vanished. The same
/// absent-versus-empty discipline `truth_page` keeps for pages, one level up:
/// `truth` seeds its edition map from `truth_member` as well as `truth_page`,
/// so the only `None` is a work with no truth rows of either kind.
#[test]
fn an_edition_with_provenance_and_no_pages_survives_the_round_trip() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let doc = TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions: BTreeMap::from([(
            "raw-mono".to_string(),
            TruthEdition {
                members: vec![RegionMember {
                    sha256: "deadbeef".to_string(),
                }],
                pages: BTreeMap::new(),
            },
        )]),
    };
    put_truth(&conn, work_id, &doc).expect("write");

    let back = truth(&conn, work_id)
        .expect("read")
        .expect("recorded provenance is a document, not nothing at all");
    let edition = &back.editions["raw-mono"];
    assert_eq!(edition.members.len(), 1);
    assert!(
        edition.pages.is_empty(),
        "nobody has looked at a page yet; none may be invented"
    );
}

/// Every closed set this module reads back out of a text column must refuse a
/// value none of its own writers ever puts there, the same rule
/// `region_from_columns` polices for a note's region — applied here to a hand
/// edit that reaches `truth_box.axis`.
#[test]
fn truth_refuses_a_stored_box_with_an_axis_put_truth_never_writes() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        "INSERT INTO truth_page (work_id, edition, page) VALUES (?1, 'raw-mono', 0)",
        [work_id],
    )
    .expect("page");
    conn.execute(
        "INSERT INTO truth_box (work_id, edition, page, x, y, w, h, axis, kind)
         VALUES (?1, 'raw-mono', 0, 0.0, 0.0, 1.0, 1.0, 'diagonal', 'other')",
        [work_id],
    )
    .expect("hand-inserted, bypassing put_truth");

    let result = truth(&conn, work_id);
    assert!(
        result.is_err(),
        "an axis value put_truth never writes must not be silently accepted"
    );
}

/// The `truth_box.kind` analogue of
/// `truth_refuses_a_stored_box_with_an_axis_put_truth_never_writes`.
#[test]
fn truth_refuses_a_stored_box_with_a_kind_put_truth_never_writes() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        "INSERT INTO truth_page (work_id, edition, page) VALUES (?1, 'raw-mono', 0)",
        [work_id],
    )
    .expect("page");
    conn.execute(
        "INSERT INTO truth_box (work_id, edition, page, x, y, w, h, kind)
         VALUES (?1, 'raw-mono', 0, 0.0, 0.0, 1.0, 1.0, 'watermark')",
        [work_id],
    )
    .expect("hand-inserted, bypassing put_truth");

    let result = truth(&conn, work_id);
    assert!(
        result.is_err(),
        "a kind value put_truth never writes must not be silently accepted"
    );
}

/// **Box order within a page survives an index on `truth_box.work_id`.**
/// `truth`'s query used to carry no `ORDER BY`, and round-tripped only because
/// nothing indexes that column: a bare `WHERE work_id = ?1` is a table scan,
/// and a table scan returns rowid order, which is the order `put_truth`
/// inserted. That is a property of the *plan*, not of the query, and the plan
/// is one `CREATE INDEX` away from changing — for a `work_id` lookup on a
/// large library, the obvious optimisation.
///
/// So the index is created here rather than waited for. The three boxes are
/// written in an order no sort would reproduce — their `x` values descend, and
/// their `text` labels are what the assertion reads — and the index is keyed
/// `(work_id, page, x)` so that if SQLite prefers it, it hands back ascending
/// `x` and the labelling comes out backwards. With `ORDER BY id` the plan
/// cannot matter.
#[test]
fn box_order_within_a_page_survives_an_index_that_would_reorder_the_scan() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let region = |x: f64, text: &str| TruthRegion {
        x,
        y: 0.0,
        w: 0.1,
        h: 0.1,
        axis: None,
        kind: Kind::Dialogue,
        ignore: false,
        text: Some(text.to_string()),
    };
    let mut editions = BTreeMap::new();
    editions.insert(
        "raw-mono".to_string(),
        TruthEdition {
            members: vec![RegionMember {
                sha256: "deadbeef".to_string(),
            }],
            pages: BTreeMap::from([(
                7,
                vec![
                    region(0.9, "drawn first"),
                    region(0.5, "drawn second"),
                    region(0.1, "drawn third"),
                ],
            )]),
        },
    );
    put_truth(
        &conn,
        work_id,
        &TruthDoc {
            taiyaku_truth: taiyaku_core::TRUTH_VERSION,
            editions,
        },
    )
    .expect("write");

    conn.execute(
        "CREATE INDEX idx_truth_box_work ON truth_box (work_id, page, x)",
        [],
    )
    .expect("the index a later commit adds for speed");

    let back = truth(&conn, work_id).expect("read").expect("some");
    let boxes = &back.editions["raw-mono"].pages[&7];
    let labels: Vec<&str> = boxes
        .iter()
        .map(|b| b.text.as_deref().expect("every box carries its label"))
        .collect();
    assert_eq!(
        labels,
        vec!["drawn first", "drawn second", "drawn third"],
        "boxes come back in the order they were drawn, not in whatever order \
         the index makes cheapest"
    );
}

/// A two-edition work with one solved edge, for the alignment writer below.
///
/// `a_work` above has one edition and no alignments, which is right for
/// progress, notes and truth and useless here: the row set this function
/// exists to replace has to be non-empty before the replacement, or "replaced"
/// and "appended to" look the same.
fn a_work_with_an_edge(conn: &Connection) -> (i64, Sidecar) {
    let json = r#"{"taiyaku":1,"work":{"title":"E","reading":"rtl"},
        "editions":{
          "raw":{"label":"R","members":[{"path":"raw/v01.cbz","sha256":"aa","pages":4}]},
          "en":{"label":"E","members":[{"path":"en/v01.cbz","sha256":"bb","pages":4}]}},
        "hub":"raw",
        "alignments":[{"from":"raw","to":"en","by":"solver",
          "runs":[{"a":"0-3","b":"0-3"}]}]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id = crate::import::sidecar(conn, &doc, Path::new("/e/taiyaku.json")).expect("import");
    (work_id, doc)
}

/// **`put_alignments` replaces, it does not append.** The one property that is
/// this function's own rather than `import::replace_alignments`': a correction
/// arrives as a whole document every time, so writing the same document twice
/// has to leave the same rows, not two copies of them. Without the `DELETE`
/// the second write doubles every run and `project::sidecar` reads back a
/// document asserting each pairing twice.
#[test]
fn put_alignments_replaces_the_stored_edges_rather_than_adding_to_them() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, doc) = a_work_with_an_edge(&conn);

    let runs_now = || {
        crate::project::sidecar(&conn, work_id)
            .expect("projection")
            .alignments
            .iter()
            .map(|a| a.runs.len())
            .sum::<usize>()
    };
    let imported = runs_now();
    assert_eq!(imported, 1, "the import wrote the one solved run");

    put_alignments(&conn, work_id, &doc).expect("write");
    assert_eq!(
        runs_now(),
        imported,
        "the same document again is the same rows"
    );
}

/// The write itself, seen through the projection: a corrected document's runs
/// are what comes back out, hand attribution and all.
#[test]
fn put_alignments_writes_a_hand_corrected_document_back_out_intact() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let (work_id, mut doc) = a_work_with_an_edge(&conn);

    doc.correct(
        "raw",
        "en",
        taiyaku_core::Slot {
            page: 2,
            half: None,
        },
        Some(taiyaku_core::Slot {
            page: 1,
            half: None,
        }),
        Some("checked"),
    )
    .expect("the fixture's edge can be corrected");
    put_alignments(&conn, work_id, &doc).expect("write");

    let back = crate::project::sidecar(&conn, work_id).expect("projection");
    let hand: Vec<&str> = back
        .alignments
        .iter()
        .flat_map(|a| &a.runs)
        .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
        .map(|r| r.note.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        hand,
        vec!["checked", "checked"],
        "both the asserted run and the one it displaced are stored hand-decided, \
         each carrying the note the correction was made with"
    );
}

/// **Two readers may hold the same note id**, which a global `TEXT PRIMARY KEY`
/// on `note.id` made impossible.
///
/// Notes became per-user in spec A2 — `put_notes` deletes
/// `WHERE work_id = ?1 AND user_id = ?2` — but the id namespace stayed global,
/// so a second reader submitting a document containing an id the first already
/// held violated the primary key. The write is whole-document, so B's *entire*
/// notes document was rejected rather than one note, and the 200-versus-500 told
/// B whether an id existed anywhere in the library.
///
/// Reachable without anyone acting oddly: B imports a `taiyaku.notes.json` A
/// exported, or both accounts receive the same sidecar. Found by the
/// whole-branch review, 2026-08-18; fixed by migration 5.
#[test]
fn two_readers_can_hold_the_same_note_id() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'user')",
        [],
    )
    .expect("second user");

    let shared = "01HSHAREDID";
    put_notes(
        &conn,
        work_id,
        1,
        &notes_doc(&[(shared, "alice's", "2026-08-18T10:00:00Z")]),
    )
    .expect("alice writes");
    put_notes(
        &conn,
        work_id,
        2,
        &notes_doc(&[(shared, "bob's", "2026-08-18T10:00:00Z")]),
    )
    .expect("bob writes a note with the id alice already holds");

    let alice = notes(&conn, work_id, 1).expect("reads");
    let bob = notes(&conn, work_id, 2).expect("reads");
    assert_eq!(alice.notes.len(), 1);
    assert_eq!(bob.notes.len(), 1);
    assert_eq!(alice.notes[0].body, "alice's");
    assert_eq!(bob.notes[0].body, "bob's");
}

/// One reader still cannot hold one id twice — the constraint moved rather than
/// went away, so `put_notes`'s own `# Errors` about a repeated id stays true
/// within a document.
#[test]
fn one_reader_still_cannot_hold_one_id_twice() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let doubled = notes_doc(&[
        ("01HSAME", "first", "2026-08-18T10:00:00Z"),
        ("01HSAME", "second", "2026-08-18T10:00:00Z"),
    ]);
    assert!(
        put_notes(&conn, work_id, 1, &doubled).is_err(),
        "a document repeating an id must still be refused"
    );
}

/// **One reader may hold one note id on two different works.**
///
/// `(user_id, id)` was too wide by one column: `put_notes` replaces one
/// reader's notes on one work, so that is the scope an id is unique in.
/// Reachable by copying a `taiyaku.notes.json` between volumes, and the failure
/// was the same whole-document 500 migration 5 was written to remove.
///
/// Found by the second whole-branch review.
#[test]
fn one_reader_can_hold_one_note_id_on_two_works() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = a_work(&conn);
    let second = {
        let json = r#"{"taiyaku":1,"work":{"title":"Second","reading":"rtl"},
            "editions":{"raw-mono":{"label":"R","members":[
              {"path":"raw/v02.cbz","sha256":"cafebabe","pages":200}]}},
            "hub":"raw-mono","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, Path::new("/y/taiyaku.json")).expect("import")
    };

    let shared = "01HSAMEIDTWOWORKS";
    put_notes(
        &conn,
        first,
        1,
        &notes_doc(&[(shared, "on volume one", "2026-08-19T10:00:00Z")]),
    )
    .expect("writes on the first work");
    put_notes(
        &conn,
        second,
        1,
        &notes_doc(&[(shared, "on volume two", "2026-08-19T10:00:00Z")]),
    )
    .expect("the same reader, the same id, a different work");

    assert_eq!(
        notes(&conn, first, 1).expect("reads").notes[0].body,
        "on volume one"
    );
    assert_eq!(
        notes(&conn, second, 1).expect("reads").notes[0].body,
        "on volume two"
    );
}
