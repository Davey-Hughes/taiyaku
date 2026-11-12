//! Reading progress, notes, the labelling truth document and a work's
//! alignments, as rows.
//!
//! `Progress`, `Notes` and `Note` (with notes' own `Region`) live at this
//! crate's root rather than in `taiyaku-serve`, which cannot be the thing this
//! store depends on. `TruthDoc` is in `taiyaku-core`, which this crate
//! depends on.
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
//! [`merge_notes`]. Progress is one row per edition and truth's rule is
//! whole-document last-write-wins because `TruthRegion` has no identity to
//! merge on; both are simple enough to apply inline wherever a caller compares
//! two `updated` timestamps, which is not this module's job.
//!
//! `fold::move_positions` is a conflict resolution over this very table,
//! applying the newer-wins rule tabled above when two works' positions meet on
//! one edition key, and `fold::Moved::lost_positions` counts the reading the
//! loser gives up. The rule is not applied *here*, but it is applied, and
//! by name.
//!
//! [`put_alignments`] is the fourth writer here and belongs to none of those
//! three rules — see its own doc comment for why the document it writes is a
//! different kind of thing, and where its merge rule actually lives.
//!
//! **A position or a note names the page it was written on, beside its
//! slot** (O244): the member's digest, which copy of it, the page within it
//! and the half. [`put_progress_at`] and [`put_notes_in`] write them from the
//! layout the slot is numbered in, and [`progress_in`] and [`notes_in`]
//! resolve them against the layout the slot is read in, so a change to the
//! members or spread flags around the page leaves the row on it, unless an
//! earlier copy of its archive in the edition goes and the copies renumber
//! (`Edition::slot_of`). The stored slot is the fallback, for a row that
//! names no page or whose page is gone; `resolve_position` and
//! `resolve_note` hold the rules.
//!
//! **One layout per request** ([`Numbering`]). A server numbers its reader's
//! slots in its own copy of the work, which trails the store's for a moment
//! after a scan or a removal, so it hands that copy in and a slot is read and
//! anchored in the whole document its reader's view is built from. A caller
//! with no document of its own (import, export) uses the store's.
//!
//! **File I/O, the `ETag` helper, the `Invalid` enum and route-facing
//! validation all stay in `taiyaku-serve`.** This module is rows in, rows out.

use std::borrow::Cow;
use std::collections::BTreeMap;

use rusqlite::{Connection, params};
use taiyaku_core::{
    Axis, Direction, Edition, Kind, MemberPage, RegionMember, Sidecar, SlotOf, TruthDoc,
    TruthEdition, TruthRegion,
};

use crate::{Note, Notes, Position, Progress, Region};

// --- progress ----------------------------------------------------------

/// Every position one reader holds on one work, newest first.
///
/// **A vector rather than an `Option`**: a reader holds a place in each
/// edition of a volume. The head is the most recently updated, which is what a
/// caller wanting "where was I" should take — see `serve`'s `get_progress` and
/// `web/src/api.ts`'s `getProgress`.
///
/// Each slot is the one the position's page shows in the store's layout now,
/// as the module doc says, not necessarily the number stored.
///
/// # Errors
///
/// Returns an error if SQLite fails, if the work's stored reading direction
/// is not one this crate writes, or if a row's anchor has some but not all
/// of `at_sha256`, `at_nth` and `at_page` set, or a half with none of them,
/// a state [`put_progress_at`] cannot write.
pub fn progress(conn: &Connection, work_id: i64, user_id: i64) -> Result<Vec<Position>, String> {
    progress_in(conn, work_id, user_id, Numbering::Store)
}

