# L2 Phase 3 — the model candidate, scored — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce the head-to-head number phase 3 exists for — the promoted classical detector and comic-text-detector's block head, on the same 45 labelled pages, through the same scorer, at the same coverage.

**Architecture:** One new `#[cfg(test)]` module, `crates/taiyaku-cli/src/model_experiment.rs`, in the shape `band_sweep.rs` already established: gated on `TAIYAKU_CORPUS`, reading pages through `score::resolve_labelled` so it measures exactly the pages `taiyaku score` scores, scoring through `score::score_page` and `score::Report` at `score::COVERAGE`, and asserting only that it measured something. It runs both detectors inside one `decode_all` pass so the two rows are the same decode. `tract-onnx` becomes a dev-dependency; the weights are fetched by a script, checksummed, and never committed.

**Tech Stack:** Rust (workspace, `cargo`), `tract-onnx` 0.23.4 (pure Rust ONNX inference, no native library), `image`, `rayon` via `decode_all`.

## Global Constraints

- **`#[cfg(test)]` only.** No part of this module reaches the `taiyaku` binary. `detect_page` is not touched.
- **Gated on `TAIYAKU_CORPUS`**, and additionally on `TAIYAKU_MODEL`. Both skip cleanly with a message when unset — CI has neither and must stay green.
- **Assert that it measured something, never a quality figure.** A measurement that fails when the thing it measures improves is a check pretending to be a measurement. Copy `band_sweep.rs`'s assertion set exactly: every named edition scored, `report.pages == 45`, `by_kind` non-empty.
- **One scorer.** `score::score_page`, `score::Report`, `score::COVERAGE` (`0.5`). A second scorer is two chances to disagree while both print plausible numbers.
- **The weights are never committed.** 94,669,756 bytes, sha256 `1a86ace74961413cbd650002e7bb4dcec4980ffa21b2f19b86933372071d718f`, fetched from the `zyddnys/manga-image-translator` `beta-0.2.1` release — **not** from the Hugging Face mirror, which relabels the same bytes `apache-2.0`.
- **Attribution is a licence obligation, not a courtesy.** Manga109-s requires that "the use of the Manga109-s dataset must be indicated clearly within the published work". Anything this project publishes carrying these weights must say so.
- **The 30 held-out pages stay sealed.** They are not opened by this plan.
- **`--release` is not optional in practice.** `detect_page` runs a flood fill over every pixel of a full-resolution scan, and the model runs a 1024×1024 forward pass, once per labelled page each.
- **Clippy:** the blocking gate is `cargo clippy --workspace --all-targets --all-features -- -D warnings`. `clippy::pedantic` is advisory only. This module carries `#[allow(clippy::pedantic, reason = ...)]` exactly as `region_experiment.rs` does.

---

### Task 1: The dependency, the fetch script, and the ignore rule

**Files:**
- Modify: `crates/taiyaku-cli/Cargo.toml` (the `[dev-dependencies]` table)
- Create: `scripts/fetch-model.sh`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: nothing.
- Produces: `tract_onnx` available to `#[cfg(test)]` code in `taiyaku-cli`; `scripts/fetch-model.sh` writing a verified `models/comictextdetector.pt.onnx`.

- [ ] **Step 1: Add the dev-dependency**

In `crates/taiyaku-cli/Cargo.toml`, the `[dev-dependencies]` table currently reads:

```toml
[dev-dependencies]
zip = "7.2.0"
```

Replace it with:

```toml
# `tract-onnx` is here for `model_experiment.rs` alone — phase 3's measurement —
# and a dev-dependency is what keeps it out of the `taiyaku` binary while
# leaving the module inside the workspace, compiled by CI, and unable to rot.
#
# It is not free: CI has no cargo cache (see `.forgejo/workflows/ci.yml`), so
# both the clippy job and the test job compile it from scratch every run —
# measured at 21 s on 32 cores with a warm registry, and more on a small
# container, against `timeout-minutes: 10` on each. That cost was accepted
# rather than hidden behind an optional feature, because dodging it would mean
# dropping `--all-features` from two CI invocations and leaving this module
# never compiled by the gate.
[dev-dependencies]
tract-onnx = "0.23.4"
zip = "7.2.0"
```

- [ ] **Step 2: Verify the workspace still builds and the suite still passes**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no warnings, exit 0. (First run compiles tract — allow a few minutes.)

Run: `env -u TAIYAKU_CORPUS cargo test --workspace`
Expected: PASS, the existing 727 tests.

- [ ] **Step 3: Write the fetch script**

Create `scripts/fetch-model.sh`:

