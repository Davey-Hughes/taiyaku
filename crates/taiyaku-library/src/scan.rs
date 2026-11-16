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
//! **Each file is scanned once, under the deepest directory that holds it**
//! (overlapping-libraries spec §4). A directory of another library may sit
//! inside this one's. The walk does not descend into it (`measure_dir`), and
//! rows this directory already held under it are neither counted gone nor
//! counted toward §8.1's refusal: they wait for the nested directory's scan,
//! which takes them over, with the members under them (`take_over`), and
//! counts them toward its own §8.1 refusal: all but the rows its walk does
//! not reach and the one around it does, which that one holds (decision 21).
//! A symlink to it is skipped too, but rows recorded through one are
//! forgotten, since no scan takes them.
//!
//! **This module writes nothing to the library tree but the hand documents it
//! moves.** §10. When a directory takes a work's files over or hands them
//! back, or a removal deletes or re-points a member of a work that survives
//! it, the work's document under `.taiyaku` follows its address, into a root
//! that keeps documents (`carry_documents`). What the carry cannot move out of
//! a removed directory, the removal reports as an orphan. Otherwise the one
//! file the scanner ever creates is `watch.rs`'s probe, which is not this.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension as _};

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
    /// Patterns a scan is meant to skip, as one stored string.
    ///
    /// Compiled once per scan into an [`Excluder`] and applied by
    /// `scan_root_inner`'s walk — O92. A pattern this cannot compile is
    /// dropped and named in [`ScanReport::bad_exclude_patterns`] rather than
    /// failing the scan; see [`Excluder::build`].
    pub exclude_globs: Option<String>,
    pub scan_interval_secs: Option<i64>,
    /// Whether this root keeps `<root>/.taiyaku/<content_key>.library.json`.
    ///
    /// **Off unless asked** — migration 012's own comment carries the
    /// reasoning. Read here rather than queried per work because `reconcile`
    /// already holds a `Root` and `hand::save` is the only caller that does
    /// not.
    pub keeps_documents: bool,
    /// The libraries this directory is in, ascending: its `library_root`
    /// links (migration 19). Every writer leaves a directory at least one;
    /// see [`crate::libraries`].
    pub libraries: Vec<i64>,
}

/// Every scan root this library knows about, oldest first, once each.
///
/// **The links are a second query, never a join**: a join would answer a
/// directory in two libraries twice, and every caller that walks or derives
/// each root it is given would then do it twice. **Both read one snapshot**
/// ([`crate::one_snapshot`]): a directory removed between them would come
/// back in no library.
///
/// # Errors
///
/// Returns an error if the query against `root` or `library_root` fails.
pub fn roots(conn: &Connection) -> Result<Vec<Root>, String> {
    let _snapshot =
        crate::one_snapshot(conn).map_err(|e| format!("starting a read of the roots: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, path, edition_key, default_kind, default_language, exclude_globs, \
             scan_interval_secs, keeps_documents \
             FROM root ORDER BY id",
        )
        .map_err(|e| format!("listing roots: {e}"))?;
    let mut roots = stmt
        .query_map([], |r| {
            Ok(Root {
                id: r.get(0)?,
                path: PathBuf::from(r.get::<_, String>(1)?),
                edition_key: r.get(2)?,
                default_kind: r.get(3)?,
                default_language: r.get(4)?,
                exclude_globs: r.get(5)?,
                scan_interval_secs: r.get(6)?,
                keeps_documents: r.get(7)?,
                libraries: Vec::new(),
            })
        })
        .map_err(|e| format!("listing roots: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing roots: {e}"))?;
    let mut links = conn
        .prepare("SELECT root_id, library_id FROM library_root ORDER BY root_id, library_id")
        .map_err(|e| format!("listing the libraries of each root: {e}"))?;
    let links = links
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| format!("listing the libraries of each root: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing the libraries of each root: {e}"))?;
    for (root_id, library_id) in links {
        if let Ok(at) = roots.binary_search_by_key(&root_id, |root| root.id) {
            roots[at].libraries.push(library_id);
        }
    }
    Ok(roots)
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
/// root for, and a directory whose library of its own cannot be named because
/// both names [`InLibrary::Own`] tries are taken ([`add_root_checked`]
/// refuses that one first, saying so).
pub fn add_root(conn: &Connection, path: &Path) -> Result<Root, String> {
    add_root_in(conn, path, InLibrary::Own)
}

/// Which library a directory joins when it is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InLibrary<'a> {
    /// A library of its own, named for the directory ([`own_library_name`]):
    /// its last path component, or its whole path when a library already has
    /// that name in any ASCII case, stripped of white space either way.
    Own,
    /// The library with this name, created if there is none.
    Named(&'a str),
    /// The library with this id. [`add_root_checked_with`] refuses an id no
    /// library holds; [`add_root_in`] does not ask whether one does.
    Id(i64),
}

/// [`add_root`], filing the directory under `library`. The row is Automatic.
///
/// **A named library that does not exist yet is created here, in the same
/// transaction as the directory**, so a refused insert leaves no empty library
/// behind. See [`crate::libraries`] for why an empty one must not exist.
///
/// # Errors
///
/// As [`add_root`], and if `library` names nothing once trimmed.
pub fn add_root_in(conn: &Connection, path: &Path, library: InLibrary<'_>) -> Result<Root, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    insert_root(conn, &canonical, library, None)
}

/// [`add_root_in`] for a path already canonical, so crate-only: the directory,
/// a library made for it, and their link, in one transaction, since the store
/// cannot refuse a directory committed with no library (migration 19).
///
/// **`scan_interval_secs` is written by the `INSERT` itself** (O275): an add
/// and then a write of its schedule could lose to a scheduler tick between
/// them. It is written as handed: the server checks its range, as it does for
/// [`update_root`], which checks nothing either.
pub(crate) fn insert_root(
    conn: &Connection,
    canonical: &Path,
    library: InLibrary<'_>,
    scan_interval_secs: Option<i64>,
) -> Result<Root, String> {
    let tx = crate::Nested::open(conn, "add_root")
        .map_err(|e| format!("starting to add root {}: {e}", canonical.display()))?;
    let chosen = match library {
        InLibrary::Own => None,
        InLibrary::Named(name) => Some(match crate::libraries::id_of(&tx, name)? {
            Some(id) => id,
            None => crate::libraries::create(&tx, name)?,
        }),
        InLibrary::Id(id) => Some(id),
    };
    tx.execute(
        "INSERT INTO root (path, scan_interval_secs) VALUES (?1, ?2)",
        rusqlite::params![canonical.to_string_lossy(), scan_interval_secs],
    )
    .map_err(|e| format!("adding root {}: {e}", canonical.display()))?;
    let id = tx.last_insert_rowid();
    // After the insert, so a path already held is refused as one first.
    let library_id = match chosen {
        Some(id) => id,
        None => crate::libraries::create(&tx, &own_library_name(&tx, canonical)??)?,
    };
    tx.execute(
        "INSERT INTO library_root (library_id, root_id) VALUES (?1, ?2)",
        [library_id, id],
    )
    .map_err(|e| {
        format!(
            "filing root {} under library {library_id}: {e}",
            canonical.display()
        )
    })?;
    // Read back rather than hard-coded, so the column's default — migration
    // 012's `DEFAULT 0` — stays the one place that decides it.
    let keeps_documents = tx
        .query_row(
            "SELECT keeps_documents FROM root WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("reading root {}: {e}", canonical.display()))?;
    tx.commit()
        .map_err(|e| format!("committing root {}: {e}", canonical.display()))?;
    Ok(Root {
        id,
        path: canonical.to_path_buf(),
        edition_key: None,
        default_kind: None,
        default_language: None,
        exclude_globs: None,
        scan_interval_secs,
        keeps_documents,
        libraries: vec![library_id],
    })
}

/// File root `root_id`, a row planted by SQL with no library, under a library
/// of its own named as [`InLibrary::Own`] names one, and answer that library's
/// id: for a fixture whose directories need not exist, so cannot be added.
///
/// # Errors
///
/// Returns an error if there is no root `root_id`, if no library of its own
/// can be named for it, or if a write fails. Nothing is written then.
#[cfg(any(test, feature = "test-support"))]
pub fn file_under_its_own_library(conn: &Connection, root_id: i64) -> Result<i64, String> {
    // One transaction: a failed link would otherwise leave the library just
    // made with no directory, and `crate::libraries` says none stands empty.
    let tx = crate::Nested::open(conn, "file_under_its_own_library")
        .map_err(|e| format!("starting to file root {root_id}: {e}"))?;
    let path: String = tx
        .query_row("SELECT path FROM root WHERE id = ?1", [root_id], |r| {
            r.get(0)
        })
        .map_err(|e| format!("reading root {root_id}: {e}"))?;
    let library_id = crate::libraries::create(&tx, &own_library_name(&tx, Path::new(&path))??)?;
    tx.execute(
        "INSERT INTO library_root (library_id, root_id) VALUES (?1, ?2)",
        [library_id, root_id],
    )
    .map_err(|e| format!("filing root {root_id} under library {library_id}: {e}"))?;
    tx.commit()
        .map_err(|e| format!("committing root {root_id}'s library: {e}"))?;
    Ok(library_id)
}

/// Why a directory at `canonical` cannot be a library of its own, if it
/// cannot: [`own_library_name`]'s refusal, which [`add_root_checked_with`] asks
/// before anything is written, so it is this caller's to fix rather than a
/// store fault from inside the add.
fn own_library_clash(conn: &Connection, canonical: &Path) -> Result<Option<String>, String> {
    Ok(own_library_name(conn, canonical)?.err())
}

/// The name [`InLibrary::Own`] gives a directory at `canonical`'s library, or
/// why it can give none (the inner `Err`).
///
/// **The directory's last path component, or its whole path when the
/// component is empty or already a library's name.** When the whole path is a
/// library's name too, there is no third name to try, and [`InLibrary::Own`]
/// promises none, so this refuses and says which names are taken. The last
/// component is what follows the path's last `/`. Both names are stripped of
/// white space by [`crate::libraries::trim_name`], and asked in any ASCII
/// case, as [`crate::libraries::id_of`] finds a name. A component is empty for
/// `/` and for a directory named only white space.
///
/// **A component that strips to `.` or `..` is refused too**, as a directory
/// called `. ` or ` ..` would otherwise make a library of that name, which no
/// page address holds ([`crate::has_no_address`]). Refused rather than given
/// its whole path, as any component that is not empty and not taken names the
/// library.
///
/// # Errors
///
/// The outer `Err` is a store fault while looking a name up.
fn own_library_name(conn: &Connection, canonical: &Path) -> Result<Result<String, String>, String> {
    let taken = |name: &str| -> Result<bool, String> {
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM library WHERE name = ?1 COLLATE NOCASE)",
            [name],
            |r| r.get(0),
        )
        .map_err(|e| format!("looking up the library `{name}`: {e}"))
    };
    let path = canonical.to_string_lossy();
    let whole = crate::libraries::trim_name(&path);
    let leaf = crate::libraries::trim_name(path.rsplit('/').next().unwrap_or_default());
    if crate::has_no_address(leaf) {
        return Ok(Err(format!(
            "{path} cannot be a library of its own: a library named for it would be called \
             `{leaf}`, and no address can hold “.” or “..” as a name; add it to a library by name"
        )));
    }
    if !leaf.is_empty() && !taken(leaf)? {
        return Ok(Ok(leaf.to_string()));
    }
    if !taken(whole)? {
        return Ok(Ok(whole.to_string()));
    }
    Ok(Err(if leaf.is_empty() {
        format!(
            "{path} cannot be a library of its own: a library is already called `{whole}`; \
             add it to a library by name"
        )
    } else {
        format!(
            "{path} cannot be a library of its own: libraries are already called `{leaf}` \
             and `{whole}`; add it to a library by name"
        )
    }))
}

/// How long [`add_root_checked`] waits for its own watch probe's event to
/// arrive before deciding a root cannot be watched.
///
/// The CLI's `scan` command (`taiyaku-cli/src/library.rs`) defines its own
/// `PROBE_TIMEOUT` at this same two seconds, for this same reason —
/// `watch::report_line`'s own doc comment states the trade this figure
/// makes once, and there is no second reasoning to give here. A library
/// crate cannot depend on its CLI to reuse that constant (the dependency
/// runs the other way), so this restates the value rather than inventing a
/// different one; the two are meant to drift together; if the CLI's figure
/// ever changes, this one should change with it.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// What [`add_root_checked_with`] or [`create_library`] built: the row it
/// inserted, or the row another library holds, now linked; the watch
/// experiment run against a new row immediately, and the interval that
/// experiment implies, which is reported and not stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub root: Root,
    /// `None` for a link: the probe writes a file into the directory, and the
    /// directory was probed when it was first added.
    pub probe: Option<crate::watch::Probe>,
    /// The interval [`Added::probe`] implies, `None` where it did not run.
    /// Reported, not written: the row stores the schedule the add asked for.
    pub interval: Option<Duration>,
    /// Whether a link kept a schedule other than the one the add asked for.
    /// The row, and so its schedule, is shared by every library holding it,
    /// so a link does not write the one asked, as that would start or stop
    /// another library's scans (O275); the answer says so instead. Always
    /// `false` for a new directory, which stores what was asked.
    pub schedule_kept: bool,
    /// The names of the other libraries that already held the directory,
    /// ascending by id, read inside the add's transaction while they stand:
    /// empty for a directory new to the store.
    pub also_in: Vec<String>,
    /// The directory row's `last_scan_at`, read inside the add's transaction,
    /// as [`Added::also_in`] is, so an answer drawn after the commit needs no
    /// row that a removal since may have deleted: `None` for a directory new
    /// to the store, which has never been scanned.
    pub last_scan_at: Option<String>,
    /// The row's `last_scan_status`, read with [`Added::last_scan_at`].
    pub last_scan_status: Option<String>,
}

/// Why [`add_root_checked`] could not add a root.
///
/// **Two shapes, not the bare `String` this module's other fallible
/// functions return**, so a caller can tell which happened rather than
/// re-parsing a message meant for a person. Every other checked write this
/// codebase exposes over HTTP already keeps that split at the handler —
/// `taiyaku-serve`'s `create_rule`/`patch_rule` pre-check their own
/// caller-fault cases before ever writing, and reserve a 500 for the store
/// call itself. `add_root_checked`'s own checks run *inside* this function,
/// not in the handler that calls it, so a caller has nothing of its own to
/// pre-check against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddRootError {
    /// `path` itself: it does not exist, this process cannot read it, or
    /// [`vetted`] refuses it against the roots held. The message names which,
    /// and for a clash, the colliding directory and its library by name. Or
    /// the library it would join cannot be named or linked: an
    /// [`InLibrary::Named`] that [`crate::libraries::usable_name`] refuses, an
    /// [`InLibrary::Own`] whose two names are both taken or whose directory
    /// another library holds, or an [`InLibrary::Id`] no library holds.
    /// For [`create_library`]: no directory given, a name `usable_name`
    /// refuses or that is already a library's, or two of the directories
    /// given overlapping.
    Unusable(String),
    /// The store itself failed: opening or committing the transaction, [`vetted`]'s
    /// own reads, the library-name lookup, or the insert of a directory or
    /// of a link.
    Failed(String),
}

impl std::fmt::Display for AddRootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unusable(why) | Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for AddRootError {}

/// [`add_root_checked_in`], with the directory a library of its own.
///
/// # Errors
///
/// As [`add_root_checked_in`].
pub fn add_root_checked(conn: &Connection, path: &Path) -> Result<Added, AddRootError> {
    add_root_checked_in(conn, path, InLibrary::Own)
}

/// [`add_root_checked_with`], asking for Automatic: a new directory's row
/// stores NULL.
///
/// # Errors
///
/// As [`add_root_checked_with`].
pub fn add_root_checked_in(
    conn: &Connection,
    path: &Path,
    library: InLibrary<'_>,
) -> Result<Added, AddRootError> {
    add_root_checked_with(conn, path, library, None)
}

