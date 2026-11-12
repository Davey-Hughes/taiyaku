//! Reading and writing `taiyaku.json`.
//!
//! The format is specified in `docs/sidecar.md`. It lives in the core so a
//! browser reader gets it without reimplementing anything.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Direction, Half, Run, Slot, Verdict};

/// A position within an edition's slot table, its length, or a count drawn
/// from it.
///
/// Named apart from `slot_index` below, which answers a different question
/// (which position a `Slot` sits at) and already owns that name.
///
/// # Panics
///
/// Panics if `i` exceeds `u32::MAX`. It cannot in this crate: every call
/// site — here and in `credits.rs`, `route.rs` — derives `i` from
/// `Edition::slots()`'s output, a `Vec<Slot>` at most twice the length of
/// the edition's page count (`expand` in `slot.rs` emits at most two `Slot`s
/// per page), and a page count is bounded by what a book can physically
/// contain, far below `u32::MAX`.
pub(crate) fn slot_position(i: usize) -> u32 {
    u32::try_from(i).expect("slot position exceeds u32::MAX")
}

/// `slot_position`, from a source already computed with signed arithmetic —
/// carrying a slot position across a paired run's offset, or clamping one
/// into another position's range.
///
/// # Panics
///
/// Panics if `i` is negative or exceeds `u32::MAX`. Every call site in this
/// module builds `i` from slot-table positions that are themselves within
/// `u32::MAX` (the bound `slot_position` relies on), combined only by
/// addition or by `min`/`max` against another in-range bound, so `i` cannot
/// leave the non-negative, in-range band its inputs started in.
fn slot_position_signed(i: i64) -> u32 {
    u32::try_from(i).expect("slot offset is negative or exceeds u32::MAX")
}

/// A contiguous stretch of slots, written the way the sidecar writes it.
///
/// The notation is page-based rather than slot-index-based because a person has
/// to be able to read and correct it: `7` is a whole page, `4R` the right half
/// of the spread at page 4, `45R-46L` a range spanning two spreads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotSpan {
    pub start: Slot,
    pub end: Slot,
}

impl SlotSpan {
    /// # Errors
    ///
    /// Returns an error if `text` has more than one `-`, so more than two
    /// endpoints — `"1-2-3"` — or if either endpoint is not a page reference:
    /// empty, containing a non-digit byte other than a trailing `L`/`R`, or a
    /// digit run too large for `u32`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut ends = text.split('-');
        let first = ends.next().unwrap_or("");
        let second = ends.next();
        if ends.next().is_some() {
            return Err(format!("span has more than two endpoints: {text:?}"));
        }
        let start = parse_slot(first)?;
        let end = match second {
            Some(s) => parse_slot(s)?,
            None => start,
        };
        Ok(Self { start, end })
    }
}

/// One endpoint: digits, then an optional `L` or `R` naming the half.
fn parse_slot(text: &str) -> Result<Slot, String> {
    let (digits, half) = match text.as_bytes().last() {
        Some(b'L') => (&text[..text.len() - 1], Some(Half::Left)),
        Some(b'R') => (&text[..text.len() - 1], Some(Half::Right)),
        _ => (text, None),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("not a page reference: {text:?}"));
    }
    let page = digits
        .parse()
        .map_err(|_| format!("page number out of range: {text:?}"))?;
    Ok(Slot { page, half })
}

impl std::fmt::Display for SlotSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", show_slot(self.start))?;
        if self.end != self.start {
            write!(f, "-{}", show_slot(self.end))?;
        }
        Ok(())
    }
}

fn show_slot(slot: Slot) -> String {
    match slot.half {
        Some(Half::Left) => format!("{}L", slot.page),
        Some(Half::Right) => format!("{}R", slot.page),
        None => slot.page.to_string(),
    }
}

// A span lives as a plain string in the document, which is what makes a run
// readable. Implemented by hand rather than derived, because the derive would
// write the struct's two `Slot` fields out as an object and lose the notation
// entirely. `Option<SlotSpan>` needs no separate impl: serde's blanket
// `Option<T>` handling calls straight through to these for `Some`, and
// already treats a missing field and an explicit `null` the same way `None`
// is treated here — see `RunEntry::b` and its tests.
impl Serialize for SlotSpan {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SlotSpan {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Convert solved runs into the entries a sidecar stores.
///
/// The solver works in slot indices; the document is written in page terms. The
/// slot tables are what translate between them, and getting this wrong by one
/// would misfile every run in the file while still producing valid JSON.
///
/// # Errors
///
/// Returns an error if any run's `a` or `b` endpoint is a slot index at or
/// past the length of the matching slot table (`a_slots` for `a`, `b_slots`
/// for `b`) — a run the solver produced against a different edition, or one
/// hand-edited to reach past the edition's last slot. The message names the
/// offending index and how many slots the edition actually has.
pub fn entries_from_runs(
    runs: &[crate::Run],
    a_slots: &[Slot],
    b_slots: &[Slot],
) -> Result<Vec<RunEntry>, String> {
    let span = |slots: &[Slot], from: u32, to: u32| -> Result<SlotSpan, String> {
        let at = |i: u32| {
            slots.get(i as usize).copied().ok_or_else(|| {
                format!(
                    "run refers to slot {i}, but the edition has {}",
                    slots.len()
                )
            })
        };
        Ok(SlotSpan {
            start: at(from)?,
            end: at(to)?,
        })
    };

    runs.iter()
        .map(|run| {
            Ok(RunEntry {
                a: span(a_slots, run.a.start, run.a.end)?,
                b: match run.b {
                    Some(b) => Some(span(b_slots, b.start, b.end)?),
                    None => None,
                },
                margin: None,
                by: None,
                note: None,
            })
        })
        .collect()
}

/// Where a slot sits in an edition's slot sequence.
///
/// A page reference with no half names the first slot of that page, which is
/// what makes `"7"` mean the same thing whether page 7 is whole or a spread.
pub(crate) fn slot_index(slots: &[Slot], slot: Slot) -> Option<usize> {
    slots
        .iter()
        .position(|s| s.page == slot.page && (slot.half.is_none() || s.half == slot.half))
}

/// Who produced a run, or an alignment as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Solver,
    Hand,
}

/// One member of an edition: an archive, or a directory of page images.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub path: String,
    /// Over the member file, so a reader can tell the archive was replaced.
    pub sha256: String,
    pub pages: u32,
}

/// A named, ordered page sequence assembled from one or more members.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edition {
    pub label: String,
    pub members: Vec<Member>,
    /// Indices, in the flattened page sequence, of joined double-page spreads.
    #[serde(default)]
    pub spreads: Vec<u32>,
    /// Indices, in the flattened page sequence, of pages a reader can keep out
    /// of the reading flow — a scanlation group's donation cards and staff
    /// credits.
    ///
    /// Page indices, like `spreads`, and not slots: both describe the page
    /// sequence rather than the alignment. Unlike `spreads` this is a
    /// heuristic's output rather than a measurement of the image, which is why
    /// `taiyaku credits` prints it for review and writes it only under
    /// `--write`.
    ///
    /// Skipped when empty, so a sidecar with no list round-trips byte for byte.
    /// An absent list and an empty one both mean nothing has been classified.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<u32>,
}

impl Edition {
    /// How many pages this edition presents, from its members' recorded counts.
    ///
    /// Derived here rather than by each consumer because it is normative —
    /// `docs/sidecar.md` rule 1 — and a server that sums it differently
    /// disagrees with the aligner about where every member after the first
    /// begins, while still matching every `sha256` in the file.
    #[must_use]
    pub fn total_pages(&self) -> u32 {
        self.members.iter().map(|m| m.pages).sum()
    }

    /// Which member holds page `index`, and how far into it that page is.
    ///
    /// `None` past the end, rather than wrapping or clamping: a reader asking
    /// for a page that does not exist wants to be told so.
    ///
    /// Members recorded with zero pages are stepped over, so a hand-edited
    /// sidecar cannot make one of them absorb an index.
    #[must_use]
    pub fn locate(&self, index: u32) -> Option<(usize, u32)> {
        let mut offset = 0u32;
        for (position, member) in self.members.iter().enumerate() {
            if index < offset + member.pages {
                return Some((position, index - offset));
            }
            offset += member.pages;
        }
        None
    }

