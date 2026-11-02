//! The browser's half of the guard: a cookie, an origin check, and the two
//! routes that start and end a session.
//!
//! An OPDS client needs none of the cookie — it sends `Authorization: Basic` and
//! is answered by `auth::from_basic`. The cookie is here because a browser
//! cannot hold a credential the way Panels can, and it brings a problem a
//! header does not: it is sent by the browser whether or not the page asking
//! for it is ours. [`origin_is_acceptable`] is the whole of the answer to that.
//!
//! The permits a password verify runs under are here as well
//! ([`VerifyLimit`]), and both halves take them.
//!
//! Spec A2 §4 is the design.

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use std::sync::{Arc, Mutex, PoisonError};
use taiyaku_library::credential::WorkingMemory;

use crate::ServeState;
use crate::auth::Principal;

pub(crate) const COOKIE_NAME: &str = "taiyaku_session";
const MAX_AGE_SECS: i64 = taiyaku_library::account::SESSION_DAYS * 24 * 60 * 60;

/// What `serve` knows about the deployment it sits in, and the permits its
/// password verifies run under.
///
/// Small on purpose: spec A2 §11 puts TLS in a reverse proxy, so none of it is
/// here. §6 is the whole of what this server knows about being behind one.
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// Whether the session cookie carries `Secure`.
    ///
    /// Configuration rather than environment: a trusted proxy implies TLS
    /// terminated in front of this server (§6), and `--insecure-cookies` is the
    /// deliberate opt-out for a plain-HTTP private network.
    pub secure_cookies: bool,
    /// Whether the unauthenticated claim route exists at all (§5, Task 9).
    pub allow_web_setup: bool,
    /// The peers whose `X-Forwarded-For` is believed (§6).
    ///
    /// **A list of ranges rather than a boolean**, because a boolean
    /// `--behind-proxy` would have to trust the header from whatever connected,
    /// which on a misconfigured deployment means trusting the internet. Empty is
    /// the default and trusts nobody, so an unconfigured server rate-limits on
    /// the address that actually connected.
    pub trusted_proxies: Vec<crate::auth::Cidr>,
    /// The permits a password verify runs under (O57). See [`VerifyLimit`]
    /// for why they are here and not in a `static`.
    pub verifies: VerifyLimit,
}

/// How many password verifies one server runs at once, and so the most
/// working memories it makes (O57).
///
/// **What it bounds is memory.** A verify runs argon2 in a [`WorkingMemory`],
/// which for a hash this server made is as large as `credential`'s memory
/// cost. The memory is its permit's for as long as the permit is held, and a
/// server makes a new one only when every one it has is in a permit holder's
/// hands. So it never makes more of them than this, and it keeps the ones it
/// has made. Without the permits every request of a burst would run in a
/// buffer of its own. With them a request past the count waits its turn on a
/// `spawn_blocking` thread, holding no memory.
///
/// The count is a decision and not a measurement. The memory it bounds does
/// not depend on how fast the host is.
///
/// **Verifies, not hashes.** The routes that set a password hash it while
/// they hold the library's one connection, so at most one of those runs at a
/// time already, and none of them takes a permit. They hash in a memory
/// `credential::hash_password` keeps for the process. It is one memory for as
/// long as those routes run one at a time, and the only one argon2 runs in
/// here that is not a permit's.
pub(crate) const MAX_CONCURRENT_VERIFIES: usize = 4;

/// The permits a password verify runs under, [`MAX_CONCURRENT_VERIFIES`] of
/// them for each server, and the working memories those verifies run in.
///
/// **A field of [`AuthConfig`] rather than a `static`**, so that each
/// `ServeState` has its own. One process serves one library, so in `serve`
/// the two are the same thing. In the tests they are not: a test holds every
/// permit of its own server to see what waits for one, and with a `static` it
/// would hold every other test's too wherever tests share a process, which is
/// how `.cargo/mutants.toml` runs them. `cover::tests`' `ONE_HOLDER` is what
/// that costs a `static`.
///
/// Cloning shares the permits and the memories, so a copy of the
/// configuration is not a second limit.
#[derive(Debug, Clone)]
pub struct VerifyLimit(Arc<Verifies>);

/// What every clone of one [`VerifyLimit`] shares.
#[derive(Debug)]
struct Verifies {
    permits: crate::limit::Semaphore,
    /// The working memories no permit is holding.
    ///
    /// **A server never makes more of them than it has permits.** Only a
    /// permit holder takes one from here or makes one, it makes one only when
    /// there is none here to take, and it puts its own back before its permit
    /// is anyone else's. So whenever one is made, every memory made before it
    /// is held by another permit holder, and there are fewer of those than
    /// there are permits.
    ///
    /// **A leaf lock.** It is taken to move one memory in or out and released
    /// at once: never held while waiting for a permit or for the library's
    /// connection, and never across a verify.
    spare: Mutex<Vec<WorkingMemory>>,
    /// How many working memories this has made.
    #[cfg(test)]
    made: std::sync::atomic::AtomicUsize,
}

