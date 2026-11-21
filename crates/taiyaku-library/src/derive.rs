//! `scan_entry` + `rule` + `content_match` → series, works, editions,
//! members, and the alignments of volumes matched by content.
//!
//! **Two inputs, read on every derive, which is what keeps §2's claim true.**
//! Structure comes from the facts and the rules, and from the pairs matched
//! by content, which §6 of
//! `docs/superpowers/specs/2026-11-15-edition-matching-by-content-design.md`
//! adds as the second because a rule is a path pattern and cannot carry a
//! page range, a score or a Split; a rule's literal series still wins over a
//! match ([`joins`]). Nothing else — not a manual accept, which §6.1 routes
//! through a written `rule` for precisely this reason. An input a derive did
//! not read would mean a re-derive either discards the accepts or needs a
//! merge rule to protect them, which is the problem the store spec already
//! solved once with `by: "hand"` and `Sidecar::merge_spoke`; solving it a
//! second time in a different shape is how two mechanisms come to disagree.
//!
//! **It never opens a file.** Page counts and digests are columns on
//! `scan_entry`, written by the walk, so editing a rule re-derives at memory
//! speed. `derive/tests.rs` deletes the tree before the second derive, which
//! is the only way to test a negative like this.
//!
//! # The one file it does open, and why that is not a contradiction
//!
//! §3 makes hashing lazy: `scan.rs` leaves `sha256` NULL because hashing the
//! series on every walk is not affordable. A member about to become part of a
//! work needs its digest — `content_key` is a hash over them — as does the
//! entry of one a work already holds that no rule stages
//! (`digest_held_members`), so this module is where that laziness ends,
//! once per member ever, with the answer written **back onto `scan_entry`**.
//! That is what makes the *next* derive need no filesystem at all, which is
//! the property the tests pin.
//!
//! # One writer per table, and what it writes directly
//!
//! `work`, `edition` and `member` **rows** are written by [`crate::import`]
//! alone — this module builds a [`Sidecar`] and hands it to
//! `import::scanned_inserting` rather than issuing its own inserts, so the
//! column lists cannot drift from `project.rs`'s reader. What it then writes
//! itself is exactly what the sidecar format has no field for and so cannot
//! carry: `work.series_id`, `work.volume`, `edition.kind`/`edition.language`,
//! `member.chapter_declared`/`chapter_source` and `member.root_id`. All but
//! the last of those are already targeted updates in `librarydoc::apply`, so
//! this is the established shape here rather than a new one; a member's root
//! is the one column nothing but a walk can know. `series` and `series_title`
//! have no importer at all and are written here outright. And where a join
//! moves the edition a work's hub names off it, `work.hub`, which a scan's
//! import leaves as it stands ([`joins::move_left_behind`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};
use taiyaku_core::{Direction, Edition, Member, Sidecar, Work};

use crate::progress::{Outcome, Step, Unwatched, Watch};
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

/// Why derivation could make no structure out of an entry, one field per
/// cause — [`DeriveReport::unreadable`]. O90.
///
/// **A path where a person has to find the file, a count where they do not.**
/// A refused format is counted per token, because one loop fixes the archives
/// of one format and their names add nothing to it, and `by_format` is bounded
/// by the formats there are rather than by the size of the library. A container
/// holding no pages of its own is counted, because its usual instance is a
/// volume directory holding archives, which is a layout and not a fault. The
/// other two are single files that something is wrong with, so they are named,
/// for [`DeriveReport::unreadable_hand`]'s reason: the person has to be told
/// *which* file. Each is joined to its root, since two roots can hold the
/// same `rel_path`.
///
/// `docs/superpowers/specs/2026-09-15-o90-unreadable-attribution-design.md`
/// §2.1 records the argument.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unreadable {
    /// Entries in a format [`taiyaku_read::Format::is_readable`] refuses, by
    /// `scan_entry.format` token — the schema's own vocabulary, for the
    /// reason [`taiyaku_read::Format::token`] gives. A file no signature
    /// matched has a NULL format and no token, and is counted as `unknown`,
    /// the spelling `taiyaku-serve`'s `stamp` already uses for it.
    ///
    /// A format that refused nothing has no key rather than a zero, so an
    /// empty map and a map of zeros cannot both mean "none".
    pub by_format: BTreeMap<String, usize>,
    /// Containers whose pages the walk could not count, so `pages` is NULL:
    /// an archive that would not open, **or one that opened and held nothing
    /// `taiyaku-read` reads as a page**. The store keeps no record of which:
    /// `pages_of` hands `member_pages`' refusal to the scan's log, not to the
    /// row (O131).
    ///
    /// The second is not hypothetical (`docs/notes.md`, "O90").
    pub uncounted: Vec<String>,
    /// Containers that listed cleanly and held no page images of their own,
    /// so `pages` is `Some(0)` — a volume directory holding archives is one,
    /// and the archives inside it are what become the members.
    ///
    /// Only a directory arrives here. An archive with no page images is a
    /// refusal from `member_pages`, never a zero, so it lands in
    /// [`Self::uncounted`] instead.
    pub no_pages: usize,
    /// Members whose file was no longer there to hash — an unmounted share,
    /// or a tree deleted between the walk and the derive.
    pub vanished: Vec<String>,
}

impl Unreadable {
    /// How many entries were refused by format: `by_format`'s counts, summed.
    ///
    /// A method rather than a field, so the total and the breakdown cannot
    /// disagree.
    #[must_use]
    pub fn refused_format(&self) -> usize {
        self.by_format.values().sum()
    }

    /// Every entry counted for any of the four causes — the one number
    /// [`DeriveReport::unreadable`] was before O90.
    #[must_use]
    pub fn total(&self) -> usize {
        self.refused_format() + self.uncounted.len() + self.no_pages + self.vanished.len()
    }
}

/// The [`Unreadable::by_format`] key for an entry whose `scan_entry.format`
/// is NULL: a file no signature matched, which [`taiyaku_read::Format::token`]
/// gives no token. `taiyaku-serve`'s `stamp` spells the same fact the same
/// way.
const UNKNOWN_FORMAT: &str = "unknown";

/// What one [`fn@derive`] call turned the facts into.
///
/// `unreadable` is every entry derivation could make no structure out of for
/// want of readable bytes, which is four situations and not one — see
/// [`Unreadable`], which keeps them apart. All four are reported rather than
/// raised, because one unmounted share or one truncated download must not
/// leave a whole library underived.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeriveReport {
    pub series: usize,
    pub works: usize,
    pub editions: usize,
    pub members: usize,
    pub unattached_chapter_dirs: usize,
    pub unreadable: Unreadable,
    /// Entries an ancestor spoke for — a volume split into chapter
    /// subdirectories contributes one member and three of these. See
    /// [`Seen::claims_subtree`].
    ///
    /// **Counted because a silent drop is indistinguishable from an empty
    /// library.** A rule set that made a series directory speak for its
    /// volumes would swallow every one of them, and without this number the
    /// result reads as a library that happens to hold nothing rather than as
    /// a rule that swallowed it. A `set_series` rule broad enough to match the
    /// root's own `rel_path` collapses the whole library onto one work, and
    /// this is the field that says so.
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
    /// narrowing it would mean asking `import` which probe answered, a third
    /// question for a signature that answers two.
    pub rekeyed: usize,
    /// Editions left standing on a work this derive wrote or left as it
    /// stands, that this derive did not derive.
    ///
    /// **The stated cost of [`crate::import::scanned`] not deleting them.** A
    /// scan cannot tell "the scan never saw this edition" from "a rule edit
    /// removed it", so it must not act as though it can — deleting a
    /// hand-aligned edition is unrecoverable, while an orphan is visible and
    /// fixable. What that leaves is an orphan nobody asked for: edit a rule so
    /// `raw` becomes `colour`, and the work keeps a `raw` edition naming files
    /// the rules no longer place there.
    ///
    /// So it gets a number, for the same reason [`Self::suppressed`] and
    /// [`Self::rekeyed`] have one — the outcome changes what a person sees
    /// and cannot be inferred from the other counts. `editions` counts what
    /// this derive named; nothing in it says how many were already there.
    ///
    /// **Counted on every derive, not only the one that orphaned it.** A
    /// standing orphan is a standing fact, and a count that appeared once and
    /// then went quiet would say the library had been fixed.
    pub kept_editions: usize,
    /// Works whose every member is now held by a work this derive derived — see
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
    /// job quietly — which is why the counters below travel with it rather
    /// than leaving "1 work folded away" to stand for whatever happened inside.
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
    /// pair where one reader holds one note id on both works, in two copies
    /// that differ (identical ones are dropped, see [`Self::dropped_notes`]);
    /// a pair whose ground truth on a mapped edition was drawn against
    /// different archives on the two sides; and a pair holding different
    /// hand-set titles, volumes or series. `derive` refuses a fold onto a work
    /// whose hand document did not read this pass. None of these is the
    /// fold's to destroy or to rewrite; [`Self::unfoldable`] says which.
    ///
    /// **And a fold that would leave a row naming another page (O121)** — a
    /// reading position or a note whose page and half has no slot on the
    /// surviving edition, or an alignment the surviving work already holds
    /// whose spans the spread flags the fold writes would move. A slot is an
    /// index into one edition's spreads and reading direction, and the fold
    /// places it by page and half rather than carrying the number.
    pub unfolded_works: usize,
    /// Why each of those works, and each of [`Self::unmoved_editions`], stands:
    /// one message each, naming the work and what could not be mapped or moved.
    ///
    /// **A count says something was refused; it does not say what to look
    /// at.** The refusal message `fold::absorb` builds already names the work
    /// id and the edition keys, and dropping it for a number would leave an
    /// operator with "1 superseded work left standing" over a library of
    /// thousands. [`crate::scan::ScanReport::unreadable_dirs`] is the precedent
    /// and the same argument: a `Vec` costs one field, and the alternative is
    /// a person grepping for a work they cannot name.
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
    /// **Not only what a failed save had left missing.** A failed write is
    /// one cause and not the only one: a root whose `keeps_documents` was off
    /// when the edit was made, and has since been turned on, arrives here with
    /// its marker armed and no file — the ordinary path for anyone opting a
    /// root back in, and the one a person is most likely to be watching.
    /// Naming a cause the count cannot know is how a report starts lying about
    /// the common case.
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
    /// addresses — O81 — and, after a removal, each document its carry left in
    /// a removed directory's `.taiyaku`, which no root's reconcile reads.
    ///
    /// **Never a document a live work still addresses.** A root that has
    /// declined to keep documents reports those under
    /// [`Self::declined_documents`] instead: migration 012 defaults every
    /// existing root's `keeps_documents` off, so a report that called an
    /// addressed file an orphan would name *every* hand document in the
    /// library as stranded on the first derive after upgrading.
    ///
    /// Each carries the title out of the file, whose name is a `content_key`,
    /// a hash no person reads a work from. See [`crate::hand::Orphan`].
    pub orphan_documents: Vec<crate::hand::Orphan>,
    /// Documents at a work's own address whose root has declined to keep
    /// them: `root.keeps_documents` is off, so nothing is owed, but a live
    /// work still addresses the file — see [`crate::hand::Declined`].
    ///
    /// **A field rather than a silence, under the same rule as
    /// [`Self::suppressed`] and [`Self::rekeyed`]: any derivation outcome
    /// that changes what a person sees and cannot be inferred from the other
    /// counts gets one.** This name is what lets a person read the difference
    /// between "act on this, it is stranded" and "this root simply no longer
    /// keeps what is here."
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
    /// does not depend on which edition key sorted first.
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
    /// sidecar imported against both volumes. Copies that differ in anything
    /// refuse the fold, and are named in [`Self::unfoldable`].
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
    /// The alignment a fold cannot carry is [`Self::uncarried_alignments`].
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
    /// **So a fold is not the only way in, and the name says so.**
    /// [`fn@derive`] mints a series row for every series a rule or a
    /// directory names, including one holding nothing but unattached chapter
    /// directories — an ordinary shape in this project's corpus — so a first
    /// scan of a library that has never folded anything reports one of these.
    pub series_holding_no_work: usize,
    /// Entries that joined an edition another entry had already opened — a
    /// volume arriving as two chapter directories contributes one of these; a
    /// copy of a member under another directory joins no page sequence and none.
    ///
    /// **A merge is a page sequence.** Member order is page order (see
    /// [`write_work`]), so two entries in one edition are one continuous
    /// volume as far as every alignment offset after the first member is
    /// concerned. That is right for the chapters of a volume and wrong for
    /// anything else, and derivation cannot always tell which it has: the
    /// rules resolved a volume for both, and that is all it knows.
    ///
    /// So it is counted, for the same reason [`Self::suppressed`] is: a merge
    /// of entries that are not one volume would otherwise be indistinguishable
    /// from a work that genuinely holds one volume, so merges are reported
    /// rather than assumed correct.
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
    /// **One raiser, like one writer.** `derive` calls `proposals::raise` at
    /// the end of every pass, and this is that call's own count, carried out
    /// through the report rather than re-derived by a second call: a second
    /// call would find every proposal the first had just opened already in
    /// `proposal`, and its `INSERT OR IGNORE` would conflict on every row and
    /// count `0`.
    pub raised: usize,
    /// Members whose root was deleted by a concurrent [`crate::scan::remove_root`]
    /// while this derive was already running, between the group this pass
    /// built in memory and [`write_roots`] writing it back.
    ///
    /// `derive_with` holds no transaction of its own (see this module's own
    /// doc on [`fn@derive_with`]), so under WAL a root removal that commits
    /// mid-pass leaves a member's `Candidate::root_id` naming a row that is
    /// already gone. `write_roots` catches the foreign-key constraint on
    /// `member.root_id` that its `UPDATE` then trips, clears the column to
    /// `NULL` instead — the identical state `schema/006_member_root.sql`'s own
    /// `ON DELETE SET NULL` gives every other member of the removed root — and
    /// counts it here rather than either failing the pass or losing the event,
    /// for [`Self::suppressed`]'s reason; a copy's directory gone the same way
    /// is counted too and gets no `member_root` row. A member counted here
    /// reads as an imported member with no root on record does (O54), and the
    /// next scan that finds it under a surviving root recovers it.
    pub root_removed_mid_derive: usize,
    /// Works this pass marked missing: none of their members is held by any
    /// directory they were under ([`mark_missing`]).
    ///
    /// **Counted because the mark hides.** A partial mount marks every work
    /// whose files it left out and takes each out of every listing, so without
    /// this number one that hides hundreds of works reads as a library that
    /// is merely smaller.
    pub marked_missing: usize,
    /// Marked works this pass cleared: a directory holds a member again, or
    /// the work now has a member under no directory or no member at all, so
    /// no scan is the authority for it ([`mark_missing`]).
    ///
    /// Counted beside [`Self::marked_missing`] for its reason: the remount
    /// that brings those works back says how many it brought.
    pub found_again: usize,
    /// Files that joined another group's work as an edition because their
    /// pages matched one of its volumes ([`joins::apply`]), an edition that
    /// moved with its group's matched file included.
    ///
    /// **Counted on every derive, not only the one that moved them**, for
    /// [`Self::kept_editions`]' reason: a join is a standing fact of the
    /// files, the rules and the match rows, and a count that went quiet once
    /// the rows were written would say the joins had gone.
    pub joined: usize,
    /// Works that joined nothing and moved, with their own volume numbers,
    /// into the series their directory's joined files went to
    /// ([`joins::apply`], spec §4 "Directories follow").
    ///
    /// Counted on every derive, for [`Self::joined`]'s reason.
    pub followed: usize,
    /// Pairs whose files share a work by content and whose alignment this pass
    /// did not write: the match pass numbers a file's pages from its prints
    /// and the edition from the scan, which can differ for good
    /// (`fingerprints.rs`, `numbers_other_pages`), and runs on the pass's
    /// numbers would name pages the edition lacks, or the wrong ones
    /// ([`joins::write_alignments`]). Counted on every derive, for
    /// [`Self::joined`]'s reason, so a join without its ⇄ says why.
    pub unwritten_alignments: usize,
    /// Editions a join, an unjoin or a Split left on a work that keeps
    /// others, moved onto the work this derive placed their file on with
    /// their readers' positions, notes and truth ([`joins::move_left_behind`]).
    /// What each carried is in the fold's counts above, and an edition moved
    /// is not one of [`Self::kept_editions`], as it no longer stands.
    pub moved_editions: usize,
    /// Editions such a move refused, left standing on both works, each named
    /// in [`Self::unfoldable`]. Counted on every derive, for
    /// [`Self::kept_editions`]' reason: each derive tries the move again.
    pub unmoved_editions: usize,
}

