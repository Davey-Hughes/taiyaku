# L2a — text-block detection, `taiyaku regions`, and the region document

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect text blocks in one edition's pages and record them in a sibling
document `<stem>.regions.json`, behind a ninth command that proposes and prints
and writes only under `--write`.

**Architecture:** The document format lives in `taiyaku-core` beside `sidecar`,
because it is a format and because L2b's pairing rule and the browser both have
to read it. Detection lives in `taiyaku-cli` beside `fingerprint`, because it
decodes images and that is native-only. The command follows `credits`
line for line: it identifies the edition by content hash rather than by name,
prints a per-page table to stderr and one fixed-field line to stdout, and writes
only under `--write`.

**Tech Stack:** Rust 2024 workspace. `serde`/`serde_json` in core; `image` and
`rayon` in the CLI. Bash for the survey.

## Global Constraints

Copied from `docs/superpowers/specs/2026-08-08-l2-text-regions-design.md`. Every
task's requirements implicitly include this section.

- **No new dependency.** `image` and `rayon` are already `taiyaku-cli`'s;
  `serde`/`serde_json` are already `taiyaku-core`'s. Connected-component
  labelling and run-length smearing are written here, not pulled in.
- **`taiyaku-core` must keep compiling to `wasm32-unknown-unknown`.** No
  threads, no native libraries, no `rayon`. CI enforces it.
- **A region is a text block, never a balloon outline.** Five fields:
  `x`, `y`, `w`, `h` in page fractions, and `axis` of `"v"` or `"h"`.
- **Keyed by page index in the edition's flat page sequence**, the same
  addressing `spreads` and `credits` use. Page, not slot.
- **`--write` writes through `taiyaku_serve::atomic::write`.** O26 and O29 are
  paid for there.
- **Unknown flags are refused** (O21), and **a flag that cannot take effect is
  refused rather than ignored** (O19, O20, O22).
- **No OCR, no text extraction, no pairing.** Pairing is L2b.
- **Nothing in the aligner, the solver, `verdict`, or any alignment changes.**
- Detection constants are **starting values to be settled by the survey**, and
  every one of them says so at its definition.

## Deviations from the spec, and why

Two, both found by checking the plan against the Global Constraints above rather
than against the spec's prose. Neither is a scope change.

**1. `--cache` is not a flag.** The spec's §3 gives the signature as
`taiyaku regions <member>... --sidecar FILE [--cache DIR] [--write]`, and §4 says
detection results are cached under `.taiyaku-cache/` keyed on the member's
SHA-256. Both are dropped:

- A flag that cannot take effect is **refused rather than accepted**, which is
  the rule O19, O20 and O22 each closed a defect to establish. Accepting a
  `--cache` that does nothing breaks that rule in the act of citing it. The
  fingerprint cache stores hashes and dimensions, not pixels, so there is
  nothing for this command to read from it.
- A detection cache keyed on the member's SHA-256 would hold the same regions
  the region document already holds, keyed on the same hash, in a second place
  that can disagree with the first. That is the duplication O10 was closed to
  remove, and the region document is the durable artefact — `--write` is what
  makes a run cost nothing the second time.

If a full-corpus run turns out slow enough to matter, the survey in Task 6 is
what will say so, and a cache can be added then against a number.

**2. `is_stale_for` ships unused.** Task 1 builds and tests it; nothing in L2a
calls it. It is the reader's check (spec §11 — a mismatch means the regions are
ignored rather than drawn on the wrong art), and it belongs with the format
rather than with the consumer. It is `pub` in a library crate, so this raises no
warning; it is named here so a reviewer does not read it as an oversight.

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-core/src/regions.rs` (create) | The `<stem>.regions.json` format: types, parse, emit, staleness |
| `crates/taiyaku-core/src/lib.rs` (modify) | Declare and re-export the module |
| `crates/taiyaku-cli/src/detect.rs` (create) | Pixels → text blocks. Otsu, labelling, smearing, filtering, axis |
| `crates/taiyaku-cli/src/regions.rs` (create) | The `taiyaku regions` command: arguments, edition-by-content, table, survey line, `--write` |
| `crates/taiyaku-cli/src/fingerprint.rs` (modify) | Widen `decode_all` to `pub(crate)` so detection reuses the bounded parallel decode |
| `crates/taiyaku-cli/src/main.rs` (modify) | Module declaration, dispatch arm, usage text, and `edition_of_digests` shared out of `credits` |
| `crates/taiyaku-cli/src/credits.rs` (modify) | Loses its copy of `edition_of_digests` and calls the shared one |
| `scripts/region-survey.sh` (create) | One `regions:` line per surveyable edition |
| `docs/sidecar.md`, `docs/status.md`, `README.md` (modify) | The command list and the corrected L2 lines |

Seven tasks. Tasks 1–5 each end green and committed; Task 6 produces the numbers
that settle the constants; Task 7 is the documentation the earlier tasks made
stale.

---

### Task 1: The region document format

**Files:**
- Create: `crates/taiyaku-core/src/regions.rs`
- Modify: `crates/taiyaku-core/src/lib.rs`
- Test: in-file `#[cfg(test)] mod tests`, the convention every core module uses

**Interfaces:**
- Consumes: nothing.
- Produces: `taiyaku_core::{Axis, Region, RegionDoc, RegionEdition, RegionMember}`.
  `RegionDoc::from_json(&str) -> Result<RegionDoc, String>`,
  `RegionDoc::to_json(&self) -> String`,
  `RegionDoc::path_beside(sidecar: &Path) -> PathBuf`,
  `RegionEdition::is_stale_for(&self, digests: &[String]) -> bool`.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-core/src/regions.rs` with only this test module plus the
`use` line, so it fails to compile for the right reason:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> RegionDoc {
        let mut pages = BTreeMap::new();
        pages.insert(
            4,
            vec![Region { x: 0.12, y: 0.08, w: 0.11, h: 0.31, axis: Axis::Vertical }],
        );
        pages.insert(5, Vec::new());
        let mut editions = BTreeMap::new();
        editions.insert(
            "raw-mono".to_string(),
            RegionEdition {
                members: vec![RegionMember { sha256: "3f9a".to_string() }],
                pages,
            },
        );
        RegionDoc { taiyaku_regions: VERSION, editions }
    }

    #[test]
    fn a_document_round_trips_through_json() {
        let parsed = RegionDoc::from_json(&doc().to_json()).expect("parses");
        assert_eq!(parsed.editions["raw-mono"].pages[&4][0].axis, Axis::Vertical);
        assert_eq!(parsed.editions["raw-mono"].pages[&4][0].x, 0.12);
        assert_eq!(parsed.editions["raw-mono"].members[0].sha256, "3f9a");
    }

    /// The distinction is load-bearing and is why `pages` is a map rather than
    /// a list: a page that was looked at and holds no text is a different fact
    /// from a page nothing has looked at, and only the first can be stated.
    #[test]
    fn a_page_detected_empty_is_not_a_page_never_detected() {
        let parsed = RegionDoc::from_json(&doc().to_json()).expect("parses");
        let pages = &parsed.editions["raw-mono"].pages;
        assert!(pages.contains_key(&5), "5 was detected and found nothing");
        assert!(pages[&5].is_empty());
        assert!(!pages.contains_key(&6), "6 was never detected");
    }

    #[test]
    fn an_axis_is_one_character_in_the_file() {
        assert!(doc().to_json().contains("\"axis\": \"v\""));
    }

    /// Page keys are integers in Rust and strings in JSON, and a reader that
    /// disagreed about that would read every page as missing.
    #[test]
    fn page_keys_are_strings_in_the_file() {
        assert!(doc().to_json().contains("\"4\""));
    }

    #[test]
    fn a_replaced_archive_makes_an_edition_stale() {
        let edition = &doc().editions["raw-mono"];
        assert!(!edition.is_stale_for(&["3f9a".to_string()]));
        assert!(edition.is_stale_for(&["c104".to_string()]));
        assert!(
            edition.is_stale_for(&["3f9a".to_string(), "c104".to_string()]),
            "a member added to the edition is a change too"
        );
    }

    /// The same rule `sidecar.md` states for the sidecar: order is page order,
    /// so a reordering is a different edition and not the same one.
    #[test]
    fn member_order_is_part_of_the_identity() {
        let mut two = doc();
        let edition = two.editions.get_mut("raw-mono").expect("present");
        edition.members = vec![
            RegionMember { sha256: "a".to_string() },
            RegionMember { sha256: "b".to_string() },
        ];
        assert!(!edition.is_stale_for(&["a".to_string(), "b".to_string()]));
        assert!(edition.is_stale_for(&["b".to_string(), "a".to_string()]));
    }

    #[test]
    fn the_document_sits_beside_the_sidecar_on_its_stem() {
        assert_eq!(
            RegionDoc::path_beside(Path::new("/w/taiyaku.json")),
            PathBuf::from("/w/taiyaku.regions.json")
        );
        assert_eq!(
            RegionDoc::path_beside(Path::new("gotoubun.json")),
            PathBuf::from("gotoubun.regions.json")
        );
    }

    #[test]
    fn an_unreadable_document_says_what_it_is() {
        let err = RegionDoc::from_json("{").expect_err("refuses");
        assert!(err.contains("regions"), "names the document: {err}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core regions`