impl Default for VerifyLimit {
    fn default() -> Self {
        Self(Arc::new(Verifies {
            permits: crate::limit::Semaphore::new(MAX_CONCURRENT_VERIFIES),
            spare: Mutex::new(Vec::new()),
            #[cfg(test)]
            made: std::sync::atomic::AtomicUsize::new(0),
        }))
    }
}

impl VerifyLimit {
    /// Wait for a permit, and hold it until the returned value drops. With
    /// it comes a working memory: a spare one an earlier permit gave back, or,
    /// when none is spare, a new one that holds nothing until a verify runs
    /// in it.
    ///
    /// **It blocks, so it is called on the blocking pool and never on a
    /// runtime worker**, like the SQLite reads on either side of it.
    ///
    /// **Take it before the library's connection, and never while holding
    /// the connection.** A verifying path holds its permit and then asks for
    /// the connection. Anything that held the connection and waited here
    /// would stop every one of them, and they it.
    #[must_use = "a permit that is not kept is given back at once"]
    pub(crate) fn permit(&self) -> VerifyPermit<'_> {
        let held = self.0.permits.acquire();
        // Asked for only now that the permit is held: `Verifies::spare` has
        // why that is what bounds how many are made.
        let spare = self
            .0
            .spare
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        let memory = spare.unwrap_or_else(|| {
            #[cfg(test)]
            self.0
                .made
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            WorkingMemory::new()
        });
        VerifyPermit {
            memory,
            spare: &self.0.spare,
            _held: held,
        }
    }

    /// Every permit at once, so that the next verify waits until they drop.
    #[cfg(test)]
    pub(crate) fn hold_all(&self) -> Vec<VerifyPermit<'_>> {
        (0..MAX_CONCURRENT_VERIFIES)
            .map(|_| self.permit())
            .collect()
    }

    /// How many working memories this server has made since it started.
    #[cfg(test)]
    pub(crate) fn memories_made(&self) -> usize {
        self.0.made.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// How each working memory no permit is holding prints itself. `Debug`
    /// is the one thing a memory says outside its own crate, and what it says
    /// is how much it holds.
    #[cfg(test)]
    pub(crate) fn spare_memories(&self) -> Vec<String> {
        self.0
            .spare
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|memory| format!("{memory:?}"))
            .collect()
    }
}

/// One of a server's verify permits, and the working memory a verify under
/// it runs in. Both are given back when it drops.
#[derive(Debug)]
pub(crate) struct VerifyPermit<'a> {
    memory: WorkingMemory,
    spare: &'a Mutex<Vec<WorkingMemory>>,
    _held: crate::limit::SemaphoreGuard<'a>,
}

impl Drop for VerifyPermit<'_> {
    /// **The memory goes back before the permit does.** This runs before
    /// `_held` drops, so by the time anyone else can hold this permit its
    /// memory is spare again. In the other order the next holder could find
    /// nothing spare and make a memory the server already had.
    fn drop(&mut self) {
        let memory = std::mem::take(&mut self.memory);
        self.spare
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(memory);
    }
}

/// Whether `password` is the one `phc` was made from, for a caller holding a
/// permit, computed in that permit's working memory.
///
/// **The one place this crate verifies a password somebody offered.** It
/// takes the permit so that a verify cannot be written without one in hand,
/// and takes it mutably because the permit's memory is what argon2 fills. A
/// permit bound to `_` is given back before the next statement runs, and it
/// has no name to pass here.
pub(crate) fn verify_holding(permit: &mut VerifyPermit<'_>, password: &str, phc: &str) -> bool {
    taiyaku_library::credential::verify_password_in(&mut permit.memory, password, phc)
}

/// The `Set-Cookie` value for a new session.
///
/// **`SameSite=Lax` rather than `Strict`.** `Strict` breaks following a link
/// into the app from anywhere else — a bookmark, a message, an OPDS client
/// handing off to a browser — and the gap it closes is covered by
/// [`origin_is_acceptable`] below.
pub(crate) fn set_cookie(token: &str, secure: bool) -> String {
    let base =
        format!("{COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={MAX_AGE_SECS}");
    if secure {
        format!("{base}; Secure")
    } else {
        base
    }
}

/// The `Set-Cookie` that ends one.
pub(crate) fn clear_cookie(secure: bool) -> String {
    let base = format!("{COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    if secure {
        format!("{base}; Secure")
    } else {
        base
    }
}

/// Whether a request may change state, given its method and `Origin`.
///
/// **This applies to the cookie path only.** Header auth is not CSRF-able — a
/// forged cross-site form cannot set `Authorization` — so an OPDS client needs
/// none of it. A cookie is different in kind: the browser attaches it to any
/// request to this origin, including one a page on another site caused.
///
/// So every `PUT`, `PATCH`, `POST` and `DELETE` needs an `Origin` matching the
/// server's own. Browsers send it on all four. **A request without one is
/// refused rather than assumed same-origin**, because absent is precisely what
/// a forged cross-site form produces.
pub(crate) fn origin_is_acceptable(
    method: &Method,
    origin: Option<&str>,
    host: Option<&str>,
) -> bool {
    if !matches!(
        *method,
        Method::PUT | Method::PATCH | Method::POST | Method::DELETE
    ) {
        // **Asked before the host is required, not after.** A safe method never
        // needs one, and not every request carries a `Host` — HTTP/2 does not,
        // since hyper puts the authority in the URI and synthesises no header.
        return true;
    }
    let (Some(origin), Some(host)) = (origin, host) else {
        return false;
    };
    // The scheme is dropped and the authority compared, because `Host` carries
    // no scheme to compare against. That is not a hole: what this check
    // defends is "some other site made this request", and another site is
    // another authority whichever scheme it used.
    origin
        .rsplit_once("://")
        .is_some_and(|(_, authority)| authority == host)
}

