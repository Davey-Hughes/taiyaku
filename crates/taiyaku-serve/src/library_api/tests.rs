use crate::test_support;
use taiyaku_library::account::Access;
// Shared through `test_support` rather than copied: `get_json`'s panic is
// load-bearing (see its own doc comment), and two copies would be two places to
// forget it. `get` sends no credential of its own — the fixture's router
// injects one — which is what lets a *reader* fixture answer as a reader
// through the very same helper.
use crate::test_support::{get, get_json};
use tower::ServiceExt as _;

/// The same request with no credentials at all.
///
/// **Takes a bare router deliberately.** `test_support::router_over_a_library`
/// wraps its router in `authenticating`, which injects the fixture's
/// credentials into every request — so a request sent through it is never
/// anonymous, whatever headers the caller omits. Callers must build the router
/// with `crate::build_router` directly.
async fn get_anonymous(
    router: &axum::Router,
    path: &str,
) -> axum::http::Response<axum::body::Body> {
    router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(path)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Every browse route refuses an anonymous request, and refuses it **without a
/// challenge header**.
///
/// The workspace already has `every_route_refuses_an_anonymous_request`, which
/// walks both route lists and derives its own count — this is about the
/// *shape* of the refusal, which spec A2 §4 fixes: no `WWW-Authenticate`,
/// because that header makes the browser raise its own credential dialog
/// instead of the reader's login screen.
#[tokio::test]
async fn the_browse_routes_refuse_anonymously_without_a_challenge_header() {
    let (_authenticated, state, _fixture) = test_support::router_over_a_library().await;
    // The guard as it really is, with nothing injecting credentials.
    let router = crate::build_router(state);
    for path in [
        "/api/library/home",
        "/api/library/series",
        "/api/library/series/1",
        "/api/library/series/1/cover",
        "/api/library/works/w",
        "/api/library/search?q=x",
        "/api/library/status",
    ] {
        let response = get_anonymous(&router, path).await;
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNAUTHORIZED,
            "{path} answered anonymously"
        );
        assert!(
            !response
                .headers()
                .contains_key(axum::http::header::WWW_AUTHENTICATE),
            "{path} challenged the browser"
        );
    }
}

/// The fixture library holds one **imported** work, so it exercises spec C1
/// §5.4's unfiled bucket rather than the series grid.
#[tokio::test]
async fn the_grid_carries_an_imported_work_in_the_unfiled_bucket() {
    let (router, _state, _fixture) = test_support::router_over_a_library().await;
    let body = get_json(&router, "/api/library/series").await;

    assert_eq!(
        body["series"].as_array().expect("series").len(),
        0,
        "an import creates no series"
    );
    let unfiled = body["unfiled"].as_array().expect("unfiled");
    assert_eq!(unfiled.len(), 1, "the imported work is unfiled");
    assert_eq!(unfiled[0]["title"], "W");
    assert_eq!(
        unfiled[0]["editions"].as_array().expect("editions").len(),
        1
    );
}

#[tokio::test]
async fn one_work_is_served_by_slug_and_a_missing_one_is_a_404() {
    let (router, state, _fixture) = test_support::router_over_a_library().await;
    let slug = state.works.snapshot()[0].id.clone();
    {
        // O73: `EditionCard::missing` has to reach the client or
        // nothing can render it. `browse::missing_by_edition` counts a member
        // missing once it has a root but no matching `scan_entry` row — the
        // state `scan::forget_unseen` leaves behind — so giving the fixture's
        // one member a root and no `scan_entry` at all reproduces that
        // without running a second scan just to remove one row.
        let conn = state.library.conn();
        conn.execute("INSERT INTO root (id, path) VALUES (1, '/library/w')", [])
            .expect("a root for the fixture's one member");
        conn.execute("UPDATE member SET root_id = 1", [])
            .expect("files the member under it");
    }

    let body = get_json(&router, &format!("/api/library/works/{slug}")).await;
    assert_eq!(body["slug"], slug);
    assert_eq!(body["title"], "W");
    assert_eq!(
        body["editions"][0]["missing"], 1,
        "the count `browse` computes must reach the client — nothing else \
         forwards it"
    );

    let missing = get(&router, "/api/library/works/no-such-work").await;
    assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_home_lists_are_present_and_empty_for_a_reader_who_has_read_nothing() {
    let (router, _state, _fixture) = test_support::router_over_a_library().await;
    let body = get_json(&router, "/api/library/home").await;

    assert_eq!(body["continuing"].as_array().expect("continuing").len(), 0);
    assert_eq!(body["read"].as_array().expect("read").len(), 0);
    assert_eq!(
        body["added"].as_array().expect("added").len(),
        1,
        "the one work is newly added"
    );
}

/// Spec C1 §8: the empty state names what was searched, so a reader can tell
/// "no matches" from "the box didn't hold what I thought it did".
#[tokio::test]
async fn a_search_echoes_the_query_back_so_an_empty_result_can_name_it() {
    let (router, _state, _fixture) = test_support::router_over_a_library().await;

    let hit = get_json(&router, "/api/library/search?q=W").await;
    assert_eq!(hit["query"], "W");
    assert_eq!(
        hit["unfiled"].as_array().expect("unfiled").len(),
        1,
        "the imported work is found by its own title"
    );

    // Not a tokeniser miss: three characters is `TRIGRAM_MIN` exactly, so
    // this goes through the trigram index, which segments CJK fine since
    // migration 8. It misses because this fixture holds no Japanese title —
    // what this half pins is that a non-ASCII `query` round-trips through the
    // URL and back out into the echoed empty state, not any search failure.
    let miss = get_json(&router, "/api/library/search?q=%E4%BA%94%E7%AD%89%E5%88%86").await;
    assert_eq!(miss["query"], "五等分");
    assert_eq!(miss["series"].as_array().expect("series").len(), 0);
    assert_eq!(miss["unfiled"].as_array().expect("unfiled").len(), 0);
}

/// A missing `?q=` is an empty search, not a 400 — the search screen loads
/// before anybody has typed.
#[tokio::test]
async fn a_search_with_no_query_is_empty_rather_than_a_refusal() {
    let (router, _state, _fixture) = test_support::router_over_a_library().await;
    let body = get_json(&router, "/api/library/search").await;
    assert_eq!(body["query"], "");
    assert_eq!(body["series"].as_array().expect("series").len(), 0);
}

/// The cover redirects to the thumbnail route rather than growing a second
/// cover pipeline beside `cover.rs`.
#[tokio::test]
async fn a_series_cover_redirects_to_the_existing_thumbnail_route() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let series_id: i64 = {
        let conn = state.library.conn();
        conn.query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
            .expect("reads the series")
    };

    let response = get(&router, &format!("/api/library/series/{series_id}/cover")).await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::TEMPORARY_REDIRECT
    );
    let location = response
        .headers()
        .get(axum::http::header::LOCATION)
        .expect("a Location header")
        .to_str()
        .expect("ascii");
    assert!(
        location.starts_with("/api/works/") && location.ends_with("/pages/0/thumb"),
        "points at the thumb route: {location}"
    );
}

/// O133: a caller that says how large it will draw the cover is sent to the
/// tile route at that size, which serves page 0 there — not to the page route,
/// which refuses an account without `can_stream`. A `w` that is not one of the
/// cover sizes, or not a number, keeps the tile, and is not refused.
#[tokio::test]
async fn a_series_cover_asked_for_a_cover_size_redirects_to_the_tile_at_that_size() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let series_id: i64 = {
        let conn = state.library.conn();
        conn.query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
            .expect("reads the series")
    };

    for (w, ends) in [
        ("512", "/pages/0/thumb?w=512"),
        ("1", "/pages/0/thumb"),
        ("1000", "/pages/0/thumb"),
        ("abc", "/pages/0/thumb"),
        ("768&w=256", "/pages/0/thumb?w=768"),
    ] {
        let response = get(
            &router,
            &format!("/api/library/series/{series_id}/cover?w={w}"),
        )
        .await;
        assert_eq!(
            response.status(),
            axum::http::StatusCode::TEMPORARY_REDIRECT,
            "w={w}"
        );
        let location = response
            .headers()
            .get(axum::http::header::LOCATION)
            .expect("a Location header")
            .to_str()
            .expect("ascii");
        assert!(
            location.starts_with("/api/works/") && location.ends_with(ends),
            "w={w} points at {ends}: {location}"
        );
    }
}

