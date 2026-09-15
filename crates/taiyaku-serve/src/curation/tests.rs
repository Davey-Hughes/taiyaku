use crate::test_support::{self, get, get_json, send_json};
use axum::http::{Method, StatusCode};

/// Raise this library's proposals and hand back the id of the highest-scoring
/// open one.
///
/// Every fixture these tests use raises exactly one, which the assertion here
/// pins: a fixture that quietly started raising two would otherwise make every
/// test below silently about whichever came first.
fn the_one_open_proposal(state: &crate::ServeState) -> i64 {
    let conn = state.library.conn();
    taiyaku_library::proposals::raise(&conn).expect("raise");
    let open = taiyaku_library::proposals::open(&conn, 0.0).expect("open");
    assert_eq!(open.len(), 1, "the fixture raises exactly one: {open:?}");
    open[0].id
}

/// How many rules an accept has written.
///
/// Keyed on the note rather than on a row count, because `003_scanner.sql`
/// seeds the rules that decide a bare series name in the first place and
/// those are in the table before any accept — the same way
/// `proposals/tests.rs` counts them.
fn accepted_rules(state: &crate::ServeState) -> i64 {
    let conn = state.library.conn();
    conn.query_row(
        "SELECT count(*) FROM rule WHERE note LIKE 'accepted%'",
        [],
        |r| r.get(0),
    )
    .expect("counts the accepted rules")
}

fn proposal_state(state: &crate::ServeState, id: i64) -> String {
    let conn = state.library.conn();
    conn.query_row("SELECT state FROM proposal WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .expect("reads the proposal's state")
}

/// A second proposal beside the fixture's own: a different kind, a score
/// below it, and `resting_in` as its state. Hands back the fixture
/// proposal's own score, which is the threshold that separates the two.
///
/// **Written as a row rather than raised.** `raise` opens exactly one
/// proposal over this tree — which [`the_one_open_proposal`] pins, and which
/// every test here relies on — while a question about a threshold or about
/// which state a count follows can only be answered by two rows that differ
/// in the column being asked about. A fixture whose two rows agreed on score
/// and kind would answer the same map whichever way the handler read it.
fn a_weaker_proposal_beside(state: &crate::ServeState, id: i64, resting_in: &str) -> f64 {
    let conn = state.library.conn();
    let (subject_id, target_id, score): (i64, Option<i64>, f64) = conn
        .query_row(
            "SELECT subject_id, target_id, score FROM proposal WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the fixture's own proposal");
    assert!(
        score >= 1.0,
        "the fixture's proposal has to outscore the row written below it, \
         and scores {score}"
    );
    conn.execute(
        "INSERT INTO proposal (kind, subject_id, target_id, score, reason, state)
         VALUES ('same-series', ?1, ?2, 0.0, 'a fixture row, scored below the raised one', ?3)",
        rusqlite::params![subject_id, target_id, resting_in],
    )
    .expect("writes the weaker proposal");
    score
}

/// The rule the server itself previewed for `id`, as the client is shown it.
async fn previewed_rule(router: &axum::Router, id: i64) -> serde_json::Value {
    let detail = get_json(router, &format!("/api/library/proposals/{id}")).await;
    let rule = detail["rule"].clone();
    assert!(
        rule.is_object(),
        "the fixture's proposal must be one an accept is offered for: {detail}"
    );
    rule
}

#[tokio::test]
async fn the_inbox_lists_proposals_and_their_counts() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);

    let body = get_json(&router, "/api/library/proposals?state=open&min_score=0").await;

    // **The row itself, not that an array arrived.** Over a library with no
    // `scan_entry` rows this route answers `{"proposals":[],"counts":{}}` and
    // an `is_array()`/`is_object()` pair passes over both — which is a test
    // that cannot tell a working handler from one that returns two empty
    // containers.
    let listed = body["proposals"].as_array().expect("proposals");
    assert_eq!(listed.len(), 1, "{body}");
    assert_eq!(listed[0]["id"], id);
    assert_eq!(listed[0]["kind"], "edition-of");
    assert_eq!(listed[0]["state"], "open");
    assert_eq!(
        listed[0]["created_rule_id"],
        serde_json::Value::Null,
        "nothing is accepted yet: {body}"
    );
    assert!(
        listed[0]["score"].as_f64().is_some_and(|score| score > 0.0),
        "the score crosses the wire as a number: {body}"
    );
    assert!(
        listed[0]["target_id"].is_i64(),
        "an `edition-of` proposal names the series it is about: {body}"
    );

    // **The two titles, which are what makes a collapsed row identifiable.**
    // C3 §3's inbox is a list rather than a queue so a person can see that 29
    // of 37 rows ask the same question; a row carrying ids and a truncated
    // sentence cannot be told from the row above it. The store joins for
    // these (`proposals::list`) and this route is the only one that sends
    // them — the detail route has both titles inside `subject`/`target`
    // already.
    //
    // The exact strings, and both of them: the subject is the *suffixed*
    // series and the target is the base one, so a handler reading the same
    // title into both fields — or reading them the wrong way round — is
    // visible here and nowhere else.
    assert_eq!(
        listed[0]["subject_title"], "Go-Toubun no Hanayome - Full Color Edition",
        "{body}"
    );
    assert_eq!(listed[0]["target_title"], "Go-Toubun no Hanayome", "{body}");
    assert!(
        listed[0]["reason"]
            .as_str()
            .is_some_and(|why| !why.is_empty()),
        "the reason is what makes a proposal readable rather than a coin \
         flip with a score attached: {body}"
    );
    assert_eq!(body["counts"]["edition-of"], 1, "{body}");

    // Each of the three query parameters narrows, which is what stops this
    // being a handler that answers one fixed list whatever it was asked.
    for narrowed in [
        "/api/library/proposals?state=open&min_score=9",
        "/api/library/proposals?state=dismissed&min_score=0",
        "/api/library/proposals?state=open&kind=same-series&min_score=0",
    ] {
        let body = get_json(&router, narrowed).await;
        assert_eq!(
            body["proposals"].as_array().map(Vec::len),
            Some(0),
            "{narrowed}: {body}"
        );
    }

    // **And two of the three narrow the counts with them, while `kind` does
    // not.** That split is a decision — the tabs are what a kind is chosen
    // from, so a tab counted with its own filter applied would read 0 the
    // moment somebody selected it, and the standard faceted shape computes
    // each facet with its own filter removed — and until this assertion it
    // was a decision nothing held: the loop above reads `proposals` and never
    // `counts`, so `counts.retain(|k, _| k == kind)` passed the whole suite.
    let by_kind = get_json(
        &router,
        "/api/library/proposals?state=open&kind=same-series&min_score=0",
    )
    .await;
    assert_eq!(
        by_kind["counts"]["edition-of"], 1,
        "the `edition-of` tab still counts its own proposal while the \
         `same-series` tab is the one being read: {by_kind}"
    );

    // `min_score` is the parameter that has to agree with the list, and did
    // not: this same request answered `{"counts":{"edition-of":1},
    // "proposals":[]}` — a tab reading "1" over an empty list — until
    // `counts_by_kind` took a threshold of its own.
    let above_everything = get_json(&router, "/api/library/proposals?state=open&min_score=9").await;
    assert_eq!(
        above_everything["counts"]
            .as_object()
            .map(serde_json::Map::len),
        Some(0),
        "nothing is above a score of 9, so no tab may claim anything is: \
         {above_everything}"
    );
}

