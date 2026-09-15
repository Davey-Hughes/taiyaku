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
    /// A hand-set work title, overriding the one derivation minted.
    ///
    /// `Option`, and projected from `work.title_hand` rather than from
    /// `work.title` — see migration 010 for why the effective column cannot
    /// be the one that appears here. **O67** is the defect: a work spanning
    /// five editions is named after whichever file minted its row, so a
    /// reader is shown "…Full Color Edition… — en-mono" for a monochrome
    /// edition. `work.slug` is unaffected and deliberately so; it is an
    /// address, and progress, notes and truth all hang off it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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
    /// **Whether a person put this work on this shelf — O80.**
    ///
    /// The rest of this struct is the *series'* own metadata, which is
    /// series-level data replicated into every member work's document. This
    /// one field is about *this work*: it says the attachment was chosen
    /// rather than derived, and it is what `apply_series` consults before
    /// recording `work.series_id_hand`.
    ///
    /// **Without it, a document carrying hand-set titles would re-assert a
    /// derived attachment as a person's** on the next apply, because
    /// `apply_series` resolves by title text and cannot tell why the block is
    /// there. That is O80's freeze moved rather than fixed.
    ///
    /// Absent in the JSON when false, so a document that only corrects a
    /// series' titles says nothing about attachment at all.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chosen: bool,
}