    /// The slot sequence this edition presents to an alignment.
    ///
    /// Reconstructed from the page count and the spread list rather than
    /// stored, because storing it would be a second source of truth that could
    /// disagree with the first.
    #[must_use]
    pub fn slots(&self, reading: Direction) -> Vec<Slot> {
        let pages = self.total_pages();
        let wide: Vec<bool> = (0..pages).map(|p| self.spreads.contains(&p)).collect();
        crate::expand(&wide, reading)
    }

    /// The page of one member that `slot` shows, so a reading position or a
    /// note can find that page again after the members around it or the
    /// spread flags change, which move every slot after the change (O244),
    /// unless an earlier copy of the member's archive goes (`slot_of`).
    ///
    /// The member is named by its digest and by its occurrence among the
    /// members with that digest, because one archive can stand twice in an
    /// edition, and the digest alone would name the first copy.
    ///
    /// `None` past the end.
    #[must_use]
    pub fn member_page(&self, slot: u32, reading: Direction) -> Option<MemberPage> {
        let at = *self.slots(reading).get(slot as usize)?;
        let (position, page) = self.locate(at.page)?;
        let member = &self.members[position];
        let nth = self.members[..position]
            .iter()
            .filter(|m| m.sha256 == member.sha256)
            .count();
        Some(MemberPage {
            sha256: member.sha256.clone(),
            nth: u32::try_from(nth).ok()?,
            page,
            half: at.half,
        })
    }

    /// Where `at` stands in this edition's slots now. `None` where that copy
    /// of the archive is gone, or `at.page` is past its end rather than a page
    /// of the next member; [`SlotOf::PageOnly`] where the page is there and
    /// its half is not.
    ///
    /// The copy is matched by occurrence as well as digest, because one archive
    /// can stand twice in an edition (derive keeps two such files as two
    /// members). Removing the first copy renumbers the second, so its pages
    /// answer `None` though they are still there: accepted, as rare (O244).
    #[must_use]
    pub fn slot_of(&self, at: &MemberPage, reading: Direction) -> Option<SlotOf> {
        let mut first = 0u32;
        let mut seen = 0u32;
        for member in &self.members {
            if member.sha256 == at.sha256 {
                if seen == at.nth {
                    if at.page >= member.pages {
                        return None;
                    }
                    let page = first + at.page;
                    let slots = self.slots(reading);
                    let exact = Slot {
                        page,
                        half: at.half,
                    };
                    return match slots.iter().position(|s| *s == exact) {
                        Some(i) => Some(SlotOf::Exact(slot_position(i))),
                        None => slot_index(&slots, Slot { page, half: None })
                            .map(|i| SlotOf::PageOnly(slot_position(i))),
                    };
                }
                seen += 1;
            }
            first += member.pages;
        }
        None
    }
}

/// Where a reading position or a note stands: a page of one member of an
/// edition, held by the member's digest so the members around it can change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberPage {
    pub sha256: String,
    /// Which of the edition's members with this digest, from 0 in member order.
    pub nth: u32,
    /// The page within that member, from 0.
    pub page: u32,
    /// The half of a spread; `None` for a whole page.
    pub half: Option<Half>,
}

/// Where a `MemberPage` stands in an edition's slots now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotOf {
    /// The slot naming that page and half.
    Exact(u32),
    /// The page is there and its half is not: a half on a page that is no
    /// longer a spread, or a whole page that now is one. The page's first
    /// slot in reading order.
    PageOnly(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Work {
    pub title: String,
    pub reading: Direction,
}

/// How the alignment was produced, recorded so a re-solve is reproducible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolverInfo {
    pub hash: String,
    pub floor: f32,
}

/// One run, as stored. `b` absent means the pages in `a` have no counterpart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunEntry {
    pub a: SlotSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<SlotSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<Source>,
    /// Free text from whoever corrected this run. JSON has no comments, and a
    /// comment would not survive the next re-solve in any case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Whether an alignment is believable, and the figures that say so.
///
/// Stored because a reader holding only the document should not have to open
/// the archives to find out — which is the work the sidecar exists to avoid.
/// The figures accompany the verdict rather than the verdict standing alone,
/// because the threshold behind it is a moving policy, so a bare verdict could
/// mean different things in two files written under different thresholds.
///
/// This is a summary, never an authority. `Sidecar::quality_of` recomputes it
/// from the runs, so a hand-edited document cannot leave a stale verdict
/// standing beside contested runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quality {
    pub verdict: Verdict,
    /// Slots in the alignment's own `from` edition — not necessarily the
    /// caller's `from`, since `quality_of` reads a stored edge in either
    /// direction but runs stay indexed by whichever side the alignment itself
    /// calls `from`.
    pub slots: u32,
    /// How many of those slots found a counterpart.
    pub paired: u32,
    /// Mean length of the paired runs, in slots.
    ///
    /// `f64`, matching `average_run_length`'s return type. Unlike that
    /// function's result, this field is rounded — see `rounded_avg_run` —
    /// because it is stored in the sidecar document, which `docs/sidecar.md`
    /// pins to a human-readable, diffable form; `average_run_length` itself
    /// stays unrounded, since `verdict` compares its result against a
    /// threshold and rounding there would move which pairings cross it.
    pub avg_run: f64,
}

/// Round an average run length to the precision the sidecar document stores,
/// without touching the value `verdict` actually compares against.
///
/// Without rounding, a raw figure like `64.33333333333333` would reach the
/// document, which is longer for no reason `docs/sidecar.md`'s
/// "human-readable, diffable" goals can point to. Five decimals is the chosen
/// bound rather than the least that would serve: no consumer needs the third
/// digit — `stats.rs` prints one, and nothing reads the stored field back for
/// a decision, since `verdict` always recomputes from `average_run_length`
/// directly — but five costs three characters against sixteen and keeps the
/// stored figure from ever being the reason a later reader has less precision
/// than they wanted. That headroom is the house default here; two would have
/// been the minimum.
///
/// Five also stays a *decimal* figure once written. `avg_run` is a ratio of
/// two slot counts, so `(raw * 100_000.0).round()` peaks around 1e8 for any
/// book — exact well below 2^53 — and the quotient's shortest round-tripping
/// form never runs past five decimals.
///
/// Called wherever a `Quality` is computed from runs (`sidecar.rs`'s `rewrite`
/// and `quality_of`, and `taiyaku-cli`'s `built_alignment`), rather than left
/// for each to round inline, so the precision cannot drift between them.
#[must_use]
pub fn rounded_avg_run(raw: f64) -> f64 {
    (raw * 100_000.0).round() / 100_000.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alignment {
    pub from: String,
    pub to: String,
    pub by: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver: Option<SolverInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
    pub runs: Vec<RunEntry>,
}

/// A whole `taiyaku.json`.
///
/// Unknown keys are preserved nowhere and ignored everywhere: an L0 reader must
/// tolerate the L1 and L2 fields it does not understand, which is what serde
/// does by default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub taiyaku: u32,
    pub work: Work,
    pub editions: BTreeMap<String, Edition>,
    pub hub: String,
    pub alignments: Vec<Alignment>,
}

/// The format version this build writes.
pub const VERSION: u32 = 1;

/// **A document declaring a format version newer than this build reads is
/// refused**, in one sentence for each document whose key is listed below.
///
/// `key` is the version field (`taiyaku`, `taiyaku_library`, `taiyaku_notes`,
/// `taiyaku_truth`, `taiyaku_regions`; `taiyaku_progress` refuses in its own
/// words), named so whoever holds several documents beside one archive can
/// tell which was refused. [`Sidecar::readable_version`] gives the reason:
/// each is written back whole, a newer notes document as version 1 and the
/// rest without fields this build cannot name (O264). Only newer is refused.
///
/// # Errors
///
/// Returns an error naming `key` and both versions if `declared` exceeds
/// `reads`.
pub fn refuse_newer(key: &str, declared: u32, reads: u32) -> Result<(), String> {
    if declared > reads {
        return Err(format!(
            "this document declares {key} format version {declared}, and this build reads \
             {reads}. Refusing to read it rather than dropping the fields it does not \
             understand and writing them back out as version {reads} — upgrade taiyaku instead"
        ));
    }
    Ok(())
}

/// Where an alignment being merged is allowed to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The star: the alignment must begin at the document's hub, so every
    /// edition is one hop from every other.
    Hub,
    /// A direct edge deliberately layered over the star, for a pair where
    /// composition demonstrably loses.
    Direct,
}

