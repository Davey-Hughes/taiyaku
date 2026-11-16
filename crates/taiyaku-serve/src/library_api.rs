//! The `/api/library/*` handlers. Spec C1 §5.
//!
//! Thin by design: parse, call [`taiyaku_library::browse`], serialise. Every
//! decision about *what* a browse screen may see lives in that module, in SQL,
//! so there is one place the root filter is applied and one place to read in
//! order to know whether it is.
//!
//! Kept out of `routes.rs`, which holds the reader's own routes — the two share
//! no state and have no reason to be read together.
//!
//! # Every handler takes the principal's access and passes it straight down
//!
//! `Principal.access` is the caller's grant: every library, or only some
//! directories, where only none sees nothing. Nothing here interprets it; it goes
//! to `browse`, which applies it in SQL.
//!
//! # Every handler reads the library on the blocking pool
//!
//! Every browse request reads `state.library`, whatever the grant, because
//! `browse` applies a grant in SQL; and the library's reads share one
//! connection behind a mutex (O42). Each handler asks through `on_the_library`,
//! so its wait on that mutex and its queries run on the blocking pool rather
//! than on a tokio worker thread, where they would stall the whole worker.

use crate::{ServeState, auth::Principal};
use axum::{
    Json,
    extract::{Extension, Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use taiyaku_library::browse::{self, CoverTarget};

/// A `browse` failure, or a read that panicked, which is a 500 rather than
/// anything a caller can fix.
pub(crate) fn failed(what: &str, why: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("reading {what}: {why}"),
    )
        .into_response()
}

/// `read`, answered on the blocking pool over the library's connection — O116.
///
/// **One trip per request.** `series` and `one_series` each ask two questions,
/// and ask both here on one connection, so the mutex is waited on once and
/// nothing after the trip takes the library again. The response is built
/// inside it too.
///
/// A panic in `read` is a 500 naming `what`.
pub(crate) async fn on_the_library(
    state: Arc<ServeState>,
    what: &'static str,
    read: impl FnOnce(&rusqlite::Connection) -> Response + Send + 'static,
) -> Response {
    tokio::task::spawn_blocking(move || read(&state.library.conn()))
        .await
        .unwrap_or_else(|why| failed(what, &why.to_string()))
}

/// One work as the browse screens render it.
fn work_json(w: &browse::WorkCard) -> serde_json::Value {
    serde_json::json!({
        "id": w.id,
        "slug": w.slug,
        "title": w.title,
        "volume": w.volume,
        "aligned": w.aligned,
        "pages": w.pages,
        // **O84.** The edition this caller's reader will open, which is not
        // always the hub — see `browse::WorkCard::opens`. `library.ts`'s
        // `unreadable` asks about this edition rather than about all of them.
        "opens": w.opens,
        "editions": w.editions.iter().map(|e| serde_json::json!({
            "key": e.key,
            "label": e.label,
            "members": e.members,
            "pages": e.pages,
            "missing": e.missing,
            "book": e.book,
            "cover": e.page_zero_sha256.as_deref()
                .map(|sha256| cover_address(&w.slug, &e.key, sha256)),
        })).collect::<Vec<_>>(),
    })
}

/// `/api/works/{work}/{edition}/pages/0/thumb?v=…`, each segment and the
/// version encoded by [`uri_component`].
///
/// `v` is the cover version of page 0, offset 0 of the member `sha256` names,
/// which the thumbnail route answers `COVER_CACHE_CONTROL` for while it serves
/// that member's page. Encoded although a scan's hash is hex: an imported
/// sidecar's `sha256` is its own text, and a `&` or `+` left as it is would cut
/// `v` short or change it.
fn cover_address(work: &str, edition: &str, sha256: &str) -> String {
    format!(
        "/api/works/{}/{}/pages/0/thumb?v={}",
        uri_component(work),
        uri_component(edition),
        uri_component(&crate::cover::cover_version(sha256, 0)),
    )
}

/// `text` as JavaScript's `encodeURIComponent` writes it: every byte of its
/// UTF-8 as `%XX` but ASCII letters, digits and `-_.!~*'()`.
///
/// `encodeURIComponent`'s set because the web client builds a work's page
/// addresses with it, so the server's and the client's addresses for one
/// work spell its segments alike. `opds::percent_encode` escapes `!*'()` too.
fn uri_component(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    out
}

/// One series tile.
fn series_json(s: &browse::SeriesCard) -> serde_json::Value {
    serde_json::json!({
        "id": s.id,
        "title": s.title,
        "works": s.works,
        "editions": s.editions,
        "aligned": s.aligned,
        "added": s.added,
        "unread": s.unread,
        // `null` draws the missing-cover box, with no request: no edition of
        // the series is visible, or the one its cover is drawn from has no page.
        "cover": s.cover.as_ref().and_then(|cover| {
            cover
                .page_zero_sha256
                .as_deref()
                .map(|sha256| cover_address(&cover.slug, &cover.edition, sha256))
        }),
    })
}

/// The optional `?library=` the home lists carry.
#[derive(serde::Deserialize)]
pub(crate) struct HomeQuery {
    library: Option<i64>,
}

/// `GET /api/library/home` — continue reading, recently added, recently read.
pub(crate) async fn home(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    Query(query): Query<HomeQuery>,
) -> Response {
    on_the_library(state, "the home lists", move |conn| {
        match browse::home(conn, &principal.access, principal.user_id, query.library) {
            Ok(home) => Json(serde_json::json!({
                "continuing": home.continuing.iter().map(work_json).collect::<Vec<_>>(),
                "added": home.added.iter().map(work_json).collect::<Vec<_>>(),
                "read": home.read.iter().map(work_json).collect::<Vec<_>>(),
            }))
            .into_response(),
            Err(why) => failed("the home lists", &why),
        }
    })
    .await
}

/// The grid's `?library=`, and `?root=`, which is named here
/// only to be refused.
#[derive(serde::Deserialize)]
pub(crate) struct GridQuery {
    library: Option<i64>,
    root: Option<String>,
}

/// `GET /api/library/series` — the grid.
///
/// Returns `{ series, unfiled }`, per spec C1 §5.4: a hand-imported work has
/// no series, and a grid that only listed series would never show one.
///
/// **`?root=` is refused, 400, naming `library`**, before the library is
/// read. An address that carries it asks a question this route does not answer.
pub(crate) async fn series(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    Query(query): Query<GridQuery>,
) -> Response {
    if query.root.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            "the grid is narrowed by `library`, a library's id; `root` is no longer a parameter",
        )
            .into_response();
    }
    on_the_library(state, "the series listing", move |conn| {
        let series = match browse::series(conn, &principal.access, principal.user_id, query.library)
        {
            Ok(series) => series,
            Err(why) => return failed("the series listing", &why),
        };
        // **`query.library` here too.** Fetched without it, this would answer
        // a library-agnostic list beside a library-scoped grid — one response,
        // two different questions.
        // `browse` leaves every imported work out of it in SQL, from §5.1's
        // own NULL-excludes-itself rule: an imported member sits under no
        // directory, and asking for a library asks for what is in its directories.
        let unfiled =
            match browse::unfiled(conn, &principal.access, principal.user_id, query.library) {
                Ok(unfiled) => unfiled,
                Err(why) => return failed("the unfiled works", &why),
            };
        Json(serde_json::json!({
            "series": series.iter().map(series_json).collect::<Vec<_>>(),
            "unfiled": unfiled.iter().map(work_json).collect::<Vec<_>>(),
        }))
        .into_response()
    })
    .await
}