/// `target_title` is `null` exactly where `target_id` is, and the row is
/// still listed.
///
/// The `attach-chapters` shape, which only a chapter-organised tree raises
/// and which no fixture here builds — so the row is written directly, for the
/// reason [`a_weaker_proposal_beside`] is written rather than raised: the
/// question is what the store's join does with a NULL target, and the wrong
/// answer is not a wrong title but a row that vanishes from the inbox.
#[tokio::test]
async fn a_proposal_naming_one_series_is_listed_with_a_null_target_title() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    {
        let conn = state.library.conn();
        let subject_id: i64 = conn
            .query_row("SELECT subject_id FROM proposal WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .expect("the fixture's own proposal");
        conn.execute(
            "INSERT INTO proposal (kind, subject_id, target_id, score, reason, state)
             VALUES ('attach-chapters', ?1, NULL, 3.0, 'organised by chapter', 'open')",
            [subject_id],
        )
        .expect("writes the targetless proposal");
    }

    let body = get_json(&router, "/api/library/proposals?state=open&min_score=0").await;

    let listed = body["proposals"].as_array().expect("proposals");
    let chapters = listed
        .iter()
        .find(|row| row["kind"] == "attach-chapters")
        .unwrap_or_else(|| panic!("the targetless proposal is still listed: {body}"));
    assert_eq!(
        chapters["target_title"],
        serde_json::Value::Null,
        "there is no second series for the row to name: {body}"
    );
    assert_eq!(
        chapters["subject_title"], "Go-Toubun no Hanayome - Full Color Edition",
        "while the one series it does name is still there: {body}"
    );
}

/// The threshold narrows the counts and the list **together**, over a fixture
/// where it genuinely excludes something.
///
/// The assertion above it is one-sided by construction — at a threshold above
/// everything, a `counts_by_kind` that ignored the threshold *and* one that
/// answered an empty map for every request agree. The scale is `{0, 1, 2, 3}`
/// (`proposals::Filter::min_score`), so a fixture with two proposals at
/// different scores is the only thing that can tell a count that follows the
/// threshold from one that ignores it, and this asserts both sides of it: at
/// no threshold both kinds are counted, at the raised one only the kind above
/// it is.
#[tokio::test]
async fn the_threshold_narrows_the_counts_and_the_list_together() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let raised_score = a_weaker_proposal_beside(&state, id, "open");

    let everything = get_json(&router, "/api/library/proposals?state=open&min_score=0").await;
    assert_eq!(
        everything["proposals"].as_array().map(Vec::len),
        Some(2),
        "{everything}"
    );
    assert_eq!(everything["counts"]["edition-of"], 1, "{everything}");
    assert_eq!(
        everything["counts"]["same-series"], 1,
        "both kinds are counted below the threshold: {everything}"
    );

    let above = get_json(
        &router,
        &format!("/api/library/proposals?state=open&min_score={raised_score}"),
    )
    .await;
    let listed = above["proposals"].as_array().expect("proposals");
    assert_eq!(listed.len(), 1, "{above}");
    assert_eq!(
        listed[0]["id"], id,
        "the weaker proposal is below it: {above}"
    );
    assert_eq!(above["counts"]["edition-of"], 1, "{above}");
    assert_eq!(
        above["counts"].get("same-series"),
        None,
        "and the tab over the list agrees with the list: {above}"
    );
}