```bash
#!/usr/bin/env bash
#
# Fetch comic-text-detector's ONNX weights for the L2 phase-3 measurement.
#
# The weights are **not committed**: 95 MB in every clone forever, for a module
# that is `#[cfg(test)]` and skips unless you ask for it. See
# `docs/superpowers/specs/2026-08-11-l2-detector-design.md` §4.2.
#
# --- where these come from, and why not from the obvious place ---------------
#
# From the `zyddnys/manga-image-translator` release, which is a GPL-3.0 project.
# The Hugging Face mirror `mayocream/comic-text-detector-onnx` serves the
# **same bytes** — verified, identical sha256 — while tagging them `apache-2.0`.
# It is the first hit when you search for this model and it is mislabelled, so
# the authoritative source is named here explicitly.
#
# --- the licence obligation this creates -------------------------------------
#
# One third of the training data is Manga109-s, the 87-volume commercially
# approved subset of Manga109 (**not** the academic-only Manga109 whose "solely
# for academic purposes" clause a reader meets first on that site). Its terms
# permit publishing results "including pre-trained models" on one condition:
#
#   "the use of the Manga109-s dataset must be indicated clearly within the
#    published work"
#
# So anything this project publishes carrying these weights must say so. See
# `docs/notes.md`, "comic-text-detector through `tract`".
#
# --- the checksum -------------------------------------------------------------
#
# A GitHub release asset can be replaced in place by whoever owns the
# repository, and upstream publishes no checksum of its own. The digest below
# was computed here from the artefact this measurement was taken against. Bump
# it together with the URL, by downloading and re-computing — never by copying a
# value out of a README or an issue. This is the same rule the nextest download
# in `.forgejo/workflows/ci.yml` follows, for the same reason.

set -euo pipefail

URL="https://github.com/zyddnys/manga-image-translator/releases/download/beta-0.2.1/comictextdetector.pt.onnx"
SHA256="1a86ace74961413cbd650002e7bb4dcec4980ffa21b2f19b86933372071d718f"
DEST="${1:-models/comictextdetector.pt.onnx}"

if [ -f "$DEST" ] && echo "$SHA256  $DEST" | sha256sum -c - >/dev/null 2>&1; then
  echo "taiyaku: $DEST is already present and verified"
else
  mkdir -p "$(dirname "$DEST")"
  echo "taiyaku: fetching 95 MB from the manga-image-translator release..."
  curl -fsSL -o "$DEST" "$URL"
  echo "$SHA256  $DEST" | sha256sum -c -
fi

cat <<EOF

taiyaku: verified. To run the phase-3 measurement:

  TAIYAKU_CORPUS="\$PWD/corpus" TAIYAKU_MODEL="\$PWD/$DEST" \\
    cargo test --release -p taiyaku-cli model_experiment -- --nocapture

EOF
```

Then: `chmod +x scripts/fetch-model.sh`

- [ ] **Step 4: Add the ignore rule**

In `.gitignore`, immediately after the `spike/models/` and `spike/out/` block added on 2026-08-12, add:

```gitignore
# Model weights for the L2 phase-3 measurement, fetched by
# scripts/fetch-model.sh and verified against a recorded sha256. 95 MB, from a
# GPL-3.0 project whose training data may not be redistributed. Never committed.
models/
```

- [ ] **Step 5: Run the script and verify it**

Run: `./scripts/fetch-model.sh`
Expected: `models/comictextdetector.pt.onnx` written, `sha256sum -c` prints `OK`, and the usage block prints.

Run it a second time.
Expected: `taiyaku: models/comictextdetector.pt.onnx is already present and verified` — no second download.

Run: `git status --short`
Expected: `models/` does not appear.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/Cargo.toml Cargo.lock scripts/fetch-model.sh .gitignore
git commit -m "Phase 3's dependency and its weights: tract as a dev-dependency, fetched and checksummed, never committed"
```

---

### Task 2: The letterbox, ported from the ONNX branch

