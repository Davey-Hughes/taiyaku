use crate::test_support::{self, get_json, send_json};
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
fn a_folder_of(fixture: &test_support::Fixture, name: &str, series: &str) -> String {
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
        roots: Vec::new(),
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
async fn adding_a_folder_refuses_a_reader() {
    a_reader_is_refused(Method::POST, "/api/library/libraries/1/folders").await;
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
async fn adding_a_folder_refuses_nobody() {
    an_unsigned_request_is_refused(Method::POST, "/api/library/libraries/1/folders").await;
}

/// Two libraries with different non-zero counts of each kind, one of them
/// holding two folders, named so that by-name order is not by-id order, and
/// its second folder named so that by-path order is not oldest first.
#[tokio::test]
async fn the_list_gives_each_library_its_own_folders_works_and_books() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let first = get_json(&router, "/api/library/libraries").await;
    let a_id = the_library(&first, "root-a")["id"].as_i64().expect("an id");
    let b_id = the_library(&first, "root-b")["id"].as_i64().expect("an id");
    {
        let conn = state.library.conn();
        taiyaku_library::libraries::rename(&conn, a_id, "zz-first").expect("renames");
        // A second work under library a, from a folder added to it and scanned.
        let extra = fixture.path().join("aaa-extra");
        test_support::write_pages(&extra.join("Series C").join("Series C - v01"), 3);
        let added = taiyaku_library::scan::add_root_checked_in(
            &conn,
            &extra,
            taiyaku_library::scan::InLibrary::Id(a_id),
        )
        .expect("adds a folder to library a");
        taiyaku_library::scan::scan_root(&conn, &added.root).expect("scans it");
        taiyaku_library::derive::derive(&conn).expect("derives");
        // Two books under a's first folder and two under b's.
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
                .expect("a book under the library's first folder");
            }
        }
        // And a third under a's second folder, which a count over the first
        // folder alone would miss.
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             SELECT id, 'a3.epub', 'file', 'epub-reflow', 1, 1 FROM root
              WHERE library_id = ?1 ORDER BY id DESC LIMIT 1",
            [a_id],
        )
        .expect("a book under the library's second folder");
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
        vec!["books", "folders", "id", "name", "works"],
        "{b}"
    );
    assert_eq!(b["id"], b_id);
    assert_eq!(b["works"], 1, "{listed}");
    assert_eq!(b["books"], 2, "{listed}");
    assert_eq!(
        b["folders"].as_array().expect("folders").len(),
        1,
        "{listed}"
    );
    assert!(
        b["folders"][0]["path"]
            .as_str()
            .expect("a path")
            .ends_with("root-b")
    );

    let a = the_library(&listed, "zz-first");
    assert_eq!(a["id"], a_id);
    assert_eq!(a["works"], 2, "{listed}");
    assert_eq!(a["books"], 3, "{listed}");
    let folders = a["folders"].as_array().expect("folders");
    assert_eq!(folders.len(), 2, "{listed}");
    assert!(
        folders[0]["path"]
            .as_str()
            .expect("a path")
            .ends_with("root-a")
    );
    assert!(
        folders[1]["path"]
            .as_str()
            .expect("a path")
            .ends_with("aaa-extra")
    );
    assert!(
        folders[0]["id"].as_i64() < folders[1]["id"].as_i64(),
        "oldest first"
    );
    for folder in folders {
        assert_eq!(
            keys_of(folder),
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
            "a folder is root_json's shape: {folder}"
        );
    }
    assert_eq!(folders[0]["last_scan_status"], "ok", "{listed}");
    assert!(folders[0]["last_scan_at"].is_string(), "{listed}");
    assert_eq!(folders[1]["last_scan_status"], "ok", "{listed}");
    assert_eq!(b["folders"][0]["last_scan_status"], "ok", "{listed}");
}

