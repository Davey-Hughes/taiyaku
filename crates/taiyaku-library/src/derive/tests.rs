use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::progress::{Outcome, Step};
use crate::scan::tests::{
    fixture, write_archive, write_archive_labelled, write_archive_of, write_pages,
};
use crate::test_support::{Recording, Report};
use crate::{Library, derive, scan};

/// A root whose volumes are **archives inside volume directories** —
/// `Series/v01/Series v01.cbz` — which is the layout this project's own
/// corpus uses.
///
/// Every other fixture here builds members out of directories of page images.
/// A volume directory holding archives rather than loose images is counted by
/// the walk as holding no pages of its own.
fn archived_volumes() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("derive-archived");
    let series = dir.path().join("Series");
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
    let series = dir.path().join("Chaptered");
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
    write_pages(&dir.path().join("Alone - v01"), 2);
    dir
}

/// One volume-less work — a one-shot — in each of two roots, the second under
/// a rule naming an edition key that sorts before the first's, and holding a
/// different number of pages so the two editions' digests differ.
fn oneshot(label: &str, pages: usize) -> crate::tests::TempDir {
    let dir = crate::tests::tempdir(label);
    write_pages(&dir.path().join("Oneshot").join("Some Oneshot"), pages);
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
    let series = dir.path().join("ONE PIECE");
    for (name, pages) in [("001", 3), ("002", 4), ("003", 5), ("004", 6)] {
        write_pages(&series.join(name), pages);
    }
    dir
}

/// A root whose one series arrives as several chapter directories under one
/// parent — §7's shape, at the scale that makes the count mean something: the
/// directory is what stays unattached, not each of the files in it.
fn chapters() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("derive-chapters");
    let series = dir.path().join("Kaguya-sama - Fan Translated");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    assert_eq!(edition_keys(&conn), vec!["raw", "raw"]);

    std::fs::remove_dir_all(tree.path()).expect("remove the tree entirely");

    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key)
         VALUES (99, 'prefix', '', 'path', 'colour')",
        [],
    )
    .expect("new rule");
    let report = derive::derive(&conn).expect("re-derive with no filesystem at all");

    assert_eq!(report.works, 2);
    // **Every key, sorted, rather than `SELECT key FROM edition LIMIT 1`.**
    // Both keys are present, and that query would answer `colour` from the
    // `(work_id, key)` index — where `colour` sorts before `raw` — whether or
    // not the rename had replaced anything.
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
    // The stated cost of `import::scanned` not deleting an edition the scan
    // did not name, and the reason `kept_editions` exists: the orphan is
    // reported rather than discovered. Without the counter, "the rule edit
    // renamed the edition" and "the rule edit added a second one beside the
    // first" read identically from the summary — one of them is a library
    // that needs fixing and the other is not.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // §7. Which chapter files constitute volume 3 is a question only the
    // alignment answers, so the members are recorded with `chapter_declared`
    // and the directory stays visibly unattached.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

    // **The identity assertion above cannot fail on its own**:
    // `series.sort_title` carries no unique constraint, so a derive that
    // inserted unconditionally would leave two rows for one series and
    // `SELECT id FROM series WHERE sort_title = …` would still answer with the
    // first — the same id, both times, forever. The count is what actually
    // sees it.
    //
    // `derive_with`'s own doc claims a derive is "idempotent by construction".
    // Every table derivation writes is counted here, because each is reconciled
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
    //
    // Both roots are directories of one library: an edition arriving in another
    // library's directory is another work, and no hazard to this one.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root_in(&conn, tree.path(), scan::InLibrary::Named("Manga")).expect("add");
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
    let series = second.path().join("Go-Toubun no Hanayome");
    write_pages(&series.join("Go-Toubun no Hanayome - v01 (2019)"), 3);
    write_pages(&series.join("Go-Toubun no Hanayome - v02 (2019)"), 4);
    let other =
        scan::add_root_in(&conn, second.path(), scan::InLibrary::Named("Manga")).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.unreadable,
        derive::Unreadable {
            by_format: formats(&[("pdf", 1)]),
            ..derive::Unreadable::default()
        },
        "notes.pdf, and nothing else"
    );
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
    // became no member is not hashed at all, since hashing a series to decide
    // it is not a member is the cost the fact layer exists to avoid.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    std::fs::remove_dir_all(tree.path()).expect("remove the tree before anything is hashed");

    let report = derive::derive(&conn).expect("a missing file is not a failed derive");

    assert_eq!(
        report.works, 0,
        "nothing could be hashed, so nothing became a work"
    );
    assert_eq!(
        report.unreadable,
        derive::Unreadable {
            by_format: formats(&[("pdf", 1)]),
            vanished: vec![
                under(
                    &root,
                    "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2019)"
                ),
                under(
                    &root,
                    "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v02 (2019)"
                ),
            ],
            ..derive::Unreadable::default()
        },
        "the PDF, and the two members whose bytes are no longer there to hash, \
         each named by its path so a person can go and look"
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // value survives it, and so must the write beneath it: `builtin-chapter`
    // is a seeded rule, so the very re-derive a person runs right after
    // correcting a chapter is the one that would revert it, silently, whenever
    // the filename still parses.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // `comicinfo` survives the clear, which is scoped by `chapter_source`
    // exactly as `hand` is, and so must the write beneath it: unscoped, it
    // would overwrite a `comicinfo` number on the next scan of a member whose
    // filename happens to parse. The same hazard as the hand case.
    let tree = chaptered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // The `attach-chapters` proposal keys `subject_id` on a `series` row. The
    // chapter-organised directory produces no work, so if the series were
    // minted only alongside a work there would be nothing for that proposal to
    // reference and §6's inbox could never name it.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // `librarydoc::apply` — reached through `migrate::work`, §4.4 — is the
    // documented mechanism for hand-setting `work.volume`/`work.series_id`, and
    // `<stem>.library.json` exists precisely so that survives a re-derivation.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

    // **`volume_hand`, not `volume` — O80, migration 011.** A hand-set volume
    // lives in its own provenance column: `derive` writes
    // `COALESCE(volume_hand, <derived>)`, so a value parked in `volume` alone
    // is exactly what a re-derive is supposed to replace.
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

/// O53's arm. **Through the real `crate::hand::save`**, unlike the test above,
/// which sets `volume_hand` directly: `save` is what a real hand edit calls,
/// and it both writes `<content_key>.library.json` beside the root *and*
/// records the marker in the same operation. So by the time the second `derive`
/// below runs, the marker this test's own connection already holds is exactly
/// what a real edit would have left. This does not exercise `hand::load` being
/// consulted by a *fresh* database with no marker of its own (that is
/// `hand::tests::a_rescan_with_no_database_recovers_the_hand_set_title`, across
/// two separate connections); it pins the guard's already-armed case.
#[test]
fn a_hand_document_beside_the_library_survives_the_next_derive() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
/// `write_work` leaves that test green; it pins the guard, not the read-back.
///
/// What this test pins is a marker cleared on a work the database already has,
/// which is what `migrate::work`'s own migration path can produce: the row,
/// `root_id` included, exists by the time `DELETE FROM migrated_document`
/// runs. The case of a work minted for the first time, in a database that has
/// never seen this root before, is
/// [`a_fresh_database_recovers_a_hand_set_title_on_its_first_derive`], across
/// two separate connections rather than one connection with a table cleared.
#[test]
fn a_derive_applies_a_hand_document_whose_marker_was_cleared_on_an_existing_work() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

/// **A work scanned for the first time by a brand-new database.** Losing the
/// database costs one rescan (store spec §4.4), not two. Such a work's
/// `member.root_id` is NULL until a root is written onto its members, and a
/// read-back that consulted `hand::load` before then would leave
/// `hand::path_for`'s `JOIN root ON root.id = member.root_id` matching
/// nothing, `path_for` answering `None`, `hand::load` answering `Ok(None)`,
/// and the correction sitting right beside the archives silently skipped. A
/// *second* derive against the same, now-populated database would recover
/// it, which is exactly the extra rescan this test exists to refuse.
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
        let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

/// The read-back's own inline comment states the rule — `!empty`, matching
/// `hand::save`'s identical rule, since a hand-emptied document has nothing
/// left to protect and arming the marker for it would freeze the cleared
/// state in permanently. This test pins it by handing the read-back a
/// document with nothing in it.
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

/// A user's file cannot abort a derive. This test and the next one reach
/// `write_work`'s two `report.unreadable_hand.push(why)` arms.
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

/// A hand document from a newer taiyaku is reported as unreadable and not
/// absorbed: absorbing it would let this build's `hand::save` later rewrite
/// it from rows without the fields it has no column for (O264).
#[test]
fn a_hand_document_from_a_newer_taiyaku_is_reported_and_its_title_not_absorbed() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let title = |conn: &Connection| -> String {
        conn.query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading the title")
    };
    let derived = title(&conn);
    assert_ne!(
        derived, "Hand title",
        "the document's title must differ from the derived one, or absorbing it would not show"
    );

    let newer = crate::librarydoc::VERSION + 1;
    let path = crate::hand::path_for(&conn, work_id)
        .expect("resolving")
        .expect("a scanned work has an address");
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
    std::fs::write(
        &path,
        format!(
            r#"{{"taiyaku_library": {newer}, "updated": "2026-11-10T09:00:00Z", "title": "Hand title"}}"#
        ),
    )
    .expect("writing the newer document");

    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");

    let report = derive::derive(&conn).expect("a newer document must not fail the derive");

    assert_eq!(
        report.unreadable_hand.len(),
        1,
        "{:?}",
        report.unreadable_hand
    );
    assert!(
        report.unreadable_hand[0].contains(&path.display().to_string()),
        "the message must name the file: {:?}",
        report.unreadable_hand
    );
    assert!(
        report.unreadable_hand[0]
            .contains(&format!("declares taiyaku_library format version {newer}")),
        "the message must say the document is newer: {:?}",
        report.unreadable_hand
    );
    assert_eq!(
        title(&conn),
        derived,
        "the newer document's title must not have been absorbed"
    );
}

/// A hand edit over a newer document leaves the marker as the read-back left
/// it, so the next derive still reports the document (O264). The edit gives
/// the rows a hand title, so `save` projects content and reaches its guard:
/// an empty projection would record an unset marker anyway.
#[test]
fn a_hand_edit_over_a_newer_document_leaves_it_reported_at_the_next_derive() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    // `save` returns early, recording the marker, for a root that keeps no
    // documents; the guard is on the other path.
    conn.execute("UPDATE root SET keeps_documents = 1", [])
        .expect("the root keeps documents");
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
    let newer = crate::librarydoc::VERSION + 1;
    let path = crate::hand::path_for(&conn, work_id)
        .expect("resolving")
        .expect("a scanned work has an address");
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
    std::fs::write(
        &path,
        format!(r#"{{"taiyaku_library": {newer}, "updated": "2026-11-10T09:00:00Z"}}"#),
    )
    .expect("writing the newer document");
    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");
    let refused = derive::derive(&conn).expect("a newer document must not fail the derive");
    assert_eq!(
        refused.unreadable_hand.len(),
        1,
        "the read-back must refuse the document first: {:?}",
        refused.unreadable_hand
    );

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand title".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying the hand title");
    let saved = crate::hand::save(&conn, work_id).expect("saving");
    assert!(!saved.persisted, "the guard must have refused the write");

    let report = derive::derive(&conn).expect("the next derive");

    assert_eq!(
        report.unreadable_hand.len(),
        1,
        "the document is still reported: {:?}",
        report.unreadable_hand
    );
    assert!(
        report.unreadable_hand[0].contains(&path.display().to_string()),
        "{:?}",
        report.unreadable_hand
    );
    let title: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading the title");
    assert_eq!(
        title, "Hand title",
        "the edit stands in the rows although its document was left alone"
    );
}

/// The apply-failure arm. `librarydoc::apply`'s own errors name a `work_id`
/// and a `series_id`, never a file — unlike the parse failure above, this is
/// the case whose message needed the path prepended by the read-back rather
/// than inherited from the error, since there is nothing to inherit.
#[test]
fn a_hand_document_naming_an_edition_the_work_does_not_have_is_reported_and_the_scan_goes_on() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
/// `volume`/`series_id` every time.
#[test]
fn a_re_derive_still_writes_the_volume_for_a_work_no_hand_document_has_touched() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    // Simulate a stray value in `volume` with no `volume_hand` behind it, so
    // this test would fail loudly if the write read anything weaker than
    // `volume_hand` (a non-NULL `volume`, say).
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

thread_local! {
    static WAL_COMMITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The commits `conn` writes to its WAL while `pass` runs — the ones that wait
/// for the disk. SQLite's commit hook would also count a write statement that
/// changed nothing, which writes no frame and costs no sync; the WAL hook
/// fires only for a commit that wrote frames. It takes a plain `fn`, hence
/// the thread-local: SQLite calls it on the thread running the statement.
///
/// Setting a WAL hook replaces the automatic checkpoint, which is SQLite's own
/// WAL hook, so this sets the default back afterwards.
fn wal_commits_during<T>(
    conn: &mut rusqlite::Connection,
    pass: impl FnOnce(&rusqlite::Connection) -> T,
) -> (usize, T) {
    fn count(_: &mut rusqlite::hooks::Wal, _: std::os::raw::c_int) -> rusqlite::Result<()> {
        WAL_COMMITS.with(|n| n.set(n.get() + 1));
        Ok(())
    }
    WAL_COMMITS.with(|n| n.set(0));
    conn.wal_hook(Some(count)).expect("hooks");
    let out = pass(conn);
    conn.wal_hook(
        None::<fn(&mut rusqlite::hooks::Wal, std::os::raw::c_int) -> rusqlite::Result<()>>,
    )
    .expect("unhooks");
    conn.pragma_update(None, "wal_autocheckpoint", 1000)
        .expect("restoring the automatic checkpoint");
    (WAL_COMMITS.with(std::cell::Cell::get), out)
}

/// The write transactions `conn` commits while `pass` runs, counted by the
/// commit hook, so one whose statements changed nothing counts too: it still
/// held the write lock a save waits for (O272).
pub(super) fn commits_during<T>(
    conn: &mut Connection,
    pass: impl FnOnce(&Connection) -> T,
) -> (usize, T) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let commits = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&commits);
    conn.commit_hook(Some(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        false
    }))
    .expect("hooks");
    let out = pass(conn);
    conn.commit_hook(None::<fn() -> bool>).expect("unhooks");
    (commits.load(Ordering::SeqCst), out)
}

/// **O210: a derive commits each work it writes once.** Every commit waits
/// for the disk.
///
/// Counted on a second derive that has something to write on every work and
/// no rekey: each edition's language is changed behind its back. On disk,
/// because an in-memory store has no WAL.
#[test]
fn a_derive_commits_each_work_it_writes_once() {
    let tree = fixture();
    let store = crate::tests::tempdir("derive-commits");
    let library = Library::open(&store.path().join("library.db")).expect("open");
    let mut conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");
    conn.execute("UPDATE edition SET language = 'stale'", [])
        .expect("a value the next derive writes back");

    let (commits, report) = wal_commits_during(&mut conn, derive::derive);
    let report = report.expect("second derive");

    assert_eq!(report.works, 2, "the fixture's two volumes: {report:?}");
    assert_eq!(commits, 2, "one commit per work: {report:?}");
    assert!(
        edition_metadata(&conn)
            .iter()
            .all(|(_, _, language)| language.is_none()),
        "the fixture: the derive wrote each work back: {:?}",
        edition_metadata(&conn)
    );
}

/// O272: a derive over a store it already derived, with nothing changed,
/// takes no write transaction and changes no row, so a quiet scan's derive
/// never holds the write lock a page turn's save waits for.
///
/// The fixture reaches every write a derive makes on a held work: chapters
/// from names, a member under two directories, an edition no longer named, a
/// refused kind (counted every pass), an open proposal (raised every pass)
/// and hand values the derive would not reach. Its report must equal a pass
/// that writes every work.
#[test]
fn a_derive_over_unchanged_files_writes_nothing() {
    let tree = fixture();
    let chapters = chaptered();
    let (first, second) = crate::tests::a_copy_of_one_volume();
    let store = crate::tests::tempdir("derive-quiet");
    let db = store.path().join("library.db");
    let library = Library::open(&db).expect("open");
    let mut conn = library.conn();
    for dir in [tree.path(), chapters.path(), first.path(), second.path()] {
        let root = scan::add_root(&conn, dir).expect("add");
        scan::scan_root(&conn, &root).expect("scan");
    }
    a_rule_setting(&conn, Some("bootleg"), Some("Japanese"));
    derive::derive(&conn).expect("first derive");
    a_kept_edition(&conn, chapters.path());
    // Hand and column set together, as the product's writers set them, so a
    // check that compared the column with the derived value would rewrite it.
    let edition =
        "(SELECT edition_id FROM member WHERE path LIKE '%Go-Toubun no Hanayome - v01 (2019)')";
    let work = format!("(SELECT work_id FROM edition WHERE id = {edition})");
    conn.execute_batch(&format!(
        "UPDATE work SET series_id_hand = {series}, series_id = {series},
                         volume_hand = 7, volume = 7
          WHERE id = {work};
         UPDATE edition SET label_hand = 'by hand', label = 'by hand',
                            kind_hand = 'official', kind = 'official',
                            language_hand = 'en', language = 'en'
          WHERE id = {edition}",
        series = another_series(&work),
    ))
    .expect("a work edited by hand");
    conn.execute("UPDATE edition SET language = 'stale'", [])
        .expect("a value the next derive writes back");
    let writing = derive::derive(&conn).expect("a derive that writes every work");

    assert!(
        writing.unrecognised_kinds > 0 && writing.kept_editions > 0,
        "the fixture: a refused kind and a kept edition: {writing:?}"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM edition e JOIN work w ON w.id = e.work_id
                  WHERE e.id = {edition} AND w.series_id = w.series_id_hand
                    AND w.volume = 7 AND e.label = 'by hand' AND e.kind = 'official'
                    AND e.language = 'en'"
            )
        ),
        1,
        "the fixture: a work written from its hand values"
    );
    assert!(
        scalar(&conn, "SELECT count(*) FROM proposal WHERE state = 'open'") > 0,
        "the fixture: an open proposal"
    );
    assert!(
        scalar(
            &conn,
            "SELECT count(*) FROM member WHERE chapter_source = 'filename'"
        ) > 0,
        "the fixture: a chapter from a name"
    );
    assert!(
        scalar(
            &conn,
            "SELECT count(*) FROM (SELECT member_id FROM member_root
               GROUP BY member_id HAVING count(*) > 1)"
        ) > 0,
        "the fixture: a member under two directories"
    );

    let watcher = Connection::open(&db).expect("a second connection");
    let data_version = |c: &Connection| -> i64 {
        c.query_row("PRAGMA data_version", [], |r| r.get(0))
            .expect("data_version")
    };
    let seen = data_version(&watcher);
    let changes = conn.total_changes();
    let counter = crate::structure::version(&conn).expect("reads");

    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    let quiet = quiet.expect("a derive over nothing changed");

    assert_eq!(quiet, writing, "counted as a pass that writes");
    assert_eq!(commits, 0, "no write transaction committed");
    assert_eq!(conn.total_changes(), changes, "no row changed");
    assert_eq!(
        crate::structure::version(&conn).expect("reads"),
        counter,
        "the structure counter stays"
    );
    assert_eq!(
        data_version(&watcher),
        seen,
        "another connection sees no commit"
    );
}

/// O272: a dismissed proposal is never rescored, so a derive raising it at
/// another score takes no write transaction for it.
#[test]
fn a_derive_takes_no_write_for_a_dismissed_proposal_whose_score_moved() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");
    let dismissed = "SELECT count(*) FROM proposal WHERE state = 'dismissed' AND score = 0.5";
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM proposal WHERE score = 0.5"),
        0,
        "the fixture: a score the derive does not give"
    );
    conn.execute("UPDATE proposal SET state = 'dismissed', score = 0.5", [])
        .expect("dismissed at that score");
    assert_eq!(scalar(&conn, dismissed), 1, "the fixture: one proposal");

    let (commits, report) = commits_during(&mut conn, derive::derive);
    report.expect("derives");

    assert_eq!(
        commits, 0,
        "no write transaction for the dismissed proposal"
    );
    assert_eq!(scalar(&conn, dismissed), 1, "left as it was dismissed");
}

/// The lowest series id other than the one `work` (a subquery) is in.
fn another_series(work: &str) -> String {
    format!(
        "(SELECT min(id) FROM series WHERE id <> (SELECT series_id FROM work WHERE id = {work}))"
    )
}

/// Names the editions of `dir`'s directory `scan` and derives, so each work
/// there keeps its `raw` edition beside the new one
/// ([`derive::DeriveReport::kept_editions`]).
fn a_kept_edition(conn: &Connection, dir: &std::path::Path) {
    let root = scan::roots(conn)
        .expect("lists")
        .into_iter()
        .find(|r| r.path == dir.canonicalize().expect("canonical"))
        .expect("the directory");
    scan::update_root(
        conn,
        root.id,
        &scan::RootPatch {
            edition_key: Some(Some("scan".to_string())),
            ..Default::default()
        },
    )
    .expect("renames the directory's edition");
    let report = derive::derive(conn).expect("derives the new edition");
    assert!(report.kept_editions > 0, "the fixture: {report:?}");
}

/// Every row of the structure a derive writes, without row ids.
fn work_rows(conn: &Connection) -> String {
    let rows = |sql: &str| -> Vec<String> {
        let mut stmt = conn.prepare(sql).expect(sql);
        let n = stmt.column_count();
        stmt.query_map([], |r| {
            (0..n)
                .map(|i| {
                    r.get::<_, rusqlite::types::Value>(i)
                        .map(|v| format!("{v:?}"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|v| v.join("|"))
        })
        .expect(sql)
        .collect::<Result<_, _>>()
        .expect(sql)
    };
    format!(
        "works {:?}\neditions {:?}\nmembers {:?}\ndirectories {:?}",
        rows("SELECT content_key, series_id, volume FROM work ORDER BY content_key"),
        rows(
            "SELECT w.content_key, e.key, e.label, e.label_hand, e.label_derived, e.kind,
                    e.language
               FROM edition e JOIN work w ON w.id = e.work_id ORDER BY 1, 2"
        ),
        rows(
            "SELECT w.content_key, e.key, m.ordinal, m.path, m.sha256, m.pages,
                    m.chapter_declared, m.chapter_source, m.root_id
               FROM member m JOIN edition e ON e.id = m.edition_id
               JOIN work w ON w.id = e.work_id ORDER BY 1, 2, 3"
        ),
        rows(
            "SELECT e.key, m.path, mr.root_id FROM member_root mr
               JOIN member m ON m.id = mr.member_id
               JOIN edition e ON e.id = m.edition_id ORDER BY 1, 2, 3"
        ),
    )
}

/// O272: a held work is skipped only where the derive would write nothing to
/// it. Each change moves one thing a derive writes on a work whose files have
/// not changed, and the next derive puts it back.
#[test]
fn a_derive_writes_back_what_a_held_work_no_longer_holds() {
    let tree = fixture();
    let chapters = chaptered();
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    for dir in [tree.path(), chapters.path()] {
        let root = scan::add_root(&conn, dir).expect("add");
        scan::scan_root(&conn, &root).expect("scan");
    }
    derive::derive(&conn).expect("first derive");
    a_kept_edition(&conn, chapters.path());
    let held = work_rows(&conn);

    let kept = "(SELECT id FROM edition WHERE key = 'raw'
                  AND work_id IN (SELECT work_id FROM edition WHERE key = 'scan'))";
    let named = format!("(SELECT id FROM edition WHERE id NOT IN {kept})");
    let edition = format!("(SELECT min(id) FROM edition WHERE id IN {named})");
    let member = format!("(SELECT min(id) FROM member WHERE edition_id IN {named})");
    let chaptered = format!(
        "(SELECT min(id) FROM member WHERE edition_id IN {named} AND chapter_source = 'filename')"
    );
    let changes = [
        ("the series", "UPDATE work SET series_id = NULL".to_string()),
        ("the volume", "UPDATE work SET volume = 99".to_string()),
        (
            "the derived label",
            format!("UPDATE edition SET label_derived = NULL WHERE id = {edition}"),
        ),
        (
            "the label",
            format!("UPDATE edition SET label = 'elsewhere' WHERE id = {edition}"),
        ),
        (
            "the kind",
            format!("UPDATE edition SET kind = 'fan' WHERE id = {edition}"),
        ),
        (
            "the language",
            format!("UPDATE edition SET language = 'xx' WHERE id = {edition}"),
        ),
        (
            "an edition",
            "DELETE FROM edition WHERE id = (SELECT max(id) FROM edition)".to_string(),
        ),
        (
            "a member",
            format!(
                "DELETE FROM member WHERE id = (SELECT max(id) FROM member WHERE edition_id = (SELECT edition_id FROM member WHERE id = {chaptered}))"
            ),
        ),
        (
            "an ordinal",
            format!("UPDATE member SET ordinal = ordinal + 5 WHERE id = {member}"),
        ),
        (
            "a path",
            format!("UPDATE member SET path = path || '.moved' WHERE id = {member}"),
        ),
        (
            "a digest",
            format!("UPDATE member SET sha256 = 'beef' WHERE id = {member}"),
        ),
        (
            "a page count",
            format!("UPDATE member SET pages = pages + 1 WHERE id = {member}"),
        ),
        (
            "a directory",
            format!("UPDATE member SET root_id = NULL WHERE id = {member}"),
        ),
        (
            "a directory column alone",
            format!(
                "UPDATE member SET root_id = NULL WHERE id = {member};
                 INSERT INTO member_root (member_id, root_id)
                 SELECT {member}, min(id) FROM root"
            ),
        ),
        (
            "a directory row",
            format!("DELETE FROM member_root WHERE member_id = {member}"),
        ),
        (
            "a chapter",
            format!("UPDATE member SET chapter_declared = 99 WHERE id = {chaptered}"),
        ),
        (
            "a kept edition's chapter",
            format!(
                "UPDATE member SET chapter_declared = 3, chapter_source = 'filename' WHERE id = (SELECT min(id) FROM member WHERE edition_id IN {kept})"
            ),
        ),
    ];
    for (what, sql) in changes {
        conn.execute_batch(&sql).expect(&sql);
        assert_ne!(work_rows(&conn), held, "the fixture: {what} moved");
        derive::derive(&conn).expect("derives");
        assert_eq!(work_rows(&conn), held, "{what} is written back");
    }

    // A hand value set without its column, which no writer of the product
    // leaves, so only a check that reads the hand value sees the change.
    let edition =
        "(SELECT edition_id FROM member WHERE path LIKE '%Go-Toubun no Hanayome - v01 (2019)')";
    let work = format!("(SELECT work_id FROM edition WHERE id = {edition})");
    let series = another_series(&work);
    for (table, row, column, value) in [
        ("work", work.as_str(), "series_id", series.as_str()),
        ("work", work.as_str(), "volume", "7"),
        ("edition", edition, "label", "'by hand'"),
        ("edition", edition, "kind", "'official'"),
        ("edition", edition, "language", "'en'"),
    ] {
        let set =
            |value: &str| format!("UPDATE {table} SET {column}_hand = {value} WHERE id = {row}");
        let follows = format!("SELECT {column} IS {column}_hand FROM {table} WHERE id = {row}");
        conn.execute_batch(&set(value)).expect(column);
        assert_eq!(
            scalar(&conn, &follows),
            0,
            "the fixture: {column}_hand moved"
        );
        derive::derive(&conn).expect("derives");
        assert_eq!(
            scalar(&conn, &follows),
            1,
            "{column}_hand is written to {column}"
        );
        conn.execute_batch(&set("NULL")).expect(column);
        derive::derive(&conn).expect("derives");
        assert_eq!(
            work_rows(&conn),
            held,
            "{column} is derived again once its hand value is cleared"
        );
    }

    // Twins share a digest, and which one's chapter the restore keeps is open
    // (`import::held_member_meta`), so this pins only that the check agrees
    // with the write: the work is written once and then left alone.
    conn.execute_batch(&format!(
        "UPDATE member SET chapter_source = 'hand' WHERE id = {chaptered}"
    ))
    .expect("a hand chapter on a member whose kept twin has none");
    assert_ne!(
        work_rows(&conn),
        held,
        "the fixture: a chapter source moved"
    );
    let (written, _) = commits_during(&mut conn, |conn| derive::derive(conn).expect("derives"));
    let after = work_rows(&conn);
    let (again, _) = commits_during(&mut conn, |conn| derive::derive(conn).expect("derives"));
    assert_eq!(written, 1, "the restore's chapter is written");
    assert_eq!(again, 0, "and the next pass leaves it");
    assert_eq!(work_rows(&conn), after, "as the pass before left it");
}

/// O272: where a hand-set volume puts one work at another's address, the
/// derive counts a rekey on every pass (`write_work` reads the address first),
/// and a pass that could skip the other work counts it as a writing pass does.
#[test]
fn a_rekey_at_an_address_another_work_holds_is_counted_on_every_pass() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");
    conn.execute(
        "UPDATE work SET volume_hand = 2, volume = 2 WHERE volume = 1",
        [],
    )
    .expect("volume 1 set to 2 by hand");

    let first = derive::derive(&conn).expect("derives");
    let second = derive::derive(&conn).expect("derives again");

    assert_eq!(first.rekeyed, 1, "the fixture: {first:?}");
    assert_eq!(second, first, "counted as the pass before");
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
/// series — the two columns the accept form offers as controls, written the
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
    // Accepting an `edition-of` proposal writes a rule holding
    // `set_kind = 'official'`, and the next scan derives the edition that rule
    // names; that edition's `kind` has to come out `official`.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // `librarydoc::apply` is the only other production writer of a value into
    // `edition.kind` and `edition.language`, and it sets `kind_hand` and
    // `language_hand` with them, which a re-derive's `COALESCE` keeps over the
    // rules' answer — as `work.volume` and `work.series_id` keep theirs two
    // lines away.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // And the hand columns do not quiet the whole library: the other work,
    // which no document has touched, follows the rule.
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // `work.title` is what a reader sees, what `work_fts` indexes and, for a
    // book with no series or volume, what the slug is built from (O282). A
    // member with no directory above it has no `/`
    // in its `rel_path`, and taking "the part after the last slash" of a
    // string with no slash is one off-by-one away from `lone - v01`.
    let tree = flat();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    let (title, slug): (String, String) = conn
        .query_row("SELECT title, slug FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("work");
    assert_eq!(title, "Alone - v01");
    assert_eq!(slug, "alone-v01");
}

#[test]
fn a_volume_less_work_still_matches_itself_on_the_next_derive() {
    // `volume` is nullable and `NULL = NULL` is NULL in SQL, so the hub
    // lookup uses `IS`. With `=` a one-shot never matches the row it already
    // minted: the hub is chosen fresh, an arriving edition whose key sorts
    // first takes it, `content_key` moves with it, and `import` inserts a
    // second work — leaving the reader's progress and notes on the first.
    //
    // Both roots are directories of one library, where one one-shot in two
    // directories is one work.
    let first = oneshot("derive-oneshot-a", 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root =
        scan::add_root_in(&conn, first.path(), scan::InLibrary::Named("Manga")).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let (id, volume): (i64, Option<f64>) = conn
        .query_row("SELECT id, volume FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("work");
    assert_eq!(volume, None, "nothing in the name says which volume it is");

    let second = oneshot("derive-oneshot-b", 5);
    let other =
        scan::add_root_in(&conn, second.path(), scan::InLibrary::Named("Manga")).expect("add");
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
fn an_entrys_own_key_takes_its_volume_else_its_captured_title_else_its_path() {
    // A volume is what groups, whatever title a rule also captured: keyed by
    // the title too, two editions of volume 2 titled differently would be two
    // works.
    assert_eq!(
        derive::own_key("Alpha", Some(2.0), Some("Ignored"), "a/x.cbz"),
        ("Alpha".to_string(), Some(2.0f64.to_bits()), None),
    );
    // With no volume, the captured title comes before the path, so two
    // translations a rule titles alike are one work.
    assert_eq!(
        derive::own_key("Alpha", None, Some("Book"), "a/y.cbz"),
        (
            "Alpha".to_string(),
            None,
            Some(derive::Alone::Titled("Book".to_string())),
        ),
    );
    assert_eq!(
        derive::own_key("Alpha", None, None, "a/z.cbz"),
        (
            "Alpha".to_string(),
            None,
            Some(derive::Alone::Path("a/z.cbz".to_string())),
        ),
    );
}

#[test]
fn own_keys_sort_volume_less_first_then_by_volume_and_keep_half_volumes_apart() {
    // Whatever walks groups in key order meets a series' volumes as a reader
    // numbers them, not as their names sort (`v10` before `v2`). Sorted
    // stably by key alone, so two volumes keyed alike keep their input order
    // and show as out of place.
    let volumes = [None, Some(10.0), Some(2.0), Some(1.5), Some(1.0)];
    let mut keyed: Vec<_> = volumes
        .iter()
        .map(|&volume| (derive::own_key("Alpha", volume, None, "a/x.cbz"), volume))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let order: Vec<Option<f64>> = keyed.into_iter().map(|(_, volume)| volume).collect();
    assert_eq!(order, [None, Some(1.0), Some(1.5), Some(2.0), Some(10.0)]);
    // A half-volume is a volume of its own ("omnibus and half-volumes exist"),
    // not more pages of the whole one below it.
    assert_ne!(
        derive::own_key("Alpha", Some(1.0), None, "a/x.cbz"),
        derive::own_key("Alpha", Some(1.5), None, "a/x.cbz"),
    );
}

/// A watch asked to stop from the start.
struct Stopped;

impl crate::progress::Watch for Stopped {
    fn at(&self, _step: Step, _done: u64, _of: Option<u64>) {}

    fn cancelled(&self) -> bool {
        true
    }
}

/// The input an early pass reads: it hashes nothing, so it sizes nothing,
/// and reads no cancel flag, since the only step a cancel here interrupts is
/// sizing. The controls show the fixture sized and cancelled when asked.
#[test]
fn the_input_read_without_sizes_sizes_no_folder_and_reads_no_cancel() {
    let tree = crate::tests::tempdir("derive-input-unsized");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 2);
    write_pages(&tree.path().join("Beta").join("Beta v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, tree.path(), None);
    let (engine, _) = crate::rules::Engine::load(&conn).expect("rules");
    let roots = scan::roots(&conn).expect("lists");
    let read = |watch: &dyn crate::progress::Watch, size: bool| {
        let mut report = derive::DeriveReport::default();
        match derive::read_input(
            &conn,
            &engine,
            &roots,
            &std::collections::HashSet::new(),
            watch,
            &mut report,
            size,
        )
        .expect("reads")
        {
            Outcome::Done(input) => Some(
                input
                    .iter()
                    .flat_map(|(_, kept)| kept)
                    .filter(|(seen, _)| derive::hashes(seen))
                    .map(|(seen, bytes)| (seen.entry.rel_path.clone(), bytes.is_some()))
                    .collect::<Vec<_>>(),
            ),
            Outcome::Cancelled(_) => None,
        }
    };

    let without_sizes = read(&Stopped, false);

    assert_eq!(
        without_sizes,
        Some(vec![
            ("Alpha/Alpha v01.cbz".to_string(), false),
            ("Beta/Beta v01".to_string(), false),
        ]),
        "read whole, with no bytes for either"
    );
    assert_eq!(
        read(&crate::progress::Unwatched, true),
        Some(vec![
            ("Alpha/Alpha v01.cbz".to_string(), true),
            ("Beta/Beta v01".to_string(), true),
        ]),
        "the control: sized when asked"
    );
    assert_eq!(
        read(&Stopped, true),
        None,
        "the control: the folder's sizing reads the flag"
    );
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
        tree.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v03.cbz"),
        b"PK\x03\x04",
    )
    .expect("four bytes of zip signature and nothing behind them");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("a bad archive is not a failed derive");

    assert_eq!(report.works, 2, "v01 and v02, and not the truncated v03");
    assert_eq!(
        report.unreadable,
        derive::Unreadable {
            by_format: formats(&[("pdf", 1)]),
            uncounted: vec![under(
                &root,
                "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v03.cbz"
            )],
            ..derive::Unreadable::default()
        },
        "the PDF, and the archive that would not open, named"
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
    // `work.title` — what a reader sees, what `work_fts` indexes and, for a
    // book with no series or volume, what the slug is built from (O282).
    // Without it the name on disk is the only answer, and
    // `- v01 (2019)` is a scene tag rather than a title.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

/// A root holding a three-page archive at each of `rel_paths`.
fn archives(label: &str, rel_paths: &[&str]) -> crate::tests::TempDir {
    let dir = crate::tests::tempdir(label);
    for rel in rel_paths {
        write_archive(&dir.path().join(rel), 3);
    }
    dir
}

/// Every work's `(volume, slug, title, title_derived)`, by id.
fn minted(conn: &Connection) -> Vec<(Option<f64>, String, String, String)> {
    conn.prepare("SELECT volume, slug, title, title_derived FROM work ORDER BY id")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect")
}

fn minted_row(
    volume: Option<f64>,
    slug: &str,
    title: &str,
) -> (Option<f64>, String, String, String) {
    (volume, slug.into(), title.into(), title.into())
}

#[test]
fn a_scanned_volume_is_minted_its_series_and_volume_and_titled_without_its_extension() {
    // O282 and O284. v02's provisional title slug is already `berserk-v02`,
    // so the mint must not count the row's own slug as another's.
    let tree = archives(
        "derive-minted",
        &[
            "Berserk/Berserk - v01 (2003).cbz",
            "Berserk/Berserk v02.cbz",
        ],
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    derive::derive(&conn).expect("derive");

    let mut works = minted(&conn);
    works.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("volumes compare"));
    assert_eq!(
        works,
        vec![
            minted_row(Some(1.0), "berserk-v01", "Berserk - v01 (2003)"),
            minted_row(Some(2.0), "berserk-v02", "Berserk v02"),
        ]
    );
}

#[test]
fn a_book_renumbered_by_hand_keeps_its_slug() {
    // The slug is an address, minted once (O282). The rule changes the
    // edition's kind, so the second derive rewrites the held row and runs
    // `write_work`'s tail, where an inserted row's slug is minted.
    let tree = archives("derive-renumbered", &["Berserk/Berserk - v01 (2003).cbz"]);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");
    conn.execute("UPDATE work SET volume_hand = 2, volume = 2", [])
        .expect("a person renumbers it");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_kind)
         VALUES (99, 'prefix', '', 'path', 'official')",
        [],
    )
    .expect("a rule that changes the edition");

    derive::derive(&conn).expect("second derive");

    let rows: Vec<(String, f64, Option<String>)> = conn
        .prepare("SELECT w.slug, w.volume, e.kind FROM work w JOIN edition e ON e.work_id = w.id")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        rows,
        vec![("berserk-v01".to_string(), 2.0, Some("official".to_string()))],
        "the slug it was minted with, the hand volume, and the rule's kind — \
         proof the second derive wrote the row rather than leaving it"
    );
}

#[test]
fn a_volume_minted_onto_a_slug_another_work_holds_takes_the_next_suffix() {
    // `work.slug` is UNIQUE, and a document-backed work keeps its title slug.
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let imported = crate::import::sidecar(
        &conn,
        &taiyaku_core::Sidecar::from_json(
            r#"{"taiyaku":1,"work":{"title":"Berserk v01","reading":"rtl"},
                "editions":{"raw":{"label":"raw","members":[
                  {"path":"raw/imported.cbz","sha256":"aa","pages":2}]}},
                "hub":"raw","alignments":[]}"#,
        )
        .expect("parses"),
        std::path::Path::new("/x/taiyaku.json"),
    )
    .expect("imports a work no directory holds");
    let tree = archives("derive-minted-taken", &["Berserk/Berserk - v01 (2003).cbz"]);
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    derive::derive(&conn).expect("derive");

    let slugs: Vec<(i64, String)> = conn
        .prepare("SELECT id, slug FROM work ORDER BY id")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        slugs,
        vec![
            (imported, "berserk-v01".to_string()),
            (imported + 1, "berserk-v01-2".to_string()),
        ]
    );
}

#[test]
fn a_volume_less_book_is_minted_from_its_title() {
    // A series and no volume: the title rule, never a volume of 0.
    let tree = archives("derive-volume-less", &["Look Back/Look Back (2022).cbz"]);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    derive::derive(&conn).expect("derive");

    let series: Option<i64> = conn
        .query_row("SELECT series_id FROM work", [], |r| r.get(0))
        .expect("series");
    assert!(
        series.is_some(),
        "the fixture: a series, from the directory"
    );
    assert_eq!(
        minted(&conn),
        vec![minted_row(None, "look-back-2022", "Look Back (2022)")]
    );
}

#[test]
fn a_title_a_rule_captures_keeps_its_extension() {
    // A capture is a person's choice of title and shows as written (O284);
    // the slug is still the series and the volume.
    let tree = archives("derive-captured", &["Berserk/Berserk - v01 (2003).cbz"]);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (99, 'regex', '^(?<title>.+\\.cbz)$', 'name')",
        [],
    )
    .expect("a rule that captures the whole file name");

    derive::derive(&conn).expect("derive");

    assert_eq!(
        minted(&conn),
        vec![minted_row(
            Some(1.0),
            "berserk-v01",
            "Berserk - v01 (2003).cbz"
        )]
    );
}

#[test]
fn volume_less_books_a_rule_titles_alike_are_one_work_an_edition_each() {
    // Two translations of one book, neither numbered, each in its own
    // directory: the title one rule captures from both is what makes them
    // one work, and a key apiece what keeps them two editions.
    let tree = archives(
        "derive-translations",
        &[
            "Homer/The Odyssey (Fagles)/The Odyssey (Fagles).cbz",
            "Homer/The Odyssey of Homer (Lattimore)/The Odyssey of Homer (Lattimore).cbz",
            "Homer/The Iliad (Fagles)/The Iliad (Fagles).cbz",
        ],
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute_batch(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key)
         VALUES (97, 'prefix', 'Homer/The Odyssey (Fagles)/', 'path', 'fagles'),
                (98, 'prefix', 'Homer/The Odyssey of Homer (Lattimore)/', 'path', 'lattimore');",
    )
    .expect("a key for each translation");
    derive::derive(&conn).expect("derive");
    let live = |conn: &Connection| -> Vec<(String, String)> {
        conn.prepare(
            "SELECT w.title, group_concat(e.key, ',') FROM work w
             JOIN (SELECT * FROM edition ORDER BY key) e ON e.work_id = w.id
             WHERE w.missing_since IS NULL GROUP BY w.id ORDER BY w.title",
        )
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect")
    };
    assert_eq!(
        live(&conn).len(),
        3,
        "untitled, each volume-less book is its own work: {:?}",
        live(&conn)
    );

    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (99, 'regex', '^Homer/(?<title>The Odyssey)\\b', 'path')",
        [],
    )
    .expect("one title for both translations");
    derive::derive(&conn).expect("derive again");

    // A scan names a work once, so the Odyssey keeps the title it was first
    // given; which editions share a work is what the capture decides.
    let mut editions: Vec<String> = live(&conn).into_iter().map(|(_, keys)| keys).collect();
    editions.sort();
    assert_eq!(editions, ["fagles,lattimore", "raw"]);
}

