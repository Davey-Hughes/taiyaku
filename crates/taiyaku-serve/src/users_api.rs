//! The `/api/library/users*` handlers — spec C4 §4 and §6.
//!
//! Thin by design, like `config_api.rs` and `curation.rs`: parse, call
//! [`taiyaku_library::account`], serialise. **Every refusal that stops this
//! screen locking an administrator out lives in the store**, beside
//! `account::promote`'s own — the CLI is a second writer, and a copy of an
//! invariant in each is a copy that drifts. This file's only job around them
//! is to give each the right status code.
//!
//! **Not `config_api`.** A third and fourth route family inside that module
//! would lengthen it and its tests.
//!
//! **No password ever leaves here.** [`taiyaku_library::account::Account`]
//! carries no hash by construction — `by_name` returns it separately for
//! exactly one comparison — so there is no field for [`user_json`] to leak
//! even by accident.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use taiyaku_library::account::{self, Account, KeySummary, SessionSummary};

use crate::{ServeState, auth::Principal};

/// Why a users route could not answer.
///
/// Shaped like `config_api::AdminAnswer`.
#[derive(Debug)]
enum UserAnswer {
    /// `id` names no account — 404.
    Missing(i64),
    /// Nothing under `{id}` matches the session or key named — 404.
    ///
    /// **A variant of its own, because [`Self::Missing`]'s body is wrong
    /// here.** It says there is no account, which a stale session or key under
    /// an account that exists does not mean. The message below names only what
    /// was looked for, since `account::revoke_session` never checks that the
    /// account exists.
    NoSuch(&'static str),
    /// The caller's own mistake: a body that could not be read, a name
    /// already taken, a role the schema's `CHECK` refuses — 400.
    Unusable(String),
    /// A request that is **well-formed, names a real account and is still not
    /// allowed** — C4 §4's refusals. `409 Conflict` rather than `400`,
    /// because nothing about the request is malformed and retrying it
    /// unchanged will keep failing for a reason about the library's state;
    /// and rather than `500`, because nothing failed.
    Refused(String),
    /// A store failure — 500. The store's text goes to the log alone, and the
    /// client is sent [`crate::routes::COULD_NOT_ANSWER`] (O387).
    Failed(String),
}

impl IntoResponse for UserAnswer {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Failed(why) => return crate::routes::store_failed_as_json(&why),
            Self::Missing(id) => (StatusCode::NOT_FOUND, format!("there is no account {id}")),
            Self::NoSuch(what) => (
                StatusCode::NOT_FOUND,
                format!("no such {what} under that account"),
            ),
            Self::Unusable(why) => (StatusCode::BAD_REQUEST, why),
            Self::Refused(why) => (StatusCode::CONFLICT, why),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
/// The one place a store refusal becomes a status.
///
/// **Shared by all three writers.** `update`, `remove` and `removal_cost`
/// return the same [`account::WriteError`], and mapping it at each call site
/// would let them disagree about the status of the same refusal.
fn answered(why: account::WriteError) -> UserAnswer {
    match why {
        account::WriteError::Unusable(why) => UserAnswer::Unusable(why),
        account::WriteError::Refused(why) => UserAnswer::Refused(why),
        account::WriteError::Failed(why) => UserAnswer::Failed(why),
    }
}

/// `config_api::admin_panicked`'s twin for this module: the one failure
/// `spawn_blocking` itself can hand back, as JSON like every other refusal
/// these routes send, the panic's own message in the log alone (O387).
fn user_panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    crate::routes::panicked_as_json(what, why)
}

/// One account, whole, as C4 §4's screen shows it: the row, whether anybody
/// can log in as it, its wrong passwords this window, whether it sees every
/// library, its sessions, its keys and the libraries it is granted.
///
/// **`every_library` beside `libraries`**, because an empty list is no
/// library and cannot also say "every library". An administrator's is sent
/// as stored, though the role sees every library whatever it says.
///
/// **All of it together rather than a row plus three more requests.** The
/// screen shows them beside the account, and a list that answered the row
/// alone would render in stages.
fn user_json(
    account: &Account,
    can_log_in: bool,
    wrong_passwords: i64,
    every_library: bool,
    sessions: &[SessionSummary],
    keys: &[KeySummary],
    libraries: &[(i64, String)],
) -> serde_json::Value {
    serde_json::json!({
        "id": account.id,
        "name": account.name,
        "role": account.role,
        "can_download": account.can_download,
        "can_stream": account.can_stream,
        "can_stamp": account.can_stamp,
        // An account still holding `credential::LOCKED` has a row and no way
        // in, and nothing else about it says so — `account::list`'s own doc
        // comment calls this half "the one worth printing".
        "can_log_in": can_log_in,
        // This window's wrong passwords from every address (O279), with the
        // verdict beside them, so the ceiling lives only in
        // `account::ALARM_AFTER` and the screen cannot disagree with the log.
        "wrong_passwords": wrong_passwords,
        "wrong_password_alarm": wrong_passwords >= account::ALARM_AFTER,
        "sessions": sessions.iter().map(|s| serde_json::json!({
            "id": s.id,
            "created": s.created,
            "last_seen": s.last_seen,
            "user_agent": s.user_agent,
            "ip": s.ip,
            "current": s.current,
        })).collect::<Vec<_>>(),
        "keys": keys.iter().map(|k| serde_json::json!({
            "id": k.id,
            "name": k.name,
            "created": k.created,
            "last_used": k.last_used,
        })).collect::<Vec<_>>(),
        "every_library": every_library,
        "libraries": libraries.iter().map(|(id, name)| serde_json::json!({
            "id": id,
            "name": name,
        })).collect::<Vec<_>>(),
    })
}

/// Read one account's sessions, keys and grants and render it whole.
///
/// **`current_token` is the caller's own session cookie**, and it is a
/// parameter rather than `None` because [`SessionSummary::current`] is
/// otherwise a constant on this route, so C4 §6.2's "whether it is the browser
/// doing the asking" would never say yes — on the one screen where revoking
/// the wrong row signs the administrator out of the browser they are working
/// in. Only the caller's own account can hold a session whose sha256 matches,
/// so passing it for every account is correct rather than merely harmless:
/// [`account::sessions_of`] compares hashes, and inverting one is the sha256
/// preimage problem.
fn assembled(
    conn: &rusqlite::Connection,
    account: &Account,
    can_log_in: bool,
    current_token: Option<&str>,
) -> Result<serde_json::Value, String> {
    let sessions = account::sessions_of(conn, account.id, current_token)?;
    let keys = account::keys_of(conn, account.id)?;
    let every_library = account::access_for(conn, account.id)
        .map_err(|e| format!("reading which libraries account {} sees: {e}", account.id))?
        == account::Access::Every;
    let libraries = account::granted_libraries(conn, account.id)?;
    let wrong_passwords = account::account_failures(conn, account.id)?;
    Ok(user_json(
        account,
        can_log_in,
        wrong_passwords,
        every_library,
        &sessions,
        &keys,
        &libraries,
    ))
}

/// `GET /api/library/users` — every account, with what C4 §4's screen shows
/// beside each.
pub(crate) async fn list_users(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let token = crate::session::token_of(&headers);

    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let mut rendered = Vec::new();
        for (account, can_log_in) in account::list(&conn)? {
            rendered.push(assembled(&conn, &account, can_log_in, token.as_deref())?);
        }
        Ok::<_, String>(rendered)
    })
    .await;

    match built {
        Ok(Ok(users)) => Json(serde_json::json!({ "users": users })).into_response(),
        Ok(Err(why)) => UserAnswer::Failed(why).into_response(),
        Err(why) => user_panicked("listing accounts", &why),
    }
}

