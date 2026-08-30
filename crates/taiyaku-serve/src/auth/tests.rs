use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine as _;
use taiyaku_library::credential;
use tower::ServiceExt as _;

use crate::ServeState;
use crate::auth::{Cidr, Principal, Role, client_address, client_of};
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
        "/api/works/{work}/{edition}/download",
        "/api/works/{work}/{edition}/members/{ordinal}/download",
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
        // Spec C1 §5's browse API. Guarded, which is the whole point of this
        // list: `route_layer` wraps only what is registered before it, so a
        // browse route that drifted into `open_routes` would ship
        // unauthenticated and this assertion is what refuses to let that
        // happen quietly.
        "/api/library/home",
        "/api/library/series",
        "/api/library/series/{id}",
        "/api/library/series/{id}/cover",
        "/api/library/works/{slug}",
        "/api/library/search",
        "/api/library/status",
        // Spec C3 §7's proposals inbox, which is admin-gated *inside* each
        // handler — so it is doubly important that it is guarded here first:
        // an anonymous request has no `Principal` for that gate to read at
        // all.
        "/api/library/proposals",
        "/api/library/proposals/{id}",
        "/api/library/proposals/{id}/resolution",
        "/api/library/proposals/{id}/accept",
        "/api/library/proposals/{id}/dismiss",
        // Spec C3 §5's metadata-editing routes, admin-gated the same way —
        // and it matters doubly here too, since the guard is what stands
        // between an anonymous request and a `Principal`-less
        // `admin_or_forbidden` call. `/api/library/works/{slug}` and
        // `/api/library/series/{id}` repeat two paths already above: a
        // `PATCH` merged onto the same `GET` route, not a second route at
        // the same address (`routes.rs`'s own comment beside the
        // registration).
        "/api/library/works/{slug}",
        "/api/library/series/{id}",
        "/api/library/editions/{id}",
        "/api/library/members/{id}",
        "/opds/v1.2/catalog",
        "/opds/v1.2/works",
        "/opds/v1.2/works/{work}",
        "/opds/v1.2/works/{work}/{edition}/pages/{number}",
        "/opds/v1.2/works/{work}/{edition}/cover",
        "/opds/v1.2/works/{work}/{edition}/download",
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

/// **The match guard that lets a header credential speak past a cross-site
/// cookie.** `resolve`'s own comment states it — "a header credential beside it
/// is still allowed to speak for the request" — and mutation testing found the
/// guard could be replaced with `true` on 2026-08-19 with every test passing.
///
/// With the guard always true, an OPDS client that happens to carry a stale
/// cookie from a browser session on the same host is refused 403 for an origin
/// it never sent, while its perfectly valid `Authorization` header is never
/// looked at. Nothing in the suite paired the two credentials, so nothing saw
/// it.
#[tokio::test]
async fn a_basic_header_still_speaks_past_a_cookie_refused_for_its_origin() {
    let state = Arc::new(crate::test_support::state(
        "taiyaku-test-crosssite-plus-basic",
    ));
    with_password(&state, "a password");
    let token = credential::mint_token();
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, 1, &token, None, None).expect("opens");
    }

    // The cookie alone, cross-site: refused, and refused as a *session* problem
    // rather than as a signed-out reader.
    let cookie_only = build_router(state.clone())
        .oneshot(
            Request::post("/api/works/1/progress")
                .header("host", "test")
                .header("origin", "http://evil.test")
                .header("cookie", format!("taiyaku_session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":0}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(cookie_only.status(), StatusCode::FORBIDDEN);

    // The same request with a valid header beside the same cookie: the header
    // answers it. A header credential is not CSRF-able, which is the entire
    // reason the origin check is documented as the cookie path's alone.
    let with_header = build_router(state)
        .oneshot(
            Request::post("/api/works/1/progress")
                .header("host", "test")
                .header("origin", "http://evil.test")
                .header("cookie", format!("taiyaku_session={token}"))
                .header("authorization", basic("local", "a password"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":0}"#))
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_ne!(
        with_header.status(),
        StatusCode::FORBIDDEN,
        "the cookie's origin refusal swallowed a valid Authorization header, so \
         a client carrying both is locked out by the weaker of its two \
         credentials"
    );
    assert_ne!(with_header.status(), StatusCode::UNAUTHORIZED);
}

/// The mapped-address sentence in `Cidr`'s refusal, which is a decision inside
/// a `format!` and survived `&&` becoming `||` on 2026-08-19.
///
/// It exists because `::ffff:10.0.0.0/104` is an IPv4 range written in IPv6 and
/// /104 there means /8 here — a person who typed one number is being told about
/// another. With `||`, every over-long prefix gets the mapped explanation,
/// including plain IPv6 ones where it is simply false.
#[test]
fn an_over_long_prefix_explains_itself_differently_for_a_mapped_address() {
    // Asserted on the whole sentence rather than on `contains` of one word, and
    // on the translated number in particular: /104 in the mapped form means /8,
    // and that arithmetic is the only reason the sentence exists.
    let mapped = "::ffff:10.0.0.0/104".parse::<Cidr>().expect_err("refused");
    assert!(
        mapped.contains("written in IPv6") && mapped.contains("/8 is what /104 means here"),
        "a mapped address needs the sentence saying what the number means in \
         the form it canonicalises to: {mapped}"
    );
    assert!(
        mapped.contains("32 bits of an IPv4 address"),
        "and the width it is measured against is IPv4's: {mapped}"
    );

    let plain = "2001:db8::/200".parse::<Cidr>().expect_err("refused");
    assert!(
        !plain.contains("written in IPv6"),
        "a plain IPv6 range was given the mapped-address explanation, which is \
         not true of it: {plain}"
    );
    assert!(
        plain.contains("128 bits of an IPv6 address"),
        "it should still name the width: {plain}"
    );

    // **The case that separates `&&` from `||`, and the reason the first
    // version of this test did not kill the mutant.** Both halves agree on the
    // two cases above — a mapped address satisfies both, a plain IPv6 one
    // neither — so an assertion on those alone passes either way. An *ordinary*
    // IPv4 range is the one input where they differ: it contains no `::ffff:`
    // and its base is v4, so `&&` says no and `||` says yes. Under `||` this
    // over-long v4 prefix is told it is "an IPv4 address written in IPv6",
    // which it is not, and given a translated number that means nothing.
    let plain_v4 = "10.0.0.0/40".parse::<Cidr>().expect_err("refused");
    assert!(
        !plain_v4.contains("written in IPv6"),
        "an ordinary IPv4 range was told it was written in IPv6: {plain_v4}"
    );
    assert!(
        plain_v4.contains("32 bits of an IPv4 address"),
        "it should still name the width: {plain_v4}"
    );
}

/// The decision behind the `X-Forwarded-For` warning, extracted from the
/// `eprintln!` it used to live inside so that it can be asserted at all.
///
/// The warning tells an operator that their login rate limit is being applied to
/// the proxy rather than to a reader. `&&` surviving as `||` meant it fired for
/// every request from a direct client that sent no such header — and a warning
/// that cries wolf is the one nobody reads on the day it is right.
#[test]
fn the_forwarded_warning_fires_only_when_a_header_is_being_ignored() {
    let mut config = crate::session::AuthConfig::default();

    assert!(
        !crate::auth::forwarded_but_untrusted(false, &config),
        "no header means nothing is being ignored, whatever the proxy list says"
    );
    assert!(
        crate::auth::forwarded_but_untrusted(true, &config),
        "a header with no --trusted-proxy is exactly the case worth warning about"
    );

    config.trusted_proxies = vec!["10.0.0.0/8".parse().expect("parses")];
    assert!(
        !crate::auth::forwarded_but_untrusted(true, &config),
        "a header from a configured proxy is being honoured, not ignored"
    );
    assert!(!crate::auth::forwarded_but_untrusted(false, &config));
}

/// `failed` builds the 500 every account route answers when the database
/// refuses a write, and it survived being replaced with `Response::default()`
/// on 2026-08-19 — **a 200 with an empty body**. Nothing asserted on it, so a
/// password change that changed nothing would have answered success.
///
/// Reached by taking away the table `set_password` writes through, which is the
/// same error class the route reasons about — `serve` and `library import`
/// share one file — reproduced deterministically rather than by racing a writer.
#[tokio::test]
async fn a_password_change_the_database_refuses_is_not_answered_as_success() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-password-refused"));
    {
        let conn = state.library.conn();
        // `set_password` ends the account's other sessions as part of the same
        // change, so this is the write that fails. Basic auth carries the
        // request, which needs no session of its own.
        conn.execute("DROP TABLE session", []).expect("drops");
    }

    let response = build_router(state)
        .oneshot(
            Request::put("/api/password")
                .header("host", "test")
                .header(
                    "authorization",
                    basic(
                        crate::test_support::FIXTURE_NAME,
                        crate::test_support::FIXTURE_PASSWORD,
                    ),
                )
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"current":"{}","next":"a new password"}}"#,
                    crate::test_support::FIXTURE_PASSWORD
                )))
                .unwrap(),
        )
        .await
        .expect("answers");

    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "a password change that did not happen came back as success, so the \
         reader now believes a password they cannot log in with"
    );
}

