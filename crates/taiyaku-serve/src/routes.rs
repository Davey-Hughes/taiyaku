//! HTTP. Everything interesting happens below this file.

// **Imported rather than written `std::collections::` inline, and that is
// load-bearing for the mutation harness — O88.** `cargo-mutants` replaces a
// function body with a literal it generates and emits the SHORT type name, so
// a function *returning* a fully-qualified collection has whole-body mutants
// that fail to compile and are reported `unviable` — never run, and invisible
// in the survivor list, because an unviable mutant is not a survivor.
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Path as AxPath, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use taiyaku_core::FromFull;
use taiyaku_library::account::Access;

use crate::{ServeState, Work};

/// An error a client can act on, rather than a bare status.
///
/// Every failure here names the thing that was not found, because a reader
/// showing a blank page with no explanation is the failure mode this whole
/// project exists to avoid.
pub(crate) fn not_found(message: String) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

/// A URL that does not address anything, as against one addressing something
/// this server does not have.
///
/// Separate from `not_found` because they are genuinely different and a client
/// is entitled to tell them apart: `pages/4` names a page an edition does not
/// have, `pages/abc` does not name a page at all.
fn bad_request(message: String) -> Response {
    (StatusCode::BAD_REQUEST, message).into_response()
}

/// The page index a URL carries, or the refusal to send instead.
///
/// O5: the two routes addressed by page index — this module's `/api/…` route
/// and the OPDS stream — refuse a malformed one here rather than through
/// axum's `Path` rejection, which is phrased in terms of the Rust type it
/// failed to parse into and the tuple position it sat at ("Cannot parse value
/// at index 2 with value `abc` to a `u32`"), which names two things a client
/// knows nothing about.
///
/// Shared rather than written twice, for the reason `ServeState::page` is
/// shared: the two routes are two spellings of one operation, and answering a
/// malformed index two different ways is the drift that starts the moment
/// there are two copies.
pub(crate) fn page_index(raw: &str, edition: &str) -> Result<u32, NotAPageIndex> {
    raw.parse::<u32>().map_err(|_| {
        NotAPageIndex(format!(
            "`{raw}` is not a page number; pages of `{edition}` are numbered from 0"
        ))
    })
}

/// A URL segment that was supposed to be a page index and was not.
///
/// A named type rather than a bare `Response`, for the same reason
/// `NoSuchWork` below is one: a `Response` is 128 bytes and
/// `Result<u32, Response>` trips `clippy::result_large_err`.
#[derive(Debug)]
pub(crate) struct NotAPageIndex(String);

impl IntoResponse for NotAPageIndex {
    fn into_response(self) -> Response {
        bad_request(self.0)
    }
}

/// A page's `ETag`: the member's `sha256` and the page's offset within it —
/// the same key `cover::thumb_cache_path` derives a cache filename from, and
/// for the same reason. A member holds one `sha256` and many pages; the
/// offset is what tells them apart. Keying on the member hash alone would
/// answer `304` for every page after the first one a client had fetched —
/// the entire member would look unchanged.
///
/// Quoted, per RFC 9110 §8.8.3: an `ETag`'s value is a `quoted-string`, not
/// a bare token.
/// **The requested long edge is part of the tag**, and leaving it out would be
/// the subtle way to break this. One page has as many representations as a
/// reader has viewport sizes; a tag naming only the member and offset would let
/// a client that cached the 1024px version be answered `304` when it later
/// asked for 2048, and go on drawing the small one — a page that looks soft for
/// a day with nothing to point at. `None` is the full-size request, whose tag
/// names only the member and offset.
///
/// **A resized tag is `cover::page_key`, quoted, and so carries the format
/// version as well (O106).** The same trap applies: a resized page is a
/// rendering, and a bump of `PAGE_FORMAT_VERSION` is a new rendering under an
/// unchanged size. The full-size tag is left alone: the member's own bytes do
/// not depend on how this server resizes anything.
pub(crate) fn page_etag(sha256: &str, offset: u32, long_edge: Option<u32>) -> String {
    match long_edge {
        Some(edge) => format!("\"{}\"", crate::cover::page_key(sha256, offset, edge)),
        None => format!("\"{sha256}-p{offset}\""),
    }
}

/// Whether a request's `If-None-Match` already names `tag`.
///
/// A direct compare against the header's raw text, rather than a full parse
/// of the comma-separated list or `*` the grammar allows for: this server
/// mints every tag it is ever compared against and sends exactly one per
/// response, so a client echoing one back sends exactly what it received.
/// The extra generality would guard against a request shape nothing here has
/// been seen to send.
pub(crate) fn if_none_match_hits(headers: &HeaderMap, tag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        == Some(tag)
}

/// How long a client may reuse a page, tile or cover without asking at all.
///
/// O1: an `ETag` shrinks a repeat to a small response; it does not remove the
/// round trip. Prefetch exists to hide a round trip before the reader notices
/// it, so a round trip that still has to happen — just for fewer bytes — is one
/// the reader still waits on.
///
/// `private` because the cache this exists to reach is the reader's own browser
/// rather than a shared proxy.
///
/// **Not `immutable`, and the reason is the URL rather than the bytes.** A
/// page's bytes never change for a given member `sha256` and offset — that is
/// exactly what `page_etag` above is derived from — but the URL names a work,
/// an edition and an index, none of which move when an archive is replaced.
/// `immutable` tells a client never to revalidate for the entry's whole
/// lifetime, so replacing a member would leave every reader who had seen that
/// page holding the old bytes, with no way to correct it short of clearing the
/// browser's cache.
///
/// **No `Last-Modified` alongside it, deliberately.** It would need the
/// member file's mtime, which needs a `stat`, on the async worker, on every
/// request — putting disk I/O back on the one path `EditionView::locate` is
/// written to keep clear of. It would also be the weaker validator of the two:
/// these bytes are addressed by content, and mtime is not, so re-downloading a
/// byte-identical archive would invalidate a cache entry that is still correct.
/// The `ETag` covers revalidation; this covers freshness; there is no third
/// question for a date to answer.
///
/// **`no-cache` rather than a freshness lifetime, because the URL is not a
/// unique name for these bytes.** A **work id is not unique**. It is the
/// sidecar's filename stem, and every sidecar across this corpus is
/// `taiyaku.json`, so `/api/works/taiyaku/raw-mono/pages/10` addresses a
/// different volume's page 10 depending only on which sidecar the server was
/// started with. The server can tell them apart, since the `ETag`s derive from
/// different member digests; a client holding a day's freshness would never
/// ask. A labeller restarting the server onto the next volume would see the
/// previous volume's artwork with the new volume's page numbers, and the only
/// symptom would be recognising the pictures.
///
/// That is worse than a stale read. Boxes drawn against it would be labels on
/// the wrong art, which `TruthEdition::is_stale_for` calls the sharpest form
/// of the staleness problem, and nothing downstream could detect it.
///
/// `no-cache` does not mean "do not store": the entry is kept and revalidated,
/// so the `ETag` above does its work and a page that really is unchanged comes
/// back as a bodiless `304`. The cost is one conditional request per page
/// rather than none; the alternative is a client that cannot be told it is
/// looking at the wrong book.
///
/// A cover whose address carries the version of the bytes it serves is the
/// one exception: `COVER_CACHE_CONTROL`.
pub(crate) const PAGE_CACHE_CONTROL: &str = "private, no-cache";

/// A library cover whose `?v=` is the `cover::cover_version` of the bytes it
/// serves: kept for good, never revalidated.
///
/// `PAGE_CACHE_CONTROL`'s reason for `no-cache` is that its URL does not name
/// its bytes. This address does: the version is the cover's `ETag` without the
/// size, so a replaced member or a new `PAGE_FORMAT_VERSION` gives the cover a
/// new address, and the kept one is not asked for again. A `v` that names
/// other bytes than these is answered with `PAGE_CACHE_CONTROL`, so no address
/// is kept for good over bytes it does not name. `private` keeps it out of a
/// shared cache: a cover is answered only to an account whose grants show it.
pub(crate) const COVER_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

/// The reply to a validator that already matches: the tag, and no body —
/// sending the very bytes the header exists to avoid transferring would
/// defeat the entire point of a `304`.
///
/// **`freshness` is a parameter rather than a constant, and its callers do not
/// agree on it.** RFC 9111 §4.3.4 has a client update its stored response from
/// a `304`'s headers, so whatever this sends is what the client believes until
/// it asks again. For a page that is `PAGE_CACHE_CONTROL`: omitting it would
/// leave the entry exactly as stale as it was and cost a round trip on the very
/// next read, so a revalidation that carries no freshness buys one response and
/// no more. For a cover whose `?v=` names its bytes it is `COVER_CACHE_CONTROL`,
/// or the `304` would put a cover kept for good back to asking on every visit.
/// For a sidecar or a notes document it is `None`, and that is not a smaller
/// version of the same thing — those two are rewritten by `PATCH /alignments`
/// and `PUT /notes`, so a freshness lifetime on either says a reader may go a
/// day without seeing a correction they made themselves.
///
/// Written as an argument each caller must supply rather than a default one
/// of them overrides, because `get_sidecar` and `get_notes` share this function
/// and want the opposite answer from a page's. A caller that has to say which
/// it is cannot inherit the wrong one by being written later.
pub(crate) fn not_modified(tag: &str, freshness: Option<&str>) -> Response {
    let mut response =
        (StatusCode::NOT_MODIFIED, [(header::ETAG, tag.to_string())]).into_response();
    if let Some(freshness) = freshness {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_str(freshness).expect("freshness is a valid header value"),
        );
    }
    response
}

/// A sidecar's `ETag`: a SHA-256 over exactly the bytes this server would send.
///
/// Over the serialised document rather than a version counter, because the
/// point is that two servers, or one server across a restart, agree about
/// whether a client's copy is current — a counter would restart at zero and
/// silently validate a stale copy. Computed on demand rather than cached: a
/// sidecar is tens of kilobytes and this route is asked once per page load,
/// against a `PATCH` path where being wrong costs a lost correction.
pub(crate) fn sidecar_etag(sidecar: &taiyaku_core::Sidecar) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(sidecar.to_json().as_bytes());
    let digest = hasher.finalize();
    let hex = digest.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
    format!("\"{hex}\"")
}

/// A notes or truth document's `ETag` as `Access::Every` is sent it: a
/// SHA-256 over exactly the bytes being sent.
///
/// Lives in `crate::notes` as `etag_for`, beside `notes::to_json`, the
/// projection it is computed over — the two are the invariant `notes.rs`'s and
/// `truth.rs`'s own doc comments state, and keeping them in the same module is
/// what makes that visible rather than asserted. Routes take it through
/// `View::etag_naming_numbering`; tests, as `notes_etag`.
#[cfg(test)]
pub(crate) use crate::notes::etag_for as notes_etag;

/// Whether a request's `If-Match` names `tag`.
///
/// `None` where the header is absent, which is a different answer from `false`
/// and gets a different status: a client that sent no validator has not lost a
/// race, it has not entered one, and telling it "precondition failed" would
/// send it looking for a conflict that does not exist.
///
/// Proven directly by `if_match_distinguishes_absent_from_matching_from_stale`
/// below, as well as through `patch_alignments`.
pub(crate) fn if_match_hits(headers: &HeaderMap, tag: &str) -> Option<bool> {
    let sent = headers.get(header::IF_MATCH)?.to_str().ok()?;
    Some(sent == tag)
}

/// A request naming a work this server does not hold.
///
/// A named type rather than a bare `Response` because a `Response` is 128
/// bytes and `Result<Arc<Work>, Response>` trips `clippy::result_large_err`. It
/// carries the status and the wording together, which is the point: one
/// answer to one question, wherever it is asked.
#[derive(Debug)]
pub(crate) struct NoSuchWork(String);

impl IntoResponse for NoSuchWork {
    fn into_response(self) -> Response {
        not_found(self.0)
    }
}

/// The work a route names, or the refusal to send instead, for a handler on
/// a runtime worker. `ServeState::find` decides the wording and does the
/// refresh a miss needs off the worker. This only wraps it in the
/// `Response`-carrying type the view's loaders (`view.rs`) want.
pub(crate) async fn work(state: &Arc<ServeState>, id: &str) -> Result<Arc<Work>, NoSuchWork> {
    state.find(id).await.map_err(NoSuchWork)
}

/// Why a progress request could not be answered.
///
/// Two kinds, and they are not one status: a work this server does not hold, or
/// an edition or slot the work does not have, is the client's mistake and a
/// 404; a SQLite read or write that fails is this server's and a 500.
/// `get_page` needs no such type because both of its failures are 404s. Here
/// the distinction has to survive the trip back out of `spawn_blocking`, which
/// flattens a handler's failures into one `Result` however different they were
/// on the way in.
#[derive(Debug)]
enum NoProgress {
    NotFound(String),
    Failed(String),
}

impl IntoResponse for NoProgress {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => not_found(message),
            Self::Failed(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
        }
    }
}

