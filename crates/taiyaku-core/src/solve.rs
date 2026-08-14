//! Recovering a page correspondence from two sequences of page hashes.

use crate::{PageRange, Run};

/// A page or slot index, as the sidecar format spells it.
///
/// # Panics
///
/// Panics if `i` exceeds `u32::MAX`. It cannot in this crate: every call site
/// in this module derives `i` from the length of, or an `enumerate()` over, a
/// `&[PageHash]` — one hash per page of a scanned book — and reaching
/// `u32::MAX` would take 4 billion pages, 137 GB of hashes alone. The
/// `expect` is here so that an impossible input is loud rather than a
/// silently wrong index, which is what both a truncating `as` and a clamp
/// would produce.
fn page_index(i: usize) -> u32 {
    u32::try_from(i).expect("page index exceeds u32::MAX")
}

/// A 256-bit structural page hash.
///
/// 256 bits because that is what the corpus asked for: a 16x16 difference hash
/// resolved 99.5% of the sanity-baseline pair against 97.9% for the usual
/// 64-bit form, and wider hashes bought nothing further.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PageHash(pub [u64; 4]);

/// Fraction of bits two hashes agree on, in `0.0..=1.0`.
#[must_use]
pub fn similarity(a: PageHash, b: PageHash) -> f32 {
    let differing: u32 =
        a.0.iter()
            .zip(b.0.iter())
            .map(|(x, y)| (x ^ y).count_ones())
            .sum();
    // differing sums four u32::count_ones() results, each at most 64, over the
    // fixed 4-element PageHash array, so differing is at most 256 — clamped
    // here so the u16 narrowing below is checked rather than merely true, and
    // f32::from(u16) is then exact for every value that type can hold.
    let differing = f32::from(differing.min(256) as u16);
    1.0 - differing / 256.0
}

/// Cost of leaving a page unpaired.
///
/// Small on purpose: a gap should win whenever the best available match is
/// meaningfully below the floor, and lose whenever it is not, so the penalty
/// only has to break ties between equally plausible paths.
const GAP_PENALTY: f32 = 0.05;

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
    /// the gap from that best to the second best does not. The survey measures
    /// Color Walk's art book pages against a One Piece volume at a margin of
    /// 0.004 — one bit of a 256-bit hash — where a genuine pairing of the same
    /// shape sits an order of magnitude higher.
    ///
    /// Being a median of `best - runner_up`, this is never negative. The signed
    /// per-page figures the notes quote for the art book are `confidence`'s
    /// margin, a different quantity: that one scores the counterpart the
    /// alignment actually chose against its best rival, so it goes negative
    /// when a better-scoring candidate was passed over.
    pub margin: f32,
    /// The similarity below which a pairing is not worth making, midway
    /// between noise and signal.
    pub floor: f32,
}

impl Calibration {
    /// How far the signal stands clear of the noise.
    #[must_use]
    pub fn separation(&self) -> f32 {
        self.signal - self.noise
    }

    /// Whether any pairing between these two editions is worth making.
    #[must_use]
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
#[must_use]
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
            // Both comparisons are strict, and both survive mutation to `>=`
            // for good: the two spellings cannot be told apart, so they stay
            // in `missed.txt` on every run and need no test written for them.
            //
            // The loop keeps `row_second <= row_best`. So when `s == row_best`
            // the strict form falls through and raises the runner-up to `s`,
            // while `>=` shifts the old best — which is `s` — down into it:
            // both end at `(row_best, row_second) = (s, s)`. And the second
            // comparison is only reached when `s <= row_best`, where `>=`
            // differs only by assigning `row_second` the value it already
            // holds. Not even a signed zero separates them: `row_second`'s
            // only consumers are the subtraction and the `is_finite` below,
            // and neither can tell `+0.0` from `-0.0`.
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
        margins.push(if row_second.is_finite() {
            row_best - row_second
        } else {
            row_best
        });
    }
    let noise = median(&mut all);
    let signal = median(&mut best);
    Some(Calibration {
        noise,
        signal,
        margin: median(&mut margins),
        floor: f32::midpoint(noise, signal),
    })
}

/// How far the signal must stand clear of the noise before any pairing is
/// believed.
///
/// Without this the estimator is circular: given two unrelated editions it
/// still finds a midpoint, and everything above that midpoint becomes a
/// confident pairing. Refusing to align is the correct answer for a pair that
/// does not correspond, and an alignment nobody can tell is wrong is worse than
/// none at all.
///
/// The value is provisional. Genuine pairings in the corpus separate by 0.21 or
/// more; the two known mislabelled copies separate by 0.11 to 0.14, so this
/// admits them and wants tuning against the corpus before it is trusted to
/// reject a bad file on its own.
const MIN_SEPARATION: f32 = 0.10;

