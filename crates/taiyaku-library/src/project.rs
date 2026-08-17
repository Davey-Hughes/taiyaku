//! Rows back into the documents this project already has types for.
//!
//! The store is a projection, and this is the half that proves it. Nothing here
//! decides anything about an alignment: it reassembles a `Sidecar` and hands it
//! to `taiyaku-core`, which owns every rule about what one means.

use std::collections::BTreeMap;

use rusqlite::Connection;
use taiyaku_core::{
    Alignment, Direction, Edition, Half, Member, Quality, RunEntry, Sidecar, Slot, SlotSpan,
    SolverInfo, Source, Verdict, Work,
};

/// Rebuild one work's sidecar from its rows.
///
/// **`taiyaku` is [`taiyaku_core::VERSION`], never a stored value, and that is
/// the honest answer rather than the convenient one.** The field's own
/// definition is "the format version this build writes", and what this
/// function returns is a document assembled by this build out of this schema's
/// columns — so `VERSION` describes the bytes exactly. Carrying a number
/// imported from the document the rows came from would do the opposite: at
/// `VERSION == 2` it would stamp `1` on a document written in the version 2
/// shape, which is a false declaration, and a reader that trusted it would
/// parse the file with the wrong rules. The version is a property of the
/// writer, and here the writer is this code.
///
/// What the review that raised this ("a reimport rewrites a document's
/// declared format version") was actually pointing at is the *other*
/// direction, and it is closed where it belongs: `import::readable_version`
/// refuses a document declaring a version newer than this build reads, so a
/// document whose fields this projection would silently drop never reaches
/// these rows at all. The remaining case — a version 1 document imported by a
/// build that writes 2 and exported back out as 2 — is a format upgrade
/// performed deliberately by a caller who typed `--in-place`, not a silent
/// rewrite, and it is what `export` is for.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if a stored slot span cannot be
/// parsed back (which means a hand edit reached the database), or if SQLite
/// fails.
pub fn sidecar(conn: &Connection, work_id: i64) -> Result<Sidecar, String> {
    let (title, reading, hub) = work_row(conn, work_id)?;
    let editions = editions_for(conn, work_id)?;
    let alignments = alignments_for(conn, work_id)?;
    Ok(Sidecar {
        taiyaku: taiyaku_core::VERSION,
        work: Work { title, reading },
        editions,
        hub,
        alignments,
    })
}

fn work_row(conn: &Connection, work_id: i64) -> Result<(String, Direction, String), String> {
    let (title, reading, hub): (String, String, String) = conn
        .query_row(
            "SELECT title, reading, hub FROM work WHERE id = ?1",
            [work_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| format!("reading work {work_id}: {e}"))?;
    Ok((title, parse_direction(&reading)?, hub))
}

fn editions_for(conn: &Connection, work_id: i64) -> Result<BTreeMap<String, Edition>, String> {
    let mut stmt = conn
        .prepare("SELECT id, key, label FROM edition WHERE work_id = ?1 ORDER BY id")
        .map_err(|e| format!("preparing editions: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("querying editions: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading editions: {e}"))?;

    let mut editions = BTreeMap::new();
    for (edition_id, key, label) in rows {
        editions.insert(
            key,
            Edition {
                label,
                members: members_for(conn, edition_id)?,
                spreads: flags_for(conn, edition_id, "spread")?,
                credits: flags_for(conn, edition_id, "credit")?,
            },
        );
    }
    Ok(editions)
}

fn members_for(conn: &Connection, edition_id: i64) -> Result<Vec<Member>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT path, sha256, pages FROM member
             WHERE edition_id = ?1 ORDER BY ordinal",
        )
        .map_err(|e| format!("preparing members: {e}"))?;
    let rows = stmt
        .query_map([edition_id], |row| {
            Ok(Member {
                path: row.get(0)?,
                sha256: row.get(1)?,
                pages: row.get(2)?,
            })
        })
        .map_err(|e| format!("querying members: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading members: {e}"))
}

fn flags_for(conn: &Connection, edition_id: i64, kind: &str) -> Result<Vec<u32>, String> {
    let mut stmt = conn
        .prepare("SELECT page FROM page_flag WHERE edition_id = ?1 AND kind = ?2 ORDER BY page")
        .map_err(|e| format!("preparing page flags: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![edition_id, kind], |row| {
            row.get::<_, u32>(0)
        })
        .map_err(|e| format!("querying page flags: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading page flags: {e}"))
}

/// Rebuild every alignment stored against `work_id`, in insertion order.
///
/// Three rules the byte-for-byte test polices:
///
/// 1. `ORDER BY ordinal` on `run`, applied in `runs_for` — run order is the
///    document's order and the solver's.
/// 2. Each span is rebuilt from its four stored columns through `SlotSpan`,
///    never from a rendered string, and `b_start_page IS NULL` means the run
///    has no `b` side at all (`RunEntry { b: None }`), not an empty span.
/// 3. `verdict` is read back, along with `slots`, `paired` and `avg_run`:
///    `Quality` derives `Serialize` and `to_json` writes whatever it holds, so
///    recomputing here would hand a reader a different document from the one
///    stored. `Sidecar::quality_of` is the recomputing path, and it is a query
///    accessor nothing in this module calls.
fn alignments_for(conn: &Connection, work_id: i64) -> Result<Vec<Alignment>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, from_key, to_key, by, solver_hash, solver_floor,
                    verdict, slots, paired, avg_run
             FROM alignment WHERE work_id = ?1 ORDER BY id",
        )
        .map_err(|e| format!("preparing alignments: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<f64>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<u32>>(7)?,
                row.get::<_, Option<u32>>(8)?,
                row.get::<_, Option<f64>>(9)?,
            ))
        })
        .map_err(|e| format!("querying alignments: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading alignments: {e}"))?;

    let mut alignments = Vec::with_capacity(rows.len());
    for (id, from, to, by, solver_hash, solver_floor, verdict, slots, paired, avg_run) in rows {
        let solver = match solver_hash {
            Some(hash) => Some(SolverInfo {
                hash,
                floor: solver_floor
                    .ok_or_else(|| format!("alignment {id} has a solver hash but no floor"))?
                    as f32,
            }),
            None => None,
        };
        let quality = match verdict {
            Some(text) => Some(Quality {
                verdict: parse_verdict(&text)?,
                slots: slots.ok_or_else(|| format!("alignment {id} has a verdict but no slots"))?,
                paired: paired
                    .ok_or_else(|| format!("alignment {id} has a verdict but no paired"))?,
                avg_run: avg_run
                    .ok_or_else(|| format!("alignment {id} has a verdict but no avg_run"))?,
            }),
            None => None,
        };
        alignments.push(Alignment {
            from,
            to,
            by: parse_source(&by)?,
            solver,
            quality,
            runs: runs_for(conn, id)?,
        });
    }
    Ok(alignments)
}

