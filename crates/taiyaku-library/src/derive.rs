//! `scan_entry` + `rule` → series, works, editions, members.
//!
//! **Exactly one input, which is what makes §2's claim true.** Structure comes
//! from the facts and the rules and from nothing else — not from a manual
//! accept, which §6.1 routes through a written `rule` for precisely this
//! reason. Two inputs would mean a re-derive either discards the accepts or
//! needs a merge rule to protect them, which is the problem the store spec
//! already solved once with `by: "hand"` and `Sidecar::merge_spoke`; solving
//! it a second time in a different shape is how two mechanisms come to
//! disagree.
//!
//! **It never opens a file.** Page counts and digests are columns on
//! `scan_entry`, written by the walk, so editing a rule re-derives 156 series
//! at memory speed. `derive/tests.rs` deletes the tree before the second
//! derive, which is the only way to test a negative like this.
//!
//! # The one file it does open, and why that is not a contradiction
//!
//! §3 makes hashing lazy: `scan.rs` leaves `sha256` NULL because hashing 156
//! series on every walk is not affordable. A member about to become part of a
//! work needs its digest — `content_key` is a hash over them — so this module
//! is where that laziness ends, once per member ever, with the answer written
//! **back onto `scan_entry`**. That is what makes the *next* derive need no
//! filesystem at all, which is the property the tests pin.
//!
//! # One writer per table, and what it writes directly
//!
//! `work`, `edition` and `member` **rows** are written by [`crate::import`]
//! alone — this module builds a [`Sidecar`] and hands it to
//! [`crate::import::scanned`] rather than issuing its own inserts, so the
//! column lists cannot drift from `project.rs`'s reader. What it then writes
//! itself is exactly what the sidecar format has no field for and so cannot
//! carry: `work.series_id`, `work.volume`, `edition.kind`/`edition.language`,
//! `member.chapter_declared`/`chapter_source` and `member.root_id`. All but
//! the last of those are already targeted updates in `librarydoc::apply`, so
//! this is the established shape here rather than a new one; a member's root
//! is the one column nothing but a walk can know. `series` and `series_title`
//! have no importer at all and are written here outright.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};
use taiyaku_core::{Direction, Edition, Member, Sidecar, Work};

use crate::proposals;
use crate::rules::Engine;
use crate::scan::Root;

/// The edition key a work gets when nothing sets one.
///
/// Reached when neither a rule nor the root's own `edition_key` column
/// decides — a root someone added and has not configured. `raw` rather than
/// something neutral like `default` because it is what an unconfigured root
/// holding one edition per work actually is in this corpus, and because a
/// reader meets this string: it is the edition name in the swap UI. C4's root
/// editor is what changes it, and a rule overrides it per path.
const DEFAULT_EDITION_KEY: &str = "raw";

/// The `member.chapter_source` this module writes, and the only value it will
/// clear.
///
/// `filename` because every route a rule has to a chapter number reads a
/// name: `builtin-chapter` is handed [`taiyaku_read::chapter_number`] over a
/// path component, and a `regex` rule's `chapter` capture is taken from the
/// same string. `comicinfo` is the other machine source and nothing writes it
/// yet; `hand` is a person's, and the clear below is scoped by this value
/// precisely so a re-derive cannot reach one.
const SCANNER_CHAPTER_SOURCE: &str = "filename";

/// What one [`fn@derive`] call turned the facts into.
///
/// `unreadable` counts every entry derivation could make no structure out of
/// for want of readable bytes, which is four situations and not one: a format
/// [`taiyaku_read::Format::is_readable`] refuses, a container the walk met but
/// could not list (so `pages` is NULL), a container that listed cleanly and
/// held no page images of its own (so `pages` is `Some(0)` — a volume
/// directory holding archives is one, and the archives inside it are what
/// become the members), and a member whose file is no longer there to hash.
/// All four are counted rather than raised, because one unmounted share or one
/// truncated download must not leave a whole library underived.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeriveReport {
    pub series: usize,
    pub works: usize,
    pub editions: usize,
    pub members: usize,
    pub unattached_chapter_dirs: usize,
    pub unreadable: usize,
    /// Entries an ancestor spoke for — a volume split into chapter
    /// subdirectories contributes one member and three of these. See
    /// [`Seen::claims_subtree`].
    ///
    /// **Counted because a silent drop is indistinguishable from an empty
    /// library.** A rule set that made a series directory speak for its
    /// volumes would swallow every one of them, and without this number the
    /// result reads as a library that happens to hold nothing rather than as
    /// a rule that swallowed it. That is not hypothetical any more: a
    /// `set_series` rule broad enough to match the root's own `rel_path`
    /// collapses the whole library onto one work, and this is the field that
    /// says so.
    ///
    /// **Every entry the ancestor test dropped, not only the ones that would
    /// have been members.** A directory the rules gave neither a volume nor a
    /// chapter is suppressed too — `Series - v01/Chapters/` is one — and it
    /// would otherwise have become a volume-less work of its own, so leaving
    /// it out of the count would be this field under-reporting the very thing
    /// it exists to report.
    pub suppressed: usize,
    /// Works whose `content_key` moved under them — the volume's files
    /// changed, so its identity did, and the row followed rather than a second
    /// one being minted beside it.
    ///
    /// **Reported for the same reason [`Self::suppressed`] is: a reconcile
    /// that silently did the wrong thing would be worse than the bug it
    /// fixes.** Adding a page, repacking an archive or fixing a scan all land
    /// here, and all of them are ordinary. A number nobody prints is a change
    /// nobody can see.
    ///
    /// **It over-counts in one degenerate case.** The test is "the row at this
    /// address holds a different key", which is not quite "this work followed
    /// its files": where two volumes have been given identical bytes, content
    /// resolves the group onto a *third* row and the address is never
    /// consulted, so nothing followed anything and this still counts one. That
    /// needs a duplicated download to reach — see
    /// `a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive` — and
    /// narrowing it would mean asking `import` which probe answered, which is
    /// a second question for a signature that answers one.
    pub rekeyed: usize,
    /// Editions left standing on a work this derive wrote, that this derive
    /// did not derive.
    ///
    /// **The stated cost of [`crate::import::scanned`] no longer deleting
    /// them.** A scan cannot tell "the scan never saw this edition" from "a
    /// rule edit removed it", so it must not act as though it can — deleting
    /// a hand-aligned edition is unrecoverable, while an orphan is visible
    /// and fixable. What that leaves is an orphan nobody asked for: edit a
    /// rule so `raw` becomes `colour`, and the work keeps a `raw` edition
    /// naming files the rules no longer place there.
    ///
    /// So it gets a number, for the same reason [`Self::suppressed`] and
    /// [`Self::rekeyed`] have one — the outcome changes what a person sees
    /// and cannot be inferred from the other counts. `editions` counts what
    /// this derive wrote; nothing in it says how many were already there.
    ///
    /// **Counted on every derive, not only the one that orphaned it.** A
    /// standing orphan is a standing fact, and a count that appeared once and
    /// then went quiet would say the library had been fixed.
    pub kept_editions: usize,
    /// Works whose every member is now held by a work this derive wrote — see
    /// [`superseded`] for the five conditions — and which this derive has
    /// therefore folded away: their reader rows moved onto the target and the
    /// `work` row itself deleted, by `fold::absorb`.
    ///
    /// **Counted for the reason [`Self::kept_editions`] is, and more so.** A
    /// store that silently deleted a work would be worse than the duplication
    /// (O73) it fixes, and a number nobody prints is a change nobody can see.
    /// This is the only `DELETE FROM work` `derive` itself reaches — a
    /// second exists in `scan::apply_removal`, deleting the works a removed
    /// root alone accounted for, before that function calls into
    /// `derive_with` in turn — so it is the last thing that should do its
    /// job quietly — which is why the four counters below travel with it
    /// rather than leaving "1 work folded away" to stand for whatever
    /// happened inside.
    pub emptied_works: usize,
    /// Superseded works left standing because the fold was refused.
    ///
    /// **`superseded` covers the digest *set*, not the edition layout.** A
    /// candidate holding `[X, Y]` as one edition, against a target holding
    /// `[X]` and `[Y]` as two, satisfies every one of that function's five
    /// conditions and has no edition on the target holding its archives in
    /// its own order. `fold::absorb` refuses such a pair outright — half a
    /// fold is worse than none — and `derive` counts the refusal
    /// rather than propagating it: a scan that died because one work of
    /// thousands could not be folded would be worse than a scan that folds
    /// nothing. The work stays visible and duplicated, which is the safe
    /// direction, and this is what says so.
    ///
    /// A refusal is not always a mapping failure. `fold::absorb` also refuses
    /// a work carrying a reader's row on an edition key it no longer holds; a
    /// pair where one reader holds one note id on both works and the two
    /// copies differ, which `note`'s key was widened to permit and which the
    /// move would collide on (copies identical in every column are dropped
    /// from the superseded work instead, and counted in
    /// [`Self::dropped_notes`]); and a
    /// pair whose ground truth on a mapped edition was drawn against different
    /// archives on the two sides. All three are the reader's own state or the
    /// library's ground truth, and none is this fold's to destroy or to
    /// rewrite. [`Self::unfoldable`] is where the difference is legible.
    ///
    /// **And since O121, a fold that would leave a row naming another page** —
    /// a reading position or a note whose page and half has no slot on the
    /// surviving edition, or an alignment the surviving work already holds
    /// whose spans the spread flags the fold writes would move. A slot is an
    /// index into one edition's spreads and reading direction, and the fold
    /// places it by page and half rather than carrying the number.
    pub unfolded_works: usize,
    /// Why each of those works was left standing, one message per work,
    /// naming the work and what could not be mapped or was stranded on it.
    ///
    /// **A count says something was refused; it does not say what to look
    /// at.** The refusal message `fold::absorb` builds already names the work
    /// id and the edition keys, and dropping it for a number left an operator
    /// with "1 superseded work left standing" over a library of thousands.
    /// [`crate::scan::ScanReport::unreadable_dirs`] is the precedent and the
    /// same argument: a `Vec` costs one field, and the alternative is a
    /// person grepping for a work they cannot name.
    ///
    /// In `superseded`'s own order, which is deterministic, so two runs over
    /// an unchanged library produce the same list.
    pub unfoldable: Vec<String>,
    /// Hand documents that could not be read, parsed, or applied, each with
    /// the reason and, ordinarily, the path.
    ///
    /// A `Vec` rather than a counter — `unfoldable`'s choice — because the
    /// person has to be told *which* file to fix.
    ///
    /// **The path is prepended by the read-back, not inherited from the
    /// error — except when `hand::load` fails before it has one to prepend.**
    /// `hand::load`'s own read/parse failures carry the path, and
    /// `librarydoc::apply`'s name a `work_id` and a `series_id` and never a
    /// file, so the read-back prepends the path there — that is the exact gap
    /// this field exists to close. But `hand::load` fails a third way, before
    /// either: `hand::path_for`'s two errors (reading `work.content_key`, then
    /// the hub edition's root) can only name the `work_id`, since SQLite
    /// refused the very query that would have produced an address to name.
    /// Those two propagate through `load` unprefixed, so this field is not
    /// "each with the path" without exception — only whichever a path had
    /// already been resolved by the time something failed.
    pub unreadable_hand: Vec<String>,
    /// Documents written on this pass that were not on disk — O82's retry.
    ///
    /// **Deliberately not "that a failed save had left missing", which this
    /// said until 2026-09-02.** A failed write is one cause and no longer the
    /// only one: a root whose `keeps_documents` was off when the edit was
    /// made, and has since been turned on, arrives here with its marker armed
    /// and no file — the ordinary path for anyone opting a root back in, and
    /// the one a person is most likely to be watching. Naming a cause the
    /// count cannot know is how a report starts lying about the common case.
    ///
    /// **A field rather than a silence, under the same rule as
    /// [`Self::suppressed`] and [`Self::rekeyed`]:** any derivation outcome
    /// that changes what a person sees and cannot be inferred from the other
    /// counts gets one. A correction becoming durable is exactly that.
    pub repaired_documents: usize,
    /// Documents that still could not be written, each naming the path and the
    /// `io::Error` — `hand::Saved::why`'s own shape.
    ///
    /// A `Vec` rather than a counter, for [`Self::unreadable_hand`]'s reason:
    /// the person has to be told *which* root cannot take its documents. A
    /// permanently read-only root reports the same list every scan, which is
    /// honest rather than noisy — the condition really is still true.
    pub unwritable_documents: Vec<String>,
    /// Documents under a root's `.taiyaku` that nothing in the library
    /// addresses — O81.
    ///
    /// **Never a document a live work still addresses.** A root that has
    /// declined to keep documents reports those under
    /// [`Self::declined_documents`] instead — see [`crate::hand::Orphan`]'s
    /// own doc comment for the Critical that folding the two together was:
    /// migration 012 defaults every existing root's `keeps_documents` off,
    /// so a report that called an addressed file an orphan named *every*
    /// hand document in the library as stranded on the first derive after
    /// upgrading.
    ///
    /// Each carries the title out of the file, because the filename is the
    /// `content_key` the document was written under and names content that no
    /// longer exists. See [`crate::hand::Orphan`].
    pub orphan_documents: Vec<crate::hand::Orphan>,
    /// Documents at a work's own address whose root has declined to keep
    /// them: `root.keeps_documents` is off, so nothing is owed, but a live
    /// work still addresses the file — see [`crate::hand::Declined`].
    ///
    /// **A field rather than a silence, under the same rule as
    /// [`Self::suppressed`] and [`Self::rekeyed`]: any derivation outcome
    /// that changes what a person sees and cannot be inferred from the other
    /// counts gets one.** Before this field existed, `hand::reconcile` folded
    /// this case into [`Self::orphan_documents`], so every derive after
    /// migration 012 (which turns `keeps_documents` off for every existing
    /// root) reported every hand document in the library as stranded — on
    /// the very pass that `hand::load` read each one's title back into the
    /// database. This name is what lets a person read the difference between
    /// "act on this, it is stranded" and "this root simply no longer keeps
    /// what is here."
    pub declined_documents: Vec<crate::hand::Declined>,
    /// Documents at a work's own address that disagree with the database.
    ///
    /// **Reported and resolved by nothing**, deliberately: O44 records this as
    /// the one document this project designs for hand-editing, so the database
    /// is not automatically right. The design's §2 records the alternatives
    /// and what would earn a resolution rule.
    pub stale_documents: Vec<String>,
    /// Reading positions written onto the target's matching edition by a
    /// fold — writes, not survivors, for the reason `fold::Moved::positions`
    /// states. [`Self::lost_positions`] is the exact number.
    pub moved_positions: usize,
    /// Reading positions that existed before a fold and do not exist after
    /// it — the superseded work's or the target's own.
    ///
    /// **The conflict is reachable, and `fold` documents both routes.**
    /// `fold::edition_map` is explicitly non-injective, so two editions of the
    /// superseded work over identical digests both land on the one edition of
    /// the target that holds them inside a single fold; and `progress.edition`
    /// has no foreign key to `edition(key)`, so the target can already carry a
    /// grandfathered row under the mapped key. "The newer position wins" is
    /// the decision and it stands — a slot number is not mergeable — but the
    /// loser is a reading position the fold's `DELETE` then makes
    /// unrecoverable.
    ///
    /// **A whole-fold identity rather than a per-edition subtraction**, which
    /// matters to what this number can be trusted to say:
    /// `fold::Moved::lost_positions` reads the `progress` rows on both works
    /// before the move loop and on the target after it, so it counts a row
    /// overwritten by an incoming one as well as a row that never landed, and
    /// does not depend on which edition key sorted first. The subtraction it
    /// replaced could see neither, and reported `0` for a fold that destroyed
    /// a position.
    ///
    /// Counted for [`Self::moved_truth_collisions`]' reason, which is the same
    /// reason: a silent loss is indistinguishable from no loss.
    pub lost_positions: usize,
    /// Notes moved onto the target's matching edition by a fold, `edition`
    /// rewritten with them.
    ///
    /// **Rows moved, and only those.** A copy the fold dropped because the
    /// target already held it identically is [`Self::dropped_notes`], never
    /// here.
    pub moved_notes: usize,
    /// Notes a fold deleted from the superseded work because the target
    /// already held the same note, under the same id, for the same reader —
    /// `fold::Moved::dropped_notes`, summed.
    ///
    /// **O78.** "The same" is every column but the two a fold rewrites: body,
    /// slot, region, colour, `created` and `updated` equal, and the target's
    /// edition exactly the one the superseded work's edition maps onto. Those
    /// two rows are one fact written twice — the ordinary result of one
    /// sidecar imported against both volumes — and until 2026-09-14 they
    /// refused the whole fold. Copies that differ in anything still refuse,
    /// and are named in [`Self::unfoldable`].
    ///
    /// A field rather than a silence, under the rule [`Self::suppressed`] and
    /// [`Self::rekeyed`] state: a reader who counts their notes after a fold
    /// finds these gone, and [`Self::moved_notes`] cannot say why.
    pub dropped_notes: usize,
    /// Labelled truth pages, with their boxes, moved onto the target.
    pub moved_truth_pages: usize,
    /// Truth pages a fold left behind because the target already held them.
    ///
    /// Counted for [`Self::kept_editions`]' reason: truth is library-wide, so
    /// this is somebody's labelling that the fold declined to overwrite, and
    /// a labelling silently not moved is exactly the loss these counters
    /// exist to make visible.
    pub moved_truth_collisions: usize,
    /// Rows a fold's `DELETE` took that the fold did not move.
    ///
    /// Credits, tags, collection places, credit page flags, hand-entered
    /// chapter numbers and hand-classified editions — `fold::DISCARDED` is the
    /// list and states, per entry, why each is counted rather than migrated.
    /// *Until O76, 2026-09-15, it counted alignments, their runs and spread
    /// flags too, and this doc said the number existed so that "1 work folded
    /// away" could never quietly mean "and its hand-corrected alignment went
    /// with it". A fold carries those now, and the alignment it cannot carry is
    /// [`Self::uncarried_alignments`].*
    pub discarded_rows: usize,
    /// Alignments a fold did not carry onto the surviving work and that
    /// nothing there stands in for — `fold::Moved::uncarried_alignments`,
    /// summed.
    ///
    /// **O76.** A fold carries an alignment when every span of it covers the
    /// same pages and halves on the surviving work's editions as it did on the
    /// folded work's. This counts the ones it could not: an alignment naming
    /// an edition the folded work did not hold, one whose spans would cover
    /// other slots there, a hand-corrected one whose pair the surviving work
    /// already answers with a hand-corrected alignment of its own, which it
    /// keeps, and a hand-corrected one between two editions the fold makes
    /// one. A solver's alignment whose pair is already answered is not
    /// counted, having an answer, and nor is a solver's edge between two
    /// editions that become one.
    ///
    /// A field rather than a clause of [`Self::discarded_rows`], because it is
    /// the one thing a fold leaves behind that a person may have made and a
    /// rescan cannot make again.
    pub uncarried_alignments: usize,
    /// Series holding no work at all.
    ///
    /// **Left standing deliberately.** `series` has no dependency on `work` —
    /// `work.series_id` is `ON DELETE SET NULL` in the other direction — so a
    /// fold that deletes a series' last work leaves the row behind. `browse`
    /// filters a series with no works out of its listing, so it is invisible
    /// rather than wrong, and deleting it would cascade `series_title`,
    /// `series_tag`, `credit`, `external_id` and `collection_item` — a second
    /// cascade question, and not the one the fold was built to answer.
    /// Counted so that "invisible" never quietly becomes "unknown".
    ///
    /// **Counted on every derive, over the whole library**, not only over the
    /// works this pass folded: a series standing with nothing in it is a
    /// standing fact, for [`Self::kept_editions`]' reason.
    ///
    /// **So a fold is not the only way in, and the name says so since
    /// 2026-08-27.** [`fn@derive`] mints a series row for every series a rule or
    /// a directory names, including one holding nothing but unattached chapter
    /// directories — an ordinary shape in this project's corpus — so a first
    /// scan of a library that has never folded anything reports one of these.
    /// It was called `emptied_series`, and `taiyaku-cli`'s summary printed
    /// "N series left empty" from it on the same line as the fold's own
    /// counts.
    pub series_holding_no_work: usize,
    /// Entries that joined an edition another entry had already opened — a
    /// volume arriving as two chapter directories contributes two members and
    /// one of these.
    ///
    /// **A merge is a page sequence.** Member order is page order (see
    /// [`write_work`]), so two entries in one edition are one continuous
    /// volume as far as every alignment offset after the first member is
    /// concerned. That is right for the chapters of a volume and wrong for
    /// anything else, and derivation cannot always tell which it has: the
    /// rules resolved a volume for both, and that is all it knows.
    ///
    /// So it is counted, for the same reason [`Self::suppressed`] is. Until
    /// this field existed, `ONE PIECE/001..004` — four bare-numbered
    /// directories the rules deliberately refuse to read a volume out of —
    /// became **one** work of 18 pages, reported as one work with nothing
    /// distinguishing it from a work that genuinely holds one volume. That
    /// particular merge is fixed (those entries are keyed individually now),
    /// and the ones that remain are reported rather than assumed correct.
    pub merged: usize,
    /// Editions whose rules named an `edition.kind` the column's closed set
    /// does not hold, so none was written.
    ///
    /// **Counted for the reason [`Self::suppressed`], [`Self::rekeyed`],
    /// [`Self::kept_editions`] and [`Self::merged`] are.** A typo in a
    /// `set_kind` otherwise decides nothing in silence, and reads from every
    /// side exactly like a rule that named no kind at all — while the person
    /// who wrote it is looking at an edition whose kind never moved.
    ///
    /// See [`write_edition_metadata`] for why the value is dropped rather
    /// than stored, and what stores it would break.
    pub unrecognised_kinds: usize,
    /// Newly-opened proposals this derive's own [`proposals::raise`] call
    /// produced.
    ///
    /// **The counter that used to read zero unconditionally.** `derive`
    /// already calls `proposals::raise` at the end of every pass; `library
    /// scan` (`taiyaku-cli`) used to call it a *second* time to get a number
    /// to print, and every proposal that first call had just opened was
    /// already in `proposal` by then, so the second call's `INSERT OR
    /// IGNORE` conflicted on every single row and its own count was always
    /// `0` — measured with one genuinely open proposal in
    /// `docs/notes.md`'s history. One raiser, like one writer: this is that
    /// raiser's own count, carried out through the report rather than
    /// re-derived by a second call.
    pub raised: usize,
    /// Members whose root was deleted by a concurrent [`crate::scan::remove_root`]
    /// while this derive was already running, between the group this pass
    /// built in memory and [`write_roots`] writing it back.
    ///
    /// **Whole-branch review, Important 1.** `derive_with` holds no
    /// transaction of its own (see this module's own doc on
    /// [`fn@derive_with`]), so under WAL a root removal that commits mid-pass
    /// leaves a member's `Candidate::root_id` naming a row that is already
    /// gone — `write_roots`'s own `UPDATE` used to let SQLite's foreign key
    /// on `member.root_id` refuse that outright, failing the whole derive
    /// with a bare "FOREIGN KEY constraint failed" over a race nobody could
    /// see coming. `write_roots` now catches exactly that constraint,
    /// clears the column to `NULL` instead — the identical state
    /// `schema/006_member_root.sql`'s own `ON DELETE SET NULL` already gave
    /// every other member of the removed root — and counts it here rather
    /// than either crashing the pass or losing the event, for
    /// [`Self::suppressed`]'s reason. A member counted here is not stranded:
    /// it reads exactly as an imported member with no root on record does
    /// (O54), and the next scan that finds it under a surviving root
    /// recovers it the ordinary way.
    pub root_removed_mid_derive: usize,
}

