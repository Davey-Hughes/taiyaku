# Code history moved out of comments

Text moved verbatim out of code comments when they were trimmed to their why. Each entry names the file and the line of code the comment sat above.

## `crates/taiyaku-core/src/sidecar.rs`, above `fn carry_hand_runs(`

**An earlier version of this function filtered out whole solved runs by
page-range overlap instead of splitting them**, on the reasoning that the
two lists are already in the document's own page notation and converting
to slot indices and back would need both slot tables for no gain. That
reasoning covers *detecting* overlap, which page order tracks exactly as
well as slot order does — it does not cover *keeping what survives it*.
The preservation test below measured the difference: two one-slot
corrections at slots 5 and 9 inside a single 30-slot solved run left slots
0-4, 6-8 and 10-29 with no counterpart at all under the drop-the-run
version, where `docs/sidecar.md` promises only the corrected slots move.
So this does need the slot tables, to split each overlapping solved run
around every hand run it contains rather than discard it, and converts
back to page notation through them once the splitting is done.

## `crates/taiyaku-core/src/solve.rs`, above `const MIN_AVERAGE_RUN: f32 = 27.0;`

That 6.8 is the one-chapter-to-volume subset; the highest among the
known-bad copies, the needle and the unrelated pairs is 4.8.

Color Walk's nine art book pages
score 1.0 / 9 = 0.11 where Nausicaä's genuine 4:1 repagination scores
46.1 / 600 = 0.077, so the needle would outrank a real alignment.

## `crates/taiyaku-core/src/solve.rs`, above `pub signal: f32,`

Estimating from a high percentile instead fixes that case and
costs more than it buys: with the 90th percentile, Go-Toubun's raw
against Kaguya's English release pairs 146 of 195 pages instead of being
refused, and Kaguya's own alignment degrades from 8 runs and 204 pages
paired to 12 and 201 as the raised floor rejects real matches.

## `crates/taiyaku-core/src/solve.rs`, above `#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]`

a 16x16 difference hash
resolved 99.5% of the sanity-baseline pair against 97.9% for the usual
64-bit form, and wider hashes bought nothing further.

## `crates/taiyaku-core/src/solve.rs`, above `#[must_use]`

Measured across the corpus, correct matches
land anywhere from 0.65 to 0.98 depending on which pair of editions is being
compared — colourisation alone moves it 0.10 to 0.15 — while unrelated pages
sit near 0.5 throughout.
