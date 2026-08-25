//! The `ComicInfo.xml` a generated CBZ carries, built from the library's rows.
//!
//! **Only the generated path.** A passthrough download is served byte for byte
//! and gains nothing: whatever `ComicInfo.xml` the source archive holds travels
//! unchanged, and 12 of this corpus's 57 archives hold one (`docs/sidecar.md`).
//! Spec C2 §3 and §6.
//!
//! **An absent fact is an absent element.** A `<Writer/>` with nothing in it
//! claims the writer is unknown; leaving it out claims nothing. Most credit
//! fields *are* absent today — `credit` has no writer until spec C3's metadata
//! editing ships, which is O44 — so in practice a download carries series,
//! number, title, page count and the edition label. Said here rather than
//! discovered later.
//!
//! **`LanguageISO` comes from `edition.language`**, a real column the rule
//! engine's `set_language` writes and `import` preserves — not from the edition
//! key. Deriving a language from `raw` or `en` would be a guess presented as
//! metadata, which is what `work.root_id` was rejected for in spec C1.

use std::fmt::Write as _;

use rusqlite::Connection;

/// The facts a `ComicInfo.xml` states, each optional because each may be
/// genuinely unknown.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComicInfo {
    pub(crate) series: Option<String>,
    pub(crate) number: Option<f64>,
    pub(crate) title: Option<String>,
    pub(crate) page_count: u32,
    pub(crate) language: Option<String>,
    /// `(role, person name)` as `credit` holds them, in a stable order.
    pub(crate) credits: Vec<(String, String)>,
    pub(crate) notes: Option<String>,
}

/// What this library knows about one edition of one work.
///
/// # Errors
///
/// Returns an error if the rows cannot be read, or if no work has this slug —
/// which a caller reaches only after the visibility check has already passed,
/// so it means the work went away between the two.
pub(crate) fn read(
    conn: &Connection,
    slug: &str,
    edition_key: &str,
    page_count: u32,
) -> Result<ComicInfo, String> {
    let (work_id, title, number, series_id): (i64, String, Option<f64>, Option<i64>) = conn
        .query_row(
            "SELECT id, title, volume, series_id FROM work WHERE slug = ?1",
            [slug],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|e| format!("reading work `{slug}` for its metadata: {e}"))?;

    let series = match series_id {
        None => None,
        Some(id) => conn
            .query_row(
                // `is_primary` first, then any title, then the sort title —
                // a series row always has the last of the three.
                "SELECT COALESCE(
                     (SELECT title FROM series_title
                       WHERE series_id = ?1 ORDER BY is_primary DESC, title LIMIT 1),
                     sort_title)
                   FROM series WHERE id = ?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| format!("reading the series of `{slug}`: {e}"))
            .map(Some)?,
    };

    let (edition_id, label, language): (i64, String, Option<String>) = conn
        .query_row(
            "SELECT id, label, language FROM edition WHERE work_id = ?1 AND key = ?2",
            rusqlite::params![work_id, edition_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| format!("reading edition `{edition_key}` of `{slug}`: {e}"))?;

    // Series-level, work-level and edition-level credits all apply to this
    // download, which is what the three nullable columns are for. Ordered so
    // the output is stable between two downloads of the same edition.
    let mut stmt = conn
        .prepare(
            "SELECT c.role, p.name FROM credit c JOIN person p ON p.id = c.person_id
              WHERE c.edition_id = ?1 OR c.work_id = ?2
              -- `c.series_id = ?3` alone, deliberately. SQL's NULL comparison
              -- already makes this arm sit out for a work with no series:
              -- `c.series_id = NULL` is NULL, never true, for every row. An
              -- `IS NOT NULL` guard beside it changed no result and read as
              -- though it were load-bearing, which is the same thing the zip64
              -- trigger in `cbz.rs` was cut down for on this branch.
                 OR c.series_id = ?3
              ORDER BY c.role, p.name",
        )
        .map_err(|e| format!("preparing the credits of `{slug}`: {e}"))?;
    let credits: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![edition_id, work_id, series_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| format!("reading the credits of `{slug}`: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading the credits of `{slug}`: {e}"))?;

    Ok(ComicInfo {
        series,
        number,
        title: Some(title),
        page_count,
        language,
        credits,
        notes: Some(label),
    })
}