/// The inbox's **first** load names no state, and counts the open tabs.
///
/// `proposals::list` reads an absent `state` as every state and
/// `counts_by_kind` takes one state, so the two need an answer here — which
/// is `DEFAULT_COUNTED_STATE`, and which nothing pinned: changing it to
/// `"dismissed"` left the whole suite green, and the inbox's first load would
/// have counted the wrong state's tabs with nothing saying so. The fixture
/// carries one open proposal and one dismissed one **of different kinds**, so
/// each candidate default answers a different map rather than the same one.
#[tokio::test]
async fn the_counts_a_request_naming_no_state_carries_are_the_open_ones() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    a_weaker_proposal_beside(&state, id, "dismissed");

    let body = get_json(&router, "/api/library/proposals").await;

    assert_eq!(
        body["counts"]["edition-of"], 1,
        "the open proposal's kind is counted: {body}"
    );
    assert_eq!(
        body["counts"].get("same-series"),
        None,
        "and the dismissed one's is not — this is the open tab set, not \
         every state's and not the dismissed one's: {body}"
    );

    // The list beside them is *not* narrowed the same way, which is the half
    // that makes the default a decision rather than an oversight: `list`
    // reads an absent `state` as every state, so the two rows are both here
    // under counts that describe only one of them.
    assert_eq!(
        body["proposals"].as_array().map(Vec::len),
        Some(2),
        "an absent `state` lists every state: {body}"
    );
}

#[tokio::test]
async fn a_reader_is_refused_the_inbox() {
    // C3 §7: everything here is admin-gated per A2. A 403 rather than a 404 —
    // the caller is who they say they are, and there is nothing to
    // re-authenticate.
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_as_a_reader();

    // **The reader is genuinely signed in**, which the refusal below cannot
    // say on its own: a fixture whose credential simply did not work would
    // satisfy that assertion with the admin gate deleted outright. A browse
    // route answering 200 through the very same router is what rules that
    // out, and it is the same router — not a second one built differently.
    let browsing = get(&router, "/api/library/status").await;
    assert_eq!(
        browsing.status(),
        StatusCode::OK,
        "the reader fixture must authenticate; a 401 here would make the \
         403 below meaningless"
    );

    let response = get(&router, "/api/library/proposals").await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// The gate is on every route here, not only on the one the test above drives.
///
/// Enumerated for the reason `routes::guarded_routes`'s own doc comment gives
/// for enumerating the guard: a reviewer's checklist goes stale the first time
/// someone adds a route and does not think about it. Every row here but the
/// first would otherwise have shipped ungated with a green suite.
#[tokio::test]
async fn every_curation_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_as_a_reader();
    let body = serde_json::json!({});

    for (method, path) in [
        (Method::GET, "/api/library/proposals"),
        (Method::GET, "/api/library/proposals/1"),
        (Method::POST, "/api/library/proposals/1/resolution"),
        (Method::POST, "/api/library/proposals/1/accept"),
        (Method::DELETE, "/api/library/proposals/1/accept"),
        (Method::POST, "/api/library/proposals/1/dismiss"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}

#[tokio::test]
async fn accept_writes_the_rule_in_the_body() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);

    let mut rule = previewed_rule(&router, id).await;
    // **A key the server would never derive**, pinned against the one it
    // does: `preview` slugifies the suffix that made this an `edition-of`
    // proposal. Asserting only that the two differ would leave this test
    // passing over a route that re-derived the rule at write time and
    // happened to land on something else again — this says which value a
    // re-derivation would produce, so the row read back below can only carry
    // one of the two.
    assert_eq!(
        rule["set_edition_key"], "full-color-edition",
        "the rule the server previewed: {rule}"
    );
    rule["set_edition_key"] = serde_json::json!("en-color");

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let answered: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert!(answered["rule_id"].is_i64(), "{body}");

    let conn = state.library.conn();
    let key: Option<String> = conn
        .query_row(
            "SELECT set_edition_key FROM rule ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("the written rule");
    assert_eq!(
        key.as_deref(),
        Some("en-color"),
        "the server wrote what it was shown"
    );
}

#[tokio::test]
async fn a_rule_that_would_move_an_unrelated_series_is_refused_with_its_name() {
    let (router, state, _fixture) = test_support::router_over_a_shared_publisher_directory();
    let id = the_one_open_proposal(&state);
    let mut rule = previewed_rule(&router, id).await;
    rule["pattern"] = serde_json::json!("Publisher/");

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = test_support::body_string(response).await;
    assert!(
        body.contains("Kaguya-sama"),
        "names what it would move: {body}"
    );
    // A refused accept leaves nothing behind — the same property
    // `proposals/tests.rs` pins at the store, asserted again here because a
    // route that answered 409 *after* the write would look identical to a
    // caller.
    assert_eq!(accepted_rules(&state), 0, "a refused accept writes no rule");
    assert_eq!(proposal_state(&state, id), "open");
}

/// The dry run behind the form: the same route, the same fixture, two rules,
/// two different answers.
///
/// Two-sided deliberately. Asserting only that a widened pattern reports
/// collateral would pass over a handler that reported collateral for
/// everything; asserting only that the narrow rule reports none would pass
/// over one that never reported any.
#[tokio::test]
async fn the_dry_run_names_the_collateral_a_widened_pattern_would_take() {
    let (router, state, _fixture) = test_support::router_over_a_shared_publisher_directory();
    let id = the_one_open_proposal(&state);
    let narrow = previewed_rule(&router, id).await;
    let path = format!("/api/library/proposals/{id}/resolution");

    let response = send_json(&router, Method::POST, &path, &narrow).await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let clean: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert_eq!(
        clean["collateral"].as_array().map(Vec::len),
        Some(0),
        "the rule the server itself previewed reaches nobody else: {clean}"
    );
    assert_eq!(clean["subject_reached"], true, "{clean}");
    assert!(
        clean["works_moved"].as_u64().is_some_and(|n| n > 0),
        "and it moves the subject's own material: {clean}"
    );
    assert!(clean["also_resolves"].is_array(), "{clean}");
    assert_eq!(
        clean["targets_reached"].as_array().map(Vec::len),
        Some(0),
        "{clean}"
    );
    assert!(clean["entries_matched"].as_u64().is_some(), "{clean}");

    let mut wide = narrow.clone();
    wide["pattern"] = serde_json::json!("Publisher/");
    let response = send_json(&router, Method::POST, &path, &wide).await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let widened: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    let collateral = widened["collateral"].as_array().expect("collateral");
    assert!(
        collateral
            .iter()
            .any(|title| title.as_str() == Some("Kaguya-sama wa Kokurasetai")),
        "the widened pattern names whose material it would take: {widened}"
    );

    // Nothing was written by either call: a dry run that accepted would be
    // indistinguishable from one that did not, to a caller reading only the
    // body.
    assert_eq!(accepted_rules(&state), 0);
    assert_eq!(proposal_state(&state, id), "open");
}

/// A rule the engine cannot evaluate is the caller's own text, so it is a 400
/// rather than the 409 a refusal about the library's state gets.
#[tokio::test]
async fn a_rule_that_cannot_be_evaluated_is_a_400_rather_than_a_conflict() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let mut rule = previewed_rule(&router, id).await;
    rule["kind"] = serde_json::json!("regex");
    rule["pattern"] = serde_json::json!("(unclosed");

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/resolution"),
        &rule,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // And a body that is not a rule at all reaches the same answer without
    // touching the store.
    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/resolution"),
        &serde_json::json!({ "kind": "prefix" }),
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.contains("pattern"),
        "names the field it wanted: {body}"
    );
}

