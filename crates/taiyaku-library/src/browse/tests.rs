use super::*;
use crate::scan::tests::write_pages;
use crate::{Library, derive, scan};

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

#[test]
fn an_empty_grant_sees_every_root() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[]).expect("lists");
    assert_eq!(cards.len(), 1, "one series");
    assert_eq!(cards[0].editions, 2, "both editions are visible");
    assert_eq!(cards[0].works, 1, "one volume, held in two roots");
}

#[test]
fn a_grant_hides_the_editions_it_does_not_name_and_keeps_the_work() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[en_id]).expect("lists");
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
        series(&conn, &[]).expect("lists").len(),
        1,
        "an import creates no series, so the grid still holds one"
    );
    assert_eq!(
        unfiled(&conn, &[]).expect("lists").len(),
        1,
        "unrestricted sees the imported work in the unfiled bucket"
    );
    assert!(
        unfiled(&conn, &[en_id]).expect("lists").is_empty(),
        "a restricted account sees no work that sits under no root"
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
    let hits = search(&conn, &[], "Series").expect("searches");
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
    let hits = search(&conn, &[], "Hanayome").expect("searches");
    assert!(hits.series.is_empty(), "no series is named Hanayome");
    assert_eq!(hits.unfiled.len(), 1, "the work itself is the hit");
    assert_eq!(hits.unfiled[0].title, "Hanayome");
}

#[test]
fn an_empty_query_searches_for_nothing_rather_than_everything() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let hits = search(&conn, &[], "   ").expect("searches");
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

    let all = series(&conn, &[]).expect("lists");
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
    let total: i64 = unrestricted.editions.iter().map(|e| e.members).sum();
    assert_eq!(total, 2, "both roots' members");

    for (root, label) in [(a_id, "a"), (b_id, "b")] {
        let seen = work(&conn, &[root], &slug)
            .expect("reads")
            .expect("visible");
        let members: i64 = seen.editions.iter().map(|e| e.members).sum();
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
    let hits = search(&conn, &[], "Volume05").expect("searches");
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

    let (card, works) = one_series(&conn, &[], series_id)
        .expect("reads")
        .expect("the series is visible");
    assert_eq!(card.id, series_id);
    assert_eq!(works.len(), 1, "its one volume");
    assert_eq!(works[0].editions.len(), 2, "both editions");

    assert!(
        one_series(&conn, &[], 9999).expect("reads").is_none(),
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

    let by_series = search(&conn, &[], "Quintessential").expect("searches");
    assert_eq!(
        by_series.series.len(),
        1,
        "a series alias is found through the series index"
    );

    let by_work = search(&conn, &[], "Nakano").expect("searches");
    assert_eq!(
        by_work.series.len(),
        1,
        "a work title drags its series in through the work index"
    );
}
