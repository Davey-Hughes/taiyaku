//! The `/api/library/{works,series,editions,members}/{id}` PATCH handlers.
//! Spec C3 §5.
//!
//! Thin, like `curation.rs`: parse the body, resolve the URL's id or slug to
//! the work (or, for a series, to every work under it) whose document the
//! edited field lives on, apply the edit, and answer with what the store
//! actually holds afterwards.
//!
//! **This is what closes O44.** What the four routes here add is a caller a
//! person can invoke on purpose, which turns "apply a hand document" into
//! something a request can ask for, rather than something `migrate` or `derive`
//! only ever do to a work implicitly, once, on their own schedule.
//!
//! Every one of them does the same three things in the same order — apply,
//! save, re-project — but they do not all share one function for it.
//! [`patch_edition`] and [`patch_member`] touch exactly one document, so they
//! go through [`edit_work`], which folds that single [`hand::Saved`] into the
//! answer directly. [`patch_work`] and [`patch_series`] can fan out to a whole
//! series' worth of documents, so they call [`edit_work_raw`] (or, for
//! `patch_series`'s own per-work loop, [`edit_series_member_raw`] — see that
//! function's own doc comment for why) themselves, once per document, and
//! fold the whole set through [`worst_case`] and [`merge_worst_case`], which
//! [`edit_work`] cannot do because it only ever sees the one `Saved` its own
//! single call produced.
//!
//! **Not in `library_api`.** That module is C1's read layer over `browse`;
//! these routes write, are admin-only, and answer about the library's
//! *organisation* rather than about what a reader may see — the identical
//! split `curation.rs`'s own doc comment draws for the proposals inbox.
//!
//! # Admin, and the status codes
//!
//! Every route here refuses a non-admin with **403** — `curation.rs`'s own
//! doc comment states the reasoning and it applies unchanged: the credential
//! is valid and the caller is who they say, so a 401 would be the wrong answer.
//!
//! An id or slug naming nothing this route can edit is **404** —
//! [`NoAnswer::Missing`].
//!
//! Everything [`taiyaku_library::librarydoc::of`],
//! [`taiyaku_library::librarydoc::apply`] and
//! [`taiyaku_library::hand::save`] can fail or refuse with is folded into one
//! [`NoAnswer::Refused`], answered **409**, because none of the three
//! separates a refusal a caller could act on (a series edit naming external
//! ids with no titles to attach them to, say) from a genuine store fault
//! raised while the write was underway, and telling them apart here would
//! mean matching on a `String` none of the three promises to keep stable.
//!
//! A `spawn_blocking` join failure is **500**, naming what was running —
//! [`NoAnswer::Failed`], the one variant here a caller's request can never
//! produce.
//!
//! # `patch_series` touches more than one work
//!
//! `series_title` and `external_id` are shared by every work in a series, and
//! every one of those works' documents carries the series block, so
//! [`patch_series`] resolves the whole membership and re-saves each work's
//! own document in turn: saving only the work named in the URL would leave
//! every sibling work's document naming the old titles, and the first rescan
//! to read one of them back would undo the edit.
//!
//! **Titles are written once, directly onto the id the URL named — never
//! through [`librarydoc::apply_series`]'s resolve-by-title matching, and the
//! per-work loop below does not reach that matching either.**
//! [`librarydoc::set_series_titles`]'s own doc comment has the reason
//! matching exists at all: `patch_work`'s own `series` field and import, where
//! the caller has a title and not yet an id to attach it to. A PATCH to
//! `/series/{id}` already has the id, and matching its edited text anyway can
//! resolve onto a *different* series than the one the URL named.
//! [`set_series_titles`] refuses outright a title text another series already
//! holds — an error, not a write — which is what closes the door the collision
//! needs to exist at all; **and** the loop below applies each work's document
//! through [`librarydoc::apply_with_series_id`], not [`librarydoc::apply`],
//! which does not call [`librarydoc::apply_series`] — so even should the
//! ambiguous state ever exist regardless (stale data predating the refusal,
//! say), this loop cannot re-resolve a work onto the wrong series by matching
//! its title text. `reading`, `tags` and `external` flow through
//! [`apply_series_edit`] — its own doc comment has the detail — since none of
//! them can mint or orphan a series row the way matching a title can.
//!
//! **[`patch_work`]'s own `series` field is a second door onto the same
//! rows** — [`WorkEdit`]'s own doc comment has the detail — so it fans out to
//! the same siblings `patch_series` would, in both the series a work now
//! belongs to and the one it left. Both routes fold every document's
//! [`hand::Saved`] through [`worst_case`] rather than answering only about
//! the one document named in the URL.
//!
//! # `chapter_source` is forced to `Hand`
//!
//! C3 §5: a hand-set chapter number is a different kind of claim from a
//! parsed one — not because the scanner is unable to read one from a
//! filename (`builtin-chapter` is a seeded rule and does exactly that), but
//! because a hand value is a person's own answer. `taiyaku_library::derive`'s
//! `SCANNER_CHAPTER_SOURCE` names the *opposite* of that: `'filename'` is the
//! value `derive::write_chapters` clears and rewrites from the filename; `'hand'`
//! and `'comicinfo'` are the ones a re-derive is scoped never to overwrite —
//! `write_chapters`'s own doc comment states which values are protected and
//! why. A client that could send `chapter_source: "comicinfo"` alongside a
//! number a person typed could launder the one into the other, so
//! [`MemberEdit`] carries no such field at all.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path as AxPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::OptionalExtension;
use taiyaku_library::{
    account::Access,
    hand,
    librarydoc::{self, LibraryDoc},
};

