//! The `/api/library/rules*` handlers — spec C4 §6, task 8 of the branch plan.
//!
//! Thin by design, like `curation.rs`: parse, call [`taiyaku_library::rules`]
//! or [`taiyaku_library::preview`], serialise. Every decision about what a
//! rule may write, or what a preview means, lives in those modules; this file
//! only carries JSON across the boundary.
//!
//! **Not `library_api` and not `curation`.** `library_api` is C1's read layer
//! and carries the root grant into every query; `curation` answers proposals,
//! which are the system's own suggestions. These routes edit the rule set
//! itself — admin-only, and about the library's organisation rather than
//! about what any one account may see, the same standing C4's opening line
//! ("everything here is admin-gated per A2") gives every route in this spec.
//!
//! # `admin_or_forbidden` is defined here, not reused from `curation`
//!
//! `curation::admin_or_forbidden` is a private `fn`, reachable only from
//! `curation` and its own `tests` submodule — not from a sibling module like
//! this one. Rather than widen its visibility for a borrow across an
//! unrelated spec, this file mirrors its shape (the task 8 brief's own
//! "curation.rs has its own `admin_or_forbidden`; decide whether to reuse it
//! or mirror it") and declares its own, `pub(crate)` — the roots and scan
//! routes tasks 11–12 add are C4 routes too and reach this one directly
//! rather than each growing a third copy.
//!
//! # Two boundaries this module keeps a rule id off
//!
//! **`preview_rules`'s `selected` is an index into the rule list the request
//! carries, never a rule id.** [`taiyaku_library::preview::preview_with_rows`]'s
//! own doc comment gives the reason in full: an unsaved rule in C4's editor
//! carries `id: 0`, which collides with `taiyaku_library::rules::is_root_fallback`'s
//! own sentinel for "the root's own default column decided this" the moment
//! two unsaved rules sit in one request. A position in the exact list the
//! caller just sent has no such collision, whatever id any rule in it does or
//! does not carry yet. [`taiyaku_library::preview::Override::first`] and
//! `::wins` are the same boundary, for the same reason, and this module passes
//! both straight through rather than resolving either back to an id.
//!
//! **The preview route's own reads never open a second transaction to check
//! `selected` is in range.** [`preview::preview_with_rows`] would answer that
//! itself — "an error naming the index and `proposed`'s own length if that
//! index is out of range" — but folded into whatever else its one `Result`
//! carries, the same fold `curation.rs`'s own module doc records measuring as
//! `400 Bad Request :: reading scan entries for the dry run: no such table:
//! scan_entry` on the identical mistake for `proposals::resolves`. `selected`
//! is checked against `proposed.len()` here, before `spawn_blocking` ever
//! opens a connection, so a client's own out-of-range index is answered 400
//! from the request alone and a real derivation fault stays a 500.
//!
//! # `RulePatch`'s `Option<Option<T>>` columns, over JSON
//!
//! Six of `RulePatch`'s eleven fields are nullable columns
//! (`root_id`, `note`, and the four `set_*`s), and the type says a patch must
//! be able to leave one alone (outer `None`) or clear it to `NULL`
//! (`Some(None)`) — two different instructions a single-layer `Option` cannot
//! tell apart. JSON has exactly the same three-way shape already: a key can be
//! **absent**, present and **`null`**, or present with a **value** — so
//! [`rule_patch_from_json`] reads `body.get(field)` itself rather than
//! `body.get(field).and_then(Value::as_str)` (which collapses absent and
//! `null` into one `None`, the shape `curation::rule_from_json` deliberately
//! wants for `ProposedRule`'s single-layer optionals and deliberately wrong
//! here): `None` (key absent) → outer `None`, `Some(&Value::Null)` → `Some(None)`,
//! anything else → `Some(Some(value))`. The five non-nullable columns
//! (`ordinal`, `enabled`, `kind`, `pattern`, `match_on`) get the ordinary
//! two-way reading, since the schema itself gives a caller no way to clear
//! them.
//!
//! # The preview route's connection, and what it must never cost
//!
//! [`preview_rules`] takes its connection from [`taiyaku_library::Library::working`],
//! not [`taiyaku_library::Library::conn`] — the one rule task 8's own brief
//! states explicitly, because `ServeState.library` is `Arc<Library>` holding a
//! single `Mutex<Connection>`, and a derivation on that handle stalls every
//! other request behind it for as long as the derivation runs. Measured at a
//! **median of 281 ms** on a real library — 130 series, 1858 works
//! (`docs/notes.md`, "What a rule preview costs on a real library") — which is
//! also why this doc comment does not call the route safe to call on every
//! keystroke: C4 §6 did, and the build design's own re-measurement
//! (`docs/superpowers/specs/2026-09-02-library-configuration-build-design.md`,
//! §2) records the number and the decision it forced — the editor previews on
//! demand, not per keystroke, and that decision is the client's; this route
//! only has to answer the request it is actually sent.
//!
//! **281 ms is `preview_with_rows` alone — `preview::overrides` is a second,
//! quadratic term in the same request, in the number of proposed rules.**
//! Measured on a one-work fixture (so essentially the rule term by itself,
//! debug build): 10 extra rules cost 47 ms, 40 → 401 ms, 80 → 1.34 s,
//! 160 → 5.01 s. [`MATCHED_FILES_LIMIT`] bounds `preview_with_rows`'s
//! *output*; [`MAX_PROPOSED_RULES`] is this route's bound on the *input* both
//! derivations share — the term the quadratic cost is actually in, and the
//! one dimension neither limit touched before it existed.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::OptionalExtension;
use taiyaku_library::{derive, hand, preview, rules, scan};

use crate::{ServeState, auth::Principal};

/// The matched-files table's own cap for [`preview_rules`] — a number this
/// route chooses for itself; nothing in spec C4 sets one, and
/// [`preview::Rows::truncated`] is what tells a caller the cap was actually
/// reached rather than that the table happened to hold exactly this many
/// rows, the same discipline that field's own doc comment states. Generous
/// against the 2,044-row `scan_entry` table `docs/notes.md` measured, and
/// still bounded: an editor showing every match for a near-universal prefix
/// rule has no use for thousands of identical rows in one response.
const MATCHED_FILES_LIMIT: usize = 500;

/// Both [`preview_rules`]'s and [`save_rules`]'s cap on the `rules` array
/// itself — the term [`preview::overrides`] is quadratic in, per this
/// module's own doc comment on what `preview_rules` costs and why 281 ms is
/// only half the story. `axum`'s 2 MB body default was, until this constant
/// existed, the only bound on that array at all.
///
/// **200 is chosen, not measured** — nothing in spec C4 sets a ceiling, and
/// the seeded default rule set is four rules, fifty times smaller than this.
/// Extrapolating the measured curve (10 extra rules 47 ms, 40 → 401 ms,
/// 80 → 1.34 s, 160 → 5.01 s, debug build, one-work fixture), 200 rules costs
/// on the order of 8 s in that same debug build — generous enough to hold any
/// rule set a person plausibly curates by hand, since a library with 200
/// active rules would already be unmanageable through the editor's own UI
/// long before it threatened this route, while still refusing the
/// pathological case (an automated or malformed caller inflating the array)
/// before it ever reaches [`preview::overrides`] rather than after paying for
/// it.
const MAX_PROPOSED_RULES: usize = 200;

/// Why a config-API route could not answer.
///
/// One type rather than a `Response` built where the failure happened, for
/// the reason `curation::NoAnswer`'s own doc comment gives: every handler
/// here does its SQLite work inside `spawn_blocking`, which flattens a
/// handler's failures into one `Result` however different they were on the
/// way in.
#[derive(Debug)]
enum NoAnswer {
    /// `id` names no rule — 404.
    Missing(i64),
    /// The caller's own text, decided before any store call: a body that
    /// could not be read as a rule, a new rule or a patch; a `selected` index
    /// out of range for the proposed list; or — [`import_rules`]'s one
    /// admitted compromise, stated in full on that function's own doc
    /// comment — anything [`taiyaku_library::rules::import_json`] refused,
    /// since that function has no `evaluable`-shaped split of its own to ask
    /// the "is this document even well-formed" half without a connection.
    Unusable(String),
    /// A store failure, which is a 500 rather than anything a caller can fix.
    Failed(String),
    /// A scan or another save already holds [`crate::ServeState::library_job`]
    /// — 409, [`save_rules`]'s own whole-branch review, I4. The same spirit as
    /// [`start_scan`]'s own `409` for a second scan of one root: naming why is
    /// cheaper than a caller discovering it by watching the clock.
    Busy(String),
}

