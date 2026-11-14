use std::path::Path;

use super::*;
use crate::Library;

/// A work with one edition, `raw-mono`, and one member whose `sha256` matches
/// `FIXTURE`'s — so `apply(FIXTURE)` has real rows to attach metadata to.
fn a_work(conn: &Connection) -> i64 {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw-mono":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"3f9a","pages":300}]}},
        "hub":"raw-mono","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    crate::import::sidecar(conn, &doc, Path::new("/x/taiyaku.json")).expect("import")
}

/// A work with two editions — `raw-mono` (member `3f9a`) and `en-fan`
/// (member `eeff`) — for tests that need two of `edition.kind` or two
/// distinct edition-scoped credit lists at once, which `a_work`'s single
/// edition cannot carry.
fn a_work_with_two_editions(conn: &Connection) -> i64 {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "raw-mono":{"label":"R","members":[
            {"path":"raw/v01.cbz","sha256":"3f9a","pages":300}]},
          "en-fan":{"label":"E","members":[
            {"path":"en/v01.cbz","sha256":"eeff","pages":300}]}
        },
        "hub":"raw-mono","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    crate::import::sidecar(conn, &doc, Path::new("/x/taiyaku.json")).expect("import")
}

/// A work with one edition, `en-mono`, and a fresh, owned, migrated
/// connection to go with it.
///
/// Built through `import::sidecar`, like [`a_work`] above — so
/// `work.title_hand` and `edition.label_hand` are NULL, exactly as a plain
/// import leaves them: nothing about importing a sidecar ever touches
/// migration 010's two hand-fixed columns.
///
/// **Returns an owned `Connection` rather than borrowing one through
/// `Library::open_in_memory`.** `Library::conn()` hands back a `MutexGuard`
/// tied to the `Library`'s own lifetime, and a zero-argument helper has no
/// `Library` left alive by the time it returns one to its caller — so this
/// builds the connection directly, the same way `derive::tests` and
/// `fold::tests` do for the same reason.
fn a_work_with_one_edition() -> (Connection, i64) {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"en-mono":{"label":"R","members":[
          {"path":"en/v01.cbz","sha256":"3f9a","pages":300}]}},
        "hub":"en-mono","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id =
        crate::import::sidecar(&conn, &doc, Path::new("/x/taiyaku.json")).expect("import");
    (conn, work_id)
}

/// Members are keyed by sha256, never by path — the convention
/// `<stem>.truth.json` already uses (`core/truth.rs`, `RegionMember`), and what
/// lets a chapter number survive a file being moved or renamed.
#[test]
fn members_are_keyed_by_sha256_so_a_move_keeps_the_chapter_number() {
    let json = r#"{"taiyaku_library":1,"updated":"2026-08-14T11:03:22Z",
                   "members":[{"sha256":"3f9a","chapter":1.0,"chapter_source":"hand"}]}"#;
    let doc = LibraryDoc::from_json(json).expect("parses");
    assert_eq!(doc.members[0].sha256, "3f9a");
    assert!(doc.members[0].chapter.is_some());
}

const FIXTURE: &str = r#"{
  "taiyaku_library": 1,
  "updated": "2026-08-14T11:03:22Z",
  "series": {
    "chosen": true,
    "titles": [
      { "title": "五等分の花嫁", "kind": "native", "primary": true },
      { "title": "Go-Toubun no Hanayome", "kind": "romaji" }
    ],
    "reading": "rtl"
  },
  "volume": 1,
  "editions": {
    "raw-mono": {
      "language": "ja",
      "kind": "raw",
      "credits": [{ "role": "artist", "name": "Negi Haruba" }]
    }
  },
  "members": [{ "sha256": "3f9a", "chapter": 1.0, "chapter_source": "hand" }],
  "tags": ["romance"],
  "external": { "anilist": "97852" }
}"#;

#[test]
fn a_library_doc_round_trips() {
    let doc = LibraryDoc::from_json(FIXTURE).expect("the fixture parses");
    let back = LibraryDoc::from_json(&doc.to_json()).expect("its own output parses");
    assert_eq!(back, doc);
}

#[test]
fn the_primary_title_is_distinguishable_from_the_others() {
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");
    let primary: Vec<&str> = doc
        .series
        .titles
        .iter()
        .filter(|t| t.primary)
        .map(|t| t.title.as_str())
        .collect();
    assert_eq!(primary, vec!["五等分の花嫁"]);
}

/// `SeriesInfo::is_empty` is an `&&` with two data-losing consequences behind
/// it. The ordinary shape — titles and no reading direction, which is what a
/// series gets when nobody has stated a convention for it — would, under `&&` →
/// `||`, read as empty, which (a) drops the whole `series` block out of every
/// exported `taiyaku.library.json` via `skip_serializing_if` and (b) makes
/// `LibraryDoc::is_empty` answer `true`, which is what `migrate` reads to
/// decide whether a sibling document is worth applying and what `export::work`
/// reads to decide whether the document is worth writing at all.
///
/// The reading-only shape is asserted beside it because it is the same `&&`
/// read from the other side: a document may state a convention for a series
/// whose names it does not repeat.
#[test]
fn a_series_with_titles_and_no_reading_direction_is_not_empty() {
    let titles_only = LibraryDoc {
        taiyaku_library: VERSION,
        updated: "2026-08-17T09:00:00Z".to_string(),
        series: SeriesInfo {
            titles: vec![SeriesTitle {
                title: "Frieren".to_string(),
                kind: TitleKind::English,
                primary: true,
            }],
            chosen: false,
            reading: None,
        },
        volume: None,
        title: None,
        editions: BTreeMap::new(),
        members: Vec::new(),
        tags: Vec::new(),
        external: BTreeMap::new(),
    };

    assert!(
        !titles_only.series.is_empty(),
        "a series with a name is a series, stated reading direction or not"
    );
    assert!(
        !titles_only.is_empty(),
        "and a document that names one holds hand-fixed metadata, so `migrate` \
         must apply it and `export` must write it"
    );
    let json = titles_only.to_json();
    assert!(
        json.contains("\"series\""),
        "the block a reader hand-edited must survive the round trip out: {json}"
    );
    assert_eq!(
        LibraryDoc::from_json(&json).expect("its own output parses"),
        titles_only
    );

    let reading_only = LibraryDoc {
        series: SeriesInfo {
            titles: Vec::new(),
            chosen: false,
            reading: Some(Direction::LeftToRight),
        },
        ..titles_only
    };
    assert!(
        !reading_only.series.is_empty(),
        "and a stated convention is metadata on its own"
    );
    assert!(reading_only.to_json().contains("\"reading\""));

    assert!(
        SeriesInfo::default().is_empty(),
        "only both absent is empty"
    );
}

