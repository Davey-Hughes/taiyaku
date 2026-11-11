//! Read the four sibling documents `export.rs` writes, back into rows, once.
//!
//! `export.rs`'s own doc comment is this module's inverse in miniature: where
//! that one turns rows into `<stem>.{library,notes,progress,truth}.json`
//! beside a sidecar, this turns those same four files back into rows, for the
//! one caller that needs it — a sidecar entering the library for the first
//! time, where "first" is a property of the *database*, not of the file.
//!
//! **Why this exists at all.** Reading progress, notes and the labelling truth
//! are rows, not these files, so without this module an existing install
//! upgrading to a database-backed library would come up with every reading
//! position at zero, every note gone and every labelled truth page unopened —
//! the files sitting on disk, intact and unread, with no error and
//! nothing printed.
//!
//! # "First import" is checked once per document, not once per work
//!
//! A `work` row can exist with nothing recorded against it at all —
//! `taiyaku-serve`'s `works::row_id_for_write` mints exactly that kind of
//! placeholder so a reading position can be saved before anything is ever
//! imported — so "the row is new" is the wrong test.
//!
//! **What this module checks is a stored fact: has this work's *this*
//! document been migrated before?** One row in `migrated_document` per work
//! per document, written the first time that document is considered at all.
//! A document with a row is never revisited: the sibling file is left unread
//! whether or not it exists, and whether or not the rows it once wrote are
//! still there.
//!
//! **That row carries a second fact, `absorbed`, which this module writes and
//! [`crate::export`] reads**: whether this library has ever had the file's
//! *content* — read it in here, or written it out there. Settled and absorbed
//! are not the same thing, and treating them as one would let an export delete
//! a `taiyaku.truth.json` that had appeared after the first import and so had
//! never been read by anything. See [`absorbed`].
//!
//! Inferring that fact from content — "the database holds no note for this
//! work" standing in for "this work's notes have never been migrated" — would
//! conflate two different questions, and the difference is a reader's own
//! deletion: delete your last note and the inference falls through, so the
//! stale file would be applied again and the note would come back.
//! `taiyaku serve <sidecar>...` imports every path on every start, so it would
//! come back at every restart, forever, and nothing a reader could type would
//! make the deletion stick. **"Migrated once" and "currently empty" have to be
//! distinguishable, and only a stored fact distinguishes them.**
//!
//! [`crate::export::work`] does remove a sibling file whose state has become
//! `None`, which is the *other* half and not a substitute for this one: it runs
//! when a person asks for an export, while the resurrection above would happen
//! on every start, before anyone could have asked for anything. **It removes
//! only what this library has absorbed**, which is the line the two halves meet
//! along — see [`absorbed`].
//!
//! On that first pass, and only there, the live rows still get the last word
//! over the file — a `work` row can be older than its first import:
//!
//! | Document | "Already holds state" means | Checked with |
//! | --- | --- | --- |
//! | `.progress.json` | a `progress` row exists for this work and that user | [`crate::state::progress`] is non-empty |
//! | `.notes.json` | at least one `note` row exists for this work and that user | [`crate::state::notes`]`.notes` is non-empty |
//! | `.truth.json` | at least one `truth_page` or `truth_member` row exists for this work | [`crate::state::truth`] is `Some` |
//! | `.library.json` | any hand-fixed metadata is recorded for this work | ![`LibraryDoc::is_empty`] |
//!
//! A document already holding state is left alone even when its sibling file
//! exists and parses cleanly — **rows are current, files are history, and
//! this is a one-way migration, not a sync.** Overwriting a reader's live
//! progress with a stale file every time a library is reopened would be a
//! second, quieter version of the defect this module exists to close.
//!
//! **A file that appears later is not migrated either**, because the record
//! is per document rather than per file: a work imported with no siblings has
//! all four documents recorded as considered, so an `export --in-place` that
//! writes them and a deletion that follows are not undone by the next start.
//! Losing the database is what makes a sibling file authoritative again — it
//! takes these rows with it, which is exactly the state this module is for.
//!
//! Such a file is not *destroyed* either: it is settled but unabsorbed, so an
//! export leaves it exactly where it is and names it, rather than overwriting
//! it with an empty document or taking it away. Neither side claims it — this
//! module will not read it, and export will not touch it — and that outcome is
//! reported (`Unread`) rather than filed under `kept`, which means
//! "the database already held state" where here it holds none.
//!
//! # A malformed sibling never fails the import, and is the one thing retried
//!
//! Reading a bad `.notes.json` reports and skips only that one document —
//! [`work`] returns `Ok` regardless, and the sidecar itself still imports.
//! Refusing the whole import over one unreadable auxiliary file would let one
//! bad file block an entire library, which is a worse failure than losing
//! that one file's own state (still on disk, untouched, and importable again
//! once fixed — see the next section).
//!
//! It is also the one outcome that writes **no** `migrated_document` row: a
//! file that could not be read has not been migrated, and importing again
//! once it is fixed is how a reader recovers. Every other outcome —
//! applied, left unread because the database already held state, no file
//! there at all — is recorded, and never revisited.
//!
//! No row means unabsorbed as well as unsettled, so an export will not
//! overwrite or remove that file either. Both halves of the recovery path
//! have to hold for it to be one: a malformed `taiyaku.truth.json` deleted by
//! the next routine export leaves nothing to fix.
//!
//! # The library is never modified
//!
//! Nothing in this module writes, deletes or moves a sibling file. "Your
//! library is not modified" is this project's stated principle and these
//! files are the reader's; leaving them alone also means a migration that
//! failed partway (a malformed `.truth.json` on an otherwise-good work) can
//! simply be retried later, once the file is fixed, with no state lost in the
//! meantime.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use taiyaku_core::TruthDoc;

