//! HTTP. Everything interesting happens below this file.

// **Imported rather than written `std::collections::` inline, and that is
// load-bearing for the mutation harness — O88.** `cargo-mutants` replaces a
// function body with a literal it generates and emits the SHORT type name, so
// a function *returning* a fully-qualified collection has whole-body mutants
// that fail to compile and are reported `unviable` — never run, and invisible
// in the survivor list, because an unviable mutant is not a survivor.
use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Path as AxPath, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

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
/// and the OPDS stream — used to declare the index as `u32` in the extractor
/// and let axum's `Path` rejection answer a malformed one. That message is
/// phrased in terms of the Rust type it failed to parse into and the tuple
/// position it sat at ("Cannot parse value at index 2 with value `abc` to a
/// `u32`"), which names two things a client knows nothing about, and it was
/// the only message this server sent that this server had not written.
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

/// The member `sha256` and within-member offset a page's `ETag` is derived
/// from — found without reading a single byte, so a route can answer a
/// matching `If-None-Match` before doing any of the work that would produce
/// those bytes: a zip decompression from disk, or an HTTP round trip to
/// Stump.
///
/// The same lookup and the same wording `ServeState::page` uses on its own
/// way to the bytes (`lib.rs`), duplicated here rather than shared with it,
/// because the two callers want different things back: this one needs only
/// the location, `ServeState::page` needs only the bytes, and threading a
/// location out through it would make every caller that has no use for an
/// `ETag` pay to compute one. `get_thumb` already draws this same split, for
/// the same reason — its own locate-then-generate call inside
/// `spawn_blocking` is the same shape, one level further in.
pub(crate) fn locate_member(
    work: &Work,
    edition: &str,
    index: u32,
) -> Result<(String, u32), String> {
    let sidecar = work.sidecar();
    let found = sidecar
        .editions
        .get(edition)
        .ok_or_else(|| format!("no edition `{edition}` in {}", work.path.display()))?;
    let (position, within) = found.locate(index).ok_or_else(|| {
        format!(
            "`{edition}` has {} pages; there is no page {index}",
            found.total_pages()
        )
    })?;
    Ok((found.members[position].sha256.clone(), within))
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
/// the subtle way to break this. One page now has as many representations as a
/// reader has viewport sizes; a tag naming only the member and offset would let
/// a client that cached the 1024px version be answered `304` when it later
/// asked for 2048, and go on drawing the small one — a page that looks soft for
/// a day with nothing to point at. `None` is the full-size request and keeps
/// the tag it always had, so a client holding a page cached before this
/// existed still validates against it.
pub(crate) fn page_etag(sha256: &str, offset: u32, long_edge: Option<u32>) -> String {
    match long_edge {
        Some(edge) => format!("\"{sha256}-p{offset}-r{edge}\""),
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
/// O1: an `ETag` shrinks a repeat to about 300 bytes; it does not remove the
/// round trip. Prefetch exists to hide a round trip before the reader notices
/// it, so a round trip that still has to happen — just for fewer bytes — is one
/// the reader still waits on. This is the half that makes a genuine repeat cost
/// nothing.
///
/// `private` because this server has no auth, and the cache this exists to
/// reach is the reader's own browser rather than a shared proxy.
///
/// **Not `immutable`, and the reason is the URL rather than the bytes.** A
/// page's bytes never change for a given member `sha256` and offset — that is
/// exactly what `page_etag` above is derived from — but the URL names a work,
/// an edition and an index, none of which move when an archive is replaced.
/// `immutable` tells a client never to revalidate for the entry's whole
/// lifetime, so replacing a member would leave every reader who had seen that
/// page holding the old bytes, with no way to correct it short of clearing the
/// browser's cache. The bound below is what makes a replacement
/// self-correcting.
///
/// **No `Last-Modified` alongside it, deliberately.** It would need the
/// member file's mtime, which needs a `stat`, on the async worker, on every
/// request — putting disk I/O back on the one path `locate_member` above is
/// written to keep clear of. It would also be the weaker validator of the two:
/// these bytes are addressed by content, and mtime is not, so re-downloading a
/// byte-identical archive would invalidate a cache entry that is still correct.
/// The `ETag` covers revalidation; this covers freshness; there is no third
/// question for a date to answer.
///
/// **`no-cache` rather than a freshness lifetime, because the URL is not a
/// unique name for these bytes.** This was `private, max-age=86400`, chosen so
/// that a replaced archive would correct itself inside a day. The paragraph
/// above had the shape of the problem right — "the URL names a work, an
/// edition and an index" — and one thing wrong: a **work id is not unique**.
/// It is the sidecar's filename stem, and every sidecar across this corpus is
/// `taiyaku.json`, so `/api/works/taiyaku/raw-mono/pages/10` addresses a
/// different volume's page 10 depending only on which sidecar the server was
/// started with.
///
/// Measured on two of them: the same URL returned images with SHA-256s
/// `ab5da993…` and `17655ca0…`, under `ETag`s the server had already derived
/// from different member digests. The server could tell them apart the whole
/// time; the client was told not to ask for a day. A labeller restarting the
/// server onto the next volume — which this corpus requires, since two works
/// cannot be served at once for the same reason — saw the previous volume's
/// artwork with the new volume's page numbers, and the only symptom was
/// recognising the pictures.
///
/// That is worse than a stale read. Boxes drawn against it would be labels on
/// the wrong art, which `TruthEdition::is_stale_for` calls the sharpest form
/// of the staleness problem, and nothing downstream could detect it.
///
/// `no-cache` does not mean "do not store": the entry is kept and revalidated,
/// so the `ETag` above does the work it was always able to do and a page that
/// really is unchanged comes back as a bodiless `304`. The cost is one
/// conditional request per page rather than none; the alternative is a client
/// that cannot be told it is looking at the wrong book.
pub(crate) const PAGE_CACHE_CONTROL: &str = "private, no-cache";

/// The reply to a validator that already matches: the tag, and no body —
/// sending the very bytes the header exists to avoid transferring would
/// defeat the entire point of a `304`.
///
/// **`freshness` is a parameter rather than a constant, and the four callers
/// do not agree on it.** RFC 9111 §4.3.4 has a client update its stored
/// response from a `304`'s headers, so whatever this sends is what the client
/// believes until it asks again. For a page that is `PAGE_CACHE_CONTROL`:
/// omitting it would leave the entry exactly as stale as it was and cost a
/// round trip on the very next read, so a revalidation that carries no
/// freshness buys one response and no more. For a sidecar or a notes document
/// it is `None`, and that is not a smaller version of the same thing — those
/// two are rewritten by `PATCH /alignments` and `PUT /notes`, so a freshness
/// lifetime on either says a reader may go a day without seeing a correction
/// they made themselves.
///
/// Written as an argument each caller must supply rather than a default one
/// of them overrides, because the first version of this took the page constant
/// unconditionally and silently handed it to `get_sidecar` and `get_notes` —
/// which share this function and want the opposite answer. A caller that has
/// to say which it is cannot inherit the wrong one by being written later.
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

/// A notes or truth document's `ETag`: a SHA-256 over exactly the bytes being
/// sent.
///
/// Moved to `crate::notes` (Task 13, as `etag_for`) so it sits beside
/// `notes::to_json`, the projection it is computed over — the two are the
/// invariant `notes.rs`'s and `truth.rs`'s own doc comments state, and
/// keeping them in the same module is what makes that visible rather than
/// asserted. Re-exported under its old name so every existing call site in
/// this file, and every test that reaches it through `super::*`, keeps
/// resolving unchanged.
pub(crate) use crate::notes::etag_for as notes_etag;

/// Whether a request's `If-Match` names `tag`.
///
/// `None` where the header is absent, which is a different answer from `false`
/// and gets a different status: a client that sent no validator has not lost a
/// race, it has not entered one, and telling it "precondition failed" would
/// send it looking for a conflict that does not exist.
///
/// Proven directly by `if_match_distinguishes_absent_from_matching_from_stale`
/// below, as well as through `patch_alignments`, the one route that writes
/// against it.
pub(crate) fn if_match_hits(headers: &HeaderMap, tag: &str) -> Option<bool> {
    let sent = headers.get(header::IF_MATCH)?.to_str().ok()?;
    Some(sent == tag)
}

/// A request naming a work this server does not hold.
///
/// A named type rather than a bare `Response` because a `Response` is 128
/// bytes and `Result<&Work, Response>` trips `clippy::result_large_err`. It
/// carries the status and the wording together, which is the point: one
/// answer to one question, wherever it is asked.
#[derive(Debug)]
struct NoSuchWork(String);

impl IntoResponse for NoSuchWork {
    fn into_response(self) -> Response {
        not_found(self.0)
    }
}

/// The work a route names, or the refusal to send instead.
///
/// Every route below is addressed by work, and M3 added more — a thumbnail, a
/// stream, a work feed. `ServeState::work_or_missing` is the one place that
/// decides the wording; this just wraps it in the `Response`-carrying type
/// `get_sidecar` — the one handler here that touches no disk, and so is the
/// one that still runs on the async worker — wants.
fn work<'a>(state: &'a ServeState, id: &str) -> Result<&'a Work, NoSuchWork> {
    state.work_or_missing(id).map_err(NoSuchWork)
}

/// Why a progress request could not be answered.
///
/// Two kinds, and they are not one status: a work this server does not hold,
/// or an edition or slot the work does not have, is the client's mistake and a
/// 404; a position file that will not read or write is this server's and a
/// 500. `get_page` needs no such type because both of its failures are 404s.
/// Progress is the one route where the distinction has to survive the trip
/// back out of `spawn_blocking`, which flattens a handler's failures into one
/// `Result` however different they were on the way in.
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

/// Every work this caller's grant admits, as `GET /api/works` answers it:
/// slug, title, series, volume, editions and a cover URL — see
/// `works::WorkSummary` for where each of those six comes from and why two
/// of them are `null` today.
///
/// **Applied no grant at all until this route joined the other four.** This
/// list answered every work the server held regardless of `Principal.roots`,
/// which handed a restricted account the id, title, series and volume of
/// every work outside every root it was granted — the same shape of gap the
/// whole-branch review closed on the page routes (`refuse_if_hidden`) and
/// commit 5d41bd6 closed on the OPDS mirror (`opds::visible_editions_by_work`,
/// named `visible_slugs` at the time).
///
/// **Filtered `editions` too, and the `cover` URL built from a visible one —
/// gated only at the work level until a second whole-branch review.** This
/// list first filtered by the now-removed `browse::visible_work_slugs`,
/// which answered "does this work keep *any* edition under a granted root",
/// and then reported
/// every edition of each surviving work regardless: a work split across
/// roots (`en` granted, `raw` withheld) named `raw` in its `editions` array
/// and built its `cover` from `sidecar.hub` unconditionally, so a hidden
/// hub's key reached the client through a thumbnail URL. Filtered per
/// edition now, through `works::summary`'s `visible` parameter — the same
/// per-edition question `get_sidecar` and `patch_alignments` ask via
/// `visible_editions_or_refuse`, asked here for every work in the library at
/// once so this route stays a list rather than becoming a lookup.
///
/// **A list, not a lookup: filtered, not refused.** A restricted account gets
/// the works it may see and simply does not learn about the rest; refusing
/// the whole route the way `refuse_if_hidden` refuses a single edition would
/// hide the library from every restricted reader, not just the parts outside
/// their grant.
///
/// **One query for the whole list, not one per work.** Asking
/// `browse::work` per work here — the tool the single-work routes use — would
/// be the same N+1 the whole-branch review closed in `home` at 111 queries
/// on the largest series, reopened at the edition level this time:
/// `browse::visible_editions` answers every work's visible edition set in one
/// round trip, and this filter is then only a `HashSet` lookup per edition.
/// An empty grant is unrestricted and skips that query entirely, the same
/// short-circuit `refuse_if_hidden` applies.
///
/// **Not a home screen.** Spec C1 is the browse UI this payload exists to
/// eventually feed; nothing here builds one, and `openFirstWork`
/// (`web/src/startup.ts`) still only ever opens the first entry.
///
/// One connection, opened once, rather than `works::summary` opening its own
/// through a `Library`-taking wrapper the way `progress::load` does — a list
/// answers every work in one request, so one lock over the whole pass beats
/// re-acquiring `state.library`'s mutex per work.
async fn list_works(
    State(state): State<Arc<ServeState>>,
    request_extensions: axum::http::Extensions,
) -> Response {
    let roots = request_extensions
        .get::<crate::auth::Principal>()
        .map_or_else(Vec::new, |principal| principal.roots.clone());

    // A SELECT per work against `state.library` — `get_page`'s reasoning
    // about the async worker applies unchanged.
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let visible = if roots.is_empty() {
            None
        } else {
            Some(taiyaku_library::browse::visible_editions(&conn, &roots)?)
        };
        state
            .works
            .iter()
            // A work absent from `visible`'s keys has no edition inside the
            // grant at all — `browse::visible_editions` never mints an entry
            // for one — so it is dropped here exactly as it was before, and
            // a work present in it always has a non-empty edition set for
            // `summary` to fall back on.
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

/// Refuse a work or edition the caller's grant hides, as a 404.
///
/// **The page routes applied no grant at all until the whole-branch review
/// said so**, which made spec C1's per-root sharing a browse filter rather
/// than a boundary: a restricted reader who knew a slug could read every page
/// of every root, because `/api/works/{slug}/{edition}/pages/N` never asked.
/// Browse hid the work and the bytes were served anyway.
///
/// **404, not 403**, matching every other refusal here: telling an
/// unauthorised caller which slugs are real is the enumeration this project
/// refuses elsewhere.
///
/// An empty grant is unrestricted and short-circuits inside
/// `browse::edition_visible` without touching the database, so the ordinary
/// single-reader deployment pays nothing for this.
///
/// **Progress, notes and truth are deliberately not guarded here.** They are
/// per-user state keyed on a work rather than the library's material, a
/// reader can only write their own, and widening the check to them is a
/// question about what a grant *means* rather than about what it leaks —
/// recorded as an open item rather than answered in a fix round.
pub(crate) fn refuse_if_hidden(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
    slug: &str,
    edition: &str,
) -> Option<Response> {
    let principal = extensions.get::<crate::auth::Principal>()?;
    if principal.roots.is_empty() {
        return None;
    }
    let conn = state.library.conn();
    match taiyaku_library::browse::edition_visible(&conn, &principal.roots, slug, edition) {
        Ok(true) => None,
        Ok(false) => Some(not_found(format!("no work `{slug}`"))),
        Err(why) => Some((StatusCode::INTERNAL_SERVER_ERROR, why).into_response()),
    }
}

/// **403, not 404, and that is the design.** `refuse_if_hidden` above answers
/// 404 because an edition outside the grant does not admit that it exists;
/// this answers 403 because the caller *may* see this edition and may not have
/// a copy of it — a different fact that a 404 would lie about.
///
/// **This landed as an always-passing stub and did not stay one.** Task 7 added
/// it returning `None` so the crate would compile before the capability work,
/// and a security review flagged the obvious: a fail-open authorization gate in
/// committed code is not made safe by a comment promising a later task. The
/// real check is here; Task 9 keeps the tests that prove a withheld capability
/// refuses, and the `can_stream` half beside it. Half of O54.
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
/// Both columns default to 1 (`schema/004_auth.sql:18`), so nothing changed for
/// any existing account the day this shipped, and nothing can set either to 0
/// outside a fixture until spec C4's user administration. Spec C2 §2.2.
pub(crate) fn refuse_if_cannot_stream(extensions: &axum::http::Extensions) -> Option<Response> {
    let principal = extensions.get::<crate::auth::Principal>()?;
    (!principal.can_stream).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may browse and download but not read pages here",
        )
            .into_response()
    })
}

