use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use taiyaku_library::credential;
use tower::ServiceExt as _;

use crate::ServeState;
use crate::auth::{Cidr, client_address, client_of};
use crate::routes::{build_router, guarded_routes};

/// A `Basic` header value for this name and secret.
fn basic(name: &str, secret: &str) -> String {
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("{name}:{secret}").as_bytes());
    format!("Basic {encoded}")
}

/// The fixture's seeded account, given a real password.
fn with_password(state: &ServeState, password: &str) {
    let conn = state.library.conn();
    let phc = credential::hash_password(password).expect("hashes");
    conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
        .expect("sets a password");
}

/// A concrete request path for a route pattern, using the fixture work's own
/// identifiers so a 401 cannot be a 404 wearing a disguise.
fn concrete(pattern: &str) -> String {
    pattern
        .replace("{work}", "t")
        .replace("{edition}", "raw")
        .replace("{index}", "0")
        .replace("{number}", "0")
}

/// **Every guarded route refuses an anonymous request, enumerated from the
/// router's own route list rather than from a checklist.**
///
/// A reviewer's checklist goes stale the first time someone adds a route and
/// does not think about auth, which is exactly the failure this guards. Driving
/// it off `guarded_routes()` — the same list `build_router` folds — means a new
/// unguarded route fails the suite on the commit that adds it.
#[tokio::test]
async fn every_route_refuses_an_anonymous_request() {
    let patterns: Vec<&'static str> = guarded_routes().into_iter().map(|(path, _)| path).collect();

    // **The guarded list, stated.** Two earlier attempts at this were both
    // useless and the second was worse, because it looked derived: a `>= 13`
    // floor that never moved when six account routes were added twenty lines
    // below it, and then `patterns.len() == route_count() - open_routes().len()`
    // — which is `G == G`, since `route_count` is *defined* as guarded plus
    // open. A tautology in a safety net is worse than an obvious hand-typed
    // list, because the commit message claimed it was derived.
    //
    // So it is typed, like the sibling test's exemption list, and the two
    // together are what hold the line: a route that moves between the lists
    // fails one of them. Found by the second whole-branch review.
    let expected = [
        "/api/works",
        "/api/works/{work}",
        "/api/works/{work}/{edition}/pages/{index}",
        "/api/works/{work}/{edition}/pages/{index}/thumb",
        "/api/works/{work}/progress",
        "/api/works/{work}/notes",
        "/api/works/{work}/truth",
        "/api/works/{work}/alignments",
        "/api/me",
        "/api/keys",
        "/api/keys/{id}",
        "/api/sessions",
        "/api/sessions/{id}",
        "/api/password",
        "/opds/v1.2/catalog",
        "/opds/v1.2/works",
        "/opds/v1.2/works/{work}",
        "/opds/v1.2/works/{work}/{edition}/pages/{number}",
        "/opds/v1.2/works/{work}/{edition}/cover",
    ];
    assert_eq!(
        patterns, expected,
        "the guarded list changed; a route that left it must appear in \
         `open_routes` with a reason a guarded route could not serve it, which \
         `open_routes_are_the_ones_that_cannot_be_guarded` asserts"
    );

    for pattern in patterns {
        let path = concrete(pattern);
        let state = Arc::new(crate::test_support::state("taiyaku-test-auth-enum"));
        let response = build_router(state)
            .oneshot(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .expect("router answers");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} answered {} to an anonymous request",
            response.status()
        );
    }
}

