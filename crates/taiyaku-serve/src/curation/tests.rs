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
    // C3 §1's inbox is a list rather than a queue so a person can see that
    // many rows ask the same question; a row carrying ids and a truncated
    // sentence cannot be told from the row above it. The store joins for these
    // (`proposals::list`) and this route is the only one that sends them — the
    // detail route has both titles inside `subject`/`target` already.
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
    // each facet with its own filter removed — and nothing in the loop above
    // holds it: that loop reads `proposals` and never `counts`.
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

    // `min_score` is the parameter that has to agree with the list: a count
    // that ignored it would leave a tab reading "1" over an empty list.
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
/// is `DEFAULT_COUNTED_STATE`, and any other default would count the wrong
/// state's tabs on the inbox's first load. The fixture carries one open
/// proposal and one dismissed one **of different kinds**, so each candidate
/// default answers a different map rather than the same one.
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
/// someone adds a route and does not think about it.
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
        (Method::DELETE, "/api/library/proposals/1/dismiss"),
        (Method::POST, "/api/library/matches/1/split"),
        (Method::DELETE, "/api/library/matches/1/split"),
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
/// directories joined into `"{root} (2 directories)"` would be prose the
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
/// fails the first of them and the second's `map_err` is never reached. What
/// lets one fail while the other answers is the filter they do not share:
/// `kind` narrows the list and deliberately does not narrow the counts, for
/// the reason `proposals::counts_by_kind`'s own doc gives. So a row the counts
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
/// which is the whole of `NoAnswer::Refused`'s argument.
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
/// See O70. The rule is the one the server previewed, so every refusal an
/// accept makes is passed before the write is reached: what fails is the
/// write itself.
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
/// route through a different `?`.
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
/// The fault is on the `DELETE FROM rule` that follows the reopening `UPDATE`
/// inside one transaction, so the state assertion below is also the rollback's.
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

/// **O72.** `DELETE .../dismiss` takes a dismissal back: the proposal leaves
/// the dismissed list and is in the open one again, with both tab counts
/// moving with it.
#[tokio::test]
async fn reopening_puts_a_dismissed_proposal_back_in_the_open_list() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let path = format!("/api/library/proposals/{id}/dismiss");
    let response = send_json(&router, Method::POST, &path, &serde_json::json!({})).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(proposal_state(&state, id), "dismissed");

    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    let open = get_json(&router, "/api/library/proposals?state=open&min_score=0").await;
    let ids: Vec<i64> = open["proposals"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|p| p["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(ids, vec![id], "{open}");
    assert_eq!(open["counts"]["edition-of"], 1, "{open}");
    let dismissed = get_json(
        &router,
        "/api/library/proposals?state=dismissed&min_score=0",
    )
    .await;
    assert_eq!(
        dismissed["proposals"].as_array().map(Vec::len),
        Some(0),
        "{dismissed}"
    );
    assert_eq!(
        dismissed["counts"].as_object().map(serde_json::Map::len),
        Some(0),
        "{dismissed}"
    );
}

/// Reopening a proposal that is not dismissed is a **409 naming why**, not a
/// failure and not a silent success — for an open one and for an accepted
/// one, whose sentence points at un-accepting instead.
#[tokio::test]
async fn reopening_a_proposal_that_is_not_dismissed_is_a_conflict() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let path = format!("/api/library/proposals/{id}/dismiss");

    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("only a dismissal can be reopened"), "{body}");
    assert_eq!(proposal_state(&state, id), "open");

    let rule = previewed_rule(&router, id).await;
    let response = send_json(
        &router,
        Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &rule,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    let status = response.status();
    let body = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("un-accepting it is what reopens it"),
        "{body}"
    );
    assert_eq!(proposal_state(&state, id), "accepted");
}

/// A store fault **during a reopen** is a 500, and the dismissal stands.
#[tokio::test]
async fn a_store_fault_during_a_reopen_is_a_500_rather_than_a_conflict() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let path = format!("/api/library/proposals/{id}/dismiss");
    let response = send_json(&router, Method::POST, &path, &serde_json::json!({})).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    a_store_fault_on(
        &state,
        "BEFORE UPDATE OF state ON proposal WHEN NEW.state = 'open'",
    );

    let response = send_json(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    let status = response.status();
    let body = test_support::body_string(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.contains("a simulated store fault"), "{body}");
    assert_eq!(proposal_state(&state, id), "dismissed");
}

/// A store `Err` for a proposal **gone by the time the store read it** is a
/// 404, and any other is the 500 a failed write gets (O72).
///
/// The race that produces the first — another process deleting the row
/// between `present` and the write's own read — cannot be staged
/// deterministically from here, so this drives the mapping the `dismiss` and
/// `reopen` handlers hand their store `Err` to, with a row that is gone and a
/// row that is not.
#[tokio::test]
async fn a_write_error_for_a_proposal_that_has_gone_is_a_404_and_any_other_a_500() {
    let (_router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = the_one_open_proposal(&state);
    let conn = state.library.conn();
    let gone = id + 1000;

    let answered = super::failed_unless_gone(&conn, gone, "looking up proposal: gone".to_string());
    assert!(
        matches!(answered, super::NoAnswer::Missing(missing) if missing == gone),
        "{answered:?}"
    );
    let answered = super::failed_unless_gone(&conn, id, "a simulated store fault".to_string());
    assert!(
        matches!(&answered, super::NoAnswer::Failed(why) if why == "a simulated store fault"),
        "{answered:?}"
    );
}

/// A rule the engine cannot evaluate, handed to the **accept** route, is still
/// a 409 naming why.
///
/// Not the 400 the dry run answers for the same rule: this route does not ask
/// `proposals::evaluable` first, and `accept_with` refuses such a rule
/// itself. What this pins is that the refusal is a refusal and not a store
/// failure — the check lives in `resolves`' sweep, whose `Err` is a failed
/// read, so an `accept_with` that left it there would answer 500 for a typo in
/// a pattern.
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
/// join error `spawn_blocking` itself hands back is not.
///
/// **The helper is called directly**, because these closures offer no lever to
/// panic on purpose: every store call hands back a `Result`, and
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
/// the library instead of the one the proposal is about. The two fixtures
/// here hold one root each, so the wrong answer and the right one decide the
/// same material.
///
/// Both directions, against a rule whose fields are all populated: the
/// outbound half through the route, where `root_id` and `set_kind` carry
/// values the server derived and can be compared to the library's own rows,
/// and the inbound half through an accept whose body sets `set_language`,
/// the third.
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
/// identically to one that does not — the same shape as the `root_id` case
/// above. This is the pair's own contract, stated over a rule with nothing
/// left at a default: what [`rule_json`] writes, [`rule_from_json`] reads
/// back unchanged.
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
/// A 409 is a refusal about the state of the library, which a caller can act
/// on; an id that does not exist is not that, it is not a 400 about the
/// caller's own rule either, and neither is it this server failing.
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
        (Method::DELETE, "/api/library/proposals/9999/dismiss"),
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
        // The schema's default, and a `true` here would mean an administrator
        // inherits `can_stamp` after all — the thing
        // `auth::tests::an_administrator_does_not_inherit_can_stamp` exists
        // to refuse.
        can_stamp: false,
        access: taiyaku_library::account::Access::Every,
    }
}