/// `updated` is a documented field of the interchange format. The shape is what
/// is checked — `strftime`'s own format string is the implementation, so
/// re-asserting it literally would only restate the code — plus a year in a
/// range no clock error and no placeholder falls into.
#[test]
fn of_stamps_updated_with_an_rfc3339_timestamp() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let updated = of(&conn, work_id).expect("of").updated;

    assert_eq!(
        updated.len(),
        20,
        "`YYYY-MM-DDTHH:MM:SSZ` is twenty characters: {updated}"
    );
    let (date, time) = updated.split_once('T').unwrap_or_else(|| {
        panic!("a date, a `T`, a time and a `Z`: {updated}");
    });
    assert!(time.ends_with('Z'), "UTC, and it says so: {updated}");
    let year: i32 = date[..4]
        .parse()
        .unwrap_or_else(|e| panic!("a four-digit year: {updated}: {e}"));
    assert!(
        (2026..2200).contains(&year),
        "the year this was stamped, not a placeholder: {updated}"
    );
    assert!(
        date[4..].starts_with('-') && date.len() == 10,
        "a `YYYY-MM-DD` date: {updated}"
    );
    assert!(
        time[..8].chars().all(|c| c.is_ascii_digit() || c == ':'),
        "an `HH:MM:SS` time: {updated}"
    );
}

/// `SeriesTitle.primary`'s `skip_serializing_if = "is_false"`: the point of the
/// skip is that this file is meant to be hand-edited and diffed in git — one
/// flag on the title that carries it, not a `false` on every other line.
#[test]
fn only_the_primary_title_carries_the_flag_in_the_document() {
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");
    let json = doc.to_json();

    assert_eq!(
        json.matches("\"primary\"").count(),
        1,
        "the fixture has two titles and one of them is primary: {json}"
    );
    let native = json
        .lines()
        .position(|l| l.contains("五等分の花嫁"))
        .expect("the primary title is in the document");
    let romaji = json
        .lines()
        .position(|l| l.contains("Go-Toubun"))
        .expect("the other title is too");
    let flag = json
        .lines()
        .position(|l| l.contains("\"primary\""))
        .expect("and the flag is written once");
    assert!(
        native < flag && flag < romaji,
        "the flag belongs to the title it follows, not the one after it: {json}"
    );
}

/// `apply` writes into every table this document touches: `work.volume`,
/// `series`/`series_title`, `edition.language`/`kind`, `credit`,
/// `member.chapter_declared`/`chapter_source`, `work_tag` and `external_id`.
#[test]
fn applying_the_fixture_writes_series_editions_members_tags_and_external() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");

    apply(&conn, work_id, &doc).expect("apply");

    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("volume");
    assert_eq!(volume, Some(1.0));

    let series_id: i64 = conn
        .query_row("SELECT series_id FROM work WHERE id = ?1", [work_id], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .expect("series_id")
        .expect("work has a series");

    let (sort_title, reading): (String, Option<String>) = conn
        .query_row(
            "SELECT sort_title, reading FROM series WHERE id = ?1",
            [series_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("series row");
    assert_eq!(
        sort_title, "五等分の花嫁",
        "a series a document mints takes its primary title as `sort_title`"
    );
    assert_eq!(reading.as_deref(), Some("rtl"));

    let title_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_title WHERE series_id = ?1",
            [series_id],
            |r| r.get(0),
        )
        .expect("title count");
    assert_eq!(title_count, 2);

    let (language, kind): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT language, kind FROM edition WHERE work_id = ?1 AND key = 'raw-mono'",
            [work_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("edition row");
    assert_eq!(language.as_deref(), Some("ja"));
    assert_eq!(kind.as_deref(), Some("raw"));

    let (person_name, role): (String, String) = conn
        .query_row(
            "SELECT person.name, credit.role FROM credit
             JOIN person ON person.id = credit.person_id
             JOIN edition ON edition.id = credit.edition_id
             WHERE edition.work_id = ?1",
            [work_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("credit row");
    assert_eq!(person_name, "Negi Haruba");
    assert_eq!(role, "artist");

    let (chapter, chapter_source): (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT chapter_declared, chapter_source FROM member WHERE sha256 = '3f9a'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("member row");
    assert_eq!(chapter, Some(1.0));
    assert_eq!(chapter_source.as_deref(), Some("hand"));

    let tag: String = conn
        .query_row(
            "SELECT tag.name FROM work_tag JOIN tag ON tag.id = work_tag.tag_id
             WHERE work_tag.work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("tag row");
    assert_eq!(tag, "romance");

    let external_id: String = conn
        .query_row(
            "SELECT external_id FROM external_id WHERE series_id = ?1 AND provider = 'anilist'",
            [series_id],
            |r| r.get(0),
        )
        .expect("external id row");
    assert_eq!(external_id, "97852");
}

/// Applying the same document twice must leave the database exactly as
/// applying it once did, or an export-then-import cycle accumulates
/// duplicates.
#[test]
fn applying_the_same_document_twice_is_idempotent() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");

    apply(&conn, work_id, &doc).expect("first apply");
    apply(&conn, work_id, &doc).expect("second apply");

    let series_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM series", [], |r| r.get(0))
        .expect("series count");
    assert_eq!(
        series_count, 1,
        "the second apply must not create a second series"
    );

    let series_id: i64 = conn
        .query_row("SELECT series_id FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("series_id");

    let title_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_title WHERE series_id = ?1",
            [series_id],
            |r| r.get(0),
        )
        .expect("title count");
    assert_eq!(title_count, 2, "titles must not accumulate");

    let credit_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM credit
             JOIN edition ON edition.id = credit.edition_id
             WHERE edition.work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("credit count");
    assert_eq!(credit_count, 1, "credits must not accumulate");

    let tag_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work_tag WHERE work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("tag count");
    assert_eq!(tag_count, 1, "tags must not accumulate");

    let external_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM external_id WHERE series_id = ?1",
            [series_id],
            |r| r.get(0),
        )
        .expect("external id count");
    assert_eq!(external_count, 1, "external ids must not accumulate");
}

/// Every tag currently on this work, in name order — the payload the
/// wholesale-replacement test below asserts on, rather than a count, which
/// cannot tell "replaced" from "added and something else removed".
fn tags_of(conn: &Connection, work_id: i64) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT tag.name FROM work_tag JOIN tag ON tag.id = work_tag.tag_id
             WHERE work_tag.work_id = ?1 ORDER BY tag.name",
        )
        .expect("preparing tags");
    let rows = stmt
        .query_map([work_id], |r| r.get::<_, String>(0))
        .expect("querying tags");
    rows.map(|r| r.expect("tag name")).collect()
}

/// **What `apply` clears is as much a part of it as what it writes.** Its doc
/// comment says the tables this document owns outright are "replaced
/// wholesale: metadata this document does not mention is cleared, not left
/// behind from an earlier one" — and for `work_tag` that is one
/// `DELETE FROM work_tag` in `apply_tags`.
///
/// A second document naming a *different* tag rather than none, so this
/// separates "replaced" from "cleared and then coincidentally empty": the
/// stored set must be exactly the second document's, with the first's gone.
/// Removing a tag from `<stem>.library.json` and re-importing is the ordinary
/// way a person edits their own metadata, and a tag that came back would come
/// back for good.
#[test]
fn apply_replaces_the_tag_list_rather_than_adding_to_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    apply(
        &conn,
        work_id,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("first apply");
    assert_eq!(tags_of(&conn, work_id), vec!["romance".to_string()]);

    let retagged = LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-08-15T09:00:00Z","tags":["shounen"]}"#,
    )
    .expect("parses");
    apply(&conn, work_id, &retagged).expect("second apply");

    assert_eq!(
        tags_of(&conn, work_id),
        vec!["shounen".to_string()],
        "the tag the second document does not name must be gone, not kept"
    );
}

/// The same claim for `member.chapter_declared`/`chapter_source`, cleared by
/// `apply_members`' opening `UPDATE … SET chapter_declared = NULL`.
///
/// Two editions, and the second document moves the chapter number from one
/// member to the other, so this pins the clear as *work-wide* rather than
/// "the row this document happens to write next". A stale chapter number left
/// on a member is a wrong chapter readout, from data a person hand-entered
/// and then hand-removed.
#[test]
fn apply_clears_a_member_chapter_the_document_no_longer_names() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work_with_two_editions(&conn);

    let first = LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-08-14T11:03:22Z",
            "members":[{"sha256":"3f9a","chapter":1.0,"chapter_source":"hand"}]}"#,
    )
    .expect("parses");
    apply(&conn, work_id, &first).expect("first apply");

    let moved = LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-08-15T09:00:00Z",
            "members":[{"sha256":"eeff","chapter":2.0,"chapter_source":"hand"}]}"#,
    )
    .expect("parses");
    apply(&conn, work_id, &moved).expect("second apply");

    let chapter_of = |sha: &str| -> (Option<f64>, Option<String>) {
        conn.query_row(
            "SELECT chapter_declared, chapter_source FROM member WHERE sha256 = ?1",
            [sha],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("member row")
    };
    assert_eq!(
        chapter_of("3f9a"),
        (None, None),
        "the member the second document does not name must be cleared"
    );
    assert_eq!(
        chapter_of("eeff"),
        (Some(2.0), Some("hand".to_string())),
        "and the one it does name must carry its chapter"
    );
}

