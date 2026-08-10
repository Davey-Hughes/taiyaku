# Killing `taiyaku-core`'s mutation survivors — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the aligner's verification out of a NAS-backed corpus check and into tests the forge can run, by killing all 47 mutation survivors in `taiyaku-core`.

**Architecture:** No production code changes except where a survivor reveals dead or wrong code. Every task adds tests to an existing `#[cfg(test)] mod tests` in the file that holds the survivors, then re-runs `cargo-mutants` scoped to that file to prove the survivor count dropped. Equivalent mutants are dismissed **in writing**, with the argument, never silently.

**Tech Stack:** Rust 2024, `cargo-mutants` 27.1.0, `cargo-nextest`, existing `#[cfg(test)]` modules.

**Source of truth:** `docs/superpowers/specs/2026-08-10-mutation-testing-and-pedantic-design.md`, committed as `2cb7434`. Part 1 only. Part 2 (the pedantic lints) is a separate branch and a separate plan.

## Global Constraints

- **The CI-visible suite must stay fast.** It is 526 tests in 1.9 s under `cargo nextest run --workspace --all-features`, and the coverage job runs it. Fixtures stay small; no test may read the corpus.
- **No test may depend on `TAIYAKU_CORPUS`.** The whole point of the branch is that CI cannot reach it.
- **Every command in this plan runs with `env -u TAIYAKU_CORPUS`.** With the variable set, an unfiltered workspace test run also executes `geometry_experiment` in the debug profile — a run was once killed at 1h09m inside that one test.
- **`cargo mutants` requires `--gitignore=true`** on this repository. Without it, it copies the 26 GB gitignored `corpus/` into `/tmp` and dies with `No space left on device`.
- **`--output` must point outside the working tree**, or `mutants.out/` appears as an untracked directory. Task 1 gitignores it as a belt-and-braces measure; still pass `--output`.
- **Test style follows the file it lands in:** a doc comment on every test saying *why the behaviour matters*, not what the code does; a descriptive `snake_case` name that reads as a sentence. See `solve.rs`'s `a_refusal_is_not_mistaken_for_a_clean_alignment`.
- **`debug_assert!` fires in the test profile.** `PageRange::new(start, end)` panics on an inverted range and `Run::paired` panics on unequal side lengths. Fixtures must respect both.
- **Commit after every task.** No attribution trailers in commit messages.

## The 47 survivors

Recorded from the baseline run of 2026-08-10 (1,660 mutants, 43 min). Five `taiyaku-core` **timeouts** are tracked separately and are not among the 47: `solve.rs:227` and `credits.rs:93` ×3, `credits.rs:94`.

| file | survivors | task |
| ---- | --------- | ---- |
| `solve.rs` | 26 | 2, 3, 4, 5, 6 |
| `sidecar.rs` | 11 | 8, 9 |
| `credits.rs` | 7 | 7 |
| `slot.rs` | 2 | 7 |
| `lib.rs` | 1 | 7 |

## The verification loop, used by every task

Each task ends by re-running mutants scoped to one file and comparing the survivor count to the number this plan states. The command:

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/<FILE>.rs \
  --output /tmp/mut-<FILE>
```

Read the survivors with:

```bash
cat /tmp/mut-<FILE>/mutants.out/missed.txt
```

**A task is not done because tests pass. It is done when `missed.txt` shrank by the stated amount.** That distinction is the entire reason this branch exists.

---

### Task 1: Record the baseline and document the tool

**Files:**
- Modify: `.gitignore`
- Modify: `docs/ci.md` (append a section)
- Modify: `docs/notes.md` (append a section)
- Modify: `docs/status.md` (add an open item)

**Interfaces:**
- Consumes: nothing.
- Produces: the documented invocation every later task's verification loop uses.

- [ ] **Step 1: Gitignore the tool's output directory**

Add to `.gitignore`, after the existing generated-output block:

```gitignore
# cargo-mutants scratch output. Always pass --output to keep it out of the
# tree anyway; this is the second line of defence, not the first.
mutants.out/
```

- [ ] **Step 2: Verify the ignore takes**

```bash
mkdir -p mutants.out && git status --porcelain | grep mutants.out; echo "exit=$?"
```

Expected: no output from `grep`, `exit=1`. Then `rmdir mutants.out`.

- [ ] **Step 3: Document the invocation in `docs/ci.md`**

Append a section titled `## Mutation testing`. It must state, in the register of the rest of that file:

- The command: `env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest --output <dir>`.
- **Why `--gitignore=true` is not optional here**, with the failure quoted:
  `Error: Failed to copy .../corpus/incoming/needles/colorwalk-comp2/cwc2-310.tif` / `Caused by: No space left on device (os error 28)`. `cargo-mutants` does not honour `.gitignore` by default; `corpus/` is gitignored and is 26 GB, including archival TIFFs at 430 MB a page. Nothing in the error names the corpus, and the obvious misreading is that the machine is short of disk.
- **Why `env -u TAIYAKU_CORPUS`**: see the Global Constraints above.
- That this is **not a CI job and not a gate**, and why: the numbers to size a threshold do not exist yet, and a threshold chosen before its baseline is a number picked to be passed. Point at the spec's §5.

- [ ] **Step 4: Record the baseline in `docs/notes.md`**

Append a section titled `## The mutation baseline, and the minus sign nothing was testing`. It must record, as measurements:

- 1,660 mutants in 43 min: 1,149 caught, 346 missed, 136 unviable, 29 timed out. 75.4% of 1,524 viable mutants caught.
- The per-crate table: `taiyaku-cli` 752/478/217, `taiyaku-core` 460/370/47, `taiyaku-serve` 304/217/52, `taiyaku-read` 93/56/17, `taiyaku-wasm` 51/28/13.
- The per-file table for `taiyaku-core`: `solve.rs` 26, `sidecar.rs` 11, `credits.rs` 7, `slot.rs` 2, `lib.rs` 1.
- **The finding**: the gap penalty at `solve.rs:188` can be deleted — turning a penalty into a bonus — and all 526 CI-visible tests still pass. Applied by hand and run against the twelve corpus checks under `--release`, exactly one fails: `corpus::composing_through_the_hub_agrees_with_a_direct_solve`, at 11 passed / 1 failed in 15.6 s.
- That `solve.rs` alone missed 19% of its viable mutants against the workspace's 23%, so it is representative rather than an outlier.
- That the estimate this replaces — "1,000+ mutants at 10–30 s each, hours" — was wrong on count and an order of magnitude wrong on cost, because the suite is 526 tests in 1.9 s and nearly all of a mutant's cost is the incremental rebuild.

- [ ] **Step 5: Add the open item to `docs/status.md`**

`docs/status.md` is authoritative for next steps. Add an item under "What is known to be wrong or missing" naming the 346 workspace survivors, stating that this branch addresses `taiyaku-core`'s 47 only, and listing the other four crates with their counts as measured-but-not-addressed.

- [ ] **Step 6: Commit**

