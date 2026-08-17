//! Rows back into the five documents this project already has types for.
//!
//! The inverse of `import.rs`: where that module turns documents into rows,
//! this turns rows back into documents, so that "the sidecar remains the
//! interchange format" (design §4.5) is true rather than nominal. The
//! database is the working store; these five files are how a library leaves
//! it, and how it comes back if the database is ever lost.
//!
//! | File | Source |
//! | --- | --- |
//! | `taiyaku.json` | `project::sidecar` |
//! | `taiyaku.library.json` | `librarydoc::of` |
//! | `taiyaku.notes.json` | `state::notes` |
//! | `taiyaku.progress.json` | `state::progress` |
//! | `taiyaku.truth.json` | `state::truth` |
//!
//! **Explicit, not write-through.** The design rejected debouncing every
//! database write out to its sibling file: that reintroduces two writers for
//! one document, which is exactly the condition `Sidecar::merge_spoke`
//! exists to arbitrate (§4.2), and it would run that arbitration on every
//! keystroke rather than once per import. A caller decides when a library's
//! state is worth writing back out.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Migration 1's one seeded row (`schema/001_initial.sql`): `INSERT INTO
/// user (id, name, role) VALUES (1, 'local', 'admin')`. `state::notes` and
/// `state::progress` are per-user, but `taiyaku.notes.json` and
/// `taiyaku.progress.json` are not — those formats predate any notion of a
/// second reader, and spec A2 (the design this schema's own comment points
/// to: "the server behaves exactly as it does today: no login, no hashing,
/// no guards") has not landed yet. Until it does, "the" user a sidecar
/// speaks for is unambiguous: it is this row, the only one that exists.
/// Revisit this constant, not its call sites, once a library can hold more
/// than one real account.
///
/// `pub(crate)` rather than private: `migrate.rs`'s job is this module's
/// exact inverse (rows back into these same five documents, once, on first
/// import) and needs the identical user id for the identical reason — a
/// third copy of this constant, in the same crate, would be the thing this
/// doc comment already warns a second one elsewhere is.
pub(crate) const LOCAL_USER: i64 = 1;

/// What one work's export did to the filesystem: the documents written, the
/// stale ones taken away, and the ones it deliberately did not touch.
///
/// Three lists rather than one, because a caller has to be able to say which
/// happened. `taiyaku library export --in-place` writes over a reader's own
/// files, and "removed `taiyaku.truth.json`" is not a quieter form of "wrote
/// `taiyaku.truth.json`" — it is the more consequential of the two, and the
/// one a person would want to see named before they discovered it.
#[derive(Debug, Default)]
pub struct Exported {
    /// The documents written, in the fixed order [`work`]'s table lists.
    pub written: Vec<PathBuf>,
    /// The stale sibling documents deleted, in the same order — at most
    /// `<stem>.library.json`, `<stem>.progress.json` and `<stem>.truth.json`,
    /// the three whose state has a meaningful absence.
    pub removed: Vec<PathBuf>,
    /// The sibling documents left exactly as they were, because this library
    /// has never had their content: see [`work`]'s "What an export will not
    /// touch". Written first, then stale, each group in the same fixed order.
    /// Empty for a `--dir` export, which is writing somewhere the caller
    /// named rather than over a reader's own files.
    pub kept: Vec<PathBuf>,
}

