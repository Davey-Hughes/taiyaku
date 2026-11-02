//! Reading and writing a work's labelling truth document, as rows.
//!
//! The shape `notes.rs` already has, and deliberately not a second
//! convention: one serialisation shared by the projection, the response body
//! and the `ETag` over that body, so a validator can never be computed over
//! bytes a client was not sent — the failure that would make a conditional
//! write silently unconditional. `taiyaku_library::state::truth`/`put_truth`
//! own the storage, and this module's job is bridging `work: &Work` — keyed
//! by slug — to the numeric `work_id` those functions need.

use taiyaku_core::TruthDoc;
use taiyaku_library::Library;

use crate::works::{self, Work};

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
