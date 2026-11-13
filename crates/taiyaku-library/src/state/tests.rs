use std::path::Path;

use taiyaku_core::{Direction, Edition, Half, Member, MemberPage};

use super::*;
use crate::{Library, Note, Notes};

/// A work with one edition, `raw-mono`, and one member — enough to hang
/// progress, notes and truth rows off.
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
        taiyaku_notes: crate::NOTES_VERSION,
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
                stale: false,
                at: None,
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
        taiyaku_notes: crate::NOTES_VERSION,
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
            stale: false,
            at: None,
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
/// wholesale, not accumulate.
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
    let back = progress(&conn, work_id, 1).expect("read");
    assert_eq!(back.len(), 1, "one edition written, one position held");
    assert_eq!(back[0].slot, 193);
}

/// "Never opened" and "on the first page" must stay distinguishable (O3):
/// "never opened" is the empty vector.
#[test]
fn progress_is_empty_when_nothing_recorded() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let back = progress(&conn, work_id, 1).expect("read");
    assert_eq!(back, Vec::new());
}

/// Newest first, so a caller that wants "where was I" takes the head.
///
/// **`ORDER BY updated DESC, edition ASC`.** `updated` has one-second
/// resolution, so two positions written in the same second would otherwise
/// come back in an order SQLite is free to vary — a test that passed on
/// insertion order and failed on a rebuild. The tiebreak makes the order
/// total.
///
/// This test exercises the primary key (`updated DESC`) by choosing edition
/// names whose alphabetical order is the *opposite* of their recency order.
/// `"en-fan"` is written first and aged to 2020; `"raw-mono"` is written
/// last as newest. With correct `ORDER BY updated DESC, edition ASC`, the
/// result is `["raw-mono", "en-fan"]` (newest first). With a broken query
/// that had lost the `updated DESC` key — leaving only `ORDER BY edition ASC`
/// — the result would be `["en-fan", "raw-mono"]` (alphabetical). Only the
/// correct sort produces the asserted order, so this test can fail.
#[test]
fn positions_come_back_newest_first() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    // Create a work with a second edition so we can write to "en-fan".
    // The standard a_work has only "raw-mono", so we construct directly.
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw-mono":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]},
        "en-fan":{"label":"F","members":[
          {"path":"en/v01.cbz","sha256":"cafebabe","pages":300}]}},
        "hub":"raw-mono","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id =
        crate::import::sidecar(&conn, &doc, Path::new("/x/taiyaku.json")).expect("import");

    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "en-fan".into(),
            slot: 1,
        },
    )
    .expect("older");
    // `updated` defaults to `datetime('now')`, whose resolution is one
    // second, so the two rows are separated by hand rather than by waiting.
    conn.execute(
        "UPDATE progress SET updated = '2020-01-01 00:00:00' WHERE edition = 'en-fan'",
        [],
    )
    .expect("ageing the first row");
    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "raw-mono".into(),
            slot: 2,
        },
    )
    .expect("newer");

    let held = progress(&conn, work_id, 1).expect("reading back");
    assert_eq!(
        held.iter().map(|p| p.edition.as_str()).collect::<Vec<_>>(),
        vec!["raw-mono", "en-fan"],
        "the most recently updated position comes first; edition names are chosen \
         so alphabetical order (en-fan < raw-mono) and recency order are opposite, \
         so only a correct ORDER BY updated DESC produces this result"
    );
}

/// The tiebreak: when two positions have the same `updated` timestamp, they
/// must come back in ascending `edition` order.
///
/// `updated` has one-second resolution. Two positions written in the same
/// second (a real case, not contrived) would come back in an undefined order
/// without the `ORDER BY … edition ASC` tiebreak — a test that passes on
/// insertion order and fails on a rebuild. This test explicitly sets both rows'
/// `updated` to an identical value, so the tiebreak is the only thing that
/// can order them. `progress` queries `ORDER BY updated DESC, edition ASC`,
/// so both positions have the same `updated` here, and the result is
/// determined entirely by `edition ASC`.
#[test]
fn positions_with_the_same_updated_come_back_in_ascending_edition_order() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    // Use a work with two editions.
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw-mono":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]},
        "en-fan":{"label":"F","members":[
          {"path":"en/v01.cbz","sha256":"cafebabe","pages":300}]}},
        "hub":"raw-mono","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id =
        crate::import::sidecar(&conn, &doc, Path::new("/x/taiyaku.json")).expect("import");

    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "raw-mono".into(),
            slot: 1,
        },
    )
    .expect("first position");
    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "en-fan".into(),
            slot: 2,
        },
    )
    .expect("second position");

    // Set both rows' `updated` to the same literal value, so the tiebreak is
    // the only thing that can order them.
    conn.execute(
        "UPDATE progress SET updated = '2026-08-27 12:00:00' WHERE work_id = ?1",
        [work_id],
    )
    .expect("synchronizing timestamps");

    let held = progress(&conn, work_id, 1).expect("reading back");
    assert_eq!(
        held.iter().map(|p| p.edition.as_str()).collect::<Vec<_>>(),
        vec!["en-fan", "raw-mono"],
        "with identical updated timestamps, positions come back in ascending edition order; \
         only ORDER BY edition ASC makes this happen"
    );
}

/// **A position on a second edition must not disturb the first.** `progress`'s
/// primary key is `(user_id, work_id, edition)`, so a reader at slot 0 of
/// `raw-mono` who then opens `full-color` and turns one page keeps the
/// `raw-mono` position.
#[test]
fn a_second_put_progress_on_a_different_edition_leaves_the_first_position_standing() {
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
            edition: "full-color".into(),
            slot: 12,
        },
    )
    .expect("second write, a different edition");

    let held = progress(&conn, work_id, 1).expect("reading back");
    let mut pairs: Vec<(String, u32)> = held.into_iter().map(|p| (p.edition, p.slot)).collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![("full-color".to_string(), 12), ("raw-mono".to_string(), 0)],
        "both editions' positions must stand, not just the one written last"
    );
}

