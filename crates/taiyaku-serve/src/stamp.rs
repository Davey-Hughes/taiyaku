//! Spec C3 §6's two routes: what a stamp would rewrite, and doing it.
//!
//! **Its own module rather than `metadata.rs`'s.** C3 §6's argument is that
//! this is not a save path — *"putting it on the same Save button would make
//! the irreversible thing a side effect of the reversible one"* — and sharing
//! a module with the four `PATCH` routes is that same mistake one layer down.
//!
//! **The capability is read before the slug is resolved**, per the build
//! design's §2. Resolving the slug first is what would leak: an account
//! without `can_stamp` would then be answered 404 for a work it cannot see
//! and 403 for one it can, which is an existence oracle handed to exactly the
//! account §1.1 exists to make possible — the admin role with the capability
//! withheld. The 403 here is a fact about the account and is identical for
//! every slug.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path as AxPath, State},
    response::{IntoResponse, Response},
};

use crate::{ServeState, auth::Principal, metadata::NoAnswer};

/// One archive on the wire, in both directions.
///
/// The `POST` sends back what the `GET` returned, and
/// [`taiyaku_library::stamp::apply`] compares the two field for field — so
/// this type is the plan's identity rather than a summary of it.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct WireArchive {
    member_id: i64,
    edition_key: String,
    path: String,
    bytes: u64,
}

impl From<&taiyaku_library::stamp::Planned> for WireArchive {
    fn from(planned: &taiyaku_library::stamp::Planned) -> Self {
        Self {
            member_id: planned.member_id,
            edition_key: planned.edition_key.clone(),
            path: planned.path.to_string_lossy().into_owned(),
            bytes: planned.bytes,
        }
    }
}

impl From<&WireArchive> for taiyaku_library::stamp::Planned {
    fn from(wire: &WireArchive) -> Self {
        Self {
            member_id: wire.member_id,
            edition_key: wire.edition_key.clone(),
            path: std::path::PathBuf::from(&wire.path),
            bytes: wire.bytes,
        }
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct Confirmation {
    archives: Vec<WireArchive>,
}

/// The formats this work holds that a stamp will not write.
///
/// **Distinct from the plan simply being short.** "4 archives" reads as "all
/// of them" unless the screen can also say the rar was left alone, which is
/// what C3 §6's *absent, not disabled* asks for at the level of a whole work.
/// Sniffed rather than read from `member.format`, for the reason
/// [`taiyaku_library::stamp::plan`] gives: nothing writes that column (O69).
fn skipped_formats(paths: &[std::path::PathBuf]) -> Vec<String> {
    let mut formats: Vec<String> = paths
        .iter()
        .filter_map(|path| match taiyaku_read::sniff(path) {
            Ok(taiyaku_read::Format::Zip) => None,
            // A format taiyaku knows and will not write.
            Ok(format) => Some(format.token().unwrap_or("unknown").to_string()),
            // **Reported, not swallowed.** `stamp::plan` skips a member it
            // cannot open rather than failing the whole work, so this is the
            // only place left that can say the work has an archive nothing
            // looked at. Dropping the error here would make that silent,
            // which is the thing skipping it was allowed on condition of.
            Err(_) => Some("unreadable".to_string()),
        })
        .collect();
    formats.sort_unstable();
    formats.dedup();
    formats
}

/// `GET /api/library/works/{slug}/stamp-comicinfo` — what would change.
///
/// An empty list is a `200`, not a `404`: "nothing to do" is an answer, and
/// the screen renders it as an absent control rather than as an error.
pub(crate) async fn plan(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(slug): AxPath<String>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_stamp(&extensions) {
        return refusal;
    }
    answer(
        tokio::task::spawn_blocking(move || -> Result<serde_json::Value, NoAnswer> {
            // **`working()`, not `conn()`.** A plan opens every member of the
            // work to read its existing document, a seek apiece on the NAS this
            // runs on. Holding the shared `Mutex<Connection>` across that
            // stalls every other request, which is the reasoning `config_api`'s
            // own scan and save already act on.
            let conn = state.library.working().map_err(NoAnswer::Failed)?;
            let work_id = crate::metadata::work_by_slug(&conn, &principal.access, &slug)?;
            let planned =
                taiyaku_library::stamp::plan(&conn, work_id).map_err(NoAnswer::Refused)?;
            let paths =
                taiyaku_library::stamp::member_paths(&conn, work_id).map_err(NoAnswer::Failed)?;
            let mut skipped = skipped_formats(&paths);
            // Not a format, like `unreadable`: a member held under two
            // directories, which a stamp leaves alone so its copies stay one
            // member. See `stamp::plan`.
            if taiyaku_library::stamp::copied(&conn, work_id).map_err(NoAnswer::Failed)? {
                skipped.push("copied".to_string());
                skipped.sort_unstable();
            }
            Ok(serde_json::json!({
                "archives": planned.iter().map(WireArchive::from).collect::<Vec<_>>(),
                "total_bytes": planned.iter().map(|p| p.bytes).sum::<u64>(),
                "skipped_formats": skipped,
            }))
        })
        .await,
    )
}

/// `POST /api/library/works/{slug}/stamp-comicinfo` — do it.
///
/// The body carries the plan the screen was shown, and
/// [`taiyaku_library::stamp::apply`] refuses one that no longer matches. That
/// refusal arrives here as [`NoAnswer::Refused`] and so answers **409**, not
/// 400: the request was well formed and the world moved under it.
pub(crate) async fn perform(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(slug): AxPath<String>,
    Json(confirmation): Json<Confirmation>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_stamp(&extensions) {
        return refusal;
    }
    answer(
        tokio::task::spawn_blocking(move || -> Result<serde_json::Value, NoAnswer> {
            // **Tried, never waited for**, exactly as a rule save does it: a
            // stamp is the second-longest thing this server does to a library
            // after a scan, and a caller queued silently behind a scan cannot
            // tell "still rewriting" from "hung".
            // See `ServeState::library_job`.
            //
            // It matters more here than for a save. A scan re-derives while
            // this rewrites the very archives it is walking, and `derive` can
            // rekey a work under a stamp that is midway through moving its
            // members' digests.
            let _job = match state.library_job.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    return Err(NoAnswer::Refused(
                        "a scan or another library job is currently running against this \
                         library; wait for it to finish and stamp again"
                            .to_string(),
                    ));
                }
                Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            };
            // A second connection for the same reason the `GET` takes one, and
            // more so: this one copies and renames every planned archive.
            let conn = state.library.working().map_err(NoAnswer::Failed)?;
            let work_id = crate::metadata::work_by_slug(&conn, &principal.access, &slug)?;
            let expected: Vec<taiyaku_library::stamp::Planned> = confirmation
                .archives
                .iter()
                .map(taiyaku_library::stamp::Planned::from)
                .collect();
            let applied =
                taiyaku_library::stamp::apply(&conn, work_id, principal.user_id, &expected)
                    .map_err(NoAnswer::Refused)?;
            Ok(serde_json::json!({
                "rewritten": applied.members.len(),
                "content_key": applied.content_key,
                "document_moved": applied.document_moved,
                "sidecar_rewritten": applied.sidecar_rewritten,
            }))
        })
        .await,
    )
}

/// The `spawn_blocking` join, mapped the way `metadata.rs` maps its own.
fn answer(joined: Result<Result<serde_json::Value, NoAnswer>, tokio::task::JoinError>) -> Response {
    match joined {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no)) => no.into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("the stamp did not run: {e}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests;
