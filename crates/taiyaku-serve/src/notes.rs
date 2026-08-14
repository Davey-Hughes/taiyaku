//! What a reader wrote down, kept beside the sidecar rather than inside it.
//!
//! Notes are content, so they are server-side for the same reason progress is:
//! they have to survive a different machine, and a browser profile is neither
//! portable nor exportable. They are a sibling file rather than a field in
//! `taiyaku.json` because that document describes correspondence and must diff
//! meaningfully in git — human prose would be noise in it, and re-solving
//! rewrites it, which is the hazard O13 already records for hand corrections.
//!
//! A note is anchored to a **slot**, not a page: a note on the left half of a
//! joined spread must not silently become a note on the whole spread. Its
//! `region`, when it has one, is in fractions of that slot, so a re-scan or the
//! same edition served at another resolution does not move the marker.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::works::Work;

/// A rectangle on one slot, in fractions of that slot's own width and height.
///
/// Fractions rather than pixels, and the whole marker arithmetic is scale-free
/// as a result — a tab or a tint tracks correctly at any fit scale with no
/// extra work.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One note.
///
/// `region` is optional and absent means the whole slot — a degenerate case
/// rather than a second kind of note, so nothing downstream needs two code
/// paths. `created`/`updated` are RFC 3339 strings minted by the client: this
/// server has no clock in the format, and a note written offline should carry
/// the time it was written rather than the time it reached a server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: String,
    pub edition: String,
    pub slot: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    /// Which colour a reader gave this note, as a name rather than as pixels.
    ///
    /// A **token**, never CSS: `taiyaku.json` carries correspondence and this
    /// file carries what a reader wrote, and neither is a stylesheet. What
    /// `amber` looks like is the reader's business, and a second reader — a
    /// native shell, a printout — is entitled to a different answer.
    ///
    /// Absent means the default, so every note written before colours existed
    /// stays valid and reads as it always did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub body: String,
    pub created: String,
    pub updated: String,
}

/// Every note on one work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
    pub work: String,
    pub notes: Vec<Note>,
}

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
/// work, forever, including the writes that would have removed it. Measured
/// before it was fixed: one stale note, and adding an ordinary new note beside
/// it answered `404 note 01STALE names the edition raw-mono-OLD`.
///
/// So a stale note survives as something a reader can see (the review list
/// already marks a note it cannot reach from here) and delete, rather than as
/// something that quietly bricks their notes. What is still refused is a
/// *new* bad anchor, and moving an existing note onto one — the cases where
/// this write is what makes the anchor wrong.
///
/// # Errors
///
/// Returns `Invalid::Refused` (a 400's shape) if `notes.work` does not match
/// `work.id`, if two notes in the document share an `id`, if a note's colour
/// is not lowercase ASCII letters, digits and dashes of at most 24
/// characters, or if a note's region strays outside `0..1` of its slot by
/// more than floating-point slack.
///
/// Returns `Invalid::Missing` (a 404's shape) if a note's anchor — its
/// `edition` and `slot` — is new or has moved, and names an edition `work`
/// does not have, or a slot at or past that edition's slot count. An anchor
/// already present, unmoved, in `stored` is exempt from that edition/slot
/// check, which is what keeps a re-solve that invalidates an untouched
/// note's anchor from refusing every write on the work thereafter.
pub fn validate(notes: &Notes, work: &Work, stored: &Notes) -> Result<(), Invalid> {
    if notes.work != work.id {
        return Err(Invalid::Refused(format!(
            "these notes name the work `{}`, but this is `{}`",
            notes.work, work.id
        )));
    }

    let sidecar = work.sidecar();
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
                    note.id, note.edition, work.id
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

/// Where a work's notes file sits: beside its sidecar, same stem.
pub fn notes_path(work: &Work) -> PathBuf {
    work.sibling("notes.json")
}

/// A work with nothing written on it yet.
///
/// The route hands this back where no file exists, so a client that has never
/// seen a document still gets one — and, with it, the `ETag` its first write
/// has to be conditional on.
#[must_use]
pub fn empty(id: &str) -> Notes {
    Notes {
        work: id.to_string(),
        notes: Vec::new(),
    }
}

/// The one serialisation of a notes document.
///
/// Shared by the file on disk, the body of a response, and the `ETag` over
/// that body, so a validator can never be computed over bytes a client was not
/// sent — the failure mode that would make a conditional write silently
/// unconditional.
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
/// # Errors
///
/// Returns an error if the notes file exists but its contents are not valid
/// JSON for `Notes` — including a field this build does not recognise, since
/// `Notes` and `Note` both deny unknown fields — or if reading it fails for
/// any reason other than the file simply not existing, such as a permission
/// refusal or a directory sitting where the file should. A missing file is
/// not an error: it is `Ok(None)`.
pub fn load(work: &Work) -> Result<Option<Notes>, String> {
    let path = notes_path(work);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Write the notes, atomically.
///
/// `progress::store`'s reasoning applies unchanged and with one addition: a
/// notes file holds prose a person typed, so a crash mid-write truncating it
/// destroys work that cannot be recomputed from anything, where a truncated
/// position merely loses a page number.
///
/// # Errors
///
/// Returns an error if `to_json` fails to serialise `notes` (see its own
/// `# Errors`), or if writing the result to `notes_path(work)` fails — see
/// `atomic::write` for the two ways that can happen.
pub fn store(work: &Work, notes: &Notes) -> Result<(), String> {
    let path = notes_path(work);
    let text = to_json(notes)?;
    crate::atomic::write(&path, text.as_bytes(), "json")
}

#[cfg(test)]
mod tests;
