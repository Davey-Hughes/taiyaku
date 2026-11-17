use super::*;
use crate::scan::tests::write_pages;
use crate::{Library, Progress, derive, scan, state};

/// Two roots, one series, one volume in each — the shape spec C1 §5.1 is
/// designed around, and the only one where the filter can be seen working.
///
/// Both roots are directories of one library: a work gathers its editions
/// within a library, and a root of its own library would hold a work of its
/// own. **That library is numbered 70**, apart from the directories, so a
/// narrowing that read a library id as a directory id finds nothing rather
/// than the English directory that happens to share its number.
fn two_root_library() -> (
    Library,
    i64,
    i64,
    crate::tests::TempDir,
    crate::tests::TempDir,
) {
    let en = crate::tests::tempdir("browse-en");
    write_pages(&en.path().join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("browse-raw");
    write_pages(&raw.path().join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    let (en_id, raw_id) = {
        let conn = library.conn();
        conn.execute("INSERT INTO library (id, name) VALUES (70, 'Manga')", [])
            .expect("the library, numbered apart from its directories");
        let en_root =
            scan::add_root_in(&conn, en.path(), scan::InLibrary::Named("Manga")).expect("adds en");
        let raw_root = scan::add_root_in(&conn, raw.path(), scan::InLibrary::Named("Manga"))
            .expect("adds raw");
        // **Each root declares its own edition key**, or both default to
        // `raw` and derivation merges their members into ONE edition — which
        // is correct behaviour and makes this fixture unable to show a filter
        // hiding one edition and keeping another.
        conn.execute(
            "UPDATE root SET edition_key = 'en' WHERE id = ?1",
            [en_root.id],
        )
        .expect("names the English root's edition");
        conn.execute(
            "UPDATE root SET edition_key = 'raw' WHERE id = ?1",
            [raw_root.id],
        )
        .expect("names the raw root's edition");
        let en_root = scan::roots(&conn)
            .expect("re-reads the roots")
            .into_iter()
            .find(|r| r.id == en_root.id)
            .expect("the English root");
        let raw_root = scan::roots(&conn)
            .expect("re-reads the roots")
            .into_iter()
            .find(|r| r.id == raw_root.id)
            .expect("the raw root");
        scan::scan_root(&conn, &en_root).expect("scans en");
        scan::scan_root(&conn, &raw_root).expect("scans raw");
        derive::derive(&conn).expect("derives");
        (en_root.id, raw_root.id)
    };
    (library, en_id, raw_id, en, raw)
}

/// Two libraries, scanned and derived.
///
/// - **Manga**: the English and raw directories of `two_root_library`, edition
///   keys `en` and `raw`, one series `Series` whose one work holds both
///   editions.
/// - **comics**: one directory holding a `Series` of its own (the same title, so
///   a second series row, and byte-identical pages, so a second work) and a
///   series `Other`. Its edition key is the default, `raw`.
///
/// Lower-case `comics` on purpose: listed by name (`COLLATE NOCASE`) it comes
/// first, by bytes or by id it comes second.
///
/// **The libraries are numbered 70 and 80**, apart from the directories, so a
/// query that read a library id as a directory id finds nothing rather than the
/// directory that happens to share its number.
struct TwoLibraries {
    library: Library,
    manga: i64,
    comics: i64,
    en_root: i64,
    raw_root: i64,
    comics_root: i64,
    _dirs: [crate::tests::TempDir; 3],
}

fn two_library_store() -> TwoLibraries {
    let en = crate::tests::tempdir("browse-lib-en");
    write_pages(&en.path().join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("browse-lib-raw");
    write_pages(&raw.path().join("Series").join("Series - v01"), 2);
    let comics = crate::tests::tempdir("browse-lib-comics");
    write_pages(
        &comics
            .path()
            .join("Comics Series")
            .join("Comics Series - v01"),
        2,
    );
    write_pages(&comics.path().join("Other").join("Other - v01"), 3);

    let library = Library::open_in_memory().expect("in-memory");
    let (manga, comics_id, en_root, raw_root, comics_root) = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'Manga'), (80, 'comics')",
            [],
        )
        .expect("two libraries, numbered apart from their directories");
        let en_root = scan::add_root_in(&conn, en.path(), scan::InLibrary::Named("Manga"))
            .expect("adds en")
            .id;
        let raw_root = scan::add_root_in(&conn, raw.path(), scan::InLibrary::Named("Manga"))
            .expect("adds raw")
            .id;
        let comics_root = scan::add_root_in(&conn, comics.path(), scan::InLibrary::Named("comics"))
            .expect("adds comics")
            .id;
        conn.execute(
            "UPDATE root SET edition_key = 'en' WHERE id = ?1",
            [en_root],
        )
        .expect("names the English directory's edition");
        conn.execute(
            "UPDATE root SET edition_key = 'raw' WHERE id = ?1",
            [raw_root],
        )
        .expect("names the raw directory's edition");
        // Re-read, so each directory is walked carrying the key just set.
        for root in scan::roots(&conn).expect("re-reads the roots") {
            scan::scan_root(&conn, &root).expect("scans");
        }
        derive::derive(&conn).expect("derives");
        let library_of = |root: i64| crate::test_support::library_of_root(&conn, root);
        (
            library_of(en_root),
            library_of(comics_root),
            en_root,
            raw_root,
            comics_root,
        )
    };
    TwoLibraries {
        library,
        manga,
        comics: comics_id,
        en_root,
        raw_root,
        comics_root,
        _dirs: [en, raw, comics],
    }
}

/// The ids of the series holding a member under a library's directories,
/// which is what narrowing to a library reads: one series can be in both.
fn series_of(library: &Library, library_id: i64) -> Vec<i64> {
    let conn = library.conn();
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT s.id FROM series s
               JOIN work w ON w.series_id = s.id JOIN edition e ON e.work_id = w.id
               JOIN member m ON m.edition_id = e.id JOIN library_root r ON r.root_id = m.root_id
              WHERE r.library_id = ?1 ORDER BY s.id",
        )
        .expect("prepares");
    stmt.query_map([library_id], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("reads")
}

/// An imported work — one with a `sidecar_path` and no series.
///
/// **The digest is derived from the title**, because `import::sidecar`
/// reconciles by `content_key` first: two works whose members share a sha256
/// are one work, and two calls to this helper with one constant digest would
/// produce a single row.
fn import_a_work(library: &Library, title: &str, path: &str) {
    let conn = library.conn();
    let digest: String = title
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .repeat(8);
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"{title}","reading":"rtl"}},
            "editions":{{"raw":{{"label":"raw","members":[
              {{"path":"raw/{title}.cbz","sha256":"{digest}","pages":2}}]}}}},
            "hub":"raw","alignments":[]}}"#
    );
    let doc = taiyaku_core::Sidecar::from_json(&json).expect("parses");
    crate::import::sidecar(&conn, &doc, std::path::Path::new(path)).expect("imports");
}

/// One root, one series, **two** volumes — the smallest fixture where
/// `unread == works` and `unread == works - 1` are different numbers.
///
/// `two_root_library` has a single volume, so every count it can produce is 1
/// or 0, and a test over it passes whether or not the progress join works.
fn two_volume_library() -> (Library, i64, crate::tests::TempDir) {
    let dir = crate::tests::tempdir("browse-two-volumes");
    write_pages(&dir.path().join("Series").join("Series - v01"), 2);
    write_pages(&dir.path().join("Series").join("Series - v02"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    let root_id = {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("adds the root");
        scan::scan_root(&conn, &root).expect("scans");
        derive::derive(&conn).expect("derives");
        root.id
    };
    (library, root_id, dir)
}

/// The ids of this library's works, lowest volume first.
fn work_ids(library: &Library) -> Vec<i64> {
    let conn = library.conn();
    let mut stmt = conn
        .prepare("SELECT id FROM work ORDER BY volume, id")
        .expect("prepares");
    stmt.query_map([], |row| row.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("reads")
}

#[test]
fn access_every_sees_every_root() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &Access::Every, 1, None).expect("lists");
    assert_eq!(cards.len(), 1, "one series");
    assert_eq!(cards[0].editions, 2, "both editions are visible");
    assert_eq!(cards[0].works, 1, "one volume, held in two roots");
}

/// **`Only` with no directory sees nothing, at every public function**: the
/// access `account::access_for` reads for a grant whose library holds no
/// directory, which a clause left out would read as every library.
///
/// The fixture holds what each function could leak: a series whose work has
/// two editions, an alignment between them and a reader's open position, an
/// imported (unfiled) work, and a member gone from disk. Each function is
/// asked with `Every` first, so every empty answer below is the filter's and
/// not the fixture's.
#[test]
fn only_with_no_dir_sees_nothing() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let scanned = work_ids(&library)[0];
    align(&library, scanned, "en", "raw");
    import_a_work(&library, "Imported", "/x/taiyaku.json");
    let conn = library.conn();
    conn.execute("DELETE FROM scan_entry WHERE root_id = ?1", [en_id])
        .expect("the en member has left the disk");
    state::put_progress(
        &conn,
        scanned,
        1,
        &Progress {
            edition: "en".into(),
            slot: 0,
        },
    )
    .expect("reader 1 is partway through the work");
    let series_id: i64 = conn
        .query_row("SELECT id FROM series", [], |row| row.get(0))
        .expect("the one series");
    let editions: Vec<i64> = conn
        .prepare("SELECT id FROM edition WHERE work_id = ?1")
        .expect("prepares")
        .query_map([scanned], |row| row.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    let library_id: i64 = crate::test_support::library_of_root(&conn, en_id);
    let slug = "series-v01";

    let granted_nothing = Access::Only {
        roots: Vec::new(),
        libraries: Vec::new(),
    };
    for access in [Access::Every, granted_nothing] {
        let sees = access == Access::Every;
        let series = series(&conn, &access, 1, None).expect("lists series");
        assert_eq!(!series.is_empty(), sees, "series, {access:?}");
        let unfiled = unfiled(&conn, &access, 1, None).expect("lists unfiled");
        assert_eq!(!unfiled.is_empty(), sees, "unfiled, {access:?}");
        let hits = search(&conn, &access, 1, "Series", None).expect("searches");
        assert_eq!(!hits.series.is_empty(), sees, "search, {access:?}");
        let home = home(&conn, &access, 1, None).expect("reads home");
        assert_eq!(!home.added.is_empty(), sees, "home's added, {access:?}");
        assert_eq!(
            !home.continuing.is_empty(),
            sees,
            "home's continuing, {access:?}"
        );
        assert!(home.read.is_empty(), "nothing is finished");
        let one = one_series(&conn, &access, 1, series_id, None).expect("reads the series");
        assert_eq!(one.is_some(), sees, "one_series, {access:?}");
        let detail = work(&conn, &access, 1, slug).expect("reads the work");
        assert_eq!(detail.is_some(), sees, "work, {access:?}");
        let cover = cover_work(&conn, &access, series_id).expect("reads the cover");
        assert_eq!(cover.is_some(), sees, "cover_work, {access:?}");
        let scope = library_scope(&conn, &access, library_id).expect("reads the scope");
        assert_eq!(scope.is_some(), sees, "library_scope, {access:?}");
        let visible = sees_edition(&conn, &access, slug, "en");
        assert_eq!(visible, sees, "visible_members, {access:?}");
        let all = visible_editions(&conn, &access).expect("lists editions");
        assert_eq!(!all.is_empty(), sees, "visible_editions, {access:?}");
        let missing = missing_by_edition(&conn, &access, &editions).expect("counts");
        assert_eq!(!missing.is_empty(), sees, "missing_by_edition, {access:?}");
        let libraries = status(&conn, &access).expect("lists libraries").libraries;
        assert_eq!(!libraries.is_empty(), sees, "status, {access:?}");
        // Every public path reaches the pair only through rows the edition
        // clause already admitted, so the pair's own reading of this access
        // is asked of the one function that reads it alone.
        let pairs = alignments_of(&conn, &access, scanned).expect("lists pairs");
        assert_eq!(!pairs.is_empty(), sees, "alignments_of, {access:?}");
    }
}

#[test]
fn a_grant_hides_the_editions_it_does_not_name_and_keeps_the_work() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &Access::roots(vec![en_id]), 1, None).expect("lists");
    assert_eq!(cards.len(), 1, "the work is still listed");
    assert_eq!(cards[0].editions, 1, "only the granted root's edition");
    assert_eq!(cards[0].works, 1, "the count is computed after the filter");
}

/// An imported work is **`unfiled`, not a series of its own** — `import.rs`
/// never writes `series_id`, so no series row is created and `series()`
/// returns one either way. Spec C1 §5.4.
///
/// **NULL excludes itself**, which is what lets it need no special case: an
/// imported member has `root_id` NULL, so a restricted account does not see
/// it, and an unrestricted one does.
#[test]
fn an_imported_work_is_unfiled_rather_than_a_series_of_its_own() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    import_a_work(&library, "Imported", "/x/taiyaku.json");

    let conn = library.conn();
    assert_eq!(
        series(&conn, &Access::Every, 1, None).expect("lists").len(),
        1,
        "an import creates no series, so the grid still holds one"
    );
    assert_eq!(
        unfiled(&conn, &Access::Every, 1, None)
            .expect("lists")
            .len(),
        1,
        "unrestricted sees the imported work in the unfiled bucket"
    );
    assert!(
        unfiled(&conn, &Access::roots(vec![en_id]), 1, None)
            .expect("lists")
            .is_empty(),
        "a restricted account sees no work that sits under no root"
    );
}

/// O64. Contrast `an_imported_work_is_unfiled_rather_than_a_series_of_its_own`
/// directly above: a member genuinely new to the document has no held row and
/// correctly gets `root_id` NULL, but a member `two_root_library`'s own scan
/// already stamped is not new — a `library import` over the same work (the
/// route a hand-corrected `taiyaku.json` reimport opens) must not blank it.
///
/// A blanked `root_id` would make `browse.rs`'s `IN (…)` filter exclude the
/// member by SQL's own rules, so the work would vanish from a root-restricted
/// grant that could see it before the reimport.
#[test]
fn a_reimport_over_a_scanned_work_keeps_its_roots_grant() {
    let (library, en_id, raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let (work_id, slug): (i64, String) = conn
        .query_row("SELECT id, slug FROM work LIMIT 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("reads the work the scan derived");

    // The baseline this test's failure would otherwise be mistaken for: both
    // roots see the work before any reimport touches it.
    assert!(
        work(&conn, &Access::roots(vec![en_id]), 1, &slug)
            .expect("reads")
            .is_some(),
        "the en root sees the work before the reimport"
    );
    assert!(
        work(&conn, &Access::roots(vec![raw_id]), 1, &slug)
            .expect("reads")
            .is_some(),
        "the raw root sees the work before the reimport"
    );

    // A `library import` over the very document this work already is —
    // exactly the route `taiyaku library import` opens when re-importing a
    // hand-corrected `taiyaku.json`. Reconciling by `content_key`, this lands
    // on the same row rather than minting a second work.
    let doc = crate::project::sidecar(&conn, work_id).expect("projects the current document");
    crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
        .expect("reimports");

    assert!(
        work(&conn, &Access::roots(vec![en_id]), 1, &slug)
            .expect("reads")
            .is_some(),
        "the en root's grant must still see a work its own scan stamped"
    );
    assert!(
        work(&conn, &Access::roots(vec![raw_id]), 1, &slug)
            .expect("reads")
            .is_some(),
        "the raw root's grant must still see a work its own scan stamped"
    );
}

#[test]
fn a_work_is_found_by_slug_within_the_grant() {
    let (library, en_id, raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reads a slug");
    assert!(
        work(&conn, &Access::Every, 1, &slug)
            .expect("reads")
            .is_some()
    );
    assert!(
        work(&conn, &Access::roots(vec![en_id]), 1, &slug)
            .expect("reads")
            .is_some()
    );
    assert!(
        work(&conn, &Access::roots(vec![raw_id]), 1, &slug)
            .expect("reads")
            .is_some()
    );
}

/// A work whose every edition sits outside the grant is `None`, which is the
/// same answer as one that does not exist.
#[test]
fn a_work_entirely_outside_the_grant_is_none() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    import_a_work(&library, "Imported", "/x/taiyaku.json");
    let conn = library.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work WHERE series_id IS NULL", [], |row| {
            row.get(0)
        })
        .expect("reads the imported slug");
    assert!(
        work(&conn, &Access::Every, 1, &slug)
            .expect("reads")
            .is_some()
    );
    assert!(
        work(&conn, &Access::roots(vec![en_id]), 1, &slug)
            .expect("reads")
            .is_none(),
        "a restricted account cannot reach a work under no root"
    );
}

#[test]
fn search_finds_a_series_by_its_title() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let hits = search(&conn, &Access::Every, 1, "Series", None).expect("searches");
    assert_eq!(hits.series.len(), 1);
    assert!(hits.unfiled.is_empty());
}

/// Search reads **both** FTS indexes, so a work is findable by its own title
/// even when it belongs to no series — spec C1 §5.4.
#[test]
fn search_finds_an_unfiled_work_by_its_own_title() {
    let (library, _, _, _en, _raw) = two_root_library();
    import_a_work(&library, "Hanayome", "/x/taiyaku.json");
    let conn = library.conn();
    let hits = search(&conn, &Access::Every, 1, "Hanayome", None).expect("searches");
    assert!(hits.series.is_empty(), "no series is named Hanayome");
    assert_eq!(hits.unfiled.len(), 1, "the work itself is the hit");
    assert_eq!(hits.unfiled[0].title, "Hanayome");
}