/// The hazard: `series_title`'s primary key is the composite
/// `(series_id, title)`, not a rowid alias, so `INSERT OR REPLACE` against it
/// resolves a conflict by deleting and re-inserting with a fresh rowid. That
/// implicit delete only reaches the trigger keeping `series_fts` current when
/// `PRAGMA recursive_triggers` is on — which `Library::open_in_memory` sets.
/// A second identical `apply` must not leave a phantom row behind in the
/// search index.
#[test]
fn reapplying_the_same_series_titles_does_not_leave_phantom_fts_rows() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");

    apply(&conn, work_id, &doc).expect("first apply");
    apply(&conn, work_id, &doc).expect("second apply");

    let hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_fts WHERE series_fts MATCH 'Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("fts query");
    assert_eq!(
        hits, 1,
        "a title present once in series_title must be found once in series_fts, \
         not duplicated by the REPLACE-driven implicit delete"
    );
}

/// How many `series` rows, `series_title` rows and `series_fts` hits for
/// `title` the library holds — the three places one shelf lives, asked
/// together because the orphan below leaves all three behind at once.
fn series_rows(conn: &Connection, title: &str) -> (i64, i64, i64) {
    let count = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> i64 {
        conn.query_row(sql, params, |r| r.get(0))
            .unwrap_or_else(|e| panic!("{sql}: {e}"))
    };
    (
        count("SELECT COUNT(*) FROM series", &[]),
        count("SELECT COUNT(*) FROM series_title", &[]),
        count(
            "SELECT COUNT(*) FROM series_fts WHERE series_fts MATCH ?1",
            &[&title],
        ),
    )
}

/// Search (spec C1) reads `series_fts`, so a series no work is under would
/// keep answering queries.
///
/// The `series_fts` hit is the assertion that matters: `series_title` cascades
/// from `series` on `ON DELETE CASCADE`, and that cascaded delete reaches the
/// `series_title_ad` trigger on its own — SQLite fires a table's ordinary
/// `DELETE` triggers for a foreign-key cascade the same as for an explicit
/// `DELETE` against it, with no pragma involved. **This is not the test that
/// would notice `recursive_triggers` going away**: the tests that would
/// notice are `reapplying_the_same_series_titles_does_not_leave_phantom_fts_rows`
/// and `replacing_a_series_title_leaves_no_phantom_in_the_search_index`
/// (`lib.rs`'s own comment on the pragma explains why: it gates the
/// *implicit* delete `INSERT OR REPLACE` performs to resolve a conflict,
/// which is what `apply_series` relies on and this test's `apply` does not).
#[test]
fn the_last_work_leaving_a_series_takes_the_shelf_with_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    apply(
        &conn,
        work_id,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("the work joins a series");
    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (1, 2, 1),
        "the fixture must really shelve it, or this test proves nothing"
    );

    // The reader edits the sibling document down to no series at all.
    let mut cleared = LibraryDoc::from_json(FIXTURE).expect("parses");
    cleared.series = SeriesInfo::default();
    cleared.external.clear();
    apply(&conn, work_id, &cleared).expect("the work leaves the series");

    assert_eq!(
        series_of(&conn, work_id).expect("reads"),
        None,
        "the work is off the shelf"
    );
    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (0, 0, 0),
        "and the shelf goes with it — row, titles and search index"
    );
    let external: i64 = conn
        .query_row("SELECT COUNT(*) FROM external_id", [], |r| r.get(0))
        .expect("counting external ids");
    assert_eq!(external, 0, "and the provider ids keyed on it");
}

/// The other half, and the reason this is a check rather than an unconditional
/// delete: a series is shared. One work leaving a two-work series must leave
/// the shelf standing, titles and search index intact, or one work's document
/// would erase a sibling work's own contribution — the exact loss `apply`'s
/// upsert discipline exists to prevent.
#[test]
fn a_series_another_work_is_still_under_survives_one_work_leaving() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = a_work(&conn);
    let second = {
        let json = r#"{"taiyaku":1,"work":{"title":"T2","reading":"rtl"},
            "editions":{"raw-mono":{"label":"R","members":[
              {"path":"raw/v02.cbz","sha256":"4b0c","pages":300}]}},
            "hub":"raw-mono","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, Path::new("/x/v02/taiyaku.json")).expect("import")
    };
    let shelved = LibraryDoc::from_json(FIXTURE).expect("parses");
    apply(&conn, first, &shelved).expect("volume 1 joins");
    let mut second_doc = shelved.clone();
    second_doc.editions.clear();
    second_doc.members.clear();
    apply(&conn, second, &second_doc).expect("volume 2 joins the same series");

    let mut cleared = shelved.clone();
    cleared.series = SeriesInfo::default();
    cleared.external.clear();
    cleared.editions.clear();
    cleared.members.clear();
    apply(&conn, first, &cleared).expect("volume 1 leaves");

    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (1, 2, 1),
        "volume 2 is still under it, so nothing here is anyone's to delete"
    );
    assert!(
        series_of(&conn, second).expect("reads").is_some(),
        "and volume 2 is still on it"
    );
}

/// The same orphan reached by renaming rather than clearing: a document that
/// names a different series resolves to a different `series` row, and the one
/// this work just left is as empty as the cleared case if nothing else was in
/// it. `apply_series` returns early on the empty-titles branch, so this path
/// needs its own check and its own test.
#[test]
fn renaming_a_works_only_series_does_not_leave_the_old_one_behind() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    apply(
        &conn,
        work_id,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("the work joins a series");

    let mut renamed = LibraryDoc::from_json(FIXTURE).expect("parses");
    renamed.series.titles = vec![SeriesTitle {
        title: "Quintessential Quintuplets".to_string(),
        kind: TitleKind::English,
        primary: true,
    }];
    apply(&conn, work_id, &renamed).expect("the reader retitles the shelf");

    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (1, 1, 0),
        "one series, holding only the new title, and the old one is out of the index"
    );
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_fts WHERE series_fts MATCH 'Quintuplets'",
            [],
            |r| r.get(0),
        )
        .expect("fts query");
    assert_eq!(found, 1, "the new title is the one that answers now");
}