use crate::{ServeState, auth::Principal};

/// Why a metadata route could not answer.
///
/// Shaped like `curation::NoAnswer`, for the reason that module's own doc
/// comment gives: every handler here does its SQLite work inside
/// `spawn_blocking`, which flattens a handler's failures into one `Result`
/// however different they were on the way in, and a `Response` already built
/// cannot be asked which of them it was.
#[derive(Debug)]
pub(crate) enum NoAnswer {
    /// The route's own slug or id names nothing this route can edit — 404.
    ///
    /// Unlike `curation::NoAnswer::Missing`, this carries no id: the four
    /// resolvers below (`work_by_slug`, `edition_owner`, `member_owner`,
    /// `works_in_series`) return it for a slug or id naming nothing *and*
    /// for one naming material the caller's own `principal.access` does not
    /// grant (I8) — a hidden work, edition, member or series answers exactly
    /// as one that does not exist, the same way `library_api::work` already
    /// answers a `GET` for either. [`patch_series`] also reaches it directly
    /// — a series with no visible member work left to answer for, which
    /// looks identical whether that is because the last one left or because
    /// every one of them is outside the grant.
    Missing,
    /// Whatever [`librarydoc::of`], [`librarydoc::apply`] or [`hand::save`]
    /// refused or failed with, folded into one `String` — see this module's
    /// own doc comment for why the two are not separated here. Answered
    /// **409**: a refusal about the state of the library, which a caller can
    /// act on.
    Refused(String),
    /// A store failure while resolving an id, which is a 500 rather than
    /// anything a caller can fix. The message is the store's own.
    Failed(String),
}

impl IntoResponse for NoAnswer {
    fn into_response(self) -> Response {
        match self {
            Self::Missing => (
                StatusCode::NOT_FOUND,
                "no such work, edition, member or series",
            )
                .into_response(),
            Self::Refused(why) => (StatusCode::CONFLICT, why).into_response(),
            Self::Failed(why) => (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
        }
    }
}

/// The one failure `spawn_blocking` itself can hand back: the closure
/// panicked and nothing answered at all. A 500 naming what was running,
/// worded the way `curation::panicked` words its own.
fn panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    NoAnswer::Failed(format!("{what} panicked: {why}")).into_response()
}
/// Apply an edit to one work's document and persist it, answering the
/// re-projected document **and** what [`hand::save`] made of it — the raw
/// halves [`edit_work`] merges for a single-document route, and
/// [`patch_series`] and [`patch_work`]'s own `series` field need apart, since
/// both of them may have more than one document's `Saved` to fold into one
/// answer.
///
/// **Three steps in this order, on every route here.** `apply` writes the
/// rows; [`taiyaku_library::hand::save`] writes the hand document
/// (`<root>/.taiyaku/<key>.library.json`) *and records the `LIBRARY` marker*,
/// bar a newer one in a root that keeps documents; re-projecting is what the
/// client is answered with, so it sees what the store holds, not what it sent.
///
/// **`apply` is a parameter, not always [`librarydoc::apply`].**
/// [`patch_series`]'s own per-work loop calls [`edit_series_member_raw`]
/// instead of this function directly, which applies through
/// [`librarydoc::apply_with_series_id`] — see that function's own doc
/// comment, and `patch_series`'s, for why re-resolving series identity by
/// title text in that loop is unsafe. Every other route here has no series
/// identity to protect and keeps the ordinary [`librarydoc::apply`],
/// through [`edit_work_raw`] below.
fn edit_work_raw_applying(
    conn: &rusqlite::Connection,
    work_id: i64,
    edit: impl FnOnce(&mut LibraryDoc),
    apply: impl FnOnce(&rusqlite::Connection, i64, &LibraryDoc) -> Result<(), String>,
) -> Result<(serde_json::Value, hand::Saved), NoAnswer> {
    let mut doc = librarydoc::of(conn, work_id).map_err(NoAnswer::Refused)?;
    edit(&mut doc);
    apply(conn, work_id, &doc).map_err(NoAnswer::Refused)?;
    let saved = hand::save(conn, work_id).map_err(NoAnswer::Refused)?;
    let doc = librarydoc::of(conn, work_id).map_err(NoAnswer::Refused)?;
    let body = serde_json::to_value(&doc).map_err(|e| NoAnswer::Refused(e.to_string()))?;
    Ok((body, saved))
}

/// [`edit_work_raw_applying`], through the ordinary [`librarydoc::apply`] —
/// what every route here except [`patch_series`]'s own per-work loop wants.
fn edit_work_raw(
    conn: &rusqlite::Connection,
    work_id: i64,
    edit: impl FnOnce(&mut LibraryDoc),
) -> Result<(serde_json::Value, hand::Saved), NoAnswer> {
    edit_work_raw_applying(conn, work_id, edit, librarydoc::apply)
}

