# Accepted mutation-test survivors

Every mutant listed here is a change to the source that **the whole test suite
still passes against**, and that somebody has looked at and accepted anyway.
`scripts/mutants.sh` diffs a run against this file; anything not listed is the
output worth reading.

The point of writing them down is that a survivor list is only actionable when a
new one is distinguishable from an old one. 45 mutants in `detect.rs` and
`regions.rs` survive today and are deferred until L2 redesigns the detector — a
run that reports them every time trains the reader to skim it, which is the one
failure mode a weekly job that gates nothing is most prone to.

It is also the only memory this project has of what has already been checked.
Four whole-branch reviews of spec A2 returned fifteen findings each and the rate
never fell, because every reviewer re-sampled the whole branch from scratch.
Coverage says which lines *ran*. This says which lines have an assertion behind
them, and which are known not to.

## What belongs here, and what does not

**An accepted survivor needs a reason, and the reason has to be one of these:**

- **An equivalent mutant.** The mutation produces a program that behaves
  identically, so no test could distinguish it. `percentile`'s order-independent
  comparison is the recurring example. These are permanent.
- **Deferred behind named work.** `detect.rs` and `regions.rs` are waiting on
  L2's detector redesign, and writing fixtures against a detector about to be
  replaced is work thrown away. These carry the item they are waiting on.
- **A decision inside output nobody asserts on.** A branch that only changes what
  a command prints for a person to read. Usually the right fix is to extract the
  decision into a function that returns a value — which is what killed 11 mutants
  in `inventory` and 24 in `topology` — so this reason should be rare and should
  say why extraction was not done.

**"No test reaches it" is not a reason.** That is the finding, not an
explanation of it. Neither is "it is only reachable with a real archive": the
sweep that closed five crates found that a *decision inside a print statement* is
the most common shape of untested logic in this codebase, and extracting it is
the fix.

## What this file cannot tell you

**cargo-mutants does not mutate every expression.** It generated no mutant at all
for `run.b.map(|_| run.a)` in a `match` arm, so O38's defect was invisible to it
both before and after the fix. A file with no entries here is not a verified
file; it is a file with no *known* survivors.

**An entry is keyed on `file:line:col: description`, so moving a function
invalidates its entries** and its survivors reappear as new. That is deliberate.
Matching on the description alone would carry an accepted survivor across a
rewrite that turned it into a real defect, with the entry still explaining why
the old code was fine.

## Regenerating

`scripts/mutants.sh --baseline` (with whatever scope) rewrites the entries below
for the files that run examined, carrying existing reasons across and stamping
new entries with a placeholder. The placeholders are yours to replace — the
script exits non-zero while any remain, because an entry with no reason is
indistinguishable from a defect somebody stopped thinking about.

<!-- entries -->

### `crates/taiyaku-cli/src/detect.rs`

- `crates/taiyaku-cli/src/detect.rs:1013:54: replace * with / in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:1013:54: replace * with + in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:1015:49: replace * with / in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:1015:49: replace * with + in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:1039:22: replace > with >= in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:128:44: replace * with + in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:132:53: delete - in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:142:24: replace += with *= in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:142:40: replace * with + in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:155:48: replace / with * in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:155:48: replace / with % in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:163:36: replace - with + in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:163:61: replace / with * in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:163:61: replace / with % in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:171:37: replace * with / in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:171:37: replace * with + in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:171:52: replace * with / in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:171:52: replace * with + in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:171:66: replace - with / in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:171:66: replace - with + in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:172:21: replace > with >= in otsu`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:234:48: replace + with - in components`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:235:48: replace + with - in components`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:236:31: replace < with <= in components`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:236:41: replace < with <= in components`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:473:19: replace < with > in merge_along`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:475:24: replace < with <= in merge_along`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:475:24: replace < with == in merge_along`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:475:24: replace < with > in merge_along`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:612:50: replace * with + in uncontained`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:620:87: replace < with <= in uncontained`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:925:19: replace || with && in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:942:22: replace * with + in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:965:34: replace && with || in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:965:54: replace && with || in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/detect.rs:965:74: replace && with || in detect_page_with`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.

### `crates/taiyaku-cli/src/duplicates.rs`

- `crates/taiyaku-cli/src/duplicates.rs:453:53: replace > with >= in contradictions`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/duplicates.rs:455:33: replace + with * in contradictions`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/fingerprint.rs`

- `crates/taiyaku-cli/src/fingerprint.rs:170:22: replace || with && in flat_border`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/library.rs`

