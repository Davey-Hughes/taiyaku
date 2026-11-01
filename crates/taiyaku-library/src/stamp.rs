//! Spec C3 §6: the one feature that writes to the user's library.
//!
//! Designed in
//! `docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md`, whose
//! gates are the store spec's §13.2. Every decision that matters is recorded
//! there; what is here is the order they run in.
//!
//! **A plan is computed by reading, not remembered in a column.** Reading each
//! archive's existing document stays true across a database rebuild and across
//! a hand-edit, neither of which a column survives. §1.3.

use std::path::PathBuf;

use rusqlite::Connection;

/// The entry a stamp writes. Matched case-insensitively when replacing, and
/// written under exactly this spelling.
pub const DOCUMENT: &str = "ComicInfo.xml";

/// One archive a stamp would rewrite, as the person confirming it sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub member_id: i64,
    pub edition_key: String,
    /// Absolute, resolved the way `download::member_paths` resolves one.
    pub path: PathBuf,
    /// The archive as it is now — the number on the confirmation.
    pub bytes: u64,
}

/// Which of this work's archives would change, and nothing else.
///
/// Reads each zip member's existing `ComicInfo.xml` and keeps the member only
/// when those bytes differ from what [`crate::comicinfo::xml`] renders now. A
/// re-stamp after a one-field edit therefore names the archives that edit
/// reached, and a stamp with nothing to do costs a central-directory read per
/// member rather than a copy of the whole work.
///
/// **`rar` and `7z` never appear**, per C3 §6 — only zip is written, and the
/// feature is absent rather than disabled for the rest. The test is what a
/// member *is*, not which two names it is not.
///
/// **The format is sniffed here rather than read from `member.format`, and
/// rather than joined back to `scan_entry`.** Two reasons, in that order.
/// `member.format` is never written by anything: the repository's only
/// `INSERT INTO member` — `import.rs`'s — lists
/// `(edition_id, ordinal, path, sha256, pages)` and no format, and no
/// `UPDATE` reaching those rows sets `format`. `proposals.rs`'s
/// `edition_formats` records the same finding. A filter on that column would
/// make this feature fire on nothing, in every library.
///
/// `edition_formats` answers it by joining `scan_entry` on
/// `(root_id, rel_path)`, and **this deliberately does not**, because the two
/// are answering different questions. That function reports a historical
/// fact — what these members were *walked* as — and says so; a member with no
/// root contributes nothing to it, which is right for a report. This function
/// is deciding whether to **rewrite the file it is looking at**, so what the
/// bytes are *now* is the only answer that can be trusted, a cached token
/// from an earlier walk is exactly the staleness hazard, and an imported work
/// (`root_id` NULL by design, per `schema/006_member_root.sql`) must not be
/// silently unstampable. [`taiyaku_read::sniff`] is the same measurement the
/// scanner makes, made at the moment it matters. It is not the suffix guess
/// `edition_formats` rejects: `format.rs`'s own opening line is that
/// `scan_entry.format` is sniffed, never derived from a suffix.
///
/// The cost is one open per member, which this function was already paying to
/// read the existing document.
///
/// **`PageCount` is the member's own page count**, not the edition's total. A
/// `ComicInfo.xml` describes the archive it sits in, so an edition split
/// across three archives gets three different counts. The download's own call
/// passes the count of the single file it is generating, which is the same
/// rule and not the same number.
///
/// # Errors
///
/// Returns an error if the rows cannot be read, or if a zip archive cannot
/// be opened; a member whose file cannot be read is skipped.
pub fn plan(conn: &Connection, work_id: i64) -> Result<Vec<Planned>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.path, m.pages, e.key, w.slug, r.path
               FROM member m
               JOIN edition e ON e.id = m.edition_id
               JOIN work w    ON w.id = e.work_id
               LEFT JOIN root r ON r.id = m.root_id
              WHERE e.work_id = ?1
              ORDER BY e.key, m.ordinal",
        )
        .map_err(|e| format!("preparing this work's members: {e}"))?;

    type Row = (i64, String, u32, String, String, Option<String>);
    let rows: Vec<Row> = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .map_err(|e| format!("reading this work's members: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading this work's members: {e}"))?;

    let mut planned = Vec::new();
    for (member_id, member_path, pages, edition_key, slug, root) in rows {
        let path = resolve(&member_path, root.as_deref());
        // **A member whose bytes cannot be read is skipped, not fatal.** A
        // sniff opens the file, so one member gone from disk (O62's case)
        // would fail the plan for the whole work — including for the archives
        // beside it that are perfectly stampable. A rar that vanished is not
        // a reason the zip next to it cannot carry its metadata.
        //
        // It is not silent: `member_paths` returns every member, and the
        // route's `skipped_formats` reports an unreadable one as `unreadable`
        // beside the `rar` and `7z` it names — so the screen still says the
        // work has an archive this will not touch, and why.
        let Ok(format) = taiyaku_read::sniff(&path) else {
            continue;
        };
        if format != taiyaku_read::Format::Zip {
            continue;
        }

        let rendered = {
            let info = crate::comicinfo::read(conn, &slug, &edition_key, pages)?;
            crate::comicinfo::xml(&info).into_bytes()
        };
        if taiyaku_read::member_entry(&path, DOCUMENT)?.as_deref() == Some(rendered.as_slice()) {
            continue;
        }

        let bytes = std::fs::metadata(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len();

        planned.push(Planned {
            member_id,
            edition_key,
            path,
            bytes,
        });
    }
    Ok(planned)
}

