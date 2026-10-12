use crate::test_support::{self, get_json, send_json};
use axum::http::{Method, StatusCode};

/// C4 §4: the list carries each account's sessions, keys and grants, because
/// the screen shows all three beside the account and a second request per row
/// is a screen that renders in stages.
#[tokio::test]
async fn the_user_list_carries_sessions_keys_and_grants() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let json = get_json(&router, "/api/library/users").await;
    let users = json["users"].as_array().expect("a users array");
    assert!(!users.is_empty(), "{json}");

    let first = &users[0];
    for key in [
        "id",
        "name",
        "role",
        "can_download",
        "can_stream",
        "can_log_in",
        "sessions",
        "keys",
        "libraries",
    ] {
        assert!(
            !first[key].is_null(),
            "`{key}` is missing from a user row: {json}"
        );
    }
    assert!(first["sessions"].is_array(), "{json}");
    assert!(first["keys"].is_array(), "{json}");
    assert!(first["libraries"].is_array(), "{json}");
    assert!(
        first.get("roots").is_none(),
        "the old key is gone, not sent beside the new one: {json}"
    );
    assert_eq!(
        first["keys"].as_array().expect("keys").len(),
        1,
        "the fixture account's own key is listed, so `keys` is not merely an \
         empty array that would pass whatever was written: {json}"
    );
}

/// A created account can log in and is a reader unless the body says
/// otherwise — `004_auth.sql` defaults the column to `user` because "any
/// insert that omitted the column minted an administrator".
#[tokio::test]
async fn creating_a_user_defaults_to_a_reader_and_never_echoes_a_hash() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("json");

    assert_eq!(body["user"]["name"], "reader");
    assert_eq!(body["user"]["role"], "user");
    assert_eq!(body["user"]["can_log_in"], true);
    assert!(
        !serde_json::to_string(&body)
            .expect("serialises")
            .contains("argon2"),
        "no PHC string may reach the wire: {body}"
    );
}

/// A body missing a required field is the caller's own mistake — 400, before
/// any store call.
#[tokio::test]
async fn creating_a_user_without_a_password_is_the_callers_mistake() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// A duplicate name is refused by `user.name`'s `UNIQUE`, and the refusal is
/// the caller's mistake rather than a 500.
#[tokio::test]
async fn creating_a_user_with_a_name_already_taken_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let body = serde_json::json!({ "name": "reader", "password": "hunter2" });

    send_json(&router, Method::POST, "/api/library/users", &body).await;
    let again = send_json(&router, Method::POST, "/api/library/users", &body).await;
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
}

/// `config_api::tests::every_config_api_route_refuses_a_reader`'s own
/// reasoning, for this module's routes: a reviewer's checklist goes stale the
/// first time somebody adds a route and does not think about it.
#[tokio::test]
async fn every_users_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let body = serde_json::json!({});

    for (method, path) in [
        (Method::GET, "/api/library/users"),
        (Method::POST, "/api/library/users"),
        (Method::PATCH, "/api/library/users/1"),
        (Method::DELETE, "/api/library/users/1"),
        (Method::DELETE, "/api/library/users/1/sessions/abc"),
        (Method::DELETE, "/api/library/users/1/keys/1"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}

/// The id of the account named `name`, read straight from the database rather
/// than from a route's own answer.
fn user_id(state: &crate::ServeState, name: &str) -> i64 {
    let conn = state.library.conn();
    conn.query_row("SELECT id FROM user WHERE name = ?1", [name], |r| r.get(0))
        .expect("the account exists")
}

/// Create `reader` through the route, and answer its id.
async fn a_reader(router: &axum::Router, state: &crate::ServeState) -> i64 {
    send_json(
        router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    user_id(state, "reader")
}

/// O54's note stops being true here: this is the first path outside a fixture
/// that can set either capability column to 0.
#[tokio::test]
async fn patching_a_user_sets_the_capability_columns_o54_could_not_reach() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "can_download": false }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let json = get_json(&router, "/api/library/users").await;
    let row = json["users"]
        .as_array()
        .expect("users")
        .iter()
        .find(|u| u["id"] == id)
        .expect("the patched account");
    assert_eq!(row["can_download"], false);
    assert_eq!(row["can_stream"], true, "an unnamed column is left alone");
}

/// PR2 §1.1: an admin's reset ends every session the target has, and leaves
/// its API keys alone. **Both halves**, since a "revoke everything" instinct
/// would break the second.
#[tokio::test]
async fn resetting_a_password_ends_the_targets_sessions_and_keeps_its_keys() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, id, "tok", None, None)
            .expect("opens a session");
        taiyaku_library::account::create_key(&conn, id, "tablet").expect("mints a key");
    }

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "password": "rotated" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let conn = state.library.conn();
    let sessions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM session WHERE user_id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("counts");
    let keys: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM api_key WHERE user_id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("counts");
    assert_eq!(sessions, 0, "every session of the target's is ended");
    assert_eq!(
        keys, 1,
        "its API keys are independent credentials and survive"
    );
}