Expected: FAIL — `cannot find type RegionDoc in this scope`, and the module is
not declared yet.

- [ ] **Step 3: Write the implementation**

Put this **above** the test module in `crates/taiyaku-core/src/regions.rs`:

```rust
//! The `<stem>.regions.json` format: text blocks, per edition, per page.
//!
//! A sibling document rather than a block in `taiyaku.json`, on size. The real
//! Go-Toubun v01 alignment is five runs; regions are per page, and an edition of
//! 195 pages at a handful of blocks a page is two orders of magnitude more JSON
//! than the file it would sit in. `taiyaku.json`'s first two design goals are
//! human-readable and diffable in git, and this would end both.
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
/// mostly `Horizontal` means the detector is wrong, and the command says so.
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
/// project uses, and the reason is the corpus: page widths run from 685 to 5343
/// pixels across editions of the same work, so a pixel box means nothing without
/// the page it was measured on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
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
    pub fn new() -> Self {
        Self { taiyaku_regions: VERSION, editions: BTreeMap::new() }
    }

    /// Where this document lives for a given sidecar: same directory, same stem.
    ///
    /// `taiyaku.json` becomes `taiyaku.regions.json`. `set_extension` twice
    /// rather than string surgery, so a stem containing a dot keeps it.
    pub fn path_beside(sidecar: &Path) -> PathBuf {
        let mut path = sidecar.to_path_buf();
        path.set_extension("");
        path.set_extension("regions.json");
        path
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("not a readable regions document: {e}"))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("regions are always serialisable")
    }
}

impl Default for RegionDoc {
    fn default() -> Self {
        Self::new()
    }
}
```

Then declare and export it in `crates/taiyaku-core/src/lib.rs`. Add `mod regions;`
to the module block in alphabetical order (after `mod credits;`, before
`mod route;`), and add this `pub use` after the `pub use credits::…` line:

```rust
pub use regions::{
    Axis, Region, RegionDoc, RegionEdition, RegionMember, VERSION as REGIONS_VERSION,
};
```

`VERSION` is renamed on the way out because `sidecar::VERSION` is already
exported under that name and means the sidecar's.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core regions`
Expected: PASS, 8 tests.

- [ ] **Step 5: Check the WASM target still builds**

Run: `cargo build -p taiyaku-core --target wasm32-unknown-unknown`
Expected: success. This is the constraint CI enforces on this crate and the one
a new module is most likely to break.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-core/src/regions.rs crates/taiyaku-core/src/lib.rs
git commit -m "core: the regions document, a sibling of the sidecar on its own stem"
```

---

### Task 2: Otsu threshold and connected components

**Files:**
- Create: `crates/taiyaku-cli/src/detect.rs`
- Test: in-file `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `image::GrayImage`.
- Produces: `otsu(&GrayImage) -> u8`, `Bounds { x0: u32, y0: u32, x1: u32, y1: u32 }`
  with `width()`, `height()`, `longer()`, `shorter()`, and
  `components(&GrayImage, threshold: u8) -> Vec<Bounds>`.
  `Bounds` is inclusive on both corners.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-cli/src/detect.rs` containing only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma};

    /// A page that is half black and half white: the only threshold that
    /// separates them is somewhere between the two values.
    #[test]
    fn otsu_separates_two_levels() {
        let page = GrayImage::from_fn(20, 20, |x, _| {
            Luma([if x < 10 { 30 } else { 220 }])
        });
        let t = otsu(&page);
        assert!((30..220).contains(&t), "threshold {t} sits between the two levels");
    }

    /// A uniform page has no threshold that separates anything, and the failure
    /// to say so is how a blank page becomes one page-sized component.
    #[test]
    fn otsu_on_a_uniform_page_finds_nothing_dark() {
        let blank = GrayImage::from_fn(20, 20, |_, _| Luma([255]));
        assert!(components(&blank, otsu(&blank)).is_empty());
    }

    fn with_marks(marks: &[(u32, u32, u32, u32)]) -> GrayImage {
        GrayImage::from_fn(40, 40, |x, y| {
            let inside = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inside { 0 } else { 255 }])
        })
    }

    #[test]
    fn two_separated_marks_are_two_components() {
        let found = components(&with_marks(&[(2, 2, 4, 4), (20, 20, 4, 4)]), 128);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_component_bounds_its_mark_exactly() {
        let found = components(&with_marks(&[(2, 3, 4, 5)]), 128);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], Bounds { x0: 2, y0: 3, x1: 5, y1: 7 });
        assert_eq!(found[0].width(), 4);
        assert_eq!(found[0].height(), 5);
    }

    /// Eight-connectivity, not four. A diagonal stroke is one mark to a reader
    /// and would be a staircase of separate components under four.
    #[test]
    fn a_diagonal_stroke_is_one_component() {
        let diagonal = GrayImage::from_fn(20, 20, |x, y| {
            Luma([if x == y && x < 10 { 0 } else { 255 }])
        });
        assert_eq!(components(&diagonal, 128).len(), 1);
    }

    /// A page-sized component is what a flood fill produces if it walks the
    /// light pixels instead of the dark ones, so this pins the polarity.
    #[test]
    fn dark_is_ink_and_light_is_paper() {
        let found = components(&with_marks(&[(2, 2, 4, 4)]), 128);
        assert_eq!(found.len(), 1);
        assert!(found[0].width() < 10, "bounded the mark, not the paper");
    }

    #[test]
    fn longer_and_shorter_do_not_care_which_axis() {
        let wide = Bounds { x0: 0, y0: 0, x1: 9, y1: 4 };
        let tall = Bounds { x0: 0, y0: 0, x1: 4, y1: 9 };
        assert_eq!((wide.longer(), wide.shorter()), (10, 5));
        assert_eq!((tall.longer(), tall.shorter()), (10, 5));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli detect`
Expected: FAIL — the module is not declared, so `cargo` reports the file is not
included. Add `mod detect;` to `crates/taiyaku-cli/src/main.rs` in the module
block (alphabetically, after `mod credits;`) and re-run. Expected then: FAIL with
`cannot find function otsu in this scope`.

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/taiyaku-cli/src/detect.rs`:

```rust
//! Pixels to text blocks.
//!
//! Four stages, none of them a model and none of them a new dependency: threshold
//! (Otsu), label the connected components, smear the components into blocks at a
//! gap scaled to the page's own stroke size, then filter.
//!
//! **Why this is tractable at all without a model:** manga is very nearly
//! bilevel already. That is the same property `notes.md` records from the other
//! direction — the dhash is invariant to text *substitution* and not to text
//! *presence*, because a text cell is dark marks on white at comparable density
//! in either language.
//!
//! **Every constant here is a starting value, not a measurement.** They are
//! settled by `scripts/region-survey.sh` over the corpus, the way
//! `MAX_CREDIT_RUN` was, and each says so where it is defined. A constant in
//! this file asserted as measured before that survey has run is a defect.

use image::GrayImage;

