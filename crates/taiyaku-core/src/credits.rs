//! Which slots of an edition the alignment never mentions, and where they sit.
//!
//! This is signal 1 of credit-page detection, and the only one that classifies.
//! It is pure — no I/O, no image decoding — so it lives here beside `route` and
//! `slot` rather than in the CLI, and it reports geometry rather than applying
//! a threshold: the command decides what counts.
//!
//! **Runs are split at member boundaries.** Measured on `corpus/gotoubun-v01`'s
//! fan edition, an unsplit run spanning `c005`'s last page and `c005.5`'s first
//! four begins and ends mid-member, so a rule reading position off its endpoints
//! sees no boundary at all — and loses one credits page and four front-matter
//! pages in the same stroke. See `docs/notes.md`, "Corrected 2026-08-05".

use std::collections::BTreeSet;
use std::ops::Range;

use crate::Sidecar;

/// A stretch of one member's pages that the alignment never mentions.
///
/// Both index spaces are carried rather than one, because the `credits` field
/// is in pages and a reader's position is in slots, and a call site converting
/// for itself is how the two silently diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpairedRun {
    /// Half-open, in the edition's flat slot sequence.
    pub slots: Range<u32>,
    /// Half-open, in the edition's flat page sequence.
    pub pages: Range<u32>,
    /// Index into the edition's `members`. One member, always: runs are split
    /// at boundaries so this cannot span two.
    pub member: usize,
    /// Half-open, local to that member.
    pub local: Range<u32>,
    /// Does this run end on the member's last page?
    ///
    /// The only positional test that classifies. Measured four for four on
    /// `corpus/gotoubun-v01`'s fan edition, where the first-index version is
    /// zero for four — what accumulates at a member's start is front matter.
    pub at_member_end: bool,
}

