//! The fingerprint store: which files a fingerprint pass is due to read, and
//! the rows it writes, one per file's content.
//!
//! A unit is an entry the derive claims ([`derive::claimed_units`]) in a
//! format the reader reads pages from, under a directory a library with
//! matching by content reaches. Rows are keyed by digest, so a copy or a
//! rename of a file already read costs nothing. Design:
//! `docs/superpowers/specs/2026-11-17-content-matching-fingerprints-design.md`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use taiyaku_read::Format;

use crate::{derive, reach, scan};

/// How much of a file's content a pass read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every page.
    Whole,
    /// Some pages, and at least one that did not decode.
    Partial,
    /// Nothing a fingerprint could be taken from.
    Unreadable,
}

impl Outcome {
    /// The `fingerprint.outcome` value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Partial => "partial",
            Self::Unreadable => "unreadable",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [Self::Whole, Self::Partial, Self::Unreadable]
            .into_iter()
            .find(|outcome| outcome.as_str() == value)
    }
}

/// One `fingerprint` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub sha256: String,
    /// The layout and algorithm `prints` was written by.
    pub version: u32,
    pub outcome: Outcome,
    /// Why the file was not read whole.
    pub reason: Option<String>,
    /// The server build that read it.
    pub build: Option<String>,
    pub pages: u32,
    /// One record per page entry, in reading order.
    pub prints: Vec<u8>,
}

/// A file or folder a pass is due to read, with the stamp its walk recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub root_id: i64,
    pub rel_path: String,
    /// The directory's path joined with `rel_path`.
    pub path: PathBuf,
    /// A folder of pages rather than an archive.
    pub folder: bool,
    /// Its digest, when one has been taken.
    pub sha256: Option<String>,
    /// `scan_entry.size`.
    pub size: i64,
    /// `scan_entry.mtime`.
    pub mtime: i64,
}

/// Which directories' files are matched by content, and which libraries say so.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Every directory a library with matching on reaches, less the paused.
    pub roots: HashSet<i64>,
    /// Each library with matching on, by name.
    pub libraries: Vec<(i64, String)>,
}

/// A stored row a pass reads again: written in another layout (`?1` is the
/// layout this build writes), or not read whole by another build (`?2`).
/// `IS NOT`, so a row no build stamped is settled for a build with no stamp.
const STALE: &str =
    "version IS NOT ?1 OR (outcome IN ('partial', 'unreadable') AND build IS NOT ?2)";

