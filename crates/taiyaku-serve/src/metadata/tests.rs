//! `crates/taiyaku-serve/src/metadata/tests.rs`
//!
//! The plan's own sketch of this file (`docs/superpowers/plans/
//! 2026-08-27-c3-metadata-editing.md`, Task 6) names fixtures and a
//! `TestServer`-shaped client (`a_server_with_one_scanned_work`,
//! `server.as_admin`, `server.hand_document_path()`, …) that do not exist in
//! this crate — `test_support.rs` has no such type, only the free functions
//! `curation/tests.rs` already drives every route test through. What follows
//! keeps every one of the plan's assertions and reaches them the way this
//! crate's own tests do:
//!
//! - `a_server_with_one_scanned_work()` → a new fixture,
//!   [`test_support::router_over_a_single_scanned_work`] (admin) and
//!   [`test_support::router_over_a_single_scanned_work_as_a_reader`]
//!   (reader). **Not** the existing, similarly-named
//!   `router_over_a_scanned_work`/`router_over_a_scanned_work_in_a_series`:
//!   those import a hand-authored sidecar directly through
//!   `import::scanned`, which never runs a member through `scan::scan_root`
//!   and so never gives it a `root_id` — `hand::path_for` requires one, so a
//!   metadata edit against that fixture always answers `persisted: false`
//!   with no document ever reaching disk. Measured, not assumed:
//!   `patching_a_title_writes_the_row_and_the_document` failed with exactly
//!   that body the first time it ran against that fixture. The new one goes
//!   through the real walk instead — `scan::add_root` then `scan_root` then
//!   `derive`, the same as `router_over_two_suffixed_series` and its
//!   neighbours — which is what gives a work an address at all.
//! - `a_server_with_two_works_in_one_series()` → a new fixture,
//!   [`test_support::router_over_two_scanned_volumes_of_one_series`], built
//!   beside the others in that module rather than only here, on the same
//!   reasoning `router_over_two_suffixed_series` and its neighbours were: a
//!   route test and a fixture worth reusing belong together.
//! - `server.as_admin(method, path, body)` / `server.as_reader(...)` →
//!   [`test_support::send_json`] against a router already wrapped in the
//!   right principal by `router_over_*`'s own `authenticating` call.
//! - `server.json(response)` → `test_support::body_string` plus
//!   `serde_json::from_str`, the pattern `curation/tests.rs` already uses for
//!   every non-`GET` route.
//! - `server.hand_document_path()` / `.hand_document_paths()` → the two small
//!   helpers below, which read the address `taiyaku_library::hand::path_for`
//!   itself computes rather than re-deriving `<root>/.taiyaku/<key>.library.json`
//!   a second time.
//!
//! The plan's literal ids (`/api/library/members/1`, series `1`) are kept
//! only where nothing needs to resolve — `every_metadata_route_refuses_a_reader`
//! returns before any resolver runs, exactly as `curation/tests.rs`'s own
//! `every_curation_route_refuses_a_reader` does for its own literal `1`s.
//! Anywhere a request has to actually reach a row, the id is read out of the
//! fixture instead of guessed, the same way `auth/tests.rs`'s split-root
//! proof reads `split_state.works.snapshot()[0].id` rather than assuming a slug.

use crate::test_support::{self, send_json};
use axum::http::{Method, StatusCode};

/// The on-disk address of the fixture's one work's hand-fixed document,
/// unwrapped. Every fixture this module drives is a **scanned** work, which
/// is the one case [`taiyaku_library::hand::path_for`] answers `Some` for.
fn the_hand_document_path(state: &crate::ServeState) -> std::path::PathBuf {
    let conn = state.library.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work LIMIT 1", [], |row| row.get(0))
        .expect("the fixture's one work");
    taiyaku_library::hand::path_for(&conn, work_id)
        .expect("reading the work's document address")
        .expect("a scanned work always has an address")
}

/// [`the_hand_document_path`], for every work a fixture holds — what
/// `editing_one_work_in_a_series_saves_every_works_document` needs to check
/// that a series edit reached every member work's own document, not only the
/// one named in the URL.
fn the_hand_document_paths(state: &crate::ServeState) -> Vec<std::path::PathBuf> {
    let conn = state.library.conn();
    let mut stmt = conn
        .prepare("SELECT id FROM work ORDER BY id")
        .expect("preparing to list the fixture's works");
    let work_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))
        .expect("listing the fixture's works")
        .collect::<Result<_, _>>()
        .expect("reading the fixture's work ids");
    work_ids
        .into_iter()
        .map(|work_id| {
            taiyaku_library::hand::path_for(&conn, work_id)
                .expect("reading the work's document address")
                .expect("a scanned work always has an address")
        })
        .collect()
}

/// Any one member of the fixture's one work — `a_hand_set_chapter_number_is_
/// recorded_as_hand` only needs a member that exists, not a particular one.
fn any_member_id(state: &crate::ServeState) -> i64 {
    let conn = state.library.conn();
    conn.query_row("SELECT id FROM member LIMIT 1", [], |row| row.get(0))
        .expect("the fixture holds at least one member")
}

/// Every member of the fixture's one work, ordered — what
/// `editing_one_members_chapter_leaves_a_previously_set_sibling_alone` needs
/// two distinct ones to tell apart. `router_over_a_single_scanned_work`'s
/// tree has two pages, so two members, is what makes this fixture usable
/// here rather than `any_member_id`'s single one.
fn member_ids(state: &crate::ServeState) -> Vec<i64> {
    let conn = state.library.conn();
    let mut stmt = conn
        .prepare("SELECT id FROM member ORDER BY id")
        .expect("preparing to list the fixture's members");
    stmt.query_map([], |row| row.get(0))
        .expect("listing the fixture's members")
        .collect::<Result<_, _>>()
        .expect("reading the fixture's member ids")
}

/// Any one edition of the fixture's one work —
/// `patching_an_edition_writes_label_kind_language_and_a_credit` only needs
/// an edition that exists, not a particular one.
fn any_edition_id(state: &crate::ServeState) -> i64 {
    let conn = state.library.conn();
    conn.query_row("SELECT id FROM edition LIMIT 1", [], |row| row.get(0))
        .expect("the fixture holds at least one edition")
}

/// The `series_id` [`test_support::router_over_two_scanned_volumes_of_one_series`]
/// filed both its works under — read rather than assumed to be `1`, the same
/// caution `the_hand_document_path` takes over a hand-typed work id.
fn the_fixture_series_id(state: &crate::ServeState) -> i64 {
    let conn = state.library.conn();
    conn.query_row("SELECT series_id FROM work LIMIT 1", [], |row| row.get(0))
        .expect("the fixture's works are filed under a series")
}

