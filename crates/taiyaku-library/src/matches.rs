//! The match store: pairs of files scored by content, the files the match
//! pass has looked up, and which files take part. Design:
//! `docs/superpowers/specs/2026-11-19-content-matching-volume-joins-design.md`,
//! §1 and §3.
//!
//! A pair is known by its two digests, the smaller first. Whether it joins is
//! not stored: derive decides that from V, the state and the rules each time.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension};
use taiyaku_core::{Direction, PageRange};

use crate::{Nested, scan};

/// The V from which a pair joins, a pair exactly at it included: the bar the
/// volume test froze, which `taiyaku-cli`'s `the_server_bar_is_the_frozen_bar`
/// holds this equal to.
pub const BAR: f64 = 0.87;

/// The V from which a pair is stored; under it only the file's lookup is.
/// Pairs between this and [`BAR`] are kept for the align suggestions of spec
/// §7, which score nothing of their own.
pub const FLOOR: f64 = 0.6;

/// A pair's `content_match.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// As the match pass stored it.
    Scored,
    /// Kept apart by an admin, so no derive joins it.
    Split,
}

impl State {
    /// The `content_match.state` value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scored => "scored",
            Self::Split => "split",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [Self::Scored, Self::Split]
            .into_iter()
            .find(|state| state.as_str() == value)
    }
}

/// A scored pair as the match pass hands it to [`store`]: either digest may
/// come first, and every figure named for a side is that side's.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPair {
    pub sha_a: String,
    pub sha_b: String,
    pub v: f64,
    /// Slots in paired runs long enough to count: V's numerator.
    pub long_paired: u32,
    pub a_slots: u32,
    pub b_slots: u32,
    pub a_pages: u32,
    pub b_pages: u32,
    /// The solver's calibration floor, where it accepted one.
    pub floor: Option<f32>,
    /// Paired runs of whole pages, each from a's pages to b's.
    pub runs: Vec<(PageRange, PageRange)>,
}

impl NewPair {
    /// The same pair named b first.
    fn turned(&self) -> Self {
        Self {
            sha_a: self.sha_b.clone(),
            sha_b: self.sha_a.clone(),
            v: self.v,
            long_paired: self.long_paired,
            a_slots: self.b_slots,
            b_slots: self.a_slots,
            a_pages: self.b_pages,
            b_pages: self.a_pages,
            floor: self.floor,
            runs: self.runs.iter().map(|&(a, b)| (b, a)).collect(),
        }
    }
}

/// A stored pair: `new.sha_a` is the smaller digest.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub id: i64,
    pub new: NewPair,
    pub state: State,
}

/// A file that takes part in matching (spec §1).
#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    pub sha256: String,
    /// Its `fingerprint.prints`.
    pub prints: Vec<u8>,
    pub work_id: i64,
    pub series_id: Option<i64>,
    pub volume: f64,
    /// Its work's, which orders a spread's halves.
    pub reading: Direction,
    /// Each library with matching on that reaches one of its directories.
    pub libraries: BTreeSet<i64>,
}

