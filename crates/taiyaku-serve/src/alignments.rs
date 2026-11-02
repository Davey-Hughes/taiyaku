//! A hand correction, into the rows the next request is answered from.
//!
//! The fourth bridge beside `progress.rs`, `notes.rs` and `truth.rs`, and the
//! shortest, because two of the three things those modules own do not apply
//! here: the serialisation is `Sidecar::to_json`'s and the validator is
//! `routes::sidecar_etag`'s. What is left is the bridge itself — `work: &Work`,
//! keyed by slug, to the numeric `work_id`
//! `taiyaku_library::state::put_alignments` needs.
//!
//! **There is no `load` here, deliberately.** The other three read their
//! document out of the rows on every request; the alignment document is held
//! in memory (`Work`'s `RwLock<Arc<Sidecar>>`, read from
//! `taiyaku_library::project::sidecar` at start and again by every refresh of
//! a live registry), because every page request resolves a counterpart
//! through it. A `load` would be a second way to read what `Work::sidecar()`
//! already answers, and two of those is how they come to disagree.
//!
//! **This takes a `&Connection`, where the other three take a `&Library`.**
//! Not a style difference: `store` is called from inside `Work::update`, which
//! holds that work's sidecar lock for writing, and acquiring `Library`'s mutex
//! there would put this one call site in the opposite lock order from
//! `routes::list_works` — which holds the library mutex across a
//! `work.sidecar()` read — and two threads in opposite orders is a deadlock,
//! not a race that shows up under load. The connection is therefore taken
//! *before* the update and passed in, which makes the order library-then-work
//! everywhere.

use rusqlite::Connection;
use taiyaku_core::Sidecar;

use crate::works::{self, Work};

/// Replace the work's stored alignments with `doc`'s.
///
/// The library row bridging `work`'s slug to a numeric `work_id` is minted
/// here on first write if none exists yet, exactly as it is for progress,
/// notes and truth — see `works::row_id_for_document`. In production it is
/// never minted: every `Work` `taiyaku serve` builds came out of an import
/// and already has its row. The path that mints is this crate's own fixtures
/// and `corpus.rs`, which pair a hand-built `Work` with an empty library.
///
/// `doc` is the *replacement* document — the corrected one — not the one the
/// work currently holds, because the caller is inside `Work::update` and has
/// not swapped it in yet.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn store(conn: &Connection, work: &Work, doc: &Sidecar) -> Result<(), String> {
    let work_id = works::row_id_for_document(conn, &work.id, doc, &work.path)?;
    taiyaku_library::state::put_alignments(conn, work_id, doc)
}