/// A series this caller cannot see is a 404, the same answer as one that does
/// not exist — spec C1 §5.1, and the enumeration refusal this project applies
/// elsewhere.
#[tokio::test]
async fn a_series_that_does_not_exist_is_a_404() {
    let (router, _state, _fixture) = test_support::router_over_a_library().await;
    let response = get(&router, "/api/library/series/9999").await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

/// `/api/library/series/{id}`.
#[tokio::test]
async fn one_series_carries_the_series_and_its_works() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let (series_id, slug, key, sha256): (i64, String, String, String) = {
        let conn = state.library.conn();
        conn.query_row(
            "SELECT w.series_id, w.slug, e.key, m.sha256
               FROM work w JOIN edition e ON e.work_id = w.id
               JOIN member m ON m.edition_id = e.id
              WHERE m.pages > 0
              ORDER BY w.volume, w.id, e.key, m.ordinal LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("reads the series and its first page's member")
    };

    let body = get_json(&router, &format!("/api/library/series/{series_id}")).await;
    assert_eq!(body["series"]["id"], series_id);
    assert_eq!(body["series"]["title"], "Scanned");
    assert_eq!(
        body["series"]["cover"],
        format!(
            "/api/works/{slug}/{key}/pages/0/thumb?v={}",
            crate::cover::cover_version(&sha256, 0)
        )
    );

    let works = body["works"].as_array().expect("works");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0]["title"], "Scanned");
    assert_eq!(works[0]["volume"], 1.0);
    assert_eq!(
        works[0]["editions"].as_array().expect("editions").len(),
        2,
        "both editions are visible to an unrestricted reader"
    );
}

/// The grid lists a real series, not only the unfiled bucket.
#[tokio::test]
async fn the_grid_lists_a_series_with_its_counts() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let body = get_json(&router, "/api/library/series").await;

    let series = body["series"].as_array().expect("series");
    assert_eq!(series.len(), 1);
    assert_eq!(series[0]["works"], 1);
    assert_eq!(series[0]["editions"], 2);
    assert_eq!(
        series[0]["aligned"], 1,
        "the fixture's work carries a solved edge"
    );
    assert_eq!(body["unfiled"].as_array().expect("unfiled").len(), 0);
}

/// Search reaches a scanned series through `series_fts`.
#[tokio::test]
async fn search_finds_a_scanned_series_through_the_api() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let body = get_json(&router, "/api/library/search?q=Scanned").await;
    assert_eq!(body["query"], "Scanned");
    assert_eq!(body["series"].as_array().expect("series").len(), 1);
}

/// A cover for a series that does not exist is a **404, not a 500** — a
/// difference between the two would be an enumeration oracle, since
/// `/series/{id}` already answers 404.
#[tokio::test]
async fn a_cover_for_a_series_that_does_not_exist_is_a_404_like_the_series_itself() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();

    let cover = get(&router, "/api/library/series/9999/cover").await;
    let series = get(&router, "/api/library/series/9999").await;
    assert_eq!(
        cover.status(),
        series.status(),
        "a missing series and its cover must be indistinguishable"
    );
    assert_eq!(cover.status(), axum::http::StatusCode::NOT_FOUND);
}

/// The home lists carry a work once a reader has a position on it.
#[tokio::test]
async fn home_carries_a_work_the_reader_has_started() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot, updated)
             SELECT 1, id, 'raw', 3, '2026-08-19T00:00:00Z' FROM work LIMIT 1",
            [],
        )
        .expect("records a position");
    }

    let body = get_json(&router, "/api/library/home").await;
    let continuing = body["continuing"].as_array().expect("continuing");
    assert_eq!(continuing.len(), 1, "the started work is being continued");
    assert_eq!(continuing[0]["title"], "Scanned");
    assert_eq!(body["read"].as_array().expect("read").len(), 0);
}

/// A store error becomes a **500, not a panic**, on every browse route.
///
/// Every handler funnels its failures through one `failed` helper. Driven by
/// dropping the tables the queries read, which is the bluntest way to make a
/// real `rusqlite` error arrive at the handler rather than a mocked one.
#[tokio::test]
async fn a_store_error_is_a_500_rather_than_a_panic() {
    let (router, state, _fixture) = test_support::router_over_a_library().await;
    {
        let conn = state.library.conn();
        // `series_fts` and the triggers go with the tables they index.
        conn.execute_batch(
            "PRAGMA writable_schema = ON;
             DROP TABLE IF EXISTS series_fts;
             DROP TRIGGER IF EXISTS series_title_ai;
             DROP TRIGGER IF EXISTS series_title_ad;
             DROP TRIGGER IF EXISTS series_title_au;
             DROP TABLE IF EXISTS series_title;
             DROP TABLE IF EXISTS series;",
        )
        .expect("drops the tables the browse queries read");
    }

    // `/api/library/home` is deliberately absent: its queries reach `work`,
    // `edition`, `member` and `progress` and never join `series`, so it still
    // answers 200 here. That independence is the same one spec C1 §5.4 is
    // about — an imported work has no series and the home lists show it
    // anyway — and asserting a 500 from it would have been asserting a
    // coincidence.
    for path in [
        "/api/library/series",
        "/api/library/search?q=x",
        "/api/library/series/1",
        "/api/library/series/1/cover",
    ] {
        let response = get(&router, path).await;
        assert_eq!(
            response.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "{path} should answer 500 once its tables are gone"
        );
        let body = test_support::body_string(response).await;
        assert!(
            body.contains("reading"),
            "{path}: the message says what it was reading: {body}"
        );
    }
}

/// **A restricted reader cannot fetch the bytes**, not merely fail to see the
/// work listed.
///
/// This is spec C1 §5.1's boundary at the route rather than in the query.
#[tokio::test]
async fn a_restricted_reader_is_refused_the_pages_of_a_root_they_do_not_hold() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let slug = state.works.snapshot()[0].id.clone();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld')",
            [],
        )
        .expect("two roots");
        // Every member of the fixture's work lives under the root the reader
        // is NOT granted.
        conn.execute("UPDATE member SET root_id = 2", [])
            .expect("files the material under the withheld root");
        crate::test_support::grant_root(&conn, 1);
    }

    for path in [
        format!("/api/works/{slug}/raw/pages/0"),
        format!("/api/works/{slug}/raw/pages/0/thumb"),
        format!("/api/works/{slug}"),
    ] {
        let response = get(&router, &path).await;
        let status = response.status();
        let body = test_support::body_string(response).await;
        assert_eq!(
            status,
            axum::http::StatusCode::NOT_FOUND,
            "{path} served material from a root the reader was not granted"
        );
        // **The body, not only the status, and that is the whole point.** The
        // fixture's member files do not exist, so `/pages` answers 404 either
        // way. Without the grant check, these routes answer the *filesystem*
        // instead, naming an absolute path; the grant check's own refusal names
        // the work and nothing else.
        assert_eq!(
            body,
            format!("no work `{slug}`"),
            "{path} refused for the wrong reason — this must be the grant, \
             not a missing file"
        );
    }
}

/// The same reader, granted the root the material actually sits under, is
/// served — so the refusal above is the grant working rather than the fixture
/// being broken in some other way.
#[tokio::test]
async fn the_same_reader_granted_the_right_root_is_not_refused() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let slug = state.works.snapshot()[0].id.clone();
    {
        let conn = state.library.conn();
        conn.execute("INSERT INTO root (id, path) VALUES (1, '/held')", [])
            .expect("one root");
        conn.execute("UPDATE member SET root_id = 1", [])
            .expect("files the material under it");
        crate::test_support::grant_root(&conn, 1);
    }

    let response = get(&router, &format!("/api/works/{slug}")).await;
    assert_ne!(
        response.status(),
        axum::http::StatusCode::NOT_FOUND,
        "a granted reader must reach their own material"
    );
}

/// The titles in `listed`, sorted.
fn titles_in(listed: &serde_json::Value) -> Vec<String> {
    let mut titles: Vec<String> = listed
        .as_array()
        .expect("a list of works")
        .iter()
        .map(|work| work["title"].as_str().expect("a title").to_string())
        .collect();
    titles.sort();
    titles
}

/// A reader granted the library `Shelf` browses the work under each of its
/// two directories, and not the work under `Other`: in the grid, in a search, on
/// the home lists and at each work's own address.
#[tokio::test]
async fn the_grid_shows_every_dir_of_a_granted_library() {
    let tmp = test_support::Fixture::new("library-api-granted-library");
    let (router, state) = test_support::router_over_a_granted_two_dir_library(&tmp);
    let shelf = vec![
        "First Directory Work".to_string(),
        "Second Directory Work".to_string(),
    ];

    let grid = get_json(&router, "/api/library/series").await;
    assert_eq!(titles_in(&grid["unfiled"]), shelf, "the grid: {grid}");

    let found = get_json(&router, "/api/library/search?q=Work").await;
    assert_eq!(titles_in(&found["unfiled"]), shelf, "a search: {found}");

    let home = get_json(&router, "/api/library/home").await;
    assert_eq!(titles_in(&home["added"]), shelf, "recently added: {home}");

    for (title, expected) in [
        ("First Directory Work", axum::http::StatusCode::OK),
        ("Second Directory Work", axum::http::StatusCode::OK),
        ("Other Library Work", axum::http::StatusCode::NOT_FOUND),
    ] {
        let slug: String = state
            .library
            .conn()
            .query_row("SELECT slug FROM work WHERE title = ?1", [title], |r| {
                r.get(0)
            })
            .expect("the work's slug");
        let response = get(&router, &format!("/api/library/works/{slug}")).await;
        assert_eq!(response.status(), expected, "{title}");
    }
}