#[test]
fn an_empty_query_searches_for_nothing_rather_than_everything() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let hits = search(&conn, &Access::Every, 1, "   ", None).expect("searches");
    assert!(hits.series.is_empty() && hits.unfiled.is_empty());
}

/// The cover falls back to the lowest-volume work while `cover_work_id` is
/// NULL, which it is for every series in every library — spec C1 §5.2.
#[test]
fn the_cover_falls_back_to_the_lowest_volume_when_none_is_set() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("reads the series");
    assert!(
        cover_work(&conn, &Access::Every, series_id)
            .expect("reads")
            .is_some()
    );
}

/// A hand-set cover is honoured.
#[test]
fn a_hand_set_cover_wins_over_the_fallback() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let (series_id, work_id): (i64, i64) = conn
        .query_row(
            "SELECT s.id, w.id FROM series s JOIN work w ON w.series_id = s.id LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("reads");
    conn.execute(
        "UPDATE series SET cover_work_id = ?1 WHERE id = ?2",
        [work_id, series_id],
    )
    .expect("sets a cover");
    assert_eq!(
        cover_work(&conn, &Access::Every, series_id)
            .expect("reads")
            .expect("a cover")
            .work_id,
        work_id
    );
}

/// `home` orders `continuing` by when each work was last read.
///
/// **Asserting order rather than membership is the point.** The ordering
/// subquery binds `user_id` a second time, and getting the count right but the
/// order wrong is the quiet neighbouring mistake.
#[test]
fn continuing_is_ordered_by_when_each_work_was_last_read() {
    let (library, _, _, _en, _raw) = two_root_library();
    import_a_work(&library, "Older", "/older/taiyaku.json");
    import_a_work(&library, "Newer", "/newer/taiyaku.json");

    let conn = library.conn();
    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM work WHERE series_id IS NULL ORDER BY id")
            .expect("prepares");
        stmt.query_map([], |row| row.get(0))
            .expect("reads")
            .collect::<rusqlite::Result<_>>()
            .expect("reads")
    };
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (1, ?1, 'raw', 0, '2026-08-01T00:00:00Z')",
        [ids[0]],
    )
    .expect("older position");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (1, ?1, 'raw', 0, '2026-08-19T00:00:00Z')",
        [ids[1]],
    )
    .expect("newer position");

    let home = home(&conn, &Access::Every, 1, None).expect("reads home");
    assert_eq!(home.continuing.len(), 2);
    assert_eq!(home.continuing[0].id, ids[1], "most recently read first");
    assert!(
        home.read.is_empty(),
        "neither position is finished, so nothing is in `read`"
    );
}

/// A finished position moves from `continuing` to `read`.
#[test]
fn a_completed_position_is_read_rather_than_continuing() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reads");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at)
         VALUES (1, ?1, 'raw', 0, '2026-08-19T00:00:00Z', '2026-08-19T00:00:00Z')",
        [work_id],
    )
    .expect("finished position");

    let home = home(&conn, &Access::Every, 1, None).expect("reads home");
    assert!(home.continuing.is_empty());
    assert_eq!(home.read.len(), 1);
    assert_eq!(home.added.len(), 1, "the work is still newly added");
}

/// `two_root_library`'s two roots (`en`, `raw`), but **two volumes** in each
/// — the smallest fixture where one work can hold a position in more than one
/// edition while a second work exists, so an ordering test cannot pass by
/// having only one row to sort.
///
/// [`library_with_volumes`] at two, under its own name because the tests below
/// read as statements about two works rather than about a parameter.
fn library_with_two_works() -> (Library, crate::tests::TempDir, crate::tests::TempDir) {
    library_with_volumes(2)
}

/// A work sorts by the newest of its positions, not by an arbitrary one.
///
/// A scalar subquery over a work's several positions, such as
/// `(SELECT updated FROM progress p WHERE p.work_id = w.id AND p.user_id = ?)`,
/// returns whichever row SQLite reaches first and reports no error, so the
/// ordering would be arbitrary among a work's editions.
///
/// **The old position is put on `en` and the new one on `raw`, deliberately.**
/// `progress` has no index but its own primary key, `(user_id, work_id,
/// edition)`, so an unordered scan of it — exactly what a scalar subquery
/// runs — reaches `en` before `raw` for a fixed work: `'en' < 'raw'`
/// lexically. Putting the newer timestamp on `raw` means such a query reaches
/// the *older* row first for `ids[0]` and reports it as that work's position —
/// the one arrangement a single-position fixture cannot produce, and the
/// reason this test needs two positions on one work to mean anything.
#[test]
fn continuing_sorts_on_the_newest_position_a_work_holds() {
    let (library, _en, _raw) = library_with_two_works();
    let ids = work_ids(&library);
    let conn = library.conn();

    // Work `ids[0]` was touched long ago in `en` and just now in `raw`; work
    // `ids[1]` in between. Newest-first must therefore put `ids[0]` ahead of
    // `ids[1]`.
    state::put_progress(
        &conn,
        ids[0],
        1,
        &Progress {
            edition: "en".into(),
            slot: 1,
        },
    )
    .expect("an old en position");
    conn.execute(
        "UPDATE progress SET updated = '2020-01-01 00:00:00'
          WHERE work_id = ?1 AND edition = 'en'",
        [ids[0]],
    )
    .expect("backdates it");
    state::put_progress(
        &conn,
        ids[1],
        1,
        &Progress {
            edition: "raw".into(),
            slot: 1,
        },
    )
    .expect("a position in between");
    conn.execute(
        "UPDATE progress SET updated = '2024-01-01 00:00:00' WHERE work_id = ?1",
        [ids[1]],
    )
    .expect("backdates it");
    state::put_progress(
        &conn,
        ids[0],
        1,
        &Progress {
            edition: "raw".into(),
            slot: 1,
        },
    )
    .expect("a fresh raw position, newer than both of the above");

    let home = home(&conn, &Access::Every, 1, None).expect("reads home");
    let order: Vec<i64> = home.continuing.iter().map(|w| w.id).collect();
    assert_eq!(
        order,
        vec![ids[0], ids[1]],
        "work 0's newest position (`raw`, just now) is the newest of all"
    );
}

/// Finishing one edition while reading another does not put a work in both
/// lists.
#[test]
fn a_work_finished_in_one_edition_and_open_in_another_is_still_continuing() {
    let (library, _en, _raw) = library_with_two_works();
    let ids = work_ids(&library);
    let conn = library.conn();

    state::put_progress(
        &conn,
        ids[0],
        1,
        &Progress {
            edition: "raw".into(),
            slot: 3,
        },
    )
    .expect("a raw position");
    conn.execute(
        "UPDATE progress SET completed_at = '2026-01-01 00:00:00'
          WHERE work_id = ?1 AND edition = 'raw'",
        [ids[0]],
    )
    .expect("finishes the raw edition");
    state::put_progress(
        &conn,
        ids[0],
        1,
        &Progress {
            edition: "en".into(),
            slot: 1,
        },
    )
    .expect("an open en position");

    let home = home(&conn, &Access::Every, 1, None).expect("reads home");
    assert!(
        home.continuing.iter().any(|w| w.id == ids[0]),
        "an unfinished edition keeps the work continuing"
    );
    assert!(
        !home.read.iter().any(|w| w.id == ids[0]),
        "and it must not also be sitting in Read"
    );
}

/// Two roots of one library, added with no `edition_key` on either.
///
/// Both fall back to `DEFAULT_EDITION_KEY`, so derivation merges their members
/// into **one** edition. `two_root_library` renames the keys to avoid exactly
/// this, so it cannot show a merged edition.
fn merged_edition_library() -> (
    Library,
    i64,
    i64,
    crate::tests::TempDir,
    crate::tests::TempDir,
) {
    let a = crate::tests::tempdir("browse-merge-a");
    write_pages(&a.path().join("Series").join("Series - v01"), 2);
    let b = crate::tests::tempdir("browse-merge-b");
    write_pages(&b.path().join("Series").join("Series - v01"), 3);

    let library = Library::open_in_memory().expect("in-memory");
    let (a_id, b_id) = {
        let conn = library.conn();
        let ra =
            scan::add_root_in(&conn, a.path(), scan::InLibrary::Named("Manga")).expect("adds a");
        let rb =
            scan::add_root_in(&conn, b.path(), scan::InLibrary::Named("Manga")).expect("adds b");
        scan::scan_root(&conn, &ra).expect("scans a");
        scan::scan_root(&conn, &rb).expect("scans b");
        derive::derive(&conn).expect("derives");
        (ra.id, rb.id)
    };
    (library, a_id, b_id, a, b)
}

/// **A merged edition's counts cover only the members the grant allows.** When
/// two roots merge into one edition, a reader granted only one of them must
/// not see member and page counts covering both.
#[test]
fn a_merged_edition_counts_only_the_members_the_grant_allows() {
    let (library, a_id, b_id, _a, _b) = merged_edition_library();
    let conn = library.conn();

    let all = series(&conn, &Access::Every, 1, None).expect("lists");
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].editions, 1,
        "two roots with no edition_key merge into one edition — the shape \
         that made this leak reachable by default"
    );

    let slug: String = conn
        .query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reads a slug");

    let unrestricted = work(&conn, &Access::Every, 1, &slug)
        .expect("reads")
        .expect("visible");
    let total: i64 = unrestricted.card.editions.iter().map(|e| e.members).sum();
    assert_eq!(total, 2, "both roots' members");

    for (root, label) in [(a_id, "a"), (b_id, "b")] {
        let seen = work(&conn, &Access::roots(vec![root]), 1, &slug)
            .expect("reads")
            .expect("visible");
        let members: i64 = seen.card.editions.iter().map(|e| e.members).sum();
        assert_eq!(
            members, 1,
            "root {label}: a granted reader must be counted only their own members"
        );
    }
}

/// Whether `access` sees a member of `slug`'s `edition`: the page routes'
/// grant check, which `taiyaku-serve`'s `view::of_edition` reads from
/// [`visible_members`].
pub(crate) fn sees_edition(conn: &Connection, access: &Access, slug: &str, edition: &str) -> bool {
    visible_members(conn, access, slug, Some(edition))
        .expect("reads")
        .get(edition)
        .is_some_and(|members| members.iter().any(|member| member.visible))
}

/// The same boundary at the page level: `visible_members` is what the page
/// routes ask, and a merged edition must still be visible to each grant that
/// holds part of it.
#[test]
fn a_merged_edition_is_visible_to_either_grant_and_an_unheld_root_sees_nothing() {
    let (library, a_id, _b_id, _a, _b) = merged_edition_library();
    let conn = library.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reads a slug");
    let key: String = conn
        .query_row("SELECT key FROM edition LIMIT 1", [], |row| row.get(0))
        .expect("reads the edition key");

    assert!(sees_edition(&conn, &Access::Every, &slug, &key));
    assert!(sees_edition(&conn, &Access::roots(vec![a_id]), &slug, &key));
    assert!(
        !sees_edition(&conn, &Access::roots(vec![a_id]), &slug, "no-such-edition"),
        "an edition that does not exist is not visible"
    );
    assert!(
        !sees_edition(&conn, &Access::roots(vec![a_id]), "no-such-work", &key),
        "a work that does not exist is not visible"
    );
}

/// Narrowing reads each member of a merged edition, in the order the sidecar
/// lists them, so each grant must be told which of the two roots' members it
/// sees, where each sits, and which file it is.
#[test]
fn visible_members_flags_each_member_of_a_merged_edition_in_ordinal_order() {
    let (library, a_id, b_id, _a, _b) = merged_edition_library();
    let conn = library.conn();
    let (slug, key): (String, String) = conn
        .query_row(
            "SELECT w.slug, e.key FROM work w JOIN edition e ON e.work_id = w.id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the fixture: one work of one edition");
    let shape: Vec<(i64, i64, String, String)> = conn
        .prepare("SELECT pages, root_id, sha256, path FROM member ORDER BY ordinal")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect()
        })
        .expect("lists the members");
    assert_eq!(
        shape
            .iter()
            .map(|(pages, root, _, _)| (*pages, *root))
            .collect::<Vec<_>>(),
        [(2, a_id), (3, b_id)],
        "the fixture: a's two pages are member 0, b's three member 1"
    );
    assert_ne!(shape[0].2, shape[1].2, "the fixture: two different files");
    assert_ne!(shape[0].3, shape[1].3, "the fixture: at two paths");
    let read_as = |visible: &[bool]| -> Vec<SeenMember> {
        shape
            .iter()
            .zip(visible)
            .map(|((_, _, sha256, path), &visible)| SeenMember {
                sha256: sha256.clone(),
                path: path.clone(),
                visible,
            })
            .collect()
    };

    let flags = |roots: Vec<i64>, edition: Option<&str>| {
        visible_members(&conn, &Access::roots(roots), &slug, edition).expect("reads")
    };
    for (roots, seen) in [
        (vec![a_id], [true, false]),
        (vec![b_id], [false, true]),
        (vec![a_id, b_id], [true, true]),
        (vec![], [false, false]),
    ] {
        assert_eq!(
            flags(roots.clone(), None),
            HashMap::from([(key.clone(), read_as(&seen))]),
            "granted {roots:?}"
        );
    }
    assert_eq!(
        flags(vec![a_id], Some(&key)),
        HashMap::from([(key.clone(), read_as(&[true, false]))]),
        "asked for its one edition"
    );
    assert!(
        flags(vec![a_id], Some("no-such-edition")).is_empty(),
        "an edition the work does not have has no flags"
    );
    assert!(
        visible_members(&conn, &Access::roots(vec![a_id]), "no-such-work", None)
            .expect("reads")
            .is_empty(),
        "a work that does not exist has no flags"
    );
}

/// The owner's rulings "Narrow the grant" and "Path counts too" (O91): a
/// member with no entry under a directory whose walk misses its path is
/// outside that directory's grant, and one with an entry of its bytes or at
/// its path is inside it whatever the patterns. Volume 1 has a copy in `a`
/// (Manga's) and `b` (Comics'); `a` comes to exclude it and its scan forgets
/// `a`'s entry with no derive after, so both `member_root` rows stay. Each
/// pattern is asked as `a`'s walk asks, Manga holds it ([`held_in`]) by the
/// same rule, and a row whose directory its path is not under counts as before.
#[test]
fn a_member_with_no_entry_is_outside_the_grant_of_a_directory_whose_walk_misses_it() {
    let a = crate::tests::tempdir("browse-reach-a");
    let b = crate::tests::tempdir("browse-reach-b");
    let volume = |dir: &crate::tests::TempDir| dir.path().join("Series").join("Series - v01");
    write_pages(&volume(&a), 2);
    // The same bytes, so the two are one member with a copy in each.
    std::fs::create_dir_all(volume(&b)).expect("mkdir");
    for page in std::fs::read_dir(volume(&a)).expect("lists `a`'s pages") {
        let page = page.expect("a page").path();
        std::fs::copy(&page, volume(&b).join(page.file_name().expect("a name")))
            .expect("copies a page");
    }
    // Something `a` still reads once `Series/` is excluded, so its scan is
    // not refused as an empty read (§8.1).
    write_pages(&a.path().join("Other").join("Other - v01"), 2);
    let library = Library::open_in_memory().expect("in-memory");
    let conn = library.conn();
    let ra = scan::add_root_in(&conn, a.path(), scan::InLibrary::Named("Manga")).expect("adds a");
    let rb = scan::add_root_in(&conn, b.path(), scan::InLibrary::Named("Comics")).expect("adds b");
    scan::scan_root(&conn, &ra).expect("scans a");
    scan::scan_root(&conn, &rb).expect("scans b");
    derive::derive(&conn).expect("derives");
    let (slug, key, primary, held): (String, String, i64, i64) = conn
        .query_row(
            "SELECT w.slug, e.key, m.root_id,
                    (SELECT count(*) FROM member_root mr WHERE mr.member_id = m.id)
               FROM work w JOIN edition e ON e.work_id = w.id
               JOIN member m ON m.edition_id = e.id
              WHERE m.path = ?1",
            [ra.path
                .join("Series")
                .join("Series - v01")
                .to_string_lossy()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .expect("the fixture: volume 1's member names its copy in `a`");
    assert_eq!(
        (primary, held),
        (ra.id, 2),
        "the fixture: held in `a` and `b`"
    );
    let set = |globs: Option<&str>| {
        conn.execute(
            "UPDATE root SET exclude_globs = ?1 WHERE id = ?2",
            rusqlite::params![globs, ra.id],
        )
        .expect("sets `a`'s patterns");
    };
    let manga: i64 = conn
        .query_row(
            "SELECT library_id FROM library_root WHERE root_id = ?1",
            [ra.id],
            |r| r.get(0),
        )
        .expect("Manga, `a`'s library");
    let seen = |label: &str| {
        let sees = |access: Access| sees_edition(&conn, &access, &slug, &key);
        (
            label.to_owned(),
            [
                sees(Access::roots(vec![ra.id])),
                sees(Access::roots(vec![rb.id])),
                sees(Access::roots(vec![ra.id, rb.id])),
                sees(Access::Every),
                held_in(&conn, manga)
                    .expect("reads Manga's")
                    .contains(&slug),
            ],
        )
    };

    set(Some("Series/"));
    let mut table = vec![seen("an entry, excluded")];
    // As a derive leaves `a`'s entry once the volume is replaced in place by
    // other bytes and its rules make no member of them.
    let replaced = |sha256: &str| {
        conn.execute(
            "UPDATE scan_entry SET sha256 = ?1
              WHERE root_id = ?2 AND rel_path = 'Series/Series - v01'",
            rusqlite::params![sha256, ra.id],
        )
        .expect("sets `a`'s entry's digest")
    };
    let digest: String = conn
        .query_row(
            "SELECT sha256 FROM scan_entry
              WHERE root_id = ?1 AND rel_path = 'Series/Series - v01'",
            [ra.id],
            |r| r.get(0),
        )
        .expect("`a`'s entry");
    assert_eq!(replaced("other bytes"), 1, "the fixture: `a`'s entry");
    table.push(seen("an entry of other bytes at its path, excluded"));
    replaced(&digest);
    let excluding = scan::roots(&conn)
        .expect("roots")
        .into_iter()
        .find(|root| root.id == ra.id)
        .expect("`a`");
    let report = scan::scan_root(&conn, &excluding).expect("scans `a` excluding `Series/`");
    assert!(report.refused.is_none(), "{report:?}");
    let entries = |root: i64| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM scan_entry se JOIN member m ON m.sha256 = se.sha256
              WHERE se.root_id = ?1 AND m.path LIKE '%Series - v01'",
            [root],
            |r| r.get(0),
        )
        .expect("counts the volume's entries")
    };
    assert_eq!(
        (entries(ra.id), entries(rb.id)),
        (0, 1),
        "the fixture: `a`'s scan forgot its entry and `b` keeps one"
    );
    table.push(seen("no entry, its directory excluded"));
    set(Some("Series/Series - v01/"));
    table.push(seen("no entry, excluded as a directory"));
    set(Some("Other/"));
    table.push(seen("no entry, a pattern elsewhere"));
    set(None);
    table.push(seen("no entry, no pattern"));
    set(Some("Series/"));
    // As a scan of `b` that found its copy gone would leave it.
    conn.execute("DELETE FROM scan_entry WHERE root_id = ?1", [rb.id])
        .expect("forgets `b`'s entries");
    table.push(seen("no entry in either, `a` excluded"));
    let row = |label: &str, cells: [bool; 5]| (label.to_owned(), cells);
    assert_eq!(
        table,
        [
            row("an entry, excluded", [true, true, true, true, true]),
            row(
                "an entry of other bytes at its path, excluded",
                [true, true, true, true, true]
            ),
            row(
                "no entry, its directory excluded",
                [false, true, true, true, false]
            ),
            row(
                "no entry, excluded as a directory",
                [false, true, true, true, false]
            ),
            row(
                "no entry, a pattern elsewhere",
                [true, true, true, true, true]
            ),
            row("no entry, no pattern", [true, true, true, true, true]),
            row(
                "no entry in either, `a` excluded",
                [false, true, true, true, false]
            ),
        ],
        "[a, b, a and b, every, held in Manga]"
    );
}