fn median(values: &mut [f32]) -> f32 {
    percentile(values, 0.5)
}

/// The value standing `fraction` of the way through `values`, once sorted.
///
/// The `- 1.0` is what makes `fraction` 1.0 select the last element rather than
/// run past the end, and 0.0 the first. There is no clamp on `at` because it
/// cannot exceed the last index: `fraction` is bounded to `0.0..=1.0` by every
/// caller, so `(len - 1) * fraction` is at most `len - 1` before rounding. That
/// bound is now load bearing rather than merely true — out of range indexes
/// past the end instead of clamping — so it is asserted rather than trusted.
fn percentile(values: &mut [f32], fraction: f32) -> f32 {
    debug_assert!((0.0..=1.0).contains(&fraction));
    values.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "fraction is bounded to 0.0..=1.0 by every caller, asserted just above; and \
                  values is non-empty on every call in this module — calibrate returns None \
                  before percentile is reached when a or b is empty, and every direct call in \
                  this file's tests passes a non-empty slice — so len is at least 1 and \
                  (len - 1) * fraction lies in 0.0..=(len - 1): never negative, never past the \
                  last index. Tried clamping the operand with .clamp(0.0, ...) before the cast, \
                  matching the pattern that works for integer-to-integer narrowing elsewhere in \
                  this file — confirmed by direct experiment that clippy's cast_possible_truncation \
                  and cast_sign_loss give this no credit for a float source: every float-to-usize \
                  cast is flagged regardless of any .clamp()/.max() bound placed before it, clamped \
                  or not, literal or named const. No infallible float-to-usize conversion exists, \
                  and percentile returns f32, not Result, so there is no error to propagate and \
                  none would be reachable — the caller-side bound is real but this function has no \
                  way to hand a violation back."
    )]
    #[expect(
        clippy::cast_precision_loss,
        reason = "f32 represents every integer below 2^24 exactly. values.len() is a slot \
                  count bounded by what a physical book can contain, far below the 16.7 \
                  million where precision loss begins. Tried narrowing values.len() to u16 first \
                  (the route that closed differing's cast_precision_loss above in similarity()) \
                  and rejected it: that route only holds where the code itself proves a tight \
                  bound — differing is a sum of count_ones() results with a provable ceiling of \
                  256 — where here the only bound is the empirical 'a physical book can contain', \
                  true well below 2^24 but never claimed to be true below u16::MAX (65 535). \
                  Narrowing to u16 would silently misreport any input between those two ceilings \
                  instead of converting it exactly, trading a wide, honest margin for a narrow, \
                  invented one clippy cannot itself verify — confirmed separately that \
                  cast_precision_loss carries no clamp-based range analysis at all, so a .min() \
                  before this cast changes nothing regardless of the bound chosen. A third \
                  route was tried too: values.len() is usize, and std has no From<usize> \
                  for f32 (or f64) — usize's width is platform-dependent, unlike \
                  u32/u16/u8, which do have infallible From impls into these float types. \
                  So this stays a cast, not a conversion."
    )]
    let at = ((values.len() as f32 - 1.0) * fraction).round() as usize;
    values[at]
}

/// Which predecessor a cell's score came from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Pair A[i] with B[j].
    Pair,
    /// A[i] has no counterpart.
    SkipA,
    /// B[j] has no counterpart.
    SkipB,
}