/// The two suffixed series in a library on a file, scanned and derived, with
/// the fixture that holds them and the database's path.
///
/// **File-backed, unlike every other fixture in this module**: over an
/// in-memory library `working()` is the guard `conn()` would be, and there is
/// no second connection to open.
fn a_file_backed_library(
    label: &str,
) -> (
    test_support::Fixture,
    taiyaku_library::Library,
    std::path::PathBuf,
) {
    let fixture = test_support::Fixture::new(label);
    for (pages, title) in (2_usize..).zip([
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ]) {
        test_support::write_pages(
            &fixture
                .join("tree")
                .join(title)
                .join(format!("{title} - v01")),
            pages,
        );
    }
    std::fs::create_dir_all(fixture.join("db")).expect("creates the database directory");
    let db = fixture.join("db").join("library.db");
    let library = taiyaku_library::Library::open(&db).expect("opens a file-backed library");
    {
        let conn = library.conn();
        let root =
            taiyaku_library::scan::add_root(&conn, &fixture.join("tree")).expect("adds the root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    (fixture, library, db)
}

/// `GET /{id}` stats each scanned member of both sides' first volume and
/// sniffs one that moved, so it must not hold `Library::conn`, a mutex every
/// authenticated request takes, across them: it reads on `Library::working`.
///
/// The holder lets go only when told, so a route on `conn()` cannot answer
/// inside the timeout. A route on a connection of its own has the same 20 s
/// to answer in.
#[tokio::test]
async fn the_expanded_row_does_not_wait_on_the_shared_connections_mutex() {
    use axum::extract::{Extension, Path as AxPath, State};
    use std::sync::Arc;

    let (fixture, library, _db) = a_file_backed_library("detail-second-connection");
    let state = Arc::new(crate::ServeState {
        library: Arc::new(library),
        works: crate::Works::fixed(Vec::new()),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        fingerprints: Default::default(),
        matches: Default::default(),
        dirs: Vec::new(),
    });
    let id = the_one_open_proposal(&state);

    let holder_library = Arc::clone(&state.library);
    let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let guard = holder_library.conn();
        held_tx.send(()).expect("the test is still waiting");
        // Until the test says so, or has gone.
        let _ = release_rx.recv();
        drop(guard);
    });
    held_rx.recv().expect("the holder thread took the mutex");

    let answered = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        crate::curation::detail(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            test_support::from_a_browser(),
            AxPath(id),
        ),
    )
    .await;

    // Before any assertion: a route still waiting on the mutex is on the
    // blocking pool, and the runtime waits for that pool as the test ends.
    release_tx.send(()).ok();
    holder.join().expect("the holder thread did not panic");

    let response = answered.expect(
        "GET /api/library/proposals/{id} did not answer in 20 s while another thread held \
         `Library::conn`: it stats and sniffs files, so it must read on `Library::working`'s \
         own connection",
    );
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("a JSON body");
    for side in ["subject", "target"] {
        assert_eq!(
            body[side]["first_volume"]["formats"],
            serde_json::json!(["dir"]),
            "the second connection reads what the first wrote: {body}"
        );
    }
}

/// What [`at_a_row_of`] is to do, and what it has seen done.
struct WriteBeside {
    /// Text of the statement the write runs beside.
    at: &'static str,
    /// Text of a statement the route begins after that row, one whose rows
    /// the write changes.
    then: &'static str,
    /// The write, until it runs.
    write: Option<Box<dyn FnOnce()>>,
    /// The write returned. rusqlite catches a panic in a trace callback and
    /// drops it, so a write that ran is not thereby one that committed.
    committed: bool,
    /// A statement holding `then` began after the write ran.
    read_after: bool,
}

thread_local! {
    /// [`at_a_row_of`]'s state: a trace callback is a `fn`, which captures
    /// nothing.
    static WRITE_BESIDE: std::cell::RefCell<Option<WriteBeside>> =
        const { std::cell::RefCell::new(None) };
}

/// A trace callback that runs [`WRITE_BESIDE`]'s write at the first row the
/// traced connection reads from a statement holding its `at`: after that
/// read's snapshot is taken, and before any later statement takes its own.
/// From then on it notes a statement holding `then` as it begins, since one
/// the write emptied reads no row: a write the route reads nothing of
/// afterwards tests no snapshot.
fn at_a_row_of(event: rusqlite::trace::TraceEvent<'_>) {
    let write = WRITE_BESIDE.with(|cell| {
        let mut cell = cell.borrow_mut();
        let beside = cell.as_mut()?;
        match event {
            rusqlite::trace::TraceEvent::Stmt(_, sql) if beside.write.is_none() => {
                beside.read_after |= sql.contains(beside.then);
                None
            }
            rusqlite::trace::TraceEvent::Row(read) if read.sql().contains(beside.at) => {
                beside.write.take()
            }
            _ => None,
        }
    });
    // Run outside the borrow, and marked only once it has returned.
    if let Some(write) = write {
        write();
        WRITE_BESIDE.with(|cell| {
            if let Some(beside) = cell.borrow_mut().as_mut() {
                beside.committed = true;
            }
        });
    }
}

/// The expanded row of `id` read on `library`'s connection, with `write` run
/// at the first row that connection reads from a statement holding `at`
/// ([`at_a_row_of`]). `write` returns once it has committed on a connection
/// of its own. `then` is text of a read it changes, which the route must make
/// after it: a write that lands after the last such read leaves the test
/// passing whether or not the route holds a snapshot.
fn expanded_with_a_write_beside(
    library: &taiyaku_library::Library,
    id: i64,
    at: &'static str,
    then: &'static str,
    write: impl FnOnce() + 'static,
) -> Result<serde_json::Value, super::NoAnswer> {
    WRITE_BESIDE.with(|cell| {
        *cell.borrow_mut() = Some(WriteBeside {
            at,
            then,
            write: Some(Box::new(write)),
            committed: false,
            read_after: false,
        });
    });
    let mut conn = library.conn();
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT
            | rusqlite::trace::TraceEventCodes::SQLITE_TRACE_ROW,
        Some(at_a_row_of),
    )
    .expect("installs the trace");
    let answer = super::expanded(&conn, id);
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::empty(),
        None::<fn(rusqlite::trace::TraceEvent<'_>)>,
    )
    .expect("clears the trace");
    let beside = WRITE_BESIDE
        .with(|cell| cell.borrow_mut().take())
        .expect("set above, and taken only here");
    assert!(
        beside.write.is_none(),
        "the fixture: the route read no row from a statement holding `{at}`, so the write never ran"
    );
    assert!(
        beside.committed,
        "the fixture: the write did not commit while the route read `{at}`"
    );
    assert!(
        beside.read_after,
        "the fixture: after the write, at its first row from `{at}`, the route began no read of \
         `{then}`, the read the write changes, so its answer would be the same without a snapshot"
    );
    answer
}