impl IntoResponse for NoAnswer {
    fn into_response(self) -> Response {
        match self {
            Self::Missing(id) => {
                (StatusCode::NOT_FOUND, format!("there is no rule {id}")).into_response()
            }
            Self::Unusable(why) => (StatusCode::BAD_REQUEST, why).into_response(),
            Self::Failed(why) => (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
            Self::Busy(why) => (StatusCode::CONFLICT, why).into_response(),
        }
    }
}

/// The one failure `spawn_blocking` itself can hand back: the closure
/// panicked and nothing answered at all. Worded the way `curation::panicked`
/// words its own.
fn panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    NoAnswer::Failed(format!("{what} panicked: {why}")).into_response()
}
/// Whether the `root` table holds `id`.
///
/// Two different questions share this one query. [`create_rule`] and
/// [`patch_rule`] ask it before writing a `root_id` naming one, so a stale or
/// since-removed root is this caller's own mistake (400, naming the root)
/// rather than SQLite's own `FOREIGN KEY constraint failed`, surfaced
/// verbatim, the fold this module's own doc comment spends two paragraphs
/// refusing for `selected`. [`delete_root`] asks it about `id` itself, before
/// [`scan::removal_cost`]/[`scan::remove_root`] ever run — neither reports
/// "no such root" on its own, since every statement `apply_removal`
/// (`scan.rs`) issues simply matches nothing for an id no row holds, and
/// `remove_root` would still pay for a full re-derive over an id that named
/// nothing to remove.
fn root_exists(conn: &rusqlite::Connection, id: i64) -> Result<bool, String> {
    conn.query_row("SELECT 1 FROM root WHERE id = ?1", [id], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
        .map_err(|e| format!("checking whether root {id} exists: {e}"))
}

/// [`root_exists`], asked once per `root_id` a proposed rule list names —
/// whole-branch review, M2.
///
/// **[`create_rule`] and [`patch_rule`] already refuse a stale or removed
/// root this way for one rule at a time; [`preview_rules`] and [`save_rules`]
/// propose a whole list at once**, and neither called [`root_exists`] at
/// all until now — a rule scoped to a root deleted since answered with
/// [`crate::rules::replace_all`]'s own `FOREIGN KEY constraint failed`,
/// surfaced verbatim, the exact fold [`root_exists`]'s own doc comment spends
/// a paragraph refusing. Shared rather than duplicated into both routes'
/// `spawn_blocking` closures, for the reason [`read_proposed_rules`] is
/// shared: two copies of the same loop are two places to forget the check
/// the next time a third route proposes a rule list.
fn check_proposed_roots(
    conn: &rusqlite::Connection,
    proposed: &[rules::Rule],
) -> Result<(), NoAnswer> {
    for rule in proposed {
        if let Some(root_id) = rule.root_id
            && !root_exists(conn, root_id).map_err(NoAnswer::Failed)?
        {
            return Err(NoAnswer::Unusable(format!("there is no root {root_id}")));
        }
    }
    Ok(())
}

/// The one complaint [`rules::Engine::compile`] would raise for a single rule
/// with this `kind`/`pattern`, or `None` when `kind` never compiles a pattern
/// at all ([`rules::RuleKind::Prefix`] and the two builtins) or the pattern
/// compiles clean.
///
/// Built over [`rules::Engine::compile`] itself rather than calling
/// `regex::Regex::new` directly — `regex` is not a dependency of this crate,
/// and going through `compile` keeps the write-time refusal
/// ([`create_rule`], [`patch_rule`]) and the read-time report ([`list_rules`],
/// [`import_rules`]) naming the identical error for the identical mistake,
/// since both ultimately call the same function.
fn regex_complaint(kind: rules::RuleKind, pattern: &str) -> Option<String> {
    let probe = rules::Rule {
        id: 0,
        ordinal: 0,
        enabled: true,
        root_id: None,
        kind,
        pattern: pattern.to_string(),
        match_on: rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: None,
        note: None,
    };
    let (_engine, complaints) = rules::Engine::compile(std::slice::from_ref(&probe));
    complaints.into_iter().next()
}

/// [`regex_complaint`]'s twin for [`patch_rule`], where a patch can name
/// `kind` without `pattern`, `pattern` without `kind`, or neither — the
/// *resulting* rule after the patch applies is what has to compile, not the
/// patch in isolation, so a `kind: regex` patch against a rule whose existing
/// `pattern` is not a valid regex must be caught exactly as a `pattern` patch
/// against a rule already `kind: regex` would be.
///
/// Reads the current row only when at least one of `kind`/`pattern` is
/// actually being patched — neither changing means whatever is already on
/// disk is unaffected by this write, and [`list_rules`]/[`import_rules`] are
/// where an already-broken row already gets reported.
///
/// `Ok(None)` when `id` names no rule at all: `patch_rule`'s own call to
/// [`rules::update`] is what answers that as 404, so this only has to fall
/// through cleanly rather than invent a second 404 of its own.
fn patched_pattern_complaint(
    conn: &rusqlite::Connection,
    id: i64,
    patch: &rules::RulePatch,
) -> Result<Option<String>, String> {
    if patch.kind.is_none() && patch.pattern.is_none() {
        return Ok(None);
    }
    let existing: Option<(String, String)> = conn
        .query_row("SELECT kind, pattern FROM rule WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(|e| format!("reading rule {id}: {e}"))?;
    let Some((existing_kind, existing_pattern)) = existing else {
        return Ok(None);
    };
    let kind = match patch.kind {
        Some(kind) => kind,
        None => existing_kind
            .parse::<rules::RuleKind>()
            .map_err(|e| format!("rule {id}: {e}"))?,
    };
    let pattern = patch.pattern.clone().unwrap_or(existing_pattern);
    Ok(regex_complaint(kind, &pattern))
}

// --- rule <-> JSON -----------------------------------------------------

/// One `rule` row, as [`list_rules`] and [`import_rules`] send it and
/// [`preview_rules`] reads it back.
fn rule_json(rule: &rules::Rule) -> serde_json::Value {
    serde_json::json!({
        "id": rule.id,
        "ordinal": rule.ordinal,
        "enabled": rule.enabled,
        "root_id": rule.root_id,
        "kind": rule.kind.to_string(),
        "pattern": rule.pattern,
        "match_on": rule.match_on.to_string(),
        "set_series": rule.set_series,
        "set_edition_key": rule.set_edition_key,
        "set_kind": rule.set_kind,
        "set_language": rule.set_language,
        "note": rule.note,
    })
}

/// A required string field, or the caller's own mistake named against
/// `noun` — `"rule"` from [`rule_from_json`]/[`new_rule_from_json`],
/// `"root"` from [`root_path_from_json`], the one root-route caller of this
/// helper. Shared rather than duplicated per route, so the wording for a
/// missing or mistyped string field cannot drift between the two callers —
/// [`root_patch_from_json`]'s own doc comment says the same for
/// [`patch_bool`]/[`patch_nullable_text`]/[`patch_nullable_i64`], the other
/// three helpers this file shares between the rules and roots routes.
fn required_str<'a>(
    body: &'a serde_json::Value,
    field: &str,
    noun: &str,
) -> Result<&'a str, String> {
    body.get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("the {noun}'s `{field}` must be a string"))
}

/// A required, non-nullable numeric field.
fn required_i64(body: &serde_json::Value, field: &str) -> Result<i64, String> {
    body.get(field)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("the rule's `{field}` must be a number"))
}

/// A required boolean field.
fn required_bool(body: &serde_json::Value, field: &str) -> Result<bool, String> {
    body.get(field)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| format!("the rule's `{field}` must be a boolean"))
}

/// A field that is absent-or-`null`-or-a-number — the ordinary single-layer
/// `Option<i64>` reading `Rule::root_id` and `NewRule::root_id` both want,
/// where absent and `null` mean the identical thing: no root, every root.
fn optional_i64(body: &serde_json::Value, field: &str) -> Result<Option<i64>, String> {
    match body.get(field) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => Ok(Some(value.as_i64().ok_or_else(|| {
            format!("the rule's `{field}` must be a number or null")
        })?)),
    }
}

/// A field that is absent-or-`null`-or-a-string — `Rule`/`NewRule`'s
/// single-layer optional text columns.
fn optional_text(body: &serde_json::Value, field: &str) -> Result<Option<String>, String> {
    match body.get(field) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("the rule's `{field}` must be a string or null")),
    }
}

/// A rule, as [`read_proposed_rules`] reads each element of the `rules` array
/// [`preview_rules`] or [`save_rules`] was sent.
///
/// **`id` defaults to `0` rather than being required.** That is
/// `taiyaku_library::rules::Rule`'s own sentinel for "not saved yet" — an
/// editor proposing a brand-new rule has no id to send for it at all, and
/// requiring one would make every unsaved row spell that out by hand as a
/// literal `0` instead of simply omitting the field.
fn rule_from_json(body: &serde_json::Value) -> Result<rules::Rule, String> {
    if !body.is_object() {
        return Err("a rule must be a JSON object".to_string());
    }
    let id = match body.get("id") {
        None | Some(serde_json::Value::Null) => 0,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| "the rule's `id` must be a number".to_string())?,
    };
    Ok(rules::Rule {
        id,
        ordinal: required_i64(body, "ordinal")?,
        enabled: required_bool(body, "enabled")?,
        root_id: optional_i64(body, "root_id")?,
        kind: required_str(body, "kind", "rule")?.parse()?,
        pattern: required_str(body, "pattern", "rule")?.to_string(),
        match_on: required_str(body, "match_on", "rule")?.parse()?,
        set_edition_key: optional_text(body, "set_edition_key")?,
        set_kind: optional_text(body, "set_kind")?,
        set_language: optional_text(body, "set_language")?,
        set_series: optional_text(body, "set_series")?,
        note: optional_text(body, "note")?,
    })
}

/// The `rules` array both [`preview_rules`] and [`save_rules`] read off their
/// body, mapped through [`rule_from_json`] and bounded by
/// [`MAX_PROPOSED_RULES`] the identical way.
///
/// **Extracted rather than left as the near-verbatim block each route had.**
/// The design's own requirement is that a save be "symmetric with
/// `POST /rules/preview` down to the body key, the `MAX_PROPOSED_RULES`
/// bound and the `rule_from_json` reader — the same list previews and saves,
/// so the client cannot send one shape to one and another to the other." Two
/// independently-editable copies of this block can only keep that promise by
/// staying in step by hand, which is exactly the kind of promise a codebase
/// breaks the first time one copy is touched and the other is not; one
/// function reading both `rules` arrays makes the symmetry structural
/// instead — there is no second copy left to drift.
///
/// `noun` is the same shared idiom [`required_str`]/[`patch_bool`]/
/// [`patch_nullable_text`] already take — `"preview"` from [`preview_rules`],
/// `"save"` from [`save_rules`] — naming which route's own wording the
/// missing-array message carries.
fn read_proposed_rules(body: &serde_json::Value, noun: &str) -> Result<Vec<rules::Rule>, NoAnswer> {
    let Some(raw_rules) = body.get("rules").and_then(serde_json::Value::as_array) else {
        return Err(NoAnswer::Unusable(format!(
            "the {noun} body must carry a `rules` array"
        )));
    };
    let proposed: Vec<rules::Rule> = raw_rules
        .iter()
        .map(rule_from_json)
        .collect::<Result<_, _>>()
        .map_err(NoAnswer::Unusable)?;
    if proposed.len() > MAX_PROPOSED_RULES {
        return Err(NoAnswer::Unusable(format!(
            "the proposed list holds {} rules, over this route's {MAX_PROPOSED_RULES}-rule limit",
            proposed.len()
        )));
    }
    Ok(proposed)
}