#[tokio::test]
async fn deleting_an_accept_removes_the_rule_and_reopens_the_proposal() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let accept = format!("/api/library/proposals/{id}/accept");
    let nothing = serde_json::json!({});

    // Nothing to undo yet, and that is a refusal rather than a silent 204 —
    // one that says why, since a 409 with an empty body is a status to
    // render and nothing to act on.
    let response = send_json(&router, Method::DELETE, &accept, &nothing).await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("no accepted rule"), "{body}");

    let rule = previewed_rule(&router, id).await;
    let response = send_json(&router, Method::POST, &accept, &rule).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(accepted_rules(&state), 1);
    assert_eq!(proposal_state(&state, id), "accepted");

    let response = send_json(&router, Method::DELETE, &accept, &nothing).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    assert_eq!(
        accepted_rules(&state),
        0,
        "the rule the accept wrote is gone"
    );
    assert_eq!(
        proposal_state(&state, id),
        "open",
        "and the question is back in the inbox"
    );
}

#[tokio::test]
async fn dismissing_takes_a_proposal_out_of_the_open_list() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);

    let before = get_json(&router, "/api/library/proposals?state=open&min_score=0").await;
    assert_eq!(before["proposals"].as_array().map(Vec::len), Some(1));

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/dismiss"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let open = get_json(&router, "/api/library/proposals?state=open&min_score=0").await;
    assert_eq!(
        open["proposals"].as_array().map(Vec::len),
        Some(0),
        "{open}"
    );
    assert_eq!(
        open["counts"].as_object().map(serde_json::Map::len),
        Some(0),
        "the open tab counts nothing either: {open}"
    );

    let dismissed = get_json(
        &router,
        "/api/library/proposals?state=dismissed&min_score=0",
    )
    .await;
    assert_eq!(
        dismissed["proposals"].as_array().map(Vec::len),
        Some(1),
        "{dismissed}"
    );
    assert_eq!(dismissed["counts"]["edition-of"], 1, "{dismissed}");
    assert_eq!(proposal_state(&state, id), "dismissed");
}

/// The expanded row hands over both sides as **data**, not as sentences.
///
/// `directories` and `formats` are lists because the store hands over facts
/// and the client decides how to render them — a side spanning two
/// directories once crossed the wire as `"{root} (2 directories)"`, prose the
/// client could only re-parse or print. A test that only checked the fields
/// existed would pass over a handler that joined them back up.
#[tokio::test]
async fn the_expanded_row_carries_both_sides_and_the_rule() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);

    let body = get_json(&router, &format!("/api/library/proposals/{id}")).await;

    assert_eq!(body["proposal"]["id"], id, "{body}");
    let subject = &body["subject"];
    assert_eq!(
        subject["title"],
        "Go-Toubun no Hanayome - Full Color Edition"
    );
    let directories = subject["directories"].as_array().expect("directories");
    assert_eq!(directories.len(), 1, "{subject}");
    assert!(
        directories[0]
            .as_str()
            .is_some_and(|d| d.ends_with("Go-Toubun no Hanayome - Full Color Edition")),
        "a path on disk, verbatim: {subject}"
    );
    assert_eq!(subject["volume_count"], 1, "{subject}");
    assert_eq!(subject["volumes"].as_array().map(Vec::len), Some(1));
    assert!(subject["root_name"].is_string(), "{subject}");
    let first = &subject["first_volume"];
    assert!(first["work_slug"].is_string(), "{first}");
    assert!(first["edition_key"].is_string(), "{first}");
    assert!(
        first["page_count"].as_i64().is_some_and(|n| n > 0),
        "{first}"
    );
    assert!(
        first["formats"]
            .as_array()
            .is_some_and(|f| f.iter().all(|token| token.as_str() == Some("dir"))),
        "every format is its own token: {first}"
    );
    assert!(first["interior"].is_array(), "{first}");

    assert_eq!(body["target"]["title"], "Go-Toubun no Hanayome", "{body}");
    let reasons = body["reasons"].as_array().expect("reasons");
    assert!(!reasons.is_empty(), "{body}");
    assert!(
        reasons
            .iter()
            .all(|r| r["text"].is_string() && r["sign"].is_string()),
        "each signal is text and a sign rather than one sentence: {body}"
    );
    assert_eq!(body["rule"]["match_on"], "path", "{body}");
    assert_eq!(
        body["rule"]["set_series"], "Go-Toubun no Hanayome",
        "{body}"
    );
}