/// What a merge did, so a caller can say so rather than guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merged {
    Added,
    Replaced,
    /// The spoke's bytes were unchanged, so the corrections on the old edge
    /// were carried onto the new one. The count is how many.
    CarriedHandRuns(usize),
    /// The spoke's bytes changed, so a `credits` list on the old edition was
    /// discarded rather than carried onto pages that may not exist. The count
    /// is how many entries went.
    ///
    /// Dropped rather than refused, which is the opposite of the stance on
    /// hand runs (O13) and deliberately so: a hand correction is a human
    /// decision that re-running nothing can recover, while a `credits` list
    /// is a tool's output that `taiyaku credits` reproduces in one command.
    DroppedCredits(usize),
    /// `merge_spoke` itself never returns this — every other variant above is
    /// built into its `Ok` return, this one is not. It exists for a caller
    /// that catches a `merge_spoke` refusal and, rather than propagate it,
    /// drops the stored alignment `merge_spoke` could not safely merge onto
    /// and takes the freshly-solved one instead: `taiyaku_library::import::
    /// reimport`, because `serve` must never fail to start over a stale
    /// alignment. Most often this is a hub that moved underneath a spoke —
    /// the guard at `merge_spoke`'s from-side check refuses precisely
    /// because the stored runs address a page sequence that no longer
    /// exists — but whatever the refusal, the stored edge is what could not
    /// be trusted, not what should keep blocking a start. The count is how
    /// many hand-corrected runs the dropped edge held: a human decision, not
    /// a tool's output, so — like `CarriedHandRuns` and unlike
    /// `DroppedCredits` — it is always reported rather than dropped quietly.
    DroppedHandRuns(usize),
}

/// What a correction did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corrected {
    /// A stored edge was rewritten in place.
    Edge,
    /// The pair resolved through the hub, so a direct edge was written for it.
    DirectEdgeAdded,
    /// The document already said this. Nothing changed.
    AlreadyTrue,
}

/// Why a correction was refused.
///
/// A typed refusal rather than a `String` because the only caller is an HTTP
/// route that has to answer a different status for each of these, and
/// string-matching a message to pick a status is the coupling that breaks the
/// first time someone improves the wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotCorrected {
    NoSuchEdition(String),
    NoSuchSlot(String),
    SameEdition(String),
    NoPath(String),
    /// A stored edge this document cannot read — a hand edit that made a span
    /// name a slot that does not exist, say. The document's problem, not the
    /// caller's.
    Unreadable(String),
}

impl std::fmt::Display for NotCorrected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NoSuchEdition(m)
            | Self::NoSuchSlot(m)
            | Self::SameEdition(m)
            | Self::NoPath(m)
            | Self::Unreadable(m) => m,
        };
        f.write_str(message)
    }
}

/// Where a slot falls in a chapter-organised edition.
///
/// `path` is the member's path as the sidecar records it, offered as a label.
/// Nothing here parses it: a fan release's `c000.cbz` may or may not be
/// chapter zero, and inferring a chapter number from a filename is precisely
/// what `Sidecar::chapter_at` exists to avoid — `notes.md` records series
/// where the declared names are wrong and the content-derived answer is right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterAt {
    /// Zero-based index of the member within the chapter-organised edition.
    pub member: usize,
    /// How many members that edition has.
    pub of: usize,
    /// The member's path exactly as the sidecar records it, offered as a
    /// label. Nothing parses it: a fan group's `c000.cbz` may or may not be
    /// chapter zero.
    pub path: String,
}

