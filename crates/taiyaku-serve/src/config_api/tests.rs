use crate::test_support::{self, get_json, send_json};
use axum::http::{Method, StatusCode};

/// Every id currently in the `rule` table, ascending — what
/// [`the_preview_route_leaves_the_rule_table_as_it_found_it`] compares
/// before and after a preview.
fn rule_ids(state: &crate::ServeState) -> Vec<i64> {
    let conn = state.library.conn();
    let mut stmt = conn
        .prepare("SELECT id FROM rule ORDER BY id")
        .expect("prepares the rule id query");
    stmt.query_map([], |r| r.get(0))
        .expect("queries the rule ids")
        .collect::<Result<Vec<i64>, _>>()
        .expect("reads the rule ids")
}

/// `enabled`, `set_series` and `set_kind` for one rule — what
/// [`patching_a_rule_leaves_an_omitted_field_alone_but_clears_a_null_one`]
/// reads back after each patch, rather than trusting the route's own answer
/// to prove what it wrote.
fn rule_row(state: &crate::ServeState, id: i64) -> (bool, Option<String>, Option<String>) {
    let conn = state.library.conn();
    conn.query_row(
        "SELECT enabled, set_series, set_kind FROM rule WHERE id = ?1",
        [id],
        |r| Ok((r.get::<_, i64>(0)? != 0, r.get(1)?, r.get(2)?)),
    )
    .expect("reads the rule row")
}

/// `ordinal` alone — what
/// [`patching_a_rules_ordinal_writes_the_sent_number_and_an_omitted_one_leaves_it_alone`]
/// reads back, the same discipline [`rule_row`]'s own doc comment states:
/// trust the stored column, not the route's own answer.
fn rule_ordinal(state: &crate::ServeState, id: i64) -> i64 {
    let conn = state.library.conn();
    conn.query_row("SELECT ordinal FROM rule WHERE id = ?1", [id], |r| r.get(0))
        .expect("reads the rule's ordinal")
}

/// The single root [`test_support::router_over_a_single_scanned_work`]'s
/// fixture created — the real, existing root a test needs alongside a root id
/// that names nothing.
fn only_root_id(state: &crate::ServeState) -> i64 {
    let conn = state.library.conn();
    conn.query_row("SELECT id FROM root", [], |r| r.get(0))
        .expect("the fixture's own root")
}

/// The library's own rule export — [`taiyaku_library::rules::export_json`],
/// called directly rather than through HTTP: spec C4 §6 has no `GET` export
/// route, only the `POST .../import` this document is meant to round-trip
/// through.
fn exported_rules_json(state: &crate::ServeState) -> String {
    let conn = state.library.conn();
    taiyaku_library::rules::export_json(&conn).expect("exports the rule set")
}

/// A rule appended to `existing` that retitles every scan entry it reaches —
/// `kind: prefix, pattern: ""` matches every path, so `set_series` wins over
/// whatever the seeded defaults derived. No `id`, which is the ordinary shape
/// of a rule the editor has not saved yet.
fn retitle_rule(existing: &[serde_json::Value]) -> serde_json::Value {
    let max_ordinal = existing
        .iter()
        .filter_map(|r| r["ordinal"].as_i64())
        .max()
        .unwrap_or(0);
    serde_json::json!({
        "ordinal": max_ordinal + 1,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": "Retitled",
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "appended by the preview test",
    })
}

/// `POST /api/library/rules/preview`, parsed — the shared plumbing every
/// preview test below wants, so each one reads only the shape it is actually
/// about.
async fn preview_json(router: &axum::Router, body: &serde_json::Value) -> serde_json::Value {
    let response = send_json(router, Method::POST, "/api/library/rules/preview", body).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    serde_json::from_str(&text).expect("a JSON body")
}

/// An administrator, for the one test below that calls a handler directly.
fn an_administrator() -> crate::auth::Principal {
    crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::Admin,
        can_download: true,
        can_stream: true,
        // The schema's default. A `true` here would mean an administrator
        // inherits it after all — the thing
        // `auth::tests::an_administrator_does_not_inherit_can_stamp`
        // exists to refuse.
        can_stamp: false,
        access: taiyaku_library::account::Access::Every,
    }
}

/// Every route here is an administrator's, per C4's opening line: "everything
/// here is admin-gated per A2."
#[tokio::test]
async fn a_reader_cannot_read_the_rule_list() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let response = test_support::get(&router, "/api/library/rules").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// The gate is on every route here, not only on the one the test above
/// drives — `curation::tests::every_curation_route_refuses_a_reader`'s own
/// reasoning: a reviewer's checklist goes stale the first time someone adds a
/// route and does not think about it.
#[tokio::test]
async fn every_config_api_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let body = serde_json::json!({});

    for (method, path) in [
        (Method::GET, "/api/library/rules"),
        (Method::POST, "/api/library/rules"),
        // Spec C4 §4.2's save route. A *method* on a path already in this
        // list, which is exactly why it needs its own line: `guarded_routes`
        // and `auth/tests.rs`'s `expected` array are keyed by path and
        // neither of them moves when a method is added, so this loop is the
        // only thing that would notice a save route shipping ungated.
        (Method::PUT, "/api/library/rules"),
        (Method::PATCH, "/api/library/rules/1"),
        (Method::DELETE, "/api/library/rules/1"),
        (Method::POST, "/api/library/rules/preview"),
        (Method::POST, "/api/library/rules/import"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}

/// The preview route takes the whole proposed list, per C4 §6 — a preview of
/// one rule in isolation answers a question nobody asked, because layering
/// makes a rule's effect a property of its neighbours. `selected` names which
/// rule's matched-files table to build, separately from the impact and the
/// overrides (properties of the list) since only the rows are about one rule.
#[tokio::test]
async fn the_preview_route_answers_over_the_whole_proposed_list() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));
    let selected = proposed.len() - 1;

    let json = preview_json(
        &router,
        &serde_json::json!({ "rules": proposed, "selected": selected }),
    )
    .await;

    assert!(
        json["impact"]["works_rehomed"].as_u64().unwrap() > 0,
        "the appended rule retitles the scanned work's series: {json}"
    );
    assert!(json["overrides"].is_array(), "{json}");
    assert!(json["rows"].is_array(), "{json}");
    assert!(
        json["rows_truncated"].is_boolean(),
        "a capped table must say it is capped: {json}"
    );
    assert!(
        !json["rows"].as_array().unwrap().is_empty(),
        "the selected rule matched at least the scanned work's own files: {json}"
    );
}

/// **A row is its own fields, not merely an array slot.** The test just above
/// only ever checks that `json["rows"]` is a non-empty array — satisfied just
/// as well by an array of `null`s as by an array of real row objects, since a
/// JSON array's own `is_empty` counts *elements*, not whether each one carries
/// anything. This reads inside the first row instead.
#[tokio::test]
async fn a_matched_files_row_carries_its_own_fields_not_a_default_value() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));
    let selected = proposed.len() - 1;

    let json = preview_json(
        &router,
        &serde_json::json!({ "rules": proposed, "selected": selected }),
    )
    .await;

    let rows = json["rows"].as_array().expect("rows must be an array");
    let row = rows
        .first()
        .expect("the selected rule matched at least one row");
    assert!(
        row["rel_path"].is_string(),
        "a real row names the scan entry it is about: {row}"
    );
    assert!(
        row["root_id"].is_i64(),
        "a real row names the root it came from: {row}"
    );
    assert_eq!(
        row["matched"], true,
        "the fixture's own selected rule matches every entry it produced a row for: {row}"
    );
    assert!(
        row["decided_by"].is_object(),
        "a real row carries which rule decided each field: {row}"
    );
    // `decided_by`'s own values, not merely the map that holds them —
    // [`decided_json`](crate::config_api::decided_json) renders every one
    // of them, and replacing its body with `Default::default()` still
    // leaves `decided_by` a non-empty object (of `null`s), which the
    // `is_object` check above cannot tell apart from the real thing.
    assert_eq!(
        row["decided_by"]["series"],
        serde_json::json!({ "kind": "rule", "index": selected }),
        "the selected rule's own decision must name it by kind and index, not render as null: \
         {row}"
    );
}

/// `selected` is optional: the impact and the overrides stand without it, and
/// omitting it must not cost a row-building pass over every scan entry.
#[tokio::test]
async fn a_preview_with_no_rule_selected_answers_without_rows() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");
    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));

    let json = preview_json(&router, &serde_json::json!({ "rules": proposed })).await;

    assert!(json["impact"].is_object(), "{json}");
    assert!(
        json["rows"].as_array().is_none_or(std::vec::Vec::is_empty),
        "{json}"
    );
}

/// C4 §6's import takes the JSON `taiyaku library rules` exports, and
/// replaces the whole list — the same round trip the CLI already has, over
/// HTTP.
#[tokio::test]
async fn the_import_route_takes_what_the_export_produced() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let exported = exported_rules_json(&state);

    let response = test_support::send_raw(
        &router,
        Method::POST,
        "/api/library/rules/import",
        &exported,
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");

    assert_eq!(exported_rules_json(&state), exported);
}

/// A minimal `.cbz` — the same shape [`test_support::work`] writes, factored
/// out here since this is the one test in this module that scans a tree
/// rather than importing a sidecar directly.
fn write_cbz(path: &std::path::Path, pages: usize) {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).expect("creating the archive"));
    for page in 0..pages {
        zip.start_file(
            format!("{page:03}.jpg"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("starting a page entry");
        zip.write_all(format!("{}: page {page}", path.display()).as_bytes())
            .expect("writing a page entry");
    }
    zip.finish().expect("finishing the archive");
}

/// Importing a document that predates an accept orphans that proposal's
/// `created_rule_id` the same way deleting its rule by hand would. The route
/// reports the ids it reopened under `"reopened_proposals"`,
/// [`crate::config_api::delete_rule`]'s own key, rather than leaving that
/// proposal reading `accepted` with nothing deciding it.
#[tokio::test]
async fn the_import_route_reports_the_proposals_it_reopens() {
    let fixture = test_support::Fixture::new("import-reopens-proposal");
    let root_dir = fixture.path().join("root");
    write_cbz(
        &root_dir
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01.cbz"),
        3,
    );
    write_cbz(
        &root_dir
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01.cbz"),
        3,
    );

    let library = test_support::library();
    let proposal_id = {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &root_dir).expect("adding the root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scanning the root");
        taiyaku_library::derive::derive(&conn)
            .expect("the first derive, which raises the edition-of proposal");
        let proposal = taiyaku_library::proposals::open(&conn, 0.0)
            .expect("reading open proposals")
            .into_iter()
            .find(|p| p.kind == "edition-of")
            .expect("the fixture's two titles raise exactly one edition-of proposal");
        taiyaku_library::proposals::accept(&conn, proposal.id).expect("accepting the proposal");
        proposal.id
    };

    let state = std::sync::Arc::new(crate::ServeState {
        library,
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });

    // The rule set exactly as it stood before the accept above — importing
    // it back is what drops the rule the accept minted.
    let pre_accept_json = {
        let conn = state.library.conn();
        taiyaku_library::rules::export_json(&conn).expect("exporting the pre-accept rule set")
    };

    let response = crate::config_api::import_rules(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::body::Bytes::from(pre_accept_json),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("a JSON body");

    assert_eq!(
        json["reopened_proposals"],
        serde_json::json!([proposal_id]),
        "the import must report the exact proposal it reopened: {json}"
    );

    let conn = state.library.conn();
    let state_after: String = conn
        .query_row(
            "SELECT state FROM proposal WHERE id = ?1",
            [proposal_id],
            |r| r.get(0),
        )
        .expect("reading the proposal's state");
    assert_eq!(
        state_after, "open",
        "the proposal must read open again after the import, not stranded as accepted"
    );
}

/// It writes nothing, which is what makes it safe to call whenever the
/// editor previews — see this module's own doc comment on why that is "on
/// demand" rather than "every keystroke."
#[tokio::test]
async fn the_preview_route_leaves_the_rule_table_as_it_found_it() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let before = rule_ids(&state);
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");
    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));

    preview_json(&router, &serde_json::json!({ "rules": proposed })).await;

    assert_eq!(rule_ids(&state), before);
}

/// [`required_i64`](crate::config_api::required_i64) feeds `rule_from_json`'s
/// `ordinal` — the field the whole layering model orders resolution by. This
/// sends proposed rules whose ordinals disagree with their position in the
/// request's own `rules` array: were every ordinal to collapse to the same
/// constant, every tie would fall back to id order (list order, since a
/// preview mints new rules' ids in the order they were sent).
#[tokio::test]
async fn a_proposed_rules_own_ordinal_not_its_position_in_the_request_decides_a_tie() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    // Sent first but ordered last by ordinal — the opposite of what id order
    // (minted in list/send order) would produce.
    let high_ordinal_sent_first = serde_json::json!({
        "ordinal": 999,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": "Wins If Ordinal Is Honoured",
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "sent first, ordinal last",
    });
    let low_ordinal_sent_second = serde_json::json!({
        "ordinal": 500,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": "Wins If Only Id Order Is Honoured",
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "sent second, ordinal first",
    });
    let selected = proposed.len();
    proposed.push(high_ordinal_sent_first);
    proposed.push(low_ordinal_sent_second);

    let json = preview_json(
        &router,
        &serde_json::json!({ "rules": proposed, "selected": selected }),
    )
    .await;

    let rows = json["rows"].as_array().expect("rows must be an array");
    let row = rows
        .iter()
        .find(|r| r["after"].is_object())
        .expect("the scanned work's own member row carries an `after` placement");
    assert_eq!(
        row["after"]["series"], "Wins If Ordinal Is Honoured",
        "the rule with the higher ordinal must win regardless of which was sent first in the \
         request body: {row}"
    );
}

/// A stale editor — one whose copy of the rule list still shows a rule
/// someone else has since deleted — previews with that rule's own id still in
/// `proposed`. `rules::replace_all` refuses that outright ("no such rule
/// exists to keep"), the caller's own stale state rather than anything wrong
/// with the store, and this route must answer `400`, not fold it into
/// a `500`.
#[tokio::test]
async fn previewing_a_stale_rule_id_is_a_caller_fault_not_a_server_fault() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut stale = existing
        .first()
        .cloned()
        .expect("the fixture's own seeded rules");
    stale["id"] = serde_json::json!(9999);

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/rules/preview",
        &serde_json::json!({ "rules": [stale] }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a stale rule id is the caller's own mistake, not a store fault: {text}"
    );
    assert!(
        text.contains("9999"),
        "the refusal must name the stale id it refused: {text}"
    );
}

/// [`required_bool`](crate::config_api::required_bool) feeds `rule_from_json`'s
/// `enabled`: were every proposed rule forcibly enabled regardless of what was
/// actually sent, the disabled rule below would apply.
#[tokio::test]
async fn a_proposed_rules_own_enabled_flag_not_a_constant_true_decides_whether_it_applies() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    let selected = proposed.len();
    proposed.push(serde_json::json!({
        "ordinal": 500,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": "Enabled Rule Wins",
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "enabled, must apply",
    }));
    proposed.push(serde_json::json!({
        "ordinal": 501,
        "enabled": false,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": "Disabled Rule Must Not Win",
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "disabled, must never apply even though it sorts later",
    }));

    let json = preview_json(
        &router,
        &serde_json::json!({ "rules": proposed, "selected": selected }),
    )
    .await;

    let rows = json["rows"].as_array().expect("rows must be an array");
    let row = rows
        .iter()
        .find(|r| r["after"].is_object())
        .expect("the scanned work's own member row carries an `after` placement");
    assert_eq!(
        row["after"]["series"], "Enabled Rule Wins",
        "a rule sent with `\"enabled\": false` must never apply, even though its ordinal sorts \
         after the enabled rule: {row}"
    );
}

/// [`override_json`](crate::config_api::override_json) and
/// [`field_key`](crate::config_api::field_key) render one
/// [`taiyaku_library::preview::Override`] for the preview route's own
/// `"overrides"` array. This sends two proposed rules that actually disagree
/// on a field, which `the_preview_route_answers_over_the_whole_proposed_list`'s
/// own single appended rule cannot: it has nothing to disagree with. It also
/// pins [`field_value_json`](crate::config_api::field_value_json), whose text
/// arm is what the editor's own row reads.
#[tokio::test]
async fn a_genuine_disagreement_reaches_the_preview_routes_overrides_array() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    // `Field::Kind`, untouched by every seeded rule — the same choice
    // `preview::tests::one_rule_setting_a_field_across_the_library_is_not_an_override`
    // makes, for the same reason: nothing else here can produce a Kind
    // disagreement by accident.
    proposed.push(serde_json::json!({
        "ordinal": 500,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": "manga",
        "set_language": null,
        "note": "first decider of Kind",
    }));
    proposed.push(serde_json::json!({
        "ordinal": 501,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": "light-novel",
        "set_language": null,
        "note": "disagrees with the rule above",
    }));

    let json = preview_json(&router, &serde_json::json!({ "rules": proposed })).await;

    let overrides = json["overrides"]
        .as_array()
        .expect("overrides must be an array");
    let kind_override = overrides
        .iter()
        .find(|c| c["field"] == "kind")
        .unwrap_or_else(|| {
            panic!("two rules disagreeing on Kind must produce a real override entry: {json}")
        });
    assert_eq!(
        kind_override["wins"].as_u64(),
        Some((proposed.len() - 1) as u64),
        "{kind_override}"
    );
    assert_eq!(
        kind_override["first"].as_u64(),
        Some((proposed.len() - 2) as u64),
        "{kind_override}"
    );
    // **The two values, which are what the editor actually draws** — O95.
    // The indices above say *which* rules disagreed; only these say what
    // about, and a row that named the rules and dropped the values would
    // leave the reader to reconstruct both from two regexes and a path.
    assert_eq!(
        kind_override["first_value"].as_str(),
        Some("manga"),
        "the earlier rule's own value belongs on the row: {kind_override}"
    );
    assert_eq!(
        kind_override["wins_value"].as_str(),
        Some("light-novel"),
        "the winner's value belongs on the row: {kind_override}"
    );
}

