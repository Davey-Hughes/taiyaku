use super::*;
use crate::scan::tests::write_pages;
use crate::{Library, Progress, derive, scan, state};

/// Two roots, one series, one volume in each — the shape spec C1 §5.1 is
/// designed around, and the only one where the filter can be seen working.
fn two_root_library() -> (
    Library,
    i64,
    i64,
    crate::tests::TempDir,
    crate::tests::TempDir,
) {
    let en = crate::tests::tempdir("browse-en");
    write_pages(&en.path.join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("browse-raw");
    write_pages(&raw.path.join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    let (en_id, raw_id) = {
        let conn = library.conn();
        let en_root = scan::add_root(&conn, &en.path).expect("adds en");
        let raw_root = scan::add_root(&conn, &raw.path).expect("adds raw");
        // **Each root declares its own edition key**, or both default to
        // `raw` and derivation merges their members into ONE edition — which
        // is correct behaviour and makes this fixture unable to show a filter
        // hiding one edition and keeping another. Found by the first run.
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

/// An imported work — one with a `sidecar_path` and no series.
///
/// **The digest is derived from the title**, because `import::sidecar`
/// reconciles by `content_key` first: two works whose members share a sha256
/// are one work, and two calls to this helper with one constant digest
/// produced a single row. Found by the first run of
/// `continuing_is_ordered_by_when_each_work_was_last_read`.
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
    write_pages(&dir.path.join("Series").join("Series - v01"), 2);
    write_pages(&dir.path.join("Series").join("Series - v02"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    let root_id = {
        let conn = library.conn();
        let root = scan::add_root(&conn, &dir.path).expect("adds the root");
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
fn an_empty_grant_sees_every_root() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[], 1, None).expect("lists");
    assert_eq!(cards.len(), 1, "one series");
    assert_eq!(cards[0].editions, 2, "both editions are visible");
    assert_eq!(cards[0].works, 1, "one volume, held in two roots");
}

#[test]
fn a_grant_hides_the_editions_it_does_not_name_and_keeps_the_work() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[en_id], 1, None).expect("lists");
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
        series(&conn, &[], 1, None).expect("lists").len(),
        1,
        "an import creates no series, so the grid still holds one"
    );
    assert_eq!(
        unfiled(&conn, &[], None).expect("lists").len(),
        1,
        "unrestricted sees the imported work in the unfiled bucket"
    );
    assert!(
        unfiled(&conn, &[en_id], None).expect("lists").is_empty(),
        "a restricted account sees no work that sits under no root"
    );
}

/// O64. Contrast `an_imported_work_is_unfiled_rather_than_a_series_of_its_own`
/// directly above: a member genuinely new to the document has no held row and
/// correctly gets `root_id` NULL, but a member `two_root_library`'s own scan
/// already stamped is not new — a `library import` over the same work (the
/// route a hand-corrected `taiyaku.json` reimport opens) must not blank it.
///
/// Reproduces the measured symptom directly: `import.rs`'s `replace_editions`
/// deletes and reinserts every member under `Scope::Document`, naming no
/// `root_id`, so every fresh row lands NULL — and nothing re-stamps it,
/// because `derive.rs`'s `write_roots` runs only on the derive path, which a
/// plain `library import` never takes. `browse.rs`'s `IN (…)` filter then
/// excludes the NULL member by SQL's own rules — correct behaviour reacting to
/// a wrong fact — so the work vanishes from a root-restricted grant that could
/// see it before the reimport.
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
        work(&conn, &[en_id], &slug).expect("reads").is_some(),
        "the en root sees the work before the reimport"
    );
    assert!(
        work(&conn, &[raw_id], &slug).expect("reads").is_some(),
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
        work(&conn, &[en_id], &slug).expect("reads").is_some(),
        "the en root's grant must still see a work its own scan stamped"
    );
    assert!(
        work(&conn, &[raw_id], &slug).expect("reads").is_some(),
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
    assert!(work(&conn, &[], &slug).expect("reads").is_some());
    assert!(work(&conn, &[en_id], &slug).expect("reads").is_some());
    assert!(work(&conn, &[raw_id], &slug).expect("reads").is_some());
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
    assert!(work(&conn, &[], &slug).expect("reads").is_some());
    assert!(
        work(&conn, &[en_id], &slug).expect("reads").is_none(),
        "a restricted account cannot reach a work under no root"
    );
}

#[test]
fn search_finds_a_series_by_its_title() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let hits = search(&conn, &[], 1, "Series").expect("searches");
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
    let hits = search(&conn, &[], 1, "Hanayome").expect("searches");
    assert!(hits.series.is_empty(), "no series is named Hanayome");
    assert_eq!(hits.unfiled.len(), 1, "the work itself is the hit");
    assert_eq!(hits.unfiled[0].title, "Hanayome");
}