/// A bounding box in pixels, inclusive on both corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Bounds {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0 + 1
    }

    pub fn height(&self) -> u32 {
        self.y1 - self.y0 + 1
    }

    /// The longer and shorter sides, without saying which axis they came from.
    ///
    /// Both text directions are ordinary here — Japanese runs vertical and
    /// English typeset runs horizontal — so a filter written in terms of width
    /// and height would have to be written twice and would drift.
    pub fn longer(&self) -> u32 {
        self.width().max(self.height())
    }

    pub fn shorter(&self) -> u32 {
        self.width().min(self.height())
    }

    fn merge(&self, other: &Bounds) -> Bounds {
        Bounds {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// Otsu's threshold: the level that best separates the page's two populations.
///
/// Chosen over a fixed level because the corpus spans scans, re-encodes and one
/// archival TIFF set, and a fixed level would read a grey scan as solid ink. It
/// is the standard method, it is a histogram and one pass over 256 bins, and it
/// needs nothing this crate does not already have.
pub fn otsu(page: &GrayImage) -> u8 {
    let mut histogram = [0u64; 256];
    for pixel in page.pixels() {
        histogram[pixel.0[0] as usize] += 1;
    }
    let total: u64 = histogram.iter().sum();
    if total == 0 {
        return 0;
    }
    let weighted: u64 = histogram
        .iter()
        .enumerate()
        .map(|(level, count)| level as u64 * count)
        .sum();

    let (mut below, mut below_weighted) = (0u64, 0u64);
    let (mut best_level, mut best_variance) = (0u8, -1.0f64);
    for level in 0..256usize {
        below += histogram[level];
        if below == 0 {
            continue;
        }
        let above = total - below;
        if above == 0 {
            break;
        }
        below_weighted += level as u64 * histogram[level];
        let mean_below = below_weighted as f64 / below as f64;
        let mean_above = (weighted - below_weighted) as f64 / above as f64;
        let variance = below as f64 * above as f64 * (mean_below - mean_above).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_level = level as u8;
        }
    }
    best_level
}

/// Bounding boxes of the connected regions of ink.
///
/// Ink is dark: a pixel is ink where its luma is at or below `threshold`. Eight
/// connectivity, because a diagonal stroke is one mark to a reader and four
/// connectivity would return it as a staircase of separate components.
///
/// An explicit stack rather than recursion. A run of ink down one side of a
/// full-page panel border is tens of thousands of pixels, and that is a stack
/// overflow rather than a slow function.
pub fn components(page: &GrayImage, threshold: u8) -> Vec<Bounds> {
    let (width, height) = page.dimensions();
    let mut seen = vec![false; (width as usize) * (height as usize)];
    let mut found = Vec::new();
    let mut stack: Vec<(u32, u32)> = Vec::new();

    let is_ink = |x: u32, y: u32| page.get_pixel(x, y).0[0] <= threshold;
    let at = |x: u32, y: u32| (y as usize) * (width as usize) + (x as usize);

    for y in 0..height {
        for x in 0..width {
            if seen[at(x, y)] || !is_ink(x, y) {
                continue;
            }
            seen[at(x, y)] = true;
            stack.push((x, y));
            let mut bounds = Bounds { x0: x, y0: y, x1: x, y1: y };

            while let Some((cx, cy)) = stack.pop() {
                bounds = bounds.merge(&Bounds { x0: cx, y0: cy, x1: cx, y1: cy });
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let nx = cx as i64 + dx;
                        let ny = cy as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                            continue;
                        }
                        let (nx, ny) = (nx as u32, ny as u32);
                        if seen[at(nx, ny)] || !is_ink(nx, ny) {
                            continue;
                        }
                        seen[at(nx, ny)] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            found.push(bounds);
        }
    }
    found
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli detect`
Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/detect.rs crates/taiyaku-cli/src/main.rs
git commit -m "detect: Otsu and connected components, the two stages under everything else"
```

---

### Task 3: Stroke scale, smearing into blocks, and the axis

**Files:**
- Modify: `crates/taiyaku-cli/src/detect.rs`
- Test: same in-file module

**Interfaces:**
- Consumes: `Bounds`, `components` from Task 2.
- Produces: `stroke_scale(&[Bounds]) -> Option<u32>`,
  `merge_along(&[Bounds], Axis, gap: u32) -> Vec<Bounds>`,
  `axis_of(&[Bounds], gap: u32) -> Axis`.
  `Axis` is `taiyaku_core::Axis`, not a second copy.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `crates/taiyaku-cli/src/detect.rs`:

```rust
    fn boxes(spans: &[(u32, u32, u32, u32)]) -> Vec<Bounds> {
        spans
            .iter()
            .map(|&(x0, y0, x1, y1)| Bounds { x0, y0, x1, y1 })
            .collect()
    }

    /// The median of the longer sides, which is a proxy for glyph size that
    /// does not care whether a glyph came back as one component or as five
    /// strokes — the median moves with the strokes, and the gap is a multiple
    /// of it, so both scale together.
    #[test]
    fn stroke_scale_is_the_median_longer_side() {
        let strokes = boxes(&[(0, 0, 3, 3), (0, 0, 9, 9), (0, 0, 5, 5)]);
        assert_eq!(stroke_scale(&strokes), Some(6));
        assert_eq!(stroke_scale(&[]), None);
    }

    /// Two glyphs a small gap apart, stacked vertically: merged along the
    /// vertical axis they are one column; along the horizontal they are two.
    #[test]
    fn merging_along_an_axis_joins_what_is_spaced_along_it() {
        let stacked = boxes(&[(10, 0, 19, 9), (10, 14, 19, 23)]);
        assert_eq!(merge_along(&stacked, Axis::Vertical, 8).len(), 1);
        assert_eq!(merge_along(&stacked, Axis::Horizontal, 8).len(), 2);
    }

    #[test]
    fn merging_respects_the_gap() {
        let stacked = boxes(&[(10, 0, 19, 9), (10, 40, 19, 49)]);
        assert_eq!(merge_along(&stacked, Axis::Vertical, 8).len(), 2, "gap too wide to join");
        assert_eq!(merge_along(&stacked, Axis::Vertical, 40).len(), 1);
    }

    /// Two columns side by side must not merge into one just because they
    /// overlap along the axis being merged; they have to overlap *across* it.
    #[test]
    fn merging_needs_overlap_across_the_axis() {
        let apart = boxes(&[(0, 0, 9, 9), (60, 14, 69, 23)]);
        assert_eq!(merge_along(&apart, Axis::Vertical, 8).len(), 2);
    }

    #[test]
    fn a_merged_box_covers_both() {
        let stacked = boxes(&[(10, 0, 19, 9), (10, 14, 19, 23)]);
        let merged = merge_along(&stacked, Axis::Vertical, 8);
        assert_eq!(merged[0], Bounds { x0: 10, y0: 0, x1: 19, y1: 23 });
    }

    /// Text smears into fewer pieces along its own reading direction, which is
    /// the whole of the axis rule and needs no language knowledge at all.
    #[test]
    fn the_axis_is_the_direction_that_merges_furthest() {
        let column = boxes(&[(10, 0, 19, 9), (10, 14, 19, 23), (10, 28, 19, 37)]);
        assert_eq!(axis_of(&column, 8), Axis::Vertical);

        let line = boxes(&[(0, 10, 9, 19), (14, 10, 23, 19), (28, 10, 37, 19)]);
        assert_eq!(axis_of(&line, 8), Axis::Horizontal);
    }

    /// A single glyph merges to one piece either way. Vertical is the answer
    /// because this project's hub editions are Japanese, and a tie has to go
    /// somewhere rather than varying with iteration order.
    #[test]
    fn a_tie_reads_as_vertical() {
        assert_eq!(axis_of(&boxes(&[(0, 0, 9, 9)]), 8), Axis::Vertical);
    }
```

Add `use taiyaku_core::Axis;` to the test module's imports via the file's own
`use` block in Step 3 — `use super::*` picks it up.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli detect`
Expected: FAIL with `cannot find function stroke_scale in this scope`.

- [ ] **Step 3: Write the implementation**

Add `use taiyaku_core::Axis;` beside the existing `use image::GrayImage;` at the
top of `crates/taiyaku-cli/src/detect.rs`, then append below `components`:

```rust
/// The page's own glyph scale, as the median of its components' longer sides.
///
/// Every gap below is a multiple of this rather than a fraction of the page,
/// and that is the one decision in this file that the corpus forces. Page
/// widths here run from 685 to 5343 pixels for editions of the same work, so a
/// gap expressed as a fraction of the page is eight times too large at one end
/// of that range and eight times too small at the other. Expressed against the
/// page's own strokes, it scales with the type.
///
/// The median rather than the mean, because a panel border is one enormous
/// component and a mean would follow it.
pub fn stroke_scale(strokes: &[Bounds]) -> Option<u32> {
    if strokes.is_empty() {
        return None;
    }
    let mut sides: Vec<u32> = strokes.iter().map(Bounds::longer).collect();
    sides.sort_unstable();
    Some(sides[sides.len() / 2])
}

/// Join boxes that are close along `axis` and overlap across it.
///
/// This is run-length smearing done on boxes rather than on pixels: the classic
/// document-layout method, at a fraction of the memory, because the components
/// are already in hand from `components` and the pixels are not needed again.
///
/// **Overlap across the axis is required, not optional.** Without it two
/// balloons at opposite edges of the page merge whenever their columns happen
/// to line up, which on a two-column page is most of them.
pub fn merge_along(strokes: &[Bounds], axis: Axis, gap: u32) -> Vec<Bounds> {
    // Along-axis extent, across-axis extent. One accessor pair, so the two
    // directions cannot drift apart — the reason `longer`/`shorter` exist too.
    let along = |b: &Bounds| match axis {
        Axis::Vertical => (b.y0, b.y1),
        Axis::Horizontal => (b.x0, b.x1),
    };
    let across = |b: &Bounds| match axis {
        Axis::Vertical => (b.x0, b.x1),
        Axis::Horizontal => (b.y0, b.y1),
    };

    let mut open: Vec<Bounds> = Vec::new();
    let mut order: Vec<usize> = (0..strokes.len()).collect();
    order.sort_by_key(|&i| along(&strokes[i]).0);

    for &i in &order {
        let stroke = strokes[i];
        let (s0, s1) = along(&stroke);
        let (a0, a1) = across(&stroke);
        let joined = open.iter_mut().find(|group| {
            let (g0, g1) = along(group);
            let (b0, b1) = across(group);
            // Close along the axis: either overlapping, or within `gap` of it.
            let near = s0 <= g1.saturating_add(gap) && g0 <= s1.saturating_add(gap);
            let overlaps = a0 <= b1 && b0 <= a1;
            near && overlaps
        });
        match joined {
            Some(group) => *group = group.merge(&stroke),
            None => open.push(stroke),
        }
    }

    // One pass leaves groups that grew towards each other unmerged, because the
    // group a stroke joined may since have reached the group before it. Repeat
    // until nothing changes; the count strictly decreases, so this terminates.
    if open.len() < strokes.len() {
        let again = merge_along(&open, axis, gap);
        if again.len() < open.len() {
            return again;
        }
    }
    open
}

/// Which way the text runs, decided by merging both ways and taking the fewer.
///
/// Text smears into fewer pieces along its own reading direction. That is the
/// whole rule, it needs no knowledge of either language, and it is why nothing
/// here has to be told which edition is the raw. A tie reads as vertical, so a
/// single isolated glyph has a stable answer rather than one that follows
/// iteration order.
pub fn axis_of(strokes: &[Bounds], gap: u32) -> Axis {
    let vertical = merge_along(strokes, Axis::Vertical, gap).len();
    let horizontal = merge_along(strokes, Axis::Horizontal, gap).len();
    match vertical <= horizontal {
        true => Axis::Vertical,
        false => Axis::Horizontal,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli detect`
Expected: PASS, 14 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/detect.rs
git commit -m "detect: a gap measured in the page's own strokes, and an axis that needs no language"
```

---

### Task 4: The page detector

**Files:**
- Modify: `crates/taiyaku-cli/src/detect.rs`
- Modify: `crates/taiyaku-cli/src/fingerprint.rs` (one visibility change)
- Test: same in-file module

**Interfaces:**
- Consumes: everything from Tasks 2 and 3, and `taiyaku_core::Region`.
- Produces: `detect_page(&GrayImage) -> Vec<Region>`, and the constants
  `SMEAR_K`, `MIN_STROKES`, `MAX_BLOCK_AREA`, `MIN_INK`, `MIN_BLOCK_STROKE_SPAN`.
  Also `pub(crate) fn decode_all` in `fingerprint.rs`.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `crates/taiyaku-cli/src/detect.rs`:

```rust
    /// A page with one balloon's worth of vertical text and nothing else.
    fn page_with_a_column() -> GrayImage {
        let marks: Vec<(u32, u32, u32, u32)> = (0..6)
            .map(|i| (100u32, 40 + i * 30, 20, 20))
            .collect();
        GrayImage::from_fn(400, 600, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        })
    }

    #[test]
    fn a_column_of_glyphs_is_one_vertical_region() {
        let found = detect_page(&page_with_a_column());
        assert_eq!(found.len(), 1, "one block, got {found:?}");
        assert_eq!(found[0].axis, Axis::Vertical);
    }

    /// Fractions, not pixels. The corpus holds editions of one work whose page
    /// widths differ by a factor of eight, and a pixel box is meaningless
    /// without the page it came from.
    #[test]
    fn a_region_is_in_page_fractions() {
        let found = detect_page(&page_with_a_column());
        let region = found[0];
        assert!((region.x - 0.25).abs() < 0.01, "x was {}", region.x);
        assert!(region.w > 0.0 && region.w < 1.0);
        assert!(region.y + region.h <= 1.0, "stays on the page");
    }

    /// The same content at twice the size must produce the same fractions.
    /// This is what `stroke_scale` exists for and the cheapest test that can
    /// catch a gap accidentally re-expressed as a fraction of the page.
    #[test]
    fn detection_is_scale_invariant() {
        let small = page_with_a_column();
        let large = GrayImage::from_fn(800, 1200, |x, y| *small.get_pixel(x / 2, y / 2));
        let (a, b) = (detect_page(&small), detect_page(&large));
        assert_eq!(a.len(), b.len(), "same blocks at both scales");
        assert!((a[0].x - b[0].x).abs() < 0.02, "{} vs {}", a[0].x, b[0].x);
        assert!((a[0].h - b[0].h).abs() < 0.02, "{} vs {}", a[0].h, b[0].h);
    }

    #[test]
    fn a_blank_page_has_no_regions() {
        let blank = GrayImage::from_fn(400, 600, |_, _| Luma([255]));
        assert!(detect_page(&blank).is_empty());
    }

    /// A full-page panel border is one enormous, nearly empty component, and it
    /// is the single most common thing on a manga page that is not text.
    #[test]
    fn a_panel_border_is_not_a_region() {
        let bordered = GrayImage::from_fn(400, 600, |x, y| {
            let edge = x < 4 || y < 4 || x >= 396 || y >= 596;
            Luma([if edge { 0 } else { 255 }])
        });
        assert!(detect_page(&bordered).is_empty(), "got {:?}", detect_page(&bordered));
    }

    #[test]
    fn a_lone_speck_is_not_a_region() {
        let speck = GrayImage::from_fn(400, 600, |x, y| {
            Luma([if (200..204).contains(&x) && (300..304).contains(&y) { 0 } else { 255 }])
        });
        assert!(detect_page(&speck).is_empty());
    }

    /// Two balloons a long way apart stay two regions. A detector that merged
    /// them would score well on any metric counting how much text it covered
    /// and be useless for pairing, which is what the whole feature is for.
    #[test]
    fn two_distant_columns_stay_two_regions() {
        let marks: Vec<(u32, u32, u32, u32)> = (0..6)
            .flat_map(|i| [(60u32, 40 + i * 30, 20, 20), (300u32, 40 + i * 30, 20, 20)])
            .collect();
        let page = GrayImage::from_fn(400, 600, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        });
        assert_eq!(detect_page(&page).len(), 2);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli detect`
Expected: FAIL with `cannot find function detect_page in this scope`.

- [ ] **Step 3: Write the implementation**

Add `use taiyaku_core::Region;` to the imports, then append to
`crates/taiyaku-cli/src/detect.rs`:

```rust
/// How far apart two strokes may be, as a multiple of the page's stroke scale.
///
/// **A starting value, not a measurement.** It has to exceed the gap between
/// glyphs in a line and between the lines of one balloon, and stay under the
/// whitespace between two balloons. `scripts/region-survey.sh` is what settles
/// it; until that has run against the corpus, nothing may describe it as
/// measured.
const SMEAR_K: u32 = 2;

/// Fewest strokes a block may hold. **A starting value** (see `SMEAR_K`).
///
/// One stroke is a speck of dust, a screentone dot, or the tail of a balloon.
const MIN_STROKES: usize = 4;

/// Largest a block may be, as a fraction of page area. **A starting value.**
///
/// This is the disqualifier the L2 design names — a detector whose median
/// region exceeds 0.15 of the page is not finding text — set here at the
/// per-block level so such a block never reaches the median in the first place.
const MAX_BLOCK_AREA: f32 = 0.15;

/// Least ink a block may contain, as a fraction of its own area.
/// **A starting value.**
///
/// A panel border encloses a large empty box; text fills its box. This is the
/// one filter that tells those apart, and it is why the strokes' own area is
/// summed rather than the block's counted.
const MIN_INK: f32 = 0.05;

/// Fewest stroke-scales a block must span on its longer side.
/// **A starting value.**
///
/// Expressed in strokes rather than pixels for the reason `stroke_scale`
/// gives: the corpus spans an eightfold range of page widths.
const MIN_BLOCK_STROKE_SPAN: u32 = 2;

/// Every text block on one page, in page fractions.
///
/// Five stages: threshold, label, scale, smear, filter. The filters are what
/// keeps a panel border and a speck of dust out, and each of them is a
/// starting value the corpus survey settles.
pub fn detect_page(page: &GrayImage) -> Vec<Region> {
    let (width, height) = page.dimensions();
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let strokes = components(page, otsu(page));
    let Some(scale) = stroke_scale(&strokes) else {
        return Vec::new();
    };
    let gap = (scale * SMEAR_K).max(1);

    // A block's ink is the area of the strokes inside it, so `MIN_INK` can tell
    // a filled box of type from an empty box with a drawn border.
    let ink_in = |block: &Bounds| -> u64 {
        strokes
            .iter()
            .filter(|s| {
                s.x0 >= block.x0 && s.x1 <= block.x1 && s.y0 >= block.y0 && s.y1 <= block.y1
            })
            .map(|s| u64::from(s.width()) * u64::from(s.height()))
            .sum()
    };
    let strokes_in = |block: &Bounds| -> usize {
        strokes
            .iter()
            .filter(|s| {
                s.x0 >= block.x0 && s.x1 <= block.x1 && s.y0 >= block.y0 && s.y1 <= block.y1
            })
            .count()
    };

    let page_area = f64::from(width) * f64::from(height);
    let axis = axis_of(&strokes, gap);

    merge_along(&strokes, axis, gap)
        .into_iter()
        .filter(|block| strokes_in(block) >= MIN_STROKES)
        .filter(|block| block.longer() >= scale * MIN_BLOCK_STROKE_SPAN)
        .filter(|block| {
            let area = f64::from(block.width()) * f64::from(block.height());
            area / page_area <= f64::from(MAX_BLOCK_AREA)
        })
        .filter(|block| {
            let area = f64::from(block.width()) * f64::from(block.height());
            area > 0.0 && ink_in(block) as f64 / area >= f64::from(MIN_INK)
        })
        .map(|block| Region {
            x: block.x0 as f32 / width as f32,
            y: block.y0 as f32 / height as f32,
            w: block.width() as f32 / width as f32,
            h: block.height() as f32 / height as f32,
            axis,
        })
        .collect()
}
```

Then widen the parallel decode in `crates/taiyaku-cli/src/fingerprint.rs:200` so
detection reuses it rather than carrying a second bounded-decode loop. Change:

```rust
fn decode_all<T, F>(pages: &[Page], describe: F) -> Result<Vec<T>, String>
```

to:

```rust
/// Widened from private for `detect`, which needs the same bounded parallel
/// decode and must not carry a second copy of the large-page rule: the ceiling
/// is `MAX_DECODE_ALLOC` and the corpus holds 430 MB TIFFs, so decoding the
/// large ones one at a time is what keeps a detection run inside memory.
pub(crate) fn decode_all<T, F>(pages: &[Page], describe: F) -> Result<Vec<T>, String>
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli detect`
Expected: PASS, 21 tests.

If `a_column_of_glyphs_is_one_vertical_region` fails with two blocks rather than
one, `SMEAR_K` is too small for the fixture's 10px gaps at a 20px stroke scale;
that is the constant doing its job and the fixture is the thing to check first,
not the constant — the fixture's gaps must be under `SMEAR_K` stroke scales.

- [ ] **Step 5: Leave the `#![allow(dead_code)]` alone — it comes out in Task 5**

*This step said the opposite until Task 4 was run, and the correction is worth
reading.* It claimed `detect_page` was the caller that makes this module live,
so the attribute could come out here. It is not. `detect_page` is **defined**
here and called only from `#[cfg(test)]`; nothing in the binary invokes it until
`regions::command` does, in Task 5. Removing the attribute here leaves thirteen
dead-code errors under `-D warnings`, including `detect_page` itself.

The general shape of the mistake: **defining the top of a stack does not make
the stack live — a caller in the binary does.** The attribute's own comment now
names Task 5.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean, **with** the allow still present.

- [ ] **Step 6: Run the whole suite, to be sure the visibility change broke nothing**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-cli/src/detect.rs crates/taiyaku-cli/src/fingerprint.rs
git commit -m "detect: a page's text blocks, with the panel border and the speck filtered out"
```

---

### Task 5: The `taiyaku regions` command

**Files:**
- Create: `crates/taiyaku-cli/src/regions.rs`
- Modify: `crates/taiyaku-cli/src/main.rs`
- Modify: `crates/taiyaku-cli/src/credits.rs` (its copy of `edition_of_digests`
  moves out — see Step 0)
- Test: in-file `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `detect::detect_page`, `fingerprint::decode_all`,
  `taiyaku_core::{Axis, RegionDoc, RegionEdition, RegionMember, Region, Sidecar}`,
  `taiyaku_read::{read_edition, member_digest}`, `crate::option_value`,
  `crate::reject_unknown_flags`, `crate::edition_of_digests`.
- Produces: `crate::edition_of_digests`, moved from `credits.rs` and now shared.
- Produces: `pub fn command(args: &[String]) -> Result<(), String>`, and
  `fn survey_line(edition: &str, pages: &BTreeMap<u32, Vec<Region>>) -> String`.

- [ ] **Step 0: Share `edition_of_digests` instead of copying it**

Both commands ask the same question — "which edition of this document are these
members?" — and answer it with the same message. A second copy is the
duplication O10 was closed to remove, and that one spanned a crate boundary
where this is inside one.

Cut this function from `crates/taiyaku-cli/src/credits.rs` (it sits at line 237,
above `resolve_counterpart`) and paste it into `crates/taiyaku-cli/src/main.rs`
beside the other shared argument helpers, next to `option_value`, with
`pub(crate)` and this added doc line:

```rust
/// Which edition of `doc` is made of exactly these members, by content.
///
/// Content, not paths: `edition_of` compares the sorted sha256 set, the idiom
/// `merge_spoke`'s own guard states — a renamed but byte-identical archive is
/// still this edition. Shared by `credits` and `regions`, which ask the same
/// question and must not drift on the answer or on how it is worded.
pub(crate) fn edition_of_digests(
    doc: &Sidecar,
    sidecar: &str,
    digests: &[String],
) -> Result<String, String> {
    doc.edition_of(digests).map(str::to_string).ok_or_else(|| {
        format!(
            "no edition of `{sidecar}` is made of these {} members.\nlooked for: {}",
            digests.len(),
            digests.join(", ")
        )
    })
}
```

In `credits.rs`, replace its one call site — inside `command`, currently
`edition_of_digests(&doc, sidecar, &digests)?` — with
`crate::edition_of_digests(&doc, sidecar, &digests)?`.

Run: `cargo test -p taiyaku-cli credits`
Expected: PASS, unchanged. Nothing about the behaviour moved, so a test that
fails here is a transcription error rather than a design question.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-cli/src/regions.rs` containing only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn write_without_a_sidecar_is_refused() {
        let err = command(&args(&["a.cbz", "--write"])).expect_err("refuses");
        assert!(err.contains("--sidecar"), "names what is missing: {err}");
    }

    /// O21: a misspelling used to be accepted in silence by every command, and
    /// the refusal has to name the flags that do exist rather than guess.
    #[test]
    fn an_unknown_flag_is_refused_and_the_real_ones_are_named() {
        let err = command(&args(&["a.cbz", "--sidecar", "t.json", "--writ"]))
            .expect_err("refuses");
        assert!(err.contains("--writ"), "names the typo: {err}");
        assert!(err.contains("--write"), "names the real flag: {err}");
    }

    /// The deviation this plan states, pinned rather than left to the prose:
    /// `--cache` cannot take effect here, so it is refused rather than
    /// accepted and ignored — the rule O19, O20 and O22 each closed a defect
    /// to establish.
    #[test]
    fn cache_is_refused_rather_than_accepted_and_ignored() {
        let err = command(&args(&["a.cbz", "--sidecar", "t.json", "--cache", "x"]))
            .expect_err("refuses");
        assert!(err.contains("--cache"), "names it: {err}");
    }

    /// O15: a flag whose value is another flag used to be taken as absent.
    #[test]
    fn a_flag_with_no_value_is_refused() {
        let err = command(&args(&["a.cbz", "--sidecar", "--write"])).expect_err("refuses");
        assert!(err.contains("--sidecar"), "{err}");
    }

    #[test]
    fn a_run_with_no_member_prints_usage() {
        let err = command(&args(&["--sidecar", "t.json"])).expect_err("refuses");
        assert!(err.contains("taiyaku regions"), "{err}");
    }

    /// Every field a survey collects, in a fixed order, so a table in notes.md
    /// can be rebuilt rather than retyped. The discipline `--stats`,
    /// `duplicates` and `credits` all follow.
    #[test]
    fn the_survey_line_has_every_field_in_a_fixed_order() {
        let mut pages = BTreeMap::new();
        pages.insert(
            0,
            vec![
                Region { x: 0.1, y: 0.1, w: 0.1, h: 0.2, axis: Axis::Vertical },
                Region { x: 0.5, y: 0.1, w: 0.1, h: 0.2, axis: Axis::Vertical },
            ],
        );
        pages.insert(1, vec![Region { x: 0.1, y: 0.1, w: 0.2, h: 0.1, axis: Axis::Horizontal }]);
        let line = survey_line("raw-mono", &pages);
        assert_eq!(
            line,
            "regions: edition=raw-mono pages=2 blocks=3 v=2 h=1 \
             median_area=0.0200 blocks_per_page=1.50"
        );
    }

    /// A survey line for an edition where nothing was found must still be a
    /// well-formed line. An absent row reads as an edition that was not
    /// surveyed, which is the opposite of what it means.
    #[test]
    fn an_edition_with_no_blocks_still_produces_a_line() {
        let mut pages = BTreeMap::new();
        pages.insert(0, Vec::new());
        let line = survey_line("raw-mono", &pages);
        assert!(line.starts_with("regions: edition=raw-mono pages=1 blocks=0"), "{line}");
        assert!(line.contains("median_area=0.0000"), "{line}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli regions`
Expected: FAIL — the module is not declared. Add `mod regions;` to
`crates/taiyaku-cli/src/main.rs` after `mod report;`, then re-run. Expected:
FAIL with `cannot find function command in this scope`.

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/taiyaku-cli/src/regions.rs`:

```rust
//! `taiyaku regions`: propose the text blocks of one edition.
//!
//! The same contract `credits` has, and for the same reason: this is the output
//! of a heuristic, so it proposes and prints, and it writes only under
//! `--write`. It identifies the edition by content — hashing the members it was
//! handed and matching them against the document — rather than being told,
//! which is the rule `verify` established after taking the first alignment of a
//! five-edition document produced a confident report about a pair nobody asked
//! about.
//!
//! **It pairs nothing.** Which block corresponds to which block on another
//! edition is derived at read time from the alignment; see the L2 design, §5.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use taiyaku_core::{Axis, Region, RegionDoc, RegionEdition, RegionMember, Sidecar};

use crate::detect::detect_page;

const USAGE: &str = "\
taiyaku regions <member>... --sidecar FILE [--write]

Propose the text blocks of one edition, page by page. Prints what it found;
writes nothing without --write.

There is no --cache: the fingerprint cache holds hashes and dimensions, not
pixels, and the regions document is what makes a second run cheap.";

/// The median block area as a fraction of the page, and the counts around it.
///
/// The median rather than the mean, because the disqualifier the L2 design
/// states is a median and a single page-sized block would drag a mean past it
/// while every other block was fine.
fn median_area(pages: &BTreeMap<u32, Vec<Region>>) -> f32 {
    let mut areas: Vec<f32> = pages
        .values()
        .flatten()
        .map(|region| region.w * region.h)
        .collect();
    if areas.is_empty() {
        return 0.0;
    }
    areas.sort_by(|a, b| a.partial_cmp(b).expect("areas are finite"));
    areas[areas.len() / 2]
}

/// One fixed-field line for a survey to collect.
fn survey_line(edition: &str, pages: &BTreeMap<u32, Vec<Region>>) -> String {
    let blocks: usize = pages.values().map(Vec::len).sum();
    let vertical = pages
        .values()
        .flatten()
        .filter(|r| r.axis == Axis::Vertical)
        .count();
    let per_page = match pages.is_empty() {
        true => 0.0,
        false => blocks as f32 / pages.len() as f32,
    };
    format!(
        "regions: edition={edition} pages={} blocks={blocks} v={vertical} h={} \
         median_area={:.4} blocks_per_page={per_page:.2}",
        pages.len(),
        blocks - vertical,
        median_area(pages),
    )
}

/// The per-page table a person judges the proposal by.
fn table(pages: &BTreeMap<u32, Vec<Region>>) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("page  blocks  v     h     median area\n");
    for (page, regions) in pages {
        let vertical = regions.iter().filter(|r| r.axis == Axis::Vertical).count();
        let mut one = BTreeMap::new();
        one.insert(*page, regions.clone());
        let _ = writeln!(
            out,
            "{page:>4}  {:>6}  {vertical:<4}  {:<4}  {:.4}",
            regions.len(),
            regions.len() - vertical,
            median_area(&one),
        );
    }
    out
}

/// Read the document beside the sidecar, or start an empty one.
///
/// A document that will not parse is an error rather than an empty start: this
/// command rewrites one edition's entry and leaves the others alone, so
/// silently starting fresh would delete every other edition's regions.
fn load_beside(path: &Path) -> Result<RegionDoc, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => RegionDoc::from_json(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RegionDoc::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn command(args: &[String]) -> Result<(), String> {
    // No `--cache`, deliberately: a flag that cannot take effect is refused
    // rather than accepted (O19, O20, O22), the fingerprint cache holds hashes
    // and dimensions rather than pixels, and the regions document is already
    // the durable artefact a second cache would duplicate.
    const KNOWN: &[&str] = &["--sidecar", "--write"];
    crate::reject_unknown_flags("regions", args, KNOWN)?;

    // Every valued flag resolves before a member is opened, so a typo does not
    // cost the seconds a decode pass takes.
    let sidecar = crate::option_value(args, "--sidecar", "a path to a taiyaku.json")?;
    let write = args.iter().any(|a| a == "--write");

    if write && sidecar.is_none() {
        return Err(
            "--write needs --sidecar: the regions document is named after the sidecar, \
             and there is nothing to name it after"
                .into(),
        );
    }

    let members: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| Some(a.as_str()) != sidecar)
        .map(PathBuf::from)
        .collect();
    if members.is_empty() {
        return Err(USAGE.into());
    }

    // Refused rather than ignored, the rule `align` applies to every flag that
    // cannot take effect. Regions are recorded against an edition, an edition
    // is identified by content against a document, and without the document
    // there is nothing to identify against.
    let Some(sidecar) = sidecar else {
        return Err(
            "regions needs --sidecar: blocks are recorded against an edition, and the \
             sidecar is what says which edition these members are"
                .into(),
        );
    };

    // Read and parse before a single page is decoded. A mistyped path is the
    // same class of mistake as a wrong flag and belongs on the same side of the
    // cost line.
    let text = std::fs::read_to_string(sidecar).map_err(|e| format!("{sidecar}: {e}"))?;
    let doc = Sidecar::from_json(&text)?;

    let mut digests = Vec::new();
    for member in &members {
        digests.push(taiyaku_read::member_digest(member)?);
    }
    let edition = crate::edition_of_digests(&doc, sidecar, &digests)?;

    let pages = taiyaku_read::read_edition(&members)?;
    let detected = crate::fingerprint::decode_all(&pages, |_, luma| detect_page(luma))?;

    let found: BTreeMap<u32, Vec<Region>> = detected
        .into_iter()
        .enumerate()
        .map(|(index, regions)| (index as u32, regions))
        .collect();

    eprint!("{}", table(&found));

    if write {
        let path = RegionDoc::path_beside(Path::new(sidecar));
        let mut document = load_beside(&path)?;
        // Wholesale for this edition, and untouched for every other: the field
        // holds no hand entries yet, so a re-run after a constant changes must
        // leave nothing stale behind — and must not take the neighbours with it.
        document.editions.insert(
            edition.clone(),
            RegionEdition {
                members: digests.iter().map(|sha256| RegionMember { sha256: sha256.clone() }).collect(),
                pages: found.clone(),
            },
        );
        taiyaku_serve::atomic::write(&path, document.to_json().as_bytes(), "json")?;
        let blocks: usize = found.values().map(Vec::len).sum();
        eprintln!("wrote {blocks} block(s) for `{edition}` to {}", path.display());
    }

    // Printed only once whatever `--write` promised has happened, so a failed
    // run never leaves a well-formed `regions:` line on stdout for a survey to
    // collect. The same rule `credits` states at its own final line.
    println!("{}", survey_line(&edition, &found));

    Ok(())
}
```

Then wire it into `crates/taiyaku-cli/src/main.rs`:

Add the dispatch arm after the `credits` arm at line 58:

```rust
        Some("regions") => regions::command(&args[1..]),
```

Add to the usage block, after the `credits` line at line 77:

```rust
            println!("       taiyaku regions <member>... --sidecar FILE [--write]");
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli regions`
Expected: PASS, 7 tests.

- [ ] **Step 5: Drive the command's own refusals by hand**

Run: `cargo run -q -p taiyaku-cli -- regions`
Expected: the usage text, exit 1.

Run: `cargo run -q -p taiyaku-cli -- regions a.cbz --writ`
Expected: ``regions has no flag `--writ`; its flags are --sidecar, --write``.

Run: `cargo run -q -p taiyaku-cli -- regions a.cbz --cache .taiyaku-cache`
Expected: the same shape of refusal, naming `--cache`. This is the deviation
above being enforced rather than merely written down.

Run: `cargo run -q -p taiyaku-cli -- | grep regions`
Expected: the new usage line appears in the command list.

- [ ] **Step 6: Delete the temporary `#![allow(dead_code)]` from `detect.rs`**

Tasks 2, 3 and 4 carried an inner attribute at the very top of
`crates/taiyaku-cli/src/detect.rs`, above its `//!` block, with a comment
explaining itself. It is there because `taiyaku-cli` is a **binary** crate: a
private module's items are dead until something *in the binary* calls them, and
the gate is `-D warnings`.

`regions::command` is that caller — this task is where `detect_page` first
reaches the binary rather than only `#[cfg(test)]`. Delete the attribute and its
whole comment block.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean, **without** the allow.

If it is not clean, something in `detect.rs` genuinely has no caller. That is a
real finding — report it rather than putting the attribute back. Task 4 tried to
remove this one task too early and reported exactly that, which is why the step
is here and not there.

- [ ] **Step 7: Run the whole gate**

Run: `./scripts/check-all.sh`
Expected: PASS. It mirrors all four CI jobs, and the two that a new command most
often breaks are the format check and clippy.

- [ ] **Step 8: Commit**

`detect.rs` is staged here too, because Step 6 removed its `allow` — the
attribute and the caller that retires it belong in one commit.

```bash
git add crates/taiyaku-cli/src/regions.rs crates/taiyaku-cli/src/main.rs \
        crates/taiyaku-cli/src/credits.rs crates/taiyaku-cli/src/detect.rs
git commit -m "regions: a ninth command that proposes text blocks and writes only when asked"
```

---

### Task 6: The survey, and settling the constants

**Files:**
- Create: `scripts/region-survey.sh`
- Modify: `crates/taiyaku-cli/src/detect.rs` (the five constants, once the
  survey has produced numbers)
- Modify: `docs/notes.md` (the survey's table and what it settled)

**Interfaces:**
- Consumes: `taiyaku regions` from Task 5.
- Produces: the settled values of `SMEAR_K`, `MIN_STROKES`, `MAX_BLOCK_AREA`,
  `MIN_INK` and `MIN_BLOCK_STROKE_SPAN`, and a `notes.md` section recording
  what each was measured against.

- [ ] **Step 1: Write the survey script**

Create `scripts/region-survey.sh`:

```bash
#!/usr/bin/env bash
# Collect one `regions:` line per surveyable edition, for a table in
# docs/notes.md.
#
#   scripts/region-survey.sh corpus
#
# `taiyaku regions` prints its per-page table to stderr and one fixed-field
# `regions:` line to stdout, so this script can let both through: the tables are
# what a person judges a proposal by, and the lines are the table this survey
# exists to produce. Redirect stdout alone to keep the lines without the tables.
#
# **The constants in crates/taiyaku-cli/src/detect.rs are not chosen from this
# output alone.** They are chosen from this output *and* from drawing the
# proposed boxes over the pages and looking at them. What this script gives you
# is which editions to look at, and the reproducibility that the table in
# notes.md can be rebuilt rather than retyped.
#
# Every edition of every work is surveyed, not only the raws: the axis split is
# the cheapest check that detection is working at all, and it only means
# something if a vertical edition and a horizontal one are both in the table.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

root="${1:?usage: region-survey.sh <corpus root>}"
[ -d "$root" ] || { echo "error: $root is not a directory" >&2; exit 2; }

taiyaku=(cargo run --quiet --release -p taiyaku-cli --)

# No --cache is passed, and that is not an omission: `regions` has no such flag.
# It decodes pixels, and the fingerprint cache holds hashes and dimensions.
surveyed=0
skipped=()

# A plain assignment rather than `mapfile < <(find ...)`, for the reason
# duplicate-survey.sh gives at length: process substitution hides find's exit
# status from `set -o pipefail`, so a partial listing would survey some of the
# corpus under a script that looks like it aborts on error.
listing="$(find "$root" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)"

while IFS= read -r name; do
  [ -n "$name" ] || continue
  work="$root/$name"

  if [ ! -f "$work/taiyaku.json" ]; then
    skipped+=("$name (no taiyaku.json to identify an edition against)")
    continue
  fi

  # Every work that cannot be surveyed is named, never silently dropped. A work
  # missing from the output is indistinguishable from a work with nothing to
  # report, and that silence is what this repository is written against.
  editions="$(find "$work" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)"
  [ -n "$editions" ] || { skipped+=("$name (no edition directories)"); continue; }

  while IFS= read -r edition; do
    [ -n "$edition" ] || continue
    members=()
    for archive in "$work/$edition"/*.cbz; do
      [ -f "$archive" ] && members+=("$archive")
    done
    if [ "${#members[@]}" -eq 0 ]; then
      skipped+=("$name/$edition (holds no .cbz)")
      continue
    fi

    echo "== $name: $edition, ${#members[@]} member(s)" >&2
    # `|| true` on the grep, deliberately: `regions` prints its line last and
    # only after --write has done whatever it promised, so a run that failed
    # partway leaves nothing to match — and a grep matching nothing exits 1,
    # which under `set -e` would abort this assignment and take the named error
    # below with it.
    line="$("${taiyaku[@]}" regions "${members[@]}" \
      --sidecar "$work/taiyaku.json" | grep '^regions:' || true)"

    # Checked rather than assumed: an empty row in the collected table reads as
    # an edition with nothing to report, which is the opposite of what it means.
    [ -n "$line" ] || {
      echo "error: $name/$edition produced no regions: line; the run above failed" >&2
      exit 3
    }
    printf '%s %s\n' "work=$name" "$line"
    surveyed=$((surveyed + 1))
  done <<< "$editions"
done <<< "$listing"

for name in "${skipped[@]:-}"; do
  [ -n "$name" ] && echo "== skipped: $name" >&2
done

# A survey that found nothing is a broken invocation — a wrong root, most
# likely — and not a corpus with no editions in it. Exiting 0 with an empty
# table would put that mistake into notes.md as a measurement.
[ "$surveyed" -gt 0 ] || {
  echo "error: no surveyable edition under $root; every work was skipped above" >&2
  exit 4
}

echo "region-survey: $surveyed edition(s) surveyed, ${#skipped[@]} skipped" >&2
```

- [ ] **Step 2: Make it executable and check it refuses a bad invocation**

```bash
chmod +x scripts/region-survey.sh
./scripts/region-survey.sh /nonexistent; echo "exit $?"
```

Expected: `error: /nonexistent is not a directory`, exit 2.

- [ ] **Step 3: Run the survey against the corpus**

```bash
./scripts/region-survey.sh "$TAIYAKU_CORPUS" > /tmp/claude-1000/regions-survey.txt
```

Expected: one `regions:` line per edition, and a count on stderr. If
`TAIYAKU_CORPUS` is not set or the corpus is absent, stop here and report that
the survey could not run — do **not** settle the constants from the synthetic
fixtures, and do not describe any constant as measured.

- [ ] **Step 4: Read the numbers and settle the five constants**

Judge each against what the L2 design fixed:

- **`median_area` must sit well under 0.15** on every edition. At or above it,
  the detector is merging balloons; lower `SMEAR_K` first.
- **`blocks_per_page` must sit above 2** on ordinary volumes. Below it, the
  detector is missing text; raise `SMEAR_K` or lower `MIN_STROKES`.
- **The `v`/`h` split must follow the edition.** A Japanese raw coming back
  mostly `h`, or an English edition mostly `v`, means `axis_of` is wrong and is
  a defect rather than a constant to tune.

Then draw the boxes over a handful of pages and look at them. The survey says
which editions to look at; it cannot say whether a box is on the text.

Update each constant in `crates/taiyaku-cli/src/detect.rs` and rewrite its
doc comment: replace "**A starting value, not a measurement**" with what it was
measured over — how many editions, how many pages, and what moved when it
changed. A constant still calling itself a starting value after this step is
either untouched by the survey or a lie, and both need saying.

- [ ] **Step 5: Record the survey in `docs/notes.md`**

Add a section titled "The text-block survey, and what the five constants cost",
holding: the collected table, the value chosen for each constant, the
alternative it beat, and — stated plainly, the way the credits survey stated its
own false positive — every edition where the detector is visibly wrong. A survey
that reports only its successes is what `MAX_CREDIT_RUN`'s entry exists to warn
against.

- [ ] **Step 6: Re-run the suite with the settled constants**

Run: `cargo test --workspace`
Expected: PASS. The Task 4 fixtures are built at a 20px stroke scale with 10px
gaps, so a `SMEAR_K` settled below 1 would fail them — which is the fixture
catching a constant that cannot be right rather than an unrelated breakage.

- [ ] **Step 7: Commit**

```bash
git add scripts/region-survey.sh crates/taiyaku-cli/src/detect.rs docs/notes.md
git commit -m "regions: the survey, and five constants that are now measured rather than guessed"
```

---

### Task 7: The documentation the earlier tasks made stale

**Files:**
- Modify: `docs/sidecar.md`
- Modify: `docs/status.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: nothing code depends on.

- [ ] **Step 1: Correct `docs/sidecar.md`'s forward-compatibility line**

The section currently says "L1 **may** add a per-run or per-page transform, L2
adds matched text regions". Correct it in place — **do not delete it** — in the
shape the L1 correction beneath it already uses ("*Corrected on 2026-08-08, when
L1 shipped.*"): L2 as built adds no key to this document at all. Regions are a
sibling file on the sidecar's stem, and the pairing between two editions' regions
is not stored anywhere, on the grounds §5 of the L2 design gives. The permission
this section grants is unchanged and is exactly why the sibling file needed no
version bump.

- [ ] **Step 2: Correct `docs/status.md`'s "Later" entry — and leave L2 in it**

*Rewritten after Task 6. The original said to move L2 out of "Later"; that
assumed a detector that works, and the survey says it does not.*

The line reads:

> - **L2 text regions** — bubble pairing by transformed centroid, which L1's
>   `mapRect` now makes arithmetic rather than a matching problem.

**L2 stays under "Later".** Correct the line in place — the way L1's entry is
kept and corrected at the foot of that list — recording two things it got
wrong: a region is a **text block** rather than a bubble, and the line is silent
about **detection**, which is the larger half of the work and the half that has
now been tried and does not yet work.

Note beside it what L2a did land — the format, the ninth command, the survey —
and that the detector behind it does not clear the L2 design's own bar. The
evidence is `notes.md`, "The text-block detector, measured against four works,
and why it does not work yet"; do not restate its figures here beyond the one
that carries the point.

- [ ] **Step 2a: Add the detector to "What is known to be wrong or missing"**

That section exists for exactly this — things "stated plainly because each is a
thing a reader would hit". Add an entry saying `taiyaku regions` proposes text
blocks and does not yet find them reliably: `blocks_per_page` fails the design's
own bar on all nine editions surveyed, and the cause is measured rather than
guessed — `stroke_scale` is set by the sea of sub-glyph screentone and print
noise that dominates a manga page's ink components, 63% to 96% of which are
under 5 pixels, so the smear gap collapses to a few pixels and dense screentone
still welds into one page-sized blob that `MAX_BLOCK_AREA` discards.

Say what would change it, in the shape that section's other entries use: the
scale wants computing over a population that excludes sub-glyph noise, and no
value of the five constants reaches that, so this is a design change rather than
a tuning exercise.

**The five constants' doc comments must keep saying "a starting value, not a
measurement."** They are still not measured, and this task must not imply the
survey settled them.

- [ ] **Step 3: Update `docs/status.md`'s counts and command list**

Four edits:
- The command block near line 135 gains a ninth line:
  `taiyaku regions <member>... --sidecar FILE [--write]`
- "Eight commands:" becomes "Nine commands:".
- The test count in "What is built" — re-read it from `cargo test --workspace`
  output rather than adding the new tests to the old number by hand.
- The O11 entry gains a sentence: **no third cache joins `.taiyaku-cache/`.**
  `regions` writes none and takes no `--cache`; the regions document is the
  durable artefact, and a cache keyed on the same hash holding the same boxes
  would be the duplication O10 was closed to remove. Say it there, because that
  entry is where someone reading about that tree will look, and because O11's
  own reopening condition is "a second consumer of that directory" — this is a
  feature that looked like one and is not.

- [ ] **Step 4: Update `README.md`'s command list**

Add `regions` alongside the other eight, in the same shape the others use.

- [ ] **Step 5: Check the docs against the code rather than against memory**

Run: `cargo run -q -p taiyaku-cli -- | head -20`
Expected: the usage output matches the command list now written in both
`README.md` and `docs/status.md`, flag for flag. This is the check that caught a
command in `README.md` that no longer ran (#27).

- [ ] **Step 6: Commit**

```bash
git add docs/sidecar.md docs/status.md README.md
git commit -m "docs: the ninth command, and the two L2 lines that prejudged what it would be"
```

---

---

### Task 8: The diagnostic, restored as a gated experiment

*Added after the survey. The diagnosis that explains why the detector fails was
a throwaway program, discarded once `notes.md` was written — so the nine
`regions:` lines are reproducible and the eight-page diagnosis behind them is
not, which is the wrong way round: the diagnosis is what makes the negative
result a finding rather than a symptom.*

**Files:**
- Create: `crates/taiyaku-cli/src/region_experiment.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (one `#[cfg(test)] mod` line)

**The precedent is `crates/taiyaku-cli/src/geometry_experiment.rs`** and it
should be followed closely, including the part that is easy to skip. That module
is L1's one-off measurement, `#[cfg(test)]` so no part of it enters the binary,
gated on `TAIYAKU_CORPUS`, and **it asserts only that it measured something** —
a non-empty sample, every figure finite — so that a glob matching no volume
cannot print an empty table and pass, which is the one failure a measurement
must not have. `check-all.sh` filters on `corpus::`, so a module named
`region_experiment` is not picked up by the twelve standing checks and the count
of those stays twelve.

- [ ] **Step 1: Read `geometry_experiment.rs` first** — its gating, its
  skip-when-absent behaviour, and the shape of its "it measured something"
  assertions. Match them rather than inventing a second convention.

- [ ] **Step 2: Write the measurement.** For each sampled page it prints, in one
  row: total components, the fraction under 5px, `stroke_scale`, the derived
  gap, how many blocks survive each of the four filters **in order**, and the
  largest block `MAX_BLOCK_AREA` discarded as a fraction of the page. Those are
  the columns `notes.md`'s table already has, so the table can be rebuilt rather
  than retyped.

- [ ] **Step 3: Call `detect.rs`'s own functions, never copies of them.** The
  discarded program copied them verbatim and cross-checked its block counts
  against `taiyaku regions`' output to show the copy was faithful. Calling the
  real ones removes the need for that check and the drift it was guarding
  against. Everything needed is reachable within the crate.

- [ ] **Step 4: Assert only that it measured something.** A non-empty sample,
  every figure finite, every page accounted for. It must not assert a
  `blocks_per_page` figure or any other quality bar — those move when the
  detector changes, and a measurement that fails when the thing it measures
  improves is a check pretending to be a measurement.

- [ ] **Step 5: Run it** with `TAIYAKU_CORPUS="$PWD/corpus"` — absolute, per O30
  — and confirm the figures reproduce `notes.md`'s table. Where they differ,
  the table is what changes, and say so.

- [ ] **Step 6: Point `detect.rs` at it.** That module's header still says the
  constants "are settled by `scripts/region-survey.sh`" and `SMEAR_K` still says
  "until that has run against the corpus, nothing may describe it as measured".
  The survey has run and found they cannot be settled by tuning. `detect.rs` is
  the file the next person starts from and is currently the one place on this
  branch where the negative result is absent.

- [ ] **Step 7: Commit.**

---

### Task 9: The scale, computed over glyphs rather than over screentone

*The measured cause: `stroke_scale` is the median longer side over every ink
component, 63–96% of which are under 5 pixels, so the scale lands at 1–3px on
pages 685–2147px wide and dense screentone welds into a page-sized blob that
`MAX_BLOCK_AREA` discards.*

**The acceptance rule is fixed here, before any number exists**, which is the
discipline §6 of the design states and the reason the survey did not tune
anything.

| Candidate | The scale is |
| --- | --- |
| `above-floor` | the median longer side over components above a noise floor derived from the page's own distribution |
| `modal` | the mode of the component-size distribution rather than its median |
| **`all` (baseline)** | **today's behaviour — the median over every component** |

**A candidate is promoted only if, measured over the same editions the survey
covered, it beats `all` on both of these:**

- **`blocks_per_page` clears 2** — the design's own bar, failed today on all
  nine editions, best 1.86.
- **The v/h split follows the edition** on both editions of at least one work —
  a Japanese raw majority-vertical and an English edition majority-horizontal,
  the same volume, which no edition manages today.

**`median_area` is deliberately not a criterion, and the reason is a defect the
whole-branch review found.** `MAX_BLOCK_AREA` discards anything above 0.15
*before* a region is constructed, and `median_area` is computed over what
survives — so that disqualifier cannot fire and passing it has never been
evidence of anything. If a block-size health check is wanted it must be taken
*before* the filter, not after.

**If no candidate clears both, none ships**, `all` stays, and the result is
written up in `notes.md` rather than retried until it passes.

- [ ] **Step 1: Implement the candidates behind Task 8's experiment**, so all
  three are measured by the same instrument over the same pages.
- [ ] **Step 2: Measure**, and record the table.
- [ ] **Step 3: Apply the rule.** If one wins, change `stroke_scale` and rerun
  `scripts/region-survey.sh` over the same four works to confirm the survey
  agrees with the experiment. If none wins, change nothing.
- [ ] **Step 4: Update the record either way** — `notes.md` with the table and
  the outcome, and `status.md`'s "What is known to be wrong or missing" entry,
  which currently says the fix is a design change nobody has tried.
- [ ] **Step 5: The constants' doc comments stay "a starting value, not a
  measurement"** unless this task actually measures them, which it does not: it
  measures the *scale rule*, not the five thresholds built on top of it.
- [ ] **Step 6: Commit.**

---

## What L2a deliberately leaves undone

Named here so the next plan does not have to rediscover them, and so that none
of them reads as an oversight:

- **No pairing.** `taiyaku_core::regions` gains no pairing rule in this plan,
  and neither does the wasm binding. That is L2b, along with the experiment that
  picks between `contain`, `nearest`, `contain-then-nearest` and the
  `blind-crop` baseline.
- **No standing corpus check.** L2a ships a survey, which is a measurement; the
  standing `corpus::` check is L2b's, because what it would guard is the pairing
  rule. Adding one here would pin the detector's output to itself.
- **No server route and no reader.** `<stem>.regions.json` is written and never
  read back by anything but this command's own `--write` merge. That is L2c.
- **No hand correction.** A wrong box cannot yet be fixed by a reader. The
  document is shaped for it — it is a whole-file read and write, like notes —
  but nothing writes one by hand.
- **No detection cache, and no `--cache` to name one.** The regions document is
  what makes a second run cheap. If Task 6's survey shows a full-corpus run slow
  enough to matter, a cache can be added then, against a number rather than
  against the assumption that decoding is expensive because it is expensive
  elsewhere.