/// [`edit_work_raw_applying`], through [`librarydoc::apply_with_series_id`]
/// rather than [`librarydoc::apply`] — [`patch_series`]'s own per-work loop,
/// once [`librarydoc::set_series_titles`] has already written the series
/// named in the URL by id. `series_id` is that same id, passed straight
/// through rather than re-read, since none of `reading`, `tags` or `external`
/// can move a work onto a different series the way a title can
/// ([`apply_series_edit`]'s own doc comment has the detail) — this loop's
/// `series_id` cannot change out from under it mid-iteration the way
/// [`patch_work`]'s own fan-out sometimes must re-read one.
///
/// [`librarydoc::apply_with_series_id`]'s own doc comment has why re-resolving
/// series identity by title text in this loop is unsafe.
fn edit_series_member_raw(
    conn: &rusqlite::Connection,
    work_id: i64,
    series_id: i64,
    edit: impl FnOnce(&mut LibraryDoc),
) -> Result<(serde_json::Value, hand::Saved), NoAnswer> {
    edit_work_raw_applying(conn, work_id, edit, |conn, work_id, doc| {
        librarydoc::apply_with_series_id(conn, work_id, Some(series_id), doc)
    })
}

/// [`edit_work_raw`], for a route that touches exactly one document —
/// [`patch_edition`] and [`patch_member`] — so `persisted`/`why` answer for
/// that one [`hand::Saved`] directly rather than through [`worst_case`]'s
/// full fold. `persisted` is reported rather than enforced: refusing the edit
/// because the document could not be written would make a library on
/// read-only media, or a work with no address at all, uneditable. The row, its
/// `_hand` guard and, bar a newer document a root keeps, the `LIBRARY` marker
/// are written either way: only durability is lost, and the client is told so.
///
/// **`why` is a JSON array of nought or one element, the same shape
/// [`merge_worst_case`] answers with for a route that can fan out to more than
/// one document.** A single-document route has at most one reason to report,
/// but it is still an array, so a client need not special-case one route's
/// `why` being a bare string against every other route's array.
///
/// **Three reasons answer `persisted: false`, and two put anything in that
/// array.** A read-only root fails the write and `hand::save` names the path
/// and the `io::Error`; a newer document is left as it is, and `hand::save`
/// says so (O264). A work imported from a sidecar — [`hand::path_for`] answers
/// `None` for it, since its document lives beside that sidecar — answers
/// `persisted: false` with an empty `why`: there is no address to have failed
/// against, so there is nothing to name. A caller sees "not durable" either
/// way, with a reason only in the first two cases, which is the exact outcome
/// `why` exists to prevent. Whether the last case should mint a reason of its
/// own — "no address: this work was not scanned" or similar — is a design
/// question this comment raises rather than answers.
fn edit_work(
    conn: &rusqlite::Connection,
    work_id: i64,
    edit: impl FnOnce(&mut LibraryDoc),
) -> Result<serde_json::Value, NoAnswer> {
    let (mut body, saved) = edit_work_raw(conn, work_id, edit)?;
    // `merge_worst_case` here too, not only in `patch_series` and
    // `patch_work`'s fan-out: one `Saved` folded through it is a `why` array
    // of at most one element, which is what keeps this route's shape the same
    // as every other route's rather than growing a second, string-shaped one.
    merge_worst_case(
        &mut body,
        saved.persisted,
        saved.why.into_iter().collect(),
        saved.keeps_documents,
    );
    Ok(body)
}

/// Fold more than one [`hand::Saved`] into the one answer a caller gets:
/// `persisted` false if **any** document did not land, `why` carrying every
/// reason that occurred — not only the last one — and `keeps_documents` of
/// `Some(false)` only if **every** save reported it, `None` otherwise.
///
/// **What [`patch_series`] and [`patch_work`]'s own `series` field both need.**
/// Keeping only a loop's last iteration would let a series whose works span two
/// roots have the first work's document fail while the answer read
/// `persisted: true` with no `why`.
///
/// **`keeps_documents` is not a plain `&=` the way `persisted` is,** because
/// its identity element differs from `persisted`'s: a fan-out across siblings
/// that span an on root and an off root is not wholly document-less, and
/// answering `Some(false)` for it would be a claim the answer cannot support
/// — so anything short of every save agreeing collapses to `None`, the same
/// "no rooted member to ask" case a single work with no root gets.
///
/// **`saves` must not be empty.** The fold's identity element for
/// `keeps_documents` is `true` — vacuously "every save (of zero) declined" —
/// so an empty `saves` would answer `Some(false)` for a request that saved
/// nothing at all, which is not a claim an empty fold can support any more
/// than a mixed one can. Nothing reaches this today: [`patch_work`] always
/// seeds `saves` with `vec![saved]` before its own fan-out loop runs, and
/// [`patch_series`] returns [`NoAnswer::Missing`] on `works.is_empty()`
/// before it ever builds a `saves` vector to fold. The `debug_assert!` below
/// is what keeps a future caller from silently inheriting the vacuous case
/// instead of one of those two guards.
///
/// # Panics
///
/// Panics if `saves` is empty, but only in a build with `debug_assertions`
/// enabled — the check is a `debug_assert!` and does not fire in release,
/// which is the profile `check-all.sh` runs the corpus checks under.
fn worst_case(saves: impl IntoIterator<Item = hand::Saved>) -> (bool, Vec<String>, Option<bool>) {
    let mut persisted = true;
    let mut why = Vec::new();
    let mut declined_everywhere = true;
    let mut saw_any = false;
    for saved in saves {
        saw_any = true;
        persisted &= saved.persisted;
        if let Some(reason) = saved.why {
            why.push(reason);
        }
        declined_everywhere &= saved.keeps_documents == Some(false);
    }
    debug_assert!(saw_any, "worst_case folded an empty `saves`");
    (persisted, why, declined_everywhere.then_some(false))
}

