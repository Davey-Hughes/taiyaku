//! Reading and writing `taiyaku.json`.
//!
//! The format is specified in `docs/sidecar.md`. It lives in the core so a
//! browser reader gets it without reimplementing anything.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Direction, Half, Slot};

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

    // A whole page occupies one slot; a spread occupies two, and the first of
    // them is the one a page reference means.
    let index = |slots: &[Slot], slot: Slot| {
        slots
            .iter()
            .position(|s| s.page == slot.page && (slot.half.is_none() || s.half == slot.half))
    };
    let here = index(&from_slots, Slot { page, half: None })? as u32;

    for entry in &alignment.runs {
        let start = index(&from_slots, entry.a.start)? as u32;
        let end = index(&from_slots, entry.a.end)? as u32;
        if here < start || here > end {
            continue;
        }
        let target = entry.b?;
        let target_start = index(&to_slots, target.start)? as u32;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alignment {
    pub from: String,
    pub to: String,
    pub by: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver: Option<SolverInfo>,
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
}