/// [`progress`], each slot the one the position's page shows in `numbering`.
///
/// # Errors
///
/// As [`progress`].
pub fn progress_in(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    numbering: Numbering<'_>,
) -> Result<Vec<Position>, String> {
    let mut statement = conn
        .prepare(
            "SELECT edition, slot, updated, at_sha256, at_nth, at_page, at_half FROM progress
              WHERE work_id = ?1 AND user_id = ?2
              ORDER BY updated DESC, edition ASC",
        )
        .map_err(|e| format!("preparing the progress read: {e}"))?;
    let rows = statement
        .query_map(params![work_id, user_id], |row| {
            Ok((
                Position {
                    edition: row.get(0)?,
                    slot: row.get(1)?,
                    updated: row.get(2)?,
                },
                AtColumns {
                    sha256: row.get(3)?,
                    nth: row.get(4)?,
                    page: row.get(5)?,
                    half: row.get(6)?,
                },
            ))
        })
        .map_err(|e| format!("reading progress for work {work_id}, user {user_id}: {e}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("reading progress for work {work_id}, user {user_id}: {e}"))?;

    // The primary key holds one row per edition key, so each layout is read
    // once.
    rows.into_iter()
        .map(|(mut position, columns)| {
            let at = at_from_columns(columns).map_err(|e| {
                format!(
                    "the position of user {user_id} on work {work_id}, edition `{}`, has {e}",
                    position.edition
                )
            })?;
            let layout = layout_in(conn, work_id, &position.edition, numbering)?;
            position.slot = resolve_position(
                layout
                    .as_ref()
                    .map(|(edition, reading)| (edition.as_ref(), *reading)),
                position.slot,
                at.as_ref(),
            );
            Ok(position)
        })
        .collect()
}

/// Record where a reader got to, now.
///
/// The ordinary page turn, and [`put_progress_at`] with no recorded time —
/// see that function for the statement and for everything it promises.
///
/// # Errors
///
/// As [`put_progress_at`].
pub fn put_progress(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    progress: &Progress,
) -> Result<(), String> {
    put_progress_at(conn, work_id, user_id, progress, None, Numbering::Store)
}

/// [`put_progress`], its page found in `numbering`: a server passes the
/// document its reader's slot is numbered in.
///
/// # Errors
///
/// As [`put_progress_at`].
pub fn put_progress_in(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    progress: &Progress,
    numbering: Numbering<'_>,
) -> Result<(), String> {
    put_progress_at(conn, work_id, user_id, progress, None, numbering)
}

/// Record where a reader got to, at the time the caller names.
///
/// An upsert on `(user_id, work_id, edition)`: it does not touch
/// `completed_at` or `read_count`, which nothing in this module's interface
/// manages.
///
/// **`ON CONFLICT … DO UPDATE` naming the columns a position is made of
/// rather than `INSERT OR REPLACE`, and that is the whole of the promise
/// above.** A REPLACE would clear both columns on every page turn — reading
/// history the schema calls "behavioural data with NOTHING to backfill" — and
/// the two statements are otherwise interchangeable here, `progress` being
/// `WITHOUT ROWID` on `(user_id, work_id, edition)`. `edition` itself is not
/// named in the `DO UPDATE` list: it is part of the conflict target, so
/// assigning it would be assigning a column to itself. Pinned by
/// `put_progress_leaves_completed_at_and_read_count_alone` in
/// this module's `tests.rs`.
///
/// **`updated: None` stamps `datetime('now')` and is what every live write
/// passes.** `Some` exists for one caller: `migrate::apply_progress`,
/// reinstating positions out of a `taiyaku.progress.json` this project
/// exported from this very column. Stamping *those* now lands every position
/// a reader holds in the same second, which collapses
/// [`progress`]'s `ORDER BY updated DESC, edition ASC` to alphabetical — and
/// that head is what `web/src/api.ts`'s `getProgress` resumes into, so a
/// reader 190 slots into `raw-mono` and 2 into `en-fan` reopens in `en-fan`.
/// The document records when each position was reached; honouring it is the
/// difference between restoring a reading and restoring a set of slots.
///
/// **The string is written as given, and it is compared lexically
/// everywhere.** This table's own `ORDER BY`, and
/// `fold::move_positions`' `excluded.updated > progress.updated`, are both
/// string comparisons, so a time in any format but SQLite's own
/// `YYYY-MM-DD HH:MM:SS` sorts against the rest of the column rather than
/// beside it. Nothing here validates that; the one caller passing `Some`
/// reads its value back out of a document written from these rows.
///
/// # Errors
///
/// Returns an error if SQLite fails, or if the work's stored reading
/// direction is not one this crate writes.
pub fn put_progress_at(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    progress: &Progress,
    updated: Option<&str>,
    numbering: Numbering<'_>,
) -> Result<(), String> {
    // The page the slot shows in `numbering` is written beside it on insert
    // and on a page turn alike (O244), all NULL past the end or on an edition
    // the layout does not hold. A `DO UPDATE` without the four would keep the
    // first page a reader stood on, and `progress` would answer it.
    let at = layout_in(conn, work_id, &progress.edition, numbering)?
        .and_then(|(edition, reading)| edition.member_page(progress.slot, reading));
    let (sha256, nth, page, half) = at_columns(at.as_ref());
    conn.execute(
        "INSERT INTO progress
           (user_id, work_id, edition, slot, updated, at_sha256, at_nth, at_page, at_half)
         VALUES (?1, ?2, ?3, ?4, coalesce(?5, datetime('now')), ?6, ?7, ?8, ?9)
         ON CONFLICT(user_id, work_id, edition) DO UPDATE SET
           slot = excluded.slot,
           updated = excluded.updated,
           at_sha256 = excluded.at_sha256,
           at_nth = excluded.at_nth,
           at_page = excluded.at_page,
           at_half = excluded.at_half",
        params![
            user_id,
            work_id,
            progress.edition,
            progress.slot,
            updated,
            sha256,
            nth,
            page,
            half
        ],
    )
    .map_err(|e| format!("writing progress for work {work_id}, user {user_id}: {e}"))?;
    Ok(())
}

// --- anchors (O244) --------------------------------------------------------

/// The layout a position's slot is numbered in, which its page is found and
/// resolved in (O244).
#[derive(Debug, Clone, Copy)]
pub enum Numbering<'a> {
    /// The store's rows as they stand: for a caller with no document of its
    /// own, as import and export are.
    Store,
    /// The document a server numbered its reader's view with. It trails the
    /// store's for a moment after a scan or a removal. Slots are read and
    /// written in its numbering; a narrowed reader's are translated at serve.
    Document(&'a Sidecar),
}

/// Edition `key`'s layout in `numbering`, and the reading direction its slots
/// run in. `None` where that layout holds no edition `key`.
fn layout_in<'a>(
    conn: &Connection,
    work_id: i64,
    key: &str,
    numbering: Numbering<'a>,
) -> Result<Option<(Cow<'a, Edition>, Direction)>, String> {
    Ok(match numbering {
        Numbering::Store => crate::project::layout(conn, work_id, key)?
            .map(|(edition, reading)| (Cow::Owned(edition), reading)),
        Numbering::Document(doc) => doc
            .editions
            .get(key)
            .map(|edition| (Cow::Borrowed(edition), doc.work.reading)),
    })
}

/// A row's four `at_` columns as read, before [`at_from_columns`] checks them.
pub(crate) struct AtColumns {
    pub(crate) sha256: Option<String>,
    pub(crate) nth: Option<u32>,
    pub(crate) page: Option<u32>,
    pub(crate) half: Option<String>,
}

/// The four `at_` column values for `at`, all NULL for no page.
///
/// The one place a row's anchor columns are made, so the first three are all
/// set or all NULL: [`at_from_columns`] refuses anything else, and the
/// migration has no CHECK across the columns to keep it.
fn at_columns(at: Option<&MemberPage>) -> (Option<&str>, Option<u32>, Option<u32>, Option<&str>) {
    match at {
        Some(at) => (
            Some(at.sha256.as_str()),
            Some(at.nth),
            Some(at.page),
            crate::import::half_text(at.half),
        ),
        None => (None, None, None, None),
    }
}

/// A row's four `at_` columns back into the page it names, refusing what
/// [`at_columns`] never writes: some but not all of the first three set, or a
/// half with no page. The error completes a sentence naming the row.
pub(crate) fn at_from_columns(columns: AtColumns) -> Result<Option<MemberPage>, String> {
    match columns {
        AtColumns {
            sha256: None,
            nth: None,
            page: None,
            half: None,
        } => Ok(None),
        AtColumns {
            sha256: Some(sha256),
            nth: Some(nth),
            page: Some(page),
            half,
        } => Ok(Some(MemberPage {
            sha256,
            nth,
            page,
            half: crate::project::parse_half(half.as_deref())?,
        })),
        AtColumns {
            sha256: None,
            nth: None,
            page: None,
            half: Some(half),
        } => Err(format!(
            "a half with no page (at_half={half:?}); at_half is set only beside \
             at_sha256, at_nth and at_page"
        )),
        AtColumns {
            sha256,
            nth,
            page,
            half,
        } => Err(format!(
            "an anchor with only some of at_sha256, at_nth and at_page set \
             (at_sha256={sha256:?}, at_nth={nth:?}, at_page={page:?}, at_half={half:?}); \
             an anchor is all three or none"
        )),
    }
}

/// Where a position stands in its edition's layout now (O244): the slot its
/// page shows; the page's first slot where its half is no longer held (a half
/// of a page no longer a spread, or a whole page now one); and otherwise its
/// stored slot, clamped to the last slot where the page is gone, so it still
/// opens a page the edition has.
///
/// A row with no anchor, or on an edition key the layout does not hold,
/// answers its stored slot as it stands: there is nothing to find or clamp to.
pub(crate) fn resolve_position(
    layout: Option<(&Edition, Direction)>,
    stored: u32,
    at: Option<&MemberPage>,
) -> u32 {
    let (Some((edition, reading)), Some(at)) = (layout, at) else {
        return stored;
    };
    match edition.slot_of(at, reading) {
        Some(SlotOf::Exact(slot) | SlotOf::PageOnly(slot)) => slot,
        None => match edition.slots(reading).len().checked_sub(1) {
            Some(last) => stored.min(u32::try_from(last).unwrap_or(u32::MAX)),
            None => stored,
        },
    }
}

/// Where a note stands in its edition's layout now, and whether it is
/// `stale` (O244): the slot its page and half show, and otherwise its stored
/// slot, unclamped and `stale`. A note is never moved to a page it was not
/// written on, so a half the layout no longer holds leaves it where it was,
/// `stale`.
///
/// On an edition key the layout does not hold it is `stale` whatever its
/// anchor; with no anchor otherwise, it answers its stored slot and is not.
pub(crate) fn resolve_note(
    layout: Option<(&Edition, Direction)>,
    stored: u32,
    at: Option<&MemberPage>,
) -> (u32, bool) {
    let Some((edition, reading)) = layout else {
        return (stored, true);
    };
    let Some(at) = at else {
        return (stored, false);
    };
    match edition.slot_of(at, reading) {
        Some(SlotOf::Exact(slot)) => (slot, false),
        Some(SlotOf::PageOnly(_)) | None => (stored, true),
    }
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
    at: AtColumns,
}

/// Each edition key's layout in one [`Numbering`], read once however many
/// notes stand on it. `None` where that layout holds no such edition.
type Layouts<'a> = BTreeMap<String, Option<(Cow<'a, Edition>, Direction)>>;

