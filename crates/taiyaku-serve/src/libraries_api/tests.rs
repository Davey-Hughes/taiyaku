use crate::test_support::{self, HeldJob, get_json, holding_the_job, send_json};
use axum::http::{Method, StatusCode};

/// Sends, and answers the status with the body parsed as JSON.
async fn answered(
    router: &axum::Router,
    method: Method,
    path: &str,
    body: &serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = send_json(router, method, path, body).await;
    parsed(response).await
}

/// Sends `body` as it is written, which need not be JSON.
async fn answered_raw(
    router: &axum::Router,
    method: Method,
    path: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let response = test_support::send_raw(router, method, path, body).await;
    parsed(response).await
}

async fn parsed(
    response: axum::http::Response<axum::body::Body>,
) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let text = test_support::body_string(response).await;
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, json)
}

/// A refusal's body: exactly one key, `error`, holding a sentence.
fn the_sentence(refused: &serde_json::Value) -> &str {
    let object = refused
        .as_object()
        .unwrap_or_else(|| panic!("an object: {refused}"));
    assert_eq!(
        object.keys().collect::<Vec<_>>(),
        vec!["error"],
        "a refusal carries `error` and nothing else: {refused}"
    );
    let sentence = refused["error"].as_str().expect("an error string");
    assert!(!sentence.is_empty(), "{refused}");
    sentence
}

fn keys_of(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

/// A new directory under `fixture` holding one two-page volume of `series`.
fn a_dir_of(fixture: &test_support::Fixture, name: &str, series: &str) -> String {
    let dir = fixture.path().join(name);
    test_support::write_pages(&dir.join(series).join(format!("{series} - v01")), 2);
    dir.to_string_lossy().into_owned()
}

fn the_library<'a>(listed: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    listed["libraries"]
        .as_array()
        .expect("a libraries array")
        .iter()
        .find(|l| l["name"] == name)
        .unwrap_or_else(|| panic!("no library called {name}: {listed}"))
}

fn an_administrator() -> crate::auth::Principal {
    crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::Admin,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access: taiyaku_library::account::Access::Every,
    }
}

/// Every row of every table the store holds, as text, so a rehearsal or a
/// refusal can be compared with what it left. The tables are read from
/// `sqlite_master`, so one added later is held too.
///
/// Not `session`, `api_key` or `login_attempt`: a request's own sign-in
/// slides those.
fn the_store(state: &crate::ServeState) -> Vec<(String, Vec<String>)> {
    let conn = state.library.conn();
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
                AND name NOT IN ('session', 'api_key', 'login_attempt')
              ORDER BY name",
        )
        .expect("prepares the table list")
        .query_map([], |row| row.get(0))
        .expect("lists the tables")
        .collect::<Result<_, _>>()
        .expect("reads the table list");
    let mut dumped = Vec::new();
    for table in tables {
        let columns = conn
            .prepare(&format!("SELECT * FROM \"{table}\""))
            .expect("prepares a dump")
            .column_count();
        // Every column, so the order does not depend on a key the table may
        // not have.
        let order = (1..=columns)
            .map(|column| column.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = conn
            .prepare(&format!("SELECT * FROM \"{table}\" ORDER BY {order}"))
            .expect("prepares a dump");
        let rows = statement
            .query_map([], |row| {
                let mut cells = Vec::new();
                for column in 0..columns {
                    cells.push(format!(
                        "{:?}",
                        row.get::<_, rusqlite::types::Value>(column)?
                    ));
                }
                Ok(cells.join("|"))
            })
            .expect("dumps")
            .collect::<Result<Vec<_>, _>>()
            .expect("reads the dump");
        dumped.push((table, rows));
    }
    dumped
}

/// An unsigned router over the same state: no session cookie is added to what
/// it is sent.
fn unsigned(state: &std::sync::Arc<crate::ServeState>) -> axum::Router {
    crate::build_router(state.clone())
}

async fn a_reader_is_refused(method: Method, path: &str) {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let before = the_store(&state);
    let response = send_json(&router, method.clone(), path, &serde_json::json!({})).await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "{method} {path} answered a reader"
    );
    assert_eq!(
        the_store(&state),
        before,
        "{method} {path} wrote for a reader"
    );
}

async fn an_unsigned_request_is_refused(method: Method, path: &str) {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let router = unsigned(&state);
    let roots = send_json(
        &router,
        Method::GET,
        "/api/library/roots",
        &serde_json::json!({}),
    )
    .await;
    let wanted = roots.status();
    assert_eq!(
        wanted,
        StatusCode::UNAUTHORIZED,
        "the status a config route gives"
    );
    let before = the_store(&state);
    let response = send_json(&router, method.clone(), path, &serde_json::json!({})).await;
    assert_eq!(response.status(), wanted, "{method} {path} answered nobody");
    assert_eq!(
        the_store(&state),
        before,
        "{method} {path} wrote for nobody"
    );
}

#[tokio::test]
async fn listing_libraries_refuses_a_reader() {
    a_reader_is_refused(Method::GET, "/api/library/libraries").await;
}

#[tokio::test]
async fn creating_a_library_refuses_a_reader() {
    a_reader_is_refused(Method::POST, "/api/library/libraries").await;
}

#[tokio::test]
async fn renaming_a_library_refuses_a_reader() {
    a_reader_is_refused(Method::PATCH, "/api/library/libraries/1").await;
}

#[tokio::test]
async fn removing_a_library_refuses_a_reader() {
    a_reader_is_refused(Method::DELETE, "/api/library/libraries/1").await;
}

#[tokio::test]
async fn reading_whether_a_derive_runs_refuses_a_reader() {
    a_reader_is_refused(Method::GET, "/api/library/derive").await;
}

#[tokio::test]
async fn rehearsing_a_removal_refuses_a_reader() {
    a_reader_is_refused(Method::DELETE, "/api/library/libraries/1?dry_run=true").await;
}

#[tokio::test]
async fn adding_a_dir_refuses_a_reader() {
    a_reader_is_refused(Method::POST, "/api/library/libraries/1/dirs").await;
}

#[tokio::test]
async fn moving_a_dir_refuses_a_reader() {
    a_reader_is_refused(Method::POST, "/api/library/roots/1/move").await;
}

#[tokio::test]
async fn rehearsing_a_move_refuses_a_reader() {
    a_reader_is_refused(Method::POST, "/api/library/roots/1/move?dry_run=true").await;
}

#[tokio::test]
async fn listing_libraries_refuses_nobody() {
    an_unsigned_request_is_refused(Method::GET, "/api/library/libraries").await;
}

#[tokio::test]
async fn creating_a_library_refuses_nobody() {
    an_unsigned_request_is_refused(Method::POST, "/api/library/libraries").await;
}

#[tokio::test]
async fn renaming_a_library_refuses_nobody() {
    an_unsigned_request_is_refused(Method::PATCH, "/api/library/libraries/1").await;
}

#[tokio::test]
async fn removing_a_library_refuses_nobody() {
    an_unsigned_request_is_refused(Method::DELETE, "/api/library/libraries/1").await;
}

#[tokio::test]
async fn adding_a_dir_refuses_nobody() {
    an_unsigned_request_is_refused(Method::POST, "/api/library/libraries/1/dirs").await;
}

#[tokio::test]
async fn moving_a_dir_refuses_nobody() {
    an_unsigned_request_is_refused(Method::POST, "/api/library/roots/1/move").await;
}

/// Two libraries with different non-zero counts of each kind, one of them
/// holding two directories, named so that by-name order is not by-id order, and
/// its second directory named so that by-path order is not oldest first.
#[tokio::test]
async fn the_list_gives_each_library_its_own_dirs_works_and_books() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let first = get_json(&router, "/api/library/libraries").await;
    let a_id = the_library(&first, "root-a")["id"].as_i64().expect("an id");
    let b_id = the_library(&first, "root-b")["id"].as_i64().expect("an id");
    {
        let conn = state.library.conn();
        taiyaku_library::libraries::rename(&conn, a_id, "zz-first").expect("renames");
        // A second work under library a, from a directory added to it and scanned.
        let extra = fixture.path().join("aaa-extra");
        test_support::write_pages(&extra.join("Series C").join("Series C - v01"), 3);
        let added = taiyaku_library::scan::add_root_checked_in(
            &conn,
            &extra,
            taiyaku_library::scan::InLibrary::Id(a_id),
        )
        .expect("adds a directory to library a");
        taiyaku_library::scan::scan_root(&conn, &added.root).expect("scans it");
        taiyaku_library::derive::derive(&conn).expect("derives");
        // Two books under a's first directory and two under b's.
        for (library, rels) in [
            (a_id, &["a1.pdf", "a2.pdf"][..]),
            (b_id, &["b1.epub", "b2.pdf"][..]),
        ] {
            for rel in rels {
                let format = if rel.ends_with(".pdf") {
                    "pdf"
                } else {
                    "epub-fixed"
                };
                conn.execute(
                    "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
                     SELECT root_id, ?2, 'file', ?3, 1, 1 FROM library_root
                      WHERE library_id = ?1 ORDER BY root_id LIMIT 1",
                    rusqlite::params![library, rel, format],
                )
                .expect("a book under the library's first directory");
            }
        }
        // And a third under a's second directory, which a count over the first
        // directory alone would miss.
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             SELECT root_id, 'a3.epub', 'file', 'epub-fixed', 1, 1 FROM library_root
              WHERE library_id = ?1 ORDER BY root_id DESC LIMIT 1",
            [a_id],
        )
        .expect("a book under the library's second directory");
        // A cbz entry is not a book.
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             SELECT root_id, 'c.cbz', 'file', 'zip', 1, 1 FROM library_root WHERE library_id = ?1 LIMIT 1",
            [b_id],
        )
        .expect("a cbz under b");
    }

    let listed = get_json(&router, "/api/library/libraries").await;

    assert_eq!(keys_of(&listed), vec!["libraries"], "{listed}");
    let names: Vec<&str> = listed["libraries"]
        .as_array()
        .expect("a libraries array")
        .iter()
        .map(|l| l["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(names, vec!["root-b", "zz-first"], "by name: {listed}");

    let b = the_library(&listed, "root-b");
    assert_eq!(
        keys_of(b),
        vec!["books", "dirs", "id", "name", "works"],
        "{b}"
    );
    assert_eq!(b["id"], b_id);
    assert_eq!(b["works"], 1, "{listed}");
    assert_eq!(b["books"], 2, "{listed}");
    assert_eq!(b["dirs"].as_array().expect("dirs").len(), 1, "{listed}");
    assert!(
        b["dirs"][0]["path"]
            .as_str()
            .expect("a path")
            .ends_with("root-b")
    );

    let a = the_library(&listed, "zz-first");
    assert_eq!(a["id"], a_id);
    assert_eq!(a["works"], 2, "{listed}");
    assert_eq!(a["books"], 3, "{listed}");
    let dirs = a["dirs"].as_array().expect("dirs");
    assert_eq!(dirs.len(), 2, "{listed}");
    assert!(
        dirs[0]["path"]
            .as_str()
            .expect("a path")
            .ends_with("root-a")
    );
    assert!(
        dirs[1]["path"]
            .as_str()
            .expect("a path")
            .ends_with("aaa-extra")
    );
    assert!(
        dirs[0]["id"].as_i64() < dirs[1]["id"].as_i64(),
        "oldest first"
    );
    for dir in dirs {
        assert_eq!(
            keys_of(dir),
            vec![
                "default_kind",
                "default_language",
                "edition_key",
                "exclude_globs",
                "id",
                "inside",
                "keeps_documents",
                "last_scan_at",
                "last_scan_status",
                "libraries",
                "nested",
                "next_scan_at",
                "path",
                "scan_interval_secs",
            ],
            "a directory is root_json's shape, the libraries holding it and nesting_json's \
             two keys: {dir}"
        );
        assert_eq!(
            dir["libraries"],
            serde_json::json!([{ "id": a_id, "name": "zz-first" }]),
            "{dir}"
        );
        // No directory here sits in another: `aaa-extra` and `root-b` lie
        // beside `root-a`, not inside it.
        assert_eq!(dir["nested"], serde_json::json!([]), "{dir}");
        assert_eq!(dir["inside"], serde_json::json!([]), "{dir}");
    }
    assert_eq!(dirs[0]["last_scan_status"], "ok", "{listed}");
    assert!(dirs[0]["last_scan_at"].is_string(), "{listed}");
    assert_eq!(dirs[1]["last_scan_status"], "ok", "{listed}");
    assert_eq!(b["dirs"][0]["last_scan_status"], "ok", "{listed}");
}

#[tokio::test]
async fn a_library_is_created_with_its_dirs_and_then_listed() {
    let (router, _state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-route");
    let dirs = [
        a_dir_of(&fixture, "Volumes", "Alpha"),
        a_dir_of(&fixture, "Chapters", "Beta"),
    ];

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": " zzz Manga ", "dirs": dirs }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(keys_of(&made), vec!["added", "library"], "{made}");
    assert_eq!(made["library"]["name"], "zzz Manga");
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        &made["library"],
        the_library(&listed, "zzz Manga"),
        "the answer is the library as the list shows it"
    );
    let held = made["library"]["dirs"].as_array().expect("dirs");
    assert_eq!(held.len(), 2, "{made}");
    let added = made["added"].as_array().expect("added");
    assert_eq!(added.len(), 2, "{made}");
    for (one, (given, held)) in added.iter().zip(dirs.iter().zip(held)) {
        assert_eq!(
            keys_of(one),
            vec![
                "also_in",
                "interval_secs",
                "root",
                "schedule_kept",
                "watch_reason",
                "watching"
            ],
            "{made}"
        );
        assert_eq!(one["also_in"], serde_json::json!([]), "{made}");
        assert!(one["watching"].is_boolean(), "{made}");
        assert!(one["interval_secs"].is_number(), "{made}");
        assert!(
            one["root"]["path"].as_str().expect("a path").ends_with(
                std::path::Path::new(given)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            ),
            "added is in the order the directories were given: {made}"
        );
        assert_eq!(one["root"]["id"], held["id"], "{made}");
    }
    assert_eq!(
        made["library"]["works"], 0,
        "nothing is scanned by making one"
    );
    assert_eq!(made["library"]["books"], 0);
}

/// Directory `id`'s stored `scan_interval_secs`. The shared connection is
/// released on return: the in-memory library's working connection takes the
/// same lock, so a test holding it across a request would hang.
fn stored_interval(state: &crate::ServeState, id: i64) -> Option<i64> {
    state
        .library
        .conn()
        .query_row(
            "SELECT scan_interval_secs FROM root WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .expect("reads the directory's interval")
}

/// Each directory the scheduler scans, with whether it is due now, read
/// as [`stored_interval`] is.
fn the_schedule(state: &crate::ServeState) -> Vec<(i64, bool)> {
    let conn = state.library.conn();
    let now = taiyaku_library::schedule::now(&conn).expect("reads the store's clock");
    taiyaku_library::schedule::scheduled(&conn, &now)
        .expect("reads the schedule")
        .into_iter()
        .map(|one| (one.root.id, one.due))
        .collect()
}

#[tokio::test]
async fn a_library_made_off_stores_off_in_each_dir_and_leaves_them_out_of_the_schedule() {
    let (router, state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-off");
    let dirs = [
        a_dir_of(&fixture, "Volumes", "Alpha"),
        a_dir_of(&fixture, "Chapters", "Beta"),
    ];

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Off", "dirs": dirs, "scan_interval_secs": 0 }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{made}");
    let added = made["added"].as_array().expect("added");
    assert_eq!(added.len(), 2, "{made}");
    let mut off = Vec::new();
    for one in added {
        assert_eq!(one["root"]["scan_interval_secs"], 0, "{made}");
        assert_eq!(one["schedule_kept"], false, "{made}");
        // An Off add probes too: a probe is not a scan.
        assert!(one["watching"].is_boolean(), "{made}");
        assert!(one["interval_secs"].is_number(), "{made}");
        let id = one["root"]["id"].as_i64().expect("an id");
        assert_eq!(stored_interval(&state, id), Some(0), "{made}");
        off.push(id);
    }
    let schedule = the_schedule(&state);
    for id in &off {
        assert!(
            schedule.iter().all(|(scheduled, _)| scheduled != id),
            "{id} is Off: {schedule:?}"
        );
    }

    // The same make with no field: Automatic, and due, as never scanned.
    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Automatic", "dirs": [a_dir_of(&fixture, "Third", "Gamma")] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let one = &made["added"][0];
    assert!(one["root"]["scan_interval_secs"].is_null(), "{made}");
    assert_eq!(one["schedule_kept"], false, "{made}");
    let id = one["root"]["id"].as_i64().expect("an id");
    assert_eq!(stored_interval(&state, id), None, "{made}");
    assert!(
        the_schedule(&state).contains(&(id, true)),
        "{:?}",
        the_schedule(&state)
    );
}

#[tokio::test]
async fn a_library_is_refused_whole_and_as_the_callers_mistake() {
    let (router, state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-refused");
    let good = a_dir_of(&fixture, "Volumes", "Alpha");
    let before = get_json(&router, "/api/library/libraries").await;
    let stored = the_store(&state);

    for (body, says) in [
        (
            serde_json::json!({ "name": "Manga", "dirs": [good, "/no/such/taiyaku-test-dir"] }),
            "taiyaku-test-dir",
        ),
        (
            serde_json::json!({ "name": "Manga", "dirs": [] }),
            "at least one directory",
        ),
        (
            serde_json::json!({ "name": "  ", "dirs": [good] }),
            "needs a name",
        ),
    ] {
        let (status, refused) =
            answered(&router, Method::POST, "/api/library/libraries", &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {refused}");
        assert!(the_sentence(&refused).contains(says), "{body}: {refused}");
    }
    assert_eq!(get_json(&router, "/api/library/libraries").await, before);
    assert_eq!(the_store(&state), stored, "a refused library leaves no row");

    let taken = before["libraries"][0]["name"].as_str().expect("a name");
    let (status, refused) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": taken, "dirs": [good] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        the_sentence(&refused).contains("already called"),
        "{refused}"
    );

    // The directories are vetted before the name is looked at.
    let (status, refused) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": taken, "dirs": ["/no/such/taiyaku-test-dir"] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        the_sentence(&refused).contains("taiyaku-test-dir"),
        "{refused}"
    );
    assert_eq!(the_store(&state), stored);
}

#[tokio::test]
async fn a_library_that_is_not_asked_for_properly_is_a_400_with_a_sentence() {
    let (router, state, _held) = test_support::router_over_a_single_scanned_work();
    let stored = the_store(&state);
    let id = taiyaku_library::libraries::list(&state.library.conn()).expect("lists")[0].id;

    for (method, path, bodies) in [
        (
            Method::POST,
            "/api/library/libraries".to_string(),
            vec![
                ("not json", "JSON"),
                ("", "JSON"),
                ("{\"name\": \"M\"", "JSON"),
                ("[]", "`name`"),
                ("null", "`name`"),
                ("{\"dirs\": [\"/p\"]}", "`name`"),
                ("{\"name\": 7, \"dirs\": [\"/p\"]}", "`name`"),
                ("{\"name\": \"M\"}", "`dirs`"),
                ("{\"name\": \"M\", \"dirs\": \"/p\"}", "`dirs`"),
                ("{\"name\": \"M\", \"dirs\": [7]}", "`dirs`"),
                ("{\"name\": \"M\", \"dirs\": [\"/p\", null]}", "`dirs`"),
            ],
        ),
        (
            Method::PATCH,
            format!("/api/library/libraries/{id}"),
            vec![
                ("not json", "JSON"),
                ("", "JSON"),
                ("{}", "`name`"),
                ("[]", "`name`"),
                ("{\"name\": 7}", "`name`"),
                ("{\"name\": null}", "`name`"),
            ],
        ),
        (
            Method::POST,
            format!("/api/library/libraries/{id}/dirs"),
            vec![
                ("not json", "JSON"),
                ("", "JSON"),
                ("{}", "`path`"),
                ("[]", "`path`"),
                ("{\"path\": 7}", "`path`"),
                ("{\"path\": null}", "`path`"),
            ],
        ),
    ] {
        for (body, says) in bodies {
            let (status, refused) = answered_raw(&router, method.clone(), &path, body).await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{method} {path} {body:?}: {refused}"
            );
            assert!(
                the_sentence(&refused).contains(says),
                "{method} {path} {body:?}: {refused}"
            );
        }
    }
    assert_eq!(the_store(&state), stored);
}

#[tokio::test]
async fn an_id_that_is_not_a_number_is_a_400_and_one_that_names_nothing_is_a_404() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let extra = a_dir_of(&fixture, "Extra", "Gamma");
    let stored = the_store(&state);

    for (method, suffix, body) in [
        (Method::PATCH, "", serde_json::json!({ "name": "Other" })),
        (Method::DELETE, "", serde_json::json!({})),
        (Method::DELETE, "?dry_run=true", serde_json::json!({})),
        (Method::POST, "/dirs", serde_json::json!({ "path": extra })),
    ] {
        for id in ["abc", "1.5", "99999999999999999999", "-"] {
            let path = format!("/api/library/libraries/{id}{suffix}");
            let (status, refused) = answered(&router, method.clone(), &path, &body).await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{method} {path}: {refused}"
            );
            assert!(
                the_sentence(&refused).contains(id),
                "{method} {path}: {refused}"
            );
        }
        for id in ["4242", "0", "-3"] {
            let path = format!("/api/library/libraries/{id}{suffix}");
            let (status, missing) = answered(&router, method.clone(), &path, &body).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}: {missing}");
            assert_eq!(
                the_sentence(&missing),
                format!("there is no library {id}"),
                "{method} {path}"
            );
        }
    }
    assert_eq!(the_store(&state), stored);
}

