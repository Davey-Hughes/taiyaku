use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rusqlite::Connection;
use taiyaku_core::{
    Alignment, PageRange, Quality, RunEntry, Slot, SlotSpan, SolverInfo, Source, Verdict,
};

use super::{Applicable, Edge, Joins, applicable, apply};
use crate::account::Access;
use crate::derive::tests::commits_during;
use crate::derive::{self, DeriveReport, Group, GroupKey};
use crate::matches::{self, NewPair, State};
use crate::progress::{Outcome, Step, Unwatched};
use crate::rules::Engine;
use crate::scan::tests::{write_archive, write_archive_labelled};
use crate::scan::{self, InLibrary, Root};
use crate::test_support::{Recording, library_of_root};
use crate::{Library, browse, libraries, preview, project, rules, state};

/// The official English volumes: key, edition kind and language.
const VOLUMES: (&str, &str, &str) = ("en", "official", "en");
/// The Japanese raws.
const RAW: (&str, &str, &str) = ("ja", "raw", "ja");
/// An English fan translation.
const FAN: (&str, &str, &str) = ("fan", "fan", "en");

/// A tree of directories laid out as `derive/tests.rs`'s `archived_volumes` lays
/// one out: volume archives at `Series/v01/Series v01.cbz`, three pages each,
/// every page naming its file's path so no two files share bytes.
struct Shelf {
    tree: crate::tests::TempDir,
}

impl Shelf {
    fn new(label: &str) -> Self {
        Self {
            tree: crate::tests::tempdir(label),
        }
    }

    /// Where `file` sits under directory `name`.
    fn path(&self, name: &str, file: &str) -> PathBuf {
        self.tree.path().join(name).join(file)
    }

    /// Directory `name` of `library`, with the edition key, kind and
    /// language its files fall back to, holding `files`, added and scanned.
    fn directory(
        &self,
        conn: &Connection,
        name: &str,
        library: &str,
        (key, kind, language): (&str, &str, &str),
        files: &[&str],
    ) -> Root {
        for file in files {
            write_archive(&self.path(name, file), 3);
        }
        let at = self.tree.path().join(name);
        std::fs::create_dir_all(&at).expect("makes the directory");
        let root = scan::add_root_in(conn, &at, InLibrary::Named(library)).expect("adds it");
        conn.execute(
            "UPDATE root SET edition_key = ?2, default_kind = ?3, default_language = ?4
              WHERE id = ?1",
            rusqlite::params![root.id, key, kind, language],
        )
        .expect("keys it");
        scan::scan_root(conn, &root).expect("scans it");
        root
    }
}

/// Each group this derive's files and rules make, as derive makes them.
fn groups(conn: &Connection) -> BTreeMap<GroupKey, Group> {
    let (engine, _complaints) = Engine::load(conn).expect("loads the rules");
    let roots = scan::roots(conn).expect("lists the directories");
    let frozen = scan::paused(conn).expect("reads the pauses");
    let Ok(Outcome::Done(groups)) = derive::grouped(
        conn,
        &engine,
        &roots,
        &frozen,
        &Unwatched,
        &mut BTreeSet::new(),
        &mut DeriveReport::default(),
    ) else {
        panic!("groups the files");
    };
    groups
}

fn applying(conn: &Connection, groups: &BTreeMap<GroupKey, Group>) -> Applicable {
    applicable(conn, groups).expect("reads which pairs apply")
}

/// The digest of `file` under `root`, which the first derive wrote back.
fn digest(conn: &Connection, root: &Root, file: &str) -> String {
    conn.query_row(
        "SELECT sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root.id, file],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("{file}'s digest: {e}"))
}

/// A fingerprint row for `digest`, read with `outcome`.
fn read(conn: &Connection, digest: &str, outcome: &str) {
    conn.execute(
        "INSERT INTO fingerprint (sha256, version, outcome, pages, prints)
         VALUES (?1, 1, ?2, 3, x'00')",
        [digest, outcome],
    )
    .unwrap_or_else(|e| panic!("reads {digest}: {e}"));
}

/// `one` and `two` stored as a pair at `v`, as the match pass stores one.
/// Answers the pair's id.
fn pair(conn: &Connection, one: &str, two: &str, v: f64) -> i64 {
    let scored = NewPair {
        sha_a: one.to_owned(),
        sha_b: two.to_owned(),
        v,
        long_paired: 3,
        a_slots: 3,
        b_slots: 3,
        a_pages: 3,
        b_pages: 3,
        floor: None,
        runs: Vec::new(),
    };
    matches::store(conn, &[scored], one, 1, 1).expect("stores the pair");
    let (a, b) = if one < two { (one, two) } else { (two, one) };
    conn.query_row(
        "SELECT id FROM content_match WHERE sha_a = ?1 AND sha_b = ?2",
        [a, b],
        |r| r.get(0),
    )
    .expect("the pair's id")
}

/// The group of volume `volume` of `series`.
fn volume(series: &str, volume: f64) -> GroupKey {
    (series.to_owned(), Some(volume.to_bits()), None)
}

/// An edge as [`applicable`] answers one: each side's `(group, key)`, the
/// smaller digest's first. A side is `(digest, group, key)`.
fn edge(pair: i64, v: f64, one: (&str, GroupKey, &str), two: (&str, GroupKey, &str)) -> Edge {
    let [a, b] = if one.0 < two.0 {
        [one, two]
    } else {
        [two, one]
    };
    Edge {
        pair,
        v,
        at: [(a.1, a.2.to_owned()), (b.1, b.2.to_owned())],
    }
}

/// How many files edition `key` of `group` holds, none where there is no
/// such group or edition: a fixture shows with it that the group it means is
/// there, holding the files it means.
fn members(groups: &BTreeMap<GroupKey, Group>, group: &GroupKey, key: &str) -> usize {
    groups
        .get(group)
        .and_then(|group| group.editions.get(key))
        .map_or(0, Vec::len)
}

/// The edge at the bar against one a hair under it, so a `>` where `>=` belongs
/// shows.
#[test]
fn a_pair_at_the_bar_applies_and_one_just_under_does_not() {
    let shelf = Shelf::new("joins-bar");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz", "Oshi/v02/Oshi v02.cbz"],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &[
            "Oshinoko/v01/Oshinoko v01.cbz",
            "Oshinoko/v02/Oshinoko v02.cbz",
        ],
    );
    derive::derive(&conn).expect("derives");
    let en1 = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let en2 = digest(&conn, &volumes, "Oshi/v02/Oshi v02.cbz");
    let ja1 = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    let ja2 = digest(&conn, &raw, "Oshinoko/v02/Oshinoko v02.cbz");
    for file in [&en1, &en2, &ja1, &ja2] {
        read(&conn, file, "whole");
    }
    let at_the_bar = pair(&conn, &en1, &ja1, 0.87);
    pair(&conn, &en2, &ja2, 0.8699);

    let found = applying(&conn, &groups(&conn));

    assert_eq!(
        found.edges,
        [edge(
            at_the_bar,
            0.87,
            (&en1, volume("Oshi", 1.0), "en"),
            (&ja1, volume("Oshinoko", 1.0), "ja"),
        )],
        "the pair at 0.87 applies, between the two volume 1 groups; the one at 0.8699 does not"
    );
    assert!(
        found.together.is_empty(),
        "no pair is inside a group: {found:?}"
    );
    assert!(
        found.held.is_empty(),
        "the seeds and no hand value hold nothing: {found:?}"
    );
}

/// A split pair beside a scored one, so a test that applies nothing at all
/// fails.
#[test]
fn a_split_pair_applies_nothing() {
    let shelf = Shelf::new("joins-split");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz", "Oshi/v02/Oshi v02.cbz"],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &[
            "Oshinoko/v01/Oshinoko v01.cbz",
            "Oshinoko/v02/Oshinoko v02.cbz",
        ],
    );
    derive::derive(&conn).expect("derives");
    let en1 = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let en2 = digest(&conn, &volumes, "Oshi/v02/Oshi v02.cbz");
    let ja1 = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    let ja2 = digest(&conn, &raw, "Oshinoko/v02/Oshinoko v02.cbz");
    for file in [&en1, &en2, &ja1, &ja2] {
        read(&conn, file, "whole");
    }
    let split = pair(&conn, &en1, &ja1, 0.95);
    let scored = pair(&conn, &en2, &ja2, 0.95);
    assert_eq!(
        matches::set_state(&conn, &[split], State::Split, None),
        Ok(1),
        "an admin splits volume 1's pair"
    );

    let found = applying(&conn, &groups(&conn));

    assert_eq!(
        found.edges,
        [edge(
            scored,
            0.95,
            (&en2, volume("Oshi", 2.0), "en"),
            (&ja2, volume("Oshinoko", 2.0), "ja"),
        )],
        "only volume 2's pair applies; the split one does not"
    );
    assert!(found.together.is_empty(), "nor is it together: {found:?}");
}

/// A colour edition against the black-and-white one, both `en` from
/// `Volumes`, beside the raw's pair, which applies.
#[test]
fn same_key_files_link_nothing() {
    let shelf = Shelf::new("joins-same-key");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &[
            "Oshi/v01/Oshi v01.cbz",
            "Oshi Colour/v01/Oshi Colour v01.cbz",
        ],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    derive::derive(&conn).expect("derives");
    let black = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let colour = digest(&conn, &volumes, "Oshi Colour/v01/Oshi Colour v01.cbz");
    let ja = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    for file in [&black, &colour, &ja] {
        read(&conn, file, "whole");
    }
    pair(&conn, &black, &colour, 0.95);
    let raw_pair = pair(&conn, &black, &ja, 0.95);
    let made = groups(&conn);
    assert_eq!(
        members(&made, &volume("Oshi Colour", 1.0), "en"),
        1,
        "the colour volume is a group of its own, keyed en"
    );

    let found = applying(&conn, &made);

    assert_eq!(
        found.edges,
        [edge(
            raw_pair,
            0.95,
            (&black, volume("Oshi", 1.0), "en"),
            (&ja, volume("Oshinoko", 1.0), "ja"),
        )],
        "the raw's pair applies; the colour and black-and-white pair, both en, does not"
    );
    assert!(found.together.is_empty(), "nor is it together: {found:?}");
}

/// Names already put both files in one group, so the pair joins nothing.
#[test]
fn a_pair_names_already_put_in_one_group_is_together() {
    let shelf = Shelf::new("joins-together");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let raw = shelf.directory(&conn, "Raw", "Manga", RAW, &["Oshi/v01/Oshi v01.cbz"]);
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(&conn, &raw, "Oshi/v01/Oshi v01.cbz");
    read(&conn, &en, "whole");
    read(&conn, &ja, "whole");
    let id = pair(&conn, &en, &ja, 0.95);

    let found = applying(&conn, &groups(&conn));

    assert_eq!(
        found.together,
        [edge(
            id,
            0.95,
            (&en, volume("Oshi", 1.0), "en"),
            (&ja, volume("Oshi", 1.0), "ja"),
        )],
        "the pair is together, both sides in volume 1's one group"
    );
    assert!(found.edges.is_empty(), "and links no two groups: {found:?}");
}

/// One file per way a series can be decided, each its own group. The seeds
/// decide every other file's; seed 4 is then edited, and its capture still
/// holds nothing.
#[test]
fn only_a_literal_series_holds_a_file() {
    let shelf = Shelf::new("joins-literal");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &[
            "Literal/v01/Literal v01.cbz",
            "Keyed/v01/Keyed v01.cbz",
            "Seeded/v01/Seeded v01.cbz",
            "Captured/v01/Captured v01.cbz",
            "Both/v01/Both v01.cbz",
            "Restored/v01/Restored v01.cbz",
            "Qualified (Raw)/v01/Qualified v01.cbz",
        ],
    );
    conn.execute_batch(
        r#"
        INSERT INTO rule (ordinal, kind, pattern, match_on, set_series)
             VALUES (10, 'prefix', 'Literal/', 'path', 'Literal');
        INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key)
             VALUES (10, 'prefix', 'Keyed/', 'path', 'colour');
        INSERT INTO rule (ordinal, kind, pattern, match_on)
             VALUES (10, 'regex', '^(?<series>Captured) v(?<volume>\d+)', 'name');
        INSERT INTO rule (ordinal, kind, pattern, match_on, set_series)
             VALUES (10, 'regex', '^(?<series>Both) v', 'name', 'Both Literal');
        INSERT INTO rule (ordinal, kind, pattern, match_on, set_series)
             VALUES (3, 'prefix', 'Restored/', 'path', 'Restored (Volumes)');
        "#,
    )
    .expect("one rule per way a series is decided");

    let made = groups(&conn);
    let held = applying(&conn, &made).held;
    let mut wrong: Vec<String> = Vec::new();
    for (series, key, holds, why) in [
        ("Literal", "en", true, "a prefix rule's literal set_series"),
        (
            "Keyed",
            "colour",
            false,
            "a prefix rule setting only the key, over a seeded capture",
        ),
        ("Seeded", "en", false, "the seeds' captures"),
        (
            "Captured",
            "en",
            false,
            "a hand-written regex capturing series and volume",
        ),
        (
            "Both",
            "en",
            false,
            "a regex whose capture beats its own literal set_series",
        ),
        (
            "Restored (Volumes)",
            "en",
            true,
            "a literal set_series O89 restores over seed 4's capture",
        ),
        (
            "Qualified (Raw)",
            "en",
            false,
            "seed 1's capture of the directory, which O89 restores over seed 4's",
        ),
    ] {
        let group = volume(series, 1.0);
        assert_eq!(
            members(&made, &group, key),
            1,
            "{series} is a group of one file under {key}"
        );
        if held.contains(&group) != holds {
            wrong.push(format!("{series}, {why}: held is {}", !holds));
        }
    }

    let edited = conn
        .execute(
            r"UPDATE rule SET pattern = '^(?<series>.+?)[ _-]+v(?<volume>\d+)' WHERE id = 4",
            [],
        )
        .expect("edits seed 4");
    assert_eq!(edited, 1, "an admin edits seed 4 to capture the volume too");
    let made = groups(&conn);
    let held = applying(&conn, &made).held;
    for (series, holds, why) in [
        ("Seeded", false, "seed 4 edited, still a capture"),
        ("Literal", true, "the literal, still after seed 4"),
    ] {
        let group = volume(series, 1.0);
        assert_eq!(
            members(&made, &group, "en"),
            1,
            "{series} is still a group of one file"
        );
        if held.contains(&group) != holds {
            wrong.push(format!("{series}, {why}: held is {}", !holds));
        }
    }
    assert!(
        wrong.is_empty(),
        "each file is held as its series was decided: {wrong:#?}"
    );
}

/// A group of two files, the raw a rule's literal series holds and a
/// Korean scanlation read by capture: one held file holds the whole group,
/// which then never moves to join the English volume.
#[test]
fn one_held_file_holds_its_whole_group() {
    let shelf = Shelf::new("joins-held-one");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &["Oshinoko (Raw)/v01/Oshinoko v01.cbz"],
    );
    let scans = shelf.directory(
        &conn,
        "Scans",
        "Manga",
        KOREAN_SCANS,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_series)
              VALUES (10, 'prefix', 'Oshinoko (Raw)/', 'path', 'Oshinoko')",
        [],
    )
    .expect("a rule's literal series for the raw");
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(&conn, &raw, "Oshinoko (Raw)/v01/Oshinoko v01.cbz");
    let ko = digest(&conn, &scans, "Oshinoko/v01/Oshinoko v01.cbz");
    for file in [&en, &ja, &ko] {
        read(&conn, file, "whole");
    }
    pair(&conn, &en, &ja, 0.95);
    let made = groups(&conn);
    let group = volume("Oshinoko", 1.0);
    assert_eq!(
        (members(&made, &group, "ja"), members(&made, &group, "ko")),
        (1, 1),
        "the fixture: the raw and the scanlation are one group"
    );

    assert_eq!(
        applying(&conn, &made).held,
        BTreeSet::from([group]),
        "the raw's literal series holds the group"
    );
    derive::derive(&conn).expect("derives with the pair");
    let raws = holder(&conn, &ja);
    assert_eq!(
        (raws == holder(&conn, &en), holder(&conn, &ko)),
        (false, raws),
        "the raw stays with the scanlation, and neither joins the English volume"
    );
}

/// The raw's work given a hand value holds the raw; the same value on the
/// host, after a join brought the raw's file into the host's work, holds only
/// the host's own group, which never moves, so the raw still joins.
#[test]
fn an_admins_hand_value_holds_the_work_that_would_move() {
    for (column, value) in [
        ("title_hand", "'Oshi no Ko'"),
        ("volume_hand", "1.0"),
        ("series_id_hand", "series_id"),
    ] {
        let shelf = Shelf::new("joins-hand");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let volumes = shelf.directory(
            &conn,
            "Volumes",
            "Manga",
            VOLUMES,
            &["Oshi/v01/Oshi v01.cbz"],
        );
        let raw = shelf.directory(
            &conn,
            "Raw",
            "Manga",
            RAW,
            &["Oshinoko/v01/Oshinoko v01.cbz"],
        );
        derive::derive(&conn).expect("derives");
        let en = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
        let ja = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
        read(&conn, &en, "whole");
        read(&conn, &ja, "whole");
        let id = pair(&conn, &en, &ja, 0.95);
        let applies = [edge(
            id,
            0.95,
            (&en, volume("Oshi", 1.0), "en"),
            (&ja, volume("Oshinoko", 1.0), "ja"),
        )];
        let work_of = |digest: &str| -> i64 {
            conn.query_row(
                "SELECT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
                  WHERE m.sha256 = ?1",
                [digest],
                |r| r.get(0),
            )
            .expect("the work holding the file")
        };
        let (host, guest) = (work_of(&en), work_of(&ja));

        conn.execute(
            &format!("UPDATE work SET {column} = {value} WHERE id = ?1"),
            [guest],
        )
        .expect("an admin sets a hand value on the raw's work");
        let found = applying(&conn, &groups(&conn));

        assert_eq!(
            found.held,
            BTreeSet::from([volume("Oshinoko", 1.0)]),
            "{column} on the raw's work holds the raw's group"
        );
        assert_eq!(found.edges, applies, "{column}: the pair still applies");

        conn.execute(&format!("UPDATE work SET {column} = NULL"), [])
            .expect("the admin clears it");
        conn.execute(
            "INSERT INTO edition (work_id, key, label, joined_by) VALUES (?1, 'ja', 'ja', ?2)",
            [host, id],
        )
        .expect("a join brings the raw's edition into the host");
        conn.execute(
            "UPDATE member SET edition_id = ?2 WHERE sha256 = ?1",
            rusqlite::params![ja, conn.last_insert_rowid()],
        )
        .expect("with its file");
        conn.execute(
            &format!("UPDATE work SET {column} = {value} WHERE id = ?1"),
            [host],
        )
        .expect("an admin sets the hand value on the host");
        let found = applying(&conn, &groups(&conn));

        assert_eq!(
            found.held,
            BTreeSet::from([volume("Oshi", 1.0)]),
            "{column} on the host holds the host's own group, not the raw it took in"
        );
        assert_eq!(found.edges, applies, "{column}: the pair still applies");
    }
}

/// Review Focus 5: Manga reaches the English file and Comics the raw, so no
/// library reaches both; a byte-identical copy of the raw under a directory
/// of Manga's makes one that does, through the raw's copies; Manga switched
/// off reaches neither.
#[test]
fn a_pair_no_library_reaches_on_both_sides_applies_nothing() {
    let shelf = Shelf::new("joins-libraries");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Comics",
        RAW,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    read(&conn, &en, "whole");
    read(&conn, &ja, "whole");
    let id = pair(&conn, &en, &ja, 0.95);

    let found = applying(&conn, &groups(&conn));
    assert!(
        found.edges.is_empty(),
        "no library reaches both files: {found:?}"
    );
    assert!(
        found.together.is_empty(),
        "nor are they together: {found:?}"
    );

    let raw_file = shelf.path("Raw", "Oshinoko/v01/Oshinoko v01.cbz");
    write_archive_labelled(
        &shelf.path("Copies", "Oshinoko/v01/Oshinoko v01.cbz"),
        3,
        "jpg",
        &raw_file.to_string_lossy(),
    );
    let copies = shelf.directory(&conn, "Copies", "Manga", RAW, &[]);
    let made = groups(&conn);
    let raw_volume = made
        .get(&volume("Oshinoko", 1.0))
        .and_then(|group| group.editions.get("ja"))
        .expect("the raw's group");
    let copied: Vec<(i64, Vec<i64>)> = raw_volume
        .iter()
        .map(|c| (c.root_id, c.copies.iter().map(|k| k.root_id).collect()))
        .collect();
    assert_eq!(
        copied,
        [(raw.id, vec![copies.id])],
        "the raw's one file sits under Raw, with its copy under Copies"
    );

    let found = applying(&conn, &made);
    assert_eq!(
        found.edges,
        [edge(
            id,
            0.95,
            (&en, volume("Oshi", 1.0), "en"),
            (&ja, volume("Oshinoko", 1.0), "ja"),
        )],
        "Manga reaches the raw through its copy's directory"
    );

    libraries::set_match_by_content(&conn, library_of_root(&conn, volumes.id), false)
        .expect("switches Manga off");
    let found = applying(&conn, &groups(&conn));
    assert!(
        found.edges.is_empty(),
        "with Manga off, no library reaches both: {found:?}"
    );
}

/// Volume 1's raw shares its edition with another file and volume 2's was
/// read in part; volume 3's pair applies.
#[test]
fn a_file_sharing_its_edition_or_read_partially_takes_no_part() {
    let shelf = Shelf::new("joins-no-part");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &[
            "Oshi/v01/Oshi v01.cbz",
            "Oshi/v02/Oshi v02.cbz",
            "Oshi/v03/Oshi v03.cbz",
        ],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &[
            "Oshinoko/v01/Oshinoko v01 a.cbz",
            "Oshinoko/v01/Oshinoko v01 b.cbz",
            "Oshinoko/v02/Oshinoko v02.cbz",
            "Oshinoko/v03/Oshinoko v03.cbz",
        ],
    );
    derive::derive(&conn).expect("derives");
    let en: Vec<String> = [
        "Oshi/v01/Oshi v01.cbz",
        "Oshi/v02/Oshi v02.cbz",
        "Oshi/v03/Oshi v03.cbz",
    ]
    .iter()
    .map(|file| digest(&conn, &volumes, file))
    .collect();
    let shared = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01 a.cbz");
    let partial = digest(&conn, &raw, "Oshinoko/v02/Oshinoko v02.cbz");
    let whole = digest(&conn, &raw, "Oshinoko/v03/Oshinoko v03.cbz");
    for file in en.iter().chain([&shared, &whole]) {
        read(&conn, file, "whole");
    }
    read(
        &conn,
        &digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01 b.cbz"),
        "whole",
    );
    read(&conn, &partial, "partial");
    pair(&conn, &en[0], &shared, 0.95);
    pair(&conn, &en[1], &partial, 0.95);
    let applies = pair(&conn, &en[2], &whole, 0.95);
    let made = groups(&conn);
    assert_eq!(
        members(&made, &volume("Oshinoko", 1.0), "ja"),
        2,
        "volume 1's raw edition holds two files"
    );

    let found = applying(&conn, &made);

    assert_eq!(
        found.edges,
        [edge(
            applies,
            0.95,
            (&en[2], volume("Oshi", 3.0), "en"),
            (&whole, volume("Oshinoko", 3.0), "ja"),
        )],
        "only volume 3's pair applies"
    );
}

/// A one-shot with no volume, and bytes a directory holds twice under two
/// names, take no part; volume 2's pair applies.
#[test]
fn a_volume_less_file_or_bytes_held_twice_take_no_part() {
    let shelf = Shelf::new("joins-alone");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &[
            "Oneshot/Oneshot.cbz",
            "Oshi/v01/Oshi v01.cbz",
            "Oshi/v02/Oshi v02.cbz",
        ],
    );
    let twin = shelf.path("Raw", "Oshinoko/v01/Oshinoko v01.cbz");
    write_archive_labelled(
        &shelf.path("Raw", "Twice/v05/Twice v05.cbz"),
        3,
        "jpg",
        &twin.to_string_lossy(),
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &[
            "Oneshot JP/Oneshot JP.cbz",
            "Oshinoko/v01/Oshinoko v01.cbz",
            "Oshinoko/v02/Oshinoko v02.cbz",
        ],
    );
    derive::derive(&conn).expect("derives");
    let oneshot = digest(&conn, &volumes, "Oneshot/Oneshot.cbz");
    let en1 = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let en2 = digest(&conn, &volumes, "Oshi/v02/Oshi v02.cbz");
    let oneshot_jp = digest(&conn, &raw, "Oneshot JP/Oneshot JP.cbz");
    let twice = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    let ja2 = digest(&conn, &raw, "Oshinoko/v02/Oshinoko v02.cbz");
    assert_eq!(
        digest(&conn, &raw, "Twice/v05/Twice v05.cbz"),
        twice,
        "Raw holds volume 1's bytes under a second name"
    );
    for file in [&oneshot, &en1, &en2, &oneshot_jp, &twice, &ja2] {
        read(&conn, file, "whole");
    }
    pair(&conn, &oneshot, &oneshot_jp, 0.95);
    pair(&conn, &en1, &twice, 0.95);
    let applies = pair(&conn, &en2, &ja2, 0.95);
    let made = groups(&conn);
    for (group, why) in [
        (volume("Oshinoko", 1.0), "volume 1's raw"),
        (volume("Twice", 5.0), "its twin"),
    ] {
        assert_eq!(
            members(&made, &group, "ja"),
            1,
            "{why} is a group's only file"
        );
    }

    let found = applying(&conn, &made);

    assert_eq!(
        found.edges,
        [edge(
            applies,
            0.95,
            (&en2, volume("Oshi", 2.0), "en"),
            (&ja2, volume("Oshinoko", 2.0), "ja"),
        )],
        "only volume 2's pair applies"
    );
    assert!(found.together.is_empty(), "nothing is together: {found:?}");
}

/// What [`apply`] makes of this derive's groups and the stored pairs.
fn joined(conn: &Connection) -> (BTreeMap<GroupKey, Group>, Joins) {
    apply(conn, groups(conn), &mut DeriveReport::default()).expect("applies the pairs")
}

/// Each work holding `digest`, by id.
fn holders(conn: &Connection, digest: &str) -> Vec<i64> {
    conn.prepare(
        "SELECT DISTINCT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
          WHERE m.sha256 = ?1 ORDER BY e.work_id",
    )
    .and_then(|mut stmt| stmt.query_map([digest], |r| r.get(0))?.collect())
    .unwrap_or_else(|e| panic!("the works holding {digest}: {e}"))
}

/// The one work holding `digest`.
fn holder(conn: &Connection, digest: &str) -> i64 {
    let works = holders(conn, digest);
    assert_eq!(works.len(), 1, "one work holds {digest}: {works:?}");
    works[0]
}

