//! Reading and writing a work's labelling truth document, as rows.
//!
//! The shape `notes.rs` already has, and deliberately not a second
//! convention: one serialisation shared by the projection, the response body
//! and the `ETag`. The tag is over the body sent for `Access::Every`, and
//! otherwise over the stored document and the view it was cut by
//! (`View::etag_naming_numbering`, spec §3.8 decision 11), so the bytes a
//! client is sent never change under one tag — the failure that would make a
//! conditional write silently unconditional.
//!
//! `taiyaku_library::state::truth`/`put_truth` own the storage, and this
//! module's job is bridging `work: &Work` — keyed by slug — to the numeric
//! `work_id` those functions need.

use taiyaku_core::{TruthDoc, TruthEdition};
use taiyaku_library::Library;

use crate::view::View;
use crate::works::{self, Work};

/// Why a truth document was not written for an account with grant limits.
#[derive(Debug)]
pub(crate) enum Unwritable {
    /// An edition the account is not sent, in the words of one the work does
    /// not have, so a refusal tells no one which editions are real (O60) — a
    /// 404's shape.
    Missing(String),
    /// A narrowed edition, whose entry is in a numbering the account is not
    /// sent (spec §3.8) — a 403's shape.
    PartlyVisible(String),
}

/// The one serialisation of a truth document.
///
/// `notes::to_json`'s reasoning, unchanged: shared by the projection, the
/// wire and the validator so the three cannot drift. Unlike `notes::to_json`,
/// this returns a bare `String` rather than a `Result`: `TruthDoc::to_json`
/// already carries the infallibility guarantee (see its own doc comment), so
/// wrapping it in a `Result` nothing can ever put an `Err` in would only make
/// every caller handle a case that cannot happen.
#[must_use]
pub fn to_json(doc: &TruthDoc) -> String {
    doc.to_json()
}

/// What is stored, or `None` where nothing has ever been labelled.
///
/// A work with no row in `library`'s `work` table answers `None` without
/// touching `taiyaku_library`'s `truth_page`/`truth_box` tables at all —
/// `works::find_row` never creates a row, so there is nothing yet for a
/// `work_id` to be. Once a row exists, `taiyaku_library::state::truth` makes
/// its own `Some`/`None` distinction from whether any page has been looked
/// at — see its own doc comment.
///
/// # Errors
///
/// Returns an error if SQLite fails, or if a stored row's `axis` or `kind`
/// is not a value this build ever writes — see
/// `taiyaku_library::state::truth`'s own `# Errors`.
pub fn load(library: &Library, work: &Work) -> Result<Option<TruthDoc>, String> {
    let conn = library.conn();
    let Some(work_id) = works::find_row(&conn, work)? else {
        return Ok(None);
    };
    taiyaku_library::state::truth(&conn, work_id)
}

/// Replace the whole truth document for a work, inside one transaction.
///
/// The library row bridging `work`'s slug to a numeric `work_id` is minted
/// here on first write if none exists yet — see `works::row_id_for_write`. A
/// labelling session represents hours of irreplaceable human work; the
/// transaction `taiyaku_library::state::put_truth` wraps its delete-then-
/// reinsert in is what makes the replacement atomic.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn store(library: &Library, work: &Work, doc: &TruthDoc) -> Result<(), String> {
    let conn = library.conn();
    let work_id = works::row_id_for_write(&conn, work)?;
    taiyaku_library::state::put_truth(&conn, work_id, doc)
}

/// `stored` as `view`'s account is sent it: the editions it sees whole
/// (spec §3.8, O191). Truth is shared between accounts and keyed by page in
/// the full numbering, so a narrowed edition's entry is left out rather than
/// translated, and a hidden one's because the account is not told of it.
#[must_use]
pub(crate) fn as_seen(stored: &TruthDoc, view: &View) -> TruthDoc {
    let mut seen = stored.clone();
    seen.editions.retain(|key, _| view.sees_whole(key));
    seen
}

/// The `ETag` and body `view`'s account is sent for `stored`, the tag naming
/// its numbering (spec §3.8, decision 11). `put_truth` answers with this so
/// its tag is the next read's.
#[must_use]
pub(crate) fn tagged_for(stored: &TruthDoc, view: &View) -> (String, String) {
    let json = to_json(stored);
    let tag = view.etag_naming_numbering(&json);
    if view.visible().is_none() {
        return (tag, json);
    }
    (tag, to_json(&as_seen(stored, view)))
}

/// `incoming`, written by `view`'s account on `work`, as it replaces `stored`
/// (spec §3.8): every entry of `stored` for an edition the account does not
/// see whole, and the body leaves out, is kept, because a write replaces the
/// whole document and would otherwise delete it.
///
/// # Errors
///
/// For an entry of an edition the account does not see whole that is not
/// `stored`'s entry unchanged (a client that read it before the grant
/// narrowed sends it back as it was): [`Unwritable::PartlyVisible`] for a
/// narrowed edition, and [`Unwritable::Missing`] for any other, the work's or
/// not.
pub(crate) fn as_stored(
    mut incoming: TruthDoc,
    stored: &TruthDoc,
    view: &View,
    work: &str,
) -> Result<TruthDoc, Unwritable> {
    if view.visible().is_none() {
        return Ok(incoming);
    }
    for (key, entry) in &incoming.editions {
        if view.sees_whole(key) {
            continue;
        }
        let unchanged = stored.editions.get(key).is_some_and(|held| {
            let TruthEdition { members, pages } = held;
            *members == entry.members && *pages == entry.pages
        });
        if unchanged {
            continue;
        }
        return Err(if view.narrowed(key).is_some() {
            Unwritable::PartlyVisible(format!(
                "`{key}` of `{work}` is only partly visible to this account, so its ground \
                 truth cannot be written here"
            ))
        } else {
            Unwritable::Missing(format!(
                "this truth document names the edition `{key}`, which work `{work}` does not \
                 have"
            ))
        });
    }
    for (key, held) in &stored.editions {
        if !view.sees_whole(key) {
            incoming
                .editions
                .entry(key.clone())
                .or_insert_with(|| held.clone());
        }
    }
    Ok(incoming)
}
