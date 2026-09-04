//! The `/api/library/backups*` handlers — spec C4 §5.
//!
//! **A backup is the whole database, password hashes included**, and that is
//! stated here rather than left to be found: an administrator who downloads
//! one can read every account's PHC string offline at their leisure. That is
//! what a backup *is* — the same file an operator would copy off the disk —
//! and every route in this module is admin-gated on that basis. It is worth
//! naming because "the admin screen has a download button" does not, on its
//! face, look like "the admin screen hands out the password file".
//!
//! **No restore route.** C4 §5 puts restore deliberately out of band — stop
//! the server, move the file, start it — because a restore through a running
//! server would have to quiesce every writer in the process, and the value of
//! that over "stop the server" is not worth the failure modes it introduces.
//!
//! **No schedule.** The C4 build design §1 declines an in-server timer; a cron
//! entry runs `taiyaku library backup`.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path as AxPath, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use taiyaku_library::backup::{self, Backup};

use crate::{ServeState, auth::Principal, config_api::admin_or_forbidden};

/// The refusal every handler in this module opens with: an administrator,
/// **and a browser session rather than a device credential**.
///
/// **`admin_or_forbidden` alone was not enough, and this is the hole the fix
/// round found.** It reads `principal.role` and nothing else, so an
/// administrator's API key — the credential typed into a third-party OPDS
/// reader, hashed fast on the argument that it rides every page request —
/// reached every route here. Measured before the fix: `GET
/// /api/library/users` under `Authorization: Basic name:tyk_…` answered
/// **200** with every account's sessions, their IP addresses and their
/// user-agents.
///
/// Spec A2's second whole-branch review already closed exactly this for the
/// *self-service* account routes, and `session::refuse_a_device_credential`
/// is the helper it built: "a key typed into a third-party reader granted
/// minting further keys, listing the owner's browsers with their addresses,
/// and signing every one of them out". These routes are that capability's
/// **superset** — they do it to every account rather than to the key's own —
/// so a gate A2 thought necessary there cannot be optional here.
///
/// **Deliberately not extended to `config_api`, `curation` or `metadata`.**
/// Those edit the library's organisation and its shared documents, and
/// whether that counts as administering is **O58**, an open decision with a
/// spec argument on each side. Credentials are not that question: A2 decided
/// them, and this only applies its decision to the routes this branch adds.
fn administering_or_forbidden(
    principal: &Principal,
    extensions: &axum::http::Extensions,
) -> Option<Response> {
    admin_or_forbidden(principal).or_else(|| crate::session::refuse_a_device_credential(extensions))
}

/// Where this server's backups live, or `None` for a library that is not a
/// file.
///
/// **An in-memory library has no backup directory**, and that is a real state
/// rather than a test-only one: `Library::path` answers `None` for `:memory:`,
/// which is what most of this crate's own fixtures open.
///
/// **`Option`, with the refusal built separately by [`no_file_to_back_up`].**
/// A `Result<_, Response>` reads more directly and trips
/// `clippy::result_large_err`: an `axum` `Response` is 128 bytes, so every
/// success of this function would carry the size of a failure it does not
/// have.
fn directory(state: &ServeState) -> Option<std::path::PathBuf> {
    state.library.path().map(backup::directory_for)
}

/// The refusal all three routes answer when [`directory`] has nothing to give.
fn no_file_to_back_up() -> Response {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "error": "this library is in memory and has no file to back up"
        })),
    )
        .into_response()
}

fn backup_json(entry: &Backup) -> serde_json::Value {
    serde_json::json!({
        "name": entry.name,
        "bytes": entry.bytes,
        "taken": entry.taken,
    })
}

fn failed(what: &str, why: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": format!("{what}: {why}") })),
    )
        .into_response()
}

/// `GET /api/library/backups` — what exists, newest first.
pub(crate) async fn list_backups(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let Some(dir) = directory(&state) else {
        return no_file_to_back_up();
    };

    match tokio::task::spawn_blocking(move || backup::list(&dir)).await {
        Ok(Ok(found)) => Json(serde_json::json!({
            "backups": found.iter().map(backup_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Ok(Err(why)) => failed("listing backups", &why),
        Err(why) => failed("listing backups panicked", &why.to_string()),
    }
}

/// `POST /api/library/backups` — C4 §5's "run now".
///
/// **On `Library::working()`, not the server's own connection**, for the
/// reason the rule preview and the scan both take a second one: a
/// `VACUUM INTO` on the process's single mutexed connection stalls every
/// request for the length of a file copy.
///
/// **Two runs inside the same second collide**, and the second is refused
/// rather than overwriting the first — the name's timestamp has one-second
/// resolution, and a suffix counter would be machinery for a case only a
/// double-click produces. That refusal answers **409, not 500**: pressing
/// "back up now" twice is a person's timing, not a server fault, and this
/// route sent a 500 for it until the same misfiling was found three times
/// over in `users_api` and re-read here.
pub(crate) async fn create_backup(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let Some(dir) = directory(&state) else {
        return no_file_to_back_up();
    };

    let taken = tokio::task::spawn_blocking(move || {
        let working = state
            .library
            .working()
            .map_err(taiyaku_library::backup::TakeError::Failed)?;
        backup::take(&working, &dir)
    })
    .await;

    match taken {
        Ok(Ok(entry)) => Json(serde_json::json!({ "backup": backup_json(&entry) })).into_response(),
        Ok(Err(taiyaku_library::backup::TakeError::Exists(why))) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": why })),
        )
            .into_response(),
        Ok(Err(taiyaku_library::backup::TakeError::Failed(why))) => failed("taking a backup", &why),
        Err(why) => failed("taking a backup panicked", &why.to_string()),
    }
}

/// `GET /api/library/backups/{name}` — download one.
///
/// **`name` is resolved against [`backup::list`]'s own answer and is never
/// joined onto the directory.** A path-traversal check on a filename is a
/// thing that gets written wrong; matching a requested name against a listing
/// the server produced has no such failure mode, and the `readdir` it costs is
/// one this route wants anyway in order to answer 404 honestly.
///
/// Read whole into memory rather than streamed: the file is a SQLite database
/// an administrator asked for, and the alternative — a streaming body over a
/// file handle — buys nothing until somebody has measured a library big enough
/// for it to matter. Recorded rather than assumed.
pub(crate) async fn download_backup(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(name): AxPath<String>,
) -> Response {
    if let Some(refusal) = administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let Some(dir) = directory(&state) else {
        return no_file_to_back_up();
    };

    let read = tokio::task::spawn_blocking(move || {
        let found = backup::list(&dir)?;
        let Some(entry) = found.into_iter().find(|b| b.name == name) else {
            return Ok(None);
        };
        let path = dir.join(&entry.name);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some((entry, bytes))),
            Err(e) => Err(format!("reading the backup {}: {e}", entry.name)),
        }
    })
    .await;

    match read {
        Ok(Ok(Some((entry, bytes)))) => (
            [
                (header::CONTENT_TYPE, "application/vnd.sqlite3".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", entry.name),
                ),
            ],
            bytes,
        )
            .into_response(),
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "there is no such backup" })),
        )
            .into_response(),
        Ok(Err(why)) => failed("reading a backup", &why),
        Err(why) => failed("reading a backup panicked", &why.to_string()),
    }
}

#[cfg(test)]
mod tests;
