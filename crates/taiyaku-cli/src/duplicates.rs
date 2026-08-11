//! Resolving several copies of one volume into a decision.
//!
//! The protocol is `docs/status.md` §2, worked out by hand across 109 volumes
//! and recorded in `docs/notes.md` under "Sorting 109 volumes' worth of
//! duplicates": signature, then all pairs *within* the set, then a ranking of
//! what is genuinely distinct.
//!
//! All pairs rather than every copy against the first, because anchoring on the
//! first copy reported One Piece volume 77 as three mutually unrelated files
//! when two of them are an exact identity — and the first copy is the one most
//! likely to be the odd one out, since the sort that produced it was by page
//! count.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use taiyaku_core::{Direction, PageHash, PageRange, Run, Verdict, paired_slots, solve, verdict};

use crate::signature::{Signature, content_digest};
use crate::stats::verdict_word;

/// A fingerprinting closure: one copy's members in, one hash per slot out.
///
/// Named so `resolve_with`'s signature reads as a type rather than as the
/// unnamed `dyn FnMut(...)` clippy flags as too complex to inline.
type Fingerprint<'a> = dyn FnMut(&[PathBuf]) -> Result<Vec<PageHash>, String> + 'a;

/// The `--`-separated copies, stopping at the first flag.
///
/// `topology` splits on `--` throughout because it has no flags. This command
/// does, and `--against` begins with `--`, so the groups have to end somewhere:
/// at the first argument that starts with `--` and is not exactly `--`.
///
/// Members within a copy come back in natural order rather than as typed, for
/// the reason `split_editions` gives — a shell glob yields `c10.5, c10, c1, c2`
/// and a copy is a page sequence.
pub fn groups(args: &[String]) -> Result<Vec<Vec<PathBuf>>, String> {
    let end = args
        .iter()
        .position(|a| a.starts_with("--") && a != "--")
        .unwrap_or(args.len());
    let mut copies: Vec<Vec<PathBuf>> = args[..end]
        .split(|a| a == "--")
        .filter(|g| !g.is_empty())
        .map(|g| g.iter().map(PathBuf::from).collect())
        .collect();
    if copies.len() < 2 {
        return Err(
            "needs at least two copies, separated by `--`; one copy is not a duplicate set"
                .to_string(),
        );
    }
    for copy in &mut copies {
        copy.sort_by(|a, b| taiyaku_read::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    }
    Ok(copies)
}

/// The members following `--against`, up to the next flag.
///
/// `--against` with nothing after it is an error rather than a silent no-op:
/// dropping the reference edition can change `decided=` and the rank order,
/// and a flag that was typed and then ignored is worse than one refused.
fn against_members(args: &[String]) -> Result<Option<Vec<PathBuf>>, String> {
    let Some(at) = args.iter().position(|a| a == "--against") else {
        return Ok(None);
    };
    let members: Vec<PathBuf> = args[at + 1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if members.is_empty() {
        return Err("--against needs a reference edition: one or more members".to_string());
    }
    Ok(Some(members))
}

/// Which copy each copy stands for: itself, or the first earlier copy it was
/// confirmed to hold the same bytes as.
///
/// The signature is checked first and the confirmation only where it matched,
/// so a set of genuinely different copies pays for a listing and nothing more.
/// `confirm` is a parameter rather than a direct call to `content_digest` so
/// that this grouping — the thing that decides how much decoding happens — can
/// be tested without fixtures.
///
/// A copy only ever stands for a copy that stands for itself, so the result is
/// one level deep and needs no chasing.
fn representatives(
    signatures: &[Signature],
    mut confirm: impl FnMut(usize, usize) -> Result<bool, String>,
) -> Result<Vec<usize>, String> {
    let mut stands_for: Vec<usize> = (0..signatures.len()).collect();
    for copy in 0..signatures.len() {
        for earlier in 0..copy {
            if stands_for[earlier] == earlier
                && signatures[earlier].digest == signatures[copy].digest
                && confirm(earlier, copy)?
            {
                stands_for[copy] = earlier;
                break;
            }
        }
    }
    Ok(stands_for)
}

/// A `confirm` closure for `representatives` that hashes each copy's content
/// at most once, however many candidates it is compared against.
///
/// `representatives` breaks on the first `earlier` that confirms, but where
/// nothing confirms — every earlier copy sharing a signature it does not
/// actually share bytes with — it tries every one of them, and the naive
/// closure `Ok(content_digest(&copies[a])? == content_digest(&copies[b])?)`
/// re-decoded both sides from scratch on every attempt. A copy compared as
/// `earlier` against every later copy paid for its own decode once per later
/// copy: O(n²) full content reads over a set whose signatures collide, where
/// O(n) — one hash per copy — answers the same question.
///
/// `digests` fills lazily, one slot per copy, rather than all at once up
/// front, so a copy `representatives` never calls `confirm` for at all — one
/// whose signature matches nothing — is still never hashed. That is the same
/// guarantee `copies_with_different_signatures_are_never_hashed` pins at the
/// `representatives` level; this only ever asks for a digest `representatives`
/// already decided to compare.
fn memoized_confirm(
    copies: usize,
    mut digest: impl FnMut(usize) -> Result<String, String>,
) -> impl FnMut(usize, usize) -> Result<bool, String> {
    let mut digests: Vec<Option<String>> = vec![None; copies];
    move |a, b| {
        for idx in [a, b] {
            if digests[idx].is_none() {
                digests[idx] = Some(digest(idx)?);
            }
        }
        Ok(digests[a] == digests[b])
    }
}

pub fn command(args: &[String]) -> Result<(), String> {
    crate::reject_unknown_flags("duplicates", args, &["--against", "--cache", "--ltr"])?;
    let copies = groups(args)?;
    let direction = crate::reading_direction(args);
    let cache = PathBuf::from(
        crate::option_value(args, "--cache", "a directory")?.unwrap_or(".taiyaku-cache"),
    );
    let against = against_members(args)?;

    let resolution = resolve(&copies, against.as_deref(), direction, &cache)?;

    eprint!("{}", tables(&resolution));
    println!("{}", set_line(&resolution));
    Ok(())
}

/// Run the protocol over one set, fingerprinting through the cache.
///
/// Spreads always split. One Piece v092 is 200 images in one copy and 101 in
/// the other, and c0's 200 slots come out wholly within c1's 202 — every one
/// of c1's images detects as a spread, so slot expansion doubles its count.
/// `notes.md` draws the conclusion directly: page counts are a property of
/// packaging, slot counts a property of the work. A duplicate command that
/// compared pages would get its own headline case wrong, so there is no flag
/// for this.
pub fn resolve(
    copies: &[Vec<PathBuf>],
    against: Option<&[PathBuf]>,
    direction: Direction,
    cache: &Path,
) -> Result<Resolution, String> {
    let mut fingerprint =
        |members: &[PathBuf]| Ok(crate::cached_edition(cache, members, direction)?.0);
    resolve_with(copies, against, &mut fingerprint)
}

/// The protocol over one set, fingerprinting through a caller-supplied
/// closure.
///
/// A copy's fingerprinted slot count, as `PageRange::new` takes it.
///
/// # Panics
///
/// Panics if `n` exceeds `u32::MAX`. It cannot: `n` is one copy's
/// fingerprinted slot count, and real volumes never come anywhere near
/// `u32::MAX` slots — a corpus fact, not usize's.
fn slot_count(n: usize) -> u32 {
    u32::try_from(n).expect("slot count exceeds u32::MAX")
}

/// Separated from `resolve` so the branch that matters most here can be
/// tested: a copy confirmed byte-identical to an earlier one is never
/// fingerprinted, and a decode of every page is what that saves. A test
/// counting calls to this closure is the only way to hold that guarantee,
/// since the saving is invisible in the result.
pub fn resolve_with(
    copies: &[Vec<PathBuf>],
    against: Option<&[PathBuf]>,
    fingerprint: &mut Fingerprint<'_>,
) -> Result<Resolution, String> {
    // Step 1. Cheap, and it decides what the rest has to do.
    let signatures: Vec<Signature> = copies
        .iter()
        .map(|copy| Signature::of(copy))
        .collect::<Result<_, _>>()?;

    // Which copy each copy stands for. Resolved before anything is
    // fingerprinted, because that is where the saving is: skipping a solve
    // saves microseconds and skipping a decode of every page saves seconds.
    let stands_for = representatives(
        &signatures,
        memoized_confirm(copies.len(), |idx| content_digest(&copies[idx])),
    )?;
    let settled = (0..copies.len()).filter(|&c| stands_for[c] != c).count();

    // Fingerprint each representative once. A copy standing for another holds
    // the same page bytes, so it has the same dimensions, the same spreads and
    // the same slot count — there is nothing left for a decode to find.
    let mut hashes: Vec<Vec<PageHash>> = Vec::new();
    let mut described = Vec::new();
    for (copy, members) in copies.iter().enumerate() {
        let slot_hashes = if stands_for[copy] == copy {
            fingerprint(members)?
        } else {
            hashes[stands_for[copy]].clone()
        };
        described.push(Copy {
            label: members
                .first()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            slots: slot_hashes.len(),
        });
        hashes.push(slot_hashes);
    }

    // Step 2. Every unordered pair, one solve each — leaning on the symmetry
    // `docs/status.md` records as measured across five pairs in two series.
    // Two copies confirmed byte-identical are not solved: the solver would be
    // comparing a page sequence with itself, and the run describing that is the
    // whole of it.
    let mut pairs = Vec::new();
    for a in 0..copies.len() {
        for b in a + 1..copies.len() {
            let slots = described[a].slots;
            // The `slots > 0` guard above, on the untruncated usize, rules out `- 1`
            // underflowing.
            let runs = if stands_for[a] == stands_for[b] && slots > 0 {
                vec![Run::paired(
                    PageRange::new(0, slot_count(slots) - 1),
                    PageRange::new(0, slot_count(slots) - 1),
                )]
            } else {
                solve(&hashes[a], &hashes[b])
            };
            pairs.push(Pair::judge(
                a,
                b,
                &runs,
                described[a].slots,
                described[b].slots,
            ));
        }
    }

    let clusters = cluster(copies.len(), &pairs);
    let contradicting = contradictions(&clusters, &pairs);
    let within_set = flagged(copies.len(), &pairs);

    // Step 3. One alignment per representative, not per copy: an identity
    // cluster's members are the same content by measurement, so aligning both
    // against the reference would spend a solve to get the same answer twice.
    // The representative is whichever member came first in the argument list —
    // members of one cluster have equal slot counts by the definition of
    // identity, so the choice only has to be stable and explainable.
    //
    // **Every representative is scored, including ones the within-set rule
    // flagged, because where there is a reference edition it is the reference
    // that decides.** A copy pairing nothing with it is not this volume
    // whatever it did within the set; a copy that does pair with it is this
    // volume even where it refused every other copy — which is precisely the
    // mislabelled-file case, the impostor refusing the real scans and the real
    // scans refusing it right back, neither refusal saying which is which.
    // Skipping the flagged clusters before scoring them is what made `--against`
    // print byte-identical output on the one set it exists to settle.
    let reference = match against {
        Some(members) => Some(fingerprint(members)?),
        None => None,
    };
    let mut ranked = Vec::new();
    let mut excluded = Vec::new();
    for group in &clusters {
        let first = group[0];
        let scored = reference.as_ref().map(|reference| {
            let runs = solve(&hashes[first], reference);
            Against {
                runs: runs.len(),
                paired: paired_slots(&runs),
            }
        });
        let this_volume = match scored {
            Some(against) => against.paired > 0,
            None => !within_set.contains(&first),
        };
        if !this_volume {
            // The whole cluster, not just its representative: an identity
            // cluster is one content by measurement, so what the reference
            // says about one member it says about all of them.
            excluded.extend(group.iter().copied());
            continue;
        }
        ranked.push(Ranked {
            cluster: group.clone(),
            slots: described[first].slots,
            against: scored,
        });
    }
    excluded.sort_unstable();

    Ok(Resolution {
        copies: described,
        signature_settled: settled,
        pairs,
        clusters,
        contradictions: contradicting,
        // What was reported as flagged has to be what was actually kept out of
        // the ranking, or the "not this volume" section and the `flagged=`
        // field describe a rule the ranking did not use.
        flagged: if reference.is_some() {
            excluded
        } else {
            within_set
        },
        reference_given: reference.is_some(),
        ranked: rank(ranked),
    })
}

/// How two copies stand to one another, decided from one solve.
///
/// Containment is the commonest near-identity shape in the corpus: three of
/// its four identity-looking pairs are containment rather than identity, two
/// of those three `Within` and Bocchi's hamaji/krc pair `Contains`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Every slot paired on both sides: the same edition, however it is packed.
    Identity,
    /// Every slot of the `a` side is paired and the `b` side carries more, so
    /// `a` adds nothing `b` lacks. This is the base-selection statement.
    Within,
    /// The reverse: every slot of `b` is paired and `a` carries more.
    Contains,
    /// Neither side is wholly covered. Genuinely different editions.
    Distinct,
    /// Nothing paired. Not this volume at all.
    Refused,
}

/// One solved pair, kept for the table and for the clustering.
#[derive(Debug, Clone)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub a_slots: usize,
    pub b_slots: usize,
    pub runs: usize,
    pub paired: u32,
    pub verdict: Verdict,
    pub relation: Relation,
}

