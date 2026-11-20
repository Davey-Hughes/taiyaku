use std::collections::{BTreeSet, HashSet};

use rusqlite::Connection;
use taiyaku_core::{Direction, PageRange};

use super::*;
use crate::test_support::plant_root;
use crate::{Library, libraries};

/// The fingerprint layout these tests plant and ask under.
const VERSION: u32 = 3;

fn range(start: u32, end: u32) -> PageRange {
    PageRange::new(start, end)
}

/// A pair at `v` whose two sides differ in every figure that has one, so a
/// figure left on the wrong side when the pair is turned round shows.
fn a_pair(sha_a: &str, sha_b: &str, v: f64) -> NewPair {
    NewPair {
        sha_a: sha_a.to_owned(),
        sha_b: sha_b.to_owned(),
        v,
        long_paired: 36,
        a_slots: 40,
        b_slots: 44,
        a_pages: 38,
        b_pages: 41,
        floor: Some(0.25),
        runs: vec![(range(0, 9), range(2, 11)), (range(12, 30), range(15, 33))],
    }
}

fn lookup_rows(conn: &Connection) -> Vec<(String, u32, u32)> {
    conn.prepare(
        "SELECT sha256, fingerprint_version, scorer_version FROM match_lookup ORDER BY sha256",
    )
    .and_then(|mut stmt| {
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect()
    })
    .expect("reads the lookup rows")
}

/// A pass may name either side first; the row puts the smaller digest first,
/// and every figure that belongs to a side goes with it.
#[test]
fn a_pair_is_stored_once_with_the_smaller_digest_first() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    store(&conn, &[a_pair("d-b", "d-a", 0.93)], "d-b", VERSION, 1).expect("stores");

    let stored = pairs(&conn).expect("reads the pairs");
    assert_eq!(stored.len(), 1, "one row: {stored:?}");
    let pair = &stored[0].new;
    assert_eq!(pair.sha_a, "d-a", "the smaller digest is a");
    assert_eq!(pair.sha_b, "d-b", "the larger digest is b");
    assert_eq!(
        (pair.a_slots, pair.b_slots),
        (44, 40),
        "the slot counts go with their digests"
    );
    assert_eq!(
        (pair.a_pages, pair.b_pages),
        (41, 38),
        "the page counts go with their digests"
    );
    assert_eq!(
        pair.runs[0],
        (range(2, 11), range(0, 9)),
        "the first run goes from d-a's pages to d-b's"
    );
    assert_eq!(
        pair.runs[1],
        (range(15, 33), range(12, 30)),
        "the second run goes from d-a's pages to d-b's"
    );
    assert_eq!(pair.runs.len(), 2, "no run is added or lost");
    assert_eq!(pair.v, 0.93, "V belongs to neither side");
    assert_eq!(pair.long_paired, 36, "nor do the long paired slots");
    assert_eq!(pair.floor, Some(0.25), "nor does the floor");
    assert_eq!(stored[0].state, State::Scored, "a stored pair is scored");
    assert_eq!(
        scored(&conn),
        Ok(HashSet::from([("d-a".to_owned(), "d-b".to_owned())])),
        "the pair reads as scored, smaller digest first"
    );

    store(&conn, &[a_pair("d-a", "d-b", 0.99)], "d-a", VERSION, 1).expect("stores again");

    assert_eq!(
        pairs(&conn).expect("reads the pairs"),
        stored,
        "the pair named the other way round adds no row and changes none"
    );
}

