//! Resolving several copies of one volume into a decision.
//!
//! The protocol is `docs/status.md` §2, recorded in `docs/notes.md` under
//! "Sorting 109 volumes' worth of duplicates": signature, then all pairs
//! *within* the set, then a ranking of what is genuinely distinct.
//!
//! All pairs rather than every copy against the first, because the first copy
//! is the one most likely to be the odd one out, since the sort that produced
//! it was by page count.

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
/// would re-decode both sides from scratch on every attempt. A copy compared as
/// `earlier` against every later copy would pay for its own decode once per
/// later copy: O(n²) full content reads over a set whose signatures collide,
/// where O(n) — one hash per copy — answers the same question.
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
/// Spreads always split. In One Piece v092 c0's slots come out wholly within
/// c1's — every one of c1's images detects as a spread, so slot expansion
/// doubles its count. `notes.md` draws the conclusion directly: page counts are
/// a property of packaging, slot counts a property of the work. A duplicate
/// command that compared pages would get its own headline case wrong, so there
/// is no flag for this.
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

/// The protocol over one set, fingerprinting through a caller-supplied closure.
///
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
    // `docs/status.md` records as measured. Two copies confirmed byte-identical
    // are not solved: the solver would be comparing a page sequence with
    // itself, and the run describing that is the whole of it.
    let mut pairs = Vec::new();
    for a in 0..copies.len() {
        for b in a + 1..copies.len() {
            let slots = described[a].slots;
            // The `slots > 0` guard, on the untruncated usize, rules out
            // `- 1` underflowing.
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
mod tests;
