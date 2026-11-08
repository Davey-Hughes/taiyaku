//! Moving a directory from one library to another: §3.5 of
//! `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md`.
//!
//! **A move relabels the directory and writes nothing else.** It points the
//! directory's link to the library it leaves, which the caller names, at the
//! target, making the target first where a new library is named.
//! A library's works, its counts and an account's access are all read
//! through the directories a work's members are under, so every work,
//! edition, member, position and note stays the row it was, and a work with
//! an edition on each side is in both libraries. Migration 19's
//! `library_emptied_by_move` removes a library the move empties, and its
//! grants with it.
//!
//! **What changes is who sees the directory**, so that is what a move answers:
//! the accounts that gain sight of it and the accounts that lose it, each
//! account's sight read by `account::access_for`'s rule, through the
//! directories its grants reach, before the move and after it.
//!
//! **A move into a library is refused where that library already holds the
//! directory** (decision 25), **and inside or around one of its directories**
//! (`scan::beside`, in the add's words). Another library's directory may still
//! hold it, or sit inside it. The add's directory-member refusal is not asked:
//! it does not depend on the library.
//!
//! **A rehearsal is the move, rolled back**, as `scan::removal_cost` is a
//! removal rolled back.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension as _};

/// Where a directory is moved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// A library that exists, by its id.
    Library(i64),
    /// A library made for the directory. A library is born with a directory, so
    /// this is the only way to give a directory one of its own again.
    New(&'a str),
}

/// Why a move was not made. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    /// No such directory, no such library, or a source that does not hold the
    /// directory.
    Missing(String),
    /// The caller's own mistake: a library that already holds the directory,
    /// one with a directory it is inside or around (`scan::beside`), or a name for a new
    /// one that is empty, taken, or one a page address cannot hold
    /// (`crate::has_no_address`).
    Unusable(String),
    /// The move cannot be made now. This module refuses none: its caller
    /// does, while a scan, a rule save, a stamp or another move is in the way.
    /// `volumes` is always empty, because a move relabels a directory and no
    /// volume can stand in its way; it stays so the 409's body keeps the shape
    /// clients read.
    Refused { why: String, volumes: Vec<String> },
    /// A store fault.
    Failed(String),
}

/// The library directory (a `root` row) being moved, as it stands before the
/// move.
struct Dir {
    id: i64,
    path: String,
    /// The library the move takes the directory out of, and its name.
    from: i64,
    from_name: String,
    /// Every library linking it, ascending, `from` among them.
    libraries: Vec<i64>,
}