#[test]
fn an_empty_query_searches_for_nothing_rather_than_everything() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let hits = search(&conn, &[], 1, "   ").expect("searches");
    assert!(hits.series.is_empty() && hits.unfiled.is_empty());
}

/// The cover falls back to the lowest-volume work while `cover_work_id` is
/// NULL, which it is for every series in every library today — spec C1 §5.2.
#[test]
fn the_cover_falls_back_to_the_lowest_volume_when_none_is_set() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("reads the series");
    assert!(cover_work(&conn, &[], series_id).expect("reads").is_some());
}

/// A hand-set cover is honoured, and is the state C3 will produce.
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
        cover_work(&conn, &[], series_id)
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

    let home = home(&conn, &[], 1).expect("reads home");
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

    let home = home(&conn, &[], 1).expect("reads home");
    assert!(home.continuing.is_empty());
    assert_eq!(home.read.len(), 1);
    assert_eq!(home.added.len(), 1, "the work is still newly added");
}

/// `two_root_library`'s two roots (`en`, `raw`), but **two volumes** in each
/// — the smallest fixture where one work can hold a position in more than one
/// edition while a second work exists, so an ordering test cannot pass by
/// having only one row to sort.
fn library_with_two_works() -> (Library, crate::tests::TempDir, crate::tests::TempDir) {
    let en = crate::tests::tempdir("browse-home-en");
    write_pages(&en.path.join("Series").join("Series - v01"), 2);
    write_pages(&en.path.join("Series").join("Series - v02"), 2);
    let raw = crate::tests::tempdir("browse-home-raw");
    write_pages(&raw.path.join("Series").join("Series - v01"), 2);
    write_pages(&raw.path.join("Series").join("Series - v02"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        let en_root = scan::add_root(&conn, &en.path).expect("adds en");
        let raw_root = scan::add_root(&conn, &raw.path).expect("adds raw");
        // As in `two_root_library`: each root names its own edition key, or
        // both fall back to `raw` and derivation merges them into one.
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
    }
    (library, en, raw)
}

/// A work sorts by the newest of its positions, not by an arbitrary one.
///
/// `(SELECT updated FROM progress p WHERE p.work_id = w.id AND p.user_id = ?)`
/// is a scalar subquery over what is now a set: SQLite returns whichever row
/// it reaches first and reports no error, so the ordering was silently
/// arbitrary among a work's editions.
///
/// **The old position is put on `en` and the new one on `raw`, deliberately.**
/// `progress` has no index but its own primary key, `(user_id, work_id,
/// edition)`, so an unordered scan of it — exactly what the scalar subquery
/// runs — reaches `en` before `raw` for a fixed work: `'en' < 'raw'`
/// lexically. Putting the newer timestamp on `raw` means the broken query
/// reaches the *older* row first for `ids[0]` and reports it as that work's
/// position — the one arrangement a single-position fixture cannot produce,
/// and the reason this test needs two positions on one work to mean anything.
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

    let home = home(&conn, &[], 1).expect("reads home");
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

    let home = home(&conn, &[], 1).expect("reads home");
    assert!(
        home.continuing.iter().any(|w| w.id == ids[0]),
        "an unfinished edition keeps the work continuing"
    );
    assert!(
        !home.read.iter().any(|w| w.id == ids[0]),
        "and it must not also be sitting in Read"
    );
}

