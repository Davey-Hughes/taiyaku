use std::collections::BTreeSet;

use rusqlite::Connection;

use crate::scan::tests::{fixture, write_archive, write_pages};
use crate::{Library, derive, scan};

/// A root whose volumes are **archives inside volume directories** —
/// `Series/v01/Series v01.cbz` — which is the layout this project's own
/// corpus uses and the one no fixture in this module had.
///
/// Every other fixture here builds members out of directories of page images,
/// so `scan_entry.pages` was never `0`: a volume directory holding archives
/// rather than loose images is counted by the walk as holding no pages of its
/// own, and that is the fact none of them carried.
fn archived_volumes() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("derive-archived");
    let series = dir.path.join("Series");
    write_archive(&series.join("v01").join("Series v01.cbz"), 3);
    write_archive(&series.join("v02").join("Series v02.cbz"), 4);
    dir
}

/// A root whose one volume arrives as two chapter directories, named so that
/// natural order and lexical order disagree — `c9` sorts after `c10` as text
/// and before it as a number.
///
/// **Both directories resolve a volume *and* a chapter under the seeded
/// rules**, which is what makes them members of a work rather than §7's
/// unattached chapter case: `builtin-volume` reads `v03` from each name and
/// `builtin-chapter` reads `c9`/`c10` from the same name. That pairing is the
/// only shape in which a chapter number the scanner resolved has a `member`
/// row to land on, since `member.edition_id` is NOT NULL and an unattached
/// chapter directory has no work to hang an edition off.
fn chaptered() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("derive-chaptered");
    let series = dir.path.join("Chaptered");
    write_pages(&series.join("Chaptered - v03 - c9"), 1);
    write_pages(&series.join("Chaptered - v03 - c10"), 2);
    dir
}

/// A root whose one member sits directly under it, with no series directory
/// above — the layout seeded rule 4 exists for, `^(?<series>.+?)[ _-]+v` over
/// the *name*, since a filename does not always repeat what a directory would
/// have said.
fn flat() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("derive-flat");
    write_pages(&dir.path.join("Alone - v01"), 2);
    dir
}

/// One volume-less work — a one-shot — in each of two roots, the second under
/// a rule naming an edition key that sorts before the first's, and holding a
/// different number of pages so the two editions' digests differ.
fn oneshot(label: &str, pages: usize) -> crate::tests::TempDir {
    let dir = crate::tests::tempdir(label);
    write_pages(&dir.path.join("Oneshot").join("Some Oneshot"), pages);
    dir
}

/// A root whose one series arrives as four **bare-numbered** directories,
/// which is a layout the rules deliberately resolve nothing from:
/// `volume_number_marked` refuses a number with no `v`, `vol` or `第` in front
/// of it, precisely so every `c001` file in the corpus does not acquire
/// volume 1, and `chapter_number` has no fallback either. So all four resolve
/// a series and nothing else.
///
/// Different page counts per directory, so a merge is visible in the totals
/// as well as in the row count.
fn bare_numbered() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("derive-bare-numbers");
    let series = dir.path.join("ONE PIECE");
    for (name, pages) in [("001", 3), ("002", 4), ("003", 5), ("004", 6)] {
        write_pages(&series.join(name), pages);
    }
    dir
}

/// A root whose one series arrives as several chapter directories under one
/// parent — §7's shape, at the scale that makes the count mean something: the
/// directory is what stays unattached, not each of the 146 files in it.
fn chapters() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("derive-chapters");
    let series = dir.path.join("Kaguya-sama - Fan Translated");
    write_pages(&series.join("Kaguya-sama - c001"), 1);
    write_pages(&series.join("Kaguya-sama - c002"), 1);
    write_pages(&series.join("Kaguya-sama - c003"), 1);
    dir
}

#[test]
fn editing_a_rule_re_derives_the_library_without_touching_the_filesystem() {
    // §2's claim, and the reason the fact layer is persisted at all. The tree
    // is DELETED between the scan and the second derive: if anything reopens
    // an archive to recount pages or rehash bytes, this fails.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    assert_eq!(edition_keys(&conn), vec!["raw", "raw"]);

    std::fs::remove_dir_all(&tree.path).expect("remove the tree entirely");

    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key)
         VALUES (99, 'prefix', '', 'path', 'colour')",
        [],
    )
    .expect("new rule");
    let report = derive::derive(&conn).expect("re-derive with no filesystem at all");

    assert_eq!(report.works, 2);
    // **Every key, sorted, rather than `SELECT key FROM edition LIMIT 1`.**
    // That is what this read, and it stopped meaning what it said the moment
    // a scan stopped deleting the editions it did not name: both keys are
    // present now, and SQLite answered `colour` from the `(work_id, key)`
    // index — where `colour` sorts before `raw` — rather than because the
    // rename had replaced anything.
    assert_eq!(
        edition_keys(&conn),
        vec!["colour", "colour", "raw", "raw"],
        "the rule decided a new key, and the old edition is kept rather than deleted"
    );
}

/// Every `edition.key` in the library, sorted — the shape an assertion needs
/// when the interesting outcome is which keys exist rather than which one a
/// query happened to answer with first.
fn edition_keys(conn: &rusqlite::Connection) -> Vec<String> {
    conn.prepare("SELECT key FROM edition ORDER BY key, id")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect")
}

#[test]
fn a_rule_edit_that_renames_an_edition_key_leaves_the_old_edition_standing_and_counts_it() {
    // The stated cost of `import::scanned` no longer deleting an edition the
    // scan did not name, and the reason `kept_editions` exists: the orphan is
    // reported rather than discovered. Without the counter, "the rule edit
    // renamed the edition" and "the rule edit added a second one beside the
    // first" read identically from the summary — one of them is a library
    // that needs fixing and the other is not.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    let first = derive::derive(&conn).expect("first");
    assert_eq!(
        first.kept_editions, 0,
        "nothing was standing before this derive ran"
    );

    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key)
         VALUES (99, 'prefix', '', 'path', 'colour')",
        [],
    )
    .expect("a rule that renames every edition key");
    let report = derive::derive(&conn).expect("second");

    assert_eq!(
        report.kept_editions, 2,
        "the `raw` edition of each of the two works"
    );
    assert_eq!(
        report.editions, 2,
        "and `editions` still counts what this derive wrote, which is why it \
         cannot answer the same question"
    );
}

#[test]
fn a_chapter_organised_directory_produces_members_and_no_work() {
    // §7. Which seven of 146 chapter files constitute volume 3 is a question
    // only the alignment answers, so the members are recorded with
    // `chapter_declared` and the directory stays visibly unattached.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 2, "the two volumes, and not the chapter file");
    assert_eq!(report.unattached_chapter_dirs, 1);
}

#[test]
fn re_deriving_keeps_a_series_row_so_a_dismissal_still_points_at_something() {
    // `proposal.subject_id` keys on the series row. If derivation deleted and
    // re-inserted series, every dismissal would be orphaned on the next scan
    // and §6's "dismissal has to persist" would quietly stop being true.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let first: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("series");

    let second_report = derive::derive(&conn).expect("second");

    let second: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("series");
    assert_eq!(
        first, second,
        "the same row, reconciled rather than re-minted"
    );

    // **The identity assertion above cannot fail on its own**, and that is
    // not a hypothetical: `series.sort_title` carries no unique constraint,
    // so a derive that inserted unconditionally would leave two rows for one
    // series and `SELECT id FROM series WHERE sort_title = …` would still
    // answer with the first — the same id, both times, forever. Measured by
    // making derivation re-mint and watching this test pass. The count is
    // what actually sees it.
    //
    // `DeriveReport`'s own doc claims a derive is "idempotent by
    // construction", and `series` was the only table pinning it. Every table
    // derivation writes is counted here instead, because each is reconciled
    // by a different identity — `series` on `sort_title`, `work` on
    // `content_key` and then on `(series_id, volume)`, and `edition`/`member`
    // by being replaced wholesale under a work — so three of the four could
    // grow while the fourth stayed put.
    let counts: (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM series), (SELECT count(*) FROM work),
                    (SELECT count(*) FROM edition), (SELECT count(*) FROM member)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .expect("count");
    assert_eq!(
        counts,
        (2, 2, 2, 2),
        "one row per series, work, edition and member — not one per derive"
    );
    assert_eq!(
        second_report.rekeyed, 0,
        "nothing's files moved, so nothing's identity did"
    );
}

#[test]
fn a_works_hub_survives_a_second_edition_arriving_in_another_root() {
    // The identity hazard, from the side that would bite: adding an edition
    // whose key sorts BEFORE the hub must not change what the work is.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let (id, hub): (i64, String) = conn
        .query_row("SELECT id, hub FROM work ORDER BY id LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("work");
    assert_eq!(hub, "raw");

    // A second root whose rule names an edition key sorting before "raw":
    // the same series, the same two volumes, under a rule scoped to this root
    // alone so the first root's editions keep the key they already have.
    let second = crate::tests::tempdir("derive-second-root");
    let series = second.path.join("Go-Toubun no Hanayome");
    write_pages(&series.join("Go-Toubun no Hanayome - v01 (2019)"), 3);
    write_pages(&series.join("Go-Toubun no Hanayome - v02 (2019)"), 4);
    let other = scan::add_root(&conn, &second.path).expect("add");
    conn.execute(
        "INSERT INTO rule (ordinal, root_id, kind, pattern, match_on, set_edition_key)
         VALUES (99, ?1, 'prefix', '', 'path', 'colour')",
        [other.id],
    )
    .expect("a rule scoped to the second root");
    scan::scan_root(&conn, &other).expect("scan");
    derive::derive(&conn).expect("second");

    let (still, hub_now): (i64, String) = conn
        .query_row("SELECT id, hub FROM work WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the same work");
    assert_eq!(still, id);
    assert_eq!(
        hub_now, "raw",
        "the hub was fixed when the work was created"
    );
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2, "two volumes, not four");
    let editions: i64 = conn
        .query_row(
            "SELECT count(*) FROM edition WHERE work_id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        editions, 2,
        "the arriving edition joined rather than forked"
    );
}

#[test]
fn a_format_nothing_reads_is_counted_and_produces_no_structure() {
    // §3: recorded, not skipped — the walk kept `notes.pdf` with its format
    // and no page count, and derivation is where "recorded" stops meaning
    // "turned into a work". Counted rather than dropped in silence, because
    // an epub root that produces nothing at all has to say so.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.unreadable, 1, "notes.pdf, and nothing else");
    let members: i64 = conn
        .query_row(
            "SELECT count(*) FROM member WHERE path LIKE '%notes.pdf'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(members, 0);
}

#[test]
fn a_members_digest_is_written_back_and_nothing_elses_is() {
    // §3's laziness ends here and nowhere else. The digest goes back onto
    // `scan_entry` so the next derive does not need the file — which is what
    // the deleted-tree test above actually depends on — and an entry that
    // became no member is not hashed at all, since hashing a 111-volume
    // series to decide it is not a member is the cost the fact layer exists
    // to avoid.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    derive::derive(&conn).expect("derive");

    let hashed: Vec<String> = conn
        .prepare("SELECT rel_path FROM scan_entry WHERE sha256 IS NOT NULL ORDER BY rel_path")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        hashed,
        vec![
            "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2019)".to_string(),
            "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v02 (2019)".to_string(),
        ],
        "the two members, and not the container, the chapter directory or the PDF"
    );
}

#[test]
fn an_entry_whose_bytes_have_gone_before_its_first_derive_is_counted_not_an_error() {
    // The other half of the deleted-tree case, and the half that would be a
    // hard failure if `member_digest`'s refusal were propagated: an entry
    // nothing has ever hashed, whose file is no longer there to hash. A
    // library where one share is unmounted must still derive the rest.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    std::fs::remove_dir_all(&tree.path).expect("remove the tree before anything is hashed");

    let report = derive::derive(&conn).expect("a missing file is not a failed derive");

    assert_eq!(
        report.works, 0,
        "nothing could be hashed, so nothing became a work"
    );
    assert_eq!(
        report.unreadable, 3,
        "the PDF, and the two members whose bytes are no longer there to hash"
    );
    assert_eq!(
        report.unattached_chapter_dirs, 1,
        "the chapter case never needed a digest, so it is unaffected"
    );
}

#[test]
fn members_of_one_edition_are_ordered_naturally_rather_than_lexically() {
    // `c9` sorts after `c10` as text. Member order is page order — every
    // offset in every alignment is computed from it — so a lexical sort here
    // would silently interleave the wrong pages.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.works, 1,
        "one volume, split across two chapter directories"
    );
    assert_eq!(report.members, 2);
    let paths: Vec<String> = conn
        .prepare("SELECT path FROM member ORDER BY ordinal")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert!(paths[0].ends_with("Chaptered - v03 - c9"), "{paths:?}");
    assert!(paths[1].ends_with("Chaptered - v03 - c10"), "{paths:?}");
}

#[test]
fn a_chapter_number_the_rules_resolved_lands_on_the_member() {
    // §7's "the members are recorded with `chapter_declared`", reached the
    // only way the schema allows: a member row exists once the entry also
    // resolved a volume, so the number the `builtin-chapter` rule read out of
    // the filename has somewhere to go. `chapter_source` says where it came
    // from, and `filename` is what distinguishes it from a person's own
    // answer — which derivation must never overwrite.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    derive::derive(&conn).expect("derive");

    let chapters: Vec<(Option<f64>, Option<String>)> = conn
        .prepare("SELECT chapter_declared, chapter_source FROM member ORDER BY ordinal")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        chapters,
        vec![
            (Some(9.0), Some("filename".to_string())),
            (Some(10.0), Some("filename".to_string())),
        ]
    );
}

#[test]
fn a_hand_set_chapter_number_outlives_a_re_derive_that_no_longer_resolves_one() {
    // Derivation owns what derivation wrote and nothing else. It clears its
    // own `filename` rows so a rule edit cannot leave a stale chapter number
    // behind, and that clear is scoped by `chapter_source` precisely so it
    // cannot reach a number a person entered — which no re-derive could
    // recover, and which is why `member.chapter_declared` is in migration 1.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    conn.execute(
        "UPDATE member SET chapter_declared = 99.0, chapter_source = 'hand' WHERE ordinal = 0",
        [],
    )
    .expect("a person disagrees with the filename");
    // Disable the chapter rule, so a re-derive resolves no chapter at all.
    conn.execute(
        "UPDATE rule SET enabled = 0 WHERE kind = 'builtin-chapter'",
        [],
    )
    .expect("disable");

    derive::derive(&conn).expect("second");

    let chapters: Vec<(Option<f64>, Option<String>)> = conn
        .prepare("SELECT chapter_declared, chapter_source FROM member ORDER BY ordinal")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        chapters,
        vec![(Some(99.0), Some("hand".to_string())), (None, None)],
        "the hand answer stayed and the scanner's own stale one went"
    );
}

#[test]
fn a_hand_set_chapter_number_survives_a_re_derive_that_still_resolves_one() {
    // The clear above is scoped to `chapter_source = 'filename'`, so a hand
    // value survives it — but the write beneath it carried no predicate at
    // all, and `builtin-chapter` is a seeded rule, so the very re-derive a
    // person runs right after correcting a chapter is the one that reverts
    // it, silently, whenever the filename still parses.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    conn.execute(
        "UPDATE member SET chapter_declared = 99.0, chapter_source = 'hand' WHERE ordinal = 0",
        [],
    )
    .expect("a person disagrees with the filename");

    derive::derive(&conn).expect("second");

    let chapters: Vec<(Option<f64>, Option<String>)> = conn
        .prepare("SELECT chapter_declared, chapter_source FROM member ORDER BY ordinal")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        chapters,
        vec![
            (Some(99.0), Some("hand".to_string())),
            (Some(10.0), Some("filename".to_string())),
        ],
        "the hand answer should survive even though the filename still parses"
    );
}

#[test]
fn a_comicinfo_chapter_number_survives_a_re_derive_that_still_resolves_one() {
    // `comicinfo` survives the clear today, which is scoped by
    // `chapter_source` exactly as `hand` is — the write beneath it is not
    // scoped at all, and overwrites it on the next scan of a member whose
    // filename happens to parse. The same defect as the hand case, and
    // pre-existing from the scanner branch.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    conn.execute(
        "UPDATE member SET chapter_declared = 5.0, chapter_source = 'comicinfo' WHERE ordinal = 0",
        [],
    )
    .expect("a comicinfo document disagrees with the filename");

    derive::derive(&conn).expect("second");

    let chapters: Vec<(Option<f64>, Option<String>)> = conn
        .prepare("SELECT chapter_declared, chapter_source FROM member ORDER BY ordinal")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        chapters,
        vec![
            (Some(5.0), Some("comicinfo".to_string())),
            (Some(10.0), Some("filename".to_string())),
        ],
        "the comicinfo answer should survive even though the filename still parses"
    );
}

