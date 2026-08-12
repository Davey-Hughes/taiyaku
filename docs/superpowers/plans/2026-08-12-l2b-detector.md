# L2b: the detector, redesigned against measured ground truth — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Take `taiyaku regions` from 0.4% recall (2 of 478 hand-drawn boxes) to a candidate that clears the promotion rule, by first measuring what text is made of and then building the detector to that measurement.

**Architecture:** Three changes to `crates/taiyaku-cli/src/detect.rs`, preceded by a measurement. Phase 1 partitions each page's connected components into those inside a hand-drawn truth box and those outside, and reports both distributions — the first time any constant in `detect.rs` can be a measurement rather than a starting value. Phase 2 inserts a band-pass filter on the component population *before* the smear (the measured failure is that 63–96% of components are sub-5px screentone, which welds into one page-spanning blob), smears both axes instead of one, and re-derives every constant from phase 1's numbers.

**Tech Stack:** Rust 2024, `image` 0.25 for decoding, `rayon` for the bounded parallel decode. No new dependencies. The model candidate is **not** in this plan — see "Scope" below.

## Scope

**This plan covers phases 1 and 2 of `docs/superpowers/specs/2026-08-11-l2-detector-design.md`.** Phase 3 (the ONNX model candidate) is deliberately excluded: the spec routes it through a spike in `spike/` that has not run, and its three open questions — which model, which runtime, whether the weights are redistributable under GPL-3.0-or-later — are all marked unverified in spec §4.1 and §10. Writing task steps against unanswered questions would be guesswork. Phase 3 gets its own plan once the spike has answered them.

Phases 1 and 2 produce working, testable software and a promoted-or-not verdict on their own.

## Global Constraints

Copied verbatim from the spec and from `detect.rs`'s own header. Every task's requirements implicitly include these.

- **No new dependency.** `detect.rs`'s stated constraint: "none of them a model and none of them a new dependency". Phase 2 adds neither.
- **A constant in `detect.rs` asserted as measured — here or anywhere else — remains a defect** unless the measurement exists. `detect.rs`'s header states this rule; this plan makes it satisfiable, and does not relax it.
- **Experiment modules assert only that they measured something** — a non-empty sample, every page accounted for, every figure finite. Never a quality figure. "A measurement that fails when the thing it measures improves is a check pretending to be a measurement."
- **Experiment modules are `#[cfg(test)]` and gated on `TAIYAKU_CORPUS`**, skipping cleanly when unset, so CI stays green without the 24 GB corpus and nothing reaches the shipped binary.
- **Experiment modules call `detect.rs`'s own functions, never copies of them.** `region_experiment.rs`'s header states this and gives the reason: a copy drifts from what `taiyaku regions` actually does.
- **The promotion rule, as corrected on 2026-08-11 and frozen once the first candidate is scored:** *A candidate replaces the incumbent only if, over the dev set, it increases the count of 1:1 matches, and its spurious-per-page does not exceed the dev set's own truth-boxes-per-page (10.7). Ties go to the incumbent.*
- **The held-out 30 pages are not opened** anywhere in this plan. They open once, at the end of sub-project B, after a candidate is promoted.
- **No attribution in commit messages.** No `Co-Authored-By`, no `Generated with`.
- **Shell:** a variable feeding a recursive delete is written `"${VAR:?}"`, never `"$VAR"`.

## Baseline to beat

Measured on this machine, 2026-08-11, release build, all nine editions:

| | 1:1 | merged | split | fragment | missed | spurious |
| --- | --- | --- | --- | --- | --- | --- |
| total | **2** | 47 | 0 | 0 | 429 | 3 (0.07/page) |

478 scored truth boxes, 45 pages, 10.7 boxes per page. Reproduce this before changing anything (Task 1 Step 1).

## File Structure

| file | change | responsibility |
| --- | --- | --- |
| `crates/taiyaku-cli/src/score.rs` | modify | gains `Labelled` + `resolve_labelled`, extracted from `command` |
| `crates/taiyaku-cli/src/truth_population.rs` | create, `#[cfg(test)]` | phase 1: the two distributions and the table |
| `crates/taiyaku-cli/src/main.rs` | modify | one gated `mod` declaration |
| `crates/taiyaku-cli/src/detect.rs` | modify | the band, the two-axis smear, the constants |
| `docs/notes.md` | append | phase 1's table, phase 2's verdict |
| `docs/status.md` | update | the new figures, O31–O34 |

`detect.rs` is the only production file whose behaviour changes, and only in Tasks 4–6.

---

### Task 1: Extract the labelled-page resolution from `score::command`

`score::command` resolves a sidecar plus members into "which edition is this, which pages did somebody label, and are the labels stale". Phase 1 needs exactly that, and must not carry a second copy — a measurement taken on different pages than the scorer scores is an invisible defect.

Pure refactor. No behaviour change.

**Files:**
- Modify: `crates/taiyaku-cli/src/score.rs:410-540`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces, for Task 2:
  ```rust
  pub(crate) struct Labelled {
      pub truth: Vec<Vec<taiyaku_core::TruthRegion>>,
      pub pages: Vec<taiyaku_read::Page>,
  }
  pub(crate) fn resolve_labelled(
      sidecar: &str,
      members: &[std::path::PathBuf],
  ) -> Result<Labelled, String>
  ```
  `truth` and `pages` are parallel and in ascending page order.

  **Two fields deliberately absent.** An earlier draft carried `edition:
  String` and `numbers: Vec<u32>`. Nothing reads either — `command` uses
  `truth` and `pages`, and so does Task 2 — and `taiyaku-cli` is a binary
  crate with no `lib.rs`, so an unread field is `dead_code`, which
  `check-all.sh` runs as `-D warnings`. Do not add them back "for the
  report"; add them in the task that reads them.

- [ ] **Step 1: Record the baseline output, so the refactor can be proved inert**

```bash
cd /home/davey/projects/taiyaku
cargo build --release -p taiyaku-cli
./scripts/score-dev-set.sh | tee /tmp/score-before.txt
```

Expected: nine rows, `editions scored: 9 of 9`, exit 0. Their totals must be `1:1=2 merged=47 missed=429 spurious=3`, over 478 scored boxes.

**Do not hand-roll this loop.** `scripts/score-dev-set.sh` exists because the invocation is a trap that has been fallen into twice and failed quietly both times: an edition of `.cbz` archives is those archives, while an edition of loose page images is **the directory itself** — one member, not one per image. Get it wrong and `score` reports "no edition is made of these N members", which a `case "$out" in pages=*)` filter turns into a *missing row* rather than an error. Nine editions silently become six and the totals read like a finding. The script picks the right form per edition, names the nine rather than globbing, and asserts the row count.

- [ ] **Step 2: Add `Labelled` and `resolve_labelled` to `score.rs`**

Insert above `pub fn command`. The body is lines 410–531 of the current `command`, moved verbatim — every comment in that range explains a bug already paid for (argv order vs member order, unreachable pages, staleness) and must move with the code, not be summarised.