/// Recover the correspondence from edition A to edition B.
///
/// Needleman-Wunsch over the page-similarity matrix. The correspondence is
/// monotonic but admits insertions on both sides, which is exactly what this
/// recurrence expresses. Pages score `similarity - floor`, so a match below the
/// floor is worse than no match at all and becomes a gap.
#[must_use]
#[expect(
    clippy::many_single_char_names,
    reason = "a and b are the two editions being aligned (matching this module's own doc \
              comment and every other function here — similarity(a, b), calibrate(a, b)); m \
              and n are their lengths and i and j the indices walking the Needleman-Wunsch \
              matrix these are standard names for, matching CLRS and every textbook treatment \
              of this recurrence. Renaming them to `edition_a_len` and `edition_b_len` would \
              make this harder to check against the algorithm it implements, not easier."
)]
pub fn solve(a: &[PageHash], b: &[PageHash]) -> Vec<Run> {
    if a.is_empty() {
        return Vec::new();
    }
    let floor = match calibrate(a, b) {
        Some(cal) if cal.accepted() => cal.floor,
        _ => return all_unmatched(a.len()),
    };
    let (m, n) = (a.len(), b.len());
    let at = |i: usize, j: usize| i * (n + 1) + j;

    let mut score = vec![0f32; (m + 1) * (n + 1)];
    let mut from = vec![Step::Pair; (m + 1) * (n + 1)];
    for i in 1..=m {
        #[expect(
            clippy::cast_precision_loss,
            reason = "i is a 1-based page index of edition A, at most m == a.len(), a slot \
                      count bounded by what a physical book can contain — far below the \
                      2^24 where f32 begins losing integer precision. Tried the u16-narrowing \
                      route used for differing in similarity(): rejected, because that route \
                      needs a bound the code itself proves (differing's is an exact ceiling of \
                      256 from summed count_ones() results), and this bound is empirical — 'a \
                      physical book can contain' is true well under 2^24 but was never claimed \
                      true under u16::MAX, so narrowing would trade a wide, honest margin for a \
                      narrow, invented one. Also tried clamping i itself before the cast; \
                      cast_precision_loss has no clamp-based range analysis at all (confirmed by \
                      direct experiment), so no bound placed before this cast changes anything. \
                      A third route was tried too: i is usize, and std has no From<usize> \
                      for f32 — usize's width is platform-dependent, unlike u32/u16/u8, \
                      which do have infallible From impls into f32/f64."
        )]
        let i_f32 = i as f32;
        score[at(i, 0)] = -i_f32 * GAP_PENALTY;
        from[at(i, 0)] = Step::SkipA;
    }
    for j in 1..=n {
        #[expect(
            clippy::cast_precision_loss,
            reason = "j is a 1-based page index of edition B, at most n == b.len(), a slot \
                      count bounded by what a physical book can contain — far below the \
                      2^24 where f32 begins losing integer precision. Same attempt and same \
                      rejection as i_f32 just above: no code-proven bound tight enough to \
                      narrow into u16 exists, and cast_precision_loss ignores clamps entirely. \
                      Same closed third route as i_f32 above, too: j is usize, and std has \
                      no From<usize> for f32."
        )]
        let j_f32 = j as f32;
        score[at(0, j)] = -j_f32 * GAP_PENALTY;
        from[at(0, j)] = Step::SkipB;
    }

    for i in 1..=m {
        for j in 1..=n {
            let pair = score[at(i - 1, j - 1)] + similarity(a[i - 1], b[j - 1]) - floor;
            let skip_a = score[at(i - 1, j)] - GAP_PENALTY;
            let skip_b = score[at(i, j - 1)] - GAP_PENALTY;

            // Ties favour pairing, which keeps runs contiguous rather than
            // splitting them around an equally-scored detour.
            let (best, step) = if pair >= skip_a && pair >= skip_b {
                (pair, Step::Pair)
            } else if skip_a >= skip_b {
                (skip_a, Step::SkipA)
            } else {
                (skip_b, Step::SkipB)
            };
            score[at(i, j)] = best;
            from[at(i, j)] = step;
        }
    }

    let mut pairs: Vec<(u32, Option<u32>)> = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 {
        match from[at(i, j)] {
            Step::Pair => {
                pairs.push((page_index(i) - 1, Some(page_index(j) - 1)));
                i -= 1;
                j -= 1;
            }
            Step::SkipA => {
                pairs.push((page_index(i) - 1, None));
                i -= 1;
            }
            Step::SkipB => j -= 1,
        }
    }
    pairs.reverse();
    compress(&pairs)
}

/// Every page of A, with no counterpart in B.
fn all_unmatched(len: usize) -> Vec<Run> {
    compress(&(0..len).map(|i| (page_index(i), None)).collect::<Vec<_>>())
}

/// Collapse a per-page pairing into runs.
///
/// Consecutive pages extend the current run while both sides keep advancing by
/// one; anything else starts a new run. This is what keeps a clean diagonal to
/// three lines rather than 195 objects, and what makes a sidecar diff readable.
pub(crate) fn compress(pairs: &[(u32, Option<u32>)]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for &(ai, bi) in pairs {
        let extended = match runs.last_mut() {
            Some(last) => match (last.b, bi) {
                (Some(lb), Some(b)) if last.a.end + 1 == ai && lb.end + 1 == b => {
                    last.a.end = ai;
                    last.b = Some(PageRange::new(lb.start, b));
                    true
                }
                (None, None) if last.a.end + 1 == ai => {
                    last.a.end = ai;
                    true
                }
                _ => false,
            },
            None => false,
        };
        if !extended {
            runs.push(match bi {
                Some(b) => Run::paired(PageRange::new(ai, ai), PageRange::new(b, b)),
                None => Run::gap(PageRange::new(ai, ai)),
            });
        }
    }
    runs
}

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
    /// Nothing paired: these editions do not correspond.
    Refused,
    /// Paired, but stitched from so many short runs that it is not believable.
    Fragmented,
    /// A small number of long runs, as a real correspondence produces.
    Sound,
}