/// Add a new scan root at `path`, filed under `library` and scheduled as
/// `scan_interval_secs` asks, having run C4 §2's three validations that
/// [`add_root`] itself does not — "because the failures are silent otherwise."
///
/// 1. **Readable, by this process, not merely present.** `path.canonicalize`
///    answers "does it exist" and nothing else: it succeeds against a
///    directory that exists but that this process cannot open. So once
///    canonicalised, this calls [`std::fs::read_dir`] against the canonical
///    path and reports its refusal as a distinct failure, naming what it
///    was — "readable by the server process" is a different question from
///    "exists", and on a containerised deployment it is exactly the
///    question that differs.
/// 2. **No clash with the roots held, compared inside the insert's own
///    transaction** (O204), so a directory another writer adds meanwhile is
///    compared too ([`vetted`]). Both sides are canonical (this function's
///    candidate, and every stored [`Root::path`], which [`add_root`]
///    canonicalises), so a symlink or a `..` cannot slip a nested path past
///    the comparison, and [`Path::starts_with`] compares whole components, so
///    `/srv/manga2` is not inside `/srv/manga`. **A directory another library
///    holds is linked** (overlapping-libraries spec §5): one `library_root`
///    row, its shared settings, schedule included, as they stand.
/// 3. **The watcher probe, run now rather than left for the first scan**, for
///    a new directory only, as the probe writes a file into the directory.
///    [`crate::watch::probe`] answers whether this root can be watched at
///    all — spec B §8's experiment. The interval
///    [`crate::watch::interval_for`] derives from that answer is reported in
///    [`Added::interval`], not written: the row stores `scan_interval_secs`
///    as asked. An Off directory is probed too, as a probe is not a scan.
///
/// **And the library it joins can be named**, checked in the same
/// transaction, before anything is written: an [`InLibrary::Named`] needs a name
/// [`crate::libraries::usable_name`] takes, and an [`InLibrary::Own`] needs a
/// name [`own_library_name`] can give it. Otherwise each would surface as a
/// store fault from inside the add, though both are this caller's to fix.
///
/// **The library is created only once every validation has passed**, with
/// the directory or its link, so a refused path names no library into being.
///
/// [`add_root`] and [`add_root_in`] validate nothing: a great many
/// fixtures call them directly, and a validation that runs inside every
/// fixture is a validation nobody can test the absence of. This is the
/// checked path a person takes when adding a root they have not already
/// vetted by hand — through `taiyaku library scan --add`, or the HTTP API on
/// their behalf.
///
/// # Errors
///
/// Returns [`AddRootError::Unusable`] if `path` cannot be canonicalised (it
/// must exist), if the canonical path cannot be read as a directory, if
/// [`vetted`] refuses it, or if the library it would join cannot be named (an
/// [`InLibrary::Id`] no library holds is one) — every one of those is this
/// caller's own mistake, an `error` a person can act on. A path the library
/// already holds is refused as one, `<path> is already a directory of the
/// library `<name>``, and not as inside itself, which is what the nesting
/// check alone would say of an equal path; so is a path another library
/// holds, sent as [`InLibrary::Own`], naming every library that holds it.
///
/// Returns [`AddRootError::Failed`] if the store itself refuses along the
/// way — opening or committing the transaction, [`vetted`]'s own reads, the
/// library-name lookup, the insert ([`insert_root`]) or the link — none of
/// which is this caller's mistake to fix.
pub fn add_root_checked_with(
    conn: &Connection,
    path: &Path,
    library: InLibrary<'_>,
    scan_interval_secs: Option<i64>,
) -> Result<Added, AddRootError> {
    let canonical = readable_dir(path)?;

    let tx = crate::Nested::open(conn, "add_root_checked").map_err(|e| {
        AddRootError::Failed(format!("starting to add {}: {e}", canonical.display()))
    })?;
    let joins = match library {
        InLibrary::Named(name) => {
            if let Err(why) = crate::libraries::usable_name(name) {
                return Err(AddRootError::Unusable(format!(
                    "{}: {why}",
                    canonical.display()
                )));
            }
            crate::libraries::id_of(&tx, name).map_err(AddRootError::Failed)?
        }
        InLibrary::Own => {
            if let Some(why) = own_library_clash(&tx, &canonical).map_err(AddRootError::Failed)? {
                return Err(AddRootError::Unusable(why));
            }
            None
        }
        InLibrary::Id(id) => {
            if crate::libraries::name_of(&tx, id)
                .map_err(AddRootError::Failed)?
                .is_none()
            {
                return Err(AddRootError::Unusable(format!(
                    "{}: there is no library {id}",
                    canonical.display()
                )));
            }
            Some(id)
        }
    };
    let held = roots(&tx).map_err(AddRootError::Failed)?;
    let placed = match vetted(&tx, &held, &canonical, joins).map_err(AddRootError::Failed)? {
        Vetted::Refused(why) => return Err(AddRootError::Unusable(why)),
        Vetted::New => Placed::New(
            insert_root(&tx, &canonical, library, scan_interval_secs)
                .map_err(AddRootError::Failed)?,
        ),
        Vetted::Link(held) => {
            let into = match library {
                // No library is named to link it into, and a library of its
                // own would hold a directory another library already holds.
                InLibrary::Own => {
                    return Err(AddRootError::Unusable(format!(
                        "{} is already a directory of {}",
                        canonical.display(),
                        the_libraries(&tx, &held).map_err(AddRootError::Failed)?.0
                    )));
                }
                InLibrary::Id(id) => id,
                InLibrary::Named(name) => match joins {
                    Some(id) => id,
                    None => crate::libraries::create(&tx, name).map_err(AddRootError::Failed)?,
                },
            };
            Placed::Linked(
                linked(&tx, into, held, scan_interval_secs).map_err(AddRootError::Failed)?,
            )
        }
    };
    tx.commit().map_err(|e| {
        AddRootError::Failed(format!("committing root {}: {e}", canonical.display()))
    })?;
    match placed {
        Placed::New(root) => Ok(probed(root)),
        Placed::Linked(added) => Ok(added),
    }
}

/// What an add wrote inside its transaction: a new directory, which is probed
/// once it commits, or a link, which is not.
enum Placed {
    New(Root),
    Linked(Added),
}

/// Directory `held`, which other libraries hold, linked into library `into`
/// inside the caller's transaction: one `library_root` row and nothing else,
/// as the settings, scans and documents are the directory's, not a library's.
/// Its scan columns are read there too, for [`Added::last_scan_at`]'s reason.
/// `asked` is the schedule the add asked for, which is not written:
/// [`Added::schedule_kept`] says when the shared one differs.
fn linked(conn: &Connection, into: i64, held: Root, asked: Option<i64>) -> Result<Added, String> {
    conn.execute(
        "INSERT INTO library_root (library_id, root_id) VALUES (?1, ?2)",
        [into, held.id],
    )
    .map_err(|e| format!("linking {} into library {into}: {e}", held.path.display()))?;
    let also_in = names_of(conn, &held.libraries)?;
    let (last_scan_at, last_scan_status) = conn
        .query_row(
            "SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1",
            [held.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("reading the scan status of {}: {e}", held.path.display()))?;
    let mut libraries = held.libraries.clone();
    libraries.push(into);
    libraries.sort_unstable();
    // Raw values, not `schedule::every`'s reading: Automatic and an Every of
    // the same length are different settings, and only one was asked for.
    let schedule_kept = held.scan_interval_secs != asked;
    Ok(Added {
        root: Root { libraries, ..held },
        probe: None,
        interval: None,
        schedule_kept,
        also_in,
        last_scan_at,
        last_scan_status,
    })
}

/// `path` in canonical form, having checked that it exists and that this
/// process can list it: [`add_root_checked_with`]'s first validation. Disk only,
/// so it runs before any transaction rather than holding the write lock
/// across a slow mount.
fn readable_dir(path: &Path) -> Result<PathBuf, AddRootError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| AddRootError::Unusable(format!("{}: {e}", path.display())))?;

    std::fs::read_dir(&canonical).map_err(|e| {
        AddRootError::Unusable(format!("{}: cannot be read: {e}", canonical.display()))
    })?;
    Ok(canonical)
}

/// How `dir` would join `library` (`None`: one being made), as [`vetted`]
/// answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Vetted {
    /// Why it cannot.
    Refused(String),
    /// Another library holds this directory: the add links it.
    Link(Root),
    /// No library holds it: the add inserts it.
    New,
}

/// How `dir` would join `library` (`None`: one being made), read from the
/// roots as `conn`'s transaction sees them (spec §4, §5): refused by
/// [`beside`], then a link to the directory another library holds there, then
/// refused inside or at a directory member of any root (`reads_as_one_volume`).
///
/// **The link is asked after every directory of `library`**: first, it would
/// link a directory inside or around one of `library`'s own. **A link is not
/// asked the member check**: like a move, it changes no walk and no file's
/// directory.
///
/// # Errors
///
/// A read fails, or the directory held at `dir` is in no library, which no
/// writer leaves: linked, its answer would name no library that held it.
pub(crate) fn vetted(
    conn: &Connection,
    held: &[Root],
    dir: &Path,
    library: Option<i64>,
) -> Result<Vetted, String> {
    if let Some(why) = beside(conn, held, dir, library, None)? {
        return Ok(Vetted::Refused(why));
    }
    if let Some(existing) = held.iter().find(|root| root.path == dir) {
        if existing.libraries.is_empty() {
            return Err(format!("directory {} is in no library", dir.display()));
        }
        return Ok(Vetted::Link(existing.clone()));
    }
    Ok(match at_a_member(conn, held, dir)? {
        Some(why) => Vetted::Refused(why),
        None => Vetted::New,
    })
}

/// [`vetted`]'s refusal against the directories of `library` among `held`,
/// with `except`, the directory a move moves, left out: `dir` is already one
/// of them, or sits inside or around one. A move asks only this: the member
/// check does not depend on the library, so moving a directory changes
/// nothing it asks. Every refusal names `library`, the one asked about.
pub(crate) fn beside(
    conn: &Connection,
    held: &[Root],
    dir: &Path,
    library: Option<i64>,
    except: Option<i64>,
) -> Result<Option<String>, String> {
    // A library being made holds no directory yet.
    let Some(target) = library else {
        return Ok(None);
    };
    let held: Vec<&Root> = held
        .iter()
        .filter(|root| Some(root.id) != except && root.libraries.contains(&target))
        .collect();
    let chosen = dir.display();
    // Named by its library, which is how the Libraries screen groups its
    // directories, and not by its id, which no screen shows.
    let name = crate::libraries::name_of(conn, target)?
        .ok_or_else(|| format!("there is no library {target}"))?;
    for existing in held {
        // Equal paths first: each starts with the other, so the nesting check
        // would call a directory picked again inside itself. A held directory
        // inside the chosen one is named from the chosen one, so the reader is
        // told where it is rather than left to compare two full paths for the
        // part that differs. A chosen directory inside a held one keeps both full
        // paths: the held one is the shorter.
        let said = if dir == existing.path {
            format!("{chosen} is already a directory of the library `{name}`")
        } else if dir.starts_with(&existing.path) {
            format!(
                "{chosen} is inside {}, which is already a directory of the same library, \
                 `{name}`",
                existing.path.display(),
            )
        } else if let Ok(inner) = existing.path.strip_prefix(dir) {
            format!(
                "{chosen} holds `{}`, which is already a directory of the same library, `{name}`",
                inner.display(),
            )
        } else {
            continue;
        };
        return Ok(Some(said));
    }
    Ok(None)
}

/// "the library `A`", or "the libraries `A` and `B`", for every library
/// holding `dir`, ascending by id, and whether it names more than one, for a
/// sentence whose verb agrees.
///
/// # Errors
///
/// A read fails, a link names no library, or `dir` is in no library, which no
/// writer leaves: the sentence would name none.
fn the_libraries(conn: &Connection, dir: &Root) -> Result<(String, bool), String> {
    let names = dir
        .libraries
        .iter()
        .map(|&id| {
            crate::libraries::name_of(conn, id)?
                .map(|name| format!("`{name}`"))
                .ok_or_else(|| format!("there is no library {id}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(match &names[..] {
        [] => return Err(format!("directory {} is in no library", dir.path.display())),
        [one] => (format!("the library {one}"), false),
        [first, rest @ ..] => (
            format!("the libraries {}", crate::libraries::listed(first, rest)),
            true,
        ),
    })
}

/// [`vetted`]'s member check, naming every library holding the directory the
/// member is under.
fn at_a_member(conn: &Connection, held: &[Root], dir: &Path) -> Result<Option<String>, String> {
    let chosen = dir.display();
    for existing in held {
        let Ok(under) = dir.strip_prefix(&existing.path) else {
            continue;
        };
        for above in under.ancestors() {
            let rel_path = above.to_string_lossy();
            if !reads_as_one_volume(conn, existing.id, &rel_path)? {
                continue;
            }
            let (names, several) = the_libraries(conn, existing)?;
            let reads = if several { "read" } else { "reads" };
            if above == under {
                return Ok(Some(format!(
                    "{chosen} is a folder {names} {reads} as the pages of one volume"
                )));
            }
            let member = if rel_path.is_empty() {
                existing.path.clone()
            } else {
                existing.path.join(above)
            };
            return Ok(Some(format!(
                "{chosen} is inside {}, which {names} {reads} as the pages of one volume",
                member.display()
            )));
        }
    }
    Ok(None)
}

/// Whether root `root_id`'s directory at `rel_path` is a directory member (a
/// folder of page images read as one volume) or a byte-identical copy of one,
/// as the last derive left them: the derive hashes only what it stages, onto
/// each copy's own row, so a copy is found at its own path too. A root inside
/// one leaves the member's page count short of what the reader serves. A root
/// at one makes the folder its own nameless top entry, which the seeded rules
/// read no series from, while spec §4 has the outer's walk skip a nested root,
/// so the volume would be in neither library.
fn reads_as_one_volume(conn: &Connection, root_id: i64, rel_path: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM scan_entry e JOIN member m ON m.sha256 = e.sha256
                         WHERE e.root_id = ?1 AND e.rel_path = ?2 AND e.kind = ?3)",
        rusqlite::params![root_id, rel_path, Kind::Dir.as_str()],
        |r| r.get(0),
    )
    .map_err(|e| format!("reading whether {rel_path} is read as one volume: {e}"))
}

/// [`add_root_checked_with`]'s third step, for a root just inserted: the
/// watch probe, reported and not written, so the row keeps the schedule its
/// insert stored.
fn probed(root: Root) -> Added {
    let probe = crate::watch::probe(&root.path, PROBE_TIMEOUT);
    let interval = crate::watch::interval_for(&probe);
    Added {
        root,
        probe: Some(probe),
        interval: Some(interval),
        schedule_kept: false,
        also_in: Vec::new(),
        last_scan_at: None,
        last_scan_status: None,
    }
}

/// What [`create_library`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub library_id: i64,
    /// The name as stored.
    pub name: String,
    /// Each library directory (a `root` row) with its watch probe, in the
    /// order given.
    pub dirs: Vec<Added>,
}

/// A new library called `name` holding each of `paths`: every directory, or no
/// library and no directory.
///
/// Every path is read from disk and compared with the other paths given before
/// the transaction opens. Inside it, each is compared with the roots held
/// ([`vetted`], as a library being made), and the library is inserted with
/// each new directory, linking each another library holds. The new ones' watch
/// probes run after it commits, because each waits up to [`PROBE_TIMEOUT`] and
/// a write transaction held that long would stall every other writer.
///
/// # Errors
///
/// Returns [`AddRootError::Unusable`] for no path, a name
/// [`crate::libraries::usable_name`] refuses or that a library already holds,
/// a path [`add_root_checked_with`] would refuse, or two of the paths
/// overlapping. Returns [`AddRootError::Failed`] if the store refuses.
pub fn create_library(
    conn: &Connection,
    name: &str,
    paths: &[PathBuf],
    scan_interval_secs: Option<i64>,
) -> Result<Created, AddRootError> {
    if paths.is_empty() {
        return Err(AddRootError::Unusable(
            "a library needs at least one directory".to_string(),
        ));
    }
    let name = crate::libraries::usable_name(name)
        .map_err(|why| AddRootError::Unusable(why.to_string()))?;

    let mut canonical: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for path in paths {
        let held = readable_dir(path)?;
        if let Some(earlier) = canonical
            .iter()
            .find(|earlier| held.starts_with(earlier) || earlier.starts_with(&held))
        {
            return Err(AddRootError::Unusable(format!(
                "{} and {} overlap; a library's directories may not be the same directory or sit \
                 inside one another",
                earlier.display(),
                held.display()
            )));
        }
        canonical.push(held);
    }

    let tx = crate::Nested::open(conn, "create_library")
        .map_err(|e| AddRootError::Failed(format!("starting to create `{name}`: {e}")))?;
    // Asked inside the transaction: a lookup before it would be a second
    // statement for another writer to land between. The name it found free, or
    // the path it found unheld, would then fail at the insert as a store fault,
    // and a directory another writer recorded would be compared without it and
    // accepted (O204). The disk was read before it, so the write lock is not
    // held across a slow mount.
    // Each path with the directory another library holds there, if one does.
    let stored = roots(&tx).map_err(AddRootError::Failed)?;
    let mut held = Vec::with_capacity(canonical.len());
    for path in &canonical {
        held.push((
            path,
            match vetted(&tx, &stored, path, None).map_err(AddRootError::Failed)? {
                Vetted::Refused(why) => return Err(AddRootError::Unusable(why)),
                Vetted::Link(root) => Some(root),
                Vetted::New => None,
            },
        ));
    }
    if let Some(taken) = crate::libraries::id_of(&tx, name).map_err(AddRootError::Failed)? {
        let stored = crate::libraries::name_of(&tx, taken)
            .map_err(AddRootError::Failed)?
            .unwrap_or_else(|| name.to_string());
        // Adding it to that library is no advice for a directory already in it.
        if let Some((path, _)) = held.iter().find(|(_, root)| {
            root.as_ref()
                .is_some_and(|root| root.libraries.contains(&taken))
        }) {
            return Err(AddRootError::Unusable(format!(
                "{} is already a directory of the library `{stored}`",
                path.display()
            )));
        }
        return Err(AddRootError::Unusable(format!(
            "a library is already called `{stored}`; add the directory to it instead"
        )));
    }
    let library_id = crate::libraries::create(&tx, name).map_err(AddRootError::Failed)?;
    let mut placed = Vec::with_capacity(held.len());
    for (path, held) in held {
        placed.push(match held {
            // A link keeps the shared row's schedule, as in `add_root_checked_with`:
            // one library must not start or stop another's scans.
            Some(root) => Placed::Linked(
                linked(&tx, library_id, root, scan_interval_secs).map_err(AddRootError::Failed)?,
            ),
            None => Placed::New(
                insert_root(&tx, path, InLibrary::Id(library_id), scan_interval_secs)
                    .map_err(AddRootError::Failed)?,
            ),
        });
    }
    tx.commit()
        .map_err(|e| AddRootError::Failed(format!("committing the library `{name}`: {e}")))?;

    let mut dirs = Vec::with_capacity(placed.len());
    for one in placed {
        dirs.push(match one {
            Placed::New(root) => probed(root),
            Placed::Linked(added) => added,
        });
    }
    Ok(Created {
        library_id,
        name: name.to_string(),
        dirs,
    })
}

// --- root update -------------------------------------------------------