/// A pair under the floor is a caller's mistake, refused whole; a file whose
/// every pair fell under it is still marked looked up, so it is not looked up
/// again.
#[test]
fn store_refuses_a_pair_under_the_floor_and_writes_nothing() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let refused = store(&conn, &[a_pair("d-a", "d-b", 0.5999)], "d-a", 2, 5)
        .expect_err("a pair under the floor is refused");
    assert!(
        refused.contains("d-a") && refused.contains("d-b"),
        "the refusal names the pair: {refused}"
    );
    assert_eq!(pairs(&conn), Ok(Vec::new()), "no pair is stored");
    assert_eq!(lookup_rows(&conn), Vec::new(), "nor is the lookup mark");

    store(&conn, &[a_pair("d-a", "d-c", 0.6)], "d-a", 2, 5).expect("a pair at the floor");
    store(&conn, &[], "d-z", 2, 5).expect("a file whose every pair fell under the floor");

    let stored = pairs(&conn).expect("reads the pairs");
    assert_eq!(stored.len(), 1, "one pair: {stored:?}");
    assert_eq!(
        (
            stored[0].new.sha_a.as_str(),
            stored[0].new.sha_b.as_str(),
            stored[0].new.v
        ),
        ("d-a", "d-c", 0.6),
        "the pair at the floor is stored"
    );
    assert_eq!(
        lookup_rows(&conn),
        [("d-a".to_owned(), 2, 5), ("d-z".to_owned(), 2, 5)],
        "each file looked up is marked, with the fingerprint and scorer versions"
    );
    assert_eq!(
        looked_up(&conn),
        Ok(HashSet::from(["d-a".to_owned(), "d-z".to_owned()])),
        "and reads as looked up"
    );
}

/// A stored range that ends before it starts is refused when read, on either
/// side alone, so a check that let one good side vouch for the other fails
/// one of the two.
#[test]
fn a_run_backwards_on_either_side_is_refused_when_read() {
    for (runs, side) in [("[[9,0,2,11]]", "a"), ("[[0,9,11,2]]", "b")] {
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        store(&conn, &[a_pair("d-a", "d-b", 0.93)], "d-a", VERSION, 1).expect("stores");
        conn.execute("UPDATE content_match SET runs = ?1", [runs])
            .expect("the backwards run");

        let refused = pairs(&conn).expect_err("a backwards run is refused");

        assert!(refused.contains("runs backwards"), "{side} side: {refused}");
    }
}

/// `(state, split_by, split_at is set)` of each pair, by id.
fn marks(conn: &Connection) -> Vec<(i64, String, Option<i64>, bool)> {
    conn.prepare("SELECT id, state, split_by, split_at IS NOT NULL FROM content_match ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect()
        })
        .expect("reads the marks")
}

/// A Split says who kept the pair apart and when; a second Split of a pair
/// already split changes neither; Undo clears both.
#[test]
fn set_state_records_who_and_undo_clears_it() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO user (id, name, password_hash) VALUES (4, 'admin', 'x'), (5, 'other', 'x');",
    )
    .expect("two admins");
    store(
        &conn,
        &[a_pair("d-a", "d-b", 0.93), a_pair("d-a", "d-c", 0.91)],
        "d-a",
        VERSION,
        1,
    )
    .expect("stores two pairs");
    let ids: Vec<i64> = pairs(&conn)
        .expect("reads the pairs")
        .iter()
        .map(|pair| pair.id)
        .collect();
    let [split, kept] = ids[..] else {
        panic!("two pairs: {ids:?}");
    };

    assert_eq!(
        set_state(&conn, &[split], State::Split, Some(4)),
        Ok(1),
        "one pair split"
    );
    assert_eq!(
        marks(&conn),
        [
            (split, "split".to_owned(), Some(4), true),
            (kept, "scored".to_owned(), None, false)
        ],
        "the split pair names its admin and a time; the other is untouched"
    );
    assert_eq!(
        pairs(&conn).expect("reads the pairs")[0].state,
        State::Split,
        "and reads as split"
    );

    assert_eq!(
        set_state(&conn, &[split], State::Split, Some(5)),
        Ok(0),
        "a pair already split is not split again"
    );
    assert_eq!(
        marks(&conn)[0],
        (split, "split".to_owned(), Some(4), true),
        "it keeps who split it"
    );

    assert_eq!(
        set_state(&conn, &[split, 999], State::Scored, None),
        Ok(1),
        "one pair undone"
    );
    assert_eq!(
        marks(&conn),
        [
            (split, "scored".to_owned(), None, false),
            (kept, "scored".to_owned(), None, false)
        ],
        "undo clears who and when"
    );
}