use crate::{Notes, export, librarydoc, state};

/// What happened to one work's `.progress.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressOutcome {
    /// Read and written into `progress` — one row per position the document
    /// held, which is one per edition since migration 9 widened that table's
    /// key. The bare legacy document carries exactly one.
    Applied,
    /// The database already held a reading position for this work; the file,
    /// if it exists, was left unread.
    AlreadyPresent,
    /// The file exists and was left unread — this document was settled for
    /// this work on an earlier import — **and the database holds no reading
    /// position for it either**. See [`NotesOutcome::Unread`].
    Unread,
    /// The file exists but could not be read, parsed, or written into rows.
    /// The message is for a caller to print in full — never for a machine to
    /// branch on beyond "this failed".
    Malformed(String),
}

/// What happened to one work's `.notes.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesOutcome {
    /// Read and written into `note` rows. `count` is how many notes it held.
    Applied { count: usize },
    /// The database already held at least one note for this work; the file,
    /// if it exists, was left unread.
    AlreadyPresent,
    /// The file exists and was left unread — this document was settled for
    /// this work on an earlier import — **and the database holds no note for
    /// it either**.
    ///
    /// Distinct from [`Self::AlreadyPresent`], because reporting both as one
    /// word, `kept`, whose documented meaning is the *other* one, would tell a
    /// reader the database already held their notes at the exact moment it held
    /// nothing. It is also the shape worth noticing — the file was either read
    /// once and its state since cleared, or it has never been through this
    /// library at all, and [`crate::export::work`] treats those
    /// two differently.
    Unread,
    /// The file exists but could not be read, parsed, or written into rows.
    Malformed(String),
}

/// What happened to one work's `.truth.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TruthOutcome {
    /// Read and written into `truth_page`/`truth_box` rows. `pages` counts
    /// every page looked at, whether or not it held a box — the same count
    /// that would be lost by seeding from boxes alone (`state::truth`'s own
    /// doc comment). `boxes` counts the boxes across all of them.
    Applied { pages: usize, boxes: usize },
    /// The database already held at least one labelled page for this work;
    /// the file, if it exists, was left unread.
    AlreadyPresent,
    /// The file exists and was left unread — this document was settled for
    /// this work on an earlier import — **and the database holds no labelled
    /// page for it either**. See [`NotesOutcome::Unread`].
    Unread,
    /// The file exists but could not be read, parsed, or written into rows.
    Malformed(String),
}