impl DeriveReport {
    /// Add what a fold, or one edition's move, carried and left behind.
    fn count_moved(&mut self, moved: &crate::fold::Moved) {
        self.moved_positions += moved.positions;
        self.lost_positions += moved.lost_positions;
        self.moved_notes += moved.notes;
        self.dropped_notes += moved.dropped_notes;
        self.moved_truth_pages += moved.truth_pages;
        self.moved_truth_collisions += moved.collisions;
        self.discarded_rows += moved.discarded;
        self.uncarried_alignments += moved.uncarried_alignments;
    }
}

/// Whether a derivation reconciles `<root>/.taiyaku/` or leaves the tree
/// alone.
///
/// **Two call sites, and both of them write.** `derive` calls
/// [`crate::hand::reconcile`] per root, which writes a missing document back
/// where the root keeps them, and `write_work` calls
/// [`crate::hand::rename_from`], which renames a document when a work's
/// `content_key` moves, even inside a root that keeps none — a rekey
/// stranding a document is the defect it exists to prevent.
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
/// Kept as its own function rather than a defaulted parameter because most of
/// its call sites are tests, and none of them has an opinion about documents.
///
/// # Errors
///
/// As [`derive_with`].
pub fn derive(conn: &Connection) -> Result<DeriveReport, String> {
    derive_with(conn, Documents::Reconcile)
}

/// Turn every `scan_entry` in the store, whichever library holds its
/// directory, into series, works, editions and members, through the rules.
///
/// Idempotent by construction: series are reconciled on `sort_title`, and
/// works on `(series_id, volume)`, both across the store, so
/// running this twice over unchanged facts writes no second set of rows, and
/// nothing to a work that has not changed ([`write_work`]).
///
/// **No transaction of its own — but everything under it can still be
/// wrapped in one.** [`write_work`] opens one per work it writes, which
/// `import::scanned_inserting`'s own nests in, and [`crate::fold::absorb`]
/// opens one per fold; all three do it through [`crate::Nested`] rather than
/// `unchecked_transaction` directly: a plain `BEGIN` — the same constraint
/// [`crate::import::replace_alignments`] records — is refused outright inside
/// an already-open transaction, where `Nested` instead nests as a
/// `SAVEPOINT`. Called plainly, as [`fn@derive`]'s callers call it, nothing
/// is open yet, so `Nested` takes the real-`BEGIN` branch for each work and
/// each fold and a derive is atomic per work — save a work rekeyed under
/// [`Documents::Reconcile`], which commits before and after
/// [`crate::hand::rename_from`] (see [`write_work`]) — and not across
/// the store: the right grain for that case, since a store that
/// half-derives has half its works current, not none of them.
///
/// **`preview` is the one caller that needs the store-wide grain, and gets
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
    done(derive_watched(conn, documents, &[], &Unwatched)?)
}

/// The report of a derive watched by [`Unwatched`], for a caller that never
/// cancels one.
///
/// # Errors
///
/// `Unwatched` is never cancelled; a cancel is an `Err` rather than a panic
/// all the same, as `scan::scan_root` answers it.
pub fn done(outcome: Outcome<DeriveReport>) -> Result<DeriveReport, String> {
    match outcome {
        Outcome::Done(report) => Ok(report),
        Outcome::Cancelled(step) => Err(format!("the derive stopped while {}", step.as_str())),
    }
}

/// [`derive_with`], reporting to `watch` the bytes hashed of the bytes to
/// hash, then the works written of the works to write, then the tail.
///
/// **A cancel leaves what a stop at the same point would.** The flag is read
/// before each folder is sized, each entry classified and each work written,
/// and before the tail. Every digest and work written stays, none is
/// half-written, and the tail waits for the next derive
/// (spec 2026-11-15-first-scan-progress-and-cancel §5).
///
/// A paused directory that `fresh` does not name is frozen ([`frozen_entries`]).
///
/// # Errors
///
/// As [`derive_with`].
pub fn derive_watched(
    conn: &Connection,
    documents: Documents,
    fresh: &[i64],
    watch: &dyn Watch,
) -> Result<Outcome<DeriveReport>, String> {
    // Once, for the whole library. `Engine::compile` is where every regex in
    // the rule set is compiled, and compiling per entry would recompile the
    // same handful of patterns for every file in a library of thousands.
    //
    // The complaints are dropped here rather than propagated: a malformed
    // pattern disables its own rule and nothing else, and the callers that
    // report them to a person compile the rules themselves. Surfacing them a
    // second time from here would mean two callers deciding how loudly to say
    // the same thing.
    let (engine, _complaints) = Engine::load(conn)?;
    let roots = crate::scan::roots(conn)?;
    // On `conn`, so a caller's transaction sees the pauses it wrote. A root
    // `fresh` names is read as the walk saved it whatever its pause says: a
    // pressed Scan's unpause is best-effort, and freezing the directory it was
    // pressed for would hash and write nothing of what it walked.
    let frozen: HashSet<i64> = crate::scan::paused(conn)?
        .into_iter()
        .filter(|id| !fresh.contains(id))
        .collect();

    let mut report = DeriveReport::default();
    let mut named_series: BTreeSet<String> = BTreeSet::new();
    let groups = match grouped(
        conn,
        &engine,
        &roots,
        &frozen,
        watch,
        &mut named_series,
        &mut report,
    )? {
        Outcome::Done(groups) => groups,
        Outcome::Cancelled(step) => return Ok(Outcome::Cancelled(step)),
    };
    let (groups, joined) = joins::apply(conn, groups, &mut report)?;

    // Every series that produced *anything* gets a row, including one holding
    // only unattached chapters: `proposal.subject_id` references `series(id)`,
    // so the `attach-chapters` proposal has nothing to point at otherwise.
    let mut series_ids: BTreeMap<String, i64> = BTreeMap::new();
    for title in named_series {
        let id = reconcile_series(conn, &title)?;
        series_ids.insert(title, id);
    }
    report.series = series_ids.len();

    let mut written: BTreeSet<i64> = BTreeSet::new();
    // Each work whose hand document `write_work` could not read or apply this
    // pass, with the read-back's own message. The fold below refuses a pair
    // whose target is one; see its comment.
    let mut unread: BTreeMap<i64, String> = BTreeMap::new();
    // `(work_id, edition key)` pairs this pass wrote or found already written
    // — narrower than `written` alone, because a work in it can still be
    // holding an edition this pass never touched (`DeriveReport::kept_editions`).
    // `superseded` needs the narrower set: only these rows count as evidence
    // that a candidate's material has a home. `group.editions.keys()` is
    // exactly what `import::scanned`'s `Scope::Scan` arm writes into `edition`
    // for a work `write_work` writes, and what `unchanged` found for one it left.
    let mut written_editions: BTreeSet<(i64, String)> = BTreeSet::new();
    let placed = Placed::of(&groups)?;
    let to_write = u64::try_from(groups.len()).unwrap_or(u64::MAX);
    let mut works_written: u64 = 0;
    let mut work_ids: BTreeMap<GroupKey, i64> = BTreeMap::new();
    let mut hubs: BTreeMap<i64, String> = BTreeMap::new();
    for (key, group) in groups {
        watch.at(Step::Writing, works_written, Some(to_write));
        // Between works, each written in a transaction of its own, so a
        // cancel leaves none half-written.
        if watch.cancelled() {
            return Ok(Outcome::Cancelled(Step::Writing));
        }
        let series_id = *series_ids
            .get(&group.series)
            .ok_or_else(|| format!("no series row was reconciled for `{}`", group.series))?;
        let (work_id, hub) = write_work(
            conn,
            series_id,
            &group,
            &placed,
            documents,
            &mut unread,
            &mut report,
        )?;
        hubs.insert(work_id, hub);
        written_editions.extend(group.editions.keys().cloned().map(|key| (work_id, key)));
        written.insert(work_id);
        work_ids.insert(key, work_id);
        works_written += 1;
    }
    watch.at(Step::Writing, works_written, Some(to_write));
    // The tail reads and writes the whole store, so a cancelled derive leaves
    // all of it to the next one, as a stop does.
    if watch.cancelled() {
        return Ok(Outcome::Cancelled(Step::Writing));
    }
    watch.at(Step::Finishing, 0, None);

    // Once every work is written, as `fold::move_edition` carries rows onto
    // an edition it does not check stands; past the cancel check, so a
    // cancelled derive leaves the move to the next, which finds it the same way.
    joins::move_left_behind(
        conn,
        &written,
        &written_editions,
        &hubs,
        &joined.moved,
        documents,
        &mut report,
    )?;

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
    // **Where the document about to become an orphan sits** — read here,
    // before `absorb` runs, because `absorb`'s own `DELETE FROM work` removes
    // the `from_work` row itself: `hand::path_for` joins through `work`,
    // `edition`, `member` and `root` to resolve an address (it never reads
    // `migrated_document`), and once that row is gone there is nothing left
    // for it to join through. Captured after the delete instead, `path_for`
    // would find no `from_work` to query at all.
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
        // **Refused while the target's document did not read this pass.** The
        // hand-value clash check reads rows and cannot see what that document
        // holds, and a fold that carries a value can arm the target, after which
        // the fixed file is never read back. Left standing, the next scan
        // reads the fixed file and then folds.
        if let Some(why) = unread.get(&into_work) {
            report.unfolded_works += 1;
            report.unfoldable.push(format!(
                "work {from_work} was not folded into work {into_work}, whose hand document \
                 did not read on this pass ({why}); fix the file and scan again; nothing was \
                 moved and nothing was deleted"
            ));
            continue;
        }
        let document_path = crate::hand::path_for(conn, from_work)?;
        match crate::fold::absorb(conn, from_work, into_work)? {
            crate::fold::Outcome::Folded(moved) => {
                // **The target is read after the fold, unlike the path.** The
                // fold can carry the folded work's hand title onto it
                // (`fold::Moved::hand_carried`), and the orphan report names
                // the work by the title a person will find it under.
                let target: Option<(String, String)> = conn
                    .query_row(
                        "SELECT slug, title FROM work WHERE id = ?1",
                        [into_work],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(|e| format!("reading the absorbing work {into_work}: {e}"))?;
                // **A carried value must reach the target's document**, or a
                // rebuild from the files loses it: `reconcile` below writes
                // only a missing document and reports a present one that
                // disagrees as stale. Not under `Documents::Skip`, which writes
                // no file; a target whose document did not read never gets
                // here (see the refusal above). A failed write is
                // `reconcile`'s to report.
                if moved.hand_carried > 0 && documents == Documents::Reconcile {
                    crate::hand::save(conn, into_work)?;
                }
                report.emptied_works += 1;
                report.count_moved(&moved);
                // Only a completed fold orphans anything — a refused one
                // deletes nothing, per `Outcome::Refused`'s own doc. A path
                // of `None` means `from_work` had no document to strand
                // (an imported work, or one nobody ever hand-edited); a
                // target of `None` cannot happen for a work `superseded`
                // just named as a fold target, but is handled rather than
                // asserted away.
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

    // After the folds, so the host already holds whatever alignment a fold
    // carried onto it, and a person's among them keeps the solver's out.
    joins::write_alignments(conn, &joined.aligned, &work_ids, &mut report)?;

    // After the folds, so a work folded away this pass is gone rather than
    // marked or counted found again. Under both `Documents` modes: a
    // rolled-back preview predicts the mark as it predicts everything else.
    digest_held_members(conn, &roots, &frozen, &mut report.unreadable)?;
    (report.marked_missing, report.found_again) = mark_missing(conn)?;

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

    // Raised last, because every kind of proposal points at a series row and
    // this is the point at which every series this derive touched — including
    // one holding only unattached chapters — is guaranteed to have one.
    // `proposals::raise` is idempotent, so a rescan over unchanged facts opens
    // nothing a person has already dismissed or accepted.
    //
    // **The one call, and its count kept.** A second call anywhere else would
    // see every proposal this call just opened already sitting in `proposal`,
    // and `INSERT OR IGNORE` would report zero new rows for every one of them.
    // See `DeriveReport::raised`.
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
    // pins this.
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

    Ok(Outcome::Done(report))
}

/// Every root's entries resolved, hashed where they become members, and filed
/// into groups by [`place`], with each series a group or an unattached chapter
/// directory names added to `named_series`. Nothing is written but digests.
fn grouped(
    conn: &Connection,
    engine: &Engine,
    roots: &[Root],
    frozen: &HashSet<i64>,
    watch: &dyn Watch,
    named_series: &mut BTreeSet<String>,
    report: &mut DeriveReport,
) -> Result<Outcome<BTreeMap<GroupKey, Group>>, String> {
    let mut staged: Vec<Staged> = Vec::new();
    let mut chapter_dirs: BTreeSet<(i64, String)> = BTreeSet::new();

    // Hashing is shown from here, with no total yet: sizing the folders below
    // can take a while on a share, and the screen must not stay on the walk.
    watch.at(Step::Hashing, 0, None);

    // **Two passes over each root's entries, and the second is what stops a
    // volume being counted twice.** The fact layer records the container
    // directory *and* everything beneath it, so "each entry is one member"
    // makes a volume split into chapter subdirectories produce a member for
    // the volume and one per chapter. The first pass resolves every entry
    // once and notes which of them speak for their subtrees; the second drops
    // any whose ancestor does. See [`nested_in_claim`].
    let mut to_classify = Vec::with_capacity(roots.len());
    for root in roots {
        let read = if frozen.contains(&root.id) {
            frozen_entries(conn, engine, root)?
        } else {
            entries(conn, root.id)?
        };
        let mut seen = Vec::new();
        for entry in read {
            let Some(admitted) = admit(engine, root, entry, report) else {
                continue;
            };
            seen.push(admitted);
        }
        let claims = claims_of(&seen);
        let mut kept = Vec::new();
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
            let bytes = if hashes(&admitted) {
                // Sizing a folder lists it and stats each page, so the flag is
                // read before each one; nothing is written yet, so a cancel
                // here is a stop.
                if is_folder(&admitted.entry) && watch.cancelled() {
                    return Ok(Outcome::Cancelled(Step::Hashing));
                }
                Some(bytes_to_hash(root, &admitted.entry))
            } else {
                None
            };
            kept.push((admitted, bytes));
        }
        to_classify.push((root, kept));
    }

    // Every root is admitted and claimed before the first digest, so the
    // bytes to hash are known before the first is read. Nothing a root's classify
    // writes is read by another root's admit or claim: a digest lands on its
    // own root's `scan_entry` row, and claims are per root.
    let to_hash: u64 = to_classify
        .iter()
        .flat_map(|(_, kept)| kept)
        .filter_map(|(_, bytes)| *bytes)
        .sum();
    let mut hashed: u64 = 0;
    for (root, kept) in to_classify {
        for (admitted, bytes) in kept {
            if bytes.is_some() {
                watch.at(Step::Hashing, hashed, Some(to_hash));
            }
            if watch.cancelled() {
                return Ok(Outcome::Cancelled(Step::Hashing));
            }
            classify(
                conn,
                root,
                admitted,
                &mut Sorting {
                    staged: &mut staged,
                    chapter_dirs: &mut chapter_dirs,
                    named_series,
                    report,
                    frozen,
                },
            )?;
            // After the digest, and for a file found gone too, the figure the
            // total holds, so the count ends at the total.
            if let Some(bytes) = bytes {
                hashed += bytes;
            }
        }
    }
    watch.at(Step::Hashing, hashed, Some(to_hash));
    report.unattached_chapter_dirs = chapter_dirs.len();
    place(conn, staged, named_series, report).map(Outcome::Done)
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
/// **The third element keeps volume-less entries apart, and `None` is not a
/// group.** An entry the rules gave neither a volume nor a chapter has
/// nothing to be grouped *by*, and keying those on `(series, NULL)` would
/// merge every one of them in a series into a single page sequence. Merging
/// distinct material into one page sequence is the failure
/// [`DeriveReport::merged`]'s doc calls out, since member order is page order
/// and every alignment offset derives from it.
///
/// `rel_path` rather than the absolute path, so one volume-less work present
/// in two roots is still one work — which is what
/// `a_volume_less_work_still_matches_itself_on_the_next_derive` pins, and the
/// same reason [`Candidate::rel_path`] exists for ordering.
///
/// Entries that carry a chapter *and* a volume still group by that volume,
/// which is §7's chapter case and stays; an entry with a chapter and no
/// volume never reaches here at all ([`classify`] settles it first).
///
/// **No library.** Directories group together whichever libraries hold
/// them, so a book is one work with one reading position and an edition per
/// edition key: directories sharing a key share one edition
/// ([`DeriveReport::merged`]).
type GroupKey = (String, Option<u64>, Option<Alone>);

/// What a volume-less entry is grouped by ([`GroupKey`]): the title a rule
/// captured, a person naming the book as a captured volume does, so two
/// translations a rule titles alike are one work; else its own path. A
/// file's declared title groups nothing: no person chose it, and two books of
/// one name by one author are not one book. Two variants rather than one
/// string, so a title can never meet a path that spells it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Alone {
    Titled(String),
    Path(String),
}

/// One `scan_entry` row, as derivation reads it.
#[derive(Clone)]
struct Entry {
    rel_path: String,
    format: Option<String>,
    pages: Option<u32>,
    sha256: Option<String>,
    /// The title the file declares, `scan_entry.title`.
    title: Option<String>,
    /// `scan_entry.size`: an archive's bytes, and for a folder the
    /// directory's own size, which [`bytes_to_hash`] does not count by.
    size: u64,
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
    /// entries across every root in the store, so this is the only level at
    /// which the question has a single answer — see `member.root_id`'s own
    /// comment in migration 6, and spec C1 §5.1.
    root_id: i64,
    sha256: String,
    pages: u32,
    chapter: Option<f64>,
    /// A rule's `title` capture, or else the title the file declares.
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
    /// The same bytes under other directories, one per directory: one volume
    /// is one member however many directories hold it (overlapping-libraries
    /// spec §1), and [`write_roots`] gives each of them a `member_root` row so
    /// a grant on any of those directories reaches it. `path`, `rel_path` and
    /// `root_id` above are the primary copy, the one `member.path` names.
    copies: Vec<CopyAt>,
    /// Whether this derive reads the entry's directory frozen
    /// ([`frozen_entries`]), so [`drop_held`] keeps its held reading.
    frozen: bool,
    /// Whether a rule's literal `set_series` named this entry's series, which
    /// holds it where its name puts it against a match by content: a series
    /// read from a name, by a seeded rule or one written by hand, is what
    /// content overrules (spec §4 of the volume-joins design).
    held_by_rule: bool,
    /// The stored pair that brought this entry into another group's work
    /// ([`joins::apply`]), which [`write_joined_by`] writes onto its edition.
    joined_by: Option<i64>,
}

/// Where one more copy of a [`Candidate`]'s bytes sits. Named for its place
/// rather than `Copy`, which would shadow the prelude's trait in this module.
struct CopyAt {
    root_id: i64,
    path: String,
    /// The copy's own `scan_entry` row, which may sit at another path than
    /// the primary's; [`drop_held`] finds it by this.
    rel_path: String,
    /// As [`Candidate::frozen`], for the copy's directory.
    frozen: bool,
}

/// One work, before it is a work: every entry across every root, whichever
/// library holds it, that resolved to the same `(series, volume)`, split by
/// edition key. A copy is not one of them: [`place`] files it under its
/// primary, wherever its own rules would have put it.
struct Group {
    series: String,
    volume: Option<f64>,
    editions: BTreeMap<String, Vec<Candidate>>,
    /// The hub a host of a join takes where its work is new or its work's hub
    /// is no edition of its own: its best edition, by kind then key, which a
    /// joined edition's key sorting before it would otherwise take
    /// ([`document`]).
    hub_hint: Option<String>,
}

impl Group {
    /// Every path this group holds: each candidate's, and each of its copies',
    /// since a work may hold a volume under a directory other than the one
    /// walked first.
    fn paths(&self) -> impl Iterator<Item = &String> {
        self.editions
            .values()
            .flatten()
            .flat_map(|c| std::iter::once(&c.path).chain(c.copies.iter().map(|k| &k.path)))
    }
}

/// What this derive's groups hold, read before any group is written, so
/// every group's [`held_work`] reads the one answer.
struct Placed {
    /// Every path a group holds.
    paths: HashSet<String>,
    /// The `content_key` each edition of a group would hash to as its hub,
    /// with the group: the import finds the row holding that key by content
    /// for that group, so no other group takes the row by its address. Not a
    /// guest's edition: a host's work is never keyed by it, and the row its
    /// file left stays with whatever its own group left behind.
    claims: HashMap<String, GroupKey>,
}

impl Placed {
    fn of(groups: &BTreeMap<GroupKey, Group>) -> Result<Self, String> {
        let mut claims = HashMap::new();
        for (key, group) in groups {
            for (edition, candidates) in &group.editions {
                if candidates.iter().all(|c| c.joined_by.is_none()) {
                    let doc = document(group, Some(edition));
                    claims.insert(crate::identity::content_key(&doc)?, key.clone());
                }
            }
        }
        Ok(Self {
            paths: groups.values().flat_map(Group::paths).cloned().collect(),
            claims,
        })
    }
}

/// The four things [`classify`] accumulates into, and the directories this
/// derive reads frozen, gathered so it takes one argument rather than five.
///
/// `clippy::too_many_arguments` is on by default rather than merely pedantic
/// and refuses at eight, which is exactly what `classify` would take; a struct
/// of borrows is the shape `taiyaku-cli`'s `AlignWrite` already uses for the
/// same refusal, with shared borrows where these are mutable.
struct Sorting<'a> {
    staged: &'a mut Vec<Staged>,
    chapter_dirs: &'a mut BTreeSet<(i64, String)>,
    named_series: &'a mut BTreeSet<String>,
    report: &'a mut DeriveReport,
    frozen: &'a HashSet<i64>,
}

/// The `scan_entry` columns [`entry_of`] reads, first in a select and in order.
const ENTRY_COLUMNS: &str = "rel_path, format, pages, sha256, title, size";

fn entry_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        rel_path: r.get(0)?,
        format: r.get(1)?,
        pages: r.get(2)?,
        sha256: r.get(3)?,
        title: r.get(4)?,
        size: u64::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
    })
}

