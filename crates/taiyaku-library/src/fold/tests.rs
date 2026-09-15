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
/// **What the pragma actually changes here, measured on 2026-08-27 rather
/// than reasoned about**, because the obvious statement of it is wrong and
/// the numbers are worth having in front of a later reader:
///
/// - with it **on** and [`absorb`]'s `DELETE` moved above the moves,
///   `the_superseded_work_is_gone_and_took_nothing_with_it` reads
///   `(0, 0, 0, 0, 0)` where it wants `(1, 1, 1, 1, 1)` — the cascade took
///   the position, the note, the truth page, its box and its digests;
/// - with it **off** and the order correct, that same test reads
///   `(2, 1, 2, 2, 2)`: nothing cascades, so every source row stands beside
///   the copy that moved. Only `note` reads 1, being an `UPDATE` in place.
///
/// So the pragma being off does not make these tests pass silently; it makes
/// them fail for a reason that has nothing to do with what they assert, and
/// it makes `truth_page_refuses_to_be_moved_by_update` — the one that pins
/// why `move_truth` is not an `UPDATE` — fail outright, the composite key
/// being unenforced. The assertion is here so that reason is named at the
/// fixture instead of being rediscovered from two puzzling failures.
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
/// always writes exactly one member, which is why nothing reached this state
/// before.
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
/// `progress.edition` has no foreign key to `edition(key)` — which an earlier
/// version of this comment denied, calling it "impossible by construction,
/// the target's edition was derived by the scan that caused this fold". The
/// module doc corrects that; so does this fixture, which is the second route.
///
/// **The winner wins entire, `completed_at` and `read_count` included.** A
/// `DO UPDATE` setting `slot` and `updated` alone left a row belonging to
/// neither reading — the source's place under the target's re-read count —
/// and the two histories here are deliberately different so that arm cannot
/// come back unnoticed.
///
/// **And the row it wins against is a loss.** The target's older place, its
/// finish date and its re-read count are overwritten and gone; the previous
/// round counted only the *source* side and named this as a residual it had
/// not closed. `lost_positions` is a whole-fold identity precisely so that
/// the side the row was on stops mattering.
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
/// same slot number, different book, and — measured 2026-08-27 — accepted by
/// every constraint in the schema. Nothing but this assertion catches it.
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
/// **Measured 2026-08-27 and pinned here so a later refactor meets it as a
/// test rather than as a production failure.** `truth_box`'s foreign key
/// names `(work_id, edition, page)` with no `ON UPDATE`, so moving the page
/// row out from under a box is refused outright — with the edition rewritten
/// or without it, since the box's own `edition` moves with neither.
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
/// what this very derive just wrote. Choosing seven merge rules inside the
/// one function that deletes a work is more risk than the loss it avoids, and
/// every one of them is visible and re-doable by hand where a reading
/// position is not. So they are counted rather than moved, and this test is
/// what keeps the number honest.
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
        moved.discarded, 9,
        "one credit on the work, one on the edition, one alignment, one run, \
         one tag, one collection place, one page flag, one hand chapter number \
         and one hand-classified edition — each counted, none moved"
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
/// they are here rather than left out because they are what separates the two
/// predicates in [`super::DISCARDED`] that carry any logic at all from the
/// seven that are a plain `work_id`. `derive::write_chapters` writes
/// `chapter_source = 'filename'` and re-derives the same number onto the
/// target from the same filename; `derive::write_work` writes `label = key`.
/// Counting either would report a loss on every fold of every ordinary work.
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
/// three at once and `folding_a_work_that_carries_nothing_by_hand_discards_nothing`
/// sets none, so dropping either half of the `OR` chain — collapsing it to
/// one disjunct, or to two — leaves every test in this file green. Each case
/// below sets exactly one of the three and leaves the other two at their
/// default, so removing any one disjunct from the predicate fails exactly
/// the case that disjunct alone covers.
///
/// **Watched failing one disjunct at a time, restored.** Dropping
/// `label IS NOT key` failed only the `label` case; dropping
/// `language IS NOT NULL` failed only the `language` case; dropping
/// `kind IS NOT NULL` failed only the `kind` case. The other two cases stayed
/// green each time, which is what "individually load-bearing" means.
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
/// dead — the one shape that was invisible to every mover and to the count.
///
/// **`progress.edition`, `note.edition`, `truth_page.edition` and
/// `truth_member.edition` are plain `TEXT` with no foreign key to
/// `edition(key)`**, and `move_positions`, `move_notes` and `move_truth` are
/// all keyed on a `from_key` drawn from the `edition` *table*. A row naming a
/// key that table does not hold is therefore moved by nothing, absent from
/// [`super::DISCARDED`], and taken by the final `DELETE` — measured by
/// replaying [`absorb`]'s own statements against the migrated schema, which
/// reported `0` discarded and destroyed all four rows with nothing on screen.
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
/// **The only function in the project that deletes a `work` row, and it is
/// `pub`.** With `from == into` every edition maps to itself, every move is a
/// no-op, and the final `DELETE` takes the work with the reader's position,
/// their notes and the library's ground truth on it. Nothing in `derive` asks
/// for that — `superseded` skips a candidate its own `written` set holds —
/// but that is a guard in a different module, in a function `fold` never
/// calls, and a guard somewhere else is not a guard.
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
/// **Watched failing before the guard existed**, and the failure is the
/// point: `absorb` returned
/// `Err("moving notes off work 2: UNIQUE constraint failed: note.user_id,
/// note.work_id, note.id")`, which travels through `derive`'s `?` and kills
/// the whole derive — every later pair unfolded, `series_holding_no_work` and
/// `proposals::raise` never reached. Nothing is destroyed, the transaction
/// rolling back, so this is fold-defeating rather than data-losing; the
/// `expect` below is what turns that back into a failure if the guard goes.
///
/// **Refused rather than re-keyed.** The id is one the reader's own sidecar
/// names, so a silent rewrite would make their next import create a duplicate
/// note rather than update the one they edited. `seed_note` uses one fixed id
/// for user 1, which is why every other test here folds two works whose notes
/// happen to share it and none of them reached this.
///
/// **Renamed from `one_note_id_held_on_both_works_refuses_the_fold` when O78
/// closed**, because that stopped being true: two copies that agree in every
/// column fold, the source's copy dropped. These two differ in `slot` alone
/// — 33 against 11, on editions that map onto each other — which is the
/// anchor still differing once the fold has rewritten the edition key.
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
/// **Watched failing before the guard existed**: the fold succeeded with
/// `Moved { truth_pages: 1, .. }`, the source's page landing on `full-color`
/// under the target's `ccc` provenance while it had been drawn against `bbb`.
/// `truth_member`'s own schema comment calls that "the sharpest form of this
/// problem in the project" — a box on the wrong art on the *right* page — and
/// `score` would have accepted it as fresh.
///
/// The two digests differ deliberately. `truth_moves_with_its_boxes_and_its_digests`
/// and `a_truth_page_the_target_already_holds_stays_the_targets` both use
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
/// **The mirror of `where_both_works_hold_the_same_edition_the_newer_position_wins`.**
/// There the source's row is the newer and lands; here the target's is, so
/// the upsert's `WHERE excluded.updated > progress.updated` makes the
/// source's row a no-op. `execute` counts rows actually written, so before a
/// loss counter existed this fold reported `positions: 0` with a reading
/// position gone and nothing anywhere saying so.
///
/// "Newer wins" is still the decision — a slot number is not mergeable and
/// the later reading is the one somebody is in the middle of — and the
/// assertion that the target's slot stands is what says the counter did not
/// quietly become a refusal.
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
/// **Both orderings, because the arithmetic this replaced could only see
/// one.** `declined = offered - landed` was computed per edition off SQLite's
/// `changes`, so it could only see a loser that was *skipped*. With the older
/// reading on the later-sorting key — the only ordering this fixture used to
/// seed — the second `execute` is a no-op and `1-1` then `1-0` reports the
/// loss. With the **newer** reading on the later-sorting key the second
/// `execute` is an `UPDATE` that overwrites the first: `changes = 1` both
/// times, `1-1` twice, `moved_positions: 2` and `lost_positions: 0` with one
/// row surviving and one reading position destroyed. Watched failing on
/// 2026-08-27 before the whole-fold identity replaced the subtraction: the
/// `raw`-newer ordering passed and the `scan`-newer one read
/// `left: (2, 0), right: (2, 1)` — two writes, no loss reported, one row
/// left.
///
/// So the loss is asserted as the same number in both orderings, and neither
/// can pass while the other fails. `positions` is asserted per ordering
/// because it is a count of *writes* — see [`Moved::positions`] — and the
/// two orderings genuinely write a different number of times.
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
/// by definition, so the fold refuses — the safe direction, and untested
/// until now.
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
/// other side recorded — from the source side, which is the direction the
/// `offered.is_empty()` exemption used to wave straight through.
///
/// **The same defect the clash guard closes, reached from the other side.**
/// A source edition holding `truth_page`/`truth_box` rows and no
/// `truth_member` row passed the guard on the strength of "an empty list is
/// not a claim"; its boxes were then copied onto the target key, where the
/// target's own non-empty list stood over them through `move_truth`'s
/// `INSERT OR IGNORE`. `TruthEdition::is_stale_for` returns `true` when
/// `members` is empty and `digests` is not, so `score` correctly refuses that
/// edition as stale *before* the fold and reads the very same boxes as fresh
/// *after* it — "boxes drawn on superseded art recorded as fresh", which is
/// the sentence the guard exists to prevent.
///
/// **Watched failing before the exemption was narrowed**: the fold succeeded
/// with `Moved { truth_pages: 1, .. }`.
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
/// `DeriveReport::unfoldable` onto `derive_summary`'s single line, so the
/// unbounded join put hundreds of ULIDs where an operator reads a summary.
///
/// **The two copies of each id differ in `body`, and until O78 closed they did
/// not.** This fixture wrote every column the same on both works — one sidecar
/// imported against both, unedited — and asserted a refusal, which is exactly
/// the case O78 found losslessly resolvable: those copies now fold, the
/// source's dropped. So each work's copy now carries its own body, the shape
/// of that sidecar edited on one volume afterwards, and the count and the
/// "shows a few" message are still what is asserted.
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
/// **O78.** This shape used to refuse the whole fold, and it is the ordinary
/// one — the route migration 5's own rationale names, "importing one sidecar
/// against two works", writes exactly these rows — so a fold pair whose reader
/// had done that could never fold. Two rows equal in `body`, `slot`, region,
/// `color`, `created` and `updated`, on editions the fold maps onto each other,
/// are one fact written twice; dropping one loses nothing.
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
/// **There is no slot rewrite.** `edition_map` pairs editions whose members
/// carry the same digests in the same order, so slot 3 is slot 3 on both, and
/// the only part of a note's anchor a fold rewrites is its edition key. "The
/// same anchor" is therefore the target's key being exactly the one the
/// source's key maps to, with `slot` and all four region columns equal. A
/// comparison of the raw keys calls these two different and refuses.
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
///
/// **Watched with the drop moved above the refusal, restored.** The
/// transaction rolls that write back, so without
/// [`refused_leaving_every_note`]'s triggers this test passed against it.
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