/// Whether a derivation reconciles `<root>/.taiyaku/` or leaves the tree
/// alone.
///
/// **Two call sites, and both of them write.** `derive` calls
/// [`crate::hand::reconcile`] per root, which writes a missing document back
/// where the root keeps them, and `write_work` calls
/// [`crate::hand::rename_for_rekey`], which renames a document when a work's
/// `content_key` moves and is gated on nothing at all — a rekey stranding a
/// document is the defect it exists to prevent (#101).
///
/// **[`Documents::Skip`] suppresses writes, never reads.** `hand::load` stays
/// on: a hand-set title is part of the structure a preview claims to predict,
/// and loading it is what makes the preview's answer the same answer a save
/// would produce. Skipping the read instead would make the preview lie about
/// every work anyone has ever corrected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Documents {
    /// Reconcile `<root>/.taiyaku/` as a real derivation does.
    Reconcile,
    /// Touch no file under any root. Reads stay on.
    Skip,
}

/// [`derive_with`] under [`Documents::Reconcile`] — what a real derivation is.
///
/// Kept as its own function rather than a defaulted parameter because 133 call
/// sites read it, all but a handful of them tests, and none of them has an
/// opinion about documents.
///
/// # Errors
///
/// As [`derive_with`].
pub fn derive(conn: &Connection) -> Result<DeriveReport, String> {
    derive_with(conn, Documents::Reconcile)
}

