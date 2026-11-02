//! What a reader wrote down, kept as rows rather than inside the sidecar.
//!
//! Notes are content, so they are server-side for the same reason progress is:
//! they have to survive a different machine, and a browser profile is neither
//! portable nor exportable. They live in `taiyaku-library`'s `note` table
//! rather than a field in `taiyaku.json` because that document describes
//! correspondence and must diff meaningfully in git — human prose would be
//! noise in it, and re-solving rewrites it, which is the hazard O13 already
//! records for hand corrections. `taiyaku_library::state::notes` and
//! `put_notes` own the storage.
//!
//! A note is anchored to a **slot**, not a page: a note on the left half of a
//! joined spread must not silently become a note on the whole spread. Its
//! `region`, when it has one, is in fractions of that slot, so a re-scan or the
//! same edition served at another resolution does not move the marker.

/// `Note`, `Notes` and this module's own `Region` moved down to
/// `taiyaku-library` — this crate cannot be the thing a store depends on, and
/// notes must be reachable from there for Task 9's `state::notes`. Re-exported
/// here so every `crate::notes::{Note, Notes, Region}` path in this
/// crate resolves.
///
/// **Not `taiyaku_core::regions::Region`.** The two share a name; this one is
/// a note's anchor rectangle in normalised page coordinates, that one is a
/// detected text region. Do not merge them and do not `use` one where the
/// other is meant.
pub use taiyaku_library::{Note, Notes, Region};

use taiyaku_library::Library;

use crate::works::{self, Work};

/// How far outside `[0, 1]` a region may stray before it is refused.
///
/// A region comes from a drag in a browser, divided by an element's measured
/// size, so an edge-to-edge selection lands a hair either side of exactly 1.0
/// as often as not. Refusing that would be refusing the correct answer for a
/// floating-point reason, and there is nothing a reader could do about it.
const REGION_SLACK: f64 = 1e-6;

/// Why a notes document was not accepted.
#[derive(Debug)]
pub enum Invalid {
    /// Names something the work does not have — a 404's shape.
    Missing(String),
    /// Malformed in itself — a 400's shape.
    Refused(String),
}

/// Whether two notes are anchored to exactly the same place.
///
/// Not whether they are equal: a reader retyping a note's words leaves the
/// anchor alone, and re-checking an anchor that has not moved is what makes
/// `validate` refuse documents for a note nobody touched.
fn same_anchor(a: &Note, b: &Note) -> bool {
    a.edition == b.edition && a.slot == b.slot && a.region == b.region
}

/// [`validate_against`], against the work's own sidecar.
///
/// # Errors
///
/// Returns what [`validate_against`] returns.
pub fn validate(notes: &Notes, work: &Work, stored: &Notes) -> Result<(), Invalid> {
    validate_against(notes, &work.id, &work.sidecar(), stored)
}