/// A work whose every edition sits outside the grant is invisible to the page
/// routes too — the check that makes per-root sharing a boundary rather than a
/// browse filter.
#[test]
fn a_work_outside_the_grant_is_invisible_to_the_page_check() {
    let (library, en_id, raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reads a slug");

    assert!(sees_edition(
        &conn,
        &Access::roots(vec![en_id]),
        &slug,
        "en"
    ));
    assert!(
        !sees_edition(&conn, &Access::roots(vec![en_id]), &slug, "raw"),
        "the raw edition lives under a root this grant does not hold"
    );
    assert!(sees_edition(
        &conn,
        &Access::roots(vec![raw_id]),
        &slug,
        "raw"
    ));
}

/// `cover_work` answers `None` for a series that does not exist rather than
/// erroring — a 500-versus-404 difference would be an enumeration oracle.
#[test]
fn a_cover_for_a_series_that_does_not_exist_is_none_rather_than_an_error() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    assert_eq!(
        cover_work(&conn, &Access::Every, 9999).expect("does not error"),
        None
    );
}

/// The cover names an edition the grant allows, never `work.hub` when the
/// grant hides it.
#[test]
fn the_cover_names_a_visible_edition_rather_than_the_hub() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    conn.execute("UPDATE work SET hub = 'raw'", [])
        .expect("points the hub at the edition the grant will hide");
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("reads the series");

    let target = cover_work(&conn, &Access::roots(vec![en_id]), series_id)
        .expect("reads")
        .expect("a cover");
    assert_eq!(
        target.edition, "en",
        "the cover comes from the granted root, not from the hidden hub"
    );
}

/// Search caps **after** the grant filter, so a restricted reader still finds
/// what they can see even when many hits lie outside their grant.
#[test]
fn search_does_not_lose_a_visible_hit_to_the_cap() {
    let (library, _, _, _en, _raw) = two_root_library();
    // Thirty imported works all matching "Volume", none under any root, plus
    // the visible series. Capping before the filter would let these twenty
    // crowd the real hit out.
    for n in 0..30 {
        import_a_work(
            &library,
            &format!("Volume{n:02}"),
            &format!("/v{n:02}/taiyaku.json"),
        );
    }
    let conn = library.conn();
    let hits = search(&conn, &Access::Every, 1, "Volume05", None).expect("searches");
    assert_eq!(hits.unfiled.len(), 1, "the one exact match is found");
    assert_eq!(hits.unfiled[0].title, "Volume05");
}

/// `one_series` returns the series and its works, not `None`.
///
/// **O62, folded into this test rather than a parallel one**: this is the
/// series page's own shape, and `missing` is a field on the same
/// `EditionCard` `works[0].editions` already carries — a second test would
/// duplicate this whole fixture to assert one more field on it. The `en`
/// edition's `scan_entry` is deleted here to stand in for
/// `scan::forget_unseen` finding the file gone on a real rescan, without
/// running a second scan just to remove one row; `member` survives exactly as
/// `forget_unseen` leaves it.
#[test]
fn one_series_returns_the_series_and_its_works() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("reads the series");

    let (card, works) = one_series(&conn, &Access::Every, 1, series_id, None)
        .expect("reads")
        .expect("the series is visible");
    assert_eq!(card.id, series_id);
    assert_eq!(works.len(), 1, "its one volume");
    assert_eq!(works[0].editions.len(), 2, "both editions");
    assert!(
        works[0].editions.iter().all(|e| e.missing == 0),
        "nothing has left disk yet"
    );

    conn.execute("DELETE FROM scan_entry WHERE root_id = ?1", [en_id])
        .expect("simulates forget_unseen dropping the en root's scan_entry");

    let (_card, works) = one_series(&conn, &Access::Every, 1, series_id, None)
        .expect("reads")
        .expect("still visible — the member row, and so the work, survives");
    let en = works[0]
        .editions
        .iter()
        .find(|e| e.key == "en")
        .expect("the en edition");
    let raw = works[0]
        .editions
        .iter()
        .find(|e| e.key == "raw")
        .expect("the raw edition");
    assert_eq!(
        en.missing, 1,
        "its one member's scan_entry is gone, while the member row survives"
    );
    assert_eq!(en.members, 1, "the member row itself is untouched");
    assert_eq!(raw.missing, 0, "the raw edition's file is still on disk");

    assert!(
        one_series(&conn, &Access::Every, 1, 9999, None)
            .expect("reads")
            .is_none(),
        "a series that does not exist is None, and the id is what decides it"
    );
}

/// Search reads **two** indexes, and this tells them apart.
///
/// `two_root_library`'s series and its work share the word "Series", so with
/// that fixture either index alone would still produce the hit and cover for
/// the other's absence.
#[test]
fn each_search_index_is_load_bearing_on_its_own() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();

    // A name only the SERIES has — reachable through `series_fts` alone.
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, is_primary)
         SELECT id, 'Quintessential', 'english', 0 FROM series LIMIT 1",
        [],
    )
    .expect("gives the series an alias no work carries");

    // A name only the WORK has — reachable through `work_fts` alone, and it
    // must still drag its series into the results.
    conn.execute(
        "UPDATE work SET title = 'Nakano' WHERE series_id IS NOT NULL",
        [],
    )
    .expect("gives the work a title the series does not carry");

    let by_series = search(&conn, &Access::Every, 1, "Quintessential", None).expect("searches");
    assert_eq!(
        by_series.series.len(),
        1,
        "a series alias is found through the series index"
    );

    let by_work = search(&conn, &Access::Every, 1, "Nakano", None).expect("searches");
    assert_eq!(
        by_work.series.len(),
        1,
        "a work title drags its series in through the work index"
    );
}

#[test]
fn a_series_nobody_has_opened_is_entirely_unread() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let cards = series(&conn, &Access::Every, 1, None).expect("lists series");
    let card = cards.first().expect("one series");
    assert_eq!(card.works, 2, "the fixture has two volumes");
    assert_eq!(card.unread, 2, "neither has been opened");
}

#[test]
fn one_position_leaves_the_rest_of_the_series_unread() {
    let (library, _root, _dir) = two_volume_library();
    // `work_ids` before `library.conn()`, deliberately: `Library::conn` locks
    // a `Mutex<Connection>`, not reentrant, and this test's own `conn` binding
    // would still be held across the call otherwise — the two calls deadlock
    // on the same thread.
    let ids = work_ids(&library);
    let conn = library.conn();
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (1, ?1, 'raw', 0, '2026-08-19T00:00:00Z')",
        [ids[0]],
    )
    .expect("a position on volume 1");

    let cards = series(&conn, &Access::Every, 1, None).expect("lists series");
    let card = cards.first().expect("one series");
    assert_eq!(card.unread, 1, "volume 2 is still unread");
    assert_ne!(card.unread, card.works, "the series is no longer untouched");
}

/// **The test that pins the `user_id` binding.** With the parameter bound in
/// the wrong order — or not bound at all — this reads as if reader 1 had
/// opened the volume, and every other test in this file still passes.
#[test]
fn another_readers_position_does_not_mark_this_one_read() {
    let (library, _root, _dir) = two_volume_library();
    // `work_ids` before `library.conn()` — see the comment in
    // `one_position_leaves_the_rest_of_the_series_unread`.
    let ids = work_ids(&library);
    let conn = library.conn();
    // Migration 1 inserts only user 1; a second row for this test alone, the
    // same way `state::tests::notes_are_isolated_per_user` does — `progress`
    // FK-references `user`, and every real connection (including this
    // in-memory one) runs with `PRAGMA foreign_keys = ON`.
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'user')",
        [],
    )
    .expect("second user");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (2, ?1, 'raw', 0, '2026-08-19T00:00:00Z')",
        [ids[0]],
    )
    .expect("reader 2's position");

    let cards = series(&conn, &Access::Every, 1, None).expect("lists series");
    assert_eq!(
        cards.first().expect("one series").unread,
        2,
        "reader 1 has opened nothing"
    );
}

#[test]
fn added_is_the_highest_work_id_in_the_series() {
    let (library, _root, _dir) = two_volume_library();
    // `work_ids` before `library.conn()` — see the comment in
    // `one_position_leaves_the_rest_of_the_series_unread`.
    let ids = work_ids(&library);
    let conn = library.conn();
    let cards = series(&conn, &Access::Every, 1, None).expect("lists series");
    assert_eq!(
        cards.first().expect("one series").added,
        *ids.iter().max().expect("two works"),
    );
}

/// **The test that pins parameter order when the grant is non-empty.** Every
/// test above calls `series` with `Access::Every`, and `root_clause` contributes
/// zero placeholders in that case — so the query carries exactly one `?`
/// regardless of which order `series` pushes `roots` and `user_id` in, and a
/// swap between them is invisible. `en_id` gives the query a second
/// placeholder, and the decoy progress row is deliberately keyed to the
/// root's own id: if `user_id`'s hole were ever bound with a root's value
/// instead of reader 99's, this row would wrongly look like reader 99 had
/// opened the work.
#[test]
fn a_restricted_grant_still_binds_user_id_after_its_own_roots() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    // `work_ids` before `library.conn()` — see the comment in
    // `one_position_leaves_the_rest_of_the_series_unread`.
    let ids = work_ids(&library);
    let conn = library.conn();
    // A user sharing the root's id, so a root value leaking into the
    // `user_id` hole would masquerade as this reader. Migration 1 already
    // seats user 1, which is what `en_id` resolves to for a fresh library —
    // `OR IGNORE` keeps this test from assuming that rather than stating it.
    conn.execute(
        "INSERT OR IGNORE INTO user (id, name, password_hash, role)
         VALUES (?1, 'decoy', '!', 'user')",
        [en_id],
    )
    .expect("a user sharing the granted root's id");
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (99, 'reader', '!', 'user')",
        [],
    )
    .expect("the reader actually running the query");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (?1, ?2, 'en', 0, '2026-08-19T00:00:00Z')",
        [en_id, ids[0]],
    )
    .expect("a position belonging to the root-id user, not reader 99");

    let cards = series(&conn, &Access::roots(vec![en_id]), 99, None).expect("lists series");
    let card = cards
        .first()
        .expect("the series, restricted to the en root");
    assert_eq!(
        card.editions, 1,
        "only the granted root's edition is counted"
    );
    assert_eq!(
        card.unread, 1,
        "reader 99 has opened nothing, whichever hole user_id was bound to"
    );
    assert_eq!(card.added, ids[0], "the fixture's one work");
}

#[test]
fn a_library_narrows_the_grid_to_its_own_series() {
    let s = two_library_store();
    let conn = s.library.conn();

    let all = series(&conn, &Access::Every, 1, None).expect("lists");
    assert_eq!(all.len(), 3, "both libraries, unnarrowed");

    let manga = series(&conn, &Access::Every, 1, Some(s.manga)).expect("lists");
    let ids: Vec<i64> = manga.iter().map(|c| c.id).collect();
    drop(conn);
    assert_eq!(ids, series_of(&s.library, s.manga), "Manga's one series");
    assert_eq!(manga[0].editions, 2, "with both its directories' editions");

    let conn = s.library.conn();
    let comics = series(&conn, &Access::Every, 1, Some(s.comics)).expect("lists");
    let mut ids: Vec<i64> = comics.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    drop(conn);
    assert_eq!(
        ids,
        series_of(&s.library, s.comics),
        "comics' two series, and not Manga's Series"
    );
    assert!(comics.iter().all(|c| c.editions == 1));
}

/// **The narrowing never widens the grant.** A caller granted Manga asking
/// for comics is answered nothing, on every listing that narrows.
#[test]
fn a_library_outside_the_grant_answers_nothing() {
    let s = two_library_store();
    import_a_work(&s.library, "Imported", "/x/taiyaku.json");
    let conn = s.library.conn();
    let grant = Access::Only {
        roots: vec![s.en_root, s.raw_root],
        libraries: vec![s.manga],
    };
    let narrowed = Some(s.comics);
    assert!(
        series(&conn, &grant, 1, narrowed)
            .expect("lists")
            .is_empty()
    );
    assert!(
        unfiled(&conn, &grant, 1, narrowed)
            .expect("lists")
            .is_empty()
    );
    let hits = search(&conn, &grant, 1, "Series", narrowed).expect("searches");
    assert!(hits.series.is_empty() && hits.unfiled.is_empty());
    let home = home(&conn, &grant, 1, narrowed).expect("reads home");
    assert!(home.added.is_empty() && home.continuing.is_empty() && home.read.is_empty());
}

/// **A library with no directory of its own answers nothing**, never the caller's
/// whole access in its place ([`library_scope`]'s `None`). Unrestricted
/// caller, so a leak would be the whole store.
#[test]
fn a_library_that_does_not_exist_answers_nothing_rather_than_everything() {
    let s = two_library_store();
    let conn = s.library.conn();
    assert!(
        series(&conn, &Access::Every, 1, Some(9_999))
            .expect("lists")
            .is_empty()
    );
    assert!(
        home(&conn, &Access::Every, 1, Some(9_999))
            .expect("reads")
            .added
            .is_empty()
    );
    assert!(
        library_scope(&conn, &Access::Every, 9_999)
            .expect("reads")
            .is_none()
    );

    // A library with no directory, which nothing public makes (O205), written
    // by hand: the same answer.
    conn.execute("INSERT INTO library (name) VALUES ('Empty')", [])
        .expect("a library with no directory");
    let empty = conn.last_insert_rowid();
    assert!(
        series(&conn, &Access::Every, 1, Some(empty))
            .expect("lists")
            .is_empty()
    );
    assert!(
        library_scope(&conn, &Access::Every, empty)
            .expect("reads")
            .is_none()
    );
}

/// **The grant's share, not the library's whole.** Grants are whole libraries
/// through the API, but `browse` takes any directory list; this pins the
/// intersection, which a narrowing to the library's directories alone would fail
/// (Manga's series would show both editions).
#[test]
fn the_narrowing_reads_the_dirs_the_library_and_the_grant_share() {
    let s = two_library_store();
    let conn = s.library.conn();
    let grant = Access::Only {
        roots: vec![s.en_root, s.comics_root],
        libraries: vec![s.manga, s.comics],
    };
    let cards = series(&conn, &grant, 1, Some(s.manga)).expect("lists");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].editions, 1, "the English edition only");
    let scope = library_scope(&conn, &grant, s.manga)
        .expect("reads")
        .expect("Manga, through its English directory");
    assert_eq!(scope.name, "Manga");
    assert_eq!(scope.dirs, vec![s.en_root]);
}