/// Two roots added the **ordinary** way — no `edition_key` on either — which
/// is the configuration `library scan --add A` / `--add B` produces.
///
/// Both fall back to `DEFAULT_EDITION_KEY`, so derivation merges their members
/// into **one** edition. `two_root_library` renames the keys to avoid exactly
/// this, which is why every test above missed the leak the whole-branch review
/// found.
fn merged_edition_library() -> (
    Library,
    i64,
    i64,
    crate::tests::TempDir,
    crate::tests::TempDir,
) {
    let a = crate::tests::tempdir("browse-merge-a");
    write_pages(&a.path.join("Series").join("Series - v01"), 2);
    let b = crate::tests::tempdir("browse-merge-b");
    write_pages(&b.path.join("Series").join("Series - v01"), 3);

    let library = Library::open_in_memory().expect("in-memory");
    let (a_id, b_id) = {
        let conn = library.conn();
        let ra = scan::add_root(&conn, &a.path).expect("adds a");
        let rb = scan::add_root(&conn, &b.path).expect("adds b");
        scan::scan_root(&conn, &ra).expect("scans a");
        scan::scan_root(&conn, &rb).expect("scans b");
        derive::derive(&conn).expect("derives");
        (ra.id, rb.id)
    };
    (library, a_id, b_id, a, b)
}

/// **The leak the whole-branch review found.** When two roots merge into one
/// edition, a reader granted only one of them saw member and page counts
/// covering both.
#[test]
fn a_merged_edition_counts_only_the_members_the_grant_allows() {
    let (library, a_id, b_id, _a, _b) = merged_edition_library();
    let conn = library.conn();

    let all = series(&conn, &[], 1, None).expect("lists");
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].editions, 1,
        "two roots with no edition_key merge into one edition — the shape \
         that made this leak reachable by default"
    );

    let slug: String = conn
        .query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reads a slug");

    let unrestricted = work(&conn, &[], &slug).expect("reads").expect("visible");
    let total: i64 = unrestricted.card.editions.iter().map(|e| e.members).sum();
    assert_eq!(total, 2, "both roots' members");

    for (root, label) in [(a_id, "a"), (b_id, "b")] {
        let seen = work(&conn, &[root], &slug)
            .expect("reads")
            .expect("visible");
        let members: i64 = seen.card.editions.iter().map(|e| e.members).sum();
        assert_eq!(
            members, 1,
            "root {label}: a granted reader must be counted only their own members"
        );
    }
}

/// The same leak at the page level: `edition_visible` is what the page routes
/// ask, and a merged edition must still be visible to each grant that holds
/// part of it — the *bytes* are guarded per edition, which is as fine-grained
/// as the schema goes.
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

    assert!(edition_visible(&conn, &[], &slug, &key).expect("reads"));
    assert!(edition_visible(&conn, &[a_id], &slug, &key).expect("reads"));
    assert!(
        !edition_visible(&conn, &[a_id], &slug, "no-such-edition").expect("reads"),
        "an edition that does not exist is not visible"
    );
    assert!(
        !edition_visible(&conn, &[a_id], "no-such-work", &key).expect("reads"),
        "a work that does not exist is not visible"
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

    assert!(edition_visible(&conn, &[en_id], &slug, "en").expect("reads"));
    assert!(
        !edition_visible(&conn, &[en_id], &slug, "raw").expect("reads"),
        "the raw edition lives under a root this grant does not hold"
    );
    assert!(edition_visible(&conn, &[raw_id], &slug, "raw").expect("reads"));
}

/// `cover_work` answers `None` for a series that does not exist rather than
/// erroring — the 500-versus-404 difference was an enumeration oracle.
#[test]
fn a_cover_for_a_series_that_does_not_exist_is_none_rather_than_an_error() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    assert_eq!(cover_work(&conn, &[], 9999).expect("does not error"), None);
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

    let target = cover_work(&conn, &[en_id], series_id)
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
    let hits = search(&conn, &[], 1, "Volume05").expect("searches");
    assert_eq!(hits.unfiled.len(), 1, "the one exact match is found");
    assert_eq!(hits.unfiled[0].title, "Volume05");
}

