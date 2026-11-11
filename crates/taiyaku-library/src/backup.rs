//! Backup, per store spec §13.4 and spec C4 §5.
//!
//! **`VACUUM INTO`, not a file copy.** SQLite's own statement takes a
//! consistent snapshot of a live database with the server running and no
//! external tool; copying the file under a WAL-mode writer does not.
//!
//! **A backup is not a sidecar export**, which store spec §13.4 states and C4
//! §5 puts on the screen: `export` writes five documents per work and carries
//! none of the cross-work data — collections, tags, external ids, users, read
//! history — so somebody who exported their library still needs this.
//!
//! **Restore is deliberately out-of-band**: stop the server, move the file,
//! start it. C4 §5 declines an in-server restore; nothing here does one.
//!
//! **The open takes one before every migration** (O263), tagged `-before-v<N>`
//! for the version it migrates to, and [`prune`] neither counts nor deletes it.
//! It skips one only while the newest for that version still holds the
//! database unchanged; see [`stands_for`].

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::Connection;

/// What every backup file's name starts with.
///
/// **A constant because three things read it** — [`take`] mints it, [`list`]
/// filters on it and [`prune`] deletes on it. A literal repeated at the delete
/// site is a literal that can drift away from the one at the create site.
pub const PREFIX: &str = "library-";

/// What every backup file's name ends with. See [`PREFIX`].
pub const SUFFIX: &str = ".db";

/// Why [`take`] would not write one.
///
/// **Two cases, because one of them is not a fault.** A backup already
/// existing under the minted name is a person pressing "back up now" twice
/// inside the same second — the name's timestamp has one-second resolution —
/// and is not a server failure. Named here so the route can answer `409` and
/// say what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeError {
    /// A backup of this name is already there. Not a fault: wait a second.
    Exists(String),
    /// Anything else — the directory, the clock, the statement.
    Failed(String),
}

impl std::fmt::Display for TakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exists(why) | Self::Failed(why) => f.write_str(why),
        }
    }
}

/// One backup file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// The file name alone, never a path — see `backup_api`'s download route,
    /// which resolves a requested name against this listing rather than
    /// joining a caller's string onto a directory.
    pub name: String,
    pub bytes: u64,
    /// The timestamp embedded in the name, `YYYYMMDDTHHMMSSZ`.
    pub taken: String,
    /// The `N` of a `-before-v<N>` name, `None` for a backup taken on request.
    pub before: Option<u32>,
}

/// Where backups of `db` live: `<db parent>/backups`.
///
/// **Derived rather than configured, because the server has to compute it.**
/// `GET /api/library/backups` answers "what exists", and it can only answer
/// for a directory it can find from the one thing it holds — the database
/// path, which [`crate::Library::path`] already returns. The CLI's `--out DIR`
/// writes somewhere else on purpose and the server will not list what lands
/// there; the command's own help says so.
#[must_use]
pub fn directory_for(db: &Path) -> PathBuf {
    db.parent().unwrap_or(Path::new(".")).join("backups")
}

/// What sits between the stamp and [`SUFFIX`] in the name of a backup taken
/// before a migration, ahead of the version it migrates to. A constant for
/// [`PREFIX`]'s reason: [`name_before_migration`] mints it and [`stamp_of`]
/// reads it.
const BEFORE: &str = "-before-v";

