//! The `/api/library/libraries*` handlers: Configuration → Libraries, §6 of
//! `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library that exists is addressed by its id**, in the path. A name is
//! read only to make a library or to rename one: a name of white space alone
//! keeps its ends, which migration 16 strips from every other, and an id is
//! never given to a later library.
//!
//! **The directory rows inside a library's card are `config_api`'s.** A
//! directory's settings, its own scan and its removal from every library are
//! routes under `/api/library/roots/{id}`. This module makes libraries, adds
//! directories to them, takes one out of one library
//! (`/api/library/libraries/{id}/dirs/{dir}`), removes them whole, scans every
//! directory a library shows at once, and moves a directory from one library to
//! another (`/api/library/roots/{id}/move`).
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

use std::collections::{HashMap, HashSet};
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
    root_json, roots_with_last_scan, scan_interval_from_json,
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
        .filter(|root| root.libraries.contains(&id))
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
/// [`root_json`]'s shape, oldest first, with `libraries`, every library holding
/// it as `{id, name}`, ascending by id, this one too, and [`nesting_json`]'s
/// two keys. A directory among `waiting` reads `waiting`
/// ([`roots_with_last_scan`]).
fn libraries_json(
    conn: &rusqlite::Connection,
    waiting: &HashSet<i64>,
) -> Result<Vec<serde_json::Value>, String> {
    // One snapshot for the links and the names: a library removed between
    // the two reads would leave a directory linked to a library with no name.
    let _snapshot = taiyaku_library::one_snapshot(conn)
        .map_err(|e| format!("starting a read of the libraries: {e}"))?;
    let roots = roots_with_last_scan(conn, waiting)?;
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
            .filter(|(root, _, _)| root.libraries.contains(&listed.id))
            .map(|(root, at, status)| {
                let mut dir = root_json(root, at.as_deref(), status.as_deref());
                dir["libraries"] = libraries_holding(root, &names)?
                    .into_iter()
                    .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
                    .collect();
                let (nested, inside) = nesting_json(&plain, root, &names)?;
                dir["nested"] = nested;
                dir["inside"] = inside;
                Ok(dir)
            })
            .collect::<Result<_, String>>()?;
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
/// outermost first, each with every library holding it, by name ascending by
/// id (`libraries`): `nested`, each one inside `dir` and whether `dir`'s
/// library shows it (`shown`, and the `id` its scan status names it by), and
/// `inside`, each one `dir` sits in and whether its library shows `dir`'s
/// books (`shows`). Each says why one is not shown ([`kept_out_by`]).
///
/// **Both are reach**, asked of the outer directory, and worked out here: a
/// pattern matched in the browser is not the one the walk compiles (C4 §3.2).
fn nesting_json(
    roots: &[scan::Root],
    dir: &scan::Root,
    names: &HashMap<i64, &str>,
) -> Result<(serde_json::Value, serde_json::Value), String> {
    let mut others: Vec<&scan::Root> = roots
        .iter()
        .filter(|other| other.path != dir.path)
        .collect();
    // By path, so a directory comes before the directories inside it.
    others.sort_by(|a, b| a.path.cmp(&b.path));
    let libraries = |root: &scan::Root| -> Result<Vec<&str>, String> {
        Ok(libraries_holding(root, names)?
            .into_iter()
            .map(|(_, name)| name)
            .collect())
    };

    let reached = taiyaku_library::reach::reach(roots, &[dir.id]);
    let nested: Vec<serde_json::Value> = others
        .iter()
        // Component-wise: `/mx` is not inside `/m`.
        .filter(|inner| inner.path.starts_with(&dir.path))
        .map(|inner| {
            Ok(serde_json::json!({
                "id": inner.id,
                "path": inner.path.to_string_lossy(),
                "libraries": libraries(inner)?,
                "shown": reached.contains(&inner.id),
                "kept_out_by": kept_out_by(dir, inner),
            }))
        })
        .collect::<Result<_, String>>()?;
    let inside: Vec<serde_json::Value> = others
        .iter()
        .filter(|outer| dir.path.starts_with(&outer.path))
        .map(|outer| {
            Ok(serde_json::json!({
                "path": outer.path.to_string_lossy(),
                "libraries": libraries(outer)?,
                "shows": taiyaku_library::reach::reach(roots, &[outer.id]).contains(&dir.id),
                "kept_out_by": kept_out_by(outer, dir),
            }))
        })
        .collect::<Result<_, String>>()?;
    Ok((nested.into(), inside.into()))
}

/// Every library holding `root`, ascending by id, with its name.
///
/// # Errors
///
/// A directory in no library, or a link to a library `names` does not hold,
/// which no writer leaves: drawn, it would be a directory no library holds.
fn libraries_holding<'a>(
    root: &scan::Root,
    names: &HashMap<i64, &'a str>,
) -> Result<Vec<(i64, &'a str)>, String> {
    if root.libraries.is_empty() {
        return Err(format!(
            "directory {} is in no library",
            root.path.display()
        ));
    }
    root.libraries
        .iter()
        .map(|&id| {
            names
                .get(&id)
                .map(|&name| (id, name))
                .ok_or_else(|| format!("there is no library {id}"))
        })
        .collect()
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
        let waiting = crate::scan_jobs::waiting_for_a_lock(&state);
        let conn = state.library.conn();
        libraries_json(&conn, &waiting).map_err(AdminAnswer::Failed)
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

/// The schedule an add asks for, `scan_interval_secs` read and refused as
/// the `PATCH` reads and refuses it. Absent and null both ask for Automatic,
/// as an add has no stored value to leave alone (spec §3 decision 2).
fn asked_interval(body: &serde_json::Value) -> Result<Option<i64>, AdminAnswer> {
    scan_interval_from_json(body)
        .map(Option::flatten)
        .map_err(AdminAnswer::Unusable)
}

/// `name`, `dirs` and `scan_interval_secs`, as [`create_library`]'s body is
/// read.
fn new_library_from_json(
    body: &serde_json::Value,
) -> Result<(String, Vec<PathBuf>, Option<i64>), AdminAnswer> {
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
    Ok((name, dirs, asked_interval(body)?))
}

/// `POST /api/library/libraries`: a library and its directories, all of them or
/// nothing. Answers the library as the list would, and each directory's watch
/// probe, which runs on the working connection, not the shared one. The list
/// is read after the library commits, so a removal can land between: that
/// answers 409, as the store did what both requests asked and is not at fault
/// (O254).
pub(crate) async fn create_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    body: Bytes,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let (name, dirs, interval) =
        match json_body(&body).and_then(|body| new_library_from_json(&body)) {
            Ok(read) => read,
            Err(why) => return why.into_response(),
        };

    let created = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        let made = scan::create_library(&working, &name, &dirs, interval).map_err(added_error)?;
        let waiting = crate::scan_jobs::waiting_for_a_lock(&state);
        created_json(&working, &made, &waiting)
    })
    .await;

    match created {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("creating a library", &why),
    }
}

