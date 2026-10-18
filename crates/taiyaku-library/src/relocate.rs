//! Moving a folder from one library to another: §6.2 of
//! `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A move is a writer in front of the derive, and the derive does not
//! change.** Left to the derive alone, a folder whose library changed has its
//! works written again under the new library and the old ones folded into
//! them, so each has a new id and slug, and a work with an edition on each
//! side is not divided at all. So the move files the works itself first, and
//! divides them itself afterwards.
//!
//! **A work goes where its hub edition goes.** Its content key is a hash of
//! the hub's members and its hand document is addressed off the hub's
//! folder, so the row that keeps both is the one that stays with the hub.
//!
//! **It is refused where a reader's place cannot be kept exactly**, and
//! refused whole: nothing changes.
//!
//! **A rehearsal is the move, rolled back**, as `scan::removal_cost` is a
//! removal rolled back. It derives with `Documents::Skip`, so it writes no
//! file either.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension as _};

use crate::derive::{DeriveReport, Documents, derive_with};

/// Where a folder is moved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// A library that exists, by its id.
    Library(i64),
    /// A library made for the folder. A library is born with a folder, so
    /// this is the only way to give a folder one of its own again.
    New(&'a str),
}

/// Why a move was not made. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    /// No such folder, or no such library.
    Missing(String),
    /// The caller's own mistake: the folder's own library, or a name for a
    /// new one that is empty or taken.
    Unusable(String),
    /// The move cannot keep every reader's place. `volumes` names what
    /// stands in the way, each as `title (edition key)`; it is empty where no
    /// volume does.
    Refused { why: String, volumes: Vec<String> },
    /// A store fault.
    Failed(String),
}

/// The folder being moved, as it stands before the move.
struct Folder {
    id: i64,
    path: String,
    library_id: i64,
    library: String,
}