/// Which editions of `slug` this caller may see, or the refusal to send
/// instead — [`refuse_if_hidden`]'s whole-work counterpart, for a route that
/// has no single edition to check against: the document `get_sidecar`
/// returns lists every edition, and the rows `patch_alignments` writes are
/// not scoped to one edition either, since a correction names two.
///
/// **Data-carrying, unlike `refuse_if_hidden`, and that is the fix.** The
/// function this replaced (`refuse_if_work_hidden`) answered only the
/// *work*-level question — does `slug` keep any edition under a granted
/// root? — and both of this crate's callers then sent back the whole
/// document regardless: a grant that let the work through gated nothing
/// inside it, which the whole-branch review found leaking a hidden
/// edition's key, label, member path and `sha256` through `get_sidecar` and
/// `patch_alignments` alike. `browse::work` already answers the finer
/// question — `WorkCard::editions` is "only the editions this caller may
/// see", its own contract — so this returns that set instead of discarding
/// it, and the two callers filter with it via [`filter_sidecar_editions`]
/// rather than reinventing the rule a third time.
///
/// `Ok(None)`: an unrestricted caller — no [`crate::auth::Principal`] at
/// all, or an empty grant (every account in an un-migrated library holds
/// one, spec C1 §5.1) — and the database is never touched, the same
/// short-circuit `refuse_if_hidden` applies.
/// `Ok(Some(keys))`: a restricted caller; `keys` is `slug`'s visible
/// edition set, and a caller with nothing to filter still gets a real
/// (possibly full) set rather than `None`, so it cannot be mistaken for
/// "unrestricted" downstream.
/// `Err`: the refusal to send instead. `browse::work` answers `None` for a
/// work outside the grant exactly as it does for one that does not exist,
/// so this refuses identically either way — the same rule `refuse_if_hidden`
/// states for [`taiyaku_library::browse::edition_visible`].
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, which every other refusal-returning function in \
              this file also does (`NoSuchWork`, `NoPrincipal`, `NoNotes`, `NoTruth` all wrap \
              one for the identical reason their own doc comments give); this is a routing \
              handler's own refusal path, called once per request, not a hot loop, so the 128 \
              bytes clippy measures are not worth an indirection at every call site"
)]
fn visible_editions_or_refuse(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
    slug: &str,
) -> Result<Option<HashSet<String>>, Response> {
    let Some(principal) = extensions.get::<crate::auth::Principal>() else {
        return Ok(None);
    };
    if principal.roots.is_empty() {
        return Ok(None);
    }
    let conn = state.library.conn();
    match taiyaku_library::browse::work(&conn, &principal.roots, principal.user_id, slug) {
        Ok(Some(detail)) => Ok(Some(
            detail.card.editions.into_iter().map(|e| e.key).collect(),
        )),
        Ok(None) => Err(not_found(format!("no work `{slug}`"))),
        Err(why) => Err((StatusCode::INTERNAL_SERVER_ERROR, why).into_response()),
    }
}