/// C4 §4's two role refusals reach the wire as 409, not 500: the request is
/// well-formed and names a real account, and retrying it unchanged will keep
/// failing for a reason about the library's state.
#[tokio::test]
async fn the_two_role_refusals_answer_409_rather_than_500() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let admin = user_id(&state, "local");

    let demote = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{admin}"),
        &serde_json::json!({ "role": "user" }),
    )
    .await;
    assert_eq!(demote.status(), StatusCode::CONFLICT);

    let removed = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{admin}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(removed.status(), StatusCode::CONFLICT);
}

/// C4 §4: deleting a user states its counts before it happens, on the same
/// `?dry_run=true` shape PR 1 chose for a root. And the deleted account's live
/// session is refused on its next request — the cascade reaching the guard's
/// own resolver, not just the table.
#[tokio::test]
async fn deleting_a_user_quotes_its_counts_first_and_ends_its_session() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, id, "reader-token", None, None)
            .expect("opens a session");
    }

    let quoted = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{id}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(quoted.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(quoted).await).expect("json");
    assert_eq!(body["dry_run"], true);
    assert_eq!(body["removed"]["sessions"], 1);
    assert_eq!(body["removed"]["name"], "reader");
    {
        let conn = state.library.conn();
        assert!(
            taiyaku_library::account::by_id(&conn, id)
                .expect("reads")
                .is_some(),
            "a quote deletes nothing"
        );
    }

    let done = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(done.status(), StatusCode::OK);

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::session_owner(&conn, "reader-token")
            .expect("reads")
            .is_none(),
        "the deleted account's session no longer resolves to anybody"
    );
}

/// **`deny_unknown_fields` on the query struct**, which PR 1's whole-branch
/// review found the hard way: without it `?dryrun=true` — one key off — ran
/// the real, committed removal and answered 200.
#[tokio::test]
async fn a_misspelled_dry_run_is_refused_rather_than_deleting() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{id}?dryrun=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_ne!(
        response.status(),
        StatusCode::OK,
        "a misspelling must not delete"
    );

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::by_id(&conn, id)
            .expect("reads")
            .is_some(),
        "the account survives a misspelled query parameter"
    );
}

/// The library a single-scanned-work fixture's root is filed under, and its
/// name.
fn the_fixture_library(state: &crate::ServeState) -> (i64, String) {
    let conn = state.library.conn();
    conn.query_row(
        "SELECT l.id, l.name FROM root r JOIN library l ON l.id = r.library_id",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    )
    .expect("the fixture's root has a library")
}

/// A `libraries` list in the patch body writes the grants, which is PR2 §1.2's
/// whole reason for putting it there rather than on a fifth route. The answer
/// carries each library by id and name.
#[tokio::test]
async fn a_libraries_list_in_the_patch_body_writes_the_grants() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, name) = the_fixture_library(&state);
    let id = a_reader(&router, &state).await;

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "libraries": [library] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("json");

    assert_eq!(
        body["user"]["libraries"],
        serde_json::json!([{ "id": library, "name": name }]),
        "{body}"
    );
    assert!(body["user"].get("roots").is_none(), "{body}");
    let conn = state.library.conn();
    assert_eq!(
        taiyaku_library::account::libraries_for(&conn, id).expect("reads"),
        vec![library]
    );
}

/// `[]` takes every grant away, which is unrestricted, and the answer says so
/// by carrying an empty list; an absent key, in the same route, leaves the
/// grants as they were.
#[tokio::test]
async fn an_empty_libraries_list_revokes_everything_and_an_absent_key_changes_nothing() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, _name) = the_fixture_library(&state);
    let id = a_reader(&router, &state).await;
    {
        let conn = state.library.conn();
        taiyaku_library::account::grant_library(&conn, id, library).expect("grants");
    }

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "can_stream": false }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("json");
    assert_eq!(
        body["user"]["libraries"].as_array().map(Vec::len),
        Some(1),
        "an absent key left the grant: {body}"
    );
    {
        let conn = state.library.conn();
        assert_eq!(
            taiyaku_library::account::libraries_for(&conn, id).expect("reads"),
            vec![library]
        );
    }

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "libraries": [] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("json");
    assert_eq!(
        body["user"]["libraries"],
        serde_json::json!([]),
        "an empty list is an empty answer: {body}"
    );
    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::libraries_for(&conn, id)
            .expect("reads")
            .is_empty()
    );
}