/// Judge an alignment from its shape.
///
/// Coverage is checked before fragmentation, and that order still matters. A
/// refused pair is a single gap run covering everything, which
/// `average_run_length` reports as 0.0 — below the fragmentation threshold, so
/// without this check first a total refusal would read as `Fragmented` rather
/// than `Refused`. The two mean different things: refused says nothing
/// corresponds, fragmented says something did but too little of it to trust.
/// Measured across the corpus: every confirmed-good pairing averages 36.0
/// slots a run or more, and everything that must not pass 6.8 or fewer — 6.8
/// being the one-chapter-to-volume subset, with the known-bad copies, the
/// needle and the unrelated pairs all at 4.8 or below.
#[must_use]
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
    #[expect(
        clippy::cast_precision_loss,
        reason = "pages is an edition's page count, a slot count bounded by what a physical \
                  book can contain — far below the 2^24 where f32 begins losing integer \
                  precision. Tried narrowing to u16 first, as differing does in similarity(): \
                  rejected, because differing has a bound the arithmetic proves (a sum of \
                  count_ones() results ceilinged at 256), while pages only has the empirical \
                  physical-book ceiling, never claimed tighter than u16::MAX — narrowing would \
                  swap a wide, honest margin for one this file cannot back up. Also tried \
                  clamping pages before the cast; cast_precision_loss carries no clamp-based \
                  range analysis (confirmed by direct experiment against clippy 1.97), so no \
                  bound placed before an integer-to-float cast changes whether it fires. A \
                  third route was tried too: pages (this function's usize parameter) has no \
                  From<usize> for f32 in std — usize's width is platform-dependent, unlike \
                  u32/u16/u8, which do have infallible From impls into f32/f64."
    )]
    let cap = MIN_AVERAGE_RUN.min(pages as f32);
    // average_run_length now returns f64 (see its own doc). f64::from(f32) is
    // lossless, so promoting cap for the comparison changes no value: cap
    // stays exactly what it was, computed in f32 as before, only compared at
    // higher precision.
    if average_run_length(runs) < f64::from(cap) {
        Verdict::Fragmented
    } else {
        Verdict::Sound
    }
}

/// How many slots of A the alignment found a counterpart for.
///
/// Public because a sidecar records it: a reader holding only the document
/// should be able to see how much of an edition was placed without opening the
/// archives.
#[must_use]
pub fn paired_slots(runs: &[Run]) -> u32 {
    runs.iter()
        .filter(|r| r.b.is_some())
        .map(|r| r.a.len())
        .sum()
}