/// The file-backed library's one open proposal.
fn its_one_open_proposal(library: &taiyaku_library::Library) -> i64 {
    let conn = library.conn();
    taiyaku_library::proposals::raise(&conn).expect("raise");
    let open = taiyaku_library::proposals::open(&conn, 0.0).expect("open");
    assert_eq!(open.len(), 1, "the fixture raises exactly one: {open:?}");
    open[0].id
}

/// The expanded row is one committed state of the store. An accept commits on
/// a second connection once the route has read the proposal, at its next
/// read, of `series`, and the answer is the state before it: the proposal
/// open, its rule offered. Read a statement at a time, the open proposal would
/// be paired with the sweep after the accept, which offers no rule, and no
/// committed state holds that pair.
#[test]
fn the_expanded_row_reads_one_snapshot_of_the_store() {
    use taiyaku_library::proposals;

    let (_fixture, library, db) = a_file_backed_library("detail-one-snapshot");
    let id = its_one_open_proposal(&library);
    let rule =
        proposals::preview(&library.conn(), id).expect("the fixture's proposal is offered a rule");
    let other = taiyaku_library::Library::open(&db).expect("a second connection on the same file");

    let during =
        expanded_with_a_write_beside(&library, id, "FROM series", "FROM rule", move || {
            let accepted = proposals::accept_with(&other.conn(), id, &rule)
                .expect("the other connection's accept reads and writes");
            assert!(
                matches!(accepted, proposals::Offer::Made(_)),
                "the fixture: the accept is not refused"
            );
        })
        .expect("the route answers");

    assert_eq!(
        during["proposal"]["state"], "open",
        "the state before the accept: {during}"
    );
    assert!(
        during["rule"].is_object(),
        "an open proposal is offered its rule, as it was before the accept: {during}"
    );
    let after = super::expanded(&library.conn(), id).expect("the route answers");
    assert_eq!(
        after["proposal"]["state"], "accepted",
        "and the accept is there to read after it: {after}"
    );
    assert!(
        after["rule"].is_null(),
        "an accepted proposal is offered none: {after}"
    );
}

/// The snapshot starts before the route asks whether the proposal is there.
/// The row is deleted on a second connection at that first read, and the
/// answer is still the state before: found, and read whole. A snapshot taken
/// after that read would find the row and then not find it, a 500 for a
/// proposal that is a 404 from then on.
#[test]
fn the_expanded_row_reads_a_proposal_it_found_from_the_snapshot_it_found_it_in() {
    let (_fixture, library, db) = a_file_backed_library("detail-snapshot-from-the-first-read");
    let id = its_one_open_proposal(&library);
    let other = taiyaku_library::Library::open(&db).expect("a second connection on the same file");

    let during =
        expanded_with_a_write_beside(&library, id, "FROM proposal", "FROM proposal", move || {
            let deleted = other
                .conn()
                .execute("DELETE FROM proposal WHERE id = ?1", [id])
                .expect("the other connection deletes the proposal");
            assert_eq!(deleted, 1, "the fixture: the proposal was there to delete");
        });

    let during = during.expect("the proposal was there when the route started, so it answers");
    assert_eq!(
        during["proposal"]["state"], "open",
        "the state before the delete: {during}"
    );
    let after = super::expanded(&library.conn(), id);
    assert!(
        matches!(after, Err(super::NoAnswer::Missing(gone)) if gone == id),
        "and the proposal is gone after it: {after:?}"
    );
}

/// **No curation route does its SQLite work on the async worker.**
///
/// Every handler here holds the library's `MutexGuard` across the whole of its
/// work — `GET /{id}`, Split and Undo through `working()`, over this fixture
/// the guard `conn()` hands out — and several sweep every `scan_entry` row,
/// `POST /{id}/resolution` on every keystroke of C3's rule form among them.
/// On the async worker, a handler stalls the runtime.
///
/// **The milliseconds are deliberately not quoted here, and `docs/notes.md`
/// says why** ("What a swept row actually costs"): the cost is per byte of
/// path swept, so a figure quoted against a row count is a figure about
/// whichever synthetic paths a harness generated. That is O8/O12's failure
/// class, which `routes.rs` answers with `spawn_blocking` in every handler but
/// `get_sidecar`, which answers from memory and asks a grant that names roots
/// on the blocking pool (O115) — and which `auth::guard` answers for the
/// middleware every request already passes through.
///
/// **All of them, not just the ones that sweep.** A rule that holds for some of
/// a module's routes is a rule the next route added will be written against
/// wrongly — which is the same argument `every_curation_route_refuses_a_reader`
/// makes about the admin gate, and it is the reason this is a loop over the
/// module rather than a note in a doc comment.
///
/// Ordered so each write is driven in a state that lets it succeed: an accept,
/// then its undo, then a dismiss of the reopened row, then a reopen.
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

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::reopen(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(id),
        ),
    )
    .await;
    assert!(
        !stalled,
        "DELETE /api/library/proposals/{{id}}/dismiss ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(proposal_state(&state, id), "open", "and the reopen landed");

    // Split and Undo name a pair this store does not hold, which they read on
    // the library's connection: their 404 waited for the mutex.
    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::split_match(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(9999),
            axum::Json(serde_json::json!({ "work": "any", "edition": "en" })),
        ),
    )
    .await;
    assert!(
        !stalled,
        "POST /api/library/matches/{{id}}/split ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let (stalled, response) = test_support::the_runtime_stalls_on(
        &library,
        crate::curation::undo_split(
            State(Arc::clone(&state)),
            Extension(an_administrator()),
            crate::test_support::from_a_browser(),
            AxPath(9999),
        ),
    )
    .await;
    assert!(
        !stalled,
        "DELETE /api/library/matches/{{id}}/split ran on the async worker"
    );
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// --- Split and Undo ---------------------------------------------------------

/// The English volumes: edition key, kind and language.
const VOLUMES: (&str, &str, &str) = ("en", "official", "en");
/// An English fan translation.
const FAN: (&str, &str, &str) = ("fan", "fan", "en");
/// The Japanese raws.
const RAW: (&str, &str, &str) = ("ja", "raw", "ja");
/// A Korean scanlation.
const KOREAN_SCANS: (&str, &str, &str) = ("ko", "fan", "ko");

const OSHI: &str = "Oshi/v01/Oshi v01.cbz";
const OSHI_SCANS: &str = "Oshi Scans/v01/Oshi Scans v01.cbz";
const OSHINOKO: &str = "Oshinoko/v01/Oshinoko v01.cbz";

