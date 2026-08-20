use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use taiyaku_library::{account, credential};
use tower::ServiceExt as _;

use super::*;
use crate::routes::build_router;

/// §10: "The session token in the database is not the token in the cookie —
/// asserted by inequality, so storing it raw fails rather than passing by
/// inspection."
#[test]
fn the_stored_session_is_not_the_cookie_value() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let token = credential::mint_token();
    account::open_session(&conn, 1, &token, None, None).expect("opens");

    let stored: String = conn
        .query_row("SELECT token_sha256 FROM session", [], |row| row.get(0))
        .expect("reads");
    assert_ne!(stored, token, "the raw token reached the database");
    assert_eq!(stored, credential::token_sha256(&token));
}

/// §4's cookie flags. `Secure` is the one that varies, and it varies on
/// configuration rather than on environment.
#[test]
fn cookie_flags_follow_the_configuration() {
    let plain = set_cookie("tok", false);
    assert!(plain.contains("taiyaku_session=tok"));
    assert!(plain.contains("HttpOnly"));
    assert!(plain.contains("SameSite=Lax"));
    assert!(plain.contains("Path=/"));
    assert!(plain.contains("Max-Age=2592000"), "thirty days in seconds");
    assert!(
        !plain.contains("Secure"),
        "Secure without a trusted proxy would break a plain-HTTP deployment \
         silently — the cookie would simply never come back"
    );

    let secured = set_cookie("tok", true);
    assert!(secured.contains("Secure"));

    // Clearing keeps the flags, because a browser matches a replacement cookie
    // on its attributes: one cleared without `Secure` does not replace one set
    // with it, and the reader stays logged in after pressing log out.
    let cleared = clear_cookie(true);
    assert!(cleared.contains("Max-Age=0"));
    assert!(cleared.contains("HttpOnly"));
    assert!(cleared.contains("Secure"));
}

/// §4: changing a password ends every other session, so a compromised one
/// cannot outlive the change that was meant to end it — **and keeps the current
/// one**, so changing your own password does not log you out of the browser you
/// changed it from.
#[test]
fn a_password_change_ends_every_session_but_this_one() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let mine = credential::mint_token();
    let theirs = credential::mint_token();
    account::open_session(&conn, 1, &mine, None, None).expect("opens");
    account::open_session(&conn, 1, &theirs, None, None).expect("opens");

    let closed = account::close_other_sessions(&conn, 1, &mine).expect("closes");
    assert_eq!(closed, 1);
    assert!(
        account::session_owner(&conn, &mine)
            .expect("reads")
            .is_some()
    );
    assert!(
        account::session_owner(&conn, &theirs)
            .expect("reads")
            .is_none()
    );
}

/// **API keys survive a password change**, deliberately: they are independent
/// credentials with their own revocation, and bundling them would make rotating
/// a password silently break every reader in the house.
#[test]
fn a_password_change_leaves_api_keys_alone() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();
    // A key does not authenticate an unclaimed account, so this needs a claimed
    // one — the subject here is what a password change does to keys, not the
    // locked-account refusal.
    let phc = credential::hash_password("first").expect("hashes");
    conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
        .expect("claims");
    let secret = credential::mint_api_key();
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
        [credential::token_sha256(&secret)],
    )
    .expect("inserts");
    let mine = credential::mint_token();
    account::open_session(&conn, 1, &mine, None, None).expect("opens");

    account::close_other_sessions(&conn, 1, &mine).expect("closes");

    assert!(
        account::by_api_key(&conn, &secret)
            .expect("reads")
            .is_some()
    );
}

/// An expired row resolves to nobody and is deleted on the lookup that found
/// it, per §4's "deleted lazily on lookup and swept at startup".
#[test]
fn an_expired_session_resolves_to_nobody_and_is_removed() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let token = credential::mint_token();
    conn.execute(
        "INSERT INTO session (token_sha256, user_id, expires)
         VALUES (?1, 1, datetime('now', '-1 hour'))",
        [credential::token_sha256(&token)],
    )
    .expect("inserts");

    assert!(
        account::session_owner(&conn, &token)
            .expect("reads")
            .is_none()
    );
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM session", [], |row| row.get(0))
        .expect("counts");
    assert_eq!(left, 0, "the expired row was not swept on lookup");
}

/// Using a session slides its expiry forward, which is what makes a thirty-day
/// session one login per browser per month rather than one per month exactly.
#[test]
fn using_a_session_slides_its_expiry_forward() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let token = credential::mint_token();
    conn.execute(
        "INSERT INTO session (token_sha256, user_id, expires)
         VALUES (?1, 1, datetime('now', '+1 hour'))",
        [credential::token_sha256(&token)],
    )
    .expect("inserts");

    account::session_owner(&conn, &token)
        .expect("reads")
        .expect("resolves");

    let expires: String = conn
        .query_row("SELECT expires FROM session", [], |row| row.get(0))
        .expect("reads");
    let far_off: String = conn
        .query_row("SELECT datetime('now', '+2 days')", [], |row| row.get(0))
        .expect("reads");
    assert!(
        expires > far_off,
        "expiry did not slide: {expires} is not beyond {far_off}"
    );
}

/// §4's CSRF rule. **An absent `Origin` is refused rather than assumed
/// same-origin**, because absent is precisely what a forged cross-site form
/// produces.
#[test]
fn state_changing_methods_need_a_matching_origin() {
    assert!(origin_is_acceptable(
        &Method::GET,
        None,
        Some("example:8484")
    ));
    assert!(origin_is_acceptable(
        &Method::HEAD,
        None,
        Some("example:8484")
    ));

    for method in [Method::PUT, Method::POST, Method::PATCH, Method::DELETE] {
        assert!(
            !origin_is_acceptable(&method, None, Some("example:8484")),
            "{method} with no Origin was allowed"
        );
        assert!(
            !origin_is_acceptable(&method, Some("http://evil.test"), Some("example:8484")),
            "{method} from a foreign Origin was allowed"
        );
        assert!(
            origin_is_acceptable(&method, Some("http://example:8484"), Some("example:8484")),
            "{method} from its own Origin was refused"
        );
    }

    // A prefix match would let `example:8484.evil.test` through; an authority
    // comparison does not.
    assert!(!origin_is_acceptable(
        &Method::PUT,
        Some("http://example:8484.evil.test"),
        Some("example:8484"),
    ));
}

