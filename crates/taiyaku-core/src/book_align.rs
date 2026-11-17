//! Aligning two editions of a book, block by block, in reading order.
//!
//! The measurement design's §5 and §6
//! (`docs/superpowers/specs/2026-09-30-book-alignment-measurement-design.md`).
//! The manga problem one level down: two editions never line up
//! paragraph for paragraph, so the correspondence is recovered by a dynamic
//! program, as `solve` recovers pages. It is a separate program because a
//! translation splits and joins paragraphs: a step here consumes a group of
//! blocks on each side, up to `max_group`, where `solve` pairs one page with one.
//!
//! **The cost is the scorer's, and the program does not know what a block is.**
//! It sees two counts and asks a [`BlockScorer`] what pairing two groups costs,
//! so the scorers can be compared on one program. A block one side lacks is a
//! one-sided step at a fixed cost. Nothing here reads headings or chapters: a
//! table of contents can leave a chapter out (`docs/notes.md`, "Books: what the
//! library's EPUBs hold"), and an aligner that trusted it would be one chapter
//! off from there on.
//!
//! **A band, sized by measurement.** The search stays within `band` blocks of
//! the length-proportional diagonal, so a book that drifts further from it
//! than that is misaligned; the experiment scores a narrower band beside the
//! baseline's, which is how a band too narrow shows (`docs/notes.md`, "Books:
//! nine pairs, five keys, and what held out").

use std::ops::Range;

/// One step of an alignment: a run of A's blocks and the run of B's they pair
/// with. One side is empty when the other side's block has no counterpart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLink {
    pub a: Range<usize>,
    pub b: Range<usize>,
}

