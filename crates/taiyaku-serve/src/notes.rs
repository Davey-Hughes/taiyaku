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
mod tests {
    use super::*;

    fn note(id: &str, slot: u32, region: Option<Region>) -> Note {
        Note {
            id: id.into(),
            edition: "raw".into(),
            slot,
            region,
            color: None,
            body: "why is this page here".into(),
            created: "2026-08-05T10:00:00Z".into(),
            updated: "2026-08-05T10:00:00Z".into(),
        }
    }

    /// The sidecar must not gain a notes field. It is an alignment document,
    /// re-solving rewrites it, and prose in it would be noise in every diff.
    #[test]
    fn notes_are_written_beside_the_sidecar_not_into_it() {
        let work = crate::test_support::work("taiyaku-test-notes-beside");
        let sidecar_before = std::fs::read_to_string(&work.path).unwrap();

        store(
            &work,
            &Notes {
                work: "t".into(),
                notes: vec![note("01J8", 12, None)],
            },
        )
        .unwrap();

        let sidecar_after = std::fs::read_to_string(&work.path).unwrap();
        let beside = notes_path(&work);
        let exists = beside.exists();
        crate::test_support::cleanup("taiyaku-test-notes-beside");
        assert_eq!(sidecar_before, sidecar_after, "the sidecar must not change");
        assert!(exists, "notes belong in {}", beside.display());
    }

    /// A note with a region and one without must both survive the trip, and
    /// the one without must not acquire a `null` region on the way — absent
    /// means the whole slot, and `"region": null` is a third spelling nothing
    /// should have to handle.
    #[test]
    fn a_note_round_trips_with_and_without_a_region() {
        let work = crate::test_support::work("taiyaku-test-notes-round-trip");
        let stored = Notes {
            work: "t".into(),
            notes: vec![
                note(
                    "01J8A",
                    3,
                    Some(Region {
                        x: 0.12,
                        y: 0.34,
                        w: 0.28,
                        h: 0.09,
                    }),
                ),
                note("01J8B", 0, None),
            ],
        };

        store(&work, &stored).unwrap();
        let text = std::fs::read_to_string(notes_path(&work)).unwrap();
        let read_back = load(&work).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-round-trip");
        assert_eq!(read_back, Some(stored));
        assert!(
            !text.contains("null"),
            "a note with no region must omit the field, not write null: {text}"
        );
    }

    /// The bytes on disk are the bytes an `ETag` is computed over and the
    /// bytes a client is sent. Anything else lets a validator describe a
    /// document nobody has, which makes a conditional write unconditional
    /// without saying so.
    #[test]
    fn what_is_stored_is_exactly_what_to_json_produces() {
        let work = crate::test_support::work("taiyaku-test-notes-canonical");
        let notes = Notes {
            work: "t".into(),
            notes: vec![note("01J8", 1, None)],
        };

        store(&work, &notes).unwrap();
        let on_disk = std::fs::read_to_string(notes_path(&work)).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-canonical");
        assert_eq!(on_disk, to_json(&notes).unwrap());
    }

