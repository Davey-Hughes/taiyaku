//! The `/api/library/libraries*` handlers: Configuration → Libraries, §6 of
//! `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library that exists is addressed by its id**, in the path. A name is
//! read only to make a library or to rename one: a name migration 15's
//! backfill kept can carry spaces at its ends, and an id is never given to a
//! later library.
//!
//! **The folder rows inside a library's card are still `config_api`'s.** A
//! folder's settings, its own scan and its removal are routes under
//! `/api/library/roots/{id}`. This module makes libraries, adds folders to
//! them, removes them whole, and scans all of a library's folders at once.
//!
//! **A refusal these handlers make is `{"error": "<sentence>"}`**: 400, 404,
//! 409 and 500, including the ones axum would otherwise answer in plain text
//! (a body that is not JSON, an id that is not a number, a query it cannot
//! read). The body is read from bytes here, and the id from the path as text,
//! for that reason. A reader's 403 is each handler's own first statement, and
//! is plain text where a device credential's is JSON. A refusal made before a
//! handler runs is not in this shape: an over-size body's 413, a path that is
//! not UTF-8, and a method a route does not mount. The guard's 401 is
//! `{"error":"unauthorized"}`.
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
use taiyaku_library::{derive, libraries, scan};

use crate::config_api::{
    added_json, derive_report_json, finished_scan_json, panic_message, removal_json, root_json,
    roots_with_last_scan,
};
use crate::{ServeState, auth::Principal};

/// Why one of these routes did not answer with what was asked for.
#[derive(Debug)]
enum LibraryAnswer {
    /// `id` names no library: 404.
    Missing(i64),
    /// The caller's own mistake: 400, in the store's sentence where the store
    /// found it.
    Unusable(String),
    /// Well-formed, names a real library, and still not allowed: 409, in the
    /// words of [`scan::library_removal_refusal`] for a removal, and of
    /// [`reserve_folders`] for a scan.
    Refused(String),
    /// A store failure: 500.
    Failed(String),
}

impl IntoResponse for LibraryAnswer {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Missing(id) => (StatusCode::NOT_FOUND, format!("there is no library {id}")),
            Self::Unusable(why) => (StatusCode::BAD_REQUEST, why),
            Self::Refused(why) => (StatusCode::CONFLICT, why),
            Self::Failed(why) => (StatusCode::INTERNAL_SERVER_ERROR, why),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// The one failure `spawn_blocking` itself can hand back, as JSON like every
/// other refusal here.
fn library_panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    panicked_answer(what, why).into_response()
}

fn panicked_answer(what: &str, why: &tokio::task::JoinError) -> LibraryAnswer {
    LibraryAnswer::Failed(format!("{what} panicked: {why}"))
}

fn added_error(error: scan::AddRootError) -> LibraryAnswer {
    match error {
        scan::AddRootError::Unusable(why) => LibraryAnswer::Unusable(why),
        scan::AddRootError::Failed(why) => LibraryAnswer::Failed(why),
    }
}

/// The request body as JSON, or the caller's mistake.
fn json_body(bytes: &Bytes) -> Result<serde_json::Value, LibraryAnswer> {
    serde_json::from_slice(bytes)
        .map_err(|e| LibraryAnswer::Unusable(format!("the request body must be JSON: {e}")))
}

/// A library id from the path, which is a whole number.
fn library_id(raw: &str) -> Result<i64, LibraryAnswer> {
    raw.parse().map_err(|_| {
        LibraryAnswer::Unusable(format!(
            "`{raw}` is not a library id; an id is a whole number"
        ))
    })
}

/// A library's `name`, or `missing` as the caller's mistake: each route says
/// what it was asking for.
fn name_from_json(body: &serde_json::Value, missing: &str) -> Result<String, LibraryAnswer> {
    body.get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| LibraryAnswer::Unusable(missing.to_string()))
}