/// [`field_value_json`](crate::config_api::field_value_json)'s **`Number` arm**
/// — a `volume` or `chapter` value crossing as a JSON number rather than a
/// string or a `null`.
///
/// **Its own test, because nothing else in this crate reaches that arm.**
/// Every other override fixture here and in `web/` carries text values, and
/// `a_genuine_disagreement_reaches_the_preview_routes_overrides_array` above
/// asserts `as_str()` on a `kind` row. Were the arm to return
/// `serde_json::Value::Null`, the editor would draw every numeric row as
/// *"rule 2 `builtin-volume` → unset / rule 3 → unset (wins)"* — a row
/// asserting a disagreement above two identical blanks.
///
/// **This is the same hole one layer up**, which
/// `preview::tests::the_numeric_fields_report_their_own_values_and_not_each_others`
/// closed for `preview::field_value`'s two numeric arms; this test closes
/// it. `docs/mutants-baseline.md`'s 2026-09-05 row carries why a sweep cannot
/// see either of them.
#[tokio::test]
async fn a_numeric_override_crosses_the_wire_as_a_number() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let mut proposed = listed["rules"].as_array().cloned().expect("rules array");

    // Seeded `builtin-volume` reads `Scanned - v01` as 1; this reads the
    // leading zero of the same token as 0. A genuine disagreement on a
    // numeric field, which is the only way to get one of these rows at all.
    proposed.push(serde_json::json!({
        "ordinal": 500,
        "enabled": true,
        "root_id": null,
        "kind": "regex",
        "pattern": "v(?<volume>0)",
        "match_on": "name",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "the leading zero, disagreeing with builtin-volume",
    }));

    let json = preview_json(&router, &serde_json::json!({ "rules": proposed })).await;
    let overrides = json["overrides"]
        .as_array()
        .expect("overrides must be an array");
    let volume = overrides
        .iter()
        .find(|o| o["field"] == "volume")
        .unwrap_or_else(|| panic!("a volume disagreement must reach the wire: {json}"));

    // `as_f64`, not `as_str`: the arm's whole job is that a number stays a
    // number rather than arriving as `"1"` for the client to parse back.
    assert_eq!(
        volume["first_value"].as_f64(),
        Some(1.0),
        "builtin-volume's own reading of v01: {volume}"
    );
    assert_eq!(
        volume["wins_value"].as_f64(),
        Some(0.0),
        "the winning capture's reading of the same token: {volume}"
    );
}

/// The ordinary round trip: created, listed, deleted — and a delete against a
/// rule no proposal ever wrote reports no reopened proposals rather than
/// omitting the field.
#[tokio::test]
async fn a_created_rule_appears_in_the_list_and_can_be_removed() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let create_body = serde_json::json!({
        "ordinal": 99,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "Some/Prefix/",
        "match_on": "path",
        "set_series": "A New Series",
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": null,
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let created: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let id = created["id"].as_i64().expect("a minted id");

    let listed = get_json(&router, "/api/library/rules").await;
    let rules = listed["rules"].as_array().expect("rules");
    let found = rules
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("the created rule is listed: {listed}"));
    assert_eq!(found["set_series"], "A New Series");
    assert_eq!(found["pattern"], "Some/Prefix/");

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let deleted: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(
        deleted["reopened_proposals"],
        serde_json::json!([]),
        "a hand-created rule was never a proposal's answer: {deleted}"
    );

    let after = get_json(&router, "/api/library/rules").await;
    assert!(
        after["rules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"] != id),
        "the deleted rule is gone: {after}"
    );
}

/// An `id` naming no rule is a 404 on both routes that take one.
#[tokio::test]
async fn patching_or_deleting_an_unknown_rule_is_a_404() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = send_json(
        &router,
        Method::PATCH,
        "/api/library/rules/999999",
        &serde_json::json!({ "enabled": false }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = send_json(
        &router,
        Method::DELETE,
        "/api/library/rules/999999",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// A `root_id` naming no root is this caller's own mistake (400, naming the
/// root), not `rules::create`'s own `FOREIGN KEY constraint failed` surfaced
/// verbatim as a 500.
#[tokio::test]
async fn creating_a_rule_against_an_unknown_root_is_a_400_naming_it() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let create_body = serde_json::json!({
        "ordinal": 99,
        "enabled": true,
        "root_id": 999_999,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": null,
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(
        text.contains("999999"),
        "the 400 must name the missing root: {text}"
    );
    assert!(
        !text.to_lowercase().contains("foreign key"),
        "a caller's own mistake must not surface SQLite's own text: {text}"
    );
}

/// [`creating_a_rule_against_an_unknown_root_is_a_400_naming_it`]'s twin for
/// `PATCH`, plus the one case the 400 must not catch: `Some(None)` — sending
/// `root_id: null` to clear it back to "every root" — names no root to check
/// and must stay allowed.
#[tokio::test]
async fn patching_a_rule_to_an_unknown_root_is_a_400_and_clearing_it_still_works() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);
    let create_body = serde_json::json!({
        "ordinal": 60,
        "enabled": true,
        "root_id": root_id,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "a rule the root_id patch test edits",
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let text = test_support::body_string(response).await;
    let created: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let id = created["id"].as_i64().expect("a minted id");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "root_id": 999_999 }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(text.contains("999999"), "{text}");
    assert!(
        !text.to_lowercase().contains("foreign key"),
        "a caller's own mistake must not surface SQLite's own text: {text}"
    );

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "root_id": null }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let conn = state.library.conn();
    let root_id_after: Option<i64> = conn
        .query_row("SELECT root_id FROM rule WHERE id = ?1", [id], |r| r.get(0))
        .expect("reads the patched rule");
    assert_eq!(
        root_id_after, None,
        "`Some(None)` must still clear root_id, not be caught by the new check"
    );
}

/// A pattern that will never compile is refused at write time, naming the
/// regex error, rather than saved clean.
#[tokio::test]
async fn creating_a_rule_with_an_uncompilable_pattern_is_refused_naming_the_error() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let create_body = serde_json::json!({
        "ordinal": 99,
        "enabled": true,
        "root_id": null,
        "kind": "regex",
        "pattern": "([unclosed",
        "match_on": "path",
        "set_series": "Whatever",
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": null,
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(
        text.contains("does not compile"),
        "the refusal must name the regex error: {text}"
    );

    let listed = get_json(&router, "/api/library/rules").await;
    assert!(
        listed["rules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["pattern"] != "([unclosed"),
        "a rule refused for a bad pattern must not land in the table: {listed}"
    );
}

/// [`patch_rule`](crate::config_api::patch_rule)'s half of the write-time
/// refusal the test above pins: the *resulting* rule must compile, not the
/// patch in isolation — this one names no `pattern` at all, only moving `kind`
/// to `regex` over a `pattern` that was never meant to be one.
#[tokio::test]
async fn patching_a_rule_to_kind_regex_over_an_uncompilable_existing_pattern_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let create_body = serde_json::json!({
        "ordinal": 61,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "([unclosed",
        "match_on": "path",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "a prefix rule whose pattern happens to look like a broken regex",
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let text = test_support::body_string(response).await;
    let created: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let id = created["id"].as_i64().expect("a minted id");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "kind": "regex" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(text.contains("does not compile"), "{text}");
}

/// The read side of those write-time refusals: a library already holding an
/// uncompilable rule — the hand-edited or older-path case `rules::read`'s own
/// doc comment cannot see — must say so in `GET`'s `complaints`, not stay
/// silent because `rules::read` only ever checks `kind`/`match_on`.
#[tokio::test]
async fn listing_rules_reports_a_pattern_that_does_not_compile() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO rule (ordinal, enabled, root_id, kind, pattern, match_on) \
             VALUES (999, 1, NULL, 'regex', '([unclosed', 'path')",
            [],
        )
        .expect("writes a hand-edited, uncompilable rule directly");
    }

    let listed = get_json(&router, "/api/library/rules").await;
    let complaints = listed["complaints"].as_array().expect("complaints array");
    assert!(
        complaints
            .iter()
            .any(|c| c.as_str().is_some_and(|c| c.contains("does not compile"))),
        "a rule whose pattern will not compile must be reported: {listed}"
    );
}

/// **`TOO_MANY` is a literal, not `MAX_PROPOSED_RULES + 1`.** Deriving the
/// list length from the constant this test is checking would make the
/// "raise `MAX_PROPOSED_RULES` to `usize::MAX`" mutation build a list of that
/// length too — an allocation that never finishes rather than a clean, fast
/// red. A literal one past the real limit (200, `config_api`'s own doc
/// comment on the constant) catches the mutation exactly as well, quickly,
/// and independently of whatever the constant currently says.
const TOO_MANY: usize = 201;

/// The proposed list itself, not just the matched-files table, has a cap:
/// `preview::overrides` is quadratic in its length.
#[tokio::test]
async fn previewing_more_than_the_proposed_rule_limit_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let too_many: Vec<serde_json::Value> = (0..TOO_MANY)
        .map(|i| {
            serde_json::json!({
                "ordinal": i,
                "enabled": true,
                "root_id": null,
                "kind": "prefix",
                "pattern": "",
                "match_on": "path",
                "set_series": null,
                "set_edition_key": null,
                "set_kind": null,
                "set_language": null,
                "note": null,
            })
        })
        .collect();
    let count = too_many.len();

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/rules/preview",
        &serde_json::json!({ "rules": too_many }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(
        text.contains(&crate::config_api::MAX_PROPOSED_RULES.to_string())
            && text.contains(&count.to_string()),
        "the refusal must name both the limit and the length received: {text}"
    );
}

/// **The boundary itself, not just one past it.** The test above pins
/// `TOO_MANY` (201) as refused; this pins `MAX_PROPOSED_RULES` (200) itself as
/// accepted, so `proposed.len() >= MAX_PROPOSED_RULES` — refusing the boundary
/// too, not only what is genuinely over it — would fail here.
#[tokio::test]
async fn previewing_exactly_the_proposed_rule_limit_is_accepted() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let exactly_the_limit: Vec<serde_json::Value> = (0..crate::config_api::MAX_PROPOSED_RULES)
        .map(|i| {
            serde_json::json!({
                "ordinal": i,
                "enabled": true,
                "root_id": null,
                "kind": "prefix",
                "pattern": "",
                "match_on": "path",
                "set_series": null,
                "set_edition_key": null,
                "set_kind": null,
                "set_language": null,
                "note": null,
            })
        })
        .collect();
    assert_eq!(
        exactly_the_limit.len(),
        crate::config_api::MAX_PROPOSED_RULES
    );

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/rules/preview",
        &serde_json::json!({ "rules": exactly_the_limit }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "exactly the limit must be accepted, not refused as though it were over it: {text}"
    );
}

/// The load-bearing distinction `RulePatch::root_id`/`note`/the four `set_*`
/// columns need: omitting a key must leave the column alone, and sending it
/// as `null` must clear it — two different instructions JSON can already
/// tell apart and a route that collapsed them could not.
#[tokio::test]
async fn patching_a_rule_leaves_an_omitted_field_alone_but_clears_a_null_one() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let create_body = serde_json::json!({
        "ordinal": 50,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": "Original Series",
        "set_edition_key": null,
        "set_kind": "official",
        "set_language": null,
        "note": "a rule the patch test edits",
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let text = test_support::body_string(response).await;
    let created: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let id = created["id"].as_i64().expect("a minted id");

    // Patch `enabled` alone. `set_series` and `set_kind` are both omitted,
    // and both must survive untouched.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "enabled": false }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (enabled, set_series, set_kind) = rule_row(&state, id);
    assert!(!enabled, "the patched column changed");
    assert_eq!(
        set_series.as_deref(),
        Some("Original Series"),
        "an omitted key must leave the column alone"
    );
    assert_eq!(
        set_kind.as_deref(),
        Some("official"),
        "an omitted key must leave the column alone"
    );

    // Patch again, sending `set_series: null` explicitly — it must clear the
    // column rather than leave it alone the way omitting it did above.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "set_series": null }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (_enabled, set_series, set_kind) = rule_row(&state, id);
    assert_eq!(set_series, None, "an explicit `null` must clear the column");
    assert_eq!(
        set_kind.as_deref(),
        Some("official"),
        "a column this second patch never named must still survive"
    );
}

/// [`patch_i64`](crate::config_api::patch_i64)'s own two-way contract, over
/// `ordinal` — the one field of [`taiyaku_library::rules::RulePatch`] that
/// reads it. A single field can't tell a `patch_i64` that always answers
/// `Ok(Some(0))` — forcing every patch to zero the column, sent or not — or
/// always `Ok(None)` — leaving `ordinal` alone even when a caller sent a real
/// number — from the right two-way answer; this sends the field once and
/// omits it once, which needs the same function to answer differently
/// both times.
#[tokio::test]
async fn patching_a_rules_ordinal_writes_the_sent_number_and_an_omitted_one_leaves_it_alone() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let create_body = serde_json::json!({
        "ordinal": 50,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "",
        "match_on": "path",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "a rule the ordinal patch test edits",
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let text = test_support::body_string(response).await;
    let created: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let id = created["id"].as_i64().expect("a minted id");

    // Sent: `ordinal` must actually move to the number the caller chose, not
    // stay put — the failure a hard-coded `Ok(None)` would produce.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "ordinal": 77 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        rule_ordinal(&state, id),
        77,
        "a sent `ordinal` must actually be written"
    );

    // Omitted: a patch that never mentions `ordinal` at all must leave it at
    // 77 — the failure a hard-coded `Ok(Some(0))` would produce, forcing it
    // to zero on every patch regardless of what was sent.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "enabled": false }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        rule_ordinal(&state, id),
        77,
        "an omitted `ordinal` must be left exactly as the previous patch set it, not zeroed"
    );
}

/// [`patch_string`](crate::config_api::patch_string) is `pattern`'s own
/// two-way reading; a patch that sends `pattern` must change it, not leave it
/// at its original value.
#[tokio::test]
async fn patching_a_rules_pattern_writes_the_sent_string() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let create_body = serde_json::json!({
        "ordinal": 50,
        "enabled": true,
        "root_id": null,
        "kind": "prefix",
        "pattern": "Original/",
        "match_on": "path",
        "set_series": null,
        "set_edition_key": null,
        "set_kind": null,
        "set_language": null,
        "note": "a rule the pattern patch test edits",
    });
    let response = send_json(&router, Method::POST, "/api/library/rules", &create_body).await;
    let text = test_support::body_string(response).await;
    let created: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    let id = created["id"].as_i64().expect("a minted id");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/rules/{id}"),
        &serde_json::json!({ "pattern": "Changed/" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let conn = state.library.conn();
    let pattern: String = conn
        .query_row("SELECT pattern FROM rule WHERE id = ?1", [id], |r| r.get(0))
        .expect("reads the rule's pattern");
    assert_eq!(
        pattern, "Changed/",
        "a sent `pattern` must actually be written, not left at its original value"
    );
}

/// A library with `count` `scan_entry` rows under one root, written directly
/// rather than through a real scan or real files — `preview::rows_for` and
/// `preview::overrides` read only `scan_entry` and `root`, and
/// `hand::load`'s own doc comment (`preview.rs`) is what makes deriving over
/// a root with nothing on disk answer `Ok(None)` rather than fail: nothing
/// this route touches needs a byte on disk to exist. Building this any other
/// way — a real tree of hundreds of files — would make the one test that
/// needs [`crate::config_api::MATCHED_FILES_LIMIT`] exceeded the slowest one
/// in the crate for a fact this shortcut proves identically.
fn state_with_many_scan_entries(
    dir: &str,
    count: usize,
) -> (std::sync::Arc<crate::ServeState>, test_support::Fixture) {
    let fixture = test_support::Fixture::new(dir);
    let library = test_support::library();
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, fixture.path()).expect("adds a root");
        // `pages` and `sha256` are set directly, so `derive::digest` never
        // tries to open a real file: `pages.filter(|p| *p > 0)` is
        // `derive::admit`'s own gate on turning an entry into a member, and
        // an entry already carrying a hash short-circuits `digest` before it
        // ever reaches `taiyaku_read::member_digest` — see that function's
        // own doc comment ("`Ok(None)` is the file having gone"). Without
        // both, every one of these synthetic rows silently produces zero
        // members and the whole point of this fixture — 501 rows for
        // `rows_for` to walk — never reaches a single one of them.
        let mut stmt = conn
            .prepare(
                "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime, pages, sha256) \
                 VALUES (?1, ?2, 'file', 'dir', 0, 0, 1, ?3)",
            )
            .expect("prepares the bulk insert");
        // **Deliberately not `Title - v0000.cbz`.** A filename carrying
        // `" v"` also matches the seeded rule 4 (`series from the filename
        // where it repeats it`), which runs *after* rule 1 and — capturing
        // the identical series name — silently becomes the field's sole
        // provenance: `Engine::resolve` keeps only the last writer per
        // field, so rule 1 would be neither a match nor a near-miss for any
        // of these rows (`preview::rows_for`'s own doc comment on what a
        // near-miss excludes), and this fixture would still prove nothing.
        // `File0000.cbz` has no `[ _-]+v` for rule 4 to catch, so rule 1's
        // directory capture is the only rule that ever sets `series` here.
        for i in 0..count {
            stmt.execute(rusqlite::params![
                root.id,
                format!("Title/File{i:04}.cbz"),
                format!("hash{i:04}"),
            ])
            .expect("writes a scan_entry row");
        }
    }
    let state = std::sync::Arc::new(crate::ServeState {
        library,
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    (state, fixture)
}

/// [`preview::Rows::truncated`] must actually track the real row count past
/// [`crate::config_api::MATCHED_FILES_LIMIT`], not read a constant `false`.
/// 501 rows under one root, every one of them matched by the seeded
/// `series`-from-directory rule (rule 1, `^(?<series>[^/]+)/`), is one more
/// than the cap.
#[tokio::test]
async fn the_preview_routes_matched_files_table_reports_its_own_truncation() {
    let (state, _fixture) = state_with_many_scan_entries("preview-truncation", 501);
    let router = test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");
    let selected = existing
        .iter()
        .position(|r| r["note"] == "series from the first directory under the root")
        .expect("the seeded series rule is in the list");

    let json = preview_json(
        &router,
        &serde_json::json!({ "rules": existing, "selected": selected }),
    )
    .await;

    assert_eq!(
        json["rows"].as_array().map(std::vec::Vec::len),
        Some(crate::config_api::MATCHED_FILES_LIMIT),
        "{json}"
    );
    assert_eq!(
        json["rows_truncated"], true,
        "501 matched rows over a 500-row cap must say so: {json}"
    );

    // `selected` is a position in *this request's* `rules` array, not a rule
    // id and not the database's own order. Reversing the list and recomputing
    // the same rule's index must move it, and the reversed request must still
    // find the identical rule.
    let mut reversed = existing.clone();
    reversed.reverse();
    let reversed_selected = reversed
        .iter()
        .position(|r| r["note"] == "series from the first directory under the root")
        .expect("the seeded series rule is still in the reversed list");
    assert_ne!(
        selected, reversed_selected,
        "reversing the list must move the rule's own index, not just its row order in the database"
    );
    let reversed_json = preview_json(
        &router,
        &serde_json::json!({ "rules": reversed, "selected": reversed_selected }),
    )
    .await;
    assert_eq!(
        reversed_json["rows"].as_array().map(std::vec::Vec::len),
        Some(crate::config_api::MATCHED_FILES_LIMIT),
        "the same rule at its new index must still be the one previewed: {reversed_json}"
    );
}

/// Every other fixture in this crate is in-memory, where
/// `Library::working` hands back the very same `MutexGuard` `Library::conn`
/// would (`Working::Shared` — see `Library::working`'s own doc comment), so
/// swapping `working()` for `conn()` inside `preview_rules` is invisible to
/// any test built over one of them: both calls take the identical lock
/// either way. Only a **file-backed** library gives `working()` a genuine
/// second connection, which is the property this test exists to prove is
/// actually used.
#[tokio::test]
async fn the_preview_route_does_not_wait_on_the_shared_connections_mutex() {
    let fixture = test_support::Fixture::new("preview-second-connection");
    let path = fixture.path().join("library.db");
    let library = taiyaku_library::Library::open(&path).expect("opens a file-backed library");
    let state = std::sync::Arc::new(crate::ServeState {
        library: std::sync::Arc::new(library),
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });

    // A background thread takes `Library::conn`'s mutex and holds it — for up
    // to five seconds, or until this test releases it, whichever comes
    // first. `preview_rules` must never wait on this: a working
    // implementation opens its own connection, answers almost immediately,
    // and this test releases the holder as soon as it sees that. A
    // regression to `conn()` blocks behind this exact guard, so the request
    // only completes once the five-second safety valve lets the holder go —
    // late enough that the assertion below catches it without hanging the
    // suite forever.
    let holder_library = std::sync::Arc::clone(&state.library);
    let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let guard = holder_library.conn();
        held_tx.send(()).expect("the test is still waiting");
        let _ = release_rx.recv_timeout(std::time::Duration::from_secs(5));
        drop(guard);
    });
    held_rx.recv().expect("the holder thread took the mutex");

    let started = std::time::Instant::now();
    let response = crate::config_api::preview_rules(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::Json(serde_json::json!({ "rules": [] })),
    )
    .await;
    let elapsed = started.elapsed();

    let status = response.status();
    release_tx.send(()).ok();
    holder.join().expect("the holder thread did not panic");

    assert_eq!(status, StatusCode::OK);
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "the preview route waited {elapsed:?} for `Library::conn`'s mutex while a background \
         thread held it — it must open `Library::working`'s own connection instead of `conn()`"
    );
}

// --- roots ---------------------------------------------------------------

/// Every route here is an administrator's, same standing the rules block
/// above holds — C4's opening line, "everything here is admin-gated per
/// A2."
#[tokio::test]
async fn a_reader_cannot_list_roots() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let response = test_support::get(&router, "/api/library/roots").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// A roots route's refusal must not reuse the rule editor's wording: that is
/// wrong on its face for a screen that is not the rule editor at all.
#[tokio::test]
async fn a_roots_route_refusal_does_not_claim_to_be_the_rule_editor() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let response = test_support::get(&router, "/api/library/roots").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let text = test_support::body_string(response).await;
    assert!(
        !text.contains("rule editor"),
        "a roots-route refusal must not reuse the rule editor's own wording: {text}"
    );
}