/// Which element each `credit.role` becomes.
///
/// **`group` is a `Publisher`**: a scanlation group is who published that
/// edition, which is the fact `credit.edition_id` exists to hold and the one
/// Komga has nowhere to put. A role this does not know is dropped rather than
/// invented into an element name no reader would recognise.
fn element_for(role: &str) -> Option<&'static str> {
    match role {
        "author" => Some("Writer"),
        "artist" => Some("Penciller"),
        "translator" => Some("Translator"),
        "group" | "publisher" => Some("Publisher"),
        _ => None,
    }
}

/// Escapes the five characters XML cannot carry raw, and drops the rest that
/// XML 1.0's `Char` production forbids outright.
///
/// `& < > " '` become entities. The C0 controls `0x00-0x08`, `0x0B`, `0x0C`
/// and `0x0E-0x1F` cannot appear in a well-formed document even escaped —
/// there is no entity for them — so they are dropped rather than refused:
/// failing a whole download over one stray byte in a hand-edited name is
/// worse than an imperceptibly altered one. Tab, newline and carriage return
/// are legal and pass through unchanged. `U+FFFE` and `U+FFFF` are dropped
/// the same way: `Char` stops its BMP range at `U+FFFD`, both are valid Rust
/// `char`s (unlike a surrogate, which cannot survive as one), and a
/// hand-edited `taiyaku.json` can carry either through `serde_json` into a
/// title or a name. Everything else `Char` excludes — the surrogate range,
/// unpaired by construction — cannot reach here as a `char` at all, so this
/// is the whole of what `Char` forbids that Rust's type does not already
/// rule out.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\u{0}'..='\u{8}'
            | '\u{b}'
            | '\u{c}'
            | '\u{e}'..='\u{1f}'
            | '\u{fffe}'
            | '\u{ffff}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// A volume number as a person writes it: `1`, not `1.0`; `1.5` stays `1.5`.
///
/// `work.volume` is REAL because omnibus and half-volumes exist
/// (`schema/001_initial.sql`), so the fraction has to survive and the absent
/// fraction has to go.
///
/// `NaN` cannot reach here — SQLite converts a bound `NaN` to NULL, so
/// `work.volume` is never `NaN` — but `Infinity` can round-trip and reaches
/// this function's `else` branch as `"inf"`, unchanged.
fn number_text(volume: f64) -> String {
    if volume.fract() == 0.0 && volume.abs() < 1e15 {
        format!("{:.0}", volume)
    } else {
        format!("{volume}")
    }
}

/// The document, as the bytes that go into the archive.
pub(crate) fn xml(info: &ComicInfo) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ComicInfo>\n");
    let mut element = |name: &str, value: &str| {
        let _ = writeln!(out, "  <{name}>{}</{name}>", escape(value));
    };
    if let Some(series) = &info.series {
        element("Series", series);
    }
    if let Some(number) = info.number {
        element("Number", &number_text(number));
    }
    if let Some(title) = &info.title {
        element("Title", title);
    }
    element("PageCount", &info.page_count.to_string());
    if let Some(language) = &info.language {
        element("LanguageISO", language);
    }
    // One element per ComicInfo field, several people comma-separated inside
    // it, which is what the format specifies. `element_for`'s order fixes the
    // element order, so two downloads of the same edition agree.
    for name in ["Writer", "Penciller", "Translator", "Publisher"] {
        let people: Vec<&str> = info
            .credits
            .iter()
            .filter(|(role, _)| element_for(role) == Some(name))
            .map(|(_, person)| person.as_str())
            .collect();
        if !people.is_empty() {
            element(name, &people.join(", "));
        }
    }
    if let Some(notes) = &info.notes {
        element("Notes", notes);
    }
    out.push_str("</ComicInfo>\n");
    out
}

#[cfg(test)]
mod tests;