/// A new rule, as [`create_rule`]'s body is read. The counterpart of
/// [`rule_from_json`], minus `id` — `rules::create` mints one — and with
/// `ordinal` itself optional: [`taiyaku_library::rules::create`]'s own doc
/// comment is what "one past the current maximum" means when it is left out.
fn new_rule_from_json(body: &serde_json::Value) -> Result<rules::NewRule, String> {
    if !body.is_object() {
        return Err("a new rule must be a JSON object".to_string());
    }
    Ok(rules::NewRule {
        ordinal: optional_i64(body, "ordinal")?,
        enabled: required_bool(body, "enabled")?,
        root_id: optional_i64(body, "root_id")?,
        kind: required_str(body, "kind", "rule")?.parse()?,
        pattern: required_str(body, "pattern", "rule")?.to_string(),
        match_on: required_str(body, "match_on", "rule")?.parse()?,
        set_edition_key: optional_text(body, "set_edition_key")?,
        set_kind: optional_text(body, "set_kind")?,
        set_language: optional_text(body, "set_language")?,
        set_series: optional_text(body, "set_series")?,
        note: optional_text(body, "note")?,
    })
}

/// A field left alone if `patch` names nothing for it — the ordinary
/// two-state reading for `RulePatch`'s five `NOT NULL` columns, which the
/// schema gives a caller no way to clear.
fn patch_i64(body: &serde_json::Value, field: &str) -> Result<Option<i64>, String> {
    match body.get(field) {
        None => Ok(None),
        Some(value) => {
            Ok(Some(value.as_i64().ok_or_else(|| {
                format!("the rule's `{field}` must be a number")
            })?))
        }
    }
}

/// [`patch_i64`]'s boolean twin — `enabled` from the rules routes,
/// `keeps_documents` from [`root_patch_from_json`]. `noun` is the same
/// parameter [`required_str`] takes, and for the same reason: one
/// implementation shared by both routes rather than two copies of the same
/// three lines that could drift.
fn patch_bool(body: &serde_json::Value, field: &str, noun: &str) -> Result<Option<bool>, String> {
    match body.get(field) {
        None => Ok(None),
        Some(value) => {
            Ok(Some(value.as_bool().ok_or_else(|| {
                format!("the {noun}'s `{field}` must be a boolean")
            })?))
        }
    }
}

/// [`patch_i64`]'s string twin, for `pattern`.
fn patch_string(body: &serde_json::Value, field: &str) -> Result<Option<String>, String> {
    match body.get(field) {
        None => Ok(None),
        Some(value) => Ok(Some(
            value
                .as_str()
                .ok_or_else(|| format!("the rule's `{field}` must be a string"))?
                .to_string(),
        )),
    }
}

/// The three-way reading a nullable `RulePatch`/`RootPatch` column needs —
/// this module's own doc comment states the whole argument. Absent (outer
/// `None`) leaves the column alone; `null` (`Some(None)`) clears it;
/// anything else (`Some(Some(value))`) replaces it.
///
/// `noun` names the caller for the one message this can produce — `"rule"`
/// from [`rule_patch_from_json`]'s `root_id`, `"root"` from
/// [`root_patch_from_json`]'s `scan_interval_secs` — shared for the same
/// reason [`required_str`] takes the same parameter.
fn patch_nullable_i64(
    body: &serde_json::Value,
    field: &str,
    noun: &str,
) -> Result<Option<Option<i64>>, String> {
    match body.get(field) {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(None)),
        Some(value) => Ok(Some(Some(value.as_i64().ok_or_else(|| {
            format!("the {noun}'s `{field}` must be a number or null")
        })?))),
    }
}

/// [`patch_nullable_i64`]'s string twin — `root_id`'s five text-column
/// siblings from [`rule_patch_from_json`], and all four of
/// [`root_patch_from_json`]'s text columns. `noun` is the same shared
/// parameter, for the same reason.
fn patch_nullable_text(
    body: &serde_json::Value,
    field: &str,
    noun: &str,
) -> Result<Option<Option<String>>, String> {
    match body.get(field) {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(None)),
        Some(serde_json::Value::String(value)) => Ok(Some(Some(value.clone()))),
        Some(_) => Err(format!("the {noun}'s `{field}` must be a string or null")),
    }
}

/// A patch, as [`patch_rule`]'s body is read — this module's own doc comment
/// has the full argument for why every field below reads its key with
/// [`patch_nullable_i64`]/[`patch_nullable_text`] or the two-way
/// [`patch_i64`]/[`patch_bool`]/[`patch_string`], never the collapsing
/// [`optional_i64`]/[`optional_text`] [`rule_from_json`] and
/// [`new_rule_from_json`] use.
fn rule_patch_from_json(body: &serde_json::Value) -> Result<rules::RulePatch, String> {
    if !body.is_object() {
        return Err("a rule patch must be a JSON object".to_string());
    }
    let kind = match body.get("kind") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| "the rule's `kind` must be a string".to_string())?
                .parse::<rules::RuleKind>()?,
        ),
    };
    let match_on = match body.get("match_on") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| "the rule's `match_on` must be a string".to_string())?
                .parse::<rules::MatchOn>()?,
        ),
    };
    Ok(rules::RulePatch {
        ordinal: patch_i64(body, "ordinal")?,
        enabled: patch_bool(body, "enabled", "rule")?,
        root_id: patch_nullable_i64(body, "root_id", "rule")?,
        kind,
        pattern: patch_string(body, "pattern")?,
        match_on,
        set_edition_key: patch_nullable_text(body, "set_edition_key", "rule")?,
        set_kind: patch_nullable_text(body, "set_kind", "rule")?,
        set_language: patch_nullable_text(body, "set_language", "rule")?,
        set_series: patch_nullable_text(body, "set_series", "rule")?,
        note: patch_nullable_text(body, "note", "rule")?,
    })
}

// --- preview <-> JSON ---------------------------------------------------

/// [`taiyaku_library::rules::Field`]'s wire spelling — every field this
/// module ever names on the way out, in one place so [`row_json`] and
/// [`override_json`] cannot drift into two different words for the same
/// column.
fn field_key(field: rules::Field) -> &'static str {
    match field {
        rules::Field::Series => "series",
        rules::Field::EditionKey => "edition_key",
        rules::Field::Kind => "kind",
        rules::Field::Language => "language",
        rules::Field::Volume => "volume",
        rules::Field::Chapter => "chapter",
        rules::Field::Title => "title",
    }
}

fn impact_json(impact: &preview::Impact) -> serde_json::Value {
    serde_json::json!({
        "series_before": impact.series_before,
        "series_after": impact.series_after,
        "series_merged": impact.series_merged,
        "works_rehomed": impact.works_rehomed,
        "works_orphaned": impact.works_orphaned,
        "proposals_reopened": impact.proposals_reopened,
        "proposals_raised": impact.proposals_raised,
    })
}

fn placement_json(placement: &preview::Placement) -> serde_json::Value {
    serde_json::json!({
        "series": placement.series,
        "work": placement.work,
        "edition": placement.edition,
    })
}

/// Which rule — a position in the request's own `rules` array — or the
/// root's own default column decided one field, per
/// [`taiyaku_library::preview::Decided`]'s own doc comment on why a rule id
/// can never serve this role.
fn decided_json(decided: &preview::Decided) -> serde_json::Value {
    match decided {
        preview::Decided::Rule(index) => serde_json::json!({ "kind": "rule", "index": index }),
        preview::Decided::RootDefault => serde_json::json!({ "kind": "root_default" }),
    }
}

fn row_json(row: &preview::Row) -> serde_json::Value {
    serde_json::json!({
        "rel_path": row.rel_path,
        "root_id": row.root_id,
        "before": row.before.as_ref().map(placement_json),
        "after": row.after.as_ref().map(placement_json),
        "matched": row.matched,
        "decided_by": serde_json::Value::Object(
            row.decided_by
                .iter()
                .map(|(field, decided)| (field_key(*field).to_string(), decided_json(decided)))
                .collect(),
        ),
    })
}

/// One [`taiyaku_library::preview::FieldValue`] on the wire — a JSON string
/// for a text field, a JSON number for `volume` or `chapter`, and `null` for
/// either left unset.
///
/// **Typed rather than stringified**, matching how [`placement_json`] already
/// carries a `Placement` across: a `volume` that arrives as `"3"` is a number
/// the client would have to parse back, and one that arrives as `3` is the
/// number the store actually holds. `null` is a value here and not an absence
/// — a later rule *clearing* what an earlier rule set is exactly the case an
/// override row exists to show.
///
/// **A non-finite number would render as `null` too, and that is why it can no
/// longer get here.** JSON has no NaN or infinity and
/// `serde_json::Number::from_f64` refuses both, so an infinite `volume` would
/// be indistinguishable from an unset one and the editor would draw it as
/// *unset* — on a row whose whole job is telling two values apart. Three
/// review passes of the O95 branch each found one face of that, and
/// `rules::Resolved::set_number` now refuses a non-finite at the point it
/// enters, so neither arm below can receive one from any rule. The
/// `map_or(Null, …)` stays because `from_f64` is fallible in the type system
/// whatever the caller guarantees, and an `expect` here would turn a display
/// concern into a 500.
fn field_value_json(value: &preview::FieldValue) -> serde_json::Value {
    match value {
        preview::FieldValue::Text(text) => text.as_ref().map_or(serde_json::Value::Null, |t| {
            serde_json::Value::String(t.clone())
        }),
        preview::FieldValue::Number(number) => number
            .and_then(serde_json::Number::from_f64)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
    }
}

/// One [`taiyaku_library::preview::Override`], `first`/`wins` passed straight
/// through as the indices into the request's `rules` array they already are.
///
/// **`first_value` and `wins_value` describe `rel_path` alone**, not every one
/// of `examples` — [`taiyaku_library::preview::Override`]'s own doc comment
/// carries why, and the editor draws them beside that path rather than beside
/// the count for the same reason.
fn override_json(over: &preview::Override) -> serde_json::Value {
    serde_json::json!({
        "field": field_key(over.field),
        "rel_path": over.rel_path,
        "first": over.first,
        "wins": over.wins,
        "first_value": field_value_json(&over.first_value),
        "wins_value": field_value_json(&over.wins_value),
        "others": over.others,
        "examples": over.examples,
    })
}

// --- routes --------------------------------------------------------------