/// A Split's pairs carry the pair it named, so Undo of any of them finds them
/// all; a second Split's pairs are found apart from the first's, and a pair
/// no Split set apart is found alone.
#[test]
fn a_splits_pairs_are_found_together_and_apart_from_another_splits() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    store(
        &conn,
        &[
            a_pair("d-a", "d-b", 0.93),
            a_pair("d-a", "d-c", 0.91),
            a_pair("d-a", "d-d", 0.89),
            a_pair("d-a", "d-e", 0.88),
        ],
        "d-a",
        VERSION,
        1,
    )
    .expect("stores four pairs");
    let ids: Vec<i64> = pairs(&conn)
        .expect("reads the pairs")
        .iter()
        .map(|pair| pair.id)
        .collect();
    let [ab, ac, ad, ae] = ids[..] else {
        panic!("four pairs: {ids:?}");
    };
    let groups = |conn: &Connection| -> Vec<Option<i64>> {
        conn.prepare("SELECT split_group FROM content_match ORDER BY id")
            .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
            .expect("reads the groups")
    };

    assert_eq!(
        (
            set_state(&conn, &[ac, ab], State::Split, None),
            set_state(&conn, &[ad], State::Split, None)
        ),
        (Ok(2), Ok(1)),
        "a Split naming (a, c) that sets (a, b) apart with it, and one of (a, d)"
    );
    assert_eq!(
        groups(&conn),
        [Some(ac), Some(ac), Some(ad), None],
        "each pair carries the pair its Split named"
    );
    assert_eq!(
        [ab, ac, ad, ae].map(|id| split_together(&conn, id).expect("reads")),
        [vec![ab, ac], vec![ab, ac], vec![ad], vec![ae]],
        "the first Split's pairs are found together, the second's alone, a pair not split alone"
    );

    let undone = split_together(&conn, ab).expect("reads");
    assert_eq!(set_state(&conn, &undone, State::Scored, None), Ok(2));
    assert_eq!(
        (marks(&conn), groups(&conn)),
        (
            vec![
                (ab, "scored".to_owned(), None, false),
                (ac, "scored".to_owned(), None, false),
                (ad, "split".to_owned(), None, true),
                (ae, "scored".to_owned(), None, false),
            ],
            vec![None, None, Some(ad), None]
        ),
        "undoing the first Split leaves the second's pair split"
    );
}

/// One work holds o, f and r, pairwise at the bar, (o, r) at exactly 0.87,
/// beside three more pairs of r's: one under the bar, one already split and
/// one with a file on another work. A Split of r on (f, r) sets apart (f, r)
/// and (o, r) alone, of f on it (f, r) and (f, o); a pair not naming the
/// file, or no pair, sets apart nothing.
#[test]
fn a_split_sets_apart_each_pair_at_the_bar_tying_the_file_to_its_work() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    plant_root(&conn, 1, "/srv/volumes");
    let digests = ["d-o", "d-f", "d-r", "d-u", "d-s"];
    plant_work(&conn, "triangle", Some(1.0), "rtl", None, 1, &digests);
    plant_work(&conn, "elsewhere", Some(1.0), "rtl", None, 1, &["d-x"]);
    let stored = |a: &str, b: &str, v: f64| {
        store(&conn, &[a_pair(a, b, v)], a, VERSION, 1).expect("stores");
        conn.query_row(
            "SELECT id FROM content_match WHERE sha_a = ?1 AND sha_b = ?2",
            [a, b],
            |r| r.get::<_, i64>(0),
        )
        .expect("the pair")
    };
    let f_r = stored("d-f", "d-r", 0.95);
    let o_r = stored("d-o", "d-r", 0.87);
    let f_o = stored("d-f", "d-o", 0.93);
    stored("d-r", "d-u", 0.86);
    let r_s = stored("d-r", "d-s", 0.95);
    stored("d-r", "d-x", 0.95);
    conn.execute(
        "UPDATE content_match SET state = 'split' WHERE id = ?1",
        [r_s],
    )
    .expect("splits (r, s)");

    assert_eq!(
        splits_for(&conn, f_r, "d-r"),
        Ok(Some(vec![f_r, o_r])),
        "r on (f, r)"
    );
    assert_eq!(
        splits_for(&conn, f_r, "d-f"),
        Ok(Some(vec![f_r, f_o])),
        "f on (f, r)"
    );
    assert_eq!(
        splits_for(&conn, f_o, "d-r"),
        Ok(None),
        "(f, o) does not name r"
    );
    assert_eq!(splits_for(&conn, 999, "d-r"), Ok(None), "no pair is 999");
}