/// Turn every `scan_entry` this library holds into series, works, editions and
/// members, through the rules.
///
/// Idempotent by construction: series are reconciled on `sort_title` and works
/// on `(series_id, volume)`, so running this twice over unchanged facts writes
/// the same rows a second time rather than a second set of rows.
///
/// **No transaction of its own — but everything under it can still be
/// wrapped in one.** [`crate::import::scanned`] opens one per work and
/// [`crate::fold::absorb`] opens one per fold, and both do it through
/// [`crate::Nested`] rather than `unchecked_transaction` directly: a plain
/// `BEGIN` — the same constraint [`crate::import::replace_alignments`]
/// records — is refused outright inside an already-open transaction, where
/// `Nested` instead nests as a `SAVEPOINT`. Called plainly, as every caller
/// but [`crate::preview::preview`] calls it, nothing is open yet, so `Nested`
/// takes the real-`BEGIN` branch at both sites and a derive is atomic per
/// work and not across the library — the right grain for that case: a
/// library that half-derives has half its works current, not none of them.
///
/// **`preview` is the one caller that needs the library-wide grain, and gets
/// it without this function changing at all.** It wraps a whole `derive_with`
/// pass in a transaction of its own, so it can roll every write back
/// together; the per-work and per-fold `Nested` scopes above then take the
/// `SAVEPOINT` branch instead, nesting inside that outer transaction rather
/// than refusing it. See that type's own doc.
///
/// `documents` governs whether this pass writes under `<root>/.taiyaku/` at
/// all — see [`Documents`]. [`fn@derive`] is this function under
/// [`Documents::Reconcile`], which is what every caller but the rule-editor
/// preview wants.
///
/// # Errors
///
/// Returns an error if the rules or the roots cannot be read, if a `scan_entry`
/// row will not project, or if SQLite refuses one of the statements below. A
/// rule whose pattern does not compile is **not** one of those: `Engine::load`
/// returns it as a complaint and the rule matches nothing, so one malformed
/// pattern cannot leave a library underived. Neither is a file that has gone
/// or a container that will not open; see [`DeriveReport`].
pub fn derive_with(conn: &Connection, documents: Documents) -> Result<DeriveReport, String> {
    // Once, for the whole library. `Engine::compile` is where every regex in
    // the rule set is compiled, and compiling per entry would recompile the
    // same handful of patterns for every file in a library of thousands.
    //
    // The complaints are dropped here rather than propagated: a malformed
    // pattern disables its own rule and nothing else, and the caller that
    // reports them to a person is `library scan`, which loads the engine
    // itself. Surfacing them a second time from here would mean two callers
    // deciding how loudly to say the same thing.
    let (engine, _complaints) = Engine::load(conn)?;
    let roots = crate::scan::roots(conn)?;

    let mut report = DeriveReport::default();
    let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
    let mut chapter_dirs: BTreeSet<(i64, String)> = BTreeSet::new();
    let mut named_series: BTreeSet<String> = BTreeSet::new();

    // **Two passes over each root's entries, and the second is what stops a
    // volume being counted twice.** The fact layer records the container
    // directory *and* everything beneath it, so "each entry is one member"
    // makes a volume split into chapter subdirectories produce a member for
    // the volume and one per chapter — `docs/sidecar.md` records seven of
    // fifty-seven corpus archives in that layout. The first pass resolves
    // every entry once and notes which of them speak for their subtrees; the
    // second drops any whose ancestor does. See [`nested_in_claim`].
    for root in &roots {
        let mut seen = Vec::new();
        for entry in entries(conn, root.id)? {
            let Some(admitted) = admit(&engine, root, entry, &mut report) else {
                continue;
            };
            seen.push(admitted);
        }
        let claims = claims_of(&seen);
        for admitted in seen {
            // Counted, never merely skipped, and from both directions. See
            // `DeriveReport::suppressed`: a rule set that made a series
            // directory speak for its volumes would swallow every one of
            // them, and a report with no number for that reads exactly like a
            // library that holds nothing.
            if nested_in_claim(&claims, &admitted.entry.rel_path)
                || admitted.produces(&claims) == Produces::Deferred
            {
                report.suppressed += 1;
                continue;
            }
            classify(
                conn,
                root,
                admitted,
                &mut Sorting {
                    groups: &mut groups,
                    chapter_dirs: &mut chapter_dirs,
                    named_series: &mut named_series,
                    report: &mut report,
                },
            )?;
        }
    }
    report.unattached_chapter_dirs = chapter_dirs.len();

    // Every series that produced *anything* gets a row, including one holding
    // only unattached chapters: `proposal.subject_id` references `series(id)`,
    // so Task 8's `attach-chapters` proposal has nothing to point at
    // otherwise.
    let mut series_ids: BTreeMap<String, i64> = BTreeMap::new();
    for title in named_series {
        let id = reconcile_series(conn, &title)?;
        series_ids.insert(title, id);
    }
    report.series = series_ids.len();

    let mut written: BTreeSet<i64> = BTreeSet::new();
    // `(work_id, edition key)` pairs this pass actually wrote — narrower than
    // `written` alone, because a written work can still be holding an edition
    // this pass never touched (`DeriveReport::kept_editions`). `superseded`
    // needs the narrower set: only these rows count as evidence that a
    // candidate's material has a home. `group.editions.keys()` is exactly
    // `doc.editions.keys()` below in `write_work`, which is what
    // `import::scanned`'s `Scope::Scan` arm actually writes into `edition`.
    let mut written_editions: BTreeSet<(i64, String)> = BTreeSet::new();
    for group in groups.into_values() {
        let series_id = *series_ids
            .get(&group.series)
            .ok_or_else(|| format!("no series row was reconciled for `{}`", group.series))?;
        let work_id = write_work(conn, series_id, &group, documents, &mut report)?;
        written_editions.extend(group.editions.keys().cloned().map(|key| (work_id, key)));
        written.insert(work_id);
    }

    // **The only place `derive` itself reaches a `DELETE FROM work`** — the
    // other is `scan::apply_removal`'s, which deletes the works a removed
    // root alone accounted for before it calls back into this same
    // `derive_with`. `superseded` decides *which* work the fold takes and
    // `fold::absorb` decides whether it can be taken safely — the reader's
    // rows moving onto the target first, in one transaction, with the delete
    // last. A pair it refuses is counted (`DeriveReport::unfolded_works`) and
    // left standing, never propagated: a scan that died because one work of
    // thousands could not be folded would be worse than a scan that folds
    // nothing.
    //
    // The refusal's own message travels out in `DeriveReport::unfoldable`
    // rather than being dropped for the count beside it: it already names the
    // work and the keys, and a count alone leaves an operator with "1
    // superseded work left standing" over a library of thousands.
    // **Which work absorbed the one whose document is about to become an
    // orphan** — read here, before `absorb` runs, because `absorb`'s own
    // `DELETE FROM work` removes the `from_work` row itself: `hand::path_for`
    // joins through `work`, `edition`, `member` and `root` to resolve an
    // address (it never reads `migrated_document`), and once that row is gone
    // there is nothing left for it to join through. Captured after the
    // delete instead, `path_for` would find no `from_work` to query at all.
    //
    // `migrated_document`'s own `ON DELETE CASCADE` is a separate mechanism,
    // not this one: it is what makes the file an orphan in the first place,
    // by taking `from_work`'s marker row with it so `reconcile`'s `owed`
    // query stops naming it — see the "After everything" comment below,
    // where that cascade is the reason this whole loop must run after the
    // fold rather than before it.
    //
    // This does not move, copy or merge the document itself — see
    // [`crate::hand::Absorbed`] for why a fold is not allowed to decide that
    // on its own — it only remembers where to point a person who reads the
    // orphan report.
    let mut absorbed_into: HashMap<PathBuf, crate::hand::Absorbed> = HashMap::new();
    for (from_work, into_work) in superseded(conn, &written, &written_editions)? {
        let document_path = crate::hand::path_for(conn, from_work)?;
        let target: Option<(String, String)> = conn
            .query_row(
                "SELECT slug, title FROM work WHERE id = ?1",
                [into_work],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("reading the absorbing work {into_work}: {e}"))?;
        match crate::fold::absorb(conn, from_work, into_work)? {
            crate::fold::Outcome::Folded(moved) => {
                report.emptied_works += 1;
                report.moved_positions += moved.positions;
                report.lost_positions += moved.lost_positions;
                report.moved_notes += moved.notes;
                report.dropped_notes += moved.dropped_notes;
                report.moved_truth_pages += moved.truth_pages;
                report.moved_truth_collisions += moved.collisions;
                report.discarded_rows += moved.discarded;
                report.uncarried_alignments += moved.uncarried_alignments;
                // Only a completed fold orphans anything — a refused one
                // deletes nothing, per `Outcome::Refused`'s own doc. A path
                // of `None` means `from_work` had no document to strand
                // (an imported work, or one nobody ever hand-edited); a
                // target of `None` cannot happen for a work `superseded`
                // just named as a fold target, but is handled rather than
                // asserted away, matching this function's own rule against
                // returning `Err` for anything this task touches.
                if let (Some(path), Some((slug, title))) = (document_path, target) {
                    absorbed_into.insert(path, crate::hand::Absorbed { slug, title });
                }
            }
            crate::fold::Outcome::Refused(why) => {
                report.unfolded_works += 1;
                report.unfoldable.push(why);
            }
        }
    }

    // After the folds, because a fold is what empties a series — but over the
    // whole library rather than over the works just folded, for the reason
    // `DeriveReport::series_holding_no_work` gives.
    report.series_holding_no_work = conn
        .query_row(
            "SELECT count(*) FROM series s
              WHERE NOT EXISTS (SELECT 1 FROM work w WHERE w.series_id = s.id)",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
        .map_err(|e| format!("counting series left with no works: {e}"))?;

    // Task 8: raised last, because every kind of proposal points at a series
    // row and this is the point at which every series this derive touched —
    // including one holding only unattached chapters — is guaranteed to have
    // one. `proposals::raise` is idempotent, so a rescan over unchanged facts
    // opens nothing a person has already dismissed or accepted.
    //
    // **The one call, and its count kept.** A second call anywhere else —
    // `library scan` used to make one, to get a number to print — sees every
    // proposal this call just opened already sitting in `proposal`, and
    // `INSERT OR IGNORE` reports zero new rows for every one of them. See
    // `DeriveReport::raised`.
    report.raised = proposals::raise(conn)?;

    // **After everything — which is two different constraints, not one.**
    //
    // The first is `write_roots`, above: `hand::path_for` resolves an address
    // through `member.root_id`, which is still NULL on a work this pass has
    // just minted until `write_roots` has run for it — the same ordering the
    // read-back's own comment records, and getting it wrong here would report
    // every work in a new library as having no address rather than as
    // reconciled.
    //
    // **The second is the fold above, and it is not about addressing at
    // all.** `migrated_document.work_id` is `ON DELETE CASCADE`, and
    // `fold::absorb`'s `DELETE FROM work` — reached through `superseded` just
    // above — is one of two places this project ever takes that cascade; the
    // other is `scan::apply_removal`'s own `DELETE FROM work`, which fires it
    // just the same whenever a doomed work carries a `migrated_document` row.
    // Either way — folded away here, or deleted with its root there — the
    // instant the work's row goes, its `migrated_document` row goes with it,
    // so `reconcile`'s `owed` query (which reads that table) stops naming it.
    // Run this loop *before* the fold instead, and the folded work's row is
    // still there: its address is still in `expected`, its file is still
    // `present`, the two agree, and nothing is reported — the document only
    // surfaces as an orphan on some later pass, whenever something else next
    // disturbs the work it was folded into. Run after, as below, and the same
    // pass that deletes the row is the pass that reports the document
    // address-less. `folding_a_work_away_orphans_its_hand_document_on_the_same_pass`
    // pins this: moving this loop up to just after the `for group in
    // groups.into_values()` write loop — still after `write_roots` — turns
    // that assertion red while leaving every other test in this module green.
    // **Skipped whole, not per-root.** `reconcile` writes (gap repair), renames
    // and reports; a preview wants none of the three, and the report fields it
    // populates would be artefacts of a derivation nobody asked to keep.
    if documents == Documents::Reconcile {
        for root in &roots {
            let found = crate::hand::reconcile(conn, root)?;
            report.repaired_documents += found.repaired;
            report.unwritable_documents.extend(found.unwritable);
            report.orphan_documents.extend(found.orphans);
            report.declined_documents.extend(found.declined);
            report.stale_documents.extend(found.stale);
        }
    }

    // **Correlated here, not inside `reconcile`.** `reconcile` is a pure
    // per-root function that never sees a fold; `derive` is the one place
    // that watches both the sweep just above and the fold loop before it, so
    // it is the only place this join can be made at all. A path captured in
    // `absorbed_into` that matches no orphan is the ordinary case, not a
    // gap: a work nobody ever hand-edited has no document to strand, so most
    // folds add nothing here.
    for orphan in &mut report.orphan_documents {
        if let Some(absorbed) = absorbed_into.get(&orphan.path) {
            orphan.absorbed_into = Some(absorbed.clone());
        }
    }

    Ok(report)
}

/// A group's identity: the series title, the volume number as its bit
/// pattern, and — where there is no volume — the `rel_path` that keeps the
/// entry to itself.
///
/// `f64` is neither `Ord` nor `Hash`, and a volume is one — `work.volume` is
/// `REAL` because, in that column's own words, "omnibus and half-volumes
/// exist". The bits are used for grouping only, never for ordering anything a
/// person sees, so the one place they differ from numeric order (negatives)
/// does not reach anything: the real volume is carried alongside in [`Group`]
/// and it is that value the `work` row and the hub lookup are given.
///
/// **The third element is C4's fix, and `None` is not a group.** An entry the
/// rules gave neither a volume nor a chapter has nothing to be grouped *by*,
/// and keying those on `(series, NULL)` merged every one of them in a series
/// into a single page sequence: `ONE PIECE/001..004` — bare numbers, which
/// `volume_number_marked` deliberately refuses — became one work of four
/// members and 18 pages, and `Series/Omake` beside `Series/Bonus` became one
/// work called `Bonus`. Measured against the shipped binary. Merging distinct
/// material into one page sequence is the failure [`write_work`]'s own doc
/// calls out, since member order is page order and every alignment offset
/// derives from it.
///
/// `rel_path` rather than the absolute path, so one volume-less work present
/// in two roots is still one work — which is what
/// `a_volume_less_work_still_matches_itself_on_the_next_derive` pins, and the
/// same reason [`Candidate::rel_path`] exists for ordering.
///
/// Entries that carry a chapter *and* a volume still group by that volume,
/// which is §7's chapter case and stays; an entry with a chapter and no
/// volume never reaches here at all ([`classify`] settles it first).
type GroupKey = (String, Option<u64>, Option<String>);

/// One `scan_entry` row, as derivation reads it.
struct Entry {
    rel_path: String,
    format: Option<String>,
    pages: Option<u32>,
    sha256: Option<String>,
}

/// A resolved entry that will become one member.
struct Candidate {
    /// **Absolute.** A member path in a `taiyaku.json` is relative to that
    /// document, and a scanned work has no document — `import::scanned` leaves
    /// `sidecar_path` NULL by design, since one `taiyaku.json` per work
    /// directory cannot describe fourteen volumes sitting beside each other.
    /// So the only path that resolves for every reader of these rows is the
    /// one from the root down. `taiyaku-serve`'s `root.join(&member.path)`
    /// keeps working unchanged, because joining an absolute path replaces the
    /// base.
    path: String,
    /// What member order is computed on. The absolute path would sort two
    /// roots' copies of one volume by which root they came from before by
    /// what they are called.
    rel_path: String,
    /// The root this entry was walked from.
    ///
    /// **A member has one; a work does not.** The `Group` below collects
    /// entries across every root, so this is the only level at which the
    /// question has a single answer — see `member.root_id`'s own comment in
    /// migration 6, and spec C1 §5.1.
    root_id: i64,
    sha256: String,
    pages: u32,
    chapter: Option<f64>,
    title: Option<String>,
    /// What the rules resolved for `edition.kind` on **this entry's own**
    /// path, unvalidated: `rule.set_kind` is a bare `TEXT` column while
    /// `edition.kind` is a closed set, and one edition can gather entries
    /// that resolved differently. [`write_edition_metadata`] settles both
    /// questions, because both are questions about an edition rather than
    /// about a member.
    kind: Option<String>,
    /// What the rules resolved for `edition.language` on this entry's own
    /// path. Free text in the schema, so there is nothing to validate it
    /// against — only the same disagreement to settle.
    language: Option<String>,
}

/// One work, before it is a work: every entry across every root that resolved
/// to the same `(series, volume)`, split by edition key.
struct Group {
    series: String,
    volume: Option<f64>,
    editions: BTreeMap<String, Vec<Candidate>>,
}

/// The four things [`classify`] accumulates into, gathered so it takes one
/// `&mut` rather than four.
///
/// `clippy::too_many_arguments` is on by default rather than merely pedantic
/// and refuses at eight, which is exactly what `classify` would take; a struct
/// of borrows is the shape `taiyaku-cli`'s `AlignWrite` already uses for the
/// same refusal, with shared borrows where these are mutable.
struct Sorting<'a> {
    groups: &'a mut BTreeMap<GroupKey, Group>,
    chapter_dirs: &'a mut BTreeSet<(i64, String)>,
    named_series: &'a mut BTreeSet<String>,
    report: &'a mut DeriveReport,
}