/// The name a backup taken now would get, `tail` between its stamp and
/// [`SUFFIX`], from **SQLite's own clock**.
///
/// This workspace has no time crate — `docs/status.md` states the short
/// dependency list as a fact about the project — and every other timestamp in
/// the store comes from `datetime('now')`. One `query_row` costs nothing and
/// adds no dependency.
fn name_now(conn: &Connection, tail: &str) -> Result<String, String> {
    let stamp: String = conn
        .query_row("SELECT strftime('%Y%m%dT%H%M%SZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("reading the clock for a backup name: {e}"))?;
    Ok(format!("{PREFIX}{stamp}{tail}{SUFFIX}"))
}

/// The name of the backup an open takes now, before migrating to `target`.
pub(crate) fn name_before_migration(conn: &Connection, target: u32) -> Result<String, String> {
    name_now(conn, &format!("{BEFORE}{target}"))
}

/// Take a backup into `into_dir`, creating the directory if it is not there.
///
/// **Never call this on the server's own connection.** `ServeState.library`
/// holds one `Mutex<Connection>` for the whole process and a `VACUUM INTO`
/// stalls every request for the length of a file copy; the route runs it on
/// [`crate::Library::working`] inside `spawn_blocking`, as the rule preview
/// and the scan do.
///
/// # Errors
///
/// Returns an error if the directory cannot be created, if the clock cannot be
/// read, if the name is already taken or cannot be created, or if SQLite
/// refuses the `VACUUM INTO`.
pub fn take(conn: &Connection, into_dir: &Path) -> Result<Backup, TakeError> {
    let name = name_now(conn, "").map_err(TakeError::Failed)?;
    take_named(conn, into_dir, &name)
}

/// [`take`] with the name decided by the caller.
///
/// Split out so a test can drive the same-second collision without racing the
/// clock, and so the error message names the file rather than the statement.
///
/// # Errors
///
/// As [`take`].
pub fn take_named(conn: &Connection, into_dir: &Path, name: &str) -> Result<Backup, TakeError> {
    // **The shape check is on this function, not on its callers.** [`list`]
    // and [`prune`] are built so only a `stamp_of`-accepted name in one
    // directory can ever be read or deleted; this is the write side of the
    // same boundary, and it is `pub` with a doc comment that invites a test
    // caller to pass its own name. Without the check, `take_named(conn, dir,
    // "../../library.db")` joins straight onto `into_dir`, and the rename
    // into place would replace the live database with its own copy.
    let Some((taken, before)) = stamp_of(name) else {
        return Err(TakeError::Failed(format!(
            "`{name}` is not a backup name; one is `{PREFIX}YYYYMMDDTHHMMSSZ{SUFFIX}`"
        )));
    };
    std::fs::create_dir_all(into_dir)
        .map_err(|e| TakeError::Failed(format!("creating {}: {e}", into_dir.display())))?;
    let target = into_dir.join(name);
    let stage = into_dir.join(stage_for(name));

    // The copy is made under a name `stamp_of` refuses and renamed to its own
    // only once it is whole and synced, so a backup's name never holds a cut
    // copy, which an open would take as the backup (O263). The stage is
    // created exclusively and is the claim on the name: of two takes only one
    // copies, and a name taken stays that refusal, not a suffix counter (PR2
    // §4).
    let mut create = std::fs::OpenOptions::new();
    create.write(true).create_new(true);
    // The mode SQLite gives a file it creates; the copy holds password hashes.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut create, 0o644);
    if let Err(e) = create.open(&stage) {
        return Err(create_refused(name, &stage, &e));
    }
    // Asked under the claim, so no other take can rename onto the name
    // between this and the rename below, which would replace what it finds.
    match std::fs::symlink_metadata(&target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        found => {
            let _ = std::fs::remove_file(&stage);
            return Err(match found {
                Err(e) => TakeError::Failed(format!("reading {}: {e}", target.display())),
                Ok(_) => taken_already(name),
            });
        }
    }

    let copied = conn
        .execute("VACUUM INTO ?1", rusqlite::params![stage.to_string_lossy()])
        .map_err(|e| e.to_string())
        .and_then(|_| {
            std::fs::File::open(&stage)
                .and_then(|copy| copy.sync_all())
                .map_err(|e| format!("syncing the copy: {e}"))
        })
        .and_then(|()| {
            std::fs::rename(&stage, &target)
                .map_err(|e| format!("renaming the copy into place: {e}"))
        });
    if let Err(why) = copied {
        // Only this call made the stage, and nothing else will remove it.
        return Err(copy_failed(name, &why, &std::fs::remove_file(&stage)));
    }
    taiyaku_read::atomic::sync_directory_of(&target).map_err(|e| {
        TakeError::Failed(format!(
            "the backup {name} is whole, but syncing its directory failed: {e}"
        ))
    })?;

    let bytes = std::fs::metadata(&target)
        .map_err(|e| TakeError::Failed(format!("reading the size of {}: {e}", target.display())))?
        .len();
    Ok(Backup {
        taken,
        name: name.to_string(),
        bytes,
        before,
    })
}

/// Where [`take_named`] copies the backup `name` before renaming it to `name`:
/// a dot name, which [`stamp_of`] refuses, so a copy cut off there is never
/// listed, pruned or taken for the backup, and is left for a person to delete.
fn stage_for(name: &str) -> String {
    format!(".{name}.partial")
}

/// The refusal of a name already taken, or being taken by another call.
fn taken_already(name: &str) -> TakeError {
    TakeError::Exists(format!(
        "a backup named {name} is already there or being taken; the name carries \
         a whole-second timestamp, so a second backup inside the same second has \
         nowhere to go"
    ))
}

/// Why the exclusive create refused `target`. Only a name already taken is
/// [`TakeError::Exists`]: [`before_migration`] reads that as success when the
/// backup is there, so any other refusal has to stay a fault.
fn create_refused(name: &str, target: &Path, e: &std::io::Error) -> TakeError {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        taken_already(name)
    } else {
        TakeError::Failed(format!("creating {}: {e}", target.display()))
    }
}