/// Each directory a library with matching on reaches
/// ([`crate::reach::of_libraries`]), with every such library. Pauses are not
/// asked, so a pause never undoes a join: it holds a directory's scans back,
/// not the files it already holds.
///
/// # Errors
///
/// If the store refuses a read.
pub fn reach(conn: &Connection) -> Result<HashMap<i64, BTreeSet<i64>>, String> {
    let failed = |e: rusqlite::Error| format!("reading which libraries match by content: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    let on: Vec<i64> = conn
        .prepare("SELECT id FROM library WHERE match_by_content ORDER BY id")
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .map_err(failed)?;
    let roots = scan::roots(conn)?;
    let mut reached: HashMap<i64, BTreeSet<i64>> = HashMap::new();
    for library in on {
        for root in crate::reach::of_libraries(&roots, &[library]) {
            reached.entry(root).or_default().insert(library);
        }
    }
    Ok(reached)
}

/// Every file that takes part, by digest: the only member of its edition, in a
/// work with a volume, with a `whole` fingerprint of `fingerprint_version`,
/// under a directory (its own or a copy's) a library with matching on
/// reaches ([`reach`]). One per digest, the first such member's: the lookup
/// and the pair store know a file by its digest, and a digest paired with
/// itself is no pair.
///
/// # Errors
///
/// If the store refuses a read, or a work's reading direction is neither
/// `rtl` nor `ltr`.
pub fn participants(
    conn: &Connection,
    fingerprint_version: u32,
) -> Result<Vec<Participant>, String> {
    let failed = |e: rusqlite::Error| format!("listing the files matched by content: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    let reach = reach(conn)?;
    let mut libraries_of: HashMap<i64, BTreeSet<i64>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT member_id, root_id FROM member_root")
            .map_err(failed)?;
        let mut rows = stmt.query([]).map_err(failed)?;
        while let Some(row) = rows.next().map_err(failed)? {
            let (member, root): (i64, i64) =
                (row.get(0).map_err(failed)?, row.get(1).map_err(failed)?);
            if let Some(libraries) = reach.get(&root) {
                libraries_of
                    .entry(member)
                    .or_default()
                    .extend(libraries.iter().copied());
            }
        }
    }
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.sha256, f.prints, w.id, w.series_id, w.volume, w.reading
               FROM member m
               JOIN edition e ON e.id = m.edition_id
               JOIN work w ON w.id = e.work_id
               JOIN fingerprint f ON f.sha256 = m.sha256
              WHERE f.outcome = 'whole' AND f.version = ?1 AND w.volume IS NOT NULL
                AND (SELECT count(*) FROM member o WHERE o.edition_id = m.edition_id) = 1
              ORDER BY m.sha256, m.id",
        )
        .map_err(failed)?;
    let mut rows = stmt.query([fingerprint_version]).map_err(failed)?;
    let mut taking_part: Vec<Participant> = Vec::new();
    while let Some(row) = rows.next().map_err(failed)? {
        let member: i64 = row.get(0).map_err(failed)?;
        let sha256: String = row.get(1).map_err(failed)?;
        let Some(libraries) = libraries_of.remove(&member) else {
            continue;
        };
        if taking_part.last().is_some_and(|last| last.sha256 == sha256) {
            continue;
        }
        let reading: String = row.get(6).map_err(failed)?;
        taking_part.push(Participant {
            prints: row.get(2).map_err(failed)?,
            work_id: row.get(3).map_err(failed)?,
            series_id: row.get(4).map_err(failed)?,
            volume: row.get(5).map_err(failed)?,
            reading: match reading.as_str() {
                "rtl" => Direction::RightToLeft,
                "ltr" => Direction::LeftToRight,
                other => {
                    return Err(format!("{sha256}'s work reads `{other}`, not rtl or ltr"));
                }
            },
            sha256,
            libraries,
        });
    }
    Ok(taking_part)
}

/// Every digest the match pass has looked up.
///
/// # Errors
///
/// If the store refuses the read.
pub fn looked_up(conn: &Connection) -> Result<HashSet<String>, String> {
    conn.prepare("SELECT sha256 FROM match_lookup")
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .map_err(|e| format!("listing the files looked up: {e}"))
}

/// Every stored pair's `(sha_a, sha_b)`, the smaller digest first, split or
/// not, so the pass scores no pair twice.
///
/// # Errors
///
/// If the store refuses the read.
pub fn scored(conn: &Connection) -> Result<HashSet<(String, String)>, String> {
    conn.prepare("SELECT sha_a, sha_b FROM content_match")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .map_err(|e| format!("listing the pairs scored: {e}"))
}