#[tokio::test]
async fn a_library_is_created_with_its_folders_and_then_listed() {
    let (router, _state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-route");
    let folders = [
        a_folder_of(&fixture, "Volumes", "Alpha"),
        a_folder_of(&fixture, "Chapters", "Beta"),
    ];

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": " zzz Manga ", "folders": folders }),
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
    let held = made["library"]["folders"].as_array().expect("folders");
    assert_eq!(held.len(), 2, "{made}");
    let added = made["added"].as_array().expect("added");
    assert_eq!(added.len(), 2, "{made}");
    for (one, (given, held)) in added.iter().zip(folders.iter().zip(held)) {
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
            "added is in the order the folders were given: {made}"
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
    let good = a_folder_of(&fixture, "Volumes", "Alpha");
    let before = get_json(&router, "/api/library/libraries").await;
    let stored = the_store(&state);

    for (body, says) in [
        (
            serde_json::json!({ "name": "Manga", "folders": [good, "/no/such/taiyaku-test-folder"] }),
            "taiyaku-test-folder",
        ),
        (
            serde_json::json!({ "name": "Manga", "folders": [] }),
            "at least one folder",
        ),
        (
            serde_json::json!({ "name": "  ", "folders": [good] }),
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
        &serde_json::json!({ "name": taken, "folders": [good] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        the_sentence(&refused).contains("already called"),
        "{refused}"
    );

    // The folders are vetted before the name is looked at.
    let (status, refused) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": taken, "folders": ["/no/such/taiyaku-test-folder"] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        the_sentence(&refused).contains("taiyaku-test-folder"),
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
                ("{\"folders\": [\"/p\"]}", "`name`"),
                ("{\"name\": 7, \"folders\": [\"/p\"]}", "`name`"),
                ("{\"name\": \"M\"}", "`folders`"),
                ("{\"name\": \"M\", \"folders\": \"/p\"}", "`folders`"),
                ("{\"name\": \"M\", \"folders\": [7]}", "`folders`"),
                (
                    "{\"name\": \"M\", \"folders\": [\"/p\", null]}",
                    "`folders`",
                ),
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
            format!("/api/library/libraries/{id}/folders"),
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
    let extra = a_folder_of(&fixture, "Extra", "Gamma");
    let stored = the_store(&state);

    for (method, suffix, body) in [
        (Method::PATCH, "", serde_json::json!({ "name": "Other" })),
        (Method::DELETE, "", serde_json::json!({})),
        (Method::DELETE, "?dry_run=true", serde_json::json!({})),
        (
            Method::POST,
            "/folders",
            serde_json::json!({ "path": extra }),
        ),
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
async fn a_folder_is_added_to_a_library_by_the_librarys_id() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-folder-route");
    let extra = a_folder_of(&fixture, "Extra", "Gamma");
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/libraries/{id}/folders");

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
    assert_eq!(
        a["folders"].as_array().expect("folders").len(),
        2,
        "{listed}"
    );
    assert_eq!(a["folders"][1]["id"], added["root"]["id"], "{listed}");
    // Each folder carries its own scan columns: the old one its scan, the new
    // one none. The new folder's id is not its library's, so the columns cannot
    // be found by the library's id and look right.
    assert_ne!(added["root"]["id"], a["id"], "{listed}");
    assert_eq!(a["folders"][0]["last_scan_status"], "ok", "{listed}");
    assert!(a["folders"][0]["last_scan_at"].is_string(), "{listed}");
    assert!(a["folders"][1]["last_scan_status"].is_null(), "{listed}");
    assert!(a["folders"][1]["last_scan_at"].is_null(), "{listed}");
    assert_eq!(
        the_library(&listed, "root-a")["folders"]
            .as_array()
            .expect("folders")
            .len(),
        1,
        "the folder joined the library it was sent to"
    );
    assert_eq!(listed["libraries"].as_array().expect("libraries").len(), 2);

    let stored = the_store(&state);
    for (body, says) in [
        (
            serde_json::json!({ "path": test_support::a_path_inside_the_first_root(&state) }),
            "inside",
        ),
        (
            serde_json::json!({ "path": "/no/such/taiyaku-test-folder" }),
            "taiyaku-test-folder",
        ),
    ] {
        let (got, refused) = answered(&router, Method::POST, &path, &body).await;
        assert_eq!(got, StatusCode::BAD_REQUEST, "{body}: {refused}");
        assert!(the_sentence(&refused).contains(says), "{refused}");
    }
    assert_eq!(the_store(&state), stored);
}

#[tokio::test]
async fn a_librarys_dry_run_counts_and_removes_nothing_and_the_removal_removes() {
    let (router, state, fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let b = the_library(&listed, "root-b");
    let id = b["id"].as_i64().expect("an id");
    let path = format!("/api/library/libraries/{id}");
    // A second folder, so the library's removal takes two registry slots.
    let extra = a_folder_of(&fixture, "Extra", "Gamma");
    let (status, added) = answered(
        &router,
        Method::POST,
        &format!("{path}/folders"),
        &serde_json::json!({ "path": extra }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let listed = get_json(&router, "/api/library/libraries").await;
    let folders: Vec<i64> = the_library(&listed, "root-b")["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .map(|f| f["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(folders.len(), 2);
    let other = the_library(&listed, "root-a")["folders"][0]["id"]
        .as_i64()
        .expect("an id");
    for slot in folders.iter().copied().chain([other]) {
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
        vec!["notes", "positions", "works"]
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
        for folder in &folders {
            assert!(
                !scans.contains_key(folder),
                "a removed folder's id can be given to the next one added"
            );
        }
        assert!(scans.contains_key(&other), "another library's slot stays");
    }

    let (status, missing) = answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
}

#[tokio::test]
async fn removing_an_accounts_only_library_is_a_409_in_the_stores_words() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    {
        let conn = state.library.conn();
        let reader = taiyaku_library::account::add(&conn, "reader", "pw", "user").expect("adds");
        taiyaku_library::account::grant_library(&conn, reader, id).expect("grants");
    }
    let slot = the_library(&listed, "root-b")["folders"][0]["id"]
        .as_i64()
        .expect("an id");
    state.scans.lock().unwrap().insert(
        slot,
        crate::ScanProgress {
            started: std::time::Instant::now(),
            done: true,
            report: Some(serde_json::json!({})),
        },
    );
    let stored = the_store(&state);
    let sentence = taiyaku_library::scan::library_removal_refusal(&state.library.conn(), id)
        .expect("asks")
        .expect("is refused");

    for path in [
        format!("/api/library/libraries/{id}?dry_run=true"),
        format!("/api/library/libraries/{id}"),
    ] {
        let (status, refused) =
            answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}: {refused}");
        assert_eq!(the_sentence(&refused), sentence, "{path}");
        assert!(
            sentence.contains("`root-b`") && sentence.contains("`reader`"),
            "{sentence}"
        );
    }
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
    assert_eq!(
        the_store(&state),
        stored,
        "a refused removal removes nothing"
    );
    assert!(
        state.scans.lock().unwrap().contains_key(&slot),
        "and clears no slot"
    );
}

/// A folder is added to a library, or made with one. Nothing adds one alone.
/// Every other route under `/api/library/roots` still answers.
#[tokio::test]
async fn the_roots_route_no_longer_adds_a_folder_and_its_neighbours_answer() {
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
async fn a_store_fault_adding_a_folder_is_a_500_not_a_400() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("add-folder-store-fault");
    std::fs::create_dir_all(fixture.path().join("New Folder")).expect("mkdir");
    let id = {
        let conn = state.library.conn();
        conn.pragma_update(None, "query_only", true)
            .expect("sets query_only");
        taiyaku_library::libraries::list(&conn).expect("lists")[0].id
    };
    let body = serde_json::json!({ "path": fixture.path().to_string_lossy() });

    let response = crate::libraries_api::add_folder(
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
    let folder = a_folder_of(&fixture, "Volumes", "Alpha");
    {
        let conn = state.library.conn();
        conn.pragma_update(None, "query_only", true)
            .expect("sets query_only");
    }
    let body = serde_json::json!({ "name": "Manga", "folders": [folder] });

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

/// The fixture's one library, given a second folder holding another series
/// that no scan has walked. Answers the library's id and its two folder ids,
/// oldest first.
async fn a_library_with_an_unscanned_second_folder(
    router: &axum::Router,
    fixture: &test_support::Fixture,
) -> (i64, [i64; 2]) {
    let listed = get_json(router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let (status, added) = answered(
        router,
        Method::POST,
        &format!("/api/library/libraries/{id}/folders"),
        &serde_json::json!({ "path": a_folder_of(fixture, "Second", "Second Series") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let listed = get_json(router, "/api/library/libraries").await;
    let folders = listed["libraries"][0]["folders"]
        .as_array()
        .expect("folders");
    assert_eq!(folders.len(), 2, "{listed}");
    (
        id,
        [
            folders[0]["id"].as_i64().expect("an id"),
            folders[1]["id"].as_i64().expect("an id"),
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

/// [`crate::ServeState::library_job`], held by a thread of its own until this
/// is dropped. A library's scan takes the lock before it walks anything, so
/// one reserved and spawned meanwhile waits, and a test sees it running for as
/// long as it likes. A thread, not a guard in the test: a guard held across the
/// test's awaits is what `clippy::await_holding_lock` refuses.
struct HeldJob {
    release: Option<std::sync::mpsc::Sender<()>>,
    holder: Option<std::thread::JoinHandle<()>>,
}

impl Drop for HeldJob {
    fn drop(&mut self) {
        drop(self.release.take());
        if let Some(holder) = self.holder.take() {
            holder.join().expect("the holder thread ends");
        }
    }
}

fn holding_the_job(state: &std::sync::Arc<crate::ServeState>) -> HeldJob {
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (locked_tx, locked) = std::sync::mpsc::channel::<()>();
    let state = std::sync::Arc::clone(state);
    let holder = std::thread::spawn(move || {
        let _job = state.library_job.lock().expect("the job lock is free");
        locked_tx.send(()).expect("the test is waiting");
        let _ = released.recv();
    });
    locked.recv().expect("the holder took the lock");
    HeldJob {
        release: Some(release),
        holder: Some(holder),
    }
}

fn a_scan_running_on(state: &crate::ServeState, folder: i64) {
    state.scans.lock().unwrap().insert(
        folder,
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

fn a_folders_scan(folder: i64) -> String {
    format!("/api/library/roots/{folder}/scan")
}

fn scan_entries_in(state: &crate::ServeState, folder: i64) -> i64 {
    state
        .library
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM scan_entry WHERE root_id = ?1",
            [folder],
            |row| row.get(0),
        )
        .expect("counts")
}

/// The first folder's scan entries are deleted first, so that a scan which
/// does not walk it leaves it empty and the library one work short. The
/// second folder's volume has not been walked at all. Each folder's report
/// carries the derive's counts: had the derive run after the first walk, it
/// would count one work, the second folder's volume not yet being walked.
#[tokio::test]
async fn scan_now_walks_each_folder_and_derives_once() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    // The second folder holds one more volume than the first has to walk, so
    // that a walk filed under the other folder shows in its counts.
    test_support::write_pages(
        &fixture
            .path()
            .join("Second")
            .join("Second Series")
            .join("Second Series - v02"),
        2,
    );
    let paths: Vec<String> =
        get_json(&router, "/api/library/libraries").await["libraries"][0]["folders"]
            .as_array()
            .expect("folders")
            .iter()
            .map(|folder| folder["path"].as_str().expect("a path").to_string())
            .collect();
    let mut added = Vec::new();
    state
        .library
        .conn()
        .execute("DELETE FROM scan_entry WHERE root_id = ?1", [folders[0]])
        .expect("empties the first folder's entries");
    assert_eq!(scan_entries_in(&state, folders[1]), 0);

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
            "folders": [
                { "root_id": folders[0], "running": false },
                { "root_id": folders[1], "running": false },
            ],
        })
    );

    for (folder, path) in folders.into_iter().zip(&paths) {
        assert!(
            scan_entries_in(&state, folder) > 0,
            "folder {folder} walked"
        );
        let status = get_json(&router, &a_folders_scan(folder)).await;
        assert_eq!(status["running"], false, "{status}");
        assert_eq!(
            status["report"]["scan"]["root"], *path,
            "folder {folder} holds its own walk: {status}"
        );
        added.push(status["report"]["scan"]["added"].as_u64().expect("a walk"));
        assert_eq!(
            keys_of(&status["report"]),
            vec!["derive", "scan"],
            "{status}"
        );
        assert!(
            status["report"]["scan"]["added"].as_u64().expect("a walk") > 0,
            "folder {folder} was walked: {status}"
        );
        assert_eq!(
            status["report"]["derive"]["works"], 3,
            "folder {folder} carries the one derive, run after both walks: {status}"
        );
        assert_eq!(status["last_scan_status"], "ok", "{status}");
    }
    assert!(
        added[1] > added[0],
        "the second folder's walk found more than the first's: {added:?}"
    );
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(listed["libraries"][0]["works"], 3, "{listed}");
}

/// While the library's scan is waiting on the job lock, it is running: its
/// status says so for each folder, each folder's own routes answer as for a
/// scan of that folder, and a second scan of the library is refused.
#[tokio::test]
async fn while_a_librarys_scan_runs_its_folders_answer_as_scanning() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;

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
        vec!["elapsed_secs", "folders", "library_id", "running"]
    );
    assert_eq!(running["running"], true, "{running}");
    assert!(running["elapsed_secs"].is_number(), "{running}");
    assert_eq!(
        running["folders"],
        serde_json::json!([
            { "root_id": folders[0], "running": true },
            { "root_id": folders[1], "running": true },
        ])
    );
    for folder in folders {
        let own = get_json(&router, &a_folders_scan(folder)).await;
        assert_eq!(own["running"], true, "{own}");
        assert!(own["elapsed_secs"].is_number(), "{own}");
        assert!(own["report"].is_null(), "{own}");
        let (status, refused) = answered(
            &router,
            Method::POST,
            &a_folders_scan(folder),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "folder {folder}: {refused}");
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
    for folder in folders {
        let own = get_json(&router, &a_folders_scan(folder)).await;
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

/// A scan of one folder already running refuses the library's, names that
/// folder and reserves nothing for the other.
#[tokio::test]
async fn a_scan_of_one_folder_refuses_the_librarys_and_reserves_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    a_scan_running_on(&state, folders[1]);

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
        "names the folder: {refused}"
    );
    assert!(!state.scans.lock().unwrap().contains_key(&folders[0]));
    let running = get_json(&router, &the_scan_of(id)).await;
    assert_eq!(running["running"], true, "{running}");
    assert!(running["elapsed_secs"].is_number(), "{running}");
    assert_eq!(
        running["folders"],
        serde_json::json!([
            { "root_id": folders[0], "running": false },
            { "root_id": folders[1], "running": true },
        ])
    );
}

/// A scan of a folder of another library does not touch this library's, and
/// the library's scan leaves another library's folder free.
#[tokio::test]
async fn a_scan_in_another_library_neither_blocks_nor_is_blocked() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({
            "name": "Elsewhere",
            "folders": [a_folder_of(&fixture, "Elsewhere", "Else Series")],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let other = made["library"]["id"].as_i64().expect("an id");
    let other_folder = made["library"]["folders"][0]["id"].as_i64().expect("an id");

    // Another library's folder is being scanned: this library's scan starts.
    a_scan_running_on(&state, other_folder);
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

    // This library's scan is held running: another library's folder scans.
    state.scans.lock().unwrap().remove(&other_folder);
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
        &a_folders_scan(other_folder),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let other_status = get_json(&router, &the_scan_of(other)).await;
    assert_eq!(other_status["running"], true, "{other_status}");
    assert_eq!(
        get_json(&router, &the_scan_of(id)).await["folders"],
        serde_json::json!([
            { "root_id": folders[0], "running": true },
            { "root_id": folders[1], "running": true },
        ])
    );
    drop(held);
    until_the_library_scan_ends(&router, id).await;
    until_the_library_scan_ends(&router, other).await;
}

/// The first folder's directory has gone: the walk fails, the scan ends there,
/// so the second folder is never walked and nothing is derived, and each
/// folder's entry ends done with the one error.
#[tokio::test]
async fn a_failed_walk_ends_the_scan_and_every_folder_reports_the_error() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let gone = listed["libraries"][0]["folders"][0]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    std::fs::remove_dir_all(&gone).expect("removes the first folder");

    let (status, _) = answered(
        &router,
        Method::POST,
        &the_scan_of(id),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let ended = until_the_library_scan_ends(&router, id).await;

    assert_eq!(ended["folders"].as_array().expect("folders").len(), 2);
    for folder in folders {
        let own = get_json(&router, &a_folders_scan(folder)).await;
        assert_eq!(own["running"], false, "{own}");
        assert_eq!(keys_of(&own["report"]), vec!["error"], "{own}");
        let error = own["report"]["error"].as_str().expect("a sentence");
        assert!(
            error.contains(&gone),
            "names the folder that failed: {error}"
        );
    }
    assert_eq!(scan_entries_in(&state, folders[1]), 0, "never walked");
    let never = get_json(&router, &a_folders_scan(folders[1])).await;
    assert!(
        never["last_scan_status"].is_null(),
        "a folder the scan never reached keeps no status: {never}"
    );
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        listed["libraries"][0]["works"], 1,
        "nothing derived: {listed}"
    );
}

/// The second folder fails after the first was walked, and the first holds
/// new material: the first's `last_scan_status` no longer reads `ok`, since
/// what its walk found was not derived, and nothing is derived: no work or
/// series appears for the first's new volume. The error names the folder
/// that failed.
#[tokio::test]
async fn a_failed_second_walk_corrects_the_first_folders_status_and_derives_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let first_path = listed["libraries"][0]["folders"][0]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    let gone = listed["libraries"][0]["folders"][1]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    test_support::write_pages(
        &std::path::Path::new(&first_path)
            .join("First Extra Series")
            .join("First Extra Series - v01"),
        2,
    );
    std::fs::remove_dir_all(&gone).expect("removes the second folder");
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
            [folders[0]],
            |row| row.get(0),
        )
        .expect("counts");
    assert!(walked > 0, "the first folder's new series was walked");
    assert_eq!(counts(&state), before, "nothing was derived");
    let first = get_json(&router, &a_folders_scan(folders[0])).await;
    assert_eq!(first["last_scan_status"], "failed", "{first}");
    assert_eq!(keys_of(&first["report"]), vec!["error"], "{first}");
    let error = first["report"]["error"].as_str().expect("a sentence");
    assert!(
        error.contains(&gone) && !error.contains(&first_path),
        "names the folder that failed, not the one walked: {error}"
    );
}

/// A derive that fails after both walks: each folder's walk wrote `ok`, and
/// each is marked `failed`, with the one error as every folder's report.
#[tokio::test]
async fn a_failed_derive_marks_every_folder_and_reports_the_error() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
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
    for folder in folders {
        let own = get_json(&router, &a_folders_scan(folder)).await;
        assert_eq!(own["running"], false, "{own}");
        assert_eq!(own["last_scan_status"], "failed", "{own}");
        assert_eq!(keys_of(&own["report"]), vec!["error"], "{own}");
        let error = own["report"]["error"].as_str().expect("a sentence");
        assert!(
            error.starts_with(
                "the library's folders were scanned, but deriving its structure afterward failed:"
            ),
            "{error}"
        );
        assert!(error.contains("a planted derive fault"), "{error}");
    }
}

/// A job that panics leaves no folder running: each entry ends done with an
/// error that carries the panic's words, and the library can be scanned again.
#[tokio::test]
async fn a_panicking_scan_leaves_no_folder_running() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, id).expect("the folders")
    };

    let started = std::time::Instant::now();
    super::reserve_folders(&state, &roots, started).expect("reserves");
    assert_eq!(get_json(&router, &the_scan_of(id)).await["running"], true);
    // The job dies holding the job lock, which poisons it: the scans after it
    // must go on regardless.
    super::spawn_library_scan(std::sync::Arc::clone(&state), roots, started, |state, _| {
        let _job = state.library_job.lock().expect("the job lock is free");
        panic!("a walk went wrong")
    });
    let ended = until_the_library_scan_ends(&router, id).await;
    assert!(
        state.library_job.is_poisoned(),
        "the panic poisoned the lock"
    );

    assert_eq!(
        ended["folders"],
        serde_json::json!([
            { "root_id": folders[0], "running": false },
            { "root_id": folders[1], "running": false },
        ])
    );
    for folder in folders {
        let own = get_json(&router, &a_folders_scan(folder)).await;
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
    let own = get_json(&router, &a_folders_scan(folders[0])).await;
    assert!(own["report"]["scan"].is_object(), "{own}");
}

/// A library never scanned: nothing is running, and `elapsed_secs` is null,
/// not zero.
#[tokio::test]
async fn a_library_never_scanned_is_not_running() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let folder = listed["libraries"][0]["folders"][0]["id"]
        .as_i64()
        .expect("an id");

    let status = get_json(&router, &the_scan_of(id)).await;

    assert_eq!(
        status,
        serde_json::json!({
            "library_id": id,
            "running": false,
            "elapsed_secs": null,
            "folders": [{ "root_id": folder, "running": false }],
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
        &serde_json::json!({ "folders": ["/p"] }),
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
        (Method::POST, "/api/library/libraries/1/folders"),
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
        // A second work under b, from a folder added to it and scanned.
        let extra = fixture.path().join("b-extra");
        test_support::write_pages(&extra.join("Series D").join("Series D - v01"), 3);
        let added = taiyaku_library::scan::add_root_checked_in(
            &conn,
            &extra,
            taiyaku_library::scan::InLibrary::Id(b_id),
        )
        .expect("adds a folder to library b");
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
        serde_json::json!({ "works": 2, "positions": 3, "notes": 5 }),
        "{rehearsed}"
    );
    assert_eq!(the_store(&state), stored, "a rehearsal writes nothing");

    let (status, removed) = answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(
        removed["removed"],
        serde_json::json!({ "works": 2, "positions": 3, "notes": 5 }),
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

/// Waits for the job `spawn_library_scan` started, with a bound that fails
/// the test where a job never ends.
async fn the_job_ends(job: tokio::task::JoinHandle<()>) {
    tokio::time::timeout(std::time::Duration::from_secs(10), job)
        .await
        .expect("the scan's job ended within ten seconds")
        .expect("the scan's job did not panic outside its own catch");
}

/// A second library with one folder of its own, and the ids of both.
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
            "folders": [a_folder_of(fixture, name, &format!("{name} Series"))],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    (
        made["library"]["id"].as_i64().expect("an id"),
        made["library"]["folders"][0]["id"].as_i64().expect("an id"),
    )
}

async fn remove_library(router: &axum::Router, id: i64) {
    let (status, removed) = answered(
        router,
        Method::DELETE,
        &format!("/api/library/libraries/{id}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
}

/// A job that answers fewer reports than there are folders still releases
/// every folder, and each unmatched one holds an error.
#[tokio::test]
async fn a_job_answering_too_few_reports_releases_every_folder() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, id).expect("the folders")
    };
    let started = std::time::Instant::now();
    super::reserve_folders(&state, &roots, started).expect("reserves");

    let job = super::spawn_library_scan(std::sync::Arc::clone(&state), roots, started, |_, _| {
        Ok(vec![serde_json::json!({ "scan": "the first's" })])
    });
    the_job_ends(job).await;

    let status = get_json(&router, &the_scan_of(id)).await;
    assert_eq!(status["running"], false, "{status}");
    let first = get_json(&router, &a_folders_scan(folders[0])).await;
    assert_eq!(
        first["report"],
        serde_json::json!({ "scan": "the first's" })
    );
    let second = get_json(&router, &a_folders_scan(folders[1])).await;
    assert_eq!(second["running"], false, "{second}");
    assert_eq!(keys_of(&second["report"]), vec!["error"], "{second}");
}

/// The library is removed while its scan waits on the job lock: the job
/// walks nothing, writes no entry for the folders' freed ids, and files no row.
#[tokio::test]
async fn a_library_removed_while_its_scan_waits_leaves_nothing_behind() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folder) = a_library_of_its_own(&router, &fixture, "Doomed").await;
    let roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, id).expect("the folders")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    super::reserve_folders(&state, &roots, started).expect("reserves");
    let job = super::spawn_library_scan(
        std::sync::Arc::clone(&state),
        roots,
        started,
        super::library_scan_job(id),
    );

    remove_library(&router, id).await;
    drop(held);
    the_job_ends(job).await;

    assert!(
        !state.scans.lock().unwrap().contains_key(&folder),
        "no entry for the freed folder id"
    );
}

/// The same, with a new library's folder taking the freed id before the job
/// runs: that folder is never scanned, and answers as it did.
#[tokio::test]
async fn a_scan_that_waited_through_a_removal_does_not_touch_the_folder_that_took_its_id() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folder) = a_library_of_its_own(&router, &fixture, "Doomed").await;
    let roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, id).expect("the folders")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    super::reserve_folders(&state, &roots, started).expect("reserves");
    let job = super::spawn_library_scan(
        std::sync::Arc::clone(&state),
        roots,
        started,
        super::library_scan_job(id),
    );

    remove_library(&router, id).await;
    let (fresh, fresh_folder) = a_library_of_its_own(&router, &fixture, "Fresh").await;
    assert_eq!(fresh_folder, folder, "the freed id is the next folder's");
    drop(held);
    the_job_ends(job).await;

    assert_eq!(scan_entries_in(&state, fresh_folder), 0, "not walked");
    let status = get_json(&router, &a_folders_scan(fresh_folder)).await;
    assert_eq!(status["running"], false, "{status}");
    assert!(status["report"].is_null(), "{status}");
    assert!(status["last_scan_status"].is_null(), "{status}");
    let library = get_json(&router, &the_scan_of(fresh)).await;
    assert_eq!(library["running"], false, "{library}");
}

/// A folder added to the library while its scan waits makes what the job
/// captured stale: nothing is walked, and each captured folder ends with an
/// error saying so.
#[tokio::test]
async fn a_scan_that_waited_through_a_new_folder_walks_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, id).expect("the folders")
    };
    let captured = roots[0].id;
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    super::reserve_folders(&state, &roots, started).expect("reserves");
    let job = super::spawn_library_scan(
        std::sync::Arc::clone(&state),
        roots,
        started,
        super::library_scan_job(id),
    );
    let (status, added) = answered(
        &router,
        Method::POST,
        &format!("/api/library/libraries/{id}/folders"),
        &serde_json::json!({ "path": a_folder_of(&fixture, "Late", "Late Series") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let late = added["root"]["id"].as_i64().expect("an id");
    drop(held);
    the_job_ends(job).await;

    let own = get_json(&router, &a_folders_scan(captured)).await;
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
        "the new folder is not walked"
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

/// The folder that took a removed library's folder's id is reserved by a scan
/// of its own while the first scan still waits: the first scan's end leaves
/// that reservation as it is.
#[tokio::test]
async fn a_scan_that_waited_through_a_removal_leaves_a_later_scans_reservation_alone() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folder) = a_library_of_its_own(&router, &fixture, "Doomed").await;
    let roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, id).expect("the folders")
    };
    let started = std::time::Instant::now();
    let held = holding_the_job(&state);
    super::reserve_folders(&state, &roots, started).expect("reserves");
    let job = super::spawn_library_scan(
        std::sync::Arc::clone(&state),
        roots,
        started,
        super::library_scan_job(id),
    );
    remove_library(&router, id).await;
    let (fresh, fresh_folder) = a_library_of_its_own(&router, &fixture, "Fresh").await;
    assert_eq!(fresh_folder, folder, "the freed id is the next folder's");
    let fresh_roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, fresh).expect("the folders")
    };
    let later = std::time::Instant::now();
    super::reserve_folders(&state, &fresh_roots, later).expect("reserves");

    drop(held);
    the_job_ends(job).await;

    let status = get_json(&router, &the_scan_of(fresh)).await;
    assert_eq!(
        status["running"], true,
        "the later scan is still running: {status}"
    );
    let scans = state.scans.lock().unwrap();
    let entry = scans.get(&fresh_folder).expect("its reservation");
    assert!(!entry.done && entry.report.is_none() && entry.started == later);
}