/// The authority this request believes it reached, which is what an `Origin` is
/// compared against.
///
/// **`X-Forwarded-Host` from a trusted peer, else the `Host` header.** nginx's
/// documented default is `proxy_set_header Host $proxy_host`, so a reverse proxy
/// configured the ordinary way replaces `Host` with the upstream's, and
/// `Origin: https://taiyaku.example` compared against `Host: 127.0.0.1:8080`
/// would be refused, on every login and every cookie-authenticated write, in
/// exactly the deployment §6 says this is for.
///
/// Trusted-only for the same reason `X-Forwarded-For` is: a peer that may rename
/// the server it is talking to may satisfy the origin check by claiming to be
/// whatever origin it came from, which is the whole check gone.
///
/// **And "trusted" means the proxy sets this header itself**, which is a
/// deployment requirement rather than something this code can check: nginx
/// writes `X-Forwarded-Host` for nobody by default and forwards an unrecognised
/// client header of the same name straight through, so naming a proxy with
/// `--trusted-proxy` while it writes nothing lets a client choose the authority
/// its own `Origin` is compared against. `README.md` states the three
/// `proxy_set_header` lines; there is nothing here that can enforce them, which
/// is why they are written down rather than assumed.
pub(crate) fn effective_host(
    config: &AuthConfig,
    extensions: &axum::http::Extensions,
    headers: &axum::http::HeaderMap,
) -> Option<String> {
    let text = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    let peer = extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip());
    let trusted = peer.is_some_and(|peer| {
        config
            .trusted_proxies
            .iter()
            .any(|range| range.contains(peer))
    });
    if trusted {
        // **`get_all` and the first entry** — deliberately the opposite end
        // from `client_of`'s, and the asymmetry is the point rather than an
        // oversight. `X-Forwarded-For` grows `client, proxy1, proxy2`, so the
        // trusted hop's own contribution is *last*; `X-Forwarded-Host` records
        // the authority the browser asked for, which the first proxy writes and
        // later ones leave alone, so it is *first*.
        let lines: Vec<&str> = headers
            .get_all("x-forwarded-host")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect();
        if let Some(first) = lines
            .first()
            .and_then(|line| line.split(',').next())
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            return Some(first.to_string());
        }
    }
    text("host")
}

/// The session token this request presents, if it presents one.
pub(crate) fn token_of(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(name, value)| (name == COOKIE_NAME).then(|| value.to_string()))
}

/// What a session cookie came to.
pub(crate) enum FromCookie {
    Resolved(Principal),
    /// No such session, or an expired one.
    Unknown,
    /// A real session, on a state-changing request from elsewhere. Kept apart
    /// from `Unknown` so the guard can answer 403 rather than signing the reader
    /// out — see `auth::Resolved::CrossSite`.
    CrossSite,
}

/// Resolve a [`Principal`] from the session cookie, if there is one and the
/// request is allowed to use it.
pub(crate) fn from_cookie(
    state: &ServeState,
    token: &str,
    method: &Method,
    origin: Option<&str>,
    host: Option<&str>,
) -> FromCookie {
    // A missing host is refused for a state change and irrelevant to a read —
    // `origin_is_acceptable` decides which, not a `?` on the host here.
    // The session is resolved first, so "no such session" and "a real session
    // from elsewhere" can be told apart — the whole point of the distinction.
    let conn = state.library.conn();
    let Ok(Some(found)) = taiyaku_library::account::session_owner(&conn, token) else {
        return FromCookie::Unknown;
    };
    if !origin_is_acceptable(method, origin, host) {
        return FromCookie::CrossSite;
    }
    let Ok(access) = taiyaku_library::account::access_for(&conn, found.id) else {
        return FromCookie::Unknown;
    };
    FromCookie::Resolved(Principal::from_account(&found, access))
}