/// The copy's error, and the removal's when the file it made is still there.
fn copy_failed(
    name: &str,
    why: &dyn std::fmt::Display,
    removed: &std::io::Result<()>,
) -> TakeError {
    TakeError::Failed(match removed {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            format!("writing the backup {name}: {why}; the file it made could not be removed: {e}")
        }
        _ => format!("writing the backup {name}: {why}"),
    })
}

/// Take the backup an open makes before it migrates `db` (O263), into
/// [`directory_for`] `db`.
///
/// **On the one connection there is**, which [`take`] forbids for the server's
/// own: that rule is about stalling requests, and nothing is served while the
/// library opens. [`TakeError::Exists`] is success only when the backup is
/// there under its name, which a second process opening the same database in
/// the same second took. A copy still under its stage is not the backup: it
/// may be cut off, or finish after the other process migrated.
pub(crate) fn before_migration(conn: &Connection, db: &Path, name: &str) -> Result<(), String> {
    let dir = directory_for(db);
    let why = match take_named(conn, &dir, name) {
        Ok(_) => return Ok(()),
        Err(TakeError::Exists(_)) if dir.join(name).is_file() => return Ok(()),
        Err(TakeError::Exists(_)) => "another open is taking it, or one cut off left it \
                                      unfinished; an open in a later second takes one of \
                                      its own"
            .to_string(),
        Err(TakeError::Failed(why)) => why,
    };
    Err(format!(
        "the backup {name} could not be taken before migrating {}, so the database \
         was not migrated: {why}",
        db.display()
    ))
}

/// When `db` last changed: the latest of the modification and change times of
/// it and its `-wal`, a `-wal` that is not there counting for nothing. `None`
/// when either cannot be read, which [`stands_for`] takes as a change.
///
/// The `-wal` counts because a write committed there has not reached the main
/// file until a checkpoint. The `-shm` does not: every reader writes to it.
pub(crate) fn last_written(db: &Path) -> Option<SystemTime> {
    let main = changed_at(db).ok()?;
    let mut wal = db.as_os_str().to_owned();
    wal.push("-wal");
    match changed_at(Path::new(&wal)) {
        Ok(at) => Some(main.max(at)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(main),
        Err(_) => None,
    }
}

/// The later of `path`'s modification and change times. The change time
/// counts because a move keeps the modification time: where the filesystem
/// sets the change time on a rename, a database restored by `mv` is a change.
/// A `chmod` sets it too, which only costs one more backup.
fn changed_at(path: &Path) -> std::io::Result<SystemTime> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata.modified()?;
    Ok(status_changed(&metadata).map_or(modified, |changed| modified.max(changed)))
}

#[cfg(unix)]
fn status_changed(metadata: &std::fs::Metadata) -> Option<SystemTime> {
    use std::os::unix::fs::MetadataExt;
    let secs = u64::try_from(metadata.ctime()).ok()?;
    let nanos = u32::try_from(metadata.ctime_nsec()).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::new(secs, nanos))
}

#[cfg(not(unix))]
fn status_changed(_: &std::fs::Metadata) -> Option<SystemTime> {
    None
}

/// Whether the newest backup before migrating `db` from `current` to `target`
/// was written no earlier than `written` and opens at `current`, so it still
/// holds the database as it stands (O263). Without this a failing migration
/// in a restart loop backs up on every start, and nothing deletes those, until
/// the disk fills. Any doubt is `false`.
pub(crate) fn stands_for(
    db: &Path,
    current: u32,
    target: u32,
    written: Option<SystemTime>,
) -> bool {
    let Some(written) = written else {
        return false;
    };
    let dir = directory_for(db);
    let Ok(found) = list(&dir) else {
        return false;
    };
    let Some(newest) = found.iter().find(|backup| backup.before == Some(target)) else {
        return false;
    };
    let path = dir.join(&newest.name);
    // The backup's modification time alone, which dates its content: its
    // change time moves with a `chmod` and would date an old copy as new.
    // Equal is unchanged: writes are dated from a coarse clock, so a backup
    // finished in the tick of the database's last write shares its time.
    let unchanged = std::fs::symlink_metadata(&path)
        .and_then(|m| m.modified())
        .is_ok_and(|taken| written <= taken);
    unchanged && opens_at(&path, current)
}

/// Whether `path` opens as a database at `version`. [`take_named`]'s rename
/// puts only a whole copy under a backup's name; this stands between the skip
/// and a file put there any other way, an empty one included.
fn opens_at(path: &Path, version: u32) -> bool {
    Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .is_ok_and(|copy| crate::schema::version(&copy) == Ok(version))
}