```bash
git add .gitignore docs/ci.md docs/notes.md docs/status.md
git commit -m "Mutation baseline: 1,660 mutants, 43 min, 346 survivors

The gap penalty at solve.rs:188 can be turned into a gap bonus and all 526
CI-visible tests still pass. Exactly one of the twelve corpus checks
catches it, and CI cannot reach the corpus.

Records the invocation too, including why --gitignore=true is not optional:
without it cargo-mutants copies 26 GB of gitignored corpus into /tmp and
dies on disk, and the error names neither the corpus nor the ignore file."
```

---

### Task 2: `solve.rs` — `percentile` and `calibrate` (6 survivors)

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (test module at line 467)

**Interfaces:**
- Consumes: `h(seed) -> PageHash`, `degrade(p, bits) -> PageHash`, `degrade_at(p, start, bits) -> PageHash` — existing private test helpers at `solve.rs:513`, `:527`, `:538`.
- Produces: nothing later tasks depend on.

**Survivors:**

```
solve.rs:106:18  replace > with >= in calibrate
solve.rs:109:25  replace > with >= in calibrate
solve.rs:153:36  replace - with + in percentile
solve.rs:153:36  replace - with / in percentile
solve.rs:154:32  replace - with + in percentile
solve.rs:154:32  replace - with / in percentile
```

- [ ] **Step 1: Write the failing test for `percentile`**

Add to `mod tests` in `crates/taiyaku-core/src/solve.rs`:

```rust
/// `percentile` is what turns the similarity matrix into `Calibration`'s
/// three medians, and every acceptance decision rests on those. The
/// index arithmetic is off-by-one-prone in both directions: the `- 1.0`
/// is what makes fraction 1.0 select the last element rather than run
/// past the end, and fraction 0.0 the first.
#[test]
fn percentile_selects_by_position_across_the_whole_range() {
    let mut values = [0.0f32, 10.0, 20.0, 30.0, 40.0];

    assert_eq!(percentile(&mut values.clone(), 0.0), 0.0);
    assert_eq!(percentile(&mut values.clone(), 0.5), 20.0);
    assert_eq!(percentile(&mut values.clone(), 1.0), 40.0);

    // Unsorted input must give the same answer: the function sorts.
    let mut shuffled = [30.0f32, 0.0, 40.0, 10.0, 20.0];
    assert_eq!(percentile(&mut shuffled, 0.5), 20.0);

    // A single value is its own median, and its own every percentile.
    assert_eq!(percentile(&mut [7.0f32], 0.0), 7.0);
    assert_eq!(percentile(&mut [7.0f32], 1.0), 7.0);

    // An even count has no middle element; the round is what picks a side.
    assert_eq!(percentile(&mut [0.0f32, 10.0, 20.0, 30.0], 0.5), 20.0);
    assert_eq!(median(&mut [0.0f32, 10.0, 20.0, 30.0]), 20.0);
}
```

- [ ] **Step 2: Run it and confirm it passes against unmutated code**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core percentile_selects
```

Expected: PASS. This test pins existing behaviour; it is not TDD-red-first, because the behaviour is already correct and the defect is that nothing asserted it. **If it fails, the assertion is wrong, not the code — fix the assertion.**

- [ ] **Step 3: Confirm it kills the `153:36` mutants by hand**

```bash
sed -i '153s/values.len() as f32 - 1.0/values.len() as f32 + 1.0/' crates/taiyaku-core/src/solve.rs
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core percentile_selects; echo "exit=$?"
git checkout -- crates/taiyaku-core/src/solve.rs
```

Expected: FAIL, `exit=1`. If it passes, the test does not discriminate and must be strengthened before moving on.

- [ ] **Step 4: Triage the `154:32` mutants**

Line 154 is `values[at.min(values.len() - 1)]`. `at` is `((len - 1) * fraction).round()`, so for any `fraction <= 1.0` it is already `<= len - 1` and **the clamp is unreachable**. Both its mutants (`- 1` → `+ 1`, `- 1` → `/ 1`) are therefore **equivalent mutants**: they change a no-op into a different no-op.

Confirm the only callers pass `fraction <= 1.0`:

```bash
grep -rn "percentile(" crates/ --include=*.rs
```

Then take one of two actions and record which in the commit message:

- **If every caller passes `0.0..=1.0`:** delete the clamp, replacing line 154 with `values[at]`, and add to the doc comment above `percentile` a sentence saying `at` cannot exceed the last index because `fraction` is bounded, so no clamp is needed. This kills both mutants by removing the code they mutate.
- **If any caller could pass more than 1.0:** keep the clamp and add a test that exercises it, e.g. `assert_eq!(percentile(&mut [1.0f32, 2.0], 5.0), 2.0)`.

- [ ] **Step 5: Write the failing test for `calibrate`'s runner-up tracking**

Lines 106 and 109 are the `s > row_best` / `s > row_second` comparisons that find each A-row's best and second-best score. Turning either into `>=` matters only when two candidates tie exactly — which is the chapter-divider case `confidence`'s existing test already describes, and which `Calibration::margin` exists to measure.

```rust
/// `margin` is the one calibration statistic that does not inflate with
/// the length of B, and it is the difference between a row's best score
/// and its runner-up. Exact ties are not hypothetical: chapter dividers
/// share a template, and two identical pages in B score identically
/// against the same page of A. When that happens the margin is zero, and
/// a `>=` in either comparison would let the runner-up overwrite the best
/// or itself and report a margin that is not zero.
#[test]
fn an_exact_tie_in_b_leaves_no_margin() {
    let page = h(1);
    // Two pages of B that are byte-identical, so both score exactly 1.0
    // against A's single page, plus one unrelated page.
    let b = vec![page, page, h(99)];
    let a = vec![page];

    let cal = calibrate(&a, &b).expect("both sides are non-empty");

    assert_eq!(cal.signal, 1.0, "{cal:?}");
    assert_eq!(cal.margin, 0.0, "{cal:?}");
}

/// The complement: with no tie, the margin is the real distance from best
/// to runner-up, and it must not be zero. Without this the test above
/// passes against an implementation that always reports zero.
#[test]
fn a_clear_winner_leaves_a_margin_of_the_real_distance() {
    let page = h(1);
    // 26 of 256 bits => the runner-up scores 1.0 - 26/256 = 0.8984.
    let b = vec![page, degrade(page, 26), h(99)];
    let a = vec![page];

    let cal = calibrate(&a, &b).expect("both sides are non-empty");

    assert_eq!(cal.signal, 1.0, "{cal:?}");
    assert!((cal.margin - 26.0 / 256.0).abs() < 1e-6, "{cal:?}");
}
```

- [ ] **Step 6: Run both and confirm they pass**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core -E 'test(tie) + test(clear_winner)'
```

Expected: 2 passed.

- [ ] **Step 7: Confirm they kill the `106`/`109` mutants by hand**

```bash
sed -i '106s/if s > row_best/if s >= row_best/' crates/taiyaku-core/src/solve.rs
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core -E 'test(tie)'; echo "exit=$?"
git checkout -- crates/taiyaku-core/src/solve.rs
```

Expected: FAIL, `exit=1`.

Repeat for line 109 (`} else if s > row_second` → `>=`). **If the 109 mutant survives both new tests, say so in the commit message and leave it for Task 10's triage** — do not invent a test that only exists to kill it.