/// `POST /api/library/users` — create an account. C4 §4: no self-service
/// registration; an admin creates accounts, per A2 §11.
///
/// **`role` defaults to `user`.** `004_auth.sql` made the column default that
/// way because "any insert that omitted the column minted an administrator",
/// and this route must not have a second opinion.
pub(crate) async fn create_user(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let (Some(name), Some(password)) = (
        body.get("name").and_then(serde_json::Value::as_str),
        body.get("password").and_then(serde_json::Value::as_str),
    ) else {
        return UserAnswer::Unusable(
            "creating an account needs a `name` and a `password`".to_string(),
        )
        .into_response();
    };
    // **A non-string `role` is refused rather than defaulted.** Defaulting it
    // with `and_then(as_str).unwrap_or("user")` would collapse "absent" and
    // "present but not a string" into one answer, so `{"role": 123}` would
    // silently create a reader where [`user_patch_from_json`] 400s on the
    // identical shape.
    let role = match body.get("role") {
        None | Some(serde_json::Value::Null) => "user".to_string(),
        Some(serde_json::Value::String(v)) => v.clone(),
        Some(_) => {
            return UserAnswer::Unusable("`role` must be a string".to_string()).into_response();
        }
    };
    let (name, password) = (name.to_string(), password.to_string());

    let created = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        // **A duplicate name, and a role the schema's CHECK rejects, are both
        // the caller's own mistake**, not a fault — `user.name` is `UNIQUE`
        // and `role` carries `CHECK (role IN ('admin', 'user'))`, so a 500
        // here would report a typo as a server failure.
        let id = account::add(&conn, &name, &password, &role).map_err(UserAnswer::Unusable)?;
        let account = account::by_id(&conn, id)
            .map_err(UserAnswer::Failed)?
            .ok_or(UserAnswer::Missing(id))?;
        // **`None`, and not the caller's token.** An account created a moment
        // ago has no sessions at all, so there is nothing here for a token to
        // match — and the caller's session belongs to a different account by
        // construction.
        assembled(&conn, &account, true, None).map_err(UserAnswer::Failed)
    })
    .await;

    match created {
        Ok(Ok(user)) => Json(serde_json::json!({ "user": user })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("creating an account", &why),
    }
}