/// `one_series` returns the series and its works, not `None`.
///
/// Added after a mutation sweep found `one_series -> Ok(None)` surviving on
/// 2026-08-20: the route test in `taiyaku-serve` exercised it, but nothing in
/// this crate did, and the sweep runs a crate's own tests against its own
/// mutations.
#[test]
fn one_series_returns_the_series_and_its_works() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("reads the series");

    let (card, works) = one_series(&conn, &[], 1, series_id)
        .expect("reads")
        .expect("the series is visible");
    assert_eq!(card.id, series_id);
    assert_eq!(works.len(), 1, "its one volume");
    assert_eq!(works[0].editions.len(), 2, "both editions");

    assert!(
        one_series(&conn, &[], 1, 9999).expect("reads").is_none(),
        "a series that does not exist is None, and the id is what decides it"
    );
}

/// Search reads **two** indexes, and this tells them apart.
///
/// A mutation sweep found `matching_series` and `series_of_matching_works`
/// each replaceable with an empty vector while every test passed: the fixture's
/// series and its work share the word "Series", so either index alone still
/// produced the hit and each covered for the other's absence.
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

    let by_series = search(&conn, &[], 1, "Quintessential").expect("searches");
    assert_eq!(
        by_series.series.len(),
        1,
        "a series alias is found through the series index"
    );

    let by_work = search(&conn, &[], 1, "Nakano").expect("searches");
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
    let cards = series(&conn, &[], 1, None).expect("lists series");
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
    // on the same thread. Found by running this test, which hung rather than
    // failed.
    let ids = work_ids(&library);
    let conn = library.conn();
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (1, ?1, 'raw', 0, '2026-08-19T00:00:00Z')",
        [ids[0]],
    )
    .expect("a position on volume 1");

    let cards = series(&conn, &[], 1, None).expect("lists series");
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

    let cards = series(&conn, &[], 1, None).expect("lists series");
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
    let cards = series(&conn, &[], 1, None).expect("lists series");
    assert_eq!(
        cards.first().expect("one series").added,
        *ids.iter().max().expect("two works"),
    );
}

/// **The test that pins parameter order when the grant is non-empty.** Every
/// test above calls `series` with `roots: &[]`, and `root_clause` contributes
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

    let cards = series(&conn, &[en_id], 99, None).expect("lists series");
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
fn a_root_filter_keeps_only_that_roots_editions() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let all = series(&conn, &[], 1, None).expect("lists series");
    assert_eq!(all.first().expect("one series").editions, 2);

    let english = series(&conn, &[], 1, Some(en_id)).expect("lists series");
    assert_eq!(
        english.first().expect("one series").editions,
        1,
        "only the English root's edition survives"
    );
}