/// Each of home's three lists narrows. Positions: continuing on Manga's work
/// and comics' `Other`; finished on comics' `Series`.
#[test]
fn every_home_list_narrows_to_the_library() {
    let s = two_library_store();
    let (manga_work, comics_series_work, other_work) = {
        let conn = s.library.conn();
        let one = |sql: &str, p: i64| -> i64 {
            conn.query_row(sql, [p], |r| r.get(0)).expect("one work")
        };
        (
            one(
                "SELECT DISTINCT e.work_id FROM edition e JOIN member m ON m.edition_id = e.id
                  WHERE m.root_id = ?1",
                s.en_root,
            ),
            one(
                "SELECT DISTINCT w.id FROM work w JOIN series x ON x.id = w.series_id
                   JOIN edition e ON e.work_id = w.id JOIN member m ON m.edition_id = e.id
                  WHERE m.root_id = ?1 AND x.sort_title = 'Comics Series'",
                s.comics_root,
            ),
            one(
                "SELECT DISTINCT w.id FROM work w JOIN series x ON x.id = w.series_id
                   JOIN edition e ON e.work_id = w.id JOIN member m ON m.edition_id = e.id
                  WHERE m.root_id = ?1 AND x.sort_title = 'Other'",
                s.comics_root,
            ),
        )
    };
    {
        let conn = s.library.conn();
        for (work, edition, done) in [
            (manga_work, "en", None),
            (other_work, "raw", None),
            (comics_series_work, "raw", Some("2026-10-17T00:00:00Z")),
        ] {
            conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at)
                 VALUES (1, ?1, ?2, 0, '2026-10-17T00:00:00Z', ?3)",
                rusqlite::params![work, edition, done],
            )
            .expect("a position");
        }
    }
    let conn = s.library.conn();
    let ids = |works: &[WorkCard]| works.iter().map(|w| w.id).collect::<Vec<_>>();

    let comics = home(&conn, &Access::Every, 1, Some(s.comics)).expect("reads home");
    assert_eq!(ids(&comics.continuing), vec![other_work]);
    assert_eq!(ids(&comics.read), vec![comics_series_work]);
    let mut added = ids(&comics.added);
    added.sort_unstable();
    let mut want = vec![comics_series_work, other_work];
    want.sort_unstable();
    assert_eq!(added, want);

    let manga = home(&conn, &Access::Every, 1, Some(s.manga)).expect("reads home");
    assert_eq!(ids(&manga.continuing), vec![manga_work]);
    assert_eq!(ids(&manga.added), vec![manga_work]);
    assert!(
        manga.read.is_empty(),
        "the one finished work is comics', so Manga's read list is empty"
    );
}

/// Both halves of a search narrow: the series to the library's own, and the
/// unfiled works to none, since an imported work is in no library.
#[test]
fn search_narrowed_to_a_library_finds_only_its_series() {
    let s = two_library_store();
    import_a_work(&s.library, "Imported Series", "/x/taiyaku.json");
    let conn = s.library.conn();
    let all = search(&conn, &Access::Every, 1, "Series", None).expect("searches");
    assert_eq!(
        all.series.len(),
        2,
        "a series matching `Series` in each library"
    );
    assert_eq!(all.unfiled.len(), 1, "and the imported work, in neither");
    let comics = search(&conn, &Access::Every, 1, "Series", Some(s.comics)).expect("searches");
    assert!(
        comics.unfiled.is_empty(),
        "an imported work is in no library: {:?}",
        comics.unfiled
    );
    let ids: Vec<i64> = comics.series.iter().map(|c| c.id).collect();
    drop(conn);
    let theirs = series_of(&s.library, s.comics);
    assert_eq!(ids.len(), 1);
    assert!(
        theirs.contains(&ids[0]),
        "comics' `Comics Series`, not Manga's `Series`"
    );
}

/// An imported work is in no library: asking for one leaves the unfiled
/// bucket empty, by the grant's own rule (`IN (…)` is false for NULL).
#[test]
fn asking_for_a_library_leaves_the_unfiled_bucket_empty() {
    let s = two_library_store();
    import_a_work(&s.library, "Imported", "/x/taiyaku.json");
    let conn = s.library.conn();
    assert_eq!(
        unfiled(&conn, &Access::Every, 1, None)
            .expect("lists")
            .len(),
        1
    );
    assert!(
        unfiled(&conn, &Access::Every, 1, Some(s.manga))
            .expect("lists")
            .is_empty()
    );
}

/// An alignment row for a work, written directly: derivation only mints these
/// from a sidecar, and every fixture here is scanned.
fn align(library: &Library, work_id: i64, from: &str, to: &str) {
    let conn = library.conn();
    conn.execute(
        "INSERT INTO alignment (work_id, from_key, to_key, by, verdict, slots, paired)
         VALUES (?1, ?2, ?3, 'solver', 'good', 20, 18)",
        rusqlite::params![work_id, from, to],
    )
    .expect("an alignment");
}

#[test]
fn a_work_carries_the_verdict_and_paired_count_of_each_pair() {
    let (library, _en, _raw, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &Access::Every, 1, "series-v01")
        .expect("reads the work")
        .expect("a work with that slug");
    assert_eq!(detail.alignments.len(), 1);
    let pair = &detail.alignments[0];
    assert_eq!(pair.from, "en");
    assert_eq!(pair.to, "raw");
    assert_eq!(pair.verdict.as_deref(), Some("good"));
    assert_eq!(pair.paired, Some(18));
    assert_eq!(pair.slots, Some(20));
}

/// **A grant that hides one side hides the pair.** Listing an alignment against
/// an edition the reader cannot open would name material the grant withheld.
#[test]
fn an_alignment_naming_a_hidden_edition_is_not_listed() {
    let (library, en_id, _raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &Access::roots(vec![en_id]), 1, "series-v01")
        .expect("reads the work")
        .expect("the work survives on its English edition");
    assert_eq!(detail.card.editions.len(), 1, "only the English edition");
    assert!(
        detail.alignments.is_empty(),
        "the pair names an edition this grant hides"
    );
}

/// **The positive case the two tests above cannot tell apart.** Both pass a
/// grant that leaves the query with zero or one root placeholder, so neither
/// can distinguish a working `alignments_of` from one that always returns
/// nothing once `roots` is non-empty. Granting *both* sides of the pair gives
/// the statement four root placeholders plus the trailing `work_id`, and
/// checks the alignment's content rather than only that the list is non-empty.
#[test]
fn a_grant_naming_both_sides_of_a_pair_returns_the_alignment() {
    let (library, en_id, raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &Access::roots(vec![en_id, raw_id]), 1, "series-v01")
        .expect("reads the work")
        .expect("the work survives on both editions");
    assert_eq!(
        detail.alignments.len(),
        1,
        "the pair is visible when the grant holds both roots"
    );
    let pair = &detail.alignments[0];
    assert_eq!(pair.from, "en");
    assert_eq!(pair.to, "raw");
    assert_eq!(pair.verdict.as_deref(), Some("good"));
    assert_eq!(pair.paired, Some(18));
}

/// **The `aligned` counts and the alignment list are one question.** A pair
/// whose raw side sits under a root this reader does not hold must be counted
/// by neither, or one screen contradicts itself, in the module whose whole
/// reason for existing is that its counts are computed after the filter.
#[test]
fn an_alignment_naming_a_hidden_edition_is_counted_by_neither_the_work_nor_the_grid() {
    let (library, en_id, _raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &Access::roots(vec![en_id]), 1, "series-v01")
        .expect("reads the work")
        .expect("the work survives on its English edition");
    assert!(
        !detail.card.aligned,
        "the only pair names an edition this grant hides"
    );
    assert!(
        detail.alignments.is_empty(),
        "and the flag agrees with the list beside it"
    );

    let cards = series(&conn, &Access::roots(vec![en_id]), 1, None).expect("lists series");
    assert_eq!(
        cards.first().expect("one series").aligned,
        0,
        "so the grid draws no mark either"
    );
}

/// The positive half, without which the test above passes on a clause that
/// always answers false.
#[test]
fn an_alignment_wholly_inside_the_grant_is_counted_by_both() {
    let (library, en_id, raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &Access::roots(vec![en_id, raw_id]), 1, "series-v01")
        .expect("reads the work")
        .expect("the work survives on both editions");
    assert!(detail.card.aligned, "both sides are held");
    assert_eq!(detail.alignments.len(), 1, "and the list says the same");

    let cards = series(&conn, &Access::roots(vec![en_id, raw_id]), 1, None).expect("lists series");
    assert_eq!(cards.first().expect("one series").aligned, 1);
}

/// **`Every` counts the pair.** Every account without a grant reads as
/// `Every`, so a filter that tightened the count here would take the `⇄` mark
/// off every library in the world.
#[test]
fn access_every_counts_the_alignment_as_it_always_did() {
    let (library, _en_id, _raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &Access::Every, 1, "series-v01")
        .expect("reads the work")
        .expect("the work");
    assert!(detail.card.aligned, "unrestricted sees both sides");
    assert_eq!(detail.alignments.len(), 1);

    let cards = series(&conn, &Access::Every, 1, None).expect("lists series");
    assert_eq!(cards.first().expect("one series").aligned, 1);
}

/// The raw directory moved to a library of its own by hand, not derived
/// again: one work keeps an edition under each library (O191's shape).
fn raw_in_a_library_of_its_own(library: &Library, raw_root: i64) -> i64 {
    let conn = library.conn();
    conn.execute("INSERT INTO library (name) VALUES ('Raws')", [])
        .expect("a second library");
    let raws = conn.last_insert_rowid();
    conn.execute(
        "UPDATE library_root SET library_id = ?1 WHERE root_id = ?2",
        [raws, raw_root],
    )
    .expect("files the raw directory under it");
    raws
}

/// **A library narrowing reaches the `aligned` count, not only `works`,
/// `editions` and `unread`.**
///
/// With the raw directory in `Raws` ([`raw_in_a_library_of_its_own`]), the one
/// work holds its `en` edition in the English directory's library and its `raw`
/// edition in `Raws`. Narrowed to the English directory's library, the work
/// survives on its English edition and the `en`↔`raw` pair, whose other side
/// is in `Raws`, is counted by nobody: a tile drawing `⇄ 1` beside
/// `editions: 1` would claim a pair it shows half of. The `None` call is the
/// contrast: nothing changes between the two but that argument.
#[test]
fn a_library_narrows_the_aligned_count_and_not_only_the_other_three() {
    let (library, en_id, raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");
    raw_in_a_library_of_its_own(&library, raw_id);

    let conn = library.conn();
    let english_library: i64 = crate::test_support::library_of_root(&conn, en_id);
    assert!(
        ![en_id, raw_id].contains(&english_library),
        "library {english_library} shares a directory's id"
    );
    let all = series(&conn, &Access::Every, 1, None).expect("lists series");
    let whole = all.first().expect("one series");
    assert_eq!(whole.editions, 2, "unnarrowed, both editions are visible");
    assert_eq!(whole.aligned, 1, "and the pair they form is counted");

    let english = series(&conn, &Access::Every, 1, Some(english_library)).expect("lists series");
    let card = english.first().expect("one series");
    assert_eq!(card.works, 1, "the work survives on its English edition");
    assert_eq!(card.editions, 1, "which is the only one in this library");
    assert_eq!(
        card.aligned, 0,
        "so the pair, whose other side is in another library, is counted by nobody"
    );
}

/// **The positive half**, without which the test above passes on a filter
/// that answers false for every alignment there is — the hole
/// `an_alignment_wholly_inside_the_grant_is_counted_by_both` fills for the
/// grant.
///
/// Both directories are in Manga and the grant names both, so the narrowing reads
/// two directories and the pair survives with both its editions.
#[test]
fn a_library_keeps_a_pair_whose_two_editions_are_both_in_it() {
    let (library, en_id, raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let manga: i64 = crate::test_support::library_of_root(&conn, en_id);
    let grant = Access::Only {
        roots: vec![en_id, raw_id],
        libraries: vec![manga],
    };
    let cards = series(&conn, &grant, 1, Some(manga)).expect("lists series");
    let card = cards.first().expect("one series");
    assert_eq!(
        card.editions, 2,
        "both editions are in the library asked for"
    );
    assert_eq!(
        card.aligned, 1,
        "so the pair they form survives the narrowing as well as the grant"
    );
}

#[test]
fn status_lists_each_library_once_by_name_with_case_folded() {
    let s = two_library_store();
    let conn = s.library.conn();
    let names: Vec<String> = status(&conn, &Access::Every)
        .expect("reads")
        .libraries
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(
        names,
        vec!["comics".to_string(), "Manga".to_string()],
        "two directories make one entry, and lower-case `comics` sorts first"
    );
}

/// The slugs of the works with a member under each library's directories. The
/// raw directory is relabelled into comics with no derive after it, as a move
/// leaves it, so Manga's work is under both libraries' directories.
#[test]
fn held_in_answers_the_slugs_of_the_works_under_a_librarys_directories() {
    let s = two_library_store();
    let conn = s.library.conn();
    let dirs = [s.en_root, s.raw_root, s.comics_root];
    assert!(
        !dirs.contains(&s.manga) && !dirs.contains(&s.comics),
        "libraries {} and {} share a directory's id: {dirs:?}",
        s.manga,
        s.comics
    );
    conn.execute(
        "UPDATE library_root SET library_id = ?1 WHERE root_id = ?2",
        [s.comics, s.raw_root],
    )
    .expect("relabels the raw directory");
    let under = |these: &[i64]| -> HashSet<String> {
        let holes = holes(these.len());
        conn.prepare(&format!(
            "SELECT DISTINCT w.slug FROM work w
               JOIN edition e ON e.work_id = w.id
               JOIN member m ON m.edition_id = e.id
              WHERE m.root_id IN ({holes})"
        ))
        .expect("prepares")
        .query_map(params_from_iter(these), |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads")
    };

    let manga = held_in(&conn, s.manga).expect("reads Manga's");
    let comics = held_in(&conn, s.comics).expect("reads comics'");
    assert_eq!(manga.len(), 1, "Manga's one work: {manga:?}");
    assert_eq!(
        comics.len(),
        3,
        "comics' `Comics Series` and `Other`, and Manga's work: {comics:?}"
    );
    assert_eq!(manga, under(&[s.en_root]));
    assert_eq!(comics, under(&[s.raw_root, s.comics_root]));
    assert!(
        manga.is_subset(&comics),
        "Manga's work is under both: {manga:?} {comics:?}"
    );
}

#[test]
fn status_lists_only_the_libraries_the_grant_names() {
    let s = two_library_store();
    let conn = s.library.conn();
    let grant = Access::Only {
        roots: vec![s.en_root, s.raw_root],
        libraries: vec![s.manga],
    };
    let restricted = status(&conn, &grant).expect("reads");
    let ids: Vec<i64> = restricted.libraries.iter().map(|l| l.id).collect();
    assert_eq!(ids, vec![s.manga]);
}

/// The titles of `cards`, in the order listed.
fn titles(cards: Vec<SeriesCard>) -> Vec<String> {
    cards.into_iter().map(|card| card.title).collect()
}

/// Grants list, reach does not: Manga's account reaches Kids' directory and
/// is not listed Kids, and Kids' account is not listed Manga.
#[test]
fn status_lists_the_libraries_granted_and_not_those_reached() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    let listed = |reader: i64| -> Vec<i64> {
        let access = crate::account::access_for(&conn, reader).expect("reads");
        status(&conn, &access)
            .expect("lists libraries")
            .libraries
            .into_iter()
            .map(|entry| entry.id)
            .collect()
    };
    assert_eq!(listed(n.manga_reader), vec![n.manga]);
    assert_eq!(listed(n.kids_reader), vec![n.kids]);
    assert_eq!(
        status(&conn, &Access::Every)
            .expect("lists libraries")
            .libraries
            .len(),
        2,
        "every library for an administrator"
    );
}

/// A library opens for an account granted it, and shows the books its
/// directories reach; it does not open for an account that only reaches it,
/// or for one whose grant sits inside it.
#[test]
fn a_library_opens_only_where_granted_and_shows_what_it_reaches() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    let manga = crate::account::access_for(&conn, n.manga_reader).expect("reads");
    let kids = crate::account::access_for(&conn, n.kids_reader).expect("reads");

    assert_eq!(
        library_scope(&conn, &manga, n.kids).expect("reads"),
        None,
        "Manga's account reaches Kids' directory and is not granted Kids"
    );
    let scope = library_scope(&conn, &manga, n.manga)
        .expect("reads")
        .expect("Manga opens for its account");
    assert_eq!(scope.dirs, vec![n.m, n.m_kids]);
    assert_eq!(
        titles(series(&conn, &manga, n.manga_reader, Some(n.manga)).expect("lists")),
        ["Kids Book", "Manga Book"]
    );

    assert_eq!(
        library_scope(&conn, &kids, n.manga).expect("reads"),
        None,
        "Kids' account is not granted Manga"
    );
    assert!(
        series(&conn, &kids, n.kids_reader, Some(n.manga))
            .expect("lists")
            .is_empty()
    );
}

/// An outer directory's pattern takes the nested directory out of the outer
/// library's page, at once, and leaves the nested library's own page alone.
#[test]
fn a_directory_the_outer_excludes_leaves_its_page_and_stays_on_its_own() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    conn.execute(
        "UPDATE root SET exclude_globs = 'kids/' WHERE id = ?1",
        [n.m],
    )
    .expect("excludes kids/ from Manga's directory");
    let manga = crate::account::access_for(&conn, n.manga_reader).expect("reads");
    let kids = crate::account::access_for(&conn, n.kids_reader).expect("reads");

    assert_eq!(
        titles(series(&conn, &Access::Every, 1, Some(n.manga)).expect("lists")),
        ["Manga Book"]
    );
    assert_eq!(
        titles(series(&conn, &manga, n.manga_reader, Some(n.manga)).expect("lists")),
        ["Manga Book"]
    );
    assert_eq!(
        titles(series(&conn, &kids, n.kids_reader, Some(n.kids)).expect("lists")),
        ["Kids Book"]
    );
    assert_eq!(
        titles(series(&conn, &kids, n.kids_reader, None).expect("lists")),
        ["Kids Book"]
    );
}