/// A copy of `sidecar` with every edition and alignment outside `visible`
/// removed — what [`get_sidecar`] and [`patch_alignments`] send a restricted
/// caller instead of the document verbatim.
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
/// visible edition at all (`visible_editions_or_refuse` never returns an
/// empty set for one — `browse::work` answers `None` first), so this always
/// finds a fallback when it needs one.
fn filter_sidecar_editions(
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

async fn get_sidecar(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    request_extensions: axum::http::Extensions,
) -> Response {
    // The document lists every edition, so a grant that hides the whole work
    // must refuse it, and a grant that only hides *some* of it must filter
    // the rest — `visible_editions_or_refuse` answers both.
    let visible = match visible_editions_or_refuse(&state, &request_extensions, &id) {
        Ok(visible) => visible,
        Err(refusal) => return refusal,
    };
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    let sidecar = found.sidecar();
    // The `ETag` is computed over the **full** document regardless of what
    // this response sends: it is the resource's own fingerprint, and it is
    // what `patch_alignments`'s `If-Match` validates against — a tag over a
    // caller-specific projection would never match that check, and a
    // restricted reader's own correction would fail its precondition every
    // time. Filtering changes the body, not the version being agreed on.
    // The cost of that choice: the tag is also a change-detection oracle
    // over content this caller cannot see — a restricted reader's next
    // `If-None-Match` answers 200 whenever a *withheld* edition changes,
    // never a 304, even though nothing in the body they can see moved. Never
    // a *shared* cache, though — `every_guarded_response_refuses_a_shared_cache`
    // covers that half — so the leak is "something changed," not what.
    let tag = sidecar_etag(&sidecar);
    if if_none_match_hits(&headers, &tag) {
        return not_modified(&tag, None);
    }
    let body = match &visible {
        Some(keys) => filter_sidecar_editions(&sidecar, keys).to_json(),
        // Served verbatim from the parsed document rather than by
        // re-reading the file, so what the client resolves against is
        // exactly what this server holds — and exactly what the `ETag`
        // above was computed over.
        None => sidecar.to_json(),
    };
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, tag.as_str()),
        ],
        body,
    )
        .into_response()
}