fn folder(conn: &Connection, root_id: i64) -> Result<Folder, MoveError> {
    conn.query_row(
        "SELECT r.id, r.path, l.id, l.name FROM root r JOIN library l ON l.id = r.library_id
          WHERE r.id = ?1",
        [root_id],
        |r| {
            Ok(Folder {
                id: r.get(0)?,
                path: r.get(1)?,
                library_id: r.get(2)?,
                library: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| MoveError::Failed(format!("reading folder {root_id}: {e}")))?
    .ok_or_else(|| MoveError::Missing(format!("there is no folder {root_id}")))
}

/// The id of the library `target` names, making it where a new one is named.
/// Made inside the caller's transaction, so a refused or rehearsed move
/// leaves no library behind.
fn resolve(conn: &Connection, folder: &Folder, target: Target<'_>) -> Result<i64, MoveError> {
    match target {
        Target::Library(id) => {
            let name = crate::libraries::name_of(conn, id)
                .map_err(MoveError::Failed)?
                .ok_or_else(|| MoveError::Missing(format!("there is no library {id}")))?;
            if id == folder.library_id {
                return Err(MoveError::Unusable(format!(
                    "{} is already in `{name}`",
                    folder.path
                )));
            }
            Ok(id)
        }
        Target::New(name) => {
            if crate::libraries::trim_name(name).is_empty() {
                return Err(MoveError::Unusable("a library needs a name".to_string()));
            }
            if let Some(taken) = crate::libraries::id_of(conn, name).map_err(MoveError::Failed)? {
                let stored = crate::libraries::name_of(conn, taken)
                    .map_err(MoveError::Failed)?
                    .unwrap_or_default();
                return Err(MoveError::Unusable(format!(
                    "a library is already called `{stored}`; move the folder to it instead"
                )));
            }
            crate::libraries::create(conn, name).map_err(MoveError::Failed)
        }
    }
}

/// How many volumes a refusal's sentence names before it counts the rest.
/// The whole list travels beside the sentence.
const NAMED: usize = 3;

/// `a, b, c and 4 more`.
fn first_few(volumes: &[String]) -> String {
    let shown: Vec<&str> = volumes.iter().take(NAMED).map(String::as_str).collect();
    match volumes.len() - shown.len() {
        0 => shown.join(", "),
        rest => format!("{} and {rest} more", shown.join(", ")),
    }
}

/// Each `title (edition key)` a statement of two columns answers, in order.
fn volumes_of(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(sql)
        .map_err(|e| format!("reading what stands in a move's way: {e}"))?;
    let rows = statement
        .query_map(params, |r| {
            Ok(format!(
                "{} ({})",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            ))
        })
        .map_err(|e| format!("reading what stands in a move's way: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading what stands in a move's way: {e}"))
}

/// Why `folder` may not be moved to `target_id`, as far as the store can say
/// before anything is written.
///
/// 1. **One edition has members under the folder and elsewhere.** The move
///    would divide the edition itself, and no slot of it would name the page
///    it named.
/// 2. **The target holds a volume of the folder's under the same edition
///    key.** A volume is a series title and a number within a library, so
///    derived together the two are one edition of one work, holding both
///    sets of pages.
/// 3. **A work put in a series by hand moves, and works of that series stay.**
///    The series row stays with them and the work would point at it from
///    another library.
///
/// Each names its volumes. The folder being its library's last is not a
/// reason: the library goes with it, and [`Moved::left_with_no_library`]
/// names whoever that leaves with none.
fn refused_before(
    conn: &Connection,
    folder: &Folder,
    target_id: i64,
) -> Result<Option<MoveError>, String> {
    let spanning = volumes_of(
        conn,
        "SELECT w.title, e.key FROM edition e JOIN work w ON w.id = e.work_id
          WHERE EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id AND m.root_id = ?1)
            AND EXISTS (SELECT 1 FROM member m
                         WHERE m.edition_id = e.id AND m.root_id IS NOT ?1)
          ORDER BY w.title, e.key",
        &[&folder.id],
    )?;
    if !spanning.is_empty() {
        return Ok(Some(MoveError::Refused {
            why: format!(
                "{} holds part of one edition whose other files are elsewhere, and a move \
                 would divide it: {}. Nothing was changed",
                folder.path,
                first_few(&spanning)
            ),
            volumes: spanning,
        }));
    }

    let target = crate::libraries::name_of(conn, target_id)?.unwrap_or_default();
    let doubled = volumes_of(
        conn,
        "SELECT DISTINCT w.title, e.key
           FROM work w
           JOIN series s ON s.id = w.series_id
           JOIN edition e ON e.work_id = w.id
          WHERE w.volume IS NOT NULL
            AND EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id AND m.root_id = ?1)
            AND EXISTS (SELECT 1 FROM work tw
                          JOIN series ts ON ts.id = tw.series_id
                          JOIN edition te ON te.work_id = tw.id
                         WHERE tw.library_id = ?2 AND tw.id <> w.id
                           AND ts.sort_title = s.sort_title AND tw.volume = w.volume
                           AND te.key = e.key)
          ORDER BY w.title, e.key",
        &[&folder.id, &target_id],
    )?;
    if !doubled.is_empty() {
        return Ok(Some(MoveError::Refused {
            why: format!(
                "`{target}` already holds volumes of {} under the same edition key: {}. \
                 Nothing was changed",
                folder.path,
                first_few(&doubled)
            ),
            volumes: doubled,
        }));
    }

    // A work put in a series by hand moves with its hub. Its series row moves
    // only when every work of it does (`refile`), so where one stays behind
    // the work would arrive in the target still pointing at a series of the
    // library it left.
    let placed = volumes_of(
        conn,
        "SELECT w.title, w.hub FROM work w
          WHERE w.series_id_hand IS NOT NULL
            AND EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                         WHERE e.work_id = w.id AND e.key = w.hub AND m.root_id = ?1)
            AND EXISTS (SELECT 1 FROM work other
                         WHERE other.series_id = w.series_id_hand AND other.id <> w.id
                           AND NOT EXISTS (SELECT 1 FROM edition oe
                                             JOIN member om ON om.edition_id = oe.id
                                            WHERE oe.work_id = other.id AND oe.key = other.hub
                                              AND om.root_id = ?1))
          ORDER BY w.title, w.hub",
        &[&folder.id],
    )?;
    if !placed.is_empty() {
        return Ok(Some(MoveError::Refused {
            why: format!(
                "{} holds works that were put in a series by hand, and the series keeps other \
                 works behind: {}. Nothing was changed",
                folder.path,
                first_few(&placed)
            ),
            volumes: placed,
        }));
    }
    Ok(None)
}