/// `libraries` is an array of numbers; anything else is the caller's mistake
/// and writes nothing, whatever else the body said.
#[tokio::test]
async fn a_libraries_value_that_is_not_an_array_of_numbers_is_refused() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, _name) = the_fixture_library(&state);
    let id = a_reader(&router, &state).await;
    {
        let conn = state.library.conn();
        taiyaku_library::account::grant_library(&conn, id, library).expect("grants");
    }

    for bad in [
        serde_json::json!("everything"),
        serde_json::json!({ "id": library }),
        serde_json::json!([library, "x"]),
        serde_json::json!([1.5]),
        serde_json::Value::Null,
    ] {
        let response = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/users/{id}"),
            &serde_json::json!({ "can_stream": false, "libraries": bad }),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {text}");
        assert!(
            text.contains("`libraries` must be an array of library ids"),
            "{bad}: {text}"
        );
    }

    let conn = state.library.conn();
    assert_eq!(
        taiyaku_library::account::libraries_for(&conn, id).expect("reads"),
        vec![library],
        "no grant moved"
    );
    assert!(
        taiyaku_library::account::by_id(&conn, id)
            .expect("reads")
            .expect("the account")
            .can_stream,
        "and the other field in the body was not written either"
    );
}

/// The screen sent folder ids under `roots` until it sent library ids under
/// `libraries`. The old key is refused by name, never dropped: dropped, the
/// request would answer 200 having changed no grant.
#[tokio::test]
async fn a_roots_key_in_the_patch_body_is_refused_naming_libraries() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, _name) = the_fixture_library(&state);
    let id = a_reader(&router, &state).await;

    for body in [
        serde_json::json!({ "can_stream": false, "roots": [1] }),
        serde_json::json!({ "can_stream": false, "roots": null }),
        serde_json::json!({ "can_stream": false, "roots": [1], "libraries": [library] }),
    ] {
        let response = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/users/{id}"),
            &body,
        )
        .await;

        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {text}");
        assert!(
            text.contains("`roots` is no longer accepted: send `libraries`"),
            "{body}: {text}"
        );
        let conn = state.library.conn();
        assert!(
            taiyaku_library::account::libraries_for(&conn, id)
                .expect("reads")
                .is_empty(),
            "{body}"
        );
        assert!(
            taiyaku_library::account::by_id(&conn, id)
                .expect("reads")
                .expect("the account")
                .can_stream,
            "nothing else in {body} was written"
        );
    }
}

/// The "a device was lost" case. `account::revoke_session` is already scoped
/// to the account, so an id from one account's listing cannot end another's —
/// asserted here rather than assumed, since the route is what supplies both.
#[tokio::test]
async fn an_admin_revokes_one_session_and_not_another_accounts() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let reader = a_reader(&router, &state).await;
    let admin = user_id(&state, "local");
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, reader, "reader-token", None, None)
            .expect("opens the reader's session");
        taiyaku_library::account::open_session(&conn, admin, "admin-token", None, None)
            .expect("opens the admin's session");
    }
    let sid = {
        let conn = state.library.conn();
        taiyaku_library::account::sessions_of(&conn, reader, None).expect("lists")[0]
            .id
            .clone()
    };

    // The reader's own session id, offered under the *admin's* path, must not
    // end it: `revoke_session` is scoped to the account in the URL.
    let wrong = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{admin}/sessions/{sid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::NOT_FOUND);

    let right = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{reader}/sessions/{sid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(right.status(), StatusCode::NO_CONTENT);

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::session_owner(&conn, "reader-token")
            .expect("reads")
            .is_none(),
        "the revoked session no longer resolves"
    );
    assert!(
        taiyaku_library::account::session_owner(&conn, "admin-token")
            .expect("reads")
            .is_some(),
        "the other account's session is untouched"
    );
}

/// The same shape for an API key, which is the credential an OPDS reader
/// holds — the other half of "a device was lost".
#[tokio::test]
async fn an_admin_revokes_one_api_key() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let reader = a_reader(&router, &state).await;
    let secret = {
        let conn = state.library.conn();
        taiyaku_library::account::create_key(&conn, reader, "tablet").expect("mints a key")
    };
    let kid = {
        let conn = state.library.conn();
        taiyaku_library::account::keys_of(&conn, reader).expect("lists")[0].id
    };

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{reader}/keys/{kid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Scoped, not merely `drop`ped: the library's `conn()` is a `MutexGuard`
    // and clippy reads a binding still in lexical scope at an `.await` as held
    // across it, whatever the `drop` says.
    {
        let conn = state.library.conn();
        assert!(
            taiyaku_library::account::by_api_key(&conn, &secret)
                .expect("reads")
                .is_none(),
            "the revoked key authenticates as nobody"
        );
    }

    let again = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{reader}/keys/{kid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(
        again.status(),
        StatusCode::NOT_FOUND,
        "revoking twice is a 404"
    );
}

/// **Neither user route may write an empty password.** The CLI has refused
/// this since spec A2 and the claim route since its whole-branch review, in
/// three separate copies of one rule; these two routes were a fourth and fifth
/// writer with none of them, until the refusal moved into
/// `credential::hash_password`. Found by C4 PR 2's own whole-branch review.
///
/// A 400 on both, not a 500: clearing a field is the caller's mistake, not the
/// server breaking.
#[tokio::test]
async fn neither_user_route_will_write_an_empty_password() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();

    let created = send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "ghost", "password": "" }),
    )
    .await;
    assert_eq!(
        created.status(),
        StatusCode::BAD_REQUEST,
        "creating an account with an empty password"
    );
    {
        let conn = state.library.conn();
        assert!(
            taiyaku_library::account::id_of(&conn, "ghost")
                .expect("reads")
                .is_none(),
            "and no row was written"
        );
    }

    let id = a_reader(&router, &state).await;
    let reset = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "password": "" }),
    )
    .await;
    assert_eq!(
        reset.status(),
        StatusCode::BAD_REQUEST,
        "resetting an account to an empty password"
    );

    let conn = state.library.conn();
    let (_, stored) = taiyaku_library::account::by_name(&conn, "reader")
        .expect("reads")
        .expect("exists");
    let mut memory = taiyaku_library::credential::WorkingMemory::new();
    assert!(
        taiyaku_library::credential::verify_password_in(&mut memory, "hunter2", &stored),
        "the original password still works, so the refusal wrote nothing"
    );
    assert!(
        !taiyaku_library::credential::verify_password_in(&mut memory, "", &stored),
        "and the empty one does not"
    );
}