/// A store error is a **500, not a panic**, on the two routes that read.
///
/// Error arms are written, compiled and otherwise never run — the same gap
/// `library_api::tests::a_store_error_is_a_500_rather_than_a_panic` was
/// written for. Driven by dropping the table the queries read, which is the
/// bluntest way to make a real `rusqlite` error arrive at the handler rather
/// than a mocked one.
///
/// **Which arm each route answers from, given that `proposal` is the table
/// `present` reads first.** `list` fails inside `proposals::list`, so the
/// counts beside it are never reached; `detail` fails inside `present`, so
/// `evidence`'s own arm is never reached either. Those two are covered by
/// `a_failure_in_the_counts_is_a_500_even_though_the_list_beside_it_answered`
/// and by
/// `a_store_fault_under_a_real_id_is_a_500_on_both_routes_that_read_beneath_it`,
/// each of which arranges a fault this fixture cannot produce.
#[tokio::test]
async fn a_store_error_is_a_500_rather_than_a_panic() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    {
        let conn = state.library.conn();
        conn.execute_batch("PRAGMA writable_schema = ON; DROP TABLE proposal;")
            .expect("drops the table both routes read");
    }

    for path in [
        "/api/library/proposals".to_string(),
        format!("/api/library/proposals/{id}"),
    ] {
        let response = get(&router, &path).await;
        let status = response.status();
        let body = test_support::body_string(response).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{path} should answer 500 once its table is gone: {body}"
        );
        assert!(
            body.contains("proposal"),
            "{path}: the message says what it was reading: {body}"
        );
    }
}

/// A store fault under an `id` that **does** exist is a 500, on the dry run
/// as much as on the expanded row.
///
/// Two arms the test above cannot reach, because the table it drops is the
/// one `present` reads first. Dropping `scan_entry` instead leaves the row
/// `present` looks up intact and breaks the sweep underneath it — a fault
/// `proposals::lookup` cannot see, so each route reaches the call its own
/// error arm was written for.
///
/// `GET /{id}` was the regression: with `present` in front of it, `detail`'s
/// 500 came from `present` and mutating `evidence`'s own `map_err` to
/// `NoAnswer::Refused` passed all 400 tests.
///
/// `POST /{id}/resolution` was worse than unpinned — it answered **400** for
/// this, measured verbatim as `reading scan entries for the dry run: no such
/// table: scan_entry`, because `proposals::resolves` folds a rule it cannot
/// evaluate together with a read that failed into one `Err`. On the route a
/// rule-editing form calls at every keystroke, that told a client to fix its
/// rule while the database was gone.
#[tokio::test]
async fn a_store_fault_under_a_real_id_is_a_500_on_both_routes_that_read_beneath_it() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    // Read while the library is whole: this is the fixture, not the
    // measurement.
    let rule = previewed_rule(&router, id).await;
    {
        let conn = state.library.conn();
        conn.execute_batch("PRAGMA writable_schema = ON; DROP TABLE scan_entry;")
            .expect("drops the table both routes sweep");
    }

    let response = get(&router, &format!("/api/library/proposals/{id}")).await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "GET /{id} reads the sweep after `present` has already answered: {body}"
    );
    assert!(
        body.contains("scan_entry"),
        "GET /{id}: and it is the sweep that failed, not the lookup: {body}"
    );

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/resolution"),
        &rule,
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "the dry run over a broken library is this server's failure, not the \
         caller's rule: {body}"
    );
    assert!(body.contains("scan_entry"), "POST /{id}/resolution: {body}");
}

/// A failure in the **counts** is a 500, with the list beside them answering.
///
/// `list` and `counts_by_kind` read the same table, so the store test above
/// fails the first of them and the second's `map_err` is never reached —
/// mutating it to `NoAnswer::Refused` passed all 400 tests. What lets one
/// fail while the other answers is the filter they do not share: `kind`
/// narrows the list and deliberately does not narrow the counts, for the
/// reason `proposals::counts_by_kind`'s own doc gives. So a row the counts
/// cannot decode, of a kind the request does not list, is read by the counts
/// and not by the list — and the assertion below reads which of the two the
/// message came from rather than trusting that.
///
/// **A blob in a `TEXT NOT NULL` column, which is a value that column can
/// really hold.** SQLite's affinity rewrites a number into text and leaves a
/// blob alone, so this is not a mocked error: `r.get::<_, String>` refuses
/// it, in `counts_by_kind`'s own `query_map`.
#[tokio::test]
async fn a_failure_in_the_counts_is_a_500_even_though_the_list_beside_it_answered() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    {
        let conn = state.library.conn();
        let subject_id: i64 = conn
            .query_row("SELECT subject_id FROM proposal WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .expect("the fixture's own proposal");
        conn.execute(
            "INSERT INTO proposal (kind, subject_id, target_id, score, reason, state)
             VALUES (X'00', ?1, NULL, 0.0, 'a row whose kind is not text', 'open')",
            rusqlite::params![subject_id],
        )
        .expect("writes a row the counts cannot decode");

        // The list half of the handler still answers over this fixture, which
        // is what makes the 500 below the counts' own and not the list's
        // failing first all over again.
        let listed = taiyaku_library::proposals::list(
            &conn,
            &taiyaku_library::proposals::Filter {
                state: Some("open".to_string()),
                kind: Some("edition-of".to_string()),
                min_score: 0.0,
            },
        )
        .expect("the list narrows past the undecodable row");
        assert_eq!(listed.len(), 1, "{listed:?}");
    }

    let response = get(
        &router,
        "/api/library/proposals?state=open&kind=edition-of&min_score=0",
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "a count that cannot be taken is this server's failure, not a \
         refusal a caller could act on: {body}"
    );
    assert!(
        body.contains("counting proposals"),
        "and it is the counts that failed rather than the list: {body}"
    );
}