/// A PATCH that corrects a series' only title with nothing left to match:
/// [`set_series_titles`] writes the id already in hand instead of resolving one
/// by text.
#[test]
fn set_series_titles_keeps_the_same_series_id_even_when_no_old_title_matches() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    apply(
        &conn,
        work_id,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("the work joins a series");
    let series_id = series_of(&conn, work_id).expect("reads").expect("shelved");

    let corrected = vec![SeriesTitle {
        title: "Quintessential Quintuplets".to_string(),
        kind: TitleKind::English,
        primary: true,
    }];
    set_series_titles(&conn, series_id, &corrected).expect("replaces the title set");

    assert_eq!(
        series_of(&conn, work_id).expect("reads"),
        Some(series_id),
        "the id already in hand is the series edited — no new one minted, \
         this one not deleted out from under the work still shelved on it"
    );
    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (1, 1, 0),
        "one series row, holding only the new title, and the old text is \
         out of the search index"
    );
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_fts WHERE series_fts MATCH 'Quintuplets'",
            [],
            |r| r.get(0),
        )
        .expect("fts query");
    assert_eq!(found, 1, "the corrected title is the one that answers now");
}

/// Sending the full title array with one entry corrected: [`set_series_titles`]
/// deletes the whole title set for this id before writing the new one, so a
/// spelling the new set drops is actually gone.
#[test]
fn set_series_titles_removes_a_spelling_the_new_set_drops() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let mut misspelled = LibraryDoc::from_json(FIXTURE).expect("parses");
    misspelled.series.titles = vec![
        SeriesTitle {
            title: "五等分の花嫁".to_string(),
            kind: TitleKind::Native,
            primary: true,
        },
        SeriesTitle {
            title: "Gotoubun no Hanayome".to_string(),
            kind: TitleKind::Romaji,
            primary: false,
        },
    ];
    apply(&conn, work_id, &misspelled).expect("the work joins a misspelled series");
    let series_id = series_of(&conn, work_id).expect("reads").expect("shelved");

    let corrected = vec![
        SeriesTitle {
            title: "五等分の花嫁".to_string(),
            kind: TitleKind::Native,
            primary: true,
        },
        SeriesTitle {
            title: "Go-Toubun no Hanayome".to_string(),
            kind: TitleKind::Romaji,
            primary: false,
        },
    ];
    set_series_titles(&conn, series_id, &corrected).expect("corrects the spelling");

    assert_eq!(
        series_of(&conn, work_id).expect("reads"),
        Some(series_id),
        "membership is untouched"
    );
    let title_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_title WHERE series_id = ?1",
            [series_id],
            |r| r.get(0),
        )
        .expect("counting titles");
    assert_eq!(title_count, 2, "two titles in, two titles out — not three");

    let stale_hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_fts WHERE series_fts MATCH 'Gotoubun'",
            [],
            |r| r.get(0),
        )
        .expect("fts query");
    assert_eq!(
        stale_hits, 0,
        "the misspelling must not still answer a search alongside its correction"
    );
}

/// A series left with no title at all is the membership-clearing,
/// maybe-delete-the-series outcome `apply_series`'s own empty-titles branch
/// exists to produce when a work leaves its series — not a thing
/// [`set_series_titles`]'s caller ever asks for, and not reachable through
/// it: it must refuse rather than silently emptying the row set.
#[test]
fn set_series_titles_refuses_an_empty_title_list() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    apply(
        &conn,
        work_id,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("the work joins a series");
    let series_id = series_of(&conn, work_id).expect("reads").expect("shelved");

    let err = set_series_titles(&conn, series_id, &[]).expect_err("an empty title list is refused");
    assert!(
        err.contains("title"),
        "the refusal names what was wrong: {err}"
    );

    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (1, 2, 1),
        "refused before anything was touched"
    );
}

/// `browse.rs`'s own `MAX(CASE WHEN st.is_primary = 1 THEN st.title END)` picks
/// whichever of more than one `is_primary` row sorts lexically greater — the
/// symptom of a corrected spelling landing beside, not instead of, the one it
/// replaced, by a different door. `set_series_titles` is the natural place to
/// refuse it.
#[test]
fn set_series_titles_refuses_more_than_one_primary_title() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    apply(
        &conn,
        work_id,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("the work joins a series");
    let series_id = series_of(&conn, work_id).expect("reads").expect("shelved");

    let two_primaries = vec![
        SeriesTitle {
            title: "五等分の花嫁".to_string(),
            kind: TitleKind::Native,
            primary: true,
        },
        SeriesTitle {
            title: "Go-Toubun no Hanayome".to_string(),
            kind: TitleKind::Romaji,
            primary: true,
        },
    ];
    let err = set_series_titles(&conn, series_id, &two_primaries)
        .expect_err("more than one primary title is refused");
    assert!(
        err.contains("primary"),
        "the refusal names what was wrong: {err}"
    );

    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (1, 2, 1),
        "refused before anything was touched"
    );
}

/// [`set_series_titles`]'s refusal; [`apply_with_series_id`]'s doc has the
/// hazard it guards against.
#[test]
fn set_series_titles_refuses_a_title_text_another_series_already_holds() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let work_a = a_work(&conn);
    apply(
        &conn,
        work_a,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("work a joins its series");
    let series_a = series_of(&conn, work_a).expect("reads").expect("shelved");

    let work_b = {
        let json = r#"{"taiyaku":1,"work":{"title":"T2","reading":"rtl"},
            "editions":{"raw-mono":{"label":"R","members":[
              {"path":"raw/v02.cbz","sha256":"4b0c","pages":300}]}},
            "hub":"raw-mono","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, Path::new("/x/v02/taiyaku.json")).expect("import")
    };
    let mut unrelated = LibraryDoc::from_json(FIXTURE).expect("parses");
    unrelated.series.titles = vec![SeriesTitle {
        title: "AAA Unrelated Series".to_string(),
        kind: TitleKind::Romaji,
        primary: true,
    }];
    unrelated.external.clear();
    apply(&conn, work_b, &unrelated).expect("work b joins an unrelated series");
    let series_b = series_of(&conn, work_b).expect("reads").expect("shelved");
    assert_ne!(
        series_a, series_b,
        "two distinct series, or this test proves nothing"
    );

    let colliding = vec![SeriesTitle {
        title: "AAA Unrelated Series".to_string(),
        kind: TitleKind::Romaji,
        primary: true,
    }];
    let err = set_series_titles(&conn, series_a, &colliding)
        .expect_err("a title text series b already holds must be refused, not written");
    assert!(
        err.contains("AAA Unrelated Series") && err.contains(&series_b.to_string()),
        "the refusal should name the colliding text and the series that \
         already holds it, so a caller can tell this refusal apart from any \
         other: {err}"
    );

    let series_a_title_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_title WHERE series_id = ?1",
            [series_a],
            |r| r.get(0),
        )
        .expect("counting series a's own titles");
    assert_eq!(
        series_a_title_count, 2,
        "refused before series a's own title set was touched at all"
    );
    let series_a_sort_title: String = conn
        .query_row(
            "SELECT sort_title FROM series WHERE id = ?1",
            [series_a],
            |r| r.get(0),
        )
        .expect("reading series a's own sort_title");
    assert_eq!(
        series_a_sort_title, "五等分の花嫁",
        "a refused edit must not have touched series a's own sort_title either"
    );
    let series_b_title_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM series_title WHERE series_id = ?1",
            [series_b],
            |r| r.get(0),
        )
        .expect("counting series b's own titles");
    assert_eq!(
        series_b_title_count, 1,
        "series b was never the target of this edit and must be untouched"
    );
}