/// **A `PATCH` that names both a role change and a bad password applies
/// neither.** `account::update` commits its own scope and
/// `account::set_password` opens none, so ordering them the obvious way left
/// the role change committed beside a 400 — a half-applied `PATCH` on the one
/// route that edits permissions. Found by C4 PR 2's whole-branch review.
#[tokio::test]
async fn a_patch_refused_for_its_password_does_not_apply_its_other_fields() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;
    // A second administrator, so a role change here is refused for the
    // password and not for C4 §4's own invariant.
    send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "role": "admin" }),
    )
    .await;

    let refused = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "role": "user", "can_stream": false, "password": "" }),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    let conn = state.library.conn();
    let account = taiyaku_library::account::by_id(&conn, id)
        .expect("reads")
        .expect("exists");
    assert_eq!(
        account.role, "admin",
        "the role change went in with the refused password and must not have landed"
    );
    assert!(
        account.can_stream,
        "and neither did the capability change beside it"
    );
}

/// **No password hash, raw token or raw API-key secret may appear in any user
/// route's body.** `Account` carries no hash by construction and
/// `SessionSummary.id` is a sha256 — but `assembled` renders three store types
/// into one document, and this is the check that says so rather than the three
/// doc comments that assume it. C4 PR 2's whole-branch review, question 3.
#[tokio::test]
async fn no_user_route_body_carries_a_hash_a_token_or_a_key_secret() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;
    let secret = {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, id, "a-raw-session-token", None, None)
            .expect("opens a session");
        taiyaku_library::account::create_key(&conn, id, "tablet").expect("mints a key")
    };
    let stored_hash = {
        let conn = state.library.conn();
        taiyaku_library::account::by_name(&conn, "reader")
            .expect("reads")
            .expect("exists")
            .1
    };

    let listed =
        serde_json::to_string(&get_json(&router, "/api/library/users").await).expect("serialises");
    let patched = test_support::body_string(
        send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/users/{id}"),
            &serde_json::json!({ "can_stream": false }),
        )
        .await,
    )
    .await;

    for (what, body) in [("the list", &listed), ("a patch", &patched)] {
        assert!(
            !body.contains(&stored_hash),
            "{what} carried the account's PHC string"
        );
        assert!(!body.contains("$argon2"), "{what} carried a hash");
        assert!(
            !body.contains("a-raw-session-token"),
            "{what} carried a raw session token"
        );
        assert!(
            !body.contains(&secret),
            "{what} carried a raw API-key secret"
        );
        // And the fixture is not vacuous: the *hashed* forms are what a screen
        // needs and are present, so this test would notice if the sessions and
        // keys had simply not been rendered at all.
        assert!(
            body.contains(&taiyaku_library::credential::token_sha256(
                "a-raw-session-token"
            )),
            "{what} did not render the session at all, so the assertions above \
             proved nothing"
        );
        assert!(
            body.contains("tablet"),
            "{what} did not render the key at all, so `!contains(secret)` proved \
             nothing about it — the sessions half had this guard and the keys \
             half did not, which the review of the fix round caught"
        );
    }
}

/// **The two routes answer the same status for the same mistake.** Until C4
/// PR 2's whole-branch review, `account::update` returned a bare `String` and
/// this route mapped every one of them to 409 — so `POST {"role":
/// "moderator"}` was a 400 and `PATCH` of the same value was a 409, and a disk
/// fault was reported to the client as a conflict.
#[tokio::test]
async fn a_role_the_schema_refuses_is_400_on_both_routes() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;

    let created = send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "other", "password": "pw", "role": "moderator" }),
    )
    .await;
    let patched = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "role": "moderator" }),
    )
    .await;

    assert_eq!(created.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        patched.status(),
        StatusCode::BAD_REQUEST,
        "the same mistake, the same answer"
    );
}

