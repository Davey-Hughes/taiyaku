# Margin-Based Refusal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make taiyaku refuse a pair whose best matches carry no margin over their runner-ups, so a few art book pages against a whole volume stop coming back fully aligned.

**Architecture:** `calibrate_floor` currently returns `Option<f32>` and throws away the statistics it computed. It becomes `calibrate`, returning a `Calibration` struct carrying noise, signal, margin and floor, with an `accepted()` method holding the guards. A new `align --stats` prints that struct so a corpus survey is reproducible; the survey then fixes the threshold, and only after that does the margin guard start refusing anything.

**Tech Stack:** Rust edition 2024, workspace of two crates. `taiyaku-core` is I/O-free and must keep compiling for `wasm32-unknown-unknown`. No new dependencies — the existing set is `image`, `zip`, `rayon`, `serde`.

## Global Constraints

- **No new dependencies.** Nothing in this plan needs one.
- **`taiyaku-core` stays WASM-clean.** No `std::fs`, no `std::time`, no threads. There is a CI job that gates this: `cargo build -p taiyaku-core --target wasm32-unknown-unknown`.
- **`cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass** before every commit. CI runs both.
- **Doc comments state the measured reason, not the mechanism.** Every constant in `solve.rs` carries the corpus evidence that set it. Match that. Where a number is provisional, say so.
- **British spelling in prose and comments** — `colourisation`, `behaviour`, `normalise`.
- **No attribution lines in commit messages.** No `Co-Authored-By`, no `Generated with`.
- **The eleven baselines in `corpus/README.md` are the regression net.** They reproduce exactly today; verified 2026-07-30.
- **Shipping no margin guard is an allowed outcome.** If Task 3's survey shows no threshold separates good pairs from needles, Task 4 records that and stops. That is a result, not a failure.

## File Structure

| File | Responsibility | Change |
| ---- | -------------- | ------ |
| `crates/taiyaku-core/src/solve.rs` | similarity, calibration, DP, verdict, confidence | `Calibration` struct, `calibrate`, `average_run_length`, margin guard |
| `crates/taiyaku-core/src/lib.rs:20-22` | public surface of the core | swap `calibrate_floor` for `calibrate` and friends |
| `crates/taiyaku-cli/src/stats.rs` | **new** — formats the two `--stats` lines | created in Task 2 |
| `crates/taiyaku-cli/src/main.rs` | CLI dispatch, `align`, sidecar assembly | `mod stats`, `--stats` wiring, caller fix at line 581, usage text |
| `scripts/margin-survey.sh` | **new** — runs the survey over the corpus | created in Task 3 |
| `docs/notes.md` | the working log; every measurement lands here | survey results, threshold justification |
| `docs/status.md` | what is settled and what is next | item 1 resolved |

`stats.rs` is a new module rather than more of `main.rs` because `main.rs` is already the largest file in the workspace at 658 lines, and because formatting-as-a-pure-function is what makes the output testable — there is no test harness for CLI stdout in this project.

---

### Task 1: Expose the calibration, including margin, changing no behaviour

The refactor and the new statistic, with the guard deliberately **not** yet applied. Reviewable on its own: if the 27 core tests and 11 baselines are unchanged, the refactor is correct.

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs:61-94` (replace `calibrate_floor`), `solve.rs:123-129` (`solve` call site)
- Modify: `crates/taiyaku-core/src/lib.rs:20-22` (exports)
- Modify: `crates/taiyaku-cli/src/main.rs:581-586` (the one external caller)
- Test: `crates/taiyaku-core/src/solve.rs` `mod tests` (in-file, as all core tests are)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `taiyaku_core::Calibration { noise: f32, signal: f32, margin: f32, floor: f32 }`, `Calibration::separation(&self) -> f32`, `Calibration::accepted(&self) -> bool`, `taiyaku_core::calibrate(a: &[PageHash], b: &[PageHash]) -> Option<Calibration>`. Tasks 2 and 4 both use these. `calibrate_floor` ceases to exist.

- [ ] **Step 1: Add the test helper for flipping a chosen bit range**

In `crates/taiyaku-core/src/solve.rs`, inside `mod tests`, directly below the existing `degrade` function (around line 356):

```rust
    /// Flip `bits` bits starting at `start`, so two degradations can be made
    /// the same distance from a common original while differing from each
    /// other. `degrade` always flips from bit zero, which cannot express that.
    fn degrade_at(p: PageHash, start: u32, bits: u32) -> PageHash {
        let mut w = p.0;
        for k in start..start + bits {
            w[(k / 64) as usize] ^= 1 << (k % 64);
        }
        PageHash(w)
    }
```

- [ ] **Step 2: Write the failing test for the margin statistic**

Add to `mod tests` in `crates/taiyaku-core/src/solve.rs`:

```rust
    /// Margin is what does not inflate with the length of B. A row's best is a
    /// maximum over every candidate, so it climbs as candidates are added; the
    /// distance from that best to the runner-up does not. Calibration has to
    /// carry it for the guard that uses it to exist.
    #[test]
    fn the_margin_is_the_median_gap_from_a_row_s_best_to_its_runner_up() {
        // Every A page has a counterpart 8 bits away and a decoy 40 bits away,
        // so every row's margin is exactly (40 - 8) / 256.
        let a: Vec<PageHash> = (1..=3).map(h).collect();
        let b: Vec<PageHash> = a
            .iter()
            .map(|p| degrade(*p, 8))
            .chain(a.iter().map(|p| degrade(*p, 40)))
            .collect();

        let cal = calibrate(&a, &b).expect("both sides have pages");

        assert!((cal.margin - 32.0 / 256.0).abs() < 1e-6, "{cal:?}");
        assert!(cal.signal > 0.95, "{cal:?}");
        assert!(cal.noise < 0.60, "{cal:?}");
        // The floor is still the midpoint of noise and signal, unchanged.
        assert!((cal.floor - (cal.noise + cal.signal) / 2.0).abs() < 1e-6, "{cal:?}");
    }
```

- [ ] **Step 3: Run it and watch it fail**

Run: `cargo test -p taiyaku-core the_margin_is_the_median_gap 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'calibrate' in this scope`.

- [ ] **Step 4: Replace `calibrate_floor` with `calibrate`**

