use crate::test_support::{self, get, get_json, send_json};
use axum::http::{Method, StatusCode};

/// The fixture's one work, by slug, looked up rather than spelled out — the
/// slug is derived from a scanned directory name and is not this module's to
/// assert.
fn a_slug(state: &std::sync::Arc<crate::ServeState>) -> String {
    let conn = state.library.conn();
    conn.query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
        .expect("the fixture's one work")
}

fn grant_can_stamp(state: &std::sync::Arc<crate::ServeState>) {
    let conn = state.library.conn();
    let granted = conn
        .execute("UPDATE user SET can_stamp = 1 WHERE id = 1", [])
        .expect("granting the capability");
    assert_eq!(
        granted, 1,
        "the fixture account must actually be granted it"
    );
}

#[tokio::test]
async fn the_stamp_routes_refuse_a_reader() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz_as_a_reader();
    let slug = a_slug(&state);
    // Granted the capability and still a reader, so what refuses below is the
    // admin gate and not a missing grant.
    grant_can_stamp(&state);

    let read = get(
        &router,
        &format!("/api/library/works/{slug}/stamp-comicinfo"),
    )
    .await;
    assert_eq!(read.status(), StatusCode::FORBIDDEN, "GET");

    let written = send_json(
        &router,
        Method::POST,
        &format!("/api/library/works/{slug}/stamp-comicinfo"),
        &serde_json::json!({ "archives": [] }),
    )
    .await;
    assert_eq!(written.status(), StatusCode::FORBIDDEN, "POST");
}

/// **The capability, not the role** — §1.1's exception proved at the route
/// rather than only at `Principal`.
///
/// The fixture account is an administrator and holds `can_stamp = 0`, which
/// is the schema's default and what every existing library's administrator
/// holds after migration 13.
#[tokio::test]
async fn an_administrator_without_can_stamp_is_refused() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    let slug = a_slug(&state);

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/works/{slug}/stamp-comicinfo"),
        &serde_json::json!({ "archives": [] }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// **The refusal order, pinned.** An account that may not stamp is answered
/// the same way for a work that does not exist as for one that does; a 404
/// here and a 403 there is an existence oracle for exactly the account §1.1
/// exists to make possible.
#[tokio::test]
async fn an_administrator_without_can_stamp_cannot_tell_a_missing_work_from_a_real_one() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    let slug = a_slug(&state);

    let real = get(
        &router,
        &format!("/api/library/works/{slug}/stamp-comicinfo"),
    )
    .await;
    let missing = get(
        &router,
        "/api/library/works/no-such-work-at-all/stamp-comicinfo",
    )
    .await;

    assert_eq!(real.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        missing.status(),
        real.status(),
        "the two must be indistinguishable to an account that may not stamp"
    );
}

#[tokio::test]
async fn the_plan_route_reports_each_archive_and_the_total() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    grant_can_stamp(&state);
    let slug = a_slug(&state);

    let json = get_json(
        &router,
        &format!("/api/library/works/{slug}/stamp-comicinfo"),
    )
    .await;

    let archives = json["archives"].as_array().expect("an archives array");
    assert!(
        !archives.is_empty(),
        "the fixture's zip is stampable: {json}"
    );
    for key in ["member_id", "edition_key", "path", "bytes"] {
        assert!(
            !archives[0][key].is_null(),
            "`{key}` is missing from an archive row: {json}"
        );
    }
    assert_eq!(
        json["total_bytes"].as_u64().expect("a total"),
        archives
            .iter()
            .map(|a| a["bytes"].as_u64().expect("a size"))
            .sum::<u64>(),
        "the total is the sum of what it lists, not a separate claim: {json}"
    );
    assert_eq!(
        json["skipped_formats"],
        serde_json::json!([]),
        "the fixture's one member is a zip, so nothing was left alone: {json}"
    );
}

/// A work outside the caller's grant answers exactly as one that is not
/// there, which is `work_by_slug`'s existing rule rather than a second one.
#[tokio::test]
async fn a_slug_naming_nothing_is_a_404_for_an_account_that_may_stamp() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    grant_can_stamp(&state);

    let response = get(
        &router,
        "/api/library/works/no-such-work-at-all/stamp-comicinfo",
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stamping_rewrites_the_archive_and_a_second_plan_is_empty() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    grant_can_stamp(&state);
    let slug = a_slug(&state);
    let path = format!("/api/library/works/{slug}/stamp-comicinfo");

    let planned = get_json(&router, &path).await;
    let response = send_json(&router, Method::POST, &path, &planned).await;
    assert_eq!(response.status(), StatusCode::OK);

    let after = get_json(&router, &path).await;
    assert_eq!(
        after["archives"].as_array().expect("an array").len(),
        0,
        "the archives now carry exactly the document a stamp would write: {after}"
    );
    assert_eq!(
        after["total_bytes"].as_u64(),
        Some(0),
        "and the total goes with them: {after}"
    );
}