/// A non-string `role` was swallowed by `and_then(as_str).unwrap_or("user")`,
/// so `{"role": 123}` silently created a reader and answered 200 — while the
/// patch route 400s on the identical shape. Found by the whole-branch review.
#[tokio::test]
async fn a_role_that_is_not_a_string_is_refused_rather_than_defaulted() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "numeric", "password": "pw", "role": 123 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::id_of(&conn, "numeric")
            .expect("reads")
            .is_none(),
        "and no reader was quietly created"
    );
}

/// **An administrator resets other accounts here and their own at
/// `/api/password`.** The doc's justification for ending every session — "there
/// is no session of the target's that made this one" — is exactly false when
/// the target is the caller, and the result was signing the administrator out
/// of the browser they were working in. Found by the whole-branch review.
#[tokio::test]
async fn an_administrator_cannot_reset_their_own_password_through_this_route() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let admin = user_id(&state, "local");
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, admin, "the-admins-browser", None, None)
            .expect("opens the admin's own session");
    }

    let refused = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{admin}"),
        &serde_json::json!({ "password": "rotated" }),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    assert!(
        test_support::body_string(refused)
            .await
            .contains("/api/password"),
        "the refusal names the route that does this correctly"
    );

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::session_owner(&conn, "the-admins-browser")
            .expect("reads")
            .is_some(),
        "and the administrator is still signed in"
    );
}

/// Revoking a session or key that is not this account's answered "there is no
/// account 3" on a screen showing account 3 — the right status attached to the
/// wrong noun. Found by the whole-branch review.
#[tokio::test]
async fn a_missing_session_or_key_names_itself_rather_than_the_account() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;

    // Both halves: an account that exists with no such session, and an
    // account that does not exist at all. The first wording of this refusal
    // fixed one wrong noun by asserting an account that may not be there.
    for (path, noun) in [
        (
            format!("/api/library/users/{id}/sessions/deadbeef"),
            "session",
        ),
        (format!("/api/library/users/{id}/keys/999"), "API key"),
        (
            "/api/library/users/4242/sessions/deadbeef".to_string(),
            "session",
        ),
        ("/api/library/users/4242/keys/999".to_string(), "API key"),
    ] {
        let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = test_support::body_string(response).await;
        assert!(body.contains(noun), "{path}: {body}");
        assert!(
            !body.contains("there is no account"),
            "the account exists; {path} said otherwise: {body}"
        );
    }
}

/// **`can_log_in` is the one field on a user row that is computed rather than
/// read**, and nothing asserted it was right — `the_user_list_carries_sessions_keys_and_grants`
/// only checks the key is present, so mutating `can_log_in` to `Ok(true)`, to
/// `Ok(false)`, and its comparison to `!=` all survived. Found by C4 PR 2's
/// mutation sweep.
///
/// The distinction it exists for is real and invisible otherwise:
/// `account::list`'s own doc calls it "the second half worth printing", because
/// an account holding `credential::LOCKED` has a row and no way in and nothing
/// else about it says so.
#[tokio::test]
async fn can_log_in_tells_a_locked_account_from_a_usable_one() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let usable = a_reader(&router, &state).await;
    let locked = {
        let conn = state.library.conn();
        let id = taiyaku_library::account::add(&conn, "locked", "pw", "user").expect("adds");
        conn.execute(
            "UPDATE user SET password_hash = ?1 WHERE id = ?2",
            rusqlite::params![taiyaku_library::credential::LOCKED, id],
        )
        .expect("locks the account the way migration 4 leaves the seeded row");
        id
    };

    // **Through `PATCH`, not through the list.** The list route reads the flag
    // straight off `account::list`'s own tuple; `users_api::can_log_in` — the
    // function the three mutations were in — is reached only by `patch_user`'s
    // response. A first version of this test drove the list, passed with the
    // comparison mutated to `!=`, and proved nothing. Hand-applying the
    // mutation is what caught that.
    let listed = get_json(&router, "/api/library/users").await;
    let row = |id: i64| -> serde_json::Value {
        listed["users"]
            .as_array()
            .expect("users")
            .iter()
            .find(|u| u["id"] == id)
            .expect("the account")
            .clone()
    };
    assert_eq!(row(usable)["can_log_in"], true);
    assert_eq!(row(locked)["can_log_in"], false, "{listed}");

    for (id, expected) in [(usable, true), (locked, false)] {
        let patched = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/users/{id}"),
            &serde_json::json!({ "can_stream": false }),
        )
        .await;
        assert_eq!(patched.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_str(&test_support::body_string(patched).await).expect("json");
        assert_eq!(
            body["user"]["can_log_in"], expected,
            "account {id} answered the wrong `can_log_in` from a patch: {body}"
        );
    }
}

