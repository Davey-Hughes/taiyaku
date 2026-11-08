//! Libraries: a name and the directories filed under it. Migration 15, and Part F
//! of `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library is born with a directory and goes with its last one.** There is no
//! public way to make an empty one: [`crate::scan::add_root_in`] creates a
//! library, named or of the directory's own, in the same transaction as the
//! directory it is created for and the `library_root` link between them, and
//! migration 19's `library_emptied_by_unlink` and `library_emptied_by_move`
//! delete a library whose last link goes. A grant
//! names a library and `account::access_for` resolves it to the directories
//! that library reaches, so a grant of an empty library would grant nothing.
//!
//! **What a library holds is read through the directories it reaches**
//! ([`crate::reach::of_libraries`]): its own, and each directory of another
//! library nested in one of them that its walk would reach. Its works count,
//! its readable-file count and its share of a split edition all count a file
//! under `/m/kids` for Manga as well as for Kids. Its directory list is its
//! own directories alone.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension as _};

/// One library, as `taiyaku library libraries` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: i64,
    pub name: String,
    /// Its directories' paths, oldest first.
    pub dirs: Vec<PathBuf>,
    /// The works with a member under one of the directories it reaches. A
    /// work with an edition under each of two libraries' directories is counted
    /// in both, and so is a work whose one member has a copy under each, or is
    /// under a directory nested in the other's.
    pub works: usize,
}

/// Every library, by name.
///
/// # Errors
///
/// Returns an error if a read fails.
pub fn list(conn: &Connection) -> Result<Vec<Listed>, String> {
    let mut libraries = conn
        .prepare("SELECT id, name FROM library ORDER BY name, id")
        .map_err(|e| format!("listing libraries: {e}"))?;
    let rows = libraries
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| format!("listing libraries: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing libraries: {e}"))?;
    let roots = crate::scan::roots(conn)?;
    let mut dirs = conn
        .prepare(
            "SELECT r.path FROM library_root lr JOIN root r ON r.id = lr.root_id
              WHERE lr.library_id = ?1 ORDER BY r.id",
        )
        .map_err(|e| format!("listing a library's directories: {e}"))?;
    let mut listed = Vec::with_capacity(rows.len());
    for (id, name) in rows {
        let works = works_under(conn, &crate::reach::of_libraries(&roots, &[id]))
            .map_err(|e| format!("counting the works of `{name}`: {e}"))?;
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
            works,
        });
    }
    Ok(listed)
}

/// How many works have a member under one of `dirs`.
fn works_under(conn: &Connection, dirs: &[i64]) -> rusqlite::Result<usize> {
    if dirs.is_empty() {
        return Ok(0);
    }
    conn.query_row(
        &format!(
            "SELECT count(DISTINCT e.work_id)
               FROM edition e
               JOIN member m ON m.edition_id = e.id
               JOIN member_root mr ON mr.member_id = m.id
              WHERE mr.root_id IN ({})",
            crate::browse::holes(dirs.len())
        ),
        rusqlite::params_from_iter(dirs),
        |r| r.get::<_, i64>(0),
    )
    .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
}

/// `first`, "`first` and `b`", or "`first`, `b` and `c`": names in a sentence,
/// as every answer that names several libraries lists them, so the store's
/// refusals and the CLI's lines cannot drift apart. Never empty: a list of no
/// library is a caller's fault to answer, not a sentence.
#[must_use]
pub fn listed<S: AsRef<str>>(first: &str, rest: &[S]) -> String {
    let mut text = first.to_string();
    if let Some((last, between)) = rest.split_last() {
        for name in between {
            text.push_str(", ");
            text.push_str(name.as_ref());
        }
        text.push_str(" and ");
        text.push_str(last.as_ref());
    }
    text
}

/// `name` with white space stripped from both ends: the characters Unicode
/// calls White_Space, which `str::trim` strips.
///
/// **One character set wherever a library name is made or looked up.**
/// Migration 16 stripped the names already stored with SQLite's two-argument
/// `trim` over the same characters, listed by code point, so a name it left is
/// found by a lookup made through this one. `libraries/tests.rs` checks the
/// lists against this.
pub(crate) fn trim_name(name: &str) -> &str {
    name.trim()
}

