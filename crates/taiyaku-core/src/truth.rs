//! The `<stem>.truth.json` format: text blocks a person drew, per edition, per page.
//!
//! A sibling document, the convention `<stem>.regions.json` and
//! `<stem>.notes.json` already use, for the reason `regions.rs` gives: this is
//! two orders of magnitude more JSON than `taiyaku.json`, whose first two
//! design goals are human-readable and diffable in git.
//!
//! **This is what a person drew, not what a detector emitted.** The two are
//! different claims and this file is deliberately not `regions.rs`'s `Region`
//! with a field bolted on: a person can decline to answer the axis question —
//! a radial sound effect has no reading direction — and `Region::axis` is
//! `Axis`, not `Option<Axis>`. Widening that would change what every existing
//! regions document means.
//!
//! Nothing here scores anything. Comparing a detector's regions against these
//! is `taiyaku-cli`'s `score.rs`, which is native-only because nothing in the
//! browser has a use for it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::regions::{Axis, RegionMember};

/// Bumped when an existing field would mean something else.
///
/// Adding a field does not need it: this document obeys the same
/// ignore-unknown-keys rule the sidecar does, which is what lets `text` below
/// be filled in later by an OCR pass without a bump and without relabelling.
pub const VERSION: u32 = 1;

/// What a labelled box holds.
///
/// A closed set rather than a free string, so a typo is a parse error rather
/// than a silently new category that reports zero recall forever. `Other`
/// carries page numbers, publisher marks and anything else nobody wanted a
/// name for; the `ignore` flag, not this variant, is what excuses a box from
/// scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind {
    #[serde(rename = "dialogue")]
    Dialogue,
    #[serde(rename = "narration")]
    Narration,
    #[serde(rename = "sfx")]
    Sfx,
    #[serde(rename = "logo")]
    Logo,
    #[serde(rename = "credits")]
    Credits,
    #[serde(rename = "other")]
    Other,
}

/// One text run a person drew, in page fractions.
///
/// Fractions rather than pixels for `Region`'s reason: page widths run 685 to
/// 5343 pixels across editions of the same work, so a pixel box means nothing
/// without the page it was measured on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruthRegion {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// `None` where the question does not apply.
    ///
    /// A vertical column and a horizontal line have a reading direction; a
    /// radial sound effect exploding across a panel does not, and a labeller
    /// forced to pick one would be recording a coin toss as data.
    pub axis: Option<Axis>,
    pub kind: Kind,
    /// Neither a hit nor a false positive, and counted out loud.
    ///
    /// For the genuinely ambiguous tail — signage baked into background art,
    /// text on clothing, marks illegible at scan resolution. It exists because
    /// *deciding* is the expensive part of labelling, not drawing, and the
    /// scorer prints how many boxes carried this so the hatch is never silent.
    #[serde(default)]
    pub ignore: bool,
    /// Reserved. An OCR pass fills this in later.
    ///
    /// Additive, so adding it costs no version bump and no relabelling — the
    /// whole reason it is named here while nothing writes it.
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TruthEdition {
    /// In the edition's own member order, because that order is page order.
    pub members: Vec<RegionMember>,
    /// Page index in the edition's flat sequence, to its boxes.
    ///
    /// An absent key means nobody has looked at that page; a present empty
    /// value means somebody looked and found no text. Across 75 pages of
    /// manual work that difference is the progress tracker, and it is the same
    /// distinction `RegionEdition::pages` refuses to collapse.
    pub pages: BTreeMap<u32, Vec<TruthRegion>>,
}

impl TruthEdition {
    /// Whether these labels describe an archive that is no longer there.
    ///
    /// Ground truth has the sharpest form of this problem in the project. A
    /// stale alignment sends a reader to the wrong page; stale ground truth
    /// draws a box on the wrong art on the *right* page and then scores a
    /// detector against it. Content and order, not paths.
    #[must_use]
    pub fn is_stale_for(&self, digests: &[String]) -> bool {
        self.members.len() != digests.len()
            || self
                .members
                .iter()
                .zip(digests)
                .any(|(member, digest)| &member.sha256 != digest)
    }
}

/// How far past an edge a box may sit and still be taken as touching it.
///
/// A clamped drag stores `x` and `w = far - x` with both ends already inside
/// `0..1`, and `x + (far - x)` need not round back to exactly `far` — a box
/// drawn flush against the right edge can come out a single ulp over 1. One
/// part in a billion of a page is a fraction of a nanometre on any real scan,
/// while the failure this check exists for is off by whole percents: a drag
/// that ended in `PageView`'s letterbox.
const EDGE_SLACK: f64 = 1e-9;

