use std::collections::BTreeMap;

use rusqlite::{Connection, params};
use taiyaku_core::{Kind, RegionMember, TruthDoc, TruthEdition, TruthRegion};

use super::{Moved, Outcome, absorb};
use crate::state::{progress, put_notes, put_progress, put_truth};
use crate::{Note, Notes, Progress, Region};

/// A migrated database **with foreign keys actually on**, asserted rather
/// than assumed.
///
/// **`schema::migrate` restores the value it found rather than leaving them
/// on** — `schema::without_foreign_keys` turns them off for the run and puts
/// back what was there — and a bare `Connection` starts with them off, which
/// is why `Library::open_in_memory` sets the pragma and this fixture has to
/// as well. Every claim this module makes is a claim about a cascade, and
/// with the pragma off there is no cascade to make a claim about.
///
/// With the pragma off these tests do not pass silently: they fail for a
/// reason that has nothing to do with what they assert, and
/// `truth_page_refuses_to_be_moved_by_update` — the one that pins why
/// `move_truth` is not an `UPDATE` — fails outright, the composite key being
/// unenforced. The assertion is here to name that reason at the fixture.
fn open() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("enabling foreign keys");
    let on: bool = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("reading the foreign_keys pragma back");
    assert!(
        on,
        "every assertion below is about what `work`'s ON DELETE CASCADE does \
         or does not take; with foreign keys off there is no cascade at all"
    );
    conn
}

/// One `work`, one `edition` per key, one `member` per digest in the order
/// given.
///
/// **`member.root_id` is left NULL, deliberately.** Root membership is what
/// `derive::superseded` decides a candidate on, and nothing in this module
/// reads it: [`absorb`] is handed the pair that decision already produced.
/// Giving the fixture a root would suggest otherwise.
fn seed_work(conn: &Connection, work_id: i64, name: &str, editions: &[(&str, &[&str])]) {
    let hub = editions.first().map_or("raw", |(key, _)| *key);
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (?1, ?2, ?2, ?2, 'rtl', ?3)",
        params![work_id, name, hub],
    )
    .expect("seeding a work");
    for (key, digests) in editions {
        conn.execute(
            "INSERT INTO edition (work_id, key, label) VALUES (?1, ?2, ?2)",
            params![work_id, key],
        )
        .expect("seeding an edition");
        let edition_id = conn.last_insert_rowid();
        for (ordinal, sha256) in (0_i64..).zip(*digests) {
            conn.execute(
                "INSERT INTO member (edition_id, ordinal, path, sha256, pages)
                 VALUES (?1, ?2, ?3, ?4, 1)",
                params![edition_id, ordinal, format!("archive/{sha256}"), sha256],
            )
            .expect("seeding a member");
        }
    }
}

/// The shape O73's own reproduction leaves behind: work 1 (the target) holds
/// its own `raw` archive and has just gained the `full-color` one, and work 2
/// still stands over that same `full-color` archive under its own `raw` key.
///
/// The two editions holding `bbb` are the byte-identical pair — the same
/// archive, classified twice — so slots, page indices and box coordinates
/// transfer one for one and `raw` on work 2 maps to `full-color` on work 1.
fn folded_pair() -> (Connection, i64, i64) {
    let conn = open();
    seed_work(
        &conn,
        1,
        "target",
        &[("raw", &["aaa"]), ("full-color", &["bbb"])],
    );
    seed_work(&conn, 2, "superseded", &[("raw", &["bbb"])]);
    (conn, 2, 1)
}

/// [`absorb`], with the refusal arm turned into a panic naming its reason —
/// the tests below that expect a fold want the `Moved`, and one that expects
/// a refusal asserts on [`Outcome::Refused`] itself.
fn folded(conn: &Connection, from: i64, into: i64) -> Moved {
    match absorb(conn, from, into).expect("folding") {
        Outcome::Folded(moved) => moved,
        Outcome::Refused(why) => panic!("the fold was refused: {why}"),
    }
}

fn seed_note(conn: &Connection, work_id: i64, edition: &str, slot: u32) {
    put_notes(
        conn,
        work_id,
        1,
        &Notes {
            taiyaku_notes: crate::NOTES_VERSION,
            work: "ignored".to_string(),
            notes: vec![Note {
                id: "01JEXAMPLE0000000000000000".to_string(),
                edition: edition.to_string(),
                slot,
                region: None,
                color: None,
                body: "the sign in the background".to_string(),
                created: "2026-08-27T09:00:00Z".to_string(),
                updated: "2026-08-27T09:00:00Z".to_string(),
                stale: false,
                at: None,
            }],
        },
    )
    .expect("the reader's note on the superseded work");
}

/// One looked-at page with one box on it and **no record at all of what it
/// was drawn against** — `"members": []` with populated `pages`.
///
/// Written through `put_truth` for [`seed_truth`]'s reason, and reachable for
/// the same one: `state::put_truth` writes the document's `members` as given
/// and `routes::put_truth` validates neither the edition key nor the member
/// list, so any client sending this shape produces these rows. `seed_truth`
/// always writes exactly one member.
fn seed_truth_without_provenance(conn: &Connection, work_id: i64, edition: &str, page: u32) {
    let mut doc = TruthDoc::new();
    doc.editions.insert(
        edition.to_string(),
        TruthEdition {
            members: Vec::new(),
            pages: BTreeMap::from([(
                page,
                vec![TruthRegion {
                    x: 0.1,
                    y: 0.2,
                    w: 0.3,
                    h: 0.4,
                    axis: None,
                    kind: Kind::Dialogue,
                    ignore: false,
                    text: None,
                }],
            )]),
        },
    );
    put_truth(conn, work_id, &doc).expect("boxes with nothing recorded behind them");
}

/// One looked-at page with one box on it, and the digest it was drawn
/// against — written through `put_truth` rather than by `INSERT`, so the row
/// shape is the one the labelling client actually produces.
fn seed_truth(conn: &Connection, work_id: i64, edition: &str, page: u32, sha256: &str) {
    let mut doc = TruthDoc::new();
    doc.editions.insert(
        edition.to_string(),
        TruthEdition {
            members: vec![RegionMember {
                sha256: sha256.to_string(),
            }],
            pages: BTreeMap::from([(
                page,
                vec![TruthRegion {
                    x: 0.1,
                    y: 0.2,
                    w: 0.3,
                    h: 0.4,
                    axis: None,
                    kind: Kind::Dialogue,
                    ignore: false,
                    text: None,
                }],
            )]),
        },
    );
    put_truth(conn, work_id, &doc).expect("the labelling on the superseded work");
}

/// The whole point: a reader's place follows the material.
#[test]
fn a_position_moves_onto_the_edition_holding_the_same_archive() {
    let (conn, from, into) = folded_pair();
    put_progress(
        &conn,
        from,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 33,
        },
    )
    .expect("the reader's place on the superseded work");
    put_progress(
        &conn,
        into,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 11,
        },
    )
    .expect("and a place on the target's own edition");

    let moved = folded(&conn, from, into);

    let mut held: Vec<(String, u32)> = progress(&conn, into, 1)
        .expect("reading back")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect();
    held.sort();
    assert_eq!(
        held,
        vec![("full-color".to_string(), 33), ("raw".to_string(), 11)],
        "both places stand, the moved one under the edition that holds its archive"
    );
    assert_eq!(
        (moved.positions, moved.lost_positions),
        (1, 0),
        "the move is counted, and the two places are on two different keys so \
         nothing stopped existing — this specific fixture cannot catch the \
         `lost_positions` wrong-work mutation, but \
         `two_source_editions_landing_on_one_target_edition_count_the_position_that_stops_existing` \
         and the end-to-end derive fold test do"
    );
}

/// `completed_at` and `read_count` come across: they are facts about reading
/// this rendition, and the rendition is what moved.
///
/// Neither is written by `put_progress` — its own doc comment is explicit
/// that it leaves both alone — so they are set here the way a finished re-read
/// leaves them, and the assertion is that the fold did not quietly reset a
/// reading history the schema calls "behavioural data with NOTHING to
/// backfill".
#[test]
fn a_moved_position_keeps_the_reading_history_hanging_off_it() {
    let (conn, from, into) = folded_pair();
    put_progress(
        &conn,
        from,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 33,
        },
    )
    .expect("a place");
    conn.execute(
        "UPDATE progress SET completed_at = '2026-01-02T03:04:05Z', read_count = 3
          WHERE work_id = ?1",
        [from],
    )
    .expect("finished once and now re-reading");

    folded(&conn, from, into);

    let (completed, count): (Option<String>, i64) = conn
        .query_row(
            "SELECT completed_at, read_count FROM progress WHERE work_id = ?1",
            [into],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the moved position");
    assert_eq!(
        (completed.as_deref(), count),
        (Some("2026-01-02T03:04:05Z"), 3),
        "the re-read count and the finish date moved with the position"
    );
}

/// Both works hold a position on the mapped edition, the newer `updated`
/// wins, and the row it replaces is counted rather than silently dropped.
///
/// **The conflict is reachable** — [`super::edition_map`] is non-injective and
/// `progress.edition` has no foreign key to `edition(key)`.
///
/// **The winner wins entire, `completed_at` and `read_count` included.** The
/// two histories here are deliberately different, so a row taking one
/// reading's place with the other's re-read count would fail.
///
/// **And the row it wins against is a loss.** The target's older place, its
/// finish date and its re-read count are overwritten and gone.
/// `lost_positions` is a whole-fold identity precisely so that the side the
/// row was on stops mattering.
#[test]
fn where_both_works_hold_the_same_edition_the_newer_position_wins() {
    let (conn, from, into) = folded_pair();
    put_progress(
        &conn,
        from,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 33,
        },
    )
    .expect("a place on the superseded work");
    conn.execute(
        "UPDATE progress SET completed_at = '2026-01-02T03:04:05Z', read_count = 3
          WHERE work_id = ?1",
        [from],
    )
    .expect("finished once and now re-reading the superseded work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at, read_count)
         VALUES (1, ?1, 'full-color', 4, '2000-01-01T00:00:00Z', '1999-12-31T00:00:00Z', 9)",
        [into],
    )
    .expect("an older place, with its own history, already on the target's mapped edition");

    let moved = folded(&conn, from, into);
    assert_eq!(
        (moved.positions, moved.lost_positions),
        (1, 1),
        "the source's row is the newer, so it lands — over a position that \
         existed before this fold and does not exist after it"
    );

    let (slot, completed, count): (u32, Option<String>, i64) = conn
        .query_row(
            "SELECT slot, completed_at, read_count FROM progress
              WHERE work_id = ?1 AND edition = 'full-color'",
            [into],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the surviving position");
    assert_eq!(slot, 33, "the newer of the two positions stands");
    assert_eq!(
        (completed.as_deref(), count),
        (Some("2026-01-02T03:04:05Z"), 3),
        "and its own history stands with it, not the older row's"
    );
}

/// The edition string is rewritten, and this is the test that proves it.
///
/// Left as `raw`, a note about a colour page lands on the monochrome archive:
/// same slot number, different book, and accepted by every constraint in the
/// schema. Nothing but this assertion catches it.
#[test]
fn a_moved_note_names_the_edition_that_holds_its_archive() {
    let (conn, from, into) = folded_pair();
    seed_note(&conn, from, "raw", 33);

    let moved = folded(&conn, from, into);

    let edition: String = conn
        .query_row("SELECT edition FROM note WHERE work_id = ?1", [into], |r| {
            r.get(0)
        })
        .expect("the moved note");
    assert_eq!(
        edition, "full-color",
        "a note kept on `raw` is on the wrong book"
    );
    assert_eq!(moved.notes, 1, "and the move is counted");
}

/// `truth_page` cannot be moved by UPDATE — `truth_box`'s composite foreign
/// key has no ON UPDATE — so this is delete-and-reinsert, page rows first.
///
/// **The provenance digest is `ddd`, distinct from every `member.sha256` in
/// [`folded_pair`] (`aaa`, `bbb`).** A `bbb` here would be indistinguishable
/// from the archive's own current digest, so a [`super::move_truth`] that
/// read the digest off `member` instead of `truth_member` — the archives as
/// they are *now* rather than what the boxes were drawn against — would read
/// `bbb` right back and pass this test unchanged. `edition_map` compares
/// `member.sha256`; `truth_member` is the separate, client-supplied fact that
/// `TruthEdition::is_stale_for` exists to compare it against, and a fixture
/// that cannot tell the two apart proves nothing about which one `move_truth`
/// actually reads.
#[test]
fn truth_moves_with_its_boxes_and_its_digests() {
    let (conn, from, into) = folded_pair();
    seed_truth(&conn, from, "raw", 33, "ddd");

    let moved = folded(&conn, from, into);

    let (edition, page): (String, u32) = conn
        .query_row(
            "SELECT edition, page FROM truth_box WHERE work_id = ?1",
            [into],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the moved box");
    assert_eq!((edition.as_str(), page), ("full-color", 33));
    let digest: String = conn
        .query_row(
            "SELECT sha256 FROM truth_member WHERE work_id = ?1",
            [into],
            |r| r.get(0),
        )
        .expect("the moved digest");
    assert_eq!(
        digest, "ddd",
        "the labelling still names the archive it was drawn on, not the \
         archive's own current digest"
    );
    assert_eq!(
        (moved.truth_pages, moved.collisions),
        (1, 0),
        "one page moved and nothing was left behind"
    );
}

/// Why [`super::move_truth`] is delete-and-reinsert rather than the one-line
/// `UPDATE` that reads as obviously equivalent.
///
/// **Pinned here so a later refactor meets it as a test rather than as a
/// production failure.** `truth_box`'s foreign key names
/// `(work_id, edition, page)` with no `ON UPDATE`, so moving the page row out
/// from under a box is refused outright — with the edition rewritten or
/// without it, since the box's own `edition` moves with neither.
#[test]
fn truth_page_refuses_to_be_moved_by_update() {
    let (conn, from, into) = folded_pair();
    seed_truth(&conn, from, "raw", 33, "bbb");

    for statement in [
        "UPDATE truth_page SET work_id = ?2 WHERE work_id = ?1",
        "UPDATE truth_page SET work_id = ?2, edition = 'full-color' WHERE work_id = ?1",
    ] {
        let refused = conn.execute(statement, params![from, into]);
        assert!(
            refused.is_err(),
            "`{statement}` was accepted; the composite foreign key that makes \
             `move_truth` a delete-and-reinsert is gone"
        );
    }
}

/// Truth is library-wide with no `user_id`, so "the target already holds this
/// page" means somebody has labelled it, and their work is not this fold's to
/// discard.
///
/// The page stays theirs, the source's boxes are **not** merged in underneath
/// it — that would be two labellers' boxes on one page, a different decision
/// from "the target's labelling wins" — and the loss is counted rather than
/// silent.
///
/// **The source's colliding page carries two boxes, not one**, because
/// [`Moved::collisions`]'s own doc says the unit is a page: "a target page
/// carrying one box against a source page carrying forty reads `1` here".
/// Every other truth fixture in this file writes exactly one `TruthRegion`
/// per page, so a mutation changing `move_truth`'s collision count from pages
/// to boxes would leave every one of them green; this is the one place that
/// distinction is checked.
#[test]
fn a_truth_page_the_target_already_holds_stays_the_targets() {
    let (conn, from, into) = folded_pair();
    seed_truth(&conn, from, "raw", 33, "bbb");
    conn.execute(
        "INSERT INTO truth_box (work_id, edition, page, x, y, w, h, kind)
         VALUES (?1, 'raw', 33, 0.5, 0.5, 0.1, 0.1, 'sfx')",
        [from],
    )
    .expect("a second box on the source's same colliding page");
    seed_truth(&conn, into, "full-color", 33, "bbb");
    conn.execute(
        "UPDATE truth_box SET kind = 'sfx' WHERE work_id = ?1",
        [into],
    )
    .expect("marking the target's own box apart from the source's");

    let moved = folded(&conn, from, into);

    let kinds: Vec<String> = conn
        .prepare("SELECT kind FROM truth_box WHERE work_id = ?1 ORDER BY id")
        .and_then(|mut s| {
            s.query_map([into], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<String>>>()
        })
        .expect("reading the boxes back");
    assert_eq!(
        kinds,
        vec!["sfx".to_string()],
        "the target's own box stands alone; the source's were not merged in"
    );
    assert_eq!(
        (moved.truth_pages, moved.collisions),
        (0, 1),
        "nothing moved and the page the target already held is counted once, \
         not once per box the source's colliding page carries"
    );
}

/// The delete is what closes O73, and it runs last.
#[test]
fn the_superseded_work_is_gone_and_took_nothing_with_it() {
    let (conn, from, into) = folded_pair();
    put_progress(
        &conn,
        from,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 33,
        },
    )
    .expect("a place");
    seed_note(&conn, from, "raw", 33);
    seed_truth(&conn, from, "raw", 33, "bbb");

    folded(&conn, from, into);

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE id = ?1", [from], |r| {
            r.get(0)
        })
        .expect("counting");
    assert_eq!(works, 0, "the superseded work is deleted");
    let counted = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .expect("counting")
    };
    assert_eq!(
        (
            counted("progress"),
            counted("note"),
            counted("truth_page"),
            counted("truth_box"),
            counted("truth_member")
        ),
        (1, 1, 1, 1, 1),
        "the cascade took nothing: every row moved before the delete ran"
    );
    let stranded: i64 = conn
        .query_row(
            "SELECT (SELECT count(*) FROM progress WHERE work_id = ?1)
                  + (SELECT count(*) FROM note WHERE work_id = ?1)
                  + (SELECT count(*) FROM truth_page WHERE work_id = ?1)",
            [from],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(stranded, 0, "and nothing still names the deleted work");
}

