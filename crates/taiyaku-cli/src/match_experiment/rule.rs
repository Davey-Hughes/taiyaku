//! The bar each rule sets on dev, and how held-out and unlabelled pairs are
//! read against it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::lookup::share;
use super::manifest::Label;

/// How far above the highest wrong pair a bar sits. A wrong pair the dev
/// series never showed can score a little higher than any it did.
pub(super) const GAP: f64 = 0.15;

/// The fewest slots a chapter or issue needs before it can match without a
/// person: a short side always finds somewhere to align.
pub(super) const MIN_CHAPTER_SLOTS: u32 = 8;

/// Which score a class of pairs is judged on: V for volumes, C for the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rule {
    Volume,
    Chapter,
    Pack,
}

/// One rule's result on dev.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Outcome {
    pub positives: usize,
    pub negatives: usize,
    pub max_negative: Option<f64>,
    /// `None` when no bar fits: the highest negative is within `GAP` of 1.0,
    /// or there is no negative to set one from.
    pub bar: Option<f64>,
    pub positives_clearing: usize,
}

/// The bar over `(is_positive, score)` pairs, and how many positives clear it.
///
/// With no negatives there is nothing to keep out, so no bar is claimed: a
/// rule that never met a wrong pair has not shown it can refuse one.
pub(super) fn dev_outcome(scored: &[(bool, f64)]) -> Outcome {
    let positives = scored.iter().filter(|(positive, _)| *positive).count();
    let max_negative = scored
        .iter()
        .filter(|(positive, _)| !positive)
        .map(|&(_, score)| score)
        .reduce(f64::max);
    let bar = max_negative
        .map(|highest| highest + GAP)
        .filter(|bar| *bar <= 1.0);
    let positives_clearing = bar.map_or(0, |bar| {
        scored
            .iter()
            .filter(|&&(positive, score)| positive && score >= bar)
            .count()
    });
    Outcome {
        positives,
        negatives: scored.len() - positives,
        max_negative,
        bar,
        positives_clearing,
    }
}

/// The indices of held-out negatives at or above the frozen bar: each one is
/// a wrong pair the rule would have applied.
pub(super) fn held_out_failures(scored: &[(bool, f64)], bar: f64) -> Vec<usize> {
    scored
        .iter()
        .enumerate()
        .filter(|(_, (positive, score))| !positive && *score >= bar)
        .map(|(i, _)| i)
        .collect()
}

/// One scored pair as a rule reads it: `class` is the pair's place in the
/// rule, `None` for an unlabelled pair inside the rule's gates, and `score`
/// is V or C as the rule judges. `a` is the oriented short side.
#[derive(Debug, Clone)]
pub(super) struct ScoredRow {
    pub a: String,
    pub b: String,
    pub label: Label,
    pub sub_class: String,
    pub class: Option<bool>,
    pub score: f64,
}

/// A labelled negative scoring at least this is shown to a person even
/// below the line: a wrong pair that high may be a right pair misnamed.
pub(super) const SHOWN_NEGATIVE: f64 = 0.5;

impl Rule {
    pub(super) fn name(self) -> &'static str {
        match self {
            Rule::Volume => "volume",
            Rule::Chapter => "chapter",
            Rule::Pack => "pack",
        }
    }

    pub(super) fn score_name(self) -> &'static str {
        match self {
            Rule::Volume => "V",
            Rule::Chapter | Rule::Pack => "C",
        }
    }
}

/// A label as one word or two, for a table cell.
pub(super) fn label_name(label: Label) -> String {
    match label {
        Label::SameVolume { copy: true } => "SameVolume copy".to_owned(),
        Label::SameVolume { copy: false } => "SameVolume".to_owned(),
        Label::OtherSeries { franchise: true } => "OtherSeries franchise".to_owned(),
        Label::OtherSeries { franchise: false } => "OtherSeries".to_owned(),
        other => format!("{other:?}"),
    }
}

/// One rule's section of `rules.txt`: its counts, the negative that sets the
/// bar and the pair behind it, the bar, and the positives clearing it, in
/// all and by sub-class. The first of two equal negatives is named, so the
/// pair named is the same on every run over the same rows.
pub(super) fn report(rule: Rule, rows: &[ScoredRow]) -> String {
    let labelled: Vec<(bool, f64)> = (rows.iter())
        .filter_map(|r| r.class.map(|c| (c, r.score)))
        .collect();
    let outcome = dev_outcome(&labelled);
    let unlabelled: Vec<&ScoredRow> = rows.iter().filter(|r| r.class.is_none()).collect();
    let mut out = format!(
        "## {} rule, on {}\npositives: {}, negatives: {}, unlabelled in its gates: {}\n",
        rule.name(),
        rule.score_name(),
        outcome.positives,
        outcome.negatives,
        unlabelled.len()
    );
    let highest = (rows.iter()).filter(|r| r.class == Some(false)).fold(
        None,
        |best: Option<&ScoredRow>, r| match best {
            Some(b) if b.score >= r.score => Some(b),
            _ => Some(r),
        },
    );
    let Some(highest) = highest else {
        out.push_str(
            "max negative: none\nbar: no negatives, no bar claimed\n\
             unlabelled at or above the max negative: none, no negative sets the line\n",
        );
        return out;
    };
    let _ = writeln!(
        out,
        "max negative: {:.4}, {} and {} ({})",
        highest.score,
        highest.a,
        highest.b,
        label_name(highest.label)
    );
    match outcome.bar {
        Some(bar) => {
            let _ = writeln!(out, "bar: {bar:.2} (exactly {bar})");
            let _ = writeln!(
                out,
                "positives at or above the bar: {} of {} ({})",
                outcome.positives_clearing,
                outcome.positives,
                share(outcome.positives_clearing, outcome.positives)
            );
            let mut by_sub: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
            for r in rows.iter().filter(|r| r.class == Some(true)) {
                let entry = by_sub.entry(r.sub_class.as_str()).or_default();
                entry.0 += usize::from(r.score >= bar);
                entry.1 += 1;
            }
            for (sub, (clearing, all)) in by_sub {
                let _ = writeln!(
                    out,
                    "  {sub}: {clearing} of {all} ({})",
                    share(clearing, all)
                );
            }
        }
        None => {
            let _ = writeln!(
                out,
                "bar: none fits, {:.4} + {GAP} is above 1.0",
                highest.score
            );
        }
    }
    let shown = (rows.iter())
        .filter(|r| r.class == Some(false) && r.score >= SHOWN_NEGATIVE)
        .count();
    let above = unlabelled
        .iter()
        .filter(|r| r.score >= highest.score)
        .count();
    let _ = write!(
        out,
        "labelled negatives at or above {SHOWN_NEGATIVE:.2}: {shown}\n\
         unlabelled at or above the max negative: {above}\n"
    );
    out
}

/// One feature read descriptively on a rule's labelled pairs: the highest
/// negative, and how many positives score strictly above it, which no rule
/// here judges on.
pub(super) fn feature_line(name: &str, values: &[(bool, f64)]) -> String {
    let highest = (values.iter())
        .filter(|(positive, _)| !positive)
        .map(|&(_, value)| value)
        .reduce(f64::max);
    let positives = values.iter().filter(|(positive, _)| *positive).count();
    match highest {
        None => format!("  {name}: no negative"),
        Some(highest) => {
            let above = (values.iter())
                .filter(|&&(positive, value)| positive && value > highest)
                .count();
            format!(
                "  {name}: highest negative {highest:.4}, positives above it {above} of \
                 {positives} ({})",
                share(above, positives)
            )
        }
    }
}