/// A change to one existing root — everything [`update_root`] needs to patch
/// a row without touching the columns nobody named.
///
/// `Default`-derived. The five nullable columns are `Option<Option<_>>` for
/// `RulePatch`'s reason (`rules.rs`): clearing `edition_key` back to "no
/// default" is a different request from not mentioning it, and a
/// single-layer `Option<String>` cannot spell the first without spelling the
/// second too — there would be no way to write `Some(None)` at all. The one
/// `NOT NULL` column, `keeps_documents`, takes a plain `Option<bool>` for the
/// same reason `rules::RulePatch`'s five non-nullable columns do: a caller
/// can replace it or leave it alone, never clear it.
#[derive(Debug, Clone, Default)]
pub struct RootPatch {
    pub edition_key: Option<Option<String>>,
    pub default_kind: Option<Option<String>>,
    pub default_language: Option<Option<String>>,
    pub exclude_globs: Option<Option<String>>,
    pub scan_interval_secs: Option<Option<i64>>,
    pub keeps_documents: Option<bool>,
}

/// Patch root `id` from whichever fields of `patch` are `Some`, and report
/// whether `id` named a row at all.
///
/// **Builds one `UPDATE` naming only the patched columns** —
/// [`crate::rules::update`]'s own shape, copied rather than reinvented: a
/// placeholder and a positional parameter per field actually present in
/// `patch`, nothing written for a field left `None`. The alternative —
/// always writing every column, refilling whatever the caller did not patch
/// from a fresh read — is the bug this exists to avoid: a patch that only
/// means to flip `keeps_documents` must not silently clear `edition_key`
/// back to NULL along with it.
///
/// **Reports whether a row existed to patch, not whether any value on it
/// actually differed** — [`crate::rules::update`]'s own empty-`SET`-list
/// case, handled the same way: a patch naming nothing at all has no valid
/// `UPDATE … SET` to issue, so this asks for `id`'s existence directly.
///
/// # Errors
///
/// Returns an error if SQLite refuses the read or the write. An `id` naming
/// no root is `Ok(false)`, not an error, and patches nothing.
pub fn update_root(conn: &Connection, id: i64, patch: &RootPatch) -> Result<bool, String> {
    let mut sets: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(v) = &patch.edition_key {
        params.push(Box::new(v.clone()));
        sets.push(format!("edition_key = ?{}", params.len()));
    }
    if let Some(v) = &patch.default_kind {
        params.push(Box::new(v.clone()));
        sets.push(format!("default_kind = ?{}", params.len()));
    }
    if let Some(v) = &patch.default_language {
        params.push(Box::new(v.clone()));
        sets.push(format!("default_language = ?{}", params.len()));
    }
    if let Some(v) = &patch.exclude_globs {
        params.push(Box::new(v.clone()));
        sets.push(format!("exclude_globs = ?{}", params.len()));
    }
    if let Some(v) = patch.scan_interval_secs {
        params.push(Box::new(v));
        sets.push(format!("scan_interval_secs = ?{}", params.len()));
    }
    if let Some(v) = patch.keeps_documents {
        params.push(Box::new(v));
        sets.push(format!("keeps_documents = ?{}", params.len()));
    }

    if sets.is_empty() {
        return conn
            .query_row("SELECT 1 FROM root WHERE id = ?1", [id], |_| Ok(()))
            .optional()
            .map(|found| found.is_some())
            .map_err(|e| format!("checking whether root {id} exists: {e}"));
    }

    params.push(Box::new(id));
    let sql = format!(
        "UPDATE root SET {} WHERE id = ?{}",
        sets.join(", "),
        params.len()
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(AsRef::as_ref).collect();
    let changed = conn
        .execute(&sql, refs.as_slice())
        .map_err(|e| format!("updating root {id}: {e}"))?;
    Ok(changed == 1)
}

// --- removal -----------------------------------------------------------

/// What removing a root costs — the works that go with it, and the reading
/// positions and notes that go with *them*.
///
/// C4 §2 calls removal "the most destructive action in the product, and it
/// does not look like it," and requires the counts stated before it acts:
/// "This removes 412 works, 3 in-progress positions and 27 notes." This
/// struct is that sentence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Removal {
    pub works: usize,
    pub positions: usize,
    pub notes: usize,
    /// The anchored in-progress positions on works this removal keeps whose
    /// page it takes: each that resolves in the store's layout before its
    /// deletes and not after its derive (`state::Resolving`). Taken inside the
    /// transaction, so the dry run and the removal answer the same measured
    /// figure. A row on a work that goes is in [`Removal::positions`] instead.
    /// One a fold carries to another work, which is rare, is in neither, unless
    /// it replaces a position that work held on its edition: it is counted
    /// when that one resolved and it does not (O244).
    pub positions_page_gone: usize,
    /// [`Removal::positions_page_gone`] for notes, which resolve only on the
    /// page and half they were written on.
    pub notes_page_gone: usize,
    /// The accounts, by name, that see no library once this removal deletes
    /// every library they are granted: [`only_grant_holders`] of
    /// [`Removal::libraries_removed`], asked before anything is deleted. Empty
    /// where no library goes, as when a directory that is not its library's
    /// last goes.
    pub left_with_no_library: Vec<String>,
    /// The names of the libraries this removal empties, which go with their
    /// last directory, ascending by id, read inside the removal's transaction
    /// while they stand. A directory two libraries hold, and the last of
    /// each, empties both.
    pub libraries_removed: Vec<String>,
    /// The directories a library's removal unlinks and leaves to the other
    /// libraries that hold them, oldest first. Always empty for a directory's
    /// removal, which takes the directory out of every library.
    pub kept: Vec<Kept>,
    /// The ids of the directories this removal deletes, oldest first, read
    /// inside its transaction: whose scan slots a caller clears, since a
    /// freed id is given to the next directory added.
    pub removed: Vec<i64>,
}

/// A directory a library's removal leaves standing, because another library
/// holds it too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub root_id: i64,
    pub path: PathBuf,
    /// The names of the libraries that still hold it, ascending by id.
    pub libraries: Vec<String>,
}

/// What Remove from this library did, or in a rehearsal would do: the sight
/// fields of [`crate::relocate::Moved`], under the same names, since an
/// unlink changes who sees a directory and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Unlinked {
    /// The accounts, by name and in name order, that see the directory or a
    /// directory nested in it before the unlink and not after, as
    /// [`crate::relocate::Moved::loses_sight`] names them.
    pub loses_sight: Vec<String>,
    /// The library the directory left, where it was that library's last
    /// directory, and the grants that went with it.
    pub library_removed: Option<String>,
    pub grants_removed: usize,
    /// The accounts, by name, whose only grant went with that library:
    /// [`only_grant_holders`], asked before the unlink. Empty where no
    /// library is removed.
    pub left_with_no_library: Vec<String>,
}

/// Why an unlink was not made. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnlinkError {
    /// No such directory, no such library, or a library that does not hold
    /// the directory.
    Missing(String),
    /// The directory's last link: Remove from this library never deletes, so
    /// the directory can only be removed (overlapping-libraries spec §5,
    /// decision 24).
    Unusable(String),
    /// A store fault.
    Failed(String),
}

/// Take root `root_id` out of library `library_id`, which another library
/// keeps holding, or with `dry_run` answer what that would do, found by doing
/// it inside a transaction this rolls back.
///
/// **It deletes the link and nothing else.** Every row a directory's works are
/// made of is keyed by the directory or by the work, never by a library, so
/// no work, position or note goes, nothing is derived, and no document moves.
/// Migration 19's `library_emptied_by_unlink` removes a library the unlink
/// empties, and its grants with it, which is why those are read first.
///
/// # Errors
///
/// See [`UnlinkError`]. A refused or failed unlink changed nothing.
pub fn unlink(
    conn: &Connection,
    root_id: i64,
    library_id: i64,
    dry_run: bool,
) -> Result<Unlinked, UnlinkError> {
    let tx = crate::Nested::open(conn, "unlink")
        .map_err(|e| UnlinkError::Failed(format!("starting an unlink: {e}")))?;
    let unlinked = apply_unlink(&tx, root_id, library_id)?;
    if !dry_run {
        tx.commit().map_err(|e| {
            UnlinkError::Failed(format!(
                "committing the unlink of root {root_id} from library {library_id}: {e}"
            ))
        })?;
    }
    Ok(unlinked)
}

/// Whether root `root_id` can be taken out of library `library_id`: the
/// refusals an unlink makes first, in its words, for a caller to ask before it
/// waits on anything, as [`crate::relocate::check_source`] is asked before a
/// move waits, since a page older than an unlink names a link that has gone.
///
/// # Errors
///
/// [`UnlinkError::Missing`] or [`UnlinkError::Unusable`], as [`unlink`]
/// refuses; [`UnlinkError::Failed`] for a store fault.
pub fn check_unlink(conn: &Connection, root_id: i64, library_id: i64) -> Result<(), UnlinkError> {
    unlinkable(conn, root_id, library_id).map(|_| ())
}

/// The library's name, where [`unlink`] would not refuse.
fn unlinkable(conn: &Connection, root_id: i64, library_id: i64) -> Result<String, UnlinkError> {
    let library = crate::libraries::name_of(conn, library_id)
        .map_err(UnlinkError::Failed)?
        .ok_or_else(|| UnlinkError::Missing(format!("there is no library {library_id}")))?;
    let dir = roots(conn)
        .map_err(UnlinkError::Failed)?
        .into_iter()
        .find(|root| root.id == root_id)
        .ok_or_else(|| UnlinkError::Missing(format!("there is no directory {root_id}")))?;
    if !dir.libraries.contains(&library_id) {
        return Err(UnlinkError::Missing(format!(
            "`{}` is not in {library}",
            dir.path.display()
        )));
    }
    if dir.libraries.len() == 1 {
        return Err(UnlinkError::Unusable(format!(
            "`{}` is in no other library, so it can only be removed: use Remove this directory.",
            dir.path.display()
        )));
    }
    Ok(library)
}

/// [`unlink`], inside the caller's transaction.
fn apply_unlink(conn: &Connection, root_id: i64, library_id: i64) -> Result<Unlinked, UnlinkError> {
    let library = unlinkable(conn, root_id, library_id)?;
    let left = crate::relocate::leaving(conn, root_id, (library_id, library), || {
        conn.execute(
            "DELETE FROM library_root WHERE library_id = ?1 AND root_id = ?2",
            [library_id, root_id],
        )
        .map(|_| ())
        .map_err(|e| format!("unlinking root {root_id} from library {library_id}: {e}"))
    })
    .map_err(UnlinkError::Failed)?;
    Ok(Unlinked {
        loses_sight: left.loses_sight,
        library_removed: left.library_removed,
        grants_removed: left.grants_removed,
        left_with_no_library: left.left_with_no_library,
    })
}

/// Every work whose *every* member sits under root `?1` alone — the works
/// [`take_root`] deletes, and what [`Removal::works`] counts.
///
/// **Asked after [`hand_back`]**, so a work is named only where no directory
/// around or inside `?1` takes its files: those the directory around excludes,
/// or, with none around, those outside every directory nested in `?1`.
///
/// **A subquery, reused by parameter rather than materialised into a
/// `Vec<i64>` and an `IN (?, ?, …)` built for it.** [`take_root`]'s counts of
/// works, positions and notes and its delete each need this exact set, and one
/// definition keeps them in step.
///
/// **Read from `member_root`, every directory that holds a copy of a member,
/// not from `member.root_id`, the one copy its path names** (the
/// overlapping-libraries spec, §3.5). A member held under `?1` and under
/// another directory still has its bytes once `?1` is gone, and so does a work
/// with a member on some other, surviving root
/// (`a_work_spanning_two_roots_gives_each_member_its_own_root`,
/// `derive/tests.rs`); only the work whose material is entirely `?1`'s is a
/// work removing `?1` actually destroys.
///
/// **A member with no `member_root` row at all excludes its work too.** No row
/// means an imported work, whose files no directory accounts for; that is no
/// evidence that the work lives under `?1`, and a work is kept standing rather
/// than guessed away.
///
/// **Computed before `?1`'s own `root` row is deleted, and it has to be.**
/// `member_root` rows `CASCADE` with their root, so the rows naming `?1` are
/// gone the instant the `root` row goes. [`take_root`] reads this before it
/// deletes anything, and [`apply_removal`] and [`apply_library_removal`] reach
/// it through that.
const DOOMED_WORKS: &str = "
    SELECT DISTINCT e.work_id
      FROM member_root mr
      JOIN member m ON m.id = mr.member_id
      JOIN edition e ON e.id = m.edition_id
     WHERE mr.root_id = ?1
       AND NOT EXISTS (
             SELECT 1 FROM member m2
             JOIN edition e2 ON e2.id = m2.edition_id
            WHERE e2.work_id = e.work_id
              AND (NOT EXISTS (SELECT 1 FROM member_root r2 WHERE r2.member_id = m2.id)
                   OR EXISTS (SELECT 1 FROM member_root r2
                               WHERE r2.member_id = m2.id AND r2.root_id != ?1))
           )";

