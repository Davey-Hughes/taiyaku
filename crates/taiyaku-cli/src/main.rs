//! Command line for taiyaku: the aligner, and the server that hands a reader
//! what it wrote.
//!
//!   taiyaku align <a-member>... -- <b-member>...
//!   taiyaku serve <taiyaku.json>...
//!
//! Each side of an alignment is an *edition*: one or more archives or
//! directories that flatten into a single page sequence. `--` separates the
//! two, so a raw volume can be aligned against the several chapter files that
//! make it up.
//!
//! Fingerprints are cached under `.taiyaku-cache/` by member content hash, so
//! re-solving costs milliseconds rather than seconds.
//!
//! `serve` is a thin front for `taiyaku-serve`: argument parsing and a runtime.
//! Everything the server does lives in that crate.

mod cache;
#[cfg(test)]
mod corpus;
mod credits;
mod detect;
mod duplicates;
mod fingerprint;
// A one-off measurement rather than a shipped command, and gated on
// `TAIYAKU_CORPUS` exactly as `corpus` is — so it is declared the same way, and
// no part of it is compiled into the binary.
#[cfg(test)]
mod geometry_experiment;
mod inventory;
mod regions;
// A one-off measurement rather than a shipped command, gated on
// `TAIYAKU_CORPUS` exactly as `corpus` and `geometry_experiment` are — so it
// is declared the same way, and no part of it is compiled into the binary.
#[cfg(test)]
mod region_experiment;
mod report;
mod signature;
mod stats;

use std::path::{Path, PathBuf};

use taiyaku_core::{
    Alignment, Anchor, Direction, Edition, Half, Member, Merged, Run, Sidecar, Slot, SolverInfo,
    Source, Via, Work, compose, confidence, counterpart, entries_from_runs, solve,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(message) = run(&args) {
        eprintln!("taiyaku: {message}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("align") => align(&args[1..]),
        Some("verify") => verify(&args[1..]),
        Some("topology") => topology(&args[1..]),
        Some("lookup") => lookup(&args[1..]),
        Some("serve") => serve(&args[1..]),
        Some("duplicates") => duplicates::command(&args[1..]),
        Some("inventory") => inventory::command(&args[1..]),
        Some("credits") => credits::command(&args[1..]),
        Some("regions") => regions::command(&args[1..]),
        Some(other) => Err(format!("unknown command `{other}`")),
        None => {
            println!("taiyaku {}", env!("CARGO_PKG_VERSION"));
            println!("usage: taiyaku align  <a-member>... -- <b-member>...");
            println!(
                "       taiyaku verify <a-member>... -- <b-member>... [--out FILE] [--sidecar FILE]"
            );
            println!("       taiyaku topology <hub>... -- <spoke>... -- <spoke>...");
            println!("       taiyaku lookup <taiyaku.json> <from> <to> <page>...");
            println!(
                "       taiyaku serve  <taiyaku.json>... [--addr HOST:PORT] [--stump-url URL] \
                 [--stump-key KEY] [--log-requests FILE] [--refresh-stump]"
            );
            println!(
                "       taiyaku duplicates <copy>... -- <copy>... [--against <member>...] [--ltr]"
            );
            println!("       taiyaku inventory <root> [--min-pages N] [--pattern SPEC]");
            println!(
                "       taiyaku credits <member>... --sidecar FILE [--against EDITION] \
                 [--cache DIR] [--write]"
            );
            println!("       taiyaku regions <member>... --sidecar FILE [--write]");
            println!();
            println!("align options: --ltr --stats --cache DIR --write FILE --title NAME");
            println!("               --name-a KEY --name-b KEY --direct --no-split-spreads");
            println!();
            println!("align splits joined spreads by default. --no-split-spreads gives the");
            println!("page-level diagnostic instead, which cannot write a sidecar: a sidecar");
            println!("records which pages are joined spreads, and that path does not detect");
            println!("them. So --write, --cache and --ltr are refused alongside it, and");
            println!("--title, --name-a, --name-b and --direct describe a sidecar and so need");
            println!("--write. Each is refused rather than ignored where it cannot take effect.");
            println!();
            println!("--write merges into an existing FILE rather than overwriting it. On a");
            println!("merge the a-side is the document's hub, so --name-a is optional;");
            println!("--direct records a spoke-to-spoke edge instead and requires it.");
            Ok(())
        }
    }
}

/// Split `a... -- b...` into the two editions, pulling off trailing options.
///
/// Both sides come back in natural order rather than as typed. A member list is
/// a page sequence, and a shell glob is not sorted into one — `*.cbz` yields
/// `c10.5, c10, c1, c2` — so the order given is the one thing here that must not
/// be trusted. Ordering at this single funnel is what keeps the consumers in
/// agreement: `taiyaku_read::read_edition` and `cached_edition` sort for
/// themselves, but `describe_edition` writes the list straight into the sidecar
/// and `slug` and `label` name an edition after its first member.
fn split_editions(args: &[String]) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let split = args
        .iter()
        .position(|a| a == "--")
        .ok_or("needs `--` between the two editions")?;
    let (left, rest) = args.split_at(split);
    // A flag typed before the separator lands in the a-side member list and
    // used to surface as `--stats: No such file or directory`, which names a
    // file the caller never meant and hides the actual mistake. By the time
    // this runs the flag is known to exist — `reject_unknown_flags` has
    // already refused the ones that do not — so the only thing left to say is
    // that it is in the wrong place.
    if let Some(misplaced) = left.iter().find(|a| a.starts_with("--")) {
        return Err(format!(
            "`{misplaced}` is a flag, but it is before the `--` that separates \
             the two editions, so it was read as a member; flags go after both"
        ));
    }
    let mut a_members: Vec<PathBuf> = left.iter().map(PathBuf::from).collect();
    let mut b_members: Vec<PathBuf> = rest[1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if a_members.is_empty() || b_members.is_empty() {
        return Err("both sides need at least one member".into());
    }
    for side in [&mut a_members, &mut b_members] {
        side.sort_by(|a, b| taiyaku_read::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    }
    Ok((a_members, b_members))
}

/// The value typed after `flag`, or `None` where the flag was not typed at all.
///
/// `expects` names what the flag wanted — "a path", "a number of pages" — and
/// is only ever read on the two ways this goes wrong, both of which used to be
/// silent. A flag last on the line has nothing after it and used to read as no
/// flag at all; a flag followed by another flag used to take that flag as its
/// value, so `align a -- b --write --ltr` wrote the sidecar to a file named
/// `--ltr` and applied the direction anyway. Neither is distinguishable from a
/// correct run by anything the caller can see, which is why both are refused
/// here rather than left to each caller to notice.
///
/// **A value beginning with `--` is a flag, not a value**, without asking which
/// flags exist. That is the rule `split_editions`, `serve`'s path list and
/// `duplicates::against_members` already apply to their positional arguments,
/// so it is the command's existing convention rather than a new one. The cost
/// is that a file genuinely named `--something` cannot be named on the command
/// line; the benefit is that every valued flag refuses the same way, including
/// the ones whose values are never otherwise inspected. `--name-a` needs this
/// most: `-` is in `validate_key`'s safe alphabet, so `--name-b` passes as a
/// well-formed edition key and lands in a sidecar as a URL path segment.
pub(crate) fn option_value<'a>(
    args: &'a [String],
    flag: &str,
    expects: &str,
) -> Result<Option<&'a str>, String> {
    let Some(at) = args.iter().position(|a| a == flag) else {
        return Ok(None);
    };
    match args.get(at + 1) {
        Some(next) if next.starts_with("--") => Err(format!(
            "{flag} needs {expects}, and `{next}` is another flag"
        )),
        Some(value) => Ok(Some(value.as_str())),
        None => Err(format!("{flag} needs {expects}, and nothing followed it")),
    }
}

/// Which edition of `doc` is made of exactly these members, by content.
///
/// Content, not paths: `edition_of` compares the sorted sha256 set, the idiom
/// `merge_spoke`'s own guard states — a renamed but byte-identical archive is
/// still this edition. Shared by `credits` and `regions`, which ask the same
/// question and must not drift on the answer or on how it is worded.
pub(crate) fn edition_of_digests(
    doc: &Sidecar,
    sidecar: &str,
    digests: &[String],
) -> Result<String, String> {
    doc.edition_of(digests).map(str::to_string).ok_or_else(|| {
        format!(
            "no edition of `{sidecar}` is made of these {} members.\nlooked for: {}",
            digests.len(),
            digests.join(", ")
        )
    })
}

/// Refuse any `--`-prefixed argument this command does not have a flag for.
///
/// O15, O19 and O20 each refuse a *known* flag used wrongly, and all three key
/// off the flag's name — so none of them can see a name that was typed wrong.
/// A misspelling therefore reproduced the exact failure O19 closed, through
/// the one door left open: `taiyaku align a -- b --writ out.json` reported a
/// plausible alignment, wrote nothing, and exited 0. Worse, `--split-spread`
/// for `--split-spreads` returns a *different but plausible* answer rather
/// than an obviously broken one.
///
/// **O15 is what makes this checkable at all.** Once a flag's value can never
/// itself begin with `--`, every `--`-prefixed token in an argv is
/// unambiguously meant to be a flag, so an unrecognised one is a mistake
/// rather than possibly somebody's filename. Before that rule this scan would
/// have had to know which flags take values and skip past them.
///
/// `--` is the edition separator and not a flag, so it is excluded. Each
/// command passes its own set rather than sharing a global one: `--pattern` is
/// real for `inventory` and a typo for `align`, and a global list would accept
/// it in both.
fn reject_unknown_flags(command: &str, args: &[String], known: &[&str]) -> Result<(), String> {
    let Some(unknown) = args
        .iter()
        .find(|a| a.starts_with("--") && a.as_str() != "--" && !known.contains(&a.as_str()))
    else {
        return Ok(());
    };
    // The alphabet is listed rather than a nearest-match suggested. A typo is
    // usually one character off, but the failure this exists to catch is a
    // flag the caller believed in, and the honest answer to that is what does
    // exist — a single guess would be right often and confidently wrong the
    // rest of the time.
    match known.is_empty() {
        true => Err(format!(
            "{command} takes no flags, and `{unknown}` was given as one"
        )),
        false => Err(format!(
            "{command} has no flag `{unknown}`; its flags are {}",
            known.join(", ")
        )),
    }
}

/// Reject an edition key that is not URL-safe, before it reaches a sidecar.
///
/// A key is addressed in two URLs — `/api/works/{work}/{edition}/pages/{n}`
/// and the OPDS stream route — so it has to survive both. `--name-a` and
/// `--name-b` hand this function whatever the caller typed, unlike the
/// auto-derived path in `slug`, which already produces only this alphabet.
/// URL-safe is defined as that same alphabet: ASCII alphanumerics and `-`.
/// Anything else is both awkward in a URL and awkward to type by hand into a
/// correction later, which is the whole reason these are slugs and not
/// titles.
///
/// Called by `align` before any archive is opened, so a typo is a fast
/// failure rather than one that surfaces after fingerprinting has run.
fn validate_key(key: &str) -> Result<(), String> {
    // Before the character scan, because scanning an empty key finds no
    // offending character and would pass it as safe — and an empty key is not
    // a usable path segment, so it is exactly the failure the scan exists to
    // catch, arriving by the one route the scan cannot see.
    if key.is_empty() {
        return Err(
            "edition key is empty; a key names an edition in a URL and in a correction, \
             so it needs at least one character"
                .to_string(),
        );
    }
    let Some((position, offender)) = key
        .chars()
        .enumerate()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '-'))
    else {
        return Ok(());
    };
    let named = match offender {
        ' ' => "space".to_string(),
        other => format!("`{other}`"),
    };
    let suggestion = url_safe_suggestion(key);
    Err(format!(
        "edition key `{key}` is not URL-safe ({named} at position {position}); try `{suggestion}`"
    ))
}

/// The fix `validate_key`'s message points to: every character outside the
/// safe alphabet flattened to `-`, then trimmed the same way the auto-derived
/// slug is.
fn url_safe_suggestion(key: &str) -> String {
    let slug: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('-').to_string();
    match trimmed.is_empty() {
        true => "edition".to_string(),
        false => trimmed,
    }
}

/// A short identifier for an edition, derived from its first member unless the
/// caller names one. Sidecar keys are referenced by hand in corrections, so
/// `raw-mono` beats `a`.
///
/// A caller-given `given` is trusted verbatim here because `align` validates
/// it with `validate_key` before this is ever reached — this function only
/// has to shape the auto-derived path, which already produces the same safe
/// alphabet by construction.
fn slug(members: &[PathBuf], given: Option<&str>) -> String {
    if let Some(name) = given {
        return name.to_string();
    }
    let stem = members
        .first()
        .and_then(|p| p.file_stem())
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let slug: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-').to_string();
    match trimmed.is_empty() {
        true => "edition".to_string(),
        false => trimmed.chars().take(48).collect(),
    }
}

fn label(members: &[PathBuf]) -> String {
    let first = members
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    match members.len() {
        0 | 1 => first,
        n => format!("{first} + {} more", n - 1),
    }
}