/// An entry `e` whose row `f` numbers other pages than this reader counted in
/// the file, as a re-count leaves one, since it keeps the digest the row is
/// keyed by. The `signatures` parameters from `?{first}` hold
/// [`paged_signatures`]: a read numbers a zip's or a RAR's pages as this
/// reader counts them, so a pass settles the row. Not a folder, whose walk
/// leaves out pages its read takes in, nor a count another reader took, nor a
/// row with no prints: no read would bring those to the entry's count, and each
/// would be read again by every pass.
fn numbers_other_pages(first: usize, signatures: usize) -> String {
    let parameters = (first..first + signatures)
        .map(|n| format!("?{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("e.counted_by IN ({parameters}) AND f.outcome <> 'unreadable' AND e.pages <> f.pages")
}

/// What this reader stamps the count of each paged format with: a zip's, and
/// a RAR's in a build with a RAR backend. A folder has none.
fn paged_signatures() -> Vec<String> {
    Format::PAGED
        .iter()
        .filter_map(|format| format.token().and_then(taiyaku_read::reader_signature))
        .collect()
}

/// The directories whose files are matched: those any library with matching
/// on reaches ([`reach::of_libraries`]), less the paused ([`scan::paused`]).
/// A directory one library switched off stays covered through another.
///
/// # Errors
///
/// If the store refuses a read.
pub fn coverage(conn: &Connection) -> Result<Coverage, String> {
    let failed = |e: rusqlite::Error| format!("reading which libraries match by content: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    let libraries: Vec<(i64, String)> = conn
        .prepare("SELECT id, name FROM library WHERE match_by_content ORDER BY name, id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .map_err(failed)?;
    let on: Vec<i64> = libraries.iter().map(|(id, _)| *id).collect();
    let paused = scan::paused(conn)?;
    let roots = reach::of_libraries(&scan::roots(conn)?, &on)
        .into_iter()
        .filter(|id| !paused.contains(id))
        .collect();
    Ok(Coverage { roots, libraries })
}

/// Every unit due to be read by a build writing layout `version`, stamped
/// `build`: a claimed entry of a covered directory, an archive or a folder
/// holding pages, whose digest is unknown, has no row this build would keep
/// (`STALE`), or has one numbering other pages ([`numbers_other_pages`]).
/// Newest `first_seen` first, then by path, and one unit per known digest, the
/// first, since one read serves every copy.
///
/// # Errors
///
/// If the store refuses a read, or the rules or directories cannot be read.
pub fn due(conn: &Connection, version: u32, build: Option<&str>) -> Result<Vec<Unit>, String> {
    let failed = |e: rusqlite::Error| format!("listing the files due a fingerprint: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    let covered = coverage(conn)?.roots;
    let paths: HashMap<i64, PathBuf> = scan::roots(conn)?
        .into_iter()
        .filter(|root| covered.contains(&root.id))
        .map(|root| (root.id, root.path))
        .collect();
    let settled: HashSet<String> = conn
        .prepare(&format!(
            "SELECT sha256 FROM fingerprint WHERE NOT ({STALE})"
        ))
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![version, build], |r| r.get(0))?
                .collect()
        })
        .map_err(failed)?;
    let signatures = paged_signatures();
    let renumbered: HashSet<(i64, String)> = conn
        .prepare(&format!(
            "SELECT e.root_id, e.rel_path
               FROM scan_entry e JOIN fingerprint f ON f.sha256 = e.sha256
              WHERE {}",
            numbers_other_pages(1, signatures.len())
        ))
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params_from_iter(&signatures), |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect()
        })
        .map_err(failed)?;

    let mut wanted: HashMap<i64, HashMap<String, derive::Claimed>> = HashMap::new();
    for claimed in derive::claimed_units(conn)? {
        let Some(format) = claimed.format.as_deref().and_then(Format::from_token) else {
            continue;
        };
        if paths.contains_key(&claimed.root_id)
            && Format::PAGED.contains(&format)
            && (format != Format::Dir || claimed.pages.is_some_and(|pages| pages > 0))
            && (renumbered.contains(&(claimed.root_id, claimed.rel_path.clone()))
                || !claimed
                    .sha256
                    .as_ref()
                    .is_some_and(|sha256| settled.contains(sha256)))
        {
            wanted
                .entry(claimed.root_id)
                .or_default()
                .insert(claimed.rel_path.clone(), claimed);
        }
    }

    let mut units = Vec::new();
    let mut stamps = conn
        .prepare("SELECT rel_path, size, mtime, first_seen FROM scan_entry WHERE root_id = ?1")
        .map_err(failed)?;
    for (root_id, claims) in &mut wanted {
        let mut rows = stamps.query([*root_id]).map_err(failed)?;
        while let Some(row) = rows.next().map_err(failed)? {
            let rel_path: String = row.get(0).map_err(failed)?;
            let Some(claimed) = claims.remove(&rel_path) else {
                continue;
            };
            let first_seen: String = row.get(3).map_err(failed)?;
            units.push((
                first_seen,
                Unit {
                    root_id: *root_id,
                    path: paths[root_id].join(&rel_path),
                    rel_path,
                    folder: claimed.format.as_deref() == Format::Dir.token(),
                    sha256: claimed.sha256,
                    size: row.get(1).map_err(failed)?,
                    mtime: row.get(2).map_err(failed)?,
                },
            ));
        }
    }
    units.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    let mut digests = HashSet::new();
    Ok(units
        .into_iter()
        .map(|(_, unit)| unit)
        .filter(|unit| {
            unit.sha256
                .as_ref()
                .is_none_or(|sha256| digests.insert(sha256.clone()))
        })
        .collect())
}

