//! Recovering a page correspondence from two sequences of page hashes.

use crate::{PageRange, Run};

/// A 256-bit structural page hash.
///
/// 256 bits because that is what the corpus asked for: a 16x16 difference hash
/// resolved 99.5% of the sanity-baseline pair against 97.9% for the usual
/// 64-bit form, and wider hashes bought nothing further.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageHash(pub [u64; 4]);

/// Fraction of bits two hashes agree on, in `0.0..=1.0`.
pub fn similarity(a: PageHash, b: PageHash) -> f32 {
    let differing: u32 =
        a.0.iter()
            .zip(b.0.iter())
            .map(|(x, y)| (x ^ y).count_ones())
            .sum();
    1.0 - differing as f32 / 256.0
}

/// Cost of leaving a page unpaired.
///
/// Small on purpose: a gap should win whenever the best available match is
/// meaningfully below the floor, and lose whenever it is not, so the penalty
/// only has to break ties between equally plausible paths.
const GAP_PENALTY: f32 = 0.05;

/// Choose the similarity below which a pairing is not worth making, from the
/// two editions themselves.
///
/// A fixed threshold cannot work. Measured across the corpus, correct matches
/// land anywhere from 0.65 to 0.98 depending on which pair of editions is being
/// compared — colourisation alone moves it 0.10 to 0.15 — while unrelated pages
/// sit near 0.5 throughout. What identifies a real match is therefore its
/// distance from that noise, not its absolute value.
///
/// Most cells in the matrix are non-matches, so their median estimates the noise
/// level; the median of each page's best match estimates the signal level. The
/// midpoint separates them. Medians rather than means because both populations
/// have long tails — blank pages score 1.0 against each other and drag a mean up.
/// Returns `None` when the two editions show no correspondence worth trusting.
fn calibrate_floor(a: &[PageHash], b: &[PageHash]) -> Option<f32> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let mut all: Vec<f32> = Vec::with_capacity(a.len() * b.len());
    let mut best: Vec<f32> = Vec::with_capacity(a.len());
    for pa in a {
        let mut row_best = 0.0f32;
        for pb in b {
            let s = similarity(*pa, *pb);
            all.push(s);
            row_best = row_best.max(s);
        }
        best.push(row_best);
    }
    let noise = median(&mut all);
    let signal = median(&mut best);
    (signal - noise >= MIN_SEPARATION).then_some((noise + signal) / 2.0)
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
    values.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    values[values.len() / 2]
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
pub fn solve(a: &[PageHash], b: &[PageHash]) -> Vec<Run> {
    if a.is_empty() {
        return Vec::new();
    }
    let Some(floor) = calibrate_floor(a, b) else {
        return all_unmatched(a.len());
    };
    let (m, n) = (a.len(), b.len());
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
                pairs.push((i as u32 - 1, Some(j as u32 - 1)));
                i -= 1;
                j -= 1;
            }
            Step::SkipA => {
                pairs.push((i as u32 - 1, None));
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
    compress(&(0..len).map(|i| (i as u32, None)).collect::<Vec<_>>())
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
pub fn confidence(a: &[PageHash], b: &[PageHash], runs: &[Run]) -> Vec<PageConfidence> {
    (0..a.len() as u32)
        .map(|page| {
            let hash = a[page as usize];
            let scores: Vec<f32> = b.iter().map(|hb| similarity(hash, *hb)).collect();
            let best = scores
                .iter()
                .enumerate()
                .max_by(|x, y| x.1.total_cmp(y.1))
                .map(|(j, s)| (j as u32, *s));

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
                .filter(|(j, _)| Some(*j as u32) != subject)
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
mod tests {
    use super::*;

    /// A hash that is distinct per `seed` and stable across calls.
    fn h(seed: u64) -> PageHash {
        let mut w = [0u64; 4];
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
        for slot in w.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *slot = x;
        }
        PageHash(w)
    }

    /// Flip `bits` bits, to model a counterpart that is the same artwork seen
    /// through a different scan, crop and translation.
    fn degrade(p: PageHash, bits: u32) -> PageHash {
        let mut w = p.0;
        for k in 0..bits {
            w[(k / 64) as usize] ^= 1 << (k % 64);
        }
        PageHash(w)
    }

    /// The hazard the report exists to make visible. Chapter divider pages
    /// share a template, so one of them resembles all nine of its siblings.
    /// Similarity says 0.98 and means nothing; only the margin over the next
    /// candidate reveals that the choice between them was close to arbitrary.
    #[test]
    fn a_page_with_near_identical_rivals_scores_high_but_with_no_margin() {
        let template = h(1);
        // Four dividers barely distinguishable from each other, then one page
        // that resembles nothing else.
        let b: Vec<PageHash> = (0..4)
            .map(|k| degrade(template, k))
            .chain(std::iter::once(h(99)))
            .collect();
        let a = vec![b[0], b[4]];
        let runs = vec![Run::paired(PageRange::new(0, 1), PageRange::new(0, 1))];

        let scored = confidence(&a, &b, &runs);

        // Page 0 is paired with a near-twin of three other candidates.
        assert!(scored[0].similarity > 0.98, "{:?}", scored[0]);
        assert!(scored[0].margin < 0.02, "{:?}", scored[0]);
        // Page 1 was mis-paired by these hand-written runs; its real twin is
        // page 4, so the report must show the pairing it was actually given.
        assert_eq!(scored[1].counterpart, Some(1));
        assert!(scored[1].similarity < 0.7, "{:?}", scored[1]);
    }

    /// Some edition pairs are weak everywhere. Kaguya's raw against its English
    /// release differs by a median of 57-69 bits of 256, with a tenth of pages
    /// past 89 — similarity around 0.65 for real, correct matches. What makes a
    /// match real is standing clear of noise, not clearing an absolute bar, so
    /// a floor fixed in advance discards true pairings on the weaker editions.
    #[test]
    fn a_uniformly_weak_pairing_still_aligns() {
        let a: Vec<PageHash> = (0..6).map(h).collect();
        let b: Vec<PageHash> = a.iter().map(|p| degrade(*p, 97)).collect();
        // 97 of 256 bits => 0.621, well above the ~0.5 of unrelated pages.
        assert!((similarity(a[0], b[0]) - 0.621).abs() < 0.01);
        assert!(similarity(a[0], b[1]) < 0.60);

        let runs = solve(&a, &b);

        assert_eq!(
            runs,
            vec![Run::paired(PageRange::new(0, 5), PageRange::new(0, 5))]
        );
    }

    /// The commonest real pattern: the translation carries front matter the raw
    /// does not, so the whole volume sits at a constant offset. It must stay one
    /// run — a constant offset is the case run-length encoding exists for.
    #[test]
    fn a_constant_offset_stays_a_single_run() {
        let a: Vec<PageHash> = (0..5).map(h).collect();
        let mut b = vec![h(50), h(51)];
        b.extend(a.iter().copied());

        let runs = solve(&a, &b);

        assert_eq!(
            runs,
            vec![Run::paired(PageRange::new(0, 4), PageRange::new(2, 6))]
        );
    }

    /// An archive can legitimately contain no pages — a cbz holding only a
    /// ComicInfo.xml, say. Every page of A is then unmatched, which is an
    /// answer; panicking is not.
    #[test]
    fn an_empty_edition_leaves_every_page_unmatched() {
        let a: Vec<PageHash> = (0..3).map(h).collect();

        let runs = solve(&a, &[]);

        assert_eq!(runs, vec![Run::gap(PageRange::new(0, 2))]);
    }

    /// Two editions that are not the same volume must produce nothing, rather
    /// than the best available nonsense. The library really does contain files
    /// like this: two copies of [Oshi no Ko] v01 both claim the volume and only
    /// one of them is it. Calibrating the floor from the data is what makes this
    /// case dangerous — with no signal present, an estimator that always splits
    /// the difference will happily certify noise.
    #[test]
    fn unrelated_editions_produce_no_pairings() {
        let a: Vec<PageHash> = (0..6).map(h).collect();
        let b: Vec<PageHash> = (100..106).map(h).collect();

        let runs = solve(&a, &b);

        assert_eq!(runs, vec![Run::gap(PageRange::new(0, 5))]);
    }

    /// A colour insert present in the raw and dropped from the translation:
    /// the page must become a gap and the pages after it must keep matching,
    /// at an offset. Naive index-for-index pairing gets everything after the
    /// insert wrong, which is the whole reason sequencing is needed.
    #[test]
    fn a_page_absent_from_b_becomes_a_gap_and_shifts_the_rest() {
        let a: Vec<PageHash> = (0..4).map(h).collect();
        let b: Vec<PageHash> = vec![h(0), h(2), h(3)];

        let runs = solve(&a, &b);

        assert_eq!(
            runs,
            vec![
                Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
                Run::gap(PageRange::new(1, 1)),
                Run::paired(PageRange::new(2, 3), PageRange::new(1, 2)),
            ]
        );
    }

    #[test]
    fn identical_sequences_collapse_to_one_paired_run() {
        let pages: Vec<PageHash> = (0..5).map(h).collect();

        let runs = solve(&pages, &pages);

        assert_eq!(
            runs,
            vec![Run::paired(PageRange::new(0, 4), PageRange::new(0, 4))]
        );
    }
}