- [ ] **Step 8: Verify against the tool**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/solve.rs --output /tmp/mut-solve
grep -cE "solve\.rs:(106|109|153|154):" /tmp/mut-solve/mutants.out/missed.txt
```

Expected: `0` if Step 4 deleted the clamp, or `2` if it kept it and the clamp test could not be written. Any other number means a mutant this task claimed is still alive.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-core/src/solve.rs
git commit -m "Pin percentile's index arithmetic and calibrate's runner-up

Six survivors. percentile's -1.0 is what makes fraction 1.0 select the
last element; nothing asserted it. calibrate's two > comparisons only
differ from >= on an exact tie, which is the chapter-divider case the
margin statistic exists for.

The clamp at line 154 was unreachable — at is already bounded by len-1
for any fraction <= 1.0 — so its two mutants were equivalent and the
clamp is gone rather than tested."
```

---

### Task 3: `solve.rs` — the DP table's initialisation (6 survivors)

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (test module)

**Interfaces:**
- Consumes: `h`, `degrade` test helpers; `solve(a, b) -> Vec<Run>`; `PageRange::new`, `Run::paired`, `Run::gap`.
- Produces: nothing later tasks depend on.

**Survivors:**

```
solve.rs:183:42  replace + with - in solve      (the `n + 1` in the `at` closure)
solve.rs:183:42  replace + with * in solve
solve.rs:188:39  replace * with + in solve      (`-(i as f32) * GAP_PENALTY`)
solve.rs:188:39  replace * with / in solve
solve.rs:188:27  delete - in solve              (the unary minus — penalty becomes bonus)
solve.rs:192:27  delete - in solve              (same, for the first row)
```

Lines 183–193 are:

```rust
let at = |i: usize, j: usize| i * (n + 1) + j;

let mut score = vec![0f32; (m + 1) * (n + 1)];
let mut from = vec![Step::Pair; (m + 1) * (n + 1)];
for i in 1..=m {
    score[at(i, 0)] = -(i as f32) * GAP_PENALTY;
    from[at(i, 0)] = Step::SkipA;
}
for j in 1..=n {
    score[at(0, j)] = -(j as f32) * GAP_PENALTY;
    from[at(0, j)] = Step::SkipB;
}
```

The initialisation says: reaching cell `(i, 0)` means skipping `i` pages of A, which costs `i * GAP_PENALTY`. Deleting the minus makes skipping pay. Nothing in the existing suite notices, because every existing `solve` fixture has a good pairing available that wins anyway.

**The shape of the test that kills these:** a fixture where the *gap path and the pairing path compete*, and the right answer depends on the penalty being a cost. Two unrelated editions that nonetheless clear `MIN_SEPARATION` is the wrong instrument — a refusal short-circuits before the table is built. What is needed is a pairing that is accepted overall but where a leading stretch of A has no counterpart.

- [ ] **Step 1: Write the failing test for the leading-gap case**

```rust
/// The first column and first row of the score table say what it costs to
/// reach a cell having skipped that many pages, and the cost is negative
/// because skipping is a penalty. Delete the minus and skipping pays: the
/// solver then prefers to open with a long gap rather than pair from the
/// start. Every other fixture in this file has a pairing available from
/// page zero, so none of them can tell a penalty from a bonus.
#[test]
fn a_leading_run_of_unmatched_pages_stays_a_gap_and_the_rest_still_pairs() {
    // A opens with three pages B does not have, then eight it does.
    let shared: Vec<PageHash> = (10..18).map(h).collect();
    let mut a: Vec<PageHash> = vec![h(1), h(2), h(3)];
    a.extend(shared.iter().copied());
    let b: Vec<PageHash> = shared.iter().map(|p| degrade(*p, 20)).collect();

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![
            Run::gap(PageRange::new(0, 2)),
            Run::paired(PageRange::new(3, 10), PageRange::new(0, 7)),
        ],
        "a leading gap must cost, not pay"
    );
}
```

- [ ] **Step 2: Run it and confirm it passes**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core leading_run_of_unmatched
```

Expected: PASS. **If it fails, read the actual runs before changing the assertion** — the fixture may not clear `MIN_SEPARATION`, in which case `solve` returns `all_unmatched` and the assertion needs a different fixture, not a different expectation.

- [ ] **Step 3: Confirm it kills the two `delete -` mutants**

```bash
sed -i '188s/= -(i as f32)/= (i as f32)/' crates/taiyaku-core/src/solve.rs
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core leading_run_of_unmatched; echo "exit=$?"
git checkout -- crates/taiyaku-core/src/solve.rs

sed -i '192s/= -(j as f32)/= (j as f32)/' crates/taiyaku-core/src/solve.rs
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core leading_run_of_unmatched; echo "exit=$?"
git checkout -- crates/taiyaku-core/src/solve.rs
```

Expected: FAIL, `exit=1`, both times. If line 192's mutant survives, the fixture needs a leading stretch on **B's** side too — add two pages to the front of `b` that A does not have, and extend the expected runs accordingly.

- [ ] **Step 4: Write the failing test for the `at` closure's stride**

`at(i, j) = i * (n + 1) + j` flattens an `(m+1) × (n+1)` table. Changing `n + 1` to `n - 1` or `n * 1` changes the row stride, so cells collide or the index runs past the end. A fixture where `m` and `n` differ is what makes the stride observable; a square table hides `n * 1` entirely when `n == 1`.

```rust
/// The score table is flattened by hand, and `at` is the stride. A wrong
/// stride makes rows overlap, which a square fixture cannot show — with
/// equal sides several wrong strides still land on the right cell often
/// enough to produce the right answer. This one is deliberately oblong:
/// eight pages of A against thirteen of B, with a repagination in the
/// middle so the diagonal bends.
#[test]
fn the_score_table_stride_survives_an_oblong_matrix() {
    let shared: Vec<PageHash> = (20..28).map(h).collect();
    let a: Vec<PageHash> = shared.clone();
    // B carries the same eight pages with five extras interleaved after
    // the third, so the alignment is two runs at different offsets.
    let mut b: Vec<PageHash> = shared[..3].iter().map(|p| degrade(*p, 20)).collect();
    b.extend((90..95).map(h));
    b.extend(shared[3..].iter().map(|p| degrade(*p, 20)));

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![
            Run::paired(PageRange::new(0, 2), PageRange::new(0, 2)),
            Run::paired(PageRange::new(3, 7), PageRange::new(8, 12)),
        ],
        "an oblong table must index the same as a square one"
    );
}
```

- [ ] **Step 5: Run it and confirm it passes**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core oblong_matrix
```

Expected: PASS. If the runs come back differently, print them and adjust the *assertion* to the true answer — then re-check that a mutated stride still breaks it (Step 6). Do not weaken the assertion to `runs.len() == 2`.

- [ ] **Step 6: Confirm it kills the `183` and `188:39` mutants**

```bash
for M in '183s/(n + 1)/(n - 1)/' '183s/(n + 1)/(n * 1)/' \
         '188s/\* GAP_PENALTY/+ GAP_PENALTY/' '188s/\* GAP_PENALTY/\/ GAP_PENALTY/'; do
  sed -i "$M" crates/taiyaku-core/src/solve.rs
  env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core -E 'test(oblong_matrix) + test(leading_run_of_unmatched)' >/dev/null 2>&1
  echo "$M -> exit=$?"
  git checkout -- crates/taiyaku-core/src/solve.rs
done
```