#[test]
fn a_book_a_rule_titles_apart_leaves_the_work_to_the_part_holding_its_hub() {
    // One work of two translations; then a rule gives one its own title. The
    // part holding the hub's file keeps the row, and the other is a new work,
    // rather than both writing over one row in turn.
    let tree = archives(
        "derive-split",
        &[
            "Tolstoy/Anna (Bartlett)/Anna (Bartlett).cbz",
            "Tolstoy/Anna (Schwartz)/Anna (Schwartz).cbz",
        ],
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute_batch(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key)
         VALUES (97, 'prefix', 'Tolstoy/Anna (Bartlett)/', 'path', 'bartlett'),
                (98, 'prefix', 'Tolstoy/Anna (Schwartz)/', 'path', 'schwartz');
         INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (99, 'regex', '^Tolstoy/(?<title>Anna) \\(', 'path');",
    )
    .expect("one title for both");
    derive::derive(&conn).expect("derive");
    let (first, hub): (i64, String) = conn
        .query_row("SELECT id, hub FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("one work");
    assert_eq!(hub, "bartlett", "the key that sorts first");

    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (100, 'regex', '^Tolstoy/(?<title>Anna \\(Bartlett\\))/', 'path')",
        [],
    )
    .expect("Bartlett titled apart");
    derive::derive(&conn).expect("derive again");

    // Every work holding a file: a scan keeps an edition a rule change
    // orphaned (`DeriveReport::kept_editions`), so the old row may hold one too.
    let holders = |file: &str| -> Vec<i64> {
        conn.prepare(
            "SELECT edition.work_id FROM member JOIN edition ON edition.id = member.edition_id
              WHERE member.path LIKE ?1 ORDER BY edition.work_id",
        )
        .expect("prepare")
        .query_map([format!("%{file}")], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect")
    };
    assert_eq!(
        holders("Anna (Bartlett).cbz"),
        [first],
        "the hub's part keeps the row"
    );
    assert!(
        holders("Anna (Schwartz).cbz").iter().any(|&w| w != first),
        "the other is a work of its own: {:?}",
        holders("Anna (Schwartz).cbz")
    );
}

/// Every work holding a member named `file`, in any edition, by id: a scan
/// keeps an edition no group names any more (`DeriveReport::kept_editions`),
/// so a file can sit on the work a group left as well as the one it is on.
fn holders_of(conn: &Connection, file: &str) -> Vec<i64> {
    conn.prepare(
        "SELECT edition.work_id FROM member JOIN edition ON edition.id = member.edition_id
          WHERE member.path LIKE ?1 ORDER BY edition.work_id",
    )
    .expect("prepare")
    .query_map([format!("%/{file}")], |r| r.get(0))
    .expect("query")
    .collect::<Result<_, _>>()
    .expect("collect")
}

/// A root of one unnumbered book in `keys` translations, `Rowling/Stone
/// [key]/Stone [key].cbz`, scanned under a rule that titles them all `Stone`
/// and keys each by its bracket, and derived into one work.
pub(super) fn translations(
    label: &str,
    keys: &[&str],
) -> (crate::tests::TempDir, Library, scan::Root) {
    let rel_paths: Vec<String> = keys
        .iter()
        .map(|key| format!("Rowling/Stone [{key}]/Stone [{key}].cbz"))
        .collect();
    let rel_paths: Vec<&str> = rel_paths.iter().map(String::as_str).collect();
    let tree = archives(label, &rel_paths);
    let library = Library::open_in_memory().expect("open");
    let root = {
        let conn = library.conn();
        let root = scan::add_root(&conn, tree.path()).expect("add");
        scan::scan_root(&conn, &root).expect("scan");
        conn.execute(
            "INSERT INTO rule (ordinal, kind, pattern, match_on)
             VALUES (99, 'regex', '^Rowling/(?<title>Stone) \\[(?<edition>[a-z]+)\\]/', 'path')",
            [],
        )
        .expect("one title for every translation, a key for each");
        derive::derive(&conn).expect("derive");
        root
    };
    (tree, library, root)
}

#[test]
fn a_book_whose_hub_file_goes_keeps_its_work_and_what_was_read_on_it() {
    // The hub's file is the one the row's `content_key` hashes, so once it is
    // gone no group holds a file of the row's hub edition. The translation
    // left is still that book, and the reader's position, notes and pins
    // are on its row: a second row for it would strand them on the first.
    let (tree, library, root) = translations("derive-hub-gone", &["en", "ko"]);
    let conn = library.conn();
    let (work, hub): (i64, String) = conn
        .query_row("SELECT id, hub FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("one work of two editions");
    assert_eq!(hub, "en", "the key that sorts first");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'en', 2)",
        [work],
    )
    .expect("someone is reading the English");
    seed_notes(&conn, work, 1, "ko", &["n1"]);
    crate::book_pins::pin(
        &conn,
        work,
        &crate::book_pins::Pin {
            edition: "ko".to_string(),
            hub_block: 0,
            block: 0,
        },
    )
    .expect("a pin");

    std::fs::remove_dir_all(tree.path().join("Rowling/Stone [en]")).expect("the hub's file goes");
    scan::scan_root(&conn, &root).expect("rescan");
    derive::derive(&conn).expect("derive again");

    assert_eq!(
        holders_of(&conn, "Stone [ko].cbz"),
        [work],
        "the translation left is derived onto the book's row, and no second row holds it"
    );
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 1);
    for (what, table) in [
        ("the position", "progress"),
        ("the note", "note"),
        ("the pin", "book_pin"),
    ] {
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {work}")
            ),
            1,
            "{what} is on the book's row"
        );
    }
    let hub: String = conn
        .query_row("SELECT hub FROM work WHERE id = ?1", [work], |r| r.get(0))
        .expect("the work");
    assert_eq!(
        hub, "en",
        "a scan rewrites the row's key and not its hub, so the hub's file coming back restores both"
    );
}

#[test]
fn translations_a_rule_parts_once_the_hub_file_goes_each_keep_a_work_of_their_own() {
    // Two groups each hold a file of the book's row and neither holds its
    // hub's. Neither may take the row: one of them would be a guess, and both
    // would write over one row in turn on every derive.
    let (tree, library, root) = translations("derive-hub-gone-parted", &["en", "ja", "ko"]);
    let conn = library.conn();
    let work: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("one work of three editions");

    std::fs::remove_dir_all(tree.path().join("Rowling/Stone [en]")).expect("the hub's file goes");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (100, 'regex', '^Rowling/(?<title>Stone \\[ko\\])/', 'path')",
        [],
    )
    .expect("the Korean titled apart");
    scan::scan_root(&conn, &root).expect("rescan");
    derive::derive(&conn).expect("derive");

    let parted = (
        holders_of(&conn, "Stone [ja].cbz"),
        holders_of(&conn, "Stone [ko].cbz"),
    );
    let (ja, ko) = &parted;
    assert!(
        ja.len() == 2 && ko.len() == 2 && ja[0] == work && ko[0] == work && ja[1] != ko[1],
        "each part on a work of its own, the book's row keeping both as it was: {parted:?}"
    );

    let again = derive::derive(&conn).expect("derive again");
    assert_eq!(
        (
            holders_of(&conn, "Stone [ja].cbz"),
            holders_of(&conn, "Stone [ko].cbz"),
        ),
        parted,
        "the next derive leaves each part where it is"
    );
    assert_eq!(again.rekeyed, 0, "and rewrites no work's key");
}

#[test]
fn a_translation_left_with_the_bytes_of_another_works_hub_leaves_no_key_to_rewrite() {
    // The Korean left when the book's hub file goes is, byte for byte, the
    // hub of a work of its own. Content decides before the address, so the
    // Korean is that work's; taking the book's row as well would leave the
    // row's key behind, and every derive would rewrite it again.
    let (tree, library, root) = translations("derive-hub-gone-copy", &["en", "ko"]);
    let conn = library.conn();
    let korean = tree.path().join("Rowling/Stone [ko]/Stone [ko].cbz");
    write_archive_labelled(
        &tree.path().join("Strays/Stone.cbz"),
        3,
        "jpg",
        &korean.to_string_lossy(),
    );
    scan::scan_root(&conn, &root).expect("rescan");
    derive::derive(&conn).expect("derive the copy");
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        2,
        "the copy's own work"
    );

    std::fs::remove_dir_all(tree.path().join("Rowling/Stone [en]")).expect("the hub's file goes");
    scan::scan_root(&conn, &root).expect("rescan");
    derive::derive(&conn).expect("derive");
    let again = derive::derive(&conn).expect("derive again");

    assert_eq!(
        again.rekeyed, 0,
        "a derive with nothing changed rewrites no key"
    );
}