/// [`every_config_api_route_refuses_a_reader`]'s own reasoning, extended to
/// the roots routes and the scan-control route: a reviewer's checklist goes
/// stale the first time someone adds a route and does not think about it.
///
/// **Hand-removing `administering_or_forbidden` from either
/// [`crate::config_api::start_scan`] or [`crate::config_api::scan_status`]
/// must turn one of the two scan-route lines below red**, or this loop is not
/// actually proving what its own doc comment claims.
#[tokio::test]
async fn every_roots_route_refuses_a_reader() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let root_id = only_root_id(&state);
    let body = serde_json::json!({});

    for (method, path) in [
        (Method::GET, "/api/library/roots".to_string()),
        (Method::PATCH, format!("/api/library/roots/{root_id}")),
        (Method::DELETE, format!("/api/library/roots/{root_id}")),
        (Method::GET, format!("/api/library/roots/{root_id}/scan")),
        (Method::POST, format!("/api/library/roots/{root_id}/scan")),
    ] {
        let response = send_json(&router, method.clone(), &path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}

/// The ordinary round trip C4 §3's screen reads: the list carries the
/// regime line's own inputs — `scan_interval_secs`, `last_scan_at` and
/// `keeps_documents`.
///
/// **`last_scan_at`/`last_scan_status` are populated here, not `null`.**
/// `state_with_two_scanned_roots` scans both of its roots through
/// `scan::scan_root`, which writes both columns on every call (`scan.rs`'s own
/// doc comment on why the write lives there rather than in either caller), so
/// a root this fixture built has a real timestamp and `"ok"` to show for it.
#[tokio::test]
async fn the_root_list_carries_the_interval_and_the_last_scan() {
    let (router, _state, _fixture) = test_support::state_with_two_scanned_roots();
    let json = get_json(&router, "/api/library/roots").await;
    let roots = json["roots"].as_array().expect("a roots array");
    assert_eq!(roots.len(), 2, "{json}");
    let first = &roots[0];
    assert!(
        first["scan_interval_secs"].is_number() || first["scan_interval_secs"].is_null(),
        "{json}"
    );
    assert!(
        first["last_scan_at"].is_string(),
        "a root this fixture scanned must carry a real timestamp: {json}"
    );
    assert_eq!(
        first["last_scan_status"], "ok",
        "an ordinary scan of a healthy root records \"ok\": {json}"
    );
    assert!(first.get("keeps_documents").is_some(), "{json}");
    assert!(first["keeps_documents"].is_boolean(), "{json}");
}

/// `next_scan_at` is the last scan plus the interval the directory is scanned
/// at: Automatic's for a stored null, nothing for Off, and nothing for a
/// directory never scanned, which the page draws as due. Read by `get`, so a
/// key not sent is not taken for a sent `null`.
#[tokio::test]
async fn the_root_list_says_when_each_directory_is_scanned_next() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (first, second): (i64, i64) = state
        .library
        .conn()
        .query_row("SELECT MIN(id), MAX(id) FROM root", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("reads the two roots");
    state
        .library
        .conn()
        .execute_batch(&format!(
            "UPDATE root SET scan_interval_secs = NULL, last_scan_at = '2026-11-10 12:00:00' \
             WHERE id = {first};
             UPDATE root SET last_scan_at = NULL WHERE id = {second};"
        ))
        .expect("puts the scans in place");

    let next_of = |json: &serde_json::Value, id: i64| -> Option<serde_json::Value> {
        json["roots"]
            .as_array()
            .expect("a roots array")
            .iter()
            .find(|root| root["id"] == id)
            .expect("the root is listed")
            .get("next_scan_at")
            .cloned()
    };
    let json = get_json(&router, "/api/library/roots").await;
    assert_eq!(
        next_of(&json, first),
        Some(serde_json::json!("2026-11-10 12:15:00")),
        "Automatic, 15 minutes after the last scan: {json}"
    );
    assert_eq!(
        next_of(&json, second),
        Some(serde_json::Value::Null),
        "never scanned: {json}"
    );

    state
        .library
        .conn()
        .execute(
            "UPDATE root SET scan_interval_secs = 0 WHERE id = ?1",
            [first],
        )
        .expect("turns the first off");
    let json = get_json(&router, "/api/library/roots").await;
    assert_eq!(
        next_of(&json, first),
        Some(serde_json::Value::Null),
        "Off: {json}"
    );
}

/// The list carries `exclude_globs`, so the box the roots screen draws for
/// it can start at what the row actually holds.
///
/// **O92's readable half.** Were the key not sent back, that box would be
/// write-only: it would start empty whatever the row held.
///
/// **The unpatched root is checked too, and for `null` rather than for
/// absence.** A `root_json` that omits the key entirely reads as
/// `Value::Null` through indexing, so asserting only `first["exclude_globs"]
/// .is_null()` passes against the very defect this closes — it is
/// `get(...).is_some()` that tells a sent `null` from a key nobody wrote.
#[tokio::test]
async fn the_root_list_carries_the_exclude_globs_the_row_holds() {
    let (router, _state, _fixture) = test_support::state_with_two_scanned_roots();

    let before = get_json(&router, "/api/library/roots").await;
    let first = &before["roots"].as_array().expect("a roots array")[0];
    let id = first["id"].as_i64().expect("a root id");
    assert!(
        first.get("exclude_globs").is_some(),
        "the key is sent for a root that has no pattern, as an explicit null: {before}"
    );
    assert!(first["exclude_globs"].is_null(), "{before}");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{id}"),
        &serde_json::json!({ "exclude_globs": "**/extras/**" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{text}");

    let after = get_json(&router, "/api/library/roots").await;
    let patched = after["roots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .expect("the patched root is still listed");
    assert_eq!(
        patched["exclude_globs"], "**/extras/**",
        "the pattern the patch stored, read back off the list: {after}"
    );
}

/// [`patching_a_rule_leaves_an_omitted_field_alone_but_clears_a_null_one`]'s
/// own load-bearing distinction, for `RootPatch`: omitting a key leaves the
/// column alone, sending it as `null` clears it.
#[tokio::test]
async fn patching_a_root_leaves_an_omitted_field_alone_but_clears_a_null_one() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({ "default_kind": "manga", "default_language": "ja" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Patch `default_kind` alone; `default_language` is omitted and must
    // survive untouched.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({ "default_kind": "novel" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (kind, language) = default_kind_and_language(&state, root_id);
    assert_eq!(kind.as_deref(), Some("novel"), "the patched column changed");
    assert_eq!(
        language.as_deref(),
        Some("ja"),
        "an omitted key must leave the column alone"
    );

    // Patch again, sending `default_language: null` explicitly — it must
    // clear the column rather than leave it alone the way omitting it did
    // above.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({ "default_language": null }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (kind, language) = default_kind_and_language(&state, root_id);
    assert_eq!(language, None, "an explicit `null` must clear the column");
    assert_eq!(
        kind.as_deref(),
        Some("novel"),
        "a column this second patch never named must still survive"
    );
}

/// `default_kind`, `default_language` for one root — read back to prove
/// what a patch actually wrote, rather than trusting the route's own 204.
fn default_kind_and_language(
    state: &crate::ServeState,
    id: i64,
) -> (Option<String>, Option<String>) {
    let conn = state.library.conn();
    conn.query_row(
        "SELECT default_kind, default_language FROM root WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .expect("reads the root row")
}

/// An `id` naming no root is a 404 on every route that takes one.
#[tokio::test]
async fn patching_or_deleting_an_unknown_root_is_a_404() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = send_json(
        &router,
        Method::PATCH,
        "/api/library/roots/999999",
        &serde_json::json!({ "keeps_documents": true }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let text = test_support::body_string(response).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("a JSON body"),
        serde_json::json!({ "error": "there is no directory 999999" }),
        "the Libraries screen draws this, and it shows directories"
    );

    let response = send_json(
        &router,
        Method::DELETE,
        "/api/library/roots/999999",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let text = test_support::body_string(response).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("a JSON body"),
        serde_json::json!({ "error": "there is no directory 999999" }),
    );
}

/// C4 §2's whole point: the counts reach the caller **before** anything is
/// removed. `?dry_run=true` runs the identical cost computation
/// [`scan::removal_cost`] does, rolled back, so a second, unprefixed
/// `DELETE` still finds the root standing and can remove it for real —
/// proof that the dry run truly committed nothing.
#[tokio::test]
async fn a_dry_run_delete_costs_without_removing_and_a_real_one_then_still_can() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let id = test_support::second_root_id(&state);

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/roots/{id}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let dry: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(dry["dry_run"], true, "{dry}");
    let dry_works = dry["removed"]["works"].as_u64().expect("a work count");
    assert!(dry_works > 0, "{dry}");
    assert_eq!(
        (
            dry["removed"]["positions_page_gone"].as_u64(),
            dry["removed"]["notes_page_gone"].as_u64()
        ),
        (Some(0), Some(0)),
        "the rehearsal counts the pages it takes from works it keeps: {dry}"
    );

    // Nothing was actually removed: the root row is still there.
    {
        let conn = state.library.conn();
        let still_there: i64 = conn
            .query_row("SELECT count(*) FROM root WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .expect("reads the root table");
        assert_eq!(still_there, 1, "a dry run must not remove the root row");
    }
    let still_listed = get_json(&router, "/api/library/roots").await;
    assert!(
        still_listed["roots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == id),
        "a dry run must not remove the root: {still_listed}"
    );

    // The real thing: identical counts, and this time it actually happens.
    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/roots/{id}"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let real: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(real["dry_run"], false, "{real}");
    assert_eq!(
        real["removed"]["works"].as_u64(),
        Some(dry_works),
        "the dry run and the real removal must report the identical count: dry {dry}, real {real}"
    );

    let after = get_json(&router, "/api/library/roots").await;
    assert!(
        after["roots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"] != id),
        "the removed root is gone: {after}"
    );
}

/// Removing the one directory of a library that is a reader's only grant goes
/// ahead: the dry run names the reader under `left_with_no_library` and
/// removes nothing, the removal answers the same, and the reader is then
/// listed nothing. The first root's series stands, so a reader widened to
/// every library would be listed it.
#[tokio::test]
async fn removing_a_readers_only_library_names_the_reader_who_then_sees_nothing() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let id = test_support::second_root_id(&state);
    let reader = {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO user (name, password_hash, every_library) VALUES ('reader', 'x', 0)",
            [],
        )
        .expect("adds a reader seeing only what it is granted");
        let reader = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO user_library (user_id, library_id)
             SELECT ?1, library_id FROM library_root WHERE root_id = ?2",
            [reader, id],
        )
        .expect("grants the reader the second root's library and no other");
        reader
    };
    assert_eq!(
        test_support::seen_by(&router, &state, reader).await,
        1,
        "the fixture: the reader sees the second root's series"
    );
    let roots_and_works = |state: &crate::ServeState| -> i64 {
        state
            .library
            .conn()
            .query_row(
                "SELECT (SELECT count(*) FROM root) * 100 + (SELECT count(*) FROM work)",
                [],
                |r| r.get(0),
            )
            .expect("counts roots and works")
    };
    let before = roots_and_works(&state);

    let mut answers = Vec::new();
    for query in ["?dry_run=true", ""] {
        let response = send_json(
            &router,
            Method::DELETE,
            &format!("/api/library/roots/{id}{query}"),
            &serde_json::json!({}),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::OK, "`{query}`: {text}");
        let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
        assert_eq!(
            body["removed"]["left_with_no_library"],
            serde_json::json!(["reader"]),
            "`{query}`: {text}"
        );
        if query.is_empty() {
            assert_ne!(roots_and_works(&state), before, "the removal removed");
        } else {
            assert_eq!(roots_and_works(&state), before, "the dry run removed");
        }
        answers.push(body["removed"].clone());
    }
    assert_eq!(
        answers[0], answers[1],
        "the removal does what the dry run said"
    );
    assert_eq!(
        test_support::seen_by(&router, &state, reader).await,
        0,
        "the reader sees no library, not every library"
    );
}

/// A misspelled query key — `?dryrun=true` (missing the underscore) or
/// `?dryRun=true` (the wrong case) — must be refused with `400` and the root
/// left standing. `DryRunQuery` denies unknown fields; without that,
/// `serde_urlencoded` would silently drop a key it did not recognise,
/// `dry_run` would default to `false`, and the real, committed removal would
/// run on the one route C4 §2 calls "the most destructive action in the
/// product": the root, its works, its reading positions and its notes would
/// be gone.
#[tokio::test]
async fn a_misspelled_dry_run_query_key_is_refused_not_silently_ignored() {
    for misspelled in ["dryrun", "dryRun"] {
        let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
        let id = test_support::second_root_id(&state);

        let response = send_json(
            &router,
            Method::DELETE,
            &format!("/api/library/roots/{id}?{misspelled}=true"),
            &serde_json::json!({}),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "?{misspelled}=true must be refused as an unknown field, not silently run as a \
             real removal: {text}"
        );

        let conn = state.library.conn();
        let still_there: i64 = conn
            .query_row("SELECT count(*) FROM root WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .expect("reads the root table");
        assert_eq!(
            still_there, 1,
            "?{misspelled}=true must not have removed the root — the whole point of this test"
        );
    }
}

// --- the roots routes' wording and nullable columns ------------------------

/// A JSON object with exactly one key — the shape every wording/nullable
/// test below sends, built rather than spelled with `serde_json::json!`
/// directly since `field` is a variable and the macro's key position wants
/// a literal.
fn one_field_patch(field: &str, value: serde_json::Value) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert(field.to_string(), value);
    serde_json::Value::Object(body)
}

/// One nullable text column off the `root` row — what every nullable-column
/// test below reads back to prove what a patch actually wrote, rather than
/// trusting the route's own 204.
fn root_text_column(state: &crate::ServeState, id: i64, column: &str) -> Option<String> {
    let conn = state.library.conn();
    conn.query_row(
        &format!("SELECT {column} FROM root WHERE id = ?1"),
        [id],
        |r| r.get(0),
    )
    .expect("reads the root row")
}

/// [`root_text_column`]'s twin for `scan_interval_secs`, the one nullable
/// column among the five that is a number rather than text.
fn root_int_column(state: &crate::ServeState, id: i64, column: &str) -> Option<i64> {
    let conn = state.library.conn();
    conn.query_row(
        &format!("SELECT {column} FROM root WHERE id = ?1"),
        [id],
        |r| r.get(0),
    )
    .expect("reads the root row")
}

/// `root_patch_from_json` reuses `patch_bool`, `patch_nullable_text` and
/// `patch_nullable_i64`. One `PATCH` per field, each with a value of the wrong
/// type, exercises all three, so a roots-route message that named a rule, from
/// any one of the three helpers, would be caught here.
#[tokio::test]
async fn patching_a_root_with_bad_fields_names_a_root_not_a_rule() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    for (field, bad_value) in [
        ("keeps_documents", serde_json::json!("yes")), // patch_bool
        ("edition_key", serde_json::json!(123)),       // patch_nullable_text
        ("scan_interval_secs", serde_json::json!("soon")), // patch_nullable_i64
        ("default_kind", serde_json::json!(1)),        // patch_nullable_text
        ("default_language", serde_json::json!(1)),    // patch_nullable_text
        ("exclude_globs", serde_json::json!(1)),       // patch_nullable_text
    ] {
        let response = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/roots/{root_id}"),
            &one_field_patch(field, bad_value),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}: {text}");
        let json: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
        let error = json["error"].as_str().expect("an `error` string");
        assert!(
            error.contains(&format!("the directory's `{field}`")),
            "the message must name a directory: {error}"
        );
        assert!(
            !error.contains("the rule's"),
            "a roots-route validation message must never name a rule: {error}"
        );
    }
}

/// A directory's settings that are not an object are refused in the directory's
/// words, which are the ones the Libraries screen draws.
#[tokio::test]
async fn a_dir_patch_that_is_not_an_object_is_refused_in_the_dirs_words() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!([1, 2]),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("a JSON body"),
        serde_json::json!({ "error": "a directory's settings must be a JSON object" }),
    );
}