/// `name` stripped by [`trim_name`], as a library would store it, or why no
/// library can be called that: it is empty, or [`crate::has_no_address`].
///
/// **Asked of the name as it would be stored**, so ` . ` is refused: it is
/// stored as `.`. Every path that makes or renames a library asks this before
/// it writes, so each refuses the same names as the caller's mistake rather
/// than as a fault inside the write.
pub(crate) fn usable_name(name: &str) -> Result<&str, &'static str> {
    let name = trim_name(name);
    if name.is_empty() {
        Err("a library needs a name")
    } else if crate::has_no_address(name) {
        Err(crate::NO_ADDRESS)
    } else {
        Ok(name)
    }
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
/// Returns an error if [`usable_name`] refuses `name`, or if it is taken.
pub(crate) fn create(conn: &Connection, name: &str) -> Result<i64, String> {
    let name = usable_name(name).map_err(str::to_string)?;
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
    /// The caller's own mistake: a name [`usable_name`] refuses, or one
    /// another library holds.
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
    let name = usable_name(name).map_err(|why| RenameError::Unusable(why.to_string()))?;
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

/// How many entries the scans of the directories library `id` reaches
/// recorded as an EPUB (fixed-layout or reflowable) or a PDF, which derivation
/// does not admit. Entries in other formats it does not admit, such as rar, 7z
/// and tar, are not counted.
///
/// The Libraries screen says "no readable files yet" on this, for a library
/// that holds such files and no work.
///
/// # Errors
///
/// Returns an error if a read fails.
pub fn books(conn: &Connection, id: i64) -> Result<usize, String> {
    use taiyaku_read::Format;
    let dirs = crate::reach::of_libraries(&crate::scan::roots(conn)?, &[id]);
    if dirs.is_empty() {
        return Ok(0);
    }
    let formats = [
        Format::EpubFixed.token(),
        Format::EpubReflow.token(),
        Format::Pdf.token(),
    ];
    let mut bound: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(formats.len() + dirs.len());
    bound.extend(formats.iter().map(|token| token as &dyn rusqlite::ToSql));
    bound.extend(dirs.iter().map(|dir| dir as &dyn rusqlite::ToSql));
    conn.query_row(
        &format!(
            "SELECT count(*) FROM scan_entry e
              WHERE e.format IN (?, ?, ?) AND e.root_id IN ({})",
            crate::browse::holes(dirs.len())
        ),
        bound.as_slice(),
        |r| r.get::<_, i64>(0),
    )
    .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    .map_err(|e| format!("counting the books of library {id}: {e}"))
}

/// How many works are in no library: those no member of which is under a
/// directory, as every member of an imported work is.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn unfiled_works(conn: &Connection) -> Result<usize, String> {
    conn.query_row(
        "SELECT count(*) FROM work w
          WHERE NOT EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                             JOIN member_root mr ON mr.member_id = m.id
                             WHERE e.work_id = w.id)",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    .map_err(|e| format!("counting works in no library: {e}"))
}

/// An edition some library holds part of and not all of, which an account
/// granted that library and not the others sees narrowed (spec §3.8).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SplitEdition {
    /// The work's slug.
    pub work: String,
    pub title: String,
    /// The edition's key.
    pub edition: String,
    /// Every member's pages.
    pub pages: i64,
    /// In ordinal order.
    pub members: Vec<SplitMember>,
    /// By name: each library reaching a directory that holds one of the
    /// members.
    pub libraries: Vec<SplitLibrary>,
}

/// A member of a [`SplitEdition`] and the directories holding a copy of it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SplitMember {
    /// As `member.path` stores it.
    pub path: String,
    pub pages: i64,
    /// By path.
    pub directories: Vec<MemberDirectory>,
}

/// A directory holding a copy of a member, and every library it is in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemberDirectory {
    pub root: i64,
    pub path: String,
    /// Ascending.
    pub libraries: Vec<i64>,
}

/// A library holding some of a [`SplitEdition`]'s members, under the
/// directories it reaches: how many, and their pages, which is what an account
/// granted it alone is sent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SplitLibrary {
    pub id: i64,
    pub name: String,
    pub members: usize,
    pub pages: i64,
}

