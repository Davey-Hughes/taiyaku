//! The walk: what is on disk, recorded, with nothing decided about it.
//!
//! §2 of the scanner spec — a scan produces facts, and rules turn facts into
//! structure. Nothing in this module knows what a series is. `derive.rs` is
//! the module that reads these rows and decides, and it never touches the
//! filesystem.
//!
//! **Three passes. The first two are borrowed from `inventory::walk`'s shape
//! rather than its policy**: `measure` walks the tree once and sums every
//! directory's recursive page total, and `flatten`/the recording loop is the
//! second. `inventory` needs that two-pass split because it must know
//! whether a *child* qualifies before deciding a parent's fate; this module
//! needs it because a directory-format edition's page count is the sum of
//! what is beneath it, and a single pass would have to record a row before it
//! could know that number. **The third has no `inventory` counterpart**:
//! between measuring and recording, `scan_root` resolves every new-or-moved
//! entry's format and page count — `sniff` and `member_pages`, the expensive
//! calls the next paragraph is about — so that recording itself is nothing
//! but SQL over facts already in hand. See `Resolved`.
//!
//! **What the cheap walk actually saves, since the phrase invites the wrong
//! implementation.** `dir_state`'s mtime and entry count do NOT license
//! skipping a descent: a directory's mtime moves when an entry is added,
//! removed or renamed *in it*, and not when a file's content changes or when
//! anything at all happens two levels down. A walk that declined to recurse
//! into an unchanged directory would miss every deep change, silently, which
//! is the worst failure this module could have. What is skipped is **opening
//! containers** — `sniff` opens a file and, for anything carrying a zip
//! header, opens and parses it a second time; `member_pages` opens it again
//! to list its entries. Both are the expensive calls here, and an entry is
//! only reopened when its `(size, mtime)` has moved from what `scan_entry`
//! already holds. Hashing is skipped entirely: `sha256` stays NULL until
//! `derive.rs` needs it, per §3.
//!
//! **This module writes nothing to the library tree.** §10. The one file the
//! scanner ever creates is `watch.rs`'s probe, which is not this.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

// --- roots ---------------------------------------------------------------

/// A scan root: a directory taiyaku watches, and the defaults a scan applies
/// wherever no rule sets a field. A plain projection of the `root` row —
/// `schema/001_initial.sql`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    pub id: i64,
    pub path: PathBuf,
    pub edition_key: Option<String>,
    pub default_kind: Option<String>,
    pub default_language: Option<String>,
    pub scan_interval_secs: Option<i64>,
}

/// Every scan root this library knows about, oldest first.
///
/// # Errors
///
/// Returns an error if the query against `root` fails.
pub fn roots(conn: &Connection) -> Result<Vec<Root>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, path, edition_key, default_kind, default_language, scan_interval_secs \
             FROM root ORDER BY id",
        )
        .map_err(|e| format!("listing roots: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Root {
                id: r.get(0)?,
                path: PathBuf::from(r.get::<_, String>(1)?),
                edition_key: r.get(2)?,
                default_kind: r.get(3)?,
                default_language: r.get(4)?,
                scan_interval_secs: r.get(5)?,
            })
        })
        .map_err(|e| format!("listing roots: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing roots: {e}"))
}

/// Add a new scan root at `path`, with no defaults set.
///
/// `path` is canonicalised before it is stored, so `rel_path` is always
/// computed against a stable root and two spellings of one directory — a
/// trailing slash, a `..` that cancels out — cannot become two roots.
/// `taiyaku-cli`'s `canonical_sidecar_path` is the same move for the same
/// reason.
///
/// # Errors
///
/// Returns an error if `path` cannot be canonicalised — it must exist — or if
/// the insert fails, which includes a `path` this library already holds a
/// root for.
pub fn add_root(conn: &Connection, path: &Path) -> Result<Root, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    conn.execute(
        "INSERT INTO root (path) VALUES (?1)",
        rusqlite::params![canonical.to_string_lossy()],
    )
    .map_err(|e| format!("adding root {}: {e}", canonical.display()))?;
    Ok(Root {
        id: conn.last_insert_rowid(),
        path: canonical,
        edition_key: None,
        default_kind: None,
        default_language: None,
        scan_interval_secs: None,
    })
}

// --- the scan --------------------------------------------------------------

