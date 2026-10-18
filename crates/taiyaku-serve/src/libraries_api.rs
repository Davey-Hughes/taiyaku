//! The `/api/library/libraries*` handlers: Configuration → Libraries, §6 of
//! `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library that exists is addressed by its id**, in the path. A name is
//! read only to make a library or to rename one: a name of white space alone
//! keeps its ends, which migration 16 strips from every other, and an id is
//! never given to a later library.
//!
//! **The folder rows inside a library's card are still `config_api`'s.** A
//! folder's settings, its own scan and its removal are routes under
//! `/api/library/roots/{id}`. This module makes libraries, adds folders to
//! them, removes them whole, scans all of a library's folders at once, and
//! moves a folder from one library to another (`/api/library/roots/{id}/move`).
//!
//! **A refusal these handlers make is `{"error": "<sentence>"}`**: 400, 404,
//! 409 and 500, including the ones axum would otherwise answer in plain text
//! (a body that is not JSON, an id that is not a number, a query it cannot
//! read). The body is read from bytes here, and the id from the path as text,
//! for that reason. A reader's 403 is each handler's own first statement, and
//! is plain text where a device credential's is JSON. A refusal made before a
//! handler runs is not in this shape: an over-size body's 413, a path that is
//! not UTF-8, and a method a route does not mount. The guard's 401 is
//! `{"error":"unauthorized"}`. The one refusal with a second key is a move's
//! 409, `{"error", "volumes"}`, which lists the volumes that stand in its way.
//!
//! Every read or write of the library is made on the blocking pool, and a
//! handler that derives or probes takes
//! [`taiyaku_library::Library::working`], for the reasons `config_api`'s own
//! doc gives.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json,
    body::Bytes,
    extract::{Extension, Path as AxPath, Query, RawQuery, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use taiyaku_library::{libraries, relocate, scan};

use crate::config_api::{
    AdminAnswer, DryRunQuery, added_json, admin_panicked, derive_report_json, removal_json,
    root_json, roots_with_last_scan,
};
use crate::{ServeState, auth::Principal};

fn added_error(error: scan::AddRootError) -> AdminAnswer {
    match error {
        scan::AddRootError::Unusable(why) => AdminAnswer::Unusable(why),
        scan::AddRootError::Failed(why) => AdminAnswer::Failed(why),
    }
}

/// The request body as JSON, or the caller's mistake.
fn json_body(bytes: &Bytes) -> Result<serde_json::Value, AdminAnswer> {
    serde_json::from_slice(bytes)
        .map_err(|e| AdminAnswer::Unusable(format!("the request body must be JSON: {e}")))
}

/// A library id from the path, which is a whole number.
fn library_id(raw: &str) -> Result<i64, AdminAnswer> {
    raw.parse().map_err(|_| {
        AdminAnswer::Unusable(format!(
            "`{raw}` is not a library id; an id is a whole number"
        ))
    })
}

/// A library's `name`, or `missing` as the caller's mistake: each route says
/// what it was asking for.
fn name_from_json(body: &serde_json::Value, missing: &str) -> Result<String, AdminAnswer> {
    body.get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AdminAnswer::Unusable(missing.to_string()))
}

/// The folders of library `id`, oldest first, or [`AdminAnswer::MissingLibrary`]
/// where there is no such library.
fn folders_of(conn: &rusqlite::Connection, id: i64) -> Result<Vec<scan::Root>, AdminAnswer> {
    if libraries::name_of(conn, id)
        .map_err(AdminAnswer::Failed)?
        .is_none()
    {
        return Err(AdminAnswer::MissingLibrary(id));
    }
    Ok(scan::roots(conn)
        .map_err(AdminAnswer::Failed)?
        .into_iter()
        .filter(|root| root.library_id == id)
        .collect())
}

