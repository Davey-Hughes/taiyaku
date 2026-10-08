//! Libraries: a name and the folders filed under it. Migration 15, and Part F
//! of `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library is born with a folder and goes with its last one.** There is no
//! public way to make an empty one: [`crate::scan::add_root_in`] creates a
//! named library in the same transaction as the folder it is created for, and
//! migration 15's triggers file a root inserted without a library under one of
//! its own and delete a library whose last root goes. A grant names a library
//! and `account::roots_for` resolves it to that library's folders, so an empty
//! library would be a grant resolving to no folder, which `Principal.roots`
//! reads as unrestricted.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension as _};

/// One library, as `taiyaku library libraries` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: i64,
    pub name: String,
    /// Its folders' paths, oldest first.
    pub folders: Vec<PathBuf>,
    /// The works filed under it.
    pub works: usize,
}

/// Every library, by name.
///
/// # Errors
///
/// Returns an error if a read fails.
pub fn list(conn: &Connection) -> Result<Vec<Listed>, String> {
    let mut libraries = conn
        .prepare(
            "SELECT l.id, l.name, (SELECT count(*) FROM work w WHERE w.library_id = l.id)
               FROM library l ORDER BY l.name, l.id",
        )
        .map_err(|e| format!("listing libraries: {e}"))?;
    let rows = libraries
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("listing libraries: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing libraries: {e}"))?;
    let mut folders = conn
        .prepare("SELECT path FROM root WHERE library_id = ?1 ORDER BY id")
        .map_err(|e| format!("listing a library's folders: {e}"))?;
    let mut listed = Vec::with_capacity(rows.len());
    for (id, name, works) in rows {
        let paths = folders
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("listing the folders of `{name}`: {e}"))?
            .map(|row| row.map(PathBuf::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("listing the folders of `{name}`: {e}"))?;
        listed.push(Listed {
            id,
            name,
            folders: paths,
            works: usize::try_from(works).unwrap_or(usize::MAX),
        });
    }
    Ok(listed)
}

/// `name` with spaces stripped from both ends, and nothing else stripped.
///
/// **One character set wherever a library name is made or looked up.**
/// Migration 15's trigger and backfill name a library with SQLite's
/// one-argument `trim`, which strips spaces only, so Rust strips the same:
/// `str::trim` would also strip tabs and other whitespace the trigger keeps,
/// and a name stripped one way could not be found by a lookup stripped the
/// other.
pub(crate) fn trim_name(name: &str) -> &str {
    name.trim_matches(' ')
}

/// The library called `name`, if there is one: the library holding exactly
/// `name`, and otherwise the one holding `name` stripped of spaces as
/// [`trim_name`] strips them.
///
/// [`create`] and migration 15's trigger store a name stripped, so the
/// stripped lookup finds a name typed with a space too many. Migration 15's
/// backfill keeps the spaces of folders whose whole paths differ only in
/// surrounding spaces, and the exact lookup comes first for those: such a
/// name answers its own library, not the one holding the same name without
/// the spaces.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn id_of(conn: &Connection, name: &str) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM library WHERE name IN (?1, ?2) ORDER BY name = ?1 DESC LIMIT 1",
        [name, trim_name(name)],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("looking up the library `{name}`: {e}"))
}