/// **The filter narrows the grant; it never widens it.** A reader granted the
/// English root alone, asking for the raw one, is answered nothing — not the
/// raw edition, and not an error that would confirm the root exists.
#[test]
fn a_root_filter_outside_the_grant_answers_nothing() {
    let (library, en_id, raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[en_id], 1, Some(raw_id)).expect("lists series");
    assert!(
        cards.is_empty(),
        "the grant and the filter do not intersect"
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
    let detail = work(&conn, &[], "series---v01")
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
    let detail = work(&conn, &[en_id], "series---v01")
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
    let detail = work(&conn, &[en_id, raw_id], "series---v01")
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

/// **The `aligned` counts and the alignment list are one question.** Until
/// 2026-08-21 they were two pieces of SQL and answered differently: this exact
/// fixture reported `series.aligned = 1` and `work.aligned = true` on the
/// strength of a pair whose raw side sits under a root this reader does not
/// hold, while the work page — asked directly, one line further down — listed
/// none. One screen contradicting itself, in the module whose whole reason for
/// existing is that its counts are computed after the filter.
#[test]
fn an_alignment_naming_a_hidden_edition_is_counted_by_neither_the_work_nor_the_grid() {
    let (library, en_id, _raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &[en_id], "series---v01")
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

    let cards = series(&conn, &[en_id], 1, None).expect("lists series");
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
    let detail = work(&conn, &[en_id, raw_id], "series---v01")
        .expect("reads the work")
        .expect("the work survives on both editions");
    assert!(detail.card.aligned, "both sides are held");
    assert_eq!(detail.alignments.len(), 1, "and the list says the same");

    let cards = series(&conn, &[en_id, raw_id], 1, None).expect("lists series");
    assert_eq!(cards.first().expect("one series").aligned, 1);
}

/// **An empty grant is unrestricted (§5.1), and this changed nothing for it.**
/// Every existing account holds one, so a filter that tightened the count here
/// would have taken the `⇄` mark off every library in the world.
#[test]
fn an_empty_grant_counts_the_alignment_as_it_always_did() {
    let (library, _en_id, _raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &[], "series---v01")
        .expect("reads the work")
        .expect("the work");
    assert!(detail.card.aligned, "unrestricted sees both sides");
    assert_eq!(detail.alignments.len(), 1);

    let cards = series(&conn, &[], 1, None).expect("lists series");
    assert_eq!(cards.first().expect("one series").aligned, 1);
}

/// **`?root=` narrows `aligned` too, and it was the one count it did not.**
///
/// The grant half of this filter was rewritten on 2026-08-21 so that
/// `pair_clause` is the single definition of "both editions survive"; the
/// narrowing half was not threaded through it. `only_clause` reached `series`'s
/// edition join alone, so `works`, `editions` and `unread` were computed after
/// the narrowing and the `SELECT`-list `aligned` count was computed before it.
/// This exact fixture answered `editions: 1` beside `aligned: 1` — a tile
/// claiming a pair while reporting the single edition that can hold half of
/// one. The same screen-contradicting-itself the rewrite existed to remove,
/// surviving in the half the rewrite did not reach.
///
/// **No test passed a non-`None` `only` while asserting `aligned`**, which is
/// exactly why two fixes in one commit both missed it. The `None` call here is
/// the contrast: nothing changes between the two but that argument.
#[test]
fn a_root_filter_narrows_the_aligned_count_and_not_only_the_other_three() {
    let (library, en_id, _raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let all = series(&conn, &[], 1, None).expect("lists series");
    let whole = all.first().expect("one series");
    assert_eq!(whole.editions, 2, "unnarrowed, both editions are visible");
    assert_eq!(whole.aligned, 1, "and the pair they form is counted");

    let english = series(&conn, &[], 1, Some(en_id)).expect("lists series");
    let card = english.first().expect("one series");
    assert_eq!(card.works, 1, "the work survives on its English edition");
    assert_eq!(card.editions, 1, "which is the only one under this root");
    assert_eq!(
        card.aligned, 0,
        "so the pair, whose other side is not under it, is counted by nobody"
    );
}

/// **The positive half**, without which the test above passes on a narrowing
/// clause that answers false for every alignment there is — the hole
/// `an_alignment_wholly_inside_the_grant_is_counted_by_both` fills for the
/// grant.
///
/// It is also the only test that binds a **non-empty grant and a narrowing
/// together on `series` while asserting a returned row**: ten placeholders in
/// three interleaved `(roots, roots, only)` groups and then `user_id`, which is
/// the order the module header describes. `a_root_filter_outside_the_grant_
/// answers_nothing` binds the same ten and asserts only that nothing came back.
///
/// One root holding two edition keys is what a per-path rule's
/// `set_edition_key` produces — see `derive.rs`'s `DEFAULT_EDITION_KEY`. The
/// member is written by hand for the reason `align`'s is: every fixture here is
/// scanned, and scanning two directories gives each root exactly one key.
#[test]
fn a_root_filter_keeps_a_pair_whose_two_editions_are_both_under_it() {
    let (library, en_id, raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    conn.execute(
        "INSERT INTO member (edition_id, ordinal, path, sha256, pages, root_id)
              SELECT e.id, 1, '/en/Series - v01 (raw).cbz', 'ff', 2, ?1
                FROM edition e
               WHERE e.work_id = ?2 AND e.key = 'raw'",
        rusqlite::params![en_id, ids[0]],
    )
    .expect("the raw edition also held under the English root");

    let english = series(&conn, &[en_id, raw_id], 1, Some(en_id)).expect("lists series");
    let card = english.first().expect("one series");
    assert_eq!(
        card.editions, 2,
        "both editions hold a member under the root asked for"
    );
    assert_eq!(
        card.aligned, 1,
        "so the pair they form survives the narrowing as well as the grant"
    );

    let raws = series(&conn, &[en_id, raw_id], 1, Some(raw_id)).expect("lists series");
    let card = raws.first().expect("one series");
    assert_eq!(
        card.editions, 1,
        "only the raw edition has a member under the raw root"
    );
    assert_eq!(card.aligned, 0, "and half a pair is not one");
}

/// **Asking for a root answers what is under it, and no imported work is.**
/// The grid fetches series and unfiled works together; passing `?root=` to the
/// first alone answered a root-scoped grid beside every imported work in the
/// library. No new rule: `only_clause`'s `om.root_id = ?` is false for the
/// NULL an imported member carries, exactly as `IN (…)` is.
#[test]
fn asking_for_a_root_leaves_the_unfiled_bucket_empty() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    import_a_work(&library, "Imported", "/x/taiyaku.json");

    let conn = library.conn();
    assert_eq!(
        unfiled(&conn, &[], None).expect("lists").len(),
        1,
        "the whole library still carries it"
    );
    assert!(
        unfiled(&conn, &[], Some(en_id)).expect("lists").is_empty(),
        "a work under no root is not under this one"
    );
    // A grant *and* a narrowing, which is the shape that binds every group
    // this statement has: three of roots and then `only`. A miscount here is
    // a hard refusal from `rusqlite` rather than a wrong answer.
    assert!(
        unfiled(&conn, &[en_id], Some(en_id))
            .expect("lists")
            .is_empty(),
        "and the same when the caller is restricted as well"
    );
}

#[test]
fn status_names_a_root_by_its_last_path_component() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let status = status(&conn, &[]).expect("reads status");
    assert_eq!(status.roots.len(), 1);
    assert!(
        !status.roots[0].name.contains('/'),
        "a nav item, not a filesystem path: {}",
        status.roots[0].name
    );
    assert!(status.scanned, "the fixture has been scanned");
}

#[test]
fn status_lists_only_the_roots_the_grant_names() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    assert_eq!(status(&conn, &[]).expect("reads").roots.len(), 2);
    let restricted = status(&conn, &[en_id]).expect("reads");
    assert_eq!(restricted.roots.len(), 1);
    assert_eq!(restricted.roots[0].id, en_id);
}

#[test]
fn two_roots_sharing_a_last_component_fall_back_to_their_paths() {
    let outer = crate::tests::tempdir("browse-collide");
    let library = Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        for parent in ["a", "b"] {
            let path = outer.path.join(parent).join("Manga");
            write_pages(&path.join("Series - v01"), 1);
            scan::add_root(&conn, &path).expect("adds a root");
        }
    }
    let conn = library.conn();
    let status = status(&conn, &[]).expect("reads status");
    assert_eq!(status.roots.len(), 2);
    for root in &status.roots {
        assert!(
            root.name.contains('/'),
            "a colliding name falls back to the whole path: {}",
            root.name
        );
    }
    assert!(!status.scanned, "nothing has been scanned");
}

