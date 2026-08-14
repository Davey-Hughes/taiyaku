//! HTTP. Everything interesting happens below this file.

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
/// Takes the serialised text rather than the document, unlike `sidecar_etag`
/// above, because both callers already hold it — `notes::to_json` and
/// `truth::to_json` are each the one serialisation of their document, and
/// hashing a second one built here would be the drift this exists to rule
/// out. Shared by `get_truth`/`put_truth` rather than given a `truth_etag`
/// wrapper of its own: the hash has nothing document-specific in it, so a
/// second name for the same bytes would be indirection with no behaviour
/// behind it.
pub(crate) fn notes_etag(json: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let digest = hasher.finalize();
    let hex = digest.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
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
        let doc = crate::truth::load(found)
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

        let current = crate::truth::load(found)
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

        crate::truth::store(found, &incoming).map_err(NoTruth::Failed)?;
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
        .route("/api/works/{work}/truth", get(get_truth).put(put_truth))
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
