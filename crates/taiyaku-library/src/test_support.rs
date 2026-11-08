//! Fixtures for tests in this crate and, through the `test-support` feature,
//! in `taiyaku-serve`'s: one copy of each, so a fixture in either crate files
//! a planted directory the way the other's does.
//!
//! `taiyaku-serve` enables the feature only under `[dev-dependencies]`, so this
//! is compiled into test builds and `--all-features` builds and into no binary
//! this workspace ships. The file name is the one the coverage report and
//! `.cargo/mutants.toml` exclude.

use rusqlite::Connection;

/// Link directory `root_id` into library `library_id` as well, by SQL: for a
/// fixture whose directory need not exist, so cannot be linked by the add.
///
/// # Panics
///
/// If the link cannot be written.
pub fn link(conn: &Connection, library_id: i64, root_id: i64) {
    conn.execute(
        "INSERT INTO library_root (library_id, root_id) VALUES (?1, ?2)",
        [library_id, root_id],
    )
    .unwrap_or_else(|e| panic!("links directory {root_id} into library {library_id}: {e}"));
}

/// A `root` row with id `id` at `path`, which need not exist, filed under a
/// library of its own named as `scan::add_root` names one. Answers the
/// library's id.
///
/// # Panics
///
/// If the row cannot be written or filed.
pub fn plant_root(conn: &Connection, id: i64, path: &str) -> i64 {
    conn.execute(
        "INSERT INTO root (id, path) VALUES (?1, ?2)",
        rusqlite::params![id, path],
    )
    .unwrap_or_else(|e| panic!("plants root {id} at {path}: {e}"));
    crate::scan::file_under_its_own_library(conn, id)
        .unwrap_or_else(|e| panic!("files root {id} under a library of its own: {e}"))
}

/// The lowest library directory `root_id` is linked to: its one library,
/// while a directory is in one.
///
/// # Panics
///
/// If the directory is in no library.
#[must_use]
pub fn library_of_root(conn: &Connection, root_id: i64) -> i64 {
    conn.query_row(
        "SELECT min(library_id) FROM library_root WHERE root_id = ?1",
        [root_id],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("directory {root_id} is in a library: {e}"))
}