#[test]
fn a_derived_series_is_findable_by_title_so_a_document_does_not_mint_a_second_one() {
    // `series.sort_title` is not what a document looks a series up by:
    // `librarydoc::find_series_by_title` reads `series_title`, and
    // `series_fts` is fed from it by trigger. A series row with no title row
    // is invisible to both, so applying a `taiyaku.library.json` over a
    // scanned work would mint a second series beside the derived one.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    derive::derive(&conn).expect("derive");

    let found: i64 = conn
        .query_row(
            "SELECT series_id FROM series_title WHERE title = 'Go-Toubun no Hanayome'
               AND is_primary = 1",
            [],
            |r| r.get(0),
        )
        .expect("title");
    let series: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("series");
    assert_eq!(found, series);
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM series_fts WHERE series_fts MATCH 'Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("search");
    assert_eq!(hits, 1, "and the search index followed");
}

#[test]
fn a_series_that_only_holds_chapters_still_gets_a_row_for_a_proposal_to_point_at() {
    // Task 8's `attach-chapters` proposal keys `subject_id` on a `series`
    // row. The chapter-organised directory produces no work, so if the series
    // were minted only alongside a work there would be nothing for that
    // proposal to reference and §6's inbox could never name it.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.series, 2);
    let held: i64 = conn
        .query_row(
            "SELECT count(*) FROM series WHERE sort_title = 'Kaguya-sama - Fan Translated'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        held, 1,
        "a series with no work of its own is still a series"
    );
}

#[test]
fn a_work_carries_the_series_and_volume_the_rules_resolved() {
    // The two columns `import::scanned` cannot write, because the sidecar
    // format has no field for either — and the two the hub lookup reads back
    // to find a work it already minted. A derive that left them NULL would
    // mint a second work for every volume on every rescan.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.editions, 2);
    assert_eq!(report.members, 2);
    assert_eq!(
        report.suppressed, 0,
        "nothing in this tree sits inside anything else the rules named"
    );
    let volumes: Vec<(String, Option<f64>)> = conn
        .prepare(
            "SELECT series.sort_title, work.volume FROM work
             JOIN series ON series.id = work.series_id ORDER BY work.volume",
        )
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        volumes,
        vec![
            ("Go-Toubun no Hanayome".to_string(), Some(1.0)),
            ("Go-Toubun no Hanayome".to_string(), Some(2.0)),
        ]
    );
}

#[test]
fn a_hand_set_volume_outlives_a_re_derive_once_a_library_document_is_absorbed() {
    // I7, the settled half. `librarydoc::apply` — reached through
    // `migrate::work`, §4.4 — is the documented mechanism for hand-setting
    // `work.volume`/`work.series_id`, and `<stem>.library.json` exists
    // precisely so that survives a re-derivation. Before this fix, `derive`
    // wrote both columns unconditionally on every pass: measured,
    // `volume = 4.5` came back as `1.0` after one derive.
    //
    // **`migrate::record_absorbed` directly, not the full `migrate::work`
    // pipeline.** `export::work` is what calls it in production — writing
    // `taiyaku.library.json` for a work is what makes this library's own
    // metadata "the stored fact" `migrated_document` records — and calling
    // it here reaches the exact same row `export::work` would, without
    // depending on `export::work`'s own file-writing side effects, which
    // this test has no need of. `migrate_library`'s own `step()` cannot be
    // used for this: it treats *any* work whose `librarydoc::of` already
    // answers non-empty — which is every derived work, series and volume
    // both being part of that projection — as already holding state, and
    // never reaches `Step::Apply` for one at all; that is a separate,
    // pre-existing gap in the one-way-migration design, not this fix's to
    // close.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row(
            "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    // **`volume_hand`, not `volume` — O80, migration 011.** A hand-set
    // volume lives in its own provenance column now; the `library` marker no
    // longer protects anything, having been what let a title-only Save freeze
    // three columns nobody touched. Writing the effective column and arming
    // the marker was how this was expressed before, and it would now assert
    // nothing: `derive` writes `COALESCE(volume_hand, <derived>)`, so a value
    // parked in `volume` alone is exactly what a re-derive is supposed to
    // replace.
    conn.execute(
        "UPDATE work SET volume_hand = 4.5, volume = 4.5 WHERE id = ?1",
        [work_id],
    )
    .expect("a person hand-sets the volume");

    derive::derive(&conn).expect("second derive, over the same unchanged files");

    let volume_after: f64 = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("volume");
    assert_eq!(
        volume_after, 4.5,
        "the hand-set volume must survive a re-derive rather than reverting to what the \
         rules resolve"
    );
}

/// O53's arm, and the regression net for the read-back Task 5 wires into
/// `write_work`. **Through the real `crate::hand::save`**, unlike the test
/// above, which arms the guard with a direct `migrate::record_absorbed` call:
/// `save` is what a real hand edit calls, and it both writes
/// `<content_key>.library.json` beside the root *and* records the marker in
/// the same operation. So by the time the second `derive` below runs, the
/// marker this test's own connection already holds is exactly what a real
/// edit would have left — this does not yet exercise `hand::load` being
/// consulted by a *fresh* database with no marker of its own (that is
/// `hand::tests::a_rescan_with_no_database_recovers_the_hand_set_title`,
/// across two separate connections); it pins that wiring the read-back into
/// `write_work` did not disturb the guard's existing, already-armed case.
#[test]
fn a_hand_document_beside_the_library_survives_the_next_derive() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    // Migration 012 defaults `keeps_documents` off; this test's name and its
    // own doc comment both say the document lands *beside the root*, which an
    // off root would make false without ever failing an assertion — `save`
    // would still record the marker, and `volume` would still come back 4.5
    // through `COALESCE(volume_hand, …)` alone. So it opts in.
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("the fixture keeps documents");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row(
            "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.volume = Some(4.5);
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");
    assert!(
        crate::hand::path_for(&conn, work_id)
            .expect("addressing")
            .expect("an opted-in root has an address")
            .is_file(),
        "the name of this test is a claim about a file, so it checks one"
    );

    derive::derive(&conn).expect("second derive");

    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading volume");
    assert_eq!(
        volume,
        Some(4.5),
        "`derive.rs`'s own comment: a hand-set `volume = 4.5` came back as \
         `1.0` after one derive before the I7 guard"
    );
}

/// The condition the test above cannot reach, because it calls `hand::save`
/// first — which arms the marker itself, so `!absorbed` is `false` and
/// `hand::load` is never called. Deleting the whole read-back block in
/// `write_work` leaves that test green; it pins the #77 guard, not the
/// read-back Task 5 adds.
///
/// **Renamed from `a_derive_applies_a_hand_document_this_database_has_never_
/// seen`.** That name claimed the case this module's header now calls out as
/// the actual merge blocker — a work this database has never derived at all,
/// so its `member.root_id` is still NULL when the read-back's `hand::load`
/// calls `hand::path_for` — and does not build it: `work_id` and its
/// `root_id` are minted by the *first* `derive::derive` call above and never
/// touched again, so by the time `DELETE FROM migrated_document` runs, the
/// row this test clears the marker on already exists, `root_id` included.
/// What this test actually pins is a *different*, also-real case: a marker
/// cleared on a work the database already has, which is what
/// `migrate::work`'s own migration path can produce. The genuinely fresh
/// case — a work minted for the first time, in a database that has never
/// seen this root before — is
/// [`a_fresh_database_recovers_a_hand_set_title_on_its_first_derive`], across
/// two separate connections rather than one connection with a table cleared.
#[test]
fn a_derive_applies_a_hand_document_whose_marker_was_cleared_on_an_existing_work() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    // Migration 012 defaults `keeps_documents` off; this test exists to pin
    // the read-back reading a file off disk, and an off root leaves
    // `hand::save` below writing no file at all — `hand::load` would then
    // answer `Ok(None)` and the assertion below would pass for a reason that
    // has nothing to do with the read-back. So it opts in.
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("the fixture keeps documents");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row(
            "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.volume = Some(4.5);
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    // What a rebuilt database looks like: the file is still there, but this
    // database has no memory of ever having read it, and the row it left
    // behind is back to what the rules alone would derive.
    //
    // **`volume_hand` is cleared too, not just `volume`.** O80's `COALESCE
    // (volume_hand, <derived>)` means leaving `volume_hand = 4.5` in place
    // would carry the value across the second derive on its own, whether or
    // not the read-back below ever ran — exactly the vacuous pass this test
    // exists to refuse. Clearing it makes `volume` depend on the read-back
    // applying the document, the same as a database that never set the
    // provenance column in the first place.
    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");
    conn.execute(
        "UPDATE work SET volume = 1.0, volume_hand = NULL WHERE id = ?1",
        [work_id],
    )
    .expect("returning the row to its derived state");

    derive::derive(&conn).expect("second derive");

    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading volume");
    assert_eq!(
        volume,
        Some(4.5),
        "a hand document this database has never seen must be applied by the \
         read-back; without it the guard is open and derivation writes over a \
         correction sitting beside the archives"
    );
}

/// **The merge blocker, reproduced.** Losing the database was supposed to
/// cost one rescan (store spec §4.4) and, as shipped, costs two: a work
/// scanned for the first time by a brand-new database has `member.root_id`
/// still NULL at the moment `write_work`'s read-back consults
/// `hand::load` — `import::scanned` has just minted the row and nothing has
/// written a root onto its members yet — so `hand::path_for`'s
/// `JOIN root ON root.id = member.root_id` matches nothing, `path_for`
/// answers `None`, `hand::load` answers `Ok(None)`, and the correction
/// sitting right beside the archives is silently skipped. A *second* derive
/// against the same, now-populated database recovers it, which is exactly
/// the extra rescan this test exists to refuse.
///
/// **Two separate connections, not one connection with a table cleared.**
/// [`a_derive_applies_a_hand_document_whose_marker_was_cleared_on_an_existing_work`]
/// clears `migrated_document` on a connection that has already derived this
/// work once, so `work_id` and its `root_id` already exist by the time the
/// second `derive::derive` runs — the case `migrate::work`'s own migration
/// path produces. Here the second database has never seen this root at all:
/// `work_id` does not exist yet when its own `derive::derive` starts, so its
/// `member.root_id` genuinely starts NULL, which is the only way to meet the
/// guard the way a rebuilt database actually meets it.
#[test]
fn a_fresh_database_recovers_a_hand_set_title_on_its_first_derive() {
    let tree = fixture();

    // First database: scan, derive, and correct the volume 1 work's title by
    // hand — `PATCH /api/library/works/{slug}` in miniature, landing at
    // `<root>/.taiyaku/<content_key>.library.json` through the same
    // `librarydoc::apply` + `hand::save` pair the route calls.
    {
        let first = Library::open_in_memory().expect("open");
        let conn = first.conn();
        let root = scan::add_root(&conn, &tree.path).expect("add");
        // Migration 012 defaults `keeps_documents` off; this test is about
        // the document surviving the database being lost, so it opts in.
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the fixture keeps documents");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("first derive");

        let work_id: i64 = conn
            .query_row(
                "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
                 WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
                [],
                |r| r.get(0),
            )
            .expect("the volume 1 work");

        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
        doc.title = Some("Go-Toubun no Hanayome, volume 1".to_string());
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
        let saved = crate::hand::save(&conn, work_id).expect("saving");
        assert!(
            saved.persisted,
            "the correction must actually reach disk, or losing the database \
             below proves nothing"
        );
    }

    // The database above is gone — genuinely, by never having existed past
    // this point, rather than simulated by clearing a table on the same
    // connection. A second, empty database is built from scratch and pointed
    // at the very same root: what a rebuilt database actually looks like.
    let second = Library::open_in_memory().expect("open");
    let conn = second.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect(
        "one derive, against a database that has never scanned or derived this root before",
    );

    let title: String = conn
        .query_row(
            "SELECT w.title FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    assert_eq!(
        title, "Go-Toubun no Hanayome, volume 1",
        "one derive against a database that has never seen this root before must \
         recover the hand-set title — losing the database must cost one rescan \
         (store spec §4.4), not two"
    );
}

/// **I2.** The read-back's own inline comment states the rule — `!empty`,
/// matching `hand::save`'s identical rule, since a hand-emptied document has
/// nothing left to protect and arming the marker for it would freeze the
/// cleared state in permanently — and nothing pinned it: mutating that
/// `!empty` to `true` leaves every other test in this file green, since none
/// of them ever hands the read-back a document with nothing in it.
///
/// `crate::hand::load` parses the *file's* content directly
/// (`LibraryDoc::from_json`), independent of whatever the row currently
/// holds — so an empty document is written straight to the address rather
/// than through `librarydoc::apply`/`hand::save`, which would have left
/// nothing on disk to read back at all (`hand::save`'s own empty-document
/// branch removes the file rather than writing one).
#[test]
fn an_empty_hand_document_this_database_has_never_seen_leaves_the_guard_open() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row(
            "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    let empty_doc = crate::librarydoc::LibraryDoc {
        taiyaku_library: crate::librarydoc::VERSION,
        updated: "2026-08-29T00:00:00Z".to_string(),
        series: crate::librarydoc::SeriesInfo::default(),
        volume: None,
        title: None,
        editions: std::collections::BTreeMap::new(),
        members: Vec::new(),
        tags: Vec::new(),
        external: std::collections::BTreeMap::new(),
    };
    assert!(
        empty_doc.is_empty(),
        "this test's own fixture document must actually be empty, or it \
         proves nothing"
    );
    let path = crate::hand::path_for(&conn, work_id)
        .expect("resolving")
        .expect("a scanned work has an address");
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
    std::fs::write(&path, empty_doc.to_json()).expect("writing the empty document");

    // What a rebuilt database looks like, exactly as the sibling test above —
    // no memory of ever having read this file, and the row moved somewhere
    // derivation alone would never have put it, so a second derive that
    // leaves it there (rather than correcting it) proves the guard armed
    // when the document gave it nothing to protect.
    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");
    conn.execute("UPDATE work SET volume = 99.0 WHERE id = ?1", [work_id])
        .expect("moving the row away from what derivation would produce");

    derive::derive(&conn).expect("second derive");

    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading volume");
    assert_eq!(
        volume,
        Some(1.0),
        "an empty hand document has nothing to protect, so the read-back must \
         leave the guard open and let derivation resume — arming it here \
         would freeze the wrong value in permanently, with no route able to \
         clear a field back out (I2)"
    );
}

/// Step 6b2. The previous round's whole point — a user's file cannot abort a
/// derive — was reached by no test: `write_work`'s two
/// `report.unreadable_hand.push(why)` arms had zero coverage, and the only
/// test naming the field builds a `DeriveReport` by hand and calls
/// `derive_summary` directly, which exercises string formatting and nothing
/// else.
///
/// **`fixture()`'s own two volumes**, so "the scan went on" is a claim about
/// the second work rather than about a counter that never saw one — a count
/// that only ever sees one work cannot distinguish "the scan continued" from
/// "the scan stopped after the one work it had".
#[test]
fn a_hand_document_that_will_not_parse_is_reported_and_the_scan_goes_on() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    let first = derive::derive(&conn).expect("first derive");
    assert_eq!(
        first.works, 2,
        "fixture() carries two volumes, each its own work — the shape this test \
         needs to tell a continued scan from a stopped one"
    );

    let work_id: i64 = conn
        .query_row(
            "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    // A hand document at the first work's own address, holding text that is
    // not a library document at all.
    let path = crate::hand::path_for(&conn, work_id)
        .expect("resolving")
        .expect("a scanned work has an address");
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
    std::fs::write(&path, b"{ not a document").expect("writing the bad file");

    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");

    let report = derive::derive(&conn).expect(
        "one unparseable file must not fail the derive — write_work is called with `?` \
         inside the group loop, so raising here would leave every later work underived",
    );

    assert_eq!(report.unreadable_hand.len(), 1);
    assert!(
        report.unreadable_hand[0].contains(&path.display().to_string()),
        "the message must name the file to fix: {:?}",
        report.unreadable_hand
    );
    assert_eq!(
        report.works, 2,
        "the second work must still have been derived"
    );
}

/// The arm Finding 1 actually fixed. `librarydoc::apply`'s own errors name a
/// `work_id` and a `series_id`, never a file — unlike the parse failure above,
/// this is the case whose message needed the path prepended by the read-back
/// rather than inherited from the error, since there is nothing to inherit.
#[test]
fn a_hand_document_naming_an_edition_the_work_does_not_have_is_reported_and_the_scan_goes_on() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row(
            "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    // A document that parses fine but names an edition this work does not
    // have. `apply_editions` refuses it by name, and the refusal it builds
    // names a `work_id`, never a path.
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.editions.insert(
        "nonexistent-edition".to_string(),
        crate::librarydoc::EditionMeta::default(),
    );
    let path = crate::hand::path_for(&conn, work_id)
        .expect("resolving")
        .expect("a scanned work has an address");
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
    std::fs::write(&path, doc.to_json()).expect("writing the bad document");

    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");

    let report = derive::derive(&conn).expect("an apply failure must not fail the derive either");

    assert_eq!(report.unreadable_hand.len(), 1);
    assert!(
        report.unreadable_hand[0].contains(&path.display().to_string()),
        "the path must be prepended by the read-back, since `librarydoc::apply`'s own \
         message never names one: {:?}",
        report.unreadable_hand
    );
    assert!(
        report.unreadable_hand[0].contains("nonexistent-edition"),
        "apply_editions's own message names the edition it refused: {:?}",
        report.unreadable_hand
    );
    assert_eq!(
        report.works, 2,
        "the second work must still have been derived"
    );
}