```rust
/// One edition's labelled pages, resolved and ready to decode.
///
/// Extracted from `command` so `truth_population.rs` measures **the same pages
/// this command scores**. A measurement taken on a different page set than the
/// score is a defect nothing would catch: both would print plausible numbers.
///
/// `truth` and `pages` are parallel and ascending. `pages` is undecoded bytes
/// rather than `GrayImage`, because `decode_all` bounds memory by decoding
/// large pages one at a time and the corpus holds 430 MB TIFFs.
pub(crate) struct Labelled {
    pub truth: Vec<Vec<taiyaku_core::TruthRegion>>,
    pub pages: Vec<taiyaku_read::Page>,
}

pub(crate) fn resolve_labelled(
    sidecar: &str,
    members: &[std::path::PathBuf],
) -> Result<Labelled, String> {
    // ... lines 410-531 of the old `command`, verbatim, ending with:
    Ok(Labelled {
        truth,
        pages: subset,
    })
}
```

Two adjustments while moving, and only these two:

1. The old code indexed `labelled.pages[number]` at scoring time. Build the `truth` vector in the same `unzip` that builds `numbers`, so the three stay parallel by construction:

```rust
    let mut truth: Vec<Vec<taiyaku_core::TruthRegion>> = Vec::new();
    let mut subset: Vec<taiyaku_read::Page> = Vec::new();
    for (index, page) in pages.into_iter().enumerate() {
        let number = u32::try_from(index).expect("a page index fits in u32");
        // Pages absent from the truth document are skipped: nobody labelled
        // them, and scoring against no labels is not zero recall, it is no
        // measurement.
        let Some(drawn) = labelled.pages.get(&number) else {
            continue;
        };
        truth.push(drawn.clone());
        subset.push(page);
    }
```

2. `members` becomes a parameter rather than being parsed from argv.

- [ ] **Step 3: Rewrite `command`'s tail to call it**

Replace everything from the old line 410 through line 540 with:

```rust
    let resolved = resolve_labelled(sidecar, &members)?;
    let detected =
        crate::fingerprint::decode_all(&resolved.pages, |_, luma| crate::detect::detect_page(luma))?;

    let mut report = Report::default();
    for (drawn, found) in resolved.truth.iter().zip(detected) {
        report.add_page(drawn, &score_page(drawn, &found, coverage));
    }

    print!("{}", report.render());
    Ok(())
```

- [ ] **Step 4: Build and run the existing suite**

```bash
cargo test -p taiyaku-cli 2>&1 | tail -20
```

Expected: PASS. No test changes — this task adds none and breaks none. If a test fails, the move was not verbatim.

- [ ] **Step 5: Prove the refactor inert**

```bash
cargo build --release -p taiyaku-cli
./scripts/score-dev-set.sh > /tmp/score-after.txt
diff /tmp/score-before.txt /tmp/score-after.txt && echo "IDENTICAL"
```

Expected: `IDENTICAL`. A refactor that moves a number is not a refactor.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/score.rs
git commit -m "score: the labelled-page resolution, extracted so a measurement can share it

Phase 1 of the detector redesign has to measure the same pages the scorer
scores. Resolving them twice would be two chances to disagree, and both
copies would print plausible numbers while disagreeing. Moved verbatim,
comments included: every one of them records a bug already paid for.

Output over all nine editions is byte-identical before and after."
```

---

### Task 2: Phase 1 — the measurement module

**Files:**
- Create: `crates/taiyaku-cli/src/truth_population.rs`
- Modify: `crates/taiyaku-cli/src/main.rs`

**Interfaces:**
- Consumes: `score::{Labelled, resolve_labelled}` from Task 1; `detect::{otsu, components, merge_along, Bounds}` and `corpus::{corpus, members}`, all existing.
- Produces: a printed table, and the numbers Tasks 4–6 cite. No public API.

- [ ] **Step 1: Declare the module**

In `crates/taiyaku-cli/src/main.rs`, beside the existing `#[cfg(test)] mod region_experiment;`:

```rust
// Phase 1 of the detector redesign: what text is made of, measured from the
// boxes a person drew. Gated on TAIYAKU_CORPUS exactly as `corpus`,
// `geometry_experiment` and `region_experiment` are, and declared the same
// way, so no part of it is compiled into the binary.
#[cfg(test)]
mod truth_population;
```

- [ ] **Step 2: Write the module with its measurement, and run it to see it fail**

Create `crates/taiyaku-cli/src/truth_population.rs`:

```rust
//! What text is made of, measured from the boxes a person drew.
//!
//! Every constant in `detect.rs` says "a starting value, not a measurement",
//! and until 2026-08-11 there was no material against which one could become
//! one. There is now: 480 hand-drawn boxes over 45 pages. The components
//! *inside* a box are the glyph population; the ones outside are the reject
//! population. A size band, an ink floor and a smear gap are each read off
//! those two distributions rather than guessed.
//!
//! **The shape is `region_experiment.rs`'s**, deliberately, rather than a
//! second convention: `#[cfg(test)]`, gated on `TAIYAKU_CORPUS`, skipping
//! cleanly when unset, calling `detect.rs`'s own `otsu`, `components` and
//! `merge_along` rather than copies, and asserting only that it measured
//! something — never a quality figure. A measurement that fails when the thing
//! it measures improves is a check pretending to be a measurement.
//!
//! It reads its pages through `score::resolve_labelled`, so it measures
//! **exactly the pages `taiyaku score` scores**. Resolving them independently
//! would be two chances to disagree while both printed plausible numbers.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli truth_population -- --nocapture
//! ```
//!
//! **`--release` is not optional in practice**: `components` is a flood fill
//! over every pixel of a full-resolution scan, once per labelled page.

use std::path::PathBuf;

use image::GrayImage;
use taiyaku_core::{Axis, TruthRegion};

use crate::corpus::{corpus, members};
use crate::detect::{self, Bounds};
use crate::score::resolve_labelled;

/// The nine editions of four works that carry labels, named rather than
/// globbed. A glob that stops matching is the silent-nothing failure the
/// assertions below refuse to have — the same reasoning `region_experiment`'s
/// own `EDITIONS` gives.
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

/// A truth box in this page's pixels, inclusive on both corners, alongside the
/// axis the labeller gave it.
struct TruthPixels {
    bounds: Bounds,
    axis: Option<Axis>,
}

