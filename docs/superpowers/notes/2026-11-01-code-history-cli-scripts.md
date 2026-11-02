# Code history moved out of comments

Text moved verbatim out of code comments when they were trimmed to their why. Each entry names the file and where the comment sat in it.

## `crates/taiyaku-cli/src/geometry_experiment.rs`, above `const SAMPLE: usize = 24;`

24 rather than the 8 the first run used: at 8 the run took 26 s in release
and the interesting figure — the p90 of the residual, which is what the
promotion rule turns on — rested on 96 slots, so its tail was nine of them.
24 costs about a minute and a half and puts 288 slots behind it.

## `crates/taiyaku-cli/src/credits/tests.rs`, above `fn hub_with_a_spoke_to_spoke_edge() -> Sidecar {`

The buggy version of `resolve_counterpart` mapped every alignment to
"whichever side is not the hub", so `spokeA`-`spokeB` still contributed
`spokeA` even though that edge never reaches the hub at all.

## `crates/taiyaku-cli/src/detect.rs`, above `pub(crate) fn uncontained(blocks: &[Block]) -> Vec<Block> {`

Over the 90 band-free corpus pages `region_experiment.rs` samples,
the v/h split of surviving blocks moved from 55/21 to 65/8 across this
change. 69 blocks survive both before and after; 13 of them changed
axis, all horizontal to vertical, and all 13 on a block whose box,
stroke count, median stroke and ink are identical in the two runs. An
unchanged box that changes label is a duplicate being resolved by the
rule below, not a reading direction being discovered.

## `crates/taiyaku-cli/src/region_experiment.rs`, above `use std::path::{Path, PathBuf};`

*What that moved, measured before and after rather than assumed.* Almost
nothing in the paragraphs above, which is why they stand unedited:
**`kobayashi-v01/raw-mono` page 7 is identical** — 3,682 components, 63%
under 5px, `stroke_scale` 3px giving a 6px gap, one final block, 51.6%
discarded — so `notes.md`'s row still reproduces line for line. So are
"82 of 90 pages have at least half their components under 5px" and every
figure in the `MAX_BLOCK_AREA` sentence (89 of 90 pages; 21.3% to 100.0%;
median 51.6%). **One figure above is now wrong and is corrected here rather
than edited in place:** "55 of 90 (61%) come back with zero final blocks"
is **58 of 90 (64%)**. Over the whole sample the final block count went
from 76 to 73, and the v/h split of surviving blocks from 55/21 to 65/8 —
the axis shift being mostly blocks both smears produce identically, which
`detect::uncontained`'s doc comment states and bounds.

## `crates/taiyaku-cli/src/report.rs`, above `image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 70)`

A whole volume pair still lands
around 3.5 MB, which is small enough to open and to keep.

## `crates/taiyaku-cli/src/report.rs`, above `#[expect(`

Smaller ranges were tried and read as a whisper

## `.cargo/mutants.toml`, above `test_tool = "cargo"`

Turning it on took `taiyaku-wasm` from 15 s to 63 s, and on a 32-core
machine it multiplied a bounded-per-process memory limit by thirty-two —
exhausting 60 GB within a minute of first being enabled.

## `.forgejo/workflows/ci.yml`, above `jobs:`

Measured from the forge's own /actions/tasks API over
those ten runs: `test` 32-134 s, `lint` 21-85 s, `wasm` 33-84 s, `web` 9-25 s.

## `scripts/coverage-floor.mjs`, above `lines: 57,`

Lowest on a clean tree, 2026-08-14: taiyaku-serve/src/truth.rs 73.68%,
then taiyaku-serve/src/lib.rs 77.85%, taiyaku-cli/src/credits.rs
82.51%, taiyaku-core/src/regions.rs 85.71%, stump/resolve.rs 87.61%.

## `scripts/coverage-floor.mjs`, above `import { readFileSync } from 'node:fs'`

**That margin has about five lines of noise in it, and the noise was measured
rather than assumed.** Two runs of the same command on the same tree gave
6735 and 6740 covered lines — 93.95% and 94.02% — and the difference is
entirely one file: `taiyaku-serve/src/cover.rs`, lines 257-260, which are
`Semaphore::acquire`'s `while *available == 0 { … wait … }` contention branch.
They are covered when two decodes actually collide on the permit and
uncovered when the scheduler happens not to overlap them. So the figures
quoted here are the lower of the two runs, and the honest margin is 67 lines
rather than 68. (The semaphore has since moved to `limit.rs`, where a
password verify waits on it too; the two runs are from before that.)