/// Every work this caller's grant admits and no work marked missing (O91),
/// as `GET /api/works` answers it: slug, title, series, volume, editions and
/// a cover URL; `works::WorkSummary` says where each of the six comes from.
///
/// **Applies the caller's grant.** A list that answered every work the server
/// held would hand a restricted account the id, title, series and volume of
/// every work outside every root it was granted — the same shape of gap
/// `view::of_edition` closes on the page routes and
/// `opds::visible_editions_by_work` closes on the OPDS mirror.
///
/// **Filtered `editions` too, and the `cover` URL built from a visible one.**
/// A filter at the work level alone — does this work keep *any* edition under
/// a granted root — would report every edition of each surviving work: a work
/// split across roots (`en` granted, `raw` withheld) would name `raw` in its
/// `editions` array and build its `cover` from `sidecar.hub` unconditionally,
/// so a hidden hub's key would reach the client through a thumbnail URL.
/// Filtered per edition, through `works::summary`'s `visible` parameter — the
/// same per-edition question `get_sidecar` and `patch_alignments` ask through
/// `view::flags_or_refuse`, asked here for every work in the library at once
/// so this route stays a list rather than becoming a lookup.
///
/// **A list, not a lookup: filtered, not refused.** A restricted account gets
/// the works it may see and simply does not learn about the rest; refusing
/// the whole route the way `view::of_edition` refuses a single edition would
/// hide the library from every restricted reader, not just the parts outside
/// their grant.
///
/// **One query for the whole list, not one per work.** Asking
/// `browse::work` per work here — the tool the single-work routes use — would
/// be an N+1 at the edition level: `browse::visible_editions` answers every
/// work's visible edition set in one round trip, and this filter is then only
/// a `HashSet` lookup per edition. `Access::Every` skips that query entirely,
/// the same short-circuit `view::of_edition` applies.
///
/// **Not a home screen.** Nothing here builds one.
///
/// One connection, opened once, rather than `works::summary` opening its own
/// through a `Library`-taking wrapper the way `progress::load` does — a list
/// answers every work in one request, so one lock over the whole pass beats
/// re-acquiring `state.library`'s mutex per work.
async fn list_works(
    State(state): State<Arc<ServeState>>,
    request_extensions: axum::http::Extensions,
) -> Response {
    let access = crate::auth::access_of(&request_extensions);

    // A SELECT per work against `state.library` — `serve_page`'s reasoning
    // about the async worker applies unchanged.
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let visible = if access == Access::Every {
            None
        } else {
            Some(taiyaku_library::browse::visible_editions(&conn, &access)?)
        };
        let held = state.works.snapshot();
        held.iter()
            // Before the grant, because `Access::Every` asks no grant and
            // only the registry's mark leaves a missing work out for it.
            .filter(|w| !w.is_missing())
            // A work absent from `visible`'s keys has no edition inside the
            // grant at all — `browse::visible_editions` never mints an entry
            // for one — so it is dropped here, and a work present in it always
            // has a non-empty edition set for `summary` to fall back on.
            .filter_map(|w| match &visible {
                None => Some(crate::works::summary(&conn, w, None)),
                Some(map) => map
                    .get(&w.id)
                    .map(|keys| crate::works::summary(&conn, w, Some(keys))),
            })
            .collect::<Result<Vec<_>, String>>()
    })
    .await;

    match built {
        Ok(Ok(list)) => axum::Json(list).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("listing works panicked: {e}"),
        )
            .into_response(),
    }
}

/// `check`, asked of the library on the blocking pool for a reader whose
/// access is not `Every`; `unrestricted` for `Every`, without asking; and a
/// 401 for a request with no principal, which must see nothing rather than
/// everything (the guard inserts one on every guarded route, so this is a
/// routing mistake).
///
/// **Off the async worker, because the library is a mutex — O115.** A grant is
/// answered in SQL, and taking `state.library.conn()` in the handler itself
/// would make every request that asks, from a reader with roots, wait on that
/// mutex on a tokio worker thread, stalling the whole worker, before its own
/// `spawn_blocking` ran. `serve_page`'s reasoning about the async worker
/// applies to the check as it does to the read behind it. `opds::works` asks
/// through this and `opds::work_feed` through [`crate::view::of_work`]
/// (O119).
///
/// **`Every` never leaves the async worker**: it is answered without the
/// library, so the unrestricted reader pays for neither the query nor the trip
/// to the pool.
///
/// `Err` is the refusal to send: the 401, or a 500 when `check` panicked.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, as every refusal-returning function in this file \
              does: a routing handler's own refusal path, called once per request, not a hot \
              loop, so the bytes clippy measures are not worth an indirection at every call site"
)]
pub(crate) async fn on_the_grant<T: Send + 'static>(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
    unrestricted: T,
    check: impl FnOnce(&rusqlite::Connection, &crate::auth::Principal) -> T + Send + 'static,
) -> Result<T, Response> {
    let Some(principal) = extensions.get::<crate::auth::Principal>() else {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized").into_response());
    };
    if principal.access == Access::Every {
        return Ok(unrestricted);
    }
    let state = Arc::clone(state);
    let principal = principal.clone();
    tokio::task::spawn_blocking(move || check(&state.library.conn(), &principal))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("checking the grant panicked: {e}"),
            )
                .into_response()
        })
}

/// **403, not 404, and that is the design.** `view::of_edition` answers
/// 404 because an edition outside the grant does not admit that it exists;
/// this answers 403 because the caller *may* see this edition and may not have
/// a copy of it — a different fact that a 404 would lie about.
///
/// A fail-open authorization gate is not made safe by a comment promising a
/// later task. The `can_stream` half sits beside this one. Half of O54.
pub(crate) fn refuse_if_cannot_download(extensions: &axum::http::Extensions) -> Option<Response> {
    let principal = extensions.get::<crate::auth::Principal>()?;
    (!principal.can_download).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may browse and read but not download",
        )
            .into_response()
    })
}

/// Whether this request may be shown a page's own pixels.
///
/// The rule [`refuse_if_cannot_stream`] refuses on, as a question, so the two
/// callers that need it — that refusal and `get_thumb`'s sized tile (O134) —
/// read one statement of it rather than two that can drift apart.
///
/// **No `Principal` means yes**, which is [`refuse_if_cannot_stream`]'s own
/// behaviour: a server configured without auth puts none in the extensions
/// and every route here is open in that configuration. The same shape as
/// `opds.rs`'s `can_download` check.
pub(crate) fn can_stream(extensions: &axum::http::Extensions) -> bool {
    extensions
        .get::<crate::auth::Principal>()
        .is_none_or(|principal| principal.can_stream)
}

/// The same for reading pages in place.
///
/// **Gates `/pages` and the OPDS PSE stream, and deliberately not `/thumb`, the
/// library covers, or any browse route.** The line is there because a
/// download-only account must still be able to *see* its shelf: an account that
/// cannot render a cover cannot choose what to download, and "you may take
/// volumes away but not read them here" is a coherent policy while "you may
/// take volumes away but not see which ones exist" is not. Thumbnails are how
/// the shelf renders, so they follow browse.
///
/// Both columns default to 1 (`schema/004_auth.sql:18`). Spec C2 §2.2.
pub(crate) fn refuse_if_cannot_stream(extensions: &axum::http::Extensions) -> Option<Response> {
    (!can_stream(extensions)).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may browse and download but not read pages here",
        )
            .into_response()
    })
}

/// The same for rewriting a work's archives — spec C3 §6, the one route in
/// this product that modifies a file the user owns.
///
/// **Unreachable by an administrator who was not granted it**, which is what
/// separates this from the two above. [`crate::auth::Principal::can_stamp`]
/// is the column rather than `admin || column`; this function does not
/// restate that rule, it reads the field that holds it.
///
/// Defaults to 0 (`schema/013_can_stamp.sql`).
pub(crate) fn refuse_if_cannot_stamp(extensions: &axum::http::Extensions) -> Option<Response> {
    let principal = extensions.get::<crate::auth::Principal>()?;
    (!principal.can_stamp).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may edit metadata but not rewrite archives",
        )
            .into_response()
    })
}

/// [`crate::view::flags_or_refuse`] for `patch_alignments`, and each side of
/// the correction with no visible member in those flags refused as the work
/// is, a 404 naming it, so that a refusal tells no one which slugs are real
/// and the sides are judged by the one answer the view is built from. On the
/// blocking pool, as `routes::tests::no_grant_check_stalls_the_runtime`
/// measures, the work first, then `from`, then `to`.
///
/// `sides` is `None` for a body that did not parse. The handler refuses that
/// body only after this has answered, so a hidden work is still refused
/// before a malformed correction is.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `on_the_grant`'s reason"
)]
async fn visible_members_for_a_correction(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
    slug: &str,
    sides: Option<(String, String)>,
) -> Result<Option<HashMap<String, Vec<taiyaku_library::browse::SeenMember>>>, Response> {
    let slug = slug.to_string();
    on_the_grant(state, extensions, Ok(None), move |conn, principal| {
        let flags = crate::view::flags_or_refuse(conn, &principal.access, &slug, None)?;
        for side in sides.iter().flat_map(|(from, to)| [from, to]) {
            let seen = flags
                .get(side)
                .is_some_and(|members| members.iter().any(|member| member.visible));
            if !seen {
                return Err(not_found(format!("no work `{slug}`")));
            }
        }
        Ok(Some(flags))
    })
    .await
    .and_then(|answer| answer)
}

/// A copy of `sidecar` with every edition and alignment outside `visible`
/// removed — what [`crate::view::View::build`] starts a restricted caller's
/// document from, and what [`crate::view::View::visible_editions_in_full`] checks
/// a notes write against.
///
/// **Both `editions` and `alignments` are filtered, not only the former.**
/// An alignment names two editions on the wire (`from`/`to`); one naming a
/// withheld edition on either side is that edition's key leaking through a
/// second field, which is exactly the kind of second spelling this function
/// exists to avoid a caller inventing at the call site.
///
/// **`hub` is checked too, and substituted rather than left dangling.** A
/// grant can hide the hub edition precisely as it can hide any other —
/// `browse::cover_target`'s own doc comment names the same trap for a
/// series cover. `editions` is a `BTreeMap`, so its `keys()` are already
/// sorted; the first surviving one is the deterministic fallback, the same
/// tiebreak `works::summary` uses for a list entry's cover when its hub is
/// hidden. The caller is expected to have already refused a `slug` with no
/// visible edition at all (`View::build` answers `None` for one), so this
/// always finds a fallback when it needs one.
pub(crate) fn filter_sidecar_editions(
    sidecar: &taiyaku_core::Sidecar,
    visible: &HashSet<String>,
) -> taiyaku_core::Sidecar {
    let mut filtered = sidecar.clone();
    filtered.editions.retain(|key, _| visible.contains(key));
    filtered
        .alignments
        .retain(|a| visible.contains(&a.from) && visible.contains(&a.to));
    if !visible.contains(&filtered.hub)
        && let Some(first) = filtered.editions.keys().next()
    {
        filtered.hub = first.clone();
    }
    filtered
}

/// The work's document as this account is sent it ([`crate::view::View`]),
/// with an `ETag` over that document.
///
/// **A missing work answers 404 with a reason** (O91): JSON,
/// `{"error": <sentence>, "missing": true}`, so the reader tells it from a
/// work that does not exist and says its files are gone. Asked only after
/// `view::of_work`'s grant check, so a work outside the grant answers
/// `` no work `{slug}` `` as before, missing or not, and the reason never
/// tells a reader a hidden work exists.
async fn get_sidecar(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    request_extensions: axum::http::Extensions,
) -> Response {
    let view = match crate::view::of_work(&state, &request_extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };
    if state.works.get(&id).is_some_and(|work| work.is_missing()) {
        return missing_work(&id);
    }
    let sidecar = view.sidecar();
    // The `ETag` is over the document this account is sent (spec §3.8): two
    // accounts sent different documents must not revalidate each other's
    // copies, and a withheld edition changing must not change this one's tag.
    // `patch_alignments` compares `If-Match` with the same view's tag.
    let tag = sidecar_etag(sidecar);
    if if_none_match_hits(&headers, &tag) {
        return not_modified(&tag, None);
    }
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, tag.as_str()),
        ],
        sidecar.to_json(),
    )
        .into_response()
}

/// [`get_sidecar`]'s answer for a work marked missing.
fn missing_work(slug: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "error": missing_reason(slug),
            "missing": true,
        })),
    )
        .into_response()
}

/// Why a work marked missing will not open (O91). One sentence for every
/// route that refuses it: [`get_sidecar`] sends it in JSON, and the OPDS
/// work feed as its own plain-text 404.
pub(crate) fn missing_reason(slug: &str) -> String {
    format!(
        "the files of work `{slug}` are missing from every directory that held them; \
         it opens again once a scan finds them"
    )
}

/// The optional `?w=` on a page request: the long edge, in device pixels, the
/// reader intends to draw this page within.
///
/// **Why the server resizes at all.** A page far larger than the box it is
/// drawn into costs a decode and a decoded bitmap for many times the pixels
/// that reach the screen. Both halves matter: the decode is what a page turn
/// waits on, and the bitmap is what the browser evicts under memory pressure.
///
/// A query parameter rather than a separate route, unlike `get_thumb`: a tile
/// is a different *thing* — always JPEG, cropped for a strip, no validator —
/// where this is the same page at a different size, wanting the same
/// `If-None-Match` fast path and the same freshness. Absent `w` is the original
/// bytes, byte for byte, which is what `zoom` asks for and what a client that
/// sends no `w` gets.
#[derive(Debug, serde::Deserialize)]
struct PageQuery {
    w: Option<u32>,
}

async fn get_page(
    State(state): State<Arc<ServeState>>,
    // `String` rather than `u32`, so a malformed index is refused by
    // `page_index` in this server's own words rather than by axum's `Path`
    // rejection in terms of a Rust type — O5.
    AxPath((id, edition, raw)): AxPath<(String, String, String)>,
    Query(query): Query<PageQuery>,
    headers: HeaderMap,
    request_extensions: axum::http::Extensions,
) -> Response {
    let view = match crate::view::of_edition(&state, &request_extensions, &id, &edition).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };
    if let Some(refusal) = refuse_if_cannot_stream(&request_extensions) {
        return refusal;
    }
    let index = match page_index(&raw, &edition) {
        Ok(index) => index,
        Err(refusal) => return refusal.into_response(),
    };
    // Clamped rather than refused. A width outside the sensible range is a
    // client bug or a hand-typed URL, and the useful answer to both is the
    // page — at the nearest size this server will make — rather than a 400 that
    // leaves the reader with a blank cell. Zero and one are treated as absent,
    // and that filter is load-bearing rather than a shortcut:
    // `cover::fits_within` (O111) calls a page fitting only when it is already
    // within the edge, so without this filter `?w=1` hands a reader a one-pixel
    // JPEG rather than the page.
    // `a_width_of_one_is_absent_rather_than_a_one_pixel_page` is the witness.
    let long_edge = query
        .w
        .filter(|edge| *edge > 1)
        .map(|edge| edge.min(crate::cover::MAX_FIT_LONG_EDGE));

    serve_page(state, view, index, long_edge, None, &headers).await
}