/// A page-fraction truth box in the pixels of a page this size.
///
/// Saturating rather than wrapping at the right and bottom edges: a box drawn
/// flush to the edge rounds to exactly `width`, and a `Bounds` is inclusive,
/// so the last valid coordinate is `width - 1`.
fn to_pixels(t: &TruthRegion, width: u32, height: u32) -> Option<TruthPixels> {
    if width == 0 || height == 0 {
        return None;
    }
    let scale = |v: f64, span: u32| -> u32 {
        let clamped = v.clamp(0.0, 1.0) * f64::from(span);
        (clamped as u32).min(span - 1)
    };
    let x0 = scale(t.x, width);
    let y0 = scale(t.y, height);
    let x1 = scale(t.x + t.w, width);
    let y1 = scale(t.y + t.h, height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(TruthPixels {
        bounds: Bounds { x0, y0, x1, y1 },
        axis: t.axis,
    })
}

/// Whether a component belongs to a truth box: its **centre** lies inside.
///
/// The alternative, full containment, is wrong for a glyph clipped by the edge
/// of a hand-drawn box, and a human drawing 480 boxes clips some. Picked here
/// rather than left implicit, because the two rules give different populations
/// and the band is read off whichever one this is.
fn centre_inside(stroke: &Bounds, box_: &Bounds) -> bool {
    let cx = stroke.x0 + stroke.width() / 2;
    let cy = stroke.y0 + stroke.height() / 2;
    cx >= box_.x0 && cx <= box_.x1 && cy >= box_.y0 && cy <= box_.y1
}

/// The smallest gap at which `strokes` merge into a single block along `axis`,
/// searched over `0..=limit`.
///
/// Binary search is valid because `merge_along`'s block count is monotone
/// non-increasing in the gap: a larger tolerance can only join more. Calls
/// `merge_along` itself rather than reimplementing the join rule, so this
/// measures the gap **the production smear would need**, not the gap some
/// other definition of adjacency would.
///
/// `None` when they never merge at any gap up to `limit` — two components that
/// do not overlap *across* the axis never join however close they are along
/// it, which is `merge_along`'s documented and deliberate behaviour.
fn merge_gap(strokes: &[Bounds], axis: Axis, limit: u32) -> Option<u32> {
    if strokes.len() <= 1 {
        return Some(0);
    }
    if detect::merge_along(strokes, axis, limit).len() != 1 {
        return None;
    }
    let (mut lo, mut hi) = (0u32, limit);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if detect::merge_along(strokes, axis, mid).len() == 1 {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

/// Ink pixels inside `bounds`, as a fraction of its area.
///
/// **Counts pixels, not bounding boxes.** O34 records `MIN_INK` summing
/// overlapping stroke boxes, which is why 61 of 76 surviving blocks scored
/// above 1.0 against a floor of 0.05. This is the quantity that floor was
/// always meant to be, measured on the material.
fn ink_fraction(page: &GrayImage, threshold: u8, bounds: &Bounds) -> f64 {
    let mut ink = 0u64;
    for y in bounds.y0..=bounds.y1 {
        for x in bounds.x0..=bounds.x1 {
            if page.get_pixel(x, y).0[0] <= threshold {
                ink += 1;
            }
        }
    }
    let area = u64::from(bounds.width()) * u64::from(bounds.height());
    if area == 0 {
        return 0.0;
    }
    ink as f64 / area as f64
}

/// Everything one page contributes to the two distributions.
#[derive(Default)]
struct PageMeasure {
    /// Longer side of every component whose centre is inside a scored box.
    inside_sides: Vec<u32>,
    /// Longer side of every other component on the page.
    outside_sides: Vec<u32>,
    /// Components per scored truth box.
    per_box: Vec<usize>,
    /// True ink fraction per scored truth box.
    ink: Vec<f64>,
    /// Smallest gap merging one box's own components into one block, in
    /// pixels. Boxes whose axis the labeller declined are skipped.
    within_gap: Vec<u32>,
    /// Smallest gap welding two *adjacent* boxes into one block — the ceiling.
    between_gap: Vec<u32>,
    height: u32,
}

fn measure_page(page: &GrayImage, drawn: &[TruthRegion]) -> PageMeasure {
    let (width, height) = page.dimensions();
    let threshold = detect::otsu(page);
    let strokes = detect::components(page, threshold);

    let boxes: Vec<TruthPixels> = drawn
        .iter()
        .filter(|t| !t.ignore)
        .filter_map(|t| to_pixels(t, width, height))
        .collect();

    let mut m = PageMeasure {
        height,
        ..PageMeasure::default()
    };

    // Which box each component belongs to, or none.
    let owner: Vec<Option<usize>> = strokes
        .iter()
        .map(|s| boxes.iter().position(|b| centre_inside(s, &b.bounds)))
        .collect();

    for (stroke, owner) in strokes.iter().zip(&owner) {
        if owner.is_some() {
            m.inside_sides.push(stroke.longer());
        } else {
            m.outside_sides.push(stroke.longer());
        }
    }

    // The gap search is bounded by the page's longer side: no gap beyond that
    // can join anything a smaller one did not.
    let limit = width.max(height);

    for (index, b) in boxes.iter().enumerate() {
        let mine: Vec<Bounds> = strokes
            .iter()
            .zip(&owner)
            .filter(|(_, o)| **o == Some(index))
            .map(|(s, _)| *s)
            .collect();
        m.per_box.push(mine.len());
        m.ink.push(ink_fraction(page, threshold, &b.bounds));
        if let Some(axis) = b.axis
            && let Some(gap) = merge_gap(&mine, axis, limit)
        {
            m.within_gap.push(gap);
        }
    }

    // The ceiling: for each box, the nearest other box sharing its axis, and
    // the gap at which their two component sets weld into one block. A smear
    // gap at or above this merges two balloons and produces a `merged`.
    for (i, b) in boxes.iter().enumerate() {
        let Some(axis) = b.axis else { continue };
        for (j, other) in boxes.iter().enumerate() {
            if i >= j || other.axis != Some(axis) {
                continue;
            }
            let both: Vec<Bounds> = strokes
                .iter()
                .zip(&owner)
                .filter(|(_, o)| **o == Some(i) || **o == Some(j))
                .map(|(s, _)| *s)
                .collect();
            if let Some(gap) = merge_gap(&both, axis, limit) {
                m.between_gap.push(gap);
            }
        }
    }

    m
}

fn quantile(sorted: &[u32], p: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let i = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

#[test]
fn what_text_is_made_of() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };

    let mut inside: Vec<u32> = Vec::new();
    let mut outside: Vec<u32> = Vec::new();
    let mut inside_frac: Vec<u32> = Vec::new();
    let mut per_box: Vec<u32> = Vec::new();
    let mut ink: Vec<f64> = Vec::new();
    let mut within: Vec<u32> = Vec::new();
    let mut between: Vec<u32> = Vec::new();
    let mut pages_seen = 0usize;
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

        // Measure **inside** the decode closure. `decode_all` bounds memory by
        // decoding the large pages one at a time — the corpus holds 430 MB
        // TIFFs — and collecting every decoded `GrayImage` first throws that
        // away, holding five full-resolution pages of a TIFF edition at once.
        //
        // The closure is handed no index, so truth is looked up by page name.
        // Names are unique within an edition (archive entry names, or
        // filenames on disk); the assertion says so out loud rather than
        // trusting it, because a collision would silently score one page's
        // components against another page's boxes.
        let by_name: std::collections::BTreeMap<&str, &Vec<TruthRegion>> = resolved
            .pages
            .iter()
            .map(|p| p.name.as_str())
            .zip(&resolved.truth)
            .collect();
        assert_eq!(
            by_name.len(),
            resolved.pages.len(),
            "{work}/{edition}: two labelled pages share a name, so truth cannot be \
             matched to them by name"
        );
        let measured = crate::fingerprint::decode_all(&resolved.pages, |page, luma| {
            measure_page(luma, by_name[page.name.as_str()])
        })
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        let mut edition_inside = 0usize;
        for m in &measured {
            pages_seen += 1;
            edition_inside += m.inside_sides.len();
            for &side in &m.inside_sides {
                inside.push(side);
                // Per ten-thousandths of page height, so a fraction survives
                // as an integer and the band can be read off directly.
                inside_frac.push((u64::from(side) * 10_000 / u64::from(m.height)) as u32);
            }
            outside.extend(&m.outside_sides);
            per_box.extend(m.per_box.iter().map(|&n| n as u32));
            ink.extend(&m.ink);
            within.extend(&m.within_gap);
            between.extend(&m.between_gap);
        }

        println!(
            "edition={work}/{edition} pages={} inside={} outside={}",
            measured.len(),
            edition_inside,
            measured.iter().map(|m| m.outside_sides.len()).sum::<usize>(),
        );
        editions_seen += 1;
    }

    inside.sort_unstable();
    outside.sort_unstable();
    inside_frac.sort_unstable();
    per_box.sort_unstable();
    within.sort_unstable();
    between.sort_unstable();
    ink.sort_by(f64::total_cmp);

    let q = |v: &[u32]| {
        format!(
            "p05={} p25={} med={} p75={} p95={}",
            quantile(v, 0.05),
            quantile(v, 0.25),
            quantile(v, 0.50),
            quantile(v, 0.75),
            quantile(v, 0.95)
        )
    };
    println!("\ninside  longer-side px      {}", q(&inside));
    println!("outside longer-side px      {}", q(&outside));
    println!("inside  longer-side ‱page-h {}", q(&inside_frac));
    println!("components per truth box    {}", q(&per_box));
    println!("within-box merge gap px     {}", q(&within));
    println!("between-box merge gap px    {}", q(&between));
    println!(
        "true ink fraction in box    p05={:.3} med={:.3} p95={:.3}",
        ink[(ink.len() as f64 * 0.05) as usize],
        ink[ink.len() / 2],
        ink[((ink.len() as f64 * 0.95) as usize).min(ink.len() - 1)]
    );

    // Assertions: that it measured something. Never a quality figure — those
    // move when the detector improves, and a check that fails on improvement
    // is not a measurement.
    assert_eq!(editions_seen, EDITIONS.len(), "every named edition measured");
    assert_eq!(pages_seen, 45, "every labelled page measured");
    assert!(!inside.is_empty(), "some component fell inside a truth box");
    assert!(!outside.is_empty(), "some component fell outside one");
    assert!(!within.is_empty(), "some box's own components merged");
    assert!(ink.iter().all(|v| v.is_finite()), "every ink figure finite");
}
```

- [ ] **Step 3: Run it and confirm it measures**

```bash
cd /home/davey/projects/taiyaku
TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli truth_population -- --nocapture 2>&1 | tail -30
```

Expected: nine `edition=` lines, then the seven distribution lines, then `test result: ok`.

If it panics on `resolve_labelled`, the member list is wrong — check that `members()` returned the directory itself for a loose-image edition, not its contents.

- [ ] **Step 4: Confirm it skips cleanly without the corpus**

```bash
cargo test -p taiyaku-cli truth_population 2>&1 | tail -5
```

Expected: PASS, with `TAIYAKU_CORPUS unset — skipping` on stderr. A gated module that fails when the corpus is absent breaks CI for everyone.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/truth_population.rs crates/taiyaku-cli/src/main.rs
git commit -m "Measure what text is made of, from the boxes a person drew

Phase 1 of the detector redesign. Partitions each labelled page's connected
components by whether their centre falls inside a scored truth box, and
reports both populations: longer side in pixels and in page height,
components per box, true ink fraction counted in pixels rather than summed
bounding boxes, and the smear gap that joins one box's components against
the gap that welds two boxes together.

The last pair is the load-bearing output. The smear gap has to exceed the
first and stay under the second; if those distributions overlap, no single
gap separates a balloon from its neighbour and that is the finding.

Reads its pages through score::resolve_labelled, so it measures exactly the
pages taiyaku score scores. Asserts only that it measured something."
```

---

### Task 3: Record phase 1's findings, and settle the band's form

Not a code task. It is the decision point the whole plan is arranged around, and it must not be folded into Task 4 — Task 4's constants are its output.

**Files:**
- Modify: `docs/notes.md`

- [ ] **Step 1: Run phase 1 and capture the table**

```bash
cd /home/davey/projects/taiyaku
TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli truth_population -- --nocapture 2>&1 | tee /tmp/phase1.txt
```

- [ ] **Step 2: Answer the four questions the table exists to answer**

Write the answers down before writing any code. Each is a number from `/tmp/phase1.txt`, not a judgement:

1. **Do the inside and outside longer-side distributions separate?** Compare `inside p05..p95` against `outside p05..p95`. If the outside p95 sits below the inside p05, a band is trivial. If they overlap almost entirely, **C1 is refused before it is built** — record that and stop; the spec's §3.4 sends the work to C2 (balloon interiors) and this plan ends here.
2. **Is glyph size a stable fraction of page height across the nine editions?** This is spec §8's branch. Compare the `inside longer-side ‱page-h` spread against the per-edition `inside=` counts. Stable ⇒ the band is a page fraction. Not stable ⇒ the band is derived per page.
3. **Does the within-box gap sit under the between-box gap?** Compare `within-box p95` against `between-box p05`. If `within p95 < between p05`, a single smear gap works and `SMEAR_K` is any value between them. If they overlap heavily, no single gap separates balloons — a finding, and it changes Task 5.
4. **What are `MIN_STROKES` and `MIN_INK` actually?** `components per truth box p05` and `true ink fraction p05` respectively, floored a little below the p05 so the filter does not cut the tail it measured.

- [ ] **Step 3: Append the section to `docs/notes.md`**

Follow the file's existing convention: a `##` heading naming the finding, the command that produced it, the table verbatim, and what it does and does not establish. Title it:

```markdown
## What text is made of, measured against 480 hand-drawn boxes
```

State explicitly which of §8's two branches the measurement chose, and which of the four questions above came back as a refusal if any did.

- [ ] **Step 4: Commit**

```bash
git add docs/notes.md
git commit -m "What text is made of, measured

Phase 1's table, and the four questions it settles: whether the inside and
outside component populations separate at all, whether glyph size is a
stable fraction of page height, whether one smear gap can sit above every
within-box gap and below every between-box gap, and what MIN_STROKES and
MIN_INK are on real material rather than as starting values."
```

---

### Task 4: Phase 2a — band-pass the component population

The measured failure: `stroke_scale` is the median longer side over *every* component, 63–96% of which are sub-5px screentone, so the gap collapses to 4–6px and `merge_along`'s transitive passes weld thousands of specks into one page-spanning blob.

**Both prior candidates changed which components the median was taken over. Neither changed which components were smeared.** This does.

**Files:**
- Modify: `crates/taiyaku-cli/src/detect.rs`

**Interfaces:**
- Consumes: phase 1's measurement, recorded in `docs/notes.md`.
- Produces, for Tasks 5 and 7:
  ```rust
  pub(crate) struct Band { pub lo: u32, pub hi: u32 }   // ‱ of page height
  pub(crate) const BAND: Band;                          // provisional; Task 7 fixes it
  pub(crate) fn glyph_band(strokes: &[Bounds], height: u32, band: Band) -> Vec<Bounds>;
  pub(crate) fn detect_page_with(page: &GrayImage, band: Band) -> Vec<Region>;
  pub fn detect_page(page: &GrayImage) -> Vec<Region>;  // = detect_page_with(page, BAND)
  ```

- [ ] **Step 1: Write the failing tests**

Add to `detect.rs`'s `mod tests`:

```rust
    /// A field of dense sub-glyph specks — screentone — must produce no
    /// region. This is the failure the whole redesign exists to end: before
    /// the band, these weld transitively into one page-spanning block that
    /// `MAX_BLOCK_AREA` then discards, which is why the detector returns
    /// nothing on a real page. A detector that silently stopped rejecting
    /// this would look like it worked.
    #[test]
    fn a_field_of_screentone_is_not_a_region() {
        let page = GrayImage::from_fn(800, 1200, |x, y| {
            let dot = x % 6 < 2 && y % 6 < 2;
            Luma([if dot { 0 } else { 255 }])
        });
        assert!(
            detect_page(&page).is_empty(),
            "got {:?}",
            detect_page(&page)
        );
    }

    /// Screentone under a column of glyphs must not take the glyphs with it.
    /// The band keeps the glyphs and drops the dots, so the column still
    /// comes back as one region — a filter that rejected the whole page
    /// would pass the test above and be useless.
    #[test]
    fn glyphs_survive_a_screentone_background() {
        let marks: Vec<(u32, u32, u32, u32)> =
            (0..6).map(|i| (300u32, 200 + i * 60, 40, 40)).collect();
        let page = GrayImage::from_fn(800, 1200, |x, y| {
            let glyph = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            let dot = x % 6 < 2 && y % 6 < 2;
            Luma([if glyph || dot { 0 } else { 255 }])
        });
        let found = detect_page(&page);
        assert_eq!(found.len(), 1, "one block, got {found:?}");
        assert!(found[0].h > 0.2, "spans the column, got {}", found[0].h);
    }
```

- [ ] **Step 2: Run them to confirm they fail**

```bash
cargo test -p taiyaku-cli detect::tests::a_field_of_screentone_is_not_a_region \
                          detect::tests::glyphs_survive_a_screentone_background 2>&1 | tail -20
```

Expected: both FAIL. `a_field_of_screentone_is_not_a_region` may pass by accident via `MAX_BLOCK_AREA` — if it does, say so in the task report, because that is the backstop passing rather than the detector working, and Task 6 removes the accident.

- [ ] **Step 3: Add the band and call it**

Replace the constant block and the head of `detect_page`. **Use Task 3's measured values**, not the illustrative ones below — and if Task 3 chose §8's per-page branch, derive the band from the page's own distribution instead and say so in the doc comment.

```rust
/// A component-size band, in ten-thousandths of page height.
///
/// Ten-thousandths because the quantities are small: phase 1 measured the
/// inside-a-truth-box population at a median of 8 and a p95 of 141, so
/// percent would round most of the distribution to zero.
///
/// Against page height rather than pixels because the corpus spans an
/// eightfold range of page widths for editions of the same work —
/// `stroke_scale`'s doc comment records 685 to 5343px — and screentone is
/// printed at a fixed ruling, so a tone dot is a near-constant fraction of the
/// printed page whatever the scan resolution. **Not the same claim as
/// `SMEAR_K`'s**, which stays tied to the page's own strokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Band {
    pub lo: u32,
    pub hi: u32,
}

/// The band `detect_page` ships with. **Provisional — fixed by Task 7's sweep.**
///
/// `lo` starts at the outside population's p95, which phase 1 measured at 4px;
/// on this corpus's page heights that is roughly 16 ten-thousandths. `hi` is
/// generous and is not load-bearing: it exists to drop a panel border, which
/// is one component spanning the whole page at ~10,000.
///
/// A value here asserted as measured before the sweep has run would be a
/// defect under this file's own header rule. It says provisional because it is.
pub(crate) const BAND: Band = Band { lo: 16, hi: 2000 };

/// The components a band keeps.
///
/// **What this does, stated exactly, because the obvious reading is wrong.**
/// It does *not* isolate glyphs. Phase 1 measured the population inside a
/// hand-drawn truth box at a median longer side of 2px — **identical to the
/// population outside one** — because a glyph shatters into stroke fragments
/// and ringing at the Otsu threshold, and a whole glyph is rare even inside a
/// box. No threshold separates the two. See `docs/notes.md`, "What text is
/// made of, measured against 480 hand-drawn boxes".
///
/// What it does is **thin the field**. Cutting at the outside p95 keeps about
/// 35% of the inside population and at most 5% of the outside, taking the
/// welding population from 903,447 components to roughly 45,000 over the 45
/// labelled pages. `merge_along`'s transitive passes weld specks into a
/// page-spanning blob because screentone is *dense*, not because it is small,
/// so a 20-fold thinner field is a weaker weld whatever the survivors are.
///
/// **Why this is not a third attempt at the two that were refused.** L2A Task
/// 9 measured `above-floor` and `modal` against a bar fixed before either ran,
/// and refused both. Both changed which components the *median scale* was
/// taken over; neither changed which components were *smeared*. Screentone
/// stayed in `merge_along`'s input both times, so the mechanism was never
/// touched. This changes the smear input.
pub(crate) fn glyph_band(strokes: &[Bounds], height: u32, band: Band) -> Vec<Bounds> {
    if height == 0 {
        return Vec::new();
    }
    strokes
        .iter()
        .filter(|s| {
            let per_ten_thousand = u64::from(s.longer()) * 10_000 / u64::from(height);
            per_ten_thousand >= u64::from(band.lo) && per_ten_thousand <= u64::from(band.hi)
        })
        .copied()
        .collect()
}
```

Then split `detect_page` so the band is a parameter, and production supplies the
constant. **Task 7 sweeps `Band` and cannot do that if the value is welded in**,
and a sweep that reimplemented the pipeline would be the copy this repo forbids.

```rust
/// Every text block on one page, in page fractions.
pub fn detect_page(page: &GrayImage) -> Vec<Region> {
    detect_page_with(page, BAND)
}

/// `detect_page`, with the band as a parameter.
///
/// Exists so Task 7's sweep measures **this function** at several bands rather
/// than a second copy of it that could drift — the discipline
/// `region_experiment.rs`'s header states for `otsu`, `components`, `axis_of`
/// and `merge_along`.
pub(crate) fn detect_page_with(page: &GrayImage, band: Band) -> Vec<Region> {
    // ... the existing body, with the one insertion below.
}
```

In `detect_page_with`, immediately after `let strokes = components(page, otsu(page));`:

```rust
    // The band comes first, before the scale and before the smear. The scale
    // is meant to describe type, and the smear must never see the field it
    // welds. See `glyph_band` for what this does and — importantly — what it
    // does not do.
    let strokes = glyph_band(&strokes, height, band);
```

- [ ] **Step 4: Run the two new tests and the whole `detect` suite**

```bash
cargo test -p taiyaku-cli detect:: 2>&1 | tail -25
```

Expected: the two new tests PASS. **`the_gap_does_not_grow_with_the_page` is expected to FAIL** if Task 3 chose the page-fraction branch — its fixture holds strokes at 10px while the page grows to 2000×3000, where 10px is 0.33% of page height and falls under the band. Do not edit it here. Record the failure in the task report; Task 6 resolves it as spec §8 requires.

Every other existing test must pass.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/detect.rs
git commit -m "Band-pass the components before the smear, not after

The measured failure is that 63-96% of a page's components are sub-5px
screentone, and merge_along welds them transitively into one page-spanning
blob that MAX_BLOCK_AREA then discards. Two candidates have already been
refused for this, and both changed which components the median was taken
over rather than which were smeared, so the weld was never touched.

The band is measured from the components inside 478 hand-drawn boxes, and
expressed against page height because a glyph is a near-constant fraction
of the printed page while scan widths here span 685 to 5343px.

the_gap_does_not_grow_with_the_page now fails: its fixture deliberately
holds stroke size fixed while the page grows fivefold. That collision is
real rather than a stale test, and it is resolved next."
```

---

### Task 5: Phase 2b — smear both axes, union, dedupe

`axis_of` decides one axis for the whole page and stamps it on every region. The L2 design records this as a deviation it shipped without noting, and names the consequence: *a page carrying vertical dialogue beside a horizontal SFX or narration line cannot report both.* The dev set is 266 horizontal against 211 vertical, and the split is per box, not per edition.

**Files:**
- Modify: `crates/taiyaku-cli/src/detect.rs`

**Interfaces:**
- Consumes: `glyph_band` from Task 4.
- Produces: `detect_page` returning regions whose `axis` is the axis that actually produced them.

- [ ] **Step 1: Write the failing test**

```rust
    /// A vertical column beside a horizontal line must come back as two
    /// regions carrying different axes. One axis per page cannot express
    /// this, and no count can observe the difference — which is why the L2
    /// design's own correction names it and why this test is geometric.
    #[test]
    fn a_column_beside_a_row_keeps_both_axes() {
        let column: Vec<(u32, u32, u32, u32)> =
            (0..6).map(|i| (80u32, 100 + i * 60, 40, 40)).collect();
        let row: Vec<(u32, u32, u32, u32)> =
            (0..6).map(|i| (500u32 + i * 60, 700, 40, 40)).collect();
        let marks: Vec<_> = column.into_iter().chain(row).collect();
        let page = GrayImage::from_fn(1000, 1400, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        });
        let found = detect_page(&page);
        assert_eq!(found.len(), 2, "got {found:?}");
        let mut axes: Vec<Axis> = found.iter().map(|r| r.axis).collect();
        axes.sort_by_key(|a| format!("{a:?}"));
        assert_eq!(axes, vec![Axis::Horizontal, Axis::Vertical], "got {found:?}");
    }