/// Whether anybody can log in as `id`.
///
/// [`account::list`] carries the flag beside each row and there is no
/// per-account reader for it; adding one to the store for a single caller is a
/// wider change than reading the list this route already has reason to trust.
fn can_log_in(conn: &rusqlite::Connection, id: i64) -> Result<bool, String> {
    Ok(account::list(conn)?
        .into_iter()
        .find(|(a, _)| a.id == id)
        .is_some_and(|(_, can)| can))
}

/// A patch, as [`patch_user`]'s body is read.
///
/// **Which libraries the account sees is two keys, and at most one is
/// sent.** `{"every_library": true}` lifts the limit, and `{"libraries":
/// [ids]}` sets "only these", where `[]` is no library. With neither, the
/// grants are left alone. `every_library` other than `true` is refused: "not
/// every library" says nothing `libraries` does not, and guessing which list
/// it meant would be guessing what an account may see.
///
/// **`password` rides on the patch**, so the store applies it in the same
/// transaction as the columns — `account::Patch`'s own doc carries what the
/// separation cost.
fn user_patch_from_json(body: &serde_json::Value) -> Result<account::Patch, String> {
    let mut patch = account::Patch::default();
    if let Some(v) = body.get("name") {
        patch.name = Some(
            v.as_str()
                .ok_or_else(|| "`name` must be a string".to_string())?
                .to_string(),
        );
    }
    if let Some(v) = body.get("role") {
        patch.role = Some(
            v.as_str()
                .ok_or_else(|| "`role` must be a string".to_string())?
                .to_string(),
        );
    }
    if let Some(v) = body.get("can_download") {
        patch.can_download = Some(
            v.as_bool()
                .ok_or_else(|| "`can_download` must be a boolean".to_string())?,
        );
    }
    if let Some(v) = body.get("can_stream") {
        patch.can_stream = Some(
            v.as_bool()
                .ok_or_else(|| "`can_stream` must be a boolean".to_string())?,
        );
    }
    if let Some(v) = body.get("can_stamp") {
        patch.can_stamp = Some(
            v.as_bool()
                .ok_or_else(|| "`can_stamp` must be a boolean".to_string())?,
        );
    }
    // **The old key is refused, not ignored.** An ignored `roots` would
    // answer 200 having changed no grant, to a client that believes it
    // restricted an account.
    if body.get("roots").is_some() {
        return Err(
            "`roots` is no longer accepted: send `libraries`, a list of library ids".to_string(),
        );
    }
    let every_library = body.get("every_library");
    let libraries = body.get("libraries");
    if every_library.is_some() && libraries.is_some() {
        return Err("send `every_library` or `libraries`, not both".to_string());
    }
    if let Some(v) = every_library {
        if v.as_bool() != Some(true) {
            return Err(
                "`every_library` can only be `true`: send `libraries`, a list of library ids, \
                 to limit the account"
                    .to_string(),
            );
        }
        patch.grant = Some(account::Grant::Every);
    }
    if let Some(v) = libraries {
        let listed = v
            .as_array()
            .ok_or_else(|| "`libraries` must be an array of library ids".to_string())?;
        let mut ids = Vec::with_capacity(listed.len());
        for entry in listed {
            ids.push(
                entry
                    .as_i64()
                    .ok_or_else(|| "`libraries` must be an array of library ids".to_string())?,
            );
        }
        patch.grant = Some(account::Grant::Libraries(ids));
    }
    if let Some(v) = body.get("password") {
        patch.password = Some(
            v.as_str()
                .ok_or_else(|| "`password` must be a string".to_string())?
                .to_string(),
        );
    }
    Ok(patch)
}