/// **A dry run's refusal is a 409, the same as the real removal's.** Adding
/// the last-administrator check to `removal_cost` without widening how the
/// route maps its errors made the *quote* answer 500 where the commit answered
/// 409 — a routine refusal reported as a server fault, on the safety feature
/// of the destructive route. Found by re-reading the review's own finding
/// against the function one over from the one it named.
#[tokio::test]
async fn a_dry_run_and_a_real_removal_refuse_with_the_same_status() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let admin = user_id(&state, "local");

    let quoted = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{admin}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    let done = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{admin}"),
        &serde_json::json!({}),
    )
    .await;

    assert_eq!(quoted.status(), StatusCode::CONFLICT);
    assert_eq!(done.status(), StatusCode::CONFLICT);
    assert!(
        test_support::body_string(quoted)
            .await
            .contains("no administrator"),
        "and the quote says the same thing the removal would"
    );
}

/// An id naming no account is the caller's mistake on both halves of the
/// route — 404 from the existence check, never a 409 from the store.
#[tokio::test]
async fn deleting_an_account_that_does_not_exist_is_404_on_both_halves() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    for query in ["", "?dry_run=true"] {
        let response = send_json(
            &router,
            Method::DELETE,
            &format!("/api/library/users/4242{query}"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "`{query}`");
    }
}

/// **A closure that panicked is a 500 naming what was running** — the same
/// contract `curation::panicked`, `metadata::panicked` and
/// `config_api::admin_panicked` each hold, tested the same way for the same
/// reason: nothing provokes a real panic inside a `spawn_blocking` on these
/// routes, so `replace user_panicked -> Response with Default::default()` — an
/// empty 200 — survived every test until this was written. Found by C4 PR 2's
/// third mutation sweep, which is when the refactor made the mutant viable.
#[tokio::test]
async fn a_closure_that_panicked_is_a_500_naming_what_was_running() {
    let why = tokio::task::spawn_blocking(|| {
        panic!("a users closure, for the join error it produces");
    })
    .await
    .expect_err("a panicking closure joins as an error");

    let response = super::user_panicked("removing an account", &why);
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(
        body.contains("removing an account panicked"),
        "it names what was running, not just that something did: {body}"
    );
}

/// **An API key reaches none of these routes**, and this is the check that
/// says so rather than the role check that did not.
///
/// Measured before the fix: `GET /api/library/users` under an
/// administrator's `Authorization: Basic name:tyk_…` answered **200** with
/// every account's sessions, their IP addresses and their user-agents — and
/// `PATCH .../{id}` with a password would have taken over any account. Spec
/// A2's second whole-branch review closed exactly this for the self-service
/// account routes; these are the same capability over *every* account.
/// Found by the review of C4 PR 2's own fix round.
#[tokio::test]
async fn an_api_key_reaches_no_user_administration_route() {
    use tower::ServiceExt as _;
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    for (method, path) in [
        (Method::GET, "/api/library/users"),
        (Method::POST, "/api/library/users"),
        (Method::PATCH, "/api/library/users/1"),
        (Method::DELETE, "/api/library/users/1"),
        (Method::DELETE, "/api/library/users/1/sessions/abc"),
        (Method::DELETE, "/api/library/users/1/keys/1"),
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
            "{method} {path} refused without saying it was the credential kind: {body}"
        );
    }
}

/// **A library id no row holds is 400, not 500.** `user_library.library_id`
/// references `library(id)` and `INSERT OR IGNORE` does not suppress a
/// foreign-key violation, so `{"libraries": [999]}` would reach the client as
/// SQLite's own text were the id not asked about first. A list naming a
/// library that is there before one that is not writes nothing.
#[tokio::test]
async fn a_libraries_list_naming_a_library_that_does_not_exist_is_the_callers_mistake() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, _name) = the_fixture_library(&state);
    let id = a_reader(&router, &state).await;

    for listed in [serde_json::json!([999]), serde_json::json!([library, 999])] {
        let response = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/users/{id}"),
            &serde_json::json!({ "libraries": listed }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{listed}");
        assert!(
            test_support::body_string(response)
                .await
                .contains("there is no library 999"),
            "the refusal names the library that is not there: {listed}"
        );
    }

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::libraries_for(&conn, id)
            .expect("reads")
            .is_empty(),
        "and no grant was written"
    );
}

/// **A library with no folder is 409.** Granted, it would resolve to no
/// folder and the account would be unrestricted, so the patch is refused with
/// the library named and nothing is written.
#[tokio::test]
async fn a_libraries_list_naming_a_library_with_no_folder_is_refused_409() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, _name) = the_fixture_library(&state);
    let id = a_reader(&router, &state).await;
    let empty: i64 = {
        let conn = state.library.conn();
        conn.execute("INSERT INTO library (name) VALUES ('Empty')", [])
            .expect("a library with no folder, as only a hand can make one");
        conn.last_insert_rowid()
    };

    for listed in [
        serde_json::json!([empty]),
        serde_json::json!([library, empty]),
    ] {
        let response = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/users/{id}"),
            &serde_json::json!({ "libraries": listed }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{listed}");
        let body: serde_json::Value =
            serde_json::from_str(&test_support::body_string(response).await).expect("json");
        assert_eq!(
            body["error"], "the library `Empty` has no folder, so there is nothing in it to grant",
            "{body}"
        );
    }

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::libraries_for(&conn, id)
            .expect("reads")
            .is_empty(),
        "and no grant was written"
    );
}