/// Everything that has to be true of a notes document before it replaces one.
///
/// Refusing here is what keeps a note from being written against an edition
/// that does not exist or a slot past the end of one — anchors nothing could
/// ever draw, which a reader would meet as a note that simply never appears.
///
/// **An anchor already in `stored` and unchanged is not re-checked**, and that
/// exemption is the difference between a document that can be repaired and one
/// that cannot. A client sends the whole document back on every write, so
/// without it a single note left pointing at an edition a re-solve renamed —
/// or a slot an edition shrank past — refuses *every* future write on that
/// work, forever, including the writes that would have removed it.
///
/// So a stale note survives as something a reader can see (the review list
/// already marks a note it cannot reach from here) and delete, rather than as
/// something that quietly bricks their notes. What is still refused is a
/// *new* bad anchor, and moving an existing note onto one — the cases where
/// this write is what makes the anchor wrong.
///
/// **Against the sidecar it is handed, which need not be the work's whole
/// one.** `routes::put_notes` hands it the document a caller is shown, so an
/// edition their grant hides is an edition the work does not have, here and
/// in the refusal's words (O60).
///
/// # Errors
///
/// Returns `Invalid::Refused` (a 400's shape) if `notes.work` does not match
/// `work`, if two notes in the document share an `id`, if a note's colour
/// is not lowercase ASCII letters, digits and dashes of at most 24
/// characters, or if a note's region strays outside `0..1` of its slot by
/// more than floating-point slack.
///
/// Returns `Invalid::Missing` (a 404's shape) if a note's anchor — its
/// `edition` and `slot` — is new or has moved, and names an edition `sidecar`
/// does not have, or a slot at or past that edition's slot count. An anchor
/// already present, unmoved, in `stored` is exempt from that edition/slot
/// check, which is what keeps a re-solve that invalidates an untouched
/// note's anchor from refusing every write on the work thereafter.
pub fn validate_against(
    notes: &Notes,
    work: &str,
    sidecar: &taiyaku_core::Sidecar,
    stored: &Notes,
) -> Result<(), Invalid> {
    if notes.work != work {
        return Err(Invalid::Refused(format!(
            "these notes name the work `{}`, but this is `{}`",
            notes.work, work
        )));
    }

    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for note in &notes.notes {
        if !seen.insert(note.id.as_str()) {
            return Err(Invalid::Refused(format!(
                "two notes share the id `{}`; every later edit of it would be ambiguous",
                note.id
            )));
        }

        // **Only the anchor is grandfathered**, and only where it has not
        // moved. An edition key and a slot index are the two things a re-solve
        // can invalidate underneath a note that nobody touched; a colour and a
        // region cannot go stale that way, so both are checked on every note
        // every time, and a bad one is always this write's doing.
        let unchanged = stored
            .notes
            .iter()
            .any(|held| held.id == note.id && same_anchor(held, note));
        if !unchanged {
            let edition = sidecar.editions.get(&note.edition).ok_or_else(|| {
                Invalid::Missing(format!(
                    "note `{}` names the edition `{}`, which work `{}` does not have",
                    note.id, note.edition, work
                ))
            })?;

            // Slots, not pages: a joined spread is one page and two slots, and
            // a note on one half must not be refused because the page count is
            // smaller. O3 established this on progress, for the same reason.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "edition.slots(reading).len() is at most twice the edition's page \
                          count (crate::expand's flat_map in taiyaku-core/src/slot.rs emits \
                          at most two Slots per page), and a page count is bounded by what a \
                          book can physically contain, far below u32::MAX. Kept as an \
                          #[expect] rather than a try_from().expect() helper: this runs on \
                          the write path of a client-supplied notes document against a \
                          sidecar, which a re-solve or a hand edit can change, not an \
                          in-process invariant — adding a panic here would trade a truncation \
                          that cannot happen for a request failure that, however unlikely, \
                          could."
            )]
            let slots = edition.slots(sidecar.work.reading).len() as u32;
            if note.slot >= slots {
                return Err(Invalid::Missing(format!(
                    "note `{}` names slot {} of `{}`, which has {slots} slots",
                    note.id, note.slot, note.edition
                )));
            }
        }

        if let Some(color) = &note.color {
            // The same alphabet an edition key is held to, and for the same
            // reason: this is a name that a client will look up, so it has to
            // be a name rather than an arbitrary string. Deliberately *not* a
            // list of known colours — which colours exist is presentation, and
            // this server has no business holding an opinion about it, any
            // more than it holds one about what `amber` looks like.
            let named = !color.is_empty()
                && color.len() <= 24
                && color
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !named {
                return Err(Invalid::Refused(format!(
                    "note `{}` has the colour `{color}`; a colour is a name — lowercase letters, \
                     digits and dashes, up to 24 characters — not a style",
                    note.id
                )));
            }
        }

        if let Some(region) = note.region {
            // Zero width and height is a **point** — a reader marking a spot
            // rather than an area, the same degenerate case an absent region
            // already is for the whole slot. Three anchors, one field: absent
            // is the slot, zero-sized is a point, sized is an area.
            let within = region.x >= -REGION_SLACK
                && region.y >= -REGION_SLACK
                && region.w >= 0.0
                && region.h >= 0.0
                && region.x + region.w <= 1.0 + REGION_SLACK
                && region.y + region.h <= 1.0 + REGION_SLACK;
            if !within {
                return Err(Invalid::Refused(format!(
                    "note `{}` has a region ({}, {}, {}×{}) that is not inside its slot; a \
                     region is fractions of the slot, so it lies within 0..1 — zero-sized is a \
                     point, negative is nothing",
                    note.id, region.x, region.y, region.w, region.h
                )));
            }
        }
    }
    Ok(())
}

/// A work with nothing written on it yet.
///
/// The route hands this back where `load` answers `None`, so a client that has
/// never seen a document still gets one — and, with it, the `ETag` its first
/// write has to be conditional on.
#[must_use]
pub fn empty(id: &str) -> Notes {
    Notes {
        work: id.to_string(),
        notes: Vec::new(),
    }
}