/// A plain re-derive, with no hand document ever applied, still writes
/// `volume`/`series_id` every time — the I7 guard must not silently disable
/// that for every work, only for the ones a person has actually touched
/// through `librarydoc::apply`.
#[test]
fn a_re_derive_still_writes_the_volume_for_a_work_no_hand_document_has_touched() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    // Simulate exactly the corruption I7 exists to prevent — a stray hand
    // value with no `migrated_document` row behind it — so this test would
    // fail loudly if the guard read anything weaker than that marker (a
    // non-NULL `volume`, say).
    conn.execute("UPDATE work SET volume = 99.0", [])
        .expect("simulate a value nothing actually applied");

    derive::derive(&conn).expect("second derive");

    let volumes: Vec<f64> = conn
        .prepare("SELECT volume FROM work ORDER BY volume")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        volumes,
        vec![1.0, 2.0],
        "with no migrated library document, the rules' own answer must win, not the \
         stray value: {volumes:?}"
    );
}

/// Every edition's `(key, kind, language)`, in a fixed order — the shape an
/// assertion about the two columns `derive::write_edition_metadata` writes
/// needs, since `SELECT … LIMIT 1` would pass on a library where only one of
/// two editions was written.
fn edition_metadata(conn: &rusqlite::Connection) -> Vec<(String, Option<String>, Option<String>)> {
    conn.prepare("SELECT key, kind, language FROM edition ORDER BY key, id")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect")
}

/// `(kind, language)` as owned strings, so an assertion reads as the two
/// values rather than as a tower of `Some("…".to_string())`.
fn metadata(kind: Option<&str>, language: Option<&str>) -> (Option<String>, Option<String>) {
    (kind.map(str::to_string), language.map(str::to_string))
}

/// A rule that names an edition kind and a language over the fixture's own
/// series — the two columns C3's accept form offers as controls, written the
/// way `proposals::accept_with` writes them.
fn a_rule_setting(conn: &rusqlite::Connection, kind: Option<&str>, language: Option<&str>) {
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_kind, set_language)
         VALUES (99, 'prefix', 'Go-Toubun no Hanayome/', 'path', ?1, ?2)",
        rusqlite::params![kind, language],
    )
    .expect("the rule an accept would have written");
}

#[test]
fn a_rules_edition_kind_and_language_reach_the_edition_row() {
    // Measured on real archives before this was written, not reasoned about:
    // accepting an `edition-of` proposal writes a rule holding
    // `set_kind = 'official'`, the next scan derives the edition that rule
    // names, and `edition.kind` came back NULL — `import::replace_editions`
    // inserts `(work_id, key, label)`, and until now nothing else wrote
    // either column from the rules at all.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    a_rule_setting(&conn, Some("official"), Some("Japanese"));

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.unrecognised_kinds, 0);
    let held = edition_metadata(&conn);
    assert_eq!(
        held.iter()
            .map(|(_, kind, language)| (kind.clone(), language.clone()))
            .collect::<Vec<_>>(),
        vec![
            metadata(Some("official"), Some("Japanese")),
            metadata(Some("official"), Some("Japanese")),
        ],
        "both of the fixture's editions are under the rule's prefix: {held:?}",
    );
}

#[test]
fn a_rule_that_stops_naming_a_kind_clears_the_one_the_last_derive_wrote() {
    // The half that makes the control usable more than once. Filling only a
    // NULL would have been the safer-looking rule and would have frozen the
    // first answer a person ever gave: `import::replace_editions` restores
    // these two columns across its own delete-and-reinsert, so nothing else
    // would ever remove the stale value.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    a_rule_setting(&conn, Some("official"), Some("Japanese"));
    derive::derive(&conn).expect("first derive");

    conn.execute("DELETE FROM rule WHERE ordinal = 99", [])
        .expect("the person edits the rule back");
    derive::derive(&conn).expect("second derive");

    let held = edition_metadata(&conn);
    assert!(
        held.iter()
            .all(|(_, kind, language)| kind.is_none() && language.is_none()),
        "a rule that resolves nothing leaves nothing standing: {held:?}",
    );
}

#[test]
fn an_edition_kind_the_closed_set_does_not_hold_is_refused_and_counted() {
    // `edition.kind` is `raw | official | fan` and `rule.set_kind` is a bare
    // `TEXT` column, so a rule can carry a word the column cannot. Storing it
    // would cost more than the typo: `librarydoc::of` refuses an unrecognised
    // stored kind, so the work would stop describing and stop exporting —
    // which is what the last assertion here pins.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    a_rule_setting(&conn, Some("bootleg"), Some("Japanese"));

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.unrecognised_kinds, 2,
        "one per edition the rule reached, so a person is told rather than left \
         wondering why the kind never moved",
    );
    let held = edition_metadata(&conn);
    assert!(
        held.iter().all(|(_, kind, _)| kind.is_none()),
        "the word the column cannot hold is not stored: {held:?}",
    );
    assert!(
        held.iter()
            .all(|(_, _, language)| language.as_deref() == Some("Japanese")),
        "and the field that was fine is still written: {held:?}",
    );
    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("a work");
    assert!(
        crate::librarydoc::of(&conn, work_id).is_ok(),
        "the work still describes, which is the whole reason the value is dropped",
    );
}

#[test]
fn a_hand_applied_library_documents_edition_kind_outlives_a_re_derive() {
    // The other side of I7's guard, now that these two columns sit behind it.
    // `librarydoc::apply` is the only other production writer of a value into
    // them, and `migrate::absorbed(…, LIBRARY)` is what says, per work,
    // whether such a document has actually been applied — the same question
    // `work.volume` and `work.series_id` already ask two lines away.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let hand: i64 = conn
        .query_row("SELECT id FROM work ORDER BY volume LIMIT 1", [], |r| {
            r.get(0)
        })
        .expect("the volume 1 work");
    crate::migrate::record_absorbed(&conn, hand, crate::migrate::LIBRARY, true)
        .expect("marks this work's library metadata as this library's own");
    conn.execute(
        // The provenance columns, for the reason the volume test above
        // gives (O80, migration 011).
        "UPDATE edition
            SET kind_hand = 'fan', kind = 'fan',
                language_hand = 'English', language = 'English'
          WHERE work_id = ?1",
        [hand],
    )
    .expect("what the document said");
    a_rule_setting(&conn, Some("official"), Some("Japanese"));

    derive::derive(&conn).expect("second derive, over the same unchanged files");

    let (kind, language): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT kind, language FROM edition WHERE work_id = ?1",
            [hand],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the absorbed work's edition");
    assert_eq!(
        (kind, language),
        metadata(Some("fan"), Some("English")),
        "the document owns this work's edition metadata, so the rule does not reach it",
    );
    // And the guard is per work, not a switch that quiets the whole library:
    // the other work, which no document has touched, follows the rule.
    let (kind, language): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT kind, language FROM edition WHERE work_id != ?1",
            [hand],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the untouched work's edition");
    assert_eq!(
        (kind, language),
        metadata(Some("official"), Some("Japanese"))
    );
}

#[test]
fn an_edition_takes_the_kind_of_the_first_member_that_resolved_one() {
    // One edition can gather entries the rules answered differently — this
    // fixture's volume arrives as two chapter directories, and only the
    // second in page order is under the rule's prefix. Reading the kind off
    // the first member alone would answer NULL here and would throw away a
    // root's `default_kind` the moment a file from another root sorted first.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_kind)
         VALUES (99, 'prefix', 'Chaptered/Chaptered - v03 - c10', 'path', 'fan')",
        [],
    )
    .expect("a rule reaching only the later chapter");

    derive::derive(&conn).expect("derive");

    // `c9` sorts before `c10` in page order — that is what `chaptered()` is
    // built to make true — so this is the second member's answer, reached
    // past the first member's silence.
    assert_eq!(
        edition_metadata(&conn),
        vec![("raw".to_string(), Some("fan".to_string()), None)],
    );
}

#[test]
fn a_member_directly_under_the_root_keeps_its_whole_name_as_the_title() {
    // `work.title` is what a reader sees, what `work_fts` indexes and what
    // the slug is built from. A member with no directory above it has no `/`
    // in its `rel_path`, and taking "the part after the last slash" of a
    // string with no slash is one off-by-one away from `lone - v01`.
    let tree = flat();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    let (title, slug): (String, String) = conn
        .query_row("SELECT title, slug FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("work");
    assert_eq!(title, "Alone - v01");
    assert_eq!(slug, "alone---v01");
}

#[test]
fn a_volume_less_work_still_matches_itself_on_the_next_derive() {
    // `volume` is nullable and `NULL = NULL` is NULL in SQL, so the hub
    // lookup uses `IS`. With `=` a one-shot never matches the row it already
    // minted: the hub is chosen fresh, an arriving edition whose key sorts
    // first takes it, `content_key` moves with it, and `import` inserts a
    // second work — leaving the reader's progress and notes on the first.
    let first = oneshot("derive-oneshot-a", 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &first.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let (id, volume): (i64, Option<f64>) = conn
        .query_row("SELECT id, volume FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("work");
    assert_eq!(volume, None, "nothing in the name says which volume it is");

    let second = oneshot("derive-oneshot-b", 5);
    let other = scan::add_root(&conn, &second.path).expect("add");
    conn.execute(
        "INSERT INTO rule (ordinal, root_id, kind, pattern, match_on, set_edition_key)
         VALUES (99, ?1, 'prefix', '', 'path', 'colour')",
        [other.id],
    )
    .expect("a rule scoped to the second root");
    scan::scan_root(&conn, &other).expect("scan");
    let report = derive::derive(&conn).expect("second");

    assert_eq!(report.works, 1);
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 1, "the same one-shot, not a second one");
    let (still, hub): (i64, String) = conn
        .query_row("SELECT id, hub FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("work");
    assert_eq!(still, id);
    assert_eq!(hub, "raw", "the hub was fixed when the work was created");
}

#[test]
fn a_container_that_would_not_open_is_counted_rather_than_written_with_no_pages() {
    // The walk swallowed `member_pages`' refusal so one truncated download
    // would not leave a whole library unscanned, and recorded the row with
    // its format and a NULL count. `member.pages` is NOT NULL, so there is
    // nothing honest for derivation to write — and the volume it names must
    // not become a work with a page count invented for it.
    let tree = fixture();
    std::fs::write(
        tree.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v03.cbz"),
        b"PK\x03\x04",
    )
    .expect("four bytes of zip signature and nothing behind them");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("a bad archive is not a failed derive");

    assert_eq!(report.works, 2, "v01 and v02, and not the truncated v03");
    assert_eq!(
        report.unreadable, 2,
        "the PDF and the archive that would not list"
    );
    let volumes: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE volume = 3.0", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(volumes, 0);
}

#[test]
fn a_rule_that_captures_a_title_decides_the_works_own() {
    // The `title` capture is the one field of `Resolved` that reaches
    // `work.title` — what a reader sees, what `work_fts` indexes and what the
    // slug is built from. Without it the name on disk is the only answer, and
    // `- v01 (2019)` is a scene tag rather than a title.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (99, 'regex', '^(?<title>.+) \\(\\d{4}\\)$', 'name')",
        [],
    )
    .expect("a rule that strips the year");

    derive::derive(&conn).expect("derive");

    let titles: Vec<String> = conn
        .prepare("SELECT title FROM work ORDER BY volume")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        titles,
        vec![
            "Go-Toubun no Hanayome - v01".to_string(),
            "Go-Toubun no Hanayome - v02".to_string(),
        ]
    );
}

#[test]
fn several_chapter_directories_under_one_parent_are_one_unattached_directory() {
    // §7 counts the *directory*, which is the thing a person would act on:
    // `attach-chapters` asks one question about "which of these constitute
    // volume 3", not 146 identical ones. Counting the entries instead reads
    // the same on a fixture holding a single chapter and diverges the moment
    // a real chapter root is scanned.
    let tree = chapters();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.unattached_chapter_dirs, 1,
        "one directory, three chapters"
    );
    assert_eq!(report.works, 0);
    assert_eq!(report.members, 0);
    let hashed: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE sha256 IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        hashed, 0,
        "and none of them was hashed, since none was going to be a member"
    );
}

#[test]
fn a_volume_split_into_chapter_subdirectories_is_one_member() {
    // `sidecar.md` records seven of fifty-seven corpus archives doing this.
    // Without the ancestor test the container and all three children become
    // members, the volume's pages are counted twice, and three chapters
    // become editions of nothing.
    let tree = crate::tests::tempdir("derive-nested");
    let volume = tree.path.join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        crate::scan::tests::write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(report.members, 1, "the volume, not its three chapters");
    let pages: i64 = conn
        .query_row("SELECT pages FROM member", [], |r| r.get(0))
        .expect("member");
    assert_eq!(pages, 6, "counted once");
}

#[test]
fn the_chapters_inside_a_volume_are_not_an_unattached_directory() {
    // The other half of the nested case, and the half the members count
    // cannot see: those three chapter directories resolve a chapter and no
    // volume, so without the ancestor test they take §7's unattached branch
    // and the volume they are plainly *inside* is reported as needing an
    // `attach-chapters` decision nobody has to make.
    let tree = crate::tests::tempdir("derive-nested-attached");
    let volume = tree.path.join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.unattached_chapter_dirs, 0,
        "they are attached: to v01"
    );
}