/// What happened to one work's `.library.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryOutcome {
    /// Read and applied via [`librarydoc::apply`].
    Applied,
    /// The database already held some hand-fixed metadata for this work; the
    /// file, if it exists, was left unread.
    AlreadyPresent,
    /// The file exists and was left unread — this document was settled for
    /// this work on an earlier import — **and the database holds no hand-fixed
    /// metadata for it either**. See [`NotesOutcome::Unread`].
    Unread,
    /// The file exists but could not be read, parsed, or applied.
    Malformed(String),
}

/// Every sibling document [`work`] looked at for one sidecar.
///
/// `None` where no file of that kind sits beside the sidecar at all —
/// nothing to report, the same way `export::work` writes no document for a
/// `None` `Progress`/`TruthDoc`. All four fields are `pub` so a caller can
/// both format them (`taiyaku-cli`'s own `migration_summary`) and construct
/// one directly in a test.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiblingMigration {
    pub progress: Option<ProgressOutcome>,
    pub notes: Option<NotesOutcome>,
    pub truth: Option<TruthOutcome>,
    pub library: Option<LibraryOutcome>,
}

impl SiblingMigration {
    /// Whether anything here is worth a caller mentioning at all — every
    /// field `None` means no sibling file existed beside the sidecar, which
    /// is the ordinary case for a work that was never anything but a
    /// database import.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.progress.is_none()
            && self.notes.is_none()
            && self.truth.is_none()
            && self.library.is_none()
    }
}

/// Migrate every sibling of `sidecar_path` that this work does not already
/// hold state for, into `work_id`'s rows.
///
/// Called once per import, after the sidecar itself has been imported (so
/// `work_id` names a real row with real editions — `.library.json` can name
/// one of them, and [`librarydoc::apply`] needs it to already exist). Never
/// fails over a bad sibling file — see this module's own doc comment — so a
/// caller can always trust the `Ok` case represents the sidecar import
/// having succeeded, with the four documents' own fates recorded inside it.
///
/// # Errors
///
/// Returns an error only if SQLite itself fails in a way none of the four
/// documents' own `Malformed` handling catches — in practice, unreachable
/// short of a corrupted database, since every fallible step per document is
/// caught and folded into that document's own outcome.
pub fn work(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    sidecar_path: &Path,
) -> Result<SiblingMigration, String> {
    Ok(SiblingMigration {
        progress: migrate_progress(conn, work_id, user_id, sidecar_path)?,
        notes: migrate_notes(conn, work_id, user_id, sidecar_path)?,
        truth: migrate_truth(conn, work_id, sidecar_path)?,
        library: migrate_library(conn, work_id, sidecar_path)?,
    })
}

/// The four values `migrated_document.document` ever holds, one per sibling
/// document. Named constants rather than literals at each call site: a typo
/// in one of these would silently give that document a second, permanently
/// empty history — the marker would never match the one written before it,
/// and the file would be migrated again on every import, which is the defect
/// this table exists to close wearing a different hat.
///
/// `pub(crate)` because [`crate::export`] settles the same four documents
/// from the other side — a document it writes is one this library has had the
/// content of — and a fifth and sixth copy of these strings is the hazard the
/// paragraph above describes.
pub(crate) const PROGRESS: &str = "progress";
pub(crate) const NOTES: &str = "notes";
pub(crate) const TRUTH: &str = "truth";
pub(crate) const LIBRARY: &str = "library";

