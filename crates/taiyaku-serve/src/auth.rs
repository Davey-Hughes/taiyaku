//! The guard: one middleware, one [`Principal`], two shapes of refusal.
//!
//! Every path in `routes::guarded_routes` sits behind this, which is one layer
//! rather than one extractor per handler — a handler that forgot the extractor
//! would be open, and nothing would say so.
//!
//! **What is here is the HTTP shape of the question.** Who a credential belongs
//! to is `taiyaku_library::account`'s, because `taiyaku-cli` asks the same
//! question and cannot depend on this crate.
//!
//! Spec A2 §4 is the design; `docs/superpowers/specs/2026-08-14-library-auth-design.md`.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::ServeState;

/// One address range, as `--trusted-proxy` accepts it.
///
/// **Written by hand rather than pulled in as a crate.** Prefix matching over
/// `IpAddr` is thirty lines, and spec A2 names `argon2` as the one dependency it
/// needs; a second for this would double that on a branch whose whole
/// dependency claim is "one new crate". The same reasoning `opds::escape` and
/// `opds::percent_encode` already record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    base: std::net::IpAddr,
    prefix: u8,
}

impl std::str::FromStr for Cidr {
    type Err = String;

    /// `10.0.0.0/8`, `fd00::/8`, or a bare address — which is what a person
    /// means by `--trusted-proxy 10.0.0.1`, and refusing it would be pedantry
    /// about a form nobody would write by mistake.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (
                address,
                Some(
                    prefix
                        .parse::<u8>()
                        .map_err(|_| format!("`{text}`: `{prefix}` is not a prefix length"))?,
                ),
            ),
            None => (text, None),
        };
        // **Canonicalised here, where the width is decided.** `contains`
        // canonicalises too, and for one commit only it did — so
        // `--trusted-proxy ::ffff:10.0.0.1` stored width 128 against a base that
        // compares as four bytes, and the first request from any v4 peer
        // panicked with `range end index 16 out of range for slice of length 4`.
        // Startup succeeded, so the failure arrived as a dead server rather than
        // a refused flag. The base and its width have to be decided on the same
        // form. Found by the third whole-branch review.
        let base: std::net::IpAddr = address
            .parse::<std::net::IpAddr>()
            .map_err(|_| format!("`{text}`: `{address}` is not an IP address"))?
            .to_canonical();
        let width = if base.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(width);
        if prefix > width {
            // The mapped case gets its own sentence, because the address a
            // person typed and the one being measured are different forms and
            // the bare number would look wrong: `::ffff:10.0.0.0/104` is an
            // IPv4 range written in IPv6, and /104 there means /8 here.
            let mapped = address.contains("::ffff:") && base.is_ipv4();
            return Err(format!(
                "`{text}`: /{prefix} is longer than the {width} bits of an IPv{} \
                 address{}",
                if base.is_ipv4() { 4 } else { 6 },
                if mapped {
                    format!(
                        " — `{address}` is an IPv4 address written in IPv6, so its \
                         prefix is counted in IPv4's 32 bits; /{} is what /{prefix} means here",
                        prefix.saturating_sub(96)
                    )
                } else {
                    String::new()
                }
            ));
        }
        Ok(Self { base, prefix })
    }
}

impl Cidr {
    /// Whether `addr` falls inside this range.
    ///
    /// A v4 address and a v6 range never match each other, deliberately: an
    /// IPv4-mapped v6 peer (`::ffff:10.0.0.1`) is canonicalised to its v4 form
    /// first, so `10.0.0.0/8` covers it, and anything else genuinely is a
    /// different address family.
    #[must_use]
    pub fn contains(&self, addr: std::net::IpAddr) -> bool {
        let addr = addr.to_canonical();
        // `self.base` is already canonical — `from_str` stores it that way, so
        // that the width it derived and the bytes compared here agree.
        match (self.base, addr) {
            (std::net::IpAddr::V4(base), std::net::IpAddr::V4(addr)) => {
                prefix_matches(&base.octets(), &addr.octets(), self.prefix)
            }
            (std::net::IpAddr::V6(base), std::net::IpAddr::V6(addr)) => {
                prefix_matches(&base.octets(), &addr.octets(), self.prefix)
            }
            _ => false,
        }
    }
}