/// A work at `volume` reading `reading` in `series`, with one edition whose
/// members hold `digests`, each under directory `root`. Answers the work's id.
fn plant_work(
    conn: &Connection,
    name: &str,
    volume: Option<f64>,
    reading: &str,
    series: Option<i64>,
    root: i64,
    digests: &[&str],
) -> i64 {
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub, volume, series_id)
         VALUES (?1, ?1, ?1, ?2, 'en', ?3, ?4)",
        rusqlite::params![name, reading, volume, series],
    )
    .unwrap_or_else(|e| panic!("plants work {name}: {e}"));
    let work = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO edition (work_id, key, label) VALUES (?1, 'en', 'en')",
        [work],
    )
    .unwrap_or_else(|e| panic!("plants {name}'s edition: {e}"));
    let edition = conn.last_insert_rowid();
    for (ordinal, digest) in (0_i64..).zip(digests) {
        conn.execute(
            "INSERT INTO member (edition_id, ordinal, path, sha256, pages, root_id)
             VALUES (?1, ?2, ?3, ?3, 3, ?4)",
            rusqlite::params![edition, ordinal, digest, root],
        )
        .unwrap_or_else(|e| panic!("plants {digest}: {e}"));
    }
    work
}

/// A fingerprint row for `digest`, its prints the digest's own bytes so each
/// file's are its own.
fn plant_prints(conn: &Connection, digest: &str, version: u32, outcome: &str) {
    conn.execute(
        "INSERT INTO fingerprint (sha256, version, outcome, pages, prints)
         VALUES (?1, ?2, ?3, 3, ?4)",
        rusqlite::params![digest, version, outcome, digest.as_bytes()],
    )
    .unwrap_or_else(|e| panic!("plants {digest}'s prints: {e}"));
}