/// A new library called `name`, stripped of spaces by [`trim_name`].
/// **Crate-only**, so a library is only ever made together with its first
/// folder; see this module's doc.
///
/// # Errors
///
/// Returns an error if `name` is empty once trimmed, or taken.
pub(crate) fn create(conn: &Connection, name: &str) -> Result<i64, String> {
    // Any whitespace counts as no name, not only spaces: refusing a name that
    // would be invisible is not stripping one, so the stored name keeps
    // `trim_name`'s set.
    if name.trim().is_empty() {
        return Err("a library needs a name".to_string());
    }
    let name = trim_name(name);
    conn.execute("INSERT INTO library (name) VALUES (?1)", [name])
        .map_err(|e| format!("creating the library `{name}`: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// The name of library `id`, if there is one.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn name_of(conn: &Connection, id: i64) -> Result<Option<String>, String> {
    conn.query_row("SELECT name FROM library WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(|e| format!("looking up library {id}: {e}"))
}

/// Why [`rename`] would not rename a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    /// No library has this id.
    Missing(i64),
    /// The caller's own mistake: no name once trimmed, or one another library
    /// holds.
    Unusable(String),
    /// A store fault.
    Failed(String),
}

/// Rename library `id`, storing `name` stripped by [`trim_name`], and answer
/// the name stored.
///
/// **`library.name`'s UNIQUE is what refuses a name another library holds.**
/// A lookup before the write would be a second statement for a second writer
/// to land between, and the CLI is one.
///
/// # Errors
///
/// See [`RenameError`].
pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<String, RenameError> {
    if name.trim().is_empty() {
        return Err(RenameError::Unusable("a library needs a name".to_string()));
    }
    let name = trim_name(name);
    match conn.execute(
        "UPDATE library SET name = ?2 WHERE id = ?1",
        rusqlite::params![id, name],
    ) {
        Ok(0) => Err(RenameError::Missing(id)),
        Ok(_) => Ok(name.to_string()),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            Err(RenameError::Unusable(format!(
                "a library is already called `{name}`"
            )))
        }
        Err(e) => Err(RenameError::Failed(format!("renaming library {id}: {e}"))),
    }
}

/// How many entries the scans of library `id`'s folders recorded as an EPUB
/// (fixed-layout or reflowable) or a PDF, which derivation does not admit.
/// Entries in other formats it does not admit, such as rar, 7z and tar, are not
/// counted.
///
/// The Libraries screen says "no readable files yet" on this, for a library
/// that holds such files and no work.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn books(conn: &Connection, id: i64) -> Result<usize, String> {
    use taiyaku_read::Format;
    conn.query_row(
        "SELECT count(*) FROM scan_entry e JOIN root r ON r.id = e.root_id
          WHERE r.library_id = ?1 AND e.format IN (?2, ?3, ?4)",
        rusqlite::params![
            id,
            Format::EpubFixed.token(),
            Format::EpubReflow.token(),
            Format::Pdf.token(),
        ],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    .map_err(|e| format!("counting the books of library {id}: {e}"))
}

/// SQL that is true of a `work` row library `library` may derive onto: a work
/// filed under it, or a work in no library that it may take.
///
/// **A work in no library is taken only by a library whose folders hold
/// it**: one with a member under a root of `library`. A work with no member
/// under any root, an imported one for instance, may be taken by any library.
/// A work whose members are under roots of other libraries alone is not this
/// library's to take, whatever its key, series or volume.
///
/// `work` is the row's table or alias in the statement this is spliced into,
/// and `library` the placeholder the library's id is bound to there. Every
/// statement by which a library finds a work takes this text, so they cannot
/// differ: `derive::held_work`'s two, and `import::find_by_key`'s when it is
/// given a library. Each orders the library's own row before one in none,
/// and the older before the younger, and one row is filed.
pub(crate) fn own_or_adoptable_work(work: &str, library: &str) -> String {
    own_or_adoptable(
        work,
        library,
        &format!(
            "edition adopt_e
               JOIN member adopt_m
                 ON adopt_m.edition_id = adopt_e.id AND adopt_e.work_id = {work}.id"
        ),
    )
}

/// [`own_or_adoptable_work`] for a `series` row: a series filed under
/// `library`, or a series in no library that it may take.
///
/// **A series in no library goes with its works' folders**: `library` may
/// take one where a work of the series has a member under a root of
/// `library`, or where no work of the series has a member under any root,
/// which includes a series holding no work.
///
/// Spliced into `derive::reconcile_series`'s statement, and into
/// `librarydoc::find_series_by_title`'s when it is given a library.
pub(crate) fn own_or_adoptable_series(series: &str, library: &str) -> String {
    own_or_adoptable(
        series,
        library,
        &format!(
            "work adopt_w
               JOIN edition adopt_e
                 ON adopt_e.work_id = adopt_w.id AND adopt_w.series_id = {series}.id
               JOIN member adopt_m ON adopt_m.edition_id = adopt_e.id"
        ),
    )
}

/// The shape both rules share. `members` is a `FROM` clause yielding, as
/// `adopt_m`, the members that say where `row` is held.
fn own_or_adoptable(row: &str, library: &str, members: &str) -> String {
    format!(
        "({row}.library_id = {library}
          OR ({row}.library_id IS NULL
              AND (NOT EXISTS (SELECT 1 FROM {members} WHERE adopt_m.root_id IS NOT NULL)
                   OR EXISTS (SELECT 1 FROM {members}
                                JOIN root adopt_r ON adopt_r.id = adopt_m.root_id
                               WHERE adopt_r.library_id = {library}))))"
    )
}

/// How many works are filed under no library.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn unfiled_works(conn: &Connection) -> Result<usize, String> {
    conn.query_row(
        "SELECT count(*) FROM work WHERE library_id IS NULL",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    .map_err(|e| format!("counting works in no library: {e}"))
}

#[cfg(test)]
mod tests;