/// The optional `?w=` on a page request: the long edge, in device pixels, the
/// reader intends to draw this page within.
///
/// **Why the server resizes at all.** Measured in the browser against
/// `gotoubun-v01`'s `en-mono` — 2147×3056, 6.56 megapixels — drawn into a
/// 693×475 box: the decode alone costs 74.5 ms and the decoded bitmap is
/// 26.2 MB, for twenty times the pixels that reach the screen. The same page
/// fitted to 700px decodes in 4.3 ms and holds 2.8 MB. Both halves matter: the
/// decode is what a page turn waits on, and the bitmap is what the browser
/// evicts under memory pressure, which is what made an occasional turn cost
/// hundreds of milliseconds rather than single digits.
///
/// A query parameter rather than a separate route, unlike `get_thumb`: a tile
/// is a different *thing* — always JPEG, cropped for a strip, no validator —
/// where this is the same page at a different size, wanting the same
/// `If-None-Match` fast path and the same freshness. Absent `w` is the original
/// bytes, byte for byte, which is what `zoom` asks for and what every client
/// written before this parameter existed keeps getting.
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
    if let Some(refusal) = refuse_if_hidden(&state, &request_extensions, &id, &edition) {
        return refusal;
    }
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
    // and that filter is load-bearing rather than a shortcut. The sentence that
    // used to be here — "`fit_at` would answer `None` for them anyway once no
    // real page is that small" — had the condition backwards: `fit_at` answers
    // `None` when the page is *already* within the requested edge, so at `w=1`
    // it answers `Some` and re-encodes. Without this filter `?w=1` hands a
    // reader a one-pixel JPEG rather than the page.
    // `a_width_of_one_is_absent_rather_than_a_one_pixel_page` is the witness.
    let long_edge = query
        .w
        .filter(|edge| *edge > 1)
        .map(|edge| edge.min(crate::cover::MAX_FIT_LONG_EDGE));

    // O1: the validator comes from what the sidecar already holds in
    // memory — the member's recorded `sha256` and this page's offset within
    // it — so it can be checked here, on the async worker, before
    // `spawn_blocking` below ever runs. A matching `If-None-Match` answers
    // `304` without a zip decompression or an HTTP round trip to Stump ever
    // happening — the whole point of sending a validator in the first
    // place.
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    let (sha256, offset) = match locate_member(found, &edition, index) {
        Ok(pair) => pair,
        Err(message) => return not_found(message),
    };
    let tag = page_etag(&sha256, offset, long_edge);
    if if_none_match_hits(&headers, &tag) {
        return not_modified(&tag, Some(PAGE_CACHE_CONTROL));
    }

    // Reading a page blocks: a zip decompression from disk, or an HTTP round
    // trip to Stump. On a tokio worker that stalls the whole worker, and this
    // corpus holds pages of 430 MB — with the prefetching the filmstrip and
    // swap both need, one reader would occupy several of them.
    //
    // Resizing blocks harder still — it decodes — so it belongs on this same
    // thread rather than in a second `spawn_blocking` behind it. A cache hit
    // never reaches the fetch at all: `cached_or_generated_page` reads the
    // cached file first, and only a miss pays for the member.
    //
    // `PageSource` stays synchronous deliberately. The boundary belongs here,
    // where there is exactly one of it, rather than in each implementation.
    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id)?;
        let Some(edge) = long_edge else {
            return state.page(found, &edition, index);
        };
        let dir = state.thumb_cache_dir();
        let fitted = crate::cover::cached_or_generated_page(&dir, &sha256, offset, edge, || {
            state.page(found, &edition, index).map(|p| p.bytes)
        })?;
        match fitted {
            // `fit_at` always writes JPEG, and says so rather than echoing
            // whatever the archive held.
            Some(bytes) => Ok(crate::source::ServedPage {
                bytes,
                content_type: "image/jpeg".to_string(),
            }),
            // Already within the requested edge: the original bytes are the
            // right answer, so this costs one header parse and no resize.
            None => state.page(found, &edition, index),
        }
    })
    .await;

    match fetched {
        Ok(Ok(page)) => (
            [
                (header::CONTENT_TYPE, page.content_type),
                (header::ETAG, tag),
                (header::CACHE_CONTROL, PAGE_CACHE_CONTROL.to_string()),
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

/// A filmstrip tile for one page.
///
/// Separate from `get_page` rather than a query parameter on it, because the
/// two have different cache lifetimes and different content types: a page is
/// whatever the archive holds, a tile is always JPEG.
///
/// It sends `Cache-Control` and **no `ETag`**, which is not an oversight.
/// `get_page` can answer a `304` before doing any work because
/// `locate_member` derives the tag from the in-memory sidecar; here the
/// member `sha256` and offset are found inside `spawn_blocking`, one level in,
/// so a validator would arrive too late to skip anything a tile costs. The
/// freshness header removes the round trip without needing one; adding the
/// tag would mean hoisting the lookup out here, which is its own change and
/// buys only a cheaper request after this header has expired.
async fn get_thumb(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition, raw)): AxPath<(String, String, String)>,
    request_extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = refuse_if_hidden(&state, &request_extensions, &id, &edition) {
        return refusal;
    }
    let index = match page_index(&raw, &edition) {
        Ok(index) => index,
        Err(refusal) => return refusal.into_response(),
    };
    // Decoding blocks and can allocate up to `MAX_DECODE_ALLOC`; `get_page`'s
    // reasoning applies unchanged.
    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id)?;
        let sidecar = found.sidecar();
        let ed = sidecar
            .editions
            .get(&edition)
            .ok_or_else(|| format!("no edition `{edition}` in work `{id}`"))?;
        let (member, offset) = ed
            .locate(index)
            .ok_or_else(|| format!("`{edition}` has no page {index} in work `{id}`"))?;
        let sha = ed.members[member].sha256.clone();
        let dir = state.thumb_cache_dir();
        crate::cover::cached_or_generated_thumb(&dir, &sha, offset, || {
            state.page(found, &edition, index).map(|p| p.bytes)
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
/// `NoProgress`'s split, for the same reason: a work this server does not
/// hold is the client's mistake and a 404; a notes file that will not read or
/// write is this server's and a 500. The write path adds a third: a
/// precondition answer, which carries its own status because 428 ("you sent no
/// validator") and 412 ("yours is stale") are genuinely different answers and
/// a client retries differently for each — folding them into one status would
/// send a client that has not entered a race looking for a conflict.
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

/// Every note on a work.
///
/// **A work with no notes file answers an empty document rather than a 204**,
/// which is where this deliberately parts company with `get_progress` beside
/// it. Writing notes requires `If-Match`, and a client that had never seen a
/// document would have no validator to send for its first write — so it would
/// need either a second precondition spelling for creation, or an
/// unconditional first write, which is exactly the lost-update hole the header
/// exists to close, left open for the one request a second device is most
/// likely to be racing. Synthesising the document here means the first write
/// is an ordinary conditional write like every later one.
///
/// Absent and empty are collapsed here rather than in `notes::load`, which
/// reports the file honestly: a reader with no notes and a reader who deleted
/// their last note are in the same state, and nothing downstream benefits from
/// telling them apart.
/// The reader this request is, taken from the [`crate::auth::Principal`] the
/// guard resolved.
///
/// **A `Result` rather than an `Option`, and never a default.** Falling back to
/// a user id when the extension is absent is how the two `LOCAL_USER = 1`
/// constants this replaced came to answer for everybody: a default that is right
/// for one reader is silently wrong for the second. The extension is present on
/// every guarded route by construction, so its absence is a routing mistake, and
/// a 500 naming it is the honest answer.
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

async fn get_notes(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    // A file read, on the same disk a page comes from and behind the same NFS
    // mount — `get_page`'s reasoning about the async worker applies unchanged
    // (O12).
    let user_id = match reader(&extensions) {
        Ok(id) => id,
        Err(missing) => return missing.into_response(),
    };
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoNotes::NotFound)?;
        let notes = crate::notes::load(&state.library, found, user_id)
            .map_err(NoNotes::Failed)?
            .unwrap_or_else(|| crate::notes::empty(&found.id));
        crate::notes::to_json(&notes).map_err(NoNotes::Failed)
    })
    .await;

    match loaded {
        Ok(Ok(json)) => {
            let tag = notes_etag(&json);
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

    // Serialising, hashing and writing all block, and the read-compare-write
    // below holds an exclusive lock across a file write — `get_page`'s
    // reasoning about the async worker applies unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoNotes::NotFound)?;

        // Held across read, compare and write. Two requests that both read the
        // same document and both find their validator current would otherwise
        // both write, and one reader's note would vanish with nothing saying
        // so — see `Work::notes_guard`.
        let _guard = found.notes_guard();

        let current = crate::notes::load(&state.library, found, user_id)
            .map_err(NoNotes::Failed)?
            .unwrap_or_else(|| crate::notes::empty(&found.id));
        let tag = notes_etag(&crate::notes::to_json(&current).map_err(NoNotes::Failed)?);
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

        // Validated against what is stored, and therefore inside the guard
        // rather than ahead of it: an anchor that has not moved since the last
        // write is not re-checked, which is what stops one note left pointing
        // at an edition a re-solve renamed from refusing every write that
        // follows it — including the one that would remove it. See
        // `notes::validate`.
        crate::notes::validate(&incoming, found, &current).map_err(|why| match why {
            crate::notes::Invalid::Missing(message) => NoNotes::NotFound(message),
            crate::notes::Invalid::Refused(message) => NoNotes::Refused(message),
        })?;

        crate::notes::store(&state.library, found, user_id, &incoming).map_err(NoNotes::Failed)?;
        crate::notes::to_json(&incoming).map_err(NoNotes::Failed)
    })
    .await;

    match written {
        Ok(Ok(json)) => {
            let tag = notes_etag(&json);
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
            format!("notes write panicked: {e}"),
        )
            .into_response(),
    }
}