// ---------------------------------------------------------------------------
// The enumeration `routes::guarded_routes`'s own doc comment argues for and
// authentication already has: "that is exactly the failure a reviewer's
// checklist has: it goes stale the first time someone adds a route and does
// not think about auth." Six routes proved the identical failure for
// `Principal.roots` — `stream_page`, `cover`, `work_feed` and `works` in
// `opds.rs`, `list_works` in `routes.rs`, and `patch_alignments` — drifting
// out of the grant one at a time, over more than one spec, before anything
// caught it. `routes::grant_postures` and the test below are what stops a
// seventh.
// ---------------------------------------------------------------------------

/// A concrete path for a route pattern, filled in with identifiers this
/// module's grant-proof fixtures actually define — unlike [`concrete`]
/// above, which never needs `{id}`, `{slug}` or `{ordinal}` because every
/// caller of it is anonymous and refused by the guard before any handler
/// reads a path parameter at all. These requests are authenticated and reach
/// the handler, so a placeholder left as literal text (`{id}`) would be
/// rejected by axum's own path extractor before the grant is ever consulted —
/// answering 400, not the 404 a `Refuses` entry is being asked to prove.
fn grant_proof_concrete(pattern: &str) -> String {
    pattern
        .replace("{work}", "t")
        .replace("{slug}", "t")
        .replace("{edition}", "raw")
        .replace("{index}", "0")
        .replace("{number}", "0")
        .replace("{ordinal}", "0")
        .replace("{id}", "1")
}

/// A channel value for [`router_with_material_under_a_withheld_root`]'s
/// real page image — the same helper `opds::tests` and `test_support`
/// duplicate locally, for the same reason: each module's fixtures are read
/// on their own.
///
/// # Panics
///
/// Panics if `v` exceeds `u8::MAX`. It cannot: the caller multiplies a
/// coordinate `from_fn` bounds to `0..16` by 16, topping out at `15 * 16
/// == 240`.
fn channel(v: u32) -> u8 {
    u8::try_from(v).expect("channel value exceeds u8::MAX")
}