/// **409, not 400.** The body was well formed and the world moved under it,
/// which is a different thing from a malformed request and a caller acts on
/// it differently — re-read the plan, do not re-send this one.
#[tokio::test]
async fn a_plan_that_no_longer_holds_is_refused_with_409() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    grant_can_stamp(&state);
    let slug = a_slug(&state);
    let path = format!("/api/library/works/{slug}/stamp-comicinfo");

    let mut planned = get_json(&router, &path).await;
    let bytes = planned["archives"][0]["bytes"].as_u64().expect("a size");
    planned["archives"][0]["bytes"] = serde_json::json!(bytes + 1);

    let response = send_json(&router, Method::POST, &path, &planned).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// **What `skipped_formats` actually says, not merely that it says an array.**
///
/// C3 §6 wants the screen able to say "4 archives, and the rar left alone",
/// and a shape assertion cannot see whether it can.
#[tokio::test]
async fn the_plan_route_names_the_formats_it_will_not_write() {
    let (router, state, fixture) = test_support::router_over_a_scanned_cbz();
    grant_can_stamp(&state);
    let slug = a_slug(&state);

    // Replace the member's bytes with a RAR's signature. The row still points
    // at it, so it is still a member of this work — and still not stampable.
    let archive = fixture
        .path()
        .join("tree")
        .join("Scanned")
        .join("Scanned - v01.cbz");
    std::fs::write(
        &archive,
        b"Rar!\x1a\x07\x00padding to make a plausible file",
    )
    .expect("rewrites the fixture archive");

    let json = get_json(
        &router,
        &format!("/api/library/works/{slug}/stamp-comicinfo"),
    )
    .await;

    assert_eq!(
        json["archives"].as_array().expect("an array").len(),
        0,
        "nothing to stamp: {json}"
    );
    assert_eq!(
        json["skipped_formats"],
        serde_json::json!(["rar"]),
        "and the screen is told what was left alone and why: {json}"
    );
}

/// And a member whose bytes are gone is reported rather than swallowed.
#[tokio::test]
async fn the_plan_route_reports_a_member_it_could_not_read() {
    let (router, state, fixture) = test_support::router_over_a_scanned_cbz();
    grant_can_stamp(&state);
    let slug = a_slug(&state);

    let archive = fixture
        .path()
        .join("tree")
        .join("Scanned")
        .join("Scanned - v01.cbz");
    std::fs::remove_file(&archive).expect("removes the fixture archive");

    let json = get_json(
        &router,
        &format!("/api/library/works/{slug}/stamp-comicinfo"),
    )
    .await;

    assert_eq!(
        json["skipped_formats"],
        serde_json::json!(["unreadable"]),
        "a member nothing could open is named, not dropped: {json}"
    );
}

/// **A stamp refuses rather than queues behind a scan.**
///
/// `ServeState::library_job` is the lock a scan and a rule save already share.
/// A stamp takes it too, and it matters more here than for a save: a scan
/// re-derives while the stamp rewrites the very archives it is walking, and
/// `derive` can rekey a work under a stamp midway through moving its members'
/// digests. Tried and not waited for, so a caller learns why rather than
/// watching the clock.
#[tokio::test]
async fn a_stamp_refuses_while_another_library_job_holds_the_lock() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    grant_can_stamp(&state);
    let slug = a_slug(&state);
    let path = format!("/api/library/works/{slug}/stamp-comicinfo");

    let planned = get_json(&router, &path).await;
    assert!(
        !planned["archives"].as_array().expect("an array").is_empty(),
        "the fixture has something to stamp, so a 409 here is the lock and \
         not an empty plan: {planned}"
    );

    // **The lock is held on a thread of its own, never across an `.await`.**
    // Taking it here and holding it over `send_json` is what
    // `clippy::await-holding-lock` refuses, and rightly: a guard held across a
    // yield point is a deadlock waiting for the right scheduling. The two
    // channels are the handshake — the first says the lock is held, the second
    // releases it once the refusal has been observed.
    let held = state.library_job.clone();
    let (locked, is_locked) = std::sync::mpsc::channel::<()>();
    let (release, may_release) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let _guard = held.lock().expect("takes the library job");
        locked.send(()).expect("announces that it holds it");
        may_release.recv().expect("waits to be told to let go");
    });
    is_locked
        .recv()
        .expect("the lock is held before the request goes out");

    let response = send_json(&router, Method::POST, &path, &planned).await;

    release.send(()).expect("releases the lock");
    holder.join().expect("the holder thread finishes");

    assert_eq!(response.status(), StatusCode::CONFLICT);

    // And it is a refusal, not a loss: the same confirmation lands once the
    // lock is free.
    let after = send_json(&router, Method::POST, &path, &planned).await;
    assert_eq!(after.status(), StatusCode::OK);
}