#[tokio::test]
async fn the_grid_carries_the_two_fields_the_chips_sort_and_filter_on() {
    // **Not `router_over_a_library`.** That fixture only imports a work via
    // `import::sidecar`, which per `browse.rs`'s own module docs never writes
    // `series_id` — so the grid it produces has an `unfiled` work and no
    // `series` at all. This fixture inserts a `series` row and files the work
    // under it, the same way `the_grid_lists_a_series_with_its_counts`
    // above does.
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let body = get_json(&router, "/api/library/series").await;
    let first = body["series"]
        .as_array()
        .and_then(|s| s.first())
        .expect("at least one series");
    assert!(first["added"].is_i64(), "added is a number: {first}");
    assert!(first["unread"].is_i64(), "unread is a number: {first}");
}

/// The library a directory is in, read from the store rather than assumed.
fn library_of(conn: &rusqlite::Connection, root: i64) -> i64 {
    conn.query_row("SELECT library_id FROM root WHERE id = ?1", [root], |r| {
        r.get(0)
    })
    .unwrap_or_else(|e| panic!("directory {root}'s library: {e}"))
}

/// Two directories, each a library of its own (`root_gets_a_library` makes one
/// per directory inserted without one), with every member of the fixture's one
/// work under the first. Answers the two libraries' ids, the material's first.
///
/// **Directory ids 5 and 6**, so neither library's id is a directory's: a narrowing
/// that read a library id as a directory id would find nothing under either.
fn two_libraries_one_holding_the_work(state: &crate::ServeState) -> (i64, i64) {
    let conn = state.library.conn();
    conn.execute(
        "INSERT INTO root (id, path) VALUES (5, '/held'), (6, '/elsewhere')",
        [],
    )
    .expect("two directories");
    conn.execute("UPDATE member SET root_id = 5", [])
        .expect("files every member under the first");
    let (held, elsewhere) = (library_of(&conn, 5), library_of(&conn, 6));
    assert!(
        ![5, 6].contains(&held) && ![5, 6].contains(&elsewhere),
        "the libraries {held} and {elsewhere} share a directory's id"
    );
    (held, elsewhere)
}

/// **Over a fixture that actually has a series**: a grid with no series in it
/// answers no series whatever the narrowing, so this one could not tell the
/// narrowing from a handler that dropped the parameter on the floor.
#[tokio::test]
async fn a_library_that_does_not_exist_answers_an_empty_grid_rather_than_an_error() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let body = get_json(&router, "/api/library/series?library=999999").await;
    assert_eq!(
        body["series"].as_array().map(Vec::len),
        Some(0),
        "no series, and a 200: {body}"
    );
}

/// The other half: a library that *does* exist answers the material in it,
/// and a second library answers none of it.
///
/// A library that does not exist and a library holding nothing are
/// deliberately the same answer (`browse::library_scope`), so a test that only
/// ever asked for a missing one could not tell the narrowing from a handler
/// that dropped the parameter on the floor.
#[tokio::test]
async fn the_grid_narrows_to_the_library_the_query_names() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let (held, elsewhere) = two_libraries_one_holding_the_work(&state);

    let all = get_json(&router, "/api/library/series").await;
    assert_eq!(
        all["series"].as_array().map(Vec::len),
        Some(1),
        "unnarrowed, the series is listed: {all}"
    );

    let narrowed = get_json(&router, &format!("/api/library/series?library={held}")).await;
    assert_eq!(
        narrowed["series"].as_array().map(Vec::len),
        Some(1),
        "the library the material is in: {narrowed}"
    );

    let other = get_json(&router, &format!("/api/library/series?library={elsewhere}")).await;
    assert_eq!(
        other["series"].as_array().map(Vec::len),
        Some(0),
        "a library holding none of it: {other}"
    );
}

/// **`?library=` leaves imported works out of the unfiled bucket.** `series` and `unfiled` are
/// fetched together, and an imported member sits under no directory, so asking
/// for a library asks for none of them — spec C1 §5.1's NULL-excludes-itself
/// rule, applied in SQL rather than special-cased here. The library asked for
/// exists, so the empty bucket is that rule and not an unknown id.
#[tokio::test]
async fn a_library_scoped_grid_carries_no_unfiled_work() {
    let (router, state, _fixture) = test_support::router_over_a_library().await;
    let shelf = {
        let conn = state.library.conn();
        conn.execute("INSERT INTO root (id, path) VALUES (5, '/shelf')", [])
            .expect("a directory, and so a library");
        library_of(&conn, 5)
    };

    let all = get_json(&router, "/api/library/series").await;
    assert_eq!(
        all["unfiled"].as_array().map(Vec::len),
        Some(1),
        "the imported work is in the whole-store grid: {all}"
    );

    let scoped = get_json(&router, &format!("/api/library/series?library={shelf}")).await;
    assert_eq!(
        scoped["unfiled"].as_array().map(Vec::len),
        Some(0),
        "and in no library: {scoped}"
    );
}

/// `?library=` reaches the home lists: a position on the fixture's work is
/// continued in the library the work is in, and not in the other one.
#[tokio::test]
async fn the_home_lists_narrow_to_the_library_the_query_names() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let (held, elsewhere) = two_libraries_one_holding_the_work(&state);
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot, updated)
             SELECT 1, id, 'raw', 3, '2026-10-17T00:00:00Z' FROM work",
            [],
        )
        .expect("records a position");
    }

    let narrowed = get_json(&router, &format!("/api/library/home?library={held}")).await;
    assert_eq!(
        titles_in(&narrowed["continuing"]),
        ["Scanned"],
        "continued in its own library: {narrowed}"
    );

    let other = get_json(&router, &format!("/api/library/home?library={elsewhere}")).await;
    assert_eq!(
        other["continuing"].as_array().map(Vec::len),
        Some(0),
        "and not in another: {other}"
    );
}

/// `?library=` reaches search: the fixture's series is found in the library
/// it is in, and not in the other one.
#[tokio::test]
async fn search_narrows_to_the_library_the_query_names() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let (held, elsewhere) = two_libraries_one_holding_the_work(&state);

    let narrowed = get_json(
        &router,
        &format!("/api/library/search?q=Scanned&library={held}"),
    )
    .await;
    assert_eq!(
        narrowed["series"].as_array().map(Vec::len),
        Some(1),
        "found in its own library: {narrowed}"
    );

    let other = get_json(
        &router,
        &format!("/api/library/search?q=Scanned&library={elsewhere}"),
    )
    .await;
    assert_eq!(
        other["series"].as_array().map(Vec::len),
        Some(0),
        "and not in another: {other}"
    );
}

/// The volumes a response's `works` lists, in its order.
fn volumes_in(works: &serde_json::Value) -> Vec<f64> {
    works
        .as_array()
        .unwrap_or_else(|| panic!("an array of works: {works}"))
        .iter()
        .map(|w| {
            w["volume"]
                .as_f64()
                .unwrap_or_else(|| panic!("a volume: {w}"))
        })
        .collect()
}

/// Spec §3.6: `?library=` reaches the series page. Opened from `Kids`' grid,
/// whose tile counts one volume, the page counts and lists that one; with no
/// library it lists all five, and a library that does not exist is a 404.
#[tokio::test]
async fn one_series_narrows_to_the_library_the_query_names() {
    let (router, _state, _fixture, ids) = test_support::router_over_a_series_in_two_libraries();
    let series = ids.series;

    let grid = get_json(
        &router,
        &format!("/api/library/series?library={}", ids.kids),
    )
    .await;
    assert_eq!(grid["series"][0]["id"], series, "Kids' grid: {grid}");
    assert_eq!(grid["series"][0]["works"], 1, "Kids' tile: {grid}");

    let kids = get_json(
        &router,
        &format!("/api/library/series/{series}?library={}", ids.kids),
    )
    .await;
    assert_eq!(kids["series"]["works"], 1, "opened from Kids: {kids}");
    assert_eq!(volumes_in(&kids["works"]), [5.0], "opened from Kids");

    let manga = get_json(
        &router,
        &format!("/api/library/series/{series}?library={}", ids.manga),
    )
    .await;
    assert_eq!(
        volumes_in(&manga["works"]),
        [1.0, 2.0, 3.0, 4.0],
        "opened from Manga"
    );

    let all = get_json(&router, &format!("/api/library/series/{series}")).await;
    assert_eq!(all["series"]["works"], 5, "every library: {all}");
    assert_eq!(
        volumes_in(&all["works"]),
        [1.0, 2.0, 3.0, 4.0, 5.0],
        "every library"
    );

    let response = get(
        &router,
        &format!("/api/library/series/{series}?library=999999"),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

/// **`?root=` is refused, naming `library`.**
#[tokio::test]
async fn a_grid_asked_for_a_root_is_refused_naming_library() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let response = get(&router, "/api/library/series?root=1").await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.contains("`library`") && body.contains("`root`"),
        "the refusal names the parameter to send and the one it refused: {body}"
    );
}

