//! The sweep of the cover and page caches (O11): the renderings no member
//! holds, and the staging files a crashed write left, deleted by name; then
//! the least recently used renderings, down to the cap.
//!
//! Every name `cover` mints for a rendering starts with the member's digest,
//! so the name alone says whether the store still holds what it renders. A
//! name that does not parse is left and counted: the sweep deletes only what
//! it can tell the cache made. Spec §6 of
//! `docs/superpowers/specs/2026-11-10-pre-deploy-design.md`.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::{ErrorKind, Write as _};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::ServeState;

/// How often a running server sweeps, after the sweep at its start.
pub(crate) const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Sweeps `state`'s caches at once and then every [`EVERY`], each sweep's line
/// on stderr; `None` for a fixed registry, not swept: the fixtures and
/// `corpus.rs` pair one with an empty library, whose renderings no `member`
/// row holds, and the deployed server's registry is live (spec §6).
///
/// `serve` calls this before `axum::serve` accepts, so `started` is taken
/// before any request is served. A staging file this process writes follows a
/// request and a decode, so a filesystem clock a tick behind cannot date it
/// before `started` and make it a leftover.
pub(crate) fn spawn(state: &Arc<ServeState>, cap: u64) -> Option<tokio::task::JoinHandle<()>> {
    if !state.works.is_live() {
        return None;
    }
    let started = SystemTime::now();
    let state = Arc::clone(state);
    Some(every(EVERY, move || {
        let line = match sweep(&state, cap, started) {
            Ok(swept) => report(&swept),
            Err(why) => format!("could not sweep the cache: {why}"),
        };
        // Not `eprintln!`, which panics when nothing reads stderr.
        let _ = writeln!(std::io::stderr(), "{line}");
    }))
}

/// Runs `tick` at once and then every `period`, each on the blocking pool,
/// since a sweep lists directories and reads the database. Each tick is
/// awaited before the next, so two never overlap. A panicking tick is
/// reported and the schedule goes on.
pub(crate) fn every(
    period: Duration,
    tick: impl Fn() + Send + Sync + 'static,
) -> tokio::task::JoinHandle<()> {
    let tick = Arc::new(tick);
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(period);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            let ticking = Arc::clone(&tick);
            if let Err(panicked) = tokio::task::spawn_blocking(move || ticking()).await {
                let _ = writeln!(
                    std::io::stderr(),
                    "taiyaku: the cache sweep panicked: {panicked}"
                );
            }
        }
    })
}

/// [`sweep_with`] over `state`'s cover and page caches and its library.
pub(crate) fn sweep(state: &ServeState, cap: u64, started: SystemTime) -> Result<Swept, String> {
    let dirs = [state.cache_dir.clone(), state.thumb_cache_dir()];
    sweep_with(&dirs, &state.library, cap, started, || {})
}

/// One sweep's line for stderr.
pub(crate) fn report(swept: &Swept) -> String {
    let Swept {
        orphans,
        leftovers,
        evicted,
        left,
        unrecognised,
        undeleted,
        first_failure,
    } = swept;
    let failure = match first_failure {
        Some(first) => format!("; {undeleted} could not be deleted, the first: {first}"),
        None => String::new(),
    };
    format!(
        "cache sweep: deleted {} orphan(s) ({} bytes), {} leftover(s) ({} bytes) and {} \
         eviction(s) ({} bytes); {} file(s) ({} bytes) left, {unrecognised} of them not \
         recognised{failure}",
        orphans.files,
        orphans.bytes,
        leftovers.files,
        leftovers.bytes,
        evicted.files,
        evicted.bytes,
        left.files,
        left.bytes,
    )
}

/// What a file name in the cache says it is.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Parsed<'a> {
    /// A rendering of the member whose digest this is.
    Key { digest: &'a str },
    /// A rendering's staging file. It carries no digest: a leftover goes by
    /// its age whatever it renders.
    Staging,
}

/// What `name`, taken whole, is: a rendering's name as `key` reads it, or one
/// followed by `.tmp.{pid}.{n}` as `taiyaku_read::atomic::temp_path` mints it.
///
/// A digest must be the 64 lowercase hex characters
/// `taiyaku_read::member_digest` mints. No stranger's digest is held, so under
/// a `--cache` root set by mistake over someone else's `covers` or `thumbs`,
/// anything taken for a key would be deleted.
pub(crate) fn parse(name: &str) -> Option<Parsed<'_>> {
    if let Some(digest) = key(name) {
        return Some(Parsed::Key { digest });
    }
    let (front, counter) = name.rsplit_once('.')?;
    let (front, pid) = front.rsplit_once('.')?;
    let front = front.strip_suffix(".tmp")?;
    (digits(pid) && digits(counter) && key(front).is_some()).then_some(Parsed::Staging)
}

/// The digest of a cover `{digest}-{edge}v{n}.jpg`, a tile
/// `{digest}-p{offset}-{edge}v{n}.jpg` or a reading page
/// `{digest}-p{offset}-r{edge}v{n}.jpg`. Any edge, offset and version, so a
/// file of an old version or rung is still a key.
fn key(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".jpg")?;
    let (digest, rest) = stem.split_at_checked(64)?;
    if !digest
        .bytes()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return None;
    }
    let rest = rest.strip_prefix('-')?;
    let sized = match rest.strip_prefix('p') {
        Some(paged) => {
            let (offset, sized) = paged.split_once('-')?;
            if !digits(offset) {
                return None;
            }
            sized.strip_prefix('r').unwrap_or(sized)
        }
        None => rest,
    };
    let (edge, version) = sized.split_once('v')?;
    (digits(edge) && digits(version)).then_some(digest)
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// A number of files and the bytes they hold.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tally {
    files: u64,
    bytes: u64,
}