```

- [ ] **Step 2: Run it to confirm it fails**

```bash
cargo test -p taiyaku-cli detect::tests::a_column_beside_a_row_keeps_both_axes 2>&1 | tail -15
```

Expected: FAIL — one axis is stamped on both regions, so `axes` comes back as two of the same.

- [ ] **Step 3: Replace the single smear with a two-axis union**

In `detect_page`, replace `let axis = axis_of(&strokes, gap);` and the `merge_along(&strokes, axis, gap)` chain with:

```rust
    // Both axes, unioned. `axis_of` picks one direction for the whole page and
    // stamps it on every region, which the L2 design records as a deviation it
    // shipped without noting: a page carrying vertical dialogue beside a
    // horizontal sound effect cannot report both. Smearing each way and
    // keeping what survives lets a page hold both, and makes `Region::axis`
    // true for the first time — `taiyaku_core::Axis` justifies storing it on
    // the grounds that "the clustering already knows it", which was not yet
    // true of this code.
    let mut blocks: Vec<(Bounds, Axis)> = Vec::new();
    for axis in [Axis::Vertical, Axis::Horizontal] {
        blocks.extend(merge_along(&strokes, axis, gap).into_iter().map(|b| (b, axis)));
    }

    // A column smeared horizontally comes back as its own pieces, each
    // contained in the vertical block that already covers them. Drop anything
    // wholly inside a *different* block: the survivor is the one whose axis
    // actually joined the strokes, which is exactly the axis to report.
    let contained = |inner: &Bounds, outer: &Bounds| {
        inner.x0 >= outer.x0 && inner.x1 <= outer.x1 && inner.y0 >= outer.y0 && inner.y1 <= outer.y1
    };
    let kept: Vec<(Bounds, Axis)> = blocks
        .iter()
        .enumerate()
        .filter(|(i, (block, _))| {
            !blocks.iter().enumerate().any(|(j, (other, _))| {
                // Strictly larger, or equal-and-earlier: an exact duplicate
                // from both axes keeps one copy rather than dropping both.
                j != *i
                    && contained(block, other)
                    && (other.width() * other.height() > block.width() * block.height()
                        || (other.width() * other.height() == block.width() * block.height()
                            && j < *i))
            })
        })
        .map(|(_, entry)| *entry)
        .collect();