/// Merge a [`worst_case`] fold into a response body — [`patch_series`]'s own
/// shape, [`patch_work`]'s whenever its `series` field fanned out to more than
/// the one document named in the URL, and [`edit_work`]'s for the ordinary
/// single-document case, one `Saved` at a time.
///
/// **`why` is always a JSON array, present only when non-empty.** A route
/// that can touch more than one document answers with as many reasons as
/// occurred; collapsing that to one string would be exactly the loss
/// [`worst_case`] exists to stop. A route that can only ever touch one
/// document has at most one reason to report, but answers with the same
/// shape — an array of nought or one — rather than a second, string-shaped
/// `why` a client would have to special-case.
fn merge_worst_case(
    body: &mut serde_json::Value,
    persisted: bool,
    why: Vec<String>,
    keeps_documents: Option<bool>,
) {
    body["persisted"] = serde_json::Value::Bool(persisted);
    if !why.is_empty() {
        body["why"] =
            serde_json::Value::Array(why.into_iter().map(serde_json::Value::String).collect());
    }
    // **Emitted only when a root declined**, absent otherwise — the rule
    // `why` above already follows. `Some(true)` is the ordinary case and says
    // nothing; `None` is a work with no rooted member, which `path`'s own
    // `None` has always covered and which this field does not re-report.
    if keeps_documents == Some(false) {
        body["keeps_documents"] = serde_json::Value::Bool(false);
    }
}

/// The `AND EXISTS (...)` fragment restricting `work_alias`'s own rows to
/// ones with a member under one of `access`'s directories
/// ([`Access::grant_clause`]).
///
/// **I8.** `library_api::work` already resolves a slug through
/// `principal.access` (`taiyaku_library::browse::work`), so a work outside an
/// administrator's restricted grant is a 404 on GET. [`work_by_slug`] and
/// [`works_in_series`] reach this the same way — never `edition_owner` or
/// `member_owner`, whose own join targets are `edition`/`member` rather than
/// `work`, and which build their own, narrower clause inline.
fn root_grant_clause(work_alias: &str, access: &Access) -> String {
    access.grant_clause(|holes| {
        format!(
            "EXISTS (SELECT 1 FROM edition re JOIN member rm ON rm.edition_id = re.id \
             WHERE re.work_id = {work_alias}.id AND {})",
            taiyaku_library::browse::member_under("rm", holes)
        )
    })
}

