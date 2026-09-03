//! Absorbing a superseded work into the work its material moved to.
//!
//! **The only code in this project that deletes a `work` row**, which is why
//! it is a module rather than a function inside `derive`. The safety argument
//! is an ordering argument and it should be readable in one place: the reader
//! rows move first, in one transaction, and the delete is the last statement.
//! Reversed, `work`'s `ON DELETE CASCADE` takes a reader's position, their
//! notes and the library's ground truth with it — measured, not feared, on
//! 2026-08-27 against the four-archive reproduction that opened O73.
//!
//! **Eleven foreign keys name `work(id)` and every one of them cascades** —
//! `alignment`, `collection_item`, `credit`, `edition`, `migrated_document`,
//! `note`, `progress`, `truth_box`, `truth_member`, `truth_page` and
//! `work_tag`, measured through `pragma_foreign_key_list` against the schema
//! at migration 9. Four more follow transitively: `member` and `page_flag`
//! through `edition`, `run` through `alignment`, and `work_fts` via the
//! `work_ad AFTER DELETE` trigger in `001_initial.sql`. Three of those tables
//! hold a reader's own state and are moved by [`absorb`]; the rest are counted
//! as [`Moved::discarded`], because a delete this wide must not be able to
//! happen quietly.
//!
//! **The guarantee is per row, not per table.** Moving `progress`, `note`,
//! `truth_page` and `truth_member` is not the same as moving *every* row in
//! them: all four `edition` columns are plain `TEXT` with no foreign key to
//! `edition(key)`, and every mover here is keyed on a key out of the
//! `edition` table. A row on a key that table does not hold is moved by
//! nothing, counted by nothing and taken by the delete — which is why
//! [`absorb`] looks for exactly that shape and refuses. See [`stranded`].
//!
//! **What this module is *not* asked to decide.** Which work is superseded
//! and which absorbed it is `derive::superseded`'s answer, reached from
//! member digests, paths and roots. [`absorb`] is handed that pair and asks
//! four questions of its own — which edition of the target holds each edition
//! of the superseded work's archives; whether anything the reader owns is on
//! a key neither side can name; whether one reader holds one note id on both
//! works; and whether the ground truth meeting on a mapped edition was drawn
//! against the same archives on the two sides — and refuses the whole fold
//! when the answer to any of them is wrong.
//!
//! **A refusal is recoverable, a delete is not, and an `Err` is neither.** A
//! refused fold leaves a volume visibly duplicated, which an operator can see
//! and act on. A destroyed note has no file on disk behind it. And an `Err`
//! travels through `derive`'s `?` and kills the whole derive — every later
//! pair unfolded, `series_holding_no_work` and `proposals::raise` never reached —
//! which is the "worse than folding nothing" this module's [`Outcome`] exists
//! to avoid. Every state named above is therefore read for and refused rather
//! than met as a constraint failure mid-write.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

/// What one absorption moved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    /// Rows this fold wrote onto `into_work`'s `progress`.
    ///
    /// **Writes, not survivors, and the difference is reachable.**
    /// [`edition_map`] is non-injective, so two editions of `from_work` can
    /// upsert the same `(user_id, into_key)` row inside one fold: the first
    /// `execute` inserts, the second updates, and both are counted here while
    /// one row exists at the end. The number that is exact about what the
    /// reader is left holding is [`Self::lost_positions`], which is taken
    /// over the whole fold for precisely that reason. This one says how many
    /// times the fold wrote.
    pub positions: usize,
    pub notes: usize,
    pub truth_pages: usize,
    /// Truth pages the target already held, kept as the target's.
    ///
    /// **The unit is a page, not a box, and the difference can be large.** A
    /// target page carrying one box against a source page carrying forty
    /// reads `1` here: this counts labellings declined, not regions lost. A
    /// per-box number would be the size of the loss, and is not what this is
    /// — a box count would need its own query per page and would say nothing
    /// about how many pages a person has to go and look at, which is the
    /// question the count is for. `truth_pages` above is the same unit, so
    /// the two are comparable.
    pub collisions: usize,
    /// Reading positions that existed before this fold and do not exist
    /// after it — the source's, the target's, or both.
    ///
    /// **A whole-fold identity, and it is the second attempt.** The first was
    /// `offered - landed` per edition, taken off SQLite's `changes`, and that
    /// subtraction can only see a losing row that was *skipped*.
    /// [`edition_map`] is explicitly non-injective, so two editions of
    /// `from_work` can upsert the same `(user_id, into_key)` row: in
    /// `BTreeMap` key order, when the later-sorting key carries the newer
    /// reading the first `execute` inserts and the second *updates*, both
    /// report `changes = 1`, both count as landed, one row survives and one
    /// reading position is destroyed and reported as moved. Replayed in
    /// SQLite on 2026-08-27 — work 2 holding `raw` slot 5 @2020 and `scan`
    /// slot 9 @2026, both mapped onto work 1's `full-color` — gave
    /// `raw offered=1 landed=1`, `scan offered=1 landed=1` and one surviving
    /// row at slot 9, the `raw` reading gone with nothing counting it.
    ///
    /// So the number is taken over the fold as a whole instead:
    /// `offered + held - after`, where `offered` is the `progress` rows on
    /// `from_work` read before the move loop, `held` the rows on `into_work`
    /// read before it, and `after` the rows on `into_work` read when the loop
    /// is done. Every remaining source row is destroyed by the final
    /// `DELETE`, so what exists after the fold is exactly `after`, and the
    /// difference is every position that stopped existing. It cannot miss a
    /// case, and it does not depend on which edition key sorts first or on
    /// which side won.
    ///
    /// **It covers a pre-existing target row overwritten by an incoming
    /// one**, which the subtraction did not and which the previous round
    /// named as a residual and left uncounted. Where the source's row is the
    /// newer, the `DO UPDATE` replaces the target's slot, `completed_at` and
    /// `read_count` entire; that reading is as gone as a declined one, and
    /// counting the write in [`Self::positions`] said nothing about it.
    ///
    /// **Rows on unmapped keys cancel rather than distort.** [`stranded`] has
    /// already refused any `from_work` reader row on a key the map does not
    /// hold, so `offered` is exactly what the loop is about to carry; and a
    /// grandfathered `into_work` row on a key no `into_key` names is untouched
    /// by every statement below, so it is in `held` and in `after` both.
    ///
    /// "The newer position wins" is unchanged — a slot number is not
    /// mergeable and the later reading is the one somebody is in the middle
    /// of. What this number exists for is that the loss is never silent.
    pub lost_positions: usize,
    /// Rows the final `DELETE` took that this fold did not move — see
    /// [`DISCARDED`], which enumerates them.
    pub discarded: usize,
}