/// The gate is on every route here, not only on one of them.
///
/// Enumerated for the reason `curation::tests::every_curation_route_refuses_a_reader`
/// (the shape this test follows) gives for enumerating its own gate: a
/// reviewer's checklist goes stale the first time someone adds a route and
/// does not think about it. Every entry here would otherwise have shipped
/// ungated with a green suite if `admin_or_forbidden` were missing from its
/// handler — see this module's own report for how that was checked rather
/// than assumed: a handler with the gate removed answers something other
/// than 403 (a 200 with `router_over_a_scanned_work_as_a_reader`'s own
/// reader-scoped material, or a 404 for an id that then has to actually be
/// resolved), and this assertion's `assert_eq!(_, FORBIDDEN)` — rather than a
/// looser "is not 200" — is what catches either.
#[tokio::test]
async fn every_metadata_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let body = serde_json::json!({});

    for (method, path) in [
        (Method::PATCH, "/api/library/works/w"),
        (Method::PATCH, "/api/library/series/1"),
        (Method::PATCH, "/api/library/editions/1"),
        (Method::PATCH, "/api/library/members/1"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path}: the credential is valid and the caller is who \
             they say, which is exactly why a 401 would be the wrong answer"
        );
    }
}

/// The route this module exists for: apply, save, re-project, in that order,
/// and the saved document is asserted on the **file**, not only on the call
/// — C3 §8's own requirement, and the property `hand::save` returning
/// `persisted: true` does not by itself prove.
#[tokio::test]
async fn patching_a_title_writes_the_row_and_the_document() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let slug = state.works.snapshot()[0].id.clone();

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({ "title": "Go-Toubun no Hanayome, volume 1" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(body["title"], "Go-Toubun no Hanayome, volume 1", "{body}");
    assert_eq!(body["persisted"], true, "{body}");

    let path = the_hand_document_path(&state);
    let saved = std::fs::read_to_string(&path).expect("the document is beside the library");
    assert!(
        saved.contains("Go-Toubun no Hanayome, volume 1"),
        "C3 §8 asks for this asserted on the file rather than on the call: {saved}"
    );
}

/// A save against a root that keeps no documents reports success and says so,
/// so a client has something to key off other than the absence of a failure.
/// `persisted: false` keeps meaning "a write was attempted and failed" rather
/// than becoming permanent furniture on every default install.
#[tokio::test]
async fn patching_a_work_in_a_root_that_keeps_no_documents_says_so() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let slug = state.works.snapshot()[0].id.clone();
    // The fixture keeps documents so the rest of this file can assert on the
    // file; this test is about the other case, so it says so.
    state
        .library
        .conn()
        .execute("UPDATE root SET keeps_documents = 0", [])
        .expect("the root keeps no documents");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({ "title": "Hand Set" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    assert_eq!(body["title"], "Hand Set", "{body}");
    assert_eq!(body["persisted"], true, "nothing was owed: {body}");
    assert_eq!(
        body["keeps_documents"], false,
        "and the answer says why: {body}"
    );
    assert!(body.get("why").is_none(), "no failure to name: {body}");
    assert!(
        !the_hand_document_path(&state).exists(),
        "and no document was written"
    );
}

/// And a root that keeps them does not carry the field at all — absent means
/// the ordinary case, the same rule `why` follows and the same one
/// `a_clean_edit_omits_why_entirely_rather_than_sending_an_empty_array` pins
/// for its own field.
#[tokio::test]
async fn patching_a_work_in_a_root_that_keeps_documents_omits_the_field() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let slug = state.works.snapshot()[0].id.clone();

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({ "title": "Hand Set" }),
    )
    .await;
    let text = test_support::body_string(response).await;
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    assert_eq!(body["persisted"], true, "{body}");
    assert!(
        body.get("keeps_documents").is_none(),
        "absent unless the root declined: {body}"
    );
}

/// `patch_edition` had appeared only in `every_metadata_route_refuses_a_reader`'s
/// 403 loop before this: one of the four shipped routes with no success-path
/// test at all, so `doc.editions.entry(key).or_default()` and the
/// `EditionKind`/`Credit` body parsing were exercised by nothing. Covers the
/// label, kind, language and a credit round-trip, and — C3 §8's own
/// requirement, as `patching_a_title_writes_the_row_and_the_document` already
/// checks for `patch_work` — that the document on disk carries them too.
#[tokio::test]
async fn patching_an_edition_writes_label_kind_language_and_a_credit() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let edition_id = any_edition_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/editions/{edition_id}"),
        &serde_json::json!({
            "label": "Full Color",
            "language": "ja",
            "kind": "official",
            "credits": [{ "role": "translator", "name": "A. Translator" }]
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    let editions = body["editions"]
        .as_object()
        .unwrap_or_else(|| panic!("a document naming at least one edition: {body}"));
    let (_, edition) = editions
        .iter()
        .next()
        .unwrap_or_else(|| panic!("the edited edition should appear in the document: {body}"));
    assert_eq!(edition["label"], "Full Color", "{body}");
    assert_eq!(edition["language"], "ja", "{body}");
    assert_eq!(edition["kind"], "official", "{body}");
    assert_eq!(edition["credits"][0]["role"], "translator", "{body}");
    assert_eq!(edition["credits"][0]["name"], "A. Translator", "{body}");

    let path = the_hand_document_path(&state);
    let saved = std::fs::read_to_string(&path).expect("the document is beside the library");
    assert!(saved.contains("Full Color"), "{saved}");
    assert!(saved.contains("A. Translator"), "{saved}");
}

/// **I6, first shape.** `edit_work_raw` re-reads the document through
/// `librarydoc::of` after `apply`/`hand::save` land, rather than answering
/// with the in-memory `doc` the handler just edited — this module's own doc
/// comment states the property four times ("answer with what the store
/// actually holds", "it sees what the store actually holds rather than what
/// it sent"). `patch_edition`'s `doc.editions.entry(key).or_default()` runs
/// unconditionally, before any field of the body is even looked at, so the
/// in-memory `doc` right after that line always carries an entry for `key` —
/// empty or not. `librarydoc::of`'s own doc comment is explicit that an
/// edition with no hand-set label, language, kind or credit is omitted
/// rather than written out as `{}`. Sending a body that sets nothing is what
/// tells the two apart: echoing the edited `doc` back would show the
/// just-inserted empty entry; re-projecting must not.
#[tokio::test]
async fn patching_an_edition_with_nothing_set_invents_no_entry_in_the_answer() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let edition_id = any_edition_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/editions/{edition_id}"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    let still_empty = body
        .get("editions")
        .and_then(serde_json::Value::as_object)
        .is_none_or(serde_json::Map::is_empty);
    assert!(
        still_empty,
        "a PATCH that set nothing must not invent an entry for the edition it \
         named — the answer must reflect what the store holds, not the \
         handler's own scratch copy of the document: {body}"
    );
}