/// What one [`scan_root`] call found and did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    pub root: PathBuf,
    pub dirs: usize,
    pub files: usize,
    pub added: usize,
    pub updated: usize,
    pub gone: usize,
    pub refused: Option<String>,
    /// Directories beneath the root that could not be listed, one message per
    /// directory — `std::fs::read_dir`'s own refusal, naming the path.
    ///
    /// **What this module's principle — one unreadable file must not leave a
    /// library unscanned — had never actually been applied to.** A `lost+found`
    /// directory under a freshly mounted share is `0700`, owned by root; a
    /// non-root `taiyaku library scan --add /srv/manga` met it on every mount
    /// point and failed the whole root over one directory it was never going
    /// to be able to read anyway. Skipped, counted and named here instead —
    /// see [`measure_dir`] — so `Err` from [`scan_root`] is reserved for the
    /// root's own directory refusing to list, which has nothing above it to
    /// skip to.
    pub unreadable_dirs: Vec<String>,
}

impl ScanReport {
    fn for_root(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            dirs: 0,
            files: 0,
            added: 0,
            updated: 0,
            gone: 0,
            refused: None,
            unreadable_dirs: Vec::new(),
        }
    }
}

/// Walk `root` and bring `scan_entry` and `dir_state` up to date with what is
/// on disk.
///
/// Nothing about a series, a work or an edition is decided here — see the
/// module doc. A root that scanned empty while the library already holds
/// entries for it is refused rather than applied (§8.1); the returned report
/// says so through `refused` and the database is left exactly as it was.
///
/// # Errors
///
/// Returns an error if the *root's own* directory cannot be listed at all, or
/// if a symlink cycle nests past [`MAX_DEPTH`] directories, or if a database
/// statement fails, which includes `record` finding an entry classified as
/// new with nothing resolved for it: unreachable given how this function
/// builds `resolved`, but a `Result`-returning path rather than a panic if
/// that construction is ever wrong. A directory *beneath* the root that
/// refuses to list is not one of these errors — it is skipped, counted and
/// named in [`ScanReport::unreadable_dirs`] instead, the same "one unreadable
/// thing must not leave a library unscanned" trade [`pages_of`] already makes
/// for a container that will not open.
pub fn scan_root(conn: &Connection, root: &Root) -> Result<ScanReport, String> {
    let (measured, unreadable_dirs) = measure(&root.path, 0)?;
    let seen = flatten(&measured);

    // §8.1, before a single row is written.
    //
    // **The test is "nothing under the root", not "no files".** A first draft
    // of this asked whether every entry was a directory, which is true of a
    // perfectly healthy root whose members are directories of page images —
    // most of this corpus — so it would have refused every scan of one. What
    // an unmounted share actually looks like is an empty directory: the root
    // itself and nothing beneath it.
    if seen.len() <= 1 {
        let held: i64 = conn
            .query_row(
                "SELECT count(*) FROM scan_entry WHERE root_id = ?1",
                [root.id],
                |r| r.get(0),
            )
            .map_err(|e| format!("counting what this root already held: {e}"))?;
        if held > 1 {
            return Ok(ScanReport {
                root: root.path.clone(),
                dirs: 0,
                files: 0,
                added: 0,
                updated: 0,
                gone: 0,
                unreadable_dirs,
                refused: Some(format!(
                    "{} scanned empty while the library holds {held} entries for it — \
                     refusing to apply. An unmounted share, a bind mount that was not \
                     ready, or a renamed directory all look exactly like this, and \
                     applying it would drop every work under this root along with the \
                     reading positions and notes attached to them. If the root has \
                     genuinely become empty, remove it.",
                    root.path.display()
                )),
            });
        }
    }

    // Correction 5: every filesystem operation an entry might need — `sniff`,
    // `member_pages` — happens here, before `BEGIN`. `held_for` is one query
    // rather than one per entry, and both it and the resolving below run
    // outside any transaction, so the write lock `tx` takes further down is
    // held only across the pure-SQL loop that follows. See `Resolved`.
    let held = held_for(conn, root.id)?;
    let resolved: HashMap<String, Resolved> = seen
        .iter()
        .filter(|entry| !unchanged(held.get(&entry.rel_path).copied(), entry))
        .map(|entry| {
            let format = format_of(root, entry);
            let pages = pages_of(root, entry, format);
            (entry.rel_path.clone(), Resolved { format, pages })
        })
        .collect();

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the scan transaction: {e}"))?;
    let mut report = ScanReport::for_root(&root.path);
    report.unreadable_dirs = unreadable_dirs;
    for entry in &seen {
        record(
            &tx,
            root,
            entry,
            held.get(&entry.rel_path).copied(),
            resolved.get(&entry.rel_path),
            &mut report,
        )?;
    }
    report.gone = forget_unseen(&tx, root.id, &seen, &held)?;
    tx.commit()
        .map_err(|e| format!("committing the scan of {}: {e}", root.path.display()))?;
    Ok(report)
}