**Files:**
- Create: `crates/taiyaku-cli/src/model_experiment.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (one gated `mod` declaration, beside `truth_population`)

**Interfaces:**
- Consumes: `tract_onnx` from Task 1.
- Produces: `const SIDE: usize`, `fn letterbox(page: &GrayImage) -> (Tensor, usize, usize)` returning the `1×3×SIDE×SIDE` input tensor and the right/bottom padding `(dw, dh)`.

- [ ] **Step 1: Declare the module**

In `crates/taiyaku-cli/src/main.rs`, find the existing block:

```rust
// Phase 1 of the detector redesign: what text is made of, measured from the
// boxes a person drew. Gated on TAIYAKU_CORPUS exactly as `corpus`,
// `geometry_experiment` and `region_experiment` are, and declared the same
// way, so no part of it is compiled into the binary.
#[cfg(test)]
mod truth_population;
```

Immediately after it, add:

```rust
// Phase 3 of the detector redesign: the model candidate, scored against the
// same 45 pages as phase 2's sweep. Gated the same way and for the same
// reason, and additionally on TAIYAKU_MODEL, because the weights are 95 MB and
// are fetched rather than committed.
#[cfg(test)]
#[allow(
    clippy::pedantic,
    reason = "one-off measurement; see docs/ci.md and region_experiment.rs"
)]
mod model_experiment;
```

- [ ] **Step 2: Write the failing test**

Create `crates/taiyaku-cli/src/model_experiment.rs` with the header, the constants, and the first test:

```rust
//! Phase 3: the model candidate, scored against the boxes a person drew.
//!
//! The deliverable is a head-to-head table — the promoted classical detector
//! and comic-text-detector's block head, on the same 45 pages, through the same
//! scorer, at the same coverage. Whether the model is promoted is a decision a
//! person makes reading that table against the rule; nothing here asserts it.
//!
//! **The shape is `band_sweep.rs`'s**, deliberately, rather than a fourth
//! convention: `#[cfg(test)]`, gated on `TAIYAKU_CORPUS`, skipping cleanly when
//! unset, reading its pages through `score::resolve_labelled` so it measures
//! **exactly the pages `taiyaku score` scores**, scoring through
//! `score::score_page` and `score::Report` at `score::COVERAGE`, and asserting
//! only that it measured something.
//!
//! **What the two rows are, stated precisely.** The classical row is
//! `detect::detect_page` as it ships today — the candidate promoted by L2b, not
//! the incumbent. The incumbent's 2-of-478 cannot be reproduced here: three
//! changes separate it from today's detector (the band, the two-axis union,
//! `MIN_INK` counting pixels), and `band_sweep.rs`'s header already records why
//! its `lo = 0` row is not byte-for-byte that detector either. The incumbent's
//! figure is quoted from `docs/status.md`, not recomputed.
//!
//! **Pages arrive as `GrayImage` and the model wants three channels.**
//! `decode_all` decodes to luma, so the three input channels are filled with
//! the same value. Every one of the nine labelled editions is a mono scan, so
//! nothing is lost; on a colour edition this would be a real difference and it
//! is written down rather than left to be discovered.
//!
//! **The preprocessing is a port of the ONNX branch of upstream's pipeline, not
//! the torch branch**, and the two disagree. See `docs/notes.md`,
//! "comic-text-detector through `tract`", for both hazards: the letterbox is
//! **not centred** (the page goes top-left, padding on the bottom and right,
//! upstream's centring lines commented out), and the torch branch reverses the
//! channel axis a second time and feeds BGR while the ONNX branch feeds RGB.
//!
//! ```sh
//! ./scripts/fetch-model.sh
//! TAIYAKU_CORPUS="$PWD/corpus" TAIYAKU_MODEL="$PWD/models/comictextdetector.pt.onnx" \
//!   cargo test --release -p taiyaku-cli model_experiment -- --nocapture
//! ```
//!
//! **`--release` is not optional in practice**: each page pays a flood fill over
//! every pixel for the classical row and a 1024×1024 forward pass for the model
//! row.

use image::{GrayImage, imageops::FilterType};
use tract_onnx::prelude::*;

/// The model's input side. Fixed by the export, which carries no dynamic axes.
const SIDE: usize = 1024;

#[cfg(test)]
mod letterbox_tests {
    use super::*;