Expected: `exit=1` on all four (an index-out-of-bounds panic also counts as a kill — the test fails either way).

- [ ] **Step 7: Verify against the tool**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/solve.rs --output /tmp/mut-solve
grep -cE "solve\.rs:(183|188|192):" /tmp/mut-solve/mutants.out/missed.txt
```

Expected: `0`.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-core/src/solve.rs
git commit -m "Pin the DP table's initialisation: a gap costs, it does not pay

Six survivors, and the one worth naming: deleting the unary minus at
solve.rs:188 turns the gap penalty into a gap bonus and all 526 tests
still pass. Every existing solve fixture has a pairing available from
page zero, so none of them can tell a penalty from a bonus.

The stride test is oblong on purpose — with equal sides several wrong
strides still land on the right cell often enough to give the right
answer."
```

---

### Task 4: `solve.rs` — the recurrence (9 survivors)

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (test module)

**Interfaces:**
- Consumes: `h`, `degrade`, `degrade_at`; `solve`; `calibrate`.
- Produces: nothing later tasks depend on.

**Survivors:**

```
solve.rs:198:81  replace - with + in solve      (`- floor`)
solve.rs:198:81  replace - with / in solve
solve.rs:198:35  replace - with / in solve      (`i - 1` inside at(...))
solve.rs:198:42  replace - with / in solve      (`j - 1` inside at(...))
solve.rs:199:46  replace - with + in solve      (`- GAP_PENALTY` for skip_a)
solve.rs:199:37  replace - with / in solve      (`i - 1`)
solve.rs:200:46  replace - with + in solve      (`- GAP_PENALTY` for skip_b)
solve.rs:200:40  replace - with / in solve      (`j - 1`)
solve.rs:204:50  replace && with || in solve    (the tie rule)
```

Lines 198–204:

```rust
let pair = score[at(i - 1, j - 1)] + similarity(a[i - 1], b[j - 1]) - floor;
let skip_a = score[at(i - 1, j)] - GAP_PENALTY;
let skip_b = score[at(i, j - 1)] - GAP_PENALTY;

// Ties favour pairing, which keeps runs contiguous rather than
// splitting them around an equally-scored detour.
let (best, step) = if pair >= skip_a && pair >= skip_b {
```

Three distinct claims need pinning, and Task 3's fixtures already cover part of the arithmetic — run them first and re-measure before writing more.

- [ ] **Step 1: Re-measure after Task 3**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/solve.rs --output /tmp/mut-solve
grep -E "solve\.rs:(198|199|200|204):" /tmp/mut-solve/mutants.out/missed.txt
```

Task 3's oblong and leading-gap fixtures plausibly kill several of these already. **Write tests only for what is still listed.** Record the before-and-after counts in the commit message.

- [ ] **Step 2: Write the failing test for the floor's subtraction**

`- floor` is what makes a below-floor similarity score *worse* than taking a gap. Flip it to `+ floor` and every weak match becomes attractive, so pages that should fall in a gap get paired.

```rust
/// `floor` is subtracted from every pairing score, which is what makes a
/// below-floor match lose to a gap. Added instead, weak matches become
/// attractive and pages that correspond to nothing get paired to whatever
/// scored highest. The fixture is a volume with a genuine middle section
/// and two pages at the end that exist only in A.
#[test]
fn a_page_below_the_floor_takes_a_gap_rather_than_a_weak_pairing() {
    let shared: Vec<PageHash> = (30..40).map(h).collect();
    let mut a: Vec<PageHash> = shared.clone();
    // Two trailing pages unrelated to anything in B.
    a.push(h(500));
    a.push(h(501));
    let b: Vec<PageHash> = shared.iter().map(|p| degrade(*p, 20)).collect();

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![
            Run::paired(PageRange::new(0, 9), PageRange::new(0, 9)),
            Run::gap(PageRange::new(10, 11)),
        ],
        "pages below the floor must gap, not pair"
    );
}
```

- [ ] **Step 3: Write the failing test for the tie rule**

Line 204's `&&` encodes "pairing wins only if it beats *both* alternatives." With `||` it wins if it beats *either*, which splits contiguous runs around equally-scored detours — exactly what the comment above the line says the rule exists to prevent.

```rust
/// Ties favour pairing, and the rule is `pair >= skip_a && pair >= skip_b`
/// — pairing must beat both alternatives, not either. With `||` a pairing
/// that ties one alternative and loses to the other still wins, which
/// splits a clean diagonal around equally-scored detours. Repeated pages
/// are what produce those ties: a volume with a run of identical blank
/// pages scores the same in several directions at once.
#[test]
fn a_stretch_of_identical_pages_stays_one_run_rather_than_splitting() {
    let blank = h(7);
    let mut a: Vec<PageHash> = (40..44).map(h).collect();
    // Six identical blanks in the middle: every cell in that block ties.
    a.extend(std::iter::repeat_n(blank, 6));
    a.extend((44..48).map(h));
    let b: Vec<PageHash> = a.iter().map(|p| degrade(*p, 20)).collect();

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 13), PageRange::new(0, 13))],
        "an identical stretch must not fragment the diagonal"
    );
}
```

- [ ] **Step 4: Run both**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core \
  -E 'test(below_the_floor) + test(identical_pages)'
```

Expected: 2 passed. If the identical-pages fixture returns more than one run against unmutated code, the tie rule does not behave as its comment claims — **that is a finding, not a broken test.** Record the actual runs, stop, and report it before adjusting anything.

- [ ] **Step 5: Confirm the kills by hand**

```bash
for M in '198s/- floor/+ floor/' '204s/&& pair >= skip_b/|| pair >= skip_b/' \
         '199s/- GAP_PENALTY/+ GAP_PENALTY/' '200s/- GAP_PENALTY/+ GAP_PENALTY/'; do
  sed -i "$M" crates/taiyaku-core/src/solve.rs
  env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core -E 'package(taiyaku-core)' >/dev/null 2>&1
  echo "$M -> exit=$?"
  git checkout -- crates/taiyaku-core/src/solve.rs
done
```

Expected: `exit=1` on all four.

- [ ] **Step 6: Verify against the tool and commit**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/solve.rs --output /tmp/mut-solve
grep -cE "solve\.rs:(198|199|200|204):" /tmp/mut-solve/mutants.out/missed.txt
```

Expected: `0`.

```bash
git add crates/taiyaku-core/src/solve.rs
git commit -m "Pin the recurrence: the floor subtracts, and ties favour pairing