#[tokio::test]
async fn the_work_route_carries_an_alignments_array_and_the_list_routes_do_not() {
    // Not `router_over_a_library`: that fixture's work is imported via
    // `import::sidecar`, which per `browse.rs`'s own module docs never writes
    // `series_id` — so `/api/library/series` answers no series at all and
    // `first()` is `None`. This fixture inserts a `series` row and files the
    // work under it, so `listed` genuinely binds.
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let grid = get_json(&router, "/api/library/series").await;
    let listed = grid["series"].as_array().and_then(|s| s.first());
    let listed = listed.expect("the fixture's series is listed");
    assert!(
        listed.get("alignments").is_none(),
        "a list route must not carry alignment detail: {listed}"
    );
    let home = get_json(&router, "/api/library/home").await;
    let slug = home["added"][0]["slug"]
        .as_str()
        .expect("a work")
        .to_string();
    let detail = get_json(&router, &format!("/api/library/works/{slug}")).await;
    assert!(
        detail["alignments"].is_array(),
        "the single-work route carries them: {detail}"
    );
}

/// The keys of a JSON object, sorted.
fn keys_of(value: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .unwrap_or_else(|| panic!("an object: {value}"))
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// `/api/library/status` lists libraries, each `{id, name, scanned}`, beside
/// `restricted`, and carries nothing else at either level: the key lists are
/// compared whole, because a substring check cannot see an added field.
///
/// **Two libraries, one walked and one not**, so `scanned` is read from each
/// library's own directories: a constant either way fails one of them. Directory ids 5
/// and 6, so neither library's id is a directory's, and a walk looked up by the
/// library's id in place of its directories' would find nothing.
#[tokio::test]
async fn the_status_lists_libraries_and_nothing_else() {
    let (router, state, _fixture) = test_support::router_over_a_library().await;
    let (shelf, walked) = {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO root (id, path) VALUES (5, '/shelf'), (6, '/walked')",
            [],
        )
        .expect("two directories, and so two libraries");
        conn.execute(
            "INSERT INTO dir_state (root_id, rel_path, mtime, entry_count) \
             VALUES (6, '', 0, 0)",
            [],
        )
        .expect("the second directory walked");
        (library_of(&conn, 5), library_of(&conn, 6))
    };
    assert!(
        ![5, 6].contains(&shelf) && ![5, 6].contains(&walked),
        "the libraries {shelf} and {walked} share a directory's id"
    );

    let body = get_json(&router, "/api/library/status").await;
    assert_eq!(keys_of(&body), ["libraries", "restricted"], "{body}");
    let libraries = body["libraries"].as_array().expect("a list of libraries");
    assert_eq!(libraries.len(), 2, "the two libraries: {body}");
    for entry in libraries {
        assert_eq!(keys_of(entry), ["id", "name", "scanned"], "{entry}");
    }
    assert_eq!(libraries[0]["id"], shelf, "{body}");
    assert_eq!(libraries[0]["name"], "shelf", "{body}");
    assert_eq!(
        libraries[0]["scanned"], false,
        "nothing has walked it: {body}"
    );
    assert_eq!(libraries[1]["id"], walked, "{body}");
    assert_eq!(libraries[1]["name"], "walked", "{body}");
    assert_eq!(
        libraries[1]["scanned"], true,
        "its directory has been walked: {body}"
    );
}

/// Spec §4's N through the routes: the account granted Manga is listed Manga
/// alone, though it reaches Kids' directory inside Manga's and so sees Kids'
/// book on Manga's page; the account granted Kids is listed Kids alone.
#[tokio::test]
async fn the_sidebar_lists_the_libraries_granted_and_not_one_reached() {
    let n = test_support::router_over_a_library_nested_in_another();
    let listed = |body: &serde_json::Value| -> Vec<i64> {
        body["libraries"]
            .as_array()
            .expect("a list of libraries")
            .iter()
            .map(|entry| entry["id"].as_i64().expect("an id"))
            .collect()
    };
    let titles = |body: &serde_json::Value| -> Vec<String> {
        body["series"]
            .as_array()
            .expect("a list of series")
            .iter()
            .map(|card| card["title"].as_str().expect("a title").to_owned())
            .collect()
    };

    let manga = get_json(&n.manga_reader, "/api/library/status").await;
    assert_eq!(listed(&manga), vec![n.manga], "{manga}");
    let kids = get_json(&n.kids_reader, "/api/library/status").await;
    assert_eq!(listed(&kids), vec![n.kids], "{kids}");

    let page = get_json(
        &n.manga_reader,
        &format!("/api/library/series?library={}", n.manga),
    )
    .await;
    assert_eq!(titles(&page), ["Kids Book", "Manga Book"], "{page}");
}

/// `restricted` says whether the caller sees only the libraries granted to
/// it, read through the guard as each request rebuilds the principal. False
/// for an administrator, even one holding a grant (O86), and for a reader
/// seeing every library; true for a reader granted a library, and for one
/// granted none, who is listed no library.
#[tokio::test]
async fn the_status_says_whether_the_caller_sees_only_granted_libraries() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let library: i64 = {
        let conn = state.library.conn();
        conn.query_row("SELECT library_id FROM root", [], |r| r.get(0))
            .expect("the fixture's one library")
    };
    let status = || async { get_json(&router, "/api/library/status").await };
    let write = |sql: &str| {
        let conn = state.library.conn();
        conn.execute_batch(sql).expect(sql);
    };

    let body = status().await;
    assert_eq!(body["restricted"], false, "an administrator: {body}");

    {
        let conn = state.library.conn();
        taiyaku_library::account::grant_library(&conn, 1, library).expect("grants");
    }
    let body = status().await;
    assert_eq!(
        body["restricted"], false,
        "an administrator holding a grant: {body}"
    );

    write("UPDATE user SET role = 'user' WHERE id = 1");
    let body = status().await;
    assert_eq!(
        body["restricted"], true,
        "a reader granted a library: {body}"
    );
    assert_eq!(
        body["libraries"].as_array().map(Vec::len),
        Some(1),
        "{body}"
    );

    write("DELETE FROM user_library WHERE user_id = 1");
    let body = status().await;
    assert_eq!(body["restricted"], true, "a reader granted none: {body}");
    assert_eq!(body["libraries"], serde_json::json!([]), "{body}");

    write("UPDATE user SET every_library = 1 WHERE id = 1");
    let body = status().await;
    assert_eq!(
        body["restricted"], false,
        "a reader seeing every library: {body}"
    );
    assert_eq!(
        body["libraries"].as_array().map(Vec::len),
        Some(1),
        "{body}"
    );
}

/// The three series [`test_support::router_over_three_unrelated_series`]
/// scans, by id, once [`plant_titles`] has given each more than one title.
struct Titled {
    gotoubun: i64,
    kaguya: i64,
    oshinoko: i64,
}

/// Each series' titles as `/api/library/series/{id}` must carry them, in the
/// order they were written: `(title, kind, primary)`. The first entry of each
/// is the row derivation wrote.
const GOTOUBUN: &[(&str, &str, bool)] = &[
    ("Go-Toubun no Hanayome", "romaji", false),
    ("五等分の花嫁", "native", true),
    ("The Quintessential Quintuplets", "english", false),
];
const KAGUYA: &[(&str, &str, bool)] = &[
    ("Kaguya-sama wa Kokurasetai", "romaji", true),
    ("かぐや様は告らせたい", "native", false),
    ("Kaguya-sama: Love Is War", "alt", false),
];
const OSHINOKO: &[(&str, &str, bool)] = &[
    ("Oshi no Ko", "alt", true),
    ("【推しの子】", "native", false),
];

