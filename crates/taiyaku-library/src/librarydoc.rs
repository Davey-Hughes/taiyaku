//! The `<stem>.library.json` format, and its round trip to rows.
//!
//! Design §4.4. The sidecar deliberately carries almost no metadata — no
//! series, no volume, no author, no language, no external ids — because the
//! sidecar is the alignment. This document is the rest: hand-fixed metadata,
//! the part nothing can re-derive. A rescan reproduces the alignment; it
//! cannot reproduce someone's decision that this directory is volume 3, or
//! that two differently-named trees are one series. So it round-trips to a
//! file for the same reason everything else here does: losing the database
//! should cost a rescan, not the organisational work.
//!
//! **Members are keyed by `sha256`, never by path** — the convention
//! `<stem>.truth.json` already uses (`taiyaku_core::RegionMember`), and what
//! lets a hand-set chapter number survive a file being moved or renamed.
//!
//! **Collections are deliberately absent.** A collection spans works and has
//! no single sidecar to sit beside; it lives in the database and is covered
//! by backup rather than by export.
//!
//! `series_title` and `external_id` are shared by every work in a series, so
//! [`apply`] only ever adds or overwrites a row there — never deletes one a
//! sibling work's own document might still depend on. The one exception is
//! the series nothing depends on any more: when the last work leaves a
//! series, that whole shelf goes with it rather than staying in `series_fts`
//! answering searches nobody can follow (see [`forget_orphaned_series`]).
//! Everything else this
//! module writes (`edition.language`/`kind`, `credit`, `member.chapter_declared`/
//! `chapter_source`, `work_tag`) belongs to this work alone, so [`apply`]
//! replaces that part wholesale: metadata this document does not mention is
//! cleared, not left stale from an earlier document.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use taiyaku_core::Direction;

/// Bumped when an existing field would mean something else.
pub const VERSION: u32 = 1;

/// A whole `<stem>.library.json`.
///
/// `series` and `editions`/`members`/`tags`/`external` default to empty when
/// absent from the document — a standalone work has no series, and a work
/// nobody has hand-edited yet has none of the rest. `series` is a plain
/// struct rather than `Option<SeriesInfo>` so a caller can read
/// `doc.series.titles` directly, the way [`SeriesInfo`]'s own fields are
/// read; an empty `titles` list means the same thing `None` would.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryDoc {
    pub taiyaku_library: u32,
    pub updated: String,
    #[serde(default, skip_serializing_if = "SeriesInfo::is_empty")]
    pub series: SeriesInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub editions: BTreeMap<String, EditionMeta>,
    /// Keyed by `sha256`, never by path — see this module's doc comment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<MemberMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Provider name (`anilist`, `mal`, `mangaupdates`, …) to that provider's
    /// id. Series-scoped, like `series` itself: `external_id` has no
    /// `work_id` column.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub external: BTreeMap<String, String>,
}

/// A series grouping: its names and its reading direction.
///
/// `reading` here is `series.reading`, a property of the series row — not
/// `work.reading`, which the sidecar already carries per edition set. A
/// series can state a reading convention even for a work whose own editions
/// disagree with it (a fan translation flipped to read left-to-right, say).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeriesInfo {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<SeriesTitle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<Direction>,
}

impl SeriesInfo {
    fn is_empty(&self) -> bool {
        self.titles.is_empty() && self.reading.is_none()
    }
}

/// One series title: 「五等分の花嫁」, one of several for one series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesTitle {
    pub title: String,
    pub kind: TitleKind,
    /// The title a reader is shown by default. Only meaningful within one
    /// series' title list — [`SeriesTitle`] carries no `series_id` of its own
    /// to make that scope explicit.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

/// A closed set — `series_title.kind` — so a typo is a parse error rather
/// than a silently new category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleKind {
    Romaji,
    Native,
    English,
    Alt,
}