/// A router over one work (`"t"`, edition `"raw"`, two members holding a
/// real, decodable page each) and the one series (id 1) it belongs to —
/// both filed entirely under a root the fixture account is **not** granted,
/// so the work and the series it alone stands for are as wholly hidden as
/// each other.
///
/// What every `Refuses` entry in `routes::grant_postures` needs, from a
/// single fixture: the page, thumb, download, sidecar and alignment routes
/// address the work by slug; the OPDS mirrors address the same work; the
/// library routes address the work by slug and the series by id.
///
/// **Returns the state too (I8)**, unlike its first version: the four
/// metadata PATCH routes need a real edition id and member id to address,
/// neither reachable from the slug alone the way the read routes' own ids
/// are.
///
/// **Real bytes, not placeholders — measured to matter.** `refuse_if_hidden`
/// and `refuse_if_work_hidden` both answer 404 before any of these routes
/// opens a member, so a first draft of this fixture used the shared
/// placeholder bytes `test_support::work` writes, on the reasoning that
/// nothing here should ever reach them. Proven wrong by deliberately
/// disabling `refuse_if_hidden` (returning `None` unconditionally) and
/// re-running this suite: `get_page`, `get_thumb`, both download routes and
/// the OPDS page/cover mirrors fell through to a real file read, found
/// nothing at the placeholder paths, and answered 404 from *that* instead —
/// the exact "colour change is not evidence" trap this branch's report
/// warns about, and the same reason `opds::tests::router_with_a_real_page_under_a_withheld_root`
/// already insists on real bytes. With real, decodable images in place, the
/// same mutation now answers 200 with the actual page for every one of
/// those routes, which is what makes the loop below a proof rather than a
/// coincidence.
fn router_with_material_under_a_withheld_root(dir: &str) -> (axum::Router, Arc<ServeState>) {
    use std::io::Write as _;

    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let shelf = root.to_string_lossy().into_owned();

    let page = image::RgbImage::from_fn(16, 16, |x, y| {
        let r = channel(x * 16);
        let g = channel(y * 16);
        image::Rgb([r, g, 200])
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(page)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let real_image = encoded.into_inner();

    for name in ["c1.cbz", "c2.cbz"] {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(name)).unwrap());
        zip.start_file("p1.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&real_image).unwrap();
        zip.finish().unwrap();
    }

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "T", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/c1.cbz", "sha256": "x", "pages": 1 }},
              {{ "path": "{shelf}/c2.cbz", "sha256": "y", "pages": 1 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");

        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld')",
            [],
        )
        .expect("two roots");
        conn.execute("UPDATE member SET root_id = 2", [])
            .expect("files every member under the withheld root");
        // **O86: a grant only restricts a non-admin.** `Principal::from_account`
        // drops an administrator's roots, so a grant test whose holder is the
        // seeded admin measures nothing — it would pass against a filter that
        // had been deleted.
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a grant restricts a user, never an admin");
        conn.execute("INSERT INTO user_root (user_id, root_id) VALUES (1, 1)", [])
            .expect("grants the fixture account the other root");

        conn.execute(
            "INSERT INTO series (id, sort_title) VALUES (1, 'Series')",
            [],
        )
        .expect("writes the series");
        conn.execute(
            "INSERT INTO series_title (series_id, title, kind, is_primary) \
             VALUES (1, 'Series', 'english', 1)",
            [],
        )
        .expect("names the series");
        conn.execute("UPDATE work SET series_id = 1, volume = 1.0", [])
            .expect(
                "files the work under the series — its only work, so the \
                 series is as wholly hidden as the work is",
            );
    }
    let works = crate::works::from_library(&library).expect("projects the imported work");
    let state = Arc::new(ServeState {
        library,
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    (build_router(state.clone()), state)
}

/// A router over two works — one filed under a root the fixture account
/// holds, one under a root it does not — for the `Filters` entries whose
/// contract is "name the granted work, omit the withheld one."
///
/// Root paths are `/visible-root` and `/hidden-root` rather than the
/// `/held`/`/withheld` convention `opds::tests` and `routes::tests` use
/// elsewhere in this crate. `library_api::status`'s proof below checks
/// whether a root's *name* is present in the body, and `"withheld"` ends in
/// `"held"` — an assertion that the body contains `"held"` would pass whether
/// or not `"withheld"` had also leaked. Distinct roots avoid that trap rather
/// than relying on a reader noticing it.
fn two_works_one_withheld(dir: &str) -> (axum::Router, Arc<ServeState>) {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let shelf = root.to_string_lossy().into_owned();

    let visible_json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Visible Work", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/v1.cbz", "sha256": "v", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );
    let hidden_json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Hidden Work", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/h1.cbz", "sha256": "h", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let visible_doc =
            taiyaku_core::Sidecar::from_json(&visible_json).expect("visible sidecar parses");
        let visible_id = taiyaku_library::import::scanned(&conn, &visible_doc, None)
            .expect("import the visible work");
        let hidden_doc =
            taiyaku_core::Sidecar::from_json(&hidden_json).expect("hidden sidecar parses");
        let hidden_id = taiyaku_library::import::scanned(&conn, &hidden_doc, None)
            .expect("import the hidden work");

        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/visible-root'), (2, '/hidden-root')",
            [],
        )
        .expect("two roots");
        conn.execute(
            "UPDATE member SET root_id = 1 WHERE edition_id IN \
             (SELECT id FROM edition WHERE work_id = ?1)",
            [visible_id],
        )
        .expect("files the visible work under the visible root");
        conn.execute(
            "UPDATE member SET root_id = 2 WHERE edition_id IN \
             (SELECT id FROM edition WHERE work_id = ?1)",
            [hidden_id],
        )
        .expect("files the hidden work under the hidden root");
        // **O86: a grant only restricts a non-admin.** `Principal::from_account`
        // drops an administrator's roots, so a grant test whose holder is the
        // seeded admin measures nothing — it would pass against a filter that
        // had been deleted.
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a grant restricts a user, never an admin");
        conn.execute("INSERT INTO user_root (user_id, root_id) VALUES (1, 1)", [])
            .expect("grants only the visible root");
    }
    let works = crate::works::from_library(&library).expect("projects the imported works");
    let state = Arc::new(ServeState {
        library,
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    (build_router(state.clone()), state)
}

/// A router over **one** work with two editions, each filed under a
/// different root — `en` under the root the fixture account holds, `raw`
/// under the one it does not. What `work_feed`'s `Filters` classification
/// needs: a work that is itself visible, with exactly one of its editions
/// outside the grant, mirroring `opds::tests::router_with_two_editions_split_across_roots`.
///
/// **Carries one pre-existing `raw` → `en` alignment**, added for
/// `patch_alignments_refuses_a_correction_naming_a_withheld_edition` (and
/// reused by `every_route_declares_its_grant_posture`'s own alignments
/// proof): `Sidecar::correct` only rewrites a *stored* edge or one it can
/// compose through the hub from stored edges either side, and a document
/// with `"alignments": []` has neither — a correction from `en` to the hub
/// (`raw`) on a document with no prior edge answers `NoPath`, not the write
/// this fixture exists to drive (or refuse).
///
/// **Filed under series id 1**, its only work — added for
/// `every_route_declares_its_grant_posture`'s split-fixture proof of
/// `/api/library/series/{id}` and `/cover`, which need a series id to
/// address. Unlike `router_with_material_under_a_withheld_root`'s series,
/// this one is *not* wholly hidden: `en` survives the grant, so the series
/// itself is visible with one of its work's two editions filtered out —
/// the split case, at the series level rather than the work level.
fn one_work_two_editions_split_across_roots(dir: &str) -> (axum::Router, Arc<ServeState>) {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let shelf = root.to_string_lossy().into_owned();

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Split", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/raw/c1.cbz", "sha256": "x", "pages": 2 }}
            ], "spreads": [] }},
            "en": {{ "label": "En", "members": [
              {{ "path": "{shelf}/en/d1.cbz", "sha256": "p", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        let work_id =
            taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");
        // Seeded by hand, directly into the `alignment`/`run` tables, rather
        // than in the document's own `"alignments"` field: `import::scanned`
        // runs under `Scope::Scan`, which never calls `replace_alignments` —
        // "a scan describes which files are members of which edition; it has
        // nothing to say about how two editions correspond" — so an
        // alignment written into the JSON above would import and vanish.
        conn.execute(
            "INSERT INTO alignment (work_id, from_key, to_key, by) \
             VALUES (?1, 'raw', 'en', 'hand')",
            [work_id],
        )
        .expect("seeds a raw-to-en alignment for the correction test to build on");
        let alignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO run \
                 (alignment_id, ordinal, a_start_page, a_end_page, b_start_page, b_end_page) \
             VALUES (?1, 0, 0, 1, 0, 1)",
            [alignment_id],
        )
        .expect("seeds the run pairing both pages straight across");
        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/visible-root'), (2, '/hidden-root')",
            [],
        )
        .expect("two roots");
        conn.execute(
            "UPDATE member SET root_id = 2 WHERE edition_id = \
             (SELECT id FROM edition WHERE work_id = ?1 AND key = 'raw')",
            [work_id],
        )
        .expect("files raw under the hidden root");
        conn.execute(
            "UPDATE member SET root_id = 1 WHERE edition_id = \
             (SELECT id FROM edition WHERE work_id = ?1 AND key = 'en')",
            [work_id],
        )
        .expect("files en under the visible root");
        // **O86: a grant only restricts a non-admin.** `Principal::from_account`
        // drops an administrator's roots, so a grant test whose holder is the
        // seeded admin measures nothing — it would pass against a filter that
        // had been deleted.
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a grant restricts a user, never an admin");
        conn.execute("INSERT INTO user_root (user_id, root_id) VALUES (1, 1)", [])
            .expect("grants only the visible root");

        // Filed under a series — its only work, mirroring
        // `router_with_material_under_a_withheld_root` — so
        // `/api/library/series/{id}` and `/cover` have an id to address.
        // Since `en` survives the grant, the series is *not* wholly hidden
        // the way that fixture's is; what every_route_declares_its_grant_posture
        // needs from this one is the opposite case.
        conn.execute(
            "INSERT INTO series (id, sort_title) VALUES (1, 'Split Series')",
            [],
        )
        .expect("writes the series");
        conn.execute(
            "INSERT INTO series_title (series_id, title, kind, is_primary) \
             VALUES (1, 'Split Series', 'english', 1)",
            [],
        )
        .expect("names the series");
        conn.execute("UPDATE work SET series_id = 1, volume = 1.0", [])
            .expect("files the work under the series");
    }
    let works = crate::works::from_library(&library).expect("projects the imported work");
    let state = Arc::new(ServeState {
        library,
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    (build_router(state.clone()), state)
}

/// A router over **one** work with **three** editions: `en` and `fr` filed
/// under the root the fixture account holds, `raw` under one it does not —
/// and `raw` is also this work's `hub`. `one_work_two_editions_split_across_roots`
/// above cannot drive either of Finding 1's two harder properties, because it
/// keeps only one visible edition: there is no pair of visible editions to
/// correct between (so a correction's response-filter can never be proven
/// without also 404ing on a withheld edition first), and its one seeded edge
/// runs `raw → en`, so a request naming the withheld edition on the `from`
/// side and a visible one on `to` has no edge to succeed over even if the
/// guard that refuses it were disabled — a mutation there would still 404,
/// just for the wrong reason. This fixture keeps two visible editions and
/// seeds an edge on *each* side of the withheld one so every one of Finding
/// 1's three assertions has something real to succeed against if its guard
/// or filter were removed:
///
/// - `raw → en`, straight across — lets a correction naming `raw` on the
///   `from` side actually succeed if `refuse_if_hidden(&correction.from)`
///   were disabled, rather than failing on `NoPath` regardless.
/// - `en → fr`, straight across — a correction between two *visible*
///   editions, so `patch_alignments`'s response-filter can be proven without
///   the request 404ing before a response is even built.
///
/// The withheld hub is deliberate too: `get_sidecar`'s `ETag` is computed
/// over the full document specifically so it agrees with what
/// `patch_alignments` recomputes from the same full document — a tag over
/// the filtered projection a restricted reader sees would drift the moment
/// `raw` changes, even though nothing this reader can see moved.
fn one_work_three_editions_hub_withheld(dir: &str) -> (axum::Router, Arc<ServeState>) {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let shelf = root.to_string_lossy().into_owned();

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Triple", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/raw/c1.cbz", "sha256": "x", "pages": 2 }}
            ], "spreads": [] }},
            "en": {{ "label": "En", "members": [
              {{ "path": "{shelf}/en/d1.cbz", "sha256": "p", "pages": 2 }}
            ], "spreads": [] }},
            "fr": {{ "label": "Fr", "members": [
              {{ "path": "{shelf}/fr/d1.cbz", "sha256": "q", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        let work_id =
            taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");
        // Seeded by hand, as `one_work_two_editions_split_across_roots` does,
        // for the identical reason: `import::scanned` runs under
        // `Scope::Scan`, which never calls `replace_alignments`.
        let seed_edge = |from: &str, to: &str| {
            conn.execute(
                "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, ?2, ?3, 'hand')",
                rusqlite::params![work_id, from, to],
            )
            .expect("seeds an alignment");
            let alignment_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO run \
                     (alignment_id, ordinal, a_start_page, a_end_page, b_start_page, b_end_page) \
                 VALUES (?1, 0, 0, 1, 0, 1)",
                [alignment_id],
            )
            .expect("seeds the run pairing both pages straight across");
        };
        seed_edge("raw", "en");
        seed_edge("en", "fr");

        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/visible-root'), (2, '/hidden-root')",
            [],
        )
        .expect("two roots");
        for key in ["en", "fr"] {
            conn.execute(
                "UPDATE member SET root_id = 1 WHERE edition_id = \
                 (SELECT id FROM edition WHERE work_id = ?1 AND key = ?2)",
                rusqlite::params![work_id, key],
            )
            .expect("files a visible edition under the visible root");
        }
        conn.execute(
            "UPDATE member SET root_id = 2 WHERE edition_id = \
             (SELECT id FROM edition WHERE work_id = ?1 AND key = 'raw')",
            [work_id],
        )
        .expect("files raw — the hub — under the hidden root");
        // **O86: a grant only restricts a non-admin.** `Principal::from_account`
        // drops an administrator's roots, so a grant test whose holder is the
        // seeded admin measures nothing — it would pass against a filter that
        // had been deleted.
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a grant restricts a user, never an admin");
        conn.execute("INSERT INTO user_root (user_id, root_id) VALUES (1, 1)", [])
            .expect("grants only the visible root");
    }
    let works = crate::works::from_library(&library).expect("projects the imported work");
    let state = Arc::new(ServeState {
        library,
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    (build_router(state.clone()), state)
}

/// **The missing enumeration test**, and the deliverable this branch's report
/// says matters most: without it, an eighth route can drift out of the grant
/// exactly the way the first six did, and nothing will say so.
///
/// Two things are asserted, not one. First, that `routes::grant_postures`'s
/// route set equals `guarded_routes`'s exactly — a route added without a
/// classification fails the build on the commit that adds it, and so does a
/// stale entry for a route `guarded_routes` no longer serves, the same
/// mechanism `every_route_refuses_an_anonymous_request` above uses for the
/// full ordered route list rather than a bare count.
///
/// Second, where a real request can prove the classification rather than
/// merely declare it, it does: every `Refuses` entry is driven with a real
/// request from an account granted a *different* root than the material it
/// addresses, and asserted 404; every `Filters` entry is driven the same way
/// and asserted to name the granted material while omitting the withheld
/// material. `GrantIndependent` and `KnownGap` entries are declared rather
/// than driven — the former because there is no grant behaviour to exercise,
/// the latter because driving one would assert something false about code
/// this branch did not change. See `GrantPosture`'s own doc comment for both.
#[tokio::test]
async fn every_route_declares_its_grant_posture() {
    use crate::routes::GrantPosture;

    let guarded: Vec<&'static str> = guarded_routes().into_iter().map(|(path, _)| path).collect();
    let table = crate::routes::grant_postures();
    let declared: Vec<&'static str> = table.iter().map(|(path, _)| *path).collect();
    assert_eq!(
        declared, guarded,
        "routes::grant_postures's route set must equal guarded_routes's \
         exactly — a route missing here has no stated grant posture, and an \
         entry here for a route guarded_routes no longer serves is stale"
    );

    // Every declared (rather than driven) entry has to say why, rather than
    // an empty string quietly satisfying the type — `GrantIndependent` and
    // `KnownGap` exist so a reviewer reads a reason, not so the match arm
    // compiles.
    for (pattern, posture) in &table {
        match posture {
            GrantPosture::GrantIndependent { reason } => assert!(
                !reason.is_empty(),
                "{pattern} is classified GrantIndependent but states no reason"
            ),
            GrantPosture::KnownGap { open_item, reason } => assert!(
                !open_item.is_empty() && !reason.is_empty(),
                "{pattern} is classified KnownGap but does not name the open \
                 item and reason it defers to"
            ),
            GrantPosture::Refuses | GrantPosture::Filters | GrantPosture::RefusesAndFilters => {}
        }
    }

    // Pinned to a literal here, not derived from the loops below: those
    // loops are driven *off* `table`'s own current contents — they walk
    // whatever `table` calls `Refuses`/`Filters`/`RefusesAndFilters` right
    // now and simply stop visiting a route once it leaves the pattern they
    // match. That makes a route's own reclassification invisible to them —
    // a route demoted from `Refuses` to `GrantIndependent` is not "driven
    // and failing", it is not driven at all, and the loop has no way to
    // notice a route it no longer iterates over. `driven` recomputes this
    // count from `table` on every run, so a route moved off any of the
    // three driven postures — in either direction, and regardless of which
    // two it moves between — changes `driven` without this assertion's own
    // literal moving to match, which is what actually catches it. (A move the
    // other way — `GrantIndependent` reclassified `Refuses` — is instead
    // caught by the loops themselves: driving a real request against a
    // route with no grant behaviour to enforce a 404 gets back whatever
    // status the route actually answers, not 404.)
    let driven = table
        .iter()
        .filter(|(_, posture)| {
            matches!(
                posture,
                GrantPosture::Refuses | GrantPosture::Filters | GrantPosture::RefusesAndFilters
            )
        })
        .count();
    assert_eq!(
        driven, 19,
        "19 of the 38 guarded routes must be classified Refuses, Filters or \
         RefusesAndFilters and driven by the proofs below — a route's \
         posture changed without this count moving to match, so a route \
         this suite believes it proves may no longer be one it does. It was \
         19; I8 raised it to 23 by reclassifying spec C3 §5's four metadata \
         routes from `GrantIndependent` to `Refuses`, because a hidden work \
         answered 404 on GET and 200 on PATCH; **O86 returned them and the \
         count with them.** Those four are admin-only and an administrator's \
         root grant is now dropped where the principal is built, so no caller \
         of them holds a grant to refuse against. I8's 404 was driven by a \
         fixture granting a root to the seeded *admin* — the combination O86 \
         made unrepresentable — so the proof is void rather than lost"
    );
    assert_eq!(
        table.len() - driven,
        19,
        "the remaining 19 guarded routes must be GrantIndependent or \
         KnownGap — see the `driven == 19` assertion just above, which this \
         one is the other half of. Was 19, then 15 while I8 held the four \
         metadata routes in the driven half, and 19 again now O86 has \
         returned them"
    );

    let credential = crate::test_support::fixture_authorization();

    // ---- `Refuses`: proven against a work, and the one series it belongs
    // to, both wholly outside the caller's grant. `RefusesAndFilters` shares
    // this proof for its refuse half — the pattern skip just below still
    // routes it to its own dedicated block instead. ----
    let (refuses_router, refuses_state) =
        router_with_material_under_a_withheld_root("taiyaku-test-grant-posture-refuses");
    for (pattern, posture) in &table {
        if !matches!(
            posture,
            GrantPosture::Refuses | GrantPosture::RefusesAndFilters
        ) {
            continue;
        }
        // `/api/works/{work}/alignments` is `PATCH`-only and names its two
        // editions in the JSON body, not the URL — this loop's GET-against-a-
        // pattern mechanism cannot express either, and a GET against it
        // would answer 405 from axum's own method dispatch, never reaching
        // the guard this loop means to prove. Driven in its own dedicated
        // block below instead, with the body and `If-Match` a correction
        // actually needs — both its `Refuses` half and its `Filters` half.
        //
        // **The four `library/{works,series,editions,members}` PATCH routes
        // (I8) are `PATCH`-only for the identical reason**, and two of them
        // share a path with an already-`Filters` `GET` — a `GET` against
        // either answers from that *other* handler, proving nothing about
        // the guard this loop means to check on the PATCH one. All four are
        // driven with real `PATCH` requests just below instead.
        if matches!(
            *pattern,
            "/api/works/{work}/alignments"
                | "/api/library/works/{slug}"
                | "/api/library/series/{id}"
                | "/api/library/editions/{id}"
                | "/api/library/members/{id}"
        ) {
            continue;
        }
        let path = grant_proof_concrete(pattern);
        let response = refuses_router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&path)
                    .header("authorization", &credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router answers");
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{pattern} is classified Refuses, but GET {path} from an \
             account granted a different root answered {}",
            response.status()
        );
    }

    // **The four metadata PATCH routes, and what now protects them — O86.**
    //
    // This block was I8's `Refuses` proof: the fixture's work, edition, member
    // and series are wholly hidden from the fixture account, so a PATCH naming
    // any of them had to 404 exactly as a GET does rather than resolving
    // through to a 200 that writes and re-projects the whole document.
    //
    // **That proof is void, not weakened.** It required an account holding a
    // root grant *and* the admin role these routes demand, and
    // `Principal::from_account` no longer produces one: an administrator's
    // grant is dropped, so nothing is hidden from any caller these routes
    // admit. The fixture above demotes the grant holder to `user` precisely so
    // its grant is live, which makes that same account inadmissible here.
    //
    // So the block keeps driving all four routes and asserts the guard that is
    // now load-bearing for them: **the role gate refuses first**. Deleting it
    // outright would leave the four with no driven proof at all, which is how
    // a reclassification quietly becomes a hole.
    let refuses_slug = refuses_state.works[0].id.clone();
    let (refuses_edition_id, refuses_member_id): (i64, i64) = {
        let conn = refuses_state.library.conn();
        let edition_id = conn
            .query_row("SELECT id FROM edition LIMIT 1", [], |row| row.get(0))
            .expect("the fixture's one edition");
        let member_id = conn
            .query_row("SELECT id FROM member LIMIT 1", [], |row| row.get(0))
            .expect("the fixture's one member");
        (edition_id, member_id)
    };
    for (pattern, uri, body) in [
        (
            "/api/library/works/{slug}",
            format!("/api/library/works/{refuses_slug}"),
            serde_json::json!({ "title": "Should Not Land" }),
        ),
        (
            "/api/library/series/{id}",
            "/api/library/series/1".to_string(),
            serde_json::json!({ "reading": "ltr" }),
        ),
        (
            "/api/library/editions/{id}",
            format!("/api/library/editions/{refuses_edition_id}"),
            serde_json::json!({ "label": "Should Not Land" }),
        ),
        (
            "/api/library/members/{id}",
            format!("/api/library/members/{refuses_member_id}"),
            serde_json::json!({ "chapter": 1.0 }),
        ),
    ] {
        let response = refuses_router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(&uri)
                    .header("authorization", &credential)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("router answers");
        let status = response.status();
        let response_body = crate::test_support::body_string(response).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{pattern} is admin-only, so PATCH {uri} from the non-admin this \
             fixture's live grant requires must be refused by the role gate \
             before any grant question is reached — answered {status}: \
             {response_body}"
        );
    }

    crate::test_support::cleanup("taiyaku-test-grant-posture-refuses");

    // ---- `Filters`: proven against two works, and — for `work_feed`, whose
    // filtering is per-edition rather than per-work — two editions of one
    // work. A route classified `Filters` is marked proven here only after its
    // own assertions pass, and every `Filters` entry in the table must be
    // marked by the end, so a `Filters` route this loop forgot to drive fails
    // the test rather than silently passing as "declared."
    let mut proven_filters: std::collections::HashSet<&'static str> =
        std::collections::HashSet::new();

    let (works_router, _works_state) =
        two_works_one_withheld("taiyaku-test-grant-posture-filters-a");
    for (pattern, uri) in [
        ("/api/works", "/api/works"),
        ("/opds/v1.2/works", "/opds/v1.2/works"),
        ("/api/library/home", "/api/library/home"),
        ("/api/library/series", "/api/library/series"),
        ("/api/library/search", "/api/library/search?q=Visible"),
    ] {
        let response = works_router
            .clone()
            .oneshot(
                Request::get(uri)
                    .header("authorization", &credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router answers");
        let status = response.status();
        let body = crate::test_support::body_string(response).await;
        assert_eq!(status, StatusCode::OK, "{pattern} ({uri}): {body}");
        assert!(
            body.contains("Visible Work"),
            "{pattern} is classified Filters and must still name the granted \
             work: {body}"
        );
        assert!(
            !body.contains("Hidden Work"),
            "{pattern} is classified Filters but named a work outside the \
             grant: {body}"
        );
        proven_filters.insert(pattern);
    }

    // `library_api::status` filters *roots*, not works or editions — the
    // split fixture below is one root's worth of material and has nothing
    // to say about this route, so its proof stays here, against the
    // distinct root names `two_works_one_withheld` was built with.
    let status_response = works_router
        .clone()
        .oneshot(
            Request::get("/api/library/status")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let status_code = status_response.status();
    let status_body = crate::test_support::body_string(status_response).await;
    assert_eq!(status_code, StatusCode::OK, "{status_body}");
    assert!(
        status_body.contains("visible-root"),
        "/api/library/status is classified Filters and must still name the \
         granted root: {status_body}"
    );
    assert!(
        !status_body.contains("hidden-root"),
        "/api/library/status named a root outside the grant: {status_body}"
    );
    proven_filters.insert("/api/library/status");
    crate::test_support::cleanup("taiyaku-test-grant-posture-filters-a");

    // ---- Finding 2: the split-across-roots case, for every `Refuses` and
    // `Filters` entry that can usefully be driven against it. ----
    //
    // `two_works_one_withheld` and `router_with_material_under_a_withheld_root`
    // above only ever hide a *whole* work — the fixture shape where the
    // work-level question and the edition-level one have the same answer,
    // which is exactly what let `get_sidecar`, `list_works` and
    // `patch_alignments` pass every proof that existed before this while
    // leaking a hidden edition sharing a work with a granted one. One
    // fixture, reused below for both the `Refuses` and the `Filters` half,
    // so both proofs see the identical split.
    let (split_router, split_state) =
        one_work_two_editions_split_across_roots("taiyaku-test-grant-posture-split");
    let split_slug = split_state.works[0].id.clone();

    // `Refuses`, again: the withheld half (`raw`) of a work that also keeps
    // a granted half (`en`) must still 404. Only the negative case is
    // asserted — `one_work_two_editions_split_across_roots` names members no
    // test writes to disk (it exists for the `Filters` proof below, which
    // never opens one), so asking for `en` and expecting 200 would answer
    // 404 from a missing file rather than from the grant: the "colour
    // change is not evidence" trap
    // `router_with_material_under_a_withheld_root`'s own doc comment warns
    // about, the wrong way round.
    for (pattern, posture) in &table {
        if !matches!(
            posture,
            GrantPosture::Refuses | GrantPosture::RefusesAndFilters
        ) {
            continue;
        }
        // See the identical skip in the wholly-hidden-work loop above:
        // `PATCH`-only, body-addressed, driven separately below.
        if matches!(
            *pattern,
            "/api/works/{work}/alignments"
                | "/api/library/works/{slug}"
                | "/api/library/series/{id}"
                | "/api/library/editions/{id}"
                | "/api/library/members/{id}"
        ) {
            continue;
        }
        let path = pattern
            .replace("{work}", &split_slug)
            .replace("{edition}", "raw")
            .replace("{index}", "0")
            .replace("{number}", "0")
            .replace("{ordinal}", "0")
            .replace("{id}", "1");
        let response = split_router
            .clone()
            .oneshot(
                Request::get(&path)
                    .header("authorization", &credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router answers");
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{pattern} is classified Refuses, but GET {path} against the \
             withheld half of a work that also has a granted half answered \
             {}",
            response.status()
        );
    }

    // `patch_edition`/`patch_member`'s own split proof (I8): the finding's
    // exact shape — an edition (and its one member) sharing a work with a
    // granted sibling. `edition_owner`/`member_owner` check the edition/
    // member's own root, not merely whether the work has *some* visible
    // material, so a PATCH naming the withheld `raw` edition had to 404 even
    // though `patch_work`/`patch_series` against this same work would
    // legitimately succeed (`en` keeps it visible).
    //
    // **Void for the same reason as the block above — O86.** The caller that
    // makes this fixture's grant live is a non-admin, and these routes admit
    // only admins, whose grant is dropped. `edition_owner` and `member_owner`
    // still ask the edition's own root and that is deliberately kept: it is
    // the correct answer for any caller holding a grant, and the day the role
    // rule changes it is the difference between a leak and none. It is simply
    // unreachable through these routes today, so what is asserted here is the
    // gate that does the refusing.
    let (raw_edition_id, raw_member_id): (i64, i64) = {
        let conn = split_state.library.conn();
        conn.query_row(
            "SELECT edition.id, member.id
               FROM edition JOIN member ON member.edition_id = edition.id
              WHERE edition.key = 'raw'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the withheld edition and its one member")
    };
    for (pattern, uri, body) in [
        (
            "/api/library/editions/{id}",
            format!("/api/library/editions/{raw_edition_id}"),
            serde_json::json!({ "label": "Should Not Land" }),
        ),
        (
            "/api/library/members/{id}",
            format!("/api/library/members/{raw_member_id}"),
            serde_json::json!({ "chapter": 1.0 }),
        ),
    ] {
        let response = split_router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(&uri)
                    .header("authorization", &credential)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("router answers");
        let status = response.status();
        let response_body = crate::test_support::body_string(response).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{pattern} is classified Refuses, but PATCH {uri} against the \
             withheld half of a work that also has a granted half answered \
             {status}: {response_body}"
        );
    }

    // **The other half of the pair, restored to meaning something under O86.**
    //
    // It used to be: a restricted grant must still allow editing material it
    // *does* cover, or the check above is not narrower than "deny everything".
    // With the grant no longer reaching these routes, asserting 403 here too
    // would collapse the pair into one assertion made twice — which looks like
    // coverage and is not.
    //
    // So the account is promoted back for this half. The pair now reads
    // non-admin → 403, admin → 200, which is the property that actually holds:
    // **the role gate refuses exactly non-admins and nothing else.** The
    // material is unchanged between the two halves, so the status difference
    // is attributable to the role alone.
    {
        let conn = split_state.library.conn();
        conn.execute("UPDATE user SET role = 'admin' WHERE id = 1", [])
            .expect("promotes for the positive half");
    }
    let (en_edition_id, en_member_id): (i64, i64) = {
        let conn = split_state.library.conn();
        conn.query_row(
            "SELECT edition.id, member.id
               FROM edition JOIN member ON member.edition_id = edition.id
              WHERE edition.key = 'en'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the granted edition and its one member")
    };
    for (pattern, uri, body) in [
        (
            "/api/library/editions/{id}",
            format!("/api/library/editions/{en_edition_id}"),
            serde_json::json!({ "label": "Should Land" }),
        ),
        (
            "/api/library/members/{id}",
            format!("/api/library/members/{en_member_id}"),
            serde_json::json!({ "chapter": 1.0 }),
        ),
    ] {
        let response = split_router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(&uri)
                    .header("authorization", &credential)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("router answers");
        let status = response.status();
        let response_body = crate::test_support::body_string(response).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{pattern} is admin-only, so PATCH {uri} from an administrator \
             must land — the role gate refuses exactly non-admins, and the \
             half above proves the other direction against the same \
             material: {response_body}"
        );
    }

    // **Demoted again, because the promotion above is scoped to that pair and
    // nothing else.** The OPDS proofs below need a caller whose grant is live,
    // and an admin's is dropped (O86) — leaving the account promoted here made
    // `/opds/v1.2/works/{work}` name an edition outside the grant, which is
    // the filter being unreachable rather than broken. The state is shared
    // across the blocks of this test, so the role has to be put back.
    {
        let conn = split_state.library.conn();
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("restores the restricted caller for the proofs below");
    }

    // `Filters`, split. `/opds/v1.2/works/{work}` was already proven this
    // way before Finding 2; the rest join it here.
    let work_feed_response = split_router
        .clone()
        .oneshot(
            Request::get(format!("/opds/v1.2/works/{split_slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let work_feed_status = work_feed_response.status();
    let work_feed_body = crate::test_support::body_string(work_feed_response).await;
    assert_eq!(work_feed_status, StatusCode::OK, "{work_feed_body}");
    assert!(
        work_feed_body.contains("/en/"),
        "/opds/v1.2/works/{{work}} is classified Filters and must still name \
         the granted edition: {work_feed_body}"
    );
    assert!(
        !work_feed_body.contains("/raw/"),
        "/opds/v1.2/works/{{work}} named an edition outside the grant: {work_feed_body}"
    );
    proven_filters.insert("/opds/v1.2/works/{work}");

    // `/api/works` (`list_works`) — Finding 1's own reproduction: `editions`
    // must be `["en"]`, and the cover must not be built from the withheld
    // hub.
    let list_response = split_router
        .clone()
        .oneshot(
            Request::get("/api/works")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let list_status = list_response.status();
    let list_body = crate::test_support::body_string(list_response).await;
    assert_eq!(list_status, StatusCode::OK, "{list_body}");
    let list_json: serde_json::Value = serde_json::from_str(&list_body).expect("valid JSON");
    let split_summary = list_json
        .as_array()
        .expect("a list")
        .iter()
        .find(|w| w["slug"].as_str() == Some(split_slug.as_str()))
        .expect("the split work is listed");
    let split_summary_editions: Vec<&str> = split_summary["editions"]
        .as_array()
        .expect("an editions array")
        .iter()
        .map(|e| e.as_str().expect("a string"))
        .collect();
    assert_eq!(
        split_summary_editions,
        vec!["en"],
        "/api/works is classified Filters but named an edition outside the \
         grant: {list_body}"
    );
    assert!(
        !split_summary["cover"]
            .as_str()
            .unwrap_or_default()
            .contains("/raw/"),
        "/api/works built a cover URL from the withheld edition: {list_body}"
    );
    proven_filters.insert("/api/works");

    // `/api/works/{work}` (`get_sidecar`) — the same document, filtered:
    // `editions` drops to `en`, and `hub` (`"raw"` in the fixture) falls
    // back to the one surviving edition rather than naming the withheld one.
    let sidecar_response = split_router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{split_slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let sidecar_status = sidecar_response.status();
    let sidecar_tag = sidecar_response.headers()[header::ETAG].clone();
    let sidecar_body = crate::test_support::body_string(sidecar_response).await;
    assert_eq!(sidecar_status, StatusCode::OK, "{sidecar_body}");
    let sidecar_json: serde_json::Value = serde_json::from_str(&sidecar_body).expect("valid JSON");
    let sidecar_editions: Vec<&str> = sidecar_json["editions"]
        .as_object()
        .expect("an editions object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        sidecar_editions,
        vec!["en"],
        "/api/works/{{work}} is classified Filters but named an edition \
         outside the grant: {sidecar_body}"
    );
    assert_eq!(
        sidecar_json["hub"].as_str(),
        Some("en"),
        "/api/works/{{work}} named the withheld hub: {sidecar_body}"
    );
    proven_filters.insert("/api/works/{work}");

    // `/api/works/{work}/alignments` (`patch_alignments`) — `RefusesAndFilters`.
    // Its write half was reclassified `Refuses` by Finding 1's
    // write-authorization fix: before it, the identical request answered
    // 200 with a sidecar filtered to `en` alone (the read-leak half, fixed
    // the round before this one) while still writing `alignment`/`run` rows
    // for `raw` — shared state every reader who *can* see `raw` would then
    // be served from. Now it is refused, like every `Refuses` entry, before
    // either `if_match_hits` or `Sidecar::correct` runs, and the seeded row
    // is unchanged. Its *response*-filtering half is proven separately just
    // below, against a work with two visible editions — this fixture keeps
    // only one, so a correction between two editions it may see cannot be
    // expressed here. Proven here (and below) rather than in the generic
    // `Refuses`/`Filters` loops above (all three explicitly skip this
    // pattern) because a correction needs a body and `If-Match`, which
    // their GET-only mechanism cannot send.
    let seeded_run = |conn: &rusqlite::Connection| -> (i64, i64, i64, i64) {
        conn.query_row(
            "SELECT a_start_page, a_end_page, b_start_page, b_end_page FROM run",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("the fixture's seeded run row exists")
    };
    let run_before = seeded_run(&split_state.library.conn());
    let patch_response = split_router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/works/{split_slug}/alignments"))
                .header("authorization", &credential)
                .header(header::IF_MATCH, sidecar_tag)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"from":"en","to":"raw","a":"1","b":"0"}"#))
                .unwrap(),
        )
        .await
        .expect("router answers");
    let patch_status = patch_response.status();
    let patch_body = crate::test_support::body_string(patch_response).await;
    assert_eq!(
        patch_status,
        StatusCode::NOT_FOUND,
        "/api/works/{{work}}/alignments is classified Refuses, but a \
         correction naming the withheld edition answered {patch_status}: {patch_body}"
    );
    let run_after = seeded_run(&split_state.library.conn());
    assert_eq!(
        run_before, run_after,
        "a correction refused for naming a withheld edition still wrote \
         shared alignment rows"
    );

    // `/api/works/{work}/alignments`'s `Filters` half: a correction between
    // two editions this caller *may* see must still answer with a response
    // that omits the third, withheld one — `one_work_three_editions_hub_withheld`'s
    // own doc comment says why `split_router` above cannot drive this case.
    let (triple_router, triple_state) =
        one_work_three_editions_hub_withheld("taiyaku-test-grant-posture-triple-filters");
    let triple_slug = triple_state.works[0].id.clone();
    let triple_get = triple_router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{triple_slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let triple_tag = triple_get.headers()[header::ETAG].clone();
    let triple_patch = triple_router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/works/{triple_slug}/alignments"))
                .header("authorization", &credential)
                .header(header::IF_MATCH, triple_tag)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"from":"en","to":"fr","a":"1","b":"0"}"#))
                .unwrap(),
        )
        .await
        .expect("router answers");
    let triple_status = triple_patch.status();
    let triple_body = crate::test_support::body_string(triple_patch).await;
    assert_eq!(
        triple_status,
        StatusCode::OK,
        "a correction between two visible editions must succeed: {triple_body}"
    );
    assert!(
        !triple_body.contains("\"raw\""),
        "/api/works/{{work}}/alignments is classified RefusesAndFilters but \
         named the withheld edition in its response: {triple_body}"
    );
    proven_filters.insert("/api/works/{work}/alignments");
    crate::test_support::cleanup("taiyaku-test-grant-posture-triple-filters");

    // `/api/library/works/{slug}` — `library_api::work` already reused
    // `browse::work`; this proves it, rather than merely trusting the read.
    let library_work_response = split_router
        .clone()
        .oneshot(
            Request::get(format!("/api/library/works/{split_slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let library_work_status = library_work_response.status();
    let library_work_body = crate::test_support::body_string(library_work_response).await;
    assert_eq!(library_work_status, StatusCode::OK, "{library_work_body}");
    let library_work_json: serde_json::Value =
        serde_json::from_str(&library_work_body).expect("valid JSON");
    let library_work_editions: Vec<&str> = library_work_json["editions"]
        .as_array()
        .expect("an editions array")
        .iter()
        .map(|e| e["key"].as_str().expect("a key"))
        .collect();
    assert_eq!(
        library_work_editions,
        vec!["en"],
        "/api/library/works/{{slug}} named an edition outside the grant: \
         {library_work_body}"
    );
    proven_filters.insert("/api/library/works/{slug}");

    // `/api/library/series/{id}` — the series is visible (`en` survives),
    // its one work must show only `en`.
    let library_series_response = split_router
        .clone()
        .oneshot(
            Request::get("/api/library/series/1")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let library_series_status = library_series_response.status();
    let library_series_body = crate::test_support::body_string(library_series_response).await;
    assert_eq!(
        library_series_status,
        StatusCode::OK,
        "{library_series_body}"
    );
    let library_series_json: serde_json::Value =
        serde_json::from_str(&library_series_body).expect("valid JSON");
    let series_work = library_series_json["works"]
        .as_array()
        .expect("a works array")
        .iter()
        .find(|w| w["slug"].as_str() == Some(split_slug.as_str()))
        .expect("the split work is listed under its series");
    let series_work_editions: Vec<&str> = series_work["editions"]
        .as_array()
        .expect("an editions array")
        .iter()
        .map(|e| e["key"].as_str().expect("a key"))
        .collect();
    assert_eq!(
        series_work_editions,
        vec!["en"],
        "/api/library/series/{{id}} named an edition outside the grant: \
         {library_series_body}"
    );
    proven_filters.insert("/api/library/series/{id}");

    // `/api/library/series/{id}/cover` — the series' hand-set cover is
    // unset, so this falls to `cover_target`'s own visible-only fallback:
    // the redirect must land on `en`, never on the withheld hub `raw`.
    let cover_response = split_router
        .clone()
        .oneshot(
            Request::get("/api/library/series/1/cover")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    assert_eq!(
        cover_response.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "/api/library/series/{{id}}/cover: {}",
        cover_response.status()
    );
    let cover_location = cover_response.headers()[header::LOCATION]
        .to_str()
        .expect("ascii")
        .to_string();
    assert!(
        cover_location.contains("/en/"),
        "/api/library/series/{{id}}/cover is classified Filters and must \
         still redirect to a granted edition: {cover_location}"
    );
    assert!(
        !cover_location.contains("/raw/"),
        "/api/library/series/{{id}}/cover redirected to the withheld \
         edition: {cover_location}"
    );
    proven_filters.insert("/api/library/series/{id}/cover");

    // `/api/library/home` — the split work is newly added and carries no
    // progress, so it surfaces in `added`; its `editions` must still be
    // `en`-only.
    let home_response = split_router
        .clone()
        .oneshot(
            Request::get("/api/library/home")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let home_status = home_response.status();
    let home_body = crate::test_support::body_string(home_response).await;
    assert_eq!(home_status, StatusCode::OK, "{home_body}");
    let home_json: serde_json::Value = serde_json::from_str(&home_body).expect("valid JSON");
    let home_work = home_json["added"]
        .as_array()
        .expect("an added list")
        .iter()
        .find(|w| w["slug"].as_str() == Some(split_slug.as_str()))
        .expect("the split work appears in `added`");
    let home_editions: Vec<&str> = home_work["editions"]
        .as_array()
        .expect("an editions array")
        .iter()
        .map(|e| e["key"].as_str().expect("a key"))
        .collect();
    assert_eq!(
        home_editions,
        vec!["en"],
        "/api/library/home named an edition outside the grant: {home_body}"
    );
    proven_filters.insert("/api/library/home");

    // `/api/library/series` (the grid) and `/api/library/search` both carry
    // only an *edition count* per series, not per-work detail —
    // `SeriesCard` has no field for either. The count is still the exact
    // number the whole-branch review found wrong before 2026-08-21
    // (`browse`'s module header, `aligned_clause`'s history), so both are
    // proven by asserting it reads 1 (the visible `en`) rather than 2 (`en`
    // and the withheld `raw`).
    let grid_response = split_router
        .clone()
        .oneshot(
            Request::get("/api/library/series")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let grid_status = grid_response.status();
    let grid_body = crate::test_support::body_string(grid_response).await;
    assert_eq!(grid_status, StatusCode::OK, "{grid_body}");
    let grid_json: serde_json::Value = serde_json::from_str(&grid_body).expect("valid JSON");
    let grid_series = grid_json["series"]
        .as_array()
        .expect("a series list")
        .iter()
        .find(|s| s["id"].as_i64() == Some(1))
        .expect("the split series is listed");
    assert_eq!(
        grid_series["editions"].as_i64(),
        Some(1),
        "/api/library/series counted an edition outside the grant: {grid_body}"
    );
    proven_filters.insert("/api/library/series");

    let search_response = split_router
        .clone()
        .oneshot(
            Request::get("/api/library/search?q=Split")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let search_status = search_response.status();
    let search_body = crate::test_support::body_string(search_response).await;
    assert_eq!(search_status, StatusCode::OK, "{search_body}");
    let search_json: serde_json::Value = serde_json::from_str(&search_body).expect("valid JSON");
    let search_series = search_json["series"]
        .as_array()
        .expect("a series list")
        .iter()
        .find(|s| s["id"].as_i64() == Some(1))
        .expect("the split series is a search hit");
    assert_eq!(
        search_series["editions"].as_i64(),
        Some(1),
        "/api/library/search counted an edition outside the grant: {search_body}"
    );
    proven_filters.insert("/api/library/search");

    // `/opds/v1.2/works` names no edition at all — only a count. That count
    // was `sidecar.editions.len()`, unfiltered by any round before Finding
    // 2 (this branch's report), so a restricted reader learned how many
    // editions a work held under a withheld root even though `opds::works`
    // named none of them directly. Fixed to `browse::visible_editions` —
    // the same bulk query `routes::list_works` already used for the
    // identical `/api/works` question — and proven against exactly this
    // split shape in `opds::tests::
    // a_restricted_readers_works_list_counts_only_the_editions_they_may_see`
    // rather than re-driven here: a count is the one field this loop's
    // string-membership assertions cannot express as cleanly as a direct
    // numeric comparison can.

    crate::test_support::cleanup("taiyaku-test-grant-posture-split");

    for (pattern, posture) in &table {
        if matches!(
            posture,
            GrantPosture::Filters | GrantPosture::RefusesAndFilters
        ) {
            assert!(
                proven_filters.contains(pattern),
                "{pattern} is classified Filters or RefusesAndFilters but \
                 this test never drove a request to prove its response is \
                 filtered"
            );
        }
    }
}

/// **Finding 1's reproduction.** `refuse_if_work_hidden` answered only the
/// *work*-level question — does `split` keep any edition under a granted
/// root? Yes, `en` does — and `get_sidecar` then returned `found.sidecar()`
/// unfiltered, every edition included. A reader granted only `en` learned
/// the withheld `raw` edition's key, label, member path and `sha256`.
#[tokio::test]
async fn get_sidecar_names_only_visible_editions() {
    let (router, state) =
        one_work_two_editions_split_across_roots("taiyaku-test-sidecar-split-leak");
    let slug = state.works[0].id.clone();
    let credential = crate::test_support::fixture_authorization();

    let response = router
        .oneshot(
            Request::get(format!("/api/works/{slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let status = response.status();
    let body = crate::test_support::body_string(response).await;
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        compact.contains("\"en\""),
        "the granted edition must still be named: {body}"
    );
    assert!(
        !compact.contains("\"raw\""),
        "GET /api/works/{{work}} named the withheld edition's key: {body}"
    );
    assert!(
        !body.contains("Raw"),
        "GET /api/works/{{work}} named the withheld edition's label: {body}"
    );
    assert!(
        !body.contains("c1.cbz"),
        "GET /api/works/{{work}} named the withheld edition's member path: {body}"
    );
    assert!(
        !compact.contains("\"sha256\":\"x\""),
        "GET /api/works/{{work}} named the withheld edition's member sha256: {body}"
    );
    crate::test_support::cleanup("taiyaku-test-sidecar-split-leak");
}

/// **Finding 1's reproduction**, for the list route. `list_works` filtered
/// by the now-removed `browse::visible_work_slugs` — work-level — and then
/// reported every edition of each surviving work, including a cover URL built
/// from `sidecar.hub` regardless of whether the hub itself was inside the
/// grant.
#[tokio::test]
async fn list_works_names_only_visible_editions_and_cover() {
    let (router, _state) =
        one_work_two_editions_split_across_roots("taiyaku-test-listworks-split-leak");
    let credential = crate::test_support::fixture_authorization();

    let response = router
        .oneshot(
            Request::get("/api/works")
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let status = response.status();
    let body = crate::test_support::body_string(response).await;
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        compact.contains("\"en\""),
        "the granted edition must still be named: {body}"
    );
    assert!(
        !compact.contains("\"raw\""),
        "GET /api/works named the withheld edition's key: {body}"
    );
    assert!(
        !body.contains("/raw/pages/"),
        "GET /api/works built a cover URL from the withheld edition: {body}"
    );
    crate::test_support::cleanup("taiyaku-test-listworks-split-leak");
}

/// **Finding 1's reproduction**, for the write itself rather than the
/// response. The response-leak fix round put `patch_alignments` on the same
/// work-level `visible_editions_or_refuse` helper `get_sidecar` uses and
/// filtered what came back — a reader granted only `en` passed it (`split`
/// keeps `en`), submitted a correction naming `raw` on either side, and got
/// back a 200 whose *response* no longer named `raw`. The write itself was
/// never asked: the correction was accepted and its rows landed in shared
/// `alignment`/`run` state regardless, which every other reader of this
/// work — including one who *can* see `raw` — is served from. Worse than a
/// read leak, because a read leak costs only the caller who triggered it; an
/// unauthorized write like this one costs every other reader a fact this
/// caller had no standing to assert.
#[tokio::test]
async fn patch_alignments_refuses_a_correction_naming_a_withheld_edition() {
    let (router, state) = one_work_two_editions_split_across_roots("taiyaku-test-patch-write-leak");
    let slug = state.works[0].id.clone();
    let credential = crate::test_support::fixture_authorization();

    let seeded_run = |conn: &rusqlite::Connection| -> (i64, i64, i64, i64) {
        conn.query_row(
            "SELECT a_start_page, a_end_page, b_start_page, b_end_page FROM run",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("the fixture's seeded run row exists")
    };
    let run_before = seeded_run(&state.library.conn());

    // The `If-Match` a real client would send comes from a real `GET`, the
    // same route the caller's own reader used.
    let get_response = router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let tag = get_response.headers()[header::ETAG].clone();

    let response = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/works/{slug}/alignments"))
                .header("authorization", &credential)
                .header(header::IF_MATCH, tag)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"from":"en","to":"raw","a":"1","b":"0"}"#))
                .unwrap(),
        )
        .await
        .expect("router answers");
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a correction naming a withheld edition must be refused before it \
         is written, not merely filtered on the way back out: {body}"
    );

    let run_after = seeded_run(&state.library.conn());
    assert_eq!(
        run_before, run_after,
        "PATCH /api/works/{{work}}/alignments wrote shared alignment rows \
         for a correction naming a withheld edition"
    );

    crate::test_support::cleanup("taiyaku-test-patch-write-leak");
}

/// **Finding 1** (verification pass, 2026-08-25): three properties on this
/// path that every one of the 371 tests existing before this one passed
/// without exercising. Each is proven below against
/// `one_work_three_editions_hub_withheld` — see that fixture's own doc
/// comment for why the two-edition split fixture cannot drive any of the
/// three — in its own test rather than as three assertions of one, **so
/// that one property failing cannot stop another from being checked at
/// all**. They started as a single test; a later review (Finding 5, this
/// branch's second report) found that shape meant a failure in (a) would
/// abort the test before (b) or (c) ever ran, and (c) — the `from`-side
/// write guard — has no other coverage anywhere. A regression there would
/// have looked, from the suite's output, exactly like an unrelated ETag
/// failure: one red test, not two, and the guard silently unproven. Split
/// so each reports independently regardless of what the others do.
///
/// (a) **The ETag is computed over the full, unfiltered document.** A
/// restricted reader's own `GET` → `If-Match` → `PATCH` round trip — on two
/// editions both inside their grant — must succeed. Mutating `get_sidecar`'s
/// `let tag = sidecar_etag(&sidecar)` to a tag over the filtered projection
/// instead makes this assertion fail with 412: the reader's `GET` hands back
/// a tag over `{en, fr}` alone, while `patch_alignments` always recomputes
/// its own comparison tag over the full (unfiltered) document, so the two
/// never agree again for a restricted reader.
#[tokio::test]
async fn a_restricted_readers_own_etag_round_trips_through_a_correction_between_visible_editions() {
    let (router, state) =
        one_work_three_editions_hub_withheld("taiyaku-test-grant-posture-triple-etag");
    let slug = state.works[0].id.clone();
    let credential = crate::test_support::fixture_authorization();

    let get_response = router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let tag = get_response.headers()[header::ETAG].clone();

    let patch_response = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/works/{slug}/alignments"))
                .header("authorization", &credential)
                .header(header::IF_MATCH, tag)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"from":"en","to":"fr","a":"1","b":"0"}"#))
                .unwrap(),
        )
        .await
        .expect("router answers");
    let patch_status = patch_response.status();
    let patch_body = crate::test_support::body_string(patch_response).await;
    assert_eq!(
        patch_status,
        StatusCode::OK,
        "a restricted reader's own If-Match, taken from their own GET, must \
         validate against a correction between two editions they can both \
         see: {patch_body}"
    );

    crate::test_support::cleanup("taiyaku-test-grant-posture-triple-etag");
}

/// (b) **`patch_alignments`'s response is filtered**, on a correction
/// between two *visible* editions (`en`/`fr`) so the request does not 404
/// before a response is even built. Mutating the `Some(keys) =>
/// filter_sidecar_editions(&next, keys).to_json()` arm to return the
/// unfiltered `json` instead makes this assertion fail: the 200 body then
/// names `raw`. See the doc comment above the `(a)` test, just above this
/// one, for why this is its own test rather than a second assertion tacked
/// onto that one: this property must still be checked (and reported) even
/// when `(a)` itself is broken.
#[tokio::test]
async fn a_correction_between_visible_editions_omits_the_withheld_one_from_its_response() {
    let (router, state) =
        one_work_three_editions_hub_withheld("taiyaku-test-grant-posture-triple-response-filter");
    let slug = state.works[0].id.clone();
    let credential = crate::test_support::fixture_authorization();

    let get_response = router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let tag = get_response.headers()[header::ETAG].clone();

    let patch_response = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/works/{slug}/alignments"))
                .header("authorization", &credential)
                .header(header::IF_MATCH, tag)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"from":"en","to":"fr","a":"1","b":"0"}"#))
                .unwrap(),
        )
        .await
        .expect("router answers");
    let patch_status = patch_response.status();
    let patch_body = crate::test_support::body_string(patch_response).await;
    assert_eq!(
        patch_status,
        StatusCode::OK,
        "a correction between two editions this reader may see must \
         succeed before its response can be checked for filtering: {patch_body}"
    );
    let patch_json: serde_json::Value = serde_json::from_str(&patch_body).expect("valid JSON");
    let patch_editions: std::collections::BTreeSet<&str> = patch_json["editions"]
        .as_object()
        .expect("an editions object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        patch_editions,
        std::collections::BTreeSet::from(["en", "fr"]),
        "PATCH /api/works/{{work}}/alignments named the withheld edition in \
         its response: {patch_body}"
    );

    crate::test_support::cleanup("taiyaku-test-grant-posture-triple-response-filter");
}

/// (c) **Both write guards are covered**, not only the one every existing
/// test drove. `patch_alignments_refuses_a_correction_naming_a_withheld_edition`
/// above only ever sent the withheld edition on `to`; this sends it on
/// `from` instead. Disabling `refuse_if_hidden(&correction.from)` — the
/// first of the two guard calls — makes this assertion fail: `raw → en` is a
/// real, seeded edge, so the correction would succeed (200) rather than 404.
/// This has no other coverage anywhere in the suite — which is exactly why
/// it is its own test rather than a third assertion after `(a)` and `(b)`:
/// nothing about this guard should depend on either of those passing first.
#[tokio::test]
async fn a_correction_naming_the_withheld_edition_on_the_from_side_is_still_refused() {
    let (router, state) =
        one_work_three_editions_hub_withheld("taiyaku-test-grant-posture-triple-from-guard");
    let slug = state.works[0].id.clone();
    let credential = crate::test_support::fixture_authorization();

    let get_response = router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{slug}"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let tag = get_response.headers()[header::ETAG].clone();

    let refused_response = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/works/{slug}/alignments"))
                .header("authorization", &credential)
                .header(header::IF_MATCH, tag)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"from":"raw","to":"en","a":"1","b":"0"}"#))
                .unwrap(),
        )
        .await
        .expect("router answers");
    let refused_status = refused_response.status();
    let refused_body = crate::test_support::body_string(refused_response).await;
    assert_eq!(
        refused_status,
        StatusCode::NOT_FOUND,
        "a correction naming the withheld edition on the `from` side must be \
         refused exactly as one naming it on `to` is: {refused_body}"
    );

    crate::test_support::cleanup("taiyaku-test-grant-posture-triple-from-guard");
}

/// **O86: an administrator is unrestricted in every dimension, and the grant
/// is dropped where the principal is built.**
///
/// `role`, `roots`, `can_download` and `can_stream` are four independent
/// columns, and spec A2 lists them side by side without saying whether they
/// compose. They do not: **role dominates.** An administrator edits rows no
/// grant can scope — `series_title` is keyed `(series_id, title)` and has no
/// per-work column to filter on — so an admin who can see only part of a
/// series can rename it for the whole library while only the works they can
/// see get their cached `.library.json` refreshed. That was O86, and this rule
/// is why it cannot happen rather than why it is refused.
///
/// **Here rather than at `taiyaku library user grant-root`**: all three
/// resolvers (API key, HTTP Basic, session cookie) build through
/// `from_account`, so this cannot be forgotten by a route, and it covers a
/// grant that already exists — including one held by an account promoted to
/// admin after it was granted.
#[test]
fn an_administrator_is_unrestricted_whatever_the_row_says() {
    let granted = taiyaku_library::account::Account {
        id: 7,
        name: "curator".into(),
        role: "admin".into(),
        can_download: false,
        can_stream: false,
    };
    let principal = Principal::from_account(&granted, vec![1, 2]);

    assert_eq!(principal.role, Role::Admin);
    assert!(
        principal.roots.is_empty(),
        "an admin's root grant is dropped — empty is what `browse` reads as \
         unrestricted, so this is the existing meaning rather than a new one"
    );
    assert!(principal.can_download, "and the capability flags follow it");
    assert!(principal.can_stream, "and the capability flags follow it");
}

/// The other half, and the one that fails if the rule is written as
/// "always unrestricted" rather than "unrestricted when admin".
#[test]
fn a_users_grant_and_capabilities_are_left_exactly_as_the_row_holds_them() {
    let granted = taiyaku_library::account::Account {
        id: 8,
        name: "reader".into(),
        role: "user".into(),
        can_download: false,
        can_stream: true,
    };
    let principal = Principal::from_account(&granted, vec![1, 2]);

    assert_eq!(principal.role, Role::User);
    assert_eq!(
        principal.roots,
        vec![1, 2],
        "a user's grant still restricts"
    );
    assert!(
        !principal.can_download,
        "and their flags are still their own"
    );
    assert!(principal.can_stream);
}
