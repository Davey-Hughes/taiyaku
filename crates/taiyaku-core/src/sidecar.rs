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

        let replaced = self.editions.insert(name.to_string(), edition).is_some();
        // Edition and edge are replaced together. A surviving edge would
        // address pages of an edition that is no longer there.
        self.alignments
            .retain(|a| !(a.from == start && a.to == name) && !(a.from == name && a.to == start));
        self.alignments.push(alignment);
        Ok(match replaced {
            true => Merged::Replaced,
            false => Merged::Added,
        })
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

    /// Re-solving a spoke after fixing its archive is ordinary. Refusing it
    /// would make the common case the annoying one, so it replaces — edition
    /// and edge together, so neither can outlive the other.
    #[test]
    fn re_aligning_a_spoke_replaces_it_rather_than_duplicating() {
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
}