/// The accounts, by name and in name order, that see no library once every
/// one of `libraries` goes: limited accounts granted at least one of them and
/// none outside them.
///
/// **Asked of the set at once**: an account granted exactly the two libraries
/// one removal empties holds another grant whichever is asked about alone.
/// Never named: an administrator or an account set to every library, whose
/// rows limit neither, and one with no grant left to lose. Asked before the
/// libraries are deleted, because their grants go with them.
pub(crate) fn only_grant_holders(
    conn: &Connection,
    libraries: &[i64],
) -> Result<Vec<String>, String> {
    if libraries.is_empty() {
        return Ok(Vec::new());
    }
    let among = (1..=libraries.len())
        .map(|n| format!("?{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let failed = |e: rusqlite::Error| format!("checking whose only grants {libraries:?} are: {e}");
    let mut statement = conn
        .prepare(&format!(
            "SELECT u.name
               FROM user u
              WHERE u.role = 'user'
                AND u.every_library = 0
                AND EXISTS (SELECT 1 FROM user_library ul
                             WHERE ul.user_id = u.id AND ul.library_id IN ({among}))
                AND NOT EXISTS (SELECT 1 FROM user_library held
                                 WHERE held.user_id = u.id
                                   AND held.library_id NOT IN ({among}))
              ORDER BY u.name"
        ))
        .map_err(failed)?;
    statement
        .query_map(rusqlite::params_from_iter(libraries), |r| {
            r.get::<_, String>(0)
        })
        .map_err(failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failed)
}

/// Hand what root `removed` holds to the directories that hold its files once
/// it is gone (spec §4), in the caller's transaction before [`take_root`], so
/// [`DOOMED_WORKS`] names only what none of them takes, and note in `carried`
/// each work whose members move. Returns the `scan_entry` rows copied.
/// **Copied rather than walked again:** a walk cannot run inside the removal's
/// transaction, so the disk is asked only before it, by [`gone_from_disk`].
/// `dir_state` is not copied, so the receiving directory's next walk lists
/// those directories whole.
fn hand_back(
    tx: &Connection,
    removed: i64,
    gone: &HashSet<String>,
    carried: &mut Carried,
) -> Result<usize, String> {
    let all = roots(tx)?;
    let Some(root) = all.iter().find(|root| root.id == removed) else {
        return Ok(0);
    };
    let failed = |what: &str, e: rusqlite::Error| {
        format!("handing back {what} of {}: {e}", root.path.display())
    };
    let inside: Vec<(&Root, String, Excluder)> = all
        .iter()
        .filter_map(|other| {
            let rel = crate::reach::nested_in(&other.path, &root.path)?;
            Some((other, rel, Excluder::build(other.exclude_globs.as_deref())))
        })
        .collect();
    let outer = Enclosing::of(&all, root);
    let rows: Vec<(String, String)> = tx
        .prepare("SELECT rel_path, kind FROM scan_entry WHERE root_id = ?1")
        .and_then(|mut stmt| {
            stmt.query_map([removed], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .map_err(|e| failed("the entries", e))?;
    // The directories nested in `removed` whose trees hold `rel_path`,
    // deepest first, each with the path relative to it and its patterns.
    let nested_at = |rel_path: &str| {
        let mut holding: Vec<_> = inside
            .iter()
            .filter(|(_, rel, _)| under_any(rel_path, std::slice::from_ref(rel)))
            .map(|(nested, rel, excluder)| {
                // `rel_path` is `rel` or beneath it, as `under_any` asked.
                let own = rel_path
                    .strip_prefix(rel.as_str())
                    .map_or(rel_path, |rest| rest.strip_prefix('/').unwrap_or(rest));
                (nested.id, own.to_string(), excluder)
            })
            .collect();
        holding.sort_by_key(|(_, own, _)| own.len());
        holding
    };
    // Where a row at `rel_path` goes once `removed` is gone, by id, with the
    // path there: the deepest directory left whose tree holds it, where that
    // one's walk reaches it, else, where that one is nested in `removed`, the
    // directory immediately around it, where its walk does (decision 21);
    // `None` where neither does. A directory further out is not asked.
    let place = |rel_path: &str, is_dir: bool| -> Option<(i64, String)> {
        let nested = nested_at(rel_path);
        if let Some((to, own, _)) = nested
            .iter()
            .take(2)
            .find(|(_, own, excluder)| excluder.visits(own, is_dir))
        {
            return Some((*to, own.clone()));
        }
        if nested.len() > 1 {
            return None;
        }
        outer.as_ref()?.place(rel_path, is_dir)
    };
    let mut copied = 0;
    let mut went_to: HashMap<String, i64> = HashMap::new();
    for (rel_path, kind) in &rows {
        let Some((to, to_rel)) = place(rel_path, kind == Kind::Dir.as_str()) else {
            continue;
        };
        copied +=
            copy_entry(tx, removed, rel_path, to, &to_rel).map_err(|e| failed(rel_path, e))?;
        went_to.insert(root.path.join(rel_path).to_string_lossy().into_owned(), to);
    }
    // Each member at a copied row's path follows the row. A member left on
    // `removed` is one `take_root` removes.
    let members = members_of(tx, removed).map_err(|e| failed("the members", e))?;
    let held: HashSet<String> = rows
        .iter()
        .map(|(rel, _)| root.path.join(rel).to_string_lossy().into_owned())
        .collect();
    let own = Excluder::build(root.exclude_globs.as_deref());
    let here = format!("{}/", root.path.to_string_lossy());
    let reaches = |excluder: &Excluder, at: &str| excluder.reaches_member(at);
    for (_, path, ..) in &members {
        if went_to.contains_key(path) {
            continue;
        }
        let Some(rel) = path.strip_prefix(here.as_str()) else {
            continue;
        };
        let nested = nested_at(rel);
        // With no row, the file vanished only where every walk that may last
        // have looked for it reaches it: `removed`'s, which looked before a
        // directory nested in it was added, and each such directory's since.
        // And only where the disk says so too: a pattern lifted with no scan
        // since makes a walk "reach" a file it never looked for.
        let looked =
            reaches(&own, rel) && nested.iter().all(|(_, at, excluder)| reaches(excluder, at));
        if !held.contains(path) && looked && gone.contains(path) {
            continue;
        }
        // Any other file is taken to be on disk: a row says the last walk saw
        // it, and a row forgotten by exclusion says nothing. With no row
        // copied, its member goes to the nearest directory left around the
        // file, the one walk that can record it again: its work stays missing,
        // not deleted, until that walk does (the owner's rulings of 2026-11-10,
        // "Never delete on disk", "Cover both" and "Every removal"). With no
        // directory left around the file, it stays for `take_root` to remove.
        let to = nested
            .first()
            .map(|(nested, ..)| *nested)
            .or_else(|| outer.as_ref().map(|outer| outer.root.id));
        if let Some(to) = to {
            went_to.insert(path.clone(), to);
        }
    }
    move_members(tx, &members, &went_to, carried, &failed)?;
    Ok(copied)
}

/// The paths of the row-less members of each directory `removing` picks at
/// which the disk holds nothing, of either kind, for [`hand_back`]: a pattern
/// lifted with no scan since makes a walk reach a file it never looked for,
/// and a removal must never delete a work whose file is there inside a
/// directory left (the owner's ruling of 2026-11-11, "Fix it"). Read before
/// the removal's transaction, so a slow share's stat holds no lock. A stat
/// failing other than "not found", a member this read missed, and a file
/// under a directory that [`reads_unmounted`] each keep their work.
fn gone_from_disk(
    conn: &Connection,
    removing: impl Fn(&Root) -> bool,
) -> Result<HashSet<String>, String> {
    let all = roots(conn)?;
    let mut unmounted: HashMap<i64, bool> = HashMap::new();
    let mut gone = HashSet::new();
    for root in all.iter().filter(|root| removing(root)) {
        let failed = |e: rusqlite::Error| {
            format!(
                "reading what {} holds with no row: {e}",
                root.path.display()
            )
        };
        let rows: HashSet<String> = conn
            .prepare("SELECT rel_path FROM scan_entry WHERE root_id = ?1")
            .and_then(|mut stmt| {
                stmt.query_map([root.id], |r| {
                    Ok(root
                        .path
                        .join(r.get::<_, String>(0)?)
                        .to_string_lossy()
                        .into_owned())
                })?
                .collect()
            })
            .map_err(failed)?;
        for (_, path, ..) in members_of(conn, root.id).map_err(failed)? {
            if rows.contains(&path)
                || !std::fs::metadata(&path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                continue;
            }
            // Each directory whose tree holds the file is asked, the removed
            // one and those left around and inside it, since any may be the
            // share that is not mounted.
            let at = Path::new(&path);
            let under_unmounted = all
                .iter()
                .filter(|dir| at.starts_with(&dir.path))
                .any(|dir| {
                    *unmounted
                        .entry(dir.id)
                        .or_insert_with(|| reads_unmounted(&dir.path))
                });
            if !under_unmounted {
                gone.insert(path);
            }
        }
    }
    Ok(gone)
}

/// Whether directory `path` reads as an unmounted share does, §8.1's test at a
/// removal (the owner's ruling of 2026-11-11, "Add a guard"): absent or
/// unreadable, or holding no name a walk visits, a walk skipping every dot
/// name. Read at its top only, where §8.1 walks the tree: any entry there is
/// something beneath it, and the patterns and nested directories §8.1's walk
/// also applies shape what a scan records, not whether the share is mounted.
fn reads_unmounted(path: &Path) -> bool {
    !std::fs::read_dir(path).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
    })
}

/// The deepest directory around another: where a file the inner one stops
/// holding goes, if this one's walk reaches it (spec §4, decision 21).
struct Enclosing<'a> {
    root: &'a Root,
    /// The inner directory's path relative to `root`.
    rel: String,
    excluder: Excluder,
}

impl<'a> Enclosing<'a> {
    fn of(all: &'a [Root], inner: &Root) -> Option<Self> {
        around(all, inner)
            .into_iter()
            .max_by_key(|(outer, ..)| outer.path.components().count())
            .map(|(root, rel, _)| Self {
                root,
                rel,
                excluder: Excluder::build(root.exclude_globs.as_deref()),
            })
    }

    /// Where this directory's walk reaches `rel_path`, a path relative to the
    /// inner one: its id and the path relative to it, or `None` where its
    /// patterns keep the walk out. Asked per file, so a file-level pattern
    /// that governed nothing while the inner directory held the file governs
    /// it again.
    fn place(&self, rel_path: &str, is_dir: bool) -> Option<(i64, String)> {
        let to_rel = if rel_path.is_empty() {
            self.rel.clone()
        } else {
            join_rel(&self.rel, rel_path)
        };
        self.excluder
            .visits(&to_rel, is_dir)
            .then_some((self.root.id, to_rel))
    }
}

/// Which directory holds a row at `rel_path` that the walk of the directory
/// whose patterns are `excluder` does not reach (decision 21): `around`, by id
/// with the path there, where its walk reaches the file; `None` where the
/// inner walk reaches it, or no walk does. The one decision behind keeping a
/// row ([`take_over`]), handing one back ([`forget_unseen`]) and §8.1's counts,
/// so which came first, the pattern or the scan, cannot change the outcome.
fn outer_holder(
    around: Option<&Enclosing>,
    excluder: &Excluder,
    rel_path: &str,
    is_dir: bool,
) -> Option<(i64, String)> {
    if excluder.visits(rel_path, is_dir) {
        return None;
    }
    around?.place(rel_path, is_dir)
}

/// Copy directory `from`'s `scan_entry` row at `rel_path` to directory `to` at
/// `to_rel`. A row `to` already holds there is kept as it is. Returns the rows
/// copied.
fn copy_entry(
    tx: &Connection,
    from: i64,
    rel_path: &str,
    to: i64,
    to_rel: &str,
) -> rusqlite::Result<usize> {
    tx.execute(
        "INSERT INTO scan_entry
             (root_id, rel_path, kind, format, size, mtime, pages, title, sha256, first_seen,
              last_seen)
         SELECT ?1, ?2, kind, format, size, mtime, pages, title, sha256, first_seen, last_seen
           FROM scan_entry WHERE root_id = ?3 AND rel_path = ?4
         ON CONFLICT (root_id, rel_path) DO NOTHING",
        rusqlite::params![to, to_rel, from, rel_path],
    )
}

/// A member: its id, its path, its work and its format.
type MemberRow = (i64, String, i64, Option<String>);

/// Each member directory `root_id` holds.
fn members_of(tx: &Connection, root_id: i64) -> rusqlite::Result<Vec<MemberRow>> {
    tx.prepare(
        "SELECT m.id, m.path, e.work_id, m.format FROM member m
           JOIN edition e ON e.id = m.edition_id
          WHERE m.root_id = ?1",
    )
    .and_then(|mut stmt| {
        stmt.query_map([root_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect()
    })
}

/// Re-point each of `members` whose path `went_to` names (joined as
/// `derive::classify` joins it) to the directory named there; migration 018's
/// trigger moves its own `member_root` row, and the next derive its copies'.
/// Each moving member's work's address is noted in `carried` first: read
/// before any member moves or a derive rekeys the work, since afterwards
/// nothing names where its document is.
fn move_members(
    tx: &Connection,
    members: &[MemberRow],
    went_to: &HashMap<String, i64>,
    carried: &mut Carried,
    failed: &dyn Fn(&str, rusqlite::Error) -> String,
) -> Result<(), String> {
    note_addresses(
        tx,
        members
            .iter()
            .filter(|(_, path, ..)| went_to.contains_key(path))
            .map(|(_, _, work, _)| *work),
        carried,
    )?;
    for (member, path, ..) in members {
        if let Some(to) = went_to.get(path) {
            tx.execute(
                "UPDATE member SET root_id = ?1 WHERE id = ?2",
                rusqlite::params![to, member],
            )
            .map_err(|e| failed(path, e))?;
        }
    }
    Ok(())
}

/// Each work whose hand document a hand-over or a removal must carry, with
/// the address the document is at.
type Carried = BTreeMap<i64, PathBuf>;

/// Add to `carried` the address of each of `works` it does not hold yet.
///
/// **The first address read stands.** A library removal hands back one
/// directory after another, and a work read again after an earlier directory
/// moved its members would name where its document now belongs, not where it
/// is.
fn note_addresses(
    conn: &Connection,
    works: impl IntoIterator<Item = i64>,
    carried: &mut Carried,
) -> Result<(), String> {
    for work in works {
        if let std::collections::btree_map::Entry::Vacant(slot) = carried.entry(work)
            && let Some(address) = crate::hand::path_for(conn, work)?
        {
            slot.insert(address);
        }
    }
    Ok(())
}

/// Move each document `carried` names to its work's address now: after the
/// commit that moved the members, for [`crate::hand::rename_from`]'s reason,
/// and before the derive that follows, whose `hand::reconcile` would
/// otherwise report it as an orphan and write a fresh one at the new address.
/// Answers each document that stays in a `.taiyaku` no root holds now, which
/// no `reconcile` reads, for the caller to report.
fn carry_documents(
    conn: &Connection,
    carried: &Carried,
) -> Result<Vec<crate::hand::Orphan>, String> {
    // Most scans and removals carry nothing, and need no read after their
    // commit that could fail them.
    if carried.is_empty() {
        return Ok(Vec::new());
    }
    let held: Vec<PathBuf> = roots(conn)?
        .into_iter()
        .map(|root| root.path.join(crate::hand::DIR))
        .collect();
    let mut left = Vec::new();
    for (work, old) in carried {
        if crate::hand::rename_from(conn, *work, old)? {
            continue;
        }
        // Most carried works have no document. One under a live root is not
        // listed: it can still be its work's document, which listing would
        // call an orphan, and if it is not, that root's `reconcile` reports it.
        let under_a_root = old
            .parent()
            .is_some_and(|dir| held.iter().any(|kept| kept == dir));
        if old.is_file() && !under_a_root {
            left.push(crate::hand::Orphan {
                path: old.clone(),
                title: crate::hand::title_in(old),
                absorbed_into: None,
            });
        }
    }
    Ok(left)
}

/// Each of `all` that a scan must not run beside while the directories
/// `removed` go: those directories, each directory nested in one and each
/// directory around one. [`hand_back`] gives files to the nested ones and to
/// the deepest directory around only; refusing the others is conservative.
pub fn removal_touches(all: &[Root], removed: &[i64]) -> Vec<Root> {
    let gone: Vec<&Root> = all
        .iter()
        .filter(|root| removed.contains(&root.id))
        .collect();
    all.iter()
        .filter(|root| {
            gone.iter().any(|gone| {
                gone.id == root.id
                    || crate::reach::nested_in(&root.path, &gone.path).is_some()
                    || crate::reach::nested_in(&gone.path, &root.path).is_some()
            })
        })
        .cloned()
        .collect()
}

/// Count what root `id` alone accounts for, then delete it: its works, the
/// members it alone holds on works that survive it, and the root. Nothing is
/// derived and no account is named; [`apply_removal`] and
/// [`apply_library_removal`] do both around it, in one transaction.
///
/// A member whose every `member_root` row is `id`'s is deleted, not left under
/// no directory (O54). One with a copy elsewhere is kept, and where `id` held
/// its `root_id` and `path` they are re-pointed at that copy; see
/// [`repoint_kept_members`]. Each surviving work of either goes in `carried`.
fn take_root(conn: &Connection, id: i64, carried: &mut Carried) -> Result<Removal, String> {
    let doomed: Vec<i64> = conn
        .prepare(DOOMED_WORKS)
        .and_then(|mut stmt| stmt.query_map([id], |r| r.get(0))?.collect())
        .map_err(|e| format!("reading the works root {id} alone accounts for: {e}"))?;
    let positions: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM progress WHERE work_id IN ({DOOMED_WORKS})"),
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("counting the reading positions root {id} would take: {e}"))?;
    let notes: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM note WHERE work_id IN ({DOOMED_WORKS})"),
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("counting the notes root {id} would take: {e}"))?;

    // A work this deletes keeps its document where it is, unlisted, as when its
    // one directory goes: a library removal's earlier directory may have noted
    // it while a copy here kept it alive.
    for work in &doomed {
        carried.remove(work);
    }
    conn.execute(
        &format!("DELETE FROM work WHERE id IN ({DOOMED_WORKS})"),
        [id],
    )
    .map_err(|e| format!("deleting the works root {id} alone accounts for: {e}"))?;
    // Read before any member of `id` is deleted, re-pointed or left with no
    // directory: each can move a surviving work's address, and afterwards
    // nothing names the one its document is at.
    let touched: Vec<i64> = conn
        .prepare(
            "SELECT DISTINCT e.work_id FROM member m
               JOIN edition e ON e.id = m.edition_id
              WHERE m.root_id = ?1",
        )
        .and_then(|mut stmt| stmt.query_map([id], |r| r.get(0))?.collect())
        .map_err(|e| format!("reading the works root {id} holds members of: {e}"))?;
    note_addresses(conn, touched, carried)?;
    conn.execute(
        "DELETE FROM member
          WHERE id IN (SELECT member_id FROM member_root WHERE root_id = ?1)
            AND NOT EXISTS (SELECT 1 FROM member_root other
                             WHERE other.member_id = member.id AND other.root_id != ?1)",
        [id],
    )
    .map_err(|e| {
        format!("clearing what root {id} alone still held on a work that survives it: {e}")
    })?;
    repoint_kept_members(conn, id)?;
    conn.execute("DELETE FROM root WHERE id = ?1", [id])
        .map_err(|e| format!("removing root {id}: {e}"))?;

    Ok(Removal {
        works: doomed.len(),
        positions: usize::try_from(positions).unwrap_or(usize::MAX),
        notes: usize::try_from(notes).unwrap_or(usize::MAX),
        removed: vec![id],
        ..Removal::default()
    })
}

/// Re-point each member whose `root_id` is `id` at a copy another directory
/// holds: the oldest such directory's entry of the same `sha256`, lowest
/// `rel_path` first, or nowhere where none holds one. A member left at NULL
/// keeps its work from being superseded (`derive::superseded`'s condition 4).
/// Re-pointed, the old work is folded into the work the copy's rules file it
/// in, carrying its readers' rows, where the derive writes none of its
/// editions, every member left lands on that one work, and `fold::absorb`
/// finds there the same archives in the same order; else both stay (O243).
fn repoint_kept_members(conn: &Connection, id: i64) -> Result<(), String> {
    let mut statement = conn
        .prepare(
            "SELECT m.id, r.id, r.path, se.rel_path
               FROM member m
               JOIN member_root mr ON mr.member_id = m.id AND mr.root_id <> ?1
               JOIN root r ON r.id = mr.root_id
               JOIN scan_entry se ON se.root_id = r.id AND se.sha256 = m.sha256
              WHERE m.root_id = ?1
              ORDER BY m.id, r.id, se.rel_path",
        )
        .map_err(|e| format!("reading the copies of what root {id} held: {e}"))?;
    let copies = statement
        .query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|e| format!("reading the copies of what root {id} held: {e}"))?;
    let mut repointed = HashSet::new();
    for (member_id, root_id, root_path, rel_path) in copies {
        if !repointed.insert(member_id) {
            continue;
        }
        // Joined as `derive::classify` joins a candidate's path, so the
        // derive matches it to the copy's entry.
        let path = Path::new(&root_path)
            .join(&rel_path)
            .to_string_lossy()
            .into_owned();
        conn.execute(
            "UPDATE member SET root_id = ?1, path = ?2 WHERE id = ?3",
            rusqlite::params![root_id, path, member_id],
        )
        .map_err(|e| format!("re-pointing member {member_id} at root {root_id}: {e}"))?;
    }
    Ok(())
}

/// Every library root `id` is the last directory of, which removing it
/// deletes, ascending by id. Empty where each library holding it keeps another
/// directory, and where `id` names no root. A directory two libraries hold can
/// be the last of both.
fn emptied_by_removing(conn: &Connection, id: i64) -> Result<Vec<i64>, String> {
    let failed =
        |e: rusqlite::Error| format!("checking which libraries root {id} is the last of: {e}");
    conn.prepare(
        "SELECT lr.library_id FROM library_root lr
          WHERE lr.root_id = ?1
            AND NOT EXISTS (SELECT 1 FROM library_root other
                             WHERE other.library_id = lr.library_id AND other.root_id <> ?1)
          ORDER BY lr.library_id",
    )
    .map_err(failed)?
    .query_map([id], |r| r.get::<_, i64>(0))
    .map_err(failed)?
    .collect::<Result<_, _>>()
    .map_err(failed)
}

/// Delete root `id`, every work that lived entirely under it, and re-derive
/// — the mechanism [`removal_cost`] runs and rolls back, and [`remove_root`]
/// runs and commits.
///
/// **Whoever is left with no library is read before anything is deleted**,
/// where `id` is the last directory of a library or of several: each goes with
/// that directory, and its grants with it, so afterwards nothing records who
/// held them.
///
/// **Delete-plus-derive, not a bare `DELETE FROM root`.** `scan_entry`,
/// `dir_state`, `rule` and its `library_root` links cascade with the root, and
/// a library left with no link goes with it (migration 19's
/// `library_emptied_by_unlink`), taking its `user_library` grants
/// (`schema/003_scanner.sql`, `schema/019_library_links.sql`), but `work`, `edition`,
/// `member`, `progress` and `note` do not follow — `progress` and `note` are
/// keyed on `work_id` and carry no root at all, and `member.root_id` is `ON
/// DELETE SET NULL`, not `CASCADE`, precisely so an ordinary rescan finding a
/// root briefly missing cannot delete a library's pages
/// (`schema/006_member_root.sql`). A bare delete would leave every work,
/// edition, member, note and reading position standing with `member.root_id`
/// quietly `NULL` and no `member_root` row — invisible to every grant (O54) —
/// asserting evidence that is gone. This function does the rest of the job:
/// [`DOOMED_WORKS`] names what `?1` alone accounts for, deleted explicitly and
/// before the `root` row goes, and `derive_with` re-derives everything else —
/// `series`, `kept_editions` on a work that only partly depended on `?1`, and
/// `crate::proposals::raise` for whatever the loss changes.
///
/// **Always derives under [`crate::derive::Documents::Skip`], never
/// `Reconcile`.** This function's own writes run inside a transaction that
/// belongs to its caller — committed by [`remove_root`], rolled back by
/// [`removal_cost`] — and `Reconcile` writes and renames hand-metadata
/// documents under `<root>/.taiyaku/` while that transaction is still open.
/// `write_work`'s own comment on the rename says what that risks: "A rollback
/// that left the file renamed would strand it under a key the database never
/// adopted." [`remove_root`] runs the real, `Reconcile`-mode pass itself, after
/// its own transaction commits — see that function's own doc.
///
/// # Errors
///
/// Returns an error if a read, [`hand_back`], [`take_root`] or a derive fails.
fn apply_removal(
    conn: &Connection,
    id: i64,
    gone: &HashSet<String>,
) -> Result<(Removal, crate::derive::DeriveReport, Carried), String> {
    let emptied = emptied_by_removing(conn, id)?;
    let left_with_no_library = only_grant_holders(conn, &emptied)?;
    let libraries_removed = names_of(conn, &emptied)?;
    let before = crate::state::resolving(conn)?;
    // Spec §4: the directories around and inside `id` keep the files they
    // would walk, so `take_root` counts only what none of them holds, and the
    // rehearsal, which runs this function too, counts what the removal does.
    let mut carried = Carried::new();
    hand_back(conn, id, gone, &mut carried)?;
    let mut removal = take_root(conn, id, &mut carried)?;
    removal.left_with_no_library = left_with_no_library;
    removal.libraries_removed = libraries_removed;
    let derived = crate::derive::derive_with(conn, crate::derive::Documents::Skip)?;
    (removal.positions_page_gone, removal.notes_page_gone) =
        crate::state::resolving(conn)?.lost_since(&before);

    Ok((removal, derived, carried))
}

/// What removing root `id` would cost, the pages it takes from works it keeps
/// included, found by doing it inside a transaction this rolls back.
///
/// [`crate::preview::preview`]'s own shape, reused rather than a second
/// mechanism for "what would this change do": open a transaction, make the
/// change, derive for real, read the result, roll back. [`apply_removal`]
/// always derives under [`crate::derive::Documents::Skip`], for
/// [`crate::preview::preview`]'s own reason — a real derivation writes and
/// renames hand-metadata documents under `<root>/.taiyaku/`, and a `ROLLBACK`
/// does not undo a filesystem write. A cost estimate that renamed files in
/// someone's library would be worse than the bug it exists to prevent.
///
/// **The [`crate::derive::DeriveReport`] returned alongside [`Removal`] is a
/// prediction, not a promise.** It is what `Skip`-mode derivation found while
/// this transaction was still open, rolled back the same as everything else
/// this function touched — so a caller can show "this removal would also
/// leave 1 hand document unreadable" before anyone commits to it, but the
/// fields only [`crate::derive::Documents::Reconcile`] fills in
/// (`repaired_documents`, `orphan_documents`, and the rest — see
/// [`remove_root`]'s own doc) are never populated here, because this pass
/// never runs that far.
///
/// **[`crate::Nested`], not `unchecked_transaction` directly**, because a
/// caller may already hold a transaction, and a plain `unchecked_transaction`
/// opened inside one is refused, as `Nested`'s own doc shows under the
/// `BEGIN IMMEDIATE` that `preview::preview` wraps a whole `derive_with` pass
/// in. Rolled back rather than committed by simply never calling
/// [`crate::Nested::commit`] and letting this scope drop: `Nested`'s own
/// `Drop` then undoes exactly what `apply_removal` just did, a real
/// `ROLLBACK` if nothing else was open or a `ROLLBACK TO SAVEPOINT` folded
/// back into a caller's own already-open transaction if something was — see
/// that type's own doc. Either way this function takes nothing, whether it is
/// called standalone or nested inside another transaction.
///
/// # Errors
///
/// Returns an error if SQLite refuses the transaction or the reads before it,
/// or if [`apply_removal`] fails.
pub fn removal_cost(
    conn: &Connection,
    id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let gone = gone_from_disk(conn, |root| root.id == id)?;
    let tx = crate::Nested::open(conn, "removal_cost")
        .map_err(|e| format!("starting the removal-cost transaction: {e}"))?;
    let (removal, derived, _carried) = apply_removal(&tx, id, &gone)?;
    // Deliberately not `tx.commit()` — see this function's own doc for why
    // dropping `tx` here, uncommitted, is the rollback.
    drop(tx);
    Ok((removal, derived))
}

/// Remove root `id`: delete it and every work that lived entirely under it,
/// re-derive, and reconcile `<root>/.taiyaku/` on every remaining root, as
/// [`fn@crate::derive::derive`] does. Pages taken are counted as in a dry run.
///
/// **Two derive passes, not one.** [`apply_removal`] always derives under
/// [`crate::derive::Documents::Skip`] (see that function's own doc), so the
/// deletes and the SQL-only re-derivation they need commit as one atomic unit
/// with nothing written to disk while that transaction is open. The real,
/// `Documents::Reconcile` pass — the one that writes and renames hand-metadata
/// documents — runs here, second, on `conn` directly, only *after* the
/// transaction above has committed. Twice over unchanged facts `derive_with`
/// "writes the same rows a second time rather than a second set of rows", so
/// the rows are right; but this pass counts 0 for an event the first made, a
/// rekey say, and the answer takes only the marks from the first (O271). A
/// `Reconcile` pass inside the transaction `Nested` opens below would rename
/// documents while that transaction is open, and `write_work`'s own comment on
/// the rename says what that risks: "A rollback that left the file renamed
/// would strand it under a key the database never adopted."
///
/// The returned [`crate::derive::DeriveReport`] is this second, real pass's
/// own — the one whose findings actually reached the library, unlike
/// [`apply_removal`]'s own `Skip`-mode report from inside the transaction,
/// which this function does not return: every field the read-back into
/// `librarydoc::apply` can populate (`unreadable_hand` among them) does not
/// depend on `documents` and so agrees between the two passes; every field
/// that does depend on it — `repaired_documents`, `unwritable_documents`,
/// `orphan_documents`, `declined_documents`, `stale_documents` — is `Skip`'s
/// own report, and reporting it dropped and reporting nothing would say
/// the removal reconciled documents. Its `orphan_documents` ends with each
/// document the carry left in a `.taiyaku` no root holds now, which no
/// `reconcile` reads; its marks count the first pass's ([`with_marks_of`]).
///
/// **[`crate::Nested`], not `unchecked_transaction` directly** — see
/// [`removal_cost`]'s own doc for why.
///
/// # Errors
///
/// Returns an error if SQLite refuses the transaction or the reads before it,
/// if [`apply_removal`] fails, if the commit is refused, or if the post-commit
/// derive fails — in which case the removal itself has already committed and
/// only the reconciliation pass over its aftermath did not complete. Likewise
/// if SQLite fails, after the commit, while carrying the hand documents of the
/// works whose members it handed back, deleted or re-pointed: the removal has
/// committed, the documents not yet carried stay where they were, and the
/// reconciling derive is not run.
pub fn remove_root(
    conn: &Connection,
    id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let gone = gone_from_disk(conn, |root| root.id == id)?;
    let tx = crate::Nested::open(conn, "remove_root")
        .map_err(|e| format!("starting the root-removal transaction: {e}"))?;
    let (removal, within, carried) = apply_removal(&tx, id, &gone)?;
    tx.commit()
        .map_err(|e| format!("committing the removal of root {id}: {e}"))?;
    let left = carry_documents(conn, &carried)
        .map_err(|e| format!("moving hand documents after removing root {id}: {e}"))?;
    let mut derived = crate::derive::derive(conn)
        .map_err(|e| format!("reconciling documents after removing root {id}: {e}"))?;
    derived.orphan_documents.extend(left);
    Ok((removal, with_marks_of(derived, &within)))
}

/// `after`, a removal's reconciling derive, counting the works `within`, the
/// derive inside its transaction, marked missing and found again. That pass
/// made the marks, so `after` finds them made and counts none: answered alone,
/// it would tell the administrator that nothing went missing while the
/// removal hid works, and contradict the dry run, which answers `within`.
fn with_marks_of(
    mut after: crate::derive::DeriveReport,
    within: &crate::derive::DeriveReport,
) -> crate::derive::DeriveReport {
    after.marked_missing += within.marked_missing;
    after.found_again += within.found_again;
    after
}

/// The name of each library of `ids`, in order: read inside a removal's
/// transaction before the libraries go, and a fault, never a gap, where one
/// is missing.
fn names_of(conn: &Connection, ids: &[i64]) -> Result<Vec<String>, String> {
    ids.iter()
        .map(|id| {
            crate::libraries::name_of(conn, *id)?
                .ok_or_else(|| format!("library {id} is linked and has no row"))
        })
        .collect()
}

/// Remove each directory `library_id` alone holds, summing what each takes,
/// then unlink those another library holds too, counted nothing and named in
/// [`Removal::kept`] (overlapping-libraries spec §5), and derive once where a
/// directory went: `None` where none did, and nothing was derived.
///
/// **One directory after another in the caller's transaction, never one dry run
/// each**: a work with an edition under each of two directories is in
/// [`DOOMED_WORKS`] for neither while both stand. Who is left with no library,
/// and every name this answers, are read before anything goes.
fn apply_library_removal(
    conn: &Connection,
    library_id: i64,
    gone: &HashSet<String>,
) -> Result<(Removal, Option<crate::derive::DeriveReport>, Carried), String> {
    let (shared, alone): (Vec<Root>, Vec<Root>) = roots(conn)?
        .into_iter()
        .filter(|root| root.libraries.contains(&library_id))
        .partition(|root| root.libraries.len() > 1);
    // A library with no directory goes only with its last one, so it stays,
    // and a directory it alone holds empties no other library.
    let emptied = if shared.is_empty() && alone.is_empty() {
        Vec::new()
    } else {
        vec![library_id]
    };
    let mut total = Removal {
        left_with_no_library: only_grant_holders(conn, &emptied)?,
        libraries_removed: names_of(conn, &emptied)?,
        ..Removal::default()
    };
    for dir in &shared {
        let others: Vec<i64> = dir
            .libraries
            .iter()
            .copied()
            .filter(|library| *library != library_id)
            .collect();
        total.kept.push(Kept {
            root_id: dir.id,
            path: dir.path.clone(),
            libraries: names_of(conn, &others)?,
        });
    }
    // Only a directory's removal can take a page: an unlink derives nothing.
    let before = if alone.is_empty() {
        None
    } else {
        Some(crate::state::resolving(conn)?)
    };
    let mut carried = Carried::new();
    for dir in alone {
        // As in `apply_removal`. Directories of one library never nest, so a
        // directory given rows is another library's, not removed later here.
        hand_back(conn, dir.id, gone, &mut carried)?;
        let taken = take_root(conn, dir.id, &mut carried)?;
        total.works += taken.works;
        total.positions += taken.positions;
        total.notes += taken.notes;
        total.removed.extend(taken.removed);
    }
    for dir in shared {
        conn.execute(
            "DELETE FROM library_root WHERE library_id = ?1 AND root_id = ?2",
            [library_id, dir.id],
        )
        .map_err(|e| format!("unlinking root {} from library {library_id}: {e}", dir.id))?;
    }
    // A derive reads no link or grant, so a removal that only unlinks changes
    // nothing a derive or its reconcile pass reads.
    let derived = match before {
        Some(before) => {
            let derived = crate::derive::derive_with(conn, crate::derive::Documents::Skip)?;
            (total.positions_page_gone, total.notes_page_gone) =
                crate::state::resolving(conn)?.lost_since(&before);
            Some(derived)
        }
        None => None,
    };
    Ok((total, derived, carried))
}

/// What removing library `library_id` would cost, the pages it takes from
/// works it keeps included, found by doing it inside a transaction this rolls
/// back: [`removal_cost`], for a library.
///
/// # Errors
///
/// As [`removal_cost`].
pub fn library_removal_cost(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let gone = gone_from_disk(conn, |root| root.libraries == [library_id])?;
    let tx = crate::Nested::open(conn, "library_removal_cost")
        .map_err(|e| format!("starting the library-removal-cost transaction: {e}"))?;
    let (removal, derived, _carried) = apply_library_removal(&tx, library_id, &gone)?;
    // Not committed: dropping `tx` is the rollback, as in `removal_cost`.
    drop(tx);
    Ok((removal, derived.unwrap_or_default()))
}

/// Remove library `library_id`: each directory it alone holds and the works
/// they alone accounted for, its links to the directories another library
/// holds too, and the library, which migration 19 deletes with its last link.
/// Where a directory went, derive again with documents reconciled, after the
/// commit, for [`remove_root`]'s reason, end its `orphan_documents` with what
/// the carry left and count the first pass's marks, as that does; where none
/// did, the report is empty. Its pages taken are counted as the dry run's are.
///
/// # Errors
///
/// As [`remove_root`], the carry after the commit included: the library is
/// already removed when it fails, and the reconciling derive is not run.
pub fn remove_library(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let gone = gone_from_disk(conn, |root| root.libraries == [library_id])?;
    let tx = crate::Nested::open(conn, "remove_library")
        .map_err(|e| format!("starting the library-removal transaction: {e}"))?;
    let (removal, within, carried) = apply_library_removal(&tx, library_id, &gone)?;
    tx.commit()
        .map_err(|e| format!("committing the removal of library {library_id}: {e}"))?;
    let Some(within) = within else {
        return Ok((removal, crate::derive::DeriveReport::default()));
    };
    let left = carry_documents(conn, &carried)
        .map_err(|e| format!("moving hand documents after removing library {library_id}: {e}"))?;
    let mut derived = crate::derive::derive(conn)
        .map_err(|e| format!("reconciling documents after removing library {library_id}: {e}"))?;
    derived.orphan_documents.extend(left);
    Ok((removal, with_marks_of(derived, &within)))
}

// --- exclusion -------------------------------------------------------------

/// Why a root's walk does not reach a path ([`Excluder::visits`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeptOut {
    /// A name on the way starts with `.`, which the walk always skips.
    DotName,
    /// One of the root's exclude patterns names it or a directory above it.
    Exclusion,
}

