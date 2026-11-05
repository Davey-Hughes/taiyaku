//! The `/api/library/libraries*` handlers: Configuration → Libraries, §6 of
//! `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library that exists is addressed by its id**, in the path. A name is
//! read only to make a library or to rename one: a name of white space alone
//! keeps its ends, which migration 16 strips from every other, and an id is
//! never given to a later library.
//!
//! **The directory rows inside a library's card are `config_api`'s.** A
//! directory's settings, its own scan and its removal are routes under
//! `/api/library/roots/{id}`. This module makes libraries, adds directories to
//! them, removes them whole, scans every directory a library shows at once, and
//! moves a directory from one library to another (`/api/library/roots/{id}/move`).
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
//! 409, `{"error", "volumes"}`, whose `volumes` is always empty: see
//! `relocate::MoveError::Refused`.
//!
//! Every read or write of the library is made on the blocking pool, and a
//! handler that derives or probes takes
//! [`taiyaku_library::Library::working`], for the reasons `config_api`'s own
//! doc gives.

use std::collections::HashMap;
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

/// The directories filed under library `id`, oldest first, or
/// [`AdminAnswer::MissingLibrary`] where there is no such library.
///
/// **Its own directories, not every one it reaches**: what a removal of the
/// library removes, and whose scan slots it clears. A scan walks
/// [`walked_dirs_of`].
fn dirs_of(conn: &rusqlite::Connection, id: i64) -> Result<Vec<scan::Root>, AdminAnswer> {
    Ok(roots_if_held(conn, id)?
        .into_iter()
        .filter(|root| root.library_id == id)
        .collect())
}

/// The directories a scan of library `id` walks
/// ([`taiyaku_library::reach::walked_by`]): every one it reaches, by path, or
/// [`AdminAnswer::MissingLibrary`] where there is no such library.
fn walked_dirs_of(conn: &rusqlite::Connection, id: i64) -> Result<Vec<scan::Root>, AdminAnswer> {
    Ok(taiyaku_library::reach::walked_by(
        &roots_if_held(conn, id)?,
        id,
    ))
}

/// Every directory, or [`AdminAnswer::MissingLibrary`] where there is no
/// library `id`.
fn roots_if_held(conn: &rusqlite::Connection, id: i64) -> Result<Vec<scan::Root>, AdminAnswer> {
    if libraries::name_of(conn, id)
        .map_err(AdminAnswer::Failed)?
        .is_none()
    {
        return Err(AdminAnswer::MissingLibrary(id));
    }
    scan::roots(conn).map_err(AdminAnswer::Failed)
}