/// Write as many of one work's five documents as it has state for, atomically,
/// and return the paths written and removed.
///
/// **The stem and directory both come from `sidecar_path`, unless overridden.**
/// `dir` — when given — replaces the directory the five documents land in,
/// with **one subdirectory per work, named for the work's `slug`**: they go
/// to `dir/<slug>/`, not to `dir/` itself. The filename stem still comes from
/// `sidecar_path`'s own file stem, falling back to `taiyaku` when there is
/// none — [`stem_of`], the one copy of that arithmetic this crate has, and
/// the convention this project documents for the file itself. That fallback
/// is also what fires when
/// `sidecar_path` is NULL altogether: there is no filename to take a stem
/// from, so the result is the same as a sidecar with no stem at all —
/// `taiyaku.json` and its four siblings.
///
/// **The subdirectory is why `--all --dir` does not destroy a library.**
/// Every work in a real library keeps its sidecar at `<dir>/taiyaku.json` —
/// that is the documented convention, and the reason a work's identity comes
/// from `unique_slug` rather than from its filename — so the stem is the
/// *same string* for every work. A flat `dir` therefore gave every work the
/// same five paths and each overwrote the one before it: three works in,
/// three files out, holding only the third, with `atomic::write` making each
/// individual write clean enough that nothing looked wrong (whole-branch
/// review 2, C1). Keying the
/// directory on `slug` — `UNIQUE` in the schema, so no two works can collide
/// — is what makes the paths distinct, and it keeps the property the way
/// back in needs: the five documents of one work stay siblings of *each
/// other*, sharing the `<stem>.notes.json` arithmetic `migrate.rs` finds
/// them by. The subdirectory is created if it does not exist, for the reason
/// `taiyaku-cli` already creates `--dir` itself: `taiyaku_read::atomic::write`
/// stages its temporary beside the target, so the directory has to exist
/// before the first document is written, and this is a directory the caller
/// never named and so cannot have created.
///
/// **A NULL `sidecar_path` with no `dir` is an error, not a panic or a
/// silent no-op.** Such a work — one that arrived by a route that never had
/// a path — has no directory of its own to default to, and writing nowhere
/// is not a legitimate outcome of a command whose entire job is writing
/// files. The error names the work by its slug so a caller can act on it
/// rather than guess which of possibly many works failed.
///
/// **Every document is computed before any file is touched.** A failure
/// projecting the sidecar, reading state, or serialising a document this way
/// changes nothing on disk. Once writing starts, each of the (at most) five
/// files is written through `taiyaku_read::atomic::write`, which makes each
/// individual file atomic — never a truncated half-write. There is no
/// transaction across the five files themselves: a later file failing to
/// write (a full disk, most plausibly) leaves the earlier ones written and
/// current, which is the same independence `progress.rs`, `notes.rs` and
/// `truth.rs` already have from each other and from the sidecar, both today
/// and before this crate existed. Coordinating five renames into one
/// all-or-nothing unit would need a mechanism this codebase does not have
/// anywhere else, to buy a guarantee export does not need: a document that
/// did write is never wrong for having written, only possibly out of step
/// with a sibling that did not.
///
/// **`taiyaku.notes.json` is always written, even when nobody has annotated
/// the work.** `state::notes` answers an empty document rather than `None`
/// for exactly this reason (`startup.ts`'s `getNotes`), so there is no
/// empty-versus-absent case for this function to invent on top of it.
/// `taiyaku.progress.json` and `taiyaku.truth.json` are the opposite: both
/// `state::progress` and `state::truth` return `Option`, and export honours
/// that distinction rather than papering over it — `None` means the file is
/// not written at all, the same convention `taiyaku-serve`'s own
/// `progress::load`/`truth::load` already read back ("a file that is not
/// there is `Ok(None)`... which is not a failure"). Neither document has a
/// meaningful empty form the way `Notes` does: a `Progress` with no position
/// to report is not a smaller `Progress`, it is the absence of one, and
/// `taiyaku.truth.json` already models "never labelled" as the whole
/// document being missing, not as `editions` being empty (`state::truth`'s
/// own doc comment). Writing an empty placeholder for either would assert a
/// fact — "this work has a recorded position", "someone opened the labelling
/// tool here" — that is not true.
///
/// **`taiyaku.library.json` joins those two rather than the always-written
/// pair, and that is a decision this branch made rather than inherited**
/// (whole-branch review 3, Minor 12). `librarydoc::of` always answers a
/// document, so this used to write one unconditionally — and a work nobody has
/// hand-edited projects to `{"taiyaku_library": 1, "updated": "..."}` and
/// nothing else, whose `updated` is stamped fresh on every run. Every
/// `--in-place` export therefore created a new, permanently-dirty file in
/// every work directory in the reader's library, one whose only field changed
/// each time it was written, in a project whose README promises the library is
/// not modified.
///
/// It is written when there is something to say and not written when there is
/// not, because an empty one says nothing a missing one does not. The
/// predicate is `LibraryDoc::is_empty`, which already exists and already means
/// this: `migrate.rs` reads it as "this work's hand-fixed metadata has never
/// been applied", and its own doc records that an applied-then-cleared
/// document and a never-touched one have no observable difference. So the
/// absent file and the empty file do not either — except on disk, where one of
/// them churns. `<stem>.library.json` is `state::progress`'s shape, not
/// `state::notes`': no consumer needs an empty one the way `getNotes` needs an
/// empty `Notes`, since nothing serves this document over HTTP at all.
///
/// The consequence is the one the other two already carry: a reader who
/// clears a work's volume, tags and series and exports has the file taken
/// away, because that is what a snapshot of nothing is — and, as ever, only
/// where this library had the file's content to begin with.
///
/// **And a stale one is removed, which is what makes this a snapshot rather
/// than an overlay.** Not writing `<stem>.truth.json` leaves whatever was
/// already at that path, so a reader who cleared their labelling and exported
/// found the old document still sitting beside the sidecar, asserting boxes
/// they had deleted. That file is inert against *this* library — `migrate.rs`
/// records the document as considered, so a later import will not read it back
/// in (whole-branch review 2, C3) — but the five documents are also how a
/// library travels: copy the directory to another machine, import it there,
/// and the deleted labelling comes back. So a document with no state left to
/// write has its file taken away rather than left, and the removal is
/// reported in [`Exported::removed`] rather than done quietly. A path that is
/// not there needs no removing and is not reported; a path that is there and
/// cannot be removed fails the export, since a stale document that outlived
/// the removal is the overlay this exists to prevent.
///
/// # What an export will not touch
///
/// **Only a sibling document this library has had the content of.** Writing
/// over a file and taking one away are the same claim — that the database is
/// the current copy of that document — and it is only true where the database
/// ever had it: read into rows on an import, or written out of them by an
/// export like this one. `migrate::absorbed` is that fact, and where it is
/// false and a file is sitting at the path, this leaves it alone and reports
/// it in [`Exported::kept`].
///
/// Three shapes reach that branch, all reproduced (whole-branch review 3, C1)
/// against a database that recorded only "this document is settled":
///
/// - a sibling **restored from a backup or copied back from another machine**
///   after the first import. `migrate` records every document as settled on
///   that first pass, including the ones with no file at all, so a file that
///   turns up later is never read — and was then deleted, or overwritten with
///   an empty document, by the next routine export. `taiyaku serve
///   <sidecar>...` imports every path on every start, so the marker is there
///   after the first start a work is ever served.
/// - a **malformed** sibling, which records nothing at all, so that fixing it
///   and importing again works — the recovery path `migrate`'s own doc and
///   design §4.3 both promise. A single character typed into a
///   `taiyaku.truth.json` plus one export used to be enough to end it.
/// - a sibling **left unread because the rows already held state** for that
///   document. The rows win, which is what makes this a migration rather than
///   a sync; that is not a reason to destroy a file nothing has read.
///
/// The escape from all three is the same and is the reader's to take: move
/// the file aside, and the next export writes a fresh one from the rows.
///
/// **Writing one claims it**, so an export is not one-shot: the document that
/// lands is this library's from then on, which is also what stops the next
/// import reading it straight back over the rows it came from. Removing one
/// gives the claim up again — nothing of that document is held on either side
/// afterwards, so a file that appears at that path later is once more
/// something this library has never seen.
///
/// **`--dir` is exempt**, and both halves are: no file there is checked, and
/// no marker is written. Those paths are a destination a caller named for
/// output, not the reader's own documents, and the markers describe the
/// siblings beside the sidecar rather than whatever is under some `DIR/slug/`.
///
/// The returned paths are in the fixed order the table above lists, for
/// however many of the five documents were actually written.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if `sidecar_path` is NULL
/// and `dir` is `None`, if the per-work subdirectory under `dir` cannot be
/// created, if projecting the sidecar or reading any state fails, if a
/// document fails to serialise, if any write fails — see
/// `taiyaku_read::atomic::write` — if a stale document exists and cannot be
/// removed, or if reading or writing a `migrated_document` marker fails.
pub fn work(conn: &Connection, work_id: i64, dir: Option<&Path>) -> Result<Exported, String> {
    let (slug, sidecar_path) = work_row(conn, work_id)?;
    let target = target_dir(dir, sidecar_path.as_deref(), &slug, work_id)?;
    let stem = stem_of(sidecar_path.as_deref());

    let sidecar = crate::project::sidecar(conn, work_id)?;
    let library_doc = crate::librarydoc::of(conn, work_id)?;
    let notes = crate::state::notes(conn, work_id, LOCAL_USER)?;
    let progress = crate::state::progress(conn, work_id, LOCAL_USER)?;
    let truth = crate::state::truth(conn, work_id)?;

    let notes_json = serde_json::to_string_pretty(&notes)
        .map_err(|e| format!("serialising notes for work `{slug}`: {e}"))?;
    let progress_json = progress
        .as_ref()
        .map(|p| {
            serde_json::to_string_pretty(p)
                .map_err(|e| format!("serialising progress for work `{slug}`: {e}"))
        })
        .transpose()?;

    // Fixed order, matching the table in this module's doc comment: the
    // sidecar test that checks this byte-for-byte relies on `taiyaku.json`
    // being first, and `exporting_writes_five_documents_beside_the_sidecar`
    // relies on this exact ordering.
    //
    // The middle field is which `migrated_document` this document answers to,
    // and `taiyaku.json` has none: the sidecar is what an import reads by
    // definition, and `--in-place` writing over it is that flag's whole point
    // (`taiyaku-cli`'s own refusal without it is where that decision is made).
    let mut documents: Vec<(&str, Option<&str>, String)> = vec![("json", None, sidecar.to_json())];
    // The three whose state has a meaningful absence, each paired with whether
    // this export has a document for it. The `false` arm is what gets removed
    // below; keeping the pair explicit is what stops the write list and the
    // removal list from drifting apart into two different ideas of which
    // documents are optional. Both lists stay in the fixed order the table
    // above gives, which is why `library.json` is decided before `notes.json`
    // is pushed rather than after.
    let mut stale: Vec<(&str, &str)> = Vec::new();
    if library_doc.is_empty() {
        stale.push(("library.json", crate::migrate::LIBRARY));
    } else {
        documents.push((
            "library.json",
            Some(crate::migrate::LIBRARY),
            library_doc.to_json(),
        ));
    }
    documents.push(("notes.json", Some(crate::migrate::NOTES), notes_json));
    if let Some(text) = progress_json {
        documents.push(("progress.json", Some(crate::migrate::PROGRESS), text));
    } else {
        stale.push(("progress.json", crate::migrate::PROGRESS));
    }
    if let Some(doc) = &truth {
        documents.push(("truth.json", Some(crate::migrate::TRUTH), doc.to_json()));
    } else {
        stale.push(("truth.json", crate::migrate::TRUTH));
    }

    // After every document is in hand, so a projection that failed still
    // leaves the filesystem as it found it — and only when `dir` was given,
    // since the in-place target is `sidecar_path`'s own parent, which exists
    // by construction.
    if dir.is_some() {
        std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    }

    // Only an in-place export is writing over the reader's own documents, and
    // only there do the markers describe these paths. See "What an export
    // will not touch".
    let in_place = dir.is_none();
    let mut written = Vec::with_capacity(documents.len());
    let mut kept = Vec::new();
    for (suffix, document, text) in &documents {
        let path = target.join(format!("{stem}.{suffix}"));
        let claim = if in_place { *document } else { None };
        if let Some(document) = claim
            && path.is_file()
            && !crate::migrate::absorbed(conn, work_id, document)?
        {
            kept.push(path);
            continue;
        }
        taiyaku_read::atomic::write(&path, text.as_bytes(), "json")?;
        if let Some(document) = claim {
            crate::migrate::record_absorbed(conn, work_id, document, true)?;
        }
        written.push(path);
    }

    // Last, after every write has succeeded: a removal is the one step here
    // that cannot be undone by running the command again, so it happens only
    // once the documents that replace this work's state on disk are all
    // present.
    let mut removed = Vec::with_capacity(stale.len());
    for (suffix, document) in stale {
        let path = target.join(format!("{stem}.{suffix}"));
        if in_place && path.is_file() && !crate::migrate::absorbed(conn, work_id, document)? {
            kept.push(path);
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {
                if in_place {
                    crate::migrate::record_absorbed(conn, work_id, document, false)?;
                }
                removed.push(path);
            }
            // Nothing there is the ordinary case — most works have never had
            // any of these three documents — and is not a removal to report.
            // Every *other* failure is, and is fatal: a stale document that
            // could not be taken away is still sitting beside a sidecar
            // asserting state the database no longer holds, which is exactly
            // the overlay this removal exists to prevent, so swallowing the
            // error would leave the caller told a snapshot had been taken when
            // it had not.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "removing the stale {} for work `{slug}`: {e}",
                    path.display()
                ));
            }
        }
    }

    Ok(Exported {
        written,
        removed,
        kept,
    })
}