/// Parameterising `required_str`/`patch_bool`/`patch_nullable_text`/
/// `patch_nullable_i64` with a noun must not cost the rules routes their own
/// wording — a future re-share that let `"root"` leak backward into a
/// rules-route call site would be silent.
#[tokio::test]
async fn a_rules_route_validation_message_still_names_a_rule() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    // `required_str`, via `new_rule_from_json`: `kind` is missing.
    let response = send_json(
        &router,
        Method::POST,
        "/api/library/rules",
        &serde_json::json!({ "enabled": true }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(text.contains("the rule's `kind`"), "{text}");
    assert!(!text.contains("the directory's"), "{text}");

    // `patch_bool`, `patch_nullable_text` and `patch_nullable_i64`, via
    // `rule_patch_from_json` — the id need not exist, since a patch this
    // malformed is refused before any store call runs.
    for (field, bad_value) in [
        ("enabled", serde_json::json!("no")),      // patch_bool
        ("set_series", serde_json::json!(123)),    // patch_nullable_text
        ("root_id", serde_json::json!("nowhere")), // patch_nullable_i64
    ] {
        let response = send_json(
            &router,
            Method::PATCH,
            "/api/library/rules/999999",
            &one_field_patch(field, bad_value),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}: {text}");
        assert!(
            text.contains(&format!("the rule's `{field}`")),
            "{field}: {text}"
        );
        assert!(!text.contains("the directory's"), "{field}: {text}");
    }
}

/// `RootPatch`'s five nullable columns (`edition_key`, `default_kind`,
/// `default_language`, `exclude_globs`, `scan_interval_secs`) all route
/// through the same three-way `patch_nullable_text`/`patch_nullable_i64`
/// helpers, so there is no per-field wiring to drift.
/// `root_patch_from_json` evaluates every nullable column against the whole
/// request body regardless of which keys are actually present, so a `PATCH`
/// that never mentions `field` at all is exactly what a helper collapsed to
/// the two-way `optional_text`/`optional_i64` reading gets wrong: it would
/// clear the column instead of leaving it alone.
async fn a_nullable_root_text_column_is_left_alone_when_omitted_but_cleared_by_null(field: &str) {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch(field, serde_json::json!("first")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_text_column(&state, root_id, field).as_deref(),
        Some("first"),
        "{field}: the patch must have written it"
    );

    // A patch that never names `field` at all — not even as `null` — must
    // leave it alone.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({ "keeps_documents": true }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_text_column(&state, root_id, field).as_deref(),
        Some("first"),
        "{field}: omitting the key must leave the column alone"
    );

    // An explicit `null` clears it.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch(field, serde_json::Value::Null),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_text_column(&state, root_id, field),
        None,
        "{field}: an explicit `null` must clear the column"
    );
}

#[tokio::test]
async fn a_roots_patch_leaves_edition_key_alone_when_omitted_but_clears_it_on_null() {
    a_nullable_root_text_column_is_left_alone_when_omitted_but_cleared_by_null("edition_key").await;
}

#[tokio::test]
async fn a_roots_patch_leaves_exclude_globs_alone_when_omitted_but_clears_it_on_null() {
    a_nullable_root_text_column_is_left_alone_when_omitted_but_cleared_by_null("exclude_globs")
        .await;
}

/// [`a_nullable_root_text_column_is_left_alone_when_omitted_but_cleared_by_null`]'s
/// own reasoning, for `scan_interval_secs` — the one nullable `RootPatch`
/// column that is a number rather than text, so it needs its own body and
/// its own read-back rather than reusing that helper.
#[tokio::test]
async fn a_roots_patch_leaves_scan_interval_secs_alone_when_omitted_but_clears_it_on_null() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("scan_interval_secs", serde_json::json!(600)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_int_column(&state, root_id, "scan_interval_secs"),
        Some(600)
    );

    // A patch that never names `scan_interval_secs` must leave it alone.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({ "keeps_documents": true }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_int_column(&state, root_id, "scan_interval_secs"),
        Some(600),
        "omitting the key must leave the column alone"
    );

    // An explicit `null` clears it.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("scan_interval_secs", serde_json::Value::Null),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_int_column(&state, root_id, "scan_interval_secs"),
        None,
        "an explicit `null` must clear the column"
    );
}

/// Every is at least 5 minutes: a shorter interval would keep the job lock
/// held through a derive for most of the time (spec §4, Decision 3). A value
/// below 0 is no setting at all. Neither is stored.
#[tokio::test]
async fn a_roots_patch_refuses_an_interval_under_five_minutes_and_one_below_zero() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);
    state
        .library
        .conn()
        .execute(
            "UPDATE root SET scan_interval_secs = 600 WHERE id = ?1",
            [root_id],
        )
        .expect("sets the interval");

    for refused in [299, -1] {
        let response = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/roots/{root_id}"),
            &one_field_patch("scan_interval_secs", serde_json::json!(refused)),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}: {text}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).expect("a JSON body"),
            serde_json::json!({
                "error": "the directory's `scan_interval_secs` must be null (automatic), 0 (off) \
                          or at least 300 (5 minutes)"
            }),
            "{refused}"
        );
        assert_eq!(
            root_int_column(&state, root_id, "scan_interval_secs"),
            Some(600),
            "{refused} must leave the column as it was"
        );
    }
}

/// The three settings the minimum leaves: Every at exactly the minimum, Off
/// (0) and Automatic (null).
#[tokio::test]
async fn a_roots_patch_stores_off_automatic_and_five_minutes() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    for (sent, stored) in [
        (serde_json::json!(300), Some(300)),
        (serde_json::json!(0), Some(0)),
        (serde_json::Value::Null, None),
    ] {
        let response = send_json(
            &router,
            Method::PATCH,
            &format!("/api/library/roots/{root_id}"),
            &one_field_patch("scan_interval_secs", sent.clone()),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{sent}: {text}");
        assert_eq!(
            root_int_column(&state, root_id, "scan_interval_secs"),
            stored,
            "{sent}"
        );
    }
}

// --- scan control ---------------------------------------------------------

/// A page directory `scan_root` will find — [`test_support`]'s own
/// `write_pages` restated here rather than reused: that one is private to
/// `test_support.rs` (`pub(crate)` inside that module only), the same gap
/// `test_support::write_pages`'s own doc comment notes for
/// `scan::tests::write_pages`.
fn write_page_dir(dir: &std::path::Path, pages: usize) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let label = dir.to_string_lossy().into_owned();
    for page in 0..pages {
        std::fs::write(
            dir.join(format!("{page:03}.jpg")),
            format!("not really a jpeg: {label} page {page}").as_bytes(),
        )
        .expect("writes a page");
    }
}

/// **A derive that fails after a successful walk corrects
/// `last_scan_status`.** `scan_jobs::run` runs `scan::scan_root` and
/// `derive::derive` as two separate calls, and `scan_root` itself already
/// writes `last_scan_status = "ok"` the moment its own walk commits — before
/// the derive half of the job has even started. The walk genuinely succeeded,
/// but the library it now describes may be only partly derived, and C4 §3's
/// regime line — the one column this whole route exists to make trustworthy —
/// must not keep reading `"ok"` anyway.
///
/// **The derive failure here is engineered, not raced.** Dropping `rule`
/// after a first successful scan+derive makes the *next* `derive::derive`
/// fail deterministically (`rules::read`/`Engine::load` have nothing to
/// read), while `scan::scan_root` never touches that table at all and so
/// still succeeds and still writes `"ok"` — reproducing "the walk succeeded,
/// the derive after it did not" without depending on how two real threads
/// happen to interleave, the same choice
/// `derive::tests::write_roots_survives_a_root_deleted_after_its_own_group_was_built`
/// makes for the race itself.
#[tokio::test]
async fn a_derive_failure_after_a_successful_scan_corrects_last_scan_status() {
    let fixture = test_support::Fixture::new("scan-then-derive-fails");
    let root_dir = fixture.path().join("root");
    write_page_dir(&root_dir.join("Series").join("Series - v01"), 2);

    let library = test_support::library();
    let root = {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &root_dir).expect("adding the root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("the first scan");
        taiyaku_library::derive::derive(&conn).expect("the first derive");
        assert_eq!(
            conn.query_row(
                "SELECT last_scan_status FROM root WHERE id = ?1",
                [root.id],
                |r| r.get::<_, String>(0),
            )
            .expect("reading the status the first scan wrote"),
            "ok",
            "sanity: the first scan must have left last_scan_status = \"ok\", or this test \
             proves nothing about correcting it"
        );
        conn.execute("DROP TABLE rule", [])
            .expect("dropping rule so the next derive fails deterministically");
        root
    };

    let job = crate::LibraryJob::default();
    let outcome = crate::test_support::run_unreserved(
        &library,
        std::slice::from_ref(&root),
        crate::scan_jobs::Reread::Dir,
        &job,
    );
    let why = outcome.expect_err("the derive must actually fail, or this test proves nothing");
    assert!(
        why.starts_with(&format!(
            "{} was scanned, but deriving",
            root.path.display()
        )),
        "{why}"
    );

    let conn = library.conn();
    let status: String = conn
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("reading the corrected status");
    assert_eq!(
        status, "failed",
        "the walk succeeded but the derive right after it did not — last_scan_status must say \
         so, not keep reading the walk's own \"ok\""
    );
}