/// Average paired-run length below which an alignment is confetti rather than a
/// correspondence.
///
/// Measured, not guessed. Across the corpus survey every confirmed-good pairing
/// averages 36.0 slots a run or more, up to 200.0 for a pair that is one run
/// covering the whole volume; everything that must not pass averages 6.8 or
/// fewer. That 6.8 is the one-chapter-to-volume subset; the highest among the
/// known-bad copies, the needle and the unrelated pairs is 4.8. This sits at
/// three quarters of the weakest good figure, clearing 6.8 by nearly four
/// times (27.0 / 6.8 = 3.97).
///
/// This replaced runs per hundred pages, but not because that measure failed on
/// this corpus: gated on coverage it separates too, the good rows running 0.5 to
/// 4.9 per hundred and everything that must not pass 21.2 to 100.0. The swap was
/// made because run length states the property in slots and so does not have to
/// be rescaled by the edition's length before it means anything, and because the
/// two disagree on a shape no corpus row happens to take — twenty paired runs of
/// two slots across 210 pages reads as 9.5 per hundred and therefore sound,
/// while averaging two slots a run.
///
/// The old measure's hole at the bottom is real and already gated: a wholly
/// refused pair is one gap run covering everything and so scores 0.5 per
/// hundred, matching the best correct alignment measured — the One Piece v092
/// identity pairing, one run over 200 slots, also scores 0.5 — and beating every
/// other. Average run length excludes gap runs by construction, so the same pair
/// reports 0.0, which is equally below the threshold and equally needs coverage
/// checked first — see `verdict`.
///
/// `verdict` caps this by the edition's own length, because the constant is
/// unmeasured below 146 slots: the shortest confirmed-good pairing in the
/// survey is Kobayashi v01 at 146, so nothing here says what a twenty-page
/// one-shot should score. Uncapped, such an edition is `Fragmented` by
/// arithmetic — its average cannot exceed its length — however cleanly it
/// aligns.
///
/// Making the threshold *proportional* to the length is the obvious
/// alternative, and it fails on this corpus: Color Walk's nine art book pages
/// score 1.0 / 9 = 0.11 where Nausicaä's genuine 4:1 repagination scores
/// 46.1 / 600 = 0.077, so the needle would outrank a real alignment. The cap
/// keeps the measured absolute figure wherever it has been measured, and only
/// relaxes below the range the corpus covers.
const MIN_AVERAGE_RUN: f32 = 27.0;

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
#[must_use]
pub fn average_run_length(runs: &[Run]) -> f64 {
    // Folded rather than collected: the sum and the count are all this needs,
    // and an intermediate allocation is not worth making in the crate that
    // compiles to WebAssembly, on a path an align walks up to three times.
    let (total, count) = runs
        .iter()
        .filter(|r| r.b.is_some())
        .fold((0u32, 0u32), |(total, count), run| {
            (total + run.a.len(), count + 1)
        });
    match count {
        0 => 0.0,
        // total and count are both derived from a's paired-run lengths, slot
        // counts bounded by what a physical book can contain — far below the
        // 2^24 where f32 begins losing integer precision, and further still
        // below f64's 2^53. total and count are u32, and f64::from(u32) is
        // lossless outright (u32's 32 bits fit f64's 52-bit mantissa), where
        // there is no equivalent `From<u32> for f32` (f32's mantissa is only
        // 24 bits) — so widening this function's return type to f64 closes
        // the cast without a suppression. `verdict` promotes its f32 cap to
        // f64 with `f64::from` to compare against this, which is itself
        // lossless, so the threshold value being compared against is
        // unchanged.
        _ => f64::from(total) / f64::from(count),
    }
}

/// How well one page's pairing is supported by the evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageConfidence {
    /// Index in edition A.
    pub page: u32,
    /// Where the alignment sent it, if anywhere.
    pub counterpart: Option<u32>,
    /// Similarity to the counterpart. For a gap, the best score that was
    /// nonetheless rejected, which is what explains the gap.
    pub similarity: f32,
    /// How far the counterpart beats the next best candidate in B.
    pub margin: f32,
}

/// Score every page of A against the alignment it was given.
///
/// Margin, not similarity, is the signal worth showing. A page that resembles
/// one page of B and nothing else is safe at 0.75; a chapter divider that
/// resembles all nine of its siblings is dangerous at 0.98, and only the margin
/// tells them apart. The verification report shades on this so the eye is drawn
/// to the pairings that need a human.
#[must_use]
pub fn confidence(a: &[PageHash], b: &[PageHash], runs: &[Run]) -> Vec<PageConfidence> {
    let pages = page_index(a.len());
    (0..pages)
        .map(|page| {
            let hash = a[page as usize];
            let scores: Vec<f32> = b.iter().map(|hb| similarity(hash, *hb)).collect();
            let best = scores
                .iter()
                .enumerate()
                .max_by(|x, y| x.1.total_cmp(y.1))
                .map(|(j, s)| (page_index(j), *s));

            let counterpart = crate::counterpart(runs, page);
            // Report the pairing the alignment actually made, not the highest
            // scorer. Where the two differ the alignment passed over a better
            // score to keep the sequence monotonic, and hiding that would
            // conceal exactly the disagreement a reviewer needs to see.
            let subject = counterpart.or(best.map(|(j, _)| j));
            let similarity_here = subject
                .and_then(|j| scores.get(j as usize).copied())
                .unwrap_or(0.0);
            let rival = scores
                .iter()
                .enumerate()
                .filter(|(j, _)| Some(page_index(*j)) != subject)
                .map(|(_, s)| *s)
                .fold(f32::NEG_INFINITY, f32::max);

            PageConfidence {
                page,
                counterpart,
                similarity: similarity_here,
                // Signed on purpose: negative means a better-scoring candidate
                // was rejected, which is usually monotonicity doing its job and
                // occasionally the alignment being wrong.
                margin: if rival.is_finite() {
                    similarity_here - rival
                } else {
                    similarity_here
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