/// The patterns one root excludes from its own walk, compiled once per scan.
///
/// **gitignore's language, from `ignore`, not a glob matcher of this
/// module's own.** `root.exclude_globs` holds one pattern per line, and a
/// line means exactly what the same line in a `.gitignore` file means:
/// negation with `!`, anchoring with a leading `/`, a directory only with a
/// trailing `/`, `**` across separators, `#` comments and blank lines
/// ignored. None of that is written here, which is the point — `rules.rs`
/// refuses "a second, drifting reimplementation" for the same reason.
///
/// **Building never fails.** A line that will not compile is dropped and
/// named in [`Self::complaints`], which [`ScanReport::bad_exclude_patterns`]
/// carries out to whoever asked for the scan. That is `rules.rs`'s rule
/// again: a malformed pattern disables its own rule and nothing else,
/// because a scan of a real library must not die over one bad line. The
/// column predates any validation — migration 1 — so a row can already hold
/// anything, and treating a build failure as impossible would make it a
/// crash rather than a complaint.
pub(crate) struct Excluder {
    gitignore: ignore::gitignore::Gitignore,
    /// One message per line that would not compile, naming the line.
    complaints: Vec<String>,
}

impl Excluder {
    /// Compile `patterns`, one per line. `None` and `""` both mean
    /// "exclude nothing".
    pub(crate) fn build(patterns: Option<&str>) -> Self {
        let mut builder = ignore::gitignore::GitignoreBuilder::new("");
        let mut complaints = Vec::new();
        for line in patterns.unwrap_or_default().lines() {
            if let Err(why) = builder.add_line(None, line) {
                complaints.push(format!("`{}`: {why}", line.trim()));
            }
        }
        // `build` cannot fail once `add_line` has accepted every line it
        // kept. Handled rather than unwrapped anyway: this module's rule is
        // that a pattern problem is a complaint, never a panic.
        let gitignore = builder.build().unwrap_or_else(|why| {
            complaints.push(format!(
                "compiling this directory's exclude patterns: {why}"
            ));
            ignore::gitignore::Gitignore::empty()
        });
        Self {
            gitignore,
            complaints,
        }
    }