/// `GET /api/library/rules` — every rule this library holds, in the order
/// [`taiyaku_library::rules::Engine::load`] itself reads them, plus the
/// complaints raised by any row whose `kind` or `match_on` no longer parses
/// **and** by any row whose `pattern` no longer compiles — [`rules::read`]
/// only covers the first half; [`rules::Engine::compile`]'s own complaints
/// (`"rule {id}: pattern {:?} does not compile: {e}"`) are the second, added
/// here rather than left for a caller to discover a saved rule silently
/// matches nothing.
pub(crate) async fn list_rules(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let (rules, mut complaints) = rules::read(&conn).map_err(NoAnswer::Failed)?;
        let (_engine, compile_complaints) = rules::Engine::compile(&rules);
        complaints.extend(compile_complaints);
        Ok::<_, NoAnswer>(serde_json::json!({
            "rules": rules.iter().map(rule_json).collect::<Vec<_>>(),
            "complaints": complaints,
        }))
    })
    .await;

    match built {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("listing rules", &why),
    }
}

/// `POST /api/library/rules` — add one rule, and report the id
/// [`taiyaku_library::rules::create`] minted for it.
///
/// Refuses, 400, before ever opening a connection, when the rule's own
/// pattern will not compile — [`regex_complaint`] runs the same check
/// [`rules::Engine::compile`] would, so a rule that could never match is not
/// one this route accepts silently. `root_id`, if present, is checked for
/// existence inside `spawn_blocking` — [`root_exists`] — so a stale or
/// removed root answers 400 naming the root rather than surfacing
/// `rules::create`'s own `FOREIGN KEY constraint failed` text verbatim.
pub(crate) async fn create_rule(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let new_rule = match new_rule_from_json(&body) {
        Ok(new_rule) => new_rule,
        Err(why) => return NoAnswer::Unusable(why).into_response(),
    };
    if let Some(complaint) = regex_complaint(new_rule.kind, &new_rule.pattern) {
        return NoAnswer::Unusable(complaint).into_response();
    }

    let created = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        if let Some(root_id) = new_rule.root_id {
            if !root_exists(&conn, root_id).map_err(NoAnswer::Failed)? {
                return Err(NoAnswer::Unusable(format!("there is no root {root_id}")));
            }
        }
        rules::create(&conn, &new_rule).map_err(NoAnswer::Failed)
    })
    .await;

    match created {
        Ok(Ok(id)) => Json(serde_json::json!({ "id": id })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("creating a rule", &why),
    }
}

/// `PATCH /api/library/rules/{id}` — change only the columns `patch` names.
/// 404 when `id` names no rule, otherwise 204: nothing is server-derived
/// about a patch the way `POST` mints an id, so there is nothing this answer
/// needs to say beyond "done."
///
/// Two checks run before [`rules::update`] ever writes, both inside
/// `spawn_blocking` since both need a connection: [`patched_pattern_complaint`]
/// refuses, 400, a patch that would leave the *resulting* rule's pattern
/// uncompilable — whether `kind` moved to `regex`, `pattern` changed under an
/// already-`regex` rule, or both — and, when `patch.root_id` is
/// `Some(Some(id))` (a caller naming a root, not clearing one), [`root_exists`]
/// refuses a stale or removed root the same way [`create_rule`] does.
/// `Some(None)` — clearing `root_id` back to "every root" — names no root to
/// check and is always allowed.
pub(crate) async fn patch_rule(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let patch = match rule_patch_from_json(&body) {
        Ok(patch) => patch,
        Err(why) => return NoAnswer::Unusable(why).into_response(),
    };

    let updated = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        if let Some(Some(root_id)) = patch.root_id {
            if !root_exists(&conn, root_id).map_err(NoAnswer::Failed)? {
                return Err(NoAnswer::Unusable(format!("there is no root {root_id}")));
            }
        }
        if let Some(complaint) =
            patched_pattern_complaint(&conn, id, &patch).map_err(NoAnswer::Failed)?
        {
            return Err(NoAnswer::Unusable(complaint));
        }
        match rules::update(&conn, id, &patch) {
            Ok(true) => Ok(()),
            Ok(false) => Err(NoAnswer::Missing(id)),
            Err(why) => Err(NoAnswer::Failed(why)),
        }
    })
    .await;

    match updated {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("patching a rule", &why),
    }
}

/// `DELETE /api/library/rules/{id}` — remove one rule, reopening whichever
/// proposal(s) it answered (`taiyaku_library::rules::delete`'s own doc
/// comment). 404 when `id` names no rule; otherwise 200 with the ids of every
/// proposal this reopened, which an empty array covers exactly as well as a
/// hand-written rule reopening nothing at all — a 204 here would leave the
/// editor with no way to tell a caller their delete put a question back in
/// the inbox.
pub(crate) async fn delete_rule(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let deleted = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        match rules::delete(&conn, id) {
            Ok(deleted) if deleted.existed => Ok(deleted.reopened_proposals),
            Ok(_not_existed) => Err(NoAnswer::Missing(id)),
            Err(why) => Err(NoAnswer::Failed(why)),
        }
    })
    .await;

    match deleted {
        Ok(Ok(reopened)) => {
            Json(serde_json::json!({ "reopened_proposals": reopened })).into_response()
        }
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("deleting a rule", &why),
    }
}

/// `POST /api/library/rules/preview` — what replacing the rule set with the
/// whole `rules` the request carries would change, plus which pairs of them
/// disagree, found without writing anything: [`preview::preview_with_rows`]
/// rolls its own transaction back and [`preview::overrides`] opens none.
///
/// **Takes its connection from [`taiyaku_library::Library::working`], not
/// [`taiyaku_library::Library::conn`].** This module's own doc comment states
/// why in full; the short version is that a derivation on the shared
/// `Mutex<Connection>` stalls every other request behind it for the length of
/// the derivation, and `working` is what buys a second connection to the same
/// file instead — the same reasoning `preview_with_rows`'s own doc comment
/// gives for opening it inside the transaction rather than before it.
///
/// `selected`, when present, must be in range for `rules` — checked here,
/// before the connection is even opened, so an out-of-range index is this
/// caller's own mistake (400) rather than something the derivation's `Result`
/// could fold together with a genuine store fault the way `curation.rs`'s own
/// doc comment records happening to `proposals::resolves`.
///
/// **`preview::preview_with_rows`'s own `Result` is folded that way too —
/// whole-branch review, M1.** `rules::replace_all` refuses a nonzero id in
/// `proposed` that names no row this table currently holds ("no such rule
/// exists to keep"), which is exactly what a stale editor sends: a rule the
/// caller's own copy still shows, deleted by someone else since. Mapped to
/// `NoAnswer::Unusable` (400), the same admitted compromise
/// [`import_rules`]'s own doc comment states for `rules::import_json` — this
/// function has no `evaluable`-shaped split either, and answering `500` for
/// a caller's stale state is the exact fold this module otherwise refuses.
///
/// **[`check_proposed_roots`] refuses, `400`, a rule scoped to a root that no
/// longer exists — whole-branch review, M2.** Shared with [`save_rules`];
/// see that function's own call to it.
pub(crate) async fn preview_rules(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let proposed = match read_proposed_rules(&body, "preview") {
        Ok(proposed) => proposed,
        Err(no_answer) => return no_answer.into_response(),
    };
    let selected: Option<usize> = match body.get("selected") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => match value.as_u64().and_then(|n| usize::try_from(n).ok()) {
            Some(index) => Some(index),
            None => {
                return NoAnswer::Unusable(
                    "`selected` must be a whole number that fits a list index".to_string(),
                )
                .into_response();
            }
        },
    };
    if let Some(index) = selected
        && index >= proposed.len()
    {
        return NoAnswer::Unusable(format!(
            "selected index {index} is out of range for the proposed list, which holds {} rules",
            proposed.len()
        ))
        .into_response();
    }

    let preview = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(NoAnswer::Failed)?;
        check_proposed_roots(&working, &proposed)?;
        let (impact, rows) =
            preview::preview_with_rows(&working, &proposed, selected, MATCHED_FILES_LIMIT)
                .map_err(NoAnswer::Unusable)?;
        let overrides = preview::overrides(&working, &proposed).map_err(NoAnswer::Failed)?;
        Ok::<_, NoAnswer>(serde_json::json!({
            "impact": impact_json(&impact),
            "rows": rows.rows.iter().map(row_json).collect::<Vec<_>>(),
            "rows_truncated": rows.truncated,
            "overrides": overrides.iter().map(override_json).collect::<Vec<_>>(),
        }))
    })
    .await;

    match preview {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("previewing the proposed rule list", &why),
    }
}

