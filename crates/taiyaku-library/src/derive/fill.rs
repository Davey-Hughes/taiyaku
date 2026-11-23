//! The filling derive and its early pass (O314): a scan job hashes its files
//! series by series, and while files are still being hashed the derive in
//! [`Mode::Early`] writes new volumes whose files are all hashed, so a reader
//! sees them before the job ends; [`early_writes`] holds some back for the
//! full derive (design §7's three rules). Design
//! `docs/superpowers/specs/2026-11-22-first-scan-series-as-they-finish-design.md`,
//! §7, "The hashing queue" to "Cancel".

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use rusqlite::Connection;

use super::{
    DeriveReport, Documents, Entry, Group, GroupKey, Mode, Placed, Setup, derive_in, digest,
    held_work, own_key_of, parent_of, read_input, series_id_of, setup,
};
use crate::progress::{Outcome, Step, Watch};
use crate::scan::Root;

/// When the filling derive runs an early pass: once a group completes and
/// enough time has passed, the longer of `floor` and `factor` times the
/// last pass.
pub struct Cadence<'a> {
    /// The least time between passes, so a small store does not pass after
    /// every volume.
    pub floor: Duration,
    /// The hashing that must follow a pass, as a multiple of the pass's own
    /// time, so passes take a bounded share of the job however their cost
    /// grows with the store.
    pub factor: u32,
    /// The clock both are read on.
    pub now: &'a dyn Fn() -> Instant,
}

impl std::fmt::Debug for Cadence<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cadence")
            .field("floor", &self.floor)
            .field("factor", &self.factor)
            .finish_non_exhaustive()
    }
}

/// A scan job's floor (design call 5): ten seconds keeps a small store from
/// passing after every volume.
const FLOOR: Duration = Duration::from_secs(10);

/// A scan job's factor (design call 5): a pass of `p` is followed by at
/// least `4p` of hashing, so passes take at most a fifth of the job.
const FACTOR: u32 = 4;

/// [`derive_filling_at`] at a scan job's cadence, on the system clock.
///
/// # Errors
///
/// As [`super::derive_with`].
pub fn derive_filling(
    conn: &Connection,
    documents: Documents,
    fresh: &[i64],
    watch: &dyn Watch,
) -> Result<Outcome<DeriveReport>, String> {
    derive_filling_at(
        conn,
        documents,
        fresh,
        watch,
        &Cadence {
            floor: FLOOR,
            factor: FACTOR,
            now: &Instant::now,
        },
    )
}

/// A scan job's derive (O314, design §7): what [`super::derive_watched`]
/// would hash, hashed in series order across every directory, with an early
/// pass at `cadence` once a group completes and [`Watch::written`] after each
/// that wrote a volume; then the full derive, whose outcome this answers and
/// whose Hashing reports it drops. Each reads the input as it was at the start.
///
/// A cancel leaves what a stop at that point would. Before the full derive it
/// answers [`Step::Hashing`], the step the job shows.
///
/// # Errors
///
/// As [`super::derive_with`].
pub fn derive_filling_at(
    conn: &Connection,
    documents: Documents,
    fresh: &[i64],
    watch: &dyn Watch,
    cadence: &Cadence<'_>,
) -> Result<Outcome<DeriveReport>, String> {
    let setup = setup(conn, fresh)?;
    // As `grouped` shows it: sizing the folders can take a while on a share.
    watch.at(Step::Hashing, 0, None);
    // What it counts is the full derive's to report, over the same input.
    let mut scratch = DeriveReport::default();
    let Setup {
        engine,
        roots,
        frozen,
    } = &setup;
    let input = match read_input(conn, engine, roots, frozen, watch, &mut scratch, true)? {
        Outcome::Done(input) => input,
        Outcome::Cancelled(_) => return Ok(Outcome::Cancelled(Step::Hashing)),
    };
    // Only what `read_input` sized: an entry `hashes` passes over holds its
    // digest already.
    let mut queue: Vec<Queued<'_>> = input
        .into_iter()
        .flat_map(|(root, kept)| {
            kept.into_iter().filter_map(move |(seen, bytes)| {
                let bytes = bytes?;
                Some(Queued {
                    key: own_key_of(&seen),
                    root,
                    entry: seen.entry,
                    bytes,
                })
            })
        })
        .collect();
    queue.sort_by(|a, b| {
        (&a.key, a.root.id, &a.entry.rel_path).cmp(&(&b.key, b.root.id, &b.entry.rel_path))
    });
    let total: u64 = queue.iter().map(|queued| queued.bytes).sum();
    let mut hashed: u64 = 0;
    let mut written: u64 = 0;
    // Once sizing is done, so the first pass waits from when hashing began,
    // not from a walk of the folders.
    let mut last_end = (cadence.now)();
    let mut last_pass = Duration::ZERO;
    for (at, queued) in queue.iter().enumerate() {
        watch.at(Step::Hashing, hashed, Some(total));
        if watch.cancelled() {
            return Ok(Outcome::Cancelled(Step::Hashing));
        }
        // A file gone answers no digest and is left unhashed, for the full
        // derive to find gone again and count.
        digest(conn, queued.root, &queued.entry)?;
        hashed += queued.bytes;
        let completes = queue.get(at + 1).is_some_and(|next| next.key != queued.key);
        if !completes
            || !pass_due(
                (cadence.now)().saturating_duration_since(last_end),
                last_pass,
                cadence,
            )
        {
            continue;
        }
        // A cancel asked during the digest stops the job before a pass reads
        // the store (design call 7).
        if watch.cancelled() {
            return Ok(Outcome::Cancelled(Step::Hashing));
        }
        let started = (cadence.now)();
        let works = match early_pass(conn, documents, &setup, watch)? {
            Outcome::Done(works) => works,
            // The step the job shows: a pass runs inside Hashing.
            Outcome::Cancelled(_) => return Ok(Outcome::Cancelled(Step::Hashing)),
        };
        last_end = (cadence.now)();
        last_pass = last_end.saturating_duration_since(started);
        if works > 0 {
            written += works;
            watch.written(written);
        }
    }
    watch.at(Step::Hashing, hashed, Some(total));
    if watch.cancelled() {
        return Ok(Outcome::Cancelled(Step::Hashing));
    }
    derive_in(conn, documents, &setup, &NoHashing(watch), Mode::Full)
}

