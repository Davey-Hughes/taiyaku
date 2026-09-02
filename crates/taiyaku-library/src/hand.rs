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
/// same marker `derive::write_work`'s own read-back arms, and it means one
/// thing only: **this work's document has been read back from disk, so do not
/// read it again.**
///
/// *It used to mean a second thing — "do not overwrite `work.series_id`,
/// `work.volume`, `edition.kind` and `edition.language`" — and conflating the
/// two is what O80 was: a title-only Save armed it and froze three columns
/// nobody had touched. Migration 011 answers that per field with a `_hand`
/// column each, and `derive` writes `COALESCE(<hand>, <derived>)`, so the
/// marker carries none of it now.*
///
/// An edit made against a
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

/// Move this work's document from the address it had under `old_key` to the
/// one it has now.
///
/// **Called from `derive::write_work`, immediately after `write_roots`.** An
/// earlier version called this from `import_with`, right after `tx.commit()`
/// — the moment that looks like it should work, since the old `content_key`
/// is still readable from the row just replaced. It cannot: for a
/// single-member work a repack changes the member's `sha256`, and
/// `import::replace_editions`'s delete-and-reinsert leaves that member's
/// `root_id` NULL there, because `held_member_meta` restores it keyed on
/// `sha256` — exactly what changed — so the lookup misses. [`path_for`] then
/// answers `None` and the rename is silently skipped. `write_roots` restores
/// `root_id` by `(edition key, ordinal)` instead, and runs after
/// `import::scanned` returns — so the call from `derive::write_work`, after
/// `write_roots`, is the first point at which this can resolve an address at
/// all. Once `import::scanned` commits, the old key is gone from the row, and
/// since O80 closed a title-only document carries no series, no volume and no
/// member digests either — so nothing can match the file to a work once it
/// strands. See `derive.rs`'s own comment at the call site for the full
/// account, and see [`Orphan`] for what a stranded document then looks like.
///
/// **Never overwrites.** Two works cannot legitimately share an address, so a
/// file already at the target means something this was not designed for;
/// `false` comes back, the correction under the old key survives, and
/// [`reconcile`] reports it as an orphan for a person to read.
///
/// # Errors
///
/// Returns an error if SQLite fails resolving the new address. **Not** if the
/// rename fails: a root that cannot take the move leaves the document where it
/// is, which [`reconcile`] then reports.
pub fn rename_for_rekey(conn: &Connection, work_id: i64, old_key: &str) -> Result<bool, String> {
    let Some(new_path) = path_for(conn, work_id)? else {
        return Ok(false);
    };
    let Some(dir) = new_path.parent() else {
        return Ok(false);
    };
    let old_path = dir.join(format!("{old_key}.library.json"));
    if !old_path.is_file() || new_path.exists() {
        return Ok(false);
    }
    Ok(std::fs::rename(&old_path, &new_path).is_ok())
}

/// A document under `<root>/.taiyaku` that no work in the library addresses.
///
/// **The title travels with the path because the path is a hash.** The
/// filename is the `content_key` the document was written under, which names
/// content that no longer exists; and since O80 closed (#97, *the document
/// projects only what a person set*) a title-only document — the common case,
/// and the one the metadata editor's single control produces — carries no
/// series, no volume and no member digests either. So there is no route back
/// to a work from the file's contents, and a report naming only the path
/// leaves a person nothing to act on. The title is the one actionable thing
/// in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub path: PathBuf,
    /// `None` for a document that carries no title — one correcting only a
    /// series or an edition label. Reported anyway: the file is still
    /// somebody's work and still unreachable.
    pub title: Option<String>,
    /// The work that absorbed the one this document belonged to, when a fold
    /// is why it is orphaned.
    ///
    /// **Named, never moved.** `fold::absorb` could instead have carried the
    /// orphaned document itself onto the target — but the target work may
    /// already hold its own hand document, and a fold has no basis for
    /// deciding whose correction is right; `fold::absorb`'s own doc comment
    /// refuses a colliding note id for the identical reason, "the id is one
    /// the reader's own sidecar names, and rewriting it silently is worse
    /// than declining." A hand-set title is the same kind of thing. So this
    /// carries only where to look — `reconcile` cannot fill it in, because it
    /// runs per root and never sees a fold; only [`crate::derive::derive`]
    /// watches both the fold and the sweep, and it is what sets this field.
    pub absorbed_into: Option<Absorbed>,
}