/// Whether `unit` is still worth reading: its directory is covered now, and
/// its digest, if known, has no row this build would keep that numbers the
/// pages its entry counts. A pass asks before each read, since a switch, a
/// pause or a twin's row can land mid-pass.
///
/// # Errors
///
/// If the store refuses a read.
pub fn still_due(
    conn: &Connection,
    unit: &Unit,
    version: u32,
    build: Option<&str>,
) -> Result<bool, String> {
    if !coverage(conn)?.roots.contains(&unit.root_id) {
        return Ok(false);
    }
    let Some(sha256) = &unit.sha256 else {
        return Ok(true);
    };
    let signatures = paged_signatures();
    let mut bound: Vec<&dyn rusqlite::ToSql> =
        vec![&version, &build, sha256, &unit.root_id, &unit.rel_path];
    let first = bound.len() + 1;
    bound.extend(
        signatures
            .iter()
            .map(|signature| signature as &dyn rusqlite::ToSql),
    );
    let settled: bool = conn
        .query_row(
            &format!(
                "SELECT EXISTS (
                   SELECT 1 FROM fingerprint f WHERE f.sha256 = ?3 AND NOT ({STALE})
                      AND NOT EXISTS (
                        SELECT 1 FROM scan_entry e
                         WHERE e.root_id = ?4 AND e.rel_path = ?5 AND e.sha256 = f.sha256
                           AND {}))",
                numbers_other_pages(first, signatures.len())
            ),
            bound.as_slice(),
            |r| r.get(0),
        )
        .map_err(|e| format!("asking whether {} is still due: {e}", unit.rel_path))?;
    Ok(!settled)
}

/// `path`'s `(size, mtime)` as a walk records them in `scan_entry`, or
/// `None` if it cannot be read. A pass compares it with its unit's before
/// and after a read, so a file that moved meanwhile is not stored.
#[must_use]
pub fn stamp_of(path: &Path) -> Option<(i64, i64)> {
    std::fs::metadata(path).ok().map(|meta| stamp_in(&meta))
}

/// [`stamp_of`] from a stat already taken, for a caller that asks the same
/// stat what kind of thing is there: one stat, so the stamp and the kind
/// describe one moment.
pub(crate) fn stamp_in(meta: &std::fs::Metadata) -> (i64, i64) {
    (
        i64::try_from(meta.len()).unwrap_or(i64::MAX),
        scan::mtime_secs(meta),
    )
}

/// Write `sha256` onto `unit`'s entry, as a derive writes a digest it took,
/// and say whether it was written. Only where no digest is held, the size and
/// mtime are still the unit's, and the directory is not paused, all asked in
/// the one statement that writes. A pass runs beside scans: a walk that
/// recorded new bytes since must not be given the old bytes' digest, and a
/// paused directory's derive passes an entry whose digest its works hold, so
/// a digest written there could add a copy the pause holds back.
///
/// # Errors
///
/// If the store refuses the write.
pub fn record_digest(conn: &Connection, unit: &Unit, sha256: &str) -> Result<bool, String> {
    conn.execute(
        "UPDATE scan_entry SET sha256 = ?3
          WHERE root_id = ?1 AND rel_path = ?2 AND sha256 IS NULL AND size = ?4 AND mtime = ?5
            AND EXISTS (SELECT 1 FROM root WHERE id = ?1 AND paused_at IS NULL)",
        rusqlite::params![unit.root_id, unit.rel_path, sha256, unit.size, unit.mtime],
    )
    .map(|written| written > 0)
    .map_err(|e| format!("recording the digest of {}: {e}", unit.rel_path))
}