/// Every library as the screen draws it, by name: its id, name, works, the
/// EPUB and PDF entries its scans recorded, and each directory in
/// [`root_json`]'s shape, oldest first, with [`nesting_json`]'s two keys.
fn libraries_json(conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>, String> {
    let roots = roots_with_last_scan(conn)?;
    let every = libraries::list(conn)?;
    let names: HashMap<i64, &str> = every
        .iter()
        .map(|library| (library.id, library.name.as_str()))
        .collect();
    let plain: Vec<scan::Root> = roots.iter().map(|(root, _, _)| root.clone()).collect();
    let mut rendered = Vec::new();
    for listed in &every {
        let dirs: Vec<serde_json::Value> = roots
            .iter()
            .filter(|(root, _, _)| root.library_id == listed.id)
            .map(|(root, at, status)| {
                let mut dir = root_json(root, at.as_deref(), status.as_deref());
                let (nested, inside) = nesting_json(&plain, root, &names);
                dir["nested"] = nested;
                dir["inside"] = inside;
                dir
            })
            .collect();
        rendered.push(serde_json::json!({
            "id": listed.id,
            "name": listed.name,
            "works": listed.works,
            "books": libraries::books(conn, listed.id)?,
            "dirs": dirs,
        }));
    }
    Ok(rendered)
}

/// What `dir`'s page says of the directories around and inside it, by path,
/// outermost first, each with its library's name: `nested`, each directory
/// inside `dir` and whether `dir`'s library shows it (`shown`, with the `id`
/// the library's scan status names it by), and `inside`, each directory `dir`
/// sits in and whether that one's library shows `dir`'s books (`shows`).
/// Each says why one is not shown ([`kept_out_by`]).
///
/// **Both are reach**, asked of the outer directory, and worked out here: a
/// pattern matched in the browser is not the one the walk compiles (C4 §3.2).
fn nesting_json(
    roots: &[scan::Root],
    dir: &scan::Root,
    names: &HashMap<i64, &str>,
) -> (serde_json::Value, serde_json::Value) {
    let mut others: Vec<&scan::Root> = roots
        .iter()
        .filter(|other| other.path != dir.path)
        .collect();
    // By path, so a directory comes before the directories inside it.
    others.sort_by(|a, b| a.path.cmp(&b.path));
    let library = |root: &scan::Root| names.get(&root.library_id).copied();

    let reached = taiyaku_library::reach::reach(roots, &[dir.id]);
    let nested: Vec<serde_json::Value> = others
        .iter()
        // Component-wise: `/mx` is not inside `/m`.
        .filter(|inner| inner.path.starts_with(&dir.path))
        .map(|inner| {
            serde_json::json!({
                "id": inner.id,
                "path": inner.path.to_string_lossy(),
                "library": library(inner),
                "shown": reached.contains(&inner.id),
                "kept_out_by": kept_out_by(dir, inner),
            })
        })
        .collect();
    let inside: Vec<serde_json::Value> = others
        .iter()
        .filter(|outer| dir.path.starts_with(&outer.path))
        .map(|outer| {
            serde_json::json!({
                "path": outer.path.to_string_lossy(),
                "library": library(outer),
                "shows": taiyaku_library::reach::reach(roots, &[outer.id]).contains(&dir.id),
                "kept_out_by": kept_out_by(outer, dir),
            })
        })
        .collect();
    (nested.into(), inside.into())
}

/// Why `outer`'s library does not show `inner`, nested in it:
/// `"exclusion"`, one of `outer`'s patterns, or `"dot_name"`, a name on the
/// way that starts with `.`; `null` where it shows it.
fn kept_out_by(outer: &scan::Root, inner: &scan::Root) -> serde_json::Value {
    match taiyaku_library::reach::kept_out(outer, inner) {
        Some(scan::KeptOut::Exclusion) => "exclusion".into(),
        Some(scan::KeptOut::DotName) => "dot_name".into(),
        None => serde_json::Value::Null,
    }
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

/// `GET /api/library/split-editions`: every edition some library holds part
/// of and not all of, which an account granted that library alone sees
/// narrowed (spec §3.8), as [`libraries::split_editions`] lists them.
pub(crate) async fn list_split_editions(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        libraries::split_editions(&conn).map_err(AdminAnswer::Failed)
    })
    .await;

    match built {
        Ok(Ok(editions)) => Json(serde_json::json!({ "editions": editions })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("listing the split editions", &why),
    }
}

/// `name` and `dirs`, as [`create_library`]'s body is read.
fn new_library_from_json(body: &serde_json::Value) -> Result<(String, Vec<PathBuf>), AdminAnswer> {
    let name = name_from_json(body, "a library needs a `name`, a string")?;
    let listed = body
        .get("dirs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            AdminAnswer::Unusable("a library needs `dirs`, an array of paths".to_string())
        })?;
    let mut dirs = Vec::with_capacity(listed.len());
    for entry in listed {
        dirs.push(absolute_dir(entry.as_str().ok_or_else(|| {
            AdminAnswer::Unusable("a library's `dirs` must be an array of paths".to_string())
        })?)?);
    }
    Ok((name, dirs))
}

/// `POST /api/library/libraries`: a library and its directories, all of them or
/// nothing. Answers the library as the list would, and each directory's watch
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
    let (name, dirs) = match json_body(&body).and_then(|body| new_library_from_json(&body)) {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let created = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        let made = scan::create_library(&working, &name, &dirs).map_err(added_error)?;
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
            "added": made.dirs.iter().map(added_json).collect::<Vec<_>>(),
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

/// A directory's path as a request gives it, which must be absolute.
///
/// The store resolves a relative path against the server's working directory,
/// which is the command line's own word and not a request's, and answers an
/// empty one with an operating-system error that names nothing, so neither is
/// passed on.
fn absolute_dir(raw: &str) -> Result<PathBuf, AdminAnswer> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(AdminAnswer::Unusable(format!(
            "a directory's path must be absolute, and `{raw}` is not"
        )))
    }
}

/// `path`, as [`add_dir`]'s body is read.
fn dir_path_from_json(body: &serde_json::Value) -> Result<PathBuf, AdminAnswer> {
    let raw = body
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AdminAnswer::Unusable("adding a directory needs a `path`, a string".to_string())
        })?;
    absolute_dir(raw)
}

/// `POST /api/library/libraries/{id}/dirs`: add a directory to this library,
/// through [`scan::add_root_checked_in`]'s validations.
pub(crate) async fn add_dir(
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
        Ok((id, dir_path_from_json(&body)?))
    });
    let (id, path) = match read {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let added = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        dirs_of(&working, id)?;
        scan::add_root_checked_in(&working, &path, scan::InLibrary::Id(id)).map_err(added_error)
    })
    .await;

    match added {
        Ok(Ok(added)) => Json(added_json(&added)).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("adding a directory", &why),
    }
}