/// A work's series, volume and hub.
fn work(conn: &Connection, id: i64) -> (String, Option<f64>, String) {
    conn.query_row(
        "SELECT s.sort_title, w.volume, w.hub FROM work w JOIN series s ON s.id = w.series_id
          WHERE w.id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap_or_else(|e| panic!("work {id}: {e}"))
}

/// An edition as [`editions`] reads one: key, kind, language and the pair
/// that joined it.
type EditionRow = (String, Option<String>, Option<String>, Option<i64>);

/// A work's editions, by key.
fn editions(conn: &Connection, id: i64) -> Vec<EditionRow> {
    conn.prepare(
        "SELECT key, kind, language, joined_by FROM edition WHERE work_id = ?1 ORDER BY key",
    )
    .and_then(|mut stmt| {
        stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect()
    })
    .unwrap_or_else(|e| panic!("the editions of work {id}: {e}"))
}

/// The edition a directory labelled `(key, kind, language)` makes, joined by
/// `joined_by`.
fn labelled((key, kind, language): (&str, &str, &str), joined_by: Option<i64>) -> EditionRow {
    (
        key.to_owned(),
        Some(kind.to_owned()),
        Some(language.to_owned()),
        joined_by,
    )
}

/// The two digests pair `id` names.
fn named(conn: &Connection, id: i64) -> [String; 2] {
    conn.query_row(
        "SELECT sha_a, sha_b FROM content_match WHERE id = ?1",
        [id],
        |r| Ok([r.get(0)?, r.get(1)?]),
    )
    .unwrap_or_else(|e| panic!("pair {id}: {e}"))
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect(sql)
}

/// Each series no work sits in, by title.
fn workless(conn: &Connection) -> Vec<String> {
    conn.prepare(
        "SELECT sort_title FROM series s
          WHERE NOT EXISTS (SELECT 1 FROM work w WHERE w.series_id = s.id)
          ORDER BY sort_title",
    )
    .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
    .expect("the series holding no work")
}

/// Every edition in the store, named by what two stores of the same files
/// share: its work's series and volume, its key, its files' digests, and the
/// digests of the pair that joined it.
type Laid = (String, Option<f64>, String, String, Option<[String; 2]>);

fn layout(conn: &Connection) -> Vec<Laid> {
    conn.prepare(
        "SELECT s.sort_title, w.volume, e.key,
                (SELECT group_concat(m.sha256, ',' ORDER BY m.ordinal) FROM member m
                  WHERE m.edition_id = e.id),
                cm.sha_a, cm.sha_b
           FROM edition e JOIN work w ON w.id = e.work_id JOIN series s ON s.id = w.series_id
           LEFT JOIN content_match cm ON cm.id = e.joined_by
          ORDER BY s.sort_title, w.volume, e.key",
    )
    .and_then(|mut stmt| {
        stmt.query_map([], |r| {
            let pair: (Option<String>, Option<String>) = (r.get(4)?, r.get(5)?);
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                match pair {
                    (Some(a), Some(b)) => Some([a, b]),
                    _ => None,
                },
            ))
        })?
        .collect()
    })
    .expect("the store's editions")
}

/// The English volume and its raw, derived, read whole and paired at 0.95:
/// the two digests and the pair.
fn english_and_raw(shelf: &Shelf, conn: &Connection) -> (String, String, i64) {
    let volumes = shelf.directory(
        conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let raw = shelf.directory(
        conn,
        "Raw",
        "Manga",
        RAW,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    derive::derive(conn).expect("derives");
    let en = digest(conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    read(conn, &en, "whole");
    read(conn, &ja, "whole");
    let id = pair(conn, &en, &ja, 0.95);
    (en, ja, id)
}

/// One English volume and its raw, each in a series of its own.
#[test]
fn a_raw_volume_joins_the_english_volumes_work_as_an_edition() {
    let shelf = Shelf::new("joins-apply-raw");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, id) = english_and_raw(&shelf, &conn);
    let english = holder(&conn, &en);
    let was = work(&conn, english);
    assert_eq!(
        (was.clone(), holder(&conn, &ja) == english),
        (("Oshi".to_owned(), Some(1.0), "en".to_owned()), false),
        "the fixture: the English volume's work, and the raw on a work of its own"
    );

    let report = derive::derive(&conn).expect("derives the join");

    assert_eq!(count(&conn, "SELECT count(*) FROM work"), 1, "one work");
    assert_eq!(
        holder(&conn, &ja),
        english,
        "the raw is on the English work"
    );
    assert_eq!(
        work(&conn, english),
        was,
        "which keeps its series, volume and hub"
    );
    assert_eq!(
        editions(&conn, english),
        [labelled(VOLUMES, None), labelled(RAW, Some(id))],
        "the English edition as it was; the raw's under its own key, kind and language, joined by the pair"
    );
    assert_eq!(
        (report.series_holding_no_work, workless(&conn)),
        (1, vec!["Oshinoko".to_owned()]),
        "the raw's series holds no work, and is counted"
    );
    assert_eq!(report.joined, 1, "one file joined: {report:?}");
}

/// A side of [`hosting`]: a directory, the series its volume 1 is named for,
/// and the directory's label.
type Side<'a> = (&'a str, &'a str, (&'a str, &'a str, &'a str));

/// What [`hosting`] finds: each file `apply` moves, by its series, with the
/// series of the group it joins; and each side's series id and work id.
#[derive(Debug)]
struct Hosted {
    moved: Vec<(String, String)>,
    series: [i64; 2],
    works: [i64; 2],
}

/// Two directories each holding `<series>/v01/<series> v01.cbz`, derived
/// over the series rows `older` already holds, read whole and paired at 0.95.
fn hosting(label: &str, older: &[&str], one: Side<'_>, two: Side<'_>) -> Hosted {
    let shelf = Shelf::new(label);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for series in older {
        conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [series])
            .expect("an older series");
    }
    let sides = [one, two];
    let file = |series: &str| format!("{series}/v01/{series} v01.cbz");
    let roots = sides.map(|(name, series, labels)| {
        shelf.directory(&conn, name, "Manga", labels, &[&file(series)])
    });
    derive::derive(&conn).expect("derives");
    let digests: Vec<String> = sides
        .iter()
        .zip(&roots)
        .map(|((_, series, _), root)| digest(&conn, root, &file(series)))
        .collect();
    for file in &digests {
        read(&conn, file, "whole");
    }
    pair(&conn, &digests[0], &digests[1], 0.95);
    let (_, joins) = joined(&conn);
    let series_of = |file: &str| {
        let at = digests
            .iter()
            .position(|d| d == file)
            .expect("a side's file");
        sides[at].1.to_owned()
    };
    Hosted {
        moved: joins
            .moved
            .iter()
            .map(|(file, (host, _, _))| (series_of(file), host.clone()))
            .collect(),
        series: sides.map(|(_, series, _)| {
            conn.query_row(
                "SELECT id FROM series WHERE sort_title = ?1",
                [series],
                |r| r.get(0),
            )
            .expect("the side's series")
        }),
        works: [holder(&conn, &digests[0]), holder(&conn, &digests[1])],
    }
}

/// One fixture per step of the host's rank, each where the next step would
/// choose the other side: the series named Alpha is the older, as the first
/// derive makes it, unless a row of Zeta's comes first.
#[test]
fn the_host_ranks_official_then_fan_then_raw_then_older_series_then_older_work() {
    let mut wrong: Vec<String> = Vec::new();
    for (step, older, one, two) in [
        (
            "official over fan",
            &[][..],
            ("Fan", "Alpha", FAN),
            ("Volumes", "Zeta", VOLUMES),
        ),
        (
            "fan over raw",
            &[][..],
            ("Raw", "Alpha", RAW),
            ("Fan", "Zeta", FAN),
        ),
        (
            "raw over a kind outside the three",
            &[][..],
            ("Bootleg", "Alpha", ("scan", "bootleg", "en")),
            ("Raw", "Zeta", RAW),
        ),
        (
            "the older series over the older work",
            &["Zeta"][..],
            ("Volumes", "Alpha", VOLUMES),
            ("French", "Zeta", ("fr", "official", "fr")),
        ),
    ] {
        let hosted = hosting("joins-rank", older, one, two);
        let next_chooses_alpha = if older.is_empty() {
            hosted.series[0] < hosted.series[1]
        } else {
            hosted.works[0] < hosted.works[1] && hosted.series[1] < hosted.series[0]
        };
        assert!(
            next_chooses_alpha,
            "{step}, the fixture: the next step would choose Alpha: {hosted:?}"
        );
        if hosted.moved != [("Alpha".to_owned(), "Zeta".to_owned())] {
            wrong.push(format!("{step}: {:?}", hosted.moved));
        }
    }

    let shelf = Shelf::new("joins-rank-work");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let french = shelf.directory(
        &conn,
        "French",
        "Manga",
        ("fr", "official", "fr"),
        &["Oshi/v02/Oshi v02.cbz"],
    );
    derive::derive(&conn).expect("derives the second volume first");
    let english = shelf.directory(
        &conn,
        "English",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    derive::derive(&conn).expect("derives the first");
    let fr = digest(&conn, &french, "Oshi/v02/Oshi v02.cbz");
    let en = digest(&conn, &english, "Oshi/v01/Oshi v01.cbz");
    assert!(
        holder(&conn, &fr) < holder(&conn, &en) && volume("Oshi", 1.0) < volume("Oshi", 2.0),
        "the fixture: volume 2's work is the older, and volume 1's key sorts first"
    );
    read(&conn, &fr, "whole");
    read(&conn, &en, "whole");
    pair(&conn, &fr, &en, 0.95);
    let (_, joins) = joined(&conn);
    if joins.moved != BTreeMap::from([(en.clone(), volume("Oshi", 2.0))]) {
        wrong.push(format!("the older work over the key: {:?}", joins.moved));
    }

    assert!(wrong.is_empty(), "each step chooses its host: {wrong:#?}");
}

/// Where [`black_colour_raw`] moves each file, by name.
type Moves = Vec<(&'static str, GroupKey)>;

/// A store over the black-and-white volume `Oshi/v01`, the colour one
/// `Colour/v01`, both `en` from directories of their own, and the raw `raw`:
/// written into `shelf` when `write`, derived over the series rows `older`
/// already holds, read whole, and the raw paired with each English volume at
/// `(black, colour)`. Answers each series' id, by title, and where `apply`
/// moves each file.
fn black_colour_raw(
    shelf: &Shelf,
    write: bool,
    older: &[&str],
    raw: &'static str,
    (black, colour): (f64, f64),
) -> (BTreeMap<String, i64>, Moves) {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for series in older {
        conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [series])
            .expect("an older series");
    }
    let sides = [
        ("black", "Volumes", VOLUMES, "Oshi/v01/Oshi v01.cbz"),
        ("colour", "Colour", VOLUMES, "Colour/v01/Colour v01.cbz"),
        ("raw", "Raw", RAW, raw),
    ];
    let roots = sides.map(|(_, name, labels, file)| {
        let files: &[&str] = if write { &[file] } else { &[] };
        (shelf.directory(&conn, name, "Manga", labels, files), file)
    });
    derive::derive(&conn).expect("derives");
    let digests = roots.map(|(root, file)| digest(&conn, &root, file));
    for file in &digests {
        read(&conn, file, "whole");
    }
    pair(&conn, &digests[0], &digests[2], black);
    pair(&conn, &digests[1], &digests[2], colour);
    let series = conn
        .prepare("SELECT sort_title, id FROM series")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("the series");
    let (_, joins) = joined(&conn);
    let moves = joins
        .moved
        .into_iter()
        .map(|(file, host)| {
            let at = digests
                .iter()
                .position(|d| *d == file)
                .expect("a side's file");
            (sides[at].0, host)
        })
        .collect();
    (series, moves)
}

/// A raw matching both the black-and-white and the colour volume, which
/// cannot share a work: three fixtures, each where the next criterion would
/// choose colour, and for the last two stores of one set of files with the
/// older series swapped, so whichever way their digests fall, one store has
/// them choose the other side.
#[test]
fn a_raw_matching_black_and_white_and_colour_joins_the_higher_score() {
    let higher = Shelf::new("joins-colour-v");
    let (series, moves) = black_colour_raw(
        &higher,
        true,
        &[],
        "Oshinoko/v01/Oshinoko v01.cbz",
        (0.95, 0.91),
    );
    assert!(
        series["Colour"] < series["Oshi"],
        "the fixture: colour's series is the older"
    );
    assert_eq!(
        moves,
        [("raw", volume("Oshi", 1.0))],
        "at 0.95 against 0.91, the raw joins black and white"
    );

    let same_series = Shelf::new("joins-colour-series");
    let (series, moves) = black_colour_raw(
        &same_series,
        true,
        &[],
        "Oshi/v02/Oshi v02.cbz",
        (0.93, 0.93),
    );
    assert!(
        series["Colour"] < series["Oshi"],
        "the fixture: colour's series is the older"
    );
    assert_eq!(
        moves,
        [("raw", volume("Oshi", 1.0))],
        "at equal scores, the raw named in black and white's series joins it"
    );

    let older = Shelf::new("joins-colour-older");
    let mut wrong = Vec::new();
    for (write, first, host) in [
        (true, &[][..], volume("Colour", 1.0)),
        (false, &["Oshi"][..], volume("Oshi", 1.0)),
    ] {
        let (series, moves) = black_colour_raw(
            &older,
            write,
            first,
            "Oshinoko/v01/Oshinoko v01.cbz",
            (0.93, 0.93),
        );
        assert!(
            series["Colour"].max(series["Oshi"]) < series["Oshinoko"],
            "the fixture: the raw's series is the newest"
        );
        if moves != [("raw", host.clone())] {
            wrong.push((series, moves));
        }
    }
    assert!(
        wrong.is_empty(),
        "at equal scores and no series shared, the raw joins the older series: {wrong:#?}"
    );
}

/// The raw pairs better with black and white than with colour, whose series
/// is the older and so would host the three were colour's `en` let in.
#[test]
fn a_file_whose_key_the_component_holds_stays_where_its_name_put_it() {
    let shelf = Shelf::new("joins-key-held");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let colour = shelf.directory(
        &conn,
        "Colour",
        "Manga",
        VOLUMES,
        &["Colour/v01/Colour v01.cbz"],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    derive::derive(&conn).expect("derives");
    let black = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let tinted = digest(&conn, &colour, "Colour/v01/Colour v01.cbz");
    let ja = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    for file in [&black, &tinted, &ja] {
        read(&conn, file, "whole");
    }
    let joins_black = pair(&conn, &black, &ja, 0.95);
    let joins_colour = pair(&conn, &tinted, &ja, 0.91);
    let (english, coloured) = (holder(&conn, &black), holder(&conn, &tinted));

    derive::derive(&conn).expect("derives the join");
    let (_, joins) = joined(&conn);

    assert_eq!(
        (holder(&conn, &ja), holder(&conn, &tinted)),
        (english, coloured),
        "the raw is on black and white's work, and colour on its own"
    );
    assert_eq!(
        editions(&conn, english),
        [labelled(VOLUMES, None), labelled(RAW, Some(joins_black))],
        "black and white's work holds its own edition and the raw's"
    );
    assert_eq!(
        (work(&conn, coloured), editions(&conn, coloured)),
        (
            ("Colour".to_owned(), Some(1.0), "en".to_owned()),
            vec![labelled(VOLUMES, None)]
        ),
        "colour stays where its name put it, alone"
    );
    assert_eq!(
        joins.aligned,
        [edge(
            joins_black,
            0.95,
            (&black, volume("Oshi", 1.0), "en"),
            (&ja, volume("Oshi", 1.0), "ja"),
        )],
        "only the pair whose files end in one group is aligned, not {joins_colour}"
    );
}

/// The raw's work also holds a Korean edition no pair names, under a key the
/// English work lacks.
#[test]
fn an_untouched_edition_with_a_new_key_moves_with_its_work() {
    let shelf = Shelf::new("joins-untouched");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let korean_labels = ("ko", "official", "ko");
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    let korean = shelf.directory(
        &conn,
        "Korean",
        "Manga",
        korean_labels,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    let ko = digest(&conn, &korean, "Oshinoko/v01/Oshinoko v01.cbz");
    for file in [&en, &ja, &ko] {
        read(&conn, file, "whole");
    }
    let id = pair(&conn, &en, &ja, 0.95);
    let english = holder(&conn, &en);
    assert_eq!(
        (
            holder(&conn, &ja) == holder(&conn, &ko),
            holder(&conn, &ja) == english,
            named(&conn, id).contains(&ko),
        ),
        (true, false, false),
        "the fixture: the raw and the Korean share a work, and the pair does not name the Korean"
    );

    let report = derive::derive(&conn).expect("derives the join");
    let (_, joins) = joined(&conn);

    assert_eq!(
        (holder(&conn, &ja), holder(&conn, &ko)),
        (english, english),
        "both editions are on the English work"
    );
    assert_eq!(
        editions(&conn, english),
        [
            labelled(VOLUMES, None),
            labelled(RAW, Some(id)),
            labelled(korean_labels, Some(id)),
        ],
        "the Korean is joined by the pair that moved its group"
    );
    assert_eq!(
        joins.moved,
        BTreeMap::from([(ja, volume("Oshi", 1.0)), (ko, volume("Oshi", 1.0))]),
        "both files moved into volume 1 of Oshi"
    );
    assert_eq!(report.joined, 2, "two files joined: {report:?}");
}

/// As above, with a fan volume joined to the English one first, by a higher
/// pair: the Korean's mark is still the pair that moved its group, not the
/// first pair the work applied.
#[test]
fn an_untouched_edition_is_marked_by_its_groups_pair_not_the_works_first() {
    let shelf = Shelf::new("joins-untouched-second");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let korean_labels = ("ko", "official", "ko");
    let fan_labels = ("fan", "fan", "en");
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let fans = shelf.directory(
        &conn,
        "Fans",
        "Manga",
        fan_labels,
        &["Oshi Fan/v01/Oshi Fan v01.cbz"],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    let korean = shelf.directory(
        &conn,
        "Korean",
        "Manga",
        korean_labels,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let fan = digest(&conn, &fans, "Oshi Fan/v01/Oshi Fan v01.cbz");
    let ja = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    let ko = digest(&conn, &korean, "Oshinoko/v01/Oshinoko v01.cbz");
    for file in [&en, &fan, &ja, &ko] {
        read(&conn, file, "whole");
    }
    let first = pair(&conn, &en, &fan, 0.97);
    let id = pair(&conn, &en, &ja, 0.95);

    derive::derive(&conn).expect("derives the joins");

    let english = holder(&conn, &en);
    assert_eq!(
        editions(&conn, english),
        [
            labelled(VOLUMES, None),
            labelled(fan_labels, Some(first)),
            labelled(RAW, Some(id)),
            labelled(korean_labels, Some(id)),
        ],
        "the Korean is joined by the pair that moved its group"
    );
}

/// The host's key `volumes` sorts after the guest's `raw`, and neither work
/// was written before: the digests come from a first store over the same
/// files, and a second derives the join on its first pass.
#[test]
fn a_new_host_keeps_its_own_hub() {
    let shelf = Shelf::new("joins-new-hub");
    let host = ("volumes", "official", "en");
    let guest = ("raw", "raw", "ja");
    let learn = Library::open_in_memory().expect("open");
    let digests = {
        let conn = learn.conn();
        let volumes = shelf.directory(&conn, "Volumes", "Manga", host, &["Oshi/v01/Oshi v01.cbz"]);
        let raw = shelf.directory(
            &conn,
            "Raw",
            "Manga",
            guest,
            &["Oshinoko/v01/Oshinoko v01.cbz"],
        );
        derive::derive(&conn).expect("derives the first store");
        [
            digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz"),
            digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz"),
        ]
    };
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    shelf.directory(&conn, "Volumes", "Manga", host, &[]);
    shelf.directory(&conn, "Raw", "Manga", guest, &[]);
    for file in &digests {
        read(&conn, file, "whole");
    }
    let id = pair(&conn, &digests[0], &digests[1], 0.95);
    assert_eq!(
        count(&conn, "SELECT count(*) FROM work"),
        0,
        "the fixture: no work written"
    );

    derive::derive(&conn).expect("derives the join first");

    let joined_work = holder(&conn, &digests[0]);
    assert_eq!(
        (holder(&conn, &digests[1]), work(&conn, joined_work)),
        (
            joined_work,
            ("Oshi".to_owned(), Some(1.0), "volumes".to_owned())
        ),
        "one work, the host's, with the host's own key as its hub"
    );
    assert_eq!(
        editions(&conn, joined_work),
        [labelled(guest, Some(id)), labelled(host, None)],
        "holding both editions"
    );
}

/// Official, fan and raw volumes, each read whole, no pair stored yet.
struct Triangle {
    library: Library,
    official: String,
    fan: String,
    raw: String,
}

impl Triangle {
    fn new(shelf: &Shelf) -> Self {
        let library = Library::open_in_memory().expect("open");
        let (official, fan, raw) = {
            let conn = library.conn();
            let volumes = shelf.directory(
                &conn,
                "Volumes",
                "Manga",
                VOLUMES,
                &["Oshi/v01/Oshi v01.cbz"],
            );
            let fans = shelf.directory(
                &conn,
                "Fan",
                "Manga",
                FAN,
                &["Oshi Scans/v01/Oshi Scans v01.cbz"],
            );
            let raws = shelf.directory(
                &conn,
                "Raw",
                "Manga",
                RAW,
                &["Oshinoko/v01/Oshinoko v01.cbz"],
            );
            derive::derive(&conn).expect("derives");
            let files = (
                digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz"),
                digest(&conn, &fans, "Oshi Scans/v01/Oshi Scans v01.cbz"),
                digest(&conn, &raws, "Oshinoko/v01/Oshinoko v01.cbz"),
            );
            for file in [&files.0, &files.1, &files.2] {
                read(&conn, file, "whole");
            }
            files
        };
        Self {
            library,
            official,
            fan,
            raw,
        }
    }
}

/// Review Focus 1: the raw pairs best with the official volume, then with the
/// fan's. Splitting the first frees the raw though it still pairs with the fan,
/// and the fan stays, joined by its own pair with the official volume;
/// splitting both frees only the raw.
#[test]
fn a_triangle_becomes_one_work_and_splitting_the_raws_pairs_frees_only_the_raw() {
    let shelf = Shelf::new("joins-triangle-split");
    let triangle = Triangle::new(&shelf);
    let conn = triangle.library.conn();
    let (official, fan, raw) = (&triangle.official, &triangle.fan, &triangle.raw);
    let official_raw = pair(&conn, official, raw, 0.93);
    let fan_raw = pair(&conn, fan, raw, 0.91);
    let official_fan = pair(&conn, official, fan, 0.89);
    let host = holder(&conn, official);

    derive::derive(&conn).expect("derives the triangle");

    assert_eq!(
        (holder(&conn, fan), holder(&conn, raw)),
        (host, host),
        "all three are one work, the official volume's"
    );
    assert_eq!(
        editions(&conn, host),
        [
            labelled(VOLUMES, None),
            labelled(FAN, Some(fan_raw)),
            labelled(RAW, Some(official_raw)),
        ],
        "the raw joined by the official volume, the fan through the raw, at 0.91 over 0.89"
    );

    matches::set_state(&conn, &[official_raw], State::Split, None).expect("splits one");
    derive::derive(&conn).expect("derives after one split");

    let freed = holder(&conn, raw);
    assert_ne!(freed, host, "the raw leaves the official volume");
    assert_eq!(
        (work(&conn, freed), editions(&conn, freed)),
        (
            ("Oshinoko".to_owned(), Some(1.0), "ja".to_owned()),
            vec![labelled(RAW, None)]
        ),
        "where its name puts it, though it still pairs with the fan"
    );
    assert_eq!(
        (holder(&conn, fan), editions(&conn, host)),
        (
            host,
            vec![labelled(VOLUMES, None), labelled(FAN, Some(official_fan))]
        ),
        "the fan stays, marked by its pair with the official volume, not by one naming the raw"
    );

    matches::set_state(&conn, &[fan_raw], State::Split, None).expect("splits the other");
    derive::derive(&conn).expect("derives after both splits");
    let (groups, joins) = joined(&conn);

    assert_eq!(
        joins.moved,
        BTreeMap::from([(fan.clone(), volume("Oshi", 1.0))]),
        "only the fan joins"
    );
    assert_eq!(
        members(&groups, &volume("Oshinoko", 1.0), "ja"),
        1,
        "the raw is a group of its own again"
    );
    assert_eq!(
        (holder(&conn, fan), editions(&conn, host)),
        (
            host,
            vec![labelled(VOLUMES, None), labelled(FAN, Some(official_fan))]
        ),
        "the fan stays on the official volume's work, which keeps no copy of the raw"
    );
    let freed = holder(&conn, raw);
    assert_ne!(freed, host, "the raw has a work of its own");
    assert_eq!(
        (work(&conn, freed), editions(&conn, freed)),
        (
            ("Oshinoko".to_owned(), Some(1.0), "ja".to_owned()),
            vec![labelled(RAW, None)]
        ),
        "where its name puts it, joined by nothing"
    );
}

/// V(fan, raw) > V(official, raw) > V(official, fan): the fan takes the raw
/// first, and the official volume then takes both.
#[test]
fn a_triangle_whose_fan_and_raw_score_highest_marks_each_file_with_a_pair_naming_it() {
    let shelf = Shelf::new("joins-triangle-marks");
    let triangle = Triangle::new(&shelf);
    let conn = triangle.library.conn();
    let (official, fan, raw) = (&triangle.official, &triangle.fan, &triangle.raw);
    let fan_raw = pair(&conn, fan, raw, 0.95);
    let official_raw = pair(&conn, official, raw, 0.92);
    let official_fan = pair(&conn, official, fan, 0.89);
    let host = holder(&conn, official);

    derive::derive(&conn).expect("derives the triangle");
    let (_, joins) = joined(&conn);

    assert_eq!(
        (holder(&conn, fan), holder(&conn, raw)),
        (host, host),
        "all three are one work, the official volume's"
    );
    assert_eq!(
        editions(&conn, host),
        [
            labelled(VOLUMES, None),
            labelled(FAN, Some(fan_raw)),
            labelled(RAW, Some(fan_raw)),
        ],
        "the fan's mark is the pair naming it, not the official and raw pair that moved its group"
    );
    let at = |file: &str, key: &'static str| (file.to_owned(), volume("Oshi", 1.0), key);
    let sides =
        |one: (String, GroupKey, &'static str), two: (String, GroupKey, &'static str)| [one, two];
    let aligned: Vec<Edge> = [
        (fan_raw, 0.95, sides(at(fan, "fan"), at(raw, "ja"))),
        (official_raw, 0.92, sides(at(official, "en"), at(raw, "ja"))),
        (
            official_fan,
            0.89,
            sides(at(official, "en"), at(fan, "fan")),
        ),
    ]
    .into_iter()
    .map(|(id, v, [one, two])| edge(id, v, (&one.0, one.1, one.2), (&two.0, two.1, two.2)))
    .collect();
    assert_eq!(
        joins.aligned, aligned,
        "every pair's files end in the host's group"
    );
}

/// What [`stored_in_order`] answers: the store's editions, the pairs' ids in
/// the order they were stored, and each series' id by title.
type Stored = (Vec<Laid>, [i64; 2], BTreeMap<String, i64>);

/// The raw [`stored_in_order`] pairs, and the English volumes it can pair with.
const ALPHA: &str = "Alpha/v01/Alpha v01.cbz";
const BETA: &str = "Beta/v01/Beta v01.cbz";
const GAMMA: &str = "Gamma/v01/Gamma v01.cbz";

/// A store over the raw [`ALPHA`], under directory Raw, and the English
/// volumes `hosts`, under directory English: written into `shelf` when
/// `write`, derived over the series rows `older` already holds, the raw
/// paired at 0.93 with each English volume in the order `hosts` names them,
/// then derived again.
fn stored_in_order(shelf: &Shelf, write: bool, older: &[&str], hosts: [&str; 2]) -> Stored {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for series in older {
        conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [series])
            .expect("an older series");
    }
    let (raws, volumes): (&[&str], &[&str]) = if write {
        (&[ALPHA], &hosts)
    } else {
        (&[], &[])
    };
    let raw = shelf.directory(&conn, "Raw", "Manga", RAW, raws);
    let english = shelf.directory(&conn, "English", "Manga", VOLUMES, volumes);
    derive::derive(&conn).expect("derives");
    let ja = digest(&conn, &raw, ALPHA);
    let hosted = hosts.map(|host| digest(&conn, &english, host));
    for file in hosted.iter().chain([&ja]) {
        read(&conn, file, "whole");
    }
    let ids = hosted.each_ref().map(|host| pair(&conn, host, &ja, 0.93));
    derive::derive(&conn).expect("derives the join");
    let series = conn
        .prepare("SELECT sort_title, id FROM series")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("the series");
    (layout(&conn), ids, series)
}

/// The series of the work the raw's edition `ja` sits on.
fn raw_joined(laid: &[Laid]) -> Vec<&str> {
    laid.iter()
        .filter(|(_, _, key, _, _)| key == "ja")
        .map(|(series, _, _, _, _)| series.as_str())
        .collect()
}

/// One raw, in the oldest series, at equal V to two English volumes holding
/// one key; two stores of the same files, the pairs inserted in opposite
/// orders.
#[test]
fn the_outcome_does_not_depend_on_the_order_pairs_were_stored() {
    let shelf = Shelf::new("joins-store-order");
    let (beta_first, [beta, gamma], _) = stored_in_order(&shelf, true, &[], [BETA, GAMMA]);
    let (gamma_first, [gamma_then, beta_then], _) =
        stored_in_order(&shelf, false, &[], [GAMMA, BETA]);
    assert!(
        beta < gamma && gamma_then < beta_then,
        "the fixture: each store gave its first pair the lower id"
    );

    assert_eq!(
        raw_joined(&beta_first),
        ["Beta"],
        "the raw joins Beta, the older of the two hosts' series"
    );
    assert_eq!(
        beta_first, gamma_first,
        "the same works, editions and pairs in both stores"
    );
}

/// The raw's own series is the oldest, so the older of each pair's two series
/// is the same and the newer decides: two stores of one set of files, Beta
/// the older host in the first and Gamma in the second, so whichever way the
/// digests fall, one store has them choose the other side.
#[test]
fn at_equal_scores_the_raw_joins_the_host_in_the_older_series_whatever_its_own() {
    let shelf = Shelf::new("joins-newer-series");
    let mut wrong = Vec::new();
    for (write, older, host) in [
        (true, &[][..], "Beta"),
        (false, &["Alpha", "Gamma"][..], "Gamma"),
    ] {
        let (laid, _, series) = stored_in_order(&shelf, write, older, [BETA, GAMMA]);
        assert!(
            series["Alpha"] < series["Beta"].min(series["Gamma"])
                && series[host] == series["Beta"].min(series["Gamma"]),
            "the fixture: Alpha is the oldest series, then {host}: {series:?}"
        );
        if raw_joined(&laid) != [host] {
            wrong.push((host, laid));
        }
    }
    assert!(
        wrong.is_empty(),
        "the raw joins the host in the older series: {wrong:#?}"
    );
}

/// Equal V, and the two hosts volumes of one series, so neither series breaks
/// the tie and only the digests can: two stores of the same files, the pairs
/// inserted in opposite orders, which a sort that stopped short of the
/// digests would follow.
#[test]
fn a_tie_no_series_breaks_goes_to_the_digests_whatever_order_the_pairs_were_stored() {
    let shelf = Shelf::new("joins-digest-tie");
    let second_volume = "Beta/v02/Beta v02.cbz";
    let (first, [one, two], _) = stored_in_order(&shelf, true, &[], [BETA, second_volume]);
    let (second, [two_then, one_then], _) =
        stored_in_order(&shelf, false, &[], [second_volume, BETA]);
    assert!(
        one < two && two_then < one_then,
        "the fixture: each store gave its first pair the lower id"
    );
    let ja = &first
        .iter()
        .find(|(_, _, key, _, _)| key == "ja")
        .expect("the raw's edition")
        .3;
    let pair_of = |host: &String| {
        let mut files = [host.clone(), ja.clone()];
        files.sort();
        files
    };
    let smaller = first
        .iter()
        .filter(|(_, _, key, _, _)| key == "en")
        .min_by_key(|(_, _, _, host, _)| pair_of(host))
        .map(|(series, volume, _, _, _)| (series.clone(), *volume));
    let joined: Vec<(String, Option<f64>)> = first
        .iter()
        .filter(|(_, _, key, _, _)| key == "ja")
        .map(|(series, volume, _, _, _)| (series.clone(), *volume))
        .collect();

    assert_eq!(
        joined,
        Vec::from_iter(smaller),
        "the raw joins the volume whose pair's digests sort first"
    );
    assert_eq!(
        first, second,
        "the same works, editions and pairs in both stores"
    );
}

/// An English scanlation of Oshinoko volume 1.
const EN_SCANS: (&str, &str, &str) = ("en", "fan", "en");

/// What [`partial`] lays out: the three files' digests, the pair, and the
/// works of the English volume and of the raw.
struct Partial {
    ja: String,
    scanned: String,
    id: i64,
    english: i64,
    old: i64,
}

/// Oshi volume 1 under `volumes`; the Oshinoko raw and a scanlation under
/// `scans`, which names put on one work; the first two read whole and paired
/// at 0.95, derived once. The scanlation's key is the Oshi work's, so only
/// the raw can join it.
fn partial(
    shelf: &Shelf,
    conn: &Connection,
    volumes: (&str, &str, &str),
    scans: (&str, &str, &str),
) -> Partial {
    let file = "Oshinoko/v01/Oshinoko v01.cbz";
    let english = shelf.directory(
        conn,
        "Volumes",
        "Manga",
        volumes,
        &["Oshi/v01/Oshi v01.cbz"],
    );
    let raw = shelf.directory(conn, "Raw", "Manga", RAW, &[file]);
    let scanlation = shelf.directory(conn, "Scans", "Manga", scans, &[file]);
    derive::derive(conn).expect("derives");
    let en = digest(conn, &english, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(conn, &raw, file);
    let scanned = digest(conn, &scanlation, file);
    for file in [&en, &ja, &scanned] {
        read(conn, file, "whole");
    }
    let id = pair(conn, &en, &ja, 0.95);
    let (english, old) = (holder(conn, &en), holder(conn, &ja));
    assert_eq!(
        holder(conn, &scanned),
        old,
        "the fixture: the raw and the scanlation share a work"
    );
    Partial {
        ja,
        scanned,
        id,
        english,
        old,
    }
}

/// Review Focus 3: the raw's work also holds an English edition no pair names,
/// under the key the English volume's work holds, so the raw joins and that
/// edition stays on the old work.
#[test]
fn a_partial_join_leaves_the_guests_other_edition_on_its_old_work() {
    let shelf = Shelf::new("joins-partial");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let Partial {
        ja,
        scanned,
        id,
        english,
        old,
    } = partial(&shelf, &conn, VOLUMES, EN_SCANS);

    let report = derive::derive(&conn).expect("derives the join");
    let (groups, joins) = joined(&conn);

    assert_eq!(
        joins.moved,
        BTreeMap::from([(ja.clone(), volume("Oshi", 1.0))]),
        "the raw joins, alone"
    );
    assert_eq!(
        members(&groups, &volume("Oshinoko", 1.0), "en"),
        1,
        "the other English edition stays in its group"
    );
    assert_eq!(
        editions(&conn, english),
        [labelled(VOLUMES, None), labelled(RAW, Some(id))],
        "the English work takes the raw"
    );
    assert_eq!(
        holder(&conn, &scanned),
        old,
        "the other English edition stays on the old work"
    );
    assert!(
        editions(&conn, old).contains(&labelled(EN_SCANS, None)),
        "unmarked: {:?}",
        editions(&conn, old)
    );
    assert_eq!(report.joined, 1, "one file joined: {report:?}");
}

/// Aoi holds en and fr; Bara a raw and an English edition under en; Chiyo a
/// Korean volume; Dango an English edition under en and a Korean one. Aoi's
/// fr pairs best with Bara's raw, which joins, leaving Bara's en behind;
/// Bara's en then pairs with Chiyo, and Aoi's fr with Dango's en, whose key
/// Aoi holds. Neither brings anything into Aoi's work: Dango stays where it
/// is, and Bara's en, a component of its own once left behind, takes Chiyo
/// in where its name puts it.
#[test]
fn a_file_a_join_leaves_behind_brings_nothing_into_it_but_may_host_its_own() {
    let shelf = Shelf::new("joins-left-behind");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let french = ("fr", "official", "fr");
    let korean = ("ko", "raw", "ko");
    let file = |series: &str| format!("{series}/v01/{series} v01.cbz");
    let mut roots = BTreeMap::new();
    for (name, labels, series) in [
        ("English", VOLUMES, &["Aoi", "Bara", "Dango"][..]),
        ("French", french, &["Aoi"][..]),
        ("Raw", RAW, &["Bara"][..]),
        ("Korean", korean, &["Chiyo", "Dango"][..]),
    ] {
        let files: Vec<String> = series.iter().map(|series| file(series)).collect();
        let named: Vec<&str> = files.iter().map(String::as_str).collect();
        roots.insert(name, shelf.directory(&conn, name, "Manga", labels, &named));
    }
    derive::derive(&conn).expect("derives");
    let at = |name: &str, series: &str| digest(&conn, &roots[name], &file(series));
    let (aoi_fr, bara_ja, bara_en) = (
        at("French", "Aoi"),
        at("Raw", "Bara"),
        at("English", "Bara"),
    );
    let (chiyo_ko, dango_en) = (at("Korean", "Chiyo"), at("English", "Dango"));
    for file in [&aoi_fr, &bara_ja, &bara_en, &chiyo_ko, &dango_en] {
        read(&conn, file, "whole");
    }
    pair(&conn, &aoi_fr, &bara_ja, 0.95);
    pair(&conn, &aoi_fr, &dango_en, 0.93);
    pair(&conn, &bara_en, &chiyo_ko, 0.91);

    let (groups, joins) = joined(&conn);

    assert_eq!(
        joins.moved,
        BTreeMap::from([
            (bara_ja, volume("Aoi", 1.0)),
            (chiyo_ko, volume("Bara", 1.0))
        ]),
        "only Bara's raw joins Aoi, and Chiyo joins Bara's en"
    );
    let mut wrong = Vec::new();
    for (series, key) in [
        ("Bara", "en"),
        ("Bara", "ko"),
        ("Dango", "en"),
        ("Dango", "ko"),
    ] {
        if members(&groups, &volume(series, 1.0), key) != 1 {
            wrong.push(format!("{series} {key}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "each is in Bara's or its own group: {wrong:?}"
    );
}

/// O272 for a join: a derive over the store a join was written into, with
/// nothing changed, commits nothing, counted as a pass that writes its one
/// work: the raw's mark cleared behind its back and nothing else, so only a
/// derive that reads the mark as a change writes it back.
#[test]
fn a_second_derive_commits_nothing() {
    let shelf = Shelf::new("joins-quiet");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let (_, ja, id) = english_and_raw(&shelf, &conn);
    derive::derive(&conn).expect("derives the join");
    conn.execute("UPDATE edition SET joined_by = NULL", [])
        .expect("the mark the next derive writes back");
    let writing = derive::derive(&conn).expect("a derive that writes the work");
    assert_eq!(
        editions(&conn, holder(&conn, &ja))[1],
        labelled(RAW, Some(id)),
        "the derive that writes the work writes the raw's mark back"
    );

    let changes = conn.total_changes();
    let counter = crate::structure::version(&conn).expect("reads");
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    let quiet = quiet.expect("a derive over nothing changed");

    assert_eq!(quiet, writing, "counted as a pass that writes");
    assert_eq!(quiet.joined, 1, "the join counted again: {quiet:?}");
    assert_eq!(commits, 0, "no write transaction committed");
    assert_eq!(conn.total_changes(), changes, "no row changed");
    assert_eq!(
        crate::structure::version(&conn).expect("reads"),
        counter,
        "the structure counter stays"
    );
}

/// The English volume and its raw paired after the first derive: a preview
/// of the rules as they stand derives the join and rolls it back.
#[test]
fn a_preview_shows_the_join_and_keeps_none() {
    let shelf = Shelf::new("joins-preview");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, _) = english_and_raw(&shelf, &conn);
    let (english, raws) = (holder(&conn, &en), holder(&conn, &ja));
    let current = rules::read(&conn).expect("reads the rules").0;

    let impact = preview::preview(&conn, &current).expect("previews");

    assert_eq!(
        (
            impact.series_before,
            impact.series_after,
            impact.works_rehomed
        ),
        (2, 1, 1),
        "the raw's file moves into the English series: {impact:?}"
    );
    assert_eq!(
        (holder(&conn, &en), holder(&conn, &ja)),
        (english, raws),
        "each file stays on its own work"
    );
    assert_eq!(
        editions(&conn, english),
        [labelled(VOLUMES, None)],
        "and the English work holds no raw edition"
    );
}

/// Review Focus 4: a derive cancelled once the host is written, before the
/// raw's old work is folded, leaves the raw's file on two works.
#[test]
fn a_derive_cancelled_after_a_join_converges_on_the_next() {
    let shelf = Shelf::new("joins-cancelled");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let (en, ja, _) = english_and_raw(&shelf, &conn);
    let english = holder(&conn, &en);
    let watch = Recording::cancelling(|(step, written, _)| step == Step::Writing && written > 0);

    let outcome =
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");
    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Writing)),
        "{outcome:?}"
    );
    assert_eq!(
        holders(&conn, &ja).len(),
        2,
        "the fixture: the raw stands on the English work and on its old one"
    );

    derive::derive(&conn).expect("derives again");

    assert_eq!(
        (
            count(&conn, "SELECT count(*) FROM work"),
            holders(&conn, &ja)
        ),
        (1, vec![english]),
        "one work, the English, holds the raw"
    );
    let (commits, report) = commits_during(&mut conn, derive::derive);
    report.expect("derives a third time");
    assert_eq!(commits, 0, "and the derive after commits nothing");
}

/// A hand title on the raw's work holds the raw where it is; on the English
/// work, it does not stop the join.
#[test]
fn a_hand_value_holds_the_guest_and_never_stops_the_host() {
    let shelf = Shelf::new("joins-hand-host");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, id) = english_and_raw(&shelf, &conn);
    let (english, raws) = (holder(&conn, &en), holder(&conn, &ja));
    let titled = "UPDATE work SET title_hand = 'Oshi no Ko', title = 'Oshi no Ko' WHERE id = ?1";
    conn.execute(titled, [raws])
        .expect("an admin titles the raw's work");

    derive::derive(&conn).expect("derives");

    assert_eq!(
        (holder(&conn, &ja), editions(&conn, english)),
        (raws, vec![labelled(VOLUMES, None)]),
        "the raw stays on its own work"
    );

    conn.execute("UPDATE work SET title_hand = NULL WHERE id = ?1", [raws])
        .expect("the admin clears it");
    conn.execute(titled, [english])
        .expect("and titles the English work");
    derive::derive(&conn).expect("derives");

    assert_eq!(
        (holder(&conn, &ja), editions(&conn, english)),
        (
            english,
            vec![labelled(VOLUMES, None), labelled(RAW, Some(id))]
        ),
        "the English work takes the raw"
    );
}

/// Volume `n` of the official English Oshi, in a directory of its own.
fn oshi(n: u32) -> String {
    format!("Oshi/v{n:02}/Oshi v{n:02}.cbz")
}

/// Volume `n` of the raw Oshinoko, beside the other raw volumes in one
/// directory, which is what a directory follows by.
fn oshinoko(n: u32) -> String {
    format!("Oshinoko/Oshinoko v{n:02}.cbz")
}

/// A scanlation's directory, whose key sorts after the raws': a check of a
/// group's first file alone would read the raw's directory and move it.
const SCANS: (&str, &str, &str) = ("scans", "fan", "en");

/// The English and raw directories [`shelved`] adds.
struct Shelved {
    volumes: Root,
    raw: Root,
}

impl Shelved {
    fn en(&self, conn: &Connection, n: u32) -> String {
        digest(conn, &self.volumes, &oshi(n))
    }

    fn ja(&self, conn: &Connection, n: u32) -> String {
        digest(conn, &self.raw, &oshinoko(n))
    }

    /// Each of English `volumes` paired with its raw at 0.95: the pairs.
    fn pair_volumes(&self, conn: &Connection, volumes: &[u32]) -> Vec<i64> {
        volumes
            .iter()
            .map(|&n| pair(conn, &self.en(conn, n), &self.ja(conn, n), 0.95))
            .collect()
    }
}

fn as_strs(files: &[String]) -> Vec<&str> {
    files.iter().map(String::as_str).collect()
}

/// `english` under an English directory and `raws` under a raw one, derived,
/// every file read whole.
fn shelved(shelf: &Shelf, conn: &Connection, english: &[String], raws: &[String]) -> Shelved {
    let volumes = shelf.directory(conn, "Volumes", "Manga", VOLUMES, &as_strs(english));
    let raw = shelf.directory(conn, "Raw", "Manga", RAW, &as_strs(raws));
    derive::derive(conn).expect("derives");
    for (root, files) in [(&volumes, english), (&raw, raws)] {
        for file in files {
            read(conn, &digest(conn, root, file), "whole");
        }
    }
    Shelved { volumes, raw }
}

/// A work as [`work`] reads one: in `series` at `volume`, its hub the raw's.
fn raw_at(series: &str, volume: u32) -> (String, Option<f64>, String) {
    (series.to_owned(), Some(f64::from(volume)), "ja".to_owned())
}

/// Raws 1 to 5 in one directory beside English 1 to 3, the first three
/// paired: raws 4 and 5 follow into the English series on their own works,
/// joined by nothing, and a second derive commits nothing.
#[test]
fn raw_volumes_past_the_last_english_one_follow_their_directory() {
    let shelf = Shelf::new("joins-follow");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let raws: Vec<String> = (1..=5).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    let past: Vec<i64> = (4..=5)
        .map(|n| holder(&conn, &shelved.ja(&conn, n)))
        .collect();
    assert_eq!(
        past.iter().map(|&id| work(&conn, id)).collect::<Vec<_>>(),
        [raw_at("Oshinoko", 4), raw_at("Oshinoko", 5)],
        "the fixture: raws 4 and 5 where their names put them"
    );
    let ids = shelved.pair_volumes(&conn, &[1, 2, 3]);

    let report = derive::derive(&conn).expect("derives the joins");

    for (n, id) in (1..=3).zip(&ids) {
        let english = holder(&conn, &shelved.en(&conn, n));
        assert_eq!(
            (
                holder(&conn, &shelved.ja(&conn, n)),
                editions(&conn, english)
            ),
            (
                english,
                vec![labelled(VOLUMES, None), labelled(RAW, Some(*id))]
            ),
            "raw {n} joined English {n}"
        );
    }
    for (n, id) in (4..=5).zip(&past) {
        assert_eq!(
            (
                holder(&conn, &shelved.ja(&conn, n)),
                work(&conn, *id),
                editions(&conn, *id)
            ),
            (*id, raw_at("Oshi", n), vec![labelled(RAW, None)]),
            "raw {n} on its own work, in the English series at volume {n}, joined by nothing"
        );
    }
    assert_eq!(
        (report.joined, report.followed),
        (3, 2),
        "three joined, two followed: {report:?}"
    );
    assert_eq!(
        workless(&conn),
        ["Oshinoko"],
        "the raws' series holds no work"
    );

    let (commits, again) = commits_during(&mut conn, derive::derive);
    assert_eq!(
        (commits, again.expect("derives again").followed),
        (0, 2),
        "a second derive commits nothing and counts the followers again"
    );
}

/// Raw 2 has no pair though English 2 is there, so the series holds volume
/// 2 and raw 2 stays where its name puts it; raw 4 shows the directory
/// follows.
#[test]
fn a_follower_fills_only_volumes_the_series_lacks() {
    let shelf = Shelf::new("joins-follow-lacks");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let raws: Vec<String> = (1..=4).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    let (second, fourth) = (
        holder(&conn, &shelved.ja(&conn, 2)),
        holder(&conn, &shelved.ja(&conn, 4)),
    );
    shelved.pair_volumes(&conn, &[1, 3]);

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (holder(&conn, &shelved.ja(&conn, 2)), work(&conn, second)),
        (second, raw_at("Oshinoko", 2)),
        "raw 2 stays on its own work where its name puts it"
    );
    assert_eq!(
        editions(&conn, holder(&conn, &shelved.en(&conn, 2))),
        [labelled(VOLUMES, None)],
        "and English 2 holds only itself"
    );
    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 4)),
            work(&conn, fourth),
            report.followed
        ),
        (fourth, raw_at("Oshi", 4), 1),
        "raw 4, a volume the series lacks, follows"
    );
}