#[test]
fn a_chapter_file_naming_its_own_volume_does_not_become_a_second_member() {
    // The shape that actually double-counts. `Series - v01 - c001` carries a
    // volume marker as well as a chapter one, so the rules give it BOTH — it
    // resolves to the same `(series, volume)` as the directory containing it
    // and joins that work as a member beside its own parent. Six pages become
    // twelve and one volume becomes four members, three of which are pieces
    // of the first. `Kaguya-sama v03/Kaguya-sama v03 c015` is that layout.
    let tree = crate::tests::tempdir("derive-nested-marked");
    let volume = tree.path.join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        write_pages(&volume.join(format!("Series - v01 - {chapter}")), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(
        report.members, 1,
        "the volume, not the volume and its pieces"
    );
    let (path, pages): (String, i64) = conn
        .query_row("SELECT path, pages FROM member", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("member");
    assert!(path.ends_with("Series - v01"), "{path}");
    assert_eq!(pages, 6, "counted once");
}

#[test]
fn a_chapter_directory_inside_another_one_is_not_a_second_unattached_directory() {
    // The chapter half of the ancestor rule, which the volume half cannot
    // reach: `c001` resolves a chapter and no volume, so it is a candidate
    // without ever being a member, and the half-chapter directory inside it
    // is already covered by whatever decision `c001` is waiting for. Counting
    // both would put two rows in §6's inbox for one question.
    let tree = crate::tests::tempdir("derive-nested-chapters");
    let series = tree.path.join("Chaptered");
    write_pages(
        &series.join("Chaptered - c001").join("Chaptered - c001.5"),
        2,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.unattached_chapter_dirs, 1, "one question, not two");
    assert_eq!(report.works, 0);
}

#[test]
fn a_chapter_two_levels_below_a_volume_is_still_inside_it() {
    // Every ancestor is tested, not just the immediate parent. A volume whose
    // chapters sit in a `Chapters/` subfolder puts one directory the rules
    // give neither a volume nor a chapter between the two, so a check that
    // looked only one level up would let all three chapter directories back
    // in as members of the very work their grandparent forms.
    let tree = crate::tests::tempdir("derive-nested-deep");
    let inside = tree
        .path
        .join("Series")
        .join("Series - v01")
        .join("Chapters");
    write_pages(&inside.join("Series - v01 - c001"), 2);
    write_pages(&inside.join("Series - v01 - c002"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(report.members, 1, "the volume, two levels above them");
    assert_eq!(
        report.suppressed, 3,
        "the two chapter directories and the `Chapters` folder holding them — \
         that folder is no candidate, but it would have become a volume-less \
         work of its own, so leaving it out would under-report the drop"
    );
    let (path, pages): (String, i64) = conn
        .query_row("SELECT path, pages FROM member", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("member");
    assert!(path.ends_with("Series - v01"), "{path}");
    assert_eq!(pages, 4, "counted once");
}

#[test]
fn a_volume_less_container_does_not_become_a_member_beside_what_is_inside_it() {
    // **Ten pages for five.** Measured against the shipped binary on this
    // tree: one work with two members — `Series/Extras` at 5 pages and
    // `Series/Extras/Art Book` at 5 pages, the same five images counted
    // twice — and `suppressed = 0` to say nothing had been dropped.
    //
    // `nested_in_candidate` only suppressed under an ancestor that resolved a
    // volume or a chapter, and a volume-less container never does — while
    // `classify` admitted it as a member all the same. Every fixture in this
    // module gave its containers a volume marker, so no test had an entry
    // that resolved neither (whole-branch review, C3).
    let tree = crate::tests::tempdir("derive-extras");
    write_pages(&tree.path.join("Series").join("Extras").join("Art Book"), 5);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(
        report.members, 1,
        "the thing that holds the pages, not it and the directory above it"
    );
    assert_eq!(
        report.suppressed, 1,
        "and the entry that stopped being a member is counted, not dropped"
    );
    let (path, pages): (String, i64) = conn
        .query_row("SELECT path, pages FROM member", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("member");
    // **The inner one, and that direction is measured rather than picked.**
    // Taking the container instead satisfies the page count just as well and
    // is what the first fix did: over this project's own corpus it turned 107
    // members into 40, collapsing `incoming/bocchi`'s five editions into a
    // single 439-page member. A volume-less container is a name in a path;
    // what is inside it is the material.
    assert!(path.ends_with("Series/Extras/Art Book"), "{path}");
    assert_eq!(pages, 5, "five pages, counted once");
}

#[test]
fn three_levels_of_volume_less_containers_are_one_member() {
    // The same rule at depth, which is what makes it a rule rather than a
    // special case: every level of a volume-less chain was a member of its
    // own, so three levels produced three members and the same pages three
    // times over.
    let tree = crate::tests::tempdir("derive-extras-deep");
    write_pages(
        &tree
            .path
            .join("Series")
            .join("Extras")
            .join("Art Books")
            .join("Volume 1 Insert"),
        5,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.members, 1);
    assert_eq!(report.suppressed, 2, "the two levels beneath the member");
}

#[test]
fn a_broad_rule_that_reaches_the_root_does_not_swallow_the_library() {
    // A `prefix` rule with a `set_series` — which is exactly what
    // `proposals::accept` writes — resolves a series for the root's own entry
    // and for every container between it and a volume. Measured against the
    // shipped binary before the fix: every one of them became a member of one
    // work, so five pages were counted four times and the work carrying them
    // was titled with the **empty string**.
    //
    // Each of those containers now defers to what is inside it, and the
    // volume is what is left. Counted rather than merely skipped, so a rule
    // that really does say "every path is one series" is visible as a number
    // rather than as a library that quietly holds less than it did.
    let tree = crate::tests::tempdir("derive-root-defers");
    write_pages(&tree.path.join("Series").join("Series - v01"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_series)
         VALUES (99, 'prefix', '', 'path', 'Everything')",
        [],
    )
    .expect("a rule that gives every path one series");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(report.members, 1);
    assert_eq!(
        report.suppressed, 2,
        "the root and the series directory, each deferring to what is inside it"
    );
    let (path, title): (String, String) = conn
        .query_row(
            "SELECT member.path, work.title FROM member
             JOIN edition ON edition.id = member.edition_id
             JOIN work ON work.id = edition.work_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("member");
    assert!(path.ends_with("Series/Series - v01"), "{path}");
    assert_eq!(title, "Series - v01");
}

#[test]
fn a_root_that_is_itself_the_material_is_named_after_its_own_directory() {
    // The remaining route to the root becoming a member, and the reason
    // `title_of` needs an arm for `rel_path == ""`: a root holding page
    // images directly, with a `set_series` rule broad enough to admit it.
    // There is nothing inside it to defer to, so the root is the member —
    // and `title_of` used to take "the part after the last slash" of the
    // empty string and answer with it, producing a work with no title at all
    // and the slug `work`, which is `slugify`'s fallback for a name holding
    // nothing alphanumeric.
    let tree = crate::tests::tempdir("derive-root-material");
    write_pages(&tree.path, 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_series)
         VALUES (99, 'prefix', '', 'path', 'Everything')",
        [],
    )
    .expect("a rule that gives every path one series");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(report.members, 1);
    assert_eq!(report.suppressed, 0, "there is nothing under it");
    let (title, slug, pages): (String, String, i64) = conn
        .query_row(
            "SELECT work.title, work.slug, member.pages FROM work
             JOIN edition ON edition.work_id = work.id
             JOIN member ON member.edition_id = edition.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("work");
    let named = tree
        .path
        .file_name()
        .expect("the temp directory has a name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        title, named,
        "the root's own directory name, since its `rel_path` is the empty string"
    );
    assert_ne!(
        slug, "work",
        "which is the slug an empty title falls back to"
    );
    assert_eq!(pages, 3);
}

#[test]
fn every_volume_less_entry_in_a_series_is_its_own_work() {
    // **Eighteen pages in one sequence.** Measured against the shipped binary
    // on this tree: one work, four members, 18 pages, and a report saying
    // "1 works" with nothing at all signalling the merge.
    //
    // The group key was `(series, volume)`, so every entry the rules gave no
    // volume to shared one key — and member order is page order, which
    // `write_work`'s own doc calls out: every alignment offset after the
    // first member is computed from it. Four separate works with
    // `volume = NULL` is an answer a person can correct; one work with four
    // unrelated volumes concatenated is not (whole-branch review, C4).
    let tree = bare_numbered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 4);
    assert_eq!(report.members, 4);
    assert_eq!(report.merged, 0, "nothing joined anything");
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 4, "four rows, not one report of four members");
    let per_work: Vec<(String, i64, i64)> = conn
        .prepare(
            "SELECT work.title, count(member.id), sum(member.pages) FROM work
             JOIN edition ON edition.work_id = work.id
             JOIN member ON member.edition_id = edition.id
             GROUP BY work.id ORDER BY work.title",
        )
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        per_work,
        vec![
            ("001".to_string(), 1, 3),
            ("002".to_string(), 1, 4),
            ("003".to_string(), 1, 5),
            ("004".to_string(), 1, 6),
        ],
        "one member each, and each work's own pages"
    );
}

#[test]
fn two_differently_named_volume_less_directories_in_one_series_do_not_merge() {
    // The same defect at the scale it turns up in this corpus:
    // `Series/Omake` and `Series/Bonus` are different material, and neither
    // carries a volume marker, so they shared a group key and one page
    // sequence.
    let tree = crate::tests::tempdir("derive-omake");
    let series = tree.path.join("Series");
    write_pages(&series.join("Omake"), 2);
    write_pages(&series.join("Bonus"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 2);
    assert_eq!(report.members, 2);
    let titles: Vec<String> = conn
        .prepare("SELECT title FROM work ORDER BY title")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(titles, vec!["Bonus".to_string(), "Omake".to_string()]);
}

#[test]
fn a_second_volume_less_work_arriving_later_does_not_take_the_first_ones_row() {
    // **What keying volume-less entries individually costs, paid rather than
    // discovered.** `(series_id, volume)` is the address `write_work` probes
    // after `content_key` misses, and with four volume-less works in one
    // series it is no longer an address at all: `volume IS NULL` matches all
    // of them and `ORDER BY id LIMIT 1` hands every one of them the first
    // row. A work arriving later would take the row of the work that got
    // there first, with the reader's position on it.
    //
    // So a volume-less group is probed by the files it is made of, which is
    // the same thing that keys it. Removing that probe puts Bonus's content
    // on Omake's row.
    let tree = crate::tests::tempdir("derive-omake-later");
    let series = tree.path.join("Series");
    write_pages(&series.join("Omake"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let omake: i64 = conn
        .query_row("SELECT id FROM work WHERE title = 'Omake'", [], |r| {
            r.get(0)
        })
        .expect("work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 4)",
        [omake],
    )
    .expect("someone is four slots into the omake");

    write_pages(&series.join("Bonus"), 2);
    scan::scan_root(&conn, &root).expect("rescan");
    derive::derive(&conn).expect("second");

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2, "the arriving work is a second row");
    let path: String = conn
        .query_row(
            "SELECT member.path FROM member
             JOIN edition ON edition.id = member.edition_id
             WHERE edition.work_id = ?1",
            [omake],
            |r| r.get(0),
        )
        .expect("member");
    assert!(
        path.ends_with("Series/Omake"),
        "the first work still holds its own files: {path}"
    );
    let slot: i64 = conn
        .query_row(
            "SELECT slot FROM progress WHERE work_id = ?1",
            [omake],
            |r| r.get(0),
        )
        .expect("progress");
    assert_eq!(slot, 4, "and the position that was on it");
}

#[test]
fn two_entries_joining_one_edition_are_counted_as_a_merge() {
    // The merge that remains: `Chaptered - v03 - c9` and `c10` both resolve
    // volume 3, so they are two members of one page sequence — which is §7's
    // chapter case working as designed. It is still a merge, and the counter
    // principle does not have an exemption for merges that are meant: a
    // number nobody prints is a decision nobody can see.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(report.members, 2);
    assert_eq!(
        report.merged, 1,
        "one entry joined an edition another had already opened"
    );
}

#[test]
fn a_suppressed_entry_is_counted_rather_than_vanishing() {
    let tree = crate::tests::tempdir("derive-suppressed-count");
    let volume = tree.path.join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        crate::scan::tests::write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.members, 1);
    assert_eq!(report.suppressed, 3, "the three chapter directories");
}

#[test]
fn an_archive_inside_a_volume_directory_is_the_member_rather_than_the_directory() {
    // **Measured against the shipped binary on this exact tree**, before the
    // `pages > 0` tests below existed: `derived 1 series, 2 works, 2 editions,
    // 2 members, 2 suppressed`, and the database held ONE work — `v02`,
    // volume 2.0 — whose single member was the directory `Series/v02` with
    // ZERO pages. Both archives suppressed; nothing readable.
    //
    // Three steps, each of which the fixtures could not reach: the walk gives
    // a directory holding only archives `pages = Some(0)`; `builtin-volume`
    // reads `v01` from that directory's own name, so it became a member
    // candidate and suppressed the archive inside it; and `member_digest`
    // over a page-less directory is the digest of nothing — the *same* hash
    // for every such directory — so both groups' `content_key`s collided and
    // one row swallowed the other.
    let tree = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 2);
    assert_eq!(report.members, 2);
    assert_eq!(
        report.suppressed, 0,
        "a directory with no pages of its own suppresses nothing"
    );
    // **The row count as well as the report count**, because the two
    // disagreed: the report counted groups while the database held one row.
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2, "two rows, not one group counted twice");
    let members: Vec<(String, i64)> = conn
        .prepare("SELECT path, pages FROM member ORDER BY path")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(members.len(), 2, "{members:?}");
    assert!(members[0].0.ends_with("Series v01.cbz"), "{members:?}");
    assert_eq!(members[0].1, 3, "the archive's pages, not the directory's");
    assert!(members[1].0.ends_with("Series v02.cbz"), "{members:?}");
    assert_eq!(members[1].1, 4, "{members:?}");
}

#[test]
fn a_container_holding_no_pages_of_its_own_is_counted_rather_than_silently_dropped() {
    // The other half of the fix above, and the half the member paths cannot
    // see. `Series/v01` resolves a series and a volume and is admitted, so
    // something has to become of it — and what becomes of it is a count.
    // Without one, "the container stopped being a member" and "the container
    // was never looked at" read identically from the report, which is the
    // failure `DeriveReport` exists to prevent.
    let tree = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.unreadable, 2,
        "the two volume directories, which hold archives rather than pages"
    );
}

#[test]
fn a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let (id, key_before): (i64, String) = conn
        .query_row(
            "SELECT id, content_key FROM work WHERE volume = 1.0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 7)",
        [id],
    )
    .expect("progress");

    std::fs::write(
        tree.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01 (2019)")
            .join("003.jpg"),
        b"one more page",
    )
    .expect("write");
    // The member is the *directory*, and a directory's `mtime` moves when an
    // entry is added or removed — not when the bytes inside an existing one
    // change. So without this stamp `unchanged` skips the re-hash and the
    // digest stays what it was, which is the same second-resolution flake as
    // `a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive`. CI
    // caught both on 2026-08-19; both pass here.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    std::fs::File::open(
        tree.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01 (2019)"),
    )
    .expect("open the member directory")
    .set_times(std::fs::FileTimes::new().set_modified(later))
    .expect("stamp it as changed");
    scan::scan_root(&conn, &root).expect("rescan");
    let report = derive::derive(&conn).expect("second");

    let at_volume_one: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE volume = 1.0", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(at_volume_one, 1, "one work, not two");

    let (still, key_after): (i64, String) = conn
        .query_row(
            "SELECT id, content_key FROM work WHERE volume = 1.0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("work");
    assert_eq!(still, id, "the same row followed its files");
    assert_ne!(key_after, key_before, "and took the new key with it");
    assert_eq!(report.rekeyed, 1, "reported rather than silent");

    let slot: i64 = conn
        .query_row("SELECT slot FROM progress WHERE work_id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("progress survived");
    assert_eq!(slot, 7);
}

#[test]
fn a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive() {
    // Content decides before the known address, which is the order `sidecar`
    // has always used for `sidecar_path` and the order that keeps this from
    // being a hard error. A duplicate download gives two volumes the same
    // bytes and so the same `content_key`; probing the address first would
    // then write that key onto a second row and hit
    // `UNIQUE constraint failed: work.content_key`, failing the derive of an
    // entire library over one repeated file. Which work the duplicate lands
    // on is degenerate-corpus behaviour rather than a designed outcome — what
    // is pinned here is that the scan survives it.
    let tree = crate::tests::tempdir("derive-duplicate");
    write_pages(&tree.path.join("Alpha").join("Alpha - v01"), 3);
    let beta = tree.path.join("Beta").join("Beta - v01");
    write_pages(&beta, 5);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    // Beta's volume is replaced with a byte-for-byte copy of Alpha's — copied
    // explicitly rather than written by a second `write_pages` call, now that
    // `write_pages` bakes its own directory into every page's bytes
    // precisely so two calls can never collide by accident (Task 5/8 review).
    // Deliberate duplication belongs at the call site that wants it.
    let alpha = tree.path.join("Alpha").join("Alpha - v01");
    std::fs::remove_dir_all(&beta).expect("remove");
    std::fs::create_dir_all(&beta).expect("mkdir");
    for page in 0..3 {
        std::fs::copy(
            alpha.join(format!("{page:03}.jpg")),
            beta.join(format!("{page:03}.jpg")),
        )
        .expect("copy");
    }
    // **The rescan has to be able to see the change.** `unchanged` compares
    // `(size, mtime)`, a directory's size is its block size whether it holds
    // three entries or five, and `mtime` has second resolution — so a rebuild
    // that lands in the same second as the first scan is skipped and Beta keeps
    // the digest of its *old* five pages. The assertion below then compares
    // Alpha's hash against a stale one and fails, which is what CI did on
    // 2026-08-19 while this passed fifteen times running here.
    //
    // Stamped rather than slept: a `sleep(1)` would make this the slowest test
    // in the crate to fix a determinism problem, and would still be a race on a
    // filesystem with coarser stamps. This models what a real replacement is —
    // one that happens later — without waiting for it.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    std::fs::File::open(&beta)
        .expect("open the rebuilt directory")
        .set_times(std::fs::FileTimes::new().set_modified(later))
        .expect("stamp it as changed");
    scan::scan_root(&conn, &root).expect("rescan");

    derive::derive(&conn).expect("a repeated file is not a failed derive");

    // **Pins the collision itself, not only that the derive survived it.**
    // Without this, a derive that took some unrelated path — Beta simply
    // re-keyed at its own address, never touching Alpha's row at all — would
    // pass every assertion below just as well, proving nothing about the
    // `UNIQUE constraint failed: work.content_key` case this test exists
    // for. Found on review: distinct per-page bytes (Task 5/8) make this the
    // seventh place on this branch where a fixture reproduced a scenario
    // without a test ever checking it was reached.
    let (alpha_sha, beta_sha): (String, String) = (
        conn.query_row(
            "SELECT sha256 FROM scan_entry WHERE rel_path = 'Alpha/Alpha - v01'",
            [],
            |r| r.get(0),
        )
        .expect("alpha digest"),
        conn.query_row(
            "SELECT sha256 FROM scan_entry WHERE rel_path = 'Beta/Beta - v01'",
            [],
            |r| r.get(0),
        )
        .expect("beta digest"),
    );
    assert_eq!(
        alpha_sha, beta_sha,
        "the copy must actually collide with Alpha's content, or nothing below tests the \
         scenario this function exists for"
    );

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2, "and no third row was minted over it");

    // A third derive, because the state left behind is the one that puts two
    // works at a single `(series_id, volume)`: Beta's group landed on Alpha's
    // row by content and stamped Beta's address onto it, while the old Beta
    // row kept that address too. This is the only route to the hub lookup's
    // multi-row branch, so it is exercised rather than merely reasoned about.
    derive::derive(&conn).expect("and again over the state that leaves");

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2, "still two, not one more per derive");
}

/// The two `is_readable`s agree, for every format there is.
///
/// `taiyaku_read::Format::is_readable` is where the question is decided and
/// **has no caller in this workspace** — `derive.rs` cannot call it, because
/// `Format` has no parse-back from the `scan_entry.format` token and adding
/// one to answer a two-arm question would be the wrong trade. So the answer
/// is spelled a second time here, in tokens, and the doc on each side says
/// the other exists. What nothing did was check that they still say the same
/// thing: a third readable format, or a fourth, is a change in two places,
/// and the second one being missed is silent — a member the derive believes
/// unreadable, counted as a fault, and never offered to a reader that could
/// have opened it.
///
/// Enumerated rather than sampled, so a variant added to `Format` and not to
/// this list fails to compile rather than going unchecked.
#[test]
fn the_token_side_readability_test_agrees_with_the_enums_own() {
    use taiyaku_read::Format;

    for format in [
        Format::Dir,
        Format::Zip,
        Format::EpubFixed,
        Format::EpubReflow,
        Format::Pdf,
        Format::Rar,
        Format::SevenZip,
        Format::Tar,
        Format::Unknown,
    ] {
        assert_eq!(
            super::is_readable(format.token()),
            format.is_readable(),
            "{format:?}: the token-side answer and the enum's own must not drift"
        );
    }

    // A `format` column this scan never wrote — a row from before the column
    // existed, say — is not readable either, and takes the same arm
    // `Format::Unknown` does without needing one of its own.
    assert!(!super::is_readable(None));
}