/// The origin check, for the two routes that sit **outside** the guard.
///
/// [`origin_is_acceptable`] runs inside `auth::guard` for every other
/// state-changing request. `login` and `logout` cannot go behind that guard —
/// one is how a session begins and the other has to work from a session that
/// has already expired — so the check they would otherwise inherit is applied
/// here instead.
///
/// **Login CSRF is the reason, and it runs the opposite way from the usual
/// worry.** The attack is not stealing a session: it is a page on another
/// origin making the reader's browser post the *attacker's* credentials, after
/// which the reader annotates and records positions into an account the
/// attacker can sign into and read back.
///
/// Two accidents of the browser stand in the way as well: an HTML form cannot
/// send `application/json`, and `fetch` with that content type — or with
/// `DELETE` — triggers a CORS preflight this server never answers. Both are
/// real and neither is this server's doing, which is exactly the objection to
/// resting on them: a permissive CORS layer, or an extractor that accepts a
/// form encoding, would open this with nothing here changing to say so.
///
/// **403 rather than 401**, deliberately. Every client reads a 401 from a login
/// route as "wrong password", so a reverse proxy that rewrites `Origin` or drops
/// `Host` would present as every account's password being wrong at once — the
/// least actionable message this server could send.
fn refuse_cross_site(
    config: &AuthConfig,
    extensions: &axum::http::Extensions,
    headers: &axum::http::HeaderMap,
    method: &Method,
) -> Option<Response> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let host = effective_host(config, extensions, headers);
    if origin_is_acceptable(method, origin.as_deref(), host.as_deref()) {
        return None;
    }
    Some(
        (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"this request's Origin does not match this server; a session can only be opened or closed from the reader's own origin"}"#,
        )
            .into_response(),
    )
}

#[derive(serde::Deserialize)]
pub(crate) struct LoginBody {
    name: String,
    password: String,
}

/// `POST /api/session` — exchange a name and password for a session cookie.
///
/// **Outside the guard**, because a request that is about to log in cannot
/// already have.
pub(crate) async fn login(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<LoginBody>,
) -> Response {
    if let Some(refusal) = refuse_cross_site(&state.auth, &extensions, &headers, &Method::POST) {
        return refusal;
    }
    let address = crate::auth::client_of(&state.auth, &extensions, &headers);
    let secure_cookies = state.auth.secure_cookies;
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    // `spawn_blocking` for the reasons the guard uses it: SQLite behind a
    // mutex, an argon2 verify, and a wait for a permit before it (O57). A login
    // that is not locked out pays all three.
    let opened = tokio::task::spawn_blocking(move || {
        use taiyaku_library::{account, credential};

        // §7: keyed on the pair, so one misconfigured client locks out only
        // itself. Checked before the account is even looked up, so a locked-out
        // pair costs no argon2 — which is the other half of what a rate limit
        // is for on a route whose work is deliberately expensive.
        let key = account::attempt_key(&address.to_string(), &body.name);
        // **The wider key, checked first and before anything expensive.** The
        // pair's name half is supplied by the caller, so a fresh invented name
        // is a fresh counter and the decoy verify below is free to run — see
        // `account::address_key`.
        let wide = account::address_key(&address.to_string());
        // Asked twice below, so written once: both reads must ask both keys
        // and fail the same way.
        let locked_out = || {
            let conn = state.library.conn();
            // Fails closed: `unwrap_or(false)` would read a `SQLITE_BUSY` on
            // the one shared connection as "not locked out", losing the limiter
            // under exactly the load that triggers it.
            account::is_address_locked_out(&conn, &wide).unwrap_or(true)
                || account::is_locked_out(&conn, &key).unwrap_or(true)
        };
        if locked_out() {
            return Attempt::TooMany;
        }
        // **O57: a permit, and then the same question a second time.** The
        // read above is what refuses a caller already locked out: it costs
        // that caller neither a wait nor a verify. It cannot bound a burst,
        // because requests that arrive together all pass it before any of
        // them has failed. So the lockout is read again here, by a request
        // holding a permit it keeps until its own failure is recorded. The
        // only requests between this read and that record are the ones
        // holding a permit, so a burst costs the allowance it found and at
        // most one more verify for each other permit.
        let mut permit = state.auth.verifies.permit();
        if locked_out() {
            return Attempt::TooMany;
        }

        let found = {
            let conn = state.library.conn();
            account::by_name(&conn, &body.name).ok().flatten()
        };
        // A failure for an account that does not exist still counts, or the
        // limiter would only slow down guesses at names it has confirmed — and
        // it must cost the same, or response time enumerates the roster. See
        // `credential::DECOY_HASH`, and `auth::from_basic` for the same split.
        let Some((found, phc)) = found else {
            let _ = verify_holding(&mut permit, &body.password, credential::DECOY_HASH);
            let conn = state.library.conn();
            // **Both, unconditionally.** `||` short-circuits, so a pair-key
            // failure that errored would skip the wide key entirely — the one
            // counter that bounds the cost, dropped on the path that most
            // needs it. The decision is taken after both have been recorded.
            let pair = account::note_failure(&conn, &key);
            let address = account::note_failure(&conn, &wide);
            if pair.is_err() || address.is_err() {
                return Attempt::TooMany;
            }
            return Attempt::No;
        };
        // Verified holding the permit and not the library's connection — see
        // `auth::from_basic`.
        if !verify_holding(&mut permit, &body.password, &phc) {
            let conn = state.library.conn();
            // An unrecordable failure is an unbounded one; the safe reading of
            // "the limiter is not working" is to stop rather than to continue.
            // **Both, unconditionally.** `||` short-circuits, so a pair-key
            // failure that errored would skip the wide key entirely — the one
            // counter that bounds the cost, dropped on the path that most
            // needs it. The decision is taken after both have been recorded.
            let pair = account::note_failure(&conn, &key);
            let address = account::note_failure(&conn, &wide);
            if pair.is_err() || address.is_err() {
                return Attempt::TooMany;
            }
            return Attempt::No;
        }
        // The verify passed and there is no failure to record, so the next
        // one may start.
        drop(permit);

        let token = credential::mint_token();
        {
            let conn = state.library.conn();
            // **Not `Attempt::No`.** The password was right; the database
            // refused the write — `serve` and `library import` share one file,
            // so an INSERT can lose to a busy writer. Answering 401 makes every
            // client say "that name and password did not match", which is
            // actively false and leaves the reader retrying a correct password
            // forever with nothing recorded anywhere. The two database failures
            // above it in this same closure reason about this error class and
            // answer 429.
            if account::open_session(
                &conn,
                found.id,
                &token,
                user_agent.as_deref(),
                Some(&address.to_string()),
            )
            .is_err()
            {
                return Attempt::Failed;
            }
            // Getting it right forgets the mistakes before it, so two typos and
            // then the right password leave nothing to accumulate against the
            // next slip.
            //
            // **The pair only, deliberately.** Clearing the address key too
            // would let one correct login wipe out every other name's failures
            // from the same address — on a shared NAT, an innocent reader
            // signing in would reset a guesser's budget. The wide counter is a
            // cost ceiling rather than a lockout, and it lapses with its window.
            let _ = account::clear_failures(&conn, &key);
        }
        Attempt::Yes(token)
    })
    .await;

    match opened {
        Ok(Attempt::Yes(token)) => (
            StatusCode::NO_CONTENT,
            [(header::SET_COOKIE, set_cookie(&token, secure_cookies))],
        )
            .into_response(),
        Ok(Attempt::TooMany) => (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"too many failed attempts; try again shortly"}"#,
        )
            .into_response(),
        Ok(Attempt::Failed) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"the password was accepted but this session could not be recorded; the library may be busy"}"#,
        )
            .into_response(),
        // One refusal for "no such account" and for "wrong password", so a
        // caller cannot enumerate names by the shape of the answer.
        _ => (
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"unauthorized"}"#,
        )
            .into_response(),
    }
}