/// One entry the filling derive hashes: its own key, which orders the queue
/// and says when its group completes, where it is, and the bytes it reads.
struct Queued<'r> {
    key: GroupKey,
    root: &'r Root,
    entry: Entry,
    bytes: u64,
}

/// Whether a pass is due, `since_last_end` after the last pass ended (or
/// hashing began) and the last pass having taken `last_pass`: once the
/// longer of the floor and `factor` times the last pass has gone by.
fn pass_due(since_last_end: Duration, last_pass: Duration, cadence: &Cadence<'_>) -> bool {
    since_last_end >= cadence.floor.max(last_pass.saturating_mul(cadence.factor))
}

/// A [`Watch`] that drops every Hashing report and forwards the rest and the
/// cancel flag, for the full derive the filling derive ends with: its files
/// are hashed by then, so its Hashing step would take the job's line back to
/// sizing and a zero total on its way to Writing.
struct NoHashing<'a>(&'a dyn Watch);

impl Watch for NoHashing<'_> {
    fn at(&self, step: Step, done: u64, of: Option<u64>) {
        if step != Step::Hashing {
            self.0.at(step, done, of);
        }
    }

    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }
}

/// One early pass over the [`Setup`] the job's full derive reads, under
/// the job's `documents`, so a hand document at a new work's
/// address applies as it would at the end. Answers its report's
/// [`works`](super::DeriveReport::works), which counts only the volumes it
/// inserted, since [`early_writes`] keeps no group a work holds, and drops the
/// rest: the job's full derive reports every volume, these among them.
///
/// It reports no step through `watch`, whose Hashing count the job is
/// showing; a cancel still reaches it before each work.
///
/// # Errors
///
/// As [`super::derive_with`].
pub(super) fn early_pass(
    conn: &Connection,
    documents: Documents,
    setup: &Setup,
    watch: &dyn Watch,
) -> Result<Outcome<u64>, String> {
    Ok(
        match derive_in(conn, documents, setup, &Quiet(watch), Mode::Early)? {
            Outcome::Done(report) => Outcome::Done(u64::try_from(report.works).unwrap_or(u64::MAX)),
            Outcome::Cancelled(step) => Outcome::Cancelled(step),
        },
    )
}

/// A [`Watch`] that drops every report and forwards the cancel flag. The
/// early pass makes the reports the full derive makes, and they are the only
/// thing this silences.
struct Quiet<'a>(&'a dyn Watch);

impl Watch for Quiet<'_> {
    fn at(&self, _step: Step, _done: u64, _of: Option<u64>) {}

    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }
}

