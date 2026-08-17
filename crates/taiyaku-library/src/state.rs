//! Reading progress, notes, the labelling truth document and a work's
//! alignments, as rows.
//!
//! `Progress`, `Notes` and `Note` (with notes' own `Region`) live at this
//! crate's root — moved down from `taiyaku-serve`, which cannot be the thing
//! this store depends on. `TruthDoc` needs no move: it is already in
//! `taiyaku-core`, which this crate depends on.
//!
//! Three merge rules, deliberately different (design §4.3):
//!
//! | Document | Rule |
//! | --- | --- |
//! | progress | later `updated` wins |
//! | notes | union by ULID; per note, later `updated` wins |
//! | truth | whole document, later wins |
//!
//! Only notes' rule has structure worth a pure function to test —
//! [`merge_notes`]. Progress is one row with nothing to conflict over, and
//! truth's rule is whole-document last-write-wins because `TruthRegion` has no
//! identity to merge on; both are simple enough to apply inline wherever a
//! caller compares two `updated` timestamps, which is not this module's job.
//!
//! [`put_alignments`] is the fourth writer here and belongs to none of those
//! three rules — see its own doc comment for why the document it writes is a
//! different kind of thing, and where its merge rule actually lives.
//!
//! **File I/O, the `ETag` helper, the `Invalid` enum and route-facing
//! validation all stay in `taiyaku-serve`.** This module is rows in, rows out.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use taiyaku_core::{Axis, Kind, RegionMember, Sidecar, TruthDoc, TruthEdition, TruthRegion};

use crate::{Note, Notes, Progress, Region};

// --- progress ----------------------------------------------------------

/// What was recorded for one reader on one work, or `None` when nothing ever
/// was.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn progress(conn: &Connection, work_id: i64, user_id: i64) -> Result<Option<Progress>, String> {
    conn.query_row(
        "SELECT edition, slot FROM progress WHERE work_id = ?1 AND user_id = ?2",
        params![work_id, user_id],
        |row| {
            Ok(Progress {
                edition: row.get(0)?,
                slot: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("reading progress for work {work_id}, user {user_id}: {e}"))
}

/// Record where a reader got to.
///
/// An upsert on `(user_id, work_id)`: it does not touch `completed_at` or
/// `read_count`, which nothing in this module's interface manages.
///
/// **`ON CONFLICT … DO UPDATE` naming three columns rather than
/// `INSERT OR REPLACE`, and that is the whole of the promise above.** A
/// REPLACE would clear both columns on every page turn — reading history the
/// schema calls "behavioural data with NOTHING to backfill" — and the two
/// statements are otherwise interchangeable here, `progress` being
/// `WITHOUT ROWID` on `(user_id, work_id)`. Pinned by
/// `put_progress_leaves_completed_at_and_read_count_alone` in this module's
/// `tests.rs`, which is red under exactly that swap.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn put_progress(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    progress: &Progress,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(user_id, work_id) DO UPDATE SET
           edition = excluded.edition,
           slot = excluded.slot,
           updated = excluded.updated",
        params![user_id, work_id, progress.edition, progress.slot],
    )
    .map_err(|e| format!("writing progress for work {work_id}, user {user_id}: {e}"))?;
    Ok(())
}

// --- notes ---------------------------------------------------------------

/// One `note` row before its four region columns have been reassembled into
/// an `Option<Region>` — named to keep `query_map`'s closure under
/// `clippy::type_complexity`, the way `project.rs`'s `RunColumns` does.
struct NoteRow {
    id: String,
    edition: String,
    slot: u32,
    rx: Option<f64>,
    ry: Option<f64>,
    rw: Option<f64>,
    rh: Option<f64>,
    color: Option<String>,
    body: String,
    created: String,
    updated: String,
}

/// Every note a reader holds on a work.
///
/// `work` is filled from `work.slug` — the same shape `taiyaku-serve`'s own
/// work id is slugified to (`import.rs`'s `unique_slug`) — since nothing in
/// the `note` table carries a work label of its own.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if SQLite fails, or if a
/// stored note has a region with some but not all of its four columns set —
/// a state `put_notes` cannot write and that must not be read back as either
/// "no region" or a silently-guessed one.
pub fn notes(conn: &Connection, work_id: i64, user_id: i64) -> Result<Notes, String> {
    let work = work_slug(conn, work_id)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, edition, slot, rx, ry, rw, rh, color, body, created, updated
             FROM note WHERE work_id = ?1 AND user_id = ?2 ORDER BY id",
        )
        .map_err(|e| format!("preparing notes: {e}"))?;
    let rows = stmt
        .query_map(params![work_id, user_id], |row| {
            Ok(NoteRow {
                id: row.get(0)?,
                edition: row.get(1)?,
                slot: row.get(2)?,
                rx: row.get(3)?,
                ry: row.get(4)?,
                rw: row.get(5)?,
                rh: row.get(6)?,
                color: row.get(7)?,
                body: row.get(8)?,
                created: row.get(9)?,
                updated: row.get(10)?,
            })
        })
        .map_err(|e| format!("querying notes: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading notes: {e}"))?;

    let notes = rows
        .into_iter()
        .map(note_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Notes { work, notes })
}