/// One thing the walk found, before anything has been decided about it.
struct Seen {
    rel_path: String,
    kind: Kind,
    size: u64,
    mtime: i64,
    /// Recursive page total for a directory; `None` for a file, whose count
    /// costs an archive open and is paid in `scan_root`, before the
    /// transaction opens.
    pages: Option<u32>,
    /// Directories only: how many entries `read_dir` returned, for
    /// `dir_state`. See the module doc for what this does and does not
    /// license skipping.
    entry_count: Option<i64>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    File,
    Dir,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Dir => "dir",
        }
    }
}

/// Whether `held` already matches `seen`'s current `(size, mtime)`.
///
/// The one test that decides two things that must never disagree: what
/// `scan_root` resolves before the transaction opens, and which branch
/// `record` takes once it has. A single function computing it, rather than
/// the same comparison written out twice, is what keeps those two decisions
/// from drifting apart.
fn unchanged(held: Option<(i64, i64)>, seen: &Seen) -> bool {
    let size = i64::try_from(seen.size).unwrap_or(i64::MAX);
    held == Some((size, seen.mtime))
}

/// Write one entry, using facts `scan_root` already resolved rather than
/// opening anything itself — see [`Resolved`] for why that split exists.
///
/// **`(size, mtime)` is the whole cheap-walk saving.** `sniff` — an open,
/// plus a full central-directory parse for anything carrying a zip header —
/// and `member_pages`, another archive open, are the expensive calls this
/// saves, and neither happens here: `scan_root` pays them only for entries
/// whose size and mtime moved. An entry that did not move keeps its stored
/// `sha256`, and a file its stored `pages`, and is otherwise touched only to
/// move `last_seen` forward. A directory's total is rewritten either way —
/// see the branch below — because a directory's mtime does not move when
/// something changes beneath it.
///
/// **`resolved` is `Some` exactly when `held` says this entry is new or has
/// moved**, which is also exactly when a branch below needs a format or a
/// page count to write. The match is shaped on that pairing directly, rather
/// than on a separately recomputed flag that could disagree with it — the
/// fourth combination, a new entry with nothing resolved, is a bug in the
/// caller rather than a case this function silently absorbs.
///
/// `sha256` stays NULL either way — §3 makes hashing lazy, and `derive.rs` is
/// what fills it in when a member is about to become part of a work.
fn record(
    conn: &Connection,
    root: &Root,
    seen: &Seen,
    held: Option<(i64, i64)>,
    resolved: Option<&Resolved>,
    report: &mut ScanReport,
) -> Result<(), String> {
    let size = i64::try_from(seen.size).unwrap_or(i64::MAX);

    match (held.is_some(), resolved) {
        (true, None) => {
            // `last_seen`, and for a directory its recursive page total.
            //
            // **A directory's total is written even when the directory itself has
            // not moved**, because a directory's mtime moves only when an entry is
            // added, removed or renamed *in it*. Measured on this filesystem on
            // 2026-08-17: adding a page to `series/vol01/` moved `vol01` from 60 to
            // 80 bytes with a new mtime and left `series` and the root
            // byte-identical. Without this, every ancestor of a changed directory
            // keeps its pre-change total forever, and `derive.rs` reads exactly
            // those totals to pick the deepest directory that is a member.
            //
            // It costs nothing: the walk summed it on the way past, so no container
            // is opened. `COALESCE` leaves a file's stored count alone.
            conn.execute(
                "UPDATE scan_entry SET last_seen = datetime('now'), pages = COALESCE(?3, pages)
                  WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![root.id, seen.rel_path, dir_pages(seen)],
            )
            .map_err(|e| format!("touching {}: {e}", seen.rel_path))?;
        }
        (true, Some(resolved)) => {
            report.updated += 1;
            conn.execute(
                "UPDATE scan_entry
                    SET kind = ?3, format = ?4, size = ?5, mtime = ?6, pages = ?7,
                        sha256 = NULL, last_seen = datetime('now')
                  WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![
                    root.id,
                    seen.rel_path,
                    seen.kind.as_str(),
                    resolved.format,
                    size,
                    seen.mtime,
                    resolved.pages,
                ],
            )
            .map_err(|e| format!("updating {}: {e}", seen.rel_path))?;
        }
        (false, Some(resolved)) => {
            report.added += 1;
            conn.execute(
                "INSERT INTO scan_entry
                     (root_id, rel_path, kind, format, size, mtime, pages)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    root.id,
                    seen.rel_path,
                    seen.kind.as_str(),
                    resolved.format,
                    size,
                    seen.mtime,
                    resolved.pages,
                ],
            )
            .map_err(|e| format!("recording {}: {e}", seen.rel_path))?;
        }
        (false, None) => {
            return Err(format!(
                "{}: classified as new but arrived with no resolved facts — an internal \
                 inconsistency between scan_root's classification and record's",
                seen.rel_path
            ));
        }
    }

    match seen.kind {
        Kind::Dir => report.dirs += 1,
        Kind::File => report.files += 1,
    }
    if let (Kind::Dir, Some(count)) = (seen.kind, seen.entry_count) {
        conn.execute(
            "INSERT INTO dir_state (root_id, rel_path, mtime, entry_count, last_walked)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(root_id, rel_path) DO UPDATE SET
                 mtime = excluded.mtime,
                 entry_count = excluded.entry_count,
                 last_walked = excluded.last_walked",
            rusqlite::params![root.id, seen.rel_path, seen.mtime, count],
        )
        .map_err(|e| format!("recording the state of {}: {e}", seen.rel_path))?;
    }
    Ok(())
}