/// The groups an early pass writes, of every group it placed: those keeping
/// design §7's three rules, so a volume a reader sees early is written once,
/// whole, where the full derive leaves it. 1: no entry still to hash names it
/// by its own key, so it holds every member and the hub one derive gives it, a
/// copy aside ([`super::own_key_of`]). 2: no work holds it ([`address_held`],
/// [`bytes_held`]), so its write is an insert and the pass counts new volumes
/// alone. 3: none of its files sits in a directory a stored match names
/// ([`named_directories`]), so no join or follow at the end moves it.
pub(super) fn early_writes(
    conn: &Connection,
    groups: &BTreeMap<GroupKey, Group>,
    waiting: &BTreeSet<GroupKey>,
    placed: &Placed,
) -> Result<BTreeSet<GroupKey>, String> {
    let named = named_directories(conn)?;
    let mut series: HashMap<&str, Option<i64>> = HashMap::new();
    let mut writes = BTreeSet::new();
    for (key, group) in groups {
        if waiting.contains(key) || in_a_named_directory(group, &named) {
            continue;
        }
        let series_id = if let Some(id) = series.get(group.series.as_str()) {
            *id
        } else {
            let id = series_id_of(conn, &group.series)?;
            series.insert(&group.series, id);
            id
        };
        // No series row, no work in it: only the digests can name a work.
        if let Some(series_id) = series_id
            && address_held(conn, series_id, group, placed)?
        {
            continue;
        }
        if bytes_held(conn, group)? {
            continue;
        }
        writes.insert(key.clone());
    }
    Ok(writes)
}

/// Whether a work stands at `group`'s address in series `series_id`. With a
/// volume, any work at it, which reads the design's "no row at its address"
/// more strictly than [`held_work`] does: that passes over a row whose
/// content another group claims, and the full derive moves such a row to that
/// group, so a group written early beside it would show as a second work at
/// one address until then. Without a volume, [`held_work`]'s own answer.
fn address_held(
    conn: &Connection,
    series_id: i64,
    group: &Group,
    placed: &Placed,
) -> Result<bool, String> {
    let Some(volume) = group.volume else {
        return Ok(held_work(conn, series_id, group, placed)?.is_some());
    };
    conn.prepare_cached("SELECT EXISTS (SELECT 1 FROM work WHERE series_id = ?1 AND volume IS ?2)")
        .and_then(|mut stmt| stmt.query_row(rusqlite::params![series_id, volume], |r| r.get(0)))
        .map_err(|e| {
            format!(
                "looking for a work at volume {volume} of `{}`: {e}",
                group.series
            )
        })
}

/// Whether any member holds the bytes of one of `group`'s files, a copy's
/// being its primary's. Such bytes wait for the full derive to say where they
/// go: its import finds a work by content before its address, so a pass
/// writing them could take a work from under its own address.
fn bytes_held(conn: &Connection, group: &Group) -> Result<bool, String> {
    let mut held = conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM member WHERE sha256 = ?1)")
        .map_err(|e| {
            format!(
                "looking for a member holding a file of `{}`: {e}",
                group.series
            )
        })?;
    for candidate in group.editions.values().flatten() {
        if held
            .query_row([&candidate.sha256], |r| r.get(0))
            .map_err(|e| format!("looking for a member holding `{}`: {e}", candidate.path))?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every directory (root, then parent path) holding a file a stored match
/// names, in any state and at any score: wider than the pairs a join applies
/// (design call 4), as which apply is the joins' own reading and an Undo can
/// restore a split one. A file is named by its digest, or by the reading it
/// holds while it waits to be hashed again, which a pair still names when
/// only the file's time changed: hashed at the full derive, it joins, and its
/// directory follows.
fn named_directories(conn: &Connection) -> Result<HashMap<i64, HashSet<String>>, String> {
    let failed = |e: rusqlite::Error| format!("reading the directories a stored match names: {e}");
    let mut stmt = conn
        .prepare(
            "WITH named (sha256) AS (SELECT sha_a FROM content_match
                                     UNION SELECT sha_b FROM content_match)
             SELECT root_id, rel_path FROM scan_entry
              WHERE sha256 IN (SELECT sha256 FROM named)
                 OR held_sha256 IN (SELECT sha256 FROM named)",
        )
        .map_err(failed)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(failed)?;
    let mut named: HashMap<i64, HashSet<String>> = HashMap::new();
    for row in rows {
        let (root_id, rel_path) = row.map_err(failed)?;
        named
            .entry(root_id)
            .or_default()
            .insert(parent_of(&rel_path).to_owned());
    }
    Ok(named)
}

/// Whether one of `group`'s files, a primary or a copy, sits in a directory
/// [`named_directories`] answered.
fn in_a_named_directory(group: &Group, named: &HashMap<i64, HashSet<String>>) -> bool {
    group.editions.values().flatten().any(|candidate| {
        std::iter::once((candidate.root_id, &candidate.rel_path))
            .chain(
                candidate
                    .copies
                    .iter()
                    .map(|copy| (copy.root_id, &copy.rel_path)),
            )
            .any(|(root_id, rel_path)| {
                named
                    .get(&root_id)
                    .is_some_and(|directories| directories.contains(parent_of(rel_path)))
            })
    })
}

#[cfg(test)]
mod tests;
