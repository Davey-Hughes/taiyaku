//! The `<stem>.regions.json` format: text blocks, per edition, per page.
//!
//! A sibling document rather than a block in `taiyaku.json`, on size. Regions
//! are per page, and a handful of blocks a page is far more JSON than the file
//! it would sit in. `taiyaku.json`'s first two design goals are human-readable
//! and diffable in git, and this would end both.
//!
//! The convention is the one `<stem>.notes.json` and `taiyaku.stump.json`
//! already use: same directory, same stem. It is also stronger forward
//! compatibility than `sidecar.md`'s ignore-unknown-keys rule gives — an L0
//! reader does not merely ignore these keys, it never fetches the file.
//!
//! **Nothing here pairs anything.** Which region of one edition corresponds to
//! which region of another is derived at read time from the alignment and the
//! two pages' dimensions, and is deliberately not stored: a stored pairing is a
//! derived thing that outlives what it derived from, which is O24's open
//! failure mode. See the L2 design, §5.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bumped when an existing field would mean something else.
///
/// Adding a field does not need it: this document obeys the same
/// ignore-unknown-keys rule the sidecar does.
pub const VERSION: u32 = 1;

/// Which way the text in a block runs.
///
/// Stored because the clustering already knows it, because it is the first
/// thing a later text pass needs, and because it is a correctness check that
/// costs nothing and needs no ground truth: a Japanese edition coming back
/// mostly `Horizontal` means the detector is wrong, and the command prints the
/// two counts a person can see that in. It draws no conclusion of its own —
/// `taiyaku regions` reports `v=` and `h=` and leaves the judgement to whoever
/// knows which way the edition reads, which the document does not record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    #[serde(rename = "v")]
    Vertical,
    #[serde(rename = "h")]
    Horizontal,
}

/// One text block, in page fractions.
///
/// Fractions rather than pixels, which is the unit every other geometry in this
/// project uses, and the reason is the corpus: page widths differ widely across
/// editions of the same work, so a pixel box means nothing without the page it
/// was measured on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub axis: Axis,
}

/// A member's content hash, and nothing else.
///
/// Paths are already in the sidecar and would only drift from it. What this
/// document needs is the ability to notice that the archive was replaced, and
/// that is the hash's job here exactly as it is in the sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionMember {
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegionEdition {
    /// In the edition's own member order, because that order is page order.
    pub members: Vec<RegionMember>,
    /// Page index in the edition's flat sequence, to its blocks.
    ///
    /// A map rather than a list of lists: an absent key means nothing has
    /// looked at that page, and a present empty value means something looked
    /// and found no text. A list cannot tell those apart, and the difference is
    /// exactly the silence this repository is written against.
    pub pages: BTreeMap<u32, Vec<Region>>,
}

impl RegionEdition {
    /// Whether these regions describe an archive that is no longer there.
    ///
    /// Content and order, not paths — the idiom `merge_spoke`'s guard states.
    /// Regions have the sharper version of the staleness problem the sidecar
    /// has: a stale alignment sends a reader to the wrong page, where stale
    /// regions draw a box on the wrong art on the *right* page, which is harder
    /// to notice.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionDoc {
    pub taiyaku_regions: u32,
    /// Edition key, exactly as the sidecar spells it.
    #[serde(default)]
    pub editions: BTreeMap<String, RegionEdition>,
}

impl RegionDoc {
    #[must_use]
    pub fn new() -> Self {
        Self {
            taiyaku_regions: VERSION,
            editions: BTreeMap::new(),
        }
    }

    /// Where this document lives for a given sidecar: same directory, same stem.
    ///
    /// `taiyaku.json` becomes `taiyaku.regions.json`. Built from `file_stem`
    /// and a `format!`, the same arithmetic [`crate::TruthDoc::path_beside`]
    /// carries for `<stem>.truth.json` and `taiyaku-library`'s
    /// `export::stem_of` for the four documents a library round-trips —
    /// not `set_extension` twice, which loses a dot in the stem rather than
    /// keeping it: `go.between.json` first strips to `go.between`, whose own
    /// extension is then `between`, and the second call replaces that,
    /// giving `go.regions.json` rather than `go.between.regions.json`.
    #[must_use]
    pub fn path_beside(sidecar: &Path) -> PathBuf {
        let stem = sidecar.file_stem().map_or_else(
            || "taiyaku".to_string(),
            |s| s.to_string_lossy().to_string(),
        );
        sidecar
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{stem}.regions.json"))
    }

    /// # Errors
    ///
    /// Returns an error if `text` is not valid JSON, or is JSON that does not
    /// match this document's shape — a missing `taiyaku_regions` field, an
    /// `axis` value other than `"v"` or `"h"`, a `pages` entry keyed by
    /// something other than a page number. The message embeds `serde_json`'s
    /// own, which names the field and byte offset.
    ///
    /// Also returns an error if the document declares a `taiyaku_regions`
    /// version newer than [`VERSION`] — see [`crate::refuse_newer`].
    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: Self = serde_json::from_str(text)
            .map_err(|e| format!("not a readable regions document: {e}"))?;
        crate::refuse_newer("taiyaku_regions", doc.taiyaku_regions, VERSION)?;
        Ok(doc)
    }

    /// # Panics
    ///
    /// Panics if this document fails to serialise. In practice it cannot:
    /// every field is a plain number, string, `BTreeMap` or enum with no
    /// custom `Serialize` impl, so the only way `serde_json` returns `Err`
    /// here — a non-finite float used as a map key — does not apply, since
    /// this document's only float fields (`Region`'s `x`, `y`, `w`, `h`) are
    /// values, not keys, and `u32` page keys are always finite.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("regions are always serialisable")
    }
}

impl Default for RegionDoc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