/// Why a truth request could not be answered.
///
/// `NoNotes`'s split, minus `Refused`: nothing here validates an incoming
/// document against the sidecar the way `notes::validate` does — a truth
/// document names no slot and no edition list to check a page against — so
/// there is no third kind of client mistake to tell apart from a missing
/// work.
#[derive(Debug)]
enum NoTruth {
    NotFound(String),
    Precondition(StatusCode, String),
    Failed(String),
}

impl IntoResponse for NoTruth {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => not_found(message),
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
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    // A file read, on the same disk a page comes from and behind the same NFS
    // mount — `get_page`'s reasoning about the async worker applies unchanged
    // (O12).
    let loaded = tokio::task::spawn_blocking(move || -> Result<String, NoTruth> {
        let found = state.work_or_missing(&id).map_err(NoTruth::NotFound)?;
        let doc = crate::truth::load(&state.library, found)
            .map_err(NoTruth::Failed)?
            .unwrap_or_default();
        Ok(crate::truth::to_json(&doc))
    })
    .await;

    match loaded {
        Ok(Ok(json)) => {
            let tag = notes_etag(&json);
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

    // Serialising, hashing and writing all block, and the read-compare-write
    // below holds an exclusive lock across a file write — `get_page`'s
    // reasoning about the async worker applies unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoTruth::NotFound)?;

        // Held across read, compare and write, on the same mutex
        // `put_notes` holds for the same reason — `Work` has one
        // read-modify-write guard per work, not one per document kind, and
        // a truth write racing a notes write on the same work is exactly the
        // case it exists to exclude. Two requests that both read the same
        // document and both find their validator current would otherwise
        // both write, and one labeller's session would vanish with nothing
        // saying so.
        let _guard = found.notes_guard();

        let current = crate::truth::load(&state.library, found)
            .map_err(NoTruth::Failed)?
            .unwrap_or_default();
        let tag = notes_etag(&crate::truth::to_json(&current));
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

        crate::truth::store(&state.library, found, &incoming).map_err(NoTruth::Failed)?;
        Ok(crate::truth::to_json(&incoming))
    })
    .await;

    match written {
        Ok(Ok(json)) => {
            let tag = notes_etag(&json);
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

    // O12: a file read, on the same disk a page comes from and behind the same
    // NFS mount. `get_page`'s reasoning above applies to it unchanged — these
    // two handlers simply did not get it when M2 wrote them.
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoProgress::NotFound)?;
        crate::progress::load(&state.library, found, user_id).map_err(NoProgress::Failed)
    })
    .await;

    match loaded {
        // Nothing recorded is not an error: a work never opened has no
        // position, and 404 would say the work does not exist. An empty set
        // is that same fact, and it stays a status code rather than becoming
        // a document a client has to unpack.
        Ok(Ok(positions)) if positions.is_empty() => StatusCode::NO_CONTENT.into_response(),
        // Every edition the reader holds a position in, newest first —
        // `crate::progress::load`'s own promise, unchanged here.
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

    // Validation moves in here with the write rather than staying outside it.
    // It reads the in-memory sidecar and blocks on nothing, but splitting the
    // handler across the boundary would mean looking the work up twice — once
    // to validate and once inside the closure, which cannot borrow across it.
    let stored = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoProgress::NotFound)?;
        let sidecar = found.sidecar();
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
        if progress.slot >= slots {
            return Err(NoProgress::NotFound(format!(
                "`{}` has {slots} slots in work `{id}`; there is no slot {}",
                progress.edition, progress.slot
            )));
        }
        crate::progress::store(&state.library, found, user_id, &progress)
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
/// omission: a client assembling this body from a partial object, or one
/// written before the field existed, would otherwise unpair a page and be told
/// `200`. `deny_unknown_fields` closes a misspelled or version-skewed field
/// name.
///
/// On `b` specifically the two now say the same thing — a typo'd `"bb"` leaves
/// `b` absent, so requiring it refuses that request even with the attribute
/// removed (measured: the sabotage answers `400`, just with the other
/// message). The attribute still earns its place on the fields where nothing
/// else would notice: a mistyped `note` is not destructive, but it would be
/// silently discarded, and a reader who wrote down why they made a correction
/// should not lose it to a typo in a key.
///
/// The cost is one key on the wire. This is not the OPDS surface — that is
/// `/opds/v1.2/…`, Atom, and read-only, and it is what third-party readers
/// use; this is taiyaku's own JSON API, and its only client today is `web/`,
/// whose `CorrectionRequest` already types `b` as required-and-nullable.
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
    /// Measured, not assumed: dropping `#[serde(default)]` and re-running the
    /// status table left `{"from":…,"to":…,"a":"2"}` answering `200` and
    /// unpairing the slot.
    ///
    /// `Pairing` is what the *present* value means; the outer `Option` here is
    /// still "present or not", filled by `#[serde(default)]` only when the key
    /// is absent. `present` still has to force the wrapper past a plain
    /// `Option`'s null-absorbing `Deserialize` — that absorption is exactly
    /// what would make `"b": null` and an absent `b` indistinguishable again,
    /// now one level up: measured the same way, `Option<Pairing>` without
    /// `deserialize_with` answers `an_explicit_null_unpairs_the_slot`'s `"b":
    /// null` with the *absent-b* refusal, because `Option<Pairing>::deserialize`
    /// consumes the `null` and reports `None` before `Pairing::deserialize`
    /// ever runs.
    #[serde(default, deserialize_with = "present")]
    b: Option<Pairing>,
    #[serde(default)]
    note: Option<String>,
}

