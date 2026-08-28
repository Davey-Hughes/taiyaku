//! Where you were, kept as a row rather than inside the sidecar.
//!
//! Position is server-side because it has to survive a different machine and
//! because a progress tracker needs to read it. It is recorded in **slots**,
//! not pages: a slot is the addressed unit everywhere else in the format, and
//! a page would lose which half of a joined spread was reached.
//!
//! It is a row in `taiyaku-library`'s `progress` table rather than a field in
//! `taiyaku.json`, which stays a pure alignment document that diffs
//! meaningfully in git. Until Task 13 this was a sibling `taiyaku.progress.json`
//! file instead; `taiyaku_library::state::progress`/`put_progress` now own the
//! storage, and this module's job is bridging `work: &Work` — keyed by slug —
//! to the numeric `work_id` those functions need (`works::find_row`,
//! `works::row_id_for_write`).

use taiyaku_library::{Library, Position, Progress};

use crate::works::{self, Work};

/// Every position this reader holds on this work, newest first.
///
/// An empty vector where nothing was ever recorded. The distinction the old
/// `Option` carried is intact: "never opened" is an empty set and "on the
/// first page" is a set holding slot 0, and conflating them would reopen a
/// work at its cover forever.
///
/// A work with no row in `library`'s `work` table answers empty without
/// touching `progress` at all: `works::find_row` never creates a row.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn load(library: &Library, work: &Work, user_id: i64) -> Result<Vec<Position>, String> {
    let conn = library.conn();
    let Some(work_id) = works::find_row(&conn, work)? else {
        return Ok(Vec::new());
    };
    taiyaku_library::state::progress(&conn, work_id, user_id)
}

/// Record where the reader got to.
///
/// The library row bridging `work`'s slug to a numeric `work_id` is minted
/// here on first write if none exists yet — see `works::row_id_for_write`.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn store(
    library: &Library,
    work: &Work,
    user_id: i64,
    progress: &Progress,
) -> Result<(), String> {
    let conn = library.conn();
    let work_id = works::row_id_for_write(&conn, work)?;
    taiyaku_library::state::put_progress(&conn, work_id, user_id, progress)
}

#[cfg(test)]
mod tests;