/// Two deep, `m` excluding `kids/tiny`: Manga's page shows Kids' book and not
/// Tiny's, though Kids reaches Tiny. The page reads the scope's directories
/// as they are; reaching again from them would add Tiny through Kids.
#[test]
fn a_library_page_reads_its_reach_and_does_not_reach_again_from_it() {
    let n = crate::reach::tests::nested_libraries(true);
    let tiny = n.m_kids_tiny.expect("the third directory");
    let conn = n.library.conn();
    conn.execute(
        "UPDATE root SET exclude_globs = 'kids/tiny' WHERE id = ?1",
        [n.m],
    )
    .expect("excludes kids/tiny from Manga's directory");

    let scope = library_scope(&conn, &Access::Every, n.manga)
        .expect("reads")
        .expect("Manga opens");
    assert_eq!(scope.dirs, vec![n.m, n.m_kids]);
    assert_eq!(
        titles(series(&conn, &Access::Every, 1, Some(n.manga)).expect("lists")),
        ["Kids Book", "Manga Book"]
    );
    let manga = crate::account::access_for(&conn, n.manga_reader).expect("reads");
    assert_eq!(
        titles(series(&conn, &manga, n.manga_reader, Some(n.manga)).expect("lists")),
        ["Kids Book", "Manga Book"]
    );
    assert_eq!(
        library_scope(&conn, &Access::Every, n.kids)
            .expect("reads")
            .expect("Kids opens")
            .dirs,
        vec![n.m_kids, tiny],
        "Kids reaches Tiny by its own patterns"
    );
}

/// Spec §4's N: the works held in Manga are those under the directories it
/// reaches, Kids' among them; Kids holds its own alone.
#[test]
fn held_in_names_the_works_under_the_directories_a_library_reaches() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    // Each directory holds one work, its own book's.
    let slug_under = |dir: i64| -> String {
        conn.query_row(
            "SELECT w.slug FROM work w JOIN edition e ON e.work_id = w.id
               JOIN member m ON m.edition_id = e.id WHERE m.root_id = ?1",
            [dir],
            |r| r.get(0),
        )
        .expect("the work under the directory")
    };

    assert_eq!(
        held_in(&conn, n.manga).expect("reads Manga's"),
        HashSet::from([slug_under(n.m), slug_under(n.m_kids)])
    );
    assert_eq!(
        held_in(&conn, n.kids).expect("reads Kids'"),
        HashSet::from([slug_under(n.m_kids)])
    );
}

/// Per library: a scan of one library's directory says nothing of another's.
///
/// **The libraries are numbered 70 and 80**, apart from their directories, so a
/// status that read a library id as a directory id (`d.root_id = l.id`, with no
/// `JOIN root`) cannot find the walked directory's rows under a library of the
/// same number and pass.
#[test]
fn scanned_is_true_only_for_a_library_one_of_whose_dirs_was_walked() {
    let walked = crate::tests::tempdir("browse-walked");
    write_pages(&walked.path().join("Series").join("Series - v01"), 1);
    let unwalked = crate::tests::tempdir("browse-unwalked");
    write_pages(&unwalked.path().join("Other").join("Other - v01"), 1);
    let library = Library::open_in_memory().expect("in-memory");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO library (id, name) VALUES (70, 'Walked'), (80, 'Unwalked')",
        [],
    )
    .expect("two libraries, numbered apart from their directories");
    let a =
        scan::add_root_in(&conn, walked.path(), scan::InLibrary::Named("Walked")).expect("adds");
    let b = scan::add_root_in(&conn, unwalked.path(), scan::InLibrary::Named("Unwalked"))
        .expect("adds");
    let filed: Vec<(i64, i64)> = conn
        .prepare("SELECT root_id, library_id FROM library_root ORDER BY root_id")
        .expect("prepares")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert_eq!(
        filed,
        vec![(a.id, 70), (b.id, 80)],
        "each directory in its library"
    );
    assert!(
        filed
            .iter()
            .all(|(_, library)| filed.iter().all(|(dir, _)| dir != library)),
        "no library's id is a directory's: {filed:?}"
    );
    let before = status(&conn, &Access::Every).expect("reads");
    assert!(
        before.libraries.iter().all(|l| !l.scanned),
        "nothing walked yet"
    );
    scan::scan_root(&conn, &a).expect("scans one");
    let after = status(&conn, &Access::Every).expect("reads");
    let flags: Vec<(String, bool)> = after
        .libraries
        .into_iter()
        .map(|l| (l.name, l.scanned))
        .collect();
    assert_eq!(
        flags,
        vec![
            ("Unwalked".to_string(), false),
            ("Walked".to_string(), true)
        ]
    );
}

/// A first scan cancelled while hashing leaves the walk's rows and no last
/// scan, and its library has no works yet: a reader is told "Not read yet",
/// not "nothing here" (spec §7, "A paused, never-scanned library").
#[test]
fn a_library_whose_cancelled_first_scan_committed_its_walk_reads_not_scanned() {
    let walked = crate::tests::tempdir("browse-cancelled-first-scan");
    write_pages(&walked.path().join("Series").join("Series - v01"), 1);
    let library = Library::open_in_memory().expect("in-memory");
    let conn = library.conn();
    let root = scan::add_root(&conn, walked.path()).expect("adds");
    let roots = std::slice::from_ref(&root);
    let scanned = |conn: &Connection| -> Vec<bool> {
        let listed = status(conn, &Access::Every).expect("reads").libraries;
        listed.into_iter().map(|l| l.scanned).collect()
    };

    let held = scan::last_scans(&conn, roots).expect("reads the last scan");
    scan::scan_root(&conn, &root).expect("walks");
    let walked_reads = scanned(&conn);
    scan::mark_cancelled(&conn, &held).expect("marks it cancelled");

    let rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM dir_state WHERE root_id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("counts the walk's rows");
    assert!(rows > 0, "sanity: the walk's rows stay");
    assert_eq!(
        (walked_reads, scanned(&conn)),
        (vec![true], vec![false]),
        "(walked, cancelled)"
    );
}

/// A two-character Japanese query is a real word, and trigram answers it with
/// silence — so `search` answers it with `LIKE`.
///
/// `花嫁` is the second half of `五等分の花嫁` and two characters long: below
/// trigram's window, above nothing.
#[test]
fn a_query_too_short_for_trigram_is_answered_by_like() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, is_primary)
         SELECT id, '五等分の花嫁', 'native', 0 FROM series LIMIT 1",
        [],
    )
    .expect("gives the series its native title");

    let hits = search(&conn, &Access::Every, 1, "花嫁", None).expect("searches");
    assert_eq!(
        hits.series.len(),
        1,
        "two characters must still find the shelf"
    );

    let long = search(&conn, &Access::Every, 1, "分の花嫁", None).expect("searches");
    assert_eq!(
        long.series.len(),
        1,
        "and four characters go through the index instead"
    );
}

/// `match_for` is pinned directly, not only through what `search` returns.
///
/// `LIKE '%q%'` is a superset of what trigram `MATCH` answers for every
/// fixture above it in this file — pin `TRIGRAM_MIN` at `100` instead of `3`
/// and every one of those tests still passes, because the `LIKE` scan quietly
/// answers what the index was supposed to. A result-shaped assertion cannot
/// tell the two paths apart; asserting on `match_for`'s own return is a direct
/// unit test of the decision rather than of a consequence the wrong path also
/// produces, and simpler than proving the same thing through
/// `EXPLAIN QUERY PLAN`.
#[test]
fn match_for_switches_paths_exactly_at_trigram_min() {
    // Fixed lengths, not `TRIGRAM_MIN - 1` / `TRIGRAM_MIN`: building the
    // fixture from the constant under test would make this test move with
    // any mutation to it and never fail. `TRIGRAM_MIN` is 3, measured
    // against the tokeniser; a change to that measurement should break this
    // pin, not glide past it.
    assert!(
        matches!(match_for("ab"), Match::Like(_)),
        "two characters is below TRIGRAM_MIN and must take the LIKE path"
    );

    assert!(
        matches!(match_for("abc"), Match::Fts(_)),
        "three characters is TRIGRAM_MIN and must take the FTS path"
    );
}

/// The `LIKE` path escapes the reader's own wildcards.
///
/// **Without `ESCAPE`, a search for `%` returns the whole library.** That turns
/// a search box into a table dump, and it is one character long.
#[test]
fn a_wildcard_typed_into_the_search_box_is_a_literal() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();

    for wildcard in ["%", "_"] {
        let hits = search(&conn, &Access::Every, 1, wildcard, None).expect("searches");
        assert!(
            hits.series.is_empty() && hits.unfiled.is_empty(),
            "`{wildcard}` is a character to look for, not an operator"
        );
    }
}

/// A backslash typed into the search box is a literal too — pinned
/// separately from `%` and `_` because an empty-result assertion cannot
/// tell an escaped backslash from an unescaped one.
///
/// Unescaped, `%` and `_` each match *every* row of a fixture, which the
/// "expect nothing" assertion above catches. An unescaped `\` does the
/// opposite — it escapes the query's own trailing `%` in
/// `title LIKE '%' || ? || '%' ESCAPE '\'`, so the pattern becomes "ends with
/// a literal `%`" and matches *nothing*, same as the correctly-escaped form.
/// A fixture with no backslash in any title makes the two behaviours
/// indistinguishable by an empty-result check. The only assertion that tells
/// them apart is a positive one: give a work a title that contains a literal
/// backslash and expect to find it.
#[test]
fn a_backslash_typed_into_the_search_box_is_a_literal() {
    let (library, _, _, _en, _raw) = two_root_library();
    import_a_work(&library, "Placeholder", "/x/taiyaku.json");
    let conn = library.conn();
    conn.execute(
        "UPDATE work SET title = 'back\\slash' WHERE series_id IS NULL",
        [],
    )
    .expect("gives the unfiled work a title with a literal backslash");

    let hits = search(&conn, &Access::Every, 1, "\\", None).expect("searches");
    assert_eq!(
        hits.unfiled.len(),
        1,
        "a literal backslash in the query must still find the work that has one"
    );
}

/// An unfiled work is reachable by a short query too — the `LIKE` path has to
/// cover both indexes, not only the series one.
#[test]
fn a_short_query_finds_an_unfiled_work_by_its_own_title() {
    let (library, _, _, _en, _raw) = two_root_library();
    // `two_root_library` files its one work into a series; an unfiled work
    // needs an import, same as `search_finds_an_unfiled_work_by_its_own_title`
    // above. Without this, the `UPDATE` below renames zero rows.
    import_a_work(&library, "Placeholder", "/x/taiyaku.json");
    let conn = library.conn();
    conn.execute("UPDATE work SET title = '花嫁' WHERE series_id IS NULL", [])
        .expect("renames the unfiled work");

    let hits = search(&conn, &Access::Every, 1, "花嫁", None).expect("searches");
    assert_eq!(
        hits.unfiled.len(),
        1,
        "the unfiled index needs the same fallback"
    );
}

/// A short query matching a *work's* title must still drag its parent series
/// into the results — `series_of_matching_works`'s `LIKE` arm, pinned on its
/// own.
///
/// `each_search_index_is_load_bearing_on_its_own` above already proves
/// `series_of_matching_works` is load-bearing, but only on the FTS path: its
/// fixture renames the work to `Nakano`, five characters, well above
/// `TRIGRAM_MIN`, so that test never reaches `Match::Like` at all. This one
/// gives the work a two-character title its series does not carry and
/// searches for the whole thing, which is short enough that `match_for` can
/// only produce `Match::Like` — so it is the `LIKE` arm on trial here, not
/// the `MATCH` one.
#[test]
fn a_short_query_matching_a_works_title_drags_its_series_in() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    conn.execute(
        "UPDATE work SET title = '花嫁' WHERE series_id IS NOT NULL",
        [],
    )
    .expect("gives the work a title its series does not carry");

    let hits = search(&conn, &Access::Every, 1, "花嫁", None).expect("searches");
    assert_eq!(
        hits.series.len(),
        1,
        "a work's own short title must still surface the series it belongs to"
    );
}

/// The grant still decides, whichever path answered.
///
/// **A `LIMIT` before the grant filter could make a search that matched
/// visible rows report nothing found**, once every row the `LIMIT` kept is
/// one the grant hides. A second query path is a second chance to make that
/// mistake, so it is pinned on both.
#[test]
fn a_short_query_is_filtered_by_the_grant_like_a_long_one() {
    let (library, _en_root, raw_root, _en, _raw) = two_root_library();
    let conn = library.conn();
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, is_primary)
         SELECT id, '五等分の花嫁', 'native', 0 FROM series LIMIT 1",
        [],
    )
    .expect("native title");

    let unrestricted = search(&conn, &Access::Every, 1, "花嫁", None).expect("searches");
    assert_eq!(unrestricted.series.len(), 1, "`Every` sees it");

    let granted = search(&conn, &Access::roots(vec![raw_root]), 1, "花嫁", None).expect("searches");
    assert_eq!(
        granted.series.len(),
        1,
        "a grant covering an edition of it still sees it"
    );

    let elsewhere = search(&conn, &Access::roots(vec![9999]), 1, "花嫁", None).expect("searches");
    assert!(
        elsewhere.series.is_empty(),
        "a grant covering nothing sees nothing, however the query was answered"
    );
}

/// With no library asked for, search reads the whole grant: a series whose
/// two editions are under two granted directories is found with both of them.
/// `search_narrowed_to_a_library_finds_only_its_series` is the narrowed half.
#[test]
fn search_with_no_library_spans_the_whole_grant() {
    let (library, en_root, raw_root, _en, _raw) = two_root_library();
    let conn = library.conn();

    let both = search(
        &conn,
        &Access::roots(vec![en_root, raw_root]),
        1,
        "Series",
        None,
    )
    .expect("searches");
    assert_eq!(
        both.series.len(),
        1,
        "the series is found through the grant covering both its roots"
    );
    let card = both.series.first().expect("one series");
    assert_eq!(
        card.editions, 2,
        "and it carries both editions, one under each granted directory"
    );
}

/// O62. `scan::forget_unseen` deletes a `scan_entry` row for a file it no
/// longer meets while the `member` row survives, so the difference between
/// the two tables is the whole fact — no new column, no new state machine.
///
/// **Discriminates rather than degenerates.** `here.cbz` keeps its
/// `scan_entry` and must NOT be counted; `gone.cbz` has none and must be. A
/// query that counted every member (2), counted none (0), or whose
/// `ltrim`/`substr` arithmetic failed to match `here.cbz` back to its
/// `scan_entry` at all (which would also misfire as `here.cbz` being
/// "missing", making the total 2) all disagree with the answer this asserts.
///
/// `scan_entry`'s `kind`, `size` and `mtime` are `NOT NULL` with no default.
#[test]
fn an_edition_whose_file_has_left_the_disk_reports_it_missing() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO root (id, path) VALUES (1, '/library');
         INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 'w', 'W', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (1, 1, 0, '/library/here.cbz', 'a', 10, 1),
                (2, 1, 1, '/library/gone.cbz', 'b', 10, 1);
         INSERT INTO scan_entry (root_id, rel_path, kind, size, mtime)
         VALUES (1, 'here.cbz', 'file', 100, 0);",
    )
    .expect("seeding");

    let missing = missing_for(&conn, &Access::Every, 1);

    assert_eq!(
        missing, 1,
        "the scanner deletes `scan_entry` rows for files it no longer meets \
         while `member` rows survive — that difference is the whole fact"
    );
}

/// **O73: `missing_by_edition` must take the caller's grant.**
/// `member_clause`'s own doc records that a merged edition — two roots with no
/// `edition_key`, reachable in the default configuration rather than in a
/// corner — is exactly the shape that leaks when a count ignores the grant.
///
/// `merged_edition_library` gives one edition two members, one under each
/// root. Both are made to look gone from disk the way
/// `one_series_returns_the_series_and_its_works` simulates
/// `scan::forget_unseen`: delete the `scan_entry` row, leave `member` alone.
///
/// **A grant test, not a count test.** A caller granted only root `a` must be
/// told about **one** missing file — its own — never root `b`'s, which it
/// cannot see at all. `Every` is unrestricted and must still see both:
/// `member_clause` returns an empty string for that case, so the query is
/// unchanged, and asserting only the restricted half would not notice that
/// path breaking.
#[test]
fn a_restricted_grant_counts_only_the_missing_members_it_may_see() {
    let (library, a_id, b_id, _a, _b) = merged_edition_library();
    let conn = library.conn();
    let edition_id: i64 = conn
        .query_row("SELECT id FROM edition LIMIT 1", [], |row| row.get(0))
        .expect("the merged edition");

    conn.execute("DELETE FROM scan_entry WHERE root_id = ?1", [a_id])
        .expect("simulates forget_unseen dropping root a's file");
    conn.execute("DELETE FROM scan_entry WHERE root_id = ?1", [b_id])
        .expect("simulates forget_unseen dropping root b's file");

    let unrestricted = missing_for(&conn, &Access::Every, edition_id);
    assert_eq!(
        unrestricted, 2,
        "`Every` still sees both roots' missing members"
    );

    let restricted = missing_for(&conn, &Access::roots(vec![a_id]), edition_id);
    assert_eq!(
        restricted, 1,
        "a grant naming only root a is told about its one missing member, \
         not root b's, which it cannot see at all"
    );
}