    /// Whether `rel_path` itself is excluded. The walk's question, asked
    /// about a child before it is counted or descended into.
    fn excludes(&self, rel_path: &str, is_dir: bool) -> bool {
        matches!(
            self.gitignore.matched(rel_path, is_dir),
            ignore::Match::Ignore(_)
        )
    }

    /// Whether this root's walk would reach `rel` (relative to the root,
    /// `/`-separated): no name starts with `.` (`measure_dir`'s rule), and
    /// [`Self::excludes`] answers false for each ancestor prefix, outermost
    /// first, and for `rel` itself with `is_dir`.
    ///
    /// **The walk's own question, top-down**, so [`crate::reach`] and the walk
    /// agree: [`Self::excludes_with_parents`] would let through a whitelisted
    /// child of an excluded parent, which the walk never descends to.
    pub(crate) fn visits(&self, rel: &str, is_dir: bool) -> bool {
        self.keeps_out(rel, is_dir).is_none()
    }

    /// Whether this root's walk reaches a member at `rel`. `member.format` is
    /// never written, so its path is asked as a file and as a directory: a
    /// pattern of either kind may have kept a walk out. A removal's hand-back
    /// and the grant (`browse::member_under`) both ask it, so they agree on
    /// which walk could have recorded a member.
    pub(crate) fn reaches_member(&self, rel: &str) -> bool {
        self.visits(rel, false) && self.visits(rel, true)
    }

    /// Why [`Self::visits`] answers false for `rel`: the first of its two
    /// rules to stop the walk, asked in the walk's own order.
    pub(crate) fn keeps_out(&self, rel: &str, is_dir: bool) -> Option<KeptOut> {
        let mut prefix = String::new();
        let mut names = rel.split('/').peekable();
        while let Some(name) = names.next() {
            if name.starts_with('.') {
                return Some(KeptOut::DotName);
            }
            prefix = join_rel(&prefix, name);
            let last = names.peek().is_none();
            if self.excludes(&prefix, !last || is_dir) {
                return Some(KeptOut::Exclusion);
            }
        }
        None
    }

    /// Whether `rel_path` or any directory above it is excluded —
    /// [`forget_unseen`]'s question, since a row under a directory the walk
    /// never descended into is stale for a reason the row's own path does
    /// not state.
    ///
    /// **Fractionally more permissive than git**, and it is worth naming:
    /// this tests the path itself before its parents, so a whitelisted child
    /// of an excluded parent (`!a/extras/keep` under an excluded `extras`)
    /// answers false here while the walk, correctly, never reaches it. It
    /// decides only which of two counters a number lands in, never whether a
    /// row is deleted.
    fn excludes_with_parents(&self, rel_path: &str, is_dir: bool) -> bool {
        matches!(
            self.gitignore.matched_path_or_any_parents(rel_path, is_dir),
            ignore::Match::Ignore(_)
        )
    }
}

/// Whether every line of `patterns` is a pattern this library can compile.
///
/// **`taiyaku-serve`'s `PATCH /api/library/roots/{id}` refuses on this**, so an
/// admin hears about a typo at save rather than after a scan. A pattern already
/// stored that will not build is a different matter and is not this function's
/// business — see [`Excluder::build`], which drops it and complains.
///
/// **What this actually catches is narrow, and callers must not imply
/// otherwise.** With `ignore` 0.4.33, `add_line` rejects a dangling `\` and an
/// unclosed `{`. It accepts `a[`, `[a-`, `[[:bogus:]]`, `[]`, `***` and `a**b`.
/// This is a guard against two specific typos, not a check that a pattern means
/// what its author meant.
///
/// # Errors
///
/// Returns a message naming the offending line, ready to be handed to a
/// person, if any line will not compile.
pub fn check_exclude_globs(patterns: &str) -> Result<(), String> {
    let mut builder = ignore::gitignore::GitignoreBuilder::new("");
    for line in patterns.lines() {
        builder.add_line(None, line).map_err(|why| {
            format!(
                "the directory's `exclude_globs` line `{}` is not a usable pattern: {why}",
                line.trim()
            )
        })?;
    }
    builder
        .build()
        .map(|_| ())
        .map_err(|why| format!("the directory's `exclude_globs` will not compile: {why}"))
}

// --- the scan --------------------------------------------------------------

/// What one [`scan_root`] call found and did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    pub root: PathBuf,
    /// Every directory on disk the walk recorded (`Kind::Dir` entries), the
    /// root's own included: the file-system sense, not the `root`-row sense
    /// `dirs` has elsewhere in this module.
    pub dirs: usize,
    pub files: usize,
    pub added: usize,
    pub updated: usize,
    pub gone: usize,
    pub refused: Option<String>,
    /// Directories beneath the root that could not be listed, one message per
    /// directory — `std::fs::read_dir`'s own refusal, naming the path.
    ///
    /// **This module's principle — one unreadable file must not leave a library
    /// unscanned — applies to a directory too.** A `lost+found` directory under
    /// a freshly mounted share is `0700`, owned by root, so a non-root scan can
    /// never list it. Such a directory is skipped, counted and named here — see
    /// [`measure_dir`] — so `Err` from [`scan_root`] is reserved for the root's
    /// own directory refusing to list, which has nothing above it to skip to.
    pub unreadable_dirs: Vec<String>,
    /// Directories a pattern in `root.exclude_globs` named, which this walk
    /// therefore never descended into — no `read_dir`, no `stat`, no
    /// `sniff`, no `member_pages` for anything beneath them.
    ///
    /// **C4 §3's fourth confession.** That section requires a scan to report
    /// "directories skipped by `exclude_globs`" alongside the containers it
    /// could not open and the formats it cannot read (O92).
    pub excluded_dirs: Vec<String>,
    /// Files a pattern named, counted rather than listed.
    ///
    /// **A count where [`Self::excluded_dirs`] is a list, deliberately.** A
    /// library excludes a handful of subtrees and may exclude thousands of
    /// `*.nfo` files; C4 §3.4 requires this report to render whole, and a
    /// list of thousands cannot. Page images are included: a pattern that
    /// excludes one reduces its directory's page total, which is what
    /// excluding it means.
    pub excluded_files: usize,
    /// Lines of `root.exclude_globs` that would not compile, each naming
    /// itself.
    ///
    /// A malformed pattern disables its own line and nothing else —
    /// `rules.rs`'s rule, because a scan of a real library must not die over
    /// one bad line, and migration 1's column predates any validation.
    pub bad_exclude_patterns: Vec<String>,
    /// Rows deleted from `scan_entry` because a pattern now covers them,
    /// rather than because the file disappeared.
    ///
    /// **Split out of [`Self::gone`] rather than folded into it.** `gone`
    /// means the file is no longer on disk. "You excluded it" and "it
    /// disappeared" are different facts, and not conflating those is the
    /// whole of this report's discipline — an admin who adds a pattern and
    /// watches `gone` jump has been told the wrong thing.
    ///
    /// Nothing a work, a position or a note needs is destroyed by this: nothing
    /// references `scan_entry`, and `derive` drops a work only to fold it. A
    /// nested directory's row, counted too, goes first with its member to the
    /// directory around that reaches the file (decision 21). A work no
    /// directory holds is marked missing until a scan records the file (O92).
    pub forgotten_by_exclusion: usize,
    /// Rows taken from the directories around this one: their `scan_entry`
    /// rows at or under it that this scan recorded too (spec §4: each file is
    /// scanned once, under the deepest directory that holds it).
    ///
    /// **Never part of [`Self::gone`]**: the files are still on disk, and the
    /// members under them move with the rows. A row around this one that this
    /// scan did not record is counted `gone`, or forgotten where a pattern here
    /// covers it, unless the directory around this one reaches its file: that
    /// one then holds the row and it counts nowhere (decision 21).
    pub taken_over: usize,
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
            excluded_dirs: Vec::new(),
            excluded_files: 0,
            bad_exclude_patterns: Vec::new(),
            forgotten_by_exclusion: 0,
            taken_over: 0,
        }
    }
}

/// Walk `root` and bring `scan_entry` and `dir_state` up to date with what is
/// on disk, then record how it went on `root` itself — `last_scan_at` and
/// `last_scan_status`, whichever of the three outcomes below actually
/// happened.
///
/// Nothing about a series, a work or an edition is decided here — see the
/// module doc. A root that scanned empty while the library already holds
/// entries for it is refused rather than applied (§8.1); the returned report
/// says so through `refused` and the database is left exactly as it was.
///
/// **`last_scan_at`/`last_scan_status` are written here, on every path out —
/// including the refusal — because this is the one function `taiyaku-cli`'s
/// `library scan` and `taiyaku-serve`'s `POST /api/library/roots/{id}/scan`
/// both call.** Putting the write inside this function rather than in either
/// caller is what keeps the CLI and the route from ever disagreeing about
/// what "the last scan" means for a root — there is exactly one place this
/// fact is decided, and both front ends read the same row afterward.
/// `last_scan_at` is `datetime('now')`, SQLite's own clock rather than this
/// process's, so a scan run from a container with a skewed clock still agrees
/// with every other timestamp the same database already keeps that way.
/// `last_scan_status` is one of three words here: `"ok"` (recorded and
/// applied), `"refused"` (§8.1 — nothing was applied, and this call still says
/// so rather than leaving the column to read as "never scanned"), or
/// `"failed"` (an `Err` below — a permission refusal on the root's own
/// directory, a symlink cycle past [`MAX_DEPTH`], or a database statement
/// failing outright). `taiyaku-serve`'s scan job adds `"waiting"` and
/// `"deriving"`, and its start `"interrupted"`, through the functions beside
/// this one (O51, O186). The write is a separate statement, after the scan's
/// own transaction has committed or rolled back, so a scan that rolled back
/// still leaves a truthful `"failed"`.
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
/// for a container that will not open. Every one of these failures is still
/// recorded as `last_scan_status = 'failed'` before it propagates — the one
/// case this function tries hardest not to leave silent, since it is the
/// one nobody chose.
///
/// It also returns an error if SQLite fails while the hand documents of the
/// works taken over are carried after the commit. The walk's rows have then
/// committed, and the documents not yet carried stay where they were.
///
/// **Nothing is recorded for a root that is no longer the one walked.** A
/// root can be removed while its walk runs, and its id given to a root added
/// after it. The walk's entries are written only where, inside the
/// transaction that writes them, `root.id` still names `root.path`, and the
/// status write here asks the same, so neither lands on another root.
pub fn scan_root(conn: &Connection, root: &Root) -> Result<ScanReport, String> {
    // A pass that walks again follows another writer's commit to this
    // directory's entries, so the loop ends once they stop moving.
    let result = loop {
        if let Some(walked) = scan_root_inner(conn, root).transpose() {
            break walked;
        }
    };
    let status = match &result {
        Ok(report) if report.refused.is_some() => "refused",
        Ok(_) => "ok",
        Err(_) => "failed",
    };
    // Best-effort in the sense that a failure here is not folded into
    // `result` — a scan that genuinely succeeded (or was genuinely refused)
    // must not be reported as failed merely because this bookkeeping write
    // could not land, and a scan that already failed has nothing worse to
    // become. In practice this statement fails only when `result` itself
    // already carries a store fault, since both run against the same
    // connection.
    let _ = conn.execute(
        "UPDATE root SET last_scan_at = datetime('now'), last_scan_status = ?2 \
         WHERE id = ?1 AND path = ?3",
        rusqlite::params![root.id, status, root.path.to_string_lossy()],
    );
    result
}

/// Correct `last_scan_status` from `"deriving"` or `"waiting"` to `"failed"`
/// where the derive after a walk failed; a refused or failed walk keeps its word (O186).
///
/// **The one caller this exists for**: `taiyaku-serve`'s scan job runs
/// [`scan_root`] on each directory it was given and then
/// [`fn@crate::derive::derive`] once, and folds both into the one report
/// C4 §3's screen shows. [`scan_root`] itself only ever sees the walk — it
/// writes `"ok"` the moment that commits, before the derive half of the job
/// has even started — and the job then marks it `"deriving"`: a derive that
/// fails would leave that standing, though the library the walk describes may
/// be only partly derived. `taiyaku-cli`'s own `library scan` does not call
/// it: an `Err` from its derive fails the whole command.
///
/// Kept here, not in `taiyaku-serve`, for the reason [`scan_root`]'s own doc
/// comment states about `last_scan_at`/`last_scan_status`: there is exactly
/// one module that writes either column, so nothing outside it can disagree
/// about what they mean.
///
/// Best-effort, [`scan_root`]'s own way: a caller correcting a status after
/// its own derive already failed has a real error to report regardless, and
/// this write failing too must not replace that error with a bookkeeping one.
///
/// # Errors
///
/// Returns an error only if the `UPDATE` itself is refused.
pub fn mark_derive_failed(conn: &Connection, root: &Root) -> Result<(), String> {
    conn.execute(
        "UPDATE root SET last_scan_status = 'failed' \
         WHERE id = ?1 AND path = ?2 AND last_scan_status IN ('deriving', 'waiting')",
        rusqlite::params![root.id, root.path.to_string_lossy()],
    )
    .map_err(|e| {
        format!(
            "recording that the post-scan derive of directory {} failed: {e}",
            root.path.display()
        )
    })?;
    Ok(())
}

/// Mark `root` `"deriving"` where its walk has just written `"ok"`, until
/// [`mark_derived`] or [`mark_derive_failed`] ends it (O186). A stop between
/// leaves `"deriving"` for the server's next start to rewrite
/// ([`interrupt_derives`]). The CLI does not call it (spec §4, Decision 10).
///
/// Only over `"ok"`, so a refused walk keeps `"refused"`, and only where
/// `root.id` still names `root.path`, as [`scan_root`] writes.
///
/// # Errors
///
/// If the `UPDATE` is refused.
pub fn mark_deriving(conn: &Connection, root: &Root) -> Result<(), String> {
    conn.execute(
        "UPDATE root SET last_scan_status = 'deriving' \
         WHERE id = ?1 AND path = ?2 AND last_scan_status = 'ok'",
        rusqlite::params![root.id, root.path.to_string_lossy()],
    )
    .map_err(|e| {
        format!(
            "recording that directory {} is being derived: {e}",
            root.path.display()
        )
    })?;
    Ok(())
}

/// Mark each of `roots` `"ok"` again where it reads `"deriving"` or
/// `"waiting"`, once the derive after their walks has returned (O186). After
/// that one derive a `"waiting"` can only be one of the job's own marks that
/// no later write replaced (O51).
///
/// A root that reads anything else is left as it reads: a refused walk keeps
/// `"refused"`. Each write asks [`mark_deriving`]'s id-and-path question.
///
/// # Errors
///
/// Returns the first refused `UPDATE`'s error. The roots after it are still
/// marked: one refusal must not leave them `"deriving"`, which the next start
/// would rewrite to `"interrupted"` though their derive finished.
pub fn mark_derived(conn: &Connection, roots: &[Root]) -> Result<(), String> {
    rewrite_each(conn, roots, &["deriving", "waiting"], "ok", "was derived")
}

/// Mark each of `roots` `"waiting"` where it reads `"deriving"`: its derive
/// waits for the derive lease, which another derive holds (O51).
/// [`mark_waited`] marks it `"deriving"` again once the lease is held.
///
/// # Errors
///
/// As [`mark_derived`].
pub fn mark_waiting(conn: &Connection, roots: &[Root]) -> Result<(), String> {
    rewrite_each(
        conn,
        roots,
        &["deriving"],
        "waiting",
        "waits for another derive",
    )
}

/// Mark each of `roots` `"deriving"` again where it reads `"waiting"`, once
/// its derive holds the lease (O51). A walk that reads anything else, a
/// `"refused"` or another scan's `"ok"`, keeps it.
///
/// # Errors
///
/// As [`mark_derived`].
pub fn mark_waited(conn: &Connection, roots: &[Root]) -> Result<(), String> {
    rewrite_each(conn, roots, &["waiting"], "deriving", "is being derived")
}

/// A directory [`mark_queued`] marked `"waiting"`, with the word it read
/// before, which [`unmark_queued`] writes back.
#[derive(Debug)]
pub struct Queued {
    root: Root,
    before: Option<String>,
}

/// Mark each of `roots` `"waiting"`: its scan has not started, because
/// another job in the server holds the job lock (O51). Answers the
/// directories marked, each with its word before, for [`unmark_queued`].
///
/// Each write asks that the word is still the one just read, and
/// [`mark_deriving`]'s id-and-path question, so a word another writer
/// changed meanwhile is left as it reads, and not written back later.
///
/// Best-effort, as every status write of the job is: a directory whose read
/// or write is refused is left as it reads and is not answered.
pub fn mark_queued(conn: &Connection, roots: &[Root]) -> Vec<Queued> {
    roots
        .iter()
        .filter_map(|root| {
            let path = root.path.to_string_lossy();
            let before: Option<String> = conn
                .query_row(
                    "SELECT last_scan_status FROM root WHERE id = ?1 AND path = ?2",
                    rusqlite::params![root.id, path],
                    |r| r.get(0),
                )
                .ok()?;
            let marked = conn
                .execute(
                    "UPDATE root SET last_scan_status = 'waiting' \
                     WHERE id = ?1 AND path = ?2 AND last_scan_status IS ?3",
                    rusqlite::params![root.id, path, before],
                )
                .ok()?;
            (marked == 1).then(|| Queued {
                root: root.clone(),
                before,
            })
        })
        .collect()
}