/// **I6, second shape.** `merge_worst_case` only writes `why` into the answer
/// when `worst_case`'s fold is non-empty — `why` absent means "nothing went
/// wrong", the same property `SeriesEdit`/`WorkEdit`/`EditionEdit`/
/// `MemberEdit`'s shared "send only what changed" contract holds for a
/// request body, read the other way for a response. An ordinary edit against
/// a writable, scanned work always lands cleanly, so its answer must have no
/// `why` key at all — not a present, empty `[]`, which is the shape a client
/// that defaults an absent field would be unable to tell apart from "one
/// reason occurred and it was the empty string".
#[tokio::test]
async fn a_clean_edit_omits_why_entirely_rather_than_sending_an_empty_array() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let slug = state.works.snapshot()[0].id.clone();

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({ "title": "Renamed" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    assert_eq!(body["persisted"], true, "{body}");
    assert!(
        body.get("why").is_none(),
        "a clean edit must omit `why` entirely, not send an empty array: {body}"
    );
}

/// C3 §5: the one field a client may set but not choose the provenance of.
#[tokio::test]
async fn a_hand_set_chapter_number_is_recorded_as_hand() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let member_id = any_member_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/members/{member_id}"),
        &serde_json::json!({ "chapter": 12.5 }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(body["members"][0]["chapter"], 12.5, "{body}");
    assert_eq!(
        body["members"][0]["chapter_source"], "hand",
        "C3 §5: a hand-set chapter number is a different kind of claim from a \
         parsed one, and spec G's sync will need to know which it has. The \
         client cannot choose this value: {body}"
    );
}

/// `MemberEdit`'s own contract, matching `WorkEdit`/`EditionEdit`/`SeriesEdit`:
/// a client sends only what it changed, and an absent field leaves the
/// member's entry alone. Before this, `patch_member` dropped and re-pushed
/// the member's entry unconditionally, so `PATCH {}` — exactly what
/// `every_metadata_route_refuses_a_reader` sends as a reader — set
/// `chapter_declared`/`chapter_source` to NULL as an admin, destroying a
/// hand-set value with an empty body.
#[tokio::test]
async fn an_absent_chapter_leaves_a_previously_set_one_alone() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let member_id = any_member_id(&state);

    let first = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/members/{member_id}"),
        &serde_json::json!({ "chapter": 12.5 }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let second = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/members/{member_id}"),
        &serde_json::json!({}),
    )
    .await;
    let status = second.status();
    let text = test_support::body_string(second).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(
        body["members"][0]["chapter"], 12.5,
        "an absent `chapter` must not erase the value a previous PATCH set: {body}"
    );
    assert_eq!(
        body["members"][0]["chapter_source"], "hand",
        "the provenance must survive alongside the number: {body}"
    );
}

/// `patch_member`'s edit closure opens with `doc.members.retain(|m| m.sha256
/// != sha256)` before pushing this member's own new entry — dropping any
/// existing entry for *this* member's content while leaving every other
/// member's alone. Flip that `!=` to `==` and `retain` keeps only the entry
/// matching this member, dropping every sibling's — and
/// `librarydoc::apply_members` clears every member's chapter for the whole
/// work before restoring only what `doc.members` still names, so editing one
/// member's chapter would silently erase every other member's hand-set
/// chapter. The same data-loss shape as the `PATCH {}` finding fixed above,
/// on the other side of the same `retain` call.
///
/// **`router_over_a_library_with_two_editions`, not
/// `router_over_a_single_scanned_work`.** This module's own header warns off
/// a sidecar-imported fixture for the ordinary PATCH tests, because
/// `hand::path_for` answers `None` for one and every such test would read
/// `persisted: false` with nothing ever reaching disk — but that fixture's
/// one scanned work has exactly one member (one page, `write_pages`'s own
/// count), and this defect needs two. Nothing here asserts on `persisted` or
/// on disk at all; only the shape of `doc.members` the response reprojects,
/// which `librarydoc::of` reads from the row the same way regardless of
/// where the work came from.
#[tokio::test]
async fn editing_one_members_chapter_leaves_a_sibling_members_own_alone() {
    let (router, state, _work_id, _fixture) =
        test_support::router_over_a_library_with_two_editions();
    let ids = member_ids(&state);
    assert!(
        ids.len() >= 2,
        "the fixture must hold at least two members, or this test proves nothing: {ids:?}"
    );
    let (member_a, member_b) = (ids[0], ids[1]);

    let first = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/members/{member_a}"),
        &serde_json::json!({ "chapter": 1.0 }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let second = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/members/{member_b}"),
        &serde_json::json!({ "chapter": 2.0 }),
    )
    .await;
    let status = second.status();
    let text = test_support::body_string(second).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let chapters: std::collections::BTreeSet<u64> = body["members"]
        .as_array()
        .expect("a members array")
        .iter()
        .map(|m| m["chapter"].as_f64().expect("a chapter number") as u64)
        .collect();
    assert_eq!(
        chapters,
        [1, 2].into_iter().collect(),
        "editing member b's chapter must not drop member a's own hand-set \
         chapter: {body}"
    );
}