/// **A partial collision only demotes the roots that actually collide.**
/// `named()` decides each row independently against a count computed over
/// *all* rows, so three roots where two share a last component and one does
/// not should fall back for the colliding pair while the third keeps its
/// short name. Nothing pinned that before this test: an implementation that
/// fell back for every root as soon as *any* pair collided would still pass
/// `two_roots_sharing_a_last_component_fall_back_to_their_paths` (both
/// collide there) and `status_names_a_root_by_its_last_path_component` (none
/// collide there) without ever being caught.
#[test]
fn a_partial_collision_falls_back_only_the_colliding_pair() {
    let outer = crate::tests::tempdir("browse-partial-collide");
    let library = Library::open_in_memory().expect("in-memory");
    let (colliding, unique_id) = {
        let conn = library.conn();
        let mut colliding = Vec::new();
        for parent in ["a", "b"] {
            let path = outer.path.join(parent).join("Manga");
            write_pages(&path.join("Series - v01"), 1);
            colliding.push(
                scan::add_root(&conn, &path)
                    .expect("adds a colliding root")
                    .id,
            );
        }
        let unique = outer.path.join("c").join("Unique");
        write_pages(&unique.join("Series - v01"), 1);
        let unique_id = scan::add_root(&conn, &unique)
            .expect("adds the non-colliding root")
            .id;
        (colliding, unique_id)
    };

    let conn = library.conn();
    let status = status(&conn, &[]).expect("reads status");
    assert_eq!(status.roots.len(), 3);

    for root in &status.roots {
        if colliding.contains(&root.id) {
            assert!(
                root.name.contains('/'),
                "a colliding root falls back to its whole path: {}",
                root.name
            );
        } else if root.id == unique_id {
            assert_eq!(
                root.name, "Unique",
                "the non-colliding root keeps its short name even though another pair collided"
            );
        }
    }
}

