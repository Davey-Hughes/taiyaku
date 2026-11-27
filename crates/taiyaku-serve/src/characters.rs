//! The characters a tapped name names, as of the chapter being read: the
//! book route's checks, then [`characters::answer`] for the spine document
//! of the tapped block.

use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use taiyaku_library::account::Access;
use taiyaku_library::characters::{self, Answer, Asked, LONGEST_BEFORE, LONGEST_TEXT};

use crate::ServeState;

/// `GET /api/works/{work}/{edition}/book/characters?block=…&text=…`: who
/// `text` names, read from the tapped word in block `block` of book edition
/// `edition`, numbered as the book route numbers it, and from the block's
/// words before it when `before=…` sends them; [`tap`] reads the query.
///
/// Refused as [`crate::book::get_book_image`] refuses, and before the query
/// is read, so a hidden work or edition is 404 whatever the query says. With
/// cards off, haiyaku's tables are not known to be at the version this build
/// reads: they are left unread and the book is not known.
pub(crate) async fn get_characters(
    State(state): State<Arc<ServeState>>,
    AxPath((id, key)): AxPath<(String, String)>,
    Query(query): Query<Vec<(String, String)>>,
    extensions: axum::http::Extensions,
) -> Response {
    state.fingerprints.reader_seen();
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };
    let Some(work) = state.works.get(&id) else {
        return (StatusCode::NOT_FOUND, format!("no work `{id}`")).into_response();
    };
    if work.is_missing() {
        return crate::routes::missing_work(&id);
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_stream(&extensions) {
        return refusal;
    }
    let Some(access) = extensions
        .get::<crate::auth::Principal>()
        .map(|principal| principal.access.clone())
    else {
        return crate::routes::NoPrincipal.into_response();
    };
    let (edition, path) = match crate::book::book_edition(&state, &view, &work, &id, &key).await {
        Ok(found) => found,
        Err(refusal) => return refusal,
    };
    let tap = match tap(&query) {
        Ok(tap) => tap,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let block = tap.block;
    let what = format!("`{key}` of `{id}`");
    let sha256 = edition.members[0].sha256.clone();
    let looked =
        tokio::task::spawn_blocking(move || looked_up(&state, &sha256, &path, &tap, &access))
            .await
            .map_err(|why| why.to_string())
            .and_then(|looked| looked);
    match looked {
        Ok(Some(answer)) => axum::Json(answer).into_response(),
        Ok(None) => (
            StatusCode::BAD_REQUEST,
            format!("block {block} is past the end of {what}"),
        )
            .into_response(),
        Err(why) => crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the characters of {what} could not be read"),
            &why,
        ),
    }
}

/// [`get_characters`]' answer to `tap` in the book with digest `sha256` at
/// `path`, or `None` where the book has no block `tap.block`: the part of
/// the route that blocks, for the blocking pool.
///
/// The fold reads on a connection of its own
/// ([`taiyaku_library::Library::working`]): it reads every event of a series
/// up to the tapped chapter, and may not hold the shared connection's lock,
/// which every request waits on, for longer than a page request does (the
/// character cards design, §7).
fn looked_up(
    state: &ServeState,
    sha256: &str,
    path: &std::path::Path,
    tap: &Tap,
    access: &Access,
) -> Result<Option<Answer>, String> {
    let Some(document) = crate::book::document_at(sha256, path, tap.block)? else {
        return Ok(None);
    };
    if state.library.cards_off().is_some() {
        return Ok(Some(Answer::unknown()));
    }
    let asked = Asked {
        book: sha256,
        document: &document,
        text: &tap.text,
        before: &tap.before,
        earlier: tap.earlier,
        series: tap.series.as_deref(),
        order: tap.order.as_deref(),
    };
    // An in-memory library's working connection is its shared one, held
    // until this returns: anything below that asked for `Library::conn`
    // would wait on this thread's own lock.
    let working = state.library.working()?;
    characters::answer(&working, &asked, access, state.library.titles()).map(Some)
}

/// What a reader tapped, as [`get_characters`]' query says it.
#[derive(Debug)]
struct Tap {
    block: usize,
    text: String,
    before: String,
    earlier: bool,
    series: Option<String>,
    order: Option<String>,
}

/// `query` read as a [`Tap`], or why it is refused with 400. `before` may
/// be absent or empty, as it is for a block's first word. `earlier` is on
/// unless it is `0`, as the reader's setting is by default, and a value
/// other than `0` or `1` is refused rather than guessed; an empty `series`
/// or `order` names none, and a key not read here is ignored.
fn tap(query: &[(String, String)]) -> Result<Tap, String> {
    let value = |key: &str| crate::routes::first_query_value(query, key);
    let block = value("block")
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| "`block` is a block's number in the edition, from 0".to_owned())?;
    // The bound the client cuts its text to, counted as it counts it.
    let text = value("text")
        .filter(|text| !text.is_empty() && text.encode_utf16().count() <= LONGEST_TEXT)
        .ok_or_else(|| format!("`text` is 1 to {LONGEST_TEXT} UTF-16 units"))?;
    let before = value("before").unwrap_or_default();
    if before.encode_utf16().count() > LONGEST_BEFORE {
        return Err(format!("`before` is at most {LONGEST_BEFORE} UTF-16 units"));
    }
    let earlier = match value("earlier") {
        None | Some("1") => true,
        Some("0") => false,
        Some(_) => return Err("`earlier` is 1 or 0".to_owned()),
    };
    let named = |key: &str| {
        value(key)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    };
    Ok(Tap {
        block,
        text: text.to_owned(),
        before: before.to_owned(),
        earlier,
        series: named("series"),
        order: named("order"),
    })
}

#[cfg(test)]
mod tests;