/// Dismissing an **accepted** proposal is a 409, not a 500.
///
/// `proposals::dismiss` refuses one — "un-accept it first" — and that is a
/// statement about the state of the library which the caller can act on,
/// which is the whole of `NoAnswer::Refused`'s argument. Nothing drove it:
/// every other dismiss in this file dismisses an open row, so mutating the
/// arm to `NoAnswer::Failed` passed all 400 tests.
#[tokio::test]
async fn dismissing_an_accepted_proposal_is_refused_rather_than_reported_as_a_failure() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let rule = previewed_rule(&router, id).await;
    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(proposal_state(&state, id), "accepted");

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/dismiss"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("un-accept"),
        "and it says what to do about it: {body}"
    );
    assert_eq!(
        proposal_state(&state, id),
        "accepted",
        "a refused dismiss leaves the row where it was"
    );
}

/// A trigger that fails one statement a write route makes, the way a full
/// disk or a locked database fails it: SQLite's own error, raised from inside
/// the statement, with nothing about the library's state that a caller could
/// change. `on` is the trigger's event, `ON` clause and `WHEN` included.
fn a_store_fault_on(state: &crate::ServeState, on: &str) {
    let conn = state.library.conn();
    conn.execute_batch(&format!(
        "CREATE TRIGGER a_store_fault {on} \
         BEGIN SELECT RAISE(ABORT, 'a simulated store fault'); END;"
    ))
    .expect("installs the trigger");
}

/// A store fault **during an accept** is a 500, not the 409 a refusal gets.
///
/// `proposals::accept_with` answered its refusals and its store failures
/// through one `Result<_, String>`, and this route mapped that one type onto
/// `NoAnswer::Refused`: measured on `main` at `6a2e72d` with this test, the
/// `INSERT INTO rule` below failing answered **409** with the trigger's own
/// sentence, telling a client the library's state was in the way when the
/// store had failed. O70.
///
/// The rule is the one the server previewed, so every refusal an accept makes
/// is passed before the write is reached: what fails is the write itself.
#[tokio::test]
async fn a_store_fault_during_an_accept_is_a_500_rather_than_a_conflict() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let rule = previewed_rule(&router, id).await;
    a_store_fault_on(&state, "BEFORE INSERT ON rule");

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(
        body.contains("a simulated store fault"),
        "and it is the write that failed, not a refusal before it: {body}"
    );
    assert_eq!(accepted_rules(&state), 0, "a failed accept writes no rule");
    assert_eq!(proposal_state(&state, id), "open");
}

/// A **read** that fails during an accept is a 500 too.
///
/// The fault above is on the write; this one is in the dry run `accept_with`
/// makes before it — `resolves`' sweep of `scan_entry` — which reaches the
/// route through a different `?`. Measured on `main` at `6a2e72d` with this
/// test: **409**, quoting `reading scan entries for the dry run: no such
/// table: scan_entry`.
#[tokio::test]
async fn a_failed_read_during_an_accept_is_a_500_rather_than_a_conflict() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let rule = previewed_rule(&router, id).await;
    {
        let conn = state.library.conn();
        conn.execute_batch("PRAGMA writable_schema = ON; DROP TABLE scan_entry;")
            .expect("drops the table the dry run sweeps");
    }

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.contains("scan_entry"), "{body}");
    assert_eq!(accepted_rules(&state), 0);
    assert_eq!(proposal_state(&state, id), "open");
}

/// A store fault **during an un-accept** is a 500, not a 409.
///
/// Measured on `main` at `6a2e72d` with this test: **409**. The fault is on
/// the `DELETE FROM rule` that follows the reopening `UPDATE` inside one
/// transaction, so the state assertion below is also the rollback's.
#[tokio::test]
async fn a_store_fault_during_an_unaccept_is_a_500_rather_than_a_conflict() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let rule = previewed_rule(&router, id).await;
    let accept = format!("/api/library/proposals/{id}/accept");
    let response = send_json(&router, Method::POST, &accept, &rule).await;
    assert_eq!(response.status(), StatusCode::OK);
    a_store_fault_on(&state, "BEFORE DELETE ON rule");

    let response = send_json(&router, Method::DELETE, &accept, &serde_json::json!({})).await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.contains("a simulated store fault"), "{body}");
    assert_eq!(accepted_rules(&state), 1, "the rule is still there");
    assert_eq!(
        proposal_state(&state, id),
        "accepted",
        "and the reopen before the failed delete was rolled back with it"
    );
}

/// A store fault **during a dismiss** is a 500, not a 409.
///
/// Measured on `main` at `6a2e72d` with this test: **409**.
#[tokio::test]
async fn a_store_fault_during_a_dismiss_is_a_500_rather_than_a_conflict() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    a_store_fault_on(
        &state,
        "BEFORE UPDATE OF state ON proposal WHEN NEW.state = 'dismissed'",
    );

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/dismiss"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.contains("a simulated store fault"), "{body}");
    assert_eq!(proposal_state(&state, id), "open");
}