```

Then the filter chain runs over `kept`, and the final `.map` uses each block's own axis:

```rust
    kept.into_iter()
        .filter(|(block, _)| strokes_in(block) >= MIN_STROKES)
        .filter(|(block, _)| block.longer() >= scale * MIN_BLOCK_STROKE_SPAN)
        .filter(|(block, _)| {
            let area = f64::from(block.width()) * f64::from(block.height());
            area / page_area <= f64::from(MAX_BLOCK_AREA)
        })
        .filter(|(block, _)| {
            let area = f64::from(block.width()) * f64::from(block.height());
            area > 0.0 && ink_in(block) as f64 / area >= f64::from(MIN_INK)
        })
        .map(|(block, axis)| {
            let x = f64::from(block.x0) / f64::from(width);
            let y = f64::from(block.y0) / f64::from(height);
            let w = f64::from(block.width()) / f64::from(width);
            let h = f64::from(block.height()) / f64::from(height);
            Region { x, y, w, h, axis }
        })
        .collect()
```

Keep the `#[expect(clippy::cast_precision_loss, ...)]` block on the `ink_in` filter exactly as it is — its reason text is a measured account of O34 and must not be dropped while moving the closure's parameter shape.

`axis_of` is now unused by `detect_page`. **Do not delete it** — `region_experiment.rs` calls it, and its tests pin the "text smears into fewer pieces along its own reading direction" rule that this change relies on.