/// The one serialisation of a notes document.
///
/// Shared by the projection `load` returns, the body of a response, and the
/// `ETag` over that body (`etag_for`, below), so a validator can never be
/// computed over bytes a client was not sent — the failure mode that would
/// make a conditional write silently unconditional.
///
/// # Errors
///
/// Returns an error if serialising `notes` to JSON fails. In practice it
/// cannot: `Notes`, `Note` and `Region` carry no hand-written `Serialize`
/// impl, and the only way `serde_json` returns `Err` from `to_string_pretty`
/// is a non-finite float used as a **map key** — `Region`'s `x`, `y`, `w` and
/// `h` are struct values, never a key, so that path is not reachable here.
/// Confirmed directly against this workspace's `serde_json 1.0.151`:
/// serialising a `Region { x: f64::NAN, y: f64::INFINITY, .. }` as a field
/// returns `Ok`, writing JSON `null`, not `Err`. The `Result` return stays
/// because the signature already commits to it, not because the `Err` arm is
/// reachable.
pub fn to_json(notes: &Notes) -> Result<String, String> {
    serde_json::to_string_pretty(notes).map_err(|e| format!("serialising notes: {e}"))
}

/// What is stored, or `None` where nothing ever was.
///
/// A work with no row in `library`'s `work` table — nothing has ever been
/// written against it — answers `None` without touching `taiyaku_library`'s
/// `note` table at all: `works::find_row` never creates a row, so there is
/// nothing yet for a `work_id` to be. A row that does exist always answers
/// `Some`, even with zero notes on it — `taiyaku_library::state::notes`
/// returns a real, empty `Notes` for a work with no note rows, and that is a
/// different fact from a work nobody has ever touched: a reader who deleted
/// their last note is not the same as a reader who never wrote one, and the
/// route above this one relies on being able to tell them apart (`get_notes`'s
/// doc comment collapses the two deliberately, but that collapsing is its
/// decision to make, not this function's to make for it).
///
/// # Errors
///
/// Returns an error if SQLite fails, or if a stored note has a region with
/// only some of its four columns set — see
/// `taiyaku_library::state::notes`'s own `# Errors`.
pub fn load(library: &Library, work: &Work, user_id: i64) -> Result<Option<Notes>, String> {
    let conn = library.conn();
    let Some(work_id) = works::find_row(&conn, work)? else {
        return Ok(None);
    };
    taiyaku_library::state::notes(&conn, work_id, user_id).map(Some)
}

/// Replace the notes, inside one transaction.
///
/// The library row bridging `work`'s slug to a numeric `work_id` is minted
/// here on first write if none exists yet — see `works::row_id_for_write`.
/// `notes.work` is not written anywhere: nothing in the `note` table carries
/// a work label, and `taiyaku_library::state::notes` reconstructs it on read
/// from `work_id`'s own slug — which, by construction, is `work.id`, so a
/// caller-supplied `notes.work` that differs never survives to be read back
/// differently. `notes::validate_against` already refuses any document where
/// it differs before this is ever called, so nothing here re-checks it.
///
/// # Errors
///
/// Returns an error if SQLite fails — including a `UNIQUE` violation on a
/// note `id` repeated in `notes.notes`, since this module's own
/// `validate_against` is what is relied on to have refused that already.
pub fn store(library: &Library, work: &Work, user_id: i64, notes: &Notes) -> Result<(), String> {
    let conn = library.conn();
    let work_id = works::row_id_for_write(&conn, work)?;
    taiyaku_library::state::put_notes(&conn, work_id, user_id, notes)
}

/// A notes or truth document's `ETag`: a SHA-256 over exactly the bytes being
/// sent.
///
/// Takes the serialised text rather than the document, because both callers
/// already hold it — `to_json` and `truth::to_json` are each the one
/// serialisation of their document, and hashing a second copy built here
/// would be the drift this exists to rule out. Shared by `get_truth`/
/// `put_truth` in `routes.rs` rather than given a `truth_etag` wrapper of its
/// own: the hash has nothing document-specific in it, so a second name for
/// the same bytes would be indirection with no behaviour behind it. Lives
/// here rather than in `routes.rs`: this is where `to_json` is, and the two
/// belong together for the same reason `to_json`'s own doc comment gives.
pub(crate) fn etag_for(json: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let digest = hasher.finalize();
    let hex = digest.iter().fold(String::new(), |mut hex, b| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
        hex
    });
    format!("\"{hex}\"")
}

#[cfg(test)]
mod tests;