/// **A page turn must not reset the reading history**, which is the one
/// property standing between `put_progress` and the two columns
/// `001_initial.sql` describes as "behavioural data with NOTHING to backfill".
///
/// `progress` is `WITHOUT ROWID` with `PRIMARY KEY (user_id, work_id, edition)`,
/// so `INSERT OR REPLACE` would be the obvious way to write it and would clear
/// `completed_at` and `read_count` on **every page turn** — a reader who
/// finished a volume in March would lose that fact the next time they opened
/// it, silently and permanently. `ON CONFLICT … DO UPDATE` naming the two
/// columns that are not part of the conflict target is what keeps them.
///
/// The two columns are written here by hand because nothing in this module's
/// interface manages them, which is exactly why the guard is needed before
/// something does.
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

/// `slot` on `raw-mono`, put in the order its save arrived in.
fn put_in_order(
    conn: &Connection,
    order: &ProgressOrder,
    arrival: Arrival,
    (user_id, work_id): (i64, i64),
    slot: u32,
) -> Result<Landed, String> {
    let progress = Progress {
        edition: "raw-mono".into(),
        slot,
    };
    put_progress_in_order(
        conn,
        order,
        arrival,
        work_id,
        user_id,
        &progress,
        Numbering::Store,
    )
}

/// The slot held on `raw-mono`, if any.
fn held_slot(conn: &Connection, (user_id, work_id): (i64, i64)) -> Option<u32> {
    let held = progress(conn, work_id, user_id).expect("reading back");
    held.iter()
        .find(|position| position.edition == "raw-mono")
        .map(|position| position.slot)
}

/// Three of one reader's saves on one work end at the last arrival's slot,
/// in each order they could take the write lock in, and each that lands
/// after a later arrival writes nothing.
#[test]
fn three_saves_end_at_the_last_arrivals_slot_in_every_landing_order() {
    use Landed::{Superseded as S, Written as W};
    for (landing, landed) in [
        ([0, 1, 2], [W, W, W]),
        ([0, 2, 1], [W, W, S]),
        ([1, 0, 2], [W, S, W]),
        ([1, 2, 0], [W, W, S]),
        ([2, 0, 1], [W, S, S]),
        ([2, 1, 0], [W, S, S]),
    ] {
        let library = Library::open_in_memory().expect("library");
        let conn = library.conn();
        let key = (1, a_work(&conn));
        let arrivals = [Arrival::next(), Arrival::next(), Arrival::next()];

        let outcomes: Vec<Landed> = landing
            .iter()
            .map(|&which| {
                put_in_order(
                    &conn,
                    library.progress_order(),
                    arrivals[which],
                    key,
                    [10, 11, 12][which],
                )
                .expect("writes")
            })
            .collect();

        assert_eq!(outcomes, landed, "landing in the order {landing:?}");
        assert_eq!(
            held_slot(&conn, key),
            Some(12),
            "landing in the order {landing:?}"
        );
    }
}

/// A save on another work, or by another reader, never supersedes one that
/// arrived before it: each lands, newest arrival first.
#[test]
fn saves_on_other_works_or_by_other_readers_never_supersede_each_other() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = a_work(&conn);
    let json = r#"{"taiyaku":1,"work":{"title":"U","reading":"rtl"},
        "editions":{"raw-mono":{"label":"R","members":[
          {"path":"raw/u01.cbz","sha256":"feedface","pages":300}]}},
        "hub":"raw-mono","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let second = crate::import::sidecar(&conn, &doc, Path::new("/y/taiyaku.json")).expect("import");
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'admin')",
        [],
    )
    .expect("second user");
    let keys = [(1, first), (1, second), (2, first)];
    let arrivals = [Arrival::next(), Arrival::next(), Arrival::next()];

    for which in [2, 1, 0] {
        let landed = put_in_order(
            &conn,
            library.progress_order(),
            arrivals[which],
            keys[which],
            [3, 4, 5][which],
        )
        .expect("writes");
        assert_eq!(landed, Landed::Written, "{:?}", keys[which]);
    }

    let held: Vec<Option<u32>> = keys.iter().map(|&key| held_slot(&conn, key)).collect();
    assert_eq!(held, [Some(3), Some(4), Some(5)]);
}