/// The optional `?library=` a series page carries: the library whose page it
/// was opened from (spec §3.6). It narrows the card and the works as it
/// narrows the grid, and a library holding none of the series is the 404.
#[derive(serde::Deserialize)]
pub(crate) struct SeriesQuery {
    library: Option<i64>,
}

/// `GET /api/library/series/{id}` — one series and its works.
///
/// **`series.titles` is on this response and on no other — O83.** Every title
/// the series holds, each `{title, kind, primary}`: the list
/// `PATCH /api/library/series/{id}` takes and replaces whole
/// (`librarydoc::set_series_titles`). `series.title` is one string, the
/// primary title, so a caller holding only that could send back a list of one
/// and delete the rest. A `false` `primary` is sent, where a document's own
/// `SeriesTitle` leaves it out, so the list goes back with nothing to fill
/// in. The grid and search answer the tile alone.
///
/// **Read only once `browse` has answered that this caller can see the
/// series**, so the 404 below still covers one the grant hides.
/// `series_title` has no per-root column to filter on, the same as
/// `series.title` itself.
pub(crate) async fn one_series(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
    Query(query): Query<SeriesQuery>,
) -> Response {
    on_the_library(state, "the series", move |conn| {
        match browse::one_series(
            conn,
            &principal.access,
            principal.user_id,
            id,
            query.library,
        ) {
            Ok(Some((card, works))) => {
                let titles = match taiyaku_library::librarydoc::series_titles(conn, card.id) {
                    Ok(titles) => titles,
                    Err(why) => return failed("the series titles", &why),
                };
                let mut series = series_json(&card);
                series["titles"] = serde_json::json!(
                    titles
                        .iter()
                        .map(|t| serde_json::json!({
                            "title": t.title,
                            "kind": t.kind,
                            "primary": t.primary,
                        }))
                        .collect::<Vec<_>>()
                );
                Json(serde_json::json!({
                    "series": series,
                    "works": works.iter().map(work_json).collect::<Vec<_>>(),
                }))
                .into_response()
            }
            // **404 rather than 403 for a series the grant hides.** Telling an
            // unauthorised caller which ids are real is the enumeration this
            // project refuses elsewhere; `browse` answers `None` for both cases
            // and this route cannot tell them apart either.
            Ok(None) => (StatusCode::NOT_FOUND, format!("no series {id}")).into_response(),
            Err(why) => failed("the series", &why),
        }
    })
    .await
}