fn verify(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("verify", args, &["--cache", "--ltr", "--out", "--sidecar"])?;
    let (a_members, b_members) = split_editions(args)?;
    // Every valued flag is resolved here, before an archive is opened, for the
    // reason `align` gives at its own top: a flag typed wrong is a typo, and a
    // typo should not cost the seconds it takes to read two editions before it
    // is reported. `--sidecar` and `--cache` are each read once here rather
    // than at each of the two places below that used to look them up.
    let out = option_value(args, "--out", "a path")?.unwrap_or("taiyaku-verify.html");
    let sidecar_path = option_value(args, "--sidecar", "a path")?;
    let cache_root = std::path::PathBuf::from(
        option_value(args, "--cache", "a directory")?.unwrap_or(".taiyaku-cache"),
    );
    // O22, O20's rule in the command O20 did not reach. Both of these are read
    // only from the `--sidecar` arm below: without a sidecar `verify` solves
    // the pair directly, never calls `cached_edition`, and so consults neither
    // the cache nor a reading direction. Measured before the fix — `verify a
    // -- b --cache DIR` did not create DIR.
    if sidecar_path.is_none() {
        if args.iter().any(|a| a == "--cache") {
            return Err(
                "--cache needs --sidecar: without one verify solves the pair \
                        directly and reads nothing through the cache"
                    .into(),
            );
        }
        if args.iter().any(|a| a == "--ltr") {
            return Err(
                "--ltr needs --sidecar: without one verify pairs pages rather than \
                        slots, and a page pairing has no direction to read"
                    .into(),
            );
        }
    }

    let a_pages = taiyaku_read::read_edition(&a_members)?;
    let b_pages = taiyaku_read::read_edition(&b_members)?;
    let (a, a_thumbs) = report::prepare(&a_pages, report::THUMB_WIDTH)?;
    let (b, b_thumbs) = report::prepare(&b_pages, report::THUMB_WIDTH)?;
    eprintln!("A: {} pages | B: {} pages", a.len(), b.len());

    // With a sidecar the alignment comes from the file rather than the solver,
    // which is what makes a hand correction reviewable: edit the runs, render
    // again, look at what changed. Confidence is still recomputed from the
    // fingerprints, because a stored run-level margin cannot shade a page.
    let direction = if args.iter().any(|a| a == "--ltr") {
        Direction::LeftToRight
    } else {
        Direction::RightToLeft
    };
    // The summary at the end is counted in whatever unit the runs are addressed
    // in. A solved run indexes pages; a run out of a sidecar indexes slots, and
    // an edition with seven spreads has seven more slots than pages — which is
    // how the same summary reported 192 of 187 pages paired, at 102.7%.
    let (runs, a_units) = match sidecar_path {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let sidecar = Sidecar::from_json(&text)?;

            // Which two editions is this? Asked of the archives rather than
            // assumed from the document's order. Taking the first alignment
            // worked only while `align` could write just one; against a
            // five-edition document it renders a confident report of a pair
            // nobody asked about, which is the single failure this report
            // exists to prevent.
            let named = |members: &[PathBuf], side: &str| -> Result<String, String> {
                let mut digests = Vec::new();
                for member in members {
                    digests.push(taiyaku_read::member_digest(member)?);
                }
                sidecar
                    .edition_of(&digests)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        format!(
                            "the {side} members are not an edition of {path}. It defines {}; \
                             the first member hashes {}",
                            sidecar
                                .editions
                                .keys()
                                .map(String::as_str)
                                .collect::<Vec<_>>()
                                .join(", "),
                            digests
                                .first()
                                .map(|d| &d[..8.min(d.len())])
                                .unwrap_or("nothing"),
                        )
                    })
            };
            let a_name = named(&a_members, "a-side")?;
            let b_name = named(&b_members, "b-side")?;

            // Through `route` rather than a stored edge, so a composed pair is
            // reviewable too. That is the alignment most worth looking at: it
            // is the one no solver ever checked.
            let route = taiyaku_core::route(&sidecar, &a_name, &b_name)
                .ok_or_else(|| format!("{path} carries no path from `{a_name}` to `{b_name}`"))?;
            let how = match &route.via {
                Via::Direct => String::new(),
                Via::Inverted => format!(", from the stored {b_name} -> {a_name} edge"),
                Via::Hub(hub) => format!(", composed through {hub}"),
                Via::Identity => ", identical edition".to_string(),
            };
            // `by` belongs to a stored edge, and a composed path has none.
            let by = sidecar
                .alignments
                .iter()
                .find(|al| {
                    (al.from == a_name && al.to == b_name) || (al.from == b_name && al.to == a_name)
                })
                .map(|al| format!(", by {:?}", al.by))
                .unwrap_or_default();
            eprintln!(
                "alignment from {path}: {a_name} -> {b_name}{how} ({} runs{by})",
                route.runs().len()
            );
            // The document's own reading order, not the `--ltr` flag: the runs
            // were written under the former and the slot count has to match
            // what they address.
            let slots = sidecar
                .editions
                .get(&a_name)
                .map(|e| e.slots(sidecar.work.reading).len())
                .unwrap_or(a.len());
            (route.runs().to_vec(), slots)
        }
        None => (solve(&a, &b), a.len()),
    };
    // Confidence has to be measured in whatever space the runs are addressed in.
    // A solved run indexes pages; a run out of a sidecar indexes slots, and
    // scoring it against page hashes silently compares the wrong things — it
    // reported 189 doubtful pairings on an alignment that has two.
    let scored = match sidecar_path {
        Some(_) => {
            let (a_slot_hashes, a_slots, _) = cached_edition(&cache_root, &a_members, direction)?;
            let (b_slot_hashes, b_slots, _) = cached_edition(&cache_root, &b_members, direction)?;
            let slot_scores = confidence(&a_slot_hashes, &b_slot_hashes, &runs);
            // The report is page-indexed, so map slots back to the pages whose
            // thumbnails it will show.
            slot_scores
                .into_iter()
                .filter_map(|s| {
                    let page = a_slots.get(s.page as usize)?.page;
                    Some(taiyaku_core::PageConfidence {
                        page,
                        counterpart: s
                            .counterpart
                            .and_then(|c| b_slots.get(c as usize))
                            .map(|slot| slot.page),
                        ..s
                    })
                })
                .collect()
        }
        None => confidence(&a, &b, &runs),
    };
    let html = report::render(
        &label(&a_members),
        &label(&b_members),
        &a_thumbs,
        &b_thumbs,
        &scored,
        runs.len(),
    );
    std::fs::write(out, &html).map_err(|e| format!("{out}: {e}"))?;

    report_summary(&runs, a_units);
    let ambiguous = scored
        .iter()
        .filter(|s| s.counterpart.is_some() && s.margin < 0.05)
        .count();
    eprintln!(
        "wrote {out} ({:.1} MB) | {ambiguous} ambiguous pairings to review",
        html.len() as f64 / 1_048_576.0
    );
    Ok(())
}

/// Measure what hub-and-spoke costs against solving every pair directly.
///
/// The star topology is a bet: that composing A to hub to B is as good as
/// aligning A to B, in exchange for N-1 alignments instead of N(N-1)/2 and
/// transitivity for free. The bet is testable because all-pairs is not a second
/// system, only the same solver run more times.
fn topology(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("topology", args, &[])?;
    let groups: Vec<Vec<PathBuf>> = args
        .split(|a| a == "--")
        .filter(|g| !g.is_empty())
        .map(|g| g.iter().map(PathBuf::from).collect())
        .collect();
    if groups.len() < 3 {
        return Err("needs a hub and at least two spokes, separated by `--`".into());
    }
    let (hub, spokes) = groups.split_first().unwrap();

    eprintln!("hub:  {}", label(hub));
    let hub_hashes = fingerprint::fingerprint_edition(&taiyaku_read::read_edition(hub)?)?;
    let mut spoke_hashes = Vec::new();
    for (i, spoke) in spokes.iter().enumerate() {
        eprintln!("spoke {i}: {}", label(spoke));
        spoke_hashes.push(fingerprint::fingerprint_edition(
            &taiyaku_read::read_edition(spoke)?,
        )?);
    }

    // The star: one alignment per spoke, and its inverse so a spoke can reach
    // the hub as well as be reached from it.
    let to_hub: Vec<Vec<Run>> = spoke_hashes.iter().map(|s| solve(s, &hub_hashes)).collect();
    let from_hub: Vec<Vec<Run>> = spoke_hashes.iter().map(|s| solve(&hub_hashes, s)).collect();
    eprintln!(
        "\nstar: {} alignments | all-pairs would need {}",
        to_hub.len() + from_hub.len(),
        spokes.len() * (spokes.len() - 1)
    );

    println!(
        "{:<9} {:>6} {:>7} {:>8} {:>9} {:>9} {:>21}",
        "pair", "pages", "direct", "loss", "disagree", "inverse", "verdict"
    );
    let (mut total_pages, mut total_loss, mut total_disagree, mut total_inverse) = (0, 0, 0, 0);
    // Composition can merge two fragmented runs into one clean one, so judging
    // the composed runs is not the same as taking the worse of the two legs.
    // `Sidecar::trust` takes the worse leg; this counts how often that is the
    // more pessimistic answer, which is what decides whether it should.
    let mut verdict_splits = 0;

    for i in 0..spokes.len() {
        for j in 0..spokes.len() {
            if i == j {
                continue;
            }
            let direct = solve(&spoke_hashes[i], &spoke_hashes[j]);
            let composed = compose(&to_hub[i], &from_hub[j]);

            let composed_verdict = taiyaku_core::verdict(&composed, spoke_hashes[i].len());
            let worse_leg = taiyaku_core::verdict(&to_hub[i], spoke_hashes[i].len())
                .min(taiyaku_core::verdict(&from_hub[j], hub_hashes.len()));
            if composed_verdict != worse_leg {
                verdict_splits += 1;
            }

            let (mut direct_paired, mut loss, mut disagree, mut inverse) = (0, 0, 0, 0);
            for page in 0..spoke_hashes[i].len() as u32 {
                let d = counterpart(&direct, page);
                let c = counterpart(&composed, page);
                if d.is_some() {
                    direct_paired += 1;
                }
                match (d, c) {
                    (Some(_), None) => loss += 1,
                    (None, Some(_)) => inverse += 1,
                    (Some(x), Some(y)) if x != y => disagree += 1,
                    _ => {}
                }
            }
            let pages = spoke_hashes[i].len();
            println!(
                "{i}->{j:<7} {pages:>6} {direct_paired:>7} {:>7.1}% {:>8.1}% {:>8.1}% {:>21}",
                100.0 * loss as f64 / pages as f64,
                100.0 * disagree as f64 / pages as f64,
                100.0 * inverse as f64 / pages as f64,
                match composed_verdict == worse_leg {
                    true => stats::verdict_word(composed_verdict).to_string(),
                    false => format!(
                        "{} != {}",
                        stats::verdict_word(composed_verdict),
                        stats::verdict_word(worse_leg)
                    ),
                }
            );
            total_pages += pages;
            total_loss += loss;
            total_disagree += disagree;
            total_inverse += inverse;
        }
    }

    eprintln!(
        "\nacross {total_pages} page comparisons: loss {:.2}% | disagreement {:.2}% | inverse loss {:.2}%",
        100.0 * total_loss as f64 / total_pages as f64,
        100.0 * total_disagree as f64 / total_pages as f64,
        100.0 * total_inverse as f64 / total_pages as f64,
    );
    eprintln!(
        "loss is the number that decides it: pages a direct solve pairs and composition cannot"
    );
    eprintln!(
        "composed verdict differs from the worse leg on {verdict_splits} of {} pairs; \
         at 0, `trust` may as well keep taking the worse leg",
        spokes.len() * (spokes.len() - 1)
    );
    Ok(())
}

/// Express `target` relative to `base`, for writing into a sidecar.
///
/// The format says a member's path is relative to the sidecar, so what a
/// caller typed — which is relative to wherever they happened to be — cannot
/// be written down as-is. Both sides are canonicalised first so that `..`,
/// symlinks and differing spellings of the same directory do not produce a
/// relative path that walks somewhere else.
///
/// When either side cannot be canonicalised, returns an *absolute* path built
/// from `target` — never the raw argument. The raw argument is exactly the
/// working-directory-relative path this function exists to stop writing into
/// the sidecar, so returning it on the failure path would silently reopen the
/// bug the success path fixes. If even `std::path::absolute` fails — which
/// essentially means the working directory is unreadable — this errors rather
/// than falling back to it, because a sidecar that cannot be written is
/// recoverable and one that records unresolvable paths is not.
fn relative_to(target: &Path, base: &Path) -> Result<PathBuf, String> {
    let (Ok(here), Ok(there)) = (target.canonicalize(), base.canonicalize()) else {
        // Absolute, not raw. An absolute path is still correct and merely
        // less portable; the path as the caller typed it is relative to
        // their working directory, which is the very thing the sidecar must
        // not record. A relative path computed against a base that could not
        // be resolved would be worse than either — comparing a relative
        // target's components against an absolute base's would share no
        // prefix, producing a plausible-looking path that points elsewhere.
        return std::path::absolute(target).map_err(|e| {
            format!(
                "{}: cannot be made absolute, so it cannot be recorded in a sidecar: {e}",
                target.display()
            )
        });
    };

    let mut target_parts = here.components().peekable();
    let mut base_parts = there.components().peekable();
    while target_parts.peek().is_some() && target_parts.peek() == base_parts.peek() {
        target_parts.next();
        base_parts.next();
    }

    let mut relative = PathBuf::new();
    for _ in base_parts {
        relative.push("..");
    }
    for component in target_parts {
        relative.push(component);
    }
    Ok(match relative.as_os_str().is_empty() {
        true => PathBuf::from("."),
        false => relative,
    })
}