/// Every `scan_entry` under one root, in path order.
fn entries(conn: &Connection, root_id: i64) -> Result<Vec<Entry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT rel_path, format, pages, sha256 FROM scan_entry \
             WHERE root_id = ?1 ORDER BY rel_path",
        )
        .map_err(|e| format!("reading the facts for root {root_id}: {e}"))?;
    let rows = stmt
        .query_map([root_id], |r| {
            Ok(Entry {
                rel_path: r.get(0)?,
                format: r.get(1)?,
                pages: r.get(2)?,
                sha256: r.get(3)?,
            })
        })
        .map_err(|e| format!("reading the facts for root {root_id}: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading the facts for root {root_id}: {e}"))
}

/// One entry, resolved, if it is something derivation could make structure
/// out of at all — readable bytes, and a series to belong to.
///
/// `Resolved::series` is moved out into its own field rather than left as an
/// `Option`: past this point it is decided, and the rest of the module should
/// not have to keep asking.
struct Seen {
    entry: Entry,
    series: String,
    resolved: crate::rules::Resolved,
}

/// What one admitted entry makes of itself, once what is beneath it is known.
///
/// The four outcomes are exhaustive and each has a consequence: two of them
/// speak for the subtree (so nothing under them is looked at again), and two
/// of them are counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Produces {
    /// One member of one work. Speaks for its subtree.
    Member,
    /// §7's unattached chapter directory, and no member at all. Speaks for
    /// its subtree too: whatever decision it is waiting for covers what is
    /// inside it, which is why a chapter directory inside another one is not
    /// a second question for §6's inbox.
    ChapterDirectory,
    /// Nothing, because something *inside* it does. A container of works is
    /// not itself a work. Counted as [`DeriveReport::suppressed`], and no
    /// material is lost: what it holds is described by the entries beneath
    /// it.
    Deferred,
    /// Nothing, because there is nothing there: no pages, or a container the
    /// walk could not list. [`classify`] counts it as
    /// [`DeriveReport::unreadable`].
    Nothing,
}

impl Seen {
    /// What this entry produces, given the entries beneath it that produce
    /// something — see [`Produces`].
    ///
    /// **"What will this entry produce", not "could it be a member".** Those
    /// came apart on `Series/Extras/Art Book`: the test used to be "a volume
    /// or a chapter", per Step 7b's rule, while [`classify`] admitted a
    /// volume-less entry as a member anyway — so the container and its
    /// contents both became members of one work, five pages counted as ten,
    /// and `suppressed` said `0`. Three levels gave three members
    /// (whole-branch review, C3; measured against the shipped binary).
    ///
    /// The order of the tests is the whole rule:
    ///
    /// 1. A chapter with no volume is §7's case, settled before pages are
    ///    looked at because it needs none.
    /// 2. No pages, no anything. This is the other half of C1: a directory
    ///    holding archives rather than loose images gets `pages = Some(0)`
    ///    from the walk, and `builtin-volume` reads `v01` out of its *name*
    ///    regardless — so the page-less directory suppressed the archive that
    ///    actually held the pages, and `member_digest` over it is the digest
    ///    of nothing, the same hash for every such directory, so the
    ///    surviving groups collided on `content_key` and landed on one row.
    /// 3. **A volume-less container defers to whatever is inside it.** A rule
    ///    marked no volume here, so this entry is a name in a path and not a
    ///    thing anybody released; if something beneath it is a member, that
    ///    is what the material is. Without this clause the first fix for C3
    ///    took the *container* as the member and dropped everything under it
    ///    — measured over this project's own corpus as 107 members becoming
    ///    40, with `incoming/bocchi`'s five editions collapsed into one
    ///    439-page member and `incoming/onepiece`'s six into one of 630. That
    ///    is precisely the failure C4 was settled to stop: member order is
    ///    page order, so one member of five editions is five works
    ///    concatenated.
    /// 4. Otherwise a member — including a marked volume, which speaks for
    ///    its own chapter subdirectories even though they are members'
    ///    material. That is Step 7b, and it is the one place a container
    ///    *does* win: `Series - v01/c001` is a piece of `v01`, not a work
    ///    beside it, and the rules said so by resolving a volume.
    ///
    /// **A series is required too**, which the old wording left implicit: an
    /// entry with no series has no group to join and can never be a member.
    /// Unreachable here in any case — [`admit`] has already refused one.
    ///
    /// **What this does not consult is the digest**, which is the one place
    /// it is coarser than "became a member": an entry whose bytes have gone
    /// between the walk and now still speaks for its subtree. That is
    /// deliberate. It keeps this a question about the *facts* — rules and
    /// page counts, the two things [`unattached_chapter_dirs`] also has — so
    /// that function can ask it rather than reimplement a near-miss of it,
    /// and an unmounted share reports one missing volume rather than a
    /// missing volume and every chapter directory inside it.
    fn produces(&self, beneath: &BTreeSet<String>) -> Produces {
        if self.resolved.volume.is_none() && self.resolved.chapter.is_some() {
            return Produces::ChapterDirectory;
        }
        if !self.entry.pages.is_some_and(|pages| pages > 0) {
            return Produces::Nothing;
        }
        if self.resolved.volume.is_none() && any_beneath(beneath, &self.entry.rel_path) {
            return Produces::Deferred;
        }
        Produces::Member
    }

    /// Whether this entry speaks for everything beneath it, which is the test
    /// [`nested_in_claim`] applies to ancestors.
    fn claims_subtree(&self, beneath: &BTreeSet<String>) -> bool {
        matches!(
            self.produces(beneath),
            Produces::Member | Produces::ChapterDirectory
        )
    }
}

/// Whether `claims` holds anything **strictly** beneath `rel_path`.
///
/// A range over the `rel_path/` prefix rather than a scan: a library is
/// hundreds of thousands of entries and this is asked once per admitted one.
///
/// The root's own `rel_path` is `""`, which has no prefix form — everything
/// is beneath it — so it is its own arm rather than a `"/"` that would match
/// nothing. **Strictly** is what that arm has to be careful about: the root's
/// own entry is in this set too once it has claimed, and `!claims.is_empty()`
/// therefore made the root defer to itself. Measured as a root of loose page
/// images deriving to nothing at all, with one suppressed entry to show for
/// it. It is also what keeps [`claims_of`]'s two passes agreeing: the
/// non-empty arm cannot match `rel_path` itself, because a path does not
/// start with itself followed by a separator, and this arm must not either.
fn any_beneath(claims: &BTreeSet<String>, rel_path: &str) -> bool {
    if rel_path.is_empty() {
        return claims.iter().any(|held| !held.is_empty());
    }
    let prefix = format!("{rel_path}/");
    claims
        .range(prefix.clone()..)
        .next()
        .is_some_and(|first| first.starts_with(&prefix))
}

/// Which entries speak for their subtrees, decided **deepest first** so that
/// [`Produces::Deferred`] can be answered at all: a container's outcome
/// depends on its descendants' outcomes, and `rel_path` order puts every
/// ancestor before its descendants, so this walks it backwards.
///
/// The answer is the same when [`Seen::produces`] is asked again over the
/// finished set — every descendant of an entry is decided before that entry
/// is, and nothing is ever removed — which is why [`derive`]'s second pass
/// re-asks rather than carrying a second structure of outcomes alongside.
fn claims_of(seen: &[Seen]) -> BTreeSet<String> {
    let mut claims: BTreeSet<String> = BTreeSet::new();
    for admitted in seen.iter().rev() {
        if admitted.claims_subtree(&claims) {
            claims.insert(admitted.entry.rel_path.clone());
        }
    }
    claims
}

/// Resolve one entry, or refuse it: unreadable bytes are counted, and an entry
/// no rule gave a series to is passed over in silence.
///
/// The root's own row and every container directory above a member land in
/// that second case under the seeded rules, which is why the fixture's seven
/// entries produce two works rather than seven.
fn admit(engine: &Engine, root: &Root, entry: Entry, report: &mut DeriveReport) -> Option<Seen> {
    if !is_readable(entry.format.as_deref()) {
        report.unreadable += 1;
        return None;
    }
    let mut resolved = engine.resolve(root, &entry.rel_path);
    let series = resolved.series.take()?;
    Some(Seen {
        entry,
        series,
        resolved,
    })
}

/// Whether any ancestor of `rel_path` already speaks for it — see
/// [`Seen::claims_subtree`].
///
/// **Step 7b's rule: a directory is a member only if nothing above it is.**
/// The fact layer records a container and its children alike, so without this
/// a volume split into chapter subdirectories produces a member for the volume
/// *and* one per chapter — the volume's pages counted twice over, and three
/// chapters made editions of nothing. `inventory::decide` settles the same
/// question with a page threshold, which derivation has no equivalent of; the
/// rule layer gives a better one, and it needs nothing but `rel_path`
/// prefixes.
///
/// A series directory holding volume directories claims nothing — the walk
/// gives it a recursive page total, so it *would* claim, but [`admit`] never
/// gets that far: seeded rule 1 needs a `/` in the path to name a series, and
/// a first-level directory has none. A volume directory does claim, so it
/// suppresses its own chapter subdirectories, whether those resolve a chapter
/// alone (§7's case, which would otherwise report a perfectly attached volume
/// as unattached) or a volume *and* a chapter, which is the layout that
/// actually double-counts: `Series - v01/Series - v01 - c001` joins the very
/// group its parent forms.
///
/// **The root's own entry — `rel_path` `""` — is walked to like any other
/// ancestor, and is reachable.** An earlier version of this comment argued it
/// was not: [`admit`] refuses an entry with no series, seeded rule 1 needs a
/// `/` to give one, and no extractor reads a number out of the empty string.
/// The first clause is the whole of it, and any `set_series` rule broad
/// enough to match `""` satisfies it — a `prefix` rule with an empty pattern
/// is what `proposals::accept` writes. Measured: the root became a member,
/// the work it made was titled with the empty string, and every entry in the
/// library was counted a second time inside it. It now claims the subtree
/// like any other container, which collapses that library to one work and
/// says so through `suppressed` — the honest reading of a rule that says
/// every path is one series.
fn nested_in_claim(claims: &BTreeSet<String>, rel_path: &str) -> bool {
    let mut at = rel_path;
    while !at.is_empty() {
        at = parent_of(at);
        if claims.contains(at) {
            return true;
        }
    }
    false
}

/// File one admitted entry — as a member of some work, as one more reason a
/// directory is an unattached chapter directory, or as nothing at all.
///
/// The order of the tests below is load-bearing. The chapter case is settled
/// **before** anything is hashed, because §7's answer for it is that it
/// produces no work — so paying a digest for it would be paying for a member
/// that is never written, on every entry of a 146-chapter directory.
fn classify(
    conn: &Connection,
    root: &Root,
    seen: Seen,
    into: &mut Sorting<'_>,
) -> Result<(), String> {
    let Seen {
        entry,
        series,
        resolved,
    } = seen;

    // §7: which seven of 146 chapter files constitute volume 3 is a question
    // only the alignment answers. Count the directory and leave it visibly
    // unattached; `proposals::raise` is what asks about it. Chapters that sit
    // *inside* a volume never reach here — `nested_in_claim` dropped them,
    // because that question has already been answered for them.
    if resolved.volume.is_none() && resolved.chapter.is_some() {
        into.named_series.insert(series);
        into.chapter_dirs
            .insert((root.id, parent_of(&entry.rel_path).to_string()));
        return Ok(());
    }

    // A readable container the walk could not list — `pages_of` swallowed the
    // refusal so one truncated download would not leave a library unscanned,
    // and `member.pages` is NOT NULL, so there is nothing honest to write.
    //
    // **And a container that listed cleanly and held no pages**, which is not
    // the same fact and reaches the same answer: a volume directory holding
    // `.cbz` files has `pages = Some(0)`, and a member of no pages is a
    // member of nothing — `member_digest` hashes it to the digest of nothing,
    // which is the same hash for every such directory, so writing it would
    // collide every one of them onto one work. The archives *inside* it are
    // the members, and [`Seen::claims_subtree`] is what stops this entry
    // suppressing them.
    let Some(pages) = entry.pages.filter(|pages| *pages > 0) else {
        into.report.unreadable += 1;
        return Ok(());
    };
    let Some(sha256) = digest(conn, root, &entry)? else {
        into.report.unreadable += 1;
        return Ok(());
    };

    into.named_series.insert(series.clone());
    let volume = resolved.volume;
    // No volume here means no chapter either — the branch above returned for
    // that case — so this is the entry the rules placed in a series and
    // nothing more. It is keyed to itself. See [`GroupKey`].
    let alone = if volume.is_none() {
        Some(entry.rel_path.clone())
    } else {
        None
    };
    let key = (series.clone(), volume.map(f64::to_bits), alone);
    let group = into.groups.entry(key).or_insert_with(|| Group {
        series,
        volume,
        editions: BTreeMap::new(),
    });
    let edition = group
        .editions
        .entry(
            resolved
                .edition_key
                .unwrap_or_else(|| DEFAULT_EDITION_KEY.to_string()),
        )
        .or_default();
    // Counted here rather than from the finished group, because "how many
    // entries joined something" and "how many members an edition ended with"
    // differ by one per edition and this is the number that means something:
    // one entry joining another's page sequence.
    if !edition.is_empty() {
        into.report.merged += 1;
    }
    edition.push(Candidate {
        path: root
            .path
            .join(&entry.rel_path)
            .to_string_lossy()
            .into_owned(),
        rel_path: entry.rel_path.clone(),
        root_id: root.id,
        sha256,
        pages,
        chapter: resolved.chapter,
        title: resolved.title,
        kind: resolved.kind,
        language: resolved.language,
    });
    Ok(())
}

