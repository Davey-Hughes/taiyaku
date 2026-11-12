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
    let tmp = crate::test_support::Fixture::new("session-round-trip");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    let tmp = crate::test_support::Fixture::new("session-bad-login");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    let tmp = crate::test_support::Fixture::new("session-csrf");
    let state = crate::test_support::state(&tmp);
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
    // a 401 here would sign the reader out and discard their unsaved notes.
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
    let tmp = crate::test_support::Fixture::new("session-fallthrough");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    let tmp = crate::test_support::Fixture::new("session-me");
    let state = Arc::new(crate::test_support::state(&tmp));

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
/// `login` and `logout` sit outside the guard, so they apply the origin check
/// §4 gives every other state-changing request themselves. Two accidents of the
/// browser stand in the way as well: an HTML form cannot send
/// `application/json`, and `fetch` with that content type — or with `DELETE` —
/// triggers a CORS preflight this server never answers. Both are real, and
/// neither is this server's doing: adding a permissive CORS layer, or an
/// extractor that accepts a form encoding, would open it with nothing here
/// changing to say so.
#[tokio::test]
async fn a_cross_site_page_cannot_log_a_reader_in_or_out() {
    let tmp = crate::test_support::Fixture::new("session-login-csrf");
    let state = Arc::new(crate::test_support::state(&tmp));
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

    // And the reader's own origin still works.
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
    let tmp = crate::test_support::Fixture::new("session-403-not-401");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    let tmp = crate::test_support::Fixture::new("session-lockout");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    let tmp = crate::test_support::Fixture::new("session-forgets");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    let tmp = crate::test_support::Fixture::new("session-name-guess");
    let state = Arc::new(crate::test_support::state(&tmp));

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

/// **A new name on every request does not buy unlimited hashing.**
///
/// `guessing_at_names_is_rate_limited_as_well` above states the pair key's gap
/// and calls it a deliberate §7 trade. `credential::DECOY_HASH` makes an
/// unknown name cost what a real one costs, to close a timing oracle, so with
/// the pair key alone every name nobody has ever used would cost an argon2
/// verify and leave a `login_attempt` row behind, with no counter it could ever
/// reach: an unauthenticated denial of service. The wider key — the address
/// alone, at `ADDRESS_LOCKOUT_AFTER` — is what bounds it, and it is checked
/// before the decoy runs.
#[tokio::test]
async fn inventing_a_new_name_every_time_does_not_buy_unlimited_hashing() {
    use taiyaku_library::account::ADDRESS_LOCKOUT_AFTER;

    let tmp = crate::test_support::Fixture::new("session-name-flood");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    // cost this ceiling exists to bound. Three requests prove what is actually
    // in question: that a fresh name is still counted, and that the gate fires
    // when the count arrives.
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
    // of what an unmetered key would cost: one `login_attempt` row per invented
    // name, swept only at startup.
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
fn unclaimed(dir: &std::path::Path, allow_web_setup: bool) -> Arc<ServeState> {
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
    let tmp = crate::test_support::Fixture::new("claim-off");
    let off = unclaimed(&tmp, false);
    let response = build_router(off)
        .oneshot(claim_request("davey", "a password"))
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Flag on, no admin: claimed, and signed in by the same response.
    let tmp2 = crate::test_support::Fixture::new("claim-on");
    let on = unclaimed(&tmp2, true);
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
    let tmp = crate::test_support::Fixture::new("claim-promotes");
    let state = unclaimed(&tmp, true);
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
    let tmp = crate::test_support::Fixture::new("claim-csrf");
    let state = unclaimed(&tmp, true);
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

/// **The fail-closed claims on the login path.**
///
/// Each is a documented decision that the code comments argue for at length.
///
/// The 401 is the one answer that cannot be true in this case: the password was
/// right and the session write lost to a busy writer, so telling the client
/// "that name and password did not match" leaves a reader retyping a correct
/// password forever.
///
/// Reachable by taking the tables away, which is the same error class the
/// comments reason about — `serve` and `library import` share one file, so a
/// write can lose to a busy writer — reproduced deterministically rather than by
/// racing anything.
#[tokio::test]
async fn a_login_that_cannot_be_recorded_is_not_answered_as_a_wrong_password() {
    let tmp = crate::test_support::Fixture::new("login-unrecordable");
    let state = Arc::new(crate::test_support::state(&tmp));
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
/// Argued for in comments across `login` and `from_basic`.
#[tokio::test]
async fn a_failure_the_limiter_cannot_record_is_answered_as_a_lockout() {
    let tmp = crate::test_support::Fixture::new("login-unrecordable-failure");
    let state = Arc::new(crate::test_support::state(&tmp));
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
    let tmp = crate::test_support::Fixture::new("basic-unrecordable");
    let state = Arc::new(crate::test_support::state(&tmp));
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
/// the claim route must not ask the wrong one.**
///
/// `taiyaku library user add alice` is a real way to create the first account —
/// it inserts with role `user` — and it leaves the seeded row locked. So
/// `admin_exists` answers false (the only account with a password is not an
/// admin) while `unclaimed` still finds row 1. A claim route gated on
/// `admin_exists` would let a stranger reaching `POST /api/claim` take that
/// row: administrator, plus every note and reading position recorded before the
/// library had a login, because §5 makes promotion update row 1 rather than
/// insert beside it.
#[tokio::test]
async fn a_library_someone_has_already_set_up_cannot_be_claimed_by_a_stranger() {
    let tmp = crate::test_support::Fixture::new("claim-after-add");
    let state = unclaimed(&tmp, true);
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
fn two_accounts(dir: &std::path::Path) -> (Arc<ServeState>, String, String, i64) {
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
    let tmp = crate::test_support::Fixture::new("keys");
    let (state, alice, _bob, _) = two_accounts(&tmp);

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
        // Scoped past the suite's own key, which `test_support::library`
        // inserts so that every authenticated request costs sha256 rather
        // than argon2.
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

/// **One account cannot revoke another's key.** A route that took an id and
/// revoked it whoever owned it would take anybody's.
#[tokio::test]
async fn a_key_id_from_another_account_is_not_revocable() {
    let tmp = crate::test_support::Fixture::new("keys-scoped");
    let (state, alice, bob, bob_id) = two_accounts(&tmp);
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
    let tmp = crate::test_support::Fixture::new("keys-unnamed");
    let (state, alice, _, _) = two_accounts(&tmp);
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
    let tmp = crate::test_support::Fixture::new("sessions-list");
    let state = crate::test_support::state(&tmp);
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
    let tmp = crate::test_support::Fixture::new("sessions-scoped");
    let (state, alice, _bob, bob_id) = two_accounts(&tmp);
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
    let tmp = crate::test_support::Fixture::new("password-change");
    let state = crate::test_support::state(&tmp);
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
        }
    };

    assert_eq!(
        change("not the password", "whatever").await.status(),
        StatusCode::FORBIDDEN
    );
    let empty = change(crate::test_support::FIXTURE_PASSWORD, "").await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
    let why = crate::test_support::body_string(empty).await;
    assert!(
        why.contains("presses Enter"),
        "an empty password is refused because anybody could sign in with it: {why}"
    );
    assert_eq!(
        change(crate::test_support::FIXTURE_PASSWORD, "a new one")
            .await
            .status(),
        StatusCode::NO_CONTENT
    );

    let conn = state.library.conn();
    let (_, phc) = account::by_name(&conn, crate::test_support::FIXTURE_NAME)
        .expect("reads")
        .expect("exists");
    assert!(credential::verify_password_in(
        &mut WorkingMemory::new(),
        "a new one",
        &phc
    ));
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
    let tmp = crate::test_support::Fixture::new("password-csrf");
    let state = crate::test_support::state(&tmp);
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

/// **An empty password is refused with 400, and the server stays claimable.**
#[tokio::test]
async fn a_server_cannot_be_claimed_with_an_empty_password() {
    let tmp = crate::test_support::Fixture::new("claim-empty");
    let state = unclaimed(&tmp, true);

    let refused = build_router(state.clone())
        .oneshot(claim_request("davey", ""))
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value =
        serde_json::from_str(&crate::test_support::body_string(refused).await).expect("JSON");
    let why = body["error"].as_str().expect("an error in words");
    assert!(
        why.contains("presses Enter") && why.contains("administrator"),
        "an empty password is refused because anybody could then sign in as the administrator: {why}"
    );

    // And the server is still claimable, which is the half that makes this a
    // refusal rather than a different way to lose it.
    let conn = state.library.conn();
    assert!(
        !account::admin_exists(&conn).expect("reads"),
        "a refused claim must leave the server unclaimed"
    );
}

/// No page address holds `.` or `..`, so the claim refuses either name as the
/// caller's mistake, in the sentence the pages draw, and not as the 404 a
/// claimed server answers.
#[tokio::test]
async fn a_server_cannot_be_claimed_under_a_dot_name() {
    let tmp = crate::test_support::Fixture::new("claim-dot-name");
    let state = unclaimed(&tmp, true);

    for name in [".", ".."] {
        let refused = build_router(state.clone())
            .oneshot(claim_request(name, "a password"))
            .await
            .expect("answers");
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{name:?}");
        let body: serde_json::Value =
            serde_json::from_str(&crate::test_support::body_string(refused).await).expect("JSON");
        assert_eq!(
            body,
            serde_json::json!({ "error": "An address cannot hold “.” or “..” as a name." }),
            "{name:?}"
        );
    }
    let conn = state.library.conn();
    assert!(
        !account::is_claimed(&conn).expect("reads"),
        "a refused claim must leave the server unclaimed"
    );
}

/// An empty name has no page address either, so the claim refuses it as the
/// caller's mistake, as it does `.` and `..`.
#[tokio::test]
async fn a_server_cannot_be_claimed_under_an_empty_name() {
    let tmp = crate::test_support::Fixture::new("claim-empty-name");
    let state = unclaimed(&tmp, true);

    let refused = build_router(state.clone())
        .oneshot(claim_request("", "a password"))
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value =
        serde_json::from_str(&crate::test_support::body_string(refused).await).expect("JSON");
    assert_eq!(
        body,
        serde_json::json!({ "error": "an account needs a name, since its page is at its name" })
    );
    let conn = state.library.conn();
    assert!(
        !account::is_claimed(&conn).expect("reads"),
        "a refused claim must leave the server unclaimed"
    );
}

/// A store fault while claiming is answered as a claimed server is: the
/// caller has not signed in, and the store's own words, which name its
/// accounts and its tables, are not for them.
#[tokio::test]
async fn a_store_fault_while_claiming_is_answered_as_shut_without_the_stores_words() {
    let tmp = crate::test_support::Fixture::new("claim-store-fault");
    let state = unclaimed(&tmp, true);
    state
        .library
        .conn()
        .execute_batch(
            "CREATE TRIGGER a_claim_fault BEFORE UPDATE ON user
             BEGIN SELECT RAISE(ABORT, 'a planted store fault'); END;",
        )
        .expect("plants the fault");

    let refused = build_router(state.clone())
        .oneshot(claim_request("davey", "a password"))
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        crate::test_support::body_string(refused).await,
        crate::test_support::body_string(shut()).await,
        "the answer of a server that cannot be claimed, and nothing else"
    );
    let conn = state.library.conn();
    assert!(
        !account::is_claimed(&conn).expect("reads"),
        "the fault left the server unclaimed"
    );
}

/// **Using a session re-issues the cookie that carries it**, so the cookie's
/// `Max-Age` slides forward with the database row's `expires` rather than
/// ending thirty days after sign-in however often it was used. Three doc
/// comments promise the sliding behaviour (`004_auth.sql`, `SESSION_DAYS`, and
/// §9's cost argument for refusing a `--no-auth` bypass).
#[tokio::test]
async fn using_a_session_re_issues_the_cookie_that_carries_it() {
    let tmp = crate::test_support::Fixture::new("session-reissue");
    let state = crate::test_support::state(&tmp);
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
    let tmp = crate::test_support::Fixture::new("session-no-reissue");
    let state = Arc::new(crate::test_support::state(&tmp));
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
/// `origin_is_acceptable` returns true unconditionally for anything but a
/// state-changing method, so a GET never needs a host. hyper puts the authority
/// in the URI for HTTP/2 and synthesises no `Host`, so behind an h2c upstream
/// a request arrives with none.
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
/// an explicit override the server would compare `Origin:
/// https://taiyaku.example` against `Host: 127.0.0.1:8080` and refuse — every
/// login 403 and every cookie-authenticated write failing, in exactly the
/// reverse-proxy deployment §6 says this is for. `X-Forwarded-For` is honoured
/// only from a configured `--trusted-proxy`, and so is this.
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
        verifies: VerifyLimit::default(),
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
/// OPDS client presents for every page of a volume. A key typed into a
/// third-party reader would otherwise also mint further keys — so revoking the
/// device would not lock anyone out — list the owner's browsers with their
/// user-agents and addresses, sign every one of them out, and change
/// the password.
#[tokio::test]
async fn an_api_key_cannot_mint_keys_or_end_sessions() {
    use base64::Engine as _;

    let tmp = crate::test_support::Fixture::new("key-cannot-manage");
    let state = crate::test_support::state(&tmp);
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

    // And the key still reads pages.
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
/// `Origin`. The guard already runs the origin check on the cookie path, and a
/// header credential is not CSRF-able.
#[tokio::test]
async fn a_password_change_over_basic_needs_no_origin() {
    let tmp = crate::test_support::Fixture::new("password-basic");
    let state = Arc::new(crate::test_support::state(&tmp));

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
    // *origin* is not the reason.
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
/// Refusing a correct password with a message asserting an API key had been
/// used would be a false statement about which credential was presented, and a
/// refusal of the strongest one the account has. It would leave no non-browser
/// path to mint a first key or change a password at all.
#[tokio::test]
async fn a_password_over_basic_can_manage_the_account() {
    use base64::Engine as _;

    let tmp = crate::test_support::Fixture::new("password-manages");
    let state = Arc::new(crate::test_support::state(&tmp));
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
/// normally and the reader is signed in. A 401 on the first debounced note or
/// position write would be routed by `api.ts` as "signed out", discarding
/// unsaved note text although the cookie is valid.
#[tokio::test]
async fn a_valid_session_is_not_signed_out_by_an_origin_mismatch() {
    let tmp = crate::test_support::Fixture::new("origin-not-signout");
    let state = crate::test_support::state(&tmp);
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

/// **§7 applies to the password change too**: without a counter, the one
/// secret standing between a borrowed session and permanent account takeover
/// would take unlimited guesses — from the very session left open on a shared
/// machine that this route's own doc says the current-password requirement
/// exists for. It would be an authenticated denial of service as well, since
/// each guess costs an argon2 verify.
#[tokio::test]
async fn guessing_the_current_password_is_rate_limited() {
    let tmp = crate::test_support::Fixture::new("change-lockout");
    let state = crate::test_support::state(&tmp);
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

/// The address every `oneshot` request is held against: `auth::client_of`'s
/// stated placeholder for a request that carries no peer.
const NO_PEER: &str = "0.0.0.0";

/// How long a test waits for something a correct server does not let happen.
///
/// **No correct run needs it to be any particular length**: what a test reads
/// after the wait is the same on a machine of any speed. The wait is spent in
/// two ways.
///
/// **A test that has sent requests leaves them this long before it acts**,
/// and there only a wrong implementation makes use of it. A request waiting
/// for a permit this test holds is still waiting however long this is. A
/// request that should have waited and did not needs time to get past the
/// point it should have stopped at, and this is that time.
///
/// **The probe in [`told_and_free_at_each_commit`] is given this long to take
/// a permit**, at each commit. At a commit the request makes while it holds
/// its permit, a correct run sits out all of it: the permit the probe is
/// waiting for is the request's own. A wrong implementation has given that
/// permit back by then, and is found at once.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(300);

/// How long any of these tests waits for its answers before it fails.
const LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// How many requests a burst sends.
const BURST: usize = 12;

// More than there are permits, or no implementation could exceed the bound
// the burst tests assert.
const _: () = assert!(BURST > MAX_CONCURRENT_VERIFIES);

/// Puts `key`'s failures at `count`, inside the current window.
fn wind(state: &ServeState, key: &str, count: i64) {
    let conn = state.library.conn();
    account::note_failure(&conn, key).expect("records a failure to wind from");
    conn.execute(
        "UPDATE login_attempt SET count = ?2 WHERE key = ?1",
        rusqlite::params![key, count],
    )
    .expect("winds the counter");
}

/// Sends every request at once, each on a task of its own.
fn send_all(
    state: &Arc<ServeState>,
    requests: Vec<Request<Body>>,
) -> Vec<tokio::task::JoinHandle<StatusCode>> {
    requests
        .into_iter()
        .map(|request| {
            let router = build_router(state.clone());
            tokio::spawn(async move { router.oneshot(request).await.expect("answers").status() })
        })
        .collect()
}

/// What each of `sent` was answered, in the order they were sent, or `None`
/// if they had not all answered within [`LIMIT`].
async fn answers(sent: Vec<tokio::task::JoinHandle<StatusCode>>) -> Option<Vec<StatusCode>> {
    tokio::time::timeout(LIMIT, async {
        let mut statuses = Vec::with_capacity(sent.len());
        for handle in sent {
            statuses.push(handle.await.expect("the request did not panic"));
        }
        statuses
    })
    .await
    .ok()
}

/// Sends `request` while this test holds every permit, raises `key` to
/// `ceiling` while the request waits, frees the permits, and answers what the
/// request was told.
///
/// `key` is under its ceiling when the request arrives, so the request passes
/// the read it makes before it waits. The read it makes once it holds a
/// permit comes after the raise, and refuses it as locked out. A request
/// that does not ask again, or never waited, is verified instead.
async fn told_after_waiting(
    state: &Arc<ServeState>,
    request: Request<Body>,
    key: &str,
    ceiling: i64,
) -> StatusCode {
    let held = state.auth.verifies.hold_all();
    let sent = send_all(state, vec![request]);
    tokio::time::sleep(SETTLE).await;
    wind(state, key, ceiling);
    drop(held);
    answers(sent)
        .await
        .expect("the request answers once the permits are free")[0]
}

/// Sends `request` while this test holds every permit, and answers what it
/// was told while they were still held, or `None` if it waited for one.
async fn told_while_every_permit_is_held(
    state: &Arc<ServeState>,
    request: Request<Body>,
) -> Option<StatusCode> {
    let held = state.auth.verifies.hold_all();
    let sent = send_all(state, vec![request]);
    let answered = answers(sent).await;
    // Before the caller asserts, so a request this has shown to be waiting is
    // let go and the runtime can shut down.
    drop(held);
    answered.map(|statuses| statuses[0])
}

/// Sends `requests` while this test holds every permit, so that none of them
/// is verified before the permits are freed. Then frees the permits and
/// answers what each was told.
///
/// [`SETTLE`] is how long the requests are given to reach the point they
/// wait at; nothing asserts that they did.
async fn burst(state: &Arc<ServeState>, requests: Vec<Request<Body>>) -> Vec<StatusCode> {
    let held = state.auth.verifies.hold_all();
    let sent = send_all(state, requests);
    tokio::time::sleep(SETTLE).await;
    drop(held);
    answers(sent)
        .await
        .expect("every request answers once the permits are free")
}

/// What a burst may come to against a counter that stood one failure under
/// its ceiling.
///
/// Of the reads made under a permit, the ones that come before the first
/// failure lands are verified and answered `refused`, and every later one is
/// locked out. A permit is held from that read until the failure is recorded, so
/// the first group all hold permits at one moment: it is at most
/// `MAX_CONCURRENT_VERIFIES`, whatever the scheduling.
fn assert_one_allowance(statuses: &[StatusCode], refused: StatusCode) {
    let verified = statuses.iter().filter(|status| **status == refused).count();
    let locked = statuses
        .iter()
        .filter(|status| **status == StatusCode::TOO_MANY_REQUESTS)
        .count();
    assert_eq!(
        verified + locked,
        statuses.len(),
        "an answer that is neither a refusal nor a lockout: {statuses:?}"
    );
    assert!(
        verified >= 1,
        "the last of the allowance was not spendable: {statuses:?}"
    );
    assert!(
        verified <= MAX_CONCURRENT_VERIFIES,
        "{verified} of {} requests were verified against a counter one failure under its \
         ceiling. Only a request holding one of the {MAX_CONCURRENT_VERIFIES} permits can \
         read that counter before the first failure lands: {statuses:?}",
        statuses.len()
    );
}

/// A login as `name` with `password`.
fn login_as(name: &str, password: &str) -> Request<Body> {
    Request::post("/api/session")
        .header("content-type", "application/json")
        .header("host", "test")
        .header("origin", "http://test")
        .body(Body::from(format!(
            r#"{{"name":"{name}","password":"{password}"}}"#
        )))
        .unwrap()
}

/// A login the server must refuse: a password nobody holds.
fn wrong_login(name: &str) -> Request<Body> {
    login_as(name, "not the password")
}

/// **O57: a login that waited for a permit reads the lockout again once it
/// has one.**
///
/// Requests that arrive together all read a count under the ceiling, so without
/// a permit every one of them would be verified. The permit closes that only if
/// the lockout is asked again behind it: this login passes the first read,
/// waits for a permit while its address is locked out, and must be told so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_that_waited_for_a_permit_reads_the_lockout_again() {
    let tmp = crate::test_support::Fixture::new("verify-permit-login");
    let state = Arc::new(crate::test_support::state(&tmp));

    let told = told_after_waiting(
        &state,
        wrong_login("nobody-at-all"),
        &account::address_key(NO_PEER),
        account::ADDRESS_LOCKOUT_AFTER,
    )
    .await;

    assert_eq!(
        told,
        StatusCode::TOO_MANY_REQUESTS,
        "the address was locked out while this login waited for a permit, and it was \
         verified all the same: it did not ask again once it held one, or never waited"
    );
}

/// **A login already locked out is refused at once.** It does not wait for a
/// permit to be told so.
///
/// The second read alone would refuse it too, after a wait. That wait is a
/// place in the queue for a permit, taken by a caller the server has already
/// decided to refuse, and a flood from a locked-out address is exactly the
/// caller that would fill it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_out_login_is_refused_while_every_permit_is_held() {
    let tmp = crate::test_support::Fixture::new("verify-permit-login-locked");
    let state = Arc::new(crate::test_support::state(&tmp));
    wind(
        &state,
        &account::address_key(NO_PEER),
        account::ADDRESS_LOCKOUT_AFTER,
    );

    let told = told_while_every_permit_is_held(&state, wrong_login("nobody-at-all")).await;

    assert_eq!(
        told,
        Some(StatusCode::TOO_MANY_REQUESTS),
        "a login from an address already locked out waited for a verify permit before \
         it was refused"
    );
}

/// A burst of names nobody holds, against an address one failure under its
/// ceiling: the decoy verify, and the wide key that bounds it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_of_invented_names_spends_one_allowance_at_the_login_route() {
    let tmp = crate::test_support::Fixture::new("verify-burst-names");
    let state = Arc::new(crate::test_support::state(&tmp));
    wind(
        &state,
        &account::address_key(NO_PEER),
        account::ADDRESS_LOCKOUT_AFTER - 1,
    );

    let requests = (0..BURST)
        .map(|n| wrong_login(&format!("nobody-{n}")))
        .collect();
    let statuses = burst(&state, requests).await;

    assert_one_allowance(&statuses, StatusCode::UNAUTHORIZED);
}

/// The same burst at one real account, one failure under the pair's ceiling:
/// the real verify.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_at_one_name_spends_one_allowance_at_the_login_route() {
    let tmp = crate::test_support::Fixture::new("verify-burst-one-name");
    let state = Arc::new(crate::test_support::state(&tmp));
    wind(
        &state,
        &account::attempt_key(NO_PEER, crate::test_support::FIXTURE_NAME),
        account::LOCKOUT_AFTER - 1,
    );

    let requests = (0..BURST)
        .map(|_| wrong_login(crate::test_support::FIXTURE_NAME))
        .collect();
    let statuses = burst(&state, requests).await;

    assert_one_allowance(&statuses, StatusCode::UNAUTHORIZED);
}

/// A guarded request carrying `password` over `Authorization: Basic`.
fn basic_as(name: &str, password: &str) -> Request<Body> {
    use base64::Engine as _;
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("{name}:{password}").as_bytes());
    Request::get("/api/works")
        .header("host", "test")
        .header("authorization", format!("Basic {credentials}"))
        .body(Body::empty())
        .unwrap()
}

/// A guarded request carrying a wrong password over `Authorization: Basic`.
fn wrong_basic(name: &str) -> Request<Body> {
    basic_as(name, "not the password")
}

/// A password change behind `cookie` giving `current` as the current password.
fn change_with(cookie: &str, current: &str) -> Request<Body> {
    Request::put("/api/password")
        .header("cookie", cookie)
        .header("host", "test")
        .header("origin", "http://test")
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"current":"{current}","next":"a new one"}}"#
        )))
        .unwrap()
}

/// A password change behind `cookie` whose current password is wrong.
fn wrong_change(cookie: &str) -> Request<Body> {
    change_with(cookie, "not the password")
}

/// The key `change_password` counts account 1's wrong guesses under.
fn change_key() -> String {
    account::attempt_key(NO_PEER, "  change-password 1")
}

/// `a_login_that_waited_for_a_permit_reads_the_lockout_again`, on the
/// `Authorization: Basic` path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_basic_request_that_waited_for_a_permit_reads_the_lockout_again() {
    let tmp = crate::test_support::Fixture::new("verify-permit-basic");
    let state = Arc::new(crate::test_support::state(&tmp));

    let told = told_after_waiting(
        &state,
        wrong_basic(crate::test_support::FIXTURE_NAME),
        &account::attempt_key(NO_PEER, crate::test_support::FIXTURE_NAME),
        account::LOCKOUT_AFTER,
    )
    .await;

    assert_eq!(
        told,
        StatusCode::TOO_MANY_REQUESTS,
        "the pair was locked out while this request waited for a permit, and its \
         password was verified all the same"
    );
}

