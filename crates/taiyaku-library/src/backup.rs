//! Backup, per store spec §13.4 and spec C4 §5.
//!
//! **`VACUUM INTO`, not a file copy.** SQLite's own statement takes a
//! consistent snapshot of a live database without stopping the server and
//! needs no external tool; copying the file out from under a WAL-mode writer
//! does not.
//!
//! **A backup is not a sidecar export**, which store spec §13.4 states and C4
//! §5 puts on the screen: `export` writes five documents per work and carries
//! none of the cross-work data — collections, tags, external ids, users, read
//! history — so somebody who exported their library still needs this.
//!
//! **Restore is deliberately out-of-band**: stop the server, move the file,
//! start it. C4 §5 declines an in-server restore, and this module has no
//! function for one.

use std::path::{Path, PathBuf};

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

/// The name a backup taken now would get, from **SQLite's own clock**.
///
/// This workspace has no time crate — `docs/status.md` states the short
/// dependency list as a fact about the project — and every other timestamp in
/// the store comes from `datetime('now')`. One `query_row` costs nothing and
/// adds no dependency.
fn name_now(conn: &Connection) -> Result<String, String> {
    let stamp: String = conn
        .query_row("SELECT strftime('%Y%m%dT%H%M%SZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("reading the clock for a backup name: {e}"))?;
    Ok(format!("{PREFIX}{stamp}{SUFFIX}"))
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
/// read, or if SQLite refuses the `VACUUM INTO` — which includes the target
/// file already existing.
pub fn take(conn: &Connection, into_dir: &Path) -> Result<Backup, TakeError> {
    let name = name_now(conn).map_err(TakeError::Failed)?;
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
    // "../../library.db")` joins straight onto `into_dir` and writes outside
    // it — `VACUUM INTO`'s refusal of an existing target is the only thing
    // between that and the live database, and it does not hold for a target
    // that does not exist yet.
    let Some(taken) = stamp_of(name) else {
        return Err(TakeError::Failed(format!(
            "`{name}` is not a backup name; one is `{PREFIX}YYYYMMDDTHHMMSSZ{SUFFIX}`"
        )));
    };
    std::fs::create_dir_all(into_dir)
        .map_err(|e| TakeError::Failed(format!("creating {}: {e}", into_dir.display())))?;
    let target = into_dir.join(name);

    // Asked before the statement rather than read out of its message: SQLite's
    // "output file already exists" is prose, and matching on prose is how a
    // caller's error handling breaks on a version bump.
    if target.exists() {
        return Err(TakeError::Exists(format!(
            "a backup named {name} is already there; the name carries a \
             whole-second timestamp, so a second backup inside the same second \
             has nowhere to go"
        )));
    }

    // `VACUUM INTO` refuses an existing target rather than overwriting it, and
    // that refusal is kept as itself: a suffix counter would be machinery for
    // a case only a test loop produces (PR2 §4).
    conn.execute(
        "VACUUM INTO ?1",
        rusqlite::params![target.to_string_lossy()],
    )
    .map_err(|e| TakeError::Failed(format!("writing the backup {name}: {e}")))?;

    let bytes = std::fs::metadata(&target)
        .map_err(|e| TakeError::Failed(format!("reading the size of {}: {e}", target.display())))?
        .len();
    Ok(Backup {
        taken,
        name: name.to_string(),
        bytes,
    })
}

/// The timestamp inside a backup's name, or `None` for a name that is not one.
///
/// **The one place that decides what counts as a backup file**, read by
/// [`list`] and therefore by [`prune`].
fn stamp_of(name: &str) -> Option<String> {
    let stamp = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    // `YYYYMMDDTHHMMSSZ` — 16 characters, digits with a `T` and a `Z`. Checked
    // rather than assumed so `prune` cannot be talked into deleting
    // `library-.db` or `library-../../etc/passwd.db`.
    let bytes = stamp.as_bytes();
    let shaped = bytes.len() == 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 8 || i == 15 || b.is_ascii_digit());
    shaped.then(|| stamp.to_string())
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
        let Some(taken) = stamp_of(&name) else {
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
        });
    }
    // The stamp sorts lexicographically in time order, which is the whole
    // reason the name carries `YYYYMMDDTHHMMSSZ` rather than anything
    // friendlier.
    found.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| b.name.cmp(&a.name)));
    Ok(found)
}

/// Keep the `keep` newest backups in `dir` and delete the rest, answering
/// which files were removed, oldest first.
///
/// **It is written to be unable to delete the wrong one.** It deletes only
/// names [`list`] returned — which is to say only names [`stamp_of`] accepted,
/// in this one directory, never recursing and never following a subdirectory —
/// and it returns the names rather than a count, so the CLI prints what
/// happened instead of a number the operator has to trust.
///
/// A `keep` larger than the number of backups removes nothing. A `keep` of `0`
/// removes all of them, which is a caller asking for exactly that; the CLI's
/// `--keep` is absent by default rather than defaulting to a number, so
/// nothing reaches this with a value nobody typed.
///
/// # Errors
///
/// Returns an error if the directory cannot be read, or if a delete fails —
/// after which the returned error names the file, and the files already
/// deleted stay deleted.
pub fn prune(dir: &Path, keep: usize) -> Result<Vec<String>, String> {
    let found = list(dir)?; // newest first
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