/// What a move did, or in a rehearsal would do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Moved {
    /// The library the folder is in afterwards, and its name.
    pub library_id: i64,
    pub library: String,
    /// Works wholly under the folder that kept their row.
    pub works_kept: usize,
    /// Works wholly under the folder that were folded into a work the
    /// target already held, as another edition of it.
    pub works_joined: usize,
    /// Divided works an edition was carried out of.
    pub works_split: usize,
    /// Series the target already held that gained a volume from the folder.
    pub series_joined: usize,
    /// Reading positions and notes on the editions that change library: all
    /// those of a work wholly under the folder, and those of a divided
    /// work's editions that are under it. They all go with them, or the move
    /// is refused. What a divided work's staying editions hold is not counted.
    pub positions: usize,
    pub notes: usize,
    /// Alignments between two editions the move separated.
    pub alignments_dropped: usize,
    /// The library the folder left, where it was that library's last
    /// folder, and the grants that went with it.
    pub library_removed: Option<String>,
    pub grants_removed: usize,
    /// The accounts, by name, whose only grant went with that library, and
    /// who see no library afterwards: `scan::only_grant_holders`, asked
    /// before the move. Empty where no library is removed.
    pub left_with_no_library: Vec<String>,
}

/// A divided work, as it stood before the move.
struct Home {
    /// `title (hub key)`.
    named: String,
    /// Whether its hub edition is under the folder, so that the work's row
    /// goes with the folder.
    hub_moves: bool,
    /// The library it was filed under.
    library: Option<i64>,
}

/// What the store held before a move, for the verdict after it.
struct Census {
    /// Works every member of which is under the folder, with their titles.
    whole: BTreeMap<i64, String>,
    /// Works with a member under the folder and a member elsewhere.
    divided: BTreeMap<i64, String>,
    /// Where each divided work must stand afterwards, and how to name it.
    homes: BTreeMap<i64, Home>,
    /// Positions and notes on the editions that change library.
    positions: usize,
    notes: usize,
    /// How many of each the whole store holds.
    all_positions: i64,
    all_notes: i64,
    all_truth_pages: i64,
    /// Every work filed under a library.
    filed: BTreeSet<i64>,
    /// The series the target held before the move.
    target_series: BTreeSet<i64>,
    /// The folder's library, where the folder is its last, its grants, and
    /// the accounts it was the only grant of.
    library_removed: Option<String>,
    grants_removed: usize,
    left_with_no_library: Vec<String>,
}

