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

use taiyaku_core::Sidecar;
use taiyaku_library::state::{self, Arrival, Numbering};
use taiyaku_library::{Library, Position, Progress};

use crate::works::{self, Work};

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
/// from (O244).
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
    taiyaku_library::state::progress_in(&conn, work_id, user_id, Numbering::Document(numbered))
}

/// Record where the reader got to, unless a save of theirs on this work that
/// arrived after `arrival` is already written, which answers as a written one
/// would: the stored position is the newer (`state::put_progress_in_order`).
///
/// The library row bridging `work`'s slug to a numeric `work_id` is minted
/// here on first write if none exists yet — see `works::row_id_for_write`.
///
/// The slot is in `numbered`, the document the reader's view was numbered
/// from, and the page it names there is stored with it (O244).
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn store(
    library: &Library,
    work: &Work,
    user_id: i64,
    progress: &Progress,
    numbered: &Sidecar,
    arrival: Arrival,
) -> Result<(), String> {
    let conn = library.saving()?;
    let work_id = works::row_id_for_write(&conn, work)?;
    state::put_progress_in_order(
        &conn,
        library.progress_order(),
        arrival,
        work_id,
        user_id,
        progress,
        Numbering::Document(numbered),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