/// `PATCH /api/library/users/{id}` — change only what the body names.
///
/// **A `password` here is an administrator resetting somebody else's**, so the
/// store ends every session the target has. A2 §4's "keeps the session that
/// made it" is a rule about self-service, and there is no session of the
/// *target's* that made this one. PR 2's design §1.1 records what that beat.
/// API keys are deliberately untouched — independent credentials with their
/// own revocation, and bundling them would silently break every reader in the
/// house.
///
/// **Which is why an administrator cannot reset their *own* password here.**
/// That justification is exactly false when the target is the caller: the
/// session that made the change *is* the target's, A2 §4 says to keep it, and
/// this route would end it — signing the administrator out of the browser they
/// are working in, on the next request. `/api/password` is the self-service
/// route that gets this right, and C4 §8 already frames this screen as "the
/// administrative view of *other people*". Refused with that route named.
pub(crate) async fn patch_user(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    // An administrator patching their *own* capabilities gets their own row
    // back, and a row disagreeing with the list about which session is current
    // would be a screen contradicting itself one render later.
    let token = crate::session::token_of(&headers);
    let patch = match user_patch_from_json(&body) {
        Ok(read) => read,
        Err(why) => return UserAnswer::Unusable(why).into_response(),
    };
    let acting_as = principal.user_id;
    if patch.password.is_some() && acting_as == id {
        return UserAnswer::Refused(
            "an administrator resets other accounts' passwords here and their own at \
             `/api/password`, which keeps the session making the change"
                .to_string(),
        )
        .into_response();
    }

    let updated = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        if account::by_id(&conn, id)
            .map_err(UserAnswer::Failed)?
            .is_none()
        {
            return Err(UserAnswer::Missing(id));
        }
        // **The store says which of the three this is; this route only maps
        // it to a status.** The three are `account::WriteError`'s cases, and
        // `answered` maps them.
        //
        // **The password goes through the same call**, inside the same
        // transaction as the columns: a store fault on a password set
        // separately would leave the role and capability changes committed
        // beside a failed request.
        let account = account::update(&conn, id, &patch, Some(acting_as)).map_err(answered)?;
        let can = can_log_in(&conn, id).map_err(UserAnswer::Failed)?;
        assembled(&conn, &account, can, token.as_deref()).map_err(UserAnswer::Failed)
    })
    .await;

    match updated {
        Ok(Ok(user)) => Json(serde_json::json!({ "user": user })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("patching an account", &why),
    }
}

