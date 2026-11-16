//! `GET /api/dictionary/{lang}/{word}`: a word looked up in the dictionary
//! `taiyaku dictionary import` built for its language, on this server, so a
//! reader's word never leaves it.

use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::ServeState;

/// What the dictionary for `lang` has for `word`: each entry's headword,
/// part of speech and senses, and the form it was found by where that is not
/// the headword. 404 where this server has no dictionary for the language,
/// which is how a reader learns to say so.
pub(crate) async fn lookup(
    State(state): State<Arc<ServeState>>,
    AxPath((lang, word)): AxPath<(String, String)>,
) -> Response {
    // A book's language may carry a region, `ru-RU`; dictionaries are kept
    // by the language alone, and its name is a file's.
    let lang = lang
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return (
            StatusCode::BAD_REQUEST,
            format!("`{lang}` is no language code"),
        )
            .into_response();
    }
    let Some(path) = state
        .dictionaries
        .as_ref()
        .map(|dir| dir.join(format!("{lang}.db")))
        .filter(|path| path.is_file())
    else {
        return (
            StatusCode::NOT_FOUND,
            format!("this server has no dictionary for {lang}"),
        )
            .into_response();
    };
    let found = tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        taiyaku_library::dictionary::lookup(&conn, &word)
    })
    .await;
    match found {
        Ok(Ok(found)) => axum::Json(
            found
                .into_iter()
                .map(|f| {
                    serde_json::json!({
                        "word": f.word,
                        "pos": f.pos,
                        "glosses": f.glosses,
                        "via": f.via,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Ok(Err(why)) => crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the dictionary could not be read".into(),
            &why,
        ),
        Err(why) => crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the dictionary could not be read".into(),
            &why.to_string(),
        ),
    }
}

#[cfg(test)]
mod tests;