/// One `run` row, before its four span columns per side have been rebuilt
/// through `SlotSpan`. A named struct rather than the same shape as a tuple,
/// which is what triggers clippy's `type_complexity` here: eleven columns is
/// exactly the width `run` has.
struct RunColumns {
    a_start_page: u32,
    a_start_half: Option<String>,
    a_end_page: u32,
    a_end_half: Option<String>,
    b_start_page: Option<u32>,
    b_start_half: Option<String>,
    b_end_page: Option<u32>,
    b_end_half: Option<String>,
    margin: Option<f64>,
    by: Option<String>,
    note: Option<String>,
}

fn runs_for(conn: &Connection, alignment_id: i64) -> Result<Vec<RunEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a_start_page, a_start_half, a_end_page, a_end_half,
                    b_start_page, b_start_half, b_end_page, b_end_half,
                    margin, by, note
             FROM run WHERE alignment_id = ?1 ORDER BY ordinal",
        )
        .map_err(|e| format!("preparing runs: {e}"))?;
    let rows = stmt
        .query_map([alignment_id], |row| {
            Ok(RunColumns {
                a_start_page: row.get(0)?,
                a_start_half: row.get(1)?,
                a_end_page: row.get(2)?,
                a_end_half: row.get(3)?,
                b_start_page: row.get(4)?,
                b_start_half: row.get(5)?,
                b_end_page: row.get(6)?,
                b_end_half: row.get(7)?,
                margin: row.get(8)?,
                by: row.get(9)?,
                note: row.get(10)?,
            })
        })
        .map_err(|e| format!("querying runs: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading runs: {e}"))?;

    rows.into_iter().map(run_entry).collect()
}

fn run_entry(columns: RunColumns) -> Result<RunEntry, String> {
    let RunColumns {
        a_start_page,
        a_start_half,
        a_end_page,
        a_end_half,
        b_start_page,
        b_start_half,
        b_end_page,
        b_end_half,
        margin,
        by,
        note,
    } = columns;

    let a = SlotSpan {
        start: Slot {
            page: a_start_page,
            half: parse_half(a_start_half.as_deref())?,
        },
        end: Slot {
            page: a_end_page,
            half: parse_half(a_end_half.as_deref())?,
        },
    };
    // `b_start_page IS NULL` means the run has no `b` side at all — a gap,
    // not a zero-length span at whatever the columns happen to hold.
    let b = match b_start_page {
        Some(start_page) => Some(SlotSpan {
            start: Slot {
                page: start_page,
                half: parse_half(b_start_half.as_deref())?,
            },
            end: Slot {
                page: b_end_page
                    .ok_or_else(|| "run has a b start page but no b end page".to_string())?,
                half: parse_half(b_end_half.as_deref())?,
            },
        }),
        None => None,
    };

    Ok(RunEntry {
        a,
        b,
        margin: margin.map(|m| m as f32),
        by: by.as_deref().map(parse_source).transpose()?,
        note,
    })
}

fn parse_direction(text: &str) -> Result<Direction, String> {
    match text {
        "rtl" => Ok(Direction::RightToLeft),
        "ltr" => Ok(Direction::LeftToRight),
        other => Err(format!("`{other}` is not a reading direction")),
    }
}

fn parse_source(text: &str) -> Result<Source, String> {
    match text {
        "solver" => Ok(Source::Solver),
        "hand" => Ok(Source::Hand),
        other => Err(format!("`{other}` is not a source")),
    }
}

fn parse_verdict(text: &str) -> Result<Verdict, String> {
    match text {
        "refused" => Ok(Verdict::Refused),
        "fragmented" => Ok(Verdict::Fragmented),
        "sound" => Ok(Verdict::Sound),
        other => Err(format!("`{other}` is not a verdict")),
    }
}

fn parse_half(text: Option<&str>) -> Result<Option<Half>, String> {
    match text {
        None => Ok(None),
        Some("left") => Ok(Some(Half::Left)),
        Some("right") => Ok(Some(Half::Right)),
        Some(other) => Err(format!("`{other}` is not a slot half")),
    }
}

#[cfg(test)]
mod tests;