/// A page, at `long_edge` or as the archive holds it, after whatever refusals
/// the route that called this has already made.
///
/// `get_page` calls it for every page, after its stream check; `get_thumb`
/// calls it for a library cover — page 0 at one of `cover::COVER_LONG_EDGES` —
/// without one (O133). One body, so the cover a download-only account sees is
/// exactly the image, the cache file and the validator the page route would
/// have given it.
///
/// `version` is a cover's `?v=`, and only `get_thumb`'s cover passes one: a
/// version equal to the `cover::cover_version` of the page located here makes
/// both the `200` and the `304` `COVER_CACHE_CONTROL`, and any other `200` or
/// `304` is `PAGE_CACHE_CONTROL`.
async fn serve_page(
    state: Arc<ServeState>,
    view: crate::view::EditionView,
    index: u32,
    long_edge: Option<u32>,
    version: Option<&str>,
    headers: &HeaderMap,
) -> Response {
    // O1: the validator comes from what the sidecar already holds in
    // memory — the member's recorded `sha256` and this page's offset within
    // it — so it can be checked here, on the async worker, before
    // `spawn_blocking` below ever runs. A matching `If-None-Match` answers
    // `304` without a zip decompression or an HTTP round trip to Stump ever
    // happening — the whole point of sending a validator in the first
    // place.
    let index = match view.page_to_full(index) {
        Ok(index) => index,
        Err(message) => return not_found(message),
    };
    // Located, tagged and read in the sidecar `view` checked its flags
    // against, never the work's current one, which a refresh can have
    // replaced since with members the flags never named.
    let (sha256, offset) = match view.locate(index) {
        Ok((member, offset)) => (member.sha256.clone(), offset),
        Err(message) => return not_found(message),
    };
    let tag = page_etag(&sha256, offset, long_edge);
    // Compared against the page just located, so a `v` from a list read
    // before its member changed is answered `PAGE_CACHE_CONTROL`, and its
    // address is never kept over the new bytes. The `304` carries the same
    // policy as the `200`: RFC 9111 §4.3.4 has the client store a `304`'s
    // headers as its answer's.
    let freshness = if version.is_some_and(|v| v == crate::cover::cover_version(&sha256, offset)) {
        COVER_CACHE_CONTROL
    } else {
        PAGE_CACHE_CONTROL
    };
    if if_none_match_hits(headers, &tag) {
        return not_modified(&tag, Some(freshness));
    }

    // Reading a page blocks: a zip decompression from disk, or an HTTP round
    // trip to Stump. On a tokio worker that stalls the whole worker, and this
    // corpus holds pages of 430 MB — with the prefetching the filmstrip and
    // swap both need, one reader would occupy several of them.
    //
    // Resizing blocks harder still — it decodes — so it belongs on this same
    // thread rather than in a second `spawn_blocking` behind it. A cache hit
    // never reaches the fetch at all: `cached_or_generated_page` reads the
    // cached file first, and only a miss pays for the member. A miss reads the
    // member once, whether the page is then resized or already fits.
    //
    // `PageSource` stays synchronous deliberately. The boundary belongs here,
    // where there is exactly one of it, rather than in each implementation.
    let fetched = tokio::task::spawn_blocking(move || {
        let Some(edge) = long_edge else {
            return state.page(&view, index);
        };
        let dir = state.thumb_cache_dir();
        let fitted = crate::cover::cached_or_generated_page(&dir, &sha256, offset, edge, || {
            state.page(&view, index)
        })?;
        match fitted {
            // A resize always writes JPEG, and says so rather than echoing
            // whatever the archive held.
            crate::cover::Fitted::Resized(bytes) => Ok(crate::source::ServedPage {
                bytes,
                content_type: "image/jpeg".to_string(),
            }),
            // Already within the requested edge: the page as the fetch above
            // read it, with the content type its source gave it. That read is
            // the one that decided it fits, so serving it costs no second one.
            crate::cover::Fitted::Original(page) => Ok(page),
        }
    })
    .await;

    match fetched {
        Ok(Ok(page)) => (
            [
                (header::CONTENT_TYPE, page.content_type),
                (header::ETAG, tag),
                (header::CACHE_CONTROL, freshness.to_string()),
            ],
            page.bytes,
        )
            .into_response(),
        Ok(Err(e)) => not_found(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("page read panicked: {e}"),
        )
            .into_response(),
    }
}

/// The first value of `key` in a query string's pairs, if it has one.
///
/// Read from the raw pairs rather than through a struct, so a value that is not
/// a number, or a key given twice, is ignored rather than refused with a `400`
/// in terms of a Rust type (O133).
pub(crate) fn first_query_value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// The long edge to serve a tile request at, if it is a library cover: page 0,
/// at exactly one of `cover::COVER_LONG_EDGES`. Anything else is the tile.
///
/// Page 0 alone, and those sizes alone, because this route skips
/// `refuse_if_cannot_stream`: sizing any page to any width here would hand an
/// account that may not stream every page at a size worth reading.
fn cover_edge(index: u32, w: Option<&str>) -> Option<u32> {
    let edge = w?.parse::<u32>().ok()?;
    (index == 0 && crate::cover::COVER_LONG_EDGES.contains(&edge)).then_some(edge)
}

/// The long edge to serve a tile at, if the request asked for one this server
/// makes: **any** page, at exactly one of `cover::TILE_LONG_EDGES` — O134.
///
/// Unlike [`cover_edge`] this is not restricted to page 0, which is why its
/// caller pairs it with [`can_stream`]: sizing any page for an account that
/// may not read pages is exactly what `cover_edge`'s own doc refuses to do.
/// Anything else — an unknown size, a `w` that is not a number, no `w` at all
/// — is `None` and so the 160 px tile.
fn tile_edge(w: Option<&str>) -> Option<u32> {
    let edge = w?.parse::<u32>().ok()?;
    crate::cover::TILE_LONG_EDGES
        .contains(&edge)
        .then_some(edge)
}

/// A filmstrip tile for one page.
///
/// Separate from `get_page` rather than a query parameter on it, because the
/// two have different content types: a page is whatever the archive holds, a
/// tile is always JPEG. (A library cover, below, is the exception: it is the
/// page route's answer, so a page 0 already within the size is the archive's
/// own bytes and type.)
///
/// The tile sends `Cache-Control` and **no `ETag`**, which is not an oversight.
/// `get_page` can answer a `304` before doing any work because
/// `EditionView::locate` derives the tag from the in-memory sidecar; here the
/// member `sha256` and offset are found inside `spawn_blocking`, one level in,
/// so a validator would arrive too late to skip anything a tile costs. Adding
/// one would mean hoisting the lookup out here, its own change. A library cover
/// does carry one: it is served by `serve_page`, whose lookup runs out here.
///
/// **`?w=` makes page 0 a library cover** (O133): at 256, 512 or 768 px on its
/// long edge (`cover::COVER_LONG_EDGES`) it is served by `serve_page`, exactly
/// as the page route would size it, validator and all. It is here rather than
/// on the page route because covers must reach an account without
/// `can_stream`, which the page route refuses (`refuse_if_cannot_stream` says
/// why), and `cover_edge` keeps it to covers: any other page or size is the
/// tile. A `w` at one of `cover::TILE_LONG_EDGES` on any page is that tile at
/// that size (O134), for a caller that may stream (`tile_edge`); every other
/// `w` is the 160 px tile.
///
/// **`?v=` is read on a cover alone**, and `serve_page` decides whether it
/// names the bytes. A cover's version is made from `serve_page`'s rendering
/// (`cover::cover_version`), so it names no tile's bytes, and a tile with a `v`
/// is the tile.
async fn get_thumb(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition, raw)): AxPath<(String, String, String)>,
    Query(query): Query<Vec<(String, String)>>,
    headers: HeaderMap,
    request_extensions: axum::http::Extensions,
) -> Response {
    let view = match crate::view::of_edition(&state, &request_extensions, &id, &edition).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };
    let index = match page_index(&raw, &edition) {
        Ok(index) => index,
        Err(refusal) => return refusal.into_response(),
    };
    if let Some(edge) = cover_edge(index, first_query_value(&query, "w")) {
        let version = first_query_value(&query, "v");
        return serve_page(state, view, index, Some(edge), version, &headers).await;
    }
    // O134. Below `cover_edge` deliberately: page 0 at a *cover* size stays
    // O133's cover — the page route's own answer, validator and all — and a
    // tile rung is a tile. `can_stream` is what keeps this route's exemption
    // from `refuse_if_cannot_stream` (see `get_thumb`'s doc) from becoming a
    // way to read pages: an account without it is served the 160 px tile, the
    // same fallback an unrecognised `w` gets.
    let edge = tile_edge(first_query_value(&query, "w"))
        .filter(|_| can_stream(&request_extensions))
        .unwrap_or(crate::cover::THUMB_LONG_EDGE);
    let index = match view.page_to_full(index) {
        Ok(index) => index,
        Err(message) => return not_found(message),
    };
    // Decoding blocks and can allocate up to `MAX_DECODE_ALLOC`; `serve_page`'s
    // reasoning applies unchanged, and so does its reason to locate in the
    // sidecar `view` checked.
    let fetched = tokio::task::spawn_blocking(move || {
        let (member, offset) = view.locate(index)?;
        let sha = member.sha256.clone();
        let dir = state.thumb_cache_dir();
        crate::cover::cached_or_generated_thumb(&dir, &sha, offset, edge, || {
            state.page(&view, index).map(|p| p.bytes)
        })
    })
    .await;

    match fetched {
        Ok(Ok(bytes)) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, PAGE_CACHE_CONTROL),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(e)) => not_found(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("thumbnail read panicked: {e}"),
        )
            .into_response(),
    }
}

/// Why a notes request could not be answered.
///
/// `NoProgress`'s split, for the same reason: a work this server does not hold
/// is the client's mistake and a 404; a SQLite read or write that fails is this
/// server's and a 500. The write path adds a precondition answer, which carries
/// its own status because 428 ("you sent no validator") and 412 ("yours is
/// stale") are genuinely different answers and a client retries differently for
/// each — folding them into one status would send a client that has not entered
/// a race looking for a conflict.
#[derive(Debug)]
enum NoNotes {
    NotFound(String),
    /// Malformed in itself — a duplicate id, a region outside its slot, a
    /// document naming another work. Distinct from `NotFound`, which names
    /// something this work does not have.
    Refused(String),
    Precondition(StatusCode, String),
    Failed(String),
}

impl From<crate::notes::Invalid> for NoNotes {
    fn from(why: crate::notes::Invalid) -> Self {
        match why {
            crate::notes::Invalid::Missing(message) => Self::NotFound(message),
            crate::notes::Invalid::Refused(message) => Self::Refused(message),
        }
    }
}

impl IntoResponse for NoNotes {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => not_found(message),
            Self::Refused(message) => bad_request(message),
            Self::Precondition(status, message) => (status, message).into_response(),
            Self::Failed(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
        }
    }
}

/// The reader this request is, taken from the [`crate::auth::Principal`] the
/// guard resolved.
///
/// **A `Result` rather than an `Option`, and never a default.** Falling back to
/// a user id when the extension is absent would answer for everybody: a default
/// that is right for one reader is silently wrong for the second. The extension
/// is present on every guarded route by construction, so its absence is a
/// routing mistake, and a 500 naming it is the honest answer.
fn reader(request_extensions: &axum::http::Extensions) -> Result<i64, NoPrincipal> {
    request_extensions
        .get::<crate::auth::Principal>()
        .map(|principal| principal.user_id)
        .ok_or(NoPrincipal)
}

/// A guarded route reached without a principal, which is a routing mistake
/// rather than anything a client did.
///
/// A named zero-sized type rather than a bare `Response`, for the same reason
/// [`NotAPageIndex`] is one: a `Response` is 128 bytes and `Result<i64, Response>`
/// trips `clippy::result_large_err`.
#[derive(Debug)]
struct NoPrincipal;

impl IntoResponse for NoPrincipal {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "no principal on a guarded route",
        )
            .into_response()
    }
}

/// Every note on a work.
///
/// **A work with no notes answers an empty document rather than a 204**, which
/// is where this deliberately parts company with `get_progress` beside it.
///
/// Absent and empty are collapsed here rather than in `notes::load`, which
/// keeps them distinct: a reader with no notes and a reader who deleted their
/// last note are in the same state, and nothing downstream benefits from
/// telling them apart.
async fn get_notes(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    // A read of `state.library` — `serve_page`'s reasoning about the async
    // worker applies unchanged (O12).
    let user_id = match reader(&extensions) {
        Ok(id) => id,
        Err(missing) => return missing.into_response(),
    };
    // O60: a work outside the caller's grant is answered as a slug no work
    // holds. Asked here, where the work is looked up. For an account with no
    // grant limits the question is answered without the library
    // (`on_the_grant`).
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.find_blocking(&id).map_err(NoNotes::NotFound)?;
        // Writing notes requires `If-Match`, and a client that had never seen a
        // document would have no validator to send for its first write — so it
        // would need either a second precondition spelling for creation, or an
        // unconditional first write, which is exactly the lost-update hole the
        // header exists to close, left open for the one request a second device
        // is most likely to be racing. Synthesising the document here means the
        // first write is an ordinary conditional write like every later one.
        let notes = crate::notes::load(&state.library, &found, user_id)
            .map_err(NoNotes::Failed)?
            .unwrap_or_else(|| crate::notes::empty(&found.id));
        // The body is the part of the document this account is sent, under the
        // tag `put_notes` checks `If-Match` against (spec §3.8).
        crate::notes::tagged_for(&notes, &view).map_err(NoNotes::Failed)
    })
    .await;

    match loaded {
        Ok(Ok((tag, json))) => {
            if if_none_match_hits(&headers, &tag) {
                return not_modified(&tag, None);
            }
            (
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::ETAG, tag.as_str()),
                ],
                json,
            )
                .into_response()
        }
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("notes read panicked: {e}"),
        )
            .into_response(),
    }
}