/// Whether two addresses agree on their first `prefix` bits.
fn prefix_matches(base: &[u8], addr: &[u8], prefix: u8) -> bool {
    let whole = usize::from(prefix / 8);
    let spare = prefix % 8;
    if base[..whole] != addr[..whole] {
        return false;
    }
    if spare == 0 {
        return true;
    }
    // The high `spare` bits of the next byte, and nothing below them.
    let mask = 0xffu8 << (8 - spare);
    base[whole] & mask == addr[whole] & mask
}

/// The address to hold this request against, given who actually connected and
/// what they claim.
///
/// **`X-Forwarded-For` is honoured only from a configured peer**, which is §6's
/// first row: a boolean `--behind-proxy` would have to trust the header from
/// whatever connected, and on a misconfigured deployment that means trusting the
/// internet. Any client could then spoof its address and the rate limiter below
/// would be useless.
///
/// The **last** entry of the chain is taken, not the first. A proxy appends; the
/// entries before its own are whatever the client sent, so reading from the
/// front lets a client prepend an address of its choosing.
pub(crate) fn client_address(
    trusted: &[Cidr],
    peer: std::net::IpAddr,
    forwarded: Option<&str>,
) -> std::net::IpAddr {
    if !trusted.iter().any(|range| range.contains(peer)) {
        return peer;
    }
    forwarded
        .and_then(|chain| chain.rsplit(',').next())
        .and_then(|last| last.trim().parse().ok())
        .unwrap_or(peer)
}
/// Whether a request's `X-Forwarded-For` is being ignored for want of a
/// `--trusted-proxy`.
///
/// **Extracted from the `if` it used to be**, because it was a decision inside a
/// `eprintln!` and nothing could assert on it: mutation testing replaced the
/// `&&` with `||` on 2026-08-19 with every test still passing, which would have
/// warned on every request from a client that sent no such header. The warning
/// it guards is the one that tells an operator their rate limit is being applied
/// to the proxy rather than to a reader — precisely the message that must not
/// cry wolf.
///
/// The `static SAID` at the call site is what makes it print once, and that
/// remains untestable in-process: a second test in the same binary sees an
/// atomic another test already swapped. Splitting the decision out is what makes
/// the half that *can* be tested testable.
fn forwarded_but_untrusted(forwarded: bool, config: &crate::session::AuthConfig) -> bool {
    forwarded && config.trusted_proxies.is_empty()
}