/// The whole round trip through the router: log in, use the cookie, log out,
/// and find the cookie no longer works.
#[tokio::test]
async fn a_session_can_be_opened_used_and_ended() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-session-round-trip",
    ));

    let login = build_router(state.clone())
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::from(format!(
                    r#"{{"name":"{}","password":"{}"}}"#,
                    crate::test_support::FIXTURE_NAME,
                    crate::test_support::FIXTURE_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(login.status(), StatusCode::NO_CONTENT);

    let set = login
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .expect("a Set-Cookie")
        .to_string();
    let cookie = set
        .split(';')
        .next()
        .expect("a name=value pair")
        .to_string();
    assert!(cookie.starts_with("taiyaku_session="));

    let used = build_router(state.clone())
        .oneshot(
            Request::get("/api/works")
                .header("cookie", &cookie)
                .header("host", "test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(
        used.status(),
        StatusCode::OK,
        "the session cookie did not work"
    );

    let out = build_router(state.clone())
        .oneshot(
            Request::delete("/api/session")
                .header("cookie", &cookie)
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(out.status(), StatusCode::NO_CONTENT);

    let after = build_router(state)
        .oneshot(
            Request::get("/api/works")
                .header("cookie", &cookie)
                .header("host", "test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(
        after.status(),
        StatusCode::UNAUTHORIZED,
        "the cookie still worked after logging out"
    );
}

/// A wrong password is one refusal shape whether the account exists or not, so
/// a caller cannot enumerate names by the answer.
#[tokio::test]
async fn a_bad_login_says_the_same_thing_whoever_it_names() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-session-bad-login"));

    let mut answers = Vec::new();
    for body in [
        r#"{"name":"local","password":"wrong"}"#,
        r#"{"name":"nobody-at-all","password":"wrong"}"#,
    ] {
        let response = build_router(state.clone())
            .oneshot(
                Request::post("/api/session")
                    .header("content-type", "application/json")
                    .header("host", "test")
                    .header("origin", "http://test")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .expect("answers");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().get("set-cookie").is_none());
        answers.push(
            String::from_utf8(
                axum::body::to_bytes(response.into_body(), 1 << 16)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap(),
        );
    }
    assert_eq!(
        answers[0], answers[1],
        "a wrong password and an unknown account answered differently, which \
         is how an account list gets enumerated"
    );
}

/// A cookie is not enough for a state-changing request without a matching
/// `Origin` — the CSRF rule, asserted through the router rather than only on
/// the predicate.
#[tokio::test]
async fn a_cookie_alone_cannot_change_state_cross_site() {
    let state = crate::test_support::state("taiyaku-test-session-csrf");
    let token = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, 1, &token, None, None).expect("opens");
    }
    let state = Arc::new(state);
    let cookie = format!("{COOKIE_NAME}={token}");

    let forged = build_router(state.clone())
        .oneshot(
            Request::put("/api/works/t/progress")
                .header("cookie", &cookie)
                .header("host", "test")
                .header("origin", "http://evil.test")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":1}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    // **403, not 401.** The reader is signed in and their cookie is valid; the
    // request came from elsewhere. `api.ts` routes a 401 to the login screen, so
    // answering one here signed the reader out and discarded their unsaved notes
    // — repeatedly, since signing in again worked and the next write threw them
    // out again. Found by the third whole-branch review.
    assert_eq!(forged.status(), StatusCode::FORBIDDEN);

    let same_site = build_router(state)
        .oneshot(
            Request::put("/api/works/t/progress")
                .header("cookie", &cookie)
                .header("host", "test")
                .header("origin", "http://test")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":1}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(same_site.status(), StatusCode::NO_CONTENT);
}

/// **A stale cookie beside a valid header is answered by the header.**
///
/// The cookie is tried first because it is the reader's common path; falling
/// through when it does not resolve is what stops a client holding both from
/// being locked out by the weaker of its two credentials.
#[tokio::test]
async fn a_dead_cookie_does_not_veto_a_valid_header() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-session-fallthrough",
    ));

    let response = build_router(state)
        .oneshot(
            Request::get("/api/works")
                .header("cookie", format!("{COOKIE_NAME}=never-was-a-session"))
                .header(
                    "authorization",
                    crate::test_support::fixture_authorization(),
                )
                .header("host", "test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
}

/// `/api/me` is behind the guard, so a 401 there is the reader's signal to show
/// its login screen rather than an error to display.
#[tokio::test]
async fn whoami_is_guarded_and_answers_the_resolved_principal() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-session-me"));

    let anonymous = build_router(state.clone())
        .oneshot(Request::get("/api/me").body(Body::empty()).unwrap())
        .await
        .expect("answers");
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let signed_in = build_router(state)
        .oneshot(
            Request::get("/api/me")
                .header(
                    "authorization",
                    crate::test_support::fixture_authorization(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(signed_in.status(), StatusCode::OK);
    let body = String::from_utf8(
        axum::body::to_bytes(signed_in.into_body(), 1 << 16)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("\"user_id\":1"), "answered {body}");
    assert!(body.contains("\"admin\":true"), "answered {body}");
}

/// **Login CSRF: a cross-site page must not be able to log a reader in.**
///
/// The attack is not "steal the session" — it is the reverse. A page on another
/// origin makes the victim's browser post the *attacker's* credentials, and the
/// victim then reads, annotates and records positions into an account the
/// attacker can log into and read back.
///
/// `login` and `logout` sit outside the guard, so the origin check §4 applies to
/// every other state-changing request did not reach them. Two accidents of the
/// browser were doing the work instead: an HTML form cannot send
/// `application/json`, and `fetch` with that content type — or with `DELETE` —
/// triggers a CORS preflight this server never answers. Both are real, and
/// neither is this server's doing: adding a permissive CORS layer, or an
/// extractor that accepts a form encoding, would open it with nothing here
/// changing to say so.
///
/// Found by an automated security review of this branch on 2026-08-18.
#[tokio::test]
async fn a_cross_site_page_cannot_log_a_reader_in_or_out() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-session-login-csrf",
    ));
    let credentials = format!(
        r#"{{"name":"{}","password":"{}"}}"#,
        crate::test_support::FIXTURE_NAME,
        crate::test_support::FIXTURE_PASSWORD
    );

    let forged = build_router(state.clone())
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://evil.test")
                .body(Body::from(credentials.clone()))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(
        forged.status(),
        StatusCode::FORBIDDEN,
        "a login from a foreign Origin was accepted"
    );
    assert!(
        forged.headers().get("set-cookie").is_none(),
        "a refused login must not hand out a session"
    );

    // Absent is refused too, and for the reason §4 already gives about the
    // guarded routes: absent is precisely what a forged cross-site form
    // produces, so assuming same-origin resolves the ambiguity the wrong way.
    let anonymous_origin = build_router(state.clone())
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .header("host", "test")
                .body(Body::from(credentials.clone()))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(anonymous_origin.status(), StatusCode::FORBIDDEN);

    // Logout is the same shape and a smaller prize — an attacker who can forge
    // it can only sign the reader out — but it costs nothing to close and
    // leaving it open would make the rule "every state change checks its
    // origin, except this one".
    let forged_logout = build_router(state.clone())
        .oneshot(
            Request::delete("/api/session")
                .header("host", "test")
                .header("origin", "http://evil.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(forged_logout.status(), StatusCode::FORBIDDEN);

    // And the reader's own origin still works, which is the half that would
    // make this a regression rather than a fix if it broke.
    let honest = build_router(state)
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::from(credentials))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(honest.status(), StatusCode::NO_CONTENT);
    assert!(honest.headers().get("set-cookie").is_some());
}

/// A refused origin is **403 rather than 401**, and that is a diagnosis rather
/// than a preference: every client reads a 401 from a login route as "wrong
/// password". A misconfigured reverse proxy that rewrites `Origin` or drops
/// `Host` would otherwise present as every account's password being wrong at
/// once, which is the least actionable message this server could send.
#[tokio::test]
async fn a_refused_origin_does_not_read_as_a_wrong_password() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-session-403-not-401",
    ));

    let response = build_router(state)
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://evil.test")
                .body(Body::from(r#"{"name":"local","password":"whatever"}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        body.to_lowercase().contains("origin"),
        "the refusal must name what was wrong with the request: {body}"
    );
}

/// §7 through the router: after enough failures the route stops answering
/// "wrong password" and starts answering "not now", and the right password does
/// not get through while it does.
///
/// **429 rather than 401**, because they are different facts and a client
/// retries differently for each — a reader told "wrong password" ten times
/// starts doubting the password.
#[tokio::test]
async fn a_run_of_failures_locks_the_pair_out_and_the_right_password_waits() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-session-lockout"));

    let attempt = |password: &'static str| {
        let state = state.clone();
        async move {
            build_router(state)
                .oneshot(
                    Request::post("/api/session")
                        .header("content-type", "application/json")
                        .header("host", "test")
                        .header("origin", "http://test")
                        .body(Body::from(format!(
                            r#"{{"name":"{}","password":"{password}"}}"#,
                            crate::test_support::FIXTURE_NAME
                        )))
                        .unwrap(),
                )
                .await
                .expect("answers")
                .status()
        }
    };

    for _ in 0..taiyaku_library::account::LOCKOUT_AFTER {
        assert_eq!(attempt("wrong").await, StatusCode::UNAUTHORIZED);
    }

    assert_eq!(
        attempt("wrong").await,
        StatusCode::TOO_MANY_REQUESTS,
        "the eleventh failure still read as a wrong password"
    );
    assert_eq!(
        attempt(crate::test_support::FIXTURE_PASSWORD).await,
        StatusCode::TOO_MANY_REQUESTS,
        "a locked-out pair must not be let in by getting it right on the next try"
    );
}