/// Whether `taiyaku-read` can turn what this `scan_entry.format` token names
/// into pages.
///
/// Compared as tokens rather than by rebuilding the enum, because
/// [`taiyaku_read::Format`] has no parse-back and `token` is explicitly the
/// schema's vocabulary rather than free text — the same move `scan::pages_of`
/// makes against `Format::Zip.token()`. The two named here are exactly
/// [`taiyaku_read::Format::is_readable`]'s two, and that function stays the
/// place the question is decided; a third readable format is a change in both
/// places, which is the cost of there being no token parser.
///
/// A NULL `format` — a file whose bytes matched no signature — needs no arm
/// of its own: neither token is `None`, so `None` matches neither, which is
/// the same answer `Format::Unknown` gives.
fn is_readable(format: Option<&str>) -> bool {
    format == taiyaku_read::Format::Dir.token() || format == taiyaku_read::Format::Zip.token()
}

/// This member's digest, hashing it and writing the answer back onto
/// `scan_entry` if nothing has yet.
///
/// **`Ok(None)` is the file having gone**, not an error. An entry nothing has
/// ever hashed, whose bytes are no longer there to hash, is exactly what an
/// unmounted share and a deleted tree both look like, and neither must leave
/// the rest of the library underived. The count is the caller's.
///
/// The write-back is the whole of §2's claim in one statement: it is why the
/// *next* derive over this entry opens no file, and why editing a rule
/// re-derives a library that is no longer on disk.
fn digest(conn: &Connection, root: &Root, entry: &Entry) -> Result<Option<String>, String> {
    if let Some(held) = &entry.sha256 {
        return Ok(Some(held.clone()));
    }
    let Ok(hash) = taiyaku_read::member_digest(&root.path.join(&entry.rel_path)) else {
        return Ok(None);
    };
    conn.execute(
        "UPDATE scan_entry SET sha256 = ?1 WHERE root_id = ?2 AND rel_path = ?3",
        rusqlite::params![hash, root.id, entry.rel_path],
    )
    .map_err(|e| format!("recording the digest of {}: {e}", entry.rel_path))?;
    Ok(Some(hash))
}

/// The `rel_path` of the directory holding `rel_path`, or `""` for something
/// directly under the root — which is the root's own `rel_path`, so the two
/// agree rather than needing a separate case.
///
/// `pub(crate)` for `proposals::common_directory_pattern`, which needs the
/// exact same answer to derive `accept`'s rule pattern — this module's own
/// doctrine, on [`unattached_chapter_dirs`] and [`rel_paths_for_series`], is
/// that a second implementation of a two-line question is still a second
/// implementation, and it is how two mechanisms come to disagree.
pub(crate) fn parent_of(rel_path: &str) -> &str {
    match rel_path.rfind('/') {
        Some(at) => &rel_path[..at],
        None => "",
    }
}

/// The `series` row for `sort_title`, reconciled rather than re-minted.
///
/// **`proposal.subject_id` points at these rows** — `003_scanner.sql` says so
/// in as many words — so deleting and re-inserting a series on every derive
/// would orphan every dismissal a person has made, and §6's "a dismissal has
/// to persist" would quietly stop being true.
///
/// **`sort_title` is the name the files gave the series, and a correction does
/// not move it — O114.** `librarydoc::set_series_titles` and `apply_series`'s
/// matched branch leave it alone. When either moved this column, the next pass
/// found nothing under the scanned name, minted a second series and moved every
/// volume onto it.
///
/// **A `SELECT` then an `INSERT`, not `INSERT … ON CONFLICT DO NOTHING`.**
/// There is no unique constraint on `series.sort_title` — `001_initial.sql`
/// declares the column bare, and migrations here are append-only — so an
/// upsert would find no conflict to do nothing about and would insert a
/// duplicate row on every single derive, silently, while every query that
/// reads one back still found the first. The race that opens between the two
/// statements is not closed and does not need to be: `Library` hands out one
/// connection behind a `Mutex`, and the only other writer of this table
/// (`librarydoc::apply`) reaches it through the same one.
///
/// The `series_title` row is not decoration. `librarydoc::find_series_by_title`
/// looks a series up by it, and `series_fts` is fed from it by trigger, so a
/// `series` row without one is invisible to both — and a
/// `taiyaku.library.json` later applied over a scanned work would mint a
/// second series beside the derived one rather than finding it.
fn reconcile_series(conn: &Connection, sort_title: &str) -> Result<i64, String> {
    let held: Option<i64> = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = ?1",
            [sort_title],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("looking up the series `{sort_title}`: {e}"))?;
    let series_id = if let Some(id) = held {
        id
    } else {
        conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [sort_title])
            .map_err(|e| format!("inserting the series `{sort_title}`: {e}"))?;
        conn.last_insert_rowid()
    };
    // **Only onto a series holding no title — O114.** A correction through
    // `librarydoc::set_series_titles` replaces the whole set and may drop this
    // text; writing it back on every pass returned a removed spelling as a
    // second primary title. A series minted just above holds none.
    conn.execute(
        "INSERT INTO series_title (series_id, title, is_primary)
         SELECT ?1, ?2, 1
          WHERE NOT EXISTS (SELECT 1 FROM series_title WHERE series_id = ?1)",
        rusqlite::params![series_id, sort_title],
    )
    .map_err(|e| format!("recording the title of series `{sort_title}`: {e}"))?;
    Ok(series_id)
}

/// Build one group's document and hand it to [`crate::import::scanned`], then
/// write the columns the document cannot carry — `work.series_id`,
/// `work.volume` and, through [`write_chapters`], `member.chapter_declared`.
///
/// It also reads back and applies a hand document sitting at this work's
/// address when nothing has absorbed it yet — the whole `LibraryDoc`: title,
/// volume, series row and titles, external ids, edition metadata, member
/// chapters, tags — which is what closes the guard below for the rest of this
/// call.
///
/// Returns the `work_id` it resolved — new or reconciled — so [`derive`] can
/// collect the set of works this pass wrote and hand it to [`superseded`].
fn write_work(
    conn: &Connection,
    series_id: i64,
    group: &Group,
    documents: Documents,
    report: &mut DeriveReport,
) -> Result<i64, String> {
    let mut editions = BTreeMap::new();
    for (key, candidates) in &group.editions {
        let ordered = in_page_order(candidates);
        editions.insert(
            key.clone(),
            Edition {
                // Neither a root nor a rule carries a display name for an
                // edition — there is no `set_label` column — so the key is
                // the only honest label. C4's root editor is where a better
                // one would come from.
                label: key.clone(),
                members: ordered
                    .iter()
                    .map(|c| Member {
                        path: c.path.clone(),
                        sha256: c.sha256.clone(),
                        pages: c.pages,
                    })
                    .collect(),
                spreads: Vec::new(),
                credits: Vec::new(),
            },
        );
    }

    let held = held_work(conn, series_id, group)?;
    // Only a genuinely new work chooses one, and then the choice is arbitrary
    // but permanent: whichever key sorts first among the editions it has
    // today. **The hub is fixed when the work is created** — `identity.rs`'s
    // own doc says so, and it is load-bearing: `content_key` is a hash over
    // the hub edition's members, so a work whose hub changed would get a
    // different key, `import` would mint a second row, and the reader's
    // progress and notes would be orphaned on the first.
    //
    // "The key that sorts first" is stable only while the set does not change,
    // which is exactly why it is consulted once. The `.filter` arm is the
    // remaining case: if the edition that *was* the hub has gone from disk,
    // its members are gone with it and the old `content_key` cannot be
    // reproduced anyway, so a new hub is chosen and a new work minted. That is
    // the honest outcome rather than a recoverable one.
    let hub_key = held
        .as_ref()
        .map(|(_, hub, _)| hub.clone())
        .filter(|held| editions.contains_key(held))
        .unwrap_or_else(|| {
            editions
                .keys()
                .next()
                .cloned()
                .expect("a group exists only once a member has been pushed into it")
        });
    let title = title_of(group, &hub_key);

    report.editions += editions.len();
    report.members += editions.values().map(|e| e.members.len()).sum::<usize>();

    let doc = Sidecar {
        taiyaku: taiyaku_core::VERSION,
        work: Work {
            title,
            // Every edition in this corpus is manga, and neither `root` nor
            // `rule` carries a reading direction to set. A rule field for it is
            // spec C4's business; until something in the library reads
            // left-to-right this is a default rather than a guess, and it is
            // recorded here so the next person does not read it as one.
            reading: Direction::RightToLeft,
        },
        editions,
        hub: hub_key,
        // **Empty because a walk has nothing to say here, and empty no longer
        // means deleted.** Solving is `taiyaku align`'s, not the scanner's, so
        // this document declares no correspondence — and `import::scanned`
        // reads that as silence rather than as an answer: it leaves the work's
        // `alignment`, `run` and `page_flag` rows exactly as they were. Until
        // 2026-08-17 it replaced them wholesale from this empty list, so a
        // rescan after an `align --write` dropped every run, hand-corrected
        // ones included. See `import::scanned`'s own doc for the rule and for
        // what it costs — an edge naming an edition a later rule edit removed
        // dangles, resolvably rather than dangerously.
        alignments: Vec::new(),
    };
    // **Counted before the import, because afterwards the row already holds
    // the new key and the move is unobservable.** `content_key` is recomputed
    // here rather than returned from `import` — a sha256 over a handful of
    // hex digests, against leaving `import::scanned`'s signature answering two
    // questions instead of one.
    //
    // **`rekeyed_from` is captured in the same breath, for
    // [`crate::hand::rename_for_rekey`] near the end of this function.**
    // `held_key` is this row's `content_key` before `import::scanned`
    // overwrites it — the only place that value is still readable — so it is
    // saved here rather than re-derived from `work.content_key` after the
    // fact, which is what a caller with no memory of `held` would have to do.
    let mut rekeyed_from = None;
    if let Some((_, _, held_key)) = &held {
        if *held_key != crate::identity::content_key(&doc)? {
            report.rekeyed += 1;
            rekeyed_from = Some(held_key.clone());
        }
    }
    // The second identity probe: this work's address, for a work that has no
    // document to be addressed by. Without it, the changed files above would
    // mint a second row and leave the reader's progress on the first.
    let work_id = crate::import::scanned(conn, &doc, held.map(|(id, _, _)| id))?;
    // **Counted after the import, where `rekeyed` above had to be counted
    // before it.** The two are not the same shape: a `content_key` is
    // overwritten by the import, so the old value is gone once it returns,
    // while an edition it left standing is still standing — and the row it is
    // standing on is `work_id`, which content may have resolved to something
    // other than the `held` address probed above. Asking afterwards is
    // therefore both simpler and exact.
    report.kept_editions += kept_editions(conn, work_id, &doc)?;

    // **Moved ahead of the read-back below, and of both guards after it.**
    // `write_roots` only issues `UPDATE member SET root_id = ?` from the
    // group's own candidates, matched on `(edition key, ordinal)` —
    // `import::scanned`, just above, is the only thing either depends on, so
    // nothing here needs the guard or the read-back to have run first. The
    // read-back does need this to have run first: on a work being minted for
    // the first time, `member.root_id` is still NULL the moment
    // `hand::load` calls `hand::path_for`, whose
    // `JOIN root ON root.id = member.root_id` then matches nothing —
    // `path_for` answers `None`, `hand::load` answers `Ok(None)`, and a
    // correction sitting right beside the archives is silently skipped. A
    // *second* derive recovers it, because by then this call has already run
    // once. That is the store spec §4.4 claim broken: losing the database
    // was supposed to cost one rescan, not two.
    //
    // **`write_chapters` stays below, unmoved, and that is not an
    // oversight.** It looks like the same shape — another targeted update
    // keyed on `(edition key, ordinal)` from `import::scanned`'s own rows —
    // but it has a real ordering dependency the read-back's `apply` call
    // creates: `librarydoc::apply`'s `apply_members` clears *every* member's
    // chapter for this work unconditionally, then rewrites only the entries
    // the hand document names, by `sha256`. Running `write_chapters` before
    // that guard would have its scanner-derived chapters wiped right back to
    // NULL for every member the document does not mention. Running it after,
    // as it already does, lets it fill exactly those NULLs back in — its own
    // per-candidate guard, `chapter_source IS NULL OR chapter_source =
    // SCANNER_CHAPTER_SOURCE`, is what stops it from also overwriting a
    // member the document *did* set. `write_roots` has no such
    // interaction: nothing the guard or the read-back writes touches
    // `member.root_id`.
    write_roots(conn, work_id, group, report)?;

    // **The document follows the row here, after `write_roots` and after
    // `import::scanned`'s own transaction has already committed — never
    // inside it.** A rollback that left the file renamed would strand it
    // under a key the database never adopted: this module's own defect,
    // inverted, and harder to see, because `hand::reconcile` would then read
    // every root as clean.
    //
    // **Why here and not in `import_with`, where an earlier version of this
    // change placed it.** `hand::path_for` resolves a work's address through
    // `member.root_id`, and for the one member whose bytes just changed —
    // which, for a single-file volume, is every member the hub edition has —
    // `replace_editions`' delete-and-reinsert leaves that column NULL:
    // `held_member_meta` restores it by matching the old and new rows on
    // `sha256`, and a repacked archive's hash is exactly what changed.
    // `write_roots`, just above, is what restores it, by `(edition key,
    // ordinal)` instead — and it runs *after* `import::scanned` returns, so it
    // cannot have run yet at any point inside `import_with`. Calling the
    // rename there, as the first version of this change did, found
    // `path_for` answering `None` for `archived_volumes`' own fixture — a
    // work with exactly one member — and left the document exactly where
    // `a_repacked_volume_takes_its_document_with_it` catches it: unmoved,
    // reported as an orphan. Waiting until here is what lets one derive do
    // what the store spec §4.4 asks of it, rather than needing a second pass
    // the way a brand-new work's hand document already does, above.
    if let Some(old_key) = rekeyed_from {
        if documents == Documents::Reconcile {
            crate::hand::rename_for_rekey(conn, work_id, &old_key)?;
        }
    }

    // The `work` columns the sidecar format has no field for, which are also
    // the two the hub lookup above reads back — and, under the same guard and
    // for the same reason, `edition.kind` and `edition.language`, which the
    // format has no field for either. See [`write_edition_metadata`], which
    // is where that second pair's own argument for being here lives. Written
    // after the import rather than before, because a brand-new work has no id
    // until then.
    //
    // **Skipped when a hand document has actually been applied to this
    // work (I7).** `librarydoc::apply` — reached through `migrate::work`,
    // §4.4, or through the read-back below — is the documented mechanism for
    // setting `work.volume` and `work.series_id` by hand, and
    // `<stem>.library.json` exists precisely so that hand metadata survives a
    // re-derivation. Before this, `derive` wrote both unconditionally on
    // every pass: a hand-set `volume = 4.5` came back as `1.0` after one
    // derive, and a hand-renamed series minted a second row and moved the
    // works to it, leaving the renamed one empty. `migrate::absorbed(…,
    // LIBRARY)` is the exact question `migrate.rs` already answers for the
    // same class of "has a hand document actually been applied here"
    // decision — reused rather than a second implementation of it. **This is
    // no longer true of the marker alone**: the read-back below now arms it
    // itself, precisely because a `.library.json` file sits on disk with
    // nothing in `migrated_document` yet to say so.
    //
    // **Content matching still finds this row.** `import::scanned`'s own
    // `find_by_key` probes `content_key` before it ever falls back to the
    // address `held_work` computed above, and `content_key` is a hash over
    // the hub edition's members — untouched by a hand-set `series_id` or
    // `volume`. So a work whose hand metadata has moved its address away
    // from what the rules currently resolve is still reconciled onto the
    // same row by its content, exactly as an ordinary rescan is; only the
    // two columns below are left alone.
    //
    // **That design question is answered, and this guard no longer holds the
    // answer — O80, migration 011.** This paragraph used to end "a provenance
    // column, scoping this to works with no `sidecar_path`, and so on — is
    // still the design question I7 left open, and nothing here answers it".
    // The provenance column is what shipped: `volume_hand`, `series_id_hand`,
    // `language_hand`, `kind_hand`, `series_title.hand` and
    // `series.reading_hand`, with every writer below using
    // `COALESCE(<hand>, <what this pass derived>)`. So the marker no longer
    // decides whether these columns may be written — only whether the
    // document has already been read back, which is the block immediately
    // following.
    //
    // **The document on disk, read back before the guard is consulted.** A
    // fresh database has no marker for a work it has just minted, so without
    // this the guard below is open and derivation writes over a correction
    // that is sitting right beside the archives. `migrate::work`'s
    // `Step::Apply` cannot do this job — O53: `librarydoc::of` reads non-empty
    // the moment a work has a derived volume or series, so that step is
    // unreachable for any work a scan has touched. This is the "real but
    // different path" O53's own entry names.
    if !crate::migrate::absorbed(conn, work_id, crate::migrate::LIBRARY)? {
        match crate::hand::load(conn, work_id) {
            Ok(Some((path, doc))) => {
                let empty = doc.is_empty();
                match crate::librarydoc::apply(conn, work_id, &doc)
                    .map_err(|e| format!("{}: {e}", path.display()))
                {
                    // `!empty`, matching `hand::save`'s own rule: a
                    // hand-emptied document has nothing left to protect and
                    // derivation should resume. Arming the marker for it would
                    // freeze the cleared state permanently, since nothing calls
                    // `save` on this work again.
                    Ok(()) => crate::migrate::record_absorbed(
                        conn,
                        work_id,
                        crate::migrate::LIBRARY,
                        !empty,
                    )?,
                    Err(why) => report.unreadable_hand.push(why),
                }
            }
            Ok(None) => {}
            // **Reported, not raised.** `write_work` is called with `?` inside
            // the group loop and `derive` holds no transaction of its own, so a
            // `?` here would leave the library half-derived — works before this
            // one written, works after it untouched, and `superseded`/
            // `fold::absorb` and the proposals pass never reached — because one
            // file in someone's library would not parse. This module states the
            // opposite principle three times: *"one unmounted share or one
            // truncated download must not leave a whole library underived"*,
            // *"a scan that died because one work of thousands could not be
            // folded would be worse than a scan that folds nothing"*, and
            // `scan.rs`'s *"one unreadable file must not leave a library
            // unscanned"*. On a fresh database every work is marker-less, so
            // every document is read on that one pass — exactly when a single
            // bad file is most likely to be met.
            Err(why) => report.unreadable_hand.push(why),
        }
    }
    // **No `absorbed` guard here since O80 — the provenance columns answer
    // it per field instead.** That guard did two jobs: "this document has been
    // read back from disk, do not read it again" (the block above, which keeps
    // it) and "do not overwrite these four columns". Conflating them is what
    // let a title-only Save freeze `volume`, `series_id`, `kind` and
    // `language` for a work nobody had edited that way — `librarydoc::of`
    // projected the *effective* values, so every document claimed derivation's
    // own answers as a person's.
    //
    // Migration 011 gave each of the four a `_hand` column, so this pass can
    // write what it derived unconditionally and let `COALESCE` decide. A hand
    // value wins; everything else re-derives, every scan, exactly as it did
    // before anyone edited anything. This is the provenance column I7 left
    // open and `derive.rs` has carried a comment about since #77.
    conn.execute(
        "UPDATE work
            SET series_id = COALESCE(series_id_hand, ?1),
                volume    = COALESCE(volume_hand, ?2)
          WHERE id = ?3",
        rusqlite::params![series_id, group.volume, work_id],
    )
    .map_err(|e| format!("attaching work {work_id} to series {series_id}: {e}"))?;
    write_edition_metadata(conn, work_id, group, report)?;
    write_chapters(conn, work_id, group)?;
    report.works += 1;
    Ok(work_id)
}