/// Every `scan_entry` under one root, in path order.
fn entries(conn: &Connection, root_id: i64) -> Result<Vec<Entry>, String> {
    conn.prepare(&format!(
        "SELECT {ENTRY_COLUMNS} FROM scan_entry WHERE root_id = ?1 ORDER BY rel_path"
    ))
    .and_then(|mut stmt| stmt.query_map([root_id], entry_of)?.collect())
    .map_err(|e| format!("reading the facts for root {root_id}: {e}"))
}

/// Every `scan_entry` under a paused root, in path order, so a derive hashes
/// none of it and reads each held file by the reading its works hold (O320).
/// Held is each digest of a member whose `member_root` names the root, a
/// copy's too. A chapter-only entry passes as it is; else one whose held
/// reading is held, read by it; else one whose digest is; else one of no
/// pages. Any other entry waits, with the chapter-only entries it claims. Its
/// works can still change: by a folder the walk could not list whole, a rules
/// save, or another directory's file joining one (spec §7, §8).
fn frozen_entries(conn: &Connection, engine: &Engine, root: &Root) -> Result<Vec<Entry>, String> {
    type Row = (
        Entry,
        Option<String>,
        Option<String>,
        Option<u32>,
        Option<String>,
    );
    let failed = |e: rusqlite::Error| format!("reading what root {}'s works hold: {e}", root.id);
    let held: HashSet<String> = conn
        .prepare(
            "SELECT m.sha256 FROM member m JOIN member_root mr ON mr.member_id = m.id
              WHERE mr.root_id = ?1",
        )
        .and_then(|mut stmt| stmt.query_map([root.id], |r| r.get(0))?.collect())
        .map_err(failed)?;
    let rows: Vec<Row> = conn
        .prepare(&format!(
            "SELECT {ENTRY_COLUMNS}, held_sha256, held_format, held_pages, held_title
               FROM scan_entry WHERE root_id = ?1 ORDER BY rel_path"
        ))
        .and_then(|mut stmt| {
            stmt.query_map([root.id], |r| {
                Ok((entry_of(r)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?))
            })?
            .collect()
        })
        .map_err(failed)?;
    let saved: Vec<Entry> = rows.iter().map(|row| row.0.clone()).collect();
    let mut kept = Vec::with_capacity(rows.len());
    let mut chapters = HashSet::new();
    for (mut entry, held_sha256, format, pages, title) in rows {
        let chapter = chapter_only(&engine.resolve(root, &entry.rel_path));
        if chapter {
            chapters.insert(entry.rel_path.clone());
        } else {
            match held_sha256.filter(|sha256| held.contains(sha256)) {
                // Before the digest: by the drop, a held reading the root holds
                // is what its works hold for this row, and bytes hashed since
                // may equal another member's.
                Some(sha256) => {
                    (entry.sha256, entry.format, entry.pages, entry.title) =
                        (Some(sha256), format, pages, title);
                }
                None if entry
                    .sha256
                    .as_ref()
                    .is_some_and(|sha256| held.contains(sha256)) => {}
                // After the held reading: a member truncated since its write
                // would pass at no pages, not the reading it was written from.
                None if holds_no_pages(&entry) => {}
                None => continue,
            }
        }
        kept.push(entry);
    }
    // What waits is what the rows claim as the walk saved them, less what
    // passed: a new folder volume, say. A chapter-only entry passed beneath it
    // would stand under no claim, counted unattached and naming a series no
    // work holds. Proposals read `entries`, where the volume still claims it.
    let passed: HashSet<&str> = kept.iter().map(|entry| entry.rel_path.as_str()).collect();
    let mut thrown_away = DeriveReport::default();
    let admitted: Vec<Seen> = saved
        .into_iter()
        .filter_map(|entry| admit(engine, root, entry, &mut thrown_away))
        .collect();
    let waiting: BTreeSet<String> = claims_of(&admitted)
        .into_iter()
        .filter(|claim| !passed.contains(claim.as_str()))
        .collect();
    kept.retain(|entry| {
        !(chapters.contains(&entry.rel_path) && nested_in_claim(&waiting, &entry.rel_path))
    });
    Ok(kept)
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
    /// Nothing, because there is nothing there: no pages, or a container
    /// whose pages the walk could not count. [`classify`] reports it in
    /// [`DeriveReport::unreadable`], under [`Unreadable::no_pages`] or
    /// [`Unreadable::uncounted`].
    Nothing,
}