#[test]
fn several_chapter_directories_under_one_parent_are_one_unattached_directory() {
    // §7 counts the *directory*, which is the thing a person would act on:
    // `attach-chapters` asks one question about "which of these constitute
    // volume 3", not one per chapter file. Counting the entries instead reads
    // the same on a fixture holding a single chapter and diverges the moment
    // a real chapter root is scanned.
    let tree = chapters();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // `sidecar.md` records corpus archives doing this. Without the ancestor
    // test the container and all three children become members, the volume's
    // pages are counted twice, and three chapters become editions of nothing.
    let tree = crate::tests::tempdir("derive-nested");
    let volume = tree.path().join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        crate::scan::tests::write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let volume = tree.path().join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let volume = tree.path().join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        write_pages(&volume.join(format!("Series - v01 - {chapter}")), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let series = tree.path().join("Chaptered");
    write_pages(
        &series.join("Chaptered - c001").join("Chaptered - c001.5"),
        2,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.unattached_chapter_dirs, 1, "one question, not two");
    assert_eq!(report.works, 0);
}

#[test]
fn only_a_directory_resolving_a_chapter_and_no_volume_is_asked_about() {
    // O77. `unattached_chapter_dirs` is what `proposals::raise` asks, and
    // `report.unattached_chapter_dirs` above is not its answer — that count is
    // `classify`'s own `chapter_dirs` — so no assertion on the report can see
    // this function's guard. Five directories, one per side of each half of
    // `volume.is_none() && chapter.is_some()`, and the shape the guard exists
    // for twice, under different parents and different series: the answer is
    // a map of two named directories, not a count a swap could keep.
    let tree = crate::tests::tempdir("derive-unattached-guard");
    // A chapter and no volume, twice: the two directories to report.
    write_pages(
        &tree.path().join("Kaguya-sama").join("Kaguya-sama - c001"),
        1,
    );
    write_pages(&tree.path().join("Dandadan").join("Dandadan - c012"), 2);
    // A volume and a chapter: a member of v03 and nothing to ask about.
    write_pages(
        &tree.path().join("Chaptered").join("Chaptered - v03 - c9"),
        3,
    );
    // A volume and no chapter.
    write_pages(&tree.path().join("Series").join("Series - v01"), 4);
    // Neither: `bare_numbered`'s shape, a series and nothing else.
    write_pages(&tree.path().join("ONE PIECE").join("001"), 5);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");
    assert_eq!(
        (report.works, report.unattached_chapter_dirs),
        (3, 2),
        "the fixture is what its comments say: v03, v01 and the volume-less \
         `001` are works, and the two chapter directories are not"
    );

    let asked = derive::unattached_chapter_dirs(&conn).expect("unattached");

    assert_eq!(
        asked,
        BTreeMap::from([
            ((root.id, "Dandadan".to_string()), "Dandadan".to_string()),
            (
                (root.id, "Kaguya-sama".to_string()),
                "Kaguya-sama".to_string()
            ),
        ]),
    );
}

#[test]
fn a_chapter_two_levels_below_a_volume_is_still_inside_it() {
    // Every ancestor is tested, not just the immediate parent. A volume whose
    // chapters sit in a `Chapters/` subdirectory puts one directory the rules
    // give neither a volume nor a chapter between the two, so a check that
    // looked only one level up would let all three chapter directories back
    // in as members of the very work their grandparent forms.
    let tree = crate::tests::tempdir("derive-nested-deep");
    let inside = tree
        .path()
        .join("Series")
        .join("Series - v01")
        .join("Chapters");
    write_pages(&inside.join("Series - v01 - c001"), 2);
    write_pages(&inside.join("Series - v01 - c002"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(report.members, 1, "the volume, two levels above them");
    assert_eq!(
        report.suppressed, 3,
        "the two chapter directories and the `Chapters` directory holding them — \
         that directory is no candidate, but it would have become a volume-less \
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
    // **Ten pages for five.** A volume-less container beside what is inside it
    // would make one work with two members — `Series/Extras` at 5 pages and
    // `Series/Extras/Art Book` at 5 pages, the same five images counted twice.
    let tree = crate::tests::tempdir("derive-extras");
    write_pages(
        &tree.path().join("Series").join("Extras").join("Art Book"),
        5,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // **The inner one.** Taking the container instead satisfies the page count
    // just as well. A volume-less container is a name in a path; what is
    // inside it is the material.
    assert!(path.ends_with("Series/Extras/Art Book"), "{path}");
    assert_eq!(pages, 5, "five pages, counted once");
}

#[test]
fn three_levels_of_volume_less_containers_are_one_member() {
    // The same rule at depth, which is what makes it a rule rather than a
    // special case: were every level of a volume-less chain a member of its
    // own, three levels would produce three members and the same pages three
    // times over.
    let tree = crate::tests::tempdir("derive-extras-deep");
    write_pages(
        &tree
            .path()
            .join("Series")
            .join("Extras")
            .join("Art Books")
            .join("Volume 1 Insert"),
        5,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.members, 1);
    assert_eq!(report.suppressed, 2, "the two levels beneath the member");
}

#[test]
fn a_broad_rule_that_reaches_the_root_does_not_swallow_the_library() {
    // A `prefix` rule with a `set_series` — which is exactly what
    // `proposals::accept` writes — resolves a series for the root's own entry
    // and for every container between it and a volume. Each of those
    // containers defers to what is inside it, and the volume is what is left.
    // Counted rather than merely skipped, so a rule that really does say
    // "every path is one series" is visible as a number rather than as a
    // library that quietly holds less than it did.
    let tree = crate::tests::tempdir("derive-root-defers");
    write_pages(&tree.path().join("Series").join("Series - v01"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // There is nothing inside it to defer to, so the root is the member — and
    // without that arm `title_of` would take "the part after the last slash"
    // of the empty string and answer with it, producing a work with no title
    // at all and the slug `work`, which is `slugify`'s fallback for a name
    // holding nothing alphanumeric.
    let tree = crate::tests::tempdir("derive-root-material");
    write_pages(tree.path(), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
        .path()
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
    // **Eighteen pages in one sequence.** Were volume-less entries keyed by
    // `(series, volume)` alone, every entry the rules gave no volume to would
    // share one key — and member order is page order, which `in_page_order`'s
    // doc calls out: every alignment offset after the first member is computed
    // from it. Four separate works with `volume = NULL` is an answer a person
    // can correct; one work with four unrelated volumes concatenated is not.
    let tree = bare_numbered();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // The same hazard at the scale it turns up in this corpus: `Series/Omake`
    // and `Series/Bonus` are different material, and neither carries a volume
    // marker, so they must not share a group key and one page sequence.
    let tree = crate::tests::tempdir("derive-omake");
    let series = tree.path().join("Series");
    write_pages(&series.join("Omake"), 2);
    write_pages(&series.join("Bonus"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    // after `content_key` misses, and with several volume-less works in one
    // series it is not an address at all: `volume IS NULL` matches all of
    // them and `ORDER BY id LIMIT 1` hands every one of them the first row. A
    // work arriving later would take the row of the work that got there
    // first, with the reader's position on it.
    //
    // So a volume-less group is probed by the files it is made of, which is
    // the same thing that keys it. Removing that probe puts Bonus's content
    // on Omake's row.
    let tree = crate::tests::tempdir("derive-omake-later");
    let series = tree.path().join("Series");
    write_pages(&series.join("Omake"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
    let volume = tree.path().join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        crate::scan::tests::write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.members, 1);
    assert_eq!(report.suppressed, 3, "the three chapter directories");
}

#[test]
fn an_archive_inside_a_volume_directory_is_the_member_rather_than_the_directory() {
    // **Three steps, none of which a fixture of loose images can reach.** The
    // walk gives a directory holding only archives `pages = Some(0)`;
    // `builtin-volume` reads `v01` from that directory's own name, so it would
    // be a member candidate and would suppress the archive inside it; and
    // `member_digest` over a page-less directory is the digest of nothing —
    // the *same* hash for every such directory — so both groups'
    // `content_key`s would collide and one row would swallow the other.
    let tree = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 2);
    assert_eq!(report.members, 2);
    assert_eq!(
        report.suppressed, 0,
        "a directory with no pages of its own suppresses nothing"
    );
    // **The row count as well as the report count**, because the report counts
    // groups written, so it could say two while the database held one row.
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
    // The other half of the case above, and the half the member paths cannot
    // see. `Series/v01` resolves a series and a volume and is admitted, so
    // something has to become of it — and what becomes of it is a count.
    // Without one, "the container stopped being a member" and "the container
    // was never looked at" read identically from the report, which is the
    // failure `DeriveReport` exists to prevent.
    let tree = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.unreadable,
        derive::Unreadable {
            no_pages: 2,
            ..derive::Unreadable::default()
        },
        "the two volume directories, which hold archives rather than pages, \
         and nothing else"
    );
}

/// `formats(&[("rar", 2)])` — an `Unreadable::by_format` to compare against.
fn formats(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs
        .iter()
        .map(|(format, n)| ((*format).to_string(), *n))
        .collect()
}

/// `rel_path` under `root`, spelled the way `Unreadable` names a file.
fn under(root: &scan::Root, rel_path: &str) -> String {
    root.path.join(rel_path).display().to_string()
}

/// O90's own case. A RAR archive named `.cbz` is sniffed as `rar` from its
/// bytes and refused for them, not for what its name claims: by format in a
/// build without a RAR backend, counted under the token its bytes gave, and in
/// a build with one, where a header and no archive behind it has no count, as
/// a file named uncounted.
#[test]
fn a_rar_header_named_cbz_is_refused_by_its_bytes() {
    let tree = crate::tests::tempdir("derive-rar-named-cbz");
    let series = tree.path().join("Vagabond");
    write_pages(&series.join("Vagabond - v01"), 2);
    std::fs::write(
        series.join("Vagabond - v02.cbz"),
        b"Rar!\x1a\x07\x01\x00 and no decoder behind it",
    )
    .expect("a RAR header in a file named .cbz");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1, "v01, and not the RAR");
    let rar = "Vagabond/Vagabond - v02.cbz";
    assert_eq!(
        report.unreadable,
        if taiyaku_read::Format::Rar.is_readable() {
            derive::Unreadable {
                uncounted: vec![under(&root, rar)],
                ..derive::Unreadable::default()
            }
        } else {
            derive::Unreadable {
                by_format: formats(&[("rar", 1)]),
                ..derive::Unreadable::default()
            }
        },
    );
}

/// `by_format` counts per token, so a library holding two refused formats
/// says how many of each. Two RAR headers, one 7-Zip and a file no signature
/// matched, which has no token and is counted as `unknown`, the spelling
/// `taiyaku-serve`'s `stamp` already gives a format nothing recognised. A
/// build with a RAR backend admits RAR, so it names the two headers, which have
/// no count, as uncounted instead.
#[test]
fn each_refused_format_is_counted_under_its_own_token() {
    let tree = crate::tests::tempdir("derive-refused-formats");
    let series = tree.path().join("Series");
    std::fs::create_dir_all(&series).expect("mkdir");
    std::fs::write(series.join("Series - v01.cbz"), b"Rar!\x1a\x07\x00 one").expect("write");
    std::fs::write(series.join("Series - v02.cbr"), b"Rar!\x1a\x07\x00 two").expect("write");
    std::fs::write(
        series.join("Series - v03.cb7"),
        b"7z\xbc\xaf\x27\x1c and nothing else",
    )
    .expect("write");
    std::fs::write(series.join("Series - v04.txt"), b"no signature at all").expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    let mut unreadable = report.unreadable;
    unreadable.uncounted.sort();
    assert_eq!(
        unreadable,
        if taiyaku_read::Format::Rar.is_readable() {
            derive::Unreadable {
                by_format: formats(&[("7z", 1), ("unknown", 1)]),
                uncounted: vec![
                    under(&root, "Series/Series - v01.cbz"),
                    under(&root, "Series/Series - v02.cbr"),
                ],
                ..derive::Unreadable::default()
            }
        } else {
            derive::Unreadable {
                by_format: formats(&[("7z", 1), ("rar", 2), ("unknown", 1)]),
                ..derive::Unreadable::default()
            }
        },
    );
}

/// A zip of JPEG XL pages is a member with its pages counted, beside a JPEG
/// volume: it is a work of its own, not an archive named as uncounted.
#[test]
fn an_archive_of_jpeg_xl_pages_is_a_member_with_its_pages_counted() {
    let tree = crate::tests::tempdir("derive-jxl-counted");
    let series = tree.path().join("Bisque Doll");
    write_archive_of(&series.join("Bisque Doll - v01.cbz"), 3, "jxl");
    write_archive(&series.join("Bisque Doll - v02.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 2, "the JPEG XL volume and the JPEG one");
    assert_eq!(report.unreadable, derive::Unreadable::default());
    let pages: i64 = conn
        .query_row(
            "SELECT pages FROM member WHERE path LIKE '%v01%'",
            [],
            |r| r.get(0),
        )
        .expect("the JPEG XL member");
    assert_eq!(pages, 3, "its pages counted");
}

/// The RAR fixtures, each copied into `series` as a volume of it named `.cbz`,
/// as most RARs are, in the fixtures' order: `(fixture, rel_path)`.
fn rar_volumes(
    fixtures: Vec<taiyaku_read::test_support::RarFixture>,
    series: &std::path::Path,
) -> Vec<(taiyaku_read::test_support::RarFixture, String)> {
    std::fs::create_dir_all(series).expect("mkdir");
    let name = series
        .file_name()
        .expect("a series directory")
        .to_string_lossy()
        .into_owned();
    (1..)
        .zip(fixtures)
        .map(|(volume, fixture)| {
            let file = format!("{name} - v{volume:02}.cbz");
            std::fs::copy(&fixture.path, series.join(&file)).expect("copies the fixture");
            (fixture, format!("{name}/{file}"))
        })
        .collect()
}

/// Each RAR fixture that reads whole is a member with the pages its manifest
/// lists counted, a volume and a work of its own, in a build with a RAR
/// backend. A build without one refuses each by its format, as O90 did.
#[test]
fn a_rar_archive_is_a_member_with_its_pages_counted_in_a_build_that_reads_rar() {
    let tree = crate::tests::tempdir("derive-rar-counted");
    let readable = rar_volumes(
        taiyaku_read::test_support::rar_fixtures()
            .into_iter()
            .filter(|fixture| fixture.refused.is_none() && !fixture.pages.is_empty())
            .collect(),
        &tree.path().join("Rar"),
    );
    assert!(readable.len() > 1, "the fixture: RARs that read whole");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    if !taiyaku_read::Format::Rar.is_readable() {
        assert_eq!(report.works, 0, "{report:?}");
        assert_eq!(
            report.unreadable,
            derive::Unreadable {
                by_format: formats(&[("rar", readable.len())]),
                ..derive::Unreadable::default()
            },
        );
        return;
    }
    assert_eq!(report.works, readable.len(), "one work a volume");
    assert_eq!(report.unreadable, derive::Unreadable::default());
    for (fixture, rel_path) in &readable {
        let pages: i64 = conn
            .query_row(
                "SELECT pages FROM member WHERE path = ?1",
                [under(&root, rel_path)],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("{}: a member: {e}", fixture.file));
        assert_eq!(
            usize::try_from(pages).ok(),
            Some(fixture.pages.len()),
            "{}",
            fixture.file
        );
    }
}

/// Each RAR fixture refused at the count, and the one that holds no page, is
/// named as uncounted in a build with a RAR backend: the derive admits RAR
/// there, and the scan had no count to give it. A build without one refuses
/// each by its format.
#[test]
fn a_rar_archive_refused_at_the_count_is_named_as_uncounted_in_a_build_that_reads_rar() {
    let tree = crate::tests::tempdir("derive-rar-uncounted");
    let refused = rar_volumes(
        taiyaku_read::test_support::rar_fixtures()
            .into_iter()
            .filter(|fixture| fixture.refused.is_some() || fixture.pages.is_empty())
            .collect(),
        &tree.path().join("Rar"),
    );
    assert!(
        refused.iter().any(|(fixture, _)| fixture.refused.is_none()),
        "the fixture: a RAR holding no page"
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 0, "{report:?}");
    let mut unreadable = report.unreadable;
    unreadable.uncounted.sort();
    let mut uncounted: Vec<String> = refused
        .iter()
        .map(|(_, rel_path)| under(&root, rel_path))
        .collect();
    uncounted.sort();
    assert_eq!(
        unreadable,
        if taiyaku_read::Format::Rar.is_readable() {
            derive::Unreadable {
                uncounted,
                ..derive::Unreadable::default()
            }
        } else {
            derive::Unreadable {
                by_format: formats(&[("rar", refused.len())]),
                ..derive::Unreadable::default()
            }
        },
    );
}

/// A zip that opens cleanly and holds only HEIC pages, which `taiyaku-read`
/// does not read. The walk cannot count its pages, so derivation names it with
/// the archives that would not open. It is not a vanished member.
#[test]
fn an_archive_of_pages_taiyaku_does_not_read_is_named_as_uncounted() {
    let tree = crate::tests::tempdir("derive-heic-archive");
    let series = tree.path().join("Undecoded");
    write_archive_of(&series.join("Undecoded - v01.cbz"), 3, "heic");
    write_archive(&series.join("Undecoded - v02.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1, "v02, and not the HEIC archive");
    assert_eq!(
        report.unreadable,
        derive::Unreadable {
            uncounted: vec![under(&root, "Undecoded/Undecoded - v01.cbz")],
            ..derive::Unreadable::default()
        },
    );
}

/// `refused_format` and `total` over a report whose causes all differ, so a
/// sum that drops any one term, or counts one twice, gives another number.
#[test]
fn the_unreadable_sums_count_every_cause_once() {
    let named = |n: usize| (0..n).map(|i| format!("/library/{i}")).collect::<Vec<_>>();
    let unreadable = derive::Unreadable {
        by_format: formats(&[("7z", 3), ("rar", 2)]),
        uncounted: named(7),
        no_pages: 11,
        vanished: named(13),
    };

    assert_eq!(unreadable.refused_format(), 5);
    assert_eq!(unreadable.total(), 5 + 7 + 11 + 13);
    assert_eq!(derive::Unreadable::default().total(), 0);
}

#[test]
fn a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
        tree.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01 (2019)")
            .join("003.jpg"),
        b"one more page",
    )
    .expect("write");
    // The member is the *directory*, and a directory's `mtime` moves when an
    // entry is added or removed — not when the bytes inside an existing one
    // change. So without this stamp `scan::due` skips the re-hash and the
    // digest stays what it was, which is the same second-resolution flake as
    // `a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive`.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    std::fs::File::open(
        tree.path()
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

/// `Series - v01` renamed `Series - v02`, a reader's position on it, while a
/// new `Series - v01` takes its old number. The work at volume 1 is the
/// renamed files' by content, so it moves to volume 2 with them and the
/// position, and the new volume 1 takes a work of its own; each file is on
/// one work, and the next derive commits nothing.
#[test]
fn a_volume_renumbered_by_its_name_keeps_its_work_from_the_volume_taking_its_number() {
    let dir = crate::tests::tempdir("derive-renumbered");
    let series = dir.path().join("Series");
    write_pages(&series.join("Series - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let root = scanned_root(&conn, dir.path(), None);
    derive::derive(&conn).expect("derives");
    let renamed = work_under(&conn, &root).0;
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [renamed],
    )
    .expect("a position");
    std::fs::rename(series.join("Series - v01"), series.join("Series - v02"))
        .expect("renames volume 1 to volume 2");
    write_pages(&series.join("Series - v01"), 2);
    // A directory's mtime has a second's resolution, and `scan::due` reads
    // an unchanged one as unchanged bytes.
    std::fs::File::open(series.join("Series - v01"))
        .expect("opens the new volume 1")
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10)),
        )
        .expect("stamps it as changed");
    scan::scan_root(&conn, &root).expect("rescans");

    derive::derive(&conn).expect("re-derives");

    let arrived = holders_of(&conn, "Series - v01");
    assert_eq!(
        (holders_of(&conn, "Series - v02"), arrived.len()),
        (vec![renamed], 1),
        "each file is on one work, the renamed one on the work it had"
    );
    assert_ne!(
        arrived[0], renamed,
        "the new volume 1 is on a work of its own"
    );
    let volume = |work: i64| {
        scalar(
            &conn,
            &format!("SELECT CAST(volume AS INTEGER) FROM work WHERE id = {work}"),
        )
    };
    assert_eq!(
        (volume(renamed), volume(arrived[0])),
        (2, 1),
        "the renamed files' work is volume 2"
    );
    assert_eq!(
        scalar(&conn, "SELECT work_id FROM progress WHERE slot = 2"),
        renamed,
        "the position stays with the renamed files"
    );
    let (commits, _) = commits_during(&mut conn, |conn| derive::derive(conn).expect("derives"));
    assert_eq!(commits, 0, "the next derive commits nothing");
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
    write_pages(&tree.path().join("Alpha").join("Alpha - v01"), 3);
    let beta = tree.path().join("Beta").join("Beta - v01");
    write_pages(&beta, 5);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    // Beta's volume is replaced with a byte-for-byte copy of Alpha's — copied
    // explicitly rather than written by a second `write_pages` call, because
    // `write_pages` bakes its own directory into every page's bytes precisely
    // so two calls can never collide by accident. Deliberate duplication
    // belongs at the call site that wants it.
    let alpha = tree.path().join("Alpha").join("Alpha - v01");
    std::fs::remove_dir_all(&beta).expect("remove");
    std::fs::create_dir_all(&beta).expect("mkdir");
    for page in 0..3 {
        std::fs::copy(
            alpha.join(format!("{page:03}.jpg")),
            beta.join(format!("{page:03}.jpg")),
        )
        .expect("copy");
    }
    // **The rescan has to be able to see the change.** `scan::due` compares
    // `(size, mtime)`, a directory's size is its block size whether it holds
    // three entries or five, and `mtime` has second resolution — so a rebuild
    // that lands in the same second as the first scan is skipped and Beta
    // keeps the digest of its *old* five pages. The assertion below then
    // compares Alpha's hash against a stale one and fails.
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
    // `UNIQUE constraint failed: work.content_key` case this test exists for.
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

/// The derive's `is_readable` gives `taiyaku_read::Format::is_readable`'s
/// answer for every format's token. It reads the token back through
/// `Format::from_token`, and a token read back as no format would be silent:
/// a member the derive believes unreadable, counted as a fault, and never
/// offered to a reader that could have opened it.
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
    write_pages(&english.path().join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("derive-root-raw");
    write_pages(&raw.path().join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    let en_root = scan::add_root_in(&conn, english.path(), scan::InLibrary::Named("Manga"))
        .expect("adds the English root");
    let raw_root = scan::add_root_in(&conn, raw.path(), scan::InLibrary::Named("Manga"))
        .expect("adds the raw root");
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
    write_pages(&dir.path().join("Series").join("Series - v01"), 2);

    // A second, real root to scribble with: `member.root_id` REFERENCES
    // `root(id)`, so a made-up id is refused by the constraint rather than
    // stored. Scribbling a root that genuinely exists is also the stronger
    // assertion, since it proves the re-derive writes the *right* root and not
    // merely a different one.
    let other = crate::tests::tempdir("derive-root-rewrite-other");
    write_pages(&other.path().join("Other").join("Other - v01"), 1);

    let library = Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("adds the root");
    let wrong_root = scan::add_root(&conn, other.path()).expect("adds the other root");
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
    // paying for three passes over every `scan_entry` row in the library. The
    // property that makes the plural form substitutable for the singular one
    // is this: each title gets exactly what a separate call would have
    // given it, and a title nothing resolves to is present and empty rather
    // than missing.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");

    let asked = std::collections::BTreeSet::from([
        "Go-Toubun no Hanayome",
        "Kaguya-sama - Fan Translated",
        "A Series Nothing Here Is",
    ]);
    let together = derive::rel_paths_by_series(&conn, &asked).expect("one pass");
    let answered = |title: &str| &together[title];

    assert_eq!(
        together.keys().map(String::as_str).collect::<Vec<_>>(),
        asked.iter().copied().collect::<Vec<_>>(),
        "every title asked about is answered, including the one with nothing under it",
    );
    for title in &asked {
        assert_eq!(
            *answered(title),
            derive::rel_paths_for_series(&conn, title).expect("one at a time"),
            "`{title}` is answered the same either way",
        );
    }
    assert!(
        !answered("Go-Toubun no Hanayome").is_empty()
            && !answered("Kaguya-sama - Fan Translated").is_empty(),
        "and two of the three genuinely have entries, or this compares empty against empty: \
         {together:?}",
    );
    assert!(answered("A Series Nothing Here Is").is_empty());
}

/// One `work` row, one `edition` and one `member` per `(edition key, path,
/// sha256)` triple, every member's `root_id` pointing at root 1 — the shape a
/// scan leaves behind. `seed_work_without_root` is the same shape with
/// `root_id` left NULL, which is what an imported work (no root at all) and a
/// work whose root was later removed (`ON DELETE SET NULL`) both look like.
///
/// **`path` is given explicitly, not derived from `name`/`key`.** `superseded`
/// matches a candidate's members against a target's by `path` as well as by
/// `sha256`, so a fixture meaning "this file moved from one work's edition to
/// another's" has to give the two members the identical path a real fold
/// leaves behind — the file never moves on disk, only the classification of
/// who owns it does — while a fixture meaning "coincidence, not a fold" has to
/// give them different ones.
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

/// A work whose every member is held by a work this derive derived.
///
/// **Not "a work with zero editions", which never occurs.** Under
/// `Scope::Scan`, `import::replace_editions` deletes only the editions the
/// incoming document names, and the superseded work is not in `groups` at all
/// — so nothing empties it.
///
/// **The covering member shares work 2's exact path.** That is what a fold
/// actually leaves behind — see `superseded`'s own doc — and it is deliberate
/// here rather than incidental:
/// `a_byte_identical_file_at_a_different_path_is_not_superseded` is the
/// fixture that isolates what happens when it does not.
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

/// A byte-identical file at a different path is not evidence of a fold.
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

/// A stale edition on a written work is not evidence of a fold.
///
/// Work 1 is written this pass, but only its `full-color` edition is: `raw`
/// is exactly `DeriveReport::kept_editions`'s shape, a row a rule edit
/// orphaned that this derive left standing rather than deleting. Work 2's
/// only member matches `raw`'s path and digest, not `full-color`'s, so it
/// must not read as covered.
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
/// a first-write-wins map from digest to one "owner".
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
/// `candidates.filter(|c| !c.is_empty())` agree on every one of them — the
/// latter would additionally take the lowest work id as an arbitrary target.
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

/// A written work that is *not* the target sits in the written set here.
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
            taiyaku_notes: crate::NOTES_VERSION,
            work: "ignored".to_string(),
            notes: ids
                .iter()
                .map(|id| crate::Note {
                    text: None,
                    id: (*id).to_string(),
                    edition: edition.to_string(),
                    slot: 2,
                    region: None,
                    color: None,
                    body: format!("the sign, note {id}"),
                    created: "2026-08-27T09:00:00Z".to_string(),
                    updated: "2026-08-27T09:00:00Z".to_string(),
                    stale: false,
                    at: None,
                })
                .collect(),
        },
    )
    .expect("the reader's notes");
}

/// The two directories the tests below start from: a base series and a Full
/// Color one, scanned and derived into two separate works, with the id of the
/// colour work returned.
fn two_works_before_the_fold(tree: &crate::tests::TempDir, conn: &Connection) -> i64 {
    write_pages(&tree.path().join("Series").join("Series - v01"), 12);
    write_pages(
        &tree
            .path()
            .join("Series - Full Color")
            .join("Series - Full Color - v01"),
        12,
    );
    let root = scan::add_root(conn, tree.path()).expect("add");
    // Migration 012 defaults `keeps_documents` off; the tests built on this
    // fixture that hand-edit the colour work need its document to actually
    // reach disk, so this opts in — harmless for the others, which never
    // call `hand::save`.
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
/// or the fold never being called. This test goes through `derive()` itself,
/// over real scanned directories: two directories start as two separate series
/// and two separate works, and a `set_series` + `set_edition_key` rule — the
/// same shape `proposals::accept` writes, and the one the "O73's cascade,
/// measured" reproduction uses against real archives — folds the second into
/// the first as a new edition on the next derive.
///
/// **The reader is planted before the fold and read back after it**, through
/// `state`'s own writers rather than by `INSERT`, because the whole claim is
/// that their place survives a rescan they never asked for.
///
/// **Every counter gets its own number, and that is the point of the fixture's
/// shape.** `derive` fans one `fold::Moved` out into eight report fields. Two
/// readers who move, three notes, four pages moved, five pages the target
/// already held, six discarded rows, seven readings destroyed, eight identical
/// note copies dropped and the nine alignments below make every one of the
/// eight pinned by the number in it rather than by luck, so swapping two
/// assignments, or deleting `report.discarded_rows += …`, fails it.
///
/// **`lost_positions` is pinned at seven, not at `0`.** `usize::default()` is
/// `0`, so asserting `0` would pass with `report.lost_positions +=
/// moved.lost_positions;` deleted from the fan-out: it would say "none was
/// lost" and pin "the counter was never reached" equally well — which is the
/// one thing a counter for a destroyed reading must not do. Seven readers
/// therefore arrive holding a *newer* place on the target's mapped edition
/// than the one they hold on the work about to be folded, which is exactly the
/// shape `fold::move_positions`' `WHERE excluded.updated > progress.updated`
/// makes a silent no-op, and `Moved::lost_positions` exists to see.
///
/// **`uncarried_alignments` (O76).** Nine alignments on the work about to be
/// folded name edition keys it does not hold, so no fold can carry them and
/// each is counted — nine, in no other field.
///
/// **`dropped_notes` (O78).** Reader 2 holds eight notes on both works in
/// identical copies, so the fold drops eight and moves none of them — a
/// number no other field here carries — and the target's own eight rows are
/// read back by `rowid`, because a dropped copy and a dropped original leave
/// the same columns behind.
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

    for gone in 1..=9 {
        conn.execute(
            "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, 'raw', 'gone-' || ?2, 'solver')",
            [colour, gone],
        )
        .expect("an alignment naming an edition the work does not hold");
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
            second.uncarried_alignments,
        ),
        (1, 0, 2, 3, 4, 5, 6, 7, 8, 9),
        "the Full Color work folded away; two places and three notes moved, \
         four labelled pages moved and five stayed the target's, six rows \
         the fold does not move were counted before the delete took them, \
         seven readings the newer target place overrode stopped existing, \
         eight identical note copies were dropped rather than moved, and \
         nine alignments naming editions the work did not hold were left"
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
/// `edition` table, so without the guard this note would be moved by nothing,
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
/// provenance column for exactly this reason. Projecting plain `volume` and
/// `series_id` instead, whatever the last derive wrote, would make the document
/// every save writes claim a *derived* volume as though a person had chosen it.
///
/// `MetadataEditor.tsx` sends `{ title }` unconditionally on Save, and title
/// is the only field the shipped UI edits — so pressing Save, even with no
/// change at all, would be enough, and the state would have no way back: no
/// route can clear a field, because `#[serde(default)] Option<T>` makes an
/// absent key and an explicit `null` identical on the wire.
///
/// This asserts the root cause rather than the symptom: a document describing
/// a work nobody has edited must claim no volume and no series.
#[test]
fn a_title_only_edit_claims_no_volume_and_no_series() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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
/// **How this test reaches a rekey.** It reaches the identical rekey
/// `a_repacked_volume_takes_its_document_with_it` exercises through a real
/// archive rewrite, but by corrupting the digest a layer lower instead — see
/// the comment at the `UPDATE scan_entry` below for why that is needed to get
/// past `import::scanned`'s own self-healing. `hand::rename_from` carries
/// the document to its new address before `reconcile` ever sees a gap. A
/// second technique reaching the same rekey is worth keeping alongside the
/// archive-rewrite test: a fix that works for one route to a changed digest
/// and not the other is exactly the shape a single fixture would hide.
#[test]
fn a_rekeyed_digest_takes_its_document_with_it() {
    let dir = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add");
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
        .strip_prefix(&format!("{}/", dir.path().to_string_lossy()))
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
        "rename_from carried the document to its new address, rather \
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

/// Make the `UPDATE work` that `write_work` runs after the rename refuse,
/// for work `work_id` alone — a failure late in the work, past every write
/// the import and `write_roots` made.
fn refuse_the_series_update(conn: &Connection, work_id: i64) {
    conn.execute_batch(&format!(
        "CREATE TEMP TRIGGER refuse_the_series_update
           BEFORE UPDATE OF series_id, volume ON work WHEN OLD.id = {work_id}
         BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;"
    ))
    .expect("installing the refusal");
}

/// **O210: a work that fails part-way leaves none of its rows behind.**
/// `write_work` commits once, at the end, so a refusal after `write_roots`
/// rolls back what `write_roots` wrote along with the rest.
#[test]
fn a_work_that_fails_part_way_leaves_none_of_its_rows_behind() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");
    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("a work was derived");
    conn.execute(
        "UPDATE member SET root_id = NULL
          WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
        [work_id],
    )
    .expect("clearing the roots write_roots puts back");
    refuse_the_series_update(&conn, work_id);

    let refused = derive::derive(&conn).expect_err("the refusal fails the derive");

    assert!(refused.contains("refused by the test"), "{refused}");
    let roots: Vec<Option<i64>> = conn
        .prepare(
            "SELECT m.root_id FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE e.work_id = ?1",
        )
        .expect("prepare")
        .query_map([work_id], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        roots,
        vec![None],
        "write_roots ran before the refusal, and its write went back with the work"
    );
}

/// **O210: a rekeyed work commits before its document moves.** The rename
/// must follow rows that are committed, so `write_work` commits a rekeyed
/// work's import and `write_roots` first, renames, and writes the rest in a
/// second scope. A refusal in that second scope then leaves the document at
/// the address the committed key gives it. Renamed inside the one scope, the
/// refusal would roll the key back and leave the document under a key the
/// database never kept — the strand `rename_from` exists to prevent.
#[test]
fn a_rekeyed_works_document_follows_only_a_committed_key() {
    let dir = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add");
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("the fixture keeps documents");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");
    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |r| r.get(0))
        .expect("a work was derived");
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");
    let key_before: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("the key the document was saved under");
    // The rekey `a_rekeyed_digest_takes_its_document_with_it` makes, by the
    // same route: a digest the next derive trusts without re-hashing.
    conn.execute(
        "UPDATE scan_entry SET sha256 = 'moved'
          WHERE rel_path = (SELECT substr(m.path, length(r.path) + 2)
                              FROM member m
                              JOIN edition e ON e.id = m.edition_id
                              JOIN root r ON r.id = m.root_id
                             WHERE e.work_id = ?1)",
        [work_id],
    )
    .expect("rekeying");
    refuse_the_series_update(&conn, work_id);

    derive::derive(&conn).expect_err("the refusal fails the derive");

    let path = crate::hand::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    assert!(
        path.is_file(),
        "the document is at the address the committed key gives: {}",
        path.display()
    );
    let key_after: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("the work is still there");
    assert_ne!(
        key_after, key_before,
        "the rekey was committed, so the document moved"
    );
}

/// **The reason the `hand::reconcile` loop sits last** — after the fold, not
/// merely after `write_roots`.
///
/// `migrated_document.work_id` is `ON DELETE CASCADE`, and `fold::absorb`'s own
/// `DELETE FROM work` takes that cascade. The moment `colour` below is folded
/// away, its `migrated_document` row goes with it — so `hand::reconcile`'s
/// `owed` query, which reads that table, stops naming `colour` at all. Its
/// `.library.json` file is untouched by the fold (nothing in `fold::absorb`
/// deletes a file), so it is still sitting on disk, and it is now a document
/// that nothing in the library addresses: exactly [`crate::hand::Orphan`].
///
/// **Reconciling before the fold would miss it.** Moved earlier, the loop
/// would run while `colour` still has its row, so its address is still in
/// `expected`, the file is still `present`, the two match, and nothing is
/// reported — the orphan surfaces only on some *later* pass, after something
/// else disturbs the work it was folded into.
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
    assert_eq!(
        target_title, "Hand Set",
        "the target carries the folded work's hand title"
    );
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

/// A hand title through `librarydoc::apply`, the route a person's edit takes.
fn set_hand_title(conn: &Connection, work_id: i64, title: Option<&str>) {
    let mut doc = crate::librarydoc::of(conn, work_id).expect("projecting");
    doc.title = title.map(str::to_string);
    crate::librarydoc::apply(conn, work_id, &doc).expect("applying");
}

/// **The derive's own fold carries a hand title, and is refused while the two
/// works hold different ones** — `fold::absorb`'s rule, reached through
/// `derive_with`'s `superseded` path over real directories.
///
/// The second derive writes the target's document after the carry, which is
/// what keeps the carried title through a rebuild from the files alone.
#[test]
fn a_derives_fold_carries_a_hand_title_and_is_refused_while_the_two_works_differ() {
    let tree = crate::tests::tempdir("derive-fold-carries-hand-title");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);
    let base: i64 = conn
        .query_row("SELECT id FROM work WHERE id <> ?1", [colour], |r| r.get(0))
        .expect("the base work");
    set_hand_title(&conn, colour, Some("Colour By Hand"));
    crate::hand::save(&conn, colour).expect("saving the colour work's document");
    set_hand_title(&conn, base, Some("Base By Hand"));
    write_the_fold_rule(&conn);

    let refused = derive::derive(&conn).expect("the refusing derive");
    assert_eq!(
        (refused.emptied_works, refused.unfolded_works),
        (0, 1),
        "{refused:?}"
    );
    assert!(
        refused.unfoldable.len() == 1
            && refused.unfoldable[0].contains("`Colour By Hand`")
            && refused.unfoldable[0].contains("`Base By Hand`"),
        "the refusal names both titles: {:?}",
        refused.unfoldable
    );
    let titles: Vec<(i64, String)> = conn
        .prepare("SELECT id, title FROM work ORDER BY id")
        .expect("preparing")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("reading the works")
        .collect::<rusqlite::Result<_>>()
        .expect("reading a work");
    let mut expected = vec![
        (base, "Base By Hand".to_string()),
        (colour, "Colour By Hand".to_string()),
    ];
    expected.sort();
    assert_eq!(
        titles, expected,
        "both works stand, each with its own title"
    );

    set_hand_title(&conn, base, None);
    let folded = derive::derive(&conn).expect("the folding derive");
    assert_eq!(
        (folded.emptied_works, folded.unfolded_works),
        (1, 0),
        "{folded:?}"
    );
    let (title_hand, title): (Option<String>, String) = conn
        .query_row(
            "SELECT title_hand, title FROM work WHERE id = ?1",
            [base],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the target");
    assert_eq!(
        (title_hand.as_deref(), title.as_str()),
        (Some("Colour By Hand"), "Colour By Hand"),
        "the target carries the folded work's title"
    );
    let path = crate::hand::path_for(&conn, base)
        .expect("addressing")
        .expect("a rooted work has an address");
    let written = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the target's document at {}: {e}", path.display()));
    assert_eq!(
        crate::librarydoc::LibraryDoc::from_json(&written)
            .expect("parsing the target's document")
            .title
            .as_deref(),
        Some("Colour By Hand"),
        "the target's own document holds the carried title"
    );
}

/// The colour work holds a saved hand volume and the base work a saved hand
/// title, each in its own document, and the fold rule is written. Returns the
/// base work's id and its document's address.
///
/// **The volume is the folded work's, not the target's.** A hand volume on the
/// base would move it off the group's address, and the derive would then
/// reconcile the group onto the colour work and fold the base away instead.
fn a_volume_to_carry_onto_a_documented_target(
    tree: &crate::tests::TempDir,
    conn: &Connection,
) -> (i64, std::path::PathBuf) {
    let colour = two_works_before_the_fold(tree, conn);
    let base: i64 = conn
        .query_row("SELECT id FROM work WHERE id <> ?1", [colour], |r| r.get(0))
        .expect("the base work");
    let mut doc = crate::librarydoc::of(conn, colour).expect("projecting");
    doc.volume = Some(4.5);
    crate::librarydoc::apply(conn, colour, &doc).expect("applying");
    crate::hand::save(conn, colour).expect("saving the colour work's document");
    set_hand_title(conn, base, Some("Base By Hand"));
    let saved = crate::hand::save(conn, base).expect("saving the base work's document");
    assert!(
        saved.persisted,
        "the base work's own document is on disk: {saved:?}"
    );
    write_the_fold_rule(conn);
    let path = crate::hand::path_for(conn, base)
        .expect("addressing")
        .expect("a rooted work has an address");
    (base, path)
}

/// **A carried value reaches a target document that already held another
/// field.** `hand::reconcile` writes only a missing document and reports one
/// that disagrees with the rows as stale, so without the derive's own save the
/// file would lack the volume and a rebuild from the files would lose it.
#[test]
fn a_carry_onto_a_target_with_its_own_document_rewrites_that_document() {
    let tree = crate::tests::tempdir("derive-fold-carry-rewrites-document");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (_, path) = a_volume_to_carry_onto_a_documented_target(&tree, &conn);

    let report = derive::derive(&conn).expect("the folding derive");

    assert_eq!(
        (report.emptied_works, report.unfolded_works),
        (1, 0),
        "{report:?}"
    );
    let on_disk = crate::librarydoc::LibraryDoc::from_json(
        &std::fs::read_to_string(&path).expect("the target's document"),
    )
    .expect("parsing the target's document");
    assert_eq!(
        (on_disk.title.as_deref(), on_disk.volume),
        (Some("Base By Hand"), Some(4.5)),
        "the target's document holds its own title and the carried volume"
    );
    assert!(
        report.stale_documents.is_empty(),
        "nothing is stale: {:?}",
        report.stale_documents
    );
}

/// **Under `Documents::Skip` the carry writes no file.** The rows carry the
/// volume; every file under the root is byte-identical.
#[test]
fn a_carry_under_documents_skip_writes_no_document() {
    let tree = crate::tests::tempdir("derive-fold-carry-skip-writes-nothing");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (base, _) = a_volume_to_carry_onto_a_documented_target(&tree, &conn);
    let before = crate::tests::tree_snapshot(tree.path());

    let report = derive::derive_with(&conn, derive::Documents::Skip).expect("the folding derive");

    assert_eq!(
        (report.emptied_works, report.unfolded_works),
        (1, 0),
        "{report:?}"
    );
    let volume_hand: Option<f64> = conn
        .query_row("SELECT volume_hand FROM work WHERE id = ?1", [base], |r| {
            r.get(0)
        })
        .expect("the target");
    assert_eq!(volume_hand, Some(4.5), "the rows carry it");
    assert_eq!(
        crate::tests::tree_snapshot(tree.path()),
        before,
        "no file under the root changed"
    );
}

/// **A fold onto a target whose document this pass could not read or apply is
/// refused**, and the file stays as the person left it. The clash check reads
/// rows and cannot see what that document holds, and a fold that carries a
/// value would arm the target, so the fixed file would never be read back.
/// Left standing with its marker unrecorded, the target's fixed file is read
/// on the next scan, and that scan folds. Both of the read-back's failures: a
/// file that does not parse, and one that parses and names an edition the
/// target lacks.
#[test]
fn a_fold_onto_a_target_whose_document_did_not_read_waits_for_the_fixed_file() {
    use rusqlite::OptionalExtension as _;

    let unparseable = |_: &Connection, _: i64| "{ not a library document".to_string();
    let unappliable = |conn: &Connection, base: i64| {
        let mut doc = crate::librarydoc::of(conn, base).expect("projecting");
        doc.editions.insert(
            "nonexistent-edition".to_string(),
            crate::librarydoc::EditionMeta::default(),
        );
        doc.to_json()
    };
    type Document = fn(&Connection, i64) -> String;
    let cases: [(&str, Document); 2] = [
        ("does not parse", unparseable),
        ("does not apply", unappliable),
    ];
    for (case, document) in cases {
        let tree = crate::tests::tempdir("derive-fold-onto-unread-target");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let colour = two_works_before_the_fold(&tree, &conn);
        let base: i64 = conn
            .query_row("SELECT id FROM work WHERE id <> ?1", [colour], |r| r.get(0))
            .expect("the base work");
        set_hand_title(&conn, colour, Some("Colour By Hand"));
        crate::hand::save(&conn, colour).expect("saving the colour work's document");
        let path = crate::hand::path_for(&conn, base)
            .expect("addressing")
            .expect("a rooted work has an address");
        let text = document(&conn, base);
        std::fs::create_dir_all(path.parent().expect("an address has a directory"))
            .expect("the documents directory");
        std::fs::write(&path, &text).expect("the target's document");
        write_the_fold_rule(&conn);

        let refused = derive::derive(&conn).expect("the refusing derive");

        assert_eq!(
            (refused.emptied_works, refused.unfolded_works),
            (0, 1),
            "{case}: {refused:?}"
        );
        let shown = path.display().to_string();
        for named in [
            format!("work {colour}"),
            format!("work {base}"),
            shown.clone(),
            "nothing was moved and nothing was deleted".to_string(),
        ] {
            assert!(
                refused.unfoldable.len() == 1 && refused.unfoldable[0].contains(&named),
                "{case}: the refusal names {named:?}: {:?}",
                refused.unfoldable
            );
        }
        assert!(
            refused
                .unreadable_hand
                .iter()
                .any(|why| why.starts_with(&shown)),
            "{case}: the pass reports it unreadable: {:?}",
            refused.unreadable_hand
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("the target's document"),
            text,
            "{case}: the document is as the person left it"
        );
        let standing: i64 = conn
            .query_row(
                "SELECT count(*) FROM work WHERE id IN (?1, ?2)",
                [colour, base],
                |r| r.get(0),
            )
            .expect("counting the two works");
        assert_eq!(standing, 2, "{case}: both works stand");
        let marker: Option<i64> = conn
            .query_row(
                "SELECT absorbed FROM migrated_document WHERE work_id = ?1 AND document = 'library'",
                [base],
                |r| r.get(0),
            )
            .optional()
            .expect("reading the target's marker");
        assert_eq!(marker, None, "{case}: the target's marker is unrecorded");

        let mut fixed = crate::librarydoc::of(&conn, base).expect("projecting");
        fixed.tags = vec!["Fixed By Hand".to_string()];
        std::fs::write(&path, fixed.to_json()).expect("the fixed document");
        let folded = derive::derive(&conn).expect("the folding derive");

        assert_eq!(
            (folded.emptied_works, folded.unfolded_works),
            (1, 0),
            "{case}: {folded:?}"
        );
        let on_disk = crate::librarydoc::LibraryDoc::from_json(
            &std::fs::read_to_string(&path).expect("the target's document"),
        )
        .expect("parsing the target's document");
        assert_eq!(
            (on_disk.tags, on_disk.title.as_deref()),
            (vec!["Fixed By Hand".to_string()], Some("Colour By Hand")),
            "{case}: the fixed file was read, and the fold then carried the title"
        );
    }
}

/// **The save follows the carry, not the folded work's marker.** A hand title
/// set in the rows alone leaves the colour work unarmed, so `absorb` arms
/// nothing, and the target's document is still written.
#[test]
fn a_carry_from_an_unarmed_work_still_writes_the_targets_document() {
    let tree = crate::tests::tempdir("derive-fold-carry-from-unarmed");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);
    let base: i64 = conn
        .query_row("SELECT id FROM work WHERE id <> ?1", [colour], |r| r.get(0))
        .expect("the base work");
    set_hand_title(&conn, colour, Some("Colour By Hand"));
    write_the_fold_rule(&conn);

    let report = derive::derive(&conn).expect("the folding derive");

    assert_eq!(
        (report.emptied_works, report.unfolded_works),
        (1, 0),
        "{report:?}"
    );
    let path = crate::hand::path_for(&conn, base)
        .expect("addressing")
        .expect("a rooted work has an address");
    let on_disk = crate::librarydoc::LibraryDoc::from_json(
        &std::fs::read_to_string(&path).expect("the target's document"),
    )
    .expect("parsing the target's document");
    assert_eq!(on_disk.title.as_deref(), Some("Colour By Hand"));
}

/// **A fold that carries nothing writes nothing.** The target's own hand
/// title lives in the rows alone; making it durable is not this fold's to do,
/// so its address stays empty.
#[test]
fn a_fold_that_carries_nothing_writes_no_document_for_its_target() {
    let tree = crate::tests::tempdir("derive-fold-carries-nothing");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);
    let base: i64 = conn
        .query_row("SELECT id FROM work WHERE id <> ?1", [colour], |r| r.get(0))
        .expect("the base work");
    set_hand_title(&conn, base, Some("Base By Hand"));
    write_the_fold_rule(&conn);

    let report = derive::derive(&conn).expect("the folding derive");

    assert_eq!(
        (report.emptied_works, report.unfolded_works),
        (1, 0),
        "{report:?}"
    );
    let path = crate::hand::path_for(&conn, base)
        .expect("addressing")
        .expect("a rooted work has an address");
    assert!(!path.exists(), "no document at {}", path.display());
}

/// **`report.repaired_documents += found.repaired` accumulates once per root
/// inside the reconcile loop above.** `-=` starting from `0` would underflow,
/// and `*=` starting from `0` would pin the field at `0` forever, since zero
/// times anything is zero. Only a genuine `+=` gets right two roots each
/// needing their own document repaired on the same pass, which is what this
/// test builds.
///
/// **The gap is opened by deleting the settled `.library.json` file
/// directly, not by rekeying.** `a_rekeyed_digest_takes_its_document_with_it`'s
/// own comment records why a rekey does not open a gap through a real
/// derive at all: `rename_from` carries the document to its new
/// address in the same pass, before `reconcile` ever sees anything missing.
/// Deleting the file after it has settled — the same move
/// `preview::tests::a_preview_leaves_the_library_tree_byte_identical` uses,
/// credited there to `hand/tests.rs`'s `a_gap_is_written_on_the_next_pass` —
/// is what actually reopens the gap `reconcile`'s repair exists to close.
#[test]
fn repaired_documents_accumulates_across_every_root_that_needed_one() {
    let dir = crate::tests::tempdir("derive-repaired-documents-two-roots");
    let root_a_dir = dir.path().join("root-a");
    write_archive(&root_a_dir.join("Alpha").join("Alpha - v01.cbz"), 3);
    let root_b_dir = dir.path().join("root-b");
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

/// **Two orphans in one pass, only one of which came from a fold.** With a
/// single orphan and a single `absorbed_into` entry, a correlation keyed on the
/// orphan's own path (`absorbed_into.get(&orphan.path)`) and one that just
/// grabs *an* entry (`absorbed_into.values().next().cloned()`) read
/// identically: there is only one path and only one value, so either
/// implementation hands the one orphan the one entry. This fixture forces them
/// apart by adding a second orphan that no fold ever touched — a document
/// planted directly under the root's `.taiyaku`, naming no work this library
/// has ever held, never reachable through `superseded` at all (a real rekey
/// does not do it: see `a_rekeyed_digest_takes_its_document_with_it`, where
/// `rename_from` carries the document to its new address before
/// `reconcile` ever sees a gap). Under the keyed lookup the planted orphan's
/// path is not in the map, so it reads `None`; under `.values().next()` it
/// would be handed colour's entry anyway, because a one-entry map hands out the
/// same value regardless of which orphan is asking.
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
    let taiyaku_dir = tree.path().join(".taiyaku");
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
/// not the organisational work.*
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
/// opening it.
#[test]
fn a_repacked_volume_takes_its_document_with_it() {
    let dir = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add");
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

    // **One page added.** `write_archive` is deterministic and each page's
    // bytes name the archive holding them, so rewriting v01 with four pages
    // instead of three changes the member digest, and with it the work's
    // `content_key`. v02 already holds four pages and does not collide,
    // because the label differs — `write_archive`'s own comment says why it
    // is built that way.
    write_archive(
        &dir.path().join("Series").join("v01").join("Series v01.cbz"),
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

/// **A page gained while a database is watching**, which is the transition
/// `rename_from` exists to carry the document across. A baseline that
/// never moves the `content_key` does not reach it.
///
/// **The `orphan_documents` assertion inside the scope block is the one that
/// actually tests `rename_from`; the title assertion at the end does
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
    let v01 = dir.path().join("Series").join("v01").join("Series v01.cbz");

    {
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("add");
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
    let root = scan::add_root(&conn, dir.path()).expect("add");
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
    let root = scan::add_root(&conn, dir.path()).expect("add");
    scan::scan_root(&conn, &root).expect("rescanning");
    derive::derive(&conn).expect("rederiving");
    conn.query_row("SELECT title FROM work ORDER BY id LIMIT 1", [], |row| {
        row.get(0)
    })
    .expect("the work came back")
}

/// A preview derivation must not rename, write or delete anything under
/// `<root>/.taiyaku/`. A rekey's `hand::rename_from` runs whatever the root
/// keeps — a rekey stranding a document is the defect it exists to prevent —
/// so without a documents policy a rule preview would rename files in
/// someone's library and the database rollback would not undo it.
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

/// A plain `derive` reconciles documents, which is the default policy.
///
/// **Location on disk, not the report's own counters, is what has to pin the
/// delegation.** `report.rekeyed` increments before either policy's guard
/// runs, so it is identical whichever policy `derive` actually calls, and
/// `report.orphan_documents` is populated only inside the reconcile loop —
/// it reads empty both when `rename_from` genuinely carried the
/// document across *and* when a `Skip`-delegating `derive` never populated
/// it at all, because nothing else in this module writes to it. Neither
/// counter tells the two apart. The filesystem does: under a correct
/// `derive` the document ends up at the new `content_key` address and the
/// old one is gone; under a `Skip`-delegating `derive` neither the rename
/// nor the reconcile loop's gap repair runs, so the old-key file is still
/// standing and the new-key file was never written.
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
        "rename_from should have carried the document across: {:?}",
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

/// `write_roots` writes `member.root_id` from a [`super::Group`] built earlier
/// in the same derive pass — and `derive_with` holds no transaction of its
/// own, so under WAL a concurrent [`crate::scan::remove_root`] can commit a
/// root's deletion in the window between that group being built and this
/// write running. The `UPDATE` then names a `root_id` the `root` table no
/// longer holds, and the column's own foreign key refuses it, which
/// unhandled would fail the *whole* derive over a race nobody watching the
/// walk's own `report.error` could see coming.
///
/// **Reproduced deterministically here, not through real threads.** A
/// [`super::Group`] naming a `root_id` the `root` table has *already* lost by
/// the time `write_roots` runs is exactly what that race leaves behind at
/// that `UPDATE` — constructing it by hand after deleting the root pins the
/// recovery without a timing-dependent test that could pass or fail
/// depending on how two threads happen to interleave.
#[test]
fn write_roots_survives_a_root_deleted_after_its_own_group_was_built() {
    let dir = crate::tests::tempdir("derive-write-roots-vanished-root");
    write_pages(&dir.path().join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("adding the root");
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
        hub_hint: None,
    };
    group.editions.insert(
        edition_key,
        vec![super::Candidate {
            path: dir
                .path()
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
            copies: Vec::new(),
            frozen: false,
            held_by_rule: false,
            joined_by: None,
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
/// gives for "propagates unchanged" — could not be staged deterministically, so
/// this stands in for them: some other, non-FK error on that exact statement.
#[test]
fn write_roots_propagates_a_non_foreign_key_error_rather_than_treating_it_as_a_vanished_root() {
    let dir = crate::tests::tempdir("derive-write-roots-non-fk-error");
    write_pages(&dir.path().join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("adding the root");
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
        hub_hint: None,
    };
    group.editions.insert(
        edition_key,
        vec![super::Candidate {
            path: dir
                .path()
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
            copies: Vec::new(),
            frozen: false,
            held_by_rule: false,
            joined_by: None,
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
        let root = scan::add_root(&conn, tree.path()).expect("add");
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

/// **O114: a correction that replaces the scanned title.** `reconcile_series`
/// finds a series by `sort_title`, so a correction that moved
/// `series.sort_title` with it would leave the next scan finding nothing
/// under the scanned name, and it would mint a second series and move every
/// volume onto it.
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
/// title and making it the primary one keeps the series just the same.
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

/// **O114, the scanned text kept as the primary title.** Pinned beside the
/// other two because the title that derivation writes on every pass must land
/// on the row already holding that text rather than beside it.
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
/// series by any title text the two share, and is reached by
/// `PATCH /api/library/works/{slug}` and by `derive` reading a hand document
/// back. Its matched branch must not write the document's primary title into
/// `sort_title`: a document naming the scanned text beside a native primary
/// must keep the series.
#[test]
fn a_rescan_after_a_document_names_another_primary_title_keeps_the_series() {
    use crate::librarydoc::TitleKind;
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
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

/// **A spread flag a fold writes onto its twin survives the next derive.** The
/// flag is what every slot on that edition is built from, so a rescan that
/// dropped it would move every place and note the fold had just placed.
/// `import::replace_editions` carries a held page flag across a scan by
/// edition key, and this is the fold's flag reaching it.
#[test]
fn a_spread_flag_a_fold_writes_survives_the_next_derive() {
    let tree = crate::tests::tempdir("derive-fold-spread");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'spread', 3 FROM edition WHERE work_id = ?1",
        [colour],
    )
    .expect("the colour edition's spread, as `taiyaku align` would have measured it");
    write_the_fold_rule(&conn);

    let spreads = |conn: &Connection| -> Vec<i64> {
        conn.prepare(
            "SELECT page_flag.page FROM page_flag
               JOIN edition ON edition.id = page_flag.edition_id
              WHERE edition.key = 'full-color' AND page_flag.kind = 'spread'",
        )
        .and_then(|mut s| {
            s.query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()
        })
        .expect("the full-color edition's spread flags")
    };
    let folding = derive::derive(&conn).expect("the derive that folds");
    assert_eq!(
        (folding.emptied_works, spreads(&conn)),
        (1, vec![3]),
        "folded, flag written"
    );
    derive::derive(&conn).expect("the next derive");
    assert_eq!(spreads(&conn), vec![3], "and still there after a rescan");
}

/// O89 end to end: two releases of one volume, told apart only by their
/// directories, derive as two works of two series rather than one work of
/// two members.
#[test]
fn two_releases_told_apart_only_by_their_directories_are_two_works() {
    let tree = crate::tests::tempdir("derive-o89-two-releases");
    write_archive(
        &tree
            .path()
            .join("[Oshi no Ko]")
            .join("[Oshi no Ko] - v01.cbz"),
        3,
    );
    write_archive(
        &tree
            .path()
            .join("[Oshi no Ko] - Fan Translated")
            .join("[Oshi no Ko] - v01.cbz"),
        5,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(
        report.merged, 0,
        "neither release joined the other's page sequence"
    );
    assert_eq!(report.works, 2);
    assert_eq!(report.members, 2);

    let mut stmt = conn
        .prepare("SELECT sort_title FROM series ORDER BY sort_title")
        .expect("prepare");
    let titles: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(titles, ["[Oshi no Ko]", "[Oshi no Ko] - Fan Translated"]);
}

// --- One derive across libraries (overlapping-libraries spec §3.2) ----------

/// The one integer `sql` answers with: a count, or an id where the fixture
/// holds one row.
pub(super) fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect(sql)
}

/// A directory of its own library for `dir`, with its edition key set to
/// `edition` where one is given, scanned.
pub(super) fn scanned_root(
    conn: &Connection,
    dir: &std::path::Path,
    edition: Option<&str>,
) -> scan::Root {
    let root = scan::add_root(conn, dir).expect("a library of its own");
    if let Some(edition) = edition {
        scan::update_root(
            conn,
            root.id,
            &scan::RootPatch {
                edition_key: Some(Some(edition.to_string())),
                ..Default::default()
            },
        )
        .expect("sets the directory's edition");
    }
    let root = scan::roots(conn)
        .expect("lists")
        .into_iter()
        .find(|r| r.id == root.id)
        .expect("the directory just added");
    scan::scan_root(conn, &root).expect("scans");
    root
}

/// Spec decision 4: a volume under one library's directory and its chapter
/// release under another's, each directory setting its own edition key, are
/// one work with an edition from each, in one series.
#[test]
fn a_volume_and_its_chapter_release_in_two_libraries_are_one_work_of_two_editions() {
    let volumes = crate::tests::tempdir("derive-across-volume");
    write_pages(&volumes.path().join("Series").join("Series - v01"), 3);
    let chapters = crate::tests::tempdir("derive-across-chapters");
    for chapter in ["c001", "c002"] {
        write_pages(
            &chapters
                .path()
                .join("Series")
                .join(format!("Series - v01 - {chapter}")),
            2,
        );
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let a = scanned_root(&conn, volumes.path(), None);
    let b = scanned_root(&conn, chapters.path(), Some("en"));
    assert_ne!(a.libraries[0], b.libraries[0], "the fixture: two libraries");

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(report.works, 1, "written once, from one group");
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 1, "one work");
    assert_eq!(
        edition_keys(&conn),
        vec!["en", "raw"],
        "with an edition from each library's directory"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM member m JOIN edition e ON e.id = m.edition_id WHERE e.key = 'en'"
        ),
        2,
        "the chapter release's two files in its edition"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM series"),
        1,
        "one series"
    );
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(
        (again.works, again.rekeyed, again.kept_editions),
        (1, 0, 0),
        "and a second pass finds the work as the first left it"
    );
}

/// The same title in two libraries is one series, and where each directory
/// sets its own edition key, one volume 1 with an edition from each.
#[test]
fn the_same_title_in_two_libraries_is_one_series_and_one_work() {
    let ecchi = crate::tests::tempdir("derive-lib-ecchi");
    write_pages(&ecchi.path().join("Series").join("Series - v01"), 2);
    let volumes = crate::tests::tempdir("derive-lib-volumes");
    write_pages(&volumes.path().join("Series").join("Series - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, ecchi.path(), None);
    scanned_root(&conn, volumes.path(), Some("en"));

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(report.works, 1, "written once, from one group");

    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM series WHERE sort_title = 'Series'"
        ),
        1,
        "one series of that title"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        1,
        "one volume 1"
    );
    assert_eq!(edition_keys(&conn), vec!["en", "raw"]);
}

/// `Foo` in two libraries is one series, holding both libraries' volumes, and
/// nothing proposes it as the same series as itself.
#[test]
fn foo_in_two_libraries_is_one_series_and_raises_no_proposal() {
    let one = crate::tests::tempdir("derive-foo-one");
    write_pages(&one.path().join("Foo").join("Foo - v01"), 2);
    let two = crate::tests::tempdir("derive-foo-two");
    write_pages(&two.path().join("Foo").join("Foo - v02"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, one.path(), None);
    scanned_root(&conn, two.path(), None);

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(report.series, 1);
    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM series WHERE sort_title = 'Foo'"
        ),
        1,
        "one `Foo`"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(DISTINCT series_id) FROM work"),
        1,
        "holding both volumes"
    );
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 2);
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM proposal"),
        0,
        "and no proposal of any kind"
    );
}

/// Where two series already share a `sort_title`, the derive files both
/// libraries' volumes under the older and leaves the younger as it was.
#[test]
fn a_title_two_series_share_is_reconciled_onto_the_older() {
    let one = crate::tests::tempdir("derive-older-series-one");
    write_pages(&one.path().join("Foo").join("Foo - v01"), 2);
    let two = crate::tests::tempdir("derive-older-series-two");
    write_pages(&two.path().join("Foo").join("Foo - v02"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'Foo'), (2, 'Foo');
         INSERT INTO series_title (series_id, title, is_primary)
         VALUES (1, 'Foo', 1), (2, 'Foo', 1);",
    )
    .expect("two series of one title");
    scanned_root(&conn, one.path(), None);
    scanned_root(&conn, two.path(), None);

    derive::derive(&conn).expect("derives");

    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work WHERE series_id = 1"),
        2,
        "both volumes in the older series"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM series"),
        2,
        "nothing minted"
    );
}

#[test]
fn two_dirs_of_one_library_still_make_one_work_of_a_volume_and_its_other_release() {
    let volumes = crate::tests::tempdir("derive-lib-one-volumes");
    write_pages(&volumes.path().join("Series").join("Series - v01"), 2);
    let chapters = crate::tests::tempdir("derive-lib-one-chapters");
    write_pages(&chapters.path().join("Series").join("Series - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let v =
        scan::add_root_in(&conn, volumes.path(), scan::InLibrary::Named("Manga")).expect("adds");
    let c =
        scan::add_root_in(&conn, chapters.path(), scan::InLibrary::Named("Manga")).expect("adds");
    scan::update_root(
        &conn,
        c.id,
        &scan::RootPatch {
            edition_key: Some(Some("en".to_string())),
            ..Default::default()
        },
    )
    .expect("the second directory is another edition");
    scan::scan_root(&conn, &v).expect("scans");
    scan::scan_root(&conn, &scan::roots(&conn).expect("lists")[1]).expect("scans");

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        1,
        "one work across both directories"
    );
    assert_eq!(
        edition_keys(&conn),
        vec!["en", "raw"],
        "with each directory's edition"
    );
    assert_eq!(report.works, 1, "written once, from one group");
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(
        (again.rekeyed, again.kept_editions),
        (0, 0),
        "and a second pass finds the work as the first left it"
    );
}

/// Every row a relabel must leave alone, as text: each series, each work's
/// id, slug, series and key, each edition, and each position and note.
fn identities(conn: &Connection) -> Vec<String> {
    let rows = |sql: &str| -> Vec<String> {
        conn.prepare(sql)
            .expect(sql)
            .query_map([], |r| r.get::<_, String>(0))
            .expect(sql)
            .collect::<rusqlite::Result<_>>()
            .expect(sql)
    };
    [
        "SELECT 'series ' || id || ' ' || sort_title FROM series ORDER BY id",
        "SELECT 'work ' || id || ' ' || slug || ' ' || series_id || ' ' || content_key
           FROM work ORDER BY id",
        "SELECT 'edition ' || work_id || ' ' || key FROM edition ORDER BY work_id, key",
        "SELECT 'progress ' || work_id || ' ' || edition || ' ' || slot FROM progress
          ORDER BY work_id",
        "SELECT 'note ' || work_id || ' ' || id FROM note ORDER BY work_id",
    ]
    .iter()
    .flat_map(|sql| rows(sql))
    .collect()
}

/// Spec §3.5: Move relabels a directory's library, and the derive after it
/// changes no work, slug, series, edition or position. One directory moves out
/// of a library that keeps another, and one out of a library it was alone in,
/// which goes.
#[test]
fn relabelling_a_directorys_library_changes_no_work_series_edition_or_position() {
    let kept = crate::tests::tempdir("derive-relabel-kept");
    write_pages(&kept.path().join("Kept").join("Kept - v01"), 2);
    let moved = crate::tests::tempdir("derive-relabel-moved");
    write_pages(&moved.path().join("Moved").join("Moved - v01"), 3);
    let alone = crate::tests::tempdir("derive-relabel-alone");
    write_pages(&alone.path().join("Alone").join("Alone - v01"), 4);
    let elsewhere = crate::tests::tempdir("derive-relabel-elsewhere");
    write_pages(&elsewhere.path().join("Moved").join("Moved - v02"), 5);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let k = scan::add_root_in(&conn, kept.path(), scan::InLibrary::Named("A")).expect("adds");
    let m = scan::add_root_in(&conn, moved.path(), scan::InLibrary::Named("A")).expect("adds");
    let l = scan::add_root(&conn, alone.path()).expect("adds");
    let e = scan::add_root_in(&conn, elsewhere.path(), scan::InLibrary::Named("B")).expect("adds");
    for root in [&k, &m, &l, &e] {
        scan::scan_root(&conn, root).expect("scans");
    }
    derive::derive(&conn).expect("derives");
    for (title, slot) in [("Moved - v01", 2), ("Alone - v01", 3)] {
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot)
             SELECT 1, id, 'raw', ?2 FROM work WHERE title = ?1",
            rusqlite::params![title, slot],
        )
        .expect("a position on each directory about to move");
    }
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         SELECT 1, 'n', id, 'raw', 1, 'kept', 'now', 'now' FROM work WHERE title = 'Moved - v01'",
        [],
    )
    .expect("a note");
    let before = identities(&conn);
    assert_eq!(
        before.iter().filter(|r| r.starts_with("progress ")).count(),
        2,
        "the fixture: both positions written"
    );

    crate::relocate::move_root(
        &conn,
        m.id,
        m.libraries[0],
        crate::relocate::Target::Library(e.libraries[0]),
    )
    .expect("moves a directory out of a library that keeps another");
    let gone = crate::relocate::move_root(
        &conn,
        l.id,
        l.libraries[0],
        crate::relocate::Target::Library(e.libraries[0]),
    )
    .expect("moves a directory out of a library it was alone in");
    assert!(
        gone.library_removed.is_some(),
        "the fixture: that library went"
    );
    let report = derive::derive(&conn).expect("re-derives");

    assert_eq!(
        identities(&conn),
        before,
        "every series, work, slug, edition, position and note as it was"
    );
    assert_eq!(
        (report.rekeyed, report.emptied_works, report.kept_editions),
        (0, 0, 0),
        "nothing rekeyed, folded or left behind"
    );
}

/// Several series hold the title `Series`: the one the files named, and a
/// younger one a correction gave it as an alternative. A hand document naming
/// it finds the oldest in the store, whichever library its work is in.
#[test]
fn a_hand_document_finds_the_oldest_series_of_its_title_across_libraries() {
    let first = crate::tests::tempdir("derive-lib-doc-a");
    write_pages(&first.path().join("Series").join("Series - v01"), 2);
    let second = crate::tests::tempdir("derive-lib-doc-b");
    write_pages(&second.path().join("Series").join("Series - v02"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, first.path(), None);
    scanned_root(&conn, second.path(), None);
    derive::derive(&conn).expect("derives");
    let (work, oldest): (i64, i64) = conn
        .query_row(
            "SELECT id, series_id FROM work WHERE volume = 2.0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the second library's volume");
    conn.execute_batch(
        "INSERT INTO series (sort_title) VALUES ('Shirizu');
         INSERT INTO series_title (series_id, title, is_primary)
         VALUES (last_insert_rowid(), 'Series', 0);",
    )
    .expect("a younger series holding the title too");
    let doc = crate::librarydoc::LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-10-05T00:00:00Z",
            "series":{"chosen":true,"titles":[{"title":"Series","kind":"romaji","primary":true}]}}"#,
    )
    .expect("parses");

    crate::librarydoc::apply(&conn, work, &doc).expect("applies");

    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT series_id FROM work WHERE id = {work}")
        ),
        oldest,
        "the oldest series of that title, which both libraries' volumes are in"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(DISTINCT series_id) FROM work"),
        1
    );
}

/// A series a hand document names for the first time is minted once, and a
/// document naming the same title on a work under another library's
/// directory, or on a work no directory holds, finds it.
#[test]
fn a_hand_document_naming_a_new_series_mints_one_the_next_document_finds() {
    let dir = crate::tests::tempdir("derive-lib-doc-mint");
    write_pages(&dir.path().join("Series").join("Series - v01"), 2);
    let elsewhere = crate::tests::tempdir("derive-lib-doc-mint-elsewhere");
    write_pages(&elsewhere.path().join("Other").join("Other - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, dir.path(), None);
    let other = scanned_root(&conn, elsewhere.path(), None);
    assert_ne!(
        root.libraries[0], other.libraries[0],
        "the fixture: two libraries"
    );
    derive::derive(&conn).expect("derives");
    let scanned = work_under(&conn, &root).0;
    let in_other = work_under(&conn, &other).0;
    let imported = crate::import::sidecar(
        &conn,
        &taiyaku_core::Sidecar::from_json(
            r#"{"taiyaku":1,"work":{"title":"Imported","reading":"rtl"},
                "editions":{"raw":{"label":"raw","members":[
                  {"path":"raw/imported.cbz","sha256":"aa","pages":2}]}},
                "hub":"raw","alignments":[]}"#,
        )
        .expect("parses"),
        std::path::Path::new("/x/taiyaku.json"),
    )
    .expect("imports a work no directory holds");
    let doc = crate::librarydoc::LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-10-05T00:00:00Z",
            "series":{"chosen":true,
                      "titles":[{"title":"Named By Hand","kind":"romaji","primary":true}]}}"#,
    )
    .expect("parses");

    crate::librarydoc::apply(&conn, scanned, &doc).expect("applies");
    crate::librarydoc::apply(&conn, in_other, &doc).expect("applies");
    crate::librarydoc::apply(&conn, imported, &doc).expect("applies");

    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM series WHERE sort_title = 'Named By Hand'"
        ),
        1,
        "minted once"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM work w JOIN series s ON s.id = w.series_id
              WHERE s.sort_title = 'Named By Hand'"
        ),
        3,
        "and all three works are in it"
    );
}

/// Three series of one title, made by hand: a hand document on any work,
/// whichever library holds it or none, finds the oldest and mints nothing.
#[test]
fn a_hand_document_resolves_one_title_to_the_oldest_series_in_the_store() {
    let dir = crate::tests::tempdir("derive-lib-doc-three");
    write_pages(&dir.path().join("Else").join("Else - v01"), 2);
    let elsewhere = crate::tests::tempdir("derive-lib-doc-three-elsewhere");
    write_pages(&elsewhere.path().join("Other").join("Other - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO series (id, sort_title)
         VALUES (1, 'Series'), (2, 'Series Two'), (3, 'Series Three');
         INSERT INTO series_title (series_id, title, is_primary)
         VALUES (1, 'Series', 1), (2, 'Series', 0), (3, 'Series', 0);",
    )
    .expect("three series of one title");
    let root = scanned_root(&conn, dir.path(), None);
    let other = scanned_root(&conn, elsewhere.path(), None);
    assert_ne!(
        root.libraries[0], other.libraries[0],
        "the fixture: two libraries"
    );
    derive::derive(&conn).expect("derives");
    let scanned = work_under(&conn, &root).0;
    let in_other = work_under(&conn, &other).0;
    let imported = crate::import::scanned(
        &conn,
        &taiyaku_core::Sidecar::from_json(
            r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
                "editions":{"raw":{"label":"raw","members":[
                  {"path":"raw/v01.cbz","sha256":"bb","pages":2}]}},
                "hub":"raw","alignments":[]}"#,
        )
        .expect("parses"),
        None,
    )
    .expect("a work no directory holds");
    let doc = crate::librarydoc::LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-10-05T00:00:00Z",
            "series":{"chosen":true,"titles":[{"title":"Series","kind":"romaji","primary":true}]}}"#,
    )
    .expect("parses");

    crate::librarydoc::apply(&conn, scanned, &doc).expect("applies");
    crate::librarydoc::apply(&conn, in_other, &doc).expect("applies");
    crate::librarydoc::apply(&conn, imported, &doc).expect("applies");

    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM work
                  WHERE id IN ({scanned}, {in_other}, {imported}) AND series_id = 1"
            )
        ),
        3,
        "all three find the oldest"
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM series WHERE sort_title LIKE 'Series%'"
        ),
        3,
        "and nothing was minted"
    );
}

