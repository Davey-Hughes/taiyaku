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
    /// The library this directory is filed under — migration 15. Every root has
    /// one; see [`crate::libraries`].
    pub library_id: i64,
}

/// Every scan root this library knows about, oldest first.
///
/// # Errors
///
/// Returns an error if the query against `root` fails.
pub fn roots(conn: &Connection) -> Result<Vec<Root>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, path, edition_key, default_kind, default_language, exclude_globs, \
             scan_interval_secs, keeps_documents, library_id \
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
                exclude_globs: r.get(5)?,
                scan_interval_secs: r.get(6)?,
                keeps_documents: r.get(7)?,
                library_id: r.get(8)?,
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
/// root for, and a directory whose library of its own cannot be named because
/// both names [`InLibrary::Own`] tries are taken ([`add_root_checked`]
/// refuses that one first, saying so).
pub fn add_root(conn: &Connection, path: &Path) -> Result<Root, String> {
    add_root_in(conn, path, InLibrary::Own)
}

/// Which library a directory joins when it is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InLibrary<'a> {
    /// A library of its own, named for the directory by the
    /// `root_gets_a_library` trigger (migration 16): its last path component,
    /// or its whole path when a library already has that name in any ASCII
    /// case, stripped of white space either way.
    Own,
    /// The library with this name, created if there is none.
    Named(&'a str),
    /// The library with this id. [`add_root_checked_in`] refuses an id no
    /// library holds; [`add_root_in`] does not ask whether one does.
    Id(i64),
}

/// [`add_root`], filing the directory under `library`.
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
        "INSERT INTO root (path, library_id) VALUES (?1, ?2)",
        rusqlite::params![canonical.to_string_lossy(), chosen],
    )
    .map_err(|e| format!("adding root {}: {e}", canonical.display()))?;
    let id = tx.last_insert_rowid();
    // Read back rather than hard-coded, so the column's default — migration
    // 012's `DEFAULT 0` — stays the one place that decides it, and so a
    // library the `root_gets_a_library` trigger chose is the one recorded.
    let (keeps_documents, library_id) = tx
        .query_row(
            "SELECT keeps_documents, library_id FROM root WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("reading root {}: {e}", canonical.display()))?;
    tx.commit()
        .map_err(|e| format!("committing root {}: {e}", canonical.display()))?;
    Ok(Root {
        id,
        path: canonical,
        edition_key: None,
        default_kind: None,
        default_language: None,
        exclude_globs: None,
        scan_interval_secs: None,
        keeps_documents,
        library_id,
    })
}