/// The folders of library `id`, oldest first, or [`LibraryAnswer::Missing`]
/// where there is no such library.
fn folders_of(conn: &rusqlite::Connection, id: i64) -> Result<Vec<scan::Root>, LibraryAnswer> {
    if libraries::name_of(conn, id)
        .map_err(LibraryAnswer::Failed)?
        .is_none()
    {
        return Err(LibraryAnswer::Missing(id));
    }
    Ok(scan::roots(conn)
        .map_err(LibraryAnswer::Failed)?
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
        libraries_json(&conn).map_err(LibraryAnswer::Failed)
    })
    .await;

    match built {
        Ok(Ok(libraries)) => Json(serde_json::json!({ "libraries": libraries })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("listing libraries", &why),
    }
}

/// `name` and `folders`, as [`create_library`]'s body is read.
fn new_library_from_json(
    body: &serde_json::Value,
) -> Result<(String, Vec<PathBuf>), LibraryAnswer> {
    let name = name_from_json(body, "a library needs a `name`, a string")?;
    let listed = body
        .get("folders")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            LibraryAnswer::Unusable("a library needs `folders`, an array of paths".to_string())
        })?;
    let mut folders = Vec::with_capacity(listed.len());
    for entry in listed {
        folders.push(absolute_folder(entry.as_str().ok_or_else(|| {
            LibraryAnswer::Unusable("a library's `folders` must be an array of paths".to_string())
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
        let working = state.library.working().map_err(LibraryAnswer::Failed)?;
        let made = scan::create_library(&working, &name, &folders).map_err(added_error)?;
        let library = libraries_json(&working)
            .map_err(LibraryAnswer::Failed)?
            .into_iter()
            .find(|library| library["id"] == made.library_id)
            .ok_or_else(|| {
                LibraryAnswer::Failed(format!(
                    "library {} was made and is not listed",
                    made.library_id
                ))
            })?;
        Ok::<_, LibraryAnswer>(serde_json::json!({
            "library": library,
            "added": made.folders.iter().map(added_json).collect::<Vec<_>>(),
        }))
    })
    .await;

    match created {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("creating a library", &why),
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
            libraries::RenameError::Missing(id) => LibraryAnswer::Missing(id),
            libraries::RenameError::Unusable(why) => LibraryAnswer::Unusable(why),
            libraries::RenameError::Failed(why) => LibraryAnswer::Failed(why),
        })
    })
    .await;

    match renamed {
        Ok(Ok(name)) => Json(serde_json::json!({ "id": id, "name": name })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("renaming a library", &why),
    }
}

/// A folder's path as a request gives it, which must be absolute.
///
/// The store resolves a relative path against the server's working directory,
/// which is the command line's own word and not a request's, and answers an
/// empty one with an operating-system error that names nothing, so neither is
/// passed on.
fn absolute_folder(raw: &str) -> Result<PathBuf, LibraryAnswer> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(LibraryAnswer::Unusable(format!(
            "a folder's path must be absolute, and `{raw}` is not"
        )))
    }
}

/// `path`, as [`add_folder`]'s body is read.
fn folder_path_from_json(body: &serde_json::Value) -> Result<PathBuf, LibraryAnswer> {
    let raw = body
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            LibraryAnswer::Unusable("adding a folder needs a `path`, a string".to_string())
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
        let working = state.library.working().map_err(LibraryAnswer::Failed)?;
        folders_of(&working, id)?;
        scan::add_root_checked_in(&working, &path, scan::InLibrary::Id(id)).map_err(added_error)
    })
    .await;

    match added {
        Ok(Ok(added)) => Json(added_json(&added)).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("adding a folder", &why),
    }
}

/// The one query parameter [`delete_library`] reads.
///
/// `deny_unknown_fields`, for the reason `config_api::DeleteRootQuery`
/// carries: a misspelt `dry_run` must be refused, not dropped, on a route
/// whose other reading removes a library.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteLibraryQuery {
    #[serde(default)]
    dry_run: bool,
}