impl Seen {
    /// What this entry produces, given the entries beneath it that produce
    /// something — see [`Produces`].
    ///
    /// **"What will this entry produce", not "could it be a member".**
    ///
    /// The order of the tests is the whole rule:
    ///
    /// 1. A chapter with no volume is §7's case, settled before pages are
    ///    looked at because it needs none.
    /// 2. No pages, no anything. A directory holding archives rather than
    ///    loose images gets `pages = Some(0)` from the walk, and
    ///    `builtin-volume` reads `v01` out of its *name* regardless. Taken as
    ///    a member, the page-less directory would suppress the archive that
    ///    actually held the pages, and `member_digest` over it is the digest
    ///    of nothing, the same hash for every such directory, so the groups
    ///    that survived would collide on `content_key` and land on one row.
    /// 3. **A volume-less container defers to whatever is inside it.** A rule
    ///    marked no volume here, so this entry is a name in a path and not a
    ///    thing anybody released; if something beneath it is a member, that
    ///    is what the material is. Without this clause the *container* would
    ///    be taken as the member and everything under it dropped. That is
    ///    precisely the failure C4 was settled to stop: member order is page
    ///    order, so one member of five editions is five works concatenated.
    /// 4. Otherwise a member — including a marked volume, which speaks for
    ///    its own chapter subdirectories even though they are members'
    ///    material. It is the one place a container *does* win:
    ///    `Series - v01/c001` is a piece of `v01`, not a work beside it, and
    ///    the rules said so by resolving a volume.
    ///
    /// **A series is required too**: an entry with no series has no group to
    /// join and can never be a member. Unreachable here in any case —
    /// [`admit`] has already refused one.
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
/// own entry is in this set too once it has claimed, so `!claims.is_empty()`
/// would make the root defer to itself. It is also what keeps
/// [`claims_of`]'s two passes agreeing: the non-empty arm cannot match
/// `rel_path` itself, because a path does not start with itself followed by a
/// separator, and this arm must not either.
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
/// that second case under the seeded rules.
fn admit(engine: &Engine, root: &Root, entry: Entry, report: &mut DeriveReport) -> Option<Seen> {
    if !is_readable(entry.format.as_deref()) {
        let token = entry.format.unwrap_or_else(|| UNKNOWN_FORMAT.to_string());
        *report.unreadable.by_format.entry(token).or_default() += 1;
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
/// rule layer gives a better one, and it needs nothing but `rel_path` prefixes.
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
/// ancestor, and is reachable.** [`admit`] refuses an entry with no series,
/// but any `set_series` rule broad enough to match `""` gives it one: a
/// `prefix` rule with an empty pattern is what `proposals::accept` writes.
/// The root then claims the subtree like any other container, which collapses
/// that library to one work and says so through `suppressed` — the honest
/// reading of a rule that says every path is one series.
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

/// Whether [`classify`] reads this entry's bytes: it is not chapter-only, it
/// holds pages, and no digest is held for it. A watched derive sums the bytes
/// to hash over the same test, so its count and its total cannot disagree.
fn hashes(seen: &Seen) -> bool {
    !chapter_only(&seen.resolved)
        && seen.entry.pages.is_some_and(|pages| pages > 0)
        && seen.entry.sha256.is_none()
}

/// Whether an entry holds no pages: a container found empty or not counted,
/// or a file in a format nothing reads, which the walk gives no count. Such an
/// entry makes no structure: [`admit`] or [`classify`] counts it, nothing
/// hashes or writes it, and it claims nothing beneath it.
fn holds_no_pages(entry: &Entry) -> bool {
    !entry.pages.is_some_and(|pages| pages > 0)
}

/// A chapter with no volume, §7's case: [`classify`] counts it and never
/// hashes it or makes it a member.
fn chapter_only(resolved: &crate::rules::Resolved) -> bool {
    resolved.volume.is_none() && resolved.chapter.is_some()
}

/// Whether the walk recorded this entry as a folder, which its digest reads
/// page file by page file.
fn is_folder(entry: &Entry) -> bool {
    entry.format.as_deref() == taiyaku_read::Format::Dir.token()
}

/// The bytes [`digest`] reads for an entry [`hashes`] admits. An archive is
/// read whole, so its walked size counts. A folder is read page by page, so
/// the sizes of the page files `taiyaku_read::member_page_sizes` lists count:
/// it lists them as `member_digest` does, so the two cannot disagree. A folder
/// that cannot be listed counts 0, since its digest cannot read it either.
fn bytes_to_hash(root: &Root, entry: &Entry) -> u64 {
    if !is_folder(entry) {
        return entry.size;
    }
    taiyaku_read::member_page_sizes(&root.path.join(&entry.rel_path))
        .map_or(0, |sizes| sizes.iter().sum())
}

/// Stage one admitted entry for [`place`], or count it as one more reason a
/// directory is an unattached chapter directory, or as nothing at all.
///
/// The order of the tests below is load-bearing. The chapter case is settled
/// **before** anything is hashed, because §7's answer for it is that it
/// produces no work — so paying a digest for it would be paying for a member
/// that is never written, on every entry of a directory of chapters.
fn classify(
    conn: &Connection,
    root: &Root,
    seen: Seen,
    into: &mut Sorting<'_>,
) -> Result<(), String> {
    let hashing = hashes(&seen);
    let Seen {
        entry,
        series,
        resolved,
    } = seen;

    // §7: which chapter files constitute volume 3 is a question only the
    // alignment answers. Count the directory and leave it visibly unattached;
    // `proposals::raise` is what asks about it. Chapters that sit *inside* a
    // volume never reach here — `nested_in_claim` dropped them, because that
    // question has already been answered for them.
    if resolved.volume.is_none() && resolved.chapter.is_some() {
        into.named_series.insert(series);
        into.chapter_dirs
            .insert((root.id, parent_of(&entry.rel_path).to_string()));
        return Ok(());
    }

    // A readable container whose pages the walk could not count —
    // `pages_of` swallowed the refusal so one truncated download would not
    // leave a library unscanned, and `member.pages` is NOT NULL, so there is
    // nothing honest to write. Named, because it is one file with something
    // wrong with it: see [`Unreadable::uncounted`] for the two things that
    // can be.
    let Some(pages) = entry.pages else {
        into.report
            .unreadable
            .uncounted
            .push(root.path.join(&entry.rel_path).display().to_string());
        return Ok(());
    };
    // **And a container that listed cleanly and held no pages**, which is not
    // the same fact and reaches the same answer: a volume directory holding
    // `.cbz` files has `pages = Some(0)`, and a member of no pages is a
    // member of nothing — `member_digest` hashes it to the digest of nothing,
    // which is the same hash for every such directory, so writing it would
    // collide every one of them onto one work. The archives *inside* it are
    // the members, and [`Seen::claims_subtree`] is what stops this entry
    // suppressing them. Counted rather than named: this is a layout, not a
    // fault.
    if pages == 0 {
        into.report.unreadable.no_pages += 1;
        return Ok(());
    }
    // Hashed only where `hashes` says, so no bytes are read that the total
    // left out. Past the tests above, an entry it passes over holds a digest.
    let held = if hashing {
        digest(conn, root, &entry)?
    } else {
        entry.sha256.clone()
    };
    let Some(sha256) = held else {
        into.report
            .unreadable
            .vanished
            .push(root.path.join(&entry.rel_path).display().to_string());
        return Ok(());
    };

    let path = root
        .path
        .join(&entry.rel_path)
        .to_string_lossy()
        .into_owned();
    let titled = resolved.title.clone();
    into.staged.push(Staged {
        candidate: Candidate {
            path,
            rel_path: entry.rel_path,
            root_id: root.id,
            sha256,
            pages,
            chapter: resolved.chapter,
            title: resolved.title.or(entry.title),
            kind: resolved.kind,
            language: resolved.language,
            copies: Vec::new(),
            frozen: into.frozen.contains(&root.id),
            // `admit` passed only an entry a rule gave a series, so a series
            // no capture decided came from a rule's literal `set_series`.
            held_by_rule: !resolved.series_was_captured,
            joined_by: None,
        },
        series,
        volume: resolved.volume,
        edition_key: resolved
            .edition_key
            .unwrap_or_else(|| DEFAULT_EDITION_KEY.to_string()),
        titled,
    });
    Ok(())
}

/// One entry [`classify`] admitted and hashed, with where its own directory's
/// rules file it. [`place`] decides whether it is a member or a copy.
struct Staged {
    candidate: Candidate,
    series: String,
    volume: Option<f64>,
    edition_key: String,
    /// The title a rule captured, apart from the one the file declares that
    /// [`Candidate::title`] falls back to: only this one groups.
    titled: Option<String>,
}

/// File every staged entry once every root has been walked. **A byte-identical
/// file under another directory is a copy, not more pages**, wherever that
/// directory's rules would file it (the ruling of 2026-11-04): as a member it
/// would show twice in one edition, or make a second group whose hub hashes
/// the same `content_key`, and two groups would write one row in turn, the
/// last one's directory holding it. The primary's rules name it. A volume
/// that shares only some of its files splits: the rest stay where their own
/// rules put them.
fn place(
    conn: &Connection,
    staged: Vec<Staged>,
    named_series: &mut BTreeSet<String>,
    report: &mut DeriveReport,
) -> Result<BTreeMap<GroupKey, Group>, String> {
    let mut by_sha: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (at, entry) in staged.iter().enumerate() {
        by_sha
            .entry(entry.candidate.sha256.as_str())
            .or_default()
            .push(at);
    }
    let mut copies: Vec<Vec<usize>> = vec![Vec::new(); staged.len()];
    let mut is_copy = vec![false; staged.len()];
    for (sha256, mut at) in by_sha {
        let first = staged[at[0]].candidate.root_id;
        if at.iter().all(|&i| staged[i].candidate.root_id == first) {
            continue;
        }
        // The primary is the first in walk order (roots by id) once held
        // copies are put first. Two such files under one directory stay two
        // members: a `member_root` row is per directory, so a second copy
        // there would have nothing to record it.
        keep_primaries(conn, sha256, &staged, &mut at)?;
        let mut primaries: Vec<usize> = Vec::new();
        for i in at {
            let root = staged[i].candidate.root_id;
            let primary = primaries.iter().copied().find(|&p| {
                staged[p].candidate.root_id != root
                    && copies[p]
                        .iter()
                        .all(|&k| staged[k].candidate.root_id != root)
            });
            match primary {
                Some(p) => {
                    copies[p].push(i);
                    is_copy[i] = true;
                }
                None => primaries.push(i),
            }
        }
    }

    let mut slots: Vec<Option<Staged>> = staged.into_iter().map(Some).collect();
    let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
    for at in 0..slots.len() {
        if is_copy[at] {
            continue;
        }
        let Some(Staged {
            mut candidate,
            series,
            volume,
            edition_key,
            titled,
        }) = slots[at].take()
        else {
            continue;
        };
        for &k in &copies[at] {
            if let Some(copy) = slots[k].take() {
                candidate.copies.push(CopyAt {
                    root_id: copy.candidate.root_id,
                    path: copy.candidate.path,
                    rel_path: copy.candidate.rel_path,
                    frozen: copy.candidate.frozen,
                });
            }
        }
        named_series.insert(series.clone());
        // No volume here means no chapter either — `classify` returned for
        // that case — so this is the entry the rules placed in a series and
        // nothing more: keyed to the title a rule gave it, or else to itself.
        // See [`GroupKey`].
        let alone = volume.is_none().then(|| match titled {
            Some(title) => Alone::Titled(title),
            None => Alone::Path(candidate.rel_path.clone()),
        });
        let key = (series.clone(), volume.map(f64::to_bits), alone);
        let group = groups.entry(key).or_insert_with(|| Group {
            series,
            volume,
            editions: BTreeMap::new(),
            hub_hint: None,
        });
        let edition = group.editions.entry(edition_key).or_default();
        // Counted here rather than from the finished group, because "how many
        // entries joined something" and "how many members an edition ended
        // with" differ by one per edition and this is the number that means
        // something: one entry joining another's page sequence. A copy joins
        // no page sequence and is not one.
        if !edition.is_empty() {
            report.merged += 1;
        }
        edition.push(candidate);
    }
    Ok(groups)
}

/// Whether `taiyaku-read` can read what this `scan_entry.format` token names,
/// as [`taiyaku_read::Format::is_readable`] answers for the token's format.
///
/// A NULL `format`, a file whose bytes matched no signature, names no format,
/// so it is not readable, the answer `Format::Unknown` gives.
fn is_readable(format: Option<&str>) -> bool {
    format
        .and_then(taiyaku_read::Format::from_token)
        .is_some_and(taiyaku_read::Format::is_readable)
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
    // Bytes equal to the held reading's make it the current one, so it goes
    // here, safely even if this derive is then cancelled: [`drop_held`] runs
    // only where the entry's work is written, which an unchanged reading
    // alone does not cause (O320).
    conn.execute(
        "UPDATE scan_entry
            SET held_sha256 = NULL, held_format = NULL, held_pages = NULL, held_title = NULL
          WHERE root_id = ?1 AND rel_path = ?2 AND held_sha256 = ?3",
        rusqlite::params![root.id, entry.rel_path, hash],
    )
    .map_err(|e| format!("dropping the held reading of {}: {e}", entry.rel_path))?;
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
/// matched branch leave it alone: were either to move this column, the next
/// pass would find nothing under the scanned name, mint a second series and
/// move every volume onto it.
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
///
/// **Across the store.** The same title in two libraries is one series, so
/// a work with editions in both has one series to sit in (overlapping-libraries
/// spec §3.2). The row found is the oldest of that `sort_title`: the one whose
/// id a dismissed proposal and a reader's links already name, where more than
/// one exists.
fn reconcile_series(conn: &Connection, sort_title: &str) -> Result<i64, String> {
    let held: Option<i64> = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = ?1 ORDER BY id LIMIT 1",
            [sort_title],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("looking up the series `{sort_title}`: {e}"))?;
    let series_id = match held {
        Some(id) => id,
        None => {
            conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [sort_title])
                .map_err(|e| format!("inserting the series `{sort_title}`: {e}"))?;
            conn.last_insert_rowid()
        }
    };
    // **Only onto a series holding no title — O114.** A correction through
    // `librarydoc::set_series_titles` replaces the whole set and may drop this
    // text; writing it back on every pass would return a removed spelling as a
    // second primary title. A series minted just above holds none. Asked
    // first, because the insert takes the write lock even where it inserts
    // nothing (O272).
    let titled = held.is_some()
        && conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM series_title WHERE series_id = ?1)",
                [series_id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| format!("reading the titles of series `{sort_title}`: {e}"))?;
    if !titled {
        conn.execute(
            "INSERT INTO series_title (series_id, title, is_primary)
             SELECT ?1, ?2, 1
              WHERE NOT EXISTS (SELECT 1 FROM series_title WHERE series_id = ?1)",
            rusqlite::params![series_id, sort_title],
        )
        .map_err(|e| format!("recording the title of series `{sort_title}`: {e}"))?;
    }
    Ok(series_id)
}

/// The document [`write_work`] imports for one group, with `held_hub` the hub
/// of the work [`held_work`] found at the group's address.
fn document(group: &Group, held_hub: Option<&str>) -> Sidecar {
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

    // Only a genuinely new work chooses one, and then the choice is arbitrary
    // but permanent: its [`Group::hub_hint`], else whichever key sorts first
    // today. **The hub is fixed when the work is created** — `identity.rs`'s
    // own doc says so, and it is load-bearing: `content_key` is a hash over
    // the hub edition's members, so a work whose hub changed would get a
    // different key, `import` would mint a second row, and the reader's
    // progress and notes would be orphaned on the first.
    //
    // "The key that sorts first" is stable only while the set does not change,
    // so it is consulted once. Once the hub's files are gone, or a guest holds
    // its key, the host's [`Group::hub_hint`], else the first key left, is
    // hashed instead, every derive alike; never a guest's, whose files hash to
    // its own work's key. A scan leaves `work.hub` as it was. The row
    // [`held_work`] found takes that key; a work is minted only where it found none.
    let own = |key: &str| {
        group
            .editions
            .get(key)
            .is_some_and(|candidates| candidates.iter().all(|c| c.joined_by.is_none()))
    };
    let hub_key = held_hub
        .filter(|held| own(held))
        .or(group.hub_hint.as_deref())
        .filter(|held| editions.contains_key(*held))
        .map_or_else(
            || {
                editions
                    .keys()
                    .next()
                    .cloned()
                    .expect("a group exists only once a member has been pushed into it")
            },
            str::to_string,
        );
    let title = title_of(group, &hub_key);

    Sidecar {
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
        // **Empty because a walk has nothing to say here, and empty does not
        // mean deleted.** Solving is `taiyaku align`'s, not the scanner's, so
        // this document declares no correspondence — and `import::scanned`
        // reads that as silence rather than as an answer: it leaves the work's
        // `alignment`, `run` and `page_flag` rows exactly as they were. See
        // `import::scanned`'s own doc for the rule and for what it costs — an
        // edge naming an edition a later rule edit removed dangles, resolvably
        // rather than dangerously.
        alignments: Vec::new(),
    }
}

/// A held work [`write_work`] would leave as it stands.
struct Unchanged {
    work_id: i64,
    /// The hub its `content_key` is hashed over.
    hub: String,
    /// Its editions this derive does not name ([`DeriveReport::kept_editions`]).
    kept: usize,
}

/// The work `group` is already written as, or `None` where [`write_work`]
/// would change anything on it — read without the write lock, so a derive
/// takes none for a work it leaves as it stands (O272). Each check is one
/// of `write_work`'s writes with nothing left to do, and whatever is not
/// checked counts as a change, so a doubt costs a rewrite, never a skipped one.
/// A member's chapter is compared with what the import's restore by digest
/// ([`crate::import::held_member_meta`]) and then [`write_chapters`] leave.
fn unchanged(
    conn: &Connection,
    series_id: i64,
    group: &Group,
    placed: &Placed,
) -> Result<Option<Unchanged>, String> {
    // One read snapshot, so a save committing between two of these reads
    // cannot pair halves of two states. `None` inside a caller's transaction
    // (a rule preview), whose own view is already one state.
    let _snapshot = crate::one_snapshot(conn)
        .map_err(|e| format!("reading the held work of `{}`: {e}", group.series))?;
    let held = held_work(conn, series_id, group, placed)?;
    let doc = document(group, held.as_ref().map(|(_, hub, _)| hub.as_str()));
    let key = crate::identity::content_key(&doc)?;
    if held
        .as_ref()
        .is_some_and(|(_, _, held_key)| *held_key != key)
    {
        return Ok(None);
    }
    let Some(work_id) = crate::import::find_by_key(conn, &key)? else {
        return Ok(None);
    };

    let (series, series_hand, volume, volume_hand): (
        Option<i64>,
        Option<i64>,
        Option<f64>,
        Option<f64>,
    ) = conn
        .prepare_cached(
            "SELECT series_id, series_id_hand, volume, volume_hand FROM work WHERE id = ?1",
        )
        .and_then(|mut q| {
            q.query_row([work_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
        })
        .map_err(|e| format!("reading work {work_id}: {e}"))?;
    if series != Some(series_hand.unwrap_or(series_id)) || volume != volume_hand.or(group.volume) {
        return Ok(None);
    }
    if !crate::migrate::absorbed(conn, work_id, crate::migrate::LIBRARY)?
        && !matches!(crate::hand::load(conn, work_id), Ok(None))
    {
        return Ok(None);
    }

    let held_editions = held_editions_of(conn, work_id)?;
    let held_members = held_members_of(conn, work_id)?;
    let restored = crate::import::held_member_meta(conn, work_id)?;
    let mut kept = 0;
    let mut named = 0;
    for edition in &held_editions {
        let members = held_members.iter().filter(|m| m.edition_id == edition.id);
        let Some(candidates) = group.editions.get(&edition.key) else {
            kept += 1;
            if !members
                .into_iter()
                .all(|m| chapter_after(m.chapter.clone(), None) == m.chapter)
            {
                return Ok(None);
            }
            continue;
        };
        named += 1;
        let metadata = resolved_metadata(candidates);
        let Some(label_derived) = &edition.label_derived else {
            return Ok(None);
        };
        if edition.label != *edition.label_hand.as_ref().unwrap_or(label_derived)
            || edition.kind != edition.kind_hand.clone().or(metadata.kind)
            || edition.language != edition.language_hand.clone().or(metadata.language)
            || edition.joined_by != joined_by(candidates)
        {
            return Ok(None);
        }
        let members: Vec<&MemberRow> = members.collect();
        let ordered = in_page_order(candidates);
        if members.len() != ordered.len() {
            return Ok(None);
        }
        for (ordinal, (member, candidate)) in (0_i64..).zip(members.into_iter().zip(ordered)) {
            let directories: BTreeSet<i64> = std::iter::once(candidate.root_id)
                .chain(candidate.copies.iter().map(|k| k.root_id))
                .collect();
            if member.ordinal != ordinal
                || member.path != candidate.path
                || member.sha256 != candidate.sha256
                || member.pages != i64::from(candidate.pages)
                || member.root_id != Some(candidate.root_id)
                || member.directories != directories
                || chapter_after(
                    restored
                        .get(&member.sha256)
                        .map_or((None, None), crate::import::HeldMember::chapter),
                    candidate.chapter,
                ) != member.chapter
            {
                return Ok(None);
            }
        }
    }
    if named != group.editions.len() {
        return Ok(None);
    }
    Ok(Some(Unchanged {
        work_id,
        hub: doc.hub,
        kept,
    }))
}

/// One `edition` row of a work, as [`unchanged`] compares it.
struct EditionRow {
    id: i64,
    key: String,
    label: String,
    label_hand: Option<String>,
    label_derived: Option<String>,
    kind: Option<String>,
    kind_hand: Option<String>,
    language: Option<String>,
    language_hand: Option<String>,
    joined_by: Option<i64>,
}

fn held_editions_of(conn: &Connection, work_id: i64) -> Result<Vec<EditionRow>, String> {
    let failed = |e: rusqlite::Error| format!("reading the editions of work {work_id}: {e}");
    conn.prepare_cached(
        "SELECT id, key, label, label_hand, label_derived, kind, kind_hand,
                language, language_hand, joined_by
           FROM edition WHERE work_id = ?1",
    )
    .map_err(failed)?
    .query_map([work_id], |r| {
        Ok(EditionRow {
            id: r.get(0)?,
            key: r.get(1)?,
            label: r.get(2)?,
            label_hand: r.get(3)?,
            label_derived: r.get(4)?,
            kind: r.get(5)?,
            kind_hand: r.get(6)?,
            language: r.get(7)?,
            language_hand: r.get(8)?,
            joined_by: r.get(9)?,
        })
    })
    .map_err(failed)?
    .collect::<Result<_, _>>()
    .map_err(failed)
}

/// One `member` row of a work, with its directories, as [`unchanged`]
/// compares it.
struct MemberRow {
    edition_id: i64,
    ordinal: i64,
    path: String,
    sha256: String,
    pages: i64,
    /// `(chapter_declared, chapter_source)`.
    chapter: (Option<f64>, Option<String>),
    root_id: Option<i64>,
    directories: BTreeSet<i64>,
}

/// Every member of a work in edition and page order, with its directories.
fn held_members_of(conn: &Connection, work_id: i64) -> Result<Vec<MemberRow>, String> {
    let failed = |e: rusqlite::Error| format!("reading the members of work {work_id}: {e}");
    let mut directories: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    let rows = conn
        .prepare_cached(
            "SELECT member_root.member_id, member_root.root_id FROM member_root
               JOIN member ON member.id = member_root.member_id
               JOIN edition ON edition.id = member.edition_id
              WHERE edition.work_id = ?1",
        )
        .map_err(failed)?
        .query_map([work_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failed)?;
    for (member_id, root_id) in rows {
        directories.entry(member_id).or_default().insert(root_id);
    }
    conn.prepare_cached(
        "SELECT member.id, member.edition_id, member.ordinal, member.path, member.sha256,
                member.pages, member.chapter_declared, member.chapter_source,
                member.root_id
           FROM member JOIN edition ON edition.id = member.edition_id
          WHERE edition.work_id = ?1
          ORDER BY member.edition_id, member.ordinal",
    )
    .map_err(failed)?
    .query_map([work_id], |r| {
        Ok(MemberRow {
            directories: directories.remove(&r.get::<_, i64>(0)?).unwrap_or_default(),
            edition_id: r.get(1)?,
            ordinal: r.get(2)?,
            path: r.get(3)?,
            sha256: r.get(4)?,
            pages: r.get(5)?,
            chapter: (r.get(6)?, r.get(7)?),
            root_id: r.get(8)?,
        })
    })
    .map_err(failed)?
    .collect::<Result<_, _>>()
    .map_err(failed)
}

/// Build one group's document and hand it to
/// [`crate::import::scanned_inserting`], then write the columns the document
/// cannot carry — `work.series_id`, `work.volume`, a new row's `work.slug`
/// and, through [`write_chapters`], `member.chapter_declared` — and apply a
/// hand document at this work's address that nothing has absorbed.
/// A held work [`unchanged`] finds already written is left as it stands.
///
/// Returns the `work_id` it resolved, written or left, for [`superseded`], and
/// the hub its `content_key` is hashed over, for [`joins::move_left_behind`].
fn write_work(
    conn: &Connection,
    series_id: i64,
    group: &Group,
    placed: &Placed,
    documents: Documents,
    unread: &mut BTreeMap<i64, String>,
    report: &mut DeriveReport,
) -> Result<(i64, String), String> {
    report.editions += group.editions.len();
    report.members += group.editions.values().map(Vec::len).sum::<usize>();
    // **A work with nothing to write takes no write lock — O272.** Every pass
    // below would rewrite a held work's rows, and the `BEGIN IMMEDIATE` holds
    // the lock a page turn's save waits for, so a scan where no file changed
    // would hold it work by work for the whole derive.
    //
    // **Nor is a held reading dropped here**, which would take that lock
    // (O320). An entry hashed equal to its held reading had it cleared by
    // [`digest`].
    if let Some(held) = unchanged(conn, series_id, group, placed)? {
        report.kept_editions += held.kept;
        report.unrecognised_kinds += group
            .editions
            .values()
            .filter(|candidates| resolved_metadata(candidates).refused)
            .count();
        report.works += 1;
        return Ok((held.work_id, held.hub));
    }
    // **One commit for the work, not one per statement — O210.** Every commit
    // waits for the disk. Everything below runs on `conn` inside this scope —
    // `Nested` is a state of the connection, not a second handle — and the
    // import's own `Nested`, like `librarydoc::apply`'s, nests in it as a
    // `SAVEPOINT`. Opened before `held_work` reads, so the address it finds is
    // the one the import writes over.
    let mut tx = crate::Nested::open(conn, "write_work")
        .map_err(|e| format!("starting to write a work of `{}`: {e}", group.series))?;
    let held = held_work(conn, series_id, group, placed)?;
    let doc = document(group, held.as_ref().map(|(_, hub, _)| hub.as_str()));
    // **Counted before the import, because afterwards the row already holds
    // the new key and the move is unobservable.** `content_key` is recomputed
    // here rather than returned from `import` — a sha256 over a handful of
    // hex digests, against `import::scanned_inserting`'s signature answering
    // three questions instead of two.
    //
    // **`rekeyed_from` is captured in the same breath, for
    // [`crate::hand::rename_from`] near the end of this function.**
    // `held_key` is this row's `content_key` before `import::scanned_inserting`
    // overwrites it — the only place that value is still readable — so it is
    // saved here rather than re-derived from `work.content_key` after the
    // fact, which is what a caller with no memory of `held` would have to do.
    let mut rekeyed_from = None;
    if let Some((held_id, _, held_key)) = &held
        && *held_key != crate::identity::content_key(&doc)?
    {
        report.rekeyed += 1;
        rekeyed_from = Some((held_key.clone(), crate::hand::path_for(conn, *held_id)?));
    }
    // The second identity probe: this work's address, for a work that has no
    // document to be addressed by. Without it, the changed files above would
    // mint a second row and leave the reader's progress on the first.
    let (work_id, inserted) =
        crate::import::scanned_inserting(conn, &doc, held.map(|(id, _, _)| id))?;
    // **Counted after the import, where `rekeyed` above had to be counted
    // before it.** The two are not the same shape: a `content_key` is
    // overwritten by the import, so the old value is gone once it returns,
    // while an edition it left standing is still standing — and the row it is
    // standing on is `work_id`, which content may have resolved to something
    // other than the `held` address probed above. Asking afterwards is
    // therefore both simpler and exact.
    report.kept_editions += kept_editions(conn, work_id, &doc)?;

    // **Before the read-back below.** `write_roots` only issues
    // `UPDATE member SET root_id = ?` from the group's own candidates, matched
    // on `(edition key, ordinal)` — `import::scanned_inserting`, just above, is the
    // only thing either depends on, so nothing here needs the guard or the
    // read-back to have run first. The read-back does need this to have run
    // first: on a work being minted for the first time, `member.root_id` would
    // still be NULL the moment `hand::load` calls `hand::path_for`, whose
    // `JOIN root ON root.id = member.root_id` would then match nothing —
    // `path_for` would answer `None`, `hand::load` `Ok(None)`, and a correction
    // sitting right beside the archives would be silently skipped until a
    // *second* derive. That would break the store spec §4.4 claim: losing the
    // database was supposed to cost one rescan, not two.
    //
    // **`write_chapters` stays below, and that is not an oversight.** It looks
    // like the same shape — another targeted update keyed on
    // `(edition key, ordinal)` from `import::scanned_inserting`'s own rows — but it
    // has a real ordering dependency the read-back's `apply` call creates:
    // `librarydoc::apply`'s `apply_members` clears *every* member's chapter for
    // this work unconditionally, then rewrites only the entries the hand
    // document names, by `sha256`. Running `write_chapters` before that guard
    // would have its scanner-derived chapters wiped right back to NULL for
    // every member the document does not mention. Running it after, as it
    // already does, lets it fill exactly those NULLs back in — its own
    // guard ([`chapter_after`]: only a number with no source or
    // `SCANNER_CHAPTER_SOURCE` moves) is what stops it from also overwriting a
    // member the document *did* set. `write_roots` has no such
    // interaction: nothing the guard or the read-back writes touches
    // `member.root_id`.
    write_roots(conn, work_id, group, report)?;
    // In the work's first scope, so it commits with the rows written from the
    // current readings and a cancel before this work keeps them held.
    drop_held(conn, group)?;

    // **The document follows the row here, after `write_roots` and after the
    // work's first commit — never inside it.** A rollback that left the file
    // renamed would strand it under a key the database never adopted: this
    // module's own defect, inverted, and harder to see, because
    // `hand::reconcile` would then read every root as clean. **So a rekeyed
    // work is the one that commits twice**: the import and `write_roots` here,
    // before the rename, and the rest of the work in a second scope after it,
    // so the read-back below still finds the document at the address the
    // rename gave it. Inside a caller's own transaction — a rule save's
    // derive runs in one — that first commit is a `RELEASE`.
    //
    // **Why here and not in `import_with`.** `hand::path_for` resolves a
    // work's address through `member.root_id`, and for the one member whose
    // bytes just changed — which, for a single-file volume, is every member
    // the hub edition has — `replace_editions`' delete-and-reinsert leaves
    // that column NULL: `held_member_meta` restores it by matching the old and
    // new rows on `sha256`, and a repacked archive's hash is exactly what
    // changed. `write_roots`, just above, is what restores it, by `(edition
    // key, ordinal)` instead — and it runs *after* `import::scanned_inserting`
    // returns, so it cannot have run yet at any point inside `import_with`.
    // Calling the rename there would, for a work with exactly one member, find
    // `path_for` answering `None` and leave the document unmoved, reported as
    // an orphan, which `a_repacked_volume_takes_its_document_with_it` catches.
    // Waiting until here is what lets one derive do what the store spec §4.4
    // asks of it.
    if let Some((old_key, held_at)) = rekeyed_from
        && documents == Documents::Reconcile
    {
        tx.commit()
            .map_err(|e| format!("committing work {work_id} before its rekey: {e}"))?;
        // The address before the import, when the document is there: a scan
        // that carried it into a nested directory (`scan::carry_documents`)
        // left it there, and this rekey can readdress the work back out.
        // Otherwise the old key in the directory the work is addressed in now,
        // where the document stays when a directory that keeps no documents
        // refused the carry.
        let old = match held_at.filter(|at| at.is_file()) {
            Some(at) => Some(at),
            None => crate::hand::path_for(conn, work_id)?
                .map(|now| now.with_file_name(crate::hand::file_name(&old_key))),
        };
        if let Some(old) = old {
            crate::hand::rename_from(conn, work_id, &old)?;
        }
        tx = crate::Nested::open(conn, "write_work")
            .map_err(|e| format!("resuming work {work_id} after its rekey: {e}"))?;
    }

    // The `work` columns the sidecar format has no field for, which are also
    // the two the hub lookup above reads back — and, for the same reason,
    // `edition.kind` and `edition.language`, which the format has no field for
    // either. See [`write_edition_metadata`], which is where that second pair's
    // own argument for being here lives. Written after the import rather than
    // before, because a brand-new work has no id until then.
    //
    // **Hand metadata survives a re-derivation.** `librarydoc::apply` —
    // reached through `migrate::work`, §4.4, or through the read-back below —
    // is the documented mechanism for setting `work.volume` and
    // `work.series_id` by hand, and `<stem>.library.json` exists precisely so
    // that hand metadata survives a re-derivation. `migrate::absorbed(…,
    // LIBRARY)` is the exact question `migrate.rs` already answers for the
    // same class of "has a hand document actually been applied here"
    // decision — reused rather than a second implementation of it. The
    // read-back below arms the marker itself, because a `.library.json` file
    // sits on disk with nothing in `migrated_document` yet to say so.
    //
    // **Content matching finds this row.** `import::scanned_inserting`'s own
    // `find_by_key` probes `content_key`, across the store, before
    // it ever falls back to the address `held_work` computed above, and
    // `content_key` is a hash over the hub edition's members — untouched by a
    // hand-set `series_id` or `volume`. So a work whose hand metadata has
    // moved its address away from what the rules currently resolve is still
    // reconciled onto the same row by its content, exactly as an ordinary
    // rescan is.
    //
    // **Hand values are kept per column — O80, migration 011.** The
    // provenance columns are `volume_hand`, `series_id_hand`, `language_hand`,
    // `kind_hand`, `series_title.hand` and `series.reading_hand`, with every
    // writer below using `COALESCE(<hand>, <what this pass derived>)`. So the
    // marker does not decide whether these columns may be written — only
    // whether the document has already been read back, which is the block
    // immediately following.
    //
    // **The document on disk, read back.** A fresh database has no marker for
    // a work it has just minted, so a correction sitting right beside the
    // archives is read here. `migrate::work`'s `Step::Apply` cannot do this
    // job (O53). This is the "real but different path" O53's own entry names.
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
                    Err(why) => {
                        unread.insert(work_id, why.clone());
                        report.unreadable_hand.push(why);
                    }
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
            Err(why) => {
                unread.insert(work_id, why.clone());
                report.unreadable_hand.push(why);
            }
        }
    }
    // **No `absorbed` guard here — the provenance columns answer it per field
    // instead (O80).** Migration 011 gave each of `volume`, `series_id`,
    // `kind` and `language` a `_hand` column, so this pass can write what it
    // derived unconditionally and let `COALESCE` decide. A hand value wins;
    // everything else re-derives, every scan, as it does for a work nobody
    // has edited.
    conn.execute(
        "UPDATE work
            SET series_id = COALESCE(series_id_hand, ?1),
                volume    = COALESCE(volume_hand, ?2)
          WHERE id = ?3",
        rusqlite::params![series_id, group.volume, work_id],
    )
    .map_err(|e| format!("attaching work {work_id} to series {series_id}: {e}"))?;
    // **The slug is minted here, and only for a row the import inserted
    // (O282).** The series and volume are final only after the read-back and
    // the `UPDATE` above. A row the import found keeps its slug whatever its
    // series or volume now say: the slug is the URL bookmarks and OPDS hold.
    if inserted {
        crate::import::mint_slug(conn, work_id)?;
    }
    write_edition_metadata(conn, work_id, group, report)?;
    write_joined_by(conn, work_id, group)?;
    write_chapters(conn, work_id, group)?;
    tx.commit()
        .map_err(|e| format!("committing work {work_id}: {e}"))?;
    report.works += 1;
    Ok((work_id, doc.hub))
}

/// One member as [`superseded`] reads it.
struct HeldDigest {
    /// The edition it sits on — needed to test whether this derive derived
    /// this row (see [`superseded`]'s condition 3), which `work_id` alone
    /// cannot answer: a work this derive derived can still hold an edition
    /// this pass never touched (`DeriveReport::kept_editions`).
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
/// sha256)`, on an edition this derive derived — by a work this derive
/// derived, paired with the work that holds it. Detection only — see
/// [`DeriveReport::emptied_works`]; nothing here moves or deletes a row.
///
/// **The five conditions, and why each is needed.** A work qualifies when:
///
/// 1. this derive did not derive it;
/// 2. it holds at least one member, and *every* member's
///    `(root_id, path, sha256)` is also held by some one work this derive
///    *did* derive;
/// 3. that covering row sits on an edition of that work whose key is in
///    `written_editions`, not merely on the work;
/// 4. every one of its members carries a non-NULL `root_id`;
/// 5. across every one of its members, the intersection of the sets of
///    written works holding that member's key resolves to exactly one
///    work — the target.
///
/// **(2) matches on `path`, not `sha256` alone.** A live work can drop
/// material from `held` through at least three ordinary routes with nothing
/// wrong having happened — `scan::forget_unseen` deleting `scan_entry` rows
/// out from under a `member.root_id` that survives an unreadable subtree, a
/// rule edit that suppresses entries (`Produces::Deferred`, see
/// [`DeriveReport::suppressed`]), or a re-hash that fails and leaves the entry
/// dropped while the member keeps its old digest. A byte-identical file
/// sitting at an unrelated path anywhere else in the library would, on
/// `sha256` alone, read as proof that one of those routes was actually a fold.
/// It is not: the fold this function exists to detect never moves a file on
/// disk, it only reclassifies which work and edition an unchanged path belongs
/// to. So the superseded work's member and the covering member on the target
/// must name the identical path, exactly as O73's own reproduction does, or
/// there is no evidence of a fold at all.
///
/// **(2) also requires the covering member's `root_id` to equal the
/// candidate's own.** `scan::add_root` canonicalises and inserts with no
/// containment check, and `scan_entry` is unique on `(root_id, rel_path)`, so
/// registering `/srv/manga` and then `/srv/manga/Extras` walks everything
/// under `Extras` twice, as two rows with different `rel_path`s but identical
/// absolute `path` and `sha256`. The rules can resolve those two rows into two
/// different works, and letting either one leave `held` through any of the
/// three routes above then produces every symptom of a fold over material
/// that never moved: same path, same digest, and both roots still present so
/// condition 4 excludes neither side. `root_id` equality on top of `path` and
/// `sha256` separates that shape from an actual fold cleanly: O73's own
/// reproduction has the candidate's member and the covering member descend
/// from the same `scan_entry` under the same root, so the two ids are equal by
/// construction, and [`write_roots`] guarantees the covering member carries
/// one at all.
///
/// **(3) requires the covering row to sit on an edition this derive
/// derived, not merely on a work it derived.** Such a work can still
/// be holding an edition this pass never touched —
/// [`DeriveReport::kept_editions`]'s own doc names exactly this shape, a work
/// "naming files the rules no longer place there". Treating such a stale row
/// as evidence would let a work be judged superseded on the strength of a
/// fact this derive did not establish.
///
/// **(4) is what keeps an imported work safe.** `006_member_root.sql` states
/// that NULL means "under no root", "which is exactly true of an imported
/// work" — so a hand-imported `taiyaku.json` naming the same archive is never
/// superseded by a scan. A member of a removed root that `scan::take_root`
/// found no copy of elsewhere reads NULL too (`ON DELETE SET NULL`), and its
/// work is likewise left alone: the safe direction of an ambiguity this
/// function must not resolve on its own.
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
    // function), both of which mean editions this derive did *not* derive and
    // left standing. This map is the opposite — editions this derive *did*
    // derive — and a local shadowing that name would make the guard below
    // read backwards to anyone carrying the module's own vocabulary.
    let mut wrote_editions: BTreeMap<i64, BTreeSet<&str>> = BTreeMap::new();
    for (work_id, key) in written_editions {
        wrote_editions
            .entry(*work_id)
            .or_default()
            .insert(key.as_str());
    }

    // Every `(root_id, path, sha256)` held on an edition this derive derived,
    // mapped to the *set* of works in `written` that hold it — not to a
    // single "owner", which would be too strict (see this function's own
    // doc). A stale edition on a work in `written` (`DeriveReport::kept_editions`)
    // contributes nothing here, by construction: only rows whose `(work_id,
    // edition_key)` is in `written_editions` are counted as evidence at all.
    // `root_id` joins the key alongside `path` and `sha256` — see condition
    // 2's own doc above for why matching on the file alone is not enough.
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
    // `BTreeMap` iteration order: `derive_with` iterates this list to delete,
    // and that order must not depend on an incidental property of `held`.
    found.sort_unstable();
    Ok(found)
}

/// `ORDER BY` text putting a `work` row some directory holds — one with a
/// member under a root — before one none does. See [`held_work`].
const HELD_FIRST: &str = "NOT EXISTS (SELECT 1 FROM edition held_e
                                     JOIN member held_m ON held_m.edition_id = held_e.id
                                    WHERE held_e.work_id = work.id
                                      AND held_m.root_id IS NOT NULL)";

/// The `work` row this group already sits on, as `(id, hub, content_key)`, or
/// `None` for a group no derive has written.
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
/// `ORDER BY … id`, first wins, because two rows really can sit at one address, and
/// reading the hub off whichever one SQLite happened to return would make the
/// answer depend on the page order of a b-tree. **A row some directory holds
/// wins over one none does** (an imported work placed here by hand), then the
/// oldest, which is the one a reader's progress is on: taking the imported row
/// would strand the row these files were already derived onto.
///
/// **One derive can produce that state on its own.** `derive/tests.rs`'s
/// `a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive` is the
/// case: content decides before the address, so a duplicated download sends
/// the second series' group onto the first's row, the `UPDATE` in
/// [`write_work`] then stamps that row with the second series'
/// `(series_id, volume)`, and the row that used to hold that address keeps
/// it. Two rows, one address.
///
/// **A group with no volume is addressed by its own files**, and it has to
/// be. `(series_id, NULL)` is not an address, because those groups are keyed
/// individually: a series holding `001`, `002`, `003` and `004` has four rows
/// matching `volume IS NULL`, and `ORDER BY id LIMIT 1` would hand every one
/// of those groups the first of them — so a work whose bytes changed, or a
/// work arriving later, would take the row of whichever volume-less work got
/// there first, with the reader's position on it.
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
/// A path is probed in the row's hub edition first, then in any edition: the
/// second identity probe over any member that [`crate::import::scanned`]'s doc
/// refuses, as it would merge two works sharing a file. Here it is scoped to
/// one series' volume-less rows, for a row whose files held this derive are
/// this group's alone and whose document's key is no other row's.
///
/// # Errors
///
/// Returns an error if SQLite refuses a statement.
fn held_work(
    conn: &Connection,
    series_id: i64,
    group: &Group,
    placed: &Placed,
) -> Result<Option<(i64, String, String)>, String> {
    // Every statement orders by `HELD_FIRST`, so none can differ on which of
    // two rows at one address is this group's.
    if group.volume.is_some() {
        // Not a row another group's edition hashes to ([`Placed::claims`]):
        // the import finds it by content for that group, and taking it here
        // would rekey it from under those files, leaving them on two works. A
        // follower's work is one once the volume it filled arrives.
        let mine: GroupKey = (group.series.clone(), group.volume.map(f64::to_bits), None);
        let rows: Vec<((i64, String, String), bool)> = conn
            .prepare_cached(&format!(
                "SELECT id, hub, content_key, series_id_hand IS NULL AND volume_hand IS NULL
                   FROM work
                  WHERE series_id = ?1 AND volume IS ?2
                  ORDER BY {HELD_FIRST}, id"
            ))
            .and_then(|mut stmt| {
                stmt.query_map(rusqlite::params![series_id, group.volume], |r| {
                    Ok(((r.get(0)?, r.get(1)?, r.get(2)?), r.get(3)?))
                })?
                .collect()
            })
            .map_err(|e| format!("looking up an existing work at this volume: {e}"))?;
        for (row, derived) in rows {
            let takes = match placed.claims.get(&row.2) {
                None => true,
                Some(by) if *by == mine => true,
                Some(_) if derived => false,
                // A hand series or volume says where a row belongs, so it stays
                // the address's where that group stands in this series (a hand
                // volume renumbered the work within it) or where it names a
                // file of this group's (a work placed for files yet to arrive).
                // One moved by hand from another series is that group's: the
                // volume arriving here takes a work of its own.
                Some(by) if by.0 == group.series => true,
                Some(_) => holds_a_file_of(conn, row.0, group)?,
            };
            if takes {
                return Ok(Some(row));
            }
        }
        return Ok(None);
    }
    // A path in the row's hub edition, whose files its `content_key` hashes:
    // a rule titling part of a work apart splits one group into two that both
    // hold the row's files, and only the part holding the hub's may keep it,
    // or the two write over one row in turn.
    for path in group.paths() {
        let held: Option<(i64, String, String)> = conn
            .query_row(
                &format!(
                    "SELECT work.id, work.hub, work.content_key FROM work
                      WHERE work.series_id = ?1 AND work.volume IS NULL
                        AND EXISTS (SELECT 1 FROM edition
                                      JOIN member ON member.edition_id = edition.id
                                     WHERE edition.work_id = work.id AND edition.key = work.hub
                                       AND member.path = ?2)
                      ORDER BY {HELD_FIRST}, work.id LIMIT 1"
                ),
                rusqlite::params![series_id, path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| format!("looking up an existing work holding `{path}`: {e}"))?;
        if held.is_some() {
            return Ok(held);
        }
    }

    // Else a path in any edition of a row whose files this derive holds
    // (`placed`) are this group's alone. No group holds its hub's files, so
    // none can claim it above, and there is no other part to split it from:
    // the translations left when a book's hub file goes keep the book's row,
    // with the reader's position, notes and pins. A row another group holds a
    // file of is left to neither, as giving it to one would be a guess.
    let failed = |e: rusqlite::Error| format!("looking up a work this group's files are on: {e}");
    let mut on = conn
        .prepare_cached(&format!(
            "SELECT work.id, work.hub, work.content_key FROM work
              WHERE work.series_id = ?1 AND work.volume IS NULL
                AND EXISTS (SELECT 1 FROM edition
                              JOIN member ON member.edition_id = edition.id
                             WHERE edition.work_id = work.id AND member.path = ?2)
              ORDER BY {HELD_FIRST}, work.id"
        ))
        .map_err(failed)?;
    let mut files = conn
        .prepare_cached(
            "SELECT member.path FROM member JOIN edition ON edition.id = member.edition_id
              WHERE edition.work_id = ?1",
        )
        .map_err(failed)?;
    let own: HashSet<&String> = group.paths().collect();
    let mut asked = HashSet::new();
    for path in group.paths() {
        let rows: Vec<(i64, String, String)> = on
            .query_map(rusqlite::params![series_id, path], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(failed)?
            .collect::<Result<_, _>>()
            .map_err(failed)?;
        for row in rows {
            if !asked.insert(row.0) {
                continue;
            }
            let held: Vec<String> = files
                .query_map([row.0], |r| r.get(0))
                .map_err(failed)?
                .collect::<Result<_, _>>()
                .map_err(failed)?;
            if !held
                .iter()
                .all(|file| own.contains(file) || !placed.paths.contains(file))
            {
                continue;
            }
            // The import looks the document's key up before this address, so
            // the row is this group's only where that key names no other row:
            // a translation left that is byte for byte another work's hub is
            // that work's, and taking this row too would leave its key
            // unwritten, for every derive to rewrite again.
            let key = crate::identity::content_key(&document(group, Some(&row.1)))?;
            if crate::import::find_by_key(conn, &key)?.is_none_or(|found| found == row.0) {
                return Ok(Some(row));
            }
        }
    }
    Ok(None)
}

/// Whether work `id` holds a member at one of `group`'s paths.
fn holds_a_file_of(conn: &Connection, id: i64, group: &Group) -> Result<bool, String> {
    let held: HashSet<String> = conn
        .prepare_cached(
            "SELECT member.path FROM member JOIN edition ON edition.id = member.edition_id
              WHERE edition.work_id = ?1",
        )
        .and_then(|mut stmt| stmt.query_map([id], |r| r.get(0))?.collect())
        .map_err(|e| format!("reading the files of work {id}: {e}"))?;
    Ok(group.paths().any(|path| held.contains(path)))
}

/// Put first, among one digest's staged entries in walk order, those whose
/// directory a held member of those bytes, in an edition of the entry's own
/// key, already names; [`place`] makes the first of them the primary. **Sticky**
/// because `hand::path_for` resolves through the hub's first `member.root_id`,
/// and the primary's rules name the work: a copy under a lower id would
/// otherwise move the document's address and rename the work. Read by the
/// bytes across the store, so a work a hand series or volume moved keeps it.
fn keep_primaries(
    conn: &Connection,
    sha256: &str,
    staged: &[Staged],
    at: &mut [usize],
) -> Result<(), String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT DISTINCT member.root_id FROM member
               JOIN edition ON edition.id = member.edition_id
              WHERE edition.key = ?1 AND member.sha256 = ?2
                AND member.root_id IS NOT NULL",
        )
        .map_err(|e| format!("reading where the store holds a copy: {e}"))?;
    let mut held: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
    for &i in at.iter() {
        let key = staged[i].edition_key.as_str();
        if held.contains_key(key) {
            continue;
        }
        let roots = stmt
            .query_map(rusqlite::params![key, sha256], |r| r.get::<_, i64>(0))
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
            .map_err(|e| format!("reading where the store holds `{sha256}`: {e}"))?;
        held.insert(key, roots);
    }
    // Stable, so walk order decides among the held and among the rest.
    at.sort_by_key(|&i| {
        let entry = &staged[i];
        !held[entry.edition_key.as_str()].contains(&entry.candidate.root_id)
    });
    Ok(())
}

/// How many editions of `work_id` this derive did not derive — see
/// [`DeriveReport::kept_editions`].
///
/// Read back rather than diffed against what was there before: the keys this
/// derive wrote are `doc.editions`, and every other edition on the row is by
/// definition one [`crate::import::scanned_inserting`] left standing.
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
/// A rule's `title` capture, the file's own (a book's `dc:title`), or the name
/// of the thing the **hub** edition holds — the hub rather than whichever
/// edition sorts first, so a second edition arriving in another root cannot
/// rename a work someone has already bookmarked.
///
/// There is no formatting of the series and volume into a title here: what is
/// on disk is a name a person chose, and `Go-Toubun no Hanayome - v01 (2019)`
/// says more than `Go-Toubun no Hanayome 1` reconstructed from two columns.
///
/// **The whole `rel_path` when it holds no `/`**: a member directly under
/// the root (`Series - v01.cbz`, which seeded rule 4 gives a series). Its own
/// arm, because [`parent_of`] answers `""` there and would take the title's
/// first character off with it.
///
/// **And the root's own directory name when the `rel_path` is empty**: the
/// root itself became the member, through a `set_series` rule matching `""`
/// (`proposals::accept` writes an empty `prefix`). It is read off the
/// member's absolute [`Candidate::path`], the only name that entry has.
///
/// **A name from disk loses one archive extension (O284)**: the extension
/// names the container, not the book, and the title is what every tile, the
/// reader and OPDS show. A rule's capture is a person's choice and is kept
/// as written.
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
    let name = if first.rel_path.is_empty() {
        // `root.path.join("")` leaves a trailing separator, which
        // `Path::file_name` steps over — it reads components, not bytes. The
        // fallback is for a root that genuinely has no final component, `/`
        // being the one that does.
        std::path::Path::new(&first.path).file_name().map_or_else(
            || first.path.clone(),
            |name| name.to_string_lossy().into_owned(),
        )
    } else {
        match first.rel_path.rfind('/') {
            Some(at) => first.rel_path[at + 1..].to_string(),
            None => first.rel_path.clone(),
        }
    };
    match crate::import::archive_stem(&name) {
        Some(stem) if !stem.is_empty() => stem.to_string(),
        _ => name,
    }
}