/// A work no directory holds can sit at a scanned work's address: an
/// imported `taiyaku.json` a person placed in a scanned series, at a volume
/// the directory also holds. When the volume changes on disk, the scanned work
/// follows the files; the older imported work is not taken for it.
#[test]
fn a_work_follows_its_files_before_an_older_one_no_directory_holds_at_its_address() {
    a_held_work_follows_its_files("derive-held-first", "Series - v01", Some(1.0));
}

/// The same for a work with no volume, whose address is its files' own path:
/// the imported work names the path the directory's one-shot is at.
#[test]
fn a_volume_less_work_follows_its_files_before_an_older_one_no_directory_holds() {
    a_held_work_follows_its_files("derive-held-first-oneshot", "Some Oneshot", None);
}

/// One member, `Series/<name>`, scanned after another work was imported. The
/// imported work, whose members are under no directory, is placed at the
/// scanned one's address, `volume` and member path both. Then the member's
/// files change.
fn a_held_work_follows_its_files(label: &str, name: &str, volume: Option<f64>) {
    let dir = crate::tests::tempdir(label);
    let volume_dir = dir.path().join("Series").join(name);
    write_pages(&volume_dir, 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let imported_doc = taiyaku_core::Sidecar::from_json(
        r#"{"taiyaku":1,"work":{"title":"Imported","reading":"rtl"},
            "editions":{"raw":{"label":"raw","members":[
              {"path":"raw/imported.cbz","sha256":"aa","pages":2}]}},
            "hub":"raw","alignments":[]}"#,
    )
    .expect("parses");
    let imported = crate::import::sidecar(
        &conn,
        &imported_doc,
        std::path::Path::new("/x/taiyaku.json"),
    )
    .expect("imports, before anything is scanned, so it is the older row");
    let root = scanned_root(&conn, dir.path(), None);
    derive::derive(&conn).expect("derives");
    let (scanned, series, key_before, derived_volume): (i64, i64, String, Option<f64>) = conn
        .query_row(
            "SELECT id, series_id, content_key, volume FROM work WHERE id != ?1",
            [imported],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .expect("the scanned work");
    assert!(
        imported < scanned,
        "the fixture: the imported work is older"
    );
    assert_eq!(
        derived_volume, volume,
        "the fixture: the scanned work is at the address this case is about"
    );
    conn.execute(
        "UPDATE work SET series_id = ?1, series_id_hand = ?1, volume = ?2, volume_hand = ?2
          WHERE id = ?3",
        rusqlite::params![series, volume, imported],
    )
    .expect("a person places the imported work at the scanned one's address");
    conn.execute(
        "UPDATE member
            SET path = (SELECT m.path FROM member m JOIN edition e ON e.id = m.edition_id
                         WHERE e.work_id = ?1)
          WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?2)",
        [scanned, imported],
    )
    .expect("and it names the scanned member's own path, which is a volume-less address");
    let imported_key = |conn: &Connection| -> String {
        conn.query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [imported],
            |r| r.get(0),
        )
        .expect("reads")
    };
    let imported_before = imported_key(&conn);

    std::fs::write(volume_dir.join("002.jpg"), b"one more page").expect("write");
    // A directory member is re-hashed only when its `mtime` moves; see
    // `a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress`.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    std::fs::File::open(&volume_dir)
        .expect("open the member directory")
        .set_times(std::fs::FileTimes::new().set_modified(later))
        .expect("stamp it as changed");
    scan::scan_root(&conn, &root).expect("rescans");
    let report = derive::derive(&conn).expect("re-derives");

    let key_after: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [scanned],
            |r| r.get(0),
        )
        .expect("the scanned work is still there");
    assert_ne!(key_after, key_before, "the scanned work followed its files");
    assert_eq!(report.rekeyed, 1);
    assert_eq!(
        imported_key(&conn),
        imported_before,
        "and the imported work is as it was"
    );
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 2);
}

/// One work gathered editions from two directories of one library, and one
/// directory is then moved to another library. The work stays whole: the same
/// row with both editions, and the position on the moved directory's edition
/// stays on it.
#[test]
fn a_work_spanning_two_libraries_stays_whole() {
    let hub = crate::tests::tempdir("derive-lib-whole-hub");
    write_pages(&hub.path().join("Series").join("Series - v01"), 2);
    let other = crate::tests::tempdir("derive-lib-whole-other");
    write_pages(&other.path().join("Series").join("Series - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let h = scan::add_root_in(&conn, hub.path(), scan::InLibrary::Named("Manga")).expect("adds");
    let o = scan::add_root_in(&conn, other.path(), scan::InLibrary::Named("Manga")).expect("adds");
    scan::update_root(
        &conn,
        h.id,
        &scan::RootPatch {
            edition_key: Some(Some("en".to_string())),
            ..Default::default()
        },
    )
    .expect("the hub directory's edition sorts first, so it is the hub");
    for root in scan::roots(&conn).expect("lists") {
        scan::scan_root(&conn, &root).expect("scans");
    }
    derive::derive(&conn).expect("derives");
    let whole = scalar(&conn, "SELECT id FROM work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [whole],
    )
    .expect("a position on the edition whose directory is about to move");

    crate::relocate::move_root(
        &conn,
        o.id,
        o.libraries[0],
        crate::relocate::Target::New("Other"),
    )
    .expect("moves the other directory to a library of its own");
    let report = derive::derive(&conn).expect("re-derives");

    let editions: Vec<(i64, String)> = conn
        .prepare("SELECT work_id, key FROM edition ORDER BY work_id, key")
        .expect("prepares")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert_eq!(
        editions,
        vec![(whole, "en".to_string()), (whole, "raw".to_string())],
        "one work, holding both directories' editions"
    );
    assert_eq!(
        (report.works, report.kept_editions, report.rekeyed),
        (1, 0, 0),
        "written from one group, with nothing left behind or rekeyed"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM progress WHERE work_id = {whole} AND edition = 'raw'")
        ),
        1,
        "and the position is still on it"
    );
}

/// The work holding a member under `root`, as `(id, slug, series_id,
/// content_key)`.
fn work_under(conn: &Connection, root: &scan::Root) -> (i64, String, i64, String) {
    conn.query_row(
        "SELECT w.id, w.slug, w.series_id, w.content_key FROM work w
           JOIN edition e ON e.work_id = w.id JOIN member m ON m.edition_id = e.id
          WHERE m.root_id = ?1",
        [root.id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .expect("the root's one work")
}

/// A work imported from a `taiyaku.json` has no member under any root, so
/// the derive whose files match its key takes it.
#[test]
fn an_imported_work_is_taken_by_the_derive_whose_files_match_it() {
    let dir = crate::tests::tempdir("derive-lib-imported");
    let volume = dir.path().join("Series").join("Series - v01");
    write_pages(&volume, 2);
    let digest = taiyaku_read::member_digest(&volume).expect("hashes the volume");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let doc = taiyaku_core::Sidecar::from_json(&format!(
        r#"{{"taiyaku":1,"work":{{"title":"Imported","reading":"rtl"}},
            "editions":{{"raw":{{"label":"raw","members":[
              {{"path":"Series - v01","sha256":"{digest}","pages":2}}]}}}},
            "hub":"raw","alignments":[]}}"#
    ))
    .expect("parses");
    let imported = crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
        .expect("imports");
    let root = scanned_root(&conn, dir.path(), None);

    derive::derive(&conn).expect("derives");

    assert_eq!(
        work_under(&conn, &root).0,
        imported,
        "the same row, its member now under the directory"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        1,
        "and no second work"
    );
}

/// `Zeta/<held>` in one library, and `Series/Series - v02` in another. The
/// held work is placed by hand in `Series` at `volume`, naming the path
/// `Series/<gained>` will have. Then `Series/<gained>` appears in the other
/// library and everything is derived.
///
/// The gained files are at the held work's address, and a row some directory
/// holds is that address's work: they land on it, as they would were both
/// directories in one library, and no second work is minted at the address.
fn a_work_placed_at_an_address_takes_the_files_that_appear_there(
    label: &str,
    held: &str,
    gained: &str,
    volume: Option<f64>,
) {
    let held_dir = crate::tests::tempdir(&format!("{label}-held"));
    write_pages(&held_dir.path().join("Zeta").join(held), 3);
    let own = crate::tests::tempdir(&format!("{label}-own"));
    write_pages(&own.path().join("Series").join("Series - v02"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let h = scanned_root(&conn, held_dir.path(), None);
    let o = scanned_root(&conn, own.path(), None);
    derive::derive(&conn).expect("derives");
    let before = work_under(&conn, &h);
    let own_series = work_under(&conn, &o).2;
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [before.0],
    )
    .expect("a position");
    let gained_path = o
        .path
        .join("Series")
        .join(gained)
        .to_string_lossy()
        .into_owned();
    conn.execute(
        "UPDATE work SET series_id = ?1, series_id_hand = ?1, volume = ?2, volume_hand = ?2
          WHERE id = ?3",
        rusqlite::params![own_series, volume, before.0],
    )
    .expect("a person places the held work in the other library's series");
    conn.execute(
        "UPDATE member SET path = ?1
          WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?2)",
        rusqlite::params![gained_path, before.0],
    )
    .expect("and it names the path the gained member will have");
    write_pages(&own.path().join("Series").join(gained), 2);
    scan::scan_root(&conn, &o).expect("rescans");

    let report = derive::derive(&conn).expect("re-derives");

    let gained_works: Vec<i64> = conn
        .prepare(
            "SELECT e.work_id FROM edition e JOIN member m ON m.edition_id = e.id
              WHERE m.path = ?1 ORDER BY e.work_id",
        )
        .expect("prepares")
        .query_map([&gained_path], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert_eq!(
        gained_works,
        vec![before.0],
        "the gained member has one work: the one at that address"
    );
    assert_eq!(report.rekeyed, 1, "which was rekeyed to the gained files");
    assert_ne!(
        work_under(&conn, &h).0,
        before.0,
        "and the held directory's own files mint a work of their own"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM work WHERE series_id = {own_series} AND volume IS {}",
                volume.map_or("NULL".to_string(), |v| v.to_string())
            )
        ),
        1,
        "and no second work there"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM progress WHERE work_id = {} AND slot = 2",
                before.0
            )
        ),
        1,
        "the position stays on the row, which now holds the gained files"
    );
}

#[test]
fn a_work_placed_at_an_address_takes_the_volume_that_appears_there() {
    a_work_placed_at_an_address_takes_the_files_that_appear_there(
        "derive-placed-address",
        "Zeta - v01",
        "Series - v01",
        Some(1.0),
    );
}

/// The same for a work with no volume, whose address is a member's path.
#[test]
fn a_volume_less_work_placed_at_a_path_takes_the_files_that_appear_there() {
    a_work_placed_at_an_address_takes_the_files_that_appear_there(
        "derive-placed-path",
        "Zeta Oneshot",
        "Some Oneshot",
        None,
    );
}

/// `Zeta/Zeta - v01` in one library and `Series/Series - v02` in another,
/// each directory its own edition. A person places Zeta's work in `Series`
/// by hand (`hand`), a position on it, and then `Series/Series - v01`
/// appears. The placed work names none of the files that appeared, and
/// Zeta's own files find it by content, so those files take a work of their
/// own beside it: each file is on one work in `Series` at volume 1, the
/// position stays with Zeta's, and the next derive commits nothing.
fn a_work_placed_from_another_series_leaves_the_files_that_appear_at_its_address(
    label: &str,
    hand: &str,
) {
    let held_dir = crate::tests::tempdir(&format!("{label}-held"));
    write_pages(&held_dir.path().join("Zeta").join("Zeta - v01"), 3);
    let own = crate::tests::tempdir(&format!("{label}-own"));
    write_pages(&own.path().join("Series").join("Series - v02"), 2);
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    let h = scanned_root(&conn, held_dir.path(), Some("ja"));
    let o = scanned_root(&conn, own.path(), Some("en"));
    derive::derive(&conn).expect("derives");
    let placed = work_under(&conn, &h).0;
    let series = work_under(&conn, &o).2;
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'ja', 2)",
        [placed],
    )
    .expect("a position");
    conn.execute(
        &format!("UPDATE work SET series_id = ?1, {hand} WHERE id = ?2"),
        [series, placed],
    )
    .expect("a person places Zeta's work in Series");
    derive::derive(&conn).expect("derives the placed work");
    write_pages(&own.path().join("Series").join("Series - v01"), 2);
    scan::scan_root(&conn, &o).expect("rescans");

    derive::derive(&conn).expect("re-derives");

    let appeared = holders_of(&conn, "Series - v01");
    assert_eq!(
        (holders_of(&conn, "Zeta - v01"), appeared.len()),
        (vec![placed], 1),
        "each file is on one work, Zeta's on the placed work"
    );
    assert_ne!(
        appeared[0], placed,
        "the files that appeared are on a work of their own"
    );
    let stands = |work: i64| -> i64 {
        scalar(
            &conn,
            &format!("SELECT series_id = {series} AND volume = 1 FROM work WHERE id = {work}"),
        )
    };
    assert_eq!(
        (stands(placed), stands(appeared[0])),
        (1, 1),
        "both stand in Series at volume 1"
    );
    assert_eq!(
        scalar(&conn, "SELECT work_id FROM progress WHERE slot = 2"),
        placed,
        "the position stays with Zeta's files"
    );
    let (commits, _) = commits_during(&mut conn, |conn| derive::derive(conn).expect("derives"));
    assert_eq!(commits, 0, "the next derive commits nothing");
}

/// Where the placed work names the files that appear, it takes them
/// (`a_work_placed_at_an_address_takes_the_volume_that_appears_there`).
#[test]
fn a_volume_appearing_where_a_work_was_placed_from_another_series_takes_a_work_of_its_own() {
    a_work_placed_from_another_series_leaves_the_files_that_appear_at_its_address(
        "derive-placed-series",
        "series_id_hand = ?1",
    );
}

/// The same with the placed work's volume set by hand too.
#[test]
fn a_volume_appearing_where_a_work_was_placed_with_its_volume_takes_a_work_of_its_own() {
    a_work_placed_from_another_series_leaves_the_files_that_appear_at_its_address(
        "derive-placed-series-volume",
        "series_id_hand = ?1, volume_hand = 1",
    );
}

/// Two works no directory holds can sit at one address: two imported
/// `taiyaku.json`s a person placed at one series and volume. When files
/// appear there, they go to the older and leave the other.
#[test]
fn the_older_of_two_works_at_one_address_takes_the_files() {
    the_older_of_two_works_at_one_address_is_taken(
        "derive-older-address",
        "Series - v01",
        Some(1.0),
    );
}

/// The same for works with no volume, whose address is a member's path.
#[test]
fn the_older_of_two_volume_less_works_at_one_path_takes_the_files() {
    the_older_of_two_works_at_one_address_is_taken("derive-older-path", "Some Oneshot", None);
}

/// `Series/Series - v02` in a library, and two imported works placed by hand
/// in `Series` at `volume`, each naming the path `Series/<gained>` will have.
/// Then `Series/<gained>` appears. Neither imported work holds its content,
/// so the group finds a row by its address alone.
fn the_older_of_two_works_at_one_address_is_taken(label: &str, gained: &str, volume: Option<f64>) {
    let dir = crate::tests::tempdir(label);
    write_pages(&dir.path().join("Series").join("Series - v02"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, dir.path(), None);
    derive::derive(&conn).expect("derives");
    let series = scalar(&conn, "SELECT series_id FROM work");
    let gained_path = root
        .path
        .join("Series")
        .join(gained)
        .to_string_lossy()
        .into_owned();
    let place = |sha: &str, document: &str| -> i64 {
        let work = crate::import::sidecar(
            &conn,
            &taiyaku_core::Sidecar::from_json(&format!(
                r#"{{"taiyaku":1,"work":{{"title":"Imported","reading":"rtl"}},
                    "editions":{{"raw":{{"label":"raw","members":[
                      {{"path":"raw/imported.cbz","sha256":"{sha}","pages":2}}]}}}},
                    "hub":"raw","alignments":[]}}"#
            ))
            .expect("parses"),
            std::path::Path::new(document),
        )
        .expect("imports a work no directory holds");
        conn.execute(
            "UPDATE work SET series_id = ?1, series_id_hand = ?1, volume = ?2, volume_hand = ?2
              WHERE id = ?3",
            rusqlite::params![series, volume, work],
        )
        .expect("a person places it at the address");
        conn.execute(
            "UPDATE member SET path = ?1
              WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?2)",
            rusqlite::params![gained_path, work],
        )
        .expect("and it names the path the gained member will have");
        work
    };
    let older = place("aa", "/x/taiyaku.json");
    let younger = place("bb", "/y/taiyaku.json");
    assert!(older < younger, "the fixture");
    let key = |work: i64| -> String {
        conn.query_row("SELECT content_key FROM work WHERE id = ?1", [work], |r| {
            r.get(0)
        })
        .expect("reads")
    };
    let (older_before, younger_before) = (key(older), key(younger));

    write_pages(&dir.path().join("Series").join(gained), 3);
    scan::scan_root(&conn, &root).expect("rescans");
    derive::derive(&conn).expect("re-derives");

    assert_ne!(
        key(older),
        older_before,
        "the older work was taken for the gained files"
    );
    assert_eq!(key(younger), younger_before, "and the younger is as it was");
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        3,
        "nothing was minted"
    );
}

/// A directory holding only chapters makes no work, and still names a series:
/// the one of its title, which a volume directory in another library names
/// too.
#[test]
fn a_chapter_only_dir_names_the_series_of_its_title() {
    let volumes = crate::tests::tempdir("derive-lib-chapters-volumes");
    write_pages(&volumes.path().join("Series").join("Series - v01"), 2);
    let chapters = crate::tests::tempdir("derive-lib-chapters-chapters");
    write_pages(&chapters.path().join("Series").join("Series - c001"), 1);
    write_pages(&chapters.path().join("Series").join("Series - c002"), 1);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let v = scanned_root(&conn, volumes.path(), None);
    let c = scanned_root(&conn, chapters.path(), None);
    assert_ne!(v.libraries[0], c.libraries[0], "the fixture: two libraries");

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(report.unattached_chapter_dirs, 1);
    let series: Vec<String> = conn
        .prepare("SELECT sort_title FROM series ORDER BY sort_title, id")
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert_eq!(
        series,
        vec!["Series".to_string()],
        "one `Series`, named by both libraries' directories"
    );
}

// --- A title belongs to one series across the store ---------------------------