/// The work an orphaned document's own work folded into — see
/// [`Orphan::absorbed_into`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Absorbed {
    /// What a person types to go and look: `work.slug`, which is stable
    /// across a rescan the way the folded work's own `content_key` was not.
    pub slug: String,
    /// What a person recognises: `work.title`, printed alongside the slug
    /// because a slug alone does not say which volume this was.
    pub title: String,
}

/// What one root's `.taiyaku` directory and the works that owe it documents
/// had to say about each other.
///
/// Three states, three answers, and only one of them is a repair — see the
/// design's §2 for why the stale case resolves to nothing. `repaired` and
/// `unwritable` partition the gaps; `orphans` and `stale` are reported and
/// touched by nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Gaps closed on this pass: a marked work whose document was missing and
    /// has now been written.
    pub repaired: usize,
    /// Gaps that could not be closed, each carrying the `io::Error`'s own
    /// message naming the path — [`Saved::why`]'s shape, for the same reason.
    ///
    /// **A permanently read-only root reports the same list every scan, and
    /// that is correct rather than noisy**: the condition really is still
    /// true, and the alternative — reporting it once and going quiet — would
    /// leave a library whose corrections are not durable looking like one
    /// whose corrections are.
    pub unwritable: Vec<String>,
    /// Documents no work addresses. See [`Orphan`] for why each carries a
    /// title.
    pub orphans: Vec<Orphan>,
    /// Documents that exist at a work's address and disagree with the
    /// database. **Reported and not resolved**: the database is not
    /// automatically right, since O44 records this as the one document this
    /// project designs for hand-editing.
    pub stale: Vec<String>,
}

