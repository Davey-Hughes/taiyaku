use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine as _;
use taiyaku_library::account::Access;
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

    // **The guarded list, stated.** A tautology in a safety net is worse than
    // an obvious hand-typed list, so it is typed, like the sibling test's
    // exemption list, and the two together are what hold the line: a route
    // that moves between the lists fails one of them.
    let expected = [
        "/api/works",
        "/api/works/{work}",
        "/api/works/{work}/{edition}/pages/{index}",
        "/api/works/{work}/{edition}/pages/{index}/thumb",
        "/api/works/{work}/{edition}/book/images/{index}",
        "/api/works/{work}/{edition}/download",
        "/api/works/{work}/{edition}/members/{ordinal}/download",
        "/api/works/{work}/progress",
        "/api/works/{work}/notes",
        "/api/works/{work}/truth",
        "/api/works/{work}/book",
        "/api/works/{work}/book/pins",
        "/api/works/{work}/alignments",
        "/api/me",
        "/api/about",
        "/api/keys",
        "/api/keys/{id}",
        "/api/sessions",
        "/api/dictionary/{lang}/{word}",
        "/api/dictionary/{lang}/at/{text}",
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
        // Spec C3 §5's metadata-editing routes, admin-gated the same way — and
        // it matters doubly here too, since the guard is what stands between an
        // anonymous request and a `Principal`-less `administering_or_forbidden`
        // call. `/api/library/works/{slug}` and `/api/library/series/{id}`
        // repeat two paths already above: a `PATCH` merged onto the same `GET`
        // route, not a second route at the same address (`routes.rs`'s own
        // comment beside the registration).
        "/api/library/works/{slug}",
        "/api/library/series/{id}",
        "/api/library/editions/{id}",
        "/api/library/members/{id}",
        // Spec C3 §6's ComicInfo write-back — admin-gated, and gated again on
        // `can_stamp`, which an administrator does not inherit.
        "/api/library/works/{slug}/stamp-comicinfo",
        // Spec C4 §6's rule editor — admin-gated *inside* each handler, the
        // identical reasoning the proposals and metadata comments above give
        // for why it matters doubly that these are guarded here first.
        "/api/library/rules",
        "/api/library/rules/{id}",
        "/api/library/rules/preview",
        "/api/library/rules/import",
        // Spec C4 §2's roots routes — admin-gated *inside* each handler, the
        // identical reasoning the rules comment just above gives for why it
        // matters doubly that these are guarded here first.
        "/api/library/roots",
        "/api/library/roots/{id}",
        "/api/library/roots/{id}/move",
        // Spec C4 §3's scan-control route — admin-gated *inside* the handler,
        // the identical reasoning the roots comment just above gives for why it
        // matters doubly that it is guarded here first.
        "/api/library/roots/{id}/scan",
        "/api/library/libraries",
        "/api/library/libraries/{id}",
        "/api/library/libraries/{id}/dirs",
        "/api/library/libraries/{id}/dirs/{dir}",
        "/api/library/libraries/{id}/scan",
        "/api/library/scans",
        "/api/library/scans/{job}/cancel",
        "/api/library/derive",
        "/api/library/split-editions",
        "/api/library/dirs",
        // Spec C4 §4's user administration — admin-gated *inside* each
        // handler, the identical reasoning the scan comment just above gives,
        // and with the sharpest instance of it: these routes are the only
        // writer of `role`, `can_download`, `can_stream` and `user_library` in
        // the product, so an anonymous request reaching one at all would be
        // an anonymous request able to grant itself administration.
        "/api/library/users",
        "/api/library/users/{id}",
        "/api/library/users/{id}/sessions/{sid}",
        "/api/library/users/{id}/keys/{kid}",
        // Spec C4 §5's backup routes — admin-gated *inside* each handler, and
        // the guard matters most here of anywhere in this list: a backup is the
        // whole database, so an anonymous request reaching the download route
        // would be an anonymous download of every account's password hash.
        "/api/library/backups",
        "/api/library/backups/{name}",
        "/opds/v1.2/catalog",
        "/opds/v1.2/works",
        "/opds/v1.2/works/{work}",
        "/opds/v1.2/works/{work}/{edition}/pages/{number}",
        "/opds/v1.2/works/{work}/{edition}/cover",
        "/opds/v1.2/works/{work}/{edition}/download",
        "/opds/v1.2/libraries",
        "/opds/v1.2/libraries/{library}/works",
        "/opds/v1.2/libraries/by-work",
        "/opds/v1.2/libraries/{library}/works/by-work",
    ];
    assert_eq!(
        patterns, expected,
        "the guarded list changed; a route that left it must appear in \
         `open_routes` with a reason a guarded route could not serve it, which \
         `open_routes_are_the_ones_that_cannot_be_guarded` asserts"
    );

    for pattern in patterns {
        let path = concrete(pattern);
        let tmp = crate::test_support::Fixture::new("auth-enum");
        let state = Arc::new(crate::test_support::state(&tmp));
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

/// **The other half of the enumeration.**
///
/// `open_routes` is a by-name exemption from the guard.
///
/// This asserts the exemption is exactly the paths that *cannot* be guarded,
/// and says for each why — a login cannot already be authenticated, a logout
/// has to work from an expired session, and a claim is what creates the first
/// account. Anything else appearing here fails.
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
    let tmp = crate::test_support::Fixture::new("auth-open-login");
    let state = Arc::new(crate::test_support::state(&tmp));
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
    let tmp2 = crate::test_support::Fixture::new("auth-open-logout");
    let state = Arc::new(crate::test_support::state(&tmp2));
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
    let tmp3 = crate::test_support::Fixture::new("auth-open-claim");
    let state = Arc::new(crate::test_support::state(&tmp3));
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
    let tmp = crate::test_support::Fixture::new("auth-401-api");
    let state = Arc::new(crate::test_support::state(&tmp));
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

    let tmp2 = crate::test_support::Fixture::new("auth-401-opds");
    let state = Arc::new(crate::test_support::state(&tmp2));
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
    let tmp = crate::test_support::Fixture::new("auth-basic");
    let state = crate::test_support::state(&tmp);
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
/// The account's stored hash is replaced with one no password can ever verify
/// against. An API key that authenticates anyway therefore *cannot* have been
/// checked by argon2 — the only path that reaches argon2 is the one that would
/// have failed. A timing assertion would say the same thing more slowly and
/// less reliably; this one fails outright if a later refactor makes the key
/// path fall through to the password verifier.
#[tokio::test]
async fn an_api_key_authenticates_without_reaching_argon2() {
    let tmp = crate::test_support::Fixture::new("auth-key");
    let state = crate::test_support::state(&tmp);
    let secret = credential::mint_api_key();
    {
        let conn = state.library.conn();
        // **The proof rests on this line.** The account's stored hash is
        // replaced with a well-formed-looking string that argon2 cannot verify
        // anything against, so a key authenticating anyway cannot have been
        // checked by the password path.
        //
        // Not `LOCKED`: `by_api_key` refuses a locked account outright —
        // an unclaimed row must not be reachable by a key any more than by
        // a password — so locking it would prove the refusal rather than
        // the routing.
        conn.execute(
            "UPDATE user SET password_hash = '$argon2id$unverifiable' WHERE id = 1",
            [],
        )
        .expect("gives the account a hash nothing verifies against");
        assert!(
            !credential::verify_password_in(
                &mut credential::WorkingMemory::new(),
                "anything at all",
                "$argon2id$unverifiable"
            ),
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
        "an API key must authenticate against an account whose password nothing verifies"
    );

    // And a password for the same account is refused, so the success above is
    // the key path rather than a guard that stopped checking.
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
    let tmp = crate::test_support::Fixture::new("auth-key-name");
    let state = crate::test_support::state(&tmp);
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
        let tmp = crate::test_support::Fixture::new("auth-malformed");
        let state = Arc::new(crate::test_support::state(&tmp));
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
    let tmp = crate::test_support::Fixture::new("auth-404");
    let state = Arc::new(crate::test_support::state(&tmp));
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

/// **§7 names two paths for the limiter.** The design says it is "on the login
/// route *and on failed `Authorization: Basic` verification only*". Without the
/// second, a password is guessable at any rate through any guarded route while
/// ten tries at `POST /api/session` lock the pair out for fifteen minutes — so
/// the limiter would guard the one door a guesser has no reason to use.
///
/// Each guess also takes a `spawn_blocking` thread, so the limit bounds an
/// unauthenticated denial of service as much as a dictionary.
#[tokio::test]
async fn guessing_a_password_over_basic_is_rate_limited_too() {
    let tmp = crate::test_support::Fixture::new("auth-basic-lockout");
    let state = crate::test_support::state(&tmp);
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
    let tmp = crate::test_support::Fixture::new("auth-key-not-limited");
    let state = crate::test_support::state(&tmp);
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
        verifies: crate::session::VerifyLimit::default(),
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
/// `--trusted-proxy ::ffff:10.0.0.1` is a v6-written base that must match the
/// v4 peer it names; this test guards the slice panic `Cidr::from_str`'s
/// comment describes.
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
#[tokio::test]
async fn every_guarded_response_refuses_a_shared_cache() {
    for (pattern, _) in guarded_routes() {
        let path = concrete(pattern);
        let tmp = crate::test_support::Fixture::new("auth-cache");
        let state = crate::test_support::state(&tmp);
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
/// Stated as its own test because the answer is a security rule rather than a
/// detail of the enum; `By::Password`'s doc says why a password and a key are
/// separate variants.
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
/// is still allowed to speak for the request".
///
/// With the guard always true, an OPDS client that happens to carry a stale
/// cookie from a browser session on the same host is refused 403 for an origin
/// it never sent, while its perfectly valid `Authorization` header is never
/// looked at.
#[tokio::test]
async fn a_basic_header_still_speaks_past_a_cookie_refused_for_its_origin() {
    let tmp = crate::test_support::Fixture::new("crosssite-plus-basic");
    let state = Arc::new(crate::test_support::state(&tmp));
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
/// a `format!`.
///
/// It exists because `::ffff:10.0.0.0/104` is an IPv4 range written in IPv6 and
/// /104 there means /8 here — a person who typed one number is being told about
/// another. A plain IPv6 range must not get it, since it is simply false there.
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

    // **The case that separates `&&` from `||`.** Both halves agree on the
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

/// The decision and text behind the `X-Forwarded-For` warning.
///
/// The warning tells an operator that their login rate limit is being applied to
/// the proxy rather than to a reader. It must not fire for every request from
/// a direct client that sent no such header — a warning that cries wolf is the
/// one nobody reads on the day it is right. The peer it names is the value an
/// operator copies into `TAIYAKU_TRUSTED_PROXY`, so each case must carry it.
#[test]
fn the_forwarded_warning_fires_only_when_a_header_is_being_ignored() {
    use crate::auth::{IgnoredForwarded, ignored_forwarded_warning};
    let peer: std::net::IpAddr = "172.18.0.5".parse().expect("parses");
    let mut config = crate::session::AuthConfig::default();

    assert!(
        ignored_forwarded_warning(false, peer, &config).is_none(),
        "no header means nothing is being ignored, whatever the proxy list says"
    );
    let (case, text) = ignored_forwarded_warning(true, peer, &config)
        .expect("a header with no --trusted-proxy is exactly the case worth warning about");
    assert_eq!(case, IgnoredForwarded::NoProxy);
    assert!(text.contains("172.18.0.5"), "{text}");
    assert!(text.contains("TAIYAKU_TRUSTED_PROXY"), "{text}");
    assert!(text.contains("--trusted-proxy"), "{text}");

    config.trusted_proxies = vec!["10.0.0.0/8".parse().expect("parses")];
    assert!(
        ignored_forwarded_warning(false, peer, &config).is_none(),
        "no header, nothing ignored, with a list configured too"
    );
    let (case, text) = ignored_forwarded_warning(true, peer, &config)
        .expect("a header from a peer the list does not contain is being ignored");
    assert_eq!(case, IgnoredForwarded::NotListed);
    assert!(text.contains("172.18.0.5"), "{text}");
    assert!(text.contains("TAIYAKU_TRUSTED_PROXY"), "{text}");
    assert!(text.contains("--trusted-proxy"), "{text}");

    let trusted: std::net::IpAddr = "10.1.2.3".parse().expect("parses");
    assert!(
        ignored_forwarded_warning(true, trusted, &config).is_none(),
        "a header from a configured proxy is being honoured, not ignored"
    );
}

/// `failed` builds the 500 every account route answers when the database
/// refuses a write, so a password change that changed nothing is not answered
/// as success.
///
/// Reached by taking away the table `set_password` writes through, which is the
/// same error class the route reasons about — `serve` and `library import`
/// share one file — reproduced deterministically rather than by racing a writer.
#[tokio::test]
async fn a_password_change_the_database_refuses_is_not_answered_as_success() {
    let tmp = crate::test_support::Fixture::new("password-refused");
    let state = Arc::new(crate::test_support::state(&tmp));
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
// authentication already has: "exactly the failure a reviewer's checklist has:
// it goes stale the first time someone adds a route and does not think about
// auth." `routes::grant_postures` and the test below apply it to
// `Principal.access`, so a route cannot drift out of the grant unnoticed.
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

/// Directory 1 at `held` and directory 2 at `withheld`, each the one directory of a
/// library named after its leaf, the libraries numbered 70 and 80: apart
/// from the directories, so a handler that read a library's id as a directory's
/// finds no directory of that number rather than the one that happens to share
/// it.
fn two_dirs_numbered_apart_from_their_libraries(
    conn: &rusqlite::Connection,
    held: &str,
    withheld: &str,
) {
    let leaf = |path: &str| path.trim_start_matches('/').to_string();
    conn.execute(
        "INSERT INTO library (id, name) VALUES (70, ?1), (80, ?2)",
        [leaf(held), leaf(withheld)],
    )
    .expect("two libraries, numbered apart from their directories");
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, ?1), (2, ?2)",
        [held, withheld],
    )
    .expect("two directories");
    // Each in a library of its own.
    crate::test_support::link(conn, 70, 1);
    crate::test_support::link(conn, 80, 2);
}

/// The library `dir` is in, read from the store, and checked to be
/// numbered apart from every directory: a handler that read a library's id as a
/// directory's would otherwise find the directory of the same number and pass.
/// The same helper `opds::tests` defines, for the reason [`channel`] gives.
fn library_of(state: &ServeState, dir: i64) -> i64 {
    let conn = state.library.conn();
    let library = crate::test_support::library_of_root(&conn, dir);
    let dirs: Vec<i64> = conn
        .prepare("SELECT id FROM root")
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert!(
        !dirs.contains(&library),
        "library {library} shares a directory's id: {dirs:?}"
    );
    library
}

/// A router over one work (`"t"`, edition `"raw"`, two members holding a
/// real, decodable page each) and the one series (id 1) it belongs to —
/// both filed entirely under a root the fixture account is **not** granted,
/// so the work and the series it alone stands for are as wholly hidden as
/// each other.
///
/// What every `Refuses` entry in `routes::grant_postures` needs, from a
/// single fixture: the page, thumb, download and sidecar routes address the
/// work by slug; the OPDS mirrors address the same work; the
/// library routes address the work by slug and the series by id.
///
/// **Returns the state too (I8)**, since the four metadata PATCH routes need a
/// real edition id and member id to address, neither reachable from the slug
/// alone the way the read routes' own ids are.
///
/// **Real bytes, not placeholders.** `view::of_edition` and `view::of_work`
/// answer 404 before any of these routes opens a member, so the 404 asserted
/// here must come from that refusal and not from a file read that found
/// nothing.
/// With real, decodable images in place, a route that opened a member would
/// find a page there, which makes the loop below a proof rather than a
/// coincidence. `opds::tests::router_with_a_real_page_under_a_withheld_root`
/// insists on real bytes for the same reason.
///
/// Each directory is a library of its own, numbered apart from the directories
/// ([`two_dirs_numbered_apart_from_their_libraries`]), which the two OPDS
/// routes addressed by a library's id are driven against.
fn router_with_material_under_a_withheld_root(
    dir: &std::path::Path,
) -> (axum::Router, Arc<ServeState>) {
    use std::io::Write as _;

    let root = dir;
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

        two_dirs_numbered_apart_from_their_libraries(&conn, "/held", "/withheld");
        conn.execute("UPDATE member SET root_id = 2", [])
            .expect("files every member under the withheld root");
        // **O86: a grant only restricts a non-admin.** `Principal::from_account`
        // drops an administrator's roots, so a grant test whose holder is the
        // seeded admin measures nothing — it would pass against a filter that
        // had been deleted.
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a grant restricts a user, never an admin");
        conn.execute_batch(
            "INSERT INTO user_library (user_id, library_id)
             SELECT 1, library_id FROM library_root WHERE root_id = 1;
             UPDATE user SET every_library = 0 WHERE id = 1;",
        )
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
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        fingerprints: Default::default(),
        dirs: Vec::new(),
    });
    (build_router(state.clone()), state)
}

/// A router over two works — one filed under a root the fixture account
/// holds, one under a root it does not — for the `Filters` entries whose
/// contract is "name the granted work, omit the withheld one."
///
/// Root paths are `/visible-root` and `/hidden-root` rather than the
/// `/held`/`/withheld` convention `opds::tests` and `routes::tests` use
/// elsewhere in this crate. `library_api::status`'s proof below, and the
/// OPDS libraries feeds' beside it, check whether a library's *name*, which
/// this fixture takes from its directory's, is present in the body, and
/// `"withheld"` ends in `"held"` — an assertion that the body contains
/// `"held"` would pass whether or not `"withheld"` had also leaked. Distinct
/// roots avoid that trap rather than relying on a reader noticing it.
///
/// Each directory is a library of its own, numbered apart from the directories
/// ([`two_dirs_numbered_apart_from_their_libraries`]).
fn two_works_one_withheld(dir: &std::path::Path) -> (axum::Router, Arc<ServeState>) {
    let root = dir;
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

        two_dirs_numbered_apart_from_their_libraries(&conn, "/visible-root", "/hidden-root");
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
        conn.execute_batch(
            "INSERT INTO user_library (user_id, library_id)
             SELECT 1, library_id FROM library_root WHERE root_id = 1;
             UPDATE user SET every_library = 0 WHERE id = 1;",
        )
        .expect("grants only the visible root");
    }
    let works = crate::works::from_library(&library).expect("projects the imported works");
    let state = Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        fingerprints: Default::default(),
        dirs: Vec::new(),
    });
    (build_router(state.clone()), state)
}

/// A router over **one** work with two editions, each filed under a
/// different root — `en` under the root the fixture account holds, `raw`
/// under the one it does not. What `work_feed`'s `Filters` classification
/// needs: a work that is itself visible, with exactly one of its editions
/// outside the grant, mirroring `opds::tests::router_with_two_editions_split_across_roots`.
///
/// **Carries one `raw` → `en` alignment**, so a document that kept the
/// alignments of an edition outside the grant would name `raw` through it.
///
/// **Filed under series id 1**, its only work — added for
/// `every_route_declares_its_grant_posture`'s split-fixture proof of
/// `/api/library/series/{id}` and `/cover`, which need a series id to
/// address. Unlike `router_with_material_under_a_withheld_root`'s series,
/// this one is *not* wholly hidden: `en` survives the grant, so the series
/// itself is visible with one of its work's two editions filtered out —
/// the split case, at the series level rather than the work level.
///
/// Each directory is a library of its own, numbered apart from the directories
/// ([`two_dirs_numbered_apart_from_their_libraries`]), so the work is
/// O191's shape: an edition under each library's directory.
fn one_work_two_editions_split_across_roots(
    dir: &std::path::Path,
) -> (axum::Router, Arc<ServeState>) {
    let root = dir;
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
        .expect("seeds a raw-to-en alignment");
        let alignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO run \
                 (alignment_id, ordinal, a_start_page, a_end_page, b_start_page, b_end_page) \
             VALUES (?1, 0, 0, 1, 0, 1)",
            [alignment_id],
        )
        .expect("seeds the run pairing both pages straight across");
        two_dirs_numbered_apart_from_their_libraries(&conn, "/visible-root", "/hidden-root");
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
        conn.execute_batch(
            "INSERT INTO user_library (user_id, library_id)
             SELECT 1, library_id FROM library_root WHERE root_id = 1;
             UPDATE user SET every_library = 0 WHERE id = 1;",
        )
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
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        fingerprints: Default::default(),
        dirs: Vec::new(),
    });
    (build_router(state.clone()), state)
}