/// `PUT /api/library/rules` — replace the whole rule set with the `rules` the
/// request carries **and derive over the library**, committed.
///
/// **The first route in this product that applies a rule.** Every other rules
/// route writes the `rule` table and returns — [`patch_rule`]'s own doc
/// comment says so in as many words — so a person editing rules through this
/// screen changed the rule table and nothing else until somebody scanned.
/// Spec C4 §1's Save button states "apply to 71 works"; this is what makes
/// that sentence true. See the build design's §4.
///
/// **`PUT`, not `POST`**, because the body is the complete desired state of a
/// collection that already has a URL, which is what `PUT` means.
/// [`import_rules`] stays what it is — a *document* import that mints fresh
/// ids by design, for a document from another library that has none of this
/// one's, and which therefore reopens every accepted proposal whose rule it
/// replaces. That is correct for a foreign document and wrong for a save,
/// which is the whole reason these are two routes.
///
/// **Symmetric with [`preview_rules`] down to the body key**, the
/// [`MAX_PROPOSED_RULES`] bound and the [`rule_from_json`] reader — the same
/// list previews and saves, so the client cannot send one shape to one and
/// another to the other. It takes no `selected`: rows are a property of which
/// rule is open in the editor, and a save has no such thing.
///
/// **On [`taiyaku_library::Library::working`], not `conn`** — this module's
/// own doc comment states why in full, and a save is the longest job here:
/// one full derivation over the whole library under a single write lock,
/// measured at a median 281 ms on 130 series and 1858 works.
///
/// **Every [`taiyaku_library::preview::apply`] failure is answered 400**, the
/// same admitted fold [`preview_rules`] states for the identical reason:
/// `rules::replace_all` refuses a nonzero id naming no row this table holds,
/// which is exactly what a stale editor sends — a rule the caller's own copy
/// still shows, deleted by someone else since — and there is no
/// `evaluable`-shaped split to tell that apart from a genuine store fault.
///
/// **[`check_proposed_roots`] refuses, `400`, a rule scoped to a root that no
/// longer exists — whole-branch review, M2.** [`create_rule`]/[`patch_rule`]
/// already made this same check for one rule; a save proposes a whole list,
/// so a stale `root_id` anywhere in it is caught here rather than reaching
/// `rules::replace_all`'s own foreign key and surfacing SQLite's raw text.
///
/// **Refuses, `409`, rather than racing a scan — whole-branch review, I4.**
/// [`crate::ServeState::library_job`] is the lock this route and a scan's own
/// derive both take before ever calling [`taiyaku_library::Library::working`]
/// — see that field's own doc comment for the interleaving it closes: a
/// save's single transaction landing between two of a scan's own, replacing
/// the rule table and re-deriving the whole library out from under a scan
/// still mid-pass under the rules that transaction just replaced. This route
/// never waits for the lock — it tries once and answers at once if a scan or
/// another save already holds it, the same shape [`start_scan`]'s own
/// registry already answers for a second scan of one root.
///
/// **Carries the whole derive report — whole-branch review, I5.** Every other
/// `derive` caller in this product reports what happened under
/// `<root>/.taiyaku/`; this route used to report nothing, so a save that
/// could not write a hand document answered `200` with a rehome count and
/// silence. `"derive"` is [`derive_report_json`] — the identical renderer
/// [`finished_scan_json`] already uses — over
/// [`taiyaku_library::preview::Applied::derive_report`].
pub(crate) async fn save_rules(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let proposed = match read_proposed_rules(&body, "save") {
        Ok(proposed) => proposed,
        Err(no_answer) => return no_answer.into_response(),
    };

    let saved = tokio::task::spawn_blocking(move || {
        // Tried, never waited for: a scan can run for as long as a real
        // filesystem walk takes, and blocking this request behind it would
        // leave a caller with no way to tell "still applying" apart from
        // "hung." See `ServeState::library_job`'s own doc comment.
        let _job = match state.library_job.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(NoAnswer::Busy(
                    "a scan or another save is currently running against this library; wait \
                     for it to finish and save again"
                        .to_string(),
                ));
            }
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        let working = state.library.working().map_err(NoAnswer::Failed)?;
        check_proposed_roots(&working, &proposed)?;
        let applied = preview::apply(&working, &proposed).map_err(NoAnswer::Unusable)?;
        Ok::<_, NoAnswer>(serde_json::json!({
            "impact": impact_json(&applied.impact),
            "rules": applied.rules.iter().map(rule_json).collect::<Vec<_>>(),
            "reopened_proposals": applied.reopened_proposals,
            "complaints": applied.complaints,
            "derive": derive_report_json(&applied.derive_report),
        }))
    })
    .await;

    match saved {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("saving the proposed rule list", &why),
    }
}

/// `POST /api/library/rules/import` — replace the whole rule set with the raw
/// JSON body, the same document [`taiyaku_library::rules::export_json`]
/// produces and the CLI's `taiyaku library rules export`/`import` pair
/// already round-trips. Answers the freshly-imported list, in the same shape
/// [`list_rules`] sends, so a client does not need a second request to see
/// what landed.
///
/// **Every failure `taiyaku_library::rules::import_json` can raise is
/// answered 400 here, store faults included — the one compromise this
/// module makes and states rather than hides.** `import_json` folds a
/// document that will not parse, a `root_path` this library does not have,
/// and an unrecognised `kind`/`match_on` together with a genuine write
/// failure into one `Result<Vec<i64>, String>`, and unlike
/// `taiyaku_library::proposals::resolves` — which `curation.rs` was able to
/// split via `proposals::evaluable`, a check that takes no connection — `rules`
/// offers no such split for an import: telling a bad document apart from a
/// broken database would mean this module either matching on
/// `import_json`'s own error text (which `taiyaku_library::proposals::Reason`'s
/// doc comment refuses to make any caller do) or reaching into
/// `rules::RuleJson`, which is private to that module by design. Given that
/// choice, 400 is the answer that is usually right — importing a hand-edited
/// or foreign-host document is the whole reason this route exists, and a
/// malformed one is the overwhelmingly likely failure — at the cost of
/// mislabelling the rare store fault the same way, which a future split in
/// `rules.rs` could fix the way `evaluable` fixed it for proposals.
///
/// **`"reopened_proposals"`, [`delete_rule`]'s own key — whole-branch review,
/// Important 3.** Replacing the whole rule set orphans `created_rule_id` on
/// every proposal an accepted, now-deleted rule had answered, exactly as
/// deleting one rule at a time does; `import_json` itself now reopens them
/// atomically, inside the same transaction that replaces the rules, and this
/// body reports the ids the same way `delete_rule` already does, rather than
/// leaving them reading `accepted` with nothing deciding them until whatever
/// scan happens to run next.
pub(crate) async fn import_rules(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    body: axum::body::Bytes,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let text = match std::str::from_utf8(&body) {
        Ok(text) => text.to_string(),
        Err(why) => {
            return NoAnswer::Unusable(format!("the import body is not valid UTF-8: {why}"))
                .into_response();
        }
    };

    let imported = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let reopened = rules::import_json(&conn, &text).map_err(NoAnswer::Unusable)?;
        let (rules, mut complaints) = rules::read(&conn).map_err(NoAnswer::Failed)?;
        // Same addition as `list_rules`, for the same reason: `rules::read`'s
        // complaints cover only an unparseable `kind`/`match_on`, never a
        // pattern that will not compile, and an import is exactly how a
        // hand-edited or foreign-host document could carry one.
        let (_engine, compile_complaints) = rules::Engine::compile(&rules);
        complaints.extend(compile_complaints);
        Ok::<_, NoAnswer>(serde_json::json!({
            "rules": rules.iter().map(rule_json).collect::<Vec<_>>(),
            "complaints": complaints,
            "reopened_proposals": reopened,
        }))
    })
    .await;

    match imported {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("importing rules", &why),
    }
}

// --- roots <-> JSON --------------------------------------------------------

/// Why a `/api/library/roots*` route could not answer.
///
/// **Not [`NoAnswer`] reused.** Every rules route above already ships that
/// type's plain-text body — task 8's own shape, and its own tests already
/// read it with `text.contains(...)`. This task's own brief asks for a
/// caller's mistake to come back "with the store's own message in an
/// `error` field" instead — a JSON body — and widening `NoAnswer` to do that
/// would rewrite the wire format of seven routes this task never touches for
/// the sake of four it does. A second, small type is cheaper than that, and
/// leaves task 8's answer alone.
#[derive(Debug)]
enum RootAnswer {
    /// `id` names no root — 404.
    Missing(i64),
    /// The caller's own mistake, decided before any store call or inside
    /// one: a body that could not be read as a path or a patch, or
    /// [`scan::AddRootError::Unusable`] — one of `add_root_checked`'s three
    /// refusals (does not exist, cannot be read, overlaps an existing root),
    /// its own message already naming which.
    Unusable(String),
    /// A store failure — 500. Includes [`scan::AddRootError::Failed`]: a
    /// fault inside `add_root_checked`'s own store calls is exactly as much
    /// a 500 here as one inside [`scan::update_root`] or
    /// [`scan::remove_root`] already is.
    Failed(String),
}

impl IntoResponse for RootAnswer {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Missing(id) => (StatusCode::NOT_FOUND, format!("there is no root {id}")),
            Self::Unusable(why) => (StatusCode::BAD_REQUEST, why),
            Self::Failed(why) => (StatusCode::INTERNAL_SERVER_ERROR, why),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// [`panicked`]'s twin for the roots routes: the one failure `spawn_blocking`
/// itself can hand back, worded through [`RootAnswer`] rather than
/// [`NoAnswer`] so the body stays JSON like every other refusal these routes
/// send.
fn root_panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    RootAnswer::Failed(format!("{what} panicked: {why}")).into_response()
}

/// One `root` row, as [`list_roots`] sends it and [`create_root`] sends the
/// row it just minted.
///
/// **`last_scan_at`/`last_scan_status` are read separately from
/// [`scan::Root`], not carried on it** — that type has neither column: Task
/// 12 is what writes them, and Task 11's own brief is explicit that adding
/// them to a `taiyaku-library` type is outside this task's file list
/// (`config_api.rs`, its own tests, `routes.rs`). Passed in here instead,
/// straight off the `root` row, so the shape a caller reads is complete
/// today — the key is always present, whether or not this particular root
/// has ever been scanned. Task 12 is what now writes both columns
/// ([`scan::scan_root`]'s own doc comment), so a root this process or an
/// earlier one has scanned carries a real timestamp and status here; one
/// that has not yet still reads `null` for both, exactly as it always did.
/// C4 §3's regime line needs the columns present, not populated.
fn root_json(
    root: &scan::Root,
    last_scan_at: Option<&str>,
    last_scan_status: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "id": root.id,
        "path": root.path.to_string_lossy(),
        "edition_key": root.edition_key,
        "default_kind": root.default_kind,
        "default_language": root.default_language,
        "exclude_globs": root.exclude_globs,
        "scan_interval_secs": root.scan_interval_secs,
        "keeps_documents": root.keeps_documents,
        "last_scan_at": last_scan_at,
        "last_scan_status": last_scan_status,
    })
}

/// [`create_root`]'s success body: the row [`scan::add_root_checked`]
/// minted, plus the watch probe it ran immediately and the interval that
/// probe implies — [`scan::Added`]'s own three fields, over the wire.
///
/// `last_scan_at`/`last_scan_status` are sent `null` unconditionally rather
/// than queried: a root this call just inserted has never been scanned, so
/// both are genuinely absent and asking the database to confirm that would
/// be a second round trip for an answer this call already knows.
fn added_json(added: &scan::Added) -> serde_json::Value {
    serde_json::json!({
        "root": root_json(&added.root, None, None),
        "watching": added.probe.watching,
        "watch_reason": added.probe.reason,
        "interval_secs": added.interval.as_secs(),
    })
}

/// A required `path` field — [`create_root`]'s only input.
fn root_path_from_json(body: &serde_json::Value) -> Result<PathBuf, String> {
    required_str(body, "path", "root").map(PathBuf::from)
}