    #[test]
    fn the_page_goes_top_left_and_the_padding_is_black() {
        // 512 wide, 256 tall, uniformly white. r = min(1024/256, 1024/512) = 2,
        // so it resizes to 1024x512 and pads 512 rows at the bottom.
        let page = GrayImage::from_pixel(512, 256, image::Luma([255]));

        let (tensor, dw, dh) = letterbox(&page);

        assert_eq!(dw, 0, "the width fills the square exactly");
        assert_eq!(dh, 512, "the height leaves half the square to pad");

        // `to_plain_array_view`, not `to_array_view` — the latter is not on
        // `Tensor` (it lives on the plain-view types), and the compiler's
        // suggestion when you reach for it is exactly this method.
        let view = tensor
            .to_plain_array_view::<f32>()
            .expect("the tensor is f32");

        // Top-left is page, and page is white.
        assert!(
            (view[[0, 0, 0, 0]] - 1.0).abs() < 1e-6,
            "the page starts at the top-left corner"
        );
        // A row inside the padding is black — this is what a centred letterbox
        // would get wrong, and it would displace every box by dh/2.
        assert!(
            view[[0, 0, 600, 0]].abs() < 1e-6,
            "the padding is on the bottom, not split top and bottom"
        );
        // All three channels carry the same luma.
        assert!(
            (view[[0, 1, 0, 0]] - view[[0, 0, 0, 0]]).abs() < 1e-6
                && (view[[0, 2, 0, 0]] - view[[0, 0, 0, 0]]).abs() < 1e-6,
            "a mono page fills all three channels alike"
        );
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-cli letterbox_tests`
Expected: FAIL to compile — `cannot find function `letterbox` in this scope`.

- [ ] **Step 4: Write the implementation**

Add to `crates/taiyaku-cli/src/model_experiment.rs`, above the `letterbox_tests` module:

```rust
/// Resize preserving aspect, pad to a square with black on the right and
/// bottom, and pack into the `1×3×SIDE×SIDE` f32 tensor the graph expects.
///
/// A port of upstream `imgproc_utils.py::letterbox(auto=False, scaleup=True)`
/// followed by `cv2.dnn.blobFromImage(scalefactor=1/255)`. Two details are
/// load-bearing and neither is the obvious choice:
///
///   - **Not centred.** `copyMakeBorder(im, 0, dh, 0, dw, ...)`, with the
///     centring lines commented out upstream. The page is at the top left.
///   - **`scaleup` is left on**, so a page smaller than `SIDE` is enlarged
///     rather than padded. The corpus has 685×1024 pages; they are upscaled.
///
/// Returns the tensor and the padding, which the caller needs to turn boxes in
/// square-space back into page fractions.
fn letterbox(page: &GrayImage) -> (Tensor, usize, usize) {
    let side = u32::try_from(SIDE).expect("SIDE fits u32");
    let (w, h) = page.dimensions();

    // The smaller ratio is the one that fits both sides.
    let r = (f64::from(side) / f64::from(h)).min(f64::from(side) / f64::from(w));
    let new_w = ((f64::from(w) * r).round() as u32).clamp(1, side);
    let new_h = ((f64::from(h) * r).round() as u32).clamp(1, side);

    let resized = image::imageops::resize(page, new_w, new_h, FilterType::Triangle);

    let mut square = GrayImage::new(side, side); // zero-filled: black, as upstream
    image::imageops::replace(&mut square, &resized, 0, 0);

    // Three identical channels: see the module header on luma.
    let tensor: Tensor = tract_ndarray::Array4::from_shape_fn((1, 3, SIDE, SIDE), |(_, _, y, x)| {
        let luma = square
            .get_pixel(
                u32::try_from(x).expect("SIDE fits u32"),
                u32::try_from(y).expect("SIDE fits u32"),
            )
            .0[0];
        f32::from(luma) / 255.0
    })
    .into();

    (tensor, (side - new_w) as usize, (side - new_h) as usize)
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-cli letterbox_tests`
Expected: PASS, 1 test.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/model_experiment.rs crates/taiyaku-cli/src/main.rs
git commit -m "Phase 3's letterbox, ported from the ONNX branch and pinned by the top-left convention that would silently displace every box"
```

---

### Task 3: The block head — confidence, suppression, and page fractions

**Files:**
- Modify: `crates/taiyaku-cli/src/model_experiment.rs`

**Interfaces:**
- Consumes: `SIDE` and `letterbox` from Task 2.
- Produces: `struct Cand { x0: f64, y0: f64, x1: f64, y1: f64, class: usize, score: f32 }`, `fn candidates(blk: &[f32]) -> Vec<Cand>`, `fn suppress(candidates: Vec<Cand>) -> Vec<Cand>`, `fn to_regions(kept: &[Cand], dw: usize, dh: usize) -> Vec<Region>`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/taiyaku-cli/src/model_experiment.rs`:

```rust
#[cfg(test)]
mod block_tests {
    use super::*;

    fn cand(x0: f64, y0: f64, x1: f64, y1: f64, class: usize, score: f32) -> Cand {
        Cand { x0, y0, x1, y1, class, score }
    }

    #[test]
    fn suppression_drops_the_weaker_of_two_overlapping_boxes_of_one_class() {
        let kept = suppress(vec![
            cand(0.0, 0.0, 100.0, 100.0, 0, 0.9),
            cand(5.0, 5.0, 105.0, 105.0, 0, 0.8),
        ]);
        assert_eq!(kept.len(), 1, "near-identical boxes are one detection");
        assert!((kept[0].score - 0.9).abs() < 1e-6, "the stronger one survives");
    }

    #[test]
    fn suppression_is_class_aware() {
        // The same geometry, two classes. YOLOv5 suppresses within a class, so
        // both survive — and a class-blind port would silently halve the count.
        let kept = suppress(vec![
            cand(0.0, 0.0, 100.0, 100.0, 0, 0.9),
            cand(0.0, 0.0, 100.0, 100.0, 1, 0.8),
        ]);
        assert_eq!(kept.len(), 2, "two classes never suppress each other");
    }

    #[test]
    fn suppression_keeps_two_balloons_that_merely_sit_near_each_other() {
        let kept = suppress(vec![
            cand(0.0, 0.0, 100.0, 100.0, 0, 0.9),
            cand(200.0, 200.0, 300.0, 300.0, 0, 0.8),
        ]);
        assert_eq!(kept.len(), 2, "disjoint boxes are two detections");
    }

    #[test]
    fn a_candidate_filling_the_live_region_becomes_the_whole_page() {
        // dw = dh = 0, so the live region is the whole square, and a box over
        // all of it is the whole page in fractions.
        let regions = to_regions(&[cand(0.0, 0.0, 1024.0, 1024.0, 0, 0.9)], 0, 0);
        assert_eq!(regions.len(), 1);
        assert!((regions[0].x - 0.0).abs() < 1e-9);
        assert!((regions[0].w - 1.0).abs() < 1e-9);
        assert!((regions[0].h - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_padding_is_excluded_from_the_page_fraction() {
        // Half the square is padding on the bottom. A box over the live half is
        // the whole page height, not half of it — this is the arithmetic that
        // maps square-space back onto the scan.
        let regions = to_regions(&[cand(0.0, 0.0, 1024.0, 512.0, 0, 0.9)], 0, 512);
        assert!(
            (regions[0].h - 1.0).abs() < 1e-9,
            "the live region is the page, and the padding is not"
        );
    }

    #[test]
    fn a_tall_box_is_vertical_and_a_wide_one_is_horizontal() {
        let tall = to_regions(&[cand(0.0, 0.0, 100.0, 900.0, 0, 0.9)], 0, 0);
        let wide = to_regions(&[cand(0.0, 0.0, 900.0, 100.0, 0, 0.9)], 0, 0);
        assert_eq!(tall[0].axis, Axis::Vertical);
        assert_eq!(wide[0].axis, Axis::Horizontal);
    }

    #[test]
    fn a_candidate_below_the_confidence_floor_is_not_proposed() {
        // One grid cell, objectness under the floor. BLK_STRIDE floats:
        // cx, cy, w, h, objectness, class0, class1.
        let weak = [512.0, 512.0, 100.0, 100.0, 0.05, 0.9, 0.1];
        assert!(candidates(&weak).is_empty(), "objectness gates first");

        let strong = [512.0, 512.0, 100.0, 100.0, 0.95, 0.9, 0.1];
        assert_eq!(candidates(&strong).len(), 1, "a confident cell is proposed");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-cli block_tests`
Expected: FAIL to compile — `cannot find type `Cand``, `cannot find function `suppress``, `cannot find function `to_regions``, `cannot find function `candidates``.

- [ ] **Step 3: Write the implementation**

Add to `crates/taiyaku-cli/src/model_experiment.rs`. Extend the `use` block at the top to:

```rust
use image::{GrayImage, imageops::FilterType};
use taiyaku_core::{Axis, Region};
use tract_onnx::prelude::*;
```

**`Axis` is imported explicitly on purpose, and the order of those two lines is
not what makes it work.** `tract_onnx::prelude::*` re-exports `tract-core`'s own
`Axis` (`tract_core::axes::Axis`, a tensor-axis descriptor — nothing to do with
writing direction). An explicit `use` outranks a glob import in Rust, so
`taiyaku_core::Axis` is the one that binds and this compiles. The hazard is that
**deleting the explicit import does not break the build** — it silently
substitutes a different `Axis`, and the first sign would be a type error
somewhere else entirely. Leave it named.

and add the constants beside `SIDE`:

```rust
/// Upstream's `TextDetector.__init__` defaults for the block head:
/// `conf_thresh=0.4`, `nms_thresh=0.35`. Taken rather than tuned — tuning them
/// against this dev set would be the overfit `band_sweep.rs` already flags in
/// its own header, and a candidate that only wins at a tuned threshold is not
/// the finding phase 3 is after.
const BLK_CONF: f32 = 0.4;
const BLK_IOU: f64 = 0.35;

/// `blk` is `1 × 64512 × 7`: cx, cy, w, h, objectness, then two class scores.
const BLK_STRIDE: usize = 7;
```

then the implementation:

```rust
/// One decoded grid cell, in square-space pixels.
#[derive(Clone, Copy, Debug)]
struct Cand {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    class: usize,
    score: f32,
}

/// Intersection over union, in square-space pixels.
fn iou(a: &Cand, b: &Cand) -> f64 {
    let w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let overlap = w * h;
    let union = (a.x1 - a.x0) * (a.y1 - a.y0) + (b.x1 - b.x0) * (b.y1 - b.y0) - overlap;
    if union <= 0.0 { 0.0 } else { overlap / union }
}

/// Grid cells that clear the confidence floor, as corner boxes.
///
/// The export decodes the grid in-graph — anchors applied, sigmoids applied,
/// coordinates already in square-space pixels — so this is a filter, not a
/// decode. Objectness gates first and then objectness × class, which is what
/// YOLOv5's own `non_max_suppression` does.
fn candidates(blk: &[f32]) -> Vec<Cand> {
    let mut out = Vec::new();
    for c in blk.chunks_exact(BLK_STRIDE) {
        let objectness = c[4];
        if objectness < BLK_CONF {
            continue;
        }
        let (class, class_score) = if c[5] >= c[6] { (0, c[5]) } else { (1, c[6]) };
        let score = objectness * class_score;
        if score < BLK_CONF {
            continue;
        }
        let (cx, cy) = (f64::from(c[0]), f64::from(c[1]));
        let (w, h) = (f64::from(c[2]), f64::from(c[3]));
        out.push(Cand {
            x0: cx - w / 2.0,
            y0: cy - h / 2.0,
            x1: cx + w / 2.0,
            y1: cy + h / 2.0,
            class,
            score,
        });
    }
    out
}

/// Greedy non-maximum suppression, within a class.
///
/// Written here because **the graph does not contain it**: there is no
/// `NonMaxSuppression` operator in the file, and upstream suppresses in Python
/// (`inference.py::postprocess_yolo`). That absence is also why `tract` runs
/// this model at all — the operators a YOLO export usually trips a pure-Rust
/// runtime on are the ones that come after the grid.
fn suppress(mut candidates: Vec<Cand>) -> Vec<Cand> {
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<Cand> = Vec::new();
    for candidate in candidates {
        if kept
            .iter()
            .any(|k| k.class == candidate.class && iou(k, &candidate) > BLK_IOU)
        {
            continue;
        }
        kept.push(candidate);
    }
    kept
}

/// Square-space boxes as page fractions.
///
/// The live region — the square minus the right and bottom padding — *is* the
/// page, so a fraction of the live region is a fraction of the page and the
/// scan's own pixel size never enters. Boxes are clamped to the page: a grid
/// cell near the edge can decode to a box that runs slightly outside it.
///
/// **`axis` is derived from the box's own shape, not from the model's class
/// channel.** What the two classes mean is not established, and `score_page`
/// does not read `axis` — it scores geometry. Asserting a meaning for the class
/// channel here would be exactly the kind of unmeasured claim this sub-project
/// exists to stop.
fn to_regions(kept: &[Cand], dw: usize, dh: usize) -> Vec<Region> {
    let live_w = (SIDE - dw) as f64;
    let live_h = (SIDE - dh) as f64;
    kept.iter()
        .map(|c| {
            let x = (c.x0 / live_w).clamp(0.0, 1.0);
            let y = (c.y0 / live_h).clamp(0.0, 1.0);
            let w = (c.x1 / live_w).clamp(0.0, 1.0) - x;
            let h = (c.y1 / live_h).clamp(0.0, 1.0) - y;
            Region {
                x,
                y,
                w,
                h,
                axis: if h > w { Axis::Vertical } else { Axis::Horizontal },
            }
        })
        .filter(|r| r.w > 0.0 && r.h > 0.0)
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-cli block_tests`
Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/model_experiment.rs
git commit -m "Phase 3's block head: the confidence filter, the class-aware suppression the graph does not carry, and square-space to page fractions"
```

---

### Task 4: The scored run

**Files:**
- Modify: `crates/taiyaku-cli/src/model_experiment.rs`

**Interfaces:**
- Consumes: `letterbox`, `candidates`, `suppress`, `to_regions` from Tasks 2 and 3.
- Produces: `#[test] fn the_model_candidate_scored()` printing the head-to-head table.

- [ ] **Step 1: Extend the imports and add the edition list**

Extend the `use` block at the top of `crates/taiyaku-cli/src/model_experiment.rs` to:

```rust
use std::path::PathBuf;

use image::{GrayImage, imageops::FilterType};
use taiyaku_core::{Axis, Region};
use tract_onnx::prelude::*;

use crate::corpus::{corpus, members};
use crate::detect;
use crate::score::{COVERAGE, Report, resolve_labelled, score_page};
```

and add, beside the constants:

```rust
/// The nine editions of four works that carry labels, named rather than
/// globbed — `truth_population.rs`'s and `band_sweep.rs`'s array, for their
/// reason: a glob that stops matching is the silent-nothing failure the
/// assertions below refuse to have.
const EDITIONS: [(&str, &str); 9] = [
    ("gotoubun-v01", "raw-mono"),
    ("gotoubun-v01", "en-mono"),
    ("gotoubun-v01", "en-fan"),
    ("kaguya-v01", "raw-mono"),
    ("kaguya-v01", "en-mono"),
    ("kobayashi-v01", "raw-mono"),
    ("kobayashi-v01", "en-mono"),
    ("oshinoko-v01", "raw-mono"),
    ("oshinoko-v01", "en-mono"),
];
```

- [ ] **Step 2: Write the model wrapper**

Add to `crates/taiyaku-cli/src/model_experiment.rs`:

```rust
/// The weights, from `TAIYAKU_MODEL`.
///
/// An env var rather than a path guessed from `CARGO_MANIFEST_DIR`, so that the
/// skip message can name exactly what is missing and how to get it. 95 MB is
/// not something to silently look for in three places.
fn model_path() -> Option<PathBuf> {
    let raw = std::env::var_os("TAIYAKU_MODEL")?;
    let path = PathBuf::from(raw);
    path.is_file().then_some(path)
}

/// Build the runnable graph once, for reuse across every page.
///
/// The input fact is fixed at `1×3×SIDE×SIDE`: the export carries no dynamic
/// axes and `tract` cannot resolve output facts without being told the shape.
fn load(path: &std::path::Path) -> TractResult<std::sync::Arc<TypedRunnableModel>> {
    tract_onnx::onnx()
        .model_for_path(path)?
        .with_input_fact(
            0,
            InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, SIDE, SIDE)),
        )?
        .into_optimized()?
        .into_runnable()
}

/// One page through the model, as regions in page fractions.
///
/// Output order is the file's — `blk`, `seg`, `det`. Only `blk` is read.
/// `seg`, the full-resolution mask, recalls the same text at glyph granularity
/// and would score as `split` and `fragment` rather than `1:1`; `det` is the
/// line head, below the level L2 pairs on. Both are measured in `docs/notes.md`
/// and neither is what the promotion rule counts.
fn detect_with_model(
    plan: &std::sync::Arc<TypedRunnableModel>,
    page: &GrayImage,
) -> TractResult<Vec<Region>> {
    let (tensor, dw, dh) = letterbox(page);
    let outputs = plan.run(tvec!(tensor.into()))?;
    let blk: Vec<f32> = outputs[0]
        .to_plain_array_view::<f32>()?
        .iter()
        .copied()
        .collect();
    Ok(to_regions(&suppress(candidates(&blk)), dw, dh))
}
```

- [ ] **Step 3: Write the test**

Add to `crates/taiyaku-cli/src/model_experiment.rs`:

```rust
#[test]
fn the_model_candidate_scored() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    let Some(weights) = model_path() else {
        eprintln!("TAIYAKU_MODEL unset or not a file — skipping; run ./scripts/fetch-model.sh");
        return;
    };

    let plan = load(&weights).unwrap_or_else(|e| panic!("{}: {e}", weights.display()));

    let mut classical = Report::default();
    let mut model = Report::default();
    let mut editions_seen = 0usize;

    for (work, edition) in EDITIONS {
        let dir = root.join(work).join(edition);
        let sidecar = root.join(work).join("taiyaku.json");
        let paths: Vec<PathBuf> = members(&dir).into_iter().map(PathBuf::from).collect();
        let resolved = resolve_labelled(
            sidecar.to_str().expect("a corpus path is valid UTF-8"),
            &paths,
        )
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // **Both detectors inside one decode**, which is `band_sweep.rs`'s
        // reason restated: `decode_all` bounds memory by decoding large pages
        // one at a time — the corpus holds 430 MB TIFFs — so the pixels are not
        // available afterwards, and a second pass would pay for 45 more
        // full-resolution decodes to score the same pages twice.
        let detected = crate::fingerprint::decode_all(&resolved.pages, |_, luma| {
            (
                detect::detect_page(luma),
                detect_with_model(&plan, luma).expect("forward pass"),
            )
        })
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        for (drawn, (found_classical, found_model)) in resolved.truth.iter().zip(&detected) {
            classical.add_page(drawn, &score_page(drawn, found_classical, COVERAGE));
            model.add_page(drawn, &score_page(drawn, found_model, COVERAGE));
        }

        println!("edition={work}/{edition} pages={}", detected.len());
        editions_seen += 1;
    }

    println!("\n{:<12} {:>5} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7}", "candidate", "1:1", "merged", "split", "frag", "missed", "spurious", "/page");
    for (name, report) in [("detect_page", &classical), ("model/blk", &model)] {
        println!(
            "{name:<12} {:>5} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7.2}",
            report.one_to_one,
            report.merged,
            report.split,
            report.fragment,
            report.missed,
            report.spurious,
            report.spurious_per_page(),
        );
    }

    // Per kind beside the total, because they are different results: the
    // per-kind split is the evidence spec §3.4 defers the balloon-interior
    // question to, and the model's whole claim is about a kind the classical
    // detector scores zero on.
    println!("\nfound/drawn per kind, same runs:");
    for (name, report) in [("detect_page", &classical), ("model/blk", &model)] {
        let kinds: Vec<String> = report
            .by_kind
            .iter()
            .map(|(kind, tally)| format!("{kind:?}={}/{}", tally.found, tally.total))
            .collect();
        println!("{name:<12} {}", kinds.join(" "));
    }

    // Assertions: that it measured something. Never a quality figure — those
    // move when a detector improves, and a check that fails on improvement is
    // not a measurement. Whether the model is promoted is read off the table
    // above against the rule, by a person.
    assert_eq!(editions_seen, EDITIONS.len(), "every named edition scored");
    assert_eq!(classical.pages, 45, "every labelled page scored classically");
    assert_eq!(model.pages, 45, "every labelled page scored by the model");
    assert!(!model.by_kind.is_empty(), "some truth box was scored");
}
```

- [ ] **Step 4: Run it against the corpus**

Run:

```bash
./scripts/fetch-model.sh
TAIYAKU_CORPUS="$PWD/corpus" TAIYAKU_MODEL="$PWD/models/comictextdetector.pt.onnx" \
  cargo test --release -p taiyaku-cli model_experiment -- --nocapture
```

Expected: PASS. Nine `edition=` lines, then a two-row table, then the per-kind lines. Roughly three to six minutes — 45 pages, each paying a flood fill and a forward pass.

If the machine runs short of memory, `decode_all` parallelises across pages via rayon and each forward pass allocates its own intermediates; cap it with `RAYON_NUM_THREADS=4` prepended to the command. Record it if you need it.

- [ ] **Step 5: Verify it skips cleanly with neither variable**

Run: `env -u TAIYAKU_CORPUS -u TAIYAKU_MODEL cargo test -p taiyaku-cli model_experiment`
Expected: PASS, with `TAIYAKU_CORPUS unset — skipping` on stderr. This is what CI runs.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/model_experiment.rs
git commit -m "Phase 3's scored run: both detectors in one decode, one scorer, one table, and no assertion about which wins"
```

---

### Task 5: The gate

**Files:**
- Modify: `.forgejo/workflows/ci.yml` (two `--ignore-filename-regex` lines)
- Modify: `scripts/check-all.sh` (two `--ignore-filename-regex` lines)
- Modify: `docs/status.md` (the test count, twice; the dependency sentence)

**Interfaces:**
- Consumes: the module from Task 4.
- Produces: a green `./scripts/check-all.sh`.

- [ ] **Step 1: Add the module to all four ignore-regex sites**

The regex appears **four** times and the copies cannot silently disagree — `ci.yml`'s own comment records that a narrower copy lets the excluded files in at 12–40% while a wider one drops the file count and trips `coverage-floor.mjs`'s minimum-file check.

In `.forgejo/workflows/ci.yml` (the "Tests, with coverage" step and the "Coverage floor, per file" step) and in `scripts/check-all.sh` (lines 190 and 199), change every occurrence of:

```
'(band_sweep|geometry_experiment|region_experiment|truth_population|score_corpus|/corpus)\.rs$'
```

to:

```
'(band_sweep|geometry_experiment|model_experiment|region_experiment|truth_population|score_corpus|/corpus)\.rs$'
```

- [ ] **Step 2: Verify the two files still hash identically**

`ci.yml`'s comment requires that the six lines match `check-all.sh`'s after stripping leading whitespace and the `run` / `run env -u TAIYAKU_CORPUS` prefixes. Confirm by eye that all four edited lines are character-identical apart from indentation, then:

Run: `grep -c 'model_experiment' .forgejo/workflows/ci.yml scripts/check-all.sh`
Expected: `.forgejo/workflows/ci.yml:2` and `scripts/check-all.sh:2`.

- [ ] **Step 3: Update the test count in `docs/status.md`**

`scripts/status-test-count.mjs` asserts the figure in `status.md` against `target/nextest/default/junit.xml`. The new module adds exactly one test that runs and returns early, plus the eight synthetic ones from Tasks 2 and 3 — nine in total, so **727 becomes 736**.

Confirm the true number rather than trusting this arithmetic:

Run: `env -u TAIYAKU_CORPUS cargo test --workspace 2>&1 | grep -E '^test result' `
Expected: a set of lines whose `passed` values sum to the new total.

Then edit both occurrences in `docs/status.md` — line 78 (`Five crates, 727 tests, plus 1,306 in web's TypeScript`) and line 94 (`Seventeen of the 727 are standing checks gated on`) — replacing `727` with the measured figure.

**Leave "Seventeen" alone.** That is a per-site list — ten in `corpus.rs`, three in `taiyaku-serve/src/corpus.rs`, four in `score_corpus.rs` — of *standing checks*, and `model_experiment.rs` is a one-off measurement like `band_sweep.rs`, which is not counted there either.

- [ ] **Step 4: Correct the dependency sentence in `docs/status.md`**

Line 78 also claims "no dependencies beyond `image`, `zip`, `sha2`, `rayon`, `base64` and `serde`, plus …". `zip` is already a dev-dependency and is listed, so the sentence covers dev-dependencies and is now wrong. Add `tract-onnx` to it, marked for what it is — for example, after the existing list, insert:

```
and `tract-onnx` as a dev-dependency of `taiyaku-cli` alone, for phase 3's
gated measurement — it reaches no binary and no other crate
```

- [ ] **Step 5: Run the whole gate**

Run: `./scripts/check-all.sh`
Expected: green, all four jobs. This is the run that proves the coverage regex, the file count and the test count all agree.

- [ ] **Step 6: Commit**

```bash
git add .forgejo/workflows/ci.yml scripts/check-all.sh docs/status.md
git commit -m "The gate learns about model_experiment: four regex sites, the test count, and a dependency sentence that already covered dev-dependencies"
```

---

### Task 6: The finding

**Files:**
- Modify: `docs/notes.md` (append a section)
- Modify: `docs/status.md` (the "L2 text regions" entry)
- Modify: `docs/superpowers/specs/2026-08-11-l2-detector-design.md` (§4.2)

**Interfaces:**
- Consumes: the table from Task 4.
- Produces: the written-up result. No code.

- [ ] **Step 1: Append the measurement to `docs/notes.md`**

Add a section titled `## The model candidate, scored against the same 45 pages`. It must contain, at minimum:

- the two-row table exactly as printed, both candidates, all six outcome columns plus spurious-per-page;
- the per-kind lines, with the four Japanese `raw-mono` editions called out, since `0 of 167` on dialogue is the figure the whole phase exists against;
- the verdict **stated against the promotion rule as fixed on 2026-08-11** — a candidate is promoted only if it increases 1:1 *and* keeps spurious-per-page at or under 10.7, ties to the incumbent — and not against the outcome;
- what the run cost, in wall-clock, and on what;
- **that `COVERAGE` is still provisional at 0.5 and unmeasured**, together with the fact recorded on 2026-08-12 that the model's boxes hug the glyphs where a hand-drawn box carries margin. That interaction is predictable in direction and unmeasured in size, and it bears on this table more than on any previous one.

- [ ] **Step 2: Update the "L2 text regions" entry in `docs/status.md`**

The entry currently ends by saying phase 3's spike answered its questions and that "Producing the score is phase 3 proper, in `model_experiment.rs`, and it is the next thing this entry is waiting on." Replace that closing with the number, and say plainly whether the model clears the rule. If it does not, that is the finding and it is written up rather than tuned away.

- [ ] **Step 3: Close §4.2 of the spec**

Add a dated block to `docs/superpowers/specs/2026-08-11-l2-detector-design.md` §4.2 recording what landed, in the same shape as the 2026-08-12 block already in §4.1.

If the model wins, note that promoting it into the binary remains a **separate decision** — §9 says so — and that decision now carries the Manga109-s attribution obligation with it.

- [ ] **Step 4: Delete the spike, or say why not**

`spike/`'s own header says the directory gets deleted once its findings land, and the workspace `Cargo.toml` repeats it. Its findings have landed twice now. Either remove it:

```bash
git rm -r --cached spike && rm -rf "${PWD:?}/spike"
```

or leave it and record, in `notes.md`, what it still answers that the workspace does not — the overlay PNGs are a real answer, and `model_experiment.rs` draws nothing.

- [ ] **Step 5: Commit**

```bash
git add docs/notes.md docs/status.md docs/superpowers/specs/2026-08-11-l2-detector-design.md
git commit -m "The number phase 3 exists for, and what it says about the rule"
```