/// A library holding one scanned, derived work under a root that keeps hand
/// documents, with a hand-set title already durable on disk — and then
/// `<root>/.taiyaku` itself replaced with a plain file, so the *next*
/// derive's own attempt to keep that document written finds nowhere to
/// write it.
///
/// **A plain file where the directory must go, not a read-only root or a
/// `chmod`.** `hand/tests.rs`'s own
/// `a_root_that_cannot_take_the_document_still_arms_the_guard` states the
/// reason this exact shape exists: **CI runs as root**, which holds
/// `CAP_DAC_OVERRIDE` and ignores every permission bit a test could set, so
/// a `chmod 0000` fixture passes locally and proves nothing on the forge.
/// `std::fs::read_dir` refuses a plain file with `NotADirectory` for any
/// uid, which is what this fixture actually needs: a scan whose derive pass
/// cannot list `.taiyaku` to find what is already there, while
/// `migrated_document.absorbed = 1` still says a document is owed.
///
/// **The first `hand::save` has to succeed before the obstruction goes
/// down.** `absorbed` is the register `hand::reconcile` reads to decide a
/// document is owed at all (`hand.rs`'s own doc comment on `reconcile`), and
/// only [`taiyaku_library::hand::save`] arms it — so this fixture writes the
/// document for real once, over a genuine directory, and only then swaps
/// `.taiyaku` for a file. The scan endpoint's *own* derive pass — not this
/// fixture — is what then finds the gap and cannot close it, which is the
/// whole point: the report this test reads has to come from the route, not
/// from a fixture that engineered the failure and reported it itself.
fn state_with_a_root_that_cannot_write_its_document(
    dir: &str,
) -> (std::sync::Arc<crate::ServeState>, test_support::Fixture) {
    let fixture = test_support::Fixture::new(dir);
    let root_dir = fixture.path().join("root");
    write_page_dir(&root_dir.join("Series").join("Series - v01"), 2);

    let library = test_support::library();
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds a root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("the first scan");
        taiyaku_library::derive::derive(&conn).expect("the first derive");
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("opts the root into hand documents");

        let work_id: i64 = conn
            .query_row("SELECT id FROM work", [], |r| r.get(0))
            .expect("the fixture's one work");
        let mut doc = taiyaku_library::librarydoc::of(&conn, work_id).expect("projecting");
        doc.title = Some("Hand Set Title".to_string());
        taiyaku_library::librarydoc::apply(&conn, work_id, &doc).expect("applying the hand edit");
        taiyaku_library::hand::save(&conn, work_id)
            .expect("the first save, over a real directory, must succeed");
    }

    let taiyaku_dir = root_dir.join(".taiyaku");
    assert!(
        taiyaku_dir.is_dir(),
        "the first save must have created a real .taiyaku directory"
    );
    std::fs::remove_dir_all(&taiyaku_dir).expect("removing the real .taiyaku directory");
    std::fs::write(&taiyaku_dir, b"not a directory")
        .expect("planting a file where the directory must go");

    let state = std::sync::Arc::new(crate::ServeState {
        library,
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    (state, fixture)
}

/// A library holding one root with two directories under it — a base series
/// and its "Full Color" edition — with a hand-set title already saved on the
/// colour work, and a fold rule (`prefix "Series - Full Color/"`, the exact
/// shape `proposals::accept` writes) sitting in `rule` but not yet applied.
///
/// **The library level of this same fixture is
/// `taiyaku_library::derive::tests::folding_a_work_away_orphans_its_hand_document_on_the_same_pass`.**
/// That test proves `DeriveReport::orphan_documents[_].absorbed_into` is
/// populated when `derive::derive` itself folds a hand-documented work away.
/// This restates its setup rather than reusing it — that helper is private
/// to `taiyaku-library`'s own test module — so the scan endpoint's *own*
/// second derive, run through `start_scan`/`scan_status` rather than called
/// directly, is what performs the fold and renders the answer, which is what
/// [`a_scan_report_names_the_work_a_folded_documents_orphan_was_absorbed_into`]
/// needs: `orphan_json`/`absorbed_json` are private to `config_api`, reached
/// only by a route actually rendering one.
fn state_with_a_root_that_folds_a_hand_documented_work(
    dir: &str,
) -> (std::sync::Arc<crate::ServeState>, test_support::Fixture) {
    let fixture = test_support::Fixture::new(dir);
    let root_dir = fixture.path().join("root");
    write_page_dir(&root_dir.join("Series").join("Series - v01"), 2);
    write_page_dir(
        &root_dir
            .join("Series - Full Color")
            .join("Series - Full Color - v01"),
        2,
    );

    let library = test_support::library();
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds a root");
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("opts the root into hand documents");
        taiyaku_library::scan::scan_root(&conn, &root).expect("the first scan");
        taiyaku_library::derive::derive(&conn)
            .expect("the first derive — two directories, two works, nothing folded yet");

        let colour_work_id: i64 = conn
            .query_row(
                "SELECT edition.work_id FROM member
                   JOIN edition ON edition.id = member.edition_id
                  WHERE member.path LIKE '%Full Color%'",
                [],
                |r| r.get(0),
            )
            .expect("the colour work");
        let mut doc = taiyaku_library::librarydoc::of(&conn, colour_work_id).expect("projecting");
        doc.title = Some("Hand Set Colour Title".to_string());
        taiyaku_library::librarydoc::apply(&conn, colour_work_id, &doc)
            .expect("applying the hand edit");
        taiyaku_library::hand::save(&conn, colour_work_id)
            .expect("saving the colour work's own document, over a real directory");

        // `proposals::accept`'s own shape for folding an edition into its
        // base series — `taiyaku_library::derive::tests::write_the_fold_rule`,
        // restated here for the same reason `write_page_dir` restates
        // `test_support::write_pages`: that helper is private to the other
        // crate's own test module.
        conn.execute(
            "INSERT INTO rule (ordinal, kind, pattern, match_on, set_series, set_edition_key)
             VALUES (99, 'prefix', 'Series - Full Color/', 'path', 'Series', 'full-color')",
            [],
        )
        .expect("the fold rule, not yet applied — the scan endpoint's own derive is what runs it");
    }

    let state = std::sync::Arc::new(crate::ServeState {
        library,
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    (state, fixture)
}

/// **`uncarried_alignments` reaches the scan report with its own number —
/// O76.** Three alignments on the colour work name editions it does not hold,
/// so the fold the scan runs can carry none of them, and three is not the
/// number `discarded_rows` holds, which the test checks rather than assumes.
#[tokio::test]
async fn a_scan_report_counts_the_alignments_a_fold_left_behind() {
    let (state, _fixture) = state_with_a_root_that_folds_a_hand_documented_work(
        "taiyaku-test-scan-fold-uncarried-alignments",
    );
    let root_id = only_root_id(&state);
    {
        let conn = state.library.conn();
        let colour: i64 = conn
            .query_row(
                "SELECT edition.work_id FROM member
                   JOIN edition ON edition.id = member.edition_id
                  WHERE member.path LIKE '%Full Color%'",
                [],
                |r| r.get(0),
            )
            .expect("the colour work");
        for gone in 1..=3 {
            conn.execute(
                "INSERT INTO alignment (work_id, from_key, to_key, by)
                 VALUES (?1, 'raw', 'gone-' || ?2, 'solver')",
                rusqlite::params![colour, gone],
            )
            .expect("an alignment naming an edition the colour work does not hold");
        }
    }

    let started = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;
    assert_eq!(started.status(), StatusCode::ACCEPTED);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let json = loop {
        let response = crate::config_api::scan_status(
            axum::extract::State(std::sync::Arc::clone(&state)),
            axum::extract::Extension(an_administrator()),
            test_support::from_a_browser(),
            axum::extract::Path(root_id),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = test_support::body_string(response).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
        if json["running"] == false {
            break json;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the scan did not finish within 5s: {json}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };

    let derive = &json["report"]["derive"];
    assert_eq!(derive["emptied_works"], 1, "the fold ran: {derive}");
    assert_ne!(
        derive["discarded_rows"], 3,
        "the fixture must not give discarded_rows the number under test: {derive}"
    );
    assert_eq!(
        derive["uncarried_alignments"], 3,
        "the three alignments the fold could not carry: {derive}"
    );
}

/// O90: `unreadable` is an object naming each cause, rendered whole.
///
/// **Every key, with its value, asserted by equality.** `derive_report_json`
/// destructures with no `..`, which catches a field missing from the pattern
/// but not a key dropped from the `json!` beside it or given another field's
/// value, and cargo-mutants makes no mutant for a `json!` key. Every cause
/// here holds a different number, so a value under the wrong key fails too.
#[test]
fn the_derive_report_renders_every_unreadable_cause_under_its_own_key() {
    let mut report = taiyaku_library::derive::DeriveReport::default();
    report.unreadable.by_format.insert("rar".to_string(), 12);
    report.unreadable.by_format.insert("7z".to_string(), 3);
    report.unreadable.uncounted = vec!["/srv/manga/a.cbz".to_string()];
    report.unreadable.vanished = vec!["/srv/manga/b".to_string(), "/srv/manga/c".to_string()];
    report.unreadable.no_pages = 7;

    let json = crate::config_api::derive_report_json(&report);

    assert_eq!(
        json["unreadable"],
        serde_json::json!({
            "total": 25,
            "refused_format": 15,
            "by_format": { "7z": 3, "rar": 12 },
            "uncounted": ["/srv/manga/a.cbz"],
            "vanished": ["/srv/manga/b", "/srv/manga/c"],
            "no_pages": 7,
        }),
    );
}

/// The two counts of the missing mark, each under its own key: a `json!` key
/// misspelt or given the other's value compiles, and the web's fixtures are
/// written by hand, so only this reads the keys the scan report draws.
#[test]
fn the_derive_report_renders_the_works_marked_missing_and_found_again() {
    let report = taiyaku_library::derive::DeriveReport {
        marked_missing: 3,
        found_again: 5,
        ..Default::default()
    };

    let json = crate::config_api::derive_report_json(&report);

    assert_eq!(
        (&json["marked_missing"], &json["found_again"]),
        (&serde_json::json!(3), &serde_json::json!(5)),
        "{json}"
    );
}

/// The two counts of pages a removal takes, each under its own key and each a
/// different number, so a key given the other's value fails (O244). Every
/// route fixture's removal takes no page from a kept work, so only this does.
#[test]
fn a_removal_renders_each_count_of_pages_taken_under_its_own_key() {
    let removal = taiyaku_library::scan::Removal {
        positions_page_gone: 1,
        notes_page_gone: 2,
        ..Default::default()
    };

    let json = crate::config_api::removal_json(&removal);

    assert_eq!(
        (&json["positions_page_gone"], &json["notes_page_gone"]),
        (&serde_json::json!(1), &serde_json::json!(2)),
        "{json}"
    );
}

/// C4 §3 requires a scan already running is not startable twice, and the
/// refusal names which root is already running rather than queueing a
/// second walk of the same tree.
///
/// The registry entry is planted directly — `ServeState.scans`'s own field,
/// not a route — so this test proves `start_scan`'s own refusal rather than
/// racing a real scan to still be running when the second `POST` lands.
#[tokio::test]
async fn a_second_scan_of_a_root_already_scanning_is_refused() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now(),
            done: false,
            report: None,
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    let response = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "a scan already running must refuse a second start, not queue it"
    );
    let path = {
        let conn = state.library.conn();
        taiyaku_library::scan::roots(&conn)
            .expect("reads the roots")
            .into_iter()
            .find(|root| root.id == root_id)
            .expect("the root")
            .path
    };
    let text = test_support::body_string(response).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("a JSON body"),
        serde_json::json!({ "error": format!("a scan of {} is already running", path.display()) }),
        "the refusal names the directory by its path, not by an id the screen never shows"
    );

    // A *finished* scan does not block a fresh one — only `done: false`
    // does. Proves the mutation this test's own name promises to catch
    // (the refusal allowed through) is not the only way to get this test
    // green: flipping the `done` check to always-refuse would fail this
    // second half.
    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now(),
            done: true,
            report: Some(serde_json::json!({})),
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );
    let response = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "a finished scan must not block a fresh one"
    );
}