/// `DELETE /api/library/libraries/{id}`: remove a library, or with
/// `?dry_run=true` answer what that would cost. The body is
/// `config_api::delete_root`'s, so one dialog's types read both.
///
/// **The refusal is asked here, before either store call**, and answered 409
/// in its words. Both store calls refuse too, as an error with nothing to
/// tell it from a store fault.
pub(crate) async fn delete_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<String>,
    query: Result<Query<DeleteLibraryQuery>, QueryRejection>,
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
            return LibraryAnswer::Unusable(format!(
                "the query could not be read: {}",
                rejection.body_text()
            ))
            .into_response();
        }
    };

    let scans_state = Arc::clone(&state);
    let removed = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(LibraryAnswer::Failed)?;
        let folders: Vec<i64> = folders_of(&working, id)?
            .iter()
            .map(|root| root.id)
            .collect();
        if let Some(refusal) =
            scan::library_removal_refusal(&working, id).map_err(LibraryAnswer::Failed)?
        {
            return Err(LibraryAnswer::Refused(refusal));
        }
        let (removal, derived) = if dry_run {
            scan::library_removal_cost(&working, id)
        } else {
            scan::remove_library(&working, id)
        }
        .map_err(LibraryAnswer::Failed)?;
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
        Err(why) => library_panicked("removing a library", &why),
    }
}

/// What each folder's report says of a scan that found its library changed.
const CHANGED: &str = "the library's folders changed while the scan waited to start, so nothing \
                       was scanned";

/// Walk each of `roots`, the folders of library `id`, in turn and derive
/// once: one report per root, in order, each its own walk with the one derive.
///
/// Holds [`ServeState::library_job`] across the walks and the derive, as
/// `config_api::run_scan_inner` holds it across one of each.
///
/// **The folders are read again once the lock is held.** `roots` is what the
/// request found; a library removed or given other folders while the scan
/// waited is not the one that was asked for, and its folders' ids may belong
/// to others by now. Where the two readings differ in the ids or paths of the
/// folders, or in their number, nothing is walked. Where they agree the
/// second reading is what is walked, with the settings the folders hold now.
///
/// # Errors
///
/// Returns the sentence every folder's report then carries: [`CHANGED`] where
/// the library is gone or its folders differ, the store's own words where the
/// second reading failed. The first walk that fails ends the scan: no later
/// folder is walked and nothing is derived. Each folder already walked has its
/// `last_scan_status` corrected by [`scan::mark_derive_failed`], as a failed
/// derive's folders have: its walk wrote `ok`, and what it found was not
/// derived.
fn run_library_scan_inner(
    library: &taiyaku_library::Library,
    id: i64,
    roots: &[scan::Root],
    job: &std::sync::Mutex<()>,
) -> Result<Vec<serde_json::Value>, String> {
    let _job = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let working = library.working()?;
    let roots = match folders_of(&working, id) {
        Ok(now) => {
            let same = now.len() == roots.len()
                && now
                    .iter()
                    .zip(roots)
                    .all(|(now, asked)| now.id == asked.id && now.path == asked.path);
            if !same {
                return Err(CHANGED.to_string());
            }
            now
        }
        Err(LibraryAnswer::Missing(_)) => return Err(CHANGED.to_string()),
        Err(
            LibraryAnswer::Unusable(why) | LibraryAnswer::Refused(why) | LibraryAnswer::Failed(why),
        ) => return Err(why),
    };
    let mut walked = Vec::with_capacity(roots.len());
    for root in &roots {
        match scan::scan_root(&working, root) {
            Ok(report) => walked.push(report),
            Err(why) => {
                for earlier in &roots[..walked.len()] {
                    let _ = scan::mark_derive_failed(&working, earlier.id);
                }
                return Err(format!("scanning {}: {why}", root.path.display()));
            }
        }
    }
    let derived = match derive::derive(&working) {
        Ok(report) => report,
        Err(why) => {
            for root in &roots {
                let _ = scan::mark_derive_failed(&working, root.id);
            }
            return Err(format!(
                "the library's folders were scanned, but deriving its structure afterward \
                 failed: {why}"
            ));
        }
    };
    Ok(walked
        .iter()
        .map(|walk| finished_scan_json(walk, &derived))
        .collect())
}

/// The job [`start_library_scan`] hands to [`spawn_library_scan`] for
/// library `id`.
fn library_scan_job(
    id: i64,
) -> impl FnOnce(&ServeState, &[scan::Root]) -> Result<Vec<serde_json::Value>, String> + Send + 'static
{
    move |state, roots| run_library_scan_inner(&state.library, id, roots, &state.library_job)
}

