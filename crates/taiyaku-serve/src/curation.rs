//! The `/api/library/proposals/*` handlers (spec C3 §7), and content
//! matching's Split and Undo on `/api/library/matches/{id}/split` (spec
//! 2026-11-19-content-matching-volume-joins §5).
//!
//! Thin by design, like `library_api`: parse, call
//! [`taiyaku_library::proposals`], serialise. Every decision about what an
//! accept may write lives in that module, so there is one place the collateral
//! guard is applied and one place to read in order to know whether it is.
//!
//! **Not in `library_api`.** That module is C1's read layer over `browse` and
//! carries the root grant into every query; these routes write, are
//! admin-only, and answer about the library's *organisation* rather than about
//! what a reader may see.
//!
//! # Admin, and the status codes
//!
//! Every route here refuses a non-admin with **403**: the credential is valid
//! and the caller is who they say, which is exactly why a 401 would be the
//! wrong answer — the same reasoning `session.rs` records for
//! `refuse_a_device_credential`.
//!
//! A refused accept is **409**, not 400. The request is well-formed and the
//! rule is one the server itself previewed; what is wrong is the state of the
//! library it would be applied to, and the body names the series it would have
//! moved.
//!
//! A store that fails during any of the writes is **500**, as a store
//! failing anywhere else here is (O70). `accept_with`, `unaccept`, `dismiss`
//! and `reopen` answer `Result<Offer<_>, String>`: [`made`] turns the refusal
//! into [`NoAnswer::Refused`], and each handler turns the `Err` beside it into
//! [`NoAnswer::Failed`] — and nothing here reads the sentence to decide which.
//!
//! An `id` naming no proposal is **404**, on every route here that takes one
//! — [`NoAnswer::Missing`] is that answer, and [`present`] is what makes
//! answering it possible without reading the store's error text.
//!
//! A rule that cannot be *evaluated* is **400** instead, and it is decided
//! before the store is read. [`taiyaku_library::proposals::resolves`] returns
//! `Ok` for every rule it can evaluate, refusal and all — the refusals are
//! fields of its answer — but its `Err` covers a rule it could not compile
//! *and* a read that failed underneath it, in one `String`. Mapping that
//! `Err` to a 400 would tell a client to fix its input while the database was
//! broken, on the route a rule-editing form calls at every keystroke.
//! [`taiyaku_library::proposals::evaluable`] asks the rule half of that
//! question on its own and reads nothing, so [`resolution`] asks it first and
//! the dry run's own `Err` is left meaning a failed read.
//!
//! # Lists, not sentences
//!
//! `Side::directories` and `FirstVolume::formats` cross the wire as arrays.
//! The store hands over data and the client decides how to render it; joining
//! them here would be a store-side sentence the client had to parse back
//! apart, which is what `proposals::Reason`'s own doc comment refuses to make
//! anyone do.
//!
//! # Split and Undo
//!
//! Each writes every pair of one Split and derives the whole store in one
//! transaction, so the answer is the library the next page load shows: a Split
//! that waited for the next scan would look like it did nothing. A derive
//! beside another would write works from a store the other is rewriting, so
//! each tries the job lock and the derive lease once and answers **409** while
//! either is held, as `config_api::save_rules` does. A pair, work or edition
//! the request names that the store does not hold that way is **404**.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use taiyaku_library::derive::{self, DeriveReport};
use taiyaku_library::proposals::{self, Filter, Offer, ProposedRule};
use taiyaku_library::{lease, matches};

use crate::{ServeState, auth::Principal};

/// The state the inbox counts its kind tabs within when the query names none.
///
/// `proposals::list` reads `None` as "every state"; `counts_by_kind` has no
/// such reading and takes one state, so the two need an answer here for a
/// request that supplies neither.
const DEFAULT_COUNTED_STATE: &str = "open";