fn layouts_in<'a, 'k>(
    conn: &Connection,
    work_id: i64,
    keys: impl IntoIterator<Item = &'k str>,
    numbering: Numbering<'a>,
) -> Result<Layouts<'a>, String> {
    let mut layouts = Layouts::new();
    for key in keys {
        if !layouts.contains_key(key) {
            let layout = layout_in(conn, work_id, key, numbering)?;
            layouts.insert(key.to_string(), layout);
        }
    }
    Ok(layouts)
}

fn layout_of<'l>(layouts: &'l Layouts<'_>, key: &str) -> Option<(&'l Edition, Direction)> {
    layouts
        .get(key)
        .and_then(Option::as_ref)
        .map(|(edition, reading)| (edition.as_ref(), *reading))
}

/// Every note a reader holds on a work.
///
/// `work` is filled from `work.slug`, which is `taiyaku-serve`'s work id for
/// a work with a store row, since nothing in the `note` table carries a work
/// label of its own.
///
/// Each slot is the one the note's page shows in the store's layout now, or
/// its stored slot marked `stale` where that page is not there, as the module
/// doc says.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if SQLite fails, if the
/// work's stored reading direction is not one this crate writes, or if a
/// stored note has a region with some but not all of its four columns set —
/// a state `put_notes` cannot write and that must not be read back as either
/// "no region" or a silently-guessed one. The same for an anchor with some
/// but not all of `at_sha256`, `at_nth` and `at_page` set, or a half with
/// none of them.
pub fn notes(conn: &Connection, work_id: i64, user_id: i64) -> Result<Notes, String> {
    notes_in(conn, work_id, user_id, Numbering::Store)
}