/// `DELETE /api/library/libraries/{id}`: remove a library, or with
/// `?dry_run=true` answer what that would cost. The body is
/// `config_api::delete_root`'s, so one dialog's types read both.
///
/// **A scan reserved on any of its directories, or on a directory around or
/// nested in one, refuses it** ([`scan::removal_touches`]), 409, for the dry run
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
        let dirs: Vec<i64> = dirs_of(&working, id)?.iter().map(|root| root.id).collect();
        let all = scan::roots(&working).map_err(AdminAnswer::Failed)?;
        if let Some(root) =
            crate::scan_jobs::a_scan_reserved_on(&state, &scan::removal_touches(&all, &dirs))
        {
            return Err(AdminAnswer::Refused(crate::scan_jobs::removal_waits_for(
                root,
                "the library",
            )));
        }
        let (removal, derived) = if dry_run {
            scan::library_removal_cost(&working, id)
        } else {
            scan::remove_library(&working, id)
        }
        .map_err(AdminAnswer::Failed)?;
        Ok((removal, derived, dirs))
    })
    .await;

    // A freed `root.id` is given to the next directory added, so a removal that
    // answers `Ok` clears each directory's slot in the scan registry, as
    // `config_api::delete_root` clears one. The slots are cleared when the
    // removal answers, not when it commits: a removal that commits and then
    // fails its reconcile derive answers 500 with the library gone and the
    // slots left.
    if !dry_run && let Ok(Ok((_, _, dirs))) = &removed {
        let mut scans = scans_state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for dir in dirs {
            scans.remove(dir);
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

/// The directories a scan of library `id` walks ([`walked_dirs_of`]), for a
/// scan route, or the answer to give.
async fn scan_dirs(
    state: &Arc<ServeState>,
    id: i64,
    what: &'static str,
) -> Result<Vec<scan::Root>, AdminAnswer> {
    let lookup_state = Arc::clone(state);
    let found = tokio::task::spawn_blocking(move || {
        let conn = lookup_state.library.conn();
        walked_dirs_of(&conn, id)
    })
    .await;
    match found {
        Ok(found) => found,
        Err(why) => Err(AdminAnswer::Failed(format!("{what} panicked: {why}"))),
    }
}

/// `POST /api/library/libraries/{id}/scan`: walk each directory the library
/// reaches ([`walked_dirs_of`]) in the background, derive once, answer at once.
///
/// **The scan holds the registry slot of each directory.** A scan already
/// running on one directory refuses the library's, naming the directory, with no
/// slot taken for the others. And while the library's runs,
/// `config_api::start_scan` refuses each directory as already being scanned.
/// A directory's slot is released when the whole scan ends, since the one derive
/// is part of each directory's report.
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
             whole of each directory"
        ))
        .into_response();
    }
    let roots = match scan_dirs(&state, id, "looking up a library to scan").await {
        Ok(roots) => roots,
        Err(answer) => return answer.into_response(),
    };

    let started = Instant::now();
    if let Err(refused) = crate::scan_jobs::reserve(&state, &roots, started, id) {
        return AdminAnswer::Refused(refused).into_response();
    }
    // The answer names them: a page's list can be older than a pattern edit or
    // a nested add, so a page seeds what is scanning from these, not the list.
    let root_ids: Vec<i64> = roots.iter().map(|root| root.id).collect();
    crate::scan_jobs::spawn(state, roots, started, crate::scan_jobs::Reread::Library(id));

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "library_id": id,
            "status_url": format!("/api/library/libraries/{id}/scan"),
            "root_ids": root_ids,
        })),
    )
        .into_response()
}

/// `GET /api/library/libraries/{id}/scan`: whether a scan of any directory a
/// scan of the library walks is running, and which; a directory's report is
/// read from its own status route. `held_removals` names each of the
/// library's own directories whose removal waits for a scan around or inside
/// it ([`scan::removal_touches`]), and `moves_held` says a move would wait for
/// the job lock ([`crate::LibraryJob::is_held`]), so a page holds what would be
/// refused. `scans` groups the running directories by the library that
/// reserved them (`null` where none did).
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
    let built = tokio::task::spawn_blocking(move || scan_status_json(&state, id)).await;
    match built {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(answer)) => answer.into_response(),
        Err(why) => AdminAnswer::Failed(format!("reading a library's scan status panicked: {why}"))
            .into_response(),
    }
}

/// How long a reserved scan has run, and the library whose request reserved
/// it ([`crate::ScanProgress::library`]).
type Reserved = (std::time::Duration, Option<i64>);