/// Replace a work's notes.
///
/// A whole-document `PUT` rather than a route per note, because the document is
/// small, a reader edits it from one place at a time, and `If-Match` over the
/// whole thing is one rule to hold rather than one per note. **The cost is
/// stated rather than hidden:** two devices each adding a note at the same time
/// means the second is refused and has to re-read and re-apply — this does not
/// merge. That is the bargain `patch_alignments` already makes, and the
/// alternative would need this server to decide what two conflicting edits to
/// *one* note mean, a question nobody has had yet.
///
/// `#[allow(clippy::result_large_err)]` for the reason `patch_alignments`
/// carries it: the closure below resolves a `Response`-carrying error and
/// clippy checks each closure as its own item.
#[allow(clippy::result_large_err)]
async fn put_notes(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // Taken as `Bytes` and deserialised here rather than through `Json`, so a
    // malformed body is refused in this server's own words — O5's rule, and
    // the shape `patch_alignments` already follows.
    let incoming: crate::notes::Notes = match serde_json::from_slice(&body) {
        Ok(notes) => notes,
        Err(why) => return bad_request(format!("these are not notes: {why}")),
    };

    let user_id = match reader(&extensions) {
        Ok(id) => id,
        Err(missing) => return missing.into_response(),
    };

    // O60, as `get_notes` asks it: after the body is parsed, so a body that
    // is not notes is refused as that whatever work it names. The view is
    // kept to translate the notes and validate them below.
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };

    // Serialising, hashing and writing all block, and the read-compare-write
    // below holds an exclusive lock across the write — `serve_page`'s reasoning
    // about the async worker applies unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.find_blocking(&id).map_err(NoNotes::NotFound)?;

        // Held across read, compare and write. Two requests that both read the
        // same document and both find their validator current would otherwise
        // both write, and one reader's note would vanish with nothing saying
        // so — see `Work::notes_guard`.
        let _guard = found.notes_guard();

        let current = crate::notes::load(&state.library, &found, user_id)
            .map_err(NoNotes::Failed)?
            .unwrap_or_else(|| crate::notes::empty(&found.id));
        let tag =
            view.etag_naming_numbering(&crate::notes::to_json(&current).map_err(NoNotes::Failed)?);
        match if_match_hits(&headers, &tag) {
            None => {
                return Err(NoNotes::Precondition(
                    StatusCode::PRECONDITION_REQUIRED,
                    "writing notes must carry `If-Match` with the notes' ETag, so two devices \
                     annotating one work cannot silently overwrite each other"
                        .to_string(),
                ));
            }
            Some(false) => {
                return Err(NoNotes::Precondition(
                    StatusCode::PRECONDITION_FAILED,
                    format!(
                        "these notes have changed since you read them; their ETag is now {tag}. \
                         Re-read the notes and write again."
                    ),
                ));
            }
            Some(true) => {}
        }

        // In the stored numbering, with the notes this account is not sent
        // kept (spec §3.8).
        let mut merged = crate::notes::as_stored(incoming, &current, &view)?;

        // Validated against what is stored, and therefore inside the guard
        // rather than ahead of it: an anchor that has not moved since the last
        // write is not re-checked, which is what stops one note left pointing
        // at an edition a re-solve renamed from refusing every write that
        // follows it — including the one that would remove it. See
        // `notes::validate_against`.
        crate::notes::validate_against(
            &merged,
            &found.id,
            &view.visible_editions_in_full(),
            &current,
        )?;
        // In id order, the order `notes::load` reads, so the answer's `ETag`
        // is the one the next write is checked against.
        merged.notes.sort_by(|a, b| a.id.cmp(&b.id));

        crate::notes::store(&state.library, &found, user_id, &merged).map_err(NoNotes::Failed)?;
        crate::notes::tagged_for(&merged, &view).map_err(NoNotes::Failed)
    })
    .await;

    match written {
        Ok(Ok((tag, json))) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ETAG, tag.as_str()),
            ],
            json,
        )
            .into_response(),
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("notes write panicked: {e}"),
        )
            .into_response(),
    }
}

/// Why a truth request could not be answered.
///
/// `NoNotes`'s split, with `Forbidden` in place of `Refused`: a truth document
/// names no slot to check, so the one client mistake besides a missing work
/// or edition is an entry for an edition the account sees only in part
/// (`truth::Unwritable`).
#[derive(Debug)]
enum NoTruth {
    NotFound(String),
    Forbidden(String),
    Precondition(StatusCode, String),
    Failed(String),
}

impl From<crate::truth::Unwritable> for NoTruth {
    fn from(why: crate::truth::Unwritable) -> Self {
        match why {
            crate::truth::Unwritable::Missing(message) => Self::NotFound(message),
            crate::truth::Unwritable::PartlyVisible(message) => Self::Forbidden(message),
        }
    }
}

impl IntoResponse for NoTruth {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => not_found(message),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, message).into_response(),
            Self::Precondition(status, message) => (status, message).into_response(),
            Self::Failed(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
        }
    }
}

/// Every text block a person has drawn on a work.
///
/// **A work nobody has labelled yet answers an empty document rather than a
/// 404 or a 204**, `get_notes`'s reasoning exactly: writing truth requires
/// `If-Match`, and a client that had never seen a document would have no
/// validator to send for its first write. Synthesising the document here
/// means the first write is an ordinary conditional write like every later
/// one.
async fn get_truth(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    // O60, as `get_notes` asks it. A truth document is shared, and a caller
    // whose grant hides the whole work is not told the work exists. The view
    // names the editions it is sent whole, and the numbering its tag names.
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };
    // A read of `state.library` — `serve_page`'s reasoning about the async
    // worker applies unchanged (O12).
    let loaded = tokio::task::spawn_blocking(move || -> Result<(String, String), NoTruth> {
        let found = state.find_blocking(&id).map_err(NoTruth::NotFound)?;
        let doc = crate::truth::load(&state.library, &found)
            .map_err(NoTruth::Failed)?
            .unwrap_or_default();
        Ok(crate::truth::tagged_for(&doc, &view))
    })
    .await;

    match loaded {
        Ok(Ok((tag, json))) => {
            if if_none_match_hits(&headers, &tag) {
                return not_modified(&tag, None);
            }
            (
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::ETAG, tag.as_str()),
                ],
                json,
            )
                .into_response()
        }
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("truth read panicked: {e}"),
        )
            .into_response(),
    }
}

/// Replace a work's truth document.
///
/// A whole-document `PUT`, `put_notes`'s reasoning unchanged: the document is
/// small, a labeller edits it from one place at a time, and `If-Match` over
/// the whole thing is one rule to hold rather than one per page. Two sessions
/// labelling the same work at once is the same bargain `put_notes` and
/// `patch_alignments` already make — the second is refused and has to re-read
/// and re-apply rather than being merged.
///
/// `#[allow(clippy::result_large_err)]` for the reason `put_notes` carries it:
/// the closure below resolves a `Response`-carrying error and clippy checks
/// each closure as its own item.
#[allow(clippy::result_large_err)]
async fn put_truth(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // Taken as `Bytes` and deserialised here rather than through `Json`, so a
    // malformed body is refused in this server's own words — O5's rule, and
    // the shape `put_notes` already follows. `TruthDoc::from_json` already
    // carries that wording (see its own doc comment), so its message is
    // passed straight through rather than re-derived here.
    let text = match std::str::from_utf8(&body) {
        Ok(text) => text,
        Err(why) => return bad_request(format!("truth body is not valid UTF-8: {why}")),
    };
    let incoming = match taiyaku_core::TruthDoc::from_json(text) {
        Ok(doc) => doc,
        Err(why) => return bad_request(why),
    };

    // O60, as `put_notes` asks it: after the body is parsed. The view is kept:
    // the write keeps what the account is not sent.
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };

    // Serialising, hashing and writing all block, and the read-compare-write
    // below holds an exclusive lock across the write — `serve_page`'s reasoning
    // about the async worker applies unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.find_blocking(&id).map_err(NoTruth::NotFound)?;

        // Held across read, compare and write, on the same mutex
        // `put_notes` holds for the same reason — `Work` has one
        // read-modify-write guard per work, not one per document kind, and
        // a truth write racing a notes write on the same work is exactly the
        // case it exists to exclude. Two requests that both read the same
        // document and both find their validator current would otherwise
        // both write, and one labeller's session would vanish with nothing
        // saying so.
        let _guard = found.notes_guard();

        let current = crate::truth::load(&state.library, &found)
            .map_err(NoTruth::Failed)?
            .unwrap_or_default();
        let tag = view.etag_naming_numbering(&crate::truth::to_json(&current));
        match if_match_hits(&headers, &tag) {
            None => {
                return Err(NoTruth::Precondition(
                    StatusCode::PRECONDITION_REQUIRED,
                    "writing truth must carry `If-Match` with the truth document's ETag, so two \
                     labelling sessions on one work cannot silently overwrite each other"
                        .to_string(),
                ));
            }
            Some(false) => {
                return Err(NoTruth::Precondition(
                    StatusCode::PRECONDITION_FAILED,
                    format!(
                        "this truth document has changed since you read it; its ETag is now \
                         {tag}. Re-read it and write again."
                    ),
                ));
            }
            Some(true) => {}
        }

        // Every entry the account is not sent whole kept, and one it changed
        // refused (spec §3.8, O191).
        let merged = crate::truth::as_stored(incoming, &current, &view, &found.id)?;
        crate::truth::store(&state.library, &found, &merged).map_err(NoTruth::Failed)?;
        Ok(crate::truth::tagged_for(&merged, &view))
    })
    .await;

    match written {
        Ok(Ok((tag, json))) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ETAG, tag.as_str()),
            ],
            json,
        )
            .into_response(),
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("truth write panicked: {e}"),
        )
            .into_response(),
    }
}

async fn get_progress(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
) -> Response {
    let user_id = match reader(&extensions) {
        Ok(id) => id,
        Err(missing) => return missing.into_response(),
    };

    // O60, as `get_notes` asks it. The view is kept: it names the editions
    // this caller may be told of, and numbers a narrowed one's slots.
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };

    // O12: a read of `state.library`. `serve_page`'s reasoning above applies to
    // it unchanged.
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.find_blocking(&id).map_err(NoProgress::NotFound)?;
        crate::progress::load(&state.library, &found, user_id)
            .map(|mut positions| {
                // A position on an edition outside the grant is left out, as
                // `get_sidecar` leaves the edition out, and so is one past a
                // narrowed edition's end; one on a page it hides is sent as
                // the nearest page before it (spec §3.8). With none left the
                // answer below is 204, nothing recorded.
                positions.retain_mut(|position| {
                    match view.slot_from_full(&position.edition, position.slot) {
                        Some(FromFull::Visible(slot) | FromFull::Hidden { fallback: slot }) => {
                            position.slot = slot;
                            true
                        }
                        Some(FromFull::Outside) | None => false,
                    }
                });
                positions
            })
            .map_err(NoProgress::Failed)
    })
    .await;

    match loaded {
        // Nothing recorded is not an error: a work never opened has no
        // position, and 404 would say the work does not exist. An empty set
        // is that same fact, and it stays a status code rather than becoming
        // a document a client has to unpack.
        Ok(Ok(positions)) if positions.is_empty() => StatusCode::NO_CONTENT.into_response(),
        // The positions the closure above let through, newest first:
        // `crate::progress::load`'s order, kept.
        Ok(Ok(positions)) => {
            axum::Json(serde_json::json!({ "positions": positions })).into_response()
        }
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("progress read panicked: {e}"),
        )
            .into_response(),
    }
}

async fn put_progress(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    axum::Json(progress): axum::Json<crate::Progress>,
) -> Response {
    let user_id = match reader(&extensions) {
        Ok(id) => id,
        Err(missing) => return missing.into_response(),
    };

    // O60, as `get_notes` asks it. The view is kept for the edition check
    // below.
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };

    // Validation moves in here with the write rather than staying outside it.
    // It reads the in-memory sidecar and blocks on nothing, but splitting the
    // handler across the boundary would mean looking the work up twice — once
    // to validate and once inside the closure, which cannot borrow across it.
    let stored = tokio::task::spawn_blocking(move || {
        let found = state.find_blocking(&id).map_err(NoProgress::NotFound)?;
        // The document this caller is shown (O60), so an edition outside the
        // grant is refused just below as one the work does not have, and a
        // narrowed one's slots are counted as this caller numbers them.
        let sidecar = view.sidecar();
        let edition = sidecar.editions.get(&progress.edition).ok_or_else(|| {
            NoProgress::NotFound(format!("no edition `{}` in work `{id}`", progress.edition))
        })?;
        // Counted in slots rather than pages, because that is what a position
        // is: `progress.rs` records slots so a spread cannot lose which half
        // was reached, and an edition that joins spreads has more slots than
        // pages. Checking against `total_pages()` would refuse the last slot
        // of every such edition — One Piece v092 is spreads throughout.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "edition.slots(reading).len() is at most twice the edition's page count \
                      (crate::expand's flat_map in taiyaku-core/src/slot.rs emits at most two \
                      Slots per page), and a page count is bounded by what a book can \
                      physically contain, far below u32::MAX. Kept as an #[expect] rather \
                      than a try_from().expect() helper: this runs inside a progress-write \
                      request handler against a sidecar, which a re-solve or a hand edit can \
                      change, not an in-process invariant — adding a panic here would trade a \
                      truncation that cannot happen for a request failure that, however \
                      unlikely, could."
        )]
        let slots = edition.slots(sidecar.work.reading).len() as u32;
        let slot = view
            .slot_to_full(&progress.edition, progress.slot)
            .filter(|_| progress.slot < slots)
            .ok_or_else(|| {
                NoProgress::NotFound(format!(
                    "`{}` has {slots} slots in work `{id}`; there is no slot {}",
                    progress.edition, progress.slot
                ))
            })?;
        let progress = crate::Progress { slot, ..progress };
        crate::progress::store(&state.library, &found, user_id, &progress)
            .map_err(NoProgress::Failed)
    })
    .await;

    match stored {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("progress write panicked: {e}"),
        )
            .into_response(),
    }
}

