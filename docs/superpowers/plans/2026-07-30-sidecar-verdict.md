# Sidecar Verdict Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a sidecar say whether its alignment is believable, so a reader can tell a worthless one from a good one without re-deriving it from the archives.

**Architecture:** A `quality` block joins `solver` on each `Alignment`, carrying the verdict and the two figures behind it. A new `Sidecar::quality_of` **recomputes** that block from the document's own runs rather than reading the stored copy, so a hand edit cannot leave a stale verdict standing; `Sidecar::trust` is the verdict-only view of it and composes along the hub path, taking the worst edge. `lookup` prints the result on every call.

**Tech Stack:** Rust edition 2024, workspace of two crates. `taiyaku-core` is I/O-free and must keep compiling for `wasm32-unknown-unknown`. No new dependencies — the set is `image`, `zip`, `rayon`, `serde`.

## Global Constraints

- **No new dependencies.** Nothing here needs one.
- **`taiyaku-core` stays WASM-clean.** No `std::fs`, no `std::time`, no threads. CI gates it: `cargo build -p taiyaku-core --target wasm32-unknown-unknown`.
- **`cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass** before every commit.
- **No `taiyaku` version bump.** `VERSION` stays 1. The change is additive; `sidecar.md` reserves bumps for changes that make an old reader *wrong* rather than merely incomplete.
- **British spelling in prose and comments.**
- **No attribution lines in commit messages.** No `Co-Authored-By`, no `Generated with`, no trailers.
- **Doc comments state the measured reason for a decision, citing corpus figures**, not the mechanism. Match the surrounding style.
- **Baselines must not move.** The eleven pairings in `corpus/README.md` and the four needle checks in its "Two traps" section are the regression net. This work adds a field and a query; it must change no alignment.
- Current test counts: **32 core, 15 CLI**.

## File Structure

| File | Responsibility | Change |
| ---- | -------------- | ------ |
| `crates/taiyaku-core/src/solve.rs` | similarity, calibration, DP, verdict | `Verdict` gains ordering and serde; shape helpers extracted |
| `crates/taiyaku-core/src/sidecar.rs` | the `taiyaku.json` format — parse, emit, resolve | `Quality`, `Alignment.quality`, `quality_of`, `trust` |
| `crates/taiyaku-core/src/lib.rs` | public surface of the core | export `Quality` |
| `crates/taiyaku-cli/src/main.rs` | CLI dispatch, `align`, `lookup` | populate `quality` on write; print trust in `lookup` |
| `docs/sidecar.md` | the format specification | document the block and the recompute rule |

No new files. `sidecar.rs` is 545 lines and gains roughly 80; that is within its one responsibility — it is the module that owns the format.

---

### Task 1: Give `Verdict` an order and a wire form, and extract the shape helpers

Groundwork with no behaviour change. Reviewable on its own: if the 32 core tests still pass, the extraction is faithful.

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (the `Verdict` enum and `verdict`)
- Modify: `crates/taiyaku-core/src/lib.rs` (exports)
- Test: `crates/taiyaku-core/src/solve.rs` `mod tests`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `Verdict` deriving `PartialOrd, Ord, Serialize, Deserialize` and serialising lowercase; `paired_slots(runs: &[Run]) -> u32`. Tasks 2 and 3 use both.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/taiyaku-core/src/solve.rs`:

```rust
    /// Composing an alignment through the hub is only as trustworthy as its
    /// weakest edge, so the three verdicts have to compare. The declaration
    /// order is the ranking, worst first, and `min` is what picks the weaker
    /// of two edges.
    #[test]
    fn verdicts_rank_from_refused_up_to_sound() {
        assert!(Verdict::Refused < Verdict::Fragmented);
        assert!(Verdict::Fragmented < Verdict::Sound);
        assert_eq!(Verdict::Sound.min(Verdict::Fragmented), Verdict::Fragmented);
        assert_eq!(Verdict::Fragmented.min(Verdict::Refused), Verdict::Refused);
    }

    /// The verdict goes into a JSON document, so it needs a wire form, and it
    /// must be the lowercase one the rest of the format already uses for
    /// `Source`.
    #[test]
    fn a_verdict_round_trips_through_lowercase_json() {
        assert_eq!(
            serde_json::to_string(&Verdict::Fragmented).unwrap(),
            "\"fragmented\""
        );
        assert_eq!(
            serde_json::from_str::<Verdict>("\"sound\"").unwrap(),
            Verdict::Sound
        );
    }

    /// A sidecar records how many slots found a counterpart, so the count has
    /// to be callable rather than buried inside `verdict`. Gap runs do not
    /// count towards it.
    #[test]
    fn paired_slots_counts_only_the_runs_that_found_a_counterpart() {
        let runs = vec![
            Run::paired(PageRange::new(0, 9), PageRange::new(0, 9)),
            Run::gap(PageRange::new(10, 19)),
            Run::paired(PageRange::new(20, 24), PageRange::new(10, 14)),
        ];

        assert_eq!(paired_slots(&runs), 15);
        assert_eq!(paired_slots(&[Run::gap(PageRange::new(0, 9))]), 0);
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p taiyaku-core verdicts_rank a_verdict_round_trips paired_slots_counts 2>&1 | tail -20`