/// Getting it right forgets the failures before it, so two typos do not
/// accumulate against the next slip weeks later.
#[tokio::test]
async fn succeeding_forgets_the_failures_before_it() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-session-forgets"));

    let attempt = |password: String| {
        let state = state.clone();
        async move {
            build_router(state)
                .oneshot(
                    Request::post("/api/session")
                        .header("content-type", "application/json")
                        .header("host", "test")
                        .header("origin", "http://test")
                        .body(Body::from(format!(
                            r#"{{"name":"{}","password":"{password}"}}"#,
                            crate::test_support::FIXTURE_NAME
                        )))
                        .unwrap(),
                )
                .await
                .expect("answers")
                .status()
        }
    };

    for _ in 0..taiyaku_library::account::LOCKOUT_AFTER - 1 {
        assert_eq!(attempt("wrong".to_string()).await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(
        attempt(crate::test_support::FIXTURE_PASSWORD.to_string()).await,
        StatusCode::NO_CONTENT
    );

    // The count is back to zero, so a fresh run of failures gets the full
    // allowance rather than tripping on the first.
    for _ in 0..taiyaku_library::account::LOCKOUT_AFTER - 1 {
        assert_eq!(attempt("wrong".to_string()).await, StatusCode::UNAUTHORIZED);
    }
}

/// A failure against an account that does not exist counts too, or the limiter
/// would only ever slow down guesses at names it had already confirmed.
#[tokio::test]
async fn guessing_at_names_is_rate_limited_as_well() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-session-name-guess",
    ));

    let attempt = |name: String| {
        let state = state.clone();
        async move {
            build_router(state)
                .oneshot(
                    Request::post("/api/session")
                        .header("content-type", "application/json")
                        .header("host", "test")
                        .header("origin", "http://test")
                        .body(Body::from(format!(
                            r#"{{"name":"{name}","password":"whatever"}}"#
                        )))
                        .unwrap(),
                )
                .await
                .expect("answers")
                .status()
        }
    };

    // The same name every time, since the key is (address, name) — a guesser
    // trying a different name each time is a different pair, which is the
    // trade §7 makes deliberately and is worth stating rather than pretending
    // otherwise.
    for _ in 0..taiyaku_library::account::LOCKOUT_AFTER {
        assert_eq!(
            attempt("no-such-account".to_string()).await,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        attempt("no-such-account".to_string()).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// **A new name every request dodged the pair key, and after the decoy hash
/// landed that made argon2 free to run.**
///
/// `guessing_at_names_is_rate_limited_as_well` above states the pair key's gap
/// and calls it a deliberate §7 trade, and it was one — while an unknown name
/// returned in under a millisecond. `credential::DECOY_HASH` then made an
/// unknown name cost what a real one costs, deliberately, to close a timing
/// oracle. That turned the documented trade into an unauthenticated denial of
/// service: every request carrying a name nobody has ever used takes a
/// `spawn_blocking` thread and argon2's 19 MiB for ~12 ms, with no counter it
/// can ever reach, and leaves a `login_attempt` row behind as well.
///
/// One fix produced the other finding two rounds later. The wider key —
/// the address alone, at `ADDRESS_LOCKOUT_AFTER` — is what bounds it, and it
/// is checked before the decoy runs.
///
/// Found by the fourth whole-branch review, 2026-08-19.
#[tokio::test]
async fn inventing_a_new_name_every_time_does_not_buy_unlimited_hashing() {
    use taiyaku_library::account::ADDRESS_LOCKOUT_AFTER;

    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-session-name-flood",
    ));

    let attempt = |name: String| {
        let state = state.clone();
        async move {
            build_router(state)
                .oneshot(
                    Request::post("/api/session")
                        .header("content-type", "application/json")
                        .header("host", "test")
                        .header("origin", "http://test")
                        .body(Body::from(format!(
                            r#"{{"name":"{name}","password":"whatever"}}"#
                        )))
                        .unwrap(),
                )
                .await
                .expect("answers")
                .status()
        }
    };

    // One real request with a name nobody has ever held. It is refused, and it
    // must leave a row under the address-only key — which is the half the pair
    // key could never see.
    assert_eq!(
        attempt("nobody-one".to_string()).await,
        StatusCode::UNAUTHORIZED
    );
    let wide: String = state
        .library
        .conn()
        .query_row(
            "SELECT key FROM login_attempt WHERE key NOT LIKE '%' || char(10) || '%'",
            [],
            |row| row.get(0),
        )
        .expect("a request with an unknown name recorded nothing against the address alone");

    // **Wound forward rather than driven there by ADDRESS_LOCKOUT_AFTER real
    // requests.** Each one costs a full argon2 verify by design — that is the
    // cost this ceiling exists to bound — and fifty of them made a test the
    // slowest thing in the workspace, which `.config/nextest.toml` reasons
    // about. Three requests prove what is actually in question: that a fresh
    // name is still counted, and that the gate fires when the count arrives.
    state
        .library
        .conn()
        .execute(
            "UPDATE login_attempt SET count = ?2 WHERE key = ?1",
            rusqlite::params![&wide, ADDRESS_LOCKOUT_AFTER - 1],
        )
        .expect("winds the window forward");

    assert_eq!(
        attempt("nobody-two".to_string()).await,
        StatusCode::UNAUTHORIZED,
        "the last of the allowance is still spendable"
    );
    assert_eq!(
        attempt("nobody-three".to_string()).await,
        StatusCode::TOO_MANY_REQUESTS,
        "a fresh name still bought another argon2 verify, so an unauthenticated \
         caller can spend this server's memory and threads without limit"
    );

    // And the rows stop accumulating with the hashing, which is the other half
    // of what an unmetered key cost: one `login_attempt` row per invented name,
    // swept only at startup.
    let rows: i64 = state
        .library
        .conn()
        .query_row("SELECT COUNT(*) FROM login_attempt", [], |row| row.get(0))
        .expect("counts");
    assert_eq!(
        rows, 3,
        "the refused third request still wrote a row, so the table grows once \
         per name offered however long the flood runs"
    );
}