/// `GET /api/library/works/{slug}` — one work.
pub(crate) async fn work(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(slug): AxPath<String>,
) -> Response {
    on_the_library(state, "the work", move |conn| {
        match browse::work(conn, &principal.access, principal.user_id, &slug) {
            Ok(Some(detail)) => {
                let mut body = work_json(&detail.card);
                body["alignments"] = serde_json::json!(
                    detail
                        .alignments
                        .iter()
                        .map(|a| serde_json::json!({
                            "from": a.from,
                            "to": a.to,
                            "verdict": a.verdict,
                            "slots": a.slots,
                            "paired": a.paired,
                        }))
                        .collect::<Vec<_>>()
                );
                Json(body).into_response()
            }
            Ok(None) => (StatusCode::NOT_FOUND, format!("no work `{slug}`")).into_response(),
            Err(why) => failed("the work", &why),
        }
    })
    .await
}

/// The `?q=` a search carries, and its optional `?library=`.
#[derive(serde::Deserialize)]
pub(crate) struct SearchQuery {
    #[serde(default)]
    q: String,
    library: Option<i64>,
}

/// `GET /api/library/search?q=` — trigram FTS5 over series titles and work
/// titles, with a `LIKE` scan underneath for a query too short for trigram to
/// answer at all. See `browse::search`. `?library=` narrows it as it narrows
/// the grid.
///
/// **The query is echoed back**, because spec C1 §8 wants the empty state to
/// name what was searched: a reader looking at zero results cannot tell "no
/// matches" from "the box didn't hold what I thought it did" without the
/// server saying back what it understood the query to be.
pub(crate) async fn search(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    Query(query): Query<SearchQuery>,
) -> Response {
    on_the_library(state, "the search", move |conn| {
        match browse::search(
            conn,
            &principal.access,
            principal.user_id,
            &query.q,
            query.library,
        ) {
            Ok(hits) => Json(serde_json::json!({
                "query": query.q,
                "series": hits.series.iter().map(series_json).collect::<Vec<_>>(),
                "unfiled": hits.unfiled.iter().map(work_json).collect::<Vec<_>>(),
            }))
            .into_response(),
            Err(why) => failed("the search", &why),
        }
    })
    .await
}

