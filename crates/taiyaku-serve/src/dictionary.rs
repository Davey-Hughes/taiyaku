//! `GET /api/dictionary/{lang}/{word}` and `.../{lang}/at/{text}`: a word, or
//! the word a text begins with, looked up in the dictionary `taiyaku
//! dictionary import` built for its language, on this server, so a reader's
//! word never leaves it.

use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::ServeState;

/// The dictionary file for `lang`, or the answer that says there is none.
/// 404 where this server has no dictionary for the language, which is how a
/// reader learns to say so.
fn dictionary_for(state: &ServeState, lang: &str) -> Result<std::path::PathBuf, Box<Response>> {
    // A book's language may carry a region, `ru-RU`; dictionaries are kept
    // by the language alone, and its name is a file's.
    let lang = lang
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                format!("`{lang}` is no language code"),
            )
                .into_response(),
        ));
    }
    state
        .dictionaries
        .as_ref()
        .map(|dir| dir.join(format!("{lang}.db")))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            Box::new(
                (
                    StatusCode::NOT_FOUND,
                    format!("this server has no dictionary for {lang}"),
                )
                    .into_response(),
            )
        })
}

fn entries_json(found: Vec<taiyaku_library::dictionary::Found>) -> Vec<serde_json::Value> {
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
        .collect()
}

fn unreadable(why: &str) -> Response {
    crate::routes::answered_and_logged(
        StatusCode::INTERNAL_SERVER_ERROR,
        "the dictionary could not be read".into(),
        why,
    )
}

fn open(path: &std::path::Path) -> Result<rusqlite::Connection, String> {
    rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// What the dictionary for `lang` has for `word`: each entry's headword,
/// part of speech and senses, and the form it was found by where that is not
/// the headword. 404 where this server has no dictionary for the language.
pub(crate) async fn lookup(
    State(state): State<Arc<ServeState>>,
    AxPath((lang, word)): AxPath<(String, String)>,
) -> Response {
    let path = match dictionary_for(&state, &lang) {
        Ok(path) => path,
        Err(refused) => return *refused,
    };
    let found = tokio::task::spawn_blocking(move || {
        taiyaku_library::dictionary::lookup(&open(&path)?, &word)
    })
    .await;
    match found {
        Ok(Ok(found)) => axum::Json(entries_json(found)).into_response(),
        Ok(Err(why)) => unreadable(&why),
        Err(why) => unreadable(&why.to_string()),
    }
}

/// The longest word `text` begins with in the dictionary for `lang`, for a
/// tap on a book: `{"length", "entries"}`, the length in UTF-16 units because
/// the reader's offsets are. 400 where `text` is over `LONGEST_PREFIX` code
/// points, the most a word is tried at.
pub(crate) async fn lookup_at(
    State(state): State<Arc<ServeState>>,
    AxPath((lang, text)): AxPath<(String, String)>,
) -> Response {
    let path = match dictionary_for(&state, &lang) {
        Ok(path) => path,
        Err(refused) => return *refused,
    };
    let most = taiyaku_library::dictionary::LONGEST_PREFIX;
    if text.chars().count() > most {
        return (
            StatusCode::BAD_REQUEST,
            format!("text is at most {most} code points"),
        )
            .into_response();
    }
    let found = tokio::task::spawn_blocking(move || {
        let (chars, found) = taiyaku_library::dictionary::longest_at(&open(&path)?, &text)?;
        let units: usize = text.chars().take(chars).map(char::len_utf16).sum();
        Ok::<_, String>((units, found))
    })
    .await;
    match found {
        Ok(Ok((length, found))) => {
            axum::Json(serde_json::json!({"length": length, "entries": entries_json(found)}))
                .into_response()
        }
        Ok(Err(why)) => unreadable(&why),
        Err(why) => unreadable(&why.to_string()),
    }
}

#[cfg(test)]
mod tests;