/// What one login attempt came to. Three outcomes rather than an `Option`,
/// because "refused" and "not now" are answered with different statuses and a
/// client retries differently for each.
enum Attempt {
    Yes(String),
    No,
    TooMany,
    /// The credential was right and the server could not record the session.
    /// Answered 500, because it is this server's failure and not the caller's —
    /// and because "wrong password" would be a lie the reader cannot act on.
    Failed,
}

/// `DELETE /api/session` — end this session.
///
/// Answers 204 whether or not a session was found: a logout that reports "you
/// were not logged in" tells a caller something about a token it just presented,
/// and there is nothing a client would do differently.
pub(crate) async fn logout(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Some(refusal) = refuse_cross_site(&state.auth, &extensions, &headers, &Method::DELETE) {
        return refusal;
    }
    let token = token_of(&headers);
    let secure = state.auth.secure_cookies;
    if let Some(token) = token {
        let _ = tokio::task::spawn_blocking(move || {
            let conn = state.library.conn();
            taiyaku_library::account::close_session(&conn, &token)
        })
        .await;
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie(secure))],
    )
        .into_response()
}

/// `POST /api/claim` — take an unclaimed server from a browser.
///
/// **Off by default and behind `--allow-web-setup`** (§5). The CLI is the
/// ordinary path, because promoting the first account from the host's own shell
/// means there is never a window in which an unclaimed server sits on the
/// network waiting to be claimed by whoever reaches it first.
///
/// **It closes the instant an admin exists**, not when the flag is turned off —
/// which nobody will do. `account::is_claimed` is what decides, and it reads
/// the password rather than the role for exactly this reason: migration 1 seeds
/// a row with role `admin` and migration 4 locks it, so a fresh library has an
/// admin row and no admin, and reading the role alone would close this route
/// before anybody could reach it.
///
/// **404 rather than 403 when it is shut**, so the route's existence is not
/// itself a signal: a server that has been claimed looks exactly like a server
/// built without the flag.
///
/// One code path with the CLI, per §5: this calls the same `account::promote`,
/// so "promotion updates the seeded row rather than inserting beside it" is one
/// rule with one implementation rather than two that can drift.
pub(crate) async fn claim(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<LoginBody>,
) -> Response {
    if !state.auth.allow_web_setup {
        return shut();
    }
    if let Some(refusal) = refuse_cross_site(&state.auth, &extensions, &headers, &Method::POST) {
        return refusal;
    }
    // **The same refusal `change_password` and the CLI make.**
    // `credential::hash_password` refuses an empty password too, and its doc
    // says why; this check fires earlier and words the refusal for this route.
    if body.password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"an empty password would leave this server's only administrator with no password at all, and the claim route shuts once one exists"}"#,
        )
            .into_response();
    }

    // On every request it serves, per §5: "a flag whose danger is invisible
    // while it is doing nothing is a flag that stays on."
    eprintln!(
        "WARNING: --allow-web-setup served a claim attempt. Anyone who can reach this \
         server can claim it until an admin exists."
    );

    /// What the claim's blocking half came to.
    enum Claimed {
        /// Claimed, and signed in with this session token.
        Token(String),
        /// The caller's own mistake, in the store's sentence.
        Unusable(String),
        /// Anything else, answered as a claimed server is.
        Shut,
    }

    let secure_cookies = state.auth.secure_cookies;
    let claimed = tokio::task::spawn_blocking(move || {
        use taiyaku_library::{account, credential};

        let conn = state.library.conn();
        // **`is_claimed`, not `admin_exists`.** The two answer different
        // questions and this route needs the wider one: `user add alice`
        // inserts with role `user`, so a library its owner had already set up
        // and was already reading from would report no admin to `admin_exists`,
        // and a stranger could take the seeded row — with every note and
        // reading position hanging off it.
        if account::is_claimed(&conn).unwrap_or(true) {
            return Claimed::Shut;
        }
        // **A name the store refuses is said, as the empty password above
        // is**: answered as shut, a name such as `.` would read as a server
        // already claimed, and the person would stop trying. Every other
        // refusal stays shut, since one that is not the caller's mistake says
        // only that this server cannot be claimed now.
        match account::promote(&conn, &body.name, &body.password, "admin") {
            Ok(_) => {}
            Err(account::WriteError::Unusable(why)) => return Claimed::Unusable(why),
            Err(_) => return Claimed::Shut,
        }
        let Ok(Some(id)) = account::id_of(&conn, &body.name) else {
            return Claimed::Shut;
        };
        let token = credential::mint_token();
        if account::open_session(&conn, id, &token, None, None).is_err() {
            return Claimed::Shut;
        }
        Claimed::Token(token)
    })
    .await;

    match claimed {
        // Signed in by the same response that claimed it: a person who has just
        // set the only password on the server should not then be asked for it.
        Ok(Claimed::Token(token)) => (
            StatusCode::NO_CONTENT,
            [(header::SET_COOKIE, set_cookie(&token, secure_cookies))],
        )
            .into_response(),
        Ok(Claimed::Unusable(why)) => (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": why })),
        )
            .into_response(),
        Ok(Claimed::Shut) | Err(_) => shut(),
    }
}