/// A state whose fixture account has been put back to locked, so the library
/// reads as unclaimed — which is what a fresh one is, and what
/// `test_support::library` deliberately is not.
fn unclaimed(dir: &str, allow_web_setup: bool) -> Arc<ServeState> {
    let mut state = crate::test_support::state(dir);
    {
        let conn = state.library.conn();
        conn.execute(
            "UPDATE user SET password_hash = ?1 WHERE id = 1",
            [credential::LOCKED],
        )
        .expect("locks the fixture account");
    }
    state.auth.allow_web_setup = allow_web_setup;
    Arc::new(state)
}

fn claim_request(name: &str, password: &str) -> Request<Body> {
    Request::post("/api/claim")
        .header("content-type", "application/json")
        .header("host", "test")
        .header("origin", "http://test")
        .body(Body::from(format!(
            r#"{{"name":"{name}","password":"{password}"}}"#
        )))
        .unwrap()
}

/// §5: the claim route exists only behind the flag, and **closes the instant an
/// admin exists** rather than when the flag is turned off — which nobody will
/// do.
#[tokio::test]
async fn the_claim_route_closes_the_moment_an_admin_exists() {
    // Flag off: 404, and indistinguishable from a server built without it.
    let off = unclaimed("taiyaku-test-claim-off", false);
    let response = build_router(off)
        .oneshot(claim_request("davey", "a password"))
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Flag on, no admin: claimed, and signed in by the same response.
    let on = unclaimed("taiyaku-test-claim-on", true);
    let claimed = build_router(on.clone())
        .oneshot(claim_request("davey", "a password"))
        .await
        .expect("answers");
    assert_eq!(claimed.status(), StatusCode::NO_CONTENT);
    assert!(
        claimed.headers().get("set-cookie").is_some(),
        "a person who has just set the only password on the server should not \
         then be asked for it"
    );

    // Flag still on, admin now exists: shut, with the flag untouched.
    let again = build_router(on)
        .oneshot(claim_request("someone-else", "another"))
        .await
        .expect("answers");
    assert_eq!(
        again.status(),
        StatusCode::NOT_FOUND,
        "the claim route stayed open after the server was claimed"
    );
}

/// §5's constraint again, through the web path this time: claiming promotes the
/// seeded row rather than inserting beside it, so everything recorded before the
/// server had a login still belongs to the account that now exists.
#[tokio::test]
async fn claiming_from_the_browser_promotes_the_same_row_the_cli_would() {
    let state = unclaimed("taiyaku-test-claim-promotes", true);
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO work (content_key, slug, title, reading, hub)
             VALUES ('k', 's', 'T', 'rtl', 'raw')",
            [],
        )
        .expect("a work");
        let work_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 12)",
            [work_id],
        )
        .expect("a position from before there was a login");
    }

    let response = build_router(state.clone())
        .oneshot(claim_request("davey", "a password"))
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let conn = state.library.conn();
    let accounts: i64 = conn
        .query_row("SELECT COUNT(*) FROM user", [], |row| row.get(0))
        .expect("counts");
    assert_eq!(accounts, 1, "claiming inserted a second account");
    let (id, name): (i64, String) = conn
        .query_row("SELECT id, name FROM user", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("reads");
    assert_eq!(id, 1);
    assert_eq!(name, "davey");
    let slot: i64 = conn
        .query_row("SELECT slot FROM progress WHERE user_id = 1", [], |row| {
            row.get(0)
        })
        .expect("the position survived");
    assert_eq!(slot, 12);
}