/// Which pages of `edition` the alignment against `against` never reaches.
///
/// A page counts as unpaired only when **every** slot it presents is unpaired.
/// A joined spread whose halves are half-reached is content the other edition
/// partly has, and no credits card is half of a spread.
pub fn unpaired_runs(
    sidecar: &Sidecar,
    edition: &str,
    against: &str,
) -> Result<Vec<UnpairedRun>, String> {
    let target = sidecar
        .editions
        .get(edition)
        .ok_or_else(|| format!("this work has no edition `{edition}`"))?;

    // Which side of the stored edge is the edition being examined? The
    // alignment is directed and only one side's spans address this edition's
    // slots, so taking the wrong one silently reports another edition's gaps.
    let (alignment, mine_is_a) = sidecar
        .alignments
        .iter()
        .find_map(|al| match (al.from.as_str(), al.to.as_str()) {
            (f, t) if f == edition && t == against => Some((al, true)),
            (f, t) if f == against && t == edition => Some((al, false)),
            _ => None,
        })
        .ok_or_else(|| format!("no stored alignment joins `{edition}` and `{against}`"))?;

    let runs = sidecar.runs_of(alignment)?;

    let mut mentioned: BTreeSet<u32> = BTreeSet::new();
    for run in &runs {
        let range = match mine_is_a {
            true => Some(run.a),
            false => run.b,
        };
        if let Some(r) = range {
            mentioned.extend(r.start..=r.end);
        }
    }

    let slots = target.slots(sidecar.work.reading);

    // Group slots by the page they belong to, and keep only pages every one of
    // whose slots is unmentioned.
    let mut unpaired_pages: Vec<u32> = Vec::new();
    let mut at = 0usize;
    while at < slots.len() {
        let page = slots[at].page;
        let first = at;
        while at < slots.len() && slots[at].page == page {
            at += 1;
        }
        if (first..at).all(|i| !mentioned.contains(&(i as u32))) {
            unpaired_pages.push(page);
        }
    }

    // Where each page's slots begin, so a run can report both index spaces.
    let slot_start = |page: u32| -> u32 {
        slots
            .iter()
            .position(|s| s.page == page)
            .expect("every page has at least one slot") as u32
    };
    let slot_count = |page: u32| -> u32 { slots.iter().filter(|s| s.page == page).count() as u32 };

    let mut out: Vec<UnpairedRun> = Vec::new();
    for page in unpaired_pages {
        let (member, local) = target
            .locate(page)
            .ok_or_else(|| format!("`{edition}` has no page {page}"))?;

        // Extend the previous run only when this page is its immediate
        // successor *within the same member*. The member test is what splits a
        // run at a boundary, and it is the whole reason this function exists
        // rather than a complement over the slot set.
        match out.last_mut() {
            Some(last) if last.member == member && last.local.end == local => {
                last.pages.end = page + 1;
                last.local.end = local + 1;
                last.slots.end = slot_start(page) + slot_count(page);
            }
            _ => out.push(UnpairedRun {
                slots: slot_start(page)..slot_start(page) + slot_count(page),
                pages: page..page + 1,
                member,
                local: local..local + 1,
                at_member_end: false,
            }),
        }
    }

    for run in &mut out {
        run.at_member_end = run.local.end == target.members[run.member].pages;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sidecar;

    /// Two members of four pages each, no spreads, aligned against a hub that
    /// pairs everything except the pages each test names.
    ///
    /// Written as JSON rather than built with struct literals because the span
    /// notation is what a real sidecar carries, and a fixture that skips the
    /// parse cannot catch a span the parser reads differently.
    fn two_members(b_runs: &str) -> Sidecar {
        let text = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "W", "reading": "rtl" }},
              "editions": {{
                "raw": {{
                  "label": "raw",
                  "members": [{{ "path": "r.cbz", "sha256": "r0", "pages": 8 }}],
                  "spreads": []
                }},
                "fan": {{
                  "label": "fan",
                  "members": [
                    {{ "path": "c1.cbz", "sha256": "c1", "pages": 4 }},
                    {{ "path": "c2.cbz", "sha256": "c2", "pages": 4 }}
                  ],
                  "spreads": []
                }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "fan", "by": "solver", "runs": {b_runs} }}
              ]
            }}"#
        );
        Sidecar::from_json(&text).expect("fixture parses")
    }

    #[test]
    fn a_run_at_a_members_final_index_is_flagged_at_its_end() {
        // fan pages 0,1,2 and 4..7 pair; page 3 — c1's last — does not.
        let doc = two_members(r#"[{"a":"0-2","b":"0-2"},{"a":"3"},{"a":"4-7","b":"4-7"}]"#);
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].pages, 3..4);
        assert_eq!(runs[0].member, 0);
        assert_eq!(runs[0].local, 3..4);
        assert!(runs[0].at_member_end, "page 3 is c1's last of four");
    }

    #[test]
    fn a_run_at_a_members_first_index_is_not_flagged_at_its_end() {
        // fan page 4 — c2's first — is unpaired. This is the front-matter
        // shape, and it must not read as a credits candidate.
        let doc = two_members(r#"[{"a":"0-3","b":"0-3"},{"a":"4"},{"a":"5-7","b":"5-7"}]"#);
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].member, 1);
        assert_eq!(runs[0].local, 0..1);
        assert!(
            !runs[0].at_member_end,
            "c2 has four pages; this is its first"
        );
    }

    #[test]
    fn a_run_spanning_a_member_boundary_splits_into_two() {
        // The corpus shape: c1's last page and c2's first two, contiguous.
        // Unsplit this is one run flagged at neither end, which is how the
        // first design lost a credits page and gained four false positives.
        let doc = two_members(r#"[{"a":"0-2","b":"0-2"},{"a":"3-5"},{"a":"6-7","b":"6-7"}]"#);
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 2, "one run per member, never one spanning both");
        assert_eq!((runs[0].member, runs[0].local.clone()), (0, 3..4));
        assert!(runs[0].at_member_end);
        assert_eq!((runs[1].member, runs[1].local.clone()), (1, 0..2));
        assert!(!runs[1].at_member_end);
    }

    #[test]
    fn a_run_touching_neither_end_is_flagged_at_neither() {
        let doc = two_members(r#"[{"a":"0","b":"0"},{"a":"1-2"},{"a":"3-7","b":"3-7"}]"#);
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].local, 1..3);
        assert!(!runs[0].at_member_end);
    }

    #[test]
    fn an_edition_with_nothing_unpaired_yields_no_runs() {
        let doc = two_members(r#"[{"a":"0-7","b":"0-7"}]"#);
        assert!(
            unpaired_runs(&doc, "fan", "raw")
                .expect("resolves")
                .is_empty()
        );
    }

    #[test]
    fn a_spread_inside_an_unpaired_run_keeps_pages_and_slots_apart() {
        // The corpus cannot catch a page-for-slot substitution: Go-Toubun's fan
        // edition has five spreads and every one of them falls outside every
        // unpaired region, so both indexings agree there. This fixture makes
        // them disagree by construction.
        //
        // `fan` page 1 is a spread, so the edition is 4 pages and 5 slots:
        //   slot 0 = page 0
        //   slot 1 = page 1 right half
        //   slot 2 = page 1 left half
        //   slot 3 = page 2
        //   slot 4 = page 3
        // Leaving slots 1..3 unpaired means pages 1..2 — one page, two slots.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 5 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [{ "path": "c1.cbz", "sha256": "c1", "pages": 4 }],
              "spreads": [1]
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0","b":"0"},{"a":"1-2"},{"a":"3-4","b":"2-3"}] }
          ]
        }"#;
        let doc = Sidecar::from_json(text).expect("fixture parses");
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].slots, 1..3, "two slots");
        assert_eq!(runs[0].pages, 1..2, "one page");
        assert!(!runs[0].at_member_end, "page 1 of 4 is not the last");
    }

    #[test]
    fn a_half_paired_spread_page_is_not_unpaired() {
        // Only one half of the spread at page 1 is unmentioned. The page is
        // partly reachable, so it is not a credits candidate: a credits card is
        // never half of a joined spread.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 5 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [{ "path": "c1.cbz", "sha256": "c1", "pages": 4 }],
              "spreads": [1]
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0-1","b":"0-1R"},{"a":"2"},{"a":"3-4","b":"2-3"}] }
          ]
        }"#;
        let doc = Sidecar::from_json(text).expect("fixture parses");
        assert!(
            unpaired_runs(&doc, "fan", "raw")
                .expect("resolves")
                .is_empty(),
            "a page with any paired slot is not an unpaired page"
        );
    }

    /// The stored alignment is directed, and only one side's spans address
    /// the edition being examined. Picking the wrong side reports another
    /// edition's gaps under this edition's page numbers — a wrong answer that
    /// looks entirely well-formed. The fixture asks the same question from
    /// both ends of one edge and requires the two answers to differ.
    ///
    /// Every other test in this module examines the edge's `to` side, which
    /// is what the CLI does; nothing had ever asked the `from` side, so the
    /// arm of the match that answers it could be deleted outright.
    #[test]
    fn each_end_of_a_directed_edge_reports_its_own_gaps() {
        // One edge, `raw` -> `en`, and one run: raw slots 0-3 pair with en
        // slots 2-5. So raw's last two pages are unreached, and en's first two
        // and last two are — different pages, in different page spaces, out of
        // the same stored run.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 6 }],
              "spreads": []
            },
            "en": {
              "label": "en",
              "members": [{ "path": "e.cbz", "sha256": "e0", "pages": 8 }],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en", "by": "solver",
              "runs": [{"a":"0-3","b":"2-5"}] }
          ]
        }"#;
        let sidecar = Sidecar::from_json(text).expect("fixture parses");

        let forward = unpaired_runs(&sidecar, "raw", "en").expect("edge exists");
        let backward = unpaired_runs(&sidecar, "en", "raw").expect("same edge, other end");

        // Named exactly, because differing is not enough. `assert_ne!` alone
        // holds under a full orientation swap — with `mine_is_a` inverted,
        // `forward` becomes 0..2 and `backward` 4..8, which are still two
        // different answers. An upper bound on `pages.end` does not help
        // either: the arithmetic mutants only ever lower it. So both sides
        // are pinned to the exact runs the fixture implies, in both index
        // spaces where a wrong orientation would show.
        assert_eq!(forward.len(), 1, "{forward:?}");
        assert_eq!(forward[0].pages, 4..6, "raw's own last two pages");
        assert_eq!(forward[0].slots, 4..6);
        assert_eq!(backward.len(), 2, "{backward:?}");
        assert_eq!(backward[0].pages, 0..2, "en's own first two pages");
        assert_eq!(backward[0].slots, 0..2);
        assert_eq!(backward[1].pages, 6..8, "and its own last two");
        assert_eq!(backward[1].slots, 6..8);
    }

    /// A run of unpaired pages stops at a member boundary, and the member
    /// test is the whole reason this function exists rather than a complement
    /// over the slot set. With `||` a credits page at the end of one chapter
    /// file and another at the start of the next merge into a single run that
    /// spans a boundary no reader can see.
    ///
    /// The member sizes are chosen so the two tests disagree. `c1`'s run ends
    /// at local index 2 and `c2`'s begins at local index 2, so the local test
    /// alone says "adjacent" while the member test says "different file" — and
    /// with `||` the disagreement resolves the wrong way. The uneven fixture
    /// above cannot show this: there the local indices happen to disagree too,
    /// so either operator splits.
    #[test]
    fn an_unpaired_run_stops_at_a_member_boundary() {
        // A two-chapter fan release with a credits card at the end of each
        // chapter: `en` page 1 is c1's last, pages 4 and 5 are c2's last two.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 6 }],
              "spreads": []
            },
            "en": {
              "label": "en",
              "members": [
                { "path": "c1.cbz", "sha256": "c1", "pages": 2 },
                { "path": "c2.cbz", "sha256": "c2", "pages": 4 }
              ],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en", "by": "solver",
              "runs": [{"a":"0","b":"0"},{"a":"1"},{"a":"2-3","b":"2-3"},{"a":"4-5"}] }
          ]
        }"#;
        let sidecar = Sidecar::from_json(text).expect("fixture parses");

        let runs = unpaired_runs(&sidecar, "en", "raw").expect("edge exists");

        assert_eq!(runs.len(), 2, "{runs:?}");
        assert_ne!(runs[0].member, runs[1].member);
        // The exclusive ends: a one-page run ends one past its start.
        assert_eq!(runs[0].pages.end, runs[0].pages.start + 1);
        assert_eq!(runs[0].local.end, runs[0].local.start + 1);
        assert!(runs[0].at_member_end, "{runs:?}");
        // The second run is two pages, so it is the one that exercises the
        // extension arm. Both of its index spaces are stated, because the
        // exclusive end of an extended run is arithmetic no other test reads:
        // the existing ones assert `local` and let `pages` and `slots` pass
        // unread.
        assert_eq!(runs[1].pages, 4..6, "{runs:?}");
        assert_eq!(runs[1].slots, 4..6, "{runs:?}");
        assert_eq!(runs[1].local, 2..4, "{runs:?}");
        assert!(runs[1].at_member_end, "{runs:?}");
    }

    #[test]
    fn an_edition_the_document_does_not_have_is_refused_by_name() {
        let doc = two_members(r#"[{"a":"0-7","b":"0-7"}]"#);
        let err = unpaired_runs(&doc, "nope", "raw").expect_err("refuses");
        assert!(err.contains("nope"), "the message must name it: {err}");
    }

    #[test]
    fn a_pair_with_no_edge_is_refused_naming_both() {
        let doc = two_members(r#"[{"a":"0-7","b":"0-7"}]"#);
        let err = unpaired_runs(&doc, "fan", "fan").expect_err("refuses");
        assert!(err.contains("fan"), "the message must name the pair: {err}");
    }
}