/// A refused list undoes the rest of its own body: the rename and the
/// capability, written before the grants were reached, are what they were.
#[tokio::test]
async fn a_refused_libraries_list_leaves_the_rest_of_its_body_unwritten() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, _name) = the_fixture_library(&state);
    let id = a_reader(&router, &state).await;

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({
            "name": "renamed",
            "can_stream": false,
            "libraries": [library, 999],
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let conn = state.library.conn();
    let account = taiyaku_library::account::by_id(&conn, id)
        .expect("reads")
        .expect("the account");
    assert_eq!(account.name, "reader", "the rename was undone");
    assert!(account.can_stream, "and so was the capability");
    assert!(
        taiyaku_library::account::libraries_for(&conn, id)
            .expect("reads")
            .is_empty(),
        "and no grant was written"
    );
}

/// An administrator's own grants are written like anyone's.
#[tokio::test]
async fn an_administrator_patching_their_own_libraries_is_written() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let (library, name) = the_fixture_library(&state);
    let me = user_id(&state, test_support::FIXTURE_NAME);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{me}"),
        &serde_json::json!({ "libraries": [library] }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(
        body["user"]["libraries"],
        serde_json::json!([{ "id": library, "name": name }]),
        "{body}"
    );
    let conn = state.library.conn();
    assert_eq!(
        taiyaku_library::account::libraries_for(&conn, me).expect("reads"),
        vec![library]
    );
}

/// C4 §6.2: the expanded card shows "whether it is the browser doing the
/// asking", so a person can tell which row is the session in front of them
/// before revoking one.
///
/// **It was `false` for every session on this route.** `assembled` passed
/// `None` where `session::list_sessions` passes `token_of(&headers)`, so the
/// field shipped as a constant — on the one screen where revoking the wrong
/// row signs the administrator out of the browser they are working in.
#[tokio::test]
async fn the_user_list_marks_the_session_doing_the_asking_and_no_other() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();

    // A second account with a session of its own, so "marked" cannot pass by
    // marking everything.
    let reader = a_reader(&router, &state).await;
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(
            &conn,
            reader,
            "a-readers-token",
            Some("another browser"),
            None,
        )
        .expect("opens the reader's session");
    }

    let json = get_json(&router, "/api/library/users").await;
    let users = json["users"].as_array().expect("a users array");

    let admin = users
        .iter()
        .find(|u| u["id"] == 1)
        .expect("the fixture administrator");
    let marked = admin["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .filter(|s| s["current"] == true)
        .count();
    assert_eq!(
        marked, 1,
        "exactly the session this request arrived on is the current one: {json}"
    );

    let other = users
        .iter()
        .find(|u| u["id"] == reader)
        .expect("the reader");
    let reader_sessions = other["sessions"].as_array().expect("sessions");
    assert_eq!(reader_sessions.len(), 1, "{json}");
    assert_eq!(
        reader_sessions[0]["current"], false,
        "another account's session is never the browser doing the asking: {json}"
    );
}

/// Spec C3 §6's capability is granted from the same screen as the other two.
///
/// **`can_stamp` is written and the two beside it are left alone**, which is
/// the half a test asserting only the `200` would miss: `update`'s
/// set-builder names each column in its own `if let`, so a missing arm is a
/// silent no-op with a `200` in front of it.
#[tokio::test]
async fn patching_a_user_sets_can_stamp() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "can_stamp": true }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let json = get_json(&router, "/api/library/users").await;
    let row = json["users"]
        .as_array()
        .expect("users")
        .iter()
        .find(|u| u["id"] == id)
        .expect("the patched account");
    assert_eq!(row["can_stamp"], true, "the column was written: {json}");
    assert_eq!(
        row["can_download"], true,
        "an unnamed column is left alone: {json}"
    );
}

/// The list reports it, so the screen can render a checkbox without a second
/// request per row — the reason C4 §4 gives for the other three.
#[tokio::test]
async fn the_user_list_carries_can_stamp() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let json = get_json(&router, "/api/library/users").await;
    let first = &json["users"].as_array().expect("a users array")[0];

    assert!(
        !first["can_stamp"].is_null(),
        "`can_stamp` is missing from a user row: {json}"
    );
    assert_eq!(
        first["can_stamp"], false,
        "and the seeded administrator does not hold it: {json}"
    );
}

