//! Reading and writing `taiyaku.json`.
//!
//! The format is specified in `docs/sidecar.md`. It lives in the core so a
//! browser reader gets it without reimplementing anything.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Direction, Half, Run, Slot, Verdict};

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

/// Convert solved runs into the entries a sidecar stores.
///
/// The solver works in slot indices; the document is written in page terms. The
/// slot tables are what translate between them, and getting this wrong by one
/// would misfile every run in the file while still producing valid JSON.
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
    /// Skipped when empty, so a sidecar written before this feature existed
    /// round-trips byte for byte. An absent list and an empty one both mean
    /// nothing has been classified.
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
    pub fn slots(&self, reading: Direction) -> Vec<Slot> {
        let pages = self.total_pages();
        let wide: Vec<bool> = (0..pages).map(|p| self.spreads.contains(&p)).collect();
        crate::expand(&wide, reading)
    }
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
    #[serde(with = "span_text")]
    pub a: SlotSpan,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "span_option")]
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
/// because the threshold behind it is a moving policy: it is an average run
/// length of 27 slots today and was a runs-per-hundred-pages figure a week
/// ago, so a bare verdict would mean different things in two files written a
/// week apart.
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
    pub avg_run: f32,
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

/// Where an alignment being merged is allowed to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The star: the alignment must begin at the document's hub, so every
    /// edition is one hop from every other.
    Hub,
    /// A direct edge deliberately layered over the star, for a pair where
    /// composition demonstrably loses. The measurements name the candidate:
    /// the two editions furthest from the hub lose 3.5% to each other.
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
/// what this function exists to avoid — `notes.md` records two series where
/// the declared names are wrong and the content-derived answer is right.
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
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("not a readable taiyaku.json: {e}"))
    }

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

        // O13. `docs/sidecar.md` promises a hand-corrected run survives a
        // re-solve, and dropping the stored edge wholesale is what broke that
        // promise for as long as nothing could produce one.
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
        // C1: a hand-corrected run addresses `name`'s slots no matter which
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
        // this call is actually re-solving. It is not true that no surviving
        // edge can address a `name` that is now gone: a solver-only direct
        // edge to a third edition can still survive unchanged and stale,
        // re-solving it being a separate step this function does not take.
        // What the guard above does guarantee is narrower but load-bearing:
        // no *hand-corrected* edge to a third edition survives with stale
        // meaning, because `spoke_replaced` refused before this line runs.
        self.alignments
            .retain(|a| !(a.from == start && a.to == name) && !(a.from == name && a.to == start));
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
    pub fn correct(
        &mut self,
        from: &str,
        to: &str,
        a: Slot,
        b: Option<Slot>,
        note: Option<String>,
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
        let a_index = slot_index(&from_slots, a).ok_or_else(|| {
            NotCorrected::NoSuchSlot(format!("`{from}` has no slot `{}`", show_slot(a)))
        })? as u32;
        let b_index = match b {
            Some(b) => Some(slot_index(&to_slots, b).ok_or_else(|| {
                NotCorrected::NoSuchSlot(format!("`{to}` has no slot `{}`", show_slot(b)))
            })? as u32),
            None => None,
        };

        let (key, source, target) = match self.edge_between(from, to) {
            Some(edge) if edge.from == from => {
                ((from.to_string(), to.to_string()), a_index, b_index)
            }
            Some(edge) => {
                let key = (edge.from.clone(), edge.to.clone());
                let runs = self.runs_of(edge).map_err(NotCorrected::Unreadable)?;
                match b_index {
                    Some(b) => (key, b, Some(a_index)),
                    // "this slot pairs with nothing", read in the stored
                    // direction, is "whatever currently reaches it pairs with
                    // nothing".
                    None => match (0..to_slots.len() as u32)
                        .find(|&s| crate::counterpart(&runs, s) == Some(a_index))
                    {
                        Some(owner) => (key, owner, None),
                        None => return Ok(Corrected::AlreadyTrue),
                    },
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
                self.rewrite(&(from.to_string(), to.to_string()), fixed, &before, note)?;
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
        self.rewrite(&key, fixed, &before, note)?;
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
        runs: Vec<crate::Run>,
        before: &[crate::Run],
        note: Option<String>,
    ) -> Result<(), NotCorrected> {
        let reading = self.work.reading;
        let from_slots = self.editions[&key.0].slots(reading);
        let to_slots = self.editions[&key.1].slots(reading);
        let position = self
            .alignments
            .iter()
            .position(|e| e.from == key.0 && e.to == key.1)
            .expect("the edge was found before this was called");

        // Exactly the slots *this* correction changed the answer for: the one
        // asserted, and the one displaced from the target it held. Derived by
        // diffing rather than tracked, so the two cannot drift apart.
        let changed: Vec<u32> = (0..from_slots.len() as u32)
            .filter(|&s| crate::counterpart(before, s) != crate::counterpart(&runs, s))
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
            let start = slot_index(&from_slots, entry.a.start)
                .expect("span was validated by runs_of") as u32;
            let end =
                slot_index(&from_slots, entry.a.end).expect("span was validated by runs_of") as u32;
            by_hand.extend(start..=end);
        }

        let mut entries =
            entries_from_runs(&runs, &from_slots, &to_slots).map_err(NotCorrected::Unreadable)?;
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
                let start = slot_index(&from_slots, e.a.start)
                    .expect("span was validated by runs_of") as u32;
                let end =
                    slot_index(&from_slots, e.a.end).expect("span was validated by runs_of") as u32;
                Some((start, end, note))
            })
            .collect();
        for entry in entries.iter_mut() {
            let start = slot_index(&from_slots, entry.a.start)
                .expect("span was validated by runs_of") as u32;
            let end =
                slot_index(&from_slots, entry.a.end).expect("span was validated by runs_of") as u32;

            // Carry the margin of a run this correction did not touch.
            //
            // `entries_from_runs` writes `margin: None` for everything — it
            // rebuilds entries from bare runs and has no confidence figure to
            // hand them — so without this every correction silently stripped
            // the margins off the *whole* edge, not just the runs it changed.
            // Measured on the primary corpus volume before this was fixed: one
            // correction against `raw-mono -> en-fan` took that edge from 8
            // margins to 0, while the untouched `raw-mono -> en-mono` edge kept
            // all of its. A margin is the weakest per-page figure inside its
            // run, recoverable only by re-solving against the archives, and
            // `docs/sidecar.md` has it there precisely so a reader need not
            // open them.
            //
            // Matched on both spans being identical rather than on overlap: a
            // margin describes exactly the run it was computed over, so a run
            // that split, merged or moved has no figure that honestly applies
            // to it any more, and inheriting one would overstate what was
            // measured. Those come back as `None` — unknown, which is true —
            // until the edge is re-solved.
            entry.margin = previous
                .iter()
                .find(|e| e.a == entry.a && e.b == entry.b)
                .and_then(|e| e.margin);

            // Every slot of the run, not any of them. A correction that
            // changes the answer always lands as its own single-slot run, so
            // `all` marks exactly what a person decided. `any` would mark the
            // whole run it sits inside — measured at 188 slots on the primary
            // corpus volume — and Task 3's refusal would then fire on every
            // later re-solve of an edge somebody once touched.
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
            if (start..=end).any(|s| changed.contains(&s)) {
                if let Some(n) = &note {
                    notes.push(n.clone());
                }
            }
            let mut deduped: Vec<String> = Vec::new();
            for n in notes {
                if !deduped.contains(&n) {
                    deduped.push(n);
                }
            }
            entry.note = match deduped.is_empty() {
                true => None,
                false => Some(deduped.join("; ")),
            };
        }

        self.alignments[position].runs = entries;
        self.alignments[position].quality = Some(Quality {
            verdict: crate::verdict(&runs, from_slots.len()),
            slots: from_slots.len() as u32,
            paired: crate::paired_slots(&runs),
            avg_run: crate::average_run_length(&runs),
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
            slot_index(slots, want).map(|i| i as u32).ok_or_else(|| {
                let slot = show_slot(want);
                match slot == span.to_string() {
                    true => format!("{side} has no slot `{slot}`"),
                    false => format!("{side} has no slot `{slot}` in span `{span}`"),
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
    pub(crate) fn edge_between(&self, from: &str, to: &str) -> Option<&Alignment> {
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
    pub fn quality_of(&self, from: &str, to: &str) -> Option<Quality> {
        let edge = self.edge_between(from, to)?;
        let runs = self.runs_of(edge).ok()?;
        // Runs are indexed by the alignment's own `from` side, whichever
        // direction the caller asked in.
        let slots = self.editions.get(&edge.from)?.slots(self.work.reading);

        Some(Quality {
            verdict: crate::verdict(&runs, slots.len()),
            slots: slots.len() as u32,
            paired: crate::paired_slots(&runs),
            avg_run: crate::average_run_length(&runs),
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
    /// sound: every sidecar written before the `quality` block existed is
    /// silent, and a reader must not read silence as approval.
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
/// **An earlier version of this function filtered out whole solved runs by
/// page-range overlap instead of splitting them**, on the reasoning that the
/// two lists are already in the document's own page notation and converting
/// to slot indices and back would need both slot tables for no gain. That
/// reasoning covers *detecting* overlap, which page order tracks exactly as
/// well as slot order does — it does not cover *keeping what survives it*.
/// The preservation test below measured the difference: two one-slot
/// corrections at slots 5 and 9 inside a single 30-slot solved run left slots
/// 0-4, 6-8 and 10-29 with no counterpart at all under the drop-the-run
/// version, where `docs/sidecar.md` promises only the corrected slots move.
/// So this does need the slot tables, to split each overlapping solved run
/// around every hand run it contains rather than discard it, and converts
/// back to page notation through them once the splitting is done.
///
/// **C2: splitting around the hand runs' source spans restores 1:1 on the `a`
/// side only.** Nothing checked whether a surviving solved piece's *target*
/// already landed on a slot a hand run claims — reachable once the fresh
/// solve disagrees in shape with the run the hand corrections were made
/// against, which the unchanged-spoke fixture this function was written
/// against cannot exercise, because `correct` also hand-marks the slot it
/// displaces and that mark carries across and neutralises the old owner. A
/// second pass fixes it: after the `a`-side split, any surviving piece whose
/// `b` overlaps a hand run's `b` gives up exactly the overlapping slots,
/// becoming a gap there — the same rule `correct_runs` applies to a single
/// correction (`lib.rs:126-134`), the hand run winning because that is the
/// whole point of the field. This does not re-derive the merged edge through
/// `correct_runs` itself: that pipeline's `Run` carries no `margin`, `by` or
/// `note`, and re-deriving through `compress` can move a piece's boundary
/// away from both the solved run and the hand run it came from, leaving
/// nothing honest to reattach that metadata to. Splicing, then evicting what
/// the splice missed, keeps every piece traceable to the run it inherits
/// `margin` from and the hand run it deferred to.
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
        slot_index(slots, slot)
            .expect("hand and solved runs are addressed in this edition's own slots") as u32
    };

    // Hand spans in slot indices, sorted by where they start, so a solved run
    // overlapping more than one of them — as it does in the fixture above —
    // is split around all of them in a single pass rather than one at a time.
    let mut hand_spans: Vec<(u32, u32)> = hand
        .iter()
        .map(|h| (index(a_slots, h.a.start), index(a_slots, h.a.end)))
        .collect();
    hand_spans.sort_unstable();

    // The hand runs' own targets, in slot indices. A surviving solved piece
    // whose target overlaps one of these has to give up the overlap: see the
    // C2 paragraph above.
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
                    b_offset.map(|offset| SlotSpan {
                        start: b_slots[(a_from as i64 + offset) as usize],
                        end: b_slots[(a_to as i64 + offset) as usize],
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
            .map(|b| index(b_slots, b.start) as i64 - s_start as i64);
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
        let offset = index(b_slots, b.start) as i64 - a_start as i64;
        let cuts: Vec<(u32, u32)> = hand_b_spans
            .iter()
            .filter_map(|&(hs, he)| {
                let cs = hs as i64 - offset;
                let ce = he as i64 - offset;
                (ce >= a_start as i64 && cs <= a_end as i64)
                    .then(|| (cs.max(a_start as i64) as u32, ce.min(a_end as i64) as u32))
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

// Spans live as strings in the document, which is what makes a run readable.
mod span_text {
    use super::SlotSpan;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(span: &SlotSpan, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&span.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SlotSpan, D::Error> {
        let text = String::deserialize(d)?;
        SlotSpan::parse(&text).map_err(serde::de::Error::custom)
    }
}

mod span_option {
    use super::SlotSpan;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(span: &Option<SlotSpan>, s: S) -> Result<S::Ok, S::Error> {
        match span {
            Some(span) => s.serialize_str(&span.to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<SlotSpan>, D::Error> {
        let text = Option::<String>::deserialize(d)?;
        match text {
            Some(text) => SlotSpan::parse(&text)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
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
mod tests {
    use super::*;
    use crate::{Via, route};

    /// A two-edition document to merge into, and the pieces a merge needs.
    fn two_edition() -> Sidecar {
        Sidecar::from_json(
            r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 30 }] }
          },
          "hub": "a",
          "alignments": [
            { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-29", "b": "0-29" }] }
          ]
        }"#,
        )
        .expect("parses")
    }

    fn edition(path: &str, sha: &str) -> Edition {
        Edition {
            label: path.into(),
            members: vec![Member {
                path: path.into(),
                sha256: sha.into(),
                pages: 30,
            }],
            spreads: Vec::new(),
            credits: Vec::new(),
        }
    }

    fn edge(from: &str, to: &str) -> Alignment {
        Alignment {
            from: from.into(),
            to: to.into(),
            by: Source::Solver,
            solver: None,
            quality: None,
            runs: vec![RunEntry {
                a: SlotSpan::parse("0-29").unwrap(),
                b: Some(SlotSpan::parse("0-29").unwrap()),
                margin: None,
                by: None,
                note: None,
            }],
        }
    }

    fn edition_with_pages(path: &str, sha: &str, pages: u32) -> Edition {
        Edition {
            label: path.into(),
            members: vec![Member {
                path: path.into(),
                sha256: sha.into(),
                pages,
            }],
            spreads: Vec::new(),
            credits: Vec::new(),
        }
    }

    /// The invariant `correct_runs` protects for one correction
    /// (`lib.rs:126-134`), checked here against a *whole* merged run list:
    /// no target slot may be claimed twice. C2 is `carry_hand_runs` breaking
    /// exactly this while restoring the `a` side correctly, and nothing above
    /// `correct_runs` asserted `claimed == distinct` on the `b` side before —
    /// this is that assertion, shared so every merge test can run it.
    fn assert_b_side_is_one_to_one(runs: &[RunEntry], a_slots: &[Slot], b_slots: &[Slot]) {
        let mut owners: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for entry in runs {
            let Some(b) = entry.b else { continue };
            let b_start = slot_index(b_slots, b.start).expect("valid b span") as u32;
            let b_end = slot_index(b_slots, b.end).expect("valid b span") as u32;
            let a_start = slot_index(a_slots, entry.a.start).expect("valid a span") as u32;
            for offset in 0..=(b_end - b_start) {
                owners
                    .entry(b_start + offset)
                    .or_default()
                    .push(a_start + offset);
            }
        }
        let claimed: usize = owners.values().map(Vec::len).sum();
        let distinct = owners.len();
        let violations: Vec<String> = owners
            .iter()
            .filter(|(_, owners)| owners.len() > 1)
            .map(|(slot, owners)| format!("b slot {slot} is claimed by a slots {owners:?}"))
            .collect();
        assert_eq!(
            claimed,
            distinct,
            "the merged run list is not 1:1 on the b side: {}",
            violations.join("; ")
        );
    }

    /// A two-edition document like `two_edition`, but `b` carries one extra
    /// page `a` does not — its own front cover, say — so the stored run is
    /// `a "0-29"` against `b "1-30"`, a constant +1 offset. `two_edition`'s
    /// identity mapping cannot tell `carry_hand_runs`'s `b`-side arithmetic
    /// correct from off-by-one: every index a bug shifted by one there still
    /// lands on the right page by accident. This fixture is what pins it.
    fn two_edition_offset() -> Sidecar {
        Sidecar::from_json(
            r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 31 }] }
          },
          "hub": "a",
          "alignments": [
            { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-29", "b": "1-30" }] }
          ]
        }"#,
        )
        .expect("parses")
    }

    /// `verify` is handed archives, not edition names, and has to work out
    /// which pair of the document it is being asked about. Getting this wrong
    /// renders a confident report of the wrong alignment, which is the one
    /// failure the verification report exists to prevent.
    #[test]
    fn an_edition_is_identified_by_the_content_of_its_members() {
        let mut s = two_edition();
        s.editions.insert(
            "many".into(),
            Edition {
                label: "Many".into(),
                members: vec![
                    Member {
                        path: "c1.cbz".into(),
                        sha256: "aa".into(),
                        pages: 10,
                    },
                    Member {
                        path: "c2.cbz".into(),
                        sha256: "bb".into(),
                        pages: 10,
                    },
                ],
                spreads: Vec::new(),
                credits: Vec::new(),
            },
        );

        assert_eq!(s.edition_of(&["00".into()]), Some("a"));
        assert_eq!(s.edition_of(&["01".into()]), Some("b"));
        // Order is not identity: a glob need not reproduce what was recorded.
        assert_eq!(s.edition_of(&["bb".into(), "aa".into()]), Some("many"));
        // A subset is not the edition, and neither is unknown content.
        assert_eq!(s.edition_of(&["aa".into()]), None);
        assert_eq!(s.edition_of(&["ff".into()]), None);
        assert_eq!(s.edition_of(&[]), None);
    }

    /// The ordinary case, and the one the whole piece of work exists for: a
    /// third edition joins a document that had two, and the hub does not move.
    #[test]
    fn a_spoke_joins_an_existing_document() {
        let mut s = two_edition();

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "c",
                edition("c.cbz", "02"),
                edge("a", "c"),
            )
            .expect("merges");

        assert_eq!(merged, Merged::Added);
        assert_eq!(s.editions.len(), 3);
        assert_eq!(s.alignments.len(), 2);
        assert_eq!(s.hub, "a", "a merge must never move the hub");
        // And the point of it: the new edition is reachable from the old one.
        assert_eq!(
            route(&s, "b", "c").map(|r| r.via),
            Some(Via::Hub("a".into()))
        );
    }

    /// Re-solving a spoke after fixing its archive is ordinary, so a changed
    /// `sha256` on the spoke side still merges. Narrowed by O13: this is now
    /// specifically the *no hand runs* case, which is what keeps that fix a
    /// narrowing rather than a breaking change.
    #[test]
    fn re_aligning_a_spoke_with_no_hand_runs_replaces_it_rather_than_duplicating() {
        let mut s = two_edition();

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                edition("b2.cbz", "99"),
                edge("a", "b"),
            )
            .expect("merges");

        assert_eq!(merged, Merged::Replaced);
        assert_eq!(s.editions.len(), 2);
        assert_eq!(s.alignments.len(), 1, "the old edge must not survive");
        assert_eq!(s.editions["b"].members[0].sha256, "99");
    }

    /// `align --write` merges an edition in; `taiyaku credits --write` fills
    /// its `credits` list. A merge can therefore land on a list it knows
    /// nothing about, and an unchanged re-solve — the ordinary case above —
    /// must not discard it.
    #[test]
    fn re_merging_an_unchanged_spoke_keeps_its_credits() {
        let mut s = two_edition();
        s.editions.get_mut("b").unwrap().credits = vec![7, 51];

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                edition("b.cbz", "01"),
                edge("a", "b"),
            )
            .expect("merges");

        assert_ne!(merged, Merged::Added, "the spoke already existed");
        assert_eq!(
            s.editions["b"].credits,
            vec![7, 51],
            "unchanged bytes must keep the list: the pages it names still exist"
        );
    }

    /// A replaced spoke's `credits` list names pages of bytes that no longer
    /// exist. Dropped rather than refused: unlike a hand-corrected run (O13),
    /// nothing here is a human decision — `taiyaku credits` regenerates the
    /// list in one command.
    #[test]
    fn replacing_a_spoke_drops_its_credits_and_says_how_many() {
        let mut s = two_edition();
        s.editions.get_mut("b").unwrap().credits = vec![7, 51];

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                edition("b2.cbz", "99"),
                edge("a", "b"),
            )
            .expect("merges");

        assert_eq!(
            merged,
            Merged::DroppedCredits(2),
            "a list addressing pages that no longer exist must not survive silently"
        );
        assert!(s.editions["b"].credits.is_empty());
    }

    /// A spoke whose bytes are unchanged is the ordinary re-solve: the solver
    /// ran again over the same two editions, and a run somebody fixed by hand
    /// is a statement about those bytes that the solver has no better claim
    /// on. `docs/sidecar.md`: "a hand-corrected run must survive a re-solve;
    /// this is the field that makes the correction UI more than a viewer."
    ///
    /// Two corrections land in the fixture's single 30-slot run — one at slot
    /// 5, one at slot 9 — deliberately, so this pins more than "the corrected
    /// slot still says what it said": it pins that the *rest* of the run,
    /// including the slots strictly between the two corrections, is not
    /// collateral damage. `carry_hand_runs`'s first cut dropped the whole
    /// 30-slot solved run wherever any hand run overlapped it, which this
    /// fixture's own shape — one long run — hid completely if only the
    /// corrected slots themselves were checked.
    #[test]
    fn a_hand_run_survives_a_re_solve_of_an_unchanged_spoke() {
        let mut s = two_edition();
        s.correct(
            "a",
            "b",
            Slot {
                page: 5,
                half: None,
            },
            Some(Slot {
                page: 9,
                half: None,
            }),
            None,
        )
        .expect("corrects");
        let hand_before = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .count();
        assert!(hand_before > 0, "the fixture must actually hold a hand run");

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                edition("b.cbz", "01"),
                edge("a", "b"),
            )
            .expect("an unchanged spoke merges");

        assert_eq!(merged, Merged::CarriedHandRuns(hand_before));
        assert_eq!(
            s.alignments[0]
                .runs
                .iter()
                .filter(|e| e.by == Some(Source::Hand))
                .count(),
            hand_before,
            "the corrections are still there after the solver's runs replaced the rest"
        );
        assert_b_side_is_one_to_one(
            &s.alignments[0].runs,
            &s.editions["a"].slots(Direction::RightToLeft),
            &s.editions["b"].slots(Direction::RightToLeft),
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 5),
            Some(Slot {
                page: 9,
                half: None
            }),
            "and they still say what they said"
        );
        // The slots either side of the carried corrections, pinned by
        // measurement rather than assumed: a helper that drops the whole
        // overlapping solved run instead of splitting it around the hand
        // run's span would answer `None` for all three of these, since the
        // fixture's entire 30-slot solved run overlaps both corrections.
        assert_eq!(
            crate::resolve(&s, "a", "b", 4),
            Some(Slot {
                page: 4,
                half: None
            }),
            "the slot just before the first correction must still resolve"
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 7),
            Some(Slot {
                page: 7,
                half: None
            }),
            "a slot strictly between the two corrections must still resolve"
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 10),
            Some(Slot {
                page: 10,
                half: None
            }),
            "the slot just after the second correction must still resolve"
        );
    }

    /// The case the from-side guard never covered. Re-solving a spoke after
    /// fixing its archive is ordinary and stays allowed — but a correction
    /// addresses slots of the edition that is being replaced, so carrying it
    /// across would resurrect runs pointing into bytes that are gone. What
    /// should become of such a correction is a real question, deliberately
    /// left open in `docs/status.md`; refusing is what stops it being answered
    /// silently.
    #[test]
    fn replacing_a_spoke_that_holds_hand_runs_is_refused() {
        let mut s = two_edition();
        s.correct(
            "a",
            "b",
            Slot {
                page: 5,
                half: None,
            },
            Some(Slot {
                page: 9,
                half: None,
            }),
            None,
        )
        .expect("corrects");

        let refused = s.merge_spoke(
            Direction::RightToLeft,
            Anchor::Hub,
            &edition("a.cbz", "00"),
            "b",
            edition("b2.cbz", "99"),
            edge("a", "b"),
        );

        let message = refused.expect_err("a replaced spoke with corrections must not merge");
        assert!(message.contains("`b`"), "must name the spoke: {message}");
        assert!(
            message.contains("hand"),
            "must say what is at risk: {message}"
        );
        assert!(
            message.chars().any(|c| c.is_ascii_digit()),
            "must say how many: {message}"
        );
        assert_eq!(
            s.editions["b"].members[0].sha256, "01",
            "a refused merge must change nothing"
        );
    }

    /// `spoke_replaced` must compare content, not paths — the same rule the
    /// from-side guard already applies and the design table already states
    /// ("Spoke `sha256` unchanged"). A spoke archive that was moved or
    /// renamed with identical bytes is the ordinary unchanged-spoke case, not
    /// a replacement: it must carry the hand run across, not refuse.
    #[test]
    fn re_solving_a_spoke_whose_archive_moved_carries_hand_runs_rather_than_refusing() {
        let mut s = two_edition();
        s.correct(
            "a",
            "b",
            Slot {
                page: 5,
                half: None,
            },
            Some(Slot {
                page: 9,
                half: None,
            }),
            None,
        )
        .expect("corrects");
        let hand_before = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .count();
        assert!(hand_before > 0, "the fixture must actually hold a hand run");

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                // Same sha256 as the stored `b`, different path: a rename or
                // a move, not a content change.
                edition("b-moved.cbz", "01"),
                edge("a", "b"),
            )
            .expect("a moved-but-unchanged spoke merges and carries");

        assert_eq!(merged, Merged::CarriedHandRuns(hand_before));
        assert_eq!(s.editions["b"].members[0].path, "b-moved.cbz");
        assert_b_side_is_one_to_one(
            &s.alignments[0].runs,
            &s.editions["a"].slots(Direction::RightToLeft),
            &s.editions["b"].slots(Direction::RightToLeft),
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 5),
            Some(Slot {
                page: 9,
                half: None
            }),
            "the correction must survive a path-only change"
        );
    }

    /// A hub edge is always stored `start` -> `name`, but a direct edge can
    /// be stored in whichever direction `Sidecar::correct` first created it
    /// in. Here `correct("c", "b", ...)` has no existing `b`/`c` edge to
    /// rewrite, so it composes through the hub and writes a *new* direct edge
    /// `c -> b` — and that edge's hand run is addressed against `c`'s slots.
    /// Re-merging the same pair as `b -> c` must not splice that hand run in
    /// as though it were addressed against `b`.
    #[test]
    fn a_direct_edge_stored_the_other_way_round_refuses_rather_than_misapplying_hand_runs() {
        let mut s = two_edition();
        s.editions.insert("c".into(), edition("c.cbz", "02"));
        s.alignments.push(edge("a", "c"));
        s.correct(
            "c",
            "b",
            Slot {
                page: 5,
                half: None,
            },
            Some(Slot {
                page: 9,
                half: None,
            }),
            None,
        )
        .expect("corrects through the hub, creating a new c -> b direct edge");
        assert_eq!(
            (s.alignments[2].from.as_str(), s.alignments[2].to.as_str()),
            ("c", "b"),
            "the fixture must actually hold the edge the other way round"
        );
        assert!(
            s.alignments[2]
                .runs
                .iter()
                .any(|e| e.by == Some(Source::Hand)),
            "the fixture must actually hold a hand run"
        );

        let refused = s.merge_spoke(
            Direction::RightToLeft,
            Anchor::Direct,
            &edition("b.cbz", "01"),
            "c",
            edition("c.cbz", "02"),
            Alignment {
                from: "b".into(),
                to: "c".into(),
                by: Source::Solver,
                solver: None,
                quality: None,
                runs: vec![RunEntry {
                    a: SlotSpan::parse("0-29").unwrap(),
                    b: Some(SlotSpan::parse("0-29").unwrap()),
                    margin: None,
                    by: None,
                    note: None,
                }],
            },
        );

        let message =
            refused.expect_err("an orientation-mismatched hand edge must not merge silently");
        assert!(
            message.contains("`b`") && message.contains("`c`"),
            "must name both editions: {message}"
        );
        assert_eq!(
            (s.alignments[2].from.as_str(), s.alignments[2].to.as_str()),
            ("c", "b"),
            "a refused merge must change nothing about the stored edge"
        );
    }

    /// `two_edition`'s fixture pairs `a` and `b` 1:1, so `carry_hand_runs`'s
    /// `b`-offset arithmetic is zero in every other test here — an off-by-one
    /// in the cast from a slot index to `b_slots` would still pass the whole
    /// suite. `two_edition_offset` gives `b` a constant +1 offset instead, so
    /// this pins a split piece's `b` span on both sides of a carried hand
    /// run, not just its `a` span.
    #[test]
    fn carried_split_pieces_keep_the_correct_b_offset() {
        let mut s = two_edition_offset();
        // a5 <-> b6 under the stored +1 offset; correct it to b20 instead.
        // b20 was a19's counterpart, so a19 is displaced to a gap — the same
        // "asserted slot plus displaced slot" shape every other correction
        // test in this file relies on.
        s.correct(
            "a",
            "b",
            Slot {
                page: 5,
                half: None,
            },
            Some(Slot {
                page: 20,
                half: None,
            }),
            None,
        )
        .expect("corrects");
        let hand_before = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .count();
        assert_eq!(hand_before, 2, "the asserted slot and the one it displaced");

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition_with_pages("a.cbz", "00", 30),
                "b",
                edition_with_pages("b.cbz", "01", 31),
                Alignment {
                    from: "a".into(),
                    to: "b".into(),
                    by: Source::Solver,
                    solver: None,
                    quality: None,
                    runs: vec![RunEntry {
                        a: SlotSpan::parse("0-29").unwrap(),
                        b: Some(SlotSpan::parse("1-30").unwrap()),
                        margin: None,
                        by: None,
                        note: None,
                    }],
                },
            )
            .expect("an unchanged spoke merges");
        assert_eq!(merged, Merged::CarriedHandRuns(2));
        assert_b_side_is_one_to_one(
            &s.alignments[0].runs,
            &s.editions["a"].slots(Direction::RightToLeft),
            &s.editions["b"].slots(Direction::RightToLeft),
        );

        // The corrected slot and the one it displaced.
        assert_eq!(
            crate::resolve(&s, "a", "b", 5),
            Some(Slot {
                page: 20,
                half: None
            })
        );
        assert_eq!(crate::resolve(&s, "a", "b", 19), None, "displaced to a gap");
        // The split piece to the left of the correction: the +1 offset holds
        // both at its far end and right up against the hand run.
        assert_eq!(
            crate::resolve(&s, "a", "b", 0),
            Some(Slot {
                page: 1,
                half: None
            })
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 4),
            Some(Slot {
                page: 5,
                half: None
            })
        );
        // The split piece to the right of the displaced gap: the offset
        // survives the split rather than resetting to the run's own start.
        assert_eq!(
            crate::resolve(&s, "a", "b", 20),
            Some(Slot {
                page: 21,
                half: None
            })
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 29),
            Some(Slot {
                page: 30,
                half: None
            })
        );
    }

    /// C2. `carry_hand_runs` splits solved runs around a hand run's **source**
    /// span, but did nothing about a solved piece's **target** landing on a
    /// slot a hand run already claims. The unchanged-spoke fixture used
    /// everywhere else in this file hides it, because `correct` also
    /// hand-marks the slot it displaced, and that mark carries across and
    /// neutralises the old owner — but only while the fresh solve agrees with
    /// the old one about where everything goes. Re-solving with a genuinely
    /// different run shape (as a newer solver or a different `floor` would)
    /// does not agree, and the reviewer's reproduction is exactly this: a
    /// fresh solve shifted by one slot from the run the hand corrections were
    /// made against.
    ///
    /// Before the fix, slot `a` 10 keeps the counterpart (`b` 9) the fresh
    /// solve's own offset gives it, even though the hand run at `a` 5 already
    /// claims `b` 9 — the exact collision `correct_runs` exists to prevent.
    /// `b` 9 then answers `a` 5 one way and `a` 10 the other, which is the
    /// silent page move `notes.md`'s "swapping must not round-trip through
    /// the alignment" is about.
    #[test]
    fn carrying_hand_runs_across_a_reshaped_solve_evicts_the_collision() {
        let mut s = two_edition();
        s.correct(
            "a",
            "b",
            Slot {
                page: 5,
                half: None,
            },
            Some(Slot {
                page: 9,
                half: None,
            }),
            None,
        )
        .expect("corrects");
        let hand_before = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .count();
        assert_eq!(hand_before, 2, "the asserted slot and the one it displaced");

        // A fresh solve of the same pair, shaped differently from the run the
        // hand corrections were made against: slot 4 is now a gap, and
        // everything from slot 5 on is offset by -1 rather than lining up
        // exactly. `SolverInfo { hash, floor }` exists to record that a
        // re-solve can look like this.
        let fresh = Alignment {
            from: "a".into(),
            to: "b".into(),
            by: Source::Solver,
            solver: None,
            quality: None,
            runs: vec![
                RunEntry {
                    a: SlotSpan::parse("0-3").unwrap(),
                    b: Some(SlotSpan::parse("0-3").unwrap()),
                    margin: None,
                    by: None,
                    note: None,
                },
                RunEntry {
                    a: SlotSpan::parse("4").unwrap(),
                    b: None,
                    margin: None,
                    by: None,
                    note: None,
                },
                RunEntry {
                    a: SlotSpan::parse("5-29").unwrap(),
                    b: Some(SlotSpan::parse("4-28").unwrap()),
                    margin: None,
                    by: None,
                    note: None,
                },
            ],
        };

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                edition("b.cbz", "01"),
                fresh,
            )
            .expect("an unchanged spoke merges even when the fresh solve disagrees in shape");
        assert_eq!(merged, Merged::CarriedHandRuns(2));

        let a_slots = s.editions["a"].slots(Direction::RightToLeft);
        let b_slots = s.editions["b"].slots(Direction::RightToLeft);
        assert_b_side_is_one_to_one(&s.alignments[0].runs, &a_slots, &b_slots);

        // The hand runs still say what they said.
        assert_eq!(
            crate::resolve(&s, "a", "b", 5),
            Some(Slot {
                page: 9,
                half: None
            }),
            "the corrected slot"
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 9),
            None,
            "the slot the correction displaced"
        );
        // Slot 10 is where the fresh solve's own -1 offset would also land on
        // b9 — the collision. The hand run wins, so slot 10 gives up its
        // target rather than silently sharing it.
        assert_eq!(
            crate::resolve(&s, "a", "b", 10),
            None,
            "evicted: the fresh solve's own offset would have landed slot 10 on b9 too"
        );
        // Everything else the fresh solve said still stands.
        assert_eq!(
            crate::resolve(&s, "a", "b", 4),
            None,
            "the fresh solve's own gap"
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 6),
            Some(Slot {
                page: 5,
                half: None
            })
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 8),
            Some(Slot {
                page: 7,
                half: None
            })
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 11),
            Some(Slot {
                page: 10,
                half: None
            }),
            "the slot just after the evicted one keeps the fresh solve's own offset"
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 29),
            Some(Slot {
                page: 28,
                half: None
            })
        );
    }

    /// C1. `merge_spoke` looked for hand runs only on the edge between the
    /// two editions being merged (`start` <-> `name`), and `Sidecar::correct`
    /// can now write a hand-corrected *direct* edge to a **third** edition —
    /// `b -> c` here, while the spoke being re-solved is `a -> b`. That direct
    /// edge is not the one being replaced, so it was never inspected, and a
    /// spoke whose bytes changed merged silently while the stale `b -> c`
    /// edge — hand runs and all — kept addressing the old `b`'s slots.
    #[test]
    fn replacing_a_spoke_whose_hand_corrected_edge_is_to_a_third_edition_is_refused() {
        let mut s = two_edition();
        s.editions.insert("c".into(), edition("c.cbz", "02"));
        s.alignments.push(edge("a", "c"));

        s.correct(
            "b",
            "c",
            Slot {
                page: 5,
                half: None,
            },
            Some(Slot {
                page: 9,
                half: None,
            }),
            None,
        )
        .expect("corrects through the hub, creating a new b -> c direct edge");
        assert_eq!(
            (s.alignments[2].from.as_str(), s.alignments[2].to.as_str()),
            ("b", "c"),
            "the fixture must actually hold the edge the other way round from the spoke being re-solved"
        );
        let hand_on_bc = s.alignments[2]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .count();
        assert_eq!(hand_on_bc, 2, "the fixture must actually hold hand runs");

        // Re-solve `b` against the hub, as an ordinary re-scan would — same
        // page count, different bytes. This never touches the `a` <-> `b`
        // edge's own hand runs (there are none), only the `b` -> `c` edge
        // that addresses `b`'s slots from the other side.
        let refused = s.merge_spoke(
            Direction::RightToLeft,
            Anchor::Hub,
            &edition("a.cbz", "00"),
            "b",
            edition("b2.cbz", "99"),
            edge("a", "b"),
        );

        let message = refused.expect_err(
            "a spoke whose bytes changed must not merge while a hand-corrected edge to a \
             third edition still addresses its old slots",
        );
        assert!(message.contains("`b`"), "must name the spoke: {message}");
        assert!(
            message.contains("hand"),
            "must say what is at risk: {message}"
        );
        assert!(
            message.chars().any(|c| c.is_ascii_digit()),
            "must say how many: {message}"
        );
        assert_eq!(
            s.editions["b"].members[0].sha256, "01",
            "a refused merge must change nothing"
        );
        assert_eq!(
            (s.alignments[2].from.as_str(), s.alignments[2].to.as_str()),
            ("b", "c"),
            "a refused merge must change nothing about the stored edge"
        );
        assert_eq!(
            s.alignments[2]
                .runs
                .iter()
                .filter(|e| e.by == Some(Source::Hand))
                .count(),
            2,
            "a refused merge must change nothing about the hand runs either"
        );
    }

    /// The answer to "what does a third edition align against". The star says
    /// the hub, and the document names its own hub, so this is checkable rather
    /// than conventional.
    #[test]
    fn a_spoke_aligned_against_a_non_hub_edition_is_refused() {
        let mut s = two_edition();

        let err = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("b.cbz", "01"),
                "c",
                edition("c.cbz", "02"),
                edge("b", "c"),
            )
            .expect_err("must refuse");

        assert!(err.contains("`a`"), "the error must name the hub: {err}");
        assert!(err.contains("--direct"), "and the way round it: {err}");
        assert_eq!(s.editions.len(), 2, "a refused merge must change nothing");
    }

    /// `Anchor::Direct` is the mitigation the format already designs: a direct
    /// edge for a pair where composition demonstrably loses.
    #[test]
    fn a_direct_edge_may_start_anywhere_the_document_defines() {
        let mut s = two_edition();

        s.merge_spoke(
            Direction::RightToLeft,
            Anchor::Direct,
            &edition("b.cbz", "01"),
            "c",
            edition("c.cbz", "02"),
            edge("b", "c"),
        )
        .expect("merges");

        assert_eq!(s.alignments.len(), 2);
        assert_eq!(s.alignments[1].from, "b");
        assert_eq!(s.hub, "a");
    }

    /// Reading direction decides which half of a joined spread comes first, so
    /// merging under the wrong one misfiles every spread in the file while
    /// producing valid JSON.
    #[test]
    fn a_merge_under_the_wrong_reading_direction_is_refused() {
        let mut s = two_edition();

        let err = s
            .merge_spoke(
                Direction::LeftToRight,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "c",
                edition("c.cbz", "02"),
                edge("a", "c"),
            )
            .expect_err("must refuse");

        assert!(err.contains("reading"), "{err}");
    }

    /// The library holds two files both claiming to be volume 1 of the same
    /// work, only one of which is. If the hub's archives changed under the
    /// document, every existing spoke is aligned to something else.
    #[test]
    fn a_merge_against_changed_hub_archives_is_refused() {
        let mut s = two_edition();

        let err = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a-redownloaded.cbz", "ff"),
                "c",
                edition("c.cbz", "02"),
                edge("a", "c"),
            )
            .expect_err("must refuse");

        assert!(
            err.contains("a-redownloaded.cbz"),
            "the error must name the file: {err}"
        );
        assert_eq!(s.editions.len(), 2);
    }

    fn whole(page: u32) -> Slot {
        Slot { page, half: None }
    }
    fn right(page: u32) -> Slot {
        Slot {
            page,
            half: Some(Half::Right),
        }
    }
    fn left(page: u32) -> Slot {
        Slot {
            page,
            half: Some(Half::Left),
        }
    }

    /// Every shape the notation has to carry, and the round trip that keeps a
    /// hand-edited file from being silently rewritten into something else.
    #[test]
    fn slot_spans_round_trip_through_their_text_form() {
        let cases = [
            (
                "7",
                SlotSpan {
                    start: whole(7),
                    end: whole(7),
                },
            ),
            (
                "2-5",
                SlotSpan {
                    start: whole(2),
                    end: whole(5),
                },
            ),
            (
                "4R",
                SlotSpan {
                    start: right(4),
                    end: right(4),
                },
            ),
            (
                "1-4R",
                SlotSpan {
                    start: whole(1),
                    end: right(4),
                },
            ),
            (
                "45R-46L",
                SlotSpan {
                    start: right(45),
                    end: left(46),
                },
            ),
        ];

        for (text, expected) in cases {
            assert_eq!(SlotSpan::parse(text), Ok(expected), "parsing {text}");
            assert_eq!(expected.to_string(), text, "formatting {text}");
        }
    }

    #[test]
    fn credits_round_trip_present_empty_and_absent() {
        // Present: parsed, and emitted again in the same order.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "a.cbz", "sha256": "aa", "pages": 4 }],
              "spreads": [],
              "credits": [1, 3]
            }
          },
          "hub": "raw",
          "alignments": []
        }"#;
        let parsed = Sidecar::from_json(text).expect("parses");
        assert_eq!(parsed.editions["raw"].credits, vec![1, 3]);
        let again = Sidecar::from_json(&parsed.to_json()).expect("re-parses");
        assert_eq!(again.editions["raw"].credits, vec![1, 3]);

        // Absent: defaults to empty rather than failing to parse.
        let without = text.replace(",\n              \"credits\": [1, 3]", "");
        let parsed = Sidecar::from_json(&without).expect("parses without credits");
        assert!(parsed.editions["raw"].credits.is_empty());
    }

    #[test]
    fn an_edition_with_no_credits_emits_no_credits_key() {
        // An empty list and an absent one mean the same thing, so a sidecar that
        // has never met this feature must round-trip without growing a field.
        // `spreads` always emits because every alignment computes one; `credits`
        // exists only once somebody runs the command.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "a.cbz", "sha256": "aa", "pages": 4 }],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": []
        }"#;
        let emitted = Sidecar::from_json(text).expect("parses").to_json();
        assert!(
            !emitted.contains("credits"),
            "an edition with no credits must not emit the key:\n{emitted}"
        );
    }

    #[test]
    fn the_real_sidecar_round_trips() {
        let parsed = Sidecar::from_json(REAL).expect("parses");

        assert_eq!(parsed.work.reading, Direction::RightToLeft);
        assert_eq!(parsed.hub, "raw-mono");
        assert_eq!(parsed.editions["en-mono"].spreads.len(), 7);
        let runs = &parsed.alignments[0].runs;
        assert_eq!(runs.len(), 5);
        // The gap keeps its shape rather than becoming a pairing.
        assert_eq!(runs[1].b, None);
        // The spread half survives as a half, not as a bare page.
        assert_eq!(
            runs[2].b.unwrap().end,
            Slot {
                page: 4,
                half: Some(Half::Right)
            }
        );

        let again = Sidecar::from_json(&parsed.to_json()).expect("re-parses");
        assert_eq!(again, parsed, "round trip changed the document");
    }

    /// An L0 reader has to tolerate fields it does not understand, or shipping
    /// L1 breaks every reader already in the wild.
    #[test]
    fn unknown_fields_are_ignored_rather_than_fatal() {
        let with_l1 = REAL.replace(
            r#"{ "a": "0", "b": "0", "margin": 0.09 }"#,
            r#"{ "a": "0", "b": "0", "margin": 0.09, "transform": [1,0,0,1,0,0], "future": {"x": 1} }"#,
        );

        let parsed = Sidecar::from_json(&with_l1).expect("L1 fields must not break L0");

        assert_eq!(parsed.alignments[0].runs.len(), 5);
    }

    /// Runs come out of the solver indexed by slot; the file records pages.
    /// This is the translation, and it is shaped after the real Go-Toubun
    /// alignment: page 4 of edition B is a joined spread, so slot 4 is its
    /// right half and every later slot sits one ahead of its page number.
    #[test]
    fn runs_become_page_notation_entries() {
        use crate::{PageRange, Run};

        let a_slots = crate::expand(&[false; 8], Direction::RightToLeft);
        // B page 4 is a spread, so B has nine slots for eight pages.
        let mut wide = [false; 8];
        wide[4] = true;
        let b_slots = crate::expand(&wide, Direction::RightToLeft);

        let runs = vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            // A slots 2..5 land on B slots 1..4, and B slot 4 is 4R.
            Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
        ];

        let entries = entries_from_runs(&runs, &a_slots, &b_slots).expect("converts");

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].a.to_string(), "0");
        assert_eq!(entries[0].b.map(|s| s.to_string()), Some("0".into()));
        assert_eq!(entries[1].a.to_string(), "1");
        assert_eq!(entries[1].b, None, "a gap stays a gap");
        assert_eq!(entries[2].a.to_string(), "2-5");
        assert_eq!(
            entries[2].b.map(|s| s.to_string()),
            Some("1-4R".into()),
            "the spread half must survive as a half"
        );
    }

    /// A run pointing outside its slot table means the tables and the runs came
    /// from different solves. Writing that file would produce valid JSON
    /// describing an alignment that never existed.
    #[test]
    fn a_run_beyond_the_slot_table_is_rejected() {
        use crate::{PageRange, Run};

        let slots = crate::expand(&[false; 3], Direction::RightToLeft);
        let runs = vec![Run::paired(PageRange::new(0, 9), PageRange::new(0, 9))];

        assert!(entries_from_runs(&runs, &slots, &slots).is_err());
    }

    /// A malformed span must be reported, not guessed at. Silently reading
    /// `4X` as page 4 would move a reader to the wrong half of a spread.
    #[test]
    fn a_malformed_span_is_rejected() {
        for bad in ["", "-", "4X", "abc", "4-", "-4", "4R-", "1-2-3"] {
            assert!(SlotSpan::parse(bad).is_err(), "should reject {bad:?}");
        }
    }

    /// The block a reader needs in order to tell a worthless alignment from a
    /// good one. Measured: seven short unrelated pairings pair every page they
    /// have and are indistinguishable from a correct alignment on both
    /// calibration statistics, so the run shape is the only thing that
    /// separates them and the document has to carry it.
    #[test]
    fn an_alignment_carries_its_quality_through_a_round_trip() {
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 4 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 4 }] }
          },
          "hub": "a",
          "alignments": [{
            "from": "a", "to": "b", "by": "solver",
            "quality": { "verdict": "sound", "slots": 4, "paired": 4, "avg_run": 4.0 },
            "runs": [{ "a": "0-3", "b": "0-3" }]
          }]
        }"#;

        let sidecar = Sidecar::from_json(json).expect("parses");
        let quality = sidecar.alignments[0].quality.expect("quality is present");
        assert_eq!(quality.verdict, Verdict::Sound);
        assert_eq!(quality.slots, 4);
        assert_eq!(quality.paired, 4);

        let round_tripped = Sidecar::from_json(&sidecar.to_json()).expect("re-parses");
        assert_eq!(
            round_tripped.alignments[0].quality,
            sidecar.alignments[0].quality
        );
    }

    /// Every sidecar written before this field existed lacks it, and those
    /// files must keep parsing. Absent means unknown, which Task 3's `trust`
    /// distinguishes from sound.
    #[test]
    fn a_document_without_quality_still_parses_and_omits_the_field() {
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 2 }] }
          },
          "hub": "a",
          "alignments": [{ "from": "a", "to": "a", "by": "hand", "runs": [{ "a": "0-1", "b": "0-1" }] }]
        }"#;

        let sidecar = Sidecar::from_json(json).expect("parses");

        assert_eq!(sidecar.alignments[0].quality, None);
        // And it must not reappear as a null on the way out.
        assert!(!sidecar.to_json().contains("quality"));
    }

    /// The test the design turns on. A document whose stored block claims
    /// `sound` over runs that are confetti must be judged on the runs. The
    /// stored block is a summary for readers that have only the document; it
    /// is never what this library believes, because a hand edit to the runs
    /// would otherwise leave a stale verdict standing beside them.
    #[test]
    fn a_stored_verdict_is_ignored_in_favour_of_the_runs_it_sits_beside() {
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 30 }] }
          },
          "hub": "a",
          "alignments": [{
            "from": "a", "to": "b", "by": "hand",
            "quality": { "verdict": "sound", "slots": 30, "paired": 30, "avg_run": 30.0 },
            "runs": [
              { "a": "0-1", "b": "0-1" }, { "a": "2-3", "b": "4-5" },
              { "a": "4-5", "b": "8-9" }, { "a": "6-7", "b": "12-13" },
              { "a": "8-9", "b": "16-17" }, { "a": "10-11", "b": "20-21" },
              { "a": "12-13", "b": "24-25" }, { "a": "14-15", "b": "28-29" }
            ]
          }]
        }"#;
        let sidecar = Sidecar::from_json(json).expect("parses");

        // Eight paired runs of two slots each: sixteen paired, averaging 2.0.
        let recomputed = sidecar.quality_of("a", "b").expect("both editions exist");
        assert_eq!(recomputed.paired, 16);
        assert!((recomputed.avg_run - 2.0).abs() < 1e-6, "{recomputed:?}");
        assert_eq!(recomputed.verdict, Verdict::Fragmented);
        assert_eq!(sidecar.trust("a", "b"), Some(Verdict::Fragmented));
    }

    /// A lookup from one spoke to another resolves through the hub, so it is
    /// only as trustworthy as the weaker of the two edges it crosses.
    #[test]
    fn trust_composed_through_the_hub_takes_the_worse_edge() {
        // hub `a`; `a -> b` is one clean run, `a -> c` is confetti.
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 30 }] },
            "c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] }
          },
          "hub": "a",
          "alignments": [
            { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-29", "b": "0-29" }] },
            { "from": "a", "to": "c", "by": "solver", "runs": [
              { "a": "0-1", "b": "0-1" }, { "a": "2-3", "b": "4-5" },
              { "a": "4-5", "b": "8-9" }, { "a": "6-7", "b": "12-13" },
              { "a": "8-9", "b": "16-17" }, { "a": "10-11", "b": "20-21" },
              { "a": "12-13", "b": "24-25" }, { "a": "14-15", "b": "28-29" }
            ] }
          ]
        }"#;
        let sidecar = Sidecar::from_json(json).expect("parses");

        assert_eq!(sidecar.trust("a", "b"), Some(Verdict::Sound));
        assert_eq!(sidecar.trust("a", "c"), Some(Verdict::Fragmented));
        // b -> c crosses both, and the confetti edge decides it.
        assert_eq!(sidecar.trust("b", "c"), Some(Verdict::Fragmented));
        // Both orderings, or an implementation that returned only the second
        // leg would pass on this alone.
        assert_eq!(sidecar.trust("c", "b"), Some(Verdict::Fragmented));
        // An edge is readable in either direction for trust, because the
        // verdict is a property of the pairing rather than of a direction.
        assert_eq!(sidecar.trust("b", "a"), Some(Verdict::Sound));
    }

    /// The cases where the document simply does not say. `None` is not
    /// `Sound` — every sidecar written before this field existed is unknown,
    /// and a reader must not read silence as approval.
    #[test]
    fn trust_says_nothing_when_the_document_does_not() {
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 4 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 4 }] },
            "c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 4 }] },
            "lonely": { "label": "L", "members": [{ "path": "l.cbz", "sha256": "03", "pages": 4 }] }
          },
          "hub": "a",
          "alignments": [
            { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] },
            { "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] },
            { "from": "b", "to": "c", "by": "hand", "runs": [{ "a": "0-3", "b": "0-9" }] }
          ]
        }"#;
        let sidecar = Sidecar::from_json(json).expect("parses");

        assert_eq!(sidecar.trust("a", "nosuch"), None);
        assert_eq!(sidecar.trust("nosuch", "b"), None);
        // Neither edition exists; the guard has to reject this before
        // identity short-circuits it to `Sound`.
        assert_eq!(sidecar.trust("nosuch", "nosuch"), None);
        // Defined, but nothing aligns it to anything.
        assert_eq!(sidecar.trust("b", "lonely"), None);
        // An edition corresponds to itself.
        assert_eq!(sidecar.trust("b", "b"), Some(Verdict::Sound));
        // `b -> c` is a direct edge that names a page `c` does not have.
        // Both hub legs (`a -> b`, `a -> c`) are sound, so a version that
        // fell through to hub composition on any `None` from `quality_of`
        // would answer `Some(Sound)` here — about a path the caller did not
        // ask about, in place of the broken edge that was named.
        assert_eq!(sidecar.quality_of("b", "c"), None);
        assert_eq!(sidecar.trust("b", "c"), None);
    }

    /// A span written backwards must be reported, not turned into a panic by
    /// `PageRange::new`'s `debug_assert`. Reproduced from review: `"9-2"`
    /// panics with "page range must not be inverted" in a debug build, which
    /// is what the test suite and debug wasm both are.
    #[test]
    fn an_inverted_span_is_rejected_rather_than_panicking() {
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 10 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 10 }] }
          },
          "hub": "a",
          "alignments": [{
            "from": "a", "to": "b", "by": "hand",
            "runs": [{ "a": "9-2", "b": "0-7" }]
          }]
        }"#;
        let sidecar = Sidecar::from_json(json).expect("parses");

        let err = sidecar
            .runs_of(&sidecar.alignments[0])
            .expect_err("must not panic");
        assert!(err.contains("9-2"), "error should name the span: {err}");
        assert_eq!(sidecar.quality_of("a", "b"), None);
    }

    /// A paired run whose sides cover different numbers of slots must be
    /// reported, not turned into a panic by `Run::paired`'s `debug_assert_eq`.
    /// Reproduced from review: `"a": "0-1", "b": "0-5"` panics with "paired
    /// run sides must be equal length" in a debug build.
    #[test]
    fn a_mismatched_paired_span_is_rejected_rather_than_panicking() {
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 10 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 10 }] }
          },
          "hub": "a",
          "alignments": [{
            "from": "a", "to": "b", "by": "hand",
            "runs": [{ "a": "0-1", "b": "0-5" }]
          }]
        }"#;
        let sidecar = Sidecar::from_json(json).expect("parses");

        let err = sidecar
            .runs_of(&sidecar.alignments[0])
            .expect_err("must not panic");
        assert!(
            err.contains("0-1") && err.contains("0-5"),
            "error should name both spans: {err}"
        );
        assert_eq!(sidecar.quality_of("a", "b"), None);
    }

    fn two_member_edition() -> Edition {
        Edition {
            label: "Raw".into(),
            members: vec![
                Member {
                    path: "c1.cbz".into(),
                    sha256: "x".into(),
                    pages: 2,
                },
                Member {
                    path: "c2.cbz".into(),
                    sha256: "y".into(),
                    pages: 3,
                },
            ],
            spreads: vec![],
            credits: Vec::new(),
        }
    }

    /// An edition is its members concatenated in array order — `sidecar.md`
    /// rule 1. Getting this wrong shows the wrong page for everything after
    /// the first member while every hash and count in the file still matches,
    /// which is why the arithmetic belongs in one place rather than in each
    /// consumer.
    #[test]
    fn a_page_index_locates_the_member_that_holds_it() {
        let edition = two_member_edition();

        assert_eq!(edition.total_pages(), 5);
        assert_eq!(edition.locate(0), Some((0, 0)));
        assert_eq!(edition.locate(1), Some((0, 1)));
        assert_eq!(edition.locate(2), Some((1, 0)), "the boundary");
        assert_eq!(edition.locate(4), Some((1, 2)), "the last page");
        assert_eq!(edition.locate(5), None, "past the end must not wrap");
        assert_eq!(edition.locate(u32::MAX), None);
    }

    /// An edition with no members has no pages and locates nothing, rather
    /// than panicking or reporting member 0.
    #[test]
    fn an_edition_with_no_members_locates_nothing() {
        let empty = Edition {
            label: "E".into(),
            members: vec![],
            spreads: vec![],
            credits: Vec::new(),
        };

        assert_eq!(empty.total_pages(), 0);
        assert_eq!(empty.locate(0), None);
    }

    /// A member recorded with zero pages is skipped rather than swallowing an
    /// index. `align` cannot currently write one, but a hand-edited sidecar
    /// can, and returning `(1, 0)` here would serve the wrong member's first
    /// page for the rest of the edition.
    #[test]
    fn a_zero_page_member_is_skipped_rather_than_matched() {
        let edition = Edition {
            label: "E".into(),
            members: vec![
                Member {
                    path: "a.cbz".into(),
                    sha256: "x".into(),
                    pages: 0,
                },
                Member {
                    path: "b.cbz".into(),
                    sha256: "y".into(),
                    pages: 2,
                },
            ],
            spreads: vec![],
            credits: Vec::new(),
        };

        assert_eq!(edition.total_pages(), 2);
        assert_eq!(
            edition.locate(0),
            Some((1, 0)),
            "must skip the empty member"
        );
        assert_eq!(edition.locate(1), Some((1, 1)));
        assert_eq!(edition.locate(2), None);
    }

    /// The ordinary case: a slot of the single-member edition resolves into
    /// the chapter-organised one and lands in the member the alignment says
    /// it does, not the one page arithmetic on the filename would suggest.
    #[test]
    fn chapter_at_names_the_member_a_slot_lands_in() {
        let sc = Sidecar::from_json(CHAPTERED).unwrap();
        let at = |page| sc.chapter_at("volume", Slot { page, half: None }).unwrap();
        assert_eq!((at(0).member, at(0).of), (0, 3));
        assert_eq!(at(0).path, "c001.cbz");
        assert_eq!(at(2).member, 1);
        assert_eq!(at(5).member, 2);
        assert_eq!(at(5).path, "c003.cbz");
    }

    /// `None` is the common case: most works never have a chapter-organised
    /// edition at all, and that must not be an error.
    #[test]
    fn chapter_at_is_none_where_no_edition_is_chapter_organised() {
        // Both editions are one member each, so nothing here is a chapter list.
        let sc = Sidecar::from_json(FIXTURE_SINGLE_MEMBER_PAIR).unwrap();
        assert_eq!(
            sc.chapter_at(
                "a",
                Slot {
                    page: 0,
                    half: None
                }
            ),
            None
        );
    }

    /// A slot that resolves into the chaptered edition's alignment but lands
    /// in a gap has no member to report, and must not be confused with there
    /// being no chapter-organised edition at all.
    #[test]
    fn chapter_at_is_none_for_a_slot_with_no_counterpart() {
        let sc = Sidecar::from_json(CHAPTERED_WITH_A_GAP).unwrap();
        assert_eq!(
            sc.chapter_at(
                "volume",
                Slot {
                    page: 3,
                    half: None
                }
            ),
            None
        );
    }

    /// A sibling of the test above, against the same fixture: it asserts a
    /// non-gap slot still resolves to `Some`, so the two tests together can
    /// only pass if the gap is the actual cause of that `None`. Without this,
    /// `route()` failing outright for the `volume` -> `chapters` pair — which
    /// `runs_of` will do given a run whose two sides disagree in slot count —
    /// would make every slot `None`, and the test above would still pass for
    /// the wrong reason.
    #[test]
    fn chapter_at_still_resolves_a_neighbouring_slot_in_the_gap_fixture() {
        let sc = Sidecar::from_json(CHAPTERED_WITH_A_GAP).unwrap();
        assert!(
            sc.chapter_at(
                "volume",
                Slot {
                    page: 0,
                    half: None
                }
            )
            .is_some()
        );
    }

    /// Asking from the chapter-organised edition itself needs no alignment —
    /// `key == from` is identity, not a lookup that happens to succeed.
    #[test]
    fn chapter_at_from_the_chaptered_edition_itself_needs_no_alignment() {
        let sc = Sidecar::from_json(CHAPTERED).unwrap();
        let at = sc
            .chapter_at(
                "chapters",
                Slot {
                    page: 4,
                    half: None,
                },
            )
            .unwrap();
        assert_eq!(at.member, 2);
    }

    const STAR_FOR_CORRECTION: &str = r#"{
      "taiyaku": 1,
      "work": { "title": "T", "reading": "rtl" },
      "editions": {
        "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
        "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 30 }] },
        "c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] }
      },
      "hub": "a",
      "alignments": [
        { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-29", "b": "0-29" }] },
        { "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] }
      ]
    }"#;

    fn at(page: u32) -> Slot {
        Slot { page, half: None }
    }

    /// The ordinary case, on the real document: the edge is stored
    /// `raw-mono -> en-mono` and the correction is asked for in that
    /// direction. Slot 50 of the raw currently resolves to English page 49;
    /// asserting page 40 instead must land, and must displace whoever held it.
    #[test]
    fn correcting_a_stored_edge_rewrites_it_in_place() {
        let mut s = Sidecar::from_json(REAL).expect("parses");

        let what = s
            .correct("raw-mono", "en-mono", at(50), Some(at(40)), None)
            .expect("corrects");

        assert_eq!(what, Corrected::Edge);
        assert_eq!(s.alignments.len(), 1, "no new edge for a pair that has one");
        assert_eq!(
            crate::resolve(&s, "raw-mono", "en-mono", 50),
            Some(at(40)),
            "the assertion is what the document now says"
        );
        let hands: Vec<String> = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .map(|e| e.a.to_string())
            .collect();
        assert_eq!(
            hands,
            vec!["42".to_string(), "50".to_string()],
            "exactly the slot asserted and the slot displaced — not the run they sat in"
        );
    }

    /// The edge is stored one way and the reader is looking the other way.
    /// Correcting through `invert` and back would silently drop every gap the
    /// stored direction records, because `invert` drops gaps by construction.
    #[test]
    fn correcting_against_the_stored_direction_keeps_the_stored_gaps() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        let gaps_before = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.b.is_none())
            .count();

        let what = s
            .correct("en-mono", "raw-mono", at(10), Some(at(20)), None)
            .expect("corrects");

        assert_eq!(what, Corrected::Edge);
        assert_eq!(s.alignments.len(), 1);
        assert_eq!(
            s.alignments[0].from, "raw-mono",
            "the edge keeps its orientation"
        );
        assert_eq!(
            s.alignments[0]
                .runs
                .iter()
                .filter(|e| e.b.is_none())
                .count(),
            gaps_before + 1,
            "the stored gaps survive, plus one for the displaced slot"
        );
        assert_eq!(crate::resolve(&s, "en-mono", "raw-mono", 10), Some(at(20)));
    }

    /// A pair with no stored edge resolves through the hub, so there is
    /// nothing to edit. The correction writes a direct edge seeded from the
    /// composition with the fix applied — which `sidecar.md` already sanctions
    /// for a pair where composition demonstrably loses, and a hand correction
    /// is that demonstration.
    #[test]
    fn correcting_a_composed_pair_writes_a_direct_edge() {
        let mut s = Sidecar::from_json(STAR_FOR_CORRECTION).expect("parses");
        assert_eq!(
            route(&s, "b", "c").map(|r| r.via),
            Some(Via::Hub("a".into()))
        );

        let what = s
            .correct("b", "c", at(0), Some(at(0)), None)
            .expect("corrects");

        assert_eq!(what, Corrected::DirectEdgeAdded);
        assert_eq!(s.alignments.len(), 3);
        let added = s.alignments.last().expect("just pushed");
        assert_eq!((added.from.as_str(), added.to.as_str()), ("b", "c"));
        assert_eq!(added.by, Source::Hand);
        assert_eq!(added.solver, None, "no floor was calibrated for this edge");
        assert_eq!(route(&s, "b", "c").map(|r| r.via), Some(Via::Direct));
        assert_eq!(
            crate::resolve(&s, "b", "c", 0),
            Some(at(0)),
            "the composed gap at the head is what the reader was correcting"
        );
    }

    /// A correction the document already agrees with must change nothing at
    /// all — not the runs, not `by`, not `note`. Without this, "correcting" a
    /// slot to where it already resolves stamps `hand` across the whole run it
    /// sits in, which on the real fixture is 188 slots.
    #[test]
    fn a_correction_that_changes_nothing_rewrites_nothing() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        let before = s.clone();

        let what = s
            .correct(
                "raw-mono",
                "en-mono",
                at(100),
                Some(at(94)),
                Some("noted".into()),
            )
            .expect("is accepted");

        assert_eq!(what, Corrected::AlreadyTrue);
        assert_eq!(s, before, "an already-true assertion is not an edit");
    }

    /// The quality block is recomputed from the runs that are now there, not
    /// carried over. Measured on this fixture: two corrections take the
    /// average run length from 64.33 to 21.22, past the 27.0 threshold — see
    /// O23 in `docs/status.md`, which records that rather than hiding it.
    #[test]
    fn correcting_recomputes_the_quality_beside_the_runs() {
        let mut s = Sidecar::from_json(REAL).expect("parses");

        s.correct("raw-mono", "en-mono", at(50), Some(at(40)), None)
            .unwrap();
        let after_one = s.alignments[0].quality.expect("recomputed");
        s.correct("raw-mono", "en-mono", at(150), Some(at(140)), None)
            .unwrap();
        let after_two = s.alignments[0].quality.expect("recomputed");

        assert_eq!(after_one.verdict, Verdict::Sound);
        assert!((after_one.avg_run - 32.0).abs() < 0.01, "{after_one:?}");
        assert_eq!(
            after_two.verdict,
            Verdict::Fragmented,
            "two corrections is where this volume crosses the threshold"
        );
        assert!((after_two.avg_run - 21.22).abs() < 0.01, "{after_two:?}");
    }

    /// A correction must not strip the confidence figures off the runs it did
    /// not touch.
    ///
    /// `entries_from_runs` writes `margin: None` for every entry it builds, so
    /// before this was guarded a single correction silently cleared the margins
    /// across the whole edge. Found by driving the reader against the real
    /// corpus, not by any test: one correction took `raw-mono -> en-fan` from
    /// eight margins to zero while the untouched `raw-mono -> en-mono` edge
    /// kept all of its. A margin is recoverable only by re-solving against the
    /// archives, which is the work `docs/sidecar.md` says the document exists
    /// to avoid.
    ///
    /// `REAL`'s stored edge carries a margin on three of its five runs, and the
    /// correction below rewrites only the run holding slot 50 — so the runs at
    /// either end must come through with their figures intact, and the ones the
    /// correction actually reshaped must come through as `None` rather than
    /// inheriting a figure measured over a different span.
    #[test]
    fn a_correction_keeps_the_margins_of_the_runs_it_did_not_touch() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        let before: Vec<Option<f32>> = s.alignments[0].runs.iter().map(|r| r.margin).collect();
        assert_eq!(
            before.iter().filter(|m| m.is_some()).count(),
            3,
            "the fixture must actually carry margins, or this test asserts nothing"
        );

        s.correct("raw-mono", "en-mono", at(50), Some(at(40)), None)
            .expect("corrects");

        let kept: Vec<(String, Option<f32>)> = s.alignments[0]
            .runs
            .iter()
            .map(|r| (r.a.to_string(), r.margin))
            .collect();
        // The first run is untouched by a correction at slot 50 and keeps the
        // figure `REAL` records for it.
        assert_eq!(
            kept.iter().find(|(a, _)| a == "0").map(|(_, m)| *m),
            Some(Some(0.09)),
            "an untouched run keeps its margin: {kept:?}"
        );
        assert_eq!(
            kept.iter().find(|(a, _)| a == "2-5").map(|(_, m)| *m),
            Some(Some(0.15)),
            "and so does this one: {kept:?}"
        );
        // The long run that held slot 50 was split by the correction, so no
        // stored figure describes any of its pieces any more.
        assert!(
            kept.iter()
                .filter(|(a, _)| a != "0" && a != "2-5")
                .all(|(_, m)| m.is_none()),
            "a reshaped run must not inherit a margin measured over a different span: {kept:?}"
        );
    }

    /// A hand correction has to survive the file being written and read back,
    /// or nothing else in this milestone means anything.
    #[test]
    fn a_correction_round_trips_through_the_document() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        s.correct(
            "raw-mono",
            "en-mono",
            at(50),
            Some(at(40)),
            Some("checked".into()),
        )
        .unwrap();

        let reparsed = Sidecar::from_json(&s.to_json()).expect("round-trips");

        assert_eq!(reparsed, s);
        let noted = reparsed.alignments[0]
            .runs
            .iter()
            .find(|e| e.note.is_some())
            .expect("the note survives");
        assert_eq!(noted.note.as_deref(), Some("checked"));
        assert_eq!(noted.by, Some(Source::Hand));
    }

    /// A second correction must not rewrite the first one's explanation.
    /// `note` is why *that* slot was changed, and the run list is rebuilt
    /// wholesale on every correction, so carrying it is deliberate work
    /// rather than something that happens by itself.
    #[test]
    fn a_later_correction_leaves_an_earlier_ones_note_alone() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        s.correct(
            "raw-mono",
            "en-mono",
            at(50),
            Some(at(40)),
            Some("first".into()),
        )
        .unwrap();

        s.correct(
            "raw-mono",
            "en-mono",
            at(150),
            Some(at(140)),
            Some("second".into()),
        )
        .unwrap();

        let notes: Vec<Option<&str>> = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .map(|e| e.note.as_deref())
            .collect();
        assert!(notes.contains(&Some("first")), "{notes:?}");
        assert!(notes.contains(&Some("second")), "{notes:?}");
    }

    /// Each refusal names what was missing, and none of them is a panic.
    #[test]
    fn a_correction_naming_something_absent_is_refused_by_name() {
        let mut s = Sidecar::from_json(REAL).expect("parses");

        let no_edition = s.correct("raw-mono", "nosuch", at(0), Some(at(0)), None);
        let no_slot = s.correct("raw-mono", "en-mono", at(9999), Some(at(0)), None);
        let itself = s.correct("raw-mono", "raw-mono", at(0), Some(at(0)), None);

        assert!(
            matches!(no_edition, Err(NotCorrected::NoSuchEdition(ref m)) if m.contains("`nosuch`")),
            "{no_edition:?}"
        );
        assert!(
            matches!(no_slot, Err(NotCorrected::NoSuchSlot(ref m)) if m.contains("9999")),
            "{no_slot:?}"
        );
        assert!(
            matches!(itself, Err(NotCorrected::SameEdition(_))),
            "{itself:?}"
        );
    }

    /// `correcting_a_stored_edge_rewrites_it_in_place` cannot pin `all` versus
    /// `any` on its own: that correction only ever *splits* the run it lands
    /// in, so the displaced and asserted slots never share a run with an
    /// untouched slot for `any` to over-mark. This one *merges* instead. En
    /// slot 4 (page 4's right half) is currently claimed by raw slot 5;
    /// reassigning it to raw slot 100 displaces slot 5, which sits right next
    /// to the pre-existing gap at slot 6, and `compress` folds the two into
    /// one run, `"5-6"`. Only slot 5 was decided by this correction — slot 6
    /// was already a gap before it — so `all` must leave `"5-6"` unmarked.
    /// `any` would mark it hand because slot 5 alone satisfies it, and on a
    /// run a correction merges into rather than splits, that generalises to
    /// the whole run: measured at up to 188 of this fixture's 195 slots for a
    /// differently shaped correction. Task 3's re-solve refusal keys off
    /// which runs are hand-marked, so an over-broad mark here would make it
    /// refuse forever.
    #[test]
    fn a_hand_mark_covers_only_the_slots_a_person_decided() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        let target = Slot {
            page: 4,
            half: Some(Half::Right),
        };

        s.correct("raw-mono", "en-mono", at(100), Some(target), None)
            .expect("corrects");

        assert!(
            s.alignments[0]
                .runs
                .iter()
                .any(|e| e.a.to_string() == "5-6"),
            "the displaced slot must have merged with the existing gap, or this test proves nothing: {:?}",
            s.alignments[0].runs
        );
        let hands: Vec<String> = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .map(|e| e.a.to_string())
            .collect();
        assert_eq!(
            hands,
            vec!["100".to_string()],
            "slot 6 was already a gap; merging into it must not mark it hand too"
        );
    }

    /// Two corrections whose changed slots land index-adjacent merge into one
    /// run — `compress` groups consecutive gaps by adjacency alone, with no
    /// awareness of `by` or `note`. The old note-recovery looked up the
    /// previous note by exact span equality, so once the merged run's span
    /// (`"60-61"`) matched neither correction's original single-slot span
    /// (`"60"`, `"61"`), the lookup missed and the *first* correction's note
    /// vanished — even though the run stayed correctly `by: Hand`. Recovery
    /// by overlap, joining rather than choosing, is what keeps both.
    #[test]
    fn two_merged_corrections_keep_both_notes() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        s.correct("raw-mono", "en-mono", at(60), None, Some("first".into()))
            .expect("corrects");
        s.correct("raw-mono", "en-mono", at(61), None, Some("second".into()))
            .expect("corrects");

        // Guard: the fixture must actually merge the two corrections into one
        // run, or the rest of this test proves nothing about recovery across
        // a merge.
        let merged = s.alignments[0]
            .runs
            .iter()
            .find(|e| e.a.to_string() == "60-61");
        let merged = merged.expect("the two adjacent gaps must have merged into one run");

        assert_eq!(merged.by, Some(Source::Hand));
        assert_eq!(merged.note.as_deref(), Some("first; second"));
    }
}