/// Every library as the screen draws it, by name: its id, name, works, the
/// EPUB and PDF entries its scans recorded, and each folder in
/// [`root_json`]'s shape, oldest first.
fn libraries_json(conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>, String> {
    let roots = roots_with_last_scan(conn)?;
    let mut rendered = Vec::new();
    for listed in libraries::list(conn)? {
        let folders: Vec<serde_json::Value> = roots
            .iter()
            .filter(|(root, _, _)| root.library_id == listed.id)
            .map(|(root, at, status)| root_json(root, at.as_deref(), status.as_deref()))
            .collect();
        rendered.push(serde_json::json!({
            "id": listed.id,
            "name": listed.name,
            "works": listed.works,
            "books": libraries::books(conn, listed.id)?,
            "folders": folders,
        }));
    }
    Ok(rendered)
}

/// `GET /api/library/libraries`.
pub(crate) async fn list_libraries(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        libraries_json(&conn).map_err(AdminAnswer::Failed)
    })
    .await;

    match built {
        Ok(Ok(libraries)) => Json(serde_json::json!({ "libraries": libraries })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("listing libraries", &why),
    }
}

/// `name` and `folders`, as [`create_library`]'s body is read.
fn new_library_from_json(body: &serde_json::Value) -> Result<(String, Vec<PathBuf>), AdminAnswer> {
    let name = name_from_json(body, "a library needs a `name`, a string")?;
    let listed = body
        .get("folders")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            AdminAnswer::Unusable("a library needs `folders`, an array of paths".to_string())
        })?;
    let mut folders = Vec::with_capacity(listed.len());
    for entry in listed {
        folders.push(absolute_folder(entry.as_str().ok_or_else(|| {
            AdminAnswer::Unusable("a library's `folders` must be an array of paths".to_string())
        })?)?);
    }
    Ok((name, folders))
}

/// `POST /api/library/libraries`: a library and its folders, all of them or
/// nothing. Answers the library as the list would, and each folder's watch
/// probe, which runs on the working connection, not the shared one.
pub(crate) async fn create_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    body: Bytes,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let (name, folders) = match json_body(&body).and_then(|body| new_library_from_json(&body)) {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let created = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        let made = scan::create_library(&working, &name, &folders).map_err(added_error)?;
        let library = libraries_json(&working)
            .map_err(AdminAnswer::Failed)?
            .into_iter()
            .find(|library| library["id"] == made.library_id)
            .ok_or_else(|| {
                AdminAnswer::Failed(format!(
                    "library {} was made and is not listed",
                    made.library_id
                ))
            })?;
        Ok::<_, AdminAnswer>(serde_json::json!({
            "library": library,
            "added": made.folders.iter().map(added_json).collect::<Vec<_>>(),
        }))
    })
    .await;

    match created {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("creating a library", &why),
    }
}

/// `PATCH /api/library/libraries/{id}`: rename. Answers the name as stored.
pub(crate) async fn patch_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    body: Bytes,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let read = library_id(&id).and_then(|id| {
        let body = json_body(&body)?;
        Ok((
            id,
            name_from_json(&body, "renaming a library needs a `name`, a string")?,
        ))
    });
    let (id, name) = match read {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let renamed = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        libraries::rename(&conn, id, &name).map_err(|error| match error {
            libraries::RenameError::Missing(id) => AdminAnswer::MissingLibrary(id),
            libraries::RenameError::Unusable(why) => AdminAnswer::Unusable(why),
            libraries::RenameError::Failed(why) => AdminAnswer::Failed(why),
        })
    })
    .await;

    match renamed {
        Ok(Ok(name)) => Json(serde_json::json!({ "id": id, "name": name })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("renaming a library", &why),
    }
}

/// A folder's path as a request gives it, which must be absolute.
///
/// The store resolves a relative path against the server's working directory,
/// which is the command line's own word and not a request's, and answers an
/// empty one with an operating-system error that names nothing, so neither is
/// passed on.
fn absolute_folder(raw: &str) -> Result<PathBuf, AdminAnswer> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(AdminAnswer::Unusable(format!(
            "a folder's path must be absolute, and `{raw}` is not"
        )))
    }
}