/// `derive::superseded` guarantees that every digest of the candidate is held
/// by the target. It does **not** guarantee that the candidate's editions
/// line up with the target's one for one: a candidate holding `[X, Y]` as one
/// edition against a target holding `[X]` and `[Y]` as two is covered by that
/// rule and mappable by nothing.
///
/// **The work is left standing and the scan carries on.** A scan that died
/// because one work could not be folded is worse than a scan that folds
/// nothing, and a half-moved work is worse than either.
#[test]
fn an_edition_with_no_digest_identical_counterpart_leaves_the_work_standing() {
    let conn = open();
    seed_work(
        &conn,
        1,
        "target",
        &[("raw", &["xxx"]), ("colour", &["yyy"])],
    );
    seed_work(&conn, 2, "superseded", &[("raw", &["xxx", "yyy"])]);
    put_progress(
        &conn,
        2,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 33,
        },
    )
    .expect("a place on the work that cannot be folded");

    let outcome = absorb(&conn, 2, 1).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => panic!("nothing was mappable, yet it folded: {moved:?}"),
        // **The arm's own phrase, not just the key.** The position seeded
        // above sits on the same unmapped key, so `raw` is both an edition
        // `edition_map` could not map and a stranded-row key: delete this
        // arm from `absorb` and the stranded arm answers with a message that
        // also names `raw`. `raw` is a substring of `drawn` besides, which
        // the truth-provenance arm's message contains. Only the reason
        // separates them.
        Outcome::Refused(why) => {
            assert!(
                why.contains("with no digest-identical edition on work 1"),
                "the refusal must be the unmappable-edition one, not another \
                 arm that happens to mention the same key: {why}"
            );
            assert!(
                why.contains("`raw`"),
                "and it must name the edition it could not map: {why}"
            );
        }
    }
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE id = 2", [], |r| r.get(0))
        .expect("counting");
    assert_eq!(works, 1, "the superseded work still stands");
    let (work_id, slot): (i64, u32) = conn
        .query_row("SELECT work_id, slot FROM progress", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the position");
    assert_eq!(
        (work_id, slot),
        (2, 33),
        "and the reader's place was not moved half way"
    );
}

/// An edition the target holds under a *different* order of the same digests
/// is not the same rendition, and mapping it would send every slot to the
/// wrong page.
///
/// **The same unmappable-edition refusal as
/// [`an_edition_with_no_digest_identical_counterpart_leaves_the_work_standing`],
/// reached by a reversed-order fixture instead of a missing one**, so the
/// assertions mirror that test's shape rather than a bare
/// `Outcome::Refused(_) => {}`: the refusal must be *this* refusal, naming the
/// edition it could not map, and the work — with the reader's place on it —
/// must still be standing afterwards. Matching only on the variant would also
/// pass a `move_truth` or `shared_note_ids` mistake that refused for an
/// unrelated reason, or one that refused and then moved something anyway.
#[test]
fn the_same_digests_in_a_different_order_are_not_a_counterpart() {
    let conn = open();
    seed_work(&conn, 1, "target", &[("colour", &["yyy", "xxx"])]);
    seed_work(&conn, 2, "superseded", &[("raw", &["xxx", "yyy"])]);
    put_progress(
        &conn,
        2,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 33,
        },
    )
    .expect("a place on the work that cannot be folded");

    let outcome = absorb(&conn, 2, 1).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => panic!("the order was reversed, yet it folded: {moved:?}"),
        Outcome::Refused(why) => {
            assert!(
                why.contains("with no digest-identical edition on work 1"),
                "the refusal must be the unmappable-edition one, not another \
                 arm that happens to refuse for some other reason: {why}"
            );
            assert!(
                why.contains("`raw`"),
                "and it must name the edition it could not map: {why}"
            );
        }
    }
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE id = 2", [], |r| r.get(0))
        .expect("counting");
    assert_eq!(works, 1, "the superseded work still stands");
    let (work_id, slot): (i64, u32) = conn
        .query_row("SELECT work_id, slot FROM progress", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the position");
    assert_eq!(
        (work_id, slot),
        (2, 33),
        "and the reader's place was not moved half way"
    );
}

/// What the final `DELETE` takes that the fold does not move, counted so the
/// scan's summary can say it happened.
///
/// **These are the library's description of the material, not the reader's
/// state**, and each would need its own merge rule against what the target
/// already holds — `credit` has no unique constraint so a move duplicates,
/// `collection_item`'s key would collide, `edition.label` would overwrite
/// what this very derive just wrote. Choosing seven merge rules inside a
/// function that deletes a work is more risk than the loss it avoids, and
/// every one of them is visible and re-doable by hand where a reading
/// position is not. So they are counted rather than moved, and this test is
/// what keeps the number honest.
///
/// The alignment here names an edition, `en`, that the folded work does not
/// hold, so nothing can carry it and it is counted where an alignment left
/// behind is counted, in `uncarried_alignments` — and the spread flag beside
/// the credit flag is written onto the twin rather than counted, where the
/// credit flag is.
#[test]
fn what_the_delete_takes_that_the_fold_does_not_move_is_counted() {
    let (conn, from, into) = folded_pair();
    conn.execute(
        "INSERT INTO person (id, name) VALUES (1, 'a scanlator')",
        [],
    )
    .expect("a person");
    conn.execute("INSERT INTO tag (id, name) VALUES (1, 'seinen')", [])
        .expect("a tag");
    conn.execute(
        "INSERT INTO credit (person_id, role, work_id) VALUES (1, 'translator', ?1)",
        [from],
    )
    .expect("a credit on the work");
    conn.execute(
        "INSERT INTO credit (person_id, role, edition_id)
         SELECT 1, 'group', id FROM edition WHERE work_id = ?1",
        [from],
    )
    .expect("a credit on the edition");
    conn.execute(
        "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, 'raw', 'en', 'hand')",
        [from],
    )
    .expect("a hand alignment");
    let alignment_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO run (alignment_id, ordinal, a_start_page, a_end_page)
         VALUES (?1, 0, 0, 1)",
        [alignment_id],
    )
    .expect("a hand-corrected run");
    conn.execute(
        "INSERT INTO work_tag (work_id, tag_id) VALUES (?1, 1)",
        [from],
    )
    .expect("a tag on the work");
    conn.execute(
        "INSERT INTO collection (id, name) VALUES (1, 'to re-read')",
        [],
    )
    .expect("a collection");
    conn.execute(
        "INSERT INTO collection_item (collection_id, ordinal, work_id) VALUES (1, 0, ?1)",
        [from],
    )
    .expect("a place in it");
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'credit', 7 FROM edition WHERE work_id = ?1",
        [from],
    )
    .expect("a credit page flag");
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'spread', 0 FROM edition WHERE work_id = ?1",
        [from],
    )
    .expect("a spread flag, which the twin takes");
    conn.execute(
        "UPDATE member SET chapter_declared = 4.5, chapter_source = 'hand'
          WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
        [from],
    )
    .expect("a hand-entered chapter number");
    conn.execute(
        "UPDATE edition SET label = 'Japanese raw', language = 'ja', kind = 'raw'
          WHERE work_id = ?1",
        [from],
    )
    .expect("a hand-classified edition");

    let moved = folded(&conn, from, into);

    assert_eq!(
        (moved.discarded, moved.uncarried_alignments),
        (7, 1),
        "one credit on the work, one on the edition, one tag, one collection \
         place, one credit page flag, one hand chapter number and one \
         hand-classified edition — each counted, none moved — and the \
         alignment naming an edition the work does not hold, left behind"
    );
    let spreads: Vec<i64> = conn
        .prepare(
            "SELECT page FROM page_flag JOIN edition ON edition.id = page_flag.edition_id
              WHERE edition.work_id = ?1 AND edition.key = 'full-color' AND page_flag.kind = 'spread'",
        )
        .and_then(|mut s| {
            s.query_map([into], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()
        })
        .expect("the twin's spread flags");
    assert_eq!(
        spreads,
        vec![0],
        "the spread flag went to the twin, not to the count"
    );
    let left: i64 = conn
        .query_row("SELECT count(*) FROM credit", [], |r| r.get(0))
        .expect("counting");
    assert_eq!(left, 0, "and the delete really did take them");
}

/// A work carrying nothing but its own editions and members discards nothing:
/// the duplicate rows the fold exists to remove are not a loss.
///
/// **A chapter number and a label the *scanner* wrote are part of that**, and
/// they are here rather than left out because they are what separates two of
/// the predicates in [`super::DISCARDED`] from a plain `work_id`.
/// `derive::write_chapters` writes `chapter_source = 'filename'` and re-derives
/// the same number onto the target from the same filename; `derive::write_work`
/// writes `label = key`. Counting either would report a loss on every fold of
/// every ordinary work.
#[test]
fn folding_a_work_that_carries_nothing_by_hand_discards_nothing() {
    let (conn, from, into) = folded_pair();
    conn.execute(
        "UPDATE member SET chapter_declared = 3.0, chapter_source = 'filename'
          WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
        [from],
    )
    .expect("a chapter number the scan itself derived");

    let moved = folded(&conn, from, into);
    assert_eq!(
        moved,
        Moved {
            positions: 0,
            notes: 0,
            dropped_notes: 0,
            truth_pages: 0,
            collisions: 0,
            lost_positions: 0,
            discarded: 0,
            uncarried_alignments: 0,
            hand_carried: 0,
        },
        "the edition and member rows the fold removes are the duplication, \
         not a cost"
    );
}

/// Each disjunct of the `edition` predicate in [`super::DISCARDED`] —
/// `label IS NOT key OR language IS NOT NULL OR kind IS NOT NULL` — is
/// individually load-bearing.
///
/// **Neither existing fixture can say that.**
/// `what_the_delete_takes_that_the_fold_does_not_move_is_counted` sets all
/// three at once and
/// `folding_a_work_that_carries_nothing_by_hand_discards_nothing` sets none,
/// so dropping either half of the `OR` chain — collapsing it to one disjunct,
/// or to two — leaves every test in this file green. Each case below sets
/// exactly one of the three and leaves the other two at their default, so
/// removing any one disjunct from the predicate fails exactly the case that
/// disjunct alone covers.
#[test]
fn each_hand_classification_alone_is_discarded() {
    for update in [
        "UPDATE edition SET label = 'Japanese raw' WHERE work_id = ?1",
        "UPDATE edition SET language = 'ja' WHERE work_id = ?1",
        "UPDATE edition SET kind = 'raw' WHERE work_id = ?1",
    ] {
        let (conn, from, into) = folded_pair();
        conn.execute(update, [from])
            .expect("one hand classification set alone, the other two at their default");

        let moved = folded(&conn, from, into);

        assert_eq!(
            moved.discarded, 1,
            "`{update}` alone must be discarded on its own, with the other \
             two disjuncts false"
        );
    }
}

/// A reader row on an edition key the work no longer carries stops the fold
/// dead — the one shape that is invisible to every mover and to the count.
///
/// **`progress.edition`, `note.edition`, `truth_page.edition` and
/// `truth_member.edition` are plain `TEXT` with no foreign key to
/// `edition(key)`**, and `move_positions`, `move_notes` and `move_truth` are
/// all keyed on a `from_key` drawn from the `edition` *table*. A row naming a
/// key that table does not hold is therefore moved by nothing, absent from
/// [`super::DISCARDED`], and taken by the final `DELETE`.
///
/// **The state is reachable and the repository documents it.**
/// `taiyaku-serve`'s `notes` module calls an edition key one of "the two
/// things a re-solve can invalidate underneath a note that nobody touched"
/// and deliberately grandfathers such notes so they survive later writes;
/// `routes::put_truth` validates no edition key at all; and
/// `import::replace_editions` under `Scope::Document` deletes a work's
/// editions wholesale, stranding whatever named a key the incoming document
/// dropped or renamed.
///
/// **Refused rather than counted as discarded, deliberately.** [`DISCARDED`]
/// is for the library's own description of the material — derived, visible
/// and re-doable by hand. This is the reader's own state, which is why a
/// refusal is the answer: leaving a volume visibly duplicated is recoverable
/// and an operator can act on it, and a deleted note is neither.
///
/// [`DISCARDED`]: super::DISCARDED
#[test]
fn a_reader_row_on_an_edition_the_work_does_not_hold_refuses_the_fold() {
    let (conn, from, into) = folded_pair();
    put_progress(
        &conn,
        from,
        1,
        &Progress {
            edition: "dropped".into(),
            slot: 33,
        },
    )
    .expect("a place on an edition key the work no longer holds");
    seed_note(&conn, from, "dropped", 33);
    seed_truth(&conn, from, "dropped", 33, "bbb");

    let outcome = absorb(&conn, from, into).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => {
            panic!("four reader rows were on an unheld edition, yet it folded: {moved:?}")
        }
        Outcome::Refused(why) => {
            assert!(
                why.contains("dropped"),
                "the refusal must name the stranded key: {why}"
            );
            assert!(
                why.contains('4'),
                "and how many rows are on it — one position, one note, one \
                 truth page and one truth digest: {why}"
            );
            // **Every table by name, because the count alone proves
            // nothing.** With `truth_member` dropped from `READER_TABLES`
            // this would read 3 and fail, which is the point — but with
            // `truth_box` substituted for it, it would read 4 and pass, and
            // `truth_box` cannot be stranded on its own. Four tables are also
            // four different things for an operator to go and look at, which
            // is what the message is for.
            for table in ["progress", "note", "truth_page", "truth_member"] {
                assert!(
                    why.contains(table),
                    "the refusal must name `{table}`, not just a total that \
                     any other four rows would also produce: {why}"
                );
            }
        }
    }

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE id = ?1", [from], |r| {
            r.get(0)
        })
        .expect("counting");
    assert_eq!(works, 1, "the superseded work still stands");
    let counted = |table: &str| -> i64 {
        conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE work_id = ?1"),
            [from],
            |r| r.get(0),
        )
        .expect("counting")
    };
    assert_eq!(
        (
            counted("progress"),
            counted("note"),
            counted("truth_page"),
            counted("truth_box"),
            counted("truth_member")
        ),
        (1, 1, 1, 1, 1),
        "and every row the reader owns is still on it, untouched"
    );
}