/// What `migrated_document` records about this work's `document`, if
/// anything: `None` where no row exists at all, and otherwise whether this
/// library has ever had the file's content.
///
/// See [`absorbed`] for what that second half means and why it exists.
fn marker(conn: &Connection, work_id: i64, document: &str) -> Result<Option<bool>, String> {
    use rusqlite::OptionalExtension as _;

    conn.query_row(
        "SELECT absorbed FROM migrated_document WHERE work_id = ?1 AND document = ?2",
        rusqlite::params![work_id, document],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map(|found| found.map(|absorbed| absorbed != 0))
    .map_err(|e| format!("checking whether `{document}` was migrated for work {work_id}: {e}"))
}

/// **Whether this library has ever had the content of this work's
/// `document`** — read it into rows on an import, or written it out of them
/// on an export.
///
/// The question [`crate::export::work`] has to ask before it overwrites or
/// removes a sibling file. "This document has been settled" and "the database
/// has authority over the file at that path" are different facts, and
/// collapsing them would mean a `taiyaku.truth.json` copied in from another
/// machine — never read, because the document was already settled — would be
/// deleted by the next routine export, and a `taiyaku.notes.json` in the same
/// state would be overwritten with an empty document. Neither would have been
/// in the database.
///
/// A document with no row at all answers `false`: nothing has been settled,
/// so nothing has been absorbed. That is also the malformed case, which is
/// what keeps the documented recovery path — fix the file and import it again
/// — from being destroyed by an export in between.
pub(crate) fn absorbed(conn: &Connection, work_id: i64, document: &str) -> Result<bool, String> {
    Ok(marker(conn, work_id, document)?.unwrap_or(false))
}

/// Whether this library has ever had the content of a **per-user** document
/// belonging to *this* user.
///
/// **`absorbed` alone is not enough for a per-user document.** That marker is
/// keyed `(work_id, document)` with no user, while the state an export reads is
/// keyed on the user — so `export --all --in-place` defaulting to the lowest
/// account id would find no notes and no position for them, mark both documents
/// stale, and have the guard against removing a never-absorbed file satisfied
/// by a *different* reader's original import. The reader's own
/// `taiyaku.notes.json` would be overwritten with an empty document and their
/// `taiyaku.progress.json` deleted.
///
/// So a per-user document is treated as absorbed when the exporting user is the
/// one who has state for it. `truth` and `library` are not per-user and use
/// [`absorbed`] alone.
///
/// # Errors
///
/// Returns an error if the query fails.
pub(crate) fn absorbed_for(
    conn: &Connection,
    work_id: i64,
    document: &str,
    user_id: i64,
) -> Result<bool, String> {
    if !absorbed(conn, work_id, document)? {
        return Ok(false);
    }
    let (mine, theirs) = match document {
        NOTES => held(conn, "note", work_id, user_id)?,
        PROGRESS => held(conn, "progress", work_id, user_id)?,
        // Not per-user: one answer for the whole library, so the marker alone
        // is the whole question.
        _ => return Ok(true),
    };
    // **The danger is narrower than "this user has no state".** A document
    // nobody has state for is nobody's to lose — declining to write it there
    // would make an export refuse to rewrite the empty file it had just
    // written. What must be refused is touching a file whose content belongs to
    // *somebody else*.
    Ok(mine || !theirs)
}

/// Whether this user, and whether anyone else, holds rows for this work in a
/// per-user table.
///
/// `table` is one of two fixed literals from the match above, never caller
/// input — a `?1` cannot name a table, and this is the whole of why the two
/// arms exist rather than one parameterised query.
fn held(
    conn: &Connection,
    table: &str,
    work_id: i64,
    user_id: i64,
) -> Result<(bool, bool), String> {
    let count = |sql: String, args: [i64; 2]| -> Result<i64, String> {
        conn.query_row(&sql, rusqlite::params![args[0], args[1]], |row| row.get(0))
            .map_err(|e| format!("checking {table} for work {work_id}: {e}"))
    };
    let mine = count(
        format!("SELECT COUNT(*) FROM {table} WHERE work_id = ?1 AND user_id = ?2"),
        [work_id, user_id],
    )?;
    let theirs = count(
        format!("SELECT COUNT(*) FROM {table} WHERE work_id = ?1 AND user_id <> ?2"),
        [work_id, user_id],
    )?;
    Ok((mine > 0, theirs > 0))
}

/// Record that this document is settled for this work, and whether the file's
/// content came with it.
///
/// `DO NOTHING` on conflict rather than `INSERT OR REPLACE`: `migrated` is
/// when this document was *first* settled, a second import is not a second
/// migration, and — since this module only ever calls this where there is no
/// row — nothing here should be able to turn an absorbed document back into
/// an unabsorbed one by accident. [`record_absorbed`] is the deliberate way.
fn record_settled(
    conn: &Connection,
    work_id: i64,
    document: &str,
    absorbed: bool,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO migrated_document (work_id, document, absorbed) VALUES (?1, ?2, ?3)
         ON CONFLICT (work_id, document) DO NOTHING",
        rusqlite::params![work_id, document, i64::from(absorbed)],
    )
    .map_err(|e| format!("recording `{document}` as migrated for work {work_id}: {e}"))?;
    Ok(())
}