/// A save whose commit fails leaves no mark, so an older save that lands
/// after it is still written.
#[test]
fn an_older_save_lands_after_a_later_one_failed_to_commit() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let key = (1, a_work(&conn));
    // A row naming no work, checked only at COMMIT, written by slot 99 alone.
    conn.execute_batch(
        "CREATE TABLE refused_at_commit (
           work_id INTEGER REFERENCES work(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER refuse_slot_99 AFTER INSERT ON progress WHEN NEW.slot = 99
         BEGIN INSERT INTO refused_at_commit VALUES (-1); END;",
    )
    .expect("a commit that fails for slot 99");
    let (older, later) = (Arrival::next(), Arrival::next());

    let failed = put_in_order(&conn, library.progress_order(), later, key, 99);
    let landed = put_in_order(&conn, library.progress_order(), older, key, 4);

    let why = failed.expect_err("the later save's commit fails");
    assert!(
        why.starts_with("committing progress for work ")
            && why.ends_with("FOREIGN KEY constraint failed"),
        "{why}"
    );
    assert_eq!(landed, Ok(Landed::Written), "the older save");
    assert_eq!(held_slot(&conn, key), Some(4));
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

/// `TruthEdition::members` is the record of *which archives the boxes were
/// drawn against*, and `TruthEdition::is_stale_for` — the guard `taiyaku score`
/// and `score_corpus` both refuse on — compares it element-wise against the
/// digests of the archives in hand now. Re-projecting `members` from whatever
/// `member` rows exist at read time would make that comparison compare a list
/// against itself: it could never report stale.
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
/// balloon that scores as an SFX.
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
/// Without an `ORDER BY`, a bare `WHERE work_id = ?1` is a table scan only
/// while nothing indexes that column, and a table scan returns rowid order,
/// which is the order `put_truth` inserted. That is a property of the *plan*,
/// not of the query, and the plan is one `CREATE INDEX` away from changing —
/// for a `work_id` lookup on a large library, the obvious optimisation.
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

/// **Two readers may hold the same note id.**
///
/// The write is whole-document, so an id reader A already holds must not
/// reject reader B's entire notes document.
///
/// Reachable without anyone acting oddly: B imports a `taiyaku.notes.json` A
/// exported, or both accounts receive the same sidecar.
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

/// One reader cannot hold one id twice, so `put_notes`'s own `# Errors` about
/// a repeated id holds within a document.
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
/// `put_notes` replaces one reader's notes on one work, so that is the
/// scope an id is unique in. Reachable by copying a `taiyaku.notes.json`
/// between volumes.
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

// --- anchors (O244) ------------------------------------------------------------

fn an_anchor(sha256: &str, nth: u32, page: u32, half: Option<Half>) -> MemberPage {
    MemberPage {
        sha256: sha256.into(),
        nth,
        page,
        half,
    }
}

/// `a` (3 pages, page 1 a spread) then `b` (2 pages). Under rtl the slots are
/// page 0, page 1's right then left half, then pages 2, 3 and 4: slots 0-5.
fn a_layout() -> Edition {
    let member = |sha256: &str, pages: u32| Member {
        path: format!("{sha256}.cbz"),
        sha256: sha256.into(),
        pages,
    };
    Edition {
        label: "R".into(),
        members: vec![member("a", 3), member("b", 2)],
        spreads: vec![1],
        credits: Vec::new(),
    }
}

/// One case per row of the resolution table, for a position. Every stored
/// slot differs from the slot the anchor would give, so each arm's answer is
/// its own.
#[test]
fn a_position_answers_its_page_or_falls_back_to_its_slot_clamped() {
    let layout = a_layout();
    let rtl = Direction::RightToLeft;
    let on = Some((&layout, rtl));
    let gone = an_anchor("z", 0, 0, None);

    assert_eq!(
        resolve_position(None, 9, Some(&an_anchor("b", 0, 1, None))),
        9,
        "an edition key the work no longer has: the stored slot, unclamped"
    );
    assert_eq!(
        resolve_position(on, 9, None),
        9,
        "no anchor: the stored slot, unclamped"
    );
    assert_eq!(
        resolve_position(on, 2, Some(&an_anchor("b", 0, 1, None))),
        5,
        "the page: `b`'s second page is the last slot"
    );
    assert_eq!(
        resolve_position(on, 3, Some(&an_anchor("a", 0, 1, None))),
        1,
        "a whole page that is now a spread: the page's first slot"
    );
    assert_eq!(
        resolve_position(on, 9, Some(&gone)),
        5,
        "the page gone: the stored slot clamped to the last slot"
    );
    let empty = Edition {
        members: Vec::new(),
        spreads: Vec::new(),
        ..a_layout()
    };
    assert_eq!(
        resolve_position(Some((&empty, rtl)), 9, Some(&gone)),
        9,
        "the page gone from an edition with no slots: the stored slot"
    );
}

/// One case per row of the resolution table, for a note: it never moves to a
/// slot its anchor does not name exactly, and is `stale` wherever it stays
/// for want of its page.
#[test]
fn a_note_answers_its_exact_page_or_stays_at_its_slot_stale() {
    let layout = a_layout();
    let rtl = Direction::RightToLeft;
    let on = Some((&layout, rtl));

    assert_eq!(
        resolve_note(None, 9, Some(&an_anchor("b", 0, 1, None))),
        (9, true),
        "an edition key the work no longer has: the stored slot, stale"
    );
    assert_eq!(
        resolve_note(None, 9, None),
        (9, true),
        "stale there whatever its anchor"
    );
    assert_eq!(
        resolve_note(on, 9, None),
        (9, false),
        "no anchor: the stored slot"
    );
    assert_eq!(
        resolve_note(on, 2, Some(&an_anchor("b", 0, 1, None))),
        (5, false),
        "the page: `b`'s second page"
    );
    assert_eq!(
        resolve_note(on, 3, Some(&an_anchor("a", 0, 1, None))),
        (3, true),
        "a whole page that is now a spread: the stored slot, stale"
    );
    assert_eq!(
        resolve_note(on, 9, Some(&an_anchor("z", 0, 0, None))),
        (9, true),
        "the page gone: the stored slot, unclamped, stale"
    );
}

/// `aa` (3 pages) then `bb` (4 pages, its page 1 a spread), read rtl: slots
/// 0-2 are `aa`'s, 3 is `bb`'s first page, 4 and 5 its spread's right and
/// left halves, 6 and 7 its last two pages.
fn a_work_of_two_members(conn: &Connection) -> i64 {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/c001.cbz","sha256":"aa","pages":3},
          {"path":"raw/c002.cbz","sha256":"bb","pages":4}],
          "spreads":[4]}},
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    crate::import::sidecar(conn, &doc, Path::new("/x/taiyaku.json")).expect("import")
}

type StoredAnchor = (Option<String>, Option<u32>, Option<u32>, Option<String>);