/// Why a curation route could not answer.
///
/// **One type rather than a `Response` built where the failure happened**,
/// for the reason `routes::NoProgress` exists: every handler here does its
/// SQLite work inside `spawn_blocking`, which flattens a handler's failures
/// into one `Result` however different they were on the way in, and a
/// `Response` already built cannot be asked which of them it was.
#[derive(Debug)]
enum NoAnswer {
    /// **`id` names no proposal — 404, on every proposal route that takes
    /// one.**
    ///
    /// See [`present`] for how the condition is told apart from a read that
    /// failed, and `an_unknown_id_is_a_404_on_every_route_that_takes_one` for
    /// the enumeration.
    Missing(i64),
    /// A refusal about the state of the library — see this module's own doc
    /// comment for why that is a 409 — and nothing else: the sentence of an
    /// `Offer::Refused` from `accept_with`, `unaccept`, `dismiss` or `reopen`,
    /// handed to the client whole by [`made`].
    ///
    /// **A store fault never reaches it (O70).** Those functions return a
    /// refusal in an `Offer`, apart from the `Err` a failure comes in, so
    /// telling the two apart here takes no match on the store's own sentence.
    Refused(String),
    /// A store failure, which is a 500 rather than anything a caller can fix.
    ///
    /// The message is the store's own. `taiyaku_library::proposals` words
    /// every error with what it was doing — `listing proposals: …` — so a
    /// second phrase of this module's would say it twice.
    Failed(String),
    /// The caller's own text: a body this module could not read as a rule or
    /// a Split, or a rule the engine could not evaluate.
    ///
    /// **All are decided before any store call**, which is what makes that
    /// sentence true. [`taiyaku_library::proposals::resolves`] folds a rule it
    /// cannot evaluate together with a read that failed underneath it into one
    /// `Err(String)`, and mapping that `Err` here would answer 400 for a broken
    /// database — see this module's own doc comment. The rule half is asked by
    /// [`taiyaku_library::proposals::evaluable`], which takes no connection, so
    /// only it, [`rule_from_json`] and [`split_from_json`] reach this variant.
    Unusable(String),
    /// A pair, or the work and edition a Split names, that the store does not
    /// hold as the request names it — 404. The sentence says which.
    Absent(String),
    /// The job lock or the derive lease is held, so a Split or an Undo would
    /// derive beside another job — 409, in
    /// [`crate::library_job::waits_for_the_job`]'s words.
    Busy(String),
}

impl IntoResponse for NoAnswer {
    fn into_response(self) -> Response {
        match self {
            Self::Missing(id) => {
                (StatusCode::NOT_FOUND, format!("there is no proposal {id}")).into_response()
            }
            Self::Refused(why) => (StatusCode::CONFLICT, why).into_response(),
            Self::Failed(why) => (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
            Self::Unusable(why) => (StatusCode::BAD_REQUEST, why).into_response(),
            Self::Absent(why) => (StatusCode::NOT_FOUND, why).into_response(),
            Self::Busy(why) => (StatusCode::CONFLICT, why).into_response(),
        }
    }
}

/// The one failure `spawn_blocking` itself can hand back: the closure
/// panicked and nothing answered at all. A 500 naming what was running,
/// worded the way `routes.rs` words its own.
fn panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    NoAnswer::Failed(format!("{what} panicked: {why}")).into_response()
}

/// Refuse `id` with a 404 unless the library actually holds it.
///
/// **Not a match on the store's error text.**
/// [`taiyaku_library::proposals::lookup`] answers `Ok(None)` for a row that
/// is not there and `Err` only for a read that failed — the same split
/// `proposals::Offer` makes for `preview`, made here for a read.
///
/// One indexed read by primary key, on routes that are each about to do
/// considerably more than that. It replaces nothing: every store call below
/// still makes its own refusals about the row it reads itself, so this is a
/// status this module can answer rather than a guard the store leans on.
fn present(conn: &rusqlite::Connection, id: i64) -> Result<(), NoAnswer> {
    match proposals::lookup(conn, id) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(NoAnswer::Missing(id)),
        Err(why) => Err(NoAnswer::Failed(why)),
    }
}