/// Write the chapter numbers the rules resolved onto the member rows
/// `import::scanned_inserting` just wrote, each as [`chapter_after`] settles it, and
/// only onto a row it changes: every member of the work, an edition this
/// derive did not name included.
///
/// **A number this derive wrote is cleared when the rules stop resolving
/// one.** A rule edit that stops resolving a chapter must not leave the
/// number a previous derive wrote standing — `replace_editions` carries
/// `chapter_declared` across its own wholesale replace, so nothing else would
/// remove it. **Only a number this derive owns moves**: one with no source,
/// or one sourced [`SCANNER_CHAPTER_SOURCE`]. A number a person entered by
/// hand, or one a `comicinfo` document supplied, is not this derive's to
/// recover if it turns out wrong, and a member whose filename still parses —
/// exactly the member someone corrects a chapter on, since `builtin-chapter`
/// is a seeded rule — would otherwise have its hand or `comicinfo` value
/// reverted to the filename's answer on the very next re-derive,
/// `chapter_source` included.
///
/// Matched on `(edition key, ordinal)`, which is where the members this call
/// just described actually landed: `replace_editions` inserts them in document
/// order counting from 0, and `project.rs` reads them back `ORDER BY ordinal`.
fn write_chapters(conn: &Connection, work_id: i64, group: &Group) -> Result<(), String> {
    let mut resolved: HashMap<(&str, i64), f64> = HashMap::new();
    for (key, candidates) in &group.editions {
        // `(0_i64..).zip(…)` rather than `enumerate()`, so the ordinal is the
        // `i64` the column wants without a `usize as i64` cast on the way.
        for (ordinal, candidate) in (0_i64..).zip(in_page_order(candidates)) {
            if let Some(chapter) = candidate.chapter {
                resolved.insert((key.as_str(), ordinal), chapter);
            }
        }
    }
    let failed = |e: rusqlite::Error| format!("reading the chapters of work {work_id}: {e}");
    let held = conn
        .prepare(
            "SELECT member.id, edition.key, member.ordinal, member.chapter_declared,
                    member.chapter_source
               FROM member JOIN edition ON edition.id = member.edition_id
              WHERE edition.work_id = ?1",
        )
        .map_err(failed)?
        .query_map([work_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<f64>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failed)?;
    for (member_id, key, ordinal, declared, source) in held {
        let chapter = resolved.get(&(key.as_str(), ordinal)).copied();
        let held = (declared, source);
        let after = chapter_after(held.clone(), chapter);
        if after == held {
            continue;
        }
        conn.execute(
            "UPDATE member SET chapter_declared = ?1, chapter_source = ?2 WHERE id = ?3",
            rusqlite::params![after.0, after.1, member_id],
        )
        .map_err(|e| {
            format!("recording the chapter of `{key}` member {ordinal} on work {work_id}: {e}")
        })?;
    }
    Ok(())
}

/// A member's `(chapter_declared, chapter_source)` once [`write_chapters`]
/// has written `resolved`, the chapter the rules resolved for it, over what it
/// held. One rule for that function and for [`unchanged`], so the check that
/// skips a work cannot disagree with the write it skips.
fn chapter_after(
    (declared, source): (Option<f64>, Option<String>),
    resolved: Option<f64>,
) -> (Option<f64>, Option<String>) {
    let (declared, source) = if source.as_deref() == Some(SCANNER_CHAPTER_SOURCE) {
        (None, None)
    } else {
        (declared, source)
    };
    match resolved {
        Some(chapter) if source.is_none() => {
            (Some(chapter), Some(SCANNER_CHAPTER_SOURCE.to_string()))
        }
        _ => (declared, source),
    }
}

/// Write each member's root and `member_root` rows onto `import::scanned_inserting`'s.
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
/// running does not fail the derive.** `group`'s root ids were read before
/// this call, and `derive_with` holds no transaction of its own (see that
/// function's own doc) — so a concurrent [`crate::scan::remove_root`]
/// committing in that window deletes a `root` row the `UPDATE` of
/// `member.root_id` or an `INSERT` into `member_root` is about to name, and
/// the foreign key refuses it. Caught specifically, by
/// [`is_foreign_key_violation`]: the column becomes the `NULL` migration 6's
/// `ON DELETE SET NULL` writes, a copy's row is skipped, and either is counted
/// in [`DeriveReport::root_removed_mid_derive`] rather than propagated. Any
/// other error from these statements — a locked database, a genuinely
/// malformed statement — propagates unchanged.
fn write_roots(
    conn: &Connection,
    work_id: i64,
    group: &Group,
    report: &mut DeriveReport,
) -> Result<(), String> {
    for (key, candidates) in &group.editions {
        let ordered = in_page_order(candidates);
        for (ordinal, candidate) in (0_i64..).zip(ordered) {
            let failed = |e: rusqlite::Error| {
                format!("recording the root of `{key}` member {ordinal} on work {work_id}: {e}")
            };
            let Some(member_id) = conn
                .query_row(
                    "SELECT member.id FROM member
                       JOIN edition ON edition.id = member.edition_id
                      WHERE edition.work_id = ?1 AND edition.key = ?2 AND member.ordinal = ?3",
                    rusqlite::params![work_id, key, ordinal],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(failed)?
            else {
                continue;
            };
            let primary = match conn.execute(
                "UPDATE member SET root_id = ?1 WHERE id = ?2",
                rusqlite::params![candidate.root_id, member_id],
            ) {
                Ok(_) => Some(candidate.root_id),
                Err(e) if is_foreign_key_violation(&e) => {
                    conn.execute(
                        "UPDATE member SET root_id = NULL WHERE id = ?1",
                        [member_id],
                    )
                    .map_err(|e| {
                        format!(
                            "clearing the vanished root of `{key}` member {ordinal} \
                             on work {work_id}: {e}"
                        )
                    })?;
                    report.root_removed_mid_derive += 1;
                    None
                }
                Err(e) => return Err(failed(e)),
            };
            // Exactly the primary's directory and each copy's, written after
            // the column rather than left to migration 018's triggers:
            // `member_root_on_update` drops the old directory's row when the
            // primary moves though a copy may still sit there, and the
            // import's restore may have put back one that holds no copy now.
            conn.execute("DELETE FROM member_root WHERE member_id = ?1", [member_id])
                .map_err(failed)?;
            for root_id in primary
                .into_iter()
                .chain(candidate.copies.iter().map(|k| k.root_id))
            {
                match conn.execute(
                    "INSERT INTO member_root (member_id, root_id) VALUES (?1, ?2)",
                    rusqlite::params![member_id, root_id],
                ) {
                    Ok(_) => {}
                    // A copy's directory removed mid-pass, as for the primary.
                    Err(e) if is_foreign_key_violation(&e) => report.root_removed_mid_derive += 1,
                    Err(e) => return Err(failed(e)),
                }
            }
        }
    }
    Ok(())
}

/// Drop the held reading of each entry `group` was read from, a copy's too,
/// once its work is written from their current readings (O320): a later
/// frozen derive would otherwise read the work by a reading it no longer
/// holds. Per work, so a cancel between two works keeps the readings of the
/// one not written. An entry whose directory this derive reads frozen keeps
/// its own.
fn drop_held(conn: &Connection, group: &Group) -> Result<(), String> {
    let mut stmt = conn
        .prepare_cached(
            "UPDATE scan_entry
                SET held_sha256 = NULL, held_format = NULL, held_pages = NULL, held_title = NULL
              WHERE root_id = ?1 AND rel_path = ?2 AND held_sha256 IS NOT NULL",
        )
        .map_err(|e| format!("dropping the held readings of `{}`: {e}", group.series))?;
    for candidate in group.editions.values().flatten() {
        let primary = (candidate.root_id, &candidate.rel_path, candidate.frozen);
        let copies = candidate
            .copies
            .iter()
            .map(|k| (k.root_id, &k.rel_path, k.frozen));
        for (root_id, rel_path, frozen) in std::iter::once(primary).chain(copies) {
            if frozen {
                continue;
            }
            stmt.execute(rusqlite::params![root_id, rel_path])
                .map_err(|e| format!("dropping the held reading of {rel_path}: {e}"))?;
        }
    }
    Ok(())
}

/// Whether SQLite refused a statement because it would have written a value
/// with no matching row on the other end of a foreign key — [`write_roots`]'s
/// own use is the only one: a root deleted out from under an in-flight derive.
fn is_foreign_key_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    )
}