/// What one stamp changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// `(member id, its new sha256)`, in the order the plan named them.
    pub members: Vec<(i64, String)>,
    /// `Some` when the hub edition moved and the work's identity with it.
    pub content_key: Option<String>,
    /// What [`crate::hand::rename_for_rekey`] answered.
    pub document_moved: bool,
    /// Whether a sidecar was rewritten. `false` for a scanned work, which
    /// has none — see [`apply`].
    pub sidecar_rewritten: bool,
}

/// Rewrite this work's planned archives and bring the database up to them.
///
/// **`expected` is compared field for field against a freshly derived plan,
/// and a mismatch refuses.** Not a count and not a set of member ids: the
/// case this guard exists for is an archive that grew between the plan and
/// the confirmation, which is the same member at the same path, and whose
/// size is the number the person read before agreeing. C3 §3's principle —
/// the server writes what the client was shown — applied to the one route
/// in this product that rewrites files.
///
/// **Stage one, rename one, then one transaction.** Peak extra disk is one
/// archive rather than the whole work, on a machine chosen for being small.
/// The cost is a window: a crash after some renames leaves those files stamped
/// and their rows stale, which is exactly what `member.sha256` exists to
/// detect — the scanner reports the work stale and a rescan reconciles it. The
/// design's §1.4 records why the two alternatives lost.
///
/// **The file steps cannot join the transaction, and this does not pretend
/// otherwise.** §13.2 asks that the new `sha256`, the sidecar and
/// `content_key` move "in one transaction, or the write does not happen". The
/// database half does. A rename is not a SQLite statement.
///
/// **The sidecar is rewritten only when there is one.** `export::work` fails
/// when `work.sidecar_path` is NULL and no directory is given — its own
/// documented behaviour — and a *scanned* work's is always NULL. Re-exporting
/// unconditionally would make the stamp fail on exactly the works the scanner
/// produces, which is most of a real library; there is also nothing to bring
/// back in line, because a work with no sidecar has no file recording the
/// digest that just moved.
///
/// # Errors
///
/// Returns an error if the plan no longer holds, if any archive cannot be
/// rewritten, or if the transaction fails. **An error rewriting an archive
/// after others have been renamed says how many**, because those files are
/// stamped and their rows are not: the database is behind the disk until a
/// rescan, and a message that did not say so would leave no trace of it.
pub fn apply(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    expected: &[Planned],
) -> Result<Applied, String> {
    let current = plan(conn, work_id)?;
    if current.as_slice() != expected {
        return Err(format!(
            "this work's archives have changed since the plan was made \
             ({} then, {} now); nothing was written",
            expected.len(),
            current.len()
        ));
    }

    // **Nothing to do is done, not performed.** An empty plan matches an empty
    // confirmation and is accepted rather than refused — "this work is already
    // stamped" is a success — but everything below writes, and there is
    // nothing here to write about. Falling through would re-export the sidecar
    // and its siblings on every repeat, and `export::work` is scoped to the
    // acting user, so a no-op stamp would republish one person's view of the
    // work's documents each time it was pressed.
    if current.is_empty() {
        return Ok(Applied {
            members: Vec::new(),
            content_key: None,
            document_moved: false,
            sidecar_rewritten: false,
        });
    }

    let (old_key, sidecar_path): (String, Option<String>) = conn
        .query_row(
            "SELECT content_key, sidecar_path FROM work WHERE id = ?1",
            [work_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("reading work {work_id}: {e}"))?;

    let mut written: Vec<(i64, String)> = Vec::new();
    for item in &current {
        let pages: u32 = conn
            .query_row(
                "SELECT pages FROM member WHERE id = ?1",
                [item.member_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("reading member {}: {e}", item.member_id))?;
        let slug: String = conn
            .query_row("SELECT slug FROM work WHERE id = ?1", [work_id], |row| {
                row.get(0)
            })
            .map_err(|e| format!("reading work {work_id}: {e}"))?;
        let info = crate::comicinfo::read(conn, &slug, &item.edition_key, pages)?;
        let document = crate::comicinfo::xml(&info).into_bytes();

        let digest = rewrite_one(&item.path, &document).map_err(|e| {
            format!(
                "{e} — {} of this work's archives were rewritten before this and \
                 their rows are now stale; a rescan reconciles them",
                written.len()
            )
        })?;
        written.push((item.member_id, digest));
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the transaction for this stamp: {e}"))?;
    for (member_id, digest) in &written {
        tx.execute(
            "UPDATE member SET sha256 = ?1 WHERE id = ?2",
            rusqlite::params![digest, member_id],
        )
        .map_err(|e| format!("recording the new digest of member {member_id}: {e}"))?;
    }

    // Derived from the rows just written, so the hash lives in
    // `identity::content_key` alone and is not restated here.
    let new_key = crate::identity::content_key(&crate::project::sidecar(&tx, work_id)?)?;
    let moved_key = (new_key != old_key).then(|| new_key.clone());
    if moved_key.is_some() {
        tx.execute(
            "UPDATE work SET content_key = ?1 WHERE id = ?2",
            rusqlite::params![new_key, work_id],
        )
        .map_err(|e| format!("recording the new content key of work {work_id}: {e}"))?;
    }
    tx.commit()
        .map_err(|e| format!("committing this stamp: {e}"))?;

    // Both of these project the rows the transaction just committed, so they
    // run after it rather than inside it.
    let document_moved = match &moved_key {
        Some(_) => crate::hand::rename_for_rekey(conn, work_id, &old_key)?,
        None => false,
    };
    let sidecar_rewritten = sidecar_path.is_some();
    if sidecar_rewritten {
        crate::export::work(conn, work_id, user_id, None)?;
    }

    Ok(Applied {
        members: written,
        content_key: moved_key,
        document_moved,
        sidecar_rewritten,
    })
}

/// Rewrite one archive with `document` in place of any `ComicInfo.xml` it
/// holds, and answer the new file's sha256.
///
/// **Every other entry is `raw_copy_file`d**, so its stored bytes and its
/// compression method survive untouched. Two reasons, and neither is
/// tidiness: re-encoding would rewrite every page of a file the user owns,
/// and `STORE` — which `cbz.rs` chose for the *generated* download so that
/// `Content-Length` is exact before the first byte — would inflate most of
/// this corpus's entries, turning a metadata edit into a size increase across
/// the library.
///
/// **The document is matched case-insensitively and written under
/// [`DOCUMENT`]'s spelling.** An archive carrying `comicinfo.xml` has one
/// document, not a different file that would leave two after a rewrite —
/// `taiyaku-cli`'s `credits.rs` already reads it with `eq_ignore_ascii_case`,
/// so this is that module's existing answer rather than a second one.
///
/// **Staged as a sibling, synced, and renamed**, so the replacement is atomic
/// and a crash leaves either the old archive or the new one, never a
/// half-written file. The sibling shares the target's own directory
/// deliberately: staging under a temp directory elsewhere would put the rename
/// across a filesystem boundary, where it stops being atomic and becomes a
/// copy.
///
/// **The sync is O104.** The rename can reach the journal before the staged
/// bytes reach the disk, and a crash in between leaves the archive's path
/// naming data that was never written — the one file in this tree nothing can
/// regenerate. So the staged archive is synced before the rename and its
/// directory after it, the same ordering `taiyaku_read::atomic::write` keeps.
///
/// # Errors
///
/// Returns an error if the archive cannot be read, written or synced, if the
/// rename fails, or if syncing the directory after the rename fails. On any
/// error before the rename the staging file is removed and the original is
/// left exactly as it was; the last error arrives with the new archive already
/// in place, and says so.
fn rewrite_one(path: &std::path::Path, document: &[u8]) -> Result<String, String> {
    use std::io::Read as _;

    let staged = staging_path(path);

    let result = (|| -> Result<String, String> {
        {
            let source =
                std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut archive =
                zip::ZipArchive::new(source).map_err(|e| format!("{}: {e}", path.display()))?;

            let out =
                std::fs::File::create(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
            let mut writer = zip::ZipWriter::new(out);

            for i in 0..archive.len() {
                let entry = archive
                    .by_index_raw(i)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                if entry.name().eq_ignore_ascii_case(DOCUMENT) {
                    continue;
                }
                writer
                    .raw_copy_file(entry)
                    .map_err(|e| format!("{}: {e}", staged.display()))?;
            }

            writer
                .start_file(DOCUMENT, zip::write::SimpleFileOptions::default())
                .map_err(|e| format!("{}: {e}", staged.display()))?;
            std::io::Write::write_all(&mut writer, document)
                .map_err(|e| format!("{}: {e}", staged.display()))?;
            writer
                .finish()
                .and_then(|out| out.sync_all().map_err(zip::result::ZipError::Io))
                .map_err(|e| format!("{}: {e}", staged.display()))?;
        }

        let digest = {
            let mut file =
                std::fs::File::open(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
            let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
            let mut buffer = vec![0u8; 64 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|e| format!("{}: {e}", staged.display()))?;
                if read == 0 {
                    break;
                }
                sha2::Digest::update(&mut hasher, &buffer[..read]);
            }
            sha2::Digest::finalize(hasher)
                .iter()
                .fold(String::new(), |mut hex, b| {
                    use std::fmt::Write as _;
                    let _ = write!(hex, "{b:02x}");
                    hex
                })
        };

        std::fs::rename(&staged, path)
            .map_err(|e| format!("renaming {} onto {}: {e}", staged.display(), path.display()))?;
        taiyaku_read::atomic::sync_directory_of(path).map_err(|e| {
            format!(
                "{}: renamed into place, but syncing its directory failed: {e}",
                path.display()
            )
        })?;
        Ok(digest)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

/// Where one archive is staged while it is being rewritten.
///
/// Beside the target and never in a temp directory, so the rename that
/// follows is a rename rather than a cross-filesystem copy. The suffix is
/// appended to the whole filename rather than replacing the extension, so
/// `v01.cbz` stages as `v01.cbz.taiyaku-stamp` and two members of the same
/// edition cannot collide on it the way `with_extension` would make
/// `v01.cbz` and `v01.cbr` collide.
fn staging_path(path: &std::path::Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".taiyaku-stamp");
    path.with_file_name(name)
}

/// Every member path of a work, whether or not a stamp would touch it.
///
/// [`plan`] answers only the archives that would change; a caller that needs
/// to say what was *left out* — and why — needs the whole set. **Here rather
/// than in `taiyaku-serve`:** a copy there would re-derive [`resolve`]'s
/// root-join independently, so the two could disagree about which file a
/// member names and the screen could report a format for an archive the plan
/// had resolved somewhere else entirely.
///
/// # Errors
///
/// Returns an error if the rows cannot be read.
pub fn member_paths(conn: &Connection, work_id: i64) -> Result<Vec<PathBuf>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.path, r.path FROM member m
               JOIN edition e ON e.id = m.edition_id
               LEFT JOIN root r ON r.id = m.root_id
              WHERE e.work_id = ?1
              ORDER BY e.key, m.ordinal",
        )
        .map_err(|e| format!("preparing this work's members: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| format!("reading this work's members: {e}"))?;
    rows.map(|row| {
        row.map(|(member, root)| resolve(&member, root.as_deref()))
            .map_err(|e| format!("reading this work's members: {e}"))
    })
    .collect()
}

/// A member's absolute path: its root joined to it.
///
/// **Both stored shapes are real** — `download::member_paths`'s own comment
/// records the second: an imported document may name a member relatively,
/// and a scanned work has no document, so its member paths are stored absolute.
///
/// **And `Path::join` already tells them apart, so there is no guard here.**
/// Joining an absolute path onto a base discards the base — `/shelf` joined
/// with `/elsewhere/v01.cbz` is `/elsewhere/v01.cbz` — which is exactly the
/// answer the absolute case wants. A guard on `path.is_relative()` would
/// compute the same value on both of its branches for every input, so it
/// would state a distinction `join` has already made.
fn resolve(member_path: &str, root: Option<&str>) -> PathBuf {
    let path = std::path::Path::new(member_path);
    match root {
        Some(root) => std::path::Path::new(root).join(path),
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests;