impl Sidecar {
    /// # Errors
    ///
    /// Returns an error if `text` is not valid JSON, or is JSON that does not
    /// match `taiyaku.json`'s shape — a missing required field such as
    /// `work` or `hub`, a `reading` value that is neither `"ltr"` nor `"rtl"`,
    /// an alignment entry whose span notation `SlotSpan::parse` rejects, and
    /// so on. The message embeds `serde_json`'s own, which names the field
    /// and byte offset.
    ///
    /// **Also if the document declares a format version this build does not
    /// read** — see [`Sidecar::readable_version`], which this calls once the
    /// shape is known to be right.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: Self =
            serde_json::from_str(text).map_err(|e| format!("not a readable taiyaku.json: {e}"))?;
        doc.readable_version()?;
        Ok(doc)
    }

    /// **A document declaring a format version newer than this build reads is
    /// refused.**
    ///
    /// Unknown *keys* are ignored on purpose — an L0 reader must tolerate the
    /// L1 and L2 fields it has no use for, which is what [`Sidecar`]'s own doc
    /// comment describes. A version bump is by definition the change that is
    /// not additive, and tolerating one stops being free the moment anything
    /// writes the document back: every command that reads a `taiyaku.json`,
    /// changes part of it and writes it out — `align --write`,
    /// `credits --write`, `regions`, `library import` followed by
    /// `export --in-place` — would write a copy stripped of every field it had
    /// no name for, stamped [`VERSION`], over the reader's only one, with
    /// nothing anywhere saying a field had been dropped.
    ///
    /// **Called from [`Sidecar::from_json`], which is why it is checked once
    /// rather than per command.** It is `pub` as well because a `Sidecar` can
    /// be built without being parsed — `taiyaku_library::project::sidecar`
    /// assembles one out of rows, and a caller can construct the struct
    /// outright — so the boundary a library crate wants to guard is not always
    /// one `from_json` was on the way in to.
    ///
    /// Refusing is the same stance `taiyaku_library::schema::migrate` takes on
    /// a database "written by a newer taiyaku", for the same reason: running an
    /// older binary over newer data corrupts what it does not understand. Only
    /// *newer* is refused. There is no older version to accept — [`VERSION`] is
    /// 1 — and whoever bumps it owns the question of what reading a 1 means,
    /// which is an upgrade path, not a comparison.
    ///
    /// # Errors
    ///
    /// Returns an error naming both versions if `self.taiyaku` exceeds
    /// [`VERSION`].
    pub fn readable_version(&self) -> Result<(), String> {
        refuse_newer("taiyaku", self.taiyaku, VERSION)
    }

    /// # Panics
    ///
    /// Panics if this document fails to serialise. In practice it cannot:
    /// the only hand-written `Serialize` impl here is `SlotSpan`'s, which
    /// writes as a plain string and cannot fail, and every other field is a
    /// number, string, `BTreeMap`, `Vec` or derived enum. `serde_json` only
    /// returns `Err` from `to_string_pretty` for a non-finite float used as
    /// a map key, and this document's only map keys (`editions`, by
    /// `String`) are not floats.
    #[must_use]
    pub fn to_json(&self) -> String {
        // Pretty-printed on purpose: the format is meant to diff in git, and a
        // one-line document defeats that entirely.
        serde_json::to_string_pretty(self).expect("sidecar is always serialisable")
    }

    /// Which edition of this document a set of members is, identified by
    /// content.
    ///
    /// Paths cannot be compared — the document's are relative to itself and a
    /// caller's to the working directory — so the `sha256` list is what
    /// identifies an edition. It is compared as a sorted set rather than in
    /// order, because an edition is identified by which files it holds and a
    /// caller passing a glob need not reproduce the order the document
    /// recorded. `merge_spoke` is the place that compares in order, because
    /// there the order is the page sequence being solved.
    pub fn edition_of(&self, sha256: &[String]) -> Option<&str> {
        let mut want: Vec<&str> = sha256.iter().map(String::as_str).collect();
        want.sort_unstable();
        self.editions.iter().find_map(|(name, edition)| {
            let mut have: Vec<&str> = edition.members.iter().map(|m| m.sha256.as_str()).collect();
            have.sort_unstable();
            (have == want).then_some(name.as_str())
        })
    }

    /// Add an edition and its alignment to a document that already exists.
    ///
    /// The rules live here rather than in the CLI because a browser reader
    /// writing a correction has to apply the same ones, and because they are
    /// worth testing without archives present.
    ///
    /// `from_side` is the freshly described `alignment.from` edition. It is
    /// passed rather than looked up because the check that matters is whether
    /// the document's copy and the just-aligned copy are the same content.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `reading` does not match the document's own reading order;
    /// - `anchor` is `Anchor::Hub` and `alignment.from` is not this
    ///   document's hub;
    /// - this document has no edition named `alignment.from`, or its stored
    ///   content (member `sha256`s, in order) does not match `from_side`;
    /// - `name` and `alignment.from` are the same edition;
    /// - a stored edge between them is recorded in the other direction and
    ///   holds a hand-corrected run, which this call cannot invert safely; or
    /// - `name`'s stored content differs from `edition`'s (a spoke being
    ///   replaced) while some alignment mentioning `name` holds a
    ///   hand-corrected run — carrying it onto different bytes would point
    ///   it at pages that may no longer exist.
    ///
    /// # Panics
    ///
    /// Does not panic in practice, though the body carries an `.expect("just
    /// inserted")` and four indexing operations that would if their
    /// invariants broke. The `.expect` follows
    /// `self.editions.insert(name.to_string(), edition)` on the same key, so
    /// the entry is always there. The two `members[i]` lookups in the
    /// content check take `i` from a `zip` position, so it is within both
    /// member lists. The other two (`self.editions[&start]`,
    /// `self.editions[name]`) are reached only when `carried > 0`, by which
    /// point `start`'s presence was confirmed earlier in this function and
    /// `name` was just inserted; the self-pairing check above already rules
    /// out `name == start`.
    #[expect(
        clippy::too_many_lines,
        reason = "six sequential guards (reading order, hub anchor, the from-side edition's \
                  presence and content match, self-pairing, the stored-edge orientation check \
                  O13 exists for, and the spoke-replaced hand-run check C1 exists for) each \
                  refuse a distinct way this merge could silently corrupt the document, and a \
                  seventh check counts the credits a replaced spoke loses so the answer can \
                  report them; after them comes the one mutation the guards were clearing the \
                  way for. The Panics doc comment above ties two indexing operations and one \
                  .expect() to the exact order these guards run in — moving a guard into a \
                  helper would separate its refusal from the invariant it establishes for the \
                  mutation below, which is the thing a reader of this function most needs kept \
                  in view."
    )]
    pub fn merge_spoke(
        &mut self,
        reading: Direction,
        anchor: Anchor,
        from_side: &Edition,
        name: &str,
        edition: Edition,
        alignment: Alignment,
    ) -> Result<Merged, String> {
        if reading != self.work.reading {
            return Err(format!(
                "this document reads {:?} and the alignment was solved {reading:?}; \
                 reading order decides which half of a spread comes first, so merging \
                 would misfile every spread in the file",
                self.work.reading
            ));
        }
        let start = alignment.from.clone();
        if anchor == Anchor::Hub && start != self.hub {
            return Err(format!(
                "the hub of this document is `{}`; align against it, or pass --direct \
                 to record a `{start}` -> `{name}` edge deliberately",
                self.hub
            ));
        }
        let stored = self
            .editions
            .get(&start)
            .ok_or_else(|| format!("this document defines no edition named `{start}`"))?;

        // Content, not paths. A path in the document is relative to the
        // document and the caller's is relative to the working directory, so
        // only the hashes can be compared — and their order matters, because it
        // is what defines the flattened page sequence.
        let mismatch = stored
            .members
            .iter()
            .zip(&from_side.members)
            .position(|(here, there)| here.sha256 != there.sha256);
        if let Some(i) = mismatch {
            return Err(format!(
                "`{start}` in this document is not the edition just aligned: member {i} is \
                 `{}` here and `{}` there. Re-solve the work rather than merging into it.",
                stored.members[i].path, from_side.members[i].path
            ));
        }
        if stored.members.len() != from_side.members.len() {
            return Err(format!(
                "`{start}` has {} member(s) in this document and {} in the alignment. \
                 Re-solve the work rather than merging into it.",
                stored.members.len(),
                from_side.members.len()
            ));
        }
        if name == start {
            return Err(format!(
                "`{name}` is the edition being aligned against; an edition cannot be its \
                 own counterpart"
            ));
        }

        // O13. `docs/sidecar.md` promises that a hand-corrected run survives
        // a re-solve.
        //
        // The guard above compares the *from* side: it refuses when the
        // document's copy of the hub is not the edition just aligned. The
        // spoke is a different question, and the answer divides. Where the
        // spoke's bytes are unchanged, a correction is a statement about those
        // bytes and carries across. Where the spoke is being replaced, a
        // correction addresses slots of an edition that is going away, and
        // carrying it would resurrect runs pointing into bytes that no longer
        // exist — the failure the from-side guard exists to prevent, let back
        // in through the other door. What *should* become of such a correction
        // is left open deliberately; refusing is what keeps it from being
        // answered silently.
        let stored_edge = self
            .alignments
            .iter()
            .find(|e| (e.from == start && e.to == name) || (e.from == name && e.to == start));
        // A hub edge is always stored `start` -> `name` — `merge_spoke` never
        // writes it any other way — but a direct edge can be stored in
        // whichever direction `Sidecar::correct` first created it in, and its
        // hand runs' spans are addressed against *its own* `from`, not
        // necessarily `start`. Carrying them in as though they were requires
        // inverting first, which `carry_hand_runs` does not do; splicing them
        // in unchanged would either misfile a hand run against the wrong
        // edition's pages or panic on a slot lookup that only makes sense in
        // the edge's own orientation. Refusing is what O13 exists to prefer
        // over either.
        if let Some(edge) = stored_edge
            && edge.from != start
            && edge.runs.iter().any(|r| r.by == Some(Source::Hand))
        {
            return Err(format!(
                "the stored edge between `{start}` and `{name}` is recorded `{}` -> `{}`, not \
                 `{start}` -> `{name}`, and holds hand-corrected run(s) addressed against \
                 `{}`'s slots. Carrying them onto `{start}` -> `{name}` without inverting them \
                 first would misfile them, so this refuses rather than doing that silently. \
                 Re-solve as `{}` -> `{}` to match how the edge is stored, or remove the \
                 `\"by\": \"hand\"` runs from `{}` -> `{}` deliberately.",
                edge.from, edge.to, edge.from, edge.from, edge.to, edge.from, edge.to
            ));
        }
        let hand: Vec<RunEntry> = stored_edge
            .map(|e| {
                e.runs
                    .iter()
                    .filter(|r| r.by == Some(Source::Hand))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        // Content, not paths — the same idiom as the from-side guard above,
        // and for the same reason: a spoke archive that was moved or renamed
        // with identical bytes is not a replacement, and comparing whole
        // `Member` structs (which include `path`) would misclassify it as
        // one, refusing a merge that holds hand runs or silently discarding
        // ones that do not.
        let spoke_replaced = self.editions.get(name).is_some_and(|held| {
            held.members.len() != edition.members.len()
                || held
                    .members
                    .iter()
                    .zip(&edition.members)
                    .any(|(here, there)| here.sha256 != there.sha256)
        });
        // A hand-corrected run addresses `name`'s slots no matter which
        // alignment stores it, and `start` <-> `name` is not the only one
        // that can. `Sidecar::correct` writes a *direct* edge to whichever
        // edition it is asked to pair `name` against when there is no stored
        // edge to rewrite, and that edge's hand runs are just as much at risk
        // as the ones on the edge being replaced here — so every alignment
        // mentioning `name` is checked, not only `stored_edge`.
        if spoke_replaced {
            let at_risk: Vec<(&str, &str, usize)> = self
                .alignments
                .iter()
                .filter(|a| a.from == name || a.to == name)
                .filter_map(|a| {
                    let hand = a.runs.iter().filter(|r| r.by == Some(Source::Hand)).count();
                    (hand > 0).then_some((a.from.as_str(), a.to.as_str(), hand))
                })
                .collect();
            if !at_risk.is_empty() {
                let total: usize = at_risk.iter().map(|&(_, _, n)| n).sum();
                let edges: Vec<String> = at_risk
                    .iter()
                    .map(|&(from, to, n)| format!("`{from}` -> `{to}` ({n})"))
                    .collect();
                return Err(format!(
                    "`{name}` in this document holds {total} hand-corrected run(s), on {}, \
                     and the edition being merged is not the one they were made against. A \
                     correction addresses slots of the edition it was made on, so carrying it \
                     onto different bytes would point it at pages that no longer exist. \
                     Re-solve the work, or remove the `\"by\": \"hand\"` run(s) deliberately.",
                    edges.join(", ")
                ));
            }
        }

        // Content, not paths — the same idiom as the from-side guard above,
        // and `spoke_replaced` already computed it: a `credits` list
        // addresses pages of specific bytes, so whether those bytes changed
        // is exactly what decides whether the list still means anything.
        let stored_credits = self
            .editions
            .get(name)
            .map(|e| e.credits.clone())
            .unwrap_or_default();
        let dropped = if spoke_replaced {
            stored_credits.len()
        } else {
            0
        };

        let replaced = self.editions.insert(name.to_string(), edition).is_some();
        if !spoke_replaced {
            // Unchanged bytes, so the list still names pages that exist —
            // restore it onto the freshly inserted edition. A no-op when
            // `name` is brand new: `stored_credits` is empty there too.
            self.editions.get_mut(name).expect("just inserted").credits = stored_credits;
        }
        // Only the `start` <-> `name` edge is replaced here — the one edge
        // this call is actually re-solving. A solver-only direct edge to a
        // third edition can still survive unchanged and stale, re-solving it
        // being a separate step this function does not take.
        // What the guard above does guarantee is narrower but load-bearing:
        // no *hand-corrected* edge to a third edition survives with stale
        // meaning, because `spoke_replaced` refused before this line runs.
        self.alignments
            .retain(|a| !(a.from == start && a.to == name || a.from == name && a.to == start));
        let mut alignment = alignment;
        // Carried onto the freshly solved edge by *replacing* the solver's run
        // wherever a hand run covers the same source slots, so the two cannot
        // both claim one slot. The hand run wins: that is the whole point of
        // the field.
        let carried = hand.len();
        if carried > 0 {
            // Slot tables, not page numbers: see `carry_hand_runs` for why
            // splitting needs them even though overlap detection alone would
            // not. `start`'s content was checked against `from_side` above and
            // `name`'s is unchanged in this branch (`spoke_replaced` is
            // false whenever `hand` is non-empty and this line is reached),
            // so both are the tables `hand` and the fresh solve were built
            // against.
            let a_slots = self.editions[&start].slots(reading);
            let b_slots = self.editions[name].slots(reading);
            alignment.runs = carry_hand_runs(&alignment.runs, &hand, &a_slots, &b_slots);
        }
        self.alignments.push(alignment);
        Ok(match (carried, replaced) {
            // A caller that has lost a list needs to hear that rather than
            // the fact of replacement, which it already asked for.
            (0, true) if dropped > 0 => Merged::DroppedCredits(dropped),
            (0, true) => Merged::Replaced,
            (0, false) => Merged::Added,
            _ => Merged::CarriedHandRuns(carried),
        })
    }

    /// Record by hand that slot `a` of `from` pairs with slot `b` of `to` — or,
    /// where `b` is `None`, with nothing.
    ///
    /// This is the operation the correction UI performs, and it lives here
    /// rather than in the server because it is format logic: any other reader
    /// writing back would need exactly this, and `docs/sidecar.md`'s promise
    /// that a hand-corrected run survives a re-solve is only meaningful if one
    /// place decides what a hand-corrected run *is*.
    ///
    /// Three cases, and the middle one is the subtle one:
    ///
    /// - The edge is stored in the direction asked for: rewrite it.
    /// - The edge is stored the other way round: **rewrite it in its own
    ///   direction**, not inverted-corrected-inverted. The assertion is
    ///   symmetric — "a pairs with b" and "b pairs with a" are one fact — and
    ///   `invert` drops gaps by construction, so a round trip through it would
    ///   discard every unpaired run the stored direction records.
    /// - There is no edge, so the pair resolves through the hub: write a direct
    ///   edge seeded from the composition with the fix applied. `sidecar.md`
    ///   permits a direct edge "for pairs where composition demonstrably
    ///   loses", and a hand correction is that demonstration. The cost is that
    ///   the pair leaves the star and stops inheriting transitivity.
    ///
    /// # Errors
    ///
    /// Returns `NotCorrected::SameEdition` if `from` and `to` name the same
    /// edition; `NotCorrected::NoSuchEdition` if either does not exist;
    /// `NotCorrected::NoSuchSlot` if `a` is not a slot `from` presents, or
    /// `b`, when given, is not a slot `to` presents; `NotCorrected::NoPath`
    /// if no edge, direct or through the hub, joins `from` and `to`; and
    /// `NotCorrected::Unreadable` if a stored edge between them holds a span
    /// this document can no longer read — a hand edit naming a slot that has
    /// since stopped existing.
    ///
    /// # Panics
    ///
    /// Panics if the edge just located by `edge_between` cannot be found
    /// again immediately afterwards by its `(from, to)` key. Nothing in this
    /// function mutates `self.alignments` between the two lookups in the
    /// branch that reaches this `.expect`, so the second lookup always
    /// finds what the first one did.
    pub fn correct(
        &mut self,
        from: &str,
        to: &str,
        a: Slot,
        b: Option<Slot>,
        note: Option<&str>,
    ) -> Result<Corrected, NotCorrected> {
        if from == to {
            return Err(NotCorrected::SameEdition(format!(
                "`{from}` is the edition being corrected against; an edition cannot be its own \
                 counterpart"
            )));
        }
        let reading = self.work.reading;
        let slots_of = |name: &str| {
            self.editions
                .get(name)
                .map(|e| e.slots(reading))
                .ok_or_else(|| {
                    NotCorrected::NoSuchEdition(format!(
                        "this document defines no edition named `{name}`"
                    ))
                })
        };
        let from_slots = slots_of(from)?;
        let to_slots = slots_of(to)?;
        let a_index = slot_position(slot_index(&from_slots, a).ok_or_else(|| {
            NotCorrected::NoSuchSlot(format!("`{from}` has no slot `{}`", show_slot(a)))
        })?);
        let b_index = match b {
            Some(b) => Some(slot_position(slot_index(&to_slots, b).ok_or_else(
                || NotCorrected::NoSuchSlot(format!("`{to}` has no slot `{}`", show_slot(b))),
            )?)),
            None => None,
        };

        let (key, source, target) = match self.edge_between(from, to) {
            Some(edge) if edge.from == from => {
                ((from.to_string(), to.to_string()), a_index, b_index)
            }
            Some(edge) => {
                let key = (edge.from.clone(), edge.to.clone());
                let runs = self.runs_of(edge).map_err(NotCorrected::Unreadable)?;
                // Hoisted out of the `None` arm so it stays a single expression (a
                // block here nudges clippy toward suggesting `if let`).
                let to_len = slot_position(to_slots.len());
                match b_index {
                    Some(b) => (key, b, Some(a_index)),
                    // "this slot pairs with nothing", read in the stored
                    // direction, is "whatever currently reaches it pairs with
                    // nothing".
                    None => {
                        match (0..to_len).find(|&s| crate::counterpart(&runs, s) == Some(a_index)) {
                            Some(owner) => (key, owner, None),
                            None => return Ok(Corrected::AlreadyTrue),
                        }
                    }
                }
            }
            None => {
                let composed = crate::route(self, from, to).ok_or_else(|| {
                    NotCorrected::NoPath(format!(
                        "no path from `{from}` to `{to}` in this document"
                    ))
                })?;
                if crate::counterpart(composed.runs(), a_index) == b_index {
                    return Ok(Corrected::AlreadyTrue);
                }
                let before = composed.runs().to_vec();
                let fixed = crate::correct_runs(&before, a_index, b_index);
                self.alignments.push(Alignment {
                    from: from.to_string(),
                    to: to.to_string(),
                    by: Source::Hand,
                    solver: None,
                    quality: None,
                    runs: Vec::new(),
                });
                let key = (from.to_string(), to.to_string());
                self.rewrite(&key, &fixed, &before, note)?;
                return Ok(Corrected::DirectEdgeAdded);
            }
        };

        let edge = self
            .alignments
            .iter()
            .find(|e| e.from == key.0 && e.to == key.1)
            .expect("orientation was found by the same lookup");
        let before = self.runs_of(edge).map_err(NotCorrected::Unreadable)?;
        if crate::counterpart(&before, source) == target {
            return Ok(Corrected::AlreadyTrue);
        }
        let fixed = crate::correct_runs(&before, source, target);
        self.rewrite(&key, &fixed, &before, note)?;
        Ok(Corrected::Edge)
    }

    /// Replace one edge's runs, mark by hand exactly what changed, and
    /// recompute the quality block beside them.
    ///
    /// `key` names the edge in its **stored** orientation, so everything here
    /// is in that edge's own slot space.
    fn rewrite(
        &mut self,
        key: &(String, String),
        runs: &[crate::Run],
        before: &[crate::Run],
        note: Option<&str>,
    ) -> Result<(), NotCorrected> {
        let reading = self.work.reading;
        let from_slots = self.editions[&key.0].slots(reading);
        let to_slots = self.editions[&key.1].slots(reading);
        let position = self
            .alignments
            .iter()
            .position(|e| e.from == key.0 && e.to == key.1)
            .expect("the edge was found before this was called");

        let idx = |slot: Slot| -> u32 {
            slot_position(slot_index(&from_slots, slot).expect("span was validated by runs_of"))
        };
        let from_len = slot_position(from_slots.len());

        // Exactly the slots *this* correction changed the answer for: the one
        // asserted, and the one displaced from the target it held. Derived by
        // diffing rather than tracked, so the two cannot drift apart.
        let changed: Vec<u32> = (0..from_len)
            .filter(|&s| crate::counterpart(before, s) != crate::counterpart(runs, s))
            .collect();
        // Every source slot an *earlier* hand decision already covers. Kept
        // separate from `changed` because the note below belongs only to this
        // correction: folding them together would let each correction
        // overwrite the note every earlier one left behind.
        let mut by_hand: Vec<u32> = changed.clone();
        for entry in self.alignments[position]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
        {
            by_hand.extend(idx(entry.a.start)..=idx(entry.a.end));
        }

        let mut entries =
            entries_from_runs(runs, &from_slots, &to_slots).map_err(NotCorrected::Unreadable)?;
        let previous = self.alignments[position].runs.clone();
        // Every earlier note, with the slot range it was written for — in
        // slot indices, not page notation, so overlap can be tested exactly
        // (two different slots can share a page). Only entries that carry a
        // note are worth keeping here; everything else contributes nothing
        // below regardless of overlap.
        let previous_notes: Vec<(u32, u32, &str)> = previous
            .iter()
            .filter_map(|e| {
                let note = e.note.as_deref()?;
                Some((idx(e.a.start), idx(e.a.end), note))
            })
            .collect();
        for entry in &mut entries {
            let start = idx(entry.a.start);
            let end = idx(entry.a.end);

            // Carry the margin of a run this correction did not touch.
            //
            // `entries_from_runs` writes `margin: None` for everything — it
            // rebuilds entries from bare runs and has no confidence figure to
            // hand them — so without this every correction would silently
            // strip the margins off the *whole* edge, not just the runs it
            // changed. A margin is the weakest per-page figure inside its
            // run, recoverable only by re-solving against the archives, and
            // `docs/sidecar.md` has it there precisely so a reader need not
            // open them.
            //
            // Matched on both spans being identical rather than on overlap: a
            // margin describes exactly the run it was computed over, so a run
            // that split, merged or moved has no figure that honestly applies
            // to it, and inheriting one would overstate what was
            // measured. Those come back as `None` — unknown, which is true —
            // until the edge is re-solved.
            entry.margin = previous
                .iter()
                .find(|e| e.a == entry.a && e.b == entry.b)
                .and_then(|e| e.margin);

            // Every slot of the run, not any of them. A correction that
            // changes the answer lands as its own single-slot run unless it
            // agrees with its neighbours, so `all` marks only what a
            // person decided. `any` would mark the whole run it sits inside,
            // and the refusal over hand-corrected runs would then fire on
            // every later re-solve of an edge somebody once touched.
            if !(start..=end).all(|s| by_hand.contains(&s)) {
                continue;
            }
            entry.by = Some(Source::Hand);

            // `compress` merges an index-adjacent run into this one with no
            // awareness of `by` or `note`, so a single hand-marked run here
            // can descend from more than one earlier correction — and this
            // one besides, where it also decided a slot inside it. Choosing
            // one note over the others would silently drop whichever
            // correction's explanation did not win, which is exactly the
            // silence this format is written against. So every distinct note
            // whose slots survive into this run is kept: the earlier ones by
            // span overlap (not span equality — a merged run's span matches
            // none of them exactly), in span order, and this correction's own
            // note last, where this run carries any slot it decided.
            let mut notes: Vec<String> = previous_notes
                .iter()
                .filter(|&&(ps, pe, _)| ps <= end && pe >= start)
                .map(|&(_, _, n)| n.to_string())
                .collect();
            if (start..=end).any(|s| changed.contains(&s))
                && let Some(n) = note
            {
                notes.push(n.to_string());
            }
            let mut deduped: Vec<String> = Vec::new();
            for n in notes {
                if !deduped.contains(&n) {
                    deduped.push(n);
                }
            }
            entry.note = if deduped.is_empty() {
                None
            } else {
                Some(deduped.join("; "))
            };
        }

        self.alignments[position].runs = entries;
        self.alignments[position].quality = Some(Quality {
            verdict: crate::verdict(runs, from_slots.len()),
            slots: from_len,
            paired: crate::paired_slots(runs),
            avg_run: rounded_avg_run(crate::average_run_length(runs)),
        });
        Ok(())
    }

    /// Rebuild solver runs from a stored alignment.
    ///
    /// The document addresses slots by page; the solver, the report and the
    /// verdict address them by index. This is the inverse of
    /// `entries_from_runs`, and it is what lets a hand-edited file drive a
    /// report or be judged.
    ///
    /// Validated here rather than left to `PageRange::new` and `Run::paired`,
    /// whose invariants are only `debug_assert`s: a hand-edited span can say
    /// anything, and a library query answering in `Result` has to turn a
    /// backwards or mismatched span into an `Err`, not a panic that a debug
    /// build — which is what the test suite and debug wasm both are — would
    /// turn into an abort.
    ///
    /// # Errors
    ///
    /// Returns an error if `alignment.from` or `alignment.to` names an
    /// edition this document does not define, if a run's span names a slot
    /// that does not exist in the edition it addresses (the message names
    /// the side and, when the span covers more than one slot, which endpoint
    /// failed), if a span's endpoints run backwards, or if a paired run's two
    /// spans cover different numbers of slots.
    pub fn runs_of(&self, alignment: &Alignment) -> Result<Vec<Run>, String> {
        let from_slots = self
            .editions
            .get(&alignment.from)
            .ok_or_else(|| format!("no edition named `{}`", alignment.from))?
            .slots(self.work.reading);
        let to_slots = self
            .editions
            .get(&alignment.to)
            .ok_or_else(|| format!("no edition named `{}`", alignment.to))?
            .slots(self.work.reading);

        // A span such as `4L` names a half of a page that need not be a spread.
        // Reporting "has no page 4" there is false — the page exists, the half
        // does not — and it sends a reader looking for something that is right
        // in front of them. The span is what they would search the file for, so
        // the message names it, and names the endpoint that failed when the
        // span covers more than one slot.
        let at = |slots: &[Slot], want: Slot, span: SlotSpan, side: &str| {
            slot_index(slots, want).map(slot_position).ok_or_else(|| {
                let slot = show_slot(want);
                if slot == span.to_string() {
                    format!("{side} has no slot `{slot}`")
                } else {
                    format!("{side} has no slot `{slot}` in span `{span}`")
                }
            })
        };

        // Resolve a span to a `PageRange`, rejecting one that runs backwards
        // before `PageRange::new` can assert on it. Named by its own text
        // (`span`) as well as its side, so the error points at what a person
        // would search for in the file.
        let range =
            |slots: &[Slot], span: SlotSpan, side: &str| -> Result<crate::PageRange, String> {
                let start = at(slots, span.start, span, side)?;
                let end = at(slots, span.end, span, side)?;
                if start > end {
                    return Err(format!("{side} span `{span}` starts after it ends"));
                }
                Ok(crate::PageRange::new(start, end))
            };

        alignment
            .runs
            .iter()
            .map(|entry| {
                let a = range(&from_slots, entry.a, "the source edition")?;
                Ok(match entry.b {
                    Some(b_span) => {
                        let b = range(&to_slots, b_span, "the target edition")?;
                        // `Run::paired` asserts the two sides are the same
                        // length; a hand edit can easily make that false, and
                        // that has to come back as an `Err` naming both spans
                        // rather than panic.
                        if a.len() != b.len() {
                            return Err(format!(
                                "the source edition span `{}` and the target edition span `{}` cover different numbers of slots",
                                entry.a, b_span
                            ));
                        }
                        Run::paired(a, b)
                    }
                    None => Run::gap(a),
                })
            })
            .collect()
    }

    /// The alignment relating two editions, in whichever direction it is
    /// stored. A verdict is a property of the pairing rather than of a
    /// direction, so either orientation answers.
    ///
    /// `pub` rather than `pub(crate)` because `taiyaku-cli` asks the same
    /// question — `verify` for the `by` it prints beside an alignment, `lookup`
    /// for the edge whose read error `route` swallowed. A copy of the two-way
    /// comparison in each would be another place for the orientation to be
    /// got wrong.
    #[must_use]
    pub fn edge_between(&self, from: &str, to: &str) -> Option<&Alignment> {
        self.alignments
            .iter()
            .find(|a| (a.from == from && a.to == to) || (a.from == to && a.to == from))
    }

    /// Recompute how believable an alignment is, from the runs in this
    /// document rather than from the `quality` block stored beside them.
    ///
    /// Recomputed rather than read because a hand-corrected alignment is a
    /// first-class citizen of this format, and an edited run list with an
    /// unedited verdict next to it is exactly the silent contradiction the
    /// document is supposed to prevent. Everything needed is present: the runs
    /// give the paired count and the run lengths, and the edition gives the
    /// slot count.
    ///
    /// `None` when the path cannot be established at all — see `trust`.
    #[must_use]
    pub fn quality_of(&self, from: &str, to: &str) -> Option<Quality> {
        let edge = self.edge_between(from, to)?;
        let runs = self.runs_of(edge).ok()?;
        // Runs are indexed by the alignment's own `from` side, whichever
        // direction the caller asked in.
        let slots = self.editions.get(&edge.from)?.slots(self.work.reading);

        Some(Quality {
            verdict: crate::verdict(&runs, slots.len()),
            slots: slot_position(slots.len()),
            paired: crate::paired_slots(&runs),
            avg_run: rounded_avg_run(crate::average_run_length(&runs)),
        })
    }

    /// How far a lookup from one edition to another can be trusted.
    ///
    /// An edition corresponds to itself by definition, not by anything a
    /// stored alignment says, so identity is answered before anything is
    /// consulted — a self-alignment full of confetti runs can leave
    /// `quality_of("x", "x")` at `Fragmented` while `trust("x", "x")` stays
    /// `Sound`, and that disagreement is correct rather than a bug: identity
    /// is a property of the edition, not of a run list that need not even
    /// exist.
    ///
    /// A direct edge, where the format carries one, is the whole answer —
    /// including a `None` from `quality_of` when the edge exists but cannot
    /// be read. Composing through the hub in that case would answer a
    /// different question than the one asked: not "is the edge the caller
    /// named trustworthy", but "is some other path trustworthy instead".
    /// Composition is used only when there is no direct edge at all, and then
    /// it is only as good as its weaker leg, so the two are combined with
    /// `min` over the verdict ordering.
    ///
    /// `None` means the document does not say, which is **not** the same as
    /// sound: a reader must not read silence as approval.
    #[must_use]
    pub fn trust(&self, from: &str, to: &str) -> Option<Verdict> {
        if !self.editions.contains_key(from) || !self.editions.contains_key(to) {
            return None;
        }
        if from == to {
            return Some(Verdict::Sound);
        }
        if self.edge_between(from, to).is_some() {
            return self.quality_of(from, to).map(|quality| quality.verdict);
        }
        let first = self.quality_of(from, &self.hub)?.verdict;
        let second = self.quality_of(&self.hub, to)?.verdict;
        Some(first.min(second))
    }

    /// Which chapter a slot of `from` falls in, or `None`.
    ///
    /// `None` covers three situations deliberately: this work has no
    /// chapter-organised edition (the common case, and not an error); there
    /// is no path from `from` to it; or this slot has no counterpart there.
    ///
    /// **Picking the chapter-organised edition is a heuristic, not a
    /// definition — nothing in the format declares an edition to be one.**
    /// This takes the edition assembled from the most members as a
    /// stand-in, on the theory that a chapter list has many files and a
    /// volume has one. A work with two multi-member editions is genuinely
    /// ambiguous under that theory; the tie is broken by edition key only so
    /// the answer is stable, not because a key ordering settles which
    /// edition is really the chapter list. If that case arises in practice
    /// it wants a real decision, not this guess.
    #[must_use]
    pub fn chapter_at(&self, from: &str, slot: Slot) -> Option<ChapterAt> {
        let (key, edition) = self
            .editions
            .iter()
            .filter(|(_, e)| e.members.len() > 1)
            .max_by(|a, b| a.1.members.len().cmp(&b.1.members.len()).then(b.0.cmp(a.0)))?;

        let landed = if key == from {
            slot
        } else {
            crate::route(self, from, key)?.resolve_slot(slot)?
        };
        let (member, _) = edition.locate(landed.page)?;
        Some(ChapterAt {
            member,
            of: edition.members.len(),
            path: edition.members[member].path.clone(),
        })
    }
}

/// Split `[start, end]` around every span in `cuts` that overlaps it, calling
/// `push(from, to, cut)` once for each surviving piece, in order: `cut =
/// false` for a stretch untouched by any cut, `cut = true` for a stretch
/// inside one. `cuts` need not be sorted or already clipped to `[start,
/// end]` — only the overlap is used.
///
/// Shared by `carry_hand_runs`'s two eviction passes, which differ only in
/// what a `cut` piece means to the caller: on the `a` side, a hand run's own
/// source span supplies that stretch itself, so the caller drops it; on the
/// `b` side, a solved slot that loses the target a hand run just claimed does
/// not stop existing, so the caller keeps it as a gap. Same shape, different
/// cuts, different fate for the cut piece — which is exactly why `cut` is
/// reported rather than decided here.
fn split_around(start: u32, end: u32, cuts: &[(u32, u32)], mut push: impl FnMut(u32, u32, bool)) {
    let mut sorted: Vec<(u32, u32)> = cuts
        .iter()
        .filter(|&&(cs, ce)| ce >= start && cs <= end)
        .map(|&(cs, ce)| (cs.max(start), ce.min(end)))
        .collect();
    sorted.sort_unstable();

    let mut cursor = start;
    for (cs, ce) in sorted {
        if cs > cursor {
            push(cursor, cs - 1, false);
        }
        if ce >= cursor {
            push(cs.max(cursor), ce, true);
            cursor = ce + 1;
        }
    }
    if cursor <= end {
        push(cursor, end, false);
    }
}

/// Splice hand-corrected runs into a freshly solved run list.
///
/// A hand run's source span is authoritative, so any solved run overlapping it
/// gives up the overlapping slots — but only the slots the hand run actually
/// covers, not the whole run it happens to sit inside.
///
/// Slot tables are needed, not just page order: overlap detection alone would
/// not need them, since page order tracks overlap exactly as well as slot
/// order does, but keeping what survives it does. Dropping a whole overlapped
/// solved run leaves its other slots with no counterpart, where
/// `docs/sidecar.md` promises only the corrected slots move. So this splits
/// each overlapping solved run around every hand run it contains rather than
/// discarding it, and converts back to page notation through the tables once
/// the splitting is done.
///
/// **Splitting around the hand runs' source spans restores 1:1 on the `a` side
/// only.** A surviving solved piece's *target* can still land on a slot a hand
/// run claims — reachable once the fresh solve disagrees in shape with the run
/// the hand corrections were made against. A second pass fixes it: after the
/// `a`-side split, any surviving piece whose `b` overlaps a hand run's `b`
/// gives up exactly the overlapping slots, becoming a gap there — the same rule
/// `correct_runs` applies to a single correction, the hand run winning because
/// that is the whole point of the field. This does not re-derive the merged
/// edge through `correct_runs` itself: that pipeline's `Run` carries no
/// `margin`, `by` or `note`, and re-deriving through `compress` can move a
/// piece's boundary away from both the solved run and the hand run it came
/// from, leaving nothing honest to reattach that metadata to. Splicing, then
/// evicting what the splice missed, keeps every piece traceable to the run it
/// inherits `margin` from and the hand run it deferred to.
///
/// Each surviving piece inherits its parent solved run's `margin` — except a
/// piece the second pass evicts, which does not: that figure was a measure of
/// confidence in the very pairing eviction just discarded, so carrying it
/// forward would describe a `b` this piece no longer claims.
///
/// Requires `hand` to be expressed in the same `a`-side edition as `solved`.
/// The caller (`merge_spoke`) guarantees this: a hub edge is always stored
/// `start` -> `name`, and a direct edge stored the other way round is refused
/// before this is called, rather than silently inverted or misfiled.
fn carry_hand_runs(
    solved: &[RunEntry],
    hand: &[RunEntry],
    a_slots: &[Slot],
    b_slots: &[Slot],
) -> Vec<RunEntry> {
    let index = |slots: &[Slot], slot: Slot| -> u32 {
        slot_position(
            slot_index(slots, slot)
                .expect("hand and solved runs are addressed in this edition's own slots"),
        )
    };

    // Hand spans in slot indices, sorted by where they start, so a solved run
    // overlapping more than one of them is split around all of them in a single
    // pass rather than one at a time.
    let mut hand_spans: Vec<(u32, u32)> = hand
        .iter()
        .map(|h| (index(a_slots, h.a.start), index(a_slots, h.a.end)))
        .collect();
    hand_spans.sort_unstable();

    // The hand runs' own targets, in slot indices. A surviving solved piece
    // whose target overlaps one of these has to give up the overlap: see the
    // second-pass paragraph above.
    let mut hand_b_spans: Vec<(u32, u32)> = hand
        .iter()
        .filter_map(|h| {
            h.b.map(|b| (index(b_slots, b.start), index(b_slots, b.end)))
        })
        .collect();
    hand_b_spans.sort_unstable();

    // Build one output piece from a template entry (for `margin`/`by`/`note`)
    // plus an `a`-range and the `b`-offset it was solved at. `forced_gap` is
    // what the eviction pass uses to turn a piece into a gap without losing
    // the `a`-range it still occupies.
    let build =
        |entry: &RunEntry, a_from: u32, a_to: u32, b_offset: Option<i64>, forced_gap: bool| {
            RunEntry {
                a: SlotSpan {
                    start: a_slots[a_from as usize],
                    end: a_slots[a_to as usize],
                },
                b: if forced_gap {
                    None
                } else {
                    b_offset.map(|offset| {
                        // Non-negative: a_from, a_to >= s_start by split_around's
                        // contract, and offset = index(b_slots, ..) - s_start, so
                        // x + offset >= index(b_slots, ..) >= 0 -- within
                        // slot_position_signed's bound. `as usize` below only
                        // widens a u32, which cannot lose information.
                        let at_b = |x: u32| -> Slot {
                            b_slots[slot_position_signed(i64::from(x) + offset) as usize]
                        };
                        let (start, end) = (at_b(a_from), at_b(a_to));
                        SlotSpan { start, end }
                    })
                },
                margin: if forced_gap { None } else { entry.margin },
                by: entry.by,
                note: entry.note.clone(),
            }
        };

    // First pass: carve each solved run around the hand runs' own source
    // spans. A `cut` piece here is dropped — the hand run itself supplies
    // that stretch, appended once below.
    let mut spliced = Vec::new();
    for entry in solved {
        let s_start = index(a_slots, entry.a.start);
        let s_end = index(a_slots, entry.a.end);
        // A paired run's two sides advance together one slot at a time, so
        // the offset from an `a` index to its `b` index at the run's own
        // start is the offset everywhere inside it.
        let b_offset = entry
            .b
            .map(|b| i64::from(index(b_slots, b.start)) - i64::from(s_start));
        split_around(s_start, s_end, &hand_spans, |from, to, cut| {
            if !cut {
                spliced.push(build(entry, from, to, b_offset, false));
            }
        });
    }

    // Second pass: evict whatever the first pass left holding a target one of
    // the hand runs now claims. `hand_b_spans` is translated into each
    // piece's own `a`-space through its own offset, since a piece's `b`
    // slots and its `a` slots are not the same indices.
    let mut evicted = Vec::new();
    for piece in &spliced {
        let a_start = index(a_slots, piece.a.start);
        let a_end = index(a_slots, piece.a.end);
        let Some(b) = piece.b else {
            evicted.push(piece.clone());
            continue;
        };
        let offset = i64::from(index(b_slots, b.start)) - i64::from(a_start);
        let cuts: Vec<(u32, u32)> = hand_b_spans
            .iter()
            .filter_map(|&(hs, he)| {
                let cs = i64::from(hs) - offset;
                let ce = i64::from(he) - offset;
                (ce >= i64::from(a_start) && cs <= i64::from(a_end)).then(|| {
                    // Clamped into [a_start, a_end] by .max/.min -- both bounds
                    // are u32, so slot_position_signed's bound holds.
                    (
                        slot_position_signed(cs.max(i64::from(a_start))),
                        slot_position_signed(ce.min(i64::from(a_end))),
                    )
                })
            })
            .collect();
        split_around(a_start, a_end, &cuts, |from, to, cut| {
            evicted.push(build(piece, from, to, Some(offset), cut));
        });
    }

    let mut out = evicted;
    out.extend(hand.iter().cloned());
    out.sort_by_key(|e| (e.a.start.page, e.a.start.half.is_some()));
    out
}

/// The document from `docs/sidecar.md`, verbatim: the real alignment of
/// Go-Toubun volume 1.
///
/// Shared with `route.rs`, whose tests resolve pages through it. Parsing it,
/// writing it back, and parsing that again has to give the same thing, or a
/// hand-edited file cannot survive a tool that rewrites it.
#[cfg(test)]
pub(crate) const REAL: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "Go-Toubun no Hanayome", "reading": "rtl" },
  "editions": {
    "raw-mono": {
      "label": "Raw, monochrome",
      "members": [
        { "path": "Go-Toubun no Hanayome - v01.cbz", "sha256": "3f9a", "pages": 195 }
      ],
      "spreads": []
    },
    "en-mono": {
      "label": "English official, Kodansha",
      "members": [
        { "path": "Go-Toubun no Hanayome - v01 (2018).cbz", "sha256": "c104", "pages": 187 }
      ],
      "spreads": [4, 45, 46, 49, 90, 121, 152]
    }
  },
  "hub": "raw-mono",
  "alignments": [
    {
      "from": "raw-mono",
      "to": "en-mono",
      "by": "solver",
      "solver": { "hash": "dhash-16", "floor": 0.68 },
      "runs": [
        { "a": "0", "b": "0", "margin": 0.09 },
        { "a": "1" },
        { "a": "2-5", "b": "1-4R", "margin": 0.15 },
        { "a": "6" },
        { "a": "7-194", "b": "5-186", "margin": 0.17 }
      ]
    }
  ]
}"#;