/// Hand-set metadata for one edition, keyed by the sidecar's own edition key.
///
/// `language` and `kind` are `edition.language`/`edition.kind` — columns the
/// sidecar import never sets, since `taiyaku.json` carries neither.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EditionMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EditionKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<Credit>,
}

/// A closed set — `edition.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditionKind {
    Raw,
    Official,
    Fan,
}

/// One person credited on one edition. `name` rather than a person id: this
/// document is meant to be hand-edited, and nobody hand-edits a surrogate
/// key. [`apply`] finds or creates the matching `person` row by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Credit {
    pub role: CreditRole,
    pub name: String,
}

/// A closed set — `credit.role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CreditRole {
    Author,
    Artist,
    Translator,
    Group,
    Publisher,
}

/// One member's hand-set chapter number, addressed by content rather than
/// position — see this module's doc comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberMeta {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_source: Option<ChapterSource>,
}

/// A closed set — `member.chapter_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChapterSource {
    Comicinfo,
    Filename,
    Hand,
}

impl LibraryDoc {
    /// # Errors
    ///
    /// Returns an error if `text` is not valid JSON, or is JSON that does not
    /// match this document's shape — a `kind` or `role` outside its closed
    /// set, for instance. The message embeds `serde_json`'s own, which names
    /// the field and byte offset.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("not a readable library document: {e}"))
    }

    /// # Panics
    ///
    /// Panics if this document fails to serialise. In practice it cannot:
    /// every field is a plain number, string, `Option`, `Vec`, `BTreeMap` or
    /// derived enum with no hand-written `Serialize` impl, and this
    /// document's only map keys (`editions`, `external`, both `String`) are
    /// not floats — the one case `serde_json::to_string_pretty` can fail on.
    #[must_use]
    pub fn to_json(&self) -> String {
        // Pretty-printed on purpose, the same reason as `Sidecar::to_json`:
        // this file is meant to diff in git.
        serde_json::to_string_pretty(self).expect("library document is always serialisable")
    }

    /// Every field at its default — no series, no volume, no edition or
    /// member metadata, no tags, no external ids.
    ///
    /// **What `migrate.rs` uses to tell "this work's hand-fixed metadata has
    /// never been applied" from "the work row is new".** `of` always answers
    /// a document — a work nobody has ever hand-edited projects to exactly
    /// this empty shape (`of_is_empty_for_a_work_nothing_has_been_applied_to`,
    /// this module's own test), which is indistinguishable from a document
    /// that was applied and then hand-cleared back down to nothing. The two
    /// cases have no observable difference and re-`apply`ing the sibling in
    /// either one is a no-op, so treating them alike costs nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
            && self.volume.is_none()
            && self.editions.is_empty()
            && self.members.is_empty()
            && self.tags.is_empty()
            && self.external.is_empty()
    }
}

/// Project one work's hand-fixed metadata out of the database.
///
/// `updated` is not a stored value — nothing in the schema holds a
/// last-edited timestamp for this document, the same reason `taiyaku_truth`
/// is always the current [`taiyaku_core::TRUTH_VERSION`] rather than
/// something read back (`state.rs`'s `truth`): this is a fresh projection,
/// stamped with the time it was taken.
///
/// An edition appears in `editions` only if it has hand-set `language`,
/// `kind`, or at least one credit — an edition nobody has annotated is
/// omitted rather than written out as `{}`, keeping the document limited to
/// what a person actually set.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if a stored `kind`, `role` or
/// `chapter_source` is not one of the values [`apply`] ever writes, or if
/// SQLite fails.
pub fn of(conn: &Connection, work_id: i64) -> Result<LibraryDoc, String> {
    let (volume, series_id): (Option<f64>, Option<i64>) = conn
        .query_row(
            "SELECT volume, series_id FROM work WHERE id = ?1",
            [work_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("reading work {work_id}: {e}"))?;

    let series = match series_id {
        Some(id) => series_info_of(conn, id)?,
        None => SeriesInfo::default(),
    };
    let editions = editions_meta_of(conn, work_id)?;
    let members = members_meta_of(conn, work_id)?;
    let tags = tags_of(conn, work_id)?;
    let external = match series_id {
        Some(id) => external_of(conn, id)?,
        None => BTreeMap::new(),
    };

    Ok(LibraryDoc {
        taiyaku_library: VERSION,
        updated: now_rfc3339(conn)?,
        series,
        volume,
        editions,
        members,
        tags,
        external,
    })
}

fn now_rfc3339(conn: &Connection) -> Result<String, String> {
    conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("reading the current time: {e}"))
}