/// What pairing a group of A's blocks with a group of B's costs. Lower is better.
pub trait BlockScorer {
    /// `a` and `b` are both non-empty, and at most `max_group` long.
    fn cost(&self, a: Range<usize>, b: Range<usize>) -> f64;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockParams {
    /// The most blocks one step may take from either side.
    pub max_group: usize,
    /// What a block with no counterpart costs, or the first block of a run.
    pub skip_cost: f64,
    /// The most blocks one one-sided step may take. Each block past the first
    /// costs `skip_extend` rather than `skip_cost`, so a passage one edition
    /// lacks (an epigraph, a translator's note) is cheaper to leave out whole
    /// than to absorb into the merges either side of it. 1 makes every
    /// one-sided step a single block. At most 255.
    pub max_skip: usize,
    pub skip_extend: f64,
    /// How far from the diagonal, in B's blocks, the search may go. Raised to
    /// the least that still reaches the end, so a band never makes the program
    /// fail to finish.
    pub band: usize,
}

/// Align `a_len` blocks against `b_len`, returning the links in reading order.
///
/// Every block of each side is in exactly one link. Links are steps of at most
/// `max_group` blocks a side, or up to `max_skip` blocks against none.
#[must_use]
pub fn align_blocks(
    a_len: usize,
    b_len: usize,
    scorer: &impl BlockScorer,
    params: BlockParams,
) -> Vec<BlockLink> {
    let band = Band::new(a_len, b_len, params.band);
    let steps = steps(params.max_group.max(1), params.max_skip.clamp(1, 255));

    // `cost[i]` and `from[i]` hold row i's cells from `band.lo(i)` to `band.hi(i)`.
    let mut cost: Vec<Vec<f64>> = Vec::with_capacity(a_len + 1);
    let mut from: Vec<Vec<(u8, u8)>> = Vec::with_capacity(a_len + 1);
    for i in 0..=a_len {
        let width = band.hi(i) - band.lo(i) + 1;
        cost.push(vec![f64::INFINITY; width]);
        from.push(vec![(0, 0); width]);
        for j in band.lo(i)..=band.hi(i) {
            if i == 0 && j == 0 {
                cost[0][0] = 0.0;
                continue;
            }
            let mut best = (f64::INFINITY, (0, 0));
            for &(di, dj) in &steps {
                if di > i || dj > j {
                    continue;
                }
                let earlier = cell(&cost, &band, i - di, j - dj);
                if earlier.is_infinite() {
                    continue;
                }
                let step = if di == 0 || dj == 0 {
                    params.skip_cost + params.skip_extend * count(di + dj - 1)
                } else {
                    scorer.cost(i - di..i, j - dj..j)
                };
                if earlier + step < best.0 {
                    best = (earlier + step, (di, dj));
                }
            }
            cost[i][j - band.lo(i)] = best.0;
            from[i][j - band.lo(i)] = (
                u8::try_from(best.1.0).unwrap_or(u8::MAX),
                u8::try_from(best.1.1).unwrap_or(u8::MAX),
            );
        }
    }

    let mut links = Vec::new();
    let (mut i, mut j) = (a_len, b_len);
    while i > 0 || j > 0 {
        let (di, dj) = from[i][j - band.lo(i)];
        let (di, dj) = (usize::from(di), usize::from(dj));
        links.push(BlockLink {
            a: i - di..i,
            b: j - dj..j,
        });
        i -= di;
        j -= dj;
    }
    links.reverse();
    links
}

/// Every step a link may take: pairings first, so a tie keeps blocks paired
/// rather than detouring through two one-sided steps.
fn steps(max_group: usize, max_skip: usize) -> Vec<(usize, usize)> {
    let mut steps: Vec<(usize, usize)> = (1..=max_group)
        .flat_map(|di| (1..=max_group).map(move |dj| (di, dj)))
        .collect();
    steps.extend((1..=max_skip).flat_map(|n| [(n, 0), (0, n)]));
    steps
}

fn cell(cost: &[Vec<f64>], band: &Band, i: usize, j: usize) -> f64 {
    if j < band.lo(i) || j > band.hi(i) {
        return f64::INFINITY;
    }
    cost[i][j - band.lo(i)]
}

/// Align two books' texts, block by block, at the setting the measurement
/// chose: length with anchors, groups of up to four, one block left out at a
/// time (`docs/notes.md`, "Books: nine pairs, five keys, and what held out").
#[must_use]
pub fn align_texts(a: &[&str], b: &[&str]) -> Vec<BlockLink> {
    let lengths = |texts: &[&str]| -> Vec<u32> {
        texts
            .iter()
            .map(|t| u32::try_from(t.chars().count()).unwrap_or(u32::MAX))
            .collect()
    };
    let anchors =
        |texts: &[&str]| -> Vec<Anchors> { texts.iter().map(|t| Anchors::of(t)).collect() };
    let length = LengthScorer::new(&lengths(a), &lengths(b), GALE_CHURCH_VARIANCE);
    let scorer = AnchorScorer::new(length, &anchors(a), &anchors(b), 1.0);
    let params = BlockParams {
        max_group: 4,
        skip_cost: 4.5,
        max_skip: 1,
        skip_extend: 4.5,
        band: 400,
    };
    align_blocks(a.len(), b.len(), &scorer, params)
}

/// Align two books' texts as [`align_texts`] does, but with each pin `(i, j)` a
/// link a person asserted: A's block `i` and B's block `j` each start one. The
/// stretches between pins are aligned on their own, so a pin holds however the
/// text either side of it reads. A pin out of order with an earlier one, or
/// past either book's end, is passed over.
#[must_use]
pub fn align_texts_pinned(a: &[&str], b: &[&str], pins: &[(usize, usize)]) -> Vec<BlockLink> {
    let mut sorted = pins.to_vec();
    sorted.sort_unstable();
    let mut cuts = vec![(0, 0)];
    for (i, j) in sorted {
        let (last_i, last_j) = cuts[cuts.len() - 1];
        let after = i >= last_i && j >= last_j && (i, j) != (last_i, last_j);
        if after && i < a.len() && j < b.len() {
            cuts.push((i, j));
        }
    }
    cuts.push((a.len(), b.len()));
    let mut links = Vec::new();
    for pair in cuts.windows(2) {
        let ((a0, b0), (a1, b1)) = (pair[0], pair[1]);
        let stretch = match (a1 > a0, b1 > b0) {
            (true, true) => align_texts(&a[a0..a1], &b[b0..b1]),
            (false, false) => Vec::new(),
            _ => vec![BlockLink {
                a: 0..a1 - a0,
                b: 0..b1 - b0,
            }],
        };
        links.extend(stretch.into_iter().map(|link| BlockLink {
            a: link.a.start + a0..link.a.end + a0,
            b: link.b.start + b0..link.b.end + b0,
        }));
    }
    links
}

/// The cells row i may use: `width` either side of where B's position would be
/// if the two editions ran at a constant ratio.
#[derive(Debug)]
pub struct Band {
    a_len: usize,
    b_len: usize,
    width: usize,
}

impl Band {
    /// `width` is raised to the least that keeps every row's cells touching the
    /// next row's, which is what lets the end be reached.
    #[must_use]
    pub fn new(a_len: usize, b_len: usize, width: usize) -> Self {
        let least = b_len.div_ceil(a_len.max(1)) + 1;
        Self {
            a_len,
            b_len,
            width: width.max(least),
        }
    }

