//! `taiyaku.progress.json`: every position one reader holds on one work.
//!
//! **Versioned since a position became per-edition.** The document carried a
//! bare `{"edition": …, "slot": …}` while a reader held one position per work,
//! and that shape cannot say what it now has to. `taiyaku_progress: 1` is the
//! same device `TruthDoc`'s `taiyaku_truth: 1` uses, and for the same reason:
//! this document now has a shape that can change again.
//!
//! **The legacy form is still read, and that is not politeness.** Real
//! documents are on disk — `corpus/gotoubun-v01/taiyaku.progress.json` is one
//! — and `taiyaku serve <sidecar>…` runs `migrate::apply_progress` over them
//! at *every* start. A refusal there would drop a reader's position silently,
//! and `export --in-place` would then have nothing to write back.
//!
//! Reading answers [`Recorded`] rather than `Vec<Position>`, and that is the
//! whole of what the two forms above force: a versioned document says when
//! each position was reached and the legacy bare form says nothing at all, so
//! the time is an `Option` here and nowhere else. `migrate::apply_progress`
//! is where the two are told apart — a recorded time is written as the row's
//! own, an absent one is stamped `datetime('now')`. Writing takes
//! `Position`s, which is what the store holds.

use serde::{Deserialize, Serialize};

use crate::{Position, Progress};

/// One position as a document carries it: the place, and when it was reached
/// if the document says.
///
/// **Not [`Position`], because `updated` is not always there.** `Position` is
/// the store's shape and every row has a time; the legacy bare
/// `{"edition": …, "slot": …}` has none, and reading it as a `Position` would
/// mean inventing one here — in the module that reads documents — rather than
/// stamping one in the module that writes rows. So the absence travels, and
/// `state::put_progress_at` is handed it.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub place: Progress,
    /// `None` for the legacy bare form, which has no such field.
    pub updated: Option<String>,
}

/// The version this build writes and is the only one it reads.
const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressDoc {
    pub taiyaku_progress: u32,
    pub positions: Vec<Position>,
}

/// Read either form.
///
/// **Tried versioned-first, legacy-second, and the order matters.** A bare
/// `{"edition", "slot"}` cannot deserialise as a `ProgressDoc` — the required
/// `taiyaku_progress` is absent — while the reverse is not true of every
/// serde configuration, so the unambiguous shape is tested first and the
/// permissive one is the fallback.
///
/// # Errors
///
/// Returns an error if the text is neither form, or is a version this build
/// does not know.
pub fn from_json(text: &str) -> Result<Vec<Recorded>, String> {
    if let Ok(doc) = serde_json::from_str::<ProgressDoc>(text) {
        if doc.taiyaku_progress != VERSION {
            return Err(format!(
                "this is a taiyaku_progress {} document and this build reads {VERSION}; \
                 a newer taiyaku wrote it",
                doc.taiyaku_progress
            ));
        }
        return Ok(doc
            .positions
            .into_iter()
            .map(|p| Recorded {
                place: Progress {
                    edition: p.edition,
                    slot: p.slot,
                },
                updated: Some(p.updated),
            })
            .collect());
    }
    let legacy: Progress =
        serde_json::from_str(text).map_err(|e| format!("not a readable progress document: {e}"))?;
    Ok(vec![Recorded {
        place: legacy,
        updated: None,
    }])
}

/// Write the versioned form.
///
/// # Errors
///
/// Returns an error if serialisation fails.
pub fn to_json(positions: &[Position]) -> Result<String, String> {
    serde_json::to_string_pretty(&ProgressDoc {
        taiyaku_progress: VERSION,
        positions: positions.to_vec(),
    })
    .map_err(|e| format!("serialising a progress document: {e}"))
}

#[cfg(test)]
mod tests;
