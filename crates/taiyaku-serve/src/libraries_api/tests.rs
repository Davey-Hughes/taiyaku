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
                    "epub-reflow"
                };
                conn.execute(
                    "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
                     SELECT id, ?2, 'file', ?3, 1, 1 FROM root
                      WHERE library_id = ?1 ORDER BY id LIMIT 1",
                    rusqlite::params![library, rel, format],
                )
                .expect("a book under the library's first directory");
            }
        }
        // And a third under a's second directory, which a count over the first
        // directory alone would miss.
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             SELECT id, 'a3.epub', 'file', 'epub-reflow', 1, 1 FROM root
              WHERE library_id = ?1 ORDER BY id DESC LIMIT 1",
            [a_id],
        )
        .expect("a book under the library's second directory");
        // A cbz entry is not a book.
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             SELECT id, 'c.cbz', 'file', 'zip', 1, 1 FROM root WHERE library_id = ?1 LIMIT 1",
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
                "keeps_documents",
                "last_scan_at",
                "last_scan_status",
                "path",
                "scan_interval_secs",
            ],
            "a directory is root_json's shape: {dir}"
        );
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
            vec!["interval_secs", "root", "watch_reason", "watching"],
            "{made}"
        );
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
        vec!["interval_secs", "root", "watch_reason", "watching"],
        "{added}"
    );
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
    for (body, says) in [
        (
            serde_json::json!({ "path": inside }),
            format!(
                "{} is inside {held}, which is already a directory of the library `root-a`",
                canonical(&inside).display()
            ),
        ),
        (
            serde_json::json!({ "path": held }),
            format!(
                "{} is already a directory of the library `root-a`",
                canonical(&held).display()
            ),
        ),
    ] {
        let (got, refused) = answered(&router, Method::POST, &path, &body).await;
        assert_eq!(got, StatusCode::BAD_REQUEST, "{body}: {refused}");
        assert_eq!(the_sentence(&refused), says, "{body}");
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
                started: std::time::Instant::now(),
                done: true,
                report: Some(serde_json::json!({})),
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
        vec!["left_with_no_library", "notes", "positions", "works"]
    );
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
/// Every other route under `/api/library/roots` still answers.
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
        &serde_json::json!({ "path": a_dir_of(fixture, "Second", "Second Series") }),
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
            started: std::time::Instant::now(),
            done: false,
            report: None,
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
            .join("Second")
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
        serde_json::json!({ "library_id": id, "status_url": the_scan_of(id) })
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
        vec!["dirs", "elapsed_secs", "library_id", "running"]
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
                "the library's directories were scanned, but deriving its structure afterward failed:"
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
        super::dirs_of(&conn, id).expect("the directories")
    };

    let started = std::time::Instant::now();
    crate::scan_jobs::reserve(&state, &roots, started).expect("reserves");
    assert_eq!(get_json(&router, &the_scan_of(id)).await["running"], true);
    // The job dies holding the job lock, which poisons it: the scans after it
    // must go on regardless.
    crate::scan_jobs::spawn_job(std::sync::Arc::clone(&state), roots, started, |state, _| {
        let _job = state.library_job.lock().expect("the job lock is free");
        panic!("a walk went wrong")
    });
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
            conn.prepare("SELECT id FROM work WHERE library_id = ?1 ORDER BY id")
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
            "left_with_no_library": [],
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
            "left_with_no_library": [],
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
        super::dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    crate::scan_jobs::reserve(&state, &roots, started).expect("reserves");

    let job = crate::scan_jobs::spawn_job(std::sync::Arc::clone(&state), roots, started, |_, _| {
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
        super::dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started).expect("reserves");
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
        super::dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started).expect("reserves");
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
        super::dirs_of(&conn, id).expect("the directories")
    };
    let captured = roots[0].id;
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started).expect("reserves");
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
        super::dirs_of(&conn, id).expect("the directories")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    crate::scan_jobs::reserve(&state, &roots, started).expect("reserves");
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
        super::dirs_of(&conn, fresh).expect("the directories")
    };
    let later = std::time::Instant::now();
    crate::scan_jobs::reserve(&state, &fresh_roots, later).expect("reserves");

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
        super::dirs_of(&conn, id).expect("the directories")
    };
    let held = holding_the_job(state);
    let started = std::time::Instant::now();
    crate::scan_jobs::reserve(state, &roots, started).expect("reserves");
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
            "left_with_no_library": [],
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
    for (table, column) in [
        ("library", "id"),
        ("root", "library_id"),
        ("work", "library_id"),
    ] {
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
/// do and changes nothing; the move does it.
#[tokio::test]
async fn a_dir_is_rehearsed_and_then_moved_to_another_library() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let dir = the_library(&listed, "root-b")["dirs"][0]["id"]
        .as_i64()
        .expect("a directory id");
    let to = serde_json::json!({ "library": a });
    let stored = the_store(&state);

    let (status, rehearsed) = moved(&router, dir, true, &to).await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(rehearsed["dry_run"], true);
    assert_eq!(rehearsed["moved"]["library"], "root-a");
    assert_eq!(rehearsed["moved"]["works_kept"], 1, "{rehearsed}");
    assert_eq!(
        rehearsed["moved"]["library_removed"], "root-b",
        "{rehearsed}"
    );
    assert_eq!(
        keys_of(&rehearsed["moved"]),
        [
            "alignments_dropped",
            "grants_removed",
            "left_with_no_library",
            "library",
            "library_id",
            "library_removed",
            "notes",
            "positions",
            "series_joined",
            "works_joined",
            "works_kept",
            "works_split",
        ]
    );
    assert!(rehearsed["derive"].is_object(), "{rehearsed}");
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
    let (dir, reader) = {
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
            solo.id, solo.library_id,
            "the fixture must tell a directory's id from its library's: {solo:?}"
        );
        let reader = taiyaku_library::account::add(&conn, "reader", "pw", "user").expect("adds");
        taiyaku_library::account::grant_library(&conn, reader, solo.library_id).expect("grants");
        (solo.id, reader)
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
    let to = serde_json::json!({ "library": a });
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
        &serde_json::json!({ "new_library": " Extras " }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["moved"]["library"], "Extras");
    assert_eq!(done["moved"]["library_removed"], serde_json::Value::Null);
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

/// The same volume under the same edition key in both libraries: 409, the
/// volume named in the sentence and listed beside it, and nothing changed.
#[tokio::test]
async fn a_refused_move_is_a_409_naming_its_volumes() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let mut ids = Vec::new();
    for name in ["One", "Two"] {
        let (status, made) = answered(
            &router,
            Method::POST,
            "/api/library/libraries",
            &serde_json::json!({ "name": name, "dirs": [a_dir_of(&fixture, name, "Same")] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
        let id = made["library"]["id"].as_i64().expect("an id");
        let (status, _) = answered(
            &router,
            Method::POST,
            &format!("/api/library/libraries/{id}/scan"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        until_the_library_scan_ends(&router, id).await;
        ids.push((
            id,
            made["library"]["dirs"][0]["id"]
                .as_i64()
                .expect("a directory"),
        ));
    }
    let stored = the_store(&state);

    for dry_run in [true, false] {
        let (status, refused) = moved(
            &router,
            ids[0].1,
            dry_run,
            &serde_json::json!({ "library": ids[1].0 }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(keys_of(&refused), ["error", "volumes"], "{refused}");
        assert!(
            refused["error"]
                .as_str()
                .expect("an error")
                .contains("same edition key"),
            "{refused}"
        );
        assert_eq!(
            refused["volumes"].as_array().expect("volumes").len(),
            1,
            "{refused}"
        );
    }
    assert_eq!(the_store(&state), stored);
}

#[tokio::test]
async fn a_move_that_names_no_one_target_or_nothing_there_is_the_callers_mistake() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let own = a["id"].as_i64().expect("an id");
    let dir = a["dirs"][0]["id"].as_i64().expect("a directory id");
    let stored = the_store(&state);

    for (body, status, says) in [
        (
            serde_json::json!({}),
            StatusCode::BAD_REQUEST,
            "exactly one",
        ),
        (
            serde_json::json!({ "library": 1, "new_library": "X" }),
            StatusCode::BAD_REQUEST,
            "exactly one",
        ),
        (
            serde_json::json!({ "library": "one" }),
            StatusCode::BAD_REQUEST,
            "`library`",
        ),
        (
            serde_json::json!({ "new_library": 7 }),
            StatusCode::BAD_REQUEST,
            "`new_library`",
        ),
        (
            serde_json::json!({ "new_library": "  " }),
            StatusCode::BAD_REQUEST,
            "name",
        ),
        (
            serde_json::json!({ "new_library": "root-b" }),
            StatusCode::BAD_REQUEST,
            "root-b",
        ),
        (
            serde_json::json!({ "library": own }),
            StatusCode::BAD_REQUEST,
            "root-a",
        ),
        (
            serde_json::json!({ "library": 4242 }),
            StatusCode::NOT_FOUND,
            "there is no library 4242",
        ),
    ] {
        for dry_run in [true, false] {
            let (got, refused) = moved(&router, dir, dry_run, &body).await;
            assert_eq!(got, status, "{body}: {refused}");
            assert!(the_sentence(&refused).contains(says), "{body}: {refused}");
        }
    }
    let (status, missing) = moved(
        &router,
        4242,
        false,
        &serde_json::json!({ "new_library": "X" }),
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
        &serde_json::json!({ "new_library": "X" }),
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
        &serde_json::json!({ "new_library": "X" }),
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
    let path = format!("/api/library/roots/{dir}/move");
    let to = serde_json::json!({ "library": a });
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

/// A move derives the whole store, so it does not run beside a scan: 409 at
/// once, for a rehearsal too, and nothing moved.
#[tokio::test]
async fn a_move_is_refused_while_a_scan_holds_the_library() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"]
        .as_i64()
        .expect("an id");
    let dir = the_library(&listed, "root-b")["dirs"][0]["id"]
        .as_i64()
        .expect("a directory id");
    let stored = the_store(&state);
    let held = holding_the_job(&state);

    for dry_run in [true, false] {
        let (status, refused) =
            moved(&router, dir, dry_run, &serde_json::json!({ "library": a })).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(keys_of(&refused), ["error", "volumes"], "{refused}");
        assert!(
            refused["error"]
                .as_str()
                .expect("an error")
                .contains("running")
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
    let to = serde_json::json!({ "library": a["id"] });
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

    // A new library is made, so only the directory's own library is asked.
    a_scan_running_on(&state, a_dir);
    let (status, made) = moved(
        &router,
        b_dir,
        true,
        &serde_json::json!({ "new_library": "Elsewhere" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a scan of an unrelated library: {made}"
    );
}

/// A scan reserved on a sibling of the moving directory, in the directory's own
/// library, holds the move up: the library's derive would run under it.
#[tokio::test]
async fn a_move_is_refused_while_a_scan_of_a_sibling_dir_is_reserved() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (_, [moving, sibling]) = a_library_with_an_unscanned_second_dir(&router, &fixture).await;
    let stored = the_store(&state);
    a_scan_running_on(&state, sibling);

    for dry_run in [true, false] {
        let (status, refused) = moved(
            &router,
            moving,
            dry_run,
            &serde_json::json!({ "new_library": "Elsewhere" }),
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
    let dir = {
        let conn = state.library.conn();
        let dir = taiyaku_library::scan::roots(&conn).expect("lists")[1].id;
        conn.pragma_update(None, "query_only", true)
            .expect("sets query_only");
        dir
    };
    let body = serde_json::json!({ "new_library": "Elsewhere" });

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
        works_kept: 22,
        works_joined: 33,
        works_split: 44,
        series_joined: 55,
        positions: 66,
        notes: 77,
        alignments_dropped: 88,
        library_removed: Some("Left".to_string()),
        grants_removed: 99,
        left_with_no_library: vec!["ann".to_string(), "bo".to_string()],
    };
    let expected = serde_json::json!({
        "library_id": 11,
        "library": "Target",
        "works_kept": 22,
        "works_joined": 33,
        "works_split": 44,
        "series_joined": 55,
        "positions": 66,
        "notes": 77,
        "alignments_dropped": 88,
        "library_removed": "Left",
        "grants_removed": 99,
        "left_with_no_library": ["ann", "bo"],
    });
    assert_eq!(super::moved_json(&moved), expected);

    let kept = taiyaku_library::relocate::Moved {
        library_removed: None,
        left_with_no_library: Vec::new(),
        ..moved
    };
    let mut expected = expected;
    expected["library_removed"] = serde_json::Value::Null;
    expected["left_with_no_library"] = serde_json::json!([]);
    assert_eq!(super::moved_json(&kept), expected);
}
