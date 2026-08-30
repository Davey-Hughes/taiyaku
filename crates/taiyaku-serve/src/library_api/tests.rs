use crate::test_support;
// `get` and `get_json` were private here until C3's `curation` module needed
// the same two. Promoted rather than copied: `get_json`'s panic is
// load-bearing (see its own doc comment), and two copies would be two places
// to forget it. `get` sends no credential of its own — the fixture's router
// injects one — which is what lets a *reader* fixture answer as a reader
// through the very same helper.
use crate::test_support::{get, get_json};
use tower::ServiceExt as _;

/// The same request with no credentials at all.
///
/// **Takes a bare router deliberately.** `test_support::router_over_a_library`
/// wraps its router in `authenticating`, which injects the fixture's
/// credentials into every request — so a request sent through it is never
/// anonymous, whatever headers the caller omits. The first version of this
/// helper used that router and reported a 200, which is how the trap was
/// found. Callers must build the router with `crate::build_router` directly.
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
    let (_authenticated, state) = test_support::router_over_a_library().await;
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
    let (router, _state) = test_support::router_over_a_library().await;
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
    let (router, state) = test_support::router_over_a_library().await;
    let slug = state.works[0].id.clone();
    {
        // O73/Task 7b: `EditionCard::missing` has to reach the client or
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
    let (router, _state) = test_support::router_over_a_library().await;
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
    let (router, _state) = test_support::router_over_a_library().await;

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
    let (router, _state) = test_support::router_over_a_library().await;
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

/// A series this caller cannot see is a 404, the same answer as one that does
/// not exist — spec C1 §5.1, and the enumeration refusal this project applies
/// elsewhere.
#[tokio::test]
async fn a_series_that_does_not_exist_is_a_404() {
    let (router, _state) = test_support::router_over_a_library().await;
    let response = get(&router, "/api/library/series/9999").await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

/// `/api/library/series/{id}` — the handler that had no test at all, which
/// the coverage gate caught before the review did.
#[tokio::test]
async fn one_series_carries_the_series_and_its_works() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let series_id: i64 = {
        let conn = state.library.conn();
        conn.query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
            .expect("reads the series")
    };

    let body = get_json(&router, &format!("/api/library/series/{series_id}")).await;
    assert_eq!(body["series"]["id"], series_id);
    assert_eq!(body["series"]["title"], "Scanned");
    assert_eq!(
        body["series"]["cover"],
        format!("/api/library/series/{series_id}/cover")
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

/// A cover for a series that does not exist is a **404, not a 500** — the
/// difference was an enumeration oracle, since `/series/{id}` already answered
/// 404. Whole-branch review.
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
/// Every handler funnels its failures through one `failed` helper, and until
/// this test nothing ever invoked it — the error arms were written, compiled
/// and never run. Driven by dropping the tables the queries read, which is the
/// bluntest way to make a real `rusqlite` error arrive at the handler rather
/// than a mocked one.
#[tokio::test]
async fn a_store_error_is_a_500_rather_than_a_panic() {
    let (router, state) = test_support::router_over_a_library().await;
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
/// This is spec C1 §5.1's boundary at the route rather than in the query, and
/// it exists because a mutation sweep on 2026-08-20 found that
/// `refuse_if_hidden` could be replaced with `None` — the check deleted
/// outright — with every test still passing. The whole-branch review found the
/// missing guard; the sweep found that the fix was unpinned.
#[tokio::test]
async fn a_restricted_reader_is_refused_the_pages_of_a_root_they_do_not_hold() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let slug = state.works[0].id.clone();
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
        conn.execute("INSERT INTO user_root (user_id, root_id) VALUES (1, 1)", [])
            .expect("grants the fixture account the other root");
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
        // way — an earlier version of this test asserted the status alone and
        // passed with the guard deleted, satisfied by a coincidence. Deleting
        // it makes these routes answer the *filesystem* instead, naming an
        // absolute path; the guard's own refusal names the work and nothing
        // else.
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
    let slug = state.works[0].id.clone();
    {
        let conn = state.library.conn();
        conn.execute("INSERT INTO root (id, path) VALUES (1, '/held')", [])
            .expect("one root");
        conn.execute("UPDATE member SET root_id = 1", [])
            .expect("files the material under it");
        conn.execute("INSERT INTO user_root (user_id, root_id) VALUES (1, 1)", [])
            .expect("grants it");
    }

    let response = get(&router, &format!("/api/works/{slug}")).await;
    assert_ne!(
        response.status(),
        axum::http::StatusCode::NOT_FOUND,
        "a granted reader must reach their own material"
    );
}

#[tokio::test]
async fn the_grid_carries_the_two_fields_the_chips_sort_and_filter_on() {
    // **Not `router_over_a_library`.** That fixture only imports a work via
    // `import::sidecar`, which per `browse.rs`'s own module docs never writes
    // `series_id` — so the grid it produces has an `unfiled` work and no
    // `series` at all, and `first()` on an empty array panicked here. This
    // fixture inserts a `series` row and files the work under it, the same
    // way `the_grid_lists_a_series_with_its_counts` above does.
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let body = get_json(&router, "/api/library/series").await;
    let first = body["series"]
        .as_array()
        .and_then(|s| s.first())
        .expect("at least one series");
    assert!(first["added"].is_i64(), "added is a number: {first}");
    assert!(first["unread"].is_i64(), "unread is a number: {first}");
}

/// **Over a fixture that actually has a series**, which is the whole of what
/// this test was missing: `router_over_a_library`'s grid holds zero series
/// whatever the filter, so the assertion below was satisfied by the fixture
/// rather than by the filter — replacing the handler's `query.root` with
/// `None` left all 291 `taiyaku-serve` tests passing. Whole-branch review,
/// mutation-checked 2026-08-21.
#[tokio::test]
async fn a_root_that_does_not_exist_answers_an_empty_grid_rather_than_an_error() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let body = get_json(&router, "/api/library/series?root=999999").await;
    assert_eq!(
        body["series"].as_array().map(Vec::len),
        Some(0),
        "no series, and a 200: {body}"
    );
}

/// The other half: a root that *does* exist answers the material under it, and
/// a second root answers none of it.
///
/// A root that does not exist and a root holding nothing are deliberately the
/// same answer — `only_clause`'s doc says why — so a test that only ever asked
/// for a missing one could not tell the filter from a handler that dropped the
/// parameter on the floor.
#[tokio::test]
async fn the_grid_narrows_to_the_root_the_query_names() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/elsewhere')",
            [],
        )
        .expect("two roots");
        conn.execute("UPDATE member SET root_id = 1", [])
            .expect("files every member under the first");
    }

    let all = get_json(&router, "/api/library/series").await;
    assert_eq!(
        all["series"].as_array().map(Vec::len),
        Some(1),
        "unnarrowed, the series is listed: {all}"
    );

    let held = get_json(&router, "/api/library/series?root=1").await;
    assert_eq!(
        held["series"].as_array().map(Vec::len),
        Some(1),
        "the root the material sits under: {held}"
    );

    let elsewhere = get_json(&router, "/api/library/series?root=2").await;
    assert_eq!(
        elsewhere["series"].as_array().map(Vec::len),
        Some(0),
        "a root holding none of it: {elsewhere}"
    );
}