/// A rule the engine cannot evaluate, handed to the **accept** route, is still
/// a 409 naming why.
///
/// Not the 400 the dry run answers for the same rule: this route has never
/// asked `proposals::evaluable` first, and `accept_with` refuses such a rule
/// itself. What this pins is that the refusal stayed a refusal once
/// `accept_with`'s store failures became `Err` — the check lives in
/// `resolves`' sweep, whose `Err` is a failed read, so an `accept_with` that
/// left it there would answer 500 for a typo in a pattern.
#[tokio::test]
async fn a_rule_that_cannot_be_evaluated_is_still_refused_by_the_accept_route() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let mut rule = previewed_rule(&router, id).await;
    rule["kind"] = serde_json::json!("regex");
    rule["pattern"] = serde_json::json!("(unclosed");

    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("this pattern will not compile"), "{body}");
    assert_eq!(accepted_rules(&state), 0);
}

/// The failure `spawn_blocking` itself hands back is a **500 naming what was
/// running**.
///
/// Every arm of the `NoAnswer` mapping is driven through the routes; the
/// join error `spawn_blocking` itself hands back is not, and replacing
/// `panicked`'s body with `Response::default()` — a 200 with an empty body —
/// passed all 400 tests.
///
/// **The helper is called directly**, because these six closures offer no
/// lever to panic on purpose: every store call hands back a `Result`, and
/// `Library::conn` recovers a poisoned mutex rather than unwrapping it. The
/// `JoinError` is a real one from a real `spawn_blocking` that panicked, so
/// this is the value the handlers' `Err` arms actually receive rather than a
/// stand-in built to look like it.
#[tokio::test]
async fn a_closure_that_panicked_is_a_500_naming_what_was_running() {
    let why = tokio::task::spawn_blocking(|| {
        panic!("a curation closure, for the join error it produces");
    })
    .await
    .expect_err("a panicking closure joins as an error");

    let response = crate::curation::panicked("listing proposals", &why);
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(
        body.contains("listing proposals panicked"),
        "it names what was running, not just that something did: {body}"
    );
}

/// The rule crosses the wire **whole**, and `root_id` is the field that
/// matters most.
///
/// `ProposedRule::root_id`'s own doc is *"`NULL` means every root"*, so a
/// `rule_json` that dropped it would show — and then, handed back by a client
/// that displayed what it was shown, write — a rule scoped to every root in
/// the library instead of the one the proposal is about. Emitting `Null` for
/// it left the whole suite green: no test read the field, and the two
/// fixtures here hold one root each, so the wrong answer and the right one
/// decide the same material. `set_kind` and `set_language` were unpinned the
/// same way.
///
/// Both directions, against a rule whose fields are all populated: the
/// outbound half through the route, where `root_id` and `set_kind` carry
/// values the server derived and can be compared to the library's own rows,
/// and the inbound half through an accept whose body sets the third.
#[tokio::test]
async fn the_rule_carries_its_root_and_its_columns_in_both_directions() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let root_id: i64 = {
        let conn = state.library.conn();
        conn.query_row("SELECT id FROM root", [], |r| r.get(0))
            .expect("the fixture's one root")
    };

    let mut rule = previewed_rule(&router, id).await;

    assert_eq!(
        rule["root_id"].as_i64(),
        Some(root_id),
        "the previewed rule is scoped to the root the proposal's material \
         sits under, not to every root: {rule}"
    );
    assert_eq!(
        rule["set_kind"], "official",
        "\"Full Color Edition\" matches the vocabulary and implies a \
         publisher's edition: {rule}"
    );
    assert_eq!(
        rule["set_language"],
        serde_json::Value::Null,
        "and a language is never derived from a suffix: {rule}"
    );

    // The inbound half. `set_language` is the one column `preview` never
    // sets, so it is the one that can only be pinned by sending it.
    rule["set_language"] = serde_json::json!("ja");
    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let conn = state.library.conn();
    let written: (Option<i64>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT root_id, set_kind, set_language FROM rule ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the written rule");
    assert_eq!(
        written,
        (Some(root_id), Some("official".into()), Some("ja".into())),
        "the server wrote every column it was shown, `root_id` included"
    );
}

/// The same rule, through both halves of the pair at once.
///
/// The route test above cannot reach every field: `preview` writes `None` for
/// `set_language`, so a `rule_json` that emitted `Null` for it renders
/// identically to one that does not — the same shape as the `root_id` gap it
/// was written for. This is the pair's own contract, stated over a rule with
/// nothing left at a default: what [`rule_json`] writes, [`rule_from_json`]
/// reads back unchanged.
#[test]
fn a_rule_survives_the_pair_that_carries_it_with_every_field_set() {
    let sent = taiyaku_library::proposals::ProposedRule {
        root_id: Some(7),
        kind: "regex".to_string(),
        pattern: "^Publisher/(?<series>[^/]+)/".to_string(),
        match_on: "path".to_string(),
        set_series: Some("Go-Toubun no Hanayome".to_string()),
        set_edition_key: Some("full-color-edition".to_string()),
        set_kind: Some("official".to_string()),
        set_language: Some("ja".to_string()),
        note: "accepted from proposal 1".to_string(),
    };

    let back = crate::curation::rule_from_json(&crate::curation::rule_json(&sent))
        .expect("the pair reads its own output");

    assert_eq!(back, sent);
}