/// Handed one work twice, [`absorb`] refuses instead of deleting it.
///
/// **It deletes a `work` row, and it is `pub`.** With `from == into` every
/// edition maps to itself, every move is a no-op, and the final `DELETE` takes
/// the work with the reader's position, their notes and the library's ground
/// truth on it. Nothing in `derive` asks for that — `superseded` skips a
/// candidate its own `written` set holds — but that is a guard in a different
/// module, in a function `fold` never calls, and a guard somewhere else is not
/// a guard.
#[test]
fn a_work_cannot_be_folded_into_itself() {
    let (conn, _, into) = folded_pair();
    put_progress(
        &conn,
        into,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 11,
        },
    )
    .expect("a place on the work");
    seed_note(&conn, into, "raw", 11);

    match absorb(&conn, into, into).expect("absorbing") {
        Outcome::Folded(moved) => panic!("a work was folded into itself: {moved:?}"),
        Outcome::Refused(why) => assert!(
            why.contains("itself"),
            "the refusal must say what it refused: {why}"
        ),
    }

    let (works, positions, notes): (i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM work WHERE id = ?1),
                    (SELECT count(*) FROM progress WHERE work_id = ?1),
                    (SELECT count(*) FROM note WHERE work_id = ?1)",
            [into],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("counting");
    assert_eq!(
        (works, positions, notes),
        (1, 1, 1),
        "the work still stands and so does everything on it"
    );
}

/// One reader holding the same note id on both works, on two different slots,
/// stops the fold rather than killing the derive.
///
/// **`note`'s key is `(user_id, work_id, id)` and migration 5 widened it to
/// that specifically to permit this state.** That migration's own rationale
/// names the routes — "copying a `taiyaku.notes.json` between volumes, or
/// importing one sidecar against two works" — and records that
/// `notes::validate` cannot see it either, its duplicate check being a
/// `HashSet` over the submitted document that "cannot see another work's
/// rows". A fold pair is by construction two works over the same volume,
/// which is exactly when a reader would have copied notes between them.
///
/// **Refused rather than re-keyed.** The id is one the reader's own sidecar
/// names, so a silent rewrite would make their next import create a duplicate
/// note rather than update the one they edited. `seed_note` uses one fixed id
/// for user 1, which is why every other test here folds two works whose notes
/// happen to share it and none of them reached this.
///
/// Two copies that agree in every column fold, the source's copy dropped
/// (O78); these two differ in `slot` alone — 33 against 11, on editions that
/// map onto each other — which is the anchor still differing once the fold has
/// rewritten the edition key.
#[test]
fn a_note_id_whose_two_copies_sit_on_different_slots_refuses_the_fold() {
    let (conn, from, into) = folded_pair();
    seed_note(&conn, from, "raw", 33);
    seed_note(&conn, into, "full-color", 11);

    let why = refused_leaving_every_note(&conn, from, into);

    assert!(
        why.contains("01JEXAMPLE0000000000000000"),
        "the refusal must name the id the reader's own sidecar names: {why}"
    );
}

/// Provenance that differs between the two sides is not mergeable and is not
/// this fold's to choose between.
///
/// **`edition_map` compares `member.sha256`; `truth_member` is a different
/// fact.** The map is built from the archives as they are *now*.
/// `truth_member` is a client-supplied snapshot of what the boxes were *drawn
/// against*, which `state::put_truth` writes verbatim with no validation. The
/// two diverging is not an edge case — it is the entire reason
/// `TruthEdition::is_stale_for` exists — so "the mapping is by identical
/// digest list, therefore the target already holds the same digests in the
/// same order" does not follow.
///
/// The two digests differ deliberately.
/// `truth_moves_with_its_boxes_and_its_digests` and
/// `a_truth_page_the_target_already_holds_stays_the_targets` both use
/// identical digests on the two sides, which is why neither reaches this.
#[test]
fn truth_drawn_against_different_archives_refuses_the_fold() {
    let (conn, from, into) = folded_pair();
    seed_truth(&conn, from, "raw", 33, "bbb");
    seed_truth(&conn, into, "full-color", 7, "ccc");

    let outcome = absorb(&conn, from, into).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => {
            panic!("two different digest lists met, yet it folded: {moved:?}")
        }
        Outcome::Refused(why) => {
            assert!(
                why.contains("full-color"),
                "the refusal must name the edition whose provenance differs: {why}"
            );
            assert!(
                why.contains("drawn against different archives"),
                "the refusal must distinguish this case from unclaimed boxes: {why}"
            );
        }
    }
}

/// The source's position loses the conflict, is destroyed by the fold's
/// `DELETE`, and is counted rather than vanishing.
///
/// **The mirror of
/// `where_both_works_hold_the_same_edition_the_newer_position_wins`.** There
/// the source's row is the newer and lands; here the target's is, so the
/// upsert's `WHERE excluded.updated > progress.updated` makes the source's row
/// a no-op. `execute` counts rows actually written, so `positions` is `0` here
/// and the loss is counted in `lost_positions`.
///
/// "Newer wins" is the decision — a slot number is not mergeable and the later
/// reading is the one somebody is in the middle of — and the assertion that
/// the target's slot stands says the counter did not quietly become a refusal.
#[test]
fn a_position_that_loses_the_conflict_is_counted_rather_than_lost_silently() {
    let (conn, from, into) = folded_pair();
    put_progress(
        &conn,
        from,
        1,
        &Progress {
            edition: "raw".into(),
            slot: 33,
        },
    )
    .expect("a place on the superseded work");
    conn.execute(
        "UPDATE progress SET updated = '2000-01-01T00:00:00Z' WHERE work_id = ?1",
        [from],
    )
    .expect("read long ago and not since");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (1, ?1, 'full-color', 4, '2026-08-27T00:00:00Z')",
        [into],
    )
    .expect("a newer place already on the target's mapped edition");

    let moved = folded(&conn, from, into);

    assert_eq!(
        (moved.positions, moved.lost_positions),
        (0, 1),
        "nothing was written and the position that lost is counted"
    );
    let (slot, rows): (u32, i64) = conn
        .query_row(
            "SELECT slot, (SELECT count(*) FROM progress) FROM progress
              WHERE work_id = ?1 AND edition = 'full-color'",
            [into],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the surviving position");
    assert_eq!(
        (slot, rows),
        (4, 1),
        "the newer position stands and the older one is gone — which is the \
         decision, not an accident, and is why it is counted rather than refused"
    );
}

/// Two editions of the superseded work over identical archives both land on
/// the one edition of the target that holds them, one of the two reading
/// positions stops existing, and the count says so **whichever way round the
/// two sort**.
///
/// **`edition_map` is non-injective and says so.** This is the second of the
/// two routes to a `progress` conflict, and it needs no grandfathered row at
/// all: both source rows are live, both are the reader's, and one of them is
/// about to be deleted.
///
/// **Both orderings.** With the older reading on the later-sorting key the
/// second `execute` is a no-op; with the **newer** reading on the
/// later-sorting key it is an `UPDATE` that overwrites the first. Either way
/// one row survives and one reading position is destroyed, so the loss is
/// asserted as the same number in both orderings.
///
/// `positions` is asserted per ordering because it is a count of *writes* —
/// see [`Moved::positions`] — and the two orderings genuinely write a
/// different number of times.
#[test]
fn two_source_editions_landing_on_one_target_edition_count_the_position_that_stops_existing() {
    // `raw` sorts before `scan`, so the second element of each pair is the
    // one whose `execute` meets a row that is already there.
    for (newer, writes, survivor) in [("raw", 1, 33), ("scan", 2, 7)] {
        let conn = open();
        seed_work(&conn, 1, "target", &[("full-color", &["bbb"])]);
        seed_work(
            &conn,
            2,
            "superseded",
            &[("raw", &["bbb"]), ("scan", &["bbb"])],
        );
        for (edition, slot) in [("raw", 33), ("scan", 7)] {
            let updated = if edition == newer {
                "2026-08-27T00:00:00Z"
            } else {
                "2000-01-01T00:00:00Z"
            };
            conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot, updated)
                 VALUES (1, 2, ?1, ?2, ?3)",
                params![edition, slot, updated],
            )
            .expect("a place on each of the two renditions");
        }

        let moved = folded(&conn, 2, 1);

        assert_eq!(
            (moved.positions, moved.lost_positions),
            (writes, 1),
            "two positions existed and one of them stopped existing, with \
             `{newer}` carrying the newer reading"
        );
        let held: Vec<(String, u32)> = conn
            .prepare("SELECT edition, slot FROM progress ORDER BY edition")
            .and_then(|mut s| {
                s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
                    .collect::<rusqlite::Result<Vec<(String, u32)>>>()
            })
            .expect("every position left in the library");
        assert_eq!(
            held,
            vec![("full-color".to_string(), survivor)],
            "one row survives and it is the newer reading"
        );
    }
}

/// An edition holding no members at all has no digest-identical counterpart
/// by definition, so the fold refuses — the safe direction.
///
/// `digests_by_edition` joins `member`, so such an edition is absent from the
/// map for the same reason an unmatched one is; `edition_keys` is read
/// separately precisely so that absence reads as "unmapped" rather than as
/// "mapped". Nothing hangs off a memberless edition but the metadata
/// `DISCARDED` counts, so refusing costs a person only the duplicate they
/// could already see.
///
/// [`DISCARDED`]: super::DISCARDED
#[test]
fn an_edition_holding_no_members_at_all_refuses_the_fold() {
    let conn = open();
    seed_work(&conn, 1, "target", &[("full-color", &["bbb"])]);
    seed_work(&conn, 2, "superseded", &[("raw", &["bbb"]), ("empty", &[])]);

    match absorb(&conn, 2, 1).expect("the scan must not die of this") {
        Outcome::Folded(moved) => panic!("a memberless edition was mapped: {moved:?}"),
        Outcome::Refused(why) => assert!(
            why.contains("empty"),
            "the refusal must name the edition holding nothing: {why}"
        ),
    }
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE id = 2", [], |r| r.get(0))
        .expect("counting");
    assert_eq!(works, 1, "the superseded work still stands");
}

/// One side holding no `truth_member` row at all is not a competing claim,
/// so the fold goes ahead and the provenance that does exist moves with the
/// boxes.
///
/// **The guard would otherwise be a fold-defeating narrowing.** Truth is
/// written per edition by a labelling client that may never have touched the
/// target's, and refusing every pair where one side is silent would leave
/// O73's duplication standing over the ordinary case.
///
/// **The digest is `ddd`, not `bbb`, for
/// [`truth_moves_with_its_boxes_and_its_digests`]'s reason.** `bbb` is also
/// `member.sha256` on both sides of [`folded_pair`], so a [`super::move_truth`]
/// that sourced the digest from `member` instead of `truth_member` would read
/// `bbb` right back and pass this test unchanged.
#[test]
fn truth_provenance_on_one_side_only_is_not_a_clash() {
    let (conn, from, into) = folded_pair();
    seed_truth(&conn, from, "raw", 33, "ddd");

    let moved = folded(&conn, from, into);

    let digests: Vec<String> = conn
        .prepare("SELECT sha256 FROM truth_member WHERE work_id = ?1 AND edition = 'full-color'")
        .and_then(|mut s| {
            s.query_map([into], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<String>>>()
        })
        .expect("reading the provenance back");
    assert_eq!(
        digests,
        vec!["ddd".to_string()],
        "the target held none, so the source's moved across intact, not the \
         archive's own current digest"
    );
    assert_eq!(moved.truth_pages, 1, "and the page moved with it");
}

/// Boxes carrying no provenance of their own do not take the provenance the
/// other side recorded — from the source side.
///
/// **The same shape the clash guard closes, reached from the other side.**
/// A source edition holding `truth_page`/`truth_box` rows and no
/// `truth_member` row would otherwise have its boxes copied onto the target
/// key, where the target's own non-empty list stands over them through
/// `move_truth`'s `INSERT OR IGNORE`. `TruthEdition::is_stale_for` returns
/// `true` when `members` is empty and `digests` is not, so `score` refuses
/// that edition as stale *before* the fold and would read the very same boxes
/// as fresh *after* it — "boxes drawn on superseded art recorded as fresh",
/// which is the sentence the guard exists to prevent.
#[test]
fn source_boxes_with_no_provenance_do_not_take_the_targets() {
    let (conn, from, into) = folded_pair();
    seed_truth_without_provenance(&conn, from, "raw", 33);
    seed_truth(&conn, into, "full-color", 7, "ccc");

    let outcome = absorb(&conn, from, into).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => panic!(
            "boxes with no provenance came under the target's `ccc`, yet it folded: {moved:?}"
        ),
        Outcome::Refused(why) => {
            assert!(
                why.contains("full-color"),
                "the refusal must name the edition whose provenance would spread: {why}"
            );
            assert!(
                why.contains("boxes on one side record no archives at all"),
                "the refusal must distinguish this case from differing archives: {why}"
            );
        }
    }
    let pages: i64 = conn
        .query_row(
            "SELECT count(*) FROM truth_page WHERE work_id = ?1",
            [into],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(pages, 1, "nothing moved onto the target");
}

/// The same rule from the target side: the source's provenance does not come
/// to stand over boxes the target already held with none.
///
/// **Not the case `truth_provenance_on_one_side_only_is_not_a_clash` pins.**
/// There the target holds no truth *at all*, so the source's provenance
/// arrives over the source's own boxes and covers nothing else — the ordinary
/// O73 fold, and refusing it would defeat the fold this module exists for.
/// Here the target holds boxes and no record of what they were drawn against,
/// so the list arriving from the source would become their claim too.
#[test]
fn target_boxes_with_no_provenance_do_not_take_the_sources() {
    let (conn, from, into) = folded_pair();
    seed_truth(&conn, from, "raw", 33, "bbb");
    seed_truth_without_provenance(&conn, into, "full-color", 7);

    let outcome = absorb(&conn, from, into).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => panic!(
            "the source's `bbb` came to stand over the target's unclaimed boxes, \
             yet it folded: {moved:?}"
        ),
        Outcome::Refused(why) => {
            assert!(
                why.contains("full-color"),
                "the refusal must name the edition whose provenance would spread: {why}"
            );
            assert!(
                why.contains("boxes on one side record no archives at all"),
                "the refusal must distinguish this case from differing archives: {why}"
            );
        }
    }
}

/// Each stranded count is printed against its own table, and the fixture is
/// what makes that assertable.
///
/// **`a_reader_row_on_an_edition_the_work_does_not_hold_refuses_the_fold`
/// cannot say this.** It seeds one row per table, asserts the total `4` and
/// then asserts each of the four table names appears somewhere in the
/// message — so a `stranded` that zipped the counts to the wrong tables, or
/// printed the same count four times, would pass it unchanged. Every count
/// here is different from every other, and the assertion is on the paired
/// text, for the reason
/// `library::tests::derive_summary_names_the_four_headline_counts_and_the_proposal_count`
/// gives about repeated fixture values.
#[test]
fn the_stranded_message_pairs_each_count_with_the_table_it_came_from() {
    let (conn, from, into) = folded_pair();
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'user')",
        [],
    )
    .expect("a second reader");
    for user in [1, 2] {
        put_progress(
            &conn,
            from,
            user,
            &Progress {
                edition: "dropped".into(),
                slot: 33,
            },
        )
        .expect("two places on a key the work no longer holds");
    }
    put_notes(
        &conn,
        from,
        1,
        &Notes {
            taiyaku_notes: crate::NOTES_VERSION,
            work: "ignored".to_string(),
            notes: (1..=3)
                .map(|n| Note {
                    id: format!("01JEXAMPLE000000000000000{n}"),
                    edition: "dropped".to_string(),
                    slot: 33,
                    region: None,
                    color: None,
                    body: "a note on a key nothing can move".to_string(),
                    created: "2026-08-27T09:00:00Z".to_string(),
                    updated: "2026-08-27T09:00:00Z".to_string(),
                    stale: false,
                    at: None,
                })
                .collect(),
        },
    )
    .expect("three notes on that same key");
    let mut doc = TruthDoc::new();
    doc.editions.insert(
        "dropped".to_string(),
        TruthEdition {
            members: (1..=5)
                .map(|n| RegionMember {
                    sha256: format!("digest-{n}"),
                })
                .collect(),
            pages: (1..=4)
                .map(|page| {
                    (
                        page,
                        vec![TruthRegion {
                            x: 0.1,
                            y: 0.2,
                            w: 0.3,
                            h: 0.4,
                            axis: None,
                            kind: Kind::Dialogue,
                            ignore: false,
                            text: None,
                        }],
                    )
                })
                .collect(),
        },
    );
    put_truth(&conn, from, &doc).expect("four labelled pages against five archives");

    let outcome = absorb(&conn, from, into).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => {
            panic!("fourteen reader rows were on an unheld edition, yet it folded: {moved:?}")
        }
        Outcome::Refused(why) => {
            assert!(
                why.contains("14 reader row(s)"),
                "the total counts every stranded row: {why}"
            );
            assert!(
                why.contains("`dropped` (2 progress, 3 note, 4 truth_page, 5 truth_member)"),
                "each count must be printed against the table it was read \
                 from, in `READER_TABLES` order: {why}"
            );
        }
    }
}

