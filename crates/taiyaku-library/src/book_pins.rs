//! Links a person pinned between a book's hub and another of its editions,
//! which `taiyaku-serve`'s book route aligns through (`book_pin`, migration
//! 25).

use rusqlite::{Connection, params};

/// The hub's block `hub_block` and `edition`'s block `block` start one link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub edition: String,
    pub hub_block: u32,
    pub block: u32,
}

/// Every pin on work `work_id`, by edition and then by hub block.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn pins(conn: &Connection, work_id: i64) -> Result<Vec<Pin>, String> {
    let failed = |e: rusqlite::Error| format!("reading the pins of work {work_id}: {e}");
    let mut stmt = conn
        .prepare(
            "SELECT edition, hub_block, block FROM book_pin
              WHERE work_id = ?1 ORDER BY edition, hub_block",
        )
        .map_err(failed)?;
    stmt.query_map([work_id], |row| {
        Ok(Pin {
            edition: row.get(0)?,
            hub_block: row.get(1)?,
            block: row.get(2)?,
        })
    })
    .map_err(failed)?
    .collect::<Result<_, _>>()
    .map_err(failed)
}

/// Pin `pin` on work `work_id`, first taking away every pin on its edition
/// that crosses it, so the pins stay in order on both sides: the newest is
/// the one a person meant.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn pin(conn: &Connection, work_id: i64, pin: &Pin) -> Result<(), String> {
    let failed = |e: rusqlite::Error| format!("pinning a link of work {work_id}: {e}");
    let tx = crate::Nested::open(conn, "book_pin").map_err(failed)?;
    tx.execute(
        "DELETE FROM book_pin
          WHERE work_id = ?1 AND edition = ?2
            AND ((hub_block >= ?3 AND block <= ?4) OR (hub_block <= ?3 AND block >= ?4))",
        params![work_id, pin.edition, pin.hub_block, pin.block],
    )
    .map_err(failed)?;
    tx.execute(
        "INSERT INTO book_pin (work_id, edition, hub_block, block) VALUES (?1, ?2, ?3, ?4)",
        params![work_id, pin.edition, pin.hub_block, pin.block],
    )
    .map_err(failed)?;
    tx.commit().map_err(failed)
}

/// Take away the pin on work `work_id`'s `edition` at hub block `hub_block`,
/// answering whether there was one.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn unpin(
    conn: &Connection,
    work_id: i64,
    edition: &str,
    hub_block: u32,
) -> Result<bool, String> {
    conn.execute(
        "DELETE FROM book_pin WHERE work_id = ?1 AND edition = ?2 AND hub_block = ?3",
        params![work_id, edition, hub_block],
    )
    .map(|removed| removed > 0)
    .map_err(|e| format!("unpinning a link of work {work_id}: {e}"))
}

#[cfg(test)]
mod tests;