/// This drives [`apply_with_series_id`] directly, bypassing
/// [`set_series_titles`] entirely, so the collision exists in the database
/// regardless of whether the refusal above is doing its job: this function
/// and that refusal must each hold on their own, and mutating away either one
/// alone must fail
/// a test. `metadata::tests::patching_a_series_title_to_text_another_series_already_holds_does_not_delete_it`
/// (`taiyaku-serve`) is the route-level reproduction and cannot serve this
/// role, because the refusal aborts that request before its own loop ever
/// reaches `apply_with_series_id` at all.
#[test]
fn apply_with_series_id_does_not_resolve_by_title_even_when_the_text_collides() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let work_a = a_work(&conn);
    apply(
        &conn,
        work_a,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("work a joins its series");
    let series_a = series_of(&conn, work_a).expect("reads").expect("shelved");

    let work_b = {
        let json = r#"{"taiyaku":1,"work":{"title":"T2","reading":"rtl"},
            "editions":{"raw-mono":{"label":"R","members":[
              {"path":"raw/v02.cbz","sha256":"4b0c","pages":300}]}},
            "hub":"raw-mono","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, Path::new("/x/v02/taiyaku.json")).expect("import")
    };
    let mut unrelated = LibraryDoc::from_json(FIXTURE).expect("parses");
    unrelated.series.titles = vec![SeriesTitle {
        title: "AAA Unrelated Series".to_string(),
        kind: TitleKind::Romaji,
        primary: true,
    }];
    unrelated.external.clear();
    apply(&conn, work_b, &unrelated).expect("work b joins an unrelated series");
    let series_b = series_of(&conn, work_b).expect("reads").expect("shelved");
    assert_ne!(
        series_a, series_b,
        "two distinct series, or this test proves nothing"
    );

    // The exact hazard: work a's own document names, verbatim, the title
    // text series b already holds — regardless of whether anything upstream
    // would ever have refused writing it there.
    let mut colliding = LibraryDoc::from_json(FIXTURE).expect("parses");
    colliding.series.titles = vec![SeriesTitle {
        title: "AAA Unrelated Series".to_string(),
        kind: TitleKind::Romaji,
        primary: true,
    }];
    apply_with_series_id(&conn, work_a, Some(series_a), &colliding)
        .expect("applies without resolving series identity");

    assert_eq!(
        series_of(&conn, work_a).expect("reads"),
        Some(series_a),
        "work a must stay filed under the series it was already on, not \
         migrate onto series b because its document's title text collided"
    );
    let series_a_still_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE id = ?1)",
            [series_a],
            |r| r.get(0),
        )
        .expect("checking series a");
    assert!(
        series_a_still_exists,
        "series a must not have been deleted as an orphan"
    );
    assert_eq!(
        series_of(&conn, work_b).expect("reads"),
        Some(series_b),
        "series b, and work b's membership in it, must be untouched"
    );
}

/// **A no-op `apply_with_series_id` still passes the test above.** That test
/// only reads back `work_a`'s series membership, which a version of this
/// function that wrote nothing at all would also leave untouched — so it proves
/// the refusal to resolve by title without proving the function applies the
/// document it was handed. This drives it with `series_id: None` — the shape
/// this function's own doc comment says still writes everything but series
/// identity and the reading direction — and reads back the same fields
/// `applying_the_fixture_writes_series_editions_members_tags_and_external`
/// checks for [`apply`]: `work.volume`, `edition.language`/`kind`,
/// `member.chapter_declared`/`chapter_source`, and `work_tag`. `doc.external`
/// is cleared first, the same way the collision test above clears it, because
/// an external id with no series to attach to is refused rather than silently
/// dropped, and this test passes `None`.
#[test]
fn apply_with_series_id_writes_editions_members_and_tags_like_apply_does() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let mut doc = LibraryDoc::from_json(FIXTURE).expect("parses");
    doc.external.clear();

    apply_with_series_id(&conn, work_id, None, &doc).expect("apply_with_series_id");

    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("volume");
    assert_eq!(
        volume,
        Some(1.0),
        "work.volume must be written, not left at its pre-apply default"
    );

    let (language, kind): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT language, kind FROM edition WHERE work_id = ?1 AND key = 'raw-mono'",
            [work_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("edition row");
    assert_eq!(language.as_deref(), Some("ja"));
    assert_eq!(kind.as_deref(), Some("raw"));

    let (chapter, chapter_source): (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT chapter_declared, chapter_source FROM member WHERE sha256 = '3f9a'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("member row");
    assert_eq!(chapter, Some(1.0));
    assert_eq!(chapter_source.as_deref(), Some("hand"));

    let tag: String = conn
        .query_row(
            "SELECT tag.name FROM work_tag JOIN tag ON tag.id = work_tag.tag_id
             WHERE work_tag.work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("tag row");
    assert_eq!(tag, "romance");
}

/// `doc.editions` names an edition key this work does not have — a document
/// written for a different sidecar, or one that has gone stale since. This
/// must not silently write nothing; the mismatch is worth refusing.
#[test]
fn applying_metadata_for_an_unknown_edition_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let mut doc = LibraryDoc::from_json(FIXTURE).expect("parses");
    let meta = doc.editions.remove("raw-mono").expect("fixture has it");
    doc.editions.insert("en-fan".to_string(), meta);

    let result = apply(&conn, work_id, &doc);
    assert!(
        result.is_err(),
        "an edition key the work does not have must be refused, not silently ignored"
    );
}

/// `external_id` has no `work_id` column — it is keyed by series alone. A
/// document with external ids but no series titles has nowhere for them to
/// go, and that inconsistency must be refused rather than silently dropped.
#[test]
fn applying_external_ids_with_no_series_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = LibraryDoc {
        taiyaku_library: VERSION,
        updated: "2026-08-14T11:03:22Z".to_string(),
        series: SeriesInfo::default(),
        volume: None,
        title: None,
        editions: BTreeMap::new(),
        members: Vec::new(),
        tags: Vec::new(),
        external: BTreeMap::from([("anilist".to_string(), "1".to_string())]),
    };

    let result = apply(&conn, work_id, &doc);
    assert!(result.is_err());
}

/// A `sha256` naming content this work does not hold is not an error: the
/// content it names may simply be gone, and there is nothing to attach a
/// stale chapter number to.
#[test]
fn a_chapter_number_for_content_not_present_is_a_silent_no_op() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = LibraryDoc {
        taiyaku_library: VERSION,
        updated: "2026-08-14T11:03:22Z".to_string(),
        series: SeriesInfo::default(),
        volume: None,
        title: None,
        editions: BTreeMap::new(),
        members: vec![MemberMeta {
            sha256: "not-a-real-member".to_string(),
            chapter: Some(4.0),
            chapter_source: Some(ChapterSource::Hand),
        }],
        tags: Vec::new(),
        external: BTreeMap::new(),
    };

    apply(&conn, work_id, &doc).expect("apply must not fail on unmatched content");
}