/// The same on the password change, whose wrong guess answers 403.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_password_change_that_waited_for_a_permit_reads_the_lockout_again() {
    let tmp = crate::test_support::Fixture::new("verify-permit-change");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);

    let told = told_after_waiting(
        &state,
        wrong_change(&cookie),
        &change_key(),
        account::LOCKOUT_AFTER,
    )
    .await;

    assert_eq!(
        told,
        StatusCode::TOO_MANY_REQUESTS,
        "the account's guesses were spent while this change waited for a permit, and \
         its current password was verified all the same"
    );
}

/// `a_locked_out_login_is_refused_while_every_permit_is_held`, on the
/// `Authorization: Basic` path. This is the path a flood most easily takes:
/// it is open on every guarded route.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_out_basic_request_is_refused_while_every_permit_is_held() {
    let tmp = crate::test_support::Fixture::new("verify-permit-basic-locked");
    let state = Arc::new(crate::test_support::state(&tmp));
    wind(
        &state,
        &account::attempt_key(NO_PEER, crate::test_support::FIXTURE_NAME),
        account::LOCKOUT_AFTER,
    );

    let told =
        told_while_every_permit_is_held(&state, wrong_basic(crate::test_support::FIXTURE_NAME))
            .await;

    assert_eq!(
        told,
        Some(StatusCode::TOO_MANY_REQUESTS),
        "a Basic request from a pair already locked out waited for a verify permit \
         before it was refused"
    );
}