/// A write's store `Err`, as the status it is owed: **404** when `id` names
/// no proposal by the time the store read it, **500** for anything else.
///
/// **[`present`] answers the 404 before a write starts, and this answers it
/// after (O72).** Another process can delete the row between the two, and the
/// store then reports the missing row as its `Err`, which would be a 500 for a
/// proposal that is simply not there. Only a row that is gone becomes a 404:
/// where the read fails as well, the write's own failure is what the caller
/// is told.
///
/// Used by the two writes whose store call reads the row after its
/// conditional `UPDATE`, `dismiss` and `reopen`. `accept` and `unaccept` have the same gap behind
/// `present` and do not use it yet.
fn failed_unless_gone(conn: &rusqlite::Connection, id: i64, why: String) -> NoAnswer {
    match proposals::lookup(conn, id) {
        Ok(None) => NoAnswer::Missing(id),
        Ok(Some(_)) | Err(_) => NoAnswer::Failed(why),
    }
}

/// What a write came to once its store call answered at all: what it made,
/// or its refusal as a 409 carrying the store's sentence whole.
///
/// **Handed the `Offer` alone, never the `Result` around it.** Each handler
/// maps that `Err` to [`NoAnswer::Failed`] before calling this, so a store
/// that failed cannot arrive here, and this is the one place a refusal
/// becomes a status.
fn made<T>(offer: Offer<T>) -> Result<T, NoAnswer> {
    match offer {
        Offer::Made(made) => Ok(made),
        Offer::Refused(why) => Err(NoAnswer::Refused(why)),
    }
}

/// One proposal, as the detail route echoes it back inside its evidence.
fn proposal_json(p: &proposals::Proposal) -> serde_json::Value {
    serde_json::json!({
        "id": p.id,
        "kind": p.kind,
        "subject_id": p.subject_id,
        "target_id": p.target_id,
        "score": p.score,
        "reason": p.reason,
        "state": p.state,
        "created_rule_id": p.created_rule_id,
    })
}

/// One proposal row, as the inbox *lists* it: [`proposal_json`] plus the two
/// series titles the collapsed line names.
///
/// **A superset of the detail route's shape, not a different one.** The
/// client's listing type extends its `Proposal`, so a row can be read by
/// anything that reads a proposal; only the inbox has the titles, because
/// only [`taiyaku_library::proposals::list`] joins for them. `target_title`
/// is `null` exactly where `target_id` is.
fn listed_proposal_json(p: &proposals::ListedProposal) -> serde_json::Value {
    let mut row = proposal_json(&p.proposal);
    let object = row
        .as_object_mut()
        .expect("proposal_json builds a JSON object");
    object.insert("subject_title".to_string(), p.subject_title.clone().into());
    object.insert("target_title".to_string(), p.target_title.clone().into());
    row
}

/// The first volume of one side of a proposal, as the evidence panel draws
/// it.
fn first_volume_json(v: &proposals::FirstVolume) -> serde_json::Value {
    serde_json::json!({
        "work_slug": v.work_slug,
        "edition_key": v.edition_key,
        "page_count": v.page_count,
        "formats": v.formats,
        "interior": v.interior,
    })
}

/// One end of a proposal.
fn side_json(s: &proposals::Side) -> serde_json::Value {
    serde_json::json!({
        "series_id": s.series_id,
        "title": s.title,
        "directories": s.directories,
        "volume_count": s.volume_count,
        "volumes": s.volumes,
        "root_name": s.root_name,
        "first_volume": s.first_volume.as_ref().map(first_volume_json),
    })
}

/// The rule an accept would write, on its way out.
///
/// The counterpart of [`rule_from_json`], and the two are a pair on purpose:
/// C3 §7 is *"the client shows the rule, the server writes what it was
/// shown"*, so this type has to round-trip rather than be re-derived at write
/// time.
fn rule_json(rule: &ProposedRule) -> serde_json::Value {
    serde_json::json!({
        "root_id": rule.root_id,
        "kind": rule.kind,
        "pattern": rule.pattern,
        "match_on": rule.match_on,
        "set_series": rule.set_series,
        "set_edition_key": rule.set_edition_key,
        "set_kind": rule.set_kind,
        "set_language": rule.set_language,
        "note": rule.note,
    })
}