/// The shared-note refusal names how many ids collide and shows a few, rather
/// than joining every one of them onto the summary line.
///
/// **The realistic instance is not one id.** `shared_note_ids`' own rationale
/// names the route — "importing one sidecar against two works" — and that
/// shares *every* note in the document. The message travels through
/// `DeriveReport::unfoldable` onto `derive_summary`'s single line, which is
/// why the ids shown are bounded.
///
/// **The two copies of each id differ in `body`**: the shape of one sidecar
/// edited on one volume after it was imported against both. Two copies that
/// agree in every column fold instead (O78).
#[test]
fn the_shared_note_refusal_counts_the_ids_and_shows_a_few() {
    let (conn, from, into) = folded_pair();
    let ids: Vec<String> = (1..=5)
        .map(|n| format!("01JEXAMPLE000000000000000{n}"))
        .collect();
    for (work, edition) in [(from, "raw"), (into, "full-color")] {
        put_notes(
            &conn,
            work,
            1,
            &Notes {
                taiyaku_notes: crate::NOTES_VERSION,
                work: "ignored".to_string(),
                notes: ids
                    .iter()
                    .map(|id| Note {
                        id: id.clone(),
                        edition: edition.to_string(),
                        slot: 3,
                        region: None,
                        color: None,
                        body: format!(
                            "one sidecar imported against both works, then edited on work {work}"
                        ),
                        created: "2026-08-27T09:00:00Z".to_string(),
                        updated: "2026-08-27T09:00:00Z".to_string(),
                        stale: false,
                        at: None,
                    })
                    .collect(),
            },
        )
        .expect("the same five ids on both works");
    }

    let outcome = absorb(&conn, from, into).expect("the scan must not die of this");

    match outcome {
        Outcome::Folded(moved) => panic!("five note ids collided, yet it folded: {moved:?}"),
        Outcome::Refused(why) => {
            assert!(
                why.contains("holds 5 note id(s)"),
                "the count says how large the overlap is: {why}"
            );
            assert!(
                why.contains(&ids[0]) && why.contains("and 2 more"),
                "a few ids to go and look at, and the rest as a number: {why}"
            );
            assert!(
                !why.contains(&ids[4]),
                "the whole list is what this stopped printing: {why}"
            );
        }
    }
}

// --- O78: one note id on both works ----------------------------------------

/// The id reader 1 holds on both works in the tests below.
const SHARED: &str = "01JSHARED00000000000000000";

/// The note under [`SHARED`] as one sidecar imported against both works
/// writes it: the whole slot, the default colour, one pair of timestamps.
fn shared_note(edition: &str) -> Note {
    Note {
        id: SHARED.to_string(),
        edition: edition.to_string(),
        slot: 3,
        region: None,
        color: None,
        body: "the shop sign reads 'closed on Tuesdays'".to_string(),
        created: "2026-08-27T09:00:00Z".to_string(),
        updated: "2026-08-27T09:00:00Z".to_string(),
        stale: false,
        at: None,
    }
}

/// [`shared_note`] with a sized region and a colour, so both nullable groups
/// carry a value rather than NULL.
fn sized_note(edition: &str) -> Note {
    Note {
        region: Some(Region {
            x: 0.25,
            y: 0.5,
            w: 0.125,
            h: 0.0625,
        }),
        color: Some("amber".to_string()),
        ..shared_note(edition)
    }
}

/// One reader's whole notes document on one work, through `put_notes`.
fn write_notes(conn: &Connection, work_id: i64, user_id: i64, notes: Vec<Note>) {
    put_notes(
        conn,
        work_id,
        user_id,
        &Notes {
            taiyaku_notes: crate::NOTES_VERSION,
            work: "ignored".to_string(),
            notes,
        },
    )
    .expect("a reader's notes");
}

/// One `note` row, every column and its `rowid`.
///
/// **The `rowid` is what tells the target's row from a copy of it.** Two
/// identical notes are identical in every column a reader can see, so a fold
/// that deleted the target's row and moved the source's into its place leaves
/// a table no column comparison can tell apart from the right answer. `note`
/// is a rowid table and `move_notes` is an `UPDATE`, so the row's `rowid`
/// survives a move and does not survive a delete.
#[derive(Debug, Clone, PartialEq)]
struct NoteRow {
    rowid: i64,
    user_id: i64,
    id: String,
    work_id: i64,
    edition: String,
    slot: i64,
    region: [Option<f64>; 4],
    color: Option<String>,
    body: String,
    created: String,
    updated: String,
}

/// Every `note` row in the library, in `rowid` order — so "nothing was
/// written" and "this is what is left" are comparisons of the whole table
/// rather than a count, which any other rows of the same number would satisfy.
fn every_note(conn: &Connection) -> Vec<NoteRow> {
    conn.prepare(
        "SELECT rowid, user_id, id, work_id, edition, slot, rx, ry, rw, rh,
                color, body, created, updated
           FROM note ORDER BY rowid",
    )
    .and_then(|mut statement| {
        statement
            .query_map([], |r| {
                Ok(NoteRow {
                    rowid: r.get(0)?,
                    user_id: r.get(1)?,
                    id: r.get(2)?,
                    work_id: r.get(3)?,
                    edition: r.get(4)?,
                    slot: r.get(5)?,
                    region: [r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?],
                    color: r.get(10)?,
                    body: r.get(11)?,
                    created: r.get(12)?,
                    updated: r.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<NoteRow>>>()
    })
    .expect("every note in the library")
}

/// [`absorb`], expected to refuse, with every write to `note` made to fail
/// first and every row compared afterwards.
///
/// **The triggers are what make "nothing was written" observable.** A refusal
/// returns out of `absorb`'s transaction, and its `Drop` rolls back whatever
/// that transaction wrote, so a fold that dropped identical copies *before*
/// deciding to refuse would leave the table exactly as it found it — and a
/// before-and-after comparison alone would pass it. The module's promise is
/// stronger than the rollback: every refusal arm is a `SELECT`. With a trigger
/// raising on any insert, update or delete of `note`, a write before the
/// refusal comes back as an `Err` rather than vanishing.
fn refused_leaving_every_note(conn: &Connection, from: i64, into: i64) -> String {
    let before = every_note(conn);
    conn.execute_batch(
        "CREATE TRIGGER note_insert_refused BEFORE INSERT ON note
           BEGIN SELECT RAISE(ABORT, 'a fold that should have refused wrote to note'); END;
         CREATE TRIGGER note_update_refused BEFORE UPDATE ON note
           BEGIN SELECT RAISE(ABORT, 'a fold that should have refused wrote to note'); END;
         CREATE TRIGGER note_delete_refused BEFORE DELETE ON note
           BEGIN SELECT RAISE(ABORT, 'a fold that should have refused wrote to note'); END;",
    )
    .expect("making every write to `note` fail");

    let outcome = absorb(conn, from, into).expect(
        "a refusal writes nothing, so no trigger on `note` fires; an `Err` here is a \
         write, from a fold that went ahead or from a refusal that wrote first",
    );

    let why = match outcome {
        Outcome::Folded(moved) => {
            panic!("two copies of one note id that differ, yet it folded: {moved:?}")
        }
        Outcome::Refused(why) => why,
    };
    assert_eq!(
        every_note(conn),
        before,
        "every note row — every column, and its rowid — is as it was"
    );
    let works: i64 = conn
        .query_row(
            "SELECT count(*) FROM work WHERE id IN (?1, ?2)",
            params![from, into],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(works, 2, "and both works still stand");
    why
}

/// Reader 1 holds [`SHARED`] on both works of [`folded_pair`], as `source` on
/// the superseded work's `raw` and as `target` on the target, and the fold is
/// refused over it.
fn refused_over(source: Note, target: Note) -> String {
    let (conn, from, into) = folded_pair();
    write_notes(&conn, from, 1, vec![source]);
    write_notes(&conn, into, 1, vec![target]);

    let why = refused_leaving_every_note(&conn, from, into);

    assert!(
        why.contains(SHARED),
        "the refusal must name the id whose copies differ: {why}"
    );
    why
}

/// Two copies of one note, one on each work, agreeing in every column but the
/// two a fold rewrites: the source's copy is dropped, the target's row stays
/// exactly as it was, and the fold goes ahead.
///
/// **O78.** This is the ordinary shape — the route migration 5's own
/// rationale names, "importing one sidecar against two works", writes exactly
/// these rows. Two rows equal in `body`, `slot`, region, `color`, `created`
/// and `updated`, on editions the fold maps onto each other, are one fact
/// written twice; dropping one loses nothing.
///
/// **Whole-slot and default colour, so all five nullable columns are NULL on
/// both sides.** SQL's `=` answers NULL for `NULL = NULL`, which is not true,
/// so a comparison written that way would call these two different and
/// refuse. [`copies_on_two_edition_keys_that_map_onto_each_other_are_one_note`]
/// is the same fold with every one of those columns set.
///
/// **Two more notes move beside it**, so the counts are three different facts
/// rather than a number any mistake could reach: two rows moved, one copy
/// dropped. The target's row is identified by `rowid`, for [`NoteRow`]'s reason.
#[test]
fn an_identical_copy_of_a_note_on_both_works_is_dropped_and_the_fold_goes_ahead() {
    let (conn, from, into) = folded_pair();
    write_notes(
        &conn,
        from,
        1,
        vec![
            shared_note("raw"),
            Note {
                id: "01JONLYSOURCE0000000000001".to_string(),
                ..shared_note("raw")
            },
            Note {
                id: "01JONLYSOURCE0000000000002".to_string(),
                slot: 4,
                ..shared_note("raw")
            },
        ],
    );
    write_notes(&conn, into, 1, vec![shared_note("full-color")]);
    let before = every_note(&conn);

    let moved = folded(&conn, from, into);

    let mut expected: Vec<NoteRow> = before
        .into_iter()
        .filter(|n| !(n.work_id == from && n.id == SHARED))
        .map(|n| {
            if n.work_id == from {
                NoteRow {
                    work_id: into,
                    edition: "full-color".to_string(),
                    ..n
                }
            } else {
                n
            }
        })
        .collect();
    expected.sort_by_key(|n| n.rowid);
    assert_eq!(
        every_note(&conn),
        expected,
        "the target's own row stands untouched under its own rowid, the \
         source's copy of it is gone, and the other two notes moved"
    );
    assert_eq!(
        (moved.notes, moved.dropped_notes),
        (2, 1),
        "two notes moved and one identical copy dropped — the copy is not a move"
    );
}

/// The same fold with a sized region and a colour on both copies, and the
/// edition keys differing: `raw` on the superseded work, `full-color` on the
/// target, which is where `raw`'s archive lives.
///
/// **No slot moves in this fixture.** `edition_map` pairs editions whose
/// members carry the same digests in the same order, and neither side here
/// holds a spread flag, so slot 3 is slot 3 on both and the only part of a
/// note's anchor this fold rewrites is its edition key. Where the two
/// editions' spread flags differ the slot moves too, and
/// `note_copies_are_one_note_by_the_page_they_name_not_the_number_they_hold`
/// is that case. "The same anchor" is therefore the target's key being
/// exactly the one the source's key maps to, with `slot` and all four
/// region columns equal. A comparison of the raw keys calls these two
/// different and refuses.
#[test]
fn copies_on_two_edition_keys_that_map_onto_each_other_are_one_note() {
    let (conn, from, into) = folded_pair();
    write_notes(&conn, from, 1, vec![sized_note("raw")]);
    write_notes(&conn, into, 1, vec![sized_note("full-color")]);
    let target: Vec<NoteRow> = every_note(&conn)
        .into_iter()
        .filter(|n| n.work_id == into)
        .collect();

    let moved = folded(&conn, from, into);

    assert_eq!(
        every_note(&conn),
        target,
        "the target's row, region and colour included, is all that is left"
    );
    assert_eq!((moved.notes, moved.dropped_notes), (0, 1));
}

/// Same id, same anchor, different words: not one note, and not this fold's to
/// choose between.
#[test]
fn a_note_id_whose_two_copies_say_different_things_refuses_the_fold() {
    refused_over(
        shared_note("raw"),
        Note {
            body: "the shop sign reads 'closed on Wednesdays'".to_string(),
            ..shared_note("full-color")
        },
    );
}

/// A colour is something a reader chose, so two copies differing in it are
/// two different notes even where every word and every coordinate agrees.
#[test]
fn a_note_id_whose_two_copies_differ_in_colour_refuses_the_fold() {
    refused_over(
        shared_note("raw"),
        Note {
            color: Some("amber".to_string()),
            ..shared_note("full-color")
        },
    );
}

/// `updated` alone differing is an edit on one side — a recolour or a nudged
/// box put back — and `state::merge_notes` reads `updated` to pick a winner,
/// so it is a fact about the note rather than a formatting detail.
#[test]
fn a_note_id_whose_two_copies_differ_in_updated_refuses_the_fold() {
    refused_over(
        shared_note("raw"),
        Note {
            updated: "2026-09-01T10:00:00Z".to_string(),
            ..shared_note("full-color")
        },
    );
}

/// `created` differing means two notes that were written separately and happen
/// to carry one id, whatever else agrees.
#[test]
fn a_note_id_whose_two_copies_differ_in_created_refuses_the_fold() {
    refused_over(
        shared_note("raw"),
        Note {
            created: "2026-08-26T09:00:00Z".to_string(),
            ..shared_note("full-color")
        },
    );
}

/// One coordinate of the region differing, both copies sized: a box dragged
/// on one volume and not the other.
#[test]
fn a_note_id_whose_two_copies_differ_in_region_refuses_the_fold() {
    refused_over(
        sized_note("raw"),
        Note {
            region: Some(Region {
                x: 0.25,
                y: 0.5,
                w: 0.125,
                h: 0.125,
            }),
            ..sized_note("full-color")
        },
    );
}

/// Every column equal, and the edition key equal too — `raw` on both — but
/// the superseded work's `raw` holds the archive the target calls
/// `full-color`, and the target's own `raw` is a different book.
///
/// **So the raw keys agreeing is the wrong answer.** After the fold the source's
/// note would be on `full-color` and the target's is on `raw`: two anchors.
/// A comparison of the keys as stored calls these one note and drops a note
/// about one book because a note about another agreed with it.
#[test]
fn a_note_id_whose_edition_does_not_map_to_the_targets_refuses_the_fold() {
    refused_over(shared_note("raw"), shared_note("raw"));
}

/// One id with identical copies and one whose copies differ, in the same fold:
/// refused whole, and the identical copy is not dropped on the way.
#[test]
fn one_identical_copy_and_one_that_differs_refuse_the_fold_whole() {
    let differs = "01JDIFFERS0000000000000000";
    let (conn, from, into) = folded_pair();
    write_notes(
        &conn,
        from,
        1,
        vec![
            shared_note("raw"),
            Note {
                id: differs.to_string(),
                ..shared_note("raw")
            },
        ],
    );
    write_notes(
        &conn,
        into,
        1,
        vec![
            shared_note("full-color"),
            Note {
                id: differs.to_string(),
                body: "edited on the colour volume".to_string(),
                ..shared_note("full-color")
            },
        ],
    );

    let why = refused_leaving_every_note(&conn, from, into);

    assert!(
        why.contains("holds 1 note id(s)") && why.contains(differs),
        "the refusal counts and names the one id whose copies differ: {why}"
    );
    assert!(
        !why.contains(SHARED),
        "the identical pair is not a conflict and is not named as one: {why}"
    );
}

/// A second reader's notes are theirs: the first reader's identical pair is
/// resolved per reader, and nothing of the second's is dropped or disturbed.
///
/// **Reader 2 holds [`SHARED`] on the superseded work too**, in every column
/// the same as reader 1's copy on the target — their own note, which happens
/// to carry the id — and a note under a different id on the target. For
/// reader 2 that id is on one work only, so it moves. A match or a delete that
/// left `user_id` out would find reader 1's target copy, call reader 2's note
/// identical to it and drop it.
#[test]
fn a_second_readers_notes_are_not_touched_by_the_first_readers_identical_copy() {
    let (conn, from, into) = folded_pair();
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'user')",
        [],
    )
    .expect("a second reader");
    write_notes(&conn, from, 1, vec![shared_note("raw")]);
    write_notes(&conn, into, 1, vec![shared_note("full-color")]);
    write_notes(&conn, from, 2, vec![shared_note("raw")]);
    write_notes(
        &conn,
        into,
        2,
        vec![Note {
            id: "01JSECONDREADER00000000000".to_string(),
            ..shared_note("full-color")
        }],
    );
    let second_before: Vec<NoteRow> = every_note(&conn)
        .into_iter()
        .filter(|n| n.user_id == 2)
        .collect();

    let moved = folded(&conn, from, into);

    let second_after: Vec<NoteRow> = every_note(&conn)
        .into_iter()
        .filter(|n| n.user_id == 2)
        .collect();
    let expected: Vec<NoteRow> = second_before
        .into_iter()
        .map(|n| {
            if n.work_id == from {
                NoteRow {
                    work_id: into,
                    edition: "full-color".to_string(),
                    ..n
                }
            } else {
                n
            }
        })
        .collect();
    assert_eq!(
        second_after, expected,
        "reader 2's note on the superseded work moved under its own rowid, and \
         their note on the target is untouched"
    );
    assert_eq!(
        (moved.notes, moved.dropped_notes),
        (1, 1),
        "reader 2's note moved; reader 1's copy was the only one dropped"
    );
}

/// One `work` whose editions each hold one member of `pages` pages, with the
/// spread flags given and read in `reading` — the slot space a carried slot is
/// addressed in, which [`seed_work`]'s one-page members cannot express.
fn seed_paged_work(
    conn: &Connection,
    work_id: i64,
    name: &str,
    reading: &str,
    editions: &[(&str, &str, u32, &[u32])],
) {
    let hub = editions.first().map_or("raw", |(key, ..)| *key);
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (?1, ?2, ?2, ?2, ?3, ?4)",
        params![work_id, name, reading, hub],
    )
    .expect("seeding a work");
    for (key, sha256, pages, spreads) in editions {
        conn.execute(
            "INSERT INTO edition (work_id, key, label) VALUES (?1, ?2, ?2)",
            params![work_id, key],
        )
        .expect("seeding an edition");
        let edition_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO member (edition_id, ordinal, path, sha256, pages)
             VALUES (?1, 0, ?2, ?3, ?4)",
            params![edition_id, format!("archive/{sha256}"), sha256, pages],
        )
        .expect("seeding a member");
        for page in *spreads {
            conn.execute(
                "INSERT INTO page_flag (edition_id, kind, page) VALUES (?1, 'spread', ?2)",
                params![edition_id, page],
            )
            .expect("seeding a spread flag");
        }
    }
}

/// The page, and the half, that slot `slot` of `work_id`'s edition `key` names
/// — read through the projection and `Edition::slots`, the same rebuild the
/// reader draws from, so an assertion on it is an assertion about what a
/// person sees rather than about a column.
fn page_at(conn: &Connection, work_id: i64, key: &str, slot: i64) -> Option<taiyaku_core::Slot> {
    let sidecar = crate::project::sidecar(conn, work_id).expect("projecting the work");
    let slots = sidecar.editions.get(key)?.slots(sidecar.work.reading);
    usize::try_from(slot)
        .ok()
        .and_then(|i| slots.get(i).copied())
}

/// **O121: a slot is only a number inside one edition's spread flags.** The
/// superseded work's `colour` edition holds a joined spread at page 2 — slots
/// `0, 1, 2R, 2L, 3, 4, 5` — and its digest-identical twin on the target was
/// built by a scan, which never writes a spread flag, so there slot 5 is page
/// 5 and slot 3 is page 3. A place on page 4 and a note on the left half of
/// page 2 must still name page 4 and the left half of page 2 once they live on
/// the target.
#[test]
fn a_carried_position_and_note_name_the_page_they_named_before_the_fold() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, &[])],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 6, &[2])]);
    put_progress(
        &conn,
        2,
        1,
        &Progress {
            edition: "colour".to_string(),
            slot: 5,
        },
    )
    .expect("the reader's place on page 4");
    seed_note(&conn, 2, "colour", 3);
    let left_of_2 = taiyaku_core::Slot {
        page: 2,
        half: Some(taiyaku_core::Half::Left),
    };
    let page_4 = taiyaku_core::Slot {
        page: 4,
        half: None,
    };
    assert_eq!(
        page_at(&conn, 2, "colour", 5),
        Some(page_4),
        "the fixture's own premise"
    );
    assert_eq!(
        page_at(&conn, 2, "colour", 3),
        Some(left_of_2),
        "the fixture's own premise"
    );

    folded(&conn, 2, 1);

    let position: i64 = conn
        .query_row(
            "SELECT slot FROM progress WHERE user_id = 1 AND work_id = 1 AND edition = 'full-color'",
            [],
            |r| r.get(0),
        )
        .expect("the carried position");
    let note: i64 = conn
        .query_row(
            "SELECT slot FROM note WHERE user_id = 1 AND work_id = 1 AND edition = 'full-color'",
            [],
            |r| r.get(0),
        )
        .expect("the carried note");
    assert_eq!(
        (
            page_at(&conn, 1, "full-color", position),
            page_at(&conn, 1, "full-color", note)
        ),
        (Some(page_4), Some(left_of_2)),
        "position slot {position} and note slot {note} on the target must name what they \
         named on the folded work"
    );
}