/// The directory a sidecar written to `out` lives in, so member paths can be
/// recorded relative to it.
///
/// `Path::new("taiyaku.json").parent()` returns `Some("")` for a bare
/// filename with no directory component, rather than `None` — and computing
/// member paths against an empty base would be silently wrong. Normalised to
/// `.` instead.
fn sidecar_dir(out: &Path) -> &Path {
    out.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// Write a sidecar back, replacing the file rather than truncating it.
///
/// **The one writer both `align --write` (O26) and `credits --write` go
/// through**, which is why it sits here rather than in either command's module.
///
/// Through `taiyaku_serve::atomic::write`, never `fs::write`. `fs::write` opens
/// for truncation and then writes, so a crash or a full disk inside that window
/// leaves a short file where a whole sidecar was — and this is the worst file in
/// the project to lose half of. It holds *every* edition of the work, every
/// alignment between them, and every run a reader hand-corrected through the
/// reading UI. `credits --write` changes one list on one edition and `align
/// --write` adds or replaces one spoke; either would take all the rest down with
/// it. A write-and-rename either publishes the whole new document or leaves the
/// whole old one.
///
/// `atomic::write` names the destination — the path spelled right here, as the
/// caller typed it — in both of its failures, so there is nothing to add. That
/// is O29, closed: the staging branch used to answer with the temporary it
/// invents, which is not a file anyone asked for and is deleted before the
/// error is returned.
///
/// A named function rather than a statement inside either command, because a
/// test can call one and cannot reach the other: `align` solves and `credits`
/// fingerprints before either reaches the write, so driving it through a command
/// needs real archives with real pages in them, and the property is about the
/// write alone. Two tests exercise this one function over the two documents the
/// two commands actually put at risk —
/// `tests::writing_an_alignment_replaces_the_sidecar_rather_than_truncating_it`
/// on a three-edition star carrying a hand correction, and
/// `credits::tests::writing_a_credits_list_replaces_the_sidecar_rather_than_truncating_it`
/// on the pair `credits` rewrites. Neither proves its command still *calls*
/// this; that was driven against the corpus instead, and `notes.md` records it.
fn write_sidecar(out: &str, sidecar: &Sidecar) -> Result<(), String> {
    taiyaku_serve::atomic::write(Path::new(out), sidecar.to_json().as_bytes(), "json")
}

/// Describe one edition for the sidecar: its members, their content hashes,
/// and which of its pages are joined spreads.
///
/// `beside` is the directory the sidecar will be written to, because the format
/// records a member's path relative to the sidecar rather than to wherever the
/// caller happened to be standing.
fn describe_edition(
    label: &str,
    members: &[PathBuf],
    slots: &[Slot],
    beside: &Path,
) -> Result<Edition, String> {
    let mut described = Vec::new();
    for member in members {
        described.push(Member {
            path: relative_to(member, beside)?.to_string_lossy().to_string(),
            sha256: taiyaku_read::member_digest(member)?,
            pages: taiyaku_read::member_pages(member)?,
        });
    }
    // A page appears twice in the slot table exactly when it is a spread.
    let mut spreads: Vec<u32> = slots
        .iter()
        .filter(|s| s.half.is_some())
        .map(|s| s.page)
        .collect();
    spreads.dedup();
    Ok(Edition {
        label: label.to_string(),
        members: described,
        spreads,
        credits: Vec::new(),
    })
}

/// Resolve pages through a stored sidecar, with no archives and no solving.
///
/// This exists to prove the format carries what the solver found: the answers
/// here must match what `align` printed, from the file alone.
fn lookup(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("lookup", args, &[])?;
    let (path, from, to) = match args {
        [p, f, t, ..] => (p, f, t),
        _ => return Err("usage: taiyaku lookup <taiyaku.json> <from> <to> <page>...".into()),
    };
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let sidecar = Sidecar::from_json(&text)?;
    eprintln!(
        "{} | {} editions | reading {:?}",
        sidecar.work.title,
        sidecar.editions.len(),
        sidecar.work.reading
    );
    // Printed on every call rather than only on a bad one. A warning that
    // appears sometimes teaches people to skip it; a line that is always there
    // is read as part of the answer. Measured across twenty-two needles, seven
    // pair every page they have while being wholly unrelated, and the run
    // shape is the only thing that says so.
    //
    // The line describes the path this call will actually walk. `route` and
    // `trust` now agree about which paths exist, so the old "not by a path this
    // lookup can walk" case is gone — but a composed answer is weaker than a
    // direct one, measured at about 1.7% of pages lost to hub gaps, and the
    // line has to say which it is.
    let route = taiyaku_core::route(&sidecar, from, to);
    match &route {
        Some(found) => match &found.via {
            Via::Identity => eprintln!("  alignment {from} -> {to}: identical edition"),
            via => {
                let verdict = match via {
                    // Composition is only as good as its weaker leg, which is
                    // what `trust` reports. The run length is of the composed
                    // runs, because those are what this call walks.
                    Via::Hub(_) => sidecar.trust(from, to),
                    _ => sidecar.quality_of(from, to).map(|q| q.verdict),
                };
                let how = match via {
                    Via::Inverted => format!(", from the stored {to} -> {from} edge"),
                    Via::Hub(hub) => format!(", composed through {hub}"),
                    _ => String::new(),
                };
                match verdict {
                    Some(v) => eprintln!(
                        "  alignment {from} -> {to}: {}, average run {:.1}{how}",
                        stats::verdict_word(v),
                        taiyaku_core::average_run_length(found.runs())
                    ),
                    None => {
                        eprintln!("  alignment {from} -> {to}: this document does not say{how}")
                    }
                }
            }
        },
        // Nothing to say about the pairing — but if an edge is there and
        // unreadable, `route` swallowed the reason. Those messages are written
        // for whoever is hand-editing the file, and this is the only place they
        // can reach them.
        None => match sidecar
            .alignments
            .iter()
            .find(|a| (a.from == *from && a.to == *to) || (a.from == *to && a.to == *from))
            .and_then(|a| sidecar.runs_of(a).err())
        {
            Some(why) => eprintln!("  alignment {from} -> {to}: cannot be read: {why}"),
            None => eprintln!("  alignment {from} -> {to}: this document does not say"),
        },
    }
    for page in &args[3..] {
        let page: u32 = page.parse().map_err(|_| format!("not a page: {page}"))?;
        // Resolved through the route bound above rather than by calling
        // `resolve` per page, which would rebuild the composition every time.
        match route.as_ref().and_then(|found| found.resolve(page)) {
            Some(slot) => println!(
                "  {from} {page:>4} -> {to} {:>4}{}",
                slot.page,
                match slot.half {
                    Some(Half::Right) => " right half",
                    Some(Half::Left) => " left half",
                    None => "",
                }
            ),
            None => println!("  {from} {page:>4} -> no counterpart"),
        }
    }
    Ok(())
}

/// Serve a reader the sidecars it was handed.
///
/// Sidecar paths are arguments rather than discovered, the same way `align`
/// takes members rather than scanning: deciding which files constitute a
/// library is a different job, and doing it badly here would be worse than not
/// doing it at all.
fn serve(args: &[String]) -> Result<(), String> {
    reject_unknown_flags(
        "serve",
        args,
        &[
            "--addr",
            "--log-requests",
            "--refresh-stump",
            "--stump-key",
            "--stump-url",
        ],
    )?;
    let paths: Vec<PathBuf> = args
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if paths.is_empty() {
        return Err("needs at least one taiyaku.json to serve".into());
    }
    let addr = option_value(args, "--addr", "a host:port")?.unwrap_or("127.0.0.1:8484");

    // A key on the command line is visible in `ps`, so the environment is the
    // documented way and the flag is the override.
    let key = option_value(args, "--stump-key", "an API key")?
        .map(str::to_string)
        .or_else(|| std::env::var("TAIYAKU_STUMP_KEY").ok());
    let base = option_value(args, "--stump-url", "a URL")?
        .map(str::to_string)
        .or_else(|| std::env::var("TAIYAKU_STUMP_URL").ok());
    let stump = match (base, key) {
        (Some(base), Some(key)) => Some(taiyaku_serve::StumpConfig::new(base, key)),
        (Some(_), None) => {
            return Err("--stump-url needs a key: --stump-key, or TAIYAKU_STUMP_KEY".into());
        }
        (None, Some(_)) => {
            return Err("a stump key was given with no --stump-url or TAIYAKU_STUMP_URL".into());
        }
        (None, None) => None,
    };
    let log = option_value(args, "--log-requests", "a path")?.map(PathBuf::from);
    // Throws away every work's remembered Stump resolutions before this start
    // resolves them again. The escape hatch for a book added to Stump after a
    // work first resolved, which nothing here notices on its own.
    let refresh_stump = args.iter().any(|a| a == "--refresh-stump");

    let works = taiyaku_serve::load_works(&paths)?;
    for work in &works {
        let sidecar = work.sidecar();
        eprintln!(
            "{} | {} editions | {}",
            work.id,
            sidecar.editions.len(),
            sidecar.work.title
        );
    }
    tokio::runtime::Runtime::new()
        .map_err(|e| format!("{e}"))?
        .block_on(taiyaku_serve::serve(works, addr, stump, log, refresh_stump))
}

/// Fingerprint an edition through the cache, splitting joined spreads.
///
/// Returns the hashes, the slot table, and how many members were served from
/// cache — worth reporting, because a cold run and a warm one differ by three
/// orders of magnitude and it would be misleading to imply otherwise.
fn cached_edition(
    root: &std::path::Path,
    members: &[PathBuf],
    direction: Direction,
) -> Result<(Vec<taiyaku_core::PageHash>, Vec<Slot>, usize), String> {
    let mut described = Vec::new();
    let mut hits = 0;
    let mut ordered = members.to_vec();
    ordered.sort_by(|a, b| taiyaku_read::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    for member in &ordered {
        let (pages, hit) = cache::member_pages(root, member)?;
        hits += usize::from(hit);
        described.extend(pages);
    }

    // Whether a page is a spread is decided against the edition it sits in, so
    // it cannot be cached per member: the baseline has to be taken over the
    // whole flattened sequence.
    let dimensions: Vec<(u32, u32)> = described.iter().map(|p| (p.width, p.height)).collect();
    let flags = taiyaku_core::spread_flags(&dimensions);

    let mut hashes = Vec::new();
    let mut wide = Vec::new();
    for (page, spread) in described.iter().zip(&flags) {
        match (*spread, page.halves) {
            (true, Some((left, right))) => {
                // Reading order, matching the slot table.
                match direction {
                    Direction::RightToLeft => hashes.extend([right, left]),
                    Direction::LeftToRight => hashes.extend([left, right]),
                }
                wide.push(true);
            }
            _ => {
                hashes.push(page.hash);
                wide.push(false);
            }
        }
    }
    Ok((hashes, taiyaku_core::expand(&wide, direction), hits))
}

fn align(args: &[String]) -> Result<(), String> {
    // Before `reject_unknown_flags`, which would otherwise call it a
    // misspelling. A flag that was documented yesterday is not a typo, and the
    // two failures want different answers: a typo wants the alphabet, a
    // retirement wants to know what replaced it. Accepting it as a no-op was
    // the other option and is the one this group has spent five items arguing
    // against — a flag that cannot change what a run does is refused, not
    // ignored.
    if args.iter().any(|a| a == "--split-spreads") {
        return Err(
            "--split-spreads is retired: splitting joined spreads is now what \
                    align does by default, so the flag had nothing left to turn on; \
                    pass --no-split-spreads for the page-level diagnostic"
                .into(),
        );
    }
    reject_unknown_flags(
        "align",
        args,
        &[
            "--cache",
            "--direct",
            "--ltr",
            "--name-a",
            "--name-b",
            "--no-split-spreads",
            "--stats",
            "--title",
            "--write",
        ],
    )?;
    let (a_members, b_members) = split_editions(args)?;
    // Splitting joined spreads is the default. `corpus/README.md` has said to
    // pass it "essentially always" since before there was a second caller —
    // unhandled spreads make a correct alignment look broken, which cost
    // several hours once — so the flag spent its whole life being the answer
    // to a question nobody wanted asked. O19 and O20 then made the default
    // path the one that can do almost nothing: it cannot write a sidecar, and
    // it refuses six of the nine flags. A default nobody should choose is the
    // same silence this group has been closing, one level up.
    let split_spreads = !args.iter().any(|a| a == "--no-split-spreads");
    let show_stats = args.iter().any(|a| a == "--stats");
    let direct = args.iter().any(|a| a == "--direct");

    // Every valued flag is resolved here, before either edition is read, and
    // not merely before the sidecar is written: a flag typed wrong is a typo,
    // not a reason to spend seconds fingerprinting pages before saying so. The
    // key checks were already here on that argument; the rest joined them when
    // O15 made a missing value a refusal, because a refusal that arrives after
    // the fingerprinting has run is most of the cost of the mistake anyway.
    //
    // Reading each of these once also removes the second lookup `--name-a`,
    // `--name-b` and `--title` each used to do further down, where a flag
    // could in principle have been resolved twice and answered differently.
    let name_a = option_value(args, "--name-a", "an edition key")?;
    let name_b = option_value(args, "--name-b", "an edition key")?;
    let write = option_value(args, "--write", "a path")?;
    let title = option_value(args, "--title", "a title")?;
    let cache_root = std::path::PathBuf::from(
        option_value(args, "--cache", "a directory")?.unwrap_or(".taiyaku-cache"),
    );
    if let Some(name) = name_a {
        validate_key(name)?;
    }
    if let Some(name) = name_b {
        validate_key(name)?;
    }
    // O19. `--write` is only reachable from the split-spreads path below, and
    // that is not an oversight to wire around: the plain path has pages where
    // the sidecar wants slots. `describe_edition` derives an edition's
    // `spreads` list from its slot table — a page is a spread exactly when it
    // appears in that table twice — so a plain-path sidecar could only ever
    // claim `"spreads": []`, having never looked. That claim is not cosmetic.
    // `sidecar.md` makes the slot sequence follow from `spreads`, so an
    // edition that does join spreads would have every run after the first one
    // silently re-indexed, which is the shape of the two failures this
    // repository already records: verify's "192 of 187 pages paired, at
    // 102.7%", and "entirely an artifact of joined spreads having no
    // representation". Refusing costs a documented option pairing; the
    // alternative writes an unverified claim into the normative format.
    if write.is_some() && !split_spreads {
        return Err(
            "--write cannot be used with --no-split-spreads: a sidecar records \
                    which pages are joined spreads, and that path does not detect them"
                .into(),
        );
    }
    // O20, the same rule as O19 applied to the flags it left behind: a flag
    // that cannot change what this run does is refused rather than ignored.
    //
    // These four are read only inside the `--write` block, so they are dead on
    // *both* paths when nothing is written — `--split-spreads` alone skips that
    // block exactly as the plain path does, which is what the framing "dead on
    // the plain path" got wrong. One message for all four, because the reason
    // is identical and naming it four ways would suggest four rules.
    //
    // `validate_key` above still runs first, so a malformed `--name-a` reports
    // its own shape rather than this: the value is wrong whatever the run then
    // does with it, and that is the more specific complaint.
    if write.is_none() {
        for (flag, given) in [
            ("--title", title.is_some()),
            ("--name-a", name_a.is_some()),
            ("--name-b", name_b.is_some()),
            ("--direct", direct),
        ] {
            if given {
                return Err(format!(
                    "{flag} needs --write: it describes a sidecar, and this run writes none"
                ));
            }
        }
    }
    // The other two are live on the default path and inert on the page-level
    // one, so they name `--no-split-spreads` rather than `--write`. Each says
    // what that path does instead, because naming the flag alone reads as
    // arbitrary.
    if !split_spreads {
        if args.iter().any(|a| a == "--cache") {
            return Err("--cache cannot be used with --no-split-spreads: that path \
                        fingerprints directly and reads nothing through the cache"
                .into());
        }
        if args.iter().any(|a| a == "--ltr") {
            return Err(
                "--ltr cannot be used with --no-split-spreads: that path pairs pages \
                        rather than slots, and a page pairing has no direction to read"
                    .into(),
            );
        }
    }

    if !split_spreads {
        let a_pages = taiyaku_read::read_edition(&a_members)?;
        let b_pages = taiyaku_read::read_edition(&b_members)?;
        let a = fingerprint::fingerprint_edition(&a_pages)?;
        let b = fingerprint::fingerprint_edition(&b_pages)?;
        eprintln!("A: {} pages | B: {} pages", a.len(), b.len());
        let runs = solve(&a, &b);
        if show_stats {
            let cal = taiyaku_core::calibrate(&a, &b);
            eprintln!(
                "{}",
                stats::calibration_line(cal.as_ref(), a.len(), b.len())
            );
            eprintln!("{}", stats::shape_line(&runs, a.len()));
        }
        report_summary(&runs, a.len());
        return Ok(());
    }

    // Prototype: cut joined spreads in half on BOTH sides before solving.
    // Which edition joins a spread is not fixed - raws split them and English
    // releases join them here, but the colour editions join them too, so the
    // same pair read the other way round puts the spreads on the other side.
    let direction = if args.iter().any(|a| a == "--ltr") {
        Direction::LeftToRight
    } else {
        Direction::RightToLeft
    };
    let started = std::time::Instant::now();
    let (a_slots_hashes, a_slots, a_hits) = cached_edition(&cache_root, &a_members, direction)?;
    let (b, b_slots, b_hits) = cached_edition(&cache_root, &b_members, direction)?;
    eprintln!(
        "fingerprints in {:?} | {} of {} members from cache",
        started.elapsed(),
        a_hits + b_hits,
        a_members.len() + b_members.len()
    );
    let a_wide = a_slots.iter().filter(|s| s.half.is_some()).count() / 2;
    let b_wide = b_slots.iter().filter(|s| s.half.is_some()).count() / 2;
    eprintln!(
        "A: {} slots ({a_wide} spreads) | B: {} slots ({b_wide} spreads)",
        a_slots_hashes.len(),
        b.len()
    );

    let runs = solve(&a_slots_hashes, &b);
    if show_stats {
        let cal = taiyaku_core::calibrate(&a_slots_hashes, &b);
        eprintln!(
            "{}",
            stats::calibration_line(cal.as_ref(), a_slots_hashes.len(), b.len())
        );
        eprintln!("{}", stats::shape_line(&runs, a_slots_hashes.len()));
    }
    let mut whole = 0u32;
    let mut halves = 0u32;
    let mut gaps = 0u32;
    for (i, from) in a_slots.iter().enumerate() {
        match counterpart(&runs, i as u32).and_then(|slot| b_slots.get(slot as usize)) {
            Some(to) => {
                if from.half.is_some() || to.half.is_some() {
                    halves += 1;
                } else {
                    whole += 1;
                }
                println!(
                    "{:>4}{:<6} -> {:>4}{}",
                    from.page,
                    from.half.map(|h| format!(" {h:?}")).unwrap_or_default(),
                    to.page,
                    to.half.map(|h| format!(" {h:?}")).unwrap_or_default()
                );
            }
            None => gaps += 1,
        }
    }
    // Runs in slot notation, which is what the sidecar stores.
    let show = |slots: &[taiyaku_core::Slot], i: u32| -> String {
        match slots.get(i as usize) {
            Some(s) => match s.half {
                Some(Half::Left) => format!("s{i}={}L", s.page),
                Some(Half::Right) => format!("s{i}={}R", s.page),
                None => format!("s{i}={}", s.page),
            },
            None => "?".into(),
        }
    };
    let span = |slots: &[taiyaku_core::Slot], from: u32, to: u32| {
        format!("{}-{}", show(slots, from), show(slots, to))
    };
    eprintln!("runs in slot notation:");
    for run in &runs {
        let a_span = span(&a_slots, run.a.start, run.a.end);
        match run.b {
            Some(b) => eprintln!("  {a_span:>22} -> {}", span(&b_slots, b.start, b.end)),
            None => eprintln!("  {a_span:>22} -> gap"),
        }
    }
    eprintln!(
        "{} runs | {whole} whole pairings | {halves} involving a spread half | {gaps} unmatched slots",
        runs.len()
    );
    // The verdict was previously printed only in the plain path, so the
    // documented way of running the tool - `--split-spreads`, which
    // corpus/README.md says to pass essentially always - never showed it. An
    // art book paired page-for-page against a volume looked like a clean
    // result.
    eprintln!("{}", stats::verdict_sentence(&runs, a_slots.len()));

    if let Some(out) = write {
        // Read before writing. `--write` used to be a bare `fs::write`, which
        // silently discarded a document that may hold four other editions.
        let existing = match std::fs::read_to_string(out) {
            Ok(text) => Some(Sidecar::from_json(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("{out}: {e}")),
        };

        // On a merge the a-side is the edition the document already names, so
        // `--name-a` is optional and, given, must agree. `--direct` has no hub
        // to fall back on and so requires it.
        let a_name = match (&existing, name_a) {
            (Some(_), None) if direct => {
                return Err(
                    "--direct needs --name-a to say which edition the edge starts from".into(),
                );
            }
            (Some(sidecar), None) => sidecar.hub.clone(),
            (Some(_), Some(given)) => given.to_string(),
            (None, given) => slug(&a_members, given),
        };
        let b_name = slug(&b_members, name_b);
        if a_name == b_name {
            return Err(format!(
                "both editions would be named `{a_name}`; pass --name-a and --name-b"
            ));
        }

        // Member paths are recorded relative to the sidecar, so the writer has
        // to know where the sidecar is going before it can describe an edition.
        let beside = sidecar_dir(std::path::Path::new(out));
        let a_edition = describe_edition(&label(&a_members), &a_members, &a_slots, beside)?;
        let b_edition = describe_edition(&label(&b_members), &b_members, &b_slots, beside)?;
        let alignment = Alignment {
            from: a_name.clone(),
            to: b_name.clone(),
            by: Source::Solver,
            // The floor the solver actually used, not a placeholder: it is
            // calibrated from these two editions and a re-solve needs it to be
            // reproducible.
            // Only a calibration the solver accepted, so a refused pair records
            // no solver block rather than a floor that was never used.
            // `calibrate` reports its figures either way; `accepted` is what
            // decides whether they drove an alignment.
            solver: taiyaku_core::calibrate(&a_slots_hashes, &b)
                .filter(taiyaku_core::Calibration::accepted)
                .map(|cal| SolverInfo {
                    hash: "dhash-16".to_string(),
                    floor: cal.floor,
                }),
            // Computed here rather than left for a reader to derive: the whole
            // point of the document is that a reader should not need the
            // archives. `Sidecar::quality_of` recomputes it anyway, so this is
            // a summary for anything not using this library.
            quality: Some(taiyaku_core::Quality {
                verdict: taiyaku_core::verdict(&runs, a_slots.len()),
                slots: a_slots.len() as u32,
                paired: taiyaku_core::paired_slots(&runs),
                avg_run: taiyaku_core::average_run_length(&runs),
            }),
            runs: {
                // The weakest per-page margin inside each run, which is what
                // the verification report shades on. Storing it means a reader
                // can show confidence without the archives.
                let scored = confidence(&a_slots_hashes, &b, &runs);
                let mut entries = entries_from_runs(&runs, &a_slots, &b_slots)?;
                for (entry, run) in entries.iter_mut().zip(&runs) {
                    if entry.b.is_none() {
                        continue;
                    }
                    entry.margin = scored
                        .iter()
                        .filter(|s| run.a.contains(s.page))
                        .map(|s| s.margin)
                        .min_by(f32::total_cmp);
                }
                entries
            },
        };

        let (sidecar, note) = match existing {
            Some(mut sidecar) => {
                if let Some(title) = title
                    && title != sidecar.work.title
                {
                    return Err(format!(
                        "{out} is the sidecar for `{}`; --title cannot rename a work on a merge",
                        sidecar.work.title
                    ));
                }
                let anchor = match direct {
                    true => Anchor::Direct,
                    false => Anchor::Hub,
                };
                let merged = sidecar
                    .merge_spoke(direction, anchor, &a_edition, &b_name, b_edition, alignment)?;
                let verb = match merged {
                    Merged::Added => "added".to_string(),
                    Merged::Replaced => "replaced".to_string(),
                    Merged::CarriedHandRuns(n) => format!("replaced, carrying {n} hand run(s)"),
                    Merged::DroppedCredits(n) => {
                        format!("replaced, dropping {n} credits page(s)")
                    }
                };
                let note = format!(
                    "merged {out}: {verb} `{b_name}` | {} editions, {} alignments",
                    sidecar.editions.len(),
                    sidecar.alignments.len()
                );
                (sidecar, note)
            }
            None => (
                Sidecar {
                    taiyaku: taiyaku_core::VERSION,
                    work: Work {
                        title: title.unwrap_or("Untitled work").to_string(),
                        reading: direction,
                    },
                    editions: [(a_name.clone(), a_edition), (b_name.clone(), b_edition)]
                        .into_iter()
                        .collect(),
                    // A two-edition sidecar is a degenerate star: A is the
                    // reference simply because it is the only thing B is
                    // aligned to. A third edition makes it a real one.
                    hub: a_name.clone(),
                    alignments: vec![alignment],
                },
                format!("wrote {out}"),
            ),
        };
        write_sidecar(out, &sidecar)?;
        eprintln!("{note}");
    }
    Ok(())
}

fn report_summary(runs: &[Run], a_len: usize) {
    let mut paired = 0u32;
    for run in runs {
        match run.b {
            Some(b) => {
                paired += run.a.len();
                println!(
                    "{:>4}-{:<4} -> {:>4}-{:<4}  ({} pages, offset {:+})",
                    run.a.start,
                    run.a.end,
                    b.start,
                    b.end,
                    run.a.len(),
                    b.start as i64 - run.a.start as i64
                );
            }
            None => println!(
                "{:>4}-{:<4} -> {:^11}  ({} pages)",
                run.a.start,
                run.a.end,
                "gap",
                run.a.len()
            ),
        }
    }
    // Fragmentation, not coverage, is what tells a real alignment from a
    // plausible-looking one. A correct pairing is a few long runs; a spurious
    // one is confetti at roughly the same coverage. Runs per hundred pages is
    // kept here as a diagnostic to show the user, not as the thing judged on:
    // the 18-row survey in docs/notes.md ("The margin survey, and where the
    // refusal threshold sits") retired it in favour of average run length,
    // which `verdict_sentence` reports.
    let per_hundred = 100.0 * runs.len() as f64 / a_len.max(1) as f64;
    // The verdict is printed alongside the number because the number alone is
    // misleading: a refusal is one run, which reads as better than any correct
    // alignment on a runs-per-page scale.
    eprintln!(
        "{} runs | {paired}/{a_len} pages paired ({:.1}%) | {per_hundred:.1} runs per 100 pages | {}",
        runs.len(),
        100.0 * paired as f64 / a_len.max(1) as f64,
        stats::verdict_sentence(runs, a_len)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shell glob is not in reading order, and the sidecar used to record it
    /// verbatim while the solver quietly worked in natural order — so a
    /// document's member list disagreed with the page sequence its own runs
    /// addressed. Nothing in this repository reads member order, which is why
    /// it went unnoticed; a reader that concatenates `members` to rebuild the
    /// sequence would show the wrong page for every lookup.
    ///
    /// Kaguya's fan translation is the reproduction: `*.cbz` yields
    /// `c10.5, c10, c1, c2, …` and the solve used `c1, c2, … c10, c10.5`.
    /// Ordering here rather than in each consumer is what makes them agree —
    /// `slug` and `label` name an edition after its first member, and they
    /// were reading the glob's first member too.
    #[test]
    fn both_editions_come_back_in_natural_order() {
        let args: Vec<String> = [
            "v02",
            "v01",
            "--",
            "c10.5.cbz",
            "c10.cbz",
            "c1.cbz",
            "c2.cbz",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let (a, b) = split_editions(&args).unwrap();

        assert_eq!(a, ["v01", "v02"].map(PathBuf::from));
        assert_eq!(
            b,
            ["c1.cbz", "c2.cbz", "c10.cbz", "c10.5.cbz"].map(PathBuf::from)
        );
    }

    /// `docs/sidecar.md` says a member's `path` is relative to the sidecar, and
    /// `LocalLibrary` resolves it that way. `describe_edition` wrote the path as
    /// typed on the command line, which is relative to the working directory —
    /// so a sidecar written from one directory and stored in another named
    /// members that were not there, and every page 404'd.
    ///
    /// The same class as the member-order bug: the writer disagreeing with a
    /// rule the format states, invisible until something resolved it.
    #[test]
    fn a_member_path_is_recorded_relative_to_the_sidecar() {
        let root = std::env::temp_dir().join("taiyaku-test-relative");
        let _ = std::fs::remove_dir_all(&root);
        let library = root.join("library");
        let beside = root.join("sidecars");
        std::fs::create_dir_all(&library).unwrap();
        std::fs::create_dir_all(&beside).unwrap();
        let member = library.join("v01.cbz");
        std::fs::write(&member, b"placeholder").unwrap();

        let recorded = relative_to(&member, &beside).unwrap();

        let resolved = beside.join(&recorded);
        let ok = resolved.canonicalize().ok() == member.canonicalize().ok();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(recorded, PathBuf::from("../library/v01.cbz"));
        assert!(ok, "{} should resolve to the member", recorded.display());
    }

    /// The sibling case above is not the common one. The normal layout is a
    /// sidecar sitting beside or inside the corpus it describes — `taiyaku.json`
    /// at the root, members underneath it — which the sibling test does not
    /// exercise at all. A regression that broke only this case would pass the
    /// existing suite.
    #[test]
    fn a_member_inside_the_base_directory_is_recorded_relative_to_it() {
        let root = std::env::temp_dir().join("taiyaku-test-relative-nested");
        let _ = std::fs::remove_dir_all(&root);
        let raw_mono = root.join("raw-mono");
        std::fs::create_dir_all(&raw_mono).unwrap();
        let member = raw_mono.join("v01.cbz");
        std::fs::write(&member, b"placeholder").unwrap();
        // Sidecar at <root>/taiyaku.json, so its directory is <root> itself.
        let beside = root.clone();

        let recorded = relative_to(&member, &beside).unwrap();

        let resolved = beside.join(&recorded);
        let ok = resolved.canonicalize().ok() == member.canonicalize().ok();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(recorded, PathBuf::from("raw-mono/v01.cbz"));
        assert!(ok, "{} should resolve to the member", recorded.display());
    }

    /// A `let-else` pattern's bindings are not in scope inside its `else`
    /// block, so `let (Ok(target), Ok(base)) = (...) else { return
    /// target... }` returns the *parameter*, not a canonicalised path. In this
    /// call chain that parameter is exactly the working-directory-relative
    /// path this function exists to stop writing into the sidecar — the
    /// fallback was reintroducing the bug it exists to prevent.
    ///
    /// Constructed the way the review asked: a base that cannot be resolved (a
    /// sidecar directory that does not exist) and a relative target, the shape
    /// a caller's argument actually has. Before the fix this returned
    /// `target` completely unchanged, `relative/thing.cbz` — not absolute, not
    /// even looked at. After the fix it returns an absolute path.
    ///
    /// Pinned against `current_dir().join(target)` rather than only asserting
    /// `is_absolute()`: the weaker assertion passes for any absolute path at
    /// all, including one built from `base`, so it would not have noticed the
    /// two being swapped. What has to hold is that the recorded path still
    /// names the *member*.
    #[test]
    fn relative_to_returns_an_absolute_path_when_the_base_cannot_be_resolved() {
        let target = Path::new("relative/thing.cbz");
        let missing_base = std::env::temp_dir().join("taiyaku-test-relative-missing-base");
        let _ = std::fs::remove_dir_all(&missing_base);

        let recorded = relative_to(target, &missing_base).unwrap();

        assert!(
            recorded.is_absolute(),
            "expected an absolute path when the base cannot be resolved, got {}",
            recorded.display()
        );
        assert_eq!(
            recorded,
            std::env::current_dir().unwrap().join(target),
            "must be the member made absolute, not the base and not the raw argument"
        );
        assert!(
            recorded.ends_with("relative/thing.cbz"),
            "must still name the member: {}",
            recorded.display()
        );
    }

    /// `Path::new("taiyaku.json").parent()` returns `Some("")`, not `None`,
    /// for a bare filename — the guard at the `--write` call site exists
    /// because computing member paths against an empty base would be silently
    /// wrong. Pinned directly rather than only through the inline expression
    /// it used to be.
    #[test]
    fn sidecar_dir_falls_back_to_the_current_directory_for_a_bare_filename() {
        assert_eq!(sidecar_dir(Path::new("taiyaku.json")), Path::new("."));
    }

    #[test]
    fn sidecar_dir_is_the_parent_of_a_pathed_filename() {
        assert_eq!(sidecar_dir(Path::new("out/taiyaku.json")), Path::new("out"));
    }

    /// A star with a hand-corrected run in it: what `align --write` rewrites
    /// wholesale in order to add or replace one spoke.
    fn three_editions_one_hand_corrected() -> Sidecar {
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Reversed Order Co.", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 8 }],
              "spreads": []
            },
            "en-fan": {
              "label": "en-fan",
              "members": [{ "path": "f.cbz", "sha256": "f0", "pages": 8 }],
              "spreads": []
            },
            "en-mono": {
              "label": "en-mono",
              "members": [{ "path": "m.cbz", "sha256": "m0", "pages": 8 }],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en-fan", "by": "solver",
              "runs": [{"a":"0-3","b":"0-3"},{"a":"4-7","b":"4-7","by":"hand"}] },
            { "from": "raw", "to": "en-mono", "by": "solver",
              "runs": [{"a":"0-7","b":"0-7"}] }
          ]
        }"#;
        Sidecar::from_json(text).expect("fixture parses")
    }

    /// O26: `align --write` must not be able to leave a short sidecar behind.
    ///
    /// The property O2 established for progress and `atomic.rs` now owns for
    /// everything that writes beside a sidecar: the destination is *replaced*
    /// by a rename, never opened for truncation. A crash cannot be staged in a
    /// test, so this pins the mechanism that rules it out — the same shape
    /// `credits::tests::writing_a_credits_list_replaces_the_sidecar_rather_than_truncating_it`
    /// set for the command next door.
    ///
    /// Observed by **inode identity** rather than by taking write permission
    /// off the existing file. `atomic.rs`'s own test records why: on a
    /// filesystem that ignores the read-only bit, or under root, the
    /// permission-denied write that approach distinguishes "replaced" from
    /// "truncated" by simply succeeds, and the skip line it prints is captured
    /// by libtest — a green that measured nothing. A rename gives the path a
    /// different inode; an in-place write keeps the same one, whoever is
    /// running and whatever the filesystem enforces.
    ///
    /// The fixture is a **three-edition star carrying a hand correction**,
    /// because that is what is actually at stake here and it is more than
    /// `credits` risks: `align --write` rewrites the whole document to add or
    /// replace *one* spoke, so a half-written file takes the other editions,
    /// the edges joining them, and every run a reader corrected through the
    /// reading UI with it.
    #[test]
    fn writing_an_alignment_replaces_the_sidecar_rather_than_truncating_it() {
        let dir = std::env::temp_dir().join("taiyaku-test-align-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("taiyaku.json");
        let out = path.to_str().expect("utf-8 temp path").to_string();

        let doc = three_editions_one_hand_corrected();
        std::fs::write(&path, doc.to_json()).expect("the fixture is written");

        #[cfg(unix)]
        let before = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&path).expect("stat").ino()
        };

        let wrote = write_sidecar(&out, &doc);

        let read_back = std::fs::read_to_string(&path).expect("the destination is readable");

        #[cfg(unix)]
        let after = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&path).expect("stat").ino()
        };

        // Nothing may be left lying beside the sidecar. A failed write and a
        // failed rename each clean up after themselves rather than leaving one
        // file per attempt in the reader's own library directory.
        let strays: Vec<String> = std::fs::read_dir(&dir)
            .expect("the temp dir is readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(".tmp"))
            .collect();

        let _ = std::fs::remove_dir_all(&dir);

        wrote.expect("the write succeeds");
        #[cfg(unix)]
        assert_ne!(
            before, after,
            "a rename replaces the destination with a different inode; an \
             in-place truncating write would have kept the same one"
        );
        assert!(strays.is_empty(), "temp files left behind: {strays:?}");

        // Whole, not half: everything the write never touched came through.
        let written =
            Sidecar::from_json(&read_back).expect("the destination holds a whole document");
        assert_eq!(
            written.editions.len(),
            3,
            "every edition must survive the rewrite"
        );
        assert_eq!(
            written.alignments.len(),
            2,
            "every edge must survive the rewrite"
        );
        assert!(
            written
                .alignments
                .iter()
                .flat_map(|edge| &edge.runs)
                .any(|run| run.by == Some(Source::Hand)),
            "the hand-corrected run must survive the rewrite"
        );
    }

    /// Important 1a: an edition key reaches two URLs —
    /// `/api/works/{work}/{edition}/pages/{n}` and the OPDS stream route —
    /// verbatim. `--name-a`/`--name-b` used to hand `slug` a caller's string
    /// unexamined, so `taiyaku align --name-a "raw color"` wrote a key
    /// containing a space, breaking both `href`s it went into. Pinned to the
    /// exact wording the review asked for: the key, the offending character,
    /// its position, and a suggestion.
    #[test]
    fn validate_key_names_the_character_position_and_a_suggestion() {
        let err = validate_key("raw color").unwrap_err();
        assert_eq!(
            err,
            "edition key `raw color` is not URL-safe (space at position 3); try `raw-color`"
        );
    }

    /// A `/` is worse than a space: it inserts a path segment the registered
    /// route cannot parse back at all, rather than merely mangling one.
    #[test]
    fn validate_key_names_a_non_space_offender_in_backticks() {
        let err = validate_key("raw/mono").unwrap_err();
        assert_eq!(
            err,
            "edition key `raw/mono` is not URL-safe (`/` at position 3); try `raw-mono`"
        );
    }

    /// The alphabet the auto-derived slug already produces — ASCII
    /// alphanumerics and `-` — has to be accepted, or a caller cannot ever
    /// pass `--name-a`/`--name-b` at all.
    #[test]
    fn validate_key_accepts_ascii_alphanumerics_and_hyphens() {
        assert!(validate_key("raw-mono-2").is_ok());
    }

    /// The one bad key the character scan cannot see, because it scans no
    /// characters and so finds nothing to object to. An empty key is not a
    /// usable path segment, which is the property this whole function is
    /// about, so it has to be refused by its own branch rather than falling
    /// through the loop as safe.
    #[test]
    fn validate_key_refuses_an_empty_key() {
        let err = validate_key("").unwrap_err();
        assert!(
            err.contains("empty"),
            "must say what is wrong with it: {err}"
        );
        assert!(
            !err.contains("position"),
            "there is no offending position to name: {err}"
        );
    }

    /// The end-to-end path: `align` itself refuses the bad key, and does so
    /// before either edition is read — these member paths do not exist on
    /// disk, and a version of this check that ran after fingerprinting would
    /// fail on a file-not-found instead of ever reaching the validation.
    #[test]
    fn align_refuses_a_name_a_key_that_is_not_url_safe_before_touching_any_archive() {
        let args: Vec<String> = [
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--name-a",
            "raw color",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let err = align(&args).unwrap_err();

        assert_eq!(
            err,
            "edition key `raw color` is not URL-safe (space at position 3); try `raw-color`"
        );
    }

    /// The same check on the other flag, so a bad `--name-b` cannot slip
    /// through because only `--name-a` was checked.
    #[test]
    fn align_refuses_a_name_b_key_that_is_not_url_safe_before_touching_any_archive() {
        let args: Vec<String> = [
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--name-b",
            "en/fan",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let err = align(&args).unwrap_err();

        assert_eq!(
            err,
            "edition key `en/fan` is not URL-safe (`/` at position 2); try `en-fan`"
        );
    }

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    /// Splitting joined spreads is what the corpus README has said to do
    /// "essentially always" since before the flag had a second user, so it is
    /// the default. The observable consequence at this level: `--write` alone
    /// no longer trips O19's refusal, because the run it names now detects
    /// spreads.
    #[test]
    fn writing_a_sidecar_no_longer_needs_a_flag_to_enable_it() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--write",
            "taiyaku.json",
        ]);

        let err = align(&args).unwrap_err();

        assert!(
            !err.contains("needs"),
            "should have got past argument handling, but refused: {err}"
        );
    }

    /// O19's rule survives the inversion — it now keys off the flag that asks
    /// for the page-level path rather than off the absence of the one that
    /// asked for the slot-level path. The reason is unchanged: that path
    /// cannot describe an edition's spreads, so it cannot write a sidecar.
    #[test]
    fn align_refuses_write_with_no_split_spreads() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--no-split-spreads",
            "--write",
            "taiyaku.json",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--write cannot be used with --no-split-spreads: a sidecar records \
             which pages are joined spreads, and that path does not detect them"
        );
    }

    /// The retired flag explains itself rather than being accepted as a no-op
    /// or reported as a typo. Accepting it would break the rule O19 and O20
    /// established — a flag that cannot change what a run does is refused —
    /// and `reject_unknown_flags` alone would call yesterday's documented flag
    /// a misspelling.
    #[test]
    fn the_retired_split_spreads_flag_says_what_replaced_it() {
        let args = argv(&["a.cbz", "--", "b.cbz", "--split-spreads"]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--split-spreads is retired: splitting joined spreads is now what \
             align does by default, so the flag had nothing left to turn on; \
             pass --no-split-spreads for the page-level diagnostic"
        );
    }

    /// O20's two split-path flags follow the same inversion.
    #[test]
    fn align_refuses_ltr_with_no_split_spreads() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--no-split-spreads",
            "--ltr",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--ltr cannot be used with --no-split-spreads: that path pairs pages \
             rather than slots, and a page pairing has no direction to read"
        );
    }

    /// The plain diagnostic is still reachable, and still takes `--stats`.
    #[test]
    fn the_page_level_diagnostic_is_still_reachable() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--no-split-spreads",
            "--stats",
        ]);

        let err = align(&args).unwrap_err();

        assert!(
            !err.contains("cannot be used") && !err.contains("no flag"),
            "the diagnostic path was refused: {err}"
        );
    }

    /// O21. O15, O19 and O20 all key off a flag's *name*, so none of them can
    /// see a name that was typed wrong — and a misspelling reproduces exactly
    /// the failure O19 closed: `--writ` is not `--write`, so the run reports a
    /// plausible alignment and writes nothing, at exit 0.
    #[test]
    fn a_misspelled_flag_is_refused_rather_than_ignored() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--writ",
            "taiyaku.json",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "align has no flag `--writ`; its flags are --cache, --direct, --ltr, \
             --name-a, --name-b, --no-split-spreads, --stats, --title, --write"
        );
    }

    /// The near-miss that motivates naming the alphabet in the message: this
    /// one differs from a real flag by a single character and produces a
    /// plausible-looking answer rather than an obvious failure.
    #[test]
    fn a_flag_missing_one_letter_is_refused() {
        let args = argv(&["a.cbz", "--", "b.cbz", "--split-spread"]);

        assert!(
            align(&args)
                .unwrap_err()
                .starts_with("align has no flag `--split-spread`;"),
            "expected an unknown-flag refusal naming the typo"
        );
    }

    /// Each command validates against its own set rather than a global one,
    /// so a flag that is real elsewhere is still refused here.
    #[test]
    fn a_flag_belonging_to_another_command_is_refused() {
        let args = argv(&["some-root", "--split-spreads"]);

        assert_eq!(
            inventory::command(&args).unwrap_err(),
            "inventory has no flag `--split-spreads`; its flags are --min-pages, --pattern"
        );
    }

    /// O22, O20's rule in the command O20 did not touch. Measured before the
    /// fix: `verify a -- b --cache DIR` never created DIR, because
    /// `cached_edition` is reached only from the `--sidecar` arm — without one
    /// `verify` solves the pair directly and the cache is never consulted.
    #[test]
    fn verify_refuses_a_cache_it_would_never_read() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--cache",
            "somewhere",
        ]);

        assert_eq!(
            verify(&args).unwrap_err(),
            "--cache needs --sidecar: without one verify solves the pair \
             directly and reads nothing through the cache"
        );
    }

    /// The same arm carries `direction`, so `--ltr` is dead in `verify` for
    /// the same reason it is dead on `align`'s plain path — and with a sidecar
    /// it is the document's own reading order that indexes the runs anyway.
    #[test]
    fn verify_refuses_ltr_without_a_sidecar_to_apply_it_to() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--ltr",
        ]);

        assert_eq!(
            verify(&args).unwrap_err(),
            "--ltr needs --sidecar: without one verify pairs pages rather than \
             slots, and a page pairing has no direction to read"
        );
    }

    /// The over-refusal guard: with a sidecar named, both get past argument
    /// handling and fail on the archives instead.
    #[test]
    fn verify_allows_cache_and_ltr_when_a_sidecar_is_named() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--sidecar",
            "taiyaku.json",
            "--cache",
            "somewhere",
            "--ltr",
        ]);

        let err = verify(&args).unwrap_err();

        assert!(
            !err.contains("needs"),
            "should have got past argument handling, but refused: {err}"
        );
    }

    /// Two commands take no flags at all, where "its flags are " would trail
    /// off into nothing.
    #[test]
    fn a_command_with_no_flags_says_so_rather_than_listing_an_empty_set() {
        let args = argv(&["taiyaku.json", "raw", "en", "--stats"]);

        assert_eq!(
            lookup(&args).unwrap_err(),
            "lookup takes no flags, and `--stats` was given as one"
        );
    }

    /// `--` is the edition separator, not a flag, and must survive a scan that
    /// refuses everything beginning with `--`.
    #[test]
    fn the_edition_separator_is_not_treated_as_an_unknown_flag() {
        let args = argv(&["does-not-exist-a.cbz", "--", "does-not-exist-b.cbz"]);

        let err = align(&args).unwrap_err();

        assert!(
            !err.contains("no flag"),
            "the separator was refused as a flag: {err}"
        );
    }

    /// #3, folded in here because the fix is the same scan: a flag typed on
    /// the a-side lands in the member list, and used to be reported as a file
    /// that does not exist — which names the wrong problem entirely.
    #[test]
    fn a_flag_before_the_separator_says_so_rather_than_reporting_a_missing_file() {
        let args = argv(&["a.cbz", "--stats", "--", "b.cbz"]);

        assert_eq!(
            align(&args).unwrap_err(),
            "`--stats` is a flag, but it is before the `--` that separates the \
             two editions, so it was read as a member; flags go after both"
        );
    }

    /// The flag was not typed, so there is nothing to complain about and the
    /// caller's default stands. This is the path every `unwrap_or` below
    /// depends on, and the one a refusal must not break.
    #[test]
    fn an_absent_flag_is_absent_rather_than_an_error() {
        let args = argv(&["a.cbz", "--", "b.cbz"]);

        assert_eq!(option_value(&args, "--write", "a path"), Ok(None));
    }

    #[test]
    fn a_flag_with_a_value_still_returns_it() {
        let args = argv(&["a.cbz", "--", "b.cbz", "--write", "taiyaku.json"]);

        assert_eq!(
            option_value(&args, "--write", "a path"),
            Ok(Some("taiyaku.json"))
        );
    }

    /// The first of O15's two silences: a flag last on the line has nothing
    /// after it, and `args.get(at + 1)` answering `None` is indistinguishable
    /// from the flag never being typed. `taiyaku inventory releases
    /// --min-pages` walked with the default threshold of 60 and said nothing.
    #[test]
    fn a_flag_with_nothing_after_it_is_refused_rather_than_read_as_absent() {
        let args = argv(&["a.cbz", "--", "b.cbz", "--write"]);

        assert_eq!(
            option_value(&args, "--write", "a path"),
            Err("--write needs a path, and nothing followed it".to_string())
        );
    }

    /// The second: the argument after the flag is taken verbatim however it
    /// reads, so `align a -- b --write --ltr` wrote the sidecar to a file
    /// named `--ltr` — and applied the direction anyway, since `--ltr` is
    /// matched separately, so nothing reported a problem.
    #[test]
    fn a_flag_followed_by_another_flag_is_refused_rather_than_taking_it_as_the_value() {
        let args = argv(&["a.cbz", "--", "b.cbz", "--write", "--ltr"]);

        assert_eq!(
            option_value(&args, "--write", "a path"),
            Err("--write needs a path, and `--ltr` is another flag".to_string())
        );
    }

    /// The refusal is worth nothing if it arrives after fingerprinting, which
    /// is the argument the `--name-a` check above already makes — these member
    /// paths do not exist, so a version of this that resolved options late
    /// would fail on a file-not-found instead.
    #[test]
    fn align_refuses_a_valueless_write_before_touching_any_archive() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--write",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--write needs a path, and nothing followed it"
        );
    }

    /// O20, the four flags that describe a sidecar. Dead on *both* paths when
    /// nothing is written, not merely on the page-level one — the default
    /// path without `--write` skips the block that reads them just as
    /// `--no-split-spreads` does.
    #[test]
    fn align_refuses_a_title_when_no_sidecar_is_written() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--title",
            "Some Work",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--title needs --write: it describes a sidecar, and this run writes none"
        );
    }

    /// The same rule reached through a flag that carries no value, so the
    /// check cannot be spelled "a resolved option was Some".
    #[test]
    fn align_refuses_direct_when_no_sidecar_is_written() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--direct",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--direct needs --write: it describes a sidecar, and this run writes none"
        );
    }

    /// O20's other half: two flags that are live on the default path and inert
    /// on the page-level one. `--cache` writes the fingerprint cache there and
    /// nothing at all here, measured — the directory is never created.
    #[test]
    fn align_refuses_a_cache_the_page_level_path_would_never_write() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--no-split-spreads",
            "--cache",
            "somewhere",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--cache cannot be used with --no-split-spreads: that path \
             fingerprints directly and reads nothing through the cache"
        );
    }

    /// The guard against over-refusing: with a sidecar actually being written,
    /// every one of these gets past argument handling and fails on the
    /// archives instead.
    #[test]
    fn align_allows_the_sidecar_flags_when_one_is_written() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--write",
            "taiyaku.json",
            "--title",
            "Some Work",
            "--name-a",
            "raw",
            "--ltr",
            "--cache",
            "somewhere",
        ]);

        let err = align(&args).unwrap_err();

        assert!(
            !err.contains("needs"),
            "should have got past argument handling, but refused: {err}"
        );
    }

    /// `validate_key` does not catch this one and cannot: `-` is in its safe
    /// alphabet, so `--name-b` is a well-formed edition key. Before this,
    /// `align … --name-a --name-b` wrote an edition keyed `--name-b` into the
    /// sidecar, where it becomes a path segment in two URLs.
    #[test]
    fn align_refuses_a_name_a_that_swallowed_the_next_flag() {
        let args = argv(&[
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--name-a",
            "--name-b",
        ]);

        assert_eq!(
            align(&args).unwrap_err(),
            "--name-a needs an edition key, and `--name-b` is another flag"
        );
    }

    // -- slug, label, url_safe_suggestion: pure formatting, no archives needed --

    /// A caller-given name is trusted verbatim; the auto-derivation below is
    /// never consulted.
    #[test]
    fn slug_returns_the_given_name_unchanged() {
        assert_eq!(slug(&[], Some("raw-mono")), "raw-mono");
    }

    /// The common case: a stem with spaces becomes hyphens, lowercased.
    #[test]
    fn slug_derives_from_the_first_members_stem() {
        let members = [PathBuf::from("Raw Volume 01.cbz"), PathBuf::from("z.cbz")];
        assert_eq!(slug(&members, None), "raw-volume-01");
    }

    /// The fallback this function shares with `url_safe_suggestion`: a stem
    /// that is nothing but unsafe characters trims to empty, and an empty slug
    /// is not a usable edition key.
    #[test]
    fn slug_falls_back_to_edition_when_the_stem_has_no_safe_characters() {
        assert_eq!(slug(&[], None), "edition");
        assert_eq!(slug(&[PathBuf::from("!!!.cbz")], None), "edition");
    }

    /// `validate_key`'s suggestion goes through this same fallback when the
    /// key survives on nothing: every character is unsafe, so the slug it
    /// proposes must still be a usable key rather than an empty string.
    #[test]
    fn validate_key_suggests_edition_when_nothing_survives_the_slug() {
        let err = validate_key("!!!").unwrap_err();
        assert_eq!(
            err,
            "edition key `!!!` is not URL-safe (`!` at position 0); try `edition`"
        );
    }

    #[test]
    fn label_of_no_members_is_empty() {
        assert_eq!(label(&[]), "");
    }

    #[test]
    fn label_of_one_member_is_its_file_name() {
        assert_eq!(label(&[PathBuf::from("a.cbz")]), "a.cbz");
    }

    #[test]
    fn label_of_several_members_counts_the_rest() {
        let members = [
            PathBuf::from("a.cbz"),
            PathBuf::from("b.cbz"),
            PathBuf::from("c.cbz"),
        ];
        assert_eq!(label(&members), "a.cbz + 2 more");
    }

    /// `split_editions`' other refusal: the `--` was found, but one side of it
    /// is empty. A member list with nothing in it cannot be aligned against
    /// anything.
    #[test]
    fn split_editions_refuses_an_empty_side() {
        let err = split_editions(&argv(&["--", "b.cbz"])).unwrap_err();
        assert_eq!(err, "both sides need at least one member");
    }

    // -- run(): the dispatcher itself, not just the commands it calls --
    //
    // Every test above drives a command function directly, which never
    // exercises `run`'s own `match` — the line that turns `"align"` into a
    // call to `align` could be deleted, or point at the wrong function
    // entirely, and nothing above would notice. These call `run` and lean on
    // each command's own distinctive refusal to prove which function actually
    // ran, the same way `a_flag_belonging_to_another_command_is_refused`
    // proves `reject_unknown_flags` is not shared.

    #[test]
    fn run_dispatches_to_align() {
        let args = argv(&["align", "a.cbz", "--", "b.cbz", "--split-spreads"]);
        assert_eq!(
            run(&args).unwrap_err(),
            "--split-spreads is retired: splitting joined spreads is now what \
             align does by default, so the flag had nothing left to turn on; \
             pass --no-split-spreads for the page-level diagnostic"
        );
    }

    #[test]
    fn run_dispatches_to_verify() {
        let args = argv(&[
            "verify",
            "does-not-exist-a.cbz",
            "--",
            "does-not-exist-b.cbz",
            "--cache",
            "somewhere",
        ]);
        assert_eq!(
            run(&args).unwrap_err(),
            "--cache needs --sidecar: without one verify solves the pair \
             directly and reads nothing through the cache"
        );
    }

    #[test]
    fn run_dispatches_to_topology() {
        assert_eq!(
            run(&argv(&["topology"])).unwrap_err(),
            "needs a hub and at least two spokes, separated by `--`"
        );
    }

    #[test]
    fn run_dispatches_to_lookup() {
        assert_eq!(
            run(&argv(&["lookup"])).unwrap_err(),
            "usage: taiyaku lookup <taiyaku.json> <from> <to> <page>..."
        );
    }

    #[test]
    fn run_dispatches_to_serve() {
        assert_eq!(
            run(&argv(&["serve"])).unwrap_err(),
            "needs at least one taiyaku.json to serve"
        );
    }

    #[test]
    fn run_dispatches_to_duplicates() {
        assert_eq!(
            run(&argv(&["duplicates"])).unwrap_err(),
            "needs at least two copies, separated by `--`; one copy is not a duplicate set"
        );
    }

    #[test]
    fn run_dispatches_to_inventory() {
        assert_eq!(
            run(&argv(&["inventory"])).unwrap_err(),
            "needs a directory to walk"
        );
    }

    #[test]
    fn run_dispatches_to_credits() {
        let err = run(&argv(&["credits"])).unwrap_err();
        assert!(err.starts_with("taiyaku credits <member>..."), "{err}");
    }

    #[test]
    fn run_dispatches_to_regions() {
        let err = run(&argv(&["regions"])).unwrap_err();
        assert!(err.starts_with("taiyaku regions <member>..."), "{err}");
    }

    #[test]
    fn run_refuses_an_unknown_command() {
        assert_eq!(
            run(&argv(&["frobnicate"])).unwrap_err(),
            "unknown command `frobnicate`"
        );
    }

    /// No command at all prints usage rather than refusing — the one arm none
    /// of the dispatch tests above reach, since every one of them names a
    /// command.
    #[test]
    fn run_with_no_command_prints_usage_and_succeeds() {
        assert_eq!(run(&[]), Ok(()));
    }

    // -- lookup(): resolved entirely from a sidecar, so no archive is needed --

    /// Write a sidecar fixture to its own temp directory and return its path,
    /// as a `String` since every command here takes paths as `&str`.
    fn write_fixture(name: &str, doc: &Sidecar) -> String {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("taiyaku.json");
        std::fs::write(&path, doc.to_json()).expect("fixture written");
        path.to_str().expect("utf-8 temp path").to_string()
    }

    /// A star built to walk `lookup` through every `Via` it can report, and
    /// both ways `route` comes back `None`: `broken` names an edge that is
    /// there but cannot be read (a run pointing at a slot `broken` does not
    /// have), and `island` is an edition with no edge to anything at all.
    fn lookup_fixture() -> Sidecar {
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Lookup Fixture", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 8 }],
              "spreads": []
            },
            "en-fan": {
              "label": "en-fan",
              "members": [{ "path": "f.cbz", "sha256": "f0", "pages": 8 }],
              "spreads": []
            },
            "en-mono": {
              "label": "en-mono",
              "members": [{ "path": "m.cbz", "sha256": "m0", "pages": 8 }],
              "spreads": []
            },
            "broken": {
              "label": "broken",
              "members": [{ "path": "k.cbz", "sha256": "k0", "pages": 3 }],
              "spreads": []
            },
            "island": {
              "label": "island",
              "members": [{ "path": "i.cbz", "sha256": "i0", "pages": 3 }],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en-fan", "by": "solver",
              "runs": [{"a":"0-7","b":"0-7"}] },
            { "from": "raw", "to": "en-mono", "by": "solver",
              "runs": [{"a":"0-7","b":"0-7"}] },
            { "from": "raw", "to": "broken", "by": "solver",
              "runs": [{"a":"0-7","b":"0-99"}] }
          ]
        }"#;
        Sidecar::from_json(text).expect("fixture parses")
    }

    /// Pinned against `route` directly, not just `lookup`'s `Result`:
    /// `lookup` reports `Ok(())` whether it found a direct edge, an inverted
    /// one, a hub composition, or nothing at all, so the `Result` alone
    /// cannot tell this apart from any of those.
    #[test]
    fn lookup_reports_a_direct_edge() {
        let doc = lookup_fixture();
        let route = taiyaku_core::route(&doc, "raw", "en-fan").expect("a direct edge is stored");
        assert_eq!(route.via, Via::Direct);
        assert_eq!(
            route.resolve(0),
            Some(Slot {
                page: 0,
                half: None
            })
        );
        assert_eq!(
            route.resolve(7),
            Some(Slot {
                page: 7,
                half: None
            })
        );

        let path = write_fixture("taiyaku-test-lookup-direct", &doc);
        assert_eq!(lookup(&argv(&[&path, "raw", "en-fan", "0", "7"])), Ok(()));
    }

    /// The reverse of a stored edge, walked without a second edge ever being
    /// written: `sidecar.md` stores N-1 edges and reads the (N-1)th one both
    /// ways. Pinned against `route` so a `Via::Direct` masquerading as the
    /// reverse — or a resolve that silently fell through to the hub — would
    /// fail here rather than only showing up as a printed line nothing reads.
    #[test]
    fn lookup_reports_an_inverted_edge() {
        let doc = lookup_fixture();
        let route =
            taiyaku_core::route(&doc, "en-fan", "raw").expect("the raw -> en-fan edge reverses");
        assert_eq!(route.via, Via::Inverted);
        assert_eq!(
            route.resolve(0),
            Some(Slot {
                page: 0,
                half: None
            })
        );

        let path = write_fixture("taiyaku-test-lookup-inverted", &doc);
        assert_eq!(lookup(&argv(&[&path, "en-fan", "raw", "0"])), Ok(()));
    }

    /// Neither `en-fan` nor `en-mono` has a direct edge to the other; both
    /// have one to the hub, so the route composes through it. Pinned against
    /// `route` so a composition that quietly failed and left the pages
    /// unresolved — indistinguishable from success through `lookup`'s
    /// `Result` alone — would fail here.
    #[test]
    fn lookup_reports_a_hub_composed_route() {
        let doc = lookup_fixture();
        let route =
            taiyaku_core::route(&doc, "en-fan", "en-mono").expect("both spokes reach the hub");
        assert_eq!(route.via, Via::Hub("raw".to_string()));
        assert_eq!(
            route.resolve(0),
            Some(Slot {
                page: 0,
                half: None
            })
        );

        let path = write_fixture("taiyaku-test-lookup-hub", &doc);
        assert_eq!(lookup(&argv(&[&path, "en-fan", "en-mono", "0"])), Ok(()));
    }

    /// Pinned against `route` so an identity route that quietly resolved
    /// nothing — or one that was reached only by a same-name coincidence
    /// through the hub rather than `Via::Identity` — would fail here.
    #[test]
    fn lookup_reports_identity_for_the_same_edition() {
        let doc = lookup_fixture();
        let route = taiyaku_core::route(&doc, "raw", "raw").expect("an edition aligns to itself");
        assert_eq!(route.via, Via::Identity);
        assert_eq!(
            route.resolve(0),
            Some(Slot {
                page: 0,
                half: None
            })
        );

        let path = write_fixture("taiyaku-test-lookup-identity", &doc);
        assert_eq!(lookup(&argv(&[&path, "raw", "raw", "0"])), Ok(()));
    }

    /// **A stored edge is the whole answer, including when it cannot be
    /// read** — `route`'s own doc comment. `raw -> broken` is stored, so this
    /// must not fall back to composing through the hub and must not error
    /// lookup as a whole. Pinned against `route` and `runs_of` directly,
    /// since `lookup`'s `Result` cannot distinguish this from a hub fallback,
    /// an island with nothing stored, or a clean direct edge — all of them
    /// also return `Ok(())`.
    #[test]
    fn lookup_reports_an_unreadable_edge_rather_than_falling_back_to_the_hub() {
        let doc = lookup_fixture();
        assert_eq!(
            taiyaku_core::route(&doc, "raw", "broken"),
            None,
            "an edge that cannot be read must not fall back to the hub"
        );
        let alignment = doc
            .alignments
            .iter()
            .find(|a| a.from == "raw" && a.to == "broken")
            .expect("the raw -> broken edge is stored");
        assert!(
            doc.runs_of(alignment).is_err(),
            "the stored edge names a page the 3-page `broken` edition does not have"
        );

        let path = write_fixture("taiyaku-test-lookup-broken", &doc);
        assert_eq!(lookup(&argv(&[&path, "raw", "broken", "0"])), Ok(()));
    }

    /// `island` has no edge to anything, including the hub, so there is
    /// nothing to compose through it either. The other `None` case above is
    /// an edge that exists and fails to read; this is no edge existing at
    /// all — pinned against `route` and the alignment list so the two
    /// `None` cases cannot be silently swapped for one another.
    #[test]
    fn lookup_reports_that_an_unrelated_pair_has_nothing_stored() {
        let doc = lookup_fixture();
        assert_eq!(taiyaku_core::route(&doc, "raw", "island"), None);
        assert!(
            doc.alignments
                .iter()
                .all(|a| a.from != "island" && a.to != "island"),
            "island must have no stored edge to anything, unlike `broken`"
        );

        let path = write_fixture("taiyaku-test-lookup-island", &doc);
        assert_eq!(lookup(&argv(&[&path, "raw", "island", "0"])), Ok(()));
    }

    #[test]
    fn lookup_refuses_a_page_that_does_not_parse() {
        let path = write_fixture("taiyaku-test-lookup-badpage", &lookup_fixture());
        let err = lookup(&argv(&[&path, "raw", "en-fan", "not-a-number"])).unwrap_err();
        assert_eq!(err, "not a page: not-a-number");
    }

    /// A two-slot spread, page 1 of a 3-page edition, present on both sides so
    /// the whole thing is one paired run. Reading order decides which half is
    /// found first by a bare page number — `RightToLeft` puts the right half
    /// first, `LeftToRight` the left — so both are exercised rather than
    /// whichever one the default direction happens to reach.
    fn spread_lookup_fixture(reading: &str) -> Sidecar {
        let text = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Spread Fixture", "reading": "{reading}" }},
              "editions": {{
                "raw": {{
                  "label": "raw",
                  "members": [{{ "path": "r.cbz", "sha256": "r0", "pages": 3 }}],
                  "spreads": [1]
                }},
                "en": {{
                  "label": "en",
                  "members": [{{ "path": "e.cbz", "sha256": "e0", "pages": 3 }}],
                  "spreads": [1]
                }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
        );
        Sidecar::from_json(&text).expect("fixture parses")
    }

    /// Pinned against `route`/`resolve` directly, not just `lookup`'s
    /// `Result`: a span written in page notation is easy to get subtly wrong
    /// — `"0-3"` reads as "page 3", which does not exist on a 3-page edition,
    /// rather than "all four slots" — and `assert_eq!(lookup(...), Ok(()))`
    /// alone cannot tell that mistake apart from success, since an
    /// unreadable edge still returns `Ok(())` having printed a warning
    /// instead of the half. This is what caught exactly that mistake in this
    /// fixture during development.
    #[test]
    fn lookup_names_the_right_half_of_a_spread_in_a_right_to_left_document() {
        let doc = spread_lookup_fixture("rtl");
        let route = taiyaku_core::route(&doc, "raw", "en").expect("a direct edge is stored");
        assert_eq!(
            route.resolve(1),
            Some(Slot {
                page: 1,
                half: Some(Half::Right)
            })
        );

        let path = write_fixture("taiyaku-test-lookup-spread-rtl", &doc);
        assert_eq!(lookup(&argv(&[&path, "raw", "en", "1"])), Ok(()));
    }

    #[test]
    fn lookup_names_the_left_half_of_a_spread_in_a_left_to_right_document() {
        let doc = spread_lookup_fixture("ltr");
        let route = taiyaku_core::route(&doc, "raw", "en").expect("a direct edge is stored");
        assert_eq!(
            route.resolve(1),
            Some(Slot {
                page: 1,
                half: Some(Half::Left)
            })
        );

        let path = write_fixture("taiyaku-test-lookup-spread-ltr", &doc);
        assert_eq!(lookup(&argv(&[&path, "raw", "en", "1"])), Ok(()));
    }

    // -- align, verify, topology: the happy paths, which need real pages --
    //
    // Everything above stops short of `taiyaku_read::read_edition` actually
    // succeeding: every fixture is a nonexistent path, because the property
    // under test is argument handling, and real archives would only slow that
    // down. The full pipeline — decode, fingerprint, solve, print, and for
    // `align --write`, write a sidecar back — has had no test at all up to
    // this point, so everything below builds a real, decodable `.cbz`.

    /// Build a cbz of `count` real, decodable, and mutually distinguishable
    /// pages.
    ///
    /// A solid fill will not do here the way it does for `regions.rs`'s
    /// `cbz_with_page`: `fingerprint::fingerprint` is a *difference* hash,
    /// comparing each pixel to its neighbour, and every neighbour comparison
    /// in a flat image is a tie — so every solid-colour page hashes to the
    /// same all-zero fingerprint regardless of its level, and every alignment
    /// over such a set would look like a perfect match by accident rather
    /// than by the solver actually distinguishing anything. The pattern below
    /// varies along both axes and by page and by `seed`, so distinct calls
    /// produce distinct fingerprints.
    fn cbz_pages(name: &str, count: u32, seed: u32) -> PathBuf {
        use std::io::Write;
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("create cbz"));
        for i in 0..count {
            let level = seed.wrapping_add(i).wrapping_mul(101);
            let image = image::GrayImage::from_fn(8, 8, |x, y| {
                let v = x.wrapping_mul(37).wrapping_add(y.wrapping_mul(59)) + level;
                image::Luma([(v % 256) as u8])
            });
            let mut bytes = Vec::new();
            image
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .expect("an 8x8 gray image always encodes");
            zip.start_file(
                format!("{i:03}.png"),
                zip::write::SimpleFileOptions::default(),
            )
            .expect("start entry");
            zip.write_all(&bytes).expect("write page bytes");
        }
        zip.finish().expect("finish cbz");
        path
    }

    /// The page-level diagnostic (`--no-split-spreads`), with `--stats` so
    /// both `stats::calibration_line` and `stats::shape_line` run too, not
    /// just the plain `report_summary` beneath them.
    ///
    /// Everything this path computes is printed, not returned, so pin
    /// against the same `taiyaku_read::read_edition` +
    /// `fingerprint::fingerprint_edition` + `solve` chain `align` runs on
    /// this exact fixture: `a` and `b` share a seed, so every page is
    /// byte-identical to its counterpart and the correspondence is exact,
    /// not approximate. A solve that lost a page, or paired pages out of
    /// order, would still leave `align` returning `Ok(())`.
    #[test]
    fn align_runs_the_page_level_diagnostic_against_real_pages() {
        let a = cbz_pages("taiyaku-test-main-diag-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-diag-b.cbz", 3, 1);

        let a_pages = taiyaku_read::read_edition(std::slice::from_ref(&a)).expect("a decodes");
        let b_pages = taiyaku_read::read_edition(std::slice::from_ref(&b)).expect("b decodes");
        let a_hashes = fingerprint::fingerprint_edition(&a_pages).expect("a fingerprints");
        let b_hashes = fingerprint::fingerprint_edition(&b_pages).expect("b fingerprints");
        let runs = solve(&a_hashes, &b_hashes);
        for page in 0..3u32 {
            assert_eq!(
                counterpart(&runs, page),
                Some(page),
                "page {page} is identical on both sides and should pair with itself"
            );
        }

        let args = argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--no-split-spreads",
            "--stats",
        ]);

        assert_eq!(align(&args), Ok(()));
    }

    /// The default, split-spreads path without `--write`: fingerprints
    /// through `cached_edition`, solves, and prints the per-slot table.
    /// Unequal page counts so at least one slot goes unmatched, reaching the
    /// `None => gaps += 1` arm alongside the paired ones.
    ///
    /// Everything this path computes is printed, not returned, so pin
    /// against the same `cached_edition` + `solve` chain `align` runs, on a
    /// cache directory of its own so this does not depend on `align`
    /// having populated its cache first. `a` and `b` do not share a seed —
    /// this is not a trivial identity pairing — and at these seeds two of
    /// `a`'s four pages land on `b`'s two, with the other two the gaps the
    /// doc comment above is about. A swapped or reordered pairing here
    /// would still leave `align` returning `Ok(())`.
    #[test]
    fn align_runs_the_default_diagnostic_against_real_pages() {
        let a = cbz_pages("taiyaku-test-main-split-a.cbz", 4, 1);
        let b = cbz_pages("taiyaku-test-main-split-b.cbz", 2, 50);

        let probe_cache = std::env::temp_dir().join("taiyaku-test-main-split-cache-expected");
        let _ = std::fs::remove_dir_all(&probe_cache);
        let (a_hashes, _, _) = cached_edition(
            &probe_cache,
            std::slice::from_ref(&a),
            Direction::LeftToRight,
        )
        .expect("a fingerprints");
        let (b_hashes, _, _) = cached_edition(
            &probe_cache,
            std::slice::from_ref(&b),
            Direction::LeftToRight,
        )
        .expect("b fingerprints");
        let runs = solve(&a_hashes, &b_hashes);
        assert_eq!(
            counterpart(&runs, 0),
            None,
            "page 0 of a has no counterpart in the 2-page b"
        );
        assert_eq!(counterpart(&runs, 1), Some(0));
        assert_eq!(counterpart(&runs, 2), Some(1));
        assert_eq!(
            counterpart(&runs, 3),
            None,
            "page 3 of a has no counterpart in the 2-page b"
        );

        let cache = std::env::temp_dir().join("taiyaku-test-main-split-cache");
        let _ = std::fs::remove_dir_all(&cache);
        let args = argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--stats",
            "--ltr",
        ]);

        assert_eq!(align(&args), Ok(()));
    }

    /// `--write` with no existing file at `out`: the `None` arm of the
    /// existing/absent match, building a fresh two-edition `Sidecar` from
    /// scratch. Asserted against the file `align` actually wrote, not just
    /// the `Result` — `write_sidecar`'s own tests already cover truncation
    /// safety, so what is worth pinning here is that `align` handed it the
    /// document `docs/sidecar.md` describes: both editions, one alignment,
    /// the title and hub this run asked for.
    ///
    /// `b` shares `a`'s seed rather than using one of its own, so every page
    /// matches its counterpart exactly: a real solve with no signal to speak
    /// of would leave `solver: None` and every run's `margin: None`, which is
    /// correct but exercises neither. A clean match is what makes calibration
    /// worth accepting and every run fully paired, reaching both.
    #[test]
    fn align_write_creates_a_new_sidecar_from_real_pages() {
        let a = cbz_pages("taiyaku-test-main-write-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-write-b.cbz", 3, 1);
        let cache = std::env::temp_dir().join("taiyaku-test-main-write-cache");
        let _ = std::fs::remove_dir_all(&cache);
        let out_dir = std::env::temp_dir().join("taiyaku-test-main-write-out");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).expect("temp dir");
        let out = out_dir.join("taiyaku.json");
        let args = argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--title",
            "Test Work",
            "--name-a",
            "raw",
            "--name-b",
            "en",
        ]);

        assert_eq!(align(&args), Ok(()));

        let text = std::fs::read_to_string(&out).expect("sidecar written");
        let doc = Sidecar::from_json(&text).expect("written sidecar parses");
        assert_eq!(doc.work.title, "Test Work");
        assert_eq!(doc.hub, "raw");
        assert_eq!(doc.editions.len(), 2);
        assert!(doc.editions.contains_key("raw"));
        assert!(doc.editions.contains_key("en"));
        assert_eq!(doc.alignments.len(), 1);
        let alignment = &doc.alignments[0];
        assert_eq!(alignment.from, "raw");
        assert_eq!(alignment.to, "en");
        assert_eq!(
            alignment.runs.len(),
            1,
            "one clean run over identical pages"
        );
        assert!(
            alignment.runs[0].b.is_some(),
            "the run is paired, not a gap"
        );
        assert!(
            alignment.runs[0].margin.is_some(),
            "a paired run's weakest margin is recorded"
        );
        assert!(
            alignment.solver.is_some(),
            "a real match calibrates as accepted"
        );
    }

    /// Merging a second and then a third spoke into the document the test
    /// above could have written, covering what a fresh write cannot: the
    /// `Some(existing)` arm end to end, `--name-a` both given and derived
    /// from the document's own hub, and both `Merged::Added` (`fr` is new)
    /// and `Merged::Replaced` (`en` already exists) — `merge_spoke`'s own
    /// tests cover the rule; what is worth pinning here is that `align`
    /// reaches it with the right arguments in both shapes.
    #[test]
    fn align_write_merges_further_spokes_into_an_existing_sidecar() {
        let a = cbz_pages("taiyaku-test-main-merge-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-merge-b.cbz", 3, 60);
        let c = cbz_pages("taiyaku-test-main-merge-c.cbz", 3, 90);
        let cache = std::env::temp_dir().join("taiyaku-test-main-merge-cache");
        let _ = std::fs::remove_dir_all(&cache);
        let out_dir = std::env::temp_dir().join("taiyaku-test-main-merge-out");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).expect("temp dir");
        let out = out_dir.join("taiyaku.json");

        // First write: establishes the document, hub `raw`.
        assert_eq!(
            align(&argv(&[
                a.to_str().unwrap(),
                "--",
                b.to_str().unwrap(),
                "--cache",
                cache.to_str().unwrap(),
                "--write",
                out.to_str().unwrap(),
                "--name-a",
                "raw",
                "--name-b",
                "en",
            ])),
            Ok(())
        );

        // Second write: no --name-a, so it is read off the document's own
        // hub. `fr` does not exist yet, so this is `Merged::Added`.
        assert_eq!(
            align(&argv(&[
                a.to_str().unwrap(),
                "--",
                c.to_str().unwrap(),
                "--cache",
                cache.to_str().unwrap(),
                "--write",
                out.to_str().unwrap(),
                "--name-b",
                "fr",
            ])),
            Ok(())
        );

        // Third write: --name-a given explicitly and it agrees with the hub,
        // re-merging `en`, which already exists — `Merged::Replaced`.
        assert_eq!(
            align(&argv(&[
                a.to_str().unwrap(),
                "--",
                b.to_str().unwrap(),
                "--cache",
                cache.to_str().unwrap(),
                "--write",
                out.to_str().unwrap(),
                "--name-a",
                "raw",
                "--name-b",
                "en",
            ])),
            Ok(())
        );

        let text = std::fs::read_to_string(&out).expect("sidecar written");
        let doc = Sidecar::from_json(&text).expect("written sidecar parses");
        assert_eq!(doc.editions.len(), 3, "raw, en and fr all present");
        assert!(doc.editions.contains_key("fr"));
        assert_eq!(doc.alignments.len(), 2, "raw->en replaced, not duplicated");
    }

    /// `--direct` on a merge has no hub to fall back on, so it requires
    /// `--name-a` — the one arm of that `match` the tests above never reach,
    /// since both give an existing document but never `--direct` without a
    /// name.
    #[test]
    fn align_write_refuses_direct_without_name_a_on_a_merge() {
        let a = cbz_pages("taiyaku-test-main-direct-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-direct-b.cbz", 3, 60);
        let cache = std::env::temp_dir().join("taiyaku-test-main-direct-cache");
        let _ = std::fs::remove_dir_all(&cache);
        let out_dir = std::env::temp_dir().join("taiyaku-test-main-direct-out");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).expect("temp dir");
        let out = out_dir.join("taiyaku.json");
        assert_eq!(
            align(&argv(&[
                a.to_str().unwrap(),
                "--",
                b.to_str().unwrap(),
                "--cache",
                cache.to_str().unwrap(),
                "--write",
                out.to_str().unwrap(),
                "--name-a",
                "raw",
                "--name-b",
                "en",
            ])),
            Ok(())
        );

        let err = align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--direct",
            "--name-b",
            "fr",
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "--direct needs --name-a to say which edition the edge starts from"
        );
    }

    #[test]
    fn align_write_refuses_both_editions_named_the_same() {
        let a = cbz_pages("taiyaku-test-main-clash-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-clash-b.cbz", 3, 60);
        let out = std::env::temp_dir().join("taiyaku-test-main-clash.json");
        let _ = std::fs::remove_file(&out);

        let err = align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--name-a",
            "same",
            "--name-b",
            "same",
        ]))
        .unwrap_err();

        assert_eq!(
            err,
            "both editions would be named `same`; pass --name-a and --name-b"
        );
    }

    /// A merge cannot silently rename the work it is joining — `--title` on
    /// an existing document is only honoured when it agrees.
    #[test]
    fn align_write_refuses_to_rename_a_work_on_a_merge() {
        let a = cbz_pages("taiyaku-test-main-retitle-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-retitle-b.cbz", 3, 60);
        let out_dir = std::env::temp_dir().join("taiyaku-test-main-retitle-out");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).expect("temp dir");
        let out = out_dir.join("taiyaku.json");
        assert_eq!(
            align(&argv(&[
                a.to_str().unwrap(),
                "--",
                b.to_str().unwrap(),
                "--write",
                out.to_str().unwrap(),
                "--title",
                "Original Title",
                "--name-a",
                "raw",
                "--name-b",
                "en",
            ])),
            Ok(())
        );

        let err = align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--title",
            "Different Title",
            "--name-b",
            "en",
        ]))
        .unwrap_err();

        assert_eq!(
            err,
            format!(
                "{} is the sidecar for `Original Title`; --title cannot rename a work on a merge",
                out.to_str().unwrap()
            )
        );
    }

    /// `verify` with no `--sidecar`: solves the pair directly and writes the
    /// HTML report. `report::render`'s own tests cover the markup; what is
    /// worth pinning at this level is that `verify` actually reaches it and
    /// leaves a non-empty file at `--out`.
    #[test]
    fn verify_writes_a_report_for_a_direct_solve() {
        let a = cbz_pages("taiyaku-test-main-verify-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-verify-b.cbz", 3, 60);
        let out_dir = std::env::temp_dir().join("taiyaku-test-main-verify-out");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).expect("temp dir");
        let out = out_dir.join("report.html");
        let args = argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);

        assert_eq!(verify(&args), Ok(()));

        let html = std::fs::read_to_string(&out).expect("report written");
        assert!(!html.is_empty(), "a real report was written");
    }

    /// `verify --sidecar`: the branch that reads runs from a file rather than
    /// solving, indexes by slot rather than page, and maps slot-confidence
    /// back to page numbers for the report. The sidecar's `sha256` entries
    /// are the real digests of the real archives below, computed the same way
    /// `verify`'s own `named` closure computes them — a placeholder digest
    /// would fail `edition_of` before any of the rest of this ran.
    #[test]
    fn verify_reports_confidence_through_a_stored_sidecar() {
        let a = cbz_pages("taiyaku-test-main-verify-sc-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-verify-sc-b.cbz", 3, 60);
        let a_digest = taiyaku_read::member_digest(&a).expect("digest a");
        let b_digest = taiyaku_read::member_digest(&b).expect("digest b");
        let doc_text = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Verify Fixture", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "raw",
                  "members": [{{ "path": "a.cbz", "sha256": "{a_digest}", "pages": 3 }}],
                  "spreads": [] }},
                "en": {{ "label": "en",
                  "members": [{{ "path": "b.cbz", "sha256": "{b_digest}", "pages": 3 }}],
                  "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
        );
        let dir = std::env::temp_dir().join("taiyaku-test-main-verify-sidecar");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let sidecar_path = dir.join("taiyaku.json");
        std::fs::write(&sidecar_path, &doc_text).expect("fixture written");

        let out = dir.join("report.html");
        let cache = dir.join("cache");
        let args = argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--sidecar",
            sidecar_path.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
        ]);

        assert_eq!(verify(&args), Ok(()));
        assert!(out.exists(), "report written through the sidecar path");
    }

    /// The whole star: hub-to-spoke and spoke-to-spoke, direct against
    /// composed, printed and totalled. `merge_spoke`, `compose` and `verdict`
    /// each have their own unit tests in `taiyaku-core`; what only exists
    /// here is `topology` actually wiring them together over real pages.
    ///
    /// `topology` only prints — nothing it computes reaches the `Result` —
    /// and capturing stdout would need a dependency this crate does not
    /// have, so this pins against the same `fingerprint_edition`, `solve`
    /// and `compose` chain `topology` runs, on this exact fixture, instead.
    /// At these seeds `spoke2 -> hub` is a real calibration refusal (a total
    /// gap) while the direct `spoke2 -> spoke1` solve is a clean match — so
    /// `compose(to_hub[spoke2], from_hub[spoke1])`, the composed leg
    /// `topology` builds for that pair, must come back strictly weaker than
    /// the direct solve it is measured against. That gap is exactly what the
    /// `loss` column `topology` prints counts. A composition that silently
    /// fell back to the direct answer, or a hub/spoke argument swapped in
    /// `topology`'s own `solve` calls, would still return `Ok(())` and would
    /// still fail this.
    #[test]
    fn topology_reports_the_star_against_two_spokes() {
        let hub = cbz_pages("taiyaku-test-main-topology-hub.cbz", 3, 10);
        let spoke1 = cbz_pages("taiyaku-test-main-topology-s1.cbz", 3, 20);
        let spoke2 = cbz_pages("taiyaku-test-main-topology-s2.cbz", 3, 30);

        let hub_pages =
            taiyaku_read::read_edition(std::slice::from_ref(&hub)).expect("hub decodes");
        let s1_pages =
            taiyaku_read::read_edition(std::slice::from_ref(&spoke1)).expect("spoke1 decodes");
        let s2_pages =
            taiyaku_read::read_edition(std::slice::from_ref(&spoke2)).expect("spoke2 decodes");
        let hub_hashes = fingerprint::fingerprint_edition(&hub_pages).expect("hub fingerprints");
        let s1_hashes = fingerprint::fingerprint_edition(&s1_pages).expect("spoke1 fingerprints");
        let s2_hashes = fingerprint::fingerprint_edition(&s2_pages).expect("spoke2 fingerprints");

        let to_hub_2 = solve(&s2_hashes, &hub_hashes);
        let from_hub_1 = solve(&hub_hashes, &s1_hashes);
        let direct_2_1 = solve(&s2_hashes, &s1_hashes);
        let composed_2_1 = compose(&to_hub_2, &from_hub_1);

        for page in 0..3u32 {
            assert_eq!(
                counterpart(&direct_2_1, page),
                Some(page),
                "spoke2 and spoke1 solve directly as a clean, page-for-page match at these seeds"
            );
            assert_eq!(
                counterpart(&to_hub_2, page),
                None,
                "spoke2 -> hub is a calibration refusal at these seeds, not a match"
            );
            assert_eq!(
                counterpart(&composed_2_1, page),
                None,
                "composed through the hub must not recover a pairing the hub leg refused"
            );
        }

        let args = argv(&[
            hub.to_str().unwrap(),
            "--",
            spoke1.to_str().unwrap(),
            "--",
            spoke2.to_str().unwrap(),
        ]);

        assert_eq!(topology(&args), Ok(()));
    }

    /// The relative path is `.` exactly when `target` and `base` canonicalise
    /// to the same directory — the empty-components case `PathBuf` cannot
    /// represent any other way, since `PathBuf::from("")` is not a usable
    /// path segment.
    #[test]
    fn relative_to_is_current_directory_when_target_and_base_are_the_same() {
        let dir = std::env::temp_dir().join("taiyaku-test-relative-same");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        let recorded = relative_to(&dir, &dir);

        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(recorded, Ok(PathBuf::from(".")));
    }

    // -- verify(): the sidecar arm's remaining branches --

    /// A sidecar this specific pair is not an edition of: `--sidecar` names a
    /// real document, but the members given on the command line hash to
    /// nothing it defines. `named`'s closure is what catches this, and it has
    /// to report which side.
    #[test]
    fn verify_refuses_members_that_match_no_edition_of_the_sidecar() {
        let a = cbz_pages("taiyaku-test-main-verify-mismatch-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-verify-mismatch-b.cbz", 3, 60);
        let unrelated = cbz_pages("taiyaku-test-main-verify-mismatch-x.cbz", 3, 200);
        let a_digest = taiyaku_read::member_digest(&a).expect("digest a");
        let b_digest = taiyaku_read::member_digest(&b).expect("digest b");
        let doc_text = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Mismatch Fixture", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "raw",
                  "members": [{{ "path": "a.cbz", "sha256": "{a_digest}", "pages": 3 }}],
                  "spreads": [] }},
                "en": {{ "label": "en",
                  "members": [{{ "path": "b.cbz", "sha256": "{b_digest}", "pages": 3 }}],
                  "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
        );
        let dir = std::env::temp_dir().join("taiyaku-test-main-verify-mismatch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let sidecar_path = dir.join("taiyaku.json");
        std::fs::write(&sidecar_path, &doc_text).expect("fixture written");

        let args = argv(&[
            unrelated.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--sidecar",
            sidecar_path.to_str().unwrap(),
        ]);

        let err = verify(&args).unwrap_err();
        assert!(
            err.contains("the a-side members are not an edition of"),
            "{err}"
        );
    }

    /// The same fixture read the other way round: `raw -> en` is the stored
    /// edge, so asking for `en`'s members as the a-side and `raw`'s as the
    /// b-side reads it inverted rather than direct — the `--ltr` on this test
    /// also covers `Direction::LeftToRight`, which nothing else here reaches
    /// since every other sidecar test leaves it at the default.
    #[test]
    fn verify_reports_confidence_through_an_inverted_edge() {
        let a = cbz_pages("taiyaku-test-main-verify-inv-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-verify-inv-b.cbz", 3, 60);
        let a_digest = taiyaku_read::member_digest(&a).expect("digest a");
        let b_digest = taiyaku_read::member_digest(&b).expect("digest b");
        let doc_text = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Inverted Fixture", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "raw",
                  "members": [{{ "path": "a.cbz", "sha256": "{a_digest}", "pages": 3 }}],
                  "spreads": [] }},
                "en": {{ "label": "en",
                  "members": [{{ "path": "b.cbz", "sha256": "{b_digest}", "pages": 3 }}],
                  "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
        );
        let dir = std::env::temp_dir().join("taiyaku-test-main-verify-inverted");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let sidecar_path = dir.join("taiyaku.json");
        std::fs::write(&sidecar_path, &doc_text).expect("fixture written");

        let out = dir.join("report.html");
        let cache = dir.join("cache");
        // b's real members on the a-side, a's on the b-side: the reverse of
        // the stored `raw -> en` edge.
        let args = argv(&[
            b.to_str().unwrap(),
            "--",
            a.to_str().unwrap(),
            "--sidecar",
            sidecar_path.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--ltr",
        ]);

        assert_eq!(verify(&args), Ok(()));
        assert!(out.exists(), "report written through the inverted edge");
    }

    // -- serve(): argument parsing only. Actually starting the server would
    // block this test forever, so every one of these stops at the first
    // thing that fails before a socket is ever bound: `load_works` on a
    // sidecar that does not exist.

    #[test]
    fn serve_refuses_an_unknown_flag() {
        let err = serve(&argv(&["--bogus"])).unwrap_err();
        assert_eq!(
            err,
            "serve has no flag `--bogus`; its flags are --addr, --log-requests, \
             --refresh-stump, --stump-key, --stump-url"
        );
    }

    #[test]
    fn serve_refuses_a_stump_url_with_no_key() {
        let err = serve(&argv(&["a.json", "--stump-url", "https://example.test"])).unwrap_err();
        assert_eq!(
            err,
            "--stump-url needs a key: --stump-key, or TAIYAKU_STUMP_KEY"
        );
    }

    #[test]
    fn serve_refuses_a_stump_key_with_no_url() {
        let err = serve(&argv(&["a.json", "--stump-key", "secret"])).unwrap_err();
        assert_eq!(
            err,
            "a stump key was given with no --stump-url or TAIYAKU_STUMP_URL"
        );
    }

    /// Past every argument-parsing branch — both stump flags together,
    /// `--log-requests`, `--refresh-stump` — and then failing on the sidecar,
    /// which does not exist, rather than ever reaching `tokio::runtime` and
    /// blocking this test on a server that never stops.
    #[test]
    fn serve_reaches_load_works_with_every_flag_resolved() {
        let log = std::env::temp_dir().join("taiyaku-test-serve-log.jsonl");
        let args = argv(&[
            "does-not-exist.json",
            "--addr",
            "127.0.0.1:0",
            "--stump-url",
            "https://example.test",
            "--stump-key",
            "secret",
            "--log-requests",
            log.to_str().unwrap(),
            "--refresh-stump",
        ]);

        let err = serve(&args).unwrap_err();
        assert!(err.contains("does-not-exist.json"), "{err}");
    }

    /// Neither `--stump-url` nor `--stump-key` given, and neither environment
    /// variable set: the fourth arm of the `(base, key)` match, `stump`
    /// staying `None` rather than being configured at all — the ordinary case
    /// for a reader with no Stump instance, which every other serve test here
    /// bypasses by giving both flags.
    #[test]
    fn serve_reaches_load_works_with_no_stump_configured() {
        let err = serve(&argv(&["does-not-exist.json"])).unwrap_err();
        assert!(err.contains("does-not-exist.json"), "{err}");
    }

    /// `--write` reading an existing `out` that fails for a reason other than
    /// "not found" — here, `out` is itself a directory — reports that error
    /// rather than reading it as an absent file and trying to create a fresh
    /// document over it.
    #[test]
    fn align_write_reports_a_read_error_other_than_not_found() {
        let a = cbz_pages("taiyaku-test-main-readerr-a.cbz", 3, 1);
        let b = cbz_pages("taiyaku-test-main-readerr-b.cbz", 3, 60);
        let out = std::env::temp_dir().join("taiyaku-test-main-readerr-out");
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).expect("out exists as a directory, not a file");

        let err = align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--name-a",
            "raw",
            "--name-b",
            "en",
        ]))
        .unwrap_err();

        let _ = std::fs::remove_dir_all(&out);
        assert!(
            err.starts_with(&format!("{}: ", out.to_str().unwrap())),
            "{err}"
        );
    }

    // `main` itself is not tested here, and cannot usefully be: it calls
    // `std::process::exit`, which would tear down this whole test binary
    // along with every other test running in it, so calling it in-process is
    // not an option. The standard escape hatch — `env!("CARGO_BIN_EXE_...")`,
    // running the real compiled binary as a subprocess — does not apply
    // either: that variable is set by Cargo for a package's *other* test
    // targets, not for a `[[bin]]`'s own unit tests, since building this test
    // binary and building the `taiyaku` binary it would need to invoke is the
    // same `cargo build` step. `main` is four lines that unpack `argv`, call
    // `run`, and translate `Err` into a message on stderr and exit code 1;
    // every one of those four lines is exercised in `run`'s own tests above,
    // and `run` is the whole of what `main` delegates to.
}