/// Two roots holding the same volume produce **one work with two editions**,
/// and each edition's members carry the root they actually came from.
///
/// This is the shape `work.root_id` could not have represented, and the
/// reason spec C1 §5.1 put the column on `member` instead. If this test ever
/// reads one root id for both editions, the grouping has changed and the
/// filter is filtering something else.
#[test]
fn a_work_spanning_two_roots_gives_each_member_its_own_root() {
    let english = crate::tests::tempdir("derive-root-en");
    write_pages(&english.path.join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("derive-root-raw");
    write_pages(&raw.path.join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    let en_root = scan::add_root(&conn, &english.path).expect("adds the English root");
    let raw_root = scan::add_root(&conn, &raw.path).expect("adds the raw root");
    scan::scan_root(&conn, &en_root).expect("scans English");
    scan::scan_root(&conn, &raw_root).expect("scans raw");
    derive::derive(&conn).expect("derives");

    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT m.root_id FROM member m
               JOIN edition e ON e.id = m.edition_id
              ORDER BY m.root_id",
        )
        .expect("preparing the root read");
    let roots: Vec<Option<i64>> = stmt
        .query_map([], |row| row.get(0))
        .expect("reading roots")
        .collect::<rusqlite::Result<_>>()
        .expect("reading roots");

    assert_eq!(
        roots,
        vec![Some(en_root.id), Some(raw_root.id)],
        "each member carries the root its entry was walked from"
    );
}

/// A re-derive that no longer resolves a member must not leave the previous
/// derive's root id standing, for the same reason `write_chapters` clears
/// before it writes.
#[test]
fn a_rederive_rewrites_the_root_rather_than_leaving_the_old_one() {
    let dir = crate::tests::tempdir("derive-root-rewrite");
    write_pages(&dir.path.join("Series").join("Series - v01"), 2);

    // A second, real root to scribble with: `member.root_id` REFERENCES
    // `root(id)`, so a made-up id is refused by the constraint rather than
    // stored — which the first version of this test discovered by being
    // refused. Scribbling a root that genuinely exists is also the stronger
    // assertion, since it proves the re-derive writes the *right* root and
    // not merely a different one.
    let other = crate::tests::tempdir("derive-root-rewrite-other");
    write_pages(&other.path.join("Other").join("Other - v01"), 1);

    let library = Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("adds the root");
    let wrong_root = scan::add_root(&conn, &other.path).expect("adds the other root");
    scan::scan_root(&conn, &root).expect("scans");
    derive::derive(&conn).expect("derives");

    conn.execute("UPDATE member SET root_id = ?1", [wrong_root.id])
        .expect("scribbles the wrong root over every member");
    derive::derive(&conn).expect("re-derives");

    let wrong: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM member WHERE root_id <> ?1",
            [root.id],
            |row| row.get(0),
        )
        .expect("counting");
    assert_eq!(wrong, 0, "a re-derive rewrites every member's root");
}

#[test]
fn one_pass_answers_for_every_series_it_was_asked_about() {
    // `rel_paths_by_series` exists so `proposals::evidence` can ask about
    // both ends of a proposal — and about the rule beside them — without
    // paying for three passes over every `scan_entry` row in the library,
    // which is what it did. The property that makes the plural form
    // substitutable for the singular one is this: each title gets exactly
    // what a separate call would have given it, and a title nothing resolves
    // to is present and empty rather than missing.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");

    let asked = std::collections::BTreeSet::from([
        "Go-Toubun no Hanayome",
        "Kaguya-sama - Fan Translated",
        "A Series Nothing Here Is",
    ]);
    let together = derive::rel_paths_by_series(&conn, &asked).expect("one pass");

    assert_eq!(
        together.keys().map(String::as_str).collect::<Vec<_>>(),
        asked.iter().copied().collect::<Vec<_>>(),
        "every title asked about is answered, including the one with nothing under it",
    );
    for title in &asked {
        assert_eq!(
            together[*title],
            derive::rel_paths_for_series(&conn, title).expect("one at a time"),
            "`{title}` is answered the same either way",
        );
    }
    assert!(
        !together["Go-Toubun no Hanayome"].is_empty()
            && !together["Kaguya-sama - Fan Translated"].is_empty(),
        "and two of the three genuinely have entries, or this compares empty against empty: \
         {together:?}",
    );
    assert!(together["A Series Nothing Here Is"].is_empty());
}

/// One `work` row, one `edition` and one `member` per `(edition key, path,
/// sha256)` triple, every member's `root_id` pointing at root 1 — the shape a
/// scan leaves behind. `seed_work_without_root` is the same shape with
/// `root_id` left NULL, which is what an imported work (no root at all) and a
/// work whose root was later removed (`ON DELETE SET NULL`) both look like.
///
/// **`path` is given explicitly, not derived from `name`/`key`.** `superseded`
/// matches a candidate's members against a target's by `path` as well as by
/// `sha256` (C1), so a fixture meaning "this file moved from one work's
/// edition to another's" has to give the two members the identical path a
/// real fold leaves behind — the file never moves on disk, only the
/// classification of who owns it does — while a fixture meaning "coincidence,
/// not a fold" has to give them different ones.
///
/// `content_key` and `slug` are both `name`, which is fine: they are unique
/// on different columns, and every caller in this module uses a distinct
/// `name` per work.
fn seed_work(conn: &Connection, work_id: i64, name: &str, editions: &[(&str, &str, &str)]) {
    seed_work_at(conn, work_id, name, editions, Some(1));
}

/// As [`seed_work`], but with no root at all — see its doc.
fn seed_work_without_root(
    conn: &Connection,
    work_id: i64,
    name: &str,
    editions: &[(&str, &str, &str)],
) {
    seed_work_at(conn, work_id, name, editions, None);
}

fn seed_work_at(
    conn: &Connection,
    work_id: i64,
    name: &str,
    editions: &[(&str, &str, &str)],
    root_id: Option<i64>,
) {
    let hub = editions.first().map_or("raw", |(key, _, _)| *key);
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (?1, ?2, ?2, ?2, 'rtl', ?3)",
        rusqlite::params![work_id, name, hub],
    )
    .expect("seeding a work");
    for (key, path, sha256) in editions {
        conn.execute(
            "INSERT INTO edition (work_id, key, label) VALUES (?1, ?2, ?2)",
            rusqlite::params![work_id, key],
        )
        .expect("seeding an edition");
        let edition_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO member (edition_id, ordinal, path, sha256, pages, root_id)
             VALUES (?1, 0, ?2, ?3, 1, ?4)",
            rusqlite::params![edition_id, path, sha256, root_id],
        )
        .expect("seeding a member");
    }
}

/// A work whose every member is now held by a work this derive wrote.
///
/// **Not "a work with zero editions", which never occurs.** Under
/// `Scope::Scan`, `import::replace_editions` deletes only the editions the
/// incoming document names, and the superseded work is not in `groups` at all
/// — so nothing empties it. Measured 2026-08-27: after the fold, the two
/// superseded works still held their editions over the same files.
///
/// **The covering member shares work 2's exact path.** That is what a fold
/// actually leaves behind — see `superseded`'s own doc on C1 — and it is
/// deliberate here rather than incidental: `a_byte_identical_file_at_a_different_path_is_not_superseded`
/// is the fixture that isolates what happens when it does not.
#[test]
fn a_work_whose_material_all_moved_is_superseded() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");

    // Work 1 keeps its own `raw` and gains `full-color` over the same archive
    // work 2 holds — same path *and* digest; work 2 is the one the fold
    // supersedes.
    seed_work(
        &conn,
        1,
        "target",
        &[
            ("raw", "target/raw", "aaa"),
            ("full-color", "archive/v01", "bbb"),
        ],
    );
    seed_work(&conn, 2, "superseded", &[("raw", "archive/v01", "bbb")]);

    let written = BTreeSet::from([1_i64]);
    let written_editions = BTreeSet::from([(1_i64, "full-color".to_string())]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert_eq!(found, vec![(2, 1)], "work 2's material is all under work 1");
}

/// Half a work's material moving is not supersession.
#[test]
fn a_work_that_kept_some_of_its_material_is_not_superseded() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "target", &[("full-color", "archive/v01", "bbb")]);
    seed_work(
        &conn,
        2,
        "kept",
        &[("raw", "archive/v01", "bbb"), ("raw2", "kept/raw2", "ccc")],
    );

    let written = BTreeSet::from([1_i64]);
    let written_editions = BTreeSet::from([(1_i64, "full-color".to_string())]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert!(
        found.is_empty(),
        "ccc is held by nothing else, got {found:?}"
    );
}

/// An imported work is never superseded by a scan.
#[test]
fn a_work_with_no_root_is_left_alone() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "target", &[("full-color", "archive/v01", "bbb")]);
    seed_work_without_root(&conn, 2, "imported", &[("raw", "archive/v01", "bbb")]);

    let written = BTreeSet::from([1_i64]);
    let written_editions = BTreeSet::from([(1_i64, "full-color".to_string())]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert!(
        found.is_empty(),
        "an imported work has no root, got {found:?}"
    );
}

/// The same absolute file reachable under two registered roots is not
/// evidence of a fold, even though it satisfies every other condition.
///
/// `scan::add_root` canonicalises and inserts with no containment check, and
/// `scan_entry` is unique on `(root_id, rel_path)` — so registering `/r` and
/// then `/r/extras` walks everything under `extras` twice, as two rows with
/// different `rel_path`s but the identical absolute `path` and `sha256`. The
/// rules can resolve those two rows into two different works. Nothing here
/// moved: both `root` rows still exist, so condition 4 excludes neither side,
/// and `path`/`sha256` alone match exactly as they would for a real fold.
/// Only the two members' `root_id`s tell them apart — work 1's covering
/// member sits under root 2, work 2's own member sits under root 1.
#[test]
fn nested_roots_covering_the_same_path_are_not_superseded() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, '/r'), (2, '/r/extras')",
        [],
    )
    .expect("roots");

    // Same absolute path, same digest — what nested registration leaves
    // behind — but each member sits under a different root.
    seed_work_at(
        &conn,
        1,
        "target",
        &[("full-color", "/r/extras/v01", "bbb")],
        Some(2),
    );
    seed_work_at(
        &conn,
        2,
        "unrelated-duplicate",
        &[("raw", "/r/extras/v01", "bbb")],
        Some(1),
    );

    let written = BTreeSet::from([1_i64]);
    let written_editions = BTreeSet::from([(1_i64, "full-color".to_string())]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert!(
        found.is_empty(),
        "same path and digest under two different roots is not a fold, got {found:?}"
    );
}

/// A byte-identical file at a different path is not evidence of a fold — C1.
///
/// Without the `path` half of the match this is indistinguishable from
/// `a_work_whose_material_all_moved_is_superseded`: same `sha256`, held by a
/// written work, root intact. The difference is that nothing here actually
/// moved — `live-duplicate`'s file sits where it always has — and that is
/// exactly the case `superseded`'s own doc lists three ordinary routes into:
/// an unreadable subtree, a rule edit that suppresses entries, or a failed
/// re-hash can each hand a fully live work this same "written work holds an
/// identical digest" shape. A false positive here is `DELETE FROM work` on a
/// live row.
#[test]
fn a_byte_identical_file_at_a_different_path_is_not_superseded() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "target", &[("raw", "archive/target", "dup")]);
    // Not written, root intact: a fully live work that happens to hold the
    // same bytes at a path of its own.
    seed_work(
        &conn,
        2,
        "live-duplicate",
        &[("raw", "elsewhere/live", "dup")],
    );

    let written = BTreeSet::from([1_i64]);
    let written_editions = BTreeSet::from([(1_i64, "raw".to_string())]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert!(
        found.is_empty(),
        "same bytes, different path — nothing moved, got {found:?}"
    );
}

/// A stale edition on a written work is not evidence of a fold — C2.
///
/// Work 1 is written this pass, but only its `full-color` edition is: `raw`
/// is exactly `DeriveReport::kept_editions`'s shape, a row a rule edit
/// orphaned that this derive left standing rather than deleting. Work 2's
/// only member matches `raw`'s path and digest, not `full-color`'s, so it
/// must not read as covered — the tie-break that used to sit here preferred
/// exactly this kind of row, being the smaller (so older, so likelier-stale)
/// work id.
#[test]
fn a_stale_edition_on_a_written_work_is_not_evidence() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(
        &conn,
        1,
        "target",
        &[
            ("raw", "archive/v01", "bbb"),
            ("full-color", "target/full-color", "aaa"),
        ],
    );
    seed_work(&conn, 2, "superseded", &[("raw", "archive/v01", "bbb")]);

    let written = BTreeSet::from([1_i64]);
    // Only `full-color` was actually written this pass — `raw` is standing
    // from an earlier one, exactly as `kept_editions` describes.
    let written_editions = BTreeSet::from([(1_i64, "full-color".to_string())]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert!(
        found.is_empty(),
        "the only covering member sits on an edition this derive did not write, got {found:?}"
    );
}

/// The single target is found by intersecting per-member holder sets, not by
/// a first-write-wins map from digest to one "owner" — I1.
///
/// Work 1 and work 5 are both written. Work 1 holds only `A`; work 5 holds
/// both `A` and `B`, its `A` at the identical path and digest work 1's copy
/// carries. A map that assigns each digest to a single owner resolves
/// `A → 1` and `B → 5`; those disagree, so it refuses work 2 even though
/// work 5 alone covers everything work 2 holds. Intersecting the holder sets
/// `{1, 5}` (of `A`) and `{5}` (of `B`) finds it: `{5}`.
#[test]
fn a_target_found_by_intersection_a_single_owner_map_would_miss() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "narrow", &[("raw", "shared/a", "aaa")]);
    seed_work(
        &conn,
        5,
        "wide",
        &[("raw", "shared/a", "aaa"), ("full-color", "wide/b", "bbb")],
    );
    seed_work(
        &conn,
        2,
        "superseded",
        &[("raw", "shared/a", "aaa"), ("full-color", "wide/b", "bbb")],
    );

    let written = BTreeSet::from([1_i64, 5_i64]);
    let written_editions = BTreeSet::from([
        (1_i64, "raw".to_string()),
        (5_i64, "raw".to_string()),
        (5_i64, "full-color".to_string()),
    ]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert_eq!(
        found,
        vec![(2, 5)],
        "work 5 alone covers both of work 2's members"
    );
}

/// Two written works covering a candidate's one member identically leave the
/// candidate standing — condition 5, and the only fixture that gives it
/// anything to check.
///
/// Every other fixture in this module produces an intersection of size 0 or
/// 1, so `candidates.filter(|c| c.len() == 1)` and a hypothetical
/// `candidates.filter(|c| !c.is_empty())` agree on every one of them —
/// the latter would additionally take the lowest work id as an arbitrary
/// target, exactly the tie-break an earlier fix round was raised to remove.
/// Here work 1 and work 3 both hold `shared/a`/`aaa` under root 1, so the
/// intersection across the candidate's single member is `{1, 3}`: no exact
/// answer, so nothing is reported.
#[test]
fn two_works_covering_the_same_material_leave_the_candidate_standing() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "alpha", &[("raw", "shared/a", "aaa")]);
    seed_work(&conn, 3, "beta", &[("raw", "shared/a", "aaa")]);
    seed_work(&conn, 2, "candidate", &[("raw", "shared/a", "aaa")]);

    let written = BTreeSet::from([1_i64, 3_i64]);
    let written_editions = BTreeSet::from([(1_i64, "raw".to_string()), (3_i64, "raw".to_string())]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert!(
        found.is_empty(),
        "work 1 and work 3 both cover the candidate — no single target, got {found:?}"
    );
}

/// The only fixture here where a written work that is *not* the target sits
/// in the written set at all — every other fixture has exactly one written
/// work, which also happens to be the target.
///
/// This does not isolate condition 1 on its own: removing the
/// `written.contains(&work_id)` exclusion would make work 1 join `found` as
/// `(1, 1)` too, through the same degenerate self-supersession every other
/// fixture already fails through once that exclusion is gone. What this
/// fixture actually adds is narrower and is not covered elsewhere — work 5's
/// own written material sits in `digest_holders` alongside work 1's, and
/// proving that a candidate with nothing to do with work 5 is still found
/// (and found as `(2, 1)`, not refused or misdirected) shows that unrelated
/// written material does not perturb the intersection.
#[test]
fn a_written_work_holding_unrelated_material_is_never_a_candidate() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "target", &[("full-color", "archive/v01", "bbb")]);
    seed_work(&conn, 5, "unrelated", &[("raw", "unrelated/v01", "zzz")]);
    seed_work(&conn, 2, "superseded", &[("raw", "archive/v01", "bbb")]);

    let written = BTreeSet::from([1_i64, 5_i64]);
    let written_editions = BTreeSet::from([
        (1_i64, "full-color".to_string()),
        (5_i64, "raw".to_string()),
    ]);
    let found = super::superseded(&conn, &written, &written_editions).expect("detecting");
    assert_eq!(
        found,
        vec![(2, 1)],
        "work 5's own material must not make it a candidate, got {found:?}"
    );
}