    /// A field this build does not know is refused rather than dropped. The
    /// same rule M5d's `Correction` applies on the wire, applied at rest: a
    /// file written by a later build, or hand-edited with a typo, must not
    /// come back with the reader's own prose silently missing.
    #[test]
    fn a_notes_file_with_an_unknown_field_is_refused_rather_than_silently_dropped() {
        let work = crate::test_support::work("taiyaku-test-notes-unknown-field");
        std::fs::write(
            notes_path(&work),
            r#"{"work":"t","notes":[{"id":"01J8","edition":"raw","slot":0,
               "body":"b","created":"c","updated":"u","colour":"red"}]}"#,
        )
        .unwrap();

        let read_back = load(&work);

        crate::test_support::cleanup("taiyaku-test-notes-unknown-field");
        let message = read_back.expect_err("an unknown field must not parse");
        assert!(
            message.contains("colour"),
            "the refusal has to name the field: {message}"
        );
    }

    /// A colour is a name a client looks up, not a style it pastes into an
    /// attribute — so it is held to the alphabet an edition key is, and the
    /// refusal names what was wrong rather than quietly dropping the field.
    #[test]
    fn a_colour_must_be_a_name_rather_than_a_style() {
        let work = crate::test_support::work("taiyaku-test-notes-colour");
        let coloured = |value: &str| {
            let mut notes = on("raw", 0, None);
            notes.notes[0].color = Some(value.to_string());
            validate(&notes, &work, &empty("t"))
        };

        let plain = coloured("amber");
        let dashed = coloured("deep-sea-blue");
        let css = coloured("red; background: url(x)");
        let shouty = coloured("Amber");
        let empty = coloured("");
        crate::test_support::cleanup("taiyaku-test-notes-colour");

        plain.expect("an ordinary name");
        dashed.expect("dashes are how a two-word name is spelled here");
        assert!(matches!(css, Err(Invalid::Refused(_))), "not a style");
        assert!(
            matches!(shouty, Err(Invalid::Refused(_))),
            "one spelling only"
        );
        assert!(
            matches!(empty, Err(Invalid::Refused(_))),
            "absent, not blank"
        );
    }

    /// **One stale note must not brick a reader's notes.** A re-solve can
    /// rename or drop an edition, or shrink one past a slot a note sits on,
    /// and a client sends the whole document back on every write — so
    /// re-checking an anchor nobody touched refuses every future write on that
    /// work, including the write that would have removed the stale note.
    /// Measured before the fix: adding an ordinary note beside a stale one
    /// answered 404.
    #[test]
    fn a_stale_anchor_already_stored_does_not_refuse_the_writes_that_follow_it() {
        let work = crate::test_support::work("taiyaku-test-notes-stale");
        let mut stale = on("gone-in-a-re-solve", 3, None);
        stale.notes[0].id = "01STALE".into();

        // As it stands today, refused — this write is what introduces it.
        let introducing = validate(&stale, &work, &empty("t"));

        // Already stored, and now a reader adds an ordinary note beside it.
        let mut both = stale.clone();
        both.notes.push(Note {
            id: "01NEW".into(),
            edition: "raw".into(),
            slot: 1,
            region: None,
            color: None,
            body: "an ordinary note".into(),
            created: "2026-08-05T10:00:00Z".into(),
            updated: "2026-08-05T10:00:00Z".into(),
        });
        let alongside = validate(&both, &work, &stale);

        // And removing the stale one has to work, which is how a reader gets
        // out of it.
        let removing = validate(&empty("t"), &work, &stale);

        crate::test_support::cleanup("taiyaku-test-notes-stale");
        assert!(
            matches!(introducing, Err(Invalid::Missing(_))),
            "a new note on an edition this work does not have is still refused"
        );
        alongside.expect("a stale note already stored must not refuse the notes written beside it");
        removing.expect("and must not refuse its own removal");
    }

    /// The other half: grandfathering is for anchors that have not moved, not
    /// for notes that carry a stale id. Dragging a stale note onto a slot of
    /// an edition that still does not exist is this write's doing.
    #[test]
    fn moving_a_stale_note_is_refused_even_though_it_was_already_stored() {
        let work = crate::test_support::work("taiyaku-test-notes-stale-moved");
        let mut stored = on("gone-in-a-re-solve", 3, None);
        stored.notes[0].id = "01STALE".into();
        let mut moved = stored.clone();
        moved.notes[0].slot = 4;

        let refused = validate(&moved, &work, &stored);
        crate::test_support::cleanup("taiyaku-test-notes-stale-moved");
        assert!(
            matches!(refused, Err(Invalid::Missing(_))),
            "moving a note onto a bad anchor is this write's doing"
        );
    }

    /// Grandfathering covers the anchor and nothing else: a colour cannot be
    /// made stale by a re-solve, so a bad one is refused however long the note
    /// has been stored.
    #[test]
    fn a_grandfathered_note_still_has_its_colour_checked() {
        let work = crate::test_support::work("taiyaku-test-notes-stale-colour");
        let stored = on("raw", 0, None);
        let mut recoloured = stored.clone();
        recoloured.notes[0].color = Some("NOT A NAME".into());

        let refused = validate(&recoloured, &work, &stored);
        crate::test_support::cleanup("taiyaku-test-notes-stale-colour");
        assert!(matches!(refused, Err(Invalid::Refused(_))));
    }

    /// Nothing recorded is `None`, not an empty document — the file's
    /// existence is a fact this layer reports honestly. Collapsing the two is
    /// the *route*'s decision, made there and explained there.
    #[test]
    fn a_work_with_no_notes_file_loads_as_nothing_recorded() {
        let work = crate::test_support::work("taiyaku-test-notes-absent");
        let loaded = load(&work);
        crate::test_support::cleanup("taiyaku-test-notes-absent");
        assert_eq!(loaded.unwrap(), None);
    }

    fn on(edition: &str, slot: u32, region: Option<Region>) -> Notes {
        Notes {
            work: "t".into(),
            notes: vec![Note {
                id: "01J8".into(),
                edition: edition.into(),
                slot,
                region,
                color: None,
                body: "b".into(),
                created: "2026-08-05T10:00:00Z".into(),
                updated: "2026-08-05T10:00:00Z".into(),
            }],
        }
    }

    /// Every refusal, and the one acceptance that proves the guard is not
    /// spelled "refuse everything".
    #[test]
    fn a_note_must_name_a_slot_of_an_edition_this_work_has() {
        let work = crate::test_support::work("taiyaku-test-notes-validate");
        // The fixture is one edition, `raw`, of four pages and no spreads —
        // four slots, so slot 3 is the last and slot 4 is one past the end.
        let accepted = validate(&on("raw", 3, None), &work, &empty("t"));
        let unknown_edition = validate(&on("nope", 0, None), &work, &empty("t"));
        let past_the_end = validate(&on("raw", 4, None), &work, &empty("t"));
        crate::test_support::cleanup("taiyaku-test-notes-validate");

        accepted.expect("the last slot of a real edition is a real anchor");
        let message = match unknown_edition {
            Err(Invalid::Missing(m)) => m,
            other => panic!("an unknown edition is a 404's shape: {other:?}"),
        };
        assert!(message.contains("nope"), "name it: {message}");
        let message = match past_the_end {
            Err(Invalid::Missing(m)) => m,
            other => panic!("a slot past the end is a 404's shape: {other:?}"),
        };
        assert!(
            message.contains('4') && message.contains("slot"),
            "say how many slots there are: {message}"
        );
    }

    /// Counted in slots, not pages — a joined spread is one page and two
    /// slots, so checking against `total_pages()` would refuse a real anchor
    /// on the last slot of every spread-joining edition. O3 learned this on
    /// progress; a note is anchored the same way.
    #[test]
    fn the_range_check_counts_slots_not_pages() {
        let built = crate::test_support::work("taiyaku-test-notes-spread-slot");
        // Rewrite the fixture's sidecar so page 0 is a joined spread: 4 pages,
        // 5 slots, and slot 4 must be accepted.
        let json = std::fs::read_to_string(&built.path)
            .unwrap()
            .replace(r#""spreads": []"#, r#""spreads": [0]"#);
        let work = crate::works::Work::new(
            built.id.clone(),
            built.path.clone(),
            taiyaku_core::Sidecar::from_json(&json).unwrap(),
        );

        let last = validate(&on("raw", 4, None), &work, &empty("t"));
        let past = validate(&on("raw", 5, None), &work, &empty("t"));

        crate::test_support::cleanup("taiyaku-test-notes-spread-slot");
        last.expect("4 pages with page 0 joined is 5 slots, and slot 4 is real");
        assert!(past.is_err(), "slot 5 is past the end even so");
    }

    /// A region outside the slot would draw a marker off the page. A
    /// zero-sized one is not nothing — it is a point, the same degenerate case
    /// an absent region already is for the whole slot.
    #[test]
    fn a_region_must_lie_within_the_slot_and_have_a_size() {
        let work = crate::test_support::work("taiyaku-test-notes-region");
        let ok = Region {
            x: 0.1,
            y: 0.1,
            w: 0.5,
            h: 0.5,
        };
        let full = Region {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let over = Region {
            x: 0.8,
            y: 0.1,
            w: 0.5,
            h: 0.2,
        };
        let point = Region {
            x: 0.1,
            y: 0.1,
            w: 0.0,
            h: 0.0,
        };
        let negative = Region {
            x: -0.1,
            y: 0.1,
            w: 0.2,
            h: 0.2,
        };

        let results = [
            validate(&on("raw", 0, Some(ok)), &work, &empty("t")),
            validate(&on("raw", 0, Some(full)), &work, &empty("t")),
            validate(&on("raw", 0, Some(over)), &work, &empty("t")),
            validate(&on("raw", 0, Some(point)), &work, &empty("t")),
            validate(&on("raw", 0, Some(negative)), &work, &empty("t")),
        ];
        crate::test_support::cleanup("taiyaku-test-notes-region");

        results[0].as_ref().expect("an ordinary region");
        results[1]
            .as_ref()
            .expect("the whole slot, stated explicitly");
        assert!(
            matches!(results[2], Err(Invalid::Refused(_))),
            "runs off the edge"
        );
        results[3]
            .as_ref()
            .expect("zero-sized is a point — a reader marking a spot, not an area");
        assert!(
            matches!(results[4], Err(Invalid::Refused(_))),
            "negative origin"
        );
    }

    /// Two notes sharing an id makes every later edit ambiguous — which of the
    /// two did the reader mean? — so it is refused at the boundary rather than
    /// discovered by a client that edits the wrong one.
    #[test]
    fn two_notes_cannot_share_an_id() {
        let work = crate::test_support::work("taiyaku-test-notes-dupe-id");
        let mut notes = on("raw", 0, None);
        notes.notes.push(notes.notes[0].clone());

        let refused = validate(&notes, &work, &empty("t"));
        crate::test_support::cleanup("taiyaku-test-notes-dupe-id");

        let message = match refused {
            Err(Invalid::Refused(m)) => m,
            other => panic!("a duplicate id is malformed, not missing: {other:?}"),
        };
        assert!(message.contains("01J8"), "name the id: {message}");
    }

    /// The document says which work it belongs to, and a mismatch means a
    /// client is about to write one work's notes over another's — the one
    /// mistake here that destroys notes rather than refusing them.
    #[test]
    fn a_document_naming_a_different_work_is_refused() {
        let work = crate::test_support::work("taiyaku-test-notes-wrong-work");
        let mut notes = on("raw", 0, None);
        notes.work = "some-other-work".into();

        let refused = validate(&notes, &work, &empty("t"));
        crate::test_support::cleanup("taiyaku-test-notes-wrong-work");

        let message = match refused {
            Err(Invalid::Refused(m)) => m,
            other => panic!("{other:?}"),
        };
        assert!(
            message.contains("some-other-work") && message.contains('t'),
            "name both, or nobody can tell which is wrong: {message}"
        );
    }

    /// `empty` is what the route hands a client that has never written a note,
    /// and it must be a document about *this* work — a client writes it
    /// straight back, and `validate` refuses a document naming another work.
    #[test]
    fn an_empty_document_names_the_work_it_is_for() {
        assert_eq!(
            empty("go-toubun-v01"),
            Notes {
                work: "go-toubun-v01".into(),
                notes: Vec::new()
            }
        );
    }

    /// The same rule on the other axis. `a_region_must_lie_within_the_slot_and_have_a_size`
    /// overflows in x only — `x: 0.8, w: 0.5` — so the `y + h` half of the
    /// bound was never asked, and both arithmetic mutations of it survived. A
    /// marker can run off the bottom of a page as easily as off the side.
    #[test]
    fn a_region_that_runs_off_the_bottom_is_refused_like_one_that_runs_off_the_side() {
        let work = crate::test_support::work("taiyaku-test-notes-region-y");
        let over_y = Region {
            x: 0.1,
            y: 0.8,
            w: 0.2,
            h: 0.5,
        };
        let result = validate(&on("raw", 0, Some(over_y)), &work, &empty("t"));
        crate::test_support::cleanup("taiyaku-test-notes-region-y");

        assert!(
            matches!(result, Err(Invalid::Refused(_))),
            "y + h is 1.3, which is off the page: {result:?}"
        );
    }

    /// `NotFound` means "nothing recorded"; every other error means the notes
    /// could not be read and must not be reported as an empty document.
    ///
    /// The guard `Err(e) if e.kind() == NotFound` could be replaced by `true`
    /// unnoticed, because every test either wrote a file or wrote none — and
    /// a missing file and an unreadable one are the same to a test that only
    /// ever checks for `None`. A reader whose notes file had become
    /// unreadable would be shown a blank margin and, on the next write, have
    /// the file replaced with one note.
    #[test]
    fn an_unreadable_notes_file_is_an_error_rather_than_nothing_recorded() {
        let work = crate::test_support::work("taiyaku-test-notes-unreadable");
        // A directory where a file belongs: readable as an entry, not as text.
        std::fs::create_dir_all(notes_path(&work)).unwrap();

        let result = load(&work);

        crate::test_support::cleanup("taiyaku-test-notes-unreadable");
        assert!(
            result.is_err(),
            "must not read as `no notes yet`: {result:?}"
        );
    }
}