/// What each of `count` folders' registry entry then holds: `outcome`'s
/// reports, or, where it failed or panicked, the same `{"error": …}` for every
/// folder, so none is left running for good. Its caller catches a panic for
/// the reason `config_api::run_scan` catches one: nothing awaits the job, so
/// nothing else would see it.
fn reports_of(
    count: usize,
    outcome: std::thread::Result<Result<Vec<serde_json::Value>, String>>,
) -> Vec<serde_json::Value> {
    let why = match outcome {
        Ok(Ok(reports)) => return reports,
        Ok(Err(why)) => why,
        Err(payload) => format!("the scan panicked: {}", panic_message(payload.as_ref())),
    };
    (0..count)
        .map(|_| serde_json::json!({ "error": why }))
        .collect()
}

/// Reserve the registry slot of each of `roots` for one scan that started at
/// `started`, or refuse naming a folder already being scanned.
///
/// The check and the reservations are one lock: a refusal reserves nothing,
/// and two requests racing for the same folders cannot both find them free.
fn reserve_folders(
    state: &ServeState,
    roots: &[scan::Root],
    started: Instant,
) -> Result<(), LibraryAnswer> {
    let mut scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(busy) = roots
        .iter()
        .find(|root| scans.get(&root.id).is_some_and(|progress| !progress.done))
    {
        return Err(LibraryAnswer::Refused(format!(
            "a scan of {} is already running",
            busy.path.display()
        )));
    }
    for root in roots {
        scans.insert(
            root.id,
            crate::ScanProgress {
                started,
                done: false,
                report: None,
            },
        );
    }
    Ok(())
}

/// Run `job` on the blocking pool, and when it ends, however it ends, mark
/// every one of `roots` done with its report, or with an error where the job
/// answered none for it.
///
/// A reservation is identified by its `started`: an entry that another scan
/// started, or that a removal cleared, is left as it is.
fn spawn_library_scan<F>(
    state: Arc<ServeState>,
    roots: Vec<scan::Root>,
    started: Instant,
    job: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnOnce(&ServeState, &[scan::Root]) -> Result<Vec<serde_json::Value>, String>
        + Send
        + 'static,
{
    tokio::task::spawn_blocking(move || {
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&state, &roots)));
        let mut reports = reports_of(roots.len(), outcome).into_iter();
        let mut scans = state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for root in &roots {
            let report = reports.next().unwrap_or_else(
                || serde_json::json!({ "error": "the scan ended without a report for this folder" }),
            );
            // Only this scan's own reservation, which its start instant
            // names, is replaced: a removal clears the entry, and a scan of
            // a folder that took the freed id holds an entry of its own.
            if let Some(entry) = scans.get_mut(&root.id)
                && entry.started == started
            {
                *entry = crate::ScanProgress {
                    started,
                    done: true,
                    report: Some(report),
                };
            }
        }
    })
}

/// The folders of library `id` for a scan route, or the answer to give.
async fn scan_folders(
    state: &Arc<ServeState>,
    id: i64,
    what: &'static str,
) -> Result<Vec<scan::Root>, LibraryAnswer> {
    let lookup_state = Arc::clone(state);
    let found = tokio::task::spawn_blocking(move || {
        let conn = lookup_state.library.conn();
        folders_of(&conn, id)
    })
    .await;
    match found {
        Ok(found) => found,
        Err(why) => Err(panicked_answer(what, &why)),
    }
}

/// `POST /api/library/libraries/{id}/scan`: walk each folder of the library
/// in the background, derive once, and answer at once.
///
/// **The scan holds the registry slot of each folder.** A scan already
/// running on one folder refuses the library's, naming the folder, with no
/// slot taken for the others. And while the library's runs,
/// `config_api::start_scan` refuses each folder by the check it always made.
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
        return LibraryAnswer::Unusable(format!(
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
    if let Err(refused) = reserve_folders(&state, &roots, started) {
        return refused.into_response();
    }
    spawn_library_scan(state, roots, started, library_scan_job(id));

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

#[cfg(test)]
mod tests;
