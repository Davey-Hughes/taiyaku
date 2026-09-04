use crate::test_support::{self, get_json, send_json};
use axum::http::{Method, StatusCode};

/// Take one, list it, download it, and open what came back as a library —
/// the whole round trip, because "the response was 200" would pass over a body
/// of zero bytes.
#[tokio::test]
async fn a_backup_is_taken_listed_and_downloaded_whole() {
    let (router, state, _fixture) = test_support::router_over_a_file_backed_library();

    let empty = get_json(&router, "/api/library/backups").await;
    assert_eq!(
        empty["backups"].as_array().expect("an array").len(),
        0,
        "a library nobody has backed up lists nothing rather than erroring"
    );

    let taken = send_json(
        &router,
        Method::POST,
        "/api/library/backups",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(taken.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(taken).await).expect("json");
    let name = body["backup"]["name"].as_str().expect("a name").to_string();
    assert!(
        body["backup"]["bytes"].as_u64().expect("bytes") > 0,
        "{body}"
    );

    let listed = get_json(&router, "/api/library/backups").await;
    assert_eq!(listed["backups"].as_array().expect("an array").len(), 1);

    let response = test_support::get(&router, &format!("/api/library/backups/{name}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("reads the body");

    let db = state
        .library
        .path()
        .expect("a file-backed library")
        .to_path_buf();
    let restored_path = taiyaku_library::backup::directory_for(&db).join("downloaded.sqlite");
    std::fs::write(&restored_path, &bytes).expect("writes what was downloaded");
    let restored = taiyaku_library::Library::open(&restored_path).expect("it opens as a library");
    let works: i64 = restored
        .conn()
        .query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))
        .expect("reads");
    assert!(
        works > 0,
        "the downloaded bytes are the library, not an empty file"
    );
}

/// **The download route resolves `{name}` against the listing and never joins
/// a caller's string onto the directory.** A traversal attempt is therefore a
/// 404 — "no such backup" — rather than something a path check has to catch.
#[tokio::test]
async fn a_name_that_is_not_in_the_listing_is_not_found() {
    let (router, _state, _fixture) = test_support::router_over_a_file_backed_library();
    send_json(
        &router,
        Method::POST,
        "/api/library/backups",
        &serde_json::json!({}),
    )
    .await;

    for name in [
        "library-19700101T000000Z.db",
        "..%2F..%2Fetc%2Fpasswd",
        "library.db",
    ] {
        let response = test_support::get(&router, &format!("/api/library/backups/{name}")).await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "`{name}` answered something other than 404"
        );
    }
}

/// Every backup route is an administrator's, for the reason C4 §5's own screen
/// is: the file is the whole database, password hashes included.
#[tokio::test]
async fn every_backup_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_file_backed_library_as_a_reader();
    let body = serde_json::json!({});

    for (method, path) in [
        (
            Method::GET,
            "/api/library/backups/library-19700101T000000Z.db",
        ),
        (Method::GET, "/api/library/backups"),
        (Method::POST, "/api/library/backups"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}

/// **An in-memory library has no file to back up, and says so.** A mutation
/// replacing `no_file_to_back_up` with `Response::default()` — a 200 with an
/// empty body — survived every test, so nothing asserted this refusal at all.
/// Found by C4 PR 2's mutation sweep.
#[tokio::test]
async fn an_in_memory_library_refuses_all_three_backup_routes() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work_in_a_browser();
    assert!(
        state.library.path().is_none(),
        "this fixture is the in-memory one, which is the point of the test"
    );

    for (method, path) in [
        (Method::GET, "/api/library/backups"),
        (Method::POST, "/api/library/backups"),
        (
            Method::GET,
            "/api/library/backups/library-19700101T000000Z.db",
        ),
    ] {
        let response = send_json(&router, method.clone(), path, &serde_json::json!({})).await;
        assert_eq!(
            response.status(),
            StatusCode::CONFLICT,
            "{method} {path} did not refuse an in-memory library"
        );
        assert!(
            test_support::body_string(response).await.contains("memory"),
            "{method} {path} refused without saying why"
        );
    }
}

/// **A store failure is a 500 with a message, not an empty 200.** A mutation
/// replacing `failed` with `Response::default()` survived every test. Provoked
/// by putting a *file* where the backup directory belongs, which is the one
/// `list` failure reachable without breaking the disk. Found by C4 PR 2's
/// mutation sweep.
#[tokio::test]
async fn a_backup_directory_that_is_a_file_answers_500_and_says_so() {
    let (router, state, _fixture) = test_support::router_over_a_file_backed_library();
    let db = state
        .library
        .path()
        .expect("a file-backed library")
        .to_path_buf();
    std::fs::write(
        taiyaku_library::backup::directory_for(&db),
        b"a file where the backup directory belongs",
    )
    .expect("writes the impostor");

    let response = send_json(
        &router,
        Method::GET,
        "/api/library/backups",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        test_support::body_string(response)
            .await
            .contains("listing backups"),
        "the 500 names what was being done"
    );
}

/// **Pressing "back up now" twice inside one second is a 409, not a 500.**
/// The name carries a whole-second timestamp, so the second run has nowhere to
/// put its file — a person's timing, not a server fault, and this route sent a
/// 500 for it until the identical misfiling was found three times over in
/// `users_api` and re-read here.
#[tokio::test]
async fn a_second_backup_in_the_same_second_is_409_and_says_why() {
    let (router, state, _fixture) = test_support::router_over_a_file_backed_library();

    let first = send_json(
        &router,
        Method::POST,
        "/api/library/backups",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(first).await).expect("json");
    let name = body["backup"]["name"].as_str().expect("a name").to_string();

    // Rather than race the clock, keep the name taken and ask again — the
    // route's own path to the collision, without a sleep in the suite.
    let db = state
        .library
        .path()
        .expect("a file-backed library")
        .to_path_buf();
    let dir = taiyaku_library::backup::directory_for(&db);
    std::fs::write(dir.join(&name), b"stand-in").expect("keeps the name taken");
    let taken_again = taiyaku_library::backup::take_named(&state.library.conn(), &dir, &name);
    assert!(
        matches!(
            taken_again,
            Err(taiyaku_library::backup::TakeError::Exists(_))
        ),
        "the store calls this a collision rather than a fault: {taken_again:?}"
    );

    // And the route turns that into 409 with the reason, which is the half a
    // person sees.
    let response = send_json(
        &router,
        Method::POST,
        "/api/library/backups",
        &serde_json::json!({}),
    )
    .await;
    if response.status() == StatusCode::OK {
        // A second ticked over between the two requests: the collision did not
        // happen, which is the ordinary case and not a failure of this test.
        return;
    }
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(
        test_support::body_string(response)
            .await
            .contains("same second"),
        "the 409 says what a person should do about it"
    );
}

/// **An API key reaches no backup route either**, and this one matters most
/// of the three: a backup is the whole database, so the download route under
/// a device credential would hand every account's password hash to whatever
/// the key was typed into. See
/// `users_api::tests::an_api_key_reaches_no_user_administration_route` for
/// what was measured before the fix.
#[tokio::test]
async fn an_api_key_reaches_no_backup_route() {
    use tower::ServiceExt as _;
    let (router, _state, _fixture) = test_support::router_over_a_file_backed_library();

    for (method, path) in [
        (Method::GET, "/api/library/backups"),
        (Method::POST, "/api/library/backups"),
        (
            Method::GET,
            "/api/library/backups/library-19700101T000000Z.db",
        ),
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
    }
}