/// What a present `b` asks for: `null` versus a slot. `Correction::b`'s outer
/// `Option` is presence, filled by `#[serde(default)]`; this is the other
/// axis, the one that used to be a second, anonymous `Option`.
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
/// **Took no `Extensions` and consulted no grant at all until this joined the
/// other six.** It resolved its target by slug alone
/// (`state.work_or_missing`), so a restricted account could both read the
/// sidecar this handler returns and write shared `alignment`/`run` rows for a
/// work outside every root it held — worse than the read leaks the sibling
/// routes closed, because an alignment is shared library truth rather than
/// per-account state, and O60's "nothing leaks" reasoning for progress, notes
/// and truth does not carry to it.
///
/// **Filters its response exactly as `get_sidecar` does, and for the same
/// reason — this was declared a 404-only lookup and was not one.** This
/// route sits on the same [`visible_editions_or_refuse`] helper `get_sidecar`
/// uses: a reader granted only `en` on a work also holding a withheld `raw`
/// passes it — the work keeps a visible edition — and until this fix got
/// back a 200 carrying the *whole* corrected sidecar, `raw` included. The
/// fix filters the response the same way.
///
/// **Refuses a correction that itself names a withheld edition, on either
/// side.** The read-leak fix above shaped only what came back; it left the
/// write open — a reader granted `en` alone could still submit
/// `{"from":"en","to":"raw",…}` and have it accepted, writing shared
/// `alignment`/`run` rows for `raw` that every reader who *can* see `raw`
/// is then served from. That is worse than the response leak this function
/// closed first: a read leak costs the caller who triggered it a fact they
/// should not have; this write would have cost every other reader a fact
/// they did not ask this caller to assert. Closed with two calls to
/// [`refuse_if_hidden`] — the exact per-edition question `/pages`, `/thumb`
/// and the download routes already ask, reused rather than a third spelling
/// of it — before the body is otherwise parsed.
///
/// **What is stored is never filtered — only what is returned.** `visible`
/// is read once, before the write, and used solely to shape the HTTP
/// response; the file and the `alignment`/`run` rows below always hold the
/// complete, unfiltered document, because a correction is shared library
/// truth and a caller-specific projection of it must never reach disk. The
/// `refuse_if_hidden` calls above are what keep an invisible edition from
/// reaching that write at all, rather than a filter on it afterward.
///
/// `#[allow(clippy::result_large_err)]` because everything under this
/// handler — `one_slot`'s calls, the `spawn_blocking` closure and the
/// `Work::update` closure nested inside it — resolves a `Response` as its
/// error type, and clippy checks each closure as its own item (`works.rs`'s
/// `a_replaced_sidecar_is_what_the_next_reader_sees` says the same thing at
/// its own call site).
#[allow(clippy::result_large_err)]
async fn patch_alignments(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    request_extensions: axum::http::Extensions,
    body: axum::body::Bytes,
) -> Response {
    let visible = match visible_editions_or_refuse(&state, &request_extensions, &id) {
        Ok(visible) => visible,
        Err(refusal) => return refusal,
    };
    let correction: Correction = match serde_json::from_slice(&body) {
        Ok(correction) => correction,
        Err(why) => return bad_request(format!("this is not a correction: {why}")),
    };
    // **The write-authorization half of Finding 1, not merely its response
    // leak.** `visible` above only shapes what this handler sends back; it
    // was read once and never asked whether the correction's own `from`/`to`
    // are inside the grant. A reader granted only `en` on a work that also
    // holds a withheld `raw` could name `raw` on either side and have it
    // written into the shared `alignment`/`run` rows every other reader of
    // this work is served from — worse than a read leak, because the write
    // is not scoped to the caller who made it. `refuse_if_hidden` is the
    // exact per-edition question `/pages`, `/thumb` and the download routes
    // already ask; reused here rather than a second spelling of it.
    if let Some(refusal) = refuse_if_hidden(&state, &request_extensions, &id, &correction.from) {
        return refusal;
    }
    if let Some(refusal) = refuse_if_hidden(&state, &request_extensions, &id, &correction.to) {
        return refusal;
    }
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
    // below holds an exclusive lock across the file write — `get_page`'s
    // reasoning about the async worker applies to it unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(not_found)?;
        // **The library's connection before the work's document, never the
        // other way round.** `list_works` holds this mutex across a
        // `work.sidecar()` read, so a thread that took the sidecar's write
        // lock first and reached for the mutex second would deadlock against
        // it — and the `spawn_blocking` pool is where both of them run. It is
        // held across the file write below, which is the cost of that order:
        // one correction — a person dragging a filmstrip tile — briefly
        // blocks the other library readers.
        let conn = state.library.conn();
        found.update(|current| {
            let tag = sidecar_etag(current);
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

            // The file, then the rows, then memory. A failed write leaves the
            // served document exactly as it was, so a client that gets a 500
            // has lost its correction and nothing else — where swapping memory
            // first would leave this server serving an alignment that is on no
            // disk anywhere.
            //
            // The rows are what every *other* request is answered from
            // (`taiyaku serve` projects each `Work` out of them at start), so
            // writing only the file left the two disagreeing until the next
            // start re-imported it — O43. Between them, the file is still
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
                    taiyaku_read::atomic::write(&found.path, json.as_bytes(), "json")
                        .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())?;
                }
                crate::alignments::store(&conn, found, &next)
                    .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())?;
            }
            // The response is filtered to this caller's grant, the same
            // question `get_sidecar` answers with the same helper — `json`
            // above stays the full document that was stored.
            let response_json = match &visible {
                Some(keys) => filter_sidecar_editions(&next, keys).to_json(),
                None => json,
            };

            let next = std::sync::Arc::new(next);
            let tag = sidecar_etag(&next);
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

/// Build the router. Separate from `serve` so tests can drive it with
/// `oneshot` and never open a socket.
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
    // in the other list would ship unauthenticated with a green suite —
    // `every_route_refuses_an_anonymous_request` derives its count from both
    // lists and is what catches that.
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
        // _refuses_a_reader` drives all six of these for a 403.
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
            axum::routing::post(crate::curation::dismiss),
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
        // **Spec C4 §6's rule-editor routes, task 8 of the branch plan.**
        // Admin-gated the same way the curation and metadata blocks above
        // are, and for the identical reason stated on each of theirs: a
        // route that writes cannot afford to land in `open_routes`.
        // `config_api::tests::every_config_api_route_refuses_a_reader`
        // drives all six for a 403.
        (
            "/api/library/rules",
            get(crate::config_api::list_rules).post(crate::config_api::create_rule),
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
        // **Spec C4 §2's roots routes, task 11 of the branch plan.**
        // Admin-gated the same way the rules block above is, and for the
        // identical reason stated on each of theirs: a route that adds,
        // edits or destroys a scan root cannot afford to land in
        // `open_routes`. `config_api::tests::every_config_api_route_refuses
        // _a_reader` (task 8's own hand-typed list, not an enumeration — see
        // its own doc comment) is extended to drive all four of these too.
        (
            "/api/library/roots",
            get(crate::config_api::list_roots).post(crate::config_api::create_root),
        ),
        (
            "/api/library/roots/{id}",
            axum::routing::patch(crate::config_api::patch_root)
                .delete(crate::config_api::delete_root),
        ),
        // **Spec C4 §3's scan-control route, task 12 of the branch plan.**
        // Admin-gated like every other route in this block, and for the
        // identical reason: the server has never scanned before this task,
        // and the trigger that lets it start one is exactly as destructive a
        // capability as adding or removing a root above. `POST` starts a scan
        // — refusing a second one already running against the same root,
        // 409, rather than queuing it — and `GET` reads the registry
        // `ServeState.scans` holds for it: live progress while the spawned
        // task is still walking, the finished report once it is done.
        (
            "/api/library/roots/{id}/scan",
            get(crate::config_api::scan_status).post(crate::config_api::start_scan),
        ),
    ]);
    routes.extend(crate::opds::routes());
    routes
}