fn ids_and_titles(
    conn: &Connection,
    sql: &str,
    root_id: i64,
) -> Result<BTreeMap<i64, String>, String> {
    let mut statement = conn
        .prepare(sql)
        .map_err(|e| format!("reading the works under root {root_id}: {e}"))?;
    let rows = statement
        .query_map([root_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("reading the works under root {root_id}: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("reading the works under root {root_id}: {e}"))
}

fn count(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<i64, String> {
    conn.query_row(sql, params, |r| r.get(0))
        .map_err(|e| format!("counting for a move: {e}"))
}

impl Census {
    fn take(conn: &Connection, folder: &Folder, target_id: i64) -> Result<Self, String> {
        let under = "EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                              WHERE e.work_id = w.id AND m.root_id = ?1)";
        let elsewhere = "EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                                  WHERE e.work_id = w.id AND m.root_id IS NOT ?1)";
        let whole = ids_and_titles(
            conn,
            &format!("SELECT w.id, w.title FROM work w WHERE {under} AND NOT {elsewhere}"),
            folder.id,
        )?;
        let divided = ids_and_titles(
            conn,
            &format!("SELECT w.id, w.title FROM work w WHERE {under} AND {elsewhere}"),
            folder.id,
        )?;
        let homes = {
            let mut statement = conn
                .prepare(&format!(
                    "SELECT w.id, w.title, w.hub, w.library_id,
                            EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                                     WHERE e.work_id = w.id AND e.key = w.hub AND m.root_id = ?1)
                       FROM work w WHERE {under} AND {elsewhere}"
                ))
                .map_err(|e| format!("reading the divided works under root {}: {e}", folder.id))?;
            let rows = statement
                .query_map([folder.id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        Home {
                            named: format!(
                                "{} ({})",
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?
                            ),
                            library: r.get(3)?,
                            hub_moves: r.get(4)?,
                        },
                    ))
                })
                .map_err(|e| format!("reading the divided works under root {}: {e}", folder.id))?;
            rows.collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(|e| format!("reading the divided works under root {}: {e}", folder.id))?
        };
        let on_them = |table: &str| {
            count(
                conn,
                &format!(
                    "SELECT count(*) FROM {table} t JOIN work w ON w.id = t.work_id
                      WHERE ({under} AND NOT {elsewhere})
                         OR EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                                     WHERE e.work_id = w.id AND e.key = t.edition
                                       AND m.root_id = ?1)"
                ),
                &[&folder.id],
            )
        };
        let last = count(
            conn,
            "SELECT count(*) FROM root WHERE library_id = ?1 AND id <> ?2",
            &[&folder.library_id, &folder.id],
        )? == 0;
        let filed = conn
            .prepare("SELECT id FROM work WHERE library_id IS NOT NULL")
            .map_err(|e| format!("reading the filed works: {e}"))?
            .query_map([], |r| r.get(0))
            .map_err(|e| format!("reading the filed works: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading the filed works: {e}"))?;
        let target_series = conn
            .prepare("SELECT id FROM series WHERE library_id = ?1")
            .map_err(|e| format!("reading the series of library {target_id}: {e}"))?
            .query_map([target_id], |r| r.get(0))
            .map_err(|e| format!("reading the series of library {target_id}: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading the series of library {target_id}: {e}"))?;
        Ok(Self {
            positions: usize::try_from(on_them("progress")?).unwrap_or(usize::MAX),
            notes: usize::try_from(on_them("note")?).unwrap_or(usize::MAX),
            whole,
            divided,
            homes,
            all_positions: count(conn, "SELECT count(*) FROM progress", &[])?,
            all_notes: count(conn, "SELECT count(*) FROM note", &[])?,
            all_truth_pages: count(conn, "SELECT count(*) FROM truth_page", &[])?,
            filed,
            target_series,
            library_removed: last.then(|| folder.library.clone()),
            grants_removed: if last {
                usize::try_from(count(
                    conn,
                    "SELECT count(*) FROM user_library WHERE library_id = ?1",
                    &[&folder.library_id],
                )?)
                .unwrap_or(usize::MAX)
            } else {
                0
            },
            left_with_no_library: if last {
                crate::scan::only_grant_holders(conn, folder.library_id)?
            } else {
                Vec::new()
            },
        })
    }
}

/// File under `target_id` each work whose hub edition is under the folder,
/// and each series all of whose works were so filed, unless the target holds
/// a series of its title: the works then join that one at the derive.
///
/// **Before the folder's own library changes**, so that a library migration
/// 15 deletes with its last folder takes no row's `library_id` with it.
///
/// **A work the target already holds is left where it is.** One of the same
/// content key cannot be filed there: `work` is unique on its content key
/// within a library. One at the same address, its series' `sort_title` and
/// its volume, is the volume the target already has: the derive writes the
/// folder's files onto the target's row as another edition, and folds this
/// one into it.
///
/// **What the address clause decides is a divided work's row.** A work whose
/// hub is under the folder and whose other editions stay, filed to a library
/// that holds its volume, would go to the target and be folded away there at
/// once, and its staying editions would have no row to stay on. Left where it
/// is, the move is refused by the verdict
/// (`a_divided_work_whose_hub_moves_to_a_library_holding_its_volume_is_refused`
/// is its guard). For a work wholly under the folder the end state is the
/// same with or without it, which is why the join cases do not decide it.
///
/// **Why filing is enough.** A derive finds the row a group of files belongs
/// on by content key within the group's library (`import::scanned_in`)
/// before it asks for the address, and the key is a hash of the hub
/// edition's members, which a move does not touch.
fn refile(conn: &Connection, folder: &Folder, target_id: i64) -> Result<(), String> {
    let moving: Vec<(i64, Option<i64>)> = conn
        .prepare(
            "SELECT w.id, w.series_id
               FROM work w
               JOIN edition e ON e.work_id = w.id AND e.key = w.hub
              WHERE EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id AND m.root_id = ?1)
                AND NOT EXISTS (SELECT 1 FROM work tw
                                 WHERE tw.library_id = ?2 AND tw.id <> w.id
                                   AND tw.content_key = w.content_key)
                AND NOT (w.volume IS NOT NULL AND EXISTS (
                           SELECT 1 FROM work tw
                             JOIN series ts ON ts.id = tw.series_id
                             JOIN series s ON s.id = w.series_id
                            WHERE tw.library_id = ?2 AND tw.id <> w.id
                              AND ts.sort_title = s.sort_title AND tw.volume = w.volume))
              ORDER BY w.id",
        )
        .map_err(|e| format!("reading the works to file under library {target_id}: {e}"))?
        .query_map([folder.id, target_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("reading the works to file under library {target_id}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading the works to file under library {target_id}: {e}"))?;
    for (work, _) in &moving {
        conn.execute(
            "UPDATE work SET library_id = ?2 WHERE id = ?1",
            [*work, target_id],
        )
        .map_err(|e| format!("filing work {work} under library {target_id}: {e}"))?;
    }

    let series: BTreeSet<i64> = moving.iter().filter_map(|(_, series)| *series).collect();
    for series in series {
        let keeps_works_behind = count(
            conn,
            "SELECT count(*) FROM work WHERE series_id = ?1 AND library_id IS NOT ?2",
            &[&series, &target_id],
        )? > 0;
        if keeps_works_behind {
            continue;
        }
        let held = count(
            conn,
            "SELECT count(*) FROM series t JOIN series s ON s.id = ?1
              WHERE t.library_id = ?2 AND t.id <> s.id AND t.sort_title = s.sort_title",
            &[&series, &target_id],
        )? > 0;
        if held {
            continue;
        }
        conn.execute(
            "UPDATE series SET library_id = ?2 WHERE id = ?1",
            [series, target_id],
        )
        .map_err(|e| format!("filing series {series} under library {target_id}: {e}"))?;
    }
    Ok(())
}