/// Move an already-settled document's `absorbed` to `absorbed`, settling it
/// first if it was not.
///
/// [`crate::export::work`]'s half of the marker, in both directions. Writing
/// a sibling document in place makes it this library's: the file came out of
/// these rows, so reading it back on the next import is the resurrection
/// `migrated_document` exists to prevent, and overwriting it on the next
/// export is simply the snapshot being current. Removing one is the opposite
/// — this library then holds nothing of that document on either side, so a
/// file that turns up at that path later is once again something it has never
/// seen, and is left alone rather than deleted a second time.
///
/// `migrated` is deliberately not touched: when a document was first settled
/// does not change because an export ran.
pub(crate) fn record_absorbed(
    conn: &Connection,
    work_id: i64,
    document: &str,
    absorbed: bool,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO migrated_document (work_id, document, absorbed) VALUES (?1, ?2, ?3)
         ON CONFLICT (work_id, document) DO UPDATE SET absorbed = excluded.absorbed",
        rusqlite::params![work_id, document, i64::from(absorbed)],
    )
    .map_err(|e| format!("recording `{document}` for work {work_id}: {e}"))?;
    Ok(())
}

/// The format version the document at `path` declares under `key`, read as
/// plain JSON rather than as the document, so a writer can ask before it
/// writes over or removes a file a newer taiyaku wrote (O264).
///
/// `None` for a file that is missing or unreadable, is not JSON, is not an
/// object, lacks `key`, or holds anything but a non-negative integer there:
/// none of those declares a newer version, and each writer already has its
/// own answer for such a file.
pub fn declared_version(path: &Path, key: &str) -> Option<u64> {
    let text = std::fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&text).ok()?;
    value.as_object()?.get(key)?.as_u64()
}

/// `<stem>.<suffix>`, beside `sidecar_path` — [`export::stem_of`]'s
/// arithmetic, reused rather than carried a second time.
fn sibling_path(sidecar_path: &Path, suffix: &str) -> PathBuf {
    let stem = export::stem_of(Some(&sidecar_path.to_string_lossy()));
    sidecar_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{stem}.{suffix}"))
}

