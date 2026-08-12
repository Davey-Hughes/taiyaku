//! Reading and writing `<stem>.truth.json` for a served work.
//!
//! The shape `notes.rs` already has, and deliberately not a second convention:
//! one serialisation shared by the file, the response body and the `ETag` over
//! that body, so a validator can never be computed over bytes a client was not
//! sent — the failure that would make a conditional write silently
//! unconditional.

use std::path::PathBuf;

use taiyaku_core::TruthDoc;

use crate::works::Work;

/// Where a work's truth document sits: beside its sidecar, same stem.
pub fn truth_path(work: &Work) -> PathBuf {
    work.sibling("truth.json")
}

/// The one serialisation of a truth document.
///
/// `notes::to_json`'s reasoning, unchanged: shared by disk, wire and
/// validator so the three cannot drift. Unlike `notes::to_json`, this returns
/// a bare `String` rather than a `Result`: `TruthDoc::to_json` already carries
/// the infallibility guarantee (see its own doc comment), so wrapping it in a
/// `Result` nothing can ever put an `Err` in would only make every caller
/// handle a case that cannot happen.
#[must_use]
pub fn to_json(doc: &TruthDoc) -> String {
    doc.to_json()
}

/// # Errors
///
/// Returns an error if the file exists but cannot be read or parsed. A file
/// that is not there is `Ok(None)` — nobody has labelled this work yet, which
/// is not a failure.
pub fn load(work: &Work) -> Result<Option<TruthDoc>, String> {
    let path = truth_path(work);
    match std::fs::read_to_string(&path) {
        Ok(text) => TruthDoc::from_json(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// # Errors
///
/// Returns an error if writing the document fails — see `atomic::write` for
/// the two ways that can happen.
///
/// Atomic, via the same `atomic::write` `notes::store` calls rather than a
/// second way to write a file in place: a labelling session represents hours
/// of irreplaceable human work, and a truncated write that loses it is the
/// defect O26 was.
pub fn store(work: &Work, doc: &TruthDoc) -> Result<(), String> {
    crate::atomic::write(&truth_path(work), to_json(doc).as_bytes(), "json")
}