/// A second `apply` that drops an edition entry present in the first must
/// clear that edition's previously hand-set metadata, not leave it stale —
/// the same wholesale-replace rule `state::put_notes` and `state::put_truth`
/// apply to their own documents.
#[test]
fn a_second_apply_without_an_edition_entry_clears_its_previous_metadata() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let first = LibraryDoc::from_json(FIXTURE).expect("parses");
    apply(&conn, work_id, &first).expect("first apply");

    let mut second = first.clone();
    second.editions.clear();
    apply(&conn, work_id, &second).expect("second apply");

    let (language, kind): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT language, kind FROM edition WHERE work_id = ?1 AND key = 'raw-mono'",
            [work_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("edition row");
    assert_eq!(
        language, None,
        "language must be cleared, not left from the first apply"
    );
    assert_eq!(
        kind, None,
        "kind must be cleared, not left from the first apply"
    );

    let credit_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM credit
             JOIN edition ON edition.id = credit.edition_id
             WHERE edition.work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("credit count");
    assert_eq!(
        credit_count, 0,
        "the credit must be cleared along with the edition entry"
    );
}

/// `of` is `apply`'s inverse: everything the fixture set is projected back,
/// except `updated`, which is stamped fresh rather than stored — see `of`'s
/// doc comment.
#[test]
fn of_reprojects_what_apply_wrote() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");
    apply(&conn, work_id, &doc).expect("apply");

    let back = of(&conn, work_id).expect("of");

    assert_eq!(back.taiyaku_library, VERSION);
    assert_eq!(back.volume, doc.volume);
    assert_eq!(back.series.reading, doc.series.reading);
    let mut back_titles = back.series.titles.clone();
    let mut doc_titles = doc.series.titles.clone();
    back_titles.sort_by(|a, b| a.title.cmp(&b.title));
    doc_titles.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(back_titles, doc_titles);
    assert_eq!(back.editions, doc.editions);
    assert_eq!(back.members, doc.members);
    assert_eq!(back.tags, doc.tags);
    assert_eq!(back.external, doc.external);
}

/// A freshly imported work has no hand-fixed metadata at all: no series, no
/// volume, no annotated editions, no members, no tags, no external ids.
#[test]
fn of_is_empty_for_a_work_nothing_has_been_applied_to() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let doc = of(&conn, work_id).expect("of");

    assert_eq!(doc.series, SeriesInfo::default());
    assert_eq!(doc.volume, None);
    assert!(
        doc.editions.is_empty(),
        "no edition has been hand-annotated yet"
    );
    assert!(doc.members.is_empty());
    assert!(doc.tags.is_empty());
    assert!(doc.external.is_empty());
}

/// The closed-set variants `FIXTURE` never exercises: `ltr`, `english` and
/// `alt` titles, `official` and `fan` editions, the `author`/`translator`/
/// `group`/`publisher` credit roles, and the `comicinfo`/`filename` chapter
/// sources. Each is a real value `apply`/`of` must carry faithfully, not
/// filler — a document naming a translator or a fan edition is ordinary.
#[test]
fn every_remaining_closed_set_variant_round_trips_through_the_database() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work_with_two_editions(&conn);

    let doc = LibraryDoc {
        taiyaku_library: VERSION,
        updated: "2026-08-14T11:03:22Z".to_string(),
        series: SeriesInfo {
            titles: vec![
                SeriesTitle {
                    title: "Native Name".to_string(),
                    kind: TitleKind::Native,
                    primary: true,
                },
                SeriesTitle {
                    title: "English Name".to_string(),
                    kind: TitleKind::English,
                    primary: false,
                },
                SeriesTitle {
                    title: "Alt Name".to_string(),
                    kind: TitleKind::Alt,
                    primary: false,
                },
            ],
            chosen: false,
            reading: Some(Direction::LeftToRight),
        },
        volume: None,
        title: None,
        editions: BTreeMap::from([
            (
                "raw-mono".to_string(),
                EditionMeta {
                    label: None,
                    language: None,
                    kind: Some(EditionKind::Official),
                    credits: vec![
                        Credit {
                            role: CreditRole::Author,
                            name: "A".to_string(),
                        },
                        Credit {
                            role: CreditRole::Translator,
                            name: "B".to_string(),
                        },
                    ],
                },
            ),
            (
                "en-fan".to_string(),
                EditionMeta {
                    label: None,
                    language: None,
                    kind: Some(EditionKind::Fan),
                    credits: vec![
                        Credit {
                            role: CreditRole::Group,
                            name: "C".to_string(),
                        },
                        Credit {
                            role: CreditRole::Publisher,
                            name: "D".to_string(),
                        },
                    ],
                },
            ),
        ]),
        members: vec![
            MemberMeta {
                sha256: "3f9a".to_string(),
                chapter: Some(1.0),
                chapter_source: Some(ChapterSource::Comicinfo),
            },
            MemberMeta {
                sha256: "eeff".to_string(),
                chapter: Some(1.0),
                chapter_source: Some(ChapterSource::Filename),
            },
        ],
        tags: Vec::new(),
        external: BTreeMap::new(),
    };

    apply(&conn, work_id, &doc).expect("apply");
    let back = of(&conn, work_id).expect("of");

    assert_eq!(back.series.reading, Some(Direction::LeftToRight));
    let kinds: Vec<TitleKind> = back.series.titles.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&TitleKind::English));
    assert!(kinds.contains(&TitleKind::Alt));

    assert_eq!(back.editions["raw-mono"].kind, Some(EditionKind::Official));
    assert_eq!(back.editions["en-fan"].kind, Some(EditionKind::Fan));

    let raw_roles: Vec<CreditRole> = back.editions["raw-mono"]
        .credits
        .iter()
        .map(|c| c.role)
        .collect();
    assert!(raw_roles.contains(&CreditRole::Author));
    assert!(raw_roles.contains(&CreditRole::Translator));
    let fan_roles: Vec<CreditRole> = back.editions["en-fan"]
        .credits
        .iter()
        .map(|c| c.role)
        .collect();
    assert!(fan_roles.contains(&CreditRole::Group));
    assert!(fan_roles.contains(&CreditRole::Publisher));

    let sources: Vec<Option<ChapterSource>> =
        back.members.iter().map(|m| m.chapter_source).collect();
    assert!(sources.contains(&Some(ChapterSource::Comicinfo)));
    assert!(sources.contains(&Some(ChapterSource::Filename)));
}

/// Every closed set this module reads back out of text columns must refuse a
/// value none of its own writers ever puts there — the same rule
/// `state::region_from_columns` polices for a note's region, applied here to
/// a hand edit that reaches `series.reading`.
#[test]
fn an_invalid_stored_reading_direction_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        // `reading_hand`: `of` projects provenance, not the effective
        // column (O80, migration 011).
        "INSERT INTO series (sort_title, reading, reading_hand)
         VALUES ('x', 'diagonal', 'diagonal')",
        [],
    )
    .expect("series");
    let series_id = conn.last_insert_rowid();
    conn.execute(
        // `series_id_hand` too (O80, migration 011).
        "UPDATE work SET series_id_hand = ?1, series_id = ?1 WHERE id = ?2",
        params![series_id, work_id],
    )
    .expect("attach");

    assert!(
        of(&conn, work_id).is_err(),
        "an unrecognised reading direction must not be silently accepted"
    );
}