/// C4 §3's discipline, and `inventory`'s: a scan report names what it did
/// not do. [`state_with_a_root_that_cannot_write_its_document`] gives the
/// scan endpoint's own derive pass a document it cannot write, root-safely —
/// see that fixture's own doc comment for why not a `chmod`.
///
/// **Polls, rather than sleeping a fixed duration.** The fixture is a
/// handful of files on an in-memory database; the scan is expected to finish
/// well under this test's own ceiling, and polling is what keeps the test
/// from being slower than the scan it is timing, or flaky on a loaded CI
/// runner where a fixed sleep would sometimes be too short.
#[tokio::test]
async fn the_scan_report_names_what_it_could_not_read() {
    let (state, _fixture) =
        state_with_a_root_that_cannot_write_its_document("scan-unwritable-document");
    let root_id = only_root_id(&state);

    let started = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;
    assert_eq!(started.status(), StatusCode::ACCEPTED);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let json = loop {
        let response = crate::config_api::scan_status(
            axum::extract::State(std::sync::Arc::clone(&state)),
            axum::extract::Extension(an_administrator()),
            test_support::from_a_browser(),
            axum::extract::Path(root_id),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = test_support::body_string(response).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
        if json["running"] == false {
            break json;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the scan did not finish within 5s: {json}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };

    let report = &json["report"];
    // Whole, per C4 §3 — every field present, the empty collections
    // included, not only the one this test is actually about.
    assert!(report["scan"]["unreadable_dirs"].is_array(), "{report}");
    // O92: the four exclusion fields, present even though this fixture's root
    // excludes nothing. C4 §3.4 — "nothing excluded this time" and "this field
    // does not exist in this version" have to look different.
    assert!(report["scan"]["excluded_dirs"].is_array(), "{report}");
    assert!(
        report["scan"]["bad_exclude_patterns"].is_array(),
        "{report}"
    );
    assert!(report["scan"]["excluded_files"].is_number(), "{report}");
    assert!(
        report["scan"]["forgotten_by_exclusion"].is_number(),
        "{report}"
    );
    assert!(report["scan"]["taken_over"].is_number(), "{report}");
    assert!(report["scan"]["refused"].is_null(), "{report}");
    assert!(report["derive"]["raised"].is_number(), "{report}");
    assert!(report["derive"]["unfoldable"].is_array(), "{report}");
    assert!(report["derive"]["orphan_documents"].is_array(), "{report}");
    assert!(
        report["derive"]["declined_documents"].is_array(),
        "{report}"
    );
    assert!(report["derive"]["stale_documents"].is_array(), "{report}");

    // And the one field that is not empty: the document `.taiyaku`-as-a-file
    // kept this pass from writing.
    let unwritable = report["derive"]["unwritable_documents"]
        .as_array()
        .expect("unwritable_documents must be an array");
    assert!(
        !unwritable.is_empty(),
        "a document the derive pass could not write must be named, not silently dropped: {report}"
    );
}

/// A library holding one root that has already been scanned once with no
/// `exclude_globs`, with a real pattern set on it afterward — so the *next*
/// scan, the one [`the_scan_reports_each_exclusion_field_with_its_own_value`]
/// actually drives through the HTTP route, is the one that excludes something.
///
/// **Built so no two of the four exclusion fields land on the same value.**
/// [`scan_report_json`](crate::config_api::scan_report_json) destructures
/// `ScanReport` with no `..`, which closes a *missing* key but not a
/// transposition between two same-typed fields (`excluded_files` and
/// `forgotten_by_exclusion`, say). A fixture where two of the four happened to
/// share a value could not catch one; this one is built so they cannot:
///
/// - `Junk/` and the file inside it are recorded by the *first*, unexcluded
///   scan, then covered by the `Junk/` line once `exclude_globs` is set — a
///   directory-only pattern excludes the directory itself, not files by
///   name, so both rows land in `forgotten_by_exclusion`, never
///   `excluded_files`.
/// - `a.nfo`..`d.nfo` are written only *after* the first scan, so the
///   second, excluding scan meets all four for the first time and skips
///   them at the walk itself — `excluded_files`, not
///   `forgotten_by_exclusion`, since nothing was ever held for them to forget.
/// - `a{b` is a line `Excluder::build` cannot compile — the same shape
///   `taiyaku_library::scan::tests::check_exclude_globs_names_the_line_it_refuses`
///   uses — so `bad_exclude_patterns` holds exactly one complaint naming it.
fn state_with_a_root_scanned_once_then_given_real_exclusions(
    dir: &str,
) -> (std::sync::Arc<crate::ServeState>, test_support::Fixture) {
    let fixture = test_support::Fixture::new(dir);
    let root_dir = fixture.path().join("root");
    write_page_dir(&root_dir.join("Series").join("Series - v01"), 2);
    std::fs::create_dir_all(root_dir.join("Junk")).expect("mkdir");
    std::fs::write(root_dir.join("Junk").join("keep.txt"), b"keep").expect("write");

    let library = test_support::library();
    let root_id = {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds a root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("the first, unexcluded scan");
        root.id
    };

    // Written only now — after the first scan — so the excluding scan below
    // meets these for the first time rather than forgetting previously-held
    // rows for them. See this fixture's own doc comment.
    std::fs::write(root_dir.join("a.nfo"), b"a").expect("write");
    std::fs::write(root_dir.join("b.nfo"), b"b").expect("write");
    std::fs::write(root_dir.join("c.nfo"), b"c").expect("write");
    std::fs::write(root_dir.join("d.nfo"), b"d").expect("write");
    {
        let conn = library.conn();
        conn.execute(
            "UPDATE root SET exclude_globs = ?2 WHERE id = ?1",
            rusqlite::params![root_id, "Junk/\n*.nfo\na{b"],
        )
        .expect("setting exclude_globs");
    }

    let state = std::sync::Arc::new(crate::ServeState {
        library,
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    (state, fixture)
}

/// The four exclusion fields on the wire, driven through the real
/// `start_scan`/`scan_status` route pair rather than called directly, and
/// pinned to distinct values so a transposition between any two of them
/// fails this test. See
/// [`state_with_a_root_scanned_once_then_given_real_exclusions`]'s own doc
/// comment for exactly how each of the four gets a value nothing else in
/// this fixture shares.
#[tokio::test]
async fn the_scan_reports_each_exclusion_field_with_its_own_value() {
    let (state, _fixture) = state_with_a_root_scanned_once_then_given_real_exclusions(
        "taiyaku-test-scan-exclusion-fields-distinct",
    );
    let root_id = only_root_id(&state);

    let started = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;
    assert_eq!(started.status(), StatusCode::ACCEPTED);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let json = loop {
        let response = crate::config_api::scan_status(
            axum::extract::State(std::sync::Arc::clone(&state)),
            axum::extract::Extension(an_administrator()),
            test_support::from_a_browser(),
            axum::extract::Path(root_id),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = test_support::body_string(response).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
        if json["running"] == false {
            break json;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the scan did not finish within 5s: {json}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };

    let scan = &json["report"]["scan"];

    assert_eq!(
        scan["excluded_dirs"],
        serde_json::json!(["Junk"]),
        "the directory-only pattern excludes Junk, and only Junk: {scan}"
    );
    let bad_patterns = scan["bad_exclude_patterns"]
        .as_array()
        .expect("bad_exclude_patterns must be an array");
    assert_eq!(
        bad_patterns.len(),
        1,
        "exactly one line of exclude_globs will not compile: {scan}"
    );
    assert!(
        bad_patterns[0].as_str().is_some_and(|s| s.contains("a{b")),
        "the complaint names the line it refuses: {scan}"
    );
    assert_eq!(
        scan["excluded_files"], 4,
        "a.nfo through d.nfo, met for the first time by the excluding scan: {scan}"
    );
    assert_eq!(
        scan["forgotten_by_exclusion"], 2,
        "Junk and keep.txt, held from the first scan and now covered by Junk/: {scan}"
    );

    // The point of the fixture: no two of the four hold the same value, so a
    // transposition between any pair of them is something this test can
    // actually see.
    assert_ne!(
        scan["excluded_files"], scan["forgotten_by_exclusion"],
        "the two number fields must differ, or a swap between them is invisible: {scan}"
    );
    assert_ne!(
        scan["excluded_dirs"], scan["bad_exclude_patterns"],
        "the two array fields must differ, or a swap between them is invisible: {scan}"
    );
}

/// **[`absorbed_json`](crate::config_api::absorbed_json) must render
/// `orphan_documents[_]["absorbed_into"]` as the work the fold absorbed the
/// orphan into, not as a default value.**
/// [`state_with_a_root_that_folds_a_hand_documented_work`] seeds a
/// hand-documented colour work and a fold rule that has not run yet; this
/// drives the real `start_scan`/`scan_status` route pair, the same polling loop
/// [`the_scan_report_names_what_it_could_not_read`] uses, so the fold and the
/// JSON it produces both come from the route under test, not from a fixture
/// that ran the fold itself and merely echoed the answer.
#[tokio::test]
async fn a_scan_report_names_the_work_a_folded_documents_orphan_was_absorbed_into() {
    let (state, _fixture) = state_with_a_root_that_folds_a_hand_documented_work(
        "taiyaku-test-scan-fold-absorbs-document",
    );
    let root_id = only_root_id(&state);

    let started = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;
    assert_eq!(started.status(), StatusCode::ACCEPTED);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let json = loop {
        let response = crate::config_api::scan_status(
            axum::extract::State(std::sync::Arc::clone(&state)),
            axum::extract::Extension(an_administrator()),
            test_support::from_a_browser(),
            axum::extract::Path(root_id),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = test_support::body_string(response).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
        if json["running"] == false {
            break json;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the scan did not finish within 5s: {json}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };

    // Read back after the scan, by query — the same discipline `rule_row`'s
    // own doc comment states — rather than trusting the route's own JSON for
    // what the fold actually left behind.
    let (target_slug, target_title): (String, String) = {
        let conn = state.library.conn();
        conn.query_row("SELECT slug, title FROM work", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("exactly one work remains once the fold has run")
    };

    let report = &json["report"];
    assert_eq!(
        report["derive"]["emptied_works"], 1,
        "the fixture only means something if the colour work is actually folded away on this \
         scan's derive pass: {report}"
    );
    let orphans = report["derive"]["orphan_documents"]
        .as_array()
        .expect("orphan_documents must be an array");
    assert_eq!(
        orphans.len(),
        1,
        "the folded work's own document must be reported orphaned: {report}"
    );
    let absorbed = &orphans[0]["absorbed_into"];
    assert_eq!(
        absorbed["slug"].as_str(),
        Some(target_slug.as_str()),
        "the orphan must name the work the fold actually absorbed it into, not a default value: \
         {report}"
    );
    assert_eq!(
        absorbed["title"].as_str(),
        Some(target_title.as_str()),
        "and that work's own title, not a default value: {report}"
    );
}

/// `scan_jobs::run`'s own analogue of
/// `the_preview_route_does_not_wait_on_the_shared_connections_mutex`, that
/// test's own doc comment gives the whole reasoning: every other fixture in
/// this crate is in-memory, where `Library::working` hands back the very
/// same guard `Library::conn` would, so swapping `working()` for `conn()`
/// inside `scan_jobs::run` is invisible to any test built over one of them.
/// Only a **file-backed** library gives `working()` a genuine second
/// connection, which is the property this test exists to prove is actually
/// used — for a walk plus a library-wide derive, longer than a preview ever
/// runs.
///
/// Calls `scan_jobs::run` directly rather than through `start_scan`: the
/// route's own first `spawn_blocking` looks up the root through `conn()`
/// too, so holding the shared mutex around the whole request would stall on
/// that fast lookup and prove nothing about the scan itself.
#[tokio::test]
async fn a_scan_does_not_wait_on_the_shared_connections_mutex() {
    let fixture = test_support::Fixture::new("scan-second-connection");
    let root_dir = fixture.path().join("root");
    let volume_dir = root_dir.join("Series").join("Series - v01");
    std::fs::create_dir_all(&volume_dir).expect("mkdir");
    std::fs::write(volume_dir.join("001.jpg"), b"not really a jpeg").expect("write");

    let db_path = fixture.path().join("library.db");
    let library = taiyaku_library::Library::open(&db_path).expect("opens a file-backed library");
    let root = {
        let conn = library.conn();
        taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds the root")
    };
    let library = std::sync::Arc::new(library);

    // A background thread takes `Library::conn`'s mutex and holds it — the
    // identical guard `the_preview_route_does_not_wait_on_the_shared_connections_mutex`
    // uses, for the identical reason.
    let holder_library = std::sync::Arc::clone(&library);
    let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let guard = holder_library.conn();
        held_tx.send(()).expect("the test is still waiting");
        let _ = release_rx.recv_timeout(std::time::Duration::from_secs(5));
        drop(guard);
    });
    held_rx.recv().expect("the holder thread took the mutex");

    let started = std::time::Instant::now();
    let job = crate::LibraryJob::default();
    let result = crate::test_support::run_unreserved(
        &library,
        std::slice::from_ref(&root),
        crate::scan_jobs::Reread::Dir,
        &job,
    );
    let elapsed = started.elapsed();

    release_tx.send(()).ok();
    holder.join().expect("the holder thread did not panic");

    assert!(result.is_ok(), "{result:?}");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "the scan waited {elapsed:?} for `Library::conn`'s mutex while a background thread \
         held it — `scan_jobs::run` must open `Library::working`'s own connection instead of \
         `conn()`"
    );
}

/// C4 §3 and `ServeState::scans`'s own doc comment both say a root's scan slot
/// is independent of every other root's. This uses both ids
/// `state_with_two_scanned_roots` provides in one assertion, so a
/// single-flight check broad enough to serialise every root would fail here:
/// `scans.values().any(|p| !p.done)` rather than `scans.get(&id)`.
#[tokio::test]
async fn a_second_roots_scan_is_not_blocked_by_the_first_roots_own_scan() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let root_ids: Vec<i64> = {
        let conn = state.library.conn();
        let mut stmt = conn
            .prepare("SELECT id FROM root ORDER BY id")
            .expect("prepares the root id query");
        stmt.query_map([], |r| r.get(0))
            .expect("queries the root ids")
            .collect::<Result<Vec<i64>, _>>()
            .expect("reads the root ids")
    };
    assert_eq!(root_ids.len(), 2, "{root_ids:?}");
    let (root_a, root_b) = (root_ids[0], root_ids[1]);

    // Root A is in flight; root B has never been touched.
    state.scans.lock().unwrap().insert(
        root_a,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now(),
            done: false,
            report: None,
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    let response = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_b),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "root B must be scannable while only root A is in flight — the registry is keyed per \
         root, not one flag shared by every root"
    );
}

/// An `id` naming no root this library holds is the one case `scan_status`
/// answers `404` for, exactly like `start_scan`'s own 404 on the same path.
#[tokio::test]
async fn scan_status_404s_for_a_root_this_library_does_not_hold() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = crate::config_api::scan_status(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(999_999),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// A root the registry holds nothing for is not the same as a root this
/// library does not hold. `state_with_two_scanned_roots` scans both of its
/// roots directly through `taiyaku_library::scan::scan_root`, never through
/// `start_scan`, so its registry holds nothing for either id — exactly the
/// shape a restarted process, or a root this process has simply never scanned,
/// leaves behind. `scan_status` must fall back to the database rather than
/// 404ing on a real root with a real `last_scan_at`/`last_scan_status` sitting
/// on its row.
#[tokio::test]
async fn scan_status_reads_the_database_for_a_known_root_with_no_registry_entry() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let root_id = test_support::second_root_id(&state);
    assert!(
        state.scans.lock().unwrap().get(&root_id).is_none(),
        "this fixture scans its roots directly, never through `start_scan`, so the registry \
         must hold nothing for this id"
    );

    let response = crate::config_api::scan_status(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(json["running"], false, "{json}");
    assert!(json["report"].is_null(), "{json}");
    assert_eq!(
        json["last_scan_status"], "ok",
        "the database's own record of this root's real scan must come through: {json}"
    );
    assert!(
        json["last_scan_at"].is_string(),
        "a root this fixture scanned must carry a real timestamp: {json}"
    );
}

/// `root.id` is `INTEGER PRIMARY KEY` with no `AUTOINCREMENT`, so SQLite
/// reissues a freed id to the very next `add_root`. A registry entry
/// `delete_root` left standing under that id would then answer for a root it
/// never scanned — a `GET` on the new root's own `/scan` route reporting the
/// *deleted* root's stale finished report.
#[tokio::test]
async fn deleting_a_root_clears_its_stale_scan_registry_entry() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    // A finished scan's report, planted directly — the shape a real
    // `start_scan`/background task would have left behind.
    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now(),
            done: true,
            report: Some(serde_json::json!({ "note": "the deleted root's own report" })),
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({}),
    )
    .await;
    let text = test_support::body_string(response).await;
    assert!(text.contains("\"removed\""), "{text}");

    assert!(
        state.scans.lock().unwrap().get(&root_id).is_none(),
        "delete_root must clear the registry slot for the id it just removed"
    );

    // A fresh root, added in the same process — on a table `delete_root` just
    // emptied, with no `AUTOINCREMENT`, SQLite reissues the id it just freed.
    let new_fixture = test_support::Fixture::new("reused-root-id");
    std::fs::create_dir_all(new_fixture.path().join("New Root")).expect("mkdir");
    let new_id = {
        let working = state.library.working().expect("a working connection");
        taiyaku_library::scan::add_root_checked(&working, new_fixture.path())
            .expect("adds the directory")
            .root
            .id
    };
    assert_eq!(
        new_id, root_id,
        "this test only proves what it means to prove if the freed id was actually reused"
    );

    let status = crate::config_api::scan_status(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(new_id),
    )
    .await;
    let body = test_support::body_string(status).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert!(
        json["report"].is_null(),
        "the new root at a reused id must not answer with the deleted root's stale report: \
         {json}"
    );
}

/// **The other half of the registry-clearing guard**, untested by the test
/// above: `delete_root` clears the registry slot only for a real removal that
/// actually committed — `!dry_run && matches!(&removed, Ok(Ok(_)))`. A
/// `?dry_run=true` call runs [`taiyaku_library::scan::removal_cost`], which
/// answers `Ok(Ok(_))` exactly as a real removal would while rolling its own
/// transaction back — nothing is actually removed — so `&&` turning into `||`
/// would clear a live registry entry over a request that changed nothing at
/// all, the moment anyone asks "what would removing this root cost".
#[tokio::test]
async fn a_dry_run_delete_leaves_the_scan_registry_entry_alone() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now(),
            done: true,
            report: Some(serde_json::json!({ "note": "a live, finished report" })),
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/roots/{root_id}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    let text = test_support::body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
    assert_eq!(json["dry_run"], true, "{json}");

    assert!(
        state.scans.lock().unwrap().get(&root_id).is_some(),
        "a dry run must not clear the registry slot for a root nothing was actually removed \
         from — the id was never freed, so nothing can have been reissued onto it"
    );

    let status = crate::config_api::scan_status(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
    )
    .await;
    let body = test_support::body_string(status).await;
    let status_json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert_eq!(
        status_json["report"],
        serde_json::json!({ "note": "a live, finished report" }),
        "the finished report the registry held before the dry run must still be the one \
         scan_status answers with: {status_json}"
    );
}

/// `last_scan_at` and `last_scan_status` are the two columns this whole route
/// exists to show (its own doc comment), and C4 §3's regime line reads them
/// while a scan is running or has just finished — exactly the moment a caller
/// is watching it. This plants a registry entry for a root that already has a
/// real `last_scan_at`/`last_scan_status = "ok"` on its row (from the
/// fixture's own scan) and checks both still come through once a live registry
/// entry exists too.
///
/// **`elapsed_secs` is checked alongside**: `started` is backdated by 500ms,
/// under a second, so a truncating `Duration::as_secs` would answer exactly
/// `0` regardless — this asserts a nonzero, fractional value instead.
#[tokio::test]
async fn scan_status_carries_both_database_columns_even_with_a_registry_entry() {
    let (_router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let root_id = test_support::second_root_id(&state);

    {
        let conn = state.library.conn();
        let (last_scan_at, last_scan_status): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1",
                [root_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("reading the fixture's own real scan status");
        assert!(
            last_scan_at.is_some(),
            "the fixture's own scan must have written a real last_scan_at, or this test proves \
             nothing"
        );
        assert_eq!(last_scan_status.as_deref(), Some("ok"));
    }

    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now() - std::time::Duration::from_millis(500),
            done: false,
            report: None,
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    let response = crate::config_api::scan_status(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("a JSON body");

    assert_eq!(json["running"], true, "{json}");
    assert!(
        json["last_scan_at"].is_string(),
        "a live registry entry must not blank out the database's own last_scan_at: {json}"
    );
    assert_eq!(
        json["last_scan_status"], "ok",
        "a live registry entry must not blank out the database's own last_scan_status: {json}"
    );
    let elapsed = json["elapsed_secs"]
        .as_f64()
        .expect("elapsed_secs must be a number");
    assert!(
        elapsed > 0.0,
        "500ms backdated must not truncate to 0, the way Duration::as_secs would: {json}"
    );
}

/// `?full=` is in C4 §3's own API table for this route, and `axum` drops an
/// unrecognised query key silently, so without `ScanQuery` a
/// `POST .../scan?full=true` would answer `202` and run the identical scan a
/// plain `POST` would, accepted and quietly ignored. It is refused rather
/// than swallowed, and before the registry is ever touched.
#[tokio::test]
async fn a_full_query_parameter_is_refused_rather_than_silently_ignored() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/roots/{root_id}/scan?full=true"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(
        text.contains("full"),
        "the refusal must name the parameter it refused: {text}"
    );
    assert!(
        state.scans.lock().unwrap().get(&root_id).is_none(),
        "a refused request must never reserve a registry slot"
    );
}

/// The failure `spawn_blocking` itself hands back is a **500 naming what was
/// running** — [`crate::config_api::panicked`]'s own contract, restating
/// `curation::tests::a_closure_that_panicked_is_a_500_naming_what_was_running`
/// for this module's own copy of the helper. See that test's own doc
/// comment for why it is called directly rather than driven through a
/// route: none of this module's own closures offers a lever to panic on
/// purpose either, for the identical reason (`Library::conn` recovers a
/// poisoned mutex rather than unwrapping it), and the `JoinError` a real
/// panic produces is what the routes' own `Err` arms actually receive.
#[tokio::test]
async fn a_closure_that_panicked_is_a_500_naming_what_was_running() {
    let why = tokio::task::spawn_blocking(|| {
        panic!("a config_api closure, for the join error it produces");
    })
    .await
    .expect_err("a panicking closure joins as an error");

    let response = crate::config_api::panicked("listing rules", &why);
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(
        body.contains("listing rules panicked"),
        "it names what was running, not just that something did: {body}"
    );
}

/// [`crate::config_api::admin_panicked`]'s own contract — [`panicked`]'s
/// twin for the roots routes, worded through JSON (`AdminAnswer`) rather
/// than plain text (`NoAnswer`), so this checks the `"error"` key rather
/// than the raw body.
#[tokio::test]
async fn a_roots_closure_that_panicked_is_a_500_naming_what_was_running() {
    let why = tokio::task::spawn_blocking(|| {
        panic!("a config_api roots closure, for the join error it produces");
    })
    .await
    .expect_err("a panicking closure joins as an error");

    let response = crate::config_api::admin_panicked("listing roots", &why);
    let status = response.status();
    let body = test_support::body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(
        json["error"]
            .as_str()
            .is_some_and(|error| error.contains("listing roots panicked")),
        "it names what was running, not just that something did: {body}"
    );
}

/// **I4.** `save_rules` and a scan's own derive both take
/// [`crate::ServeState::library_job`] before running — see that field's own
/// doc comment for the interleaving it closes. A background thread holding
/// the lock stands in for a scan actually holding it, which is simpler and
/// deterministic: this is a test of `save_rules`'s own refusal, not of
/// `scan_root`/`derive::derive`'s timing.
#[tokio::test]
async fn a_save_refuses_while_the_library_job_lock_is_held() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let job = std::sync::Arc::clone(&state.library_job);
    let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let guard = job
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        held_tx.send(()).expect("the test is still waiting");
        let _ = release_rx.recv_timeout(std::time::Duration::from_secs(5));
        drop(guard);
    });
    held_rx.recv().expect("the holder thread took the lock");

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": existing }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;

    release_tx.send(()).ok();
    holder.join().expect("the holder thread did not panic");

    assert_eq!(status, StatusCode::CONFLICT, "{text}");
    assert_eq!(
        text, "a scan or another library job is running; save again once it has finished",
        "the refusal must say why, the same spirit as the existing 409 for a second scan of \
         one root"
    );

    // Once the lock is free, the identical request must succeed — proving
    // the refusal above was about contention, not a route that always fails.
    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": existing }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
}

/// **The route derives**, asserted by reading the grouping back after the
/// save rather than by trusting the `impact` it reported. Spec C4 §7: a
/// handler that wrote the rule table and returned would answer the identical
/// body and leave this red — which is precisely the defect this route exists
/// to close, since every other rules route does exactly that.
#[tokio::test]
async fn saving_rules_derives_over_the_library() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));

    let before = {
        let conn = state.library.conn();
        taiyaku_library::preview::grouping(&conn).expect("the grouping before the save")
    };

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("a JSON body");

    let after = {
        let conn = state.library.conn();
        taiyaku_library::preview::grouping(&conn).expect("the grouping after the save")
    };
    assert_ne!(
        before, after,
        "the save must have re-derived the library, not merely written the rule table: {json}"
    );
    assert!(
        json["impact"]["works_rehomed"].as_u64().unwrap() > 0,
        "{json}"
    );
    assert_eq!(
        json["rules"].as_array().unwrap().len(),
        proposed.len(),
        "the answer carries the rules as written: {json}"
    );
    assert!(
        json["reopened_proposals"].is_array(),
        "the same key delete_rule and import_rules answer: {json}"
    );
    assert!(json["complaints"].is_array(), "{json}");
}

/// **The save and the preview it was made from must agree** — the screen
/// compares them and draws a warning when they differ, and this is the check
/// that the comparison is between two things that can agree at all.
#[tokio::test]
async fn a_save_answers_the_impact_its_own_preview_predicted() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));

    let previewed = preview_json(&router, &serde_json::json!({ "rules": proposed })).await;
    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the save must succeed before its body can be compared to the preview's"
    );
    let saved: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("a JSON body");

    assert_eq!(
        previewed["impact"], saved["impact"],
        "the preview and the save it was made from disagree, which is the one thing this \
         screen's promise cannot survive"
    );
}

/// A stale editor — a rule the caller's own copy still shows, deleted by
/// somebody else since — is the caller's mistake, not a store fault.
/// [`crate::config_api::preview_rules`]'s own doc comment states the same fold
/// for the same reason.
#[tokio::test]
async fn saving_a_rule_that_no_longer_exists_is_the_callers_mistake() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let mut proposed = listed["rules"].as_array().cloned().expect("rules array");
    proposed[0]["id"] = serde_json::json!(9_999);

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = test_support::body_string(response).await;
    assert!(
        text.contains("9999") || text.contains("no such rule"),
        "the refusal must name the rule that is not there: {text}"
    );
}

/// [`creating_a_rule_against_an_unknown_root_is_a_400_naming_it`]'s twin for
/// `PUT /api/library/rules`. With `foreign_keys = ON`, a proposed rule scoped
/// to a root that does not exist would reach `rules::replace_all`'s own
/// foreign key and answer with SQLite's raw `FOREIGN KEY constraint failed`
/// text unless `save_rules` checked its `root_id` the way `create_rule` and
/// `patch_rule` do.
#[tokio::test]
async fn saving_a_rule_scoped_to_an_unknown_root_is_a_400_naming_it() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    let mut stray = retitle_rule(&existing);
    stray["root_id"] = serde_json::json!(999_999);
    proposed.push(stray);

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;

    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(
        text.contains("999999"),
        "the 400 must name the missing root: {text}"
    );
    assert!(
        !text.to_lowercase().contains("foreign key"),
        "a caller's own mistake must not surface SQLite's own text: {text}"
    );
}

/// The same bound the preview route holds, spelled the same way — spec §4.2:
/// the same list previews and saves, so the client cannot send one shape to
/// one and another to the other.
#[tokio::test]
async fn saving_more_rules_than_the_limit_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");
    let one = existing.first().cloned().expect("at least one seeded rule");

    let mut proposed = Vec::new();
    for _ in 0..=201 {
        let mut copy = one.clone();
        copy["id"] = serde_json::json!(0);
        proposed.push(copy);
    }

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = test_support::body_string(response).await;
    assert!(text.contains("200"), "the refusal names the limit: {text}");
}

