//! The `/api/library/*` handlers. Spec C1 §5.
//!
//! Thin by design: parse, call [`taiyaku_library::browse`], serialise. Every
//! decision about *what* a browse screen may see lives in that module, in SQL,
//! so there is one place the root filter is applied and one place to read in
//! order to know whether it is.
//!
//! Kept out of `routes.rs`, which is already over 1,500 lines and holds the
//! reader's own routes — the two share no state and have no reason to be read
//! together.
//!
//! # Every handler takes the principal's roots and passes them straight down
//!
//! `Principal.roots` is the caller's grant, and **empty means unrestricted**
//! (spec C1 §5.1). Nothing here interprets it; it goes to `browse`, which
//! applies it in SQL. This module is where spec A2's
//! `#[expect(dead_code)]` on that field finally comes off.

use crate::{ServeState, auth::Principal};
use axum::{
    Json,
    extract::{Extension, Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use taiyaku_library::browse::{self, CoverTarget};

/// A `browse` failure, which is a 500 rather than anything a caller can fix.
fn failed(what: &str, why: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("reading {what}: {why}"),
    )
        .into_response()
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
        "editions": w.editions.iter().map(|e| serde_json::json!({
            "key": e.key,
            "label": e.label,
            "members": e.members,
            "pages": e.pages,
        })).collect::<Vec<_>>(),
    })
}

/// One series tile.
fn series_json(s: &browse::SeriesCard) -> serde_json::Value {
    serde_json::json!({
        "id": s.id,
        "title": s.title,
        "works": s.works,
        "editions": s.editions,
        "aligned": s.aligned,
        "cover": format!("/api/library/series/{}/cover", s.id),
    })
}

/// `GET /api/library/home` — continue reading, recently added, recently read.
pub(crate) async fn home(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
) -> Response {
    let conn = state.library.conn();
    match browse::home(&conn, &principal.roots, principal.user_id) {
        Ok(home) => Json(serde_json::json!({
            "continuing": home.continuing.iter().map(work_json).collect::<Vec<_>>(),
            "added": home.added.iter().map(work_json).collect::<Vec<_>>(),
            "read": home.read.iter().map(work_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(why) => failed("the home lists", &why),
    }
}

/// `GET /api/library/series` — the grid.
///
/// Returns `{ series, unfiled }`, per spec C1 §5.4: a hand-imported work has
/// no series, and a grid that only listed series would never show one.
pub(crate) async fn series(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
) -> Response {
    let conn = state.library.conn();
    let series = match browse::series(&conn, &principal.roots) {
        Ok(series) => series,
        Err(why) => return failed("the series listing", &why),
    };
    let unfiled = match browse::unfiled(&conn, &principal.roots) {
        Ok(unfiled) => unfiled,
        Err(why) => return failed("the unfiled works", &why),
    };
    Json(serde_json::json!({
        "series": series.iter().map(series_json).collect::<Vec<_>>(),
        "unfiled": unfiled.iter().map(work_json).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// `GET /api/library/series/{id}` — one series and its works.
pub(crate) async fn one_series(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
) -> Response {
    let conn = state.library.conn();
    match browse::one_series(&conn, &principal.roots, id) {
        Ok(Some((card, works))) => Json(serde_json::json!({
            "series": series_json(&card),
            "works": works.iter().map(work_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        // **404 rather than 403 for a series the grant hides.** Telling an
        // unauthorised caller which ids are real is the enumeration this
        // project refuses elsewhere; `browse` answers `None` for both cases
        // and this route cannot tell them apart either.
        Ok(None) => (StatusCode::NOT_FOUND, format!("no series {id}")).into_response(),
        Err(why) => failed("the series", &why),
    }
}

/// `GET /api/library/works/{slug}` — one work.
pub(crate) async fn work(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(slug): AxPath<String>,
) -> Response {
    let conn = state.library.conn();
    match browse::work(&conn, &principal.roots, &slug) {
        Ok(Some(card)) => Json(work_json(&card)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, format!("no work `{slug}`")).into_response(),
        Err(why) => failed("the work", &why),
    }
}

/// The `?q=` a search carries.
#[derive(serde::Deserialize)]
pub(crate) struct SearchQuery {
    #[serde(default)]
    q: String,
}

/// `GET /api/library/search?q=` — FTS5 over series titles and work titles.
///
/// **The query is echoed back**, because spec C1 §8 wants the empty state to
/// name what was searched: FTS5's default tokeniser does not segment CJK, so a
/// query can match nothing for reasons that are invisible unless it is shown.
pub(crate) async fn search(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    Query(query): Query<SearchQuery>,
) -> Response {
    let conn = state.library.conn();
    match browse::search(&conn, &principal.roots, &query.q) {
        Ok(hits) => Json(serde_json::json!({
            "query": query.q,
            "series": hits.series.iter().map(series_json).collect::<Vec<_>>(),
            "unfiled": hits.unfiled.iter().map(work_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(why) => failed("the search", &why),
    }
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
pub(crate) async fn cover(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
) -> Response {
    let conn = state.library.conn();
    let target = match browse::cover_work(&conn, &principal.roots, id) {
        Ok(Some(target)) => target,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, format!("series {id} has no cover")).into_response();
        }
        Err(why) => return failed("the series cover", &why),
    };
    drop(conn);

    // **`target.edition`, not `work.hub`.** An earlier version redirected to
    // the hub, which a grant can hide: grant `Manga` (key `en`) and not
    // `Manga (RAW)` (key `raw`, the hub) and the cover served page 0 of the
    // edition the reader cannot see — with `cover_work`'s own doc comment
    // promising the opposite. Whole-branch review, 2026-08-20.
    let CoverTarget { slug, edition, .. } = target;
    (
        StatusCode::TEMPORARY_REDIRECT,
        [(
            axum::http::header::LOCATION,
            format!("/api/works/{slug}/{edition}/pages/0/thumb"),
        )],
    )
        .into_response()
}

#[cfg(test)]
mod tests;