/// One edition of each kind, and a third whose file no scan recorded, which
/// is no book: a query that matched every member, or matched by path alone,
/// would name more than the one.
#[test]
fn an_edition_walked_as_a_reflowable_epub_is_a_book() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO root (id, path) VALUES (1, '/library');
         INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 'w', 'W', 'ltr', 'en');
         INSERT INTO edition (id, work_id, key, label)
         VALUES (1, 1, 'en', 'en'), (2, 1, 'ko', 'ko'), (3, 1, 'fr', 'fr');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (1, 1, 0, '/library/en.epub', 'a', 3248, 1),
                (2, 2, 0, '/library/ko.cbz', 'b', 120, 1),
                (3, 3, 0, '/library/fr.epub', 'c', 900, 1);
         INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
         VALUES (1, 'en.epub', 'file', 'epub-reflow', 100, 0),
                (1, 'ko.cbz', 'file', 'zip', 100, 0);",
    )
    .expect("seeding");

    let books = books_among(&conn, &[1, 2, 3]).expect("lists");

    assert_eq!(books, HashSet::from([1]));
}

/// One edition's missing count, as [`missing_by_edition`] answers it.
///
/// It answers a map (O85) in which an edition with nothing missing has no row
/// at all — so absence reads as zero here, at the one place these tests ask.
fn missing_for(conn: &Connection, access: &Access, edition_id: i64) -> usize {
    missing_by_edition(conn, access, &[edition_id])
        .expect("counts")
        .get(&edition_id)
        .copied()
        .unwrap_or(0)
}

/// Two roots (`en`, `raw`), one series, `volumes` volumes in each.
///
/// O85's check needs two libraries whose *only* difference is how many works
/// the series holds, so the count it compares can be attributed to that and
/// nothing else. Each root names its own edition key, so every work carries
/// two editions and the per-edition half of the N+1 is reachable.
fn library_with_volumes(volumes: usize) -> (Library, crate::tests::TempDir, crate::tests::TempDir) {
    let en = crate::tests::tempdir("browse-count-en");
    let raw = crate::tests::tempdir("browse-count-raw");
    for v in 1..=volumes {
        let name = format!("Series - v{v:02}");
        write_pages(&en.path().join("Series").join(&name), 2);
        write_pages(&raw.path().join("Series").join(&name), 2);
    }

    let library = Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        // Directories of one library, as `two_root_library`'s are and for its
        // reason.
        let en_root =
            scan::add_root_in(&conn, en.path(), scan::InLibrary::Named("Manga")).expect("adds en");
        let raw_root = scan::add_root_in(&conn, raw.path(), scan::InLibrary::Named("Manga"))
            .expect("adds raw");
        conn.execute(
            "UPDATE root SET edition_key = 'en' WHERE id = ?1",
            [en_root.id],
        )
        .expect("names the English root's edition");
        conn.execute(
            "UPDATE root SET edition_key = 'raw' WHERE id = ?1",
            [raw_root.id],
        )
        .expect("names the raw root's edition");
        for id in [en_root.id, raw_root.id] {
            let root = scan::roots(&conn)
                .expect("re-reads the roots")
                .into_iter()
                .find(|r| r.id == id)
                .expect("the root just added");
            scan::scan_root(&conn, &root).expect("scans");
        }
        derive::derive(&conn).expect("derives");
    }
    (library, en, raw)
}

/// How many SQL statements one `one_series` listing issues.
fn statements_listing_series(library: &Library) -> (usize, usize) {
    let conn = library.conn();
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("reads the series");

    STATEMENTS.with(|c| c.set(0));
    let (_card, works) = one_series(&conn, &Access::Every, 1, series_id, None)
        .expect("reads")
        .expect("the series is visible");
    (STATEMENTS.with(std::cell::Cell::get), works.len())
}

/// **O85.** Listing a series must not issue one query per work for its
/// editions, and one per edition for what is missing: that is an N+1 nested
/// inside an N+1.
///
/// Scale invariance rather than a constant: the number itself is an
/// implementation detail that a later join may legitimately move, while "the
/// cost does not grow with the set" is the property O85 is about. A test
/// asserting a literal would have to be edited by the very change that breaks
/// it.
#[test]
fn listing_a_series_costs_the_same_number_of_statements_however_many_works_it_holds() {
    let (small_lib, _sen, _sraw) = library_with_volumes(2);
    let (large_lib, _len, _lraw) = library_with_volumes(6);

    let (small, small_works) = statements_listing_series(&small_lib);
    let (large, large_works) = statements_listing_series(&large_lib);

    assert_eq!(small_works, 2, "the small fixture's works");
    assert_eq!(large_works, 6, "the large fixture's works");
    assert_eq!(
        small, large,
        "a {small_works}-work series cost {small} statements and a \
         {large_works}-work one cost {large}: the listing's cost must not \
         grow with the number of works or editions in it",
    );
}

/// **The batched edition listing under a restricted grant, across more than one
/// work.**
///
/// [`editions_by_work`] binds its roots twice and then a generated
/// `IN (?,?,…)` of work ids, so both the placeholder count and the binding
/// order scale with the set. Every other grant test in this module asks about a
/// **single** work — `two_root_library` holds one volume — and an `IN` list of
/// one cannot show a mis-sized list or an off-by-one in the order the
/// parameters are pushed.
///
/// **The grant is on `raw`, the root added *second*, and that is the whole
/// point of the fixture.** Granting `en` — root id 1, against work ids 1, 2 and
/// 3 — cannot distinguish the correct binding from one that pushes the work ids
/// before the second root group: the swapped parameters read `{w2, w3, r}` for
/// the `IN` list, which is `{2, 3, 1}`, *the same set* as `{1, 2, 3}`, and the
/// misplaced root id is `w1`, which is `1`, the granted root. Right and wrong
/// render identically.
#[test]
fn a_restricted_grant_hides_its_edition_in_every_work_of_a_series() {
    let (library, _en, _raw) = library_with_volumes(3);
    let conn = library.conn();
    let raw_id: i64 = conn
        .query_row("SELECT id FROM root WHERE edition_key = 'raw'", [], |row| {
            row.get(0)
        })
        .expect("the raw root");
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("the series");

    let (_card, unrestricted) = one_series(&conn, &Access::Every, 1, series_id, None)
        .expect("reads")
        .expect("visible");
    assert_eq!(unrestricted.len(), 3, "three volumes");
    for work in &unrestricted {
        assert_eq!(
            work.editions.len(),
            2,
            "`Every` is unrestricted, so every work keeps both editions"
        );
    }

    let (_card, restricted) = one_series(&conn, &Access::roots(vec![raw_id]), 1, series_id, None)
        .expect("reads")
        .expect("visible");
    assert_eq!(restricted.len(), 3, "the works themselves all survive");
    for work in &restricted {
        let keys: Vec<&str> = work.editions.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["raw"],
            "work {:?} must keep only the granted root's edition — the en one \
             is hidden in every work of the set, not just the first",
            work.slug
        );
    }
}

/// **O84: `opens` is the edition the reader will actually open, not the hub.**
///
/// `startup.ts` calls `reading::restore(progress, hub, resolver)`, which takes
/// the stored position's edition when the resolver knows it and the hub
/// otherwise, and `api.ts::getProgress` hands it the head of a newest-first
/// list. This is that rule, server-side, and the three cases are the three
/// branches of it.
///
/// The hub is read out of the row rather than written here as `"en"`: which
/// edition derivation picks is `derive`'s business, and a test asserting the
/// letter of it would fail for the wrong reason the day that changes.
#[test]
fn opens_names_the_stored_position_and_falls_back_to_the_hub() {
    let (library, _en, _raw) = library_with_volumes(1);
    let conn = library.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work LIMIT 1", [], |row| row.get(0))
        .expect("the work");
    let hub: String = conn
        .query_row("SELECT hub FROM work LIMIT 1", [], |row| row.get(0))
        .expect("its hub");
    let other = if hub == "en" { "raw" } else { "en" };
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("the series");
    let read = |access: &Access| {
        one_series(&conn, access, 1, series_id, None)
            .expect("reads")
            .expect("visible")
            .1
            .remove(0)
            .opens
    };

    assert_eq!(
        read(&Access::Every),
        hub,
        "no position at all opens the hub"
    );

    state::put_progress(
        &conn,
        work_id,
        1,
        &Progress {
            edition: other.into(),
            slot: 0,
        },
    )
    .expect("a position in the non-hub edition");
    assert_eq!(
        read(&Access::Every),
        other,
        "a stored position opens its own edition, not the hub — this is the \
         whole of O84, and the case a hub-only field cannot express"
    );

    // The grant that hides the edition the position names. `restore` answers
    // the hub there because `resolver.slotCount` answers `undefined`, and this
    // must agree with it.
    let hub_root: i64 = conn
        .query_row(
            "SELECT id FROM root WHERE edition_key = ?1",
            [&hub],
            |row| row.get(0),
        )
        .expect("the hub's root");
    assert_eq!(
        read(&Access::roots(vec![hub_root])),
        hub,
        "a position naming an edition this grant hides falls back to the hub, \
         exactly as `restore` does when the resolver does not know it"
    );
}

/// **A grant that hides the hub substitutes, and `opens` has to substitute the
/// same way.**
///
/// `routes::filter_sidecar_editions` replaces a hidden hub with the first
/// surviving edition in key order before the reader ever sees the document —
/// its own doc says so, and `browse::cover_target` names the same trap for a
/// series cover. So the reader opens that substitute, and an `opens` still
/// naming the hidden hub would point `library.ts`'s `unreadable` at an edition
/// that is neither on the screen nor ever opened: `find` answers `undefined`,
/// the guard answers "readable", and whatever the substitute has actually lost
/// goes unasked.
#[test]
fn opens_substitutes_the_first_visible_edition_when_the_grant_hides_the_hub() {
    let (library, _en, _raw) = library_with_volumes(1);
    let conn = library.conn();
    let hub: String = conn
        .query_row("SELECT hub FROM work LIMIT 1", [], |row| row.get(0))
        .expect("its hub");
    let other = if hub == "en" { "raw" } else { "en" };
    let other_root: i64 = conn
        .query_row(
            "SELECT id FROM root WHERE edition_key = ?1",
            [other],
            |row| row.get(0),
        )
        .expect("the non-hub root");
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("the series");

    let card = one_series(&conn, &Access::roots(vec![other_root]), 1, series_id, None)
        .expect("reads")
        .expect("visible")
        .1
        .remove(0);

    assert_eq!(
        card.editions
            .iter()
            .map(|e| e.key.as_str())
            .collect::<Vec<_>>(),
        vec![other],
        "the grant leaves only the non-hub edition"
    );
    assert_eq!(
        card.opens, other,
        "`opens` must name an edition this caller can actually see — the hub \
         is hidden here, and `filter_sidecar_editions` will hand the reader \
         this one instead"
    );
}

/// **O114.** `PATCH /api/library/series/{id}` accepts a title list with no
/// entry marked primary. `sort_title` is the scan's key and a correction does
/// not move it, so the grid falls back to the first title the series holds
/// before the scanned name — text the correction may have removed.
#[test]
fn a_series_with_no_primary_title_is_shown_by_its_first_title_not_its_scanned_name() {
    let tree = crate::scan::tests::fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    let series_id: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("the scanned series");
    crate::librarydoc::set_series_titles(
        &conn,
        series_id,
        &[
            crate::librarydoc::SeriesTitle {
                title: "五等分の花嫁".to_string(),
                kind: crate::librarydoc::TitleKind::Native,
                primary: false,
            },
            crate::librarydoc::SeriesTitle {
                title: "The Quintessential Quintuplets".to_string(),
                kind: crate::librarydoc::TitleKind::English,
                primary: false,
            },
        ],
    )
    .expect("corrects the titles");

    let cards = series(&conn, &Access::Every, 1, None).expect("lists");
    let card = cards
        .iter()
        .find(|card| card.id == series_id)
        .unwrap_or_else(|| panic!("series {series_id} is on the grid: {cards:?}"));
    assert_eq!(
        card.title, "五等分の花嫁",
        "the first title written — not `The Quintessential Quintuplets`, which \
         sorts first, nor `Go-Toubun no Hanayome`, which the series no longer holds"
    );
}

/// One series in one directory whose volume 2 was scanned and derived before
/// volume 1 was written, so volume 1 holds the lower `volume` and the higher
/// `work.id`.
fn a_lower_volume_added_after() -> (Library, crate::tests::TempDir) {
    let dir = crate::tests::tempdir("browse-lower-volume-after");
    write_pages(&dir.path().join("Series").join("Series - v02"), 2);
    let library = Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("adds the root");
        scan::scan_root(&conn, &root).expect("scans volume 2");
        derive::derive(&conn).expect("derives volume 2");
        write_pages(&dir.path().join("Series").join("Series - v01"), 3);
        scan::scan_root(&conn, &root).expect("scans volume 1");
        derive::derive(&conn).expect("derives volume 1");
    }
    (library, dir)
}

/// One series of two volumes in one library, volume 1 under one directory and
/// volume 2 under another, so a grant on either directory hides the other
/// volume whole. Both directories take the default edition key.
fn two_volumes_across_two_roots() -> (
    Library,
    i64,
    i64,
    crate::tests::TempDir,
    crate::tests::TempDir,
) {
    let first = crate::tests::tempdir("browse-across-v01");
    write_pages(&first.path().join("Series").join("Series - v01"), 2);
    let second = crate::tests::tempdir("browse-across-v02");
    write_pages(&second.path().join("Series").join("Series - v02"), 3);
    let library = Library::open_in_memory().expect("in-memory");
    let (first_id, second_id) = {
        let conn = library.conn();
        let a = scan::add_root_in(&conn, first.path(), scan::InLibrary::Named("Manga"))
            .expect("adds volume 1's directory");
        let b = scan::add_root_in(&conn, second.path(), scan::InLibrary::Named("Manga"))
            .expect("adds volume 2's directory");
        scan::scan_root(&conn, &a).expect("scans volume 1");
        scan::scan_root(&conn, &b).expect("scans volume 2");
        derive::derive(&conn).expect("derives");
        (a.id, b.id)
    };
    (library, first_id, second_id, first, second)
}

/// The work holding `volume`.
fn work_of_volume(conn: &Connection, volume: f64) -> i64 {
    conn.query_row("SELECT id FROM work WHERE volume = ?1", [volume], |row| {
        row.get(0)
    })
    .unwrap_or_else(|e| panic!("the work of volume {volume}: {e}"))
}

/// The one series a fixture holds.
fn the_series(conn: &Connection) -> i64 {
    conn.query_row("SELECT id FROM series", [], |row| row.get(0))
        .expect("the fixture's one series")
}

fn hand_set_cover(conn: &Connection, series_id: i64, work_id: i64) {
    conn.execute(
        "UPDATE series SET cover_work_id = ?1 WHERE id = ?2",
        [work_id, series_id],
    )
    .expect("hand-sets the cover");
}

/// The grid lists `want`'s series in its order, each with the cover work and
/// edition `want` names, and each cover is the one `cover_work` gives the `307`
/// for the same caller.
///
/// Both, because the grid and the `307` read one fragment: agreeing with
/// `cover_work` alone passes when the fragment itself chooses wrongly, and the
/// expected pair alone passes when the two read different rules that happen to
/// meet on a fixture.
fn assert_grid_covers(conn: &Connection, access: &Access, want: &[(i64, Option<(i64, &str)>)]) {
    let grid = series(conn, access, 1, None).expect("lists series");
    assert_eq!(
        grid.iter().map(|card| card.id).collect::<Vec<_>>(),
        want.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        "{access:?}: the series listed"
    );
    for (card, (id, expected)) in grid.iter().zip(want) {
        let chosen = card
            .cover
            .as_ref()
            .map(|cover| (cover.work_id, cover.edition.as_str()));
        assert_eq!(
            chosen, *expected,
            "series {id}, {access:?}: the grid's cover"
        );
        let redirect = cover_work(conn, access, *id).expect("reads the 307's cover");
        assert_eq!(
            card.cover, redirect,
            "series {id}, {access:?}: the grid and the 307 choose one cover"
        );
    }
}

/// A hand-set cover on volume 2 is the grid's cover and the `307`'s, where
/// the order alone would choose volume 1.
#[test]
fn a_hand_set_cover_is_the_grids_cover_as_it_is_the_307s() {
    let (library, root_id, _dir) = two_volume_library();
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));

    for access in [Access::Every, Access::roots(vec![root_id])] {
        assert_grid_covers(&conn, &access, &[(series_id, Some((v1, "raw")))]);
    }
    hand_set_cover(&conn, series_id, v2);
    for access in [Access::Every, Access::roots(vec![root_id])] {
        assert_grid_covers(&conn, &access, &[(series_id, Some((v2, "raw")))]);
    }
}