/// The same on the password change.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_out_password_change_is_refused_while_every_permit_is_held() {
    let tmp = crate::test_support::Fixture::new("verify-permit-change-locked");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);
    wind(&state, &change_key(), account::LOCKOUT_AFTER);

    let told = told_while_every_permit_is_held(&state, wrong_change(&cookie)).await;

    assert_eq!(
        told,
        Some(StatusCode::TOO_MANY_REQUESTS),
        "a password change from an account whose guesses were spent waited for a \
         verify permit before it was refused"
    );
}

/// A burst of wrong passwords for one account over Basic, one failure under
/// the pair's ceiling.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_over_basic_spends_one_allowance() {
    let tmp = crate::test_support::Fixture::new("verify-burst-basic");
    let state = Arc::new(crate::test_support::state(&tmp));
    wind(
        &state,
        &account::attempt_key(NO_PEER, crate::test_support::FIXTURE_NAME),
        account::LOCKOUT_AFTER - 1,
    );

    let requests = (0..BURST)
        .map(|_| wrong_basic(crate::test_support::FIXTURE_NAME))
        .collect();
    let statuses = burst(&state, requests).await;

    assert_one_allowance(&statuses, StatusCode::UNAUTHORIZED);
}

/// A burst of password changes from one signed-in account, one failure under
/// its ceiling.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_of_password_changes_spends_one_allowance() {
    let tmp = crate::test_support::Fixture::new("verify-burst-change");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);
    wind(&state, &change_key(), account::LOCKOUT_AFTER - 1);

    let requests = (0..BURST).map(|_| wrong_change(&cookie)).collect();
    let statuses = burst(&state, requests).await;

    assert_one_allowance(&statuses, StatusCode::FORBIDDEN);
}