/// The principal a guarded handler was reached with.
fn principal_of(extensions: &axum::http::Extensions) -> Option<&Principal> {
    extensions.get::<Principal>()
}

/// Refuse an API key on a route that administers the account it belongs to.
///
/// **A key is a device credential.** §2 justifies its deliberately fast hash by
/// its role as the thing an OPDS client presents for every page of a volume, so
/// without this a key typed into a third-party reader could also mint further
/// keys (so revoking the device would not lock anyone out), list the owner's
/// browsers with their user-agents and addresses, and sign all of them out.
///
/// 403 rather than 401: the credential is valid and the caller is who they say,
/// which is exactly why a re-authentication prompt would be the wrong answer.
///
/// **Called directly only by this module's own six self-service routes.**
/// Everything administrative reaches it through
/// [`administering_or_forbidden`], which asks the role question first.
// **A refusal that asserts something untrue is a defect.** The message is
// worded for the boundary itself rather than for whichever routes happen to
// sit behind it, which is the version that does not go stale when the gate is
// reused: a wording about accounts, keys or sessions would be false for
// `DELETE /api/library/roots/{id}`, which manages none of them. The phrase
// "API key" is load-bearing:
// `auth::tests::exactly_the_declared_routes_refuse_an_api_key` classifies by
// it.
pub(crate) fn refuse_a_device_credential(extensions: &axum::http::Extensions) -> Option<Response> {
    if extensions
        .get::<crate::auth::By>()
        .is_some_and(|by| by.may_administer())
    {
        return None;
    }
    Some(
        (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"an API key reads a library; it does not administer one. Sign in from a browser to do that"}"#,
        )
            .into_response(),
    )
}
/// **The one gate every administrative route in this server opens with**: an
/// administrator, *and* a browser session rather than a device credential.
///
/// # Why the two questions are one gate
///
/// A role check alone would let an administrator's **API key**, the credential
/// typed into a third-party OPDS reader and hashed fast on the argument that it
/// rides every page request, reach the rule editor, the roots API,
/// `DELETE /api/library/roots/{id}` — which spec C4 §2 calls the most
/// destructive action in the product — the proposals inbox, and the four
/// metadata PATCHes. [`refuse_a_device_credential`] closes the same shape for
/// the self-service account routes, and this gate closes it for the
/// administrative ones (O93).
///
/// # The rule, stated so a new route cannot miss it
///
/// **A route that asks whether you are an administrator asks this instead.**
/// No function in this crate answers only the first half, which is the property
/// `auth::tests::no_module_answers_the_role_question_alone` holds.
///
/// **O58's two routes are deliberately untouched**, and they are not an
/// exception to the rule: `put_truth` and `patch_alignments` take no
/// `Principal` at all and are not administrative — any authenticated reader
/// may write a truth document. What credential kind *should* be allowed to
/// rewrite a shared document is the question O58 holds open, and it is a
/// different question from this one.
pub(crate) fn administering_or_forbidden(
    principal: &crate::auth::Principal,
    extensions: &axum::http::Extensions,
) -> Option<Response> {
    if principal.role != crate::auth::Role::Admin {
        return Some((StatusCode::FORBIDDEN, "this route is an administrator's").into_response());
    }
    refuse_a_device_credential(extensions)
}