Nine survivors in three claims. Subtracting the floor is what makes a
below-floor match lose to a gap; added instead, every weak match becomes
attractive. The && at line 204 is what the comment above it says it is —
pairing must beat both alternatives — and with || a clean diagonal
fragments around equally-scored detours."
```

---

### Task 5: `solve.rs` — the traceback (3 survivors + 1 timeout)

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (test module)

**Survivors:**

```
solve.rs:223:19  replace -= with /= in solve    (i -= 1 in the Pair arm)
solve.rs:229:30  replace -= with += in solve    (j -= 1 in the SkipB arm)
solve.rs:229:30  replace -= with /= in solve
```

**And one timeout, tracked separately:** `solve.rs:227:19` (`i -= 1` in the `SkipA` arm → `/=`). A traceback that stops decrementing never terminates, so `cargo-mutants` reports `TIMEOUT` rather than `MISSED`. Decide in Task 10 whether that counts as caught.

- [ ] **Step 1: Re-measure after Tasks 3 and 4**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/solve.rs --output /tmp/mut-solve
grep -E "solve\.rs:(223|227|229):" /tmp/mut-solve/mutants.out/missed.txt
```

The traceback runs on every `solve` call, so Tasks 3 and 4's fixtures may already have killed these. Write tests only for what survives.

- [ ] **Step 2: If any survive, write the discriminating test**

The traceback walks back from `(m, n)` emitting pairs. `i /= 1` is a no-op that leaves `i` unchanged in the `Pair` arm, so the loop spins — unless the fixture reaches `i == 0` by another arm first. `j += 1` in the `SkipB` arm walks `j` *forward*, which indexes past the table.

What discriminates is a fixture that **ends** on the arm being mutated: a trailing gap on B's side forces the traceback to open in `SkipB`.

```rust
/// The traceback walks back from the bottom-right of the table, and each
/// arm must step the index it claims to. A fixture that ends on a paired
/// cell never enters the SkipB arm at all, so the whole B-side traceback
/// can be wrong and the answer still right. This one ends with three
/// pages of B that A does not have, which is what SkipB is for.
#[test]
fn a_trailing_run_of_extra_b_pages_walks_back_through_skip_b() {
    let shared: Vec<PageHash> = (50..58).map(h).collect();
    let a: Vec<PageHash> = shared.clone();
    let mut b: Vec<PageHash> = shared.iter().map(|p| degrade(*p, 20)).collect();
    // Three trailing pages in B with no counterpart in A.
    b.extend((600..603).map(h));

    let runs = solve(&a, &b);

    assert_eq!(
        runs,
        vec![Run::paired(PageRange::new(0, 7), PageRange::new(0, 7))],
        "B's trailing extras are not A's pages and must not appear in A's runs"
    );
}
```

- [ ] **Step 3: Run it**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core trailing_run_of_extra_b
```

Expected: PASS.

- [ ] **Step 4: Confirm the kill, with a timeout guard**

A mutated traceback can hang. Bound it:

```bash
sed -i '229s/j -= 1/j += 1/' crates/taiyaku-core/src/solve.rs
timeout 60 env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core trailing_run_of_extra_b; echo "exit=$?"
git checkout -- crates/taiyaku-core/src/solve.rs
```

Expected: `exit=1` (test failure or panic) or `exit=124` (timeout kill). Both are kills. `exit=0` is not.

- [ ] **Step 5: Verify against the tool and commit**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/solve.rs --output /tmp/mut-solve
wc -l < /tmp/mut-solve/mutants.out/missed.txt
```

Expected: `1` or fewer — only `compress`/`confidence` (Task 6) should remain.

```bash
git add crates/taiyaku-core/src/solve.rs
git commit -m "Pin the traceback's SkipB arm

A fixture that ends on a paired cell never enters SkipB, so the whole
B-side traceback could be wrong and the answer still right. Three
trailing pages of B with no counterpart in A is what opens that arm."
```

---

### Task 6: `solve.rs` — `compress` and `confidence` (2 survivors)

**Files:**
- Modify: `crates/taiyaku-core/src/solve.rs` (test module)

**Survivors:**

```
solve.rs:256:33  replace match guard `last.a.end + 1 == ai` with true in compress
solve.rs:445:50  replace != with == in confidence
```

Line 256 is the **gap** arm of `compress`'s run extension:

```rust
(None, None) if last.a.end + 1 == ai => {
```

Forcing the guard to `true` merges non-adjacent gap runs into one. Line 445 is `confidence`'s rival filter — `filter(|(j, _)| Some(*j as u32) != subject)` — which excludes the chosen counterpart from the rivals. Inverted, the "rival" becomes the subject itself, so every margin collapses to zero.

- [ ] **Step 1: Write the failing test for `compress`'s gap arm**

```rust
/// Two gap runs separated by a paired stretch are two runs. `compress`
/// extends a gap only when it is the immediate successor of the previous
/// one; without that guard a volume with a colour insert at the front and
/// a bonus chapter at the back reports one gap spanning the whole book,
/// including everything paired in between.
#[test]
fn two_separated_gaps_do_not_merge_into_one() {
    let pairs: Vec<(u32, Option<u32>)> = vec![
        (0, None),
        (1, None),
        (2, Some(0)),
        (3, Some(1)),
        (4, None),
        (5, None),
    ];

    let runs = compress(&pairs);

    assert_eq!(
        runs,
        vec![
            Run::gap(PageRange::new(0, 1)),
            Run::paired(PageRange::new(2, 3), PageRange::new(0, 1)),
            Run::gap(PageRange::new(4, 5)),
        ]
    );
}
```

- [ ] **Step 2: Write the failing test for `confidence`'s rival filter**

```rust
/// The margin is the distance from the chosen counterpart to the best
/// *other* candidate, so the counterpart must be excluded from the rival
/// search. Include it and the rival is the subject itself, which makes
/// every margin exactly zero — and zero margin is the report's danger
/// signal, so the whole verification contact sheet would light up red.
#[test]
fn the_chosen_counterpart_is_not_its_own_rival() {
    let page = h(1);
    // One page of A, two of B: an exact match and something unrelated.
    let a = vec![page];
    let b = vec![page, h(99)];
    let runs = vec![Run::paired(PageRange::new(0, 0), PageRange::new(0, 0))];

    let scored = confidence(&a, &b, &runs);

    assert_eq!(scored[0].counterpart, Some(0));
    assert_eq!(scored[0].similarity, 1.0, "{:?}", scored[0]);
    assert!(
        scored[0].margin > 0.3,
        "margin must measure distance to the *other* candidate: {:?}",
        scored[0]
    );
}
```

- [ ] **Step 3: Run both**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core \
  -E 'test(separated_gaps) + test(own_rival)'
```

Expected: 2 passed.

- [ ] **Step 4: Confirm the kills**

```bash
sed -i '445s/!= subject/== subject/' crates/taiyaku-core/src/solve.rs
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core own_rival; echo "exit=$?"
git checkout -- crates/taiyaku-core/src/solve.rs
```

Expected: FAIL, `exit=1`. For line 256, edit the guard to `if true` by hand, run `separated_gaps`, expect failure, then revert.

- [ ] **Step 5: Verify `solve.rs` is clear and commit**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/solve.rs --output /tmp/mut-solve
cat /tmp/mut-solve/mutants.out/missed.txt
```

Expected: empty. **If anything remains, list it in the commit message rather than deleting the line from this plan.**

```bash
git add crates/taiyaku-core/src/solve.rs
git commit -m "Pin compress's gap adjacency and confidence's rival filter

Two survivors. Merging non-adjacent gaps turns a volume with a colour
insert and a bonus chapter into one gap spanning the whole book. And a
counterpart that is its own rival makes every margin zero, which is the
verification report's danger signal — the contact sheet would go
entirely red and nothing would fail."
```