/// A body with no `rules` array at all.
#[tokio::test]
async fn saving_without_a_rules_array_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = test_support::body_string(response).await;
    assert!(
        text.contains("save body"),
        "the refusal must name the route the caller actually reached: {text}"
    );
}

/// **The `noun` [`crate::config_api::read_proposed_rules`] takes is load-bearing,
/// and only a pair of tests can see it.** That function is shared by the
/// preview and the save precisely so the two cannot drift about what a
/// proposed list is — but the one thing it must *not* share is which route
/// the caller reached, and a single-route test passes just as well against a
/// reader that hard-codes either noun or ignores the parameter outright.
/// This is the other half of the pair; the save's own is directly above.
#[tokio::test]
async fn previewing_without_a_rules_array_is_refused_naming_the_preview() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let response = send_json(
        &router,
        Method::POST,
        "/api/library/rules/preview",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = test_support::body_string(response).await;
    assert!(
        text.contains("preview body"),
        "the refusal must name the route the caller actually reached: {text}"
    );
}

/// An `exclude_globs` line that will not compile is refused at 400 and the
/// column is left alone, so a typo cannot be stored for a scan to meet
/// later.
///
/// **Both halves.** A test asserting only the status passes against a
/// handler that answers 400 after writing; a test asserting only the column
/// passes against one that rejects every patch.
#[tokio::test]
async fn a_roots_patch_refuses_an_exclude_globs_line_that_will_not_compile() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::json!("extras")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // `a{b` is a shape `ignore` actually rejects. `a[` and `[[:bogus:]]`
    // compile, so neither would exercise this route.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::json!("extras\na{b")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = test_support::body_string(response).await;
    assert!(text.contains("a{b"), "the refusal names the line: {text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("a JSON body"),
        serde_json::json!({ "error": "the directory's `exclude_globs` line `a{b` is not a usable pattern: error parsing \
                      glob 'a{b': unclosed alternate group; missing '}' (maybe escape '{' with \
                      '[{]'?)" }),
        "the directory's words, whole"
    );

    assert_eq!(
        root_text_column(&state, root_id, "exclude_globs").as_deref(),
        Some("extras"),
        "the stored pattern survived the refused patch"
    );
}

/// Clearing the column is not a pattern and must not be validated as one.
#[tokio::test]
async fn a_roots_patch_may_still_clear_exclude_globs() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::json!("extras")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::Value::Null),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_text_column(&state, root_id, "exclude_globs"),
        None,
        "an empty column is not a pattern and must not be refused as one"
    );
}

/// A caught panic's message is read from a `&str` payload, from a `String`
/// one, and, from any other, is said not to be readable.
#[test]
fn a_panics_message_is_read_from_either_string_payload() {
    let caught = |f: fn()| std::panic::catch_unwind(f).expect_err("panics");
    assert_eq!(
        crate::scan_jobs::panic_message(caught(|| panic!("a literal")).as_ref()),
        "a literal"
    );
    assert_eq!(
        crate::scan_jobs::panic_message(
            caught(|| panic!("{}", String::from("a built one"))).as_ref()
        ),
        "a built one"
    );
    assert_eq!(
        crate::scan_jobs::panic_message(caught(|| std::panic::panic_any(7_u8)).as_ref()),
        "the scan panicked with a payload that names no message"
    );
}

/// A root's scan that panics is its `{"error": …}` and says so in the
/// sentence's first words.
#[test]
fn a_scan_that_panicked_is_an_error_report_that_says_so() {
    let outcome = std::panic::catch_unwind(|| -> Result<Vec<serde_json::Value>, String> {
        panic!("a walk went wrong")
    });
    assert_eq!(
        crate::scan_jobs::reports_of(1, outcome),
        vec![serde_json::json!({ "error": "the scan panicked: a walk went wrong" })]
    );
    assert_eq!(
        crate::scan_jobs::reports_of(1, Ok(Err("it failed".to_string()))),
        vec![serde_json::json!({ "error": "it failed" })]
    );
}

/// While a scan of a directory is reserved, neither a rehearsal nor a removal
/// of it goes ahead: 409 in a sentence naming the directory, nothing removed,
/// and the reservation left as it was. Once the scan is done, it is removed.
#[tokio::test]
async fn a_dir_is_not_removed_while_a_scan_of_it_is_reserved() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);
    let path = taiyaku_library::scan::roots(&state.library.conn()).expect("roots")[0]
        .path
        .display()
        .to_string();
    let started = std::time::Instant::now();
    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress {
            waiting: false,
            started,
            done: false,
            report: None,
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    for query in ["?dry_run=true", ""] {
        let response = send_json(
            &router,
            Method::DELETE,
            &format!("/api/library/roots/{root_id}{query}"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{query}");
        let body: serde_json::Value =
            serde_json::from_str(&test_support::body_string(response).await).expect("JSON");
        assert_eq!(
            body,
            serde_json::json!({
                "error": format!("a scan of {path} is running; remove the directory once it has finished")
            })
        );
        assert_eq!(
            only_root_id(&state),
            root_id,
            "{query}: nothing was removed"
        );
        let scans = state.scans.lock().unwrap();
        let held = &scans[&root_id];
        assert!(
            !held.done && held.started == started,
            "{query}: the reservation stands"
        );
    }

    state
        .scans
        .lock()
        .unwrap()
        .get_mut(&root_id)
        .expect("held")
        .done = true;
    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// A directory's scan that waited while its directory was removed, and another
/// directory took the freed id, walks nothing: nothing of the removed directory's
/// is filed under the new one, the new one is not marked scanned, and the
/// report says why.
#[tokio::test]
async fn a_dirs_scan_walks_nothing_when_its_dir_changed_while_it_waited() {
    let (_router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let asked = taiyaku_library::scan::roots(&state.library.conn())
        .expect("roots")
        .remove(0);
    let started = std::time::Instant::now();
    state.scans.lock().unwrap().insert(
        asked.id,
        crate::ScanProgress {
            waiting: false,
            started,
            done: false,
            report: None,
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );
    {
        // Through the store, as a removal that landed between the `DELETE`'s
        // check and its commit would.
        let working = state.library.working().expect("a working connection");
        taiyaku_library::scan::remove_root(&working, asked.id).expect("removes the directory");
        let other = fixture.path().join("Other");
        std::fs::create_dir_all(&other).expect("mkdir");
        let added =
            taiyaku_library::scan::add_root_checked(&working, &other).expect("adds another");
        assert_eq!(
            added.root.id, asked.id,
            "the freed id went to the new directory"
        );
    }

    let job = std::sync::Arc::clone(&state);
    let root = asked.clone();
    crate::scan_jobs::spawn(job, vec![root], started, crate::scan_jobs::Reread::Dir)
        .await
        .expect("the job ends");

    let conn = state.library.conn();
    let entries: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM scan_entry WHERE root_id = ?1",
            [asked.id],
            |r| r.get(0),
        )
        .expect("counts");
    assert_eq!(
        entries, 0,
        "nothing of the removed directory's was filed under the new one"
    );
    let status: Option<String> = conn
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [asked.id],
            |r| r.get(0),
        )
        .expect("reads");
    assert_eq!(status, None, "the new directory was never scanned");
    drop(conn);
    assert_eq!(
        state.scans.lock().unwrap()[&asked.id].report,
        Some(serde_json::json!({ "error": crate::scan_jobs::DIR_CHANGED }))
    );
}

/// A finished directory scan's report lands on its own reservation and on no
/// other: not on one another scan made on the same id, and not on an id a
/// removal cleared.
#[tokio::test]
async fn a_dirs_scan_lands_only_on_its_own_reservation() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root = taiyaku_library::scan::roots(&state.library.conn())
        .expect("roots")
        .remove(0);
    let ours = std::time::Instant::now();
    let theirs = ours + std::time::Duration::from_millis(1);
    state.scans.lock().unwrap().insert(
        root.id,
        crate::ScanProgress {
            waiting: false,
            started: theirs,
            done: false,
            report: None,
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    let job = std::sync::Arc::clone(&state);
    let asked = root.clone();
    crate::scan_jobs::spawn(job, vec![asked], ours, crate::scan_jobs::Reread::Dir)
        .await
        .expect("the job ends");
    {
        let scans = state.scans.lock().unwrap();
        let held = &scans[&root.id];
        assert!(!held.done && held.started == theirs && held.report.is_none());
    }

    state.scans.lock().unwrap().remove(&root.id);
    let job = std::sync::Arc::clone(&state);
    let asked = root.clone();
    crate::scan_jobs::spawn(job, vec![asked], ours, crate::scan_jobs::Reread::Dir)
        .await
        .expect("the job ends");
    assert!(
        state.scans.lock().unwrap().get(&root.id).is_none(),
        "a cleared slot stays clear"
    );
}

/// A directory's scan walks the directory as it stands once the scan starts: a
/// setting changed between the request's reading and the job lock is the one
/// the walk honours.
#[tokio::test]
async fn a_dirs_scan_walks_the_settings_the_dir_holds_when_it_starts() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let asked = taiyaku_library::scan::roots(&state.library.conn())
        .expect("roots")
        .remove(0);
    test_support::write_pages(&asked.path.join("Extra").join("Extra - v01"), 2);
    let entries_under_extra = |state: &crate::ServeState| -> i64 {
        state
            .library
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM scan_entry WHERE root_id = ?1 AND rel_path LIKE 'Extra%'",
                [asked.id],
                |r| r.get(0),
            )
            .expect("counts")
    };
    let job = std::sync::Arc::clone(&state);
    let first = asked.clone();
    crate::scan_jobs::spawn(
        job,
        vec![first],
        std::time::Instant::now(),
        crate::scan_jobs::Reread::Dir,
    )
    .await
    .expect("the job ends");
    assert!(
        entries_under_extra(&state) > 0,
        "the first walk recorded Extra"
    );

    // After the request read the directory, and before the job took its lock.
    let patch = taiyaku_library::scan::RootPatch {
        exclude_globs: Some(Some("Extra/".to_string())),
        ..Default::default()
    };
    let working = state.library.working().expect("a working connection");
    assert!(taiyaku_library::scan::update_root(&working, asked.id, &patch).expect("patches"));
    drop(working);

    let job = std::sync::Arc::clone(&state);
    let stale = asked.clone();
    assert_eq!(
        stale.exclude_globs, None,
        "the request read the directory before the change"
    );
    crate::scan_jobs::spawn(
        job,
        vec![stale],
        std::time::Instant::now(),
        crate::scan_jobs::Reread::Dir,
    )
    .await
    .expect("the job ends");
    assert_eq!(
        entries_under_extra(&state),
        0,
        "the walk honoured the exclusion it found"
    );
}

/// Spec §4: removing a directory hands its files to the directories around and
/// inside it, so a scan of either holds the removal up as a scan of the
/// directory itself does. While Manga's scan of `m` is reserved, neither a
/// rehearsal nor a removal of Kids' `m/kids` goes ahead, nor of `m` while
/// Kids' scan is: 409 naming the directory being scanned, and nothing removed.
#[tokio::test]
async fn a_dir_is_not_removed_while_a_scan_around_or_inside_it_is_reserved() {
    for inner_removed in [true, false] {
        let n = test_support::router_over_a_library_nested_in_another();
        let (removed, scanning) = if inner_removed {
            (n.m_kids, n.m)
        } else {
            (n.m, n.m_kids)
        };
        let path = taiyaku_library::scan::roots(&n.state.library.conn())
            .expect("roots")
            .into_iter()
            .find(|root| root.id == scanning)
            .expect("the directory being scanned")
            .path
            .display()
            .to_string();
        n.state.scans.lock().unwrap().insert(
            scanning,
            crate::ScanProgress {
                waiting: false,
                started: std::time::Instant::now(),
                done: false,
                report: None,
                library: None,
                job: crate::test_support::a_planted_job(),
            },
        );

        for query in ["?dry_run=true", ""] {
            let response = send_json(
                &n.admin,
                Method::DELETE,
                &format!("/api/library/roots/{removed}{query}"),
                &serde_json::json!({}),
            )
            .await;
            assert_eq!(
                response.status(),
                StatusCode::CONFLICT,
                "inner_removed={inner_removed} {query}"
            );
            let body: serde_json::Value =
                serde_json::from_str(&test_support::body_string(response).await).expect("JSON");
            assert_eq!(
                body,
                serde_json::json!({
                    "error": format!("a scan of {path} is running; remove the directory once it has finished")
                }),
                "inner_removed={inner_removed} {query}"
            );
            assert!(
                taiyaku_library::scan::roots(&n.state.library.conn())
                    .expect("roots")
                    .iter()
                    .any(|root| root.id == removed),
                "inner_removed={inner_removed} {query}: nothing was removed"
            );
        }
    }
}

/// A file of Kids' directory that a removal of Kids leaves on disk and that no
/// directory left would walk.
#[derive(Clone, Copy, Debug, PartialEq)]
enum KeptOnDisk {
    /// Kids and Manga both came to exclude it after Kids took it over.
    BothExclude,
    /// Kids holds it; Manga excludes it.
    OuterExcludes,
    /// Kids holds it; Little, nested in Kids and added after its scan,
    /// excludes it, and Manga admits it.
    InnermostExcludes,
    /// As `InnermostExcludes`, and Manga excludes it too.
    InnermostAndOuterExclude,
    /// As `InnermostExcludes`, but Kids came to exclude it too, so Little's
    /// next scan dropped Kids' row, and then Kids lifted its pattern.
    InnermostExcludesKidsRowDropped,
}

/// The owner's rulings of 2026-11-10, "Never delete on disk", "Cover both"
/// and "Every removal", as the removal dialogs count them. Kids took Kids
/// Book over at its first scan, and a position and a note are on its work. In
/// every layout of [`KeptOnDisk`] removing Kids' directory or Kids' library
/// would remove nothing of it: both rehearsals count no work, no position and
/// no note.
#[tokio::test]
async fn the_removal_dialogs_count_nothing_for_a_file_kept_on_disk() {
    for case in [
        KeptOnDisk::BothExclude,
        KeptOnDisk::OuterExcludes,
        KeptOnDisk::InnermostExcludes,
        KeptOnDisk::InnermostAndOuterExclude,
        KeptOnDisk::InnermostExcludesKidsRowDropped,
    ] {
        let n = test_support::router_over_a_library_nested_in_another();
        {
            let conn = n.state.library.conn();
            let set = |id: i64, globs: Option<&str>| {
                conn.execute(
                    "UPDATE root SET exclude_globs = ?1 WHERE id = ?2",
                    rusqlite::params![globs, id],
                )
                .expect("sets the patterns");
            };
            let kids = taiyaku_library::scan::roots(&conn)
                .expect("roots")
                .into_iter()
                .find(|root| root.id == n.m_kids)
                .expect("Kids' directory");
            let book = kids.path.join("Kids Book").join("Kids Book - v01");
            let book = book.to_string_lossy();
            let work: i64 = conn
                .query_row(
                    "SELECT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
                      WHERE m.path = ?1",
                    [&book],
                    |r| r.get(0),
                )
                .expect("Kids Book's work");
            let reader: i64 = conn
                .query_row("SELECT id FROM user WHERE name = 'kids'", [], |r| r.get(0))
                .expect("Kids' reader");
            conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (?1, ?2, 'raw', 1)",
                [reader, work],
            )
            .expect("a position");
            conn.execute(
                "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
                 VALUES (?1, 'n', ?2, 'raw', 1, 'a note', 'now', 'now')",
                [reader, work],
            )
            .expect("a note");
            if matches!(
                case,
                KeptOnDisk::BothExclude
                    | KeptOnDisk::OuterExcludes
                    | KeptOnDisk::InnermostAndOuterExclude
            ) {
                set(n.m, Some("kids/Kids Book/"));
            }
            if case == KeptOnDisk::BothExclude {
                set(n.m_kids, Some("Kids Book/"));
                // Something Kids still reads, so its scan is not refused as an
                // empty read (§8.1).
                test_support::write_pages(
                    &kids.path.join("Other Book").join("Other Book - v01"),
                    2,
                );
                let kids = taiyaku_library::scan::roots(&conn)
                    .expect("roots")
                    .into_iter()
                    .find(|root| root.id == n.m_kids)
                    .expect("Kids' directory");
                let report = taiyaku_library::scan::scan_root(&conn, &kids).expect("scans Kids");
                assert!(report.refused.is_none(), "{case:?}: {report:?}");
                taiyaku_library::derive::derive(&conn).expect("derives");
            }
            if matches!(
                case,
                KeptOnDisk::InnermostExcludes
                    | KeptOnDisk::InnermostAndOuterExclude
                    | KeptOnDisk::InnermostExcludesKidsRowDropped
            ) {
                let mut little = taiyaku_library::scan::add_root_checked_in(
                    &conn,
                    &kids.path.join("Kids Book"),
                    taiyaku_library::scan::InLibrary::Named("Little"),
                )
                .expect("adds Little")
                .root;
                set(little.id, Some("Kids Book - v01/"));
                little.exclude_globs = Some("Kids Book - v01/".to_string());
                if case == KeptOnDisk::InnermostExcludesKidsRowDropped {
                    // Something Little reads, so its scan is not refused as an
                    // empty read (§8.1).
                    test_support::write_pages(
                        &kids.path.join("Kids Book").join("Kids Book - v02"),
                        2,
                    );
                    let scan_little = || {
                        let report =
                            taiyaku_library::scan::scan_root(&conn, &little).expect("scans Little");
                        assert!(report.refused.is_none(), "{case:?}: {report:?}");
                    };
                    scan_little();
                    set(n.m_kids, Some("Kids Book/Kids Book - v01/"));
                    scan_little();
                    set(n.m_kids, None);
                    taiyaku_library::derive::derive(&conn).expect("derives");
                }
            }
            let fixture: (Option<i64>, bool) = conn
                .query_row(
                    "SELECT m.root_id, w.missing_since IS NOT NULL
                       FROM member m JOIN edition e ON e.id = m.edition_id
                       JOIN work w ON w.id = e.work_id
                      WHERE m.path = ?1",
                    [&book],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .expect("Kids Book's member");
            assert_eq!(
                fixture,
                (
                    Some(n.m_kids),
                    matches!(
                        case,
                        KeptOnDisk::BothExclude | KeptOnDisk::InnermostExcludesKidsRowDropped
                    )
                ),
                "{case:?}: the fixture, Kids Book's member is Kids', and its work is missing \
                 only where Kids' row was dropped"
            );
        }

        for path in [
            format!("/api/library/roots/{}?dry_run=true", n.m_kids),
            format!("/api/library/libraries/{}?dry_run=true", n.kids),
        ] {
            let response = send_json(&n.admin, Method::DELETE, &path, &serde_json::json!({})).await;
            assert_eq!(response.status(), StatusCode::OK, "{case:?} {path}");
            let body: serde_json::Value =
                serde_json::from_str(&test_support::body_string(response).await).expect("JSON");
            let removed = &body["removed"];
            assert_eq!(
                (
                    removed["works"].as_u64(),
                    removed["positions"].as_u64(),
                    removed["notes"].as_u64()
                ),
                (Some(0), Some(0), Some(0)),
                "{case:?} {path}: {body}"
            );
        }
    }
}

/// The owner's ruling of 2026-11-11, "Narrow the grant" (O91): removing Kids'
/// directory hands Kids Book, with no row, to Manga's, whose `kids/` keeps
/// Manga's walk out of it. Manga's reader is answered on its page, card and
/// sidecar as for a slug no work holds, before the removal and after it; an
/// administrator still reads it, and once Manga lifts the pattern so does
/// Manga's reader. With no pattern, Manga's reader reads it throughout.
#[tokio::test]
async fn a_removal_hands_no_work_into_a_grant_whose_walk_misses_its_file() {
    for excluded in [true, false] {
        let n = test_support::router_over_a_library_nested_in_another();
        let set = |globs: Option<&str>| {
            n.state
                .library
                .conn()
                .execute(
                    "UPDATE root SET exclude_globs = ?1 WHERE id = ?2",
                    rusqlite::params![globs, n.m],
                )
                .expect("sets Manga's patterns");
        };
        let (slug, key): (String, String) = n
            .state
            .library
            .conn()
            .query_row(
                "SELECT w.slug, e.key FROM work w JOIN edition e ON e.work_id = w.id
                   JOIN member m ON m.edition_id = e.id
                  WHERE m.root_id = ?1",
                [n.m_kids],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("Kids Book's work");
        if excluded {
            set(Some("kids/"));
        }
        let answers = |router: axum::Router, slug: String| {
            let key = key.clone();
            async move {
                let mut out = Vec::new();
                for uri in [
                    format!("/api/works/{slug}/{key}/pages/0"),
                    format!("/api/library/works/{slug}"),
                    format!("/api/works/{slug}"),
                ] {
                    let response = test_support::get(&router, &uri).await;
                    let status = response.status();
                    let body = test_support::body_string(response)
                        .await
                        .replace(&slug, "{slug}");
                    out.push((
                        status,
                        if status == StatusCode::OK {
                            None
                        } else {
                            Some(body)
                        },
                    ));
                }
                out
            }
        };
        let nowhere = answers(n.manga_reader.clone(), "no-such-work".to_owned()).await;
        let read = vec![(StatusCode::OK, None); 3];
        let before = answers(n.manga_reader.clone(), slug.clone()).await;

        let response = send_json(
            &n.admin,
            Method::DELETE,
            &format!("/api/library/roots/{}", n.m_kids),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "excluded={excluded}");
        let held: (Option<i64>, bool) = n
            .state
            .library
            .conn()
            .query_row(
                "SELECT m.root_id, w.missing_since IS NOT NULL
                   FROM work w JOIN edition e ON e.work_id = w.id
                   JOIN member m ON m.edition_id = e.id
                  WHERE w.slug = ?1",
                [&slug],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("Kids Book's member");
        assert_eq!(
            held,
            (Some(n.m), excluded),
            "the fixture: Manga holds Kids Book, missing only where its walk misses it"
        );
        let after = answers(n.manga_reader.clone(), slug.clone()).await;
        let admin = answers(n.admin.clone(), slug.clone()).await;
        set(None);
        let lifted = answers(n.manga_reader.clone(), slug.clone()).await;

        let expected = if excluded { &nowhere } else { &read };
        assert_eq!(
            (&before, &after),
            (expected, expected),
            "excluded={excluded}: Manga's reader before and after the removal, \
             [page, card, sidecar]"
        );
        assert_eq!(
            (&admin[..2], &lifted),
            (&read[..2], &read),
            "excluded={excluded}: the administrator's page and card, and Manga's \
             reader once Manga's walk reaches the file"
        );
    }
}

/// One removal can empty two libraries: the second root's directory is the
/// only one of its library and of Kids, which links it. Removing it names both
/// libraries, by name and in id order, and every reader whose every grant was
/// among them: `only_b`, and `b_and_kids`, whom asking of each library in turn
/// would miss. `a_and_kids` keeps the first root's library, and `gone`,
/// limited with no grant at all, has nothing to lose.
#[tokio::test]
async fn removing_a_dir_that_empties_two_libraries_names_both_and_whoever_held_only_them() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let id = test_support::second_root_id(&state);
    let (path, own) = {
        let conn = state.library.conn();
        let root = taiyaku_library::scan::roots(&conn)
            .expect("lists")
            .into_iter()
            .find(|root| root.id == id)
            .expect("the second root");
        (root.path.to_string_lossy().into_owned(), root.libraries[0])
    };
    let response = send_json(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": "Kids", "dirs": [path] }),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Kids links the directory"
    );
    {
        let conn = state.library.conn();
        let named = |name: &str| {
            taiyaku_library::libraries::id_of(&conn, name)
                .expect("reads")
                .expect("exists")
        };
        let kids = named("Kids");
        let first = taiyaku_library::scan::roots(&conn).expect("lists")[0].libraries[0];
        for (reader, grants) in [
            ("only_b", vec![own]),
            ("b_and_kids", vec![own, kids]),
            ("a_and_kids", vec![first, kids]),
        ] {
            let account = taiyaku_library::account::add(&conn, reader, "pw", "user").expect("adds");
            for library in grants {
                taiyaku_library::account::grant_library(&conn, account, library).expect("grants");
            }
        }
        let gone = taiyaku_library::account::add(&conn, "gone", "pw", "user").expect("adds");
        conn.execute("UPDATE user SET every_library = 0 WHERE id = ?1", [gone])
            .expect("gone is limited and granted nothing");
    }
    let library_name = get_json(&router, "/api/library/libraries").await["libraries"]
        .as_array()
        .expect("libraries")
        .iter()
        .find(|library| library["id"] == own)
        .expect("the second root's library")["name"]
        .clone();

    let mut answers = Vec::new();
    for query in ["?dry_run=true", ""] {
        let response = send_json(
            &router,
            Method::DELETE,
            &format!("/api/library/roots/{id}{query}"),
            &serde_json::json!({}),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::OK, "`{query}`: {text}");
        let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");
        assert_eq!(
            body["removed"]["libraries_removed"],
            serde_json::json!([library_name, "Kids"]),
            "`{query}`: {text}"
        );
        assert_eq!(
            body["removed"]["left_with_no_library"],
            serde_json::json!(["b_and_kids", "only_b"]),
            "`{query}`: {text}"
        );
        assert_eq!(
            body["removed"]["kept"],
            serde_json::json!([]),
            "`{query}`: {text}"
        );
        answers.push(body["removed"].clone());
    }
    assert_eq!(
        answers[0], answers[1],
        "the removal does what the dry run said"
    );
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        listed["libraries"].as_array().expect("libraries").len(),
        1,
        "only the first root's library is left: {listed}"
    );
}

/// A directory scan's `?library=` must hold the directory: 404 in the
/// unlink's words, or naming the library that does not exist, as the unlink
/// and Move answer them; one that is not a number is 400. Each is
/// `{"error": …}`, and no slot is reserved.
#[tokio::test]
async fn a_dir_scan_for_a_library_that_does_not_hold_it_is_refused() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let library = |name: &str| {
        listed["libraries"]
            .as_array()
            .expect("libraries")
            .iter()
            .find(|library| library["name"] == name)
            .expect("the library")
            .clone()
    };
    let (a, b) = (library("root-a"), library("root-b"));
    let dir = a["dirs"][0]["id"].as_i64().expect("a directory id");
    let path = a["dirs"][0]["path"].as_str().expect("a path");

    for (query, answered, says) in [
        (
            format!("?library={}", b["id"]),
            StatusCode::NOT_FOUND,
            format!("`{path}` is not in root-b"),
        ),
        (
            "?library=4242".to_string(),
            StatusCode::NOT_FOUND,
            "there is no library 4242".to_string(),
        ),
        (
            "?library=one".to_string(),
            StatusCode::BAD_REQUEST,
            "the query could not be read: ".to_string(),
        ),
    ] {
        let response = send_json(
            &router,
            Method::POST,
            &format!("/api/library/roots/{dir}/scan{query}"),
            &serde_json::json!({}),
        )
        .await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(status, answered, "{query}: {text}");
        let refused: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|why| panic!("{query}: not JSON ({why}): {text}"));
        let keys: Vec<&String> = refused.as_object().expect("an object").keys().collect();
        assert_eq!(keys, ["error"], "{query}: {text}");
        let sentence = refused["error"].as_str().expect("a sentence");
        if answered == StatusCode::NOT_FOUND {
            assert_eq!(sentence, says, "{query}");
        } else {
            assert!(
                sentence.starts_with(&says) && sentence.contains("library"),
                "{query}: {sentence}"
            );
        }
        assert!(
            state.scans.lock().unwrap().get(&dir).is_none(),
            "{query} reserved a slot"
        );
    }
}