/// Store `row`, replacing any row its digest has.
///
/// # Errors
///
/// If the store refuses the write.
pub fn store(conn: &Connection, row: &Row) -> Result<(), String> {
    conn.execute(
        "INSERT INTO fingerprint (sha256, version, outcome, reason, build, pages, prints)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (sha256) DO UPDATE SET
           version = excluded.version, outcome = excluded.outcome,
           reason = excluded.reason, build = excluded.build, pages = excluded.pages,
           prints = excluded.prints, read_at = datetime('now')",
        rusqlite::params![
            row.sha256,
            row.version,
            row.outcome.as_str(),
            row.reason,
            row.build,
            row.pages,
            row.prints,
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("storing the fingerprint of {}: {e}", row.sha256))
}

/// The row stored for `sha256`, if there is one.
///
/// # Errors
///
/// If the store refuses the read, or the row holds an outcome this build does
/// not know.
pub fn read(conn: &Connection, sha256: &str) -> Result<Option<Row>, String> {
    let failed = |e: rusqlite::Error| format!("reading the fingerprint of {sha256}: {e}");
    let Some((version, outcome, reason, build, pages, prints)) = conn
        .query_row(
            "SELECT version, outcome, reason, build, pages, prints
               FROM fingerprint WHERE sha256 = ?1",
            [sha256],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, String>(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(failed)?
    else {
        return Ok(None);
    };
    let outcome = Outcome::parse(&outcome)
        .ok_or_else(|| format!("the fingerprint of {sha256} has an unknown outcome {outcome}"))?;
    Ok(Some(Row {
        sha256: sha256.to_string(),
        version,
        outcome,
        reason,
        build,
        pages,
        prints,
    }))
}

/// Drop every row whose digest no claimed entry holds, in any directory, and
/// say how many went. A directory switched off keeps its rows, so switching it
/// back on reads nothing again. A row held only by an entry the derive does
/// not claim goes: what a rule edit that moves a claim leaves behind. A digest
/// a held reading names stays, since a paused directory's works are still
/// drawn from it.
///
/// # Errors
///
/// If the store refuses a read or the write, or the rules or directories
/// cannot be read.
pub fn drop_unclaimed(conn: &Connection) -> Result<usize, String> {
    let unclaimed = unclaimed(conn)?;
    // The usual case after a first pass: a write lock taken here would hold
    // every other writer behind a lowered thread for nothing.
    if unclaimed.is_empty() {
        return Ok(0);
    }
    drop_unless_held(conn, &unclaimed)
}

/// The digests of the rows no claimed entry and no held reading holds, read
/// in a snapshot of its own, closed before the write lock is taken, so scans
/// are not held up by it. A claim made after it costs one file read again,
/// not a wrong row.
fn unclaimed(conn: &Connection) -> Result<Vec<String>, String> {
    let failed = |e: rusqlite::Error| format!("dropping the fingerprints no file holds: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    let mut kept: HashSet<String> = derive::claimed_units(conn)?
        .into_iter()
        .filter_map(|unit| unit.sha256)
        .collect();
    kept.extend(held_readings(conn).map_err(failed)?);
    let mut rows = conn
        .prepare("SELECT sha256 FROM fingerprint")
        .map_err(failed)?;
    let digests = rows
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(failed)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(failed)?;
    Ok(digests
        .into_iter()
        .filter(|sha256| !kept.contains(sha256))
        .collect())
}

/// Delete the rows of `digests` in one short write, but for any a held
/// reading names by now: a pause since the snapshot holds a paused
/// directory's works by a digest, and the pass reads no paused file again.
fn drop_unless_held(conn: &Connection, digests: &[String]) -> Result<usize, String> {
    let failed = |e: rusqlite::Error| format!("dropping the fingerprints no file holds: {e}");
    let tx = crate::Nested::open(conn, "drop_unclaimed").map_err(failed)?;
    let held = held_readings(&tx).map_err(failed)?;
    let mut dropped = 0;
    {
        let mut delete = tx
            .prepare("DELETE FROM fingerprint WHERE sha256 = ?1")
            .map_err(failed)?;
        for sha256 in digests.iter().filter(|sha256| !held.contains(*sha256)) {
            dropped += delete.execute([sha256]).map_err(failed)?;
        }
    }
    tx.commit().map_err(failed)?;
    Ok(dropped)
}

/// Every digest a held reading names.
fn held_readings(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    conn.prepare("SELECT DISTINCT held_sha256 FROM scan_entry WHERE held_sha256 IS NOT NULL")?
        .query_map([], |r| r.get(0))?
        .collect()
}

#[cfg(test)]
mod tests;
