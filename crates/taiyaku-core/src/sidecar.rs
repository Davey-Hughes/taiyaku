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
fn slot_index(slots: &[Slot], slot: Slot) -> Option<usize> {
    slots
        .iter()
        .position(|s| s.page == slot.page && (slot.half.is_none() || s.half == slot.half))
}

/// Resolve a page of one edition to its counterpart in another, from a stored
/// sidecar alone.
///
/// This is the whole reader API: given a document and a page, say what to show.
/// It works in slot indices internally because that is where the arithmetic is
/// uniform, and returns a page plus an optional half because that is what a
/// reader has to display — possibly one side of a joined spread.
pub fn resolve(sidecar: &Sidecar, from: &str, to: &str, page: u32) -> Option<Slot> {
    let alignment = sidecar
        .alignments
        .iter()
        .find(|a| a.from == from && a.to == to)?;
    let from_slots = sidecar.editions.get(from)?.slots(sidecar.work.reading);
    let to_slots = sidecar.editions.get(to)?.slots(sidecar.work.reading);

    let here = slot_index(&from_slots, Slot { page, half: None })? as u32;

    for entry in &alignment.runs {
        let start = slot_index(&from_slots, entry.a.start)? as u32;
        let end = slot_index(&from_slots, entry.a.end)? as u32;
        if here < start || here > end {
            continue;
        }
        let target = entry.b?;
        let target_start = slot_index(&to_slots, target.start)? as u32;
        return to_slots
            .get((target_start + (here - start)) as usize)
            .copied();
    }
    None
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
    /// The slot sequence this edition presents to an alignment.
    ///
    /// Reconstructed from the page count and the spread list rather than
    /// stored, because storing it would be a second source of truth that could
    /// disagree with the first.
    pub fn slots(&self, reading: Direction) -> Vec<Slot> {
        let pages: u32 = self.members.iter().map(|m| m.pages).sum();
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

impl Sidecar {
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("not a readable taiyaku.json: {e}"))
    }

    pub fn to_json(&self) -> String {
        // Pretty-printed on purpose: the format is meant to diff in git, and a
        // one-line document defeats that entirely.
        serde_json::to_string_pretty(self).expect("sidecar is always serialisable")
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
    fn edge_between(&self, from: &str, to: &str) -> Option<&Alignment> {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The document from `docs/sidecar.md`, verbatim: the real alignment of
    /// Go-Toubun volume 1. Parsing it, writing it back, and parsing that again
    /// has to give the same thing, or a hand-edited file cannot survive a tool
    /// that rewrites it.
    const REAL: &str = r#"{
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

    /// The reader's whole job, done from the stored document with no archives
    /// and no solving. These expectations come from the aligner's own output on
    /// Go-Toubun volume 1, so agreement means the file carries what was solved.
    #[test]
    fn a_page_resolves_through_a_stored_sidecar() {
        let s = Sidecar::from_json(REAL).expect("parses");

        // Ordinary page inside the long final run. Raw 100 lands on English 94,
        // not 99: five of the seven spreads sit before it, and each absorbs a
        // page. Working that out by eye gets it wrong, which is the point.
        assert_eq!(
            resolve(&s, "raw-mono", "en-mono", 100),
            Some(Slot {
                page: 94,
                half: None
            })
        );
        // Raw page 47 lands on the right half of the spread at English page 45.
        // Getting the slot arithmetic wrong here shows the wrong half.
        assert_eq!(
            resolve(&s, "raw-mono", "en-mono", 47),
            Some(Slot {
                page: 45,
                half: Some(Half::Right)
            })
        );
        assert_eq!(
            resolve(&s, "raw-mono", "en-mono", 48),
            Some(Slot {
                page: 45,
                half: Some(Half::Left)
            })
        );
        // Page 1 is the blank the solver refused to pair.
        assert_eq!(resolve(&s, "raw-mono", "en-mono", 1), None);
        // Beyond the volume.
        assert_eq!(resolve(&s, "raw-mono", "en-mono", 9999), None);
        // An edition pair with no alignment recorded.
        assert_eq!(resolve(&s, "en-mono", "raw-mono", 0), None);
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
}