- `crates/taiyaku-cli/src/library.rs:202:5: replace resolve_export_user -> Result<i64, String> with Ok(1)`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/library.rs:720:20: delete ! in export_targets`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/main.rs`

- `crates/taiyaku-cli/src/main.rs:1479:8: delete ! in serve_work`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1808:5: replace slot_index -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1808:5: replace slot_index -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1828:5: replace print_alignment_detail with ()`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:726:37: replace / with * in verify`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:726:37: replace / with % in verify`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:788:5: replace page_count -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:788:5: replace page_count -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/regions.rs`

- `crates/taiyaku-cli/src/regions.rs:115:5: replace load_beside -> Result<RegionDoc, String> with Ok(Default::default())`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:117:19: replace match guard e.kind() == std::io::ErrorKind::NotFound with true in load_beside`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:131:5: replace page_index -> u32 with 0`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:131:5: replace page_index -> u32 with 1`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:145:39: replace == with != in command`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:45:23: replace / with % in median_area`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:92:5: replace table -> String with "xyzzy".into()`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:92:5: replace table -> String with String::new()`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.
- `crates/taiyaku-cli/src/regions.rs:95:57: replace == with != in table`
  Deferred until L2 redesigns the detector. `detect.rs` and `regions.rs` are the
  credit-page detector and its region geometry, both of which L2 replaces
  outright — writing fixtures against a detector that is about to be deleted is
  work thrown away, and `docs/status.md` has carried that position since the
  August sweep. 45 survivors here, against the 44 recorded then.

  **This is the block the baseline exists for.** Reprinting these every week is
  what would train a reader to skim the weekly job's output, which is the one
  failure a job that gates nothing cannot survive.

### `crates/taiyaku-cli/src/sample.rs`

- `crates/taiyaku-cli/src/sample.rs:58:30: replace < with <= in dev_pages`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/score.rs`

- `crates/taiyaku-cli/src/score.rs:119:14: replace > with < in classify_single_cover`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:130:29: replace < with <= in classify_single_cover`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:150:12: replace * with + in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:151:20: replace > with >= in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:152:20: replace > with >= in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:155:13: replace && with || in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:261:46: replace += with -= in Report::add_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:261:46: replace += with *= in Report::add_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:304:41: replace == with != in Report::render`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:312:40: replace / with * in Report::render`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:312:40: replace / with % in Report::render`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:507:34: replace == with != in command`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:675:33: replace += with -= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:675:33: replace += with *= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:693:27: replace += with -= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:693:27: replace += with *= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:77:30: replace - with + in intersection`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:77:9: replace - with + in intersection`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/user.rs`

- `crates/taiyaku-cli/src/user.rs:116:5: replace read_password -> Result<String, String> with Ok("xyzzy".into())`
  The process boundary. `read_password` is a terminal prompt, two blocking reads
  from stdin and an `stty` call; nothing under `cargo test` has a tty. **The
  testable half is already extracted** — `accept(first, &second)` holds the rule
  about which inputs are refused and in what words, and it is tested; the
  commands take an injected `AskForPassword` rather than calling this at all.
  What survives here is the reading itself.