impl Pair {
    /// Judge one solve.
    ///
    /// Identity is stated as coverage rather than as `runs == 1` because
    /// coverage is the property that matters and the single run is a
    /// consequence: paired runs are 1:1, so full coverage on both sides forces
    /// a constant offset and an offset that never changes is one run. If a
    /// solve ever reported full mutual coverage across several runs, that would
    /// mean run compression is broken — worth finding out rather than papering
    /// over, which is why the corpus check asserts the run count separately.
    pub fn judge(a: usize, b: usize, runs: &[Run], a_slots: usize, b_slots: usize) -> Self {
        let paired = paired_slots(runs);
        let call = verdict(runs, a_slots);
        let relation = match call {
            Verdict::Refused => Relation::Refused,
            _ => match (paired as usize == a_slots, paired as usize == b_slots) {
                (true, true) => Relation::Identity,
                (true, false) => Relation::Within,
                (false, true) => Relation::Contains,
                (false, false) => Relation::Distinct,
            },
        };
        Self {
            a,
            b,
            a_slots,
            b_slots,
            runs: runs.len(),
            paired,
            verdict: call,
            relation,
        }
    }

    fn touches(&self, copy: usize) -> bool {
        self.a == copy || self.b == copy
    }
}

/// Collapse copies that are the same edition, by union-find over the identity
/// pairs.
///
/// Clusters come back with their members ascending, ordered by their lowest
/// member, so the output is stable whatever order the pairs arrived in.
pub fn cluster(copies: usize, pairs: &[Pair]) -> Vec<Vec<usize>> {
    fn root(parent: &mut [usize], mut of: usize) -> usize {
        while parent[of] != of {
            parent[of] = parent[parent[of]];
            of = parent[of];
        }
        of
    }
    let mut parent: Vec<usize> = (0..copies).collect();
    for pair in pairs.iter().filter(|p| p.relation == Relation::Identity) {
        let (a, b) = (root(&mut parent, pair.a), root(&mut parent, pair.b));
        if a != b {
            parent[a.max(b)] = a.min(b);
        }
    }

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for copy in 0..copies {
        let of = root(&mut parent, copy);
        match groups.iter_mut().find(|g| root(&mut parent, g[0]) == of) {
            Some(group) => group.push(copy),
            None => groups.push(vec![copy]),
        }
    }
    groups
}

/// Pairs the clustering merged that the solve did not call identity.
///
/// `A ≡ B` and `B ≡ C` with `A ≢ C` is a contradiction, and every pair is
/// solved anyway so it is already in hand. Reported rather than resolved: the
/// merge is still the best available answer, and the contradiction means one of
/// the three alignments is wrong — a thing to go and look at, not a reason to
/// refuse to report.
pub fn contradictions(clusters: &[Vec<usize>], pairs: &[Pair]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for group in clusters.iter().filter(|g| g.len() > 1) {
        for (at, &a) in group.iter().enumerate() {
            for &b in &group[at + 1..] {
                let judged = pairs
                    .iter()
                    .find(|p| (p.a == a && p.b == b) || (p.a == b && p.b == a));
                if judged.is_some_and(|p| p.relation != Relation::Identity) {
                    found.push((a, b));
                }
            }
        }
    }
    found
}