impl SeriesInfo {
    fn is_empty(&self) -> bool {
        self.titles.is_empty() && self.reading.is_none() && !self.chosen
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
    /// A hand-set edition label, overriding the one the import minted.
    /// Projected from `edition.label_hand`, for migration 010's reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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

    /// Every field at its default — no hand-set title, no series, no volume,
    /// no edition or member metadata, no tags, no external ids.
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
        self.title.is_none()
            && self.series.is_empty()
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
/// An edition appears in `editions` only if `label_hand`, `language`, `kind`
/// or at least one credit is non-empty — an edition nobody has touched at all
/// is omitted rather than written out as `{}`.
///
/// **Only `label` is genuinely hand-set here — `language` and `kind` are not.**
/// `label` is read from `edition.label_hand`, which is NULL until a person sets
/// one. `language` and `kind` have no `_hand` counterpart (migration 010 only
/// split `title`/`label`): they are read from the plain columns
/// `derive::write_edition_metadata` also writes, under the same
/// `migrate::absorbed(…, LIBRARY)` guard `librarydoc::apply` writes them
/// behind. So a purely-derived edition — one a rule's `set_kind`/`set_language`
/// classified and nobody has ever hand-corrected — still has a non-NULL `kind`
/// or `language`, and this function cannot tell that apart from a hand
/// correction: it appears in the document either way.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if a stored `kind`, `role` or
/// `chapter_source` is not one of the values [`apply`] ever writes, or if
/// SQLite fails.
pub fn of(conn: &Connection, work_id: i64) -> Result<LibraryDoc, String> {
    let (volume, series_id, title, effective_series): (
        Option<f64>,
        Option<i64>,
        Option<String>,
        Option<i64>,
    ) = conn
        .query_row(
            // **The hand columns, never the effective ones — O80.** Projecting
            // plain `volume`/`series_id` made every document claim whatever the
            // last derive wrote, so a title-only Save recorded derivation's own
            // answers as a person's and `write_work` froze them. Migration 011
            // gave these two the provenance `title_hand` has had since 010.
            "SELECT volume_hand, series_id_hand, title_hand, series_id \
             FROM work WHERE id = ?1",
            [work_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|e| format!("reading work {work_id}: {e}"))?;

    // **The series block is projected from the *effective* id, and says so
    // separately — O80.** A work whose attachment is derived can still belong
    // to a series a person has renamed, and that correction has to reach the
    // document; `series_info_of` returns only the hand-set titles and reading,
    // so a series nobody has touched contributes nothing. `chosen` carries the
    // one fact that is about this work rather than the series.
    let series = match effective_series {
        Some(id) => SeriesInfo {
            chosen: series_id.is_some(),
            ..series_info_of(conn, id)?
        },
        None => SeriesInfo::default(),
    };
    let editions = editions_meta_of(conn, work_id)?;
    let members = members_meta_of(conn, work_id)?;
    let tags = tags_of(conn, work_id)?;
    let external = match effective_series {
        Some(id) => external_of(conn, id)?,
        None => BTreeMap::new(),
    };

    Ok(LibraryDoc {
        taiyaku_library: VERSION,
        updated: now_rfc3339(conn)?,
        series,
        volume,
        title,
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
            // `reading_hand`: derivation's own reading is not a correction
            // and must not appear in a document (O80).
            "SELECT reading_hand FROM series WHERE id = ?1",
            [series_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("reading series {series_id}: {e}"))?;
    let reading = reading.as_deref().map(parse_direction).transpose()?;

    let titles = titles_where(
        conn,
        series_id,
        // `hand = 1` only — derivation inserts an `is_primary` row for
        // every series it mints, and projecting those made every scanned
        // work's document non-empty (O53's recorded consequence, and the
        // same defect as O80 one level up).
        "SELECT title, kind, is_primary FROM series_title \
         WHERE series_id = ?1 AND hand = 1 ORDER BY rowid",
    )?;

    // `chosen` is `of`'s to set: it is a fact about a work, and this function
    // is given only a series.
    Ok(SeriesInfo {
        titles,
        reading,
        chosen: false,
    })
}

/// Every title `series_id` holds, in the order they were written: the list
/// `GET /api/library/series/{id}` answers, and the set [`set_series_titles`]
/// replaces whole. O83.
///
/// **Every row, derivation's included, which is where this differs from
/// [`of`].** A document carries only `hand = 1` titles (O80), but
/// [`set_series_titles`] deletes every row for the series before it writes
/// the list it is given. A caller that read a document's list, corrected one
/// entry and sent it back would delete the title derivation gave the series,
/// so this answers the whole set that function replaces.
///
/// **`rowid` order**, as [`of`] reads. [`set_series_titles`] inserts in the
/// order it is given, so a list read, edited in place and sent back reads
/// back in that same order.
///
/// # Errors
///
/// Returns an error if SQLite fails, or if a row's `kind` is not one of
/// [`TitleKind`]'s.
pub fn series_titles(conn: &Connection, series_id: i64) -> Result<Vec<SeriesTitle>, String> {
    titles_where(
        conn,
        series_id,
        "SELECT title, kind, is_primary FROM series_title \
         WHERE series_id = ?1 ORDER BY rowid",
    )
}

/// The titles `sql` selects: `title`, `kind` and `is_primary`, in that
/// order, with `series_id` bound as `?1`. [`series_info_of`] and
/// [`series_titles`] differ only in which rows they ask for.
fn titles_where(conn: &Connection, series_id: i64, sql: &str) -> Result<Vec<SeriesTitle>, String> {
    let mut stmt = conn
        .prepare(sql)
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

    rows.into_iter()
        .map(|(title, kind, primary)| {
            Ok(SeriesTitle {
                title,
                kind: parse_title_kind(&kind)?,
                primary,
            })
        })
        .collect()
}

fn editions_meta_of(
    conn: &Connection,
    work_id: i64,
) -> Result<BTreeMap<String, EditionMeta>, String> {
    let mut stmt = conn
        // Hand columns only, for the reason `of` gives above (O80).
        .prepare(
            "SELECT id, key, label_hand, language_hand, kind_hand \
             FROM edition WHERE work_id = ?1",
        )
        .map_err(|e| format!("preparing editions: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| format!("querying editions: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading editions: {e}"))?;

    let mut out = BTreeMap::new();
    for (edition_id, key, label, language, kind) in rows {
        let kind = kind.as_deref().map(parse_edition_kind).transpose()?;
        let credits = credits_of(conn, edition_id)?;
        if label.is_none() && language.is_none() && kind.is_none() && credits.is_empty() {
            continue;
        }
        out.insert(
            key,
            EditionMeta {
                label,
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
/// not left behind from an earlier one. `work.title` and `edition.label` are
/// `NOT NULL`, so for them "cleared" means restored to the value derivation
/// minted — `title_derived`/`label_derived`, migration 010's copies — rather
/// than blanked, which a `NOT NULL` column cannot hold anyway.
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
    // `crate::Nested`, not `unchecked_transaction` directly: `derive::write_work`
    // reaches this through `librarydoc::apply` while `preview::preview_with_rows`
    // holds the whole derivation in a transaction of its own — a plain `BEGIN`
    // here refuses outright the moment that outer transaction is open, and the
    // refusal used to be swallowed into `DeriveReport::unreadable_hand` rather
    // than surfaced (whole-branch review, Critical 1). See `Nested`'s own doc
    // for why a `SAVEPOINT` cannot simply replace `unchecked_transaction`
    // everywhere instead.
    let tx = crate::Nested::open(conn, "librarydoc_apply")
        .map_err(|e| format!("starting library-metadata transaction: {e}"))?;

    // A plain `UPDATE ... WHERE id = ?` affects zero rows for a work_id that
    // does not exist rather than failing, so the row count is checked by
    // hand — otherwise every write below would silently no-op instead of
    // refusing, the one outcome this function's own doc comment promises
    // not to produce.
    // `title` is written to two columns: `title_hand` is the provenance this
    // document projects from, and `title` is what every reader of a work
    // actually displays. **`COALESCE(?2, title_derived)`, never
    // `COALESCE(?2, title)`** — falling back to the column being written
    // leaves a withdrawn hand title in place forever, because the value
    // derivation minted was destroyed by the first edit that overwrote it and
    // nothing recomputes one. `title_derived` is migration 010's copy of it,
    // so clearing a hand title restores derivation's own answer, which is the
    // only sense in which "no hand title" can be written to a NOT NULL column.
    //
    // **Two arms, not three.** Every production writer of this row keeps
    // `title_derived` populated: migration 010 backfills it for every row
    // that existed, `import::insert_work` mints it alongside `title` for
    // every row created since, `import::update_work`'s `Scope::Document`
    // branch (Step 6c) refreshes it on every reimport, and `taiyaku-serve`'s
    // `works::row_id_for_document` mints it alongside `title` for a work
    // bridged in with no prior library row at all — a fourth writer, and the
    // one that once did not (I3). `title_derived` carries no `NOT NULL` of
    // its own, so nothing enforces this at the schema level: the claim above
    // is only as good as `grep -rn 'INSERT.*INTO work'` across the workspace
    // naming every production call site there is, re-checked here rather
    // than trusted from the last time it was true. With that held, there is
    // no row on which `title_derived` can read NULL for this `COALESCE` to
    // need a third arm to step over. Keeping `title` as a defensive third arm
    // would not be free: it would make exactly the invariant violation this migration
    // exists to prevent (`title_derived` unexpectedly NULL) fail *silently*,
    // by falling back to whatever `title` already held — a stale value rather
    // than an error. Dropping it means that violation instead raises SQLite's
    // own `NOT NULL` constraint the moment someone clears a hand title with no
    // derived value to restore, which is the loud failure this codebase
    // prefers (see `apply`'s own row-count guard, a few lines above).
    let updated = tx
        .execute(
            // **`volume_hand` is the provenance, `volume` the effective
            // value — O80, migration 011.** `COALESCE(?1, volume)` rather
            // than `COALESCE(?1, volume_derived)`: unlike `title`, this
            // column is recomputed by `derive::write_work` on every pass, so
            // there is no derived copy to keep, and clearing restores
            // derivation's answer on the next scan rather than instantly.
            "UPDATE work
                SET volume_hand = ?1,
                    volume      = COALESCE(?1, volume),
                    title_hand  = ?2,
                    title       = COALESCE(?2, title_derived)
              WHERE id = ?3",
            params![doc.volume, doc.title, work_id],
        )
        .map_err(|e| format!("writing volume and title for work {work_id}: {e}"))?;
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

/// [`apply`], for a caller that has already resolved this work's series
/// identity and does not want it re-resolved: `taiyaku_serve::metadata::patch_series`'s
/// own per-work loop, once [`set_series_titles`] has already written the
/// series named in its URL by id.
///
/// **Does not call [`apply_series`] — at all.** `apply_series`'s job is
/// deciding series identity: resolving `doc.series.titles` by text through
/// [`find_series_by_title`], possibly minting a new `series` row, possibly
/// moving `work.series_id` onto a different one, possibly orphaning the one
/// this work is leaving. None of that is safe to re-run once
/// [`set_series_titles`] has already put the corrected text on the series
/// the caller means: `find_series_by_title` resolves by text with no `LIMIT`
/// and no notion of "the id already in hand", and `series_title`'s primary
/// key is `(series_id, title)` — the same text can sit on two different
/// series. If the corrected text happened to collide with a title some other
/// series already held, `apply_series` would resolve there instead, every
/// work in the loop would migrate onto it, and once the last one left,
/// [`forget_orphaned_series`] would delete the series the caller actually
/// named — cascading its titles and external ids with it. (Delta review of
/// the fix waves, `f133b3e..910b15a`: the Critical, reproduced end to end
/// against `patch_series` before this function existed.)
///
/// `series_id` is `work_id`'s series exactly as the caller already knows it —
/// passed in rather than read back, since `work.series_id` has not changed
/// under this code path and does not need re-reading to learn what the
/// caller already has in hand. `doc.series.titles` itself is not written
/// here at all: [`set_series_titles`] already wrote the whole set, once, for
/// the whole series, before the caller's loop began.
///
/// **`doc.series.reading` is still written, directly onto `series_id`.**
/// Unlike titles, a reading direction carries no identity to resolve —
/// writing it is a plain `UPDATE … WHERE id = series_id`, the same shape
/// [`set_series_titles`] already uses for `sort_title`, and no more unsafe to
/// repeat on every work in a series-wide edit's loop than that one already
/// is. `apply_series`'s own matched branch is the only other writer of this
/// column, and this function does not reach it, so this is the one this
/// function's own caller needs instead.
///
/// **Idempotent, for the same reason [`apply`] is.** Every sub-write this
/// function performs — the reading-direction `UPDATE`, [`apply_external`],
/// [`apply_editions`], [`apply_members`], [`apply_tags`] — is the identical
/// idempotent building block [`apply`] itself calls or an equally plain
/// `UPDATE … WHERE id = `; only the series-resolving step in between is
/// missing here, and that step decided no persisted state this function
/// depends on.
///
/// # Errors
///
/// Same as [`apply`], for the same reasons, minus anything to do with
/// resolving `doc.series.titles`.
pub fn apply_with_series_id(
    conn: &Connection,
    work_id: i64,
    series_id: Option<i64>,
    doc: &LibraryDoc,
) -> Result<(), String> {
    // `crate::Nested`, not `unchecked_transaction` directly — same reason as
    // `apply`'s own, above: this function is reached from the same
    // `derive::write_work` path a wrapping preview transaction can already
    // have open.
    let tx = crate::Nested::open(conn, "librarydoc_apply_with_series_id")
        .map_err(|e| format!("starting library-metadata transaction: {e}"))?;

    // Same row-count guard and the same `COALESCE(?2, title_derived)` choice
    // as `apply`'s own — see that function's doc comment for why.
    let updated = tx
        .execute(
            // **`volume_hand` is the provenance, `volume` the effective
            // value — O80, migration 011.** `COALESCE(?1, volume)` rather
            // than `COALESCE(?1, volume_derived)`: unlike `title`, this
            // column is recomputed by `derive::write_work` on every pass, so
            // there is no derived copy to keep, and clearing restores
            // derivation's answer on the next scan rather than instantly.
            "UPDATE work
                SET volume_hand = ?1,
                    volume      = COALESCE(?1, volume),
                    title_hand  = ?2,
                    title       = COALESCE(?2, title_derived)
              WHERE id = ?3",
            params![doc.volume, doc.title, work_id],
        )
        .map_err(|e| format!("writing volume and title for work {work_id}: {e}"))?;
    if updated == 0 {
        return Err(format!("work {work_id} does not exist"));
    }

    if let Some(series_id) = series_id {
        // `apply_series`'s own matched branch is the only other writer of
        // this column — see this function's own doc comment for why writing
        // it here, directly by id, carries none of the hazard writing
        // `series_title` by id-instead-of-text would.
        let reading = doc.series.reading.map(direction_text);
        tx.execute(
            // `reading_hand` beside it — this is a person's PATCH, and `of`
            // projects the provenance (O80, migration 011).
            "UPDATE series
                SET reading_hand = ?1,
                    reading      = COALESCE(?1, reading)
              WHERE id = ?2",
            params![reading, series_id],
        )
        .map_err(|e| format!("updating series {series_id}'s reading direction: {e}"))?;
    }

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
        // Only the provenance is cleared; `derive` re-attaches the work to
        // whatever series the next pass resolves (O80, migration 011).
        conn.execute(
            // **The hand attachment is withdrawn; the effective one goes only
            // if that is what it was — O80.** An empty series block no longer
            // means "take this work off the shelf": since `of` projects only
            // *hand-set* titles it means "nobody has corrected this series",
            // which is true of nearly every work in a scanned library, and
            // clearing `series_id` unconditionally detached all of them on the
            // first apply. A work a person *placed* is a different case: the
            // document withdrawing that placement really is asking for it
            // back, and `derive` re-attaches on the next pass if the files say
            // so.
            "UPDATE work
                SET series_id = CASE WHEN series_id_hand IS NULL
                                     THEN series_id ELSE NULL END,
                    series_id_hand = NULL
              WHERE id = ?1",
            [work_id],
        )
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
                "UPDATE series
                    SET sort_title   = ?1,
                        reading_hand = ?2,
                        reading      = COALESCE(?2, reading)
                  WHERE id = ?3",
                params![sort_title, reading, id],
            )
            .map_err(|e| format!("updating series {id}: {e}"))?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO series (sort_title, reading, reading_hand) VALUES (?1, ?2, ?2)",
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
        // `PRAGMA recursive_triggers` is on, which `Library::configure`
        // sets for exactly this reason. A connection opened any other way
        // would leave a phantom row in `series_fts` here, silently.
        conn.execute(
            // `hand = 1`: a title arriving in a document is a person's, and
            // `of` projects only those (O80). Derivation's own `is_primary`
            // row keeps the default 0 and stays out of every document.
            "INSERT OR REPLACE INTO series_title (series_id, title, kind, is_primary, hand)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![
                series_id,
                title.title,
                title_kind_text(title.kind),
                title.primary
            ],
        )
        .map_err(|e| format!("writing series title `{}`: {e}", title.title))?;
    }

    // **The attachment is recorded as a person's only when the document says
    // it is — O80, `SeriesInfo::chosen`.** The effective column is written
    // either way: resolving by title is how a document reaches a database
    // that has never seen this library, and that resolution has to land the
    // work somewhere. What it must not do is claim the landing was chosen,
    // because a document that only corrects a series' *titles* would then
    // freeze every member work's attachment — O80's defect moved one table
    // across rather than fixed.
    conn.execute(
        "UPDATE work
            SET series_id_hand = CASE WHEN ?3 THEN ?1 ELSE series_id_hand END,
                series_id      = ?1
          WHERE id = ?2",
        params![series_id, work_id, series.chosen],
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
/// **This used to be unreachable with a series to forget (delta review M5),
/// and spec C3's PATCH routes changed that.** [`apply`] no longer has a single
/// production caller: `crates/taiyaku-serve/src/metadata.rs`'s `edit_work_raw`
/// reaches it from every one of the four PATCH routes, and
/// `derive::write_work`'s own read-back reaches it too, both alongside the
/// original `migrate::apply_library`. `patch_work`'s own `series` field is the
/// one that can hand this function a real series to forget: it sets
/// `doc.series` from the request body outright, so moving a work onto a
/// different series or clearing its series entirely resolves `previous` (this
/// work's series before the edit) to `Some`, and this function deletes that
/// series once nothing else — no work, no `collection_item` — still holds it.
///
/// **`patch_series` does not reach this path — corrected, not merely
/// restated.** An earlier version of this comment claimed `patch_series`
/// never reaches here because `apply_series`'s resolved `series_id` always
/// agreed with `previous`. That was false, and the delta review of the fix
/// waves (`f133b3e..910b15a`) is why: `patch_series`'s per-work loop used to
/// call the ordinary [`apply`], which reaches `apply_series`, and
/// `apply_series` resolves by matching `doc.series.titles`' own *text* —
/// nothing about that resolution guarantees agreement with `previous` at
/// all. When [`set_series_titles`] had just written a title text some other
/// series already held, `apply_series` resolved onto that other series
/// instead, `previous` and the newly resolved `series_id` disagreed, and this
/// function deleted the series the caller actually named. `patch_series`'s
/// loop now calls [`apply_with_series_id`] instead, which does not reach
/// `apply_series` — or this function — at all, so the true reason `patch_series`
/// does not reach this path is structural: nothing in it calls the function
/// that would. This function is kept as `pub` API regardless, since
/// `patch_work`'s own `series` field still exercises it, moving a work onto a
/// different series or clearing it entirely.
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

/// Overwrite `series_id`'s own title set, by the id already resolved — never
/// by matching text through [`find_series_by_title`].
///
/// **What a PATCH to `/api/library/series/{id}` needs and [`apply_series`]
/// cannot give it.** A PATCH already names its series by id; resolving its
/// titles by text anyway can answer with a *different* series than the one
/// the URL named. If no existing `series_title` row shares any of the
/// corrected text, [`apply_series`] mints a brand new series for it and, once
/// the last work has moved onto that new row, [`forget_orphaned_series`]
/// deletes the one the URL actually asked to edit — its id, and everything
/// else keyed on it, gone. If some of the text still matches (an unchanged
/// title alongside a corrected one), [`apply_series`] upserts onto the
/// existing row set rather than replacing it, so the corrected spelling lands
/// *beside* the one it was meant to replace rather than instead of it. This
/// function has neither failure available to it: it writes exactly
/// `series_id`, and only that row's own title set, deciding no series
/// identity along the way.
///
/// **Replaces, not upserts.** Every `series_title` row `series_id` held is
/// gone (an explicit `DELETE`, so `series_title_ad`/`series_title_ai` keep
/// `series_fts` in step with no `recursive_triggers` gate needed — see
/// [`forget_orphaned_series`]'s own comment on why an explicit `DELETE`
/// differs from the implicit one an `INSERT OR REPLACE` conflict performs)
/// before `titles` is written back. `series.sort_title` is refreshed from
/// whichever of `titles` is primary, or the first if none is — the same rule
/// [`apply_series`]'s own matched branch applies.
///
/// **`work.series_id` and the `series` row itself are untouched.** Deciding
/// which series a work belongs to — moving it, or clearing it — is
/// [`apply_series`]'s job, reached through [`apply`] from `patch_work`'s own
/// `series` field. This function never runs that decision, so it can neither
/// mint a `series` row nor delete one.
///
/// **Refuses a title text some other series already holds.** This is the
/// door the Critical the delta review of the fix waves (`f133b3e..910b15a`)
/// found walks through: this function's own plain `INSERT` was the only
/// writer anywhere in the workspace that could put the same title text on
/// two different series at once — `derive::reconcile_series` resolves by
/// `sort_title` first, and [`apply_series`] resolves by text, so neither can
/// manufacture the collision this checks for. Once the ambiguous state
/// existed, [`find_series_by_title`]'s own un-`LIMIT`ed `SELECT` could answer
/// either series for the same text, and [`apply_series`] resolving onto the
/// wrong one is what let the series this function was asked to edit be
/// deleted out from under it. Checked before anything else here writes,
/// against every title in `titles`, not only the primary — an `alt` or
/// `native` entry colliding is exactly as hazardous as the sort title doing
/// so, since [`find_series_by_title`] tries every one of a document's titles
/// in turn.
///
/// **Refuses more than one title marked `primary`.** `browse.rs`'s own
/// `COALESCE(MAX(CASE WHEN st.is_primary = 1 THEN st.title END), s.sort_title)`
/// picks whichever of them sorts lexically greater when more than one row
/// has the flag — C1's second-shape symptom (a corrected spelling landing
/// beside the one it was meant to replace, so the reader-facing title
/// depends on string order rather than on which one a person actually meant)
/// by a different door, and this is the one writer that could let more than
/// one ever exist for one series at once — `apply_series`'s own
/// `INSERT OR REPLACE`, keyed on `(series_id, title)`, cannot: two distinct
/// title *texts* upserted with `primary: true` simply overwrite the same
/// row's flag in turn, one at a time, never coexisting.
///
/// # Errors
///
/// Returns an error if `titles` is empty. A series left with no title at all
/// is exactly the membership-clearing, maybe-delete-the-series outcome
/// [`apply_series`]'s own empty-titles branch exists to produce when a work
/// leaves its series — not a thing this function's caller ever asks for, and
/// not reachable through it. Returns an error if more than one of `titles`
/// is marked `primary` — this function's own doc comment above has the
/// reason. Returns an error if any of `titles` names text a *different*
/// series already holds — this function's own doc comment above has the
/// reason. Also returns an error if SQLite fails.
pub fn set_series_titles(
    conn: &Connection,
    series_id: i64,
    titles: &[SeriesTitle],
) -> Result<(), String> {
    let Some(primary) = titles.iter().find(|t| t.primary).or_else(|| titles.first()) else {
        return Err("a series must keep at least one title".to_string());
    };
    if titles.iter().filter(|t| t.primary).count() > 1 {
        return Err(format!(
            "series {series_id} would have more than one primary title; \
             a series must have at most one"
        ));
    }
    let sort_title = primary.title.clone();

    // `crate::Nested`, not `unchecked_transaction` directly. No production
    // caller today reaches this function with a transaction already open on
    // `conn` — `metadata::patch_series` calls it on `Library::conn()` before
    // its own per-work loop, never underneath a derivation — but it is the
    // same `apply`-family shape as `apply` and `apply_with_series_id` just
    // above, opened on the same kind of connection a preview can hold open,
    // so it is converted for the same reason rather than left as the one
    // survivor of this class of bug.
    let tx = crate::Nested::open(conn, "librarydoc_set_series_titles")
        .map_err(|e| format!("starting a series-title transaction: {e}"))?;

    for title in titles {
        let collision: Option<i64> = tx
            .query_row(
                "SELECT series_id FROM series_title WHERE title = ?1 AND series_id != ?2",
                params![title.title, series_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("checking for a title collision on `{}`: {e}", title.title))?;
        if let Some(other) = collision {
            return Err(format!(
                "title `{}` already belongs to series {other}; refusing to \
                 give series {series_id} the same text",
                title.title
            ));
        }
    }

    tx.execute(
        "UPDATE series SET sort_title = ?1 WHERE id = ?2",
        params![sort_title, series_id],
    )
    .map_err(|e| format!("updating series {series_id}: {e}"))?;

    tx.execute("DELETE FROM series_title WHERE series_id = ?1", [series_id])
        .map_err(|e| format!("clearing titles for series {series_id}: {e}"))?;

    for title in titles {
        tx.execute(
            // `hand = 1`: this function exists for `PATCH /api/library/series`,
            // so every row it writes is a person's (O80).
            "INSERT INTO series_title (series_id, title, kind, is_primary, hand)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![
                series_id,
                title.title,
                title_kind_text(title.kind),
                title.primary
            ],
        )
        .map_err(|e| format!("writing series title `{}`: {e}", title.title))?;
    }

    tx.commit()
        .map_err(|e| format!("committing series titles for series {series_id}: {e}"))
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
/// `language`/`kind`/`label_hand` reset — and `label` restored to
/// `label_derived` in that same statement, since it is `NOT NULL` and cannot
/// simply be cleared — before `doc.editions` is applied over them, and every
/// edition-scoped `credit` row cleared and rewritten from scratch.
fn apply_editions(
    conn: &Connection,
    work_id: i64,
    editions: &BTreeMap<String, EditionMeta>,
) -> Result<(), String> {
    // **`label` restored alongside the provenance it is cleared from.** An
    // edition dropped from the document entirely — the key absent, a
    // different statement from `label: None` that takes this branch rather
    // than the per-key update below — must not keep its withdrawn hand label
    // forever while `label_hand` reads NULL.
    //
    // **`COALESCE(label_derived, label)`, not a bare `label = label_derived`
    // — but under the invariant the two are equivalent, not distinguished by
    // any `NOT NULL` argument.** `label_derived` is never NULL on a row this
    // statement can reach, the same as everywhere else this invariant is
    // held, so both shapes restore the identical value here. That is *not*
    // the same choice `apply`'s own title write makes above: that write
    // deliberately dropped its own fallback-to-the-column-being-written arm,
    // on the argument that a violated invariant (`title_derived` NULL) should
    // fail loudly as a `NOT NULL` error rather than silently keep a stale
    // title — and that argument applies here too, so it is not why this
    // statement keeps the fallback instead of matching it. What does
    // distinguish this statement is structural, not `NOT NULL`-related: it
    // sweeps every edition of the work in one `UPDATE`, with no per-row
    // existence check the way `apply`'s own row-count check and the per-key
    // update's `edition_id` lookup below both have. Keeping `label` as the
    // fallback costs nothing while the invariant holds, and only matters if
    // some future writer ever violates it on a row this statement reaches —
    // in which case this shape absorbs that quietly here, on whichever edition
    // it was, rather than raising a constraint failure that names no edition
    // at all.
    conn.execute(
        // **Both halves cleared, effective included.** The provenance is
        // what `derive` consults (O80, migration 011), but leaving the
        // effective column populated would show a withdrawn hand value until
        // the next scan, and `a_second_apply_without_an_edition_entry_clears_
        // its_previous_metadata` is the test that says so. `write_edition_metadata`
        // re-derives both on the next pass, so nothing is lost by blanking
        // them now — unlike `label`, which has no derivation to restore it and
        // therefore falls back to `label_derived`.
        "UPDATE edition
            SET label_hand    = NULL,
                label         = COALESCE(label_derived, label),
                language_hand = NULL,
                language      = NULL,
                kind_hand     = NULL,
                kind          = NULL
          WHERE work_id = ?1",
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

        // Same `COALESCE(?1, label_derived)` reasoning as `apply`'s own
        // `title` write above: `label_hand` is this document's provenance,
        // and clearing it must restore `label_derived` — migration 010's copy
        // of what derivation or the document minted — not the effective
        // `label` column being written, which would leave a withdrawn hand
        // label in place forever.
        //
        // **Two arms, not three**, for the same reason as `apply`'s `title`
        // write: `label_derived` is kept populated by every writer of this
        // row — migration 010's backfill, `import::replace_editions`'s own
        // INSERT for a brand-new edition, and its restore for one already
        // held (Step 6c) — so there is no row on which it can read NULL here.
        // A third arm falling back to `label` would turn that invariant's
        // violation into a silently stale value instead of the loud
        // `NOT NULL` constraint failure a violation should produce.
        conn.execute(
            "UPDATE edition
                SET label_hand    = ?1,
                    label         = COALESCE(?1, label_derived),
                    language_hand = ?2,
                    language      = ?2,
                    kind_hand     = ?3,
                    kind          = ?3
              WHERE id = ?4",
            params![
                meta.label,
                meta.language,
                meta.kind.map(edition_kind_text),
                edition_id
            ],
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
