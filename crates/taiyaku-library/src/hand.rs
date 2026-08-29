//! Where a work's hand-fixed metadata lives on disk, and how it is found
//! again. Build design §12.
//!
//! `<stem>.library.json` beside a sidecar is the store spec §4.4's answer, and
//! it answers only for a work that has a sidecar. `import::scanned` leaves
//! `sidecar_path` NULL **by design** — one `taiyaku.json` per work directory
//! cannot describe fourteen volumes sitting beside each other
//! (`derive::Candidate::path`'s own doc comment) — so a scanned work had
//! nowhere at all to keep the part nothing can re-derive. That is what this
//! module gives it:
//!
//! ```text
//! <root>/.taiyaku/<content_key>.library.json
//! ```
//!
//! **Dotted, so the scanner never has to be taught to ignore it.**
//! `scan.rs:679` skips every entry whose name begins with `.`, and
//! `watch.rs`'s own probe file already relies on that. An exclusion list would
//! have been a second thing to keep in step with this one.
//!
//! **Inside a root**, because the claim being kept is the store spec §4.4's:
//! losing the database costs a rescan rather than costing the organisational
//! work. A file that lives with the machine rather than with the library does
//! not keep that claim.
//!
//! **Keyed by `content_key`**, per §4.1 — identity is content, path is a hint.
//! [`crate::identity::content_key`] hashes the hub edition's members alone,
//! and its own doc comment records that the hub is fixed when the work is
//! created *so that* adding an edition does not change a work's identity. So
//! this address survives every file being renamed, the library being remounted
//! somewhere else, and editions being added — which is the whole list of
//! things that actually happen to a library.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};

/// The directory, inside a root, that holds documents taiyaku owns.
const DIR: &str = ".taiyaku";

/// Where this work's `<content_key>.library.json` belongs, or `None` when the
/// work has no member under any root.
///
/// **The hub edition's first member that still names a root decides the
/// root** — not simply its first member. A `Group` collects entries across
/// every root, so "which root" has a single answer only at member level —
/// the same reasoning `member.root_id`'s own comment in migration 6 gives for
/// that column existing. `member.root_id` is `ON DELETE SET NULL`, so a hub
/// edition can mix members with a root and members without one; no current
/// code path deletes a `root` row, but the query orders by ordinal and skips
/// rootless members rather than assuming the first member by ordinal always
/// has one.
///
/// `None` is the ordinary answer for a work imported from a sidecar: it has a
/// `sidecar_path`, so `<stem>.library.json` beside it is already its home and
/// `export::work` already writes it.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, or if SQLite fails.
pub fn path_for(conn: &Connection, work_id: i64) -> Result<Option<PathBuf>, String> {
    let content_key: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [work_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("reading the content key of work {work_id}: {e}"))?;

    let root: Option<String> = conn
        .query_row(
            "SELECT root.path
               FROM member
               JOIN edition ON edition.id = member.edition_id
               JOIN work    ON work.id    = edition.work_id
               JOIN root    ON root.id    = member.root_id
              WHERE work.id = ?1 AND edition.key = work.hub
              ORDER BY member.ordinal
              LIMIT 1",
            [work_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("reading the root of work {work_id}: {e}"))?;

    Ok(root.map(|root| {
        PathBuf::from(root)
            .join(DIR)
            .join(format!("{content_key}.library.json"))
    }))
}

/// What [`save`] did.
///
/// `path` is `None` for a work with no address at all — an imported work,
/// whose document belongs beside its sidecar and is written by
/// [`crate::export::work`]. `persisted` is `false` when an address existed and
/// the write did not land, which is a read-only root and not an error: see
/// [`save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub path: Option<PathBuf>,
    pub persisted: bool,
    /// Why the document did not reach the disk, when it did not — the
    /// `io::Error`'s own message, naming the path.
    ///
    /// **A bool alone was the first shape and it was not honest enough.**
    /// `taiyaku_read::atomic::write` is built to distinguish a staging
    /// failure from a rename failure and to name the exact path (its O29
    /// comment); collapsing that to `false` threw away the only thing a
    /// caller could act on. Not failing the edit is what a read-only root
    /// requires — discarding the reason is not.
    pub why: Option<String>,
}