/// What this entry's bytes say it is.
///
/// **Called only when an entry is new or has moved**, which is what makes
/// this module's cheap-walk claim true rather than aspirational. `sniff`
/// opens the file, and for anything carrying a zip header opens it a second
/// time and parses the entire central directory — so calling it from the
/// walk, on every file, on every scan, was the expensive thing this module
/// says it avoids. It did exactly that until 2026-08-17.
///
/// A file whose bytes cannot be read is `None` rather than an error, for the
/// same reason [`pages_of`] swallows a refusal: one unreadable file must not
/// leave a whole library unscanned.
fn format_of(root: &Root, seen: &Seen) -> Option<&'static str> {
    match seen.kind {
        Kind::Dir => taiyaku_read::Format::Dir.token(),
        Kind::File => taiyaku_read::sniff(&root.path.join(&seen.rel_path))
            .ok()
            .and_then(taiyaku_read::Format::token),
    }
}

/// A page count for an entry that is new or has moved, or `None` where one
/// does not apply.
///
/// A directory's count comes from the walk, which summed it on the way past.
/// A file's costs an archive open, so it is only paid for a format this
/// project can actually read — a PDF or an EPUB is recorded with its format
/// and no count.
///
/// **A container that will not open is not an error.** `member_pages` refuses
/// a truncated zip, and a scan that aborted on one would leave a whole
/// library unscanned because of a single bad download. The count stays `None`
/// and the row still records what the bytes said the file was.
///
/// Takes the token [`format_of`] just produced rather than re-sniffing, and
/// compares against `Format::Zip.token()` rather than a `"zip"` literal —
/// that vocabulary is the schema's, and a second copy of it here is what
/// `Format::token`'s own doc warns against.
fn pages_of(root: &Root, seen: &Seen, format: Option<&'static str>) -> Option<u32> {
    match seen.kind {
        Kind::Dir => seen.pages,
        Kind::File if format == taiyaku_read::Format::Zip.token() => {
            taiyaku_read::member_pages(&root.path.join(&seen.rel_path)).ok()
        }
        Kind::File => None,
    }
}

/// A directory's recursive total, or `None` for a file.
///
/// Split out because the unchanged branch needs it and must not pay for a
/// file's count to get it.
fn dir_pages(seen: &Seen) -> Option<u32> {
    match seen.kind {
        Kind::Dir => seen.pages,
        Kind::File => None,
    }
}