/// Write back each of `queued`'s word where it still reads `"waiting"`, once
/// its scan holds the job lock: until its walk records, it reads what its
/// last scan left.
///
/// # Errors
///
/// As [`mark_derived`].
pub fn unmark_queued(conn: &Connection, queued: &[Queued]) -> Result<(), String> {
    let mut first_refusal = None;
    for Queued { root, before } in queued {
        if let Err(e) = conn.execute(
            "UPDATE root SET last_scan_status = ?3 \
             WHERE id = ?1 AND path = ?2 AND last_scan_status = 'waiting'",
            rusqlite::params![root.id, root.path.to_string_lossy(), before],
        ) {
            first_refusal.get_or_insert_with(|| {
                format!(
                    "recording that directory {} has started its scan: {e}",
                    root.path.display()
                )
            });
        }
    }
    first_refusal.map_or(Ok(()), Err)
}

/// Rewrite each of `roots` to `to`, where it reads one of `from` and its id
/// still names its path, as [`scan_root`] writes; `done` says what the write
/// records, in the error.
fn rewrite_each(
    conn: &Connection,
    roots: &[Root],
    from: &[&str],
    to: &str,
    done: &str,
) -> Result<(), String> {
    let sql = format!(
        "UPDATE root SET last_scan_status = ? \
         WHERE id = ? AND path = ? AND last_scan_status IN ({})",
        vec!["?"; from.len()].join(", ")
    );
    let mut first_refusal = None;
    for root in roots {
        let path = root.path.to_string_lossy();
        let mut values: Vec<&dyn rusqlite::ToSql> = vec![&to, &root.id, &path];
        values.extend(from.iter().map(|word| word as &dyn rusqlite::ToSql));
        if let Err(e) = conn.execute(&sql, values.as_slice()) {
            first_refusal.get_or_insert_with(|| {
                format!(
                    "recording that directory {} {done}: {e}",
                    root.path.display()
                )
            });
        }
    }
    first_refusal.map_or(Ok(()), Err)
}

/// Rewrite every `"deriving"` and `"waiting"` to `"interrupted"`, and answer
/// how many were rewritten (O186).
///
/// **For `taiyaku-serve` at its start, before any scan of its own can begin.**
/// Only the server writes either word, so there one is a scan that a stop, a
/// panic or a refused write (O272) left without its last word, and leaving it
/// would read as one still running. The directory's next scan writes its
/// status again.
///
/// # Errors
///
/// Returns an error only if the `UPDATE` itself is refused.
pub fn interrupt_derives(conn: &Connection) -> Result<usize, String> {
    conn.execute(
        "UPDATE root SET last_scan_status = 'interrupted' \
         WHERE last_scan_status IN ('deriving', 'waiting')",
        [],
    )
    .map_err(|e| format!("rewriting the deriving and waiting statuses to interrupted: {e}"))
}

