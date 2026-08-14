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
//!
//! # The labelling convention, written down after the fact
//!
//! **A box is the text block, not the balloon.** A balloon detector is blind to
//! the borderless narration manga uses constantly, so what is drawn is the run
//! of type, and the balloon around it — where there is one — is not part of the
//! box.
//!
//! The 480 boxes this repository holds were drawn on 2026-08-11 under
//! conventions **nobody wrote down at the time**, and they are recorded here
//! because they turned out to be load-bearing:
//!
//! - **One box per balloon, including where two balloons touch or overlap.**
//!   Where the artwork shows two balloons joined into a cluster, the labeller
//!   drew two boxes.
//! - **Sound effects were labelled thoroughly**, more completely than the
//!   project expects any detector to find them — 63 of the 480.
//! - **They were drawn to characterise the problem, not to specify a target.**
//!   At the time nothing in this project had detected anything, so the labelling
//!   was deliberately general and exploratory.
//!
//! **Why this is load-bearing rather than trivia.** The convention decides
//! `score.rs`'s boundary between `one_to_one` and `merged`. A detector that
//! emits one box per *connected balloon group* scores `merged` against a
//! per-balloon convention **even when it found exactly the right text** — that
//! is a disagreement about units, and the five outcomes cannot tell it apart
//! from a detector that genuinely over-reached. Measured on 2026-08-12: the
//! model candidate takes 87 `merged` beside 118 `one_to_one`, and how many of
//! those 87 are unit disagreement is not knowable from the score alone.
//!
//! **The open question, and it is not settled here.** What L2 pairing needs is a
//! unit that is *consistent across editions* — a reader points at a balloon on
//! the raw and lands on the English one, region to region. Consistency across
//! editions and one-box-per-balloon are not the same requirement, and they can
//! disagree. Which one the convention should encode is deferred to a
//! measurement rather than argued: see `model_experiment.rs`.
//!
//! **Changing the convention is not free.** Every figure this project has
//! stated about a detector is denominated in these boxes — 2 of 478, 48 of 478,
//! 118 of 478, and every row of the band sweep. Redrawing them makes those
//! figures incomparable, and the 30 sealed held-out pages carry the old
//! convention besides. A new convention should therefore **add** a document
//! rather than redraw this one, so the history stays readable.

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
mod tests;