/// The claim route takes the origin check too. It is the one request that can
/// create an administrator, so a cross-site page reaching it would be worse than
/// login CSRF rather than the same.
#[tokio::test]
async fn a_cross_site_page_cannot_claim_the_server() {
    let state = unclaimed("taiyaku-test-claim-csrf", true);
    let response = build_router(state)
        .oneshot(
            Request::post("/api/claim")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://evil.test")
                .body(Body::from(r#"{"name":"attacker","password":"theirs"}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// **The three fail-closed claims on the login path, none of which had a test.**
///
/// Each is a documented decision that the code comments argue for at length, and
/// mutation testing found all three unasserted on 2026-08-19: `||` could become
/// `&&` where a failure to record is turned into a lockout, and the whole
/// `Ok(Attempt::Failed)` match arm could be **deleted** — falling through to the
/// 401 beside it — with every test in the workspace still passing.
///
/// The 401 is the one answer that cannot be true in that case: the password was
/// right and the session write lost to a busy writer, so telling the client
/// "that name and password did not match" leaves a reader retyping a correct
/// password forever. That was a third-review fix, and nothing checked it.
///
/// Reachable by taking the tables away, which is the same error class the
/// comments reason about — `serve` and `library import` share one file, so a
/// write can lose to a busy writer — reproduced deterministically rather than by
/// racing anything.
#[tokio::test]
async fn a_login_that_cannot_be_recorded_is_not_answered_as_a_wrong_password() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-login-unrecordable",
    ));
    {
        let conn = state.library.conn();
        // The password is right; only the session write will fail.
        conn.execute("DROP TABLE session", []).expect("drops");
    }

    let response = build_router(state)
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::from(format!(
                    r#"{{"name":"{}","password":"{}"}}"#,
                    crate::test_support::FIXTURE_NAME,
                    crate::test_support::FIXTURE_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .expect("answers");

    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "a correct password whose session could not be written must not come \
         back as `that name and password did not match` — the reader would \
         retype a password that was right the first time, forever"
    );
}

/// The other half: a **failure to record the failure** answers 429 rather than
/// 401, because an unrecordable failure is an unbounded one and the safe
/// reading of "the limiter is not working" is to stop.
///
/// Argued for in three separate comments across `login` and `from_basic`, and
/// asserted nowhere until mutation testing asked.
#[tokio::test]
async fn a_failure_the_limiter_cannot_record_is_answered_as_a_lockout() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-login-unrecordable-failure",
    ));
    {
        let conn = state.library.conn();
        conn.execute("DROP TABLE login_attempt", []).expect("drops");
    }

    let response = build_router(state)
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://test")
                // The real account with the wrong password, so this exercises
                // the failed-verify branch rather than the unknown-name one
                // beside it — both call `note_failure`, and naming an account
                // that does not exist would have tested the wrong arm while
                // passing for the right reason.
                .body(Body::from(format!(
                    r#"{{"name":"{}","password":"not the password"}}"#,
                    crate::test_support::FIXTURE_NAME
                )))
                .unwrap(),
        )
        .await
        .expect("answers");

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a wrong password the limiter could not count must stop rather than \
         continue: unrecorded failures are unlimited ones"
    );
}

/// And the same on the `Authorization: Basic` path, which reaches the identical
/// reasoning through `from_basic` rather than through `login`.
#[tokio::test]
async fn a_basic_failure_the_limiter_cannot_record_is_answered_as_a_lockout() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-basic-unrecordable",
    ));
    {
        let conn = state.library.conn();
        conn.execute("DROP TABLE login_attempt", []).expect("drops");
    }

    use base64::Engine as _;
    let wrong = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:not the password", crate::test_support::FIXTURE_NAME).as_bytes());
    let response = build_router(state)
        .oneshot(
            Request::get("/api/works")
                .header("host", "test")
                .header("authorization", format!("Basic {wrong}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "the guard's own comment calls this the control that bounds both the \
         dictionary attack and the denial of service; nothing checked it"
    );
}

/// **`admin_exists` and `unclaimed` disagree about what "claimed" means, and
/// the claim route asked the wrong one.**
///
/// `taiyaku library user add alice` is a real way to create the first account —
/// it inserts with role `user` — and it leaves the seeded row locked. So
/// `admin_exists` answers false (the only account with a password is not an
/// admin) while `unclaimed` still finds row 1. A stranger reaching
/// `POST /api/claim` then takes that row: administrator, plus every note and
/// reading position recorded before the library had a login, because §5 makes
/// promotion update row 1 rather than insert beside it.
///
/// Found by the fourth whole-branch review, 2026-08-19.
#[tokio::test]
async fn a_library_someone_has_already_set_up_cannot_be_claimed_by_a_stranger() {
    let state = unclaimed("taiyaku-test-claim-after-add", true);
    {
        let conn = state.library.conn();
        account::add(&conn, "alice", "alice's password", "user").expect("the owner sets up");
    }

    let response = build_router(state.clone())
        .oneshot(claim_request("attacker", "theirs"))
        .await
        .expect("answers");
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a library holding an account somebody can log in as has been set up, \
         whatever that account's role, and a stranger must not be able to take \
         the seeded row out from under it"
    );

    let conn = state.library.conn();
    assert!(
        account::by_name(&conn, "attacker")
            .expect("reads")
            .is_none(),
        "the claim went through and created an administrator"
    );
}

/// The fixture state, plus a second account, and a **cookie** for each.
///
/// Cookies rather than Basic headers since the account-management routes refuse
/// a device credential: an API key is what an OPDS client presents for pages,
/// and one typed into a third-party reader must not also mint keys or sign the
/// owner's browsers out. These tests are about ownership scoping, so they
/// authenticate the way the account screen does.
fn two_accounts(dir: &str) -> (Arc<ServeState>, String, String, i64) {
    let state = crate::test_support::state(dir);
    let bob_id = {
        let conn = state.library.conn();
        let phc = credential::hash_password("bobs-password").expect("hashes");
        conn.execute(
            "INSERT INTO user (name, password_hash, role) VALUES ('bob', ?1, 'user')",
            [&phc],
        )
        .expect("adds bob");
        conn.last_insert_rowid()
    };
    let cookie_for = |user_id: i64| {
        let token = credential::mint_token();
        let conn = state.library.conn();
        account::open_session(&conn, user_id, &token, None, None).expect("opens");
        format!("{COOKIE_NAME}={token}")
    };
    let alice = cookie_for(1);
    let bob = cookie_for(bob_id);
    (Arc::new(state), alice, bob, bob_id)
}