/// The timestamp inside a backup's name and the `N` of a `-before-v<N>` tail,
/// or `None` for a name that is not a backup's.
///
/// **The one place that decides what counts as a backup file**, read by
/// [`list`] and therefore by [`prune`].
fn stamp_of(name: &str) -> Option<(String, Option<u32>)> {
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    // `YYYYMMDDTHHMMSSZ` — 16 bytes, digits with a `T` and a `Z`. Checked
    // rather than assumed so `prune` cannot be talked into deleting
    // `library-.db` or `library-../../etc/passwd.db`. Read as bytes, so a
    // letter outside ASCII is refused here rather than cut in two below.
    let bytes = rest.as_bytes();
    let shaped = bytes.len() >= 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes[..16]
            .iter()
            .enumerate()
            .all(|(i, b)| i == 8 || i == 15 || b.is_ascii_digit());
    if !shaped {
        return None;
    }
    // The first 16 bytes are ASCII, so 16 is a character boundary.
    let (stamp, tail) = rest.split_at(16);
    let before = if tail.is_empty() {
        None
    } else {
        Some(version_of(tail.strip_prefix(BEFORE)?)?)
    };
    Some((stamp.to_string(), before))
}

/// `N` with one spelling per version: ASCII digits, the first not `0`, within
/// `u32`. `str::parse` alone would also take `+5` and `019`.
fn version_of(digits: &str) -> Option<u32> {
    let plain = !digits.starts_with('0') && digits.bytes().all(|b| b.is_ascii_digit());
    plain.then(|| digits.parse().ok()).flatten()
}

/// Every backup in `dir`, newest first.
///
/// **A directory that does not exist is an empty listing**, not an error: a
/// library nobody has backed up yet is the ordinary case, and the screen that
/// lists backups is the one an operator opens before taking the first.
///
/// Files that are not backups are ignored, and so is a *directory* whose name
/// happens to match — the entry has to be a file.
///
/// # Errors
///
/// Returns an error if the directory exists and cannot be read.
pub fn list(dir: &Path) -> Result<Vec<Backup>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("reading {}: {e}", dir.display())),
    };

    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((taken, before)) = stamp_of(&name) else {
            continue;
        };
        // **`DirEntry::metadata` does not traverse a symlink**, unlike the
        // free `fs::metadata` it is easy to mistake it for. So a symlink named
        // like a backup answers `is_symlink`, not `is_file`, and is skipped
        // right here — which means [`prune`] can never be handed one and the
        // only thing it can ever delete is a real file this directory holds
        // under a name [`stamp_of`] accepted.
        //
        // The cost of that strictness is that a backup symlinked in from the
        // volume `--out DIR` wrote it to does not appear on the screen that
        // lists backups. The trade favours a delete path that cannot be aimed
        // through a link, and
        // `pruning_a_symlinked_backup_is_left_alone_entirely` holds it.
        let metadata = entry
            .metadata()
            .map_err(|e| format!("reading the size of {name}: {e}"))?;
        if !metadata.is_file() {
            continue;
        }
        found.push(Backup {
            name,
            bytes: metadata.len(),
            taken,
            before,
        });
    }
    // The stamp sorts lexicographically in time order, which is the whole
    // reason the name carries `YYYYMMDDTHHMMSSZ` rather than anything
    // friendlier.
    found.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| b.name.cmp(&a.name)));
    Ok(found)
}

/// Keep the `keep` newest backups taken on request in `dir` and delete the
/// rest, answering which were removed, oldest first. A backup taken before a
/// migration is neither counted nor deleted (O263): a cron entry runs `--keep`
/// unattended, and those are never deleted automatically. A `keep` at or above
/// the count removes nothing and `0` removes them all; `--keep` has no default,
/// so nothing reaches this with a value nobody typed.
///
/// **It is written to be unable to delete the wrong one.** It deletes only
/// names [`list`] returned — which is to say only names [`stamp_of`] accepted,
/// in this one directory, never recursing and never following a subdirectory —
/// and it returns the names rather than a count, so the CLI prints what
/// happened instead of a number the operator has to trust.
///
/// # Errors
///
/// Returns an error if the directory cannot be read, or if a delete fails —
/// after which the returned error names the file, and the files already
/// deleted stay deleted.
pub fn prune(dir: &Path, keep: usize) -> Result<Vec<String>, String> {
    let found: Vec<Backup> = list(dir)? // newest first
        .into_iter()
        .filter(|backup| backup.before.is_none())
        .collect();
    if found.len() <= keep {
        return Ok(Vec::new());
    }

    let mut removed = Vec::new();
    // `found` is newest-first, so everything past `keep` is what goes; reversed
    // so the answer reads oldest-first, which is the order a person expects a
    // retention sweep to have worked in.
    for backup in found.into_iter().skip(keep).rev() {
        let path = dir.join(&backup.name);
        std::fs::remove_file(&path)
            .map_err(|e| format!("removing the backup {}: {e}", backup.name))?;
        removed.push(backup.name);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests;
