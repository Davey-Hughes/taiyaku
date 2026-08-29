//! The `/api/library/{works,series,editions,members}/{id}` PATCH handlers.
//! Spec C3 §5.
//!
//! Thin, like `curation.rs`: parse the body, resolve the URL's id or slug to
//! the work (or, for a series, to every work under it) whose document the
//! edited field lives on, apply the edit, and answer with what the store
//! actually holds afterwards.
//!
//! **This is what closes O44, but not by giving [`taiyaku_library::librarydoc::apply`]
//! its first caller outside `migrate::apply_library`.** `derive::write_work`'s
//! own read-back already reached it, for a scanned work with no `LIBRARY`
//! marker yet — added earlier on this same branch (`b075be4`, closing O53's
//! unreachable arm), a few commits before this file existed at all, not
//! before the branch. `main` has no such read-back; nothing on it ever
//! applied a hand document outside `migrate::apply_library`. What the four
//! routes here add is a caller a person can invoke on purpose — the first
//! that turns "apply a hand document" into something a request can ask for,
//! rather than something `migrate` or `derive` only ever do to a work
//! implicitly, once, on their own schedule.
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
//! is valid and the caller is who they say, so a 401 would be the wrong
//! answer.
//!
//! An id or slug naming nothing this route can edit is **404** —
//! [`NoAnswer::Missing`].
//!
//! Everything [`taiyaku_library::librarydoc::of`],
//! [`taiyaku_library::librarydoc::apply`] and
//! [`taiyaku_library::hand::save`] can fail or refuse with is folded into one
//! [`NoAnswer::Refused`], answered **409** — the identical fold
//! `curation::NoAnswer::Refused`'s own doc comment describes and for the same
//! reason: none of the three separates a refusal a caller could act on (a
//! series edit naming external ids with no titles to attach them to, say)
//! from a genuine store fault raised while the write was underway, and
//! telling them apart here would mean matching on a `String` none of the
//! three promises to keep stable.
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
//! resolve onto a *different* series than the one the URL named — this route
//! used to do exactly that, three ways (whole-branch review's C1: a corrected
//! title with nothing left to match minted a new series and deleted this one;
//! a corrected title beside an unchanged one landed next to the old spelling
//! rather than replacing it; delta review of the fix waves,
//! `f133b3e..910b15a`, the Critical: a corrected title matching text some
//! *other* series already held resolved there instead, and once every work
//! had migrated onto it, deleted this one — the two doc-comment fixes above
//! only ruled out the first two, and shipped believing they had ruled out all
//! of it). [`set_series_titles`] refuses the third shape outright — a title
//! text another series already holds is an error, not a write — which is what
//! closes the door the collision needs to exist at all; **and** the loop below
//! applies each work's document through [`librarydoc::apply_with_series_id`],
//! not [`librarydoc::apply`], which does not call
//! [`librarydoc::apply_series`] — so even should the ambiguous state ever
//! exist regardless (stale data predating the refusal, say), this loop cannot
//! re-resolve a work onto the wrong series by matching its title text.
//! `reading`, `tags` and `external` still flow through
//! [`apply_series_edit`] the same as before — its own doc comment has the
//! detail — since none of them can mint or orphan a series row the way
//! matching a title can.
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
//! value `derive::write_chapters` clears and rewrites on every pass; `'hand'`
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
    hand,
    librarydoc::{self, LibraryDoc},
};

use crate::{
    ServeState,
    auth::{Principal, Role},
};