/// **Only a password verify waits for a permit.** With every permit held, a
/// request by API key, a request by session cookie, and an administrator
/// adding an account are all served.
///
/// The key and the cookie are what a reader presents on every page, and
/// neither waits for a permit. The account is the lock order: the route
/// hashes its new password while it holds the library's connection, so if it
/// also waited for a permit it would be waiting on the verifying paths while
/// they wait on it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_password_verify_waits_for_a_permit() {
    let tmp = crate::test_support::Fixture::new("verify-permit-others");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);

    let by_key = Request::get("/api/works")
        .header("host", "test")
        .header(
            "authorization",
            crate::test_support::fixture_authorization(),
        )
        .body(Body::empty())
        .unwrap();
    let by_cookie = Request::get("/api/works")
        .header("host", "test")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let adding_an_account = Request::post("/api/library/users")
        .header("cookie", &cookie)
        .header("host", "test")
        .header("origin", "http://test")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"reader","password":"hunter2"}"#))
        .unwrap();

    let held = state.auth.verifies.hold_all();
    let sent = send_all(&state, vec![by_key, by_cookie, adding_an_account]);
    let answered = answers(sent).await;
    // Before the assertion, so a request this test has shown to be waiting is
    // let go and the runtime can shut down.
    drop(held);

    assert_eq!(
        answered.expect(
            "a request that verifies no password waited for a verify permit: by key, by \
             cookie, or the route that hashes a new account's password"
        ),
        [StatusCode::OK, StatusCode::OK, StatusCode::OK]
    );
}