    /// B's position on the diagonal at A's position `i`.
    #[must_use]
    pub fn centre(&self, i: usize) -> usize {
        if self.a_len == 0 {
            return self.b_len;
        }
        i * self.b_len / self.a_len
    }

    #[must_use]
    pub fn lo(&self, i: usize) -> usize {
        self.centre(i).saturating_sub(self.width)
    }

    #[must_use]
    pub fn hi(&self, i: usize) -> usize {
        (self.centre(i) + self.width).min(self.b_len)
    }

    #[must_use]
    pub fn holds(&self, i: usize, j: usize) -> bool {
        (self.lo(i)..=self.hi(i)).contains(&j)
    }
}

/// Gale and Church's cost of a merge: their prior for a 2:1 link against a 1:1,
/// `-ln(0.089 / 0.89)`, which is `ln 10`, charged per block beyond the first on
/// each side. Their paper has no prior for 1:3 or 1:4, so extending it per block
/// is a guess.
pub const GALE_CHURCH_MERGE: f64 = std::f64::consts::LN_10;

/// Gale and Church's variance of B's length per unit of A's, measured on
/// their English–French–German corpus. Used as it is: a variance refitted per
/// pair from a first pass's links scored worse (`docs/notes.md`, "Books: Harry
/// Potter 1, English against Korean, aligned by length").
pub const GALE_CHURCH_VARIANCE: f64 = 6.8;

/// Two groups match when their character lengths fit the pair's ratio.
///
/// The cost is Gale and Church's: `-ln P(|δ| or more)`, where δ is how many
/// standard deviations B's length sits from A's length times the ratio, plus
/// [`GALE_CHURCH_MERGE`] for each block a group holds beyond one.
#[derive(Debug, Clone)]
pub struct LengthScorer {
    a: Vec<f64>,
    b: Vec<f64>,
    ratio: f64,
    variance: f64,
}

impl LengthScorer {
    /// The ratio is fitted from the pair's own totals, since a translation into
    /// Korean runs far shorter than its English and two English translations
    /// run about even.
    #[must_use]
    pub fn new(a_lengths: &[u32], b_lengths: &[u32], variance: f64) -> Self {
        let a = prefix_sums(a_lengths);
        let b = prefix_sums(b_lengths);
        let (total_a, total_b) = (a[a.len() - 1], b[b.len() - 1]);
        let ratio = if total_a > 0.0 {
            total_b / total_a
        } else {
            1.0
        };
        Self {
            a,
            b,
            ratio,
            variance,
        }
    }

    #[must_use]
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    fn length_a(&self, range: &Range<usize>) -> f64 {
        self.a[range.end] - self.a[range.start]
    }

    fn length_b(&self, range: &Range<usize>) -> f64 {
        self.b[range.end] - self.b[range.start]
    }
}

impl BlockScorer for LengthScorer {
    fn cost(&self, a: Range<usize>, b: Range<usize>) -> f64 {
        let (la, lb) = (self.length_a(&a), self.length_b(&b));
        let delta = (lb - la * self.ratio) / (la.max(1.0) * self.variance).sqrt();
        let extra = (a.len() + b.len()).saturating_sub(2);
        -ln_erfc(delta.abs() / std::f64::consts::SQRT_2) + GALE_CHURCH_MERGE * count(extra)
    }
}

/// Features that cost no model and survive translation: quoted speech, `?`,
/// `!`, ellipses, and numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Anchors {
    pub quotes: u32,
    pub questions: u32,
    pub exclamations: u32,
    pub ellipses: u32,
    pub numbers: u32,
}