fn work_row(conn: &Connection, work_id: i64) -> Result<(String, Option<String>), String> {
    conn.query_row(
        "SELECT slug, sidecar_path FROM work WHERE id = ?1",
        [work_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|e| format!("reading work {work_id}: {e}"))
}

/// The directory the five documents land in: `dir/<slug>` if the caller gave
/// a `dir`, otherwise `sidecar_path`'s own parent. Neither is available for a
/// work whose `sidecar_path` is NULL and whose caller passed no `dir` — see
/// this module's `work` doc comment for why that is refused rather than
/// guessed, and for why `dir` gains a per-work subdirectory rather than
/// taking the five documents directly.
fn target_dir(
    dir: Option<&Path>,
    sidecar_path: Option<&str>,
    slug: &str,
    work_id: i64,
) -> Result<PathBuf, String> {
    if let Some(d) = dir {
        return Ok(d.join(slug));
    }
    match sidecar_path {
        Some(p) => Ok(Path::new(p)
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)),
        None => Err(format!(
            "work `{slug}` (id {work_id}) has no sidecar_path on record, so export has \
             nowhere to write its documents; pass --dir to say where"
        )),
    }
}

/// The filename stem every document shares: `sidecar_path`'s own file stem,
/// falling back to `taiyaku` when there is none — including when
/// `sidecar_path` itself is NULL, the convention this project documents for
/// the file itself.
///
/// `pub(crate)`: `migrate.rs` needs the identical arithmetic to find a
/// sidecar's siblings on the way *in*, and reuses this rather than carrying a
/// second copy. It was written twice within this crate's reach until
/// `taiyaku-serve`'s `Work::sibling` — the doc comment that first warned about
/// the duplication — was deleted for having no callers at all (whole-branch
/// review 3, Minor 8).
///
/// **Three copies remain, not two.** `taiyaku_core::RegionDoc::path_beside`
/// and `TruthDoc::path_beside` are the same arithmetic in a crate this one
/// depends on, but whose types carry the paths themselves rather than a stem;
/// each names it. The third is `taiyaku-serve`'s `stump::cache::path_for`,
/// which is `file_stem`-plus-`format!` for `<stem>.stump.json` — and is
/// precisely the copy the deleted `Work::sibling` comment was warning about,
/// since two of the three files it listed have moved into this database and
/// that one has not (delta review M7). It is in a crate this one cannot depend
/// on, which is why the three stay three; being counted is the whole of what
/// this comment can do about it.
pub(crate) fn stem_of(sidecar_path: Option<&str>) -> String {
    sidecar_path
        .map(Path::new)
        .and_then(Path::file_stem)
        .map_or_else(
            || "taiyaku".to_string(),
            |s| s.to_string_lossy().to_string(),
        )
}

#[cfg(test)]
mod tests;