/// What a new-or-moved entry's bytes turned out to say, resolved before the
/// write transaction opens.
///
/// **The whole point of this type is where it is built.** Sniffing and
/// counting pages are filesystem operations, and doing them inside `tx` held
/// the write lock across every archive open in a library scan — five seconds
/// is [`crate::BUSY_TIMEOUT`], and a first scan of 156 series on network
/// storage passes that without difficulty, while `taiyaku serve` is writing a
/// `progress` row on every page turn.
struct Resolved {
    format: Option<&'static str>,
    pages: Option<u32>,
}

/// The `(size, mtime)` this root already holds, by `rel_path`.
///
/// One query rather than one per entry, and outside the transaction, so the
/// decision about what needs opening is made before any lock is taken. That
/// also makes it a snapshot: a concurrent writer could change a row between
/// this read and `record`'s, and `record` would branch on the stale value it
/// was handed. No caller does that today — this crate's only caller is its
/// own tests — so nothing closes the window yet; whoever adds a second
/// writer needs to.
fn held_for(conn: &Connection, root_id: i64) -> Result<HashMap<String, (i64, i64)>, String> {
    let mut stmt = conn
        .prepare("SELECT rel_path, size, mtime FROM scan_entry WHERE root_id = ?1")
        .map_err(|e| format!("reading what this root held: {e}"))?;
    let rows = stmt
        .query_map([root_id], |r| {
            Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?)))
        })
        .map_err(|e| format!("reading what this root held: {e}"))?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Drop rows for this root that the walk did not meet, and say how many.
///
/// Reached only after the §8.1 refusal above, so "the walk met nothing" can
/// never arrive here. Iterates `held` — `held_for`'s result, already read
/// once before the transaction opened — rather than issuing a second
/// `SELECT` for the same rows.
fn forget_unseen(
    conn: &Connection,
    root_id: i64,
    seen: &[Seen],
    held: &HashMap<String, (i64, i64)>,
) -> Result<usize, String> {
    let kept: HashSet<&str> = seen.iter().map(|entry| entry.rel_path.as_str()).collect();
    let mut gone = 0;
    let stale: Vec<&str> = held
        .keys()
        .map(String::as_str)
        .filter(|held| !kept.contains(held))
        .collect();
    for rel_path in stale {
        conn.execute(
            "DELETE FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root_id, rel_path],
        )
        .map_err(|e| format!("forgetting {rel_path}: {e}"))?;
        conn.execute(
            "DELETE FROM dir_state WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root_id, rel_path],
        )
        .map_err(|e| format!("forgetting the state of {rel_path}: {e}"))?;
        gone += 1;
    }
    Ok(gone)
}

// --- measure: the tree, gathered with no side effects -----------------------

/// Recursion ceiling for `measure`, far past any real library root — a volume
/// split into chapter subdirectories, the deepest nesting `docs/sidecar.md`
/// records, is two levels — and there purely to turn a symlink cycle into a
/// named error rather than a stack overflow.
///
/// `path.is_dir()` follows symlinks, so a link back to an ancestor recurses
/// forever. The precise fix is a cycle guard: canonicalize each directory and
/// refuse to revisit one already on the path down. That costs a syscall per
/// directory, which is not worth paying on every scan to guard against
/// something this rare — a depth limit catches the same failure for free, at
/// the cost of naming it one call late.
///
/// **Measured, not guessed, and lower than "far above a real tree" alone would
/// suggest.** A self-referential symlink does not actually recurse forever on
/// this platform: Linux caps symlink resolution at 40 hops per lookup
/// (`ELOOP`), and `Path::is_dir()` swallows that error and answers `false`
/// rather than propagating it — so past roughly 40 levels the walk does not
/// crash, it silently treats the cyclic subtree as childless and returns
/// `Ok`, which is the "silent truncation" this limit exists to rule out just
/// as much as the stack overflow. The ceiling has to fire before that
/// happens, not after, so it sits at 32 — comfortably past any real library
/// root and comfortably short of where the OS starts lying about `is_dir`.
const MAX_DEPTH: u32 = 32;