/// One truth page per number in `pages`, each carrying one box, plus the
/// digest the labelling was drawn against — written through
/// `state::put_truth` so the rows are the shape the labelling client
/// produces rather than a hand-built approximation.
fn seed_truth_pages(conn: &Connection, work_id: i64, edition: &str, pages: std::ops::Range<u32>) {
    let mut doc = taiyaku_core::TruthDoc::new();
    doc.editions.insert(
        edition.to_string(),
        taiyaku_core::TruthEdition {
            members: vec![taiyaku_core::RegionMember {
                sha256: "0".repeat(64),
            }],
            pages: pages
                .map(|page| {
                    (
                        page,
                        vec![taiyaku_core::TruthRegion {
                            x: 0.1,
                            y: 0.2,
                            w: 0.3,
                            h: 0.4,
                            axis: None,
                            kind: taiyaku_core::Kind::Dialogue,
                            ignore: false,
                            text: None,
                        }],
                    )
                })
                .collect(),
        },
    );
    crate::state::put_truth(conn, work_id, &doc).expect("the labelling");
}

/// A reader's note on `edition`, one per id given.
fn seed_notes(conn: &Connection, work_id: i64, user_id: i64, edition: &str, ids: &[&str]) {
    crate::state::put_notes(
        conn,
        work_id,
        user_id,
        &crate::Notes {
            work: "ignored".to_string(),
            notes: ids
                .iter()
                .map(|id| crate::Note {
                    id: (*id).to_string(),
                    edition: edition.to_string(),
                    slot: 2,
                    region: None,
                    color: None,
                    body: format!("the sign, note {id}"),
                    created: "2026-08-27T09:00:00Z".to_string(),
                    updated: "2026-08-27T09:00:00Z".to_string(),
                })
                .collect(),
        },
    )
    .expect("the reader's notes");
}

/// The two directories the two tests below both start from: a base series and
/// a Full Color one, scanned and derived into two separate works, with the id
/// of the colour work returned.
fn two_works_before_the_fold(tree: &crate::tests::TempDir, conn: &Connection) -> i64 {
    write_pages(&tree.path.join("Series").join("Series - v01"), 12);
    write_pages(
        &tree
            .path
            .join("Series - Full Color")
            .join("Series - Full Color - v01"),
        12,
    );
    let root = scan::add_root(conn, &tree.path).expect("add");
    // Migration 012 defaults `keeps_documents` off; two of the four tests
    // built on this fixture hand-edit the colour work and need its document
    // to actually reach disk, so this opts in — harmless for the other two,
    // which never call `hand::save`.
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("the fixture keeps documents");
    scan::scan_root(conn, &root).expect("scan");

    let first = derive::derive(conn).expect("first derive");
    assert_eq!(first.works, 2, "two directories, two series, two works");
    assert_eq!(
        first.emptied_works, 0,
        "nothing has folded yet — both works were just written"
    );

    conn.query_row(
        "SELECT edition.work_id FROM member
           JOIN edition ON edition.id = member.edition_id
          WHERE member.path LIKE '%Full Color%'",
        [],
        |r| r.get(0),
    )
    .expect("the colour work")
}

/// The rule `proposals::accept` writes for exactly this shape: fold the Full
/// Color directory into the base series as a `full-color` edition.
fn write_the_fold_rule(conn: &Connection) {
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_series, set_edition_key)
         VALUES (99, 'prefix', 'Series - Full Color/', 'path', 'Series', 'full-color')",
        [],
    )
    .expect("the fold rule");
}

/// `derive`'s own wiring from `write_work`'s returned id through
/// `written`/`written_editions` to `superseded`, and on into `fold::absorb`.
///
/// The `superseded` tests above all call the function directly with a
/// hand-built `written` set and `fold`'s own tests seed their two works by
/// `INSERT`, so none of them would notice `write_work` returning the wrong
/// id, or `derive` never populating `written` or `written_editions` at all,
/// or the fold never being called. This is the one test that goes through
/// `derive()` itself, over real scanned directories: two directories start as
/// two separate series and two separate works, and a `set_series` +
/// `set_edition_key` rule — the same shape `proposals::accept` writes, and
/// the one the "O73's cascade, measured" reproduction uses against real
/// archives — folds the second into the first as a new edition on the next
/// derive.
///
/// **The reader is planted before the fold and read back after it**, through
/// `state`'s own writers rather than by `INSERT`, because the whole claim of
/// this branch is that their place survives a rescan they never asked for.
///
/// **Every counter gets its own number, and that is the point of the
/// fixture's shape.** `derive` fans one `fold::Moved` out into eight report
/// fields, and this is the only test that goes through that fan-out. An
/// earlier version of it planted one position and one note and asserted
/// `(1, 0, 1, 1)` — so swapping the two assignments passed, and
/// `moved_truth_pages`, `moved_truth_collisions` and `discarded_rows` were
/// asserted nowhere at all: deleting `report.discarded_rows += …` broke
/// nothing. Two readers who move, three notes, four pages moved, five pages
/// the target already held, six discarded rows, seven readings destroyed and
/// eight identical note copies dropped make every one of the eight pinned by
/// the number in it rather than by luck.
///
/// **`lost_positions` was the last one still exempt from that rule**, and
/// asserting it as `0` was the exemption: `usize::default()` is `0`, so
/// deleting `report.lost_positions += moved.lost_positions;` from the fan-out
/// left the assertion green. It said "none was lost" and pinned "the counter
/// was never reached" equally well — which is the one thing a counter for a
/// destroyed reading must not do. Seven readers therefore arrive holding a
/// *newer* place on the target's mapped edition than the one they hold on the
/// work about to be folded, which is exactly the shape
/// `fold::move_positions`' `WHERE excluded.updated > progress.updated` makes
/// a silent no-op, and `Moved::lost_positions` exists to see.
///
/// **`dropped_notes` is the eighth field, and O78 added it.** Reader 2 holds
/// eight notes on both works in identical copies, so the fold drops eight and
/// moves none of them — a number no other field here carries — and the
/// target's own eight rows are read back by `rowid`, because a dropped copy
/// and a dropped original leave the same columns behind.
#[test]
fn deriving_after_a_fold_moves_the_reader_onto_the_target_and_deletes_the_work() {
    let tree = crate::tests::tempdir("derive-fold-emptied");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);
    let base: i64 = conn
        .query_row("SELECT id FROM work WHERE id <> ?1", [colour], |r| r.get(0))
        .expect("the base work");

    // Two readers whose places move, so `moved_positions` cannot be the same
    // number as anything else in the report: `progress` is keyed per user per
    // edition, so one reader can only ever contribute one row here, and
    // `move_positions` counts rows it actually wrote.
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'user')",
        [],
    )
    .expect("a second reader");
    for user in [1, 2] {
        crate::state::put_progress(
            &conn,
            colour,
            user,
            &crate::Progress {
                edition: "raw".into(),
                slot: 2,
            },
        )
        .expect("the reader's place");
    }
    // Seven more who lose one. Each holds a place on the work about to be
    // folded *and* a newer one on the target, under the very edition key the
    // fold maps onto — the grandfathered row `move_positions` documents,
    // legal because `progress.edition` has no foreign key to `edition(key)`.
    // Newer wins, so their place on the folded work is skipped by the upsert
    // and destroyed by the `DELETE`: seven readings that existed before this
    // derive and do not exist after it, and nothing but `lost_positions` says
    // so.
    for user in 3..=9 {
        conn.execute(
            "INSERT INTO user (id, name, password_hash, role)
             VALUES (?1, 'reader ' || ?1, '!', 'user')",
            [user],
        )
        .expect("a reader who is about to lose a place");
        crate::state::put_progress(
            &conn,
            colour,
            user,
            &crate::Progress {
                edition: "raw".into(),
                slot: 60,
            },
        )
        .expect("the place on the work about to be folded");
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot, updated)
             VALUES (?1, ?2, 'full-color', 90, '2099-01-01 00:00:00')",
            [user, base],
        )
        .expect("the newer place they already hold on the target");
    }
    seed_notes(
        &conn,
        colour,
        1,
        "raw",
        &[
            "01JEXAMPLE0000000000000001",
            "01JEXAMPLE0000000000000002",
            "01JEXAMPLE0000000000000003",
        ],
    );
    // Eight notes reader 2 holds on both works, identical copies of each
    // other: `seed_notes` writes one slot, no region, no colour, a body per id
    // and one pair of timestamps, and the target's copies sit under the key
    // the fold maps `raw` onto — grandfathered there the way the truth pages
    // below are. O78: the fold drops the eight on the folded work and keeps
    // the target's, and none of them is a note moved.
    let identical: Vec<String> = (1..=8)
        .map(|n| format!("01JIDENTICAL0000000000000{n}"))
        .collect();
    let identical: Vec<&str> = identical.iter().map(String::as_str).collect();
    seed_notes(&conn, colour, 2, "raw", &identical);
    seed_notes(&conn, base, 2, "full-color", &identical);
    let targets_copies: Vec<i64> = conn
        .prepare("SELECT rowid FROM note WHERE user_id = 2 AND work_id = ?1 ORDER BY id")
        .and_then(|mut s| {
            s.query_map([base], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .expect("the target's copies");
    // Nine labelled pages on the work that is about to be folded away, and
    // five of the same nine already labelled on the target under the edition
    // the fold will map onto — so four move and five are left as the target's.
    // The target holds them under an edition key it does not yet carry, which
    // is legal (`truth_page.edition` has no foreign key) and is what the next
    // derive is about to create.
    seed_truth_pages(&conn, colour, "raw", 0..9);
    seed_truth_pages(&conn, base, "full-color", 0..5);
    // Six rows the fold counts and does not move — `fold::DISCARDED`'s first
    // predicate that is a plain `work_id`, six times over.
    for tag in 1..=6 {
        conn.execute(
            "INSERT INTO tag (id, name) VALUES (?1, 'tag ' || ?1)",
            [tag],
        )
        .expect("a tag");
        conn.execute(
            "INSERT INTO work_tag (work_id, tag_id) VALUES (?1, ?2)",
            [colour, tag],
        )
        .expect("a tag on the work about to be folded");
    }

    write_the_fold_rule(&conn);

    let second = derive::derive(&conn).expect("second derive");
    assert_eq!(
        second.works, 1,
        "one group now — both directories resolve series `Series`, volume 1.0"
    );
    assert_eq!(
        (
            second.emptied_works,
            second.unfolded_works,
            second.moved_positions,
            second.moved_notes,
            second.moved_truth_pages,
            second.moved_truth_collisions,
            second.discarded_rows,
            second.lost_positions,
            second.dropped_notes,
        ),
        (1, 0, 2, 3, 4, 5, 6, 7, 8),
        "the Full Color work folded away; two places and three notes moved, \
         four labelled pages moved and five stayed the target's, six rows \
         the fold does not move were counted before the delete took them, \
         seven readings the newer target place overrode stopped existing, \
         and eight identical note copies were dropped rather than moved"
    );
    assert!(
        second.unfoldable.is_empty(),
        "nothing was refused, so nothing is named: {:?}",
        second.unfoldable
    );

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 1, "the superseded work is gone");
    let survivor: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the one work left");
    assert_ne!(survivor, colour, "and it is the target, not the candidate");

    let held: Vec<(String, u32)> = crate::state::progress(&conn, survivor, 1)
        .expect("reading the position back")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect();
    assert_eq!(
        held,
        vec![("full-color".to_string(), 2)],
        "the reader's place is on the target, under the edition holding the \
         archive they were actually reading"
    );
    let notes: Vec<(i64, String)> = conn
        .prepare("SELECT work_id, edition FROM note WHERE user_id = 1 ORDER BY id")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .expect("the notes");
    assert_eq!(
        notes,
        vec![(survivor, "full-color".to_string()); 3],
        "and so are all three of their notes, on the same edition"
    );
    let second_readers: Vec<(i64, i64)> = conn
        .prepare("SELECT rowid, work_id FROM note WHERE user_id = 2 ORDER BY id")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .expect("reader 2's notes");
    assert_eq!(
        second_readers,
        targets_copies
            .iter()
            .map(|rowid| (*rowid, survivor))
            .collect::<Vec<_>>(),
        "reader 2 is left holding the target's own eight rows, under their own \
         rowids, and none of the copies from the folded work"
    );
}

/// The other end of the same wiring: a fold `absorb` refuses is counted,
/// named, and leaves everything standing — through `derive()` rather than
/// through `fold`'s own tests.
///
/// **The refusal here is the reader-row one**, which is the reachable shape:
/// a note on an edition key the work no longer holds is a *supported* state —
/// `taiyaku-serve`'s `notes` module grandfathers exactly this so it survives
/// later writes — and `progress.edition`, `note.edition`, `truth_page.edition`
/// and `truth_member.edition` are plain `TEXT` with no foreign key to
/// `edition(key)`. Every mover in `fold` is keyed on a `from_key` out of the
/// `edition` table, so before the guard this note was moved by nothing,
/// counted by nothing, and taken by the `DELETE FROM work`.
///
/// The message travelling out in `DeriveReport::unfoldable` is the other half:
/// a count tells an operator that something was refused and not which work to
/// go and look at.
#[test]
fn deriving_leaves_a_work_standing_when_the_fold_is_refused_and_names_it() {
    let tree = crate::tests::tempdir("derive-fold-refused");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);

    // A rule edit renamed this work's edition key some derives ago; the note
    // still names the old one, which is what `notes.rs` promises it may.
    seed_notes(
        &conn,
        colour,
        1,
        "the-key-a-rule-edit-dropped",
        &["01JEXAMPLE0000000000000001"],
    );

    write_the_fold_rule(&conn);

    let second = derive::derive(&conn).expect("second derive");
    assert_eq!(
        (second.emptied_works, second.unfolded_works),
        (0, 1),
        "the fold was refused, so nothing was folded away"
    );
    assert_eq!(second.unfoldable.len(), 1, "{:?}", second.unfoldable);
    let why = &second.unfoldable[0];
    assert!(
        why.contains(&colour.to_string()) && why.contains("the-key-a-rule-edit-dropped"),
        "the refusal must name the work to look at and the key stranded on it: {why}"
    );

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2, "both works still stand, visibly duplicated");
    let (note_work, note_edition): (i64, String) = conn
        .query_row("SELECT work_id, edition FROM note", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the note");
    assert_eq!(
        (note_work, note_edition.as_str()),
        (colour, "the-key-a-rule-edit-dropped"),
        "and the note nobody could map is exactly where the reader left it"
    );
}

/// **O80: a title-only edit must not claim the volume and series derivation
/// computed.**
///
/// `librarydoc::of` projects `title_hand` — migration 010 gave title a
/// provenance column for exactly this reason — but plain `volume` and plain
/// `series_id`, which are whatever the last derive wrote. So the document
/// every save writes claims a *derived* volume as though a person had chosen
/// it, `hand::save` arms the `LIBRARY` marker on any non-empty document, and
/// `write_work`'s guard then stops derivation ever touching `work.volume`,
/// `work.series_id`, `edition.kind` or `edition.language` again.
///
/// `MetadataEditor.tsx` sends `{ title }` unconditionally on Save, and title
/// is the only field the shipped UI edits — so pressing Save, even with no
/// change at all, was enough. The state left no way back: no route can clear
/// a field, because `#[serde(default)] Option<T>` makes an absent key and an
/// explicit `null` identical on the wire.
///
/// This asserts the root cause rather than the symptom: a document describing
/// a work nobody has edited must be empty of the four guarded fields.
#[test]
fn a_title_only_edit_claims_no_volume_and_no_series() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row(
            "SELECT w.id FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0",
            [],
            |r| r.get(0),
        )
        .expect("the volume 1 work");

    let fresh = crate::librarydoc::of(&conn, work_id).expect("projecting");
    assert_eq!(
        fresh.volume, None,
        "a work nobody has edited must project no hand volume — this claimed \
         derivation's own answer, which is what froze it"
    );
    assert!(
        fresh.series.titles.is_empty(),
        "and no hand series: {:?}",
        fresh.series
    );
    assert!(
        fresh.is_empty(),
        "so the whole document is empty, and `hand::save` arms no marker for it"
    );

    // The edit the shipped UI actually performs.
    let mut doc = fresh;
    doc.title = Some("Corrected Title".into());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    let after = crate::librarydoc::of(&conn, work_id).expect("re-projecting");
    assert_eq!(
        after.title.as_deref(),
        Some("Corrected Title"),
        "the title edit is recorded"
    );
    assert_eq!(
        after.volume, None,
        "and it still claims no volume — a title correction withdraws nothing"
    );
}