/// Why a metadata route could not answer.
///
/// Shaped like `curation::NoAnswer`, for the reason that module's own doc
/// comment gives: every handler here does its SQLite work inside
/// `spawn_blocking`, which flattens a handler's failures into one `Result`
/// however different they were on the way in, and a `Response` already built
/// cannot be asked which of them it was.
#[derive(Debug)]
enum NoAnswer {
    /// The route's own slug or id names nothing this route can edit — 404.
    ///
    /// Unlike `curation::NoAnswer::Missing`, this carries no id: the four
    /// resolvers below (`work_by_slug`, `edition_owner`, `member_owner`,
    /// `works_in_series`) return it for a slug or id naming nothing *and*
    /// for one naming material the caller's own `principal.roots` does not
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

/// The refusal every handler here opens with, or `None` for an administrator.
///
/// Shaped like `curation::admin_or_forbidden`: one place decides what this
/// answer is, so no two handlers here can drift into different refusals of
/// the same question.
fn admin_or_forbidden(principal: &Principal) -> Option<Response> {
    (principal.role != Role::Admin).then(|| {
        (
            StatusCode::FORBIDDEN,
            "editing library metadata is an administrator's",
        )
            .into_response()
    })
}

/// Apply an edit to one work's document and persist it, answering the
/// re-projected document **and** what [`hand::save`] made of it — the raw
/// halves [`edit_work`] merges for a single-document route, and
/// [`patch_series`] and [`patch_work`]'s own `series` field need apart, since
/// both of them may have more than one document's `Saved` to fold into one
/// answer.
///
/// **Three steps in this order, on every route here.** `apply` writes the
/// rows; [`taiyaku_library::hand::save`] writes
/// `<root>/.taiyaku/<key>.library.json` *and records the `LIBRARY` marker*,
/// which is what stops the next scan reverting the edit
/// (`derive::write_work`'s own `migrate::absorbed` guard); re-projecting is
/// what the client is answered with, so it sees what the store actually
/// holds rather than what it sent.
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
/// **This is half of the Critical the delta review of the fix waves
/// (`f133b3e..910b15a`) found.** [`librarydoc::apply_with_series_id`]'s own
/// doc comment has the mechanism; `patch_series`'s own doc comment on this
/// module has the end-to-end reproduction it closes.
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
/// read-only media, or a work with no address at all, uneditable. The row is
/// written and the guard is armed either way — only durability is lost, and
/// the client is told so.
///
/// **`why` is a JSON array of nought or one element, the same shape
/// [`merge_worst_case`] answers with for a route that can fan out to more than
/// one document.** A single-document route has at most one reason to report,
/// but it is still an array: a client that must special-case one route's
/// `why` being a bare string against every other route's array is exactly the
/// defect Task 8 and 9's client would otherwise have to work around.
///
/// **Two different reasons answer `persisted: false`, and only one of them
/// puts anything in that array.** A read-only root fails the write and
/// `hand::save` names the path and the `io::Error`. A work imported from a
/// sidecar — [`hand::path_for`] answers `None` for it, since its document
/// already lives beside that sidecar rather than under a root's `.taiyaku` —
/// answers `persisted: false` with an empty `why`: there is no address to
/// have failed against, so there is nothing to name. A caller sees "not
/// durable" either way, with a reason only in the first case, which is the
/// exact outcome `why` exists to prevent. Whether the second case should mint
/// a reason of its own — "no address: this work was not scanned" or similar —
/// is a design question this comment raises rather than answers.
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
    merge_worst_case(&mut body, saved.persisted, saved.why.into_iter().collect());
    Ok(body)
}

/// Fold more than one [`hand::Saved`] into the one answer a caller gets:
/// `persisted` false if **any** document did not land, `why` carrying every
/// reason that occurred — not only the last one.
///
/// **What [`patch_series`] and [`patch_work`]'s own `series` field both need,
/// and used to lack.** `patch_series` kept only its loop's last iteration, so
/// a series whose works span two roots could have the first work's document
/// fail while the answer read `persisted: true` with no `why` — the exact
/// finding Task 4's review closed once already, this time reopened by a loop
/// throwing away every result but one.
fn worst_case(saves: impl IntoIterator<Item = hand::Saved>) -> (bool, Vec<String>) {
    let mut persisted = true;
    let mut why = Vec::new();
    for saved in saves {
        persisted &= saved.persisted;
        if let Some(reason) = saved.why {
            why.push(reason);
        }
    }
    (persisted, why)
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
fn merge_worst_case(body: &mut serde_json::Value, persisted: bool, why: Vec<String>) {
    body["persisted"] = serde_json::Value::Bool(persisted);
    if !why.is_empty() {
        body["why"] =
            serde_json::Value::Array(why.into_iter().map(serde_json::Value::String).collect());
    }
}

/// The `AND EXISTS (...)` fragment restricting `work_alias`'s own rows to
/// ones with a member under one of `roots` — empty for an unrestricted
/// grant, the same "an empty grant sees everything" rule every clause in
/// `taiyaku_library::browse` holds itself to (`Principal::roots`'s own doc
/// comment states it for the account side of that same rule).
///
/// **I8.** `library_api::work` already resolves a slug through
/// `principal.roots` (`taiyaku_library::browse::work`), so a work outside an
/// administrator's restricted grant is a 404 on GET. [`work_by_slug`] and
/// [`works_in_series`] reach this the same way — never `edition_owner` or
/// `member_owner`, whose own join targets are `edition`/`member` rather than
/// `work`, and which build their own, narrower clause inline.
fn root_grant_clause(work_alias: &str, roots: &[i64]) -> String {
    if roots.is_empty() {
        return String::new();
    }
    let holes = std::iter::repeat_n("?", roots.len())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        " AND EXISTS (SELECT 1 FROM edition re JOIN member rm ON rm.edition_id = re.id \
         WHERE re.work_id = {work_alias}.id AND rm.root_id IN ({holes}))"
    )
}