async fn body_of(response: axum::http::Response<Body>) -> String {
    String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

/// A key is created once, listed without its secret, and revoked.
#[tokio::test]
async fn keys_are_created_listed_and_revoked() {
    let (state, alice, _bob, _) = two_accounts("taiyaku-test-keys");

    let created = build_router(state.clone())
        .oneshot(
            Request::post("/api/keys")
                .header("cookie", &alice)
                .header("host", "test")
                .header("origin", "http://test")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"phone"}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(created.status(), StatusCode::OK);
    let secret = body_of(created).await;
    assert!(secret.contains("tyk_"), "{secret}");

    let listed = build_router(state.clone())
        .oneshot(
            Request::get("/api/keys")
                .header("cookie", &alice)
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    let text = body_of(listed).await;
    assert!(text.contains("phone"), "{text}");
    assert!(
        !text.contains("tyk_"),
        "the listing must never carry a secret — only its hash is stored, and \
         this is the route that would leak it if one were: {text}"
    );

    let id: i64 = {
        let conn = state.library.conn();
        // Scoped past the suite's own key, which `test_support::library` now
        // inserts so that every authenticated request costs sha256 rather than
        // argon2.
        conn.query_row("SELECT id FROM api_key WHERE name = 'phone'", [], |row| {
            row.get(0)
        })
        .expect("reads")
    };
    let revoked = build_router(state.clone())
        .oneshot(
            Request::delete(format!("/api/keys/{id}"))
                .header("cookie", &alice)
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let gone: i64 = {
        let conn = state.library.conn();
        conn.query_row(
            "SELECT COUNT(*) FROM api_key WHERE name = 'phone'",
            [],
            |row| row.get(0),
        )
        .expect("counts")
    };
    assert_eq!(gone, 0);
}

/// **One account cannot revoke another's key.** §9 ships no user
/// administration, so a route that took an id and revoked it would be one — and
/// a badly scoped one, since it would take anybody's.
#[tokio::test]
async fn a_key_id_from_another_account_is_not_revocable() {
    let (state, alice, bob, bob_id) = two_accounts("taiyaku-test-keys-scoped");
    let bobs_key = {
        let conn = state.library.conn();
        taiyaku_library::account::create_key(&conn, bob_id, "bobs-phone").expect("creates")
    };
    let id: i64 = {
        let conn = state.library.conn();
        conn.query_row(
            "SELECT id FROM api_key WHERE name = 'bobs-phone'",
            [],
            |row| row.get(0),
        )
        .expect("reads")
    };

    let refused = build_router(state.clone())
        .oneshot(
            Request::delete(format!("/api/keys/{id}"))
                .header("cookie", &alice)
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
    assert!(
        {
            let conn = state.library.conn();
            taiyaku_library::account::by_api_key(&conn, &bobs_key)
                .expect("reads")
                .is_some()
        },
        "alice revoked bob's key"
    );

    // And bob's own listing shows it, so the 404 above is about ownership
    // rather than about the key not existing.
    let listed = build_router(state)
        .oneshot(
            Request::get("/api/keys")
                .header("cookie", &bob)
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert!(body_of(listed).await.contains("bobs-phone"));
}

/// A key needs a name, because an account screen listing three keys called
/// nothing is unusable for deciding which to revoke.
#[tokio::test]
async fn a_key_without_a_name_is_refused() {
    let (state, alice, _, _) = two_accounts("taiyaku-test-keys-unnamed");
    let refused = build_router(state)
        .oneshot(
            Request::post("/api/keys")
                .header("cookie", &alice)
                .header("host", "test")
                .header("origin", "http://test")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"   "}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
}

/// Sessions list with the current one marked, and another can be revoked.
#[tokio::test]
async fn sessions_are_listed_with_this_one_marked_and_others_revocable() {
    let state = crate::test_support::state("taiyaku-test-sessions-list");
    let mine = credential::mint_token();
    let theirs = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, 1, &mine, Some("Firefox"), None).expect("opens");
        account::open_session(&conn, 1, &theirs, Some("Safari"), None).expect("opens");
    }
    let state = Arc::new(state);
    let cookie = format!("{COOKIE_NAME}={mine}");

    let listed = build_router(state.clone())
        .oneshot(
            Request::get("/api/sessions")
                .header("cookie", &cookie)
                .header("host", "test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    let text = body_of(listed).await;
    assert!(text.contains("Firefox"), "{text}");
    assert!(text.contains("Safari"), "{text}");
    assert!(text.contains("\"current\":true"), "{text}");
    assert!(
        !text.contains(&mine),
        "the raw token must not come back out of the listing: {text}"
    );

    let other_id = credential::token_sha256(&theirs);
    let revoked = build_router(state.clone())
        .oneshot(
            Request::delete(format!("/api/sessions/{other_id}"))
                .header("cookie", &cookie)
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let conn = state.library.conn();
    assert!(
        account::session_owner(&conn, &theirs)
            .expect("reads")
            .is_none()
    );
    assert!(
        account::session_owner(&conn, &mine)
            .expect("reads")
            .is_some()
    );
}

/// A session id from another account is not this one's to revoke.
#[tokio::test]
async fn a_session_id_from_another_account_is_not_revocable() {
    let (state, alice, _bob, bob_id) = two_accounts("taiyaku-test-sessions-scoped");
    let bobs = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, bob_id, &bobs, None, None).expect("opens");
    }

    let refused = build_router(state.clone())
        .oneshot(
            Request::delete(format!("/api/sessions/{}", credential::token_sha256(&bobs)))
                .header("cookie", &alice)
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);

    let conn = state.library.conn();
    assert!(
        account::session_owner(&conn, &bobs)
            .expect("reads")
            .is_some(),
        "alice ended bob's session"
    );
}

/// **A password change needs the current password even though the session
/// already proves who this is.** A session left open on a shared machine is
/// exactly the case: the cookie says "this browser was signed in", and only the
/// password says "the person here knows it".
#[tokio::test]
async fn changing_a_password_needs_the_current_one() {
    let state = crate::test_support::state("taiyaku-test-password-change");
    let mine = credential::mint_token();
    let elsewhere = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, 1, &mine, None, None).expect("opens");
        account::open_session(&conn, 1, &elsewhere, None, None).expect("opens");
    }
    let state = Arc::new(state);
    let cookie = format!("{COOKIE_NAME}={mine}");

    let change = |current: &'static str, next: &'static str| {
        let state = state.clone();
        let cookie = cookie.clone();
        async move {
            build_router(state)
                .oneshot(
                    Request::put("/api/password")
                        .header("cookie", cookie)
                        .header("host", "test")
                        .header("origin", "http://test")
                        .header("content-type", "application/json")
                        .body(Body::from(format!(
                            r#"{{"current":"{current}","next":"{next}"}}"#
                        )))
                        .unwrap(),
                )
                .await
                .expect("answers")
                .status()
        }
    };

    assert_eq!(
        change("not the password", "whatever").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        change(crate::test_support::FIXTURE_PASSWORD, "").await,
        StatusCode::BAD_REQUEST,
        "an empty password would lock the account rather than open it"
    );
    assert_eq!(
        change(crate::test_support::FIXTURE_PASSWORD, "a new one").await,
        StatusCode::NO_CONTENT
    );

    let conn = state.library.conn();
    let (_, phc) = account::by_name(&conn, crate::test_support::FIXTURE_NAME)
        .expect("reads")
        .expect("exists");
    assert!(credential::verify_password("a new one", &phc));
    // §4: the browser that made the change keeps its session; every other one
    // ends, so a compromised session cannot outlive the change meant to end it.
    assert!(
        account::session_owner(&conn, &mine)
            .expect("reads")
            .is_some()
    );
    assert!(
        account::session_owner(&conn, &elsewhere)
            .expect("reads")
            .is_none()
    );
}

/// The password change takes the origin check too — it is a state change on the
/// cookie path, which is what that rule is for.
#[tokio::test]
async fn a_cross_site_page_cannot_change_a_password() {
    let state = crate::test_support::state("taiyaku-test-password-csrf");
    let mine = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, 1, &mine, None, None).expect("opens");
    }
    let state = Arc::new(state);

    let refused = build_router(state)
        .oneshot(
            Request::put("/api/password")
                .header("cookie", format!("{COOKIE_NAME}={mine}"))
                .header("host", "test")
                .header("origin", "http://evil.test")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"current":"fixture","next":"theirs"}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    // The guard refuses the cookie for a cross-origin state change before the
    // handler is reached at all, which is the outer of the two checks — and
    // answers 403 rather than 401, so a reader whose proxy mangles `Host` is not
    // signed out by every write.
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
}