/// [`client_address`], reading the peer and the header off what a handler has.
///
/// A convenience over the pure function above rather than a second rule: the
/// function is what the tests drive, because a `SocketAddr` and a `HeaderMap`
/// are harder to state a case with than an address and a string.
///
/// **A missing peer falls back to an unroutable address rather than to
/// nothing.** `ConnectInfo` is absent when the router was built without it —
/// which is what every `oneshot` test does — and a rate limiter keyed on
/// `Option` would either lock every such caller out together or exempt them
/// together. `0.0.0.0` is honest: this request arrived by a route that carries
/// no address.
pub(crate) fn client_of(
    config: &crate::session::AuthConfig,
    extensions: &axum::http::Extensions,
    headers: &axum::http::HeaderMap,
) -> std::net::IpAddr {
    // **`get_all`, not `get`.** `HeaderMap::get` answers the first value for a
    // repeated name, and a client can send its own `X-Forwarded-For` — so a
    // proxy that emits its own as a *second* header line, which several stacks
    // do by default, would leave the attacker's line the one read. Joining every
    // line puts the trusted hop's entry last, where `client_address`'s
    // last-element rule expects it.
    let forwarded = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");
    let forwarded = (!forwarded.is_empty()).then_some(forwarded);
    let peer = extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map_or(std::net::IpAddr::from([0, 0, 0, 0]), |info| info.0.ip());

    // **A forwarded header with no trusted proxy configured is a
    // misconfiguration worth saying out loud, once.** §7 keys the lockout on
    // `(address, name)` precisely so one misconfigured client locks out only
    // itself — but behind a proxy that is not named by `--trusted-proxy`, every
    // request arrives from the proxy's own address, so the pair collapses to the
    // account and any stranger can lock a named reader out of every device.
    // Verified: ten wrong guesses at one account answered 429 to that account's
    // correct password while another account was unaffected.
    //
    // Not silently trusted instead — that is §6's first row, and would let any
    // client spoof its address. Said once rather than per request, because a
    // line per page turn is a line nobody reads. Found by the second
    // whole-branch review.
    if forwarded_but_untrusted(forwarded.is_some(), config) {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!(
                "WARNING: a request carried X-Forwarded-For and no --trusted-proxy is \
                 configured, so it was ignored and every request is being held against \
                 the proxy's own address. The login rate limit therefore applies to a \
                 whole account rather than to one client, and anyone can lock a reader \
                 out. Pass --trusted-proxy CIDR naming the proxy."
            );
        }
    }

    client_address(&config.trusted_proxies, peer, forwarded.as_deref())
}

/// Admin or reader. Spec A2 §8 keeps this to two deliberately: no age ratings,
/// no content restrictions, no per-label filters — those exist for libraries
/// shared with children and were declined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}

/// Who this request is, resolved once by [`guard`] and inserted as a request
/// extension for the handlers to read.
///
/// Carries no password hash and no token: `account::Account` is returned without
/// its hash for that reason, and the token that produced a session is not
/// recoverable from the row it matched.
///
/// `user_id` is read by the progress and notes routes, which is what replaced
/// the two `LOCAL_USER = 1` constants, and `role` decides `admin` in `/api/me`.
///
/// **`can_download` and `can_stream` are reported by `/api/me` and govern
/// nothing** — a distinction worth keeping, because an earlier version of this
/// comment said they were unread, which `whoami` had already made false. They
/// wait for spec C2's download; `roots` waits for C1's browse, and is the one
/// field genuinely nothing reads. O54 tracks all three.
#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: i64,
    pub role: Role,
    pub can_download: bool,
    pub can_stream: bool,
    /// The roots this account was granted.
    ///
    /// **The one field nothing reads**, per spec A2 §8 as amended on 2026-08-18:
    /// spec B landed before this one and gave `work` no `root_id`, and no route
    /// lists by root. Resolved anyway so spec C1's browse API adds a filter
    /// rather than a lookup. `#[expect]` on this field alone rather than on the
    /// struct, so the day C1 reads it the attribute fails instead of lingering —
    /// and so it cannot go on excusing a second field the way the struct-level
    /// one did while `whoami` was already reading two of them. O54.
    #[expect(dead_code, reason = "resolved for spec C1's browse filter")]
    pub roots: Vec<i64>,
}

impl Principal {
    pub(crate) fn from_account(
        account: &taiyaku_library::account::Account,
        roots: Vec<i64>,
    ) -> Self {
        Self {
            user_id: account.id,
            role: if account.role == "admin" {
                Role::Admin
            } else {
                Role::User
            },
            can_download: account.can_download,
            can_stream: account.can_stream,
            roots,
        }
    }
}

/// The 401 an OPDS client needs and the 401 the reader needs, which are not the
/// same response.
///
/// §4: `WWW-Authenticate` is what makes Panels or Chunky prompt for
/// credentials — and it is also what makes a browser raise its own native auth
/// dialog, which the web app cannot style, cannot pre-fill and cannot recover
/// from. So `/opds` gets the header and `/api` gets plain JSON, and the reader
/// routes itself to its own login screen.
fn refuse(path: &str) -> Response {
    if path.starts_with("/opds") {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"taiyaku\"")],
            "unauthorized",
        )
            .into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"unauthorized"}"#,
        )
            .into_response()
    }
}