/// `series`, `series_title` and `external_id` are shared by every work in a
/// series, and every one of those works' documents carries the series block
/// (`librarydoc.rs`'s own module comment) — so editing one work's series must
/// save every member work's document, not only the one named in the URL.
///
/// **And it must edit the series the URL named, not mint a different one.**
/// Whole-branch review C1: `apply_series_edit` used to reach
/// `librarydoc::apply_series`, which resolves a series by matching its
/// *text* against `series_title` — never by the id already in the URL. This
/// request's `titles` names nothing the fixture's series already has (the
/// fixture holds only its scanned `sort_title`, "Go-Toubun no Hanayome"), so
/// under that resolution nothing matches, a brand-new `series` row is minted
/// for the corrected title, every work in the loop moves onto it, and
/// `forget_orphaned_series` then deletes the id the URL actually named —
/// cascading its `series_title` row with it. The response still says 200 and
/// every document still gains the new title, which is exactly why the
/// original version of this test passed while it happened: it checked the
/// documents and never checked the series row they all still point at.
#[tokio::test]
async fn editing_one_work_in_a_series_saves_every_works_document() {
    let (router, state, _fixture) = test_support::router_over_two_scanned_volumes_of_one_series();
    let series_id = the_fixture_series_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{series_id}"),
        &serde_json::json!({
            "titles": [{ "title": "五等分の花嫁", "kind": "native", "primary": true }]
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    // Both works' documents land, so `worst_case`'s fold over the two
    // `Saved`s it collects here must answer `(true, [])` — not a constant
    // that always says otherwise. A mutant that hard-codes `worst_case`'s
    // return to `(false, vec![…])` survives every test in this file that only
    // checks the *failure* shape (`persisted: false`, `why` non-empty), since
    // its own fixtures already expect exactly that; this is the one path here
    // where every document actually lands, so it is what tells a constant
    // `false` apart from a real fold.
    assert_eq!(body["persisted"], true, "{body}");
    assert!(
        body.get("why").is_none(),
        "a clean edit reaching every work in the series must omit `why` \
         entirely, not send one a constant fold invented: {body}"
    );

    for path in the_hand_document_paths(&state) {
        let saved = std::fs::read_to_string(&path).expect("each work keeps its own document");
        assert!(
            saved.contains("五等分の花嫁"),
            "a series block lives in every member work's document; saving \
             one would leave the others naming the old titles, and the first \
             rescan to read one back would undo the edit: {}",
            path.display()
        );
    }

    let conn = state.library.conn();
    let still_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE id = ?1)",
            [series_id],
            |row| row.get(0),
        )
        .expect("checking whether the series row survived");
    assert!(
        still_exists,
        "PATCH /series/{series_id} must edit that series by its id, not \
         resolve a different one by title text and delete this one out from \
         under it"
    );

    let every_work_still_under_it: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work WHERE series_id != ?1 OR series_id IS NULL",
            [series_id],
            |row| row.get(0),
        )
        .expect("counting works that moved off the original series");
    assert_eq!(
        every_work_still_under_it, 0,
        "every work the fixture shelved under {series_id} must still be \
         there — a title correction is not a membership change"
    );
}