fn titled(titles: &[(&str, bool)]) -> Vec<crate::librarydoc::SeriesTitle> {
    titles
        .iter()
        .map(|(title, primary)| crate::librarydoc::SeriesTitle {
            title: (*title).to_string(),
            kind: if *primary {
                crate::librarydoc::TitleKind::Romaji
            } else {
                crate::librarydoc::TitleKind::Alt
            },
            primary: *primary,
        })
        .collect()
}

/// A title one series holds is refused to every other: one of the same
/// library, one of another library, and one no directory names.
#[test]
fn a_title_another_series_holds_is_refused_across_the_store() {
    let one = crate::tests::tempdir("derive-titles-one");
    write_pages(&one.path().join("Series").join("Series - v01"), 2);
    write_pages(&one.path().join("Same").join("Same - v01"), 3);
    let two = crate::tests::tempdir("derive-titles-two");
    write_pages(&two.path().join("Other").join("Other - v01"), 4);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, one.path(), None);
    scanned_root(&conn, two.path(), None);
    derive::derive(&conn).expect("derives");
    conn.execute_batch(
        "INSERT INTO series (sort_title) VALUES ('Loose');
         INSERT INTO series_title (series_id, title, is_primary)
         VALUES (last_insert_rowid(), 'Loose', 1);",
    )
    .expect("a series no directory names");
    let id_of = |title: &str| {
        scalar(
            &conn,
            &format!("SELECT id FROM series WHERE sort_title = '{title}'"),
        )
    };
    let held = id_of("Series");

    for title in ["Same", "Other", "Loose"] {
        let edited = id_of(title);
        let refused = crate::librarydoc::set_series_titles(
            &conn,
            edited,
            &titled(&[(title, true), ("Series", false)]),
        )
        .expect_err("a title another series holds");
        assert_eq!(
            refused,
            format!(
                "title `Series` already belongs to series {held}; refusing to give series \
                 {edited} the same text"
            ),
            "refused for `{title}`"
        );
    }
}

// --- A volume held under two directories (overlapping-libraries findings §0) -

use crate::tests::{a_copy_of, a_copy_of_one_volume, member_root_ids, the_member};

/// Spec §1: a byte-identical volume in two libraries is one work, with one
/// position per account. The second library's copy joins the member the
/// first made, so the work keeps its key, slug and position, and either
/// directory's `member_root` row leads to it.
#[test]
fn a_volume_held_in_two_libraries_is_one_work_of_one_member() {
    let (first, second) = a_copy_of_one_volume();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let a = scanned_root(&conn, first.path(), None);
    derive::derive(&conn).expect("derives the first library");
    let (work_id, key, slug): (i64, String, String) = conn
        .query_row("SELECT id, content_key, slug FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .expect("the one work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [work_id],
    )
    .expect("a position");
    let b = scanned_root(&conn, second.path(), None);
    assert_ne!(a.libraries[0], b.libraries[0], "the fixture: two libraries");

    let report = derive::derive(&conn).expect("derives both");

    assert_eq!(
        (report.members, report.merged, report.rekeyed),
        (1, 0, 0),
        "one member, no merge and no rekey"
    );
    assert_eq!(
        conn.query_row("SELECT id, content_key, slug FROM work", [], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .expect("still one work"),
        (work_id, key, slug),
        "the same work, its key that of one member"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM member"),
        1,
        "one member"
    );
    assert_eq!(member_root_ids(&conn), vec![a.id, b.id], "held under both");
    for root in [a.id, b.id] {
        let slots: Vec<i64> = conn
            .prepare(
                "SELECT p.slot FROM member_root mr
                   JOIN member m ON m.id = mr.member_id
                   JOIN edition e ON e.id = m.edition_id
                   JOIN progress p ON p.work_id = e.work_id
                  WHERE mr.root_id = ?1",
            )
            .expect("prepare")
            .query_map([root], |r| r.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");
        assert_eq!(slots, vec![2], "one position, read through root {root}");
    }
}

/// Two byte-identical files under one directory stay two members of one
/// page sequence: a `member_root` row is per directory, so a second copy
/// there would have nothing to record it.
#[test]
fn two_byte_identical_files_under_one_directory_stay_two_members() {
    let dir = crate::tests::tempdir("derive-copy-one-directory");
    let archive = dir.path().join("Foo").join("Foo v01.cbz");
    crate::scan::tests::write_archive(&archive, 3);
    std::fs::copy(&archive, dir.path().join("Foo").join("Foo v01 again.cbz")).expect("copies");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, dir.path(), None);

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (report.members, report.merged),
        (2, 1),
        "two members, one merge"
    );
    assert_eq!(member_root_ids(&conn), vec![root.id, root.id], "a row each");
}

/// A directory of the library called `library` for `dir`, with its edition
/// key set to `edition` where one is given, scanned.
fn root_in(
    conn: &Connection,
    dir: &std::path::Path,
    library: &str,
    edition: Option<&str>,
) -> scan::Root {
    let root = scan::add_root_in(conn, dir, scan::InLibrary::Named(library)).expect("adds");
    if let Some(edition) = edition {
        scan::update_root(
            conn,
            root.id,
            &scan::RootPatch {
                edition_key: Some(Some(edition.to_string())),
                ..Default::default()
            },
        )
        .expect("sets the directory's edition");
    }
    let root = scan::roots(conn)
        .expect("lists")
        .into_iter()
        .find(|r| r.id == root.id)
        .expect("the directory just added");
    scan::scan_root(conn, &root).expect("scans");
    root
}

/// A store with libraries A (70) and B (80) and no directory yet.
fn libraries_a_and_b() -> Library {
    let library = Library::open_in_memory().expect("open");
    library
        .conn()
        .execute(
            "INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B')",
            [],
        )
        .expect("libraries");
    library
}

/// Every work with its series and volume, every member with its directory
/// and path, and every `member_root` row: what an unchanged pass must leave
/// as it found it.
fn store_shape(conn: &Connection) -> String {
    let works: Vec<(i64, String, Option<f64>)> = conn
        .prepare(
            "SELECT w.id, s.sort_title, w.volume FROM work w
               JOIN series s ON s.id = w.series_id ORDER BY w.id",
        )
        .and_then(|mut q| {
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("works");
    let members: Vec<(i64, String, i64, Option<i64>, String)> = conn
        .prepare(
            "SELECT e.work_id, e.key, m.ordinal, m.root_id, m.path FROM member m
               JOIN edition e ON e.id = m.edition_id ORDER BY e.work_id, e.key, m.ordinal",
        )
        .and_then(|mut q| {
            q.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect()
        })
        .expect("members");
    let held: Vec<(String, i64)> = conn
        .prepare(
            "SELECT m.path, mr.root_id FROM member_root mr
               JOIN member m ON m.id = mr.member_id ORDER BY m.path, mr.root_id",
        )
        .and_then(|mut q| q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .expect("member_root");
    format!("works {works:?}; members {members:?}; held {held:?}")
}

/// How far one derive over unchanged files moves the structure counter.
fn an_unchanged_pass_moves(conn: &Connection) -> i64 {
    let before = crate::structure::version(conn).expect("reads");
    derive::derive(conn).expect("derives");
    crate::structure::version(conn).expect("reads") - before
}

/// The series of the one work in the store.
fn the_series(conn: &Connection) -> String {
    conn.query_row(
        "SELECT s.sort_title FROM work w JOIN series s ON s.id = w.series_id",
        [],
        |r| r.get(0),
    )
    .expect("one work in one series")
}

/// `Foo/Foo v01.cbz` under library A's directory, and the same bytes at
/// `Bar/Bar v01.cbz` under library B's, so the two directories' rules name
/// one file in two series. With `b_older`, B's directory is added first and
/// has the lower root id. Returns A's directory, then B's.
fn a_copy_under_two_series(
    b_older: bool,
) -> (Library, scan::Root, scan::Root, [crate::tests::TempDir; 2]) {
    let a_dir = crate::tests::tempdir("derive-two-series-a");
    let archive = a_dir.path().join("Foo").join("Foo v01.cbz");
    write_archive(&archive, 3);
    let b_dir = crate::tests::tempdir("derive-two-series-b");
    std::fs::create_dir_all(b_dir.path().join("Bar")).expect("mkdir");
    std::fs::copy(&archive, b_dir.path().join("Bar").join("Bar v01.cbz")).expect("copies");
    let library = libraries_a_and_b();
    let (a, b) = {
        let conn = library.conn();
        if b_older {
            let b = root_in(&conn, b_dir.path(), "B", None);
            (root_in(&conn, a_dir.path(), "A", None), b)
        } else {
            let a = root_in(&conn, a_dir.path(), "A", None);
            (a, root_in(&conn, b_dir.path(), "B", None))
        }
    };
    (library, a, b, [a_dir, b_dir])
}

/// The ruling of 2026-11-04 on overlapping libraries: a byte-identical file
/// is one work however its directories' rules name it, held under both,
/// named by the oldest directory's rules, and written once per pass, so it
/// moves the structure counter as a copy both directories name alike does.
/// Each library's grant sees it.
#[test]
fn a_copy_whose_directories_name_it_in_two_series_is_one_work_held_under_both() {
    let alike = {
        let (first, second) = a_copy_of_one_volume();
        let library = libraries_a_and_b();
        let conn = library.conn();
        root_in(&conn, first.path(), "A", None);
        root_in(&conn, second.path(), "B", None);
        derive::derive(&conn).expect("derives");
        an_unchanged_pass_moves(&conn)
    };
    for b_older in [false, true] {
        let (library, a, b, _dirs) = a_copy_under_two_series(b_older);
        let conn = library.conn();
        assert_eq!(
            a.id < b.id,
            !b_older,
            "the fixture: the older directory has the lower id"
        );
        let report = derive::derive(&conn).expect("derives");
        let shape = store_shape(&conn);
        let series = if b_older { "Bar" } else { "Foo" };

        assert_eq!(report.works, 1, "b older {b_older}: written as one work");
        assert_eq!(
            scalar(&conn, "SELECT count(*) FROM work"),
            1,
            "b older {b_older}: one work"
        );
        assert_eq!(
            scalar(&conn, "SELECT count(*) FROM member"),
            1,
            "b older {b_older}: one member"
        );
        let mut both = vec![a.id, b.id];
        both.sort_unstable();
        assert_eq!(
            member_root_ids(&conn),
            both,
            "b older {b_older}: held under both"
        );
        assert_eq!(
            the_series(&conn),
            series,
            "b older {b_older}: named by the older directory's rules"
        );
        for pass in 1..=2 {
            assert_eq!(
                an_unchanged_pass_moves(&conn),
                alike,
                "b older {b_older}, pass {pass}: written once, as a copy named alike is"
            );
            assert_eq!(
                store_shape(&conn),
                shape,
                "b older {b_older}, pass {pass}: nothing moves"
            );
        }
        let slug: String = conn
            .query_row("SELECT slug FROM work", [], |r| r.get(0))
            .expect("the slug");
        for root in [a.id, b.id] {
            let seen =
                crate::browse::visible_editions(&conn, &crate::account::Access::roots(vec![root]))
                    .expect("reads");
            assert_eq!(
                seen.get(&slug).cloned(),
                Some(std::collections::HashSet::from(["raw".to_string()])),
                "b older {b_older}: the grant on directory {root} sees it"
            );
        }
    }
}

/// Removing either directory keeps the work and its reader's position, and
/// the next name is the remaining directory's.
#[test]
fn removing_either_directory_of_a_copy_named_in_two_series_keeps_the_work() {
    for b_older in [false, true] {
        for remove_a in [false, true] {
            let (library, a, b, _dirs) = a_copy_under_two_series(b_older);
            let conn = library.conn();
            derive::derive(&conn).expect("derives");
            let work_id = scalar(&conn, "SELECT id FROM work");
            conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
                [work_id],
            )
            .expect("a position");
            let (gone, kept, series) = if remove_a {
                (a.id, b.id, "Bar")
            } else {
                (b.id, a.id, "Foo")
            };
            let label = format!("b older {b_older}, removing {gone}");

            let (removal, _) = scan::remove_root(&conn, gone).expect("removes");

            assert_eq!(
                (removal.works, removal.positions),
                (0, 0),
                "{label}: nothing deleted"
            );
            assert_eq!(
                scalar(&conn, "SELECT id FROM work"),
                work_id,
                "{label}: the same work"
            );
            assert_eq!(
                scalar(
                    &conn,
                    "SELECT count(*) FROM progress WHERE work_id = (SELECT id FROM work)"
                ),
                1,
                "{label}: its position"
            );
            assert_eq!(
                member_root_ids(&conn),
                vec![kept],
                "{label}: held under the other"
            );
            assert_eq!(
                the_series(&conn),
                series,
                "{label}: named by the other's rules"
            );
        }
    }
}

/// A held primary is sticky across series too: a copy arriving under an
/// older directory whose rules name another series renames nothing.
#[test]
fn a_copy_named_in_another_series_arriving_under_an_older_directory_renames_nothing() {
    let (library, a, b, dirs) = a_copy_under_two_series(false);
    let conn = library.conn();
    assert!(a.id < b.id, "the fixture: A's directory is older");
    let held = dirs[0].path().join("Foo").join("Foo v01.cbz");
    let aside = crate::tests::tempdir("derive-two-series-aside");
    std::fs::rename(&held, aside.path().join("Foo v01.cbz")).expect("set aside");
    scan::scan_root(&conn, &a).expect("rescans A without it");
    derive::derive(&conn).expect("derives the work under B");
    let work_id = scalar(&conn, "SELECT id FROM work");
    let before = (
        the_member(&conn),
        crate::hand::path_for(&conn, work_id).expect("an address"),
    );
    assert_eq!(before.0.0, Some(b.id), "the fixture: held under B");
    assert_eq!(the_series(&conn), "Bar", "the fixture: named by B's rules");

    std::fs::rename(aside.path().join("Foo v01.cbz"), &held).expect("the copy arrives");
    scan::scan_root(&conn, &a).expect("rescans A");
    derive::derive(&conn).expect("derives again");

    assert_eq!(
        (
            the_member(&conn),
            crate::hand::path_for(&conn, work_id).expect("an address"),
        ),
        before,
        "member.root_id, member.path and the document's address stay"
    );
    assert_eq!(the_series(&conn), "Bar", "and the name");
    assert_eq!(member_root_ids(&conn), vec![a.id, b.id], "held under both");
}

/// The members of `slug`'s `edition` in page order, each as whether `root`'s
/// grant sees it.
fn seen_through(conn: &Connection, root: i64, slug: &str, edition: &str) -> Vec<bool> {
    crate::browse::visible_members(
        conn,
        &crate::account::Access::roots(vec![root]),
        slug,
        Some(edition),
    )
    .expect("reads")
    .remove(edition)
    .unwrap_or_default()
    .into_iter()
    .map(|m| m.visible)
    .collect()
}

/// The slug of the work in `series`.
fn slug_in(conn: &Connection, series: &str) -> String {
    conn.query_row(
        "SELECT w.slug FROM work w JOIN series s ON s.id = w.series_id WHERE s.sort_title = ?1",
        [series],
        |r| r.get(0),
    )
    .expect("one work in the series")
}

/// A volume that shares one of its two files with another directory's
/// volume splits, per file (the ruling of 2026-11-04): the shared file is a
/// copy in the older directory's work, which B's grant sees narrowed to it,
/// and B's other file is a work of its own. The hidden member is the first,
/// so a narrowing that hid nothing would answer differently.
#[test]
fn a_volume_sharing_one_of_its_files_splits_per_file() {
    let a_dir = crate::tests::tempdir("derive-split-a");
    let b_dir = crate::tests::tempdir("derive-split-b");
    let shared = a_dir.path().join("Foo").join("Foo - v01 - c002.cbz");
    write_archive(&a_dir.path().join("Foo").join("Foo - v01 - c001.cbz"), 3);
    write_archive(&shared, 4);
    write_archive(&b_dir.path().join("Bar").join("Bar - v01 - c001.cbz"), 5);
    std::fs::copy(
        &shared,
        b_dir.path().join("Bar").join("Bar - v01 - c002.cbz"),
    )
    .expect("copies");
    let library = libraries_a_and_b();
    let conn = library.conn();
    let a = root_in(&conn, a_dir.path(), "A", None);
    let b = root_in(&conn, b_dir.path(), "B", None);
    assert!(a.id < b.id, "the fixture: A's directory is older");

    let report = derive::derive(&conn).expect("derives");
    let shape = store_shape(&conn);

    assert_eq!(report.works, 2, "two works");
    let foo = slug_in(&conn, "Foo");
    let bar = slug_in(&conn, "Bar");
    assert_eq!(
        seen_through(&conn, a.id, &foo, "raw"),
        [true, true],
        "A sees its volume whole"
    );
    assert_eq!(
        seen_through(&conn, b.id, &foo, "raw"),
        [false, true],
        "B sees A's volume narrowed to the shared file"
    );
    assert_eq!(
        seen_through(&conn, b.id, &bar, "raw"),
        [true],
        "and its other file as a work of its own"
    );
    assert!(
        seen_through(&conn, a.id, &bar, "raw")
            .iter()
            .all(|seen| !seen),
        "which A does not see"
    );
    for pass in 1..=2 {
        let report = derive::derive(&conn).expect("derives");
        assert_eq!(
            (report.works, report.rekeyed),
            (2, 0),
            "pass {pass}: the same two works, rekeyed by nothing"
        );
        assert_eq!(store_shape(&conn), shape, "pass {pass}: nothing moves");
    }
}

/// The partial shape that only a per-file collapse settles: B's directories
/// give its volume a shared `raw` file and a `zh` file of its own, and `raw`
/// sorts first, so B's group would take `raw` as its hub and hash the key of
/// A's work. The shared file is a copy in A's work and `zh` is a work of B's.
#[test]
fn a_shared_hub_file_beside_an_edition_of_its_own_is_two_works() {
    let a_dir = crate::tests::tempdir("derive-hub-a");
    let raw_dir = crate::tests::tempdir("derive-hub-raw");
    let zh_dir = crate::tests::tempdir("derive-hub-zh");
    let archive = a_dir.path().join("Foo").join("Foo v01.cbz");
    write_archive(&archive, 3);
    std::fs::create_dir_all(raw_dir.path().join("Bar")).expect("mkdir");
    std::fs::copy(&archive, raw_dir.path().join("Bar").join("Bar v01.cbz")).expect("copies");
    write_archive(&zh_dir.path().join("Bar").join("Bar v01.cbz"), 4);
    let library = libraries_a_and_b();
    let conn = library.conn();
    let a = root_in(&conn, a_dir.path(), "A", None);
    let raw = root_in(&conn, raw_dir.path(), "B", None);
    let zh = root_in(&conn, zh_dir.path(), "B", Some("zh"));
    assert!(
        derive::DEFAULT_EDITION_KEY < "zh",
        "the fixture: the shared edition sorts first, so it would be B's hub"
    );

    derive::derive(&conn).expect("derives");
    let shape = store_shape(&conn);

    let foo = slug_in(&conn, "Foo");
    let bar = slug_in(&conn, "Bar");
    assert_ne!(foo, bar, "two works");
    let b_grant = crate::account::Access::roots(vec![raw.id, zh.id]);
    let b_sees = crate::browse::visible_editions(&conn, &b_grant).expect("reads");
    let a_sees = crate::browse::visible_editions(&conn, &crate::account::Access::roots(vec![a.id]))
        .expect("reads");
    let keys = |key: &str| Some(std::collections::HashSet::from([key.to_string()]));
    assert_eq!(
        b_sees.get(&foo).cloned(),
        keys("raw"),
        "B sees the shared file in A's work"
    );
    assert_eq!(b_sees.get(&bar).cloned(), keys("zh"), "and its own edition");
    assert_eq!(a_sees.get(&foo).cloned(), keys("raw"), "A sees its work");
    assert_eq!(a_sees.get(&bar), None, "and not B's edition");
    for pass in 1..=2 {
        let report = derive::derive(&conn).expect("derives");
        assert_eq!(
            (report.works, report.rekeyed),
            (2, 0),
            "pass {pass}: the same two works, rekeyed by nothing"
        );
        assert_eq!(store_shape(&conn), shape, "pass {pass}: nothing moves");
    }
}

/// What removing a directory leaves of a work whose shared file the other
/// directory's rules name into another work: one work, the file one member of
/// it at the kept directory's copy, no member under no directory, the
/// reader's position carried onto it, and the rehearsal saying the work folds
/// and the position moves. Answers the reader's slot afterwards.
fn removing_the_directory_a_shared_file_was_named_by(
    conn: &Connection,
    gone: &scan::Root,
    kept: &scan::Root,
    slot: i64,
    label: &str,
) -> i64 {
    let (held, sha256, root): (i64, String, Option<i64>) = conn
        .query_row(
            "SELECT e.work_id, m.sha256, m.root_id FROM member m
               JOIN edition e ON e.id = m.edition_id
              WHERE (SELECT count(*) FROM member_root mr WHERE mr.member_id = m.id) = 2",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the one member held under both directories");
    assert_eq!(
        root,
        Some(gone.id),
        "{label}: the fixture: the shared file is named by the directory removed"
    );
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', ?2)",
        rusqlite::params![held, slot],
    )
    .expect("a position");

    let (cost, rehearsed) = scan::removal_cost(conn, gone.id).expect("rehearses");
    assert_eq!(
        (cost.works, cost.positions),
        (0, 0),
        "{label}: the rehearsal deletes nothing"
    );
    assert_eq!(
        (
            rehearsed.emptied_works,
            rehearsed.moved_positions,
            rehearsed.lost_positions,
            rehearsed.unfolded_works,
        ),
        (1, 1, 0, 0),
        "{label}: the rehearsal folds the work and moves the position: {:?}",
        rehearsed.unfoldable
    );
    let (removal, _) = scan::remove_root(conn, gone.id).expect("removes");
    assert_eq!((removal.works, removal.positions), (0, 0), "{label}");

    assert_eq!(
        scalar(conn, "SELECT count(*) FROM work"),
        1,
        "{label}: no work is left holding the file twice"
    );
    let shared: Vec<(i64, Option<i64>, String)> = conn
        .prepare(
            "SELECT e.work_id, m.root_id, m.path FROM member m
               JOIN edition e ON e.id = m.edition_id WHERE m.sha256 = ?1",
        )
        .and_then(|mut q| {
            q.query_map([&sha256], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("the shared file's members");
    assert_eq!(
        shared.len(),
        1,
        "{label}: one member holds the file: {shared:?}"
    );
    let (surviving, root, path) = &shared[0];
    assert_eq!(*root, Some(kept.id), "{label}: under the kept directory");
    assert!(
        path.starts_with(&kept.path.display().to_string()),
        "{label}: at its copy: {path}"
    );
    assert_eq!(
        scalar(conn, "SELECT count(*) FROM member WHERE root_id IS NULL"),
        0,
        "{label}: no member is left under no directory"
    );
    let positions: Vec<(i64, String, i64)> = conn
        .prepare("SELECT work_id, edition, slot FROM progress")
        .and_then(|mut q| {
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("positions");
    assert_eq!(
        positions.len(),
        1,
        "{label}: the position survives: {positions:?}"
    );
    assert_eq!(
        (positions[0].0, positions[0].1.as_str()),
        (*surviving, "raw"),
        "{label}: on the work holding the file"
    );
    for pass in 1..=2 {
        let report = derive::derive(conn).expect("derives");
        assert_eq!(
            (report.works, report.emptied_works, report.unfolded_works),
            (1, 0, 0),
            "{label}, pass {pass}: nothing more to fold"
        );
    }
    positions[0].2
}

/// The reviewer's second shape: B, the older directory, holds a volume of
/// three chapter files, one of them byte-identical to a chapter A's rules
/// file under another series; B goes. The shared file becomes a work of
/// A's, Bar is folded into it, and a reader on the shared file's second page
/// is on it still where nothing before it went.
#[test]
fn removing_the_older_directory_of_a_shared_chapter_folds_its_work_into_the_copys() {
    for shared_at in [0, 1] {
        let a_dir = crate::tests::tempdir("derive-fold-a");
        let b_dir = crate::tests::tempdir("derive-fold-b");
        let mut pages_before = 0;
        for (at, pages) in [3_usize, 5, 6].into_iter().enumerate() {
            let name = format!("Bar - v01 - c00{}.cbz", at + 1);
            let pages = if at == shared_at { 4 } else { pages };
            write_archive(&b_dir.path().join("Bar").join(&name), pages);
            if at < shared_at {
                pages_before += i64::try_from(pages).expect("a few pages");
            }
        }
        std::fs::create_dir_all(a_dir.path().join("Foo")).expect("mkdir");
        std::fs::copy(
            b_dir
                .path()
                .join("Bar")
                .join(format!("Bar - v01 - c00{}.cbz", shared_at + 1)),
            a_dir
                .path()
                .join("Foo")
                .join(format!("Foo - v01 - c00{}.cbz", shared_at + 1)),
        )
        .expect("copies");
        let library = libraries_a_and_b();
        let conn = library.conn();
        let b = root_in(&conn, b_dir.path(), "B", None);
        let a = root_in(&conn, a_dir.path(), "A", None);
        assert!(b.id < a.id, "the fixture: B's directory is older");
        derive::derive(&conn).expect("derives");
        assert_eq!(
            scalar(&conn, "SELECT count(*) FROM work"),
            1,
            "the fixture: B's volume is one work, the shared file a copy in it"
        );
        let label = format!("shared file at {shared_at}");

        let slot = removing_the_directory_a_shared_file_was_named_by(
            &conn,
            &b,
            &a,
            pages_before + 1,
            &label,
        );

        assert_eq!(the_series(&conn), "Foo", "{label}: named by A's rules");
        if shared_at == 0 {
            assert_eq!(slot, 1, "{label}: on the shared file's second page");
        }
    }
}

/// Removing the directory a copy is named by hands the name to the oldest
/// directory left holding it, as a fresh store would: one file under A, B and
/// C, each naming it in a series of its own; A goes and B's rules name it.
#[test]
fn removing_the_primary_directory_of_a_copy_names_it_by_the_oldest_left() {
    let dirs: Vec<crate::tests::TempDir> = ["Foo", "Bar", "Baz"]
        .iter()
        .map(|series| crate::tests::tempdir(&format!("derive-oldest-{series}")))
        .collect();
    let archive = dirs[0].path().join("Foo").join("Foo v01.cbz");
    write_archive(&archive, 3);
    for (dir, series) in dirs.iter().zip(["Foo", "Bar", "Baz"]).skip(1) {
        std::fs::create_dir_all(dir.path().join(series)).expect("mkdir");
        std::fs::copy(
            &archive,
            dir.path().join(series).join(format!("{series} v01.cbz")),
        )
        .expect("copies");
    }
    let library = libraries_a_and_b();
    let conn = library.conn();
    let a = root_in(&conn, dirs[0].path(), "A", None);
    root_in(&conn, dirs[1].path(), "B", None);
    root_in(&conn, dirs[2].path(), "B", None);
    derive::derive(&conn).expect("derives");
    assert_eq!(the_series(&conn), "Foo", "the fixture: named by A's rules");

    scan::remove_root(&conn, a.id).expect("removes");

    assert_eq!(
        the_series(&conn),
        "Bar",
        "named by the older of the two left"
    );
}

/// The raw and zh shape with A removed: A's `raw` file is a copy in B's
/// `raw` directory, whose rules put it beside B's own `zh` edition in Bar. A
/// goes, and Foo is folded into Bar, the reader on its third page.
#[test]
fn removing_the_directory_of_a_shared_hub_file_folds_its_work_into_the_copys() {
    let a_dir = crate::tests::tempdir("derive-fold-hub-a");
    let raw_dir = crate::tests::tempdir("derive-fold-hub-raw");
    let zh_dir = crate::tests::tempdir("derive-fold-hub-zh");
    let archive = a_dir.path().join("Foo").join("Foo v01.cbz");
    write_archive(&archive, 3);
    std::fs::create_dir_all(raw_dir.path().join("Bar")).expect("mkdir");
    std::fs::copy(&archive, raw_dir.path().join("Bar").join("Bar v01.cbz")).expect("copies");
    write_archive(&zh_dir.path().join("Bar").join("Bar v01.cbz"), 4);
    let library = libraries_a_and_b();
    let conn = library.conn();
    let a = root_in(&conn, a_dir.path(), "A", None);
    let raw = root_in(&conn, raw_dir.path(), "B", None);
    root_in(&conn, zh_dir.path(), "B", Some("zh"));
    derive::derive(&conn).expect("derives");
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        2,
        "the fixture: Foo, and Bar's `zh`"
    );

    let slot = removing_the_directory_a_shared_file_was_named_by(&conn, &a, &raw, 2, "hub");

    assert_eq!(the_series(&conn), "Bar", "named by B's rules");
    assert_eq!(slot, 2, "on the file's third page");
    let keys: Vec<String> = conn
        .prepare("SELECT key FROM edition ORDER BY key")
        .and_then(|mut q| q.query_map([], |r| r.get(0))?.collect())
        .expect("keys");
    assert_eq!(keys, ["raw", "zh"], "Bar holds the file beside its own");
}

/// The split volume with its older directory removed: A held Foo's c001 and
/// c002, and B held its own c001 and a copy of c002. Once A goes, B's rules
/// put c002 in Bar beside B's c001, and Foo's edition, c002 alone, is a run
/// of Bar's rather than the same list, so the fold is refused (O243). Foo
/// stays its own work, held under B, with its reader's position, and every
/// derive reports it unfolded until a fold can carry it. No derive after the
/// removal writes Foo, and B holds its file, so it is never marked missing
/// (spec §5).
#[test]
fn removing_the_older_directory_of_a_split_volume_leaves_its_work_unfolded() {
    let a_dir = crate::tests::tempdir("derive-unfolded-a");
    let b_dir = crate::tests::tempdir("derive-unfolded-b");
    let shared = a_dir.path().join("Foo").join("Foo - v01 - c002.cbz");
    write_archive(&a_dir.path().join("Foo").join("Foo - v01 - c001.cbz"), 3);
    write_archive(&shared, 4);
    write_archive(&b_dir.path().join("Bar").join("Bar - v01 - c001.cbz"), 5);
    std::fs::copy(
        &shared,
        b_dir.path().join("Bar").join("Bar - v01 - c002.cbz"),
    )
    .expect("copies");
    let library = libraries_a_and_b();
    let conn = library.conn();
    let a = root_in(&conn, a_dir.path(), "A", None);
    let b = root_in(&conn, b_dir.path(), "B", None);
    derive::derive(&conn).expect("derives");
    let work_in = |series: &str| {
        scalar(
            &conn,
            &format!(
                "SELECT w.id FROM work w JOIN series s ON s.id = w.series_id
                  WHERE s.sort_title = '{series}'"
            ),
        )
    };
    let (foo, bar) = (work_in("Foo"), work_in("Bar"));
    let sha256: String = conn
        .query_row(
            "SELECT sha256 FROM member WHERE path = ?1",
            [shared.display().to_string()],
            |r| r.get(0),
        )
        .expect("the shared file's member");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 4)",
        [foo],
    )
    .expect("a position on c002's second page");
    let refused = vec![format!(
        "work {foo} holds edition(s) `raw` with no digest-identical edition on work {bar}; \
         nothing was moved and nothing was deleted"
    )];

    let (cost, rehearsed) = scan::removal_cost(&conn, a.id).expect("rehearses");
    assert_eq!(
        (cost.works, cost.positions),
        (0, 0),
        "the rehearsal deletes nothing"
    );
    assert_eq!(
        (rehearsed.emptied_works, rehearsed.unfolded_works),
        (0, 1),
        "the rehearsal says Foo is not folded"
    );
    assert_eq!(rehearsed.unfoldable, refused, "in absorb's words");
    let (removal, derived) = scan::remove_root(&conn, a.id).expect("removes");
    assert_eq!((removal.works, removal.positions), (0, 0));
    assert_eq!(
        (derived.unfolded_works, derived.unfoldable.clone()),
        (1, refused.clone()),
        "the removal's own derive"
    );

    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 2, "two works");
    let foos: Vec<(String, Option<i64>, String)> = conn
        .prepare(
            "SELECT m.sha256, m.root_id, m.path FROM member m
               JOIN edition e ON e.id = m.edition_id WHERE e.work_id = ?1",
        )
        .and_then(|mut q| {
            q.query_map([foo], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("Foo's members");
    assert_eq!(foos.len(), 1, "Foo holds c002 alone: {foos:?}");
    assert_eq!(
        (foos[0].0.as_str(), foos[0].1),
        (sha256.as_str(), Some(b.id)),
        "under B"
    );
    assert!(
        foos[0].2.starts_with(&b.path.display().to_string()),
        "at B's copy: {}",
        foos[0].2
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM member WHERE root_id IS NULL"),
        0,
        "no member under no directory"
    );
    let unmarked = {
        let mut works = vec![(foo, false), (bar, false)];
        works.sort_unstable();
        works
    };
    assert_eq!(
        marks(&conn),
        unmarked,
        "B holds Foo's c002, so no work is marked, the unfolded Foo included"
    );
    let positions = || -> Vec<(i64, String, i64)> {
        conn.prepare("SELECT work_id, edition, slot FROM progress")
            .and_then(|mut q| {
                q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .expect("positions")
    };
    // Past the end of Foo's 4 pages: this removal rewrites no stored slot,
    // and this row, inserted with no anchor, is read at it (O244).
    assert_eq!(
        positions(),
        [(foo, "raw".to_string(), 4)],
        "the position stays on Foo"
    );

    let shape = store_shape(&conn);
    for pass in 1..=2 {
        let report = derive::derive(&conn).expect("derives");
        assert_eq!(
            (
                report.emptied_works,
                report.unfolded_works,
                report.unfoldable
            ),
            (0, 1, refused.clone()),
            "pass {pass}: reported unfolded again"
        );
        assert_eq!(store_shape(&conn), shape, "pass {pass}: nothing moves");
        assert_eq!(marks(&conn), unmarked, "pass {pass}: no work is marked");
        assert_eq!(
            positions(),
            [(foo, "raw".to_string(), 4)],
            "pass {pass}: the position"
        );
    }
}

/// One edition's directory removed from a work of two: B, the oldest
/// directory, held Bar's `raw` c001, c002 and c003, C held Bar's `en` c001,
/// and A a copy of c002 under Foo. Once B goes, A's rules file c002 in a work
/// of Foo, while Bar's `en` keeps Bar written, so `derive::superseded` never
/// weighs it and its `raw`, c002 alone, stays beside Foo's. Both works hold
/// the file, as O243's ruling keeps them, with the reader's position where it
/// was and nothing reported but a kept edition, and neither is marked missing
/// (spec §5).
#[test]
fn removing_the_directory_of_one_of_two_editions_keeps_its_stale_edition_beside_the_copys_work() {
    let b_dir = crate::tests::tempdir("derive-two-editions-b");
    let c_dir = crate::tests::tempdir("derive-two-editions-c");
    let a_dir = crate::tests::tempdir("derive-two-editions-a");
    let bar = |c: usize| {
        b_dir
            .path()
            .join("Bar")
            .join(format!("Bar - v01 - c00{c}.cbz"))
    };
    write_archive(&bar(1), 3);
    write_archive(&bar(2), 4);
    write_archive(&bar(3), 5);
    write_archive(&c_dir.path().join("Bar").join("Bar - v01 - c001.cbz"), 7);
    std::fs::create_dir_all(a_dir.path().join("Foo")).expect("mkdir");
    std::fs::copy(
        bar(2),
        a_dir.path().join("Foo").join("Foo - v01 - c002.cbz"),
    )
    .expect("copies");
    let library = libraries_a_and_b();
    let conn = library.conn();
    let b = root_in(&conn, b_dir.path(), "B", None);
    let c = root_in(&conn, c_dir.path(), "B", Some("en"));
    let a = root_in(&conn, a_dir.path(), "A", None);
    derive::derive(&conn).expect("derives");
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        1,
        "the fixture: one work, Bar"
    );
    let bar_work = scalar(&conn, "SELECT id FROM work");
    let sha256: String = conn
        .query_row(
            "SELECT sha256 FROM member WHERE path = ?1",
            [b.path
                .join("Bar")
                .join("Bar - v01 - c002.cbz")
                .display()
                .to_string()],
            |r| r.get(0),
        )
        .expect("c002's member, named by B");
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM member_root mr JOIN member m ON m.id = mr.member_id
                  WHERE m.sha256 = '{sha256}'"
            ),
        ),
        2,
        "the fixture: c002 is one member, held under B and A"
    );
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 4)",
        [bar_work],
    )
    .expect("a position on c002's second page");
    let reported = |r: &derive::DeriveReport| {
        (
            r.emptied_works,
            r.moved_positions,
            r.lost_positions,
            r.unfolded_works,
            r.kept_editions,
            r.unfoldable.clone(),
        )
    };
    let kept = (0, 0, 0, 0, 1, Vec::<String>::new());

    let (cost, rehearsed) = scan::removal_cost(&conn, b.id).expect("rehearses");
    assert_eq!(
        (cost.works, cost.positions),
        (0, 0),
        "the rehearsal deletes nothing"
    );
    assert_eq!(
        reported(&rehearsed),
        kept,
        "the rehearsal reports a kept edition and nothing else"
    );
    let (removal, derived) = scan::remove_root(&conn, b.id).expect("removes");
    assert_eq!((removal.works, removal.positions), (0, 0));
    assert_eq!(reported(&derived), kept, "the removal's own derive");

    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 2, "two works");
    let foo_work = scalar(
        &conn,
        &format!("SELECT id FROM work WHERE id <> {bar_work}"),
    );
    let members_of = |work: i64| -> Vec<(String, String, Option<i64>, String)> {
        conn.prepare(
            "SELECT e.key, m.sha256, m.root_id, m.path FROM member m
               JOIN edition e ON e.id = m.edition_id WHERE e.work_id = ?1
              ORDER BY e.key, m.ordinal",
        )
        .and_then(|mut q| {
            q.query_map([work], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect()
        })
        .expect("a work's members")
    };
    let copy = a
        .path
        .join("Foo")
        .join("Foo - v01 - c002.cbz")
        .display()
        .to_string();
    let en = c
        .path
        .join("Bar")
        .join("Bar - v01 - c001.cbz")
        .display()
        .to_string();
    let bars = members_of(bar_work);
    assert_eq!(
        bars.iter()
            .map(|(key, _, root, path)| (key.as_str(), *root, path.as_str()))
            .collect::<Vec<_>>(),
        [
            ("en", Some(c.id), en.as_str()),
            ("raw", Some(a.id), copy.as_str())
        ],
        "Bar keeps `en` and its stale `raw`, at A's copy"
    );
    assert_eq!(bars[1].1, sha256, "the stale `raw` is c002");
    assert_eq!(
        members_of(foo_work),
        [("raw".to_string(), sha256.clone(), Some(a.id), copy.clone())],
        "Foo holds c002 at the same path"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM member WHERE root_id IS NULL"),
        0,
        "no member under no directory"
    );
    let unmarked = {
        let mut works = vec![(bar_work, false), (foo_work, false)];
        works.sort_unstable();
        works
    };
    assert_eq!(
        marks(&conn),
        unmarked,
        "A holds c002 and C holds `en`, so no work is marked"
    );
    let positions = || -> Vec<(i64, String, i64)> {
        conn.prepare("SELECT work_id, edition, slot FROM progress")
            .and_then(|mut q| {
                q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .expect("positions")
    };
    // Past the end of the stale `raw`'s 4 pages: this removal rewrites no
    // stored slot, and this row, inserted with no anchor, is read at it
    // (O244).
    assert_eq!(
        positions(),
        [(bar_work, "raw".to_string(), 4)],
        "the position stays on Bar's `raw`"
    );

    let shape = store_shape(&conn);
    for pass in 1..=2 {
        let report = derive::derive(&conn).expect("derives");
        assert_eq!(reported(&report), kept, "pass {pass}: a kept edition again");
        assert_eq!(store_shape(&conn), shape, "pass {pass}: nothing moves");
        assert_eq!(marks(&conn), unmarked, "pass {pass}: no work is marked");
        assert_eq!(
            positions(),
            [(bar_work, "raw".to_string(), 4)],
            "pass {pass}: the position"
        );
    }
}

/// The primary copy is the one the work already names, so a copy appearing
/// under a directory of a lower id moves neither `member.path` nor the
/// work's document address.
#[test]
fn a_copy_under_an_older_directory_leaves_the_primary_where_it_was() {
    the_primary_stays_when_a_copy_arrives_under_an_older_directory("Foo v01.cbz", Address::Derived);
}

/// And for a work with no volume, which `held_work` finds by a member's path
/// rather than by its volume: the path it holds is the younger directory's.
#[test]
fn a_copy_of_a_volume_less_work_under_an_older_directory_leaves_the_primary_where_it_was() {
    the_primary_stays_when_a_copy_arrives_under_an_older_directory("Foo.cbz", Address::Derived);
}

/// And for a work a hand series has moved off the address its files resolve,
/// which `held_work` cannot find and content still does. Such a work is the
/// one with a hand document to keep at its address.
#[test]
fn a_copy_under_an_older_directory_leaves_the_primary_of_a_work_a_hand_series_moved() {
    the_primary_stays_when_a_copy_arrives_under_an_older_directory(
        "Foo v01.cbz",
        Address::HandSeries,
    );
}

/// The same for a hand volume.
#[test]
fn a_copy_under_an_older_directory_leaves_the_primary_of_a_work_a_hand_volume_moved() {
    the_primary_stays_when_a_copy_arrives_under_an_older_directory(
        "Foo v01.cbz",
        Address::HandVolume,
    );
}

/// And for a volume-less work a hand series moved, whose path probe is
/// scoped to the series its files resolve.
#[test]
fn a_copy_of_a_volume_less_work_under_an_older_directory_leaves_the_primary_a_hand_series_moved() {
    the_primary_stays_when_a_copy_arrives_under_an_older_directory("Foo.cbz", Address::HandSeries);
}

/// Where the work sits before the copy arrives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Address {
    /// Where its files resolve.
    Derived,
    /// In a series a person chose.
    HandSeries,
    /// At a volume a person chose.
    HandVolume,
}

fn the_primary_stays_when_a_copy_arrives_under_an_older_directory(name: &str, address: Address) {
    let (first, second) = a_copy_of(name);
    let held = first.path().join("Foo").join(name);
    let moved = crate::tests::tempdir("derive-copy-aside");
    std::fs::rename(&held, moved.path().join(name)).expect("set aside");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let a = scanned_root(&conn, first.path(), None);
    let b = scanned_root(&conn, second.path(), None);
    assert!(a.id < b.id, "the fixture: the empty directory is older");
    derive::derive(&conn).expect("derives the work under the younger");
    let work_id = scalar(&conn, "SELECT id FROM work");
    let before = (
        the_member(&conn),
        crate::hand::path_for(&conn, work_id).expect("an address"),
    );
    assert_eq!(
        before.0.0,
        Some(b.id),
        "the fixture: held under the younger"
    );
    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work", [], |r| r.get(0))
        .expect("the work's volume");
    assert_eq!(
        volume.is_none(),
        !name.contains(" v01"),
        "the fixture: a volume-less work only where the name carries none"
    );
    match address {
        Address::Derived => {}
        Address::HandSeries => {
            conn.execute("INSERT INTO series (sort_title) VALUES ('Elsewhere')", [])
                .expect("a series of a person's choosing");
            conn.execute(
                "UPDATE work SET series_id_hand = ?1, series_id = ?1",
                [conn.last_insert_rowid()],
            )
            .expect("a hand series");
        }
        Address::HandVolume => {
            conn.execute("UPDATE work SET volume_hand = 7, volume = 7", [])
                .expect("a hand volume");
        }
    }
    let at: (String, Option<f64>) = conn
        .query_row(
            "SELECT series.sort_title, work.volume FROM work
               JOIN series ON series.id = work.series_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the work's address");
    assert_eq!(
        at == ("Foo".to_string(), volume),
        address == Address::Derived,
        "the fixture: off the address its files resolve only where a hand value moved it"
    );

    std::fs::rename(moved.path().join(name), &held).expect("the copy arrives");
    scan::scan_root(&conn, &a).expect("rescans the older");
    derive::derive(&conn).expect("derives again");

    assert_eq!(
        (
            the_member(&conn),
            crate::hand::path_for(&conn, work_id).expect("an address"),
        ),
        before,
        "member.root_id, member.path and the document's address stay"
    );
    assert_eq!(member_root_ids(&conn), vec![a.id, b.id], "held under both");
}

/// A document reimport deletes and reinserts the members, and both
/// directories come back with the member, the column on the one its path
/// names.
#[test]
fn a_document_reimport_over_a_copied_volume_keeps_both_directories() {
    let (first, second) = a_copy_of_one_volume();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let a = scanned_root(&conn, first.path(), None);
    let b = scanned_root(&conn, second.path(), None);
    derive::derive(&conn).expect("derives");
    let work_id = scalar(&conn, "SELECT id FROM work");
    let before = the_member(&conn);
    assert_eq!(member_root_ids(&conn), vec![a.id, b.id], "the fixture");

    let doc = crate::project::sidecar(&conn, work_id).expect("projects");
    let reimported =
        crate::import::sidecar(&conn, &doc, &first.path().join("taiyaku.json")).expect("reimports");

    assert_eq!(reimported, work_id, "the same work");
    assert_eq!(member_root_ids(&conn), vec![a.id, b.id], "both directories");
    assert_eq!(the_member(&conn), before, "and the primary's");
}

/// Two directories of one library holding a copy are one member too. A work
/// that holds the copy twice, its key hashing two digests, is rekeyed once,
/// and counted.
#[test]
fn two_directories_of_one_library_holding_a_copy_are_one_member_rekeyed_once() {
    let (first, second) = a_copy_of_one_volume();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let mut roots = Vec::new();
    for dir in [first.path(), second.path()] {
        let root = scan::add_root_in(&conn, dir, scan::InLibrary::Named("Manga")).expect("adds");
        scan::scan_root(&conn, &root).expect("scans");
        roots.push(root);
    }
    derive::derive(&conn).expect("derives");
    let work_id = scalar(&conn, "SELECT id FROM work");
    // The copy as a second member, keyed on both digests.
    conn.execute(
        "INSERT INTO member (edition_id, ordinal, path, sha256, pages, root_id)
         SELECT edition_id, 1, ?1, sha256, pages, ?2 FROM member",
        rusqlite::params![
            second
                .path()
                .join("Foo")
                .join("Foo v01.cbz")
                .to_string_lossy(),
            roots[1].id
        ],
    )
    .expect("the second copy as a member");
    let doubled =
        crate::identity::content_key(&crate::project::sidecar(&conn, work_id).expect("projects"))
            .expect("keys");
    conn.execute(
        "UPDATE work SET content_key = ?1 WHERE id = ?2",
        rusqlite::params![doubled, work_id],
    )
    .expect("keyed on both");

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (report.members, report.rekeyed),
        (1, 1),
        "rekeyed to one member"
    );
    assert_eq!(scalar(&conn, "SELECT count(*) FROM member"), 1);
    assert_eq!(
        scalar(&conn, "SELECT id FROM work"),
        work_id,
        "the same work"
    );
    assert_eq!(
        member_root_ids(&conn),
        vec![roots[0].id, roots[1].id],
        "held under both"
    );
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(again.rekeyed, 0, "once");
}

/// The derived copy pair, with a group naming the member's candidate as
/// `write_roots` receives it: primary under `primary`, a copy under `copy`.
fn copied_group(conn: &Connection, primary: (i64, &str), copy: (i64, &str)) -> super::Group {
    let mut group = super::Group {
        series: "Foo".to_string(),
        volume: Some(1.0),
        editions: BTreeMap::new(),
        hub_hint: None,
    };
    let (sha256, pages): (String, u32) = conn
        .query_row("SELECT sha256, pages FROM member", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the one member");
    group.editions.insert(
        derive::DEFAULT_EDITION_KEY.to_string(),
        vec![super::Candidate {
            path: primary.1.to_string(),
            rel_path: "Foo/Foo v01.cbz".to_string(),
            root_id: primary.0,
            sha256,
            pages,
            chapter: None,
            title: None,
            kind: None,
            language: None,
            copies: vec![super::CopyAt {
                root_id: copy.0,
                path: copy.1.to_string(),
                rel_path: "Foo/Foo v01.cbz".to_string(),
                frozen: false,
            }],
            frozen: false,
            held_by_rule: false,
            joined_by: None,
        }],
    );
    group
}

/// The primary is kept where the same edition holds the bytes, not where
/// another edition's member of them sits: `raw` holds them under b, the
/// omnibus under a, and b's entry is put before a's.
#[test]
fn keep_primaries_reads_the_candidates_own_edition() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, '/a'), (2, '/b')",
        [],
    )
    .expect("roots");
    seed_work_at(
        &conn,
        1,
        "held",
        &[
            ("raw", "/b/Foo/Foo v01.cbz", "same"),
            ("omni", "/a/Omni.cbz", "same"),
        ],
        Some(2),
    );
    conn.execute(
        "UPDATE member SET root_id = 1 WHERE path = '/a/Omni.cbz'",
        [],
    )
    .expect("the omnibus member under a");
    let staged: Vec<super::Staged> = [(1, "/a/Foo/Foo v01.cbz"), (2, "/b/Foo/Foo v01.cbz")]
        .into_iter()
        .map(|(root_id, path)| {
            let mut group = copied_group(&conn, (root_id, path), (root_id, path));
            let mut candidate = group
                .editions
                .remove(derive::DEFAULT_EDITION_KEY)
                .expect("raw")
                .remove(0);
            candidate.copies.clear();
            super::Staged {
                candidate,
                series: "Foo".to_string(),
                volume: Some(1.0),
                edition_key: derive::DEFAULT_EDITION_KEY.to_string(),
                titled: None,
            }
        })
        .collect();
    let sha256 = staged[0].candidate.sha256.clone();
    let mut at = [0, 1];

    super::keep_primaries(&conn, &sha256, &staged, &mut at).expect("reads");

    let first = &staged[at[0]].candidate;
    assert_eq!(
        (first.root_id, first.path.as_str()),
        (2, "/b/Foo/Foo v01.cbz"),
        "the primary is the copy raw holds"
    );
}