/// Titles planted beside the one derivation gave each series.
///
/// **Derivation's own row stays in, on every series.** It is `hand = 0` with
/// `kind` at its `alt` default; Go-Toubun's and Kaguya's are re-kinded, and
/// Oshi no Ko's is left exactly as `derive` wrote it. `librarydoc::of`
/// projects only `hand = 1` titles (O80), while `set_series_titles` deletes
/// every row, derivation's included — so a read path answering the
/// document's list would drop the one title nobody typed, and the round trip
/// below would delete it.
///
/// **The ids are checked against the grid, not assumed.** A native primary
/// title sorts after every romaji one, so Go-Toubun, derived first with the
/// lowest id, lists last. The assertion at the end is what says no series' id
/// equals its position in the grid, counted from nought or from one, so a read
/// that found a series by its place in a list could not pass by coincidence.
///
/// **Nor does a work's id equal its series' id**, for Kaguya and Oshi no Ko:
/// the fixture's second Go-Toubun volume moves them apart, and the check below
/// says so. Go-Toubun's first work keeps id 1 beside series 1, which is why the
/// check names the other two and the shape test reads all three.
async fn plant_titles(router: &axum::Router, state: &crate::ServeState) -> Titled {
    let titled = {
        let conn = state.library.conn();
        let id = |sort_title: &str| -> i64 {
            conn.query_row(
                "SELECT id FROM series WHERE sort_title = ?1",
                [sort_title],
                |row| row.get(0),
            )
            .unwrap_or_else(|e| panic!("the fixture's `{sort_title}` series: {e}"))
        };
        let titled = Titled {
            gotoubun: id("Go-Toubun no Hanayome"),
            kaguya: id("Kaguya-sama wa Kokurasetai"),
            oshinoko: id("Oshi no Ko"),
        };
        for series_id in [titled.kaguya, titled.oshinoko] {
            let sharing: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM work WHERE series_id = ?1 AND id = ?1",
                    [series_id],
                    |row| row.get(0),
                )
                .expect("counts the series' works that carry its id");
            assert_eq!(
                sharing, 0,
                "series {series_id} holds a work with its own id, so a read that \
                 took a work's id for the series' would find the right titles by \
                 coincidence"
            );
        }
        for (series_id, primary) in [(titled.gotoubun, false), (titled.kaguya, true)] {
            let rekinded = conn
                .execute(
                    "UPDATE series_title SET kind = 'romaji', is_primary = ?1
                      WHERE series_id = ?2 AND hand = 0",
                    rusqlite::params![primary, series_id],
                )
                .expect("re-kinds derivation's title");
            assert_eq!(rekinded, 1, "series {series_id} holds one derived title");
        }
        for (series_id, want) in [
            (titled.gotoubun, GOTOUBUN),
            (titled.kaguya, KAGUYA),
            (titled.oshinoko, OSHINOKO),
        ] {
            for (title, kind, primary) in &want[1..] {
                conn.execute(
                    "INSERT INTO series_title (series_id, title, kind, is_primary, hand)
                     VALUES (?1, ?2, ?3, ?4, 1)",
                    rusqlite::params![series_id, title, kind, primary],
                )
                .expect("plants a hand title");
            }
        }
        titled
    };

    let grid = get_json(router, "/api/library/series").await;
    let listed = grid["series"].as_array().expect("the grid lists series");
    assert_eq!(listed.len(), 3, "the fixture's three series: {grid}");
    for (at, series) in listed.iter().enumerate() {
        let id = series["id"].as_i64().expect("a series id");
        let at = i64::try_from(at).expect("a small position");
        assert!(
            id != at && id != at + 1,
            "series {id} lists at position {at}, so a read that looked a series \
             up by its place in a list would find the right one by coincidence: \
             {grid}"
        );
    }
    titled
}

/// `series.titles` from `/api/library/series/{id}`, panicking with the whole
/// body when it is not an array.
async fn titles_of(router: &axum::Router, series_id: i64) -> Vec<serde_json::Value> {
    let body = get_json(router, &format!("/api/library/series/{series_id}")).await;
    body["series"]["titles"]
        .as_array()
        .unwrap_or_else(|| panic!("series {series_id} carries no `titles` array: {body}"))
        .clone()
}

/// Each entry of `got` against `want`, field by field, with nothing beside
/// those three fields, then the count.
fn assert_titles(series_id: i64, got: &[serde_json::Value], want: &[(&str, &str, bool)]) {
    for (at, (title, kind, primary)) in want.iter().enumerate() {
        let entry = got
            .get(at)
            .unwrap_or_else(|| panic!("series {series_id} has no title {at}, `{title}`: {got:?}"));
        assert_eq!(
            entry["title"], *title,
            "series {series_id}, title {at}: its text: {entry}"
        );
        assert_eq!(
            entry["kind"], *kind,
            "series {series_id}, title {at}: its kind: {entry}"
        );
        assert_eq!(
            entry["primary"], *primary,
            "series {series_id}, title {at}: whether it is primary: {entry}"
        );
        assert_eq!(
            entry.as_object().map(serde_json::Map::len),
            Some(3),
            "series {series_id}, title {at}: exactly `title`, `kind` and \
             `primary`: {entry}"
        );
    }
    assert_eq!(
        got.len(),
        want.len(),
        "series {series_id}'s titles: {got:?}"
    );
}

/// **O83.** `/api/library/series/{id}` carries every title the series holds,
/// each in the `{title, kind, primary}` shape `PATCH /api/library/series/{id}`
/// takes. That route replaces the whole set, and `SeriesCard.title` is one
/// string, the primary title, so without this a screen could only send a list that drops
/// the series' other titles.
///
/// All three series are asked for, so a read answering a fixed id, or a
/// neighbour's titles, is wrong for at least two of them.
#[tokio::test]
async fn one_series_carries_each_of_its_titles_in_the_shape_the_patch_route_takes() {
    let (router, state, _fixture) = test_support::router_over_three_unrelated_series();
    let titled = plant_titles(&router, &state).await;

    for (series_id, want) in [
        (titled.gotoubun, GOTOUBUN),
        (titled.kaguya, KAGUYA),
        (titled.oshinoko, OSHINOKO),
    ] {
        assert_titles(series_id, &titles_of(&router, series_id).await, want);
    }
}