/// One entry's facts, gathered before anything is recorded, plus its
/// children's — the shape `flatten` turns into the `Vec<Seen>` a scan writes.
///
/// Carries exactly what [`Seen`] carries, with `children` alongside so a
/// directory's recursive page total can be summed on the way back up before
/// any row is written. See the module doc for why that has to be a separate
/// pass from recording.
struct Measured {
    rel_path: String,
    kind: Kind,
    size: u64,
    mtime: i64,
    pages: Option<u32>,
    entry_count: Option<i64>,
    children: Vec<Measured>,
}

/// Why one call to [`measure_dir`] did not produce a [`Measured`].
///
/// Two different failures, and they are not treated alike — see
/// [`measure_dir`]'s own doc comment for which is which and why.
enum MeasureError {
    /// `read_dir` or `metadata` refused this directory. Caught by the caller
    /// for any directory below the root and turned into one more line in
    /// [`ScanReport::unreadable_dirs`]; propagated as a hard `Err` only when
    /// it is the root's own directory that refused, since there is nothing
    /// above the root to skip to and continue the walk from.
    Unreadable(String),
    /// [`MAX_DEPTH`] exceeded. Always propagated, never caught and skipped —
    /// see that constant's own doc comment for why silently truncating a
    /// symlink cycle is exactly the failure this must not become.
    TooDeep(String),
}

impl MeasureError {
    fn into_message(self) -> String {
        match self {
            Self::Unreadable(message) | Self::TooDeep(message) => message,
        }
    }
}

/// Measure `path` and everything beneath it, recursively, and every directory
/// beneath it — but not `path` itself — that could not be listed.
///
/// `inventory::totals_at`'s shape: a directory's own page images are tallied
/// alongside its children's recursive totals, and dotfiles are skipped
/// exactly as `inventory` skips them — which is also what keeps `watch.rs`'s
/// probe file out of the facts. Unlike `inventory`, every non-page-image
/// entry becomes a node of its own here, since a fact-layer walk has to
/// record files `inventory` never looked at twice.
///
/// # Errors
///
/// Returns an error if `path` itself cannot be listed, or if a symlink cycle
/// nests past [`MAX_DEPTH`] directories anywhere beneath it — see
/// [`MeasureError`] for why the second one is not caught and skipped the way
/// the first is when it happens below the top.
fn measure(path: &Path, depth: u32) -> Result<(Measured, Vec<String>), String> {
    let mut unreadable = Vec::new();
    let measured = measure_dir(path, String::new(), depth, &mut unreadable)
        .map_err(MeasureError::into_message)?;
    Ok((measured, unreadable))
}

/// One directory, measured — see [`measure`]. `unreadable` collects a message
/// for every descendant directory this call meets and cannot list; `path`
/// itself refusing to list is returned as an `Err` instead, which is what
/// lets the caller tell "this one directory is unreadable" from "the
/// directory I was just asked to measure is" — see [`MeasureError`].
fn measure_dir(
    path: &Path,
    rel_path: String,
    depth: u32,
    unreadable: &mut Vec<String>,
) -> Result<Measured, MeasureError> {
    if depth > MAX_DEPTH {
        return Err(MeasureError::TooDeep(format!(
            "{}: nested past {MAX_DEPTH} directories deep; a symlink cycle is the likely cause",
            path.display()
        )));
    }
    // Stat before listing. A file landing in the directory mid-walk can be
    // missed by `read_dir` while `metadata` (below) still picks up the
    // resulting mtime move — stat first, and that race fails safe: the
    // recorded mtime already reflects the change `read_dir` did not see, so
    // the next scan does not mistake this directory for unchanged.
    let meta = std::fs::metadata(path)
        .map_err(|e| MeasureError::Unreadable(format!("{}: {e}", path.display())))?;
    let entries = std::fs::read_dir(path)
        .map_err(|e| MeasureError::Unreadable(format!("{}: {e}", path.display())))?;

    let mut here = 0u32;
    let mut entry_count = 0i64;
    let mut dir_entries = Vec::new();
    let mut file_entries = Vec::new();
    for entry in entries.flatten() {
        let child_path = entry.path();
        let name = child_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.starts_with('.') {
            continue;
        }
        entry_count += 1;
        if child_path.is_dir() {
            dir_entries.push((name, child_path));
        } else if taiyaku_read::is_page_name(&name) {
            here += 1;
        } else {
            file_entries.push((name, child_path));
        }
    }
    dir_entries.sort();
    file_entries.sort();

    let mut pages = here;
    let mut children = Vec::new();
    for (name, child_path) in dir_entries {
        match measure_dir(
            &child_path,
            join_rel(&rel_path, &name),
            depth + 1,
            unreadable,
        ) {
            Ok(child) => {
                if let Some(child_pages) = child.pages {
                    pages += child_pages;
                }
                children.push(child);
            }
            // Skipped, not propagated: this directory cannot speak for
            // whatever is unreadable beneath it, but it can still speak for
            // everything else it holds. See `ScanReport::unreadable_dirs`.
            Err(MeasureError::Unreadable(message)) => unreadable.push(message),
            // A cycle's depth is a property of the whole path down to it, not
            // of this one directory — caught here it would look like an
            // ordinary permission refusal and be silently skipped, which is
            // exactly the silent truncation `MAX_DEPTH`'s own doc comment
            // says this must not become.
            Err(err @ MeasureError::TooDeep(_)) => return Err(err),
        }
    }
    for (name, child_path) in file_entries {
        if let Some(file) = measure_file(&child_path, join_rel(&rel_path, &name)) {
            children.push(file);
        }
    }

    Ok(Measured {
        rel_path,
        kind: Kind::Dir,
        size: meta.len(),
        mtime: mtime_secs(&meta),
        pages: Some(pages),
        entry_count: Some(entry_count),
        children,
    })
}