/// Give each divided work's misfiled editions to the work of the library
/// their files are in. Answers what was carried, and each edition that had
/// nowhere to go or was refused.
///
/// After the derive, a divided work still holds the editions whose files are
/// now another library's: a derive leaves standing an edition it did not
/// name. The same derive made, in that other library, a work holding a fresh
/// edition of the same key over the same members.
///
/// **Misfiled by this move, and only so.** Where the work's hub stays, an
/// edition with a member under the moved folder; where the hub went with the
/// folder, an edition whose files are left in the folder's old library. An
/// edition an earlier change (migration 15's split) left in a library other
/// than its work's is not this move's to give away.
fn split(conn: &Connection, folder: &Folder, census: &Census) -> Result<Split, String> {
    let mut carried = crate::fold::Rehomed::default();
    let mut unplaced = Vec::new();
    let mut reasons = Vec::new();
    let mut works = 0;
    for (&work, title) in &census.divided {
        let hub_moves = census.homes.get(&work).is_some_and(|home| home.hub_moves);
        // An edition whose members are all under folders of one library,
        // which is not this work's, with the work of that library that
        // holds the same key over a member of the same folder, path and
        // digest.
        let misfiled: Vec<(String, Option<i64>)> = conn
            .prepare(
                "SELECT e.key,
                        (SELECT te.work_id
                           FROM member m
                           JOIN member tm ON tm.root_id = m.root_id AND tm.path = m.path
                                         AND tm.sha256 = m.sha256
                           JOIN edition te ON te.id = tm.edition_id AND te.key = e.key
                                          AND te.id <> e.id
                           JOIN work tw ON tw.id = te.work_id
                          WHERE m.edition_id = e.id
                            AND tw.library_id = (SELECT min(r.library_id)
                                                   FROM member m2 JOIN root r ON r.id = m2.root_id
                                                  WHERE m2.edition_id = e.id)
                          LIMIT 1)
                   FROM edition e
                   JOIN work w ON w.id = e.work_id
                  WHERE e.work_id = ?1
                    AND (SELECT count(DISTINCT r.library_id)
                           FROM member m JOIN root r ON r.id = m.root_id
                          WHERE m.edition_id = e.id) = 1
                    AND NOT EXISTS (SELECT 1 FROM member m
                                     WHERE m.edition_id = e.id AND m.root_id IS NULL)
                    AND (SELECT min(r.library_id)
                           FROM member m JOIN root r ON r.id = m.root_id
                          WHERE m.edition_id = e.id) IS NOT w.library_id
                    AND CASE WHEN ?4
                         THEN (SELECT min(r.library_id)
                                 FROM member m JOIN root r ON r.id = m.root_id
                                WHERE m.edition_id = e.id) IS ?3
                         ELSE EXISTS (SELECT 1 FROM member m
                                       WHERE m.edition_id = e.id AND m.root_id = ?2)
                       END
                  ORDER BY e.key",
            )
            .map_err(|e| format!("reading the misfiled editions of work {work}: {e}"))?
            .query_map(
                rusqlite::params![work, folder.id, folder.library_id, hub_moves],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| format!("reading the misfiled editions of work {work}: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading the misfiled editions of work {work}: {e}"))?;

        let mut by_twin: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        for (key, twin) in misfiled {
            match twin {
                Some(twin) => by_twin.entry(twin).or_default().push(key),
                None => unplaced.push(format!("{title} ({key})")),
            }
        }
        let mut carried_out = false;
        for (twin, keys) in by_twin {
            match crate::fold::rehome_editions(conn, work, twin, &keys)? {
                crate::fold::Rehoming::Done(moved) => {
                    carried_out = true;
                    carried.alignments_dropped += moved.alignments_dropped;
                }
                crate::fold::Rehoming::Refused(why) => {
                    reasons.push(why);
                    unplaced.extend(keys.iter().map(|key| format!("{title} ({key})")));
                }
            }
        }
        if carried_out {
            works += 1;
        }
    }
    Ok(Split {
        carried,
        unplaced,
        reasons,
        works,
    })
}