- [ ] **Step 4: Run the whole `detect` suite**

```bash
cargo test -p taiyaku-cli detect:: 2>&1 | tail -25
```

Expected: the new test PASSES. `a_tie_reads_as_vertical` and `the_axis_is_the_direction_that_merges_furthest` still pass — they test `axis_of` directly, which is unchanged. `two_distant_columns_stay_two_regions` and `a_column_of_glyphs_is_one_vertical_region` must still pass; if either now returns an extra horizontal block, the containment rule is dropping the wrong side and needs fixing here rather than in a later task.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/detect.rs
git commit -m "Smear both axes and keep what survives, instead of picking one per page

axis_of decides one direction for the whole page and stamps it on every
region. The L2 design records that as a deviation L2a shipped without
noting, and names the cost: a page with vertical dialogue beside a
horizontal sound effect cannot report both. The dev set is 266 horizontal
against 211 vertical and the split is per box, not per edition.

Smearing each way and dropping blocks wholly contained in a block from the
other axis leaves each survivor carrying the axis that actually joined its
strokes. That also makes Region::axis true for the first time: Axis's own
doc justifies storing it because 'the clustering already knows it', which
was not yet true of the code.

axis_of stays: region_experiment calls it, and its tests pin the rule this
change relies on."
```

---

### Task 6: Phase 2c — constants cite measurements, and O32/O34 close

**Files:**
- Modify: `crates/taiyaku-cli/src/detect.rs`

- [ ] **Step 1: Resolve the `the_gap_does_not_grow_with_the_page` collision**

Spec §8. If Task 3 chose the page-fraction branch, that fixture fails because it holds strokes at 10px while the page grows to 2000×3000. It is load-bearing — the only test catching a gap tied to page width — so it is restated, not deleted, and a new test takes over what it protected:

```rust
    /// Two 5-stroke clusters at a fixed size and spacing **relative to the
    /// page**, on a canvas of any size.
    ///
    /// Restated on <!-- Task 6 date -->: this fixture used to hold stroke size
    /// at 10px while the page grew fivefold, which the measured glyph band now
    /// rejects on the large page — 10px is 0.33% of a 3000px page, below the
    /// band, and a component that size on a real 3000px scan is screentone
    /// rather than type. Scaling the marks with the page keeps the fixture
    /// testing what it was written to test.
    fn two_gapped_clusters(width: u32, height: u32) -> GrayImage {
        let unit = height / 60;
        let marks: Vec<(u32, u32, u32, u32)> = (0..5)
            .map(|k| (width / 4, unit * 4 + unit * 3 * k, unit * 2, unit * 2))
            .chain((0..5).map(|k| (width / 4, unit * 23 + unit * 3 * k, unit * 2, unit * 2)))
            .collect();
        GrayImage::from_fn(width, height, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        })
    }

    /// The smear gap is tied to the page's own strokes, never to its width.
    ///
    /// **This is what `detection_is_scale_invariant` cannot catch**, and the
    /// reason both exist: that fixture upsamples uniformly, so page size and
    /// stroke size move together and any gap linear in either passes it.
    ///
    /// The gap this pins is `stroke_scale * SMEAR_K` computed on the strokes
    /// present, so the two clusters stay apart at either canvas size.
    #[test]
    fn the_gap_does_not_grow_with_the_page() {
        let small = two_gapped_clusters(400, 600);
        let large = two_gapped_clusters(2000, 3000);
        let (a, b) = (detect_page(&small), detect_page(&large));
        assert_eq!(a.len(), 2, "small page: got {a:?}");
        assert_eq!(b.len(), 2, "large page: got {b:?}");
    }

    /// What the old fixture protected, kept as its own test: growing only the
    /// canvas — same strokes, same spacing, more empty margin — must not
    /// change how many blocks come back. A gap computed as a fraction of page
    /// width would weld the two clusters on the larger canvas.
    #[test]
    fn empty_margin_does_not_change_the_smear() {
        let marks: Vec<(u32, u32, u32, u32)> = (0..5)
            .map(|k| (100u32, 40 + 15 * k, 10, 10))
            .chain((0..5).map(|k| (100u32, 139 + 15 * k, 10, 10)))
            .collect();
        let page = |w: u32, h: u32| {
            GrayImage::from_fn(w, h, |x, y| {
                let inked = marks
                    .iter()
                    .any(|&(x0, y0, mw, mh)| x >= x0 && x < x0 + mw && y >= y0 && y < y0 + mh);
                Luma([if inked { 0 } else { 255 }])
            })
        };
        // The band is applied against page height, so both canvases must keep
        // these 10px marks inside it — 600 and 900 are close enough that they
        // do, which is what isolates the *gap* as the thing under test.
        let tight = merge_along(&components(&page(400, 600), 128), Axis::Vertical, 20).len();
        let loose = merge_along(&components(&page(400, 900), 128), Axis::Vertical, 20).len();
        assert_eq!(tight, loose, "the same strokes smear the same way");
        assert_eq!(tight, 2, "two clusters, not one");
    }
