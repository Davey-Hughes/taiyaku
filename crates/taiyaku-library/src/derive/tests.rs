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
fn a_derived_series_is_findable_by_title_so_a_document_does_not_mint_a_second_one() {
    // `series.sort_title` is not what anything looks a series up by:
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

    crate::migrate::record_absorbed(&conn, work_id, crate::migrate::LIBRARY, true)
        .expect("marks this work's library metadata as this library's own");
    conn.execute("UPDATE work SET volume = 4.5 WHERE id = ?1", [work_id])
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