/// A hand-set cover whose work the grant hides gives way to the lowest volume
/// the grant shows, in the grid as in the `307`.
#[test]
fn a_hand_set_cover_the_grant_hides_gives_way_in_the_grid_as_in_the_307() {
    let (library, first, second, _a, _b) = two_volumes_across_two_roots();
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));

    assert_grid_covers(&conn, &Access::Every, &[(series_id, Some((v1, "raw")))]);
    assert_grid_covers(
        &conn,
        &Access::roots(vec![second]),
        &[(series_id, Some((v2, "raw")))],
    );
    hand_set_cover(&conn, series_id, v2);
    assert_grid_covers(&conn, &Access::Every, &[(series_id, Some((v2, "raw")))]);
    assert_grid_covers(
        &conn,
        &Access::roots(vec![first]),
        &[(series_id, Some((v1, "raw")))],
    );
    assert_grid_covers(
        &conn,
        &Access::roots(vec![second]),
        &[(series_id, Some((v2, "raw")))],
    );
}

/// A volume 1 scanned after volume 2 holds the higher `work.id` and is still
/// the cover, in the grid as in the `307`: the order is by `volume` first.
#[test]
fn a_lower_volume_added_after_is_the_cover_in_the_grid_as_in_the_307() {
    let (library, _dir) = a_lower_volume_added_after();
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    assert!(
        v1 > v2,
        "the fixture's premise: volume 1, added after, holds the higher id ({v1} against {v2})"
    );

    assert_grid_covers(&conn, &Access::Every, &[(series_id, Some((v1, "raw")))]);
}

/// Of a work's several editions the grid takes the first by key that the
/// grant shows, as the `307` does, with a hand-set cover or without one.
#[test]
fn the_grid_takes_the_edition_the_307_takes_of_a_work_with_several() {
    let (library, _en, _raw) = library_with_volumes(2);
    let conn = library.conn();
    let root = |key: &str| -> i64 {
        conn.query_row("SELECT id FROM root WHERE edition_key = ?1", [key], |row| {
            row.get(0)
        })
        .expect("the directory of that edition")
    };
    let (en, raw) = (root("en"), root("raw"));
    let series_id = the_series(&conn);
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));

    for (access, key) in [
        (Access::Every, "en"),
        (Access::roots(vec![en]), "en"),
        (Access::roots(vec![raw]), "raw"),
    ] {
        assert_grid_covers(&conn, &access, &[(series_id, Some((v1, key)))]);
    }
    hand_set_cover(&conn, series_id, v2);
    for (access, key) in [
        (Access::Every, "en"),
        (Access::roots(vec![en]), "en"),
        (Access::roots(vec![raw]), "raw"),
    ] {
        assert_grid_covers(&conn, &access, &[(series_id, Some((v2, key)))]);
    }
}

/// A series with no edition holding a member the caller may see has no cover:
/// not listed under a grant that hides it, and listed with none where every
/// edition it holds has lost its members, and the `307` answers none for both.
#[test]
fn a_series_with_no_visible_cover_has_none_in_the_grid_as_in_the_307() {
    let store = two_library_store();
    let manga = series_of(&store.library, store.manga);
    let comics = series_of(&store.library, store.comics);
    let conn = store.library.conn();
    assert_eq!(manga.len(), 1, "Manga holds one series");
    let shown = Access::roots(vec![store.en_root]);
    let grid = series(&conn, &shown, 1, None).expect("lists series");
    assert_eq!(
        grid.iter().map(|card| card.id).collect::<Vec<_>>(),
        manga,
        "only Manga's series is listed"
    );
    assert!(grid[0].cover.is_some(), "the listed series has a cover");
    for id in comics {
        assert_eq!(
            cover_work(&conn, &shown, id).expect("reads"),
            None,
            "series {id} is hidden, and so is its cover"
        );
    }

    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let series_id = the_series(&conn);
    conn.execute("DELETE FROM member", [])
        .expect("every edition loses its members");
    assert_grid_covers(&conn, &Access::Every, &[(series_id, None)]);
}

/// The members of the fixture's one edition, by `ordinal`: id, root, sha256.
fn members_by_ordinal(conn: &Connection) -> Vec<(i64, i64, String)> {
    conn.prepare("SELECT id, root_id, sha256 FROM member ORDER BY ordinal")
        .expect("prepares")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads")
}

/// The sha256 of the member the page routes read edition `key`'s page 0 from:
/// `Edition::locate` over the projected sidecar, which is what the served
/// registry holds.
fn page_zero_as_located(conn: &Connection, work_id: i64, key: &str) -> Option<String> {
    let sidecar = crate::project::sidecar(conn, work_id).expect("projects the work");
    let edition = &sidecar.editions[key];
    edition
        .locate(0)
        .map(|(position, _)| edition.members[position].sha256.clone())
}

/// Every place an answer reads page 0's member from, for the fixture's one
/// series, one work and one edition: the edition card, the grid's cover and
/// the `307`'s.
fn page_zero_read(conn: &Connection, access: &Access) -> [Option<String>; 3] {
    let slug: String = conn
        .query_row("SELECT slug FROM work", [], |row| row.get(0))
        .expect("the one work");
    let detail = work(conn, access, 1, &slug)
        .expect("reads the work")
        .expect("the work is visible");
    assert_eq!(detail.card.editions.len(), 1, "one edition");
    let grid = series(conn, access, 1, None).expect("lists series");
    assert_eq!(grid.len(), 1, "one series");
    let cover = cover_work(conn, access, grid[0].id)
        .expect("reads the cover")
        .expect("a cover");
    [
        detail.card.editions[0].page_zero_sha256.clone(),
        grid[0]
            .cover
            .as_ref()
            .expect("the grid's cover")
            .page_zero_sha256
            .clone(),
        cover.page_zero_sha256,
    ]
}

/// An edition whose first member holds no pages takes page 0, and so its
/// cover's version, from the next member, as `Edition::locate` does; and one
/// none of whose members holds a page has no page 0 at all.
#[test]
fn an_edition_whose_first_member_has_no_pages_takes_page_zero_from_the_next() {
    let (library, _a, _b, _da, _db) = merged_edition_library();
    let conn = library.conn();
    let members = members_by_ordinal(&conn);
    assert_eq!(members.len(), 2, "the merged edition's two members");
    conn.execute("UPDATE member SET pages = 0 WHERE id = ?1", [members[0].0])
        .expect("the first member holds no pages");
    let (work_id, key): (i64, String) = conn
        .query_row("SELECT work_id, key FROM edition", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("the one edition");
    let located = page_zero_as_located(&conn, work_id, &key);
    assert_eq!(
        located.as_deref(),
        Some(members[1].2.as_str()),
        "the fixture's premise: the page routes read page 0 from the second member"
    );

    assert_eq!(
        page_zero_read(&conn, &Access::Every),
        [located.clone(), located.clone(), located],
        "the edition card, the grid's cover and the 307's"
    );

    conn.execute("UPDATE member SET pages = 0", [])
        .expect("no member holds a page");
    assert_eq!(page_zero_as_located(&conn, work_id, &key), None);
    assert_eq!(page_zero_read(&conn, &Access::Every), [None, None, None]);
}

/// Page 0 is read from the members the grant shows, as the page routes
/// narrow a merged edition to them (spec §3.8), so a reader granted only the
/// directory of its second member is handed the version of that member's
/// first page: the bytes the thumbnail route serves them.
#[test]
fn page_zero_is_read_from_the_members_the_grant_shows() {
    let (library, _a, _b, _da, _db) = merged_edition_library();
    let conn = library.conn();
    let members = members_by_ordinal(&conn);
    assert_eq!(members.len(), 2, "the merged edition's two members");
    assert_ne!(members[0].1, members[1].1, "under two directories");
    assert_ne!(members[0].2, members[1].2, "holding different bytes");
    let (work_id, key): (i64, String) = conn
        .query_row("SELECT work_id, key FROM edition", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("the one edition");
    let located = page_zero_as_located(&conn, work_id, &key);
    assert_eq!(located.as_deref(), Some(members[0].2.as_str()));

    for (access, member) in [
        (Access::Every, 0),
        (Access::roots(vec![members[0].1]), 0),
        (Access::roots(vec![members[1].1]), 1),
    ] {
        let seen = Some(members[member].2.clone());
        assert_eq!(
            page_zero_read(&conn, &access),
            [seen.clone(), seen.clone(), seen],
            "{access:?}: the edition card, the grid's cover and the 307's"
        );
    }
}

/// A listing narrowed to a library reads page 0 through the account's grant,
/// not through that library's directories: the thumbnail route narrows by the
/// grant alone, so an account that sees the whole edition is served its first
/// member's page 0 whichever listing it came from.
#[test]
fn a_listing_narrowed_to_a_library_reads_page_zero_through_the_accounts_grant() {
    let (library, a_id, b_id, _da, _db) = merged_edition_library();
    let second = raw_in_a_library_of_its_own(&library, b_id);
    let conn = library.conn();
    let members = members_by_ordinal(&conn);
    assert_eq!(
        members.iter().map(|m| m.1).collect::<Vec<_>>(),
        [a_id, b_id],
        "the fixture: the second member alone is in the library listed"
    );
    let series_id = the_series(&conn);
    let first = Some(members[0].2.clone());
    let manga: i64 = crate::test_support::library_of_root(&conn, a_id);
    let both = Access::Only {
        roots: vec![a_id, b_id],
        libraries: vec![manga, second],
    };
    for access in [Access::Every, both] {
        let grid = series(&conn, &access, 1, Some(second)).expect("lists series");
        let (row, works) = one_series(&conn, &access, 1, series_id, Some(second))
            .expect("reads the series")
            .expect("the series is in the library");
        let added = home(&conn, &access, 1, Some(second))
            .expect("reads home")
            .added;
        let hits = search(&conn, &access, 1, "Series", Some(second)).expect("searches");
        let cover = |card: &SeriesCard| {
            card.cover
                .as_ref()
                .expect("a cover")
                .page_zero_sha256
                .clone()
        };
        assert_eq!(
            [
                cover(&grid[0]),
                cover(&row),
                cover(&hits.series[0]),
                works[0].editions[0].page_zero_sha256.clone(),
                added[0].editions[0].page_zero_sha256.clone(),
            ],
            [
                first.clone(),
                first.clone(),
                first.clone(),
                first.clone(),
                first.clone()
            ],
            "{access:?}: the grid's row, the series page's row, the search hit, \
             and the edition cards of the series page and home's added"
        );
    }
}

/// Each edition card of each work names the member its page 0 is located in:
/// over two works of two editions each, so a hash read from another edition
/// or work is a different one, and over an edition of two members, so one read
/// from the wrong member is too.
#[test]
fn every_edition_card_names_the_member_its_page_zero_is_located_in() {
    let (library, _en, _raw) = library_with_volumes(2);
    assert_each_edition_card_names_its_page_zero(&library, 2, 2);
    let (merged, _a, _b, _da, _db) = merged_edition_library();
    assert_each_edition_card_names_its_page_zero(&merged, 1, 1);
    let conn = merged.conn();
    assert_eq!(
        members_by_ordinal(&conn).len(),
        2,
        "the merged edition's two members"
    );
}

fn assert_each_edition_card_names_its_page_zero(library: &Library, works: usize, editions: usize) {
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (_card, cards) = one_series(&conn, &Access::Every, 1, series_id, None)
        .expect("reads")
        .expect("the series is visible");
    assert_eq!(cards.len(), works, "the works");
    for card in &cards {
        assert_eq!(
            card.editions.len(),
            editions,
            "the editions of {}",
            card.slug
        );
        for edition in &card.editions {
            let located = page_zero_as_located(&conn, card.id, &edition.key);
            assert!(
                located.is_some(),
                "{} {} has a page 0",
                card.slug,
                edition.key
            );
            assert_eq!(
                edition.page_zero_sha256, located,
                "{} {}: the member page 0 is located in",
                card.slug, edition.key
            );
        }
    }
}

/// A grid narrowed to a library takes its cover from what that library's
/// directories show, as `cover_work` chooses it for an account granted only
/// them: the narrowing's rule for every count on the row.
///
/// The one work keeps `en` under the English directory's library and `raw`
/// under `Raws`, so the unnarrowed grid's cover is `en`, by key, and the one
/// narrowed to `Raws` is `raw`.
#[test]
fn a_grid_narrowed_to_a_library_takes_the_cover_its_directories_show() {
    let (library, _en_id, raw_id, _en, _raw) = two_root_library();
    let raws = raw_in_a_library_of_its_own(&library, raw_id);
    let conn = library.conn();
    let series_id = the_series(&conn);
    let work_id = work_of_volume(&conn, 1.0);

    assert_grid_covers(&conn, &Access::Every, &[(series_id, Some((work_id, "en")))]);
    let narrowed = series(&conn, &Access::Every, 1, Some(raws)).expect("lists series");
    assert_eq!(narrowed.len(), 1, "the series is in Raws too");
    assert_eq!(
        narrowed[0].cover,
        cover_work(&conn, &Access::roots(vec![raw_id]), series_id).expect("reads the cover"),
        "the cover an account granted only Raws' directory is given"
    );
    assert_eq!(
        narrowed[0]
            .cover
            .as_ref()
            .map(|cover| cover.edition.as_str()),
        Some("raw")
    );
}

/// Two works of one series sharing a volume, or both without one, are ordered
/// by `work.id` before their editions' keys, in the grid as in the `307`.
///
/// The lower id's only edition is keyed `raw` and the higher's `en`, so an
/// order that consulted the key first, or never reached the id, would choose
/// the other work.
#[test]
fn works_sharing_a_volume_take_the_cover_by_id_before_key() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (first, second) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    assert!(
        first < second,
        "the fixture's premise: volume 1 has the lower id"
    );
    conn.execute("UPDATE edition SET key = 'en' WHERE work_id = ?1", [second])
        .expect("keys the higher id's edition before the lower's");

    for volume in [Some(1.0), None] {
        conn.execute("UPDATE work SET volume = ?1", [volume])
            .expect("gives both works one volume");
        assert_grid_covers(&conn, &Access::Every, &[(series_id, Some((first, "raw")))]);
    }
}

/// One work whose edition `en` is under `Kids`' directory and whose edition
/// `raw` is under `Manga`'s, and a series `Other` under a third library's
/// directory, scanned and derived. The libraries are numbered 70, 80 and 90,
/// apart from the directories.
struct KidsAndManga {
    library: Library,
    kids: i64,
    manga: i64,
    other: i64,
    slug: String,
    _dirs: [crate::tests::TempDir; 3],
}

fn kids_and_manga() -> KidsAndManga {
    let kids = crate::tests::tempdir("browse-kids");
    write_pages(&kids.path().join("Series").join("Series - v01"), 2);
    let manga = crate::tests::tempdir("browse-manga");
    write_pages(&manga.path().join("Series").join("Series - v01"), 2);
    let other = crate::tests::tempdir("browse-other");
    write_pages(&other.path().join("Other").join("Other - v01"), 3);

    let library = Library::open_in_memory().expect("in-memory");
    let (kids_root, manga_root, other_root, slug) = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'Kids'), (80, 'Manga'), (90, 'Other')",
            [],
        )
        .expect("three libraries, numbered apart from their directories");
        let add = |dir: &crate::tests::TempDir, name: &str, edition: &str| -> i64 {
            let id = scan::add_root_in(&conn, dir.path(), scan::InLibrary::Named(name))
                .expect("adds the directory")
                .id;
            conn.execute(
                "UPDATE root SET edition_key = ?1 WHERE id = ?2",
                rusqlite::params![edition, id],
            )
            .expect("names the directory's edition");
            id
        };
        let k = add(&kids, "Kids", "en");
        let m = add(&manga, "Manga", "raw");
        let o = add(&other, "Other", "raw");
        for root in scan::roots(&conn).expect("re-reads the roots") {
            scan::scan_root(&conn, &root).expect("scans");
        }
        derive::derive(&conn).expect("derives");
        let slug: String = conn
            .query_row(
                "SELECT w.slug FROM work w JOIN series s ON s.id = w.series_id
                  WHERE s.sort_title = 'Series'",
                [],
                |r| r.get(0),
            )
            .expect("the fixture: one work of `Series`, holding both editions");
        (k, m, o, slug)
    };
    KidsAndManga {
        library,
        kids: kids_root,
        manga: manga_root,
        other: other_root,
        slug,
        _dirs: [kids, manga, other],
    }
}