/// Migration 018's `member_root_on_update` deletes the old directory's row
/// when the primary moves, though that directory still holds a copy;
/// `write_roots` writes the rows after the column, so both stay.
#[test]
fn a_primary_moving_to_another_copy_keeps_both_directories() {
    let (first, second) = a_copy_of_one_volume();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let a = scanned_root(&conn, first.path(), None);
    let b = scanned_root(&conn, second.path(), None);
    derive::derive(&conn).expect("derives");
    let work_id = scalar(&conn, "SELECT id FROM work");
    assert_eq!(
        the_member(&conn).0,
        Some(a.id),
        "the fixture: primary under a"
    );
    let group = copied_group(&conn, (b.id, "/b"), (a.id, "/a"));
    let mut report = derive::DeriveReport::default();

    super::write_roots(&conn, work_id, &group, &mut report).expect("writes");

    assert_eq!(the_member(&conn).0, Some(b.id), "the primary moved");
    assert_eq!(member_root_ids(&conn), vec![a.id, b.id], "and both stay");
}

/// A copy's directory removed while the derive runs gets no row and is
/// counted, as a primary's is; the primary's row stays.
#[test]
fn a_copys_directory_removed_mid_derive_is_counted_and_gets_no_row() {
    let (first, second) = a_copy_of_one_volume();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let a = scanned_root(&conn, first.path(), None);
    let b = scanned_root(&conn, second.path(), None);
    derive::derive(&conn).expect("derives");
    let work_id = scalar(&conn, "SELECT id FROM work");
    let group = copied_group(&conn, (a.id, "/a"), (b.id, "/b"));
    conn.execute("DELETE FROM root WHERE id = ?1", [b.id])
        .expect("removes the copy's directory");
    let mut report = derive::DeriveReport::default();

    super::write_roots(&conn, work_id, &group, &mut report)
        .expect("a vanished copy's directory does not fail the write");

    assert_eq!(report.root_removed_mid_derive, 1, "counted");
    assert_eq!(member_root_ids(&conn), vec![a.id], "the primary's row only");
    assert_eq!(the_member(&conn).0, Some(a.id));
}

/// What the comparison below compares: every work, edition, member, position
/// and note, each by its work's content key rather than its id, and each
/// member by its path under `top` and its digest. Root ids are left out.
fn rows_root_ids_aside(conn: &Connection, top: &std::path::Path) -> Vec<String> {
    let top = top
        .canonicalize()
        .expect("canonical")
        .to_string_lossy()
        .into_owned();
    let rows = |sql: &str| -> Vec<String> {
        conn.prepare(sql)
            .expect(sql)
            .query_map([], |r| r.get::<_, String>(0))
            .expect(sql)
            .collect::<rusqlite::Result<_>>()
            .expect(sql)
    };
    [
        "SELECT 'work ' || content_key || ' ' || slug || ' ' || ifnull(volume, '-') || ' '
                || title || ' ' || ifnull(title_hand, '-') || ' ' || reading || ' ' || hub
           FROM work ORDER BY content_key",
        "SELECT 'edition ' || w.content_key || ' ' || e.key
           FROM edition e JOIN work w ON w.id = e.work_id ORDER BY w.content_key, e.key",
        "SELECT 'member ' || w.content_key || ' ' || e.key || ' ' || m.ordinal || ' '
                || m.path || ' ' || m.pages || ' ' || m.sha256
           FROM member m JOIN edition e ON e.id = m.edition_id JOIN work w ON w.id = e.work_id
          ORDER BY w.content_key, e.key, m.ordinal",
        "SELECT 'progress ' || w.content_key || ' ' || p.user_id || ' ' || p.edition || ' '
                || p.slot
           FROM progress p JOIN work w ON w.id = p.work_id ORDER BY w.content_key, p.user_id",
        "SELECT 'note ' || w.content_key || ' ' || n.id || ' ' || n.edition || ' ' || n.slot
                || ' ' || n.body
           FROM note n JOIN work w ON w.id = n.work_id ORDER BY w.content_key, n.id",
    ]
    .iter()
    .flat_map(|sql| rows(sql))
    .map(|row| row.replace(&top, "<top>"))
    .collect()
}

/// The plan's "a hand-over does what an in-root rule edit does: no better and
/// no worse". Store A: Kids, at `m/kids` with `edition_key = en`, takes Bleach
/// over from Manga. Store B: Manga alone, with a rule on `kids/` that sets
/// edition `en`. The works, editions, members, positions and notes agree, root
/// ids aside: each Bleach work keeps `raw` beside `en`, counted in
/// `kept_editions`, with the reader left on `raw`. The series is the stated
/// difference: Kids' rules name it from `Bleach/…`, Manga's from `kids/…`.
#[test]
fn taking_over_a_nested_directory_leaves_the_rows_an_in_root_rule_edit_leaves() {
    let (a_top, a, a_manga) =
        scan::tests::manga_holding("derive-taken-over", scan::tests::NEST_FILES);
    let (taken, kids) = {
        let conn = a.conn();
        scan::tests::read_bleach_v01(&conn, &a_manga);
        let kids = scan::add_root_checked_in(
            &conn,
            &a_manga.path.join("kids"),
            scan::InLibrary::Named("Kids"),
        )
        .expect("adds Kids")
        .root;
        conn.execute(
            "UPDATE root SET edition_key = 'en' WHERE id = ?1",
            [kids.id],
        )
        .expect("Kids names its editions en");
        let kids = scan::Root {
            edition_key: Some("en".to_string()),
            ..kids
        };
        scan::scan_root(&conn, &kids).expect("scans Kids");
        scan::scan_root(&conn, &a_manga).expect("scans Manga");
        (derive::derive(&conn).expect("derives"), kids.id)
    };
    let (b_top, b, b_manga) =
        scan::tests::manga_holding("derive-rule-edit", scan::tests::NEST_FILES);
    let edited = {
        let conn = b.conn();
        scan::tests::read_bleach_v01(&conn, &b_manga);
        crate::rules::create(
            &conn,
            &crate::rules::NewRule {
                ordinal: None,
                enabled: true,
                root_id: Some(b_manga.id),
                kind: crate::rules::RuleKind::Prefix,
                pattern: "kids/".to_string(),
                match_on: crate::rules::MatchOn::Path,
                set_edition_key: Some("en".to_string()),
                set_kind: None,
                set_language: None,
                set_series: None,
                note: None,
            },
        )
        .expect("a rule on kids/ setting en");
        scan::scan_root(&conn, &b_manga).expect("scans Manga");
        derive::derive(&conn).expect("derives")
    };

    assert_eq!(
        (taken.kept_editions, edited.kept_editions),
        (2, 2),
        "each Bleach work keeps raw beside en"
    );
    let (a_rows, b_rows) = (
        rows_root_ids_aside(&a.conn(), a_top.path()),
        rows_root_ids_aside(&b.conn(), b_top.path()),
    );
    assert!(
        a_rows.iter().any(|row| row.starts_with("progress ")) && a_rows.len() > 10,
        "the fixture: {a_rows:#?}"
    );
    assert_eq!(a_rows, b_rows);
    let held: Vec<(String, i64)> = a
        .conn()
        .prepare(
            "SELECT e.key, mr.root_id FROM member m JOIN edition e ON e.id = m.edition_id
               JOIN member_root mr ON mr.member_id = m.id
              WHERE substr(m.path, 1, length(?1) + 1) = ?1 || '/'
              ORDER BY e.key, mr.root_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([a_manga.path.join("kids").to_string_lossy()], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect()
        })
        .expect("reads where Bleach's members are held");
    assert_eq!(
        held,
        [("en", kids), ("en", kids), ("raw", kids), ("raw", kids)]
            .map(|(key, root)| (key.to_string(), root)),
        "the kept raw members are Kids' too, so a grant of Kids reaches them"
    );
    assert_eq!(
        (
            scan::tests::series_names(&a.conn()),
            scan::tests::series_names(&b.conn())
        ),
        (
            vec!["Bleach".to_string(), "Naruto".to_string()],
            vec!["Naruto".to_string(), "kids".to_string()]
        )
    );
}

// --- A work whose files are gone (pre-deploy spec §5) --------------------------

/// `Series/Series v0{n}.cbz` for each volume up to `count`, every one under
/// one directory, and the path of each.
fn a_series_of(label: &str, count: usize) -> (crate::tests::TempDir, Vec<std::path::PathBuf>) {
    let dir = crate::tests::tempdir(label);
    let paths: Vec<_> = (1..=count)
        .map(|n| {
            let path = dir.path().join("Series").join(format!("Series v0{n}.cbz"));
            write_archive(&path, 2 + n);
            path
        })
        .collect();
    (dir, paths)
}

/// The id and slug of the work holding volume `volume`.
fn volume_work(conn: &Connection, volume: i64) -> (i64, String) {
    conn.query_row(
        "SELECT id, slug FROM work WHERE volume = ?1",
        [volume],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .expect("the volume's work")
}

/// Whether the work holding volume `volume` is marked missing.
fn marked(conn: &Connection, volume: i64) -> bool {
    conn.query_row(
        "SELECT missing_since IS NOT NULL FROM work WHERE volume = ?1",
        [volume],
        |r| r.get(0),
    )
    .expect("reads the volume's mark")
}

/// The mark pass runs after the fold loop: the colour work is marked while its
/// directory is away, and when it comes back under the fold rule the work is
/// folded into the base one, so that derive neither marks it nor counts it
/// found again. Run before the folds, the pass would clear the colour work's
/// mark and count it found again in the derive that then deletes it.
#[test]
fn a_missing_work_folded_away_on_its_return_is_not_counted_found_again() {
    let tree = crate::tests::tempdir("derive-missing-folded");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let colour = two_works_before_the_fold(&tree, &conn);
    let root = scan::roots(&conn)
        .expect("roots")
        .into_iter()
        .next()
        .expect("the fixture's directory");
    let here = tree.path().join("Series - Full Color");
    let outside = crate::tests::tempdir("derive-missing-folded-away");
    let away = outside.path().join("Series - Full Color");
    std::fs::rename(&here, &away).expect("the colour directory goes");
    scan::scan_root(&conn, &root).expect("scans without it");
    let gone = derive::derive(&conn).expect("derives without it");
    let marked = |id: i64| -> Option<bool> {
        use rusqlite::OptionalExtension as _;
        conn.query_row(
            "SELECT missing_since IS NOT NULL FROM work WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .expect("reads the mark")
    };
    assert_eq!(
        (gone.marked_missing, marked(colour)),
        (1, Some(true)),
        "the fixture: the colour work is marked"
    );
    std::fs::rename(&away, &here).expect("the colour directory returns");
    write_the_fold_rule(&conn);
    scan::scan_root(&conn, &root).expect("scans with it again");

    let back = derive::derive(&conn).expect("derives with it and the rule");

    assert_eq!(
        (back.emptied_works, marked(colour)),
        (1, None),
        "the fixture: the colour work is folded away"
    );
    assert_eq!(
        (back.marked_missing, back.found_again),
        (0, 0),
        "a work folded away is neither marked nor found again"
    );
}

/// Part D's central claim: a volume whose file goes is marked missing rather
/// than deleted, and when the same bytes come back the mark clears on the
/// same work, so the reading on it survives the gap.
#[test]
fn a_volume_whose_file_goes_is_marked_missing_and_returns_with_its_work_position_and_note() {
    let (dir, paths) = a_series_of("derive-missing-rejoin", 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    let (id, slug) = volume_work(&conn, 2);
    crate::state::put_progress(
        &conn,
        id,
        1,
        &crate::Progress {
            edition: "raw".into(),
            slot: 2,
        },
    )
    .expect("a position");
    seed_notes(&conn, id, 1, "raw", &["n1"]);
    let bytes = std::fs::read(&paths[1]).expect("reads volume 2");

    std::fs::remove_file(&paths[1]).expect("volume 2 goes");
    scan::scan_root(&conn, &root).expect("scans without it");
    derive::derive(&conn).expect("derives without it");
    assert_eq!(
        (marked(&conn, 1), marked(&conn, 2)),
        (false, true),
        "volume 2 is marked, volume 1 is not"
    );
    std::fs::write(&paths[1], &bytes).expect("volume 2 returns, the same bytes");
    scan::scan_root(&conn, &root).expect("scans with it again");
    derive::derive(&conn).expect("derives with it again");

    assert_eq!(
        (marked(&conn, 1), marked(&conn, 2)),
        (false, false),
        "volume 2's mark is cleared"
    );
    assert_eq!(volume_work(&conn, 2), (id, slug), "the same work");
    let positions: Vec<(String, u32)> = crate::state::progress(&conn, id, 1)
        .expect("reads the positions")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect();
    assert_eq!(positions, [("raw".to_string(), 2)], "its position");
    let notes: Vec<String> = crate::state::notes(&conn, id, 1)
        .expect("reads the notes")
        .notes
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(notes, ["n1"], "its note");
}

/// Every work's id with whether it is marked missing, by id.
fn marks(conn: &Connection) -> Vec<(i64, bool)> {
    conn.prepare("SELECT id, missing_since IS NOT NULL FROM work ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the marks")
}

/// No scan is the authority for a work whose member was imported (it has no
/// `member_root` row, O54), nor for a work with no member at all, so neither
/// is marked. A work a directory held, whose digest no entry there has, is
/// marked beside them.
#[test]
fn an_imported_work_and_a_work_with_no_member_are_never_marked() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("a directory");
    seed_work(&conn, 1, "scanned", &[("raw", "/r/a.cbz", "aaa")]);
    seed_work_without_root(&conn, 2, "imported", &[("raw", "/x/b.cbz", "bbb")]);
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (3, 'empty', 'empty', 'Empty', 'rtl', 'raw')",
        [],
    )
    .expect("a work with no edition");

    let answer = derive::mark_missing(&conn);

    assert_eq!(
        (answer, marks(&conn)),
        (Ok((1, 0)), vec![(1, true), (2, false), (3, false)]),
        "the scanned work is marked; the imported one and the empty one are not"
    );
}

/// One member imported is enough: a work with a member under a directory and
/// a member under none (O249's leftover, or a directory removed between a
/// derive's reads and its writes) is never marked, though no directory holds
/// either. A work whose every member is under a directory is marked beside
/// it.
#[test]
fn a_work_with_one_member_under_no_directory_is_never_marked() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("a directory");
    seed_work(
        &conn,
        1,
        "mixed",
        &[("raw", "/r/a.cbz", "aaa"), ("en", "/x/b.cbz", "bbb")],
    );
    conn.execute(
        "UPDATE member SET root_id = NULL WHERE path = '/x/b.cbz'",
        [],
    )
    .expect("one member under no directory");
    seed_work(&conn, 2, "scanned", &[("raw", "/r/c.cbz", "ccc")]);

    let answer = derive::mark_missing(&conn);

    assert_eq!(
        (answer, marks(&conn)),
        (Ok((1, 0)), vec![(1, false), (2, true)]),
        "the mixed work is not marked; the all-scanned one is"
    );
}

/// A `scan_entry` of `sha256` at `rel_path` under directory `root`.
fn held_under(conn: &Connection, root: i64, rel_path: &str, sha256: &str) {
    conn.execute(
        "INSERT INTO scan_entry (root_id, rel_path, kind, size, mtime, sha256)
         VALUES (?1, ?2, 'file', 1, 0, ?3)",
        rusqlite::params![root, rel_path, sha256],
    )
    .expect("a scan entry");
}

/// Decision 2: a member is held only by a directory its `member_root` rows
/// name. Its bytes under another directory do not hold its work, which is
/// marked; a work whose bytes are under its own directory is not.
#[test]
fn a_digest_held_only_under_a_directory_the_member_is_not_under_holds_nothing() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, '/r'), (2, '/s')",
        [],
    )
    .expect("two directories");
    seed_work(&conn, 1, "elsewhere", &[("raw", "/r/a.cbz", "aaa")]);
    seed_work(&conn, 2, "held", &[("raw", "/r/b.cbz", "bbb")]);
    held_under(&conn, 2, "a.cbz", "aaa");
    held_under(&conn, 1, "b.cbz", "bbb");

    let answer = derive::mark_missing(&conn);

    assert_eq!(
        (answer, marks(&conn)),
        (Ok((1, 0)), vec![(1, true), (2, false)]),
        "the work whose bytes are only under the other directory is marked; \
         the one under its own is not"
    );
}

/// The owner's ruling "Path counts too" (O91): an entry at a member's path,
/// under a directory the member is under, holds it whatever its digest, so
/// work 1, its file replaced in place, is not marked. Work 2 is: its second
/// `member_root` row names `/s`, whose entry `b.cbz` is `/s/b.cbz`, not the
/// member's path, though both directories' paths are as long. Work 3 is too:
/// the entry at its path is `/r/n`'s, a directory it is not under. Each entry
/// has no page count, as the walk leaves a file whose pages it does not count,
/// and work 1's still holds it.
#[test]
fn an_entry_at_a_members_path_holds_it_whatever_its_digest() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, '/r'), (2, '/s'), (3, '/r/n')",
        [],
    )
    .expect("three directories");
    seed_work(&conn, 1, "replaced", &[("raw", "/r/a.cbz", "aaa")]);
    seed_work(&conn, 2, "beside", &[("raw", "/r/b.cbz", "bbb")]);
    conn.execute(
        "INSERT INTO member_root (member_id, root_id)
         SELECT id, 2 FROM member WHERE path = '/r/b.cbz'",
        [],
    )
    .expect("work 2's member is under `/s` too");
    seed_work(&conn, 3, "nested", &[("raw", "/r/n/c.cbz", "ccc")]);
    held_under(&conn, 1, "a.cbz", "new bytes");
    held_under(&conn, 2, "b.cbz", "other bytes");
    held_under(&conn, 3, "c.cbz", "more bytes");

    let answer = derive::mark_missing(&conn);

    assert_eq!(
        (answer, marks(&conn)),
        (Ok((2, 0)), vec![(1, false), (2, true), (3, true)]),
        "the work with an entry at its path is not marked; the one whose entry is \
         at a path as long under another directory, and the one whose path's \
         entry is under a directory it is not under, are"
    );
}

/// The entry must be at the member's path exactly: `a/b.cbz.bak`, which
/// starts with it, holds nothing at `/r/a/b.cbz`, nor does `a/bb.cbz`, which
/// ends with its name, at `/s/a/b.cbz`, nor `d.cbz`, which `/r/x/d.cbz` ends
/// with, so works 1, 2 and 4 are marked. Work 3, beside work 1 with an entry
/// at its own path, is not.
#[test]
fn an_entry_at_a_near_miss_of_a_members_path_holds_nothing() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, '/r'), (2, '/s')",
        [],
    )
    .expect("two directories");
    seed_work(&conn, 1, "longer", &[("raw", "/r/a/b.cbz", "aaa")]);
    seed_work_at(
        &conn,
        2,
        "ends-alike",
        &[("raw", "/s/a/b.cbz", "bbb")],
        Some(2),
    );
    seed_work(&conn, 3, "exact", &[("raw", "/r/a/c.cbz", "ccc")]);
    seed_work(&conn, 4, "shorter", &[("raw", "/r/x/d.cbz", "ddd")]);
    held_under(&conn, 1, "a/b.cbz.bak", "other bytes");
    held_under(&conn, 2, "a/bb.cbz", "more bytes");
    held_under(&conn, 1, "a/c.cbz", "new bytes");
    held_under(&conn, 1, "d.cbz", "still more bytes");

    let answer = derive::mark_missing(&conn);

    assert_eq!(
        (answer, marks(&conn)),
        (
            Ok((3, 0)),
            vec![(1, true), (2, true), (3, false), (4, true)]
        ),
        "the works whose entries only start or end like their paths, or whose \
         path only ends with its entry's, are marked; the one with an entry at \
         its path is not"
    );
}

