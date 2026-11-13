//! Where you were, kept as a row rather than inside the sidecar.
//!
//! Position is server-side because it has to survive a different machine and
//! because a progress tracker needs to read it. It is recorded in **slots**,
//! not pages: a slot is the addressed unit everywhere else in the format, and
//! a page would lose which half of a joined spread was reached.
//!
//! It is a row in `taiyaku-library`'s `progress` table rather than a field in
//! `taiyaku.json`, which stays a pure alignment document that diffs
//! meaningfully in git. `taiyaku_library::state::progress`/`put_progress` own
//! the storage, and this module's job is bridging `work: &Work` — keyed by
//! slug — to the numeric `work_id` those functions need
//! (`works::find_row`, `works::row_id_for_write`).

use serde::Deserialize;
use taiyaku_core::{MemberPage, Sidecar, SlotOf};
use taiyaku_library::state::{self, Arrival, Numbering};
use taiyaku_library::{Library, Position, Progress};

use crate::view::View;
use crate::works::{self, Mint, NoRow, Work};

/// Every position this reader holds on this work, newest first.
///
/// An empty vector where nothing was ever recorded. "Never opened" is an empty
/// set and "on the first page" is a set holding slot 0, and conflating them
/// would reopen a work at its cover forever.
///
/// A work with no row in `library`'s `work` table answers empty without
/// touching `progress` at all: `works::find_row` never creates a row.
///
/// Each slot is in `numbered`, the document the reader's view was numbered
/// from (O244), and each `at` the page its row names, found or not (O240).
///
/// # Errors
///
/// Returns an error if SQLite fails, or if a stored position's anchor has
/// some but not all of `at_sha256`, `at_nth` and `at_page` set, or a half
/// with none of them: a state `taiyaku_library::state::put_progress_at`
/// cannot write.
pub fn load(
    library: &Library,
    work: &Work,
    user_id: i64,
    numbered: &Sidecar,
) -> Result<Vec<Position>, String> {
    let conn = library.conn();
    let Some(work_id) = works::find_row(&conn, work)? else {
        return Ok(Vec::new());
    };
    state::progress_anchored_in(&conn, work_id, user_id, Numbering::Document(numbered))
}

/// Record where the reader got to, unless a save of theirs on this work that
/// arrived after `arrival` is already written, which answers as a written one
/// would: the stored position is the newer (`state::put_progress_in_order`).
///
/// The library row bridging `work`'s slug to a numeric `work_id` is minted
/// here if none exists and `mint` allows it — see `works::row_id_for_write`.
///
/// The slot is in `numbered`, the document the reader's view was numbered
/// from, and the page it names there is stored with it (O244).
///
/// # Errors
///
/// [`NoRow::Gone`] if the work's row is gone and `mint` mints none, and
/// [`NoRow::Failed`] if SQLite fails.
pub fn store(
    library: &Library,
    work: &Work,
    user_id: i64,
    progress: &Progress,
    numbered: &Sidecar,
    arrival: Arrival,
    mint: Mint,
) -> Result<(), NoRow> {
    let conn = library.saving().map_err(NoRow::Failed)?;
    let work_id = works::row_id_for_write(&conn, work, mint)?;
    state::put_progress_in_order(
        &conn,
        library.progress_order(),
        arrival,
        work_id,
        user_id,
        progress,
        Numbering::Document(numbered),
    )
    .map_err(NoRow::Failed)?;
    Ok(())
}

/// A position as a reader's `PUT` sends it: [`Progress`] and the page the
/// slot showed in the reader's layout (O240). Serve's own, so `Progress`
/// stays the library's write shape; unknown keys are ignored, as there.
#[derive(Debug, Deserialize)]
pub(crate) struct Body {
    pub(crate) edition: String,
    pub(crate) slot: u32,
    #[serde(default)]
    pub(crate) at: Option<MemberPage>,
}

/// What a position's `at` decides (settled call 9).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Placed {
    /// Write it on this slot, in the account's numbering.
    Slot(u32),
    /// Leave the stored row as it is.
    Kept,
    /// Refuse it: a page the account is not sent, and not the row's.
    Refused,
}

/// Where a position's `at` puts it on `edition` as `view` sends it, `stored`
/// the page its row there names. A page found wins over the slot sent, a
/// half no longer held going to its page's first slot (pre-deploy decision
/// 6). Equality decides only a page not found, and writes nothing, since the
/// writer would tie the row to the page its slot shows now. Asked first, it
/// would skip the write that makes this edition the one resumed (spec gap 4).
pub(crate) fn place(
    view: &View,
    edition: &str,
    at: &MemberPage,
    stored: Option<&MemberPage>,
) -> Placed {
    match view.slot_of_anchor(edition, at) {
        Some(SlotOf::Exact(slot) | SlotOf::PageOnly(slot)) => Placed::Slot(slot),
        None if stored == Some(at) => Placed::Kept,
        None => Placed::Refused,
    }
}

#[cfg(test)]
mod tests;