/// Hash each entry a member names that [`classify`] left without a digest,
/// through [`digest`] as `classify` does. A scan writes none (§3), so the
/// entry of a member these rules make no member of, such as one a directory
/// nested in the one that derived it took over and reads no series from its
/// file's name, would keep none for good. The mark holds such a member by its
/// path whatever its digest ([`crate::browse::held_by`]), but
/// `scan::reads_as_one_volume` finds a folder member's entry by its digest
/// alone, to refuse a directory added inside it (O91).
fn digest_held_members(
    conn: &Connection,
    roots: &[Root],
    frozen: &HashSet<i64>,
    unreadable: &mut Unreadable,
) -> Result<(), String> {
    // A frozen root's changed files wait for Scan, hashed by no derive (O320).
    for root in roots.iter().filter(|root| !frozen.contains(&root.id)) {
        let failed = |e: rusqlite::Error| format!("reading the members of root {}: {e}", root.id);
        let held: HashSet<String> = conn
            .prepare(
                "SELECT m.path FROM member m JOIN member_root mr ON mr.member_id = m.id
                  WHERE mr.root_id = ?1",
            )
            .and_then(|mut stmt| stmt.query_map([root.id], |r| r.get(0))?.collect())
            .map_err(failed)?;
        let undigested: Vec<Entry> = conn
            .prepare(&format!(
                "SELECT {ENTRY_COLUMNS} FROM scan_entry WHERE root_id = ?1 AND sha256 IS NULL"
            ))
            .and_then(|mut stmt| stmt.query_map([root.id], entry_of)?.collect())
            .map_err(failed)?;
        for entry in undigested {
            // Joined as `classify` joins a candidate's path.
            let path = root.path.join(&entry.rel_path);
            if !held.contains(path.to_string_lossy().as_ref())
                || digest(conn, root, &entry)?.is_some()
            {
                continue;
            }
            // Its entry holds the work by path until the next walk forgets it,
            // so the report is the one place its going shows; `classify` has
            // named it already where these rules stage it.
            let gone = path.display().to_string();
            if !unreadable.vanished.contains(&gone) {
                unreadable.vanished.push(gone);
            }
        }
    }
    Ok(())
}