/// The numeric `work.id` behind a slug, or [`NoAnswer::Missing`] — for a slug
/// naming nothing, and for one naming a work `access` does not grant, exactly
/// as [`taiyaku_library::browse::work`] answers `None` for both (I8).
pub(crate) fn work_by_slug(
    conn: &rusqlite::Connection,
    access: &Access,
    slug: &str,
) -> Result<i64, NoAnswer> {
    let sql = format!(
        "SELECT w.id FROM work w WHERE w.slug = ?1{}",
        root_grant_clause("w", access)
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&slug];
    params.extend(access.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
    conn.query_row(&sql, params.as_slice(), |row| row.get(0))
        .optional()
        .map_err(|e| NoAnswer::Failed(format!("looking up work `{slug}`: {e}")))?
        .ok_or(NoAnswer::Missing)
}

/// The work and edition key an `edition.id` belongs to, or
/// [`NoAnswer::Missing`] — for an id naming nothing, and for an edition with
/// no member under a granted root (I8): the question
/// [`taiyaku_library::browse::visible_members`] answers by slug and key, put
/// here in terms of the id a PATCH actually carries rather than adding a
/// second lookup to translate one into the other.
fn edition_owner(
    conn: &rusqlite::Connection,
    access: &Access,
    id: i64,
) -> Result<(i64, String), NoAnswer> {
    let clause = access.grant_clause(|holes| {
        format!(
            "EXISTS (SELECT 1 FROM member m WHERE m.edition_id = edition.id AND {})",
            taiyaku_library::browse::member_under("m", holes)
        )
    });
    let sql = format!("SELECT work_id, key FROM edition WHERE id = ?1{clause}");
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&id];
    params.extend(access.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
    conn.query_row(&sql, params.as_slice(), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
    .map_err(|e| NoAnswer::Failed(format!("looking up edition {id}: {e}")))?
    .ok_or(NoAnswer::Missing)
}

/// The work and content hash a `member.id` belongs to, or
/// [`NoAnswer::Missing`] — for an id naming nothing, and for a member none of
/// whose directories is one of `access`'s (I8). A member is under every
/// directory it has a `member_root` row for, so a copy held in two libraries
/// resolves through either grant. An imported member has no row —
/// [`browse::unfiled`]'s own doc comment states the identical rule for the
/// read side: a restricted grant sees none of them.
fn member_owner(
    conn: &rusqlite::Connection,
    access: &Access,
    id: i64,
) -> Result<(i64, String), NoAnswer> {
    let clause =
        access.grant_clause(|holes| taiyaku_library::browse::member_under("member", holes));
    let sql = format!(
        "SELECT edition.work_id, member.sha256
           FROM member JOIN edition ON edition.id = member.edition_id
          WHERE member.id = ?1{clause}"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&id];
    params.extend(access.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
    conn.query_row(&sql, params.as_slice(), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
    .map_err(|e| NoAnswer::Failed(format!("looking up member {id}: {e}")))?
    .ok_or(NoAnswer::Missing)
}

/// Every work under `series_id` that `access` grants, in a stable order —
/// empty, rather than [`NoAnswer::Missing`], for a `series_id` naming
/// nothing: [`patch_series`] is what decides that, since an empty answer
/// here is exactly what a series that once existed and lost its last work
/// also looks like, and (I8) exactly what one wholly outside a restricted
/// grant looks like too — the same 404 either way.
fn works_in_series(
    conn: &rusqlite::Connection,
    access: &Access,
    series_id: i64,
) -> Result<Vec<i64>, NoAnswer> {
    let sql = format!(
        "SELECT w.id FROM work w WHERE w.series_id = ?1{} ORDER BY w.id",
        root_grant_clause("w", access)
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| NoAnswer::Failed(format!("preparing to list series {series_id}: {e}")))?;
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&series_id];
    params.extend(access.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
    let rows = stmt
        .query_map(params.as_slice(), |row| row.get(0))
        .map_err(|e| NoAnswer::Failed(format!("listing series {series_id}: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| NoAnswer::Failed(format!("reading series {series_id}: {e}")))
}

/// [`works_in_series`], minus `excluding` — the sibling set a fan-out saves
/// alongside the work already saved directly. `None` answers empty: a work
/// with no series has no siblings to protect.
fn siblings(
    conn: &rusqlite::Connection,
    access: &Access,
    series_id: Option<i64>,
    excluding: i64,
) -> Result<Vec<i64>, NoAnswer> {
    let Some(series_id) = series_id else {
        return Ok(Vec::new());
    };
    Ok(works_in_series(conn, access, series_id)?
        .into_iter()
        .filter(|&id| id != excluding)
        .collect())
}

/// The `series_id` a work is filed under right now, or [`NoAnswer::Failed`]
/// if `work_id` names nothing — unreachable in practice, since every caller
/// has already resolved `work_id` from a real row, but SQLite's own row-count
/// answer is what [`work_by_slug`] and its neighbours already trust for the
/// same question.
fn series_of_work(conn: &rusqlite::Connection, work_id: i64) -> Result<Option<i64>, NoAnswer> {
    conn.query_row(
        "SELECT series_id FROM work WHERE id = ?1",
        [work_id],
        |row| row.get(0),
    )
    .map_err(|e| NoAnswer::Failed(format!("reading the series of work {work_id}: {e}")))
}

/// `PATCH /api/library/works/{slug}` — volume, title, series membership.
///
/// All three of C3 §5's work-level fields. Series membership is here rather
/// than deferred: see the build design §7's second correction, and
/// `derive::write_work`'s comment on the provenance columns (O80) for why a
/// rescan keeps the edit.
///
/// **A second door onto the shared rows [`patch_series`] exists to protect.**
/// `doc.series` reaches `librarydoc::apply_series`, which upserts
/// `series_title` on whichever series this work resolves onto — the same
/// shared table [`patch_series`]'s own doc comment names — and it does that
/// whether the caller is `patch_series` or this route. So when `edit.series`
/// is set, this handler fans out the same way: every sibling still under the
/// series this work now belongs to, and every sibling left under the one it
/// belonged to before the edit (the two can differ — a document naming a
/// title that resolves to a different existing series, or naming none at
/// all, moves the work rather than merely renaming its current series). A
/// pure title/volume edit with no `series` field touches none of this and
/// costs nothing extra. What an aborted fan-out leaves behind, and why a
/// retry converges, is written at the loop below.
///
/// **`why` in the answer is always a JSON array** — [`merge_worst_case`]'s own
/// doc comment has the shape — of nought or one element when `series` is
/// absent (one document, [`edit_work_raw`]'s own `Saved`) and of as many as
/// occurred when it fanned out to a series' worth of siblings.
#[derive(serde::Deserialize)]
pub(crate) struct WorkEdit {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    volume: Option<f64>,
    #[serde(default)]
    series: Option<librarydoc::SeriesInfo>,
}

pub(crate) async fn patch_work(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(slug): AxPath<String>,
    Json(edit): Json<WorkEdit>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, NoAnswer> {
        let conn = state.library.conn();
        let work_id = work_by_slug(&conn, &principal.access, &slug)?;
        let touches_series = edit.series.is_some();
        let previous_series_id = if touches_series {
            series_of_work(&conn, work_id)?
        } else {
            None
        };

        let (mut body, saved) = edit_work_raw(&conn, work_id, |doc| {
            if let Some(title) = edit.title {
                doc.title = Some(title);
            }
            if let Some(volume) = edit.volume {
                doc.volume = Some(volume);
            }
            if let Some(series) = edit.series {
                doc.series = series;
            }
        })?;

        if !touches_series {
            merge_worst_case(
                &mut body,
                saved.persisted,
                saved.why.into_iter().collect(),
                saved.keeps_documents,
            );
            return Ok(body);
        }

        // The series this work resolved onto may not be the one it was in a
        // moment ago — `apply_series` can move it onto a different existing
        // series or clear it to none. Both the destination's siblings and the
        // origin's remaining ones share rows this edit may have touched, so
        // both fan-outs run; `siblings` already excludes `work_id`, and the
        // two sets cannot overlap (a work has exactly one `series_id`).
        //
        // **What a half-finished fan-out leaves behind.** The DB's own shared
        // rows (`series_title`, `external_id`) are already correct by this
        // point: the `edit_work_raw` call above already ran the one
        // `librarydoc::apply` that upserts them, keyed by `series_id` rather
        // than by `work_id`, so every sibling's *row* already reflects the
        // edit before either loop below runs a single iteration. What the
        // loops still owe is each sibling's own *document* — `hand::save`
        // re-projects and rewrites it. A hard `Err` partway through one of
        // them — `hand::save`'s own `record_absorbed` failing, say, not a
        // soft `Saved { persisted: false, .. }` — aborts the whole request
        // through `NoAnswer::Refused` via `?`, and the answer is a 409 with no
        // `persisted`/`why` at all: the client does not learn which siblings
        // were reached. The siblings already looped over keep the document
        // this loop just gave them; the one that failed, and every sibling
        // after it in iteration order, keep the document they had before this
        // edit — stale against the row it should describe, exactly the
        // staleness this module's own doc comment says a rescan would read
        // back and undo. It does not self-heal; it heals the next time
        // something calls `hand::save` for that work, which a retry of this
        // identical request supplies: `librarydoc::apply` is idempotent (its
        // own doc comment), so re-running these loops over the current
        // membership re-saves the already-correct siblings with no change and
        // finally saves the ones an earlier attempt did not reach. A soft
        // failure does not abort either loop — every sibling is still
        // reached, and a `persisted`/`why` that does come back describes all
        // of them, never a subset.
        let new_series_id = series_of_work(&conn, work_id)?;
        let mut saves = vec![saved];
        for sibling in siblings(&conn, &principal.access, new_series_id, work_id)? {
            saves.push(hand::save(&conn, sibling).map_err(NoAnswer::Refused)?);
        }
        if previous_series_id != new_series_id {
            for sibling in siblings(&conn, &principal.access, previous_series_id, work_id)? {
                saves.push(hand::save(&conn, sibling).map_err(NoAnswer::Refused)?);
            }
        }
        let (persisted, why, keeps_documents) = worst_case(saves);
        merge_worst_case(&mut body, persisted, why, keeps_documents);
        Ok(body)
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing a work", &why),
    }
}

/// `PATCH /api/library/editions/{id}` — label, kind, language, credits.
///
/// Touches exactly one document, so `why` in the answer is [`edit_work`]'s
/// shape: a JSON array of nought or one element, never a bare string.
#[derive(serde::Deserialize)]
pub(crate) struct EditionEdit {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    kind: Option<librarydoc::EditionKind>,
    #[serde(default)]
    credits: Option<Vec<librarydoc::Credit>>,
}

pub(crate) async fn patch_edition(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(edit): Json<EditionEdit>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let (work_id, key) = edition_owner(&conn, &principal.access, id)?;
        edit_work(&conn, work_id, |doc| {
            let meta = doc.editions.entry(key).or_default();
            if let Some(label) = edit.label {
                meta.label = Some(label);
            }
            if let Some(language) = edit.language {
                meta.language = Some(language);
            }
            if let Some(kind) = edit.kind {
                meta.kind = Some(kind);
            }
            if let Some(credits) = edit.credits {
                meta.credits = credits;
            }
        })
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing an edition", &why),
    }
}

/// `PATCH /api/library/members/{id}` — a hand-set chapter number.
///
/// **`chapter` absent leaves the member's entry untouched — the same "send
/// only what changed" contract [`WorkEdit`], [`EditionEdit`] and
/// [`SeriesEdit`] hold their own fields to.** `every_metadata_route_refuses_a_reader`
/// sends `{}` to this very route, which is what makes an unconditional
/// drop-and-repush worth a test of its own: harmless as a reader, and
/// destructive as an admin, since `PATCH {}` would otherwise set
/// `chapter_declared` and `chapter_source` to NULL on every existing member.
///
/// **`chapter_source` is forced to `Hand` and is not a field of the body.**
/// C3 §5: *"a hand-set chapter number is a different kind of claim from a
/// parsed one"* — not because anything refuses to read one from a filename
/// (the scanner does exactly that, seeded and on by default), but because a
/// hand value is a person's own answer. `derive.rs`'s `SCANNER_CHAPTER_SOURCE`
/// names the filename-derived value instead — the one `write_chapters` clears
/// and rewrites from the filename — and `write_chapters`'s own doc comment is
/// where the protected values (`hand`, `comicinfo`) are named. A client that
/// could send `chapter_source: "comicinfo"` alongside a number a person typed
/// would be able to launder the one into the other, and spec G's AniList sync
/// will need to know which claim it actually has.
///
/// Touches exactly one document, so `why` in the answer is [`edit_work`]'s
/// shape: a JSON array of nought or one element, never a bare string.
///
/// **The URL addresses one member row; the write it produces can land on
/// two.** [`member_owner`] resolves `id` to a `(work_id, sha256)` pair, and
/// [`librarydoc::apply_members`] then writes `chapter_declared`/
/// `chapter_source` onto every member of `work_id` sharing that `sha256` —
/// the document's own model is content-keyed, per `librarydoc.rs`'s own doc
/// comment ("Members are keyed by `sha256`, never by path"), and this route
/// inherits that. Ordinarily harmless, since one file is one member — but
/// `import.rs`'s `held_member_meta` already documents a real case where it is
/// not: the same `sha256` can be a member of two editions of one work (a
/// repack sharing a file with a standalone release, or two roots merged with
/// no `edition_key`, O54). A PATCH naming one edition's copy of such a file
/// sets the chapter number on the sibling edition's copy too, silently.
#[derive(serde::Deserialize)]
pub(crate) struct MemberEdit {
    #[serde(default)]
    chapter: Option<f64>,
}

pub(crate) async fn patch_member(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(edit): Json<MemberEdit>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let (work_id, sha256) = member_owner(&conn, &principal.access, id)?;
        edit_work(&conn, work_id, |doc| {
            // Absent `chapter` leaves this member's entry alone — see
            // `MemberEdit`'s own doc comment. Only a present value replaces
            // it, and it always replaces the whole entry (never a partial
            // merge), since `chapter_source` is not independently settable.
            if let Some(chapter) = edit.chapter {
                doc.members.retain(|m| m.sha256 != sha256);
                doc.members.push(librarydoc::MemberMeta {
                    sha256,
                    chapter: Some(chapter),
                    chapter_source: Some(librarydoc::ChapterSource::Hand),
                });
            }
        })
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing a member", &why),
    }
}

/// `PATCH /api/library/series/{id}` body — titles and their kind, which is
/// primary, reading direction, tags, external ids.
///
/// Each field `Option`, so a client sends only what it changed — the same
/// contract [`WorkEdit`] and [`EditionEdit`] hold their own fields to, and
/// [`apply_series_edit`] is what applies exactly that subset onto the shared
/// part of the document.
#[derive(serde::Deserialize)]
pub(crate) struct SeriesEdit {
    #[serde(default)]
    titles: Option<Vec<librarydoc::SeriesTitle>>,
    #[serde(default)]
    reading: Option<taiyaku_core::Direction>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    external: Option<std::collections::BTreeMap<String, String>>,
}

/// Write every field `edit` actually set onto `doc`'s reading direction, tags
/// and external ids — the parts of a series edit that flow through
/// [`edit_series_member_raw`]'s per-work call, because none of them can mint
/// or orphan a `series` row the way matching a title can. A field `edit` left
/// absent leaves `doc`'s own value alone, the same "send only what changed"
/// contract [`SeriesEdit`]'s own doc comment states.
///
/// **`edit.titles` is not one of these fields.** [`patch_series`] writes it
/// through [`librarydoc::set_series_titles`] instead, once, directly onto the
/// id the URL named, before this function or the loop that calls it ever
/// runs — this module's own doc comment on `patch_series` has the reason.
/// `doc.series.titles` reaches this function already carrying that write:
/// `doc` is read fresh from [`librarydoc::of`] in [`edit_series_member_raw`],
/// after `set_series_titles` has run — and this function does not touch
/// `doc.series.titles` at all, so it is the only thing [`edit_series_member_raw`]'s
/// own [`librarydoc::apply_with_series_id`] call ever writes for the series
/// block: everything else in it stays exactly what `set_series_titles`
/// already put there.
fn apply_series_edit(doc: &mut LibraryDoc, edit: &SeriesEdit) {
    if let Some(reading) = edit.reading {
        doc.series.reading = Some(reading);
    }
    if let Some(tags) = &edit.tags {
        doc.tags = tags.clone();
    }
    if let Some(external) = &edit.external {
        doc.external = external.clone();
    }
}

/// `PATCH /api/library/series/{id}` — titles and their kind, which is
/// primary, reading direction, tags, external ids.
///
/// **Titles are written once, directly onto `id`, before anything else
/// here runs** — [`librarydoc::set_series_titles`], not
/// [`librarydoc::apply_series`]'s resolve-by-title matching. This module's
/// own doc comment on this section has the reason. `set_series_titles`
/// refuses outright if any of the corrected titles names text a *different*
/// series already holds — its own doc comment has the reason — so a refusal
/// here aborts the whole request via `?` before any work's document is
/// touched at all, the same as an empty `titles` array does.
///
/// **The per-work loop below never resolves series identity by title text
/// either, regardless.** It calls [`edit_series_member_raw`], not
/// [`edit_work_raw`], which applies through
/// [`librarydoc::apply_with_series_id`] rather than [`librarydoc::apply`] —
/// that function's own doc comment has the mechanism. The refusal above and
/// this loop's own `apply_with_series_id` guard against a resolution onto
/// another series two ways at once: the ambiguous state it depends on cannot be
/// written, and even were it already there regardless (stale data predating the
/// refusal), this loop could not re-resolve onto it.
///
/// **Every work in the series, not one, for `reading`, `tags` and
/// `external`.** `external_id` is shared the way `librarydoc.rs`'s own module
/// comment describes — upserted, not replaced, so one work's document cannot
/// erase a sibling's contribution — and every work's own document carries the
/// series block regardless, so this loop re-saves each one: saving only the
/// work named in the URL would leave every sibling's document naming the old
/// titles, and the first rescan to read one of them back would undo the edit.
/// What an aborted loop leaves behind, and why a retry converges, is written
/// at the loop below.
///
/// **`why` in the answer is always a JSON array** — [`merge_worst_case`]'s own
/// doc comment has the shape — carrying as many reasons as occurred across
/// every work in the series, never collapsed to the last one.
pub(crate) async fn patch_series(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(edit): Json<SeriesEdit>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let works = works_in_series(&conn, &principal.access, id)?;
        if works.is_empty() {
            return Err(NoAnswer::Missing);
        }
        // Titles first, once, atomically, and by the id already in hand —
        // see this module's own doc comment on `patch_series` and
        // `set_series_titles`'s own for why this is not folded into the
        // per-work loop below the way `reading`/`tags`/`external` are. A
        // refusal here (an empty `titles` array; SQLite failing) aborts
        // before any work's document is touched at all.
        if let Some(titles) = &edit.titles {
            librarydoc::set_series_titles(&conn, id, titles).map_err(NoAnswer::Refused)?;
        }
        // The worst case, not the last one — see `worst_case`'s own
        // doc comment.
        //
        // **A hard `Err` here has the same abort semantics `patch_work`'s own
        // fan-out documents at its loop.** It aborts the whole request through
        // `NoAnswer::Refused` (409) via `?`, with no `persisted`/`why` in the
        // answer at all. The works already looped over keep the document this
        // loop just gave them; the ones not yet reached keep their pre-edit
        // one. Titles are not part of that staleness: the write above lands
        // before the loop's first iteration, atomically, so every
        // not-yet-reached work's `series_title` row is already correct — only
        // its own JSON document file names the old text. `external_id` is the
        // one table keyed to the loop itself: it is upserted, not replaced, so
        // the first iteration's `librarydoc::apply_with_series_id` already
        // rewrites it for the whole series (kept by `series_id`, not
        // `work_id`), and a not-yet-reached work's stale document is the only
        // thing left behind for it too. It heals on a retry:
        // `librarydoc::apply_with_series_id` is idempotent (its own doc
        // comment), and `set_series_titles` overwriting the same title set
        // twice is a no-op, so re-running this whole handler over the
        // current membership re-saves the already-correct works with no
        // change and finally saves the ones an earlier attempt did not reach.
        //
        // **"The current membership" is `works_in_series`'s own answer —
        // `principal.access`-filtered (I8) — not the series' true membership:
        // the gap between those two is O86, closed by the permission model
        // rather than by anything here.**
        //
        // `set_series_titles` above writes `series_title` for the whole series
        // regardless of `principal.access`, since that table is keyed
        // `(series_id, title)` and has no per-work column a grant could filter
        // on; this loop then re-saves only the works `works_in_series` returns.
        // So an administrator who could see part of a series would rename it
        // for the whole library and leave every hidden sibling holding a stale
        // cached copy of the series block — not corrupt, the row data being
        // untouched, but never healed either, because no later request from
        // that same principal would reach those works.
        //
        // **There is no such administrator.** `auth::Principal::from_account`
        // gives an admin `Access::Every` outright — role dominates the other
        // three permission columns, which spec A2 listed side by side without
        // deciding — and these routes admit nobody else. `works_in_series`
        // therefore always answers the series' true membership here, and the
        // two cannot differ. The alternative considered and declined was
        // refusing a `titles` edit whenever a series straddled the grant, which
        // buys the same safety at the price of a rule every future shared-row
        // edit would have to remember; the entry's other suggestion, leaving
        // `series_title` partially updated, cannot be done at all, there being
        // no per-work dimension in that table to leave partial.
        //
        // **The hazard a stale document poses.** A later, independent rebuild
        // re-applies such a document — `derive::write_work`'s read-back runs
        // `librarydoc::apply` (never `apply_with_series_id`) over every work
        // with no `LIBRARY` marker, which is every work on a database rebuilt
        // from scratch — and `apply` reaches `apply_series`'s ordinary
        // resolve-by-title matching, deliberately left untouched for the import
        // path. A stale title not matching what the fresh scan derived is
        // exactly the case `apply_series` handles by minting or moving, so the
        // document is never lost, only silently re-homed.
        let mut last_doc = None;
        let mut saves = Vec::new();
        for work_id in works {
            let (doc, saved) =
                edit_series_member_raw(&conn, work_id, id, |doc| apply_series_edit(doc, &edit))?;
            saves.push(saved);
            last_doc = Some(doc);
        }
        let mut body = last_doc.expect("checked works.is_empty() above");
        let (persisted, why, keeps_documents) = worst_case(saves);
        merge_worst_case(&mut body, persisted, why, keeps_documents);
        Ok(body)
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing a series", &why),
    }
}

#[cfg(test)]
mod tests;