---

### Task 7: `slot.rs`, `lib.rs` and `credits.rs` (10 survivors)

**Files:**
- Modify: `crates/taiyaku-core/src/slot.rs` (test module at line 147)
- Modify: `crates/taiyaku-core/src/lib.rs` (test module at line 164)
- Modify: `crates/taiyaku-core/src/credits.rs` (test module at line 144)

**Survivors:**

```
slot.rs:100:36    replace / with % in spread_flags   (all[all.len() / 2] — the median index)
slot.rs:103:33    replace / with % in spread_flags   (portrait[portrait.len() / 2])
lib.rs:55:9       replace PageRange::is_empty -> bool with true
credits.rs:65:23  replace match guard `f == edition && t == against` with false
credits.rs:65:41  replace == with != in unpaired_runs
credits.rs:121:49 replace && with || in unpaired_runs
credits.rs:122:39 replace + with - in unpaired_runs
credits.rs:122:39 replace + with * in unpaired_runs
credits.rs:124:51 replace + with - in unpaired_runs
credits.rs:124:51 replace + with * in unpaired_runs
```

**Plus four timeouts in `credits.rs`** — `93:18` ×3 and `94:16`, all in the `while at < slots.len() && slots[at].page == page { at += 1 }` page-grouping loop. A mutated increment never terminates. Task 10 decides their disposition.

- [ ] **Step 1: `lib.rs` — assert the inclusive range is never empty**

`is_empty` returns a literal `false` and exists so clippy's `len_without_is_empty` is satisfied. Nothing calls it, so only a direct assertion can kill the mutant.

```rust
/// `PageRange` is inclusive, so `new(4, 4)` is one page, not zero. The
/// method exists because `len` does and clippy pairs them; it is a
/// constant, and a constant with no test is a constant that can be
/// flipped without anything noticing.
#[test]
fn an_inclusive_page_range_is_never_empty() {
    assert!(!PageRange::new(4, 4).is_empty());
    assert_eq!(PageRange::new(4, 4).len(), 1);
    assert!(!PageRange::new(0, 209).is_empty());
    assert_eq!(PageRange::new(0, 209).len(), 210);
}
```

- [ ] **Step 2: `slot.rs` — assert the median index on an even-length input**

`all.len() / 2` and `portrait.len() / 2` pick the median. `%` gives `len % 2`, which is `0` for an even count and `1` for an odd one — so on an **odd**-length sorted list of 3 the mutant picks index 1 and `/` also picks index 1. Only an even count, or a length above 3, discriminates.

```rust
/// The baseline is the median aspect ratio, and the median index is
/// `len / 2`. Replacing it with `len % 2` picks index 0 or 1 — the two
/// narrowest pages — which on a real edition means the baseline is set by
/// whichever page was cropped tightest. The fixture needs six portrait
/// pages, because with three `/` and `%` agree.
#[test]
fn the_ordinary_page_is_the_median_and_not_the_narrowest() {
    // Six portrait pages, one much narrower than the rest, plus one true
    // spread at roughly twice the ordinary aspect.
    let mut dims: Vec<(u32, u32)> = vec![
        (700, 1400), // markedly narrow: 0.50
        (1000, 1400),
        (1010, 1400),
        (1020, 1400),
        (1030, 1400),
        (1040, 1400),
    ];
    dims.push((2040, 1400)); // a joined spread of the ordinary page

    let flags = spread_flags(&dims);

    assert_eq!(
        flags,
        vec![false, false, false, false, false, false, true],
        "the narrow page is not a spread and must not set the baseline"
    );
}

/// The other branch: an edition published entirely as joined spreads has
/// no portrait page at all, so the baseline comes from halving the median
/// of every page. Same median-index hazard, different array.
#[test]
fn an_all_landscape_edition_takes_its_baseline_from_the_halved_median() {
    // Six landscape pages of similar aspect: every one is a spread.
    let dims: Vec<(u32, u32)> = vec![
        (1900, 1400),
        (2000, 1400),
        (2010, 1400),
        (2020, 1400),
        (2030, 1400),
        (2100, 1400),
    ];

    let flags = spread_flags(&dims);

    assert_eq!(flags, vec![true; 6]);
}
```

- [ ] **Step 3: Run the `slot.rs` and `lib.rs` tests**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core \
  -E 'test(never_empty) + test(median_and_not_the_narrowest) + test(halved_median)'
```

Expected: 3 passed. **If `the_ordinary_page_is_the_median` fails, print `flags` and check the aspect arithmetic against `is_spread`'s `(1.7..=2.8)` band before touching the assertion** — the spread page may not clear it.

- [ ] **Step 4: `credits.rs` — assert the alignment orientation is picked correctly**

Line 65's guard is the one that decides which side of a stored, directed edge addresses the edition being examined. Taking the wrong side silently reports another edition's gaps — the comment above it says exactly that, and nothing tested it.

Build the fixture from whatever `Sidecar` constructor `credits.rs`'s existing tests already use; read `mod tests` at `credits.rs:144` first and follow it rather than inventing a second fixture style.

```rust
/// The stored alignment is directed, and only one side's spans address
/// the edition being examined. Picking the wrong side reports another
/// edition's gaps under this edition's page numbers — a wrong answer that
/// looks entirely well-formed. The fixture asks the same question from
/// both ends of one edge and requires the two answers to differ.
#[test]
fn each_end_of_a_directed_edge_reports_its_own_gaps() {
    let sidecar = /* the fixture style already used in this module */;

    let forward = unpaired_runs(&sidecar, "raw", "en").expect("edge exists");
    let backward = unpaired_runs(&sidecar, "en", "raw").expect("same edge, other end");

    assert_ne!(
        forward, backward,
        "the two ends of a directed edge do not have the same gaps"
    );
    // And each must be expressed in its own edition's page numbers.
    assert!(forward.iter().all(|r| r.pages.end <= 6));
    assert!(backward.iter().all(|r| r.pages.end <= 8));
}
```

- [ ] **Step 5: `credits.rs` — assert run extension needs the same member *and* adjacency**

Lines 121–124 extend the previous unpaired run only when this page is its immediate successor **within the same member**. `&&` → `||` merges across a member boundary; the `+ 1`s at 122 and 124 are the run's exclusive end.

```rust
/// A run of unpaired pages stops at a member boundary, and the member
/// test is the whole reason this function exists rather than a complement
/// over the slot set. With `||` a credits page at the end of one chapter
/// file and another at the start of the next merge into a single run that
/// spans a boundary no reader can see.
#[test]
fn an_unpaired_run_stops_at_a_member_boundary() {
    let sidecar = /* two members, with the last page of the first and the
                     first page of the second both unpaired */;

    let runs = unpaired_runs(&sidecar, "en", "raw").expect("edge exists");

    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_ne!(runs[0].member, runs[1].member);
    // The exclusive ends: a one-page run ends one past its start.
    assert_eq!(runs[0].pages.end, runs[0].pages.start + 1);
    assert_eq!(runs[0].local.end, runs[0].local.start + 1);
    assert!(runs[0].at_member_end, "{runs:?}");
}
```

- [ ] **Step 6: Run the `credits.rs` tests**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core -E 'package(taiyaku-core) and test(/credits|member_boundary|directed_edge/)'
```

