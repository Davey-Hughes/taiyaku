//! Migration 23 (O282, O284): every book's slug re-made from its series and
//! volume by [`mint_slug`], and every derived title without its archive
//! extension. A Rust pass because the slug rule is Rust.
//!
//! **A `bridge:` row is left whole.** `taiyaku-serve`'s
//! `works::row_id_for_document` finds one by its slug on every write, so a
//! new slug would have it mint a second row and strand the position on the
//! first. Left as it is, its slug still counts as taken, since `mint_slug`
//! probes every row, and its title is left too: it is a copy of the
//! sidecar's, which the fixed registry serves.
//!
//! **Every other slug is parked first**, at `~<id>`, so that a later row's
//! old slug cannot push an earlier book to `-2`; no slug holds a `~`, so a
//! parked one never stands in a new slug's way. **Then each is minted in id
//! order**, the order the rows were inserted in, so a `-2` lands on the
//! later-added book, as an import gives it.
//!
//! **A `taiyaku.json` work's title is stripped like a scanned one's**, which
//! holds because the import strips it at every reimport too. **So is a
//! rule's captured title**: no column records where a `title_derived` came
//! from, so a stored title cannot be told from a capture, and the pass strips
//! both.

use rusqlite::Connection;

use crate::import::{archive_stem, mint_slug};

/// The titles first, so a book with no series or volume is minted from the
/// title it will show, as a new one is; then the park; then the mint.
pub(super) fn slugs_and_titles(conn: &Connection) -> Result<(), String> {
    strip_derived_titles(conn)?;
    let remade: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM work WHERE content_key NOT LIKE 'bridge:%' ORDER BY id")
            .map_err(|e| format!("listing the works to re-make: {e}"))?;
        stmt.query_map([], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(|e| format!("listing the works to re-make: {e}"))?
    };
    conn.execute(
        "UPDATE work SET slug = '~' || id WHERE content_key NOT LIKE 'bridge:%'",
        [],
    )
    .map_err(|e| format!("parking the slugs to re-make: {e}"))?;
    for id in remade {
        mint_slug(conn, id)?;
    }
    Ok(())
}

/// One trailing archive extension off each derived title, and off the shown
/// title unless a hand title stands over it. A stem left empty is no title,
/// so that name keeps its extension.
fn strip_derived_titles(conn: &Connection) -> Result<(), String> {
    let derived: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, title_derived FROM work
                  WHERE title_derived IS NOT NULL AND content_key NOT LIKE 'bridge:%'
                  ORDER BY id",
            )
            .map_err(|e| format!("listing the derived titles: {e}"))?;
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .and_then(Iterator::collect)
            .map_err(|e| format!("listing the derived titles: {e}"))?
    };
    for (id, title) in &derived {
        let Some(stem) = archive_stem(title).filter(|stem| !stem.is_empty()) else {
            continue;
        };
        conn.execute(
            "UPDATE work SET title_derived = ?1, title = COALESCE(title_hand, ?1)
              WHERE id = ?2",
            rusqlite::params![stem, id],
        )
        .map_err(|e| format!("dropping the extension from work {id}'s title: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