/// A directory of the fixture: its name, what its files fall back to as
/// edition key, kind and language, and its volume archives.
type Directory<'a> = (&'a str, (&'a str, &'a str, &'a str), &'a [&'a str]);

/// A router over `directories`, each in library `Manga`, which matches by
/// content from its start (migration 28), every file a three-page archive
/// of bytes its own, scanned, derived and read whole: each file takes part
/// once a pair names it. Answers each file's digest by [`at`] its directory
/// and path, as two directories can hold one path.
fn shelved(
    label: &str,
    directories: &[Directory<'_>],
) -> (
    axum::Router,
    std::sync::Arc<crate::ServeState>,
    test_support::Fixture,
    std::collections::BTreeMap<String, String>,
) {
    use taiyaku_library::scan::{self, InLibrary};

    let fixture = test_support::Fixture::new(label);
    let library = test_support::library();
    let mut digests = std::collections::BTreeMap::new();
    {
        let conn = library.conn();
        let mut seed = 0;
        let mut roots = Vec::new();
        for (name, (key, kind, language), files) in directories {
            let at = fixture.path().join(name);
            for file in *files {
                let path = at.join(file);
                let dir = path.parent().expect("a file sits in a directory");
                std::fs::create_dir_all(dir).expect("makes the volume's directory");
                seed += 10;
                taiyaku_read::test_support::cbz_pages(
                    dir,
                    &path.file_name().expect("a file name").to_string_lossy(),
                    3,
                    seed,
                );
            }
            let root = scan::add_root_in(&conn, &at, InLibrary::Named("Manga")).expect("adds it");
            conn.execute(
                "UPDATE root SET edition_key = ?2, default_kind = ?3, default_language = ?4
                  WHERE id = ?1",
                rusqlite::params![root.id, key, kind, language],
            )
            .expect("labels it");
            scan::scan_root(&conn, &root).expect("scans it");
            roots.push((root.id, *name, *files));
        }
        taiyaku_library::derive::derive(&conn).expect("derives");
        for (root, name, files) in roots {
            for file in files {
                let digest: String = conn
                    .query_row(
                        "SELECT sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
                        rusqlite::params![root, file],
                        |r| r.get(0),
                    )
                    .unwrap_or_else(|e| panic!("{file}'s digest: {e}"));
                conn.execute(
                    "INSERT INTO fingerprint (sha256, version, outcome, pages, prints)
                     VALUES (?1, ?2, 'whole', 3, x'00')",
                    rusqlite::params![digest, crate::fingerprint::VERSION],
                )
                .unwrap_or_else(|e| panic!("reads {file} whole: {e}"));
                digests.insert(at(name, file), digest);
            }
        }
    }
    let works = crate::Works::fixed(
        crate::works::from_library(&library).expect("projects the derived works"),
    );
    let (router, state) = test_support::router_over(library, works, &fixture);
    (router, state, fixture, digests)
}

/// The key [`shelved`] answers `file` of directory `dir` under.
fn at(dir: &str, file: &str) -> String {
    format!("{dir}/{file}")
}

/// `one` and `two` stored as a pair at `v`, as the match pass stores one,
/// and the pair's id.
fn pair(state: &crate::ServeState, one: &str, two: &str, v: f64) -> i64 {
    use taiyaku_library::matches::{self, NewPair};

    let conn = state.library.conn();
    let scored = NewPair {
        sha_a: one.to_owned(),
        sha_b: two.to_owned(),
        v,
        long_paired: 3,
        a_slots: 3,
        b_slots: 3,
        a_pages: 3,
        b_pages: 3,
        floor: None,
        runs: Vec::new(),
    };
    matches::store(&conn, &[scored], one, 1, 1).expect("stores the pair");
    conn.query_row(
        "SELECT id FROM content_match WHERE ?1 IN (sha_a, sha_b) AND ?2 IN (sha_a, sha_b)",
        [one, two],
        |r| r.get(0),
    )
    .expect("the pair's id")
}

fn derived(state: &crate::ServeState) {
    taiyaku_library::derive::derive(&state.library.conn()).expect("derives");
}

/// The one work holding `digest`.
fn holder(state: &crate::ServeState, digest: &str) -> i64 {
    let conn = state.library.conn();
    let works: Vec<i64> = conn
        .prepare(
            "SELECT DISTINCT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE m.sha256 = ?1 ORDER BY e.work_id",
        )
        .and_then(|mut stmt| stmt.query_map([digest], |r| r.get(0))?.collect())
        .expect("the works holding the file");
    assert_eq!(works.len(), 1, "one work holds {digest}: {works:?}");
    works[0]
}

fn slug(state: &crate::ServeState, work: i64) -> String {
    let conn = state.library.conn();
    conn.query_row("SELECT slug FROM work WHERE id = ?1", [work], |r| r.get(0))
        .expect("the work's slug")
}

/// A work's series and volume.
fn placed(state: &crate::ServeState, work: i64) -> (String, Option<f64>) {
    let conn = state.library.conn();
    conn.query_row(
        "SELECT s.sort_title, w.volume FROM work w JOIN series s ON s.id = w.series_id
          WHERE w.id = ?1",
        [work],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .expect("the work's series and volume")
}

/// A work's editions by key, each with the pair that joined it.
fn joined_editions(state: &crate::ServeState, work: i64) -> Vec<(String, Option<i64>)> {
    let conn = state.library.conn();
    conn.prepare("SELECT key, joined_by FROM edition WHERE work_id = ?1 ORDER BY key")
        .and_then(|mut stmt| {
            stmt.query_map([work], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("the work's editions")
}

/// The edition keyed `key`, joined by `by`.
fn keyed(key: &str, by: Option<i64>) -> (String, Option<i64>) {
    (key.to_owned(), by)
}

/// Every pair's state and the admin who split it, by id.
fn pair_states(state: &crate::ServeState) -> Vec<(i64, String, Option<i64>)> {
    let conn = state.library.conn();
    conn.prepare("SELECT id, state, split_by FROM content_match ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("the pairs' states")
}

/// The fixture account, the administrator every request here is made as.
const ADMIN: Option<i64> = Some(1);

fn scored(id: i64) -> (i64, String, Option<i64>) {
    (id, "scored".to_owned(), None)
}

fn split_by_the_admin(id: i64) -> (i64, String, Option<i64>) {
    (id, "split".to_owned(), ADMIN)
}

/// A Split of pair `id` naming edition `key` of work `work`: its status and
/// body.
async fn split(
    router: &axum::Router,
    state: &crate::ServeState,
    id: i64,
    work: i64,
    key: &str,
) -> (StatusCode, String) {
    let body = serde_json::json!({ "work": slug(state, work), "edition": key });
    let response = send_json(
        router,
        Method::POST,
        &format!("/api/library/matches/{id}/split"),
        &body,
    )
    .await;
    let status = response.status();
    (status, test_support::body_string(response).await)
}

/// An Undo of pair `id`'s Split: its status and body.
async fn undo(router: &axum::Router, id: i64) -> (StatusCode, String) {
    let response = send_json(
        router,
        Method::DELETE,
        &format!("/api/library/matches/{id}/split"),
        &serde_json::Value::Null,
    )
    .await;
    let status = response.status();
    (status, test_support::body_string(response).await)
}

fn json_of(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("a JSON body: {e}: {body}"))
}

/// One English volume and its raw, each in a series of its own, paired at
/// 0.95 and joined: the router, the store, the two digests and the pair.
fn english_and_raw(
    label: &str,
) -> (
    axum::Router,
    std::sync::Arc<crate::ServeState>,
    test_support::Fixture,
    (String, String, i64),
) {
    let (router, state, fixture, digests) = shelved(
        label,
        &[("Volumes", VOLUMES, &[OSHI]), ("Raw", RAW, &[OSHINOKO])],
    );
    let (en, ja) = (
        digests[&at("Volumes", OSHI)].clone(),
        digests[&at("Raw", OSHINOKO)].clone(),
    );
    let id = pair(&state, &en, &ja, 0.95);
    derived(&state);
    assert_eq!(
        holder(&state, &ja),
        holder(&state, &en),
        "the fixture: the raw joined the English volume's work"
    );
    (router, state, fixture, (en, ja, id))
}

/// The Split answers with the library it leaves: the raw is back where its
/// name puts it before the next derive, and a further derive keeps it there.
#[tokio::test]
async fn split_sends_the_file_back_at_once_and_a_rescan_keeps_it_apart() {
    let (router, state, _fixture, (en, ja, id)) = english_and_raw("split-at-once");
    let host = holder(&state, &en);

    let (status, body) = split(&router, &state, id, host, "ja").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let answer = json_of(&body);
    assert_eq!(answer["split"], serde_json::json!([id]), "{body}");
    assert_eq!(answer["derive"]["joined"], 0, "the derive's report: {body}");
    let freed = holder(&state, &ja);
    assert_ne!(freed, host, "the raw left the English volume's work");
    assert_eq!(
        (placed(&state, freed), joined_editions(&state, freed)),
        (("Oshinoko".to_owned(), Some(1.0)), vec![keyed("ja", None)]),
        "the raw's own work, where its name puts it"
    );
    assert_eq!(
        joined_editions(&state, host),
        [keyed("en", None)],
        "the host keeps no copy of it"
    );
    assert_eq!(pair_states(&state), [split_by_the_admin(id)]);

    derived(&state);

    assert_eq!(holder(&state, &ja), freed, "a later derive keeps it apart");
    assert_eq!(joined_editions(&state, host), [keyed("en", None)]);
    assert_eq!(pair_states(&state), [split_by_the_admin(id)]);
}

/// Official, fan and raw volumes, each with a work of its own until `pairs`
/// are stored and derived: the router, the store, and the three digests.
struct Triangle {
    router: axum::Router,
    state: std::sync::Arc<crate::ServeState>,
    _fixture: test_support::Fixture,
    official: String,
    fan: String,
    raw: String,
}

impl Triangle {
    fn new(label: &str) -> Self {
        let (router, state, fixture, digests) = shelved(
            label,
            &[
                ("Volumes", VOLUMES, &[OSHI]),
                ("Fan", FAN, &[OSHI_SCANS]),
                ("Raw", RAW, &[OSHINOKO]),
            ],
        );
        Self {
            router,
            state,
            _fixture: fixture,
            official: digests[&at("Volumes", OSHI)].clone(),
            fan: digests[&at("Fan", OSHI_SCANS)].clone(),
            raw: digests[&at("Raw", OSHINOKO)].clone(),
        }
    }
}

/// Review Focus 1: the raw pairs best with the official volume, then with
/// the fan's. A Split of the first naming the raw splits the second too, or
/// the raw would join the same work again through the fan; the fan stays,
/// joined by the pair it is in with the official volume.
#[tokio::test]
async fn split_on_a_triangle_splits_every_pair_tying_the_file_to_its_work() {
    let t = Triangle::new("split-triangle");
    let official_raw = pair(&t.state, &t.official, &t.raw, 0.93);
    let fan_raw = pair(&t.state, &t.fan, &t.raw, 0.91);
    let official_fan = pair(&t.state, &t.official, &t.fan, 0.89);
    derived(&t.state);
    let host = holder(&t.state, &t.official);
    assert_eq!(
        joined_editions(&t.state, host),
        [
            keyed("en", None),
            keyed("fan", Some(fan_raw)),
            keyed("ja", Some(official_raw)),
        ],
        "the fixture: one work, the raw joined by the official volume"
    );

    let (status, body) = split(&t.router, &t.state, official_raw, host, "ja").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        json_of(&body)["split"],
        serde_json::json!([official_raw, fan_raw]),
        "{body}"
    );
    let freed = holder(&t.state, &t.raw);
    assert_ne!(freed, host, "the raw left");
    assert_eq!(placed(&t.state, freed), ("Oshinoko".to_owned(), Some(1.0)));
    assert_eq!(holder(&t.state, &t.fan), host, "the fan stays");
    assert_eq!(
        joined_editions(&t.state, host),
        [keyed("en", None), keyed("fan", Some(official_fan))],
        "joined now by the pair it is in with the official volume"
    );
    assert_eq!(
        pair_states(&t.state),
        [
            split_by_the_admin(official_raw),
            split_by_the_admin(fan_raw),
            scored(official_fan),
        ]
    );

    let (hosts, raws) = (slug(&t.state, host), slug(&t.state, freed));
    let kept = |id: i64, series: &str, slug: &str| serde_json::json!({ "id": id, "split": official_raw, "series": series, "volume": 1.0, "slug": slug });
    let raw_panel = get_json(&t.router, &format!("/api/library/works/{raws}")).await;
    assert_eq!(
        raw_panel["kept_apart"],
        serde_json::json!([
            kept(official_raw, "Oshi", &hosts),
            kept(fan_raw, "Oshi", &hosts)
        ]),
        "the raw's panel lists both pairs kept apart: {raw_panel}"
    );
    let host_panel = get_json(&t.router, &format!("/api/library/works/{hosts}")).await;
    assert_eq!(
        host_panel["kept_apart"],
        serde_json::json!([
            kept(official_raw, "Oshinoko", &raws),
            kept(fan_raw, "Oshinoko", &raws)
        ]),
        "{host_panel}"
    );
    assert_eq!(
        marked(&host_panel),
        [
            ("en".to_owned(), serde_json::Value::Null),
            (
                "fan".to_owned(),
                serde_json::json!({ "id": official_fan, "score": 0.89 })
            ),
        ],
        "the host's panel marks no raw: {host_panel}"
    );
}

/// Each edition of a work as the work API sends it, with its mark, by key.
fn marked(work: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    let mut marks: Vec<(String, serde_json::Value)> = work["editions"]
        .as_array()
        .unwrap_or_else(|| panic!("editions: {work}"))
        .iter()
        .map(|edition| {
            (
                edition["key"].as_str().expect("a key").to_owned(),
                edition["matched"].clone(),
            )
        })
        .collect();
    marks.sort_by(|one, two| one.0.cmp(&two.0));
    marks
}

/// V(fan, raw) > V(official, raw) > V(official, fan): the fan and the raw
/// both carry (fan, raw) as the pair that joined them.
fn the_fan_and_raw_score_highest(t: &Triangle) -> (i64, i64, i64, i64) {
    let fan_raw = pair(&t.state, &t.fan, &t.raw, 0.95);
    let official_raw = pair(&t.state, &t.official, &t.raw, 0.92);
    let official_fan = pair(&t.state, &t.official, &t.fan, 0.89);
    derived(&t.state);
    let host = holder(&t.state, &t.official);
    assert_eq!(
        joined_editions(&t.state, host),
        [
            keyed("en", None),
            keyed("fan", Some(fan_raw)),
            keyed("ja", Some(fan_raw)),
        ],
        "the fixture: one work, the fan and the raw both joined by their own pair"
    );
    (host, fan_raw, official_raw, official_fan)
}

/// A Split of (fan, raw) naming the fan's edition splits the fan's ties to
/// its work and no other: the fan leaves, and the raw stays on the official
/// volume's work through (official, raw).
#[tokio::test]
async fn split_on_the_fan_of_a_triangle_leaves_the_raw_joined() {
    let t = Triangle::new("split-triangle-fan");
    let (host, fan_raw, official_raw, official_fan) = the_fan_and_raw_score_highest(&t);

    let (status, body) = split(&t.router, &t.state, fan_raw, host, "fan").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        json_of(&body)["split"],
        serde_json::json!([fan_raw, official_fan]),
        "{body}"
    );
    let freed = holder(&t.state, &t.fan);
    assert_ne!(freed, host, "the fan left");
    assert_eq!(
        placed(&t.state, freed),
        ("Oshi Scans".to_owned(), Some(1.0))
    );
    assert_eq!(holder(&t.state, &t.raw), host, "the raw stays");
    assert_eq!(
        joined_editions(&t.state, host),
        [keyed("en", None), keyed("ja", Some(official_raw))]
    );
    assert_eq!(
        pair_states(&t.state),
        [
            split_by_the_admin(fan_raw),
            scored(official_raw),
            split_by_the_admin(official_fan),
        ]
    );
}

/// The same pair naming the raw's edition: the raw leaves, and the fan stays
/// through (official, fan).
#[tokio::test]
async fn split_on_the_raw_of_that_triangle_leaves_the_fan_joined() {
    let t = Triangle::new("split-triangle-raw");
    let (host, fan_raw, official_raw, official_fan) = the_fan_and_raw_score_highest(&t);

    let (status, body) = split(&t.router, &t.state, fan_raw, host, "ja").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        json_of(&body)["split"],
        serde_json::json!([fan_raw, official_raw]),
        "{body}"
    );
    let freed = holder(&t.state, &t.raw);
    assert_ne!(freed, host, "the raw left");
    assert_eq!(placed(&t.state, freed), ("Oshinoko".to_owned(), Some(1.0)));
    assert_eq!(holder(&t.state, &t.fan), host, "the fan stays");
    assert_eq!(
        joined_editions(&t.state, host),
        [keyed("en", None), keyed("fan", Some(official_fan))]
    );
    assert_eq!(
        pair_states(&t.state),
        [
            split_by_the_admin(fan_raw),
            split_by_the_admin(official_raw),
            scored(official_fan),
        ]
    );
}

/// In the triangle where (fan, raw) scores highest, the raw's mark is (fan,
/// raw), though the official volume pairs with it on the same work: a Split of
/// (official, raw) naming the raw is a 404 that splits nothing, as the panel
/// offers no such Split.
#[tokio::test]
async fn a_split_naming_an_edition_another_pair_joined_is_a_404() {
    let t = Triangle::new("split-other-mark");
    let (host, fan_raw, official_raw, official_fan) = the_fan_and_raw_score_highest(&t);

    let (status, body) = split(&t.router, &t.state, official_raw, host, "ja").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        pair_states(&t.state),
        [scored(fan_raw), scored(official_raw), scored(official_fan)],
        "nothing split"
    );
    assert_eq!(
        joined_editions(&t.state, host),
        [
            keyed("en", None),
            keyed("fan", Some(fan_raw)),
            keyed("ja", Some(fan_raw)),
        ],
        "the fan and the raw stay, each marked as before"
    );
}

/// Review Focus 1's triangle with the raw split off through the route, which
/// sets both its pairs apart: the host, the raw's own work, and the pairs
/// (official, raw), (fan, raw) and (official, fan).
async fn the_raw_split_off_a_triangle(t: &Triangle) -> (i64, i64, [i64; 3]) {
    let official_raw = pair(&t.state, &t.official, &t.raw, 0.93);
    let fan_raw = pair(&t.state, &t.fan, &t.raw, 0.91);
    let official_fan = pair(&t.state, &t.official, &t.fan, 0.89);
    derived(&t.state);
    let host = holder(&t.state, &t.official);
    let (status, body) = split(&t.router, &t.state, official_raw, host, "ja").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the fixture: the Split lands: {body}"
    );
    assert_eq!(
        json_of(&body)["split"],
        serde_json::json!([official_raw, fan_raw]),
        "the fixture: both the raw's pairs are split: {body}"
    );
    let freed = holder(&t.state, &t.raw);
    assert_eq!(
        (
            holder(&t.state, &t.fan),
            joined_editions(&t.state, host),
            joined_editions(&t.state, freed)
        ),
        (
            host,
            vec![keyed("en", None), keyed("fan", Some(official_fan))],
            vec![keyed("ja", None)]
        ),
        "the fixture: the raw is on a work of its own, and the fan stays, marked by (official, fan)"
    );
    (host, freed, [official_raw, fan_raw, official_fan])
}

/// The panel of work `work`, through the work API.
async fn panel(t: &Triangle, work: i64) -> serde_json::Value {
    get_json(
        &t.router,
        &format!("/api/library/works/{}", slug(&t.state, work)),
    )
    .await
}

/// The triangle as it was before the Split: the raw and the fan both on the
/// host, each marked by the pair that joined it there, and every pair scored.
fn the_triangle_as_before_the_split(
    t: &Triangle,
    host: i64,
    [official_raw, fan_raw, official_fan]: [i64; 3],
) {
    assert_eq!(
        (holder(&t.state, &t.raw), holder(&t.state, &t.fan)),
        (host, host),
        "the raw and the fan are both back on the official volume's work"
    );
    assert_eq!(
        joined_editions(&t.state, host),
        [
            keyed("en", None),
            keyed("fan", Some(fan_raw)),
            keyed("ja", Some(official_raw)),
        ],
        "each marked as before the Split"
    );
    assert_eq!(
        pair_states(&t.state),
        [scored(official_raw), scored(fan_raw), scored(official_fan)],
        "every pair the Split set apart is scored again"
    );
}

/// The library's rows, so a derive that changes none shows.
fn changes(state: &crate::ServeState) -> u64 {
    state.library.conn().total_changes()
}

/// Undo restores the whole Split: naming (official, raw), the Split's own
/// pair, puts the triangle back as it was, the host's panel marks as before,
/// and the next derive changes nothing.
#[tokio::test]
async fn undoing_official_and_raw_after_a_triangle_split_restores_the_whole_split() {
    let t = Triangle::new("undo-triangle-official");
    let (host, _, pairs) = the_raw_split_off_a_triangle(&t).await;
    let [official_raw, fan_raw, _] = pairs;

    let (status, body) = undo(&t.router, official_raw).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    the_triangle_as_before_the_split(&t, host, pairs);
    let hosts = panel(&t, host).await;
    assert_eq!(
        (marked(&hosts), &hosts["kept_apart"]),
        (
            vec![
                ("en".to_owned(), serde_json::Value::Null),
                (
                    "fan".to_owned(),
                    serde_json::json!({ "id": fan_raw, "score": 0.91 })
                ),
                (
                    "ja".to_owned(),
                    serde_json::json!({ "id": official_raw, "score": 0.93 })
                ),
            ],
            &serde_json::json!([])
        ),
        "{hosts}"
    );
    let before = changes(&t.state);
    derived(&t.state);
    assert_eq!(changes(&t.state), before, "the next derive changes nothing");
}

/// Naming (fan, raw), the other pair the same Split set apart, undoes the
/// same whole Split.
#[tokio::test]
async fn undoing_fan_and_raw_after_a_triangle_split_restores_the_whole_split_too() {
    let t = Triangle::new("undo-triangle-fan");
    let (host, _, pairs) = the_raw_split_off_a_triangle(&t).await;

    let (status, body) = undo(&t.router, pairs[1]).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    the_triangle_as_before_the_split(&t, host, pairs);
    let before = changes(&t.state);
    derived(&t.state);
    assert_eq!(changes(&t.state), before, "the next derive changes nothing");
}

/// Two Splits: the raw off the triangle, then the fan off the host, which
/// sets (official, fan) apart alone. Undoing the first leaves the second's
/// pair split: the raw is back, and the fan stays on a work of its own.
#[tokio::test]
async fn undoing_one_split_leaves_another_splits_pairs_split() {
    let t = Triangle::new("undo-one-of-two");
    let (host, _, [official_raw, fan_raw, official_fan]) = the_raw_split_off_a_triangle(&t).await;
    let (status, body) = split(&t.router, &t.state, official_fan, host, "fan").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the fixture: the second Split lands: {body}"
    );
    assert_eq!(
        json_of(&body)["split"],
        serde_json::json!([official_fan]),
        "the fixture: the second Split sets (official, fan) apart alone: {body}"
    );

    let (status, body) = undo(&t.router, official_raw).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        pair_states(&t.state),
        [
            scored(official_raw),
            scored(fan_raw),
            split_by_the_admin(official_fan),
        ],
        "the first Split's pairs are scored, the second's split"
    );
    assert_eq!(holder(&t.state, &t.raw), host, "the raw is back");
    let fans = holder(&t.state, &t.fan);
    assert_ne!(fans, host, "the fan stays apart from the official volume");
    assert_eq!(
        joined_editions(&t.state, host),
        [keyed("en", None), keyed("ja", Some(official_raw))]
    );
}