/// `path`, as [`add_folder`]'s body is read.
fn folder_path_from_json(body: &serde_json::Value) -> Result<PathBuf, AdminAnswer> {
    let raw = body
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AdminAnswer::Unusable("adding a folder needs a `path`, a string".to_string())
        })?;
    absolute_folder(raw)
}

/// `POST /api/library/libraries/{id}/folders`: add a folder to this library,
/// through [`scan::add_root_checked_in`]'s validations.
pub(crate) async fn add_folder(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    body: Bytes,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let read = library_id(&id).and_then(|id| {
        let body = json_body(&body)?;
        Ok((id, folder_path_from_json(&body)?))
    });
    let (id, path) = match read {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let added = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        folders_of(&working, id)?;
        scan::add_root_checked_in(&working, &path, scan::InLibrary::Id(id)).map_err(added_error)
    })
    .await;

    match added {
        Ok(Ok(added)) => Json(added_json(&added)).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("adding a folder", &why),
    }
}

/// `DELETE /api/library/libraries/{id}`: remove a library, or with
/// `?dry_run=true` answer what that would cost. The body is
/// `config_api::delete_root`'s, so one dialog's types read both.
///
/// **A scan reserved on any of its folders refuses it**, 409, for the dry run
/// as for the removal. A removal that leaves an account with no library goes
/// ahead, and the body names those accounts.
pub(crate) async fn delete_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    query: Result<Query<DryRunQuery>, QueryRejection>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let id = match library_id(&id) {
        Ok(id) => id,
        Err(why) => return why.into_response(),
    };
    let dry_run = match query {
        Ok(Query(query)) => query.dry_run,
        Err(rejection) => {
            return AdminAnswer::Unusable(format!(
                "the query could not be read: {}",
                rejection.body_text()
            ))
            .into_response();
        }
    };

    let scans_state = Arc::clone(&state);
    let removed = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        let folders = folders_of(&working, id)?;
        if let Some(root) = crate::scan_jobs::a_scan_reserved_on(&state, &folders) {
            return Err(AdminAnswer::Refused(crate::scan_jobs::removal_waits_for(
                root,
                "the library",
            )));
        }
        let folders: Vec<i64> = folders.iter().map(|root| root.id).collect();
        let (removal, derived) = if dry_run {
            scan::library_removal_cost(&working, id)
        } else {
            scan::remove_library(&working, id)
        }
        .map_err(AdminAnswer::Failed)?;
        Ok((removal, derived, folders))
    })
    .await;

    // A freed `root.id` is given to the next folder added, so a removal that
    // answers `Ok` clears each folder's slot in the scan registry, as
    // `config_api::delete_root` clears one. The slots are cleared when the
    // removal answers, not when it commits: a removal that commits and then
    // fails its reconcile derive answers 500 with the library gone and the
    // slots left.
    if !dry_run && let Ok(Ok((_, _, folders))) = &removed {
        let mut scans = scans_state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for folder in folders {
            scans.remove(folder);
        }
    }

    match removed {
        Ok(Ok((removal, derived, _))) => Json(serde_json::json!({
            "removed": removal_json(&removal),
            "derive": derive_report_json(&derived),
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("removing a library", &why),
    }
}

/// The folders of library `id` for a scan route, or the answer to give.
async fn scan_folders(
    state: &Arc<ServeState>,
    id: i64,
    what: &'static str,
) -> Result<Vec<scan::Root>, AdminAnswer> {
    let lookup_state = Arc::clone(state);
    let found = tokio::task::spawn_blocking(move || {
        let conn = lookup_state.library.conn();
        folders_of(&conn, id)
    })
    .await;
    match found {
        Ok(found) => found,
        Err(why) => Err(AdminAnswer::Failed(format!("{what} panicked: {why}"))),
    }
}

/// `POST /api/library/libraries/{id}/scan`: walk each folder of the library
/// in the background, derive once, and answer at once.
///
/// **The scan holds the registry slot of each folder.** A scan already
/// running on one folder refuses the library's, naming the folder, with no
/// slot taken for the others. And while the library's runs,
/// `config_api::start_scan` refuses each folder as already being scanned.
/// A folder's slot is released when the whole scan ends, since the one derive
/// is part of each folder's report.
pub(crate) async fn start_library_scan(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    RawQuery(query): RawQuery,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let id = match library_id(&id) {
        Ok(id) => id,
        Err(why) => return why.into_response(),
    };
    if let Some(query) = query.filter(|query| !query.is_empty()) {
        return AdminAnswer::Unusable(format!(
            "starting a scan takes no query, and `?{query}` was sent; every scan walks the \
             whole of each folder"
        ))
        .into_response();
    }
    let roots = match scan_folders(&state, id, "looking up a library to scan").await {
        Ok(roots) => roots,
        Err(answer) => return answer.into_response(),
    };

    let started = Instant::now();
    if let Err(refused) = crate::scan_jobs::reserve(&state, &roots, started) {
        return AdminAnswer::Refused(refused).into_response();
    }
    crate::scan_jobs::spawn(state, roots, started, crate::scan_jobs::Reread::Library(id));

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "library_id": id,
            "status_url": format!("/api/library/libraries/{id}/scan"),
        })),
    )
        .into_response()
}