/// The walk itself — see [`scan_root`], which wraps this to record the
/// outcome on `root.last_scan_at`/`last_scan_status` regardless of which of
/// the three ways out below this function takes.
///
/// `Ok(None)` is a fourth, which records nothing: another writer changed this
/// directory's entries after the walk read them, so [`scan_root`] walks again.
fn scan_root_inner(conn: &Connection, root: &Root) -> Result<Option<ScanReport>, String> {
    let excluder = Excluder::build(root.exclude_globs.as_deref());
    // Spec §4: a file under a directory nested in this one is that
    // directory's to scan, by its rules, so this walk does not descend into
    // it.
    let all = roots(conn)?;
    let nested: HashSet<PathBuf> = all
        .iter()
        .filter(|other| crate::reach::nested_in(&other.path, &root.path).is_some())
        .map(|other| other.path.clone())
        .collect();
    let (measured, walk) = measure(&root.path, 0, &excluder, &nested)?;
    let seen = flatten(&measured);
    // Every filesystem operation an entry might need — `sniff`, `member_pages`
    // — happens below, before `BEGIN`. `held_for` is one query rather than one
    // per entry, and both it and the resolving below run outside any
    // transaction, so the write lock `tx` takes further down is held only
    // across the pure-SQL loop that follows. See `Resolved`.
    let held = held_for(conn, root.id)?;

    // §8.1, before a single row is written.
    //
    // **The test is "nothing under the root", not "no files".** A test that
    // every entry is a directory would refuse every scan of a perfectly healthy
    // root whose members are directories of page images. What an unmounted
    // share actually looks like is an empty directory: the root itself and
    // nothing beneath it.
    if seen.len() <= 1 {
        // Rows under a directory skipped as nested wait for its scan to take
        // them over, and rows under a link to one, which the walk met, are
        // forgotten below: neither makes this directory look unmounted. The
        // outer's rows this scan would drop count, so an empty read at a nested
        // directory's first scan cannot drop what the outer holds; those the
        // outer keeps do not, since no scan here drops them (decision 21), nor
        // do this directory's rows that go back to the outer.
        let around = Enclosing::of(&all, root);
        let held = held
            .iter()
            .filter(|(rel_path, entry)| {
                !under_any(rel_path, &walk.skipped)
                    && !under_any(rel_path, &walk.aliases)
                    && outer_holder(around.as_ref(), &excluder, rel_path, entry.is_dir).is_none()
            })
            .count()
            + takeable(conn, root, &all, &excluder)?;
        if held > 1 {
            // A fourth cause, alongside the three below: this root's own
            // `exclude_globs` can cover everything beneath it just as
            // completely as an unmounted share can, and it is the one cause
            // here an admin can have created themselves, seconds earlier —
            // `Excluder::build("*\n!Naruto/**").excludes("Naruto", true)` is
            // `true`, because git cannot re-include a child of an excluded
            // parent, so the natural first attempt at a whitelist produces
            // exactly this shape. Whether it did is decided from the walk
            // that already ran, not guessed: `excluded_everything` is true
            // only when this walk actually excluded a directory or a file on
            // the way to finding nothing.
            let excluded_everything = !walk.excluded_dirs.is_empty() || walk.excluded_files > 0;
            // "Remove it" is C4 §2's most destructive action, and it must never
            // be the advice handed to someone whose own patterns did this a
            // moment ago. When the walk excluded nothing, exclusion cannot be
            // the cause and the plain advice applies.
            let remedy = if excluded_everything {
                format!(
                    "This scan's own exclude_globs excluded {} director{} and {} \
                     file{} beneath this directory, which is enough on its own to \
                     produce exactly this — loosen or clear the patterns before \
                     assuming anything is actually gone.",
                    walk.excluded_dirs.len(),
                    if walk.excluded_dirs.len() == 1 {
                        "y"
                    } else {
                        "ies"
                    },
                    walk.excluded_files,
                    if walk.excluded_files == 1 { "" } else { "s" },
                )
            } else if walk.skipped.is_empty() {
                "If the directory has genuinely become empty, remove it.".to_string()
            } else {
                // It still shows a nested directory, so "remove it" is wrong.
                // It does not claim the directory is not empty: a nested share
                // mounted while this one's is not reads exactly like this.
                let mut skipped = walk.skipped.clone();
                skipped.sort();
                let names = skipped
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let (what, whose) = if skipped.len() == 1 {
                    ("a nested directory", "its")
                } else {
                    ("nested directories", "their")
                };
                format!(
                    "It still shows {names}, {what} scanned on {whose} own, so do not remove \
                     it; check that its share is mounted and what became of everything else \
                     it held."
                )
            };
            return Ok(Some(ScanReport {
                root: root.path.clone(),
                dirs: 0,
                files: 0,
                added: 0,
                updated: 0,
                gone: 0,
                unreadable_dirs: walk.unreadable,
                excluded_dirs: walk.excluded_dirs,
                excluded_files: walk.excluded_files,
                bad_exclude_patterns: excluder.complaints,
                forgotten_by_exclusion: 0,
                taken_over: 0,
                refused: Some(format!(
                    "{} scanned empty while the library holds {held} entries for it — \
                     refusing to apply. An unmounted share, a bind mount that was not \
                     ready, a renamed directory, or this directory's own exclude_globs \
                     excluding everything beneath it all look exactly like this, and \
                     applying it would drop every work under this directory along with the \
                     reading positions and notes attached to them. {remedy}",
                    root.path.display()
                )),
            }));
        }
    }

    let resolved: HashMap<String, Resolved> = seen
        .iter()
        .filter(|entry| !unchanged(held.get(&entry.rel_path).copied(), entry))
        .map(|entry| {
            let format = format_of(root, entry);
            let (pages, title) = pages_of(root, entry, format);
            (
                entry.rel_path.clone(),
                Resolved {
                    format,
                    pages,
                    title,
                },
            )
        })
        .collect();

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the scan transaction: {e}"))?;
    // The connection begins its transactions `Immediate`, so the write lock is
    // held from here: no removal can commit between this question and the
    // writes below. `still_this_dir` asks after the library directory walked:
    // its `root` row, by id and path.
    let still_this_dir: bool = tx
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM root WHERE id = ?1 AND path = ?2)",
            rusqlite::params![root.id, root.path.to_string_lossy()],
            |r| r.get(0),
        )
        .map_err(|e| {
            format!(
                "checking that {} is still a directory: {e}",
                root.path.display()
            )
        })?;
    if !still_this_dir {
        return Err(format!(
            "{} was removed while it was being scanned, so nothing was recorded",
            root.path.display()
        ));
    }
    // `record` and `forget_unseen` act on `held`, read before `BEGIN`. Another
    // scan of this directory (the server's schedule beside a CLI `library
    // scan`) may have committed since: on that stale read the second to commit
    // fails a new entry on `UNIQUE (root_id, rel_path)`, or writes over what
    // the first recorded. Asked again under the write lock, a read that moved
    // records nothing here, and the walk starts over from the store as it is.
    if held_for(&tx, root.id)? != held {
        return Ok(None);
    }
    let mut report = ScanReport::for_root(&root.path);
    report.unreadable_dirs = walk.unreadable;
    report.excluded_dirs = walk.excluded_dirs;
    report.excluded_files = walk.excluded_files;
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
    let mut carried = Carried::new();
    let (gone, forgotten_by_exclusion) = forget_unseen(
        &tx,
        root,
        &seen,
        &held,
        &excluder,
        &walk.skipped,
        &mut carried,
    )?;
    report.gone = gone;
    report.forgotten_by_exclusion = forgotten_by_exclusion;
    let recorded: HashSet<&str> = seen.iter().map(|entry| entry.rel_path.as_str()).collect();
    let taken = take_over(&tx, root, &recorded, &excluder, &mut carried)?;
    report.taken_over = taken.recorded;
    report.gone += taken.gone;
    report.forgotten_by_exclusion += taken.excluded;
    // Last use of `excluder` — moved out here rather than beside the other
    // `walk`-derived fields above, because `forget_unseen` still needs a
    // whole-struct `&excluder` to ask `excludes_with_parents`, and moving
    // `complaints` out of it any earlier leaves `excluder` partially moved
    // and that borrow will not compile.
    report.bad_exclude_patterns = excluder.complaints;
    tx.commit()
        .map_err(|e| format!("committing the scan of {}: {e}", root.path.display()))?;
    // What the carry leaves is not reported here. A hand-over notes addresses
    // under directories that are roots when it commits, whose `reconcile`
    // reports what stays there; a removal of one of them committing before
    // this carry runs can leave a document under no root, and this scan does
    // not list it.
    carry_documents(conn, &carried).map_err(|e| {
        format!(
            "moving hand documents after the scan of {}: {e}",
            root.path.display()
        )
    })?;
    Ok(Some(report))
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
    /// A directory on disk the walk recorded, the root's own included; not a
    /// library directory (a `root` row).
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
fn unchanged(held: Option<Held>, seen: &Seen) -> bool {
    let size = i64::try_from(seen.size).unwrap_or(i64::MAX);
    held.is_some_and(|held| held.size == size && held.mtime == seen.mtime)
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
    held: Option<Held>,
    resolved: Option<&Resolved>,
    report: &mut ScanReport,
) -> Result<(), String> {
    let size = i64::try_from(seen.size).unwrap_or(i64::MAX);

    match (held.is_some(), resolved) {
        (true, None) => {
            // `last_seen`, and for a directory its recursive page total.
            //
            // **A directory's total is written even when the directory itself
            // has not moved**, because a directory's mtime moves only when an
            // entry is added, removed or renamed *in it*. Without this, every
            // ancestor of a changed directory keeps its pre-change total
            // forever, and `derive.rs` reads exactly those totals to pick the
            // deepest directory that is a member.
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
                        title = ?8, sha256 = NULL, last_seen = datetime('now')
                  WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![
                    root.id,
                    seen.rel_path,
                    seen.kind.as_str(),
                    resolved.format,
                    size,
                    seen.mtime,
                    resolved.pages,
                    resolved.title,
                ],
            )
            .map_err(|e| format!("updating {}: {e}", seen.rel_path))?;
        }
        (false, Some(resolved)) => {
            report.added += 1;
            conn.execute(
                "INSERT INTO scan_entry
                     (root_id, rel_path, kind, format, size, mtime, pages, title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    root.id,
                    seen.rel_path,
                    seen.kind.as_str(),
                    resolved.format,
                    size,
                    seen.mtime,
                    resolved.pages,
                    resolved.title,
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
/// walk, on every file, on every scan, would be the expensive thing this
/// module says it avoids.
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
/// does not apply, and the title it declares.
///
/// A directory's count comes from the walk, which summed it on the way past.
/// A file's costs an archive open, so it is only paid for a format this
/// project can read, a reflowable EPUB counting its blocks and naming its
/// `dc:title`; a PDF or fixed-layout EPUB is recorded with no count.
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
fn pages_of(
    root: &Root,
    seen: &Seen,
    format: Option<&'static str>,
) -> (Option<u32>, Option<String>) {
    match seen.kind {
        Kind::Dir => (seen.pages, None),
        Kind::File if format == taiyaku_read::Format::Zip.token() => (
            taiyaku_read::member_pages(&root.path.join(&seen.rel_path)).ok(),
            None,
        ),
        Kind::File if format == taiyaku_read::Format::EpubReflow.token() => {
            // A book with no text has no count, as `member_pages` refuses an
            // archive with no page images: the derive names a container with
            // no count as unreadable, and takes a zero for a directory of
            // archives, counted and never named.
            match taiyaku_read::read_book(&root.path.join(&seen.rel_path)) {
                Ok(book) => (
                    u32::try_from(book.blocks.len())
                        .ok()
                        .filter(|&blocks| blocks > 0),
                    book.title,
                ),
                Err(_) => (None, None),
            }
        }
        Kind::File => (None, None),
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
/// **The whole point of this type is where it is built.** Sniffing and counting
/// pages are filesystem operations, and doing them inside `tx` would hold the
/// write lock across every archive open in a library scan — five seconds is
/// [`crate::BUSY_TIMEOUT`] — while `taiyaku serve` is writing a `progress` row
/// on every page turn.
struct Resolved {
    format: Option<&'static str>,
    pages: Option<u32>,
    /// The title the file declares, a book's `dc:title`.
    title: Option<String>,
}

/// One row `scan_entry` already holds for this root, as much of it as the
/// walk needs: `(size, mtime)` to decide whether anything must be reopened,
/// and `is_dir` so [`forget_unseen`] can ask [`Excluder`] about a stale path
/// without a second query.
///
/// A named type, the shape [`Seen`], [`Resolved`] and [`Measured`] use. `Copy`
/// because `held_for`'s map is read by value throughout this module.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Held {
    size: i64,
    mtime: i64,
    is_dir: bool,
}

/// What `(size, mtime, is_dir)` this root already holds, by `rel_path`.
///
/// One query rather than one per entry, and outside the transaction, so the
/// decision about what needs opening is made before any lock is taken. That
/// makes it a snapshot another writer can move before the transaction opens,
/// so [`scan_root_inner`] reads it again inside, and walks again where the
/// two differ.
fn held_for(conn: &Connection, root_id: i64) -> Result<HashMap<String, Held>, String> {
    let mut stmt = conn
        .prepare("SELECT rel_path, size, mtime, kind FROM scan_entry WHERE root_id = ?1")
        .map_err(|e| format!("reading what this directory held: {e}"))?;
    let rows = stmt
        .query_map([root_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Held {
                    size: r.get(1)?,
                    mtime: r.get(2)?,
                    is_dir: r.get::<_, String>(3)? == Kind::Dir.as_str(),
                },
            ))
        })
        .map_err(|e| format!("reading what this directory held: {e}"))?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Drop rows for this root that the walk did not meet, and say how many for
/// each of two reasons: `.0` genuinely gone from disk, `.1` forgotten only
/// because a pattern now covers them — see [`ScanReport::forgotten_by_exclusion`]
/// for why those are not the same number. A row `root`'s walk no longer
/// reaches goes first, with its member, to the one [`outer_holder`] names.
///
/// Reached only after the §8.1 refusal above, so "the walk met nothing" can
/// never arrive here. Iterates `held`, read before the transaction opened.
fn forget_unseen(
    conn: &Connection,
    root: &Root,
    seen: &[Seen],
    held: &HashMap<String, Held>,
    excluder: &Excluder,
    skipped: &[String],
    carried: &mut Carried,
) -> Result<(usize, usize), String> {
    let kept: HashSet<&str> = seen.iter().map(|entry| entry.rel_path.as_str()).collect();
    let mut gone = 0;
    let mut forgotten_by_exclusion = 0;
    let stale: Vec<(&str, Held)> = held
        .iter()
        .filter(|(rel_path, _)| !kept.contains(rel_path.as_str()))
        // The walk did not look under a nested directory in `skipped`, so it
        // cannot say a file there is gone: that directory's scan takes the row
        // over ([`take_over`]). A row recorded through a symlink to one is
        // forgotten, since no hand-over takes it.
        .filter(|(rel_path, _)| !under_any(rel_path, skipped))
        .map(|(rel_path, held)| (rel_path.as_str(), *held))
        .collect();
    let all = roots(conn)?;
    let around = Enclosing::of(&all, root);
    let failed = |what: &str, e: rusqlite::Error| format!("handing back {what}: {e}");
    let mut went_to: HashMap<String, i64> = HashMap::new();
    for (rel_path, held) in stale {
        let holder = outer_holder(around.as_ref(), excluder, rel_path, held.is_dir);
        // Why it is stale, not merely that it is. A row under a directory
        // the walk never descended into has a path that need not match any
        // pattern itself, which is what `excludes_with_parents` is for. A row
        // handed back is never gone: the directory around reaches its file.
        if holder.is_some() || excluder.excludes_with_parents(rel_path, held.is_dir) {
            forgotten_by_exclusion += 1;
        } else {
            gone += 1;
        }
        // Decision 21: the directory around still reaches the file, so its
        // accounts keep the work, as when the pattern comes before this
        // directory's first scan. Copied before the row goes.
        if let Some((to, to_rel)) = holder {
            copy_entry(conn, root.id, rel_path, to, &to_rel).map_err(|e| failed(rel_path, e))?;
            went_to.insert(root.path.join(rel_path).to_string_lossy().into_owned(), to);
        }
        conn.execute(
            "DELETE FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, rel_path],
        )
        .map_err(|e| format!("forgetting {rel_path}: {e}"))?;
        conn.execute(
            "DELETE FROM dir_state WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, rel_path],
        )
        .map_err(|e| format!("forgetting the state of {rel_path}: {e}"))?;
    }
    if !went_to.is_empty() {
        let members = members_of(conn, root.id).map_err(|e| failed("the members", e))?;
        move_members(conn, &members, &went_to, carried, &failed)?;
    }
    Ok((gone, forgotten_by_exclusion))
}

/// Whether `rel_path` is one of `dirs` or beneath one, by whole names: `kidsx`
/// is not beneath `kids`.
fn under_any(rel_path: &str, dirs: &[String]) -> bool {
    dirs.iter().any(|dir| {
        rel_path == dir
            || rel_path
                .strip_prefix(dir.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// SQL for `column` at or beneath the path bound to `?first`, and at or
/// beneath none of the `outside` paths bound after it.
///
/// `substr`, never `LIKE`: `_` and `%` occur in names. The `'/'` makes the
/// match whole names, so `kidsx` is not beneath `kids`.
fn under_but_not(column: &str, first: usize, outside: usize) -> String {
    let at_or_under = |n: usize| {
        format!("({column} = ?{n} OR substr({column}, 1, length(?{n}) + 1) = ?{n} || '/')")
    };
    let mut clause = at_or_under(first);
    for n in first + 1..=first + outside {
        clause.push_str(" AND NOT ");
        clause.push_str(&at_or_under(n));
    }
    clause
}

/// Each directory around `root`, with `root`'s path relative to it and the
/// paths, relative to it, of the directories nested in `root`.
fn around<'a>(all: &'a [Root], root: &Root) -> Vec<(&'a Root, String, Vec<String>)> {
    let inside: Vec<&Path> = all
        .iter()
        .filter(|other| crate::reach::nested_in(&other.path, &root.path).is_some())
        .map(|other| other.path.as_path())
        .collect();
    all.iter()
        .filter_map(|outer| {
            let rel = crate::reach::nested_in(&root.path, &outer.path)?;
            let inside_rel = inside
                .iter()
                .filter_map(|path| crate::reach::nested_in(path, &outer.path))
                .collect();
            Some((outer, rel, inside_rel))
        })
        .collect()
}

/// `outer`'s `scan_entry` rows, by path and kind, at or beneath `rel` (the
/// nested directory's path relative to it) but not beneath `inside_rel`: what
/// [`take_over`] reads and [`takeable`] counts.
fn held_around(
    conn: &Connection,
    outer: &Root,
    rel: &str,
    inside_rel: &[String],
) -> rusqlite::Result<Vec<(String, String)>> {
    let mut bound: Vec<&dyn rusqlite::ToSql> = vec![&outer.id, &rel];
    bound.extend(inside_rel.iter().map(|rel| rel as &dyn rusqlite::ToSql));
    conn.prepare(&format!(
        "SELECT rel_path, kind FROM scan_entry WHERE root_id = ?1 AND {}",
        under_but_not("rel_path", 2, inside_rel.len())
    ))
    .and_then(|mut stmt| {
        stmt.query_map(bound.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    })
}

/// `rel_path`, which is `rel` or beneath it, relative to `rel`.
fn relative_to<'a>(rel_path: &'a str, rel: &str) -> &'a str {
    rel_path
        .strip_prefix(rel)
        .map_or(rel_path, |rest| rest.strip_prefix('/').unwrap_or(rest))
}

/// How many `scan_entry` rows [`take_over`] would drop from the directories
/// around `root`, for §8.1's count: all it reads but those [`outer_holder`]
/// gives a directory, which keeps or receives them.
fn takeable(
    conn: &Connection,
    root: &Root,
    all: &[Root],
    excluder: &Excluder,
) -> Result<usize, String> {
    let mut rows = 0;
    let enclosing = Enclosing::of(all, root);
    for (outer, rel, inside_rel) in around(all, root) {
        let held = held_around(conn, outer, &rel, &inside_rel).map_err(|e| {
            format!(
                "counting what {} holds under {}: {e}",
                outer.path.display(),
                root.path.display()
            )
        })?;
        rows += held
            .iter()
            .filter(|(rel_path, kind)| {
                let own = relative_to(rel_path, &rel);
                outer_holder(
                    enclosing.as_ref(),
                    excluder,
                    own,
                    kind == Kind::Dir.as_str(),
                )
                .is_none()
            })
            .count();
    }
    Ok(rows)
}

/// The rows [`take_over`] took, each counted as this scan counts its own.
#[derive(Default)]
struct Taken {
    /// Rows this walk recorded too: [`ScanReport::taken_over`].
    recorded: usize,
    /// Rows dropped that this directory's patterns cover:
    /// [`ScanReport::forgotten_by_exclusion`].
    excluded: usize,
    /// Rows this walk did not meet and its patterns do not cover.
    gone: usize,
}

/// Take over what each directory around `root` holds under it: delete its
/// `dir_state` and `scan_entry` rows at or beneath `root`, and move to `root`
/// its members of the files this walk recorded. A row `root`'s walk does not
/// reach stays with, or goes with its member to, the directory [`outer_holder`]
/// names (decision 21). Each moving work is noted in `carried`.
///
/// Spec §4. Run in the scan's transaction, after its own rows are recorded,
/// so a file this walk also met is held by one directory or the other at every
/// commit.
fn take_over(
    conn: &Connection,
    root: &Root,
    recorded: &HashSet<&str>,
    excluder: &Excluder,
    carried: &mut Carried,
) -> Result<Taken, String> {
    // Read inside the transaction: the directories as they stand at commit.
    let all = roots(conn)?;
    // Rows beneath a directory nested in `root` are that directory's, taken at
    // its own scan; taken here, they would go to a walk that never reads them.
    let inside_paths: Vec<String> = all
        .iter()
        .filter(|other| crate::reach::nested_in(&other.path, &root.path).is_some())
        .map(|other| other.path.to_string_lossy().into_owned())
        .collect();
    let here = root.path.to_string_lossy().into_owned();
    // Each recorded file's member path, joined as `derive::classify` joins it.
    let recorded_paths: HashSet<String> = recorded
        .iter()
        .map(|rel| root.path.join(rel).to_string_lossy().into_owned())
        .collect();
    let enclosing = Enclosing::of(&all, root);
    let mut taken = Taken::default();
    for (outer, rel, inside_rel) in around(&all, root) {
        let failed = |what: &str, e: rusqlite::Error| {
            format!(
                "taking over {what} from {} for {}: {e}",
                outer.path.display(),
                root.path.display()
            )
        };
        let rows = under_but_not("rel_path", 2, inside_rel.len());
        let mut bound: Vec<&dyn rusqlite::ToSql> = vec![&outer.id, &rel];
        bound.extend(inside_rel.iter().map(|rel| rel as &dyn rusqlite::ToSql));
        let held =
            held_around(conn, outer, &rel, &inside_rel).map_err(|e| failed("its entries", e))?;
        // Each member path that moves, and the directory it moves to.
        let mut went_to: HashMap<String, i64> = HashMap::new();
        let mut dropped: Vec<&str> = Vec::with_capacity(held.len());
        for (rel_path, kind) in &held {
            let own = relative_to(rel_path, &rel);
            let is_dir = kind == Kind::Dir.as_str();
            if recorded.contains(own) {
                taken.recorded += 1;
            } else if let Some((to, to_rel)) =
                outer_holder(enclosing.as_ref(), excluder, own, is_dir)
            {
                // `outer`'s walk never descends here, so dropped, the missing
                // mark (O91) would hide a work whose file is on disk.
                if to == outer.id {
                    continue;
                }
                copy_entry(conn, outer.id, rel_path, to, &to_rel)
                    .map_err(|e| failed(rel_path, e))?;
                went_to.insert(root.path.join(own).to_string_lossy().into_owned(), to);
            } else if excluder.excludes_with_parents(own, is_dir) {
                taken.excluded += 1;
            } else {
                taken.gone += 1;
            }
            dropped.push(rel_path);
        }
        let mut drop = conn
            .prepare("DELETE FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2")
            .map_err(|e| failed("its entries", e))?;
        for rel_path in dropped {
            drop.execute(rusqlite::params![outer.id, rel_path])
                .map_err(|e| failed("its entries", e))?;
        }
        conn.execute(
            &format!("DELETE FROM dir_state WHERE root_id = ?1 AND {rows}"),
            bound.as_slice(),
        )
        .map_err(|e| failed("its directory states", e))?;
        let under = under_but_not("m.path", 2, inside_paths.len());
        let mut bound: Vec<&dyn rusqlite::ToSql> = vec![&outer.id, &here];
        bound.extend(inside_paths.iter().map(|path| path as &dyn rusqlite::ToSql));
        let members: Vec<MemberRow> = conn
            .prepare(&format!(
                "SELECT m.id, m.path, e.work_id, m.format FROM member m
                   JOIN edition e ON e.id = m.edition_id
                  WHERE m.root_id = ?1 AND {under}"
            ))
            .and_then(|mut stmt| {
                stmt.query_map(bound.as_slice(), |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .collect()
            })
            .map_err(|e| failed("its members", e))?;
        // Only the members of files this walk recorded move to `root`, a stale
        // edition's included, since it shares a recorded file's path: a grant
        // of `root`'s library then reaches the edition the derive keeps beside
        // a renamed one. A member of a file `root`'s walk does not reach stays
        // off `root`, with its row where `outer_holder` names a directory and
        // without one where it names none, so `root`'s accounts never see it
        // and removing `root` loses nothing of it (spec decision 21).
        for (_, path, ..) in &members {
            if recorded_paths.contains(path.as_str()) {
                went_to.insert(path.clone(), root.id);
            }
        }
        move_members(conn, &members, &went_to, carried, &failed)?;
    }
    Ok(taken)
}

// --- measure: the tree, gathered with no side effects -----------------------

/// Recursion ceiling for `measure`, far past any real library root — a volume
/// split into chapter subdirectories, the deepest nesting `docs/sidecar.md`
/// records, is two levels — and there purely to turn a symlink cycle into a
/// named error rather than an `Ok` with the cycle cut short.
///
/// `child_path.is_dir()` follows symlinks, so a link back to an ancestor sends
/// the walk back down into that ancestor at every level. The precise fix is a
/// cycle guard: canonicalize each directory and refuse to revisit one already
/// on the path down. That costs a syscall per directory, not worth paying on
/// every scan for something this rare — a depth limit is free, but names the
/// cycle only past `MAX_DEPTH` levels down rather than at its first revisit.
///
/// **Lower than "far above a real tree" alone would suggest.** A
/// self-referential symlink does not recurse forever on this platform:
/// Linux caps symlink resolution at 40 hops per lookup (`ELOOP`), and
/// `Path::is_dir()` swallows that error and answers `false` rather than
/// propagating it — so past roughly 40 levels the walk does not crash, it
/// silently treats the cyclic subtree as childless and returns `Ok`, which is
/// the "silent truncation" this limit exists to rule out. The ceiling has to
/// fire before that happens, not after, so it sits at 32 — comfortably past any
/// real library root and comfortably short of where the OS starts lying about
/// `is_dir`.
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

/// What one walk accumulated besides the tree itself.
///
/// One accumulator rather than three out-params to [`measure_dir`].
#[derive(Default)]
struct Walk {
    unreadable: Vec<String>,
    excluded_dirs: Vec<String>,
    excluded_files: usize,
    /// Nested directories met at their own path and not descended into, by
    /// relative path. The rows under them are left for that directory's scan
    /// (`forget_unseen`, §8.1).
    skipped: Vec<String>,
    /// Symlinks to a nested directory, met and not descended into. No
    /// hand-over takes the rows recorded through one, so `forget_unseen`
    /// forgets them; §8.1 does not count them, since the walk met the link.
    aliases: Vec<String>,
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
///
/// A directory in `nested`, which holds canonical paths, is not descended
/// into: see [`measure_dir`].
fn measure(
    path: &Path,
    depth: u32,
    excluder: &Excluder,
    nested: &HashSet<PathBuf>,
) -> Result<(Measured, Walk), String> {
    let mut walk = Walk::default();
    let measured = measure_dir(path, String::new(), depth, excluder, nested, &mut walk)
        .map_err(MeasureError::into_message)?;
    // `walk.excluded_dirs` is appended in `read_dir` order, which is not stable
    // across identical scans of an unchanged tree. Sorted here, once, over the
    // flat accumulator rather than inside `measure_dir`'s own loop, so the
    // report is deterministic without touching `dir_entries.sort()`, the
    // ordering the walk actually descends in.
    walk.excluded_dirs.sort();
    Ok((measured, walk))
}

/// One directory, measured — see [`measure`]. `walk.unreadable` collects a
/// message for every descendant directory this call meets and cannot list;
/// `path` itself refusing to list is returned as an `Err` instead, which is
/// what lets the caller tell "this one directory is unreadable" from "the
/// directory I was just asked to measure is" — see [`MeasureError`]. A child
/// directory in `nested` is skipped and named in `walk.skipped`.
fn measure_dir(
    path: &Path,
    rel_path: String,
    depth: u32,
    excluder: &Excluder,
    nested: &HashSet<PathBuf>,
    walk: &mut Walk,
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
        let child_rel = join_rel(&rel_path, &name);
        let is_dir = child_path.is_dir();
        // **Before `entry_count`, and before the page/file split.**
        // `dir_state.entry_count` exists to say whether a directory changed;
        // an excluded file arriving or leaving is not a change this library
        // has any interest in, and counting it would make one show up. The
        // cost is that `entry_count` does not equal what `read_dir`
        // returned — consistent, since it is only ever compared against a
        // number computed the same way, and a pattern edit *should* read as
        // a change.
        //
        // A directory is skipped here rather than filtered later, which is
        // the whole saving: nothing beneath it is listed, stat'ed, sniffed
        // or opened. It also gives git's own rule for free — a file whose
        // parent directory is excluded cannot be re-included, because the
        // walk never reaches it.
        if excluder.excludes(&child_rel, is_dir) {
            if is_dir {
                walk.excluded_dirs.push(child_rel);
            } else {
                walk.excluded_files += 1;
            }
            continue;
        }
        // Spec §4: a nested directory scans its files by its own rules, once.
        // A symlink is compared by its target, so a link to a nested directory
        // does not read its files back under this one's rules. After the
        // exclusion, so a directory this one's patterns exclude is reported
        // as excluded, as reach treats it.
        if is_dir && !nested.is_empty() {
            let is_link = entry.file_type().is_ok_and(|kind| kind.is_symlink());
            if is_link {
                if child_path
                    .canonicalize()
                    .is_ok_and(|target| nested.contains(&target))
                {
                    walk.aliases.push(child_rel);
                    continue;
                }
            } else if nested.contains(&child_path) {
                walk.skipped.push(child_rel);
                continue;
            }
        }
        entry_count += 1;
        if is_dir {
            dir_entries.push((child_rel, child_path));
        } else if taiyaku_read::is_page_name(&name) {
            here += 1;
        } else {
            file_entries.push((child_rel, child_path));
        }
    }
    dir_entries.sort();
    file_entries.sort();

    let mut pages = here;
    let mut children = Vec::new();
    for (child_rel, child_path) in dir_entries {
        match measure_dir(&child_path, child_rel, depth + 1, excluder, nested, walk) {
            Ok(child) => {
                if let Some(child_pages) = child.pages {
                    pages += child_pages;
                }
                children.push(child);
            }
            // Skipped, not propagated: this directory cannot speak for
            // whatever is unreadable beneath it, but it can still speak for
            // everything else it holds. See `ScanReport::unreadable_dirs`.
            Err(MeasureError::Unreadable(message)) => walk.unreadable.push(message),
            // A cycle's depth is a property of the whole path down to it, not
            // of this one directory — caught here it would look like an
            // ordinary permission refusal and be silently skipped, which is
            // exactly the silent truncation `MAX_DEPTH`'s own doc comment
            // says this must not become.
            Err(err @ MeasureError::TooDeep(_)) => return Err(err),
        }
    }
    for (child_rel, child_path) in file_entries {
        if let Some(file) = measure_file(&child_path, child_rel) {
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
/// writer appears.
///
/// **A file with no read permission is not handled here.** `stat` needs only
/// execute permission on the containing directory, so it succeeds where a
/// later `File::open` would not; that refusal surfaces downstream, in
/// [`format_of`]'s `.ok()`.
fn measure_file(path: &Path, rel_path: String) -> Option<Measured> {
    let meta = std::fs::metadata(path).ok()?;
    // **Regular files and directories only.** `stat` succeeds on a FIFO, a
    // socket and a device node, so the walk would record them — and then
    // `sniff` would `File::open` a FIFO and block until a writer appears.
    // Nothing here can read one anyway.
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