/// [`notes`], each slot the one the note's page shows in `numbering`.
///
/// # Errors
///
/// As [`notes`].
pub fn notes_in(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    numbering: Numbering<'_>,
) -> Result<Notes, String> {
    let work = work_slug(conn, work_id)?;
    let held = held_notes(conn, work_id, user_id)?;
    let layouts = layouts_in(
        conn,
        work_id,
        held.iter().map(|(note, _)| note.edition.as_str()),
        numbering,
    )?;
    let notes = held
        .into_iter()
        .map(|(mut note, at)| {
            resolve(&mut note, at.as_ref(), &layouts);
            note
        })
        .collect();
    Ok(Notes {
        taiyaku_notes: crate::NOTES_VERSION,
        work,
        notes,
    })
}

/// Every note row as stored, in id order, beside the page its row names.
fn held_notes(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
) -> Result<Vec<(Note, Option<MemberPage>)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, edition, slot, rx, ry, rw, rh, color, body, created, updated,
                    at_sha256, at_nth, at_page, at_half
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
                at: AtColumns {
                    sha256: row.get(11)?,
                    nth: row.get(12)?,
                    page: row.get(13)?,
                    half: row.get(14)?,
                },
            })
        })
        .map_err(|e| format!("querying notes: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading notes: {e}"))?;

    rows.into_iter().map(note_from_row).collect()
}