/// Store one file's lookup: `pairs`, each turned to put its smaller digest
/// first, and the mark that `looked_up` was looked up under these versions,
/// all in one transaction. A pair already stored keeps its row, a Split
/// included.
///
/// # Errors
///
/// If a pair is under [`FLOOR`], when nothing is written, or the store
/// refuses a write.
pub fn store(
    conn: &Connection,
    pairs: &[NewPair],
    looked_up: &str,
    fingerprint_version: u32,
    scorer_version: u32,
) -> Result<(), String> {
    let kept = |pair: &NewPair| pair.v >= FLOOR;
    if let Some(under) = pairs.iter().find(|&pair| !kept(pair)) {
        return Err(format!(
            "{} and {} are under the floor at V {}, so they are not stored",
            under.sha_a, under.sha_b, under.v
        ));
    }
    let failed = |e: rusqlite::Error| format!("storing the pairs of {looked_up}: {e}");
    let tx = Nested::open(conn, "matches_store").map_err(failed)?;
    for pair in pairs {
        let pair = if pair.sha_a > pair.sha_b {
            pair.turned()
        } else {
            pair.clone()
        };
        let runs: Vec<[u32; 4]> = pair
            .runs
            .iter()
            .map(|(a, b)| [a.start, a.end, b.start, b.end])
            .collect();
        let runs = serde_json::to_string(&runs).map_err(|e| format!("writing runs: {e}"))?;
        tx.execute(
            "INSERT INTO content_match
               (sha_a, sha_b, v, long_paired, a_slots, b_slots, a_pages, b_pages, floor, runs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (sha_a, sha_b) DO NOTHING",
            rusqlite::params![
                pair.sha_a,
                pair.sha_b,
                pair.v,
                pair.long_paired,
                pair.a_slots,
                pair.b_slots,
                pair.a_pages,
                pair.b_pages,
                pair.floor,
                runs,
            ],
        )
        .map_err(failed)?;
    }
    tx.execute(
        "INSERT INTO match_lookup (sha256, fingerprint_version, scorer_version)
         VALUES (?1, ?2, ?3)
         ON CONFLICT (sha256) DO UPDATE SET
           fingerprint_version = excluded.fingerprint_version,
           scorer_version = excluded.scorer_version, looked_at = datetime('now')",
        rusqlite::params![looked_up, fingerprint_version, scorer_version],
    )
    .map_err(failed)?;
    tx.commit().map_err(failed)
}

/// Every stored pair, by id.
///
/// # Errors
///
/// If the store refuses the read, or a row holds a state or runs this build
/// does not read.
pub fn pairs(conn: &Connection) -> Result<Vec<Pair>, String> {
    let failed = |e: rusqlite::Error| format!("reading the pairs scored: {e}");
    let mut stmt = conn
        .prepare(
            "SELECT id, sha_a, sha_b, v, long_paired, a_slots, b_slots, a_pages, b_pages, floor,
                    runs, state
               FROM content_match ORDER BY id",
        )
        .map_err(failed)?;
    let mut rows = stmt.query([]).map_err(failed)?;
    let mut pairs = Vec::new();
    while let Some(row) = rows.next().map_err(failed)? {
        let id: i64 = row.get(0).map_err(failed)?;
        let runs: String = row.get(10).map_err(failed)?;
        let state: String = row.get(11).map_err(failed)?;
        pairs.push(Pair {
            id,
            new: NewPair {
                sha_a: row.get(1).map_err(failed)?,
                sha_b: row.get(2).map_err(failed)?,
                v: row.get(3).map_err(failed)?,
                long_paired: row.get(4).map_err(failed)?,
                a_slots: row.get(5).map_err(failed)?,
                b_slots: row.get(6).map_err(failed)?,
                a_pages: row.get(7).map_err(failed)?,
                b_pages: row.get(8).map_err(failed)?,
                floor: row.get(9).map_err(failed)?,
                runs: parse_runs(&runs).map_err(|why| format!("pair {id}'s runs: {why}"))?,
            },
            state: State::parse(&state)
                .ok_or_else(|| format!("pair {id} has an unknown state {state}"))?,
        });
    }
    Ok(pairs)
}

/// `content_match.runs`, refusing a range that ends before it starts, which
/// [`PageRange::new`] would only catch in a debug build.
fn parse_runs(text: &str) -> Result<Vec<(PageRange, PageRange)>, String> {
    let runs: Vec<[u32; 4]> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    runs.into_iter()
        .map(|[a_start, a_end, b_start, b_end]| {
            if a_start <= a_end && b_start <= b_end {
                Ok((
                    PageRange::new(a_start, a_end),
                    PageRange::new(b_start, b_end),
                ))
            } else {
                Err(format!(
                    "[{a_start}, {a_end}, {b_start}, {b_end}] runs backwards"
                ))
            }
        })
        .collect()
}

/// Set each pair of `ids` not already in `state` to it, and say how many
/// changed. A Split records `by`, the time and its group, the first of
/// `ids` ([`splits_for`] puts the pair it names first); Undo clears all
/// three, so a pair split twice keeps the first Split's. An id no pair holds
/// changes nothing.
///
/// # Errors
///
/// If the store refuses a write; then none is kept.
pub fn set_state(
    conn: &Connection,
    ids: &[i64],
    state: State,
    by: Option<i64>,
) -> Result<usize, String> {
    let failed = |e: rusqlite::Error| format!("marking pairs {state:?}: {e}");
    let split = state == State::Split;
    let tx = Nested::open(conn, "matches_set_state").map_err(failed)?;
    let mut changed = 0;
    for id in ids {
        changed += tx
            .execute(
                "UPDATE content_match
                    SET state = ?2, split_by = ?3,
                        split_at = CASE WHEN ?4 THEN datetime('now') END, split_group = ?5
                  WHERE id = ?1 AND state <> ?2",
                rusqlite::params![
                    id,
                    state.as_str(),
                    by.filter(|_| split),
                    split,
                    ids.first().filter(|_| split)
                ],
            )
            .map_err(failed)?;
    }
    tx.commit().map_err(failed)?;
    Ok(changed)
}

/// What a Split of file `sha256` on pair `id` sets apart: `id`, then by id
/// every other pair from [`BAR`] up, not split, between `sha256` and a file
/// of a work holding it, any of which would join it to that work again at the
/// next derive (spec §5's triangle). `None` where no pair is `id` or `sha256`
/// is neither of its files. The file is named because the pair cannot say
/// which leaves: two files of one work can both carry `id` as `joined_by`.
///
/// # Errors
///
/// If the store refuses a read.
pub fn splits_for(conn: &Connection, id: i64, sha256: &str) -> Result<Option<Vec<i64>>, String> {
    let failed = |e: rusqlite::Error| format!("reading what a split of pair {id} splits: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    let named = conn
        .query_row(
            "SELECT 1 FROM content_match WHERE id = ?1 AND ?2 IN (sha_a, sha_b)",
            rusqlite::params![id, sha256],
            |_| Ok(()),
        )
        .optional()
        .map_err(failed)?;
    if named.is_none() {
        return Ok(None);
    }
    let others: Vec<i64> = conn
        .prepare(
            "WITH mates AS (SELECT m.sha256 FROM member m JOIN edition e ON e.id = m.edition_id
                             WHERE m.sha256 <> ?2
                               AND e.work_id IN (SELECT e.work_id FROM member m
                                                   JOIN edition e ON e.id = m.edition_id
                                                  WHERE m.sha256 = ?2))
             SELECT id FROM content_match
              WHERE id <> ?1 AND state <> ?3 AND v >= ?4
                AND ((sha_a = ?2 AND sha_b IN mates) OR (sha_b = ?2 AND sha_a IN mates))
              ORDER BY id",
        )
        .and_then(|mut stmt| {
            stmt.query_map(
                rusqlite::params![id, sha256, State::Split.as_str(), BAR],
                |r| r.get(0),
            )?
            .collect()
        })
        .map_err(failed)?;
    Ok(Some(std::iter::once(id).chain(others).collect()))
}

/// Every pair the Split that set pair `id` apart set apart, `id` among them,
/// by id, so Undo restores the whole Split at once; only `id` where it is not
/// split.
///
/// # Errors
///
/// If the store refuses a read.
pub fn split_together(conn: &Connection, id: i64) -> Result<Vec<i64>, String> {
    let failed = |e: rusqlite::Error| format!("reading what split pair {id} with it: {e}");
    let together: Vec<i64> = conn
        .prepare(
            "SELECT id FROM content_match
              WHERE state = ?2
                AND split_group = (SELECT split_group FROM content_match
                                    WHERE id = ?1 AND state = ?2)
              ORDER BY id",
        )
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![id, State::Split.as_str()], |r| r.get(0))?
                .collect()
        })
        .map_err(failed)?;
    Ok(if together.contains(&id) {
        together
    } else {
        vec![id]
    })
}