/// Resolve every request to a [`Principal`], or refuse it.
///
/// **Resolution happens inside `spawn_blocking`**, for the same reason every
/// handler in `routes.rs` does: it reads SQLite through `state.library`, whose
/// `conn()` is a `MutexGuard`, and a password additionally costs an argon2
/// verify measured at 11.9 ms. Doing either on the async worker stalls the whole
/// runtime — which `routes::tests::a_progress_read_does_not_stall_the_runtime`
/// asserts, and which it caught this middleware doing when the guard was first
/// written. A guard is the one piece of middleware every request passes through,
/// so it is the worst possible place to block.
pub(crate) async fn guard(
    State(state): State<Arc<ServeState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    // Only what resolution needs, cloned out, so the request can move into
    // `next.run` afterwards rather than being borrowed across an await.
    let offered = Offered::from(&state, &request);
    // Kept for the re-issue below, before `offered` moves into the closure.
    let cookie = offered.cookie.clone();
    let secure_cookies = state.auth.secure_cookies;
    let resolved = tokio::task::spawn_blocking(move || resolve(&state, &offered)).await;

    let (principal, by) = match resolved {
        Ok(Resolved::Yes(principal, by)) => (principal, by),
        Ok(Resolved::TooMany) => return too_many(),
        Ok(Resolved::CrossSite) => return cross_site(),
        // A `JoinError` lands in `_`, which is a panic in resolution: refusing
        // is the only safe reading of "we could not establish who this is".
        _ => return refuse(&path),
    };
    let mut request = request;
    request.extensions_mut().insert(principal);
    request.extensions_mut().insert(by);
    let mut response = next.run(request).await;

    // **The row's expiry slid forward inside `session_owner`; the cookie's has
    // to slide here or the two disagree.** `Max-Age` is absolute from the moment
    // the browser receives it, so a cookie written once at login is discarded
    // thirty days later however often it was used — while the row stays valid,
    // leaving the reader bounced to a login screen and a dead browser listed as
    // signed in. Three doc comments promised the sliding behaviour and all three
    // were true of the row and false of the cookie until this. Found by the
    // whole-branch review, 2026-08-18.
    //
    // Only for the cookie path: an OPDS client presenting an API key has no
    // session to extend, and a `Set-Cookie` on every page of a volume would be
    // noise it never asked for.
    if by == By::Cookie
        && let Some(token) = cookie
        && let Ok(value) =
            axum::http::HeaderValue::from_str(&crate::session::set_cookie(&token, secure_cookies))
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// What resolving a request's credentials came to.
///
/// Three outcomes rather than an `Option`, because "no" and "not now" are
/// answered with different statuses and a client retries differently for each —
/// the same split `session::Attempt` makes on the login route, and for the same
/// reason.
enum Resolved {
    /// Authenticated, and by which credential — because a cookie's own lifetime
    /// has to be extended in the response and a header's must not be.
    Yes(Principal, By),
    No,
    TooMany,
    /// A cookie that resolves to a real session, on a state-changing request
    /// whose `Origin` does not match.
    ///
    /// **Answered 403 rather than 401**, because the reader is signed in and
    /// `api.ts` routes a 401 to the login screen. An operator who leaves nginx's
    /// default `proxy_set_header Host $proxy_host` in place gets safe methods
    /// succeeding — `origin_is_acceptable` waves those through — so the library
    /// loads, and then the first debounced note or position write signs the
    /// reader out and discards their unsaved text. Signing in works, the cookie
    /// was always valid, and the next write throws them out again.
    ///
    /// `refuse_cross_site` already reasoned this way for the open routes: "every
    /// client reads a 401 from a login route as 'wrong password' … the least
    /// actionable message this server could send". The guarded path kept the
    /// 401. Found by the third whole-branch review.
    CrossSite,
}

/// Which credential answered for this request.
///
/// **Inserted into the request's extensions**, not only used for the cookie
/// re-issue: the account-management routes read it to refuse an API key. A key
/// is a *device* credential — §2 justifies its fast hash by its role as an OPDS
/// page credential — and one typed into a third-party reader granted minting
/// further keys, listing the owner's browsers with their addresses, and signing
/// every one of them out. Nothing narrowed it to that role until this. Found by
/// the second whole-branch review.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum By {
    /// A session cookie, carrying the raw token so the response can re-issue it.
    Cookie,
    /// `Authorization: Basic` with the account's **password**.
    ///
    /// Distinguished from a key because only the key is a *device* credential.
    /// Folding the two into one `Header` variant made the account routes refuse
    /// a correct password with a message asserting an API key had been used —
    /// and left no non-browser path to mint a first key or change a password at
    /// all. Found by the third whole-branch review.
    Password,
    /// `Authorization: Basic` with a `tyk_` API key.
    Key,
}