- `crates/taiyaku-cli/src/user.rs:116:5: replace read_password -> Result<String, String> with Ok(String::new())`
  The process boundary. `read_password` is a terminal prompt, two blocking reads
  from stdin and an `stty` call; nothing under `cargo test` has a tty. **The
  testable half is already extracted** — `accept(first, &second)` holds the rule
  about which inputs are refused and in what words, and it is tested; the
  commands take an injected `AskForPassword` rather than calling this at all.
  What survives here is the reading itself.
- `crates/taiyaku-cli/src/user.rs:119:8: delete ! in read_password`
  The `if !quiet` that warns when echo could not be turned off — a decision about
  what a terminal shows, reachable only with a tty. It sits above the extracted
  `accept` for the same reason the rest of this function does.
- `crates/taiyaku-cli/src/user.rs:159:5: replace read_line -> Result<String, String> with Ok("xyzzy".into())`
  `read_line` is `std::io::stdin().read_line`. There is nothing here but the read.
- `crates/taiyaku-cli/src/user.rs:159:5: replace read_line -> Result<String, String> with Ok(String::new())`
  `read_line` is `std::io::stdin().read_line`. There is nothing here but the read.
- `crates/taiyaku-cli/src/user.rs:169:5: replace set_echo -> bool with false`
  `set_echo` shells out to `stty` against an inherited stdin. Under a test harness
  stdin is not a tty, so it answers false — which would kill `-> true` and not
  `-> false`, and only while the harness captures stdin. A test whose result
  depends on whether the runner has a terminal is worse than this entry.
- `crates/taiyaku-cli/src/user.rs:169:5: replace set_echo -> bool with true`
  `set_echo` shells out to `stty` against an inherited stdin. Under a test harness
  stdin is not a tty, so it answers false — which would kill `-> true` and not
  `-> false`, and only while the harness captures stdin. A test whose result
  depends on whether the runner has a terminal is worse than this entry.

### `crates/taiyaku-core/src/route.rs`

- `crates/taiyaku-core/src/route.rs:131:61: replace - with / in route`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/route.rs:131:61: replace - with + in route`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/sidecar.rs`

- `crates/taiyaku-core/src/sidecar.rs:1326:46: replace || with && in Sidecar::trust`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/solve.rs`

- `crates/taiyaku-core/src/solve.rs:143:18: replace > with >= in calibrate`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/solve.rs:146:25: replace > with >= in calibrate`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/truth.rs`

- `crates/taiyaku-core/src/truth.rs:200:17: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:200:17: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:200:28: replace > with == in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:200:28: replace > with >= in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:201:40: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:201:40: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:203:28: replace > with >= in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:204:40: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:204:40: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/credential.rs`

- `crates/taiyaku-library/src/credential.rs:99:5: replace hasher -> argon2::Argon2<'static> with Default::default()`
  **An equivalent mutant, verified rather than assumed.**
  `argon2::Params::DEFAULT_M_COST`, `DEFAULT_T_COST` and `DEFAULT_P_COST` are
  19456, 2 and 1 — printed from the crate, not recalled — which is exactly what
  `M_COST_KIB`, `T_COST` and `P_COST` hold, because both are OWASP's current
  recommendation. `Argon2::default()` therefore produces the same hasher.

  **It stops being equivalent the day these parameters are raised**, which §14's
  pending measurement on the NAS may well do.
  `credential::tests::the_stored_hash_carries_this_modules_parameters` is what
  fails on that day: it reads the parameters back out of the PHC string, so a
  hasher built from anything but this module's constants is caught the moment
  they differ from the crate's defaults.

### `crates/taiyaku-library/src/derive.rs`

- `crates/taiyaku-library/src/derive.rs:1262:51: replace && with || in unattached_chapter_dirs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/import.rs`

- `crates/taiyaku-library/src/import.rs:437:36: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/import.rs:437:72: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/import.rs:441:74: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/lib.rs`

- `crates/taiyaku-library/src/lib.rs:142:5: replace is_busy -> bool with true`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/lib.rs:180:56: replace < with <= in set_wal_within`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/librarydoc.rs`