/// Everything the account screen reads and writes.
///
/// **Each of these is scoped to the principal**, never to an id in the URL:
/// `/api/keys/{id}` revokes a key of *this* account, and a key id belonging to
/// somebody else is a 404 rather than a revocation. Spec A2 §9 ships no user
/// administration, so there is deliberately no route here that names another
/// account at all.
pub(crate) async fn list_keys(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
) -> Response {
    let Some(user_id) = principal_of(&extensions).map(|p| p.user_id) else {
        return shut();
    };
    if let Some(refusal) = refuse_a_device_credential(&extensions) {
        return refusal;
    }
    let listed = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        taiyaku_library::account::keys_of(&conn, user_id)
    })
    .await;
    match listed {
        Ok(Ok(keys)) => (axum::Json(
            keys.into_iter()
                .map(|key| {
                    serde_json::json!({
                        "id": key.id,
                        "name": key.name,
                        "created": key.created,
                        "last_used": key.last_used,
                    })
                })
                .collect::<Vec<_>>(),
        ),)
            .into_response(),
        _ => failed("listing keys"),
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct NamedBody {
    name: String,
}

pub(crate) async fn create_key(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    axum::Json(body): axum::Json<NamedBody>,
) -> Response {
    let Some(user_id) = principal_of(&extensions).map(|p| p.user_id) else {
        return shut();
    };
    if let Some(refusal) = refuse_a_device_credential(&extensions) {
        return refusal;
    }
    if body.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "a key needs a name, so the right one can be revoked later",
        )
            .into_response();
    }
    let created = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        taiyaku_library::account::create_key(&conn, user_id, &body.name)
    })
    .await;
    match created {
        // **The only time the secret leaves the server.** Only its sha256 is
        // stored, so there is no route that could answer it a second time.
        Ok(Ok(secret)) => (axum::Json(serde_json::json!({ "secret": secret })),).into_response(),
        _ => failed("creating a key"),
    }
}

pub(crate) async fn revoke_key(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Response {
    let Some(user_id) = principal_of(&extensions).map(|p| p.user_id) else {
        return shut();
    };
    if let Some(refusal) = refuse_a_device_credential(&extensions) {
        return refusal;
    }
    // Scoped in SQL rather than by listing every key and scanning for a boolean:
    // the ownership check belongs in the function, where the next caller
    // inherits it, rather than at the one call site that happens to have it.
    let revoked = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        taiyaku_library::account::revoke_key(&conn, user_id, id)
    })
    .await;
    match revoked {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => (StatusCode::NOT_FOUND, "no such key").into_response(),
        _ => failed("revoking a key"),
    }
}

pub(crate) async fn list_sessions(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some(user_id) = principal_of(&extensions).map(|p| p.user_id) else {
        return shut();
    };
    if let Some(refusal) = refuse_a_device_credential(&extensions) {
        return refusal;
    }
    let token = token_of(&headers);
    let listed = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        taiyaku_library::account::sessions_of(&conn, user_id, token.as_deref())
    })
    .await;
    match listed {
        Ok(Ok(sessions)) => (axum::Json(
            sessions
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "created": s.created,
                        "last_seen": s.last_seen,
                        "user_agent": s.user_agent,
                        "ip": s.ip,
                        "current": s.current,
                    })
                })
                .collect::<Vec<_>>(),
        ),)
            .into_response(),
        _ => failed("listing sessions"),
    }
}

pub(crate) async fn revoke_session(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Some(user_id) = principal_of(&extensions).map(|p| p.user_id) else {
        return shut();
    };
    if let Some(refusal) = refuse_a_device_credential(&extensions) {
        return refusal;
    }
    let revoked = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        taiyaku_library::account::revoke_session(&conn, user_id, &id)
    })
    .await;
    match revoked {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => (StatusCode::NOT_FOUND, "no such session").into_response(),
        _ => failed("revoking a session"),
    }
}

/// What one password change came to.
///
/// Three answers rather than a bool, because a client retries differently for
/// each — the same split `Attempt` makes on the login route, and for the same
/// reason.
enum Outcome {
    Changed,
    Wrong,
    TooMany,
}

#[derive(serde::Deserialize)]
pub(crate) struct PasswordChange {
    current: String,
    next: String,
}