/// `note`'s slot and `stale` as its own edition's layout answers them.
fn resolve(note: &mut Note, at: Option<&MemberPage>, layouts: &Layouts<'_>) {
    (note.slot, note.stale) = resolve_note(layout_of(layouts, &note.edition), note.slot, at);
}

fn note_from_row(row: NoteRow) -> Result<(Note, Option<MemberPage>), String> {
    let region = region_from_columns(&row.id, row.rx, row.ry, row.rw, row.rh)?;
    let at = at_from_columns(row.at).map_err(|e| format!("note `{}` has {e}", row.id))?;
    let note = Note {
        id: row.id,
        edition: row.edition,
        slot: row.slot,
        region,
        color: row.color,
        body: row.body,
        created: row.created,
        updated: row.updated,
        stale: false,
    };
    Ok((note, at))
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
/// `validate` doc comment), so a write is delete-then-reinsert, inside one
/// transaction so a failure partway through leaves the prior set intact rather
/// than half-replaced. `doc.work` is not written anywhere: nothing in the
/// `note` table carries a work label, and `work_id` already says which work
/// this is. A note nobody moved keeps the page its row names
/// ([`put_notes_in`]).
///
/// # Errors
///
/// Returns an error if SQLite fails — including a `UNIQUE` violation on
/// `note.id` if `doc.notes` repeats an id, since this module does not
/// duplicate `notes.rs`'s own check for that — or if a held note cannot be
/// read, as [`notes`] says.
pub fn put_notes(conn: &Connection, work_id: i64, user_id: i64, doc: &Notes) -> Result<(), String> {
    put_notes_in(conn, work_id, user_id, doc, Numbering::Store)
}

/// [`put_notes`], each slot read in `numbering` (O244). A note whose edition,
/// slot and region equal its held note's ([`Note::same_anchor`]) keeps the
/// held `at_` columns; any other is given the page its slot shows. Giving every
/// note that page would tie a lost file's note to the page its stored slot
/// shows now, so it could never find its own page again. The held notes are
/// read in `numbering` too: `same_anchor` compares slots, so one read in
/// another numbering would look moved.
///
/// # Errors
///
/// As [`put_notes`].
pub fn put_notes_in(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    doc: &Notes,
    numbering: Numbering<'_>,
) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting notes transaction: {e}"))?;
    let held = held_notes(&tx, work_id, user_id)?;
    let layouts = layouts_in(
        &tx,
        work_id,
        held.iter()
            .map(|(note, _)| note.edition.as_str())
            .chain(doc.notes.iter().map(|note| note.edition.as_str())),
        numbering,
    )?;
    let held: BTreeMap<String, (Note, Option<MemberPage>)> = held
        .into_iter()
        .map(|(mut note, at)| {
            resolve(&mut note, at.as_ref(), &layouts);
            (note.id.clone(), (note, at))
        })
        .collect();
    tx.execute(
        "DELETE FROM note WHERE work_id = ?1 AND user_id = ?2",
        params![work_id, user_id],
    )
    .map_err(|e| format!("clearing notes for work {work_id}, user {user_id}: {e}"))?;

    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO note
                   (id, user_id, work_id, edition, slot, rx, ry, rw, rh, color, body, created,
                    updated, at_sha256, at_nth, at_page, at_half)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                         ?14, ?15, ?16, ?17)",
            )
            .map_err(|e| format!("preparing note insert: {e}"))?;
        for note in &doc.notes {
            let (rx, ry, rw, rh) = match note.region {
                Some(r) => (Some(r.x), Some(r.y), Some(r.w), Some(r.h)),
                None => (None, None, None, None),
            };
            let at = match held.get(&note.id) {
                Some((stored, at)) if stored.same_anchor(note) => at.clone(),
                _ => layout_of(&layouts, &note.edition)
                    .and_then(|(edition, reading)| edition.member_page(note.slot, reading)),
            };
            let (sha256, nth, page, half) = at_columns(at.as_ref());
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
                sha256,
                nth,
                page,
                half,
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
        taiyaku_notes: crate::NOTES_VERSION,
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
/// be a projection of the present: that would make the comparison compare a
/// list against itself. `truth_member` holds it. `taiyaku_truth` is the
/// current [`taiyaku_core::TRUTH_VERSION`], never a stored value: that one
/// really is a property of this build.
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
/// examined and found empty round-trip as exactly that, not as "never opened".
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
/// which archives these boxes were drawn against, and the only copy of it.
/// The labelling client stamps it fresh from the sidecar it is reading on
/// every write (`web/src/truthWrite.ts`'s `withPage`), so what arrives here is
/// what the labeller had in front of them.
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
/// belong together (O43).
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