/// A run of an alignment fixture: `a`, and `b` where it pairs, marked by hand
/// or not.
fn run(a: &str, b: Option<&str>, hand: bool) -> taiyaku_core::RunEntry {
    taiyaku_core::RunEntry {
        a: taiyaku_core::SlotSpan::parse(a).expect("an `a` span"),
        b: b.map(|b| taiyaku_core::SlotSpan::parse(b).expect("a `b` span")),
        margin: None,
        by: hand.then_some(taiyaku_core::Source::Hand),
        note: None,
    }
}

/// Replace `work_id`'s alignments with these, through `state::put_alignments`
/// — the writer the correction route uses.
fn write_alignments(
    conn: &Connection,
    work_id: i64,
    alignments: &[(&str, &str, Vec<taiyaku_core::RunEntry>)],
) {
    let doc = taiyaku_core::Sidecar {
        taiyaku: taiyaku_core::VERSION,
        work: taiyaku_core::Work {
            title: "fixture".to_string(),
            reading: taiyaku_core::Direction::RightToLeft,
        },
        editions: BTreeMap::new(),
        hub: String::new(),
        alignments: alignments
            .iter()
            .map(|(from, to, runs)| taiyaku_core::Alignment {
                from: (*from).to_string(),
                to: (*to).to_string(),
                by: taiyaku_core::Source::Solver,
                solver: None,
                quality: None,
                runs: runs.clone(),
            })
            .collect(),
    };
    crate::state::put_alignments(conn, work_id, &doc).expect("the alignments");
}

fn projected(conn: &Connection, work_id: i64) -> taiyaku_core::Sidecar {
    crate::project::sidecar(conn, work_id).expect("projecting the work")
}

fn alignments_on(conn: &Connection, work_id: i64) -> Vec<(String, String)> {
    projected(conn, work_id)
        .alignments
        .iter()
        .map(|alignment| (alignment.from.clone(), alignment.to.clone()))
        .collect()
}

fn spreads_on(conn: &Connection, work_id: i64, key: &str) -> Vec<u32> {
    projected(conn, work_id).editions[key].spreads.clone()
}

/// What `route` answers on `work_id` for one slot of `from`, as a reader
/// swapping editions would be answered.
fn resolves(
    conn: &Connection,
    work_id: i64,
    from: &str,
    to: &str,
    slot: taiyaku_core::Slot,
) -> Option<taiyaku_core::Slot> {
    taiyaku_core::route(&projected(conn, work_id), from, to)
        .and_then(|route| route.resolve_slot(slot))
}

fn whole(page: u32) -> taiyaku_core::Slot {
    taiyaku_core::Slot { page, half: None }
}

fn right(page: u32) -> taiyaku_core::Slot {
    taiyaku_core::Slot {
        page,
        half: Some(taiyaku_core::Half::Right),
    }
}

fn left(page: u32) -> taiyaku_core::Slot {
    taiyaku_core::Slot {
        page,
        half: Some(taiyaku_core::Half::Left),
    }
}

fn place(conn: &Connection, work_id: i64, user_id: i64, edition: &str, slot: u32) {
    if user_id != 1 {
        conn.execute(
            "INSERT OR IGNORE INTO user (id, name, password_hash, role)
             VALUES (?1, 'reader ' || ?1, '!', 'user')",
            [user_id],
        )
        .expect("a second reader");
    }
    put_progress(
        conn,
        work_id,
        user_id,
        &Progress {
            edition: edition.to_string(),
            slot,
        },
    )
    .expect("a reading position");
}

fn position_slot(conn: &Connection, work_id: i64, user_id: i64, edition: &str) -> i64 {
    conn.query_row(
        "SELECT slot FROM progress WHERE user_id = ?1 AND work_id = ?2 AND edition = ?3",
        params![user_id, work_id, edition],
        |r| r.get(0),
    )
    .expect("the position")
}

fn note_slot(conn: &Connection, work_id: i64, edition: &str) -> i64 {
    conn.query_row(
        "SELECT slot FROM note WHERE work_id = ?1 AND edition = ?2",
        params![work_id, edition],
        |r| r.get(0),
    )
    .expect("the note")
}

/// Make every insert, update and delete on `tables` raise.
fn forbid_writes(conn: &Connection, tables: &[&str]) {
    for table in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!(
                "CREATE TRIGGER {table}_{op}_forbidden BEFORE {op} ON {table}
                   BEGIN SELECT RAISE(ABORT, 'a fold that should have refused wrote to {table}'); END;"
            ))
            .expect("making a write fail");
        }
    }
}

/// [`absorb`], expected to refuse, with every table the slot plan writes made
/// to raise on any write first — O78's `refused_leaving_every_note`, widened.
///
/// **A refusal rolls back, so without the triggers a write made before it is
/// invisible.** Every refusal arm is a `SELECT` and the plan's four writes —
/// spread flags, the twin's own rows, the folded work's rows and the carried
/// alignments — all come after the last of them.
fn refusal(conn: &Connection, from: i64, into: i64) -> String {
    forbid_writes(conn, &["page_flag", "progress", "note", "alignment", "run"]);
    match absorb(conn, from, into)
        .expect("a refusal writes nothing, so no trigger fires; an `Err` here is a write")
    {
        Outcome::Refused(why) => why,
        Outcome::Folded(moved) => panic!("the fold went ahead: {moved:?}"),
    }
}

/// **The flags a twin takes are the folded edition's spread flags, and only
/// those.** A credit flag is a heuristic's reading of one scanlation's pages
/// and stays counted; a spread flag is a measurement of the bytes both
/// editions hold, and every slot on the twin is built from it.
#[test]
fn a_twin_holding_no_spread_flags_takes_the_folded_editions_and_no_credit_flag() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, &[])],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "rtl",
        &[("colour", "bbb", 6, &[2, 4])],
    );
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'credit', 5 FROM edition WHERE work_id = 2",
        [],
    )
    .expect("a credit flag");

    let moved = folded(&conn, 2, 1);

    let target = projected(&conn, 1);
    assert_eq!(
        (
            target.editions["full-color"].spreads.clone(),
            target.editions["full-color"].credits.clone(),
            target.editions["raw"].spreads.clone(),
        ),
        (vec![2, 4], vec![], vec![]),
        "the twin takes both spread flags, no credit flag, and no other edition takes anything"
    );
    assert_eq!(
        moved.discarded, 1,
        "the credit flag is counted and the spread flags are not"
    );
}

/// **O76: an alignment moves with its editions, and routes on the surviving
/// work exactly as it did on the folded one.** The folded work's `colour`
/// joins page 1 as a spread; its twin `full-color` was built by a scan and
/// holds no flags. The alignment names `colour` and `en`, which the target
/// calls `full-color` and `en-color`, so a move that kept the keys, swapped
/// them, or left the flags behind each fails a different assertion. One run is
/// a person's correction, pairing page 2 with page 3, and the edge it lands on
/// is a direct one between two editions neither of which is the hub.
#[test]
fn a_carried_alignment_names_the_twins_and_routes_as_it_did() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[
            ("raw", "aaa", 8, &[]),
            ("full-color", "bbb", 8, &[]),
            ("en-color", "ccc", 8, &[]),
        ],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "rtl",
        &[("colour", "bbb", 8, &[1]), ("en", "ccc", 8, &[])],
    );
    write_alignments(
        &conn,
        2,
        &[(
            "colour",
            "en",
            vec![
                run("0-1R", Some("0-1"), false),
                run("1L", None, false),
                run("2", Some("3"), true),
                run("3", None, false),
                run("4-7", Some("4-7"), false),
            ],
        )],
    );
    assert_eq!(
        resolves(&conn, 2, "colour", "en", whole(2)),
        Some(whole(3)),
        "the fixture's own premise: the correction pairs page 2 with page 3"
    );

    let moved = folded(&conn, 2, 1);

    assert_eq!(moved.uncarried_alignments, 0, "nothing was left behind");
    assert_eq!(
        alignments_on(&conn, 1),
        vec![("full-color".to_string(), "en-color".to_string())],
        "one alignment, naming the twins"
    );
    assert_eq!(
        [whole(2), right(1), left(1), whole(5)].map(|slot| resolves(
            &conn,
            1,
            "full-color",
            "en-color",
            slot
        )),
        [Some(whole(3)), Some(whole(1)), None, Some(whole(5))],
        "the corrected page, a half, the gap and an ordinary page answer as they did"
    );
    let target = projected(&conn, 1);
    assert_eq!(
        taiyaku_core::route(&target, "full-color", "en-color").map(|route| route.via),
        Some(taiyaku_core::Via::Direct),
        "a stored edge between two editions neither of which is the hub"
    );
    assert_eq!(
        target.alignments[0]
            .runs
            .iter()
            .filter(|run| run.by == Some(taiyaku_core::Source::Hand))
            .count(),
        1,
        "the correction is still marked as a person's"
    );
}