/// A patch, as [`patch_root`]'s body is read.
///
/// [`rule_patch_from_json`]'s own doc comment carries the whole argument for
/// why the five nullable columns below read [`patch_nullable_text`]/
/// [`patch_nullable_i64`] rather than the collapsing
/// [`optional_text`]/[`optional_i64`]: [`scan::RootPatch`]'s own doc comment
/// states the identical three-way need `rules::RulePatch` has, for the
/// identical reason. `keeps_documents` is the one `NOT NULL` column and
/// takes the ordinary two-way [`patch_bool`], since the schema gives a
/// caller no way to clear it — `rules::RulePatch`'s five non-nullable
/// columns take the same two-way reading for the same reason.
///
/// **Every call below passes `"root"`.** [`patch_nullable_text`],
/// [`patch_nullable_i64`] and [`patch_bool`] are the identical functions
/// [`rule_patch_from_json`] calls with `"rule"` — this is not a second copy
/// of their validation logic, only a different noun for the message each
/// can produce, so a stray `{"keeps_documents": "yes"}` here is reported
/// against a root rather than "the rule's `keeps_documents`".
fn root_patch_from_json(body: &serde_json::Value) -> Result<scan::RootPatch, String> {
    if !body.is_object() {
        return Err("a root patch must be a JSON object".to_string());
    }
    // **Refused here rather than at the store**, so a typo is caught at save
    // rather than after a scan. `Some(None)` — clearing the column — is not
    // a pattern and is not checked. What this catches is narrow: `ignore`
    // rejects a dangling `\` and an unclosed `{` and accepts a great deal
    // that means nothing useful, which is why a stored pattern that will not
    // build is *also* handled, in `scan::Excluder::build`, rather than
    // treated as impossible.
    let exclude_globs = patch_nullable_text(body, "exclude_globs", "root")?;
    if let Some(Some(patterns)) = &exclude_globs {
        scan::check_exclude_globs(patterns)?;
    }
    Ok(scan::RootPatch {
        edition_key: patch_nullable_text(body, "edition_key", "root")?,
        default_kind: patch_nullable_text(body, "default_kind", "root")?,
        default_language: patch_nullable_text(body, "default_language", "root")?,
        exclude_globs,
        scan_interval_secs: patch_nullable_i64(body, "scan_interval_secs", "root")?,
        keeps_documents: patch_bool(body, "keeps_documents", "root")?,
    })
}

/// What one [`scan::Removal`] costs, over the wire — [`delete_root`]'s body,
/// whether `dry_run` asked for a rehearsal or the real thing.
fn removal_json(removal: &scan::Removal) -> serde_json::Value {
    serde_json::json!({
        "works": removal.works,
        "positions": removal.positions,
        "notes": removal.notes,
    })
}

// --- scan <-> JSON -------------------------------------------------------
//
// C4 §3's own discipline, stated on `ScanProgress::report` and repeated
// here: a scan names what it did not do, so every field of both reports
// below is rendered, always, including a collection that happens to be
// empty this time. Dropping an empty `Vec` to keep the body small would
// make "nothing unreadable this time" indistinguishable from "this field
// does not exist in this version of the API" — exactly the omission C4 §3
// writes rules against.

/// [`scan::ScanReport`], whole — [`scan_status`]'s `"scan"` key.
///
/// **Destructured with no `..`, deliberately.** A rendered report field went
/// missing once already — four keys deleted from this function and
/// [`derive_report_json`] and the whole suite stayed green, because nothing
/// asserted every field was present. Naming every field of the pattern below
/// turns adding one to [`scan::ScanReport`] into a compile error here rather
/// than a silent omission a test would have to be written to catch — and,
/// under this workspace's `-D warnings` clippy gate, turns *removing* one
/// from the object below into an unused-binding error, the same guarantee in
/// the other direction.
fn scan_report_json(report: &scan::ScanReport) -> serde_json::Value {
    let scan::ScanReport {
        root,
        dirs,
        files,
        added,
        updated,
        gone,
        refused,
        unreadable_dirs,
        excluded_dirs,
        excluded_files,
        bad_exclude_patterns,
        forgotten_by_exclusion,
    } = report;
    serde_json::json!({
        "root": root.to_string_lossy(),
        "dirs": dirs,
        "files": files,
        "added": added,
        "updated": updated,
        "gone": gone,
        "forgotten_by_exclusion": forgotten_by_exclusion,
        "refused": refused,
        "unreadable_dirs": unreadable_dirs,
        "excluded_dirs": excluded_dirs,
        "excluded_files": excluded_files,
        "bad_exclude_patterns": bad_exclude_patterns,
    })
}

/// [`hand::Absorbed`] — the work an orphaned document's own work folded
/// into, [`orphan_json`]'s one nested shape.
fn absorbed_json(absorbed: &hand::Absorbed) -> serde_json::Value {
    serde_json::json!({ "slug": absorbed.slug, "title": absorbed.title })
}

/// [`hand::Orphan`] — one entry of [`derive::DeriveReport::orphan_documents`].
fn orphan_json(orphan: &hand::Orphan) -> serde_json::Value {
    serde_json::json!({
        "path": orphan.path.to_string_lossy(),
        "title": orphan.title,
        "absorbed_into": orphan.absorbed_into.as_ref().map(absorbed_json),
    })
}

/// [`hand::Declined`] — one entry of
/// [`derive::DeriveReport::declined_documents`].
fn declined_json(declined: &hand::Declined) -> serde_json::Value {
    serde_json::json!({
        "path": declined.path.to_string_lossy(),
        "title": declined.title,
    })
}

/// [`derive::Unreadable`] — why derivation made nothing of an entry, one key
/// per cause (O90), [`derive_report_json`]'s `"unreadable"`.
///
/// **Destructured with no `..`**, for [`scan_report_json`]'s reason, one level
/// down: a cause added to the struct is a compile error here rather than a
/// key that never reaches the screen.
///
/// **`total` and `refused_format` are computed here, not stored.** They are
/// the struct's own methods, and sending them means the client draws the
/// server's sums rather than writing a second implementation of each.
fn unreadable_json(unreadable: &derive::Unreadable) -> serde_json::Value {
    let derive::Unreadable {
        by_format,
        uncounted,
        no_pages,
        vanished,
    } = unreadable;
    serde_json::json!({
        "total": unreadable.total(),
        "refused_format": unreadable.refused_format(),
        "by_format": by_format,
        "uncounted": uncounted,
        "vanished": vanished,
        "no_pages": no_pages,
    })
}

/// [`derive::DeriveReport`], whole, field for field — [`scan_status`]'s
/// `"derive"` key. See this section's own header comment for why every
/// field is here, including the Vecs, which can be empty.
///
/// **Destructured with no `..`** — [`scan_report_json`]'s own doc comment
/// states the whole argument, and this is the renderer that argument was
/// actually written for: [`derive::DeriveReport`] is the struct four of the
/// deleted keys came from.
fn derive_report_json(report: &derive::DeriveReport) -> serde_json::Value {
    let derive::DeriveReport {
        series,
        works,
        editions,
        members,
        unattached_chapter_dirs,
        unreadable,
        suppressed,
        rekeyed,
        kept_editions,
        emptied_works,
        unfolded_works,
        unfoldable,
        unreadable_hand,
        repaired_documents,
        unwritable_documents,
        orphan_documents,
        declined_documents,
        stale_documents,
        moved_positions,
        lost_positions,
        moved_notes,
        dropped_notes,
        moved_truth_pages,
        moved_truth_collisions,
        discarded_rows,
        uncarried_alignments,
        series_holding_no_work,
        merged,
        unrecognised_kinds,
        raised,
        root_removed_mid_derive,
    } = report;
    serde_json::json!({
        "series": series,
        "works": works,
        "editions": editions,
        "members": members,
        "unattached_chapter_dirs": unattached_chapter_dirs,
        "unreadable": unreadable_json(unreadable),
        "suppressed": suppressed,
        "rekeyed": rekeyed,
        "kept_editions": kept_editions,
        "emptied_works": emptied_works,
        "unfolded_works": unfolded_works,
        "unfoldable": unfoldable,
        "unreadable_hand": unreadable_hand,
        "repaired_documents": repaired_documents,
        "unwritable_documents": unwritable_documents,
        "orphan_documents": orphan_documents.iter().map(orphan_json).collect::<Vec<_>>(),
        "declined_documents": declined_documents.iter().map(declined_json).collect::<Vec<_>>(),
        "stale_documents": stale_documents,
        "moved_positions": moved_positions,
        "lost_positions": lost_positions,
        "moved_notes": moved_notes,
        "dropped_notes": dropped_notes,
        "moved_truth_pages": moved_truth_pages,
        "moved_truth_collisions": moved_truth_collisions,
        "discarded_rows": discarded_rows,
        "uncarried_alignments": uncarried_alignments,
        "series_holding_no_work": series_holding_no_work,
        "merged": merged,
        "unrecognised_kinds": unrecognised_kinds,
        "raised": raised,
        "root_removed_mid_derive": root_removed_mid_derive,
    })
}

/// What one root's finished scan produced — [`run_scan`]'s success body, and
/// [`ScanProgress::report`]'s shape once `done` is true.
///
/// **Two nested objects, `"scan"` and `"derive"`, not one flat merge of both
/// structs' fields.** The two reports are two different passes over the
/// library — [`scan::scan_root`]'s walk, then [`fn@derive::derive`]'s
/// structure-building read of what the walk recorded — and nothing here
/// requires them to share a namespace: keeping them apart is what lets a
/// client render "what the walk found" and "what derivation made of it" as
/// the two distinct sections C4 §3's screen actually needs, rather than one
/// caller having to know which of thirty-odd keys came from which pass.
///
/// **`note` — the one thing this body says about what it is not.** C4 §3's
/// screen reads this report as proof the library is current; the one place
/// that is not automatically true is `taiyaku-serve`'s own in-memory
/// `ServeState.works` (see that field's own doc comment), which every
/// `/api/library/*` route bypasses by reading the database directly, but
/// which the plain `/api/works` list still reads. Stated here rather than
/// left for a caller to discover that one list disagrees with a
/// `last_scan_at` this same response just wrote.
fn finished_scan_json(
    scan_report: &scan::ScanReport,
    derive_report: &derive::DeriveReport,
) -> serde_json::Value {
    serde_json::json!({
        "scan": scan_report_json(scan_report),
        "derive": derive_report_json(derive_report),
        "note": "this process's own /api/works list is a startup snapshot; \
                  it is not refreshed by this scan, though every \
                  /api/library/* route is current, since each reads the \
                  database directly rather than that in-memory list",
    })
}