impl Anchors {
    /// Opening curly quotation marks are counted, so a closing mark and an
    /// apostrophe (both `’`) are not; a straight `"` opens and closes alike, so
    /// two of them are one quotation, as one curly pair is. An ellipsis is one
    /// however it is typeset: `…`, `……`, `...` or `. . .`.
    ///
    /// A number is a run of digits, or an English number word set in capitals:
    /// a heading's `CHAPTER THIRTEEN` is the `13장` of its translation, which is
    /// what keeps a short heading with its chapter. A number word in running
    /// text is left out, since a translation as often spells it as a word.
    #[must_use]
    pub fn of(text: &str) -> Self {
        let mut anchors = Self::default();
        let mut straight: u32 = 0;
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '“' | '‘' => anchors.quotes += 1,
                '"' => straight += 1,
                '?' => anchors.questions += 1,
                '!' => anchors.exclamations += 1,
                '…' | '.' => {
                    let end = ellipsis_end(&chars, i);
                    if end > i {
                        anchors.ellipses += 1;
                        i = end;
                        continue;
                    }
                }
                c if c.is_ascii_digit() => {
                    anchors.numbers += 1;
                    while i + 1 < chars.len()
                        && (chars[i + 1].is_ascii_digit() || chars[i + 1] == ',')
                    {
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        anchors.numbers += text
            .split(|c: char| !(c.is_ascii_alphabetic() || c == '-'))
            .filter(|word| !word.chars().any(|c| c.is_ascii_lowercase()))
            .filter(|word| english_number(word).is_some())
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        // An unpaired one is taken for a quotation opened, though it may close
        // one an earlier block opened, where a curly `”` counts nothing (O326).
        anchors.quotes += straight.div_ceil(2);
        anchors
    }

    fn distance(self, other: Self) -> u32 {
        self.quotes.abs_diff(other.quotes)
            + self.questions.abs_diff(other.questions)
            + self.exclamations.abs_diff(other.exclamations)
            + self.ellipses.abs_diff(other.ellipses)
            + self.numbers.abs_diff(other.numbers)
    }

    fn add(self, other: Self) -> Self {
        Self {
            quotes: self.quotes + other.quotes,
            questions: self.questions + other.questions,
            exclamations: self.exclamations + other.exclamations,
            ellipses: self.ellipses + other.ellipses,
            numbers: self.numbers + other.numbers,
        }
    }
}

/// The value of an English number word from `zero` to `ninety-nine`, in any
/// case: `thirteen`, `THIRTY-SEVEN`. Tens come before ones, so `seven-thirty`
/// is not one.
#[must_use]
pub fn english_number(word: &str) -> Option<u32> {
    const ONES: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 8] = [
        "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    let index = |list: &[&str], w: &str| {
        list.iter()
            .position(|x| x.eq_ignore_ascii_case(w))
            .and_then(|n| u32::try_from(n).ok())
    };
    let tens = |w: &str| index(&TENS, w).map(|n| 20 + 10 * n);
    match word.split_once('-') {
        None => index(&ONES, word).or_else(|| tens(word)),
        Some((t, o)) => Some(tens(t)? + index(&ONES, o).filter(|n| (1..10).contains(n))?),
    }
}

/// Where an ellipsis starting at `i` ends, or `i` if none starts there.
fn ellipsis_end(chars: &[char], i: usize) -> usize {
    if chars[i] == '…' {
        let mut end = i;
        while end < chars.len() && chars[end] == '…' {
            end += 1;
        }
        return end;
    }
    let dots: String = chars[i..chars.len().min(i + 5)].iter().collect();
    if dots.starts_with(". . .") {
        i + 5
    } else if dots.starts_with("...") {
        i + 3
    } else {
        i
    }
}

/// [`LengthScorer`]'s cost plus `weight` for each anchor the two groups disagree on.
#[derive(Debug, Clone)]
pub struct AnchorScorer {
    length: LengthScorer,
    a: Vec<Anchors>,
    b: Vec<Anchors>,
    weight: f64,
}

impl AnchorScorer {
    #[must_use]
    pub fn new(length: LengthScorer, a: &[Anchors], b: &[Anchors], weight: f64) -> Self {
        Self {
            length,
            a: a.to_vec(),
            b: b.to_vec(),
            weight,
        }
    }
}

impl BlockScorer for AnchorScorer {
    fn cost(&self, a: Range<usize>, b: Range<usize>) -> f64 {
        let sum = |anchors: &[Anchors]| anchors.iter().fold(Anchors::default(), |s, x| s.add(*x));
        let disagree = sum(&self.a[a.clone()]).distance(sum(&self.b[b.clone()]));
        self.length.cost(a, b) + self.weight * f64::from(disagree)
    }
}

fn prefix_sums(lengths: &[u32]) -> Vec<f64> {
    let mut sums = Vec::with_capacity(lengths.len() + 1);
    let mut total = 0.0;
    sums.push(total);
    for length in lengths {
        total += f64::from(*length);
        sums.push(total);
    }
    sums
}

fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// `ln(erfc(x))` for `x >= 0`, computed in log space so a large `x` is a large
/// negative number rather than `ln(0)`. Numerical Recipes' Chebyshev fit, with a
/// fractional error under 1.2e-7 everywhere.
fn ln_erfc(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.5 * x);
    let poly = -1.265_512_23
        + t * (1.000_023_68
            + t * (0.374_091_96
                + t * (0.096_784_18
                    + t * (-0.186_288_06
                        + t * (0.278_868_07
                            + t * (-1.135_203_98
                                + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77))))))));
    t.ln() - x * x + poly
}

#[cfg(test)]
mod tests;