/// A scan of library `id` reserved and spawned while the job lock is held:
/// it waits, folders captured as of now, until the returned guard is dropped.
fn a_waiting_scan(
    state: &std::sync::Arc<crate::ServeState>,
    id: i64,
) -> (HeldJob, tokio::task::JoinHandle<()>) {
    let roots = {
        let conn = state.library.conn();
        super::folders_of(&conn, id).expect("the folders")
    };
    let held = holding_the_job(state);
    let started = std::time::Instant::now();
    super::reserve_folders(state, &roots, started).expect("reserves");
    let job = super::spawn_library_scan(
        std::sync::Arc::clone(state),
        roots,
        started,
        super::library_scan_job(id),
    );
    (held, job)
}

async fn remove_folder(router: &axum::Router, folder: i64) {
    let (status, removed) = answered(
        router,
        Method::DELETE,
        &format!("/api/library/roots/{folder}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
}

async fn add_folder_to(router: &axum::Router, library: i64, path: &str) -> i64 {
    let (status, added) = answered(
        router,
        Method::POST,
        &format!("/api/library/libraries/{library}/folders"),
        &serde_json::json!({ "path": path }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    added["root"]["id"].as_i64().expect("an id")
}

/// After a scan that was not to walk this folder: no row under it, no status
/// written, and its own route answers as it did before any scan.
async fn never_scanned(router: &axum::Router, state: &crate::ServeState, folder: i64) {
    assert_eq!(
        scan_entries_in(state, folder),
        0,
        "no row filed under folder {folder}"
    );
    let own = get_json(router, &a_folders_scan(folder)).await;
    assert_eq!(own["running"], false, "{own}");
    assert!(own["report"].is_null(), "{own}");
    assert!(own["last_scan_status"].is_null(), "{own}");
}

/// The library's newest folder is removed and another folder, of another
/// path, is added to the same library while the scan waits: the new folder
/// takes the freed id, and the scan, which was asked about the old one, walks
/// neither.
#[tokio::test]
async fn a_scan_that_waited_through_a_swapped_folder_does_not_walk_the_old_path_under_the_new() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let (held, job) = a_waiting_scan(&state, id);

    remove_folder(&router, folders[1]).await;
    let swapped = add_folder_to(&router, id, &a_folder_of(&fixture, "Third", "Third Series")).await;
    assert_eq!(swapped, folders[1], "the new folder took the freed id");
    drop(held);
    the_job_ends(job).await;

    never_scanned(&router, &state, swapped).await;
}

/// A folder removed from the library, its id taken by another library's new
/// folder, and its path added back to the first library under a new id: the
/// paths the scan captured all still stand and one id does not. The scan
/// walks nothing, so the folder that came back, which it holds no reservation
/// for, is not walked, nor is the other library's, and the folder it does
/// hold reports why.
#[tokio::test]
async fn a_scan_that_waited_through_a_folder_re_added_under_a_new_id_walks_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let listed = get_json(&router, "/api/library/libraries").await;
    let second_path = listed["libraries"][0]["folders"][1]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    let (held, job) = a_waiting_scan(&state, id);

    remove_folder(&router, folders[1]).await;
    let (_other, taken) = a_library_of_its_own(&router, &fixture, "Other").await;
    assert_eq!(taken, folders[1], "another library's folder took the id");
    let back = add_folder_to(&router, id, &second_path).await;
    assert_ne!(back, folders[1], "the path came back under a new id");
    drop(held);
    the_job_ends(job).await;

    never_scanned(&router, &state, back).await;
    never_scanned(&router, &state, taken).await;
    let kept = get_json(&router, &a_folders_scan(folders[0])).await;
    assert!(
        kept["report"]["error"]
            .as_str()
            .expect("a sentence")
            .contains("changed while"),
        "{kept}"
    );
}

/// One of the library's two folders is removed while the scan waits: the
/// one that remains is not walked, and keeps the status it had.
#[tokio::test]
async fn a_scan_that_waited_through_a_removed_folder_walks_nothing() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let before = get_json(&router, &a_folders_scan(folders[0])).await["last_scan_status"].clone();
    let (held, job) = a_waiting_scan(&state, id);

    remove_folder(&router, folders[1]).await;
    drop(held);
    the_job_ends(job).await;

    let own = get_json(&router, &a_folders_scan(folders[0])).await;
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

/// The store fails on the job's second reading of the library's folders: the
/// folders' reports hold the store's words, not a claim that the library
/// changed.
#[tokio::test]
async fn a_store_fault_reading_the_folders_again_is_reported_in_the_stores_words() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
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

    for folder in folders {
        let own = get_json(&router, &a_folders_scan(folder)).await;
        assert_eq!(own["running"], false, "{own}");
        let error = own["report"]["error"].as_str().expect("a sentence");
        assert!(!error.contains("changed while"), "{error}");
        assert!(
            error.contains("no such table"),
            "the store's words: {error}"
        );
    }
}

/// A folder's exclusions saved while the scan waits are the ones the scan
/// walks with.
#[tokio::test]
async fn a_scan_that_waited_walks_the_folders_settings_as_they_are_when_it_starts() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let (held, job) = a_waiting_scan(&state, id);

    let (status, saved) = answered(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{}", folders[1]),
        &serde_json::json!({ "exclude_globs": "Second Series" }),
    )
    .await;
    assert!(status.is_success(), "{status}: {saved}");
    drop(held);
    the_job_ends(job).await;

    let own = get_json(&router, &a_folders_scan(folders[1])).await;
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

    let response = super::library_panicked("listing libraries", &why);
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

/// A folder's path that is empty or not absolute, as the routes are sent it.
/// The store resolves a relative path against the working directory, which is
/// right for the command line and not for a request.
const PATHS_THAT_ARE_NOT_ABSOLUTE: [&str; 4] = ["", "   ", "relative/dir", "./x"];

#[tokio::test]
async fn a_library_is_not_made_of_a_folder_whose_path_is_not_absolute() {
    let (router, state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-relative");
    let good = a_folder_of(&fixture, "Volumes", "Alpha");
    let stored = the_store(&state);

    for bad in PATHS_THAT_ARE_NOT_ABSOLUTE {
        for folders in [vec![bad.to_string()], vec![good.clone(), bad.to_string()]] {
            let (status, refused) = answered(
                &router,
                Method::POST,
                "/api/library/libraries",
                &serde_json::json!({ "name": "Manga", "folders": folders }),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{folders:?}: {refused}");
            assert!(
                the_sentence(&refused).contains("must be absolute"),
                "{folders:?}: {refused}"
            );
            assert_eq!(the_store(&state), stored, "{folders:?} left a row");
        }
    }

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Manga", "folders": [good] }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an absolute path is still taken: {made}"
    );
}

#[tokio::test]
async fn a_folder_whose_path_is_not_absolute_is_not_added() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-folder-relative");
    let good = a_folder_of(&fixture, "Extra", "Gamma");
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"]
        .as_i64()
        .expect("an id");
    let path = format!("/api/library/libraries/{id}/folders");
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
/// tests here run on: a library is made with two folders, renamed, given a
/// third, scanned (a second start refused while the first is held), rehearsed
/// and removed.
#[tokio::test]
async fn the_whole_flow_over_a_file_backed_store() {
    let (router, state, fixture) = test_support::router_over_a_file_backed_library();
    assert!(
        state.library.path().is_some(),
        "the store is a file, so `working()` is a connection of its own"
    );
    let folders: Vec<String> = ["A", "B", "C"]
        .iter()
        .map(|name| a_folder_of(&fixture, &format!("more/{name}"), &format!("{name} Series")))
        .collect();

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Two", "folders": [folders[0], folders[1]] }),
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
        &format!("/api/library/libraries/{id}/folders"),
        &serde_json::json!({ "path": folders[2] }),
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
    assert_eq!(
        ended["folders"].as_array().map(Vec::len),
        Some(3),
        "{ended}"
    );

    let listed = get_json(&router, "/api/library/libraries").await;
    let two = the_library(&listed, "Two Renamed");
    assert_eq!(two["works"], 3, "{listed}");
    assert_eq!(two["books"], 0, "{listed}");
    let ids: Vec<i64> = two["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .map(|folder| folder["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(ids.len(), 3, "{listed}");
    for folder in &ids {
        let own = get_json(&router, &a_folders_scan(*folder)).await;
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
        serde_json::json!({ "notes": 0, "positions": 0, "works": 3 }),
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
        "no folder's slot outlives its library"
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
    for folder in ids {
        let rows: i64 = conn
            .query_row(
                "SELECT (SELECT count(*) FROM root WHERE id = ?1)
                      + (SELECT count(*) FROM scan_entry WHERE root_id = ?1)",
                [folder],
                |row| row.get(0),
            )
            .expect("counts");
        assert_eq!(rows, 0, "folder {folder} left rows");
    }
}