/// **Where both works align the same two editions, a hand-corrected alignment
/// wins over one that is not, and otherwise the target keeps its own** — in
/// either stored direction. The incoming alignment pairs page 2 with page 3,
/// the target's pairs it with page 2, so which one survived is read off one
/// answer; and only the case where a person's correction is left behind is
/// counted.
#[test]
fn where_both_works_align_one_pair_a_hand_correction_wins_and_otherwise_the_target_keeps_its_own() {
    // (incoming holds a hand run, the target's does, the target's is stored
    //  reversed, where page 2 lands, alignments left behind and counted)
    let cases = [
        (true, false, true, 3, 0),
        (true, false, false, 3, 0),
        (false, true, true, 2, 0),
        (true, true, false, 2, 1),
        (false, false, true, 2, 0),
    ];
    for (incoming_hand, held_hand, reversed, lands, uncarried) in cases {
        let conn = open();
        seed_paged_work(
            &conn,
            1,
            "target",
            "rtl",
            &[
                ("raw", "aaa", 8, &[]),
                ("full-color", "bbb", 8, &[1]),
                ("en-color", "ccc", 8, &[]),
            ],
        );
        seed_paged_work(
            &conn,
            2,
            "superseded",
            "rtl",
            &[("colour", "bbb", 8, &[1]), ("en", "ccc", 8, &[])],
        );
        write_alignments(
            &conn,
            2,
            &[(
                "colour",
                "en",
                vec![
                    run("0-1R", Some("0-1"), false),
                    run("1L", None, false),
                    run("2", Some("3"), incoming_hand),
                    run("3", None, false),
                    run("4-7", Some("4-7"), false),
                ],
            )],
        );
        let held = if reversed {
            (
                "en-color",
                "full-color",
                vec![
                    run("0-1", Some("0-1R"), false),
                    run("2-7", Some("2-7"), held_hand),
                ],
            )
        } else {
            (
                "full-color",
                "en-color",
                vec![
                    run("0-1R", Some("0-1"), false),
                    run("1L", None, false),
                    run("2-7", Some("2-7"), held_hand),
                ],
            )
        };
        write_alignments(&conn, 1, &[held]);

        let moved = folded(&conn, 2, 1);

        let case =
            format!("incoming hand {incoming_hand}, target hand {held_hand}, reversed {reversed}");
        assert_eq!(
            (
                resolves(&conn, 1, "full-color", "en-color", whole(2)),
                moved.uncarried_alignments,
                alignments_on(&conn, 1).len(),
            ),
            (Some(whole(lands)), uncarried, 1),
            "{case}"
        );
        let orphaned: i64 = conn
            .query_row(
                "SELECT count(*) FROM run WHERE alignment_id NOT IN (SELECT id FROM alignment)",
                [],
                |r| r.get(0),
            )
            .expect("counting runs");
        assert_eq!(
            orphaned, 0,
            "{case}: a replaced alignment's runs go with it"
        );
    }
}

/// **An alignment between two editions the fold makes one is not carried** —
/// the map is non-injective, and an edge from an edition to itself is no edge.
/// A person's correction on one is counted; a solver's is not.
#[test]
fn an_alignment_between_two_editions_the_fold_makes_one_is_left_and_a_hand_one_is_counted() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 4, &[]), ("full-color", "bbb", 4, &[])],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "rtl",
        &[("colour", "bbb", 4, &[1]), ("colour-copy", "bbb", 4, &[1])],
    );
    write_alignments(
        &conn,
        2,
        &[
            ("colour", "colour-copy", vec![run("0-3", Some("0-3"), true)]),
            (
                "colour-copy",
                "colour",
                vec![run("0-3", Some("0-3"), false)],
            ),
        ],
    );

    let moved = folded(&conn, 2, 1);

    assert_eq!(
        (
            moved.uncarried_alignments,
            alignments_on(&conn, 1),
            spreads_on(&conn, 1, "full-color")
        ),
        (1, vec![], vec![1]),
        "neither is carried, the hand one is counted, and the flags both editions agree on are written"
    );
}

/// **Two folded editions meeting on one twin with different spread flags give
/// it neither**, and every row on either is still placed by page and half.
#[test]
fn folded_editions_that_disagree_about_spreads_give_the_twin_neither() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, &[])],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "rtl",
        &[("colour", "bbb", 6, &[1]), ("colour-copy", "bbb", 6, &[3])],
    );
    seed_note(&conn, 2, "colour-copy", 5);

    folded(&conn, 2, 1);

    assert_eq!(
        (
            spreads_on(&conn, 1, "full-color"),
            page_at(&conn, 1, "full-color", note_slot(&conn, 1, "full-color")),
        ),
        (vec![], Some(whole(4))),
        "no flags written, and the note on page 4 still on page 4"
    );
}

/// **A twin that already holds a different spread set keeps it**, and what
/// depended on the folded edition's flags is placed, counted or refused. A
/// place on page 2 is slot 3 on the folded edition and slot 2 on the twin, and
/// lands on slot 2. The alignment's first span ends on the right half of
/// page 1, which the twin does not have, so it is left behind and counted.
#[test]
fn a_twin_holding_other_spread_flags_keeps_them_and_places_what_it_can() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[
            ("raw", "aaa", 8, &[]),
            ("full-color", "bbb", 8, &[4]),
            ("en-color", "ccc", 8, &[]),
        ],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "rtl",
        &[("colour", "bbb", 8, &[1]), ("en", "ccc", 8, &[])],
    );
    place(&conn, 2, 1, "colour", 3);
    write_alignments(
        &conn,
        2,
        &[(
            "colour",
            "en",
            vec![
                run("0-1R", Some("0-1"), true),
                run("1L", None, false),
                run("2-7", Some("2-7"), false),
            ],
        )],
    );

    let moved = folded(&conn, 2, 1);

    let slot = position_slot(&conn, 1, 1, "full-color");
    assert_eq!(
        (
            spreads_on(&conn, 1, "full-color"),
            slot,
            page_at(&conn, 1, "full-color", slot),
            moved.uncarried_alignments,
            alignments_on(&conn, 1),
        ),
        (vec![4], 2, Some(whole(2)), 1, vec![]),
        "the twin's own flags stand, the place is on page 2, and the alignment is left and counted"
    );
}

/// **Where a reader's note has no slot on the twin, the fold is refused and
/// writes nothing.** The right half of page 1 exists on the folded edition,
/// which joins page 1, and not on a twin that joins page 4 instead.
#[test]
fn a_note_on_a_half_the_twin_does_not_have_refuses_the_fold() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 8, &[]), ("full-color", "bbb", 8, &[4])],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 8, &[1])]);
    seed_note(&conn, 2, "colour", 1);

    let why = refusal(&conn, 2, 1);

    // Whole, because a person reads it: it reaches `DeriveReport::unfoldable`,
    // and a `contains` would not see a run of spaces left where a line
    // continuation was lost.
    assert_eq!(
        why,
        "folding work 2 into work 1 would leave 1 row(s) naming another page — work 2's \
         `colour` (1 note); the two works' editions do not present those pages as the same \
         slots, and no slot on the surviving edition names the same page and half; nothing \
         was moved and nothing was deleted"
    );
    assert_eq!(
        (
            note_slot(&conn, 2, "colour"),
            spreads_on(&conn, 1, "full-color")
        ),
        (1, vec![4]),
        "the note is where it was and the twin's flags are untouched"
    );
}

/// **The twin's own rows were written under the slots it had before the fold
/// gave it flags, and they are placed too.** A second reader's place on page 5
/// is slot 5 on a twin holding no flags and slot 6 once page 3 is a spread; a
/// note on page 7 goes from 7 to 8; and the target's own alignment, whose spans
/// never cover page 3, reads the same afterwards and stays. The folded work's
/// place on page 6 is slot 7 already, and must not be placed a second time.
#[test]
fn the_twins_own_rows_are_placed_under_the_flags_it_takes() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[
            ("raw", "aaa", 8, &[]),
            ("full-color", "bbb", 8, &[]),
            ("en-color", "ccc", 8, &[]),
        ],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 8, &[3])]);
    place(&conn, 1, 2, "full-color", 5);
    seed_note(&conn, 1, "full-color", 7);
    place(&conn, 2, 1, "colour", 7);
    write_alignments(
        &conn,
        1,
        &[(
            "en-color",
            "full-color",
            vec![
                run("0-2", Some("0-2"), true),
                run("3", None, false),
                run("4-7", Some("4-7"), false),
            ],
        )],
    );

    folded(&conn, 2, 1);

    let pages = [
        position_slot(&conn, 1, 2, "full-color"),
        note_slot(&conn, 1, "full-color"),
        position_slot(&conn, 1, 1, "full-color"),
    ]
    .map(|slot| page_at(&conn, 1, "full-color", slot));
    assert_eq!(
        (spreads_on(&conn, 1, "full-color"), pages),
        (vec![3], [Some(whole(5)), Some(whole(7)), Some(whole(6))]),
        "the flag is written, and the target's place, its note and the carried place each name their page"
    );
    assert_eq!(
        (
            alignments_on(&conn, 1),
            resolves(&conn, 1, "en-color", "full-color", whole(5)),
        ),
        (
            vec![("en-color".to_string(), "full-color".to_string())],
            Some(whole(5))
        ),
        "the target's own alignment stands and still answers"
    );
}

/// **A row of the twin's own that the new flags would split is refused over,
/// and so is an alignment of its own whose span they would lengthen.** A place
/// on page 3, a whole page before the fold, has no whole-page slot once page 3
/// is a spread; an alignment pairing `0-7` with `0-7` would pair nine slots
/// with eight.
#[test]
fn a_twin_row_or_alignment_the_new_flags_would_move_refuses_the_fold() {
    for (what, expected) in [("progress", "(1 progress)"), ("alignment", "(1 alignment)")] {
        let conn = open();
        seed_paged_work(
            &conn,
            1,
            "target",
            "rtl",
            &[
                ("raw", "aaa", 8, &[]),
                ("full-color", "bbb", 8, &[]),
                ("en-color", "ccc", 8, &[]),
            ],
        );
        seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 8, &[3])]);
        if what == "progress" {
            place(&conn, 1, 2, "full-color", 3);
        } else {
            write_alignments(
                &conn,
                1,
                &[(
                    "en-color",
                    "full-color",
                    vec![run("0-7", Some("0-7"), false)],
                )],
            );
        }

        let why = refusal(&conn, 2, 1);

        assert!(
            why.contains(&format!("work 1's `full-color` {expected}")),
            "{what}: {why}"
        );
        assert_eq!(
            spreads_on(&conn, 1, "full-color"),
            Vec::<u32>::new(),
            "{what}: no flag written"
        );
    }
}

/// **A half is a side of the scan, not a position in reading order.** The
/// folded work reads left to right, so its slots over a spread at page 1 are
/// `0, 1L, 1R, 2, 3`; the target reads right to left, `0, 1R, 1L, 2, 3`. A
/// note on the left half and a place on the right half keep their halves and
/// swap slot numbers, and an alignment span running `1L-1R` runs backwards on
/// the target, so it is left behind and counted.
#[test]
fn across_reading_directions_a_half_keeps_its_side() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[
            ("raw", "aaa", 4, &[]),
            ("full-color", "bbb", 4, &[]),
            ("en-color", "ccc", 4, &[]),
        ],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "ltr",
        &[("colour", "bbb", 4, &[1]), ("en", "ccc", 4, &[])],
    );
    seed_note(&conn, 2, "colour", 1);
    place(&conn, 2, 1, "colour", 2);
    write_alignments(
        &conn,
        2,
        &[(
            "colour",
            "en",
            vec![
                run("0", Some("0"), false),
                run("1L-1R", Some("1-2"), true),
                run("2", Some("3"), false),
                run("3", None, false),
            ],
        )],
    );

    let moved = folded(&conn, 2, 1);

    let (note, position) = (
        note_slot(&conn, 1, "full-color"),
        position_slot(&conn, 1, 1, "full-color"),
    );
    assert_eq!(
        (
            (note, page_at(&conn, 1, "full-color", note)),
            (position, page_at(&conn, 1, "full-color", position)),
            moved.uncarried_alignments,
        ),
        ((2, Some(left(1))), (1, Some(right(1))), 1),
        "each half keeps its side, and the alignment that cannot is counted"
    );
}

/// **Two copies of one note are compared where each will stand, not as
/// stored.** The folded edition joins page 1, its twin does not yet, and the
/// fold gives it the flag: a copy on page 2 is slot 3 on one work and slot 2
/// on the other, and is one note; a copy stored as slot 3 on both is page 2 on
/// one and page 3 on the other, and is two.
#[test]
fn note_copies_are_one_note_by_the_page_they_name_not_the_number_they_hold() {
    for (target_slot, identical) in [(2_u32, true), (3, false)] {
        let conn = open();
        seed_paged_work(
            &conn,
            1,
            "target",
            "rtl",
            &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, &[])],
        );
        seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 6, &[1])]);
        seed_note(&conn, 2, "colour", 3);
        seed_note(&conn, 1, "full-color", target_slot);

        let outcome = absorb(&conn, 2, 1).expect("folding");

        match (identical, outcome) {
            (true, Outcome::Folded(moved)) => assert_eq!(
                (
                    moved.dropped_notes,
                    page_at(&conn, 1, "full-color", note_slot(&conn, 1, "full-color")),
                ),
                (1, Some(whole(2))),
                "one note, on page 2, kept once"
            ),
            (false, Outcome::Refused(why)) => {
                assert!(why.contains("whose two copies differ"), "{why}");
            }
            (identical, outcome) => {
                panic!("stored as slot {target_slot}, identical {identical}: {outcome:?}")
            }
        }
    }
}

/// **A `b` span is held to the same test as an `a` span.** The twin of `en`
/// already joins page 2 rather than page 1, so a run pairing `1-2` with
/// `1R-1L` names halves the twin does not have, and the alignment is left and
/// counted even though every `a` span reads the same.
#[test]
fn an_alignment_whose_b_span_the_twin_cannot_hold_is_left_and_counted() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[
            ("raw", "aaa", 4, &[]),
            ("full-color", "bbb", 4, &[]),
            ("en-color", "ccc", 4, &[2]),
        ],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "rtl",
        &[("colour", "bbb", 4, &[]), ("en", "ccc", 4, &[1])],
    );
    write_alignments(
        &conn,
        2,
        &[(
            "colour",
            "en",
            vec![
                run("0", Some("0"), false),
                run("1-2", Some("1R-1L"), true),
                run("3", Some("3"), false),
            ],
        )],
    );

    let moved = folded(&conn, 2, 1);

    assert_eq!(
        (moved.uncarried_alignments, alignments_on(&conn, 1)),
        (1, vec![]),
        "left behind and counted"
    );
}

/// A note row as `(slot, page it names)`, by its own key.
fn note_page(
    conn: &Connection,
    user_id: i64,
    work_id: i64,
    id: &str,
    edition: &str,
) -> (i64, Option<taiyaku_core::Slot>) {
    let slot: i64 = conn
        .query_row(
            "SELECT slot FROM note WHERE user_id = ?1 AND work_id = ?2 AND id = ?3",
            params![user_id, work_id, id],
            |r| r.get(0),
        )
        .expect("the note");
    (slot, page_at(conn, work_id, edition, slot))
}

fn insert_note(conn: &Connection, user_id: i64, work_id: i64, id: &str, edition: &str, slot: i64) {
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (?1, ?2, ?3, ?4, ?5, 'a note', '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')",
        params![user_id, id, work_id, edition, slot],
    )
    .expect("a note");
}