/// Raw 1 joins Oshi and raw 2 joins Oshi Alt: a tie, and raw 4 stays. Raw 3
/// joining Oshi then breaks the tie, and raw 4 follows into Oshi.
#[test]
fn a_tie_between_series_moves_nothing() {
    let shelf = Shelf::new("joins-follow-tie");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let alt = "Oshi Alt/v02/Oshi Alt v02.cbz".to_owned();
    let raws: Vec<String> = (1..=4).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[oshi(1), oshi(3), alt.clone()], &raws);
    let other = digest(&conn, &shelved.volumes, &alt);
    let fourth = holder(&conn, &shelved.ja(&conn, 4));
    pair(&conn, &shelved.en(&conn, 1), &shelved.ja(&conn, 1), 0.95);
    pair(&conn, &other, &shelved.ja(&conn, 2), 0.95);

    let tied = derive::derive(&conn).expect("derives the tie");

    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 1)),
            holder(&conn, &shelved.ja(&conn, 2))
        ),
        (holder(&conn, &shelved.en(&conn, 1)), holder(&conn, &other)),
        "the fixture: raw 1 joined Oshi and raw 2 Oshi Alt"
    );
    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 4)),
            work(&conn, fourth),
            tied.followed
        ),
        (fourth, raw_at("Oshinoko", 4), 0),
        "raw 4 stays where its name puts it"
    );

    shelved.pair_volumes(&conn, &[3]);
    let broken = derive::derive(&conn).expect("derives with the tie broken");

    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 4)),
            work(&conn, fourth),
            broken.followed
        ),
        (fourth, raw_at("Oshi", 4), 1),
        "two joins to Oshi against one leads, and raw 4 follows into Oshi"
    );
}

/// A scanlation of volume 4 from a directory no join came from shares raw
/// 4's work, so that work stays; raw 5, alone on its work, follows.
#[test]
fn a_work_follows_only_when_every_file_follows() {
    let shelf = Shelf::new("joins-follow-every");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let scans = shelf.directory(&conn, "Scans", "Manga", SCANS, &[&oshinoko(4)]);
    let raws: Vec<String> = (1..=5).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    let scan = digest(&conn, &scans, &oshinoko(4));
    let (fourth, fifth) = (
        holder(&conn, &shelved.ja(&conn, 4)),
        holder(&conn, &shelved.ja(&conn, 5)),
    );
    assert_eq!(
        (holder(&conn, &scan), editions(&conn, fourth)),
        (fourth, vec![labelled(RAW, None), labelled(SCANS, None)]),
        "the fixture: raw 4 and the scanlation on one work"
    );
    shelved.pair_volumes(&conn, &[1, 2, 3]);

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 4)),
            holder(&conn, &scan),
            work(&conn, fourth)
        ),
        (fourth, fourth, raw_at("Oshinoko", 4)),
        "raw 4 and the scanlation stay together where their names put them"
    );
    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 5)),
            work(&conn, fifth),
            report.followed
        ),
        (fifth, raw_at("Oshi", 5), 1),
        "raw 5 follows"
    );
}

/// Raw 4 in two copies under one key, which takes no part in matching,
/// still follows with its directory.
#[test]
fn a_volume_held_in_two_copies_still_follows() {
    let shelf = Shelf::new("joins-follow-copies");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let copy = "Oshinoko/Oshinoko v04 (2).cbz".to_owned();
    let mut raws: Vec<String> = (1..=4).map(oshinoko).collect();
    raws.push(copy.clone());
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    let copied = digest(&conn, &shelved.raw, &copy);
    let fourth = holder(&conn, &shelved.ja(&conn, 4));
    assert_eq!(
        (
            holder(&conn, &copied),
            members(&groups(&conn), &volume("Oshinoko", 4.0), "ja")
        ),
        (fourth, 2),
        "the fixture: both copies are one edition of raw 4's work"
    );
    shelved.pair_volumes(&conn, &[1, 2, 3]);

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 4)),
            holder(&conn, &copied),
            work(&conn, fourth),
            report.followed
        ),
        (fourth, fourth, raw_at("Oshi", 4), 1),
        "both copies follow on their work"
    );
}

/// An admin titled raw 4's work by hand, which holds it where it is; raw 5
/// shows the directory follows.
#[test]
fn a_held_work_does_not_follow() {
    let shelf = Shelf::new("joins-follow-held");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let raws: Vec<String> = (1..=5).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    let (fourth, fifth) = (
        holder(&conn, &shelved.ja(&conn, 4)),
        holder(&conn, &shelved.ja(&conn, 5)),
    );
    conn.execute(
        "UPDATE work SET title_hand = 'Oshi no Ko 4', title = 'Oshi no Ko 4' WHERE id = ?1",
        [fourth],
    )
    .expect("an admin titles raw 4's work");
    shelved.pair_volumes(&conn, &[1, 2, 3]);

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (holder(&conn, &shelved.ja(&conn, 4)), work(&conn, fourth)),
        (fourth, raw_at("Oshinoko", 4)),
        "raw 4 stays where its name puts it"
    );
    assert_eq!(
        (
            holder(&conn, &shelved.ja(&conn, 5)),
            work(&conn, fifth),
            report.followed
        ),
        (fifth, raw_at("Oshi", 5), 1),
        "raw 5 follows"
    );
}

/// Every work in the store, named by what two stores of the same files
/// share: its series, volume, hub, title and slug.
type Shown = (String, Option<f64>, String, String, String);

fn works(conn: &Connection) -> Vec<Shown> {
    conn.prepare(
        "SELECT s.sort_title, w.volume, w.hub, w.title, w.slug
           FROM work w JOIN series s ON s.id = w.series_id
          ORDER BY s.sort_title, w.volume, w.slug",
    )
    .and_then(|mut stmt| {
        stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect()
    })
    .expect("the store's works")
}

