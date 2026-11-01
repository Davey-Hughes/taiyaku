//! Absorbing a superseded work into the work its material moved to.
//!
//! **It deletes a `work` row**, which is why it is a module rather than a
//! function inside `derive`. The safety argument is an ordering argument and it
//! should be readable in one place: the reader rows move first, in one
//! transaction, and the delete is the last statement. Reversed, `work`'s
//! `ON DELETE CASCADE` takes a reader's position, their notes and the library's
//! ground truth with it.
//!
//! **Eleven foreign keys name `work(id)` and every one of them cascades** —
//! `alignment`, `collection_item`, `credit`, `edition`, `migrated_document`,
//! `note`, `progress`, `truth_box`, `truth_member`, `truth_page` and
//! `work_tag`. Four more follow transitively: `member` and `page_flag` through
//! `edition`, `run` through `alignment`, and `work_fts` via the `work_ad AFTER
//! DELETE` trigger in `001_initial.sql`. Three of those tables hold a reader's
//! own state and are moved by [`absorb`], and so is `alignment` (O76), with
//! `run` beneath it, wherever it can be placed exactly — [`plan_alignments`]
//! says which it leaves, and which of those it counts as
//! [`Moved::uncarried_alignments`]; the rest are counted as
//! [`Moved::discarded`], because a delete this wide must not be able to happen
//! quietly.
//!
//! **A slot moves as the page and half it names, never as its number** — O121.
//! A slot is an index into the slots an edition's spread flags and its work's
//! reading direction build, and a twin a scan has just built holds no spread
//! flags, so past a spread the same number names a different page there.
//! [`plan_slots`] decides where every slot-addressed row stands before
//! anything is written.
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
//! five questions of its own — which edition of the target holds each edition
//! of the superseded work's archives; whether anything the reader owns is on
//! a key neither side can name; whether every row addressed by slot — the
//! folded work's and the surviving work's own alike — can name the same page
//! and half afterwards, and whether the flags it would write would move a span
//! of an alignment the surviving work holds; whether one reader holds one note id on both
//! works with two copies that differ; and whether the ground truth meeting on
//! a mapped edition was drawn against the same archives on the two sides —
//! and refuses the whole fold when the answer to any of them is wrong. Two
//! copies of one note id that agree in every column are not a wrong answer:
//! they are one note, and the superseded work's copy is dropped (O78).
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
use taiyaku_core::{Alignment, Sidecar, Slot, SlotSpan, Source};

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
    /// Rows this fold moved off `from_work` onto `into_work`'s `note`.
    ///
    /// **Rows moved, and nothing else.** A source copy [`absorb`] dropped
    /// because the target already held the identical note is not in here;
    /// it is [`Self::dropped_notes`].
    pub notes: usize,
    /// Source notes this fold deleted because the target already held the
    /// same note under the same id, for the same reader.
    ///
    /// **"The same" is every column** but the two a fold rewrites: `body`,
    /// `slot`, the four region columns, `color`, `created` and `updated` equal,
    /// and the target's `edition` exactly the key the source's edition maps
    /// to. See [`shared_note_ids`], which is the only thing that decides it.
    /// Two rows that agree on all of that are one fact written twice, and the
    /// target's row is what the reader keeps. Counted apart from
    /// [`Self::notes`] because a reader who counts their notes after the
    /// fold finds these copies gone, not moved.
    pub dropped_notes: usize,
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
    /// **A whole-fold identity, not a per-edition subtraction.** `offered -
    /// landed` per edition, taken off SQLite's `changes`, could only see a
    /// losing row that was *skipped*. [`edition_map`] is explicitly
    /// non-injective, so two editions of `from_work` can upsert the same
    /// `(user_id, into_key)` row: in `BTreeMap` key order, when the
    /// later-sorting key carries the newer reading the first `execute` inserts
    /// and the second *updates*, both report `changes = 1`, both count as
    /// landed, one row survives and one reading position is destroyed and
    /// reported as moved.
    ///
    /// The number is taken over the fold as a whole: `offered + held - after`,
    /// where `offered` is the `progress` rows on `from_work` read before the
    /// move loop, `held` the rows on `into_work` read before it, and `after`
    /// the rows on `into_work` read when the loop is done. Every remaining
    /// source row is destroyed by the final `DELETE`, so what exists after the
    /// fold is exactly `after`, and the difference is every position that
    /// stopped existing. It cannot miss a case, and it does not depend on which
    /// edition key sorts first or on which side won.
    ///
    /// **It covers a pre-existing target row overwritten by an incoming one.**
    /// Where the source's row is the newer, the `DO UPDATE` replaces the
    /// target's slot, `completed_at` and `read_count` entire; that reading is
    /// as gone as a declined one, and counting the write in [`Self::positions`]
    /// says nothing about it.
    ///
    /// **Rows on unmapped keys cancel rather than distort.** [`stranded`] has
    /// already refused any `from_work` reader row on a key the map does not
    /// hold, so `offered` is exactly what the loop is about to carry; and a
    /// grandfathered `into_work` row on a key no `into_key` names is untouched
    /// by every statement below, so it is in `held` and in `after` both.
    ///
    /// "The newer position wins": a slot number is not mergeable and the later
    /// reading is the one somebody is in the middle of. What this number exists
    /// for is that the loss is never silent.
    pub lost_positions: usize,
    /// Rows the final `DELETE` took that this fold did not move — see
    /// [`DISCARDED`], which enumerates them.
    pub discarded: usize,
    /// Alignments of `from_work` this fold did not carry onto `into_work`
    /// and that nothing on `into_work` stands in for — see
    /// [`plan_alignments`] for the four ways that happens.
    ///
    /// **Not every alignment left behind.** One whose pair the target already
    /// aligns, and which holds no hand-corrected run, is not counted: the
    /// target's own edge answers the same question. Nor is a solver's edge
    /// between two editions the fold makes one. What is counted is an edge
    /// the surviving work no longer has an answer for, or a person's
    /// correction it did not keep.
    pub uncarried_alignments: usize,
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
/// **`alignment`, `run` and a `spread` page flag are not in it (O76).** They
/// are carried — see [`plan_slots`] — because the fold is between editions
/// holding the same archives, which is the one case where their meaning can be
/// kept exactly. What cannot be kept exactly is refused or counted there, never
/// here: an alignment left behind is [`Moved::uncarried_alignments`], and a
/// spread flag is either written onto the twin that held none, identical to the
/// twin's own, or a different measurement of the same bytes, where the twin's
/// own stands and every row that depended on the source's has already been
/// placed exactly, refused over or counted.
///
/// **Counted rather than moved, and that is a decision.** Each of these is the
/// library's *description* of the material rather than a reader's state, and
/// each would need its own merge rule against what the target already holds:
/// `credit` has no unique constraint, so a move duplicates a scanlation group
/// rather than reconciling it; `collection_item`'s `(collection_id, ordinal)`
/// key collides whenever the target sits in the same reading list;
/// `edition.label`, `language` and `kind` would overwrite what this very derive
/// just wrote onto the target's edition, and the next derive would write it
/// back — a flip-flop, not a migration. Choosing merge rules with no evidence,
/// inside a function that deletes a work, is more risk than the loss it
/// avoids, and every one of these is visible on screen and re-doable by hand
/// where a reading position is not.
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
const DISCARDED: [(&str, &str); 6] = [
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
    ("work_tag", "work_id = ?1"),
    ("collection_item", "work_id = ?1"),
    // A credit flag only: a spread flag is a measurement of the bytes, and
    // [`plan_slots`] carries it — see this list's own doc.
    (
        "page_flag",
        "kind = 'credit' AND edition_id IN (SELECT id FROM edition WHERE work_id = ?1)",
    ),
    // A chapter number the scanner derived is re-derived onto the target by
    // `derive::write_chapters` from the same filename; one a person entered is
    // one no re-derive could recover. Only the second is a loss, so only the
    // second is counted — matched by `chapter_source` rather than by the number
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
/// **By member digest, not by the rule that caused the fold.** The two editions
/// are the same bytes, so page indices and box coordinates transfer one for one
/// with nothing to re-derive. *Slots do not: a slot is an index into the slots
/// an edition's spread flags and its work's reading direction build, a scan
/// writes no spread flag, so a slot past a spread names a different page on a
/// twin holding none. [`plan_slots`] is what places a slot.* Reading
/// `set_edition_key` off the rule instead would tie this to *why* the material
/// moved and would guess where no rule exists; digests hold for any later
/// feature that moves material between works.
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
/// **None of these four columns has a foreign key to `edition(key)`**, so
/// SQLite will not stop a row naming a key its work does not hold, and nothing
/// else does either.
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
/// so.** [`move_positions`], [`move_notes`] and [`move_truth`] are all keyed on
/// a `from_key` drawn from [`edition_map`], which is built from the `edition`
/// *table*. A row on a key that table does not hold is invisible to all three,
/// absent from [`DISCARDED`], and taken by [`absorb`]'s final `DELETE`.
/// Pinned by
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
/// **Per table, not just per key.** The count alone is ambiguous by
/// construction: four rows spread one per table reads `4`, and so does any
/// other four. Naming the table beside the number costs one `Vec` and is what
/// an operator needs anyway, the four tables being four different things to go
/// and look at.
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

/// Everything one `note` row says apart from whose it is and where it is kept
/// — the columns two copies under one id must agree on to be one note.
///
/// **Every column of `note` but four.** `user_id` and `id` are what
/// [`shared_note_ids`] joins the two copies on, so they are equal by
/// construction; `work_id` and `edition` are the two [`move_notes`] rewrites,
/// and `edition` is compared through the map instead. Nothing else is left out:
/// `color` is a reader's choice, and `created` and `updated` are the client's
/// own timestamps, which `state::merge_notes` reads to decide between two
/// copies — so two rows differing only there are not one fact.
///
/// **Compared here, in Rust, and not in SQL, for NULL's sake.** Five of these
/// columns are nullable, and all five are NULL on the ordinary note — the
/// whole slot, the default colour. Rust's `Option` equality answers
/// `None == None` as true, which is what two whole-slot notes are. SQL's `=`
/// answers `NULL = NULL` as NULL, which is not true: a comparison written that
/// way calls every whole-slot pair different and refuses exactly the fold this
/// type exists to allow. `IS` would be right in SQL; this avoids needing it.
///
/// A new column on `note` is not covered until it is added here.
#[derive(Debug, PartialEq)]
struct NoteContent {
    slot: i64,
    rx: Option<f64>,
    ry: Option<f64>,
    rw: Option<f64>,
    rh: Option<f64>,
    color: Option<String>,
    body: String,
    created: String,
    updated: String,
}

/// [`NoteContent`] out of nine consecutive columns of `row`, starting at `at`,
/// in the order the struct declares them.
fn note_content(row: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<NoteContent> {
    Ok(NoteContent {
        slot: row.get(at)?,
        rx: row.get(at + 1)?,
        ry: row.get(at + 2)?,
        rw: row.get(at + 3)?,
        rh: row.get(at + 4)?,
        color: row.get(at + 5)?,
        body: row.get(at + 6)?,
        created: row.get(at + 7)?,
        updated: row.get(at + 8)?,
    })
}

/// Note ids one reader holds on both works, split by whether the two copies
/// are one note. See [`shared_note_ids`].
#[derive(Debug, Default)]
struct SharedNotes {
    /// `(user_id, id)` of each copy on `from_work` that the target already
    /// holds identically, in id order: the rows [`drop_identical_notes`]
    /// deletes once every refusal has been passed.
    identical: Vec<(i64, String)>,
    /// Ids whose two copies differ for at least one reader, in id order and
    /// each once — the ids [`absorb`] refuses the whole fold over.
    conflicting: BTreeSet<String>,
}

/// Note ids one reader holds on **both** works, in id order, each marked as
/// identical on the two works or not.
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
/// note.work_id, note.id`, which travels out of [`absorb`] as an `Err`, through
/// `derive`'s `?`, and kills the whole derive: every later pair goes unfolded
/// and `series_holding_no_work` and `proposals::raise` never run. Nothing is
/// destroyed, the transaction rolling back, so this is fold-defeating rather
/// than data-losing — but [`Outcome::Refused`] exists for precisely this and
/// catching the error after the write has begun would give up the "a refusal
/// costs the database nothing" property as well.
///
/// **Identical is every column: O78.** The rows the route above actually
/// writes, one sidecar imported against both works, agree in everything. Two
/// copies are one note when their [`NoteContent`] is equal and the target's
/// `edition` is exactly the key `map` sends the source's edition to.
/// [`edition_map`] pairs editions holding the same digests in the same order,
/// so a region carries across unchanged, and the edition key and the slot are
/// the parts of the anchor that move — the slot by the page and half it names,
/// below. The raw keys are the wrong thing to compare — the source's `raw` and
/// the target's `raw` can be two different books, and the source's `raw` and
/// the target's `full-color` the same one. Anything short of that — a body, a
/// colour, a timestamp, a coordinate, a key — is two notes, and choosing
/// between them is not this fold's to do.
///
/// **The slot is compared where each copy will stand, not as stored.**
/// [`plan_slots`] rewrites a slot whose edition's slot sequence the fold
/// changes, on either work, so two copies stored under different numbers can
/// name one page and two stored under one number can name two. `plan` answers
/// both; a slot it cannot place has already refused the fold, so the `None` arm
/// here is a copy that is not identical.
///
/// **Per reader, and `user_id` is in both the join and the result.** Two
/// readers holding one id is migration 5's first case and is not a collision
/// at all, so a copy is only ever compared with the same reader's copy, and
/// the drop names the reader whose copy it is.
///
/// The whole work rather than the mapped edition, and that is exact rather
/// than approximate: [`stranded`] has already refused any note on a key the
/// map does not hold, so every note still on `from_work` at this point is one
/// [`move_notes`] is about to carry across.
fn shared_note_ids(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    map: &BTreeMap<String, String>,
    plan: &SlotPlan,
) -> Result<SharedNotes, String> {
    let reading = || format!("reading the note ids work {from_work} and work {into_work} share");
    let mut statement = conn
        .prepare(
            "SELECT s.user_id, s.id, s.edition, t.edition,
                    s.slot, s.rx, s.ry, s.rw, s.rh, s.color, s.body, s.created, s.updated,
                    t.slot, t.rx, t.ry, t.rw, t.rh, t.color, t.body, t.created, t.updated
               FROM note s
               JOIN note t ON t.user_id = s.user_id AND t.id = s.id AND t.work_id = ?2
              WHERE s.work_id = ?1
              ORDER BY s.id, s.user_id",
        )
        .map_err(|e| format!("{}: {e}", reading()))?;
    let rows = statement
        .query_map(rusqlite::params![from_work, into_work], |row| {
            let from_key: String = row.get(2)?;
            let into_key: String = row.get(3)?;
            let mut source = note_content(row, 4)?;
            let mut target = note_content(row, 13)?;
            let placed = plan
                .source_slot(&from_key, source.slot)
                .zip(plan.target_slot(&into_key, target.slot));
            let identical = map.get(&from_key) == Some(&into_key)
                && placed.is_some_and(|(from_slot, into_slot)| {
                    source.slot = from_slot;
                    target.slot = into_slot;
                    source == target
                });
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, identical))
        })
        .map_err(|e| format!("{}: {e}", reading()))?;
    let mut shared = SharedNotes::default();
    for row in rows {
        let (user_id, id, identical) = row.map_err(|e| format!("{}: {e}", reading()))?;
        if identical {
            shared.identical.push((user_id, id));
        } else {
            shared.conflicting.insert(id);
        }
    }
    Ok(shared)
}