/// Seven files, each kept out by one condition alone or let in. `/srv/raw` is
/// in a library switched off; `d-copy`'s primary sits there, and its copy in
/// `/srv/copies`, recorded only as a `member_root` row, is what reaches it.
#[test]
fn participants_are_sole_members_of_volumed_works_with_whole_prints_a_matching_library_reaches() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = plant_root(&conn, 1, "/srv/volumes");
    let raw = plant_root(&conn, 2, "/srv/raw");
    let copies = plant_root(&conn, 3, "/srv/copies");
    libraries::set_match_by_content(&conn, raw, false).expect("switches /srv/raw's library off");
    conn.execute(
        "INSERT INTO series (id, sort_title) VALUES (9, 'Alpha')",
        [],
    )
    .expect("a series");

    let alone = plant_work(&conn, "alone", Some(1.0), "rtl", Some(9), 1, &["d-alone"]);
    plant_work(
        &conn,
        "two",
        Some(2.0),
        "rtl",
        None,
        1,
        &["d-two-1", "d-two-2"],
    );
    plant_work(&conn, "unnumbered", None, "rtl", None, 1, &["d-unnumbered"]);
    plant_work(&conn, "partial", Some(4.0), "rtl", None, 1, &["d-partial"]);
    plant_work(&conn, "older", Some(5.0), "rtl", None, 1, &["d-older"]);
    plant_work(&conn, "off", Some(6.0), "rtl", None, 2, &["d-off"]);
    let with_copy = plant_work(&conn, "with_copy", Some(7.5), "ltr", None, 2, &["d-copy"]);
    conn.execute(
        "INSERT INTO member_root (member_id, root_id)
         SELECT id, 3 FROM member WHERE sha256 = 'd-copy'",
        [],
    )
    .expect("records d-copy's copy under /srv/copies");
    for digest in [
        "d-alone",
        "d-two-1",
        "d-two-2",
        "d-unnumbered",
        "d-off",
        "d-copy",
    ] {
        plant_prints(&conn, digest, VERSION, "whole");
    }
    plant_prints(&conn, "d-partial", VERSION, "partial");
    plant_prints(&conn, "d-older", VERSION - 1, "whole");

    let taking_part = participants(&conn, VERSION).expect("lists the participants");

    let named = |digest: &str| taking_part.iter().find(|p| p.sha256 == digest);
    for (digest, why) in [
        ("d-two-1", "a member of an edition of two"),
        ("d-two-2", "the other member of that edition"),
        ("d-unnumbered", "a work with no volume"),
        ("d-partial", "a file read in part"),
        ("d-older", "prints of another layout"),
        ("d-off", "a directory only a switched-off library reaches"),
    ] {
        assert_eq!(named(digest), None, "{digest} takes no part: {why}");
    }
    assert_eq!(
        named("d-alone"),
        Some(&Participant {
            sha256: "d-alone".to_owned(),
            prints: b"d-alone".to_vec(),
            work_id: alone,
            series_id: Some(9),
            volume: 1.0,
            reading: Direction::RightToLeft,
            libraries: BTreeSet::from([volumes]),
        }),
        "d-alone takes part: the only member, numbered, read whole, reached"
    );
    assert_eq!(
        named("d-copy"),
        Some(&Participant {
            sha256: "d-copy".to_owned(),
            prints: b"d-copy".to_vec(),
            work_id: with_copy,
            series_id: None,
            volume: 7.5,
            reading: Direction::LeftToRight,
            libraries: BTreeSet::from([copies]),
        }),
        "d-copy takes part through its copy's directory, and only its library"
    );
    let order: Vec<&str> = taking_part.iter().map(|p| p.sha256.as_str()).collect();
    assert_eq!(order, ["d-alone", "d-copy"], "by digest");
}

/// The lookup and the pair store know a file by its digest, so bytes two
/// members hold take part once, as the first member's.
#[test]
fn a_digest_two_members_hold_takes_part_once() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    plant_root(&conn, 1, "/srv/volumes");
    let first = plant_work(&conn, "first", Some(1.0), "rtl", None, 1, &["d-twice"]);
    plant_work(&conn, "second", Some(2.0), "rtl", None, 1, &["d-twice"]);
    plant_prints(&conn, "d-twice", VERSION, "whole");

    let taking_part = participants(&conn, VERSION).expect("lists the participants");

    let found: Vec<(&str, i64)> = taking_part
        .iter()
        .map(|p| (p.sha256.as_str(), p.work_id))
        .collect();
    assert_eq!(
        found,
        [("d-twice", first)],
        "once, with the first member's work"
    );
}

/// Each directory a library with matching on reaches, with every such
/// library: a nested directory the outer library's walk reaches is the outer
/// library's too, and a library switched off reaches nothing.
#[test]
fn reach_names_each_directory_with_the_matching_libraries_that_reach_it() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let outer = plant_root(&conn, 1, "/srv/manga");
    let inner = plant_root(&conn, 2, "/srv/manga/raw");
    let off = plant_root(&conn, 3, "/srv/off");
    libraries::set_match_by_content(&conn, off, false).expect("switches /srv/off's library off");

    let reached = reach(&conn).expect("reads the reach");

    assert_eq!(
        reached.get(&1),
        Some(&BTreeSet::from([outer])),
        "/srv/manga"
    );
    assert_eq!(
        reached.get(&2),
        Some(&BTreeSet::from([outer, inner])),
        "/srv/manga/raw, by its own library and the outer one's walk"
    );
    assert_eq!(reached.get(&3), None, "/srv/off, whose only library is off");
}