/// Copies that refused against every other copy: not this volume, as far as the
/// set alone can say.
///
/// With two copies and a refusal between them both are flagged, because nothing
/// in the set says which one is the impostor — the refusal is symmetric and so
/// is the answer.
///
/// **This is the whole rule only where no reference edition was given.** With
/// one, `resolve_with` sets it aside and asks the reference instead, so
/// `Resolution.flagged` is not always this function's result: it is whichever
/// rule actually decided the ranking.
pub fn flagged(copies: usize, pairs: &[Pair]) -> Vec<usize> {
    (0..copies)
        .filter(|&copy| {
            let mut own = pairs.iter().filter(|p| p.touches(copy)).peekable();
            own.peek().is_some() && own.all(|p| p.relation == Relation::Refused)
        })
        .collect()
}

/// How a cluster's representative aligned against the reference edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Against {
    pub runs: usize,
    pub paired: u32,
}

/// One row of the ranking: a cluster, its content, and how cleanly it aligns.
#[derive(Debug, Clone)]
pub struct Ranked {
    pub cluster: Vec<usize>,
    pub slots: usize,
    pub against: Option<Against>,
}

/// Rank clusters: content first, cleanliness only as a tiebreak.
///
/// `notes.md`, on the Chainsaw Man result: "Pages and pairings measure content;
/// runs measure how cleanly the correspondence holds. Content should win,
/// because a missing page cannot be recovered from a cleaner alignment of the
/// pages that remain."
pub fn rank(mut entries: Vec<Ranked>) -> Vec<Ranked> {
    entries.sort_by(|x, y| {
        y.slots
            .cmp(&x.slots)
            .then_with(|| match (x.against, y.against) {
                (Some(a), Some(b)) => b.paired.cmp(&a.paired).then(a.runs.cmp(&b.runs)),
                _ => std::cmp::Ordering::Equal,
            })
    });
    entries
}

/// Whether the ranking has a unique winner.
///
/// A tie at the top is reported as a tie rather than broken by a statistic that
/// decides nothing — within-set cleanliness is symmetric and useless with two
/// clusters, and dressing it up as a decision would be worse than saying so.
pub fn decided(ranked: &[Ranked]) -> bool {
    match ranked {
        [] => false,
        [_] => true,
        [first, second, ..] => {
            first.slots != second.slots
                || match (first.against, second.against) {
                    (Some(a), Some(b)) => (a.paired, b.runs) != (b.paired, a.runs),
                    _ => false,
                }
        }
    }
}

/// One copy, as far as the output is concerned.
#[derive(Debug, Clone)]
pub struct Copy {
    /// The path as the caller gave it, for a person to act on.
    pub label: String,
    pub slots: usize,
}

/// Everything one set resolved to.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub copies: Vec<Copy>,
    /// Copies removed by the signature step, before any alignment.
    pub signature_settled: usize,
    pub pairs: Vec<Pair>,
    pub clusters: Vec<Vec<usize>>,
    pub contradictions: Vec<(usize, usize)>,
    /// Copies kept out of the ranking: not this volume.
    ///
    /// Which rule said so depends on `reference_given`. Without a reference it
    /// is `flagged()`'s within-set answer — refused against every other copy.
    /// With one it is every copy of a cluster whose representative paired
    /// nothing with the reference, which can flag a copy the set agreed with
    /// and spare one the set refused.
    pub flagged: Vec<usize>,
    /// Whether a reference edition was given.
    ///
    /// The output has to know, because the hint under an all-flagged set asks
    /// the person to pass `--against`. Where they already did, the question was
    /// answered and printing it again would be the command failing to notice
    /// its own reply.
    pub reference_given: bool,
    pub ranked: Vec<Ranked>,
}

/// The one line a survey script collects.
///
/// Fixed field order and no wrapping, the discipline `stats::calibration_line`
/// already establishes and `scripts/margin-survey.sh` already depends on.
/// `best` is **last** so that a path containing a space cannot break field
/// splitting for anything after it; every other field is a number or a word.
pub fn set_line(resolution: &Resolution) -> String {
    let count = |wanted: Verdict| {
        resolution
            .pairs
            .iter()
            .filter(|p| p.verdict == wanted)
            .count()
    };
    let (slots, best) = match resolution.ranked.first() {
        Some(first) => (
            first.slots.to_string(),
            resolution.copies[first.cluster[0]].label.clone(),
        ),
        None => ("-".to_string(), "-".to_string()),
    };
    format!(
        "set: copies={} signature={} clusters={} flagged={} refused={} fragmented={} \
         slots={slots} decided={} best={best}",
        resolution.copies.len(),
        resolution.signature_settled,
        resolution.clusters.len(),
        resolution.flagged.len(),
        count(Verdict::Refused),
        count(Verdict::Fragmented),
        if decided(&resolution.ranked) {
            "yes"
        } else {
            "no"
        }
    )
}

/// The readable form: what was compared, what collapsed, and what is left.
pub fn tables(resolution: &Resolution) -> String {
    let mut out = String::from("copies\n");
    for (index, copy) in resolution.copies.iter().enumerate() {
        let _ = writeln!(out, "  {index}  {} slots  {}", copy.slots, copy.label);
    }

    if !resolution.pairs.is_empty() {
        out.push_str("\npairs\n");
        for pair in &resolution.pairs {
            // Containment names the contained copy first and then the copy it
            // sits within — but "within" alone is a label, not a figure. The
            // container's own slot count says how much more it holds, and
            // goes on the line only where it is new information: in `Within`
            // the container is `b`, whose count is not printed elsewhere on
            // this line. In `Contains` the container is `a`, whose count is
            // already the denominator in the `{paired} of {a_slots}` column
            // just printed, so repeating it in the suffix would say the same
            // number twice.
            let suffix = match pair.relation {
                Relation::Identity => "  identity".to_string(),
                Relation::Within => {
                    format!("  {} within {} ({} slots)", pair.a, pair.b, pair.b_slots)
                }
                Relation::Contains => format!("  {} within {}", pair.b, pair.a),
                Relation::Distinct | Relation::Refused => String::new(),
            };
            let _ = writeln!(
                out,
                "  {}-{}  {:<9} {} of {}  {}{}",
                pair.a,
                pair.b,
                plural(pair.runs),
                pair.paired,
                pair.a_slots,
                verdict_word(pair.verdict),
                suffix
            );
        }
    }

    out.push_str("\nclusters\n");
    for group in &resolution.clusters {
        let members: Vec<String> = group.iter().map(usize::to_string).collect();
        let _ = writeln!(
            out,
            "  {{{}}}{}",
            members.join(", "),
            if group.len() > 1 { "  identity" } else { "" }
        );
    }

    for (a, b) in &resolution.contradictions {
        let _ = writeln!(
            out,
            "\ncopies {a} and {b} were merged but did not align as identity; \
             one of the three solves in that cluster is wrong"
        );
    }

    if !resolution.flagged.is_empty() {
        out.push_str("\nnot this volume\n");
        // The reason has to name the rule that applied, because with a
        // reference the two can disagree: a copy the set agreed with can pair
        // nothing with the reference, and "refused against every other copy"
        // would then be a sentence the pairs table above contradicts.
        let why = if resolution.reference_given {
            "paired nothing with the reference edition"
        } else {
            "refused against every other copy"
        };
        for &copy in &resolution.flagged {
            let _ = writeln!(out, "  {copy}  {why}  {}", resolution.copies[copy].label);
        }
        if !resolution.reference_given && resolution.flagged.len() == resolution.copies.len() {
            out.push_str(
                "  nothing in this set says which of these is the impostor; \
                 pass --against a reference edition\n",
            );
        }
    }

    if !resolution.ranked.is_empty() {
        out.push_str("\nrank\n");
        for (place, entry) in resolution.ranked.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {}.  {} slots  copy {}{}",
                place + 1,
                entry.slots,
                entry.cluster[0],
                match entry.against {
                    Some(a) => format!("  {} paired  {}", a.paired, plural(a.runs)),
                    None => String::new(),
                }
            );
        }
    }

    out
}