fn series_info_of(conn: &Connection, series_id: i64) -> Result<SeriesInfo, String> {
    let reading: Option<String> = conn
        .query_row(
            "SELECT reading FROM series WHERE id = ?1",
            [series_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("reading series {series_id}: {e}"))?;
    let reading = reading.as_deref().map(parse_direction).transpose()?;

    let mut stmt = conn
        .prepare(
            "SELECT title, kind, is_primary FROM series_title WHERE series_id = ?1 ORDER BY rowid",
        )
        .map_err(|e| format!("preparing series titles: {e}"))?;
    let rows = stmt
        .query_map([series_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })
        .map_err(|e| format!("querying series titles: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading series titles: {e}"))?;

    let titles = rows
        .into_iter()
        .map(|(title, kind, primary)| {
            Ok(SeriesTitle {
                title,
                kind: parse_title_kind(&kind)?,
                primary,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(SeriesInfo { titles, reading })
}

fn editions_meta_of(
    conn: &Connection,
    work_id: i64,
) -> Result<BTreeMap<String, EditionMeta>, String> {
    let mut stmt = conn
        .prepare("SELECT id, key, language, kind FROM edition WHERE work_id = ?1")
        .map_err(|e| format!("preparing editions: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| format!("querying editions: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading editions: {e}"))?;

    let mut out = BTreeMap::new();
    for (edition_id, key, language, kind) in rows {
        let kind = kind.as_deref().map(parse_edition_kind).transpose()?;
        let credits = credits_of(conn, edition_id)?;
        if language.is_none() && kind.is_none() && credits.is_empty() {
            continue;
        }
        out.insert(
            key,
            EditionMeta {
                language,
                kind,
                credits,
            },
        );
    }
    Ok(out)
}

fn credits_of(conn: &Connection, edition_id: i64) -> Result<Vec<Credit>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT person.name, credit.role FROM credit
             JOIN person ON person.id = credit.person_id
             WHERE credit.edition_id = ?1 ORDER BY credit.id",
        )
        .map_err(|e| format!("preparing credits: {e}"))?;
    let rows = stmt
        .query_map([edition_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("querying credits: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading credits: {e}"))?;

    rows.into_iter()
        .map(|(name, role)| {
            Ok(Credit {
                role: parse_credit_role(&role)?,
                name,
            })
        })
        .collect()
}

/// Only members carrying hand-set chapter data — most of a work's members
/// have neither, and listing every one of them would make this document as
/// large as the archive list itself.
fn members_meta_of(conn: &Connection, work_id: i64) -> Result<Vec<MemberMeta>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT member.sha256, member.chapter_declared, member.chapter_source
             FROM member JOIN edition ON edition.id = member.edition_id
             WHERE edition.work_id = ?1
               AND (member.chapter_declared IS NOT NULL OR member.chapter_source IS NOT NULL)
             ORDER BY member.sha256",
        )
        .map_err(|e| format!("preparing members: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| format!("querying members: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading members: {e}"))?;

    rows.into_iter()
        .map(|(sha256, chapter, chapter_source)| {
            Ok(MemberMeta {
                sha256,
                chapter,
                chapter_source: chapter_source
                    .as_deref()
                    .map(parse_chapter_source)
                    .transpose()?,
            })
        })
        .collect()
}

fn tags_of(conn: &Connection, work_id: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT tag.name FROM work_tag JOIN tag ON tag.id = work_tag.tag_id
             WHERE work_tag.work_id = ?1 ORDER BY tag.name",
        )
        .map_err(|e| format!("preparing tags: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| row.get::<_, String>(0))
        .map_err(|e| format!("querying tags: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading tags: {e}"))
}

fn external_of(conn: &Connection, series_id: i64) -> Result<BTreeMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT provider, external_id FROM external_id WHERE series_id = ?1")
        .map_err(|e| format!("preparing external ids: {e}"))?;
    let rows = stmt
        .query_map([series_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("querying external ids: {e}"))?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|e| format!("reading external ids: {e}"))
}

/// Write `doc` onto `work_id`, replacing whatever hand-fixed metadata this
/// work held before.
///
/// **Idempotent**: applying the same document twice leaves the database
/// exactly as applying it once did.
///
/// `series_title` and `external_id` are the two tables a series shares across
/// every work in it, so they are upserted — a title or an external id this
/// document names is added or overwritten, never deleted for naming nothing.
/// Deleting there on every apply would let one work's document erase a
/// sibling work's own contribution to the same series. A series with no work
/// left under it at all is the one thing that is removed, and only after the
/// count is checked: see [`forget_orphaned_series`]. Everything else this
/// document owns outright (`edition.language`/`kind`, `credit`,
/// `member.chapter_declared`/`chapter_source`, `work_tag`) is replaced
/// wholesale, the same way `state::put_notes` and `state::put_truth` replace
/// their own document: metadata this document does not mention is cleared,
/// not left behind from an earlier one.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if `doc.editions` names a key
/// this work has no edition for (import the sidecar first), if `doc.external`
/// is non-empty while `doc.series.titles` is empty — `external_id` has no
/// `work_id` column, so there is nowhere to attach it without a series — or
/// if SQLite fails. A `sha256` in `doc.members` that matches no member of
/// this work is not an error: the content it names may simply no longer be
/// present, and there is nothing to attach a stale chapter number to.
pub fn apply(conn: &Connection, work_id: i64, doc: &LibraryDoc) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting library-metadata transaction: {e}"))?;

    // A plain `UPDATE ... WHERE id = ?` affects zero rows for a work_id that
    // does not exist rather than failing, so the row count is checked by
    // hand — otherwise every write below would silently no-op instead of
    // refusing, the one outcome this function's own doc comment promises
    // not to produce.
    let updated = tx
        .execute(
            "UPDATE work SET volume = ?1 WHERE id = ?2",
            params![doc.volume, work_id],
        )
        .map_err(|e| format!("writing volume for work {work_id}: {e}"))?;
    if updated == 0 {
        return Err(format!("work {work_id} does not exist"));
    }

    let series_id = apply_series(&tx, work_id, &doc.series)?;
    apply_external(&tx, series_id, &doc.external)?;
    apply_editions(&tx, work_id, &doc.editions)?;
    apply_members(&tx, work_id, &doc.members)?;
    apply_tags(&tx, work_id, &doc.tags)?;

    tx.commit()
        .map_err(|e| format!("committing library metadata for work {work_id}: {e}"))
}

/// Resolve, create or clear this work's series, and upsert the titles this
/// document names onto it. Returns the resolved `series_id`, or `None` when
/// `series.titles` is empty and the work's series association was cleared.
///
/// **A series the last work in it just left is deleted, not left behind.**
/// Clearing `work.series_id` used to be the whole of this branch, so the
/// `series` row, its `series_title` rows and their `series_fts` entries stayed
/// exactly where they were, belonging to nobody. Search (spec C1) reads
/// `series_fts`, so a title nothing is under would keep answering a query
/// forever, and every re-application of a document that names a different
/// series left another one (whole-branch review 3, Minor 14). See
/// [`forget_orphaned_series`] for why it is checked rather than assumed.
fn apply_series(
    conn: &Connection,
    work_id: i64,
    series: &SeriesInfo,
) -> Result<Option<i64>, String> {
    let previous = series_of(conn, work_id)?;
    if series.titles.is_empty() {
        conn.execute("UPDATE work SET series_id = NULL WHERE id = ?1", [work_id])
            .map_err(|e| format!("clearing series for work {work_id}: {e}"))?;
        forget_orphaned_series(conn, previous)?;
        return Ok(None);
    }

    let sort_title = series
        .titles
        .iter()
        .find(|t| t.primary)
        .unwrap_or(&series.titles[0])
        .title
        .as_str();
    let reading = series.reading.map(direction_text);

    let series_id = match find_series_by_title(conn, &series.titles)? {
        Some(id) => {
            conn.execute(
                "UPDATE series SET sort_title = ?1, reading = ?2 WHERE id = ?3",
                params![sort_title, reading, id],
            )
            .map_err(|e| format!("updating series {id}: {e}"))?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO series (sort_title, reading) VALUES (?1, ?2)",
                params![sort_title, reading],
            )
            .map_err(|e| format!("inserting series: {e}"))?;
            conn.last_insert_rowid()
        }
    };

    for title in &series.titles {
        // Upsert, not delete-then-reinsert — see this module's and `apply`'s
        // doc comments for why `series_title` is shared rather than owned.
        //
        // This is the hazard the connection-level `recursive_triggers`
        // pragma exists for: `series_title`'s primary key is the composite
        // `(series_id, title)`, not a rowid alias, so this REPLACE resolves a
        // conflict by deleting the old row and inserting a new one with a
        // fresh rowid. That implicit delete only reaches `series_title_ad`
        // — the trigger keeping `series_fts` current — when
        // `PRAGMA recursive_triggers` is on, which `Library::from_connection`
        // sets for exactly this reason. A connection opened any other way
        // would leave a phantom row in `series_fts` here, silently.
        conn.execute(
            "INSERT OR REPLACE INTO series_title (series_id, title, kind, is_primary)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                series_id,
                title.title,
                title_kind_text(title.kind),
                title.primary
            ],
        )
        .map_err(|e| format!("writing series title `{}`: {e}", title.title))?;
    }

    conn.execute(
        "UPDATE work SET series_id = ?1 WHERE id = ?2",
        params![series_id, work_id],
    )
    .map_err(|e| format!("attaching work {work_id} to series {series_id}: {e}"))?;

    // The same orphan, reached the other way: a document that renames this
    // work's series onto a new title set resolves — or creates — a different
    // `series` row, and the one it just left is as empty as the cleared case
    // above if nothing else was in it.
    if previous != Some(series_id) {
        forget_orphaned_series(conn, previous)?;
    }

    Ok(Some(series_id))
}

/// The `series_id` this work is attached to right now, before this document
/// changes it.
fn series_of(conn: &Connection, work_id: i64) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT series_id FROM work WHERE id = ?1",
        [work_id],
        |row| row.get(0),
    )
    .optional()
    .map(Option::flatten)
    .map_err(|e| format!("reading the series of work {work_id}: {e}"))
}

/// Delete `series_id` if nothing is under it any more.
///
/// **Checked, not assumed.** A series is shared — that is the whole reason
/// [`apply`] upserts `series_title` and `external_id` rather than replacing
/// them — so the work that just left may well not have been the last one, and
/// deleting on the way out would erase a sibling work's shelf. `EXISTS` over
/// `work.series_id` is the question, asked after the `UPDATE` that removed
/// this work from the count.
///
/// **And `EXISTS` over `collection_item.series_id` beside it**, because a
/// collection is the one holder this document cannot restore. Everything the
/// `DELETE` below cascades into is keyed on the series and reachable only
/// through it — `series_title`, `series_tag`, `external_id` — with one
/// exception: `collection_item` belongs to a *collection*, which spans works,
/// has no sibling document to sit beside and is covered by backup rather than
/// by export (this module's own doc comment). So a shelf a reading list still
/// names is not an orphan, however few works are on it, and dropping it out of
/// that list because one work's metadata edit happened to be the last one
/// would be a loss no re-import could undo. Nothing writes `collection_item`
/// today — spec C1's collections are what will — which is exactly why the
/// guard has to be right before the writer exists: after it lands the failure
/// is silent and permanent. `work_tag`, `credit` and the rest cascade from
/// `work`, not from `series`, and are not this function's business.
///
/// One `DELETE` is enough for all of it: `series_title`, `external_id`,
/// `series_tag` and `collection_item` all declare `ON DELETE CASCADE` on
/// `series(id)`, and SQLite fires a table's own `DELETE` triggers for a
/// foreign-key cascade exactly as it would for a direct `DELETE` against that
/// table — no pragma required, `recursive_triggers` included. So this one
/// statement reaches `series_title_ad` on its own, and `series_fts` loses its
/// rows with the rest. **Not** the same dependency `apply_series`' own
/// `INSERT OR REPLACE` carries (see its comment): that one needs
/// `recursive_triggers` because an `INSERT OR REPLACE` conflict is resolved by
/// an *implicit* delete, which recursive_triggers is what gates — this
/// `DELETE` is explicit and needs no such gate.
///
/// **Nothing in the product reaches this with a series to forget yet, and that
/// is recorded rather than fixed** (delta review M5). [`apply`]'s one
/// production caller is `migrate::apply_library`, reached only through
/// `Step::Apply`, which requires that no `migrated_document` row for `library`
/// exists — and `work.series_id` is written only by [`apply_series`], which
/// records that marker on success. So on the path the product takes today the
/// `previous` handed here is always `None` and both call sites return at the
/// first line. It is kept because `apply` is `pub` API and the second apply is
/// the ordinary case the moment anything applies a hand-edited document twice
/// — a CLI for it is the open question M8 names — and a guard written after
/// that lands is a guard written after the shelves have already been dropped.
fn forget_orphaned_series(conn: &Connection, series_id: Option<i64>) -> Result<(), String> {
    let Some(series_id) = series_id else {
        return Ok(());
    };
    let still_shelved: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM work WHERE series_id = ?1)
                 OR EXISTS(SELECT 1 FROM collection_item WHERE series_id = ?1)",
            [series_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("checking whether anything still holds series {series_id}: {e}"))?;
    if still_shelved {
        return Ok(());
    }
    conn.execute("DELETE FROM series WHERE id = ?1", [series_id])
        .map_err(|e| format!("removing the empty series {series_id}: {e}"))?;
    Ok(())
}