/// A two-edition document where `chapters` is assembled from three members
/// and `volume` from one — the shape a chapter-organised edition has.
#[cfg(test)]
const CHAPTERED: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "rtl" },
  "editions": {
    "volume": { "label": "V", "members": [{ "path": "v01.cbz", "sha256": "00", "pages": 6 }] },
    "chapters": { "label": "C", "members": [
      { "path": "c001.cbz", "sha256": "01", "pages": 2 },
      { "path": "c002.cbz", "sha256": "02", "pages": 2 },
      { "path": "c003.cbz", "sha256": "03", "pages": 2 }
    ] }
  },
  "hub": "volume",
  "alignments": [
    { "from": "volume", "to": "chapters", "by": "solver", "runs": [{ "a": "0-5", "b": "0-5" }] }
  ]
}"#;

/// A two-edition document where both editions are a single member, so
/// nothing in it is chapter-organised — the case `chapter_at` must answer
/// `None` for, without needing an alignment at all.
#[cfg(test)]
const FIXTURE_SINGLE_MEMBER_PAIR: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "rtl" },
  "editions": {
    "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 4 }] },
    "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 4 }] }
  },
  "hub": "a",
  "alignments": [
    { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] }
  ]
}"#;

/// Like `CHAPTERED`, but `volume` page 3 is a gap: a run with only an `a`
/// side, the way the format's own examples record a page with no
/// counterpart. Run sides stay equal in slots, not pages, though here there
/// are no spreads so the two coincide: `chapters` presents 5 slots (3 + 2)
/// to match the 5 paired `volume` slots, leaving the sixth `volume` slot —
/// page 3 — the one with no counterpart at all.
#[cfg(test)]
const CHAPTERED_WITH_A_GAP: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "rtl" },
  "editions": {
    "volume": { "label": "V", "members": [{ "path": "v01.cbz", "sha256": "00", "pages": 6 }] },
    "chapters": { "label": "C", "members": [
      { "path": "c001.cbz", "sha256": "01", "pages": 3 },
      { "path": "c002.cbz", "sha256": "02", "pages": 2 }
    ] }
  },
  "hub": "volume",
  "alignments": [
    { "from": "volume", "to": "chapters", "by": "solver", "runs": [
      { "a": "0-2", "b": "0-2" },
      { "a": "3" },
      { "a": "4-5", "b": "3-4" }
    ] }
  ]
}"#;

#[cfg(test)]
mod tests;