/// **Every statement that places a slot touches its own row and no other.**
/// The twin takes a spread at page 3, so every row on it past page 3 moves one
/// slot. Two readers sit on it at different pages; the first reader also has a
/// place and a note on the target's other edition, and a place and a note under
/// the same edition key and the same note id on an unrelated work. Every slot
/// differs, so a `progress` or `note` statement missing any one of its user,
/// work, edition or id writes one row's slot over another's, and a spread
/// write missing its work writes a flag onto the unrelated work.
///
/// **The unrelated work's rows sit past page 3 as well**, where placing them
/// under the twin's map would move them. A row a read wrongly reaches is only
/// written when its slot would change.
#[test]
fn every_statement_that_places_a_slot_touches_only_its_own_row() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 8, &[]), ("full-color", "bbb", 8, &[])],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 8, &[3])]);
    seed_paged_work(
        &conn,
        3,
        "unrelated",
        "rtl",
        &[("full-color", "zzz", 8, &[])],
    );
    place(&conn, 1, 1, "full-color", 5);
    place(&conn, 1, 2, "full-color", 7);
    place(&conn, 1, 1, "raw", 4);
    place(&conn, 3, 1, "full-color", 4);
    // In this order, so a read that lost its work reaches the unrelated work's
    // copy after the target's and writes it last.
    insert_note(&conn, 1, 1, "01JSAMEID", "full-color", 4);
    insert_note(&conn, 2, 1, "01JSAMEID", "full-color", 6);
    insert_note(&conn, 1, 1, "01JOTHERID", "raw", 6);
    insert_note(&conn, 1, 3, "01JSAMEID", "full-color", 7);

    folded(&conn, 2, 1);

    let position = |work_id: i64, user_id: i64, edition: &str| {
        let slot = position_slot(&conn, work_id, user_id, edition);
        (slot, page_at(&conn, work_id, edition, slot))
    };
    assert_eq!(
        [
            position(1, 1, "full-color"),
            position(1, 2, "full-color"),
            position(1, 1, "raw"),
            position(3, 1, "full-color"),
        ],
        [
            (6, Some(whole(5))),
            (8, Some(whole(7))),
            (4, Some(whole(4))),
            (4, Some(whole(4))),
        ],
        "each place names its own page: two readers on the twin, the target's other edition, an unrelated work"
    );
    assert_eq!(
        [
            note_page(&conn, 1, 1, "01JSAMEID", "full-color"),
            note_page(&conn, 2, 1, "01JSAMEID", "full-color"),
            note_page(&conn, 1, 1, "01JOTHERID", "raw"),
            note_page(&conn, 1, 3, "01JSAMEID", "full-color"),
        ],
        [
            (5, Some(whole(4))),
            (7, Some(whole(6))),
            (6, Some(whole(6))),
            (7, Some(whole(7))),
        ],
        "each note names its own page, by reader, work, edition and id"
    );
    assert_eq!(
        (
            spreads_on(&conn, 1, "full-color"),
            spreads_on(&conn, 3, "full-color")
        ),
        (vec![3], vec![]),
        "the flag went onto the twin and not onto the unrelated work's edition of the same key"
    );
}

/// **A row whose slot names no page before the fold is carried as it was, and
/// only a row that named a page can refuse.** `taiyaku-serve`'s
/// `notes::validate` keeps a note stored past its edition's end on purpose.
/// Three shapes: a note past the end on a twin that takes flags, a place past
/// the end on a folded edition whose twin holds other flags, and the first
/// again with a place on a page the flags split, which refuses and names that
/// place alone.
#[test]
fn a_row_naming_no_page_is_carried_as_it_was_and_only_a_row_naming_one_refuses() {
    for shape in ["twin", "folded", "refused"] {
        let conn = open();
        let twin_flags: &[u32] = if shape == "folded" { &[4] } else { &[] };
        seed_paged_work(
            &conn,
            1,
            "target",
            "rtl",
            &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, twin_flags)],
        );
        seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 6, &[2])]);
        if shape == "folded" {
            conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, 2, 'colour', 40)",
                [],
            )
            .expect("a place past the end of the folded edition");
        } else {
            insert_note(&conn, 1, 1, "01JPASTTHEEND", "full-color", 40);
        }
        if shape == "refused" {
            place(&conn, 1, 2, "full-color", 2);
        }

        match shape {
            "refused" => {
                let why = refusal(&conn, 2, 1);
                assert!(why.contains("would leave 1 row(s)"), "{why}");
                assert!(why.contains("work 1's `full-color` (1 progress)"), "{why}");
            }
            "twin" => {
                folded(&conn, 2, 1);
                assert_eq!(
                    (
                        note_page(&conn, 1, 1, "01JPASTTHEEND", "full-color"),
                        spreads_on(&conn, 1, "full-color")
                    ),
                    ((40, None), vec![2]),
                    "the note past the end stays past the end, and the twin takes the flag"
                );
            }
            _ => {
                folded(&conn, 2, 1);
                assert_eq!(
                    position_slot(&conn, 1, 1, "full-color"),
                    40,
                    "the place past the end moved as it was"
                );
            }
        }
    }
}

/// **A folded edition holding no spread flags is unmeasured, and does not
/// disagree with one that holds them.** Two copies of one archive meet on a
/// twin holding none: `colour` measured a spread at page 2, `colour-copy` holds
/// nothing, because a scan never writes a flag. The twin takes `[2]`. A note on
/// the unmeasured copy on page 4 is placed under it; one on page 2 — a whole
/// page there, half of a spread under the flags — refuses the fold.
#[test]
fn a_folded_edition_holding_no_flags_does_not_disagree_with_one_that_holds_them() {
    for note_page_number in [4_u32, 2] {
        let conn = open();
        seed_paged_work(
            &conn,
            1,
            "target",
            "rtl",
            &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, &[])],
        );
        seed_paged_work(
            &conn,
            2,
            "superseded",
            "rtl",
            &[("colour", "bbb", 6, &[2]), ("colour-copy", "bbb", 6, &[])],
        );
        seed_note(&conn, 2, "colour-copy", note_page_number);

        if note_page_number == 4 {
            folded(&conn, 2, 1);
            let slot = note_slot(&conn, 1, "full-color");
            assert_eq!(
                (
                    spreads_on(&conn, 1, "full-color"),
                    slot,
                    page_at(&conn, 1, "full-color", slot)
                ),
                (vec![2], 5, Some(whole(4))),
                "the measured flag is taken and the unmeasured copy's note lands on its page"
            );
        } else {
            let why = refusal(&conn, 2, 1);
            assert!(why.contains("work 2's `colour-copy` (1 note)"), "{why}");
        }
    }
}

/// **A fold refused at its last arm, after the slot plan has something to
/// write, writes none of it.** The twin would take a spread, the second
/// reader's place on it would move, and an alignment would be carried — and
/// the ground truth meeting on the twin was drawn against different archives,
/// so the fold refuses at the sixth arm. With every table the plan writes made
/// to raise, a write placed above that refusal fails this test; the rollback
/// alone would hide it.
#[test]
fn a_fold_refused_after_its_slot_plan_has_writes_makes_none_of_them() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[
            ("raw", "aaa", 8, &[]),
            ("full-color", "bbb", 8, &[]),
            ("en-color", "ccc", 8, &[]),
        ],
    );
    seed_paged_work(
        &conn,
        2,
        "superseded",
        "rtl",
        &[("colour", "bbb", 8, &[3]), ("en", "ccc", 8, &[])],
    );
    place(&conn, 1, 2, "full-color", 5);
    place(&conn, 2, 1, "colour", 7);
    write_alignments(
        &conn,
        2,
        &[(
            "colour",
            "en",
            vec![
                run("0-3R", Some("0-3"), true),
                run("3L", None, false),
                run("4-7", Some("4-7"), false),
            ],
        )],
    );
    seed_truth(&conn, 2, "colour", 0, "drawn-on-the-folded-copy");
    seed_truth(&conn, 1, "full-color", 0, "drawn-on-the-twin");

    let why = refusal(&conn, 2, 1);

    assert!(why.contains("cannot be given one provenance"), "{why}");
    assert_eq!(
        (
            spreads_on(&conn, 1, "full-color"),
            position_slot(&conn, 1, 2, "full-color"),
            alignments_on(&conn, 1)
        ),
        (vec![], 5, vec![]),
        "nothing the plan would have written is there"
    );
}

/// Reader 1's position on `work_id`'s edition `key`: its four `at_` columns,
/// read by SQL, so a check of what the fold wrote is not a check of what a
/// read answers from it.
fn position_anchor(
    conn: &Connection,
    work_id: i64,
    key: &str,
) -> (Option<String>, Option<i64>, Option<i64>, Option<String>) {
    conn.query_row(
        "SELECT at_sha256, at_nth, at_page, at_half FROM progress
          WHERE user_id = 1 AND work_id = ?1 AND edition = ?2",
        params![work_id, key],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .expect("the position's anchor")
}

/// Join `page` of `work_id`'s edition `key` into a spread after the rows on
/// it were written: a layout change under them.
fn join_spread(conn: &Connection, work_id: i64, key: &str, page: u32) {
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'spread', ?3 FROM edition WHERE work_id = ?1 AND key = ?2",
        params![work_id, key, page],
    )
    .expect("a spread flag");
}

/// Reader 1's position on `work_id`'s edition `key` as `state::progress`
/// answers it.
fn answered_position(conn: &Connection, work_id: i64, key: &str) -> u32 {
    progress(conn, work_id, 1)
        .expect("reads the positions")
        .into_iter()
        .find(|position| position.edition == key)
        .expect("a position on the edition")
        .slot
}

/// **A moved position keeps the page it names (O244)**, inserted onto a twin
/// holding no position there and updating an older one on page 3 alike. The
/// folded work's place is on page 5. After the fold the twin's page 2 is
/// joined into a spread, so page 5 is slot 6 and the stored 5 names page 4.
/// A row moved without its four `at_` columns answers 5 from its stored slot
/// on the insert, and 4, page 3's slot, from the older row's anchor on the
/// update.
#[test]
fn a_moved_position_keeps_the_page_it_names_through_a_later_layout_change() {
    for older in [false, true] {
        let conn = open();
        seed_paged_work(
            &conn,
            1,
            "target",
            "rtl",
            &[("raw", "aaa", 8, &[]), ("full-color", "bbb", 8, &[])],
        );
        seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 8, &[])]);
        if older {
            crate::state::put_progress_at(
                &conn,
                1,
                1,
                &Progress {
                    edition: "full-color".into(),
                    slot: 3,
                },
                Some("2000-01-01 00:00:00"),
                crate::state::Numbering::Store,
            )
            .expect("an older place on the twin, on page 3");
        }
        place(&conn, 2, 1, "colour", 5);
        let carried = position_anchor(&conn, 2, "colour");
        assert_eq!(
            carried,
            (Some("bbb".to_string()), Some(0), Some(5), None),
            "older {older}: the fixture: the folded work's place names page 5"
        );

        let moved = folded(&conn, 2, 1);
        join_spread(&conn, 1, "full-color", 2);

        assert_eq!(
            (
                (moved.positions, moved.lost_positions),
                position_anchor(&conn, 1, "full-color"),
                answered_position(&conn, 1, "full-color"),
            ),
            ((1, usize::from(older)), carried, 6),
            "older {older}: the place moved with its four columns and answers page 5's slot"
        );
    }
}

/// **A fold plans from the slot a row answers, not the number it holds**
/// (O244, decision 9). The folded work's place was written on page 3, slot 3,
/// and page 2 was joined into a spread after, so its stored 3 names page 2's
/// left half and it answers slot 4, page 3. The twin keeps its own flag on
/// page 4: page 2 is whole there and page 3 is slot 3. Planned from the
/// stored 3, the place names a half the twin does not hold, and the fold is
/// refused.
#[test]
fn a_fold_places_a_row_by_the_slot_it_answers() {
    let conn = open();
    seed_paged_work(&conn, 1, "target", "rtl", &[("raw", "xxx", 6, &[4])]);
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("raw", "xxx", 6, &[])]);
    place(&conn, 2, 1, "raw", 3);
    join_spread(&conn, 2, "raw", 2);
    assert_eq!(
        (
            page_at(&conn, 2, "raw", 3),
            answered_position(&conn, 2, "raw")
        ),
        (Some(left(2)), 4),
        "the fixture: the stored slot names page 2's left half, and the place answers page 3's"
    );

    folded(&conn, 2, 1);

    let slot = answered_position(&conn, 1, "raw");
    assert_eq!(
        (slot, page_at(&conn, 1, "raw", i64::from(slot))),
        (3, Some(whole(3))),
        "the place is on page 3 of the twin"
    );
}

/// **The slot a fold writes is placed from the slot the row answers** (O244):
/// it is the row's fallback, and names the page its anchor does. A place and
/// a note were written on page 5 of the folded work, and page 2 was joined
/// after, so each stored 5 names page 4 and each answers slot 6. The twin
/// keeps its own flag on page 6, where page 5 is slot 5 and page 4 is slot 4.
/// Placed from the stored 5, each row's slot would be written as 4, page 4.
#[test]
fn the_slot_a_fold_writes_names_the_page_the_row_answers() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 8, &[]), ("full-color", "bbb", 8, &[6])],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 8, &[])]);
    place(&conn, 2, 1, "colour", 5);
    seed_note(&conn, 2, "colour", 5);
    join_spread(&conn, 2, "colour", 2);
    let answered_note = |work_id: i64| {
        crate::state::notes(&conn, work_id, 1)
            .expect("reads the notes")
            .notes[0]
            .slot
    };
    assert_eq!(
        (
            page_at(&conn, 2, "colour", 5),
            answered_position(&conn, 2, "colour"),
            answered_note(2),
        ),
        (Some(whole(4)), 6, 6),
        "the fixture: each stored 5 names page 4, and each row answers page 5's slot"
    );

    folded(&conn, 2, 1);

    let (position, note) = (
        position_slot(&conn, 1, 1, "full-color"),
        note_slot(&conn, 1, "full-color"),
    );
    assert_eq!(
        (
            page_at(&conn, 1, "full-color", position),
            page_at(&conn, 1, "full-color", note),
            answered_position(&conn, 1, "full-color"),
            answered_note(1),
        ),
        (Some(whole(5)), Some(whole(5)), 5, 5),
        "each stored slot names page 5 of the twin, and each row answers it"
    );
}

/// **A fold places a position and a note each by its own rule** (O244,
/// decision 6). Both were written on page 2's left half, and the folded
/// edition's flag on page 2 was cleared after, so each stored 3 names page 3.
/// The position answers page 2's slot, 2; the note keeps its stored 3,
/// `stale`. The twin keeps its own flag on page 4. Placed by the position's
/// rule the note would be rewritten to 2, and placed by the note's rule the
/// position's fallback would stay 3, page 3.
#[test]
fn a_fold_places_a_position_and_a_stale_note_each_by_its_own_rule() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, &[4])],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 6, &[2])]);
    seed_note(&conn, 2, "colour", 3);
    place(&conn, 2, 1, "colour", 3);
    conn.execute(
        "DELETE FROM page_flag WHERE edition_id IN (SELECT id FROM edition WHERE work_id = 2)",
        [],
    )
    .expect("clears the folded edition's flag");
    let answered_notes = |work_id: i64| {
        crate::state::notes(&conn, work_id, 1)
            .expect("reads the notes")
            .notes
            .into_iter()
            .map(|note| (note.slot, note.stale))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        (
            page_at(&conn, 2, "colour", 3),
            answered_notes(2),
            answered_position(&conn, 2, "colour"),
        ),
        (Some(whole(3)), vec![(3, true)], 2),
        "the fixture: the stored 3 names page 3; the note is stale there and the place answers page 2"
    );

    folded(&conn, 2, 1);

    assert_eq!(
        (
            answered_notes(1),
            position_slot(&conn, 1, 1, "full-color"),
            answered_position(&conn, 1, "full-color"),
        ),
        (vec![(3, true)], 2, 2),
        "the note stays at its stored 3, stale, and the place's stored slot and answer are page 2's"
    );
}

/// **A place whose page is gone is planned from its stored slot, as a note
/// is, not from the clamp a read answers** (O244). Each place is stored at 40,
/// past the end, on an archive the edition no longer holds, so a read clamps
/// it to the last slot. Planned from that clamp, the folded work's last half
/// that the twin does not split, and a twin's last page its new flag splits,
/// would each refuse the fold, and a twin's last page its new flag moves
/// would have its stored 40 rewritten. Each is carried at 40, and a read
/// clamps it there.
#[test]
fn a_lost_place_past_the_end_is_planned_as_stored_not_from_its_clamp() {
    let cases = [
        (true, &[1_u32][..], &[5_u32][..], (6_u32, left(5))),
        (false, &[][..], &[5][..], (5, whole(5))),
        (false, &[][..], &[1][..], (5, whole(5))),
    ];
    let mut outcomes = Vec::new();
    for (on_folded, twin_flags, folded_flags, before) in cases {
        let conn = open();
        seed_paged_work(
            &conn,
            1,
            "target",
            "rtl",
            &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, twin_flags)],
        );
        seed_paged_work(
            &conn,
            2,
            "superseded",
            "rtl",
            &[("colour", "bbb", 6, folded_flags)],
        );
        let (work_id, key) = if on_folded {
            (2, "colour")
        } else {
            (1, "full-color")
        };
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot, at_sha256, at_nth, at_page)
             VALUES (1, ?1, ?2, 40, 'an archive no longer held', 0, 0)",
            params![work_id, key],
        )
        .expect("a place on a lost file");
        let answered = answered_position(&conn, work_id, key);
        assert_eq!(
            (answered, page_at(&conn, work_id, key, i64::from(answered))),
            (before.0, Some(before.1)),
            "the fixture: on the folded work {on_folded}, the place reads as the last slot"
        );

        outcomes.push(match absorb(&conn, 2, 1).expect("folding") {
            Outcome::Folded(_) => {
                let answered = answered_position(&conn, 1, "full-color");
                Ok((
                    position_slot(&conn, 1, 1, "full-color"),
                    answered,
                    page_at(&conn, 1, "full-color", i64::from(answered)),
                ))
            }
            Outcome::Refused(why) => Err(why),
        });
    }
    assert_eq!(
        outcomes,
        [
            Ok((40, 6, Some(whole(5)))),
            Ok((40, 6, Some(left(5)))),
            Ok((40, 6, Some(whole(5)))),
        ],
        "each fold goes ahead, the place stored at 40 and read as the twin's last slot"
    );
}