/// Why a directory at `canonical` cannot be a library of its own, if it cannot.
///
/// **The `root_gets_a_library` trigger (migration 16) names that library, and
/// [`add_root_checked_in`] asks the trigger's own question here first**
/// rather than leaving its insert to fail on `library.name`'s UNIQUE: the
/// directory's last path component, or its whole path when the component is
/// empty or already a library's name. When the whole path is a library's
/// name too, the trigger has no third name to try, and [`InLibrary::Own`]
/// promises none, so this refuses and says which names are taken. The last
/// component is what follows the path's last `/`, as the trigger's `rtrim`
/// finds it. Both names are stripped of white space by
/// [`crate::libraries::trim_name`], as the trigger strips them with SQLite's
/// two-argument `trim` over the same characters, and are asked in any ASCII
/// case, as the trigger asks them. A component is empty for `/` and for a
/// directory named only white space.
///
/// **A component that strips to `.` or `..` is refused too**, as a directory
/// called `. ` or ` ..` would otherwise make a library of that name, which no
/// page address holds ([`crate::has_no_address`]). Refused rather than given
/// its whole path, because the trigger takes any component that is not empty
/// and not taken.
fn own_library_clash(conn: &Connection, canonical: &Path) -> Result<Option<String>, String> {
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
        return Ok(Some(format!(
            "{path} cannot be a library of its own: a library named for it would be called \
             `{leaf}`, and no address can hold “.” or “..” as a name; add it to a library by name"
        )));
    }
    if !leaf.is_empty() && !taken(leaf)? {
        return Ok(None);
    }
    if !taken(whole)? {
        return Ok(None);
    }
    Ok(Some(if leaf.is_empty() {
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

/// What [`add_root_checked`] built: the row [`add_root_in`] inserted, the watch
/// experiment run against it immediately, and the interval that experiment
/// implies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub root: Root,
    pub probe: crate::watch::Probe,
    pub interval: Duration,
}

/// Why [`add_root_checked`] could not add a root.
///
/// **Two shapes, not the bare `String` this module's other fallible
/// functions still return**, so a caller can tell which happened rather
/// than re-parsing a message meant for a person. Every other checked write
/// this codebase exposes over HTTP already keeps that split at the
/// handler — `taiyaku-serve`'s `create_rule`/`patch_rule` pre-check their
/// own caller-fault cases before ever writing, and reserve a 500 for the
/// store call itself. `add_root_checked` could offer no such split before
/// this type existed: its own checks run *inside* this function, not in the
/// handler that calls it, so a caller had nothing of its own to pre-check
/// against and a handler was left folding every failure this function could
/// produce — a bad path and a database gone read-only alike — into the same
/// 400.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddRootError {
    /// `path` itself: it does not exist, this process cannot read it, or it
    /// overlaps a root already held, nested in either direction. The
    /// message already names which, and for an overlap, the colliding root
    /// by path and id. Or the library it would join cannot be named: an
    /// [`InLibrary::Named`] that [`crate::libraries::usable_name`] refuses,
    /// or an [`InLibrary::Own`] whose two possible names are both taken,
    /// which the message names. Or an [`InLibrary::Id`] no library holds.
    /// For [`create_library`]: no directory given, a name `usable_name`
    /// refuses or that is already a library's, or two of the directories
    /// given overlapping.
    Unusable(String),
    /// The store itself failed: [`roots`] (the overlap check's own read),
    /// the library-name lookup, the insert inside [`add_root_in`], or the
    /// post-insert [`update_root`] write that records the watch probe's
    /// interval.
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

/// Add a new scan root at `path`, filed under `library`, having run C4 §2's
/// three validations that [`add_root`] itself does not — "because the
/// failures are silent otherwise."
///
/// 1. **Readable, by this process, not merely present.** `path.canonicalize`
///    answers "does it exist" and nothing else: it succeeds against a
///    directory that exists but that this process cannot open. So once
///    canonicalised, this calls [`std::fs::read_dir`] against the canonical
///    path and reports its refusal as a distinct failure, naming what it
///    was — "readable by the server process" is a different question from
///    "exists", and on a containerised deployment it is exactly the
///    question that differs.
/// 2. **Not nested inside, or around, any root already held — checked both
///    directions**, against every existing root's own canonical path.
///    [`Path::starts_with`] compares whole path components rather than
///    bytes, so `/srv/manga2` is never mistaken for something inside
///    `/srv/manga`; comparing the *canonical* form on both sides (this
///    function's own candidate and every stored [`Root::path`], which
///    [`add_root`] canonicalises before it ever reaches a row) is what
///    keeps a symlink or a `..` from slipping a nested path past a raw
///    string comparison. Overlapping roots double-count every file beneath
///    the overlap, and the symptom — duplicate works — looks nothing like
///    the cause, so the refusal names the existing directory it overlaps with,
///    and that directory's library, rather than only saying "nested."
/// 3. **The watcher probe, run now rather than left for the first scan.**
///    [`crate::watch::probe`] answers whether this root can be watched at
///    all — spec B §8's experiment — run at the moment a root is added
///    rather than discovered the first time [`scan_root`] or
///    [`fn@crate::derive::derive`] runs against a root nobody has probed
///    yet. The interval [`crate::watch::interval_for`] derives from that
///    answer is written to `scan_interval_secs` immediately, not left at
///    the column default until some later call happens to set it.
///
/// **And the library it joins can be named**, checked with the first two,
/// before anything is written: an [`InLibrary::Named`] needs a name
/// [`crate::libraries::usable_name`] takes, and an [`InLibrary::Own`] needs a
/// name the `root_gets_a_library` trigger can give it (see
/// `own_library_clash`). Otherwise the first would surface as a store fault
/// and the second as a bare UNIQUE failure on `library.name`, though both are
/// this caller's to fix.
///
/// **The library is created only once every validation has passed**, by
/// [`add_root_in`], so a refused path names no library into being.
///
/// [`add_root`] and [`add_root_in`] still validate nothing: a great many
/// fixtures call them directly, and a validation that runs inside every
/// fixture is a validation nobody can test the absence of. This is the
/// checked path a person takes when adding a root they have not already
/// vetted by hand — through `taiyaku library scan --add`, or the HTTP API on
/// their behalf. That command took the unchecked one until O47, and this doc
/// named `import` as a caller of [`add_root`], which it has never been.
///
/// # Errors
///
/// Returns [`AddRootError::Unusable`] if `path` cannot be canonicalised (it
/// must exist), if the canonical path cannot be read as a directory, if it
/// is inside or contains a root already held, or if the library it would
/// join cannot be named (an [`InLibrary::Id`] no library holds is one) —
/// every one of those is this caller's own mistake,
/// an `error` a person can act on. A path already held is refused as one,
/// `<path> is already a directory of the library `<name>``, and not as inside
/// itself, which is what the nesting check alone would say of an equal path.
///
/// Returns [`AddRootError::Failed`] if the store itself refuses along the
/// way — the overlap check's own read ([`roots`]), the library-name lookup,
/// the insert ([`add_root_in`]), or the write that records the watch probe's
/// interval ([`update_root`]) — none of which is this caller's mistake to
/// fix.
pub fn add_root_checked_in(
    conn: &Connection,
    path: &Path,
    library: InLibrary<'_>,
) -> Result<Added, AddRootError> {
    let canonical = vetted(conn, path)?;

    match library {
        InLibrary::Named(name) => {
            if let Err(why) = crate::libraries::usable_name(name) {
                return Err(AddRootError::Unusable(format!(
                    "{}: {why}",
                    canonical.display()
                )));
            }
        }
        InLibrary::Own => {
            if let Some(why) = own_library_clash(conn, &canonical).map_err(AddRootError::Failed)? {
                return Err(AddRootError::Unusable(why));
            }
        }
        InLibrary::Id(id) => {
            if crate::libraries::name_of(conn, id)
                .map_err(AddRootError::Failed)?
                .is_none()
            {
                return Err(AddRootError::Unusable(format!(
                    "{}: there is no library {id}",
                    canonical.display()
                )));
            }
        }
    }

    let root = add_root_in(conn, &canonical, library).map_err(AddRootError::Failed)?;
    probed(conn, root)
}

/// `path` in canonical form, having checked that it exists, that this
/// process can list it, and that it is neither inside nor around a root
/// already held: [`add_root_checked_in`]'s first two validations.
fn vetted(conn: &Connection, path: &Path) -> Result<PathBuf, AddRootError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| AddRootError::Unusable(format!("{}: {e}", path.display())))?;

    std::fs::read_dir(&canonical).map_err(|e| {
        AddRootError::Unusable(format!("{}: cannot be read: {e}", canonical.display()))
    })?;

    for existing in roots(conn).map_err(AddRootError::Failed)? {
        let chosen = canonical.display();
        // Equal paths first: each starts with the other, so the nesting check
        // would call a directory picked again inside itself. A held directory
        // inside the chosen one is named from the chosen one, so the reader is
        // told where it is rather than left to compare two full paths for the
        // part that differs. A chosen directory inside a held one keeps both full
        // paths: the held one is the shorter.
        let said = if canonical == existing.path {
            format!("{chosen} is already a directory")
        } else if canonical.starts_with(&existing.path) {
            format!(
                "{chosen} is inside {}, which is already a directory",
                existing.path.display()
            )
        } else if let Ok(inner) = existing.path.strip_prefix(&canonical) {
            format!(
                "{chosen} holds `{}`, which is already a directory",
                inner.display()
            )
        } else {
            continue;
        };
        // Named by its library, which is how the Libraries screen groups its
        // directories, and not by its id, which no screen shows.
        let library = crate::libraries::name_of(conn, existing.library_id)
            .map_err(AddRootError::Failed)?
            .map_or_else(String::new, |name| format!(" of the library `{name}`"));
        return Err(AddRootError::Unusable(format!("{said}{library}")));
    }
    Ok(canonical)
}