/// A work none of whose members any directory still holds, over the
/// statement's own `work` row: it has a member, since no scan could find a
/// work with none; every member has a `member_root` row, since one without is
/// imported (O54) and no scan is the authority for it; and no directory its
/// `member_root` rows name holds a member ([`crate::browse::held_by`]), so a
/// missing work has nothing to read at its path and no copy of its bytes
/// there. Every such directory is asked, not only the primary's, so O243's
/// unfolded works read present while a copy is held.
fn missing() -> String {
    format!(
        "EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                  WHERE e.work_id = work.id)
         AND NOT EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                          WHERE e.work_id = work.id
                            AND NOT EXISTS (SELECT 1 FROM member_root mr
                                             WHERE mr.member_id = m.id))
         AND NOT EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                           JOIN member_root mr ON mr.member_id = m.id
                          WHERE e.work_id = work.id AND {})",
        crate::browse::held_by("m", "mr")
    )
}

/// Mark every work [`missing`] holds of, and clear the mark on every marked
/// work it no longer holds of: the works marked, then the works cleared,
/// each its statement's own changes, which leave out the triggers' writes.
///
/// **Only rows whose mark changes are written.** Every `UPDATE` of `work`
/// re-indexes its title (`work_au`) and bumps `structure_version`
/// (`structure_work_upd`, which has no `WHEN`), so without the `IS NULL` and
/// `IS NOT NULL` guards every derive would re-index each missing work's title
/// and tell every live registry the structure moved when nothing did.
pub(crate) fn mark_missing(conn: &Connection) -> Result<(usize, usize), String> {
    let missing = missing();
    // Read first, in one statement and so one snapshot, and no write scope
    // where no mark would change: a derive over unchanged files then takes no
    // write lock here either (O272).
    let moves: bool = conn
        .query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM work WHERE missing_since IS NULL AND {missing})
                     OR EXISTS (SELECT 1 FROM work
                                 WHERE missing_since IS NOT NULL AND NOT ({missing}))"
            ),
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("reading which works no directory holds: {e}"))?;
    if !moves {
        return Ok((0, 0));
    }
    // One scope, so a scan another process commits between the two cannot
    // have one work marked and cleared, and counted in both, in one pass.
    let tx = crate::Nested::open(conn, "mark_missing")
        .map_err(|e| format!("starting the missing-works pass: {e}"))?;
    let marked = tx
        .execute(
            &format!(
                "UPDATE work SET missing_since = datetime('now')
                  WHERE missing_since IS NULL AND {missing}"
            ),
            [],
        )
        .map_err(|e| format!("marking the works no directory holds: {e}"))?;
    // The mark's condition negated, not "a member is held again": a marked
    // work that comes to have a member under no directory is cleared and
    // shown, as an imported work is.
    let found = tx
        .execute(
            &format!(
                "UPDATE work SET missing_since = NULL
                  WHERE missing_since IS NOT NULL AND NOT ({missing})"
            ),
            [],
        )
        .map_err(|e| format!("clearing the mark of the works a directory holds again: {e}"))?;
    tx.commit()
        .map_err(|e| format!("committing the missing-works pass: {e}"))?;
    Ok((marked, found))
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
/// rows [`crate::import::scanned_inserting`] just wrote.
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
/// This pair does not share the absence. `librarydoc::apply` is the only other
/// production writer of a *value* into `edition.kind`/`edition.language` —
/// `replace_editions`' `UPDATE` puts back what the row already held, across its
/// own delete-and-reinsert — and it sets `kind_hand` and `language_hand` with
/// them, which the `UPDATE` below keeps over the rules' answer. So these join
/// `work.series_id` and `work.volume` instead of joining a list whose reason
/// does not reach them; see [`write_work`]'s comment on the provenance columns.
///
/// **Unconditional within `COALESCE`, and that is the point.** Editing a rule
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
        let ResolvedMetadata {
            kind,
            language,
            refused,
        } = resolved_metadata(candidates);
        if refused {
            report.unrecognised_kinds += 1;
        }
        conn.execute(
            // `COALESCE(<hand>, <derived>)` — O80, migration 011. See
            // `write_work`'s note on why there is no `absorbed` guard.
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

/// Write the pair that joined each edition ([`joins::apply`]) onto the
/// `edition` rows the import just wrote, and none onto an edition no join
/// brought in, so an edition a Split sends home loses its mark.
fn write_joined_by(conn: &Connection, work_id: i64, group: &Group) -> Result<(), String> {
    for (key, candidates) in &group.editions {
        conn.execute(
            "UPDATE edition SET joined_by = ?1 WHERE work_id = ?2 AND key = ?3",
            rusqlite::params![joined_by(candidates), work_id, key],
        )
        .map_err(|e| format!("recording what joined `{key}` on work {work_id}: {e}"))?;
    }
    Ok(())
}

/// The pair that joined an edition: every file of one moves together, so the
/// first in page order speaks for it.
fn joined_by(candidates: &[Candidate]) -> Option<i64> {
    in_page_order(candidates)
        .first()
        .and_then(|candidate| candidate.joined_by)
}

/// The kind and language the rules resolved for one edition, settled as
/// [`write_edition_metadata`]'s doc says: the first entry in page order that
/// resolved each field decides it.
struct ResolvedMetadata {
    kind: Option<String>,
    language: Option<String>,
    /// A kind was resolved and is outside the closed set, so `kind` is `None`
    /// ([`DeriveReport::unrecognised_kinds`]).
    refused: bool,
}

fn resolved_metadata(candidates: &[Candidate]) -> ResolvedMetadata {
    let ordered = in_page_order(candidates);
    let language = ordered.iter().find_map(|c| c.language.clone());
    match ordered.iter().find_map(|c| c.kind.clone()) {
        Some(kind) if crate::librarydoc::parse_edition_kind(&kind).is_ok() => ResolvedMetadata {
            kind: Some(kind),
            language,
            refused: false,
        },
        resolved => ResolvedMetadata {
            kind: None,
            language,
            refused: resolved.is_some(),
        },
    }
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
/// [`classify`]'s own `chapter_dirs` set uses — with the series name
/// attached, which that set does not carry.
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

/// One entry a derive claims: it speaks for everything beneath it, and
/// nothing above it speaks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    pub root_id: i64,
    pub rel_path: String,
    pub format: Option<String>,
    pub pages: Option<u32>,
    pub sha256: Option<String>,
}

/// Every claimed entry across every root, by root and then path: the files and
/// folders a fingerprint pass reads.
///
/// Asked with the helpers `unattached_chapter_dirs` shares with [`derive()`],
/// so a pass reads exactly the entries the derive takes as members or as
/// unattached chapters, and a rule edit that moves a claim moves what is read.
///
/// # Errors
///
/// Returns an error under the same conditions [`derive()`] does: the rules or
/// roots cannot be read, or a `scan_entry` row will not project.
pub fn claimed_units(conn: &Connection) -> Result<Vec<Claimed>, String> {
    let (engine, _complaints) = Engine::load(conn)?;
    let roots = crate::scan::roots(conn)?;
    let mut report = DeriveReport::default();
    let mut units = Vec::new();

    for root in &roots {
        let mut seen = Vec::new();
        for entry in entries(conn, root.id)? {
            if let Some(admitted) = admit(&engine, root, entry, &mut report) {
                seen.push(admitted);
            }
        }
        let claims = claims_of(&seen);
        for admitted in seen {
            if !admitted.claims_subtree(&claims)
                || nested_in_claim(&claims, &admitted.entry.rel_path)
            {
                continue;
            }
            let Entry {
                rel_path,
                format,
                pages,
                sha256,
                ..
            } = admitted.entry;
            units.push(Claimed {
                root_id: root.id,
                rel_path,
                format,
                pages,
                sha256,
            });
        }
    }
    Ok(units)
}

/// Every `(root id, rel_path)`, across every root, that the rules currently
/// resolve to `series` — every `scan_entry` there [`admit`]
/// would place in this series, with unreadable and series-less entries
/// already excluded by that function.
///
/// **Shared with `proposals::accept` for the same reason
/// [`unattached_chapter_dirs`] is shared with `proposals::raise`.** Reading
/// the series' own `sort_title` as the answer to "which paths belong to this
/// series" rests on reasoning that seeded rule 1 resolves a series from the
/// first path component, so the title *is* that component. **False whenever
/// a higher-ordinal rule decides the series first** — seeded rule 4 exists
/// precisely because a filename sometimes repeats the series where the
/// directory does not. Reading `admit`'s own resolution here, rather than
/// re-deriving "which series does this path belong to" a second way in
/// `proposals.rs`, is what keeps this answer and `derive`'s own from being
/// able to disagree.
///
/// **The root id travels with the path** so `proposals::accept` can scope the
/// rule it writes to the one root the subject's own material actually lives
/// under — a `prefix` rule with `root_id` NULL applies to every root
/// forever, including one added after the accept, sharing nothing with the
/// subject beyond a directory name that happens to match.
///
/// Not scoped to candidates (volumes or chapters) the way
/// [`unattached_chapter_dirs`] is — every admitted path counts, since what
/// `accept` needs is "where does this series' material live", not "which of
/// it is a member".
///
/// **Across every library.** A subject spread over two libraries' directories
/// is answered with both, so `proposals::common_directory_pattern` refuses it.
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
    /// describes is checked by counting calls, not by a signature:
    /// `proposals::side` holds a `Connection` and `proposals::offered_rule`
    /// holds a `Connection` and a `&str`, and either could call
    /// [`rel_paths_for_series`] again with no signature change at all.
    ///
    /// Thread-local, not a shared `static`, because libtest runs `#[test]`
    /// functions concurrently on a thread pool: a single process-wide
    /// counter would also count another test's `evidence`/`preview` landing
    /// inside the same reset-then-read window on a different thread. Nothing
    /// here calls `rel_paths_by_series` from a second thread, so a
    /// thread-local count is exact for whichever test resets and reads it,
    /// regardless of what else the suite is doing.
    pub(crate) static REL_PATHS_BY_SERIES_CALLS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// What [`rel_paths_by_series`] answers: each title asked about, with the
/// `(root id, rel_path)` of every entry that resolves to it.
pub(crate) type RelPathsBySeries = BTreeMap<String, Vec<(i64, String)>>;

/// [`rel_paths_for_series`] for several series in **one** pass.
///
/// Each series is asked about by title, and is answered from every root.
/// Every title asked about is a key of the answer, holding an empty `Vec`
/// where nothing resolved to it — "no entries" is an answer here, not an
/// absence.
///
/// **The pass is what costs, not the filter.** Each call compiles every rule
/// in the library ([`Engine::load`]), reads every root, `SELECT`s every
/// `scan_entry` row under each, and resolves every one of them; deciding
/// whether the series that came back is one of two rather than one of one is
/// free beside it. `proposals::evidence` asks about both ends of a proposal
/// at once, so it pays for the whole sweep once rather than twice.
pub(crate) fn rel_paths_by_series(
    conn: &Connection,
    series: &BTreeSet<&str>,
) -> Result<RelPathsBySeries, String> {
    #[cfg(test)]
    REL_PATHS_BY_SERIES_CALLS.with(|c| c.set(c.get() + 1));
    let (engine, _complaints) = Engine::load(conn)?;
    let roots = crate::scan::roots(conn)?;
    let mut report = DeriveReport::default();
    let mut found: RelPathsBySeries = series
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

mod joins;

#[cfg(test)]
mod tests;