/// The rule an accept was handed, on its way in.
///
/// **Every field is read explicitly and nothing is invented.** A `serde`
/// default for an absent `set_*` would let a client that dropped a field on
/// the floor silently write a rule with that column cleared — and the whole
/// contract here is that what is written is what was shown. An absent field
/// and an explicit `null` are the one case that *is* folded together: JSON has
/// no third thing for them to mean, and `ProposedRule` has no third state to
/// put them in.
fn rule_from_json(body: &serde_json::Value) -> Result<ProposedRule, String> {
    let text = |field: &str| -> Result<String, String> {
        body.get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("the rule's `{field}` must be a string"))
    };
    let optional_text = |field: &str| -> Result<Option<String>, String> {
        match body.get(field) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(format!("the rule's `{field}` must be a string or null")),
        }
    };
    let root_id = match body.get("root_id") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(
            value
                .as_i64()
                .ok_or_else(|| "the rule's `root_id` must be a number or null".to_string())?,
        ),
    };
    Ok(ProposedRule {
        root_id,
        kind: text("kind")?,
        pattern: text("pattern")?,
        match_on: text("match_on")?,
        set_series: optional_text("set_series")?,
        set_edition_key: optional_text("set_edition_key")?,
        set_kind: optional_text("set_kind")?,
        set_language: optional_text("set_language")?,
        note: text("note")?,
    })
}

/// The `?state=&kind=&min_score=` the inbox's tabs carry — spec C3 §7.
#[derive(serde::Deserialize)]
pub(crate) struct InboxQuery {
    state: Option<String>,
    kind: Option<String>,
    /// Absent is `0.0`, which is every proposal: the weak ones stay reachable
    /// at a lower threshold rather than being hidden, per
    /// [`taiyaku_library::proposals::open`]'s own argument.
    #[serde(default)]
    min_score: f64,
}

/// `GET /api/library/proposals` — the inbox, and the counts its tabs carry.
pub(crate) async fn list(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Query(query): Query<InboxQuery>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    // Two queries against `state.library`, whose `conn()` is a `MutexGuard` —
    // `routes.rs`'s reasoning about the async worker applies unchanged, and
    // `auth::guard` records the same one for the middleware every request
    // already passes through.
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let filter = Filter {
            state: query.state.clone(),
            kind: query.kind.clone(),
            min_score: query.min_score,
        };
        let listed = proposals::list(&conn, &filter).map_err(NoAnswer::Failed)?;
        // **Counted within the state being viewed and above the same
        // `min_score`, not within `open` at no threshold always.** The tabs
        // count kinds, and a person reading the dismissed tab is asking how
        // many dismissed proposals of each kind there are; a threshold that
        // filtered the list and not the counts would leave a tab with a count
        // over an empty list.
        //
        // `kind` is the one parameter deliberately not applied — a tab cannot
        // count itself out of existence, which is the ordinary faceted shape:
        // each facet counted with its own filter removed.
        let counted_state = query.state.as_deref().unwrap_or(DEFAULT_COUNTED_STATE);
        let counts = proposals::counts_by_kind(&conn, counted_state, query.min_score)
            .map_err(NoAnswer::Failed)?;
        Ok::<_, NoAnswer>(serde_json::json!({
            "proposals": listed.iter().map(listed_proposal_json).collect::<Vec<_>>(),
            "counts": counts,
        }))
    })
    .await;

    match built {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("listing proposals", &why),
    }
}

/// `GET /api/library/proposals/{id}` — the expanded row.
///
/// **An `id` naming no proposal is a 404 and every other failure is a 500.**
/// [`taiyaku_library::proposals::evidence`] answers both with one
/// `Err(String)`, so [`present`] gives the two questions separate answers. A
/// proposal that cannot be *previewed* is not a failure at all: it answers 200
/// with `rule: null`.
pub(crate) async fn detail(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    // A sweep of every `scan_entry` row in the library, the reads either side
    // of it, and a stat of each scanned member of both sides' first volume —
    // `list`'s reasoning about the async worker, on the heavier of the two
    // read routes.
    let built = tokio::task::spawn_blocking(move || {
        // **`working()`, not `conn()`.** The evidence stats the scanned
        // members of each side's first volume and sniffs one that moved since
        // its scan, a wait apiece on the NAS this runs on. Holding the shared
        // `Mutex<Connection>` across that stalls every other request, key
        // and session authentication among them: `stamp::plan`'s reasoning.
        let conn = state.library.working().map_err(NoAnswer::Failed)?;
        expanded(&conn, id)
    })
    .await;

    match built {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("reading a proposal", &why),
    }
}