/// [`add_root_checked_in`]'s third validation, for a root just inserted: the
/// watch probe, and the interval it implies written to the row.
fn probed(conn: &Connection, root: Root) -> Result<Added, AddRootError> {
    let probe = crate::watch::probe(&root.path, PROBE_TIMEOUT);
    let interval = crate::watch::interval_for(&probe);
    let interval_secs = interval.as_secs() as i64;
    update_root(
        conn,
        root.id,
        &RootPatch {
            scan_interval_secs: Some(Some(interval_secs)),
            ..Default::default()
        },
    )
    .map_err(AddRootError::Failed)?;

    Ok(Added {
        root: Root {
            scan_interval_secs: Some(interval_secs),
            ..root
        },
        probe,
        interval,
    })
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
/// Every path is vetted before anything is written, against the roots held
/// and against the other paths given. The library and its directories are then
/// inserted in one transaction. The watch probes run after it commits,
/// because each waits up to [`PROBE_TIMEOUT`] and a write transaction held
/// that long would stall every other writer.
///
/// # Errors
///
/// Returns [`AddRootError::Unusable`] for no path, a name
/// [`crate::libraries::usable_name`] refuses or that a library already holds,
/// a path [`add_root_checked_in`] would refuse, or two of the paths
/// overlapping. Returns [`AddRootError::Failed`] if the store refuses; a fault
/// while recording a probe's interval is one, and by then the library exists.
pub fn create_library(
    conn: &Connection,
    name: &str,
    paths: &[PathBuf],
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
        let held = vetted(conn, path)?;
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
    // statement for another writer to land between, and the name it found
    // free would then fail at the insert as a store fault, not as the
    // caller's mistake. The paths are vetted before it because vetting reads
    // the disk.
    if let Some(held) = crate::libraries::id_of(&tx, name).map_err(AddRootError::Failed)? {
        let stored = crate::libraries::name_of(&tx, held)
            .map_err(AddRootError::Failed)?
            .unwrap_or_else(|| name.to_string());
        return Err(AddRootError::Unusable(format!(
            "a library is already called `{stored}`; add the directory to it instead"
        )));
    }
    let library_id = crate::libraries::create(&tx, name).map_err(AddRootError::Failed)?;
    let mut inserted = Vec::with_capacity(canonical.len());
    for path in &canonical {
        inserted
            .push(add_root_in(&tx, path, InLibrary::Id(library_id)).map_err(AddRootError::Failed)?);
    }
    tx.commit()
        .map_err(|e| AddRootError::Failed(format!("committing the library `{name}`: {e}")))?;

    let mut dirs = Vec::with_capacity(inserted.len());
    for root in inserted {
        dirs.push(probed(conn, root)?);
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
    /// The accounts, by name, that see no library once this removal deletes
    /// the only library they are granted: [`only_grant_holders`], asked
    /// before anything is deleted. Empty where the library stays, as it does
    /// when a directory that is not its library's last goes.
    pub left_with_no_library: Vec<String>,
}

/// Every work whose *every* member sits under root `?1` — the works
/// [`take_root`] deletes, and what [`Removal::works`] counts.
///
/// **A subquery, reused by parameter rather than materialised into a
/// `Vec<i64>` and an `IN (?, ?, …)` built for it.** Three of
/// [`take_root`]'s statements need this exact set — the work count, the
/// positions it holds, the notes it holds — and a fourth, the delete, needs
/// it a fourth time; passing `id` as `?1` to each is simpler than binding a
/// list of a size decided at runtime, and there is only ever one definition
/// of "the works this root alone accounts for" to keep in step.
///
/// **A work is "under" `?1` when every one of its members is, checked
/// against every *other* member of the same work — not merely "this work has
/// a member under `?1`."** A work with one member on `?1` and another member
/// on some other, surviving root (`a_work_spanning_two_roots_gives_each_member_its_own_root`,
/// `derive/tests.rs`) still has material once `?1` is gone; only the work
/// whose material is entirely `?1`'s is a work removing `?1` actually
/// destroys.
///
/// **A member whose `root_id` is `NULL` excludes its work from this list
/// too**, on either side of the comparison. `NULL` means an imported work —
/// `schema/006_member_root.sql`'s own reading — or, in principle, a
/// scan-derived member from before its first derive since that migration;
/// neither is evidence that a work lives under `?1`, and a work is kept
/// standing rather than guessed away.
///
/// **Computed before `?1`'s own `root` row is deleted, and it has to be.**
/// `member.root_id` is the only record of which root a member came from, and
/// `schema/006_member_root.sql`'s `ON DELETE SET NULL` erases exactly that
/// the instant the `root` row goes — the same column O54 records as the one
/// that excludes a member from every root grant once it reads `NULL`.
/// [`take_root`] reads this before it deletes anything, and [`apply_removal`] and
/// [`apply_library_removal`] reach it through that.
const DOOMED_WORKS: &str = "
    SELECT DISTINCT e.work_id
      FROM member m
      JOIN edition e ON e.id = m.edition_id
     WHERE m.root_id = ?1
       AND NOT EXISTS (
             SELECT 1 FROM member m2
             JOIN edition e2 ON e2.id = m2.edition_id
            WHERE e2.work_id = e.work_id
              AND (m2.root_id IS NULL OR m2.root_id != ?1)
           )";

/// The accounts, by name and in name order, that see no library once library
/// `library_id` goes: those limited to the libraries they are granted, and
/// granted this one alone. Empty where there are none, and where
/// `library_id` names no library.
///
/// **An administrator is never named**: it sees every library whatever its
/// rows say. **Nor is an account set to every library** that still holds a
/// row, since the row restricts nothing until the account is limited.
///
/// Asked before the library is deleted, because its grants go with it. A
/// removal and a directory move both report what this answers.
pub(crate) fn only_grant_holders(
    conn: &Connection,
    library_id: i64,
) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT u.name
               FROM user_library ul
               JOIN user u ON u.id = ul.user_id
              WHERE ul.library_id = ?1
                AND u.role = 'user'
                AND u.every_library = 0
                AND NOT EXISTS (SELECT 1 FROM user_library held
                                 WHERE held.user_id = ul.user_id
                                   AND held.library_id <> ul.library_id)
              ORDER BY u.name",
        )
        .map_err(|e| format!("checking whose only grant library {library_id} is: {e}"))?;
    statement
        .query_map([library_id], |r| r.get::<_, String>(0))
        .map_err(|e| format!("checking whose only grant library {library_id} is: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("checking whose only grant library {library_id} is: {e}"))
}

/// Count what root `id` alone accounts for, then delete it: its works, the
/// members it supplied to works that survive it, and the root. Nothing is
/// derived and no account is named; [`apply_removal`] and
/// [`apply_library_removal`] do both around it.
///
/// **A member `DOOMED_WORKS` excluded, on a work that survives on some other
/// root, is deleted here too — explicitly, not left to strand.** Once its
/// own work is spared the works delete, that member's `root_id` still names
/// `id`, and the `root` row deleted last would otherwise leave it `NULL`
/// through `schema/006_member_root.sql`'s own `ON DELETE SET NULL` — the
/// exact O54 shape, on a smaller scale, that removal exists to rule out. The
/// work stays, since it has material elsewhere; the one member that does not
/// is taken with the root that supplied it.
fn take_root(conn: &Connection, id: i64) -> Result<Removal, String> {
    let works: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM ({DOOMED_WORKS})"),
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("counting the works root {id} alone accounts for: {e}"))?;
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

    conn.execute(
        &format!("DELETE FROM work WHERE id IN ({DOOMED_WORKS})"),
        [id],
    )
    .map_err(|e| format!("deleting the works root {id} alone accounts for: {e}"))?;
    conn.execute("DELETE FROM member WHERE root_id = ?1", [id])
        .map_err(|e| {
            format!("clearing what root {id} alone still held on a work that survives it: {e}")
        })?;
    conn.execute("DELETE FROM root WHERE id = ?1", [id])
        .map_err(|e| format!("removing root {id}: {e}"))?;

    Ok(Removal {
        works: usize::try_from(works).unwrap_or(usize::MAX),
        positions: usize::try_from(positions).unwrap_or(usize::MAX),
        notes: usize::try_from(notes).unwrap_or(usize::MAX),
        left_with_no_library: Vec::new(),
    })
}

/// The library root `id` is the last directory of, which removing it deletes.
/// `None` where the library keeps another directory, and where `id` names no
/// root.
fn emptied_by_removing(conn: &Connection, id: i64) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT l.id FROM root r JOIN library l ON l.id = r.library_id
          WHERE r.id = ?1
            AND NOT EXISTS (SELECT 1 FROM root other
                             WHERE other.library_id = r.library_id AND other.id <> r.id)",
        [id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map_err(|e| format!("checking whether root {id} is its library's last directory: {e}"))
}

/// Delete root `id`, every work that lived entirely under it, and re-derive
/// — the mechanism [`removal_cost`] runs and rolls back, and [`remove_root`]
/// runs and commits.
///
/// **Whoever is left with no library is read before anything is deleted**,
/// where `id` is its library's last directory: the library goes with that
/// directory, and its grants with the library, so afterwards nothing records
/// who held them.
///
/// **Delete-plus-derive, not a bare `DELETE FROM root`.** `scan_entry`,
/// `dir_state` and `rule` cascade with the root, and a library left with no
/// root goes with it (migration 15's `library_emptied_by_delete`), taking its
/// `user_library` grants (`schema/003_scanner.sql`,
/// `schema/015_libraries.sql`), but `work`, `edition`,
/// `member`, `progress` and `note` do not follow — `progress` and `note` are
/// keyed on `work_id` and carry no root at all, and `member.root_id` is `ON
/// DELETE SET NULL`, not `CASCADE`, precisely so an ordinary rescan finding a
/// root briefly missing cannot delete a library's pages
/// (`schema/006_member_root.sql`). A bare delete would leave every work,
/// edition, member, note and reading position standing with `member.root_id`
/// quietly `NULL` — invisible to every grant (O54) — asserting evidence that
/// is gone. This function does the rest of the job: [`DOOMED_WORKS`] names
/// what `?1` alone accounts for, deleted explicitly and before the `root` row
/// goes, and `derive_with` re-derives everything else — `series`,
/// `kept_editions` on a work that only partly depended on `?1`, and
/// `crate::proposals::raise` for whatever the loss changes.
///
/// **Always derives under [`crate::derive::Documents::Skip`], never
/// `Reconcile` — whole-branch review, Important 2.** This function's own
/// writes run inside a transaction that belongs to its caller — committed by
/// [`remove_root`], rolled back by [`removal_cost`] — and `Reconcile` writes
/// and renames hand-metadata documents under `<root>/.taiyaku/` while that
/// transaction is still open. `write_work`'s own doc comment states the
/// invariant this would break: a document is renamed only after its
/// database transaction "has already committed — never inside it. A
/// rollback that left the file renamed would strand it under a key the
/// database never adopted." Before this fix, `remove_root` passed `Reconcile`
/// here, so a document deleted just before a removal was found present
/// again immediately after it — written mid-transaction, and left in place
/// even had the removal then failed and rolled back. [`remove_root`] now
/// runs the real, `Reconcile`-mode pass itself, after its own transaction
/// commits — see that function's own doc.
///
/// # Errors
///
/// Returns an error if a read, [`take_root`] or `derive_with` is refused.
fn apply_removal(
    conn: &Connection,
    id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let left_with_no_library = match emptied_by_removing(conn, id)? {
        Some(library_id) => only_grant_holders(conn, library_id)?,
        None => Vec::new(),
    };
    let mut removal = take_root(conn, id)?;
    removal.left_with_no_library = left_with_no_library;
    let derived = crate::derive::derive_with(conn, crate::derive::Documents::Skip)?;

    Ok((removal, derived))
}

/// What removing root `id` would cost, found by actually doing it — inside a
/// transaction this function rolls back before returning.
///
/// [`crate::preview::preview`]'s own shape, reused rather than a second
/// mechanism for "what would this change do": open a transaction, make the
/// change, derive for real, read the result, roll back. [`apply_removal`]
/// always derives under [`crate::derive::Documents::Skip`], for
/// [`crate::preview::preview`]'s own reason — a real derivation writes and
/// renames hand-metadata documents under `<root>/.taiyaku/`, and a
/// `ROLLBACK` does not undo a filesystem write. A cost estimate that renamed
/// files in someone's library would be worse than the bug it exists to
/// prevent.
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
/// **[`crate::Nested`], not `unchecked_transaction` directly** — Task 11 puts
/// an HTTP route over this function and [`remove_root`] both, and while that
/// route calls each on a fresh connection today, `preview::preview` already
/// proves the alternative breaks the moment that stops being true: it wraps
/// a whole `derive_with` pass in one `BEGIN IMMEDIATE`, and every plain
/// `unchecked_transaction` opened underneath it — `import::scanned`,
/// `fold::absorb` — refused outright on the very first work, which is what
/// `Nested` exists to fix. Rolled back rather than committed by simply never
/// calling [`crate::Nested::commit`] and letting this scope drop: `Nested`'s
/// own `Drop` then undoes exactly what `apply_removal` just did, a real
/// `ROLLBACK` if nothing else was open or a `ROLLBACK TO SAVEPOINT` folded
/// back into a caller's own already-open transaction if something was — see
/// that type's own doc. Either way this function takes nothing, whether it
/// is called standalone or nested inside another transaction.
///
/// # Errors
///
/// Returns an error if SQLite refuses the transaction or if [`apply_removal`]
/// fails.
pub fn removal_cost(
    conn: &Connection,
    id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let tx = crate::Nested::open(conn, "removal_cost")
        .map_err(|e| format!("starting the removal-cost transaction: {e}"))?;
    let removal = apply_removal(&tx, id)?;
    // Deliberately not `tx.commit()` — see this function's own doc for why
    // dropping `tx` here, uncommitted, is the rollback.
    drop(tx);
    Ok(removal)
}

/// Remove root `id` for real: delete it, delete every work that lived
/// entirely under it, re-derive, and reconcile `<root>/.taiyaku/` on every
/// root that remains — the works just deleted are exactly the ones whose
/// documents become orphans, and reporting that back is
/// [`crate::hand::reconcile`]'s job, run here the same way
/// [`fn@crate::derive::derive`] runs it on every other call site.
///
/// **Two derive passes, not one — whole-branch review, Important 2.**
/// [`apply_removal`] always derives under [`crate::derive::Documents::Skip`]
/// (see that function's own doc), so the deletes and the SQL-only
/// re-derivation they need commit as one atomic unit with nothing written to
/// disk while that transaction is open. The real, `Documents::Reconcile`
/// pass — the one that writes and renames hand-metadata documents — runs
/// here, second, on `conn` directly, only *after* the transaction above has
/// committed. `derive_with`'s own doc states this is safe to run twice:
/// "idempotent by construction... running this twice over unchanged facts
/// writes the same rows a second time rather than a second set of rows," so
/// the cost is one redundant SQL pass, not a correctness risk. Before this
/// split, the `Reconcile` pass ran *inside* the transaction `Nested` opens
/// below, which `write_work`'s own doc comment states the exact invariant
/// this broke: a document is renamed only "after `import::scanned`'s own
/// transaction has already committed — never inside it. A rollback that left
/// the file renamed would strand it under a key the database never
/// adopted." Verified: a document deleted just before a removal was found
/// present again immediately afterward, written while the removal's own
/// transaction was still open — any `Err` after that rename would have
/// rolled the database back while leaving the filesystem moved.
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
/// the removal reconciled documents. See M5 of the whole-branch review for
/// why [`apply_removal`]'s own report is not simply discarded either way.
///
/// **[`crate::Nested`], not `unchecked_transaction` directly** — see
/// [`removal_cost`]'s own doc for why: the same composability this function
/// needs for Task 11's HTTP route.
///
/// # Errors
///
/// Returns an error if SQLite refuses the transaction, if [`apply_removal`]
/// fails, if the commit itself is refused, or if the post-commit derive
/// fails — in which case the removal itself has already committed and only
/// the reconciliation pass over its aftermath did not complete.
pub fn remove_root(
    conn: &Connection,
    id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let tx = crate::Nested::open(conn, "remove_root")
        .map_err(|e| format!("starting the root-removal transaction: {e}"))?;
    let (removal, _predicted) = apply_removal(&tx, id)?;
    tx.commit()
        .map_err(|e| format!("committing the removal of root {id}: {e}"))?;
    let derived = crate::derive::derive(conn)
        .map_err(|e| format!("reconciling documents after removing root {id}: {e}"))?;
    Ok((removal, derived))
}

/// Remove each directory of `library_id` in turn, summing what each takes, then
/// derive once.
///
/// **One directory after another inside the caller's transaction, never one dry
/// run per directory.** [`DOOMED_WORKS`] names a work for a directory only when
/// every member of it is under that directory. A work with an edition under each
/// of two directories is therefore named for neither while both stand, and for
/// the second once [`take_root`] has deleted the first's members.
///
/// **Whoever is left with no library is read before the first directory goes**,
/// for [`apply_removal`]'s reason. A library with no directory is not deleted
/// here, since it goes only with its last directory, so it leaves no one.
fn apply_library_removal(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let dirs: Vec<i64> = conn
        .prepare("SELECT id FROM root WHERE library_id = ?1 ORDER BY id")
        .map_err(|e| format!("listing the directories of library {library_id}: {e}"))?
        .query_map([library_id], |r| r.get(0))
        .map_err(|e| format!("listing the directories of library {library_id}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("listing the directories of library {library_id}: {e}"))?;
    let mut total = Removal {
        left_with_no_library: if dirs.is_empty() {
            Vec::new()
        } else {
            only_grant_holders(conn, library_id)?
        },
        ..Removal::default()
    };
    for dir in dirs {
        let taken = take_root(conn, dir)?;
        total.works += taken.works;
        total.positions += taken.positions;
        total.notes += taken.notes;
    }
    let derived = crate::derive::derive_with(conn, crate::derive::Documents::Skip)?;
    Ok((total, derived))
}

/// What removing library `library_id` would cost, found by doing it inside a
/// transaction this rolls back: [`removal_cost`], for a library.
///
/// # Errors
///
/// As [`removal_cost`].
pub fn library_removal_cost(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let tx = crate::Nested::open(conn, "library_removal_cost")
        .map_err(|e| format!("starting the library-removal-cost transaction: {e}"))?;
    let removal = apply_library_removal(&tx, library_id)?;
    // Not committed: dropping `tx` is the rollback, as in `removal_cost`.
    drop(tx);
    Ok(removal)
}

/// Remove library `library_id`: each of its directories, the works they alone
/// accounted for, and the library, which migration 15 deletes with its last
/// directory. Then derive again with documents reconciled, after the commit, for
/// [`remove_root`]'s reason.
///
/// # Errors
///
/// As [`remove_root`].
pub fn remove_library(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let tx = crate::Nested::open(conn, "remove_library")
        .map_err(|e| format!("starting the library-removal transaction: {e}"))?;
    let (removal, _predicted) = apply_library_removal(&tx, library_id)?;
    tx.commit()
        .map_err(|e| format!("committing the removal of library {library_id}: {e}"))?;
    let derived = crate::derive::derive(conn)
        .map_err(|e| format!("reconciling documents after removing library {library_id}: {e}"))?;
    Ok((removal, derived))
}

// --- exclusion -------------------------------------------------------------

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
///
/// **Was `#[cfg_attr(not(test), expect(dead_code, …))]` on the type and on
/// each of its methods, the same choice `cbz.rs` made for `Plan::zip64` ahead
/// of its own caller landing** — three separate attributes rather than one on
/// the module, because each came off on its own caller's schedule rather than
/// all at once. Task 1 built only this matcher; `scan_root_inner` calling
/// `Excluder::build` and `.excludes` took the attribute off those two and the
/// struct itself, and `forget_unseen` calling `excludes_with_parents` (O92)
/// took it off the last method. Nothing here is dead code any more.
struct Excluder {
    gitignore: ignore::gitignore::Gitignore,
    /// One message per line that would not compile, naming the line.
    complaints: Vec<String>,
}

impl Excluder {
    /// Compile `patterns`, one per line. `None` and `""` both mean "exclude
    /// nothing", which is the path every scan in this repository took before
    /// this existed.
    fn build(patterns: Option<&str>) -> Self {
        let mut builder = ignore::gitignore::GitignoreBuilder::new("");
        let mut complaints = Vec::new();
        for line in patterns.unwrap_or_default().lines() {
            if let Err(why) = builder.add_line(None, line) {
                complaints.push(format!("`{}`: {why}", line.trim()));
            }
        }
        // `build` cannot fail once `add_line` has accepted every line it
        // kept — measured against `ignore` 0.4.33 on 2026-09-09, a builder
        // holding only a rejected line still builds, empty. Handled rather
        // than unwrapped anyway: this module's rule is that a pattern
        // problem is a complaint, never a panic.
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
/// **`taiyaku-serve`'s `PATCH /api/library/roots/{id}` refuses on this**, so
/// an admin hears about a typo at save rather than after a scan. A pattern
/// already stored that will not build is a different matter and is not this
/// function's business — see [`Excluder::build`], which drops it and
/// complains.
///
/// **What this actually catches is narrow, and callers must not imply
/// otherwise.** Measured against `ignore` 0.4.33 on 2026-09-09, `add_line`
/// rejects a dangling `\` and an unclosed `{`. It accepts `a[`, `[a-`,
/// `[[:bogus:]]`, `[]`, `***` and `a**b`. This is a guard against two
/// specific typos, not a check that a pattern means what its author meant.
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
    /// Directories a pattern in `root.exclude_globs` named, which this walk
    /// therefore never descended into — no `read_dir`, no `stat`, no
    /// `sniff`, no `member_pages` for anything beneath them.
    ///
    /// **C4 §3's fourth confession, finally a field.** That section requires
    /// a scan to report "directories skipped by `exclude_globs`" alongside
    /// the containers it could not open and the formats it cannot read; the
    /// build design asserted every item of that discipline "is already a
    /// field", which was true of the other three and false of this one until
    /// now (O92).
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
    /// Nothing about a work, an edition, a member, a reading position or a
    /// note is destroyed by this: nothing references `scan_entry`
    /// (`schema/004_auth.sql`), and `derive`'s only `DELETE FROM work` is a
    /// fold, which needs the material to have a home elsewhere. The works an
    /// exclusion covers are left standing and stale. See O92's entry.
    pub forgotten_by_exclusion: usize,
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
/// both call.** Neither column had ever been written before Task 12, though
/// both have existed since migration 1 and C4 §3's regime line quotes both.
/// Putting the write inside this function rather than in either caller is
/// what keeps the CLI and the route from ever disagreeing about what "the
/// last scan" means for a root — there is exactly one place this fact is
/// decided, and both front ends read the same row afterward. `last_scan_at`
/// is `datetime('now')`, SQLite's own clock rather than this process's, so a
/// scan run from a container with a skewed clock still agrees with every
/// other timestamp the same database already keeps that way. `last_scan_status`
/// is one of three words: `"ok"` (recorded and applied), `"refused"` (§8.1 —
/// nothing was applied, and this call still says so rather than leaving the
/// column to read as "never scanned"), or `"failed"` (an `Err` below — a
/// permission refusal on the root's own directory, a symlink cycle past
/// [`MAX_DEPTH`], or a database statement failing outright). The write itself
/// is a plain, separate statement, issued after whatever transaction the scan
/// itself opened has already committed or rolled back — never inside it —
/// so a scan that failed and rolled back still leaves a truthful `"failed"`
/// behind rather than losing the write along with everything else the
/// transaction undid.
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
/// **Nothing is recorded for a root that is no longer the one walked.** A
/// root can be removed while its walk runs, and its id given to a root added
/// after it. The walk's entries are written only where, inside the
/// transaction that writes them, `root.id` still names `root.path`, and the
/// status write here asks the same, so neither lands on another root.
pub fn scan_root(conn: &Connection, root: &Root) -> Result<ScanReport, String> {
    let result = scan_root_inner(conn, root);
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

/// Correct `last_scan_status` to `"failed"` after a walk this function had
/// nothing to do with has already recorded `"ok"`.
///
/// **The one caller this exists for**: `taiyaku-serve`'s `POST
/// /api/library/roots/{id}/scan` runs [`scan_root`] and then
/// [`fn@crate::derive::derive`] immediately after, one root at a time, and
/// folds both into the one report C4 §3's screen shows. [`scan_root`] itself
/// only ever sees the walk — it writes `"ok"` the moment that commits, before
/// the derive half of the job has even started — so a derive that then fails
/// left that `"ok"` standing: the walk genuinely succeeded, but the library
/// it now describes may be only partly derived (whole-branch review,
/// Important 1). `taiyaku-cli`'s own `library scan` does not need this: it
/// derives once after every root in the batch, not per root, so there is no
/// single root row whose status a derive failure there would need to correct
/// — an `Err` simply fails the whole command.
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
        "UPDATE root SET last_scan_status = 'failed' WHERE id = ?1 AND path = ?2",
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

/// The walk itself — see [`scan_root`], which wraps this to record the
/// outcome on `root.last_scan_at`/`last_scan_status` regardless of which of
/// the three ways out below this function takes.
fn scan_root_inner(conn: &Connection, root: &Root) -> Result<ScanReport, String> {
    let excluder = Excluder::build(root.exclude_globs.as_deref());
    let (measured, walk) = measure(&root.path, 0, &excluder)?;
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
            .map_err(|e| format!("counting what this directory already held: {e}"))?;
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
            // "Remove it" is C4 §2's most destructive action, and it must
            // never be the advice handed to someone whose own patterns did
            // this a moment ago — whole-branch review, Important 3. When the
            // walk excluded nothing, exclusion cannot be the cause and the
            // old remedy stands unchanged.
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
            } else {
                "If the directory has genuinely become empty, remove it.".to_string()
            };
            return Ok(ScanReport {
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
                refused: Some(format!(
                    "{} scanned empty while the library holds {held} entries for it — \
                     refusing to apply. An unmounted share, a bind mount that was not \
                     ready, a renamed directory, or this directory's own exclude_globs \
                     excluding everything beneath it all look exactly like this, and \
                     applying it would drop every work under this directory along with the \
                     reading positions and notes attached to them. {remedy}",
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
    let (gone, forgotten_by_exclusion) = forget_unseen(&tx, root.id, &seen, &held, &excluder)?;
    report.gone = gone;
    report.forgotten_by_exclusion = forgotten_by_exclusion;
    // Last use of `excluder` — moved out here rather than beside the other
    // `walk`-derived fields above, because `forget_unseen` still needs a
    // whole-struct `&excluder` to ask `excludes_with_parents`, and moving
    // `complaints` out of it any earlier leaves `excluder` partially moved
    // and that borrow will not compile.
    report.bad_exclude_patterns = excluder.complaints;
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

/// One row `scan_entry` already holds for this root, as much of it as the
/// walk needs: `(size, mtime)` to decide whether anything must be reopened,
/// and `is_dir` so [`forget_unseen`] can ask [`Excluder`] about a stale path
/// without a second query.
///
/// A named type rather than the `(i64, i64)` tuple this was — the shape
/// [`Seen`], [`Resolved`] and [`Measured`] already use in this module.
///
/// **Used to derive `PartialEq, Eq` it never compared with, only to keep
/// `is_dir` alive under dead-code analysis before this had a reader.**
/// rustc counts a derived `eq` body as a read while explicitly not counting a
/// derived `Clone` ("`Held` has a derived impl for the trait `Clone`, but
/// this is intentionally ignored during dead code analysis") — measured
/// 2026-09-09 — so `PartialEq, Eq` stood in for the reader `forget_unseen`
/// did not have yet. `forget_unseen` now reads `is_dir` directly, to ask
/// [`Excluder::excludes_with_parents`] whether a stale row was forgotten
/// because a pattern covers it (O92), so the field has a real reader and the
/// borrowed derives are gone with it — `Copy` remains because `held_for`'s
/// map is read by value throughout this module.
#[derive(Clone, Copy)]
struct Held {
    size: i64,
    mtime: i64,
    is_dir: bool,
}

/// What `(size, mtime, is_dir)` this root already holds, by `rel_path`.
///
/// One query rather than one per entry, and outside the transaction, so the
/// decision about what needs opening is made before any lock is taken. That
/// also makes it a snapshot: a concurrent writer could change a row between
/// this read and `record`'s, and `record` would branch on the stale value it
/// was handed. No caller does that today — this crate's only caller is its
/// own tests — so nothing closes the window yet; whoever adds a second
/// writer needs to.
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
/// for why those are not the same number.
///
/// Reached only after the §8.1 refusal above, so "the walk met nothing" can
/// never arrive here. Iterates `held` — `held_for`'s result, already read
/// once before the transaction opened — rather than issuing a second
/// `SELECT` for the same rows.
fn forget_unseen(
    conn: &Connection,
    root_id: i64,
    seen: &[Seen],
    held: &HashMap<String, Held>,
    excluder: &Excluder,
) -> Result<(usize, usize), String> {
    let kept: HashSet<&str> = seen.iter().map(|entry| entry.rel_path.as_str()).collect();
    let mut gone = 0;
    let mut forgotten_by_exclusion = 0;
    let stale: Vec<(&str, Held)> = held
        .iter()
        .filter(|(rel_path, _)| !kept.contains(rel_path.as_str()))
        .map(|(rel_path, held)| (rel_path.as_str(), *held))
        .collect();
    for (rel_path, held) in stale {
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
        // Why it is stale, not merely that it is. A row under a directory
        // the walk never descended into has a path that need not match any
        // pattern itself, which is what `excludes_with_parents` is for.
        if excluder.excludes_with_parents(rel_path, held.is_dir) {
            forgotten_by_exclusion += 1;
        } else {
            gone += 1;
        }
    }
    Ok((gone, forgotten_by_exclusion))
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

/// What one walk accumulated besides the tree itself.
///
/// One accumulator rather than three out-params: [`measure_dir`] already
/// threaded `unreadable: &mut Vec<String>`, and exclusion adds two more
/// collections to a signature that was at its limit.
#[derive(Default)]
struct Walk {
    unreadable: Vec<String>,
    excluded_dirs: Vec<String>,
    excluded_files: usize,
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
fn measure(path: &Path, depth: u32, excluder: &Excluder) -> Result<(Measured, Walk), String> {
    let mut walk = Walk::default();
    let measured = measure_dir(path, String::new(), depth, excluder, &mut walk)
        .map_err(MeasureError::into_message)?;
    // `walk.excluded_dirs` is appended in `read_dir` order, which is not
    // stable across identical scans of an unchanged tree — Minor 6. Sorted
    // here, once, over the flat accumulator rather than inside
    // `measure_dir`'s own loop, so the report is deterministic without
    // touching `dir_entries.sort()`, the ordering the walk actually descends
    // in.
    walk.excluded_dirs.sort();
    Ok((measured, walk))
}

/// One directory, measured — see [`measure`]. `walk.unreadable` collects a
/// message for every descendant directory this call meets and cannot list;
/// `path` itself refusing to list is returned as an `Err` instead, which is
/// what lets the caller tell "this one directory is unreadable" from "the
/// directory I was just asked to measure is" — see [`MeasureError`].
fn measure_dir(
    path: &Path,
    rel_path: String,
    depth: u32,
    excluder: &Excluder,
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
        // cost is that `entry_count` no longer equals what `read_dir`
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
        match measure_dir(&child_path, child_rel, depth + 1, excluder, walk) {
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