/// The four anchor columns of one reader's `raw` position, read by SQL: the
/// columns themselves are what these tests check.
fn stored_anchor(conn: &Connection, work_id: i64, user_id: i64) -> StoredAnchor {
    conn.query_row(
        "SELECT at_sha256, at_nth, at_page, at_half FROM progress
          WHERE work_id = ?1 AND user_id = ?2 AND edition = 'raw'",
        params![work_id, user_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .expect("the position's row")
}

fn put_raw(conn: &Connection, work_id: i64, user_id: i64, slot: u32) {
    put_progress(
        conn,
        work_id,
        user_id,
        &Progress {
            edition: "raw".into(),
            slot,
        },
    )
    .expect("writes the position");
}

/// Every position `state::` writes names its page, on a page turn as on the
/// first write, and a position past the end names none.
#[test]
fn every_position_write_names_the_page_it_stands_on() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work_of_two_members(&conn);

    put_raw(&conn, work_id, 1, 1);
    assert_eq!(
        stored_anchor(&conn, work_id, 1),
        (Some("aa".into()), Some(0), Some(1), None),
        "the first write names `aa`'s second page"
    );
    put_raw(&conn, work_id, 1, 5);
    assert_eq!(
        stored_anchor(&conn, work_id, 1),
        (Some("bb".into()), Some(0), Some(1), Some("left".into())),
        "a page turn names the page it turned to: `bb`'s spread, its left half"
    );

    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'admin')",
        [],
    )
    .expect("a second reader, for a row of its own");
    put_raw(&conn, work_id, 2, 8);
    assert_eq!(
        stored_anchor(&conn, work_id, 2),
        (None, None, None, None),
        "slot 8 is past the last slot (7), so it names no page"
    );

    let (rows, mixed): (i64, i64) = conn
        .query_row(
            "SELECT count(*),
                    count(*) FILTER (WHERE NOT (
                      (at_sha256 IS NULL AND at_nth IS NULL AND at_page IS NULL)
                      OR (at_sha256 IS NOT NULL AND at_nth IS NOT NULL
                          AND at_page IS NOT NULL)))
               FROM progress",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("counts the rows");
    assert_eq!((rows, mixed), (2, 0), "both rows hold all three or none");
}

/// A row whose anchor is part set is refused rather than read as no anchor,
/// as a part-set region is: no write makes one.
#[test]
fn a_position_whose_anchor_is_part_set_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work_of_two_members(&conn);
    put_raw(&conn, work_id, 1, 1);
    conn.execute(
        "UPDATE progress SET at_nth = NULL WHERE work_id = ?1",
        [work_id],
    )
    .expect("part-sets the anchor");

    assert_eq!(
        progress(&conn, work_id, 1),
        Err(format!(
            "the position of user 1 on work {work_id}, edition `raw`, has an anchor with \
             only some of at_sha256, at_nth and at_page set (at_sha256=Some(\"aa\"), \
             at_nth=None, at_page=Some(1), at_half=None); an anchor is all three or none"
        ))
    );
}

/// A half with no page is refused too, in words of its own: none of the
/// three is set, so "only some of" them would be untrue.
#[test]
fn a_position_with_a_half_and_no_page_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work_of_two_members(&conn);
    put_raw(&conn, work_id, 1, 5);
    let changed = conn
        .execute(
            "UPDATE progress SET at_sha256 = NULL, at_nth = NULL, at_page = NULL
              WHERE work_id = ?1 AND at_half = 'left'",
            [work_id],
        )
        .expect("leaves the half alone");
    assert_eq!(changed, 1, "the fixture: slot 5 is a left half");

    assert_eq!(
        progress(&conn, work_id, 1),
        Err(format!(
            "the position of user 1 on work {work_id}, edition `raw`, has a half with no \
             page (at_half=\"left\"); at_half is set only beside at_sha256, at_nth and at_page"
        ))
    );
}

/// `raw`'s position as `state::progress` answers it.
fn raw_slot(conn: &Connection, work_id: i64) -> Vec<(String, u32)> {
    progress(conn, work_id, 1)
        .expect("reads the positions")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect()
}

/// O244's own case: a position on c002's second page (slot 4) stays on that
/// page when c001's directory goes and c002's pages move down three slots.
#[test]
fn a_position_stays_on_its_page_when_a_directory_before_it_goes() {
    let volume = crate::tests::a_volume_across_two_directories();
    put_raw(&volume.conn(), volume.work_id, 1, 4);

    crate::scan::remove_root(&volume.conn(), volume.a.id).expect("removes a");

    assert_eq!(
        raw_slot(&volume.conn(), volume.work_id),
        vec![("raw".to_string(), 1)],
        "c002's second page, now slot 1"
    );
}

/// A position whose file goes falls back to its stored slot, clamped into
/// what is left, and the row is not rewritten: the page comes back with the
/// file. On c002's last page (slot 6), so the stored slot falls out of the
/// three slots c001 leaves.
#[test]
fn a_position_whose_file_goes_is_clamped_and_finds_its_page_when_it_returns() {
    let volume = crate::tests::a_volume_across_two_directories();
    put_raw(&volume.conn(), volume.work_id, 1, 6);

    crate::scan::remove_root(&volume.conn(), volume.b.id).expect("removes b");
    assert_eq!(
        raw_slot(&volume.conn(), volume.work_id),
        vec![("raw".to_string(), 2)],
        "c001's last slot, the clamp"
    );

    volume.put_back(&volume.b);
    assert_eq!(
        raw_slot(&volume.conn(), volume.work_id),
        vec![("raw".to_string(), 6)],
        "c002's last page again"
    );
}

/// Decision 6: a position on a half the layout no longer holds goes to its
/// page. On the left half, second under rtl, so the page's one slot is one
/// below the stored slot and the stored slot is the next page's.
#[test]
fn a_position_on_a_half_goes_to_its_page_when_the_spread_flag_goes() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"hh","pages":4}],
          "spreads":[1]}},
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id =
        crate::import::sidecar(&conn, &doc, Path::new("/x/taiyaku.json")).expect("import");
    put_raw(&conn, work_id, 1, 2);
    assert_eq!(
        stored_anchor(&conn, work_id, 1),
        (Some("hh".into()), Some(0), Some(1), Some("left".into())),
        "the fixture: slot 2 is page 1's left half"
    );

    conn.execute("DELETE FROM page_flag", [])
        .expect("clears the spread flag");

    assert_eq!(
        raw_slot(&conn, work_id),
        vec![("raw".to_string(), 1)],
        "page 1's one slot, not page 2 (the stored slot)"
    );
}