impl By {
    /// Whether this credential may administer the account it authenticated.
    ///
    /// A key may not: §2 justifies its fast hash by its role as the thing an
    /// OPDS client presents for every page, and one typed into a third-party
    /// reader must not also mint keys or sign the owner's browsers out. A
    /// password and a session both may — they are the account itself.
    pub(crate) fn may_administer(self) -> bool {
        matches!(self, Self::Cookie | Self::Password)
    }
}

/// A signed-in reader whose state-changing request came from elsewhere.
///
/// 403 rather than 401 so the reader is not signed out by it — see
/// [`Resolved::CrossSite`].
fn cross_site() -> Response {
    (
        StatusCode::FORBIDDEN,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"error":"this request's Origin does not match this server. If it sits behind a proxy, that proxy must pass the browser's Host through — or name it with --trusted-proxy so X-Forwarded-Host is believed"}"#,
    )
        .into_response()
}

/// §7's refusal, on the guard rather than only on the login route.
fn too_many() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"error":"too many failed attempts; try again shortly"}"#,
    )
        .into_response()
}

/// The credentials a request carries, lifted off it before resolution so no
/// `Request` has to cross a thread boundary.
struct Offered {
    basic: Option<String>,
    cookie: Option<String>,
    method: axum::http::Method,
    origin: Option<String>,
    host: Option<String>,
    /// Who to hold a failed password against (§7). Resolved here rather than in
    /// `from_basic` because `ConnectInfo` lives in the request's extensions and
    /// the request does not cross into `spawn_blocking`.
    client: std::net::IpAddr,
}