/// What [`absorb`] did.
///
/// **A refusal is an outcome, not an error**, and the distinction is the
/// whole reason this type exists rather than a bare `Result<Moved, String>`.
/// A fold that cannot be mapped must leave the work standing and let the scan
/// carry on — a scan that died because one work of thousands could not be
/// folded is worse than a scan that folds nothing — while a SQLite failure
/// really is an error and travels as one. Collapsing the two would make
/// `derive` choose between aborting on the harmless case and swallowing the
/// harmful one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The reader's rows moved and the superseded work is gone.
    Folded(Moved),
    /// Nothing moved, nothing was deleted, and this is why.
    Refused(String),
}

/// Every class of row that hangs off a superseded work, that [`absorb`] does
/// not move, and that the final `DELETE` therefore takes — as the table and
/// the predicate that finds it, with `?1` bound to the superseded work.
///
/// **Counted rather than moved, and that is a decision.** Each of these is
/// the library's *description* of the material rather than a reader's state,
/// and each would need its own merge rule against what the target already
/// holds: `credit` has no unique constraint, so a move duplicates a
/// scanlation group rather than reconciling it; `collection_item`'s
/// `(collection_id, ordinal)` key collides whenever the target sits in the
/// same reading list; `edition.label`, `language` and `kind` would overwrite
/// what this very derive just wrote onto the target's edition, and the next
/// derive would write it back — a flip-flop, not a migration. Choosing nine
/// merge rules with no evidence, inside the one function that deletes a work,
/// is more risk than the loss it avoids, and every one of these is visible
/// on screen and re-doable by hand where a reading position is not.
///
/// So they get a number instead, for [`crate::derive::DeriveReport`]'s own
/// reason: a silent loss is indistinguishable from no loss at all. Moving any
/// of them is a later item with a real fold in front of it, not a guess made
/// here.
///
/// **`edition` and `member` rows are absent deliberately.** Those are the
/// duplication O73 is about — the same archives classified twice — so
/// removing them is the fold succeeding, not a cost of it. What is counted
/// from them is only what a person put there by hand: a label that is no
/// longer the key, a language or kind, a chapter number no scan wrote.
const DISCARDED: [(&str, &str); 8] = [
    // **One entry, not one per column**, because `credit`'s `work_id` and
    // `edition_id` are separately nullable and a row setting both would be
    // counted twice by two entries. Nothing writes `work_id` today —
    // `import::replace_editions` and `librarydoc::apply` are the only two
    // writers and both name `edition_id` alone — but the schema permits it,
    // and a count that over-reports is as untrustworthy as one that
    // under-reports.
    (
        "credit",
        "work_id = ?1 OR edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
    ),
    ("alignment", "work_id = ?1"),
    (
        "run",
        "alignment_id IN (SELECT id FROM alignment WHERE work_id = ?1)",
    ),
    ("work_tag", "work_id = ?1"),
    ("collection_item", "work_id = ?1"),
    (
        "page_flag",
        "edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
    ),
    // A chapter number the scanner derived is re-derived onto the target by
    // `derive::write_chapters` from the same filename; one a person entered
    // is, in `member.chapter_declared`'s own words, "the one thing here no
    // re-derive could recover". Only the second is a loss, so only the second
    // is counted — matched by `chapter_source` rather than by the number
    // being present, which is the same test `write_chapters` clears by.
    (
        "member",
        "chapter_declared IS NOT NULL AND chapter_source IS NOT 'filename'
           AND edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
    ),
    // `derive::write_work` puts `key.clone()` in the label of every edition a
    // scan builds and `import::replace_editions` restores whatever was there
    // over it, so a label differing from the key is a person's — the
    // `Japanese raw` that comment is about. `language` and `kind` are NULL
    // unless a rule or a `<stem>.library.json` set them.
    (
        "edition",
        "work_id = ?1 AND (label IS NOT key OR language IS NOT NULL OR kind IS NOT NULL)",
    ),
];

/// Which edition of `into_work` holds the same archives as each edition of
/// `from_work`.
///
/// **By member digest, not by the rule that caused the fold.** The two
/// editions are the same bytes — measured at `920d44d8…`, 195 pages, on both
/// sides of O73's reproduction — so slots, page indices and box coordinates
/// transfer one for one with nothing to re-derive. Reading `set_edition_key`
/// off the rule instead would tie this to *why* the material moved and would
/// guess where no rule exists; digests hold for any later feature that moves
/// material between works.
///
/// **The list, in `ordinal` order, not the set.** Member order is page order,
/// so two editions holding the same archives in a different order are two
/// different renditions and every slot would land on the wrong page.
///
/// An edition with no counterpart is absent from the map, and [`absorb`]
/// refuses the whole fold rather than move half of it. An edition with no
/// members at all is absent too — the read below is a join — which is the
/// same refusal reached from the other side, and the safe one: nothing hangs
/// off such an edition but the metadata [`DISCARDED`] counts.
///
/// **The map is not required to be injective**, and does not try to be: two
/// editions of `from_work` holding identical archives both map onto the one
/// edition of `into_work` that holds them. Two renditions of the same bytes
/// are the same pages, so both sets of rows land where they mean the same
/// thing, and the ordinary rules then apply — the newer position wins, the
/// notes all move, the first labelling of a page keeps it and the second is
/// counted as a collision.
///
/// A `BTreeMap` rather than a `HashMap` so [`absorb`]'s loop runs the same
/// statements in the same order on every run, for `derive::superseded`'s own
/// stated reason.
fn edition_map(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
) -> Result<BTreeMap<String, String>, String> {
    let source = digests_by_edition(conn, from_work)?;
    let target = digests_by_edition(conn, into_work)?;
    let mut map = BTreeMap::new();
    for (key, digests) in source {
        if let Some((their_key, _)) = target.iter().find(|(_, theirs)| *theirs == digests) {
            map.insert(key, their_key.clone());
        }
    }
    Ok(map)
}