/// [`detail`]'s body for proposal `id`, read on `conn`.
///
/// **One snapshot for the whole answer** ([`taiyaku_library::one_snapshot`]).
/// `conn` is the route's own, so no write is kept out while this reads,
/// whichever connection it takes: [`accept`] the shared one, [`split_match`]'s
/// derive a working one. One committing between two of its statements would
/// pair halves of two states: a proposal read as open beside the sweep after
/// its accept, which offers no rule, or a row [`present`] found and
/// [`proposals::evidence`] did not, a 500 for a 404.
fn expanded(conn: &rusqlite::Connection, id: i64) -> Result<serde_json::Value, NoAnswer> {
    // A read transaction, held across the evidence's waits on the share. It
    // keeps no writer out, but the WAL cannot be checkpointed past the
    // snapshot until the answer is built, so it grows by what commits
    // meanwhile.
    let _snapshot = taiyaku_library::one_snapshot(conn)
        .map_err(|e| NoAnswer::Failed(format!("starting a read of proposal {id}: {e}")))?;
    present(conn, id)?;
    let evidence = proposals::evidence(conn, id).map_err(NoAnswer::Failed)?;
    Ok(serde_json::json!({
        "proposal": proposal_json(&evidence.proposal),
        "subject": side_json(&evidence.subject),
        "target": evidence.target.as_ref().map(side_json),
        "reasons": evidence
            .reasons
            .iter()
            .map(|r| serde_json::json!({ "text": r.text, "sign": r.sign }))
            .collect::<Vec<_>>(),
        "rule": evidence.rule.as_ref().map(rule_json),
    }))
}

/// `POST /api/library/proposals/{id}/resolution` — what a candidate rule
/// would do, without doing it.
///
/// A `POST` because it carries a rule in its body and a `GET` cannot; it
/// writes nothing. The refusals an accept would make are *fields* of the
/// answer rather than a status, because the form exists to show them while
/// somebody edits the rule — a 409 here would leave the client with a status
/// to render and none of the four numbers beside it.
pub(crate) async fn resolution(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let rule = match rule_from_json(&body) {
        Ok(rule) => rule,
        Err(why) => return NoAnswer::Unusable(why).into_response(),
    };
    // **Asked here rather than read back out of the dry run's own error.**
    // `resolves` answers a rule it cannot evaluate and a read that failed
    // with one `Err(String)`; this asks the rule half without a connection,
    // so what is left below is the store's and gets a 500. See this module's
    // doc comment.
    if let Err(why) = proposals::evaluable(&rule) {
        return NoAnswer::Unusable(why).into_response();
    }

    // The route a rule-editing form calls on every keystroke, and each call
    // resolves every `scan_entry` row in the library through the whole rule
    // set — `list`'s reasoning about the async worker, on the route that does
    // that most often rather than the one that does most per call.
    let resolved = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        present(&conn, id)?;
        let resolution = proposals::resolves(&conn, id, &rule).map_err(NoAnswer::Failed)?;
        Ok::<_, NoAnswer>(serde_json::json!({
            "also_resolves": resolution.also_resolves,
            "collateral": resolution.collateral,
            "targets_reached": resolution.targets_reached,
            "entries_matched": resolution.entries_matched,
            "subject_reached": resolution.subject_reached,
            "works_moved": resolution.works_moved,
        }))
    })
    .await;

    match resolved {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("resolving a candidate rule", &why),
    }
}

/// `POST /api/library/proposals/{id}/accept` — write exactly the rule in the
/// body.
///
/// The rule is handed to [`taiyaku_library::proposals::accept_with`]
/// unexamined by this module: every refusal an accept makes is that
/// function's, so there is one place they are expressed and one place to read
/// in order to know what they are.
pub(crate) async fn accept(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let rule = match rule_from_json(&body) {
        Ok(rule) => rule,
        Err(why) => return NoAnswer::Unusable(why).into_response(),
    };

    // `list`'s reasoning about the async worker: a sweep of what the rule
    // would decide, then a transaction that writes it.
    let written = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        present(&conn, id)?;
        let rule_id = made(proposals::accept_with(&conn, id, &rule).map_err(NoAnswer::Failed)?)?;
        Ok::<_, NoAnswer>(serde_json::json!({ "rule_id": rule_id }))
    })
    .await;

    match written {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("accepting a proposal", &why),
    }
}