/// **`scanned` pinned by one test rather than a pair.** `status_names_a_root
/// _by_its_last_path_component` reads a fixture that has already been scanned
/// and expects `true`; `two_roots_sharing_a_last_component_fall_back_to_their
/// _paths` reads one that has not and expects `false`. Together they pin
/// `scanned`, but neither does alone, and editing one fixture's scan step
/// without noticing the other test carried half the assertion would still
/// pass. This test reads `status` on the **same** library before and after
/// the one scan that changes the answer, so it carries the whole claim by
/// itself.
///
/// Built inline, not with `two_volume_library`, because that helper scans
/// before returning — there would be no unscanned moment left to read.
#[test]
fn scanned_turns_true_only_after_a_scan_of_this_library_runs() {
    let dir = crate::tests::tempdir("browse-not-yet-scanned");
    write_pages(&dir.path.join("Series").join("Series - v01"), 1);

    let library = Library::open_in_memory().expect("in-memory");
    // A fresh block per connection: `Library::conn` locks a non-reentrant
    // `Mutex<Connection>`, and holding a guard across a call that takes its
    // own would deadlock the test rather than fail it.
    let root = {
        let conn = library.conn();
        scan::add_root(&conn, &dir.path).expect("adds the root")
    };

    {
        let conn = library.conn();
        let status = status(&conn, &[]).expect("reads status");
        assert!(
            !status.scanned,
            "the root is added but nothing has walked it yet"
        );
    }

    {
        let conn = library.conn();
        scan::scan_root(&conn, &root).expect("scans");
        derive::derive(&conn).expect("derives");
    }

    let conn = library.conn();
    let status = status(&conn, &[]).expect("reads status");
    assert!(status.scanned, "the scan wrote a dir_state row");
}

/// A two-character Japanese query is a real word, and trigram answers it with
/// silence — so `search` answers it with `LIKE`.
///
/// `花嫁` is the second half of `五等分の花嫁` and two characters long: below
/// trigram's window, above nothing. Measured 2026-08-21: `series_fts MATCH
/// '"花嫁"'` returns no rows against an index that contains that exact title.
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

    let hits = search(&conn, &[], 1, "花嫁").expect("searches");
    assert_eq!(
        hits.series.len(),
        1,
        "two characters must still find the shelf"
    );

    let long = search(&conn, &[], 1, "分の花嫁").expect("searches");
    assert_eq!(
        long.series.len(),
        1,
        "and four characters go through the index instead"
    );
}