/// Spec §7, "Access": an account granted one of two libraries holding a
/// work's editions is listed the work with that library's edition alone, and
/// is refused the other edition's pages.
#[test]
fn an_account_granted_one_library_sees_the_work_with_that_librarys_edition_alone() {
    let s = kids_and_manga();
    let conn = s.library.conn();
    for (root, granted, withheld) in [(s.kids, "en", "raw"), (s.manga, "raw", "en")] {
        let access = Access::roots(vec![root]);
        let titles: Vec<String> = series(&conn, &access, 1, None)
            .expect("lists")
            .into_iter()
            .map(|card| {
                assert_eq!(
                    (card.works, card.editions),
                    (1, 1),
                    "{granted}: the work, counted with one edition"
                );
                card.title
            })
            .collect();
        assert_eq!(
            titles,
            ["Series"],
            "{granted}: the series of the work alone"
        );
        let keys: Vec<String> = work(&conn, &access, 1, &s.slug)
            .expect("reads")
            .expect("the work is listed")
            .card
            .editions
            .into_iter()
            .map(|e| e.key)
            .collect();
        assert_eq!(keys, [granted], "the granted library's edition alone");
        assert!(sees_edition(&conn, &access, &s.slug, granted));
        assert!(
            !sees_edition(&conn, &access, &s.slug, withheld),
            "{granted}: the other library's edition's pages are refused"
        );
    }
}

/// And an account granted neither library sees nothing of the work.
#[test]
fn an_account_granted_neither_library_sees_nothing_of_the_work() {
    let s = kids_and_manga();
    let conn = s.library.conn();
    let access = Access::roots(vec![s.other]);
    let titles: Vec<String> = series(&conn, &access, 1, None)
        .expect("lists")
        .into_iter()
        .map(|card| card.title)
        .collect();
    assert_eq!(titles, ["Other"], "the granted library's own series alone");
    assert!(work(&conn, &access, 1, &s.slug).expect("reads").is_none());
    for key in ["en", "raw"] {
        assert!(!sees_edition(&conn, &access, &s.slug, key), "{key}");
    }
    assert!(
        !visible_editions(&conn, &access)
            .expect("lists")
            .contains_key(&s.slug)
    );
}

/// Spec §7, "Access": a copy held under two libraries' directories is one
/// member, read through either library's grant alone, counted there with its
/// member and its pages, and filed in both.
#[test]
fn a_copy_held_in_two_libraries_is_visible_through_either_grant_alone() {
    let copy = crate::tests::a_volume_copied_into_two_libraries();
    let conn = copy.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work", [], |r| r.get(0))
        .expect("the one work");
    for (root, label) in [(copy.a.id, "a"), (copy.b.id, "b")] {
        let access = Access::roots(vec![root]);
        let cards = series(&conn, &access, 1, None).expect("lists");
        assert_eq!(
            cards
                .iter()
                .map(|c| (c.works, c.editions))
                .collect::<Vec<_>>(),
            [(1, 1)],
            "{label}: the work's series, counting the work and its edition"
        );
        let editions = work(&conn, &access, 1, &slug)
            .expect("reads")
            .expect("the work is listed")
            .card
            .editions;
        assert_eq!(
            editions
                .iter()
                .map(|e| (e.key.as_str(), e.members, e.pages))
                .collect::<Vec<_>>(),
            [("raw", 1, 3)],
            "{label}: the edition, counted with its one member and its pages"
        );
        assert!(
            sees_edition(&conn, &access, &slug, "raw"),
            "{label}: its pages are served"
        );
    }
    for library in [70, 80] {
        assert_eq!(
            held_in(&conn, library).expect("reads"),
            HashSet::from([slug.clone()]),
            "held in library {library}"
        );
    }
}

/// Series `Foo`, volumes 1 to 4 under `Manga`'s directory and volume 5 under
/// `Kids`', and a series `Bar` under `Manga`'s alone, scanned and derived:
/// spec §3.6's case. The libraries are numbered 70 and 80, apart from the
/// directories.
struct FooAcrossTwoLibraries {
    library: Library,
    manga: i64,
    kids: i64,
    manga_root: i64,
    kids_root: i64,
    foo: i64,
    bar: i64,
    _dirs: [crate::tests::TempDir; 2],
}

fn foo_across_two_libraries() -> FooAcrossTwoLibraries {
    let manga = crate::tests::tempdir("browse-foo-manga");
    for volume in 1..=4 {
        write_pages(
            &manga.path().join("Foo").join(format!("Foo - v{volume:02}")),
            2,
        );
    }
    write_pages(&manga.path().join("Bar").join("Bar - v01"), 2);
    let kids = crate::tests::tempdir("browse-foo-kids");
    write_pages(&kids.path().join("Foo").join("Foo - v05"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    let (manga_root, kids_root, foo, bar) = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'Manga'), (80, 'Kids')",
            [],
        )
        .expect("two libraries, numbered apart from their directories");
        let manga_root = scan::add_root_in(&conn, manga.path(), scan::InLibrary::Named("Manga"))
            .expect("adds Manga's directory")
            .id;
        let kids_root = scan::add_root_in(&conn, kids.path(), scan::InLibrary::Named("Kids"))
            .expect("adds Kids' directory")
            .id;
        for root in scan::roots(&conn).expect("re-reads the roots") {
            scan::scan_root(&conn, &root).expect("scans");
        }
        derive::derive(&conn).expect("derives");
        let titled = |title: &str| -> i64 {
            conn.query_row(
                "SELECT id FROM series WHERE sort_title = ?1",
                [title],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("the series {title}: {e}"))
        };
        (manga_root, kids_root, titled("Foo"), titled("Bar"))
    };
    FooAcrossTwoLibraries {
        library,
        manga: 70,
        kids: 80,
        manga_root,
        kids_root,
        foo,
        bar,
        _dirs: [manga, kids],
    }
}

/// The volumes of `works`, in the order listed.
fn volumes(works: &[WorkCard]) -> Vec<Option<f64>> {
    works.iter().map(|w| w.volume).collect()
}

/// Spec §3.6: for an account granted both libraries, the series page opened
/// from `Kids`' grid counts and lists the one volume `Kids`' directory holds,
/// as the tile it was opened from does, and the page asked with no library
/// lists all five.
#[test]
fn a_series_opened_from_a_library_holds_what_that_librarys_directories_hold() {
    let s = foo_across_two_libraries();
    let conn = s.library.conn();
    let both = Access::Only {
        roots: vec![s.manga_root, s.kids_root],
        libraries: vec![s.manga, s.kids],
    };

    let tile = series(&conn, &both, 1, Some(s.kids))
        .expect("lists Kids' grid")
        .into_iter()
        .find(|c| c.id == s.foo)
        .expect("Foo is in Kids' grid");
    assert_eq!(tile.works, 1, "Kids' tile counts the volume Kids holds");

    for (library, works, listed) in [
        (Some(s.kids), 1, vec![Some(5.0)]),
        (
            Some(s.manga),
            4,
            vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0)],
        ),
        (
            None,
            5,
            vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0), Some(5.0)],
        ),
    ] {
        let (card, cards) = one_series(&conn, &both, 1, s.foo, library)
            .expect("reads the series")
            .unwrap_or_else(|| panic!("Foo is found in {library:?}"));
        assert_eq!(card.works, works, "the card's count in {library:?}");
        assert_eq!(volumes(&cards), listed, "the works listed in {library:?}");
    }
    let (card, _) = one_series(&conn, &both, 1, s.foo, Some(s.kids))
        .expect("reads the series")
        .expect("Foo is in Kids");
    assert_eq!(card, tile, "the page's card is the tile it was opened from");
}

/// A library outside the grant, a library holding nothing of the series and a
/// library that does not exist each answer as a series that does not exist,
/// while the same series asked with a library that holds it, or with none, is
/// found.
#[test]
fn a_series_asked_for_in_a_library_that_holds_none_of_it_is_not_found() {
    let s = foo_across_two_libraries();
    let conn = s.library.conn();
    let manga_only = Access::Only {
        roots: vec![s.manga_root],
        libraries: vec![s.manga],
    };
    let both = Access::Only {
        roots: vec![s.manga_root, s.kids_root],
        libraries: vec![s.manga, s.kids],
    };
    let found = |access: &Access, id: i64, library: Option<i64>| {
        one_series(&conn, access, 1, id, library)
            .expect("reads the series")
            .is_some()
    };

    assert!(found(&manga_only, s.foo, Some(s.manga)), "Manga, granted");
    assert!(
        !found(&manga_only, s.foo, Some(s.kids)),
        "Kids, outside the grant"
    );
    assert!(found(&both, s.bar, None), "Bar, with no library");
    assert!(!found(&both, s.bar, Some(s.kids)), "Bar, in Kids");
    assert!(
        !found(&both, s.foo, Some(999)),
        "a library that does not exist"
    );
}

// --- A missing work is in no listing (pre-deploy spec §5) --------------------

/// Marks work `work_id` missing, as a derive does once no directory holds any
/// of its members.
fn mark_missing_by_hand(conn: &Connection, work_id: i64) {
    conn.execute(
        "UPDATE work SET missing_since = datetime('now') WHERE id = ?1",
        [work_id],
    )
    .expect("marks the work missing");
}

/// The ids of `works`, in the order listed.
fn card_ids(works: &[WorkCard]) -> Vec<i64> {
    works.iter().map(|w| w.id).collect()
}

/// A position on `work_id`'s `raw` edition for reader 1, finished or not.
fn position_on(conn: &Connection, work_id: i64, completed: bool) {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at)
         VALUES (1, ?1, 'raw', 0, '2026-08-19T00:00:00Z',
                 CASE WHEN ?2 THEN '2026-08-19T00:00:00Z' END)",
        rusqlite::params![work_id, completed],
    )
    .expect("a position");
}

/// One directory holding `Series` in two volumes and `Other` in one.
fn series_and_other() -> (Library, crate::tests::TempDir) {
    let dir = crate::tests::tempdir("browse-missing-two-series");
    write_pages(&dir.path().join("Series").join("Series - v01"), 2);
    write_pages(&dir.path().join("Series").join("Series - v02"), 2);
    write_pages(&dir.path().join("Other").join("Other - v01"), 2);
    let library = Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("adds the root");
        scan::scan_root(&conn, &root).expect("scans");
        derive::derive(&conn).expect("derives");
    }
    (library, dir)
}

/// A series' card counts only its present works, and a series whose one
/// volume is missing is not listed, while the series beside it is.
#[test]
fn the_grid_counts_no_missing_work_and_lists_no_series_of_missing_works_only() {
    let (library, _dir) = series_and_other();
    let conn = library.conn();
    let in_series = |sort_title: &str, volume: f64| -> i64 {
        conn.query_row(
            "SELECT w.id FROM work w JOIN series s ON s.id = w.series_id
              WHERE s.sort_title = ?1 AND w.volume = ?2",
            rusqlite::params![sort_title, volume],
            |row| row.get(0),
        )
        .expect("the volume's work")
    };
    mark_missing_by_hand(&conn, in_series("Series", 2.0));
    mark_missing_by_hand(&conn, in_series("Other", 1.0));

    let grid: Vec<(String, i64)> = series(&conn, &Access::Every, 1, None)
        .expect("lists series")
        .into_iter()
        .map(|card| (card.title, card.works))
        .collect();

    assert_eq!(
        grid,
        [("Series".to_string(), 1)],
        "Series with its one present volume; Other, whose only volume is missing, is gone"
    );
}

/// A hand-set cover on a missing work gives way to the series' present
/// volume, in the grid as in the `307`.
#[test]
fn a_hand_set_cover_on_a_missing_work_gives_way_to_a_present_one() {
    let (library, root_id, _dir) = two_volume_library();
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    hand_set_cover(&conn, series_id, v2);
    mark_missing_by_hand(&conn, v2);

    for access in [Access::Every, Access::roots(vec![root_id])] {
        assert_grid_covers(&conn, &access, &[(series_id, Some((v1, "raw")))]);
    }
}

/// A series page lists its present volume and not its missing one.
#[test]
fn a_series_page_lists_no_missing_work() {
    let (library, _, _dir) = two_volume_library();
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    mark_missing_by_hand(&conn, v2);

    let (_card, works) = one_series(&conn, &Access::Every, 1, series_id, None)
        .expect("reads the series")
        .expect("the series is listed");

    assert_eq!(card_ids(&works), [v1], "volume 1 only");
}

/// A series whose every work is missing has no page: `one_series` answers
/// `None`, as for a series that does not exist, while the series beside it
/// still has one. Before the mark, both have a page.
#[test]
fn a_series_of_missing_works_only_has_no_page() {
    let (library, _dir) = series_and_other();
    let conn = library.conn();
    let id_of = |sort_title: &str| -> i64 {
        conn.query_row(
            "SELECT id FROM series WHERE sort_title = ?1",
            [sort_title],
            |row| row.get(0),
        )
        .expect("the series")
    };
    let (series_id, other_id) = (id_of("Series"), id_of("Other"));
    let has_page = |id: i64| {
        one_series(&conn, &Access::Every, 1, id, None)
            .expect("reads the series")
            .is_some()
    };
    assert_eq!(
        (has_page(series_id), has_page(other_id)),
        (true, true),
        "the fixture"
    );
    let other_work: i64 = conn
        .query_row(
            "SELECT id FROM work WHERE series_id = ?1",
            [other_id],
            |row| row.get(0),
        )
        .expect("Other's one work");

    mark_missing_by_hand(&conn, other_work);

    assert_eq!(
        (has_page(series_id), has_page(other_id)),
        (true, false),
        "Series keeps its page; Other, whose only work is missing, has none"
    );
}

/// The unfiled works leave a missing one out. No derive marks an imported
/// work, so the mark here is by hand (the lane plan's Spec gaps, 3).
#[test]
fn the_unfiled_works_list_no_missing_work() {
    let library = Library::open_in_memory().expect("in-memory");
    import_a_work(&library, "Kept", "/kept/taiyaku.json");
    import_a_work(&library, "Gone", "/gone/taiyaku.json");
    let conn = library.conn();
    let gone: i64 = conn
        .query_row("SELECT id FROM work WHERE title = 'Gone'", [], |row| {
            row.get(0)
        })
        .expect("the work to mark");
    mark_missing_by_hand(&conn, gone);

    let titles: Vec<String> = unfiled(&conn, &Access::Every, 1, None)
        .expect("lists the unfiled works")
        .into_iter()
        .map(|w| w.title)
        .collect();

    assert_eq!(titles, ["Kept"], "the present work only");
}

/// A missing work with an open position is not in `continuing`.
#[test]
fn continuing_lists_no_missing_work() {
    let (library, _, _dir) = two_volume_library();
    let conn = library.conn();
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    position_on(&conn, v1, false);
    position_on(&conn, v2, false);
    mark_missing_by_hand(&conn, v2);

    let home = home(&conn, &Access::Every, 1, None).expect("reads home");

    assert_eq!(card_ids(&home.continuing), [v1], "volume 1 only");
}

/// A missing work with a finished position is not in `read`.
#[test]
fn read_lists_no_missing_work() {
    let (library, _, _dir) = two_volume_library();
    let conn = library.conn();
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    position_on(&conn, v1, true);
    position_on(&conn, v2, true);
    mark_missing_by_hand(&conn, v2);

    let home = home(&conn, &Access::Every, 1, None).expect("reads home");

    assert_eq!(card_ids(&home.read), [v1], "volume 1 only");
}

/// A missing work, however new, is not in `added`.
#[test]
fn added_lists_no_missing_work() {
    let (library, _, _dir) = two_volume_library();
    let conn = library.conn();
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    mark_missing_by_hand(&conn, v2);

    let home = home(&conn, &Access::Every, 1, None).expect("reads home");

    assert_eq!(card_ids(&home.added), [v1], "volume 1 only");
}

/// A missing work's title finds no series, by the index or by `LIKE`, while
/// the present work's still finds its own. Before the mark, both queries
/// find the series, so neither empty answer is the fixture's.
#[test]
fn a_missing_works_title_finds_no_series() {
    let (library, _, _dir) = two_volume_library();
    let conn = library.conn();
    let series_id = the_series(&conn);
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    for (id, title) in [(v1, "Alpha"), (v2, "Zebra")] {
        conn.execute(
            "UPDATE work SET title = ?1 WHERE id = ?2",
            rusqlite::params![title, id],
        )
        .expect("titles the work");
    }
    let found = |q: &str| -> Vec<i64> {
        search(&conn, &Access::Every, 1, q, None)
            .expect("searches")
            .series
            .into_iter()
            .map(|card| card.id)
            .collect()
    };
    assert_eq!(
        (found("Zebra"), found("Ze")),
        (vec![series_id], vec![series_id]),
        "the fixture: before the mark, both queries find the series"
    );

    mark_missing_by_hand(&conn, v2);

    assert_eq!(found("Zebra"), Vec::<i64>::new(), "by the index");
    assert_eq!(found("Ze"), Vec::<i64>::new(), "by `LIKE`");
    assert_eq!(found("Alpha"), [series_id], "the present work's title");
}

/// `visible_editions` names no missing work, for every access, while it names
/// the present one.
#[test]
fn visible_editions_names_no_missing_work() {
    let (library, root_id, _dir) = two_volume_library();
    let conn = library.conn();
    let (v1, v2) = (work_of_volume(&conn, 1.0), work_of_volume(&conn, 2.0));
    let slug = |id: i64| -> String {
        conn.query_row("SELECT slug FROM work WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .expect("the work's slug")
    };
    mark_missing_by_hand(&conn, v2);

    let named: Vec<Vec<String>> = [Access::Every, Access::roots(vec![root_id])]
        .iter()
        .map(|access| {
            let mut slugs: Vec<String> = visible_editions(&conn, access)
                .expect("lists editions")
                .into_keys()
                .collect();
            slugs.sort();
            slugs
        })
        .collect();

    assert_eq!(
        named,
        [[slug(v1)], [slug(v1)]],
        "volume 1 only, for every access and for a grant"
    );
}