/// The worst case, not the last one. `persisted` and `why` used to be
/// whichever work `patch_series`'s loop visited last — a series whose works
/// span two roots (`auth/tests.rs` models the same shape for a work's
/// editions) can have the *first* work's document fail to land while the
/// answer reported `persisted: true` with no `why`, because the second
/// work's own successful save overwrote it.
#[tokio::test]
async fn patch_series_answers_the_worst_case_across_its_works() {
    let (router, state, _fixture, broken_root) =
        test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots();
    let series_id = the_fixture_series_id(&state);

    // **A file where the directory must go, not a read-only root.** These
    // tests used `set_readonly` until 2026-08-29, and that premise does not
    // hold for every process: **CI runs as root, which ignores the write
    // bit**, so `hand::save` succeeded there and the `persisted: false` they
    // turn on never happened. The forge caught it on the equivalent test in
    // `taiyaku-library` and cancelled before reaching these. A plain file at
    // `<root>/.taiyaku` makes `create_dir_all` fail with `AlreadyExists`
    // (`File exists`, os error 17) for any uid, and needs no restoring
    // afterwards.
    std::fs::write(broken_root.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the first root's document directory must go");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{series_id}"),
        &serde_json::json!({
            "titles": [{ "title": "五等分の花嫁", "kind": "native", "primary": true }]
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(
        body["persisted"], false,
        "one of the two works' documents could not be written, so the whole \
         answer must say so rather than reporting whichever work the loop \
         visited last: {body}"
    );
    let why = body["why"].as_array().unwrap_or_else(|| {
        panic!("`why` should carry the reason(s) that actually occurred: {body}")
    });
    assert!(
        !why.is_empty(),
        "Task 4's review is the reason `why` exists at all; dropping it here \
         would recreate the finding it closed: {body}"
    );
}

/// `worst_case`'s AND-fold over `keeps_documents`: `Some(false)` only if
/// **every** save reported it. A series spanning an on root and an off root
/// is not wholly document-less, so the answer must omit the field entirely —
/// the same "no rooted member to ask" shape a single unrooted work gets —
/// rather than claim a decline the fold cannot support.
///
/// **Root a stays on; its sibling under root b is the one turned off —
/// deliberately not the other way round.** `router_over_two_scanned_volumes_
/// of_one_series_across_two_roots`'s own doc comment establishes that root
/// a's work is visited *first* by `patch_series`'s loop and root b's *last*,
/// so `saves.last()` here is root b's own `Saved`. A mutant that folds
/// `keeps_documents` down to `saves.last().and_then(|s| s.keeps_documents)`
/// would read that last save's `Some(false)` and answer exactly what a
/// constant `Some(false)` would; turning off the *last*-visited root rather
/// than the first is what makes both shortcuts disagree with the real
/// fold's `None` here, and so is what tells either apart from it.
#[tokio::test]
async fn patch_series_omits_keeps_documents_when_its_works_roots_disagree() {
    let (router, state, _fixture, root_a) =
        test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots();
    let series_id = the_fixture_series_id(&state);

    let canonical_root_a = root_a.canonicalize().expect("root a exists on disk");
    let turned_off = state
        .library
        .conn()
        .execute(
            "UPDATE root SET keeps_documents = 0 WHERE path != ?1",
            [canonical_root_a.to_string_lossy()],
        )
        .expect("turning off the sibling root's documents");
    assert_eq!(
        turned_off, 1,
        "exactly the sibling root must be turned off, not root a"
    );

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{series_id}"),
        &serde_json::json!({
            "titles": [{ "title": "五等分の花嫁", "kind": "native", "primary": true }]
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    assert_eq!(body["persisted"], true, "{body}");
    assert!(
        body.get("keeps_documents").is_none(),
        "one sibling's root keeps documents and the other declined; the \
         series is not wholly document-less, so the fold must not claim it \
         is: {body}"
    );
}

/// [`patch_series_omits_keeps_documents_when_its_works_roots_disagree`]'s
/// other half — every root actually declining, the one case the field may
/// carry `false` at all.
#[tokio::test]
async fn patch_series_answers_keeps_documents_false_when_every_root_declines() {
    let (router, state, _fixture, _root_a) =
        test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots();
    let series_id = the_fixture_series_id(&state);

    state
        .library
        .conn()
        .execute("UPDATE root SET keeps_documents = 0", [])
        .expect("turning off every root's documents");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{series_id}"),
        &serde_json::json!({
            "titles": [{ "title": "五等分の花嫁", "kind": "native", "primary": true }]
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    assert_eq!(body["persisted"], true, "nothing was owed: {body}");
    assert_eq!(
        body["keeps_documents"], false,
        "every sibling's root declined, so the fold may finally say so: {body}"
    );
}

/// **CRITICAL, delta review of the fix waves (`f133b3e..910b15a`).**
/// `set_series_titles` writes the corrected title text onto the id the URL
/// named, correctly. But `patch_series`'s per-work loop still applied each
/// work's document through the ordinary `librarydoc::apply`, which calls
/// `apply_series` — and `apply_series` resolves a work's series by matching
/// `doc.series.titles`'s own *text* against every `series_title` row in the
/// database, with **no `LIMIT`** and no notion of "the id this PATCH already
/// named". `series_title`'s primary key is `(series_id, title)`, so the same
/// text can sit on two different series at once, and the corrected text this
/// very request just wrote onto the URL's series is exactly such a text when
/// another series already holds it verbatim.
///
/// Reproduced here over
/// [`test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series`],
/// whose "AAA Unrelated Series" already holds the text this test corrects the
/// shared series' title to: under the code this test was written against,
/// both works resolved onto the unrelated series, and once neither was left
/// under the shared one, `forget_orphaned_series` deleted it — cascading its
/// tags and external ids, exactly the loss the reviewer's own probe measured
/// (`tags=0 external=0`) after reproducing this end to end. The response
/// still read 200, `persisted: true`, no `why`.
///
/// Two survivals must hold, independently: **`set_series_titles` must refuse
/// a title text another series already holds** (so the ambiguous state this
/// bug depends on is never created at all — checked here by the response
/// status and by the shared series' own title being left exactly as it was),
/// **and** the per-work loop must not re-resolve series identity by text even
/// if the ambiguous state existed regardless (checked independently, at the
/// unit level, by `librarydoc::tests::apply_with_series_id_does_not_resolve_by_title_even_when_the_text_collides`,
/// which drives `apply_with_series_id` directly rather than through this
/// route). Mutating away either half alone must fail one of the two.
#[tokio::test]
async fn patching_a_series_title_to_text_another_series_already_holds_does_not_delete_it() {
    let (router, state, _fixture, _root_a) =
        test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series();

    let (shared_series_id, unrelated_series_id) = {
        let conn = state.library.conn();
        let shared_series_id: i64 = conn
            .query_row(
                "SELECT DISTINCT w.series_id FROM work w JOIN series s ON s.id = w.series_id \
                 WHERE s.sort_title = 'Go-Toubun no Hanayome'",
                [],
                |r| r.get(0),
            )
            .expect("the shared series");
        let unrelated_series_id: i64 = conn
            .query_row(
                "SELECT id FROM series WHERE sort_title = 'AAA Unrelated Series'",
                [],
                |r| r.get(0),
            )
            .expect("the unrelated series");
        (shared_series_id, unrelated_series_id)
    };
    assert_ne!(
        shared_series_id, unrelated_series_id,
        "two distinct series, or this test proves nothing"
    );

    // Seeded first, so their survival is actually proven rather than assumed
    // — the reviewer's own probe measured `tags=0 external=0` only *after*
    // the series carrying them was deleted.
    let seed = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{shared_series_id}"),
        &serde_json::json!({
            "tags": ["romance"],
            "external": { "anilist": "42" }
        }),
    )
    .await;
    let seed_status = seed.status();
    let seed_text = test_support::body_string(seed).await;
    assert_eq!(seed_status, StatusCode::OK, "{seed_text}");

    // The Critical: correct the shared series' title to text the unrelated
    // series already holds, verbatim.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{shared_series_id}"),
        &serde_json::json!({
            "titles": [{ "title": "AAA Unrelated Series", "kind": "romaji", "primary": true }]
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a corrected title colliding with another series' own text must be \
         refused outright — the door this collision walks through in the \
         first place — not answered 200 while quietly resolving onto the \
         wrong series: {text}"
    );

    let conn = state.library.conn();
    let shared_still_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE id = ?1)",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("checking the shared series");
    assert!(
        shared_still_exists,
        "PATCH /series/{shared_series_id} must not delete the series it \
         named — the id in the URL — just because the corrected title text \
         collided with another series' own: {text}"
    );

    let shared_title: String = conn
        .query_row(
            "SELECT sort_title FROM series WHERE id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("reading the shared series' own title back");
    assert_eq!(
        shared_title, "Go-Toubun no Hanayome",
        "a refused edit must leave the shared series' own title exactly as \
         it was, not half-written before the loop below ever ran"
    );

    let shared_work_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work WHERE series_id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("counting works under the shared series");
    assert_eq!(
        shared_work_count, 2,
        "both volumes must still be filed under the shared series: {text}"
    );

    let tags: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work_tag JOIN work ON work.id = work_tag.work_id \
             WHERE work.series_id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("counting tags on the shared series' works");
    assert!(
        tags > 0,
        "the tags seeded above must survive the refused edit, not cascade \
         away with a series the fix must not delete: {text}"
    );

    let external: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM external_id WHERE series_id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("counting external ids on the shared series");
    assert!(
        external > 0,
        "the external id seeded above must survive the refused edit: {text}"
    );

    let unrelated_still_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE id = ?1)",
            [unrelated_series_id],
            |r| r.get(0),
        )
        .expect("checking the unrelated series");
    assert!(
        unrelated_still_exists,
        "the unrelated series must be untouched by an edit to a different \
         series entirely: {text}"
    );
    let unrelated_work_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work WHERE series_id = ?1",
            [unrelated_series_id],
            |r| r.get(0),
        )
        .expect("counting works under the unrelated series");
    assert_eq!(
        unrelated_work_count, 1,
        "the unrelated series must keep its own lone work, not gain either \
         volume from the shared one: {text}"
    );
}

/// **Half one of the Critical, pinned independently of half two's refusal —
/// the gap the delta review's own report left open.** The test above proves
/// `set_series_titles` refuses to *create* the collision through this route.
/// It cannot prove the per-work loop's own `apply_with_series_id` call is
/// what stops a *pre-existing* collision from being acted on, because that
/// refusal aborts the request before the loop ever runs — so reverting
/// `edit_series_member_raw`'s call from `librarydoc::apply_with_series_id`
/// back to the ordinary `librarydoc::apply` left the whole suite green.
///
/// This drives the collision in directly, the way a stale import or a
/// pre-refusal migration could: `series_title`'s primary key is
/// `(series_id, title)`, so a second series holding the *same* title text as
/// the shared series' own is schema-legal — nothing here weakens or bypasses
/// `set_series_titles` itself, it is simply never called, because the PATCH
/// below sends no `titles` field at all. `tags` alone still reaches every
/// work in the series through the identical per-work loop
/// (`patch_series`'s own doc comment — `reading`, `tags` and `external` flow
/// through the same `edit_series_member_raw` call as titles do), so the loop
/// runs with the collision already sitting in the database and nothing to
/// stop it from being read back through `librarydoc::of` and re-resolved.
///
/// The collision is planted on the *unrelated* series deliberately, not the
/// shared one: `find_series_by_title`'s `SELECT ... WHERE title = ?1` scans
/// the `(series_id, title)` covering index in series-id order, so the match
/// with the lower id wins regardless of which row was written first, and the
/// fixture's own doc comment establishes that the unrelated series is always
/// minted with the lower id.
#[tokio::test]
async fn patch_series_does_not_delete_it_over_a_pre_existing_title_collision() {
    let (router, state, _fixture, _root_a) =
        test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series();

    let (shared_series_id, unrelated_series_id) = {
        let conn = state.library.conn();
        let shared_series_id: i64 = conn
            .query_row(
                "SELECT DISTINCT w.series_id FROM work w JOIN series s ON s.id = w.series_id \
                 WHERE s.sort_title = 'Go-Toubun no Hanayome'",
                [],
                |r| r.get(0),
            )
            .expect("the shared series");
        let unrelated_series_id: i64 = conn
            .query_row(
                "SELECT id FROM series WHERE sort_title = 'AAA Unrelated Series'",
                [],
                |r| r.get(0),
            )
            .expect("the unrelated series");
        (shared_series_id, unrelated_series_id)
    };
    assert_ne!(
        shared_series_id, unrelated_series_id,
        "two distinct series, or this test proves nothing"
    );
    assert!(
        unrelated_series_id < shared_series_id,
        "the collision below relies on the unrelated series holding the \
         lower id — router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series's \
         own doc comment states why that always holds; if it stopped \
         holding, `find_series_by_title` would resolve the other way and \
         this test would stop discriminating without ever failing"
    );

    // Seeded before the collision exists at all, so their survival below is
    // actually proven rather than assumed — the same discipline the test
    // above follows, for the same reason: the original reproduction's own
    // probe measured `tags=0 external=0` only after the series carrying them
    // had already been deleted.
    let seed = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{shared_series_id}"),
        &serde_json::json!({
            "tags": ["romance"],
            "external": { "anilist": "42" }
        }),
    )
    .await;
    let seed_status = seed.status();
    let seed_text = test_support::body_string(seed).await;
    assert_eq!(seed_status, StatusCode::OK, "{seed_text}");

    // The schema-legal collision `set_series_titles` exists to refuse,
    // planted directly rather than through any route: the unrelated series
    // gains a second `series_title` row naming the exact text the shared
    // series already holds as its own primary title.
    let shared_title = {
        let conn = state.library.conn();
        let (title, kind): (String, String) = conn
            .query_row(
                "SELECT title, kind FROM series_title WHERE series_id = ?1 AND is_primary = 1",
                [shared_series_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("the shared series' own primary title");
        conn.execute(
            "INSERT INTO series_title (series_id, title, kind, is_primary) VALUES (?1, ?2, ?3, 0)",
            rusqlite::params![unrelated_series_id, title, kind],
        )
        .expect(
            "planting the collision directly in the database, bypassing \
             `set_series_titles`'s refusal entirely — a state the schema \
             permits and a stale import or a future relaxation of that \
             refusal could produce",
        );
        title
    };

    // No `titles` field at all: `set_series_titles` is never called, so its
    // refusal never has anything to refuse. The per-work loop below still
    // runs — `tags` alone reaches it — with the collision already resident.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{shared_series_id}"),
        &serde_json::json!({ "tags": ["isekai"] }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an edit naming no titles at all has nothing for `set_series_titles` \
         to refuse, so this must succeed — the loop below is what must \
         survive the pre-existing collision, not a refusal at the door: {text}"
    );

    let conn = state.library.conn();
    let shared_still_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE id = ?1)",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("checking the shared series");
    assert!(
        shared_still_exists,
        "PATCH /series/{shared_series_id} must not delete the series it \
         named — the id in the URL — just because some other series' own \
         `series_title` row happened to collide with its title text: {text}"
    );

    let shared_title_still_theirs: String = conn
        .query_row(
            "SELECT sort_title FROM series WHERE id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("reading the shared series' own title back");
    assert_eq!(
        shared_title_still_theirs, shared_title,
        "the shared series' own title must be untouched"
    );

    let shared_work_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work WHERE series_id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("counting works under the shared series");
    assert_eq!(
        shared_work_count, 2,
        "both volumes must still be filed under the shared series, not \
         migrated onto the unrelated one by re-resolving the collision: {text}"
    );

    let tags: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work_tag JOIN work ON work.id = work_tag.work_id \
             WHERE work.series_id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("counting tags on the shared series' works");
    assert!(
        tags > 0,
        "the tags seeded above must survive, not cascade away with a series \
         this edit must not delete: {text}"
    );

    let external: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM external_id WHERE series_id = ?1",
            [shared_series_id],
            |r| r.get(0),
        )
        .expect("counting external ids on the shared series");
    assert!(
        external > 0,
        "the external id seeded above must survive: {text}"
    );

    let unrelated_still_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE id = ?1)",
            [unrelated_series_id],
            |r| r.get(0),
        )
        .expect("checking the unrelated series");
    assert!(
        unrelated_still_exists,
        "the unrelated series must be untouched by an edit to a different \
         series entirely: {text}"
    );
    let unrelated_work_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM work WHERE series_id = ?1",
            [unrelated_series_id],
            |r| r.get(0),
        )
        .expect("counting works under the unrelated series");
    assert_eq!(
        unrelated_work_count, 1,
        "the unrelated series must keep its own lone work, not gain either \
         volume from the shared one: {text}"
    );
}

/// `patch_work`'s `series` field is a second door onto the same shared rows
/// `patch_series` exists to protect: `apply_series` upserts `series_title` on
/// the resolved series regardless of which route reached it
/// (`librarydoc.rs`'s own module comment), so a title added through
/// `/works/{slug}` must land on every sibling's own document exactly as one
/// added through `/series/{id}` already does
/// (`editing_one_work_in_a_series_saves_every_works_document`) — a case that
/// test can never reach, since it never calls this route.
#[tokio::test]
async fn patching_a_works_series_saves_every_sibling_too() {
    let (router, state, _fixture) = test_support::router_over_two_scanned_volumes_of_one_series();
    let slug = state.works.snapshot()[0].id.clone();

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({
            "series": {
                "titles": [
                    { "title": "Go-Toubun no Hanayome", "kind": "alt", "primary": true },
                    { "title": "五等分の花嫁", "kind": "native", "primary": false }
                ]
            }
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");

    for path in the_hand_document_paths(&state) {
        let saved = std::fs::read_to_string(&path).expect("each work keeps its own document");
        assert!(
            saved.contains("五等分の花嫁"),
            "`series_title` is shared across every work in the series; a \
             title added through `patch_work` must reach every sibling's own \
             document, not only the one named in the URL: {}",
            path.display()
        );
    }
}

/// The fan-out also saves every sibling in the series a work **left**, not
/// only the one it joined (`patch_work`'s own doc comment) — and nothing
/// exercised that branch until now: `patching_a_works_series_saves_every_sibling_too`
/// only moves a work within the series it is already in, so
/// `previous_series_id` and `new_series_id` are always equal there and the
/// `if previous_series_id != new_series_id` loop never runs.
///
/// Proven the way `patch_series_answers_the_worst_case_across_its_works`
/// proves its own loop reaches every work: break the *remaining* sibling's
/// root read-only, then move the *other* work out of their shared series onto
/// a brand new one. `work_a` (on `root_a`) stays behind; `work_b` moves. If
/// the branch that saves the series a work left were missing, `work_a` would
/// never be asked to save at all, and the answer would read `persisted: true`
/// with an empty `why` — `work_b`'s own save, to a writable root, is the only
/// one that would have run.
#[tokio::test]
async fn patching_a_work_out_of_a_series_saves_the_sibling_left_behind() {
    let (router, state, _fixture, root_a) =
        test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots();
    // `works[0]` is `root_a`'s volume — `router_over_two_scanned_volumes_of_one_series_across_two_roots`'s
    // own doc comment is why: `v01`/`root_a` gets the lower `work.id`.
    let leaving_slug = state.works.snapshot()[1].id.clone();

    // The same file-not-chmod technique, for the same reason.
    std::fs::write(root_a.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the left-behind work's root's document directory must go");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{leaving_slug}"),
        &serde_json::json!({
            "series": {
                "titles": [{ "title": "An Entirely Different Series", "kind": "romaji", "primary": true }]
            }
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(
        body["persisted"], false,
        "the work left behind in the old series could not have its document \
         rewritten (its root is read-only); the fan-out for the series a work \
         *left* must still reach it and fold that failure into the answer, \
         exactly as the fan-out for the series it joined already does: {body}"
    );
    let why = body["why"].as_array().unwrap_or_else(|| {
        panic!("`why` must be an array carrying the left-behind sibling's reason: {body}")
    });
    assert!(
        !why.is_empty(),
        "an empty `why` here means the left branch's `hand::save` was never \
         called at all — the exact gap this test exists to close: {body}"
    );
}

/// **A fixture-discrimination fix, not only a new assertion.** The test above
/// already asserts exactly this shape, but its fixture holds only one
/// series, so `metadata::series_of_work` returning the constant `Ok(Some(1))`
/// instead of reading `work.series_id` answers correctly by coincidence —
/// SQLite mints that lone series as id 1. Over
/// [`test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series`],
/// whose second series pushes the shared one off id 1, a caller trusting
/// that constant instead reaches the *unrelated* series' own lone work —
/// which has a writable root, so its save quietly succeeds and the real,
/// read-only-rooted sibling this test cares about is never asked at all.
#[tokio::test]
async fn patching_a_work_out_of_a_series_reaches_its_real_sibling_not_series_one() {
    let (router, state, _fixture, root_a) = test_support::router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series();

    let (shared_series_id, leaving_slug) = {
        let conn = state.library.conn();
        let shared_series_id: i64 = conn
            .query_row(
                "SELECT DISTINCT w.series_id FROM work w JOIN series s ON s.id = w.series_id \
                 WHERE s.sort_title = 'Go-Toubun no Hanayome'",
                [],
                |r| r.get(0),
            )
            .expect("the shared series");
        let leaving_slug: String = conn
            .query_row(
                "SELECT slug FROM work WHERE series_id = ?1 AND volume = 2.0",
                [shared_series_id],
                |r| r.get(0),
            )
            .expect("the work on root_b, which moves out");
        (shared_series_id, leaving_slug)
    };
    assert_ne!(
        shared_series_id, 1,
        "the unrelated series must be the one minted first, or this test does \
         not actually discriminate `series_of_work` from a constant"
    );

    // The same file-not-chmod technique, for the same reason.
    std::fs::write(root_a.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the left-behind work's root's document directory must go");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{leaving_slug}"),
        &serde_json::json!({
            "series": {
                "titles": [{ "title": "An Entirely Different Series", "kind": "romaji", "primary": true }]
            }
        }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(
        body["persisted"], false,
        "the real sibling left behind (root_a, read-only) must be the one \
         the fan-out reaches; a `series_of_work` reading a constant instead \
         of the row would fan out to the unrelated series' own lone work \
         (writable root) and never touch this one, reporting `persisted: \
         true` with nothing wrong: {body}"
    );
    let why = body["why"].as_array().unwrap_or_else(|| {
        panic!("`why` must carry the real left-behind sibling's reason: {body}")
    });
    assert!(
        !why.is_empty(),
        "the real sibling's failure must be reported: {body}"
    );
}

/// **I7.** `volume` is `patch_work`'s own field — named in C3 §5's goal
/// sentence and in `WorkEdit`'s own doc comment — and nothing sent it before
/// this test: deleting the `if let Some(volume) = edit.volume` arm left the
/// whole suite green.
#[tokio::test]
async fn patching_a_works_volume_writes_the_row_and_the_document() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let slug = state.works.snapshot()[0].id.clone();

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({ "volume": 3.5 }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(body["volume"], 3.5, "{body}");

    let conn = state.library.conn();
    let stored: f64 = conn
        .query_row("SELECT volume FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reading the volume back");
    assert_eq!(stored, 3.5, "the row itself must carry the new volume");
}

/// **I7.** `SeriesEdit.reading`, applied by `apply_series_edit`'s own
/// `if let Some(reading)` arm, had no test naming it: deleting the arm left
/// the whole suite green.
#[tokio::test]
async fn patching_a_series_reading_direction_writes_the_row() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let series_id = the_fixture_series_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{series_id}"),
        &serde_json::json!({ "reading": "ltr" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(body["series"]["reading"], "ltr", "{body}");

    let conn = state.library.conn();
    let stored: String = conn
        .query_row(
            "SELECT reading FROM series WHERE id = ?1",
            [series_id],
            |row| row.get(0),
        )
        .expect("reading the series' reading direction back");
    assert_eq!(stored, "ltr");
}

/// **I7.** `SeriesEdit.tags`, applied by `apply_series_edit`'s own
/// `if let Some(tags)` arm, had no test naming it: deleting the arm left the
/// whole suite green.
#[tokio::test]
async fn patching_a_series_tags_writes_the_row() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let series_id = the_fixture_series_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{series_id}"),
        &serde_json::json!({ "tags": ["romance", "drama"] }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let tags: Vec<&str> = body["tags"]
        .as_array()
        .unwrap_or_else(|| panic!("a tags array: {body}"))
        .iter()
        .map(|t| t.as_str().expect("each tag is a string"))
        .collect();
    assert_eq!(
        tags,
        vec!["drama", "romance"],
        "`tags_of` orders alphabetically: {body}"
    );
}

/// **I7.** `SeriesEdit.external`, applied by `apply_series_edit`'s own
/// `if let Some(external)` arm, had no test naming it: deleting the arm left
/// the whole suite green.
#[tokio::test]
async fn patching_a_series_external_ids_writes_the_row() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let series_id = the_fixture_series_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/series/{series_id}"),
        &serde_json::json!({ "external": { "anilist": "97852" } }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(body["external"]["anilist"], "97852", "{body}");

    let conn = state.library.conn();
    let stored: String = conn
        .query_row(
            "SELECT external_id FROM external_id WHERE series_id = ?1 AND provider = 'anilist'",
            [series_id],
            |row| row.get(0),
        )
        .expect("reading the external id back");
    assert_eq!(stored, "97852");
}

/// **I7.** `patch_series`'s own `if works.is_empty()` check, for a series id
/// naming no work at all, had no test naming it: mutating the check to
/// `if false` leaves the loop below running zero iterations, so `last_doc`
/// stays `None` and `last_doc.expect("checked works.is_empty() above")`
/// panics — a 500 rather than the 404 an id naming nothing should answer.
#[tokio::test]
async fn patching_a_series_naming_no_works_is_a_404() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();

    let response = send_json(
        &router,
        Method::PATCH,
        "/api/library/series/999999",
        &serde_json::json!({ "reading": "ltr" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{text}");
}

/// A slug naming no work is a 404, not the 500 a `librarydoc::of` call
/// against a nonexistent `work_id` would otherwise surface as.
#[tokio::test]
async fn patching_an_unknown_work_is_a_404() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    let response = send_json(
        &router,
        Method::PATCH,
        "/api/library/works/nope",
        &serde_json::json!({ "title": "x" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// The failure `spawn_blocking` itself hands back is a **500 naming what was
/// running** — `curation::tests::a_closure_that_panicked_is_a_500_naming_
/// what_was_running`'s own shape, over `metadata::panicked` rather than
/// `curation::panicked`. Replacing this module's `panicked` with
/// `Response::default()` — a 200 with an empty body — passes every other
/// test in this file: none of the four routes' own closures offers a lever
/// to panic on purpose, since every store call hands back a `Result` and
/// `Library::conn` recovers a poisoned mutex rather than unwrapping it.
///
/// **The helper is called directly**, for the same reason: there is no route
/// to drive through that reaches this arm honestly. The `JoinError` is a real
/// one from a real `spawn_blocking` that panicked, so this is the value the
/// handlers' `Err` arms actually receive rather than a stand-in built to
/// look like it.
#[tokio::test]
async fn a_closure_that_panicked_is_a_500_naming_what_was_running() {
    let why = tokio::task::spawn_blocking(|| {
        panic!("a metadata closure, for the join error it produces");
    })
    .await
    .expect_err("a panicking closure joins as an error");

    let response = crate::metadata::panicked("editing a member", &why);
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(
        body.contains("editing a member panicked"),
        "it names what was running, not just that something did: {body}"
    );
}

/// **The grant clause still filters, proved at the function rather than
/// through a route — O86.**
///
/// O86 made an administrator unrestricted, and these four resolvers are
/// reachable only from admin-only routes. They are still *called* on every
/// edit — a sweep catches every whole-body mutant of all four — but the
/// `roots` slice they receive is now always empty, so **the clause's
/// restrictive effect became untestable through any route**, and
/// `root_grant_clause -> String::new()` survived a mutation sweep the day the
/// rule landed. Deleting the entire grant filter passed every test.
///
/// That survivor is what this test kills. The functions take `roots` and
/// filter on it deliberately — it is correct for any caller holding a grant,
/// and if the role rule is ever revisited it is the difference between a leak
/// and none — so the contract is asserted directly, with a non-empty slice no
/// route can currently supply. **A guard nothing exercises is a guard nobody
/// can trust**, and "kept for defence in depth" is a claim, not a property,
/// until something proves it.
#[test]
fn the_grant_clause_still_filters_even_though_no_route_can_supply_a_grant() {
    let library = test_support::library();
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld');
         INSERT INTO series (id, sort_title) VALUES (1, 'Series');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
         VALUES (1, 'k', 'held-v01', 1, 'Held', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (1, 1, 0, '/withheld/a.cbz', 'a', 1, 2);",
    )
    .expect("one work, wholly under the withheld root");

    assert_eq!(
        super::work_by_slug(&conn, &[], "held-v01").expect("an empty grant is unrestricted"),
        1,
        "an empty grant sees the work — the case every route now takes"
    );
    assert!(
        matches!(
            super::work_by_slug(&conn, &[1], "held-v01"),
            Err(super::NoAnswer::Missing)
        ),
        "a grant naming only the root this work is NOT under must answer \
         Missing, exactly as `browse::work` answers None (I8)"
    );
    assert_eq!(
        super::work_by_slug(&conn, &[2], "held-v01").expect("the granting root resolves it"),
        1,
        "and a grant that does cover it still resolves"
    );

    assert_eq!(
        super::works_in_series(&conn, &[], 1).expect("unrestricted"),
        vec![1],
        "the same three answers for the series listing"
    );
    assert!(
        super::works_in_series(&conn, &[1], 1)
            .expect("a grant is not an error")
            .is_empty(),
        "a grant covering none of a series' works lists none of them"
    );
    assert_eq!(
        super::works_in_series(&conn, &[2], 1).expect("the granting root"),
        vec![1]
    );
}