/// `GET /api/library/series/{id}/cover`.
///
/// **A redirect, not a second cover pipeline.** `cover.rs` already generates
/// and content-addresses an edition's first page, and the thumb route already
/// caches it, guards it and sets its `Cache-Control`. Building a parallel path
/// here would be a second implementation of all four, and spec C1 §5's own
/// words are that a series cover *is* an edition cover — so this resolves
/// which work stands for the series and hands the caller that work's existing
/// thumbnail.
///
/// 307 rather than 301: which work stands for a series changes when C3 sets a
/// cover, when a volume is added, or when a grant changes, and a permanent
/// redirect would be cached past all three.
///
/// **`?w=` asks for the cover at that long edge instead of the 160 px tile** —
/// O133. A phone's grid draws a cover at about 256 CSS px, 768 device pixels
/// at 3×. The redirect carries a `w` that is one of `cover::COVER_LONG_EDGES`
/// on to the tile route, which serves page 0 at that size; any other `w` gets
/// the tile. Not the page route: it refuses an account without
/// `can_stream`, and a download-only account must still see its
/// shelf (`routes.rs::refuse_if_cannot_stream`).
pub(crate) async fn cover(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
    Query(query): Query<Vec<(String, String)>>,
) -> Response {
    on_the_library(state, "the series cover", move |conn| {
        let target = match browse::cover_work(conn, &principal.access, id) {
            Ok(Some(target)) => target,
            Ok(None) => {
                return (StatusCode::NOT_FOUND, format!("series {id} has no cover"))
                    .into_response();
            }
            Err(why) => return failed("the series cover", &why),
        };

        // **`target.edition`, not `work.hub`.** A grant can hide the hub:
        // grant `Manga` (key `en`) and not `Manga (RAW)` (key `raw`, the
        // hub). `target.edition` is one this caller may see (see
        // `browse::cover_work` and `CoverTarget::edition`).
        let CoverTarget { slug, edition, .. } = target;
        // Each segment encoded as `cover_address` encodes it: a slug or a key
        // can hold a letter outside ASCII (O262), which a header cannot carry.
        let tile = format!(
            "/api/works/{}/{}/pages/0/thumb",
            uri_component(&slug),
            uri_component(&edition),
        );
        let location = match crate::routes::first_query_value(&query, "w")
            .and_then(|w| w.parse::<u32>().ok())
            .filter(|edge| crate::cover::COVER_LONG_EDGES.contains(edge))
        {
            Some(edge) => format!("{tile}?w={edge}"),
            None => tile,
        };
        (
            StatusCode::TEMPORARY_REDIRECT,
            [(axum::http::header::LOCATION, location)],
        )
            .into_response()
    })
    .await
}

/// `GET /api/library/status` — spec C1 §5.5: the libraries the caller may
/// see, each with its id, its name and whether it has been scanned,
/// and whether the caller sees only libraries granted to it (`restricted`).
///
/// Read-only. Two of spec C1 §8's first-run states are indistinguishable to
/// every other route, and a first-run state nobody can render is one that
/// ships broken to the only people who will ever see it.
pub(crate) async fn status(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
) -> Response {
    on_the_library(
        state,
        "the library status",
        move |conn| match browse::status(conn, &principal.access) {
            Ok(status) => Json(serde_json::json!({
                "libraries": status.libraries.iter().map(|l| serde_json::json!({
                    "id": l.id,
                    "name": l.name,
                    "scanned": l.scanned,
                })).collect::<Vec<_>>(),
                "restricted": status.restricted,
            }))
            .into_response(),
            Err(why) => failed("the library status", &why),
        },
    )
    .await
}

#[cfg(test)]
mod tests;