/// **The other half of the enumeration, which did not exist.**
///
/// `open_routes` is a by-name exemption from the guard, and `build_router`'s own
/// comment credited the enumeration test with catching a route added to the
/// wrong list. It could not: nothing read `open_routes` at all, so a route moved
/// there shipped unauthenticated with a green suite.
///
/// This asserts the exemption is exactly the three paths that *cannot* be
/// guarded, and says for each why — a login cannot already be authenticated, a
/// logout has to work from an expired session, and a claim is what creates the
/// first account. Anything else appearing here fails, which is the direction the
/// old test could not see.
#[tokio::test]
async fn open_routes_are_the_ones_that_cannot_be_guarded() {
    let open: Vec<&'static str> = crate::routes::open_routes()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(
        open,
        vec!["/api/session", "/api/claim"],
        "the guard's exemption list changed; every entry needs a reason a \
         guarded route could not serve it"
    );

    // And each is proved to be *outside* the guard by an anonymous request the
    // guard could not have answered. Asserting "not 401" would not do it: a
    // login refusing wrong credentials answers 401 with the same body the guard
    // sends, so the only honest proof is a success no guarded route could give.

    // Login: correct credentials and no `Authorization` header at all. Behind
    // the guard this is a 401 before the handler is reached.
    let state = Arc::new(crate::test_support::state("taiyaku-test-auth-open-login"));
    let logged_in = build_router(state)
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
        logged_in.status(),
        StatusCode::NO_CONTENT,
        "a login cannot already be authenticated, so it must be outside the guard"
    );

    // Logout: anonymous, and 204 because a browser holding an expired cookie
    // needs it cleared. Behind the guard that browser would be stuck holding
    // the thing it was trying to be rid of.
    let state = Arc::new(crate::test_support::state("taiyaku-test-auth-open-logout"));
    let logged_out = build_router(state)
        .oneshot(
            Request::delete("/api/session")
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);

    // Claim: anonymous, and 404 with the flag off — which is the shut answer
    // rather than the guard's, and is what makes a claimed server look like one
    // built without the flag.
    let state = Arc::new(crate::test_support::state("taiyaku-test-auth-open-claim"));
    let claim = build_router(state)
        .oneshot(
            Request::post("/api/claim")
                .header("content-type", "application/json")
                .header("host", "test")
                .header("origin", "http://test")
                .body(Body::from(r#"{"name":"x","password":"y"}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(claim.status(), StatusCode::NOT_FOUND);
}

/// §4: `/opds` sends `WWW-Authenticate` because that is what makes an OPDS
/// client prompt for credentials. `/api` must **not**, because it makes the
/// browser raise its own native auth dialog over the reader — which the web app
/// cannot style, cannot pre-fill and cannot recover from.
#[tokio::test]
async fn the_two_401_shapes_differ_by_path() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-auth-401-api"));
    let api = build_router(state)
        .oneshot(Request::get("/api/works").body(Body::empty()).unwrap())
        .await
        .expect("answers");
    assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
    assert!(
        api.headers().get("www-authenticate").is_none(),
        "an /api 401 carrying WWW-Authenticate raises the browser's own dialog, \
         which the reader cannot style, pre-fill or recover from"
    );

    let state = Arc::new(crate::test_support::state("taiyaku-test-auth-401-opds"));
    let opds = build_router(state)
        .oneshot(
            Request::get("/opds/v1.2/catalog")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(opds.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        opds.headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"taiyaku\""),
        "without this header Panels and Chunky never prompt for credentials"
    );
}

/// A password authenticates through Basic, and a wrong one does not.
#[tokio::test]
async fn basic_accepts_a_password() {
    let state = crate::test_support::state("taiyaku-test-auth-basic");
    with_password(&state, "fixture");
    let state = Arc::new(state);

    let ok = build_router(state.clone())
        .oneshot(
            Request::get("/api/works")
                .header("authorization", basic("local", "fixture"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(ok.status(), StatusCode::OK);

    let wrong = build_router(state.clone())
        .oneshot(
            Request::get("/api/works")
                .header("authorization", basic("local", "not it"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let nobody = build_router(state)
        .oneshot(
            Request::get("/api/works")
                .header("authorization", basic("nobody", "fixture"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(nobody.status(), StatusCode::UNAUTHORIZED);
}

/// **§2's whole point, asserted structurally rather than with a stopwatch.**
///
/// The account's stored hash is left at [`credential::LOCKED`], which no
/// password can ever verify against. An API key that authenticates anyway
/// therefore *cannot* have been checked by argon2 — the only path that reaches
/// argon2 is the one that would have failed. A timing assertion would say the
/// same thing more slowly and less reliably; this one fails outright if a later
/// refactor makes the key path fall through to the password verifier.
#[tokio::test]
async fn an_api_key_authenticates_without_reaching_argon2() {
    let state = crate::test_support::state("taiyaku-test-auth-key");
    let secret = credential::mint_api_key();
    {
        let conn = state.library.conn();
        // **The proof rests on this line.** The account's stored hash is
        // replaced with a well-formed-looking string that argon2 cannot verify
        // anything against, so a key authenticating anyway cannot have been
        // checked by the password path.
        //
        // Not `LOCKED`, which an earlier version used: `by_api_key` refuses a
        // locked account outright now — an unclaimed row must not be reachable
        // by a key any more than by a password — so locking it would prove the
        // refusal rather than the routing. Found while fixing that, in the
        // third whole-branch review.
        conn.execute(
            "UPDATE user SET password_hash = '$argon2id$unverifiable' WHERE id = 1",
            [],
        )
        .expect("gives the account a hash nothing verifies against");
        assert!(
            !credential::verify_password("anything at all", "$argon2id$unverifiable"),
            "this test's proof rests on the password path being unable to succeed"
        );
        conn.execute(
            "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
            [credential::token_sha256(&secret)],
        )
        .expect("inserts the key");
    }
    let state = Arc::new(state);

    let ok = build_router(state.clone())
        .oneshot(
            Request::get("/opds/v1.2/catalog")
                .header("authorization", basic("local", &secret))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(
        ok.status(),
        StatusCode::OK,
        "an API key must authenticate against an account whose password is locked"
    );

    // And the same account's locked password still refuses, so the success
    // above is the key path rather than a guard that stopped checking.
    let refused = build_router(state)
        .oneshot(
            Request::get("/opds/v1.2/catalog")
                .header("authorization", basic("local", credential::LOCKED))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
}

/// A key that names an account in the username field it does not belong to
/// still authenticates as its **owner**, because the key is the credential and
/// the username beside it is not consulted.
///
/// Stated as a test because the alternative — matching the pair — reads as the
/// safer choice and is not: OPDS clients put arbitrary text in that field, and
/// a key that works from one client and not another would be diagnosed as a
/// broken key.
#[tokio::test]
async fn a_key_authenticates_as_its_owner_whatever_username_accompanies_it() {
    let state = crate::test_support::state("taiyaku-test-auth-key-name");
    let secret = credential::mint_api_key();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
            [credential::token_sha256(&secret)],
        )
        .expect("inserts the key");
    }
    let state = Arc::new(state);

    let response = build_router(state)
        .oneshot(
            Request::get("/opds/v1.2/catalog")
                .header("authorization", basic("whatever-panels-sends", &secret))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
}

/// A malformed `Authorization` header is a refusal rather than a panic, for
/// every shape a client can send.
#[tokio::test]
async fn a_malformed_authorization_header_is_refused() {
    for header in [
        "Basic",
        "Basic ",
        "Basic !!!not-base64!!!",
        // Valid base64, no colon.
        "Basic bm9jb2xvbg==",
        "Bearer something",
        "",
    ] {
        let state = Arc::new(crate::test_support::state("taiyaku-test-auth-malformed"));
        let response = build_router(state)
            .oneshot(
                Request::get("/api/works")
                    .header("authorization", header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("answers");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "`Authorization: {header}` answered {}",
            response.status()
        );
    }
}

/// **A path that does not exist still answers 404, not 401.**
///
/// This is why the guard is a `route_layer` rather than a `layer`: the second
/// wraps the fallback too, so every URL a caller invented would come back 401 —
/// telling them each one names something real they merely cannot see.
#[tokio::test]
async fn an_unmatched_path_is_still_a_404() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-auth-404"));
    let response = build_router(state)
        .oneshot(
            Request::get("/api/nothing-here")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn cidr_parses_and_matches_both_families() {
    use std::net::IpAddr;

    let v4: Cidr = "192.168.1.0/24".parse().expect("v4");
    assert!(v4.contains("192.168.1.0".parse::<IpAddr>().unwrap()));
    assert!(v4.contains("192.168.1.255".parse::<IpAddr>().unwrap()));
    assert!(!v4.contains("192.168.2.0".parse::<IpAddr>().unwrap()));

    // A prefix that is not a whole number of bytes, which is where a
    // byte-at-a-time comparison would quietly be wrong.
    let odd: Cidr = "10.0.0.0/12".parse().expect("v4 /12");
    assert!(odd.contains("10.15.255.255".parse::<IpAddr>().unwrap()));
    assert!(!odd.contains("10.16.0.0".parse::<IpAddr>().unwrap()));

    let v6: Cidr = "fd00::/8".parse().expect("v6");
    assert!(v6.contains("fd12::1".parse::<IpAddr>().unwrap()));
    assert!(!v6.contains("fe80::1".parse::<IpAddr>().unwrap()));

    // A bare address is the whole address, which is what a person means by
    // `--trusted-proxy 10.0.0.1`.
    let one: Cidr = "10.0.0.1".parse().expect("bare");
    assert!(one.contains("10.0.0.1".parse::<IpAddr>().unwrap()));
    assert!(!one.contains("10.0.0.2".parse::<IpAddr>().unwrap()));

    // An IPv4-mapped v6 peer is the v4 address it names — which is what a
    // dual-stack listener hands over for an IPv4 client, so a v4 range that
    // did not cover it would silently trust nothing.
    assert!(v4.contains("::ffff:192.168.1.7".parse::<IpAddr>().unwrap()));

    assert!("nonsense".parse::<Cidr>().is_err());
    assert!("10.0.0.0/33".parse::<Cidr>().is_err());
    assert!("fd00::/129".parse::<Cidr>().is_err());
    assert!("10.0.0.0/x".parse::<Cidr>().is_err());
}

/// §6's table, first row: trusting `X-Forwarded-For` from whatever connected
/// means trusting the internet on a misconfigured deployment, so the header is
/// honoured only when the **direct peer** falls inside a configured range.
#[test]
fn forwarded_for_is_honoured_only_from_inside_a_configured_range() {
    use std::net::IpAddr;

    let trusted: Vec<Cidr> = vec!["10.0.0.0/8".parse().expect("cidr")];
    let inside: IpAddr = "10.1.2.3".parse().unwrap();
    let outside: IpAddr = "203.0.113.9".parse().unwrap();
    let claimed = Some("198.51.100.7");

    assert_eq!(
        client_address(&trusted, inside, claimed),
        "198.51.100.7".parse::<IpAddr>().unwrap(),
        "a trusted proxy's X-Forwarded-For was ignored"
    );
    assert_eq!(
        client_address(&trusted, outside, claimed),
        outside,
        "an untrusted peer spoofed its address"
    );

    // With nothing configured, nothing is trusted — which is the default, and
    // the reason `--trusted-proxy` is a range rather than a boolean.
    assert_eq!(client_address(&[], inside, claimed), inside);

    // A trusted peer that sends no header is itself the client.
    assert_eq!(client_address(&trusted, inside, None), inside);
    // And one that sends nonsense is not allowed to erase itself.
    assert_eq!(
        client_address(&trusted, inside, Some("not-an-address")),
        inside
    );
}

/// The **last** entry of the chain is the one a proxy appended; the ones before
/// it are whatever the client sent, so reading from the front would let a client
/// prepend an address of its choosing.
#[test]
fn a_forwarded_chain_is_read_from_the_right_end() {
    use std::net::IpAddr;

    let trusted: Vec<Cidr> = vec!["10.0.0.0/8".parse().expect("cidr")];
    let peer: IpAddr = "10.0.0.1".parse().unwrap();
    assert_eq!(
        client_address(&trusted, peer, Some("1.2.3.4, 198.51.100.7")),
        "198.51.100.7".parse::<IpAddr>().unwrap(),
    );
}

/// **§7 names two paths and only one shipped.** The design says the limiter is
/// "on the login route *and on failed `Authorization: Basic` verification
/// only*". Without the second, a password is guessable at any rate through any
/// guarded route while ten tries at `POST /api/session` lock the pair out for
/// fifteen minutes — so the limiter guarded the one door a guesser has no
/// reason to use.
///
/// It is also an unauthenticated denial of service in its own right: each guess
/// holds a `spawn_blocking` thread for the measured 11.9 ms and allocates
/// argon2's 19 MiB `m_cost` buffer, and nothing bounds how many a stranger may
/// start.
///
/// Found by the whole-branch review, 2026-08-18.
#[tokio::test]
async fn guessing_a_password_over_basic_is_rate_limited_too() {
    let state = crate::test_support::state("taiyaku-test-auth-basic-lockout");
    with_password(&state, "fixture");
    let state = Arc::new(state);

    let attempt = |password: &'static str| {
        let state = state.clone();
        async move {
            build_router(state)
                .oneshot(
                    Request::get("/api/works")
                        .header("authorization", basic("local", password))
                        .body(Body::empty())
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
        "a password is guessable without limit through any guarded route"
    );
    assert_eq!(
        attempt("fixture").await,
        StatusCode::TOO_MANY_REQUESTS,
        "a locked-out pair must not be let in by finally guessing right"
    );
}

/// **An API key is deliberately not rate limited**, and that is §2's reasoning
/// rather than an omission: 256 bits from a CSPRNG has no dictionary and no
/// structure, so there is nothing for a limit to buy — while OPDS clients retry
/// hard enough that one would lock a reader out of their own library.
#[tokio::test]
async fn a_working_api_key_is_never_locked_out() {
    let state = crate::test_support::state("taiyaku-test-auth-key-not-limited");
    let secret = credential::mint_api_key();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
            [credential::token_sha256(&secret)],
        )
        .expect("inserts the key");
    }
    let state = Arc::new(state);

    // Well past the password path's allowance, on the credential an OPDS client
    // actually presents for every page of a volume.
    for _ in 0..taiyaku_library::account::LOCKOUT_AFTER + 5 {
        let response = build_router(state.clone())
            .oneshot(
                Request::get("/opds/v1.2/catalog")
                    .header("authorization", basic("local", &secret))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("answers");
        assert_eq!(response.status(), StatusCode::OK);
    }
}

/// **Every `X-Forwarded-For` line, not just the first.**
///
/// `HeaderMap::get` answers the *first* value for a repeated name. A client can
/// send its own `X-Forwarded-For`, and a proxy that emits its own as a second
/// header line — which several Go, Rust and Java stacks do by default, unlike
/// nginx's `$proxy_add_x_forwarded_for` — leaves the attacker's line first. The
/// "last element" rule then protects nothing: the address read is the one the
/// client chose, so rotating it restores unlimited guessing and writes a chosen
/// value into every `session.ip` the account screen shows its owner as evidence.
///
/// Found by the whole-branch review, 2026-08-18.
#[test]
fn every_forwarded_line_is_read_not_just_the_first() {
    use std::net::IpAddr;

    let mut headers = axum::http::HeaderMap::new();
    // What a client sent, then what the proxy appended as its own line.
    headers.append("x-forwarded-for", "203.0.113.9".parse().unwrap());
    headers.append("x-forwarded-for", "198.51.100.7".parse().unwrap());

    let config = crate::session::AuthConfig {
        secure_cookies: false,
        allow_web_setup: false,
        trusted_proxies: vec!["10.0.0.0/8".parse().expect("cidr")],
    };
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
        [10, 0, 0, 1],
        1234,
    ))));

    assert_eq!(
        client_of(&config, &extensions, &headers),
        "198.51.100.7".parse::<IpAddr>().unwrap(),
        "the client's own forged line was read instead of the proxy's"
    );
}

/// With no peer recorded — which is every `oneshot` test — the address is a
/// stated placeholder rather than an `Option` the limiter would have to key on.
#[test]
fn a_request_with_no_peer_gets_a_stated_address() {
    use std::net::IpAddr;

    let config = crate::session::AuthConfig::default();
    assert_eq!(
        client_of(
            &config,
            &axum::http::Extensions::new(),
            &axum::http::HeaderMap::new()
        ),
        "0.0.0.0".parse::<IpAddr>().unwrap()
    );
}

/// A forwarded header with no trusted proxy is ignored *and* said out loud, once.
///
/// The address is what §7 keys the lockout on, and behind an unnamed proxy every
/// request arrives from the proxy — so the pair collapses to the account and a
/// stranger can lock a reader out of every device. Not trusted instead, which is
/// §6's first row; warned about, so the operator can name the proxy.
#[test]
fn a_forwarded_header_without_a_trusted_proxy_is_ignored() {
    use std::net::IpAddr;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-forwarded-for", "198.51.100.7".parse().unwrap());
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
        [127, 0, 0, 1],
        1234,
    ))));

    let bare = crate::session::AuthConfig::default();
    assert_eq!(
        client_of(&bare, &extensions, &headers),
        "127.0.0.1".parse::<IpAddr>().unwrap(),
        "an unnamed peer's claim about the client must not be believed"
    );
    // Twice, because the warning is deduplicated and must not change the answer.
    assert_eq!(
        client_of(&bare, &extensions, &headers),
        "127.0.0.1".parse::<IpAddr>().unwrap()
    );
}

/// **A v6-written base against a v4 peer.**
///
/// `from_str` derived the prefix width from the *uncanonicalised* base while
/// `contains` canonicalises it, so `--trusted-proxy ::ffff:10.0.0.1` — the exact
/// form `Cidr::contains`'s own doc says was added to support — stored width 128
/// and then sliced 16 bytes out of a 4-byte array on the first request from any
/// v4 peer. Startup succeeded; every connection then panicked.
///
/// Found by the third whole-branch review.
#[test]
fn a_mapped_v6_range_matches_the_v4_peer_it_names() {
    use std::net::IpAddr;

    let mapped: Cidr = "::ffff:10.0.0.1".parse().expect("parses");
    assert!(mapped.contains("10.0.0.1".parse::<IpAddr>().unwrap()));
    assert!(!mapped.contains("10.0.0.2".parse::<IpAddr>().unwrap()));

    // A mapped address written with an IPv6 prefix is refused rather than
    // guessed at, and the refusal says what the number means in the form the
    // address canonicalises to — the two are different and a bare "/104 is
    // longer than 32 bits" would read as a bug in the server.
    let refused = "::ffff:10.0.0.0/104".parse::<Cidr>().unwrap_err();
    assert!(
        refused.contains("IPv4 address written in IPv6"),
        "{refused}"
    );
    assert!(refused.contains("/8"), "{refused}");

    // And the equivalent range, written the way it canonicalises, works.
    let ranged: Cidr = "10.0.0.0/8".parse().expect("parses");
    assert!(ranged.contains("10.0.0.7".parse::<IpAddr>().unwrap()));
    assert!(!ranged.contains("11.0.0.0".parse::<IpAddr>().unwrap()));
}

/// **Every guarded route's response says it may not be stored by a shared
/// cache**, enumerated from the router rather than from a list of routes
/// somebody remembered.
///
/// The first attempt applied the header where each per-reader document was
/// built and reached two of eight; `get_notes` — the one the constant's own doc
/// named first — was not among them, and neither were the OPDS feeds, which
/// additionally carry no `Set-Cookie` and so lack even the accident that makes
/// most proxies decline to cache an authenticated response.
///
/// Found by the third whole-branch review.
#[tokio::test]
async fn every_guarded_response_refuses_a_shared_cache() {
    for (pattern, _) in guarded_routes() {
        let path = concrete(pattern);
        let state = crate::test_support::state("taiyaku-test-auth-cache");
        let credential = crate::test_support::fixture_authorization();
        let response = build_router(Arc::new(state))
            .oneshot(
                Request::get(&path)
                    .header("authorization", &credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("answers");

        let directive = response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            directive.contains("private"),
            "{path} answered {} with Cache-Control {directive:?}; a shared cache \
             in front would store one reader's answer and replay it to another",
            response.status()
        );
        assert!(
            directive.contains("no-store") || directive.contains("no-cache"),
            "{path} answered Cache-Control {directive:?}, which permits storing"
        );
    }
}

/// Which credentials may administer the account they authenticated.
///
/// Stated as its own test because folding a password and a key into one
/// `By::Header` variant is what made the account routes refuse a correct
/// password with a message asserting a key had been used — and because the
/// answer is a security rule rather than a detail of the enum.
#[test]
fn only_a_password_or_a_session_may_administer_an_account() {
    use crate::auth::By;

    assert!(By::Cookie.may_administer());
    assert!(By::Password.may_administer());
    assert!(
        !By::Key.may_administer(),
        "an API key is a device credential — §2 justifies its fast hash by its \
         role as the thing an OPDS client presents for every page"
    );
}
