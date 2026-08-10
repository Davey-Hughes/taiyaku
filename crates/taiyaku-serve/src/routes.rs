//! HTTP. Everything interesting happens below this file.

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
fn not_found(message: String) -> Response {
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
/// One day is chosen from two ends rather than measured: long enough that a
/// work read across an evening and reopened the next morning costs no round
/// trip, short enough that a replaced archive corrects itself without anyone
/// knowing to clear anything. Any value from an hour to a week satisfies both.
pub(crate) const PAGE_CACHE_CONTROL: &str = "private, max-age=86400";

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
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

/// A notes document's `ETag`: a SHA-256 over exactly the bytes being sent.
///
/// Takes the serialised text rather than the document, unlike `sidecar_etag`
/// above, because both callers already hold it — `notes::to_json` is the one
/// serialisation of a notes document, and hashing a second one built here
/// would be the drift this exists to rule out.
pub(crate) fn notes_etag(json: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

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

async fn list_works(State(state): State<Arc<ServeState>>) -> Response {
    let ids: Vec<&str> = state.works.iter().map(|w| w.id.as_str()).collect();
    axum::Json(ids).into_response()
}

async fn get_sidecar(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    let sidecar = found.sidecar();
    let tag = sidecar_etag(&sidecar);
    if if_none_match_hits(&headers, &tag) {
        return not_modified(&tag, None);
    }
    // Served verbatim from the parsed document rather than by re-reading the
    // file, so what the client resolves against is exactly what this server
    // holds — and exactly what the `ETag` above was computed over.
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, tag.as_str()),
        ],
        sidecar.to_json(),
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
) -> Response {
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
) -> Response {
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
async fn get_notes(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    // A file read, on the same disk a page comes from and behind the same NFS
    // mount — `get_page`'s reasoning about the async worker applies unchanged
    // (O12).
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoNotes::NotFound)?;
        let notes = crate::notes::load(found)
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

        let current = crate::notes::load(found)
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

        crate::notes::store(found, &incoming).map_err(NoNotes::Failed)?;
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

async fn get_progress(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
) -> Response {
    // O12: a file read, on the same disk a page comes from and behind the same
    // NFS mount. `get_page`'s reasoning above applies to it unchanged — these
    // two handlers simply did not get it when M2 wrote them.
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoProgress::NotFound)?;
        crate::progress::load(found).map_err(NoProgress::Failed)
    })
    .await;

    match loaded {
        // Nothing recorded is not an error: a work never opened has no
        // position, and 404 would say the work does not exist.
        Ok(Ok(None)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(Some(p))) => axum::Json(p).into_response(),
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
    AxPath(id): AxPath<String>,
    axum::Json(progress): axum::Json<crate::Progress>,
) -> Response {
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
        let slots = edition.slots(sidecar.work.reading).len() as u32;
        if progress.slot >= slots {
            return Err(NoProgress::NotFound(format!(
                "`{}` has {slots} slots in work `{id}`; there is no slot {}",
                progress.edition, progress.slot
            )));
        }
        crate::progress::store(found, &progress).map_err(NoProgress::Failed)
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
    /// The two `Option`s are what separate the cases: the outer one is
    /// "present or not", filled by `#[serde(default)]` only when the key is
    /// absent, and the inner one is "a slot or `null`". `present` is what
    /// forces the wrapper past a plain `Option`'s null-absorbing
    /// `Deserialize`.
    #[serde(default, deserialize_with = "present")]
    b: Option<Option<String>>,
    #[serde(default)]
    note: Option<String>,
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
    body: axum::body::Bytes,
) -> Response {
    let correction: Correction = match serde_json::from_slice(&body) {
        Ok(correction) => correction,
        Err(why) => return bad_request(format!("this is not a correction: {why}")),
    };
    let a = match one_slot(&correction.a, "a") {
        Ok(slot) => slot,
        Err(refusal) => return refusal,
    };
    // The outer `Option` is presence, the inner one is `null`. Absence is
    // refused here rather than in the derive, so the message names the field
    // and says what the two spellings mean — serde's own "missing field `b`"
    // would not say that an unpairing has to be asked for explicitly.
    let Some(b) = correction.b else {
        return bad_request(
            "a correction needs `b`: a slot of the target edition, or `null` to say this slot \
             pairs with nothing. Leaving it out is refused, because an unpairing is an edit and \
             has to be asked for."
                .to_string(),
        );
    };
    let b = match b.as_deref().map(|text| one_slot(text, "b")) {
        Some(Ok(slot)) => Some(slot),
        Some(Err(refusal)) => return refusal,
        None => None,
    };

    // Serialising, hashing and writing all block, and the read-modify-write
    // below holds an exclusive lock across the file write — `get_page`'s
    // reasoning about the async worker applies to it unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(not_found)?;
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
                    correction.note.clone(),
                )
                .map_err(|why| {
                    use taiyaku_core::NotCorrected::*;
                    let status = match why {
                        NoSuchEdition(_) | NoSuchSlot(_) | NoPath(_) => StatusCode::NOT_FOUND,
                        SameEdition(_) => StatusCode::BAD_REQUEST,
                        Unreadable(_) => StatusCode::CONFLICT,
                    };
                    (status, why.to_string()).into_response()
                })?;

            // The file first, then memory. A failed write leaves the served
            // document exactly as it was, so a client that gets a 500 has lost
            // its correction and nothing else — where swapping memory first
            // would leave this server serving an alignment that is on no disk
            // anywhere.
            let json = next.to_json();
            if what != taiyaku_core::Corrected::AlreadyTrue {
                crate::atomic::write(&found.path, json.as_bytes(), "json")
                    .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())?;
            }
            let next = std::sync::Arc::new(next);
            let tag = sidecar_etag(&next);
            Ok((next, (json, tag)))
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
pub fn build_router(state: Arc<ServeState>) -> Router {
    let log = state.log.clone();
    Router::new()
        .route("/api/works", get(list_works))
        .route("/api/works/{work}", get(get_sidecar))
        .route("/api/works/{work}/{edition}/pages/{index}", get(get_page))
        .route(
            "/api/works/{work}/{edition}/pages/{index}/thumb",
            get(get_thumb),
        )
        .route(
            "/api/works/{work}/progress",
            get(get_progress).put(put_progress),
        )
        .route("/api/works/{work}/notes", get(get_notes).put(put_notes))
        .route(
            "/api/works/{work}/alignments",
            axum::routing::patch(patch_alignments),
        )
        .merge(crate::opds::routes())
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
                        .map(|pq| pq.as_str())
                        .unwrap_or_else(|| request.uri().path())
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
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// The three shapes a reader depends on: the work list to find a work, the
    /// sidecar verbatim so the client can resolve without asking again, and a
    /// page by index. A 404 for an edition that does not exist matters as much
    /// as the successes — a reader that gets 200 and an empty body for a typo
    /// shows a blank page and says nothing.
    #[tokio::test]
    async fn the_reader_can_list_a_work_read_its_sidecar_and_fetch_a_page() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-routes"));
        let app = build_router(state);

        let list = app
            .clone()
            .oneshot(Request::get("/api/works").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);

        let sidecar = app
            .clone()
            .oneshot(Request::get("/api/works/t").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(sidecar.status(), StatusCode::OK);

        let page = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/raw/pages/2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(
            page.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/jpeg",
            "a page has to say what it is, or only an <img> can use it"
        );
        let bytes = axum::body::to_bytes(page.into_body(), 1 << 20)
            .await
            .unwrap();
        assert_eq!(&bytes[..], b"b1.jpg", "page 2 is the second member's first");

        let missing = app
            .oneshot(
                Request::get("/api/works/t/nope/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        crate::test_support::cleanup("taiyaku-test-routes");
    }

    /// `status.md` previously claimed no test dispatched an out-of-range
    /// page index through the router — true of the `/api/…` route
    /// specifically (the OPDS stream route had its own such test since Task
    /// 6), but the sentence did not say so and so overclaimed. This closes
    /// the actual gap: the fixture edition has 4 pages, so index 4 is one
    /// past the end.
    #[tokio::test]
    async fn an_out_of_range_page_index_dispatched_through_the_router_names_the_length() {
        let state = Arc::new(crate::test_support::state(
            "taiyaku-test-routes-page-past-end",
        ));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/t/raw/pages/4")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        let message = String::from_utf8_lossy(&body).to_string();

        crate::test_support::cleanup("taiyaku-test-routes-page-past-end");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(message.contains("raw"), "must name the edition: {message}");
        assert!(
            message.contains('4'),
            "must say how many pages it has: {message}"
        );
    }

    /// O5: the one failure on this route that did not say what was wrong.
    ///
    /// An index past the end names the edition and its length; an index that
    /// is not a number at all fell through to axum's `Path` rejection, which
    /// answers a 400 phrased in terms of the Rust type it failed to parse
    /// into. Comprehensible, and the only message in this server written by
    /// something other than this server.
    ///
    /// The status stays 400 rather than becoming the 404 the sibling failure
    /// gives, deliberately: `pages/4` names a page this edition does not have,
    /// and `pages/abc` does not name a page at all. Those are different
    /// things and a client is entitled to tell them apart. What they now share
    /// is that the message comes from here and says what was expected.
    #[tokio::test]
    async fn a_page_index_that_is_not_a_number_is_refused_in_this_server_s_own_words() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-routes-page-nan"));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/t/raw/pages/abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        let message = String::from_utf8_lossy(&body).to_string();

        crate::test_support::cleanup("taiyaku-test-routes-page-nan");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            message.contains("abc"),
            "must quote what was sent: {message}"
        );
        assert!(
            message.contains("raw"),
            "must name the edition it was sent to: {message}"
        );
        assert!(
            !message.contains("u32"),
            "must not be phrased in terms of a Rust type: {message}"
        );
    }

    /// A reader reopening a work asks one question and gets one answer, so a
    /// work never opened returns 204 rather than 404 — there is nothing wrong,
    /// there is simply nothing yet.
    #[tokio::test]
    async fn progress_round_trips_through_the_api() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-progress-api"));
        let app = build_router(state);

        let empty = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/progress")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(empty.status(), StatusCode::NO_CONTENT);

        let put = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/progress")
                    .header("content-type", "application/json")
                    // The fixture's `raw` edition is 4 pages and no spreads,
                    // so 3 is its last slot. It used to be 37, which named a
                    // position the work has never had — harmless while
                    // nothing checked, and a lie about what round-trips once
                    // something does.
                    .body(Body::from(r#"{"edition":"raw","slot":3}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::NO_CONTENT);

        let got = app
            .oneshot(
                Request::get("/api/works/t/progress")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(got.into_body(), 1 << 16)
            .await
            .unwrap();
        let back: crate::Progress = serde_json::from_slice(&bytes).unwrap();

        crate::test_support::cleanup("taiyaku-test-progress-api");
        assert_eq!(back.edition, "raw");
        assert_eq!(back.slot, 3);
    }

    /// O3: the edition check was only half the guard. A position naming a real
    /// edition and a slot past its end is stored garbage in the same way a
    /// position naming no edition is — the reader is sent back somewhere that
    /// cannot be resolved, and nothing says why. It is also the shape a bug in
    /// a client produces, which is exactly when a server should refuse rather
    /// than persist.
    #[tokio::test]
    async fn progress_past_the_end_of_the_edition_is_refused() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-progress-past-end"));
        let app = build_router(state);

        // 4 pages, no spreads, so 4 slots — 0 to 3. Slot 4 is one past.
        let put = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/progress")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"edition":"raw","slot":4}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = put.status();
        let body = axum::body::to_bytes(put.into_body(), 1 << 16)
            .await
            .unwrap();
        let message = String::from_utf8_lossy(&body).to_string();

        // Nothing was written, so reopening still says there is no position
        // rather than restoring one that cannot be resolved.
        let after = app
            .oneshot(
                Request::get("/api/works/t/progress")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-progress-past-end");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(message.contains("raw"), "must name the edition: {message}");
        assert!(
            message.contains('4'),
            "must say how many slots it has: {message}"
        );
        assert_eq!(after.status(), StatusCode::NO_CONTENT);
    }

    /// A position is recorded in **slots**, not pages — `progress.rs` says so
    /// and the design turns on it, because a page would lose which half of a
    /// joined spread was reached. So the range this checks against is the slot
    /// count, and an edition that joins spreads has *more* slots than pages:
    /// this one is 4 pages with page 0 a spread, so 5 slots.
    ///
    /// Range-checking against `total_pages()` would pass every other test in
    /// this file and refuse the last slot of every spread-carrying edition in
    /// the corpus — including One Piece v092, which is spreads throughout.
    // The fixture mutation below goes through `Work::update`, whose closure
    // is checked by clippy as its own item — see the same allow on
    // `works.rs`'s `a_replaced_sidecar_is_what_the_next_reader_sees`.
    #[allow(clippy::result_large_err)]
    #[tokio::test]
    async fn the_last_slot_of_a_spread_carrying_edition_is_in_range() {
        let state = crate::test_support::state("taiyaku-test-progress-spread-slot");
        state.works[0]
            .update(|current| {
                let mut next = (**current).clone();
                next.editions
                    .get_mut("raw")
                    .expect("the fixture defines `raw`")
                    .spreads = vec![0];
                Ok((std::sync::Arc::new(next), ()))
            })
            .expect("the fixture update always succeeds");
        let app = build_router(Arc::new(state));

        let put = app
            .oneshot(
                Request::put("/api/works/t/progress")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"edition":"raw","slot":4}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = put.status();
        let body = axum::body::to_bytes(put.into_body(), 1 << 16)
            .await
            .unwrap();
        let message = String::from_utf8_lossy(&body).to_string();

        crate::test_support::cleanup("taiyaku-test-progress-spread-slot");
        assert_eq!(
            status,
            StatusCode::NO_CONTENT,
            "4 pages with one spread is 5 slots, so slot 4 is the last one: {message}"
        );
    }

    /// O12: a progress read must not run on the async worker.
    ///
    /// `get_page` has handed its work to `spawn_blocking` since M2 and these
    /// two handlers did not, so a position read — a file open on the same disk
    /// a page comes from, behind the same NFS mount — stalled a runtime thread
    /// for its whole duration. With a `current_thread` runtime, which is what
    /// this test pins deliberately, "a runtime thread" is the whole runtime.
    ///
    /// Staged with a FIFO at the progress path, because opening one for
    /// reading blocks until a writer arrives — a real block on the real code
    /// path, rather than an injected one. If the read runs on the worker,
    /// nothing else on that runtime runs at all, so the canary task never gets
    /// scheduled; a watchdog OS thread then rescues the request and records
    /// that it had to, so the failure is an assertion rather than a hung test
    /// run.
    ///
    /// **The write path gets the same treatment and is not measured here.**
    /// `store` writes through a temp file whose name is unique per attempt
    /// (see `atomic::temp_path`), so there is no path a test can pre-create
    /// as a FIFO. It is covered by inspection and by sharing this one's
    /// shape.
    #[tokio::test(flavor = "current_thread")]
    async fn a_progress_read_does_not_stall_the_runtime() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let state = crate::test_support::state("taiyaku-test-progress-blocking");
        let fifo = crate::progress::progress_path(&state.works[0]);
        let made = std::process::Command::new("mkfifo").arg(&fifo).status();
        if !matches!(&made, Ok(status) if status.success()) {
            crate::test_support::cleanup("taiyaku-test-progress-blocking");
            eprintln!(
                "NOT MEASURED: a_progress_read_does_not_stall_the_runtime \
                 (mkfifo unavailable: {made:?})"
            );
            return;
        }
        let app = build_router(Arc::new(state));

        let canary_ran = Arc::new(AtomicBool::new(false));
        let rescued_by_deadline = Arc::new(AtomicBool::new(false));

        let watchdog = {
            let fifo = fifo.clone();
            let canary_ran = Arc::clone(&canary_ran);
            let rescued_by_deadline = Arc::clone(&rescued_by_deadline);
            std::thread::spawn(move || {
                // Half a second for a task on an idle runtime to be polled
                // once. Passing costs the first 10 ms of it; only a failure
                // waits the whole way out.
                let mut waited = 0;
                while !canary_ran.load(Ordering::SeqCst) && waited < 50 {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    waited += 1;
                }
                if !canary_ran.load(Ordering::SeqCst) {
                    rescued_by_deadline.store(true, Ordering::SeqCst);
                }
                // Read *and* write. Opening a FIFO for writing alone blocks
                // until a reader arrives, which would make this rescue the
                // hang it exists to prevent; `O_RDWR` never blocks. Held
                // briefly so a reader that has not opened yet still finds a
                // writer, then dropped — the drop is what gives the read its
                // EOF and lets the request finish.
                let end = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&fifo);
                std::thread::sleep(std::time::Duration::from_millis(50));
                drop(end);
            })
        };

        // Spawned before the canary, so the runtime polls it first: by the
        // time the canary would be scheduled, a read on the worker has
        // already blocked.
        let request = tokio::spawn(
            app.oneshot(
                Request::get("/api/works/t/progress")
                    .body(Body::empty())
                    .unwrap(),
            ),
        );
        let canary = tokio::spawn({
            let canary_ran = Arc::clone(&canary_ran);
            async move { canary_ran.store(true, Ordering::SeqCst) }
        });

        request.await.unwrap().unwrap();
        canary.await.unwrap();
        watchdog.join().unwrap();

        crate::test_support::cleanup("taiyaku-test-progress-blocking");
        assert!(
            !rescued_by_deadline.load(Ordering::SeqCst),
            "the runtime polled nothing else for half a second: the progress read is \
             running on the async worker rather than behind spawn_blocking"
        );
    }

    /// The other half of what progress has to get right. A slot means nothing
    /// without the edition it is a slot of, so a body naming an edition this
    /// work does not have is stored garbage: the reader is sent back to a
    /// position in a document that cannot be resolved, and nothing anywhere
    /// says why. Accepting it silently is worse than 404, which is why the
    /// guard exists — and it was the one thing here with no test.
    #[tokio::test]
    async fn progress_for_an_edition_the_work_does_not_have_is_refused() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-progress-edition"));
        let app = build_router(state);

        let put = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/progress")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"edition":"nope","slot":37}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(put.into_body(), 1 << 16)
            .await
            .unwrap();
        let message = String::from_utf8_lossy(&body).to_string();

        // Nothing was written, so a reader that reopens the work still gets
        // 204 rather than a position it cannot resolve.
        let after = app
            .oneshot(
                Request::get("/api/works/t/progress")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-progress-edition");
        assert!(message.contains("nope"), "must name the edition: {message}");
        assert_eq!(after.status(), StatusCode::NO_CONTENT);
    }

    /// O1, shipped: a page's `ETag` is checkable before anything expensive
    /// runs, and a client that already holds the bytes gets told so with no
    /// body at all — sending one would defeat the entire point of a `304`.
    #[tokio::test]
    async fn a_page_carries_an_etag_and_a_matching_request_is_answered_304() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        let first = app
            .clone()
            .oneshot(
                Request::get("/api/works/w/a/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let tag = first.headers()[header::ETAG].clone();

        let second = app
            .oneshot(
                Request::get("/api/works/w/a/pages/0")
                    .header(header::IF_NONE_MATCH, tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
        let body = axum::body::to_bytes(second.into_body(), 1 << 16)
            .await
            .unwrap();
        assert!(
            body.is_empty(),
            "a 304 must carry no body — that is the entire point of sending one"
        );
    }

    /// The reading-size parameter, over a fixture page of 16x16: `?w=8` is
    /// smaller than the page and must resize, and the output is declared JPEG
    /// whatever the archive held (this fixture holds PNG).
    #[tokio::test]
    async fn a_width_smaller_than_the_page_resizes_it_and_declares_jpeg() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/w/a/pages/0?w=8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let decoded = image::load_from_memory(&body).expect("a decodable image");
        assert_eq!(
            (decoded.width().max(decoded.height())),
            8,
            "the long edge must be what was asked for"
        );
    }

    /// The half that makes this safe on every edition: a page already within
    /// the requested edge is handed back untouched rather than re-encoded, so
    /// asking for a size no larger than the source costs nothing and loses no
    /// quality. On the real corpus this is `raw-mono` and `en-fan`, which are
    /// frequently smaller than a viewport asks for.
    #[tokio::test]
    async fn a_width_larger_than_the_page_serves_the_original_bytes_untouched() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        let plain = app
            .clone()
            .oneshot(
                Request::get("/api/works/w/a/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let plain_type = plain.headers()[header::CONTENT_TYPE].clone();
        let plain_bytes = axum::body::to_bytes(plain.into_body(), 1 << 20)
            .await
            .unwrap();

        let wide = app
            .oneshot(
                Request::get("/api/works/w/a/pages/0?w=700")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wide.status(), StatusCode::OK);
        assert_eq!(wide.headers()[header::CONTENT_TYPE], plain_type);
        let wide_bytes = axum::body::to_bytes(wide.into_body(), 1 << 20)
            .await
            .unwrap();
        assert_eq!(
            wide_bytes, plain_bytes,
            "a page already small enough must not be re-encoded"
        );
    }

    /// The trap this parameter introduces. One page now has a representation
    /// per size, so a tag that named only the member and offset would let a
    /// client holding the small one be answered 304 for the large one and go
    /// on drawing a soft page with nothing to point at.
    #[tokio::test]
    async fn a_validator_from_one_size_does_not_satisfy_a_request_for_another() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        let small = app
            .clone()
            .oneshot(
                Request::get("/api/works/w/a/pages/0?w=8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let small_tag = small.headers()[header::ETAG].clone();

        let full = app
            .clone()
            .oneshot(
                Request::get("/api/works/w/a/pages/0")
                    .header(header::IF_NONE_MATCH, small_tag.clone())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            full.status(),
            StatusCode::OK,
            "the full-size page must not be short-circuited by a resized page's tag"
        );
        assert_ne!(full.headers()[header::ETAG], small_tag);

        // And the same size still validates, so the parameter has not cost the
        // conditional request that O1 shipped.
        let again = app
            .oneshot(
                Request::get("/api/works/w/a/pages/0?w=8")
                    .header(header::IF_NONE_MATCH, small_tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    }

    /// A width outside the sensible range is clamped rather than refused: a
    /// hand-typed URL or a client bug should still draw a page. The ceiling is
    /// what stops one cache entry per pixel value.
    #[tokio::test]
    async fn an_absurd_width_is_clamped_rather_than_refused() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        for query in ["?w=0", "?w=1", "?w=999999", "?w=4096"] {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/works/w/a/pages/0{query}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "for {query}");
        }
    }

    /// A validator naming a *different* page must not short-circuit this
    /// one — `If-None-Match` says "I already have the page this tag names",
    /// and a stale tag names something else entirely.
    #[tokio::test]
    async fn a_stale_validator_gets_the_page_rather_than_a_304() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-routes-etag-stale"));
        let app = build_router(state);

        let first = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/raw/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let stale_tag = first.headers()[header::ETAG].clone();

        let second = app
            .oneshot(
                Request::get("/api/works/t/raw/pages/2")
                    .header(header::IF_NONE_MATCH, stale_tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = second.status();
        let bytes = axum::body::to_bytes(second.into_body(), 1 << 16)
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-routes-etag-stale");
        assert_eq!(
            status,
            StatusCode::OK,
            "a tag naming a different page must not answer 304"
        );
        assert_eq!(
            &bytes[..],
            b"b1.jpg",
            "the actual page must still be served, not an empty 304 body"
        );
    }

    /// A member has one `sha256` and, in this fixture, two pages. The third
    /// test the brief calls for: this is the one that would catch keying an
    /// `ETag` on the member hash alone, which would answer every page after
    /// the first with the first page's own tag.
    #[tokio::test]
    async fn two_pages_of_one_member_have_different_etags() {
        let state = Arc::new(crate::test_support::state(
            "taiyaku-test-routes-etag-member",
        ));
        let app = build_router(state);

        // Pages 0 and 1 of `raw` are both `c1.cbz`'s pages (see
        // `test_support::work`) — one member, two offsets.
        let page0 = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/raw/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page0.status(), StatusCode::OK);
        let tag0 = page0.headers()[header::ETAG].clone();

        let page1 = app
            .oneshot(
                Request::get("/api/works/t/raw/pages/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page1.status(), StatusCode::OK);
        let tag1 = page1.headers()[header::ETAG].clone();

        crate::test_support::cleanup("taiyaku-test-routes-etag-member");
        assert_ne!(
            tag0, tag1,
            "page 0 and page 1 share one member and its one sha256 — the \
             offset is the only thing that can tell them apart, and a tag \
             keyed on the member hash alone would make these equal"
        );
    }

    #[tokio::test]
    async fn a_thumb_route_answers_a_jpeg_and_404s_a_page_that_does_not_exist() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        let ok = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/works/w/a/pages/0/thumb")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/jpeg");

        let missing = app
            .oneshot(
                Request::builder()
                    .uri("/api/works/w/a/pages/9999/thumb")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    /// O1: every route whose bytes are fixed by a member's content says how
    /// long they may be reused without asking.
    ///
    /// The `ETag` shipped before this and shrank a repeat to about 300 bytes;
    /// it never removed the round trip, which is the part a reader waits on.
    /// All four byte routes are checked rather than the one this was first
    /// noticed on, because O5 already recorded once what fixing only the
    /// `/api/…` route is worth: Panels reaches pages through the OPDS stream,
    /// and the cover is the only route a cold grid view touches at all.
    ///
    /// The `304` is checked for the same header, and it is the case most
    /// easily left out: RFC 9111 §4.3.4 has a client refresh its stored
    /// response from the `304`'s headers, so omitting it here would leave the
    /// entry exactly as stale as it was and cost a round trip on the very
    /// next read.
    #[tokio::test]
    async fn every_route_serving_a_member_s_bytes_says_how_long_they_may_be_reused() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        for uri in [
            "/api/works/w/a/pages/0",
            "/api/works/w/a/pages/0/thumb",
            "/opds/v1.2/works/w/a/pages/0",
            "/opds/v1.2/works/w/a/cover",
        ] {
            let response = app
                .clone()
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL),
                Some(&header::HeaderValue::from_static(PAGE_CACHE_CONTROL)),
                "{uri} serves bytes fixed by a member's content and must say \
                 so; without it every reuse asks the network first"
            );
        }

        // Both routes that carry a validator, revalidated — not just the
        // `/api/…` one. O5 already recorded once what checking only that one
        // is worth: Panels reaches pages through the OPDS stream, so a `304`
        // asserted only here would be asserted on the route nothing uses.
        for uri in ["/api/works/w/a/pages/0", "/opds/v1.2/works/w/a/pages/0"] {
            let first = app
                .clone()
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let tag = first.headers()[header::ETAG].clone();

            let revalidated = app
                .clone()
                .oneshot(
                    Request::get(uri)
                        .header(header::IF_NONE_MATCH, &tag)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED, "{uri}");
            assert_eq!(
                revalidated.headers().get(header::CACHE_CONTROL),
                Some(&header::HeaderValue::from_static(PAGE_CACHE_CONTROL)),
                "{uri}: a 304 that omits it buys one response and no more — \
                 the client stores the same stale entry and asks again next \
                 read"
            );
        }
    }

    /// The other half, and the half that has something to fail against: no
    /// document a client can change is given a freshness lifetime.
    ///
    /// A page is fixed by a member's `sha256`. A sidecar is rewritten by
    /// `PATCH /alignments`, notes by `PUT /notes`, progress by
    /// `PUT /progress`, and every feed restates whatever those hold — so a
    /// client told it may reuse any of them for a day would show a correction
    /// it had just made being undone, or lose a note to a second device.
    ///
    /// This is what fails the day someone reaches for a blanket
    /// `SetResponseHeaderLayer` on the router rather than the four routes
    /// where the claim is true.
    ///
    /// **Both statuses are checked, and the second one is why this test
    /// exists in the shape it does.** The first version of this looked only
    /// at the `200`s and passed while the server was actively telling clients
    /// they could reuse a sidecar for a day: `not_modified` is shared by
    /// `get_page`, `get_sidecar`, `get_notes` and the OPDS stream, so putting
    /// the page constant inside it leaked a freshness lifetime onto the `304`
    /// of every route that shares it. A client caches what the `304` says
    /// (RFC 9111 §4.3.4), so the leak was worth exactly as much as putting it
    /// on the `200` would have been, and no test saw it.
    #[tokio::test]
    async fn no_document_a_client_can_change_is_given_a_freshness_lifetime() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        for uri in [
            "/api/works",
            "/api/works/w",
            "/api/works/w/progress",
            "/api/works/w/notes",
            "/opds/v1.2/catalog",
            "/opds/v1.2/works",
            "/opds/v1.2/works/w",
        ] {
            let response = app
                .clone()
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL),
                None,
                "{uri} answers a document this server rewrites; a freshness \
                 lifetime on it hands a reader back an edit they have already \
                 made"
            );
        }

        // The two that carry a validator, revalidated. A `304` is where a
        // client refreshes what it believes about an entry, so a mutable
        // document's `304` has to be as silent about freshness as its `200`.
        for uri in ["/api/works/w", "/api/works/w/notes"] {
            let first = app
                .clone()
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let tag = first.headers()[header::ETAG].clone();

            let revalidated = app
                .clone()
                .oneshot(
                    Request::get(uri)
                        .header(header::IF_NONE_MATCH, &tag)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED, "{uri}");
            assert_eq!(
                revalidated.headers().get(header::CACHE_CONTROL),
                None,
                "{uri} shares `not_modified` with the page routes, and a \
                 client stores what a 304 tells it — a freshness lifetime \
                 here is worth exactly what one on the 200 would be"
            );
        }
    }

    /// A real request, through the real router, has to produce a real log
    /// line — and that line's `bytes` has to be non-zero.
    ///
    /// This is the case the `CONTENT_LENGTH` mistake would pass silently: a
    /// unit test on `Log` alone never sends a response through axum, so it
    /// cannot see that the header is not yet set when the middleware reads
    /// it. Only driving the middleware through a page fetch — a body with
    /// real bytes in it — proves the log records what a client actually
    /// received rather than 0 for every request.
    #[tokio::test]
    async fn a_page_fetch_through_the_router_logs_a_line_with_real_byte_count() {
        let mut state = crate::test_support::state("taiyaku-test-routes-reqlog");
        let (log, log_path) = crate::test_support::log("taiyaku-test-routes-reqlog");
        state.log = Some(log);
        let app = build_router(Arc::new(state));

        let page = app
            .oneshot(
                Request::get("/api/works/t/raw/pages/2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(page.into_body(), 1 << 20)
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-routes-reqlog");

        let entries = crate::test_support::logged_lines(&log_path);
        let _ = std::fs::remove_file(&log_path);
        assert_eq!(entries.len(), 1, "exactly one request went through");
        let entry = &entries[0];
        assert_eq!(entry["method"], "GET");
        assert_eq!(entry["path"], "/api/works/t/raw/pages/2");
        assert_eq!(entry["status"], 200);
        assert_eq!(
            entry["bytes"],
            bytes.len() as u64,
            "the logged size must match the bytes the client actually received"
        );
        assert!(
            entry["bytes"].as_u64().unwrap() > 0,
            "a page is never a zero-byte response"
        );
        assert_eq!(entry["in_flight"], 1, "a lone request arrived alone");
    }

    /// O1's question — does a client ever send `If-None-Match`? — cannot be
    /// answered from a log that records no request headers, and this one
    /// recorded none. An `ETag` is a line of code; knowing whether it would
    /// buy anything is the part that needs evidence, and the evidence has to
    /// come from a real session with a real client.
    ///
    /// The set logged is an allowlist, so this pins both halves: a header on
    /// it survives into the line, and one that is not on it does not. A log
    /// that swept up everything a client sends would grow unbounded per line
    /// and would put a `Cookie` or an `Authorization` into a file that gets
    /// pasted into issues.
    #[tokio::test]
    async fn a_conditional_request_header_is_logged_and_an_unlisted_one_is_not() {
        let mut state = crate::test_support::state("taiyaku-test-routes-reqlog-headers");
        let (log, log_path) = crate::test_support::log("taiyaku-test-routes-reqlog-headers");
        state.log = Some(log);
        let app = build_router(Arc::new(state));

        let page = app
            .oneshot(
                Request::get("/api/works/t/raw/pages/2")
                    .header("if-none-match", "\"abc123\"")
                    .header("user-agent", "Panels/1.0")
                    .header("cookie", "session=secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);

        crate::test_support::cleanup("taiyaku-test-routes-reqlog-headers");

        let entries = crate::test_support::logged_lines(&log_path);
        let _ = std::fs::remove_file(&log_path);
        assert_eq!(entries.len(), 1);
        let headers = &entries[0]["headers"];
        assert_eq!(
            headers["if-none-match"], "\"abc123\"",
            "the header O1 turns on has to survive into the line"
        );
        assert_eq!(headers["user-agent"], "Panels/1.0", "which client asked");
        assert!(
            headers.get("cookie").is_none(),
            "an unlisted header must not be recorded: {headers}"
        );
    }

    /// A client asking for `?width=1536` has to be visible in the log: this
    /// server serves no resized images at all, which makes it *more*
    /// important to see the parameter, not less — logging `path()` alone
    /// would make it look like no client ever asks for one, when the truth
    /// is only that nothing here answers it yet.
    #[tokio::test]
    async fn a_request_with_a_query_string_is_logged_with_it() {
        let mut state = crate::test_support::state("taiyaku-test-routes-reqlog-query");
        let (log, log_path) = crate::test_support::log("taiyaku-test-routes-reqlog-query");
        state.log = Some(log);
        let app = build_router(Arc::new(state));

        let page = app
            .oneshot(
                Request::get("/api/works/t/raw/pages/2?width=1536")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);

        crate::test_support::cleanup("taiyaku-test-routes-reqlog-query");

        let entries = crate::test_support::logged_lines(&log_path);
        let _ = std::fs::remove_file(&log_path);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]["path"], "/api/works/t/raw/pages/2?width=1536",
            "the query string must survive into the logged path"
        );
    }

    /// `if_match_hits`'s reason to exist: absent, matching and stale are
    /// three different answers, not two. Task 6 turns `None` into a 428 and
    /// `Some(false)` into a 412 — collapsing the first into the second would
    /// tell a client that sent no validator it lost a race it never entered.
    #[test]
    fn if_match_distinguishes_absent_from_matching_from_stale() {
        let none_sent = HeaderMap::new();
        assert_eq!(
            if_match_hits(&none_sent, "\"abc\""),
            None,
            "no header at all is not the same question as a stale one"
        );

        let mut matching = HeaderMap::new();
        matching.insert(header::IF_MATCH, "\"abc\"".parse().unwrap());
        assert_eq!(if_match_hits(&matching, "\"abc\""), Some(true));

        let mut stale = HeaderMap::new();
        stale.insert(header::IF_MATCH, "\"old\"".parse().unwrap());
        assert_eq!(if_match_hits(&stale, "\"abc\""), Some(false));
    }

    /// The validator the correction path writes against. It must change when
    /// the document does, or `If-Match` guards nothing.
    // `update`'s own `#[allow(clippy::result_large_err)]` does not reach a
    // closure written at the call site — clippy checks each closure as its
    // own item — so the explicit closure here needs the same allow, for the
    // same reason `works.rs`'s and this file's other `update` calls do.
    #[allow(clippy::result_large_err)]
    #[tokio::test]
    async fn the_sidecar_carries_an_etag_that_tracks_its_content() {
        let state = Arc::new(crate::test_support::state(
            "taiyaku-test-routes-sidecar-etag",
        ));
        let router = build_router(state.clone());

        let first = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/works/t")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tag = first.headers().get(axum::http::header::ETAG).cloned();
        state.works[0]
            .update(|current| {
                let mut next = (**current).clone();
                next.work.title = "Different".into();
                Ok((std::sync::Arc::new(next), ()))
            })
            .expect("updates");
        let second = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/works/t")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-routes-sidecar-etag");
        let tag = tag.expect("a sidecar response carries an ETag");
        assert_ne!(
            tag,
            *second.headers().get(axum::http::header::ETAG).unwrap(),
            "a changed document must not validate against the old tag"
        );
    }

    async fn etag_of(router: &axum::Router, uri: &str) -> String {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response
            .headers()
            .get(axum::http::header::ETAG)
            .expect("an ETag")
            .to_str()
            .unwrap()
            .to_string()
    }

    async fn correct(router: &axum::Router, tag: &str, body: &str) -> axum::http::StatusCode {
        router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri("/api/works/t/alignments")
                    .header("if-match", tag)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    /// The same request, keeping the body as well as the status.
    ///
    /// A refusal that names what was missing is a constraint this project
    /// holds every route to, so the wording is worth asserting rather than
    /// only the number — `correct` above stays for the cases where the status
    /// is the whole claim.
    async fn correct_with_body(
        router: &axum::Router,
        work: &str,
        tag: &str,
        body: &str,
    ) -> (axum::http::StatusCode, String) {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/works/{work}/alignments"))
                    .header("if-match", tag)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// The milestone in one test: correct a pairing, and have the document say
    /// so afterwards.
    #[tokio::test]
    async fn a_correction_is_written_and_read_back() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct",
        ));
        let router = build_router(state.clone());
        let tag = etag_of(&router, "/api/works/t").await;

        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri("/api/works/t/alignments")
                    .header("if-match", &tag)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let fresh = response.headers().get(axum::http::header::ETAG).cloned();
        // The body is the point of answering 200 rather than 204: the client
        // refreshes its resolver from this rather than making a second request
        // that could already be out of date again. Without this assertion,
        // replacing the whole response with a bare `StatusCode::OK` would
        // delete the round-trip-refresh feature and leave every other check in
        // this test green.
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let on_disk = std::fs::read_to_string(&state.works[0].path).unwrap();
        crate::test_support::cleanup("taiyaku-test-routes-correct");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_ne!(
            fresh,
            Some(tag.parse().unwrap()),
            "the ETag moves with the write"
        );
        let returned = taiyaku_core::Sidecar::from_json(&String::from_utf8_lossy(&body))
            .expect("the response body is the updated document");
        assert_eq!(
            returned
                .alignments
                .iter()
                .flat_map(|a| &a.runs)
                .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
                .count(),
            2,
            "the document handed back already carries the correction"
        );
        assert!(
            on_disk.contains("\"hand\""),
            "the file records who decided: {on_disk}"
        );
        assert_eq!(
            state.works[0].sidecar().alignments[0]
                .runs
                .iter()
                .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
                .count(),
            2,
            "the slot asserted and the slot displaced, in memory as well as on disk"
        );
    }

    /// `Corrected::AlreadyTrue` — asserting a pairing the document already
    /// holds — must not touch the file at all: `patch_alignments` only calls
    /// `atomic::write` when `what != AlreadyTrue`. A write neither read nor
    /// followed by a change is exactly the crash window `atomic::write`
    /// exists to close, so skipping it here is not an optimisation, it is one
    /// fewer moment a concurrent reader could see a temp-file rename that
    /// says nothing new.
    ///
    /// Proven by inode identity rather than by content or mtime, for the
    /// reason `atomic.rs`'s own
    /// `the_destination_is_replaced_by_rename_not_truncation` gives: an
    /// `AlreadyTrue` correction writes back byte-identical JSON, so content
    /// alone cannot tell "skipped" from "wrote the same bytes again", and this
    /// filesystem was measured to give two `atomic::write` calls microseconds
    /// apart the identical mtime down to the nanosecond — so mtime cannot
    /// either. A rename always replaces the destination with a fresh inode,
    /// whether or not the bytes changed, so the inode is the one signal that
    /// distinguishes "no write happened" from "a write happened that changed
    /// nothing".
    #[tokio::test]
    async fn a_no_op_correction_does_not_touch_the_file() {
        use std::os::unix::fs::MetadataExt;

        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-noop",
        ));
        let router = build_router(state.clone());
        let first_tag = etag_of(&router, "/api/works/t").await;
        let first_status = correct(
            &router,
            &first_tag,
            r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
        )
        .await;
        assert_eq!(
            first_status,
            axum::http::StatusCode::OK,
            "the setup write lands"
        );

        let before_bytes = std::fs::read(&state.works[0].path).unwrap();
        let before_ino = std::fs::metadata(&state.works[0].path).unwrap().ino();
        let second_tag = etag_of(&router, "/api/works/t").await;

        // The same assertion again: the document already says slot 2 pairs
        // with slot 1, so this is `Corrected::AlreadyTrue`.
        let second_status = correct(
            &router,
            &second_tag,
            r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
        )
        .await;

        let after_bytes = std::fs::read(&state.works[0].path).unwrap();
        let after_ino = std::fs::metadata(&state.works[0].path).unwrap().ino();
        crate::test_support::cleanup("taiyaku-test-routes-correct-noop");
        assert_eq!(
            second_status,
            axum::http::StatusCode::OK,
            "a no-op is still answered 200"
        );
        assert_eq!(
            before_bytes, after_bytes,
            "a no-op correction must leave the file's bytes exactly as they were"
        );
        assert_eq!(
            before_ino, after_ino,
            "identical bytes are not proof nothing was written — a rename always gives the \
             destination a fresh inode, whether or not the bytes it carries changed, so the \
             inode is what actually distinguishes a skipped write from a redundant one"
        );
    }

    /// The lost-update hole the design names. Two devices read one document,
    /// both correct it; the second must be told rather than silently winning.
    #[tokio::test]
    async fn a_correction_against_a_stale_validator_is_refused() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-stale",
        ));
        let router = build_router(state.clone());
        let stale = etag_of(&router, "/api/works/t").await;
        correct(
            &router,
            &stale,
            r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
        )
        .await;

        let (second, message) = correct_with_body(
            &router,
            "t",
            &stale,
            r#"{"from":"raw","to":"en","a":"3","b":"0"}"#,
        )
        .await;

        let after = state.works[0].sidecar();
        let current = sidecar_etag(&after);
        crate::test_support::cleanup("taiyaku-test-routes-correct-stale");
        assert_eq!(second, axum::http::StatusCode::PRECONDITION_FAILED);
        // Telling a client only that it lost is not enough to let it recover:
        // the message carries the validator it should have sent, so the next
        // attempt can be made without a second round trip to find out.
        assert!(
            message.contains(&current),
            "the refusal must carry the ETag the client now needs, got: {message}"
        );
        // The first correction only reassigns slots 1 and 2 (the one asserted
        // and the one it displaced); slot 3 is untouched by it. If the second,
        // stale-validator correction had wrongly landed, slot 3 would resolve
        // to `en`'s slot 0 instead — so slot 3 still holding the identity
        // pairing from before either correction is exactly what proves the
        // refused request never applied.
        assert_eq!(
            taiyaku_core::resolve(&after, "raw", "en", 3),
            Some(taiyaku_core::Slot {
                page: 3,
                half: None
            }),
            "the refused correction must not have landed"
        );
    }

    /// A client that sent no validator has not lost a race, it has not entered
    /// one — and telling it 412 would send it looking for a conflict that does
    /// not exist.
    #[tokio::test]
    async fn a_correction_with_no_validator_at_all_says_which_header_is_missing() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-bare",
        ));
        let router = build_router(state);

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri("/api/works/t/alignments")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        crate::test_support::cleanup("taiyaku-test-routes-correct-bare");
        assert_eq!(status, axum::http::StatusCode::PRECONDITION_REQUIRED);
        assert!(
            String::from_utf8_lossy(&body).contains("If-Match"),
            "must name the header a client has to send"
        );
    }

    /// Each row of this route's status table, pinned.
    ///
    /// The mapping from `NotCorrected` to a status is hand-written, and an
    /// exhaustive `match` only protects against a *new variant* being dropped
    /// — it says nothing about an existing one being given the wrong status.
    /// Changing `SameEdition => BAD_REQUEST` to `NOT_FOUND` compiles, and
    /// without this test the suite stays green. Task 2's own tests pin which
    /// variant `Sidecar::correct` returns; which status it becomes is this
    /// route's to answer, and so this route's to test.
    ///
    /// `Unreadable` (409) is deliberately absent: producing it needs a stored
    /// edge whose spans do not resolve, which this fixture cannot express
    /// without hand-writing a broken document, and contorting the fixture to
    /// reach one status would cost more than it pins.
    #[tokio::test]
    async fn each_refusal_answers_the_status_its_kind_calls_for() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-statuses",
        ));
        let router = build_router(state.clone());
        let tag = etag_of(&router, "/api/works/t").await;

        let cases = [
            // An edition cannot be its own counterpart — the request is
            // coherent JSON naming an incoherent operation.
            (
                r#"{"from":"raw","to":"raw","a":"2","b":"1"}"#,
                axum::http::StatusCode::BAD_REQUEST,
                "own counterpart",
            ),
            // Names an edition this document does not define.
            (
                r#"{"from":"raw","to":"nosuch","a":"2","b":"1"}"#,
                axum::http::StatusCode::NOT_FOUND,
                "nosuch",
            ),
            // Names a slot past the end of a four-page edition.
            (
                r#"{"from":"raw","to":"en","a":"99","b":"1"}"#,
                axum::http::StatusCode::NOT_FOUND,
                "99",
            ),
            // Not a correction at all.
            (
                r#"{"nonsense":true}"#,
                axum::http::StatusCode::BAD_REQUEST,
                "not a correction",
            ),
            // The field name is misspelled. Without `deny_unknown_fields` this
            // would deserialise to `b: None`, unpair slot 2, and answer 200 —
            // a destructive edit the client never asked for and is not told
            // about. See `Correction`'s own doc comment.
            (
                r#"{"from":"raw","to":"en","a":"2","bb":"1"}"#,
                axum::http::StatusCode::BAD_REQUEST,
                "not a correction",
            ),
            // `b` omitted entirely. The other half of the same rule: absence
            // must not spell "unpair", so this is refused rather than quietly
            // taking a pairing away. The refusal names the field, because
            // serde's own "missing field `b`" is the most useful thing a
            // client could be told here.
            (
                r#"{"from":"raw","to":"en","a":"2"}"#,
                axum::http::StatusCode::BAD_REQUEST,
                "needs `b`",
            ),
        ];

        let mut answered = Vec::new();
        for (body, _, _) in &cases {
            answered.push(correct_with_body(&router, "t", &tag, body).await);
        }
        // A work this server does not hold, which is refused before the body
        // is ever looked at.
        let missing = correct_with_body(
            &router,
            "nosuchwork",
            &tag,
            r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
        )
        .await;
        let unchanged = state.works[0].sidecar().alignments[0].runs.len();

        crate::test_support::cleanup("taiyaku-test-routes-correct-statuses");
        for ((body, want, names), (got, message)) in cases.iter().zip(&answered) {
            assert_eq!(got, want, "for {body}: {message}");
            assert!(
                message.to_lowercase().contains(&names.to_lowercase()),
                "the refusal for {body} must name what was wrong, got: {message}"
            );
        }
        assert_eq!(missing.0, axum::http::StatusCode::NOT_FOUND);
        assert!(
            missing.1.contains("nosuchwork"),
            "must name the work: {}",
            missing.1
        );
        assert_eq!(
            unchanged, 1,
            "not one of these refusals may have changed the document"
        );
    }

    /// The unpair path, end to end.
    ///
    /// `"b": null` is the one thing an absent `b` used to mean, and it is the
    /// reason `b` is nullable rather than simply a `String`. Nothing in `web/`
    /// can send it yet — the reader has no "this page pairs with nothing"
    /// gesture — so without this test the whole justification for keeping the
    /// field nullable would rest on a path nothing exercises.
    #[tokio::test]
    async fn an_explicit_null_unpairs_the_slot() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-unpair",
        ));
        let router = build_router(state.clone());
        let tag = etag_of(&router, "/api/works/t").await;
        let before = taiyaku_core::resolve(&state.works[0].sidecar(), "raw", "en", 2);

        let (status, _) = correct_with_body(
            &router,
            "t",
            &tag,
            r#"{"from":"raw","to":"en","a":"2","b":null}"#,
        )
        .await;

        let after = state.works[0].sidecar();
        crate::test_support::cleanup("taiyaku-test-routes-correct-unpair");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(
            before.is_some(),
            "slot 2 must start out paired, or this asserts nothing"
        );
        assert_eq!(
            taiyaku_core::resolve(&after, "raw", "en", 2),
            None,
            "the slot now pairs with nothing"
        );
        assert_eq!(
            after.alignments[0]
                .runs
                .iter()
                .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
                .count(),
            1,
            "an unpairing displaces nobody, so it is the only hand run"
        );
    }

    /// A span is the format's own notation and `45R-46L` is a legal one — but
    /// a correction addresses one slot, and taking the start of a range would
    /// silently correct something other than what was asked.
    #[tokio::test]
    async fn a_correction_naming_a_range_rather_than_a_slot_is_refused() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-range",
        ));
        let router = build_router(state.clone());
        let tag = etag_of(&router, "/api/works/t").await;

        let (status, message) = correct_with_body(
            &router,
            "t",
            &tag,
            r#"{"from":"raw","to":"en","a":"2-3","b":"1"}"#,
        )
        .await;

        crate::test_support::cleanup("taiyaku-test-routes-correct-range");
        assert!(
            message.contains("2-3") && message.contains("range"),
            "the refusal must name the span it would not take: {message}"
        );
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    /// A work nobody has annotated still answers with a document, not a 204.
    ///
    /// A `PUT` requires `If-Match`, so a client that has never seen a document
    /// would have no validator to send and no way to write its first note
    /// conditionally. Synthesising an empty one — with an `ETag` over exactly
    /// the bytes sent — makes the first write an ordinary conditional write
    /// like every later one.
    #[tokio::test]
    async fn a_work_with_no_notes_answers_an_empty_document_with_a_validator() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-get"));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let tag = response
            .headers()
            .get(header::ETAG)
            .expect("a document a client may write against carries a validator")
            .to_str()
            .unwrap()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-get");
        assert_eq!(tag, notes_etag(&body), "the tag must be over what was sent");
        let parsed: crate::notes::Notes = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.work, "t");
        assert!(parsed.notes.is_empty());
    }

    /// The same validator answers `304` on the next load, which is what keeps
    /// a per-page-load read of a document that rarely changes off the wire.
    #[tokio::test]
    async fn a_matching_if_none_match_answers_304_for_notes() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-304"));
        let app = build_router(state);
        let tag = etag_of(&app, "/api/works/t/notes").await;

        let again = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .header(header::IF_NONE_MATCH, &tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-304");
        assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    }

    /// A work this server does not hold says so, in the same words every other
    /// route addressed by work uses.
    #[tokio::test]
    async fn notes_for_an_unknown_work_are_a_404_naming_it() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-no-work"));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/nope/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-no-work");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("nope"), "the refusal must name it: {body}");
    }

    /// The notes a work already holds are what it serves, and the validator
    /// moves with them — a tag that did not change when the document did would
    /// let a stale client write over a note it never saw.
    #[tokio::test]
    async fn stored_notes_are_served_and_change_the_validator() {
        let state = crate::test_support::state("taiyaku-test-notes-stored");
        crate::notes::store(
            &state.works[0],
            &crate::notes::Notes {
                work: "t".into(),
                notes: vec![crate::notes::Note {
                    id: "01J8".into(),
                    edition: "raw".into(),
                    slot: 2,
                    region: None,
                    color: None,
                    body: "kept".into(),
                    created: "2026-08-05T10:00:00Z".into(),
                    updated: "2026-08-05T10:00:00Z".into(),
                }],
            },
        )
        .unwrap();
        let empty_tag = notes_etag(&crate::notes::to_json(&crate::notes::empty("t")).unwrap());
        let app = build_router(Arc::new(state));

        let response = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tag = response
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-stored");
        assert_eq!(parsed.notes.len(), 1);
        assert_eq!(parsed.notes[0].body, "kept");
        assert_ne!(
            tag, empty_tag,
            "a document with a note in it must not validate as an empty one"
        );
    }

    /// One note, as a client sends it.
    fn one_note(id: &str, body: &str) -> String {
        format!(
            r#"{{"work":"t","notes":[{{"id":"{id}","edition":"raw","slot":1,"body":"{body}",
               "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}]}}"#
        )
    }

    async fn write_notes(
        router: &axum::Router,
        tag: Option<&str>,
        body: String,
    ) -> (StatusCode, String) {
        let mut request =
            Request::put("/api/works/t/notes").header(header::CONTENT_TYPE, "application/json");
        if let Some(tag) = tag {
            request = request.header(header::IF_MATCH, tag);
        }
        let response = router
            .clone()
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    /// The three answers `If-Match` has, on the one route where getting them
    /// wrong loses prose a person typed: no header at all is 428 (you have not
    /// entered a race), a stale one is 412 (you lost it), a current one writes.
    #[tokio::test]
    async fn writing_notes_needs_a_current_if_match() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-put"));
        let app = build_router(state);

        let (unconditional, _) = write_notes(&app, None, one_note("01J8", "first")).await;
        assert_eq!(unconditional, StatusCode::PRECONDITION_REQUIRED);

        let (stale, _) = write_notes(&app, Some("\"nonsense\""), one_note("01J8", "first")).await;
        assert_eq!(stale, StatusCode::PRECONDITION_FAILED);

        let tag = etag_of(&app, "/api/works/t/notes").await;
        let (written, _) = write_notes(&app, Some(&tag), one_note("01J8", "first")).await;
        assert_eq!(written, StatusCode::OK);

        let fresh = etag_of(&app, "/api/works/t/notes").await;
        let read_back = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(read_back.into_body(), 1 << 20)
            .await
            .unwrap();
        let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-put");
        assert_ne!(fresh, tag, "the document changed, so the tag must have");
        assert_eq!(parsed.notes.len(), 1);
        assert_eq!(parsed.notes[0].body, "first");
    }

    /// The lost-update case itself, which is the reason any of this exists:
    /// two devices read the same document, both write, and the second must be
    /// told rather than quietly replacing the first reader's note.
    #[tokio::test]
    async fn a_second_writer_holding_the_same_tag_is_refused_rather_than_winning() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-race"));
        let app = build_router(state);
        let shared = etag_of(&app, "/api/works/t/notes").await;

        let (first, _) = write_notes(&app, Some(&shared), one_note("01J8A", "device one")).await;
        assert_eq!(first, StatusCode::OK);

        let (second, message) =
            write_notes(&app, Some(&shared), one_note("01J8B", "device two")).await;

        let survived = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(survived.into_body(), 1 << 20)
            .await
            .unwrap();
        let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-race");
        assert_eq!(second, StatusCode::PRECONDITION_FAILED);
        assert!(
            message.contains("ETag is now"),
            "a loser needs the current validator to retry against: {message}"
        );
        assert_eq!(parsed.notes[0].body, "device one", "the first write stands");
    }

    /// The validation is not merely unit-tested: a bad anchor dispatched
    /// through the router comes back as a refusal a client can read, and with
    /// the status that tells it apart from a malformed body. A guard nothing
    /// calls is the shape O4 records — correct by inspection only.
    #[tokio::test]
    async fn a_note_on_a_slot_that_does_not_exist_is_refused_through_the_router() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-bad-slot"));
        let app = build_router(state);
        let tag = etag_of(&app, "/api/works/t/notes").await;

        let (status, message) = write_notes(
            &app,
            Some(&tag),
            r#"{"work":"t","notes":[{"id":"01J8","edition":"raw","slot":99,
               "body":"b","created":"c","updated":"u"}]}"#
                .to_string(),
        )
        .await;

        let (duplicate, dupe_message) = write_notes(
            &app,
            Some(&tag),
            r#"{"work":"t","notes":[
               {"id":"01J8","edition":"raw","slot":0,"body":"a","created":"c","updated":"u"},
               {"id":"01J8","edition":"raw","slot":1,"body":"b","created":"c","updated":"u"}]}"#
                .to_string(),
        )
        .await;

        crate::test_support::cleanup("taiyaku-test-notes-bad-slot");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(
            message.contains("99") && message.contains("slots"),
            "the refusal must say what was wrong: {message}"
        );
        assert_eq!(
            duplicate,
            StatusCode::BAD_REQUEST,
            "a malformed document is not a missing one"
        );
        assert!(dupe_message.contains("01J8"), "name the id: {dupe_message}");
    }

    /// The lost-update guard under a genuine race, not a sequence.
    ///
    /// `a_second_writer_holding_the_same_tag_is_refused_rather_than_winning`
    /// sends one write and then the other, which is the case a `RwLock` would
    /// also pass without holding anything across the read-modify-write.
    /// `Work::notes_guard` exists for the interleaving that test cannot
    /// produce: two writes *in flight together*, both having read the same
    /// document and both finding their validator current.
    ///
    /// **Raced many times, deliberately.** A single race is not a reliable
    /// detector: measured by removing the guard and running this five times,
    /// two of those runs still happened to serialise and passed. One round is
    /// therefore a test that can miss the very defect it exists for, which is
    /// worse than no test — it reports green for a reason it never checked.
    /// Twenty rounds put the chance of every one of them serialising at
    /// roughly `0.4^20`, and the same sabotage now fails immediately.
    ///
    /// It cannot fail *falsely*, whatever the interleaving: exactly one of two
    /// writes can be holding the current tag when it compares, so the
    /// assertion is on the pair of outcomes rather than on which request won.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_writes_in_flight_together_leave_exactly_one_winner() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-concurrent"));
        let app = build_router(state);

        for round in 0..20 {
            let shared = etag_of(&app, "/api/works/t/notes").await;
            let first = tokio::spawn({
                let app = app.clone();
                let tag = shared.clone();
                let body = one_note(&format!("01RACEA{round}"), "device one");
                async move { write_notes(&app, Some(&tag), body).await }
            });
            let second = tokio::spawn({
                let app = app.clone();
                let tag = shared.clone();
                let body = one_note(&format!("01RACEB{round}"), "device two");
                async move { write_notes(&app, Some(&tag), body).await }
            });
            let (a, b) = (first.await.unwrap(), second.await.unwrap());

            let mut statuses = [a.0, b.0];
            statuses.sort_by_key(|s| s.as_u16());
            if statuses != [StatusCode::OK, StatusCode::PRECONDITION_FAILED] {
                crate::test_support::cleanup("taiyaku-test-notes-concurrent");
                panic!(
                    "round {round}: {statuses:?} — one write must land and the other be told;                      two 200s is a lost note"
                );
            }
        }

        let survived = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(survived.into_body(), 1 << 20)
            .await
            .unwrap();
        let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-concurrent");
        assert_eq!(
            parsed.notes.len(),
            1,
            "each round replaces the document with one note, so the last winner's is what stands"
        );
    }

    /// A body this server will not parse is answered in this server's own
    /// words, not serde's — the same rule O5 established for a malformed page
    /// index and `patch_alignments` already follows for a malformed
    /// correction.
    #[tokio::test]
    async fn a_body_that_is_not_notes_is_refused_in_this_servers_own_words() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-garbage"));
        let app = build_router(state);
        let tag = etag_of(&app, "/api/works/t/notes").await;

        let (status, message) = write_notes(&app, Some(&tag), "{\"nope\":1}".to_string()).await;

        crate::test_support::cleanup("taiyaku-test-notes-garbage");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            message.contains("these are not notes"),
            "the refusal must be this server's own: {message}"
        );
    }

    /// `?w=1` must mean "no resize", not "resize to one pixel".
    ///
    /// The filter that does it could be `>= 1` — admitting 1 — with every
    /// other test still green, because no test asked for a width of 1 and the
    /// comment above the filter claimed `fit_at` would decline it anyway. It
    /// does not: `fit_at` returns `None` only when the page is *already*
    /// within the requested edge, so at `w=1` a 16-pixel page is re-encoded
    /// down to one pixel and handed to the reader as a JPEG.
    #[tokio::test]
    async fn a_width_of_one_is_absent_rather_than_a_one_pixel_page() {
        let state = crate::test_support::state_with_real_pages();
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/w/a/pages/0?w=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let decoded = image::load_from_memory(&body).expect("a decodable image");
        assert_eq!(
            (decoded.width(), decoded.height()),
            (16, 16),
            "the page untouched, not a one-pixel rendering of it"
        );
    }

    /// Naming an unknown work must be a 404 that says so. `NoSuchWork`'s
    /// `into_response` could return `Response::default()` — an empty 200 —
    /// with the suite green, because every test that asks for a missing work
    /// went through a route that builds its own refusal, and the ones that go
    /// through this impl only ever asked for works that exist.
    ///
    /// An empty 200 is the worst of the available answers: the reader's client
    /// cannot tell "no such work" from "a work with nothing in it".
    #[tokio::test]
    async fn an_unknown_work_is_a_404_that_names_it() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-routes-unknown"));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/nope/raw/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-routes-unknown");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("nope"),
            "must name the work asked for: {text}"
        );
    }

    /// The work list must contain the works. `list_works` could return
    /// `Response::default()` — an empty 200 — and
    /// `the_reader_can_list_a_work_read_its_sidecar_and_fetch_a_page` would
    /// still pass, because it asserts the status and never looks at the body.
    /// A reader given an empty list shows an empty library.
    #[tokio::test]
    async fn the_work_list_names_every_work_rather_than_being_merely_ok() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-routes-list"));
        let app = build_router(state);

        let response = app
            .oneshot(Request::get("/api/works").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        crate::test_support::cleanup("taiyaku-test-routes-list");
        let ids: Vec<String> = serde_json::from_slice(&body).expect("a JSON array of ids");
        assert_eq!(ids, vec!["t".to_string()]);
    }
}