/// The numeric `work.id` behind a slug, or [`NoAnswer::Missing`] — for a slug
/// naming nothing, and for one naming a work `roots` does not grant, exactly
/// as [`taiyaku_library::browse::work`] answers `None` for both (I8).
fn work_by_slug(conn: &rusqlite::Connection, roots: &[i64], slug: &str) -> Result<i64, NoAnswer> {
    let sql = format!(
        "SELECT w.id FROM work w WHERE w.slug = ?1{}",
        root_grant_clause("w", roots)
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&slug];
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    conn.query_row(&sql, params.as_slice(), |row| row.get(0))
        .optional()
        .map_err(|e| NoAnswer::Failed(format!("looking up work `{slug}`: {e}")))?
        .ok_or(NoAnswer::Missing)
}

/// The work and edition key an `edition.id` belongs to, or
/// [`NoAnswer::Missing`] — for an id naming nothing, and for an edition with
/// no member under a granted root (I8): the same question
/// [`taiyaku_library::browse::edition_visible`] answers by slug and key, put
/// here in terms of the id a PATCH actually carries rather than adding a
/// second lookup to translate one into the other.
fn edition_owner(
    conn: &rusqlite::Connection,
    roots: &[i64],
    id: i64,
) -> Result<(i64, String), NoAnswer> {
    let clause = if roots.is_empty() {
        String::new()
    } else {
        let holes = std::iter::repeat_n("?", roots.len())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            " AND EXISTS (SELECT 1 FROM member m \
             WHERE m.edition_id = edition.id AND m.root_id IN ({holes}))"
        )
    };
    let sql = format!("SELECT work_id, key FROM edition WHERE id = ?1{clause}");
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&id];
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    conn.query_row(&sql, params.as_slice(), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
    .map_err(|e| NoAnswer::Failed(format!("looking up edition {id}: {e}")))?
    .ok_or(NoAnswer::Missing)
}