/// Why a guarded route may or may not read [`crate::auth::Principal::roots`],
/// and how that claim can be told apart from a hope.
///
/// **Beside [`guarded_routes`] rather than in a test file**, for the reason
/// [`open_routes`] sits here too: the lists are read together, and
/// `auth::tests::every_route_declares_its_grant_posture` asserts this table's
/// route set against `guarded_routes`'s exactly — so a route added to one
/// without the other fails the build on the commit that adds it.
/// [`guarded_routes`]'s own doc comment makes this argument for
/// authentication: "that is exactly the failure a reviewer's checklist has:
/// it goes stale the first time someone adds a route and does not think
/// about auth." Authorization had no such test until this one, and six
/// routes — `stream_page`, `cover`, `work_feed` and `works` in `opds.rs`,
/// `list_works` in this file, and `patch_alignments` — drifted out of the
/// grant one at a time, over more than one spec, before anything said so.
///
/// `#[cfg(test)]`, like `test_support` itself: this table has no production
/// caller — its whole job is being checked against `guarded_routes` by
/// `auth::tests::every_route_declares_its_grant_posture` — and leaving it
/// compiled into the production binary would make it dead code there,
/// failing this crate's `-D warnings` clippy gate on the plain `--lib`
/// target.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum GrantPosture {
    /// A lookup, not a list: a work or edition the grant hides must answer
    /// 404 — `browse::work`'s or `browse::edition_visible`'s own contract,
    /// answering a hidden work exactly as it answers one that does not
    /// exist.
    Refuses,
    /// A list: the route answers 200, and material outside the grant must
    /// simply never be named in the body.
    Filters,
    /// **Both of the above, for one route with two distinct grant-facing
    /// jobs.** A single posture per route was the table's own premise until
    /// `/api/works/{work}/alignments` broke it: `patch_alignments` refuses a
    /// request that itself *names* a withheld edition (`Refuses`, on the
    /// write) and separately filters the response body of a request that
    /// does not (`Filters`, on the read) — two different questions asked of
    /// two different parts of one request, neither of which subsumes the
    /// other. Recording it as bare `Refuses` was true but incomplete: it
    /// said nothing about the response, which is exactly why the response
    /// filter went unproven — `every_route_declares_its_grant_posture`'s
    /// `Filters` completeness check has no reason to look at a route the
    /// table calls `Refuses`. A second table (refuse-postures crossed with
    /// filter-postures) was rejected: every other route needs only one axis,
    /// so a second table would carry an empty column for all but this one
    /// entry. A combined variant costs one arm in each match already written
    /// for `Refuses` and `Filters` and states the truth in one place.
    RefusesAndFilters,
    /// The grant cannot apply — `reason` says why, stated per entry rather
    /// than inherited, the same rule [`open_routes`] holds its own list to.
    GrantIndependent { reason: &'static str },
    /// **Not one of the three above, and deliberately so.** Reserved for a
    /// route that is neither: shared library material, the way an alignment
    /// or a sidecar is, so `GrantIndependent` would misstate it as being
    /// about the account rather than the library — but unenforced, so no
    /// request against it can produce the 404 `Refuses` would assert without
    /// the test asserting something false about what the code does today.
    /// `auth::tests::every_route_declares_its_grant_posture` does not drive a
    /// request for this case; it only checks the entry exists. See its own
    /// doc comment, and this branch's grant-enumeration report, for why.
    KnownGap {
        open_item: &'static str,
        reason: &'static str,
    },
}