/// Decision 10: a position on an edition key the work no longer has answers
/// its stored slot, with nothing to clamp it into.
#[test]
fn a_position_on_an_edition_the_work_no_longer_has_answers_its_slot() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let two = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "raw":{"label":"R","members":[
            {"path":"raw/v01.cbz","sha256":"hh","pages":10}]},
          "en":{"label":"En","members":[
            {"path":"en/v01.cbz","sha256":"ee","pages":10}]}},
        "hub":"raw","alignments":[]}"#;
    let raw_alone = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"hh","pages":10}]}},
        "hub":"raw","alignments":[]}"#;
    let at = Path::new("/x/taiyaku.json");
    let work_id = crate::import::sidecar(
        &conn,
        &taiyaku_core::Sidecar::from_json(two).expect("parses"),
        at,
    )
    .expect("imports raw and en");
    put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: "en".into(),
            slot: 3,
        },
    )
    .expect("a position on en");

    let again = crate::import::sidecar(
        &conn,
        &taiyaku_core::Sidecar::from_json(raw_alone).expect("parses"),
        at,
    )
    .expect("imports raw alone");
    assert_eq!(again, work_id, "the same work");
    let editions: i64 = conn
        .query_row(
            "SELECT count(*) FROM edition WHERE work_id = ?1 AND key = 'en'",
            [work_id],
            |r| r.get(0),
        )
        .expect("counts");
    assert_eq!(editions, 0, "the fixture: en is gone");

    let held: Vec<(String, u32)> = progress(&conn, work_id, 1)
        .expect("reads")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect();
    assert_eq!(held, vec![("en".to_string(), 3)], "its stored slot");
}

/// Note `id` on `raw`'s `slot`, its words its own.
fn a_raw_note(id: &str, slot: u32) -> Note {
    Note {
        id: id.into(),
        edition: "raw".into(),
        slot,
        region: None,
        color: None,
        body: format!("{id}'s words"),
        created: "2026-11-10T10:00:00Z".into(),
        updated: "2026-11-10T10:00:00Z".into(),
        stale: false,
        at: None,
    }
}

fn hold_notes(conn: &Connection, work_id: i64, notes: Vec<Note>) {
    put_notes(
        conn,
        work_id,
        1,
        &Notes {
            taiyaku_notes: crate::NOTES_VERSION,
            work: "foo".into(),
            notes,
        },
    )
    .expect("writes the notes");
}

/// Each note as `state::notes` answers it: id, edition, slot and `stale`.
fn answered(notes: &Notes) -> Vec<(String, String, u32, bool)> {
    notes
        .notes
        .iter()
        .map(|note| (note.id.clone(), note.edition.clone(), note.slot, note.stale))
        .collect()
}

fn note_slots(conn: &Connection, work_id: i64) -> Vec<(String, String, u32, bool)> {
    answered(&notes(conn, work_id, 1).expect("reads the notes"))
}

fn on(id: &str, edition: &str, slot: u32, stale: bool) -> (String, String, u32, bool) {
    (id.into(), edition.into(), slot, stale)
}

/// Decision 2: a note whose file goes stays at its stored slot, `stale`,
/// and finds its page again when the file returns. On c002's last page (slot
/// 6), past the three slots c001 leaves, so a clamp would show.
#[test]
fn a_note_whose_file_goes_stays_at_its_slot_stale_and_finds_its_page_when_it_returns() {
    let volume = crate::tests::a_volume_across_two_directories();
    hold_notes(&volume.conn(), volume.work_id, vec![a_raw_note("n", 6)]);

    crate::scan::remove_root(&volume.conn(), volume.b.id).expect("removes b");
    assert_eq!(
        note_slots(&volume.conn(), volume.work_id),
        vec![on("n", "raw", 6, true)],
        "its stored slot, not c001's last (2), and stale"
    );

    volume.put_back(&volume.b);
    assert_eq!(
        note_slots(&volume.conn(), volume.work_id),
        vec![on("n", "raw", 6, false)],
        "c002's last page again"
    );
}

/// Decision 3: a rule that moves c001 to another edition leaves a note on its
/// page with its own edition, `stale`; the page is not looked for in the
/// work's other editions.
#[test]
fn a_note_whose_file_moves_to_another_edition_stays_on_its_own_stale() {
    let volume = crate::tests::a_volume_across_two_directories();
    hold_notes(&volume.conn(), volume.work_id, vec![a_raw_note("n", 1)]);
    {
        let conn = volume.conn();
        crate::rules::create(
            &conn,
            &crate::rules::NewRule {
                ordinal: None,
                enabled: true,
                root_id: None,
                kind: crate::rules::RuleKind::Regex,
                pattern: "c001".into(),
                match_on: crate::rules::MatchOn::Path,
                set_edition_key: Some("colour".into()),
                set_kind: None,
                set_language: None,
                set_series: None,
                note: None,
            },
        )
        .expect("a rule sending c001 to colour");
        crate::scan::scan_root(&conn, &volume.a).expect("scans a again");
        crate::derive::derive(&conn).expect("derives with the rule");

        let members: Vec<(i64, String, i64)> = conn
            .prepare(
                "SELECT e.work_id, e.key, count(*) FROM member m
                   JOIN edition e ON e.id = m.edition_id
                  GROUP BY e.work_id, e.key ORDER BY e.key",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .expect("lists the editions");
        assert_eq!(
            members,
            vec![
                (volume.work_id, "colour".to_string(), 1),
                (volume.work_id, "raw".to_string(), 1),
            ],
            "the fixture: one work, c001 on colour and c002 left on raw"
        );
    }

    assert_eq!(
        note_slots(&volume.conn(), volume.work_id),
        vec![on("n", "raw", 1, true)],
        "on raw at its stored slot, stale, and nothing on colour"
    );
}

/// Decision 6: a note on a half the layout no longer holds stays at its
/// stored slot, `stale`. On the left half, second under rtl, so the page's
/// one slot is one below the stored slot.
#[test]
fn a_note_on_a_half_stays_at_its_slot_stale_when_the_spread_flag_goes() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"hh","pages":4}],
          "spreads":[1]}},
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id =
        crate::import::sidecar(&conn, &doc, Path::new("/x/taiyaku.json")).expect("import");
    hold_notes(&conn, work_id, vec![a_raw_note("n", 2)]);
    assert_eq!(
        note_anchor(&conn, "n"),
        (Some("hh".into()), Some(0), Some(1), Some("left".into())),
        "the fixture: slot 2 is page 1's left half"
    );

    conn.execute("DELETE FROM page_flag", [])
        .expect("clears the spread flag");

    assert_eq!(
        note_slots(&conn, work_id),
        vec![on("n", "raw", 2, true)],
        "its stored slot, stale, not page 1's one slot (1)"
    );
}