/// The one query parameter [`delete_user`] reads.
///
/// **`deny_unknown_fields`, for the reason `config_api::DryRunQuery`
/// carries in full**: without it `serde_urlencoded` drops any key this struct
/// does not name rather than refusing the request, so `?dryrun=true` — one key
/// off from the one this struct actually reads — would answer `200 OK` and run
/// the real, committed removal. This route is the second-most destructive in
/// the product.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteUserQuery {
    #[serde(default)]
    dry_run: bool,
}

/// `DELETE /api/library/users/{id}` — remove an account and everything
/// `ON DELETE CASCADE` takes with it; or, with `?dry_run=true`, find out what
/// that would cost without doing any of it.
///
/// **C4 §4 requires the counts stated before it acts**, and this answers the
/// identical body shape for both requests — `{"removed": {…}, "dry_run": …}` —
/// which is exactly what `config_api::delete_root` does for a root and for the
/// same reason: a plain `DELETE` cannot show a caller anything before
/// removing.
///
/// **The counts a real removal answers with are [`account::remove`]'s own
/// return value**, counted inside the transaction that deletes. Recomputing
/// them afterwards would answer zero for the numbers this route exists to
/// state honestly.
///
/// **Nothing here deletes a work, an edition or a member.** That is the
/// difference from a root's removal, and it is why this route states five
/// counts rather than `delete_root`'s three.
pub(crate) async fn delete_user(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Query(query): Query<DeleteUserQuery>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let dry_run = query.dry_run;

    let removed = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        if account::by_id(&conn, id)
            .map_err(UserAnswer::Failed)?
            .is_none()
        {
            return Err(UserAnswer::Missing(id));
        }
        // **Both calls map all three cases, and the quote most of all.**
        // Folding `removal_cost`'s errors into `Failed` would answer a dry run
        // of removing the last administrator with **500** where the real
        // removal answers 409 — a routine refusal reported as a server fault,
        // on the safety feature of the destructive route. And folding
        // `remove`'s into `Refused` would make a disk fault a conflict.
        if dry_run {
            account::removal_cost(&conn, id).map_err(answered)
        } else {
            account::remove(&conn, id).map_err(answered)
        }
    })
    .await;

    match removed {
        Ok(Ok(removal)) => Json(serde_json::json!({
            "removed": {
                "name": removal.name,
                "positions": removal.positions,
                "notes": removal.notes,
                "sessions": removal.sessions,
                "keys": removal.keys,
                "grants": removal.grants,
            },
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("removing an account", &why),
    }
}

/// `DELETE /api/library/users/{id}/sessions/{sid}` — C4 §4's "a device was
/// lost", for a browser.
///
/// **`{sid}` is the token's sha256**, which is what
/// [`taiyaku_library::account::SessionSummary::id`] already is and what that
/// struct's own doc comment argues is safe to hand a browser: "it is what the
/// server compares against, not what a client presents, and inverting it is the
/// sha256 preimage problem."
///
/// **Scoped to `{id}`**, because [`account::revoke_session`] is: an id from
/// one account's listing must not end another account's session. A session
/// that is not this account's is a 404, the same answer as one that never
/// existed — the route does not confirm that somebody else holds it.
pub(crate) async fn revoke_session(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath((id, sid)): AxPath<(i64, String)>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let revoked = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        account::revoke_session(&conn, id, &sid).map_err(UserAnswer::Failed)
    })
    .await;

    match revoked {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => UserAnswer::NoSuch("session").into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("revoking a session", &why),
    }
}

/// `DELETE /api/library/users/{id}/keys/{kid}` — the same, for the credential
/// an OPDS reader holds.
///
/// Scoped to `{id}` for the reason [`revoke_session`] states, and
/// [`account::revoke_key`] enforces it in SQL rather than trusting this route
/// to.
pub(crate) async fn revoke_key(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath((id, kid)): AxPath<(i64, i64)>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let revoked = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        account::revoke_key(&conn, id, kid).map_err(UserAnswer::Failed)
    })
    .await;

    match revoked {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => UserAnswer::NoSuch("API key").into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("revoking an API key", &why),
    }
}

#[cfg(test)]
mod tests;