/// Delete each source copy [`shared_note_ids`] found identical to the
/// target's, returning how many rows went.
///
/// **The source's copy, never the target's.** The two agree in every column
/// a reader can see, so the table after either delete reads the same — but
/// the target's row is the one this reader's notes on the surviving work
/// already are, and the source's is about to be moved onto it or deleted with
/// its work. Keyed by `user_id` as well as `id`, so another reader's note
/// under the same id is never reached.
///
/// **Called only after every refusal in [`absorb`]**, which is what keeps
/// every refusal a `SELECT`: a fold that dropped the identical copies and then
/// refused over a conflicting one would depend on the rollback to undo it.
fn drop_identical_notes(
    conn: &Connection,
    from_work: i64,
    identical: &[(i64, String)],
) -> Result<usize, String> {
    let mut statement = conn
        .prepare("DELETE FROM note WHERE user_id = ?1 AND work_id = ?2 AND id = ?3")
        .map_err(|e| format!("preparing the identical-note drop: {e}"))?;
    let mut dropped = 0;
    for (user_id, id) in identical {
        dropped += statement
            .execute(rusqlite::params![user_id, from_work, id])
            .map_err(|e| {
                format!("dropping the identical copy of note `{id}` off work {from_work}: {e}")
            })?;
    }
    Ok(dropped)
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
/// first — which is the whole state [`truth_provenance_clashes`]'s second arm
/// is for. "No provenance" and "no boxes" are different facts and only this
/// tells them apart.
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
/// **`truth_member` is not derivable from the mapping.** [`edition_map`]
/// compares `member.sha256` — the archives as they are *now*. `truth_member` is
/// a client-supplied snapshot of what the boxes were *drawn against*, written
/// verbatim by `state::put_truth` with no validation of any kind. The two
/// diverging is not an edge case; it is the entire reason
/// `TruthEdition::is_stale_for` exists. So "the mapping is by identical digest
/// list, therefore a target that holds a `truth_member` row already holds the
/// same digests in the same order" is invalid: relying on it, the `INSERT OR
/// IGNORE` would put the source's pages under the target's digests — boxes
/// drawn on superseded art recorded as fresh, which `score` would then accept.
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
/// and it is refused.** A source carrying `truth_page`/`truth_box` rows and no
/// `truth_member` row would otherwise pass, its boxes copied onto the target
/// key, with the target's own non-empty list standing over them through
/// [`move_truth`]'s `INSERT OR IGNORE`. `TruthEdition::is_stale_for` returns
/// `true` when `members` is empty and `digests` is not — so `score` correctly
/// refuses those boxes as stale *before* the fold and would read the very same
/// boxes as fresh *after* it, which is "boxes drawn on superseded art recorded
/// as fresh" reached from the other direction. The state is reachable:
/// `state::put_truth` writes the document's `members` as given and
/// `routes::put_truth` validates neither the edition key nor the member list,
/// so any `TruthDoc` with `"members": []` and populated `pages` produces it.
///
/// The test is therefore over the boxes rather than over the lists: what
/// lands on a target key must not be *one* claim over boxes that had none.
/// Zero claims is fine — nothing is asserted about anything — and two claims
/// is the clash above.
///
/// **Both sides, and every source at once.** The unclaimed boxes can be the
/// target's as easily as a source's, and [`edition_map`] is non-injective, so
/// the question is asked of the whole set of editions meeting on one target key
/// rather than of one pair at a time. Written that way, the answer does not
/// depend on which key the loop reaches first.
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

/// One edition whose slot sequence a fold changes: its slots as they stood,
/// and as they will stand on the surviving work.
///
/// **A slot is an index, and only means a page inside one sequence.**
/// `Edition::slots` builds the sequence from the edition's page count, its
/// spread flags and its work's reading direction, so the same number names a
/// different page wherever any of the three differs. That is O121, and this
/// type is the whole of its fix: a slot is carried as the page and half it
/// named, never as the number.
#[derive(Debug)]
struct Resequenced {
    before: Vec<Slot>,
    after: Vec<Slot>,
}

impl Resequenced {
    /// Where the page and half that `slot` named stands afterwards: `None`
    /// where that page and half does not exist there — a whole page the fold
    /// makes a spread, or a half of a page it does not — and `slot` itself
    /// where it named no page before the fold.
    ///
    /// **A slot that names no page is carried as it is, not refused.** Past the
    /// end of its edition, it names no page on either work, so carrying it gets
    /// no page wrong; and `taiyaku-serve`'s `notes::validate` keeps a note
    /// stored that way on purpose, so a re-solve that shortened an edition
    /// leaves it readable and deletable.
    ///
    /// **By the page and the half, not by the position in reading order.** A
    /// half is a physical side of the scan, so the right half of page 4 is the
    /// right half of page 4 whichever direction either work reads in, and a
    /// note's region — fractions of its slot — still lies on the same artwork.
    fn place(&self, slot: i64) -> Option<i64> {
        let Some(named) = usize::try_from(slot)
            .ok()
            .and_then(|at| self.before.get(at))
        else {
            return Some(slot);
        };
        let at = self.after.iter().position(|standing| standing == named)?;
        i64::try_from(at).ok()
    }
}

/// Everything one fold does to what is addressed by slot, decided before any
/// write so that every refusal it leads to is still a `SELECT`. See
/// [`plan_slots`].
#[derive(Debug, Default)]
struct SlotPlan {
    /// Spread pages to write onto each edition of `into_work` that holds none,
    /// by key.
    spreads: BTreeMap<String, Vec<u32>>,
    /// Editions of `from_work`, by key, whose slots stand differently on their
    /// twin — every reader row on one of them is rewritten before it moves.
    source: BTreeMap<String, Resequenced>,
    /// Editions of `into_work`, by key, whose own slots [`Self::spreads`]
    /// changes — the rows already there were written under the old sequence.
    target: BTreeMap<String, Resequenced>,
    /// Alignments of `from_work` to carry, with the keys each will name.
    carried: Vec<(i64, String, String)>,
    /// Alignments of `into_work` an incoming one replaces.
    replaced: Vec<i64>,
    /// [`Moved::uncarried_alignments`].
    uncarried: usize,
    /// Alignments of `into_work`, counted under a key [`Self::target`]
    /// resequences, whose spans would name other slots once it has.
    broken: BTreeMap<String, usize>,
}

impl SlotPlan {
    /// Where a reader row on `from_work`'s edition `key` will stand.
    fn source_slot(&self, key: &str, slot: i64) -> Option<i64> {
        self.source
            .get(key)
            .map_or(Some(slot), |moved| moved.place(slot))
    }

    /// Where a row already on `into_work`'s edition `key` will stand.
    fn target_slot(&self, key: &str, slot: i64) -> Option<i64> {
        self.target
            .get(key)
            .map_or(Some(slot), |moved| moved.place(slot))
    }
}

/// Decide what a fold does to spread flags, to every reader row addressed by
/// slot, and to alignments — O76 and O121, one mechanism.
///
/// **The claim is that every slot-addressed row the fold carries names the
/// same page and half afterwards that it named on the folded work**, and the
/// three kinds of row are held to it the same way.
///
/// **Spread flags first, because every slot is built from them.** A scan
/// writes no spread flag (`derive` builds every edition with none) and
/// `taiyaku align` measures them from the images, so the ordinary fold is a
/// work imported with its flags folding onto a twin a scan has just built
/// without any. The two hold the same archives in the same order, and a flag
/// is a measurement of the bytes, so where the twin holds none it takes the
/// folded edition's. Where it already holds a set it keeps it — a different
/// set is a different measurement of the same pages, and choosing between two
/// is not this fold's. Where two folded editions meeting on one twin both hold
/// flags and the sets differ, it takes neither. **A folded edition holding no
/// flags does not disagree**: a scan never writes one, so none means
/// unmeasured rather than measured to have none, and it does not stop the
/// twin taking the measured set — which is what lets a correction on the
/// measured edition carry. A row on the unmeasured edition that the taken
/// flags would put on another page still refuses, below.
///
/// **Then every sequence is compared, before and after.** A folded edition
/// whose slots stand differently on its twin, and a twin whose own slots the
/// new flags change, each get a [`Resequenced`], and [`absorb`] refuses the
/// whole fold over any `progress` or `note` row one of them cannot place —
/// reader state, for the reason [`stranded`] gives. A row whose slot named no
/// page before the fold is carried as it is; see [`Resequenced::place`].
/// **Refused rather than counted**, because the alternative is a place or a note on another page
/// with a number saying so, and a standing duplicate is recoverable where that
/// is not.
///
/// **Alignments are carried where their spans still cover the same slots**,
/// and counted where they do not — see [`plan_alignments`].
fn plan_slots(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    map: &BTreeMap<String, String>,
) -> Result<SlotPlan, String> {
    // Through the projection, so the sequences compared here are the ones the
    // reader draws: `Edition::slots` over the same rows, not a second
    // reconstruction of spreads and halves that could drift from it.
    let source = crate::project::sidecar(conn, from_work)?;
    let target = crate::project::sidecar(conn, into_work)?;
    let mut plan = SlotPlan::default();

    let mut meeting: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (from_key, into_key) in map {
        meeting.entry(into_key).or_default().push(from_key);
    }
    let mut standing: BTreeMap<String, Vec<Slot>> = BTreeMap::new();
    for (into_key, from_keys) in meeting {
        let Some(twin) = target.editions.get(into_key) else {
            continue;
        };
        let mut twin_after = twin.clone();
        if twin.spreads.is_empty() {
            let offered: Vec<&Vec<u32>> = from_keys
                .iter()
                .filter_map(|key| source.editions.get(*key))
                .map(|edition| &edition.spreads)
                .filter(|spreads| !spreads.is_empty())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if let [only] = offered.as_slice() {
                twin_after.spreads.clone_from(only);
                plan.spreads.insert(into_key.to_string(), (*only).clone());
            }
        }
        let before = twin.slots(target.work.reading);
        let after = twin_after.slots(target.work.reading);
        if after != before {
            plan.target.insert(
                into_key.to_string(),
                Resequenced {
                    before,
                    after: after.clone(),
                },
            );
        }
        standing.insert(into_key.to_string(), after);
    }
    for (from_key, into_key) in map {
        let (Some(edition), Some(after)) = (source.editions.get(from_key), standing.get(into_key))
        else {
            continue;
        };
        let before = edition.slots(source.work.reading);
        if before != *after {
            plan.source.insert(
                from_key.clone(),
                Resequenced {
                    before,
                    after: after.clone(),
                },
            );
        }
    }

    plan_alignments(
        conn,
        (from_work, into_work),
        map,
        (&source, &target),
        &standing,
        &mut plan,
    )?;
    Ok(plan)
}

/// Who answers for one pair of `into_work`'s editions once the fold is done.
#[derive(Debug, Clone, Copy)]
enum Answer {
    /// An alignment `into_work` already holds, by id.
    Held(i64),
    /// The incoming alignment at this index of the carried list.
    Carried(usize),
}

/// Which of `from_work`'s alignments move, which of `into_work`'s they
/// replace, how many are left behind, and which of `into_work`'s own the new
/// spread flags would break.
///
/// **An alignment is carried when every one of its spans covers the same
/// slots on the twins as it did on the folded work.** Runs are stored as a
/// page and a half per endpoint, not as indices, so carrying one is
/// rewriting its work and its two keys; what can change underneath it is the
/// sequence each span is read against. A span that covers the same run of
/// pages and halves before and after means the same thing, so the run needs
/// no rewrite at all, and one that does not cannot be rewritten without
/// changing how many slots it pairs. That comparison holds whichever way the
/// two works read, and it is what counts a span straddling a page one side
/// joins and the other does not.
///
/// **Left behind, and counted in [`Moved::uncarried_alignments`]:**
///
/// 1. an alignment naming an edition key `from_work` does not hold — it was
///    unreadable there, and nothing names it here;
/// 2. one whose spans would cover other slots on the twins;
/// 3. one holding a hand-corrected run whose pair `into_work` already answers
///    with an alignment that also holds one — the target keeps its own;
/// 4. one holding a hand-corrected run between two editions the fold makes
///    one, which would be an edge from an edition to itself.
///
/// **Left behind and not counted:** a solver's alignment whose pair the target
/// already answers, and one between two editions the fold makes one (the map
/// is non-injective) that holds no hand-corrected run.
///
/// **"Holds a hand-corrected run" is `merge_spoke`'s own test** — any run with
/// `by: "hand"`. `alignment.by` is not it: that reads `hand` only on an edge
/// `Sidecar::correct` created, and a correction on a solver's edge leaves it
/// `solver`. A hand-holding alignment replaces a pair's answer that holds none,
/// in either direction, which is the one case a correction is not lost.
fn plan_alignments(
    conn: &Connection,
    (from_work, into_work): (i64, i64),
    map: &BTreeMap<String, String>,
    (source, target): (&Sidecar, &Sidecar),
    standing: &BTreeMap<String, Vec<Slot>>,
    plan: &mut SlotPlan,
) -> Result<(), String> {
    let slots = |sidecar: &Sidecar, key: &str| {
        sidecar
            .editions
            .get(key)
            .map(|edition| edition.slots(sidecar.work.reading))
            .unwrap_or_default()
    };
    let after = |key: &str| {
        standing
            .get(key)
            .cloned()
            .unwrap_or_else(|| slots(target, key))
    };
    // `project::sidecar` reads alignments `ORDER BY id`, and so does this, in
    // the same transaction, so the two zip one to one.
    let source_ids = alignment_ids(conn, from_work)?;
    let target_ids = alignment_ids(conn, into_work)?;

    let mut answers: BTreeMap<(String, String), (Answer, bool)> = BTreeMap::new();
    for (id, alignment) in target_ids.iter().zip(&target.alignments) {
        answers.insert(
            pair(&alignment.from, &alignment.to),
            (Answer::Held(*id), holds_hand(alignment)),
        );
    }
    let mut carried: Vec<Option<(i64, String, String)>> = Vec::new();
    for (id, alignment) in source_ids.iter().zip(&source.alignments) {
        let hand = holds_hand(alignment);
        let (Some(from), Some(to)) = (map.get(&alignment.from), map.get(&alignment.to)) else {
            plan.uncarried += 1;
            continue;
        };
        if from == to {
            plan.uncarried += usize::from(hand);
            continue;
        }
        let (from_before, from_after) = (slots(source, &alignment.from), after(from));
        let (to_before, to_after) = (slots(source, &alignment.to), after(to));
        if !reads_the_same(
            alignment,
            (&from_before, &from_after),
            (&to_before, &to_after),
        ) {
            plan.uncarried += 1;
            continue;
        }
        let key = pair(from, to);
        match answers.get(&key).copied() {
            Some((_, held_hand)) if held_hand || !hand => {
                plan.uncarried += usize::from(hand);
            }
            answer => {
                match answer.map(|(answer, _)| answer) {
                    Some(Answer::Held(held)) => plan.replaced.push(held),
                    Some(Answer::Carried(at)) => carried[at] = None,
                    None => {}
                }
                answers.insert(key, (Answer::Carried(carried.len()), hand));
                carried.push(Some((*id, from.clone(), to.clone())));
            }
        }
    }
    plan.carried = carried.into_iter().flatten().collect();

    for (id, alignment) in target_ids.iter().zip(&target.alignments) {
        if plan.replaced.contains(id) {
            continue;
        }
        let Some(touched) = [&alignment.from, &alignment.to]
            .into_iter()
            .find(|key| plan.target.contains_key(key.as_str()))
        else {
            continue;
        };
        let (from_before, from_after) = (slots(target, &alignment.from), after(&alignment.from));
        let (to_before, to_after) = (slots(target, &alignment.to), after(&alignment.to));
        if !reads_the_same(
            alignment,
            (&from_before, &from_after),
            (&to_before, &to_after),
        ) {
            *plan.broken.entry(touched.clone()).or_default() += 1;
        }
    }
    Ok(())
}

/// One work's alignment ids, in the order `project::sidecar` reads them.
fn alignment_ids(conn: &Connection, work_id: i64) -> Result<Vec<i64>, String> {
    conn.prepare("SELECT id FROM alignment WHERE work_id = ?1 ORDER BY id")
        .and_then(|mut statement| {
            statement
                .query_map([work_id], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()
        })
        .map_err(|e| format!("reading the alignment ids of work {work_id}: {e}"))
}

/// Two edition keys as an unordered pair: an alignment and its reverse answer
/// the same question, which is `route`'s own rule.
fn pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// `Sidecar::merge_spoke`'s test for a person's correction: any run marked
/// `by: "hand"`.
fn holds_hand(alignment: &Alignment) -> bool {
    alignment
        .runs
        .iter()
        .any(|run| run.by == Some(Source::Hand))
}

/// Whether every span of `alignment` covers the same slots in the `after`
/// sequences as in the `before` ones — `from` for the `a` side, `to` for `b`.
fn reads_the_same(
    alignment: &Alignment,
    (from_before, from_after): (&[Slot], &[Slot]),
    (to_before, to_after): (&[Slot], &[Slot]),
) -> bool {
    alignment.runs.iter().all(|run| {
        covered(from_before, run.a) == covered(from_after, run.a)
            && run
                .b
                .is_none_or(|b| covered(to_before, b) == covered(to_after, b))
    })
}

/// The slots a span covers in one sequence, or `None` where either endpoint
/// is not in it or the span runs backwards there.
fn covered(slots: &[Slot], span: SlotSpan) -> Option<&[Slot]> {
    let start = slots.iter().position(|slot| *slot == span.start)?;
    let end = slots.iter().position(|slot| *slot == span.end)?;
    (start <= end).then(|| &slots[start..=end])
}

/// Rows the fold would leave naming another page, as `(work, edition key) ->
/// [(what, rows)]`: `progress` and `note` rows that named a page and that
/// [`SlotPlan`] cannot place, on either work, and `into_work` alignments its
/// new spread flags would break. A row that named no page is not one of them.
type Unplaceable = BTreeMap<(i64, String), BTreeMap<&'static str, usize>>;

fn unplaceable(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    plan: &SlotPlan,
) -> Result<Unplaceable, String> {
    let mut found = Unplaceable::new();
    for table in ["progress", "note"] {
        for (work_id, resequenced) in [(from_work, &plan.source), (into_work, &plan.target)] {
            if resequenced.is_empty() {
                continue;
            }
            // `table` is one of the two constants above, not anything a
            // caller supplies — `stranded` says the same of its own.
            let rows: Vec<(String, i64)> = conn
                .prepare(&format!(
                    "SELECT edition, slot FROM {table} WHERE work_id = ?1"
                ))
                .and_then(|mut statement| {
                    statement
                        .query_map([work_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .map_err(|e| format!("reading the `{table}` slots of work {work_id}: {e}"))?;
            for (edition, slot) in rows {
                if let Some(moved) = resequenced.get(&edition)
                    && moved.place(slot).is_none()
                {
                    *found
                        .entry((work_id, edition))
                        .or_default()
                        .entry(table)
                        .or_default() += 1;
                }
            }
        }
    }
    for (key, count) in &plan.broken {
        found
            .entry((into_work, key.clone()))
            .or_default()
            .insert("alignment", *count);
    }
    Ok(found)
}

/// Write [`SlotPlan::spreads`] onto `into_work`'s editions.
fn write_spreads(conn: &Connection, into_work: i64, plan: &SlotPlan) -> Result<(), String> {
    for (key, pages) in &plan.spreads {
        for page in pages {
            conn.execute(
                "INSERT OR IGNORE INTO page_flag (edition_id, kind, page)
                 SELECT id, 'spread', ?3 FROM edition WHERE work_id = ?1 AND key = ?2",
                rusqlite::params![into_work, key, page],
            )
            .map_err(|e| format!("writing spread flag {page} onto `{key}`: {e}"))?;
        }
    }
    Ok(())
}

/// Rewrite every `progress` and `note` slot on `work_id`'s resequenced
/// editions to where its page and half stands.
///
/// **Read whole before the first write, and keyed by each row's own key**,
/// so a rewritten slot is never read back and rewritten again. Called for
/// `into_work` before a single row moves onto it, which is what keeps a
/// carried row — already placed — from being placed a second time.
fn resequence_rows(
    conn: &Connection,
    work_id: i64,
    resequenced: &BTreeMap<String, Resequenced>,
) -> Result<(), String> {
    for (key, moved) in resequenced {
        let positions: Vec<(i64, i64)> = conn
            .prepare("SELECT user_id, slot FROM progress WHERE work_id = ?1 AND edition = ?2")
            .and_then(|mut statement| {
                statement
                    .query_map(rusqlite::params![work_id, key], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|e| format!("reading the positions on `{key}` of work {work_id}: {e}"))?;
        for (user_id, slot) in positions {
            if let Some(placed) = moved.place(slot).filter(|placed| *placed != slot) {
                conn.execute(
                    "UPDATE progress SET slot = ?1 WHERE user_id = ?2 AND work_id = ?3 AND edition = ?4",
                    rusqlite::params![placed, user_id, work_id, key],
                )
                .map_err(|e| format!("placing a position on `{key}` of work {work_id}: {e}"))?;
            }
        }
        let notes: Vec<(i64, String, i64)> = conn
            .prepare("SELECT user_id, id, slot FROM note WHERE work_id = ?1 AND edition = ?2")
            .and_then(|mut statement| {
                statement
                    .query_map(rusqlite::params![work_id, key], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|e| format!("reading the notes on `{key}` of work {work_id}: {e}"))?;
        for (user_id, id, slot) in notes {
            if let Some(placed) = moved.place(slot).filter(|placed| *placed != slot) {
                conn.execute(
                    "UPDATE note SET slot = ?1 WHERE user_id = ?2 AND work_id = ?3 AND id = ?4",
                    rusqlite::params![placed, user_id, work_id, id],
                )
                .map_err(|e| format!("placing note `{id}` on `{key}` of work {work_id}: {e}"))?;
            }
        }
    }
    Ok(())
}

/// Carry [`SlotPlan::carried`] onto `into_work`, after deleting what they
/// replace.
///
/// **An `UPDATE`, so the runs follow by `alignment_id`** and a run is never
/// copied — its spans already name the slots they will name on the twins,
/// which is what [`plan_alignments`] checked. The replaced alignments' runs
/// are deleted by name rather than left to `ON DELETE CASCADE`, so a
/// connection with foreign keys off cannot leave them orphaned.
fn carry_alignments(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    plan: &SlotPlan,
) -> Result<(), String> {
    for id in &plan.replaced {
        conn.execute("DELETE FROM run WHERE alignment_id = ?1", [id])
            .and_then(|_| conn.execute("DELETE FROM alignment WHERE id = ?1", [id]))
            .map_err(|e| format!("removing alignment {id}, which a carried one replaces: {e}"))?;
    }
    for (id, from_key, to_key) in &plan.carried {
        conn.execute(
            "UPDATE alignment SET work_id = ?1, from_key = ?2, to_key = ?3
              WHERE id = ?4 AND work_id = ?5",
            rusqlite::params![into_work, from_key, to_key, id, from_work],
        )
        .map_err(|e| format!("carrying alignment {id} onto work {into_work}: {e}"))?;
    }
    Ok(())
}

/// Move every reader row off `from_work` onto `into_work` and delete
/// `from_work`.
///
/// Returns [`Outcome::Refused`] — nothing moved, nothing deleted — on six
/// counts, each checked before the first move so that a refusal costs the
/// database nothing. Every one of the six is a `SELECT`, which is what keeps
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
/// 4. **A row the fold would leave naming another page** — see [`plan_slots`].
///    A `progress` or `note` slot, on either work, whose page and half has no
///    slot on the surviving edition, and an alignment `into_work` already holds
///    whose spans the spread flags this fold writes would move. O121.
/// 5. **One note id held by one reader on both works, with two copies that
///    differ** — see [`shared_note_ids`]. `note`'s key is
///    `(user_id, work_id, id)` and migration 5 widened it to that
///    *specifically to permit* this state, so [`move_notes`]' bare `UPDATE`
///    collides on a shape the schema calls supported. Refused rather than
///    re-keyed: the id is one the reader's own sidecar names, and rewriting it
///    silently is worse than declining. **Copies that agree in every column
///    are not refused (O78)**: they are one note, and the superseded work's
///    copy is dropped — after the sixth check below, so that a fold refused
///    over anything has written nothing — and counted in
///    [`Moved::dropped_notes`].
/// 6. **Ground truth meeting on one target edition that cannot be given one
///    provenance** — see [`truth_provenance_clashes`]. Identical `member`
///    digests do not imply identical `truth_member` digests, and moving boxes
///    under the wrong provenance records them as fresh when they are stale. Two
///    *different* lists meeting is one half of it. The other is a side holding
///    boxes and **no** list, which the fold would otherwise hand the other
///    side's — a claim those boxes never had.
///
/// **Arms 4, 5 and 6 are refusals about a state neither side can be told to
/// fix from here, which is the point.** All three leave the duplicate work
/// visibly standing, which is recoverable and which an operator can act on,
/// where a note on the wrong page, a re-keyed note and a mis-attributed box
/// are none of those.
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
    // this module is `pub`. For code that deletes a `work` row, "no caller
    // does that" is not the same as "this cannot happen".
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
        // `edition(s)` rather than a singular/plural switch: the switch would
        // be a branch no assertion could reach without a fixture built to
        // reach it, which is a test written for a comma.
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

    // **Every slot placed before anything is compared or written.** After
    // `stranded`, so every reader row still on `from_work` is on a key the map
    // holds; before the shared-note read, which compares two copies where
    // each will stand rather than as stored. See `plan_slots`.
    let plan = plan_slots(&tx, from_work, into_work, &map)?;
    let unplaced = unplaceable(&tx, from_work, into_work, &plan)?;
    if !unplaced.is_empty() {
        let rows: usize = unplaced.values().flat_map(BTreeMap::values).sum();
        let named: Vec<String> = unplaced
            .iter()
            .map(|((work_id, key), kinds)| {
                let kinds: Vec<String> = kinds
                    .iter()
                    .map(|(kind, count)| format!("{count} {kind}"))
                    .collect();
                format!("work {work_id}'s `{key}` ({})", kinds.join(", "))
            })
            .collect();
        return Ok(Outcome::Refused(format!(
            "folding work {from_work} into work {into_work} would leave {rows} row(s) naming \
             another page — {}; the two works' editions do not present those pages as the same \
             slots, and no slot on the surviving edition names the same page and half; nothing \
             was moved and nothing was deleted",
            named.join(", "),
        )));
    }

    // **After `stranded`, and that ordering is what makes the read exact.**
    // Every note still on `from_work` here is on a key the map holds, so the
    // whole-work read below is asking about exactly the rows `move_notes` is
    // about to carry across — no per-edition loop, and nothing missed.
    //
    // **Only the ids whose copies differ refuse.** Copies identical in every
    // column are one note and are dropped from the source further down —
    // after the last refusal, not here, so that a fold refused over one
    // conflicting id has written nothing, identical copies included.
    let shared = shared_note_ids(&tx, from_work, into_work, &map, &plan)?;
    let conflicting = &shared.conflicting;
    if !conflicting.is_empty() {
        // **A count and the first few, not the whole list.** The realistic
        // instance is a reader who imported one `taiyaku.notes.json` against
        // both works — `shared_note_ids`' own rationale names that route —
        // and that shares *every* id in the document: hundreds of ULIDs on
        // one line, which `DeriveReport::unfoldable` then carries into
        // `derive_summary`'s single line of output. A few are enough to go
        // and grep for; the count is what says how large the overlap is.
        const SHOWN: usize = 3;
        let listed: Vec<&str> = conflicting.iter().take(SHOWN).map(String::as_str).collect();
        let rest = conflicting.len() - listed.len();
        let and_more = if rest > 0 {
            format!(" and {rest} more")
        } else {
            String::new()
        };
        return Ok(Outcome::Refused(format!(
            "one reader holds {} note id(s) on both work {from_work} and work \
             {into_work} whose two copies differ — `{}`{and_more}; moving them would \
             collide on `note`'s (user_id, work_id, id) key, and choosing one copy over \
             the other, or re-keying an id the reader's own sidecar names, is not this \
             fold's to do; nothing was moved and nothing was deleted",
            conflicting.len(),
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

    // **The identical note copies go first, and only now.** After the last
    // refusal above, so every refusal is still a `SELECT` and a fold refused
    // over one conflicting id has not touched the identical ones beside it;
    // before `move_notes`, whose `UPDATE` would otherwise collide on exactly
    // these ids. See `shared_note_ids` for what "identical" is.
    let mut moved = Moved {
        dropped_notes: drop_identical_notes(&tx, from_work, &shared.identical)?,
        uncarried_alignments: plan.uncarried,
        ..Moved::default()
    };

    // **The flags, then `into_work`'s own rows, then `from_work`'s, then the
    // alignments — and every one of them before the move loop.** The flags
    // are what the new sequences were built from. The target's own rows are
    // placed while they are still the only rows on those editions: once a
    // carried row has moved onto one, a rewrite keyed on the edition would
    // place it a second time. See `plan_slots`.
    write_spreads(&tx, into_work, &plan)?;
    resequence_rows(&tx, into_work, &plan.target)?;
    resequence_rows(&tx, from_work, &plan.source)?;
    carry_alignments(&tx, from_work, into_work, &plan)?;

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
    // inside a function that deletes a work.
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

/// What [`rehome_editions`] carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rehomed {
    pub positions: usize,
    pub notes: usize,
    pub truth_pages: usize,
    /// Alignments between a rehomed edition and one that stayed. An
    /// alignment names two editions of one work, and these two are editions
    /// of two works now.
    pub alignments_dropped: usize,
}

/// What [`rehome_editions`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rehoming {
    Done(Rehomed),
    /// Nothing was moved and nothing deleted. As with [`Outcome::Refused`],
    /// a refusal is an outcome and not an error.
    Refused(String),
}

/// Give `into_work` the edition rows `keys` of `from_work`, in place of its
/// own editions of those keys, with everything a reader put on them.
///
/// **For a work a directory move divides** (`crate::relocate`). The derive that
/// follows the move leaves `from_work` holding editions whose archives now
/// belong to another library, and makes `into_work` there with fresh
/// editions over the same archives. [`absorb`] cannot carry them: it moves a
/// whole work and deletes it, and `from_work` keeps its other editions.
///
/// **The row moves, not a copy of it.** `into_work`'s fresh edition is
/// deleted and `from_work`'s is re-parented in its place, so its spread
/// flags, credit flags, label and declared chapters are still on it, and a
/// slot names the page it named. That is why this needs none of
/// [`plan_slots`]: nothing about the edition changed but the work it is in.
///
/// Refused, each read before the first write:
///
/// 1. `from_work == into_work`.
/// 2. A key either work does not hold, or held over different archives:
///    member digests are compared in ordinal order, as [`edition_map`]
///    compares them.
/// 3. A key that is `from_work`'s hub. A work's content key and its hand
///    document are its hub's, so a work goes where its hub goes and what is
///    rehomed is the rest.
/// 4. `into_work` already holding a position, a note, a ground-truth page or
///    an alignment on one of the keys. Its edition is expected fresh, and
///    merging two readers' rows is [`absorb`]'s job, with its refusals.
/// 5. A note id one reader holds on both works: `note`'s key is
///    `(user_id, work_id, id)`.
/// 6. A key named more than once.
/// 7. `into_work`'s edition of a key carrying anything a person made: a
///    label, kind or language set by hand, a chapter number set by hand, a
///    flag on a page (a credit flag, or a spread flag, which a scan never
///    writes and which comes from an imported document or a fold), or a
///    credit of the edition. The edition is
///    deleted, and these would go with it. The twin of an ordinary divided
///    work is fresh, made by the derive a moment before, and holds none.
///    One that has been read or edited since is a long-lived work's, and the
///    row that would replace it cannot carry what is on it.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub(crate) fn rehome_editions(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    keys: &[String],
) -> Result<Rehoming, String> {
    let refuse = |why: String| {
        Ok(Rehoming::Refused(format!(
            "{why}; nothing was moved and nothing was deleted"
        )))
    };
    if from_work == into_work {
        return refuse(format!("work {from_work} cannot give editions to itself"));
    }
    // A key named twice would be moved by the first pass and deleted as the
    // fresh edition by the second.
    for (at, key) in keys.iter().enumerate() {
        if keys[..at].contains(key) {
            return refuse(format!("edition `{key}` is named twice"));
        }
    }

    let tx = crate::Nested::open(conn, "rehome_editions")
        .map_err(|e| format!("starting to rehome editions of work {from_work}: {e}"))?;

    let hub: String = tx
        .query_row("SELECT hub FROM work WHERE id = ?1", [from_work], |r| {
            r.get(0)
        })
        .map_err(|e| format!("reading the hub of work {from_work}: {e}"))?;
    let source = digests_by_edition(&tx, from_work)?;
    let target = digests_by_edition(&tx, into_work)?;
    let digests = |held: &[(String, Vec<String>)], key: &str| {
        held.iter()
            .find(|(held_key, _)| held_key == key)
            .map(|(_, digests)| digests.clone())
    };
    for key in keys {
        if *key == hub {
            return refuse(format!(
                "`{key}` is the hub edition of work {from_work}, and a work goes where its hub \
                 goes"
            ));
        }
        let (Some(ours), Some(theirs)) = (digests(&source, key), digests(&target, key)) else {
            return refuse(format!(
                "work {from_work} and work {into_work} do not both hold an edition `{key}`"
            ));
        };
        if ours != theirs {
            return refuse(format!(
                "edition `{key}` of work {from_work} and of work {into_work} do not hold the \
                 same archives in the same order"
            ));
        }
        for (table, column) in [
            ("progress", "edition"),
            ("note", "edition"),
            ("truth_page", "edition"),
            ("alignment", "from_key"),
            ("alignment", "to_key"),
        ] {
            let held: i64 = tx
                .query_row(
                    &format!("SELECT count(*) FROM {table} WHERE work_id = ?1 AND {column} = ?2"),
                    rusqlite::params![into_work, key],
                    |r| r.get(0),
                )
                .map_err(|e| format!("reading {table} of work {into_work}: {e}"))?;
            if held > 0 {
                return refuse(format!(
                    "work {into_work} already holds {held} {table} row(s) on edition `{key}`"
                ));
            }
        }
    }
    for key in keys {
        for (what, sql) in [
            (
                "a label, kind or language set by hand",
                "SELECT count(*) FROM edition WHERE work_id = ?1 AND key = ?2
                    AND (label_hand IS NOT NULL OR kind_hand IS NOT NULL
                         OR language_hand IS NOT NULL)",
            ),
            (
                "a chapter number set by hand",
                "SELECT count(*) FROM member m JOIN edition e ON e.id = m.edition_id
                  WHERE e.work_id = ?1 AND e.key = ?2
                    AND m.chapter_declared IS NOT NULL AND m.chapter_source = 'hand'",
            ),
            (
                "a flag on a page",
                "SELECT count(*) FROM page_flag f JOIN edition e ON e.id = f.edition_id
                  WHERE e.work_id = ?1 AND e.key = ?2",
            ),
            (
                "a credit",
                "SELECT count(*) FROM credit c JOIN edition e ON e.id = c.edition_id
                  WHERE e.work_id = ?1 AND e.key = ?2",
            ),
        ] {
            let held: i64 = tx
                .query_row(sql, rusqlite::params![into_work, key], |r| r.get(0))
                .map_err(|e| format!("reading what a person put on work {into_work}: {e}"))?;
            if held > 0 {
                return refuse(format!(
                    "work {into_work} already holds {what} on edition `{key}`, which replacing \
                     the edition would delete"
                ));
            }
        }
    }
    let shared: i64 = tx
        .query_row(
            "SELECT count(*) FROM note ours JOIN note theirs
                ON theirs.user_id = ours.user_id AND theirs.id = ours.id
             WHERE ours.work_id = ?1 AND theirs.work_id = ?2",
            [from_work, into_work],
            |r| r.get(0),
        )
        .map_err(|e| format!("reading the notes of works {from_work} and {into_work}: {e}"))?;
    if shared > 0 {
        return refuse(format!(
            "one reader holds {shared} note id(s) on both work {from_work} and work {into_work}"
        ));
    }

    let mut moved = Rehomed::default();

    // An alignment both of whose editions go, goes. One with an edition on
    // each side is between two works now, and is dropped and counted.
    let alignments: Vec<(i64, String, String)> = tx
        .prepare("SELECT id, from_key, to_key FROM alignment WHERE work_id = ?1")
        .map_err(|e| format!("reading the alignments of work {from_work}: {e}"))?
        .query_map([from_work], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| format!("reading the alignments of work {from_work}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading the alignments of work {from_work}: {e}"))?;
    for (id, from_key, to_key) in alignments {
        match (keys.contains(&from_key), keys.contains(&to_key)) {
            (true, true) => {
                tx.execute(
                    "UPDATE alignment SET work_id = ?2 WHERE id = ?1",
                    [id, into_work],
                )
                .map_err(|e| format!("moving alignment {id}: {e}"))?;
            }
            (false, false) => {}
            _ => {
                tx.execute("DELETE FROM run WHERE alignment_id = ?1", [id])
                    .and_then(|_| tx.execute("DELETE FROM alignment WHERE id = ?1", [id]))
                    .map_err(|e| format!("dropping alignment {id}: {e}"))?;
                moved.alignments_dropped += 1;
            }
        }
    }

    for key in keys {
        tx.execute(
            "DELETE FROM edition WHERE work_id = ?1 AND key = ?2",
            rusqlite::params![into_work, key],
        )
        .map_err(|e| format!("removing the fresh edition `{key}` of work {into_work}: {e}"))?;
        tx.execute(
            "UPDATE edition SET work_id = ?2 WHERE work_id = ?1 AND key = ?3",
            rusqlite::params![from_work, into_work, key],
        )
        .map_err(|e| format!("moving edition `{key}` to work {into_work}: {e}"))?;

        moved.positions += move_positions(&tx, from_work, into_work, key, key)?;
        // `move_positions` copies, and `absorb` leaves the source rows to the
        // delete of the work. This work stays, so they are deleted here.
        tx.execute(
            "DELETE FROM progress WHERE work_id = ?1 AND edition = ?2",
            rusqlite::params![from_work, key],
        )
        .map_err(|e| format!("clearing moved positions off work {from_work}: {e}"))?;
        moved.notes += move_notes(&tx, from_work, into_work, key, key)?;
        let (pages, _) = move_truth(&tx, from_work, into_work, key, key)?;
        moved.truth_pages += pages;
        // `move_truth` copies too. `truth_box` goes with its page.
        tx.execute(
            "DELETE FROM truth_page WHERE work_id = ?1 AND edition = ?2",
            rusqlite::params![from_work, key],
        )
        .and_then(|_| {
            tx.execute(
                "DELETE FROM truth_member WHERE work_id = ?1 AND edition = ?2",
                rusqlite::params![from_work, key],
            )
        })
        .map_err(|e| format!("clearing moved ground truth off work {from_work}: {e}"))?;
    }

    tx.commit()
        .map_err(|e| format!("committing the rehoming of work {from_work}'s editions: {e}"))?;
    Ok(Rehoming::Done(moved))
}

/// A reader's place, carried across with its history.
///
/// `completed_at` and `read_count` come too: they are facts about reading
/// this rendition, and the rendition is what moved. 001's own comment calls
/// them "behavioural data with NOTHING to backfill", so leaving them behind
/// would be the fold quietly resetting a finished re-read.
///
/// **The conflict is reachable, by two routes this module documents itself.**
/// [`edition_map`] is explicitly non-injective, so two editions of `from_work`
/// over identical digests both land on the one edition of `into_work` inside a
/// single fold and the second `INSERT` meets the first; and `progress.edition`
/// has no foreign key to `edition(key)` — the fact [`stranded`] rests on — so
/// `into_work` can already carry a grandfathered row under the mapped key.
///
/// **Counted rather than refused, and the grandfathered row is why that is not
/// a choice.** A refusal would need to tell the two apart: a target row that is
/// a genuine reading of the mapped edition, where "newer wins" is the right
/// answer, and one grandfathered under a key whose edition this derive has only
/// just created, where the surviving slot means nothing. Nothing in the schema
/// separates them. [`stranded`]'s test cannot be turned on the target either:
/// `into_key` is drawn from `into_work`'s own `edition` table by
/// [`edition_map`], so a target row under the mapped key is never a row on a
/// key the work does not hold, and the blanket version of it would refuse every
/// fold into a target carrying any grandfathered row, a state `taiyaku-serve`'s
/// `notes` module *deliberately supports*. That would be a narrowing that
/// defeats the fold while looking like safety. So the loss is made visible
/// instead, which is what the module asks of every derived thing it declines
/// to move.
///
/// **So the loss is counted, and not here.** `execute` returns rows actually
/// written, so this function can report how many times it wrote and nothing
/// more: the `WHERE excluded.updated > progress.updated` makes the source's row
/// a silent no-op when the target's is newer, and the `DO UPDATE` makes the
/// target's row a silent casualty when the source's is. Neither is visible in a
/// per-edition subtraction — two source editions landing on one target row
/// report `changes = 1` each, `offered - landed = 0` each, with a position
/// gone. [`absorb`] therefore counts positions as a whole-fold identity across
/// every call to this function; see [`Moved::lost_positions`] for the
/// arithmetic. "Newer wins" itself stays: a slot number is not mergeable, and
/// the later reading is the one somebody is in the middle of.
///
/// **On a conflict the newer row wins entire, history included**, which is why
/// the `DO UPDATE` sets all four columns rather than the two that make up the
/// place. Setting `slot` and `updated` alone would produce a row belonging to
/// neither reading — the source's place under the target's re-read count.
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
/// **The rewrite is the whole of this function's risk.** An `UPDATE` that moved
/// `work_id` alone would be accepted by every constraint in the schema —
/// landing a note about a colour page on the monochrome archive at the same
/// slot number, with nothing saying so. `truth_member`'s own schema comment
/// calls this the sharpest form of the problem in the project. `fold::tests`'s
/// `a_moved_note_names_the_edition_that_holds_its_archive` is the only thing
/// that catches it.
///
/// **The id being a ULID does not mean nothing collides.** `note`'s key is
/// `(user_id, work_id, id)` and migration 5 widened it to that *deliberately to
/// permit* one reader holding one id on two works — its rationale names the
/// routes, "copying a `taiyaku.notes.json` between volumes, or importing one
/// sidecar against two works", and a fold pair is by construction two works
/// over the same volume. The `UPDATE` below then fails outright, so [`absorb`]
/// reads for that state first: it refuses where the two copies differ, and
/// where they agree in every column it has already dropped the source's copy by
/// the time this runs, so nothing here collides; see [`shared_note_ids`]
/// and [`drop_identical_notes`].
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
/// foreign key names `(work_id, edition, page)` with no `ON UPDATE`, so `UPDATE
/// truth_page SET work_id = …` is refused outright, pinned by
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
/// case where it would matter.** The mapping is by identical digest list, which
/// does not mean that a target holding a `truth_member` row holds the same
/// digests in the same order: [`edition_map`] compares `member.sha256`, the
/// archives as they are now, while `truth_member` is a client-supplied snapshot
/// of what the boxes were *drawn against*, which `state::put_truth` writes
/// verbatim. The two diverging is the whole reason `TruthEdition::is_stale_for`
/// exists. So the divergence is checked for by name and refused before any
/// write; see [`truth_provenance_clashes`]. What reaches this statement is a
/// target that holds the same list or none at all — and, by that guard's second
/// arm, never boxes on either side carrying no list of their own while one is
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