/// **Where the sweep runs, and the ordering that makes it work.** `derive`
/// already walks every work and already calls `hand::load` per work; the
/// reconciliation runs at the end of the same pass, after `write_roots`.
/// Before that point `member.root_id` is still NULL on a freshly minted work,
/// `path_for`'s `JOIN root` matches nothing, and every work in a new library
/// would report as having no address at all.
///
/// **What this test asserted before task 7, and what it asserts now.** This
/// reaches the identical rekey `a_repacked_volume_takes_its_document_with_it`
/// exercises through a real archive rewrite, but by corrupting the digest a
/// layer lower instead — see the comment at the `UPDATE scan_entry` below for
/// why that is needed to get past `import::scanned`'s own self-healing.
/// Before `hand::rename_for_rekey` existed this asserted the strand:
/// `reconcile` correctly reported the document orphaned, because nothing yet
/// moved it — hence the name. Now the document is carried to its new address
/// before `reconcile` ever sees a gap, and a second technique reaching the
/// same rekey is worth keeping alongside the archive-rewrite test: a fix that
/// works for one route to a changed digest and not the other is exactly the
/// shape a single fixture would hide.
#[test]
fn a_rekeyed_digest_takes_its_document_with_it() {
    let dir = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    // Migration 012 defaults `keeps_documents` off; this test is about the
    // document surviving a rekey, so it opts in.
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("the fixture keeps documents");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("a work was derived");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    // The volume's bytes change under it: a repack. `digest` (in this same
    // module) trusts a non-NULL `scan_entry.sha256` unconditionally rather
    // than re-hashing the file — the laziness the module doc describes — so
    // corrupting `work.content_key` directly would just be undone by
    // `import::scanned`'s own address-based recovery on the very next
    // derive, which resolves this row by `(series_id, volume)` and writes
    // its freshly (and, since the file is untouched, identically) computed
    // key straight back — self-healing before `reconcile` ever runs.
    // Corrupting the digest a level lower reaches past that recovery: the
    // second derive computes a genuinely different `content_key` from it,
    // the row is rekeyed to that new value, and the document saved under the
    // old one is addressed by nothing.
    let member_path: String = conn
        .query_row(
            "SELECT member.path FROM member
               JOIN edition ON edition.id = member.edition_id
              WHERE edition.work_id = ?1",
            [work_id],
            |row| row.get(0),
        )
        .expect("the work's one member");
    let rel_path = member_path
        .strip_prefix(&format!("{}/", dir.path.to_string_lossy()))
        .expect("the member sits under the root")
        .to_string();
    conn.execute(
        "UPDATE scan_entry SET sha256 = 'moved' WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root.id, rel_path],
    )
    .expect("rekeying");

    let report = derive::derive(&conn).expect("second derive");

    assert_eq!(report.rekeyed, 1, "the work followed its files: {report:?}");
    assert_eq!(
        report.orphan_documents.len(),
        0,
        "rename_for_rekey carried the document to its new address, rather \
         than leaving it stranded under the old key: {report:?}"
    );

    let path = crate::hand::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    assert!(
        path.is_file(),
        "the document is at the new address: {}",
        path.display()
    );
}

/// **The undocumented reason the `hand::reconcile` loop sits last** —
/// after the fold, not merely after `write_roots`.
///
/// `migrated_document.work_id` is `ON DELETE CASCADE`, and `fold::absorb`'s
/// own `DELETE FROM work` is the only place this project ever takes that
/// cascade. The moment `colour` below is folded away, its `migrated_document`
/// row goes with it — so `hand::reconcile`'s `owed` query, which reads that
/// table, stops naming `colour` at all. Its `.library.json` file is
/// untouched by the fold (nothing in `fold::absorb` deletes a file), so it is
/// still sitting on disk, and it is now a document that nothing in the
/// library addresses: exactly [`crate::hand::Orphan`].
///
/// **Reconciling before the fold would miss it.** Moved earlier, the loop
/// would run while `colour` still has its row, so its address is still in
/// `expected`, the file is still `present`, the two match, and nothing is
/// reported — the orphan surfaces only on some *later* pass, after something
/// else disturbs the work it was folded into. Moving the `reconcile` loop up
/// to just after the `for group in groups.into_values()` loop — still after
/// `write_roots`, which is the *other* reason this loop is last — was
/// hand-verified to turn this test's `orphan_documents` assertion red while
/// leaving every other test in this module green, including the one just
/// above.
#[test]
fn folding_a_work_away_orphans_its_hand_document_on_the_same_pass() {
    let tree = crate::tests::tempdir("derive-fold-orphans-hand-document");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);

    // A hand correction on the work about to be folded away — through
    // `librarydoc::apply` + `hand::save`, the real route, not a row inserted
    // by hand: `a_rekeyed_digest_takes_its_document_with_it`'s own choice,
    // for the same reason.
    let mut doc = crate::librarydoc::of(&conn, colour).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, colour, &doc).expect("applying");
    let saved = crate::hand::save(&conn, colour).expect("saving");
    assert!(
        saved.persisted,
        "the document has to actually be on disk for the fold to strand it: {saved:?}"
    );

    write_the_fold_rule(&conn);

    let second = derive::derive(&conn).expect("second derive");
    assert_eq!(
        second.emptied_works, 1,
        "the fixture only means something if the colour work is actually \
         folded away on this pass: {second:?}"
    );
    assert_eq!(
        second.orphan_documents.len(),
        1,
        "the folded work's document must be reported orphaned on the very \
         pass that folded it, not a later one: {second:?}"
    );
    assert_eq!(
        second.orphan_documents[0].title.as_deref(),
        Some("Hand Set"),
        "naming the folded-away work's own document"
    );

    // Exactly one work survives the fold — `colour` is gone, and it was the
    // only other one the fixture ever had — so this is the same target
    // `fold::absorb` folded into, read back the same way a person would:
    // by querying the row rather than trusting a value threaded through the
    // test's own setup.
    let (target_slug, target_title): (String, String) = conn
        .query_row("SELECT slug, title FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("exactly one work remains once the fold has run");
    let absorbed = second.orphan_documents[0].absorbed_into.as_ref().expect(
        "a document stranded by a fold must name the work that absorbed \
             it, not merely that it is now orphaned",
    );
    assert_eq!(
        (absorbed.slug.as_str(), absorbed.title.as_str()),
        (target_slug.as_str(), target_title.as_str()),
        "the orphan must point at the work the fold actually moved the \
         reader's material onto, not merely at *some* work"
    );
}

/// **`report.repaired_documents += found.repaired` accumulates once per root
/// inside the reconcile loop above — every test in this module that opens a
/// gap at all does it under a single root, so a mutation collapsing that
/// accumulator still passed the whole suite.** `-=` starting from `0` gives a
/// negative count no existing assertion reads, and `*=` starting from `0`
/// pins the field at `0` forever, since zero times anything is zero. This is
/// the one test with two roots each needing their own document repaired on
/// the same pass, which only a genuine `+=` gets right.
///
/// **The gap is opened by deleting the settled `.library.json` file
/// directly, not by rekeying.** `a_rekeyed_digest_takes_its_document_with_it`'s
/// own comment records why a rekey does not open a gap through a real
/// derive at all: `rename_for_rekey` carries the document to its new
/// address in the same pass, before `reconcile` ever sees anything missing.
/// Deleting the file after it has settled — the same move
/// `preview::tests::a_preview_leaves_the_library_tree_byte_identical` uses,
/// credited there to `hand/tests.rs`'s `a_gap_is_written_on_the_next_pass` —
/// is what actually reopens the gap `reconcile`'s repair exists to close.
#[test]
fn repaired_documents_accumulates_across_every_root_that_needed_one() {
    let dir = crate::tests::tempdir("derive-repaired-documents-two-roots");
    let root_a_dir = dir.path.join("root-a");
    write_archive(&root_a_dir.join("Alpha").join("Alpha - v01.cbz"), 3);
    let root_b_dir = dir.path.join("root-b");
    write_archive(&root_b_dir.join("Beta").join("Beta - v01.cbz"), 3);

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root_a = scan::add_root(&conn, &root_a_dir).expect("adding root a");
    scan::scan_root(&conn, &root_a).expect("scanning root a");
    let root_b = scan::add_root(&conn, &root_b_dir).expect("adding root b");
    scan::scan_root(&conn, &root_b).expect("scanning root b");
    conn.execute("UPDATE root SET keeps_documents = 1", [])
        .expect("opting both roots into hand documents");
    derive::derive(&conn).expect("the first derive");

    let work_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM work ORDER BY id")
            .expect("preparing the work id query");
        stmt.query_map([], |r| r.get(0))
            .expect("querying the work ids")
            .collect::<Result<_, _>>()
            .expect("reading the work ids")
    };
    assert_eq!(
        work_ids.len(),
        2,
        "one work per root — the fixture this test's own reasoning depends on"
    );

    let mut addresses = Vec::new();
    for &work_id in &work_ids {
        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
        doc.title = Some(format!("Hand Set {work_id}"));
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
        crate::hand::save(&conn, work_id).expect("saving");
        addresses.push(
            crate::hand::path_for(&conn, work_id)
                .expect("addressing")
                .expect("a rooted work has an address"),
        );
    }
    // Settles both documents and arms `migrated_document`'s marker for each
    // — `crate::tests::library_with_a_kept_document`'s own second-derive
    // step, over two roots instead of one.
    derive::derive(&conn).expect("settling derive");

    for address in &addresses {
        assert!(
            address.is_file(),
            "the document must be on disk before the gap is opened: {address:?}"
        );
        std::fs::remove_file(address).expect("reopening the gap on purpose");
    }

    let report = derive::derive(&conn).expect("the gap-repair derive");
    assert_eq!(
        report.repaired_documents, 2,
        "both roots' own gaps must be counted, not just the first root's, and not left at 0 or \
         negative: {report:?}"
    );
}

/// **Two orphans in one pass, only one of which came from a fold** — the gap
/// the test above cannot close on its own. With a single orphan and a single
/// `absorbed_into` entry, a correlation keyed on the orphan's own path
/// (`absorbed_into.get(&orphan.path)`) and one that just grabs *an* entry
/// (`absorbed_into.values().next().cloned()`) read identically: there is
/// only one path and only one value, so either implementation hands the one
/// orphan the one entry. This fixture forces them apart by adding a second
/// orphan that no fold ever touched — a document planted directly under the
/// root's `.taiyaku`, naming no work this library has ever held, never
/// reachable through `superseded` at all (a real rekey does not do it: see
/// `a_rekeyed_digest_takes_its_document_with_it`, where `rename_for_rekey`
/// carries the document to its new address before `reconcile` ever sees a
/// gap). Under the keyed lookup the planted orphan's path is not in the map,
/// so it reads `None`; under `.values().next()` it would be handed colour's
/// entry anyway, because a one-entry map hands out the same value regardless
/// of which orphan is asking.
///
/// Hand-applying `absorbed_into.values().next().cloned()` in place of the
/// `.get(&orphan.path)` call and rerunning turns the `is_none()` assertion
/// below red while leaving the rest of this module green — confirmed, then
/// reverted, per O88's own verification step.
#[test]
fn two_orphans_in_one_pass_only_one_from_a_fold() {
    let tree = crate::tests::tempdir("derive-two-orphans-one-fold");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);

    // colour's own hand document — the one the fold below is about to
    // strand.
    let mut doc = crate::librarydoc::of(&conn, colour).expect("projecting");
    doc.title = Some("Colour's Own Title".to_string());
    crate::librarydoc::apply(&conn, colour, &doc).expect("applying");
    let saved = crate::hand::save(&conn, colour).expect("saving");
    assert!(
        saved.persisted,
        "the document has to actually be on disk for the fold to strand it: {saved:?}"
    );

    // A second document, planted directly under the same root's `.taiyaku`,
    // addressing no work this library has ever derived — stranded from the
    // moment it is written, never through a fold or a rekey.
    let taiyaku_dir = tree.path.join(".taiyaku");
    std::fs::create_dir_all(&taiyaku_dir).expect("creating .taiyaku");
    std::fs::write(
        taiyaku_dir.join("never-addressed.library.json"),
        br#"{"taiyaku_library":1,"updated":"x","title":"Stranded From The Start"}"#,
    )
    .expect("planting an unaddressed document");

    write_the_fold_rule(&conn);

    let second = derive::derive(&conn).expect("second derive");
    assert_eq!(
        second.emptied_works, 1,
        "only colour folds away; the planted document names no work at all: {second:?}"
    );
    assert_eq!(
        second.orphan_documents.len(),
        2,
        "colour's document orphaned by the fold, and the planted one that \
         was never addressed to begin with: {second:?}"
    );

    let folded = second
        .orphan_documents
        .iter()
        .find(|o| o.title.as_deref() == Some("Colour's Own Title"))
        .expect("colour's own orphan");
    let planted = second
        .orphan_documents
        .iter()
        .find(|o| o.title.as_deref() == Some("Stranded From The Start"))
        .expect("the planted orphan");

    // Exactly one work survives the fold — `colour` is gone, and the fixture
    // never had a third work — so this is the same target `fold::absorb`
    // folded into, read back rather than trusted through the test's own
    // setup.
    let (target_slug, target_title): (String, String) = conn
        .query_row("SELECT slug, title FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("exactly one work remains once the fold has run");
    let absorbed = folded
        .absorbed_into
        .as_ref()
        .expect("a document stranded by a fold must name the work that absorbed it");
    assert_eq!(
        (absorbed.slug.as_str(), absorbed.title.as_str()),
        (target_slug.as_str(), target_title.as_str()),
        "the orphan must point at the actual fold target, not merely *some* work"
    );

    assert!(
        planted.absorbed_into.is_none(),
        "nothing ever folded the planted document's non-existent work into \
         anything — this must stay `None` rather than borrowing colour's \
         own entry out of `absorbed_into`: {planted:?}"
    );
}

/// **Store spec §4.4 stated as a test:** *losing the database costs a rescan,
/// not the organisational work.* Measured failing on 2026-08-31 — three of
/// five works lost or reverted a hand-set title — and this is the assertion
/// that keeps it fixed.
///
/// A second, empty database over the **same root** is what a lost database
/// actually means. Nothing here shares a connection with the first.
#[test]
fn a_correction_survives_losing_the_database() {
    let dir = archived_volumes();
    correct_the_first_work(&dir);

    // The database is gone. Only the root remains — which is what losing it
    // actually means.
    assert_eq!(
        title_after_rebuilding(&dir),
        "Hand Set",
        "one rescan restores the correction — the whole point of writing the \
         document into the library rather than only into the database"
    );
}

/// **O81, prevented.** Repacking an archive moves the work's `content_key`;
/// the document must follow, because after this pass commits the old key is
/// gone from the database and the file is unattributable except by a person
/// opening it. Measured on 2026-08-31: without this, the correction is lost on
/// the next database rebuild.
#[test]
fn a_repacked_volume_takes_its_document_with_it() {
    let dir = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    // Migration 012 defaults `keeps_documents` off; this test is about the
    // document following a repack, so it opts in.
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("the fixture keeps documents");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("deriving");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("a work was derived");
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    // **One page added — one of the two events the measurement used, and it
    // needs no new helper.** `write_archive` is deterministic and each page's
    // bytes name the archive holding them, so rewriting v01 with four pages
    // instead of three changes the member digest, and with it the work's
    // `content_key`. v02 already holds four pages and does not collide,
    // because the label differs — `write_archive`'s own comment says why it
    // is built that way.
    write_archive(
        &dir.path.join("Series").join("v01").join("Series v01.cbz"),
        4,
    );

    scan::scan_root(&conn, &root).expect("rescanning");
    let report = derive::derive(&conn).expect("rederiving");

    assert_eq!(report.rekeyed, 1, "the work followed its files: {report:?}");
    assert_eq!(
        report.orphan_documents.len(),
        0,
        "and its document followed the work: {report:?}"
    );

    let path = crate::hand::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    assert!(
        path.is_file(),
        "the document is at the new address: {}",
        path.display()
    );
}

/// **The measured failure, as an assertion.** Task 5's baseline never moves
/// the `content_key`, so it passed before any of this existed. Here the volume
/// gains a page **while a database is watching**, which is what took two of
/// five works' corrections on 2026-08-31, and it is the transition
/// `rename_for_rekey` exists to carry the document across.
///
/// **The `orphan_documents` assertion inside the scope block is the one that
/// actually tests `rename_for_rekey`; the title assertion at the end does
/// not.** Delete the rename and this test would still pass at the end:
/// `hand::reconcile`'s gap repair re-projects the surviving `work` row and
/// writes a fresh document at the new address on the very next derive, so
/// the title comes back regardless. The final assertion is kept anyway
/// because it documents the end-to-end promise — store spec §4.4 — that
/// this whole feature exists to keep; `orphan_documents == 0` is what
/// actually goes red if the rename is removed.
#[test]
fn a_correction_survives_a_page_being_added_and_then_losing_the_database() {
    let dir = archived_volumes();
    let v01 = dir.path.join("Series").join("v01").join("Series v01.cbz");

    {
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let root = scan::add_root(&conn, &dir.path).expect("add");
        // Migration 012 defaults `keeps_documents` off; this test is about
        // the document surviving a rekey and then losing the database, so it
        // opts in.
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the fixture keeps documents");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("deriving");

        let work_id: i64 = conn
            .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| {
                row.get(0)
            })
            .expect("a work was derived");
        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
        doc.title = Some("Hand Set".to_string());
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
        crate::hand::save(&conn, work_id).expect("saving");

        // **One page added, and a scan that sees it.** The document moves
        // between the old and new addresses on the `derive` call below —
        // from `derive::write_work`, after `write_roots`, per that call
        // site's own comment — or it never moves at all.
        write_archive(&v01, 4);
        scan::scan_root(&conn, &root).expect("rescanning");
        let report = derive::derive(&conn).expect("rederiving");
        assert_eq!(report.rekeyed, 1, "the work followed its files: {report:?}");
        assert_eq!(
            report.orphan_documents.len(),
            0,
            "and its document followed the work: {report:?}"
        );
    }

    // Only now is the database gone, and only the root remains.
    assert_eq!(
        title_after_rebuilding(&dir),
        "Hand Set",
        "the document was carried to the new address while a scan could still \
         see both, so one rescan restores the correction — store spec §4.4"
    );
}