/// `match_for` is pinned directly, not only through what `search` returns.
///
/// **I6, whole-branch review.** `LIKE '%q%'` is a superset of what trigram
/// `MATCH` answers for every fixture above it in this file — pin
/// `TRIGRAM_MIN` at `100` instead of `3` and every one of those tests still
/// passes, because the `LIKE` scan quietly answers what the index was
/// supposed to. A result-shaped assertion cannot tell the two paths apart;
/// asserting on `match_for`'s own return is a direct unit test of the
/// decision rather than of a consequence the wrong path also produces, and
/// simpler than proving the same thing through `EXPLAIN QUERY PLAN`.
#[test]
fn match_for_switches_paths_exactly_at_trigram_min() {
    // Fixed lengths, not `TRIGRAM_MIN - 1` / `TRIGRAM_MIN`: building the
    // fixture from the constant under test would make this test move with
    // any mutation to it and never fail. `TRIGRAM_MIN` is 3, measured
    // against the tokeniser above; a change to that measurement should
    // break this pin, not glide past it.
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
/// **Without `ESCAPE`, a search for `%` returns the whole library.** Measured
/// 2026-08-21: `title LIKE '%' || '%' || '%'` matched all six rows of a
/// fixture. That is the one input that turns a search box into a table dump,
/// and it is one character long, which is exactly the path this task adds.
#[test]
fn a_wildcard_typed_into_the_search_box_is_a_literal() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();

    for wildcard in ["%", "_"] {
        let hits = search(&conn, &[], 1, wildcard).expect("searches");
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
/// Measured 2026-08-21 against SQLite 3.53.4: unescaped, `%` and `_` each
/// match *every* row of a fixture, which the "expect nothing" assertion
/// above catches fine. An unescaped `\` does the opposite — it escapes the
/// query's own trailing `%` in `title LIKE '%' || ? || '%' ESCAPE '\'`, so
/// the pattern becomes "ends with a literal `%`" and matches *nothing*,
/// same as the correctly-escaped form. A fixture with no backslash in any
/// title makes the two behaviours indistinguishable by an empty-result
/// check — deleting `'\\'` from `escape_like`'s `matches!` arm left this
/// suite green. The only assertion that tells them apart is a positive
/// one: give a work a title that contains a literal backslash and expect
/// to find it.
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

    let hits = search(&conn, &[], 1, "\\").expect("searches");
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

    let hits = search(&conn, &[], 1, "花嫁").expect("searches");
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

    let hits = search(&conn, &[], 1, "花嫁").expect("searches");
    assert_eq!(
        hits.series.len(),
        1,
        "a work's own short title must still surface the series it belongs to"
    );
}

/// The grant still decides, whichever path answered.
///
/// **This is the property the whole-branch review found broken once already**
/// — `ids` used to `LIMIT` before the grant filter ran, so a search that
/// matched only invisible rows reported nothing found. A second query path is
/// a second chance to make that mistake, so it is pinned on both.
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

    let unrestricted = search(&conn, &[], 1, "花嫁").expect("searches");
    assert_eq!(unrestricted.series.len(), 1, "an empty grant sees it");

    let granted = search(&conn, &[raw_root], 1, "花嫁").expect("searches");
    assert_eq!(
        granted.series.len(),
        1,
        "a grant covering an edition of it still sees it"
    );

    let elsewhere = search(&conn, &[9999], 1, "花嫁").expect("searches");
    assert!(
        elsewhere.series.is_empty(),
        "a grant covering nothing sees nothing, however the query was answered"
    );
}

/// `?root=` deliberately does not reach search, and this says so out loud.
///
/// `search` passes `None` for `only` to both `series` and `unfiled`. That is a
/// decision rather than an omission — the sidebar's root links point at
/// `/?root=`, not at `/search` — and it is pinned here so that changing it is
/// a change to a test rather than a silent widening. See this plan's "What
/// this plan deliberately does not do", and Plan A's ledger, which carried it.
#[test]
fn search_spans_the_whole_grant_rather_than_one_root() {
    let (library, en_root, raw_root, _en, _raw) = two_root_library();
    let conn = library.conn();

    let both = search(&conn, &[en_root, raw_root], 1, "Series").expect("searches");
    assert_eq!(
        both.series.len(),
        1,
        "the series is found through the grant covering both its roots"
    );
    let card = both.series.first().expect("one series");
    assert_eq!(
        card.editions, 2,
        "and it carries both editions — search narrows to no single root"
    );
}