Expected: FAIL to compile — `Verdict` does not implement `PartialOrd`, `Serialize` or `Deserialize`, and `paired_slots` does not exist.

- [ ] **Step 3: Add the derives**

In `crates/taiyaku-core/src/solve.rs`, replace:

```rust
/// A one-line verdict on an alignment as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
```

with:

```rust
/// A one-line verdict on an alignment as a whole.
///
/// The variants are declared worst-first, and the derived ordering is load
/// bearing: composing a lookup through the hub is only as trustworthy as the
/// weaker of its two edges, which `min` expresses directly.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
```

Leave the three variants and their doc comments exactly as they are.

- [ ] **Step 4: Extract the paired-slot count**

`verdict` opens by computing that count inline. A sidecar has to record the same
number, so lift it out. In the same file, replace the opening of `pub fn verdict`
— the `let paired: u32 = ...;` statement — so the function reads:

```rust
pub fn verdict(runs: &[Run], pages: usize) -> Verdict {
    let paired = paired_slots(runs);
    if pages == 0 || paired == 0 {
        return Verdict::Refused;
    }
    // Fragmentation is only meaningful once something was paired. The threshold
    // is capped by the edition's own length because it is measured in absolute
    // slots: an edition shorter than the constant could never reach it, and a
    // pairing that is one clean run averages exactly its own length, so it stays
    // `Sound` at any size.
    match average_run_length(runs) < MIN_AVERAGE_RUN.min(pages as f32) {
        true => Verdict::Fragmented,
        false => Verdict::Sound,
    }
}

/// How many slots of A the alignment found a counterpart for.
///
/// Public because a sidecar records it: a reader holding only the document
/// should be able to see how much of an edition was placed without opening the
/// archives.
pub fn paired_slots(runs: &[Run]) -> u32 {
    runs.iter()
        .filter(|r| r.b.is_some())
        .map(|r| r.a.len())
        .sum()
}
```

Keep `verdict`'s existing doc comment above it, unchanged — it still describes
exactly what the function does.

- [ ] **Step 5: Export it**

In `crates/taiyaku-core/src/lib.rs`, extend the `pub use solve::{...}` list so it reads, in alphabetical order:

```rust
pub use solve::{
    Calibration, PageConfidence, PageHash, Verdict, average_run_length, calibrate, confidence,
    paired_slots, similarity, solve, verdict,
};
```

- [ ] **Step 6: Run the whole suite**

Run: `cargo test --workspace 2>&1 | grep -E "^test result|FAILED|panicked at"`

Expected: **35 passed** in the core (32 plus your 3), **15 passed** in the CLI. If any pre-existing test fails, the extraction changed behaviour — report it rather than editing the test.

- [ ] **Step 7: Lints and WASM**

```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p taiyaku-core --target wasm32-unknown-unknown 2>&1 | tail -1
```

- [ ] **Step 8: Commit**

```sh
git add crates/taiyaku-core/src/solve.rs crates/taiyaku-core/src/lib.rs
git commit -m "Rank the verdicts, and give them a wire form

Composing a lookup through the hub is only as trustworthy as the weaker of
its two edges, so the three verdicts need to compare; the variants were
already declared worst-first, so the derived ordering says what was meant.
They also have to survive a round trip through JSON, in the lowercase form
the format already uses for a run's source.

paired_slots comes out of verdict because a sidecar records that count: a
reader holding only the document should see how much of an edition was
placed without opening the archives."
```

---