/// Scan `dir` into a fresh database, hand-set the first work's title to
/// `Hand Set`, save the document beside the archives, and drop the database.
/// What is left on disk is a library and its documents, and nothing else.
fn correct_the_first_work(dir: &crate::tests::TempDir) {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    // Migration 012 defaults `keeps_documents` off; this fixture's whole
    // point is a document that survives losing the database, so it opts in.
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("the fixture keeps documents");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("deriving");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("a work was derived");
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    let saved = crate::hand::save(&conn, work_id).expect("saving");
    assert!(saved.persisted, "the document reached the disk: {saved:?}");
}

/// Build a second, empty database over the same root and answer what the
/// first work's title is once the scan and derive have run.
fn title_after_rebuilding(dir: &crate::tests::TempDir) -> String {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    scan::scan_root(&conn, &root).expect("rescanning");
    derive::derive(&conn).expect("rederiving");
    conn.query_row("SELECT title FROM work ORDER BY id LIMIT 1", [], |row| {
        row.get(0)
    })
    .expect("the work came back")
}

/// A preview derivation must not rename, write or delete anything under
/// `<root>/.taiyaku/`. `hand::rename_for_rekey` is gated on nothing — a rekey
/// stranding a document is the defect it exists to prevent — so without a
/// documents policy a rule preview would rename files in someone's library and
/// the database rollback would not undo it.
#[test]
fn skipping_documents_leaves_the_library_tree_byte_identical() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();

    // Move the work's content_key: repack the archive so a rekey is what the
    // next derivation sees. Taken after the repack and rescan, not before:
    // `before` pins what `derive_with` itself must not disturb, and the
    // repack — a setup step standing in for a scan that already ran — has
    // already changed the archive's bytes by this point on purpose.
    crate::tests::add_a_page(&fixture, "Series - v01.cbz");
    crate::scan::scan_root(&conn, &crate::scan::roots(&conn).unwrap()[0]).unwrap();
    let before = crate::tests::tree_snapshot(fixture.root_path());

    crate::derive::derive_with(&conn, crate::derive::Documents::Skip).unwrap();

    assert_eq!(
        crate::tests::tree_snapshot(fixture.root_path()),
        before,
        "a Skip derivation wrote, renamed or removed a file under the root"
    );
}

/// The default policy is unchanged behaviour: `derive` still reconciles.
///
/// **Location on disk, not the report's own counters, is what has to pin the
/// delegation.** `report.rekeyed` increments before either policy's guard
/// runs, so it is identical whichever policy `derive` actually calls, and
/// `report.orphan_documents` is populated only inside the reconcile loop —
/// it reads empty both when `rename_for_rekey` genuinely carried the
/// document across *and* when a `Skip`-delegating `derive` never populated
/// it at all, because nothing else in this module writes to it. Neither
/// counter tells the two apart. The filesystem does: under a correct
/// `derive` the document ends up at the new `content_key` address and the
/// old one is gone; under a `Skip`-delegating `derive` neither the rename
/// nor the reconcile loop's gap repair runs, so the old-key file is still
/// standing and the new-key file was never written. Confirmed with a direct
/// mutation — rewiring `derive` to call `derive_with(conn, Documents::Skip)`
/// left every assertion this test had before this one green.
#[test]
fn reconciling_documents_is_what_plain_derive_does() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    crate::tests::add_a_page(&fixture, "Series - v01.cbz");
    crate::scan::scan_root(&conn, &crate::scan::roots(&conn).unwrap()[0]).unwrap();

    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the one work the fixture derived");
    let old_path = crate::hand::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");

    let report = crate::derive::derive(&conn).unwrap();

    assert_eq!(
        report.rekeyed, 1,
        "the repack should have moved content_key"
    );
    assert!(
        report.orphan_documents.is_empty(),
        "rename_for_rekey should have carried the document across: {:?}",
        report.orphan_documents
    );

    let new_path = crate::hand::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    assert_ne!(
        old_path, new_path,
        "the rekey must actually move the address, or the two checks below \
         prove nothing"
    );
    assert!(
        !old_path.is_file(),
        "the old-key document must be gone once a real derive has reconciled \
         it: {}",
        old_path.display()
    );
    assert!(
        new_path.is_file(),
        "the document must stand at the new address: {}",
        new_path.display()
    );
}

/// **Whole-branch review, Important 1.** `write_roots` writes `member.root_id`
/// from a [`super::Group`] built earlier in the same derive pass — and
/// `derive_with` holds no transaction of its own, so under WAL a concurrent
/// [`crate::scan::remove_root`] can commit a root's deletion in the window
/// between that group being built and this write running. The `UPDATE`
/// then names a `root_id` the `root` table no longer holds, and the column's
/// own foreign key used to refuse it outright — failing the *whole* derive
/// over a race nobody watching the walk's own `report.error` could see
/// coming, while `root.last_scan_status` still read `"ok"` (the walk itself
/// had succeeded).
///
/// **Reproduced deterministically here, not through real threads.** A
/// [`super::Group`] naming a `root_id` the `root` table has *already* lost by
/// the time `write_roots` runs is exactly what that race leaves behind at
/// the one call this finding names — constructing it by hand after deleting
/// the root pins the fix without a timing-dependent test that could pass or
/// fail depending on how two threads happen to interleave.
#[test]
fn write_roots_survives_a_root_deleted_after_its_own_group_was_built() {
    let dir = crate::tests::tempdir("derive-write-roots-vanished-root");
    write_pages(&dir.path.join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("adding the root");
    scan::scan_root(&conn, &root).expect("scanning the root");
    derive::derive(&conn).expect("the first derive, which assigns member.root_id for real");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the fixture's one work");
    let edition_key: String = conn
        .query_row(
            "SELECT key FROM edition WHERE work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("the fixture's one edition");
    let member_root_id: i64 = conn
        .query_row(
            "SELECT m.root_id FROM member m
               JOIN edition e ON e.id = m.edition_id
              WHERE e.work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("the member the first derive assigned a real root to");
    assert_eq!(
        member_root_id, root.id,
        "sanity: the member must carry the root this test is about to delete"
    );

    // The race: `root` is gone — as a concurrent `remove_root` would leave
    // it — while `group`, below, still names its id, exactly as a `Group`
    // built moments before that commit would.
    conn.execute("DELETE FROM root WHERE id = ?1", [root.id])
        .expect("deleting the root out from under the in-flight group");
    let stranded: Option<i64> = conn
        .query_row(
            "SELECT root_id FROM member m JOIN edition e ON e.id = m.edition_id \
             WHERE e.work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("reading the member back");
    assert_eq!(
        stranded, None,
        "sanity: schema/006_member_root.sql's own ON DELETE SET NULL must already have cleared \
         this — the point of this test is what write_roots then does when it tries to write \
         the stale id straight back"
    );

    let mut group = super::Group {
        series: "Series".to_string(),
        volume: Some(1.0),
        editions: std::collections::BTreeMap::new(),
    };
    group.editions.insert(
        edition_key,
        vec![super::Candidate {
            path: dir
                .path
                .join("Series")
                .join("Series - v01")
                .join("000.png")
                .to_string_lossy()
                .into_owned(),
            rel_path: "Series/Series - v01/000.png".to_string(),
            root_id: root.id,
            sha256: "0".repeat(64),
            pages: 1,
            chapter: None,
            title: None,
            kind: None,
            language: None,
        }],
    );
    let mut report = derive::DeriveReport::default();

    super::write_roots(&conn, work_id, &group, &mut report)
        .expect("a vanished root must not fail write_roots outright");

    assert_eq!(
        report.root_removed_mid_derive, 1,
        "the race must be counted, not merely swallowed"
    );
    let after: Option<i64> = conn
        .query_row(
            "SELECT root_id FROM member m JOIN edition e ON e.id = m.edition_id \
             WHERE e.work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("reading the member back");
    assert_eq!(
        after, None,
        "the member must read no root at all — the same state an ordinary ON DELETE SET NULL \
         already gives every other member of a removed root, not stranded on an id nothing \
         holds"
    );
}

/// **`is_foreign_key_violation`'s narrowness is what makes the recovery
/// above safe, and nothing above tests the narrowness itself.** Widen the
/// guard to match any error at all — `replace match guard
/// is_foreign_key_violation(&e) with true`, or make the function itself
/// always answer `true` — and the test above still passes, because a
/// deleted root really does raise a foreign-key violation and either
/// mutation happens to agree with the real guard on that one case.
///
/// Reached with a `BEFORE UPDATE` trigger rather than another deleted root,
/// so the error `write_roots`'s first `UPDATE` receives is unambiguously
/// *not* a foreign-key violation: the trigger only fires while the root row
/// it names is still live, still referenced, still exactly what
/// `member.root_id`'s own foreign key is satisfied by. A locked database or
/// a malformed statement — the two examples `write_roots`'s own doc comment
/// gives for "still propagates unchanged" — could not be staged
/// deterministically, so this stands in for them: some other, non-FK error
/// on that exact statement.
#[test]
fn write_roots_propagates_a_non_foreign_key_error_rather_than_treating_it_as_a_vanished_root() {
    let dir = crate::tests::tempdir("derive-write-roots-non-fk-error");
    write_pages(&dir.path.join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("adding the root");
    scan::scan_root(&conn, &root).expect("scanning the root");
    derive::derive(&conn).expect("the first derive, which assigns member.root_id for real");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the fixture's one work");
    let edition_key: String = conn
        .query_row(
            "SELECT key FROM edition WHERE work_id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("the fixture's one edition");

    // The stand-in store fault: firing only on the exact write write_roots
    // is about to make (root_id set back to the still-live root this member
    // already carries), so there is no ambiguity about why it failed — the
    // root row it names is right there, unremoved.
    conn.execute_batch(&format!(
        "CREATE TRIGGER simulate_store_fault BEFORE UPDATE OF root_id ON member \
         WHEN NEW.root_id = {} \
         BEGIN SELECT RAISE(ABORT, 'not a foreign key violation'); END;",
        root.id
    ))
    .expect("installing the trigger");

    let mut group = super::Group {
        series: "Series".to_string(),
        volume: Some(1.0),
        editions: std::collections::BTreeMap::new(),
    };
    group.editions.insert(
        edition_key,
        vec![super::Candidate {
            path: dir
                .path
                .join("Series")
                .join("Series - v01")
                .join("000.png")
                .to_string_lossy()
                .into_owned(),
            rel_path: "Series/Series - v01/000.png".to_string(),
            root_id: root.id,
            sha256: "0".repeat(64),
            pages: 1,
            chapter: None,
            title: None,
            kind: None,
            language: None,
        }],
    );
    let mut report = derive::DeriveReport::default();

    let err = super::write_roots(&conn, work_id, &group, &mut report).expect_err(
        "a non-foreign-key error on the same statement must fail the derive, not be \
         swallowed as a vanished root",
    );
    assert!(
        err.contains("not a foreign key violation"),
        "the propagated error must be the store fault itself, not a generic message: {err}"
    );
    assert_eq!(
        report.root_removed_mid_derive, 0,
        "a store fault is not a vanished root and must not be counted as one"
    );
}

/// One `series_title` entry, as `PATCH /api/library/series/{id}` sends it.
fn corrected_title(
    title: &str,
    kind: crate::librarydoc::TitleKind,
    primary: bool,
) -> crate::librarydoc::SeriesTitle {
    crate::librarydoc::SeriesTitle {
        title: title.to_string(),
        kind,
        primary,
    }
}

/// `scan::tests::fixture()` scanned and derived, Go-Toubun's titles replaced
/// with `corrected` through `librarydoc::set_series_titles` — what
/// `PATCH /api/library/series/{id}` calls — and the root scanned and derived
/// again. Answers the library and Go-Toubun's series id from before the
/// correction.
fn rescanned_after_correcting_gotoubun(
    corrected: &[crate::librarydoc::SeriesTitle],
) -> (Library, i64) {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let series_id = {
        let conn = library.conn();
        let root = scan::add_root(&conn, &tree.path).expect("add");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("first derive");
        let series_id: i64 = conn
            .query_row(
                "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
                [],
                |r| r.get(0),
            )
            .expect("the scanned series");
        crate::librarydoc::set_series_titles(&conn, series_id, corrected)
            .expect("corrects the titles");
        scan::scan_root(&conn, &root).expect("rescan");
        derive::derive(&conn).expect("second derive");
        series_id
    };
    (library, series_id)
}

/// What a rescan after a title correction must leave: the fixture's two
/// series and no third, both Go-Toubun volumes still on `series_id`, and
/// exactly `corrected` as its titles, in the order they were written.
fn assert_the_correction_survived(
    conn: &Connection,
    series_id: i64,
    corrected: &[crate::librarydoc::SeriesTitle],
) {
    let series: Vec<(i64, String)> = conn
        .prepare("SELECT id, sort_title FROM series ORDER BY id")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(
        series.len(),
        2,
        "Go-Toubun and the chapters directory, and no second Go-Toubun minted \
         under the scanned name: {series:?}"
    );
    let shelved: Vec<Option<i64>> = conn
        .prepare("SELECT series_id FROM work ORDER BY id")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(
        shelved,
        vec![Some(series_id); 2],
        "both Go-Toubun volumes stay on series {series_id}: {series:?}"
    );
    assert_eq!(
        crate::librarydoc::series_titles(conn, series_id).expect("reads the titles"),
        corrected,
        "series {series_id} holds exactly the corrected titles — none dropped, \
         and the scanned text not written back beside them"
    );
}

/// **O114, the measured case.** A correction replacing the text the files gave
/// a series — O83's drive corrected Kaguya's romaji spelling — moved
/// `series.sort_title` with it, and `reconcile_series` finds a series by
/// `sort_title`, so the next scan found nothing under the scanned name, minted
/// a second series and moved every volume onto it.
#[test]
fn a_rescan_after_a_correction_replacing_the_scanned_title_keeps_the_series() {
    use crate::librarydoc::TitleKind;
    let corrected = [
        corrected_title("Go-Tōbun no Hanayome", TitleKind::Romaji, true),
        corrected_title("五等分の花嫁", TitleKind::Native, false),
    ];
    let (library, series_id) = rescanned_after_correcting_gotoubun(&corrected);
    assert_the_correction_survived(&library.conn(), series_id, &corrected);
}

/// **O114, the scanned text kept and no longer primary.** Adding a native
/// title and making it the primary one moved `sort_title` to it just the same.
#[test]
fn a_rescan_after_a_correction_making_another_title_primary_keeps_the_series() {
    use crate::librarydoc::TitleKind;
    let corrected = [
        corrected_title("Go-Toubun no Hanayome", TitleKind::Romaji, false),
        corrected_title("五等分の花嫁", TitleKind::Native, true),
    ];
    let (library, series_id) = rescanned_after_correcting_gotoubun(&corrected);
    assert_the_correction_survived(&library.conn(), series_id, &corrected);
}

/// **O114, the case its row left unmeasured.** A correction keeping the
/// scanned text as the primary title never moved `sort_title`, so it survived
/// a rescan before the fix too. Pinned beside the other two because the title
/// derivation writes on every pass must land on the row already holding that
/// text rather than beside it.
#[test]
fn a_rescan_after_a_correction_keeping_the_scanned_primary_keeps_the_series() {
    use crate::librarydoc::TitleKind;
    let corrected = [
        corrected_title("Go-Toubun no Hanayome", TitleKind::Romaji, true),
        corrected_title("The Quintessential Quintuplets", TitleKind::English, false),
    ];
    let (library, series_id) = rescanned_after_correcting_gotoubun(&corrected);
    assert_the_correction_survived(&library.conn(), series_id, &corrected);
}

/// **O114, the second writer.** `librarydoc::apply` resolves a document's
/// series by any title text the two share, and its matched branch wrote the
/// document's primary title into `sort_title` — reached by
/// `PATCH /api/library/works/{slug}` and by `derive` reading a hand document
/// back. A document naming the scanned text beside a native primary moved the
/// key exactly as `set_series_titles` did.
#[test]
fn a_rescan_after_a_document_names_another_primary_title_keeps_the_series() {
    use crate::librarydoc::TitleKind;
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");
    let (series_id, work_id): (i64, i64) = conn
        .query_row(
            "SELECT s.id, MIN(w.id) FROM series s JOIN work w ON w.series_id = s.id
              WHERE s.sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the scanned series and its first work");
    let doc = crate::librarydoc::LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-09-15T00:00:00Z",
            "series":{"titles":[
              {"title":"Go-Toubun no Hanayome","kind":"romaji"},
              {"title":"五等分の花嫁","kind":"native","primary":true}]}}"#,
    )
    .expect("parses");
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applies the document");

    scan::scan_root(&conn, &root).expect("rescan");
    derive::derive(&conn).expect("second derive");

    assert_the_correction_survived(
        &conn,
        series_id,
        &[
            corrected_title("Go-Toubun no Hanayome", TitleKind::Romaji, false),
            corrected_title("五等分の花嫁", TitleKind::Native, true),
        ],
    );
}