/// **The order is the permit, then the library's connection.** A login, a
/// Basic request and a password change all wait for a permit, and while they
/// wait the connection is free for anyone.
///
/// The other order is a server that stops: a request holding the one
/// connection while it waits for a permit keeps every permit holder from the
/// read and the write it holds its permit for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_waiting_for_a_permit_does_not_hold_the_librarys_connection() {
    let tmp = crate::test_support::Fixture::new("verify-permit-order");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);

    let held = state.auth.verifies.hold_all();
    let sent = send_all(
        &state,
        vec![
            wrong_login(crate::test_support::FIXTURE_NAME),
            wrong_basic(crate::test_support::FIXTURE_NAME),
            wrong_change(&cookie),
        ],
    );
    tokio::time::sleep(SETTLE).await;

    // On a thread of its own and under a time limit, so that a connection
    // somebody is holding fails this test instead of hanging it.
    let (reached, waited) = std::sync::mpsc::channel();
    std::thread::spawn({
        let library = Arc::clone(&state.library);
        move || {
            drop(library.conn());
            let _ = reached.send(());
        }
    });
    let free = waited
        .recv_timeout(std::time::Duration::from_secs(5))
        .is_ok();
    drop(held);

    assert!(
        free,
        "the library's connection could not be taken while three requests waited for a \
         verify permit, so one of them is holding it"
    );
    assert_eq!(
        answers(sent)
            .await
            .expect("the three answer once the permits are free"),
        [
            StatusCode::UNAUTHORIZED,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN
        ]
    );
}

/// Sends `request` to a server whose last free permit is the request's own,
/// and answers what it was told along with whether anybody else could take a
/// permit at each commit it made.
///
/// A commit hook runs on the thread that is writing, inside the commit, so
/// this sees the permit at the moment the record is being made and not
/// before or after it. At each commit a thread of its own asks for a permit
/// and the hook waits [`SETTLE`] for it. **The wait is only how a defect
/// shows**: the one permit that is free belongs to the request, so a request
/// that still holds it leaves the probe waiting however long that is, and one
/// that has given it back is found at once.
async fn told_and_free_at_each_commit(
    state: &Arc<ServeState>,
    request: Request<Body>,
) -> (StatusCode, Vec<bool>) {
    let mut held = state.auth.verifies.hold_all();
    drop(held.pop());

    let free_at_commit = Arc::new(std::sync::Mutex::new(Vec::<bool>::new()));
    {
        let limit = state.auth.verifies.clone();
        let seen = Arc::clone(&free_at_commit);
        state
            .library
            .conn()
            .commit_hook(Some(move || {
                let (took, waited) = std::sync::mpsc::channel();
                let limit = limit.clone();
                std::thread::spawn(move || {
                    let permit = limit.permit();
                    let _ = took.send(());
                    drop(permit);
                });
                let free = waited.recv_timeout(SETTLE).is_ok();
                seen.lock().expect("the probe does not panic").push(free);
                false
            }))
            .expect("installs the hook");
    }

    let sent = send_all(state, vec![request]);
    let answered = answers(sent).await;
    // Before the caller asserts, so a red run still lets the runtime shut down.
    drop(held);

    let told = answered.expect("the request answers")[0];
    let seen = free_at_commit
        .lock()
        .expect("the probe does not panic")
        .clone();
    (told, seen)
}

/// **The permit is held until the failure is recorded.** A login for a name
/// nobody holds records two failures, the pair's and then the address's, and
/// no permit is free at either.
///
/// Given back before the record, a permit would let a request that has not yet
/// read the lockout take it and read a count the first has not yet raised.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_for_an_unknown_name_records_its_failure_holding_its_permit() {
    let tmp = crate::test_support::Fixture::new("verify-record-login-decoy");
    let state = Arc::new(crate::test_support::state(&tmp));

    let (told, free) = told_and_free_at_each_commit(&state, wrong_login("nobody-at-all")).await;

    assert_eq!(told, StatusCode::UNAUTHORIZED);
    assert_eq!(
        free,
        [false, false],
        "a permit was free while a login's failure was being recorded, so the login \
         gave its own back before the record"
    );
}

/// The same for a real account with a wrong password.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_with_a_wrong_password_records_its_failure_holding_its_permit() {
    let tmp = crate::test_support::Fixture::new("verify-record-login-real");
    let state = Arc::new(crate::test_support::state(&tmp));

    let (told, free) =
        told_and_free_at_each_commit(&state, wrong_login(crate::test_support::FIXTURE_NAME)).await;

    assert_eq!(told, StatusCode::UNAUTHORIZED);
    assert_eq!(
        free,
        [false, false],
        "a permit was free while a login's failure was being recorded, so the login \
         gave its own back before the record"
    );
}

/// The same over `Authorization: Basic`, for a name nobody holds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_basic_request_for_an_unknown_name_records_its_failure_holding_its_permit() {
    let tmp = crate::test_support::Fixture::new("verify-record-basic-decoy");
    let state = Arc::new(crate::test_support::state(&tmp));

    let (told, free) = told_and_free_at_each_commit(&state, wrong_basic("nobody-at-all")).await;

    assert_eq!(told, StatusCode::UNAUTHORIZED);
    assert_eq!(
        free,
        [false, false],
        "a permit was free while a Basic request's failure was being recorded, so the \
         request gave its own back before the record"
    );
}

/// The same over `Authorization: Basic`, for a real account.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_basic_request_with_a_wrong_password_records_its_failure_holding_its_permit() {
    let tmp = crate::test_support::Fixture::new("verify-record-basic-real");
    let state = Arc::new(crate::test_support::state(&tmp));

    let (told, free) =
        told_and_free_at_each_commit(&state, wrong_basic(crate::test_support::FIXTURE_NAME)).await;

    assert_eq!(told, StatusCode::UNAUTHORIZED);
    assert_eq!(
        free,
        [false, false],
        "a permit was free while a Basic request's failure was being recorded, so the \
         request gave its own back before the record"
    );
}