/// One hand correction, as a client asks for it.
///
/// Slots are the format's own span notation (`7`, `4R`, `45R`) rather than
/// bare indices, so the wire, the document and the contact sheet all name a
/// slot the same way — and a client that is off by a half says so visibly
/// instead of resolving to a neighbouring index.
///
/// **Nothing destructive is spelled by an absence here**, and that shapes two
/// of the attributes below.
///
/// `b` is nullable but **required**: `"b": null` unpairs `a`, and omitting the
/// field is a `400` rather than an unpairing. `note` is genuinely optional,
/// because its absence is inert — no note, nothing lost. That is the whole
/// distinction: a field may default when the default changes nothing, and must
/// not when the default edits the document.
///
/// Two guards, and they deliberately overlap. Requiring `b` closes the
/// omission: a client assembling this body from a partial object would
/// otherwise unpair a page and be told `200`. `deny_unknown_fields` closes a
/// misspelled or version-skewed field name.
///
/// On `b` specifically the two say the same thing — a typo'd `"bb"` leaves
/// `b` absent, so requiring it refuses that request even with the attribute
/// removed, just with the other message. The attribute still earns its place
/// on the fields where nothing else would notice: a mistyped `note` is not
/// destructive, but it would be silently discarded, and a reader who wrote
/// down why they made a correction should not lose it to a typo in a key.
///
/// The cost is one key on the wire. This is not the OPDS surface — that is
/// `/opds/v1.2/…`, Atom, and read-only, and it is what third-party readers
/// use; this is taiyaku's own JSON API, and its only client is `web/`,
/// whose `CorrectionRequest` types `b` as required-and-nullable.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Correction {
    from: String,
    to: String,
    a: String,
    /// `null` unpairs `a`; omitting the field is refused. See the type's own
    /// doc for why absence must not spell an unpairing.
    ///
    /// **`Option<String>` alone would not do this.** serde treats an `Option`
    /// field as optional whether or not `#[serde(default)]` is written, so a
    /// missing `b` would deserialise to `None` — indistinguishable from an
    /// explicit `null`, which is the exact confusion this field must not have.
    ///
    /// `Pairing` is what the *present* value means; the outer `Option` here is
    /// still "present or not", filled by `#[serde(default)]` only when the key
    /// is absent. `present` still has to force the wrapper past a plain
    /// `Option`'s null-absorbing `Deserialize` — that absorption is exactly
    /// what would make `"b": null` and an absent `b` indistinguishable again,
    /// one level up: `Option<Pairing>` without `deserialize_with` would answer
    /// `an_explicit_null_unpairs_the_slot`'s `"b": null` with the *absent-b*
    /// refusal, because `Option<Pairing>::deserialize` consumes the `null` and
    /// reports `None` before `Pairing::deserialize` ever runs.
    #[serde(default, deserialize_with = "present")]
    b: Option<Pairing>,
    #[serde(default)]
    note: Option<String>,
}

/// What a present `b` asks for: `null` versus a slot. `Correction::b`'s outer
/// `Option` is presence, filled by `#[serde(default)]`; this is the other axis.
#[derive(Debug)]
enum Pairing {
    /// `"b": null` — unpair `a`.
    Unpair,
    /// `"b": "4R"` — pair `a` with this slot.
    To(String),
}

impl<'de> serde::Deserialize<'de> for Pairing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match Option::<String>::deserialize(deserializer)? {
            None => Pairing::Unpair,
            Some(slot) => Pairing::To(slot),
        })
    }
}

/// Deserialize a value that may be `null` into `Some`, so a *missing* key stays
/// distinguishable from one written as `null`.
///
/// Only meaningful together with `#[serde(default)]`, which supplies the `None`
/// for the missing case; this function is never called then.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// One slot from a span, refusing a range.
///
/// `45R-46L` is legal notation and names two slots; a correction addresses
/// one, and quietly taking the start of a range would correct something other
/// than what was asked for.
///
/// A bare `Response` in the `Err` rather than a named type like
/// `NotAPageIndex` above: this function has exactly one caller, so the extra
/// type would buy nothing a comment does not already say, and the allow below
/// is the same `clippy::result_large_err` every other `Response`-carrying
/// `Result` in this file works around.
#[allow(clippy::result_large_err)]
fn one_slot(text: &str, side: &str) -> Result<taiyaku_core::Slot, Response> {
    let span = taiyaku_core::SlotSpan::parse(text)
        .map_err(|why| bad_request(format!("`{side}`: {why}")))?;
    if span.start != span.end {
        return Err(bad_request(format!(
            "`{side}` is `{text}`, which names a range; a correction addresses one slot"
        )));
    }
    Ok(span.start)
}

/// Write a hand correction into the work's sidecar.
///
/// The client sends the assertion, never a computed run list: the split is
/// applied by `Sidecar::correct` against the document this server holds, so
/// there is one implementation of the arithmetic and a stale client cannot
/// replace an edge wholesale. `If-Match` is what makes "the document this
/// server holds" and "the document the client was looking at" the same thing.
///
/// **Applies the grant itself.** Resolving its target by slug alone would let
/// a restricted account both read the sidecar this handler returns and write
/// shared `alignment`/`run` rows for a work outside every root it held — worse
/// than a read leak, because an alignment is shared library truth rather than
/// per-account state.
///
/// **Filters its response exactly as `get_sidecar` does, and for the same
/// reason.** A reader granted only `en` on a work also holding a withheld
/// `raw` passes the work-level check — the work keeps a visible edition — and
/// would get back a 200 carrying the *whole* corrected sidecar, `raw`
/// included, if the response were not filtered.
///
/// **Refuses a correction that itself names a withheld edition, on either
/// side.** Filtering the response shapes only what comes back; it would leave
/// the write open — a reader granted `en` alone could still submit
/// `{"from":"en","to":"raw",…}` and have it accepted, writing shared
/// `alignment`/`run` rows for `raw` that every reader who *can* see `raw`
/// is then served from. That is worse than a response leak: a read leak costs
/// the caller who triggered it a fact they should not have; this write would
/// cost every other reader a fact they did not ask this caller to assert.
/// Each side is asked whether any member of it is visible, the answer
/// [`crate::view::of_edition`]'s flags give the page and download routes,
/// before the body is otherwise parsed, in
/// [`visible_members_for_a_correction`]. A side the caller sees only in part
/// is refused with 403 once the view is built (spec §3.8).
///
/// **What is stored is never filtered — only what is returned.** `flags`
/// are read once, before the write, and shape only the response and both
/// tags; the file and the `alignment`/`run` rows below always hold the
/// complete, unfiltered document, because a correction is shared library
/// truth and a caller-specific projection of it must never reach disk. The
/// per-side checks above are what keep an invisible edition from reaching
/// that write at all, rather than a filter on it afterward.
///
/// `#[allow(clippy::result_large_err)]` because everything under this
/// handler — `one_slot`'s calls, the `spawn_blocking` closure and the
/// `Work::update` closure nested inside it — resolves a `Response` as its
/// error type, and clippy checks each closure as its own item
/// (`works/tests.rs`'s `a_replaced_sidecar_is_what_the_next_reader_sees`
/// says the same thing at its own call site).
#[allow(clippy::result_large_err)]
async fn patch_alignments(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    request_extensions: axum::http::Extensions,
    body: axum::body::Bytes,
) -> Response {
    // Parsed before the grant is asked and refused after it, so a hidden work
    // is still refused before a malformed body: the body lends the grant check
    // the two editions it names, when it names two.
    let parsed: Result<Correction, _> = serde_json::from_slice(&body);
    let sides = parsed
        .as_ref()
        .ok()
        .map(|correction| (correction.from.clone(), correction.to.clone()));
    // **The write-authorization half of Finding 1, not merely its response
    // leak.** `flags` only shape what this handler sends back; on their own
    // they never ask whether the correction's own `from`/`to` are inside the
    // grant. A reader granted only `en` on a work that also holds a withheld
    // `raw` could name `raw` on either side and have it written into the
    // shared `alignment`/`run` rows every other reader of this work is served
    // from — worse than a read leak, because the write is not scoped to the
    // caller who made it. Each side is asked whether any member of it is
    // visible, in the flags `view::of_edition` reads for the page and
    // download routes.
    let flags =
        match visible_members_for_a_correction(&state, &request_extensions, &id, sides).await {
            Ok(flags) => flags,
            Err(refusal) => return refusal,
        };
    let correction: Correction = match parsed {
        Ok(correction) => correction,
        Err(why) => return bad_request(format!("this is not a correction: {why}")),
    };
    let a = match one_slot(&correction.a, "a") {
        Ok(slot) => slot,
        Err(refusal) => return refusal,
    };
    // Absence is refused here rather than in the derive, so the message names
    // the field and says what the two spellings mean — serde's own "missing
    // field `b`" would not say that an unpairing has to be asked for
    // explicitly.
    let Some(pairing) = correction.b else {
        return bad_request(
            "a correction needs `b`: a slot of the target edition, or `null` to say this slot \
             pairs with nothing. Leaving it out is refused, because an unpairing is an edit and \
             has to be asked for."
                .to_string(),
        );
    };
    let b = match pairing {
        Pairing::Unpair => None,
        Pairing::To(text) => match one_slot(&text, "b") {
            Ok(slot) => Some(slot),
            Err(refusal) => return refusal,
        },
    };

    // Serialising, hashing and writing all block, and the read-modify-write
    // below holds an exclusive lock across the file write — `serve_page`'s
    // reasoning about the async worker applies to it unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.find_blocking(&id).map_err(not_found)?;
        // **The library's connection before the work's document, never the
        // other way round.** `list_works` holds this mutex across a
        // `work.sidecar()` read, so a thread that took the sidecar's write
        // lock first and reached for the mutex second would deadlock against
        // it — and the `spawn_blocking` pool is where both of them run. It is
        // held across the file write below, which is the cost of that order:
        // one correction — a person dragging a filmstrip tile — briefly
        // blocks the other library readers.
        let conn = state.library.conn();
        // The view of a document, as this caller is sent it: `If-Match` names
        // the tag `get_sidecar` sent it, which is over its view.
        let seen = |sidecar: &Arc<taiyaku_core::Sidecar>| {
            crate::view::View::build(Arc::clone(sidecar), flags.as_ref())
                .ok_or_else(|| not_found(format!("no work `{id}`")))
        };
        found.update(|current| {
            let view = seen(current)?;
            // A narrowed edition's runs are in a numbering this caller is not
            // sent, which is why its document leaves them out (spec §3.8). A
            // side the store shows but the view hides, its flags not numbering
            // these members, fails closed as a hidden side does.
            for side in [&correction.from, &correction.to] {
                if view
                    .visible()
                    .is_some_and(|keys| !keys.contains(side.as_str()))
                {
                    return Err(not_found(format!("no work `{id}`")));
                }
                if view.narrowed(side).is_some() {
                    return Err((
                        StatusCode::FORBIDDEN,
                        format!(
                            "`{side}` of `{id}` is only partly visible to this account, so its \
                             alignments cannot be corrected here"
                        ),
                    )
                        .into_response());
                }
            }
            let tag = sidecar_etag(view.sidecar());
            match if_match_hits(&headers, &tag) {
                None => {
                    return Err((
                        StatusCode::PRECONDITION_REQUIRED,
                        "a correction must carry `If-Match` with the sidecar's ETag, so two \
                         devices correcting one work cannot silently overwrite each other"
                            .to_string(),
                    )
                        .into_response());
                }
                Some(false) => {
                    return Err((
                        StatusCode::PRECONDITION_FAILED,
                        format!(
                            "this work has changed since you read it; its ETag is now {tag}. \
                             Re-read the sidecar and correct again."
                        ),
                    )
                        .into_response());
                }
                Some(true) => {}
            }

            let mut next = (**current).clone();
            let what = next
                .correct(
                    &correction.from,
                    &correction.to,
                    a,
                    b,
                    correction.note.as_deref(),
                )
                .map_err(|why| {
                    use taiyaku_core::NotCorrected::{
                        NoPath, NoSuchEdition, NoSuchSlot, SameEdition, Unreadable,
                    };
                    let status = match why {
                        NoSuchEdition(_) | NoSuchSlot(_) | NoPath(_) => StatusCode::NOT_FOUND,
                        SameEdition(_) => StatusCode::BAD_REQUEST,
                        Unreadable(_) => StatusCode::CONFLICT,
                    };
                    (status, why.to_string()).into_response()
                })?;
            // The answer is the document `get_sidecar` would send this caller
            // next, so the next correction's `If-Match` holds. It is built
            // before the write so that nothing fallible follows the write: a
            // refusal here leaves the file, the rows and memory as they were.
            let next = std::sync::Arc::new(next);
            let sent = seen(&next)?;

            // The file, then the rows, then memory. A failed write leaves the
            // served document exactly as it was, so a client that gets a 500
            // has lost its correction and nothing else — where swapping memory
            // first would leave this server serving an alignment that is on no
            // disk anywhere.
            //
            // The rows are what every *other* request is answered from
            // (`taiyaku serve` projects each `Work` out of them at start, and,
            // with no paths, again whenever the library's structure moves), so
            // writing only the file would leave the two disagreeing until the
            // next start re-imported it — O43. Between them, the file is
            // first: it is the copy a person can read, diff and back up, and
            // if the row write fails after it the correction is recoverable,
            // because the next start imports that file. The reverse ordering
            // would put the only copy of the correction in a database the
            // reader was never told to look in.
            // The full document — what gets written to disk and to the
            // `alignment`/`run` rows below. Never the filtered projection:
            // see this function's own doc comment on why storage is never
            // filtered.
            let json = next.to_json();
            if what != taiyaku_core::Corrected::AlreadyTrue {
                // **The file, then the rows — unless there is no file.**
                //
                // The ordering is argued above and is unchanged for a work
                // that has a document. A scanned work has none:
                // `import::scanned` leaves `sidecar_path` NULL by design,
                // since one `taiyaku.json` per directory cannot describe
                // fourteen volumes sitting beside each other. So there is no
                // file to be first, the rows are written alone, and spec C1
                // §5.3 states the cost plainly — for such a work the
                // correction's only copy is in the database until somebody
                // runs `library export`. That is the outcome the comment
                // above declines to accept for a document-backed work, and it
                // remains the right refusal there, because there the file
                // exists.
                //
                // Minting a `taiyaku.json` here instead was rejected: it would
                // make the server a writer into a tree spec B never opens for
                // writing, and hand `import`'s `sidecar_path` reconciliation a
                // second writer to race.
                //
                // Without this branch the write fails with "staging a
                // temporary beside it: No such file or directory" and the
                // reader loses the correction to a 500 —
                // `correcting_a_work_with_no_document_writes_rows_and_no_file`
                // reproduces exactly that.
                if found.has_document() {
                    // **A newer taiyaku's sidecar is refused, not written
                    // down** (O264): `next` is this build's projection, so the
                    // write would drop every field it has no name for and
                    // stamp it version 1. Asked before the file, and so before
                    // the rows, in the words a stamp over the file uses, less
                    // the file's path: any account granted the work calls this.
                    if let Some(declared) =
                        taiyaku_library::migrate::declared_version(&found.path, "taiyaku")
                    {
                        // Past `u32` is newer than any build, so it is named
                        // at the largest.
                        let declared = u32::try_from(declared).unwrap_or(u32::MAX);
                        taiyaku_core::refuse_newer("taiyaku", declared, taiyaku_core::VERSION)
                            .map_err(|why| (StatusCode::CONFLICT, why).into_response())?;
                    }
                    taiyaku_read::atomic::write(&found.path, json.as_bytes(), "json")
                        .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())?;
                }
                crate::alignments::store(&conn, &found, &next)
                    .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())?;
            }
            // `json` above stays the full document that was stored; the
            // answer and its tag are this caller's view of it.
            let response_json = if flags.is_some() {
                sent.sidecar().to_json()
            } else {
                json
            };
            let tag = sidecar_etag(sent.sidecar());
            Ok((next, (response_json, tag)))
        })
    })
    .await;

    match written {
        Ok(Ok((json, tag))) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ETAG, tag.as_str()),
            ],
            json,
        )
            .into_response(),
        Ok(Err(refusal)) => refusal,
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("correction panicked: {e}"),
        )
            .into_response(),
    }
}