/// Reconcile one root's `.taiyaku` directory against the works that owe it a
/// document.
///
/// **One `read_dir` per root, and it runs inside `derive`** — which already
/// walks every work and already calls [`load`] per work, so this adds one
/// directory listing to a pass that is hashing archives.
///
/// **`absorbed = 1` is the register of works that owe a document**, not the
/// presence of a file: `save` records the marker whether or not the write
/// landed, which is exactly what makes a gap detectable at all.
///
/// # Errors
///
/// Returns an error if the root's directory cannot be listed for a reason
/// other than its absence or a non-directory occupying its name, or if SQLite
/// fails. **Not** if a document cannot be written — that is
/// [`Reconciled::unwritable`].
pub fn reconcile(conn: &Connection, root: &crate::scan::Root) -> Result<Reconciled, String> {
    let mut found = Reconciled::default();
    let dir = root.path.join(DIR);

    // Every work that owes a document, with the address it owes it at.
    // Addresses outside this root belong to another root's pass — a work
    // spans roots by design, and `path_for` answers with exactly one of them.
    let mut expected: std::collections::HashMap<PathBuf, i64> = std::collections::HashMap::new();
    let mut owed: Vec<i64> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT work_id FROM migrated_document
                  WHERE document = ?1 AND absorbed = 1
                  ORDER BY work_id",
            )
            .map_err(|e| format!("preparing the marked-work query: {e}"))?;
        let rows = stmt
            .query_map([crate::migrate::LIBRARY], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("reading marked works: {e}"))?;
        for row in rows {
            owed.push(row.map_err(|e| format!("reading a marked work: {e}"))?);
        }
    }
    for work_id in owed {
        let Some(path) = path_for(conn, work_id)? else {
            // No address at all — a sidecar-imported work, whose document
            // lives beside its sidecar and is `export::work`'s to write. Out
            // of scope by design; see the design's §7.
            continue;
        };
        if path.parent() == Some(dir.as_path()) {
            expected.insert(path, work_id);
        }
    }

    // What is actually on disk under `.taiyaku`, and the orphans among it.
    //
    // **`NotFound` and `NotADirectory` both mean "nothing is listable here,"
    // not "reconcile cannot proceed" — so both fall through to the
    // gap-repair loop below with `present` left empty, rather than an early
    // return** (the brief's own sketch of this function returned early on
    // `NotFound` alone, which is what the note on the `read_dir` early
    // return flagged as possibly not typechecking; doing so also skips the
    // gap-repair loop entirely, which is wrong for a root whose obstruction
    // has just been cleared — see `a_gap_is_written_on_the_next_pass`). A
    // root that has never taken a hand edit has no `.taiyaku` at all — the
    // ordinary case, since `save` creates it on first write. A plain file
    // occupying that name — this module's own fixture for a root that
    // cannot take a document, per this task's constraint against
    // `set_readonly` — cannot contain a document either, by construction:
    // nothing was ever able to write beneath a name that is not a
    // directory. It is `save`'s own attempt below, not this listing, that
    // discovers and names the real failure for that case. Any other error
    // (permission denied, and so on) is genuinely unexpected and is not
    // folded in the same way: swallowing it would let the loop below
    // "repair" — overwrite — a document this listing merely failed to see.
    let mut present: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
                let path = entry.path();
                if !path.to_string_lossy().ends_with(".library.json") {
                    continue;
                }
                present.insert(path.clone());
                if let Some(&work_id) = expected.get(&path) {
                    // Present and addressed. The remaining question is
                    // whether it agrees with the database.
                    //
                    // **`updated` is normalised before the comparison, and
                    // must be.** `librarydoc::of`'s own doc comment records
                    // that it is stamped with the time the projection was
                    // taken and that nothing in the schema holds it — so
                    // comparing it would call every reachable document
                    // stale, including one written a moment ago.
                    //
                    // A file that cannot be read or parsed is **not**
                    // stale: it is already `DeriveReport::unreadable_hand`'s,
                    // which the read-back populates and this does not
                    // duplicate.
                    let Ok(text) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let Ok(mut on_disk) = crate::librarydoc::LibraryDoc::from_json(&text) else {
                        continue;
                    };
                    let mut projected = crate::librarydoc::of(conn, work_id)?;
                    on_disk.updated.clone_from(&projected.updated);

                    // **`members` excludes `ChapterSource::Filename` before
                    // the comparison, or every scan would manufacture a
                    // false disagreement.** `derive::write_chapters`'s own
                    // UPDATE is scoped `WHERE chapter_source = 'filename'`
                    // (its own doc comment names that clause) — it writes a
                    // fresh `Filename` chapter from every member's name on
                    // every scan, and never touches a `Hand` or `Comicinfo`
                    // row. Comparing an unfiltered `Filename` chapter would
                    // report that routine derivation — a work simply gaining
                    // a chapter, or having its filename-derived numbering
                    // shift — as a person's disagreement, on the most
                    // ordinary thing a library does.
                    //
                    // `Hand` and `Comicinfo` both stay in the comparison:
                    // a re-derive is scoped never to overwrite either one —
                    // `taiyaku-serve/src/metadata.rs` states this directly,
                    // "'hand' and 'comicinfo' are the ones a re-derive is
                    // scoped never to overwrite" — so neither can churn the
                    // way `Filename` does. Excluding `Comicinfo` too was the
                    // filter's first shape, and it was wrong: a person can
                    // hand-write `"chapter_source": "comicinfo"` into the
                    // document directly — precisely the hand-editing this
                    // module exists to serve — and that would have left it
                    // diverging from the database with nothing ever reporting
                    // it stale.
                    //
                    // Excluding `Filename`, not dropping `members` outright:
                    // dropping the `members` block from the comparison
                    // entirely was the other option, and it loses the one
                    // case this module exists to catch — a hand-set chapter
                    // that really was overwritten must still be reported, not
                    // silently lost.
                    //
                    // This narrows the comparison; it does not fix the
                    // cause. That `of` projects derived chapters into the
                    // document at all is unfinished business left open when
                    // O80 closed (#97, "the document projects only what a
                    // person set"), and is deliberately not addressed here.
                    let not_filename = |m: &crate::librarydoc::MemberMeta| {
                        !matches!(
                            m.chapter_source,
                            Some(crate::librarydoc::ChapterSource::Filename)
                        )
                    };
                    on_disk.members.retain(not_filename);
                    projected.members.retain(not_filename);

                    if on_disk != projected {
                        found.stale.push(path.display().to_string());
                    }
                    continue;
                }
                // Unreachable. Read the title out of it if we can; a
                // document we cannot parse is still an orphan, just one
                // with less to say.
                let title = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| crate::librarydoc::LibraryDoc::from_json(&text).ok())
                    .and_then(|doc| doc.title);
                // **Always `None` here.** This function runs per root and
                // never sees a fold — [`crate::derive::derive`] is the only
                // place both the fold and this sweep are in view, and it
                // fills the field in afterwards. Changing that would mean
                // `reconcile` learning about folds, which its own signature
                // is not allowed to.
                found.orphans.push(Orphan {
                    path,
                    title,
                    absorbed_into: None,
                });
            }
        }
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) => {}
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    }

    for (path, work_id) in &expected {
        if present.contains(path) {
            continue;
        }
        // **A gap, and this is the retry O82 says does not exist.** Written
        // through `save` rather than a second writer, so the marker, the
        // empty-document removal and the failure shape are all the ones the
        // PATCH routes already produce.
        let saved = save(conn, *work_id)?;
        if saved.persisted {
            // **`persisted` alone is not "written".** `save`'s remove-file
            // branch answers `persisted: true` for `ErrorKind::NotFound` too
            // — "nothing there is the ordinary case, not a failure", by that
            // branch's own doc comment — and a marked work whose projection
            // has since gone empty takes exactly that branch here: this loop
            // only runs for a `path` absent from `present`, so there was
            // never a file for `remove_file` to find, and `persisted: true`
            // reports a no-op rather than a write. Counting it as a repair
            // would print "N document(s) written" for a pass that wrote
            // none — a false claim in a feature whose whole point is honest
            // reporting.
            //
            // Re-projecting is the narrowest correct check: `save`'s
            // internal `match` only reaches its write arm for a non-empty
            // document, so `!doc.is_empty()` is exactly the condition that
            // arm already required. Checking `path.is_file()` after the
            // fact was the alternative and would answer the same question
            // less directly — through a second filesystem read of
            // something this function already knows from the database.
            if !crate::librarydoc::of(conn, *work_id)?.is_empty() {
                found.repaired += 1;
            }
        } else {
            found.unwritable.push(
                saved
                    .why
                    .unwrap_or_else(|| format!("{}: could not be written", path.display())),
            );
        }
    }

    // **Sorted, all three, before this returns.** `expected` is a `HashMap`,
    // and Rust reseeds its SipHash key every process start, so `unwritable` —
    // built by iterating `&expected` above — printed a root's gaps in a
    // different order on every scan, even one where nothing had changed.
    // `orphans` and `stale` already come out in `read_dir`'s order, which is
    // arbitrary but stable for an unchanged directory; sorting them too costs
    // one comparison per entry against a pass that is already hashing
    // archives, and it is what the one thing a person actually does with two
    // scans — diff their output — needs: identical text for an unchanged
    // library, rather than a reordering that reads as a change nothing made.
    found.unwritable.sort();
    found.orphans.sort_by(|a, b| a.path.cmp(&b.path));
    found.stale.sort();

    Ok(found)
}

#[cfg(test)]
mod tests;