/// Raw 4 followed into Oshi at volume 4, a reader's place on it, and then
/// English 4 arrives, whose series sorts before the raws'. Raw 4 goes back
/// where its name puts it on the work it had, with the place, and English 4
/// takes a work of its own: the store the same files, rules and pairs give
/// without that history, which the next derive leaves as it stands.
#[test]
fn an_english_volume_arriving_where_a_raw_followed_takes_a_work_of_its_own() {
    let shelf = Shelf::new("joins-follow-arrival");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let raws: Vec<String> = (1..=5).map(oshinoko).collect();
    let before = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    before.pair_volumes(&conn, &[1, 2, 3]);
    derive::derive(&conn).expect("derives the joins");
    let ja4 = before.ja(&conn, 4);
    let followed = holder(&conn, &ja4);
    assert_eq!(
        work(&conn, followed),
        raw_at("Oshi", 4),
        "the fixture: raw 4 followed into Oshi"
    );
    reader(&conn);
    place(&conn, followed, "ja", 2);

    write_archive(&shelf.path("Volumes", &oshi(4)), 3);
    scan::scan_root(&conn, &before.volumes).expect("rescans the English directory");
    derive::derive(&conn).expect("derives English 4");

    let english = holder(&conn, &before.en(&conn, 4));
    assert_eq!(
        (
            holder(&conn, &ja4),
            work(&conn, followed),
            editions(&conn, followed)
        ),
        (followed, raw_at("Oshinoko", 4), vec![labelled(RAW, None)]),
        "raw 4 is back where its name puts it, alone on the work it had"
    );
    assert_eq!(
        (work(&conn, english), editions(&conn, english)),
        (
            ("Oshi".to_owned(), Some(4.0), "en".to_owned()),
            vec![labelled(VOLUMES, None)]
        ),
        "English 4 is on a work of its own"
    );
    assert_eq!(
        places(&conn)
            .iter()
            .map(|at| (at.work_id, at.edition.as_str(), at.slot))
            .collect::<Vec<_>>(),
        [(followed, "ja", 2)],
        "the reader's place is with raw 4"
    );
    let fresh = Library::open_in_memory().expect("open");
    let scratch = fresh.conn();
    let again = shelved(&shelf, &scratch, &[1, 2, 3, 4].map(oshi), &raws);
    again.pair_volumes(&scratch, &[1, 2, 3]);
    derive::derive(&scratch).expect("derives the same files with English 4 from the start");
    assert_eq!(
        (layout(&conn), works(&conn)),
        (layout(&scratch), works(&scratch)),
        "the store is the one the same files give without the history"
    );
    let (commits, _) = commits_during(&mut conn, derive::derive);
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// Raw 4 followed into Oshi, and then an admin sets a hand value on its
/// work, which holds the work where it stands: in Oshi, as followed. The
/// work an admin set a hand value on before any join, raw 5's, stays where
/// its name puts it, as `a_held_work_does_not_follow` holds.
#[test]
fn a_hand_value_on_a_followers_work_keeps_it_where_it_followed() {
    for (column, value) in [
        ("title_hand", "'Oshi no Ko 4'"),
        ("volume_hand", "4.0"),
        ("series_id_hand", "series_id"),
    ] {
        let shelf = Shelf::new("joins-follow-hand");
        let library = Library::open_in_memory().expect("open");
        let mut conn = library.conn();
        let raws: Vec<String> = (1..=5).map(oshinoko).collect();
        let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
        let fifth = holder(&conn, &shelved.ja(&conn, 5));
        conn.execute(
            "UPDATE work SET title_hand = 'Oshi no Ko 5', title = 'Oshi no Ko 5' WHERE id = ?1",
            [fifth],
        )
        .expect("an admin titles raw 5's work before any join");
        shelved.pair_volumes(&conn, &[1, 2, 3]);
        derive::derive(&conn).expect("derives the joins");
        let fourth = holder(&conn, &shelved.ja(&conn, 4));
        assert_eq!(
            (work(&conn, fourth), work(&conn, fifth)),
            (raw_at("Oshi", 4), raw_at("Oshinoko", 5)),
            "the fixture: raw 4 followed into Oshi and raw 5 stayed"
        );

        conn.execute(
            &format!("UPDATE work SET {column} = {value} WHERE id = ?1"),
            [fourth],
        )
        .expect("an admin sets a hand value on raw 4's work");
        let report = derive::derive(&conn).expect("derives after the hand value");

        assert_eq!(
            (
                holder(&conn, &shelved.ja(&conn, 4)),
                work(&conn, fourth),
                work(&conn, fifth),
                report.followed
            ),
            (fourth, raw_at("Oshi", 4), raw_at("Oshinoko", 5), 1),
            "{column}: raw 4 stays in Oshi on its work, and raw 5 in Oshinoko"
        );
        let (commits, _) = commits_during(&mut conn, derive::derive);
        assert_eq!(commits, 0, "{column}: the next derive commits nothing");
    }
}

/// Raw 4 followed into Oshi, a reader's place on it, and an admin then set
/// `column` to `value` on its work; then English 4 arrives. English 4 takes a
/// work of its own, and raw 4 stays alone on the work it had, with the place,
/// standing where `stands` says: the import finds that work by raw 4's
/// content, so it is not the address's to give to the volume arriving there.
fn a_hand_valued_follower_meets_the_volume_it_filled(
    label: &str,
    (column, value): (&str, &str),
    stands: (String, Option<f64>, String),
) {
    let shelf = Shelf::new(label);
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let raws: Vec<String> = (1..=5).map(oshinoko).collect();
    let before = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    before.pair_volumes(&conn, &[1, 2, 3]);
    derive::derive(&conn).expect("derives the joins");
    let ja4 = before.ja(&conn, 4);
    let followed = holder(&conn, &ja4);
    reader(&conn);
    place(&conn, followed, "ja", 2);
    conn.execute(
        &format!("UPDATE work SET {column} = {value} WHERE id = ?1"),
        [followed],
    )
    .expect("an admin sets a hand value on raw 4's work");
    derive::derive(&conn).expect("derives after the hand value");
    assert_eq!(
        work(&conn, followed),
        raw_at("Oshi", 4),
        "the fixture: raw 4 followed into Oshi"
    );

    write_archive(&shelf.path("Volumes", &oshi(4)), 3);
    scan::scan_root(&conn, &before.volumes).expect("rescans the English directory");
    derive::derive(&conn).expect("derives English 4");

    assert_eq!(
        (
            holders(&conn, &ja4),
            work(&conn, followed),
            editions(&conn, followed)
        ),
        (vec![followed], stands, vec![labelled(RAW, None)]),
        "raw 4 is alone on the work it had"
    );
    let english = holder(&conn, &before.en(&conn, 4));
    assert_eq!(
        (work(&conn, english), editions(&conn, english)),
        (
            ("Oshi".to_owned(), Some(4.0), "en".to_owned()),
            vec![labelled(VOLUMES, None)]
        ),
        "English 4 is on a work of its own"
    );
    assert_eq!(
        places(&conn)
            .iter()
            .map(|at| (at.work_id, at.edition.as_str(), at.slot))
            .collect::<Vec<_>>(),
        [(followed, "ja", 2)],
        "the reader's place is with raw 4"
    );
    let (commits, _) = commits_during(&mut conn, derive::derive);
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// A hand title holds no address, so raw 4 goes back where its name puts it.
#[test]
fn a_follower_with_a_hand_title_goes_back_when_the_volume_it_filled_arrives() {
    a_hand_valued_follower_meets_the_volume_it_filled(
        "joins-follow-hand-title-arrival",
        ("title_hand", "'Oshi no Ko 4'"),
        raw_at("Oshinoko", 4),
    );
}

/// A hand volume holds the volume and not the series, so raw 4 goes back to
/// Oshinoko at volume 4.
#[test]
fn a_follower_with_a_hand_volume_goes_back_when_the_volume_it_filled_arrives() {
    a_hand_valued_follower_meets_the_volume_it_filled(
        "joins-follow-hand-volume-arrival",
        ("volume_hand", "4.0"),
        raw_at("Oshinoko", 4),
    );
}

/// A hand series holds raw 4 in Oshi, on its own work beside English 4's.
#[test]
fn a_follower_with_a_hand_series_stays_beside_the_volume_it_filled_when_that_arrives() {
    a_hand_valued_follower_meets_the_volume_it_filled(
        "joins-follow-hand-series-arrival",
        ("series_id_hand", "series_id"),
        raw_at("Oshi", 4),
    );
}

/// A pause holds a directory's scans back, not the files it holds: pausing
/// the raws' directory, then the English one too, keeps every join and
/// follower, and neither derive commits anything.
#[test]
fn a_pause_keeps_every_join_and_follower() {
    let shelf = Shelf::new("joins-follow-pause");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let raws: Vec<String> = (1..=4).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[1, 2].map(oshi), &raws);
    let ids = shelved.pair_volumes(&conn, &[1, 2]);
    derive::derive(&conn).expect("derives the joins");
    let before = layout(&conn);
    assert_eq!(
        (
            before.iter().filter_map(|laid| laid.4.as_ref()).count(),
            (3..=4)
                .map(|n| work(&conn, holder(&conn, &shelved.ja(&conn, n))))
                .collect::<Vec<_>>()
        ),
        (ids.len(), vec![raw_at("Oshi", 3), raw_at("Oshi", 4)]),
        "the fixture: raws 1 and 2 joined, raws 3 and 4 followed"
    );

    for root in [&shelved.raw, &shelved.volumes] {
        conn.execute(
            "UPDATE root SET paused_at = datetime('now') WHERE id = ?1",
            [root.id],
        )
        .expect("pauses the directory");
        let (commits, report) = commits_during(&mut conn, derive::derive);
        let report = report.expect("derives with the pause");
        assert_eq!(
            (
                commits,
                report.joined,
                report.followed,
                layout(&conn) == before
            ),
            (0, 2, 2, true),
            "root {} paused: every join and follower stays, and nothing commits",
            root.id
        );
    }
}

/// Raws 4 and 5 followed three joins; once all three pairs are split their
/// directory has no join, and both go back to the raws' series on the works
/// they had throughout.
#[test]
fn a_split_that_leaves_a_directory_with_no_joins_takes_its_followers_back() {
    let shelf = Shelf::new("joins-follow-split");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let raws: Vec<String> = (1..=5).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    let past: Vec<i64> = (4..=5)
        .map(|n| holder(&conn, &shelved.ja(&conn, n)))
        .collect();
    let ids = shelved.pair_volumes(&conn, &[1, 2, 3]);
    derive::derive(&conn).expect("derives the joins");
    for (n, id) in (4..=5).zip(&past) {
        assert_eq!(
            (holder(&conn, &shelved.ja(&conn, n)), work(&conn, *id)),
            (*id, raw_at("Oshi", n)),
            "the fixture: raw {n} followed into Oshi on its own work"
        );
    }

    matches::set_state(&conn, &ids, State::Split, None).expect("splits all three");
    let report = derive::derive(&conn).expect("derives after the splits");

    for (n, id) in (4..=5).zip(&past) {
        assert_eq!(
            (holder(&conn, &shelved.ja(&conn, n)), work(&conn, *id)),
            (*id, raw_at("Oshinoko", n)),
            "raw {n} is back in the raws' series, on the work it had"
        );
    }
    assert_eq!(report.followed, 0, "{report:?}");
}

/// English and raw volume 1, which names already put in one group, paired:
/// the pair is aligned in that group, moves nothing and marks no edition.
#[test]
fn a_pair_names_already_together_is_in_aligned_and_marks_nothing() {
    let shelf = Shelf::new("joins-together-aligned");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(&conn, "Volumes", "Manga", VOLUMES, &[&oshi(1)]);
    let raw = shelf.directory(&conn, "Raw", "Manga", RAW, &[&oshi(1)]);
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, &oshi(1));
    let ja = digest(&conn, &raw, &oshi(1));
    read(&conn, &en, "whole");
    read(&conn, &ja, "whole");
    let id = pair(&conn, &en, &ja, 0.95);

    let (_, joins) = joined(&conn);

    assert_eq!(
        (joins.aligned, joins.moved),
        (
            vec![edge(
                id,
                0.95,
                (&en, volume("Oshi", 1.0), "en"),
                (&ja, volume("Oshi", 1.0), "ja"),
            )],
            BTreeMap::new()
        ),
        "aligned in volume 1's group, moving nothing"
    );

    let report = derive::derive(&conn).expect("derives");
    let english = holder(&conn, &en);

    assert_eq!(
        (holder(&conn, &ja), editions(&conn, english), report.joined),
        (
            english,
            vec![labelled(VOLUMES, None), labelled(RAW, None)],
            0
        ),
        "both editions on one work, neither marked by the pair"
    );
}

/// Raw and scanlation volumes 1 and 2 side by side: raw 1 pairs with English
/// 1, joining volume 1's group, and raw 2 with scanlation 2, which names put
/// in one group. Volume 2's group follows into Oshi, and its pair is aligned
/// where the group ends.
#[test]
fn a_pair_together_in_a_follower_is_aligned_where_it_follows() {
    let shelf = Shelf::new("joins-follow-aligned");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let scans = shelf.directory(
        &conn,
        "Scans",
        "Manga",
        SCANS,
        &[&oshinoko(1), &oshinoko(2)],
    );
    let raws: Vec<String> = (1..=2).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[oshi(1)], &raws);
    let scan = digest(&conn, &scans, &oshinoko(2));
    read(&conn, &scan, "whole");
    let (en, first, second) = (
        shelved.en(&conn, 1),
        shelved.ja(&conn, 1),
        shelved.ja(&conn, 2),
    );
    let joining = pair(&conn, &en, &first, 0.95);
    let together = pair(&conn, &second, &scan, 0.93);

    let (groups, joins) = joined(&conn);

    assert_eq!(
        (
            members(&groups, &volume("Oshi", 2.0), "ja"),
            members(&groups, &volume("Oshi", 2.0), "scans")
        ),
        (1, 1),
        "volume 2's group follows into Oshi"
    );
    assert_eq!(
        joins.aligned,
        [
            edge(
                joining,
                0.95,
                (&en, volume("Oshi", 1.0), "en"),
                (&first, volume("Oshi", 1.0), "ja"),
            ),
            edge(
                together,
                0.93,
                (&second, volume("Oshi", 2.0), "ja"),
                (&scan, volume("Oshi", 2.0), "scans"),
            ),
        ],
        "each pair aligned in the group it ends in"
    );
}

/// Oshinoko and Naruto volumes side by side at the top of one directory,
/// Oshinoko's first three joined into Oshi: Oshinoko 4 to 6 follow, and
/// Naruto 4 to 6, of a series nothing joined from there, stay in Naruto.
#[test]
fn a_directory_of_two_series_takes_along_only_the_series_that_joined() {
    let shelf = Shelf::new("joins-follow-mixed");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let flat = |series: &str, n: u32| format!("{series} v{n:02}.cbz");
    let mut raws: Vec<String> = (1..=6).map(|n| flat("Oshinoko", n)).collect();
    raws.extend((4..=6).map(|n| flat("Naruto", n)));
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    let raw = |series: &str, n: u32| digest(&conn, &shelved.raw, &flat(series, n));
    let works =
        |series: &str| -> Vec<i64> { (4..=6).map(|n| holder(&conn, &raw(series, n))).collect() };
    let (oshinoko, naruto) = (works("Oshinoko"), works("Naruto"));
    assert_eq!(
        oshinoko
            .iter()
            .chain(&naruto)
            .map(|&id| work(&conn, id))
            .collect::<Vec<_>>(),
        (4..=6)
            .map(|n| raw_at("Oshinoko", n))
            .chain((4..=6).map(|n| raw_at("Naruto", n)))
            .collect::<Vec<_>>(),
        "the fixture: each volume in the series its name gives it"
    );
    for n in 1..=3 {
        pair(&conn, &shelved.en(&conn, n), &raw("Oshinoko", n), 0.95);
    }

    let report = derive::derive(&conn).expect("derives");

    for (n, id) in (4..=6).zip(&oshinoko) {
        assert_eq!(
            (holder(&conn, &raw("Oshinoko", n)), work(&conn, *id)),
            (*id, raw_at("Oshi", n)),
            "Oshinoko {n} follows into Oshi"
        );
    }
    for (n, id) in (4..=6).zip(&naruto) {
        assert_eq!(
            (holder(&conn, &raw("Naruto", n)), work(&conn, *id)),
            (*id, raw_at("Naruto", n)),
            "Naruto {n} stays in Naruto"
        );
    }
    assert_eq!(report.followed, 3, "{report:?}");
}

/// Raw 1 hosts volume 9 of a directory with no kind, whose volumes 2 and 3
/// sit beside raws 2 and 3 and join Oshi 2 and 3 with them: both directories
/// lead Oshinoko's files to Oshi, which lacks volume 1, yet raw 1's group,
/// touched by the join it hosts, stays.
#[test]
fn a_group_a_join_touched_does_not_follow() {
    let shelf = Shelf::new("joins-follow-touched");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let misc = shelf.directory(
        &conn,
        "Misc",
        "Manga",
        ("misc", "fan", "en"),
        &[&oshinoko(2), &oshinoko(3), &oshinoko(9)],
    );
    conn.execute(
        "UPDATE root SET default_kind = NULL WHERE id = ?1",
        [misc.id],
    )
    .expect("gives the directory no kind");
    let raws: Vec<String> = (1..=3).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[2, 3].map(oshi), &raws);
    let ninth = digest(&conn, &misc, &oshinoko(9));
    read(&conn, &ninth, "whole");
    let first = holder(&conn, &shelved.ja(&conn, 1));
    let unkinded = |joined_by| ("misc".to_owned(), None, Some("en".to_owned()), joined_by);
    assert_eq!(
        editions(&conn, holder(&conn, &ninth)),
        [unkinded(None)],
        "the fixture: volume 9 has no kind, so a raw ranks above it as a host"
    );
    let hosted = pair(&conn, &shelved.ja(&conn, 1), &ninth, 0.95);
    shelved.pair_volumes(&conn, &[2, 3]);

    let report = derive::derive(&conn).expect("derives");

    for n in 2..=3 {
        assert_eq!(
            holder(&conn, &digest(&conn, &misc, &oshinoko(n))),
            holder(&conn, &shelved.en(&conn, n)),
            "the fixture: the kind-less volume {n} joined Oshi {n} with raw {n}"
        );
    }
    assert_eq!(
        (
            holder(&conn, &ninth),
            work(&conn, first),
            editions(&conn, first)
        ),
        (
            first,
            raw_at("Oshinoko", 1),
            vec![labelled(RAW, None), unkinded(Some(hosted))]
        ),
        "raw 1 hosts volume 9 where its name puts it"
    );
    assert_eq!(report.followed, 0, "{report:?}");
}

impl Shelf {
    /// Directory `name` of Manga, labelled `label`, holding `file` of `pages`
    /// pages whose bytes name `content` rather than the file's path, so its
    /// digest is the same on every run and a fixture can say which side of a
    /// pair the store puts first.
    fn holding(
        &self,
        conn: &Connection,
        name: &str,
        label: (&str, &str, &str),
        file: &str,
        (content, pages): (&str, usize),
    ) -> Root {
        let root = self.directory(conn, name, "Manga", label, &[]);
        write_archive_labelled(&self.path(name, file), pages, "jpg", content);
        scan::scan_root(conn, &root).expect("scans the file");
        root
    }
}

/// `one` and `two`, each with its page count, stored as a pair at 0.95 with
/// `floor` and the pass's whole-page `runs`, `[start, end, start, end]` from
/// `one`'s pages to `two`'s: the store turns them round when `two` is the
/// smaller digest. Answers the pair's id.
fn paged_pair(
    conn: &Connection,
    (one, one_pages): (&str, u32),
    (two, two_pages): (&str, u32),
    runs: &[[u32; 4]],
    floor: Option<f32>,
) -> i64 {
    let scored = NewPair {
        sha_a: one.to_owned(),
        sha_b: two.to_owned(),
        v: 0.95,
        long_paired: runs.iter().map(|[start, end, ..]| end - start + 1).sum(),
        a_slots: one_pages,
        b_slots: two_pages,
        a_pages: one_pages,
        b_pages: two_pages,
        floor,
        runs: runs
            .iter()
            .map(|&[a_start, a_end, b_start, b_end]| {
                (
                    PageRange::new(a_start, a_end),
                    PageRange::new(b_start, b_end),
                )
            })
            .collect(),
    };
    matches::store(conn, &[scored], one, 1, 1).expect("stores the pair");
    conn.query_row(
        "SELECT id FROM content_match WHERE ?1 IN (sha_a, sha_b) AND ?2 IN (sha_a, sha_b)",
        [one, two],
        |r| r.get(0),
    )
    .expect("the pair's id")
}

/// The official English volume and its raw, each in a series of its own and
/// each a file of the given content and pages, derived and read whole: the
/// two digests.
fn english_and_raw_of(
    shelf: &Shelf,
    conn: &Connection,
    en: (&str, usize),
    ja: (&str, usize),
) -> (String, String) {
    let volumes = shelf.holding(conn, "Volumes", VOLUMES, "Oshi/v01/Oshi v01.cbz", en);
    let raw = shelf.holding(conn, "Raw", RAW, "Oshinoko/v01/Oshinoko v01.cbz", ja);
    derive::derive(conn).expect("derives");
    let en = digest(conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    read(conn, &en, "whole");
    read(conn, &ja, "whole");
    (en, ja)
}

/// Whole pages `start` to `end`.
fn whole(start: u32, end: u32) -> SlotSpan {
    SlotSpan {
        start: Slot {
            page: start,
            half: None,
        },
        end: Slot {
            page: end,
            half: None,
        },
    }
}

/// A run pairing the from side's pages `from` with the to side's `to`.
fn paired(from: (u32, u32), to: (u32, u32)) -> RunEntry {
    RunEntry {
        a: whole(from.0, from.1),
        b: Some(whole(to.0, to.1)),
        margin: None,
        by: None,
        note: None,
    }
}

/// The from side's pages `from`, which no run pairs.
fn unpaired(from: (u32, u32)) -> RunEntry {
    RunEntry {
        a: whole(from.0, from.1),
        b: None,
        margin: None,
        by: None,
        note: None,
    }
}

/// A work's alignments, as `project::sidecar` reads them back.
fn alignments(conn: &Connection, work: i64) -> Vec<Alignment> {
    project::sidecar(conn, work)
        .unwrap_or_else(|e| panic!("projects work {work}: {e}"))
        .alignments
}

/// Every alignment row: id, work, from, to and by.
type AlignmentRow = (i64, i64, String, String, String);

fn alignment_rows(conn: &Connection) -> Vec<AlignmentRow> {
    conn.prepare("SELECT id, work_id, from_key, to_key, by FROM alignment ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect()
        })
        .expect("the alignment rows")
}

/// Every run row: id, alignment and ordinal.
fn run_rows(conn: &Connection) -> Vec<(i64, i64, i64)> {
    conn.prepare("SELECT id, alignment_id, ordinal FROM run ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("the run rows")
}

/// The English digest is the pair's a, so its stored runs already read from
/// the host: eight English pages against five, unpaired before, between and
/// after the two runs, with a floor.
#[test]
fn a_joined_pair_is_aligned_from_the_host_to_the_guest() {
    let shelf = Shelf::new("joins-aligned");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja) = english_and_raw_of(&shelf, &conn, ("en-1", 8), ("ja-1", 5));
    let id = paged_pair(
        &conn,
        (&en, 8),
        (&ja, 5),
        &[[1, 3, 0, 2], [5, 6, 3, 4]],
        Some(0.75),
    );
    assert_eq!(
        named(&conn, id),
        [en.clone(), ja.clone()],
        "the fixture: the English digest is the pair's a"
    );

    derive::derive(&conn).expect("derives the join");

    let english = holder(&conn, &en);
    assert_eq!(
        holder(&conn, &ja),
        english,
        "the raw joined the English work"
    );
    let stored = alignments(&conn, english);
    assert_eq!(stored.len(), 1, "one alignment: {stored:?}");
    let alignment = &stored[0];
    assert_eq!(
        (alignment.from.as_str(), alignment.to.as_str(), alignment.by),
        ("en", "ja", Source::Solver),
        "from the host's edition to the guest's, by the solver"
    );
    assert_eq!(
        alignment.runs,
        [
            unpaired((0, 0)),
            paired((1, 3), (0, 2)),
            unpaired((4, 4)),
            paired((5, 6), (3, 4)),
            unpaired((7, 7)),
        ],
        "each English page in one run, the pages no run pairs unpaired"
    );
    assert_eq!(
        alignment.quality,
        Some(Quality {
            verdict: Verdict::Fragmented,
            slots: 8,
            paired: 5,
            avg_run: 2.5,
        }),
        "the figures over those runs and the English page count"
    );
    assert_eq!(
        alignment.solver,
        Some(SolverInfo {
            hash: "dhash-16".to_owned(),
            floor: 0.75,
        }),
        "the pair's floor"
    );
}

/// Three editions names put in one work, the English its hub: the pair of
/// the fan translation, keyed `scans`, and the raw, keyed `ja`, neither the
/// hub, is aligned from the fan translation, the higher kind, though its key
/// sorts after the raw's and the hub would answer were it either. The fan's
/// digest is the pair's a, so a direction read off the hub alone would turn
/// it round, and so would one read off the keys alone.
#[test]
fn a_pair_without_the_hub_is_aligned_from_its_higher_kind() {
    let shelf = Shelf::new("joins-direction-no-hub");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let scans = ("scans", "fan", "en");
    let path = "Oshi/v01/Oshi v01.cbz";
    let volumes = shelf.holding(&conn, "Volumes", VOLUMES, path, ("en-3", 3));
    let fans = shelf.holding(&conn, "Fans", scans, path, ("fan-3", 3));
    let raw = shelf.holding(&conn, "Raw", RAW, path, ("ja-3", 3));
    derive::derive(&conn).expect("derives");
    let [en, fan, ja] = [&volumes, &fans, &raw].map(|root| digest(&conn, root, path));
    for file in [&en, &fan, &ja] {
        read(&conn, file, "whole");
    }
    let oshi = holder(&conn, &en);
    assert_eq!(
        (holder(&conn, &fan), holder(&conn, &ja), work(&conn, oshi).2),
        (oshi, oshi, "en".to_owned()),
        "the fixture: one work by names, the English its hub"
    );
    let id = paged_pair(&conn, (&fan, 3), (&ja, 3), &[[0, 2, 0, 2]], Some(0.75));
    assert_eq!(
        named(&conn, id),
        [fan.clone(), ja.clone()],
        "the fixture: the fan's digest is a"
    );

    derive::derive(&conn).expect("derives the alignment");

    assert_eq!(
        alignments(&conn, oshi)
            .iter()
            .map(|a| (a.from.as_str(), a.to.as_str()))
            .collect::<Vec<_>>(),
        [("scans", "ja")],
        "from the fan translation"
    );
}

/// A raw pairs at the bar with two English volumes on two works, the colour
/// one higher, as Kaguya-sama's raw v02 did on the test library, and joins the
/// colour one. Split from it, the raw does not go back where its name puts
/// it: its pair with the black-and-white volume, on another work, is not one
/// the Split sets apart, and it joins that volume at the Split's own derive.
#[test]
fn a_raw_split_from_one_volume_joins_another_it_matches_at_the_bar() {
    let shelf = Shelf::new("joins-split-second-host");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour_path = "Kaguya Colour/v02/Kaguya Colour v02.cbz";
    let black_path = "Kaguya/v02/Kaguya v02.cbz";
    let raw_path = "Kaguya Zunousen/v02/Kaguya Zunousen v02.cbz";
    let colours = shelf.directory(&conn, "Colour", "Manga", VOLUMES, &[colour_path]);
    let blacks = shelf.directory(&conn, "Black", "Manga", VOLUMES, &[black_path]);
    let raws = shelf.directory(&conn, "Raw", "Manga", RAW, &[raw_path]);
    derive::derive(&conn).expect("derives");
    let colour = digest(&conn, &colours, colour_path);
    let black = digest(&conn, &blacks, black_path);
    let raw = digest(&conn, &raws, raw_path);
    for file in [&colour, &black, &raw] {
        read(&conn, file, "whole");
    }
    let to_colour = pair(&conn, &colour, &raw, 0.977);
    let to_black = pair(&conn, &black, &raw, 0.95);
    derive::derive(&conn).expect("derives the join");
    let (coloured, blacked) = (holder(&conn, &colour), holder(&conn, &black));
    assert_eq!(
        holder(&conn, &raw),
        coloured,
        "the fixture: the raw joins the colour volume, its higher pair"
    );

    let split = matches::splits_for(&conn, to_colour, &raw)
        .expect("reads what the Split sets apart")
        .expect("the pair names the raw");
    assert_eq!(split, [to_colour], "the pair to another work is not split");
    matches::set_state(&conn, &split, State::Split, None).expect("splits the raw");
    derive::derive(&conn).expect("derives after the Split");

    assert_eq!(
        (holder(&conn, &raw), holder(&conn, &colour)),
        (blacked, coloured),
        "the raw is on the black-and-white volume's work"
    );
    assert_eq!(
        editions(&conn, blacked),
        [labelled(VOLUMES, None), labelled(RAW, Some(to_black))],
        "joined by its pair with the black-and-white volume"
    );
}