fn note_from_row(row: NoteRow) -> Result<Note, String> {
    let region = region_from_columns(&row.id, row.rx, row.ry, row.rw, row.rh)?;
    Ok(Note {
        id: row.id,
        edition: row.edition,
        slot: row.slot,
        region,
        color: row.color,
        body: row.body,
        created: row.created,
        updated: row.updated,
    })
}

/// Four nullable columns back into `Option<Region>`, refusing the shape
/// `put_notes` never writes: some but not all four set. All four NULL is no
/// region; all four set is a region. Nothing else is representable.
fn region_from_columns(
    note_id: &str,
    rx: Option<f64>,
    ry: Option<f64>,
    rw: Option<f64>,
    rh: Option<f64>,
) -> Result<Option<Region>, String> {
    match (rx, ry, rw, rh) {
        (None, None, None, None) => Ok(None),
        (Some(x), Some(y), Some(w), Some(h)) => Ok(Some(Region { x, y, w, h })),
        _ => Err(format!(
            "note `{note_id}` has a region with only some of rx, ry, rw, rh set \
             (rx={rx:?}, ry={ry:?}, rw={rw:?}, rh={rh:?}); a region is all four or none"
        )),
    }
}

fn work_slug(conn: &Connection, work_id: i64) -> Result<String, String> {
    conn.query_row("SELECT slug FROM work WHERE id = ?1", [work_id], |row| {
        row.get(0)
    })
    .map_err(|e| format!("reading work {work_id}: {e}"))
}

/// Replace every note a reader holds on a work with `doc.notes`.
///
/// A client sends the whole document back on every write (`notes.rs`'s
/// `validate` doc comment), so the row-store equivalent of the old file's
/// `store` — which replaced the whole file — is delete-then-reinsert, inside
/// one transaction so a failure partway through leaves the prior set intact
/// rather than half-replaced. `doc.work` is not written anywhere: nothing in
/// the `note` table carries a work label, and `work_id` already says which
/// work this is.
///
/// # Errors
///
/// Returns an error if SQLite fails — including a `UNIQUE` violation on
/// `note.id` if `doc.notes` repeats an id, since this module does not
/// duplicate `notes.rs`'s own check for that.
pub fn put_notes(conn: &Connection, work_id: i64, user_id: i64, doc: &Notes) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting notes transaction: {e}"))?;
    tx.execute(
        "DELETE FROM note WHERE work_id = ?1 AND user_id = ?2",
        params![work_id, user_id],
    )
    .map_err(|e| format!("clearing notes for work {work_id}, user {user_id}: {e}"))?;

    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO note
                   (id, user_id, work_id, edition, slot, rx, ry, rw, rh, color, body, created, updated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )
            .map_err(|e| format!("preparing note insert: {e}"))?;
        for note in &doc.notes {
            let (rx, ry, rw, rh) = match note.region {
                Some(r) => (Some(r.x), Some(r.y), Some(r.w), Some(r.h)),
                None => (None, None, None, None),
            };
            stmt.execute(params![
                note.id,
                user_id,
                work_id,
                note.edition,
                note.slot,
                rx,
                ry,
                rw,
                rh,
                note.color,
                note.body,
                note.created,
                note.updated,
            ])
            .map_err(|e| format!("writing note `{}`: {e}", note.id))?;
        }
    }

    tx.commit().map_err(|e| format!("committing notes: {e}"))
}

/// The rule from §4.3: union by ULID, and per note the later `updated` wins.
///
/// A pure function over two documents, deliberately: the rule is the part
/// worth testing, and this way it is testable with no connection at all. Ties
/// (`held.updated == incoming.updated`) keep `held`, arbitrarily but
/// deterministically. `merged.work` is `held.work`.
#[must_use]
pub fn merge_notes(held: &Notes, incoming: &Notes) -> Notes {
    let mut by_id: BTreeMap<String, Note> = BTreeMap::new();
    for note in &held.notes {
        by_id.insert(note.id.clone(), note.clone());
    }
    for note in &incoming.notes {
        let held_is_newer = by_id
            .get(note.id.as_str())
            .is_some_and(|existing| existing.updated >= note.updated);
        if !held_is_newer {
            by_id.insert(note.id.clone(), note.clone());
        }
    }
    Notes {
        work: held.work.clone(),
        notes: by_id.into_values().collect(),
    }
}