/// The table [`GrantPosture`]'s own doc comment describes: every entry in
/// [`guarded_routes`], in the same order, classified by how it must answer a
/// caller whose grant does not cover the material a request names.
///
/// # `/api/works/{work}/truth` is not `Refuses`, and this is not an oversight
///
/// `get_truth`/`put_truth` take no `Extensions` at all — unlike every other
/// route in this table — so they cannot consult `Principal` under any
/// circumstance, and a truth document is shared library material rather than
/// the per-account state O60 found "nothing leaks" for in progress and notes.
/// O60 already names this exact gap ("truth is the awkward one, since it is
/// shared rather than per-account") and defers it to spec C4's permission
/// model rather than "a sixth call to a helper." This table does not close
/// that decision — doing so was out of scope for the write leak this branch
/// closed in `patch_alignments` — but it also must not claim a safety
/// property the code does not have, which is what `KnownGap` is for.
#[cfg(test)]
pub(crate) fn grant_postures() -> Vec<(&'static str, GrantPosture)> {
    use GrantPosture::{Filters, GrantIndependent, KnownGap, Refuses, RefusesAndFilters};
    vec![
        ("/api/works", Filters),
        // Was `Refuses` until this table's proof was extended to a work
        // split across roots (Finding 2): `get_sidecar` refuses only a work
        // with *no* visible edition, and filters `editions`/`alignments`/
        // `hub` down to the visible set for one that has some — the same
        // `Filters` shape `/opds/v1.2/works/{work}` already declares below,
        // for the identical reason stated there.
        ("/api/works/{work}", Filters),
        ("/api/works/{work}/{edition}/pages/{index}", Refuses),
        ("/api/works/{work}/{edition}/pages/{index}/thumb", Refuses),
        ("/api/works/{work}/{edition}/download", Refuses),
        (
            "/api/works/{work}/{edition}/members/{ordinal}/download",
            Refuses,
        ),
        (
            "/api/works/{work}/progress",
            GrantIndependent {
                reason: "per-account state on a work, keyed by user_id — O60: \
                          \"nothing leaks: progress and notes are per-account, \
                          and a reader can only write their own.\" A \
                          restricted reader can create a position on a work \
                          outside their grant, but can only ever read or \
                          overwrite their own row, never another reader's or \
                          the library's shared material",
            },
        ),
        (
            "/api/works/{work}/notes",
            GrantIndependent {
                reason: "O60's identical reasoning for progress, applied to \
                          notes: per-account, keyed by user_id, and a reader \
                          can only ever act on their own",
            },
        ),
        (
            "/api/works/{work}/truth",
            KnownGap {
                open_item: "O60",
                reason: "shared library material, not per-account — O60's own \
                          words: \"truth is the awkward one, since it is \
                          shared rather than per-account\" — and unlike every \
                          Refuses entry, get_truth/put_truth take no \
                          Extensions at all, so no request against this route \
                          can currently produce a grant-driven 404 (get_truth \
                          still 404s an unknown slug through work_or_missing \
                          — the same as every other route — it is only the \
                          grant check that is absent); asserting Refuses here \
                          would encode a false safety claim into the suite. \
                          Reported, not fixed here — see this branch's \
                          grant-enumeration report",
            },
        ),
        // Was briefly `Filters`, for `/api/works/{work}`'s exact reason: the
        // response-leak fix round put `patch_alignments` on the same
        // `visible_editions_or_refuse` helper `get_sidecar` uses, and a
        // correction on a work split across roots answered 200 with the
        // corrected sidecar filtered to the visible editions.
        //
        // **Then `Refuses`, once the write itself was guarded — and that was
        // still only half true.** `visible_editions_or_refuse` only ever
        // shaped the *response*; it never asked whether the correction's own
        // `from`/`to` were inside the grant, so a reader granted `en` alone
        // could still write `alignment`/`run` rows for `raw` — shared state
        // every reader who *can* see `raw` is then served from, which is
        // worse than the read leak the response-only fix closed. Two
        // `refuse_if_hidden` calls now refuse such a correction outright
        // (404) before `Sidecar::correct` or the write beneath it ever runs
        // — but the response-filtering half never stopped being true, and
        // marking this bare `Refuses` said it had: `Filters`'s completeness
        // check in `every_route_declares_its_grant_posture` only walks
        // routes the table calls `Filters`, so the response filter on a
        // correction between two *visible* editions went unproven for the
        // whole gap between the two fix rounds — a verification pass caught
        // it (Finding 1/A in this branch's report) with a mutation that
        // passed all 371 tests that existed before it.
        //
        // `RefusesAndFilters` now: the write half is proven in
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
        // Were `Refuses` until Finding 2's split-fixture proof caught the
        // mislabelling: all three already reuse `browse::work`/`one_series`/
        // `cover_work`, which filter to visible editions and 404 only when
        // *none* survive — a work split across roots gets 200 with `en`
        // alone, not a 404. The underlying code was never the bug here
        // (unlike `routes.rs`'s three); only the table's claim about it was
        // wrong, and a wrong claim is exactly what this table exists to
        // catch.
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
                          and no grant narrows which ones exist",
            },
        ),
        // **Spec C3 §5's metadata-editing routes — `Refuses`, not
        // `GrantIndependent` (I8, whole-branch review).** These four were
        // first classified `GrantIndependent` on the argument the comment
        // used to make here: a title, a series' titles, an edition's label
        // or a member's chapter number is a fact about the library's
        // organisation, not about what any one account may see — true of
        // *who may act on the answer* (still admin-only, unconditionally),
        // false of *which rows an administrator's own restricted grant
        // resolves through*. `library_api::work`/`one_series` already
        // resolve their slug/id through `principal.roots`, so a hidden work
        // was a 404 on `GET` and 200 — with the full re-projected document,
        // series titles, tags, external ids, credits, chapter numbers
        // included — on `PATCH` of the identical id. `metadata::work_by_slug`,
        // `edition_owner`, `member_owner` and `works_in_series` now take
        // `roots` and answer the same 404 a `GET` already does; membership
        // in an administrator's *account* (`Role::Admin`) is unconditional
        // and orthogonal to `Principal.roots`, which is why "admin-only" and
        // "resolves through the grant" are not in tension.
        //
        // **That last sentence stopped being true with O86, and these four
        // entries moved from `Refuses` to `GrantIndependent` because of it.**
        // The two are no longer orthogonal: `Principal::from_account` drops an
        // administrator's grant outright, so every caller these routes admit
        // holds an empty — unrestricted — one, and there is no hidden work for
        // them to refuse. The 404 the entries used to assert was driven by a
        // fixture that granted a root to the seeded *admin*, which is exactly
        // the combination O86 made unrepresentable. **The check is void rather
        // than lost**: it proved the answer for a state the model no longer
        // has.
        //
        // `work_by_slug`, `edition_owner`, `member_owner` and `works_in_series`
        // still take `roots` and still filter on it, kept deliberately: it is
        // correct for any caller with a non-empty grant, and the day the role
        // rule changes it is the difference between a leak and none.
        //
        // **Not "unreachable" — they run on every edit.** What no longer
        // reaches them through these routes is a *non-empty* `roots`, so the
        // clause is always a no-op here. The distinction is not pedantry: a
        // sweep the day this landed reported `metadata::root_grant_clause ->
        // String::new()` as a **survivor**, because deleting the entire grant
        // filter passed every test. `metadata::tests::
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
                          /api/library/proposals holds above",
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
        // Spec C4 §2's roots routes, task 11 of the branch plan. Every entry
        // states its own reason rather than inheriting a shared one, the
        // same rule the rules block above holds this table to.
        (
            "/api/library/roots",
            GrantIndependent {
                reason: "admin-only; lists and adds scan roots, which are a \
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
        // Spec C4 §3's scan-control route, task 12 of the branch plan.
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
            "/opds/v1.2/catalog",
            GrantIndependent {
                reason: "one static navigation entry naming no work at all",
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
        // the same `refuse_if_hidden`, so necessarily the same posture.
        // Driven rather than declared: `grant_proof_concrete` substitutes
        // `{work}` and `{edition}`, so the `Refuses` loops pick this up
        // against both a wholly-hidden work and one split across roots with
        // no further work here.
        ("/opds/v1.2/works/{work}/{edition}/download", Refuses),
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
/// set one itself. See the call site in [`build_router`].
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

pub fn build_router(state: Arc<ServeState>) -> Router {
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
        // **A default, not eight call sites.** Every guarded response is
        // per-reader unless it says otherwise, and the first attempt at this
        // added the header at each place a per-reader document was built — which
        // reached two of eight. `get_notes`, the route the constant's own doc
        // named first, was not one of them, and neither were the OPDS feeds,
        // which additionally get no `Set-Cookie` and so lack even the accident
        // that makes most proxies decline to cache an authenticated response.
        //
        // A response that has already set `Cache-Control` keeps it: the page and
        // thumbnail routes state their own `private, no-cache` with a lifetime
        // and mean it. Everything else gets `private, no-store`, so a proxy —
        // which spec A2 §11 puts in front — cannot store one reader's notes,
        // position or catalogue and replay them to another. RFC 9111's bar on
        // caching authenticated responses keys on `Authorization`, which the
        // browser cookie path never sends. Found by the third whole-branch
        // review.
        .layer(axum::middleware::from_fn(per_reader_by_default));

    // Merged *after* the guard is applied, which is the whole reason these two
    // are a separate list: `route_layer` wraps the routes registered before it,
    // so anything merged afterwards is outside it. That is a sharp edge — a
    // route added to the wrong list is silently open.
    //
    // **Two tests stop that being invisible, and for one commit only one of
    // them existed.** `every_route_refuses_an_anonymous_request` walks
    // `guarded_routes` and derives its own count from both lists, so a route
    // that leaves the guarded one fails it; `open_routes_are_the_ones_that_cannot_be_guarded`
    // pins the exemption list itself and proves each entry is reachable
    // anonymously. Before the second, nothing read this list at all — a route
    // moved here shipped unauthenticated with a green suite, which the
    // whole-branch review caught on 2026-08-18.
    let open = open_routes()
        .into_iter()
        .fold(Router::new(), |router, (path, handler)| {
            router.route(path, handler)
        });

    guarded
        .merge(open)
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let log = log.clone();
                async move {
                    let Some(log) = log else {
                        return next.run(request).await;
                    };
                    let method = request.method().to_string();
                    // Path *and* query string: a client asking for
                    // `?width=1536` is invisible if only `path()` is
                    // logged, and this server's total absence of resize
                    // support makes that more important to catch, not less.
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