fn plural(runs: usize) -> String {
    match runs {
        1 => "1 run".to_string(),
        n => format!("{n} runs"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taiyaku_core::PageRange;

    /// Two copies of one scan: every slot paired on both sides.
    fn whole(slots: u32) -> Vec<Run> {
        vec![Run::paired(
            PageRange::new(0, slots - 1),
            PageRange::new(0, slots - 1),
        )]
    }

    /// Identity is mutual full coverage. It is necessarily one run, because
    /// paired runs are 1:1 and an offset that never changes cannot start a
    /// second run — so the test states coverage and asserts the run count as a
    /// consequence rather than as the definition.
    #[test]
    fn full_coverage_on_both_sides_is_identity() {
        let pair = Pair::judge(0, 1, &whole(234), 234, 234);

        assert_eq!(pair.relation, Relation::Identity);
        assert_eq!(pair.runs, 1);
        assert_eq!(pair.paired, 234);
    }

    /// The distinction base selection turns on. Chainsaw Man's `v01` against
    /// `v01b` covers all 198 of A and leaves a 199th page on B that A never
    /// reaches — so `a` is wholly within `b`, adding nothing `b` lacks, and
    /// `v01b` goes on to win the rank on that extra page. Collapsing them
    /// would discard the difference the choice is about.
    #[test]
    fn one_slot_left_over_on_the_far_side_is_not_identity() {
        let pair = Pair::judge(0, 1, &whole(198), 198, 199);

        assert_eq!(pair.relation, Relation::Within);
        assert_eq!(pair.paired, 198);
    }

    /// The mirror shape: Bocchi's hamaji copy against krc, 125 slots against
    /// 124 with the 124 shared ones all paired. Here it is the `a` side that
    /// carries the extra slot, so `b` is within `a`.
    #[test]
    fn extra_slot_on_the_near_side_is_containment_the_other_way() {
        let pair = Pair::judge(0, 1, &whole(124), 125, 124);

        assert_eq!(pair.relation, Relation::Contains);
        assert_eq!(pair.paired, 124);
    }

    /// One Piece v077's c0/c1 shape: 184 of 236 paired, neither side wholly
    /// covered. The new containment variants must not swallow the case they
    /// were carved out of distinguishing from.
    #[test]
    fn neither_side_wholly_covered_is_still_distinct() {
        let pair = Pair::judge(0, 1, &spanning(100, 184), 236, 234);

        assert_eq!(pair.relation, Relation::Distinct);
    }

    /// A refusal is a claim about the volume, not about the alignment.
    #[test]
    fn nothing_paired_is_a_refusal() {
        let runs = vec![Run::gap(PageRange::new(0, 201))];

        let pair = Pair::judge(0, 1, &runs, 202, 198);

        assert_eq!(pair.relation, Relation::Refused);
        assert_eq!(pair.verdict, Verdict::Refused);
    }

    /// One Piece volume 77, which is the case that argues for all-pairs: c1 and
    /// c2 are the same edition at different paginations and c0 is a different
    /// crop. Anchoring on c0 reports three unrelated files.
    #[test]
    fn identity_pairs_collapse_and_the_outlier_stays_alone() {
        let pairs = vec![
            Pair::judge(0, 1, &fragmented(), 236, 234),
            Pair::judge(0, 2, &fragmented(), 236, 234),
            Pair::judge(1, 2, &whole(234), 234, 234),
        ];

        let clusters = cluster(3, &pairs);

        assert_eq!(clusters, vec![vec![0], vec![1, 2]]);
        assert!(contradictions(&clusters, &pairs).is_empty());
    }

    /// Identity is transitive or one of the three solves is wrong. Every pair
    /// is solved anyway, so the contradiction is already in hand: report it and
    /// keep the merge, which is still the best available answer.
    #[test]
    fn identity_that_is_not_transitive_is_reported_and_still_merged() {
        let pairs = vec![
            Pair::judge(0, 1, &whole(200), 200, 200),
            Pair::judge(1, 2, &whole(200), 200, 200),
            Pair::judge(0, 2, &fragmented(), 200, 200),
        ];

        let clusters = cluster(3, &pairs);

        assert_eq!(clusters, vec![vec![0, 1, 2]]);
        assert_eq!(contradictions(&clusters, &pairs), vec![(0, 2)]);
    }

    /// A cluster's members are examined in ascending order, but the pair list
    /// is not required to hold them that way round. A pair stored the other
    /// way is the same pair.
    #[test]
    fn a_contradiction_is_found_however_the_pair_is_stored() {
        let pairs = vec![
            Pair::judge(0, 1, &whole(200), 200, 200),
            Pair::judge(1, 2, &whole(200), 200, 200),
            Pair::judge(2, 0, &fragmented(), 200, 200),
        ];

        let clusters = cluster(3, &pairs);

        assert_eq!(clusters, vec![vec![0, 1, 2]]);
        assert_eq!(contradictions(&clusters, &pairs), vec![(0, 2)]);
    }

    /// The Bocchi Gaiden file and the One Piece Starter Book: named for a
    /// volume, refused by every copy of it.
    #[test]
    fn a_copy_that_refuses_against_everything_is_flagged() {
        let pairs = vec![
            Pair::judge(0, 1, &whole(124), 124, 124),
            Pair::judge(0, 2, &refusal(124), 124, 185),
            Pair::judge(1, 2, &refusal(124), 124, 185),
        ];

        assert_eq!(flagged(3, &pairs), vec![2]);
    }

    /// With two copies and a refusal between them, nothing in the set says
    /// which one is the impostor, so both are flagged and nothing is ranked.
    /// `--against` is what resolves it.
    #[test]
    fn two_copies_that_refuse_each_other_flag_both() {
        let pairs = vec![Pair::judge(0, 1, &refusal(202), 202, 198)];

        assert_eq!(flagged(2, &pairs), vec![0, 1]);
    }

    /// A copy nothing was compared against has not refused anything, and must
    /// not be read as though it had. An empty `all()` is vacuously true, so
    /// without the guard every uncompared copy comes back an impostor.
    #[test]
    fn a_copy_with_no_pairs_at_all_is_not_flagged() {
        let pairs = vec![Pair::judge(0, 1, &whole(124), 124, 124)];

        assert_eq!(flagged(3, &pairs), Vec::<usize>::new());
    }

    /// Content first. `notes.md`: "a missing page cannot be recovered from a
    /// cleaner alignment of the pages that remain".
    #[test]
    fn ranking_puts_the_copy_with_more_slots_first() {
        let ranked = rank(vec![
            Ranked {
                cluster: vec![1],
                slots: 234,
                against: None,
            },
            Ranked {
                cluster: vec![0],
                slots: 236,
                against: None,
            },
        ]);

        assert_eq!(ranked[0].cluster, vec![0]);
        assert!(decided(&ranked));
    }

    /// Without a reference there is no honest tiebreak, so a tie is reported as
    /// a tie rather than broken by a statistic that decides nothing.
    #[test]
    fn a_tie_on_slots_with_no_reference_is_undecided() {
        let ranked = rank(vec![
            Ranked {
                cluster: vec![0],
                slots: 198,
                against: None,
            },
            Ranked {
                cluster: vec![1],
                slots: 198,
                against: None,
            },
        ]);

        assert!(!decided(&ranked));
    }

    /// The Chainsaw Man table: four scans against one English release, ranked
    /// on slots then pairings then runs. `v01b` wins on every axis at once.
    ///
    /// **Cluster 2 is listed before cluster 0 on purpose.** The two are tied on
    /// slots and on pairings and separate only on runs, so they are the pair
    /// the reference tiebreak exists for — and `sort_by` is stable, which means
    /// a tiebreak that did nothing at all would still leave them in whatever
    /// order they arrived in. Given in the order the answer is not, so the
    /// assertion below can only pass if something actually reordered them.
    #[test]
    fn a_reference_edition_breaks_a_tie_on_pairings_then_runs() {
        let ranked = rank(vec![
            Ranked {
                cluster: vec![2],
                slots: 198,
                against: Some(Against {
                    runs: 7,
                    paired: 182,
                }),
            },
            Ranked {
                cluster: vec![1],
                slots: 199,
                against: Some(Against {
                    runs: 3,
                    paired: 183,
                }),
            },
            Ranked {
                cluster: vec![0],
                slots: 198,
                against: Some(Against {
                    runs: 5,
                    paired: 182,
                }),
            },
        ]);

        let order: Vec<usize> = ranked.iter().map(|r| r.cluster[0]).collect();
        assert_eq!(
            order,
            vec![1, 0, 2],
            "199 slots first, then 5 runs before 7"
        );
        assert!(decided(&ranked));
    }

    /// A tie on slots that the reference breaks is decided; one it does not is
    /// not.
    ///
    /// Every other test of `decided` either has a winner on slots or has no
    /// reference at all, so the arm that reads the reference never runs in any
    /// of them — and without it `decided` answers from the slot count alone and
    /// calls every reference-broken tie a tie.
    #[test]
    fn a_tie_on_slots_is_decided_by_the_reference_and_only_by_a_real_difference() {
        let tied = |paired, runs| Ranked {
            cluster: vec![0],
            slots: 198,
            against: Some(Against { runs, paired }),
        };

        assert!(
            decided(&[tied(183, 3), tied(182, 3)]),
            "more pages paired against the reference"
        );
        assert!(
            decided(&[tied(182, 3), tied(182, 7)]),
            "the same pages, in fewer runs"
        );
        assert!(
            !decided(&[tied(182, 3), tied(182, 3)]),
            "the same on both axes is a tie, and is reported as one"
        );
    }

    /// Two byte-identical copies whose fingerprint comes back with no slots.
    ///
    /// The identity short-circuit skips the solve and states the answer
    /// directly, as a run spanning `0..slots - 1` — one subtraction away from
    /// underflowing when there are no slots, and `slots > 0` is the guard
    /// against it.
    ///
    /// Reached through the closure `resolve_with` exists to take rather than
    /// through empty archives, which cannot get this far: `Signature::of`
    /// refuses a member with no page images in step 1, several steps before
    /// anything is fingerprinted. So this is a statement about the seam, not
    /// about a set of files — which is exactly what the guard is protecting,
    /// since through `resolve` the count can never be zero.
    #[test]
    fn an_identity_pair_with_no_slots_resolves_rather_than_underflowing() {
        let root = std::env::temp_dir().join("taiyaku-test-duplicates-noslots");
        let _ = std::fs::remove_dir_all(&root);
        let copies: Vec<Vec<PathBuf>> = ["a", "b"]
            .iter()
            .map(|name| {
                let dir = root.join(name);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("000.jpg"), b"identical bytes").unwrap();
                vec![dir]
            })
            .collect();

        let mut fingerprint = |_: &[PathBuf]| Ok(Vec::new());
        let resolution = resolve_with(&copies, None, &mut fingerprint);

        let _ = std::fs::remove_dir_all(&root);
        let resolution = resolution.expect("a slotless identity pair resolves");
        assert!(
            resolution.copies.iter().all(|copy| copy.slots == 0),
            "the closure's answer is the slot count, whatever the files hold"
        );
        // The guard sends the pair to the solver rather than declaring the
        // identity, and a solve over two empty sequences pairs nothing — so the
        // two are reported as refusing each other. Stated because it is the
        // consequence of not underflowing, not because it is a good answer.
        assert_eq!(resolution.clusters, vec![vec![0], vec![1]]);
        assert_eq!(resolution.flagged, vec![0, 1]);
    }

    /// A hundred short runs over most of the pages: the shape a needle and a
    /// differently-cropped scan both produce.
    fn fragmented() -> Vec<Run> {
        (0..100)
            .map(|i| Run::paired(PageRange::new(i * 2, i * 2), PageRange::new(i * 2, i * 2)))
            .collect()
    }

    fn refusal(a_slots: u32) -> Vec<Run> {
        vec![Run::gap(PageRange::new(0, a_slots - 1))]
    }

    /// One Piece volume 77 as actually measured: c0 is a different crop at 236
    /// slots, c1 and c2 are one edition at two paginations. The line a survey
    /// script collects, asserted verbatim — `scripts/margin-survey.sh` reads
    /// `stats:` lines exactly this way, and a format nobody can test drifts.
    #[test]
    fn the_set_line_reports_every_figure_that_decided_the_answer() {
        let line = set_line(&v077());

        assert_eq!(
            line,
            "set: copies=3 signature=0 clusters=2 flagged=0 refused=0 fragmented=2 \
             slots=236 decided=yes best=op/v077-c0"
        );
    }

    /// Nothing ranked still produces a line a survey loop can parse, rather
    /// than vanishing.
    #[test]
    fn a_set_with_nothing_ranked_still_produces_a_line() {
        let resolution = Resolution {
            copies: vec![
                Copy {
                    label: "op/v002-c0".into(),
                    slots: 202,
                },
                Copy {
                    label: "op/v002-c1".into(),
                    slots: 129,
                },
            ],
            signature_settled: 0,
            pairs: vec![Pair::judge(0, 1, &refusal(202), 202, 129)],
            clusters: vec![vec![0], vec![1]],
            contradictions: Vec::new(),
            flagged: vec![0, 1],
            reference_given: false,
            ranked: Vec::new(),
        };

        assert_eq!(
            set_line(&resolution),
            "set: copies=2 signature=0 clusters=2 flagged=2 refused=1 fragmented=0 \
             slots=- decided=no best=-"
        );
    }

    #[test]
    fn the_tables_show_the_pairs_the_clusters_and_the_rank() {
        let printed = tables(&v077());

        assert_eq!(
            printed,
            "\
copies
  0  236 slots  op/v077-c0
  1  234 slots  op/v077-c1
  2  234 slots  op/v077-c2

pairs
  0-1  100 runs  184 of 236  FRAGMENTED
  0-2  100 runs  186 of 236  FRAGMENTED
  1-2  1 run     234 of 234  sound  identity

clusters
  {0}
  {1, 2}  identity

rank
  1.  236 slots  copy 0
  2.  234 slots  copy 1
"
        );
    }

    /// Containment gets its own line, naming the contained copy first: "0
    /// within 1", not a bare "identity" — and now the container's own slot
    /// count, 199, so the figure that decides whether the containment matters
    /// is on the line rather than only inferable from the `copies` table
    /// above. Pinned separately from the identity table above so the "X
    /// within Y" rendering is not left only inferred from the enum's doc
    /// comment.
    #[test]
    fn the_tables_name_a_containment_pair() {
        let printed = tables(&cm_v01());

        assert_eq!(
            printed,
            "\
copies
  0  198 slots  cm/v01
  1  199 slots  cm/v01b

pairs
  0-1  1 run     198 of 198  sound  0 within 1 (199 slots)

clusters
  {0}
  {1}

rank
  1.  199 slots  copy 1
  2.  198 slots  copy 0
"
        );
    }

    /// The Chainsaw Man `v01`/`v01b` resolution, built from the shapes rather
    /// than from files: `v01` wholly within `v01b`, which carries one more
    /// slot and wins the rank on it.
    fn cm_v01() -> Resolution {
        let pairs = vec![Pair::judge(0, 1, &whole(198), 198, 199)];
        let clusters = cluster(2, &pairs);
        Resolution {
            copies: vec![
                Copy {
                    label: "cm/v01".into(),
                    slots: 198,
                },
                Copy {
                    label: "cm/v01b".into(),
                    slots: 199,
                },
            ],
            signature_settled: 0,
            contradictions: contradictions(&clusters, &pairs),
            flagged: flagged(2, &pairs),
            reference_given: false,
            ranked: rank(vec![
                Ranked {
                    cluster: vec![0],
                    slots: 198,
                    against: None,
                },
                Ranked {
                    cluster: vec![1],
                    slots: 199,
                    against: None,
                },
            ]),
            clusters,
            pairs,
        }
    }

    /// The mirror containment shape: Bocchi's hamaji copy carries the extra
    /// slot over krc, so it is the `a` side rather than the `b` side that
    /// contains. `Contains` must render the same "X within Y" wording as
    /// `Within`, naming the contained copy — here `b` — first, but *without*
    /// a parenthetical slot count: the container here is `a`, at 125 slots,
    /// and 125 is already printed as the denominator in "124 of 125" earlier
    /// on the same line, so the suffix does not say it again.
    #[test]
    fn the_tables_name_a_containment_pair_the_other_way() {
        let printed = tables(&bocchi_hamaji());

        assert_eq!(
            printed,
            "\
copies
  0  125 slots  bocchi/hamaji
  1  124 slots  bocchi/krc

pairs
  0-1  1 run     124 of 125  sound  1 within 0

clusters
  {0}
  {1}

rank
  1.  125 slots  copy 0
  2.  124 slots  copy 1
"
        );
    }

    /// Bocchi's hamaji copy against krc, built from the shapes rather than
    /// from files: 125 slots against 124, all 124 shared ones paired, `a`
    /// carrying the extra one.
    fn bocchi_hamaji() -> Resolution {
        let pairs = vec![Pair::judge(0, 1, &whole(124), 125, 124)];
        let clusters = cluster(2, &pairs);
        Resolution {
            copies: vec![
                Copy {
                    label: "bocchi/hamaji".into(),
                    slots: 125,
                },
                Copy {
                    label: "bocchi/krc".into(),
                    slots: 124,
                },
            ],
            signature_settled: 0,
            contradictions: contradictions(&clusters, &pairs),
            flagged: flagged(2, &pairs),
            reference_given: false,
            ranked: rank(vec![
                Ranked {
                    cluster: vec![0],
                    slots: 125,
                    against: None,
                },
                Ranked {
                    cluster: vec![1],
                    slots: 124,
                    against: None,
                },
            ]),
            clusters,
            pairs,
        }
    }

    /// The flagged section, which nothing rendered until the `--against` rule
    /// was fixed and which is where that rule's output lives. Without a
    /// reference both copies are named, and the hint says what would settle it
    /// — a person reading an empty rank section otherwise has to work out for
    /// themselves why it is empty.
    #[test]
    fn the_tables_flag_both_copies_and_say_what_would_settle_it() {
        let printed = tables(&starter_book(false));

        assert_eq!(
            printed,
            "\
copies
  0  202 slots  op/v002-c0
  1  198 slots  op/v002-c1

pairs
  0-1  1 run     0 of 202  REFUSED

clusters
  {0}
  {1}

not this volume
  0  refused against every other copy  op/v002-c0
  1  refused against every other copy  op/v002-c1
  nothing in this set says which of these is the impostor; pass --against a reference edition
"
        );
    }

    /// The same set with a reference edition. The reference paired with copy 1
    /// and nothing with copy 0, so only copy 0 is named — and with a different
    /// reason, because "refused against every other copy" is not what decided
    /// it and the pairs table would contradict that sentence where a flagged
    /// copy had paired inside the set. The hint is gone: the question it asks
    /// has been answered.
    #[test]
    fn the_tables_name_only_what_the_reference_refused_and_drop_the_hint() {
        let printed = tables(&starter_book(true));

        assert_eq!(
            printed,
            "\
copies
  0  202 slots  op/v002-c0
  1  198 slots  op/v002-c1

pairs
  0-1  1 run     0 of 202  REFUSED

clusters
  {0}
  {1}

not this volume
  0  paired nothing with the reference edition  op/v002-c0

rank
  1.  198 slots  copy 1  198 paired  1 run
"
        );
    }

    /// The One Piece Starter Book pair: two copies filed as one volume with a
    /// refusal between them, at 202 slots and 198. Built twice over from the
    /// same shapes — as the set alone resolves it, and as a reference edition
    /// that pairs with the smaller copy resolves it.
    fn starter_book(with_reference: bool) -> Resolution {
        let pairs = vec![Pair::judge(0, 1, &refusal(202), 202, 198)];
        let clusters = cluster(2, &pairs);
        Resolution {
            copies: vec![
                Copy {
                    label: "op/v002-c0".into(),
                    slots: 202,
                },
                Copy {
                    label: "op/v002-c1".into(),
                    slots: 198,
                },
            ],
            signature_settled: 0,
            contradictions: contradictions(&clusters, &pairs),
            flagged: if with_reference {
                vec![0]
            } else {
                flagged(2, &pairs)
            },
            reference_given: with_reference,
            ranked: if with_reference {
                vec![Ranked {
                    cluster: vec![1],
                    slots: 198,
                    against: Some(Against {
                        runs: 1,
                        paired: 198,
                    }),
                }]
            } else {
                Vec::new()
            },
            clusters,
            pairs,
        }
    }

    /// The contradictions branch: of `tables()`'s five conditional sections,
    /// the only one no test rendered until now. Built on
    /// `identity_that_is_not_transitive_is_reported_and_still_merged`'s shape
    /// — 0≡1 and 1≡2 both hold, but 0≢2 — so the cluster still merges all
    /// three and the sentence names the pair whose solve disagreed with the
    /// merge it is part of.
    #[test]
    fn the_tables_report_a_contradiction_in_a_merged_cluster() {
        let printed = tables(&contradiction_shape());

        assert_eq!(
            printed,
            "\
copies
  0  200 slots  shape/c0
  1  200 slots  shape/c1
  2  200 slots  shape/c2

pairs
  0-1  1 run     200 of 200  sound  identity
  1-2  1 run     200 of 200  sound  identity
  0-2  100 runs  100 of 200  FRAGMENTED

clusters
  {0, 1, 2}  identity

copies 0 and 2 were merged but did not align as identity; one of the three solves in that cluster is wrong

rank
  1.  200 slots  copy 0
"
        );
    }

    /// The shape `identity_that_is_not_transitive_is_reported_and_still_merged`
    /// establishes, turned into a full `Resolution` so `tables()` can render
    /// it: 0≡1 and 1≡2 both hold, but 0≢2 — a contradiction the union-find
    /// still merges into one cluster.
    fn contradiction_shape() -> Resolution {
        let pairs = vec![
            Pair::judge(0, 1, &whole(200), 200, 200),
            Pair::judge(1, 2, &whole(200), 200, 200),
            Pair::judge(0, 2, &fragmented(), 200, 200),
        ];
        let clusters = cluster(3, &pairs);
        Resolution {
            copies: vec![
                Copy {
                    label: "shape/c0".into(),
                    slots: 200,
                },
                Copy {
                    label: "shape/c1".into(),
                    slots: 200,
                },
                Copy {
                    label: "shape/c2".into(),
                    slots: 200,
                },
            ],
            signature_settled: 0,
            contradictions: contradictions(&clusters, &pairs),
            flagged: flagged(3, &pairs),
            reference_given: false,
            ranked: rank(vec![Ranked {
                cluster: vec![0, 1, 2],
                slots: 200,
                against: None,
            }]),
            clusters,
            pairs,
        }
    }

    /// The v077 resolution, built from the shapes rather than from files.
    fn v077() -> Resolution {
        let pairs = vec![
            Pair::judge(0, 1, &spanning(100, 184), 236, 234),
            Pair::judge(0, 2, &spanning(100, 186), 236, 234),
            Pair::judge(1, 2, &whole(234), 234, 234),
        ];
        let clusters = cluster(3, &pairs);
        Resolution {
            copies: vec![
                Copy {
                    label: "op/v077-c0".into(),
                    slots: 236,
                },
                Copy {
                    label: "op/v077-c1".into(),
                    slots: 234,
                },
                Copy {
                    label: "op/v077-c2".into(),
                    slots: 234,
                },
            ],
            signature_settled: 0,
            contradictions: contradictions(&clusters, &pairs),
            flagged: flagged(3, &pairs),
            reference_given: false,
            ranked: rank(vec![
                Ranked {
                    cluster: vec![0],
                    slots: 236,
                    against: None,
                },
                Ranked {
                    cluster: vec![1, 2],
                    slots: 234,
                    against: None,
                },
            ]),
            clusters,
            pairs,
        }
    }

    /// Exactly `runs` paired runs covering exactly `paired` slots between them,
    /// each separated by a slot of gap so they stay separate runs.
    ///
    /// The remainder is spread over the leading runs rather than dropped: a
    /// helper that quietly covered 100 slots when the test said 184 would make
    /// the assertion pass for the wrong reason. Assumes `paired >= runs`, which
    /// every caller satisfies.
    fn spanning(runs: u32, paired: u32) -> Vec<Run> {
        let (each, longer) = (paired / runs, paired % runs);
        let mut out = Vec::new();
        let mut at = 0;
        for run in 0..runs {
            let length = each + u32::from(run < longer);
            out.push(Run::paired(
                PageRange::new(at, at + length - 1),
                PageRange::new(at, at + length - 1),
            ));
            at += length + 1;
        }
        out
    }

    /// Copies are `--`-separated as in `topology`, but unlike `topology` this
    /// command has flags — and `--against` itself begins with `--`. Group
    /// collection therefore stops at the first argument that starts with `--`
    /// and is not exactly `--`.
    #[test]
    fn groups_end_at_the_first_flag() {
        let args: Vec<String> = ["a", "--", "b1", "b2", "--against", "ref", "--ltr"]
            .iter()
            .map(ToString::to_string)
            .collect();

        let parsed = groups(&args).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].len(), 2, "both b members, and not `ref`");
    }

    #[test]
    fn one_copy_is_not_a_duplicate_set() {
        let args: Vec<String> = ["only"].iter().map(ToString::to_string).collect();

        let error = groups(&args).unwrap_err();

        assert!(error.contains("two copies"), "unhelpful: {error}");
    }

    /// The same argument array `groups_end_at_the_first_flag` parses: the
    /// reference must not swallow `--ltr`, the flag that follows it.
    #[test]
    fn against_members_stops_at_the_next_flag() {
        let args: Vec<String> = ["a", "--", "b1", "b2", "--against", "ref", "--ltr"]
            .iter()
            .map(ToString::to_string)
            .collect();

        let members = against_members(&args).unwrap();

        assert_eq!(members, Some(vec![PathBuf::from("ref")]));
    }

    /// `--against` with nothing after it would otherwise collect an empty
    /// list and proceed with no reference edition — silently, since an empty
    /// list and "no `--against`" looked the same. Dropping the reference can
    /// change `decided=` and the rank order, so this must be refused rather
    /// than swallowed.
    #[test]
    fn against_with_no_member_is_an_error() {
        let args: Vec<String> = ["a", "--", "b", "--against", "--ltr"]
            .iter()
            .map(ToString::to_string)
            .collect();

        let error = against_members(&args).unwrap_err();

        assert!(error.contains("--against"), "unhelpful: {error}");
    }

    /// The saving the signature step exists for. Three copies of one scan
    /// fingerprint once, not three times — skipping a solve saves microseconds
    /// and skipping a decode of every page saves seconds.
    #[test]
    fn copies_confirmed_identical_all_stand_for_the_first() {
        let same = vec![signed("aaa"), signed("aaa"), signed("aaa")];
        let mut confirmed = 0;

        let stands_for = representatives(&same, |_, _| {
            confirmed += 1;
            Ok(true)
        })
        .unwrap();

        assert_eq!(stands_for, vec![0, 0, 0]);
        assert_eq!(
            confirmed, 2,
            "each later copy confirmed against the first only"
        );
    }

    /// The signature is a claim about sizes, not about bytes. When the content
    /// hash refutes it the copies stay separate and get solved like any other
    /// pair — the confirmation is what makes the short-circuit sound rather
    /// than merely fast.
    #[test]
    fn a_signature_match_the_content_hash_refutes_does_not_collapse() {
        let same = vec![signed("aaa"), signed("aaa")];

        let stands_for = representatives(&same, |_, _| Ok(false)).unwrap();

        assert_eq!(stands_for, vec![0, 1]);
    }

    /// And the expensive confirmation is never reached where a listing already
    /// settled it.
    #[test]
    fn copies_with_different_signatures_are_never_hashed() {
        let different = vec![signed("aaa"), signed("bbb")];

        let stands_for = representatives(&different, |_, _| {
            panic!("a differing signature must not cost a content hash")
        })
        .unwrap();

        assert_eq!(stands_for, vec![0, 1]);
    }

    /// The bad shape `memoized_confirm` exists to fix: a set whose signatures
    /// all collide but whose bytes all differ, so `confirm` never returns
    /// `true` and `representatives` tries every earlier copy against every
    /// later one — five copies, ten comparisons. Without memoisation each
    /// comparison re-hashes both sides, so copy 0 alone would be hashed four
    /// times over. With it, every copy's digest is computed exactly once,
    /// however many candidates it goes on to be compared against.
    #[test]
    fn a_copy_is_hashed_once_however_many_candidates_compare_it() {
        let same = vec![signed("aaa"); 5];
        let mut hashed = vec![0u32; 5];

        let confirm = memoized_confirm(5, |idx| {
            hashed[idx] += 1;
            // A digest distinct per copy, so every comparison refutes the
            // signature match and `representatives` keeps trying — the
            // all-refuse shape that makes it compare every earlier copy.
            Ok(idx.to_string())
        });
        let stands_for = representatives(&same, confirm).unwrap();

        assert_eq!(
            stands_for,
            vec![0, 1, 2, 3, 4],
            "all distinct; nothing collapses"
        );
        assert_eq!(
            hashed,
            vec![1, 1, 1, 1, 1],
            "each copy's digest computed exactly once, not once per comparison"
        );
    }

    fn signed(digest: &str) -> Signature {
        Signature {
            pages: 3,
            bytes: 600,
            digest: digest.to_string(),
        }
    }

    /// The saving the signature step exists for, held by the only means that
    /// can hold it: counting decodes. Two byte-identical copies must
    /// fingerprint once, not twice — and must still come out identity, since
    /// the second copy's slot count is derived rather than measured.
    #[test]
    fn a_copy_confirmed_identical_is_never_fingerprinted() {
        let (first, second) = two_identical_copies("skip");
        let mut decoded = 0;
        let mut fingerprint = |_: &[PathBuf]| {
            decoded += 1;
            Ok(vec![PageHash([1, 2, 3, 4]); 6])
        };

        let resolved = resolve_with(
            &[vec![first.clone()], vec![second.clone()]],
            None,
            &mut fingerprint,
        )
        .unwrap();

        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
        assert_eq!(decoded, 1, "the second copy is derived, not decoded");
        assert_eq!(resolved.signature_settled, 1);
        assert_eq!(
            resolved.copies[1].slots, 6,
            "slot count taken from the first"
        );
        assert_eq!(resolved.pairs[0].relation, Relation::Identity);
        assert_eq!(
            resolved.pairs[0].runs, 1,
            "no solve ran; the run is the whole of it"
        );
        assert_eq!(resolved.clusters, vec![vec![0, 1]]);
    }

    /// The mirror case: a signature match the content hash refutes. Same page
    /// sizes give matching signatures, but different bytes must still cost a
    /// fingerprint for both copies — proving the content-hash confirmation is
    /// load-bearing rather than decorative.
    #[test]
    fn a_signature_match_the_bytes_refute_fingerprints_both() {
        let (first, second) = two_copies_matching_sizes_different_bytes("differ");
        let mut decoded = 0;
        let mut fingerprint = |_: &[PathBuf]| {
            decoded += 1;
            Ok(vec![PageHash([1, 2, 3, 4]); 6])
        };

        // The precondition the test's name depends on. If `Signature::of` ever
        // stopped matching these two, `representatives` would never call
        // `confirm` at all, both copies would be fingerprinted for want of a
        // signature match rather than in spite of one, and both assertions
        // below would pass for the opposite reason.
        assert_eq!(
            Signature::of(std::slice::from_ref(&first)).unwrap().digest,
            Signature::of(std::slice::from_ref(&second)).unwrap().digest,
            "the fixture must produce a signature match for this test to mean anything"
        );

        let resolved = resolve_with(
            &[vec![first.clone()], vec![second.clone()]],
            None,
            &mut fingerprint,
        )
        .unwrap();

        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
        assert_eq!(decoded, 2, "the content hash refuted the signature match");
        assert_eq!(resolved.signature_settled, 0);
    }

    /// Two temp directories holding the same handful of `.jpg` files with the
    /// same bytes, so `Signature::of` and `content_digest` do genuine work on
    /// real bytes while `fingerprint` is a stub that never opens them — the
    /// files need not be decodable images at all, which is the whole point.
    /// Any stale directory from a previous run is removed first, as
    /// `signature.rs`'s `same_pages_two_ways` does.
    fn two_identical_copies(tag: &str) -> (PathBuf, PathBuf) {
        let first = std::env::temp_dir().join(format!("taiyaku-duplicates-{tag}-a"));
        let second = std::env::temp_dir().join(format!("taiyaku-duplicates-{tag}-b"));
        for dir in [&first, &second] {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).unwrap();
            for (name, bytes) in [
                ("p1.jpg", &b"one"[..]),
                ("p2.jpg", &b"two"[..]),
                ("p3.jpg", &b"three"[..]),
            ] {
                std::fs::write(dir.join(name), bytes).unwrap();
            }
        }
        (first, second)
    }

    /// Two copies with matching page sizes but differing content: same
    /// signature, different content digest. Built from `two_identical_copies`
    /// by overwriting one file with same-length, different bytes.
    fn two_copies_matching_sizes_different_bytes(tag: &str) -> (PathBuf, PathBuf) {
        let (first, second) = two_identical_copies(tag);
        std::fs::write(second.join("p2.jpg"), b"TWO").unwrap();
        (first, second)
    }

    /// What the set alone can conclude, pinned so the reference rule below
    /// cannot quietly change it: two copies, a refusal between them, no
    /// reference. Both are flagged because the refusal is symmetric, nothing is
    /// ranked, and the output says what would settle it.
    #[test]
    fn a_refusing_pair_with_no_reference_flags_both_and_ranks_nothing() {
        let impostor = copy_on_disk("within-set", "impostor", 8);
        let real = copy_on_disk("within-set", "real", 6);
        let mut fingerprint = |members: &[PathBuf]| Ok(synthetic_pages(members));

        let resolved = resolve_with(
            &[vec![impostor.clone()], vec![real.clone()]],
            None,
            &mut fingerprint,
        )
        .unwrap();

        clean_up(&[&impostor, &real]);
        assert_eq!(resolved.pairs[0].relation, Relation::Refused);
        assert_eq!(
            resolved.flagged,
            vec![0, 1],
            "nothing inside the set says which of the two is the impostor"
        );
        assert!(resolved.ranked.is_empty());
        assert!(
            tables(&resolved).contains("nothing in this set says which of these is the impostor"),
            "the hint belongs here, where the set could not decide:\n{}",
            tables(&resolved)
        );
    }

    /// The same two copies with a reference edition, which is the case
    /// `--against` exists for. The reference decides: the copy that pairs with
    /// it is this volume even though it refused the other copy, and the copy
    /// that pairs nothing with it is not, even though it carries more slots.
    ///
    /// The impostor holds 8 slots to the real copy's 6, so a ranking that
    /// merely ordered on content would put it first. It is not ordered at all —
    /// the reference removes it before the ranking sees it.
    #[test]
    fn a_reference_the_real_copy_pairs_with_flags_only_the_impostor() {
        let impostor = copy_on_disk("reference-decides", "impostor", 8);
        let real = copy_on_disk("reference-decides", "real", 6);
        let reference = copy_on_disk("reference-decides", "reference", 6);
        let mut fingerprint = |members: &[PathBuf]| Ok(synthetic_pages(members));

        let resolved = resolve_with(
            &[vec![impostor.clone()], vec![real.clone()]],
            Some(std::slice::from_ref(&reference)),
            &mut fingerprint,
        )
        .unwrap();

        clean_up(&[&impostor, &real, &reference]);
        assert_eq!(
            resolved.pairs[0].relation,
            Relation::Refused,
            "the two still refuse each other; it is the reference that decides"
        );
        assert_eq!(
            resolved.flagged,
            vec![0],
            "only the copy the reference refused"
        );
        assert_eq!(
            resolved.ranked.len(),
            1,
            "the real copy is ranked despite refusing the other: {:?}",
            resolved.ranked
        );
        assert_eq!(resolved.ranked[0].cluster, vec![1]);
        assert_eq!(
            resolved.ranked[0].against,
            Some(Against { runs: 1, paired: 6 }),
            "scored against the reference, which a flagged cluster never was"
        );
        assert!(decided(&resolved.ranked));
        assert!(
            !tables(&resolved).contains("nothing in this set says"),
            "the question the hint asks was answered:\n{}",
            tables(&resolved)
        );
    }

    /// A reference edition neither copy pairs with. Then neither is this
    /// volume, both are flagged and nothing is ranked — the reference decides
    /// against both as readily as it decides between them.
    #[test]
    fn a_reference_both_copies_refuse_flags_both() {
        let impostor = copy_on_disk("reference-refuses-both", "impostor", 8);
        let real = copy_on_disk("reference-refuses-both", "real", 6);
        let alien = copy_on_disk("reference-refuses-both", "alien", 6);
        let mut fingerprint = |members: &[PathBuf]| Ok(synthetic_pages(members));

        let resolved = resolve_with(
            &[vec![impostor.clone()], vec![real.clone()]],
            Some(std::slice::from_ref(&alien)),
            &mut fingerprint,
        )
        .unwrap();

        clean_up(&[&impostor, &real, &alien]);
        assert_eq!(resolved.flagged, vec![0, 1], "none of them is the volume");
        assert!(resolved.ranked.is_empty());
        assert_eq!(
            set_line(&resolved),
            "set: copies=2 signature=0 clusters=2 flagged=2 refused=1 fragmented=0 \
             slots=- decided=no best=-"
        );
        assert!(
            !tables(&resolved).contains("nothing in this set says"),
            "a reference was given, so the hint asking for one must not print:\n{}",
            tables(&resolved)
        );
    }

    /// One copy on disk: a directory of throwaway `.jpg` files, holding the
    /// role in its name.
    ///
    /// The files have to exist because `resolve_with` signs every copy from a
    /// listing before anything is fingerprinted, and the page counts differ by
    /// role so no two copies sign alike. What the *solver* sees is not in these
    /// bytes at all — `synthetic_pages` decides that — so they need not be
    /// images. Any stale directory from a previous run is removed first, as
    /// `two_identical_copies` does.
    fn copy_on_disk(tag: &str, role: &str, pages: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("taiyaku-duplicates-{tag}-{role}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for page in 0..pages {
            std::fs::write(dir.join(format!("p{page}.jpg")), format!("{role}-{page}")).unwrap();
        }
        dir
    }

    fn clean_up(dirs: &[&PathBuf]) {
        for dir in dirs {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Six 128-bit patterns, each with exactly 64 bits set and no two of them
    /// closer than 64 bits apart.
    ///
    /// The equal weights are what let a page of one family sit exactly 128 bits
    /// from every page of another; the mutual distance is what stops a page
    /// pairing with the wrong page of its own family.
    const PATTERNS: [(u64, u64); 6] = [
        (0xFFFF_FFFF_FFFF_FFFF, 0x0000_0000_0000_0000),
        (0x0000_0000_FFFF_FFFF, 0xFFFF_FFFF_0000_0000),
        (0xAAAA_AAAA_AAAA_AAAA, 0xAAAA_AAAA_AAAA_AAAA),
        (0x5555_5555_5555_5555, 0x5555_5555_5555_5555),
        (0xFFFF_0000_FFFF_0000, 0x0000_FFFF_0000_FFFF),
        (0x00FF_00FF_00FF_00FF, 0xFF00_FF00_FF00_FF00),
    ];

    /// What the solver sees for a copy, chosen by the role in its directory
    /// name rather than read from its bytes.
    ///
    /// `resolve_with` takes a fingerprint closure precisely so the rules over
    /// alignments can be tested on stated alignments rather than on fixtures,
    /// and a refusal has to be a certainty here rather than a likely outcome of
    /// pseudo-random bits — a test that measured the calibration threshold by
    /// accident would fail the day it moved.
    ///
    /// `real` and `reference` are the same six patterns in the low half of the
    /// hash, so they align as one clean run over every slot. `impostor` is the
    /// same patterns moved into the high half, so it differs from every `real`
    /// page in exactly 128 of 256 bits: similarity 0.5 for **every** pair in
    /// the matrix, which leaves calibration no separation to find and refuses.
    /// `alien` is that high half filled solid, a constant similarity against
    /// both families and so a refusal against both.
    fn synthetic_pages(members: &[PathBuf]) -> Vec<PageHash> {
        let name = members[0].to_string_lossy().to_string();
        match name.rsplit('-').next().unwrap_or_default() {
            "real" | "reference" => PATTERNS
                .iter()
                .map(|&(low, high)| PageHash([low, high, 0, 0]))
                .collect(),
            "impostor" => (0..8)
                .map(|page| {
                    let (low, high) = PATTERNS[page % PATTERNS.len()];
                    PageHash([0, 0, low, high])
                })
                .collect(),
            "alien" => vec![PageHash([0, 0, u64::MAX, u64::MAX]); 6],
            other => panic!("no synthetic pages for the role `{other}`"),
        }
    }
}