/// The same on the password change, which records one failure.
///
/// A request by cookie makes a commit of its own before the handler takes a
/// permit, when the guard looks the session up: `session_owner` slides it,
/// which is a write. The permit is free at that, since the request has none
/// yet. The failure's record is the last commit it makes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_password_change_records_its_failure_holding_its_permit() {
    let tmp = crate::test_support::Fixture::new("verify-record-change");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);

    let (told, free) = told_and_free_at_each_commit(&state, wrong_change(&cookie)).await;

    assert_eq!(told, StatusCode::FORBIDDEN);
    assert_eq!(
        free.last(),
        Some(&false),
        "a permit was free while a password change's failure was being recorded, so \
         the change gave its own back before the record (free at each commit: {free:?})"
    );
}

/// **A password change gives its permit back before it hashes the new
/// password.** `set_password` hashes while it holds the library's connection,
/// and its commit is where this sees the permit: a change still holding the
/// last one would wait there for itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_password_change_hashes_the_new_password_holding_no_permit() {
    let tmp = crate::test_support::Fixture::new("verify-change-hash");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);

    // All but one permit are this test's, so the change's is the last one.
    let mut held = state.auth.verifies.hold_all();
    drop(held.pop());
    assert_eq!(
        held.len(),
        MAX_CONCURRENT_VERIFIES - 1,
        "exactly one permit must be free: with two, the commit hook's own permit would be \
         granted whether or not the change still held one, and the test would see nothing"
    );
    {
        let limit = state.auth.verifies.clone();
        state
            .library
            .conn()
            .commit_hook(Some(move || {
                drop(limit.permit());
                false
            }))
            .expect("installs the hook");
    }

    let sent = send_all(
        &state,
        vec![change_with(&cookie, crate::test_support::FIXTURE_PASSWORD)],
    );
    let answered = answers(sent).await;
    drop(held);

    assert_eq!(
        answered.expect("the change was still holding its permit when it wrote the new hash"),
        [StatusCode::NO_CONTENT]
    );
}

/// A burst of names nobody holds over Basic, against an address one failure
/// under its ceiling: the decoy verify, and the wide key that bounds it on
/// the path every guarded route shares.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_of_invented_names_spends_one_allowance_over_basic() {
    let tmp = crate::test_support::Fixture::new("verify-burst-basic-names");
    let state = Arc::new(crate::test_support::state(&tmp));
    wind(
        &state,
        &account::address_key(NO_PEER),
        account::ADDRESS_LOCKOUT_AFTER - 1,
    );

    let requests = (0..BURST)
        .map(|n| wrong_basic(&format!("nobody-{n}")))
        .collect();
    let statuses = burst(&state, requests).await;

    assert_one_allowance(&statuses, StatusCode::UNAUTHORIZED);
}

/// A server whose `login_attempt` cannot be read.
fn state_with_unreadable_attempts(tmp: &crate::test_support::Fixture) -> Arc<ServeState> {
    let state = Arc::new(crate::test_support::state(tmp));
    state
        .library
        .conn()
        .execute("DROP TABLE login_attempt", [])
        .expect("drops");
    state
}

/// **The lockout read fails closed.** With `login_attempt` unreadable, a
/// login with the RIGHT password is answered as a lockout. A wrong one would
/// be answered 429 by the failure that cannot be recorded, whichever way the
/// read went.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_whose_lockout_cannot_be_read_is_answered_as_a_lockout() {
    let tmp = crate::test_support::Fixture::new("verify-failclosed-login");
    let state = state_with_unreadable_attempts(&tmp);

    let response = build_router(state)
        .oneshot(login_as(
            crate::test_support::FIXTURE_NAME,
            crate::test_support::FIXTURE_PASSWORD,
        ))
        .await
        .expect("answers");

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a lockout the server could not read was read as not locked out"
    );
}

/// The same over `Authorization: Basic`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_basic_request_whose_lockout_cannot_be_read_is_answered_as_a_lockout() {
    let tmp = crate::test_support::Fixture::new("verify-failclosed-basic");
    let state = state_with_unreadable_attempts(&tmp);

    let response = build_router(state)
        .oneshot(basic_as(
            crate::test_support::FIXTURE_NAME,
            crate::test_support::FIXTURE_PASSWORD,
        ))
        .await
        .expect("answers");

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a lockout the server could not read was read as not locked out"
    );
}

/// The same on the password change.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_password_change_whose_lockout_cannot_be_read_is_answered_as_a_lockout() {
    let tmp = crate::test_support::Fixture::new("verify-failclosed-change");
    let state = state_with_unreadable_attempts(&tmp);
    let cookie = crate::test_support::fixture_session_cookie(&state.library);

    let response = build_router(state)
        .oneshot(change_with(&cookie, crate::test_support::FIXTURE_PASSWORD))
        .await
        .expect("answers");

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a lockout the server could not read was read as not locked out"
    );
}

/// What `request` is answered, sent on its own and waited for.
async fn answer_to(state: &Arc<ServeState>, request: Request<Body>) -> Response {
    build_router(state.clone())
        .oneshot(request)
        .await
        .expect("answers")
}

/// Asserts what can be seen of where `state`'s server has verified: it has
/// made one working memory, the one is spare again now that nothing is being
/// verified, and a verify has run in it.
///
/// The last is read from how the memory prints, which differs from how a new
/// one prints. A server that made its memories and then verified each password
/// in a memory made for the call would count the same and bound nothing.
fn assert_verified_in_one_memory(state: &ServeState) {
    assert_eq!(
        state.auth.verifies.memories_made(),
        1,
        "the verifies that came one after another did not all run in the first one's \
         working memory"
    );
    let spare = state.auth.verifies.spare_memories();
    assert_eq!(
        spare.len(),
        1,
        "the one working memory was not given back with the last permit: {spare:?}"
    );
    assert_ne!(
        spare[0],
        format!("{:?}", WorkingMemory::new()),
        "the working memory this server made holds what a new one holds, so its verifies \
         ran somewhere else"
    );
}

/// **Verifies that come one after another all run in one working memory.**
///
/// A permit takes a memory an earlier permit gave back before it makes one,
/// so a server whose verifies never overlap makes one memory and no second.
/// Every login here is for a name nobody holds, so each is verified against
/// the decoy and none of them is refused as locked out before it reaches a
/// permit: a login that was never verified would say nothing about the
/// memory it ran in.
///
/// **So this is also where the login route's decoy verify is held to its
/// permit's memory.** Each other place a password is verified has a test of
/// its own below, and
/// [`a_login_at_a_real_account_is_verified_in_its_permits_memory`] says why.
#[tokio::test]
async fn logins_one_after_another_run_in_one_working_memory() {
    let tmp = crate::test_support::Fixture::new("verify-memory-in-turn");
    let state = Arc::new(crate::test_support::state(&tmp));
    assert_eq!(
        state.auth.verifies.memories_made(),
        0,
        "a server that has verified nothing has already made a working memory"
    );

    for n in 0..BURST {
        assert_eq!(
            answer_to(&state, wrong_login(&format!("nobody-{n}")))
                .await
                .status(),
            StatusCode::UNAUTHORIZED,
            "login {n} was not verified"
        );
    }

    assert_verified_in_one_memory(&state);
}