/// **An empty password would create the server's only administrator with no
/// password at all, unrecoverably.**
///
/// `credential::hash_password("")` produces a well-formed PHC string rather than
/// `LOCKED`, so `verify_password("", phc)` answers true and anybody can sign in.
/// And `admin_exists` counts `password_hash <> '!'`, so it then answers true and
/// the claim route 404s — the mistake cannot be undone through the web at all.
///
/// Both sibling paths already refuse this in the same words: `change_password`
/// and the CLI's `accept`. The one path that creates the *only* administrator
/// had neither. Found by the whole-branch review, 2026-08-18.
#[tokio::test]
async fn a_server_cannot_be_claimed_with_an_empty_password() {
    let state = unclaimed("taiyaku-test-claim-empty", true);

    let refused = build_router(state.clone())
        .oneshot(claim_request("davey", ""))
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    // And the server is still claimable, which is the half that makes this a
    // refusal rather than a different way to lose it.
    let conn = state.library.conn();
    assert!(
        !account::admin_exists(&conn).expect("reads"),
        "a refused claim must leave the server unclaimed"
    );
}

/// **The cookie's `Max-Age` was written once at login and never again**, so a
/// browser discarded it thirty days after sign-in however often it was used —
/// while the database row's `expires` slid forward on every request.
///
/// Three doc comments promise the sliding behaviour (`004_auth.sql`,
/// `SESSION_DAYS`, and §9's cost argument for refusing a `--no-auth` bypass) and
/// all three were true of the row and false of the cookie. The row then survived
/// unreachable until the sweep reaped it, so `GET /api/sessions` listed dead
/// browsers as signed in.
///
/// Found by the whole-branch review, 2026-08-18.
#[tokio::test]
async fn using_a_session_re_issues_the_cookie_that_carries_it() {
    let state = crate::test_support::state("taiyaku-test-session-reissue");
    let token = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, 1, &token, None, None).expect("opens");
    }
    let state = Arc::new(state);

    let response = build_router(state)
        .oneshot(
            Request::get("/api/works")
                .header("cookie", format!("{COOKIE_NAME}={token}"))
                .header("host", "test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);

    let set = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .expect("a request that slid the row forward must slide the cookie too");
    assert!(
        set.contains(&token),
        "the re-issued cookie is a different session"
    );
    assert!(set.contains("Max-Age=2592000"), "{set}");
    assert!(set.contains("HttpOnly"), "{set}");
}

/// A request that did not authenticate by cookie does not get one — an OPDS
/// client presenting an API key has no session to extend, and handing it a
/// `Set-Cookie` on every page of a volume would be noise at best.
#[tokio::test]
async fn a_header_authenticated_request_is_given_no_cookie() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-session-no-reissue",
    ));
    let response = build_router(state)
        .oneshot(
            Request::get("/api/works")
                .header(
                    "authorization",
                    crate::test_support::fixture_authorization(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("set-cookie").is_none());
}

/// **A request with no `Host` must not fail a *safe* method's cookie auth.**
///
/// `origin_is_acceptable` already returns true unconditionally for anything but
/// a state-changing method, so a GET never needed a host — but `host?` was
/// evaluated before it was asked, narrowing the rule from "a state change needs
/// a comparable authority" to "every request needs a `Host` header". hyper puts
/// the authority in the URI for HTTP/2 and synthesises no `Host`, so behind an
/// h2c upstream every request answered 401.
///
/// Found by the whole-branch review, 2026-08-18.
#[test]
fn a_safe_method_does_not_need_a_host_at_all() {
    assert!(origin_is_acceptable(&Method::GET, None, None));
    assert!(origin_is_acceptable(&Method::HEAD, None, None));
    // A state change still does, because there is nothing to compare against
    // and guessing resolves the ambiguity in the attacker's favour.
    assert!(!origin_is_acceptable(
        &Method::PUT,
        Some("http://example"),
        None
    ));
}

/// **The authority to compare against is the forwarded one, when a trusted
/// proxy said so.**
///
/// nginx's documented default is `proxy_set_header Host $proxy_host`, so without
/// an explicit override the server compared `Origin: https://taiyaku.example`
/// against `Host: 127.0.0.1:8080` and refused — every login 403 and every
/// cookie-authenticated write failing, in exactly the reverse-proxy deployment
/// §6 says this is for. The mechanism already existed two functions away:
/// `X-Forwarded-For` is honoured only from a configured `--trusted-proxy`.
///
/// Found by the whole-branch review, 2026-08-18.
#[test]
fn a_forwarded_host_is_honoured_only_from_a_trusted_peer() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("host", "127.0.0.1:8080".parse().unwrap());
    headers.insert("x-forwarded-host", "taiyaku.example".parse().unwrap());

    let mut extensions = axum::http::Extensions::new();
    extensions.insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
        [10, 0, 0, 1],
        1234,
    ))));

    let trusting = AuthConfig {
        secure_cookies: false,
        allow_web_setup: false,
        trusted_proxies: vec!["10.0.0.0/8".parse().expect("cidr")],
    };
    assert_eq!(
        effective_host(&trusting, &extensions, &headers).as_deref(),
        Some("taiyaku.example"),
        "a trusted proxy's X-Forwarded-Host was ignored, so every login 403s"
    );

    // And an untrusted peer cannot rename the server it is talking to — which
    // would let a page on any origin satisfy the check by claiming to be it.
    let bare = AuthConfig::default();
    assert_eq!(
        effective_host(&bare, &extensions, &headers).as_deref(),
        Some("127.0.0.1:8080")
    );
}

