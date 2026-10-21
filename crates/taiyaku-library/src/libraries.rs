//! Libraries: a name and the directories filed under it. Migration 15, and Part F
//! of `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library is born with a directory and goes with its last one.** There is no
//! public way to make an empty one: [`crate::scan::add_root_in`] creates a
//! named library in the same transaction as the directory it is created for, and
//! the `root_gets_a_library` trigger (migration 16) files a root inserted
//! without a library under one of its own, and migration 15's
//! `library_emptied_by_delete` deletes a library whose last root goes. A grant
//! names a library and `account::access_for` resolves it to that library's
//! directories, so a grant of an empty library would grant nothing.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension as _};

/// One library, as `taiyaku library libraries` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: i64,
    pub name: String,
    /// Its directories' paths, oldest first.
    pub dirs: Vec<PathBuf>,
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
    let mut dirs = conn
        .prepare("SELECT path FROM root WHERE library_id = ?1 ORDER BY id")
        .map_err(|e| format!("listing a library's directories: {e}"))?;
    let mut listed = Vec::with_capacity(rows.len());
    for (id, name, works) in rows {
        let paths = dirs
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("listing the directories of `{name}`: {e}"))?
            .map(|row| row.map(PathBuf::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("listing the directories of `{name}`: {e}"))?;
        listed.push(Listed {
            id,
            name,
            dirs: paths,
            works: usize::try_from(works).unwrap_or(usize::MAX),
        });
    }
    Ok(listed)
}

/// `name` with white space stripped from both ends: the characters Unicode
/// calls White_Space, which `str::trim` strips.
///
/// **One character set wherever a library name is made or looked up.**
/// Migration 16's trigger names a library with SQLite's two-argument `trim`
/// over the same characters, listed by code point, so a name made by either
/// is found by a lookup made through this one. `libraries/tests.rs` checks the
/// lists against this.
pub(crate) fn trim_name(name: &str) -> &str {
    name.trim()
}

/// The library called `name`, if there is one: the library whose name is
/// `name` stripped by [`trim_name`], in any ASCII case.
///
/// At most one answers. A name is stored stripped, and migration 16's index
/// refuses two names that differ only in ASCII case.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn id_of(conn: &Connection, name: &str) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM library WHERE name = ?1 COLLATE NOCASE",
        [trim_name(name)],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("looking up the library `{name}`: {e}"))
}

/// A new library called `name`, stripped by [`trim_name`].
/// **Crate-only**, so a library is only ever made together with its first
/// directory; see this module's doc.
///
/// # Errors
///
/// Returns an error if `name` is empty once trimmed, or taken.
pub(crate) fn create(conn: &Connection, name: &str) -> Result<i64, String> {
    let name = trim_name(name);
    if name.is_empty() {
        return Err("a library needs a name".to_string());
    }
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
/// **`library.name`'s UNIQUE and migration 16's index are what refuse a name
/// another library holds**, in any ASCII case. A lookup before the write
/// would be a second statement for a second writer to land between, and the
/// CLI is one. The refusal names the other library as it is stored, which is
/// not `name` where only the case differs.
///
/// # Errors
///
/// See [`RenameError`].
pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<String, RenameError> {
    let name = trim_name(name);
    if name.is_empty() {
        return Err(RenameError::Unusable("a library needs a name".to_string()));
    }
    match conn.execute(
        "UPDATE library SET name = ?2 WHERE id = ?1",
        rusqlite::params![id, name],
    ) {
        Ok(0) => Err(RenameError::Missing(id)),
        Ok(_) => Ok(name.to_string()),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            let holder: Option<String> = conn
                .query_row(
                    "SELECT name FROM library WHERE name = ?1 COLLATE NOCASE AND id <> ?2",
                    rusqlite::params![name, id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| RenameError::Failed(format!("renaming library {id}: {e}")))?;
            Err(RenameError::Unusable(format!(
                "a library is already called `{}`",
                holder.as_deref().unwrap_or(name)
            )))
        }
        Err(e) => Err(RenameError::Failed(format!("renaming library {id}: {e}"))),
    }
}

/// How many entries the scans of library `id`'s directories recorded as an EPUB
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
/// **A work in no library is taken only by a library whose directories hold
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
/// **A series in no library goes with its works' directories**: `library` may
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