Expected: all pass.

- [ ] **Step 7: Verify all three files against the tool**

```bash
for F in slot lib credits; do
  env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
    --file "crates/taiyaku-core/src/$F.rs" --output "/tmp/mut-$F"
  echo "$F: $(wc -l < /tmp/mut-$F/mutants.out/missed.txt) survivors"
done
```

Expected: `slot: 0`, `lib: 0`, `credits: 0`.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-core/src/slot.rs crates/taiyaku-core/src/lib.rs crates/taiyaku-core/src/credits.rs
git commit -m "Pin the median index, the inclusive range, and the directed edge

Ten survivors. spread_flags picks a baseline at len/2; with len%2 it
picks the narrowest page instead, and a three-element fixture cannot
tell the two apart. unpaired_runs chooses which side of a directed edge
addresses this edition — take the wrong one and it reports another
edition's gaps under this edition's page numbers, well-formed and
wrong."
```

---

### Task 8: `sidecar.rs` — the equality and boolean guards (7 survivors)

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs` (test module at line 1441)

**Survivors:**

```
sidecar.rs:47:26    replace || with && in parse_slot
sidecar.rs:518:46   replace == with != in Sidecar::merge_spoke
sidecar.rs:618:88   replace == with != in Sidecar::merge_spoke
sidecar.rs:625:20   replace > with >= in Sidecar::merge_spoke
sidecar.rs:720:65   replace == with != in Sidecar::correct
sidecar.rs:947:28   replace == with != in Sidecar::runs_of
sidecar.rs:1052:46  replace || with && in Sidecar::trust
```

Each is a one-line claim. Read the surrounding function before writing each test — `sidecar.rs` is the largest file in the crate and its fixtures are correspondingly involved.

- [ ] **Step 1: `parse_slot` — reject both halves of the malformed case**

Line 47 is `if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit())`. With `&&`, a slot reference that is empty *or* non-numeric — but not both — parses.

```rust
/// A slot reference is digits, optionally suffixed L or R. The guard
/// rejects two different malformations, and `&&` in place of `||` lets
/// each through on its own: "" has no digits but vacuously satisfies
/// all-digits, and "12a" is non-empty but not numeric.
#[test]
fn a_slot_reference_that_is_empty_or_not_numeric_is_refused() {
    assert!(parse_slot("").is_err());
    assert!(parse_slot("L").is_err());
    assert!(parse_slot("12a").is_err());
    assert!(parse_slot("a12").is_err());
    // And the forms that must still parse.
    assert!(parse_slot("12").is_ok());
    assert!(parse_slot("12L").is_ok());
    assert!(parse_slot("0R").is_ok());
}
```

- [ ] **Step 2: `trust` — both editions must be known**

Line 1052 is `if !self.editions.contains_key(from) || !self.editions.contains_key(to)`. With `&&`, an edge naming one known and one unknown edition returns a verdict instead of `None`.

```rust
/// `trust` returns None when the document does not say, and a pair
/// naming an edition this sidecar has never heard of is exactly that.
/// With `&&` only a pair where *both* are unknown returns None, so a
/// typo in one name reports the other edition's verdict.
#[test]
fn trust_refuses_a_pair_naming_an_edition_this_work_does_not_have() {
    let sidecar = /* the fixture style already used in this module */;

    assert_eq!(sidecar.trust("raw", "nonexistent"), None);
    assert_eq!(sidecar.trust("nonexistent", "raw"), None);
    assert_eq!(sidecar.trust("nonexistent", "also-not-here"), None);
    // The control: a pair it does have answers.
    assert!(sidecar.trust("raw", "en").is_some());
}
```

- [ ] **Step 3: Write the remaining five tests**

For each of `518`, `618`, `625`, `720`, `947`: read the function, state in a doc comment what the operator claims, and write a fixture where the mutated operator gives a different, observable answer.

`625` is `if carried > 0` — the count of hand runs carried onto a freshly solved edge. `>=` makes the carrying path run with zero hand runs, which is wasted work rather than a wrong answer, so **it may be an equivalent mutant.** If it is, do not force a test: record the argument and hand it to Task 10.

- [ ] **Step 4: Run the file's tests**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core -E 'package(taiyaku-core)'
```

Expected: all pass, and the total test count has risen.

- [ ] **Step 5: Verify and commit**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/sidecar.rs --output /tmp/mut-sidecar
cat /tmp/mut-sidecar/mutants.out/missed.txt
```

Expected: at most the four Task 9 addresses, plus any recorded as equivalent.

```bash
git add crates/taiyaku-core/src/sidecar.rs
git commit -m "Pin sidecar's equality and boolean guards

Seven survivors, each a one-line claim nothing asserted: parse_slot's
two malformations, trust's unknown-edition refusal, and the equality
tests inside merge_spoke, correct and runs_of."
```

---

### Task 9: `sidecar.rs` — the run-splicing guards (4 survivors)

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs` (test module)

**Survivors:**

```
sidecar.rs:356:9    replace <impl Display for NotCorrected>::fmt with Ok(Default::default())
sidecar.rs:754:39   replace && with || in Sidecar::correct
sidecar.rs:856:42   replace && with || in Sidecar::rewrite
sidecar.rs:1280:39  replace && with || in carry_hand_runs
```

- [ ] **Step 1: `NotCorrected`'s `Display` — assert the message reaches the reader**

The mutant makes `fmt` write nothing and return `Ok`. Every correction failure would then surface as an empty string.

```rust
/// A refused correction has to say why: the reader is pointing at a
/// filmstrip and the message is the whole feedback. A Display that
/// writes nothing and returns Ok looks like success at every call site
/// that formats it.
#[test]
fn every_refusal_variant_says_something() {
    let variants = [
        NotCorrected::NoSuchEdition("raw".into()),
        NotCorrected::NoSuchSlot("12L".into()),
        NotCorrected::SameEdition("raw".into()),
    ];
    for v in &variants {
        let shown = v.to_string();
        assert!(!shown.is_empty(), "{v:?} formatted to nothing");
        assert!(shown.len() > 4, "{v:?} formatted to {shown:?}");
    }
}
```

Adjust the variant list to whatever `NotCorrected` actually declares — read the enum at `sidecar.rs:340`-ish first. Include **every** variant; a loop over three of five leaves two mutable.

- [ ] **Step 2: `1280` — the overlap test in `carry_hand_runs`**

Line 1280 is `(ce >= a_start as i64 && cs <= a_end as i64)`, the test for whether a hand run's target overlaps this piece's span. With `||` non-overlapping spans are treated as overlapping, so pieces get evicted that should have survived.

```rust
/// A solved piece gives up only the stretch a hand run actually claims.
/// The overlap test is two-sided — the hand span must both end at or
/// after this piece starts and begin at or before it ends — and with `||`
/// a hand correction anywhere in the volume evicts every piece in it.
#[test]
fn a_hand_run_evicts_only_the_pieces_it_overlaps() {
    let sidecar = /* a sidecar with one long solved run and a hand
                     correction touching only its middle */;

    let after = /* apply the correction */;

    // The stretches on either side of the correction keep their pairing.
    assert!(/* the piece before the correction is still paired */);
    assert!(/* the piece after the correction is still paired */);
}
```

Read `carry_hand_runs` (from `sidecar.rs:1190`) and the existing correction tests before filling these in. **If the existing test module already builds a hand-corrected sidecar, extend that fixture rather than writing a second one.**

- [ ] **Step 3: `754` and `856` — the remaining two `&&`s**

Line 754 sits in `Sidecar::correct`'s already-true check; line 856 in `Sidecar::rewrite`'s margin carry-over. For each: read the function, state what the conjunction claims, and build the fixture where `||` differs.

- [ ] **Step 4: Run the whole crate**

```bash
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core
```

Expected: all pass.

- [ ] **Step 5: Verify and commit**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --file crates/taiyaku-core/src/sidecar.rs --output /tmp/mut-sidecar
cat /tmp/mut-sidecar/mutants.out/missed.txt
```