/// `GET /api/library/libraries/{id}/scan`: whether a scan of any of the
/// library's folders is running, and which. A folder's report is read from
/// that folder's own status route.
pub(crate) async fn library_scan_status(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let id = match library_id(&id) {
        Ok(id) => id,
        Err(why) => return why.into_response(),
    };
    let roots = match scan_folders(&state, id, "reading a library's scan status").await {
        Ok(roots) => roots,
        Err(answer) => return answer.into_response(),
    };

    let folders: Vec<(i64, Option<std::time::Duration>)> = {
        let scans = state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        roots
            .iter()
            .map(|root| {
                let running = scans
                    .get(&root.id)
                    .filter(|progress| !progress.done)
                    .map(|progress| progress.started.elapsed());
                (root.id, running)
            })
            .collect()
    };
    let elapsed = folders.iter().filter_map(|(_, running)| *running).max();

    Json(serde_json::json!({
        "library_id": id,
        "running": elapsed.is_some(),
        "elapsed_secs": elapsed.map(|e| e.as_secs_f64()),
        "folders": folders
            .iter()
            .map(|(root_id, running)| serde_json::json!({
                "root_id": root_id,
                "running": running.is_some(),
            }))
            .collect::<Vec<_>>(),
    }))
    .into_response()
}

/// Where a move's body says the folder goes.
enum MoveTo {
    Library(i64),
    New(String),
}

/// Exactly one of `library`, a library's id, and `new_library`, a name.
fn move_target_from_json(body: &serde_json::Value) -> Result<MoveTo, AdminAnswer> {
    match (body.get("library"), body.get("new_library")) {
        (Some(id), None) => id.as_i64().map(MoveTo::Library).ok_or_else(|| {
            AdminAnswer::Unusable("`library` must be a library id, a whole number".to_string())
        }),
        (None, Some(name)) => name
            .as_str()
            .map(|name| MoveTo::New(name.to_string()))
            .ok_or_else(|| {
                AdminAnswer::Unusable("`new_library` must be a name, a string".to_string())
            }),
        _ => Err(AdminAnswer::Unusable(
            "a move names exactly one of `library`, a library id, and `new_library`, a name"
                .to_string(),
        )),
    }
}

/// [`relocate::Moved`], over the wire.
///
/// Destructured with no `..`, so that a field added to it is a compile error
/// here and not a count the screen never draws.
fn moved_json(moved: &relocate::Moved) -> serde_json::Value {
    let relocate::Moved {
        library_id,
        library,
        works_kept,
        works_joined,
        works_split,
        series_joined,
        positions,
        notes,
        alignments_dropped,
        library_removed,
        grants_removed,
        left_with_no_library,
    } = moved;
    serde_json::json!({
        "library_id": library_id,
        "library": library,
        "works_kept": works_kept,
        "works_joined": works_joined,
        "works_split": works_split,
        "series_joined": series_joined,
        "positions": positions,
        "notes": notes,
        "alignments_dropped": alignments_dropped,
        "library_removed": library_removed,
        "grants_removed": grants_removed,
        "left_with_no_library": left_with_no_library,
    })
}