/// [`library_scan_status`]'s body for library `id`.
///
/// The shared connection is never held while the registry is locked.
fn scan_status_json(state: &ServeState, id: i64) -> Result<serde_json::Value, AdminAnswer> {
    let all = roots_if_held(&state.library.conn(), id)?;
    let roots = taiyaku_library::reach::walked_by(&all, id);
    let held_removals: Vec<i64> = all
        .iter()
        .filter(|dir| dir.library_id == id)
        .filter(|dir| {
            // Its own scan is its own status's to say.
            let around_or_inside: Vec<scan::Root> = scan::removal_touches(&all, &[dir.id])
                .into_iter()
                .filter(|root| root.id != dir.id)
                .collect();
            crate::scan_jobs::a_scan_reserved_on(state, &around_or_inside).is_some()
        })
        .map(|dir| dir.id)
        .collect();

    // Each walked directory with, while its scan runs, how long it has run
    // and the library whose request reserved it.
    let dirs: Vec<(i64, Option<Reserved>)> = {
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
                    .map(|progress| (progress.started.elapsed(), progress.library));
                (root.id, running)
            })
            .collect()
    };
    let elapsed = dirs
        .iter()
        .filter_map(|(_, running)| running.map(|(elapsed, _)| elapsed))
        .max();
    let mut scans: std::collections::BTreeMap<Option<i64>, Vec<i64>> =
        std::collections::BTreeMap::new();
    for (root_id, running) in &dirs {
        if let Some((_, by)) = running {
            scans.entry(*by).or_default().push(*root_id);
        }
    }
    // Only the libraries that asked for a running scan are named, usually one.
    let mut names: HashMap<i64, String> = HashMap::new();
    let conn = state.library.conn();
    for by in scans.keys().flatten() {
        if let Some(name) = libraries::name_of(&conn, *by).map_err(AdminAnswer::Failed)? {
            names.insert(*by, name);
        }
    }

    Ok(serde_json::json!({
        "library_id": id,
        "running": elapsed.is_some(),
        "elapsed_secs": elapsed.map(|e| e.as_secs_f64()),
        "dirs": dirs
            .iter()
            .map(|(root_id, running)| serde_json::json!({
                "root_id": root_id,
                "running": running.is_some(),
            }))
            .collect::<Vec<_>>(),
        "held_removals": held_removals,
        "moves_held": state.library_job.is_held(),
        "scans": scans
            .iter()
            .map(|(by, root_ids)| serde_json::json!({
                "library": by.and_then(|by| names.get(&by)),
                "root_ids": root_ids,
            }))
            .collect::<Vec<_>>(),
    }))
}

/// Where a move's body says the directory goes.
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
        gains_sight,
        loses_sight,
        library_removed,
        grants_removed,
        left_with_no_library,
    } = moved;
    serde_json::json!({
        "library_id": library_id,
        "library": library,
        "gains_sight": gains_sight,
        "loses_sight": loses_sight,
        "library_removed": library_removed,
        "grants_removed": grants_removed,
        "left_with_no_library": left_with_no_library,
    })
}

/// `POST /api/library/roots/{id}/move`: move a directory to another library or
/// to a new one, or with `?dry_run=true` answer what that would do. `body` is
/// `{"library": id}` or `{"new_library": name}`; [`relocate`] makes the move.
///
/// **A scan reserved on any directory of either library refuses it**, the directory's
/// own library and the target, as [`delete_library`] refuses a removal: a scan
/// that is queued holds no job lock, so the lock alone would not see it.
///
/// **Then it asks for [`ServeState::library_job`] and does not wait for it.**
/// A scan and a rule save each derive, reading which library each directory
/// is in, and a move would change that under them. So it answers 409 at once
/// where one holds the lock, as `config_api::save_rules` does, for the
/// rehearsal as for the move.
///
/// **A refusal is a 409 with its sentence**, and `volumes` empty: the two
/// checks above are the only ones that answer 409, and nothing a move would
/// do to a work or a reader's place refuses it. The job lock is the store's,
/// so a scan of any library, a rule save, a stamp or another move holds it.
pub(crate) async fn move_dir(
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
                "`{id}` is not a directory id; an id is a whole number"
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
        let Some(dir) = roots.iter().find(|root| root.id == id) else {
            return Err(relocate::MoveError::Missing(format!(
                "there is no directory {id}"
            )));
        };
        let held_up = roots.iter().filter(|root| {
            root.library_id == dir.library_id
                || matches!(to, MoveTo::Library(library) if root.library_id == library)
        });
        let held_up: Vec<scan::Root> = held_up.cloned().collect();
        if let Some(root) = crate::scan_jobs::a_scan_reserved_on(&state, &held_up) {
            return Err(relocate::MoveError::Refused {
                why: crate::scan_jobs::waits_for(root, "move the directory"),
                volumes: Vec::new(),
            });
        }
        let _job = match state.library_job.try_lock() {
            Ok(held) => held,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(relocate::MoveError::Refused {
                    why: "a scan or a rule save is running; move the directory when it has \
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
        Ok(Ok(moved)) => Json(serde_json::json!({
            "moved": moved_json(&moved),
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
        Err(why) => admin_panicked("moving a directory", &why),
    }
}

#[cfg(test)]
mod tests;