/// [`create_library`]'s answer for the library `made`: its entry as the list
/// draws it, `waiting` as [`libraries_json`] takes it, and each directory's
/// add.
fn created_json(
    working: &rusqlite::Connection,
    made: &scan::Created,
    waiting: &HashSet<i64>,
) -> Result<serde_json::Value, AdminAnswer> {
    let library = libraries_json(working, waiting)
        .map_err(AdminAnswer::Failed)?
        .into_iter()
        .find(|library| library["id"] == made.library_id)
        .ok_or_else(|| {
            AdminAnswer::Refused(format!(
                "another request removed {} before it could be listed",
                made.name
            ))
        })?;
    let added = made.dirs.iter().map(added_json).collect::<Vec<_>>();
    Ok(serde_json::json!({
        "library": library,
        "added": added,
    }))
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

/// `path` and `scan_interval_secs`, as [`add_dir`]'s body is read.
fn new_dir_from_json(body: &serde_json::Value) -> Result<(PathBuf, Option<i64>), AdminAnswer> {
    let raw = body
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AdminAnswer::Unusable("adding a directory needs a `path`, a string".to_string())
        })?;
    Ok((absolute_dir(raw)?, asked_interval(body)?))
}

/// `POST /api/library/libraries/{id}/dirs`: add a directory to this library,
/// through [`scan::add_root_checked_with`]'s validations.
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
        Ok((id, new_dir_from_json(&body)?))
    });
    let (id, (path, interval)) = match read {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let added = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        dirs_of(&working, id)?;
        let added = scan::add_root_checked_with(&working, &path, scan::InLibrary::Id(id), interval)
            .map_err(added_error)?;
        Ok::<_, AdminAnswer>(added_json(&added))
    })
    .await;

    match added {
        Ok(Ok(added)) => Json(added).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("adding a directory", &why),
    }
}