/// `DELETE /api/library/proposals/{id}/accept` — undo it.
///
/// 204: the rule is gone and the proposals it answered are open again, and
/// there is nothing left to say about a question that is back where it
/// started.
pub(crate) async fn unaccept(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    // `list`'s reasoning about the async worker: a read, then a transaction
    // that reopens every proposal the rule answered.
    let undone = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        present(&conn, id)?;
        made(proposals::unaccept(&conn, id).map_err(NoAnswer::Failed)?)
    })
    .await;

    match undone {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("un-accepting a proposal", &why),
    }
}

/// `POST /api/library/proposals/{id}/dismiss` — the answer is no.
///
/// Writes no rule: dismissing confirms the outcome doing nothing already has.
/// It has to persist all the same, or the next scan raises the same proposal
/// again — §6's first argument.
pub(crate) async fn dismiss(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    // `list`'s reasoning about the async worker.
    let dismissed = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        present(&conn, id)?;
        made(proposals::dismiss(&conn, id).map_err(|why| failed_unless_gone(&conn, id, why))?)
    })
    .await;

    match dismissed {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("dismissing a proposal", &why),
    }
}

/// `DELETE /api/library/proposals/{id}/dismiss` — undo it (O72).
///
/// 204: the proposal is open again, back in the inbox the dismissal took it
/// out of. This route is to `POST …/dismiss` what `DELETE …/accept` is to
/// `POST …/accept`, with one difference: it undoes the whole write, because a
/// dismissal wrote nothing a scan could have applied
/// ([`proposals::reopen`]'s own doc says why).
///
/// 409 for a proposal that is not a dismissal it can reopen: an open one has
/// nothing to reopen, and an accepted one, or a dismissed one still holding
/// an accept's rule, is reopened by un-accepting it — or, where that rule has
/// already gone, by the next scan. 404 for an id naming no
/// proposal, whether before the write or by the time the store read it
/// ([`failed_unless_gone`]).
pub(crate) async fn reopen(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    // `list`'s reasoning about the async worker.
    let reopened = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        present(&conn, id)?;
        made(proposals::reopen(&conn, id).map_err(|why| failed_unless_gone(&conn, id, why))?)
    })
    .await;

    match reopened {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("reopening a proposal", &why),
    }
}

/// `POST /api/library/matches/{id}/split` — keep a file joined by content
/// apart from the work pair `id` joined it to, and derive.
///
/// The body names the file, `{"work": <slug>, "edition": <key>}`: an edition
/// of one file that pair `id` names and joined, the pair its mark shows, as
/// both the pair's files sit in the work it joined. 200 `{"split": [ids],
/// "derive": <report>}`, `ids` being each pair [`matches::splits_for`] sets
/// apart.
pub(crate) async fn split_match(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let (work, edition) = match split_from_json(&body) {
        Ok(named) => named,
        Err(why) => return NoAnswer::Unusable(why).into_response(),
    };
    let by = principal.user_id;

    // `list`'s reasoning about the async worker, and a whole derive besides.
    let split = tokio::task::spawn_blocking(move || {
        deriving(&state, "split again", |conn| {
            matched(conn, id)?;
            let file = joined_file(conn, &work, &edition, id)?;
            let ids = match file {
                Some(file) => matches::splits_for(conn, id, &file).map_err(NoAnswer::Failed)?,
                None => None,
            }
            .ok_or_else(|| {
                NoAnswer::Absent(format!(
                    "match {id} did not join edition {edition} of work {work} by its one file"
                ))
            })?;
            matches::set_state(conn, &ids, matches::State::Split, Some(by))
                .map_err(NoAnswer::Failed)?;
            Ok(ids)
        })
    })
    .await;

    match split {
        Ok(Ok((ids, report))) => Json(serde_json::json!({
            "split": ids,
            "derive": crate::config_api::derive_report_json(&report),
        }))
        .into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("splitting a match", &why),
    }
}