/// Which field of a box puts it outside the page, and its value.
///
/// **A box outside `0..1` is refused rather than stored, because the scorer
/// divides by its area.** `classify_single_cover` measures the overlap as a
/// fraction of `t.w * t.h`, so area that is not on the page dilutes the truth
/// side of the ratio: a genuine one-to-one demotes to `Fragment` or `Missed`
/// and its prediction becomes `spurious`. Both terms of the promotion rule
/// move, in the punishing direction, and nothing anywhere says why. The
/// labeller clamps a drag before storing it (`labelling.ts`'s `addBox`); this
/// is the second line, for a document written by anything else.
///
/// A non-finite coordinate is refused here too: `!(0.0..=1.0).contains(&NAN)`
/// is true. `score.rs`'s `intersection` guards NaN as well, and for the same
/// reason — `f64::max` discards NaN rather than propagating it, so a corrupt
/// box would otherwise score as a perfect hit.
fn off_the_page(region: &TruthRegion) -> Option<(&'static str, f64)> {
    for (field, value) in [
        ("x", region.x),
        ("y", region.y),
        ("w", region.w),
        ("h", region.h),
    ] {
        if !(0.0..=1.0).contains(&value) {
            return Some((field, value));
        }
    }
    if region.x + region.w > 1.0 + EDGE_SLACK {
        return Some(("x + w", region.x + region.w));
    }
    if region.y + region.h > 1.0 + EDGE_SLACK {
        return Some(("y + h", region.y + region.h));
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthDoc {
    pub taiyaku_truth: u32,
    /// Edition key, exactly as the sidecar spells it.
    #[serde(default)]
    pub editions: BTreeMap<String, TruthEdition>,
}

impl TruthDoc {
    #[must_use]
    pub fn new() -> Self {
        Self {
            taiyaku_truth: VERSION,
            editions: BTreeMap::new(),
        }
    }

    /// Where this document lives for a given sidecar: same directory, same stem.
    ///
    /// Built from `file_stem` and a `format!`, not `set_extension` twice —
    /// which loses a dot in the stem rather than keeping it. The arithmetic
    /// `RegionDoc::path_beside` and `Work::sibling` already carry, and the
    /// reason each states.
    #[must_use]
    pub fn path_beside(sidecar: &Path) -> PathBuf {
        let stem = sidecar.file_stem().map_or_else(
            || "taiyaku".to_string(),
            |s| s.to_string_lossy().to_string(),
        );
        sidecar
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{stem}.truth.json"))
    }

    /// # Errors
    ///
    /// Returns an error if `text` is not valid JSON, or is JSON that does not
    /// match this document's shape — a missing `taiyaku_truth` field, a `kind`
    /// outside the closed set, an `axis` other than `"v"`, `"h"` or `null`, a
    /// `pages` entry keyed by something other than a page number. The message
    /// embeds `serde_json`'s own, which names the field and byte offset.
    ///
    /// Also returns an error if any box lies outside the page — see
    /// [`off_the_page`] for why that is refused rather than stored.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: Self = serde_json::from_str(text)
            .map_err(|e| format!("not a readable truth document: {e}"))?;
        for (edition, labelled) in &doc.editions {
            for (page, regions) in &labelled.pages {
                for (index, region) in regions.iter().enumerate() {
                    if let Some((field, value)) = off_the_page(region) {
                        return Err(format!(
                            "not a readable truth document: edition `{edition}` page {page} box \
                             {index} has {field} = {value}, which is off the page — a box is in \
                             page fractions and every edge of it must lie within 0..1"
                        ));
                    }
                }
            }
        }
        Ok(doc)
    }

    /// # Panics
    ///
    /// Panics if this document fails to serialise. In practice it cannot:
    /// every field is a plain number, string, `Option`, `BTreeMap` or enum
    /// with no custom `Serialize` impl, so the only way `serde_json` returns
    /// `Err` here — a non-finite float used as a map key — does not apply.
    /// The float fields are values, not keys, and `u32` page keys are finite.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("truth is always serialisable")
    }
}

