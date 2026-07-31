//! Command-line aligner for taiyaku.
//!
//!   taiyaku align <a-member>... -- <b-member>...
//!
//! Each side is an *edition*: one or more archives or directories that flatten
//! into a single page sequence. `--` separates the two, so a raw volume can be
//! aligned against the several chapter files that make it up.
//!
//! Fingerprints are cached under `.taiyaku-cache/` by member content hash, so
//! re-solving costs milliseconds rather than seconds.

mod cache;
#[cfg(test)]
mod corpus;
mod fingerprint;
mod naming;
mod pages;
mod report;
mod stats;

use std::path::PathBuf;

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
        Some(other) => Err(format!("unknown command `{other}`")),
        None => {
            println!("taiyaku {}", env!("CARGO_PKG_VERSION"));
            println!("usage: taiyaku align  <a-member>... -- <b-member>...");
            println!(
                "       taiyaku verify <a-member>... -- <b-member>... [--out FILE] [--sidecar FILE]"
            );
            println!("       taiyaku topology <hub>... -- <spoke>... -- <spoke>...");
            println!("       taiyaku lookup <taiyaku.json> <from> <to> <page>...");
            println!();
            println!("align options: --split-spreads --ltr --stats --write FILE --title NAME");
            println!("               --name-a KEY --name-b KEY --direct");
            println!();
            println!("--write merges into an existing FILE rather than overwriting it. On a");
            println!("merge the a-side is the document's hub, so --name-a is optional;");
            println!("--direct records a spoke-to-spoke edge instead and requires it.");
            Ok(())
        }
    }
}

/// Split `a... -- b...` into the two editions, pulling off trailing options.
fn split_editions(args: &[String]) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let split = args
        .iter()
        .position(|a| a == "--")
        .ok_or("needs `--` between the two editions")?;
    let (left, rest) = args.split_at(split);
    let a_members: Vec<PathBuf> = left.iter().map(PathBuf::from).collect();
    let b_members: Vec<PathBuf> = rest[1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if a_members.is_empty() || b_members.is_empty() {
        return Err("both sides need at least one member".into());
    }
    Ok((a_members, b_members))
}

fn option_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at + 1).map(String::as_str)
}

/// A short identifier for an edition, derived from its first member unless the
/// caller names one. Sidecar keys are referenced by hand in corrections, so
/// `raw-mono` beats `a`.
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
    let (a_members, b_members) = split_editions(args)?;
    let out = option_value(args, "--out").unwrap_or("taiyaku-verify.html");

    let a_pages = pages::read_edition(&a_members)?;
    let b_pages = pages::read_edition(&b_members)?;
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
    let (runs, a_units) = match option_value(args, "--sidecar") {
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
                    digests.push(pages::member_digest(member)?);
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
    let scored = match option_value(args, "--sidecar") {
        Some(_) => {
            let cache_root =
                std::path::PathBuf::from(option_value(args, "--cache").unwrap_or(".taiyaku-cache"));
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
    let hub_hashes = fingerprint::fingerprint_edition(&pages::read_edition(hub)?)?;
    let mut spoke_hashes = Vec::new();
    for (i, spoke) in spokes.iter().enumerate() {
        eprintln!("spoke {i}: {}", label(spoke));
        spoke_hashes.push(fingerprint::fingerprint_edition(&pages::read_edition(
            spoke,
        )?)?);
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

/// Describe one edition for the sidecar: its members, their content hashes,
/// and which of its pages are joined spreads.
fn describe_edition(label: &str, members: &[PathBuf], slots: &[Slot]) -> Result<Edition, String> {
    let mut described = Vec::new();
    for member in members {
        described.push(Member {
            path: member.to_string_lossy().to_string(),
            sha256: pages::member_digest(member)?,
            pages: pages::member_pages(member)?,
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
    })
}

/// Resolve pages through a stored sidecar, with no archives and no solving.
///
/// This exists to prove the format carries what the solver found: the answers
/// here must match what `align` printed, from the file alone.
fn lookup(args: &[String]) -> Result<(), String> {
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
    ordered.sort_by(|a, b| naming::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
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
    let (a_members, b_members) = split_editions(args)?;
    let split_spreads = args.iter().any(|a| a == "--split-spreads");
    let show_stats = args.iter().any(|a| a == "--stats");

    if !split_spreads {
        let a_pages = pages::read_edition(&a_members)?;
        let b_pages = pages::read_edition(&b_members)?;
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
    let cache_root =
        std::path::PathBuf::from(option_value(args, "--cache").unwrap_or(".taiyaku-cache"));
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

    if let Some(out) = option_value(args, "--write") {
        let direct = args.iter().any(|a| a == "--direct");
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
        let a_name = match (&existing, option_value(args, "--name-a")) {
            (Some(_), None) if direct => {
                return Err(
                    "--direct needs --name-a to say which edition the edge starts from".into(),
                );
            }
            (Some(sidecar), None) => sidecar.hub.clone(),
            (Some(_), Some(given)) => given.to_string(),
            (None, given) => slug(&a_members, given),
        };
        let b_name = slug(&b_members, option_value(args, "--name-b"));
        if a_name == b_name {
            return Err(format!(
                "both editions would be named `{a_name}`; pass --name-a and --name-b"
            ));
        }

        let a_edition = describe_edition(&label(&a_members), &a_members, &a_slots)?;
        let b_edition = describe_edition(&label(&b_members), &b_members, &b_slots)?;
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
                if let Some(title) = option_value(args, "--title")
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
                    Merged::Added => "added",
                    Merged::Replaced => "replaced",
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
                        title: option_value(args, "--title")
                            .unwrap_or("Untitled work")
                            .to_string(),
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
        std::fs::write(out, sidecar.to_json()).map_err(|e| format!("{out}: {e}"))?;
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