/// O51: a save refuses while another process derives, as it refuses while the
/// job lock is held, and lands once the derive ends.
#[tokio::test]
async fn a_save_refuses_while_another_derive_holds_the_lease() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    let existing = get_json(&router, "/api/library/rules").await["rules"].clone();
    let body = serde_json::json!({ "rules": existing });
    let elsewhere = test_support::the_store_elsewhere(&state);
    let held = test_support::holding_the_lease(&elsewhere);

    let response = send_json(&router, Method::PUT, "/api/library/rules", &body).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(
        (status, text.as_str()),
        (
            StatusCode::CONFLICT,
            "a scan or another library job is running; save again once it has finished"
        )
    );

    drop(held);
    let response = send_json(&router, Method::PUT, "/api/library/rules", &body).await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// O51: a preview derives the whole library, so it refuses while another
/// derive runs, and previews once it ends.
#[tokio::test]
async fn a_preview_refuses_while_another_derive_holds_the_lease() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    let existing = get_json(&router, "/api/library/rules").await["rules"].clone();
    let body = serde_json::json!({ "rules": existing });
    let elsewhere = test_support::the_store_elsewhere(&state);
    let held = test_support::holding_the_lease(&elsewhere);

    let response = send_json(&router, Method::POST, "/api/library/rules/preview", &body).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(
        (status, text.as_str()),
        (
            StatusCode::CONFLICT,
            "a scan or another library job is running; preview again once it has finished"
        )
    );

    drop(held);
    let response = send_json(&router, Method::POST, "/api/library/rules/preview", &body).await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// O51: a directory's removal derives, so even its dry run refuses while
/// another derive runs, and answers once it ends.
#[tokio::test]
async fn a_directory_removal_refuses_while_another_derive_holds_the_lease() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_cbz();
    let id = taiyaku_library::scan::roots(&state.library.conn()).expect("the directories")[0].id;
    let path = format!("/api/library/roots/{id}?dry_run=true");
    let elsewhere = test_support::the_store_elsewhere(&state);
    let held = test_support::holding_the_lease(&elsewhere);

    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(
        (
            status,
            serde_json::from_str::<serde_json::Value>(&text).ok()
        ),
        (
            StatusCode::CONFLICT,
            Some(serde_json::json!({
                "error": "a scan or another library job is running; remove the directory once it \
                          has finished"
            }))
        ),
        "{text}"
    );

    drop(held);
    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Spec §7, "Pause": a press refused because a scan already holds the
/// directory clears nothing, so its pause stands.
#[tokio::test]
async fn a_refused_scan_directory_leaves_the_pause() {
    const PAUSED: &str = "2026-11-01 00:00:00";
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);
    state
        .library
        .conn()
        .execute(
            "UPDATE root SET paused_at = ?2 WHERE id = ?1",
            rusqlite::params![root_id, PAUSED],
        )
        .expect("pauses it");
    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress {
            waiting: false,
            started: std::time::Instant::now(),
            done: false,
            report: None,
            library: None,
            job: crate::test_support::a_planted_job(),
        },
    );

    let response = crate::config_api::start_scan(
        axum::extract::State(std::sync::Arc::clone(&state)),
        axum::extract::Extension(an_administrator()),
        test_support::from_a_browser(),
        axum::extract::Path(root_id),
        Ok(axum::extract::Query(crate::config_api::ScanQuery::default())),
    )
    .await;

    let paused: Option<String> = state
        .library
        .conn()
        .query_row("SELECT paused_at FROM root WHERE id = ?1", [root_id], |r| {
            r.get(0)
        })
        .expect("reads its pause");
    assert_eq!(
        (response.status(), paused.as_deref()),
        (StatusCode::CONFLICT, Some(PAUSED))
    );
}

/// Spec §7, "Pause": each directory says when it was paused, and a directory
/// not paused sends `null`, not nothing.
#[tokio::test]
async fn a_paused_directory_lists_when_it_was_paused() {
    const PAUSED: &str = "2026-11-16 03:33:10";
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let (first, second): (i64, i64) = state
        .library
        .conn()
        .query_row("SELECT MIN(id), MAX(id) FROM root", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("reads the two roots");
    state
        .library
        .conn()
        .execute(
            "UPDATE root SET paused_at = ?2 WHERE id = ?1",
            rusqlite::params![first, PAUSED],
        )
        .expect("pauses the first");

    let json = get_json(&router, "/api/library/roots").await;

    let paused_at = |id: i64| -> Option<serde_json::Value> {
        json["roots"]
            .as_array()
            .expect("a roots array")
            .iter()
            .find(|root| root["id"] == id)
            .expect("the root is listed")
            .get("paused_at")
            .cloned()
    };
    assert_eq!(
        (paused_at(first), paused_at(second)),
        (
            Some(serde_json::json!(PAUSED)),
            Some(serde_json::Value::Null)
        ),
        "{json}"
    );
}

/// Asks its job to stop at the first count of hashing after one digest.
struct CancelAtHashing<'a> {
    inner: &'a dyn taiyaku_library::progress::Watch,
    job: &'a crate::scan_jobs::Job,
}

impl taiyaku_library::progress::Watch for CancelAtHashing<'_> {
    fn at(&self, step: taiyaku_library::progress::Step, done: u64, of: Option<u64>) {
        self.inner.at(step, done, of);
        if step == taiyaku_library::progress::Step::Hashing && done > 0 {
            self.job.request_cancel();
        }
    }

    fn cancelled(&self) -> bool {
        self.inner.cancelled()
    }
}

/// Spec §7, "Cancel checks", on the wire: a directory never scanned whose
/// job was cancelled while it hashed answers that its scan was cancelled and
/// in which step, and still reads Never.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_directorys_status_says_cancelled_and_at_which_step() {
    let (router, state, fixture) = test_support::router_over_a_live_library();
    let fresh = fixture.path().join("fresh");
    // Two volumes, so a digest is still to come when the first is counted.
    for volume in ["Fresh - v01", "Fresh - v02"] {
        test_support::write_pages(&fresh.join("Fresh").join(volume), 2);
    }
    let root = taiyaku_library::scan::add_root(&state.library.conn(), &fresh)
        .expect("adds a directory never scanned");
    let started = std::time::Instant::now();
    let job = crate::scan_jobs::reserve(&state, std::slice::from_ref(&root), started, None)
        .expect("reserves");
    let asked = std::sync::Arc::clone(&job);
    crate::scan_jobs::spawn_job(
        std::sync::Arc::clone(&state),
        vec![root.clone()],
        started,
        move |state, roots, running| {
            crate::scan_jobs::run_watched(
                &state.library,
                roots,
                crate::scan_jobs::Reread::Dir,
                &state.library_job,
                running,
                taiyaku_library::scan::scan_root_watched,
                |conn, watch| {
                    taiyaku_library::derive::derive_watched(
                        conn,
                        taiyaku_library::derive::Documents::Reconcile,
                        &CancelAtHashing {
                            inner: watch,
                            job: &asked,
                        },
                    )
                },
            )
        },
    )
    .await
    .expect("the job ends");

    let json = get_json(&router, &format!("/api/library/roots/{}/scan", root.id)).await;

    assert_eq!(
        json["report"],
        serde_json::json!({ "cancelled": { "step": "hashing" } }),
        "{json}"
    );
    assert_eq!(json["last_scan_status"], "cancelled", "{json}");
    assert_eq!(
        json.get("last_scan_at"),
        Some(&serde_json::Value::Null),
        "Never: {json}"
    );
    assert_eq!(json["running"], false, "{json}");
    assert_eq!(
        job.ended(),
        Some(crate::scan_jobs::JobEnded::Cancelled(
            taiyaku_library::progress::Step::Hashing
        )),
        "the fixture: cancelled while it hashed"
    );
}