/// `PUT /api/password` — change this account's own password.
///
/// **The current password is required even though the session already proves
/// who this is.** A session left open on a shared machine is exactly the case
/// where the difference matters: the cookie says "this browser was signed in",
/// and only the password says "the person here knows it". Without this, a
/// borrowed laptop is a permanent account takeover.
pub(crate) async fn change_password(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<PasswordChange>,
) -> Response {
    let Some(user_id) = principal_of(&extensions).map(|p| p.user_id) else {
        return shut();
    };
    if let Some(refusal) = refuse_a_device_credential(&extensions) {
        return refusal;
    }
    // **No `refuse_cross_site` here.** This is a guarded route, so the cookie
    // path runs the origin check inside `from_cookie`; re-applying it would
    // refuse every client authenticating with `Authorization: Basic` for having
    // no `Origin`, with a message about sessions. A header credential is not
    // CSRF-able, which is the stated reason `origin_is_acceptable` is
    // documented as the cookie path only. The `refuse_cross_site` call sites
    // are all `open_routes` handlers, which cannot inherit the guard's check.
    if body.next.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "an empty password would lock the account rather than open it",
        )
            .into_response();
    }
    let address = crate::auth::client_of(&state.auth, &extensions, &headers);
    let token = token_of(&headers);

    let changed = tokio::task::spawn_blocking(move || {
        use taiyaku_library::account;

        // **§7 applies here too**: this verifies the current password with
        // argon2, and without counting the failures it would allow unlimited
        // guesses at the one secret standing between a borrowed session and
        // permanent account takeover, which is the exact scenario this route's
        // own doc says the current-password requirement exists for.
        // **Keyed on the account as well as the address.** A bare
        // `"  change-password"` would make one counter for every reader behind
        // one address: a household where somebody mistyped their old password
        // ten times would lock everyone at that address out of changing theirs,
        // and a guesser working on one account would spend a budget the others
        // need. The leading spaces are the sentinel that keeps this out of
        // `by_name`'s namespace — a name reaching `attempt_key` from the login
        // route comes from a JSON body or a Basic header, either of which
        // could carry this string, so it is a separation of concerns rather
        // than a guarantee.
        let key = account::attempt_key(
            &address.to_string(),
            &format!("  change-password {user_id}"),
        );
        // Asked twice below, so written once.
        let locked_out = || {
            let conn = state.library.conn();
            account::is_locked_out(&conn, &key).unwrap_or(true)
        };
        if locked_out() {
            return Some(Outcome::TooMany);
        }
        // O57: a permit, and the same question again under it, the permit
        // kept until a failure is recorded. `login` has the reasoning. This
        // route is behind the guard, so its burst is one account's.
        let mut permit = state.auth.verifies.permit();
        if locked_out() {
            return Some(Outcome::TooMany);
        }

        let phc = {
            let conn = state.library.conn();
            let name: String = conn
                .query_row("SELECT name FROM user WHERE id = ?1", [user_id], |row| {
                    row.get(0)
                })
                .ok()?;
            account::by_name(&conn, &name).ok()??.1
        };
        // Verified holding the permit and not the connection.
        if !verify_holding(&mut permit, &body.current, &phc) {
            let conn = state.library.conn();
            if account::note_failure(&conn, &key).is_err() {
                return Some(Outcome::TooMany);
            }
            return Some(Outcome::Wrong);
        }
        // Given back before the new password is hashed. `set_password` hashes
        // while it holds the connection, as every route that sets a password
        // does, and none of them holds a permit.
        drop(permit);
        let conn = state.library.conn();
        // `token` is kept, so a reader is not signed out of the browser they
        // changed it from. Every other session ends (§4).
        account::set_password(&conn, user_id, &body.next, token.as_deref()).ok()?;
        // Getting it right forgets the guesses before it.
        let _ = account::clear_failures(&conn, &key);
        Some(Outcome::Changed)
    })
    .await;

    match changed {
        Ok(Some(Outcome::Changed)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Some(Outcome::Wrong)) => {
            (StatusCode::FORBIDDEN, "that current password did not match").into_response()
        }
        Ok(Some(Outcome::TooMany)) => (
            StatusCode::TOO_MANY_REQUESTS,
            "too many failed attempts; try again shortly",
        )
            .into_response(),
        _ => failed("changing the password"),
    }
}

fn failed(what: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{what} failed")).into_response()
}

/// What a closed claim route answers. See [`claim`] for why it is a 404.
fn shut() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

/// `GET /api/me` — who the guard resolved this request to.
///
/// **Behind the guard**, so a 401 here is the reader's signal to show its login
/// screen. That is the whole of its job: the reader calls it on load and routes
/// on the answer rather than discovering it is signed out halfway through
/// fetching a page.
pub(crate) async fn whoami(request: Request) -> Response {
    let Some(principal) = request.extensions().get::<Principal>() else {
        // Unreachable behind the guard, which inserts one or refuses. Answered
        // rather than unwrapped so a future routing mistake is a 401 and not a
        // panic in the one endpoint whose job is to say whether you are signed
        // in.
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    (axum::Json(serde_json::json!({
    "user_id": principal.user_id,
    "admin": principal.role == crate::auth::Role::Admin,
    "can_download": principal.can_download,
    "can_stream": principal.can_stream,
    "can_stamp": principal.can_stamp,
    })),)
        .into_response()
}

#[cfg(test)]
mod tests;