impl Offered {
    fn from(state: &ServeState, request: &Request) -> Self {
        let text = |name: axum::http::HeaderName| {
            request
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        Self {
            basic: text(header::AUTHORIZATION),
            cookie: crate::session::token_of(request.headers()),
            method: request.method().clone(),
            origin: text(header::ORIGIN),
            host: crate::session::effective_host(
                &state.auth,
                request.extensions(),
                request.headers(),
            ),
            client: client_of(&state.auth, request.extensions(), request.headers()),
        }
    }
}

/// The credential this request carries, whatever kind it is.
///
/// **The cookie goes first**, because it is the reader's path and the common
/// one: a browser that sends both should be answered by its own session rather
/// than by whatever a password manager decided to fill in.
///
/// Falling through on a cookie that does not resolve is deliberate. A stale or
/// expired session accompanied by a valid `Authorization` header should be
/// answered by the header — otherwise a client with both would be locked out by
/// the weaker of its two credentials.
fn resolve(state: &ServeState, offered: &Offered) -> Resolved {
    if let Some(token) = offered.cookie.as_deref() {
        match crate::session::from_cookie(
            state,
            token,
            &offered.method,
            offered.origin.as_deref(),
            offered.host.as_deref(),
        ) {
            crate::session::FromCookie::Resolved(principal) => {
                return Resolved::Yes(principal, By::Cookie);
            }
            // A real session refused for its origin is not a signed-out reader,
            // and must not be answered as one — but a header credential beside
            // it is still allowed to speak for the request.
            crate::session::FromCookie::CrossSite if offered.basic.is_none() => {
                return Resolved::CrossSite;
            }
            _ => {}
        }
    }
    let Some(header) = offered.basic.as_deref() else {
        return Resolved::No;
    };
    from_basic(state, header, offered.client)
}

/// `Authorization: Basic`, routed by [`taiyaku_library::credential::looks_like_api_key`].
///
/// **The key path never reaches argon2**, which is §2's whole point: one string
/// comparison decides, rather than an argon2 verify that falls back to the key
/// table. Every page request from an OPDS client carries this credential, so the
/// difference is five seconds of pure hashing on a 400-page volume.
///
/// **And the password path does not hold the library's mutex across argon2.**
/// `Library::conn()` hands out a `MutexGuard` over the one connection this
/// process has (O42, O52), so verifying inside its scope would serialise every
/// other request in the server behind 11.9 ms of hashing per login. The hash is
/// read, the guard dropped, the verify done, and the connection re-taken only
/// for the roots — three short holds rather than one long one.
fn from_basic(state: &ServeState, header: &str, client: std::net::IpAddr) -> Resolved {
    use taiyaku_library::{account, credential};

    let Some((name, secret)) = decode_basic(header) else {
        return Resolved::No;
    };

    if credential::looks_like_api_key(&secret) {
        // **Deliberately not rate limited**, which is §2's reasoning rather
        // than an omission: 256 bits from a CSPRNG has no dictionary and no
        // structure, so there is nothing for a limit to buy — while an OPDS
        // client presents this on every page of a volume and any limit worth
        // the name would lock a reader out of their own library.
        let conn = state.library.conn();
        let Ok(Some(found)) = account::by_api_key(&conn, &secret) else {
            return Resolved::No;
        };
        let Ok(roots) = account::roots_for(&conn, found.id) else {
            return Resolved::No;
        };
        return Resolved::Yes(Principal::from_account(&found, roots), By::Key);
    }

    // **§7 applies here and not only to the login route**, which is what the
    // design says and what shipped without: "on the login route *and on failed
    // `Authorization: Basic` verification only*". Without this a password is
    // guessable at any rate through any guarded route, while ten tries at the
    // login route lock the pair out — guarding the one door a guesser has no
    // reason to use. Each guess also costs a `spawn_blocking` thread for 11.9 ms
    // and argon2's 19 MiB buffer, so the limit is what bounds an unauthenticated
    // denial of service as much as a dictionary.
    //
    // Checked before the account is looked up, so a locked-out pair costs no
    // argon2 at all.
    let key = account::attempt_key(&client.to_string(), &name);
    // **The wider key too, for the same reason the login route takes it**: the
    // pair's name half comes from the `Authorization` header, so a fresh
    // invented name is a fresh counter and the decoy verify below runs against
    // no ceiling at all. `account::address_key` has the reasoning; this route
    // needs it more than the login route does, because it is reachable on every
    // guarded path rather than one.
    let wide = account::address_key(&client.to_string());
    {
        let conn = state.library.conn();
        // **Fails closed.** `unwrap_or(false)` read any SQLite error as "not
        // locked out", and the errors reachable here — SQLITE_BUSY on the one
        // mutex-guarded connection `serve` and `library import` share — are
        // exactly the ones a flood produces. The control bounding both the
        // dictionary attack and the 19 MiB-per-guess denial of service would
        // have disappeared under the load that triggers it.
        // `admin_exists(...).unwrap_or(true)` in `session.rs` already had this
        // shape; this did not. Found by the second whole-branch review.
        if account::is_address_locked_out(&conn, &wide).unwrap_or(true)
            || account::is_locked_out(&conn, &key).unwrap_or(true)
        {
            return Resolved::TooMany;
        }
    }

    let found = {
        let conn = state.library.conn();
        account::by_name(&conn, &name).ok().flatten()
    };
    // A failure against an account that does not exist counts too, or the
    // limiter would only ever slow down guesses at names it had confirmed.
    let Some((found, phc)) = found else {
        // **The same work a real account costs**, because the difference was
        // measurable: a known name answered 401 after ~11 ms of argon2 and an
        // unknown one in under 1 ms, so response time enumerated the roster —
        // and since `attempt_key` includes the name, each new name opened its
        // own window and never approached the lockout, making the walk
        // unlimited. The 401 branch's own comment claims "a caller cannot
        // enumerate names by the shape of the answer"; timing is a shape.
        // Found by the second whole-branch review.
        // The answer is discarded on purpose: what is wanted is the *cost*, so
        // that an unknown name is indistinguishable from a wrong password by
        // response time. `let _ =` rather than an `#[expect]`, because the
        // lint is right that a returned bool is usually a decision.
        let _ = credential::verify_password(&secret, credential::DECOY_HASH);
        let conn = state.library.conn();
        // Both, unconditionally — `||` short-circuits, and the wide key is
        // the counter that bounds the cost of this exact path.
        let pair = account::note_failure(&conn, &key);
        let address = account::note_failure(&conn, &wide);
        if pair.is_err() || address.is_err() {
            return Resolved::TooMany;
        }
        return Resolved::No;
    };
    // The mutex is released above and re-taken below: argon2 runs holding
    // nothing (O42, O52).
    if !credential::verify_password(&secret, &phc) {
        let conn = state.library.conn();
        // A failure that cannot be recorded is answered as a lockout rather
        // than as an ordinary refusal: an unrecordable failure is an unbounded
        // one, and the safe reading of "the limiter is not working" is to stop.
        // Both, unconditionally — `||` short-circuits, and the wide key is
        // the counter that bounds the cost of this exact path.
        let pair = account::note_failure(&conn, &key);
        let address = account::note_failure(&conn, &wide);
        if pair.is_err() || address.is_err() {
            return Resolved::TooMany;
        }
        return Resolved::No;
    }

    let conn = state.library.conn();
    // Getting it right forgets the mistakes before it — the pair only, so one
    // correct credential from a shared address cannot reset a guesser's wide
    // budget. See the same split in `session::login`.
    let _ = account::clear_failures(&conn, &key);
    let Ok(roots) = account::roots_for(&conn, found.id) else {
        return Resolved::No;
    };
    Resolved::Yes(Principal::from_account(&found, roots), By::Password)
}

/// The name and secret inside an `Authorization: Basic` header.
///
/// `split_once` rather than `split`: a password may contain a colon and the
/// username may not, which is what RFC 7617 says and what a generated API key in
/// the password field depends on.
fn decode_basic(header: &str) -> Option<(String, String)> {
    use base64::Engine as _;

    // **Case-insensitively**, because RFC 7235 defines `auth-scheme` that way.
    // A client sending `basic ` was refused forever and indistinguishable from a
    // wrong password — worse, it recorded no failure either, so nothing in the
    // database hinted at it, and the 401 carried `WWW-Authenticate: Basic` so
    // the client re-sent the identical header and looped. Directly relevant to
    // O55, which records that no OPDS client has been shown to work here yet.
    // Found by the second whole-branch review.
    let (scheme, encoded) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let pair = String::from_utf8(decoded).ok()?;
    let (name, secret) = pair.split_once(':')?;
    Some((name.to_string(), secret.to_string()))
}

#[cfg(test)]
mod tests;