/// **The login route verifies a real account's password in its permit's
/// memory.**
///
/// A password is verified by the login route, for a name nobody holds and
/// for a real account, by `Authorization: Basic` for the same two, and by
/// the password change. Each is one line that borrows its permit's working
/// memory, and each could as easily verify in a memory of its own: it would
/// answer the same, and bring back a buffer for every request, which is what
/// the permits' memories are there to stop. A test that mixes them cannot
/// tell, because one verify in the permit's memory is all
/// [`assert_verified_in_one_memory`] can see. So each has a test in which it
/// is the only verify its server runs.
///
/// **Here that is the login route's verify of a real account.** Both logins
/// are at the fixture account, and a name that exists is never verified
/// against the decoy. The route is outside the guard, so nothing is verified
/// before it, and opening a session verifies nothing after. The second login
/// gives the right password and is answered with a session, which no other
/// verify could have let through.
#[tokio::test]
async fn a_login_at_a_real_account_is_verified_in_its_permits_memory() {
    let tmp = crate::test_support::Fixture::new("verify-site-login-real");
    let state = Arc::new(crate::test_support::state(&tmp));
    let name = crate::test_support::FIXTURE_NAME;

    assert_eq!(
        answer_to(&state, wrong_login(name)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        answer_to(
            &state,
            login_as(name, crate::test_support::FIXTURE_PASSWORD)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT,
        "the fixture account's own password did not open a session, so these logins \
         did not reach the verify of a real account"
    );

    assert_verified_in_one_memory(&state);
}

/// **`Authorization: Basic` verifies a name nobody holds in its permit's
/// memory.**
///
/// The verify reached is `from_basic`'s against the decoy, in the guard, and
/// it is the only one: neither name exists, so there is no account's hash to
/// verify against, and a request the guard refuses never reaches a handler.
#[tokio::test]
async fn a_basic_request_for_an_unknown_name_is_verified_in_its_permits_memory() {
    let tmp = crate::test_support::Fixture::new("verify-site-basic-decoy");
    let state = Arc::new(crate::test_support::state(&tmp));

    for name in ["nobody-at-all", "nobody-else"] {
        assert_eq!(
            answer_to(&state, wrong_basic(name)).await.status(),
            StatusCode::UNAUTHORIZED,
            "a Basic request as {name:?} was not refused by the guard"
        );
    }

    assert_verified_in_one_memory(&state);
}

/// **`Authorization: Basic` verifies a real account's password in its
/// permit's memory.**
///
/// The verify reached is `from_basic`'s of the fixture account's hash, in the
/// guard. A name that exists is never verified against the decoy. The second
/// request gives the right password, which only this verify could have let
/// through the guard, and the handler it then reaches lists works and
/// verifies nothing.
#[tokio::test]
async fn a_basic_request_at_a_real_account_is_verified_in_its_permits_memory() {
    let tmp = crate::test_support::Fixture::new("verify-site-basic-real");
    let state = Arc::new(crate::test_support::state(&tmp));
    let name = crate::test_support::FIXTURE_NAME;

    assert_eq!(
        answer_to(&state, wrong_basic(name)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        answer_to(
            &state,
            basic_as(name, crate::test_support::FIXTURE_PASSWORD)
        )
        .await
        .status(),
        StatusCode::OK,
        "the fixture account's own password did not pass the guard, so these requests \
         did not reach the verify of a real account"
    );

    assert_verified_in_one_memory(&state);
}

/// **The password change verifies the current password in its permit's
/// memory.**
///
/// Both requests come by session cookie, which the guard resolves without
/// verifying anything. Sent over Basic, a change would be verified once in
/// the guard and again in the handler, and this would be a test of two
/// places. The second request gives the right current password and the
/// change is made, which only this verify could have allowed. The new
/// password is then hashed, and a hash runs in the memories the process keeps
/// for hashing and not in a permit's.
#[tokio::test]
async fn a_password_change_is_verified_in_its_permits_memory() {
    let tmp = crate::test_support::Fixture::new("verify-site-change");
    let state = Arc::new(crate::test_support::state(&tmp));
    let cookie = crate::test_support::fixture_session_cookie(&state.library);

    assert_eq!(
        answer_to(&state, wrong_change(&cookie)).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        answer_to(
            &state,
            change_with(&cookie, crate::test_support::FIXTURE_PASSWORD)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT,
        "the fixture account's own password did not change it, so these requests did \
         not reach the verify of the current password"
    );

    assert_verified_in_one_memory(&state);
}

/// **A server makes no more working memories than it has permits, however
/// many passwords it verifies and however they arrive.**
///
/// A memory these verifies have run in is the size of `credential`'s memory
/// cost, so this is the bound on what a burst of logins can make the server
/// hold. A memory for each request is the same burst with no bound at all.
///
/// The wait inside [`burst`] is not what this reads. The bound holds whatever
/// order the requests run in, and a server that made a memory for every
/// verify would have made at least as many as there were verifies however
/// they arrived.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_of_logins_runs_in_no_more_working_memories_than_there_are_permits() {
    let tmp = crate::test_support::Fixture::new("verify-memory-burst");
    let state = Arc::new(crate::test_support::state(&tmp));

    let requests = (0..BURST)
        .map(|n| wrong_login(&format!("nobody-{n}")))
        .collect();
    let statuses = burst(&state, requests).await;

    assert_eq!(
        statuses,
        [StatusCode::UNAUTHORIZED; BURST],
        "not every login of the burst was verified"
    );
    let made = state.auth.verifies.memories_made();
    assert!(
        made <= MAX_CONCURRENT_VERIFIES,
        "{made} working memories were made for a burst of {BURST} logins, by a server \
         with {MAX_CONCURRENT_VERIFIES} permits"
    );
    let spare = state.auth.verifies.spare_memories();
    assert_eq!(
        spare.len(),
        made,
        "a working memory was not given back with its permit: {spare:?}"
    );
    // The memories made are the ones the burst ran in, or the bound above is
    // on something the verifies never touched.
    let unused = format!("{:?}", WorkingMemory::new());
    assert!(
        spare.iter().all(|memory| *memory != unused),
        "a working memory this server made holds what a new one holds, so a verify of \
         the burst ran somewhere else: {spare:?}"
    );
}

/// **A password verified in a memory other verifies have used is answered as
/// it would be in a new one.**
///
/// The memory a login runs in is the one the login before it ran in: here a
/// decoy verify, then wrong guesses at a real account, then the right
/// password. The right password opens a session after the wrong ones, a
/// wrong one is refused after the right one, and the right one opens a
/// session again. The server made one memory for all of them, which is what
/// makes this about a used one.
#[tokio::test]
async fn a_login_verified_in_a_used_working_memory_is_answered_as_in_a_new_one() {
    let tmp = crate::test_support::Fixture::new("verify-memory-used");
    let state = Arc::new(crate::test_support::state(&tmp));
    let name = crate::test_support::FIXTURE_NAME;
    let right = || login_as(name, crate::test_support::FIXTURE_PASSWORD);

    for wrong in [
        wrong_login("nobody-at-all"),
        wrong_login(name),
        wrong_login(name),
    ] {
        assert_eq!(
            answer_to(&state, wrong).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    let opened = answer_to(&state, right()).await;
    assert_eq!(
        opened.status(),
        StatusCode::NO_CONTENT,
        "the right password was refused in a working memory wrong ones had used"
    );
    assert!(
        opened
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|cookie| cookie.starts_with(&format!("{COOKIE_NAME}="))),
        "the login was answered 204 and no session cookie came with it"
    );

    assert_eq!(
        answer_to(&state, wrong_login(name)).await.status(),
        StatusCode::UNAUTHORIZED,
        "a wrong password was accepted in the working memory the right one had just used"
    );
    assert_eq!(
        answer_to(&state, right()).await.status(),
        StatusCode::NO_CONTENT
    );

    // Or none of the answers above was shown to come from a used memory.
    assert_verified_in_one_memory(&state);
}

/// O266: a cookie request with a live session is served while another
/// connection holds the store's write lock past its busy timeout. The lookup's
/// two writes, the sweep of an expired row and the slide of a live one, are
/// bookkeeping, and neither can sign a reader out.
#[tokio::test]
async fn a_live_session_is_served_while_another_connection_holds_the_write_lock() {
    let (router, state, _fixture) = crate::test_support::state_with_two_scanned_roots_on_disk();
    let holder = rusqlite::Connection::open(state.library.path().expect("a store in a file"))
        .expect("another connection");
    holder
        .execute_batch("BEGIN IMMEDIATE")
        .expect("takes the write lock");

    let answered = router
        .oneshot(
            Request::get("/api/library/roots")
                .body(Body::empty())
                .expect("builds the request"),
        )
        .await
        .expect("answers")
        .status();
    holder.execute_batch("ROLLBACK").expect("releases the lock");

    assert_eq!(answered, StatusCode::OK);
}

/// Well under the store's busy timeout (5 s), which a request that waited for
/// the write lock would take.
const NO_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// [`crate::test_support::FIXTURE_PASSWORD`] hashed with argon2id at m=8, t=1,
/// p=1, argon2's least cost, so a password request's time is its wait and not
/// its hash. A request answered 200 with it is the proof that it verifies.
const CHEAP_PHC: &str =
    "$argon2id$v=19$m=8,t=1,p=1$bzI3Mi1jaGVhcC1zYWx0$lpSyVDkw9eWsFGcgMAS8DcTIlgwKxC22nMOGecBa2Kk";

/// Another connection's `BEGIN IMMEDIATE` on the fixture's store, as a derive
/// holds it while it writes a work.
fn holding_the_write_lock(state: &crate::ServeState) -> rusqlite::Connection {
    let holder = rusqlite::Connection::open(state.library.path().expect("a store in a file"))
        .expect("another connection");
    holder
        .execute_batch("BEGIN IMMEDIATE")
        .expect("takes the write lock");
    holder
}

/// How `router` answers `request`, and how long it took.
async fn timed(router: axum::Router, request: Request<Body>) -> (StatusCode, std::time::Duration) {
    let started = std::time::Instant::now();
    let status = router.oneshot(request).await.expect("answers").status();
    (status, started.elapsed())
}

/// A request for the works, authenticated by `authorization` where given and
/// otherwise by the fixture router's own session cookie.
fn works_request(authorization: Option<String>) -> Request<Body> {
    let request = Request::get("/api/works");
    let request = match authorization {
        Some(value) => request.header("authorization", value),
        None => request,
    };
    request.body(Body::empty()).expect("builds the request")
}

/// The fixture account's password through `Authorization: Basic`.
fn basic_password() -> String {
    use base64::Engine as _;

    let encoded = base64::engine::general_purpose::STANDARD.encode(format!(
        "{}:{}",
        crate::test_support::FIXTURE_NAME,
        crate::test_support::FIXTURE_PASSWORD
    ));
    format!("Basic {encoded}")
}

/// Gives the fixture account [`CHEAP_PHC`] and records one failure against the
/// pair a `oneshot` request is keyed on (no peer address reads `0.0.0.0`), and
/// answers that pair's key.
fn a_cheap_password_with_one_failure(state: &crate::ServeState) -> String {
    let conn = state.library.conn();
    conn.execute(
        "UPDATE user SET password_hash = ?1 WHERE id = 1",
        [CHEAP_PHC],
    )
    .expect("sets the password");
    let key = account::attempt_key("0.0.0.0", crate::test_support::FIXTURE_NAME);
    account::note_failure(&conn, &key).expect("records a failure");
    key
}

/// How many failure rows the pair `key` has.
fn failures_of(state: &crate::ServeState, key: &str) -> i64 {
    state
        .library
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM login_attempt WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .expect("counts the failures")
}

/// O272: a cookie request with a live session is answered without waiting for
/// the store's write lock to slide the session.
#[tokio::test]
async fn a_cookie_request_does_not_wait_for_the_write_lock_to_slide_its_session() {
    let (router, state, _fixture) = crate::test_support::state_with_two_scanned_roots_on_disk();
    let holder = holding_the_write_lock(&state);

    let (status, took) = timed(router, works_request(None)).await;
    holder.execute_batch("ROLLBACK").expect("releases the lock");

    assert_eq!(status, StatusCode::OK);
    assert!(took < NO_WAIT, "the request took {took:?}");
}

/// O272: a cookie naming no live session is refused without waiting for the
/// store's write lock to sweep its expired row.
#[tokio::test]
async fn a_cookie_naming_no_live_session_does_not_wait_for_the_write_lock_to_sweep_it() {
    let (router, state, _fixture) = crate::test_support::state_with_two_scanned_roots_on_disk();
    let stale = credential::mint_token();
    state
        .library
        .conn()
        .execute(
            "INSERT INTO session (token_sha256, user_id, expires)
             VALUES (?1, 1, datetime('now', '-1 hour'))",
            [credential::token_sha256(&stale)],
        )
        .expect("inserts an expired session");
    let holder = holding_the_write_lock(&state);

    let request = Request::get("/api/works")
        .header("cookie", format!("{COOKIE_NAME}={stale}"))
        .body(Body::empty())
        .expect("builds the request");
    let (status, took) = timed(router, request).await;
    holder.execute_batch("ROLLBACK").expect("releases the lock");

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(took < NO_WAIT, "the request took {took:?}");
}

/// O272: an API-key request is answered without waiting for the store's write
/// lock to record the key's use.
#[tokio::test]
async fn an_api_key_request_does_not_wait_for_the_write_lock_to_touch_its_key() {
    let (router, state, _fixture) = crate::test_support::state_with_two_scanned_roots_on_disk();
    let holder = holding_the_write_lock(&state);

    let request = works_request(Some(crate::test_support::fixture_authorization()));
    let (status, took) = timed(router, request).await;
    holder.execute_batch("ROLLBACK").expect("releases the lock");

    assert_eq!(status, StatusCode::OK);
    assert!(took < NO_WAIT, "the request took {took:?}");
}

/// O272: a request a password authenticates through `Authorization: Basic` is
/// answered without waiting for the store's write lock to forget the pair's
/// failures, which stay for the next success to forget.
#[tokio::test]
async fn a_basic_password_request_does_not_wait_for_the_write_lock_to_clear_its_failures() {
    let (router, state, _fixture) = crate::test_support::state_with_two_scanned_roots_on_disk();
    let key = a_cheap_password_with_one_failure(&state);
    let holder = holding_the_write_lock(&state);

    let (status, took) = timed(router, works_request(Some(basic_password()))).await;
    holder.execute_batch("ROLLBACK").expect("releases the lock");

    assert_eq!(
        (status, failures_of(&state, &key)),
        (StatusCode::OK, 1),
        "(the answer, the pair's failure rows)"
    );
    assert!(took < NO_WAIT, "the request took {took:?}");
}

/// O272: with the write lock free, a password through `Authorization: Basic`
/// still forgets the pair's failures.
#[tokio::test]
async fn a_basic_password_request_clears_its_failures_while_the_write_lock_is_free() {
    let (router, state, _fixture) = crate::test_support::state_with_two_scanned_roots_on_disk();
    let key = a_cheap_password_with_one_failure(&state);

    let (status, _) = timed(router, works_request(Some(basic_password()))).await;

    assert_eq!(
        (status, failures_of(&state, &key)),
        (StatusCode::OK, 0),
        "(the answer, the pair's failure rows)"
    );
}