/// Project this work's hand-fixed metadata and write it to
/// [`path_for`]'s address, then record the `LIBRARY` marker.
///
/// **The marker is recorded whether or not the file lands, and that is
/// deliberate — including when there is no address to land it at.** It is the
/// same marker `derive::write_work`'s own read-back arms, and what that
/// function consults before overwriting `work.series_id`, `work.volume`,
/// `edition.kind` and `edition.language` (I7's fix). An edit made against a
/// read-only root is still an edit the next scan must not revert; so is one
/// made against an imported work, whose sidecar leaves [`path_for`] answering
/// `None` — [`Saved::path`] is `None` too, there and only there, but the
/// marker is recorded regardless. What is lost in both cases is durability,
/// and [`Saved::persisted`] is how a caller reports that rather than
/// discovering it later.
///
/// A document that projects empty removes the file instead of writing an empty
/// one, matching `export::work`'s own write-or-remove decision. There is
/// nothing to remove when there is no address either, so an unedited work
/// with no address takes neither branch below and simply records no marker,
/// same as an unedited work that has one.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if projecting the document
/// fails, or if recording the marker fails. **Not** if the write fails: that
/// is `persisted: false`.
pub fn save(conn: &Connection, work_id: i64) -> Result<Saved, String> {
    let path = path_for(conn, work_id)?;
    let doc = crate::librarydoc::of(conn, work_id)?;

    let (persisted, why) = match &path {
        // No address to write to at all — an imported work, whose document
        // already lives beside its sidecar rather than under a root's
        // `.taiyaku`. Nothing here is durable, and there is no path to name a
        // failure against, so `why` stays `None` — see `edit_work`'s own doc
        // comment in `taiyaku-serve` for the asymmetry that leaves in the
        // answer a caller sees.
        None => (false, None),
        Some(path) if doc.is_empty() => match std::fs::remove_file(path) {
            Ok(()) => (true, None),
            // **Nothing there is the ordinary case, not a failure**, and it is
            // the common one: every work that has never been hand-edited takes
            // this branch. `export::work` draws the same line for the same
            // reason — "nothing there is the ordinary case […] and is not a
            // removal to report" — and reporting `persisted: false` for it
            // would tell a caller the library is not durable whenever there
            // was simply nothing to do.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (true, None),
            Err(e) => (false, Some(format!("{}: {e}", path.display()))),
        },
        Some(path) => {
            // The parent is created here rather than at root registration: a
            // root that never holds a hand edit never grows a `.taiyaku`
            // directory, and a directory nothing writes into is a directory
            // that reads as unexplained the first time someone finds it in
            // their library.
            let parent = path.parent().expect("an address always has a directory");
            match std::fs::create_dir_all(parent)
                .map_err(|e| format!("{}: {e}", parent.display()))
                .and_then(|()| taiyaku_read::atomic::write(path, doc.to_json().as_bytes(), "json"))
            {
                Ok(()) => (true, None),
                Err(why) => (false, Some(why)),
            }
        }
    };

    // **`!doc.is_empty()`, not `persisted`.** The marker answers "has this
    // library absorbed this document", and it has: the rows are written
    // whether or not the file reached the disk — or whether there was ever a
    // disk to reach — and the guard must be armed either way. Passing
    // `persisted` here would leave a read-only root's edit, or a no-address
    // work's edit, unprotected, which is the exact case this function's own
    // doc comment promises it is not. The empty document is the one that
    // records `false` — there is nothing left to protect, so derivation
    // should resume, which is the same write-or-remove symmetry
    // `export::work` keeps.
    crate::migrate::record_absorbed(conn, work_id, crate::migrate::LIBRARY, !doc.is_empty())?;

    Ok(Saved {
        path,
        persisted,
        why,
    })
}

/// Read the document at this work's address, if there is one there.
///
/// `Ok(None)` for a work with no address and for an address with no file —
/// both are the ordinary case and neither is a failure. A file that exists and
/// cannot be read or parsed **is** an error: it is somebody's organisational
/// work, and silently deriving over it is the outcome this whole module exists
/// to prevent.
///
/// **The path comes back with the document.** `derive::write_work`'s
/// read-back applies what this returns and may fail doing so —
/// `librarydoc::apply`'s own errors name a `work_id` and a `series_id`, never
/// a file — so the caller needs the address to report an apply failure the
/// same way it reports a read or parse failure. Handing it back here, where
/// it was already computed, is what lets both arms be formatted identically;
/// the alternative, re-deriving the address at the report site, would make a
/// message's honesty depend on two call sites agreeing.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if the file exists and cannot
/// be read, or if it is not a readable library document.
pub fn load(
    conn: &Connection,
    work_id: i64,
) -> Result<Option<(PathBuf, crate::librarydoc::LibraryDoc)>, String> {
    let Some(path) = path_for(conn, work_id)? else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc = crate::librarydoc::LibraryDoc::from_json(&text)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some((path, doc)))
}

#[cfg(test)]
mod tests;
