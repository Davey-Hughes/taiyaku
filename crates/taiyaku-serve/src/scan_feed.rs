//! `GET /api/library/scans`, what every running job is doing, and
//! `POST /api/library/scans/{job}/cancel`, which stops one (spec
//! 2026-11-15-first-scan-progress-and-cancel §7, "The feed" and "Cancel
//! endpoint").
//!
//! A job is a scan, the fingerprint pass ([`crate::fingerprint_pass`]) or the
//! match pass ([`crate::match_pass`]), each pass a job of its own kind: a pass
//! waits on other jobs and holds no library, so it is in no answer's
//! `libraries`. An administrator's pages draw it from `jobs`, and only an
//! administrator is sent it or may cancel it.
//!
//! **The feed answers from memory alone.** Every page polls it while a job
//! runs, and each poll already pays the session guard's read and slide write
//! (O272), so the handler adds no store access of its own.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use axum::Json;
use axum::extract::{Extension, Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use taiyaku_library::progress::Step;

use crate::ServeState;
use crate::auth::Principal;
use crate::scan_jobs::{Job, JobEnded, JobKind, Snapshot};

/// `GET /api/library/scans`, for any signed-in account: `ended`, a marker
/// that moves each time a scan job or the match pass's derive ends, and each
/// library a running scan job holds with that job's step. An administrator in a browser
/// ([`crate::session::administers`]) is also sent every running job, the
/// passes among them, each with its kind; any other caller only the libraries
/// it is granted, with no count and no directory path, since a job's counts and
/// directories cover libraries it may not see. A pass holds no library, so it
/// is in no answer's libraries.
pub(crate) async fn feed(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
) -> Response {
    // Read before the jobs, and a job is marked ended before it is counted:
    // an answer whose marker counts a job's end never lists that job.
    let ended = state.jobs_ended.load(Ordering::SeqCst);
    let jobs: Vec<(Arc<Job>, Snapshot)> = running_jobs(&state)
        .into_iter()
        .map(|job| {
            let snapshot = job.snapshot();
            (job, snapshot)
        })
        .collect();
    // By job id ascending, so the first step kept is the lowest id's.
    let mut held: BTreeMap<i64, Step> = BTreeMap::new();
    for (job, snapshot) in &jobs {
        if job.kind != JobKind::Scan {
            continue;
        }
        for (library, _) in libraries_of(job) {
            held.entry(library).or_insert(snapshot.step);
        }
    }

    if crate::session::administers(&extensions) {
        return Json(serde_json::json!({
            "ended": ended,
            "libraries": held_json(&held),
            "jobs": jobs
                .iter()
                .map(|(job, snapshot)| job_json(job, *snapshot))
                .collect::<Vec<_>>(),
        }))
        .into_response();
    }
    let access = crate::auth::access_of(&extensions);
    held.retain(|library, _| access.grants(*library));
    Json(serde_json::json!({
        "ended": ended,
        "libraries": held_json(&held),
    }))
    .into_response()
}

/// `POST /api/library/scans/{job}/cancel`, an administrator's: asks running
/// job `job`, a scan or a pass, to stop at its next check, 202, however often
/// it is pressed; a job that has ended answers how it ended, 200, and is not
/// asked; a job that neither a slot nor a pass names is 404.
pub(crate) async fn cancel(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<u64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let Some(job) = job_named(&state, id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("there is no job {id}") })),
        )
            .into_response();
    };
    match job.ended() {
        // A job that ends after this read is asked anyway, and no check of
        // it is left to read the flag: it ended as it would have.
        None => {
            match job.kind {
                JobKind::Scan => job.request_cancel(),
                JobKind::Fingerprint => state.fingerprints.cancel(&job),
                JobKind::Match => state.matches.cancel(&job),
            }
            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({ "job": id, "cancelling": true })),
            )
                .into_response()
        }
        Some(JobEnded::Finished) => {
            Json(serde_json::json!({ "job": id, "ended": "finished" })).into_response()
        }
        Some(JobEnded::Cancelled(step)) => Json(serde_json::json!({
            "job": id,
            "ended": "cancelled",
            "step": step.as_str(),
        }))
        .into_response(),
    }
}

/// Each job a slot of `state`'s registry holds that has not ended, once each,
/// and each pass's while it has not ended, by id ascending.
///
/// **Not ended, rather than a slot not done**: a job's slots are all done
/// before it is marked ended, so a page that sees a job leave the feed reads
/// each directory's report. A pass keeps its last job once it ends, which a
/// page would otherwise show running, with its Cancel, for good.
fn running_jobs(state: &ServeState) -> Vec<Arc<Job>> {
    let mut jobs: BTreeMap<u64, Arc<Job>> = BTreeMap::new();
    {
        let scans = state.scans.lock().unwrap_or_else(PoisonError::into_inner);
        for progress in scans.values() {
            if progress.job.ended().is_none() {
                jobs.entry(progress.job.id)
                    .or_insert_with(|| Arc::clone(&progress.job));
            }
        }
    }
    for pass in [state.fingerprints.job(), state.matches.job()]
        .into_iter()
        .flatten()
    {
        if pass.ended().is_none() {
            jobs.insert(pass.id, pass);
        }
    }
    jobs.into_values().collect()
}

/// The job slot `id` names, or a pass's job `id` names, running or ended:
/// an ended pass's Cancel answers how it ended.
fn job_named(state: &ServeState, id: u64) -> Option<Arc<Job>> {
    {
        let scans = state.scans.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(progress) = scans.values().find(|progress| progress.job.id == id) {
            return Some(Arc::clone(&progress.job));
        }
    }
    [state.fingerprints.job(), state.matches.job()]
        .into_iter()
        .flatten()
        .find(|pass| pass.id == id)
}

fn libraries_of(job: &Job) -> Vec<(i64, String)> {
    job.libraries
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn held_json(held: &BTreeMap<i64, Step>) -> Vec<serde_json::Value> {
    held.iter()
        .map(|(library, step)| serde_json::json!({ "id": library, "step": step.as_str() }))
        .collect()
}

/// One running job, as an administrator is sent it, where `snapshot` is.
fn job_json(job: &Job, snapshot: Snapshot) -> serde_json::Value {
    // Waiting has no count yet, and Finishing reports a 0 that counts nothing.
    let counted = !matches!(snapshot.step, Step::Waiting | Step::Finishing);
    serde_json::json!({
        "id": job.id,
        "kind": job.kind.as_str(),
        "step": snapshot.step.as_str(),
        "done": snapshot.done.filter(|_| counted),
        "of": snapshot.of.filter(|_| counted),
        "dir": snapshot.dir.map(|(at, of)| serde_json::json!({ "at": at, "of": of })),
        "dirs": job
            .dirs
            .iter()
            .map(|(id, path)| serde_json::json!({ "id": id, "path": path.to_string_lossy() }))
            .collect::<Vec<_>>(),
        "libraries": libraries_of(job)
            .into_iter()
            .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
            .collect::<Vec<_>>(),
        "elapsed_secs": job.started.elapsed().as_secs_f64(),
        "cancelling": job.cancelling(),
    })
}

#[cfg(test)]
mod tests;