impl Default for TruthDoc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> TruthDoc {
        let mut pages = BTreeMap::new();
        pages.insert(
            7,
            vec![
                TruthRegion {
                    x: 0.10,
                    y: 0.20,
                    w: 0.15,
                    h: 0.30,
                    axis: Some(Axis::Vertical),
                    kind: Kind::Dialogue,
                    ignore: false,
                    text: None,
                },
                TruthRegion {
                    x: 0.55,
                    y: 0.40,
                    w: 0.28,
                    h: 0.12,
                    axis: None,
                    kind: Kind::Sfx,
                    ignore: true,
                    text: None,
                },
            ],
        );
        // Present and empty: somebody looked at page 8 and there was no text.
        pages.insert(8, Vec::new());
        let mut editions = BTreeMap::new();
        editions.insert(
            "raw-mono".to_string(),
            TruthEdition {
                members: vec![RegionMember {
                    sha256: "aa".into(),
                }],
                pages,
            },
        );
        TruthDoc {
            taiyaku_truth: VERSION,
            editions,
        }
    }

    #[test]
    fn a_document_survives_a_round_trip() {
        let there_and_back = TruthDoc::from_json(&doc().to_json()).unwrap();
        assert_eq!(there_and_back.taiyaku_truth, VERSION);
        let edition = &there_and_back.editions["raw-mono"];
        assert_eq!(edition.pages[&7].len(), 2);
        assert_eq!(edition.pages[&7][0].axis, Some(Axis::Vertical));
        assert_eq!(edition.pages[&7][0].kind, Kind::Dialogue);
        assert!(!edition.pages[&7][0].ignore);
        assert_eq!(edition.pages[&7][1].axis, None);
        assert!(edition.pages[&7][1].ignore);
    }

    #[test]
    fn a_page_looked_at_and_a_page_never_opened_are_different() {
        let there_and_back = TruthDoc::from_json(&doc().to_json()).unwrap();
        let pages = &there_and_back.editions["raw-mono"].pages;
        assert!(pages.contains_key(&8), "page 8 was looked at");
        assert!(pages[&8].is_empty(), "and held no text");
        assert!(!pages.contains_key(&9), "page 9 was never opened");
    }

    #[test]
    fn an_axis_may_be_null_because_a_radial_effect_has_no_reading_direction() {
        let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":null,"kind":"sfx"}]}}}}"#;
        let parsed = TruthDoc::from_json(json).unwrap();
        assert_eq!(parsed.editions["e"].pages[&3][0].axis, None);
    }

    #[test]
    fn ignore_and_text_default_when_absent() {
        let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":"v","kind":"dialogue"}]}}}}"#;
        let region = &TruthDoc::from_json(json).unwrap().editions["e"].pages[&3][0];
        assert!(!region.ignore);
        assert_eq!(region.text, None);
    }

    #[test]
    fn an_unknown_kind_is_refused_and_the_message_names_the_field() {
        let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":"v","kind":"balloon"}]}}}}"#;
        let err = TruthDoc::from_json(json).unwrap_err();
        // Naming the rejected value and a `Kind`-only variant, rather than
        // serde's generic "variant": `err.contains("variant")` alone passes
        // just as well when it is `axis` that is malformed, so it does not
        // establish that the *kind* was what was refused.
        assert!(err.contains("balloon"), "names the value it refused: {err}");
        assert!(
            err.contains("dialogue"),
            "names Kind's own variants, so this is Kind and not Axis: {err}"
        );
    }

    /// A drag that started or ended in the letterbox stores a box bigger than
    /// the page, and the scorer divides by that inflated area — so a genuine
    /// one-to-one demotes and its prediction turns spurious, with nothing
    /// saying why. Refused on the way in, naming the field.
    #[test]
    fn a_box_off_the_page_is_refused_and_the_message_names_the_field() {
        // A drag that ran off the left edge: x negative.
        let left = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":-0.05,"y":0.1,"w":0.2,"h":0.2,
            "axis":"v","kind":"dialogue"}]}}}}"#;
        let err = TruthDoc::from_json(left).unwrap_err();
        assert!(
            err.contains("x = -0.05"),
            "names the field and value: {err}"
        );
        assert!(err.contains("off the page"), "{err}");

        // A drag that ran off the bottom: y and h are each inside 0..1, and
        // only their sum is not. A per-field check alone cannot see this one.
        let bottom = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.1,"y":0.8,"w":0.2,"h":0.5,
            "axis":"v","kind":"dialogue"}]}}}}"#;
        let err = TruthDoc::from_json(bottom).unwrap_err();
        assert!(err.contains("y + h"), "names the far edge: {err}");
        assert!(err.contains("page 3"), "and where it is: {err}");

        // A width past the page on its own.
        let wide = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.1,"w":1.4,"h":0.2,
            "axis":"v","kind":"dialogue"}]}}}}"#;
        let err = TruthDoc::from_json(wide).unwrap_err();
        assert!(err.contains("w = 1.4"), "{err}");
    }

    /// And a box that legitimately touches an edge still parses. A clamped
    /// drag stores `w = far - x`, and `x + w` need not round back to exactly
    /// `far` — refusing that would reject the boxes the labeller writes for
    /// text bleeding to the page edge, which is the very case the clamp exists
    /// for.
    #[test]
    fn a_box_flush_against_the_page_edge_is_not_refused() {
        let flush = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":1.0,"h":1.0,
            "axis":"v","kind":"dialogue"},
            {"x":0.7,"y":0.7,"w":0.30000000000000004,"h":0.3,
            "axis":"v","kind":"dialogue"}]}}}}"#;
        let parsed = TruthDoc::from_json(flush).expect("a box on the edge is on the page");
        assert_eq!(parsed.editions["e"].pages[&3].len(), 2);
    }

    #[test]
    fn a_replaced_archive_makes_a_document_stale() {
        let edition = &doc().editions["raw-mono"];
        assert!(!edition.is_stale_for(&["aa".to_string()]));
        assert!(edition.is_stale_for(&["bb".to_string()]), "content changed");
        assert!(edition.is_stale_for(&[]), "member count changed");
    }

    #[test]
    fn the_document_sits_beside_the_sidecar_keeping_a_dotted_stem() {
        assert_eq!(
            TruthDoc::path_beside(Path::new("/c/taiyaku.json")),
            Path::new("/c/taiyaku.truth.json")
        );
        assert_eq!(
            TruthDoc::path_beside(Path::new("/c/go.between.json")),
            Path::new("/c/go.between.truth.json"),
            "set_extension twice would eat the dot"
        );
    }
}