/// One member as [`superseded`] reads it.
struct HeldDigest {
    /// The edition it sits on — needed to test whether this row was written
    /// by *this* derive (see [`superseded`]'s condition 3), which `work_id`
    /// alone cannot answer: a written work can still hold an edition this
    /// pass never touched (`DeriveReport::kept_editions`).
    edition_key: String,
    /// Matched against a candidate's own path as well as its `sha256` — see
    /// [`superseded`]'s condition 2. Without it, a byte-identical file at an
    /// unrelated path anywhere in the library reads as evidence that this one
    /// moved.
    path: String,
    sha256: String,
    /// `None` fails condition 4 outright. When `Some`, it also joins the
    /// match key alongside `path` and `sha256` — see [`superseded`]'s
    /// condition 2. `scan::add_root` canonicalises and inserts with no
    /// containment check, so one absolute file can be walked under two
    /// registered roots at once, as two `scan_entry` rows with different
    /// `rel_path`s but the identical `path` and `sha256`; the rules can then
    /// resolve those two rows into two different works that share nothing but
    /// the file itself. `path`/`sha256` alone cannot tell that shape apart
    /// from a real fold, in which the candidate's member and its covering
    /// member descend from the very same `scan_entry` and so carry the same
    /// `root_id` by construction.
    root_id: Option<i64>,
}

/// Every work's members, by work id. One query rather than one per work: a
/// library of thousands is one pass here and thousands of round trips the
/// other way.
fn digests_by_work(conn: &Connection) -> Result<BTreeMap<i64, Vec<HeldDigest>>, String> {
    let mut statement = conn
        .prepare(
            "SELECT edition.work_id, edition.key, member.path, member.sha256, member.root_id
               FROM member JOIN edition ON edition.id = member.edition_id",
        )
        .map_err(|e| format!("preparing the digest read: {e}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                HeldDigest {
                    edition_key: row.get(1)?,
                    path: row.get(2)?,
                    sha256: row.get(3)?,
                    root_id: row.get(4)?,
                },
            ))
        })
        .map_err(|e| format!("reading member digests: {e}"))?;
    let mut by_work: BTreeMap<i64, Vec<HeldDigest>> = BTreeMap::new();
    for row in rows {
        let (work_id, digest) = row.map_err(|e| format!("reading a member digest: {e}"))?;
        by_work.entry(work_id).or_default().push(digest);
    }
    Ok(by_work)
}

/// Works whose every member is now held — at the same `(root_id, path,
/// sha256)`, on an edition this derive actually wrote — by a work this derive
/// wrote, paired with the work that holds it. Detection only — see
/// [`DeriveReport::emptied_works`]; nothing here moves or deletes a row.
///
/// **The five conditions, and why each is needed.** A work qualifies when:
///
/// 1. this derive did not write it;
/// 2. it holds at least one member, and *every* member's
///    `(root_id, path, sha256)` is also held by some one work this derive
///    *did* write;
/// 3. that covering row sits on an edition of that work whose key is in
///    `written_editions`, not merely on the work;
/// 4. every one of its members carries a non-NULL `root_id`;
/// 5. across every one of its members, the intersection of the sets of
///    written works holding that member's key resolves to exactly one
///    work — the target.
///
/// **(2) matches on `path`, not `sha256` alone — a false positive a review
/// found in the design itself (O73's fold), not in an implementation of
/// it.** A live work can drop material from `held` through at least three
/// ordinary routes with nothing wrong having happened — `scan::forget_unseen`
/// deleting `scan_entry` rows out from under a `member.root_id` that survives
/// an unreadable subtree, a rule edit that suppresses entries
/// (`Produces::Deferred`, see [`DeriveReport::suppressed`]), or a re-hash that
/// fails and leaves the entry dropped while the member keeps its old digest.
/// A byte-identical file sitting at an unrelated path anywhere else in the
/// library would, on `sha256` alone, read as proof that one of those routes
/// was actually a fold. It is not: the fold this function exists to detect —
/// measured against the reproduction that opened O73 — never moves a file on
/// disk, it only reclassifies which work and edition an unchanged path
/// belongs to. So the superseded work's member and the covering member on the
/// target must name the identical path, exactly as O73's own reproduction
/// does, or there is no evidence of a fold at all.
///
/// **(2) also requires the covering member's `root_id` to equal the
/// candidate's own — a second false positive a review found in the
/// registration path rather than in a fold.** `scan::add_root` canonicalises
/// and inserts with no containment check, and `scan_entry` is unique on
/// `(root_id, rel_path)`, so registering `/srv/manga` and then
/// `/srv/manga/Extras` walks everything under `Extras` twice, as two rows
/// with different `rel_path`s but identical absolute `path` and `sha256`. The
/// rules can resolve those two rows into two different works, and letting
/// either one leave `held` through any of the three routes above then
/// produces every symptom of a fold over material that never moved: same
/// path, same digest, and both roots still present so condition 4 excludes
/// neither side. `root_id` equality on top of `path` and `sha256` separates
/// that shape from an actual fold cleanly: O73's own reproduction has the
/// candidate's member and the covering member descend from the same
/// `scan_entry` under the same root, so the two ids are equal by
/// construction, and [`write_roots`] guarantees the covering member carries
/// one at all.
///
/// **(3) requires the covering row to sit on an edition this derive
/// actually wrote, not merely on a work it wrote.** A written work can still
/// be holding an edition this pass never touched —
/// [`DeriveReport::kept_editions`]'s own doc names exactly this shape, a work
/// "naming files the rules no longer place there". Treating such a stale row
/// as evidence would let a work be judged superseded on the strength of a
/// fact this derive did not establish, and the `or_insert` tie-break that used
/// to sit here compounded it by preferring the smallest work id — the older,
/// and so likelier-stale, holder.
///
/// **(4) is what keeps an imported work safe.** `006_member_root.sql` states
/// that NULL means "under no root", "which is exactly true of an imported
/// work" — so a hand-imported `taiyaku.json` naming the same archive is never
/// superseded by a scan. A scanned work whose root was removed also reads
/// NULL throughout (`member.root_id` is `ON DELETE SET NULL`) and is likewise
/// left alone: the safe direction of an ambiguity this function must not
/// resolve on its own.
///
/// **(2) is deliberately "every member", never "any".** A work half of whose
/// material moved is not emptied, and treating it as superseded would later
/// delete material nothing else holds.
///
/// **(5) chooses the target by intersecting, not by a first-write-wins
/// map.** For each member, the set of written works holding its exact
/// `(root_id, path, sha256)` as counted evidence is computed; a candidate is
/// covered only when the intersection of those sets, taken across every one
/// of its members, has exactly one work in it — that work is the target. A
/// map from digest to a single "owner" work is stricter than this and misses
/// a real target: with `written = {1, 5}`, work 1 holding `[A]`, work 5
/// holding `[A, B]`, and a candidate holding `[A, B]`, such a map assigns
/// `A → 1` and `B → 5` and refuses the candidate outright, even though work
/// 5 alone covers both of its members — the intersection `{1, 5} ∩ {5} =
/// {5}` finds it. Where the intersection holds more than one work, this
/// leaves the candidate standing rather than guessing which one is "the"
/// target.
///
/// # Errors
///
/// Returns an error if SQLite fails.
fn superseded(
    conn: &Connection,
    written: &BTreeSet<i64>,
    written_editions: &BTreeSet<(i64, String)>,
) -> Result<Vec<(i64, i64)>, String> {
    if written.is_empty() {
        return Ok(Vec::new());
    }
    let held = digests_by_work(conn)?;

    // Borrowed once rather than cloning `edition_key` per member below: a
    // work id's written edition keys as `&str`, borrowed straight out of
    // `written_editions` instead of allocating a fresh `String` at every
    // membership test.
    //
    // Named `wrote_editions`, not `kept_editions`: that name is already taken
    // by [`DeriveReport::kept_editions`] and [`kept_editions`] (the
    // function), both of which mean editions this derive did *not* write and
    // left standing. This map is the opposite — editions this derive *did*
    // write — and a local shadowing that name would make the guard below
    // read backwards to anyone carrying the module's own vocabulary.
    let mut wrote_editions: BTreeMap<i64, BTreeSet<&str>> = BTreeMap::new();
    for (work_id, key) in written_editions {
        wrote_editions
            .entry(*work_id)
            .or_default()
            .insert(key.as_str());
    }

    // Every `(root_id, path, sha256)` held on an edition this derive actually
    // wrote, mapped to the *set* of written works that hold it — not to a
    // single "owner", which is what made the map too strict (see this
    // function's own doc). A stale edition on a written work
    // (`DeriveReport::kept_editions`) contributes nothing here, by
    // construction: only rows whose `(work_id, edition_key)` is in
    // `written_editions` are counted as evidence at all. `root_id` joins the
    // key alongside `path` and `sha256` — see condition 2's own doc above for
    // why matching on the file alone is not enough.
    let mut digest_holders: BTreeMap<(i64, &str, &str), BTreeSet<i64>> = BTreeMap::new();
    for &work_id in written {
        let Some(editions) = wrote_editions.get(&work_id) else {
            continue;
        };
        for member in held.get(&work_id).into_iter().flatten() {
            if !editions.contains(member.edition_key.as_str()) {
                continue;
            }
            // `write_roots` guarantees a written member carries one; if it
            // somehow did not, it could never equal a candidate's own
            // (condition 4 already refuses any candidate with a NULL), so it
            // would never be selected as evidence regardless.
            let Some(root_id) = member.root_id else {
                continue;
            };
            digest_holders
                .entry((root_id, member.path.as_str(), member.sha256.as_str()))
                .or_default()
                .insert(work_id);
        }
    }

    let mut found = Vec::new();
    for (&work_id, members) in &held {
        // `members.is_empty()` cannot occur: `held` is built from an inner
        // `JOIN`, so a work with zero members never produces a row and so
        // never becomes a key in this map at all. Kept as a guard anyway,
        // named here so a later reader does not have to rediscover that.
        if written.contains(&work_id)
            || members.is_empty()
            || members.iter().any(|m| m.root_id.is_none())
        {
            continue;
        }
        let mut candidates: Option<BTreeSet<i64>> = None;
        for member in members {
            // `.is_none()` was excluded for every member of this work above.
            let root_id = member
                .root_id
                .expect("checked above: every member here carries a root_id");
            let key = (root_id, member.path.as_str(), member.sha256.as_str());
            // Intersected in place against a borrowed holder set rather than
            // cloning one per member: only the first member that finds a
            // match needs to allocate an owned set at all.
            match &mut candidates {
                None => candidates = Some(digest_holders.get(&key).cloned().unwrap_or_default()),
                Some(current) => match digest_holders.get(&key) {
                    Some(holders) => current.retain(|w| holders.contains(w)),
                    None => current.clear(),
                },
            }
            if candidates.as_ref().is_some_and(BTreeSet::is_empty) {
                break;
            }
        }
        if let Some(target) = candidates.filter(|c| c.len() == 1) {
            let target = *target.iter().next().expect("len() == 1 checked above");
            found.push((work_id, target));
        }
    }
    // Ascending by superseded work id, and stated rather than left to
    // `BTreeMap` iteration order: the next task iterates this list to delete,
    // and that order must not depend on an incidental property of `held`.
    found.sort_unstable();
    Ok(found)
}

