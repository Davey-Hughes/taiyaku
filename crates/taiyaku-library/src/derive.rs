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

use std::collections::{BTreeMap, BTreeSet};

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

/// What one [`derive`] call turned the facts into.
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
}

/// Turn every `scan_entry` this library holds into series, works, editions and
/// members, through the rules.
///
/// Idempotent by construction: series are reconciled on `sort_title` and works
/// on `(series_id, volume)`, so running this twice over unchanged facts writes
/// the same rows a second time rather than a second set of rows.
///
/// **No transaction of its own.** [`crate::import::scanned`] opens one per
/// work, and `unchecked_transaction` issues a plain `BEGIN`, which SQLite
/// refuses inside an open transaction — the same constraint
/// [`crate::import::replace_alignments`] records. A derive is therefore atomic
/// per work and not across the library, which is the right grain anyway: a
/// library that half-derives has half its works current, not none of them.
///
/// # Errors
///
/// Returns an error if the rules or the roots cannot be read, if a `scan_entry`
/// row will not project, or if SQLite refuses one of the statements below. A
/// rule whose pattern does not compile is **not** one of those: `Engine::load`
/// returns it as a complaint and the rule matches nothing, so one malformed
/// pattern cannot leave a library underived. Neither is a file that has gone
/// or a container that will not open; see [`DeriveReport`].
pub fn derive(conn: &Connection) -> Result<DeriveReport, String> {
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

    for group in groups.into_values() {
        let series_id = *series_ids
            .get(&group.series)
            .ok_or_else(|| format!("no series row was reconciled for `{}`", group.series))?;
        write_work(conn, series_id, &group, &mut report)?;
    }

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
    conn.execute(
        "INSERT OR IGNORE INTO series_title (series_id, title, is_primary) VALUES (?1, ?2, 1)",
        rusqlite::params![series_id, sort_title],
    )
    .map_err(|e| format!("recording the title of series `{sort_title}`: {e}"))?;
    Ok(series_id)
}

/// Build one group's document and hand it to [`crate::import::scanned`], then
/// write the columns the document cannot carry — `work.series_id`,
/// `work.volume` and, through [`write_chapters`], `member.chapter_declared`.
fn write_work(
    conn: &Connection,
    series_id: i64,
    group: &Group,
    report: &mut DeriveReport,
) -> Result<(), String> {
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
    if let Some((_, _, held_key)) = &held {
        if *held_key != crate::identity::content_key(&doc)? {
            report.rekeyed += 1;
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
    // §4.4 — is the documented mechanism for setting `work.volume` and
    // `work.series_id` by hand, and `<stem>.library.json` exists precisely
    // so that hand metadata survives a re-derivation. Before this, `derive`
    // wrote both unconditionally on every pass: a hand-set `volume = 4.5`
    // came back as `1.0` after one derive, and a hand-renamed series minted
    // a second row and moved the works to it, leaving the renamed one
    // empty. `migrate::absorbed(…, LIBRARY)` is the exact question
    // `migrate.rs` already answers for the same class of "has a hand
    // document actually been applied here" decision — reused rather than a
    // second implementation of it, and it is `true` only once
    // `LibraryOutcome::Applied` has actually landed, never merely because a
    // `.library.json` file happens to sit on disk.
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
    // **Still narrow.** `edition.kind` and `edition.language` joined this
    // guard because the same question settles them — `librarydoc::apply` is
    // the other writer of both, exactly as it is of the two columns above.
    // Everything else about hand-versus-derived metadata — a provenance
    // column, scoping this to works with no `sidecar_path`, and so on — is
    // still the design question I7 left open, and nothing here answers it.
    if !crate::migrate::absorbed(conn, work_id, crate::migrate::LIBRARY)? {
        conn.execute(
            "UPDATE work SET series_id = ?1, volume = ?2 WHERE id = ?3",
            rusqlite::params![series_id, group.volume, work_id],
        )
        .map_err(|e| format!("attaching work {work_id} to series {series_id}: {e}"))?;
        write_edition_metadata(conn, work_id, group, report)?;
    }
    write_chapters(conn, work_id, group)?;
    write_roots(conn, work_id, group)?;
    report.works += 1;
    Ok(())
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
/// **Cleared first, and scoped by `chapter_source`.** A rule edit that stops
/// resolving a chapter must not leave the number a previous derive wrote
/// standing — `replace_editions` carries `chapter_declared` across its own
/// wholesale replace, so nothing else would remove it. The clear names
/// [`SCANNER_CHAPTER_SOURCE`] rather than every row, because a number a person
/// entered by hand is the one thing here no re-derive could recover, which is
/// why `member.chapter_declared` is in migration 1 at all.
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
                                           WHERE work_id = ?4 AND key = ?5)",
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
fn write_roots(conn: &Connection, work_id: i64, group: &Group) -> Result<(), String> {
    for (key, candidates) in &group.editions {
        let ordered = in_page_order(candidates);
        for (ordinal, candidate) in (0_i64..).zip(ordered) {
            conn.execute(
                "UPDATE member SET root_id = ?1
                  WHERE ordinal = ?2
                    AND edition_id = (SELECT id FROM edition
                                       WHERE work_id = ?3 AND key = ?4)",
                rusqlite::params![candidate.root_id, ordinal, work_id, key],
            )
            .map_err(|e| {
                format!("recording the root of `{key}` member {ordinal} on work {work_id}: {e}")
            })?;
        }
    }
    Ok(())
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
            "UPDATE edition SET kind = ?1, language = ?2 WHERE work_id = ?3 AND key = ?4",
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