/// **`?root=` empties the unfiled bucket**, which it did not until 2026-08-21:
/// `series` was fetched with the filter and `unfiled` without it, so
/// `/api/library/series?root=5` answered a root-scoped grid beside every
/// imported work in the library. An imported member sits under no root, and
/// asking for a root asks for what is under it — spec C1 §5.1's existing
/// NULL-excludes-itself rule, applied in SQL rather than special-cased here.
#[tokio::test]
async fn a_root_scoped_grid_carries_no_unfiled_work() {
    let (router, _state) = test_support::router_over_a_library().await;

    let all = get_json(&router, "/api/library/series").await;
    assert_eq!(
        all["unfiled"].as_array().map(Vec::len),
        Some(1),
        "the imported work is in the whole-library grid: {all}"
    );

    let scoped = get_json(&router, "/api/library/series?root=1").await;
    assert_eq!(
        scoped["unfiled"].as_array().map(Vec::len),
        Some(0),
        "and under no root at all: {scoped}"
    );
}

#[tokio::test]
async fn the_work_route_carries_an_alignments_array_and_the_list_routes_do_not() {
    // Not `router_over_a_library`: that fixture's work is imported via
    // `import::sidecar`, which per `browse.rs`'s own module docs never writes
    // `series_id` — so `/api/library/series` answers no series at all, `first()`
    // is `None`, and the negative half below never ran. This fixture inserts a
    // `series` row and files the work under it, so `listed` genuinely binds.
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

#[tokio::test]
async fn status_answers_the_two_facts_a_first_run_screen_needs() {
    let (router, _state) = test_support::router_over_a_library().await;
    let body = get_json(&router, "/api/library/status").await;
    assert!(body["roots"].is_array(), "roots is a list: {body}");
    assert!(body["scanned"].is_boolean(), "scanned is a bool: {body}");
}