/// `POST /api/library/roots/{id}/move`: move a folder to another library or
/// to a new one, or with `?dry_run=true` answer what that would do. `body` is
/// `{"library": id}` or `{"new_library": name}`; [`relocate`] is where the
/// move is.
///
/// **A scan reserved on any folder of either library refuses it**, the folder's
/// own library and the target, as [`delete_library`] refuses a removal: a scan
/// that is queued holds no job lock, so the lock alone would not see it.
///
/// **Then it asks for [`ServeState::library_job`] and does not wait for it.**
/// A move derives the whole store, and under a scan's derive or a rule save it
/// would file works by rules or facts the other is replacing. So it answers
/// 409 at once where one holds the lock, as `config_api::save_rules` does, for
/// the rehearsal as for the move: the rehearsal runs the same derives.
///
/// **A refusal is a 409 with its sentence and its volumes**, all of them
/// where the sentence names the first few. Where a scan or a rule save is the
/// reason, `volumes` is empty.
pub(crate) async fn move_folder(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    query: Result<Query<DryRunQuery>, QueryRejection>,
    body: Bytes,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let id = match id.parse::<i64>() {
        Ok(id) => id,
        Err(_) => {
            return AdminAnswer::Unusable(format!(
                "`{id}` is not a folder id; an id is a whole number"
            ))
            .into_response();
        }
    };
    let dry_run = match query {
        Ok(Query(query)) => query.dry_run,
        Err(rejection) => {
            return AdminAnswer::Unusable(format!(
                "the query could not be read: {}",
                rejection.body_text()
            ))
            .into_response();
        }
    };
    let to = match json_body(&body).and_then(|body| move_target_from_json(&body)) {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let moved = tokio::task::spawn_blocking(move || {
        let working = state
            .library
            .working()
            .map_err(relocate::MoveError::Failed)?;
        let roots = scan::roots(&working).map_err(relocate::MoveError::Failed)?;
        let Some(folder) = roots.iter().find(|root| root.id == id) else {
            return Err(relocate::MoveError::Missing(format!(
                "there is no folder {id}"
            )));
        };
        let held_up = roots.iter().filter(|root| {
            root.library_id == folder.library_id
                || matches!(to, MoveTo::Library(library) if root.library_id == library)
        });
        let held_up: Vec<scan::Root> = held_up.cloned().collect();
        if let Some(root) = crate::scan_jobs::a_scan_reserved_on(&state, &held_up) {
            return Err(relocate::MoveError::Refused {
                why: crate::scan_jobs::waits_for(root, "move the folder"),
                volumes: Vec::new(),
            });
        }
        let _job = match state.library_job.try_lock() {
            Ok(held) => held,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(relocate::MoveError::Refused {
                    why: "a scan or a rule save is running; move the folder when it has \
                          finished"
                        .to_string(),
                    volumes: Vec::new(),
                });
            }
        };
        let target = match &to {
            MoveTo::Library(library) => relocate::Target::Library(*library),
            MoveTo::New(name) => relocate::Target::New(name),
        };
        if dry_run {
            relocate::move_cost(&working, id, target)
        } else {
            relocate::move_root(&working, id, target)
        }
    })
    .await;

    match moved {
        Ok(Ok((moved, derived))) => Json(serde_json::json!({
            "moved": moved_json(&moved),
            "derive": derive_report_json(&derived),
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Err(relocate::MoveError::Refused { why, volumes })) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": why, "volumes": volumes })),
        )
            .into_response(),
        Ok(Err(relocate::MoveError::Missing(why))) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": why })),
        )
            .into_response(),
        Ok(Err(relocate::MoveError::Unusable(why))) => AdminAnswer::Unusable(why).into_response(),
        Ok(Err(relocate::MoveError::Failed(why))) => AdminAnswer::Failed(why).into_response(),
        Err(why) => admin_panicked("moving a folder", &why),
    }
}

#[cfg(test)]
mod tests;