Expected: empty, or only entries recorded as equivalent in Task 8.

```bash
git add crates/taiyaku-core/src/sidecar.rs
git commit -m "Pin the run-splicing guards and NotCorrected's Display

Four survivors. The overlap test in carry_hand_runs is two-sided, and
with || one hand correction evicts every piece in the volume. And a
Display that writes nothing returns Ok, so a refused correction reaches
the reader as an empty string."
```

---

### Task 10: Close out — re-measure, triage, document

**Files:**
- Modify: `docs/notes.md`
- Modify: `docs/status.md`
- Modify: `docs/ci.md`

**Interfaces:**
- Consumes: every earlier task's tests.
- Produces: the branch's closing record.

- [ ] **Step 1: Re-run the whole crate's mutants**

```bash
env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest \
  --package taiyaku-core --output /tmp/mut-core-final
tail -1 /tmp/mut-core-final/mutants.out/*.txt 2>/dev/null
cat /tmp/mut-core-final/mutants.out/missed.txt
cat /tmp/mut-core-final/mutants.out/timeout.txt
```

Record the caught/missed/timeout counts against the baseline's 460 / 370 / 47 / 5.

- [ ] **Step 2: Triage what remains, in writing**

For every survivor still listed, write one of two things into `docs/notes.md`:

- **The argument that it is equivalent** — a semantically identical change that cannot be observed. Name the reason. `percentile`'s unreachable clamp and `merge_spoke:625`'s `carried > 0` are the two candidates this plan already identified.
- **The reason it is not worth a test** — with what would be needed to kill it, so a later reader can disagree with the judgement rather than rediscover the mutant.

**A survivor with neither is not triaged.** Do not delete it from the record.

- [ ] **Step 3: Decide the five timeouts**

`solve.rs:227` and `credits.rs:93` ×3, `credits.rs:94`. Each is a loop counter whose mutation stops the loop terminating. State the disposition: a hang is arguably caught (the suite would never go green), but it is not *caught by an assertion*, and `--test-tool=nextest`'s own timeout is what ends it. Record which reading this project takes, because a future `--in-diff` gate has to count them one way or the other.

- [ ] **Step 4: Confirm the suite is still fast**

```bash
time (env -u TAIYAKU_CORPUS cargo nextest run --workspace --all-features 2>&1 | tail -2)
```

Expected: still around 2 s, and the test count above 526. **If it has grown past 5 s, say so and name the slowest new test** — the coverage job runs this.

- [ ] **Step 5: Run the full gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Expected: `all green`, with the standing corpus checks named as not measured. This also runs `cargo fmt --all -- --check` and `cargo clippy ... -D warnings`, both of which the new tests must satisfy.

- [ ] **Step 6: Run the corpus checks too**

```bash
TAIYAKU_CORPUS="$PWD/corpus" cargo test --release --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

Expected: 12 passed. The branch adds no corpus dependency, so this must be unchanged — it is the control proving the new tests did not alter behaviour.

- [ ] **Step 7: Write the closing record**

Extend `docs/notes.md`'s section from Task 1 with: the final counts, the triage, the timeout decision, and **the sentence that states what changed** — that the gap penalty at `solve.rs:188` can no longer be turned into a bonus without a CI-visible test failing, where before only a corpus check on a NAS could see it.

Update `docs/status.md`'s open item: `taiyaku-core` closed, the other four crates' counts still open.

- [ ] **Step 8: Commit**

```bash
git add docs/notes.md docs/status.md docs/ci.md
git commit -m "taiyaku-core's mutation survivors: 47 to <N>

The aligner's arithmetic is now pinned by tests the forge can run. Before
this branch, deleting the minus at solve.rs:188 — turning the gap penalty
into a gap bonus — passed all 526 CI-visible tests and failed exactly one
corpus check, on material CI cannot reach.

<N> survivors remain, each recorded in notes.md with either the argument
that it is equivalent or what would be needed to kill it. The five
timeouts are recorded separately with the reading this project takes."
```

---

## Self-review

**Spec coverage.** Spec §4's five deliverables map to tasks: step 1 (baseline) → Task 1 and Task 10 step 1; step 2 (triage) → Task 10 step 2; step 3 (kill the survivors) → Tasks 2–9; step 4 (record the rest) → Task 1 step 5 and Task 10 step 7; step 5 (document the invocation) → Task 1 step 3. Spec §5's exclusions — no `--in-diff` gate, no threshold, no StrykerJS — appear in no task, which is correct. Spec §6's caveats drive Task 10 step 2's requirement that equivalence be argued rather than assumed. Part 2 of the spec is out of scope here by design and gets its own plan.

**Placeholders.** Three tests carry `/* … */` fixture comments — `credits.rs`'s two in Task 7, `sidecar.rs`'s in Tasks 8 and 9. These are not placeholders for *behaviour*: the assertions, the doc comments and the reason each test exists are all written. They are deliberate deferrals of fixture *construction*, because `Sidecar` fixtures in those modules are large and the plan instructs the implementer to extend the existing fixture rather than invent a second style. Every such site says which existing test module to read first.

**Type consistency.** `PageRange::new(start, end)`, `Run::paired(a, b)`, `Run::gap(a)`, `PageHash`, `Slot { page, half }`, `UnpairedRun { slots, pages, member, local, at_member_end }`, `Calibration { noise, signal, margin, floor }`, `PageConfidence { page, counterpart, similarity, margin }` — all match the definitions read from the source. Test helpers `h`, `degrade`, `degrade_at` are the existing private ones in `solve.rs`'s test module and are not available in `slot.rs`, `lib.rs`, `credits.rs` or `sidecar.rs`; no task uses them outside `solve.rs`.

**One risk worth naming.** Tasks 3, 4 and 5 assert exact `Vec<Run>` equality against fixtures whose solved output has not been run. If a fixture does not clear `MIN_SEPARATION` the solver returns `all_unmatched` and the assertion fails for a reason unrelated to the mutant. Each of those tasks therefore says to print the actual runs and correct the *assertion* — never to weaken it — and then re-confirm the mutant still dies.