/// What [`split`] did.
struct Split {
    carried: crate::fold::Rehomed,
    /// `title (edition key)` of each edition left where it was.
    unplaced: Vec<String>,
    /// Why `rehome_editions` refused, where it did.
    reasons: Vec<String>,
    /// The divided works it carried an edition out of.
    works: usize,
}

/// The sentence a fold or a rehoming refused in, without its closing clause:
/// the move says once that nothing was changed.
fn said(why: &str) -> &str {
    why.trim_end_matches("; nothing was moved and nothing was deleted")
}

fn refused(folder: &Folder, library: &str, why: &str, volumes: Vec<String>) -> MoveError {
    let named = if volumes.is_empty() {
        String::new()
    } else {
        format!(": {}", first_few(&volumes))
    };
    MoveError::Refused {
        why: format!(
            "{} cannot be moved to `{library}` and keep every reader's place: {why}{named}. \
             Nothing was changed",
            folder.path
        ),
        volumes,
    }
}

/// The move, inside the caller's transaction.
fn apply_move(
    conn: &Connection,
    root_id: i64,
    target: Target<'_>,
) -> Result<(Moved, DeriveReport), MoveError> {
    let folder = folder(conn, root_id)?;
    let target_id = resolve(conn, &folder, target)?;
    if let Some(refusal) = refused_before(conn, &folder, target_id).map_err(MoveError::Failed)? {
        return Err(refusal);
    }
    let library = crate::libraries::name_of(conn, target_id)
        .map_err(MoveError::Failed)?
        .unwrap_or_default();

    // What a derive reports before the move is not the move's doing.
    let baseline = derive_with(conn, Documents::Skip).map_err(MoveError::Failed)?;
    if baseline.lost_positions > 0
        || baseline.dropped_notes > 0
        || baseline.discarded_rows > 0
        || baseline.uncarried_alignments > 0
    {
        return Err(refused(
            &folder,
            &library,
            "the store holds changes not yet derived, and deriving them would lose a position, a \
             note, an alignment or another row a reader made. Scan the libraries before moving \
             anything",
            Vec::new(),
        ));
    }
    let census = Census::take(conn, &folder, target_id).map_err(MoveError::Failed)?;

    refile(conn, &folder, target_id).map_err(MoveError::Failed)?;
    conn.execute(
        "UPDATE root SET library_id = ?2 WHERE id = ?1",
        [folder.id, target_id],
    )
    .map_err(|e| MoveError::Failed(format!("moving root {root_id}: {e}")))?;
    let derived = derive_with(conn, Documents::Skip).map_err(MoveError::Failed)?;
    let Split {
        carried,
        unplaced,
        reasons,
        works: works_split,
    } = split(conn, &folder, &census).map_err(MoveError::Failed)?;

    // The verdict. Each arm is a refusal of the whole move.
    if !unplaced.is_empty() {
        let why = if reasons.is_empty() {
            "an edition the move separates from its work has no work to go to".to_string()
        } else {
            format!(
                "an edition the move separates from its work could not be given to the work \
                 that holds its files ({})",
                reasons
                    .iter()
                    .map(|why| said(why))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        return Err(refused(&folder, &library, &why, unplaced));
    }
    // A sentence that occurs more often after than before is new, so this is
    // a multiset difference and not a `contains`.
    let unfoldable: Vec<&str> = {
        let mut before: Vec<&String> = baseline.unfoldable.iter().collect();
        derived
            .unfoldable
            .iter()
            .filter(|why| match before.iter().position(|held| *held == *why) {
                Some(at) => {
                    before.swap_remove(at);
                    false
                }
                None => true,
            })
            .map(|why| said(why))
            .collect()
    };
    if !unfoldable.is_empty() {
        let unjoined: Vec<String> = {
            let mut statement = conn
                .prepare("SELECT title, hub FROM work WHERE id = ?1 AND library_id IS NOT ?2")
                .map_err(|e| MoveError::Failed(format!("reading a move's outcome: {e}")))?;
            let mut named = Vec::new();
            for id in census.whole.keys() {
                let found: Option<String> = statement
                    .query_row(rusqlite::params![id, target_id], |r| {
                        Ok(format!(
                            "{} ({})",
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?
                        ))
                    })
                    .optional()
                    .map_err(|e| MoveError::Failed(format!("reading a move's outcome: {e}")))?;
                named.extend(found);
            }
            named
        };
        return Err(refused(
            &folder,
            &library,
            &format!(
                "a work could not be joined to the one that holds its volume ({})",
                unfoldable.join("; ")
            ),
            unjoined,
        ));
    }
    // A divided work stands where its hub went. The derive may have folded
    // it into the target's work, as it does where the target holds its
    // volume, and then its row, slug and hand title are gone.
    let mut gone = Vec::new();
    for (id, home) in &census.homes {
        let now: Option<Option<i64>> = conn
            .query_row("SELECT library_id FROM work WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| MoveError::Failed(format!("reading a move's outcome: {e}")))?;
        let expected = if home.hub_moves {
            Some(target_id)
        } else {
            home.library
        };
        if now != Some(expected) {
            gone.push(home.named.clone());
        }
    }
    if !gone.is_empty() {
        return Err(refused(
            &folder,
            &library,
            "a work with files on both sides would not stand where its first edition goes",
            gone,
        ));
    }
    let titles = |sql: &str| -> Result<Vec<(i64, String)>, MoveError> {
        conn.prepare(sql)
            .and_then(|mut statement| {
                statement
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| MoveError::Failed(format!("reading a move's outcome: {e}")))
    };
    let standing = titles(&format!(
        "SELECT id, title FROM work WHERE library_id IS NOT {target_id} ORDER BY title"
    ))?;
    let outside: Vec<String> = standing
        .iter()
        .filter(|(id, _)| census.whole.contains_key(id))
        .map(|(_, title)| title.clone())
        .collect();
    if !outside.is_empty() {
        return Err(refused(
            &folder,
            &library,
            "works of the folder would be left outside it",
            outside,
        ));
    }
    let unfiled: Vec<String> =
        titles("SELECT id, title FROM work WHERE library_id IS NULL ORDER BY title")?
            .into_iter()
            .filter(|(id, _)| census.filed.contains(id))
            .map(|(_, title)| title)
            .collect();
    if !unfiled.is_empty() {
        return Err(refused(
            &folder,
            &library,
            "works would be left in no library",
            unfiled,
        ));
    }
    // What the derive after the change lost that the one before it did not:
    // a reader's row, or an alignment a fold could not carry.
    let lost = (
        derived.lost_positions > baseline.lost_positions,
        derived.dropped_notes > baseline.dropped_notes,
        derived.discarded_rows > baseline.discarded_rows,
        derived.uncarried_alignments > baseline.uncarried_alignments,
    );
    if lost != (false, false, false, false) {
        let standing: BTreeSet<i64> = titles("SELECT id, title FROM work")?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let joined: Vec<String> = census
            .whole
            .iter()
            .filter(|(id, _)| !standing.contains(id))
            .map(|(_, title)| title.clone())
            .collect();
        return Err(refused(
            &folder,
            &library,
            "joining a work to the one that holds its volume would lose a position, a note, an \
             alignment or another row a reader made",
            joined,
        ));
    }

    let after = |table: &str| {
        count(conn, &format!("SELECT count(*) FROM {table}"), &[]).map_err(MoveError::Failed)
    };
    // Counted against the store itself, because the derive's own report does
    // not see every loss: a ground-truth page lost in a fold is in no report
    // field, and this entry is its only guard. A lost note is also reported
    // as `dropped_notes` and a lost position as `lost_positions`, which the
    // arm above refuses in its own words, so those two entries are defence
    // in depth.
    for (table, before, what) in [
        ("progress", census.all_positions, "reading position"),
        ("note", census.all_notes, "note"),
        ("truth_page", census.all_truth_pages, "ground-truth page"),
    ] {
        let lost = before - after(table)?;
        if lost > 0 {
            return Err(refused(
                &folder,
                &library,
                &format!("{lost} {what}(s) would stop existing"),
                Vec::new(),
            ));
        }
    }
    let still: BTreeSet<i64> = titles("SELECT id, title FROM work")?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let works_kept = census.whole.keys().filter(|id| still.contains(id)).count();
    // Each series the target held before that now holds a work with a file
    // under the folder.
    let gained: BTreeSet<i64> = conn
        .prepare(
            "SELECT DISTINCT w.series_id FROM work w
              WHERE w.series_id IS NOT NULL
                AND EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                             WHERE e.work_id = w.id AND m.root_id = ?1)",
        )
        .and_then(|mut statement| {
            statement
                .query_map([folder.id], |r| r.get(0))?
                .collect::<Result<BTreeSet<i64>, _>>()
        })
        .map_err(|e| MoveError::Failed(format!("reading a move's outcome: {e}")))?;
    let series_joined = gained.intersection(&census.target_series).count();
    Ok((
        Moved {
            library_id: target_id,
            library,
            works_kept,
            works_joined: census.whole.len() - works_kept,
            works_split,
            series_joined,
            positions: census.positions,
            notes: census.notes,
            alignments_dropped: carried.alignments_dropped,
            library_removed: census.library_removed,
            grants_removed: census.grants_removed,
            left_with_no_library: census.left_with_no_library,
        },
        derived,
    ))
}

/// What moving root `root_id` to `target` would do, found by doing it inside
/// a transaction this rolls back. It writes no file: each derive in it runs
/// under [`Documents::Skip`].
///
/// # Errors
///
/// See [`MoveError`]. A move that would be refused is refused here in the
/// same words.
pub fn move_cost(
    conn: &Connection,
    root_id: i64,
    target: Target<'_>,
) -> Result<(Moved, DeriveReport), MoveError> {
    let tx = crate::Nested::open(conn, "move_cost")
        .map_err(|e| MoveError::Failed(format!("starting to rehearse a move: {e}")))?;
    let answer = apply_move(&tx, root_id, target);
    // Not committed: dropping `tx` is the rollback, whatever the answer.
    drop(tx);
    answer
}

/// Move root `root_id` to `target`, then derive again with documents
/// reconciled, after the commit, for `scan::remove_root`'s reason: a
/// document is written or renamed only once the rows it follows are
/// committed.
///
/// The [`DeriveReport`] answered is that second pass's.
///
/// # Errors
///
/// See [`MoveError`]. A refused move changed nothing. A failure of the pass
/// after the commit is [`MoveError::Failed`], and by then the move is made.
pub fn move_root(
    conn: &Connection,
    root_id: i64,
    target: Target<'_>,
) -> Result<(Moved, DeriveReport), MoveError> {
    let tx = crate::Nested::open(conn, "move_root")
        .map_err(|e| MoveError::Failed(format!("starting a move: {e}")))?;
    let (moved, _predicted) = apply_move(&tx, root_id, target)?;
    tx.commit()
        .map_err(|e| MoveError::Failed(format!("committing the move of root {root_id}: {e}")))?;
    let derived = crate::derive::derive(conn).map_err(|e| {
        MoveError::Failed(format!(
            "reconciling documents after moving root {root_id}: {e}"
        ))
    })?;
    Ok((moved, derived))
}

#[cfg(test)]
mod tests;