/// Run one root's scan and derive on a connection of its own — the whole job
/// [`start_scan`] hands to [`tokio::task::spawn_blocking`], never awaited by
/// the request that started it.
///
/// **`state.library.working()`, not `state.library.conn()`.** This module's
/// own doc comment states the reasoning [`preview_rules`] and [`create_root`]
/// already act on for a shorter wait; a full scan is the longest-running
/// thing this server ever does to a library, and holding the shared
/// `Mutex<Connection>` for its whole length would stall every other request
/// this server is serving — reads and writes alike — for as long as the walk
/// takes. `working` opens a genuine second connection where the library is
/// file-backed, so an ordinary request proceeds on the shared connection the
/// whole time this runs.
///
/// # Errors
///
/// Returns an error — the message [`run_scan`] renders into the registry's
/// `report.error` rather than propagating — if opening the second connection,
/// [`scan::scan_root`], or [`fn@derive::derive`] fails.
///
/// **A derive failure also corrects `last_scan_status` before returning.**
/// [`scan::scan_root`] already wrote `"ok"` the moment its own walk
/// committed, before this function's own call to [`fn@derive::derive`] even
/// started — so without this, a root whose walk succeeded but whose derive
/// then failed kept reading `"ok"`: the one column C4 §3's regime line trusts
/// to say a scan is trustworthy, saying so about a library that is now only
/// partly derived (whole-branch review, Important 1). See
/// [`scan::mark_derive_failed`] for why the correction lives in
/// `taiyaku-library` rather than here.
///
/// **Takes [`crate::ServeState::library_job`] for its whole length, walk and
/// derive both — whole-branch review, I4.** See that field's own doc comment
/// for the interleaving this closes: without it, a save's own transaction
/// could replace the rule table and re-derive the whole library while this
/// function's own `derive::derive` was still mid-pass under an `Engine`
/// compiled from the rules that transaction just replaced. Unlike
/// [`save_rules`]'s own `try_lock`, this blocks: a scan already runs
/// unwatched in the background, so there is nobody here to answer a refusal
/// to, and [`scan_status`] is where a caller learns how long the wait plus
/// the scan together took either way.
fn run_scan_inner(
    library: &taiyaku_library::Library,
    root: &scan::Root,
    job: &std::sync::Mutex<()>,
) -> Result<serde_json::Value, String> {
    let _job = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let working = library.working()?;
    let scan_report = scan::scan_root(&working, root)?;
    let derive_report = match derive::derive(&working) {
        Ok(report) => report,
        Err(why) => {
            let _ = scan::mark_derive_failed(&working, root.id);
            return Err(format!(
                "root {} ({}) scanned, but deriving its structure afterward failed: {why}",
                root.id,
                root.path.display()
            ));
        }
    };
    Ok(finished_scan_json(&scan_report, &derive_report))
}

/// [`run_scan_inner`], with a mid-scan panic caught rather than left to take
/// the registry entry down with it.
///
/// **Why this matters here and not at every other `spawn_blocking` call in
/// this module.** Every other one is `.await`ed by the request that spawned
/// it, so a panic surfaces as a `JoinError` that request can still answer —
/// [`panicked`], [`root_panicked`]. This one is not: [`start_scan`] answers
/// `202` and returns before this closure even starts running, so nothing is
/// left holding a `JoinHandle` to notice a panic if one happened. Without
/// this, a scan that panicked partway — a bug in `sniff`, an unexpected
/// filesystem error this module's own callers have not yet learned to
/// tolerate — would leave its registry entry at `done: false` forever: no
/// later request would ever flip it, `GET` would report "still running" to
/// every caller until the process restarts, and the 409 refusal above would
/// then refuse every future scan of that root for the rest of this run.
/// [`std::panic::catch_unwind`] is what keeps a single bad scan from taking
/// that root's scan slot down with it — the caught payload is rendered as
/// this report's `error`, `done` is still set `true`, and the root can be
/// scanned again on the next `POST`.
fn run_scan(
    library: &taiyaku_library::Library,
    root: &scan::Root,
    job: &std::sync::Mutex<()>,
) -> serde_json::Value {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_scan_inner(library, root, job)
    }));
    match outcome {
        Ok(Ok(report)) => report,
        Ok(Err(why)) => serde_json::json!({ "error": why }),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| {
                    "the scan panicked with a payload that names no message".to_string()
                });
            serde_json::json!({ "error": format!("the scan panicked: {message}") })
        }
    }
}

/// The one query parameter [`start_scan`] recognises by name without
/// implementing — see that function's own doc comment on `?full=` for why a
/// caller who sends it gets a `400` rather than axum silently dropping a key
/// this struct never named.
///
/// **`deny_unknown_fields` too, not only the explicit `full` check below —
/// whole-branch review, Critical 2's own sibling audit.** The explicit check
/// on `start_scan` catches exactly `?full=`, spelled exactly that way; a typo
/// of that key, or any other key nobody has named yet, would otherwise be
/// dropped by `serde_urlencoded` the same silent way `DeleteRootQuery`'s own
/// `dry_run` was — see that struct's own doc comment for the shape this
/// closes.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScanQuery {
    #[serde(default)]
    full: Option<String>,
}

/// `POST /api/library/roots/{id}/scan` — start a scan of root `id` in the
/// background and answer immediately, per C4 §3.
///
/// **Refuses a second start against the same root, `409`, rather than
/// queuing it.** [`ServeState::scans`]'s own doc comment states the whole
/// registry's shape; the check and the reservation that follows it happen
/// inside one `Mutex::lock`, so two `POST`s racing for the same root cannot
/// both observe "nothing running" and both start a walk of the same tree.
/// Starting a *different* root's scan while this one runs is unaffected —
/// the registry is keyed by root id, and each root's slot is independent.
///
/// **`202 Accepted`, with the status URL, not the report.** The scan has not
/// finished — usually has barely started — by the time this answers;
/// [`scan_status`] is where a caller polls for progress and, eventually, the
/// finished report.
///
/// `id` is resolved to a [`scan::Root`] before the registry is even touched,
/// so a caller naming a root this library does not hold answers `404`
/// without reserving — and never releasing — a registry slot for an id that
/// will never be scanned.
///
/// **`?full=` is refused, `400`, rather than silently accepted and
/// ignored.** C4 §3's own API table lists it against this route, and
/// nothing in `taiyaku_library::scan::scan_root` distinguishes a full scan
/// from any other — every scan it runs already walks the whole tree. Before
/// [`ScanQuery`] existed, `axum` simply dropped an unrecognised query key,
/// so `POST .../scan?full=true` answered `202` and ran the identical scan a
/// plain `POST` would have — accepted and quietly ignored, precisely the
/// shape C4 §3 writes rules against for a *scan report's* own fields, and no
/// better for a request parameter deciding what kind of scan runs. Refusing
/// it by name costs one field and a check; implementing an incremental scan
/// is a real feature this task's brief does not ask for.
pub(crate) async fn start_scan(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Query(query): Query<ScanQuery>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    if query.full.is_some() {
        return RootAnswer::Unusable(
            "?full= is not implemented — every scan already walks the whole tree; omit it"
                .to_string(),
        )
        .into_response();
    }

    let lookup_state = Arc::clone(&state);
    let found = tokio::task::spawn_blocking(move || {
        let conn = lookup_state.library.conn();
        scan::roots(&conn).map_err(RootAnswer::Failed)
    })
    .await;
    let root = match found {
        Ok(Ok(roots)) => match roots.into_iter().find(|r| r.id == id) {
            Some(root) => root,
            None => return RootAnswer::Missing(id).into_response(),
        },
        Ok(Err(no_answer)) => return no_answer.into_response(),
        Err(why) => return root_panicked("looking up a root to scan", &why),
    };

    let started = Instant::now();
    {
        let mut scans = state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if scans.get(&id).is_some_and(|progress| !progress.done) {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": format!(
                        "a scan of root {id} ({}) is already running",
                        root.path.display()
                    ),
                })),
            )
                .into_response();
        }
        scans.insert(
            id,
            crate::ScanProgress {
                started,
                done: false,
                report: None,
            },
        );
    }

    let spawn_state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let report = run_scan(&spawn_state.library, &root, &spawn_state.library_job);
        let mut scans = spawn_state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scans.insert(
            id,
            crate::ScanProgress {
                started,
                done: true,
                report: Some(report),
            },
        );
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "root_id": id,
            "status_url": format!("/api/library/roots/{id}/scan"),
        })),
    )
        .into_response()
}