/// The English digest is the larger, so the pair's runs are stored from the
/// raw's pages: the alignment from the English edition turns them round and
/// leaves its last page unpaired. The pair has no floor.
#[test]
fn runs_are_turned_round_when_the_host_is_b() {
    let shelf = Shelf::new("joins-turned");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja) = english_and_raw_of(&shelf, &conn, ("en-5", 5), ("ja-5", 6));
    let id = paged_pair(&conn, (&en, 5), (&ja, 6), &[[0, 3, 2, 5]], None);
    assert_eq!(
        named(&conn, id),
        [ja.clone(), en.clone()],
        "the fixture: the raw's digest is the pair's a"
    );
    let runs: String = conn
        .query_row("SELECT runs FROM content_match WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("the stored runs");
    assert_eq!(
        runs, "[[2,5,0,3]]",
        "the fixture: stored from the raw's pages"
    );

    derive::derive(&conn).expect("derives the join");

    let english = holder(&conn, &en);
    assert_eq!(
        alignments(&conn, english),
        [Alignment {
            from: "en".to_owned(),
            to: "ja".to_owned(),
            by: Source::Solver,
            solver: None,
            quality: Some(Quality {
                verdict: Verdict::Fragmented,
                slots: 5,
                paired: 4,
                avg_run: 4.0,
            }),
            runs: vec![paired((0, 3), (2, 5)), unpaired((4, 4))],
        }],
        "from the English pages, the run turned round"
    );
}

/// After the join an admin replaces the solver's alignment with one of their
/// own from the raw to the English edition, the pair the other way round:
/// the next derive writes no solver row beside it and leaves it as it was.
#[test]
fn a_hand_alignment_for_the_pair_keeps_the_solver_out() {
    let shelf = Shelf::new("joins-hand-alignment");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja) = english_and_raw_of(&shelf, &conn, ("en-3", 3), ("ja-3", 3));
    paged_pair(&conn, (&en, 3), (&ja, 3), &[[0, 2, 0, 2]], Some(0.75));
    derive::derive(&conn).expect("derives the join");
    let english = holder(&conn, &en);
    let mut doc = project::sidecar(&conn, english).expect("projects");
    assert_eq!(
        doc.alignments
            .iter()
            .map(|a| (a.from.as_str(), a.to.as_str(), a.by))
            .collect::<Vec<_>>(),
        [("en", "ja", Source::Solver)],
        "the fixture: the solver's alignment"
    );
    doc.alignments = vec![Alignment {
        from: "ja".to_owned(),
        to: "en".to_owned(),
        by: Source::Hand,
        solver: None,
        quality: None,
        runs: vec![
            RunEntry {
                by: Some(Source::Hand),
                ..paired((0, 1), (1, 2))
            },
            unpaired((2, 2)),
        ],
    }];
    state::put_alignments(&conn, english, &doc).expect("the admin's alignment");
    let before = alignment_rows(&conn);

    derive::derive(&conn).expect("derives again");

    assert_eq!(
        alignment_rows(&conn),
        before,
        "the admin's alignment alone, its row as it was"
    );
    assert_eq!(alignments(&conn, english), doc.alignments, "and its runs");
}

/// As above, with the admin's runs unmarked, as a document may hold them:
/// the alignment's own `by = hand` keeps the solver out, where the test above
/// cannot tell it from its runs' marks.
#[test]
fn a_hand_alignment_with_unmarked_runs_keeps_the_solver_out() {
    let shelf = Shelf::new("joins-hand-alignment-unmarked");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja) = english_and_raw_of(&shelf, &conn, ("en-3", 3), ("ja-3", 3));
    paged_pair(&conn, (&en, 3), (&ja, 3), &[[0, 2, 0, 2]], Some(0.75));
    derive::derive(&conn).expect("derives the join");
    let english = holder(&conn, &en);
    let mut doc = project::sidecar(&conn, english).expect("projects");
    doc.alignments = vec![Alignment {
        from: "ja".to_owned(),
        to: "en".to_owned(),
        by: Source::Hand,
        solver: None,
        quality: None,
        runs: vec![paired((0, 1), (1, 2)), unpaired((2, 2))],
    }];
    state::put_alignments(&conn, english, &doc).expect("the admin's alignment");
    assert!(
        alignments(&conn, english)[0]
            .runs
            .iter()
            .all(|run| run.by.is_none()),
        "the fixture: no run carries a mark of its own"
    );
    let before = alignment_rows(&conn);

    derive::derive(&conn).expect("derives again");

    assert_eq!(
        alignment_rows(&conn),
        before,
        "the admin's alignment alone, its row as it was"
    );
}

/// After the join `align --write` stores a solver's alignment of its own for
/// the pair, unmarked, which derive cannot tell from one it wrote: the next
/// derive replaces it with the pair's, where a run's missing mark would keep it.
#[test]
fn a_solver_alignment_stored_for_the_pair_is_replaced_by_the_pairs() {
    let shelf = Shelf::new("joins-solver-replaced");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja) = english_and_raw_of(&shelf, &conn, ("en-3", 3), ("ja-3", 3));
    paged_pair(&conn, (&en, 3), (&ja, 3), &[[0, 2, 0, 2]], Some(0.75));
    derive::derive(&conn).expect("derives the join");
    let english = holder(&conn, &en);
    let derived = alignments(&conn, english);
    let mut doc = project::sidecar(&conn, english).expect("projects");
    doc.alignments = vec![Alignment {
        from: "ja".to_owned(),
        to: "en".to_owned(),
        by: Source::Solver,
        solver: None,
        quality: None,
        runs: vec![paired((0, 1), (1, 2)), unpaired((2, 2))],
    }];
    state::put_alignments(&conn, english, &doc).expect("align's alignment");
    assert_eq!(
        alignments(&conn, english),
        doc.alignments,
        "the fixture: align's alignment in place of the derive's"
    );

    derive::derive(&conn).expect("derives again");

    assert_eq!(
        alignments(&conn, english),
        derived,
        "the pair's alignment again"
    );
}

/// After the join a person corrects one run of the solver's alignment, which
/// stays `by = solver`: the next derive neither replaces nor deletes it,
/// though the pair's runs say otherwise.
#[test]
fn a_solver_alignment_holding_a_hand_run_is_kept() {
    let shelf = Shelf::new("joins-hand-run");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja) = english_and_raw_of(&shelf, &conn, ("en-3", 3), ("ja-3", 3));
    paged_pair(&conn, (&en, 3), (&ja, 3), &[[0, 2, 0, 2]], Some(0.75));
    derive::derive(&conn).expect("derives the join");
    let english = holder(&conn, &en);
    let mut doc = project::sidecar(&conn, english).expect("projects");
    assert_eq!(
        doc.alignments.len(),
        1,
        "the fixture: the solver's alignment"
    );
    doc.alignments[0].runs = vec![
        RunEntry {
            by: Some(Source::Hand),
            ..paired((0, 1), (1, 2))
        },
        unpaired((2, 2)),
    ];
    state::put_alignments(&conn, english, &doc).expect("the correction");
    let before = alignment_rows(&conn);
    assert_eq!(
        before.iter().map(|row| row.4.as_str()).collect::<Vec<_>>(),
        ["solver"],
        "the fixture: the corrected alignment is still the solver's"
    );

    derive::derive(&conn).expect("derives again");

    assert_eq!(alignment_rows(&conn), before, "its row as it was");
    assert_eq!(
        alignments(&conn, english),
        doc.alignments,
        "and its runs, the corrected one included"
    );
}

/// Two volumes joined, so an alignment written again would not take its old
/// id back: the derive after the joins rewrites no alignment and no run, and
/// commits nothing.
#[test]
fn a_second_derive_rewrites_no_alignment() {
    let shelf = Shelf::new("joins-alignment-quiet");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let english_files = ["Oshi/v01/Oshi v01.cbz", "Oshi/v02/Oshi v02.cbz"];
    let raw_files = [
        "Oshinoko/v01/Oshinoko v01.cbz",
        "Oshinoko/v02/Oshinoko v02.cbz",
    ];
    let volumes = shelf.directory(&conn, "Volumes", "Manga", VOLUMES, &english_files);
    let raw = shelf.directory(&conn, "Raw", "Manga", RAW, &raw_files);
    derive::derive(&conn).expect("derives");
    for (english_file, raw_file) in english_files.iter().zip(&raw_files) {
        let en = digest(&conn, &volumes, english_file);
        let ja = digest(&conn, &raw, raw_file);
        read(&conn, &en, "whole");
        read(&conn, &ja, "whole");
        paged_pair(&conn, (&en, 3), (&ja, 3), &[[0, 1, 1, 2]], Some(0.75));
    }
    derive::derive(&conn).expect("derives the joins");
    let (rows, runs) = (alignment_rows(&conn), run_rows(&conn));
    assert_eq!(
        rows.len(),
        2,
        "the fixture: an alignment per volume: {rows:?}"
    );
    let changes = conn.total_changes();

    let (commits, report) = commits_during(&mut conn, derive::derive);
    report.expect("derives again");

    assert_eq!(alignment_rows(&conn), rows, "each alignment row as it was");
    assert_eq!(run_rows(&conn), runs, "each run row as it was");
    assert_eq!(commits, 0, "no write transaction committed");
    assert_eq!(conn.total_changes(), changes, "no row changed");
}

/// Names put the English volume and its raw in one work, the raw's key `ja`
/// sorting before the English `volumes`, so the work's hub is the raw: the
/// pair joins nothing and marks nothing, and its alignment goes from the hub
/// though the English edition is the official one.
#[test]
fn names_already_together_get_their_alignment() {
    let shelf = Shelf::new("joins-together-aligned");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let official = ("volumes", "official", "en");
    let volumes = shelf.holding(
        &conn,
        "Volumes",
        official,
        "Oshi/v01/Oshi v01.cbz",
        ("en-4", 4),
    );
    let raw = shelf.holding(&conn, "Raw", RAW, "Oshi/v01/Oshi v01.cbz", ("ja-4", 3));
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let ja = digest(&conn, &raw, "Oshi/v01/Oshi v01.cbz");
    read(&conn, &en, "whole");
    read(&conn, &ja, "whole");
    paged_pair(&conn, (&en, 4), (&ja, 3), &[[1, 3, 0, 2]], Some(0.5));
    let oshi = holder(&conn, &en);
    assert_eq!(
        (holder(&conn, &ja), work(&conn, oshi).2),
        (oshi, "ja".to_owned()),
        "the fixture: names put both in one work, whose hub is the raw"
    );

    derive::derive(&conn).expect("derives");

    assert_eq!(
        editions(&conn, oshi),
        [labelled(RAW, None), labelled(official, None)],
        "no edition is marked"
    );
    assert_eq!(
        alignments(&conn, oshi),
        [Alignment {
            from: "ja".to_owned(),
            to: "volumes".to_owned(),
            by: Source::Solver,
            solver: Some(SolverInfo {
                hash: "dhash-16".to_owned(),
                floor: 0.5,
            }),
            quality: Some(Quality {
                verdict: Verdict::Sound,
                slots: 3,
                paired: 3,
                avg_run: 3.0,
            }),
            runs: vec![paired((0, 2), (1, 3))],
        }],
        "from the hub, every one of its pages paired"
    );
}

/// The black-and-white and colour volumes, both `en`, pair beside the raw's
/// pair with the black-and-white volume, which shows alignments are written
/// at all: the raw's pair is aligned, the same-key pair nowhere.
#[test]
fn a_same_key_pair_gets_none() {
    let shelf = Shelf::new("joins-same-key-aligned");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = shelf.directory(
        &conn,
        "Volumes",
        "Manga",
        VOLUMES,
        &[
            "Oshi/v01/Oshi v01.cbz",
            "Oshi Colour/v01/Oshi Colour v01.cbz",
        ],
    );
    let raw = shelf.directory(
        &conn,
        "Raw",
        "Manga",
        RAW,
        &["Oshinoko/v01/Oshinoko v01.cbz"],
    );
    derive::derive(&conn).expect("derives");
    let black = digest(&conn, &volumes, "Oshi/v01/Oshi v01.cbz");
    let colour = digest(&conn, &volumes, "Oshi Colour/v01/Oshi Colour v01.cbz");
    let ja = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    for file in [&black, &colour, &ja] {
        read(&conn, file, "whole");
    }
    paged_pair(&conn, (&black, 3), (&colour, 3), &[[0, 2, 0, 2]], None);
    paged_pair(&conn, (&black, 3), (&ja, 3), &[[0, 2, 0, 2]], None);

    derive::derive(&conn).expect("derives");

    let (oshi, coloured) = (holder(&conn, &black), holder(&conn, &colour));
    assert_ne!(oshi, coloured, "the colour volume keeps a work of its own");
    assert_eq!(
        alignments(&conn, oshi)
            .iter()
            .map(|a| (a.from.as_str(), a.to.as_str()))
            .collect::<Vec<_>>(),
        [("en", "ja")],
        "the raw's pair is aligned on the black-and-white work"
    );
    assert_eq!(
        alignments(&conn, coloured),
        [],
        "and the colour work holds none"
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM alignment"),
        1,
        "nor does any other"
    );
}

/// Official, fan and raw volumes of six, seven and five pages, each pair at
/// the bar with runs reaching the last page of a side, the fan keyed
/// `scans`, which sorts after the raw's `ja`: one work, two alignments from
/// its official hub and the third from the fan, whose kind outranks the
/// raw's though its key sorts after; each reads back through `runs_of`.
#[test]
fn every_stored_alignment_reads_back_through_runs_of() {
    let shelf = Shelf::new("joins-runs-of");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let official = shelf.holding(
        &conn,
        "Volumes",
        VOLUMES,
        "Oshi/v01/Oshi v01.cbz",
        ("official", 6),
    );
    let fan = shelf.holding(
        &conn,
        "Fan",
        ("scans", "fan", "en"),
        "Oshi Scans/v01/Oshi Scans v01.cbz",
        ("fan", 7),
    );
    let raw = shelf.holding(
        &conn,
        "Raw",
        RAW,
        "Oshinoko/v01/Oshinoko v01.cbz",
        ("raw", 5),
    );
    derive::derive(&conn).expect("derives");
    let official = digest(&conn, &official, "Oshi/v01/Oshi v01.cbz");
    let fan = digest(&conn, &fan, "Oshi Scans/v01/Oshi Scans v01.cbz");
    let raw = digest(&conn, &raw, "Oshinoko/v01/Oshinoko v01.cbz");
    for file in [&official, &fan, &raw] {
        read(&conn, file, "whole");
    }
    paged_pair(&conn, (&official, 6), (&raw, 5), &[[0, 4, 0, 4]], Some(0.7));
    paged_pair(&conn, (&official, 6), (&fan, 7), &[[0, 5, 1, 6]], Some(0.7));
    paged_pair(&conn, (&fan, 7), (&raw, 5), &[[2, 6, 0, 4]], None);

    derive::derive(&conn).expect("derives the joins");

    let oshi = holder(&conn, &official);
    assert_eq!(
        (holder(&conn, &fan), holder(&conn, &raw)),
        (oshi, oshi),
        "one work holds all three"
    );
    let doc = project::sidecar(&conn, oshi).expect("projects");
    let mut directions: Vec<(&str, &str)> = doc
        .alignments
        .iter()
        .map(|a| (a.from.as_str(), a.to.as_str()))
        .collect();
    directions.sort_unstable();
    assert_eq!(
        directions,
        [("en", "ja"), ("en", "scans"), ("scans", "ja")],
        "two from the hub, the third from the fan"
    );
    for alignment in &doc.alignments {
        assert!(
            doc.runs_of(alignment).is_ok(),
            "{} to {} reads back: {:?}",
            alignment.from,
            alignment.to,
            doc.runs_of(alignment)
        );
    }
}

/// Before the join each volume is a work of its own and no series counts one
/// aligned; after it, the English series counts its one volume.
#[test]
fn the_series_card_counts_a_joined_volume_as_aligned() {
    let shelf = Shelf::new("joins-card");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja) = english_and_raw_of(&shelf, &conn, ("en-3", 3), ("ja-3", 3));
    paged_pair(&conn, (&en, 3), (&ja, 3), &[[0, 2, 0, 2]], Some(0.75));
    let aligned = |conn: &Connection| -> Vec<(String, i64)> {
        browse::series(conn, &Access::Every, 1, None)
            .expect("lists the series")
            .into_iter()
            .map(|card| (card.title, card.aligned))
            .collect()
    };
    assert_eq!(
        aligned(&conn),
        [("Oshi".to_owned(), 0), ("Oshinoko".to_owned(), 0)],
        "the fixture: two series, neither counting an aligned volume"
    );

    derive::derive(&conn).expect("derives the join");

    assert_eq!(
        aligned(&conn),
        [("Oshi".to_owned(), 1)],
        "the English series counts its joined volume"
    );
}

/// Three volumes joined, each pair given page counts of its own: volume 1's
/// pair numbers its a file's pages otherwise than that file's edition does,
/// volume 2's its b file's, and volume 3's agree, which shows alignments are
/// written at all. The first two are aligned nowhere and counted, on this
/// derive and on the next.
#[test]
fn a_pair_whose_page_count_is_not_its_editions_is_counted_and_not_aligned() {
    let shelf = Shelf::new("joins-page-counts");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let english_files = [
        "Oshi/v01/Oshi v01.cbz",
        "Oshi/v02/Oshi v02.cbz",
        "Oshi/v03/Oshi v03.cbz",
    ];
    let raw_files = [
        "Oshinoko/v01/Oshinoko v01.cbz",
        "Oshinoko/v02/Oshinoko v02.cbz",
        "Oshinoko/v03/Oshinoko v03.cbz",
    ];
    let volumes = shelf.directory(&conn, "Volumes", "Manga", VOLUMES, &english_files);
    let raw = shelf.directory(&conn, "Raw", "Manga", RAW, &raw_files);
    derive::derive(&conn).expect("derives");
    let mut english = Vec::new();
    for ((english_file, raw_file), (a_pages, b_pages)) in english_files
        .iter()
        .zip(&raw_files)
        .zip([(4, 3), (3, 5), (3, 3)])
    {
        let en = digest(&conn, &volumes, english_file);
        let ja = digest(&conn, &raw, raw_file);
        read(&conn, &en, "whole");
        read(&conn, &ja, "whole");
        let (a, b) = if en < ja { (&en, &ja) } else { (&ja, &en) };
        paged_pair(&conn, (a, a_pages), (b, b_pages), &[[0, 2, 0, 2]], None);
        english.push(en);
    }

    let report = derive::derive(&conn).expect("derives the joins");

    assert_eq!(report.joined, 3, "the fixture: every raw joins: {report:?}");
    let works: Vec<i64> = english.iter().map(|en| holder(&conn, en)).collect();
    assert_eq!(
        alignments(&conn, works[0]),
        [],
        "volume 1, its a file's count not its edition's: no alignment"
    );
    assert_eq!(
        alignments(&conn, works[1]),
        [],
        "volume 2, its b file's count not its edition's: no alignment"
    );
    assert_eq!(
        alignments(&conn, works[2])
            .iter()
            .map(|a| (a.from.as_str(), a.to.as_str()))
            .collect::<Vec<_>>(),
        [("en", "ja")],
        "volume 3, its counts its editions': aligned"
    );
    assert_eq!(
        report.unwritten_alignments, 2,
        "the two left unwritten are counted"
    );
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(
        again.unwritten_alignments, 2,
        "and counted again on the next derive"
    );
}

/// Raw 1 joins English 1, so the raws' directory leads to Oshi, and raw 2 and
/// scanlation 2, which names put in one work, pair: a joined pair's own
/// groups never follow, so this is the pair a following group holds. That
/// work follows into Oshi at volume 2, and the pair's alignment is stored on
/// it, found by the key the group has after following.
#[test]
fn a_pair_in_a_group_that_follows_is_aligned_on_the_work_it_followed_into() {
    let shelf = Shelf::new("joins-follow-alignment");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let scans = shelf.directory(
        &conn,
        "Scans",
        "Manga",
        SCANS,
        &[&oshinoko(1), &oshinoko(2)],
    );
    let raws: Vec<String> = (1..=2).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[oshi(1)], &raws);
    let scan = digest(&conn, &scans, &oshinoko(2));
    read(&conn, &scan, "whole");
    let second = shelved.ja(&conn, 2);
    let follower = holder(&conn, &second);
    assert_eq!(
        (holder(&conn, &scan), work(&conn, follower)),
        (follower, raw_at("Oshinoko", 2)),
        "the fixture: raw 2 and scanlation 2 on one work, in the raws' series"
    );
    pair(&conn, &shelved.en(&conn, 1), &shelved.ja(&conn, 1), 0.95);
    paged_pair(&conn, (&second, 3), (&scan, 3), &[[0, 2, 0, 2]], Some(0.8));

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            holder(&conn, &second),
            holder(&conn, &scan),
            work(&conn, follower),
            report.followed
        ),
        (follower, follower, raw_at("Oshi", 2), 1),
        "the work followed into Oshi at volume 2"
    );
    assert_eq!(
        alignments(&conn, follower),
        [Alignment {
            from: "ja".to_owned(),
            to: "scans".to_owned(),
            by: Source::Solver,
            solver: Some(SolverInfo {
                hash: "dhash-16".to_owned(),
                floor: 0.8,
            }),
            quality: Some(Quality {
                verdict: Verdict::Sound,
                slots: 3,
                paired: 3,
                avg_run: 3.0,
            }),
            runs: vec![paired((0, 2), (0, 2))],
        }],
        "its pair's alignment, from its hub, on the work it followed into"
    );
}

/// Reader 1, whose places and notes the tests below follow, where the store
/// does not already hold one.
fn reader(conn: &Connection) {
    conn.execute(
        "INSERT OR IGNORE INTO user (id, name, password_hash, role)
         VALUES (1, 'reader', '!', 'user')",
        [],
    )
    .expect("the reader");
}

/// Reader 1 at `slot` of edition `key` of `work_id`, saved at a fixed time.
fn place(conn: &Connection, work_id: i64, key: &str, slot: u32) {
    state::put_progress_at(
        conn,
        work_id,
        1,
        &crate::Progress {
            edition: key.to_owned(),
            slot,
        },
        Some("2026-11-01 09:00:00"),
        state::Numbering::Store,
    )
    .expect("a reading position");
}

/// A `progress` row, every column but the reader's, so a move that rewrote
/// any of them shows.
#[derive(Debug, Clone, PartialEq)]
struct Place {
    work_id: i64,
    edition: String,
    slot: i64,
    updated: String,
    completed_at: Option<String>,
    read_count: i64,
    at_sha256: Option<String>,
    at_nth: Option<i64>,
    at_page: Option<i64>,
    at_half: Option<String>,
}

/// Every reading position in the store.
fn places(conn: &Connection) -> Vec<Place> {
    conn.prepare(
        "SELECT work_id, edition, slot, updated, completed_at, read_count,
                at_sha256, at_nth, at_page, at_half
           FROM progress ORDER BY work_id, edition",
    )
    .and_then(|mut stmt| {
        stmt.query_map([], |r| {
            Ok(Place {
                work_id: r.get(0)?,
                edition: r.get(1)?,
                slot: r.get(2)?,
                updated: r.get(3)?,
                completed_at: r.get(4)?,
                read_count: r.get(5)?,
                at_sha256: r.get(6)?,
                at_nth: r.get(7)?,
                at_page: r.get(8)?,
                at_half: r.get(9)?,
            })
        })?
        .collect()
    })
    .expect("the places")
}

/// Reader 1's note `id` on slot 1 of edition `key` of `work_id`.
fn note(conn: &Connection, work_id: i64, key: &str, id: &str, body: &str) {
    state::put_notes(
        conn,
        work_id,
        1,
        &crate::Notes {
            taiyaku_notes: crate::NOTES_VERSION,
            work: "ignored".to_owned(),
            notes: vec![crate::Note {
                text: None,
                id: id.to_owned(),
                edition: key.to_owned(),
                slot: 1,
                region: None,
                color: None,
                body: body.to_owned(),
                created: "2026-11-01T09:00:00Z".to_owned(),
                updated: "2026-11-01T09:00:00Z".to_owned(),
                stale: false,
                at: None,
            }],
        },
    )
    .expect("a note");
}

/// A `note` row, every column a move could rewrite or leave behind.
#[derive(Debug, Clone, PartialEq)]
struct Noted {
    work_id: i64,
    edition: String,
    id: String,
    slot: i64,
    body: String,
    created: String,
    updated: String,
}

/// Every note in the store.
fn noted(conn: &Connection) -> Vec<Noted> {
    conn.prepare(
        "SELECT work_id, edition, id, slot, body, created, updated FROM note
          ORDER BY work_id, edition, id",
    )
    .and_then(|mut stmt| {
        stmt.query_map([], |r| {
            Ok(Noted {
                work_id: r.get(0)?,
                edition: r.get(1)?,
                id: r.get(2)?,
                slot: r.get(3)?,
                body: r.get(4)?,
                created: r.get(5)?,
                updated: r.get(6)?,
            })
        })?
        .collect()
    })
    .expect("the notes")
}

const RAW_NOTE: &str = "01JRAWNOTE0000000000000000";

/// The directory holding `digest`.
fn root_of(conn: &Connection, digest: &str) -> Root {
    let id: i64 = conn
        .query_row(
            "SELECT root_id FROM scan_entry WHERE sha256 = ?1",
            [digest],
            |r| r.get(0),
        )
        .unwrap_or_else(|e| panic!("the directory holding {digest}: {e}"));
    scan::roots(conn)
        .expect("lists the directories")
        .into_iter()
        .find(|root| root.id == id)
        .expect("the directory")
}

/// The one work holding `digest` besides `host`.
fn other_than(conn: &Connection, digest: &str, host: i64) -> i64 {
    let works = holders(conn, digest);
    assert!(works.contains(&host), "{host} holds {digest}: {works:?}");
    let others: Vec<i64> = works.into_iter().filter(|&id| id != host).collect();
    assert_eq!(others.len(), 1, "one other work holds {digest}: {others:?}");
    others[0]
}