impl Tally {
    fn add(&mut self, bytes: u64) {
        self.files += 1;
        self.bytes += bytes;
    }

    fn take(&mut self, bytes: u64) {
        self.files -= 1;
        self.bytes -= bytes;
    }
}

/// What one sweep deleted, and what it left.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Swept {
    /// Renderings whose digest no member holds.
    orphans: Tally,
    /// Staging files last modified before the sweep's task was spawned
    /// (`started`).
    leftovers: Tally,
    /// Keyed files deleted, least recently used first, to bring `left` to the
    /// cap.
    evicted: Tally,
    /// Every regular file still in the cache, whatever its name.
    left: Tally,
    /// The files of `left` whose name does not parse.
    unrecognised: u64,
    /// The files of `left` that were to go and could not be deleted.
    undeleted: u64,
    /// The first of those failures, naming its file.
    first_failure: Option<String>,
}

impl Swept {
    fn failed(&mut self, failure: String) {
        self.undeleted += 1;
        self.first_failure.get_or_insert(failure);
    }
}

/// A regular file the listing found.
struct Listed {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

/// Sweep `dirs` against the digests `library`'s members hold, then evict keyed
/// files down to `cap`, 0 for none, counting every file left ([`evict`]).
///
/// The listing comes before the digest read. A rendering is written for a
/// digest the store already holds, so a listed file's digest is in the read
/// that follows unless its member has gone since. Read first, a member added
/// and rendered between the read and the listing would lose its new file.
///
/// A failed listing or digest read ends the sweep before any delete: an unread
/// store is not an empty one. A failed delete stays in `left`, passed over.
pub(crate) fn sweep_with(
    dirs: &[PathBuf],
    library: &taiyaku_library::Library,
    cap: u64,
    started: SystemTime,
    after_listing: impl FnOnce(),
) -> Result<Swept, String> {
    let listed = list(dirs)?;
    after_listing();
    let held = held(library)?;

    let mut swept = Swept::default();
    let mut kept_keys = Vec::new();
    for file in &listed {
        let parsed = file
            .path
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(parse);
        let tally = match parsed {
            Some(Parsed::Key { digest }) if !held.contains(digest) => &mut swept.orphans,
            // Told by age, as the pid cannot: in the container the server is
            // PID 1 on every start. A staging file of this process is younger
            // than `started`, but one reusing a listed leftover's name goes.
            Some(Parsed::Staging) if file.modified < started => &mut swept.leftovers,
            Some(Parsed::Key { .. }) => {
                kept_keys.push(file);
                swept.left.add(file.bytes);
                continue;
            }
            Some(Parsed::Staging) => {
                swept.left.add(file.bytes);
                continue;
            }
            None => {
                swept.unrecognised += 1;
                swept.left.add(file.bytes);
                continue;
            }
        };
        match delete(file) {
            Ok(()) => tally.add(file.bytes),
            Err(failure) => {
                swept.left.add(file.bytes);
                swept.failed(failure);
            }
        }
    }
    if cap != 0 {
        evict(&mut swept, kept_keys, cap);
    }
    Ok(swept)
}

/// Deletes keyed files, least recently used first, until `left` is at or
/// under `cap` or none is left. Every regular file counts in `left`, since
/// each takes the disk, but only a keyed file is the cache's to remake: so
/// files it may not evict that are over the cap alone leave the sweep over it.
/// Ties go by name, so the order is total. A failed delete is passed over.
fn evict(swept: &mut Swept, mut keys: Vec<&Listed>, cap: u64) {
    keys.sort_by(|a, b| {
        a.modified
            .cmp(&b.modified)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
    let mut oldest_first = keys.into_iter();
    while swept.left.bytes > cap {
        let Some(file) = oldest_first.next() else {
            break;
        };
        match delete(file) {
            Ok(()) => {
                swept.evicted.add(file.bytes);
                swept.left.take(file.bytes);
            }
            Err(failure) => swept.failed(failure),
        }
    }
}

/// Deletes `file`. One already gone counts as deleted: what the sweep wanted
/// is done.
fn delete(file: &Listed) -> Result<(), String> {
    match std::fs::remove_file(&file.path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {error}", file.path.display())),
    }
}

/// Every regular file in `dirs`. A directory not made yet is an empty one;
/// a symlink or a directory inside one is not the cache's to count or delete.
fn list(dirs: &[PathBuf]) -> Result<Vec<Listed>, String> {
    let mut listed = Vec::new();
    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("could not list {}: {error}", dir.display())),
        };
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("could not list {}: {error}", dir.display()))?;
            let path = entry.path();
            // Does not follow a symlink.
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("could not read {}: {error}", path.display())),
            };
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata
                .modified()
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            listed.push(Listed {
                path,
                bytes: metadata.len(),
                modified,
            });
        }
    }
    Ok(listed)
}

/// The digests the store's members hold, the connection held for this one
/// statement.
fn held(library: &taiyaku_library::Library) -> Result<HashSet<String>, String> {
    let failed = |error: rusqlite::Error| format!("could not read the held digests: {error}");
    let conn = library.conn();
    let mut statement = conn
        .prepare("SELECT DISTINCT sha256 FROM member")
        .map_err(failed)?;
    statement
        .query_map([], |row| row.get(0))
        .map_err(failed)?
        .collect::<Result<HashSet<String>, _>>()
        .map_err(failed)
}

#[cfg(test)]
mod tests;