/// `GET /api/library/roots/{id}/scan` — live progress while a scan of root
/// `id` runs, and its finished report once it does not.
///
/// **`404` only when `id` names no root this library holds — never merely
/// because this process has no registry entry for it.** [`ServeState::scans`]
/// is in-memory and empty after every restart, so before this function
/// looked the id up in the database, a `status_url` a `202` had just handed
/// out could read as "that root is gone" the moment the process restarted,
/// and a root this same server scanned in a previous run — with a real
/// `last_scan_at`/`last_scan_status` sitting on its row — reported 404 to the
/// very screen those columns exist to feed. [`start_scan`] never made this
/// mistake, since it always resolves `id` to a [`scan::Root`] first; this
/// route now does the identical lookup for the identical reason.
///
/// A registry entry, when one exists, always wins over the database for
/// `running`/`elapsed_secs`/`report` — it is the freshest answer this
/// process can give, including for a scan that is still running and so has
/// not yet written anything to `root.last_scan_at`. Absent one, a known root
/// answers `200` with `running: false`, `report: null` and whatever the
/// database itself remembers — which is `null`/`null` for a root that has
/// genuinely never been scanned, exactly as [`root_json`] already renders
/// that same pair for [`list_roots`].
///
/// **`last_scan_at`/`last_scan_status` are read from the database and sent
/// in *every* answer, registry entry or not — whole-branch review, Important
/// 4.** Before this, the registry branch above returned early with
/// `root_id`/`running`/`elapsed_secs`/`report` and nothing else, so C4 §3's
/// regime line — the one this route exists to feed — went blank precisely
/// while a scan it just started was running or had just finished, and the
/// two branches answered different key sets entirely for what is meant to
/// be one shape. The registry still wins for the three fields only it can
/// know; the database is the durable answer for the two only it holds, and
/// now both are read unconditionally.
///
/// **`elapsed_secs` no longer truncates to whole seconds**, alongside the
/// fix above — same route, same screen, the same review flagged both.
pub(crate) async fn scan_status(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let entry = {
        let scans = state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scans.get(&id).map(|progress| {
            (
                !progress.done,
                progress.started.elapsed(),
                progress.report.clone(),
            )
        })
    };

    let lookup_state = Arc::clone(&state);
    let found = tokio::task::spawn_blocking(move || {
        let conn = lookup_state.library.conn();
        conn.query_row(
            "SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()
        .map_err(|e| RootAnswer::Failed(format!("reading root {id}'s scan status: {e}")))
    })
    .await;

    let (last_scan_at, last_scan_status) = match found {
        Ok(Ok(Some(pair))) => pair,
        Ok(Ok(None)) => return RootAnswer::Missing(id).into_response(),
        Ok(Err(no_answer)) => return no_answer.into_response(),
        Err(why) => return root_panicked("reading a root's scan status", &why),
    };

    match entry {
        Some((running, elapsed, report)) => Json(serde_json::json!({
            "root_id": id,
            "running": running,
            "elapsed_secs": elapsed.as_secs_f64(),
            "report": report,
            "last_scan_at": last_scan_at,
            "last_scan_status": last_scan_status,
        }))
        .into_response(),
        None => Json(serde_json::json!({
            "root_id": id,
            "running": false,
            "report": null,
            "last_scan_at": last_scan_at,
            "last_scan_status": last_scan_status,
        }))
        .into_response(),
    }
}

// --- roots: routes -----------------------------------------------------

/// `GET /api/library/roots` — every scan root this library holds, oldest
/// first ([`scan::roots`]'s own order).
pub(crate) async fn list_roots(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let roots = scan::roots(&conn).map_err(RootAnswer::Failed)?;
        // `scan::Root` carries neither column — see `root_json`'s own doc
        // comment — so they are read here, directly, in one extra query
        // rather than one per root.
        let mut stmt = conn
            .prepare("SELECT id, last_scan_at, last_scan_status FROM root")
            .map_err(|e| RootAnswer::Failed(format!("reading scan status: {e}")))?;
        let meta: std::collections::HashMap<i64, (Option<String>, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?))))
            .map_err(|e| RootAnswer::Failed(format!("reading scan status: {e}")))?
            .collect::<Result<_, _>>()
            .map_err(|e| RootAnswer::Failed(format!("reading scan status: {e}")))?;
        Ok::<_, RootAnswer>(serde_json::json!({
            "roots": roots
                .iter()
                .map(|root| {
                    let (at, status) = meta.get(&root.id).cloned().unwrap_or_default();
                    root_json(root, at.as_deref(), status.as_deref())
                })
                .collect::<Vec<_>>(),
        }))
    })
    .await;

    match built {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => root_panicked("listing roots", &why),
    }
}

/// `POST /api/library/roots` — add a root, running C4 §2's three
/// validations [`scan::add_root_checked`] carries: it must exist, be
/// readable by this process, and not overlap a root already held, in either
/// direction. Every one of those is this caller's own mistake — 400, naming
/// what it was, `add_root_checked`'s own message.
///
/// **A store fault along the way is a 500, not folded into that same 400.**
/// `add_root_checked` returns [`scan::AddRootError`], not a bare `String`,
/// precisely so this handler can tell its two shapes apart:
/// [`scan::AddRootError::Unusable`] is one of the three validations above,
/// [`scan::AddRootError::Failed`] is the overlap check's own read, the
/// insert, or the write that records the watch probe's interval failing for
/// a reason that has nothing to do with what the caller sent — a transient
/// SQL fault must not tell a caller they made a mistake they did not make.
///
/// **Takes its connection from [`taiyaku_library::Library::working`], not
/// [`taiyaku_library::Library::conn`].** `add_root_checked` runs its watch
/// probe synchronously before answering — up to two seconds waiting for a
/// filesystem event, per that function's own doc comment — and holds
/// whatever connection it was handed for the whole of that wait. Two
/// seconds on the shared `Mutex<Connection>` would stall every other request
/// this server is serving for longer than [`preview_rules`]'s own measured
/// 281 ms derivation, which is the entire reason that route takes this same
/// second connection instead — this module's own doc comment states it in
/// full. The identical trade, for a longer wait.
pub(crate) async fn create_root(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let path = match root_path_from_json(&body) {
        Ok(path) => path,
        Err(why) => return RootAnswer::Unusable(why).into_response(),
    };

    let created = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(RootAnswer::Failed)?;
        scan::add_root_checked(&working, &path).map_err(|e| match e {
            scan::AddRootError::Unusable(why) => RootAnswer::Unusable(why),
            scan::AddRootError::Failed(why) => RootAnswer::Failed(why),
        })
    })
    .await;

    match created {
        Ok(Ok(added)) => Json(added_json(&added)).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => root_panicked("adding a root", &why),
    }
}

/// `PATCH /api/library/roots/{id}` — change only the columns `patch` names.
/// 404 when `id` names no root, otherwise 204 — [`patch_rule`]'s own shape:
/// nothing server-derived about a patch, so there is nothing to say beyond
/// "done."
pub(crate) async fn patch_root(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let patch = match root_patch_from_json(&body) {
        Ok(patch) => patch,
        Err(why) => return RootAnswer::Unusable(why).into_response(),
    };

    let updated = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        match scan::update_root(&conn, id, &patch) {
            Ok(true) => Ok(()),
            Ok(false) => Err(RootAnswer::Missing(id)),
            Err(why) => Err(RootAnswer::Failed(why)),
        }
    })
    .await;

    match updated {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => root_panicked("patching a root", &why),
    }
}

/// The one query parameter [`delete_root`] reads — see that function's own
/// doc comment for what it changes.
///
/// **`deny_unknown_fields` — whole-branch review, Critical 2.** Without it,
/// `serde_urlencoded` drops any key this struct does not name rather than
/// refusing the request, so `dry_run` defaulting to `false` meant
/// `?dryrun=true` or `?dryRun=true` — either one key off from the one this
/// struct actually reads — answered `200 OK` and ran the real, committed
/// removal: works, positions and notes gone, on the one route C4 §2 calls
/// "the most destructive action in the product." [`ScanQuery`], the other
/// query-parameter struct this branch adds, gets the same attribute for the
/// same reason, on a route that is merely wrong when misspelled rather than
/// destructive.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteRootQuery {
    #[serde(default)]
    dry_run: bool,
}

/// `DELETE /api/library/roots/{id}` — remove a root, every work that lived
/// entirely under it, and re-derive; or, with `?dry_run=true`, find out what
/// that would cost without doing any of it.
///
/// **C4 §2 calls this "the most destructive action in the product, and it
/// does not look like it," and requires the counts stated before it acts.**
/// A plain `DELETE` cannot show a caller anything before removing —
/// [`scan::remove_root`] commits — so this route answers the identical body
/// shape (`{"removed": {…}, "dry_run": …}`) for two different requests: a
/// `?dry_run=true` call runs [`scan::removal_cost`], which rolls its own
/// transaction back rather than committing, so a client can render "this
/// removes 412 works, 3 positions and 27 notes" and let a person decide
/// before ever sending the second request that actually removes anything.
///
/// This is one of the three shapes the task brief offered — "a `GET` that
/// costs, a `?dry_run=`, or a two-step confirm" — chosen over a fifth route
/// because this task's own interface list caps it at four, and over a
/// server-held confirmation token because nothing else in this server keeps
/// that kind of short-lived state between requests; a query parameter on the
/// same route needs neither.
///
/// **The counts a real removal answers with are [`scan::remove_root`]'s own
/// return value, never a second count taken after the fact.** `apply_removal`
/// (`scan.rs`) counts before it deletes, inside the one transaction that
/// also does the deleting, so `remove_root`'s `Removal` already *is* "the
/// counts from before anything was removed" — recomputing them afterward,
/// once the rows are gone, would answer zero for exactly the numbers this
/// route exists to state honestly.
///
/// **`id` is checked for existence before either store call**, and
/// [`root_exists`]'s own doc comment says why: neither `removal_cost` nor
/// `remove_root` reports "no such root" on its own.
///
/// Takes its connection from `working`, not `conn`, for the identical reason
/// [`create_root`]'s own doc comment gives — both `removal_cost` and
/// `remove_root` derive.
///
/// **The body also carries a `"derive"` key, [`derive_report_json`] over
/// whichever [`crate::derive::DeriveReport`] `removal_cost`/`remove_root`
/// returned alongside their `Removal` — whole-branch review, M5.** Before
/// this, `scan::apply_removal` discarded that report outright, so nothing a
/// removal's own re-derive found — an unreadable hand document among it —
/// ever reached a caller. The shape matches [`finished_scan_json`]'s own
/// `"scan"`/`"derive"` pair rather than inventing a second one.
pub(crate) async fn delete_root(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Query(query): Query<DeleteRootQuery>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let dry_run = query.dry_run;

    let scans_state = Arc::clone(&state);
    let removed = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(RootAnswer::Failed)?;
        if !root_exists(&working, id).map_err(RootAnswer::Failed)? {
            return Err(RootAnswer::Missing(id));
        }
        if dry_run {
            scan::removal_cost(&working, id).map_err(RootAnswer::Failed)
        } else {
            scan::remove_root(&working, id).map_err(RootAnswer::Failed)
        }
    })
    .await;

    // MINOR 6 — `root.id` is `INTEGER PRIMARY KEY` with no `AUTOINCREMENT`,
    // so SQLite reissues a freed id to the next `add_root`. A stale
    // `state.scans` entry left under that id would then answer for a root it
    // was never scanning — `scan_status` returning the *deleted* root's old
    // report, or `start_scan` refusing the new root as "already running" —
    // so a real (non-`dry_run`) removal that actually committed clears the
    // slot here. A dry run or a failed removal leaves nothing to invalidate.
    if !dry_run && matches!(&removed, Ok(Ok(_))) {
        scans_state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
    }

    match removed {
        Ok(Ok((removal, derived))) => Json(serde_json::json!({
            "removed": removal_json(&removal),
            "derive": derive_report_json(&derived),
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => root_panicked("removing a root", &why),
    }
}

#[cfg(test)]
mod tests;