- `crates/taiyaku-library/src/librarydoc.rs:697:5: replace find_series_by_title -> Result<Option<i64>, String> with Ok(None)`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/migrate.rs`

- `crates/taiyaku-library/src/migrate.rs:221:9: replace SiblingMigration::is_empty -> bool with true`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:222:13: replace && with || in SiblingMigration::is_empty`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:223:13: replace && with || in SiblingMigration::is_empty`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:224:13: replace && with || in SiblingMigration::is_empty`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:382:14: replace > with < in held`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/proposals.rs`

- `crates/taiyaku-library/src/proposals.rs:115:26: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:784:54: replace && with || in raise_edition_of`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:846:20: replace += with *= in raise_same_series`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:867:53: replace += with *= in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:889:31: replace == with != in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:902:20: replace += with *= in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/scan.rs`

- `crates/taiyaku-library/src/scan.rs:200:17: replace > with >= in scan_root`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:357:28: replace += with *= in record`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:462:23: replace match guard format == taiyaku_read::Format::Zip.token() with true in pages_of`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:652:14: replace > with == in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:652:14: replace > with >= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:682:21: replace += with -= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:682:21: replace += with *= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/schema.rs`

- `crates/taiyaku-library/src/schema.rs:242:20: replace > with < in apply_pending`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/state.rs`

- `crates/taiyaku-library/src/state.rs:197:5: replace work_slug -> Result<String, String> with Ok("xyzzy".into())`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/state.rs:197:5: replace work_slug -> Result<String, String> with Ok(String::new())`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/watch.rs`

- `crates/taiyaku-library/src/watch.rs:142:30: replace match guard names_the_probe(&event, &probe_path) with true in probe`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:283:13: replace > with >= in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:285:20: replace > with >= in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:285:24: replace && with || in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/format.rs`

- `crates/taiyaku-read/src/format.rs:138:16: replace < with <= in read_header`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/naming.rs`

- `crates/taiyaku-read/src/naming.rs:238:40: replace && with || in marked_number`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/naming.rs:238:56: replace + with - in marked_number`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/pages.rs`

- `crates/taiyaku-read/src/pages.rs:390:5: replace page_count -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:390:5: replace page_count -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:41: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:41: replace * with +`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:48: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:48: replace * with +`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-serve/src/auth.rs`

- `crates/taiyaku-serve/src/auth.rs:224:12: delete ! in client_of`
  The `!SAID.swap(true, Ordering::Relaxed)` that makes the `X-Forwarded-For`
  warning print once per process. Deleting the `!` inverts which call prints, and
  no in-process test can see it: a second test in the same binary observes an
  atomic the first already swapped, so any assertion here depends on test order.

  **The decision it guards is tested.** `forwarded_but_untrusted` was extracted
  out of this `if` for exactly that reason and
  `auth::tests::the_forwarded_warning_fires_only_when_a_header_is_being_ignored`
  covers it. What is left is the once-ness, which is the part that is genuinely
  about the process rather than about the request.
- `crates/taiyaku-serve/src/auth.rs:647:26: replace || with && in from_basic`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.
- `crates/taiyaku-serve/src/auth.rs:663:26: replace || with && in from_basic`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.

### `crates/taiyaku-serve/src/lib.rs`

- `crates/taiyaku-serve/src/lib.rs:352:8: delete ! in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:353:88: delete ! in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:353:9: replace && with || in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:370:13: delete match arm Ok(true) in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-serve/src/routes.rs`

- `crates/taiyaku-serve/src/routes.rs:657:9: replace <impl IntoResponse for NoPrincipal>::into_response -> Response with Default::default()`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so the weekly job's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-serve/src/session.rs`

- `crates/taiyaku-serve/src/session.rs:355:30: replace || with && in login`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.
- `crates/taiyaku-serve/src/session.rs:371:30: replace || with && in login`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.