/// [`one_work_two_editions_split_across_roots`]'s shape for a book: two
/// editions, each an EPUB on disk, `raw` under the withheld root and `en`
/// under the granted one.
fn one_book_two_editions_split_across_roots(
    dir: &std::path::Path,
) -> (axum::Router, Arc<ServeState>) {
    let shelf = dir.to_string_lossy().into_owned();
    for (edition, text) in [("raw", "Withheld text."), ("en", "Granted text.")] {
        let folder = dir.join(edition);
        std::fs::create_dir_all(&folder).expect("mkdir");
        taiyaku_read::test_support::epub(&folder, "b.epub", &[text]);
    }
    let digest = |edition: &str| crate::test_support::sha256_of(&dir.join(edition).join("b.epub"));
    let (raw, en) = (digest("raw"), digest("en"));
    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Book", "reading": "ltr" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/raw/b.epub", "sha256": "{raw}", "pages": 1 }}
            ], "spreads": [] }},
            "en": {{ "label": "En", "members": [
              {{ "path": "{shelf}/en/b.epub", "sha256": "{en}", "pages": 1 }}
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
        two_dirs_numbered_apart_from_their_libraries(
            &conn,
            &format!("{shelf}/en"),
            &format!("{shelf}/raw"),
        );
        for (key, root) in [("raw", 2), ("en", 1)] {
            conn.execute(
                "UPDATE member SET root_id = ?1 WHERE edition_id = \
                 (SELECT id FROM edition WHERE work_id = ?2 AND key = ?3)",
                rusqlite::params![root, work_id, key],
            )
            .expect("files the edition under its root");
            conn.execute(
                "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
                 VALUES (?1, 'b.epub', 'file', 'epub-reflow', 1, 1)",
                [root],
            )
            .expect("the walk read it as a book");
        }
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a grant restricts a user, never an admin");
        conn.execute_batch(
            "INSERT INTO user_library (user_id, library_id)
             SELECT 1, library_id FROM library_root WHERE root_id = 1;
             UPDATE user SET every_library = 0 WHERE id = 1;",
        )
        .expect("grants only the visible root");
    }
    let works = crate::works::from_library(&library).expect("projects the imported work");
    let state = Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: dir.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        fingerprints: Default::default(),
        dirs: Vec::new(),
    });
    (build_router(state.clone()), state)
}