/// What an admin is shown of one work's content matches (spec §6).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// Each edition a join brought in by its own file, by key, with the pair
    /// that brought it and the pair's V. An edition that moved only because
    /// its group did carries a pair naming another file: Split on it would
    /// split a pair it is not in, so it has no mark.
    pub matched: BTreeMap<String, (i64, f64)>,
    /// Each split pair one of the work's files is in, by pair id.
    pub kept_apart: Vec<KeptApart>,
}

/// A split pair as one of its files' works shows it: the pair, which Undo
/// names, and the work the other file is in now.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptApart {
    pub id: i64,
    /// The Split that set it apart, as the pair that Split named: the panel
    /// draws one line and one Undo for each.
    pub split: i64,
    /// That work's series as the browse screens title it, or the work's own
    /// title where it is in no series.
    pub series: String,
    pub volume: Option<f64>,
    pub slug: String,
}

/// Work `work_id`'s [`Marks`]. A split pair whose other file no work holds
/// any more names nothing, so it is left out.
///
/// # Errors
///
/// If the store refuses a read.
pub fn marks(conn: &Connection, work_id: i64) -> Result<Marks, String> {
    let failed = |e: rusqlite::Error| format!("reading work {work_id}'s content matches: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    let matched = conn
        .prepare(
            "SELECT e.key, cm.id, cm.v
               FROM edition e
               JOIN content_match cm ON cm.id = e.joined_by
              WHERE e.work_id = ?1
                AND EXISTS (SELECT 1 FROM member m
                             WHERE m.edition_id = e.id AND m.sha256 IN (cm.sha_a, cm.sha_b))",
        )
        .and_then(|mut stmt| {
            stmt.query_map([work_id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
                .collect()
        })
        .map_err(failed)?;
    // The other file's work is its first member's, as [`participants`] picks
    // one per digest, and its series is titled as `browse::series` titles it,
    // so the panel names the series the grid shows.
    let kept_apart = conn
        .prepare(
            "WITH here AS (SELECT m.sha256 FROM member m JOIN edition e ON e.id = m.edition_id
                            WHERE e.work_id = ?1),
                  kept AS (SELECT cm.id, coalesce(cm.split_group, cm.id) AS split,
                                  CASE WHEN cm.sha_a IN (SELECT sha256 FROM here)
                                       THEN cm.sha_b ELSE cm.sha_a END AS other
                             FROM content_match cm
                            WHERE cm.state = 'split'
                              AND (cm.sha_a IN (SELECT sha256 FROM here)
                                   OR cm.sha_b IN (SELECT sha256 FROM here)))
             SELECT k.id, k.split, w.volume, w.slug,
                    COALESCE((SELECT MAX(title) FROM series_title
                               WHERE series_id = w.series_id AND is_primary = 1),
                             (SELECT title FROM series_title WHERE series_id = w.series_id
                               ORDER BY rowid LIMIT 1),
                             (SELECT sort_title FROM series WHERE id = w.series_id),
                             w.title)
               FROM kept k
               JOIN work w ON w.id = (SELECT e.work_id FROM member m
                                        JOIN edition e ON e.id = m.edition_id
                                       WHERE m.sha256 = k.other ORDER BY m.id LIMIT 1)
              ORDER BY k.id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([work_id], |r| {
                Ok(KeptApart {
                    id: r.get(0)?,
                    split: r.get(1)?,
                    volume: r.get(2)?,
                    slug: r.get(3)?,
                    series: r.get(4)?,
                })
            })?
            .collect()
        })
        .map_err(failed)?;
    Ok(Marks {
        matched,
        kept_apart,
    })
}

#[cfg(test)]
mod tests;