/// A malformed value is refused rather than coerced, as the two beside it are.
#[tokio::test]
async fn patching_a_user_refuses_a_non_boolean_can_stamp() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let id = a_reader(&router, &state).await;

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "can_stamp": "yes" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// `method uri` carrying `body`, from the browser of a second account that
/// administers. A fixture whose own account holds a grant has demoted it to a
/// reader, and these routes are an administrator's.
async fn send_as_another_administrator(
    router: &axum::Router,
    state: &crate::ServeState,
    method: Method,
    uri: &str,
    body: &serde_json::Value,
) -> axum::http::Response<axum::body::Body> {
    use tower::ServiceExt as _;

    let token = taiyaku_library::credential::mint_token();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT OR IGNORE INTO user (name, password_hash, role)
             VALUES ('curator', ?1, 'admin')",
            [test_support::FIXTURE_PHC],
        )
        .expect("adds the second administrator");
        let curator = user_id_on(&conn, "curator");
        taiyaku_library::account::open_session(&conn, curator, &token, Some("the suite"), None)
            .expect("opens the administrator's session");
    }
    router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header(
                    axum::http::header::COOKIE,
                    format!("{}={token}", crate::session::COOKIE_NAME),
                )
                .header(axum::http::header::HOST, "test")
                .header(axum::http::header::ORIGIN, "http://test")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// [`user_id`], on a connection the caller already holds.
fn user_id_on(conn: &rusqlite::Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM user WHERE name = ?1", [name], |r| r.get(0))
        .expect("the account exists")
}

/// The Users screen is sent libraries and sends library ids back. The fixture
/// account, granted `Shelf`, is listed with it by id and name, whole, and not
/// with its folders; a patch naming both libraries lists both, by name, and a
/// patch naming one keeps only that one.
#[tokio::test]
async fn the_users_screen_lists_and_replaces_an_accounts_libraries() {
    let tmp = test_support::Fixture::new("users-granted-library");
    let (router, state) = test_support::router_over_a_granted_two_folder_library(&tmp);
    let reader = user_id(&state, test_support::FIXTURE_NAME);
    let libraries_listed = |body: String| -> serde_json::Value {
        let json: serde_json::Value = serde_json::from_str(&body).expect("json");
        let account = json["users"]
            .as_array()
            .expect("users")
            .iter()
            .find(|u| u["id"] == reader)
            .expect("the fixture account")
            .clone();
        assert!(account.get("roots").is_none(), "{account}");
        account["libraries"].clone()
    };
    let nothing = serde_json::json!({});
    let bystander = {
        let conn = state.library.conn();
        let id = taiyaku_library::account::add(&conn, "bystander", "pw", "user").expect("adds");
        taiyaku_library::account::grant_library(&conn, id, 80).expect("grants Other");
        id
    };

    let listed =
        send_as_another_administrator(&router, &state, Method::GET, "/api/library/users", &nothing)
            .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        libraries_listed(test_support::body_string(listed).await),
        serde_json::json!([{ "id": 70, "name": "Shelf" }]),
        "the granted library, and not Other"
    );

    let patched = send_as_another_administrator(
        &router,
        &state,
        Method::PATCH,
        &format!("/api/library/users/{reader}"),
        &serde_json::json!({ "libraries": [80, 70] }),
    )
    .await;
    let status = patched.status();
    let body = test_support::body_string(patched).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let answered: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(
        answered["user"]["libraries"],
        serde_json::json!([{ "id": 80, "name": "Other" }, { "id": 70, "name": "Shelf" }]),
        "by name, whatever order the list was sent in"
    );

    let listed =
        send_as_another_administrator(&router, &state, Method::GET, "/api/library/users", &nothing)
            .await;
    assert_eq!(
        libraries_listed(test_support::body_string(listed).await),
        serde_json::json!([{ "id": 80, "name": "Other" }, { "id": 70, "name": "Shelf" }]),
        "the list route carries the same shape for two grants"
    );
    let listed =
        send_as_another_administrator(&router, &state, Method::GET, "/api/library/users", &nothing)
            .await;
    let json: serde_json::Value =
        serde_json::from_str(&test_support::body_string(listed).await).expect("json");
    let theirs = json["users"]
        .as_array()
        .expect("users")
        .iter()
        .find(|u| u["id"] == bystander)
        .expect("the other account")["libraries"]
        .clone();
    assert_eq!(
        theirs,
        serde_json::json!([{ "id": 80, "name": "Other" }]),
        "each row carries its own account's libraries and not another's"
    );

    let patched = send_as_another_administrator(
        &router,
        &state,
        Method::PATCH,
        &format!("/api/library/users/{reader}"),
        &serde_json::json!({ "libraries": [70] }),
    )
    .await;
    assert_eq!(patched.status(), StatusCode::OK);
    let conn = state.library.conn();
    assert_eq!(
        taiyaku_library::account::libraries_for(&conn, reader).expect("reads"),
        vec![70]
    );
}