```

If Task 3 chose the per-page branch instead, `the_gap_does_not_grow_with_the_page` still passes untouched — skip this step and say so in the task report.

- [ ] **Step 2: Re-derive `MIN_STROKES`, `MIN_INK` and `MIN_BLOCK_STROKE_SPAN` from Task 3**

Replace each "**A starting value**" doc comment with the measurement. `MIN_INK` also changes what it counts — O34 records it summing overlapping bounding boxes, which is why 61 of 76 blocks scored above 1.0 against a floor of 0.05:

```rust
/// Least ink a block may contain, as a fraction of its own area.
///
/// **Measured**, and measured on the right quantity. This floor used to sum
/// the *bounding boxes* of the strokes inside a block, which double-counts
/// every overlap: O34 records 61 of 76 surviving blocks scoring above 1.0
/// against a floor of 0.05, and the filter rejecting 0 blocks on 0 of 90
/// sampled pages. It now counts ink pixels, the quantity
/// `truth_population.rs` measured inside 478 hand-drawn boxes, whose p05 is
/// <!-- Task 3 -->.
pub(crate) const MIN_INK: f32 = 0.0; // Task 3
```

and replace `ink_in` with a pixel count:

```rust
    // Ink pixels, not summed stroke boxes. See `MIN_INK` and O34.
    let ink_in = |block: &Bounds| -> u64 {
        let mut ink = 0u64;
        for y in block.y0..=block.y1 {
            for x in block.x0..=block.x1 {
                if page.get_pixel(x, y).0[0] <= threshold {
                    ink += 1;
                }
            }
        }
        ink
    };
```

This needs `threshold` in scope: change `let strokes = components(page, otsu(page));` to

```rust
    let threshold = otsu(page);
    let strokes = components(page, threshold);
```

The `#[expect(clippy::cast_precision_loss, ...)]` on that filter now needs a different reason — the sum is bounded by the block's own pixel area, which is the bound the old text said did *not* hold. Rewrite it to say so rather than leaving the old justification attached to new arithmetic.

- [ ] **Step 3: Update `MAX_BLOCK_AREA`'s doc comment for O32**

O32 is that `MAX_BLOCK_AREA` clamps every region below 0.15 before `median_area` is taken, so the design's `median_area > 0.15` disqualifier cannot fire. Record what the labels say about the ceiling itself:

```rust
/// Largest a block may be, as a fraction of page area.
///
/// **Measured against the labels, and kept.** Exactly 1 of 478 hand-drawn
/// truth boxes exceeds 0.15 — a full-page sound effect on
/// `kobayashi-v01/raw-mono` page 65, at 0.35 — so this ceiling excludes 0.2%
/// of the target. It is not what was costing recall.
///
/// **O32: it also makes `median_area` vacuous as an acceptance rule**, because
/// every region is clamped under 0.15 before the median is taken. That is why
/// the promotion rule is stated in 1:1 matches and spurious-per-page instead;
/// see `docs/superpowers/specs/2026-08-11-l2-detector-design.md` §1.
pub(crate) const MAX_BLOCK_AREA: f32 = 0.15;
```

- [ ] **Step 4: Run the whole workspace suite**

```bash
cargo test -p taiyaku-cli 2>&1 | tail -25
```

Expected: PASS, including both fixtures from Step 1 and every test from Tasks 4 and 5.

- [ ] **Step 5: Run the gate**

```bash
./scripts/check-all.sh --no-wasm 2>&1 | tail -20
```