In `crates/taiyaku-core/src/solve.rs`, replace the whole of `calibrate_floor` (lines 30-94, i.e. the doc comment through the closing brace, keeping `GAP_PENALTY` above it and `MIN_SEPARATION`'s own doc comment) with:

```rust
/// What the two editions look like to each other, before any pairing is made.
///
/// The numbers are returned rather than collapsed to a yes-or-no because they
/// are the explanation for the answer. A refusal with no figures beside it is
/// not reviewable, and `align --stats` exists to print these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    /// Median of every cell in the similarity matrix. Most cells are
    /// non-matches, so this estimates the noise level. A median rather than a
    /// mean because blank pages score 1.0 against each other and drag a mean up.
    pub noise: f32,
    /// Median of each A page's best match.
    ///
    /// Deliberately the median, which refuses a pair where only part of A has
    /// counterparts — a whole volume against one of its own chapters, say —
    /// because most of A then really does match nothing and the median lands on
    /// noise. Estimating from a high percentile instead fixes that case and
    /// costs more than it buys: with the 90th percentile, Go-Toubun's raw
    /// against Kaguya's English release pairs 146 of 195 pages instead of being
    /// refused, and Kaguya's own alignment degrades from 8 runs and 204 pages
    /// paired to 12 and 201 as the raised floor rejects real matches.
    pub signal: f32,
    /// Median of each A page's distance from its best match to its runner-up.
    ///
    /// This is the statistic that does not inflate with the length of B. A
    /// row's best is a maximum over `|B|` candidates and climbs as candidates
    /// are added, for reasons unrelated to whether the editions correspond;
    /// the gap from that best to the second best does not. Four art book pages
    /// against a 210-page volume pair completely at margins of 0.000, -0.012,
    /// 0.020 and -0.047, two of them negative.
    pub margin: f32,
    /// The similarity below which a pairing is not worth making, midway
    /// between noise and signal.
    pub floor: f32,
}

impl Calibration {
    /// How far the signal stands clear of the noise.
    pub fn separation(&self) -> f32 {
        self.signal - self.noise
    }

    /// Whether any pairing between these two editions is worth making.
    pub fn accepted(&self) -> bool {
        self.separation() >= MIN_SEPARATION
    }
}

/// Measure the two editions against each other.
///
/// A fixed threshold cannot work. Measured across the corpus, correct matches
/// land anywhere from 0.65 to 0.98 depending on which pair of editions is being
/// compared — colourisation alone moves it 0.10 to 0.15 — while unrelated pages
/// sit near 0.5 throughout. What identifies a real match is therefore its
/// distance from that noise, not its absolute value.
///
/// Returns `None` only when a side has no pages at all.
pub fn calibrate(a: &[PageHash], b: &[PageHash]) -> Option<Calibration> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let mut all: Vec<f32> = Vec::with_capacity(a.len() * b.len());
    let mut best: Vec<f32> = Vec::with_capacity(a.len());
    let mut margins: Vec<f32> = Vec::with_capacity(a.len());
    for pa in a {
        let (mut row_best, mut row_second) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for pb in b {
            let s = similarity(*pa, *pb);
            all.push(s);
            if s > row_best {
                row_second = row_best;
                row_best = s;
            } else if s > row_second {
                row_second = s;
            }
        }
        best.push(row_best);
        // A single-page B has no runner-up, so the score stands alone. This
        // matches how `confidence` treats the same case rather than inventing
        // a second convention for it.
        margins.push(match row_second.is_finite() {
            true => row_best - row_second,
            false => row_best,
        });
    }
    let noise = median(&mut all);
    let signal = median(&mut best);
    Some(Calibration {
        noise,
        signal,
        margin: median(&mut margins),
        floor: (noise + signal) / 2.0,
    })
}
```

Leave `MIN_SEPARATION` and its doc comment exactly where they are, below this.

- [ ] **Step 5: Update `solve` to use it**

In `crates/taiyaku-core/src/solve.rs`, replace lines 127-129:

```rust
    let Some(floor) = calibrate_floor(a, b) else {
        return all_unmatched(a.len());
    };
```

with:

```rust
    let floor = match calibrate(a, b) {
        Some(cal) if cal.accepted() => cal.floor,
        _ => return all_unmatched(a.len()),
    };
```

- [ ] **Step 6: Update the core's exports**

In `crates/taiyaku-core/src/lib.rs`, replace lines 20-22:

```rust
pub use solve::{
    PageConfidence, PageHash, Verdict, calibrate_floor, confidence, similarity, solve, verdict,
};
```

with:

```rust
pub use solve::{
    Calibration, PageConfidence, PageHash, Verdict, calibrate, confidence, similarity, solve,
    verdict,
};
```

- [ ] **Step 7: Update the one external caller**

In `crates/taiyaku-cli/src/main.rs`, replace lines 581-586:

```rust
                solver: taiyaku_core::calibrate_floor(&a_slots_hashes, &b).map(|floor| {
                    SolverInfo {
                        hash: "dhash-16".to_string(),
                        floor,
                    }
                }),
```

with:

```rust
                // Only a calibration the solver accepted, so a refused pair
                // records no solver block rather than a floor that was never
                // used. `calibrate` reports its figures either way; `accepted`
                // is what decides whether they drove an alignment.
                solver: taiyaku_core::calibrate(&a_slots_hashes, &b)
                    .filter(taiyaku_core::Calibration::accepted)
                    .map(|cal| SolverInfo {
                        hash: "dhash-16".to_string(),
                        floor: cal.floor,
                    }),
```

- [ ] **Step 8: Run the new test and the whole suite**

Run: `cargo test --workspace 2>&1 | grep -E "^test result|FAILED|panicked"`

Expected: two `test result: ok` lines with **28 passed** in the core (27 existing plus the new one) and **9 passed** in the CLI. Zero failures. If any of the 27 moved, the refactor changed behaviour and is wrong.

- [ ] **Step 9: Check formatting, lints and the WASM target**

Run:
```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && \
  cargo build -p taiyaku-core --target wasm32-unknown-unknown
```

Expected: no output from the first two, `Finished` from the third.

- [ ] **Step 10: Verify two baselines still reproduce exactly**

Run:
```sh
cargo build --release
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- \
  corpus/gotoubun-v01/en-mono/*.cbz --split-spreads 2>&1 | tail -1
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- \
  corpus/kaguya-v01/en-mono/*.cbz --split-spreads 2>&1 | tail -1
```

Expected, exactly:
```
5 runs | 180 whole pairings | 13 involving a spread half | 2 unmatched slots
1 runs | 0 whole pairings | 0 involving a spread half | 195 unmatched slots
```

- [ ] **Step 11: Commit**

```sh
git add crates/taiyaku-core/src/solve.rs crates/taiyaku-core/src/lib.rs \
        crates/taiyaku-cli/src/main.rs
git commit -m "Return the calibration figures instead of discarding them

calibrate_floor collapsed noise, signal and separation into an Option<f32>,
so on a refusal every number that explained the refusal was computed and
thrown away. It now returns a Calibration carrying all of them, plus the
median margin from each row's best to its runner-up.

Margin is the statistic that does not inflate with the length of B, which
is what the needle-in-a-haystack case needs. Nothing gates on it yet;
accepted() still checks separation alone, so behaviour is unchanged and the
twenty-seven core tests and the corpus baselines all hold."
```

---

### Task 2: `align --stats` prints the calibration and the shape

**Files:**
- Create: `crates/taiyaku-cli/src/stats.rs`
- Modify: `crates/taiyaku-cli/src/main.rs:12-16` (module list), `main.rs:450-473` (the non-spread path in `align`), `main.rs:~530` (after the spread path solves), `main.rs:50-52` (usage text)
- Modify: `crates/taiyaku-core/src/solve.rs` (add `average_run_length`), `crates/taiyaku-core/src/lib.rs:20-22` (export it)
- Test: `crates/taiyaku-cli/src/stats.rs` `mod tests`, and `crates/taiyaku-core/src/solve.rs` `mod tests`

**Interfaces:**
- Consumes: `taiyaku_core::Calibration` and `calibrate` from Task 1.
- Produces: `taiyaku_core::average_run_length(runs: &[Run]) -> f32` (Task 5 uses it), and `stats::calibration_line(cal: Option<&Calibration>, a_len: usize, b_len: usize) -> String` plus `stats::shape_line(runs: &[Run], a_len: usize) -> String`.

- [ ] **Step 1: Write the failing test for average run length**

Add to `mod tests` in `crates/taiyaku-core/src/solve.rs`:

```rust
    /// Average paired-run length is the cheaper statement of what
    /// fragmentation measures, and the survey needs it to decide whether it is
    /// also the better one. Gap runs are excluded: a refused pair is one long
    /// gap, and counting it would make a total refusal look like the longest
    /// run in the corpus.
    #[test]
    fn average_run_length_counts_paired_runs_only() {
        let runs = vec![
            Run::paired(PageRange::new(0, 9), PageRange::new(0, 9)),   // 10 slots
            Run::gap(PageRange::new(10, 109)),                          // ignored
            Run::paired(PageRange::new(110, 129), PageRange::new(10, 29)), // 20 slots
        ];

        assert!((average_run_length(&runs) - 15.0).abs() < 1e-6);

        // A refusal has no paired run at all, and must not divide by zero.
        let refused = vec![Run::gap(PageRange::new(0, 209))];
        assert_eq!(average_run_length(&refused), 0.0);
    }
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p taiyaku-core average_run_length 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'average_run_length'`.

- [ ] **Step 3: Implement it**

In `crates/taiyaku-core/src/solve.rs`, add directly below `FRAGMENTED_RUNS_PER_HUNDRED` (after line 264):

```rust
/// Mean length of the paired runs, in slots.
///
/// Runs per hundred pages and this measure describe the same thing from
/// opposite ends, and this end may be the more legible one: measured on One
/// Piece Starter Book against volume 1, the genuine opening section was a
/// single run of 103 slots where the noise that followed ran 1.8 to 2.1.
///
/// Gap runs are excluded deliberately. A refused pair is one gap covering the
/// whole edition, and including it would report the longest run in the corpus
/// for the pairing that made none.
pub fn average_run_length(runs: &[Run]) -> f32 {
    let paired: Vec<u32> = runs.iter().filter(|r| r.b.is_some()).map(|r| r.a.len()).collect();
    match paired.is_empty() {
        true => 0.0,
        false => paired.iter().sum::<u32>() as f32 / paired.len() as f32,
    }
}
```

- [ ] **Step 4: Export it**

In `crates/taiyaku-core/src/lib.rs`, replace the `pub use solve::{...}` block with:

```rust
pub use solve::{
    Calibration, PageConfidence, PageHash, Verdict, average_run_length, calibrate, confidence,
    similarity, solve, verdict,
};
```

- [ ] **Step 5: Run the test and watch it pass**

Run: `cargo test -p taiyaku-core average_run_length 2>&1 | grep -E "^test result"`

Expected: `test result: ok. 1 passed`.

- [ ] **Step 6: Write the failing tests for the two output lines**

Create `crates/taiyaku-cli/src/stats.rs`:

```rust
//! The `--stats` lines: what calibration saw, and what shape came out.
//!
//! Formatting lives in its own module and returns strings rather than printing
//! them, because these two lines are the entire evidence base for two tuning
//! constants and a format nobody can test is a format that drifts.

use taiyaku_core::{Calibration, Run, Verdict, average_run_length, verdict};

#[cfg(test)]
mod tests {
    use super::*;
    use taiyaku_core::PageRange;

    #[test]
    fn the_calibration_line_reports_every_figure_that_decided_the_answer() {
        let cal = Calibration {
            noise: 0.518,
            signal: 0.856,
            margin: 0.213,
            floor: 0.687,
        };

        let line = calibration_line(Some(&cal), 195, 187);

        assert_eq!(
            line,
            "stats: a=195 b=187 noise=0.518 signal=0.856 sep=0.338 margin=0.213 \
             floor=0.687 accepted=yes"
        );
    }

    /// An empty edition has no calibration at all, and the line still has to
    /// say something a survey loop can parse rather than vanishing.
    #[test]
    fn an_uncalibratable_pair_still_produces_a_line() {
        let line = calibration_line(None, 3, 0);

        assert_eq!(line, "stats: a=3 b=0 no calibration: a side has no pages");
    }

    #[test]
    fn the_shape_line_reports_runs_coverage_and_both_fragmentation_measures() {
        // Go-Toubun volume 1 as actually solved: five runs, two of them gaps.
        let runs = vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
            Run::gap(PageRange::new(6, 6)),
            Run::paired(PageRange::new(7, 194), PageRange::new(5, 192)),
        ];

        let line = shape_line(&runs, 195);

        assert_eq!(
            line,
            "shape: runs=5 paired=193 avg_run=64.3 per_100=2.6 verdict=Sound"
        );
    }
}
```

- [ ] **Step 7: Run them and watch them fail**

Run: `cargo test -p taiyaku-cli stats:: 2>&1 | tail -20`

Expected: FAIL to compile — `stats.rs` is not yet a module of the crate, so nothing runs. Add `mod stats;` first (next step) and the failure becomes `cannot find function 'calibration_line'`.

- [ ] **Step 8: Register the module and implement both functions**

In `crates/taiyaku-cli/src/main.rs`, add to the module list at lines 12-16, in alphabetical position:

```rust
mod cache;
mod fingerprint;
mod naming;
mod pages;
mod report;
mod stats;
```

Then in `crates/taiyaku-cli/src/stats.rs`, insert these two functions between the `use` line and `mod tests`:

```rust
/// What calibration measured, whether or not it accepted the pair.
///
/// Fixed field order and three decimals throughout, so a survey can collect
/// dozens of these into a table with `grep` and read down a column.
pub fn calibration_line(cal: Option<&Calibration>, a_len: usize, b_len: usize) -> String {
    match cal {
        None => format!("stats: a={a_len} b={b_len} no calibration: a side has no pages"),
        Some(cal) => format!(
            "stats: a={a_len} b={b_len} noise={:.3} signal={:.3} sep={:.3} margin={:.3} \
             floor={:.3} accepted={}",
            cal.noise,
            cal.signal,
            cal.separation(),
            cal.margin,
            cal.floor,
            match cal.accepted() {
                true => "yes",
                false => "no",
            }
        ),
    }
}

/// What the alignment came out looking like.
///
/// Both fragmentation measures appear because the survey exists to choose
/// between them, and neither is meaningful without the coverage beside it: a
/// refused pair is one long gap run, which reads as excellent on any
/// runs-per-page scale.
pub fn shape_line(runs: &[Run], a_len: usize) -> String {
    let paired: u32 = runs.iter().filter(|r| r.b.is_some()).map(|r| r.a.len()).sum();
    let per_hundred = 100.0 * runs.len() as f32 / a_len.max(1) as f32;
    let call = match verdict(runs, a_len) {
        Verdict::Refused => "Refused",
        Verdict::Fragmented => "Fragmented",
        Verdict::Sound => "Sound",
    };
    format!(
        "shape: runs={} paired={paired} avg_run={:.1} per_100={per_hundred:.1} verdict={call}",
        runs.len(),
        average_run_length(runs)
    )
}
```

- [ ] **Step 9: Run them and watch them pass**

Run: `cargo test -p taiyaku-cli stats:: 2>&1 | grep -E "^test result|assertion"`

Expected: `test result: ok. 3 passed`. If `avg_run` disagrees, check the arithmetic: the three paired runs are 1, 4 and 188 slots, so the mean is 193/3 = 64.333.

- [ ] **Step 10: Wire the flag into both `align` paths**

In `crates/taiyaku-cli/src/main.rs`, at the top of `fn align` (line 450), after the `split_spreads` line:

```rust
fn align(args: &[String]) -> Result<(), String> {
    let (a_members, b_members) = split_editions(args)?;
    let split_spreads = args.iter().any(|a| a == "--split-spreads");
    let show_stats = args.iter().any(|a| a == "--stats");
```

In the non-spread path, replace:

```rust
        let runs = solve(&a, &b);
        report_summary(&runs, a.len());
        return Ok(());
```

with:

```rust
        let runs = solve(&a, &b);
        if show_stats {
            let cal = taiyaku_core::calibrate(&a, &b);
            eprintln!("{}", stats::calibration_line(cal.as_ref(), a.len(), b.len()));
            eprintln!("{}", stats::shape_line(&runs, a.len()));
        }
        report_summary(&runs, a.len());
        return Ok(());
```

In the spread path, replace `let runs = solve(&a_slots_hashes, &b);` with:

```rust
    let runs = solve(&a_slots_hashes, &b);
    if show_stats {
        let cal = taiyaku_core::calibrate(&a_slots_hashes, &b);
        eprintln!(
            "{}",
            stats::calibration_line(cal.as_ref(), a_slots_hashes.len(), b.len())
        );
        eprintln!("{}", stats::shape_line(&runs, a_slots_hashes.len()));
    }
```

Both go to stderr, which is where this file already sends every measurement — stdout carries the per-page mapping.

- [ ] **Step 11: Document the flag in the usage text**

In `crates/taiyaku-cli/src/main.rs`, replace lines 50-52:

```rust
            println!("align options: --split-spreads --ltr --write FILE --title NAME");
            println!("               --name-a KEY --name-b KEY");
```

with:

```rust
            println!("align options: --split-spreads --ltr --stats --write FILE --title NAME");
            println!("               --name-a KEY --name-b KEY");
```

- [ ] **Step 12: Verify against a real pair, and against a refusal**

Run:
```sh
cargo build --release
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- \
  corpus/gotoubun-v01/en-mono/*.cbz --split-spreads --stats 2>&1 | grep -E "^(stats|shape):"
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- \
  corpus/kaguya-v01/en-mono/*.cbz --split-spreads --stats 2>&1 | grep -E "^(stats|shape):"
```

Expected: two lines from each. The first pair reports `accepted=yes` and `verdict=Sound` with `runs=5`; the second reports `accepted=no`, `paired=0`, `avg_run=0.0` and `verdict=Refused`. Record the actual `margin=` figures — they are the first two rows of Task 3's table.

- [ ] **Step 13: Full suite, lints, WASM**

Run:
```sh
cargo test --workspace 2>&1 | grep -E "^test result" && cargo fmt --check && \
  cargo clippy --workspace --all-targets -- -D warnings && \
  cargo build -p taiyaku-core --target wasm32-unknown-unknown 2>&1 | tail -1
```

Expected: **29 passed** in the core, **12 passed** in the CLI, no lint output, `Finished`.

- [ ] **Step 14: Commit**

```sh
git add crates/taiyaku-cli/src/stats.rs crates/taiyaku-cli/src/main.rs \
        crates/taiyaku-core/src/solve.rs crates/taiyaku-core/src/lib.rs
git commit -m "Print what calibration saw, behind align --stats

Two lines per pair: what calibration measured, and what shape came out. The
figures are the whole evidence base for two tuning constants, so formatting
lives in its own tested module rather than in a println.

Both fragmentation measures appear side by side because choosing between
them is the point, and average run length arrives with them. It excludes gap
runs: a refused pair is one gap covering the edition, and counting it would
report the longest run in the corpus for the pairing that made none."
```

---

### Task 3: Survey the corpus and record the numbers

No production code. The deliverable is a committed script and a table in `notes.md` — the evidence Task 4 acts on.

**Files:**
- Create: `scripts/margin-survey.sh`
- Modify: `docs/notes.md` (append a section)

**Interfaces:**
- Consumes: `align --stats` from Task 2.
- Produces: the recorded table, and the decision of which of the three rules in the spec applies. Task 4 reads the chosen threshold from `notes.md`.

- [ ] **Step 1: Write the survey script**

Create `scripts/margin-survey.sh`:

```sh
#!/bin/sh
# Collect calibration figures for every pair in the corpus worth measuring.
#
# Settled pairs are discovered rather than listed, so a corpus that grows a
# volume is surveyed without editing this file. The incoming/ cases are named
# explicitly because each one exists to test a specific thing.
#
# Usage: scripts/margin-survey.sh [path/to/taiyaku]
set -eu

TAIYAKU="${1:-./target/release/taiyaku}"
test -x "$TAIYAKU" || { echo "no taiyaku at $TAIYAKU; cargo build --release" >&2; exit 1; }

# One row per pair: group, label, then the two sides as shell words.
row() {
    group=$1; label=$2; shift 2
    line=$("$TAIYAKU" align "$@" --split-spreads --stats 2>&1 | grep -E '^(stats|shape):' | tr '\n' ' ')
    printf '%-10s %-34s %s\n' "$group" "$label" "$line"
}

# The member for an edition directory: the archive inside it if there is one,
# otherwise the directory itself. Three of the four raw series are stored as
# loose images rather than archives, so both shapes have to work.
member() {
    for f in "$1"/*.cbz; do
        [ -f "$f" ] && { printf '%s\n' "$f"; return; }
    done
    printf '%s\n' "$1"
}

# --- confirmed good: every settled directory holding both a raw and an English
# --- edition. Discovered, so this needs no editing when the corpus grows.
#
# Named exactly, NOT globbed as raw-mono*: the sibling directories
# raw-mono-altcopy and raw-mono-BADCOPY exist, and a glob would file the known
# mismatched Oshi no Ko copy under `good` and quietly poison the threshold.
for dir in corpus/*-v[0-9][0-9]; do
    test -d "$dir/raw-mono" || continue
    test -d "$dir/en-mono" || continue
    row good "$(basename "$dir")" "$(member "$dir/raw-mono")" -- "$(member "$dir/en-mono")"
done

# --- confirmed good: the repagination, letterbox and identity cases
row good  "tongari-v01"        corpus/incoming/tongari/raw-v01 -- corpus/incoming/tongari/en-v01.cbz
row good  "tongari-v09-letterbox" corpus/incoming/tongari/raw-v09 -- corpus/incoming/tongari/en-v09.cbz
row good  "monster-2to1"       corpus/incoming/monster-pair/raw-v01 corpus/incoming/monster-pair/raw-v02 -- corpus/incoming/monster-pair/en-pe-v01.cbz
row good  "nausicaa-4to1"      corpus/incoming/nausicaa/raw-v01 corpus/incoming/nausicaa/raw-v02 corpus/incoming/nausicaa/raw-v03 corpus/incoming/nausicaa/raw-v04 -- corpus/incoming/nausicaa/en-deluxe-v01.cbz
row good  "op-v092-spread-repagination" corpus/incoming/op-duplicates/v092-mono-c0 -- corpus/incoming/op-duplicates/v092-mono-c1
row good  "bocchi-gaiden-real" corpus/incoming/bocchi/raw-v01-src-mangazip -- corpus/incoming/bocchi/en-gaiden-v01.cbz

# --- known bad: aligns, but wrongly
row bad   "oshinoko-v01-badcopy" corpus/oshinoko-v01/raw-mono-BADCOPY -- "$(member corpus/oshinoko-v01/en-mono)"
row bad   "op-v077-c0-vs-c1"     corpus/incoming/op-duplicates/v077-mono-c0 -- corpus/incoming/op-duplicates/v077-mono-c1

# --- needles: a short A against a long B. These must end up refused.
row needle "colorwalk-to-op-v105" corpus/incoming/colorwalk-pages -- corpus/incoming/onepiece/en-v105.cbz
# The `?` matches the space in "- v1 c1.cbz". A bare *c1.cbz would be fragile
# against a series numbering past c10, and this is one chapter, not ten.
row needle "one-chapter-to-volume" corpus/kaguya-v01/en-fan/*v1?c1.cbz -- corpus/kaguya-v01/raw-mono

# --- unrelated: already refused, and must stay so
row unrel "gotoubun-raw-vs-kaguya-en" corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/kaguya-v01/en-mono/*.cbz
row unrel "bocchi-gaiden-vs-main"     corpus/incoming/bocchi/raw-v01-src-mangazip -- corpus/incoming/bocchi/en-v01.cbz
```

- [ ] **Step 2: Make it executable and check the corpus paths resolve**

Run:
```sh
chmod +x scripts/margin-survey.sh
ls -d corpus/incoming/onepiece/en-v105.cbz corpus/incoming/op-duplicates/v077-mono-c0 \
      corpus/oshinoko-v01/raw-mono-BADCOPY corpus/incoming/colorwalk-pages
ls corpus/kaguya-v01/en-fan/*v1?c1.cbz
for dir in corpus/*-v[0-9][0-9]; do
    test -d "$dir/raw-mono" && test -d "$dir/en-mono" && echo "$dir"
done
```

Expected, verified 2026-07-30: all four paths present, the fan glob matching **exactly one** file (`… - v1 c1.cbz`), and the discovery loop printing exactly **twelve** directories — Go-Toubun, Kaguya, Kobayashi and Oshi no Ko at v01, v03 and v08. That is the twelve-volume set `notes.md` measures, so twelve is the number to expect and anything else means the corpus moved.

**If a path is missing, fix the script to name what is actually there before running the survey** — a row that silently fails is a hole in the evidence, and the whole point of step 4 is that holes are not allowed.

- [ ] **Step 3: Run the survey**

Run: `cargo build --release && scripts/margin-survey.sh 2>&1 | tee /tmp/margin-survey.txt`

Expected: one line per pair, each carrying both a `stats:` and a `shape:` fragment. Cold fingerprints for anything not already cached, so allow several minutes; Nausicaä is 600 slots and the Color Walk pages are 430 MB each.

- [ ] **Step 4: Check for rows that failed rather than measured**

Run: `grep -cE 'margin=' /tmp/margin-survey.txt; grep -vE 'margin=|no calibration' /tmp/margin-survey.txt`

Expected: the first number equals the row count; the second prints nothing. Any row without a `margin=` did not run — fix it and re-run before drawing a conclusion. A survey with silent holes is worse than no survey.

- [ ] **Step 5: Apply the decision rule from the spec**

Read off, from `/tmp/margin-survey.txt`:

- `GOOD_MIN` — the lowest `margin=` among the `good` rows, and which pair it is.
- `NEEDLE_MAX` — the highest `margin=` among the `needle` and `unrel` rows.

Then, per `docs/superpowers/specs/2026-07-30-margin-refusal-design.md` section 4:

1. If `GOOD_MIN > NEEDLE_MAX`, the threshold is `0.75 * GOOD_MIN`, **provided that value still exceeds `NEEDLE_MAX`**. Record the arithmetic.
2. If not, compute `margin / sep` for every row and repeat the test on that ratio. If the ratio separates, the guard is `margin >= MIN_MARGIN_FRACTION * separation` with the fraction at `0.75 *` the lowest good ratio.
3. If neither separates, the guard is not shippable. Record that and stop — Task 4 becomes a documentation-only task.

- [ ] **Step 6: Record the survey in `notes.md`**

Append to `docs/notes.md`, before the `## Provenance` section so the provenance table stays last:

```markdown
## The margin survey, and where the refusal threshold sits

Run with `scripts/margin-survey.sh` against the whole corpus. Every figure is
`align --stats` output; the script is committed so the table can be rebuilt.

[Paste the table here, columns: group, pair, |A|, |B|, noise, signal, sep,
margin, avg_run, per_100, verdict. Keep the raw numbers rather than rounding
them — the threshold is derived from them and a reader has to be able to
check the arithmetic.]

**The lowest good margin is [X], on [pair]. The highest needle margin is [Y],
on [pair].** [State whether rule 1, 2 or 3 applies, and the resulting
threshold with its arithmetic shown.]

**Kaguya was the canary and [did / did not] survive**, at a margin of [Z]
against a separation of [S] — the weakest pairing in the corpus by every
earlier measure, so it is what bounds the threshold from above.

**What the survey says about average run length.** [Whether it separates good
from bad at least as cleanly as runs per hundred pages, with the two columns
compared. This decides Task 5.]

**Honest limits.** The needle side of this table has [N] members against
[M] on the good side, so the threshold is anchored to the good cluster
rather than placed between the two. `|A|` is reported for every row because
a median over four margins is not the same evidence as a median over two
hundred.
```

- [ ] **Step 7: Commit the script and the numbers**

```sh
git add scripts/margin-survey.sh docs/notes.md
git commit -m "Survey the corpus for margins, and find where the threshold sits

Calibration figures for every pair worth measuring: the confirmed-good
pairings, the known-bad copies, the two needle cases and the unrelated pairs
that must stay refused. The script discovers the settled directories rather
than listing them, so a corpus that grows a volume is surveyed unchanged.

The table is the evidence for the threshold in the next commit, and for
whether average run length is the better fragmentation statistic."
```

---

### Task 4: Apply the margin guard

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (`accepted`, plus the new constant beside `MIN_SEPARATION`)
- Modify: `docs/status.md` (item 1 resolved)
- Test: `crates/taiyaku-core/src/solve.rs` `mod tests`

**Interfaces:**
- Consumes: `Calibration` and `accepted` from Task 1; the threshold from Task 3's `notes.md` section.
- Produces: no new API. `Calibration::accepted` changes meaning, which is the whole deliverable.

**If Task 3 reached rule 3 (nothing separates), skip steps 1-8 and do only step 9**, recording in `status.md` that the guard was measured and found unshippable, with the survey as the evidence.

- [ ] **Step 1: Write the failing test for the needle**

Add to `mod tests` in `crates/taiyaku-core/src/solve.rs`:

```rust
    /// A small needle against a large haystack always aligned, and coverage
    /// could not see it: the DP inserts gaps in B for free, so A comes back
    /// fully covered no matter how unrelated the two are. The four Color Walk
    /// pages measured against One Piece volume 105 paired completely at
    /// margins of 0.000, -0.012, 0.020 and -0.047, two of them negative.
    ///
    /// Modelled here as four A pages that resemble forty B candidates *equally
    /// well*. The similarity is high and the separation clears the old guard
    /// comfortably, which is exactly why counts could not catch this; only the
    /// absence of any margin distinguishes it.
    #[test]
    fn a_short_side_with_no_discrimination_is_refused() {
        let twin = h(1);
        // Forty candidates, each exactly twenty bits from the twin but in a
        // different place, so none of them is a better match than any other.
        let mut b: Vec<PageHash> = (0..40).map(|j| degrade_at(twin, j * 4, 20)).collect();
        // And a hundred and sixty unrelated pages, which is what puts the
        // noise median where a real volume's would be.
        b.extend((500..660).map(h));
        // Four A pages, each one bit from the twin, in a region none of B's
        // flips touch — so every A row scores identically against all forty.
        let a: Vec<PageHash> = (0..4).map(|i| degrade_at(twin, 200 + i, 1)).collect();

        let cal = calibrate(&a, &b).expect("both sides have pages");
        // The old guard is not what catches this, and the test would be
        // vacuous if it were.
        assert!(cal.separation() >= MIN_SEPARATION, "{cal:?}");
        assert!(cal.margin < 0.01, "{cal:?}");

        assert_eq!(solve(&a, &b), all_unmatched(a.len()));
    }
```

- [ ] **Step 2: Write the failing test that keeps the guard honest**

Add immediately after it:

```rust
    /// The guard must reject absence of discrimination, not shortness. Without
    /// this the test above is satisfiable by refusing every short A, which
    /// would be a worse bug than the one being fixed: aligning a handful of
    /// pages against the volume they came from is a thing the tool should do.
    #[test]
    fn a_short_side_that_genuinely_matches_still_aligns() {
        let a: Vec<PageHash> = (1..=4).map(h).collect();
        // Ten unrelated pages, then A's four real counterparts in order, then
        // a hundred and eighty-six more unrelated ones.
        let mut b: Vec<PageHash> = (500..510).map(h).collect();
        b.extend(a.iter().map(|p| degrade(*p, 40)));
        b.extend((600..786).map(h));

        let cal = calibrate(&a, &b).expect("both sides have pages");
        assert!(cal.margin > 0.20, "{cal:?}");

        assert_eq!(
            solve(&a, &b),
            vec![Run::paired(PageRange::new(0, 3), PageRange::new(10, 13))]
        );
    }
```

- [ ] **Step 3: Run both and watch the first fail**

Run: `cargo test -p taiyaku-core a_short_side 2>&1 | grep -E "^test |assertion|left|right"`

Expected: `a_short_side_that_genuinely_matches_still_aligns` passes already — it is regression cover, not driven behaviour. `a_short_side_with_no_discrimination_is_refused` FAILS on the final assertion, showing four paired runs where `all_unmatched` was expected. If it passes, the model is wrong and the test proves nothing: check that `cal.separation()` really does clear `MIN_SEPARATION`.

- [ ] **Step 4: Add the threshold constant**

In `crates/taiyaku-core/src/solve.rs`, directly below `MIN_SEPARATION` and its doc comment (after line 94), add **one** of the following, per the rule Task 3 selected.

For rule 1, a constant:

```rust
/// How much a best match must beat its runner-up before any pairing is
/// believed.
///
/// The guard the separation check cannot supply. A row's best is a maximum
/// over `|B|` candidates and therefore climbs with the length of B, so a
/// short side against a long one clears the separation bar on arithmetic
/// alone. The distance from that best to the second best does not climb, and
/// unrelated material has no reason to produce one.
///
/// Set from the corpus survey: the weakest confirmed-good pairing measures
/// MARGIN_OF_WEAKEST, and this sits at three quarters of it, which is
/// THRESHOLD. The highest needle measures NEEDLE_MAX, so the bar clears it by
/// HEADROOM. Anchored to the good cluster rather than placed midway because
/// that side has GOOD_COUNT members and the needle side has NEEDLE_COUNT. See
/// "The margin survey" in notes.md.
const MIN_MARGIN: f32 = THRESHOLD;
```

For rule 2, a ratio:

```rust
/// How much a best match must beat its runner-up, as a fraction of how far
/// the signal stands clear of the noise.
///
/// A fraction rather than a constant because the survey found the absolute
/// margins of weak and strong edition pairs overlapping, in the same way that
/// forced the floor itself to be calibrated per pair: a weak pairing has
/// compressed margins throughout and must be held to a proportionally weaker
/// bar. Set from the corpus survey — the weakest confirmed-good pairing
/// measures a ratio of RATIO_OF_WEAKEST, and this sits at three quarters of
/// it. See "The margin survey" in notes.md.
const MIN_MARGIN_FRACTION: f32 = FRACTION;
```

Replace every capitalised placeholder with the figure from `notes.md`, and delete whichever variant does not apply. **A capitalised word left in the source is an incomplete task** — the doc comment has to state the arithmetic that produced the number, as every other constant in this file does.

- [ ] **Step 5: Add the guard to `accepted`**

Replace `Calibration::accepted` with, for rule 1:

```rust
    /// Whether any pairing between these two editions is worth making.
    ///
    /// Two guards, catching different things. Separation rejects editions with
    /// no correspondence at all. Margin rejects a correspondence that is not
    /// discriminating — the case where a short A against a long B clears
    /// separation on arithmetic alone.
    pub fn accepted(&self) -> bool {
        self.separation() >= MIN_SEPARATION && self.margin >= MIN_MARGIN
    }
```

or, for rule 2, the same doc comment with:

```rust
    pub fn accepted(&self) -> bool {
        self.separation() >= MIN_SEPARATION
            && self.margin >= MIN_MARGIN_FRACTION * self.separation()
    }
```

- [ ] **Step 6: Run the two tests and the whole suite**

Run: `cargo test --workspace 2>&1 | grep -E "^test result|FAILED|panicked at"`

Expected: **31 passed** in the core (29 after Task 2, plus 2), **12 passed** in the CLI, zero failures.

The two tests that matter most are `a_uniformly_weak_pairing_still_aligns` and `a_pair_where_only_part_of_a_matches_is_refused`. They pull in opposite directions. If the weak-pairing test now fails, `MIN_MARGIN` is set above what a legitimately weak pair produces and the threshold is wrong — go back to the survey rather than weakening the test.

- [ ] **Step 7: Re-run all eleven baselines**

Run:
```sh
cargo build --release
T=./target/release/taiyaku
$T align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/raw-color/*.cbz --split-spreads 2>&1 | tail -1
$T align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz --split-spreads 2>&1 | tail -1
$T align corpus/kaguya-v01/raw-mono -- corpus/kaguya-v01/en-mono/*.cbz --split-spreads 2>&1 | tail -1
$T align corpus/oshinoko-v01/raw-mono -- corpus/oshinoko-v01/en-mono/*.cbz --split-spreads 2>&1 | tail -1
$T align corpus/incoming/tongari/raw-v09 -- corpus/incoming/tongari/en-v09.cbz --split-spreads 2>&1 | tail -1
$T align corpus/incoming/op-duplicates/v092-mono-c0 -- corpus/incoming/op-duplicates/v092-mono-c1 --split-spreads 2>&1 | tail -1
$T align corpus/incoming/monster-pair/raw-v01 corpus/incoming/monster-pair/raw-v02 -- corpus/incoming/monster-pair/en-pe-v01.cbz --split-spreads 2>&1 | tail -1
$T align corpus/incoming/nausicaa/raw-v0{1,2,3,4} -- corpus/incoming/nausicaa/en-deluxe-v01.cbz --split-spreads 2>&1 | tail -1
$T align corpus/incoming/bocchi/raw-v01-src-mangazip -- corpus/incoming/bocchi/en-v01.cbz --split-spreads 2>&1 | tail -1
$T align corpus/incoming/bocchi/raw-v01-src-mangazip -- corpus/incoming/bocchi/en-gaiden-v01.cbz --split-spreads 2>&1 | tail -1
$T align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/kaguya-v01/en-mono/*.cbz --split-spreads 2>&1 | tail -1
```

Expected, in order and exactly:
```
2 runs | 194 whole pairings | 0 involving a spread half | 1 unmatched slots
5 runs | 180 whole pairings | 13 involving a spread half | 2 unmatched slots
8 runs | 204 whole pairings | 5 involving a spread half | 4 unmatched slots
6 runs | 216 whole pairings | 6 involving a spread half | 1 unmatched slots
7 runs | 0 whole pairings | 176 involving a spread half | 4 unmatched slots
1 runs | 0 whole pairings | 200 involving a spread half | 0 unmatched slots
13 runs | 425 whole pairings | 2 involving a spread half | 17 unmatched slots
20 runs | 542 whole pairings | 11 involving a spread half | 47 unmatched slots
1 runs | 0 whole pairings | 0 involving a spread half | 185 unmatched slots
8 runs | 175 whole pairings | 7 involving a spread half | 3 unmatched slots
1 runs | 0 whole pairings | 0 involving a spread half | 195 unmatched slots
```

**Any deviation is a regression, and the fix is the threshold, not the baseline.**

- [ ] **Step 8: Confirm the real needle is now refused**

Run:
```sh
./target/release/taiyaku align corpus/incoming/colorwalk-pages -- \
  corpus/incoming/onepiece/en-v105.cbz --split-spreads --stats 2>&1 | grep -E "^(stats|shape):"
```

Expected: `accepted=no`, `paired=0`, `verdict=Refused`. This is the bug the whole plan exists to fix, measured on the material that exposed it.

- [ ] **Step 9: Update `status.md`**

In `docs/status.md`, delete the "**A small needle in a large haystack always aligns.**" paragraph from "What is known to be wrong or missing", and delete section "### 1. Refuse on margins, not just counts" from "What comes next", renumbering the two sections below it. Add to "What is settled":

```markdown
- **Two refusal guards, not one.** Separation rejects editions with no
  correspondence; margin rejects a correspondence that does not discriminate,
  which is the case a short side against a long one produces on arithmetic
  alone. Neither is sufficient by itself.
```

And add to "What has been proven against real material":

```markdown
| **A needle in a haystack** (Color Walk pages ↔ One Piece v105) | refused, on margins where counts could not |
```

If Task 3 reached rule 3 instead, replace all of the above with a rewritten "small needle" paragraph in "What is known to be wrong or missing", stating what was measured, that no threshold separated the cases, and pointing at the survey table.

- [ ] **Step 10: Lints, WASM, commit**

```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && \
  cargo build -p taiyaku-core --target wasm32-unknown-unknown 2>&1 | tail -1
git add crates/taiyaku-core/src/solve.rs docs/status.md
git commit -m "Refuse a pairing that discriminates nothing

A row's best match is a maximum over every candidate in B, so it climbs with
the length of B whether or not the editions correspond. Four art book pages
against a 210-page volume therefore cleared the separation guard and paired
completely, at margins of zero and below, and coverage could not object
because gaps in B are free.

Margin does not climb that way, being a difference within one row rather than
a maximum over it. Calibration now requires both, with the threshold taken
from the corpus survey and anchored to the confirmed-good cluster. The eleven
baselines are unchanged and the needle is refused."
```

---

### Task 5: Decide what to do about the fragmentation statistic

Acts on the average-run-length column of Task 3's survey. Separable from Task 4: a reviewer may accept the margin guard and reject this.

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (`verdict`, `FRAGMENTED_RUNS_PER_HUNDRED`, possibly a new constant)
- Modify: `docs/notes.md`, `docs/status.md`
- Test: `crates/taiyaku-core/src/solve.rs` `mod tests`

**Interfaces:**
- Consumes: `average_run_length` from Task 2; the survey's comparison from Task 3.
- Produces: no API change. `verdict`'s signature stays `(runs: &[Run], pages: usize) -> Verdict`.

- [ ] **Step 1: Read the survey's verdict on the two columns**

From the table in `notes.md`, compare `avg_run` against `per_100` as separators of `good` from `bad` rows. Average run length replaces runs-per-hundred **only if** its good and bad populations are at least as far apart, measured as the gap between the worst good row and the best bad row. Otherwise it stays a reported diagnostic and `FRAGMENTED_RUNS_PER_HUNDRED` keeps its job.

Both outcomes are real results. Record whichever holds.

- [ ] **Step 2a: If average run length wins — write the failing test**

Add to `mod tests` in `crates/taiyaku-core/src/solve.rs`:

```rust
    /// Average run length replaces runs per hundred pages as the fragmentation
    /// test, because the survey measured it separating the confirmed-good
    /// pairings from the known-bad copies by a wider gap. The hole at the
    /// bottom is unchanged and still gated by coverage: a refusal is one long
    /// gap run, so any runs-based measure reads it as excellent.
    #[test]
    fn fragmentation_is_judged_on_how_long_the_paired_runs_are() {
        // Go-Toubun volume 1: three paired runs averaging 64 slots.
        let sound = vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
            Run::gap(PageRange::new(6, 6)),
            Run::paired(PageRange::new(7, 194), PageRange::new(5, 192)),
        ];
        assert_eq!(verdict(&sound, 195), Verdict::Sound);

        // A mislabelled copy: seventy paired runs of three slots each.
        let fragmented: Vec<Run> = (0..70)
            .map(|i| {
                Run::paired(
                    PageRange::new(i * 3, i * 3 + 2),
                    PageRange::new(i * 3, i * 3 + 2),
                )
            })
            .collect();
        assert_eq!(verdict(&fragmented, 210), Verdict::Fragmented);

        // And a refusal is still Refused rather than the longest run measured.
        assert_eq!(verdict(&[Run::gap(PageRange::new(0, 209))], 210), Verdict::Refused);
    }
```

- [ ] **Step 2b: If average run length wins — run it, then swap the measure**

Run: `cargo test -p taiyaku-core fragmentation_is_judged 2>&1 | grep -E "^test |assertion"`

Note whether it already passes: with these shapes both measures agree, so it may. That makes it regression cover rather than driven behaviour — say so in the commit rather than pretending it drove the change.

Then in `verdict` (line 251), replace:

```rust
    match 100.0 * runs.len() as f32 / pages as f32 > FRAGMENTED_RUNS_PER_HUNDRED {
        true => Verdict::Fragmented,
        false => Verdict::Sound,
    }
```

with:

```rust
    match average_run_length(runs) < MIN_AVERAGE_RUN {
        true => Verdict::Fragmented,
        false => Verdict::Sound,
    }
```

and replace `FRAGMENTED_RUNS_PER_HUNDRED` and its doc comment with a `MIN_AVERAGE_RUN` constant whose doc comment states the survey figures that bound it from both sides, in the same shape as the constant it replaces.

- [ ] **Step 3a: If runs-per-hundred wins — tighten its doc comment instead**

Leave `verdict` alone. Replace the doc comment on `FRAGMENTED_RUNS_PER_HUNDRED` (lines 257-263) with one citing the survey: how many confirmed-good pairs now sit below it, how many known-bad above, whether the 8-to-22 band is still unpopulated, and that average run length was measured and did not separate more cleanly. A constant described as provisional when it has since been surveyed is a stale comment.

- [ ] **Step 4: Record the outcome in `notes.md`**

Extend the "margin survey" section with a short subsection naming which measure won and by what gap. This is the claim at `notes.md:1976` — that average run length "may be the better one to expose" — either confirmed or retired, and it should not be left standing as a guess once measured.

- [ ] **Step 5: Full suite, baselines, lints**

Run:
```sh
cargo test --workspace 2>&1 | grep -E "^test result|FAILED" && \
  cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && \
  cargo build --release
scripts/margin-survey.sh 2>&1 | grep -cE 'verdict=Sound'
```

Expected: all tests pass. The count of `verdict=Sound` rows must be at least as high as it was in Task 3's run — if changing the measure reclassified a confirmed-good pair as `Fragmented`, the new threshold is wrong.

- [ ] **Step 6: Commit**

```sh
git add crates/taiyaku-core/src/solve.rs docs/notes.md docs/status.md
git commit -m "Settle which measure judges fragmentation

The survey compared average paired-run length against runs per hundred pages
across the whole corpus. [State which separated the confirmed-good pairings
from the known-bad copies more cleanly, and by how much.]

Either way the claim in notes.md that average run length 'may be the better
one to expose' stops being a guess. The hole at the bottom is unchanged:
a refusal is one long gap run, and coverage still has to gate the measure."
```

---

## Self-Review

**Spec coverage.** Section 1 (`calibrate` returning its evidence) → Task 1. Section 2 (`align --stats`) → Task 2. Section 3 (the survey) → Task 3. Section 4 (the threshold rule, all three branches) → Task 3 step 5 and Task 4 step 4, with the rule-3 escape wired through Task 4's preamble and step 9. Section 5 (average run length) → Task 2 steps 1-5 for the function, Task 5 for the decision. Section 6 (tests) → Task 4 steps 1-2 for the two new behaviours, with the existing-27 and eleven-baseline checks appearing in every task that can break them. The spec's two risk statements are covered: the false-refusal risk by Task 4 step 6's instruction to fix the threshold rather than the test, and the small-`|A|` fragility by `|A|` being a recorded column in Task 3 step 6.

**Placeholders.** The capitalised words in Task 4 step 4 are the one deliberate exception, and they are values that cannot exist before Task 3 has run — the step says explicitly that leaving one in the source is an incomplete task. The bracketed passages in Task 3 step 6 and Task 5 step 6 are prose to be written from measured numbers, likewise labelled.

**Type consistency.** `Calibration` has the same four public fields wherever it appears (Task 1 step 4, Task 2 step 6's test). `calibrate` returns `Option<Calibration>` everywhere, and `calibration_line` therefore takes `Option<&Calibration>` with `cal.as_ref()` at both call sites. `accepted` is a method taking `&self` throughout, which is what makes `.filter(taiyaku_core::Calibration::accepted)` typecheck in Task 1 step 7. `average_run_length(&[Run]) -> f32` is defined in Task 2 step 3 and used unchanged in Task 2 step 8, Task 4's tests and Task 5 step 2b. Test counts accumulate consistently: 27 → 28 (Task 1) → 29 core and 12 CLI (Task 2) → 31 core (Task 4).
