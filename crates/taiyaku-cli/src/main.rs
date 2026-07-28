//! Command-line aligner for taiyaku.
//!
//!   taiyaku align <a-member>... -- <b-member>...
//!
//! Each side is an *edition*: one or more archives or directories that flatten
//! into a single page sequence. `--` separates the two, so a raw volume can be
//! aligned against the several chapter files that make it up.
//!
//! Fingerprint caching and sidecar output land next; this prints the runs.

mod fingerprint;
mod naming;
mod pages;
mod report;
mod spread;

use std::path::PathBuf;

use taiyaku_core::{Run, compose, confidence, counterpart, solve};

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
        Some(other) => Err(format!("unknown command `{other}`")),
        None => {
            println!("taiyaku {}", env!("CARGO_PKG_VERSION"));
            println!("usage: taiyaku align  <a-member>... -- <b-member>...");
            println!("       taiyaku verify <a-member>... -- <b-member>... [--out FILE]");
            println!("       taiyaku topology <hub>... -- <spoke>... -- <spoke>...");
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

    let runs = solve(&a, &b);
    let scored = confidence(&a, &b, &runs);
    let html = report::render(
        &label(&a_members),
        &label(&b_members),
        &a_thumbs,
        &b_thumbs,
        &scored,
        runs.len(),
    );
    std::fs::write(out, &html).map_err(|e| format!("{out}: {e}"))?;

    report_summary(&runs, a.len());
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
        "{:<9} {:>6} {:>7} {:>8} {:>9} {:>9}",
        "pair", "pages", "direct", "loss", "disagree", "inverse"
    );
    let (mut total_pages, mut total_loss, mut total_disagree, mut total_inverse) = (0, 0, 0, 0);

    for i in 0..spokes.len() {
        for j in 0..spokes.len() {
            if i == j {
                continue;
            }
            let direct = solve(&spoke_hashes[i], &spoke_hashes[j]);
            let composed = compose(&to_hub[i], &from_hub[j]);

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
                "{i}->{j:<7} {pages:>6} {direct_paired:>7} {:>7.1}% {:>8.1}% {:>8.1}%",
                100.0 * loss as f64 / pages as f64,
                100.0 * disagree as f64 / pages as f64,
                100.0 * inverse as f64 / pages as f64,
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
    Ok(())
}

fn align(args: &[String]) -> Result<(), String> {
    let (a_members, b_members) = split_editions(args)?;
    let split_spreads = args.iter().any(|a| a == "--split-spreads");

    let a_pages = pages::read_edition(&a_members)?;
    let b_pages = pages::read_edition(&b_members)?;
    let a = fingerprint::fingerprint_edition(&a_pages)?;

    if !split_spreads {
        let b = fingerprint::fingerprint_edition(&b_pages)?;
        eprintln!("A: {} pages | B: {} pages", a.len(), b.len());
        let runs = solve(&a, &b);
        report_summary(&runs, a.len());
        return Ok(());
    }

    // Prototype: cut joined spreads in half on BOTH sides before solving.
    // Which edition joins a spread is not fixed - raws split them and English
    // releases join them here, but the colour editions join them too, so the
    // same pair read the other way round puts the spreads on the other side.
    let direction = if args.iter().any(|a| a == "--ltr") {
        spread::Direction::LeftToRight
    } else {
        spread::Direction::RightToLeft
    };
    let (a_slots_hashes, a_slots) = fingerprint::fingerprint_edition_split(&a_pages, direction)?;
    let (b, b_slots) = fingerprint::fingerprint_edition_split(&b_pages, direction)?;
    let a_wide = a_slots.iter().filter(|s| s.half.is_some()).count() / 2;
    let b_wide = b_slots.iter().filter(|s| s.half.is_some()).count() / 2;
    eprintln!(
        "A: {} pages in {} slots ({a_wide} spreads) | B: {} pages in {} slots ({b_wide} spreads)",
        a_pages.len(),
        a_slots_hashes.len(),
        b_pages.len(),
        b.len()
    );

    let runs = solve(&a_slots_hashes, &b);
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
    eprintln!(
        "{} runs | {whole} whole pairings | {halves} involving a spread half | {gaps} unmatched slots",
        runs.len()
    );
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
    // one is confetti at roughly the same coverage. Measured on the corpus,
    // every correct pairing sits at or below 8 runs per hundred pages and every
    // known-bad one at or above 22 — but that gap is drawn from four series and
    // is a diagnostic to show the user, not yet a threshold to refuse on.
    let per_hundred = 100.0 * runs.len() as f64 / a_len.max(1) as f64;
    eprintln!(
        "{} runs | {paired}/{a_len} pages paired ({:.1}%) | {per_hundred:.1} runs per 100 pages",
        runs.len(),
        100.0 * paired as f64 / a_len.max(1) as f64
    );
}