### Task 2: The `quality` block in the format

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs` (new `Quality`, field on `Alignment`)
- Modify: `crates/taiyaku-core/src/lib.rs` (export `Quality`)
- Modify: `crates/taiyaku-cli/src/main.rs` (populate it in `align --write`)
- Test: `crates/taiyaku-core/src/sidecar.rs` `mod tests`

**Interfaces:**
- Consumes: `Verdict` with serde and `paired_slots` from Task 1.
- Produces: `taiyaku_core::Quality { verdict: Verdict, slots: u32, paired: u32, avg_run: f32 }` and `Alignment.quality: Option<Quality>`. Task 3 recomputes this type; Task 4 prints from it.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/taiyaku-core/src/sidecar.rs`:

```rust
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
        assert_eq!(round_tripped.alignments[0].quality, sidecar.alignments[0].quality);
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
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p taiyaku-core -- an_alignment_carries_its_quality a_document_without_quality 2>&1 | tail -20`

Expected: FAIL to compile — no field `quality` on `Alignment`.

- [ ] **Step 3: Add the `Quality` struct**

In `crates/taiyaku-core/src/sidecar.rs`, add directly above `pub struct Alignment`:

```rust
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
    /// Slots in the `from` edition, which is the side runs are indexed by.
    pub slots: u32,
    /// How many of those slots found a counterpart.
    pub paired: u32,
    /// Mean length of the paired runs, in slots.
    pub avg_run: f32,
}
```

Then add the field to `Alignment`, after `solver`:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
```

`Verdict` needs importing into this module. Line 10 of the file currently reads:

```rust
use crate::{Direction, Half, Slot};
```

Change it to:

```rust
use crate::{Direction, Half, Slot, Verdict};
```

- [ ] **Step 4: Export it**

In `crates/taiyaku-core/src/lib.rs`, add `Quality` to the `pub use sidecar::{...}` list in alphabetical position.

- [ ] **Step 5: Run the tests and watch them pass**

Run: `cargo test -p taiyaku-core quality 2>&1 | grep -E "^test result|assertion"`

Expected: both pass. Every other construction of `Alignment` in the workspace now fails to compile for a missing field — the next step fixes the one that exists.

- [ ] **Step 6: Populate it when writing a sidecar**

In `crates/taiyaku-cli/src/main.rs`, inside the `Sidecar { ... }` literal in `fn align`, the `alignments` vector builds one `Alignment`. Add a `quality` field to it, directly after the `solver` field:

```rust
                // Computed here rather than left for a reader to derive: the
                // whole point of the document is that a reader should not need
                // the archives. `Sidecar::quality_of` recomputes it anyway, so
                // this is a summary for anything not using this library.
                quality: Some(taiyaku_core::Quality {
                    verdict: taiyaku_core::verdict(&runs, a_slots.len()),
                    slots: a_slots.len() as u32,
                    paired: taiyaku_core::paired_slots(&runs),
                    avg_run: taiyaku_core::average_run_length(&runs),
                }),
```

Everything here is fully qualified on purpose: `verdict` is **not** in this file's `use taiyaku_core::{...}` list — it was removed when `report_summary` switched to `stats::verdict_sentence` — and adding four names to that list to save four prefixes is not worth the churn. Do not "tidy" it by importing them.

If `Alignment` is constructed anywhere else in the workspace, add the field there too; `cargo build` will name every site.

- [ ] **Step 7: Verify against a real alignment and a real needle**

```sh
cargo build --release
S=/tmp/taiyaku-good.json
N=/tmp/taiyaku-needle.json
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz \
  --split-spreads --title "Go-Toubun no Hanayome" --name-a raw-mono --name-b en-mono --write "$S" 2>&1 | tail -1
./target/release/taiyaku align corpus/incoming/needles/op-xcalendar-2015 -- corpus/incoming/onepiece/op-raw-v105 \
  --split-spreads --title "not a real work" --name-a cal --name-b op --write "$N" 2>&1 | tail -1