Expected: green. Clippy pedantic is on; a new cast needs an `#[expect]` with a stated bound, and a module-level allow is forbidden by design §10.3.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/detect.rs
git commit -m "Constants that cite measurements, and MIN_INK counting the right thing

detect.rs's header has said since it was written that a constant here
asserted as measured remains a defect. Phase 1 makes the rule satisfiable
rather than relaxing it: every threshold now names the distribution behind
it, taken over the components inside 478 hand-drawn boxes.

MIN_INK also changes what it counts. O34 records it summing overlapping
stroke bounding boxes, so 61 of 76 surviving blocks scored above 1.0
against a floor of 0.05 and it rejected nothing on 90 sampled pages. It
counts ink pixels now, which is the quantity phase 1 measured.

O32 stays as a note rather than a change: 1 of 478 truth boxes exceeds
MAX_BLOCK_AREA, so the ceiling was never what cost recall - but it does
clamp every region under 0.15 before the median, which is why the
promotion rule is stated in 1:1 matches instead of median_area."
```

---

### Task 7: Score the candidate, and take the verdict

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`

- [ ] **Step 1: Score all nine editions**

```bash
cd /home/davey/projects/taiyaku
cargo build --release -p taiyaku-cli
./scripts/score-dev-set.sh | tee /tmp/score-candidate.txt
```

Expected: `editions scored: 9 of 9` and exit 0. If any edition errors the script fails loudly — a missing row is a measurement that did not happen, not a zero, and the promotion rule is a sum over all nine.

For the per-kind recall lines that Step 4 needs, re-run one edition at a time without the summary filter, e.g.:

```bash
./target/release/taiyaku score corpus/oshinoko-v01/raw-mono \
    --sidecar corpus/oshinoko-v01/taiyaku.json
```

- [ ] **Step 1b: Sweep the band, and fix `BAND` from the sweep**

`BAND` shipped provisional from Task 4. Fix it here, against a measurement, in a `#[cfg(test)]` corpus-gated module `crates/taiyaku-cli/src/band_sweep.rs` following `truth_population.rs`'s shape exactly — gated, skipping cleanly, asserting only that it measured something.

It scores `detect_page_with` at each band over all nine editions, reusing `score::{resolve_labelled, score_page}` and `crate::fingerprint::decode_all`:

```rust
/// Bands to try, in ten-thousandths of page height.
///
/// **`lo = 0` is the incumbent** — no band at all — and it is in the list on
/// purpose. A sweep whose worst option is still a change cannot tell you the
/// change helped. Phase 1 measured the outside population's p95 at 4px, which
/// is roughly 16 here, so the list brackets that by a factor of four either way.
const BANDS: [Band; 5] = [
    Band { lo: 0, hi: 2000 },
    Band { lo: 8, hi: 2000 },
    Band { lo: 16, hi: 2000 },
    Band { lo: 32, hi: 2000 },
    Band { lo: 64, hi: 2000 },
];
```

For each band, sum over the nine editions and print one row: `lo`, 1:1, merged, split, fragment, missed, spurious, spurious-per-page. Then set `BAND` in `detect.rs` to the row that wins under the promotion rule, and record the whole table — losers included — in `docs/notes.md`.

**Print every row and commit the table.** A sweep reported only by its winner is a constant asserted as measured with the evidence thrown away, which is the defect `detect.rs`'s header names.

**This is a one-parameter sweep over a fixed dev set, which is a mild overfit and is stated as one.** The held-out 30 pages are what checks it, and they are not opened here.

- [ ] **Step 2: Apply the promotion rule, exactly as written**

Sum `1:1` and `spurious` across the nine editions. The rule, frozen from here on:

> A candidate replaces the incumbent only if, over the dev set, it increases the count of 1:1 matches, and its spurious-per-page does not exceed the dev set's own truth-boxes-per-page (10.7). Ties go to the incumbent.

- Term 1: total `1:1` **> 2**.
- Term 2: total `spurious` / 45 **≤ 10.7**, i.e. total spurious ≤ 481.

Record both numbers. **If either term fails, the candidate is not promoted** — `detect.rs` is reverted to the incumbent on a branch that keeps the measurement, and the refusal is written up. That is a real outcome, and the two prior refusals are why it is written down here rather than assumed away.

- [ ] **Step 3: Update `docs/status.md`**

In the `taiyaku regions` proposes text blocks and does not reliably find them` section: keep the 0.4% baseline as the recorded prior figure — it is what the candidate is measured against — and add the candidate's table beside it. Update O31–O34 to whatever the work actually closed. Update the test count the way `scripts/check-all.sh` derives it rather than by hand.

- [ ] **Step 4: Append the verdict to `docs/notes.md`**

Per-kind recall matters more than the total here: `dialogue` is 390 of 480 boxes and `sfx` is 63, and a candidate that finds dialogue and no sound effects is a different result from one that finds a little of each. Record the per-kind table and say which misses remain, because that is the evidence spec §3.4 defers the C2 decision to.

- [ ] **Step 5: Run the gate and commit**

```bash
./scripts/check-all.sh --no-wasm 2>&1 | tail -20
git add docs/status.md docs/notes.md
git commit -m "The first candidate measured against ground truth, and what it did

<one line: promoted or refused, with the two numbers>

Per-kind recall is recorded beside the total because it is what decides
whether the remaining misses are balloon-shaped, which is the evidence the
design defers the C2 question to."
```

---

## Self-Review

**Spec coverage.** §1 promotion rule → Task 7 Step 2. §2 phase 1, all five measurements → Task 2 Step 2 (`inside_sides`/`outside_sides`, `per_box`, `ink`, `within_gap`, `between_gap`) and the membership rule → `centre_inside`. §3.1 band → Task 4. §3.2 two-axis smear → Task 5. §3.3 constants + O32 + O34 → Task 6. §3.4 C2 deferred → Task 7 Step 4 records the evidence. §4 phase 3 → out of scope, stated at the top. §5 held-out set → not opened, in Global Constraints. §6 files → File Structure, minus phase 3's. §7 testing, both load-bearing tests → Task 4 Step 1 and Task 5 Step 1. §8 the fixture collision → Task 3 Step 2 question 2 decides it, Task 6 Step 1 resolves it. §9 not-done → honoured. §10 unmeasured → Task 3 Step 2 questions 1 and 3 are the two that could refuse the whole approach.

**Gap found and closed:** the spec's §2 measurement list has five items and an earlier draft of Task 2 measured four — the between-box gap was missing, which is half of the load-bearing pair. Added.

**Type consistency.** `Labelled{edition,numbers,truth,pages}` and `resolve_labelled` (Task 1) are used with those exact field names in Task 2. `glyph_band(&[Bounds], u32) -> Vec<Bounds>` (Task 4) is called from `detect_page` in the same task and its output feeds Task 5's `merge_along`. `Bounds`, `Axis`, `Region`, `TruthRegion`, `merge_along`, `otsu`, `components` are all existing and unchanged in signature.

**Known-failing-by-design:** Task 4 Step 4 leaves `the_gap_does_not_grow_with_the_page` failing, and Task 6 Step 1 fixes it. That is deliberate and stated in both places, so a reviewer does not read it as a broken task. It is the one place in this plan where a task ends with a red test.