/// The `series_title.kind` analogue of `an_invalid_stored_reading_direction_is_refused`.
#[test]
fn an_invalid_stored_series_title_kind_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute("INSERT INTO series (sort_title) VALUES ('x')", [])
        .expect("series");
    let series_id = conn.last_insert_rowid();
    conn.execute(
        // `series_id_hand` too (O80, migration 011).
        "UPDATE work SET series_id_hand = ?1, series_id = ?1 WHERE id = ?2",
        params![series_id, work_id],
    )
    .expect("attach");
    conn.execute(
        // `hand = 1`: `of` projects only a person's titles (O80).
        "INSERT INTO series_title (series_id, title, kind, hand)
         VALUES (?1, 'T', 'mystery', 1)",
        params![series_id],
    )
    .expect("title");

    assert!(of(&conn, work_id).is_err());
}

/// The `edition.kind` analogue.
#[test]
fn an_invalid_stored_edition_kind_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        // `kind_hand`: `of` projects provenance, not the effective column
        // (O80, migration 011).
        "UPDATE edition SET kind_hand = 'bootleg' WHERE work_id = ?1",
        [work_id],
    )
    .expect("edition");

    assert!(of(&conn, work_id).is_err());
}

/// The `credit.role` analogue.
#[test]
fn an_invalid_stored_credit_role_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let edition_id: i64 = conn
        .query_row(
            "SELECT id FROM edition WHERE work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("edition");
    conn.execute("INSERT INTO person (name) VALUES ('X')", [])
        .expect("person");
    let person_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO credit (person_id, role, edition_id) VALUES (?1, 'mascot', ?2)",
        params![person_id, edition_id],
    )
    .expect("credit");

    assert!(of(&conn, work_id).is_err());
}

/// The `member.chapter_source` analogue.
#[test]
fn an_invalid_stored_chapter_source_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        "UPDATE member SET chapter_source = 'vibes' WHERE sha256 = '3f9a'",
        [],
    )
    .expect("member");

    assert!(of(&conn, work_id).is_err());
}

/// A plain `UPDATE ... WHERE id = ?` affects zero rows for a `work_id` that
/// does not exist rather than failing — `apply` must not let that read as
/// silent success on a document nobody could actually attach to anything.
#[test]
fn applying_to_a_work_id_that_does_not_exist_is_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");

    let refused = apply(&conn, 404, &doc)
        .expect_err("apply must refuse a work_id that names no work, not silently no-op");

    // **The row-count guard's own message, not merely an `Err`.** Without the
    // guard this fixture still fails later, with a different message, so
    // asserting `is_err()` alone would not show the guard is there.
    assert!(
        refused.contains("work 404 does not exist"),
        "the refusal must be the row-count guard's, not a later failure that \
         happens to also be an Err: {refused}"
    );
}

/// **The credit list comes back in the order it was written, not in any order
/// the columns happen to suggest.** `credits_of` orders by `credit.id`, and
/// `apply_editions` inserts a document's credits in document order, so the two
/// together make the list a *sequence* rather than a set — which is what a
/// credit list is on a book's own colophon: the author is named first because
/// somebody decided so, not because the name sorts first.
///
/// Three credits here, and deliberately adversarial: the names descend
/// alphabetically and the roles are in neither the document order nor
/// `CreditRole`'s declaration order, so `ORDER BY person.name`,
/// `ORDER BY credit.role` and `ORDER BY credit.id DESC` each produce a
/// different list from the one asserted.
#[test]
fn a_multi_credit_edition_keeps_the_order_the_document_gave() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);

    let json = r#"{"taiyaku_library":1,"updated":"2026-08-14T11:03:22Z",
                   "editions":{"raw-mono":{"credits":[
                     {"role":"translator","name":"Yuri"},
                     {"role":"author","name":"Miki"},
                     {"role":"artist","name":"Aoi"}]}}}"#;
    let doc = LibraryDoc::from_json(json).expect("parses");
    apply(&conn, work_id, &doc).expect("apply");

    let back = of(&conn, work_id).expect("project");
    let credits = &back.editions["raw-mono"].credits;
    assert_eq!(
        credits,
        &vec![
            Credit {
                role: CreditRole::Translator,
                name: "Yuri".to_string()
            },
            Credit {
                role: CreditRole::Author,
                name: "Miki".to_string()
            },
            Credit {
                role: CreditRole::Artist,
                name: "Aoi".to_string()
            },
        ],
        "the projection must reproduce the document's own order"
    );
}

/// `DELETE FROM series` cascades into tables this document does not own, and
/// one of them is not derivable from anything: `collection_item`. A collection
/// spans works, has no sidecar to sit beside, and is covered by backup rather
/// than by export (this module's own doc comment says so) — so a shelf dropped
/// out of a reading list because the last *work* left the series is a loss no
/// sibling document can restore.
///
/// Nothing writes `collection_item` today, which is why this test writes the
/// row by hand: the guard has to be right before the writer exists, because
/// the moment spec C1's collections land the failure is silent and permanent.
/// The other three cascades are correct as they stand — `series_title`,
/// `series_tag` and `external_id` are all keyed on a series that, once no work
/// and no collection reaches it, nothing can reach at all.
#[test]
fn a_series_a_collection_still_lists_survives_its_last_work_leaving() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    apply(
        &conn,
        work_id,
        &LibraryDoc::from_json(FIXTURE).expect("parses"),
    )
    .expect("the work joins a series");
    let series_id = series_of(&conn, work_id)
        .expect("reads")
        .expect("the fixture shelves it");

    conn.execute(
        "INSERT INTO collection (id, name) VALUES (1, 'Reading now')",
        [],
    )
    .expect("a collection");
    conn.execute(
        "INSERT INTO collection_item (collection_id, ordinal, series_id) VALUES (1, 0, ?1)",
        [series_id],
    )
    .expect("the collection lists this series");

    let mut cleared = LibraryDoc::from_json(FIXTURE).expect("parses");
    cleared.series = SeriesInfo::default();
    cleared.external.clear();
    apply(&conn, work_id, &cleared).expect("the work leaves the series");

    assert_eq!(
        series_of(&conn, work_id).expect("reads"),
        None,
        "the work is off the shelf, which is what the document asked for"
    );
    assert_eq!(
        series_rows(&conn, "Hanayome"),
        (1, 2, 1),
        "but the shelf itself stays: a collection still lists it, and nothing \
         about one work's metadata edit is a request to empty a reading list"
    );
    let listed: i64 = conn
        .query_row("SELECT COUNT(*) FROM collection_item", [], |r| r.get(0))
        .expect("counting collection items");
    assert_eq!(listed, 1, "and the reading list still holds its entry");
}