/// Every edition some library holds part of and not all of, by work title as
/// the rest of the UI sorts one (`COLLATE NOCASE`), then edition key.
///
/// **Members, not bytes.** A byte-identical copy is one member held under a
/// directory of each library, so it splits nothing. A member under no
/// directory, as an imported work's are, is held by no library.
///
/// **Each library by the directories it reaches** ([`crate::reach::of_libraries`]),
/// as an account granted it alone is sent them: an edition with `c001` under
/// `/m` and `c002` under Kids' `/m/kids` is whole for Manga and split for Kids.
///
/// # Errors
///
/// Returns an error if a read fails, or if a member's directory is in no
/// library, which no writer leaves.
pub fn split_editions(conn: &Connection) -> Result<Vec<SplitEdition>, String> {
    let failed = |e: rusqlite::Error| format!("listing the split editions: {e}");
    let roots = crate::scan::roots(conn)?;
    let names: Vec<(i64, String)> = conn
        .prepare("SELECT id, name FROM library ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(failed)?;
    // `(library, directory, the library's name)` for each directory each
    // library reaches.
    let reached: Vec<(i64, i64, &str)> = names
        .iter()
        .flat_map(|(library, name)| {
            crate::reach::of_libraries(&roots, &[*library])
                .into_iter()
                .map(|dir| (*library, dir, name.as_str()))
        })
        .collect();
    // No library reaches a directory, so none holds a member.
    if reached.is_empty() {
        return Ok(Vec::new());
    }
    let pairs = std::iter::repeat_n("(?, ?)", reached.len())
        .collect::<Vec<_>>()
        .join(", ");
    let found = conn
        .prepare(&format!(
            "WITH reach (library_id, root_id) AS (VALUES {pairs})
             SELECT e.id, w.slug, w.title, e.key FROM edition e JOIN work w ON w.id = e.work_id
              WHERE EXISTS (SELECT 1 FROM library l
                WHERE EXISTS (SELECT 1 FROM member m
                                JOIN member_root mr ON mr.member_id = m.id
                                JOIN reach r ON r.root_id = mr.root_id
                               WHERE m.edition_id = e.id AND r.library_id = l.id)
                  AND EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id
                                AND NOT EXISTS (SELECT 1 FROM member_root mr
                                                  JOIN reach r ON r.root_id = mr.root_id
                                                 WHERE mr.member_id = m.id
                                                   AND r.library_id = l.id)))
              ORDER BY w.title COLLATE NOCASE, e.key, w.slug"
        ))
        .and_then(|mut stmt| {
            let bound = reached.iter().flat_map(|&(library, dir, _)| [library, dir]);
            stmt.query_map(rusqlite::params_from_iter(bound), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .map_err(failed)?;
    let mut members = conn
        .prepare("SELECT id, path, pages FROM member WHERE edition_id = ?1 ORDER BY ordinal")
        .map_err(failed)?;
    // Each directory's libraries in the statement that finds it, so a
    // directory added since `roots` was read is named as it stands, and once:
    // a join through `library_root` would answer it once per link.
    let mut directories = conn
        .prepare(
            "SELECT r.id, r.path,
                    (SELECT group_concat(lr.library_id) FROM library_root lr
                      WHERE lr.root_id = r.id)
               FROM member_root mr
               JOIN root r ON r.id = mr.root_id
              WHERE mr.member_id = ?1 ORDER BY r.path, r.id",
        )
        .map_err(failed)?;
    let libraries_of = |path: &str, listed: Option<String>| -> Result<Vec<i64>, String> {
        let mut libraries = listed
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter(|id| !id.is_empty())
            .map(|id| {
                id.parse::<i64>()
                    .map_err(|e| format!("reading the libraries of {path}: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if libraries.is_empty() {
            return Err(format!("directory {path} is in no library"));
        }
        libraries.sort_unstable();
        Ok(libraries)
    };
    let mut split = Vec::with_capacity(found.len());
    for (edition_id, work, title, edition) in found {
        let held = members
            .query_map([edition_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
            .map_err(failed)?;
        let mut listed = Vec::with_capacity(held.len());
        let mut libraries: Vec<SplitLibrary> = Vec::new();
        for (member_id, path, pages) in held {
            let under = directories
                .query_map([member_id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })
                .and_then(Iterator::collect::<Result<Vec<_>, _>>)
                .map_err(failed)?
                .into_iter()
                .map(|(root, path, listed)| {
                    Ok(MemberDirectory {
                        root,
                        libraries: libraries_of(&path, listed)?,
                        path,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            // A member under two directories one library reaches is one of its
            // members, not two.
            let mut counted: Vec<i64> = Vec::new();
            for &(holder, _, name) in reached
                .iter()
                .filter(|(_, dir, _)| under.iter().any(|directory| directory.root == *dir))
            {
                if counted.contains(&holder) {
                    continue;
                }
                counted.push(holder);
                match libraries.iter_mut().find(|l| l.id == holder) {
                    Some(library) => {
                        library.members += 1;
                        library.pages += pages;
                    }
                    None => libraries.push(SplitLibrary {
                        id: holder,
                        name: name.to_string(),
                        members: 1,
                        pages,
                    }),
                }
            }
            listed.push(SplitMember {
                path,
                pages,
                directories: under,
            });
        }
        libraries.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        split.push(SplitEdition {
            work,
            title,
            edition,
            pages: listed.iter().map(|member| member.pages).sum(),
            members: listed,
            libraries,
        });
    }
    Ok(split)
}

#[cfg(test)]
mod tests;
