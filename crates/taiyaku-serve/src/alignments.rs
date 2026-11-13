//! A hand correction, into the rows the next request is answered from.
//!
//! The fourth writer beside `progress.rs`, `notes.rs` and `truth.rs`, and the
//! shortest: the serialisation is `Sidecar::to_json`'s, the validator is
//! `routes::sidecar_etag`'s, and the caller resolves the numeric `work_id`
//! (`works::row_id_for_document`) before it writes the work's file, so a work
//! whose row is gone writes neither (O184). What is left is the rows.
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
//! everywhere. It is [`taiyaku_library::Library::saving`]'s, with a transaction
//! already open, so the store's write lock is held before the work's (O272).

use rusqlite::Connection;
use taiyaku_core::Sidecar;

/// Replace the stored alignments of the work `work_id` names with `doc`'s.
///
/// The caller resolves `work_id` as progress, notes and truth do. A fixed
/// registry still mints a missing row (`works::Mint::Bridge`):
/// `test_support::state`'s fixtures and `corpus.rs` pair a fixed registry with
/// an empty library. A live registry never does.
///
/// `doc` is the *replacement* document — the corrected one — not the one the
/// work currently holds, because the caller is inside `Work::update` and has
/// not swapped it in yet.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn store(conn: &Connection, work_id: i64, doc: &Sidecar) -> Result<(), String> {
    taiyaku_library::state::put_alignments(conn, work_id, doc)
}