/// The `series_id` of an existing series sharing any of `titles`, if one
/// already holds a matching `series_title` row — first match in document
/// order, so re-applying the same document always resolves to the series it
/// resolved to before.
fn find_series_by_title(conn: &Connection, titles: &[SeriesTitle]) -> Result<Option<i64>, String> {
    for title in titles {
        let found: Option<i64> = conn
            .query_row(
                "SELECT series_id FROM series_title WHERE title = ?1",
                [&title.title],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("looking up series title `{}`: {e}", title.title))?;
        if let Some(id) = found {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn apply_external(
    conn: &Connection,
    series_id: Option<i64>,
    external: &BTreeMap<String, String>,
) -> Result<(), String> {
    if external.is_empty() {
        return Ok(());
    }
    let series_id = series_id.ok_or_else(|| {
        "this document has external ids but no series titles to attach them to; \
         `external_id` is keyed by series, not by work"
            .to_string()
    })?;
    for (provider, external_id) in external {
        // Upserted, same reason as `series_title`: a provider mapping is a
        // property of the series, and a sibling work's document may set a
        // provider this one does not mention.
        conn.execute(
            "INSERT OR REPLACE INTO external_id (series_id, provider, external_id)
             VALUES (?1, ?2, ?3)",
            params![series_id, provider, external_id],
        )
        .map_err(|e| format!("writing external id `{provider}`: {e}"))?;
    }
    Ok(())
}

/// Replace this work's edition-level metadata wholesale: every edition's
/// `language`/`kind` reset before `doc.editions` is applied over them, and
/// every edition-scoped `credit` row cleared and rewritten from scratch.
fn apply_editions(
    conn: &Connection,
    work_id: i64,
    editions: &BTreeMap<String, EditionMeta>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE edition SET language = NULL, kind = NULL WHERE work_id = ?1",
        [work_id],
    )
    .map_err(|e| format!("clearing edition metadata for work {work_id}: {e}"))?;
    conn.execute(
        "DELETE FROM credit WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
        [work_id],
    )
    .map_err(|e| format!("clearing edition credits for work {work_id}: {e}"))?;

    for (key, meta) in editions {
        let edition_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM edition WHERE work_id = ?1 AND key = ?2",
                params![work_id, key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("looking up edition `{key}`: {e}"))?;
        let edition_id = edition_id.ok_or_else(|| {
            format!(
                "work {work_id} has no edition named `{key}`; import its sidecar before \
                 applying metadata that names it"
            )
        })?;

        conn.execute(
            "UPDATE edition SET language = ?1, kind = ?2 WHERE id = ?3",
            params![meta.language, meta.kind.map(edition_kind_text), edition_id],
        )
        .map_err(|e| format!("writing metadata for edition `{key}`: {e}"))?;

        for credit in &meta.credits {
            let person_id = find_or_create_person(conn, &credit.name)?;
            conn.execute(
                "INSERT INTO credit (person_id, role, edition_id) VALUES (?1, ?2, ?3)",
                params![person_id, credit_role_text(credit.role), edition_id],
            )
            .map_err(|e| format!("writing a credit on edition `{key}`: {e}"))?;
        }
    }
    Ok(())
}

fn find_or_create_person(conn: &Connection, name: &str) -> Result<i64, String> {
    let found: Option<i64> = conn
        .query_row("SELECT id FROM person WHERE name = ?1", [name], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| format!("looking up person `{name}`: {e}"))?;
    if let Some(id) = found {
        return Ok(id);
    }
    conn.execute("INSERT INTO person (name) VALUES (?1)", [name])
        .map_err(|e| format!("inserting person `{name}`: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Reset every member's chapter data across this work's editions, then write
/// `members` back over it — content matched by `sha256`, never by path or
/// edition. A `sha256` that matches no member here is skipped rather than
/// refused: the content it names may no longer be present, and content that
/// is not present has nothing to attach a chapter number to.
fn apply_members(conn: &Connection, work_id: i64, members: &[MemberMeta]) -> Result<(), String> {
    conn.execute(
        "UPDATE member SET chapter_declared = NULL, chapter_source = NULL
         WHERE edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
        [work_id],
    )
    .map_err(|e| format!("clearing member chapters for work {work_id}: {e}"))?;

    for member in members {
        conn.execute(
            "UPDATE member SET chapter_declared = ?1, chapter_source = ?2
             WHERE sha256 = ?3 AND edition_id IN (SELECT id FROM edition WHERE work_id = ?4)",
            params![
                member.chapter,
                member.chapter_source.map(chapter_source_text),
                member.sha256,
                work_id,
            ],
        )
        .map_err(|e| format!("writing chapter for member `{}`: {e}", member.sha256))?;
    }
    Ok(())
}

fn apply_tags(conn: &Connection, work_id: i64, tags: &[String]) -> Result<(), String> {
    conn.execute("DELETE FROM work_tag WHERE work_id = ?1", [work_id])
        .map_err(|e| format!("clearing tags for work {work_id}: {e}"))?;
    for name in tags {
        let tag_id = find_or_create_tag(conn, name)?;
        conn.execute(
            "INSERT OR IGNORE INTO work_tag (work_id, tag_id) VALUES (?1, ?2)",
            params![work_id, tag_id],
        )
        .map_err(|e| format!("tagging work {work_id} with `{name}`: {e}"))?;
    }
    Ok(())
}

fn find_or_create_tag(conn: &Connection, name: &str) -> Result<i64, String> {
    let found: Option<i64> = conn
        .query_row("SELECT id FROM tag WHERE name = ?1", [name], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| format!("looking up tag `{name}`: {e}"))?;
    if let Some(id) = found {
        return Ok(id);
    }
    conn.execute("INSERT INTO tag (name) VALUES (?1)", [name])
        .map_err(|e| format!("inserting tag `{name}`: {e}"))?;
    Ok(conn.last_insert_rowid())
}

fn direction_text(direction: Direction) -> &'static str {
    match direction {
        Direction::RightToLeft => "rtl",
        Direction::LeftToRight => "ltr",
    }
}

fn parse_direction(text: &str) -> Result<Direction, String> {
    match text {
        "rtl" => Ok(Direction::RightToLeft),
        "ltr" => Ok(Direction::LeftToRight),
        other => Err(format!("`{other}` is not a reading direction")),
    }
}

fn title_kind_text(kind: TitleKind) -> &'static str {
    match kind {
        TitleKind::Romaji => "romaji",
        TitleKind::Native => "native",
        TitleKind::English => "english",
        TitleKind::Alt => "alt",
    }
}

fn parse_title_kind(text: &str) -> Result<TitleKind, String> {
    match text {
        "romaji" => Ok(TitleKind::Romaji),
        "native" => Ok(TitleKind::Native),
        "english" => Ok(TitleKind::English),
        "alt" => Ok(TitleKind::Alt),
        other => Err(format!("`{other}` is not a series-title kind")),
    }
}

fn edition_kind_text(kind: EditionKind) -> &'static str {
    match kind {
        EditionKind::Raw => "raw",
        EditionKind::Official => "official",
        EditionKind::Fan => "fan",
    }
}

/// `edition.kind`'s closed set, read out of the column's stored text.
///
/// `pub(crate)` for `derive::write_edition_metadata`, which has to know
/// whether a rule's `set_kind` is a value this column can hold before it
/// writes one — and the answer has to be this list, since this is the list
/// [`of`] refuses a stored value against. A second copy of the three words is
/// how two mechanisms come to disagree, which is the same argument
/// `derive::parent_of` carries for `proposals::common_directory_pattern`.
pub(crate) fn parse_edition_kind(text: &str) -> Result<EditionKind, String> {
    match text {
        "raw" => Ok(EditionKind::Raw),
        "official" => Ok(EditionKind::Official),
        "fan" => Ok(EditionKind::Fan),
        other => Err(format!("`{other}` is not an edition kind")),
    }
}

fn credit_role_text(role: CreditRole) -> &'static str {
    match role {
        CreditRole::Author => "author",
        CreditRole::Artist => "artist",
        CreditRole::Translator => "translator",
        CreditRole::Group => "group",
        CreditRole::Publisher => "publisher",
    }
}

fn parse_credit_role(text: &str) -> Result<CreditRole, String> {
    match text {
        "author" => Ok(CreditRole::Author),
        "artist" => Ok(CreditRole::Artist),
        "translator" => Ok(CreditRole::Translator),
        "group" => Ok(CreditRole::Group),
        "publisher" => Ok(CreditRole::Publisher),
        other => Err(format!("`{other}` is not a credit role")),
    }
}

fn chapter_source_text(source: ChapterSource) -> &'static str {
    match source {
        ChapterSource::Comicinfo => "comicinfo",
        ChapterSource::Filename => "filename",
        ChapterSource::Hand => "hand",
    }
}

fn parse_chapter_source(text: &str) -> Result<ChapterSource, String> {
    match text {
        "comicinfo" => Ok(ChapterSource::Comicinfo),
        "filename" => Ok(ChapterSource::Filename),
        "hand" => Ok(ChapterSource::Hand),
        other => Err(format!("`{other}` is not a chapter source")),
    }
}

#[cfg(test)]
mod tests;