/// Decision 10: a note on an edition key the work no longer has answers its
/// stored slot, `stale`.
#[test]
fn a_note_on_an_edition_the_work_no_longer_has_is_stale() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let two = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "raw":{"label":"R","members":[
            {"path":"raw/v01.cbz","sha256":"hh","pages":10}]},
          "en":{"label":"En","members":[
            {"path":"en/v01.cbz","sha256":"ee","pages":10}]}},
        "hub":"raw","alignments":[]}"#;
    let raw_alone = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"hh","pages":10}]}},
        "hub":"raw","alignments":[]}"#;
    let at = Path::new("/x/taiyaku.json");
    let work_id = crate::import::sidecar(
        &conn,
        &taiyaku_core::Sidecar::from_json(two).expect("parses"),
        at,
    )
    .expect("imports raw and en");
    hold_notes(
        &conn,
        work_id,
        vec![Note {
            edition: "en".into(),
            ..a_raw_note("n", 3)
        }],
    );
    assert_eq!(
        note_slots(&conn, work_id),
        vec![on("n", "en", 3, false)],
        "the fixture: on en's fourth page"
    );

    crate::import::sidecar(
        &conn,
        &taiyaku_core::Sidecar::from_json(raw_alone).expect("parses"),
        at,
    )
    .expect("imports raw alone");

    assert_eq!(
        note_slots(&conn, work_id),
        vec![on("n", "en", 3, true)],
        "its stored slot, stale"
    );
}

type NoteAnchor = (Option<String>, Option<u32>, Option<u32>, Option<String>);

/// The digest of the member whose file names `chapter`.
fn digest_of(conn: &Connection, chapter: &str) -> String {
    conn.query_row(
        "SELECT sha256 FROM member WHERE path LIKE '%' || ?1 || '.cbz'",
        [chapter],
        |r| r.get(0),
    )
    .expect("the member's digest")
}