/// **Two copies of a note are one by the slot each answers, and their
/// anchors are not compared** (O244; O78). Both works hold `xxx`. The folded
/// work's copy was written on page 3 and page 1 was joined after, so its
/// stored 3 names page 2 and it answers slot 4. The twin's copy holds no
/// anchor, as a row written before anchors does, at slot 3, page 3; the twin
/// takes the flag, so it is placed from 3 to 4. One note on page 3: the folded
/// work's copy is dropped and the twin's row stays. Compared at the stored 3,
/// or with the anchors compared, the copies differ and the fold is refused.
#[test]
fn note_copies_are_one_note_by_the_slot_each_answers_whatever_their_anchors() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 6, &[]), ("full-color", "xxx", 6, &[])],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "xxx", 6, &[])]);
    write_notes(
        &conn,
        2,
        1,
        vec![Note {
            id: SHARED.to_string(),
            edition: "colour".to_string(),
            slot: 3,
            region: None,
            color: None,
            body: "a note".to_string(),
            created: "2026-09-15T00:00:00Z".to_string(),
            updated: "2026-09-15T00:00:00Z".to_string(),
            stale: false,
            at: None,
        }],
    );
    join_spread(&conn, 2, "colour", 1);
    insert_note(&conn, 1, 1, SHARED, "full-color", 3);
    let answered = |work_id: i64| {
        crate::state::notes(&conn, work_id, 1)
            .expect("reads the notes")
            .notes
            .into_iter()
            .map(|note| (note.slot, note.stale))
            .collect::<Vec<_>>()
    };
    let twins_row = every_note(&conn)
        .into_iter()
        .find(|row| row.work_id == 1)
        .expect("the twin's copy")
        .rowid;
    assert_eq!(
        (page_at(&conn, 2, "colour", 3), answered(2), answered(1)),
        (Some(whole(2)), vec![(4, false)], vec![(3, false)]),
        "the fixture: the folded copy's stored 3 names page 2 and it answers 4; the twin's answers 3"
    );

    let moved = folded(&conn, 2, 1);

    let left: Vec<(i64, i64)> = every_note(&conn)
        .into_iter()
        .map(|row| (row.rowid, row.work_id))
        .collect();
    assert_eq!(
        (
            moved.dropped_notes,
            left,
            answered(1),
            page_at(&conn, 1, "full-color", 4)
        ),
        (1, vec![(twins_row, 1)], vec![(4, false)], Some(whole(3))),
        "one note, the twin's row, on page 3"
    );
}

/// A row whose anchor is part set fails the fold, naming the row, as
/// `state::progress` refuses it: no writer makes one, so the slot it answers
/// is not known.
#[test]
fn a_row_whose_anchor_is_part_set_fails_the_fold_naming_it() {
    let conn = open();
    seed_paged_work(
        &conn,
        1,
        "target",
        "rtl",
        &[("raw", "aaa", 6, &[]), ("full-color", "bbb", 6, &[])],
    );
    seed_paged_work(&conn, 2, "superseded", "rtl", &[("colour", "bbb", 6, &[1])]);
    place(&conn, 1, 1, "full-color", 4);
    conn.execute("UPDATE progress SET at_page = NULL", [])
        .expect("part-sets the anchor");

    assert_eq!(
        absorb(&conn, 2, 1).map(|_| ()),
        Err(
            "a `progress` row of work 1 on `full-color` has an anchor with only some of \
             at_sha256, at_nth and at_page set (at_sha256=Some(\"bbb\"), at_nth=Some(0), \
             at_page=None, at_half=None); an anchor is all three or none"
                .to_string()
        )
    );
}

/// The tables a fold moves rows between, the target's hand and effective
/// columns it carries values into and the markers it arms, as text rows, so
/// that a refusal that wrote to any of them is seen.
fn store_dump(conn: &Connection) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'work ' || id || ' ' || quote(title) || ' ' || quote(title_hand) || ' '
                || quote(volume) || ' ' || quote(volume_hand) || ' '
                || quote(series_id) || ' ' || quote(series_id_hand) FROM work ORDER BY 1",
        "SELECT 'migrated_document ' || work_id || ' ' || document || ' ' || absorbed
           FROM migrated_document ORDER BY 1",
        "SELECT 'edition ' || id || ' ' || work_id || ' ' || key FROM edition ORDER BY 1",
        "SELECT 'progress ' || work_id || ' ' || edition || ' ' || slot FROM progress ORDER BY 1",
        "SELECT 'note ' || work_id || ' ' || edition || ' ' || id FROM note ORDER BY 1",
        "SELECT 'truth_page ' || work_id || ' ' || edition || ' ' || page FROM truth_page ORDER BY 1",
        "SELECT 'truth_box ' || id || ' ' || work_id || ' ' || edition || ' ' || page FROM truth_box ORDER BY 1",
        "SELECT 'truth_member ' || work_id || ' ' || edition || ' ' || ordinal FROM truth_member ORDER BY 1",
        "SELECT 'alignment ' || id || ' ' || work_id || ' ' || from_key || ' ' || to_key FROM alignment ORDER BY 1",
        "SELECT 'run ' || alignment_id || ' ' || ordinal FROM run ORDER BY 1",
    ] {
        let mut statement = conn.prepare(sql).expect(sql);
        rows.extend(
            statement
                .query_map([], |r| r.get::<_, String>(0))
                .expect(sql)
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect(sql),
        );
    }
    rows
}

/// A series a hand-set `series_id_hand` can name.
fn seed_series(conn: &Connection, series_id: i64, title: &str) {
    conn.execute(
        "INSERT INTO series (id, sort_title) VALUES (?1, ?2)",
        params![series_id, title],
    )
    .expect("seeding a series");
}

/// One work's hand column and the effective column beside it, in that order.
fn hand_and_effective<T: rusqlite::types::FromSql>(
    conn: &Connection,
    work_id: i64,
    hand: &str,
    effective: &str,
) -> (Option<T>, Option<T>) {
    conn.query_row(
        &format!("SELECT {hand}, {effective} FROM work WHERE id = ?1"),
        [work_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .expect("the work's hand and effective columns")
}

/// Whether this work's `LIBRARY` marker is armed: `None` with no row at all.
fn library_marker(conn: &Connection, work_id: i64) -> Option<bool> {
    use rusqlite::OptionalExtension as _;
    conn.query_row(
        "SELECT absorbed FROM migrated_document WHERE work_id = ?1 AND document = 'library'",
        [work_id],
        |r| r.get(0),
    )
    .optional()
    .expect("reading the library marker")
}

/// **A title only the folded work holds is the target's afterwards**, in
/// `title` as well as `title_hand`: `librarydoc::apply` writes both, and the
/// target must read right before its next derive recomputes anything.
#[test]
fn a_hand_title_only_the_folded_work_holds_is_carried_onto_the_target() {
    let (conn, from, into) = folded_pair();
    conn.execute(
        "UPDATE work SET title_hand = 'By Hand', title = 'By Hand' WHERE id = ?1",
        [from],
    )
    .expect("a hand title on the work about to be folded");

    let moved = folded(&conn, from, into);

    assert_eq!(
        (
            hand_and_effective::<String>(&conn, into, "title_hand", "title"),
            moved.hand_carried
        ),
        ((Some("By Hand".into()), Some("By Hand".into())), 1),
        "the target holds the title, provenance and effective value both"
    );
}

#[test]
fn a_hand_volume_only_the_folded_work_holds_is_carried_onto_the_target() {
    let (conn, from, into) = folded_pair();
    conn.execute("UPDATE work SET volume = 1.0 WHERE id = ?1", [into])
        .expect("the volume the target's derive gave it");
    conn.execute(
        "UPDATE work SET volume_hand = 4.5, volume = 4.5 WHERE id = ?1",
        [from],
    )
    .expect("a hand volume on the work about to be folded");

    let moved = folded(&conn, from, into);

    assert_eq!(
        (
            hand_and_effective::<f64>(&conn, into, "volume_hand", "volume"),
            moved.hand_carried
        ),
        ((Some(4.5), Some(4.5)), 1),
        "the target holds the volume, provenance and effective value both"
    );
}

#[test]
fn a_hand_series_only_the_folded_work_holds_is_carried_onto_the_target() {
    let (conn, from, into) = folded_pair();
    seed_series(&conn, 10, "Derived");
    seed_series(&conn, 20, "Placed By Hand");
    conn.execute("UPDATE work SET series_id = 10 WHERE id = ?1", [into])
        .expect("the series the target's derive gave it");
    conn.execute(
        "UPDATE work SET series_id_hand = 20, series_id = 20 WHERE id = ?1",
        [from],
    )
    .expect("a hand series on the work about to be folded");

    let moved = folded(&conn, from, into);

    assert_eq!(
        (
            hand_and_effective::<i64>(&conn, into, "series_id_hand", "series_id"),
            moved.hand_carried
        ),
        ((Some(20), Some(20)), 1),
        "the target sits in the series a person placed the folded work in"
    );
}

/// **Two different answers about one work are refused, each of the three, and
/// the refusal writes nothing.** Every table the fold writes raises on a write
/// before the call, because a refusal rolls back and a write made before it
/// would otherwise be invisible to the comparison after.
#[test]
fn a_fold_between_two_works_holding_different_hand_values_is_refused_and_writes_nothing() {
    for (field, set_from, set_into, values) in [
        (
            "title",
            "title_hand = 'Folded Title', title = 'Folded Title'",
            "title_hand = 'Target Title', title = 'Target Title'",
            ["`Folded Title`", "`Target Title`"],
        ),
        (
            "volume",
            "volume_hand = 4.5, volume = 4.5",
            "volume_hand = 7.25, volume = 7.25",
            ["4.5", "7.25"],
        ),
        (
            "series",
            "series_id_hand = 20, series_id = 20",
            "series_id_hand = 10, series_id = 10",
            ["20 `Folded Series`", "10 `Target Series`"],
        ),
    ] {
        let (conn, from, into) = folded_pair();
        seed_series(&conn, 10, "Target Series");
        seed_series(&conn, 20, "Folded Series");
        conn.execute(&format!("UPDATE work SET {set_from} WHERE id = ?1"), [from])
            .expect("the folded work's hand value");
        conn.execute(&format!("UPDATE work SET {set_into} WHERE id = ?1"), [into])
            .expect("the target's own, different, hand value");
        conn.execute(
            "INSERT INTO migrated_document (work_id, document, absorbed) VALUES (?1, 'library', 1)",
            [from],
        )
        .expect("the folded work's armed marker");
        let before = store_dump(&conn);
        forbid_writes(&conn, &["work", "migrated_document"]);

        let why = refusal(&conn, from, into);

        for named in [
            format!("— {field} {} on work {from}", values[0]),
            format!("{} on work {into}", values[1]),
            "nothing was moved and nothing was deleted".to_string(),
        ] {
            assert!(
                why.contains(&named),
                "the {field} refusal names {named:?}: {why}"
            );
        }
        assert_eq!(
            store_dump(&conn),
            before,
            "the {field} refusal wrote nothing"
        );
        let standing: i64 = conn
            .query_row(
                "SELECT count(*) FROM work WHERE id IN (?1, ?2)",
                [from, into],
                |r| r.get(0),
            )
            .expect("counting the two works");
        assert_eq!(standing, 2, "both works stand after the {field} refusal");
    }
}

/// The same answer twice is one answer, for each of the three: the fold goes
/// ahead and carries nothing, since the target already holds it.
#[test]
fn a_fold_between_two_works_holding_the_same_hand_value_goes_ahead() {
    for (field, set_both, read) in [
        (
            "title",
            "title_hand = 'Agreed', title = 'Agreed'",
            "SELECT quote(title_hand) || ' ' || quote(title) FROM work WHERE id = ?1",
        ),
        (
            "volume",
            "volume_hand = 4.5, volume = 4.5",
            "SELECT quote(volume_hand) || ' ' || quote(volume) FROM work WHERE id = ?1",
        ),
        (
            "series",
            "series_id_hand = 20, series_id = 20",
            "SELECT quote(series_id_hand) || ' ' || quote(series_id) FROM work WHERE id = ?1",
        ),
    ] {
        let (conn, from, into) = folded_pair();
        seed_series(&conn, 20, "Agreed Series");
        conn.execute(
            &format!("UPDATE work SET {set_both} WHERE id IN (?1, ?2)"),
            [from, into],
        )
        .expect("one hand value on both works");
        let held: String = conn
            .query_row(read, [into], |r| r.get(0))
            .expect("the target's value before");

        let moved = match absorb(&conn, from, into).expect("folding") {
            Outcome::Folded(moved) => moved,
            Outcome::Refused(why) => panic!("the {field} fold was refused: {why}"),
        };

        let kept: String = conn
            .query_row(read, [into], |r| r.get(0))
            .expect("the target's value after");
        assert_eq!(
            (kept, moved.hand_carried),
            (held, 0),
            "the target keeps the {field} both held, and nothing needed carrying"
        );
    }
}

/// **A carried value takes the folded work's armed marker with it**, so the
/// target owes a document holding it: the value stood in the folded work's
/// document, and must stand in the target's to survive a rebuild from the
/// files.
#[test]
fn a_carried_hand_value_arms_the_targets_marker_where_the_folded_works_was_armed() {
    let (conn, from, into) = folded_pair();
    conn.execute(
        "UPDATE work SET title_hand = 'By Hand', title = 'By Hand' WHERE id = ?1",
        [from],
    )
    .expect("a hand title on the work about to be folded");
    conn.execute(
        "INSERT INTO migrated_document (work_id, document, absorbed) VALUES (?1, 'library', 1)",
        [from],
    )
    .expect("the folded work's armed marker");
    assert_eq!(
        library_marker(&conn, into),
        None,
        "the target starts unarmed"
    );

    folded(&conn, from, into);

    assert_eq!(library_marker(&conn, into), Some(true));
}

/// **Armed only where both halves hold**: a value carried, from a work whose
/// marker was armed. This pins `absorb`'s own arming, which the ruling keeps
/// for a pass that writes no document; under `Documents::Reconcile` the derive
/// saves the target's document after a carry, armed source or not.
#[test]
fn the_targets_marker_is_left_alone_unless_a_value_carried_from_an_armed_work() {
    let (conn, from, into) = folded_pair();
    conn.execute(
        "INSERT INTO migrated_document (work_id, document, absorbed) VALUES (?1, 'library', 1)",
        [from],
    )
    .expect("the folded work's armed marker");
    folded(&conn, from, into);
    assert_eq!(
        library_marker(&conn, into),
        None,
        "nothing carried, so the target's marker is untouched"
    );

    let (conn, from, into) = folded_pair();
    conn.execute(
        "UPDATE work SET title_hand = 'By Hand', title = 'By Hand' WHERE id = ?1",
        [from],
    )
    .expect("a hand title on the work about to be folded");
    folded(&conn, from, into);
    assert_eq!(
        library_marker(&conn, into),
        None,
        "carried from an unarmed work, so the target's marker is untouched"
    );
}