/// **Every path this server answers, and the handler for each.**
///
/// The router's own source of truth, and the reason
/// `auth::tests::every_route_refuses_an_anonymous_request` is a test rather
/// than a checklist. [`build_router`] folds this list, so a path that is not
/// here does not exist, and the enumeration test walks the same list. A route
/// added here therefore arrives guarded *and* covered on the commit that adds
/// it — which is exactly the failure a reviewer's checklist has: it goes stale
/// the first time someone adds a route and does not think about auth.
///
/// A `Vec` rather than a `const` array because `MethodRouter` is not
/// const-constructible. The cost is one allocation per `build_router` call, of
/// which there is one per process and one per test.
pub(crate) fn guarded_routes() -> Vec<(&'static str, axum::routing::MethodRouter<Arc<ServeState>>)>
{
    let mut routes = vec![
        ("/api/works", get(list_works)),
        ("/api/works/{work}", get(get_sidecar)),
        ("/api/works/{work}/{edition}/pages/{index}", get(get_page)),
        (
            "/api/works/{work}/{edition}/pages/{index}/thumb",
            get(get_thumb),
        ),
        (
            "/api/works/{work}/{edition}/download",
            get(crate::download::edition),
        ),
        (
            "/api/works/{work}/{edition}/members/{ordinal}/download",
            get(crate::download::member),
        ),
        (
            "/api/works/{work}/progress",
            get(get_progress).put(put_progress),
        ),
        ("/api/works/{work}/notes", get(get_notes).put(put_notes)),
        ("/api/works/{work}/truth", get(get_truth).put(put_truth)),
        (
            "/api/works/{work}/alignments",
            axum::routing::patch(patch_alignments),
        ),
    ];
    routes.push(("/api/me", get(crate::session::whoami)));
    routes.push(("/api/about", get(crate::about::about)));
    routes.push((
        "/api/keys",
        get(crate::session::list_keys).post(crate::session::create_key),
    ));
    routes.push((
        "/api/keys/{id}",
        axum::routing::delete(crate::session::revoke_key),
    ));
    routes.push(("/api/sessions", get(crate::session::list_sessions)));
    routes.push((
        "/api/sessions/{id}",
        axum::routing::delete(crate::session::revoke_session),
    ));
    routes.push((
        "/api/password",
        axum::routing::put(crate::session::change_password),
    ));
    // **Spec C1 §5, and in `guarded_routes` rather than `open_routes`.**
    // `route_layer` wraps only what is registered before it, so a browse route
    // in the other list would ship unauthenticated —
    // `every_route_refuses_an_anonymous_request` checks its paths against a
    // typed list and is what catches that.
    routes.extend([
        ("/api/library/home", get(crate::library_api::home)),
        // Returns `{ series, unfiled }` — spec C1 §5.4. A hand-imported work
        // has no series, and a grid that listed only series would never show
        // one.
        ("/api/library/series", get(crate::library_api::series)),
        (
            "/api/library/series/{id}",
            get(crate::library_api::one_series),
        ),
        (
            "/api/library/series/{id}/cover",
            get(crate::library_api::cover),
        ),
        ("/api/library/works/{slug}", get(crate::library_api::work)),
        ("/api/library/search", get(crate::library_api::search)),
        ("/api/library/status", get(crate::library_api::status)),
        // **Spec C3 §7, and in `guarded_routes` rather than
        // `open_routes`** — the identical argument the C1 comment above
        // makes, and one these routes cannot afford to get wrong: they
        // write.
        //
        // Which `extend` call they arrive in is not what carries that.
        // `guarded_routes` returns one Vec and `build_router` applies
        // `route_layer` to the whole of it, so the `opds::routes()` extended
        // on the next line is as guarded as these are. What holds the line is
        // three tests, none of which can be satisfied by a route that is
        // merely nearby: `auth::tests::every_route_refuses_an_anonymous
        // _request` checks this function's paths against a typed list and
        // drives each one for a 401, `auth::tests::every_route_declares_its
        // _grant_posture` asserts its posture table's route set against this
        // function's exactly, and `curation::tests::every_curation_route
        // _refuses_a_reader` drives all seven of these methods for a 403.
        ("/api/library/proposals", get(crate::curation::list)),
        ("/api/library/proposals/{id}", get(crate::curation::detail)),
        (
            "/api/library/proposals/{id}/resolution",
            axum::routing::post(crate::curation::resolution),
        ),
        (
            "/api/library/proposals/{id}/accept",
            axum::routing::post(crate::curation::accept).delete(crate::curation::unaccept),
        ),
        (
            "/api/library/proposals/{id}/dismiss",
            axum::routing::post(crate::curation::dismiss).delete(crate::curation::reopen),
        ),
        // **Spec C3 §5's metadata-editing routes, closing O44.** Merged onto
        // the same two paths `library_api::work`/`one_series` already
        // registered above (a `GET` and a `PATCH` on one path merge into one
        // `MethodRouter`, they do not conflict — see `axum::Router::route`'s
        // own doc), which is why `/api/library/works/{slug}` and
        // `/api/library/series/{id}` appear a second time here rather than
        // gaining a second row where their `GET` already sits: this is the
        // curation block's own reasoning restated — a route that write
        // cannot afford to land in `open_routes`, and its posture belongs
        // beside the other write routes, not beside the read one sharing its
        // path. `metadata::tests::every_metadata_route_refuses_a_reader`
        // drives all four for a 403.
        (
            "/api/library/works/{slug}",
            axum::routing::patch(crate::metadata::patch_work),
        ),
        (
            "/api/library/series/{id}",
            axum::routing::patch(crate::metadata::patch_series),
        ),
        (
            "/api/library/editions/{id}",
            axum::routing::patch(crate::metadata::patch_edition),
        ),
        (
            "/api/library/members/{id}",
            axum::routing::patch(crate::metadata::patch_member),
        ),
        // **Spec C3 §6's write-back, and the only route in this product that
        // modifies a file the user owns.** Guarded four ways —
        // authenticated, admin, `can_stamp`, then the slug — and the third is
        // a capability an administrator does NOT inherit, which is a
        // documented exception to O86. See `auth::Principal::can_stamp`, and
        // `stamp.rs`'s own module comment for why the capability is read
        // before the slug is resolved rather than after.
        (
            "/api/library/works/{slug}/stamp-comicinfo",
            axum::routing::get(crate::stamp::plan).post(crate::stamp::perform),
        ),
        // **Spec C4 §6's rule-editor routes.**
        // Admin-gated the same way the curation and metadata blocks above
        // are, and for the identical reason stated on each of theirs: a
        // route that writes cannot afford to land in `open_routes`.
        // `config_api::tests::every_config_api_route_refuses_a_reader`
        // drives all seven for a 403.
        (
            "/api/library/rules",
            get(crate::config_api::list_rules)
                .post(crate::config_api::create_rule)
                // Spec C4 §4.2's save route. A method on this same path, so
                // nothing else in this file's route *lists* moves — and
                // `config_api::tests::every_config_api_route_refuses_a_reader`
                // has a line for it, because a per-path list cannot see a
                // method arrive.
                .put(crate::config_api::save_rules),
        ),
        (
            "/api/library/rules/{id}",
            axum::routing::patch(crate::config_api::patch_rule)
                .delete(crate::config_api::delete_rule),
        ),
        (
            "/api/library/rules/preview",
            axum::routing::post(crate::config_api::preview_rules),
        ),
        (
            "/api/library/rules/import",
            axum::routing::post(crate::config_api::import_rules),
        ),
        // **Spec C4 §2's roots routes.** Admin-gated the same way the rules
        // block above is, and for the identical reason stated on each of
        // theirs: a route that edits or destroys a scan root cannot afford to
        // land in `open_routes`. `config_api::tests::every_roots_route_refuses
        // _a_reader` is a hand-typed list, not an enumeration, and drives
        // each of these for a 403.
        ("/api/library/roots", get(crate::config_api::list_roots)),
        (
            "/api/library/roots/{id}",
            axum::routing::patch(crate::config_api::patch_root)
                .delete(crate::config_api::delete_root),
        ),
        (
            "/api/library/roots/{id}/move",
            axum::routing::post(crate::libraries_api::move_dir),
        ),
        // **Spec C4 §3's scan-control route.** Admin-gated like every other
        // route in this block, and for the identical reason: the trigger that
        // lets the server start a scan is exactly as destructive a capability
        // as removing a root above. `POST` starts a scan
        // — refusing a second one already running against the same root,
        // 409, rather than queuing it — and `GET` reads the registry
        // `ServeState.scans` holds for it: live progress while the spawned
        // task is still walking, the finished report once it is done.
        (
            "/api/library/roots/{id}/scan",
            get(crate::config_api::scan_status).post(crate::config_api::start_scan),
        ),
        // The libraries an administrator makes, renames and removes on
        // Configuration → Libraries. Admin-gated inside each handler, like
        // the roots routes above, whose directory rows these cards hold.
        (
            "/api/library/libraries",
            get(crate::libraries_api::list_libraries).post(crate::libraries_api::create_library),
        ),
        (
            "/api/library/libraries/{id}",
            axum::routing::patch(crate::libraries_api::patch_library)
                .delete(crate::libraries_api::delete_library),
        ),
        (
            "/api/library/libraries/{id}/dirs",
            axum::routing::post(crate::libraries_api::add_dir),
        ),
        (
            "/api/library/libraries/{id}/dirs/{dir}",
            axum::routing::delete(crate::libraries_api::unlink_dir),
        ),
        (
            "/api/library/libraries/{id}/scan",
            axum::routing::get(crate::libraries_api::library_scan_status)
                .post(crate::libraries_api::start_library_scan),
        ),
        (
            "/api/library/split-editions",
            get(crate::libraries_api::list_split_editions),
        ),
        // The directory browser behind "add a directory": directories beneath the
        // prefixes the server was started with, and nowhere else.
        ("/api/library/dirs", get(crate::dirs_api::list_dirs)),
        // **Spec C4 §4's user-administration routes.** Admin-gated for the
        // reason every route in this block is, with one of its own: these
        // routes write `role`, `can_download`, `can_stream` and
        // `every_library`, and add `user_library` rows (removing a library
        // only drops its rows), so a route here that landed in `open_routes`
        // would let anybody grant themselves anything. Outside this block,
        // `/api/claim` also writes `role`, and only to promote the seeded
        // account while the server is unclaimed (`account::promote`).
        // `users_api::tests::every_users_route_refuses_a_reader` drives them
        // all for a 403.
        (
            "/api/library/users",
            get(crate::users_api::list_users).post(crate::users_api::create_user),
        ),
        (
            "/api/library/users/{id}",
            axum::routing::patch(crate::users_api::patch_user)
                .delete(crate::users_api::delete_user),
        ),
        (
            "/api/library/users/{id}/sessions/{sid}",
            axum::routing::delete(crate::users_api::revoke_session),
        ),
        (
            "/api/library/users/{id}/keys/{kid}",
            axum::routing::delete(crate::users_api::revoke_key),
        ),
        // **Spec C4 §5's backup routes.** Admin-gated with a reason of their
        // own on top of the block's: a backup is the whole database, password
        // hashes included, so the download route hands an administrator every
        // account's PHC string. There is no restore route — C4 §5 puts restore
        // out of band on purpose.
        (
            "/api/library/backups",
            get(crate::backup_api::list_backups).post(crate::backup_api::create_backup),
        ),
        (
            "/api/library/backups/{name}",
            get(crate::backup_api::download_backup),
        ),
    ]);
    routes.extend(crate::opds::routes());
    routes
}

/// Why a guarded route may or may not read [`crate::auth::Principal::access`],
/// and how that claim can be told apart from a hope.
///
/// **Beside [`guarded_routes`] rather than in a test file**, for the reason
/// [`open_routes`] sits here too: the lists are read together, and
/// `auth::tests::every_route_declares_its_grant_posture` asserts this table's
/// route set against `guarded_routes`'s exactly — so a route added to one
/// without the other fails the build on the commit that adds it.
/// [`guarded_routes`]'s own doc comment makes this argument for authentication:
/// "exactly the failure a reviewer's checklist has: it goes stale the first
/// time someone adds a route and does not think about auth." Authorization
/// needs the same test: a route added without thinking about the grant would
/// drift out of it unnoticed.
///
/// `#[cfg(test)]`, like `test_support` itself: this table has no production
/// caller — its whole job is being checked against `guarded_routes` by
/// `auth::tests::every_route_declares_its_grant_posture` — and leaving it
/// compiled into the production binary would make it dead code there,
/// failing this crate's `-D warnings` clippy gate on the plain `--lib` target.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum GrantPosture {
    /// A lookup, not a list: a work or edition the grant hides must answer
    /// 404 — `browse::work`'s or `view::flags_or_refuse`'s own contract,
    /// answering a hidden work exactly as it answers one that does not
    /// exist.
    Refuses,
    /// A list: the route answers 200, and material outside the grant must
    /// simply never be named in the body.
    Filters,
    /// **Both of the above, for a route with two distinct grant-facing
    /// jobs.** `patch_alignments` refuses a request that itself *names* a
    /// withheld edition (`Refuses`, on the write) and separately filters the
    /// response body of a request that does not (`Filters`, on the read) — two
    /// different questions asked of two different parts of one request, neither
    /// of which subsumes the other. A second table (refuse-postures crossed
    /// with filter-postures) was rejected: most routes need only one axis, so a
    /// second table would carry an empty column for nearly every entry. A
    /// combined variant costs one arm in each match already written for
    /// `Refuses` and `Filters` and states the truth in one place.
    RefusesAndFilters,
    /// The grant cannot apply — `reason` says why, stated per entry rather
    /// than inherited, the same rule [`open_routes`] holds its own list to.
    GrantIndependent { reason: &'static str },
}

/// The table [`GrantPosture`]'s own doc comment describes: every entry in
/// [`guarded_routes`], in the same order, classified by how it must answer a
/// caller whose grant does not cover the material a request names.
#[cfg(test)]
pub(crate) fn grant_postures() -> Vec<(&'static str, GrantPosture)> {
    use GrantPosture::{Filters, GrantIndependent, Refuses, RefusesAndFilters};
    vec![
        ("/api/works", Filters),
        // `get_sidecar` refuses only a work with *no* visible edition, and
        // filters `editions`/`alignments`/`hub` down to the visible set for one
        // that has some — the same `Filters` shape `/opds/v1.2/works/{work}`
        // declares below, for the identical reason stated there.
        ("/api/works/{work}", Filters),
        ("/api/works/{work}/{edition}/pages/{index}", Refuses),
        ("/api/works/{work}/{edition}/pages/{index}/thumb", Refuses),
        ("/api/works/{work}/{edition}/download", Refuses),
        (
            "/api/works/{work}/{edition}/members/{ordinal}/download",
            Refuses,
        ),
        // A work outside the grant is refused as a slug no work holds, and
        // the read of a work the caller sees part of leaves out a position on
        // an edition outside the grant.
        ("/api/works/{work}/progress", RefusesAndFilters),
        // A work outside the grant is refused, and the read of a work the
        // caller sees part of leaves out a note on an edition outside the
        // grant; a write keeps it (spec §3.8).
        ("/api/works/{work}/notes", RefusesAndFilters),
        // `get_truth` and `put_truth` refuse a work outside the grant. For a
        // work the caller sees part of, the read leaves out the entry of an
        // edition outside the grant, and a write keeps it (spec §3.8, O191).
        ("/api/works/{work}/truth", RefusesAndFilters),
        // `patch_alignments` filters its response to the visible editions, as
        // `get_sidecar` does (`Filters`), and refuses outright (404) a
        // correction that itself names an edition outside the grant, before
        // `Sidecar::correct` or the write beneath it runs
        // (`visible_members_for_a_correction`): a reader granted `en` alone
        // could otherwise write `alignment`/`run` rows for `raw`, shared state
        // every reader who *can* see `raw` is then served from, which is worse
        // than a read leak.
        //
        // `RefusesAndFilters`: the write half is proven in
        // `every_route_declares_its_grant_posture`'s own dedicated block
        // rather than the generic `Refuses` loops (which explicitly skip
        // this pattern) because a correction needs a body and `If-Match`
        // that a GET-only mechanism cannot send; the response-filter half is
        // proven there too, against a correction between two visible
        // editions, and recorded in `proven_filters` like every other
        // `Filters`/`RefusesAndFilters` entry.
        ("/api/works/{work}/alignments", RefusesAndFilters),
        (
            "/api/me",
            GrantIndependent {
                reason: "who the caller is, not what they may see",
            },
        ),
        (
            "/api/about",
            GrantIndependent {
                reason: "the build and the formats it writes, the same for every caller",
            },
        ),
        (
            "/api/keys",
            GrantIndependent {
                reason: "device credentials for the caller's own account",
            },
        ),
        (
            "/api/keys/{id}",
            GrantIndependent {
                reason: "revokes one of the caller's own keys; \
                          session::revoke_key refuses an id it does not own, \
                          which is an ownership check rather than a root grant",
            },
        ),
        (
            "/api/sessions",
            GrantIndependent {
                reason: "the caller's own signed-in devices",
            },
        ),
        (
            "/api/sessions/{id}",
            GrantIndependent {
                reason: "revokes one of the caller's own sessions, the same \
                          ownership check as /api/keys/{id}",
            },
        ),
        (
            "/api/password",
            GrantIndependent {
                reason: "the caller's own credential",
            },
        ),
        ("/api/library/home", Filters),
        ("/api/library/series", Filters),
        // `Filters`, not `Refuses`: all three reuse
        // `browse::work`/`one_series`/`cover_work`, which filter to visible
        // editions and 404 only when *none* survive — a work split across
        // roots gets 200 with `en` alone, not a 404.
        ("/api/library/series/{id}", Filters),
        ("/api/library/series/{id}/cover", Filters),
        ("/api/library/works/{slug}", Filters),
        ("/api/library/search", Filters),
        ("/api/library/status", Filters),
        // Spec C3 §7's inbox. Every entry states its own reason rather than
        // inheriting a shared one, which is the rule `GrantPosture` holds
        // this table to — and here they genuinely differ: one lists, one
        // draws two series, one evaluates a rule against the whole library,
        // one writes that rule, one records an answer.
        (
            "/api/library/proposals",
            GrantIndependent {
                reason: "admin-only and about the library's organisation, not \
                          about what any one account may see: a proposal names \
                          two series, and an administrator answering it is \
                          deciding structure for everybody",
            },
        ),
        (
            "/api/library/proposals/{id}",
            GrantIndependent {
                reason: "the evidence for one proposal draws both series it \
                          names, the directories they sit in and the volume \
                          each side leads with; narrowing that to the \
                          administrator's own roots would hide exactly the \
                          material the decision is about",
            },
        ),
        (
            "/api/library/proposals/{id}/resolution",
            GrantIndependent {
                reason: "a dry run of a candidate rule over every scan_entry \
                          in the library — the whole answer is which series \
                          the rule would decide for, and scoping it to one \
                          account's roots would under-report the collateral \
                          the guard exists to name",
            },
        ),
        (
            "/api/library/proposals/{id}/accept",
            GrantIndependent {
                reason: "writes a `rule` row, and a rule decides series and \
                          edition keys for the whole library rather than for \
                          the account that wrote it; there is no grant that \
                          could scope it, so admin-only plus the collateral \
                          guard is what bounds it. The DELETE half undoes \
                          exactly that write",
            },
        ),
        (
            "/api/library/proposals/{id}/dismiss",
            GrantIndependent {
                reason: "records, for the whole library, that a question was \
                          answered no — a proposal is not per-account state, \
                          and no grant narrows which ones exist. The DELETE \
                          half takes that answer back, for the whole library \
                          the same way",
            },
        ),
        // **Spec C3 §5's metadata-editing routes (I8) — `GrantIndependent`,
        // not `Refuses`.** `Principal::from_account` drops an administrator's
        // grant outright (O86), so every caller these routes admit holds
        // `Access::Every`, and there is no hidden work for them to refuse.
        //
        // `work_by_slug`, `edition_owner`, `member_owner` and `works_in_series`
        // take the access and filter on it, kept deliberately: it is correct
        // for any restricted caller, and the day the role rule changes it is
        // the difference between a leak and none.
        //
        // **Not "unreachable" — they run on every edit.** An access other than
        // `Every` does not reach them through these routes, so the clause is
        // always a no-op here. The distinction is not pedantry:
        // `metadata::root_grant_clause -> String::new()` would otherwise
        // survive a mutation sweep, because deleting the entire grant filter
        // passes every route test. `metadata::tests::
        // the_grant_clause_still_filters_even_though_no_route_can_supply_a_grant`
        // asserts the contract directly with a slice no route can supply, and
        // fails against that mutation. A guard nothing exercises is a guard
        // nobody can trust.
        (
            "/api/library/works/{slug}",
            GrantIndependent {
                reason: "admin-only, and O86 drops an admin's root grant, so \
                         no caller of this route holds one to refuse against",
            },
        ),
        (
            "/api/library/series/{id}",
            GrantIndependent {
                reason: "admin-only, and O86 drops an admin's root grant, so \
                         no caller of this route holds one to refuse against",
            },
        ),
        (
            "/api/library/editions/{id}",
            GrantIndependent {
                reason: "admin-only, and O86 drops an admin's root grant, so \
                         no caller of this route holds one to refuse against",
            },
        ),
        (
            "/api/library/members/{id}",
            GrantIndependent {
                reason: "admin-only, and O86 drops an admin's root grant, so \
                         no caller of this route holds one to refuse against",
            },
        ),
        (
            "/api/library/works/{slug}/stamp-comicinfo",
            GrantIndependent {
                reason: "admin-only and gated again on `can_stamp`; O86 drops \
                         an admin's root grant, so `work_by_slug`'s grant \
                         clause is empty for every caller that reaches here \
                         and there is no grant left to filter or refuse \
                         against — a slug this route answers 404 for is one \
                         that names nothing, not one held back",
            },
        ),
        // Spec C4 §6's rule editor. Every entry states its own reason rather
        // than inheriting a shared one, the same rule the proposals block
        // above holds this table to — and here too they genuinely differ:
        // one lists the whole set, one writes a rule, one edits or removes
        // one, one derives from a hypothetical set that was never saved.
        (
            "/api/library/rules",
            GrantIndependent {
                reason: "admin-only and about how the library is organised, \
                          not about what any one account may see: a rule is \
                          a property of the scan, and an administrator's \
                          roots are not filtered — the same standing \
                          /api/library/proposals holds above — PR B's PUT \
                          derives over every work in the library from the \
                          proposed set, which is as far from a per-account \
                          scope as a write in this product gets",
            },
        ),
        (
            "/api/library/rules/{id}",
            GrantIndependent {
                reason: "edits or removes one rule; a rule decides series \
                          and edition keys for the whole library rather than \
                          for the account that wrote it, so there is no \
                          grant that could scope either write",
            },
        ),
        (
            "/api/library/rules/preview",
            GrantIndependent {
                reason: "derives against the whole proposed rule list over \
                          every scan_entry in the library and writes nothing \
                          — the answer is what the library-wide structure \
                          would become, which a root grant has no part of \
                          narrowing",
            },
        ),
        (
            "/api/library/rules/import",
            GrantIndependent {
                reason: "replaces the whole rule set from an uploaded \
                          document — the same library-wide write \
                          /api/library/rules itself is, over every rule at \
                          once instead of one",
            },
        ),
        // Spec C4 §2's roots routes. Every entry states its own reason rather
        // than inheriting a shared one, the same rule the rules block above
        // holds this table to.
        (
            "/api/library/roots",
            GrantIndependent {
                reason: "admin-only; lists scan roots, which are a \
                          property of the scan itself rather than of any \
                          one account's access to it — the same standing \
                          the rules block above holds",
            },
        ),
        (
            "/api/library/roots/{id}",
            GrantIndependent {
                reason: "edits or removes one root; both are library-wide \
                          writes with no account-scoped meaning to filter \
                          against, the identical reasoning \
                          /api/library/rules/{id} states for a rule",
            },
        ),
        (
            "/api/library/roots/{id}/move",
            GrantIndependent {
                reason: "moves one directory between libraries, a write to the \
                          whole store that changes what a library is and so \
                          what a grant on it means, with no account's own \
                          grant to filter it by",
            },
        ),
        // Spec C4 §3's scan-control route.
        (
            "/api/library/roots/{id}/scan",
            GrantIndependent {
                reason: "starts or reports on a scan of one root, a \
                          library-wide operation with no account-scoped \
                          meaning to filter against — the identical standing \
                          /api/library/roots/{id} holds for a root's own \
                          edit or removal, one step earlier in this same \
                          table",
            },
        ),
        (
            "/api/library/libraries",
            GrantIndependent {
                reason: "admin-only; lists and makes libraries, and what an \
                          account is granted is a library, so filtering this \
                          list by the administrator's own grant would hide \
                          the libraries being administered",
            },
        ),
        (
            "/api/library/libraries/{id}",
            GrantIndependent {
                reason: "renames or removes one library, a write to the \
                          whole store with no account-scoped meaning to \
                          filter against",
            },
        ),
        (
            "/api/library/libraries/{id}/dirs",
            GrantIndependent {
                reason: "adds a directory to one library, a write to the \
                          whole store with no account-scoped meaning to \
                          filter against",
            },
        ),
        (
            "/api/library/libraries/{id}/dirs/{dir}",
            GrantIndependent {
                reason: "takes one directory out of one library, a write to \
                          the whole store that changes what a library is and \
                          so what a grant on it means, as \
                          /api/library/roots/{id}/move does, with no \
                          account's own grant to filter it by",
            },
        ),
        (
            "/api/library/libraries/{id}/scan",
            GrantIndependent {
                reason: "starts or reports on a scan of one library's \
                          directories, as the standing /api/library/roots/{id}/scan \
                          does for one directory",
            },
        ),
        (
            "/api/library/split-editions",
            GrantIndependent {
                reason: "admin-only; lists every library's share of each \
                          edition the libraries split, a fact about the whole \
                          store that the grants are set against",
            },
        ),
        (
            "/api/library/dirs",
            GrantIndependent {
                reason: "admin-only; names directories beneath the prefixes \
                          the server was started with, which are a fact \
                          about the deployment and hold nothing any \
                          account's grant could scope",
            },
        ),
        // Spec C4 §4's user routes. Every entry states its own reason rather
        // than inheriting a shared one, the same rule the rules and roots
        // blocks above hold this table to.
        (
            "/api/library/users",
            GrantIndependent {
                reason: "admin-only; lists and creates accounts, and an \
                          account's own root grants are the thing being \
                          edited here rather than a filter to apply to the \
                          answer — filtering this list by the *administrator's* \
                          grant would hide the very accounts being administered",
            },
        ),
        (
            "/api/library/users/{id}",
            GrantIndependent {
                reason: "edits or removes one account, including the root \
                          grants that decide what *that* account may see — a \
                          write about somebody else's access, with no meaning \
                          to filter against the administrator's own",
            },
        ),
        (
            "/api/library/users/{id}/sessions/{sid}",
            GrantIndependent {
                reason: "ends one browser session of one account — C4 §4's \
                          'a device was lost', which is about a credential \
                          rather than about any material a grant could scope",
            },
        ),
        (
            "/api/library/users/{id}/keys/{kid}",
            GrantIndependent {
                reason: "ends one API key of one account, the identical \
                          standing the sessions route above holds, for the \
                          credential an OPDS reader presents instead",
            },
        ),
        (
            "/api/library/backups",
            GrantIndependent {
                reason: "admin-only; lists and takes snapshots of the whole \
                          database, which is not any one account's material \
                          and has no per-root shape to filter",
            },
        ),
        (
            "/api/library/backups/{name}",
            GrantIndependent {
                reason: "hands back one whole-database snapshot — password \
                          hashes and every root's rows together, which is \
                          precisely why it is admin-only rather than filtered",
            },
        ),
        (
            "/opds/v1.2/catalog",
            GrantIndependent {
                reason: "static navigation entries naming no work and no library",
            },
        ),
        ("/opds/v1.2/works", Filters),
        // A work or edition outside the grant must not be *named*, but a
        // work with no edition inside the grant still answers 404
        // (`browse::work`'s own contract, `work_feed`'s doc comment) — a list
        // that can be entirely empty is still a list, the same way
        // `list_works` answering `[]` for a grant with nothing to show is
        // still `Filters` rather than `Refuses`.
        ("/opds/v1.2/works/{work}", Filters),
        ("/opds/v1.2/works/{work}/{edition}/pages/{number}", Refuses),
        ("/opds/v1.2/works/{work}/{edition}/cover", Refuses),
        // The same handler as `/api/works/{work}/{edition}/download` behind
        // the same `view::of_edition`, so necessarily the same posture.
        // Driven rather than declared: `grant_proof_concrete` substitutes
        // `{work}` and `{edition}`, so the `Refuses` loops pick this up
        // against both a wholly-hidden work and one split across roots with
        // no further work here.
        ("/opds/v1.2/works/{work}/{edition}/download", Refuses),
        ("/opds/v1.2/libraries", Filters),
        // A library outside the grant answers 404, as one that does not exist
        // does, and a granted library's works are listed with only the
        // editions under the directories it shows, as `/opds/v1.2/works` lists
        // them under the grant's.
        ("/opds/v1.2/libraries/{library}/works", RefusesAndFilters),
        ("/opds/v1.2/libraries/by-work", Filters),
        // The same 404, and the works under the directories the library shows
        // listed with every edition the grant allows.
        (
            "/opds/v1.2/libraries/{library}/works/by-work",
            RefusesAndFilters,
        ),
    ]
}

/// What a route does with an edition an account sees only part of (spec
/// §3.8, "Every route declares it"), declared beside [`GrantPosture`] for
/// the same reason: `auth::tests::every_route_declares_what_it_does_with_a_narrowed_edition`
/// drives each declaration against a narrowed edition, so a route added
/// later cannot forget. `#[cfg(test)]` for `GrantPosture`'s reason.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum NarrowingPosture {
    /// It takes a page index or a member ordinal in the account's numbering
    /// and reads the full numbering's.
    Translates,
    /// It sends a document, list or count built from the account's view.
    Narrows,
    /// It sends a whole document and takes it back whole, keeping what the
    /// account is not sent.
    KeepsUnseen,
    /// It refuses a request that names a narrowed edition.
    Refuses,
    /// It reads no edition's pages; `reason` says why.
    NoPages { reason: &'static str },
}

/// The table [`NarrowingPosture`]'s doc describes: every entry of
/// [`guarded_routes`], in its order.
#[cfg(test)]
pub(crate) fn narrowing_postures() -> Vec<(&'static str, NarrowingPosture)> {
    use NarrowingPosture::{KeepsUnseen, Narrows, NoPages, Refuses, Translates};
    const OWN: NarrowingPosture = NoPages {
        reason: "the caller's own account or this build, naming no work",
    };
    const ADMIN: NarrowingPosture = NoPages {
        reason: "administrators only, and an administrator is never narrowed",
    };
    vec![
        ("/api/works", Narrows),
        ("/api/works/{work}", Narrows),
        ("/api/works/{work}/{edition}/pages/{index}", Translates),
        (
            "/api/works/{work}/{edition}/pages/{index}/thumb",
            Translates,
        ),
        ("/api/works/{work}/{edition}/download", Narrows),
        (
            "/api/works/{work}/{edition}/members/{ordinal}/download",
            Translates,
        ),
        ("/api/works/{work}/progress", Translates),
        ("/api/works/{work}/notes", KeepsUnseen),
        ("/api/works/{work}/truth", KeepsUnseen),
        ("/api/works/{work}/alignments", Refuses),
        ("/api/me", OWN),
        ("/api/about", OWN),
        ("/api/keys", OWN),
        ("/api/keys/{id}", OWN),
        ("/api/sessions", OWN),
        ("/api/sessions/{id}", OWN),
        ("/api/password", OWN),
        ("/api/library/home", Narrows),
        ("/api/library/series", Narrows),
        ("/api/library/series/{id}", Narrows),
        ("/api/library/series/{id}/cover", Narrows),
        ("/api/library/works/{slug}", Narrows),
        ("/api/library/search", Narrows),
        ("/api/library/status", Narrows),
        ("/api/library/proposals", ADMIN),
        ("/api/library/proposals/{id}", ADMIN),
        ("/api/library/proposals/{id}/resolution", ADMIN),
        ("/api/library/proposals/{id}/accept", ADMIN),
        ("/api/library/proposals/{id}/dismiss", ADMIN),
        ("/api/library/works/{slug}", ADMIN),
        ("/api/library/series/{id}", ADMIN),
        ("/api/library/editions/{id}", ADMIN),
        ("/api/library/members/{id}", ADMIN),
        ("/api/library/works/{slug}/stamp-comicinfo", ADMIN),
        ("/api/library/rules", ADMIN),
        ("/api/library/rules/{id}", ADMIN),
        ("/api/library/rules/preview", ADMIN),
        ("/api/library/rules/import", ADMIN),
        ("/api/library/roots", ADMIN),
        ("/api/library/roots/{id}", ADMIN),
        ("/api/library/roots/{id}/move", ADMIN),
        ("/api/library/roots/{id}/scan", ADMIN),
        ("/api/library/libraries", ADMIN),
        ("/api/library/libraries/{id}", ADMIN),
        ("/api/library/libraries/{id}/dirs", ADMIN),
        ("/api/library/libraries/{id}/dirs/{dir}", ADMIN),
        ("/api/library/libraries/{id}/scan", ADMIN),
        ("/api/library/split-editions", ADMIN),
        ("/api/library/dirs", ADMIN),
        ("/api/library/users", ADMIN),
        ("/api/library/users/{id}", ADMIN),
        ("/api/library/users/{id}/sessions/{sid}", ADMIN),
        ("/api/library/users/{id}/keys/{kid}", ADMIN),
        ("/api/library/backups", ADMIN),
        ("/api/library/backups/{name}", ADMIN),
        (
            "/opds/v1.2/catalog",
            NoPages {
                reason: "static navigation entries naming no work",
            },
        ),
        ("/opds/v1.2/works", Narrows),
        ("/opds/v1.2/works/{work}", Narrows),
        (
            "/opds/v1.2/works/{work}/{edition}/pages/{number}",
            Translates,
        ),
        // Page 0, so it translates the account's page 0.
        ("/opds/v1.2/works/{work}/{edition}/cover", Translates),
        ("/opds/v1.2/works/{work}/{edition}/download", Narrows),
        ("/opds/v1.2/libraries", Narrows),
        ("/opds/v1.2/libraries/{library}/works", Narrows),
        ("/opds/v1.2/libraries/by-work", Narrows),
        ("/opds/v1.2/libraries/{library}/works/by-work", Narrows),
    ]
}

/// The routes that are **not** behind the guard, and why each one cannot be.
///
/// Beside [`guarded_routes`] rather than scattered, so the split is one screen
/// rather than a search — and so `auth::tests::every_route_refuses_an_anonymous_request`
/// has an explicit statement of what it is not asserting over.
///
/// `POST /api/session` is a login: a request that is about to authenticate
/// cannot already have. `DELETE /api/session` is a logout, which must work from
/// a session that has already expired — a browser holding a stale cookie needs
/// it cleared, and answering 401 would leave it stuck holding the thing it was
/// trying to get rid of.
pub(crate) fn open_routes() -> Vec<(&'static str, axum::routing::MethodRouter<Arc<ServeState>>)> {
    vec![
        (
            "/api/session",
            axum::routing::post(crate::session::login).delete(crate::session::logout),
        ),
        // Registered unconditionally and shut from inside, rather than
        // registered only when the flag is on: the two are indistinguishable to
        // a caller — both answer 404 — and one of them cannot get the condition
        // wrong at startup and leave the route standing.
        ("/api/claim", axum::routing::post(crate::session::claim)),
    ]
}

/// `Cache-Control: private, no-store` on every guarded response that did not
/// set one itself. See the call site in [`build_router_serving`].
async fn per_reader_by_default(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(request).await;
    if !response.headers().contains_key(header::CACHE_CONTROL) {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("private, no-store"),
        );
    }
    response
}