/// `DELETE /api/library/matches/{id}/split` — undo the Split that set pair
/// `id` apart, and derive: its pairs join again where they still apply.
///
/// Restores every pair that Split set apart, with one derive
/// ([`matches::split_together`]); a pair not split is restored alone, which
/// changes nothing. 200 `{"derive": <report>}`.
pub(crate) async fn undo_split(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    // `split_match`'s reasoning.
    let undone = tokio::task::spawn_blocking(move || {
        deriving(&state, "undo again", |conn| {
            matched(conn, id)?;
            let together = matches::split_together(conn, id).map_err(NoAnswer::Failed)?;
            matches::set_state(conn, &together, matches::State::Scored, None)
                .map_err(NoAnswer::Failed)?;
            Ok(())
        })
    })
    .await;

    match undone {
        Ok(Ok(((), report))) => Json(serde_json::json!({
            "derive": crate::config_api::derive_report_json(&report),
        }))
        .into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("undoing a split", &why),
    }
}

/// A Split body's work slug and edition key.
fn split_from_json(body: &serde_json::Value) -> Result<(String, String), String> {
    let named = |field: &str| {
        body.get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                format!(
                    "a Split names the work by its slug and the edition by its key: \
                     `{field}` is missing or not a string"
                )
            })
    };
    Ok((named("work")?, named("edition")?))
}

/// `Ok` where pair `id` is stored.
fn matched(conn: &rusqlite::Connection, id: i64) -> Result<(), NoAnswer> {
    let stored: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM content_match WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| NoAnswer::Failed(format!("reading match {id}: {e}")))?;
    if stored {
        Ok(())
    } else {
        Err(NoAnswer::Absent(format!("there is no match {id}")))
    }
}

/// The digest of the one file of edition `key` of work `slug`, where pair `id`
/// joined that edition; `None` where the work holds no such edition, another
/// pair or none joined it, or it holds more files than one. A Split answers the
/// mark the panel shows, which is the edition's `joined_by`, and a pair names
/// one file.
fn joined_file(
    conn: &rusqlite::Connection,
    slug: &str,
    key: &str,
    id: i64,
) -> Result<Option<String>, NoAnswer> {
    conn.query_row(
        "SELECT min(m.sha256), count(*)
           FROM work w
           JOIN edition e ON e.work_id = w.id
           JOIN member m ON m.edition_id = e.id
          WHERE w.slug = ?1 AND e.key = ?2 AND e.joined_by = ?3",
        rusqlite::params![slug, key, id],
        |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)),
    )
    .map(|(file, files)| file.filter(|_| files == 1))
    .map_err(|e| NoAnswer::Failed(format!("reading edition {key} of work {slug}: {e}")))
}

/// `write`, then a derive of the whole store, in one transaction under the
/// job lock and the derive lease, each tried once (this module's own doc
/// says why); `again` is what the refusal tells the caller to do.
fn deriving<T>(
    state: &ServeState,
    again: &str,
    write: impl FnOnce(&rusqlite::Connection) -> Result<T, NoAnswer>,
) -> Result<(T, DeriveReport), NoAnswer> {
    let busy = || NoAnswer::Busy(crate::library_job::waits_for_the_job(again));
    let _job = match state.library_job.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return Err(busy()),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
    };
    let working = state.library.working().map_err(NoAnswer::Failed)?;
    let Some(lease) = state
        .library
        .derive_lease(lease::Wait::No)
        .map_err(NoAnswer::Failed)?
    else {
        return Err(busy());
    };
    lease.holding(|| {
        let tx = working
            .unchecked_transaction()
            .map_err(|e| NoAnswer::Failed(format!("starting the transaction: {e}")))?;
        let written = write(&tx)?;
        let report =
            derive::derive_with(&tx, derive::Documents::Reconcile).map_err(NoAnswer::Failed)?;
        tx.commit()
            .map_err(|e| NoAnswer::Failed(format!("committing the derive: {e}")))?;
        Ok((written, report))
    })
}

#[cfg(test)]
mod tests;