#[tokio::test]
async fn a_library_is_renamed_by_its_id() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/libraries/{id}");

    let (status, renamed) = answered(
        &router,
        Method::PATCH,
        &path,
        &serde_json::json!({ "name": " Manga " }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed, serde_json::json!({ "id": id, "name": "Manga" }));
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(the_library(&after, "Manga")["id"], id, "{after}");
    assert_eq!(
        the_library(&after, "root-b"),
        the_library(&listed, "root-b"),
        "the other library is untouched"
    );

    let stored = the_store(&state);
    let (status, refused) = answered(
        &router,
        Method::PATCH,
        &path,
        &serde_json::json!({ "name": "root-b" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        the_sentence(&refused).contains("already called `root-b`"),
        "{refused}"
    );

    let (status, refused) = answered(
        &router,
        Method::PATCH,
        &path,
        &serde_json::json!({ "name": "  " }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(the_sentence(&refused).contains("needs a name"), "{refused}");
    assert_eq!(the_store(&state), stored);
}

#[tokio::test]
async fn a_dir_is_added_to_a_library_by_the_librarys_id() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-dir-route");
    let extra = a_dir_of(&fixture, "Extra", "Gamma");
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/libraries/{id}/dirs");

    let (status, added) = answered(
        &router,
        Method::POST,
        &path,
        &serde_json::json!({ "path": extra }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(
        keys_of(&added),
        vec![
            "also_in",
            "interval_secs",
            "root",
            "schedule_kept",
            "watch_reason",
            "watching"
        ],
        "{added}"
    );
    assert_eq!(added["also_in"], serde_json::json!([]), "{added}");
    assert!(added["watching"].is_boolean(), "{added}");
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-b");
    assert_eq!(a["dirs"].as_array().expect("dirs").len(), 2, "{listed}");
    assert_eq!(a["dirs"][1]["id"], added["root"]["id"], "{listed}");
    // Each directory carries its own scan columns: the old one its scan, the new
    // one none. The new directory's id is not its library's, so the columns cannot
    // be found by the library's id and look right.
    assert_ne!(added["root"]["id"], a["id"], "{listed}");
    assert_eq!(a["dirs"][0]["last_scan_status"], "ok", "{listed}");
    assert!(a["dirs"][0]["last_scan_at"].is_string(), "{listed}");
    assert!(a["dirs"][1]["last_scan_status"].is_null(), "{listed}");
    assert!(a["dirs"][1]["last_scan_at"].is_null(), "{listed}");
    assert_eq!(
        the_library(&listed, "root-a")["dirs"]
            .as_array()
            .expect("dirs")
            .len(),
        1,
        "the directory joined the library it was sent to"
    );
    assert_eq!(listed["libraries"].as_array().expect("libraries").len(), 2);

    let stored = the_store(&state);
    let held = the_library(&listed, "root-a")["dirs"][0]["path"]
        .as_str()
        .expect("root-a's directory")
        .to_owned();
    let inside = test_support::a_path_inside_the_first_root(&state);
    let canonical = |p: &str| std::path::Path::new(p).canonicalize().expect("exists");
    let into_a = format!(
        "/api/library/libraries/{}/dirs",
        the_library(&listed, "root-a")["id"]
            .as_i64()
            .expect("an id")
    );
    for (into, body, says) in [
        (
            &into_a,
            serde_json::json!({ "path": inside }),
            format!(
                "{} is inside {held}, which is already a directory of the same library, `root-a`",
                canonical(&inside).display()
            ),
        ),
        (
            &into_a,
            serde_json::json!({ "path": held }),
            format!(
                "{} is already a directory of the library `root-a`",
                canonical(&held).display()
            ),
        ),
    ] {
        let (got, refused) = answered(&router, Method::POST, into, &body).await;
        assert_eq!(got, StatusCode::BAD_REQUEST, "{into} {body}: {refused}");
        assert_eq!(the_sentence(&refused), says, "{into} {body}");
    }
    let (got, refused) = answered(
        &router,
        Method::POST,
        &path,
        &serde_json::json!({ "path": "/no/such/taiyaku-test-dir" }),
    )
    .await;
    assert_eq!(got, StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        the_sentence(&refused).contains("taiyaku-test-dir"),
        "{refused}"
    );
    assert_eq!(the_store(&state), stored);

    // Overlapping-libraries spec §4: root-a's folder joins root-b's library.
    let (got, nested) = answered(
        &router,
        Method::POST,
        &path,
        &serde_json::json!({ "path": inside }),
    )
    .await;
    assert_eq!(got, StatusCode::OK, "{nested}");
    assert_eq!(
        nested["root"]["path"],
        canonical(&inside).display().to_string(),
        "{nested}"
    );

    // Overlapping-libraries spec §5: root-b's own directory is linked into
    // root-a. Nothing is probed, and the answer is the scanned row as it stands.
    let b_dir = the_library(&listed, "root-b")["dirs"][0]["path"].clone();
    let (got, linked) = answered(
        &router,
        Method::POST,
        &into_a,
        &serde_json::json!({ "path": b_dir }),
    )
    .await;
    assert_eq!(got, StatusCode::OK, "{linked}");
    assert_eq!(linked["also_in"], serde_json::json!(["root-b"]), "{linked}");
    for unprobed in ["watching", "watch_reason", "interval_secs"] {
        assert!(linked[unprobed].is_null(), "{unprobed}: {linked}");
    }
    let listed = get_json(&router, "/api/library/libraries").await;
    let mut row = the_library(&listed, "root-b")["dirs"][0].clone();
    for listed_only in ["libraries", "nested", "inside"] {
        row.as_object_mut().expect("an object").remove(listed_only);
    }
    assert_eq!(
        linked["root"], row,
        "the answer is the row as listed: {listed}"
    );
    assert_eq!(linked["root"]["last_scan_status"], "ok", "{linked}");
}

#[tokio::test]
async fn a_dir_added_off_stores_off_and_is_left_out_of_the_schedule() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-dir-off");
    let listed = get_json(&router, "/api/library/libraries").await;
    let into = format!(
        "/api/library/libraries/{}/dirs",
        the_library(&listed, "root-b")["id"]
            .as_i64()
            .expect("an id")
    );

    let (status, added) = answered(
        &router,
        Method::POST,
        &into,
        &serde_json::json!({ "path": a_dir_of(&fixture, "Extra", "Gamma"), "scan_interval_secs": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["root"]["scan_interval_secs"], 0, "{added}");
    assert_eq!(added["schedule_kept"], false, "{added}");
    // An Off add probes too: a probe is not a scan.
    assert!(added["watching"].is_boolean(), "{added}");
    assert!(added["interval_secs"].is_number(), "{added}");
    let id = added["root"]["id"].as_i64().expect("an id");
    assert_eq!(stored_interval(&state, id), Some(0), "{added}");
    let schedule = the_schedule(&state);
    assert!(
        schedule.iter().all(|(scheduled, _)| *scheduled != id),
        "{id} is Off: {schedule:?}"
    );

    // The same add with no field: Automatic, and due, as never scanned.
    let (status, added) = answered(
        &router,
        Method::POST,
        &into,
        &serde_json::json!({ "path": a_dir_of(&fixture, "Other", "Delta") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert!(added["root"]["scan_interval_secs"].is_null(), "{added}");
    assert_eq!(added["schedule_kept"], false, "{added}");
    let id = added["root"]["id"].as_i64().expect("an id");
    assert_eq!(stored_interval(&state, id), None, "{added}");
    assert!(
        the_schedule(&state).contains(&(id, true)),
        "{:?}",
        the_schedule(&state)
    );
}

#[tokio::test]
async fn both_adds_refuse_an_interval_the_patch_refuses_in_its_words() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-interval-refused");
    let dir = a_dir_of(&fixture, "Extra", "Gamma");
    let listed = get_json(&router, "/api/library/libraries").await;
    let into = format!(
        "/api/library/libraries/{}/dirs",
        the_library(&listed, "root-b")["id"]
            .as_i64()
            .expect("an id")
    );
    let stored = the_store(&state);
    let range = "the directory's `scan_interval_secs` must be null (automatic), 0 (off) or at \
                 least 300 (5 minutes)";
    let not_a_number = "the directory's `scan_interval_secs` must be a number or null";

    for (asked, says) in [
        (serde_json::json!(299), range),
        (serde_json::json!(-1), range),
        (serde_json::json!("0"), not_a_number),
        (serde_json::json!(0.5), not_a_number),
    ] {
        for (route, body) in [
            (
                "/api/library/libraries",
                serde_json::json!({ "name": "Manga", "dirs": [dir], "scan_interval_secs": asked }),
            ),
            (
                into.as_str(),
                serde_json::json!({ "path": dir, "scan_interval_secs": asked }),
            ),
        ] {
            let (status, refused) = answered(&router, Method::POST, route, &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{route} {body}: {refused}");
            assert_eq!(
                refused,
                serde_json::json!({ "error": says }),
                "{route} {body}"
            );
            assert_eq!(the_store(&state), stored, "{route} {body} added nothing");
        }
    }
}

#[tokio::test]
async fn both_adds_store_automatic_and_five_minutes() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-interval-stored");
    let listed = get_json(&router, "/api/library/libraries").await;
    let into = format!(
        "/api/library/libraries/{}/dirs",
        the_library(&listed, "root-b")["id"]
            .as_i64()
            .expect("an id")
    );

    for (name, asked, stores) in [
        ("Automatic", serde_json::Value::Null, None),
        ("Five", serde_json::json!(300), Some(300)),
    ] {
        let (status, made) = answered(
            &router,
            Method::POST,
            "/api/library/libraries",
            &serde_json::json!({
                "name": name,
                "dirs": [a_dir_of(&fixture, &format!("{name} made"), "Alpha")],
                "scan_interval_secs": asked,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}: {made}");
        let made = &made["added"][0];
        assert_eq!(made["root"]["scan_interval_secs"], asked, "{name}: {made}");
        let id = made["root"]["id"].as_i64().expect("an id");
        assert_eq!(stored_interval(&state, id), stores, "{name}: {made}");

        let (status, added) = answered(
            &router,
            Method::POST,
            &into,
            &serde_json::json!({
                "path": a_dir_of(&fixture, &format!("{name} added"), "Beta"),
                "scan_interval_secs": asked,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}: {added}");
        assert_eq!(
            added["root"]["scan_interval_secs"], asked,
            "{name}: {added}"
        );
        let id = added["root"]["id"].as_i64().expect("an id");
        assert_eq!(stored_interval(&state, id), stores, "{name}: {added}");
    }
}

#[tokio::test]
async fn a_link_keeps_its_shared_schedule_and_says_so_when_the_add_asked_for_another() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("link-keeps-schedule");
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let b = the_library(&listed, "root-b");
    let a_dir = a["dirs"][0]["path"].as_str().expect("a path").to_owned();
    let a_id = a["dirs"][0]["id"].as_i64().expect("an id");
    let b_dir = b["dirs"][0]["path"].as_str().expect("a path").to_owned();
    let b_id = b["dirs"][0]["id"].as_i64().expect("an id");
    assert_eq!(
        stored_interval(&state, a_id),
        None,
        "the fixture's is Automatic"
    );
    assert_eq!(
        stored_interval(&state, b_id),
        None,
        "the fixture's is Automatic"
    );

    let (status, linked) = answered(
        &router,
        Method::POST,
        &format!(
            "/api/library/libraries/{}/dirs",
            b["id"].as_i64().expect("an id")
        ),
        &serde_json::json!({ "path": a_dir, "scan_interval_secs": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{linked}");
    assert_eq!(linked["schedule_kept"], true, "{linked}");
    assert!(linked["root"]["scan_interval_secs"].is_null(), "{linked}");
    assert_eq!(linked["also_in"], serde_json::json!(["root-a"]), "{linked}");
    assert_eq!(stored_interval(&state, a_id), None, "root-a's scans go on");

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({
            "name": "Mixed",
            "dirs": [a_dir_of(&fixture, "New", "Gamma"), a_dir],
            "scan_interval_secs": 0,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let new = &made["added"][0];
    assert_eq!(new["schedule_kept"], false, "{made}");
    let new_id = new["root"]["id"].as_i64().expect("an id");
    assert_eq!(stored_interval(&state, new_id), Some(0), "{made}");
    let held = &made["added"][1];
    assert_eq!(held["root"]["id"], a_id, "{made}");
    assert_eq!(held["schedule_kept"], true, "{made}");
    assert!(held["root"]["scan_interval_secs"].is_null(), "{made}");
    assert_eq!(stored_interval(&state, a_id), None, "{made}");

    // Automatic asked, Automatic kept: nothing to say.
    let (status, linked) = answered(
        &router,
        Method::POST,
        &format!(
            "/api/library/libraries/{}/dirs",
            made["library"]["id"].as_i64().expect("an id")
        ),
        &serde_json::json!({ "path": b_dir }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{linked}");
    assert_eq!(linked["also_in"], serde_json::json!(["root-b"]), "{linked}");
    assert_eq!(linked["schedule_kept"], false, "{linked}");
    assert_eq!(stored_interval(&state, b_id), None, "{linked}");
}

/// An add with no field asks for Automatic, so a link of a
/// directory another library set Off keeps Off and says so, or the person
/// waits for a scan that never comes.
#[tokio::test]
async fn a_link_added_with_no_schedule_keeps_an_off_shared_one_and_says_so() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let a_dir = a["dirs"][0]["path"].as_str().expect("a path").to_owned();
    let a_id = a["dirs"][0]["id"].as_i64().expect("an id");
    let into_b = format!(
        "/api/library/libraries/{}/dirs",
        the_library(&listed, "root-b")["id"]
            .as_i64()
            .expect("an id")
    );
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{a_id}"),
        &serde_json::json!({ "scan_interval_secs": 0 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (status, linked) = answered(
        &router,
        Method::POST,
        &into_b,
        &serde_json::json!({ "path": a_dir }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{linked}");
    assert_eq!(linked["also_in"], serde_json::json!(["root-a"]), "{linked}");
    assert_eq!(linked["schedule_kept"], true, "{linked}");
    assert_eq!(linked["root"]["scan_interval_secs"], 0, "{linked}");
    assert_eq!(stored_interval(&state, a_id), Some(0), "{linked}");
}

#[tokio::test]
async fn an_off_dir_is_scanned_by_scan_library_and_stays_off() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({
            "name": "Off",
            "dirs": [a_dir_of(&fixture, "Volumes", "Alpha")],
            "scan_interval_secs": 0,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let id = made["library"]["id"].as_i64().expect("an id");
    let dir = made["added"][0]["root"]["id"].as_i64().expect("an id");
    assert_eq!(stored_interval(&state, dir), Some(0), "{made}");

    let (status, started) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    until_the_library_scan_ends(&router, id).await;

    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        the_library(&listed, "Off")["dirs"][0]["last_scan_status"],
        "ok",
        "{listed}"
    );
    assert_eq!(
        stored_interval(&state, dir),
        Some(0),
        "a press does not turn it on"
    );
    let schedule = the_schedule(&state);
    assert!(
        schedule.iter().all(|(scheduled, _)| *scheduled != dir),
        "{dir} is Off: {schedule:?}"
    );
}

#[tokio::test]
async fn a_librarys_dry_run_counts_and_removes_nothing_and_the_removal_removes() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let b = the_library(&listed, "root-b");
    let id = b["id"].as_i64().expect("an id");
    let path = format!("/api/library/libraries/{id}");
    // A second directory, so the library's removal takes two registry slots.
    let extra = a_dir_of(&fixture, "Extra", "Gamma");
    let (status, added) = answered(
        &router,
        Method::POST,
        &format!("{path}/dirs"),
        &serde_json::json!({ "path": extra }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let listed = get_json(&router, "/api/library/libraries").await;
    let dirs: Vec<i64> = the_library(&listed, "root-b")["dirs"]
        .as_array()
        .expect("dirs")
        .iter()
        .map(|f| f["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(dirs.len(), 2);
    let other = the_library(&listed, "root-a")["dirs"][0]["id"]
        .as_i64()
        .expect("an id");
    for slot in dirs.iter().copied().chain([other]) {
        state.scans.lock().unwrap().insert(
            slot,
            crate::ScanProgress {
                waiting: false,
                started: std::time::Instant::now(),
                done: true,
                report: Some(serde_json::json!({})),
                library: None,
            },
        );
    }
    let stored = the_store(&state);

    let (status, rehearsed) = answered(
        &router,
        Method::DELETE,
        &format!("{path}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(
        keys_of(&rehearsed),
        vec!["derive", "dry_run", "removed"],
        "{rehearsed}"
    );
    assert_eq!(
        keys_of(&rehearsed["removed"]),
        vec![
            "kept",
            "left_with_no_library",
            "libraries_removed",
            "notes",
            "notes_page_gone",
            "positions",
            "positions_page_gone",
            "works"
        ]
    );
    assert_eq!(
        rehearsed["removed"]["libraries_removed"],
        serde_json::json!(["root-b"])
    );
    assert_eq!(rehearsed["removed"]["kept"], serde_json::json!([]));
    assert_eq!(rehearsed["dry_run"], true);
    assert_eq!(rehearsed["removed"]["works"], 1, "{rehearsed}");
    assert!(rehearsed["derive"].is_object(), "{rehearsed}");
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
    assert_eq!(the_store(&state), stored, "a rehearsal writes nothing");
    assert_eq!(state.scans.lock().unwrap().len(), 3, "and clears no slot");

    let (status, misspelt) = answered(
        &router,
        Method::DELETE,
        &format!("{path}?dryrun=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{misspelt}");
    assert!(the_sentence(&misspelt).contains("query"), "{misspelt}");
    let (status, misspelt) = answered(
        &router,
        Method::DELETE,
        &format!("{path}?dry_run=maybe"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{misspelt}");
    assert!(the_sentence(&misspelt).contains("query"), "{misspelt}");
    assert_eq!(
        the_store(&state),
        stored,
        "a misspelt rehearsal removes nothing"
    );

    let (status, removed) = answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(
        keys_of(&removed),
        vec!["derive", "dry_run", "removed"],
        "{removed}"
    );
    assert_eq!(removed["dry_run"], false);
    assert_eq!(
        removed["removed"], rehearsed["removed"],
        "the rehearsal said what happened"
    );
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        after["libraries"].as_array().expect("libraries").len(),
        1,
        "{after}"
    );
    assert_eq!(
        &after["libraries"][0],
        the_library(&listed, "root-a"),
        "the other is whole"
    );
    {
        let scans = state.scans.lock().unwrap();
        for dir in &dirs {
            assert!(
                !scans.contains_key(dir),
                "a removed directory's id can be given to the next one added"
            );
        }
        assert!(scans.contains_key(&other), "another library's slot stays");
    }

    let (status, missing) = answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
}

/// Removing the library that is a reader's only grant goes ahead: the dry run
/// names the reader under `left_with_no_library` and removes nothing, the
/// removal answers the same, and the reader is then listed nothing. The
/// other library still holds a series, so a reader widened to every library
/// would be listed one.
#[tokio::test]
async fn removing_an_accounts_only_library_names_the_reader_who_then_sees_nothing() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let reader = {
        let conn = state.library.conn();
        let reader = taiyaku_library::account::add(&conn, "reader", "pw", "user").expect("adds");
        taiyaku_library::account::grant_library(&conn, reader, id).expect("grants");
        reader
    };
    assert_eq!(
        test_support::seen_by(&router, &state, reader).await,
        1,
        "the fixture: the reader sees root-b's series"
    );
    let stored = the_store(&state);
    let path = format!("/api/library/libraries/{id}");

    let (status, rehearsed) = answered(
        &router,
        Method::DELETE,
        &format!("{path}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(
        rehearsed["removed"]["left_with_no_library"],
        serde_json::json!(["reader"]),
        "{rehearsed}"
    );
    assert_eq!(the_store(&state), stored, "a rehearsal removes nothing");

    let (status, removed) = answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(
        removed["removed"], rehearsed["removed"],
        "the removal does what the rehearsal said"
    );
    assert_eq!(
        test_support::seen_by(&router, &state, reader).await,
        0,
        "the reader sees no library, not every library"
    );
}

/// A directory is added to a library, or made with one. Nothing adds one alone.
/// Every other route under `/api/library/roots` answers.
#[tokio::test]
async fn the_roots_route_no_longer_adds_a_dir_and_its_neighbours_answer() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let stored = the_store(&state);
    let response = send_json(
        &router,
        Method::POST,
        "/api/library/roots",
        &serde_json::json!({ "path": "/srv" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(the_store(&state), stored);

    let root = taiyaku_library::scan::roots(&state.library.conn()).expect("roots")[0].id;
    for (method, path, status) in [
        (
            Method::GET,
            "/api/library/roots".to_string(),
            StatusCode::OK,
        ),
        (
            Method::PATCH,
            format!("/api/library/roots/{root}"),
            StatusCode::NO_CONTENT,
        ),
        (
            Method::DELETE,
            format!("/api/library/roots/{root}?dry_run=true"),
            StatusCode::OK,
        ),
        (
            Method::GET,
            format!("/api/library/roots/{root}/scan"),
            StatusCode::OK,
        ),
    ] {
        let response = send_json(&router, method.clone(), &path, &serde_json::json!({})).await;
        assert_eq!(response.status(), status, "{method} {path}");
    }
}

/// `PRAGMA query_only` makes the connection itself refuse a write, standing in
/// for the fault a full disk or a busy database would produce. The handlers
/// are called directly, because the same pragma refuses the session slide the
/// guard writes on every request.
#[tokio::test]
async fn a_store_fault_adding_a_dir_is_a_500_not_a_400() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("add-dir-store-fault");
    std::fs::create_dir_all(fixture.path().join("New Folder")).expect("mkdir");
    let id = {
        let conn = state.library.conn();
        conn.pragma_update(None, "query_only", true)
            .expect("sets query_only");
        taiyaku_library::libraries::list(&conn).expect("lists")[0].id
    };
    let body = serde_json::json!({ "path": fixture.path().to_string_lossy() });

    let response = crate::libraries_api::add_dir(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(id.to_string()),
        axum::body::Bytes::from(body.to_string()),
    )
    .await;
    let (status, text) = parsed(response).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "a store fault must not be answered as the caller's own mistake: {text}"
    );
    assert!(!the_sentence(&text).is_empty());
}

#[tokio::test]
async fn a_store_fault_creating_a_library_is_a_500_not_a_400() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-store-fault");
    let dir = a_dir_of(&fixture, "Volumes", "Alpha");
    {
        let conn = state.library.conn();
        conn.pragma_update(None, "query_only", true)
            .expect("sets query_only");
    }
    let body = serde_json::json!({ "name": "Manga", "dirs": [dir] });

    let response = crate::libraries_api::create_library(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::body::Bytes::from(body.to_string()),
    )
    .await;
    let (status, text) = parsed(response).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{text}");
}

#[tokio::test]
async fn a_librarys_scan_status_refuses_a_reader() {
    a_reader_is_refused(Method::GET, "/api/library/libraries/1/scan").await;
}

#[tokio::test]
async fn starting_a_librarys_scan_refuses_a_reader() {
    a_reader_is_refused(Method::POST, "/api/library/libraries/1/scan").await;
}

#[tokio::test]
async fn a_librarys_scan_status_refuses_nobody() {
    an_unsigned_request_is_refused(Method::GET, "/api/library/libraries/1/scan").await;
}

#[tokio::test]
async fn starting_a_librarys_scan_refuses_nobody() {
    an_unsigned_request_is_refused(Method::POST, "/api/library/libraries/1/scan").await;
}

/// The fixture's one library, given a second directory holding another series
/// that no scan has walked. Answers the library's id and its two directory ids,
/// oldest first.
///
/// **The second is `z-Second`**, after the first's `tree` by path, which is
/// the order a library's scan walks and lists its directories in, so "first"
/// and "second" mean the same in both orders. Neither path is a prefix of the
/// other's, so a sentence naming one does not contain the other.
async fn a_library_with_an_unscanned_second_dir(
    router: &axum::Router,
    fixture: &test_support::Fixture,
) -> (i64, [i64; 2]) {
    let listed = get_json(router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let (status, added) = answered(
        router,
        Method::POST,
        &format!("/api/library/libraries/{id}/dirs"),
        &serde_json::json!({ "path": a_dir_of(fixture, "z-Second", "Second Series") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let listed = get_json(router, "/api/library/libraries").await;
    let dirs = listed["libraries"][0]["dirs"].as_array().expect("dirs");
    assert_eq!(dirs.len(), 2, "{listed}");
    (
        id,
        [
            dirs[0]["id"].as_i64().expect("an id"),
            dirs[1]["id"].as_i64().expect("an id"),
        ],
    )
}

/// Polls the library's status route until it says nothing is running, and
/// fails the test if the scan outlasts the ceiling.
async fn until_the_library_scan_ends(router: &axum::Router, id: i64) -> serde_json::Value {
    for _ in 0..400 {
        let status = get_json(router, &format!("/api/library/libraries/{id}/scan")).await;
        if status["running"] == false {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the library's scan did not end within ten seconds");
}

fn a_scan_running_on(state: &crate::ServeState, dir: i64) {
    state.scans.lock().unwrap().insert(
        dir,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now(),
            done: false,
            report: None,
            library: None,
        },
    );
}

fn the_scan_of(library: i64) -> String {
    format!("/api/library/libraries/{library}/scan")
}

fn a_dirs_scan(dir: i64) -> String {
    format!("/api/library/roots/{dir}/scan")
}

fn scan_entries_in(state: &crate::ServeState, dir: i64) -> i64 {
    state
        .library
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM scan_entry WHERE root_id = ?1",
            [dir],
            |row| row.get(0),
        )
        .expect("counts")
}

/// The first directory's scan entries are deleted first, so that a scan which
/// does not walk it leaves it empty and the library one work short. The
/// second directory's volume has not been walked at all. Each directory's report
/// carries the derive's counts: had the derive run after the first walk, it
/// would count one work, the second directory's volume not yet being walked.
#[tokio::test]
async fn scan_now_walks_each_dir_and_derives_once() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    // The second directory holds one more volume than the first has to walk, so
    // that a walk filed under the other directory shows in its counts.
    test_support::write_pages(
        &fixture
            .path()
            .join("z-Second")
            .join("Second Series")
            .join("Second Series - v02"),
        2,
    );
    let paths: Vec<String> =
        get_json(&router, "/api/library/libraries").await["libraries"][0]["dirs"]
            .as_array()
            .expect("dirs")
            .iter()
            .map(|dir| dir["path"].as_str().expect("a path").to_string())
            .collect();
    let mut added = Vec::new();
    state
        .library
        .conn()
        .execute("DELETE FROM scan_entry WHERE root_id = ?1", [dirs[0]])
        .expect("empties the first directory's entries");
    assert_eq!(scan_entries_in(&state, dirs[1]), 0);

    let (status, started) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(
        started,
        serde_json::json!({ "library_id": id, "status_url": the_scan_of(id), "root_ids": dirs })
    );
    let ended = until_the_library_scan_ends(&router, id).await;
    assert_eq!(
        ended,
        serde_json::json!({
            "library_id": id,
            "running": false,
            "elapsed_secs": null,
            "dirs": [
                { "root_id": dirs[0], "running": false },
                { "root_id": dirs[1], "running": false },
            ],
            "held_removals": [],
            "moves_held": false,
            "derive_running": false,
            "scans": [],
        })
    );

    for (dir, path) in dirs.into_iter().zip(&paths) {
        assert!(scan_entries_in(&state, dir) > 0, "directory {dir} walked");
        let status = get_json(&router, &a_dirs_scan(dir)).await;
        assert_eq!(status["running"], false, "{status}");
        assert_eq!(
            status["report"]["scan"]["root"], *path,
            "directory {dir} holds its own walk: {status}"
        );
        added.push(status["report"]["scan"]["added"].as_u64().expect("a walk"));
        assert_eq!(
            keys_of(&status["report"]),
            vec!["derive", "scan"],
            "{status}"
        );
        assert!(
            status["report"]["scan"]["added"].as_u64().expect("a walk") > 0,
            "directory {dir} was walked: {status}"
        );
        assert_eq!(
            status["report"]["derive"]["works"], 3,
            "directory {dir} carries the one derive, run after both walks: {status}"
        );
        assert_eq!(status["last_scan_status"], "ok", "{status}");
    }
    assert!(
        added[1] > added[0],
        "the second directory's walk found more than the first's: {added:?}"
    );
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(listed["libraries"][0]["works"], 3, "{listed}");
}

/// While the library's scan is waiting on the job lock, it is running: its
/// status says so for each directory, each directory's own routes answer as for a
/// scan of that directory, and a second scan of the library is refused.
#[tokio::test]
async fn while_a_librarys_scan_runs_its_dirs_answer_as_scanning() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;

    let held = holding_the_job(&state);
    let (status, started) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");

    let running = get_json(&router, &the_scan_of(id)).await;
    assert_eq!(
        keys_of(&running),
        vec![
            "derive_running",
            "dirs",
            "elapsed_secs",
            "held_removals",
            "library_id",
            "moves_held",
            "running",
            "scans"
        ]
    );
    // Its own directories' scans are each one's own status's to say.
    assert_eq!(running["held_removals"], serde_json::json!([]), "{running}");
    // One scan, which this library asked for, of both its directories.
    let name = get_json(&router, "/api/library/libraries").await["libraries"][0]["name"].clone();
    assert_eq!(
        running["scans"],
        serde_json::json!([{ "library_id": id, "library": name, "root_ids": dirs }]),
        "{running}"
    );
    assert_eq!(running["running"], true, "{running}");
    assert!(running["elapsed_secs"].is_number(), "{running}");
    assert_eq!(
        running["dirs"],
        serde_json::json!([
            { "root_id": dirs[0], "running": true },
            { "root_id": dirs[1], "running": true },
        ])
    );
    for dir in dirs {
        let own = get_json(&router, &a_dirs_scan(dir)).await;
        assert_eq!(own["running"], true, "{own}");
        assert!(own["elapsed_secs"].is_number(), "{own}");
        assert!(own["report"].is_null(), "{own}");
        let (status, refused) = answered(
            &router,
            Method::POST,
            &a_dirs_scan(dir),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "directory {dir}: {refused}");
    }
    let (status, refused) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    let sentence = the_sentence(&refused);
    assert!(sentence.contains("already running"), "{sentence}");

    drop(held);
    let ended = until_the_library_scan_ends(&router, id).await;
    assert!(ended["elapsed_secs"].is_null(), "{ended}");
    for dir in dirs {
        let own = get_json(&router, &a_dirs_scan(dir)).await;
        assert_eq!(own["running"], false, "{own}");
        assert!(own["report"]["scan"].is_object(), "{own}");
    }
    // The slots are free again: the library can be scanned a second time.
    let (status, _) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    until_the_library_scan_ends(&router, id).await;
}

/// While a scan of one of its directories is reserved, a library is neither
/// rehearsed nor removed: 409 naming that directory, and the store as it was.
#[tokio::test]
async fn a_library_is_not_removed_while_a_scan_of_one_of_its_dirs_is_reserved() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    a_scan_running_on(&state, dirs[1]);
    let listed = get_json(&router, "/api/library/libraries").await;
    let path = listed["libraries"][0]["dirs"][1]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    let stored = the_store(&state);

    for query in ["?dry_run=true", ""] {
        let (status, refused) = answered(
            &router,
            Method::DELETE,
            &format!("/api/library/libraries/{id}{query}"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{query}: {refused}");
        assert_eq!(
            the_sentence(&refused),
            format!("a scan of {path} is running; remove the library once it has finished")
        );
        assert_eq!(the_store(&state), stored, "{query} removed something");
    }
}

/// A scan of one directory already running refuses the library's, names that
/// directory and reserves nothing for the other.
#[tokio::test]
async fn a_scan_of_one_dir_refuses_the_librarys_and_reserves_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    a_scan_running_on(&state, dirs[1]);

    let (status, refused) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(
        the_sentence(&refused).contains("Second"),
        "names the directory: {refused}"
    );
    assert!(!state.scans.lock().unwrap().contains_key(&dirs[0]));
    let running = get_json(&router, &the_scan_of(id)).await;
    assert_eq!(running["running"], true, "{running}");
    assert!(running["elapsed_secs"].is_number(), "{running}");
    assert_eq!(
        running["dirs"],
        serde_json::json!([
            { "root_id": dirs[0], "running": false },
            { "root_id": dirs[1], "running": true },
        ])
    );
}

/// A scan of a directory of another library does not touch this library's, and
/// the library's scan leaves another library's directory free.
#[tokio::test]
async fn a_scan_in_another_library_neither_blocks_nor_is_blocked() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({
            "name": "Elsewhere",
            "dirs": [a_dir_of(&fixture, "Elsewhere", "Else Series")],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let other = made["library"]["id"].as_i64().expect("an id");
    let other_dir = made["library"]["dirs"][0]["id"].as_i64().expect("an id");

    // Another library's directory is being scanned: this library's scan starts.
    a_scan_running_on(&state, other_dir);
    let (status, started) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    until_the_library_scan_ends(&router, id).await;
    let unaffected = get_json(&router, &the_scan_of(other)).await;
    assert_eq!(unaffected["running"], true, "{unaffected}");

    // This library's scan is held running: another library's directory scans.
    state.scans.lock().unwrap().remove(&other_dir);
    let held = holding_the_job(&state);
    let (status, _) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (status, started) = answered(
        &router,
        Method::POST,
        &a_dirs_scan(other_dir),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let other_status = get_json(&router, &the_scan_of(other)).await;
    assert_eq!(other_status["running"], true, "{other_status}");
    assert_eq!(
        get_json(&router, &the_scan_of(id)).await["dirs"],
        serde_json::json!([
            { "root_id": dirs[0], "running": true },
            { "root_id": dirs[1], "running": true },
        ])
    );
    drop(held);
    until_the_library_scan_ends(&router, id).await;
    until_the_library_scan_ends(&router, other).await;
}

/// The first directory is gone from the disk: the walk fails, the scan ends there,
/// so the second directory is never walked and nothing is derived, and each
/// directory's entry ends done with the one error.
#[tokio::test]
async fn a_failed_walk_ends_the_scan_and_every_dir_reports_the_error() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let gone = listed["libraries"][0]["dirs"][0]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    std::fs::remove_dir_all(&gone).expect("removes the first directory");

    let (status, _) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let ended = until_the_library_scan_ends(&router, id).await;

    assert_eq!(ended["dirs"].as_array().expect("dirs").len(), 2);
    for dir in dirs {
        let own = get_json(&router, &a_dirs_scan(dir)).await;
        assert_eq!(own["running"], false, "{own}");
        assert_eq!(keys_of(&own["report"]), vec!["error"], "{own}");
        let error = own["report"]["error"].as_str().expect("a sentence");
        assert!(
            error.contains(&gone),
            "names the directory that failed: {error}"
        );
    }
    assert_eq!(scan_entries_in(&state, dirs[1]), 0, "never walked");
    let never = get_json(&router, &a_dirs_scan(dirs[1])).await;
    assert!(
        never["last_scan_status"].is_null(),
        "a directory the scan never reached keeps no status: {never}"
    );
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        listed["libraries"][0]["works"], 1,
        "nothing derived: {listed}"
    );
}

/// The second directory fails after the first was walked, and the first holds
/// new material: the first's `last_scan_status` no longer reads `ok`, since
/// what its walk found was not derived, and nothing is derived: no work or
/// series appears for the first's new volume. The error names the directory
/// that failed.
#[tokio::test]
async fn a_failed_second_walk_corrects_the_first_dirs_status_and_derives_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let first_path = listed["libraries"][0]["dirs"][0]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    let gone = listed["libraries"][0]["dirs"][1]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    test_support::write_pages(
        &std::path::Path::new(&first_path)
            .join("First Extra Series")
            .join("First Extra Series - v01"),
        2,
    );
    std::fs::remove_dir_all(&gone).expect("removes the second directory");
    let counts = |state: &crate::ServeState| -> (i64, i64) {
        let conn = state.library.conn();
        let count = |table: &str| -> i64 {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("counts")
        };
        (count("work"), count("series"))
    };
    let before = counts(&state);

    let (status, _) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    until_the_library_scan_ends(&router, id).await;

    let walked: i64 = state
        .library
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM scan_entry WHERE root_id = ?1 AND rel_path LIKE 'First Extra Series%'",
            [dirs[0]],
            |row| row.get(0),
        )
        .expect("counts");
    assert!(walked > 0, "the first directory's new series was walked");
    assert_eq!(counts(&state), before, "nothing was derived");
    let first = get_json(&router, &a_dirs_scan(dirs[0])).await;
    assert_eq!(first["last_scan_status"], "failed", "{first}");
    assert_eq!(keys_of(&first["report"]), vec!["error"], "{first}");
    let error = first["report"]["error"].as_str().expect("a sentence");
    assert!(
        error.contains(&gone) && !error.contains(&first_path),
        "names the directory that failed, not the one walked: {error}"
    );
}

/// A derive that fails after both walks: each directory's walk wrote `ok`, and
/// each is marked `failed`, with the one error as every directory's report.
#[tokio::test]
async fn a_failed_derive_marks_every_dir_and_reports_the_error() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    state
        .library
        .conn()
        .execute_batch(
            "CREATE TRIGGER a_derive_fault BEFORE INSERT ON work
             BEGIN SELECT RAISE(ABORT, 'a planted derive fault'); END;",
        )
        .expect("plants the fault");

    let (status, started) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let ended = until_the_library_scan_ends(&router, id).await;

    assert_eq!(ended["running"], false, "{ended}");
    for dir in dirs {
        let own = get_json(&router, &a_dirs_scan(dir)).await;
        assert_eq!(own["running"], false, "{own}");
        assert_eq!(own["last_scan_status"], "failed", "{own}");
        assert_eq!(keys_of(&own["report"]), vec!["error"], "{own}");
        let error = own["report"]["error"].as_str().expect("a sentence");
        assert!(
            error.starts_with(
                "the directories the library shows were scanned, but deriving its structure afterward failed:"
            ),
            "{error}"
        );
        assert!(error.contains("a planted derive fault"), "{error}");
    }
}

/// A job that panics leaves no directory running: each entry ends done with an
/// error that carries the panic's words, and the library can be scanned again.
#[tokio::test]
async fn a_panicking_scan_leaves_no_dir_running() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, id).expect("the directories")
    };

    let started = std::time::Instant::now();
    crate::scan_jobs::reserve(&state, &roots, started, Some(id)).expect("reserves");
    assert_eq!(get_json(&router, &the_scan_of(id)).await["running"], true);
    // The job dies holding the job lock, which poisons it: the scans after it
    // must go on regardless.
    crate::scan_jobs::spawn_job(
        std::sync::Arc::clone(&state),
        roots,
        started,
        |state, _, _| {
            let _job = state.library_job.lock().expect("the job lock is free");
            panic!("a walk went wrong")
        },
    );
    let ended = until_the_library_scan_ends(&router, id).await;
    assert!(
        state.library_job.is_poisoned(),
        "the panic poisoned the lock"
    );

    assert_eq!(
        ended["dirs"],
        serde_json::json!([
            { "root_id": dirs[0], "running": false },
            { "root_id": dirs[1], "running": false },
        ])
    );
    for dir in dirs {
        let own = get_json(&router, &a_dirs_scan(dir)).await;
        assert_eq!(own["running"], false, "{own}");
        assert_eq!(
            own["report"],
            serde_json::json!({ "error": "the scan panicked: a walk went wrong" })
        );
    }
    let (status, _) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "a second scan may start");
    until_the_library_scan_ends(&router, id).await;
    let own = get_json(&router, &a_dirs_scan(dirs[0])).await;
    assert!(own["report"]["scan"].is_object(), "{own}");
}

/// A library never scanned: nothing is running, and `elapsed_secs` is null,
/// not zero.
#[tokio::test]
async fn a_library_never_scanned_is_not_running() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let dir = listed["libraries"][0]["dirs"][0]["id"]
        .as_i64()
        .expect("an id");

    let status = get_json(&router, &the_scan_of(id)).await;

    assert_eq!(
        status,
        serde_json::json!({
            "library_id": id,
            "running": false,
            "elapsed_secs": null,
            "dirs": [{ "root_id": dir, "running": false }],
            "held_removals": [],
            "moves_held": false,
            "derive_running": false,
            "scans": [],
        })
    );
}

#[tokio::test]
async fn a_scan_of_a_library_this_store_does_not_hold_is_a_404() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    for method in [Method::GET, Method::POST] {
        let (status, missing) = answered(
            &router,
            method,
            "/api/library/libraries/4242/scan",
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
        the_sentence(&missing);
    }
    assert!(state.scans.lock().unwrap().is_empty(), "nothing reserved");
}

#[tokio::test]
async fn a_scan_of_an_id_that_is_not_a_number_is_a_400_with_a_sentence() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    for method in [Method::GET, Method::POST] {
        let (status, refused) = answered(
            &router,
            method,
            "/api/library/libraries/abc/scan",
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        the_sentence(&refused);
    }
    assert!(state.scans.lock().unwrap().is_empty(), "nothing reserved");
}

/// A missing `name` is answered in the sentence of the route that was asked,
/// whole.
#[tokio::test]
async fn a_missing_name_is_answered_in_the_sentence_of_the_route_asked() {
    let (router, state, _held) = test_support::router_over_a_single_scanned_work();
    let id = taiyaku_library::libraries::list(&state.library.conn()).expect("lists")[0].id;
    let stored = the_store(&state);

    let (status, refused) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "dirs": ["/p"] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert_eq!(the_sentence(&refused), "a library needs a `name`, a string");

    let (status, refused) = answered(
        &router,
        Method::PATCH,
        &format!("/api/library/libraries/{id}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert_eq!(
        the_sentence(&refused),
        "renaming a library needs a `name`, a string"
    );
    assert_eq!(the_store(&state), stored);
}

/// A device credential reaches none of these routes, on any method.
#[tokio::test]
async fn an_api_key_reaches_no_libraries_route() {
    use tower::ServiceExt as _;
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let stored = the_store(&state);

    for (method, path) in [
        (Method::GET, "/api/library/libraries"),
        (Method::POST, "/api/library/libraries"),
        (Method::PATCH, "/api/library/libraries/1"),
        (Method::DELETE, "/api/library/libraries/1"),
        (Method::DELETE, "/api/library/libraries/1?dry_run=true"),
        (Method::POST, "/api/library/libraries/1/dirs"),
        (Method::DELETE, "/api/library/libraries/1/dirs/1"),
        (
            Method::DELETE,
            "/api/library/libraries/1/dirs/1?dry_run=true",
        ),
        (Method::POST, "/api/library/roots/1/move"),
        (Method::POST, "/api/library/roots/1/move?dry_run=true"),
        (Method::GET, "/api/library/libraries/1/scan"),
        (Method::POST, "/api/library/libraries/1/scan"),
    ] {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(method.clone())
                    .uri(path)
                    .header(
                        axum::http::header::AUTHORIZATION,
                        test_support::fixture_authorization(),
                    )
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = test_support::body_string(response).await;

        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{method} {path} answered a device credential: {body}"
        );
        assert!(
            body.contains("API key"),
            "{method} {path} refused for some other reason: {body}"
        );
    }
    assert_eq!(the_store(&state), stored, "a refused key writes nothing");
}

/// A rehearsal is `dry_run=true` and nothing else; the spellings that could be
/// read as either answer 400 and remove nothing, and `dry_run=false` removes.
#[tokio::test]
async fn a_dry_run_spelt_any_other_way_is_refused_and_false_removes() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/libraries/{id}");
    let stored = the_store(&state);

    for query in [
        "?dry_run=",
        "?dry_run",
        "?dry_run=true&dry_run=false",
        "?dry_run=false&dry_run=true",
    ] {
        let (status, refused) = answered(
            &router,
            Method::DELETE,
            &format!("{path}{query}"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {refused}");
        assert!(
            the_sentence(&refused).contains("query"),
            "{query}: {refused}"
        );
        assert_eq!(the_store(&state), stored, "{query} removed something");
    }

    let (status, removed) = answered(
        &router,
        Method::DELETE,
        &format!("{path}?dry_run=false"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["dry_run"], false, "{removed}");
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        after["libraries"].as_array().expect("libraries").len(),
        1,
        "{after}"
    );
}

/// What a removal reports is the works, the reading positions and the notes
/// that go with it, each its own number, in the rehearsal and in the removal;
/// another library's are neither counted nor removed.
#[tokio::test]
async fn a_removal_reports_its_works_positions_and_notes_each_by_its_own_count() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a_id = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let b_id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    {
        let conn = state.library.conn();
        // A second work under b, from a directory added to it and scanned.
        let extra = fixture.path().join("b-extra");
        test_support::write_pages(&extra.join("Series D").join("Series D - v01"), 3);
        let added = taiyaku_library::scan::add_root_checked_in(
            &conn,
            &extra,
            taiyaku_library::scan::InLibrary::Id(b_id),
        )
        .expect("adds a directory to library b");
        taiyaku_library::scan::scan_root(&conn, &added.root).expect("scans it");
        taiyaku_library::derive::derive(&conn).expect("derives");

        let works_of = |library: i64| -> Vec<i64> {
            conn.prepare(
                "SELECT DISTINCT e.work_id FROM edition e
                   JOIN member m ON m.edition_id = e.id JOIN library_root r ON r.root_id = m.root_id
                  WHERE r.library_id = ?1 ORDER BY 1",
            )
            .expect("prepares")
            .query_map([library], |row| row.get(0))
            .expect("queries")
            .collect::<Result<_, _>>()
            .expect("reads")
        };
        let b_works = works_of(b_id);
        let a_works = works_of(a_id);
        assert_eq!((b_works.len(), a_works.len()), (2, 1));
        // Three positions and five notes on b's two works; one position and two
        // notes on a's.
        for (work, edition) in [(b_works[0], "en"), (b_works[0], "raw"), (b_works[1], "en")] {
            conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, ?2, 1)",
                rusqlite::params![work, edition],
            )
            .expect("a position");
        }
        for (work, notes) in [(b_works[0], 3), (b_works[1], 2), (a_works[0], 2)] {
            for n in 0..notes {
                conn.execute(
                    "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
                     VALUES (1, ?1, ?2, 'en', 1, 'x', 'now', 'now')",
                    rusqlite::params![format!("note-{work}-{n}"), work],
                )
                .expect("a note");
            }
        }
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'en', 1)",
            [a_works[0]],
        )
        .expect("a position on a's work");
    }
    let path = format!("/api/library/libraries/{b_id}");
    let stored = the_store(&state);

    let (status, rehearsed) = answered(
        &router,
        Method::DELETE,
        &format!("{path}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(
        rehearsed["removed"],
        serde_json::json!({
            "works": 2,
            "positions": 3,
            "notes": 5,
            "positions_page_gone": 0,
            "notes_page_gone": 0,
            "left_with_no_library": [],
            "libraries_removed": ["root-b"],
            "kept": [],
        }),
        "{rehearsed}"
    );
    assert_eq!(the_store(&state), stored, "a rehearsal writes nothing");

    let (status, removed) = answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(
        removed["removed"],
        serde_json::json!({
            "works": 2,
            "positions": 3,
            "notes": 5,
            "positions_page_gone": 0,
            "notes_page_gone": 0,
            "left_with_no_library": [],
            "libraries_removed": ["root-b"],
            "kept": [],
        }),
        "{removed}"
    );
    let conn = state.library.conn();
    let left = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("counts")
    };
    assert_eq!(left("progress"), 1, "only a's position is left");
    assert_eq!(left("note"), 2, "only a's notes are left");
    assert_eq!(left("work"), 1, "only a's work is left");
}

/// Waits for the job `scan_jobs::spawn` started, with a bound that fails
/// the test where a job never ends.
async fn the_job_ends(job: tokio::task::JoinHandle<()>) {
    tokio::time::timeout(std::time::Duration::from_secs(10), job)
        .await
        .expect("the scan's job ended within ten seconds")
        .expect("the scan's job did not panic outside its own catch");
}

/// A second library with one directory of its own, and the ids of both.
async fn a_library_of_its_own(
    router: &axum::Router,
    fixture: &test_support::Fixture,
    name: &str,
) -> (i64, i64) {
    let (status, made) = answered(
        router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({
            "name": name,
            "dirs": [a_dir_of(fixture, name, &format!("{name} Series"))],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    (
        made["library"]["id"].as_i64().expect("an id"),
        made["library"]["dirs"][0]["id"].as_i64().expect("an id"),
    )
}

/// A removal of library `id` that landed after the `DELETE`'s check of the
/// registry and before the scan took the job lock, which the route refuses once
/// the scan is reserved: through the store, then the registry's slots cleared
/// as the route clears them when it answers.
fn remove_library(state: &crate::ServeState, id: i64) {
    let working = state.library.working().expect("a working connection");
    let dirs = super::dirs_of(&working, id).expect("the directories");
    taiyaku_library::scan::remove_library(&working, id).expect("removes the library");
    let mut scans = state.scans.lock().unwrap();
    for dir in &dirs {
        scans.remove(&dir.id);
    }
}

/// A job that answers fewer reports than there are directories still releases
/// every directory, and each unmatched one holds an error.
#[tokio::test]
async fn a_job_answering_too_few_reports_releases_every_dir() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    crate::scan_jobs::reserve(&state, &roots, started, Some(id)).expect("reserves");

    let job =
        crate::scan_jobs::spawn_job(std::sync::Arc::clone(&state), roots, started, |_, _, _| {
            Ok(vec![serde_json::json!({ "scan": "the first's" })])
        });
    the_job_ends(job).await;

    let status = get_json(&router, &the_scan_of(id)).await;
    assert_eq!(status["running"], false, "{status}");
    let first = get_json(&router, &a_dirs_scan(dirs[0])).await;
    assert_eq!(
        first["report"],
        serde_json::json!({ "scan": "the first's" })
    );
    let second = get_json(&router, &a_dirs_scan(dirs[1])).await;
    assert_eq!(second["running"], false, "{second}");
    assert_eq!(keys_of(&second["report"]), vec!["error"], "{second}");
}

/// The library is removed while its scan waits on the job lock: the job
/// walks nothing, writes no entry for the directories' freed ids, and files no row.
#[tokio::test]
async fn a_library_removed_while_its_scan_waits_leaves_nothing_behind() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dir) = a_library_of_its_own(&router, &fixture, "Doomed").await;
    let roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started, Some(id)).expect("reserves");
    let job = crate::scan_jobs::spawn(
        std::sync::Arc::clone(&state),
        roots,
        started,
        crate::scan_jobs::Reread::Library(id),
    );

    remove_library(&state, id);
    drop(held);
    the_job_ends(job).await;

    assert!(
        !state.scans.lock().unwrap().contains_key(&dir),
        "no entry for the freed directory id"
    );
}

/// The same, with a new library's directory taking the freed id before the job
/// runs: that directory is never scanned, and answers as it did.
#[tokio::test]
async fn a_scan_that_waited_through_a_removal_does_not_touch_the_dir_that_took_its_id() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dir) = a_library_of_its_own(&router, &fixture, "Doomed").await;
    let roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started, Some(id)).expect("reserves");
    let job = crate::scan_jobs::spawn(
        std::sync::Arc::clone(&state),
        roots,
        started,
        crate::scan_jobs::Reread::Library(id),
    );

    remove_library(&state, id);
    let (fresh, fresh_dir) = a_library_of_its_own(&router, &fixture, "Fresh").await;
    assert_eq!(fresh_dir, dir, "the freed id is the next directory's");
    drop(held);
    the_job_ends(job).await;

    assert_eq!(scan_entries_in(&state, fresh_dir), 0, "not walked");
    let status = get_json(&router, &a_dirs_scan(fresh_dir)).await;
    assert_eq!(status["running"], false, "{status}");
    assert!(status["report"].is_null(), "{status}");
    assert!(status["last_scan_status"].is_null(), "{status}");
    let library = get_json(&router, &the_scan_of(fresh)).await;
    assert_eq!(library["running"], false, "{library}");
}

/// A directory added to the library while its scan waits makes what the job
/// captured stale: nothing is walked, and each captured directory ends with an
/// error saying so.
#[tokio::test]
async fn a_scan_that_waited_through_a_new_dir_walks_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, id).expect("the directories")
    };
    let captured = roots[0].id;
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started, Some(id)).expect("reserves");
    let job = crate::scan_jobs::spawn(
        std::sync::Arc::clone(&state),
        roots,
        started,
        crate::scan_jobs::Reread::Library(id),
    );
    let (status, added) = answered(
        &router,
        Method::POST,
        &format!("/api/library/libraries/{id}/dirs"),
        &serde_json::json!({ "path": a_dir_of(&fixture, "Late", "Late Series") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let late = added["root"]["id"].as_i64().expect("an id");
    drop(held);
    the_job_ends(job).await;

    let own = get_json(&router, &a_dirs_scan(captured)).await;
    assert_eq!(own["running"], false, "{own}");
    assert_eq!(keys_of(&own["report"]), vec!["error"], "{own}");
    assert!(
        own["report"]["error"]
            .as_str()
            .expect("a sentence")
            .contains("changed while"),
        "{own}"
    );
    assert_eq!(
        scan_entries_in(&state, late),
        0,
        "the new directory is not walked"
    );
}

/// The route takes no query: one it was sent is refused, not dropped, and
/// nothing is reserved.
#[tokio::test]
async fn a_librarys_scan_takes_no_query() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    for query in ["?full=true", "?anything=1"] {
        let (status, refused) = answered(
            &router,
            Method::POST,
            &format!("{}{query}", the_scan_of(id)),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {refused}");
        the_sentence(&refused);
    }
    assert!(state.scans.lock().unwrap().is_empty(), "nothing reserved");
}

/// The directory that took a removed library's directory's id is reserved by a scan
/// of its own while the first scan still waits: the first scan's end leaves
/// that reservation as it is.
#[tokio::test]
async fn a_scan_that_waited_through_a_removal_leaves_a_later_scans_reservation_alone() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dir) = a_library_of_its_own(&router, &fixture, "Doomed").await;
    let roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started, Some(id)).expect("reserves");
    let job = crate::scan_jobs::spawn(
        std::sync::Arc::clone(&state),
        roots,
        started,
        crate::scan_jobs::Reread::Library(id),
    );
    remove_library(&state, id);
    let (fresh, fresh_dir) = a_library_of_its_own(&router, &fixture, "Fresh").await;
    assert_eq!(fresh_dir, dir, "the freed id is the next directory's");
    let fresh_roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, fresh).expect("the directories")
    };
    let later = std::time::Instant::now();
    crate::scan_jobs::reserve(&state, &fresh_roots, later, Some(fresh)).expect("reserves");

    drop(held);
    the_job_ends(job).await;

    let status = get_json(&router, &the_scan_of(fresh)).await;
    assert_eq!(
        status["running"], true,
        "the later scan is still running: {status}"
    );
    let scans = state.scans.lock().unwrap();
    let entry = scans.get(&fresh_dir).expect("its reservation");
    assert!(!entry.done && entry.report.is_none() && entry.started == later);
}

/// A scan of library `id` reserved and spawned while the job lock is held:
/// it waits, directories captured as of now, until the returned guard is dropped.
fn a_waiting_scan(
    state: &std::sync::Arc<crate::ServeState>,
    id: i64,
) -> (HeldJob, tokio::task::JoinHandle<()>) {
    let roots = {
        let conn = state.library.conn();
        super::walked_dirs_of(&conn, id).expect("the directories")
    };
    let held = holding_the_job(state);
    let started = std::time::Instant::now();
    crate::scan_jobs::reserve(state, &roots, started, Some(id)).expect("reserves");
    let job = crate::scan_jobs::spawn(
        std::sync::Arc::clone(state),
        roots,
        started,
        crate::scan_jobs::Reread::Library(id),
    );
    (held, job)
}

/// [`remove_library`]'s twin for one directory.
fn remove_dir(state: &crate::ServeState, dir: i64) {
    let working = state.library.working().expect("a working connection");
    taiyaku_library::scan::remove_root(&working, dir).expect("removes the directory");
    state.scans.lock().unwrap().remove(&dir);
}

async fn add_dir_to(router: &axum::Router, library: i64, path: &str) -> i64 {
    let (status, added) = answered(
        router,
        Method::POST,
        &format!("/api/library/libraries/{library}/dirs"),
        &serde_json::json!({ "path": path }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    added["root"]["id"].as_i64().expect("an id")
}

/// After a scan that was not to walk this directory: no row under it, no status
/// written, and its own route answers as it did before any scan.
async fn never_scanned(router: &axum::Router, state: &crate::ServeState, dir: i64) {
    assert_eq!(
        scan_entries_in(state, dir),
        0,
        "no row filed under directory {dir}"
    );
    let own = get_json(router, &a_dirs_scan(dir)).await;
    assert_eq!(own["running"], false, "{own}");
    assert!(own["report"].is_null(), "{own}");
    assert!(own["last_scan_status"].is_null(), "{own}");
}

/// The library's newest directory is removed and another directory, of another
/// path, is added to the same library while the scan waits: the new directory
/// takes the freed id, and the scan, which was asked about the old one, walks
/// neither.
#[tokio::test]
async fn a_scan_that_waited_through_a_swapped_dir_does_not_walk_the_old_path_under_the_new() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let (held, job) = a_waiting_scan(&state, id);

    remove_dir(&state, dirs[1]);
    let swapped = add_dir_to(&router, id, &a_dir_of(&fixture, "Third", "Third Series")).await;
    assert_eq!(swapped, dirs[1], "the new directory took the freed id");
    drop(held);
    the_job_ends(job).await;

    never_scanned(&router, &state, swapped).await;
}

/// A directory removed from the library, its id taken by another library's new
/// directory, and its path added back to the first library under a new id: the
/// paths the scan captured all still stand and one id does not. The scan
/// walks nothing, so the directory that came back, which it holds no reservation
/// for, is not walked, nor is the other library's, and the directory it does
/// hold reports why.
#[tokio::test]
async fn a_scan_that_waited_through_a_dir_re_added_under_a_new_id_walks_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let second_path = listed["libraries"][0]["dirs"][1]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    let (held, job) = a_waiting_scan(&state, id);

    remove_dir(&state, dirs[1]);
    let (_other, taken) = a_library_of_its_own(&router, &fixture, "Other").await;
    assert_eq!(taken, dirs[1], "another library's directory took the id");
    let back = add_dir_to(&router, id, &second_path).await;
    assert_ne!(back, dirs[1], "the path came back under a new id");
    drop(held);
    the_job_ends(job).await;

    never_scanned(&router, &state, back).await;
    never_scanned(&router, &state, taken).await;
    let kept = get_json(&router, &a_dirs_scan(dirs[0])).await;
    assert!(
        kept["report"]["error"]
            .as_str()
            .expect("a sentence")
            .contains("changed while"),
        "{kept}"
    );
}

/// One of the library's two directories is removed while the scan waits: the
/// one that remains is not walked, and keeps the status it had.
#[tokio::test]
async fn a_scan_that_waited_through_a_removed_dir_walks_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let before = get_json(&router, &a_dirs_scan(dirs[0])).await["last_scan_status"].clone();
    let (held, job) = a_waiting_scan(&state, id);

    remove_dir(&state, dirs[1]);
    drop(held);
    the_job_ends(job).await;

    let own = get_json(&router, &a_dirs_scan(dirs[0])).await;
    assert_eq!(own["running"], false, "{own}");
    assert_eq!(own["last_scan_status"], before, "{own}");
    assert!(
        own["report"]["error"]
            .as_str()
            .expect("a sentence")
            .contains("changed while"),
        "{own}"
    );
}

/// The store fails on the job's second reading of the library's directories: the
/// directories' reports hold the store's words, not a claim that the library
/// changed.
#[tokio::test]
async fn a_store_fault_reading_the_dirs_again_is_reported_in_the_stores_words() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let (held, job) = a_waiting_scan(&state, id);
    state
        .library
        .conn()
        .execute_batch("ALTER TABLE library RENAME TO library_away;")
        .expect("plants the fault");

    drop(held);
    the_job_ends(job).await;
    state
        .library
        .conn()
        .execute_batch("ALTER TABLE library_away RENAME TO library;")
        .expect("lifts the fault");

    for dir in dirs {
        let own = get_json(&router, &a_dirs_scan(dir)).await;
        assert_eq!(own["running"], false, "{own}");
        let error = own["report"]["error"].as_str().expect("a sentence");
        assert!(!error.contains("changed while"), "{error}");
        assert!(
            error.contains("no such table"),
            "the store's words: {error}"
        );
    }
}

/// A directory's exclusions saved while the scan waits are the ones the scan
/// walks with.
#[tokio::test]
async fn a_scan_that_waited_walks_the_dirs_settings_as_they_are_when_it_starts() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let (held, job) = a_waiting_scan(&state, id);

    let (status, saved) = answered(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{}", dirs[1]),
        &serde_json::json!({ "exclude_globs": "Second Series" }),
    )
    .await;
    assert!(status.is_success(), "{status}: {saved}");
    drop(held);
    the_job_ends(job).await;

    let own = get_json(&router, &a_dirs_scan(dirs[1])).await;
    assert_eq!(own["last_scan_status"], "ok", "{own}");
    assert_eq!(
        own["report"]["scan"]["excluded_dirs"],
        serde_json::json!(["Second Series"]),
        "the exclusion saved while the scan waited was applied: {own}"
    );
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(listed["libraries"][0]["works"], 1, "{listed}");
}

/// What a libraries route answers when its blocking work panics: a 500, as
/// JSON like every refusal here, naming what was running.
#[tokio::test]
async fn a_libraries_closure_that_panicked_is_a_500_naming_what_was_running() {
    let why = tokio::task::spawn_blocking(|| {
        panic!("a libraries closure, for the join error it produces");
    })
    .await
    .expect_err("a panicking closure joins as an error");

    let response = crate::config_api::admin_panicked("listing libraries", &why);
    let status = response.status();
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .map(|value| value.to_str().expect("a printable header").to_string());
    let body = test_support::body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert_eq!(
        json,
        serde_json::json!({
            "error": format!("listing libraries panicked: {why}")
        }),
        "the whole body is the one key, naming what was running: {body}"
    );
}

/// A directory's path that is empty or not absolute, as the routes are sent it.
/// The store resolves a relative path against the working directory, which is
/// right for the command line and not for a request.
const PATHS_THAT_ARE_NOT_ABSOLUTE: [&str; 4] = ["", "   ", "relative/dir", "./x"];

#[tokio::test]
async fn a_library_is_not_made_of_a_dir_whose_path_is_not_absolute() {
    let (router, state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-relative");
    let good = a_dir_of(&fixture, "Volumes", "Alpha");
    let stored = the_store(&state);

    for bad in PATHS_THAT_ARE_NOT_ABSOLUTE {
        for dirs in [vec![bad.to_string()], vec![good.clone(), bad.to_string()]] {
            let (status, refused) = answered(
                &router,
                Method::POST,
                "/api/library/libraries",
                &serde_json::json!({ "name": "Manga", "dirs": dirs }),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{dirs:?}: {refused}");
            assert!(
                the_sentence(&refused).contains("must be absolute"),
                "{dirs:?}: {refused}"
            );
            assert_eq!(the_store(&state), stored, "{dirs:?} left a row");
        }
    }

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Manga", "dirs": [good] }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an absolute path is still taken: {made}"
    );
}

#[tokio::test]
async fn a_dir_whose_path_is_not_absolute_is_not_added() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-dir-relative");
    let good = a_dir_of(&fixture, "Extra", "Gamma");
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/libraries/{id}/dirs");
    let stored = the_store(&state);

    for bad in PATHS_THAT_ARE_NOT_ABSOLUTE {
        let (status, refused) = answered(
            &router,
            Method::POST,
            &path,
            &serde_json::json!({ "path": bad }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad:?}: {refused}");
        assert!(
            the_sentence(&refused).contains("must be absolute"),
            "{bad:?}: {refused}"
        );
        assert_eq!(the_store(&state), stored, "{bad:?} left a row");
    }

    let (status, added) = answered(
        &router,
        Method::POST,
        &path,
        &serde_json::json!({ "path": good }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an absolute path is still taken: {added}"
    );
}

/// Every libraries route over a file-backed store, where the store's working
/// connection is a second connection and not the shared in-memory one the other
/// tests here run on: a library is made with two directories, renamed, given a
/// third, scanned (a second start refused while the first is held), rehearsed
/// and removed.
#[tokio::test]
async fn the_whole_flow_over_a_file_backed_store() {
    let (router, state, fixture) = test_support::router_over_a_file_backed_library();
    assert!(
        state.library.path().is_some(),
        "the store is a file, so `working()` is a connection of its own"
    );
    let dirs: Vec<String> = ["A", "B", "C"]
        .iter()
        .map(|name| a_dir_of(&fixture, &format!("more/{name}"), &format!("{name} Series")))
        .collect();

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Two", "dirs": [dirs[0], dirs[1]] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["added"].as_array().map(Vec::len), Some(2), "{made}");
    let id = made["library"]["id"].as_i64().expect("an id");

    let (status, renamed) = answered(
        &router,
        Method::PATCH,
        &format!("/api/library/libraries/{id}"),
        &serde_json::json!({ "name": "Two Renamed" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(
        renamed,
        serde_json::json!({ "id": id, "name": "Two Renamed" })
    );
    let (status, added) = answered(
        &router,
        Method::POST,
        &format!("/api/library/libraries/{id}/dirs"),
        &serde_json::json!({ "path": dirs[2] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");

    // The job lock is held, so the scan the first start reserves waits, and the
    // second start finds the first still running.
    let held = holding_the_job(&state);
    let (status, started) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let (status, again) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    assert!(the_sentence(&again).contains("already running"), "{again}");
    assert_eq!(get_json(&router, &the_scan_of(id)).await["running"], true);
    drop(held);
    let ended = until_the_library_scan_ends(&router, id).await;
    assert_eq!(ended["dirs"].as_array().map(Vec::len), Some(3), "{ended}");

    let listed = get_json(&router, "/api/library/libraries").await;
    let two = the_library(&listed, "Two Renamed");
    assert_eq!(two["works"], 3, "{listed}");
    assert_eq!(two["books"], 0, "{listed}");
    let ids: Vec<i64> = two["dirs"]
        .as_array()
        .expect("dirs")
        .iter()
        .map(|dir| dir["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(ids.len(), 3, "{listed}");
    for dir in &ids {
        let own = get_json(&router, &a_dirs_scan(*dir)).await;
        assert_eq!(own["last_scan_status"], "ok", "{own}");
        assert!(own["report"]["scan"].is_object(), "{own}");
        assert!(own["report"]["derive"].is_object(), "{own}");
    }

    let removal = format!("/api/library/libraries/{id}");
    let (status, rehearsed) = answered(
        &router,
        Method::DELETE,
        &format!("{removal}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(rehearsed["dry_run"], true, "{rehearsed}");
    assert_eq!(
        rehearsed["removed"],
        serde_json::json!({
            "notes": 0,
            "positions": 0,
            "works": 3,
            "positions_page_gone": 0,
            "notes_page_gone": 0,
            "left_with_no_library": [],
            "libraries_removed": ["Two Renamed"],
            "kept": [],
        }),
        "{rehearsed}"
    );
    let after_rehearsal = get_json(&router, "/api/library/libraries").await;
    assert_eq!(after_rehearsal, listed, "a rehearsal removes nothing");

    let (status, removed) =
        answered(&router, Method::DELETE, &removal, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["dry_run"], false, "{removed}");
    assert_eq!(removed["removed"], rehearsed["removed"], "{removed}");

    let listed = get_json(&router, "/api/library/libraries").await;
    assert!(
        listed["libraries"]
            .as_array()
            .expect("libraries")
            .iter()
            .all(|library| library["id"] != id),
        "{listed}"
    );
    assert!(
        state.scans.lock().unwrap().is_empty(),
        "no directory's slot outlives its library"
    );
    let conn = state.library.conn();
    for (table, column) in [("library", "id"), ("library_root", "library_id")] {
        let rows: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE {column} = ?1"),
                [id],
                |row| row.get(0),
            )
            .expect("counts");
        assert_eq!(rows, 0, "{table} still holds rows of the library");
    }
    for dir in ids {
        let rows: i64 = conn
            .query_row(
                "SELECT (SELECT count(*) FROM root WHERE id = ?1)
                      + (SELECT count(*) FROM scan_entry WHERE root_id = ?1)",
                [dir],
                |row| row.get(0),
            )
            .expect("counts");
        assert_eq!(rows, 0, "directory {dir} left rows");
    }
}

/// Polls a directory's own status route until it says nothing is running, and
/// answers it; fails the test if the scan outlasts the ceiling.
async fn until_the_dirs_scan_ends(router: &axum::Router, dir: i64) -> serde_json::Value {
    for _ in 0..400 {
        let status = get_json(router, &a_dirs_scan(dir)).await;
        if status["running"] == false && !status["report"].is_null() {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the directory's scan did not end within ten seconds");
}

/// A directory scanned through its own route, in a library of two directories, is
/// walked and derived: its report is a finished scan, not the library's
/// "changed while the scan waited" answer, which only a scan that reads the
/// library's directories again can give.
#[tokio::test]
async fn a_dir_scanned_by_its_own_route_in_a_library_of_two_is_walked() {
    let (router, _state, fixture) = test_support::router_over_a_single_scanned_work();
    let (_id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;

    let (status, started) = answered(
        &router,
        Method::POST,
        &a_dirs_scan(dirs[1]),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let own = until_the_dirs_scan_ends(&router, dirs[1]).await;

    assert_eq!(keys_of(&own["report"]), vec!["derive", "scan"], "{own}");
    assert_eq!(own["last_scan_status"], "ok", "{own}");
}

/// A directory whose walk fails, scanned through its own route, reports the
/// store's own sentence and none of the library route's wording of it.
#[tokio::test]
async fn a_dirs_walk_failure_is_reported_in_the_stores_words() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (_id, dirs) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let gone = {
        let conn = state.library.conn();
        taiyaku_library::scan::roots(&conn)
            .expect("roots")
            .into_iter()
            .find(|root| root.id == dirs[1])
            .expect("the directory")
    };
    std::fs::remove_dir_all(&gone.path).expect("removes the directory");
    let stores_words = {
        let working = state.library.working().expect("a working connection");
        taiyaku_library::scan::scan_root(&working, &gone).expect_err("the walk fails")
    };

    let (status, started) = answered(
        &router,
        Method::POST,
        &a_dirs_scan(dirs[1]),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let own = until_the_dirs_scan_ends(&router, dirs[1]).await;

    assert_eq!(
        own["report"],
        serde_json::json!({ "error": stores_words }),
        "{own}"
    );
}

/// A library scanned through its own route that waits on the job lock while a
/// directory is added to the library walks nothing: each directory it reserved ends
/// with the library's "changed" answer.
#[tokio::test]
async fn a_librarys_scan_through_its_route_notices_a_dir_added_while_it_waited() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let captured = listed["libraries"][0]["dirs"][0]["id"]
        .as_i64()
        .expect("an id");
    let held = holding_the_job(&state);
    let (status, started) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let late = add_dir_to(&router, id, &a_dir_of(&fixture, "Late", "Late Series")).await;
    drop(held);
    until_the_library_scan_ends(&router, id).await;

    let own = get_json(&router, &a_dirs_scan(captured)).await;
    assert_eq!(
        own["report"],
        serde_json::json!({ "error": crate::scan_jobs::CHANGED }),
        "{own}"
    );
    assert_eq!(
        scan_entries_in(&state, late),
        0,
        "the new directory is not walked"
    );
}

async fn moved(
    router: &axum::Router,
    dir: i64,
    dry_run: bool,
    body: &serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let query = if dry_run { "?dry_run=true" } else { "" };
    answered(
        router,
        Method::POST,
        &format!("/api/library/roots/{dir}/move{query}"),
        body,
    )
    .await
}

/// Two libraries of one directory each. The rehearsal says what the move would
/// do and changes nothing; the move does it. `ann`, granted both, sees the
/// directory before and after; `bo`, granted root-a alone, gains it.
#[tokio::test]
async fn a_dir_is_rehearsed_and_then_moved_to_another_library() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let b = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let dir = the_library(&listed, "root-b")["dirs"][0]["id"]
        .as_i64()
        .expect("a directory id");
    {
        let conn = state.library.conn();
        for (name, granted) in [("ann", &[a, b][..]), ("bo", &[a][..])] {
            let account = taiyaku_library::account::add(&conn, name, "pw", "user").expect("adds");
            for library in granted {
                taiyaku_library::account::grant_library(&conn, account, *library).expect("grants");
            }
        }
    }
    let to = serde_json::json!({ "from": b, "library": a });
    let stored = the_store(&state);

    let (status, rehearsed) = moved(&router, dir, true, &to).await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(keys_of(&rehearsed), ["dry_run", "moved"], "{rehearsed}");
    assert_eq!(rehearsed["dry_run"], true);
    assert_eq!(rehearsed["moved"]["library"], "root-a");
    assert_eq!(
        rehearsed["moved"]["gains_sight"],
        serde_json::json!(["bo"]),
        "{rehearsed}"
    );
    assert_eq!(
        rehearsed["moved"]["loses_sight"],
        serde_json::json!([]),
        "{rehearsed}"
    );
    assert_eq!(
        rehearsed["moved"]["library_removed"], "root-b",
        "{rehearsed}"
    );
    assert_eq!(
        keys_of(&rehearsed["moved"]),
        [
            "gains_sight",
            "grants_removed",
            "left_with_no_library",
            "library",
            "library_id",
            "library_removed",
            "loses_sight",
        ]
    );
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
    assert_eq!(the_store(&state), stored, "the rehearsal changed the store");

    let (status, done) = moved(&router, dir, false, &to).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["dry_run"], false);
    assert_eq!(done["moved"], rehearsed["moved"]);
    let after = get_json(&router, "/api/library/libraries").await;
    let libraries = after["libraries"].as_array().expect("libraries");
    assert_eq!(libraries.len(), 1, "{after}");
    assert_eq!(libraries[0]["dirs"].as_array().expect("dirs").len(), 2);
    assert_eq!(libraries[0]["works"], 2, "{after}");
}

/// Moving away the one directory of a reader's only library goes ahead: the
/// rehearsal names the reader under `left_with_no_library`, the move answers
/// the same, and the reader is then listed nothing.
#[tokio::test]
async fn moving_an_accounts_only_library_away_names_the_reader_who_then_sees_nothing() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    // root-a gains a second directory before `solo` is made, so the moved
    // directory's id is not its library's, and a holder lookup keyed on the
    // directory's id finds no one.
    let (dir, from, reader) = {
        use taiyaku_library::scan::InLibrary;
        let conn = state.library.conn();
        let mut added = Vec::new();
        for (dir, library, series) in [
            ("root-a-more", InLibrary::Named("root-a"), "Series C"),
            ("solo", InLibrary::Own, "Series D"),
        ] {
            let dir = fixture.path().join(dir);
            test_support::write_pages(&dir.join(series).join(format!("{series} - v01")), 2);
            let root = taiyaku_library::scan::add_root_in(&conn, &dir, library).expect("adds");
            taiyaku_library::scan::scan_root(&conn, &root).expect("scans");
            added.push(root);
        }
        taiyaku_library::derive::derive(&conn).expect("derives");
        let solo = added.pop().expect("solo");
        assert_ne!(
            solo.id, solo.libraries[0],
            "the fixture must tell a directory's id from its library's: {solo:?}"
        );
        let reader = taiyaku_library::account::add(&conn, "reader", "pw", "user").expect("adds");
        taiyaku_library::account::grant_library(&conn, reader, solo.libraries[0]).expect("grants");
        (solo.id, solo.libraries[0], reader)
    };
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    assert_eq!(
        test_support::seen_by(&router, &state, reader).await,
        1,
        "the fixture: the reader sees solo's series"
    );
    let to = serde_json::json!({ "from": from, "library": a });
    let stored = the_store(&state);

    let (status, rehearsed) = moved(&router, dir, true, &to).await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(
        rehearsed["moved"]["left_with_no_library"],
        serde_json::json!(["reader"]),
        "{rehearsed}"
    );
    assert_eq!(the_store(&state), stored, "the rehearsal changed the store");

    let (status, done) = moved(&router, dir, false, &to).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["moved"], rehearsed["moved"]);
    assert_eq!(
        test_support::seen_by(&router, &state, reader).await,
        0,
        "the reader sees no library, not every library"
    );
}

#[tokio::test]
async fn a_dir_is_moved_to_a_library_made_for_it() {
    let (router, _state, fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let (status, added) = answered(
        &router,
        Method::POST,
        &format!("/api/library/libraries/{}/dirs", a["id"]),
        &serde_json::json!({ "path": a_dir_of(&fixture, "Extra", "Gamma") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let dir = added["root"]["id"].as_i64().expect("a directory id");

    let (status, done) = moved(
        &router,
        dir,
        false,
        &serde_json::json!({ "from": a["id"], "new_library": " Extras " }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(keys_of(&done), ["dry_run", "moved"], "{done}");
    assert_eq!(done["moved"]["library"], "Extras");
    assert_eq!(done["moved"]["library_removed"], serde_json::Value::Null);
    assert_eq!(done["moved"]["gains_sight"], serde_json::json!([]));
    assert_eq!(done["moved"]["loses_sight"], serde_json::json!([]));
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(the_library(&after, "Extras")["dirs"][0]["id"], dir);
    assert_eq!(
        the_library(&after, "root-a")["dirs"]
            .as_array()
            .expect("dirs")
            .len(),
        1
    );
}

/// `from` is required, and must hold the directory; the target must not.
#[tokio::test]
async fn a_move_that_names_no_one_target_or_nothing_there_is_the_callers_mistake() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let own = a["id"].as_i64().expect("an id");
    let b = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let dir = a["dirs"][0]["id"].as_i64().expect("a directory id");
    let path = a["dirs"][0]["path"].as_str().expect("a path");
    let stored = the_store(&state);

    for (body, status, says) in [
        (
            serde_json::json!({ "library": b }),
            StatusCode::BAD_REQUEST,
            "a move names `from`, the id of the library the directory leaves".to_string(),
        ),
        (
            serde_json::json!({ "from": "one", "library": b }),
            StatusCode::BAD_REQUEST,
            "`from` must be a library id, a whole number".to_string(),
        ),
        (
            serde_json::json!({ "from": own }),
            StatusCode::BAD_REQUEST,
            "exactly one".to_string(),
        ),
        (
            serde_json::json!({ "from": own, "library": 1, "new_library": "X" }),
            StatusCode::BAD_REQUEST,
            "exactly one".to_string(),
        ),
        (
            serde_json::json!({ "from": own, "library": "one" }),
            StatusCode::BAD_REQUEST,
            "`library`".to_string(),
        ),
        (
            serde_json::json!({ "from": own, "new_library": 7 }),
            StatusCode::BAD_REQUEST,
            "`new_library`".to_string(),
        ),
        (
            serde_json::json!({ "from": own, "new_library": "  " }),
            StatusCode::BAD_REQUEST,
            "name".to_string(),
        ),
        (
            serde_json::json!({ "from": own, "new_library": "root-b" }),
            StatusCode::BAD_REQUEST,
            "root-b".to_string(),
        ),
        (
            serde_json::json!({ "from": own, "library": own }),
            StatusCode::BAD_REQUEST,
            format!("`{path}` is already in root-a"),
        ),
        (
            serde_json::json!({ "from": own, "library": 4242 }),
            StatusCode::NOT_FOUND,
            "there is no library 4242".to_string(),
        ),
        (
            serde_json::json!({ "from": b, "library": own }),
            StatusCode::NOT_FOUND,
            format!("`{path}` is not in root-b"),
        ),
        (
            serde_json::json!({ "from": 4242, "library": b }),
            StatusCode::NOT_FOUND,
            "there is no library 4242".to_string(),
        ),
    ] {
        for dry_run in [true, false] {
            let (got, refused) = moved(&router, dir, dry_run, &body).await;
            assert_eq!(got, status, "{body}: {refused}");
            assert!(the_sentence(&refused).contains(&says), "{body}: {refused}");
        }
    }
    let (status, missing) = moved(
        &router,
        4242,
        false,
        &serde_json::json!({ "from": own, "new_library": "X" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
    assert_eq!(the_sentence(&missing), "there is no directory 4242");

    let (status, not_json) = answered_raw(
        &router,
        Method::POST,
        &format!("/api/library/roots/{dir}/move"),
        "not json",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{not_json}");
    assert!(the_sentence(&not_json).contains("JSON"), "{not_json}");

    let (status, no_id) = answered(
        &router,
        Method::POST,
        "/api/library/roots/one/move",
        &serde_json::json!({ "from": own, "new_library": "X" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{no_id}");
    assert_eq!(
        the_sentence(&no_id),
        "`one` is not a directory id; an id is a whole number"
    );

    let (status, misspelt) = answered(
        &router,
        Method::POST,
        &format!("/api/library/roots/{dir}/move?dryrun=true"),
        &serde_json::json!({ "from": own, "new_library": "X" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{misspelt}");
    assert!(the_sentence(&misspelt).contains("query"), "{misspelt}");
    assert_eq!(the_store(&state), stored);
}

/// A rehearsal is `dry_run=true` and nothing else; the spellings that could be
/// read as either answer 400 and move nothing, and `dry_run=false` moves.
#[tokio::test]
async fn a_move_spelt_any_other_way_is_refused_and_false_moves() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let dir = the_library(&listed, "root-b")["dirs"][0]["id"]
        .as_i64()
        .expect("a directory id");
    let b = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/roots/{dir}/move");
    let to = serde_json::json!({ "from": b, "library": a });
    let stored = the_store(&state);

    for query in [
        "?dry_run=",
        "?dry_run",
        "?dry_run=true&dry_run=false",
        "?dry_run=false&dry_run=true",
    ] {
        let (status, refused) =
            answered(&router, Method::POST, &format!("{path}{query}"), &to).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {refused}");
        assert!(
            the_sentence(&refused).contains("query"),
            "{query}: {refused}"
        );
        assert_eq!(the_store(&state), stored, "{query} moved something");
    }

    let (status, done) =
        answered(&router, Method::POST, &format!("{path}?dry_run=false"), &to).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["dry_run"], false, "{done}");
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        after["libraries"].as_array().expect("libraries").len(),
        1,
        "{after}"
    );
}

/// A move changes which directories a scan's library shows, so it does not
/// run while anything holds the job lock: 409 at once, for a rehearsal too,
/// and nothing moved. The holder is neutral, so the sentence names no one job.
#[tokio::test]
async fn a_move_is_refused_while_the_job_lock_is_held() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let dir = the_library(&listed, "root-b")["dirs"][0]["id"]
        .as_i64()
        .expect("a directory id");
    let b = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let stored = the_store(&state);
    let held = holding_the_job(&state);

    for dry_run in [true, false] {
        let (status, refused) = moved(
            &router,
            dir,
            dry_run,
            &serde_json::json!({ "from": b, "library": a }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(keys_of(&refused), ["error", "volumes"], "{refused}");
        assert_eq!(
            refused["error"],
            "a scan or another library job is running; move the directory once it has finished",
            "{refused}"
        );
        assert_eq!(refused["volumes"], serde_json::json!([]), "{refused}");
    }
    drop(held);
    assert_eq!(the_store(&state), stored);
}

/// A scan reserved and waiting holds no job lock, so the move asks the
/// registry: for a directory of the library the directory leaves and for one of the
/// library it goes to. The reservation is left as it was.
#[tokio::test]
async fn a_move_is_refused_while_a_scan_of_either_library_is_reserved() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let b = the_library(&listed, "root-b");
    let a_dir = a["dirs"][0]["id"].as_i64().expect("a directory id");
    let b_dir = b["dirs"][0]["id"].as_i64().expect("a directory id");
    let to = serde_json::json!({ "from": b["id"], "library": a["id"] });
    let stored = the_store(&state);

    for (reserved, path) in [
        (a_dir, &a["dirs"][0]["path"]),
        (b_dir, &b["dirs"][0]["path"]),
    ] {
        a_scan_running_on(&state, reserved);
        for dry_run in [true, false] {
            let (status, refused) = moved(&router, b_dir, dry_run, &to).await;
            assert_eq!(status, StatusCode::CONFLICT, "{reserved}: {refused}");
            assert_eq!(
                refused["error"],
                format!(
                    "a scan of {} is running; move the directory once it has finished",
                    path.as_str().expect("a path")
                ),
                "{refused}"
            );
            assert_eq!(the_store(&state), stored, "{reserved} {dry_run} moved");
            assert!(
                state
                    .scans
                    .lock()
                    .unwrap()
                    .get(&reserved)
                    .is_some_and(|p| !p.done),
                "the reservation on {reserved} was taken away"
            );
        }
        state.scans.lock().unwrap().remove(&reserved);
    }

    // A new library is made, so only the library it leaves is asked.
    a_scan_running_on(&state, a_dir);
    let (status, made) = moved(
        &router,
        b_dir,
        true,
        &serde_json::json!({ "from": b["id"], "new_library": "Elsewhere" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a scan of an unrelated library: {made}"
    );
}

/// A scan reserved on a sibling of the moving directory, in the library it
/// leaves, holds the move up: it may be that library's scan, queued, whose
/// directories the move would change.
#[tokio::test]
async fn a_move_is_refused_while_a_scan_of_a_sibling_dir_is_reserved() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (from, [moving, sibling]) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let stored = the_store(&state);
    a_scan_running_on(&state, sibling);

    for dry_run in [true, false] {
        let (status, refused) = moved(
            &router,
            moving,
            dry_run,
            &serde_json::json!({ "from": from, "new_library": "Elsewhere" }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(
            refused["error"]
                .as_str()
                .expect("an error")
                .contains("is running; move the directory once it has finished"),
            "{refused}"
        );
    }
    assert_eq!(the_store(&state), stored);
}

/// A store fault in a move is the server's, not the caller's.
#[tokio::test]
async fn a_store_fault_moving_a_dir_is_a_500_not_a_400() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (dir, from) = {
        let conn = state.library.conn();
        let dir = taiyaku_library::scan::roots(&conn).expect("lists")[1].clone();
        conn.pragma_update(None, "query_only", true)
            .expect("sets query_only");
        (dir.id, dir.libraries[0])
    };
    let body = serde_json::json!({ "from": from, "new_library": "Elsewhere" });

    let response = crate::libraries_api::move_dir(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(dir.to_string()),
        Ok(axum::extract::Query(crate::config_api::DryRunQuery {
            dry_run: false,
        })),
        axum::body::Bytes::from(body.to_string()),
    )
    .await;
    let (status, text) = parsed(response).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "a store fault must not be answered as the caller's own mistake: {text}"
    );
}

/// Every value of `moved` under its own key, each a different value, so a
/// pair of keys swapped cannot pass.
#[test]
fn moved_is_written_with_each_value_under_its_own_key() {
    let moved = taiyaku_library::relocate::Moved {
        library_id: 11,
        library: "Target".to_string(),
        gains_sight: vec!["cy".to_string(), "di".to_string()],
        loses_sight: vec!["ed".to_string()],
        library_removed: Some("Left".to_string()),
        grants_removed: 99,
        left_with_no_library: vec!["ann".to_string(), "bo".to_string()],
    };
    let expected = serde_json::json!({
        "library_id": 11,
        "library": "Target",
        "gains_sight": ["cy", "di"],
        "loses_sight": ["ed"],
        "library_removed": "Left",
        "grants_removed": 99,
        "left_with_no_library": ["ann", "bo"],
    });
    assert_eq!(super::moved_json(&moved), expected);

    let kept = taiyaku_library::relocate::Moved {
        gains_sight: Vec::new(),
        loses_sight: Vec::new(),
        library_removed: None,
        left_with_no_library: Vec::new(),
        ..moved
    };
    let mut expected = expected;
    expected["gains_sight"] = serde_json::json!([]);
    expected["loses_sight"] = serde_json::json!([]);
    expected["library_removed"] = serde_json::Value::Null;
    expected["left_with_no_library"] = serde_json::json!([]);
    assert_eq!(super::moved_json(&kept), expected);
}

/// The sentence the pages draw for a library named `.` or `..`, which no page
/// address holds.
const NO_ADDRESS: &str = "An address cannot hold “.” or “..” as a name.";

/// Making a library, renaming one and moving a directory into a new one each
/// refuse `.` and `..` as the caller's mistake, in the sentence the pages draw,
/// and write nothing.
#[tokio::test]
async fn no_library_is_made_or_renamed_called_a_dot_name() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("library-dot-names");
    let good = a_dir_of(&fixture, "Volumes", "Alpha");
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let id = a["id"].as_i64().expect("an id");
    let dir = a["dirs"][0]["id"].as_i64().expect("a directory id");
    let stored = the_store(&state);

    for name in [".", ".."] {
        let (status, refused) = answered(
            &router,
            Method::POST,
            "/api/library/libraries",
            &serde_json::json!({ "name": name, "dirs": [good] }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "make {name:?}: {refused}");
        assert_eq!(the_sentence(&refused), NO_ADDRESS, "make {name:?}");

        let (status, refused) = answered(
            &router,
            Method::PATCH,
            &format!("/api/library/libraries/{id}"),
            &serde_json::json!({ "name": name }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "rename {name:?}: {refused}"
        );
        assert_eq!(the_sentence(&refused), NO_ADDRESS, "rename {name:?}");

        for dry_run in [true, false] {
            let (status, refused) = moved(
                &router,
                dir,
                dry_run,
                &serde_json::json!({ "from": id, "new_library": name }),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "move {name:?}: {refused}");
            assert_eq!(the_sentence(&refused), NO_ADDRESS, "move {name:?}");
        }
    }
    assert_eq!(the_store(&state), stored);
}

#[tokio::test]
async fn listing_split_editions_refuses_a_reader() {
    a_reader_is_refused(Method::GET, "/api/library/split-editions").await;
}

#[tokio::test]
async fn listing_split_editions_refuses_nobody() {
    an_unsigned_request_is_refused(Method::GET, "/api/library/split-editions").await;
}

/// The merged edition's fixture (spec §3.8): `raw` is part1 and part3 under
/// Manga's `m` and part2 under Kids' `k`, so it is listed with each library's
/// share; `en` and `fr`, under Manga's directories alone, are not.
#[tokio::test]
async fn an_edition_merged_from_two_libraries_is_listed_with_each_librarys_share() {
    let merged = test_support::router_over_an_edition_merged_from_two_libraries();
    let (members, roots) = {
        let conn = merged.state.library.conn();
        let members: Vec<(String, i64, i64)> = conn
            .prepare(
                "SELECT m.path, m.pages, r.library_id FROM member m
                   JOIN edition e ON e.id = m.edition_id JOIN library_root r ON r.root_id = m.root_id
                  WHERE e.key = 'raw' ORDER BY m.ordinal",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .expect("reads raw's members");
        let roots: Vec<(i64, String)> = ["m", "k"]
            .iter()
            .map(|name| {
                let root = taiyaku_library::scan::roots(&conn)
                    .expect("lists the roots")
                    .into_iter()
                    .find(|root| root.path.ends_with(name))
                    .unwrap_or_else(|| panic!("no directory {name}"));
                (root.id, root.path.display().to_string())
            })
            .collect();
        (members, roots)
    };
    let shape: Vec<(&str, i64, i64)> = members
        .iter()
        .map(|(path, pages, library)| {
            let file = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a file name");
            (file, *pages, *library)
        })
        .collect();
    assert_eq!(
        shape,
        [
            ("Foo v01 part1.cbz", 2, 70),
            ("Foo v01 part2.cbz", 3, 80),
            ("Foo v01 part3.cbz", 4, 70),
        ],
        "the fixture: Kids holds 1 member of 3 pages, Manga 2 of 6"
    );
    let under = |(root, path): &(i64, String), library: i64| {
        serde_json::json!([{
            "root": root,
            "path": path,
            "libraries": [library],
        }])
    };
    let (m, k) = (&roots[0], &roots[1]);
    let response = merged
        .get(
            test_support::Reader::Administrator,
            "/api/library/split-editions",
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed: serde_json::Value = serde_json::from_slice(response.body()).expect("JSON");
    assert_eq!(
        listed,
        serde_json::json!({ "editions": [{
            "work": merged.slug,
            "title": "Foo v01",
            "edition": "raw",
            "pages": 9,
            "members": [
                { "path": members[0].0, "pages": 2, "directories": under(m, 70) },
                { "path": members[1].0, "pages": 3, "directories": under(k, 80) },
                { "path": members[2].0, "pages": 4, "directories": under(m, 70) },
            ],
            "libraries": [
                { "id": 80, "name": "Kids", "members": 1, "pages": 3 },
                { "id": 70, "name": "Manga", "members": 2, "pages": 6 },
            ],
        }]})
    );
}

/// A byte-identical copy is one member held under a directory of each
/// library, so neither library holds only part of it.
#[tokio::test]
async fn a_copy_in_two_libraries_is_not_listed_as_split() {
    let (router, state, _fixture, _slug) =
        test_support::router_over_a_copy_granted_through_its_second_directory();
    state
        .library
        .conn()
        .execute(
            "UPDATE user SET role = 'admin', every_library = 1 WHERE id = 1",
            [],
        )
        .expect("the fixture account administers again");
    let held: (i64, Vec<String>) = {
        let conn = state.library.conn();
        let members = conn
            .query_row("SELECT count(*) FROM member", [], |r| r.get(0))
            .expect("counts the members");
        let libraries = conn
            .prepare(
                "SELECT l.name FROM member_root mr JOIN library_root r ON r.root_id = mr.root_id
                   JOIN library l ON l.id = r.library_id ORDER BY l.name",
            )
            .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
            .expect("reads where the members are held");
        (members, libraries)
    };
    assert_eq!(
        held,
        (1, vec!["A".to_string(), "B".to_string()]),
        "the fixture: a copy, one member held under a directory of each library"
    );
    assert_eq!(
        get_json(&router, "/api/library/split-editions").await,
        serde_json::json!({ "editions": [] })
    );
}

#[tokio::test]
async fn a_store_with_no_split_edition_lists_none() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    assert_eq!(
        get_json(&router, "/api/library/split-editions").await,
        serde_json::json!({ "editions": [] })
    );
}

/// Scans library `library` through its route, and answers the directories its
/// status names, in order, and those of `m` and `m/kids` whose own route holds a
/// finished walk's report, after checking that the route's `202` named the
/// directories its status then names.
async fn the_walk_of(n: &test_support::NestedLibraries, library: i64) -> (Vec<i64>, Vec<i64>) {
    let (status, started) = answered(
        &n.admin,
        Method::POST,
        &the_scan_of(library),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let reserved: Vec<i64> = started["root_ids"]
        .as_array()
        .unwrap_or_else(|| panic!("the 202 names the directories it reserved: {started}"))
        .iter()
        .map(|dir| dir.as_i64().expect("an id"))
        .collect();
    let ended = until_the_library_scan_ends(&n.admin, library).await;
    let named = ended["dirs"]
        .as_array()
        .expect("a list of directories")
        .iter()
        .map(|dir| dir["root_id"].as_i64().expect("an id"))
        .collect::<Vec<i64>>();
    assert_eq!(reserved, named, "the 202 named what the scan walks");
    let mut walked = Vec::new();
    for dir in [n.m, n.m_kids] {
        let own = get_json(&n.admin, &a_dirs_scan(dir)).await;
        if !own["report"].is_null() && keys_of(&own["report"]) == vec!["derive", "scan"] {
            walked.push(dir);
        }
    }
    (named, walked)
}

/// Spec §4's N and its decision 16: a scan of Manga walks `m` and
/// Kids' `m/kids` nested in it, each as its own directory, `m` first by path,
/// even where `m/kids` was added first and has the lower id; a scan of Kids
/// walks `m/kids` alone; with `m` excluding `kids/`, a scan of Manga walks `m`
/// alone. Each case is a fresh store, so a report found is this scan's.
#[tokio::test]
async fn a_librarys_scan_walks_every_directory_it_reaches() {
    let n = test_support::router_over_a_library_nested_in_another();
    assert_eq!(
        the_walk_of(&n, n.manga).await,
        (vec![n.m, n.m_kids], vec![n.m, n.m_kids]),
        "Manga"
    );

    let n = test_support::router_over_a_library_nested_in_another_added_first();
    assert!(
        n.m_kids < n.m,
        "the fixture: the nested directory has the lower id"
    );
    assert_eq!(
        the_walk_of(&n, n.manga).await,
        (vec![n.m, n.m_kids], vec![n.m, n.m_kids]),
        "Manga, its nested directory added first"
    );

    let n = test_support::router_over_a_library_nested_in_another();
    assert_eq!(
        the_walk_of(&n, n.kids).await,
        (vec![n.m_kids], vec![n.m_kids]),
        "Kids"
    );

    let n = test_support::router_over_a_library_nested_in_another();
    n.state
        .library
        .conn()
        .execute(
            "UPDATE root SET exclude_globs = 'kids/' WHERE id = ?1",
            [n.m],
        )
        .expect("excludes kids/ from Manga's directory");
    assert_eq!(
        the_walk_of(&n, n.manga).await,
        (vec![n.m], vec![n.m]),
        "Manga, excluding kids/"
    );
}

/// Spec §4, as `config_api`'s directory removal: a scan of a directory around
/// or inside one of a library's directories holds up the library's removal,
/// which hands its files to them. While Manga's scan of `m` is reserved, Kids
/// is neither rehearsed nor removed, nor is Manga while Kids' scan is: 409
/// naming the directory being scanned, and the store as it was.
#[tokio::test]
async fn a_library_is_not_removed_while_a_scan_around_or_inside_its_dirs_is_reserved() {
    for inner_removed in [true, false] {
        let n = test_support::router_over_a_library_nested_in_another();
        let (removed, scanning) = if inner_removed {
            (n.kids, n.m)
        } else {
            (n.manga, n.m_kids)
        };
        let path = taiyaku_library::scan::roots(&n.state.library.conn())
            .expect("roots")
            .into_iter()
            .find(|root| root.id == scanning)
            .expect("the directory being scanned")
            .path
            .display()
            .to_string();
        a_scan_running_on(&n.state, scanning);
        let stored = the_store(&n.state);

        for query in ["?dry_run=true", ""] {
            let (status, refused) = answered(
                &n.admin,
                Method::DELETE,
                &format!("/api/library/libraries/{removed}{query}"),
                &serde_json::json!({}),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::CONFLICT,
                "inner_removed={inner_removed} {query}: {refused}"
            );
            assert_eq!(
                the_sentence(&refused),
                format!("a scan of {path} is running; remove the library once it has finished")
            );
            assert_eq!(
                the_store(&n.state),
                stored,
                "inner_removed={inner_removed} {query} removed something"
            );
        }
    }
}

/// The directory `id` of library `library` in the list.
fn the_dir<'a>(listed: &'a serde_json::Value, library: &str, id: i64) -> &'a serde_json::Value {
    the_library(listed, library)["dirs"]
        .as_array()
        .expect("dirs")
        .iter()
        .find(|dir| dir["id"] == id)
        .unwrap_or_else(|| panic!("{library} lists no directory {id}: {listed}"))
}

/// The path of directory `id`, as stored.
fn path_of(state: &crate::ServeState, id: i64) -> String {
    taiyaku_library::scan::roots(&state.library.conn())
        .expect("roots")
        .into_iter()
        .find(|root| root.id == id)
        .unwrap_or_else(|| panic!("no directory {id}"))
        .path
        .to_string_lossy()
        .into_owned()
}

/// Spec §4's N, on each directory's page: Manga's `m` lists Kids' `m/kids`
/// as nested, which Manga shows, and Kids' `m/kids` lists `m` as the
/// directory it sits in, whose library shows its books. Other's `mx`, whose
/// path `m`'s spells the start of, is in neither list and sits in nothing.
#[tokio::test]
async fn the_list_says_which_directories_around_and_inside_each_govern_their_own() {
    let n = test_support::router_over_a_library_nested_in_another();
    let (m, m_kids) = (path_of(&n.state, n.m), path_of(&n.state, n.m_kids));
    let mx = {
        let dir = std::path::PathBuf::from(format!("{m}x"));
        std::fs::create_dir_all(&dir).expect("makes mx beside m");
        taiyaku_library::scan::add_root_checked_in(
            &n.state.library.conn(),
            &dir,
            taiyaku_library::scan::InLibrary::Named("Other"),
        )
        .expect("adds Other's directory beside m")
        .root
        .id
    };
    let listed = get_json(&n.admin, "/api/library/libraries").await;
    assert_eq!(
        the_dir(&listed, "Other", mx)["inside"],
        serde_json::json!([]),
        "{listed}"
    );

    let outer = the_dir(&listed, "Manga", n.m);
    assert_eq!(
        outer["nested"],
        serde_json::json!([{
            "id": n.m_kids, "path": m_kids, "libraries": ["Kids"], "shown": true, "kept_out_by": null,
        }]),
        "{listed}"
    );
    assert_eq!(outer["inside"], serde_json::json!([]), "{listed}");

    let inner = the_dir(&listed, "Kids", n.m_kids);
    assert_eq!(inner["nested"], serde_json::json!([]), "{listed}");
    assert_eq!(
        inner["inside"],
        serde_json::json!([{ "path": m, "libraries": ["Manga"], "shows": true, "kept_out_by": null }]),
        "{listed}"
    );
}

/// Reach is asked of the outer directory's own patterns: with `m` excluding
/// `kids/`, Manga does not show `m/kids`, and each side says so.
#[tokio::test]
async fn an_outer_directory_that_excludes_the_nested_one_neither_shows_it_nor_is_said_to() {
    let n = test_support::router_over_a_library_nested_in_another();
    n.state
        .library
        .conn()
        .execute(
            "UPDATE root SET exclude_globs = 'kids/' WHERE id = ?1",
            [n.m],
        )
        .expect("excludes kids/ from Manga's directory");
    let listed = get_json(&n.admin, "/api/library/libraries").await;

    assert_eq!(
        the_dir(&listed, "Manga", n.m)["nested"][0]["shown"],
        false,
        "{listed}"
    );
    assert_eq!(
        the_dir(&listed, "Kids", n.m_kids)["inside"][0]["shows"],
        false,
        "{listed}"
    );
    assert_eq!(
        the_dir(&listed, "Manga", n.m)["nested"][0]["kept_out_by"],
        "exclusion",
        "{listed}"
    );
    assert_eq!(
        the_dir(&listed, "Kids", n.m_kids)["inside"][0]["kept_out_by"],
        "exclusion",
        "{listed}"
    );
}

/// Two deep: Tiny's `m/kids/tiny` sits in both `m` and `m/kids`, outermost
/// first even where `m/kids` was added first and has the lower id, and each
/// of them lists it as nested. Reach does not chain, so whether Manga shows it
/// is `m`'s patterns alone: with `m/kids` excluding `tiny/`, Manga still shows
/// it and Kids does not.
#[tokio::test]
async fn a_directory_two_deep_is_inside_both_directories_around_it() {
    let n = test_support::router_over_a_library_nested_in_another_added_first();
    assert!(
        n.m_kids < n.m,
        "the fixture: the nested directory has the lower id"
    );
    let (m, m_kids) = (path_of(&n.state, n.m), path_of(&n.state, n.m_kids));
    let tiny = {
        let conn = n.state.library.conn();
        let dir = std::path::Path::new(&m_kids).join("tiny");
        std::fs::create_dir_all(&dir).expect("makes m/kids/tiny");
        let added = taiyaku_library::scan::add_root_checked_in(
            &conn,
            &dir,
            taiyaku_library::scan::InLibrary::Named("Tiny"),
        )
        .expect("adds Tiny's directory inside both");
        conn.execute(
            "UPDATE root SET exclude_globs = 'tiny/' WHERE id = ?1",
            [n.m_kids],
        )
        .expect("excludes tiny/ from Kids' directory");
        added.root.id
    };
    let tiny_path = path_of(&n.state, tiny);
    let listed = get_json(&n.admin, "/api/library/libraries").await;

    assert_eq!(
        the_dir(&listed, "Tiny", tiny)["inside"],
        serde_json::json!([
            { "path": m, "libraries": ["Manga"], "shows": true, "kept_out_by": null },
            { "path": m_kids, "libraries": ["Kids"], "shows": false, "kept_out_by": "exclusion" },
        ]),
        "{listed}"
    );
    assert_eq!(
        the_dir(&listed, "Manga", n.m)["nested"],
        serde_json::json!([
            { "id": n.m_kids, "path": m_kids, "libraries": ["Kids"], "shown": true, "kept_out_by": null },
            { "id": tiny, "path": tiny_path, "libraries": ["Tiny"], "shown": true, "kept_out_by": null },
        ]),
        "{listed}"
    );
    assert_eq!(
        the_dir(&listed, "Kids", n.m_kids)["nested"],
        serde_json::json!([
            { "id": tiny, "path": tiny_path, "libraries": ["Tiny"], "shown": false, "kept_out_by": "exclusion" },
        ]),
        "{listed}"
    );
}

/// The walk skips a name starting with `.`, whatever the patterns say, so
/// Manga does not show Hidden's `m/.stash/hidden`, and each side says that
/// rule is why, not an exclusion `m` does not have.
#[tokio::test]
async fn a_directory_under_a_dot_name_is_kept_out_by_the_dot_rule_not_a_pattern() {
    let n = test_support::router_over_a_library_nested_in_another();
    let hidden = {
        let dir = std::path::Path::new(&path_of(&n.state, n.m))
            .join(".stash")
            .join("hidden");
        std::fs::create_dir_all(&dir).expect("makes m/.stash/hidden");
        taiyaku_library::scan::add_root_checked_in(
            &n.state.library.conn(),
            &dir,
            taiyaku_library::scan::InLibrary::Named("Hidden"),
        )
        .expect("adds Hidden's directory under a dot name in m")
        .root
        .id
    };
    let listed = get_json(&n.admin, "/api/library/libraries").await;

    let nested = the_dir(&listed, "Manga", n.m)["nested"]
        .as_array()
        .expect("nested")
        .iter()
        .find(|one| one["id"] == hidden)
        .unwrap_or_else(|| panic!("m lists Hidden's directory: {listed}"))
        .clone();
    assert_eq!(nested["shown"], false, "{listed}");
    assert_eq!(nested["kept_out_by"], "dot_name", "{listed}");
    let inside = &the_dir(&listed, "Hidden", hidden)["inside"][0];
    assert_eq!(inside["shows"], false, "{listed}");
    assert_eq!(inside["kept_out_by"], "dot_name", "{listed}");
}

/// The server's rule for a removal: a scan of a directory around or inside
/// one holds its removal up. While Kids' `m/kids` is scanned, Manga's status
/// names `m`'s removal as held, and Kids' names none, its own scan being its
/// own status's to say; while Manga's `m` is scanned, the other way round.
#[tokio::test]
async fn a_librarys_status_names_the_removals_a_scan_around_or_inside_holds() {
    for (scanning, held_in, free_in) in [(true, "Manga", "Kids"), (false, "Kids", "Manga")] {
        let n = test_support::router_over_a_library_nested_in_another();
        let (dir, held, manga_held, kids_held) = if scanning {
            (n.m_kids, n.m, vec![n.m], Vec::<i64>::new())
        } else {
            (n.m, n.m_kids, Vec::<i64>::new(), vec![n.m_kids])
        };
        a_scan_running_on(&n.state, dir);
        let manga = get_json(&n.admin, &the_scan_of(n.manga)).await;
        let kids = get_json(&n.admin, &the_scan_of(n.kids)).await;
        assert_eq!(
            manga["held_removals"],
            serde_json::json!(manga_held),
            "{held_in} holds {held}, {free_in} nothing: {manga}"
        );
        assert_eq!(
            kids["held_removals"],
            serde_json::json!(kids_held),
            "{held_in} holds {held}, {free_in} nothing: {kids}"
        );
    }
}

/// Which rule the walk meets first is the reason given. With `.stash/`
/// excluded at `m`, `m/.stash/kids` is kept out by its dot name, met first;
/// with `old/` excluded, `m/old/.kids` is kept out by the exclusion.
#[tokio::test]
async fn the_reason_a_directory_is_kept_out_is_the_rule_the_walk_meets_first() {
    for (pattern, rel, expected) in [
        (".stash/", ".stash/kids", "dot_name"),
        ("old/", "old/.kids", "exclusion"),
    ] {
        let n = test_support::router_over_a_library_nested_in_another();
        let dir = std::path::Path::new(&path_of(&n.state, n.m)).join(rel);
        let inner = {
            let conn = n.state.library.conn();
            conn.execute(
                "UPDATE root SET exclude_globs = ?1 WHERE id = ?2",
                rusqlite::params![pattern, n.m],
            )
            .expect("excludes the pattern from Manga's directory");
            std::fs::create_dir_all(&dir).expect("makes the inner directory");
            taiyaku_library::scan::add_root_checked_in(
                &conn,
                &dir,
                taiyaku_library::scan::InLibrary::Named("Inner"),
            )
            .expect("adds Inner's directory in m")
            .root
            .id
        };
        let listed = get_json(&n.admin, "/api/library/libraries").await;
        let nested = the_dir(&listed, "Manga", n.m)["nested"]
            .as_array()
            .expect("nested")
            .iter()
            .find(|one| one["id"] == inner)
            .unwrap_or_else(|| panic!("m lists Inner's directory: {listed}"))
            .clone();
        assert_eq!(nested["kept_out_by"], expected, "{pattern} {rel}: {listed}");
        assert_eq!(
            the_dir(&listed, "Inner", inner)["inside"][0]["kept_out_by"],
            expected,
            "{pattern} {rel}: {listed}"
        );
    }
}

/// A library's status names a scan another library asked for by that
/// library: Kids' Scan library walks `m/kids` and Tiny's `m/kids/tiny`, and
/// Manga, which shows both, is told Kids is scanning them. A Scan directory
/// of `m/kids` is named by Kids too, the directory's own library.
#[tokio::test]
async fn a_librarys_status_names_each_scan_by_the_library_that_asked_for_it() {
    let n = test_support::router_over_a_library_nested_in_another();
    let dir = std::path::Path::new(&path_of(&n.state, n.m_kids)).join("tiny");
    let tiny = {
        let conn = n.state.library.conn();
        std::fs::create_dir_all(&dir).expect("makes m/kids/tiny");
        taiyaku_library::scan::add_root_checked_in(
            &conn,
            &dir,
            taiyaku_library::scan::InLibrary::Named("Tiny"),
        )
        .expect("adds Tiny's directory inside both")
        .root
        .id
    };
    let (held, job) = a_waiting_scan(&n.state, n.kids);
    let manga = get_json(&n.admin, &the_scan_of(n.manga)).await;
    assert_eq!(
        manga["scans"],
        serde_json::json!([{ "library_id": n.kids, "library": "Kids", "root_ids": [n.m_kids, tiny] }]),
        "{manga}"
    );
    drop(held);
    job.await.expect("the scan ends");

    let held = holding_the_job(&n.state);
    let (status, started) = answered(
        &n.admin,
        Method::POST,
        &a_dirs_scan(n.m_kids),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let manga = get_json(&n.admin, &the_scan_of(n.manga)).await;
    assert_eq!(
        manga["scans"],
        serde_json::json!([{ "library_id": n.kids, "library": "Kids", "root_ids": [n.m_kids] }]),
        "{manga}"
    );
    drop(held);
    until_the_library_scan_ends(&n.admin, n.manga).await;
}

/// A move waits for nothing: it is refused while anything holds the job lock,
/// a running scan of any library among them, so a library's status says so
/// for its pages to hold Move…. A scan queued for the lock is queued behind
/// whatever holds it, so the status says so then too; once the lock is free,
/// and the scan behind it has run, it does not.
#[tokio::test]
async fn a_librarys_status_says_moves_are_held_while_the_job_lock_is() {
    let n = test_support::router_over_a_library_nested_in_another();
    let moves_held = |status: &serde_json::Value| status["moves_held"].clone();
    assert_eq!(
        moves_held(&get_json(&n.admin, &the_scan_of(n.manga)).await),
        false
    );

    let (held, job) = a_waiting_scan(&n.state, n.kids);
    let manga = get_json(&n.admin, &the_scan_of(n.manga)).await;
    assert_eq!(moves_held(&manga), true, "{manga}");
    let (status, refused) = answered(
        &n.admin,
        Method::POST,
        &format!("/api/library/roots/{}/move", n.m),
        &serde_json::json!({ "from": n.manga, "new_library": "Elsewhere" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the move is refused: {refused}"
    );

    drop(held);
    job.await.expect("the scan ends");
    let manga = get_json(&n.admin, &the_scan_of(n.manga)).await;
    assert_eq!(moves_held(&manga), false, "{manga}");
}

/// Overlapping-libraries spec §5: a directory two libraries hold is listed
/// under each, and each listing names every library holding it, ascending by
/// id. New library over root-a's directory links it, and its `added` entry
/// names root-a as the add's answer does, unprobed.
#[tokio::test]
async fn a_dir_two_libraries_hold_is_listed_under_each_naming_both() {
    let (router, _state, _held) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let (a_id, held) = (a["id"].as_i64().expect("an id"), a["dirs"][0].clone());

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "New", "dirs": [held["path"]] }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{made}");
    let added = &made["added"][0];
    assert_eq!(added["also_in"], serde_json::json!(["root-a"]), "{made}");
    for unprobed in ["watching", "watch_reason", "interval_secs"] {
        assert!(added[unprobed].is_null(), "{unprobed}: {made}");
    }
    assert_eq!(added["root"]["id"], held["id"], "{made}");
    let new_id = made["library"]["id"].as_i64().expect("an id");
    let both = serde_json::json!([
        { "id": a_id, "name": "root-a" },
        { "id": new_id, "name": "New" },
    ]);
    let listed = get_json(&router, "/api/library/libraries").await;
    for library in ["root-a", "New"] {
        let dirs = the_library(&listed, library)["dirs"]
            .as_array()
            .expect("dirs");
        assert_eq!(dirs.len(), 1, "{library}: {listed}");
        assert_eq!(dirs[0]["id"], held["id"], "{library}: {listed}");
        assert_eq!(dirs[0]["libraries"], both, "{library}: {listed}");
    }
    assert_eq!(
        the_library(&listed, "root-b")["dirs"][0]["libraries"]
            .as_array()
            .expect("libraries")
            .len(),
        1,
        "{listed}"
    );
}

/// Each directory around or inside a directory's own is named with every
/// library holding it: Kids' `m/kids` linked into Other, and Manga's `m`
/// into Outer, through the add.
#[tokio::test]
async fn the_nesting_lists_name_every_library_holding_a_directory() {
    let n = test_support::router_over_a_library_nested_in_another();
    let (m, m_kids) = (path_of(&n.state, n.m), path_of(&n.state, n.m_kids));
    for (name, dir) in [("Other", &m_kids), ("Outer", &m)] {
        let (status, made) = answered(
            &n.admin,
            Method::POST,
            "/api/library/libraries",
            &serde_json::json!({ "name": name, "dirs": [dir] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}: {made}");
        assert_eq!(
            made["added"][0]["also_in"].as_array().map(Vec::len),
            Some(1)
        );
    }

    let listed = get_json(&n.admin, "/api/library/libraries").await;

    assert_eq!(
        the_dir(&listed, "Manga", n.m)["nested"],
        serde_json::json!([{
            "id": n.m_kids, "path": m_kids, "libraries": ["Kids", "Other"], "shown": true,
            "kept_out_by": null,
        }]),
        "{listed}"
    );
    assert_eq!(
        the_dir(&listed, "Other", n.m_kids)["inside"],
        serde_json::json!([{
            "path": m, "libraries": ["Manga", "Outer"], "shows": true, "kept_out_by": null,
        }]),
        "{listed}"
    );
}

/// A library sharing a directory with another is not removed while a scan
/// of that directory is reserved, as for a directory of its own: 409 naming
/// it. Kids links root-a's directory, whose lowest link is root-a's library.
#[tokio::test]
async fn a_library_is_not_removed_while_a_scan_of_a_dir_it_shares_is_reserved() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let held = the_library(&listed, "root-a")["dirs"][0].clone();
    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Kids", "dirs": [held["path"]] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let kids = made["library"]["id"].as_i64().expect("an id");
    a_scan_running_on(&state, held["id"].as_i64().expect("an id"));
    let stored = the_store(&state);

    let (status, refused) = answered(
        &router,
        Method::DELETE,
        &format!("/api/library/libraries/{kids}"),
        &serde_json::json!({}),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        the_sentence(&refused),
        format!(
            "a scan of {} is running; remove the library once it has finished",
            held["path"].as_str().expect("a path")
        )
    );
    assert_eq!(the_store(&state), stored);
}

/// A directory in no library, which no writer leaves, is a fault where the
/// list would name its libraries, not a directory drawn as no library's: here
/// one planted by SQL inside Manga's `m`, which `m`'s `nested` would list.
#[tokio::test]
async fn a_directory_in_no_library_fails_the_list_where_it_would_be_named() {
    let n = test_support::router_over_a_library_nested_in_another();
    let planted = format!("{}/planted", path_of(&n.state, n.m));
    n.state
        .library
        .conn()
        .execute("INSERT INTO root (path) VALUES (?1)", [&planted])
        .expect("plants a directory in no library");

    let (status, failed) = answered(
        &n.admin,
        Method::GET,
        "/api/library/libraries",
        &serde_json::json!({}),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{failed}");
    assert_eq!(
        the_sentence(&failed),
        format!("directory {planted} is in no library")
    );
}

#[tokio::test]
async fn unlinking_a_dir_refuses_a_reader() {
    a_reader_is_refused(Method::DELETE, "/api/library/libraries/1/dirs/1").await;
}

#[tokio::test]
async fn rehearsing_an_unlink_refuses_a_reader() {
    a_reader_is_refused(
        Method::DELETE,
        "/api/library/libraries/1/dirs/1?dry_run=true",
    )
    .await;
}

#[tokio::test]
async fn unlinking_a_dir_refuses_nobody() {
    an_unsigned_request_is_refused(Method::DELETE, "/api/library/libraries/1/dirs/1").await;
}

/// Kids, made over root-a's directory and one of its own; root-a holds the
/// first too. Answers Kids' id and the two directories as listed.
async fn kids_sharing_root_as_dir(
    router: &axum::Router,
    fixture: &test_support::Fixture,
) -> (i64, serde_json::Value, serde_json::Value) {
    let listed = get_json(router, "/api/library/libraries").await;
    let shared = the_library(&listed, "root-a")["dirs"][0].clone();
    let own = a_dir_of(fixture, "Own", "Gamma");
    let (status, made) = answered(
        router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Kids", "dirs": [shared["path"], own] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let kids = made["library"]["id"].as_i64().expect("an id");
    let listed = get_json(router, "/api/library/libraries").await;
    let own = the_library(&listed, "Kids")["dirs"]
        .as_array()
        .expect("dirs")
        .iter()
        .find(|dir| dir["id"] != shared["id"])
        .expect("Kids' own directory")
        .clone();
    (kids, shared, own)
}

fn unlink_path(library: i64, dir: &serde_json::Value, dry_run: bool) -> String {
    let query = if dry_run { "?dry_run=true" } else { "" };
    format!("/api/library/libraries/{library}/dirs/{}{query}", dir["id"])
}

/// `DELETE /api/library/libraries/{id}/dirs/{dir}`, Remove from this library,
/// unlinks: the rehearsal answers in `relocate::Moved`'s names who loses
/// sight, and writes nothing; the unlink answers the same, takes root-a's
/// directory out of Kids alone, and deletes nothing.
#[tokio::test]
async fn a_dir_is_rehearsed_and_then_unlinked_from_one_library() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    {
        let conn = state.library.conn();
        let kid = taiyaku_library::account::add(&conn, "kid", "pw", "user").expect("adds");
        taiyaku_library::account::grant_library(&conn, kid, kids).expect("grants");
    }
    let (roots, works, links) = roots_works_and_links(&state);
    let stored = the_store(&state);
    let said = serde_json::json!({
        "loses_sight": ["kid"],
        "library_removed": null,
        "grants_removed": 0,
        "left_with_no_library": [],
    });

    let (status, rehearsed) = answered(
        &router,
        Method::DELETE,
        &unlink_path(kids, &shared, true),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(keys_of(&rehearsed), ["dry_run", "unlinked"], "{rehearsed}");
    assert_eq!(rehearsed["dry_run"], true);
    assert_eq!(rehearsed["unlinked"], said, "{rehearsed}");
    assert_eq!(the_store(&state), stored, "a rehearsal writes nothing");

    let (status, unlinked) = answered(
        &router,
        Method::DELETE,
        &unlink_path(kids, &shared, false),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{unlinked}");
    assert_eq!(unlinked["dry_run"], false);
    assert_eq!(unlinked["unlinked"], said, "{unlinked}");
    let listed = get_json(&router, "/api/library/libraries").await;
    let ids = |library: &str| -> Vec<serde_json::Value> {
        the_library(&listed, library)["dirs"]
            .as_array()
            .expect("dirs")
            .iter()
            .map(|dir| dir["id"].clone())
            .collect()
    };
    assert_eq!(ids("Kids"), vec![own["id"].clone()], "{listed}");
    assert_eq!(ids("root-a"), vec![shared["id"].clone()], "{listed}");
    assert_eq!(
        roots_works_and_links(&state),
        (roots, works, links - 1),
        "every directory and work stands, with one link fewer"
    );
}

fn roots_works_and_links(state: &crate::ServeState) -> (i64, i64, i64) {
    let conn = state.library.conn();
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .expect("counts")
    };
    (count("root"), count("work"), count("library_root"))
}

/// The unlink's refusals, each writing nothing: a directory's last link is the
/// caller's mistake, naming Remove this directory; a library that does not
/// hold the directory, and a directory or library that does not exist, are
/// 404; an id that is not a number is 400.
#[tokio::test]
async fn an_unlink_is_refused_on_a_last_link_and_where_the_library_does_not_hold_the_dir() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let b = the_library(&listed, "root-b");
    let a_id = a["id"].as_i64().expect("an id");
    let b_dir = &b["dirs"][0];
    let b_path = b_dir["path"].as_str().expect("a path");
    let stored = the_store(&state);

    for dry_run in [true, false] {
        for (path, wanted, sentence) in [
            (
                unlink_path(b["id"].as_i64().expect("an id"), b_dir, dry_run),
                StatusCode::BAD_REQUEST,
                format!(
                    "`{b_path}` is in no other library, so it can only be removed: use Remove \
                     this directory."
                ),
            ),
            (
                unlink_path(a_id, b_dir, dry_run),
                StatusCode::NOT_FOUND,
                format!("`{b_path}` is not in root-a"),
            ),
            (
                unlink_path(a_id, &serde_json::json!({ "id": 999 }), dry_run),
                StatusCode::NOT_FOUND,
                "there is no directory 999".to_string(),
            ),
            (
                unlink_path(999, b_dir, dry_run),
                StatusCode::NOT_FOUND,
                "there is no library 999".to_string(),
            ),
            (
                format!(
                    "/api/library/libraries/{a_id}/dirs/x{}",
                    if dry_run { "?dry_run=true" } else { "" }
                ),
                StatusCode::BAD_REQUEST,
                "`x` is not a directory id; an id is a whole number".to_string(),
            ),
        ] {
            let (status, refused) =
                answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
            assert_eq!(status, wanted, "{path}: {refused}");
            assert_eq!(the_sentence(&refused), sentence, "{path}");
        }
    }
    assert_eq!(the_store(&state), stored);
}

/// Remove from this library refuses as Move does: 409 while the store's job
/// lock is held, and while a scan is reserved on a directory this library
/// holds, the shared one or its own, for the rehearsal too. A scan reserved on
/// a directory only root-a holds does not refuse it: root-a keeps the
/// directory, and its scan walks what it walked.
#[tokio::test]
async fn an_unlink_is_refused_while_the_job_lock_is_held_or_a_scan_is_reserved_in_the_library() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let root_a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let elsewhere = add_dir_to(&router, root_a, &a_dir_of(&fixture, "Elsewhere", "Delta")).await;
    let stored = the_store(&state);

    let held = holding_the_job(&state);
    for dry_run in [true, false] {
        let (status, refused) = answered(
            &router,
            Method::DELETE,
            &unlink_path(kids, &shared, dry_run),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(
            the_sentence(&refused),
            "a scan or another library job is running; remove the directory from this library \
             once it has finished"
        );
    }
    drop(held);

    for reserved in [&shared, &own] {
        let id = reserved["id"].as_i64().expect("an id");
        a_scan_running_on(&state, id);
        for dry_run in [true, false] {
            let (status, refused) = answered(
                &router,
                Method::DELETE,
                &unlink_path(kids, &shared, dry_run),
                &serde_json::json!({}),
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{id}: {refused}");
            assert_eq!(
                the_sentence(&refused),
                format!(
                    "a scan of {} is running; remove the directory from this library once it \
                     has finished",
                    reserved["path"].as_str().expect("a path")
                )
            );
            assert!(
                state
                    .scans
                    .lock()
                    .unwrap()
                    .get(&id)
                    .is_some_and(|p| !p.done),
                "the reservation on {id} was taken away"
            );
        }
        state.scans.lock().unwrap().remove(&id);
    }
    assert_eq!(the_store(&state), stored);

    a_scan_running_on(&state, elsewhere);
    let (status, rehearsed) = answered(
        &router,
        Method::DELETE,
        &unlink_path(kids, &shared, true),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a scan of root-a's other directory: {rehearsed}"
    );
}

/// Removing a library leaves a directory another library holds to it: Kids
/// links root-a's directory and has one of its own. The answer names the
/// shared directory and root-a under `kept`, Kids under `libraries_removed`,
/// and counts only the other's work. The shared directory's finished report,
/// from root-a's scan, keeps its slot; the removed directory's slot is cleared.
#[tokio::test]
async fn removing_a_library_leaves_a_shared_dir_to_the_other_library_with_its_scan_report() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let own_id = own["id"].as_i64().expect("an id");
    let (status, started) = answered(
        &router,
        Method::POST,
        &a_dirs_scan(own_id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    until_the_dirs_scan_ends(&router, own_id).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let root_a = the_library(&listed, "root-a").clone();
    let report = serde_json::json!({ "scanned": "by root-a" });
    for slot in [&shared, &own] {
        state.scans.lock().unwrap().insert(
            slot["id"].as_i64().expect("an id"),
            crate::ScanProgress {
                waiting: false,
                started: std::time::Instant::now(),
                done: true,
                report: Some(report.clone()),
                library: root_a["id"].as_i64(),
            },
        );
    }
    let path = format!("/api/library/libraries/{kids}");
    let said = serde_json::json!({
        "works": 1,
        "positions": 0,
        "notes": 0,
        "positions_page_gone": 0,
        "notes_page_gone": 0,
        "left_with_no_library": [],
        "libraries_removed": ["Kids"],
        "kept": [{ "path": shared["path"], "libraries": ["root-a"] }],
    });

    let (status, rehearsed) = answered(
        &router,
        Method::DELETE,
        &format!("{path}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(rehearsed["removed"], said, "{rehearsed}");

    let (status, removed) = answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["removed"], said, "{removed}");
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        after["libraries"]
            .as_array()
            .expect("libraries")
            .iter()
            .map(|library| library["name"].clone())
            .collect::<Vec<_>>(),
        ["root-a", "root-b"],
        "{after}"
    );
    let root_a_after = the_library(&after, "root-a");
    assert_eq!(root_a_after["works"], root_a["works"], "{after}");
    assert_eq!(
        root_a_after["dirs"][0]["libraries"],
        serde_json::json!([{ "id": root_a["id"], "name": "root-a" }]),
        "root-a keeps the directory, now its alone: {after}"
    );
    let scans = state.scans.lock().unwrap();
    assert_eq!(
        scans
            .get(&shared["id"].as_i64().expect("an id"))
            .and_then(|progress| progress.report.clone()),
        Some(report),
        "the shared directory keeps its slot and its report"
    );
    assert!(
        !scans.contains_key(&own["id"].as_i64().expect("an id")),
        "the removed directory's id can be given to the next one added"
    );
}

/// Each account in `accounts`, limited to the libraries listed with it.
fn accounts_granted(state: &crate::ServeState, accounts: &[(&str, &[i64])]) {
    let conn = state.library.conn();
    for (name, granted) in accounts {
        let account = taiyaku_library::account::add(&conn, name, "pw", "user").expect("adds");
        for library in *granted {
            taiyaku_library::account::grant_library(&conn, account, *library).expect("grants");
        }
    }
}

/// The ids of the directories library `name` lists.
fn dir_ids_of(listed: &serde_json::Value, name: &str) -> Vec<serde_json::Value> {
    the_library(listed, name)["dirs"]
        .as_array()
        .expect("dirs")
        .iter()
        .map(|dir| dir["id"].clone())
        .collect()
}

/// Overlapping-libraries spec §5: a move takes a shared directory out of
/// `from` and no other library. Moved from Kids to root-b, root-a keeps
/// root-a's directory, and only `kid`, who saw it through Kids alone, loses it.
#[tokio::test]
async fn a_shared_dir_moved_from_one_library_stays_in_the_other() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let id_of = |name: &str| the_library(&listed, name)["id"].as_i64().expect("an id");
    let (root_a, root_b) = (id_of("root-a"), id_of("root-b"));
    accounts_granted(
        &state,
        &[
            ("kid", &[kids]),
            ("fan", &[root_a]),
            ("both", &[kids, root_a]),
            ("bee", &[root_b]),
        ],
    );
    let dir = shared["id"].as_i64().expect("an id");
    let to = serde_json::json!({ "from": kids, "library": root_b });
    let stored = the_store(&state);

    let (status, rehearsed) = moved(&router, dir, true, &to).await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(the_store(&state), stored, "the rehearsal changed the store");
    let (status, done) = moved(&router, dir, false, &to).await;

    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["moved"], rehearsed["moved"]);
    assert_eq!(
        done["moved"],
        serde_json::json!({
            "library_id": root_b,
            "library": "root-b",
            "gains_sight": ["bee"],
            "loses_sight": ["kid"],
            "library_removed": null,
            "grants_removed": 0,
            "left_with_no_library": [],
        })
    );
    let after = get_json(&router, "/api/library/libraries").await;
    assert!(
        dir_ids_of(&after, "root-a").contains(&shared["id"]),
        "{after}"
    );
    assert!(
        dir_ids_of(&after, "root-b").contains(&shared["id"]),
        "{after}"
    );
    assert_eq!(dir_ids_of(&after, "Kids"), [own["id"].clone()], "{after}");
}

/// Decision 25: Move to a library that already holds the directory is
/// refused, 400, in its words, by the rehearsal and the move, and nothing is
/// written. The library it leaves is such a library too.
#[tokio::test]
async fn a_move_to_a_library_that_holds_the_dir_is_refused_in_decision_25s_words() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, _) = kids_sharing_root_as_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let root_a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let dir = shared["id"].as_i64().expect("an id");
    let path = shared["path"].as_str().expect("a path");
    let stored = the_store(&state);

    for (from, to, name) in [
        (kids, root_a, "root-a"),
        (root_a, kids, "Kids"),
        (kids, kids, "Kids"),
    ] {
        for dry_run in [true, false] {
            let body = serde_json::json!({ "from": from, "library": to });
            let (status, refused) = moved(&router, dir, dry_run, &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {refused}");
            assert_eq!(
                the_sentence(&refused),
                format!("`{path}` is already in {name}"),
                "{body}"
            );
        }
    }
    assert_eq!(the_store(&state), stored);
}

/// A move waits for the scans of `from` and of the target, and no other:
/// moving root-a's directory from Kids to root-b waits for a scan reserved on
/// Kids' own directory or on root-b's, not for one on a directory root-a
/// alone holds, though root-a holds the moving directory too.
#[tokio::test]
async fn a_move_waits_for_the_scans_of_the_library_it_leaves_and_the_one_it_joins() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let id_of = |name: &str| the_library(&listed, name)["id"].as_i64().expect("an id");
    let (root_a, root_b) = (id_of("root-a"), id_of("root-b"));
    let b_dir = the_library(&listed, "root-b")["dirs"][0].clone();
    let elsewhere = add_dir_to(&router, root_a, &a_dir_of(&fixture, "Elsewhere", "Delta")).await;
    let dir = shared["id"].as_i64().expect("an id");
    let to = serde_json::json!({ "from": kids, "library": root_b });

    for reserved in [&own, &b_dir] {
        let id = reserved["id"].as_i64().expect("an id");
        a_scan_running_on(&state, id);
        let (status, refused) = moved(&router, dir, true, &to).await;
        assert_eq!(status, StatusCode::CONFLICT, "{id}: {refused}");
        assert_eq!(
            refused["error"],
            format!(
                "a scan of {} is running; move the directory once it has finished",
                reserved["path"].as_str().expect("a path")
            )
        );
        state.scans.lock().unwrap().remove(&id);
    }

    a_scan_running_on(&state, elsewhere);
    let (status, rehearsed) = moved(&router, dir, true, &to).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a scan of root-a's other directory: {rehearsed}"
    );
}

/// Decision 27: a directory scan is named by the library whose page started
/// it. Root-a's directory, which Kids holds too, is scanned for root-a and
/// then for Kids, and each is named so on Kids' status; scanned for none, it
/// is no library's. Kids' own directory, scanned for none, is Kids'.
#[tokio::test]
async fn a_dir_scan_is_named_by_the_library_it_was_started_from() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let root_a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");

    for (dir, query, by_id, by) in [
        (
            &shared,
            format!("?library={root_a}"),
            serde_json::json!(root_a),
            serde_json::json!("root-a"),
        ),
        (
            &shared,
            format!("?library={kids}"),
            serde_json::json!(kids),
            serde_json::json!("Kids"),
        ),
        (
            &shared,
            String::new(),
            serde_json::Value::Null,
            serde_json::Value::Null,
        ),
        (
            &own,
            String::new(),
            serde_json::json!(kids),
            serde_json::json!("Kids"),
        ),
    ] {
        let id = dir["id"].as_i64().expect("an id");
        let held = holding_the_job(&state);
        let (status, started) = answered(
            &router,
            Method::POST,
            &format!("{}{query}", a_dirs_scan(id)),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{id}{query}: {started}");
        let status = get_json(&router, &the_scan_of(kids)).await;
        assert_eq!(
            status["scans"],
            serde_json::json!([{ "library_id": by_id, "library": by, "root_ids": [id] }]),
            "{id}{query}: {status}"
        );
        drop(held);
        until_the_library_scan_ends(&router, kids).await;
    }
}

/// O254: a scan Kids asked for of root-a's directory, which Kids held when the
/// scan was reserved, is named by no library once Kids no longer reaches the
/// directory: moved out of Kids into Teens, or Kids removed. Each change goes
/// through the store, as a request committing between the scan's check and
/// its reservation would; the route refuses it once the scan is reserved.
#[tokio::test]
async fn a_scan_is_named_by_no_library_once_its_library_no_longer_reaches_the_directory() {
    for case in ["moved", "removed"] {
        let (router, state, fixture) = test_support::state_with_two_scanned_roots();
        let (kids, shared, _) = kids_sharing_root_as_dir(&router, &fixture).await;
        let listed = get_json(&router, "/api/library/libraries").await;
        let root_a = the_library(&listed, "root-a")["id"]
            .as_i64()
            .expect("an id");
        let dir = shared["id"].as_i64().expect("an id");
        let held = holding_the_job(&state);
        let (status, started) = answered(
            &router,
            Method::POST,
            &format!("{}?library={kids}", a_dirs_scan(dir)),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{case}: {started}");
        let before = get_json(&router, &the_scan_of(root_a)).await;
        assert_eq!(
            before["scans"],
            serde_json::json!([{ "library_id": kids, "library": "Kids", "root_ids": [dir] }]),
            "{case}, before: {before}"
        );
        let teens = {
            let conn = state.library.conn();
            if case == "moved" {
                let moved = taiyaku_library::relocate::move_root(
                    &conn,
                    dir,
                    kids,
                    taiyaku_library::relocate::Target::New("Teens"),
                )
                .expect("moves root-a's directory out of Kids");
                Some(moved.library_id)
            } else {
                taiyaku_library::scan::remove_library(&conn, kids).expect("removes Kids");
                None
            }
        };

        for library in std::iter::once(root_a).chain(teens) {
            let status = get_json(&router, &the_scan_of(library)).await;
            assert_eq!(
                status["scans"],
                serde_json::json!([{ "library_id": null, "library": null, "root_ids": [dir] }]),
                "{case}, library {library}: {status}"
            );
        }
        drop(held);
        until_the_library_scan_ends(&router, root_a).await;
    }
}

/// O254: a library removed after the status read the directories and before
/// it read the names joins the one group no library is named by, its id with
/// its name. The handler is held at the scan registry, which it asks between
/// the two reads, while the test removes Kids. Load can make this miss that
/// defect, should the handler reach the first read only after the removal. It
/// cannot fail correct code.
#[tokio::test]
async fn a_scan_is_named_by_no_library_when_its_library_goes_between_the_status_reads() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, _) = kids_sharing_root_as_dir(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let root_a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let dir = shared["id"].as_i64().expect("an id");
    let held = holding_the_job(&state);
    let (status, started) = answered(
        &router,
        Method::POST,
        &format!("{}?library={kids}", a_dirs_scan(dir)),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let before = get_json(&router, &the_scan_of(root_a)).await;
    assert_eq!(
        before["scans"],
        serde_json::json!([{ "library_id": kids, "library": "Kids", "root_ids": [dir] }]),
        "before: {before}"
    );

    let (release, released) = std::sync::mpsc::channel::<()>();
    let (locked_tx, locked) = std::sync::mpsc::channel::<()>();
    let holder = {
        let state = std::sync::Arc::clone(&state);
        std::thread::spawn(move || {
            let _registry = state.scans.lock().unwrap();
            locked_tx.send(()).expect("the test is waiting");
            let _ = released.recv();
        })
    };
    locked.recv().expect("the holder took the registry");
    let reading = tokio::spawn({
        let router = router.clone();
        async move { get_json(&router, &the_scan_of(root_a)).await }
    });
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    taiyaku_library::scan::remove_library(&state.library.conn(), kids)
        .expect("removes Kids meanwhile");
    drop(release);
    holder.join().expect("the holder ends");
    let status = reading.await.expect("the status answers");

    assert_eq!(
        status["scans"],
        serde_json::json!([{ "library_id": null, "library": null, "root_ids": [dir] }]),
        "{status}"
    );
    drop(held);
    until_the_library_scan_ends(&router, root_a).await;
}

/// O254: a scan's library is sent by its id, which a page compares with its
/// own, and by the name it has when the status is read, which a page shows.
#[tokio::test]
async fn a_scans_library_is_sent_by_its_id_and_the_name_it_has_now() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, _) = kids_sharing_root_as_dir(&router, &fixture).await;
    let dir = shared["id"].as_i64().expect("an id");
    let held = holding_the_job(&state);
    let (status, started) = answered(
        &router,
        Method::POST,
        &format!("{}?library={kids}", a_dirs_scan(dir)),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    taiyaku_library::libraries::rename(&state.library.conn(), kids, "Children")
        .expect("renames Kids");

    let status = get_json(&router, &the_scan_of(kids)).await;
    assert_eq!(
        status["scans"],
        serde_json::json!([{ "library_id": kids, "library": "Children", "root_ids": [dir] }]),
        "{status}"
    );
    drop(held);
    until_the_library_scan_ends(&router, kids).await;
}

/// A page older than an unlink names a `from` that no longer holds the
/// directory. It is refused at once, 404, in the move's words, and not told
/// to wait for a scan of `from` that holds up a move it would never make.
#[tokio::test]
async fn a_stale_from_is_refused_before_the_move_waits_on_a_scan() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let (status, unlinked) = answered(
        &router,
        Method::DELETE,
        &unlink_path(kids, &shared, false),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{unlinked}");
    a_scan_running_on(&state, own["id"].as_i64().expect("an id"));
    let dir = shared["id"].as_i64().expect("an id");
    let stored = the_store(&state);

    for dry_run in [true, false] {
        let (status, refused) = moved(
            &router,
            dir,
            dry_run,
            &serde_json::json!({ "from": kids, "new_library": "Elsewhere" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
        assert_eq!(
            the_sentence(&refused),
            format!(
                "`{}` is not in Kids",
                shared["path"].as_str().expect("a path")
            )
        );
    }
    assert_eq!(the_store(&state), stored);
}

/// A page older than an unlink presses Remove from Kids on a directory Kids
/// no longer holds. It is refused at once, 404, in the unlink's words, and not
/// told to wait for a scan of Kids' own directory or for the job lock, which
/// hold up an unlink it would never make.
#[tokio::test]
async fn a_stale_unlink_is_refused_before_it_waits_on_a_scan_or_the_lock() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let (status, unlinked) = answered(
        &router,
        Method::DELETE,
        &unlink_path(kids, &shared, false),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{unlinked}");
    let says = format!(
        "`{}` is not in Kids",
        shared["path"].as_str().expect("a path")
    );
    let stored = the_store(&state);

    let own_id = own["id"].as_i64().expect("an id");
    a_scan_running_on(&state, own_id);
    for dry_run in [true, false] {
        let (status, refused) = answered(
            &router,
            Method::DELETE,
            &unlink_path(kids, &shared, dry_run),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "a scan: {refused}");
        assert_eq!(the_sentence(&refused), says);
    }
    state.scans.lock().unwrap().remove(&own_id);

    let held = holding_the_job(&state);
    for dry_run in [true, false] {
        let (status, refused) = answered(
            &router,
            Method::DELETE,
            &unlink_path(kids, &shared, dry_run),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "the lock: {refused}");
        assert_eq!(the_sentence(&refused), says);
    }
    drop(held);
    assert_eq!(the_store(&state), stored);
}

/// A last link is refused at once, 400, pointing to Remove this directory,
/// while the job lock is held: waiting would only end in the same refusal.
#[tokio::test]
async fn a_last_link_unlink_is_refused_before_it_waits_on_the_lock() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let root_a = the_library(&listed, "root-a");
    let library = root_a["id"].as_i64().expect("an id");
    let dir = root_a["dirs"][0].clone();
    let stored = the_store(&state);

    let held = holding_the_job(&state);
    for dry_run in [true, false] {
        let (status, refused) = answered(
            &router,
            Method::DELETE,
            &unlink_path(library, &dir, dry_run),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert_eq!(
            the_sentence(&refused),
            format!(
                "`{}` is in no other library, so it can only be removed: use Remove this directory.",
                dir["path"].as_str().expect("a path")
            )
        );
    }
    drop(held);
    assert_eq!(the_store(&state), stored);
}

thread_local! {
    /// The connection [`removing_a_library_at_the_first_row`] removes a
    /// library on, and the library's id: a trace callback is a `fn`, which
    /// captures nothing.
    static REMOVER: std::cell::RefCell<Option<(taiyaku_library::Library, i64)>> =
        const { std::cell::RefCell::new(None) };
}

/// A trace callback that removes [`REMOVER`]'s library, on its own
/// connection, at the first row the traced connection reads: after that
/// read's snapshot is taken, and before any later statement takes its own.
fn removing_a_library_at_the_first_row(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Row(_) = event
        && let Some((other, id)) = REMOVER.with(|remover| remover.borrow_mut().take())
    {
        taiyaku_library::scan::remove_library(&other.conn(), id)
            .expect("the other connection removes the library");
    }
}

/// The Libraries list reads its links and its names from one snapshot. Kids,
/// which shares Manga's `m` and holds a directory of its own, is removed from
/// a second connection at the first row the list reads: the list is the
/// state before the removal, never a link to a library with no name (a 500).
#[test]
fn the_libraries_list_reads_its_links_and_names_from_one_snapshot() {
    use taiyaku_library::scan::{InLibrary, add_root_checked_in};
    let fixture = test_support::Fixture::new("libraries-one-snapshot");
    let (m, own) = (fixture.path().join("m"), fixture.path().join("own"));
    std::fs::create_dir_all(&m).expect("mkdir");
    std::fs::create_dir_all(&own).expect("mkdir");
    let db = fixture.path().join("library.db");
    let a = taiyaku_library::Library::open(&db).expect("the first connection");
    let kids = {
        let conn = a.conn();
        add_root_checked_in(&conn, &m, InLibrary::Named("Manga")).expect("Manga holds m");
        add_root_checked_in(&conn, &m, InLibrary::Named("Kids")).expect("links m into Kids");
        add_root_checked_in(&conn, &own, InLibrary::Named("Kids")).expect("Kids holds own");
        taiyaku_library::libraries::id_of(&conn, "Kids")
            .expect("reads")
            .expect("Kids")
    };
    let names = |listed: Vec<serde_json::Value>| -> Vec<serde_json::Value> {
        listed
            .into_iter()
            .map(|library| library["name"].clone())
            .collect()
    };
    let b = taiyaku_library::Library::open(&db).expect("a second connection");
    REMOVER.with(|remover| *remover.borrow_mut() = Some((b, kids)));

    let conn = a.conn();
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_ROW,
        Some(removing_a_library_at_the_first_row),
    );
    let listed = super::libraries_json(&conn, &std::collections::HashSet::new());
    conn.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);

    assert!(
        REMOVER.with(|remover| remover.borrow().is_none()),
        "the fixture: Kids was removed while the list read"
    );
    assert_eq!(
        names(listed.expect("the list answers")),
        ["Kids", "Manga"],
        "the state before the removal"
    );
    assert_eq!(
        names(
            super::libraries_json(&conn, &std::collections::HashSet::new())
                .expect("the list answers")
        ),
        ["Manga"],
        "and Kids is gone after it"
    );
}

/// An add's answer names the libraries that held the directory as the add
/// read them, inside its transaction: Manga, removed after the link into Kids
/// commits and before the answer is drawn, is still named, and the answer is
/// not a 500.
#[test]
fn an_adds_answer_names_the_holders_the_add_read() {
    use taiyaku_library::scan::{InLibrary, add_root_checked_in};
    let fixture = test_support::Fixture::new("added-names-the-holders");
    let m = fixture.path().join("m");
    std::fs::create_dir_all(&m).expect("mkdir");
    let library = taiyaku_library::Library::open_in_memory().expect("a library");
    let conn = library.conn();
    add_root_checked_in(&conn, &m, InLibrary::Named("Manga")).expect("Manga holds m");
    let linked =
        add_root_checked_in(&conn, &m, InLibrary::Named("Kids")).expect("links m into Kids");
    let manga = taiyaku_library::libraries::id_of(&conn, "Manga")
        .expect("reads")
        .expect("Manga");
    taiyaku_library::scan::remove_library(&conn, manga).expect("removes Manga");

    let answer = crate::config_api::added_json(&linked);
    assert_eq!(answer["also_in"], serde_json::json!(["Manga"]), "{answer}");
}

/// An add's answer sends a linked directory's scan columns as the add read
/// them, inside its transaction: `m`, scanned under Manga and removed after
/// the link into Kids commits, still answers Manga's scan, and the answer is
/// drawn, not a 500, for a row that is gone (O254).
#[test]
fn an_adds_answer_reads_the_directorys_scan_status_inside_the_add() {
    use taiyaku_library::scan::{InLibrary, add_root_checked_in};
    let fixture = test_support::Fixture::new("added-reads-the-scan-status");
    let m = fixture.path().join("m");
    std::fs::create_dir_all(&m).expect("mkdir");
    let library = taiyaku_library::Library::open_in_memory().expect("a library");
    let conn = library.conn();
    let held = add_root_checked_in(&conn, &m, InLibrary::Named("Manga")).expect("Manga holds m");
    taiyaku_library::scan::scan_root(&conn, &held.root).expect("scans m");
    let (at, status): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1",
            [held.root.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reads m's scan columns");
    assert!(at.is_some(), "the fixture: the scan recorded its time");
    assert_eq!(status.as_deref(), Some("ok"), "the fixture: m scanned");
    let linked =
        add_root_checked_in(&conn, &m, InLibrary::Named("Kids")).expect("links m into Kids");
    taiyaku_library::scan::remove_root(&conn, held.root.id).expect("removes m");

    let answer = crate::config_api::added_json(&linked);
    assert_eq!(answer["root"]["last_scan_status"], "ok", "{answer}");
    assert_eq!(
        answer["root"]["last_scan_at"],
        serde_json::json!(at),
        "{answer}"
    );
}

/// New library's answer for a library another request removed after it was
/// made and before its list entry was read is a refusal, a 409, not a store
/// fault: the store did what both requests asked (O254).
#[test]
fn a_library_removed_before_its_answer_is_drawn_is_refused_not_failed() {
    let fixture = test_support::Fixture::new("created-then-removed");
    let m = fixture.path().join("m");
    std::fs::create_dir_all(&m).expect("mkdir");
    let library = taiyaku_library::Library::open_in_memory().expect("a library");
    let conn = library.conn();
    let made =
        taiyaku_library::scan::create_library(&conn, "Kids", &[m], None).expect("makes Kids");
    taiyaku_library::scan::remove_library(&conn, made.library_id).expect("removes Kids");

    match super::created_json(&conn, &made, &std::collections::HashSet::new()) {
        Err(super::AdminAnswer::Refused(why)) => assert_eq!(
            why,
            "another request removed Kids before it could be listed"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Removing Kids clears the scan slots of the directories the removal deletes,
/// as the store read them inside it: root-a's directory, unlinked from Kids
/// after the handler read Kids' directories and before the removal, keeps
/// root-a's finished report. The handler is held at the scan registry, which
/// it asks after that read, while the test unlinks. Load can make this miss
/// that defect, should the handler reach the read only after the unlink. It
/// cannot fail correct code, which never clears a slot it did not remove.
#[tokio::test]
async fn a_dir_unlinked_while_its_library_is_removed_keeps_its_scan_report() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let (kids, shared, own) = kids_sharing_root_as_dir(&router, &fixture).await;
    let (shared_id, own_id) = (
        shared["id"].as_i64().expect("an id"),
        own["id"].as_i64().expect("an id"),
    );
    let report = serde_json::json!({ "scanned": "by root-a" });
    for slot in [shared_id, own_id] {
        state.scans.lock().unwrap().insert(
            slot,
            crate::ScanProgress {
                waiting: false,
                started: std::time::Instant::now(),
                done: true,
                report: Some(report.clone()),
                library: None,
            },
        );
    }

    let (release, released) = std::sync::mpsc::channel::<()>();
    let (locked_tx, locked) = std::sync::mpsc::channel::<()>();
    let holder = {
        let state = std::sync::Arc::clone(&state);
        std::thread::spawn(move || {
            let _registry = state.scans.lock().unwrap();
            locked_tx.send(()).expect("the test is waiting");
            let _ = released.recv();
        })
    };
    locked.recv().expect("the holder took the registry");
    let removing = tokio::spawn({
        let router = router.clone();
        async move {
            answered(
                &router,
                Method::DELETE,
                &format!("/api/library/libraries/{kids}"),
                &serde_json::json!({}),
            )
            .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    taiyaku_library::scan::unlink(&state.library.conn(), shared_id, kids, false)
        .expect("unlinks root-a's directory from Kids meanwhile");
    drop(release);
    holder.join().expect("the holder ends");
    let (status, removed) = removing.await.expect("the removal answers");

    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(
        removed["removed"]["kept"],
        serde_json::json!([]),
        "{removed}"
    );
    let scans = state.scans.lock().unwrap();
    assert_eq!(
        scans
            .get(&shared_id)
            .and_then(|progress| progress.report.clone()),
        Some(report),
        "root-a's directory keeps its slot and its report"
    );
    assert!(
        scans.get(&own_id).is_none(),
        "the removed directory's slot is cleared"
    );
}

/// O51: a library's removal derives, so even its dry run refuses while another
/// derive runs, and answers once it ends.
#[tokio::test]
async fn a_library_removal_refuses_while_another_derive_holds_the_lease() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/libraries/{id}?dry_run=true");
    let elsewhere = test_support::the_store_elsewhere(&state);
    let held = test_support::holding_the_lease(&elsewhere);

    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    let status = response.status();
    let refused = test_support::body_string(response).await;
    assert_eq!(
        (
            status,
            serde_json::from_str::<serde_json::Value>(&refused).ok()
        ),
        (
            StatusCode::CONFLICT,
            Some(serde_json::json!({
                "error": "a scan or another library job is running; remove the library once it \
                          has finished"
            }))
        ),
        "{refused}"
    );

    drop(held);
    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// O51: a library's status says a derive runs while any derive holds the
/// lease, another process's among them, so its pages hold what the lease
/// refuses; the job lock is not what it reads.
#[tokio::test]
async fn a_librarys_status_says_a_derive_runs_while_the_lease_is_held() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let running = |status: &serde_json::Value| status["derive_running"].clone();
    let elsewhere = test_support::the_store_elsewhere(&state);

    let held = test_support::holding_the_lease(&elsewhere);
    let during = get_json(&router, &the_scan_of(id)).await;
    drop(held);
    let after = get_json(&router, &the_scan_of(id)).await;

    assert_eq!(
        (running(&during), running(&after)),
        (serde_json::json!(true), serde_json::json!(false)),
        "{during} {after}"
    );
}

/// O51: `GET /api/library/derive` says apart whether the lease or the job
/// lock is held, for the pages that read no library's status.
#[tokio::test]
async fn the_derive_status_says_apart_the_lease_and_the_job_lock() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let elsewhere = test_support::the_store_elsewhere(&state);

    let free = get_json(&router, "/api/library/derive").await;
    let held = test_support::holding_the_lease(&elsewhere);
    let leased = get_json(&router, "/api/library/derive").await;
    drop(held);
    let job = holding_the_job(&state);
    let locked = get_json(&router, "/api/library/derive").await;
    drop(job);

    assert_eq!(
        [free, leased, locked],
        [
            serde_json::json!({ "derive_running": false, "job_held": false }),
            serde_json::json!({ "derive_running": true, "job_held": false }),
            serde_json::json!({ "derive_running": false, "job_held": true }),
        ]
    );
}

/// O51: a job's word that it waits lands on its own reservations, where the
/// directory's scan status reads it.
#[tokio::test]
async fn a_jobs_word_that_it_waits_lands_on_its_reservations() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let roots = taiyaku_library::scan::roots(&state.library.conn()).expect("the directories");
    let started = std::time::Instant::now();
    crate::scan_jobs::reserve(&state, &roots, started, None).expect("reserves");
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (said, heard) = std::sync::mpsc::channel::<()>();

    let job = crate::scan_jobs::spawn_job(
        std::sync::Arc::clone(&state),
        roots.clone(),
        started,
        move |_, roots, waiting| {
            waiting(roots, true);
            said.send(()).expect("the test is listening");
            let _ = released.recv();
            Ok(roots.iter().map(|_| serde_json::json!({})).collect())
        },
    );
    heard.recv().expect("the job said it waits");
    let read = get_json(&router, &format!("/api/library/roots/{}/scan", roots[0].id)).await;
    release.send(()).expect("releases the job");
    job.await.expect("the job ends");

    assert_eq!(read["last_scan_status"], "waiting", "{read}");
}

/// O51: both lists of directories read `waiting` where the directory's scan
/// waits for the job lock or the derive lease, as its scan status does, and
/// the store's word for a directory whose scan does not: a holder in this
/// process may hold the store's write lock, and the store's word with it,
/// for its whole derive.
#[tokio::test]
async fn both_lists_read_waiting_where_a_directorys_scan_waits_and_only_there() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots_on_disk();
    let roots = taiyaku_library::scan::roots(&state.library.conn()).expect("the directories");
    let (waits, free) = (roots[0].id, roots[1].id);
    let started = std::time::Instant::now();
    let reserved = std::slice::from_ref(&roots[0]);
    crate::scan_jobs::reserve(&state, reserved, started, None).expect("reserves");
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (said, heard) = std::sync::mpsc::channel::<()>();

    let job = crate::scan_jobs::spawn_job(
        std::sync::Arc::clone(&state),
        reserved.to_vec(),
        started,
        move |_, roots, waiting| {
            waiting(roots, true);
            said.send(()).expect("the test is listening");
            let _ = released.recv();
            Ok(roots.iter().map(|_| serde_json::json!({})).collect())
        },
    );
    heard.recv().expect("the job said it waits");
    let listed = get_json(&router, "/api/library/roots").await;
    let libraries = get_json(&router, "/api/library/libraries").await;
    release.send(()).expect("releases the job");
    job.await.expect("the job ends");

    let in_roots = |id: i64| {
        listed["roots"]
            .as_array()
            .expect("a list of directories")
            .iter()
            .find(|root| root["id"] == id)
            .map(|root| root["last_scan_status"].clone())
    };
    let in_libraries = |id: i64| {
        libraries["libraries"]
            .as_array()
            .expect("a list of libraries")
            .iter()
            .flat_map(|library| library["dirs"].as_array().expect("its directories"))
            .find(|dir| dir["id"] == id)
            .map(|dir| dir["last_scan_status"].clone())
    };
    assert_eq!(
        [in_roots(waits), in_roots(free)],
        [Some("waiting".into()), Some("ok".into())],
        "{listed}"
    );
    assert_eq!(
        [in_libraries(waits), in_libraries(free)],
        [Some("waiting".into()), Some("ok".into())],
        "{libraries}"
    );
}

/// A Scan library accepted while the job lock is held has not started: every
/// directory it will walk reads `waiting`, in its own status and both lists,
/// until it runs, then `ok`.
#[tokio::test]
async fn a_library_scan_queued_behind_the_job_lock_reads_waiting_on_each_directory() {
    let n = test_support::router_over_a_library_nested_in_another();
    let held = holding_the_job(&n.state);
    let (status, started) = answered(
        &n.admin,
        Method::POST,
        &the_scan_of(n.manga),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let queued = [
        test_support::last_scan_words(&n.admin, n.m).await,
        test_support::last_scan_words(&n.admin, n.m_kids).await,
    ];
    drop(held);
    until_the_library_scan_ends(&n.admin, n.manga).await;
    let after = [
        test_support::last_scan_words(&n.admin, n.m).await,
        test_support::last_scan_words(&n.admin, n.m_kids).await,
    ];

    let waiting = test_support::every_page_reads(true, "waiting");
    let ok = test_support::every_page_reads(false, "ok");
    assert_eq!(
        [queued, after],
        [[waiting.clone(), waiting], [ok.clone(), ok]],
        "(queued, after) for (m, m/kids)"
    );
}