/// The raw's whole work joins, so `fold::absorb` carries its reader's place
/// and note onto the English work; the pair is split, so the raw comes back
/// on a work of its own while the English work keeps others, and
/// `fold::move_edition` carries them back.
#[test]
fn a_position_and_a_note_follow_a_join_and_come_back_on_a_split() {
    let shelf = Shelf::new("joins-reader-state");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, id) = english_and_raw(&shelf, &conn);
    let (english, raws) = (holder(&conn, &en), holder(&conn, &ja));
    reader(&conn);
    place(&conn, raws, "ja", 2);
    note(&conn, raws, "ja", RAW_NOTE, "the raw's note");
    let (placed, written) = (places(&conn), noted(&conn));
    assert_eq!(
        (placed.len(), written.len(), placed[0].at_page),
        (1, 1, Some(2)),
        "the fixture: a place at page 2 of the raw, and a note"
    );

    let joining = derive::derive(&conn).expect("derives the join");

    assert_eq!(holder(&conn, &ja), english, "the raw joined");
    assert_eq!(
        places(&conn),
        [Place {
            work_id: english,
            ..placed[0].clone()
        }],
        "the place is on the English work, every other column as it was"
    );
    assert_eq!(
        noted(&conn),
        [Noted {
            work_id: english,
            ..written[0].clone()
        }],
        "and the note"
    );
    assert_eq!(
        (joining.emptied_works, joining.moved_editions),
        (1, 0),
        "carried by the fold of the raw's work: {joining:?}"
    );

    matches::set_state(&conn, &[id], State::Split, None).expect("splits the pair");
    let splitting = derive::derive(&conn).expect("derives the split");

    let back = holder(&conn, &ja);
    assert_ne!(back, english, "the raw is on a work of its own");
    assert_eq!(
        editions(&conn, english),
        [labelled(VOLUMES, None)],
        "the English work keeps no copy of the raw"
    );
    assert_eq!(
        places(&conn),
        [Place {
            work_id: back,
            ..placed[0].clone()
        }],
        "the place came back with it"
    );
    assert_eq!(
        noted(&conn),
        [Noted {
            work_id: back,
            ..written[0].clone()
        }],
        "and the note"
    );
    assert_eq!(
        (
            splitting.moved_editions,
            splitting.moved_positions,
            splitting.moved_notes
        ),
        (1, 1, 1),
        "{splitting:?}"
    );
}

/// Review Focus 2: the joined raw is downloaded again, so its digest changes
/// and it has no fingerprint row. The next derive puts it where its name
/// puts it with its reader's place, and leaves no copy of the old bytes on
/// the English work.
#[test]
fn a_file_whose_bytes_changed_after_joining_goes_back_with_its_position() {
    let shelf = Shelf::new("joins-redownload");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, _) = english_and_raw(&shelf, &conn);
    let (english, raws) = (holder(&conn, &en), holder(&conn, &ja));
    reader(&conn);
    place(&conn, raws, "ja", 2);
    let placed = places(&conn);
    derive::derive(&conn).expect("derives the join");
    assert_eq!(holder(&conn, &ja), english, "the fixture: the raw joined");
    let file = "Oshinoko/v01/Oshinoko v01.cbz";
    let raw = root_of(&conn, &ja);
    write_archive(&shelf.path("Raw", file), 4);
    scan::scan_root(&conn, &raw).expect("scans the new bytes");

    let report = derive::derive(&conn).expect("derives");

    let fresh = digest(&conn, &raw, file);
    assert_ne!(fresh, ja, "the fixture: the bytes changed");

    let back = holder(&conn, &fresh);
    assert_eq!(
        (work(&conn, back), editions(&conn, back)),
        (
            ("Oshinoko".to_owned(), Some(1.0), "ja".to_owned()),
            vec![labelled(RAW, None)]
        ),
        "the new bytes are where the raw's name puts them, joined by nothing"
    );
    assert_eq!(
        (holders(&conn, &ja), editions(&conn, english)),
        (vec![], vec![labelled(VOLUMES, None)]),
        "no copy of the old bytes stands on the English work"
    );
    assert_eq!(
        places(&conn),
        [Place {
            work_id: back,
            ..placed[0].clone()
        }],
        "the place came back as it was saved"
    );
    let read_back: Vec<(String, u32)> = state::progress(&conn, back, 1)
        .expect("reads the place")
        .into_iter()
        .map(|position| (position.edition, position.slot))
        .collect();
    assert_eq!(
        read_back,
        [("ja".to_owned(), 2)],
        "and opens page 2 of the raw's work"
    );
    assert_eq!(report.moved_editions, 1, "{report:?}");
}

/// Review Focus 3: the raw shares its work with a scanlation under `en`,
/// which the English work holds, so only the raw joins. Its place and note
/// go with it, and its old work keeps the scanlation and no copy of the raw.
#[test]
fn a_partial_join_leaves_no_copy_on_the_old_work() {
    let shelf = Shelf::new("joins-partial-no-copy");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let Partial {
        ja, english, old, ..
    } = partial(&shelf, &conn, VOLUMES, EN_SCANS);
    reader(&conn);
    place(&conn, old, "ja", 2);
    note(&conn, old, "ja", RAW_NOTE, "the raw's note");
    let (placed, written) = (places(&conn), noted(&conn));
    assert_eq!(
        (placed.len(), written.len()),
        (1, 1),
        "the fixture: a place and a note on the raw"
    );

    let report = derive::derive(&conn).expect("derives the join");

    assert_eq!(
        holders(&conn, &ja),
        [english],
        "the raw is on the English work alone"
    );
    assert_eq!(
        editions(&conn, old),
        [labelled(EN_SCANS, None)],
        "the old work keeps the scanlation and no copy of the raw"
    );
    assert_eq!(
        places(&conn),
        [Place {
            work_id: english,
            ..placed[0].clone()
        }],
        "the place is on the English work, every other column as it was"
    );
    assert_eq!(
        noted(&conn),
        [Noted {
            work_id: english,
            ..written[0].clone()
        }],
        "and the note"
    );
    assert_eq!(
        (report.moved_editions, report.kept_editions),
        (1, 0),
        "one edition moved and none left standing: {report:?}"
    );
}

/// Korean volumes, in Korean and Chinese.
const KOREAN: (&str, &str, &str) = ("ko", "official", "ko");
const CHINESE: (&str, &str, &str) = ("zh", "official", "zh");
/// Korean and Chinese scanlations.
const KOREAN_SCANS: (&str, &str, &str) = ("ko", "fan", "ko");
const CHINESE_SCANS: (&str, &str, &str) = ("zh", "fan", "zh");

/// Oshi volume 1 in Korean and in Chinese; the Oshinoko raw beside a Korean
/// and a Chinese scanlation of it, which names put on one work; the Korean
/// volume and the raw read whole and paired at 0.95. The Oshi work holds both
/// scanlations' keys, so the raw alone can join. With `zh_first` the Chinese
/// scanlation is derived before the raw and the Korean one, so the Oshinoko
/// work's hub is `zh`; otherwise it is `ja`, which sorts first. Answers the
/// Oshinoko work.
fn two_kept(shelf: &Shelf, conn: &Connection, zh_first: bool) -> i64 {
    let (oshi, oshinoko) = ("Oshi/v01/Oshi v01.cbz", "Oshinoko/v01/Oshinoko v01.cbz");
    let korean = shelf.directory(conn, "Korean", "Manga", KOREAN, &[oshi]);
    shelf.directory(conn, "Chinese", "Manga", CHINESE, &[oshi]);
    shelf.directory(conn, "ScansZh", "Manga", CHINESE_SCANS, &[oshinoko]);
    if zh_first {
        derive::derive(conn).expect("derives the Chinese scanlation first");
    }
    let raw = shelf.directory(conn, "Raw", "Manga", RAW, &[oshinoko]);
    shelf.directory(conn, "ScansKo", "Manga", KOREAN_SCANS, &[oshinoko]);
    derive::derive(conn).expect("derives");
    let (ko, ja) = (digest(conn, &korean, oshi), digest(conn, &raw, oshinoko));
    read(conn, &ko, "whole");
    read(conn, &ja, "whole");
    pair(conn, &ko, &ja, 0.95);
    let old = holder(conn, &ja);
    assert_eq!(
        editions(conn, old),
        [
            labelled(RAW, None),
            labelled(KOREAN_SCANS, None),
            labelled(CHINESE_SCANS, None)
        ],
        "the fixture: the raw and both scanlations on one work"
    );
    old
}

/// The raw's `ja` is its old work's hub when it joins, and the work keeps
/// `ko` and `zh`. It takes `ko`, the first, which its key was hashed over,
/// so the derive after commits nothing.
#[test]
fn a_work_whose_hub_joined_takes_the_first_key_it_keeps_as_hub() {
    let shelf = Shelf::new("joins-hub-moves");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let old = two_kept(&shelf, &conn, false);
    assert_eq!(work(&conn, old).2, "ja", "the fixture: the raw is the hub");

    derive::derive(&conn).expect("derives the join");

    assert_eq!(
        (work(&conn, old).2, editions(&conn, old)),
        (
            "ko".to_owned(),
            vec![labelled(KOREAN_SCANS, None), labelled(CHINESE_SCANS, None)]
        ),
        "the old work's hub is the first edition it keeps"
    );
    let (commits, report) = commits_during(&mut conn, derive::derive);
    report.expect("derives again");
    assert_eq!(commits, 0, "and the derive after commits nothing");
}

/// The Chinese scanlation came first, so the old work's hub is `zh` and not
/// its first key; the raw joins and the hub stays.
#[test]
fn a_work_whose_hub_stays_keeps_it_when_another_edition_joins() {
    let shelf = Shelf::new("joins-hub-stays");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let old = two_kept(&shelf, &conn, true);
    assert_eq!(work(&conn, old).2, "zh", "the fixture: zh is the hub");

    derive::derive(&conn).expect("derives the join");

    assert_eq!(
        (work(&conn, old).2, editions(&conn, old)),
        (
            "zh".to_owned(),
            vec![labelled(KOREAN_SCANS, None), labelled(CHINESE_SCANS, None)]
        ),
        "the old work keeps its hub"
    );
    let (commits, report) = commits_during(&mut conn, derive::derive);
    report.expect("derives again");
    assert_eq!(commits, 0, "and the derive after commits nothing");
}

/// The raw's old work holds a tag, a hand value that holds nothing where it
/// is, and keeps its document under the raws' directory, which its hub `ja`
/// names. When the raw joins, the hub becomes the scanlation's `ko`, under
/// another directory, and the document goes with it: nothing is left to be
/// reported as owed to no work, on that derive or the next.
#[test]
fn a_hand_document_follows_its_work_when_its_hub_joins_away() {
    let shelf = Shelf::new("joins-hub-document");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let Partial {
        ja, scanned, old, ..
    } = partial(&shelf, &conn, KOREAN, KOREAN_SCANS);
    conn.execute("UPDATE root SET keeps_documents = 1", [])
        .expect("every directory keeps documents");
    let mut doc = crate::librarydoc::of(&conn, old).expect("projects the work");
    doc.tags = vec!["kept".to_owned()];
    crate::librarydoc::apply(&conn, old, &doc).expect("tags the work");
    crate::hand::save(&conn, old).expect("saves its document");
    let address = |conn: &Connection| {
        crate::hand::path_for(conn, old)
            .expect("addresses the work")
            .expect("a work under a directory has an address")
    };
    let (raws, scans) = (root_of(&conn, &ja).path, root_of(&conn, &scanned).path);
    let was = address(&conn);
    assert!(
        was.starts_with(&raws) && was.is_file(),
        "the fixture: the document is under the raws: {}",
        was.display()
    );

    let report = derive::derive(&conn).expect("derives the join");

    let now = address(&conn);
    assert!(
        now.starts_with(&scans) && now.is_file(),
        "the document is at the work's address, under the scanlation: {}",
        now.display()
    );
    let name = now.file_name().expect("a file name");
    assert!(
        !was.exists() && !raws.join(".taiyaku").join(name).exists(),
        "and no copy is left under the raws"
    );
    assert_eq!(
        (report.orphan_documents.len(), report.repaired_documents),
        (0, 0),
        "{report:?}"
    );
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(again.orphan_documents.len(), 0, "{again:?}");
    assert_eq!(
        crate::librarydoc::of(&conn, old)
            .expect("projects the work")
            .tags,
        ["kept"],
        "the tag stands"
    );
}

/// A stamp alias on the raw's edition (O240): its whole work joins, and
/// `fold::absorb` carries the alias onto the English work under its key.
#[test]
fn a_stamp_alias_survives_a_whole_work_join() {
    let shelf = Shelf::new("joins-stamp-alias");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, _) = english_and_raw(&shelf, &conn);
    let (english, raws) = (holder(&conn, &en), holder(&conn, &ja));
    conn.execute(
        "INSERT INTO stamp_alias (work_id, edition, was_sha256, was_nth, now_sha256, now_nth)
         VALUES (?1, 'ja', 'stamped', 0, ?2, 0)",
        rusqlite::params![raws, ja],
    )
    .expect("an alias on the raw");

    derive::derive(&conn).expect("derives the join");

    let aliases: Vec<(i64, String, String, String)> = conn
        .prepare("SELECT work_id, edition, was_sha256, now_sha256 FROM stamp_alias")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect()
        })
        .expect("the aliases");
    assert_eq!(
        aliases,
        [(english, "ja".to_owned(), "stamped".to_owned(), ja)],
        "the alias is on the English work, under the raw's key"
    );
}

/// Each stored pair's id, V and time scored.
fn scores(conn: &Connection) -> Vec<(i64, f64, String)> {
    conn.prepare("SELECT id, v, scored_at FROM content_match ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("the pairs")
}

/// Review Focus 5: switching Manga's matching off unjoins the raw at the next
/// derive, its edition going back to a work of its own; switching it on joins
/// it again. The pair is scored in the past, so a derive that scored again
/// would move its time.
#[test]
fn turning_matching_off_unjoins_and_back_on_rejoins_without_scoring() {
    let shelf = Shelf::new("joins-matching-off");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, id) = english_and_raw(&shelf, &conn);
    let english = holder(&conn, &en);
    conn.execute(
        "UPDATE content_match SET scored_at = '2026-01-01 00:00:00'",
        [],
    )
    .expect("scored in the past");
    let scored = scores(&conn);
    assert_eq!(
        scored,
        [(id, 0.95, "2026-01-01 00:00:00".to_owned())],
        "the fixture: one pair"
    );
    derive::derive(&conn).expect("derives the join");
    assert_eq!(holder(&conn, &ja), english, "the fixture: the raw joined");
    let manga = library_of_root(&conn, root_of(&conn, &en).id);

    libraries::set_match_by_content(&conn, manga, false).expect("switches Manga off");
    let off = derive::derive(&conn).expect("derives with matching off");

    let apart = holder(&conn, &ja);
    assert_ne!(apart, english, "the raw is on a work of its own");
    assert_eq!(
        (editions(&conn, english), editions(&conn, apart)),
        (vec![labelled(VOLUMES, None)], vec![labelled(RAW, None)]),
        "its edition left the English work"
    );
    assert_eq!(
        (off.moved_editions, scores(&conn)),
        (1, scored.clone()),
        "moved, and the pair stands as scored"
    );

    libraries::set_match_by_content(&conn, manga, true).expect("switches Manga on");
    derive::derive(&conn).expect("derives with matching on");

    assert_eq!(
        (holders(&conn, &ja), editions(&conn, english)),
        (
            vec![english],
            vec![labelled(VOLUMES, None), labelled(RAW, Some(id))]
        ),
        "the raw joined again"
    );
    assert_eq!(scores(&conn), scored, "and the pair stands as scored");
}

/// The raw's edition on the English work holds a spread flag when its pair is
/// split, so `fold::move_edition` refuses: both editions stand, the place
/// stays on the English work, and every derive counts it and says why.
#[test]
fn a_refused_move_leaves_both_editions_and_is_counted() {
    let shelf = Shelf::new("joins-refused-move");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, id) = english_and_raw(&shelf, &conn);
    let (english, raws) = (holder(&conn, &en), holder(&conn, &ja));
    reader(&conn);
    place(&conn, raws, "ja", 2);
    derive::derive(&conn).expect("derives the join");
    let placed = places(&conn);
    assert_eq!(
        placed.iter().map(|place| place.work_id).collect::<Vec<_>>(),
        [english],
        "the fixture: the place joined with the raw"
    );
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'spread', 1 FROM edition WHERE work_id = ?1 AND key = 'ja'",
        [english],
    )
    .expect("a spread on the joined raw");
    matches::set_state(&conn, &[id], State::Split, None).expect("splits the pair");

    let report = derive::derive(&conn).expect("derives the split");

    let back = other_than(&conn, &ja, english);
    assert_eq!(
        (editions(&conn, english), editions(&conn, back)),
        (
            vec![labelled(VOLUMES, None), labelled(RAW, Some(id))],
            vec![labelled(RAW, None)]
        ),
        "the raw's edition stands on both works"
    );
    assert_eq!(places(&conn), placed, "the place stays where it was");
    assert_eq!(
        (
            report.moved_editions,
            report.unmoved_editions,
            report.unfoldable
        ),
        (
            0,
            1,
            vec![format!(
                "edition `ja` of work {english} holds a spread flag, so a slot past it would \
                 name another page on work {back}; nothing was moved and nothing was deleted"
            )]
        ),
        "counted, and why"
    );
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(again.unmoved_editions, 1, "and counted again: {again:?}");
}

/// One reader holds one note id on the raw's work and on the English work, in
/// copies that differ, as one sidecar imported against both and then edited.
/// The partial join's move refuses, which is not an error: the derive goes
/// on, both editions stand, and each note stays where it was.
#[test]
fn differing_copies_of_a_note_refuse_the_move_and_the_derive_goes_on() {
    let shelf = Shelf::new("joins-note-copies");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let Partial {
        ja,
        id,
        english,
        old,
        ..
    } = partial(&shelf, &conn, VOLUMES, EN_SCANS);
    reader(&conn);
    note(&conn, old, "ja", RAW_NOTE, "on the raw");
    note(&conn, english, "en", RAW_NOTE, "on the English volume");
    let written = noted(&conn);

    let report = derive::derive(&conn).expect("a refusal is not an error");

    assert_eq!(
        other_than(&conn, &ja, english),
        old,
        "the raw stands on the English work and its old one"
    );
    assert_eq!(
        (editions(&conn, english), editions(&conn, old)),
        (
            vec![labelled(VOLUMES, None), labelled(RAW, Some(id))],
            vec![labelled(EN_SCANS, None), labelled(RAW, None)]
        ),
        "with its edition on each"
    );
    assert_eq!(noted(&conn), written, "each note as it was");
    assert_eq!(
        (report.moved_editions, report.unmoved_editions),
        (0, 1),
        "{report:?}"
    );
    assert!(
        report
            .unfoldable
            .iter()
            .any(|why| why.contains(&format!("note id(s) on both work {old} and work {english}"))),
        "says why: {:?}",
        report.unfoldable
    );
}

/// The raw's directory is keyed `jp` once its pair is split, so the raw comes
/// back under `jp` while the English work holds it under `ja`. No `ja`
/// stands on the raw's work for the place to land on, so nothing moves and
/// the English work's edition stays, counted as kept.
#[test]
fn a_file_back_under_another_key_leaves_its_edition_where_it_is() {
    let shelf = Shelf::new("joins-other-key");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (en, ja, id) = english_and_raw(&shelf, &conn);
    let (english, raws) = (holder(&conn, &en), holder(&conn, &ja));
    reader(&conn);
    place(&conn, raws, "ja", 2);
    derive::derive(&conn).expect("derives the join");
    let placed = places(&conn);
    matches::set_state(&conn, &[id], State::Split, None).expect("splits the pair");
    conn.execute(
        "UPDATE root SET edition_key = 'jp' WHERE id = ?1",
        [root_of(&conn, &ja).id],
    )
    .expect("keys the raws jp");

    let report = derive::derive(&conn).expect("derives");

    let back = other_than(&conn, &ja, english);
    assert_eq!(
        (editions(&conn, english), editions(&conn, back)),
        (
            vec![labelled(VOLUMES, None), labelled(RAW, Some(id))],
            vec![labelled(("jp", "raw", "ja"), None)]
        ),
        "the raw is under jp on its own work, and under ja on the English work"
    );
    assert_eq!(places(&conn), placed, "the place stays where it was");
    assert_eq!(
        (report.moved_editions, report.kept_editions),
        (0, 1),
        "{report:?}"
    );
}

/// The raw and a Korean scanlation, which names put on one work, both pair with
/// the English volume at the bar and join it with their group; once the raw's
/// pair is split, the Korean file joins by its own pair and the raw stays where
/// its name puts it, rather than coming back with its group.
#[test]
fn a_split_file_stays_behind_when_its_group_joins_through_another_file() {
    let shelf = Shelf::new("joins-split-group");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let (oshi, oshinoko) = ("Oshi/v01/Oshi v01.cbz", "Oshinoko/v01/Oshinoko v01.cbz");
    let volumes = shelf.directory(&conn, "Volumes", "Manga", VOLUMES, &[oshi]);
    let raw = shelf.directory(&conn, "Raw", "Manga", RAW, &[oshinoko]);
    let scans = shelf.directory(&conn, "Scans", "Manga", KOREAN_SCANS, &[oshinoko]);
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, oshi);
    let ja = digest(&conn, &raw, oshinoko);
    let ko = digest(&conn, &scans, oshinoko);
    for file in [&en, &ja, &ko] {
        read(&conn, file, "whole");
    }
    let en_ja = pair(&conn, &en, &ja, 0.95);
    let en_ko = pair(&conn, &en, &ko, 0.93);
    derive::derive(&conn).expect("derives the joins");
    let host = holder(&conn, &en);
    assert_eq!(
        (holder(&conn, &ja), holder(&conn, &ko)),
        (host, host),
        "the fixture: the raw's group joined the English volume's work"
    );

    matches::set_state(&conn, &[en_ja], State::Split, None).expect("splits the raw's pair");
    derive::derive(&conn).expect("derives the split");

    assert_eq!(
        editions(&conn, host),
        [labelled(VOLUMES, None), labelled(KOREAN_SCANS, Some(en_ko))],
        "the Korean file joins by its own pair, and the host keeps no copy of the raw"
    );
    let apart = holder(&conn, &ja);
    assert_ne!(apart, host, "the raw stays apart");
    assert_eq!(
        (work(&conn, apart), editions(&conn, apart)),
        (
            ("Oshinoko".to_owned(), Some(1.0), "ja".to_owned()),
            vec![labelled(RAW, None)]
        ),
        "on the work its name gives it"
    );
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// A Korean raw and a Chinese one.
const KOREAN_RAW: (&str, &str, &str) = ("ko", "raw", "ko");
const CHINESE_RAW: (&str, &str, &str) = ("zh", "raw", "zh");

/// Each mark on work `id`, by key: the pair, and whether the work holds both
/// of its files.
fn marked(conn: &Connection, id: i64) -> Vec<(String, i64, bool)> {
    let marks = matches::marks(conn, id).expect("reads the marks");
    marks
        .matched
        .into_iter()
        .map(|(key, (pair, _))| {
            let both = named(conn, pair)
                .iter()
                .all(|file| holders(conn, file).contains(&id));
            (key, pair, both)
        })
        .collect()
}

/// The pair [`hub_split_from_its_host`] stores beside its four, at 0.90.
#[derive(Clone, Copy)]
enum Beside {
    Nothing,
    FernOshi,
    FernXeno,
}

/// What [`hub_split_from_its_host`] answers.
struct HubSplit {
    library: Library,
    /// Oshi's, Quill's, Fern's, Kite's and Xeno's digests.
    files: [String; 5],
    /// Fern and Quill, Kite and Quill, Oshi and Quill, Oshi and Kite, then the
    /// pair beside them, if any.
    pairs: Vec<i64>,
}

/// Where [`hub_split_from_its_host`] puts each volume: directory, label, file.
const HUB: [(&str, (&str, &str, &str), &str); 5] = [
    ("Volumes", VOLUMES, "Oshi/v01/Oshi v01.cbz"),
    ("Fan", FAN, "Quill/v01/Quill v01.cbz"),
    ("Raw", RAW, "Fern/v01/Fern v01.cbz"),
    ("RawKo", KOREAN_RAW, "Kite/v01/Kite v01.cbz"),
    ("RawZh", CHINESE_RAW, "Xeno/v01/Xeno v01.cbz"),
];

/// Five volumes, each a series of its own, read whole: the official English
/// Oshi, the fan translation Quill, and three raws, Fern in Japanese, Kite in
/// Korean and Xeno in Chinese. Fern and Kite pair best with Quill, at 0.95 and
/// 0.94; Oshi pairs with Quill at 0.93 and with Kite at 0.92; `beside` adds
/// one pair at 0.90. Derived, so Quill, Fern and Kite join Oshi's work, then
/// Oshi and Quill's pair is split and derived again. The directories are added
/// and the pairs stored in that order, or in the opposite one when
/// `reversed`; the files are written into `shelf` when `write`.
fn hub_split_from_its_host(shelf: &Shelf, write: bool, reversed: bool, beside: Beside) -> HubSplit {
    let library = Library::open_in_memory().expect("open");
    let (files, pairs) = {
        let conn = library.conn();
        let mut order = [0, 1, 2, 3, 4];
        if reversed {
            order.reverse();
        }
        let mut roots = BTreeMap::new();
        for at in order {
            let (name, label, file) = HUB[at];
            let files: &[&str] = if write { &[file] } else { &[] };
            roots.insert(at, shelf.directory(&conn, name, "Manga", label, files));
        }
        derive::derive(&conn).expect("derives");
        let files = [0, 1, 2, 3, 4].map(|at| digest(&conn, &roots[&at], HUB[at].2));
        for file in &files {
            read(&conn, file, "whole");
        }
        let [oshi, quill, fern, kite, xeno] = &files;
        let mut scored = vec![
            (fern, quill, 0.95),
            (kite, quill, 0.94),
            (oshi, quill, 0.93),
            (oshi, kite, 0.92),
        ];
        match beside {
            Beside::Nothing => {}
            Beside::FernOshi => scored.push((fern, oshi, 0.90)),
            Beside::FernXeno => scored.push((fern, xeno, 0.90)),
        }
        let mut stored: Vec<usize> = (0..scored.len()).collect();
        if reversed {
            stored.reverse();
        }
        let mut pairs = vec![0; scored.len()];
        for at in stored {
            let (one, two, v) = scored[at];
            pairs[at] = pair(&conn, one, two, v);
        }
        derive::derive(&conn).expect("derives the joins");
        let host = holder(&conn, oshi);
        assert_eq!(
            [quill, fern, kite].map(|file| holder(&conn, file)),
            [host; 3],
            "the fixture: Quill, Fern and Kite join Oshi's work"
        );
        matches::set_state(&conn, &[pairs[2]], State::Split, None).expect("splits Oshi and Quill");
        derive::derive(&conn).expect("derives the split");
        (files, pairs)
    };
    HubSplit {
        library,
        files,
        pairs,
    }
}

/// Work `host`'s pairs kept apart, by id.
fn kept_apart(conn: &Connection, host: i64) -> Vec<i64> {
    matches::marks(conn, host)
        .expect("reads the marks")
        .kept_apart
        .iter()
        .map(|apart| apart.id)
        .collect()
}