/// The `work` row this group already sits on, as `(id, hub, content_key)`, or
/// `None` for a group this library has never written.
///
/// **Looked up by address rather than by `content_key`**, which cannot be
/// computed until the hub is known — this is the one lookup that breaks that
/// circle. What the address *is* differs by group, and that is the whole of
/// this function:
///
/// **A group with a volume is addressed by `(series_id, volume)`** — columns
/// migration 1 already carries. `volume IS ?2` rather than `=` is kept for
/// the NULL case below rather than needed here.
///
/// `ORDER BY id LIMIT 1` because two rows really can sit at one address, and
/// reading the hub off whichever one SQLite happened to return would make the
/// answer depend on the page order of a b-tree. The oldest row wins, which is
/// the one a reader's progress is on.
///
/// **One derive can produce that state on its own**, which an earlier version
/// of this comment denied by claiming it took a database written before the
/// `known` probe existed. `derive/tests.rs`'s
/// `a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive` is the
/// counterexample: content decides before the address, so a duplicated
/// download sends the second series' group onto the first's row, the `UPDATE`
/// in [`write_work`] then stamps that row with the second series'
/// `(series_id, volume)`, and the row that used to hold that address keeps
/// it. Two rows, one address, no legacy anything.
///
/// **A group with no volume is addressed by its own files**, and it has to
/// be. `(series_id, NULL)` stopped being an address the moment C4 keyed those
/// groups individually: a series holding `001`, `002`, `003` and `004` has
/// four rows matching `volume IS NULL`, and `ORDER BY id LIMIT 1` would hand
/// every one of those groups the first of them — so a work whose bytes
/// changed, or a work arriving later, would take the row of whichever
/// volume-less work got there first, with the reader's position on it.
/// Measured as a merge before C4 and as a theft after it, which is why both
/// halves land together.
///
/// So the probe asks what the group key already says: `rel_path` identifies
/// the group, `member.path` is that `rel_path` under a root, and the row
/// holding one of this group's member paths is the row this group is at. Every
/// path is tried, in a deterministic order, because a work present in two
/// roots is written with both and only one of them may have been there
/// before — that is
/// `a_volume_less_work_still_matches_itself_on_the_next_derive`'s shape, and
/// probing only the first would mint a second work for it.
///
/// This is not the "second identity probe over any member" that
/// [`crate::import::scanned`]'s doc refuses. That one is unscoped, and would
/// merge two genuinely different works sharing one file. This is scoped to
/// one series' volume-less rows and to paths derived from the very `rel_path`
/// that keys the group, and two distinct groups cannot share a member path.
///
/// # Errors
///
/// Returns an error if SQLite refuses either statement.
fn held_work(
    conn: &Connection,
    series_id: i64,
    group: &Group,
) -> Result<Option<(i64, String, String)>, String> {
    if group.volume.is_some() {
        return conn
            .query_row(
                "SELECT id, hub, content_key FROM work
                  WHERE series_id = ?1 AND volume IS ?2 ORDER BY id LIMIT 1",
                rusqlite::params![series_id, group.volume],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| format!("looking up an existing work at this volume: {e}"));
    }
    for candidate in group.editions.values().flatten() {
        let held: Option<(i64, String, String)> = conn
            .query_row(
                "SELECT work.id, work.hub, work.content_key FROM work
                  WHERE work.series_id = ?1 AND work.volume IS NULL
                    AND EXISTS (SELECT 1 FROM edition
                                  JOIN member ON member.edition_id = edition.id
                                 WHERE edition.work_id = work.id AND member.path = ?2)
                  ORDER BY work.id LIMIT 1",
                rusqlite::params![series_id, candidate.path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| {
                format!(
                    "looking up an existing work holding `{}`: {e}",
                    candidate.path
                )
            })?;
        if held.is_some() {
            return Ok(held);
        }
    }
    Ok(None)
}

/// How many editions of `work_id` this derive did not derive — see
/// [`DeriveReport::kept_editions`].
///
/// Read back rather than diffed against what was there before: the keys this
/// derive wrote are `doc.editions`, and every other edition on the row is by
/// definition one [`crate::import::scanned`] left standing.
fn kept_editions(conn: &Connection, work_id: i64, doc: &Sidecar) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("SELECT key FROM edition WHERE work_id = ?1")
        .map_err(|e| format!("reading the editions of work {work_id}: {e}"))?;
    let keys = stmt
        .query_map([work_id], |r| r.get::<_, String>(0))
        .map_err(|e| format!("reading the editions of work {work_id}: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading the editions of work {work_id}: {e}"))?;
    Ok(keys
        .iter()
        .filter(|key| !doc.editions.contains_key(*key))
        .count())
}