/// "Path counts too" (O91) asks for something to read at a member's path: a
/// folder volume emptied in place leaves its directory, whose entry the walk
/// keeps and finds empty, and that entry holds nothing, so volume 1 is marked.
/// Volume 2, whose pages are still there, is not.
#[test]
fn a_folder_volume_emptied_in_place_is_marked_missing() {
    let dir = crate::tests::tempdir("derive-missing-emptied-folder");
    let volume = dir.path().join("Series").join("Series - v01");
    write_pages(&volume, 3);
    write_pages(&dir.path().join("Series").join("Series - v02"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    let before = (marked(&conn, 1), marked(&conn, 2));
    for page in std::fs::read_dir(&volume).expect("lists volume 1") {
        std::fs::remove_file(page.expect("a page").path()).expect("a page goes");
    }
    // A directory's entry is re-read only when its `mtime` moves, which the
    // walk compares to the second; see
    // `a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress`.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    std::fs::File::open(&volume)
        .expect("open the emptied folder")
        .set_times(std::fs::FileTimes::new().set_modified(later))
        .expect("stamp it as changed");

    scan::scan_root(&conn, &root).expect("rescans");
    derive::derive(&conn).expect("derives");

    let pages: Option<i64> = conn
        .query_row(
            "SELECT pages FROM scan_entry WHERE root_id = ?1 AND rel_path = 'Series/Series - v01'",
            [root.id],
            |r| r.get(0),
        )
        .expect("the walk keeps the emptied folder's entry");
    assert_eq!(
        (before, pages, marked(&conn, 1), marked(&conn, 2)),
        ((false, false), Some(0), true, false),
        "both present before; then the emptied folder's entry is kept with no \
         pages, volume 1 is marked and volume 2 is not"
    );
}

/// A scan that hides two works and one that brings one back say so, each
/// count in its own field.
#[test]
fn a_derive_counts_the_works_it_marks_and_the_works_it_finds_again() {
    let (dir, paths) = a_series_of("derive-missing-counts", 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    let bytes = std::fs::read(&paths[1]).expect("reads volume 2");

    std::fs::remove_file(&paths[1]).expect("volume 2 goes");
    std::fs::remove_file(&paths[2]).expect("volume 3 goes");
    scan::scan_root(&conn, &root).expect("scans without them");
    let hidden = derive::derive(&conn).expect("derives without them");
    std::fs::write(&paths[1], &bytes).expect("volume 2 returns");
    scan::scan_root(&conn, &root).expect("scans with it again");
    let found = derive::derive(&conn).expect("derives with it again");

    assert_eq!(
        (hidden.marked_missing, hidden.found_again),
        (2, 0),
        "two works marked, none found again"
    );
    assert_eq!(
        (found.marked_missing, found.found_again),
        (0, 1),
        "none marked, one found again"
    );
}

/// A pass that changes no mark writes no `work` row: each `UPDATE` would
/// otherwise re-index the title and bump the structure counter of every
/// work it matched.
#[test]
fn a_pass_that_changes_no_mark_leaves_the_structure_counter_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("a directory");
    seed_work(&conn, 1, "marked", &[("raw", "/r/a.cbz", "aaa")]);
    conn.execute(
        "UPDATE work SET missing_since = '2026-11-01 00:00:00' WHERE id = 1",
        [],
    )
    .expect("marked already");
    seed_work_without_root(&conn, 2, "imported", &[("raw", "/x/b.cbz", "bbb")]);
    let before = crate::structure::version(&conn).expect("reads the counter");

    let answer = derive::mark_missing(&conn);

    let since: String = conn
        .query_row("SELECT missing_since FROM work WHERE id = 1", [], |r| {
            r.get(0)
        })
        .expect("reads the mark");
    assert_eq!(
        (
            answer,
            crate::structure::version(&conn).expect("reads the counter"),
            since.as_str()
        ),
        (Ok((0, 0)), before, "2026-11-01 00:00:00"),
        "nothing changes: the answer, the structure counter and the mark's date"
    );
}

/// O272 over a store holding a marked work, as a real store keeps one for
/// good: a derive over unchanged files and rules takes no write transaction
/// and keeps the mark and its date. A read that took any marked work for a
/// move would have every quiet scan's derive take the write lock a page
/// turn's save waits for.
#[test]
fn a_derive_over_unchanged_files_with_a_work_marked_commits_nothing() {
    let (dir, paths) = a_series_of("derive-quiet-while-marked", 2);
    let store = crate::tests::tempdir("derive-quiet-while-marked-db");
    let library = Library::open(&store.path().join("library.db")).expect("open");
    let mut conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    std::fs::remove_file(&paths[1]).expect("volume 2 goes");
    scan::scan_root(&conn, &root).expect("scans without it");
    let gone = derive::derive(&conn).expect("derives without it");
    assert_eq!(
        (gone.marked_missing, marked(&conn, 2)),
        (1, true),
        "the fixture: volume 2's work is marked"
    );
    scan::scan_root(&conn, &root).expect("scans again, nothing changed");
    let since = |conn: &Connection| -> String {
        conn.query_row("SELECT missing_since FROM work WHERE volume = 2", [], |r| {
            r.get(0)
        })
        .expect("reads the mark's date")
    };
    let before = since(&conn);

    let (commits, quiet) = commits_during(&mut conn, derive::derive);
    let quiet = quiet.expect("a derive over nothing changed");

    assert_eq!(
        (
            commits,
            quiet.marked_missing,
            quiet.found_again,
            marked(&conn, 2),
            since(&conn)
        ),
        (0, 0, 0, true, before),
        "no commit, nothing marked or found again, the mark and its date kept"
    );
}

/// The mark and the clear commit as one transaction, so a scan another
/// process commits between them cannot have one work marked and cleared in
/// one pass. Counted with no transaction open, where each `UPDATE` would
/// otherwise commit on its own.
#[test]
fn the_mark_and_the_clear_commit_as_one_transaction() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let library = Library::open_in_memory().expect("open");
    let mut conn = library.conn();
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("a directory");
    seed_work(&conn, 1, "gone", &[("raw", "/r/a.cbz", "aaa")]);
    seed_work(&conn, 2, "back", &[("raw", "/r/b.cbz", "bbb")]);
    conn.execute(
        "UPDATE work SET missing_since = '2026-11-01 00:00:00' WHERE id = 2",
        [],
    )
    .expect("marked already");
    held_under(&conn, 1, "b.cbz", "bbb");
    let commits = std::sync::Arc::new(AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&commits);
    conn.commit_hook(Some(move || {
        counted.fetch_add(1, Ordering::Relaxed);
        false
    }))
    .expect("counts the commits");

    let answer = derive::mark_missing(&conn);
    conn.commit_hook(None::<fn() -> bool>)
        .expect("stops counting");

    assert_eq!(
        (answer, marks(&conn), commits.load(Ordering::Relaxed)),
        (Ok((1, 1)), vec![(1, true), (2, false)], 1),
        "one work marked and one cleared, in one commit"
    );
}

/// `Foo/Foo v0{n}.cbz` and `Bar/Bar v0{n}.cbz` for volumes 1 and 2, under one
/// directory: two series of two volumes each.
fn two_series_of_two(label: &str) -> crate::tests::TempDir {
    let dir = crate::tests::tempdir(label);
    for series in ["Foo", "Bar"] {
        for n in 1..=2 {
            let name = format!("{series} v0{n}.cbz");
            write_archive(&dir.path().join(series).join(name), 2 + n);
        }
    }
    dir
}

/// Every work's series and volume, its id, and whether it is marked missing.
fn marks_by_series(conn: &Connection) -> Vec<(String, i64, i64, bool)> {
    conn.prepare(
        "SELECT s.sort_title, CAST(w.volume AS INTEGER), w.id, w.missing_since IS NOT NULL
           FROM work w JOIN series s ON s.id = w.series_id
          ORDER BY s.sort_title, w.volume",
    )
    .and_then(|mut stmt| {
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect()
    })
    .expect("reads the marks")
}

/// [`marks_by_series`] with the marks of series `marked` set, the ids kept.
fn with_marked(works: &[(String, i64, i64, bool)], marked: &str) -> Vec<(String, i64, i64, bool)> {
    works
        .iter()
        .map(|(series, volume, id, _)| (series.clone(), *volume, *id, series == marked))
        .collect()
}

/// O48's scenario, a partial mount: one series' directory leaves the tree and
/// the other stays, so the scan is applied rather than refused, and the derive
/// marks that series' works and no other. Put back, a scan and a derive clear
/// the marks on the same works.
#[test]
fn a_series_directory_that_leaves_the_tree_is_marked_and_returns_on_the_same_works() {
    let dir = two_series_of_two("derive-missing-mount");
    let away = crate::tests::tempdir("derive-missing-mount-away");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    let present = marks_by_series(&conn);
    assert_eq!(
        present
            .iter()
            .map(|(series, volume, _, marked)| (series.as_str(), *volume, *marked))
            .collect::<Vec<_>>(),
        [
            ("Bar", 1, false),
            ("Bar", 2, false),
            ("Foo", 1, false),
            ("Foo", 2, false)
        ],
        "the fixture: four works, none marked"
    );

    std::fs::rename(dir.path().join("Bar"), away.path().join("Bar"))
        .expect("Bar's directory leaves the tree");
    let unmounted = scan::scan_root(&conn, &root).expect("scans without it");
    derive::derive(&conn).expect("derives without it");
    let hidden = marks_by_series(&conn);
    std::fs::rename(away.path().join("Bar"), dir.path().join("Bar"))
        .expect("Bar's directory is back");
    scan::scan_root(&conn, &root).expect("scans with it again");
    derive::derive(&conn).expect("derives with it again");

    assert_eq!(unmounted.refused, None, "the scan was applied");
    assert_eq!(
        hidden,
        with_marked(&present, "Bar"),
        "Bar's works are marked, Foo's are not"
    );
    assert_eq!(
        marks_by_series(&conn),
        present,
        "every mark is cleared, on the same works"
    );
}

/// Decision 8: the mark is per work. Bar's `raw` edition is under library A
/// and its `en` edition under library B; when A's `Bar/` leaves the tree, Bar
/// stays present through B, its `raw` edition kept beside, though one of its
/// members is held nowhere. Qux, under A alone and gone the same way, is
/// marked beside it; Baz keeps A's scan from reading as an empty mount.
#[test]
fn a_work_whose_files_leave_one_library_stays_present_through_the_other() {
    let a_dir = crate::tests::tempdir("derive-missing-per-work-a");
    let b_dir = crate::tests::tempdir("derive-missing-per-work-b");
    let away = crate::tests::tempdir("derive-missing-per-work-away");
    for (dir, series, pages) in [
        (&a_dir, "Bar", 3),
        (&a_dir, "Baz", 4),
        (&a_dir, "Qux", 5),
        (&b_dir, "Bar", 7),
    ] {
        let name = format!("{series} v01.cbz");
        write_archive(&dir.path().join(series).join(name), pages);
    }
    let library = libraries_a_and_b();
    let conn = library.conn();
    let a = root_in(&conn, a_dir.path(), "A", None);
    root_in(&conn, b_dir.path(), "B", Some("en"));
    derive::derive(&conn).expect("derives");
    let present = marks_by_series(&conn);
    assert_eq!(
        (
            present
                .iter()
                .map(|(series, volume, _, marked)| (series.as_str(), *volume, *marked))
                .collect::<Vec<_>>(),
            scalar(
                &conn,
                "SELECT count(*) FROM edition e JOIN work w ON w.id = e.work_id
                  JOIN series s ON s.id = w.series_id WHERE s.sort_title = 'Bar'"
            ),
        ),
        (
            vec![("Bar", 1, false), ("Baz", 1, false), ("Qux", 1, false)],
            2
        ),
        "the fixture: three works, none marked, Bar with an edition per library"
    );

    for series in ["Bar", "Qux"] {
        std::fs::rename(a_dir.path().join(series), away.path().join(series))
            .expect("the series' directory leaves A");
    }
    let unmounted = scan::scan_root(&conn, &a).expect("scans without them");
    let derived = derive::derive(&conn).expect("derives without them");

    assert_eq!(
        (unmounted.refused, derived.kept_editions),
        (None, 1),
        "the fixture: the scan was applied, and Bar's `raw` edition is kept"
    );
    assert_eq!(
        marks_by_series(&conn),
        with_marked(&present, "Qux"),
        "Qux is marked; Bar stays present through B, and Baz is untouched"
    );
}

/// O92: a pattern covering one series forgets its entries, and the derive
/// marks its works. Cleared, a scan and a derive clear the marks on the same
/// works.
#[test]
fn an_excluded_series_is_marked_and_returns_when_the_pattern_is_cleared() {
    let dir = two_series_of_two("derive-missing-exclude");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    let present = marks_by_series(&conn);
    let excluding = |globs: Option<&str>| {
        scan::update_root(
            &conn,
            root.id,
            &scan::RootPatch {
                exclude_globs: Some(globs.map(str::to_string)),
                ..Default::default()
            },
        )
        .expect("sets the pattern");
        scan::roots(&conn)
            .expect("lists")
            .into_iter()
            .find(|r| r.id == root.id)
            .expect("the directory")
    };

    let excluded = scan::scan_root(&conn, &excluding(Some("Bar/"))).expect("scans with it");
    derive::derive(&conn).expect("derives with it");
    let hidden = marks_by_series(&conn);
    scan::scan_root(&conn, &excluding(None)).expect("scans without it");
    derive::derive(&conn).expect("derives without it");

    assert_eq!(
        excluded.forgotten_by_exclusion, 3,
        "Bar's directory and its two volumes"
    );
    assert_eq!(
        hidden,
        with_marked(&present, "Bar"),
        "Bar's works are marked, Foo's are not"
    );
    assert_eq!(
        marks_by_series(&conn),
        present,
        "every mark is cleared, on the same works"
    );
}

/// The pass runs under `Documents::Skip` as under `Reconcile`: a rolled-back
/// preview predicts the mark as it predicts everything else.
#[test]
fn a_derive_that_skips_documents_marks_a_work_whose_file_went() {
    let (dir, paths) = a_series_of("derive-missing-skip", 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    std::fs::remove_file(&paths[1]).expect("volume 2 goes");
    scan::scan_root(&conn, &root).expect("scans without it");

    let report = derive::derive_with(&conn, derive::Documents::Skip).expect("derives");

    assert_eq!(
        (report.marked_missing, marked(&conn, 1), marked(&conn, 2)),
        (1, false, true),
        "volume 2's work is marked and counted; volume 1's is not"
    );
}

/// Three archives no derive has hashed, in two series: two chapters of
/// `Alpha` volume 1, which make one work, and `Beta` volume 1. Their paths are
/// in the order a derive reads them.
fn three_undigested_archives(label: &str) -> (crate::tests::TempDir, Vec<std::path::PathBuf>) {
    let dir = crate::tests::tempdir(label);
    let paths: Vec<_> = [
        ("Alpha/Alpha - v01 - c001.cbz", 2),
        ("Alpha/Alpha - v01 - c002.cbz", 3),
        ("Beta/Beta v01.cbz", 4),
    ]
    .into_iter()
    .map(|(rel_path, pages)| {
        let path = dir.path().join(rel_path);
        write_archive(&path, pages);
        path
    })
    .collect();
    (dir, paths)
}

/// Each file's size on disk, which is what the walk records.
fn sizes_of(paths: &[std::path::PathBuf]) -> Vec<u64> {
    paths
        .iter()
        .map(|path| std::fs::metadata(path).expect("stats").len())
        .collect()
}

/// The reports `watch` was given for `step`, in order.
fn reports_at(watch: &Recording, step: Step) -> Vec<Report> {
    watch
        .reports()
        .into_iter()
        .filter(|(at, _, _)| *at == step)
        .collect()
}

#[test]
fn a_watched_derive_counts_bytes_hashed_of_bytes_to_hash_then_works_written() {
    let (tree, paths) = three_undigested_archives("derive-watched-counts");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, tree.path(), None);
    let [a, b, c] = sizes_of(&paths)[..] else {
        panic!("three sizes")
    };
    let total = a + b + c;

    let first = Recording::default();
    let outcome = derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &first)
        .expect("first derive");
    let second = Recording::default();
    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &second)
        .expect("second derive");

    assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
    assert_eq!(
        first.reports(),
        vec![
            (Step::Hashing, 0, None),
            (Step::Hashing, 0, Some(total)),
            (Step::Hashing, a, Some(total)),
            (Step::Hashing, a + b, Some(total)),
            (Step::Hashing, total, Some(total)),
            (Step::Writing, 0, Some(2)),
            (Step::Writing, 1, Some(2)),
            (Step::Writing, 2, Some(2)),
            (Step::Finishing, 0, None),
        ],
        "hashing with no total while folders are sized, then the bytes hashed before each \
         digest and after the last, then the works written before each work and after the \
         last, then the tail"
    );
    assert_eq!(
        reports_at(&second, Step::Hashing),
        vec![(Step::Hashing, 0, None), (Step::Hashing, 0, Some(0))],
        "every archive is digested, so there is nothing to hash"
    );
}

#[test]
fn the_bytes_to_hash_leave_out_what_classify_does_not_hash() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let held = crate::tests::tempdir("derive-watched-held");
    write_archive(&held.path().join("Delta").join("Delta v01.cbz"), 5);
    scanned_root(&conn, held.path(), None);
    derive::derive(&conn).expect("digests Delta's archive");
    let (tree, paths) = three_undigested_archives("derive-watched-left-out");
    write_archive(&tree.path().join("Kaguya").join("Kaguya - c001.cbz"), 2);
    let pageless = tree.path().join("Gamma").join("Gamma v01");
    std::fs::create_dir_all(&pageless).expect("mkdir");
    std::fs::write(pageless.join("notes.txt"), "no pages here").expect("writes");
    scanned_root(&conn, tree.path(), None);
    let [a, b, c] = sizes_of(&paths)[..] else {
        panic!("three sizes")
    };
    let total = a + b + c;
    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM scan_entry WHERE size > 0 AND (
                 rel_path = 'Kaguya/Kaguya - c001.cbz'
                 OR (rel_path = 'Delta/Delta v01.cbz' AND sha256 IS NOT NULL))"
        ),
        2,
        "the fixture: the chapter-only and the digested entry have bytes a total could count"
    );
    let watch = Recording::default();

    let outcome =
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");

    let Outcome::Done(report) = outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(
        (report.unattached_chapter_dirs, report.unreadable.no_pages),
        (1, 1),
        "the fixture: one chapter-only entry and one directory of no pages"
    );
    assert_eq!(
        reports_at(&watch, Step::Hashing),
        vec![
            (Step::Hashing, 0, None),
            (Step::Hashing, 0, Some(total)),
            (Step::Hashing, a, Some(total)),
            (Step::Hashing, a + b, Some(total)),
            (Step::Hashing, total, Some(total)),
        ],
        "only the three undigested archives are to hash, each reported before its digest"
    );
}

#[test]
fn a_derive_cancelled_after_one_digest_keeps_it_and_writes_no_work() {
    let (tree, _) = three_undigested_archives("derive-cancel-hashing");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, tree.path(), None);
    let watch = Recording::cancelling(|(step, hashed, _)| step == Step::Hashing && hashed > 0);

    let outcome =
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");

    assert_eq!(
        (
            scalar(
                &conn,
                "SELECT count(*) FROM scan_entry WHERE sha256 IS NOT NULL"
            ),
            scalar(&conn, "SELECT count(*) FROM work"),
        ),
        (1, 0),
        "the first archive's digest is kept, and no work is written"
    );
    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Hashing)),
        "{outcome:?}"
    );
}

#[test]
fn a_derive_cancelled_after_one_work_keeps_that_work() {
    let (tree, _) = three_undigested_archives("derive-cancel-writing");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, tree.path(), None);
    let watch = Recording::cancelling(|(step, written, _)| step == Step::Writing && written > 0);

    let outcome =
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");

    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Writing)),
        "{outcome:?}"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        1,
        "the first work stays, and the second is not written"
    );
}

/// Under a directory of its own, a work whose one file went after it was
/// derived; beside it, two volumes and an unattached chapter directory, walked
/// and not derived. The tail would mark the first work missing and raise a
/// proposal for the chapters.
#[test]
fn a_derive_cancelled_before_the_tail_raises_and_marks_nothing() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let away = crate::tests::tempdir("derive-cancel-tail-away");
    let file = away.path().join("Other").join("Other v01.cbz");
    write_archive(&file, 2);
    let other = scanned_root(&conn, away.path(), None);
    derive::derive(&conn).expect("derives Other's work");
    std::fs::remove_file(&file).expect("its file goes");
    scan::scan_root(&conn, &other).expect("walks without it");
    let tree = crate::tests::tempdir("derive-cancel-tail");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 2);
    write_archive(&tree.path().join("Alpha").join("Alpha v02.cbz"), 3);
    write_pages(&tree.path().join("Kaguya").join("Kaguya - c001"), 1);
    write_pages(&tree.path().join("Kaguya").join("Kaguya - c002"), 1);
    scanned_root(&conn, tree.path(), None);
    let marked = "SELECT count(*) FROM work WHERE missing_since IS NOT NULL";
    let proposals = "SELECT count(*) FROM proposal";
    assert_eq!(
        (scalar(&conn, proposals), scalar(&conn, marked)),
        (0, 0),
        "the fixture: nothing raised and nothing marked yet"
    );
    let watch =
        Recording::cancelling(|(step, written, of)| step == Step::Writing && Some(written) == of);

    let outcome =
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");
    let cancelled = (
        scalar(&conn, "SELECT count(*) FROM work"),
        scalar(&conn, proposals),
        scalar(&conn, marked),
    );
    let report = derive::derive(&conn).expect("derives to the end");

    assert_eq!(
        cancelled,
        (3, 0, 0),
        "Other's work and both of Alpha's stand; nothing is raised and nothing is marked"
    );
    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Writing)),
        "{outcome:?}"
    );
    assert!(
        report.raised >= 1,
        "uncancelled, the same store raises the chapters' proposal: {report:?}"
    );
    assert_eq!(scalar(&conn, marked), 1, "and marks Other's work missing");
}

/// A folder member's bytes to hash are those of the page files its digest
/// reads: not the directory's own size, and not a file that is no page.
#[test]
fn a_folder_members_bytes_to_hash_are_its_page_files_sizes() {
    let tree = crate::tests::tempdir("derive-watched-folder");
    let folder = tree.path().join("Gamma").join("Gamma v01");
    std::fs::create_dir_all(&folder).expect("mkdir");
    for (name, size) in [("001.jpg", 1000), ("002.jpg", 2000), ("003.jpg", 3000)] {
        std::fs::write(folder.join(name), vec![b'p'; size]).expect("writes a page");
    }
    std::fs::write(folder.join("notes.txt"), vec![b'n'; 500]).expect("writes a file");
    let archive = tree.path().join("Gamma").join("Gamma v02.cbz");
    write_archive(&archive, 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, tree.path(), None);
    let [archived] = sizes_of(&[archive])[..] else {
        panic!("one size")
    };
    let total = 6000 + archived;
    let watch = Recording::default();

    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");

    assert_eq!(
        reports_at(&watch, Step::Hashing),
        vec![
            (Step::Hashing, 0, None),
            (Step::Hashing, 0, Some(total)),
            (Step::Hashing, 6000, Some(total)),
            (Step::Hashing, total, Some(total)),
        ],
        "the folder's three pages, then the archive, of the two together"
    );
}

/// Sizing a folder lists it and stats each page, so a cancel is read before
/// each folder is sized: the derive stops with no total reported and nothing
/// hashed.
#[test]
fn a_derive_cancelled_while_folders_are_sized_reports_no_total_and_hashes_nothing() {
    let tree = crate::tests::tempdir("derive-cancel-sizing");
    write_pages(&tree.path().join("Gamma").join("Gamma v01"), 2);
    write_pages(&tree.path().join("Gamma").join("Gamma v02"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, tree.path(), None);
    let watch = Recording::cancelling(|(step, _, of)| step == Step::Hashing && of.is_none());

    let outcome =
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");

    assert_eq!(
        (
            watch.reports(),
            scalar(
                &conn,
                "SELECT count(*) FROM scan_entry WHERE sha256 IS NOT NULL"
            ),
        ),
        (vec![(Step::Hashing, 0, None)], 0),
        "hashing begins with no total, and stops before a folder is sized or hashed"
    );
    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Hashing)),
        "{outcome:?}"
    );
}

// --- A paused directory is frozen (O320) ---------------------------------------

/// What a freeze must leave as it stands, each sorted: the works as
/// `(id, content_key)`, the members as `(work_id, edition key, ordinal, path,
/// sha256, pages)`, and the `member_root` rows as `(member path, root_id)`.
#[derive(Debug, PartialEq)]
pub(super) struct Standing {
    works: Vec<(i64, String)>,
    members: Vec<(i64, String, i64, String, String, i64)>,
    member_roots: Vec<(String, i64)>,
}

pub(super) fn standing(conn: &Connection) -> Standing {
    let mut works: Vec<(i64, String)> = conn
        .prepare("SELECT id, content_key FROM work")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the works");
    let mut members: Vec<(i64, String, i64, String, String, i64)> = conn
        .prepare(
            "SELECT e.work_id, e.key, m.ordinal, m.path, m.sha256, m.pages
               FROM member m JOIN edition e ON e.id = m.edition_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect()
        })
        .expect("reads the members");
    let mut member_roots: Vec<(String, i64)> = conn
        .prepare(
            "SELECT m.path, mr.root_id FROM member_root mr JOIN member m ON m.id = mr.member_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the member_root rows");
    works.sort();
    members.sort();
    member_roots.sort();
    Standing {
        works,
        members,
        member_roots,
    }
}

/// `Alpha/Alpha - v01 - c001.cbz` (2 pages) and `c002.cbz` (3 pages) under
/// `dir`: one work of two members once derived.
fn two_file_volume(dir: &std::path::Path) {
    write_archive(&dir.join("Alpha").join("Alpha - v01 - c001.cbz"), 2);
    write_archive(&dir.join("Alpha").join("Alpha - v01 - c002.cbz"), 3);
}

pub(super) fn pause(conn: &Connection, root: &scan::Root) {
    scan::pause(conn, std::slice::from_ref(root)).expect("pauses the directory");
}

/// The digest `root_id`'s row at `rel_path` holds.
fn digest_at(conn: &Connection, root_id: i64, rel_path: &str) -> Option<String> {
    conn.query_row(
        "SELECT sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root_id, rel_path],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("{rel_path} has a row: {e}"))
}

pub(super) fn digests_under(conn: &Connection, root_id: i64) -> i64 {
    scalar(
        conn,
        &format!(
            "SELECT count(*) FROM scan_entry WHERE root_id = {root_id} AND sha256 IS NOT NULL"
        ),
    )
}

fn member_roots_under(conn: &Connection, root_id: i64) -> i64 {
    scalar(
        conn,
        &format!("SELECT count(*) FROM member_root WHERE root_id = {root_id}"),
    )
}

/// Every member's path, in path order.
pub(super) fn member_paths(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT path FROM member ORDER BY path")
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .expect("reads the members")
}

#[test]
fn a_derive_of_another_directory_hashes_none_of_a_paused_one() {
    let (tree, _) = three_undigested_archives("frozen-not-hashed");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    let cancel = Recording::cancelling(|(step, done, _)| step == Step::Hashing && done > 0);
    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &cancel).expect("derives");
    pause(&conn, &r);
    let other = crate::tests::tempdir("frozen-not-hashed-other");
    let delta = other.path().join("Delta").join("Delta v01.cbz");
    write_archive(&delta, 5);
    scanned_root(&conn, other.path(), None);
    let [s] = sizes_of(&[delta])[..] else {
        panic!("one size")
    };
    assert_eq!(
        digests_under(&conn, r.id),
        1,
        "the fixture: the cancelled derive kept one of R's digests"
    );
    let watch = Recording::default();

    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &watch).expect("derives");

    assert_eq!(
        reports_at(&watch, Step::Hashing),
        vec![
            (Step::Hashing, 0, None),
            (Step::Hashing, 0, Some(s)),
            (Step::Hashing, s, Some(s)),
        ],
        "only the other directory's archive is to hash"
    );
    assert_eq!(
        (digests_under(&conn, r.id), member_roots_under(&conn, r.id)),
        (1, 0),
        "R's digest stays and none of its works is written"
    );
}

#[test]
fn a_paused_directorys_works_stand_as_they_were_after_a_cancelled_rescan() {
    for hashed in [false, true] {
        let (s_dir, r_dir) = crate::tests::a_copy_of("Foo v01.cbz");
        two_file_volume(r_dir.path());
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        scanned_root(&conn, s_dir.path(), None);
        let r = scanned_root(&conn, r_dir.path(), None);
        derive::derive(&conn).expect("derives");
        let before = standing(&conn);
        assert_eq!(
            (
                before.works.len(),
                before.members.len(),
                before.member_roots.len()
            ),
            (2, 3, 4),
            "the fixture: Alpha's work of two members, and Foo's one member held by both"
        );
        let c002 = "Alpha/Alpha - v01 - c002.cbz";
        let old = digest_at(&conn, r.id, c002);
        write_archive(&r_dir.path().join(c002), 4);
        write_archive(&r_dir.path().join("Foo").join("Foo v01.cbz"), 5);
        scan::scan_root(&conn, &r).expect("walks R again");
        assert_eq!(
            scan::tests::held_reading(&conn, r.id, c002).0,
            old,
            "the fixture: the walk holds c002's old digest"
        );
        if hashed {
            let cancel = Recording::cancelling(|(step, _, _)| step == Step::Writing);
            derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &cancel)
                .expect("derives");
            assert!(
                digest_at(&conn, r.id, c002).is_some_and(|new| Some(&new) != old.as_ref()),
                "the fixture: the cancelled derive hashed c002's new bytes"
            );
        }
        pause(&conn, &r);

        derive::derive(&conn).expect("derives");

        assert_eq!(standing(&conn), before, "hashed: {hashed}");
    }
}

#[test]
fn a_held_reading_survives_a_second_cancelled_scan() {
    let tree = crate::tests::tempdir("frozen-second-cancel");
    two_file_volume(tree.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let before = standing(&conn);
    let c002 = "Alpha/Alpha - v01 - c002.cbz";
    write_archive(&tree.path().join(c002), 4);
    scan::scan_root(&conn, &r).expect("walks R again");
    let cancel = Recording::cancelling(|(step, _, _)| step == Step::Writing);
    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &cancel).expect("derives");
    assert!(
        digest_at(&conn, r.id, c002).is_some(),
        "the fixture: the first cancelled derive hashed c002"
    );
    pause(&conn, &r);
    scan::unpause(&conn, std::slice::from_ref(&r)).expect("Scan is pressed");
    write_archive(&tree.path().join(c002), 5);
    scan::scan_root(&conn, &r).expect("walks R a third time");
    pause(&conn, &r);

    derive::derive(&conn).expect("derives");

    assert_eq!(
        standing(&conn),
        before,
        "c002's member holds its first digest"
    );
}

/// A count taken again (the reader changed) that differs from the one a
/// paused directory's works were written from is held with the digest, as a
/// changed file's reading is, so the frozen derive keeps the member's pages
/// (O130, O320).
#[test]
fn a_paused_directory_keeps_the_count_its_works_hold_when_a_file_is_counted_again() {
    let tree = crate::tests::tempdir("frozen-recount");
    two_file_volume(tree.path());
    let c002 = tree.path().join(C002);
    {
        let file = std::fs::File::create(&c002).expect("creates c002");
        let mut zip = zip::ZipWriter::new(file);
        for name in ["000.jpg", "001.jpg", "002.jxl"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .expect("starts an entry");
            std::io::Write::write_all(&mut zip, format!("not really: c002 {name}").as_bytes())
                .expect("writes an entry");
        }
        zip.finish().expect("finishes c002");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, r.id, C002);
    // Counted before signatures existed: c002 at its two JPEG pages, and its
    // work written from that count.
    assert_eq!(
        (
            conn.execute(
                "UPDATE scan_entry SET pages = 2, counted_by = NULL WHERE rel_path = ?1",
                [C002],
            ),
            conn.execute(
                "UPDATE member SET pages = 2 WHERE path = ?1",
                [r.path.join(C002).to_string_lossy()],
            ),
        ),
        (Ok(1), Ok(1)),
        "the fixture: c002 counted before signatures existed"
    );
    let before = standing(&conn);
    pause(&conn, &r);
    scan::scan_root(&conn, &r).expect("walks R again");
    let counted: (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT pages, sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![r.id, C002],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("reads c002's row");
    assert_eq!(
        (counted, digest.is_some()),
        ((Some(3), digest), true),
        "the fixture: c002 is counted again at three pages and keeps its digest"
    );

    derive::derive(&conn).expect("derives");

    assert_eq!(
        standing(&conn),
        before,
        "c002's member keeps the two pages its work was written from"
    );
}

#[test]
fn a_cancel_while_writing_leaves_exactly_the_works_written() {
    let (tree, paths) = three_undigested_archives("frozen-cancel-writing");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    let cancel = Recording::cancelling(|(step, done, _)| step == Step::Writing && done > 0);
    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &cancel).expect("derives");
    assert_eq!(
        (
            digests_under(&conn, r.id),
            scalar(&conn, "SELECT count(*) FROM work")
        ),
        (3, 1),
        "the fixture: every archive hashed, and one work written"
    );
    pause(&conn, &r);

    derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            scalar(&conn, "SELECT count(*) FROM work"),
            member_paths(&conn)
        ),
        (
            1,
            vec![
                paths[0].to_string_lossy().into_owned(),
                paths[1].to_string_lossy().into_owned()
            ]
        ),
        "Alpha's work stands, and Beta's is not written"
    );
}

#[test]
fn a_paused_directory_keeps_its_chapter_only_entries() {
    let tree = crate::tests::tempdir("frozen-chapter-only");
    write_archive(&tree.path().join("Kaguya").join("Kaguya - c001.cbz"), 2);
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    let first = derive::derive(&conn).expect("derives");
    assert_eq!(
        (first.series, first.unattached_chapter_dirs),
        (2, 1),
        "the fixture: Kaguya's unattached chapter names a series beside Alpha's"
    );
    pause(&conn, &r);

    let second = derive::derive(&conn).expect("derives");

    assert_eq!(
        (second.series, second.unattached_chapter_dirs),
        (first.series, first.unattached_chapter_dirs),
        "the chapter still names its series and is still unattached"
    );
}

/// A folder volume added to a paused directory waits, and its chapter
/// folders, chapter-only entries that would otherwise pass as they are, wait
/// with it rather than stand under no claim.
#[test]
fn a_paused_directorys_waiting_folder_volume_keeps_its_chapters_with_it() {
    let tree = crate::tests::tempdir("frozen-waiting-volume");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let volume = tree.path().join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        write_pages(&volume.join(chapter), 2);
    }
    scan::scan_root(&conn, &r).expect("walks R again");
    pause(&conn, &r);
    let (engine, _) = crate::rules::Engine::load(&conn).expect("loads the rules");
    assert!(
        ["c001", "c002", "c003"].iter().all(|chapter| {
            let resolved = engine.resolve(&r, &format!("Series/Series - v01/{chapter}"));
            resolved.volume.is_none() && resolved.chapter.is_some()
        }),
        "the fixture: each chapter folder resolves a chapter and no volume"
    );
    assert_eq!(
        derive::unattached_chapter_dirs(&conn).map(|dirs| dirs.len()),
        Ok(0),
        "the fixture: read as the walk saved them, the volume claims its chapters"
    );

    let report = derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            report.unattached_chapter_dirs,
            report.series,
            scalar(&conn, "SELECT count(*) FROM series")
        ),
        (0, 1, 1),
        "the volume's chapters wait with it: none counted, and no series named for them"
    );
}