/// **O83's round trip.** Read a series' titles, correct the romaji one, send
/// the whole list back, and read again: the native and alt titles come back
/// as they went, `kind` and `primary` included, and neither other series'
/// titles move.
#[tokio::test]
async fn a_series_titles_read_corrected_and_sent_back_whole_keep_every_other_title() {
    let (router, state, _fixture) = test_support::router_over_three_unrelated_series();
    let ids = plant_titles(&router, &state).await;
    let corrected = "Kaguya-sama wa Kokurasetai: Tensai-tachi no Ren'ai Zunousen";

    let mut titles = titles_of(&router, ids.kaguya).await;
    let romaji: Vec<usize> = titles
        .iter()
        .enumerate()
        .filter(|(_, title)| title["kind"] == "romaji")
        .map(|(at, _)| at)
        .collect();
    assert_eq!(romaji, [0], "one romaji title, first: {titles:?}");
    titles[0]["title"] = corrected.into();

    let response = test_support::send_json(
        &router,
        axum::http::Method::PATCH,
        &format!("/api/library/series/{}", ids.kaguya),
        &serde_json::json!({ "titles": titles }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{text}");

    assert_titles(
        ids.kaguya,
        &titles_of(&router, ids.kaguya).await,
        &[(corrected, "romaji", true), KAGUYA[1], KAGUYA[2]],
    );
    assert_titles(
        ids.gotoubun,
        &titles_of(&router, ids.gotoubun).await,
        GOTOUBUN,
    );
    assert_titles(
        ids.oshinoko,
        &titles_of(&router, ids.oshinoko).await,
        OSHINOKO,
    );
}

/// **O114.** O83's drive, as a test: Kaguya's derived romaji title corrected
/// through `PATCH /api/library/series/{id}` and the root scanned again, which
/// must leave the same three series, Kaguya's volume on its series, and the
/// corrected titles on it.
#[tokio::test]
async fn a_rescan_after_a_series_titles_correction_keeps_the_series_its_work_and_its_titles() {
    let (router, state, _fixture) = test_support::router_over_three_unrelated_series();
    let ids = plant_titles(&router, &state).await;
    let corrected = "Kaguya-sama wa Kokurasetai: Tensai-tachi no Ren'ai Zunousen";

    let mut titles = titles_of(&router, ids.kaguya).await;
    assert_eq!(
        titles[0]["title"], "Kaguya-sama wa Kokurasetai",
        "the first entry is the text the files gave the series: {titles:?}"
    );
    titles[0]["title"] = corrected.into();
    let response = test_support::send_json(
        &router,
        axum::http::Method::PATCH,
        &format!("/api/library/series/{}", ids.kaguya),
        &serde_json::json!({ "titles": titles }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{text}");

    {
        let conn = state.library.conn();
        for root in taiyaku_library::scan::roots(&conn).expect("reads the roots") {
            taiyaku_library::scan::scan_root(&conn, &root).expect("rescans the root");
        }
        taiyaku_library::derive::derive(&conn).expect("derives again");
    }

    let grid = get_json(&router, "/api/library/series").await;
    let mut listed: Vec<i64> = grid["series"]
        .as_array()
        .expect("the grid lists series")
        .iter()
        .map(|series| series["id"].as_i64().expect("a series id"))
        .collect();
    listed.sort_unstable();
    let mut want = vec![ids.gotoubun, ids.kaguya, ids.oshinoko];
    want.sort_unstable();
    assert_eq!(
        listed, want,
        "the same three series, and no second Kaguya: {grid}"
    );
    let one = get_json(&router, &format!("/api/library/series/{}", ids.kaguya)).await;
    assert_eq!(
        one["works"].as_array().map(Vec::len),
        Some(1),
        "Kaguya's volume stays on series {}: {one}",
        ids.kaguya
    );
    assert_titles(
        ids.kaguya,
        &titles_of(&router, ids.kaguya).await,
        &[(corrected, "romaji", true), KAGUYA[1], KAGUYA[2]],
    );
    assert_titles(
        ids.gotoubun,
        &titles_of(&router, ids.gotoubun).await,
        GOTOUBUN,
    );
    assert_titles(
        ids.oshinoko,
        &titles_of(&router, ids.oshinoko).await,
        OSHINOKO,
    );
}

/// The principal `auth::guard` leaves for the fixture account demoted to a
/// reader with `access`, for a handler called directly. An administrator's
/// access is always `Every` (O86), so a reader is what makes
/// `Access::roots(vec![1])` a restriction.
fn a_reader_with(access: Access) -> crate::auth::Principal {
    crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::User,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access,
    }
}

/// **No browse route stalls the runtime** — O116: the property
/// `routes::tests::no_works_route_stalls_the_runtime` and
/// `curation::tests::no_curation_route_stalls_the_runtime` hold for their
/// modules, held for this one.
///
/// **Each access, because none is spared here.** `Every` is answered
/// without the library by `routes::on_the_grant`, but `browse` applies a grant
/// in SQL, so every browse request reads the library whatever the caller holds.
///
/// **Every call is asserted to answer as it does for a reader who can see the
/// series**, so none is free for having been refused, except under `Only`
/// with no directory: there the series, the work and the cover each answer the
/// 404 one outside the grant gets, after asking the library. Every member of
/// the series' one work is under root 1.
///
/// **The watchdog measures each call's first wait on the library's mutex**
/// ([`test_support::the_runtime_stalls_on`]). `series` and `one_series` each
/// ask two questions, so a second wait on the async worker after the first
/// would pass here; each asks both in one trip.
#[tokio::test(flavor = "current_thread")]
async fn no_browse_route_stalls_the_runtime() {
    use crate::library_api::{GridQuery, HomeQuery, SearchQuery, SeriesQuery};
    use axum::extract::{Extension, Path as AxPath, Query, State};
    use axum::http::StatusCode;
    use std::sync::Arc;

    let (_router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    // Read before any watchdog holds the mutex: this is the fixture, not the
    // measurement.
    let slug: String = {
        let conn = state.library.conn();
        conn.execute_batch(
            "INSERT INTO root (id, path) VALUES (1, '/held');
             UPDATE member SET root_id = 1;",
        )
        .expect("puts every member under root 1");
        conn.query_row("SELECT slug FROM work", [], |row| row.get(0))
            .expect("the fixture's one work")
    };
    let library = Arc::clone(&state.library);
    let mut stalled_on = Vec::new();

    for (grant, access) in [
        ("granted root 1", Access::roots(vec![1])),
        ("every library", Access::Every),
        ("granted nothing", Access::roots(Vec::new())),
    ] {
        let who = || Extension(a_reader_with(access.clone()));
        let found = |status| {
            if access == Access::roots(Vec::new()) {
                StatusCode::NOT_FOUND
            } else {
                status
            }
        };
        let calls: Vec<(&str, StatusCode, _)> = vec![
            (
                "home",
                StatusCode::OK,
                Box::pin(crate::library_api::home(
                    State(Arc::clone(&state)),
                    who(),
                    Query(HomeQuery { library: None }),
                ))
                    as std::pin::Pin<Box<dyn std::future::Future<Output = _> + Send>>,
            ),
            (
                "series",
                StatusCode::OK,
                Box::pin(crate::library_api::series(
                    State(Arc::clone(&state)),
                    who(),
                    Query(GridQuery {
                        library: None,
                        root: None,
                    }),
                )),
            ),
            (
                "one_series",
                found(StatusCode::OK),
                Box::pin(crate::library_api::one_series(
                    State(Arc::clone(&state)),
                    who(),
                    AxPath(1),
                    Query(SeriesQuery { library: None }),
                )),
            ),
            (
                "work",
                found(StatusCode::OK),
                Box::pin(crate::library_api::work(
                    State(Arc::clone(&state)),
                    who(),
                    AxPath(slug.clone()),
                )),
            ),
            (
                "search",
                StatusCode::OK,
                Box::pin(crate::library_api::search(
                    State(Arc::clone(&state)),
                    who(),
                    Query(SearchQuery {
                        q: "Scanned".to_string(),
                        library: None,
                    }),
                )),
            ),
            (
                "cover",
                found(StatusCode::TEMPORARY_REDIRECT),
                Box::pin(crate::library_api::cover(
                    State(Arc::clone(&state)),
                    who(),
                    AxPath(1),
                    Query(Vec::new()),
                )),
            ),
            (
                "status",
                StatusCode::OK,
                Box::pin(crate::library_api::status(State(Arc::clone(&state)), who())),
            ),
        ];
        for (handler, expected, call) in calls {
            let (stalled, response) = test_support::the_runtime_stalls_on(&library, call).await;
            let call = format!("library_api::{handler}, {grant}");
            if stalled {
                stalled_on.push(call.clone());
            }
            let status = response.status();
            assert_eq!(
                status,
                expected,
                "{call}: {}",
                test_support::body_string(response).await
            );
        }
    }

    assert!(
        stalled_on.is_empty(),
        "these took the library on the async worker: {stalled_on:?}"
    );
}

/// `address` asked for at the smallest cover size, as the web client's
/// `coverSources` adds `w` to it.
fn at_a_cover_size(address: &str) -> String {
    let join = if address.contains('?') { '&' } else { '?' };
    format!("{address}{join}w=256")
}

/// **Every cover address the list answers carry is kept for good by the
/// thumbnail route**: fetched at a cover size, each is a `200` sent
/// `COVER_CACHE_CONTROL`, which the route sends only for a `v` equal to the
/// version of the page it serves. So the hash and the offset an answer
/// versions its address by are the ones the route locates page 0 in.
///
/// The grid's row, a series page's own row, a search hit, and every edition
/// card of the series page and of home's `added` list. A series row's address
/// is the `307`'s target with the version added.
///
/// Over a scanned library, so the hashes are the scan's and the served
/// registry is projected from the rows the answers read.
#[tokio::test]
async fn every_cover_address_the_list_answers_carry_is_kept_for_good_by_the_thumbnail_route() {
    let (router, state, _fixture) =
        test_support::router_over_two_scanned_volumes_of_decodable_pages();
    let series_id: i64 = {
        let conn = state.library.conn();
        conn.query_row("SELECT id FROM series", [], |row| row.get(0))
            .expect("the fixture's one series")
    };
    let grid = get_json(&router, "/api/library/series").await;
    let one = get_json(&router, &format!("/api/library/series/{series_id}")).await;
    let hits = get_json(&router, "/api/library/search?q=Hanayome").await;
    let home = get_json(&router, "/api/library/home").await;

    let address = |what: &str, value: &serde_json::Value| -> (String, String) {
        let address = value
            .as_str()
            .unwrap_or_else(|| panic!("{what}: a cover address, not {value}"));
        (what.to_string(), address.to_string())
    };
    let rows = [
        address("the grid's row", &grid["series"][0]["cover"]),
        address("the series page's own row", &one["series"]["cover"]),
        address("the search hit", &hits["series"][0]["cover"]),
    ];
    let mut cards = Vec::new();
    for (list, works) in [
        ("the series page", &one["works"]),
        ("home's added", &home["added"]),
    ] {
        let works = works.as_array().unwrap_or_else(|| panic!("{list}: works"));
        assert_eq!(works.len(), 2, "{list}: both volumes");
        for work in works {
            for edition in work["editions"].as_array().expect("editions") {
                let what = format!("{list}: {} {}", work["slug"], edition["key"]);
                cards.push(address(&what, &edition["cover"]));
            }
        }
    }
    assert_eq!(cards.len(), 4, "one edition of each volume, in each list");

    let redirect = get(&router, &format!("/api/library/series/{series_id}/cover")).await;
    assert_eq!(
        redirect.status(),
        axum::http::StatusCode::TEMPORARY_REDIRECT
    );
    let target = redirect.headers()[axum::http::header::LOCATION]
        .to_str()
        .expect("ascii")
        .to_string();
    for (what, address) in &rows {
        assert_eq!(
            address.split_once("?v=").map(|(path, _)| path),
            Some(target.as_str()),
            "{what}: the 307's target, versioned"
        );
    }

    for (what, address) in rows.iter().chain(&cards) {
        let uri = at_a_cover_size(address);
        let response = get(&router, &uri).await;
        let status = response.status();
        let cache = response
            .headers()
            .get(axum::http::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert_eq!(
            (status, cache.as_str()),
            (
                axum::http::StatusCode::OK,
                crate::routes::COVER_CACHE_CONTROL
            ),
            "{what}: {uri}"
        );
    }
}

/// A cover address's segments are encoded as `encodeURIComponent` encodes a
/// component, which leaves `-_.!~*'()` as they are, and its `v` is the cover
/// version of page 0 of the member named.
#[test]
fn a_cover_address_encodes_its_segments_as_encode_uri_component_does() {
    assert_eq!(
        super::cover_address("a b_c.d-e", "en (scan)/é!*'~", "abc"),
        format!(
            "/api/works/a%20b_c.d-e/en%20(scan)%2F%C3%A9!*'~/pages/0/thumb?v={}",
            crate::cover::cover_version("abc", 0)
        )
    );
}

/// A member's `sha256` is what its sidecar says, so the version is encoded as
/// a segment is: a hash holding `&`, `+`, a space or `#` would otherwise end
/// `v`, or the query, early, or read as other text.
#[test]
fn a_cover_address_encodes_its_version() {
    let version = crate::cover::cover_version("x&y+z #", 0);
    assert_eq!(
        super::cover_address("w", "en", "x&y+z #"),
        format!(
            "/api/works/w/en/pages/0/thumb?v={}",
            version
                .replace('&', "%26")
                .replace('+', "%2B")
                .replace(' ', "%20")
                .replace('#', "%23")
        )
    );
}

/// The address `cover_address`, which the answers call, builds for a hash
/// that is not hex, as an imported sidecar may hold, is one the thumbnail
/// route keeps for good.
#[tokio::test]
async fn a_cover_address_for_a_hash_that_is_not_hex_is_kept_for_good() {
    let (state, _fixture) = test_support::state_with_tall_pages_hashed("x&y+z");
    let router = test_support::authenticating(crate::build_router(state.clone()), &state.library);
    let uri = at_a_cover_size(&super::cover_address("w", "a", "x&y+z"));
    let response = get(&router, &uri).await;
    let cache = response
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert_eq!(
        (response.status(), cache.as_str()),
        (
            axum::http::StatusCode::OK,
            crate::routes::COVER_CACHE_CONTROL
        ),
        "{uri}"
    );
}

/// A series row's `cover` is null when the series has no cover edition, or
/// when that edition holds no page; an edition card's is null when it holds no
/// page. Otherwise each is its page 0's address.
#[test]
fn a_cover_with_no_page_zero_is_null() {
    use taiyaku_library::browse::{CoverTarget, EditionCard, SeriesCard, WorkCard};
    let target = |page: Option<&str>| CoverTarget {
        work_id: 7,
        slug: "w".into(),
        edition: "en".into(),
        page_zero_sha256: page.map(str::to_string),
    };
    let row = |cover: Option<CoverTarget>| SeriesCard {
        id: 1,
        title: "S".into(),
        works: 1,
        editions: 1,
        aligned: 0,
        cover_work: None,
        added: 7,
        unread: 1,
        cover,
    };
    assert_eq!(
        super::series_json(&row(None))["cover"],
        serde_json::Value::Null
    );
    assert_eq!(
        super::series_json(&row(Some(target(None))))["cover"],
        serde_json::Value::Null
    );
    assert_eq!(
        super::series_json(&row(Some(target(Some("abc")))))["cover"],
        super::cover_address("w", "en", "abc")
    );

    let card = |key: &str, page: Option<&str>| EditionCard {
        key: key.into(),
        label: key.into(),
        members: 1,
        pages: 2,
        missing: 0,
        page_zero_sha256: page.map(str::to_string),
    };
    let work = WorkCard {
        id: 7,
        slug: "w".into(),
        title: "W".into(),
        volume: Some(1.0),
        editions: vec![
            card("en", None),
            card("fr", Some("abc")),
            card("raw", Some("def")),
        ],
        aligned: false,
        pages: 2,
        opens: "en".into(),
    };
    let editions = &super::work_json(&work)["editions"];
    assert_eq!(editions[0]["cover"], serde_json::Value::Null);
    assert_eq!(editions[1]["cover"], super::cover_address("w", "fr", "abc"));
    assert_eq!(
        editions[2]["cover"],
        super::cover_address("w", "raw", "def")
    );
}

/// Spec §3.8's cover version, over
/// [`test_support::router_over_an_edition_merged_from_two_libraries`].
mod narrowing {
    use super::*;
    use crate::test_support::{EditionMergedFromTwoLibraries, Reader};

    /// Every cover address `reader`'s grid, series page and home carry under
    /// `query`, each named by where it was found.
    async fn cover_addresses(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        query: &str,
    ) -> Vec<(String, String)> {
        let router = merged.router(reader);
        let grid = get_json(router, &format!("/api/library/series{query}")).await;
        let id = &grid["series"][0]["id"];
        let one = get_json(router, &format!("/api/library/series/{id}{query}")).await;
        let home = get_json(router, &format!("/api/library/home{query}")).await;
        let mut addresses = Vec::new();
        for (what, value) in [
            ("the grid's row", &grid["series"][0]["cover"]),
            ("the series page's own row", &one["series"]["cover"]),
        ] {
            addresses.push((what.to_string(), value.to_string()));
        }
        for (list, works) in [
            ("the series page", &one["works"]),
            ("home's added", &home["added"]),
        ] {
            for work in works.as_array().unwrap_or_else(|| panic!("{list}: works")) {
                for edition in work["editions"].as_array().expect("editions") {
                    addresses.push((
                        format!("{list}: {}", edition["key"]),
                        edition["cover"].to_string(),
                    ));
                }
            }
        }
        addresses
            .into_iter()
            .map(|(what, address)| {
                let address: String = serde_json::from_str(&address).unwrap_or_else(|_| {
                    panic!("{reader:?} {query} {what}: an address, not {address}")
                });
                (format!("{reader:?} {query} {what}"), address)
            })
            .collect()
    }

    /// `raw`'s member `file`'s sha256, from the sidecar every account shares.
    fn sha_of(merged: &EditionMergedFromTwoLibraries, file: &str) -> String {
        let work = merged.state.find_blocking(&merged.slug).expect("the work");
        let sidecar = work.sidecar();
        sidecar.editions["raw"]
            .members
            .iter()
            .find(|member| member.path.ends_with(file))
            .unwrap_or_else(|| panic!("no member {file}"))
            .sha256
            .clone()
    }

    /// The pairs `reader`'s work document carries, and the alignments its
    /// `/api/library/works/{slug}` lists with their counts.
    async fn alignments_of(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
    ) -> (Vec<(String, String)>, Vec<serde_json::Value>) {
        let router = merged.router(reader);
        let document = get_json(router, &format!("/api/works/{}", merged.slug)).await;
        let pairs = document["alignments"]
            .as_array()
            .expect("the document's alignments")
            .iter()
            .map(|a| (a["from"].to_string(), a["to"].to_string()))
            .collect();
        let detail = get_json(router, &format!("/api/library/works/{}", merged.slug)).await;
        let listed = detail["alignments"]
            .as_array()
            .expect("the work's alignments")
            .clone();
        (pairs, listed)
    }

    /// An alignment's `slots` and `paired` count its editions in the full
    /// numbering, so the library route lists one only when both its editions
    /// are seen whole, the pairs the work document carries: Manga, who sees
    /// `raw` narrowed, is told neither `raw`↔`en` nor its 9 slots, from which
    /// part2's size follows. The work card's `aligned` says the same.
    #[tokio::test]
    async fn the_work_route_lists_only_the_alignments_the_work_document_carries() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let (_, everything) = alignments_of(&merged, Reader::Administrator).await;
        assert_eq!(
            everything,
            [
                serde_json::json!({"from": "en", "to": "fr", "verdict": "sound", "slots": 5, "paired": 5}),
                serde_json::json!({"from": "raw", "to": "en", "verdict": "sound", "slots": 9, "paired": 7}),
            ],
            "the fixture: both alignments carry counts, raw's over its full 9 slots"
        );

        let raw_en = (r#""raw""#.to_string(), r#""en""#.to_string());
        let en_fr = (r#""en""#.to_string(), r#""fr""#.to_string());
        for (reader, pairs) in [
            (Reader::Administrator, vec![raw_en.clone(), en_fr.clone()]),
            (Reader::Both, vec![raw_en.clone(), en_fr.clone()]),
            (Reader::MangaOnly, vec![en_fr.clone()]),
            (Reader::KidsOnly, vec![]),
        ] {
            let (document, listed) = alignments_of(&merged, reader).await;
            let mut sorted = document.clone();
            sorted.sort();
            let mut expected = pairs.clone();
            expected.sort();
            assert_eq!(sorted, expected, "{reader:?}: the work document's pairs");
            let listed: Vec<(String, String)> = listed
                .iter()
                .map(|a| (a["from"].to_string(), a["to"].to_string()))
                .collect();
            let mut listed_sorted = listed.clone();
            listed_sorted.sort();
            assert_eq!(
                listed_sorted, expected,
                "{reader:?}: the library route lists the pairs the work document carries"
            );
        }

        // With `en`↔`fr` gone, Manga's only pair touches `raw`: its card says
        // it has none, and Both's, which sees `raw` whole, that it has one.
        // Narrowed to Manga's library the card still says what the account
        // opens, `raw` whole for Both and the administrator.
        merged
            .state
            .library
            .conn()
            .execute("DELETE FROM alignment WHERE from_key = 'en'", [])
            .expect("drops en↔fr");
        let series: i64 = merged
            .state
            .library
            .conn()
            .query_row("SELECT series_id FROM work", [], |row| row.get(0))
            .expect("the work's series");
        for (reader, query, aligned) in [
            (Reader::MangaOnly, "", false),
            (Reader::Both, "", true),
            (Reader::Both, "?library=70", true),
            (Reader::Administrator, "?library=70", true),
        ] {
            let one = get_json(
                merged.router(reader),
                &format!("/api/library/series/{series}{query}"),
            )
            .await;
            assert_eq!(
                one["works"][0]["aligned"],
                serde_json::json!(aligned),
                "{reader:?} {query}: the work card's `aligned`"
            );
            assert_eq!(
                one["series"]["aligned"],
                serde_json::json!(i64::from(aligned)),
                "{reader:?} {query}: the series card's count"
            );
        }
    }

    /// `raw` as `reader`'s series page opened with `query` lists it: its
    /// files, pages and missing files.
    async fn listed_raw(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        query: &str,
    ) -> (u64, u64, u64) {
        let series: i64 = merged
            .state
            .library
            .conn()
            .query_row("SELECT series_id FROM work", [], |row| row.get(0))
            .expect("the work's series");
        let one = get_json(
            merged.router(reader),
            &format!("/api/library/series/{series}{query}"),
        )
        .await;
        let raw = one["works"][0]["editions"]
            .as_array()
            .unwrap_or_else(|| panic!("{reader:?} {query}: editions"))
            .iter()
            .find(|edition| edition["key"] == "raw")
            .unwrap_or_else(|| panic!("{reader:?} {query}: raw is listed"))
            .clone();
        let count = |field: &str| {
            raw[field]
                .as_u64()
                .unwrap_or_else(|| panic!("{reader:?} {query}: raw's {field}"))
        };
        (count("members"), count("pages"), count("missing"))
    }

    /// Home opened from a library counts each card on each of its strips as
    /// the series page does (decision 15): by what the account opens, 9 pages
    /// of `raw` for the administrator and Both where Kids' share is 3. The
    /// work is on `added`, then on `continuing` once the reader holds a
    /// position on it, then on `read` once that position is finished.
    #[tokio::test]
    async fn home_counts_an_edition_by_what_its_account_opens_whichever_library_it_is_opened_from()
    {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let work_id: i64 = merged
            .state
            .library
            .conn()
            .query_row("SELECT id FROM work", [], |row| row.get(0))
            .expect("the work");
        for (reader, query, members, pages) in [
            (Reader::Administrator, "?library=80", 3, 9),
            (Reader::Administrator, "?library=70", 3, 9),
            (Reader::Both, "?library=80", 3, 9),
            (Reader::KidsOnly, "?library=80", 1, 3),
            (Reader::MangaOnly, "?library=70", 2, 6),
        ] {
            merged
                .state
                .library
                .conn()
                .execute("DELETE FROM progress", [])
                .expect("clears positions");
            for (strip, finished) in [
                ("added", None),
                ("continuing", Some(false)),
                ("read", Some(true)),
            ] {
                if let Some(finished) = finished {
                    merged
                        .state
                        .library
                        .conn()
                        .execute(
                            "INSERT OR REPLACE INTO progress
                                    (user_id, work_id, edition, slot, completed_at)
                             VALUES (?1, ?2, 'raw', 0,
                                     CASE WHEN ?3 THEN datetime('now') END)",
                            rusqlite::params![merged.user_id(reader), work_id, finished],
                        )
                        .expect("a position");
                }
                let home =
                    get_json(merged.router(reader), &format!("/api/library/home{query}")).await;
                let card = home[strip]
                    .as_array()
                    .and_then(|cards| {
                        cards
                            .iter()
                            .find(|card| card["slug"] == merged.slug.as_str())
                    })
                    .unwrap_or_else(|| panic!("{reader:?} {query}: the work on {strip}: {home}"));
                assert_eq!(
                    raw_of(card, &format!("{reader:?} {query} {strip}")),
                    (members, pages),
                    "{reader:?} {query}: home's {strip} card's raw"
                );
            }
        }
    }

    /// The files and pages of `raw` on a work `card`.
    fn raw_of(card: &serde_json::Value, what: &str) -> (u64, u64) {
        let raw = card["editions"]
            .as_array()
            .unwrap_or_else(|| panic!("{what}: editions"))
            .iter()
            .find(|edition| edition["key"] == "raw")
            .unwrap_or_else(|| panic!("{what}: raw is listed"));
        let count = |field: &str| {
            raw[field]
                .as_u64()
                .unwrap_or_else(|| panic!("{what}: raw's {field}"))
        };
        (count("members"), count("pages"))
    }

    /// A series page opened from a library counts each edition it lists by
    /// what the account opens, not by the library's share of it (decision
    /// 15): the cell and the download label promise the pages the work
    /// document and the download give. Kids' directory holds part2 alone, so
    /// a count through the listing's library gives 3 where the administrator
    /// and Both open 9, and Manga's gives 6.
    #[tokio::test]
    async fn a_series_page_counts_an_edition_by_what_its_account_opens_whichever_library_it_is_opened_from()
     {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        for (reader, query, members, pages) in [
            (Reader::Administrator, "?library=80", 3, 9),
            (Reader::Administrator, "?library=70", 3, 9),
            (Reader::Both, "?library=80", 3, 9),
            (Reader::Both, "?library=70", 3, 9),
            (Reader::KidsOnly, "?library=80", 1, 3),
            (Reader::MangaOnly, "?library=70", 2, 6),
        ] {
            let (listed_members, listed_pages, _) = listed_raw(&merged, reader, query).await;
            assert_eq!(
                (listed_members, listed_pages),
                (members, pages),
                "{reader:?} {query}: the series page's raw"
            );

            let document = get_json(
                merged.router(reader),
                &format!("/api/works/{}", merged.slug),
            )
            .await;
            let opened: Vec<u64> = document["editions"]["raw"]["members"]
                .as_array()
                .expect("the document's raw members")
                .iter()
                .map(|member| member["pages"].as_u64().expect("pages"))
                .collect();
            assert_eq!(
                (opened.len() as u64, opened.iter().sum::<u64>()),
                (members, pages),
                "{reader:?}: the work document's raw"
            );

            let sent = merged
                .get(
                    reader,
                    &format!("/api/works/{}/raw/download", merged.slug),
                    None,
                )
                .await;
            assert_eq!(
                sent.status(),
                axum::http::StatusCode::OK,
                "{reader:?}: the download"
            );
            let mut zipped = zip::ZipArchive::new(std::io::Cursor::new(sent.body().as_slice()))
                .expect("the download is a zip");
            let zipped_pages = (0..zipped.len())
                .filter(|&i| {
                    zipped
                        .by_index(i)
                        .expect("an entry")
                        .name()
                        .expect("a name")
                        != "ComicInfo.xml"
                })
                .count() as u64;
            assert_eq!(
                zipped_pages, pages,
                "{reader:?}: the pages the download zips"
            );
        }
    }

    /// A series page opened from a library counts an edition's missing files
    /// by the account too: with part1, under Manga's directory, gone from the
    /// scan, Both's `raw` opened from Kids' library is unreadable, as the
    /// reader would find it, and Kids alone's is whole.
    #[tokio::test]
    async fn a_series_page_counts_missing_files_by_what_its_account_opens() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let forgotten = merged
            .state
            .library
            .conn()
            .execute(
                "DELETE FROM scan_entry WHERE rel_path = 'Foo v01 part1.cbz'",
                [],
            )
            .expect("forgets part1");
        assert_eq!(forgotten, 1, "the fixture: part1 was scanned once");
        for (reader, missing) in [
            (Reader::Administrator, 1),
            (Reader::Both, 1),
            (Reader::KidsOnly, 0),
        ] {
            let (_, _, listed) = listed_raw(&merged, reader, "?library=80").await;
            assert_eq!(
                listed, missing,
                "{reader:?} ?library=80: raw's missing files"
            );
        }
    }

    /// A cover address names the page 0 the account is served, so the thumb
    /// keeps it for good: Kids alone sees `raw` as part2, whose first page is
    /// its page 0. An account that sees part1 is versioned by part1's, a
    /// listing narrowed to Kids' library included, since the thumb narrows by
    /// the account's grant and not the listing's.
    #[tokio::test]
    async fn each_cover_address_names_the_page_zero_its_account_is_served() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let part1 = crate::cover::cover_version(&sha_of(&merged, "Foo v01 part1.cbz"), 0);
        let part2 = crate::cover::cover_version(&sha_of(&merged, "Foo v01 part2.cbz"), 0);
        assert_ne!(
            part1, part2,
            "the fixture: part1's and part2's first pages differ"
        );

        for (reader, query, raw) in [
            (Reader::Administrator, "", &part1),
            (Reader::Administrator, "?library=80", &part1),
            (Reader::Both, "?library=80", &part1),
            (Reader::MangaOnly, "", &part1),
            (Reader::KidsOnly, "", &part2),
            (Reader::KidsOnly, "?library=80", &part2),
        ] {
            let addresses = cover_addresses(&merged, reader, query).await;
            let of_raw: Vec<&(String, String)> = addresses
                .iter()
                .filter(|(_, address)| address.contains("/raw/"))
                .collect();
            assert!(!of_raw.is_empty(), "{reader:?} {query}: an address of raw");
            for (what, address) in of_raw {
                assert_eq!(
                    address.split_once("?v=").map(|(_, v)| v),
                    Some(raw.as_str()),
                    "{what}: {address}"
                );
            }
            for (what, address) in &addresses {
                let response = get(merged.router(reader), &at_a_cover_size(address)).await;
                let cache = response
                    .headers()
                    .get(axum::http::header::CACHE_CONTROL)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                assert_eq!(
                    (response.status(), cache.as_str()),
                    (
                        axum::http::StatusCode::OK,
                        crate::routes::COVER_CACHE_CONTROL
                    ),
                    "{what}: {address}"
                );
            }
        }
    }
}