/// The work and content hash a `member.id` belongs to, or
/// [`NoAnswer::Missing`] — for an id naming nothing, and for a member whose
/// own `root_id` is not one of `roots` (I8). An imported member's `root_id`
/// is NULL, and `IN (…)` excludes NULL — [`browse::unfiled`]'s own doc
/// comment states the identical rule for the read side: a restricted grant
/// sees none of them.
fn member_owner(
    conn: &rusqlite::Connection,
    roots: &[i64],
    id: i64,
) -> Result<(i64, String), NoAnswer> {
    let clause = if roots.is_empty() {
        String::new()
    } else {
        let holes = std::iter::repeat_n("?", roots.len())
            .collect::<Vec<_>>()
            .join(",");
        format!(" AND member.root_id IN ({holes})")
    };
    let sql = format!(
        "SELECT edition.work_id, member.sha256
           FROM member JOIN edition ON edition.id = member.edition_id
          WHERE member.id = ?1{clause}"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&id];
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    conn.query_row(&sql, params.as_slice(), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
    .map_err(|e| NoAnswer::Failed(format!("looking up member {id}: {e}")))?
    .ok_or(NoAnswer::Missing)
}

/// Every work under `series_id` that `roots` grants, in a stable order —
/// empty, rather than [`NoAnswer::Missing`], for a `series_id` naming
/// nothing: [`patch_series`] is what decides that, since an empty answer
/// here is exactly what a series that once existed and lost its last work
/// also looks like, and (I8) exactly what one wholly outside a restricted
/// grant looks like too — the same 404 either way.
fn works_in_series(
    conn: &rusqlite::Connection,
    roots: &[i64],
    series_id: i64,
) -> Result<Vec<i64>, NoAnswer> {
    let sql = format!(
        "SELECT w.id FROM work w WHERE w.series_id = ?1{} ORDER BY w.id",
        root_grant_clause("w", roots)
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| NoAnswer::Failed(format!("preparing to list series {series_id}: {e}")))?;
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&series_id];
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
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
    roots: &[i64],
    series_id: Option<i64>,
    excluding: i64,
) -> Result<Vec<i64>, NoAnswer> {
    let Some(series_id) = series_id else {
        return Ok(Vec::new());
    };
    Ok(works_in_series(conn, roots, series_id)?
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
/// than deferred: see the build design §7's second correction — `derive` has
/// guarded these columns behind the `LIBRARY` marker since #77, and
/// [`edit_work_raw`] records that marker.
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
    AxPath(slug): AxPath<String>,
    Json(edit): Json<WorkEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, NoAnswer> {
        let conn = state.library.conn();
        let work_id = work_by_slug(&conn, &principal.roots, &slug)?;
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
            merge_worst_case(&mut body, saved.persisted, saved.why.into_iter().collect());
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
        for sibling in siblings(&conn, &principal.roots, new_series_id, work_id)? {
            saves.push(hand::save(&conn, sibling).map_err(NoAnswer::Refused)?);
        }
        if previous_series_id != new_series_id {
            for sibling in siblings(&conn, &principal.roots, previous_series_id, work_id)? {
                saves.push(hand::save(&conn, sibling).map_err(NoAnswer::Refused)?);
            }
        }
        let (persisted, why) = worst_case(saves);
        merge_worst_case(&mut body, persisted, why);
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
    AxPath(id): AxPath<i64>,
    Json(edit): Json<EditionEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let (work_id, key) = edition_owner(&conn, &principal.roots, id)?;
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
/// and rewrites on every pass — and `write_chapters`'s own doc comment is
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
    AxPath(id): AxPath<i64>,
    Json(edit): Json<MemberEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let (work_id, sha256) = member_owner(&conn, &principal.roots, id)?;
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
/// and external ids — the parts of a series edit that still flow through
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
/// series already holds — its own doc comment has the reason — so a
/// refusal here aborts the whole request via `?` before any work's document
/// is touched at all, the same as an empty `titles` array already did.
///
/// **The per-work loop below never resolves series identity by title text
/// either, regardless.** It calls [`edit_series_member_raw`], not
/// [`edit_work_raw`], which applies through
/// [`librarydoc::apply_with_series_id`] rather than [`librarydoc::apply`] —
/// that function's own doc comment has the mechanism. Before this existed,
/// the loop used the ordinary [`edit_work_raw`], which reached
/// [`librarydoc::apply_series`] for every work regardless of what `edit`
/// itself named, since `doc.series.titles` (read fresh from
/// [`librarydoc::of`]) already carried whatever `set_series_titles` had just
/// written — and if that text collided with another series, `apply_series`
/// resolved onto it instead. That was the Critical the delta review of the
/// fix waves (`f133b3e..910b15a`) found and reproduced end to end: every
/// work in this loop migrated onto the wrong series, and once none was left
/// under the one `id` named, [`librarydoc::forget_orphaned_series`] deleted
/// it — cascading its `series_title` and `external_id` rows, 200,
/// `persisted: true`, no `why`. The refusal above and this loop's own
/// `apply_with_series_id` close it two ways at once: the ambiguous state
/// this needs can no longer be written, and even were it already there
/// regardless (stale data predating the refusal), this loop could not
/// re-resolve onto it.
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
    AxPath(id): AxPath<i64>,
    Json(edit): Json<SeriesEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let works = works_in_series(&conn, &principal.roots, id)?;
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
        // The worst case, not the last one — see `worst_case`'s own doc
        // comment. Every work's document is still saved, exactly as before;
        // only the answer about them changes.
        //
        // **A hard `Err` here has the same abort semantics `patch_work`'s own
        // fan-out documents at its loop.** It aborts the whole request through
        // `NoAnswer::Refused` (409) via `?`, with no `persisted`/`why` in the
        // answer at all. The works already looped over keep the document this
        // loop just gave them; the ones not yet reached keep their pre-edit
        // one. Titles are not part of that staleness any more: the write
        // above already lands before the loop's first iteration, atomically,
        // so every not-yet-reached work's `series_title` row is already
        // correct — only its own JSON document file still names the old
        // text. `external_id` is the one table still keyed to the loop
        // itself: it is upserted, not replaced, so the first iteration's
        // `librarydoc::apply_with_series_id` already rewrites it for the
        // whole series (kept by `series_id`, not `work_id`), and a
        // not-yet-reached work's stale document is the only thing left
        // behind for it too. It heals on a retry:
        // `librarydoc::apply_with_series_id` is idempotent (its own doc
        // comment), and `set_series_titles` overwriting the same title set
        // twice is a no-op, so re-running this whole handler over the
        // current membership re-saves the already-correct works with no
        // change and finally saves the ones an earlier attempt did not
        // reach.
        //
        // **"The current membership" is `works_in_series`'s own answer —
        // `principal.roots`-filtered (I8) — not the series' true membership,
        // and for a restricted grant those two can differ permanently, not
        // only mid-retry.** `set_series_titles` above writes `series_title`
        // for the whole series regardless of `principal.roots`, since
        // `series_title` has no per-work grant to check; this loop then
        // re-saves only the works `works_in_series` returned. A work under
        // this same series but outside `principal.roots` is never in that
        // list, on this request or any later one from this same principal —
        // so a restricted administrator's retry heals every not-yet-reached
        // work *this principal can see* and cannot reach a hidden sibling's
        // document at all. That document is not corrupted — its own row data
        // is untouched, only its cached JSON copy of the series block goes
        // stale — and a principal whose grant does cover it (or an
        // unrestricted admin) heals it the same way: by saving that work's
        // document once, through this route or a rescan.
        //
        // **Assessed against this same delta review's Critical fix, and NOT
        // reduced by it.** The stale document's own hazard is that some later,
        // independent rebuild re-applies it — `derive::write_work`'s own
        // read-back runs `librarydoc::apply` (never `apply_with_series_id`)
        // over every work with no `LIBRARY` marker yet, which every work has
        // on a database rebuilt from scratch — and `apply` still reaches
        // `apply_series`'s ordinary resolve-by-title matching, deliberately
        // left untouched here for the reason this module's own doc comment on
        // `patch_series` gives: the import path needs it. A stale document's
        // title text not matching whatever the fresh scan just derived for
        // this series is exactly the general case `apply_series` was always
        // built to handle by minting or moving — the same mechanism, not a
        // new one this fix introduced or closed. So neither half of the
        // Critical's fix touches this: `set_series_titles`'s refusal only
        // runs inside *this* request, against titles *this* request is
        // writing, and a stale document's own rebuild happens in a
        // completely different one; `apply_with_series_id` only replaces
        // what this loop itself would have called, not what the read-back
        // calls. Left open as O86 (`docs/status.md`) rather than restricting
        // this write to the grant — the latter would mean either refusing a
        // straddling series' whole title edit outright, or leaving `series_title`
        // itself only partially updated, and choosing between those is a
        // product decision this fix round does not have the standing to make
        // unilaterally.
        let mut last_doc = None;
        let mut saves = Vec::new();
        for work_id in works {
            let (doc, saved) =
                edit_series_member_raw(&conn, work_id, id, |doc| apply_series_edit(doc, &edit))?;
            saves.push(saved);
            last_doc = Some(doc);
        }
        let mut body = last_doc.expect("checked works.is_empty() above");
        let (persisted, why) = worst_case(saves);
        merge_worst_case(&mut body, persisted, why);
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