// --- truth -----------------------------------------------------------------

/// One `truth_box` row before its typed columns (`axis`, `kind`) have been
/// parsed back — the same reason `NoteRow` exists.
struct TruthBoxRow {
    edition: String,
    page: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    axis: Option<String>,
    kind: String,
    ignore: bool,
    text: Option<String>,
}

/// The labelling truth document for a work, across every edition anything has
/// been labelled on, or `None` if nothing has.
///
/// **`TruthEdition::members` is stored, and comes back exactly as it was
/// written.** It is the record of which archives the boxes were drawn
/// against, and `TruthEdition::is_stale_for` compares it against the digests
/// of the archives in hand now — so it is the one field here that must *not*
/// be a projection of the present. An earlier version of this function
/// re-derived it from whatever `member` rows existed at read time, which made
/// that comparison compare a list against itself: `is_stale_for` could never
/// report stale again, and an export after a reimport wrote a
/// `taiyaku.truth.json` asserting provenance that was false, over the
/// reader's only copy (whole-branch review 2, C2). The doc comment then here
/// named a precondition — "called before a reimport overwrites `member`" —
/// that no caller satisfied or could satisfy. `truth_member` holds it now.
/// `taiyaku_truth` is still the current [`taiyaku_core::TRUTH_VERSION`],
/// never a stored value: that one really is a property of this build.
///
/// **An edition key survives with no pages at all**, if that is what was
/// written — `pages` is seeded from `truth_page` *and* `truth_member`, so an
/// edition whose provenance was recorded before any page was opened comes
/// back with its members and an empty `pages`, rather than vanishing. `None`
/// is reserved for a work with no truth rows of either kind.
///
/// **A page with zero boxes still comes back as a present, empty key.**
/// `pages` is seeded from `truth_page` — one row per page anyone has looked
/// at, written whether or not it held a box — and boxes are then attached
/// from `truth_box`. `TruthEdition::pages`'s own doc comment calls the
/// absent-vs-empty distinction the labelling progress tracker; seeding from
/// `truth_page` rather than from `truth_box` alone is what lets a page
/// examined and found empty round-trip as exactly that, not as "never
/// opened".
///
/// **Box order within a page is `truth_box.id`, stated rather than inherited.**
/// `put_truth` inserts a page's boxes in document order, so rowid order *is*
/// that order — but a query with no `ORDER BY` returns whatever plan SQLite
/// picks, and today it picks a table scan only because nothing indexes
/// `truth_box.work_id`. Add such an index — for a `work_id` lookup on a large
/// library, the obvious optimisation — and the same query starts returning
/// index order instead, silently reordering every box a person drew, on a
/// commit whose subject line is about speed. `ORDER BY id` costs nothing on the
/// plan that exists (it is already the scan order) and is what makes the
/// property survive the plan changing.
///
/// # Errors
///
/// Returns an error if SQLite fails, or if a stored row's `axis` or `kind`
/// is not one of the values `put_truth` ever writes.
pub fn truth(conn: &Connection, work_id: i64) -> Result<Option<TruthDoc>, String> {
    let mut page_stmt = conn
        .prepare("SELECT edition, page FROM truth_page WHERE work_id = ?1")
        .map_err(|e| format!("preparing truth pages: {e}"))?;
    let page_rows = page_stmt
        .query_map([work_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(|e| format!("querying truth pages: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading truth pages: {e}"))?;

    let members_by_edition = truth_members(conn, work_id)?;

    if page_rows.is_empty() && members_by_edition.is_empty() {
        return Ok(None);
    }

    let mut pages_by_edition: BTreeMap<String, BTreeMap<u32, Vec<TruthRegion>>> = BTreeMap::new();
    for edition in members_by_edition.keys() {
        pages_by_edition.entry(edition.clone()).or_default();
    }
    for (edition, page) in page_rows {
        pages_by_edition
            .entry(edition)
            .or_default()
            .entry(page)
            .or_default();
    }

    let mut box_stmt = conn
        .prepare(
            "SELECT edition, page, x, y, w, h, axis, kind, ignore, text
             FROM truth_box WHERE work_id = ?1 ORDER BY id",
        )
        .map_err(|e| format!("preparing truth boxes: {e}"))?;
    let box_rows = box_stmt
        .query_map([work_id], |row| {
            Ok(TruthBoxRow {
                edition: row.get(0)?,
                page: row.get(1)?,
                x: row.get(2)?,
                y: row.get(3)?,
                w: row.get(4)?,
                h: row.get(5)?,
                axis: row.get(6)?,
                kind: row.get(7)?,
                ignore: row.get(8)?,
                text: row.get(9)?,
            })
        })
        .map_err(|e| format!("querying truth boxes: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading truth boxes: {e}"))?;

    for row in box_rows {
        let region = truth_region_from_row(&row)?;
        pages_by_edition
            .entry(row.edition)
            .or_default()
            .entry(row.page)
            .or_default()
            .push(region);
    }

    let mut editions = BTreeMap::new();
    let mut members_by_edition = members_by_edition;
    for (edition, pages) in pages_by_edition {
        let members = members_by_edition.remove(&edition).unwrap_or_default();
        editions.insert(edition, TruthEdition { members, pages });
    }

    Ok(Some(TruthDoc {
        taiyaku_truth: taiyaku_core::TRUTH_VERSION,
        editions,
    }))
}

fn truth_region_from_row(row: &TruthBoxRow) -> Result<TruthRegion, String> {
    let axis = row.axis.as_deref().map(parse_axis).transpose()?;
    let kind = parse_kind(&row.kind)?;
    Ok(TruthRegion {
        x: row.x,
        y: row.y,
        w: row.w,
        h: row.h,
        axis,
        kind,
        ignore: row.ignore,
        text: row.text.clone(),
    })
}

/// The provenance recorded with the document, per edition, in the order it
/// was written — `ORDER BY ordinal` because member order *is* page order, and
/// `is_stale_for` compares position by position.
///
/// Deliberately not a lookup against `edition`/`member`: these digests exist
/// precisely so they can outlive the rows they were copied from. An edition
/// the sidecar no longer holds keeps the provenance its labels were drawn
/// against, which is what lets `is_stale_for` say so.
fn truth_members(
    conn: &Connection,
    work_id: i64,
) -> Result<BTreeMap<String, Vec<RegionMember>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT edition, sha256 FROM truth_member WHERE work_id = ?1 ORDER BY edition, ordinal",
        )
        .map_err(|e| format!("preparing truth members: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("querying truth members: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading truth members: {e}"))?;

    let mut by_edition: BTreeMap<String, Vec<RegionMember>> = BTreeMap::new();
    for (edition, sha256) in rows {
        by_edition
            .entry(edition)
            .or_default()
            .push(RegionMember { sha256 });
    }
    Ok(by_edition)
}

/// Replace the whole truth document for a work: every page anyone has looked
/// at, and every box on it, on every edition.
///
/// Delete-then-reinsert inside one transaction, for `put_notes`'s reason.
/// `doc.taiyaku_truth` is not written anywhere — see `truth`'s doc comment.
/// A `truth_page` row is written for **every** key in `labelled.pages`,
/// including one whose box list is empty — that row is the only record that
/// the page was examined at all, and skipping it here would reintroduce the
/// gap `truth_page` exists to close.
///
/// `labelled.members` is written to `truth_member` **as given**, for every
/// edition in the document, including one with no pages: it is the record of
/// which archives these boxes were drawn against, and the only copy of it
/// (whole-branch review 2, C2). The labelling client stamps it fresh from
/// the sidecar it is reading on every write (`web/src/truthWrite.ts`'s
/// `withPage`), so what arrives here is what the labeller had in front of
/// them.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn put_truth(conn: &Connection, work_id: i64, doc: &TruthDoc) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting truth transaction: {e}"))?;
    tx.execute("DELETE FROM truth_box WHERE work_id = ?1", [work_id])
        .map_err(|e| format!("clearing truth boxes for work {work_id}: {e}"))?;
    tx.execute("DELETE FROM truth_page WHERE work_id = ?1", [work_id])
        .map_err(|e| format!("clearing truth pages for work {work_id}: {e}"))?;
    tx.execute("DELETE FROM truth_member WHERE work_id = ?1", [work_id])
        .map_err(|e| format!("clearing truth members for work {work_id}: {e}"))?;

    {
        let mut member_stmt = tx
            .prepare(
                "INSERT INTO truth_member (work_id, edition, ordinal, sha256)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(|e| format!("preparing truth-member insert: {e}"))?;
        let mut page_stmt = tx
            .prepare("INSERT INTO truth_page (work_id, edition, page) VALUES (?1, ?2, ?3)")
            .map_err(|e| format!("preparing truth-page insert: {e}"))?;
        let mut box_stmt = tx
            .prepare(
                "INSERT INTO truth_box (work_id, edition, page, x, y, w, h, axis, kind, ignore, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .map_err(|e| format!("preparing truth-box insert: {e}"))?;
        for (edition, labelled) in &doc.editions {
            // `(0i64..)` rather than `enumerate`: the column is an INTEGER
            // and rusqlite binds no `usize`, so this is the counter's type
            // rather than a conversion with an unreachable failure arm.
            for (ordinal, member) in (0i64..).zip(&labelled.members) {
                member_stmt
                    .execute(params![work_id, edition, ordinal, member.sha256])
                    .map_err(|e| {
                        format!("writing member {ordinal} of `{edition}`'s provenance: {e}")
                    })?;
            }
            for (page, regions) in &labelled.pages {
                page_stmt
                    .execute(params![work_id, edition, page])
                    .map_err(|e| format!("writing truth page `{edition}` page {page}: {e}"))?;
                for region in regions {
                    box_stmt
                        .execute(params![
                            work_id,
                            edition,
                            page,
                            region.x,
                            region.y,
                            region.w,
                            region.h,
                            region.axis.map(axis_text),
                            kind_text(region.kind),
                            region.ignore,
                            region.text,
                        ])
                        .map_err(|e| {
                            format!("writing a truth box on `{edition}` page {page}: {e}")
                        })?;
                }
            }
        }
    }

    tx.commit().map_err(|e| format!("committing truth: {e}"))
}

// --- alignments ------------------------------------------------------------

/// Replace every alignment and run a work holds with what `doc` carries.
///
/// **The fourth per-work document, and the odd one out.** Progress, notes and
/// truth are state a reader accumulates and the rows are their only home;
/// alignments are part of `taiyaku.json` itself, so they have a file *and*
/// rows, written by [`crate::import::sidecar`] on the way in and read back by
/// [`crate::project::sidecar`] on the way out. This function exists because
/// one route writes them outside an import: `taiyaku-serve`'s
/// `patch_alignments`, a reader dragging a filmstrip tile to say two pages
/// belong together. That handler wrote the file and its own in-memory copy and
/// nothing else, so until the next `serve` start re-imported the file, the
/// database `serve` answers every request from disagreed with it (O43).
///
/// **A whole-document replace, `put_truth`'s rule** — later wins, with no
/// merge. That is right *here* and only here: the caller is the process that
/// holds the document, applying one correction to the copy it just validated
/// an `If-Match` against, so there is no second writer for a merge rule to
/// arbitrate between. The rule that decides what a *re-solve* does to a hand
/// correction is `Sidecar::merge_spoke`, applied by [`crate::import::reimport`]
/// before it ever reaches these rows; nothing here second-guesses it.
///
/// Only `alignment` and `run` are touched. A correction cannot add, remove or
/// rename an edition (`Sidecar::correct` writes `self.alignments` and nothing
/// else), so replacing the edition rows as well would be rewriting what was
/// not asked about.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn put_alignments(conn: &Connection, work_id: i64, doc: &Sidecar) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting alignments transaction: {e}"))?;
    crate::import::replace_alignments(&tx, work_id, doc)?;
    tx.commit()
        .map_err(|e| format!("committing alignments of work {work_id}: {e}"))
}

fn axis_text(axis: Axis) -> &'static str {
    match axis {
        Axis::Vertical => "v",
        Axis::Horizontal => "h",
    }
}

fn parse_axis(text: &str) -> Result<Axis, String> {
    match text {
        "v" => Ok(Axis::Vertical),
        "h" => Ok(Axis::Horizontal),
        other => Err(format!("`{other}` is not an axis")),
    }
}

fn kind_text(kind: Kind) -> &'static str {
    match kind {
        Kind::Dialogue => "dialogue",
        Kind::Narration => "narration",
        Kind::Sfx => "sfx",
        Kind::Logo => "logo",
        Kind::Credits => "credits",
        Kind::Other => "other",
    }
}

fn parse_kind(text: &str) -> Result<Kind, String> {
    match text {
        "dialogue" => Ok(Kind::Dialogue),
        "narration" => Ok(Kind::Narration),
        "sfx" => Ok(Kind::Sfx),
        "logo" => Ok(Kind::Logo),
        "credits" => Ok(Kind::Credits),
        "other" => Ok(Kind::Other),
        other => Err(format!("`{other}` is not a truth-box kind")),
    }
}

#[cfg(test)]
mod tests;