/// The next derive over `conn`'s store commits nothing, and a store of the
/// same files, with the directories added and the pairs stored the other way
/// round, lays out the same works, editions and marks.
fn settles(conn: &mut Connection, shelf: &Shelf, beside: Beside) {
    let (commits, quiet) = commits_during(conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
    let reversed = hub_split_from_its_host(shelf, false, true, beside);
    assert_eq!(
        layout(&reversed.library.conn()),
        layout(conn),
        "the same layout with the directories and pairs stored the other way round"
    );
}

/// Once Oshi and Quill's pair is split, Kite joins Oshi by their own pair and
/// Quill stays out, and Fern, whose only pair is with Quill, stays out with it,
/// the two together on Quill's work, rather than coming along with Kite.
#[test]
fn a_file_joined_only_through_a_file_a_split_leaves_behind_stays_behind_with_it() {
    let shelf = Shelf::new("joins-split-hub");
    let split = hub_split_from_its_host(&shelf, true, false, Beside::Nothing);
    let mut conn = split.library.conn();
    let [oshi, quill, fern, kite, _] = &split.files;
    let [fern_quill, _, oshi_quill, oshi_kite] = split.pairs[..] else {
        unreachable!("four pairs");
    };
    let (host, fan) = (holder(&conn, oshi), holder(&conn, quill));

    assert_eq!(
        (holder(&conn, kite), editions(&conn, host)),
        (
            host,
            vec![
                labelled(VOLUMES, None),
                labelled(KOREAN_RAW, Some(oshi_kite))
            ]
        ),
        "Kite joins by its own pair with Oshi, and Oshi's work holds neither Quill nor Fern"
    );
    assert_ne!(fan, host, "the split pair's files are on two works");
    assert_eq!(
        (holder(&conn, fern), work(&conn, fan), editions(&conn, fan)),
        (
            fan,
            ("Quill".to_owned(), Some(1.0), "fan".to_owned()),
            vec![labelled(FAN, None), labelled(RAW, Some(fern_quill))]
        ),
        "Fern stays with Quill, joined by their pair, on the work Quill's name gives it"
    );
    assert_eq!(
        [host, fan].map(|id| marked(&conn, id)),
        [
            vec![("ko".to_owned(), oshi_kite, true)],
            vec![("ja".to_owned(), fern_quill, true)]
        ],
        "each mark's pair has both its files on the mark's work"
    );
    assert_eq!(
        kept_apart(&conn, host),
        [oshi_quill],
        "Oshi's work shows the pair kept apart"
    );
    settles(&mut conn, &shelf, Beside::Nothing);
}

/// A file a Split leaves behind is not held there: Fern also pairs with Oshi
/// at 0.90, so once Oshi and Quill's pair is split and Fern is left behind
/// with Quill, that pair joins Fern to Oshi's work, and Quill stays out alone.
#[test]
fn a_file_left_behind_joins_the_host_by_its_own_pair() {
    let shelf = Shelf::new("joins-split-own-pair");
    let split = hub_split_from_its_host(&shelf, true, false, Beside::FernOshi);
    let mut conn = split.library.conn();
    let [oshi, quill, fern, kite, _] = &split.files;
    let [_, _, oshi_quill, oshi_kite, fern_oshi] = split.pairs[..] else {
        unreachable!("five pairs");
    };
    let (host, fan) = (holder(&conn, oshi), holder(&conn, quill));

    assert_eq!(
        (
            holder(&conn, fern),
            holder(&conn, kite),
            editions(&conn, host)
        ),
        (
            host,
            host,
            vec![
                labelled(VOLUMES, None),
                labelled(RAW, Some(fern_oshi)),
                labelled(KOREAN_RAW, Some(oshi_kite))
            ]
        ),
        "Fern joins Oshi's work by its own pair, and Kite by its"
    );
    assert_eq!(
        (fan == host, work(&conn, fan), editions(&conn, fan)),
        (
            false,
            ("Quill".to_owned(), Some(1.0), "fan".to_owned()),
            vec![labelled(FAN, None)]
        ),
        "Quill, split from Oshi, stays out alone"
    );
    assert_eq!(
        [host, fan].map(|id| marked(&conn, id)),
        [
            vec![
                ("ja".to_owned(), fern_oshi, true),
                ("ko".to_owned(), oshi_kite, true)
            ],
            vec![]
        ],
        "each mark's pair has both its files on the mark's work"
    );
    assert_eq!(kept_apart(&conn, host), [oshi_quill]);
    settles(&mut conn, &shelf, Beside::FernOshi);
}

/// Files a Split leaves behind together stay together and can take others
/// in: Fern also pairs with Xeno at 0.90, so once Oshi and Quill's pair is
/// split, Fern and Quill, still joined by their pair, take Xeno in on Quill's
/// work.
#[test]
fn files_left_behind_together_stay_together_and_take_in_another() {
    let shelf = Shelf::new("joins-split-take-in");
    let split = hub_split_from_its_host(&shelf, true, false, Beside::FernXeno);
    let mut conn = split.library.conn();
    let [oshi, quill, fern, kite, xeno] = &split.files;
    let [fern_quill, _, oshi_quill, oshi_kite, fern_xeno] = split.pairs[..] else {
        unreachable!("five pairs");
    };
    let (host, fan) = (holder(&conn, oshi), holder(&conn, quill));

    assert_eq!(
        (holder(&conn, kite), editions(&conn, host)),
        (
            host,
            vec![
                labelled(VOLUMES, None),
                labelled(KOREAN_RAW, Some(oshi_kite))
            ]
        ),
        "Kite joins Oshi's work by its own pair"
    );
    assert_ne!(fan, host, "the split pair's files are on two works");
    assert_eq!(
        (
            holder(&conn, fern),
            holder(&conn, xeno),
            work(&conn, fan),
            editions(&conn, fan)
        ),
        (
            fan,
            fan,
            ("Quill".to_owned(), Some(1.0), "fan".to_owned()),
            vec![
                labelled(FAN, None),
                labelled(RAW, Some(fern_quill)),
                labelled(CHINESE_RAW, Some(fern_xeno))
            ]
        ),
        "Fern stays with Quill, and Xeno joins them by its pair with Fern"
    );
    assert_eq!(
        [host, fan].map(|id| marked(&conn, id)),
        [
            vec![("ko".to_owned(), oshi_kite, true)],
            vec![
                ("ja".to_owned(), fern_quill, true),
                ("zh".to_owned(), fern_xeno, true)
            ]
        ],
        "each mark's pair has both its files on the mark's work"
    );
    assert_eq!(kept_apart(&conn, host), [oshi_quill]);
    settles(&mut conn, &shelf, Beside::FernXeno);
}

/// A file the moving file still reaches through files that join, joins with
/// them. Alpha pairs with the fan translation Quill and with Beta, Beta with
/// Gamma, and Gamma with Oshi. Once Oshi and Quill's pair is split, Quill
/// stays out and the other three join Oshi along that chain, Alpha marked by
/// its pair with Beta, though its best pair is the one with Quill.
#[test]
fn a_file_tied_to_the_join_through_files_that_join_joins_with_them() {
    let shelf = Shelf::new("joins-split-chain");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let placed = [
        ("Volumes", VOLUMES, "Oshi/v01/Oshi v01.cbz"),
        ("Fan", FAN, "Quill/v01/Quill v01.cbz"),
        ("Raw", RAW, "Alpha/v01/Alpha v01.cbz"),
        ("RawKo", KOREAN_RAW, "Beta/v01/Beta v01.cbz"),
        ("RawZh", CHINESE_RAW, "Gamma/v01/Gamma v01.cbz"),
    ];
    let roots =
        placed.map(|(name, label, file)| shelf.directory(&conn, name, "Manga", label, &[file]));
    derive::derive(&conn).expect("derives");
    let files: Vec<String> = roots
        .iter()
        .zip(placed)
        .map(|(root, (_, _, file))| digest(&conn, root, file))
        .collect();
    for file in &files {
        read(&conn, file, "whole");
    }
    let [oshi, quill, alpha, beta, gamma] = &files[..] else {
        unreachable!("five files");
    };
    pair(&conn, quill, alpha, 0.97);
    let alpha_beta = pair(&conn, alpha, beta, 0.96);
    let beta_gamma = pair(&conn, beta, gamma, 0.95);
    let oshi_quill = pair(&conn, oshi, quill, 0.93);
    pair(&conn, oshi, gamma, 0.92);
    derive::derive(&conn).expect("derives the joins");
    let host = holder(&conn, oshi);
    assert_eq!(
        [quill, alpha, beta, gamma].map(|file| holder(&conn, file)),
        [host; 4],
        "the fixture: all five are one work, Oshi's"
    );

    matches::set_state(&conn, &[oshi_quill], State::Split, None).expect("splits Oshi and Quill");
    derive::derive(&conn).expect("derives the split");

    assert_eq!(
        editions(&conn, host),
        [
            labelled(VOLUMES, None),
            labelled(RAW, Some(alpha_beta)),
            labelled(KOREAN_RAW, Some(alpha_beta)),
            labelled(CHINESE_RAW, Some(beta_gamma)),
        ],
        "Alpha, Beta and Gamma join, each marked by its best pair whose files both join"
    );
    let fan = holder(&conn, quill);
    assert_eq!(
        (fan == host, editions(&conn, fan)),
        (false, vec![labelled(FAN, None)]),
        "Quill stays out"
    );
    assert_eq!(
        marked(&conn, host),
        [
            ("ja".to_owned(), alpha_beta, true),
            ("ko".to_owned(), alpha_beta, true),
            ("zh".to_owned(), beta_gamma, true),
        ],
        "each mark's pair has both its files on the work"
    );
}

/// Names put the Japanese raw Fern and a Korean raw on one work, and Fern's
/// one pair, with the fan translation Quill, moves that group to Quill's side.
/// Once Oshi and Quill's pair is split, the Korean raw joins Oshi by its own
/// pair and brings Fern along by names. The pair that first moved their group
/// names Quill, which stays out, so neither carries it as a mark.
#[test]
fn a_file_its_name_brings_along_is_not_marked_by_a_pair_with_a_file_left_behind() {
    let shelf = Shelf::new("joins-split-names");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (oshi, quill, fern) = (
        "Oshi/v01/Oshi v01.cbz",
        "Quill/v01/Quill v01.cbz",
        "Fern/v01/Fern v01.cbz",
    );
    let volumes = shelf.directory(&conn, "Volumes", "Manga", VOLUMES, &[oshi]);
    let fans = shelf.directory(&conn, "Fan", "Manga", FAN, &[quill]);
    let raws = shelf.directory(&conn, "Raw", "Manga", RAW, &[fern]);
    let korean = shelf.directory(&conn, "RawKo", "Manga", KOREAN_RAW, &[fern]);
    derive::derive(&conn).expect("derives");
    let en = digest(&conn, &volumes, oshi);
    let fan = digest(&conn, &fans, quill);
    let ja = digest(&conn, &raws, fern);
    let ko = digest(&conn, &korean, fern);
    for file in [&en, &fan, &ja, &ko] {
        read(&conn, file, "whole");
    }
    pair(&conn, &ja, &fan, 0.95);
    let en_fan = pair(&conn, &en, &fan, 0.93);
    let en_ko = pair(&conn, &en, &ko, 0.92);
    derive::derive(&conn).expect("derives the joins");
    let host = holder(&conn, &en);
    assert_eq!(
        [&fan, &ja, &ko].map(|file| holder(&conn, file)),
        [host; 3],
        "the fixture: all four are one work, Oshi's"
    );

    matches::set_state(&conn, &[en_fan], State::Split, None).expect("splits Oshi and Quill");
    derive::derive(&conn).expect("derives the split");

    assert_eq!(
        editions(&conn, host),
        [
            labelled(VOLUMES, None),
            labelled(RAW, Some(en_ko)),
            labelled(KOREAN_RAW, Some(en_ko)),
        ],
        "Fern comes along with the Korean raw, by the pair that moved their group"
    );
    assert_eq!(
        marked(&conn, host),
        [("ko".to_owned(), en_ko, true)],
        "only the Korean raw is marked, and no mark names Quill"
    );
    let apart = holder(&conn, &fan);
    assert_eq!(
        (apart == host, editions(&conn, apart)),
        (false, vec![labelled(FAN, None)]),
        "Quill stays out"
    );
}

/// French, official.
const FRENCH: (&str, &str, &str) = ("fr", "official", "fr");
/// Japanese, official.
const JAPANESE: (&str, &str, &str) = ("ja", "official", "ja");
/// An English raw.
const ENGLISH_RAW: (&str, &str, &str) = ("en", "raw", "en");

/// Names tie a group's editions a join leaves behind into one component:
/// Gin's English fan translation is left behind when its raws join Hana,
/// which holds `en`, and its Korean raw when Hana joins Zan, which holds
/// `ko`. The two stay together on Gin's work, so Xeno's English raw, paired
/// with Gin's Korean, stays out rather than take the key Gin's English holds.
#[test]
fn a_group_left_behind_at_two_joins_keeps_its_editions_together() {
    let shelf = Shelf::new("joins-left-twice");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let file = |series: &str| format!("{series}/v01/{series} v01.cbz");
    let zan = shelf.directory(&conn, "Korean", "Manga", KOREAN, &[file("Zan").as_str()]);
    derive::derive(&conn).expect("derives Zan first, so its series is the oldest");
    let at = |name: &str, label, series: &str| {
        let root = shelf.directory(&conn, name, "Manga", label, &[file(series).as_str()]);
        (root, series.to_owned())
    };
    let roots = [
        at("English", VOLUMES, "Hana"),
        at("Scans", EN_SCANS, "Gin"),
        at("Raw", RAW, "Gin"),
        at("RawKo", KOREAN_RAW, "Gin"),
        at("RawEn", ENGLISH_RAW, "Xeno"),
    ];
    derive::derive(&conn).expect("derives");
    let zan_ko = digest(&conn, &zan, &file("Zan"));
    let [hana_en, gin_en, gin_ja, gin_ko, xeno_en] = roots
        .each_ref()
        .map(|(root, series)| digest(&conn, root, &file(series)));
    for one in [&zan_ko, &hana_en, &gin_en, &gin_ja, &gin_ko, &xeno_en] {
        read(&conn, one, "whole");
    }
    let hana_gin = pair(&conn, &hana_en, &gin_ja, 0.95);
    pair(&conn, &zan_ko, &hana_en, 0.93);
    pair(&conn, &xeno_en, &gin_ko, 0.91);
    derive::derive(&conn).expect("derives the joins");

    let (host, gin, xeno) = (
        holder(&conn, &zan_ko),
        holder(&conn, &gin_en),
        holder(&conn, &xeno_en),
    );
    assert_eq!(
        (
            holder(&conn, &hana_en),
            holder(&conn, &gin_ja),
            editions(&conn, host)
        ),
        (
            host,
            host,
            vec![
                labelled(VOLUMES, Some(hana_gin)),
                labelled(RAW, Some(hana_gin)),
                labelled(KOREAN, None)
            ]
        ),
        "Hana and Gin's Japanese raw are on Zan's work, each marked by the first pair naming it"
    );
    assert_eq!(
        (
            holder(&conn, &gin_ko),
            work(&conn, gin),
            editions(&conn, gin)
        ),
        (
            gin,
            ("Gin".to_owned(), Some(1.0), "en".to_owned()),
            vec![labelled(EN_SCANS, None), labelled(KOREAN_RAW, None)]
        ),
        "Gin's English and Korean, left behind at two joins, stay together on Gin's work"
    );
    assert_eq!(
        (xeno == gin, editions(&conn, xeno)),
        (false, vec![labelled(ENGLISH_RAW, None)]),
        "Xeno stays out"
    );
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// Hana, the older series, in French and a Japanese raw; Gin in English and
/// in Japanese labelled `japanese`; Yuki's fan translation. Hana's French and
/// Gin's English pair at 0.95, so Gin's English, its hub, joins Hana, and its
/// Japanese, whose key Hana holds, is left behind; that Japanese pairs with
/// Yuki at 0.93. Answers the store, Gin's work before the pairs were stored,
/// the digests of Hana's French, Gin's English, Gin's Japanese and Yuki, and
/// the two pairs.
fn left_with_its_hub_gone(
    shelf: &Shelf,
    japanese: (&str, &str, &str),
) -> (Library, i64, [String; 4], [i64; 2]) {
    let library = Library::open_in_memory().expect("open");
    let (gin, files, pairs) = {
        let conn = library.conn();
        let file = |series: &str| format!("{series}/v01/{series} v01.cbz");
        let french = shelf.directory(&conn, "French", "Manga", FRENCH, &[file("Hana").as_str()]);
        shelf.directory(&conn, "HanaRaw", "Manga", RAW, &[file("Hana").as_str()]);
        derive::derive(&conn).expect("derives Hana first, so its series is older");
        let roots = [
            (french, "Hana"),
            (
                shelf.directory(&conn, "English", "Manga", VOLUMES, &[file("Gin").as_str()]),
                "Gin",
            ),
            (
                shelf.directory(
                    &conn,
                    "Japanese",
                    "Manga",
                    japanese,
                    &[file("Gin").as_str()],
                ),
                "Gin",
            ),
            (
                shelf.directory(&conn, "Fan", "Manga", FAN, &[file("Yuki").as_str()]),
                "Yuki",
            ),
        ];
        derive::derive(&conn).expect("derives");
        let files = roots
            .each_ref()
            .map(|(root, series)| digest(&conn, root, &file(series)));
        for one in &files {
            read(&conn, one, "whole");
        }
        let gin = holder(&conn, &files[2]);
        let pairs = [
            pair(&conn, &files[0], &files[1], 0.95),
            pair(&conn, &files[2], &files[3], 0.93),
        ];
        derive::derive(&conn).expect("derives the joins");
        (gin, files, pairs)
    };
    (library, gin, files, pairs)
}

/// Hana, the older series, in French and a Japanese raw; Gin in English, its
/// hub, a Japanese raw and German; Yuki's fan translation. Hana's French and
/// Gin's English pair at 0.95 and Gin's raw and Yuki at 0.93, so Gin's work is
/// written onto by no group: its English and German join Hana, and its raw,
/// whose key Hana holds, is left behind and joins Yuki. `spread` flags a page
/// of Gin's raw first, which a move of that edition refuses. Answers the
/// store, the six digests (Hana's two, Gin's three, Yuki's), the two pairs
/// and the report of the derive that joins them.
fn gin_strays(shelf: &Shelf, spread: bool) -> (Library, [String; 6], [i64; 2], DeriveReport) {
    let library = Library::open_in_memory().expect("open");
    let (files, pairs, report) = {
        let conn = library.conn();
        let directory = |name: &str, labels, series: &str| {
            let path = format!("{series}/v01/{series} v01.cbz");
            let root = shelf.directory(&conn, name, "Manga", labels, &[path.as_str()]);
            (root, path)
        };
        let french = directory("French", FRENCH, "Hana");
        let hana_raw = directory("HanaRaw", RAW, "Hana");
        derive::derive(&conn).expect("derives Hana first");
        let english = directory("Volumes", VOLUMES, "Gin");
        let gin_raw = directory("GinRaw", RAW, "Gin");
        let german = directory("German", GERMAN, "Gin");
        let fans = directory("Fans", FAN, "Yuki");
        derive::derive(&conn).expect("derives");
        let files = [french, hana_raw, english, gin_raw, german, fans]
            .map(|(root, path)| digest(&conn, &root, &path));
        for one in &files {
            read(&conn, one, "whole");
        }
        let gin = holder(&conn, &files[2]);
        assert_eq!(
            (holder(&conn, &files[3]), holder(&conn, &files[4])),
            (gin, gin),
            "the fixture: Gin's three editions share a work"
        );
        if spread {
            conn.execute(
                "INSERT INTO page_flag (edition_id, kind, page)
                 SELECT id, 'spread', 1 FROM edition WHERE work_id = ?1 AND key = 'ja'",
                [gin],
            )
            .expect("flags a spread on Gin's raw");
        }
        let pairs = [
            pair(&conn, &files[0], &files[2], 0.95),
            pair(&conn, &files[3], &files[5], 0.93),
        ];
        let report = derive::derive(&conn).expect("derives the joins");
        (files, pairs, report)
    };
    (library, files, pairs, report)
}

const GERMAN: (&str, &str, &str) = ("de", "official", "de");

/// Only Gin's raw moves on its own; the German, bound where the hub goes, is
/// left to the fold, so a move of every edition but the hub counts two.
#[test]
fn a_stray_works_edition_bound_with_its_hub_is_left_to_the_fold() {
    let shelf = Shelf::new("joins-stray-bound");
    let (library, [fr, _, en, ja, de, fan], [hana_gin, gin_yuki], report) =
        gin_strays(&shelf, false);
    let conn = library.conn();

    let (hana, yuki) = (holder(&conn, &fr), holder(&conn, &fan));
    assert_eq!(
        [&en, &de, &ja].map(|one| holder(&conn, one)),
        [hana, hana, yuki],
        "Gin's English and German on Hana's work, its raw on Yuki's"
    );
    assert_eq!(
        editions(&conn, yuki),
        [labelled(FAN, None), labelled(RAW, Some(gin_yuki))],
        "Yuki's work hosts Gin's raw"
    );
    assert_eq!(
        editions(&conn, hana),
        [
            labelled(GERMAN, Some(hana_gin)),
            labelled(VOLUMES, Some(hana_gin)),
            labelled(FRENCH, None),
            labelled(RAW, None)
        ]
    );
    assert_eq!(
        (report.moved_editions, report.unmoved_editions),
        (1, 0),
        "the raw alone moved on its own: {report:?}"
    );
}

/// As above, with a spread flagged on Gin's raw: its move is refused and
/// counted, and the refusal says why.
#[test]
fn a_stray_works_edition_whose_move_is_refused_is_counted() {
    let shelf = Shelf::new("joins-stray-refused");
    let (_library, _, _, report) = gin_strays(&shelf, true);

    assert_eq!(
        (report.moved_editions, report.unmoved_editions),
        (0, 1),
        "the raw's move refused: {report:?}"
    );
    assert!(
        report.unfoldable.iter().any(|why| why.contains("spread")),
        "the refusal names the spread: {:?}",
        report.unfoldable
    );
}

/// A part a join leaves behind ranks by the editions it holds, not by those
/// of its group that left: Gin's Japanese raw, left behind when Gin's official
/// English joins Hana, ranks below Yuki's fan translation and joins Yuki's
/// work.
#[test]
fn a_part_left_behind_ranks_by_the_editions_it_holds() {
    let shelf = Shelf::new("joins-left-rank");
    let (library, _, [french, english, japanese, fan], [hana_gin, gin_yuki]) =
        left_with_its_hub_gone(&shelf, RAW);
    let mut conn = library.conn();
    let (hana, yuki) = (holder(&conn, &french), holder(&conn, &fan));
    assert_eq!(
        (holder(&conn, &english), holder(&conn, &japanese)),
        (hana, yuki),
        "Gin's English is on Hana's work, and its raw on Yuki's"
    );
    assert_eq!(
        (work(&conn, yuki), editions(&conn, yuki)),
        (
            ("Yuki".to_owned(), Some(1.0), "fan".to_owned()),
            vec![labelled(FAN, None), labelled(RAW, Some(gin_yuki))]
        ),
        "Yuki's work hosts Gin's raw"
    );
    assert_eq!(
        editions(&conn, hana),
        [
            labelled(VOLUMES, Some(hana_gin)),
            labelled(FRENCH, None),
            labelled(RAW, None)
        ]
    );
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// A part a join leaves behind that hosts keeps its group's work, and where
/// the hub that work had left takes the best edition of its own there as hub,
/// never a guest's: Gin's official Japanese, left behind when Gin's English
/// joins Hana, hosts Yuki's fan translation on Gin's work, which keeps series
/// Gin with hub `ja`, though Yuki's key `fan` sorts first and names Yuki's work.
#[test]
fn a_part_left_behind_whose_hub_left_keeps_its_work_and_its_own_hub() {
    let shelf = Shelf::new("joins-left-hub");
    let (library, gin, [french, english, japanese, fan], [hana_gin, gin_yuki]) =
        left_with_its_hub_gone(&shelf, JAPANESE);
    let mut conn = library.conn();
    let hana = holder(&conn, &french);
    assert_eq!(
        [&english, &japanese, &fan].map(|one| holder(&conn, one)),
        [hana, gin, gin],
        "Gin's English is on Hana's work, and Yuki on Gin's own"
    );
    assert_eq!(
        (work(&conn, gin), editions(&conn, gin)),
        (
            ("Gin".to_owned(), Some(1.0), "ja".to_owned()),
            vec![labelled(FAN, Some(gin_yuki)), labelled(JAPANESE, None)]
        ),
        "Gin's work keeps its series and takes its own Japanese as hub"
    );
    assert_eq!(
        editions(&conn, hana),
        [
            labelled(VOLUMES, Some(hana_gin)),
            labelled(FRENCH, None),
            labelled(RAW, None)
        ]
    );
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// Hana, the older series, in French, a Japanese raw and a Korean raw; Gin
/// in English, a Japanese raw and an official Korean; with `yuki`, Yuki's
/// scanlation. Hana's French and Gin's English pair at 0.95, so Gin's English,
/// its hub, joins Hana, and its Japanese and Korean, whose keys Hana holds,
/// are left behind together; Gin's Korean pairs with Yuki at 0.93. Answers the
/// store, Gin's work, Gin's three digests, and Yuki's digest and pair.
fn left_in_two_editions(
    shelf: &Shelf,
    yuki: bool,
) -> (Library, i64, [String; 3], Option<(String, i64)>) {
    let library = Library::open_in_memory().expect("open");
    let (gin, files, guest) = {
        let conn = library.conn();
        let file = |series: &str| format!("{series}/v01/{series} v01.cbz");
        let at = |name: &str, label, series: &str| {
            let root = shelf.directory(&conn, name, "Manga", label, &[file(series).as_str()]);
            (root, series.to_owned())
        };
        let french = at("French", FRENCH, "Hana");
        at("HanaRaw", RAW, "Hana");
        at("HanaKo", KOREAN_RAW, "Hana");
        derive::derive(&conn).expect("derives Hana first, so its series is older");
        let roots = [
            at("English", VOLUMES, "Gin"),
            at("Japanese", RAW, "Gin"),
            at("Korean", KOREAN, "Gin"),
        ];
        let scans = yuki.then(|| at("Scans", SCANS, "Yuki"));
        derive::derive(&conn).expect("derives");
        let digest_of = |(root, series): &(Root, String)| digest(&conn, root, &file(series));
        let hana_fr = digest_of(&french);
        let files = roots.each_ref().map(digest_of);
        let scans = scans.as_ref().map(digest_of);
        for one in std::iter::once(&hana_fr).chain(&files).chain(&scans) {
            read(&conn, one, "whole");
        }
        let gin = holder(&conn, &files[1]);
        pair(&conn, &hana_fr, &files[0], 0.95);
        let guest = scans.map(|scans| {
            let id = pair(&conn, &files[2], &scans, 0.93);
            (scans, id)
        });
        derive::derive(&conn).expect("derives the joins");
        (gin, files, guest)
    };
    (library, gin, files, guest)
}

/// Where the hub a part's work had left, the part's best edition by kind
/// becomes its hub, not the first by key: Gin's Japanese raw and Korean
/// official, left behind together, host Yuki's scanlation on Gin's work with
/// hub `ko`.
#[test]
fn a_part_left_behind_takes_its_best_edition_as_hub() {
    let shelf = Shelf::new("joins-left-best-hub");
    let (library, gin, [_, gin_ja, gin_ko], guest) = left_in_two_editions(&shelf, true);
    let Some((yuki, gin_yuki)) = guest else {
        unreachable!("Yuki is there");
    };
    let mut conn = library.conn();
    assert_eq!(
        [&gin_ja, &gin_ko, &yuki].map(|one| holder(&conn, one)),
        [gin; 3],
        "Gin's Japanese and Korean host Yuki on Gin's work"
    );
    assert_eq!(
        (work(&conn, gin), editions(&conn, gin)),
        (
            ("Gin".to_owned(), Some(1.0), "ko".to_owned()),
            vec![
                labelled(RAW, None),
                labelled(KOREAN, None),
                labelled(SCANS, Some(gin_yuki))
            ]
        ),
        "the official Korean is the hub, though the raw's key sorts first"
    );
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// A part left behind that hosts nothing takes its best edition as hub too:
/// with no Yuki, Gin's Japanese raw and Korean official stay on Gin's work,
/// whose hub left with its English, with hub `ko`.
#[test]
fn a_part_left_behind_hosting_nothing_takes_its_best_edition_as_hub() {
    let shelf = Shelf::new("joins-left-lone-hub");
    let (library, gin, [gin_en, gin_ja, gin_ko], _) = left_in_two_editions(&shelf, false);
    let mut conn = library.conn();
    assert_eq!(
        [&gin_ja, &gin_ko].map(|one| holder(&conn, one)),
        [gin; 2],
        "Gin's Japanese and Korean stay on Gin's work"
    );
    assert_ne!(holder(&conn, &gin_en), gin, "Gin's English joins Hana");
    assert_eq!(
        (work(&conn, gin), editions(&conn, gin)),
        (
            ("Gin".to_owned(), Some(1.0), "ko".to_owned()),
            vec![labelled(RAW, None), labelled(KOREAN, None)]
        ),
        "the official Korean is the hub, though the raw's key sorts first"
    );
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// A join that would part a group's editions left behind applies nothing, so
/// the pair of higher V keeps its join (spec §4): Gin's English, left behind
/// when its raws join Hana, takes in Yuki's Korean raw at 0.93. Zan's pair
/// with Hana at 0.91 would leave Gin's Korean behind beside it, under the key
/// Yuki's holds, so it joins nothing, and Zan stays alone.
#[test]
fn a_join_that_would_part_a_groups_editions_left_behind_applies_nothing() {
    let shelf = Shelf::new("joins-left-clash");
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let file = |series: &str| format!("{series}/v01/{series} v01.cbz");
    let at = |name: &str, label, series: &str| {
        let root = shelf.directory(&conn, name, "Manga", label, &[file(series).as_str()]);
        (root, series.to_owned())
    };
    let zan = at("Korean", KOREAN, "Zan");
    derive::derive(&conn).expect("derives Zan first, so its series is the oldest");
    let roots = [
        at("English", VOLUMES, "Hana"),
        at("Scans", EN_SCANS, "Gin"),
        at("Raw", RAW, "Gin"),
        at("RawKo", KOREAN_RAW, "Gin"),
        at("YukiKo", KOREAN_RAW, "Yuki"),
    ];
    derive::derive(&conn).expect("derives");
    let digest_of = |(root, series): &(Root, String)| digest(&conn, root, &file(series));
    let zan_ko = digest_of(&zan);
    let [hana_en, gin_en, gin_ja, gin_ko, yuki_ko] = roots.each_ref().map(digest_of);
    for one in [&zan_ko, &hana_en, &gin_en, &gin_ja, &gin_ko, &yuki_ko] {
        read(&conn, one, "whole");
    }
    let hana_gin = pair(&conn, &hana_en, &gin_ja, 0.95);
    let gin_yuki = pair(&conn, &gin_en, &yuki_ko, 0.93);
    pair(&conn, &zan_ko, &hana_en, 0.91);
    derive::derive(&conn).expect("derives the joins");

    let (zan, hana, gin) = (
        holder(&conn, &zan_ko),
        holder(&conn, &hana_en),
        holder(&conn, &gin_en),
    );
    assert_eq!(
        (zan == hana, editions(&conn, zan)),
        (false, vec![labelled(KOREAN, None)]),
        "Zan stays alone"
    );
    assert_eq!(
        (
            holder(&conn, &gin_ja),
            holder(&conn, &gin_ko),
            editions(&conn, hana)
        ),
        (
            hana,
            hana,
            vec![
                labelled(VOLUMES, None),
                labelled(RAW, Some(hana_gin)),
                labelled(KOREAN_RAW, Some(hana_gin))
            ]
        ),
        "Gin's raws are on Hana's work"
    );
    assert_eq!(
        (holder(&conn, &yuki_ko), editions(&conn, gin)),
        (
            gin,
            vec![
                labelled(EN_SCANS, None),
                labelled(KOREAN_RAW, Some(gin_yuki))
            ]
        ),
        "Yuki's Korean stays joined to Gin's English"
    );
    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    quiet.expect("a derive over nothing changed");
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// How many seeds [`drawn_stores_keep_every_rule_of_a_join`] draws by default.
const CASES: u64 = 160;

/// Seeds past [`CASES`] that caught a group's editions left behind at two
/// joins taking a key it still held, drawn by default too.
const CAUGHT: [u64; 4] = [219, 1435, 1442, 1568];

/// A seeded xorshift64*: each drawn store is the same on every run, so a
/// failing seed names one store.
struct Seeded(u64);

impl Seeded {
    fn new(seed: u64) -> Self {
        Self((seed ^ 0x9E37_79B9_7F4A_7C15).wrapping_mul(0xBF58_476D_1CE4_E5B9) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).expect("a small bound");
        usize::try_from(self.next() % n).expect("below a usize")
    }

    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

/// What a drawn store holds: series, the keys and kinds of their editions,
/// and the scores between them, few enough that ties and shared keys are
/// common.
const NAMES: [&str; 6] = ["Aoi", "Bara", "Chiyo", "Dango", "Eri", "Fumi"];
const KEYS: [(&str, &str); 5] = [
    ("en", "en"),
    ("ja", "ja"),
    ("ko", "ko"),
    ("fan", "en"),
    ("scans", "en"),
];
const KINDS: [&str; 3] = ["official", "fan", "raw"];
const SCORES: [f64; 5] = [0.87, 0.89, 0.91, 0.93, 0.95];

/// One drawn store.
#[derive(Debug)]
struct Drawn {
    /// Each file: indices into [NAMES], [KEYS] and [KINDS]. A series' files
    /// are volume 1 under keys of their own, so names put them in one group.
    files: Vec<(usize, usize, usize)>,
    /// Whether each series is derived before the others, and so is older.
    older: Vec<bool>,
    /// Each pair: two indices into `files`, V, and whether it is split.
    pairs: Vec<(usize, usize, f64, bool)>,
    /// Whether the split pairs are split before the first derive that sees
    /// any pair, or after it joined them.
    split_first: bool,
    /// A raw directory added beside the drawn files, if this store has one.
    follow: Option<Follow>,
}

/// A directory holding volumes 1 and 2 of a series no drawn file names, side
/// by side as a directory of raws holds them, volume 1 paired with
/// `files[with]` at `v`: volume 2 follows where volume 1 joins, and then
/// volume 2 of `files[with]`'s series arrives under its directory's label,
/// last when built in order and first when built reversed.
#[derive(Debug)]
struct Follow {
    /// Indices into [NAMES], [KEYS] and [KINDS].
    name: usize,
    key: usize,
    kind: usize,
    with: usize,
    v: f64,
}

fn drawn(seed: u64) -> Drawn {
    let mut draw = Seeded::new(seed);
    // Half the stores are crowded: one series in three fan or raw editions
    // among three to five series of one edition each, all under three keys,
    // so joins leave its editions behind more than once.
    let crowded = draw.one_in(2);
    let series = if crowded {
        4 + draw.below(3)
    } else {
        2 + draw.below(4)
    };
    let mut files = Vec::new();
    for name in 0..series {
        let mut keys: Vec<usize> = (0..if crowded { 3 } else { KEYS.len() }).collect();
        let editions = match (crowded, name) {
            (true, 0) => 3,
            (true, _) => 1,
            (false, _) => 1 + draw.below(3),
        };
        for _ in 0..editions {
            let key = keys.remove(draw.below(keys.len()));
            let kind = if crowded && name == 0 {
                1 + draw.below(2)
            } else {
                draw.below(KINDS.len())
            };
            files.push((name, key, kind));
        }
    }
    let older = (0..series).map(|_| draw.one_in(3)).collect();
    let count = files.len();
    let mut open: Vec<(usize, usize)> = (0..count)
        .flat_map(|one| (one + 1..count).map(move |two| (one, two)))
        .filter(|(one, two)| files[*one].0 != files[*two].0)
        .collect();
    let mut pairs = Vec::new();
    // A crowded store's first pairs chain through its three-edition series:
    // series 1 with it, series 2 with series 1 or with it, series 3 with it.
    let chain: &[(usize, &[usize])] = if crowded {
        &[(1, &[0]), (2, &[0, 1]), (3, &[0])]
    } else {
        &[]
    };
    let drawn_pair = |draw: &mut Seeded, open: &mut Vec<(usize, usize)>, at: usize| {
        let (one, two) = open.remove(at);
        (one, two, SCORES[draw.below(SCORES.len())], draw.one_in(4))
    };
    for (series, with) in chain {
        let with = with[draw.below(with.len())];
        let fits: Vec<usize> = (0..open.len())
            .filter(|at| {
                let (one, two) = open[*at];
                let ends = [files[one].0, files[two].0];
                ends.contains(series) && ends.contains(&with)
            })
            .collect();
        if !fits.is_empty() {
            let at = fits[draw.below(fits.len())];
            pairs.push(drawn_pair(&mut draw, &mut open, at));
        }
    }
    let more = if crowded {
        draw.below(5)
    } else {
        1 + draw.below(open.len().min(8))
    };
    for _ in 0..more.min(open.len()) {
        let at = draw.below(open.len());
        pairs.push(drawn_pair(&mut draw, &mut open, at));
    }
    let split_first = draw.one_in(2);
    // Drawn last, so a store without one is the store its seed drew before
    // there were any; never for a seed in [`CAUGHT`], whose store is the one
    // that caught its defect.
    let unused: Vec<usize> = (0..NAMES.len())
        .filter(|name| files.iter().all(|file| file.0 != *name))
        .collect();
    let follow = (!CAUGHT.contains(&seed) && !unused.is_empty() && draw.one_in(4)).then(|| {
        let with = draw.below(files.len());
        let keys: Vec<usize> = (0..KEYS.len())
            .filter(|key| *key != files[with].1)
            .collect();
        Follow {
            name: unused[draw.below(unused.len())],
            key: keys[draw.below(keys.len())],
            kind: draw.below(KINDS.len()),
            with,
            v: SCORES[draw.below(SCORES.len())],
        }
    });
    Drawn {
        files,
        older,
        pairs,
        split_first,
        follow,
    }
}

/// The directory holding a drawn file, one per file so a store adds only
/// the files it draws, and the file's path in it.
fn drawn_at((name, key, kind): (usize, usize, usize)) -> (String, String) {
    let series = NAMES[name];
    (
        format!("{}-{}-{series}", KEYS[key].0, KINDS[kind]),
        format!("{series}/v01/{series} v01.cbz"),
    )
}

/// A [`Follow`]'s directory and its two volumes' paths in it.
fn follow_at(follow: &Follow) -> (String, [String; 2]) {
    let series = NAMES[follow.name];
    (
        format!(
            "follow-{}-{}-{series}",
            KEYS[follow.key].0, KINDS[follow.kind]
        ),
        [1, 2].map(|n| format!("{series}/{series} v{n:02}.cbz")),
    )
}

/// The directory volume 2 of a drawn file's series arrives in, labelled as
/// the file's is, and the volume's path in it.
fn arrival_at((name, key, kind): (usize, usize, usize)) -> (String, String) {
    let series = NAMES[name];
    (
        format!("{}-{}-{series}-v02", KEYS[key].0, KINDS[kind]),
        format!("{series}/v02/{series} v02.cbz"),
    )
}

/// The label a drawn file's directory gives its editions.
fn label_of((_, key, kind): (usize, usize, usize)) -> (&'static str, &'static str, &'static str) {
    (KEYS[key].0, KINDS[kind], KEYS[key].1)
}

fn described(drawn: &Drawn) -> String {
    let file = |at: usize| {
        let (name, key, kind) = drawn.files[at];
        format!("{}:{}/{}", NAMES[name], KEYS[key].0, KINDS[kind])
    };
    let pairs: Vec<String> = drawn
        .pairs
        .iter()
        .map(|&(one, two, v, split)| {
            format!(
                "{}-{} {v}{}",
                file(one),
                file(two),
                if split { " split" } else { "" }
            )
        })
        .collect();
    let older: Vec<&str> = (0..drawn.older.len())
        .filter(|at| drawn.older[*at])
        .map(|at| NAMES[at])
        .collect();
    let follow = drawn.follow.as_ref().map_or_else(String::new, |follow| {
        format!(
            "; {}:{}/{} v01-v02 beside each other, v01-{} {}, then {} v02 arrives",
            NAMES[follow.name],
            KEYS[follow.key].0,
            KINDS[follow.kind],
            file(follow.with),
            follow.v,
            NAMES[drawn.files[follow.with].0]
        )
    });
    format!(
        "files [{}]; older {older:?}; pairs [{}]; split {}{follow}",
        (0..drawn.files.len())
            .map(file)
            .collect::<Vec<_>>()
            .join(", "),
        pairs.join(", "),
        if drawn.split_first {
            "first"
        } else {
            "after a join"
        }
    )
}

/// An empty store, copied from `template`, which is migrated already: a
/// migration takes most of the time a drawn store would.
fn migrated(template: &Library) -> Result<Library, String> {
    let mut conn = Connection::open_in_memory().map_err(|e| format!("in memory: {e}"))?;
    {
        let template = template.conn();
        rusqlite::backup::Backup::new(&template, &mut conn)
            .and_then(|copy| copy.run_to_completion(1024, std::time::Duration::ZERO, None))
            .map_err(|e| format!("copying the template: {e}"))?;
    }
    Library::from_connection(conn, None)
}

/// A drawn store, built: each file's digest, and the series its name gave it.
struct Built {
    library: Library,
    files: Vec<String>,
    named: Vec<String>,
}

/// `drawn` built as an admin would: the older series' directories added
/// and derived, then the rest with any [`Follow`]'s directory, then the pairs
/// stored and derived, with the split ones split first or after that derive,
/// and then a follow's arrival scanned and derived. The directories and pairs
/// go in the opposite order when `reversed`, the arrival first. Each file's
/// digest is in `files` as [`Drawn::files`] orders them, then a follow's
/// volumes 1 and 2, then its arrival.
fn built(
    shelf: &Shelf,
    template: &Library,
    drawn: &Drawn,
    reversed: bool,
) -> Result<Built, String> {
    let library = migrated(template)?;
    let (files, named) = {
        let conn = library.conn();
        let arrive = |conn: &Connection| {
            drawn.follow.as_ref().map(|follow| {
                let with = drawn.files[follow.with];
                let root = shelf.directory(conn, &arrival_at(with).0, "Manga", label_of(with), &[]);
                (root, follow.with)
            })
        };
        let mut arrived = None;
        let mut unread = false;
        let mut follower = None;
        let mut roots = BTreeMap::new();
        for older in [true, false] {
            let mut order: Vec<usize> = (0..drawn.files.len())
                .filter(|at| drawn.older[drawn.files[*at].0] == older)
                .collect();
            if reversed {
                order.reverse();
            }
            let follows = drawn.follow.as_ref().filter(|_| !older);
            let mut add_follower = |conn: &Connection| {
                if let Some(follow) = follows {
                    let (directory, paths) = follow_at(follow);
                    let label = (KEYS[follow.key].0, KINDS[follow.kind], KEYS[follow.key].1);
                    follower = Some((
                        shelf.directory(conn, &directory, "Manga", label, &[]),
                        paths,
                    ));
                }
            };
            if reversed {
                // In its series' own batch, so the series is as old as
                // when it arrives last.
                if drawn
                    .follow
                    .as_ref()
                    .is_some_and(|follow| drawn.older[drawn.files[follow.with].0] == older)
                {
                    arrived = arrive(&conn);
                    unread = true;
                }
                add_follower(&conn);
            }
            for at in order {
                let (directory, _) = drawn_at(drawn.files[at]);
                let label = label_of(drawn.files[at]);
                roots.insert(at, shelf.directory(&conn, &directory, "Manga", label, &[]));
                unread = true;
            }
            if !reversed {
                add_follower(&conn);
            }
            if unread || follows.is_some() {
                derive::derive(&conn)?;
                unread = false;
            }
        }
        let mut files: Vec<String> = (0..drawn.files.len())
            .map(|at| digest(&conn, &roots[&at], &drawn_at(drawn.files[at]).1))
            .chain(
                follower
                    .iter()
                    .flat_map(|(root, paths)| paths.iter().map(|path| digest(&conn, root, path))),
            )
            .collect();
        let mut named: Vec<String> = files
            .iter()
            .map(|file| work(&conn, holder(&conn, file)).0)
            .collect();
        for file in &files {
            read(&conn, file, "whole");
        }
        let mut stored: Vec<usize> = (0..drawn.pairs.len()).collect();
        if reversed {
            stored.reverse();
        }
        let follow_pair = |conn: &Connection, files: &[String]| {
            if let Some(follow) = &drawn.follow {
                pair(
                    conn,
                    &files[drawn.files.len()],
                    &files[follow.with],
                    follow.v,
                );
            }
        };
        if reversed {
            follow_pair(&conn, &files);
        }
        let mut ids = vec![0; drawn.pairs.len()];
        for at in stored {
            let (one, two, v, _) = drawn.pairs[at];
            ids[at] = pair(&conn, &files[one], &files[two], v);
        }
        if !reversed {
            follow_pair(&conn, &files);
        }
        let split: Vec<i64> = (0..ids.len())
            .filter(|at| drawn.pairs[*at].3)
            .map(|at| ids[at])
            .collect();
        if drawn.split_first && !split.is_empty() {
            matches::set_state(&conn, &split, State::Split, None)?;
        }
        derive::derive(&conn)?;
        if !drawn.split_first && !split.is_empty() {
            matches::set_state(&conn, &split, State::Split, None)?;
            derive::derive(&conn)?;
        }
        if !reversed {
            arrived = arrive(&conn);
        }
        if let Some((root, with)) = arrived {
            if !reversed {
                derive::derive(&conn)?;
            }
            let file = digest(&conn, &root, &arrival_at(drawn.files[with]).1);
            read(&conn, &file, "whole");
            files.push(file);
            named.push(named[with].clone());
        }
        (files, named)
    };
    Ok(Built {
        library,
        files,
        named,
    })
}

/// Each invariant `built` breaks, numbered as
/// [drawn_stores_keep_every_rule_of_a_join] lists them, with what broke it.
fn broken(shelf: &Shelf, template: &Library, drawn: &Drawn, built: &Built) -> Vec<(u8, String)> {
    let mut broken = Vec::new();
    let mut conn = built.library.conn();
    let file = |sha256: &str| {
        built
            .files
            .iter()
            .position(|one| one == sha256)
            .map_or_else(
                || format!("{sha256:.8}"),
                |at| match drawn.files.get(at) {
                    Some(&(name, key, _)) => format!("{}:{}", NAMES[name], KEYS[key].0),
                    None => ["follow v01", "follow v02", "arrival v02"][at - drawn.files.len()]
                        .to_owned(),
                },
            )
    };
    // A follow's volume 2 may stand in the series its volume 1's work does.
    let led: Option<(&String, String)> = drawn.follow.as_ref().and_then(|_| {
        let at = drawn.files.len();
        let [one] = holders(&conn, &built.files[at])[..] else {
            return None;
        };
        Some((&built.files[at + 1], work(&conn, one).0))
    });
    for one in &built.files {
        let works = holders(&conn, one);
        if works.len() != 1 {
            broken.push((1, format!("{} is on works {works:?}", file(one))));
        }
    }
    let rows = |sql: &str| -> Vec<(i64, String, String, String)> {
        conn.prepare(sql)
            .and_then(|mut stmt| {
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect()
            })
            .unwrap_or_else(|e| panic!("{sql}: {e}"))
    };
    for (work, key, one, two) in rows(
        "SELECT e.work_id, e.key, MIN(m.sha256), MAX(m.sha256) FROM edition e
           JOIN member m ON m.edition_id = e.id
          GROUP BY e.id HAVING COUNT(*) > 1",
    ) {
        broken.push((
            2,
            format!(
                "work {work} holds {} and {} under {key}",
                file(&one),
                file(&two)
            ),
        ));
    }
    for &(one, two, _, split) in &drawn.pairs {
        let (one, two) = (&built.files[one], &built.files[two]);
        let shared: Vec<i64> = holders(&conn, one)
            .into_iter()
            .filter(|work| holders(&conn, two).contains(work))
            .collect();
        if split && !shared.is_empty() {
            broken.push((
                3,
                format!(
                    "split {} and {} share works {shared:?}",
                    file(one),
                    file(two)
                ),
            ));
        }
    }
    for (work, key, one, two) in rows(
        "SELECT e.work_id, e.key, cm.sha_a, cm.sha_b FROM edition e
           JOIN content_match cm ON cm.id = e.joined_by",
    ) {
        for end in [&one, &two] {
            if !holders(&conn, end).contains(&work) {
                broken.push((
                    4,
                    format!(
                        "work {work}'s {key} is marked by {}-{}, and {} is not on it",
                        file(&one),
                        file(&two),
                        file(end)
                    ),
                ));
            }
        }
    }
    for (work, series, _, _) in
        rows("SELECT w.id, s.sort_title, '', '' FROM work w JOIN series s ON s.id = w.series_id")
    {
        let on: Vec<String> = conn
            .prepare(
                "SELECT m.sha256 FROM member m JOIN edition e ON e.id = m.edition_id
                  WHERE e.work_id = ?1",
            )
            .and_then(|mut stmt| stmt.query_map([work], |r| r.get(0))?.collect())
            .expect("the work's files");
        let from = on.iter().any(|one| {
            built
                .files
                .iter()
                .position(|file| file == one)
                .is_some_and(|at| built.named[at] == series)
                || led
                    .as_ref()
                    .is_some_and(|(second, into)| *second == one && *into == series)
        });
        if !from {
            let on: Vec<String> = on.iter().map(|one| file(one)).collect();
            broken.push((
                7,
                format!("work {work} is series {series} and holds {on:?}"),
            ));
        }
    }
    for (work, hub, _, _) in rows(
        "SELECT w.id, w.hub, '', '' FROM work w
          WHERE NOT EXISTS (SELECT 1 FROM edition e WHERE e.work_id = w.id AND e.key = w.hub
                                                      AND e.joined_by IS NULL)",
    ) {
        broken.push((
            8,
            format!("work {work}'s hub {hub} is no edition of its own"),
        ));
    }
    let (commits, again) = commits_during(&mut conn, derive::derive);
    if let Err(e) = again {
        broken.push((5, format!("the next derive refused: {e}")));
    } else if commits != 0 {
        broken.push((5, format!("the next derive committed {commits} times")));
    }
    match built_whole(shelf, template, drawn, true) {
        Ok(reversed) => {
            let (here, there) = (layout(&conn), layout(&reversed.library.conn()));
            if here != there {
                let differ: Vec<String> = here
                    .iter()
                    .filter(|laid| !there.contains(laid))
                    .chain(there.iter().filter(|laid| !here.contains(laid)))
                    .map(|(series, _, key, files, _)| {
                        let files: Vec<String> = files.split(',').map(file).collect();
                        format!("{series} {key} {files:?}")
                    })
                    .collect();
                broken.push((6, format!("the reversed store differs at {differ:?}")));
            }
        }
        Err(e) => broken.push((6, e)),
    }
    broken
}

/// [built], answering a panic or a refusal as an error.
fn built_whole(
    shelf: &Shelf,
    template: &Library,
    drawn: &Drawn,
    reversed: bool,
) -> Result<Built, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        built(shelf, template, drawn, reversed)
    }))
    .unwrap_or_else(|panic| {
        Err(panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_default())
    })
    .map_err(|e| format!("building the store: {e}"))
}

/// Drawn stores ([`drawn`]), each built, derived and derived again, keep
/// every rule a join must: (1) every file is on one work; (2) no work holds
/// two files under one key; (3) no split pair's files share a work; (4) each
/// mark's pair has both files on its work; (5) the next derive commits
/// nothing; (6) the store built in the opposite order lays out the same; (7)
/// every work's series is one its files' names gave; (8) every work's hub is
/// its own. [`CASES`] seeds and [`CAUGHT`] by default, a CI test's budget;
/// `TAIYAKU_JOIN_CASES` names another; a value that is not a count panics.
#[test]
fn drawn_stores_keep_every_rule_of_a_join() {
    const THREADS: u64 = 8;
    let cases: u64 = match std::env::var("TAIYAKU_JOIN_CASES") {
        Err(std::env::VarError::NotPresent) => CASES,
        Ok(value) => value.parse().unwrap_or_else(|_| {
            panic!("TAIYAKU_JOIN_CASES is {value:?}, not a whole number of cases")
        }),
        Err(other) => panic!("TAIYAKU_JOIN_CASES: {other}"),
    };
    let seeds: Vec<u64> = (0..cases)
        .chain(CAUGHT.into_iter().filter(|seed| *seed >= cases))
        .collect();
    let shelf = Shelf::new("joins-drawn");
    for name in 0..NAMES.len() {
        for key in 0..KEYS.len() {
            for kind in 0..KINDS.len() {
                let (directory, file) = drawn_at((name, key, kind));
                // Labelled by its place on the shelf, not its temporary path,
                // so a seed's digests, which break ties between pairs, are the
                // same on every run.
                write_archive_labelled(
                    &shelf.path(&directory, &file),
                    3,
                    "jpg",
                    &format!("{directory}/{file}"),
                );
                let follow = Follow {
                    name,
                    key,
                    kind,
                    with: 0,
                    v: 0.0,
                };
                let (directory, paths) = follow_at(&follow);
                let (arriving, arrival) = arrival_at((name, key, kind));
                for (directory, file) in paths
                    .iter()
                    .map(|path| (&directory, path))
                    .chain([(&arriving, &arrival)])
                {
                    write_archive_labelled(
                        &shelf.path(directory, file),
                        3,
                        "jpg",
                        &format!("{directory}/{file}"),
                    );
                }
            }
        }
    }
    let template = Library::open_in_memory().expect("open");
    let failed: std::sync::Mutex<Vec<(u64, u8, String)>> = std::sync::Mutex::default();
    std::thread::scope(|scope| {
        for thread in 0..THREADS {
            let (shelf, template, failed) = (&shelf, &template, &failed);
            let seeds = &seeds;
            scope.spawn(move || {
                let mine = seeds
                    .iter()
                    .copied()
                    .skip(usize::try_from(thread).expect("eight"));
                for seed in mine.step_by(usize::try_from(THREADS).expect("eight")) {
                    let drawn = drawn(seed);
                    let found = match built_whole(shelf, template, &drawn, false) {
                        Ok(built) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            broken(shelf, template, &drawn, &built)
                        }))
                        .unwrap_or_else(|_| vec![(0, "checking it panicked".to_owned())]),
                        Err(e) => vec![(0, e)],
                    };
                    let mut failed = failed.lock().expect("the failures");
                    failed.extend(found.into_iter().map(|(rule, what)| (seed, rule, what)));
                }
            });
        }
    });
    let mut failed = failed.into_inner().expect("the failures");
    failed.sort_by_key(|one| (one.1, one.0));
    let mut told = String::new();
    for rule in 0..=8 {
        let seeds: BTreeSet<u64> = failed.iter().filter(|f| f.1 == rule).map(|f| f.0).collect();
        if seeds.is_empty() {
            continue;
        }
        told += &format!("rule {rule}: {} seeds: {seeds:?}\n", seeds.len());
        for seed in seeds.iter().take(3) {
            for (_, _, what) in failed.iter().filter(|f| f.1 == rule && f.0 == *seed) {
                told += &format!("  seed {seed}: {what}\n");
            }
            told += &format!("  seed {seed}: {}\n", described(&drawn(*seed)));
        }
    }
    eprintln!("drew {} stores\n{told}", seeds.len());
    assert!(failed.is_empty(), "drawn stores broke a rule:\n{told}");
}