/// Root `root_id`, leaving library `from`, which must hold it: a page older
/// than a link or an unlink names a library it is no longer in.
fn dir(conn: &Connection, root_id: i64, from: i64) -> Result<Dir, MoveError> {
    let failed =
        |e: rusqlite::Error| MoveError::Failed(format!("reading directory {root_id}: {e}"));
    let path: String = conn
        .query_row("SELECT path FROM root WHERE id = ?1", [root_id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(failed)?
        .ok_or_else(|| MoveError::Missing(format!("there is no directory {root_id}")))?;
    let from_name = crate::libraries::name_of(conn, from)
        .map_err(MoveError::Failed)?
        .ok_or_else(|| MoveError::Missing(format!("there is no library {from}")))?;
    let libraries = conn
        .prepare("SELECT library_id FROM library_root WHERE root_id = ?1 ORDER BY library_id")
        .and_then(|mut stmt| {
            stmt.query_map([root_id], |r| r.get(0))?
                .collect::<Result<Vec<i64>, _>>()
        })
        .map_err(failed)?;
    if !libraries.contains(&from) {
        return Err(MoveError::Missing(format!(
            "`{path}` is not in {from_name}"
        )));
    }
    Ok(Dir {
        id: root_id,
        path,
        from,
        from_name,
        libraries,
    })
}

/// Whether root `root_id` is there to leave library `from`: the refusals a
/// move makes first, in its words, for a caller to ask before it waits on
/// anything, since a page older than an unlink names a `from` it has left.
///
/// # Errors
///
/// [`MoveError::Missing`] for no such directory or library, or a `from` that
/// does not hold the directory; [`MoveError::Failed`] for a store fault.
pub fn check_source(conn: &Connection, root_id: i64, from: i64) -> Result<(), MoveError> {
    dir(conn, root_id, from).map(|_| ())
}

/// The id of the library `target` names, making it where a new one is named.
/// Made inside the caller's transaction, so a refused or rehearsed move
/// leaves no library behind.
fn resolve(conn: &Connection, dir: &Dir, target: Target<'_>) -> Result<i64, MoveError> {
    match target {
        Target::Library(id) => {
            let name = crate::libraries::name_of(conn, id)
                .map_err(MoveError::Failed)?
                .ok_or_else(|| MoveError::Missing(format!("there is no library {id}")))?;
            // The source among them. Written as an UPDATE, a move here would
            // fail on the link's key.
            if dir.libraries.contains(&id) {
                return Err(MoveError::Unusable(format!(
                    "`{}` is already in {name}",
                    dir.path
                )));
            }
            Ok(id)
        }
        Target::New(name) => {
            if let Err(why) = crate::libraries::usable_name(name) {
                return Err(MoveError::Unusable(why.to_string()));
            }
            if let Some(taken) = crate::libraries::id_of(conn, name).map_err(MoveError::Failed)? {
                let stored = crate::libraries::name_of(conn, taken)
                    .map_err(MoveError::Failed)?
                    .unwrap_or_default();
                return Err(MoveError::Unusable(format!(
                    "a library is already called `{stored}`; move the directory to it instead"
                )));
            }
            crate::libraries::create(conn, name).map_err(MoveError::Failed)
        }
    }
}

/// What a move did, or in a rehearsal would do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Moved {
    /// The library the directory is in afterwards, and its name.
    pub library_id: i64,
    pub library: String,
    /// The accounts, by name and in name order, that see the directory after
    /// the move and not before: whose grants reach it afterwards
    /// ([`crate::account::access_for`]) and did not. An account granted a
    /// library that reaches this directory from one around it sees it on both
    /// sides, so it is not named for a grant of either library. An
    /// administrator and an every-library account see every directory before
    /// and after, so neither is named, as in `scan::only_grant_holders`.
    pub gains_sight: Vec<String>,
    /// The accounts that see the directory before the move and not after.
    /// Named as `gains_sight` names them.
    pub loses_sight: Vec<String>,
    /// The library the directory left, where it was that library's last
    /// directory, and the grants that went with it.
    pub library_removed: Option<String>,
    pub grants_removed: usize,
    /// The accounts, by name, whose only grant went with that library, and
    /// who see no library afterwards: `scan::only_grant_holders`, asked
    /// before the move. Empty where no library is removed.
    pub left_with_no_library: Vec<String>,
}

/// The accounts that see root `root_id` as the store stands, in name order,
/// under `scan::only_grant_holders`'s filter: an account whose role is `user`
/// and that does not see every library, the only kind a grant limits. Each
/// account's sight is [`crate::account::access_for`]'s, so a move or an
/// unlink names what the account is then served.
fn sighted(conn: &Connection, root_id: i64) -> Result<Vec<String>, String> {
    let failed = |e: rusqlite::Error| format!("reading who sees root {root_id}: {e}");
    let limited: Vec<(i64, String)> = conn
        .prepare(
            "SELECT id, name FROM user WHERE role = 'user' AND every_library = 0 ORDER BY name",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(failed)?;
    let mut seeing = Vec::new();
    for (id, name) in limited {
        if crate::account::access_for(conn, id)?.allows(root_id) {
            seeing.push(name);
        }
    }
    Ok(seeing)
}

/// What taking a directory out of one library does to who sees it, and to
/// that library where it was its last directory: the half [`Moved`] and
/// [`crate::scan::Unlinked`] share, under the same names, so a move's dialog
/// and an unlink's cannot answer differently.
pub(crate) struct Leaving {
    pub(crate) gains_sight: Vec<String>,
    pub(crate) loses_sight: Vec<String>,
    pub(crate) library_removed: Option<String>,
    pub(crate) grants_removed: usize,
    pub(crate) left_with_no_library: Vec<String>,
}

/// [`Leaving`] for `write`, which takes root `root_id` out of library `from`,
/// called `from_name`, inside the caller's transaction. All but the sight
/// lists is read before `write`, while a library it empties and its grants
/// are still there to read; who sees the directory is read on both sides.
pub(crate) fn leaving(
    conn: &Connection,
    root_id: i64,
    (from, from_name): (i64, String),
    write: impl FnOnce() -> Result<(), String>,
) -> Result<Leaving, String> {
    let failed = |e: rusqlite::Error| format!("reading what leaving library {from} takes: {e}");
    let saw = sighted(conn, root_id)?;
    let last = conn
        .query_row(
            "SELECT NOT EXISTS (SELECT 1 FROM library_root
                                 WHERE library_id = ?1 AND root_id <> ?2)",
            [from, root_id],
            |r| r.get::<_, bool>(0),
        )
        .map_err(failed)?;
    let (grants_removed, left_with_no_library) = if last {
        let grants = conn
            .query_row(
                "SELECT count(*) FROM user_library WHERE library_id = ?1",
                [from],
                |r| r.get::<_, i64>(0),
            )
            .map_err(failed)?;
        (
            usize::try_from(grants).unwrap_or(usize::MAX),
            crate::scan::only_grant_holders(conn, &[from])?,
        )
    } else {
        (0, Vec::new())
    };
    write()?;
    let sees = sighted(conn, root_id)?;
    Ok(Leaving {
        gains_sight: sees
            .iter()
            .filter(|name| !saw.contains(name))
            .cloned()
            .collect(),
        loses_sight: saw
            .into_iter()
            .filter(|name| !sees.contains(name))
            .collect(),
        library_removed: last.then_some(from_name),
        grants_removed,
        left_with_no_library,
    })
}

/// The move out of library `from`, inside the caller's transaction.
fn apply_move(
    conn: &Connection,
    root_id: i64,
    from: i64,
    target: Target<'_>,
) -> Result<Moved, MoveError> {
    let dir = dir(conn, root_id, from)?;
    let target_id = resolve(conn, &dir, target)?;
    // A new library holds no directory to be inside or around.
    if let Target::Library(_) = target {
        let refused = crate::scan::beside(
            conn,
            &crate::scan::roots(conn).map_err(MoveError::Failed)?,
            Path::new(&dir.path),
            Some(target_id),
            Some(dir.id),
        )
        .map_err(MoveError::Failed)?;
        if let Some(why) = refused {
            return Err(MoveError::Unusable(why));
        }
    }
    let library = crate::libraries::name_of(conn, target_id)
        .map_err(MoveError::Failed)?
        .unwrap_or_default();
    let left = leaving(conn, dir.id, (dir.from, dir.from_name), || {
        // An UPDATE of the source's link, never the delete first: in a
        // directory no other library holds, the delete would be its last
        // link, which migration 19 refuses.
        conn.execute(
            "UPDATE library_root SET library_id = ?3 WHERE library_id = ?2 AND root_id = ?1",
            [dir.id, dir.from, target_id],
        )
        .map(|_| ())
        .map_err(|e| format!("moving root {root_id}: {e}"))
    })
    .map_err(MoveError::Failed)?;
    Ok(Moved {
        library_id: target_id,
        library,
        gains_sight: left.gains_sight,
        loses_sight: left.loses_sight,
        library_removed: left.library_removed,
        grants_removed: left.grants_removed,
        left_with_no_library: left.left_with_no_library,
    })
}

/// What moving root `root_id` out of library `from` to `target` would do,
/// found by doing it inside a transaction this rolls back.
///
/// # Errors
///
/// See [`MoveError`]. A move that would fail fails here in the same words.
pub fn move_cost(
    conn: &Connection,
    root_id: i64,
    from: i64,
    target: Target<'_>,
) -> Result<Moved, MoveError> {
    let tx = crate::Nested::open(conn, "move_cost")
        .map_err(|e| MoveError::Failed(format!("starting to rehearse a move: {e}")))?;
    let answer = apply_move(&tx, root_id, from, target);
    // Not committed: dropping `tx` is the rollback, whatever the answer.
    drop(tx);
    answer
}

/// Move root `root_id` out of library `from` to `target`, and commit. Nothing
/// is derived and no file is written: no row a work is made of changes.
///
/// # Errors
///
/// See [`MoveError`]. A failed move changed nothing.
pub fn move_root(
    conn: &Connection,
    root_id: i64,
    from: i64,
    target: Target<'_>,
) -> Result<Moved, MoveError> {
    let tx = crate::Nested::open(conn, "move_root")
        .map_err(|e| MoveError::Failed(format!("starting a move: {e}")))?;
    let moved = apply_move(&tx, root_id, from, target)?;
    tx.commit()
        .map_err(|e| MoveError::Failed(format!("committing the move of root {root_id}: {e}")))?;
    Ok(moved)
}

#[cfg(test)]
mod tests;