/// One file's facts, or `None` if it is not a regular file, or cannot be
/// stat'ed at all.
///
/// **A path that cannot be stat'ed is skipped, not an error.** A dangling
/// symlink, or a file removed between `read_dir` and this call, would
/// otherwise abort the scan of an entire library — the same failure
/// [`pages_of`] already refuses to cause for a truncated archive. An `Err`
/// from this module is reserved for a *directory* that cannot be listed,
/// which is a fact about the walk rather than about one file.
///
/// **A FIFO, a socket or a device node is skipped too**, and for a sharper
/// reason than a directory listing being merely wrong about one: `stat`
/// succeeds on all three, so without a check here the walk would record one
/// and hand it to `sniff`, whose `File::open` on a FIFO blocks until a
/// writer appears — measured, `timeout 2 cat` on one exits 124.
///
/// **A file with no read permission is not handled here.** `stat` needs only
/// execute permission on the containing directory, so it succeeds where a
/// later `File::open` would not; that refusal surfaces downstream, in
/// [`format_of`]'s `.ok()`.
///
/// It no longer sniffs. See [`format_of`].
fn measure_file(path: &Path, rel_path: String) -> Option<Measured> {
    let meta = std::fs::metadata(path).ok()?;
    // **Regular files and directories only.** `stat` succeeds on a FIFO, a
    // socket and a device node, so the walk would record them — and then
    // `sniff` would `File::open` a FIFO and block until a writer appeared,
    // which measured as an indefinite hang. Nothing here can read one anyway.
    if !meta.is_file() {
        return None;
    }
    Some(Measured {
        rel_path,
        kind: Kind::File,
        size: meta.len(),
        mtime: mtime_secs(&meta),
        pages: None,
        entry_count: None,
        children: Vec::new(),
    })
}

/// `parent/name`, or just `name` when `parent` is the root's own `""`.
fn join_rel(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// A file's modification time as whole seconds since the epoch, or `0` for a
/// filesystem that cannot report one — better than failing a whole scan over
/// a `stat` field nothing here treats as more than a change signal.
fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// Walk a measured tree into the flat, root-first order [`scan_root`]'s
/// recording loop consumes.
fn flatten(measured: &Measured) -> Vec<Seen> {
    let mut seen = Vec::new();
    flatten_into(measured, &mut seen);
    seen
}

fn flatten_into(measured: &Measured, seen: &mut Vec<Seen>) {
    seen.push(Seen {
        rel_path: measured.rel_path.clone(),
        kind: measured.kind,
        size: measured.size,
        mtime: measured.mtime,
        pages: measured.pages,
        entry_count: measured.entry_count,
    });
    for child in &measured.children {
        flatten_into(child, seen);
    }
}

// `pub(crate)` rather than private, for `derive/tests.rs`: it derives from
// *this* module's fixture rather than a second tree describing the same
// corpus, since two fixtures drift and the whole point of those tests is that
// both modules read one set of facts.
#[cfg(test)]
pub(crate) mod tests;