/// What to call this work.
///
/// A rule's `title` capture if one decided, and otherwise the name of the
/// thing the **hub** edition holds — the hub rather than whichever edition
/// happens to sort first, so a second edition arriving in another root cannot
/// rename a work someone has already bookmarked.
///
/// There is no formatting of the series and volume into a title here: what is
/// on disk is a name a person chose, and `Go-Toubun no Hanayome - v01 (2019)`
/// says more than `Go-Toubun no Hanayome 1` reconstructed from two columns.
///
/// **The whole `rel_path` when it holds no `/`**, which is a member sitting
/// directly under the root — `Series - v01.cbz` beside its siblings, which
/// seeded rule 4 resolves a series for. Written as its own arm rather than
/// through [`parent_of`], which answers `""` for that case and would take the
/// title's first character off with it.
///
/// **And the root's own directory name when the `rel_path` is empty**, which
/// is the root itself having become the member. Reachable through any
/// `set_series` rule broad enough to match `""` — a `prefix` rule with an
/// empty pattern is what `proposals::accept` writes — and measured producing
/// a work whose title was the empty string and whose slug was therefore
/// `work`, since `slugify` falls back when nothing alphanumeric survives
/// (whole-branch review, C3). The name of the directory the root points at is
/// the only name that entry has; it is read off the member's absolute
/// [`Candidate::path`], since its `rel_path` carries nothing at all.
fn title_of(group: &Group, hub_key: &str) -> String {
    let first = group
        .editions
        .get(hub_key)
        .and_then(|candidates| {
            candidates
                .iter()
                .min_by(|a, b| taiyaku_read::natural_cmp(&a.rel_path, &b.rel_path))
        })
        .expect("the hub key names one of this group's own editions");
    if let Some(title) = &first.title {
        return title.clone();
    }
    if first.rel_path.is_empty() {
        // `root.path.join("")` leaves a trailing separator, which
        // `Path::file_name` steps over — it reads components, not bytes. The
        // fallback is for a root that genuinely has no final component, `/`
        // being the one that does.
        return std::path::Path::new(&first.path).file_name().map_or_else(
            || first.path.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
    }
    match first.rel_path.rfind('/') {
        Some(at) => first.rel_path[at + 1..].to_string(),
        None => first.rel_path.clone(),
    }
}

/// Write the chapter numbers the rules resolved onto the member rows
/// `import::scanned` just wrote.
///
/// **Cleared first, and scoped by `chapter_source` — and the write below is
/// scoped the same way.** A rule edit that stops resolving a chapter must not
/// leave the number a previous derive wrote standing — `replace_editions`
/// carries `chapter_declared` across its own wholesale replace, so nothing
/// else would remove it. The clear names [`SCANNER_CHAPTER_SOURCE`] rather
/// than every row, because a number a person entered by hand, or one a
/// `comicinfo` document supplied, is not this derive's to recover if it turns
/// out wrong. **The write needs the identical guard and, until it had one,
/// did not have it**: a member whose filename still parses — which is exactly
/// the member someone corrects a chapter on, since `builtin-chapter` is a
/// seeded rule — had its hand or `comicinfo` value silently reverted to the
/// filename's answer on the very next re-derive, `chapter_source` included.
/// So the write's own `WHERE` only lands on a row this derive already owns:
/// one it just cleared (`NULL`) or one it wrote on a previous pass
/// (`chapter_source = 'filename'`).
///
/// Matched on `(edition key, ordinal)`, which is where the members this call
/// just described actually landed: `replace_editions` inserts them in document
/// order counting from 0, and `project.rs` reads them back `ORDER BY ordinal`.
fn write_chapters(conn: &Connection, work_id: i64, group: &Group) -> Result<(), String> {
    conn.execute(
        "UPDATE member SET chapter_declared = NULL, chapter_source = NULL
          WHERE chapter_source = ?1
            AND edition_id IN (SELECT id FROM edition WHERE work_id = ?2)",
        rusqlite::params![SCANNER_CHAPTER_SOURCE, work_id],
    )
    .map_err(|e| format!("clearing derived chapters on work {work_id}: {e}"))?;

    for (key, candidates) in &group.editions {
        let ordered = in_page_order(candidates);
        // `(0_i64..).zip(…)` rather than `enumerate()`, so the ordinal is the
        // `i64` the column wants without a `usize as i64` cast on the way.
        for (ordinal, candidate) in (0_i64..).zip(ordered) {
            if let Some(chapter) = candidate.chapter {
                conn.execute(
                    "UPDATE member SET chapter_declared = ?1, chapter_source = ?2
                      WHERE ordinal = ?3
                        AND edition_id = (SELECT id FROM edition
                                           WHERE work_id = ?4 AND key = ?5)
                        AND (chapter_source IS NULL OR chapter_source = ?2)",
                    rusqlite::params![chapter, SCANNER_CHAPTER_SOURCE, ordinal, work_id, key],
                )
                .map_err(|e| {
                    format!("recording chapter {chapter} on `{key}` of work {work_id}: {e}")
                })?;
            }
        }
    }
    Ok(())
}

/// Write each member's root onto the rows `import::scanned` just wrote.
///
/// **The same shape as [`write_chapters`], and for the same reason**: the
/// sidecar format has no field for a root and must not gain one, because a
/// scanned work has no document at all. `derive` writes what the document
/// cannot carry, as a targeted update after the import — see this module's
/// own header for the rule.
///
/// **Unconditional, where [`write_chapters`] is scoped by `chapter_source`.**
/// A chapter number can have been entered by hand and must survive a
/// re-derive; a root is a fact about where a file was walked from, and
/// nothing but a scan can know it. So every member of every edition this
/// derive wrote is rewritten, which is what makes a member that moved between
/// roots stop claiming the old one.
///
/// Matched on `(edition key, ordinal)`, which is where the members this call
/// just described actually landed: `replace_editions` inserts them in
/// document order counting from 0, and `project.rs` reads them back
/// `ORDER BY ordinal`.
///
/// **A member whose root vanished between `group` being built and this write
/// running does not fail the derive.** `group`'s `Candidate::root_id` values
/// were read before this call, and `derive_with` holds no transaction of its
/// own (see that function's own doc) — so a concurrent
/// [`crate::scan::remove_root`] committing in that window deletes the `root`
/// row this `UPDATE` is about to write, and the `member.root_id` foreign key
/// refuses it. Caught specifically — [`is_foreign_key_violation`] — and
/// turned into the same `NULL` `schema/006_member_root.sql`'s own `ON DELETE
/// SET NULL` already wrote for every other member of that root, rather than
/// propagated: see [`DeriveReport::root_removed_mid_derive`] for why this is
/// safe to treat as ordinary rather than as a hard failure. Any other error
/// this statement can return — a locked database, a genuinely malformed
/// statement — still propagates unchanged.
fn write_roots(
    conn: &Connection,
    work_id: i64,
    group: &Group,
    report: &mut DeriveReport,
) -> Result<(), String> {
    for (key, candidates) in &group.editions {
        let ordered = in_page_order(candidates);
        for (ordinal, candidate) in (0_i64..).zip(ordered) {
            let result = conn.execute(
                "UPDATE member SET root_id = ?1
                  WHERE ordinal = ?2
                    AND edition_id = (SELECT id FROM edition
                                       WHERE work_id = ?3 AND key = ?4)",
                rusqlite::params![candidate.root_id, ordinal, work_id, key],
            );
            match result {
                Ok(_) => {}
                Err(e) if is_foreign_key_violation(&e) => {
                    conn.execute(
                        "UPDATE member SET root_id = NULL
                          WHERE ordinal = ?1
                            AND edition_id = (SELECT id FROM edition
                                               WHERE work_id = ?2 AND key = ?3)",
                        rusqlite::params![ordinal, work_id, key],
                    )
                    .map_err(|e| {
                        format!(
                            "clearing the vanished root of `{key}` member {ordinal} \
                             on work {work_id}: {e}"
                        )
                    })?;
                    report.root_removed_mid_derive += 1;
                }
                Err(e) => {
                    return Err(format!(
                        "recording the root of `{key}` member {ordinal} on work {work_id}: {e}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Whether SQLite refused a statement because it would have written a value
/// with no matching row on the other end of a foreign key — [`write_roots`]'s
/// own use is the only one today: a root deleted out from under an in-flight
/// derive.
fn is_foreign_key_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    )
}

/// One edition's candidates in page order.
///
/// **Member order is page order** — `Edition::pages` calls it normative, and
/// every offset in every alignment after the first member is computed from
/// it. `natural_cmp` rather than SQL's `ORDER BY`, because `c10` sorts before
/// `c9` as text.
///
/// One function rather than the same two lines at each call site, because
/// they are not four independent sorts: [`write_work`] writes the members in
/// this order, [`write_chapters`] and [`write_roots`] then address those very
/// rows by the ordinal it produced, and [`write_edition_metadata`] settles a
/// disagreement by it. Four copies would be four chances for one of them to
/// disagree about which member is `ordinal` 0.
fn in_page_order(candidates: &[Candidate]) -> Vec<&Candidate> {
    let mut ordered: Vec<&Candidate> = candidates.iter().collect();
    ordered.sort_by(|a, b| taiyaku_read::natural_cmp(&a.rel_path, &b.rel_path));
    ordered
}

/// Write the edition kind and language the rules resolved onto the `edition`
/// rows [`crate::import::scanned`] just wrote.
///
/// **The columns were there and nothing reached them.** `rule` has carried
/// `set_kind` and `set_language` since the scanner's own migration, the
/// engine resolves both into [`crate::rules::Resolved`], and C3's accept form
/// offers both as controls — while `import::replace_editions` inserts
/// `(work_id, key, label)` and nothing else, so a rule saying
/// `set_kind = 'official'` left `edition.kind` NULL. Measured on real
/// archives rather than reasoned about: one accepted `edition-of` proposal,
/// one rescan, and the `full-color` edition that rescan produced held no kind
/// at all, so two of the accept form's five controls decided nothing.
///
/// # Why these two do not join the never-overwrite list
///
/// A scan must not write over what a person established, which is why
/// `import::update_work` leaves `work.title`, `work.reading` and `work.hub`
/// alone on a row that already holds them, and why `import::replace_editions`
/// keeps a held `edition.label`. **What forces that rule on those four is an
/// absence**: nothing records whether the value there came from a person or
/// from an earlier derive, so never writing is the only rule available that
/// cannot destroy the answer.
///
/// This pair does not share the absence. `librarydoc::apply` is the only
/// other production writer of a *value* into `edition.kind`/`edition.language`
/// — `replace_editions`' `UPDATE` puts back what the row already held, across
/// its own delete-and-reinsert — and `migrate::absorbed(…, LIBRARY)` answers,
/// per work, whether such a document has actually been applied. So these join
/// `work.series_id` and `work.volume` under that guard instead of joining a
/// list whose reason does not reach them; see [`write_work`]'s own comment on
/// it.
///
/// **Per work rather than per edition, which is the granularity the other
/// writer has.** `librarydoc::apply` nulls `kind` and `language` on every
/// edition of the work before applying the document over them, so what it
/// speaks for is the whole work's edition metadata. A finer guard here would
/// be claiming a distinction the document itself does not draw. The cost is
/// real and stated: an edition derived *after* a document was absorbed takes
/// no kind from the rules, and editing the document is the fix.
///
/// **Unconditional within the guard, and that is the point.** Editing a rule
/// changes what the next scan writes, including back to NULL once no rule
/// resolves one — the same shape [`write_chapters`] gives a chapter number
/// this module wrote itself. Filling only a NULL would have been the safer
/// looking rule and would have made C3's Edition Kind control work exactly
/// once per edition.
///
/// **A kind outside the closed set is not written, and is counted.**
/// `edition.kind` is `raw | official | fan` (`001_initial.sql`), which is the
/// set `librarydoc::parse_edition_kind` reads and refuses against — a stored
/// value outside it makes `librarydoc::of` fail for that work, so writing one
/// would turn a typo in a rule into a work that cannot be described or
/// exported. It is reachable: `rule.set_kind` is a bare `TEXT` column, and
/// C3's accept route takes the field as free text. Filtered here rather than
/// refused where a rule is written, because this is the one place every
/// source of a rule arrives at, and counted as
/// [`DeriveReport::unrecognised_kinds`] rather than dropped in silence.
/// `language` is free text in the schema and in `librarydoc` alike, so there
/// is nothing to filter it against.
///
/// **Where one edition's entries disagree, the first in page order that
/// resolved anything decides, per field.** The rules resolve per path and an
/// edition can gather paths from more than one root, so the disagreement is
/// reachable; nothing reconciles it and nothing reports it. Per field rather
/// than per entry because that is how the rules themselves resolve — a later
/// rule overrides only the fields it sets. The alternative, writing nothing
/// unless every entry agrees, would throw away a root's `default_kind`
/// because a file from a differently-configured root sorted first.
fn write_edition_metadata(
    conn: &Connection,
    work_id: i64,
    group: &Group,
    report: &mut DeriveReport,
) -> Result<(), String> {
    for (key, candidates) in &group.editions {
        let ordered = in_page_order(candidates);
        let language = ordered.iter().find_map(|c| c.language.clone());
        let kind = match ordered.iter().find_map(|c| c.kind.clone()) {
            Some(kind) if crate::librarydoc::parse_edition_kind(&kind).is_ok() => Some(kind),
            Some(_) => {
                report.unrecognised_kinds += 1;
                None
            }
            None => None,
        };
        conn.execute(
            // `COALESCE(<hand>, <derived>)` — O80, migration 011. See
            // `write_work`'s note where the `absorbed` guard used to stand.
            "UPDATE edition
                SET kind     = COALESCE(kind_hand, ?1),
                    language = COALESCE(language_hand, ?2)
              WHERE work_id = ?3 AND key = ?4",
            rusqlite::params![kind, language, work_id, key],
        )
        .map_err(|e| {
            format!("recording the kind and language of `{key}` on work {work_id}: {e}")
        })?;
    }
    Ok(())
}

/// Every chapter-organised directory across every root — resolved a chapter
/// but no volume, after [`nested_in_claim`]'s suppression — paired with
/// the series it resolved to.
///
/// **Shared with `proposals::raise` rather than walked a second time.** A
/// second implementation of "which directory is chapter-organised" is
/// exactly how two mechanisms come to disagree — the same argument this
/// module's own doc comment makes about `series` and `edition` rows. This
/// function reuses [`entries`], [`admit`], [`Seen::claims_subtree`] and
/// [`nested_in_claim`], the same primitives [`derive`] itself walks with,
/// rather than restating their judgement: the suppression in particular is
/// the part a second implementation would have to get identically right to
/// avoid flagging a perfectly attached volume's own chapter subdirectories
/// (§7's case, recorded on [`nested_in_claim`]).
///
/// **That sharing is why [`Seen::claims_subtree`] does not consult the
/// digest.** Were the claim "became a member", this function could only
/// answer by hashing — which it must not do, since it runs on every
/// `proposals::raise` — and would have to approximate instead, which is the
/// second implementation this paragraph exists to refuse.
///
/// The map is keyed by `(root_id, directory rel_path)` — the same identity
/// [`classify`]'s own `chapter_dirs` set uses — with the series name attached,
/// which that set does not carry because nothing before Task 8 needed it.
///
/// # Errors
///
/// Returns an error under the same conditions [`derive`] does: the rules or
/// roots cannot be read, or a `scan_entry` row will not project.
pub(crate) fn unattached_chapter_dirs(
    conn: &Connection,
) -> Result<BTreeMap<(i64, String), String>, String> {
    let (engine, _complaints) = Engine::load(conn)?;
    let roots = crate::scan::roots(conn)?;
    let mut report = DeriveReport::default();
    let mut dirs: BTreeMap<(i64, String), String> = BTreeMap::new();

    for root in &roots {
        let mut seen = Vec::new();
        for entry in entries(conn, root.id)? {
            let Some(admitted) = admit(&engine, root, entry, &mut report) else {
                continue;
            };
            seen.push(admitted);
        }
        let claims = claims_of(&seen);
        for admitted in seen {
            if nested_in_claim(&claims, &admitted.entry.rel_path) {
                continue;
            }
            if admitted.resolved.volume.is_none() && admitted.resolved.chapter.is_some() {
                dirs.insert(
                    (root.id, parent_of(&admitted.entry.rel_path).to_string()),
                    admitted.series.clone(),
                );
            }
        }
    }
    Ok(dirs)
}

/// Every `(root id, rel_path)`, across every root, that the rules currently
/// resolve to `series` — every `scan_entry` [`admit`] would place in this
/// series, with unreadable and series-less entries already excluded by that
/// function.
///
/// **Shared with `proposals::accept` for the same reason
/// [`unattached_chapter_dirs`] is shared with `proposals::raise`.** An
/// earlier version of `accept` answered "which paths belong to this series"
/// by reading the series' own `sort_title` — reasoning that seeded rule 1
/// resolves a series from the first path component, so the title *is* that
/// component. **False whenever a higher-ordinal rule decides the series
/// first** — seeded rule 4 exists precisely because a filename sometimes
/// repeats the series where the directory does not — and it produced a
/// `prefix` rule that matched nothing: the "accepted, and nothing a person
/// can see" outcome this module refuses `attach-chapters` to avoid, reached
/// by a different road. Reading `admit`'s own resolution here, rather than
/// re-deriving "which series does this path belong to" a second way in
/// `proposals.rs`, is what keeps this answer and `derive`'s own from being
/// able to disagree.
///
/// **The root id travels with the path** so `proposals::accept` can scope the
/// rule it writes to the one root the subject's own material actually lives
/// under (I6) — a `prefix` rule with `root_id` NULL applies to every root
/// forever, including one added after the accept, sharing nothing with the
/// subject beyond a directory name that happens to match.
///
/// Not scoped to candidates (volumes or chapters) the way
/// [`unattached_chapter_dirs`] is — every admitted path counts, since what
/// `accept` needs is "where does this series' material live", not "which of
/// it is a member".
///
/// # Errors
///
/// Returns an error under the same conditions [`derive`] does: the rules or
/// roots cannot be read, or a `scan_entry` row will not project.
pub(crate) fn rel_paths_for_series(
    conn: &Connection,
    series: &str,
) -> Result<Vec<(i64, String)>, String> {
    Ok(rel_paths_by_series(conn, &BTreeSet::from([series]))?
        .remove(series)
        .unwrap_or_default())
}

#[cfg(test)]
thread_local! {
    /// How many times [`rel_paths_by_series`] has run **on the calling
    /// thread**, in this test process. The one-sweep guarantee its own doc
    /// describes used to be pinned by a claim in a comment ("every callee
    /// takes `&[(i64, String)]`, so a second call is not reachable without a
    /// signature change"), which was false: `proposals::side` holds a
    /// `Connection` and `proposals::offered_rule` holds a `Connection` and a
    /// `&str`, and either could call [`rel_paths_for_series`] again with no
    /// signature change at all. This counts instead of asserting.
    ///
    /// Thread-local, not a shared `static`, because libtest runs `#[test]`
    /// functions concurrently on a thread pool: a single process-wide
    /// counter measured `2` for a lone `evidence` call the moment another
    /// test's `evidence`/`preview` happened to land inside the same
    /// reset-then-read window on a different thread. Nothing here calls
    /// `rel_paths_by_series` from a second thread, so a thread-local count
    /// is exact for whichever test resets and reads it, regardless of what
    /// else the suite is doing.
    pub(crate) static REL_PATHS_BY_SERIES_CALLS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// [`rel_paths_for_series`] for several series in **one** pass.
///
/// Every requested title is a key of the answer, holding an empty `Vec` where
/// nothing resolved to it — "no entries" is an answer here, not an absence.
///
/// **The pass is what costs, not the filter.** Each call compiles every rule
/// in the library ([`Engine::load`]), reads every root, `SELECT`s every
/// `scan_entry` row under each, and resolves every one of them; deciding
/// whether the series that came back is one of two rather than one of one is
/// free beside it. `proposals::evidence` asks about both ends of a proposal
/// at once and used to pay for the whole sweep twice to do it.
pub(crate) fn rel_paths_by_series(
    conn: &Connection,
    series: &BTreeSet<&str>,
) -> Result<BTreeMap<String, Vec<(i64, String)>>, String> {
    #[cfg(test)]
    REL_PATHS_BY_SERIES_CALLS.with(|c| c.set(c.get() + 1));
    let (engine, _complaints) = Engine::load(conn)?;
    let roots = crate::scan::roots(conn)?;
    let mut report = DeriveReport::default();
    let mut found: BTreeMap<String, Vec<(i64, String)>> = series
        .iter()
        .map(|title| ((*title).to_string(), Vec::new()))
        .collect();

    for root in &roots {
        for entry in entries(conn, root.id)? {
            let Some(admitted) = admit(&engine, root, entry, &mut report) else {
                continue;
            };
            if let Some(paths) = found.get_mut(&admitted.series) {
                paths.push((root.id, admitted.entry.rel_path));
            }
        }
    }
    for paths in found.values_mut() {
        paths.sort();
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