/// An `id` naming no proposal is one condition, and gets **one** status.
///
/// It got whichever status the store call that tripped over it was mapped to.
/// Measured with `present` removed from each handler: `GET .../9999` → 500,
/// `POST .../9999/resolution` → 400, and the write routes → 409 — and the dry
/// run did not even quote the same store sentence as the others (`looking up
/// proposal 9999: Query returned no rows` against `…: there is no such
/// proposal`). A 409 is a refusal about the state of the library, which a
/// caller can act on; an id that does not exist is not that, it is not a 400
/// about the caller's own rule either, and neither is it this server failing.
///
/// The two rule-carrying routes are driven with a **real** rule, so a 404
/// here is the proposal being absent rather than the body being unreadable —
/// which is the 400 the test above this one pins.
#[tokio::test]
async fn an_unknown_id_is_a_404_on_every_route_that_takes_one() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let rule = previewed_rule(&router, id).await;

    for (method, path) in [
        (Method::GET, "/api/library/proposals/9999"),
        (Method::POST, "/api/library/proposals/9999/resolution"),
        (Method::POST, "/api/library/proposals/9999/accept"),
        (Method::DELETE, "/api/library/proposals/9999/accept"),
        (Method::POST, "/api/library/proposals/9999/dismiss"),
    ] {
        let response = send_json(&router, method.clone(), path, &rule).await;
        let status = response.status();
        let body = test_support::body_string(response).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}: {body}");
        assert!(
            body.contains("9999"),
            "{method} {path} names the id: {body}"
        );
    }

    // And the id that does exist still answers, through the same routes: a
    // module that had simply started refusing everything would satisfy the
    // loop above.
    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/resolution"),
        &rule,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// An administrator, for the handlers the test below calls directly.
fn an_administrator() -> crate::auth::Principal {
    crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::Admin,
        can_download: true,
        can_stream: true,
        // The schema's default. These fixtures drive routes that predate the
        // capability, and a `true` here would mean an administrator inherits
        // it after all — the thing `auth::tests::an_administrator_does_not
        // _inherit_can_stamp` exists to refuse.
        can_stamp: false,
        roots: Vec::new(),
    }
}

/// **No curation route does its SQLite work on the async worker.**
///
/// Every handler here held `state.library.conn()` — a `MutexGuard` — across
/// the whole of its work, and two of them sweep every `scan_entry` row in the
/// library: `POST /{id}/resolution`, which C3's rule-editing form calls on
/// every keystroke, and `GET /{id}`. Measured in release over synthetic
/// entries, the runtime was stalled for **exactly as long as the handler
/// ran** — hundreds of milliseconds at a quarter-million rows, which is one
/// row per file *and* per directory in an ordinary few-thousand-volume
/// library. That equality is the finding this test is built on, and it is the
/// part that reproduces.
///
/// **The milliseconds are deliberately not quoted here, and `docs/notes.md`
/// says why.** This doc used to carry a pair per row count, and a second
/// record of the same quantity in `notes.md` disagreed with it two-fold. The
/// re-measurement that settled it ("What a swept row actually costs",
/// 2026-08-26) found both honest and neither general: at a fixed 250,005 rows
/// the same two routes move by 2.4× with nothing varying but the *length* of
/// `scan_entry.rel_path`, so the cost is per byte of path swept and a figure
/// quoted against a row count is a figure about whichever synthetic paths
/// that harness generated. Any number here would be the same claim again.
/// That is O8/O12's failure class, which `routes.rs` answers with
/// `spawn_blocking` in every handler but `get_sidecar`, which answers from
/// memory and, since O115, asks a grant that names roots on the blocking
/// pool — and which
/// `auth::guard` answers for the middleware every request already passes
/// through.
///
/// **All six, not just the two that sweep.** The two heavy ones are what the
/// measurement is about, but a rule that holds for four of six routes in a
/// module is a rule the next route added will be written against wrongly —
/// which is the same argument `every_curation_route_refuses_a_reader` makes
/// about the admin gate, and it is the reason this is a loop over the module
/// rather than a note in a doc comment.
///
/// Ordered so the three writes are each driven in a state that lets them
/// succeed: an accept, then its undo, then a dismiss of the reopened row.
#[tokio::test(flavor = "current_thread")]
async fn no_curation_route_stalls_the_runtime() {
    use axum::extract::{Extension, Path as AxPath, Query, State};
    use std::sync::Arc;

    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    // Read before any watchdog holds the mutex: this is the fixture, not the
    // measurement.
    let rule: taiyaku_library::proposals::ProposedRule =
        crate::curation::rule_from_json(&previewed_rule(&router, id).await).expect("a rule");
    let library = Arc::clone(&state.library);

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::list(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            Query(crate::curation::InboxQuery {
                state: Some("open".to_string()),
                kind: None,
                min_score: 0.0,
            }),
        ),
    )
    .await;
    assert!(
        !stalled,
        "GET /api/library/proposals ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::detail(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(id),
        ),
    )
    .await;
    assert!(
        !stalled,
        "GET /api/library/proposals/{{id}} ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::resolution(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(id),
            axum::Json(crate::curation::rule_json(&rule)),
        ),
    )
    .await;
    assert!(
        !stalled,
        "POST /api/library/proposals/{{id}}/resolution ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::accept(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(id),
            axum::Json(crate::curation::rule_json(&rule)),
        ),
    )
    .await;
    assert!(
        !stalled,
        "POST /api/library/proposals/{{id}}/accept ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(accepted_rules(&state), 1, "and the accept did happen");

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::unaccept(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(id),
        ),
    )
    .await;
    assert!(
        !stalled,
        "DELETE /api/library/proposals/{{id}}/accept ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::dismiss(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(id),
        ),
    )
    .await;
    assert!(
        !stalled,
        "POST /api/library/proposals/{{id}}/dismiss ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        proposal_state(&state, id),
        "dismissed",
        "and each write landed, rather than being refused into a status \
         this loop happens not to read"
    );
}