/// Only chapters wait with a waiting entry, and only with one that waits: the
/// archives a folder now claims still make their work while it waits, and a
/// held volume still suppresses its chapters.
#[test]
fn only_chapters_beneath_a_waiting_folder_wait_with_it() {
    let tree = crate::tests::tempdir("frozen-beneath-waiting");
    let series = tree.path().join("Series");
    let v01 = series.join("Series - v01");
    write_archive(&v01.join("Series - v01 - c001.cbz"), 2);
    write_archive(&v01.join("Series - v01 - c002.cbz"), 3);
    for chapter in ["c001", "c002"] {
        write_pages(&series.join("Series - v02").join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    let first = derive::derive(&conn).expect("derives");
    assert_eq!(
        (first.works, first.members, first.suppressed),
        (2, 3, 2),
        "the fixture: v01's two archives, v02's folder, and v02's chapters suppressed"
    );
    write_pages(&v01, 1);
    scan::scan_root(&conn, &r).expect("walks R again");
    pause(&conn, &r);
    assert_eq!(
        conn.query_row(
            "SELECT pages, sha256, held_sha256 FROM scan_entry
              WHERE root_id = ?1 AND rel_path = 'Series/Series - v01'",
            [r.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        Ok((Some(1), None::<String>, None::<String>)),
        "the fixture: a page makes v01 a folder volume, and nothing is held for it"
    );

    let second = derive::derive(&conn).expect("derives");

    assert_eq!(
        (second.works, second.members, second.suppressed),
        (first.works, first.members, first.suppressed),
        "v01's archives still make its work, and v02 still suppresses its chapters"
    );
}

#[test]
fn digest_held_members_passes_over_a_paused_directory() {
    let tree = crate::tests::tempdir("frozen-held-members");
    let alpha = "Alpha/Alpha v01.cbz";
    write_archive(&tree.path().join(alpha), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    write_archive(&tree.path().join(alpha), 4);
    scan::scan_root(&conn, &r).expect("walks R again");
    pause(&conn, &r);
    assert!(
        digest_at(&conn, r.id, alpha).is_none()
            && scan::tests::held_reading(&conn, r.id, alpha).0.is_some(),
        "the fixture: the walk cleared the digest and holds the old one"
    );

    derive::derive(&conn).expect("derives");

    assert_eq!(
        digest_at(&conn, r.id, alpha),
        None,
        "the changed file is not hashed"
    );
}

#[test]
fn a_fresh_directory_is_read_as_the_walk_saved_it_though_paused() {
    let (tree, _) = three_undigested_archives("frozen-fresh");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    pause(&conn, &r);
    assert!(
        scan::paused(&conn).expect("reads").contains(&r.id),
        "the fixture: R is paused"
    );
    let works = |fresh: &[i64]| {
        derive::derive_watched(
            &conn,
            derive::Documents::Reconcile,
            fresh,
            &Recording::default(),
        )
        .expect("derives");
        scalar(&conn, "SELECT count(*) FROM work")
    };

    let counts = (works(&[]), works(&[r.id]));

    assert_eq!(counts, (0, 2), "frozen, then read fresh");
}

/// `Series - v01 - c001.cbz` (2 pages) and the folder `Series - v01 - c002`
/// (3 pages), one work of two members, with c001 stamped: the stamp leaves the
/// folder alone, as it leaves every member that is not a zip.
#[test]
fn a_stamped_file_in_a_paused_directory_keeps_its_member() {
    let tree = crate::tests::tempdir("frozen-stamped");
    write_archive(
        &tree.path().join("Series").join("Series - v01 - c001.cbz"),
        2,
    );
    write_pages(&tree.path().join("Series").join("Series - v01 - c002"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let unstamped = standing(&conn);
    assert_eq!(
        (unstamped.works.len(), unstamped.members.len()),
        (1, 2),
        "the fixture: one work of two members"
    );
    let work_id = unstamped.works[0].0;
    let planned = crate::stamp::plan(&conn, work_id).expect("plans");
    assert_eq!(
        planned.len(),
        1,
        "the fixture: the stamp rewrites c001 alone"
    );
    crate::stamp::apply(&conn, work_id, 1, &planned).expect("stamps");
    let stamped = standing(&conn).members;
    pause(&conn, &r);

    derive::derive(&conn).expect("derives");

    assert_eq!(
        standing(&conn).members,
        stamped,
        "c001's member keeps its new digest, and c002's its own"
    );
}

/// A paused directory's rows that make no structure, an unreadable file, an
/// archive that would not open and a folder of no pages, are still counted:
/// they hash and write nothing, and a report that dropped them would read as
/// the directory having lost them.
#[test]
fn a_paused_directory_still_counts_what_makes_no_structure() {
    let tree = crate::tests::tempdir("frozen-unreadable");
    write_archive(&tree.path().join("Alpha").join("Alpha v01.cbz"), 3);
    std::fs::write(tree.path().join("Alpha").join("notes.pdf"), b"%PDF-1.7\n").expect("writes");
    std::fs::write(
        tree.path().join("Alpha").join("Alpha v02.cbz"),
        b"PK\x03\x04",
    )
    .expect("four bytes of zip signature and nothing behind them");
    let pageless = tree.path().join("Gamma").join("Gamma v01");
    std::fs::create_dir_all(&pageless).expect("mkdir");
    std::fs::write(pageless.join("notes.txt"), "no pages here").expect("writes");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    let first = derive::derive(&conn).expect("derives").unreadable;
    assert_eq!(
        (
            first.by_format.get("pdf"),
            first.uncounted.len(),
            first.no_pages
        ),
        (Some(&1), 1, 1),
        "the fixture: the PDF, the archive that would not open and the folder of no pages"
    );
    pause(&conn, &r);

    let second = derive::derive(&conn).expect("derives").unreadable;

    assert_eq!(
        second.by_format, first.by_format,
        "the PDF and the file of no known format"
    );
    assert_eq!(
        second.uncounted, first.uncounted,
        "the archive that would not open"
    );
    assert_eq!(second.no_pages, first.no_pages, "the folder of no pages");
}

// --- The held reading is dropped when its work is written (O320) --------------

const C001: &str = "Alpha/Alpha - v01 - c001.cbz";
const C002: &str = "Alpha/Alpha - v01 - c002.cbz";
const C003: &str = "Alpha/Alpha - v01 - c003.cbz";
const BETA: &str = "Beta/Beta v01.cbz";
const FOO: &str = "Foo/Foo v01.cbz";
const BAR: &str = "Bar/Bar v01.cbz";

/// The digest of the reading `root_id`'s row at `rel_path` holds.
fn held_of(conn: &Connection, root_id: i64, rel_path: &str) -> Option<String> {
    scan::tests::held_reading(conn, root_id, rel_path).0
}

/// The digest of the member at `rel_path` under `root`, if one is there.
fn member_digest_at(conn: &Connection, root: &scan::Root, rel_path: &str) -> Option<String> {
    use rusqlite::OptionalExtension as _;
    conn.query_row(
        "SELECT sha256 FROM member WHERE path = ?1",
        [root.path.join(rel_path).to_string_lossy().into_owned()],
        |r| r.get(0),
    )
    .optional()
    .expect("reads the member")
}

/// Each member of `work_id` as `(path, sha256)`, by edition and then in page
/// order.
fn members_by_ordinal(conn: &Connection, work_id: i64) -> Vec<(String, String)> {
    conn.prepare(
        "SELECT m.path, m.sha256 FROM member m JOIN edition e ON e.id = m.edition_id
          WHERE e.work_id = ?1 ORDER BY e.key, m.ordinal",
    )
    .and_then(|mut stmt| {
        stmt.query_map([work_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    })
    .expect("reads the work's members")
}

/// A pressed Scan reads its directory fresh and writes its works from the
/// current readings, so it drops the readings they replace: a later freeze
/// would otherwise read the work by one it no longer holds.
#[test]
fn a_pressed_scan_drops_the_held_readings_of_what_it_writes() {
    let tree = crate::tests::tempdir("frozen-pressed-drops");
    two_file_volume(tree.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let a2 = digest_at(&conn, r.id, C002);
    write_archive(&tree.path().join(C002), 4);
    scan::scan_root(&conn, &r).expect("walks R again");
    pause(&conn, &r);
    assert_eq!(
        (held_of(&conn, r.id, C002), a2.is_some()),
        (a2.clone(), true),
        "the fixture: the walk holds c002's old digest"
    );

    derive::derive_watched(
        &conn,
        derive::Documents::Reconcile,
        &[r.id],
        &Recording::default(),
    )
    .expect("derives");

    let entry = digest_at(&conn, r.id, C002);
    assert_eq!(
        (member_digest_at(&conn, &r, C002) == entry, entry == a2),
        (true, false),
        "c002's member and entry hold one digest, and it is not the old one"
    );
    assert_eq!(
        scan::tests::held_reading(&conn, r.id, C002),
        (None, None, None, None),
        "c002's held reading is dropped"
    );
}

/// The drop is in the write, so a derive cancelled before it writes keeps the
/// reading the works still hold, which a freeze after it reads them by.
#[test]
fn a_derive_cancelled_before_writing_keeps_the_held_reading() {
    let tree = crate::tests::tempdir("frozen-cancel-keeps");
    two_file_volume(tree.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let a2 = digest_at(&conn, r.id, C002);
    write_archive(&tree.path().join(C002), 4);
    scan::scan_root(&conn, &r).expect("walks R again");
    assert_eq!(
        (held_of(&conn, r.id, C002), a2.is_some()),
        (a2.clone(), true),
        "the fixture: the walk holds c002's old digest"
    );
    let cancel = Recording::cancelling(|(step, _, _)| step == Step::Writing);

    let outcome =
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &cancel).expect("derives");

    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Writing))
            && digest_at(&conn, r.id, C002).is_some(),
        "the fixture: the derive hashed c002 and was cancelled before writing"
    );
    assert_eq!(
        held_of(&conn, r.id, C002),
        a2,
        "c002 still holds its old digest"
    );
}

/// Per work: a cancel between two works drops the readings of the one written
/// and keeps those of the one not, which a freeze after it still reads it by.
#[test]
fn a_cancel_between_works_drops_only_the_written_works_readings() {
    let tree = crate::tests::tempdir("frozen-cancel-between");
    two_file_volume(tree.path());
    write_archive(&tree.path().join(BETA), 4);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let (a2, b1) = (digest_at(&conn, r.id, C002), digest_at(&conn, r.id, BETA));
    write_archive(&tree.path().join(C002), 5);
    write_archive(&tree.path().join(BETA), 6);
    scan::scan_root(&conn, &r).expect("walks R again");
    assert_eq!(
        (
            held_of(&conn, r.id, C002),
            held_of(&conn, r.id, BETA),
            a2.is_some() && b1.is_some(),
            scalar(&conn, "SELECT count(*) FROM work"),
        ),
        (a2, b1.clone(), true, 2),
        "the fixture: Alpha's and Beta's works, and the walk holds both old digests"
    );
    let cancel = Recording::cancelling(|(step, done, _)| step == Step::Writing && done > 0);

    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &cancel).expect("derives");

    assert_eq!(
        (held_of(&conn, r.id, C002), held_of(&conn, r.id, BETA)),
        (None, b1),
        "Alpha's work is written and drops c002's; Beta's is not and keeps its own"
    );
}

/// A copy's entry is read for the work as the primary's is, so writing the
/// work drops the copy's held reading too.
#[test]
fn writing_a_copy_drops_its_entrys_held_reading() {
    let (s_dir, r_dir) = a_copy_of("Foo v01.cbz");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let s = scanned_root(&conn, s_dir.path(), None);
    let r = scanned_root(&conn, r_dir.path(), None);
    derive::derive(&conn).expect("derives");
    for dir in [&s_dir, &r_dir] {
        write_archive_labelled(&dir.path().join(FOO), 4, "jpg", "same");
    }
    scan::scan_root(&conn, &s).expect("walks S again");
    scan::scan_root(&conn, &r).expect("walks R again");
    let old = held_of(&conn, s.id, FOO);
    assert_eq!(
        (
            old.is_some(),
            held_of(&conn, r.id, FOO) == old,
            member_root_ids(&conn)
        ),
        (true, true, vec![s.id, r.id]),
        "the fixture: one member held by both, and both walks hold its old digest"
    );

    derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            held_of(&conn, s.id, FOO),
            held_of(&conn, r.id, FOO),
            member_root_ids(&conn)
        ),
        (None, None, vec![s.id, r.id]),
        "both held readings are dropped, R's as the member's copy"
    );
}

/// A paused directory's entries keep their held readings when another
/// directory's file joins their work: the derive that writes it is frozen for
/// them, and read them by those readings.
#[test]
fn a_paused_directorys_files_keep_their_held_reading_when_another_directory_changes_their_work() {
    let r_dir = crate::tests::tempdir("frozen-joined-r");
    two_file_volume(r_dir.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, r_dir.path(), None);
    derive::derive(&conn).expect("derives");
    let (a1, a2) = (digest_at(&conn, r.id, C001), digest_at(&conn, r.id, C002));
    write_archive(&r_dir.path().join(C002), 4);
    scan::scan_root(&conn, &r).expect("walks R again");
    pause(&conn, &r);
    let s_dir = crate::tests::tempdir("frozen-joined-s");
    write_archive(&s_dir.path().join(C003), 4);
    let s = scanned_root(&conn, s_dir.path(), None);
    let work_id = scalar(&conn, "SELECT id FROM work");
    assert_eq!(
        (
            held_of(&conn, r.id, C002),
            a1.is_some() && a2.is_some(),
            scalar(&conn, "SELECT count(*) FROM work"),
        ),
        (a2.clone(), true, 1),
        "the fixture: Alpha's one work, and the walk holds c002's old digest"
    );

    derive::derive(&conn).expect("a derive elsewhere");

    let at = |root: &scan::Root, rel_path: &str, sha256: Option<String>| {
        (
            root.path.join(rel_path).to_string_lossy().into_owned(),
            sha256.expect("a digest"),
        )
    };
    let c3 = digest_at(&conn, s.id, C003);
    assert_eq!(
        members_by_ordinal(&conn, work_id),
        vec![at(&r, C001, a1), at(&r, C002, a2.clone()), at(&s, C003, c3)],
        "S's file joins R's work, which holds c002 at its old digest"
    );
    assert_eq!(
        held_of(&conn, r.id, C002),
        a2,
        "c002 keeps its held reading"
    );
}

/// A file gone from a paused directory is marked missing as it would be
/// unpaused (no removal is frozen), and when its bytes come back and Scan is
/// pressed it rejoins its work, read fresh, whose reading stays.
#[test]
fn a_file_gone_from_a_paused_directory_is_marked_missing_and_returns_with_its_work_position_and_note()
 {
    let (dir, paths) = a_series_of("frozen-missing-rejoin", 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_in(&conn, dir.path(), "Manga", None);
    derive::derive(&conn).expect("derives");
    let (id, slug) = volume_work(&conn, 2);
    crate::state::put_progress(
        &conn,
        id,
        1,
        &crate::Progress {
            edition: "raw".into(),
            slot: 2,
        },
    )
    .expect("a position");
    seed_notes(&conn, id, 1, "raw", &["n1"]);
    let bytes = std::fs::read(&paths[1]).expect("reads volume 2");
    std::fs::remove_file(&paths[1]).expect("volume 2 goes");
    scan::scan_root(&conn, &root).expect("scans without it");
    pause(&conn, &root);
    assert!(
        scan::paused(&conn).expect("reads").contains(&root.id),
        "the fixture: the directory is paused"
    );

    derive::derive(&conn).expect("a derive elsewhere");

    assert_eq!(
        (marked(&conn, 1), marked(&conn, 2)),
        (false, true),
        "volume 2 is marked, volume 1 is not"
    );
    std::fs::write(&paths[1], &bytes).expect("volume 2 returns, the same bytes");
    scan::unpause(&conn, std::slice::from_ref(&root)).expect("Scan is pressed");
    scan::scan_root(&conn, &root).expect("scans with it again");
    derive::derive_watched(
        &conn,
        derive::Documents::Reconcile,
        &[root.id],
        &Recording::default(),
    )
    .expect("derives");
    let positions: Vec<(String, u32)> = crate::state::progress(&conn, id, 1)
        .expect("reads the positions")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect();
    let notes: Vec<String> = crate::state::notes(&conn, id, 1)
        .expect("reads the notes")
        .notes
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(
        (
            (marked(&conn, 1), marked(&conn, 2)),
            volume_work(&conn, 2),
            digest_at(&conn, root.id, "Series/Series v02.cbz").is_some(),
            positions,
            notes,
        ),
        (
            (false, false),
            (id, slug),
            true,
            vec![("raw".to_string(), 2)],
            vec!["n1".to_string()],
        ),
        "volume 2's mark is cleared on the same work, read fresh, with its position and note"
    );
}

/// A copy at another path than its primary's has a row of its own: writing
/// the work drops that row's held reading, and while the copy's directory is
/// frozen it keeps it.
#[test]
fn a_copy_at_another_path_has_its_own_rows_held_reading_dropped_or_kept() {
    let x_dir = crate::tests::tempdir("frozen-copy-path-x");
    let r_dir = crate::tests::tempdir("frozen-copy-path-r");
    write_archive_labelled(&x_dir.path().join(FOO), 3, "jpg", "first");
    write_archive_labelled(&r_dir.path().join(BAR), 3, "jpg", "first");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let x = scanned_root(&conn, x_dir.path(), None);
    let r = scanned_root(&conn, r_dir.path(), None);
    derive::derive(&conn).expect("derives");
    write_archive_labelled(&x_dir.path().join(FOO), 4, "jpg", "second");
    write_archive_labelled(&r_dir.path().join(BAR), 4, "jpg", "second");
    scan::scan_root(&conn, &x).expect("walks X again");
    scan::scan_root(&conn, &r).expect("walks R again");
    assert_eq!(
        (
            scalar(&conn, "SELECT count(*) FROM member"),
            member_root_ids(&conn),
            held_of(&conn, r.id, BAR).is_some(),
        ),
        (1, vec![x.id, r.id], true),
        "the fixture: R's Bar is a copy of X's Foo, and the walk holds its old digest"
    );

    derive::derive(&conn).expect("derives");

    assert_eq!(
        (held_of(&conn, r.id, BAR), member_root_ids(&conn)),
        (None, vec![x.id, r.id]),
        "writing the work drops the held reading of R's copy at its own path"
    );

    let second = digest_at(&conn, r.id, BAR);
    write_archive_labelled(&r_dir.path().join(BAR), 5, "jpg", "third");
    scan::scan_root(&conn, &r).expect("walks R a third time");
    pause(&conn, &r);
    write_archive(&x_dir.path().join("Foo").join("Foo - v01 - c002.cbz"), 2);
    scan::scan_root(&conn, &x).expect("walks X a third time");
    assert_eq!(
        (held_of(&conn, r.id, BAR), second.is_some()),
        (second.clone(), true),
        "the fixture: the walk holds Bar's second digest"
    );

    derive::derive(&conn).expect("a derive elsewhere");

    assert_eq!(
        (held_of(&conn, r.id, BAR), member_root_ids(&conn)),
        (second, vec![x.id, x.id, r.id]),
        "X's new file rewrites the work, and R's frozen copy keeps its held reading"
    );
}

/// A file whose mtime moved and whose bytes did not leaves its work as it
/// stands, so no write drops its held reading; the digest found equal to it
/// does, since the held reading is then the current one.
#[test]
fn a_pressed_scan_over_a_file_whose_bytes_did_not_change_drops_its_held_reading() {
    let tree = crate::tests::tempdir("frozen-touched");
    two_file_volume(tree.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let a2 = digest_at(&conn, r.id, C002);
    // The walk compares `mtime` to the second.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    std::fs::File::open(tree.path().join(C002))
        .expect("open c002")
        .set_times(std::fs::FileTimes::new().set_modified(later))
        .expect("stamp it as changed");
    scan::scan_root(&conn, &r).expect("walks R again");
    pause(&conn, &r);
    assert_eq!(
        (
            digest_at(&conn, r.id, C002),
            held_of(&conn, r.id, C002),
            a2.is_some()
        ),
        (None, a2, true),
        "the fixture: the walk read c002 as changed and holds its digest"
    );
    scan::unpause(&conn, std::slice::from_ref(&r)).expect("Scan is pressed");
    scan::scan_root(&conn, &r).expect("walks R a third time");

    derive::derive_watched(
        &conn,
        derive::Documents::Reconcile,
        &[r.id],
        &Recording::default(),
    )
    .expect("derives");

    assert_eq!(
        scan::tests::held_reading(&conn, r.id, C002),
        (None, None, None, None),
        "c002 holds no reading"
    );
}

/// Copies at one path under two directories hold a reading each, on a row of
/// their own. S's goes when its work is written from new bytes, or when its
/// bytes are found equal to it; R's, while R is paused, is what R is read by
/// and stays.
#[test]
fn a_paused_copy_at_the_same_path_keeps_its_held_reading_when_the_others_goes() {
    for rewritten in [false, true] {
        let (s_dir, r_dir) = crate::tests::a_copy_of("Foo v01.cbz");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let s = scanned_root(&conn, s_dir.path(), None);
        let r = scanned_root(&conn, r_dir.path(), None);
        derive::derive(&conn).expect("derives");
        let old = digest_at(&conn, r.id, FOO);
        // The walk compares `mtime` to the second.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
        for dir in [s_dir.path(), r_dir.path()] {
            if rewritten {
                write_archive_labelled(&dir.join(FOO), 4, "jpg", "second");
            } else {
                std::fs::File::open(dir.join(FOO))
                    .expect("open Foo")
                    .set_times(std::fs::FileTimes::new().set_modified(later))
                    .expect("stamp it as changed");
            }
        }
        scan::scan_root(&conn, &s).expect("walks S again");
        scan::scan_root(&conn, &r).expect("walks R again");
        pause(&conn, &r);
        assert_eq!(
            (
                held_of(&conn, s.id, FOO),
                held_of(&conn, r.id, FOO),
                old.is_some(),
                member_root_ids(&conn),
            ),
            (old.clone(), old.clone(), true, vec![s.id, r.id]),
            "the fixture: one member under both, and each walk holds its old digest"
        );

        derive::derive(&conn).expect("derives");

        assert_eq!(
            (held_of(&conn, s.id, FOO), held_of(&conn, r.id, FOO)),
            (None, old),
            "rewritten: {rewritten}: S's row drops its held reading and R's keeps its own"
        );
    }
}

/// Migration 27 holds a reading on an entry of a directory that is not paused
/// too, where its digest differs from its member's; the first derive that
/// writes the work from the entry's own reading drops it.
#[test]
fn a_reading_migration_27_held_is_dropped_by_the_derive_that_writes_its_work() {
    let tree = crate::tests::tempdir("frozen-backfilled");
    two_file_volume(tree.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let a2 = digest_at(&conn, r.id, C002);
    write_archive(&tree.path().join(C002), 4);
    conn.execute_batch(&format!(
        "ALTER TABLE member ADD COLUMN format TEXT;
         ALTER TABLE edition DROP COLUMN joined_by;
         DROP TABLE match_lookup;
         DROP TABLE content_match;
         DROP TABLE fingerprint;
         ALTER TABLE library DROP COLUMN match_by_content;
         ALTER TABLE scan_entry DROP COLUMN held_sha256;
         ALTER TABLE scan_entry DROP COLUMN held_format;
         ALTER TABLE scan_entry DROP COLUMN held_pages;
         ALTER TABLE scan_entry DROP COLUMN held_title;
         ALTER TABLE scan_entry DROP COLUMN counted_by;
         ALTER TABLE root DROP COLUMN scan_held_at;
         UPDATE meta SET value = '26' WHERE key = 'schema_version';
         UPDATE scan_entry SET sha256 = NULL, pages = 4 WHERE rel_path = '{C002}';"
    ))
    .expect("puts the store back to schema 26, c002 changed and walked");
    crate::schema::migrate(&conn).expect("migrates from 26");
    assert_eq!(
        (
            held_of(&conn, r.id, C002),
            a2.is_some(),
            scan::paused(&conn).expect("reads").is_empty(),
        ),
        (a2, true, true),
        "the fixture: the upgrade holds c002's member digest, and nothing is paused"
    );

    derive::derive(&conn).expect("derives");

    assert_eq!(
        scan::tests::held_reading(&conn, r.id, C002),
        (None, None, None, None),
        "the derive that writes c002's new bytes drops its held reading"
    );
}

/// A changed file whose new bytes, hashed before the cancel, are another
/// member's under the same directory is read by its held reading, not as that
/// member's bytes, so its own member stays.
#[test]
fn a_paused_file_hashed_as_another_members_bytes_is_read_by_its_held_reading() {
    let tree = crate::tests::tempdir("frozen-held-first");
    two_file_volume(tree.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let before = standing(&conn);
    let (a1, a2) = (digest_at(&conn, r.id, C001), digest_at(&conn, r.id, C002));
    std::fs::copy(tree.path().join(C001), tree.path().join(C002)).expect("copies c001 over c002");
    scan::scan_root(&conn, &r).expect("walks R again");
    let cancel = Recording::cancelling(|(step, _, _)| step == Step::Writing);
    derive::derive_watched(&conn, derive::Documents::Reconcile, &[], &cancel).expect("derives");
    pause(&conn, &r);
    assert_eq!(
        (
            digest_at(&conn, r.id, C002),
            held_of(&conn, r.id, C002),
            a1.is_some() && a1 != a2
        ),
        (a1.clone(), a2.clone(), true),
        "the fixture: c002 was hashed as c001's bytes, and holds its own old digest"
    );

    derive::derive(&conn).expect("derives");

    assert_eq!(
        standing(&conn),
        before,
        "c002's member keeps its own digest"
    );
}

/// A held reading no work holds is not read: a row whose work was written
/// without it keeps a stale one (spec §11.3), and a freeze that read it would
/// bring back a member its work no longer has.
#[test]
fn a_paused_directory_reads_no_held_reading_its_works_no_longer_hold() {
    let tree = crate::tests::tempdir("frozen-stale-held");
    two_file_volume(tree.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    let a2 = digest_at(&conn, r.id, C002);
    write_archive(&tree.path().join(C002), 0);
    scan::scan_root(&conn, &r).expect("walks R again");
    derive::derive(&conn).expect("derives");
    let before = standing(&conn);
    assert_eq!(
        (
            held_of(&conn, r.id, C002),
            member_digest_at(&conn, &r, C002),
            a2.is_some()
        ),
        (a2, None, true),
        "the fixture: c002's work was written without it, and it keeps its old reading held"
    );
    pause(&conn, &r);

    derive::derive(&conn).expect("derives");

    assert_eq!(standing(&conn), before, "c002 makes no member");
}

// --- A nested directory takes over the outer rows' readings (O320) -----------

/// The reading `root_id`'s row at `rel_path` holds now, as `(sha256, format,
/// pages, title)`.
fn reading_at(
    conn: &Connection,
    root_id: i64,
    rel_path: &str,
) -> (Option<String>, Option<String>, Option<i64>, Option<String>) {
    conn.query_row(
        "SELECT sha256, format, pages, title FROM scan_entry
          WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root_id, rel_path],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap_or_else(|e| panic!("{rel_path} has a row: {e}"))
}

/// The nested walk records each file it takes over as a new row, and the row
/// takes the outer row's reading as the walk would have kept it: a file
/// unchanged since keeps its digest, one changed since holds the reading the
/// change clears, and one the outer walk already held keeps that.
#[test]
fn a_nested_directory_takes_over_each_rows_reading_as_the_walk_would_keep_it() {
    let tree = crate::tests::tempdir("take-over-reading");
    let kids = tree.path().join("kids");
    two_file_volume(&kids);
    write_archive(&kids.join(BETA), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let outer = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");
    write_archive(&kids.join(BETA), 5);
    scan::scan_root(&conn, &outer).expect("walks the outer directory again");
    let under = |rel_path: &str| format!("kids/{rel_path}");
    let c001 = reading_at(&conn, outer.id, &under(C001));
    let c002 = reading_at(&conn, outer.id, &under(C002));
    let beta = scan::tests::held_reading(&conn, outer.id, &under(BETA));
    assert_eq!(
        (
            c001.0.is_some(),
            c002.0.is_some(),
            beta.0.is_some(),
            digest_at(&conn, outer.id, &under(BETA))
        ),
        (true, true, true, None),
        "the fixture: c001 and c002 hashed, and Beta's digest held by the outer walk"
    );
    write_archive(&kids.join(C002), 4);

    let q = scanned_root(&conn, &kids, None);

    assert_eq!(
        (
            (digest_at(&conn, q.id, C001), held_of(&conn, q.id, C001)),
            (
                digest_at(&conn, q.id, C002),
                scan::tests::held_reading(&conn, q.id, C002)
            ),
            (
                digest_at(&conn, q.id, BETA),
                scan::tests::held_reading(&conn, q.id, BETA)
            ),
        ),
        ((c001.0, None), (None, c002), (None, beta)),
        "c001 keeps its digest, c002 holds the reading it changed from, Beta keeps its held one"
    );
}

/// A nested directory's first scan cancelled after hashing part of a volume it
/// took over leaves the volume whole, paused or not.
#[test]
fn a_nested_directorys_cancelled_first_scan_keeps_the_volume_it_took_over_whole() {
    for paused in [false, true] {
        let tree = crate::tests::tempdir("frozen-take-over");
        let kids = tree.path().join("kids");
        two_file_volume(&kids);
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let outer = scanned_root(&conn, tree.path(), None);
        derive::derive(&conn).expect("derives");
        // Touched, so Q's walk holds each digest rather than keeping it, and
        // Q's derive has both files to hash. The walk compares `mtime` to the
        // second.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
        for rel_path in [C001, C002] {
            std::fs::File::open(kids.join(rel_path))
                .expect("open the file")
                .set_times(std::fs::FileTimes::new().set_modified(later))
                .expect("stamp it as changed");
        }
        let q = scanned_root(&conn, &kids, None);
        assert_eq!(
            (
                member_paths(&conn).len(),
                member_roots_under(&conn, outer.id),
                member_roots_under(&conn, q.id)
            ),
            (2, 0, 2),
            "the fixture: one work of two members, taken over by Q"
        );
        let cancel = Recording::cancelling(|(step, done, _)| step == Step::Hashing && done > 0);
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[q.id], &cancel)
            .expect("derives");
        assert_eq!(
            (
                digest_at(&conn, q.id, C001).is_some(),
                digest_at(&conn, q.id, C002).is_some()
            ),
            (true, false),
            "the fixture: Q's first derive cancelled after hashing c001 alone"
        );
        let before = standing(&conn);
        if paused {
            pause(&conn, &q);
        }

        derive::derive(&conn).expect("a derive of the store");

        assert_eq!(standing(&conn), before, "paused: {paused}");
    }
}

/// A nested directory's first scan cancelled before it hashes a file it took
/// over, whose copy is in another directory, keeps the primary, paused or not:
/// the copy is not left the only reading, to become the primary at the Scan.
#[test]
fn a_nested_directorys_cancelled_first_scan_keeps_the_primary_it_took_over() {
    for paused in [false, true] {
        let outer = crate::tests::tempdir("frozen-take-over-outer");
        let kids = outer.path().join("kids");
        write_archive(&kids.join(FOO), 3);
        let other = crate::tests::tempdir("frozen-take-over-other");
        std::fs::create_dir_all(other.path().join("Foo")).expect("mkdir");
        std::fs::copy(kids.join(FOO), other.path().join(FOO)).expect("copies");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        scanned_root(&conn, outer.path(), None);
        scanned_root(&conn, other.path(), None);
        derive::derive(&conn).expect("derives");
        let q = scanned_root(&conn, &kids, None);
        assert_eq!(
            crate::tests::the_member(&conn).0,
            Some(q.id),
            "the fixture: Q took over the primary"
        );
        let cancel = Recording::cancelling(|(step, _, _)| step == Step::Hashing);
        derive::derive_watched(&conn, derive::Documents::Reconcile, &[q.id], &cancel)
            .expect("derives");
        if paused {
            pause(&conn, &q);
        }
        derive::derive(&conn).expect("a derive of the store");

        scan::unpause(&conn, std::slice::from_ref(&q)).expect("Scan is pressed");
        scan::scan_root(&conn, &q).expect("walks Q");
        derive::derive_watched(
            &conn,
            derive::Documents::Reconcile,
            &[q.id],
            &crate::progress::Unwatched,
        )
        .expect("derives");

        assert_eq!(
            crate::tests::the_member(&conn).0,
            Some(q.id),
            "paused: {paused}"
        );
    }
}

/// The units a fingerprint pass reads are the entries the derive claims: a
/// volume folder speaks for its chapter folders, each chapter file with no
/// volume speaks for itself, and a series folder or the root for nothing.
/// `Alpha/Extras` has a series and pages but defers to the volume inside it,
/// and `Beta/Beta v01` holds an archive and no page of its own: neither claims.
#[test]
fn claimed_units_are_the_entries_the_derive_claims() {
    let tree = crate::tests::tempdir("claimed-units");
    let volume = tree.path().join("Alpha").join("Alpha - v01");
    write_pages(&volume.join("c001"), 2);
    write_pages(&volume.join("c002"), 2);
    write_pages(
        &tree.path().join("Alpha").join("Extras").join("Alpha v02"),
        2,
    );
    write_archive(
        &tree
            .path()
            .join("Beta")
            .join("Beta v01")
            .join("Beta v01.cbz"),
        3,
    );
    write_archive(&tree.path().join("Kaguya").join("Kaguya - c001.cbz"), 2);
    write_archive(&tree.path().join("Kaguya").join("Kaguya - c002.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("derives");

    let claimed: BTreeMap<String, derive::Claimed> = derive::claimed_units(&conn)
        .expect("lists the claims")
        .into_iter()
        .map(|unit| (unit.rel_path.clone(), unit))
        .collect();

    let folder = claimed.get("Alpha/Alpha - v01").expect("the volume folder");
    assert_eq!(
        folder,
        &derive::Claimed {
            root_id: root.id,
            rel_path: "Alpha/Alpha - v01".to_string(),
            format: Some("dir".to_string()),
            pages: Some(4),
            sha256: digest_at(&conn, root.id, "Alpha/Alpha - v01"),
        },
        "as its row records it"
    );
    assert!(folder.sha256.is_some(), "with the digest the derive took");
    for chapter in ["Alpha/Alpha - v01/c001", "Alpha/Alpha - v01/c002"] {
        assert!(
            !claimed.contains_key(chapter),
            "{chapter}: its volume speaks for it"
        );
    }
    for chapter in ["Kaguya/Kaguya - c001.cbz", "Kaguya/Kaguya - c002.cbz"] {
        assert!(
            claimed.contains_key(chapter),
            "{chapter}: a chapter with no volume speaks for itself"
        );
    }
    for container in ["Alpha", "Beta", "Kaguya", ""] {
        assert!(
            !claimed.contains_key(container),
            "{container:?} speaks for nothing"
        );
    }
    assert!(
        !claimed.contains_key("Alpha/Extras"),
        "a container with a series defers to the volume inside it"
    );
    assert!(
        claimed.contains_key("Alpha/Extras/Alpha v02"),
        "which speaks for itself"
    );
    assert!(
        !claimed.contains_key("Beta/Beta v01"),
        "a volume folder with no page of its own claims nothing"
    );
    assert!(
        claimed.contains_key("Beta/Beta v01/Beta v01.cbz"),
        "the archive inside it speaks for itself"
    );
    assert_eq!(claimed.len(), 5, "and nothing else is claimed");
}

/// A folder counted anew gives its digest up as a changed folder does, so a
/// paused directory's derive reads it by the reading its works were written
/// from until Scan, and Scan takes the digest again (O320).
#[test]
fn a_paused_directory_holds_the_reading_of_a_folder_counted_anew() {
    use crate::scan::tests::{SHORT_FOLDER, counted_short_of_a_page};
    let tree = crate::tests::tempdir("frozen-counted-anew");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = counted_short_of_a_page(&conn, tree.path());
    let digest = digest_at(&conn, root.id, SHORT_FOLDER).expect("the derive took its digest");
    let before = standing(&conn);
    pause(&conn, &root);

    scan::scan_root(&conn, &root).expect("walks it again");
    assert_eq!(
        digest_at(&conn, root.id, SHORT_FOLDER),
        None,
        "the fixture: the walk gave the digest up"
    );
    derive::derive(&conn).expect("derives");

    assert_eq!(standing(&conn), before, "its work stands as it was written");
    assert_eq!(
        held_of(&conn, root.id, SHORT_FOLDER),
        Some(digest.clone()),
        "the frozen derive keeps the reading"
    );
    assert_eq!(
        digest_at(&conn, root.id, SHORT_FOLDER),
        None,
        "and hashes nothing"
    );

    scan::unpause(&conn, std::slice::from_ref(&root)).expect("unpauses");
    scan::scan_root(&conn, &root).expect("walks it for the pressed Scan");
    derive::derive(&conn).expect("derives");

    let taken = digest_at(&conn, root.id, SHORT_FOLDER).expect("Scan takes the digest");
    assert_ne!(taken, digest, "over the third page too");
    assert_eq!(
        conn.query_row("SELECT sha256, pages FROM member", [], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .expect("the one member"),
        (taken, 3),
        "the member follows"
    );
    assert_eq!(
        held_of(&conn, root.id, SHORT_FOLDER),
        None,
        "the reading is dropped"
    );
}

/// What a derive leaves of a folder volume with a position on it, after
/// `change`: the same work, how many works it reports re-keyed, the position's
/// slot, the member's pages and whether its digest moved.
fn a_folder_volume_after(
    tag: &str,
    change: impl FnOnce(&Connection, &scan::Root),
) -> (bool, usize, i64, i64, bool) {
    use crate::scan::tests::SHORT_FOLDER;
    let tree = crate::tests::tempdir(tag);
    write_pages(&tree.path().join(SHORT_FOLDER), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");
    let work: i64 = conn
        .query_row("SELECT id FROM work WHERE volume = 1.0", [], |r| r.get(0))
        .expect("work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 7)",
        [work],
    )
    .expect("progress");
    let digest = digest_at(&conn, root.id, SHORT_FOLDER);

    change(&conn, &root);
    scan::scan_root(&conn, &root).expect("rescan");
    let report = derive::derive(&conn).expect("second");

    let still: i64 = conn
        .query_row("SELECT id FROM work WHERE volume = 1.0", [], |r| r.get(0))
        .expect("one work");
    let slot: i64 = conn
        .query_row(
            "SELECT slot FROM progress WHERE work_id = ?1",
            [still],
            |r| r.get(0),
        )
        .expect("the position survived");
    let pages: i64 = conn
        .query_row("SELECT pages FROM member", [], |r| r.get(0))
        .expect("one member");
    (
        still == work,
        report.rekeyed,
        slot,
        pages,
        digest_at(&conn, root.id, SHORT_FOLDER) != digest,
    )
}

#[test]
fn a_folder_counted_anew_keeps_its_work_and_position_as_a_changed_folder_does() {
    use crate::scan::tests::{SHORT_FOLDER, holds_a_page_it_was_not_counted_with};
    let changed = a_folder_volume_after("derive-folder-changed", |_, root| {
        let dir = root.path.join(SHORT_FOLDER);
        std::fs::write(dir.join("002.jpg"), b"one more page").expect("write");
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
        std::fs::File::open(&dir)
            .expect("open the member directory")
            .set_times(std::fs::FileTimes::new().set_modified(later))
            .expect("stamp it as changed");
    });
    let counted = a_folder_volume_after("derive-folder-counted", |conn, root| {
        holds_a_page_it_was_not_counted_with(conn, root, SHORT_FOLDER);
    });

    assert_eq!(
        changed,
        (true, 1, 7, 3, true),
        "the fixture: a changed folder"
    );
    assert_eq!(counted, changed, "a folder counted anew goes the same way");
}