/// **The enumeration test for the grant:** without it, a route can drift out of
/// the grant and nothing will say so.
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
/// material. `GrantIndependent` entries are declared rather than driven,
/// because there is no grant behaviour to exercise. See `GrantPosture`'s own
/// doc comment.
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
    // an empty string quietly satisfying the type — `GrantIndependent`
    // exists so a reviewer reads a reason, not so the match arm compiles.
    for (pattern, posture) in &table {
        match posture {
            GrantPosture::GrantIndependent { reason } => assert!(
                !reason.is_empty(),
                "{pattern} is classified GrantIndependent but states no reason"
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
        driven, 28,
        "28 of the guarded routes must be classified Refuses, Filters or \
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
         made unrepresentable — so the proof is void rather than lost. Tasks \
         8, 11 and 12 all only ever added `GrantIndependent` routes, so none \
         of the three moved this count, and neither does C4 PR 2's user \
         administration for the same reason. **O60 raised it to 22**: progress, \
         notes and truth refuse a work outside the grant, where they had been \
         declared and not driven. F3's four OPDS routes raised it to 26, and \
         the alignments route is one fewer, `GrantIndependent` for O86's \
         reason: O58 makes it admin-only, so no caller of it holds a grant to \
         refuse against. The book route raised it to 26, `Filters` as the work \
         document it mirrors is, the scan feed to 27, `Filters` by library, and the \
         book's images route to 28, `Refuses` as the page route it stands beside does"
    );
    assert_eq!(
        table.len() - driven,
        45,
        "the remaining guarded routes must be GrantIndependent — \
         see the `driven` assertion just above, which this one is the other \
         half of. A new admin-only route is GrantIndependent: it changes who \
         may call it and not what any caller's grant would filter, so it moves \
         this count and leaves the driven one alone"
    );

    let credential = crate::test_support::fixture_authorization();

    // ---- `Refuses`: proven against a work, and the one series it belongs
    // to, both wholly outside the caller's grant. `RefusesAndFilters` shares
    // this proof for its refuse half, except where the pattern skip just below
    // sends a route to its own block. ----
    let tmp = crate::test_support::Fixture::new("grant-posture-refuses");
    let (refuses_router, refuses_state) = router_with_material_under_a_withheld_root(&tmp);
    for (pattern, posture) in &table {
        if !matches!(
            posture,
            GrantPosture::Refuses | GrantPosture::RefusesAndFilters
        ) {
            continue;
        }
        // **The four `library/{works,series,editions,members}` PATCH routes
        // (I8) are `GrantIndependent`** (O86), so the posture check above
        // already skips them, as it does the two `Filters` `GET`s that share a
        // path with two of them. All four are driven with real `PATCH` requests
        // just below instead.
        //
        // **The two OPDS library feeds' placeholder is a library, not a
        // work**, so they are driven in their own block below, against this
        // fixture's two libraries.
        if matches!(
            *pattern,
            "/api/library/works/{slug}"
                | "/api/library/series/{id}"
                | "/api/library/editions/{id}"
                | "/api/library/members/{id}"
                | "/opds/v1.2/libraries/{library}/works"
                | "/opds/v1.2/libraries/{library}/works/by-work"
        ) {
            continue;
        }
        let path = grant_proof_concrete(pattern);
        // **O133: the tile route's cover sizes are driven too.** With `?w=` at
        // a cover size, page 0 is served by `serve_page`, which asks no grant
        // of its own, so `get_thumb`'s `view::of_edition` is all that keeps a
        // withheld edition's cover hidden.
        let sized = (*pattern == "/api/works/{work}/{edition}/pages/{index}/thumb")
            .then(|| format!("{path}?w=768"));
        for path in std::iter::once(path).chain(sized) {
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
    }

    // **The two OPDS library feeds, addressed by a library's id.** The
    // withheld directory's library answers 404, as a library that does not
    // exist does. The held directory's library holds no material and answers
    // 200, which is what shows the 404 is the grant's and not a broken route.
    let refuses_held = library_of(&refuses_state, 1);
    let refuses_withheld = library_of(&refuses_state, 2);
    for pattern in [
        "/opds/v1.2/libraries/{library}/works",
        "/opds/v1.2/libraries/{library}/works/by-work",
    ] {
        for (library, expected) in [
            (refuses_withheld, StatusCode::NOT_FOUND),
            (refuses_held, StatusCode::OK),
        ] {
            let path = pattern.replace("{library}", &library.to_string());
            let response = refuses_router
                .clone()
                .oneshot(
                    Request::get(&path)
                        .header("authorization", &credential)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .expect("router answers");
            let status = response.status();
            let body = crate::test_support::body_string(response).await;
            assert_eq!(
                status, expected,
                "{pattern} is classified RefusesAndFilters, and GET {path} from an \
                 account granted library {refuses_held} answered {status}: {body}"
            );
        }
    }

    // **The four metadata PATCH routes, and what protects them — O86.**
    //
    // The fixture's work, edition, member and series are wholly hidden from the
    // fixture account, but these routes are admin-only, and
    // `Principal::from_account` drops an administrator's grant: nothing is
    // hidden from any caller these routes admit. The fixture above demotes the
    // grant holder to `user` precisely so its grant is live, which makes that
    // same account inadmissible here.
    //
    // So the block drives all four routes and asserts the guard that is
    // load-bearing for them: **the role gate refuses first**. Deleting it
    // outright would leave the four with no driven proof at all, which is how
    // a reclassification quietly becomes a hole.
    let refuses_slug = refuses_state.works.snapshot()[0].id.clone();
    let refuses_browser = crate::test_support::fixture_session_cookie(&refuses_state.library);
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
                    // **A session, not `credential` — O93.** Held fixed against the
                    // admitted half below so the pair varies only the *role*, which is
                    // what this test claims to attribute the status difference to.
                    .header("cookie", &refuses_browser)
                    .header("host", "test")
                    .header("origin", "http://test")
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

    // ---- `Filters`: proven against two works, and — for `work_feed`, whose
    // filtering is per-edition rather than per-work — two editions of one
    // work. A route classified `Filters` is marked proven here only after its
    // own assertions pass, and every `Filters` entry in the table must be
    // marked by the end, so a `Filters` route this loop forgot to drive fails
    // the test rather than silently passing as "declared."
    let mut proven_filters: std::collections::HashSet<&'static str> =
        std::collections::HashSet::new();

    let tmp2 = crate::test_support::Fixture::new("grant-posture-filters-a");
    let (works_router, works_state) = two_works_one_withheld(&tmp2);
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

    // `library_api::status` filters *libraries*, not works or editions — the
    // split fixture below is one root's worth of material and has nothing
    // to say about this route, so its proof stays here. Each of
    // `two_works_one_withheld`'s roots is a library of its own, named after
    // the directory, so the libraries carry the distinct names that fixture was
    // built with.
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
         granted library: {status_body}"
    );
    assert!(
        !status_body.contains("hidden-root"),
        "/api/library/status named a library outside the grant: {status_body}"
    );
    proven_filters.insert("/api/library/status");

    // The OPDS libraries, by directory and by work: each feed of libraries names
    // the granted directory's library and not the other, by name and by link,
    // and the granted library's works feed names its work and not the other.
    let works_visible = library_of(&works_state, 1);
    let works_hidden = library_of(&works_state, 2);
    let get_with_credential = async |uri: &str| -> (StatusCode, String) {
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
        (status, crate::test_support::body_string(response).await)
    };
    for pattern in ["/opds/v1.2/libraries", "/opds/v1.2/libraries/by-work"] {
        let (status, body) = get_with_credential(pattern).await;
        assert_eq!(status, StatusCode::OK, "{pattern}: {body}");
        assert!(
            body.contains("visible-root")
                && body.contains(&format!("/opds/v1.2/libraries/{works_visible}/")),
            "{pattern} is classified Filters and must still name the granted \
             library: {body}"
        );
        assert!(
            !body.contains("hidden-root")
                && !body.contains(&format!("/opds/v1.2/libraries/{works_hidden}/")),
            "{pattern} named a library outside the grant: {body}"
        );
        proven_filters.insert(pattern);
    }
    let (status, body) =
        get_with_credential(&format!("/opds/v1.2/libraries/{works_visible}/works")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("Visible Work") && !body.contains("Hidden Work"),
        "/opds/v1.2/libraries/{{library}}/works must name the work under the \
         library's directory and no other: {body}"
    );
    proven_filters.insert("/opds/v1.2/libraries/{library}/works");
    let (status, body) = get_with_credential(&format!(
        "/opds/v1.2/libraries/{works_visible}/works/by-work"
    ))
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("Visible Work"),
        "/opds/v1.2/libraries/{{library}}/works/by-work must still name the \
         granted work filed in the library: {body}"
    );
    assert!(
        !body.contains("Hidden Work"),
        "/opds/v1.2/libraries/{{library}}/works/by-work named a work whose only \
         edition is under the other library's directory: {body}"
    );
    proven_filters.insert("/opds/v1.2/libraries/{library}/works/by-work");

    // The scan feed: one job holding both directories shows on both
    // libraries' pages, and the caller is sent the granted library alone.
    {
        let roots = taiyaku_library::scan::roots(&works_state.library.conn()).expect("reads");
        let job = crate::scan_jobs::reserve(&works_state, &roots, std::time::Instant::now(), None)
            .expect("reserves both directories");
        *job.libraries.lock().expect("unpoisoned") = vec![
            (works_visible, "visible-root".to_string()),
            (works_hidden, "hidden-root".to_string()),
        ];
    }
    let (status, body) = get_with_credential("/api/library/scans").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sent: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    let libraries: Vec<&serde_json::Value> = sent["libraries"]
        .as_array()
        .expect("a libraries array")
        .iter()
        .map(|library| &library["id"])
        .collect();
    assert_eq!(
        libraries,
        [&serde_json::json!(works_visible)],
        "/api/library/scans is classified Filters and must name the granted \
         library and no other: {body}"
    );
    proven_filters.insert("/api/library/scans");

    // ---- The split-across-roots case, for every `Refuses` and `Filters`
    // entry that can usefully be driven against it. ----
    //
    // `two_works_one_withheld` and `router_with_material_under_a_withheld_root`
    // above only ever hide a *whole* work — the fixture shape where the
    // work-level question and the edition-level one have the same answer. One
    // fixture, reused below for both the `Refuses` and the `Filters` half,
    // so both proofs see the identical split.
    let tmp3 = crate::test_support::Fixture::new("grant-posture-split");
    let (split_router, split_state) = one_work_two_editions_split_across_roots(&tmp3);
    let split_slug = split_state.works.snapshot()[0].id.clone();

    // `Refuses`, again: the withheld half (`raw`) of a work that also keeps
    // a granted half (`en`) must still 404. Only the negative case is
    // asserted — `one_work_two_editions_split_across_roots` names members no
    // test writes to disk (it exists for the `Filters` proof below, which
    // never opens one), so asking for `en` and expecting 200 would answer
    // 404 from a missing file rather than from the grant.
    let mut names_a_work_only = 0_usize;
    for (pattern, posture) in &table {
        if !matches!(
            posture,
            GrantPosture::Refuses | GrantPosture::RefusesAndFilters
        ) {
            continue;
        }
        // See the identical skip in the wholly-hidden-work loop above: the
        // four metadata routes, or addressed by a library, and driven in their
        // own block above.
        if matches!(
            *pattern,
            "/api/library/works/{slug}"
                | "/api/library/series/{id}"
                | "/api/library/editions/{id}"
                | "/api/library/members/{id}"
                | "/opds/v1.2/libraries/{library}/works"
                | "/opds/v1.2/libraries/{library}/works/by-work"
        ) {
            continue;
        }
        // **A path with no `{edition}` names the work, and this account may
        // see the work**: `en` is theirs. Progress, notes and truth are the
        // three (O60), and what they must not do here is refuse. This is the
        // other half of the loop over the wholly hidden work above, which a
        // route refusing every caller with a grant would pass on its own.
        if !pattern.contains("{edition}") {
            let path = pattern.replace("{work}", &split_slug);
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
            assert!(
                response.status().is_success(),
                "{pattern} names a work and no edition, and GET {path} from an account \
                 granted one of that work's two editions answered {}",
                response.status()
            );
            names_a_work_only += 1;
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
    assert_eq!(
        names_a_work_only, 3,
        "three paths name a work and no edition — progress, notes and truth — and \
         each must answer success to an account granted one of the work's editions. \
         A different count means the branch that sends that request skipped one, or \
         a route was added that it now covers without being counted here"
    );

    // `patch_edition`/`patch_member`'s own split proof: an edition (and its one
    // member) sharing a work with a granted sibling. `edition_owner`/
    // `member_owner` check the edition/member's own root, not merely whether
    // the work has *some* visible material.
    //
    // **Unreachable here for the same reason as the block above — O86.** The
    // caller that makes this fixture's grant live is a non-admin, and these
    // routes admit only admins, whose grant is dropped. `edition_owner` and
    // `member_owner` still ask the edition's own root and that is deliberately
    // kept: it is the correct answer for any caller holding a grant, and the
    // day the role rule changes it is the difference between a leak and none.
    // It is simply unreachable through these routes, so what is asserted here
    // is the gate that does the refusing.
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

    // **The other half of the pair — O86.** With the grant not reaching these
    // routes, asserting 403 here too would collapse the pair into one assertion
    // made twice — which looks like coverage and is not.
    //
    // So the account is promoted back for this half. The pair reads
    // non-admin → 403, admin → 200, which is the property that actually holds:
    // **the role gate refuses exactly non-admins and nothing else.**
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
    // **The halves send different credentials — O93.** The refused half above
    // sends `credential` (an API key) as a *reader*; this half sends a session
    // as an *administrator*. That varies two things at once, so neither gate is
    // pinned by the pair: delete the role check from
    // `administering_or_forbidden` and the refused half still 403s, on the
    // credential instead.
    let browser = crate::test_support::fixture_session_cookie(&split_state.library);
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
                    .header("cookie", &browser)
                    .header("host", "test")
                    .header("origin", "http://test")
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
    // and an admin's is dropped (O86), so a promoted account would leave the
    // filter unreachable rather than broken. The state is shared across the
    // blocks of this test, so the role has to be put back.
    {
        let conn = split_state.library.conn();
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("restores the restricted caller for the proofs below");
    }

    // `Filters`, split.
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

    // `/api/works` (`list_works`) — `editions` must be `["en"]`, and the cover
    // must not be built from the withheld hub.
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

    // `/api/works/{work}/book` (`book::get_book`): of a book whose two
    // editions sit under the two roots, only the granted edition is sent, and
    // nothing of the withheld one's text.
    let tmp_book = crate::test_support::Fixture::new("grant-posture-book");
    let (book_router, book_state) = one_book_two_editions_split_across_roots(&tmp_book);
    let book_slug = book_state.works.snapshot()[0].id.clone();
    let book_response = book_router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{book_slug}/book"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let book_status = book_response.status();
    let book_body = crate::test_support::body_string(book_response).await;
    assert_eq!(book_status, StatusCode::OK, "{book_body}");
    let book_json: serde_json::Value = serde_json::from_str(&book_body).expect("valid JSON");
    let book_editions: Vec<&str> = book_json["editions"]
        .as_object()
        .expect("an editions object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(book_editions, vec!["en"], "{book_body}");
    assert_eq!(book_json["hub"].as_str(), Some("en"), "{book_body}");
    assert!(
        !book_body.contains("Withheld text"),
        "/api/works/{{work}}/book sent the withheld edition's text: {book_body}"
    );
    proven_filters.insert("/api/works/{work}/book");

    // `/api/library/works/{slug}` — `library_api::work` reuses `browse::work`;
    // this proves it, rather than merely trusting the read.
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

    // `/api/library/series` (the grid) and `/api/library/search` carry per
    // series an edition count and a `cover` address naming one work and one
    // of its editions, and both must be read through the grant: the count
    // reads 1 (the visible `en`), not 2, and the `cover` names the split
    // work's `en`.
    //
    // **The cover needs a candidate the grant must refuse.** The split work
    // alone cannot fail without the filter: its visible `en` sorts before the
    // withheld `raw`, so the unfiltered choice is the right one too. So a
    // second work is filed under the series with its one edition under the
    // withheld root, and is hand-set as the series' cover, which ranks it
    // first wherever it is a candidate. It is removed after the search proof,
    // so the proofs below read the fixture as built.
    let withheld_work: i64 = {
        let conn = split_state.library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Withheld", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "Raw", "members": [
                  {{ "path": "{}/raw/w1.cbz", "sha256": "w", "pages": 2 }}
                ], "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": []
            }}"#,
            tmp3.path().display()
        ))
        .expect("the withheld work's sidecar parses");
        let id = taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");
        conn.execute(
            "UPDATE member SET root_id = 2 WHERE edition_id IN \
             (SELECT id FROM edition WHERE work_id = ?1)",
            [id],
        )
        .expect("files its edition under the withheld root");
        conn.execute(
            "UPDATE work SET series_id = 1, volume = 2.0 WHERE id = ?1",
            [id],
        )
        .expect("files it under the split series");
        conn.execute("UPDATE series SET cover_work_id = ?1 WHERE id = 1", [id])
            .expect("hand-sets it as the series' cover");
        id
    };
    let withheld_slug: String = split_state
        .library
        .conn()
        .query_row(
            "SELECT slug FROM work WHERE id = ?1",
            [withheld_work],
            |r| r.get(0),
        )
        .expect("the withheld work's slug");
    // The split work's `en` member is `p` (`one_work_two_editions_split_across_roots`).
    let visible_cover = format!(
        "/api/works/{split_slug}/en/pages/0/thumb?v={}",
        crate::cover::cover_version("p", 0)
    );
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
    assert_eq!(
        grid_series["cover"].as_str(),
        Some(visible_cover.as_str()),
        "/api/library/series named a cover outside the grant (`{withheld_slug}`, \
         or `raw`): {grid_body}"
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
    assert_eq!(
        search_series["cover"].as_str(),
        Some(visible_cover.as_str()),
        "/api/library/search named a cover outside the grant (`{withheld_slug}`, \
         or `raw`): {search_body}"
    );
    proven_filters.insert("/api/library/search");
    {
        let conn = split_state.library.conn();
        conn.execute("UPDATE series SET cover_work_id = NULL WHERE id = 1", [])
            .expect("unsets the hand-set cover");
        conn.execute("DELETE FROM work WHERE id = ?1", [withheld_work])
            .expect("removes the withheld work");
    }

    // `/opds/v1.2/works` names no edition at all — only a count, read through
    // `browse::visible_editions` (the same bulk query `routes::list_works` uses
    // for the identical `/api/works` question) and proven against exactly this
    // split shape in `opds::tests::
    // a_restricted_readers_works_list_counts_only_the_editions_they_may_see`
    // rather than re-driven here: a count is the one field this loop's
    // string-membership assertions cannot express as cleanly as a direct
    // numeric comparison can.

    // `/api/works/{work}/progress` (`get_progress`) — `RefusesAndFilters`
    // since O60. Its refusal is driven by the first `Refuses` loop, against
    // the wholly hidden work. This is the other half: a position on `raw`,
    // written straight into the store as one held from before the grant was
    // narrowed would be, is not named to a reader granted `en` alone, and the
    // one on `en` is. Last of the proofs on this fixture, so no other reads a
    // store with these two rows in it.
    {
        let split_work = split_state
            .find_blocking(&split_slug)
            .expect("the split work");
        for edition in ["raw", "en"] {
            crate::progress::store(
                &split_state.library,
                &split_work,
                1,
                &crate::Progress {
                    edition: edition.to_string(),
                    slot: 1,
                },
                &split_work.sidecar(),
                taiyaku_library::state::Arrival::next(),
                crate::works::Mint::Bridge,
            )
            .expect("stores a position");
        }
    }
    let progress_response = split_router
        .clone()
        .oneshot(
            Request::get(format!("/api/works/{split_slug}/progress"))
                .header("authorization", &credential)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router answers");
    let progress_status = progress_response.status();
    let progress_body = crate::test_support::body_string(progress_response).await;
    assert_eq!(progress_status, StatusCode::OK, "{progress_body}");
    let progress_json: serde_json::Value =
        serde_json::from_str(&progress_body).expect("valid JSON");
    let progress_editions: Vec<&str> = progress_json["positions"]
        .as_array()
        .expect("a positions array")
        .iter()
        .map(|p| p["edition"].as_str().expect("a string"))
        .collect();
    assert_eq!(
        progress_editions,
        vec!["en"],
        "/api/works/{{work}}/progress is classified RefusesAndFilters but named a position \
         on an edition outside the grant: {progress_body}"
    );
    proven_filters.insert("/api/works/{work}/progress");

    // `/api/works/{work}/notes` and `/api/works/{work}/truth` — the read
    // leaves out a note or a truth entry on an edition outside the grant
    // (spec §3.8, O191), and names the one on `en`.
    {
        let split_work = split_state
            .find_blocking(&split_slug)
            .expect("the split work");
        let note = |edition: &str| crate::notes::Note {
            text: None,
            id: format!("01{edition}"),
            edition: edition.to_string(),
            slot: 0,
            region: None,
            color: None,
            body: "b".into(),
            created: "c".into(),
            updated: "u".into(),
            stale: false,
            at: None,
        };
        let notes = crate::notes::Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: split_slug.clone(),
            notes: vec![note("raw"), note("en")],
        };
        crate::notes::store(
            &split_state.library,
            &split_work,
            1,
            &notes,
            &split_work.sidecar(),
            crate::works::Mint::Bridge,
        )
        .expect("stores notes");
        let mut truth = taiyaku_core::TruthDoc::new();
        for edition in ["raw", "en"] {
            let looked_at = taiyaku_core::TruthEdition {
                members: Vec::new(),
                pages: std::collections::BTreeMap::from([(0, Vec::new())]),
            };
            truth.editions.insert(edition.to_string(), looked_at);
        }
        crate::truth::store(
            &split_state.library,
            &split_work,
            &truth,
            crate::works::Mint::Bridge,
        )
        .expect("stores truth");
    }
    for (pattern, named) in [
        ("/api/works/{work}/notes", "notes"),
        ("/api/works/{work}/truth", "truth"),
    ] {
        let response = split_router
            .clone()
            .oneshot(
                Request::get(format!("/api/works/{split_slug}/{named}"))
                    .header("authorization", &credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router answers");
        let status = response.status();
        let body = crate::test_support::body_string(response).await;
        assert_eq!(status, StatusCode::OK, "{pattern}: {body}");
        let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let editions: Vec<String> = if named == "notes" {
            json["notes"]
                .as_array()
                .expect("a notes array")
                .iter()
                .map(|n| n["edition"].as_str().expect("a string").to_string())
                .collect()
        } else {
            json["editions"]
                .as_object()
                .expect("an editions object")
                .keys()
                .cloned()
                .collect()
        };
        assert_eq!(
            editions,
            vec!["en"],
            "{pattern} is classified RefusesAndFilters but named an edition outside the \
             grant: {body}"
        );
        proven_filters.insert(pattern);
    }

    // The OPDS library feeds, split: one work with an edition under each
    // library's directory. Both of the granted library's feeds count the one
    // edition the grant allows: by directory because the other edition is
    // under the other library's directory, and by work because it is outside
    // the grant.
    let tmp5 = crate::test_support::Fixture::new("grant-posture-split-libraries");
    let (libraries_router, libraries_state) = one_work_two_editions_split_across_roots(&tmp5);
    let libraries_slug = libraries_state.works.snapshot()[0].id.clone();
    let granted_library = library_of(&libraries_state, 1);
    for (pattern, uri) in [
        (
            "/opds/v1.2/libraries/{library}/works",
            format!("/opds/v1.2/libraries/{granted_library}/works"),
        ),
        (
            "/opds/v1.2/libraries/{library}/works/by-work",
            format!("/opds/v1.2/libraries/{granted_library}/works/by-work"),
        ),
    ] {
        let response = libraries_router
            .clone()
            .oneshot(
                Request::get(&uri)
                    .header("authorization", &credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router answers");
        let status = response.status();
        let body = crate::test_support::body_string(response).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        let entry = body
            .split("<entry>")
            .find(|entry| entry.contains(&format!("<id>taiyaku:work:{libraries_slug}</id>")))
            .unwrap_or_else(|| panic!("{uri} must list the work: {body}"));
        assert!(
            entry.contains(r#"<content type="text">1 editions</content>"#),
            "{pattern} is classified RefusesAndFilters but counted an edition \
             outside the grant: {entry}"
        );
        proven_filters.insert(pattern);
    }

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
    // The other direction: a filter proof is only evidence for a route the
    // table says filters, so a route declared plain `Refuses` (or anything
    // else) while a proof was driven for it is an understated declaration.
    for pattern in &proven_filters {
        let (_, posture) = table
            .iter()
            .find(|(declared, _)| declared == pattern)
            .unwrap_or_else(|| panic!("{pattern} was proven filtered but is not in the table"));
        assert!(
            matches!(
                posture,
                GrantPosture::Filters | GrantPosture::RefusesAndFilters
            ),
            "{pattern} had its response proven filtered, but the table declares it \
             {posture:?}, not Filters or RefusesAndFilters"
        );
    }
}

/// One request as `reader` over the merged fixture: its status, `ETag` and
/// body, a redirect's `Location` after the body, as it names what is sent.
async fn asked(
    merged: &crate::test_support::EditionMergedFromTwoLibraries,
    reader: crate::test_support::Reader,
    method: axum::http::Method,
    uri: &str,
    if_match: Option<&str>,
    body: Option<String>,
) -> (StatusCode, Option<String>, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(tag) = if_match {
        request = request.header(header::IF_MATCH, tag);
    }
    if body.is_some() {
        request = request.header(header::CONTENT_TYPE, "application/json");
    }
    let response = merged
        .send(
            reader,
            request
                .body(body.map_or_else(Body::empty, Body::from))
                .unwrap(),
        )
        .await;
    let tag = response
        .headers()
        .get(header::ETAG)
        .map(|tag| tag.to_str().unwrap().to_string());
    let status = response.status();
    let location = response.headers().get(header::LOCATION).cloned();
    let mut body = response.into_body();
    if let Some(location) = location {
        body.extend_from_slice(location.as_bytes());
    }
    (status, tag, body)
}

/// **The enumeration test for narrowing** (spec §3.8, "Every route declares
/// it"): `routes::narrowing_postures`'s route set is `guarded_routes`'s, and
/// each declaration that is not `NoPages` is driven as Manga alone over
/// [`crate::test_support::router_over_an_edition_merged_from_two_libraries`],
/// whose `raw` Manga sees as part1 and part3, part2 (pages 2 to 4) hidden, so
/// a route that did not translate would answer Manga's page 2 with part2's.
#[tokio::test]
async fn every_route_declares_what_it_does_with_a_narrowed_edition() {
    use crate::routes::NarrowingPosture;
    use crate::test_support::Reader;
    use axum::http::Method;

    let guarded: Vec<&'static str> = guarded_routes().into_iter().map(|(path, _)| path).collect();
    let table = crate::routes::narrowing_postures();
    let declared: Vec<&'static str> = table.iter().map(|(path, _)| *path).collect();
    assert_eq!(
        declared, guarded,
        "routes::narrowing_postures's route set must equal guarded_routes's, in its order"
    );
    // The drive below walks `table` itself, so a route moved to `NoPages` is
    // not driven and failing but not driven at all; only a count it does not
    // compute can see it, as `every_route_declares_its_grant_posture`'s does.
    let counted = |posture: fn(&NarrowingPosture) -> bool| {
        table
            .iter()
            .filter(|(_, declared)| posture(declared))
            .count()
    };
    assert_eq!(
        [
            counted(|p| matches!(p, NarrowingPosture::Narrows)),
            counted(|p| matches!(p, NarrowingPosture::Translates)),
            counted(|p| matches!(p, NarrowingPosture::KeepsUnseen)),
        ],
        [18, 6, 1],
        "the Narrows, Translates and KeepsUnseen routes, each driven below: a route \
         moved between them, or added, without its count moving here is one this suite no \
         longer proves what it was declared to do"
    );
    assert_eq!(
        counted(|p| matches!(p, NarrowingPosture::NoPages { .. })),
        48,
        "the NoPages routes, declared and not driven: a route moved here from a driven posture \
         moves this count and one above, and a new route that reads no pages moves this alone"
    );

    let merged = crate::test_support::router_over_an_edition_merged_from_two_libraries();
    let slug = merged.slug.clone();
    let work = merged.state.find_blocking(&slug).expect("the work");
    let raw = work.sidecar().editions["raw"].clone();
    let pages: Vec<u32> = raw.members.iter().map(|member| member.pages).collect();
    assert_eq!(
        pages,
        [2, 3, 4],
        "the fixture: raw is part1, part2 and part3"
    );
    let part2 = raw.members[1].clone();
    assert!(
        part2.path.ends_with("part2.cbz"),
        "the fixture: Manga's hidden member is not the last: {}",
        part2.path
    );
    let series: i64 = merged
        .state
        .library
        .conn()
        .query_row(
            "SELECT series_id FROM work WHERE slug = ?1",
            [&slug],
            |row| row.get(0),
        )
        .expect("the work's series");

    // O60's walk's placeholders, Manga's page and member in its numbering and
    // the administrator's in the full one.
    let concrete = |pattern: &str, reader: Reader| -> String {
        let (page, member) = if reader == Reader::MangaOnly {
            ("2", "1")
        } else {
            ("5", "2")
        };
        let path = pattern
            .replace("{work}", &slug)
            .replace("{slug}", &slug)
            .replace("{edition}", "raw")
            .replace("{index}", page)
            .replace("{number}", page)
            .replace("{ordinal}", member)
            .replace("{id}", &series.to_string())
            .replace("{library}", "70");
        assert!(
            !path.contains('{'),
            "{pattern} carries a placeholder this walk has no value for: {path}"
        );
        if pattern == "/api/library/search" {
            format!("{path}?q=Foo")
        } else {
            path
        }
    };
    let names_part2 = |body: &[u8]| -> bool {
        let text = String::from_utf8_lossy(body);
        text.contains(&part2.sha256) || text.contains("part2")
    };

    // What `KeepsUnseen` reads back: a note on part2's page 3, marked, beside
    // ones Manga is sent.
    let note = |id: &str, edition: &str, slot: u32, body: &str| crate::notes::Note {
        text: None,
        id: id.into(),
        edition: edition.into(),
        slot,
        region: None,
        color: None,
        body: body.into(),
        created: "c".into(),
        updated: "u".into(),
        stale: false,
        at: None,
    };
    crate::notes::store(
        &merged.state.library,
        &work,
        merged.user_id(Reader::MangaOnly),
        &crate::notes::Notes {
            taiyaku_notes: taiyaku_library::NOTES_VERSION,
            work: slug.clone(),
            notes: vec![
                note("n1", "raw", 1, "on part1"),
                note("n2", "raw", 3, "on part2"),
                note("n3", "raw", 5, "on part3"),
                note("n4", "en", 0, "on en"),
            ],
        },
        &work.sidecar(),
        crate::works::Mint::Bridge,
    )
    .expect("stores notes");
    // A truth box there too, beside ones Manga is sent: the administrator's
    // truth read names part2 by it and by its `sha256`, so Manga's read
    // leaving `raw` out is seen (`Narrows`).
    let boxed = |text: &str| taiyaku_core::TruthRegion {
        x: 0.1,
        y: 0.1,
        w: 0.2,
        h: 0.2,
        axis: None,
        kind: taiyaku_core::Kind::Dialogue,
        ignore: false,
        text: Some(text.into()),
    };
    let entry = |key: &str, pages: Vec<(u32, &str)>| taiyaku_core::TruthEdition {
        members: work.sidecar().editions[key]
            .members
            .iter()
            .map(|member| taiyaku_core::RegionMember {
                sha256: member.sha256.clone(),
            })
            .collect(),
        pages: pages
            .into_iter()
            .map(|(page, text)| (page, vec![boxed(text)]))
            .collect(),
    };
    let mut truth = taiyaku_core::TruthDoc::new();
    truth.editions.insert(
        "raw".into(),
        entry("raw", vec![(3, "on part2"), (5, "on part3")]),
    );
    truth
        .editions
        .insert("en".into(), entry("en", vec![(0, "on en")]));
    crate::truth::store(
        &merged.state.library,
        &work,
        &truth,
        crate::works::Mint::Bridge,
    )
    .expect("stores truth");
    // Notes alone: they are the only `KeepsUnseen` route, the truth write
    // being an administrator's (O58).
    let held = || -> String {
        let notes = crate::notes::load(
            &merged.state.library,
            &work,
            merged.user_id(Reader::MangaOnly),
            &work.sidecar(),
        )
        .expect("reads notes")
        .expect("notes are held");
        crate::notes::to_json(&notes).expect("serialises")
    };

    let mut named_by_the_administrator = Vec::new();
    let mut alike_for_both = Vec::new();
    for (pattern, posture) in &table {
        if let NarrowingPosture::NoPages { reason } = posture {
            assert!(!reason.is_empty(), "{pattern} states no reason");
            continue;
        }
        let manga = concrete(pattern, Reader::MangaOnly);
        match posture {
            NarrowingPosture::NoPages { .. } => unreachable!("declared, not driven"),
            // Progress takes its slot in the body: Manga's slot 2 is stored
            // as the administrator's 5.
            NarrowingPosture::Translates if pattern.ends_with("/progress") => {
                for (reader, slot) in [(Reader::MangaOnly, 2), (Reader::Administrator, 5)] {
                    let position = format!(r#"{{"edition":"raw","slot":{slot}}}"#);
                    let (status, _, said) = asked(
                        &merged,
                        reader,
                        Method::PUT,
                        &concrete(pattern, reader),
                        None,
                        Some(position),
                    )
                    .await;
                    assert!(
                        status.is_success(),
                        "{pattern}, {reader:?}: {status} {}",
                        String::from_utf8_lossy(&said)
                    );
                }
                let stored = |reader: Reader| -> Vec<(String, u32)> {
                    crate::progress::load(
                        &merged.state.library,
                        &work,
                        merged.user_id(reader),
                        &work.sidecar(),
                    )
                    .expect("reads progress")
                    .into_iter()
                    .map(|position| (position.edition, position.slot))
                    .collect()
                };
                assert_eq!(
                    stored(Reader::MangaOnly),
                    stored(Reader::Administrator),
                    "{pattern}: Manga's slot 2 is stored as the administrator's 5"
                );
            }
            // The cover is page 0, which Manga's numbering shares with the
            // full one, so it is driven as Kids alone, whose page 0 is the
            // full page 2. No route sends that page at the cover's size, so
            // the two are compared by colour, every page having its own.
            NarrowingPosture::Translates
                if *pattern == "/opds/v1.2/works/{work}/{edition}/cover" =>
            {
                let full = concrete(pattern, Reader::Administrator);
                let (status, _, kids) =
                    asked(&merged, Reader::KidsOnly, Method::GET, &full, None, None).await;
                assert_eq!(status, StatusCode::OK, "{full}");
                let thumb = format!("/api/works/{slug}/raw/pages/2/thumb");
                let (status, _, page_2) = asked(
                    &merged,
                    Reader::Administrator,
                    Method::GET,
                    &thumb,
                    None,
                    None,
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{thumb}");
                let colour = |jpeg: &[u8]| -> [u8; 3] {
                    let image = image::load_from_memory(jpeg).expect("decodes").to_rgb8();
                    image.get_pixel(image.width() / 2, image.height() / 2).0
                };
                let (cover, page) = (colour(&kids), colour(&page_2));
                assert!(
                    cover.iter().zip(page).all(|(a, b)| a.abs_diff(b) <= 8),
                    "{pattern}: Kids' cover is {cover:?}, the full page 2 {page:?}"
                );
            }
            // Manga's page 2 (member 1) answers as the administrator's page 5
            // (member 2). A pattern naming no page or member would compare
            // one answer with itself, so it must be one of the cases above.
            NarrowingPosture::Translates => {
                assert!(
                    ["{index}", "{number}", "{ordinal}"]
                        .iter()
                        .any(|placeholder| pattern.contains(placeholder)),
                    "{pattern} translates but names no page or member, so this drive cannot \
                     tell it from a route that does not"
                );
                let (status, _, mangas) =
                    asked(&merged, Reader::MangaOnly, Method::GET, &manga, None, None).await;
                assert_eq!(status, StatusCode::OK, "{manga}");
                let full = concrete(pattern, Reader::Administrator);
                let (_, _, administrators) = asked(
                    &merged,
                    Reader::Administrator,
                    Method::GET,
                    &full,
                    None,
                    None,
                )
                .await;
                assert!(
                    mangas == administrators,
                    "{pattern}: Manga's {manga} does not answer as the administrator's {full}"
                );
            }
            // Manga's answer names neither part2's `sha256` nor its file, and
            // differs from Both's, which sees `raw` whole through
            // `Access::Only` and otherwise as Manga does. Which of the
            // administrator's answers name part2, and which routes answer the
            // two alike, are pinned below, so a route whose answer stops
            // proving anything here shows.
            NarrowingPosture::Narrows => {
                let (status, _, body) =
                    asked(&merged, Reader::MangaOnly, Method::GET, &manga, None, None).await;
                assert!(
                    status == StatusCode::OK || status == StatusCode::TEMPORARY_REDIRECT,
                    "{manga}: {status}"
                );
                assert!(
                    !names_part2(&body),
                    "{pattern}: Manga's answer names part2: {}",
                    String::from_utf8_lossy(&body)
                );
                let (_, _, administrators) = asked(
                    &merged,
                    Reader::Administrator,
                    Method::GET,
                    &manga,
                    None,
                    None,
                )
                .await;
                if names_part2(&administrators) {
                    named_by_the_administrator.push(*pattern);
                }
                let (boths_status, _, boths) =
                    asked(&merged, Reader::Both, Method::GET, &manga, None, None).await;
                assert_eq!(boths_status, status, "{pattern}: Both's answer to {manga}");
                if body == boths {
                    alike_for_both.push(*pattern);
                }
            }
            // Manga's read sends nothing of part2's pages, and a write of what
            // it sent leaves the store as it was.
            NarrowingPosture::KeepsUnseen => {
                let before = held();
                let (status, tag, body) =
                    asked(&merged, Reader::MangaOnly, Method::GET, &manga, None, None).await;
                let text = String::from_utf8_lossy(&body).into_owned();
                assert_eq!(status, StatusCode::OK, "{manga}: {text}");
                assert!(
                    !text.contains("on part2") && !names_part2(&body),
                    "{pattern}: Manga is sent part2's pages: {text}"
                );
                let (status, _, said) = asked(
                    &merged,
                    Reader::MangaOnly,
                    Method::PUT,
                    &manga,
                    tag.as_deref(),
                    Some(text),
                )
                .await;
                assert_eq!(
                    status,
                    StatusCode::OK,
                    "{pattern}: {}",
                    String::from_utf8_lossy(&said)
                );
                assert_eq!(
                    held(),
                    before,
                    "{pattern}: a write of what Manga was sent changed the store"
                );
            }
        }
    }
    assert_eq!(
        named_by_the_administrator,
        ["/api/works/{work}", "/api/works/{work}/truth"],
        "the Narrows routes whose administrator's answer names part2, so that Manga's \
         leaving it out is seen: the work document, and the truth document, whose `raw` entry \
         holds part2's `sha256` and the box \"on part2\" and is left out of Manga's; the others \
         send counts and covers, which name no member"
    );
    assert_eq!(
        alike_for_both,
        [
            "/api/works",
            "/api/library/series/{id}/cover",
            "/opds/v1.2/works",
            "/opds/v1.2/libraries/{library}/works",
            "/opds/v1.2/libraries/{library}/works/by-work",
        ],
        "the Narrows routes whose answer to Manga is the same as to Both: lists naming a work's \
         editions or counting them, and a cover redirect naming page 0 of the hub, none naming \
         a member, a page or a page count"
    );
}

/// **A reader granted only `en` learns nothing of the withheld `raw` edition
/// from `get_sidecar`**: not its key, label, member path or `sha256`.
#[tokio::test]
async fn get_sidecar_names_only_visible_editions() {
    let tmp = crate::test_support::Fixture::new("sidecar-split-leak");
    let (router, state) = one_work_two_editions_split_across_roots(&tmp);
    let slug = state.works.snapshot()[0].id.clone();
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
}

/// **The list route names only the granted edition, and builds no cover URL
/// from the withheld one.**
#[tokio::test]
async fn list_works_names_only_visible_editions_and_cover() {
    let tmp = crate::test_support::Fixture::new("listworks-split-leak");
    let (router, _state) = one_work_two_editions_split_across_roots(&tmp);
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
/// **Here rather than at `taiyaku library user grant-library`**: all three
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
        can_stamp: false,
    };
    let principal = Principal::from_account(&granted, Access::roots(vec![1, 2]));

    assert_eq!(principal.role, Role::Admin);
    assert_eq!(
        principal.access,
        Access::Every,
        "an admin's grant is dropped for `Every`, the one access `browse` \
         applies no filter for"
    );
    assert!(principal.can_download, "and the capability flags follow it");
    assert!(principal.can_stream, "and the capability flags follow it");
    assert!(
        !principal.can_stamp,
        "except `can_stamp`, which O86 does not reach — spec C3 §6 and \
         `2026-09-07-comicinfo-write-back-design.md` §1.1. Stated here, in \
         O86's own test, so the rule and its one exception are read together"
    );
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
        can_stamp: false,
    };
    let principal = Principal::from_account(&granted, Access::roots(vec![1, 2]));

    assert_eq!(principal.role, Role::User);
    assert_eq!(
        principal.access,
        Access::roots(vec![1, 2]),
        "a user's grant still restricts"
    );
    assert!(
        !principal.can_download,
        "and their flags are still their own"
    );
    assert!(principal.can_stream);
}

/// A grant names a library, and the principal carries every directory of it:
/// both of `Shelf`'s and not `Other`'s, whether a key, a password or a session
/// cookie resolved it, and `Shelf` (70) as the library granted.
#[test]
fn a_principal_holds_every_dir_of_a_granted_library() {
    let tmp = crate::test_support::Fixture::new("auth-granted-library");
    let (_router, state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);
    let client = std::net::IpAddr::from([127, 0, 0, 1]);
    let shelf = Access::Only {
        roots: vec![1, 2],
        libraries: vec![70],
    };

    let super::Resolved::Yes(by_key, super::By::Key) = super::from_basic(
        &state,
        &crate::test_support::fixture_authorization(),
        client,
    ) else {
        panic!("the fixture's key resolves to its account");
    };
    assert_eq!(by_key.access, shelf, "by key");

    let password = basic(
        crate::test_support::FIXTURE_NAME,
        crate::test_support::FIXTURE_PASSWORD,
    );
    let super::Resolved::Yes(by_password, super::By::Password) =
        super::from_basic(&state, &password, client)
    else {
        panic!("the fixture's password resolves to its account");
    };
    assert_eq!(by_password.access, shelf, "by password");

    let cookie = crate::test_support::fixture_session_cookie(&state.library);
    let token = cookie
        .split_once('=')
        .expect("a `name=token` cookie")
        .1
        .to_string();
    let crate::session::FromCookie::Resolved(by_cookie) =
        crate::session::from_cookie(&state, &token, &axum::http::Method::GET, None, None)
    else {
        panic!("the fixture's session resolves to its account");
    };
    assert_eq!(by_cookie.access, shelf, "by session cookie");
}

/// **No module answers the role question alone — O93.**
///
/// `session::administering_or_forbidden` is the only function that answers
/// "may this caller administer": it refuses a device credential as well as a
/// non-admin, which a copy reading `principal.role` alone would not. This test
/// is what stops a second copy appearing. A grep rather than a request,
/// deliberately: what it catches is the *existence* of a second answer, which
/// no single request can see.
#[test]
fn no_module_answers_the_role_question_alone() {
    let sources = [
        ("config_api.rs", include_str!("../config_api.rs")),
        ("curation.rs", include_str!("../curation.rs")),
        ("metadata.rs", include_str!("../metadata.rs")),
        ("users_api.rs", include_str!("../users_api.rs")),
        ("backup_api.rs", include_str!("../backup_api.rs")),
        ("routes.rs", include_str!("../routes.rs")),
        ("library_api.rs", include_str!("../library_api.rs")),
    ];

    for (name, source) in sources {
        assert!(
            !source.contains("fn admin_or_forbidden"),
            "{name} defines its own `admin_or_forbidden`. That function answered \
             only half the question — `session::administering_or_forbidden` \
             answers both, and O93 is what three copies of the half cost"
        );
        assert!(
            !source.contains("principal.role != Role::Admin")
                && !source.contains("principal.role != crate::auth::Role::Admin"),
            "{name} compares a role to decide an administrative refusal. Call \
             `session::administering_or_forbidden`, which also refuses a device \
             credential — O93"
        );
    }
}

/// The one guarded route this test cannot classify, and why.
///
/// `PUT /api/password` takes `Json<PasswordChange>`, and an extractor runs
/// **before** the handler — so the `{}` body this test sends is rejected 422
/// by `axum` and the credential gate is never reached. It is not an
/// exemption: `session::tests::an_api_key_cannot_mint_keys_or_end_sessions`
/// drives it with a real key and a real body, alongside `/api/keys`
/// and `/api/sessions`.
///
/// Listed rather than skipped silently, so a second route joining it is a
/// deliberate act with a reason beside it.
const CLASSIFIED_ELSEWHERE: &[&str] = &["/api/password"];

/// Every guarded route observed to refuse an API key.
///
/// **Not "administrative":** spec A2's own self-service account routes refuse
/// a device credential too, for a different reason (a key may not manage the
/// account it belongs to), and naming the set after one of its two reasons
/// would invite leaving the other out. The invariant is about the answer, not
/// the motive.
const REFUSE_A_DEVICE_CREDENTIAL: &[&str] = &[
    "/api/works/{work}/truth",
    "/api/works/{work}/book/pins",
    "/api/works/{work}/alignments",
    "/api/keys",
    "/api/keys/{id}",
    "/api/sessions",
    "/api/sessions/{id}",
    "/api/library/rules",
    "/api/library/rules/{id}",
    "/api/library/rules/preview",
    "/api/library/rules/import",
    "/api/library/roots",
    "/api/library/roots/{id}",
    "/api/library/roots/{id}/move",
    "/api/library/roots/{id}/scan",
    "/api/library/libraries",
    "/api/library/libraries/{id}",
    "/api/library/libraries/{id}/dirs",
    "/api/library/libraries/{id}/dirs/{dir}",
    "/api/library/libraries/{id}/scan",
    "/api/library/scans/{job}/cancel",
    "/api/library/derive",
    "/api/library/split-editions",
    "/api/library/dirs",
    "/api/library/proposals",
    "/api/library/proposals/{id}",
    "/api/library/proposals/{id}/resolution",
    "/api/library/proposals/{id}/accept",
    "/api/library/proposals/{id}/dismiss",
    "/api/library/works/{slug}",
    "/api/library/series/{id}",
    "/api/library/editions/{id}",
    "/api/library/members/{id}",
    "/api/library/works/{slug}/stamp-comicinfo",
    "/api/library/users",
    "/api/library/users/{id}",
    "/api/library/users/{id}/sessions/{sid}",
    "/api/library/users/{id}/keys/{kid}",
    "/api/library/backups",
    "/api/library/backups/{name}",
];

/// **Every guarded route is driven with an API key, and the set that refuses
/// one must be exactly the set declared here.**
///
/// Derived from [`crate::routes::guarded_routes`] rather than hand-typed or
/// counted per module, and that is the whole point: a check that enumerates
/// modules repeats the mistake O93 was — five modules each looking fine on
/// their own terms.
#[tokio::test]
async fn exactly_the_declared_routes_refuse_an_api_key() {
    let (_router, state, _fixture) = crate::test_support::router_over_a_single_scanned_work();
    let router = crate::test_support::key_authenticating(crate::build_router(state.clone()));

    // Every guarded route is driven with a real key; whichever answer the
    // credential gate gives is observed, not assumed; and the result is
    // compared for **set equality** with `REFUSE_A_DEVICE_CREDENTIAL`. A new
    // route that refuses a key without being listed fails here, and so does one
    // that *stops* refusing — including a whole new module nobody thought to
    // add to a list.
    let mut refused: Vec<&'static str> = Vec::new();
    for (pattern, _) in crate::routes::guarded_routes() {
        if CLASSIFIED_ELSEWHERE.contains(&pattern) {
            continue;
        }
        // **A path parameter is filled with `1`, and the value does not
        // matter.** The gate runs before any id is looked up, so an
        // administrative route answers 403 for the credential whatever id it
        // was handed, and a non-administrative one answers 401/404/whatever —
        // never the credential refusal. That is what makes one substitution
        // enough for every pattern.
        let uri = pattern.replace("{slug}", "1").replace("{sid}", "1");
        let uri = regex_free_fill(&uri);

        // **Every method this server registers, `PUT` included.** A route
        // registered for only some methods answers 405 for the rest, which is
        // not the credential refusal and so does not classify it.
        let mut saw_refusal = false;
        for method in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(&uri)
                        .header("host", "test")
                        .header("origin", "http://test")
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .expect("router answers");
            let status = response.status();
            let body = crate::test_support::body_string(response).await;
            if status == StatusCode::FORBIDDEN && body.contains("API key") {
                saw_refusal = true;
            }
        }
        if saw_refusal {
            refused.push(pattern);
        }
    }

    let mut declared: Vec<&str> = REFUSE_A_DEVICE_CREDENTIAL.to_vec();
    declared.sort_unstable();
    refused.sort_unstable();
    refused.dedup();

    assert_eq!(
        refused, declared,
        "the routes that refuse an API key are not the routes declared to. A \
         route that appears in the left column and not the right is newly \
         administrative and wants adding; one in the right and not the left \
         has STOPPED refusing a device credential, which is O93"
    );
}

/// Replace every remaining `{…}` path parameter with `1`.
///
/// Written by hand rather than with `regex`, which is not a dependency of this
/// crate — the same reason `config_api` reaches for `rules::Engine::compile`
/// instead of `Regex::new`.
fn regex_free_fill(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut skipping = false;
    for c in pattern.chars() {
        match c {
            '{' => {
                skipping = true;
                out.push('1');
            }
            '}' => skipping = false,
            _ if skipping => {}
            _ => out.push(c),
        }
    }
    out
}

/// **The one exception to O86, pinned here rather than asserted in a comment.**
///
/// [`Principal::from_account`] folds `can_download`, `can_stream` and `roots`
/// into the admin role — an administrator is unrestricted in all three
/// whatever the columns say. `can_stamp` is not folded, because O86's argument
/// is about an administrator editing shared rows they cannot see, and no
/// half-sighted state follows from withholding a per-work file rewrite: a
/// stamp names one work and either happens or does not. Spec C3 §6/§7 and
/// `docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md` §1.1.
///
/// The three assertions below fail in opposite directions, so a change that
/// folds `can_stamp` in alongside the others cannot satisfy all three.
#[test]
fn an_administrator_does_not_inherit_can_stamp() {
    let account = taiyaku_library::account::Account {
        id: 1,
        name: "root".to_string(),
        role: "admin".to_string(),
        can_download: false,
        can_stream: false,
        can_stamp: false,
    };

    let principal = Principal::from_account(&account, Access::Every);

    assert!(principal.can_download, "O86: role dominates can_download");
    assert!(principal.can_stream, "O86: role dominates can_stream");
    assert!(
        !principal.can_stamp,
        "an administrator holding can_stamp = 0 may not stamp"
    );
}

/// The other half: the column is what grants it, for an administrator too.
#[test]
fn a_granted_administrator_may_stamp() {
    let account = taiyaku_library::account::Account {
        id: 1,
        name: "root".to_string(),
        role: "admin".to_string(),
        can_download: true,
        can_stream: true,
        can_stamp: true,
    };

    assert!(Principal::from_account(&account, Access::Every).can_stamp);
}

/// And for a reader, where nothing was ever folded in.
#[test]
fn a_reader_may_stamp_only_when_the_column_says_so() {
    let mut account = taiyaku_library::account::Account {
        id: 2,
        name: "reader".to_string(),
        role: "user".to_string(),
        can_download: true,
        can_stream: true,
        can_stamp: false,
    };
    assert!(!Principal::from_account(&account, Access::Every).can_stamp);

    account.can_stamp = true;
    assert!(Principal::from_account(&account, Access::Every).can_stamp);
}

/// **`auth::guard` resolves a request behind `spawn_blocking`, not on the async
/// worker** — which is what this test observes.
///
/// Its request passes this middleware first, and the guard's own
/// `spawn_blocking` takes the mutex wait onto a blocking thread, so the canary
/// runs and the watchdog lets go before `get_progress` touches SQLite. The test
/// fails if the guard's resolution moves onto the async worker, and not if only
/// `get_progress`'s closure does (`docs/notes.md`, "O71"). Whether each handler
/// does its own work off the async worker is
/// `routes::tests::no_works_route_stalls_the_runtime`, which calls them
/// directly. **Do not model a handler's stall test on this one.**
///
/// `Library::conn()` returns a `MutexGuard<'_, Connection>` from a
/// `Mutex<Connection>` (`taiyaku-library/src/lib.rs`): a lock a spawned thread
/// can hold for a controlled span, and any handler that calls `library.conn()`
/// inside its `spawn_blocking` closure blocks on it for as long as the guard is
/// held. `get_progress` does exactly that (`crate::progress::load` →
/// `library.conn()`), so the mutex serves as the controlled block.
///
/// **The block's release is gated on the canary, not a fixed sleep.** A fixed
/// sleep shorter than the watchdog's own deadline would let an unwrapped
/// handler's block happen to end before the deadline fires, passing for the
/// wrong reason. Here the watchdog thread is the one holding `library.conn()`:
/// it waits for the canary or half a second, whichever comes first, and only
/// then drops the guard — so how long the block actually lasts is decided by
/// whether the reactor was ever free to run anything else, which is the
/// property under test, rather than by a duration picked in advance.
#[tokio::test(flavor = "current_thread")]
async fn the_guard_does_not_stall_the_runtime() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let (router, state, _fixture) = crate::test_support::router_over_a_library().await;

    // A position to read back, so the eventual read proves more than "no
    // content" — it proves the value written before the block still comes
    // back correctly once the block lifts.
    let put = router
        .clone()
        .oneshot(
            Request::put("/api/works/w/progress")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"edition":"raw","slot":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    let canary_ran = Arc::new(AtomicBool::new(false));
    let rescued_by_deadline = Arc::new(AtomicBool::new(false));
    let (lock_taken_tx, lock_taken_rx) = std::sync::mpsc::channel();

    let watchdog = {
        let library = Arc::clone(&state.library);
        let canary_ran = Arc::clone(&canary_ran);
        let rescued_by_deadline = Arc::clone(&rescued_by_deadline);
        std::thread::spawn(move || {
            let guard = library.conn();
            lock_taken_tx
                .send(())
                .expect("the test thread is still waiting on this channel");
            // Half a second for a task on an idle runtime to be polled
            // once. Only a failure waits the whole way out.
            let mut waited = 0;
            while !canary_ran.load(Ordering::SeqCst) && waited < 50 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                waited += 1;
            }
            if !canary_ran.load(Ordering::SeqCst) {
                rescued_by_deadline.store(true, Ordering::SeqCst);
            }
            drop(guard);
        })
    };
    // Do not fire the request until the guard is confirmed held — otherwise
    // the request's own `spawn_blocking` closure could win the mutex first
    // and this would measure nothing.
    lock_taken_rx
        .recv()
        .expect("the watchdog thread is still running");

    // Spawned before the canary, so the runtime polls it first: by the time
    // the canary would be scheduled, a read behind `spawn_blocking` has
    // already blocked on the mutex.
    let request = tokio::spawn({
        let router = router.clone();
        async move {
            router
                .oneshot(
                    Request::get("/api/works/w/progress")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }
    });
    let canary = tokio::spawn({
        let canary_ran = Arc::clone(&canary_ran);
        async move { canary_ran.store(true, Ordering::SeqCst) }
    });

    let response = request.await.unwrap();
    canary.await.unwrap();
    watchdog.join().unwrap();

    assert!(
        !rescued_by_deadline.load(Ordering::SeqCst),
        "the runtime polled nothing else for half a second: something on the request's \
             path through the router is blocking the async worker, and auth::guard is the \
             part of that path that reads the library"
    );

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let positions = doc["positions"].as_array().expect("positions is an array");
    assert_eq!(positions.len(), 1, "one write, one position");
    assert_eq!(positions[0]["edition"].as_str().unwrap(), "raw");
    assert_eq!(
        positions[0]["slot"].as_u64().unwrap(),
        1,
        "the value written before the block still comes back correctly"
    );
}