python3 -c "import json;[print(f, json.load(open(f))['alignments'][0]['quality']) for f in ['$S','$N']]"
```

Expected: the first reports a `sound` verdict with `slots` 195 and `avg_run` around 64; the second reports `fragmented` with `slots` 26, `paired` 26 and `avg_run` about 1.2. **The needle pairing every slot it has is the defect this whole plan exists for — it should be visible in the file now.** Record both blocks verbatim in your report.

- [ ] **Step 8: Confirm no alignment moved**

```sh
T=./target/release/taiyaku
$T align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz --split-spreads 2>&1 | grep "unmatched slots"
$T align corpus/incoming/monster-pair/raw-v01 corpus/incoming/monster-pair/raw-v02 -- corpus/incoming/monster-pair/en-pe-v01.cbz --split-spreads 2>&1 | grep "unmatched slots"
$T align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/kaguya-v01/en-mono/*.cbz --split-spreads 2>&1 | grep "unmatched slots"
```

Expected, exactly:
```
5 runs | 180 whole pairings | 13 involving a spread half | 2 unmatched slots
13 runs | 425 whole pairings | 2 involving a spread half | 17 unmatched slots
1 runs | 0 whole pairings | 0 involving a spread half | 195 unmatched slots
```

- [ ] **Step 9: Full suite, lints, WASM, commit**

```sh
cargo test --workspace 2>&1 | grep -E "^test result|FAILED"
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p taiyaku-core --target wasm32-unknown-unknown 2>&1 | tail -1
git add crates/taiyaku-core/src/sidecar.rs crates/taiyaku-core/src/lib.rs crates/taiyaku-cli/src/main.rs
git commit -m "Record in the sidecar whether the alignment is believable

A sidecar written from the XCalendar needle - twenty-one runs across
twenty-six slots, every slot paired - was structurally identical to one
written from a correct alignment. A reader had no way to tell them apart
short of opening the archives, which is the work the document exists to
avoid.

The block sits on the alignment rather than inside the solver, because the
verdict is a function of the runs: a hand-corrected alignment has one too,
and a refusal can now say so rather than being inferred from an absent solver
block. The figures accompany the verdict because the threshold behind it is a
policy that has already changed once."
```

Expected: **37 passed** in the core (35 plus your 2), 15 in the CLI.

---

### Task 3: `quality_of` recomputes, and `trust` composes

The task the design turns on. A reviewer should be able to reject this one alone if the recompute rule is not honoured.

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs` (extract `slot_index`, move in `runs_of`, add two methods)
- Modify: `crates/taiyaku-cli/src/main.rs` (delete `runs_from_sidecar`, call the core method)
- Test: `crates/taiyaku-core/src/sidecar.rs` `mod tests`

**Interfaces:**
- Consumes: `Quality` from Task 2; `Verdict` and `paired_slots` from Task 1.
- Produces: `Sidecar::runs_of(&self, alignment: &Alignment) -> Result<Vec<Run>, String>`, `Sidecar::quality_of(&self, from: &str, to: &str) -> Option<Quality>`, `Sidecar::trust(&self, from: &str, to: &str) -> Option<Verdict>`. Task 4 calls the last two.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/taiyaku-core/src/sidecar.rs`:

```rust
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
            "lonely": { "label": "L", "members": [{ "path": "l.cbz", "sha256": "02", "pages": 4 }] }
          },
          "hub": "a",
          "alignments": [{ "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] }]
        }"#;
        let sidecar = Sidecar::from_json(json).expect("parses");

        assert_eq!(sidecar.trust("a", "nosuch"), None);
        assert_eq!(sidecar.trust("nosuch", "b"), None);
        // Defined, but nothing aligns it to anything.
        assert_eq!(sidecar.trust("b", "lonely"), None);
        // An edition corresponds to itself.
        assert_eq!(sidecar.trust("b", "b"), Some(Verdict::Sound));
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p taiyaku-core -- a_stored_verdict_is_ignored trust_composed trust_says_nothing 2>&1 | tail -20`

Expected: FAIL to compile — no method `quality_of` or `trust` on `Sidecar`.

- [ ] **Step 3: Extract the slot-index helper**

`resolve` defines an `index` closure that finds a slot's position. `quality_of` needs the same lookup, so lift it to a module-level function in `crates/taiyaku-core/src/sidecar.rs`, placed directly above `pub fn resolve`:

```rust
/// Where a slot sits in an edition's slot sequence.
///
/// A page reference with no half names the first slot of that page, which is
/// what makes `"7"` mean the same thing whether page 7 is whole or a spread.
fn slot_index(slots: &[Slot], slot: Slot) -> Option<usize> {
    slots
        .iter()
        .position(|s| s.page == slot.page && (slot.half.is_none() || s.half == slot.half))
}
```

Then in `resolve`, delete the local `index` closure and its comment, and replace each `index(&from_slots, ...)` and `index(&to_slots, ...)` call with `slot_index(...)`. The comment that was on the closure belongs on the new function — do not lose it; fold it into the doc comment above rather than leaving it orphaned.

- [ ] **Step 4: Move the RunEntry-to-Run conversion into the core**

`crates/taiyaku-cli/src/main.rs` holds `fn runs_from_sidecar` — 41 lines that
turn a stored alignment back into solver runs. It is pure format logic with one
caller, and its forward twin `entries_from_runs` already lives in the core, so
the two directions of the same conversion currently sit in different crates.
Move it, so `quality_of` can call the real statistics functions instead of
recomputing them by hand.

Add to the existing `impl Sidecar` block (the one holding `from_json` and
`to_json`), keeping the original doc comment and error messages:

```rust
    /// Rebuild solver runs from a stored alignment.
    ///
    /// The document addresses slots by page; the solver, the report and the
    /// verdict address them by index. This is the inverse of
    /// `entries_from_runs`, and it is what lets a hand-edited file drive a
    /// report or be judged.
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

        let at = |slots: &[Slot], want: Slot, side: &str| {
            slot_index(slots, want)
                .map(|i| i as u32)
                .ok_or_else(|| format!("{side} has no page {}", want.page))
        };

        alignment
            .runs
            .iter()
            .map(|entry| {
                let a = crate::PageRange::new(
                    at(&from_slots, entry.a.start, "the source edition")?,
                    at(&from_slots, entry.a.end, "the source edition")?,
                );
                Ok(match entry.b {
                    Some(b) => Run::paired(
                        a,
                        crate::PageRange::new(
                            at(&to_slots, b.start, "the target edition")?,
                            at(&to_slots, b.end, "the target edition")?,
                        ),
                    ),
                    None => Run::gap(a),
                })
            })
            .collect()
    }
```

Note it now calls the `slot_index` you extracted in step 3 rather than carrying
its own copy of the position search — that was the third copy of it.

`Run` and `PageRange` need to be in scope; add `Run` to the file's
`use crate::{...}` line alongside `Verdict` and refer to `PageRange` as
`crate::PageRange` as shown.

Then in `crates/taiyaku-cli/src/main.rs`, delete `fn runs_from_sidecar` entirely
and change its one call site — around line 197, inside `fn verify` — from
`runs_from_sidecar(&sidecar, alignment)?` to `sidecar.runs_of(alignment)?`.
Remove any import that becomes unused; clippy runs with `-D warnings`.

- [ ] **Step 5: Implement `quality_of` and `trust`**

Add to the same `impl Sidecar` block:

```rust
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
        let edge = self
            .alignments
            .iter()
            .find(|a| (a.from == from && a.to == to) || (a.from == to && a.to == from))?;
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
    /// Composition through the hub is only as good as its weaker edge, so the
    /// two are combined with `min` over the verdict ordering. A direct edge
    /// wins where the format carries one.
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
        if let Some(direct) = self.quality_of(from, to) {
            return Some(direct.verdict);
        }
        let first = self.quality_of(from, &self.hub)?.verdict;
        let second = self.quality_of(&self.hub, to)?.verdict;
        Some(first.min(second))
    }
```

- [ ] **Step 6: Run the whole suite**

Run: `cargo test --workspace 2>&1 | grep -E "^test result|assertion|panicked at"`

Expected: **40 passed** in the core (37 plus your 3), **15 passed** in the CLI.

If `a_stored_verdict_is_ignored...` fails with `verdict: Sound`, the implementation is reading the stored block — that is the one failure mode this task exists to prevent.

The CLI count must not move. `verify --sidecar` now calls the core method instead of its own copy; if a CLI test fails, the move changed behaviour and is wrong.

- [ ] **Step 7: Check `verify --sidecar` still works end to end**

The moved function has exactly one caller, and it is not covered by a unit test. Exercise it:

```sh
cargo build --release
S=/tmp/taiyaku-good.json
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz \
  --split-spreads --title "Go-Toubun no Hanayome" --name-a raw-mono --name-b en-mono --write "$S" >/dev/null 2>&1
./target/release/taiyaku verify corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz \
  --sidecar "$S" --out /tmp/report.html 2>&1 | tail -2
ls -l /tmp/report.html | awk '{print $5, "bytes"}'
```

Expected: it writes the report without error, a few megabytes in size. Report the output verbatim.

- [ ] **Step 8: Lints, WASM, commit**

```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p taiyaku-core --target wasm32-unknown-unknown 2>&1 | tail -1
git add crates/taiyaku-core/src/sidecar.rs crates/taiyaku-cli/src/main.rs
git commit -m "Judge a stored alignment by its runs, not by its own claim

quality_of recomputes from the runs in the document instead of reading the
block stored beside them. A hand-corrected alignment is a first-class citizen
of this format, so an edited run list with an unedited verdict next to it is
exactly the contradiction the document should not be able to express. The
stored block stays a summary for readers not using this library.

Recomputing needs the stored spans turned back into runs, which the CLI could
already do and the core could not - entries_from_runs lived here while its
inverse lived over there. The inverse moves in beside its twin, taking the
one caller with it, and the position search all three of them were doing
becomes a single function.

trust is the verdict-only view, composed through the hub and taking the worse
edge, because a lookup that crosses two alignments is only as good as the
weaker of them. It returns None where the document is silent, which is not
the same as sound."
```

---

### Task 4: `lookup` prints trust, and the format doc says so

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs` (`fn lookup`)
- Modify: `docs/sidecar.md`

**Interfaces:**
- Consumes: `Sidecar::quality_of` and `Sidecar::trust` from Task 3.
- Produces: no API. Behaviour and documentation.

- [ ] **Step 1: Print the trust line**

In `crates/taiyaku-cli/src/main.rs`, in `fn lookup`, directly after the existing `eprintln!` that prints the title and edition count, add:

```rust
    // Printed on every call rather than only on a bad one. A warning that
    // appears sometimes teaches people to skip it; a line that is always there
    // is read as part of the answer. Measured across twenty-two needles, seven
    // pair every page they have while being wholly unrelated, and the run
    // shape is the only thing that says so.
    match sidecar.quality_of(from, to) {
        Some(q) => eprintln!(
            "  alignment {from} -> {to}: {}, average run {:.1}",
            match q.verdict {
                Verdict::Refused => "REFUSED",
                Verdict::Fragmented => "FRAGMENTED",
                Verdict::Sound => "sound",
            },
            q.avg_run
        ),
        None => match sidecar.trust(from, to) {
            Some(v) => eprintln!("  alignment {from} -> {to}: {v:?}, composed through {}", sidecar.hub),
            None => eprintln!("  alignment {from} -> {to}: this document does not say"),
        },
    }
```

`Verdict` is **not** currently in this file's `use taiyaku_core::{...}` list — it was removed when `report_summary` switched to `stats::verdict_sentence` — so add it, in alphabetical position among the type names on the first line of that list.

- [ ] **Step 2: Verify against a good file and a needle**

```sh
cargo build --release
S=/tmp/taiyaku-good.json
N=/tmp/taiyaku-needle.json
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz \
  --split-spreads --title "Go-Toubun no Hanayome" --name-a raw-mono --name-b en-mono --write "$S" >/dev/null 2>&1
./target/release/taiyaku align corpus/incoming/needles/op-xcalendar-2015 -- corpus/incoming/onepiece/op-raw-v105 \
  --split-spreads --title "not a real work" --name-a cal --name-b op --write "$N" >/dev/null 2>&1
./target/release/taiyaku lookup "$S" raw-mono en-mono 47
./target/release/taiyaku lookup "$N" cal op 3
```

Expected shape — the first says `sound` with an average run in the tens and still resolves page 47 to the right half of English page 45, which is the answer `sidecar.md` records; the second says `FRAGMENTED` with an average run near 1.2 and still resolves, because `lookup` reports rather than refuses. Record both verbatim in your report.

- [ ] **Step 3: Check a document that predates the field**

```sh
python3 - <<'PY'
import json
d = json.load(open('/tmp/taiyaku-good.json'))
for a in d['alignments']:
    a.pop('quality', None)
json.dump(d, open('/tmp/taiyaku-old.json','w'))
PY
./target/release/taiyaku lookup /tmp/taiyaku-old.json raw-mono en-mono 47
```

Expected: it still resolves page 47 identically, and the trust line reports `sound` — because `quality_of` recomputes from the runs, which are still there. **This is worth understanding before you write it up:** a document that predates the field is not silent, since the evidence was always in it. `trust` returns `None` only when the *path* is missing, which is what Task 3's test pins.

- [ ] **Step 4: Document the block in `docs/sidecar.md`**

Three edits.

**(a)** In the `## Shape` example JSON, add a `quality` line to the alignment so the example matches what the tool now writes, keeping the real Go-Toubun figures: verdict `sound`, `slots` 195, `paired` 193, `avg_run` 64.3.

**(b)** Add a subsection under `## Definitions`, after the "Alignments and the hub" material, explaining: what `quality` holds; that the verdict is a function of the runs rather than of calibration, which is why it sits on the alignment rather than inside `solver`; that a reader using this library gets it recomputed by `Sidecar::trust`, so a hand edit cannot leave a stale claim standing; and that an absent block means unknown rather than good. Cite the measurement — twenty-two needles, seven of which pair every page they have while being unrelated, with run shape the only statistic that separates them. `notes.md` under "Twenty-two needles" is the reference.

**(c)** In `## Producing and consuming one`, show that `lookup` now prints the trust line, and say plainly that it reports rather than refuses.

Match the file's voice: plain declarative sentences, figures inline, British spelling, explicit about what is not known.

- [ ] **Step 5: Full suite, baselines, lints**

```sh
cargo test --workspace 2>&1 | grep -E "^test result|FAILED"
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p taiyaku-core --target wasm32-unknown-unknown 2>&1 | tail -1
T=./target/release/taiyaku
$T align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz --split-spreads 2>&1 | grep "unmatched slots"
$T align corpus/incoming/needles/op-xcalendar-2015 -- corpus/incoming/onepiece/op-raw-v105 --split-spreads 2>&1 | tail -1
```

Expected: 40 core and 15 CLI tests pass; the Go-Toubun baseline reads `5 runs | 180 whole pairings | 13 involving a spread half | 2 unmatched slots`; the needle still reads `FRAGMENTED - paired, but too broken up to trust | average run 1.2 slots`.

- [ ] **Step 6: Commit**

```sh
git add crates/taiyaku-cli/src/main.rs docs/sidecar.md
git commit -m "Say how far a lookup can be trusted, every time

lookup prints the verdict on every call rather than only a bad one. A warning
that appears sometimes teaches the reader to skip it, and this project has
just spent a week on a verdict that was computed correctly and never shown.

It reports rather than refuses: lookup exists to prove the format carries what
was solved, and a version that declined to answer on a fragmented file could
not demonstrate that on the very files most worth inspecting."
```

---

## Self-Review

**Spec coverage.** The `quality` block and its placement on `Alignment` → Task 2. The statistics accompanying the verdict → Task 2 step 3. `Sidecar::trust` and the recompute rule → Task 3, with the recompute pinned by its first test. Composition taking the worst edge → Task 3 step 1's second test. The edge-case table (unknown edition, `from == to`, direct edge, composed edge, missing leg, reversed edge) → Task 3's second and third tests plus the `trust` body. Absent `quality` meaning unknown → Task 2's second test and Task 4 step 3, which together show the subtlety that a pre-field document is still judgeable because the runs were always there. `lookup` printing on every call → Task 4. No version bump, `resolve` unchanged, `verify` untouched, refused pairs still written → all preserved by omission and asserted by the baselines in Tasks 2 and 4. The spec's two known limits are documented in Task 4 step 4(b).

**Placeholders.** None. Every code step carries its code; every doc step names the specific claims to make and the source to cite.

**Type consistency.** `Quality` has the same four fields wherever it appears — defined in Task 2 step 3, constructed in Task 2 step 6, returned in Task 3 step 5, read in Task 4 step 1. `quality_of` returns `Option<Quality>` and `trust` returns `Option<Verdict>` throughout, matching the spec. `Sidecar::runs_of` returns `Result<Vec<Run>, String>` in Task 3 step 4 and is consumed with `.ok()?` in step 5 and with `?` at the CLI call site. `paired_slots(&[Run]) -> u32` and `average_run_length(&[Run]) -> f32` are defined or already present in Task 1 and called against those signatures in Task 2 step 6 and Task 3 step 5. Test counts accumulate 32 → 35 → 37 → 40.

**One thing a reviewer should watch.** Task 3 moves `runs_from_sidecar` out of the CLI and into `Sidecar::runs_of`, which is what lets `quality_of` call the real statistics functions rather than recomputing them from spans. That move has exactly one existing caller — `verify --sidecar` — and no unit test covers it, which is why Task 3 step 7 exercises it end to end rather than trusting the compiler. The `slot_index` extraction in step 3 is load-bearing for the same reason: it collapses what would otherwise be three copies of the same position search into one.
