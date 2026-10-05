//! The counter migration 14 keeps: a number that moves whenever a row a
//! served work is built from is written, by any connection in any process.
//!
//! `schema/014_structure_version.sql` says which tables move it, and why
//! triggers do the counting rather than the writers.

use rusqlite::Connection;

/// The counter's current value.
///
/// # Errors
///
/// Returns an error if the row cannot be read, as on a database migration 14
/// never reached.
pub fn version(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT n FROM structure_version WHERE id = 1", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("reading the structure version: {e}"))
}

#[cfg(test)]
mod tests;
