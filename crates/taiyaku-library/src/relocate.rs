//! Moving a directory from one library to another: §3.5 of
//! `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md`.
//!
//! **A move relabels the directory and writes nothing else.** It sets
//! `root.library_id`, making the target first where a new library is named.
//! A library's works, its counts and an account's access are all read
//! through the directories a work's members are under, so every work,
//! edition, member, position and note stays the row it was, and a work with
//! an edition on each side is in both libraries. Migration 15's
//! `library_emptied_by_move` removes a library the move empties, and its
//! grants with it.
//!
//! **What changes is who sees the directory**, so that is what a move answers:
//! the accounts that gain sight of it and the accounts that lose it, each
//! account's sight read by `account::access_for`'s rule, through the
//! directories its grants reach, before the move and after it.
//!
//! **A move into a library is refused inside or around one of that library's
//! directories** (`scan::beside`, in the add's words). Another library's
//! directory may still hold it, or sit inside it. The add's directory-member
//! refusal is not asked: it does not depend on the library.
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
    /// No such directory, or no such library.
    Missing(String),
    /// The caller's own mistake: the directory's own library, one with a
    /// directory it is inside or around (`scan::beside`), or a name for a new
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
    library_id: i64,
    library: String,
}

fn dir(conn: &Connection, root_id: i64) -> Result<Dir, MoveError> {
    conn.query_row(
        "SELECT r.id, r.path, l.id, l.name FROM root r JOIN library l ON l.id = r.library_id
          WHERE r.id = ?1",
        [root_id],
        |r| {
            Ok(Dir {
                id: r.get(0)?,
                path: r.get(1)?,
                library_id: r.get(2)?,
                library: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| MoveError::Failed(format!("reading directory {root_id}: {e}")))?
    .ok_or_else(|| MoveError::Missing(format!("there is no directory {root_id}")))
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
            if id == dir.library_id {
                return Err(MoveError::Unusable(format!(
                    "{} is already in `{name}`",
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
/// account's sight is [`crate::account::access_for`]'s, so a move names what
/// the account is then served.
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

/// The move, inside the caller's transaction. Everything it answers but
/// `gains_sight` and `loses_sight` is read before the directory's library
/// changes, while a library the move empties and its grants are still there
/// to read; who sees the directory is read on both sides of the change.
fn apply_move(conn: &Connection, root_id: i64, target: Target<'_>) -> Result<Moved, MoveError> {
    let dir = dir(conn, root_id)?;
    let target_id = resolve(conn, &dir, target)?;
    // A new library holds no directory to be inside or around.
    if let Target::Library(_) = target {
        let refused =
            crate::scan::beside(conn, Path::new(&dir.path), Some(target_id), Some(dir.id))
                .map_err(MoveError::Failed)?;
        if let Some(why) = refused {
            return Err(MoveError::Unusable(why));
        }
    }
    let library = crate::libraries::name_of(conn, target_id)
        .map_err(MoveError::Failed)?
        .unwrap_or_default();
    let saw = sighted(conn, dir.id).map_err(MoveError::Failed)?;
    let count = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<usize, MoveError> {
        conn.query_row(sql, params, |r| r.get::<_, i64>(0))
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
            .map_err(|e| MoveError::Failed(format!("counting for a move: {e}")))
    };
    let last = count(
        "SELECT count(*) FROM root WHERE library_id = ?1 AND id <> ?2",
        &[&dir.library_id, &dir.id],
    )? == 0;
    let (grants_removed, left_with_no_library) = if last {
        (
            count(
                "SELECT count(*) FROM user_library WHERE library_id = ?1",
                &[&dir.library_id],
            )?,
            crate::scan::only_grant_holders(conn, dir.library_id).map_err(MoveError::Failed)?,
        )
    } else {
        (0, Vec::new())
    };

    conn.execute(
        "UPDATE root SET library_id = ?2 WHERE id = ?1",
        [dir.id, target_id],
    )
    .map_err(|e| MoveError::Failed(format!("moving root {root_id}: {e}")))?;
    let sees = sighted(conn, dir.id).map_err(MoveError::Failed)?;
    Ok(Moved {
        library_id: target_id,
        library,
        gains_sight: sees
            .iter()
            .filter(|name| !saw.contains(name))
            .cloned()
            .collect(),
        loses_sight: saw
            .iter()
            .filter(|name| !sees.contains(name))
            .cloned()
            .collect(),
        library_removed: last.then_some(dir.library),
        grants_removed,
        left_with_no_library,
    })
}

/// What moving root `root_id` to `target` would do, found by doing it inside
/// a transaction this rolls back.
///
/// # Errors
///
/// See [`MoveError`]. A move that would fail fails here in the same words.
pub fn move_cost(conn: &Connection, root_id: i64, target: Target<'_>) -> Result<Moved, MoveError> {
    let tx = crate::Nested::open(conn, "move_cost")
        .map_err(|e| MoveError::Failed(format!("starting to rehearse a move: {e}")))?;
    let answer = apply_move(&tx, root_id, target);
    // Not committed: dropping `tx` is the rollback, whatever the answer.
    drop(tx);
    answer
}

/// Move root `root_id` to `target`, and commit. Nothing is derived and no
/// file is written: no row a work is made of changes.
///
/// # Errors
///
/// See [`MoveError`]. A failed move changed nothing.
pub fn move_root(conn: &Connection, root_id: i64, target: Target<'_>) -> Result<Moved, MoveError> {
    let tx = crate::Nested::open(conn, "move_root")
        .map_err(|e| MoveError::Failed(format!("starting a move: {e}")))?;
    let moved = apply_move(&tx, root_id, target)?;
    tx.commit()
        .map_err(|e| MoveError::Failed(format!("committing the move of root {root_id}: {e}")))?;
    Ok(moved)
}

#[cfg(test)]
mod tests;