/// The four anchor columns of note `id`, read by SQL: the columns themselves
/// are what these tests check.
fn note_anchor(conn: &Connection, id: &str) -> NoteAnchor {
    conn.query_row(
        "SELECT at_sha256, at_nth, at_page, at_half FROM note WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .expect("the note's row")
}

/// Trap 1: saving the document keeps the page of a note nobody moved, so a
/// lost note finds its page again when its file returns. A is removed, not
/// B, so n1's stored slot (1) names one of c002's pages in what is left: an
/// anchor made again from it would name that page instead of c001's.
#[test]
fn saving_the_notes_keeps_the_page_of_a_lost_note_nobody_moved() {
    let volume = crate::tests::a_volume_across_two_directories();
    hold_notes(
        &volume.conn(),
        volume.work_id,
        vec![a_raw_note("n1", 1), a_raw_note("n2", 4)],
    );
    crate::scan::remove_root(&volume.conn(), volume.a.id).expect("removes a");
    assert_eq!(
        note_slots(&volume.conn(), volume.work_id),
        vec![on("n1", "raw", 1, true), on("n2", "raw", 1, false)],
        "the fixture: n1 lost, n2 on c002's second page, now slot 1"
    );

    {
        let conn = volume.conn();
        let mut doc = notes(&conn, volume.work_id, 1).expect("reads the notes");
        doc.notes[1].body = "n2, retyped".into();
        put_notes(&conn, volume.work_id, 1, &doc).expect("writes the notes back");
    }
    volume.put_back(&volume.a);

    assert_eq!(
        note_slots(&volume.conn(), volume.work_id),
        vec![on("n1", "raw", 1, false), on("n2", "raw", 4, false)],
        "n1 on c001's second page again, n2 on c002's"
    );
}

/// A note that moves is anchored to the page it moved to, though its id is
/// one the store holds; and every write leaves a row's first three anchor
/// columns all set or all NULL.
#[test]
fn a_note_moved_to_another_page_names_that_page() {
    let volume = crate::tests::a_volume_across_two_directories();
    let conn = volume.conn();
    hold_notes(
        &conn,
        volume.work_id,
        vec![
            a_raw_note("n1", 1),
            a_raw_note("n2", 4),
            a_raw_note("n3", 9),
        ],
    );
    assert_eq!(
        note_anchor(&conn, "n2"),
        (Some(digest_of(&conn, "c002")), Some(0), Some(1), None),
        "the fixture: n2 names c002's second page"
    );

    hold_notes(
        &conn,
        volume.work_id,
        vec![
            a_raw_note("n1", 1),
            a_raw_note("n2", 6),
            a_raw_note("n3", 9),
        ],
    );

    assert_eq!(
        note_anchor(&conn, "n2"),
        (Some(digest_of(&conn, "c002")), Some(0), Some(3), None),
        "n2 names c002's last page, where it moved"
    );
    assert_eq!(
        note_anchor(&conn, "n3"),
        (None, None, None, None),
        "slot 9 is past the last slot (6), so it names no page"
    );
    let (rows, mixed): (i64, i64) = conn
        .query_row(
            "SELECT count(*),
                    count(*) FILTER (WHERE NOT (
                      (at_sha256 IS NULL AND at_nth IS NULL AND at_page IS NULL)
                      OR (at_sha256 IS NOT NULL AND at_nth IS NOT NULL
                          AND at_page IS NOT NULL)))
               FROM note",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("counts the rows");
    assert_eq!((rows, mixed), (3, 0), "every row holds all three or none");
}

/// `same_anchor` compares slots, so a write reads the notes it holds in the
/// numbering it was handed. Here the server's document (after B went) lacks
/// c002, which the store (B back, A gone) holds: n2 is lost in the document
/// at its stored slot 4 and is on slot 1 in the store.
#[test]
fn a_write_reads_the_notes_it_holds_in_the_numbering_it_was_handed() {
    let volume = crate::tests::a_volume_across_two_directories();
    hold_notes(
        &volume.conn(),
        volume.work_id,
        vec![a_raw_note("n1", 0), a_raw_note("n2", 4)],
    );
    crate::scan::remove_root(&volume.conn(), volume.b.id).expect("removes b");
    let sent = crate::project::sidecar(&volume.conn(), volume.work_id).expect("the document");
    volume.put_back(&volume.b);
    crate::scan::remove_root(&volume.conn(), volume.a.id).expect("removes a");

    let conn = volume.conn();
    let mut doc =
        notes_in(&conn, volume.work_id, 1, Numbering::Document(&sent)).expect("reads the notes");
    assert_eq!(
        answered(&doc),
        vec![on("n1", "raw", 0, false), on("n2", "raw", 4, true)],
        "the fixture: in the document sent, n2 is lost at its stored slot"
    );
    assert_eq!(
        note_slots(&conn, volume.work_id),
        vec![on("n1", "raw", 0, true), on("n2", "raw", 1, false)],
        "the fixture: in the store, n2 is on c002's second page, slot 1"
    );

    doc.notes[0].body = "n1, retyped".into();
    put_notes_in(&conn, volume.work_id, 1, &doc, Numbering::Document(&sent))
        .expect("writes the notes back");

    assert_eq!(
        note_slots(&conn, volume.work_id),
        vec![on("n1", "raw", 0, true), on("n2", "raw", 1, false)],
        "n2 still on c002's second page"
    );
}

/// A note whose anchor is part set is refused, naming the note, as a
/// part-set region is: no write makes one.
#[test]
fn a_note_whose_anchor_is_part_set_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work_of_two_members(&conn);
    hold_notes(&conn, work_id, vec![a_raw_note("n", 1)]);
    conn.execute("UPDATE note SET at_page = NULL", [])
        .expect("part-sets the anchor");

    assert_eq!(
        notes(&conn, work_id, 1),
        Err(
            "note `n` has an anchor with only some of at_sha256, at_nth and at_page set \
             (at_sha256=Some(\"aa\"), at_nth=Some(0), at_page=None, at_half=None); an anchor \
             is all three or none"
                .to_string()
        )
    );
}

/// Stamp both of the volume's archives, as the metadata editor's write to
/// files does: each archive's digest moves and its pages stay byte for byte.
fn stamp_the_volume(volume: &crate::tests::VolumeAcrossTwoDirectories) {
    let conn = volume.conn();
    let planned = crate::stamp::plan(&conn, volume.work_id).expect("plans the stamp");
    assert_eq!(
        planned.len(),
        2,
        "the fixture: the stamp rewrites c001 and c002"
    );
    crate::stamp::apply(&conn, volume.work_id, 1, &planned).expect("stamps");
}

/// A stamp remembers each archive's digest before it and after it, on the
/// work and edition it moved the anchors on, so a save from a tab that read
/// before the stamp can be forwarded (O240); a digest it moved to is no alias.
#[test]
fn a_stamp_remembers_each_digest_it_moved() {
    let volume = crate::tests::a_volume_across_two_directories();
    let was: Vec<String> = ["c001", "c002"]
        .iter()
        .map(|chapter| digest_of(&volume.conn(), chapter))
        .collect();

    stamp_the_volume(&volume);

    let conn = volume.conn();
    for (chapter, was) in ["c001", "c002"].iter().zip(&was) {
        let now = digest_of(&conn, chapter);
        assert_ne!(&now, was, "the fixture: the stamp moved {chapter}'s digest");
        assert_eq!(
            crate::stamp::alias_of(&conn, volume.work_id, "raw", was, 0),
            Ok(Some((now, 0))),
            "{chapter}'s digest before the stamp names its digest after it"
        );
    }
    assert_eq!(
        crate::stamp::alias_of(&conn, volume.work_id, "raw", &digest_of(&conn, "c001"), 0),
        Ok(None),
        "a digest no stamp moved names nothing"
    );
}

/// Both directories scanned again and the library derived, which records
/// each stamped archive's new digest.
fn rescan_the_volume(volume: &crate::tests::VolumeAcrossTwoDirectories) {
    let conn = volume.conn();
    crate::scan::scan_root(&conn, &volume.a).expect("scans a again");
    crate::scan::scan_root(&conn, &volume.b).expect("scans b again");
    crate::derive::derive(&conn).expect("derives");
}

/// The notes as `note_slots` answers them, and the positions as `raw_slot` does.
type NotesAndPositions = (Vec<(String, String, u32, bool)>, Vec<(String, u32)>);

/// A note and a position on c002's second page (slot 4), and A removed after
/// `before_a_goes`: what `state::` answers for them then.
fn after_a_stamp_and_a_removal(
    before_a_goes: impl FnOnce(&crate::tests::VolumeAcrossTwoDirectories),
) -> NotesAndPositions {
    let volume = crate::tests::a_volume_across_two_directories();
    put_raw(&volume.conn(), volume.work_id, 1, 4);
    hold_notes(&volume.conn(), volume.work_id, vec![a_raw_note("n1", 4)]);
    before_a_goes(&volume);
    crate::scan::remove_root(&volume.conn(), volume.a.id).expect("removes a");
    let conn = volume.conn();
    (
        note_slots(&conn, volume.work_id),
        raw_slot(&conn, volume.work_id),
    )
}

/// O244's own case after a stamp: the stamp gives c002 a new digest and
/// keeps its pages, so a note and a position on its second page still name
/// that page, and stay on it when A goes.
#[test]
fn a_stamp_keeps_a_note_and_a_position_on_their_page() {
    let answered = after_a_stamp_and_a_removal(|volume| {
        stamp_the_volume(volume);
        assert_eq!(
            note_slots(&volume.conn(), volume.work_id),
            vec![on("n1", "raw", 4, false)],
            "the stamped note still names its page"
        );
        rescan_the_volume(volume);
    });

    assert_eq!(
        answered,
        (
            vec![on("n1", "raw", 1, false)],
            vec![("raw".to_string(), 1)]
        ),
        "c002's second page, now slot 1"
    );
}

/// A derive before the next scan writes c002's old digest back onto its
/// member, from the scan's entry; the scan then records the new one, and the
/// rows the stamp moved to it name their page again.
#[test]
fn a_derive_between_a_stamp_and_the_next_scan_leaves_the_rows_on_their_page() {
    let answered = after_a_stamp_and_a_removal(|volume| {
        let unstamped = digest_of(&volume.conn(), "c002");
        stamp_the_volume(volume);
        crate::derive::derive(&volume.conn()).expect("derives before the next scan");
        assert_eq!(
            digest_of(&volume.conn(), "c002"),
            unstamped,
            "the fixture: the derive put c002's old digest back"
        );
        rescan_the_volume(volume);
    });

    assert_eq!(
        answered,
        (
            vec![on("n1", "raw", 1, false)],
            vec![("raw".to_string(), 1)]
        ),
        "c002's second page, now slot 1"
    );
}

/// The volume with a note `n` and a position on c002's second page (slot 4),
/// then the directory `gone` picks removed; beside it the page both rows were
/// written with, read before the directory went.
fn on_c002s_second_page_then_without(
    gone: fn(&crate::tests::VolumeAcrossTwoDirectories) -> i64,
) -> (crate::tests::VolumeAcrossTwoDirectories, MemberPage) {
    let volume = crate::tests::a_volume_across_two_directories();
    put_raw(&volume.conn(), volume.work_id, 1, 4);
    hold_notes(&volume.conn(), volume.work_id, vec![a_raw_note("n", 4)]);
    let page = an_anchor(&digest_of(&volume.conn(), "c002"), 0, 1, None);
    crate::scan::remove_root(&volume.conn(), gone(&volume)).expect("removes the directory");
    (volume, page)
}

type NoteAt = (u32, bool, Option<MemberPage>);

/// One of `state::`'s reads of one reader's rows on one work.
type Read<T> = fn(&Connection, i64, i64, Numbering<'_>) -> Result<T, String>;

/// Each note's slot, `stale` and `at` as `read` answers them.
fn notes_at(volume: &crate::tests::VolumeAcrossTwoDirectories, read: Read<Notes>) -> Vec<NoteAt> {
    read(&volume.conn(), volume.work_id, 1, Numbering::Store)
        .expect("reads the notes")
        .notes
        .into_iter()
        .map(|note| (note.slot, note.stale, note.at))
        .collect()
}

/// Each position's edition, slot and `at` as `read` answers them.
fn positions_at(
    volume: &crate::tests::VolumeAcrossTwoDirectories,
    read: Read<Vec<Position>>,
) -> Vec<(String, u32, Option<MemberPage>)> {
    read(&volume.conn(), volume.work_id, 1, Numbering::Store)
        .expect("reads the positions")
        .into_iter()
        .map(|position| (position.edition, position.slot, position.at))
        .collect()
}

/// The anchored read answers a note's page as stored, found or not (O240).
/// With B gone the note is stale at slot 4, past c001's three slots, so no
/// page derived from its slot could be the one answered.
#[test]
fn the_anchored_notes_read_answers_each_notes_stored_page() {
    let (volume, page) = on_c002s_second_page_then_without(|volume| volume.a.id);
    assert_eq!(
        notes_at(&volume, notes_anchored_in),
        vec![(1, false, Some(page))],
        "c002's second page, now slot 1"
    );

    let (volume, page) = on_c002s_second_page_then_without(|volume| volume.b.id);
    assert_eq!(
        notes_at(&volume, notes_anchored_in),
        vec![(4, true, Some(page))],
        "stale at its stored slot, its page as stored"
    );
}

/// The anchored read answers a position's page as stored, found or not
/// (O240). With B gone the position is clamped to c001's last slot (2),
/// whose own page is c001's, not the one answered.
#[test]
fn the_anchored_progress_read_answers_each_positions_stored_page() {
    let (volume, page) = on_c002s_second_page_then_without(|volume| volume.a.id);
    assert_eq!(
        positions_at(&volume, progress_anchored_in),
        vec![("raw".to_string(), 1, Some(page))],
        "c002's second page, now slot 1"
    );

    let (volume, page) = on_c002s_second_page_then_without(|volume| volume.b.id);
    assert_eq!(
        positions_at(&volume, progress_anchored_in),
        vec![("raw".to_string(), 2, Some(page))],
        "clamped to c001's last slot, its page as stored"
    );
}

/// The plain reads answer no `at` for the same rows: an export and every
/// other caller but serve read through them (O240).
#[test]
fn the_plain_reads_answer_no_page() {
    let (volume, _) = on_c002s_second_page_then_without(|volume| volume.a.id);
    assert_eq!(notes_at(&volume, notes_in), vec![(1, false, None)]);
    assert_eq!(
        positions_at(&volume, progress_in),
        vec![("raw".to_string(), 1, None)]
    );

    let (volume, _) = on_c002s_second_page_then_without(|volume| volume.b.id);
    assert_eq!(notes_at(&volume, notes_in), vec![(4, true, None)]);
    assert_eq!(
        positions_at(&volume, progress_in),
        vec![("raw".to_string(), 2, None)]
    );
}