/// One work's editions as `(key, digests in ordinal order)`, in key order.
///
/// Grouped by consecutive key, which the `ORDER BY` is what makes correct:
/// `edition` is `UNIQUE (work_id, key)`, so one work's rows for a key are one
/// contiguous run.
fn digests_by_edition(
    conn: &Connection,
    work_id: i64,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT edition.key, member.sha256
               FROM edition JOIN member ON member.edition_id = edition.id
              WHERE edition.work_id = ?1
              ORDER BY edition.key, member.ordinal",
        )
        .map_err(|e| format!("preparing the edition read: {e}"))?;
    let rows = statement
        .query_map([work_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("reading editions of work {work_id}: {e}"))?;
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for row in rows {
        let (key, sha256) = row.map_err(|e| format!("reading an edition row: {e}"))?;
        match grouped.last_mut() {
            Some((held, digests)) if *held == key => digests.push(sha256),
            _ => grouped.push((key, vec![sha256])),
        }
    }
    Ok(grouped)
}

/// Every edition key on a work, mapped or not.
///
/// Read separately from [`digests_by_edition`], which joins `member` and so
/// cannot see an edition holding none — the one shape whose absence from the
/// map would otherwise read as "mapped".
fn edition_keys(conn: &Connection, work_id: i64) -> Result<Vec<String>, String> {
    conn.prepare("SELECT key FROM edition WHERE work_id = ?1 ORDER BY key")
        .and_then(|mut statement| {
            statement
                .query_map([work_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<String>>>()
        })
        .map_err(|e| format!("reading the edition keys of work {work_id}: {e}"))
}

/// Every table holding a reader's own state keyed by `(work_id, edition)`,
/// where `edition` is the plain `TEXT` this module has to check by hand.
///
/// **None of these four columns has a foreign key to `edition(key)`** —
/// measured against the schema at migration 9 — so SQLite will not stop a row
/// naming a key its work does not hold, and nothing else does either.
///
/// `truth_box` is absent because it cannot be stranded on its own:
/// `truth_box`'s composite foreign key names
/// `truth_page (work_id, edition, page)`, so a box exists only under a page
/// row on the same key, and checking the page finds it. Counting both would
/// report one stranded labelling twice.
const READER_TABLES: [&str; 4] = ["progress", "note", "truth_page", "truth_member"];

/// Stranded reader rows as `edition key -> [(table, rows)]`, in
/// [`READER_TABLES`] order inside each key. See [`stranded`], which is the
/// only thing that builds one.
type Stranded = BTreeMap<String, Vec<(&'static str, usize)>>;

/// Reader rows on `from_work` naming an edition key that `mapped` has no
/// entry for, as `key -> [(table, rows)]`.
///
/// **The guarantee this module makes is per row, and this is what makes it
/// so.** [`move_positions`], [`move_notes`] and [`move_truth`] are all keyed
/// on a `from_key` drawn from [`edition_map`], which is built from the
/// `edition` *table*. A row on a key that table does not hold is invisible to
/// all three, absent from [`DISCARDED`], and taken by [`absorb`]'s final
/// `DELETE` — measured 2026-08-27 by replaying `absorb`'s own statements
/// against the migrated schema, which reported `0` discarded and destroyed
/// four reader rows with nothing on screen. Pinned by
/// `fold::tests::a_reader_row_on_an_edition_the_work_does_not_hold_refuses_the_fold`.
///
/// **The state is reachable, and three places in this repository produce it.**
/// `taiyaku-serve`'s `notes` calls an edition key one of "the two things a
/// re-solve can invalidate underneath a note that nobody touched" and
/// grandfathers such a note deliberately, so it is a *supported* state rather
/// than corruption; `routes::put_truth` validates no edition key at all; and
/// `import::replace_editions` under `Scope::Document` deletes a work's
/// editions wholesale, stranding whatever named a key the incoming document
/// dropped or renamed.
///
/// **Why [`absorb`] refuses rather than counting these as [`DISCARDED`].**
/// That list is the library's own description of the material — derived,
/// visible on screen and re-doable by hand. This is the reader's own state,
/// with no file on disk behind it. Leaving a volume visibly duplicated is the
/// defect this fold exists to fix, and it is recoverable: an operator sees
/// the refusal and can act on it. A destroyed note is not.
///
/// **Per table, not just per key.** The count alone was ambiguous by
/// construction: four rows spread one per table reads `4`, and so does any
/// other four — `fold::tests`' own assertion on it would have passed just as
/// happily with `truth_member` dropped and `truth_box` counted twice. Naming
/// the table beside the number costs one `Vec` and is what an operator needs
/// anyway, the four tables being four different things to go and look at.
///
/// A `BTreeMap` for [`edition_map`]'s reason: the message names its keys in
/// the same order on every run. The `Vec` inside it is in [`READER_TABLES`]
/// order, that being the order the loop below fills it in.
fn stranded(
    conn: &Connection,
    from_work: i64,
    mapped: &BTreeMap<String, String>,
) -> Result<Stranded, String> {
    let mut found = Stranded::new();
    for table in READER_TABLES {
        // The table names are compile-time constants from `READER_TABLES`,
        // not anything a caller supplies — `discarded` says the same of its
        // own predicates.
        let mut statement = conn
            .prepare(&format!(
                "SELECT edition, count(*) FROM {table} WHERE work_id = ?1 GROUP BY edition"
            ))
            .map_err(|e| format!("preparing the stranded-row read of `{table}`: {e}"))?;
        let rows = statement
            .query_map([from_work], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("reading `{table}` rows of work {from_work}: {e}"))?;
        for row in rows {
            let (edition, count) =
                row.map_err(|e| format!("reading a `{table}` row of work {from_work}: {e}"))?;
            if !mapped.contains_key(&edition) {
                let count =
                    usize::try_from(count).map_err(|e| format!("counting `{table}` rows: {e}"))?;
                found.entry(edition).or_default().push((table, count));
            }
        }
    }
    Ok(found)
}

/// Note ids one reader holds on **both** works, in id order.
///
/// **The state is one migration 5 widened `note`'s key to permit**, and its
/// own rationale names the routes: "copying a `taiyaku.notes.json` between
/// volumes, or importing one sidecar against two works". It also records why
/// nothing upstream catches it — `notes::validate`'s duplicate check is a
/// `HashSet` over the submitted document and "cannot see another work's rows".
/// A fold pair is by construction two works over the same volume, which is
/// exactly when a reader would have copied notes between them.
///
/// **Read rather than caught.** [`move_notes`] is a bare `UPDATE` and the
/// collision comes back as `UNIQUE constraint failed: note.user_id,
/// note.work_id, note.id` — measured 2026-08-27 — which travels out of
/// [`absorb`] as an `Err`, through `derive`'s `?`, and kills the whole derive:
/// every later pair goes unfolded and `series_holding_no_work` and `proposals::raise`
/// never run. Nothing is destroyed, the transaction rolling back, so this is
/// fold-defeating rather than data-losing — but [`Outcome::Refused`] exists
/// for precisely this and catching the error after the write has begun would
/// give up the "a refusal costs the database nothing" property as well.
///
/// The whole work rather than the mapped edition, and that is exact rather
/// than approximate: [`stranded`] has already refused any note on a key the
/// map does not hold, so every note still on `from_work` at this point is one
/// [`move_notes`] is about to carry across.
fn shared_note_ids(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
) -> Result<Vec<String>, String> {
    conn.prepare(
        "SELECT DISTINCT s.id
           FROM note s
           JOIN note t ON t.user_id = s.user_id AND t.id = s.id AND t.work_id = ?2
          WHERE s.work_id = ?1
          ORDER BY s.id",
    )
    .and_then(|mut statement| {
        statement
            .query_map(rusqlite::params![from_work, into_work], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<String>>>()
    })
    .map_err(|e| format!("reading the note ids work {from_work} and work {into_work} share: {e}"))
}

/// The digests one work's ground truth on one edition was drawn against, in
/// `ordinal` order, empty when nothing was ever recorded.
fn truth_provenance(conn: &Connection, work_id: i64, edition: &str) -> Result<Vec<String>, String> {
    conn.prepare(
        "SELECT sha256 FROM truth_member
          WHERE work_id = ?1 AND edition = ?2 ORDER BY ordinal",
    )
    .and_then(|mut statement| {
        statement
            .query_map(rusqlite::params![work_id, edition], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<String>>>()
    })
    .map_err(|e| {
        format!("reading the truth provenance of work {work_id}, edition `{edition}`: {e}")
    })
}

/// Whether one work's edition carries any labelled page at all.
///
/// **Read beside [`truth_provenance`] rather than inferred from it.** The two
/// are separate rows written by one `state::put_truth` call, and a document
/// with `"members": []` and populated `pages` writes the second and not the
/// first — which is the whole state [`truth_provenance_clashes`] added its
/// second arm for. "No provenance" and "no boxes" are different facts and
/// only this tells them apart.
fn has_truth_pages(conn: &Connection, work_id: i64, edition: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM truth_page WHERE work_id = ?1 AND edition = ?2)",
        rusqlite::params![work_id, edition],
        |row| row.get::<_, bool>(0),
    )
    .map_err(|e| format!("reading whether work {work_id}, edition `{edition}` is labelled: {e}"))
}

/// Mapped editions where the ground truth arriving on one target key cannot be
/// given one honest provenance, in target-key order, each with its reason.
///
/// **`truth_member` is not derivable from the mapping, and the inference that
/// it was is the defect this exists to close.** [`edition_map`] compares
/// `member.sha256` — the archives as they are *now*. `truth_member` is a
/// client-supplied snapshot of what the boxes were *drawn against*, written
/// verbatim by `state::put_truth` with no validation of any kind. The two
/// diverging is not an edge case; it is the entire reason
/// `TruthEdition::is_stale_for` exists. So "the mapping is by identical digest
/// list, therefore a target that holds a `truth_member` row already holds the
/// same digests in the same order" is invalid, and the `INSERT OR IGNORE` it
/// justified moved the source's pages under the target's digests — boxes drawn
/// on superseded art recorded as fresh, which `score` would then accept.
/// `truth_member`'s own schema comment calls that "the sharpest form of this
/// problem in the project".
///
/// **Refused rather than merged or preferred.** Two provenance lists are two
/// different claims about which archives a labeller was looking at, and there
/// is no rule that reconciles them: keeping the target's records the source's
/// boxes against art they never saw, keeping the source's does the same to the
/// target's, and merging produces a list neither labeller ever had. Every one
/// of those is a silent falsehood in the one table `score` trusts, and a
/// standing duplicate work is recoverable where a mis-attributed box is not.
///
/// **Source against source as well as source against target**, because
/// [`edition_map`] is non-injective: two editions of `from_work` over
/// identical digests both map onto one edition of `into_work`, and their
/// `truth_member` snapshots can still differ. The `INSERT OR IGNORE` would
/// then silently keep whichever ran first. So the comparison is over every
/// list that lands on a given target key, not over one pair.
///
/// **An empty list is not a claim, and that is not the same as being
/// harmless.** A side that recorded nothing takes whichever provenance is
/// actually present — the ordinary O73 fold, where the target edition this
/// derive has just created holds no truth at all and the source's boxes
/// arrive with their own digests, pinned by
/// `truth_provenance_on_one_side_only_is_not_a_clash`. Refusing there would
/// defeat the fold this module exists to deliver.
///
/// **But a side holding boxes and no provenance is a claim being *acquired*,
/// and it is refused.** An earlier version of this function skipped a source
/// edition whose list was empty outright, and that exemption re-created the
/// defect the rest of it closes from the other side: a source carrying
/// `truth_page`/`truth_box` rows and no `truth_member` row passed, its boxes
/// were copied onto the target key, and the target's own non-empty list stood
/// over them through [`move_truth`]'s `INSERT OR IGNORE`.
/// `TruthEdition::is_stale_for` returns `true` when `members` is empty and
/// `digests` is not — so `score` correctly refuses those boxes as stale
/// *before* the fold and reads the very same boxes as fresh *after* it, which
/// is "boxes drawn on superseded art recorded as fresh" reached from the
/// other direction. The state is reachable: `state::put_truth` writes the
/// document's `members` as given and `routes::put_truth` validates neither
/// the edition key nor the member list, so any `TruthDoc` with
/// `"members": []` and populated `pages` produces it.
///
/// The test is therefore over the boxes rather than over the lists: what
/// lands on a target key must not be *one* claim over boxes that had none.
/// Zero claims is fine — nothing is asserted about anything — and two claims
/// is the clash above.
///
/// **Both sides, and every source at once.** The unclaimed boxes can be the
/// target's as easily as a source's, and [`edition_map`] is non-injective, so
/// the question is asked of the whole set of editions meeting on one target
/// key rather than of one pair at a time. Written that way, the answer does
/// not depend on which key the loop reaches first — which the previous
/// mutate-as-you-go version's did.
fn truth_provenance_clashes(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    map: &BTreeMap<String, String>,
) -> Result<Vec<(String, &'static str)>, String> {
    let mut sources: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for (from_key, into_key) in map {
        sources.entry(into_key).or_default().push(from_key);
    }
    let mut clashing = Vec::new();
    for (into_key, from_keys) in sources {
        let mut claims: BTreeSet<Vec<String>> = BTreeSet::new();
        let mut unclaimed = false;
        let meeting = std::iter::once((into_work, into_key))
            .chain(from_keys.into_iter().map(|from_key| (from_work, from_key)));
        for (work_id, key) in meeting {
            let provenance = truth_provenance(conn, work_id, key)?;
            if provenance.is_empty() {
                unclaimed |= has_truth_pages(conn, work_id, key)?;
            } else {
                claims.insert(provenance);
            }
        }
        if claims.len() > 1 {
            clashing.push((into_key.clone(), "drawn against different archives"));
        } else if claims.len() == 1 && unclaimed {
            clashing.push((
                into_key.clone(),
                "boxes on one side record no archives at all",
            ));
        }
    }
    Ok(clashing)
}

/// Move every reader row off `from_work` onto `into_work` and delete
/// `from_work`.
///
/// Returns [`Outcome::Refused`] — nothing moved, nothing deleted — on five
/// counts, each checked before the first move so that a refusal costs the
/// database nothing. Every one of the five is a `SELECT`, which is what keeps
/// that property true as the list grows:
///
/// 1. **`from_work == into_work`.** Every edition would map to itself, every
///    move would be a no-op, and the `DELETE` would take the work whole. See
///    the first statement of the body for why the guard is here rather than
///    only in `derive`.
/// 2. **An edition of `from_work` with no digest-identical counterpart on
///    `into_work`.** `derive::superseded` guarantees that the *set* of digests
///    is covered, not that the editions line up: a work holding `[X, Y]` as
///    one edition, against a target holding `[X]` and `[Y]` as two, satisfies
///    that rule and is mappable by nothing. Half a fold would be worse than
///    none, and a scan that died of one unfoldable work would be worse than
///    both.
/// 3. **A reader row on an edition key `from_work` does not hold** — see
///    [`stranded`], which is where the reasoning for refusing rather than
///    counting it lives. This is the one arm that is about a *row* rather
///    than an edition, and the guarantee the module makes is per row.
/// 4. **One note id held by one reader on both works** — see
///    [`shared_note_ids`]. `note`'s key is `(user_id, work_id, id)` and
///    migration 5 widened it to that *specifically to permit* this state, so
///    [`move_notes`]' bare `UPDATE` collides on a shape the schema calls
///    supported. Refused rather than re-keyed: the id is one the reader's own
///    sidecar names, and rewriting it silently is worse than declining.
/// 5. **Ground truth meeting on one target edition that cannot be given one
///    provenance** — see [`truth_provenance_clashes`]. Identical `member`
///    digests do not imply identical `truth_member` digests, and moving boxes
///    under the wrong provenance records them as fresh when they are stale.
///    Two *different* lists meeting is one half of it. The other is a side
///    holding boxes and **no** list, which the fold would otherwise hand the
///    other side's — a claim those boxes never had, and the same defect
///    reached from the direction the guard used to be exempt in.
///
/// **Arms 4 and 5 are refusals about a state neither side can be told to fix
/// from here, which is the point.** Both leave the duplicate work visibly
/// standing, which is recoverable and which an operator can act on, where a
/// re-keyed note and a mis-attributed box are neither.
///
/// # Errors
///
/// Returns an error if SQLite fails. A fold it declines to make is not one of
/// those; see [`Outcome`].
pub fn absorb(conn: &Connection, from_work: i64, into_work: i64) -> Result<Outcome, String> {
    // **The invariant lives where the delete is.** Handed one id twice, every
    // edition maps to itself, every move below is a no-op, and the `DELETE`
    // then takes that work with everything on it. `derive` never asks for it
    // — `superseded` skips a candidate its own `written` set holds — but that
    // is a guard in another module, in a function this one never calls, and
    // this module is `pub`. For the only code in the project that deletes a
    // `work` row, "no caller does that" is not the same as "this cannot
    // happen".
    if from_work == into_work {
        return Ok(Outcome::Refused(format!(
            "work {from_work} cannot be folded into itself; nothing was moved and nothing \
             was deleted",
        )));
    }

    // **Opened before the mapping is read, not after it.** Every decision
    // below is taken from rows, and the delete at the end acts on them; a map
    // computed outside the transaction would be a decision about a state the
    // transaction never saw. Rolled back by `Transaction`'s own `Drop` on
    // every early return, which is what makes the refusal arm cost nothing.
    //
    // `crate::Nested`, not `unchecked_transaction` directly, for
    // `preview::preview`'s sake — see that type's own doc. `derive` calls
    // this once per fold `superseded` finds, and a preview wraps a whole
    // `derive_with` pass in a transaction of its own so the whole thing
    // rolls back together; a plain `BEGIN` here would refuse the moment that
    // outer transaction is already open.
    let tx = crate::Nested::open(conn, "fold_absorb")
        .map_err(|e| format!("starting the fold transaction for work {from_work}: {e}"))?;

    let map = edition_map(&tx, from_work, into_work)?;
    let unmapped: Vec<String> = edition_keys(&tx, from_work)?
        .into_iter()
        .filter(|key| !map.contains_key(key))
        .collect();
    if !unmapped.is_empty() {
        // `edition(s)` rather than a singular/plural switch: the switch was a
        // branch no assertion could reach without a fixture built to reach
        // it, which is a test written for a comma.
        return Ok(Outcome::Refused(format!(
            "work {from_work} holds edition(s) `{}` with no digest-identical edition on \
             work {into_work}; nothing was moved and nothing was deleted",
            unmapped.join("`, `"),
        )));
    }

    // **Per row, not per table** — see `stranded`. Every mover below is
    // keyed on a `from_key` out of `map`, so a reader row naming a key that
    // is not in it is moved by nothing, counted by nothing and taken by the
    // `DELETE`. Checked after the unmapped-editions arm above so that a key
    // the work *does* hold is reported as the mapping failure it is, rather
    // than as a stranded row.
    let stranded = stranded(&tx, from_work, &map)?;
    if !stranded.is_empty() {
        let rows: usize = stranded.values().flatten().map(|(_, count)| count).sum();
        let named: Vec<String> = stranded
            .iter()
            .map(|(key, tables)| {
                let tables: Vec<String> = tables
                    .iter()
                    .map(|(table, count)| format!("{count} {table}"))
                    .collect();
                format!("`{key}` ({})", tables.join(", "))
            })
            .collect();
        return Ok(Outcome::Refused(format!(
            "work {from_work} carries {rows} reader row(s) on edition key(s) it does not \
             hold — {}; nothing was moved and nothing was deleted",
            named.join(", "),
        )));
    }

    // **After `stranded`, and that ordering is what makes the read exact.**
    // Every note still on `from_work` here is on a key the map holds, so the
    // whole-work read below is asking about exactly the rows `move_notes` is
    // about to carry across — no per-edition loop, and nothing missed.
    let shared = shared_note_ids(&tx, from_work, into_work)?;
    if !shared.is_empty() {
        // **A count and the first few, not the whole list.** The realistic
        // instance is a reader who imported one `taiyaku.notes.json` against
        // both works — `shared_note_ids`' own rationale names that route —
        // and that shares *every* id in the document: hundreds of ULIDs on
        // one line, which `DeriveReport::unfoldable` then carries into
        // `derive_summary`'s single line of output. A few are enough to go
        // and grep for; the count is what says how large the overlap is.
        const SHOWN: usize = 3;
        let listed: Vec<&str> = shared.iter().take(SHOWN).map(String::as_str).collect();
        let rest = shared.len() - listed.len();
        let and_more = if rest > 0 {
            format!(" and {rest} more")
        } else {
            String::new()
        };
        return Ok(Outcome::Refused(format!(
            "one reader holds {} note id(s) on both work {from_work} and work \
             {into_work} — `{}`{and_more}; moving them would collide on `note`'s \
             (user_id, work_id, id) key, and re-keying an id the reader's own sidecar \
             names is not this fold's to do; nothing was moved and nothing was deleted",
            shared.len(),
            listed.join("`, `"),
        )));
    }

    let clashing = truth_provenance_clashes(&tx, from_work, into_work, &map)?;
    if !clashing.is_empty() {
        // The reason travels with the key because the two arms are two
        // different things to go and look at: one is two labellers
        // disagreeing about the art, the other is boxes with no record of
        // what they were drawn against at all.
        let named: Vec<String> = clashing
            .iter()
            .map(|(key, why)| format!("`{key}` ({why})"))
            .collect();
        return Ok(Outcome::Refused(format!(
            "the ground truth meeting on edition(s) {} of work {into_work} cannot be \
             given one provenance; moving the boxes would record them as drawn against \
             archives they never saw; nothing was moved and nothing was deleted",
            named.join(", "),
        )));
    }

    let mut moved = Moved::default();

    // **The two halves of the position identity, read before the first
    // write.** Other works hold positions too, but no statement in this loop
    // touches any work other than these two; so `offered + held` is what exists
    // now of what this fold is about, and the same read of `into_work`
    // afterwards is what exists then. The difference is every position the fold
    // destroyed on either side, whichever side lost it and whichever edition
    // key sorted first — see `Moved::lost_positions` for why a per-edition
    // `offered - landed` could not say that.
    let offered = positions_on(&tx, from_work)?;
    let held = positions_on(&tx, into_work)?;

    for (from_key, into_key) in &map {
        moved.positions += move_positions(&tx, from_work, into_work, from_key, into_key)?;
        moved.notes += move_notes(&tx, from_work, into_work, from_key, into_key)?;
        let (pages, collisions) = move_truth(&tx, from_work, into_work, from_key, into_key)?;
        moved.truth_pages += pages;
        moved.collisions += collisions;
    }

    // `saturating_sub` rather than a bare `-`, and it can only ever be a
    // no-op: nothing in the loop above inserts a row that neither side
    // offered, so `after <= offered + held` holds. It is here so that a later
    // statement breaking that invariant reports `0` rather than panicking
    // inside the one function that deletes a work.
    let after = positions_on(&tx, into_work)?;
    moved.lost_positions = (offered + held).saturating_sub(after);

    // Counted while the rows are still there, which is the only moment they
    // can be counted at all.
    moved.discarded = discarded(&tx, from_work)?;

    // **Last, and only after every move above.** `progress`, `note`,
    // `truth_page`, `truth_box` and `truth_member` all reference `work(id)
    // ON DELETE CASCADE`; a delete before the moves takes every one of them,
    // and the moves are not recoverable from anything else — a reading
    // position and a note have no file on disk behind them for a scanned
    // work. This statement being the last one in the transaction is the whole
    // of the safety argument.
    tx.execute("DELETE FROM work WHERE id = ?1", [from_work])
        .map_err(|e| format!("deleting the superseded work {from_work}: {e}"))?;
    tx.commit()
        .map_err(|e| format!("committing the fold of work {from_work}: {e}"))?;
    Ok(Outcome::Folded(moved))
}

/// A reader's place, carried across with its history.
///
/// `completed_at` and `read_count` come too: they are facts about reading
/// this rendition, and the rendition is what moved. 001's own comment calls
/// them "behavioural data with NOTHING to backfill", so leaving them behind
/// would be the fold quietly resetting a finished re-read.
///
/// **The conflict is reachable, by two routes this module documents itself.**
/// An earlier comment here called it "impossible by construction — the
/// target's edition was derived by the scan that caused this fold, so no
/// reader can hold a position in it yet", and that is wrong twice over:
/// [`edition_map`] is explicitly non-injective, so two editions of `from_work`
/// over identical digests both land on the one edition of `into_work` inside a
/// single fold and the second `INSERT` meets the first; and `progress.edition`
/// has no foreign key to `edition(key)` — the fact [`stranded`] rests on — so
/// `into_work` can already carry a grandfathered row under the mapped key.
///
/// **Counted rather than refused, and the grandfathered row is why that is
/// not a choice.** A refusal would need to tell the two apart: a target row
/// that is a genuine reading of the mapped edition, where "newer wins" is the
/// right answer, and one grandfathered under a key whose edition this derive
/// has only just created, where the surviving slot means nothing. Nothing in
/// the schema separates them. [`stranded`]'s test cannot be turned on the
/// target either: `into_key` is drawn from `into_work`'s own `edition` table
/// by [`edition_map`], so a target row under the mapped key is never a row on
/// a key the work does not hold — the C1 shape cannot arise there, and the
/// blanket version of it would refuse every fold into a target carrying any
/// grandfathered row, a state `taiyaku-serve`'s `notes` module *deliberately
/// supports*. That would be a narrowing that defeats the fold while looking
/// like safety. So the loss is made visible instead, which is what the module
/// asks of every derived thing it declines to move.
///
/// **So the loss is counted, and not here.** `execute` returns rows actually
/// written, so this function can report how many times it wrote and nothing
/// more: the `WHERE excluded.updated > progress.updated` makes the source's
/// row a silent no-op when the target's is newer, and the `DO UPDATE` makes
/// the target's row a silent casualty when the source's is. Neither is
/// visible in a per-edition subtraction — two source editions landing on one
/// target row report `changes = 1` each, `offered - landed = 0` each, with a
/// position gone. [`absorb`] therefore counts positions as a whole-fold
/// identity across every call to this function; see [`Moved::lost_positions`]
/// for the arithmetic and for the replay that showed the subtraction missing
/// the case. "Newer wins" itself stays: a slot number is not mergeable, and
/// the later reading is the one somebody is in the middle of.
///
/// **On a conflict the newer row wins entire, history included**, which is
/// why the `DO UPDATE` sets all four columns rather than the two that make up
/// the place. Setting `slot` and `updated` alone produced a row belonging to
/// neither reading — the source's place under the target's re-read count —
/// and "the newer position wins" then quietly meant "and its history does
/// not". The same argument that put the guard here covers this: discovering
/// the conflict was reachable by losing a re-read count is as expensive as
/// discovering it by losing a position.
fn move_positions(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    from_key: &str,
    into_key: &str,
) -> Result<usize, String> {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at, read_count)
         SELECT user_id, ?2, ?4, slot, updated, completed_at, read_count
           FROM progress WHERE work_id = ?1 AND edition = ?3
         ON CONFLICT(user_id, work_id, edition) DO UPDATE SET
           slot = excluded.slot,
           updated = excluded.updated,
           completed_at = excluded.completed_at,
           read_count = excluded.read_count
         WHERE excluded.updated > progress.updated",
        rusqlite::params![from_work, into_work, from_key, into_key],
    )
    .map_err(|e| format!("moving positions off work {from_work}: {e}"))
}

/// How many reading positions one work carries, over every edition key.
///
/// **Over the whole work rather than one edition**, which is what makes
/// [`Moved::lost_positions`] an identity rather than a sum of per-edition
/// guesses: a row on a key no `into_key` names is untouched by every
/// statement in the move loop, so it appears in the before and after readings
/// both and cancels itself out.
fn positions_on(conn: &Connection, work_id: i64) -> Result<usize, String> {
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM progress WHERE work_id = ?1",
            [work_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("counting the reading positions on work {work_id}: {e}"))?;
    usize::try_from(count).map_err(|e| format!("counting the reading positions: {e}"))
}

/// Notes, with the edition string rewritten.
///
/// **The rewrite is the whole of this function's risk.** An `UPDATE` that
/// moved `work_id` alone would be accepted by every constraint in the schema
/// — landing a note about a colour page on the monochrome archive at the same
/// slot number, with nothing saying so. Measured 2026-08-27;
/// `truth_member`'s own schema comment calls this the sharpest form of the
/// problem in the project. `fold::tests`'s
/// `a_moved_note_names_the_edition_that_holds_its_archive` is the only thing
/// that catches it.
///
/// **The id being a ULID does not mean nothing collides**, which an earlier
/// version of this comment asserted as fact. `note`'s key is
/// `(user_id, work_id, id)` and migration 5 widened it to that *deliberately
/// to permit* one reader holding one id on two works — its rationale names
/// the routes, "copying a `taiyaku.notes.json` between volumes, or importing
/// one sidecar against two works", and a fold pair is by construction two
/// works over the same volume. The `UPDATE` below then fails outright, so
/// [`absorb`] reads for that state first and refuses; see
/// [`shared_note_ids`].
fn move_notes(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    from_key: &str,
    into_key: &str,
) -> Result<usize, String> {
    conn.execute(
        "UPDATE note SET work_id = ?2, edition = ?4
          WHERE work_id = ?1 AND edition = ?3",
        rusqlite::params![from_work, into_work, from_key, into_key],
    )
    .map_err(|e| format!("moving notes off work {from_work}: {e}"))
}

/// Ground truth: pages, then boxes, then the digests they were drawn against.
///
/// **Delete-and-reinsert, not `UPDATE`, and page rows first.** `truth_box`'s
/// foreign key names `(work_id, edition, page)` with no `ON UPDATE`, so
/// `UPDATE truth_page SET work_id = …` is refused outright — measured, both
/// with and without the edition rewrite, and pinned by
/// `truth_page_refuses_to_be_moved_by_update`. The insertion order is
/// `state::put_truth`'s own, which that composite key exists to enforce. The
/// "delete" half is [`absorb`]'s final `DELETE FROM work`: nothing is removed
/// here, so a failure below leaves both copies rather than neither.
///
/// **A page the target already holds stays the target's, and the source's
/// boxes are not added underneath it.** Truth is library-wide with no
/// `user_id`, so "the target already holds this page" means somebody has
/// labelled it, and merging two labellers' boxes onto one page is a different
/// decision from "the target's labelling wins" — not one this fold gets to
/// make. Which pages those are is settled *before* the first insert and kept
/// in hand: a `NOT EXISTS` against `truth_page` would be re-evaluated after
/// the page rows had been written and would then exclude nothing at all.
///
/// `truth_member` is `INSERT OR IGNORE` for the same reason: the target's
/// record of which archives its boxes were drawn against is not overwritten.
///
/// **That `OR IGNORE` is safe only because [`absorb`] has already refused the
/// case where it would matter.** An earlier version of this comment justified
/// it by "since the mapping is by identical digest list, a target that holds
/// one already holds the same digests in the same order", and that inference
/// is invalid: [`edition_map`] compares `member.sha256`, the archives as they
/// are now, while `truth_member` is a client-supplied snapshot of what the
/// boxes were *drawn against*, which `state::put_truth` writes verbatim. The
/// two diverging is the whole reason `TruthEdition::is_stale_for` exists. So
/// the divergence is checked for by name and refused before any write; see
/// [`truth_provenance_clashes`]. What reaches this statement is a target that
/// holds the same list or none at all — and, since that guard's second arm,
/// never boxes on either side carrying no list of their own while one is
/// about to come to stand over them. `OR IGNORE` is then exactly right.
///
/// Returns the pages moved and the pages the target already held.
fn move_truth(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    from_key: &str,
    into_key: &str,
) -> Result<(usize, usize), String> {
    let offered = pages_of(conn, from_work, from_key)?;
    let already = pages_of(conn, into_work, into_key)?;

    let mut page_stmt = conn
        .prepare("INSERT INTO truth_page (work_id, edition, page) VALUES (?1, ?2, ?3)")
        .map_err(|e| format!("preparing the truth-page move: {e}"))?;
    // `ORDER BY id`, because rowid order is document order for a page's boxes
    // — `state::truth` reads them back the same way, and its own doc comment
    // is why that is stated rather than inherited.
    let mut box_stmt = conn
        .prepare(
            "INSERT INTO truth_box (work_id, edition, page, x, y, w, h, axis, kind, ignore, text)
             SELECT ?1, ?2, ?3, x, y, w, h, axis, kind, ignore, text
               FROM truth_box WHERE work_id = ?4 AND edition = ?5 AND page = ?3
              ORDER BY id",
        )
        .map_err(|e| format!("preparing the truth-box move: {e}"))?;

    let mut moved = 0;
    let mut collisions = 0;
    for page in offered {
        if already.contains(&page) {
            collisions += 1;
            continue;
        }
        page_stmt
            .execute(rusqlite::params![into_work, into_key, page])
            .map_err(|e| format!("moving truth page {page} off work {from_work}: {e}"))?;
        box_stmt
            .execute(rusqlite::params![
                into_work, into_key, page, from_work, from_key
            ])
            .map_err(|e| format!("moving the boxes on truth page {page}: {e}"))?;
        moved += 1;
    }
    conn.execute(
        "INSERT OR IGNORE INTO truth_member (work_id, edition, ordinal, sha256)
         SELECT ?2, ?4, ordinal, sha256
           FROM truth_member WHERE work_id = ?1 AND edition = ?3",
        rusqlite::params![from_work, into_work, from_key, into_key],
    )
    .map_err(|e| format!("moving truth digests off work {from_work}: {e}"))?;

    Ok((moved, collisions))
}

/// Every page somebody has looked at on one work's edition, in page order.
fn pages_of(conn: &Connection, work_id: i64, edition: &str) -> Result<BTreeSet<i64>, String> {
    conn.prepare("SELECT page FROM truth_page WHERE work_id = ?1 AND edition = ?2")
        .and_then(|mut statement| {
            statement
                .query_map(rusqlite::params![work_id, edition], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<rusqlite::Result<BTreeSet<i64>>>()
        })
        .map_err(|e| format!("reading truth pages of work {work_id}, edition `{edition}`: {e}"))
}

/// How many rows the final `DELETE` will take that this fold has not moved.
///
/// See [`DISCARDED`] for what is in that list and why each entry is counted
/// rather than migrated. The predicates are compile-time constants from that
/// table, not anything a caller supplies.
fn discarded(conn: &Connection, from_work: i64) -> Result<usize, String> {
    let mut total: i64 = 0;
    for (table, predicate) in DISCARDED {
        total += conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE {predicate}"),
                [from_work],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| format!("counting what the fold discards from `{table}`: {e}"))?;
    }
    usize::try_from(total).map_err(|e| format!("counting what the fold discards: {e}"))
}

#[cfg(test)]
mod tests;