/// **A device credential cannot manage the account it belongs to.**
///
/// §2 justifies an API key's deliberately fast hash by its role as the thing an
/// OPDS client presents for every page of a volume. Until this, a key typed into
/// a third-party reader also minted further keys — so revoking the device did
/// not lock anyone out — listed the owner's browsers with their user-agents and
/// addresses, signed every one of them out, and changed the password.
///
/// Found by the second whole-branch review.
#[tokio::test]
async fn an_api_key_cannot_mint_keys_or_end_sessions() {
    use base64::Engine as _;

    let state = crate::test_support::state("taiyaku-test-key-cannot-manage");
    let secret = credential::mint_api_key();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
            [credential::token_sha256(&secret)],
        )
        .expect("inserts");
    }
    let state = Arc::new(state);
    let key_header = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("local:{secret}").as_bytes())
    );

    for (method, path, body) in [
        ("POST", "/api/keys", r#"{"name":"another"}"#),
        ("GET", "/api/keys", ""),
        ("GET", "/api/sessions", ""),
        (
            "PUT",
            "/api/password",
            r#"{"current":"fixture","next":"theirs"}"#,
        ),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", &key_header)
            .header("host", "test")
            .header("origin", "http://test")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = build_router(state.clone())
            .oneshot(request)
            .await
            .expect("answers");
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} was served to an API key"
        );
    }

    // And the key still reads pages, which is the half that would make this a
    // regression rather than a fix if it broke.
    let reading = build_router(state)
        .oneshot(
            Request::get("/opds/v1.2/catalog")
                .header("authorization", &key_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(reading.status(), StatusCode::OK);
}

/// A password change over `Authorization: Basic` is not refused for having no
/// `Origin`. The guard already ran the origin check on the cookie path, and a
/// header credential is not CSRF-able — re-applying it inside a guarded handler
/// refused every non-browser client with a message about sessions, while
/// `POST /api/keys` beside it answered normally.
///
/// Found by the second whole-branch review.
#[tokio::test]
async fn a_password_change_over_basic_needs_no_origin() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-password-basic"));

    let response = build_router(state)
        .oneshot(
            Request::put("/api/password")
                .header(
                    "authorization",
                    crate::test_support::fixture_authorization(),
                )
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"current":"{}","next":"a new one"}}"#,
                    crate::test_support::FIXTURE_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .expect("answers");
    // Refused as a device credential (403 with the key message), not as a
    // cross-site request — the distinction this test exists to pin is that the
    // *origin* is no longer the reason.
    let body = String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        !body.contains("Origin"),
        "a header credential was refused for having no Origin: {body}"
    );
}

/// **A password over Basic may manage the account; only a key may not.**
///
/// Folding both Basic paths into one `By::Header` refused a correct password
/// with a message asserting an API key had been used — a false statement about
/// which credential was presented, and a refusal of the strongest one the
/// account has. It left no non-browser path to mint a first key or change a
/// password at all. The test that was supposed to cover this presented the
/// fixture *key*, which `test_support` had quietly become.
///
/// Found by the third whole-branch review.
#[tokio::test]
async fn a_password_over_basic_can_manage_the_account() {
    use base64::Engine as _;

    let state = Arc::new(crate::test_support::state("taiyaku-test-password-manages"));
    let password_header = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(
            format!(
                "{}:{}",
                crate::test_support::FIXTURE_NAME,
                crate::test_support::FIXTURE_PASSWORD
            )
            .as_bytes()
        )
    );

    let listed = build_router(state.clone())
        .oneshot(
            Request::get("/api/keys")
                .header("authorization", &password_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(
        listed.status(),
        StatusCode::OK,
        "a correct password was refused from the account routes"
    );

    let created = build_router(state.clone())
        .oneshot(
            Request::post("/api/keys")
                .header("authorization", &password_header)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"a first key"}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(
        created.status(),
        StatusCode::OK,
        "there was no non-browser path to mint a first key"
    );

    // And the key that password just minted still cannot do the same.
    let secret = {
        let body = axum::body::to_bytes(created.into_body(), 1 << 16)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        text.split('"')
            .find(|part| part.starts_with("tyk_"))
            .expect("a secret")
            .to_string()
    };
    let key_header = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{secret}", crate::test_support::FIXTURE_NAME).as_bytes())
    );
    let refused = build_router(state)
        .oneshot(
            Request::get("/api/keys")
                .header("authorization", &key_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
}

/// **A misconfigured proxy must not sign a reader out on every write.**
///
/// `origin_is_acceptable` waves safe methods through, so the library loads
/// normally and the reader is signed in — then the first debounced note or
/// position write answered 401, which `api.ts` routes as "signed out",
/// discarding unsaved note text. Signing in worked, the cookie was always valid,
/// and the next write threw them out again.
///
/// Found by the third whole-branch review.
#[tokio::test]
async fn a_valid_session_is_not_signed_out_by_an_origin_mismatch() {
    let state = crate::test_support::state("taiyaku-test-origin-not-signout");
    let token = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, 1, &token, None, None).expect("opens");
    }
    let state = Arc::new(state);
    let cookie = format!("{COOKIE_NAME}={token}");

    // The read succeeds, which is what makes the write's answer matter: the
    // reader is signed in and looking at their library.
    let read = build_router(state.clone())
        .oneshot(
            Request::get("/api/works")
                .header("cookie", &cookie)
                .header("host", "127.0.0.1:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(read.status(), StatusCode::OK);

    // nginx's default rewrites `Host` to the upstream while `Origin` stays what
    // the browser asked for.
    let write = build_router(state)
        .oneshot(
            Request::put("/api/works/t/progress")
                .header("cookie", &cookie)
                .header("host", "127.0.0.1:8080")
                .header("origin", "https://taiyaku.example")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":1}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(
        write.status(),
        StatusCode::FORBIDDEN,
        "a 401 here is routed as `signed out` and discards the reader's unsaved work"
    );
    let body = String::from_utf8(
        axum::body::to_bytes(write.into_body(), 1 << 16)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        body.contains("--trusted-proxy") && body.contains("Host"),
        "the refusal must name what the operator has to change: {body}"
    );
}

/// **§7 applies to the password change too**, and did not: it verified with
/// argon2 and counted nothing, so the one secret standing between a borrowed
/// session and permanent account takeover took unlimited guesses — from the very
/// unattended laptop this route's own doc says the current-password requirement
/// exists for. It was an authenticated denial of service as well, at 11.9 ms and
/// 19 MiB a guess.
///
/// Found by the third whole-branch review.
#[tokio::test]
async fn guessing_the_current_password_is_rate_limited() {
    let state = crate::test_support::state("taiyaku-test-change-lockout");
    let token = credential::mint_token();
    {
        let conn = state.library.conn();
        account::open_session(&conn, 1, &token, None, None).expect("opens");
    }
    let state = Arc::new(state);
    let cookie = format!("{COOKIE_NAME}={token}");

    let attempt = |current: &'static str| {
        let state = state.clone();
        let cookie = cookie.clone();
        async move {
            build_router(state)
                .oneshot(
                    Request::put("/api/password")
                        .header("cookie", cookie)
                        .header("host", "test")
                        .header("origin", "http://test")
                        .header("content-type", "application/json")
                        .body(Body::from(format!(
                            r#"{{"current":"{current}","next":"a new one"}}"#
                        )))
                        .unwrap(),
                )
                .await
                .expect("answers")
                .status()
        }
    };

    for _ in 0..taiyaku_library::account::LOCKOUT_AFTER {
        assert_eq!(attempt("wrong").await, StatusCode::FORBIDDEN);
    }
    assert_eq!(
        attempt("wrong").await,
        StatusCode::TOO_MANY_REQUESTS,
        "the current password was guessable without limit"
    );
    assert_eq!(
        attempt(crate::test_support::FIXTURE_PASSWORD).await,
        StatusCode::TOO_MANY_REQUESTS,
        "a locked-out caller must not be let through by finally guessing right"
    );
}