/// `DELETE /api/library/libraries/{id}`: remove a library, or with
/// `?dry_run=true` answer what that would cost, in `config_api::delete_root`'s
/// body, so one dialog's types read both. A directory another library holds
/// too is unlinked and named under `kept` ([`scan::remove_library`]).
///
/// **A scan reserved on any of its directories, a shared one included (O196),
/// or on a directory around or nested in one, refuses it**
/// ([`scan::removal_touches`]), 409, for the dry run as for the removal. A
/// removal that leaves an account with no library goes ahead and names them.
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
        // Held through the removal's derive and the derive after its commit
        // (O51).
        let Some(lease) = state
            .library
            .derive_lease(taiyaku_library::lease::Wait::No)
            .map_err(AdminAnswer::Failed)?
        else {
            return Err(AdminAnswer::Refused(crate::library_job::waits_for_the_job(
                "remove the library",
            )));
        };
        lease.holding(|| {
            let (removal, derived) = if dry_run {
                scan::library_removal_cost(&working, id)
            } else {
                scan::remove_library(&working, id)
            }
            .map_err(AdminAnswer::Failed)?;
            Ok((removal, derived))
        })
    })
    .await;

    // A freed `root.id` is given to the next directory added, so a removal that
    // answers `Ok` clears the slot in the scan registry of each directory it
    // removed, as the store read them inside the removal: a directory unlinked
    // or moved out of this library meanwhile is not among them. A directory it
    // only unlinked keeps its id and its slot, where another library's scan
    // report may be. A removal that commits and then fails its reconcile
    // derive answers 500 with the library gone and the slots left.
    if !dry_run && let Ok(Ok((removal, _))) = &removed {
        let mut scans = scans_state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for dir in &removal.removed {
            scans.remove(dir);
        }
    }

    match removed {
        Ok(Ok((removal, derived))) => Json(serde_json::json!({
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
    if let Err(refused) = crate::scan_jobs::reserve(&state, &roots, started, Some(id)) {
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
/// it ([`scan::removal_touches`]); `moves_held` says a move would wait for the
/// job lock, and `derive_running` that a derive holds the derive lease (O51),
/// so a page holds what would be refused. `scans` groups the running directories
/// by the library that reserved them while it reaches them, by id and name; else `null`.
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

/// `GET /api/library/derive`: whether a derive holds the derive lease
/// (`derive_running`), in this process or another, and whether the job lock
/// is held (`job_held`), so a page that reads no library's scan status holds
/// what the lease or the lock would refuse (O51).
pub(crate) async fn derive_status(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    // The probe opens the lock file, which is blocking I/O.
    let built = tokio::task::spawn_blocking(move || {
        serde_json::json!({
            "derive_running": state.library.derive_running(),
            "job_held": state.library_job.is_held(),
        })
    })
    .await;
    match built {
        Ok(body) => Json(body).into_response(),
        Err(why) => AdminAnswer::Failed(format!("reading whether a derive runs panicked: {why}"))
            .into_response(),
    }
}

/// How long a reserved scan has run, and the library whose request reserved
/// it ([`crate::ScanProgress::library`]), which the status names only while
/// that library still reaches the directory.
type Reserved = (std::time::Duration, Option<i64>);

/// [`library_scan_status`]'s body for library `id`.
///
/// The shared connection is never held while the registry is locked.
fn scan_status_json(state: &ServeState, id: i64) -> Result<serde_json::Value, AdminAnswer> {
    let all = roots_if_held(&state.library.conn(), id)?;
    let roots = taiyaku_library::reach::walked_by(&all, id);
    let held_removals: Vec<i64> = all
        .iter()
        .filter(|dir| dir.libraries.contains(&id))
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
    // A running directory is named by the library that reserved it only while
    // that library still reaches it (O254): a move, an unlink or a removal can
    // commit between a scan's check and its reservation, and the slot keeps
    // the library it was reserved under. Reach, not links: a library's scan
    // walks the directories nested in its own, and decision 20 names it there.
    let mut reached: HashMap<i64, Vec<i64>> = HashMap::new();
    let asked: Vec<(i64, Option<i64>)> = dirs
        .iter()
        .filter_map(|(root_id, running)| {
            let (_, by) = (*running)?;
            let by = by.filter(|by| {
                reached
                    .entry(*by)
                    .or_insert_with(|| taiyaku_library::reach::of_libraries(&all, &[*by]))
                    .contains(root_id)
            });
            Some((*root_id, by))
        })
        .collect();
    // Only the libraries that asked for a running scan are named, usually one.
    // The shared connection is let go before the lease's probe below, which
    // opens and locks a file every other request would wait behind.
    let mut names: HashMap<i64, String> = HashMap::new();
    {
        let conn = state.library.conn();
        let named: std::collections::BTreeSet<i64> =
            asked.iter().filter_map(|(_, by)| *by).collect();
        for by in named {
            if let Some(name) = libraries::name_of(&conn, by).map_err(AdminAnswer::Failed)? {
                names.insert(by, name);
            }
        }
    }
    // A library removed since `all` was read has no name. Its directories join
    // the one group no library is named by, so a page that compares ids never
    // meets an id without its name.
    let mut scans: std::collections::BTreeMap<Option<i64>, Vec<i64>> =
        std::collections::BTreeMap::new();
    for (root_id, by) in asked {
        scans
            .entry(by.filter(|by| names.contains_key(by)))
            .or_default()
            .push(root_id);
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
        "derive_running": state.library.derive_running(),
        "scans": scans
            .iter()
            .map(|(by, root_ids)| serde_json::json!({
                "library_id": by,
                "library": by.and_then(|by| names.get(&by)),
                "root_ids": root_ids,
            }))
            .collect::<Vec<_>>(),
    }))
}

/// The store's job lock, for a change to which libraries hold a directory, or
/// the sentence that refuses it, in `to_do`'s words ("move the directory").
///
/// **A scan reserved on a directory any of `libraries` holds refuses it
/// first**: a scan that is queued holds no job lock, so the lock alone would
/// not see it, and `scan_jobs::Reread::Library` would answer `CHANGED` for
/// its library. **Then it asks for the lock and does not wait for it**, as
/// `config_api::save_rules` does; [`crate::LibraryJob`] names what holds it.
fn relinking<'a>(
    state: &'a ServeState,
    roots: &[scan::Root],
    libraries: &[i64],
    to_do: &str,
) -> Result<crate::JobGuard<'a>, String> {
    let held_up: Vec<scan::Root> = roots
        .iter()
        .filter(|root| {
            root.libraries
                .iter()
                .any(|library| libraries.contains(library))
        })
        .cloned()
        .collect();
    if let Some(root) = crate::scan_jobs::a_scan_reserved_on(state, &held_up) {
        return Err(crate::scan_jobs::waits_for(root, to_do));
    }
    match state.library_job.try_lock() {
        Ok(held) => Ok(held),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => {
            Err(crate::library_job::waits_for_the_job(to_do))
        }
    }
}

/// [`scan::Unlinked`], over the wire, under [`moved_json`]'s names for the
/// same facts.
///
/// Destructured with no `..`, so that a field added to it is a compile error
/// here and not a line the dialog never draws.
fn unlinked_json(unlinked: &scan::Unlinked) -> serde_json::Value {
    let scan::Unlinked {
        loses_sight,
        library_removed,
        grants_removed,
        left_with_no_library,
    } = unlinked;
    serde_json::json!({
        "loses_sight": loses_sight,
        "library_removed": library_removed,
        "grants_removed": grants_removed,
        "left_with_no_library": left_with_no_library,
    })
}

/// `DELETE /api/library/libraries/{id}/dirs/{dir}`: Remove from this library,
/// which unlinks a directory another library keeps ([`scan::unlink`]), or
/// with `?dry_run=true` answers what that would do.
///
/// **It refuses as [`move_dir`] does**, the rehearsal too: a link that has gone,
/// 404, or a last link, 400, first ([`scan::check_unlink`]); then [`relinking`]'s
/// 409 for a scan reserved on a directory this library holds, or for the job
/// lock, so a page holds it exactly when it holds Move, and not for a scan of
/// a directory only another library holds.
pub(crate) async fn unlink_dir(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath((id, dir)): AxPath<(String, String)>,
    query: Result<Query<DryRunQuery>, QueryRejection>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let id = match library_id(&id) {
        Ok(id) => id,
        Err(why) => return why.into_response(),
    };
    let Ok(dir) = dir.parse::<i64>() else {
        return AdminAnswer::Unusable(format!(
            "`{dir}` is not a directory id; an id is a whole number"
        ))
        .into_response();
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

    let unlinked = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(AdminAnswer::Failed)?;
        let roots = roots_if_held(&working, id)?;
        if !roots.iter().any(|root| root.id == dir) {
            return Err(AdminAnswer::MissingDir(dir));
        }
        if let Err(refused) = scan::check_unlink(&working, dir, id) {
            return Ok(Err(refused));
        }
        let _job = relinking(
            &state,
            &roots,
            &[id],
            "remove the directory from this library",
        )
        .map_err(AdminAnswer::Refused)?;
        Ok(scan::unlink(&working, dir, id, dry_run))
    })
    .await;

    match unlinked {
        Ok(Ok(Ok(unlinked))) => Json(serde_json::json!({
            "unlinked": unlinked_json(&unlinked),
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Ok(Err(scan::UnlinkError::Missing(why)))) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": why })),
        )
            .into_response(),
        Ok(Ok(Err(scan::UnlinkError::Unusable(why)))) => AdminAnswer::Unusable(why).into_response(),
        Ok(Ok(Err(scan::UnlinkError::Failed(why)))) => AdminAnswer::Failed(why).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => admin_panicked("removing a directory from a library", &why),
    }
}

/// Where a move's body says the directory goes.
enum MoveTo {
    Library(i64),
    New(String),
}

/// `from`, the id of the library the directory leaves.
fn move_source_from_json(body: &serde_json::Value) -> Result<i64, AdminAnswer> {
    body.get("from")
        .ok_or_else(|| {
            AdminAnswer::Unusable(
                "a move names `from`, the id of the library the directory leaves".to_string(),
            )
        })?
        .as_i64()
        .ok_or_else(|| {
            AdminAnswer::Unusable("`from` must be a library id, a whole number".to_string())
        })
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

/// `POST /api/library/roots/{id}/move`: move a directory out of library `from`
/// into another or a new one, or with `?dry_run=true` answer what that would
/// do. `body` is `{"from": id, "library": id}` or `{"from": id, "new_library":
/// name}`; [`relocate`] makes the move. `from` has no default: one would let
/// a page older than a link move the directory out of the wrong library, and
/// a `from` such a page sends is refused first, before any wait.
///
/// **Then [`relinking`] refuses it, over `from` and the target**: 409 while a
/// scan is reserved on a directory either holds, or while the job lock is held.
///
/// **The lock keeps a scan's directories the ones it walks.** A scan holding
/// it reads again which directories its library shows and walks those
/// (`scan_jobs::Reread::Library`), and a move changes that set for `from` and
/// the target: under the lock it lands before that reading, which answers
/// `CHANGED`, or after the scan. Two moves run one after the other, so the
/// second is refused with a 409 rather than failing on the store. A 409's
/// `volumes` is empty, for the clients that read it.
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
    let (from, to) = match json_body(&body)
        .and_then(|body| Ok((move_source_from_json(&body)?, move_target_from_json(&body)?)))
    {
        Ok(read) => read,
        Err(why) => return why.into_response(),
    };

    let moved = tokio::task::spawn_blocking(move || {
        let working = state
            .library
            .working()
            .map_err(relocate::MoveError::Failed)?;
        let roots = scan::roots(&working).map_err(relocate::MoveError::Failed)?;
        relocate::check_source(&working, id, from)?;
        // The one source: the scans asked about are those of the library the
        // move takes the directory out of, and of the one it joins.
        let mut libraries = vec![from];
        if let MoveTo::Library(library) = to {
            libraries.push(library);
        }
        let _job = relinking(&state, &roots, &libraries, "move the directory").map_err(|why| {
            relocate::MoveError::Refused {
                why,
                volumes: Vec::new(),
            }
        })?;
        let target = match &to {
            MoveTo::Library(library) => relocate::Target::Library(*library),
            MoveTo::New(name) => relocate::Target::New(name),
        };
        if dry_run {
            relocate::move_cost(&working, id, from, target)
        } else {
            relocate::move_root(&working, id, from, target)
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