/// Undo answers with the library it leaves: the pair joins again at once.
#[tokio::test]
async fn undo_rejoins_at_once() {
    let (router, state, _fixture, (en, ja, id)) = english_and_raw("undo-at-once");
    let host = holder(&state, &en);
    let (status, body) = split(&router, &state, id, host, "ja").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the fixture: the Split lands: {body}"
    );
    assert_ne!(holder(&state, &ja), host, "the fixture: the raw left");

    let (status, body) = undo(&router, id).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(json_of(&body)["derive"]["joined"], 1, "{body}");
    assert_eq!(holder(&state, &ja), host, "the raw is back");
    assert_eq!(
        joined_editions(&state, host),
        [keyed("en", None), keyed("ja", Some(id))]
    );
    assert_eq!(pair_states(&state), [scored(id)]);
}

/// Split and Undo derive the whole store, so each tries the job lock once and
/// refuses while a scan or another library job holds it, as a rule save does;
/// with the lock free, the same Split lands.
#[tokio::test]
async fn split_answers_409_while_the_job_lock_is_held() {
    let (router, state, _fixture, (en, ja, id)) = english_and_raw("split-busy");
    let host = holder(&state, &en);
    let held = test_support::holding_the_job(&state);

    let refused = [
        split(&router, &state, id, host, "ja").await,
        undo(&router, id).await,
    ];

    assert_eq!(
        refused,
        [
            (
                StatusCode::CONFLICT,
                "a scan or another library job is running; split again once it has finished"
                    .to_owned()
            ),
            (
                StatusCode::CONFLICT,
                "a scan or another library job is running; undo again once it has finished"
                    .to_owned()
            ),
        ]
    );
    assert_eq!(pair_states(&state), [scored(id)], "nothing split");
    assert_eq!(holder(&state, &ja), host, "the raw stays joined");

    drop(held);
    let (status, body) = split(&router, &state, id, host, "ja").await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// The derive lease covers a derive another process runs (O51): Split and
/// Undo refuse while it is held, as they refuse beside the job lock.
#[tokio::test]
async fn split_answers_409_while_another_derive_holds_the_lease() {
    let (router, state, _fixture, (en, ja, id)) = english_and_raw("split-leased");
    let host = holder(&state, &en);
    let held = test_support::holding_the_lease(&state.library);

    let refused = [
        split(&router, &state, id, host, "ja").await.0,
        undo(&router, id).await.0,
    ];

    assert_eq!(refused, [StatusCode::CONFLICT, StatusCode::CONFLICT]);
    assert_eq!(pair_states(&state), [scored(id)], "nothing split");
    assert_eq!(holder(&state, &ja), host, "the raw stays joined");

    drop(held);
    let (status, body) = split(&router, &state, id, host, "ja").await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// Each request names what the store does not hold as a Split needs it, and
/// is a 404 that splits nothing: pair 9999, on both routes; the raw's edition
/// under Oshi v02, whose work holds no `ja` edition (by key alone, Oshi v01's
/// would be found); the fan's edition of two files, which no derive marks, so
/// it is marked here with the pair naming one of them and only its count
/// refuses it; Oshi v02's English edition, which no pair joined; and the
/// Korean scanlation, which moved with the raw's group and carries the pair
/// that names the raw, not it.
#[tokio::test]
async fn a_split_naming_what_the_store_does_not_hold_is_a_404_and_splits_nothing() {
    const OSHI_2: &str = "Oshi/v02/Oshi v02.cbz";
    const SCANS_A: &str = "Oshi Scans/v01/Oshi Scans v01 a.cbz";
    const SCANS_B: &str = "Oshi Scans/v01/Oshi Scans v01 b.cbz";
    let (router, state, _fixture, digests) = shelved(
        "split-not-found",
        &[
            ("Volumes", VOLUMES, &[OSHI, OSHI_2]),
            ("Fan", FAN, &[SCANS_A, SCANS_B]),
            ("Raw", RAW, &[OSHINOKO]),
            ("Scans", KOREAN_SCANS, &[OSHINOKO]),
        ],
    );
    let (en, ja) = (
        &digests[&at("Volumes", OSHI)],
        &digests[&at("Raw", OSHINOKO)],
    );
    let joins = pair(&state, en, ja, 0.95);
    let shared = digests[&at("Fan", SCANS_A)]
        .clone()
        .min(digests[&at("Fan", SCANS_B)].clone());
    let sharing = pair(&state, en, &shared, 0.95);
    derived(&state);
    let host = holder(&state, en);
    let (second, scans) = (
        holder(&state, &digests[&at("Volumes", OSHI_2)]),
        holder(&state, &shared),
    );
    assert_eq!(
        (
            joined_editions(&state, host),
            joined_editions(&state, second)
        ),
        (
            vec![
                keyed("en", None),
                keyed("ja", Some(joins)),
                keyed("ko", Some(joins))
            ],
            vec![keyed("en", None)]
        ),
        "the fixture: the raw's group joined Oshi v01's work, and Oshi v02's holds only English"
    );
    state
        .library
        .conn()
        .execute(
            "UPDATE edition SET joined_by = ?1 WHERE work_id = ?2 AND key = 'fan'",
            [sharing, scans],
        )
        .expect("marks the fan's edition of two files");
    let named =
        |work: i64, key: &str| serde_json::json!({ "work": slug(&state, work), "edition": key });

    for (method, path, body) in [
        (
            Method::POST,
            "/api/library/matches/9999/split".to_owned(),
            named(host, "ja"),
        ),
        (
            Method::DELETE,
            "/api/library/matches/9999/split".to_owned(),
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            format!("/api/library/matches/{joins}/split"),
            named(second, "ja"),
        ),
        (
            Method::POST,
            format!("/api/library/matches/{sharing}/split"),
            named(scans, "fan"),
        ),
        (
            Method::POST,
            format!("/api/library/matches/{joins}/split"),
            named(second, "en"),
        ),
        (
            Method::POST,
            format!("/api/library/matches/{joins}/split"),
            named(host, "ko"),
        ),
    ] {
        let response = send_json(&router, method.clone(), &path, &body).await;
        let status = response.status();
        let text = test_support::body_string(response).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{method} {path} {body}: {text}"
        );
        assert_eq!(
            pair_states(&state),
            [scored(joins), scored(sharing)],
            "{method} {path} {body} split nothing"
        );
        assert_eq!(
            holder(&state, ja),
            host,
            "{method} {path} {body}: the raw stays"
        );
    }
}

/// Names put the raw on the English volume's work, so the pair between them
/// at the bar joined nothing and marks neither edition: a Split naming either
/// is a 404, as the mark the panel shows is the pair it names.
#[tokio::test]
async fn a_split_naming_an_edition_its_pair_did_not_join_is_a_404() {
    let (router, state, _fixture, digests) = shelved(
        "split-not-joined",
        &[("Volumes", VOLUMES, &[OSHI]), ("Raw", RAW, &[OSHI])],
    );
    let (en, ja) = (&digests[&at("Volumes", OSHI)], &digests[&at("Raw", OSHI)]);
    let id = pair(&state, en, ja, 0.95);
    derived(&state);
    let work = holder(&state, en);
    assert_eq!(
        (holder(&state, ja), joined_editions(&state, work)),
        (work, vec![keyed("en", None), keyed("ja", None)]),
        "the fixture: names put both on one work, and no join marks either"
    );

    for key in ["ja", "en"] {
        let (status, body) = split(&router, &state, id, work, key).await;

        assert_eq!(status, StatusCode::NOT_FOUND, "{key}: {body}");
        assert_eq!(pair_states(&state), [scored(id)], "{key}: nothing split");
        assert_eq!(holder(&state, ja), work, "{key}: the raw stays");
    }
}

/// A Split splits only what would join the file to its work again. The raw
/// joins Oshi v01 at 0.95, pairs at 0.7 with the fan edition names put on that
/// work, and at 0.9 with Other v01, which the English key Oshi v01 holds kept
/// it from joining. Both stay scored, and the raw then joins Other v01.
#[tokio::test]
async fn a_split_leaves_a_pair_under_the_bar_and_a_pair_with_another_work_scored() {
    const OTHER: &str = "Other/v01/Other v01.cbz";
    let (router, state, _fixture, digests) = shelved(
        "split-scope",
        &[
            ("Volumes", VOLUMES, &[OSHI, OTHER]),
            ("Fan", FAN, &[OSHI]),
            ("Raw", RAW, &[OSHINOKO]),
        ],
    );
    let (en, fan, other, ja) = (
        &digests[&at("Volumes", OSHI)],
        &digests[&at("Fan", OSHI)],
        &digests[&at("Volumes", OTHER)],
        &digests[&at("Raw", OSHINOKO)],
    );
    let joins = pair(&state, en, ja, 0.95);
    let under = pair(&state, fan, ja, 0.7);
    let elsewhere = pair(&state, other, ja, 0.9);
    derived(&state);
    let (host, second) = (holder(&state, en), holder(&state, other));
    assert_eq!(
        (
            holder(&state, fan),
            holder(&state, ja),
            joined_editions(&state, host),
            joined_editions(&state, second),
        ),
        (
            host,
            host,
            vec![
                keyed("en", None),
                keyed("fan", None),
                keyed("ja", Some(joins))
            ],
            vec![keyed("en", None)],
        ),
        "the fixture: the raw joined Oshi v01, where names put the fan edition, \
         and Other v01 holds only its own"
    );

    let (status, body) = split(&router, &state, joins, host, "ja").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        json_of(&body)["split"],
        serde_json::json!([joins]),
        "{body}"
    );
    assert_eq!(
        pair_states(&state),
        [split_by_the_admin(joins), scored(under), scored(elsewhere)]
    );
    assert_eq!(
        joined_editions(&state, host),
        [keyed("en", None), keyed("fan", None)]
    );
    assert_eq!(holder(&state, ja), second, "the raw joins Other v01");
    assert_eq!(
        joined_editions(&state, second),
        [keyed("en", None), keyed("ja", Some(elsewhere))]
    );
}