#[test]
fn a_hand_set_title_and_label_round_trip_through_the_document() {
    let (conn, work_id) = a_work_with_one_edition();

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.title = Some("Go-Toubun no Hanayome, volume 1".to_string());
    doc.editions.insert(
        "en-mono".to_string(),
        super::EditionMeta {
            label: Some("English, monochrome".to_string()),
            ..Default::default()
        },
    );
    super::apply(&conn, work_id, &doc).expect("applying");

    let effective: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading work.title");
    assert_eq!(
        effective, "Go-Toubun no Hanayome, volume 1",
        "apply writes the effective column, not only the provenance one"
    );

    let label: String = conn
        .query_row(
            "SELECT label FROM edition WHERE work_id = ?1 AND key = 'en-mono'",
            [work_id],
            |r| r.get(0),
        )
        .expect("reading edition.label");
    assert_eq!(label, "English, monochrome");

    let again = super::of(&conn, work_id).expect("re-projecting");
    assert_eq!(
        again.title.as_deref(),
        Some("Go-Toubun no Hanayome, volume 1")
    );
    assert_eq!(
        again.editions["en-mono"].label.as_deref(),
        Some("English, monochrome")
    );
}

#[test]
fn a_derived_title_and_label_leave_the_document_empty() {
    let (conn, work_id) = a_work_with_one_edition();

    let doc = super::of(&conn, work_id).expect("projecting");

    assert!(
        doc.is_empty(),
        "a work nobody has hand-edited must project to the empty document, or \
         `migrate` and `export::work` both start believing every work in the \
         library holds hand-fixed metadata: {doc:?}"
    );
}

#[test]
fn clearing_a_hand_set_title_restores_the_one_derivation_minted() {
    let (conn, work_id) = a_work_with_one_edition();
    let derived: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading the derived title");

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    super::apply(&conn, work_id, &doc).expect("applying");

    let mut doc = super::of(&conn, work_id).expect("re-projecting");
    doc.title = None;
    super::apply(&conn, work_id, &doc).expect("clearing");

    let after: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading the title back");
    assert_eq!(
        after, derived,
        "withdrawing a hand title must restore what derivation minted. \
         `COALESCE(?, title)` would leave `Hand Set` here forever, while \
         `title_hand` read NULL — so `of` would report no hand edit, \
         `export::work` would delete the document recording it, and a rescan \
         would mint a title this database disagrees with"
    );
    assert!(
        super::of(&conn, work_id).expect("projecting").is_empty(),
        "and the document is empty again, which is what removes it from disk"
    );
}

/// An edition *dropped from the document entirely* — the key absent, a
/// different statement from `label: None` — has its derived label restored, and
/// `label_hand` cleared.
#[test]
fn dropping_an_edition_from_the_document_restores_its_derived_label() {
    let (conn, work_id) = a_work_with_one_edition();

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.editions.insert(
        "en-mono".to_string(),
        super::EditionMeta {
            label: Some("English, monochrome".to_string()),
            ..Default::default()
        },
    );
    super::apply(&conn, work_id, &doc).expect("applying");

    // The key is absent entirely, which is a different statement from
    // `label: None` and takes a different branch: the blanket clear, not the
    // per-key update.
    let mut doc = super::of(&conn, work_id).expect("re-projecting");
    doc.editions.clear();
    super::apply(&conn, work_id, &doc).expect("clearing");

    let (label, label_hand, label_derived): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT label, label_hand, label_derived FROM edition WHERE work_id = ?1 AND key = 'en-mono'",
            [work_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("reading the label");
    // `a_work_with_one_edition`'s sidecar declares this edition's label as
    // `"R"` (`import.rs`'s `Scope::Document` writes the document's own label
    // into `label_derived` at import time) — not the edition key.
    assert_eq!(
        label, "R",
        "an edition dropped from the document must fall back to its derived \
         label, not keep the withdrawn hand one while `label_hand` reads NULL"
    );
    assert_eq!(
        label_hand, None,
        "the withdrawn hand label must not linger in the provenance column either"
    );
    assert_eq!(
        label_derived.as_deref(),
        Some("R"),
        "and label_derived must still hold derivation's own answer, not the \
         hand text the safety net exists to fall back from"
    );
}

#[test]
fn editing_a_title_does_not_move_the_slug() {
    let (conn, work_id) = a_work_with_one_edition();
    let before: String = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading slug");

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.title = Some("A Completely Different Name".to_string());
    super::apply(&conn, work_id, &doc).expect("applying");

    let after: String = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading slug");
    assert_eq!(
        before, after,
        "a slug is an address and a title is a display name; the slug is the \
         URL bookmarks and OPDS clients hold (O67)"
    );
}

/// **`chosen` reports the attachment — O80.**
///
/// The field exists so a document can tell "a person put this work in this
/// series" from "here are this series' corrected titles".
///
/// Both directions, because a constant `true` and a constant `false` are each
/// half right and a single assertion cannot tell them apart.
#[test]
fn chosen_reports_whether_a_person_placed_this_work_on_this_shelf() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    conn.execute(
        "INSERT INTO series (id, sort_title) VALUES (1, 'A Shelf')",
        [],
    )
    .expect("a series");
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, hand) \
         VALUES (1, 'A Shelf', 'alt', 1)",
        [],
    )
    .expect("a hand-set title, so the block is projected at all");

    // Derivation's own answer: attached, but nobody chose it.
    conn.execute("UPDATE work SET series_id = 1 WHERE id = ?1", [work_id])
        .expect("derivation shelves it");
    let derived = of(&conn, work_id).expect("projects");
    assert!(
        !derived.series.titles.is_empty(),
        "the hand title is projected, or this test cannot see `chosen` at all"
    );
    assert!(
        !derived.series.chosen,
        "a derived attachment is not a person's choice, and a document saying \
         it was would freeze it on the next apply — which is O80"
    );

    // The same shelf, this time chosen.
    conn.execute(
        "UPDATE work SET series_id_hand = 1 WHERE id = ?1",
        [work_id],
    )
    .expect("a person places it");
    let placed = of(&conn, work_id).expect("projects");
    assert!(
        placed.series.chosen,
        "a hand attachment must be recorded, or a rebuild re-derives the work \
         onto whatever shelf the files suggest and the placement is lost"
    );
}

/// The other side of the refusal below: the version this build writes still
/// parses, so that refusal cannot be met by refusing everything.
#[test]
fn a_document_declaring_this_builds_version_parses() {
    let json = format!(r#"{{"taiyaku_library":{VERSION},"updated":"2026-08-14T11:03:22Z"}}"#);
    let parsed = LibraryDoc::from_json(&json).expect("this build's own version parses");
    assert_eq!(parsed.taiyaku_library, VERSION);
}

/// A newer document is refused rather than read without the fields this
/// build has no column for: `hand::save` rewrites the document from rows, so
/// reading it would let those fields be dropped from the only copy (O264).
#[test]
fn a_document_from_a_newer_taiyaku_is_refused_in_the_sidecars_words() {
    let newer = VERSION + 1;
    let json = format!(r#"{{"taiyaku_library":{newer},"updated":"2026-08-14T11:03:22Z"}}"#);
    let refusal = LibraryDoc::from_json(&json).expect_err("a newer document is not readable here");
    let words = format!(
        "this document declares taiyaku_library format version {newer}, and this build reads \
         {VERSION}. Refusing to read it rather than dropping the fields it does not understand \
         and writing them back out as version {VERSION} — upgrade taiyaku instead"
    );
    assert!(
        refusal.contains(&words),
        "the refusal is the sidecar's sentence with this document's key: {refusal}"
    );
}