fn read_sibling(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// What is left to do with one document, once the marker and the file have
/// both been consulted.
enum Step {
    /// Nothing to report: no sibling file of this kind beside the sidecar.
    Absent,
    /// The file — present — is left unread because the database already holds
    /// live state for this document. Rows are current, files are history.
    Kept,
    /// The file — present — is left unread because this document was settled
    /// for this work before, **and the database holds nothing for it**.
    /// Neither side has this document's state.
    Unread,
    /// Read the file and apply it. The only branch that can still fail, and
    /// the only one whose marker is therefore not written yet.
    Apply,
}

/// The decision, made in one place so all four documents make it identically.
///
/// Reading the live state is deferred behind `holds_state` because it is
/// never what decides whether to *read the file*: a document already settled
/// is not revisited whatever its rows look like now, which is the whole point
/// of the marker. It decides what the caller is **told**, which is a separate
/// question: reporting every settled document as `kept`, documented as
/// "the database already held state for that document", would be wrong where
/// the database holds nothing at all.
fn step(
    conn: &Connection,
    work_id: i64,
    document: &str,
    path: &Path,
    holds_state: impl FnOnce() -> Result<bool, String>,
) -> Result<Step, String> {
    let on_disk = path.is_file();
    if marker(conn, work_id, document)?.is_some() {
        if !on_disk {
            return Ok(Step::Absent);
        }
        return Ok(if holds_state()? {
            Step::Kept
        } else {
            Step::Unread
        });
    }
    if !on_disk {
        // Recorded even with no file to read: "this document has been
        // settled for this work", so one written later — by
        // `export --in-place`, most plausibly — is not read back over the
        // rows it came from. `absorbed` is false: nothing of this document's
        // content has been through this library, so a file that does turn up
        // at that path is not this library's to overwrite or remove either.
        // See this module's own doc comment.
        record_settled(conn, work_id, document, false)?;
        return Ok(Step::Absent);
    }
    if holds_state()? {
        // Settled, and again with nothing absorbed: the rows won, and this
        // file's content was never read. Leaving it alone is the whole of
        // "your library is not modified" — an export that overwrote it would
        // be discarding a document nothing here has ever looked at.
        record_settled(conn, work_id, document, false)?;
        return Ok(Step::Kept);
    }
    Ok(Step::Apply)
}

fn migrate_progress(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    sidecar_path: &Path,
) -> Result<Option<ProgressOutcome>, String> {
    let path = sibling_path(sidecar_path, "progress.json");
    match step(conn, work_id, PROGRESS, &path, || {
        Ok(!state::progress(conn, work_id, user_id)?.is_empty())
    })? {
        Step::Absent => Ok(None),
        Step::Kept => Ok(Some(ProgressOutcome::AlreadyPresent)),
        Step::Unread => Ok(Some(ProgressOutcome::Unread)),
        Step::Apply => Ok(Some(match apply_progress(conn, work_id, user_id, &path) {
            Ok(()) => {
                record_settled(conn, work_id, PROGRESS, true)?;
                ProgressOutcome::Applied
            }
            Err(e) => ProgressOutcome::Malformed(e),
        })),
    }
}

fn apply_progress(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    path: &Path,
) -> Result<(), String> {
    let text = read_sibling(path)?;
    let recorded =
        crate::progressdoc::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    // **The document's own `updated`, not `datetime('now')`.** These rows are
    // a reading being reinstated rather than one being made, and stamping them
    // now puts every position a reader holds in one second — which makes
    // `state::progress`'s `ORDER BY updated DESC, edition ASC` alphabetical
    // and hands `getProgress` an arbitrary edition to resume into. The legacy
    // bare form records no time, so it gets the stamp: see
    // `state::put_progress_at`.
    //
    // **One transaction around every position, so a failure partway writes
    // none of them (O79).** Without it, a statement refused at the second
    // position would leave the first written; this would return `Malformed` and
    // record no marker, and the next import's `holds_state` would find that one
    // row and answer `Kept`, so the rest of the document would never be read
    // again. Rolled back, the rows stay empty and the next import applies the
    // whole document. A malformed *position* never reaches this loop:
    // `progressdoc::from_json` has already parsed the whole document above.
    // `unchecked_transaction`, as `state::put_notes` opens for the next
    // document on this same connection: nothing calls `work` with a transaction
    // already open.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting progress transaction: {e}"))?;
    for position in &recorded {
        state::put_progress_at(
            &tx,
            work_id,
            user_id,
            &position.place,
            position.updated.as_deref(),
        )?;
    }
    tx.commit()
        .map_err(|e| format!("committing progress for work {work_id}, user {user_id}: {e}"))
}

fn migrate_notes(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    sidecar_path: &Path,
) -> Result<Option<NotesOutcome>, String> {
    let path = sibling_path(sidecar_path, "notes.json");
    match step(conn, work_id, NOTES, &path, || {
        Ok(!state::notes(conn, work_id, user_id)?.notes.is_empty())
    })? {
        Step::Absent => Ok(None),
        Step::Kept => Ok(Some(NotesOutcome::AlreadyPresent)),
        Step::Unread => Ok(Some(NotesOutcome::Unread)),
        Step::Apply => Ok(Some(match apply_notes(conn, work_id, user_id, &path) {
            Ok(count) => {
                record_settled(conn, work_id, NOTES, true)?;
                NotesOutcome::Applied { count }
            }
            Err(e) => NotesOutcome::Malformed(e),
        })),
    }
}

fn apply_notes(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    path: &Path,
) -> Result<usize, String> {
    let text = read_sibling(path)?;
    let notes: Notes = serde_json::from_str(&text)
        .map_err(|e| format!("{}: not a readable notes document: {e}", path.display()))?;
    let count = notes.notes.len();
    state::put_notes(conn, work_id, user_id, &notes)?;
    Ok(count)
}

fn migrate_truth(
    conn: &Connection,
    work_id: i64,
    sidecar_path: &Path,
) -> Result<Option<TruthOutcome>, String> {
    let path = sibling_path(sidecar_path, "truth.json");
    match step(conn, work_id, TRUTH, &path, || {
        Ok(state::truth(conn, work_id)?.is_some())
    })? {
        Step::Absent => Ok(None),
        Step::Kept => Ok(Some(TruthOutcome::AlreadyPresent)),
        Step::Unread => Ok(Some(TruthOutcome::Unread)),
        Step::Apply => Ok(Some(match apply_truth(conn, work_id, &path) {
            Ok((pages, boxes)) => {
                record_settled(conn, work_id, TRUTH, true)?;
                TruthOutcome::Applied { pages, boxes }
            }
            Err(e) => TruthOutcome::Malformed(e),
        })),
    }
}

/// `TruthDoc::from_json` already refuses a box that lies off the page, so a
/// truth file corrupted in that specific way is `Malformed` here exactly as a
/// syntactically broken one is — both are "could not be trusted", and this
/// module does not need to tell them apart to do the right thing with either.
fn apply_truth(conn: &Connection, work_id: i64, path: &Path) -> Result<(usize, usize), String> {
    let text = read_sibling(path)?;
    let doc = TruthDoc::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let pages: usize = doc.editions.values().map(|e| e.pages.len()).sum();
    let boxes: usize = doc
        .editions
        .values()
        .flat_map(|e| e.pages.values())
        .map(Vec::len)
        .sum();
    state::put_truth(conn, work_id, &doc)?;
    Ok((pages, boxes))
}

fn migrate_library(
    conn: &Connection,
    work_id: i64,
    sidecar_path: &Path,
) -> Result<Option<LibraryOutcome>, String> {
    let path = sibling_path(sidecar_path, "library.json");
    match step(conn, work_id, LIBRARY, &path, || {
        Ok(!librarydoc::of(conn, work_id)?.is_empty())
    })? {
        Step::Absent => Ok(None),
        Step::Kept => Ok(Some(LibraryOutcome::AlreadyPresent)),
        Step::Unread => Ok(Some(LibraryOutcome::Unread)),
        Step::Apply => Ok(Some(match apply_library(conn, work_id, &path) {
            Ok(()) => {
                record_settled(conn, work_id, LIBRARY, true)?;
                LibraryOutcome::Applied
            }
            Err(e) => LibraryOutcome::Malformed(e),
        })),
    }
}

fn apply_library(conn: &Connection, work_id: i64, path: &Path) -> Result<(), String> {
    let text = read_sibling(path)?;
    let doc =
        librarydoc::LibraryDoc::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    librarydoc::apply(conn, work_id, &doc)
}

#[cfg(test)]
mod tests;