/// Build the router. Separate from `serve` so tests can drive it with
/// `oneshot` and never open a socket.
pub fn build_router(state: Arc<ServeState>) -> Router {
    build_router_serving(state, None)
}

/// [`build_router`], plus — given `web` — the built app in that directory,
/// answering every path no route matched (see [`crate::web`]).
///
/// **A parameter rather than a field on [`ServeState`]**: where the app lives
/// is fixed for the life of the process and read once, here, while the state
/// is built by hand across this crate's tests, none of which serve an app.
pub(crate) fn build_router_serving(
    state: Arc<ServeState>,
    web: Option<&std::path::Path>,
) -> Router {
    let log = state.log.clone();
    let guarded = guarded_routes()
        .into_iter()
        .fold(Router::new(), |router, (path, handler)| {
            router.route(path, handler)
        })
        // **`route_layer` rather than `layer`**, which is axum's own
        // distinction and matters here: `layer` would wrap the fallback too, so
        // an anonymous request to a path that does not exist would answer 401
        // instead of 404 — telling a caller that every URL they invent is a
        // real one they are merely not allowed to see. `route_layer` runs only
        // where a route matched, which is what the enumeration test asserts
        // over and what a 404 should still be able to say.
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::guard,
        ))
        // **A default, not a header at each call site.** Every guarded
        // response is per-reader unless it says otherwise. The OPDS feeds need
        // the default: they get no `Set-Cookie` and so lack even the accident
        // that makes most proxies decline to cache an authenticated response.
        //
        // A response that has already set `Cache-Control` keeps it: the page and
        // thumbnail routes state their own, `PAGE_CACHE_CONTROL` or a versioned
        // cover's `COVER_CACHE_CONTROL`, and mean it. Everything else gets
        // `private, no-store`, so a proxy — which spec A2 §11 puts in front —
        // cannot store one reader's notes, position or catalogue and replay
        // them to another. RFC 9111's bar on caching authenticated responses
        // keys on `Authorization`, which the browser cookie path never sends.
        .layer(axum::middleware::from_fn(per_reader_by_default));

    // Merged *after* the guard is applied, which is the whole reason these two
    // are a separate list: `route_layer` wraps the routes registered before it,
    // so anything merged afterwards is outside it. That is a sharp edge — a
    // route added to the wrong list is silently open.
    //
    // **Two tests stop that being invisible.**
    // `every_route_refuses_an_anonymous_request` checks `guarded_routes`'s
    // paths against a typed list, so a route that leaves the guarded one
    // fails it;
    // `open_routes_are_the_ones_that_cannot_be_guarded` pins the exemption
    // list itself and proves each entry is reachable anonymously.
    let open = open_routes()
        .into_iter()
        .fold(Router::new(), |router, (path, handler)| {
            router.route(path, handler)
        });

    let mut app = guarded.merge(open);
    // **After the merge, before the log.** A fallback set on `guarded` would
    // sit inside `per_reader_by_default` and give the app's files `private,
    // no-store`; one set after the log layer below would be the only thing
    // `--log-requests` could not see.
    if let Some(dir) = web {
        let web = crate::web::Web::new(dir);
        app = app.fallback(move |request: axum::extract::Request| web.clone().answer(request));
    }

    app.layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let log = log.clone();
            async move {
                let Some(log) = log else {
                    return next.run(request).await;
                };
                let method = request.method().to_string();
                // Path *and* query string: a client asking for
                // `?width=1536` is invisible if only `path()` is logged.
                let path = request
                    .uri()
                    .path_and_query()
                    .map_or_else(
                        || request.uri().path(),
                        axum::http::uri::PathAndQuery::as_str,
                    )
                    .to_string();
                // Read here, before the request is consumed: by the time
                // a response exists, what it was asked with is gone.
                // Filtered to `LOGGED_HEADERS` at this end rather than
                // in `reqlog`, which deliberately knows nothing about
                // `http` types — see `Log::begin`.
                let headers = crate::reqlog::LOGGED_HEADERS
                    .iter()
                    .filter_map(|name| {
                        let value = request.headers().get(*name)?.to_str().ok()?;
                        Some(((*name).to_string(), value.to_string()))
                    })
                    .collect();
                let started = log.begin(headers);
                let response = next.run(request).await;
                let status = response.status().as_u16();
                // The body's own known length, not the Content-Length
                // header: axum sets that header when it serialises the
                // response, which is after this middleware has already
                // run — so reading the header here reports 0 for every
                // request, and bytes-per-page is one of the numbers this
                // log exists to produce.
                let bytes = {
                    use axum::body::HttpBody;
                    response.body().size_hint().exact().unwrap_or(0)
                };
                log.finish(started, &method, &path, status, bytes);
                response
            }
        },
    ))
    .with_state(state)
}

#[cfg(test)]
mod tests;
