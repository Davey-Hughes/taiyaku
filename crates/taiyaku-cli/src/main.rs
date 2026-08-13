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

// Task 7 of the detector redesign: where the component-size band goes, scored
// over the whole dev set at several bands. Gated on TAIYAKU_CORPUS exactly as
// `corpus`, `geometry_experiment`, `region_experiment` and `truth_population`
// are, and declared the same way, so no part of it is compiled into the binary.
//
// Unlike the two experiments below it takes no module-level lint suppression:
// it does its arithmetic through `score::Report`, which already carries the
// per-site #[expect] for the one division involved.
#[cfg(test)]
mod band_sweep;
#[cfg(test)]
mod borrow_frequency;
mod cache;
#[cfg(test)]
mod corpus;
mod credits;
// Only `credits.rs`'s own tests consume this, and the crate runs clippy at
// pedantic level — an ungated module would be dead code outside test builds
// and fail that gate.
#[cfg(test)]
mod credit_fixture;
mod detect;
mod duplicates;
mod fingerprint;
// A one-off measurement rather than a shipped command, and gated on
// `TAIYAKU_CORPUS` exactly as `corpus` is — so it is declared the same way, and
// no part of it is compiled into the binary.
//
// It also carries one of this workspace's two module-level lint
// suppressions — the other is `region_experiment`'s, below, granted on the
// same grounds. Everything else pedantic flags is fixed or carries a
// per-site #[expect] stating its bound; §10.3 of the design forbids
// module-level allows precisely so that a file with many casts cannot
// switch the lint off wholesale. The exception is granted on the same
// grounds the coverage gate already excludes this file by name: its
// arithmetic exists to print a table for docs/notes.md once, and writing 29
// bounds for it would spend the mechanism's credibility on the code least
// able to hurt anyone.
//
// The cost, stated: a truncating cast introduced here is invisible. That is
// accepted because a wrong number in a measurement shows up as a wrong number
// in the table it prints, which a person reads before it reaches notes.md.
#[cfg(test)]
#[allow(
    clippy::pedantic,
    reason = "one-off measurement; see above and docs/ci.md"
)]
mod geometry_experiment;
mod inventory;
mod regions;
// `sample.rs`'s three items (`DEV_EDITIONS`, `HELD_OUT_EDITIONS`, `dev_pages`)
// used to be read by `score_corpus.rs` alone, and were gated on `cfg(test)`
// for exactly that reason — a `mod sample;` compiled unconditionally would
// have left those three genuinely unused in the plain `bin "taiyaku"` build.
// `score --sample` (`score.rs`) now reads them from that plain build itself,
// to print the labelling worklist a human runs the binary to see, so the
// build this comment used to describe no longer exists: the always-on caller
// it said would justify removing the gate has arrived.
mod sample;
mod score;
#[cfg(test)]
mod score_corpus;
// A one-off measurement rather than a shipped command, gated on
// `TAIYAKU_CORPUS` exactly as `corpus` and `geometry_experiment` are — so it
// is declared the same way, and no part of it is compiled into the binary.
//
// It also carries the second of this workspace's two module-level lint
// suppressions — `geometry_experiment`'s, above, is the other, granted on
// the same grounds. Everything else pedantic flags is fixed or carries a
// per-site #[expect] stating its bound; §10.3 of the design forbids
// module-level allows precisely so that a file with many casts cannot
// switch the lint off wholesale. The exception is granted on the same
// grounds the coverage gate already excludes this file by name: its
// arithmetic exists to print a table for docs/notes.md once, and writing 16
// bounds for it would spend the mechanism's credibility on the code least
// able to hurt anyone.
//
// The cost, stated: a truncating cast introduced here is invisible. That is
// accepted because a wrong number in a measurement shows up as a wrong number
// in the table it prints, which a person reads before it reaches notes.md.
#[cfg(test)]
#[allow(
    clippy::pedantic,
    reason = "one-off measurement; see above and docs/ci.md"
)]
mod region_experiment;

// Phase 1 of the detector redesign: what text is made of, measured from the
// boxes a person drew. Gated on TAIYAKU_CORPUS exactly as `corpus`,
// `geometry_experiment` and `region_experiment` are, and declared the same
// way, so no part of it is compiled into the binary.
#[cfg(test)]
mod truth_population;

// Phase 3 of the detector redesign: the model candidate, scored against the
// same 45 pages as phase 2's sweep. Gated the same way and for the same
// reason, and additionally on TAIYAKU_MODEL, because the weights are 95 MB and
// are fetched rather than committed.
#[cfg(test)]
#[allow(
    clippy::pedantic,
    reason = "one-off measurement; see docs/ci.md and region_experiment.rs"
)]
mod model_experiment;

mod report;
mod signature;
mod stats;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use taiyaku_core::{
    Alignment, Anchor, Direction, Edition, Half, Member, Merged, Run, Sidecar, Slot, SolverInfo,
    Source, Verdict, Via, Work, compose, confidence, counterpart, entries_from_runs, solve,
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
        Some("score") => score::command(&args[1..]),
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
                 [--stump-key KEY] [--log-requests FILE] [--refresh-stump] [--cache DIR]"
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
            // `--sidecar` is a flag, not a positional: `score` refuses to run
            // without it, and a bare path in its place would be taken for a
            // member and die in `member_digest`. `--sample` gets its own line
            // because it is the only way to discover which pages to label, and
            // a person who cannot see it here has no way to find the worklist.
            // `score.rs`'s own `USAGE` is the other statement of this; the two
            // must not disagree.
            println!("       taiyaku score <member>... --sidecar FILE [--coverage F]");
            println!("       taiyaku score --sample <corpus-root>");
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
/// Whether a bare flag was given.
///
/// One function rather than an `any(|a| a == "--x")` written out at each site.
/// The comparison is the whole of the decision and it is the same decision
/// every time — inverted, a flag is on exactly when it was not passed, and
/// nothing about the run looks wrong.
fn flag_present(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Which way the work reads, which decides which half of a spread comes first.
///
/// Right to left is the default because it is what every work in the corpus is;
/// `--ltr` is the exception a caller asks for by name.
fn reading_direction(args: &[String]) -> Direction {
    if flag_present(args, "--ltr") {
        Direction::LeftToRight
    } else {
        Direction::RightToLeft
    }
}

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
    if known.is_empty() {
        Err(format!(
            "{command} takes no flags, and `{unknown}` was given as one"
        ))
    } else {
        Err(format!(
            "{command} has no flag `{unknown}`; its flags are {}",
            known.join(", ")
        ))
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
    if trimmed.is_empty() {
        "edition".to_string()
    } else {
        trimmed
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
    if trimmed.is_empty() {
        "edition".to_string()
    } else {
        trimmed.chars().take(48).collect()
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

/// Resolve `verify`'s runs and their addressing unit from a `--sidecar`
/// document, rather than solving `a` against `b` directly.
///
/// Split out of `verify`, which otherwise reaches `too_many_lines` on this
/// branch alone: which two editions `path` names is asked of the archives
/// (`named`, below) rather than assumed, and that check — plus the route
/// lookup and the `by`/`how` diagnostic line — is a single self-contained
/// question with its own doc comments, not a piece of the render-and-write
/// flow around it.
///
/// `a_len` is `a.len()`: the page count of the already-read a-side, used only
/// as the slot-count fallback when `path`'s edition has no slot table of its
/// own to consult.
fn runs_from_sidecar(
    path: &str,
    a_members: &[PathBuf],
    b_members: &[PathBuf],
    a_len: usize,
) -> Result<(Vec<Run>, usize), String> {
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
                    digests.first().map_or("nothing", |d| &d[..8.min(d.len())]),
                )
            })
    };
    let a_name = named(a_members, "a-side")?;
    let b_name = named(b_members, "b-side")?;

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
        .edge_between(&a_name, &b_name)
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
        .map_or(a_len, |e| e.slots(sidecar.work.reading).len());
    Ok((route.runs().to_vec(), slots))
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
    let direction = reading_direction(args);
    // The summary at the end is counted in whatever unit the runs are addressed
    // in. A solved run indexes pages; a run out of a sidecar indexes slots, and
    // an edition with seven spreads has seven more slots than pages — which is
    // how the same summary reported 192 of 187 pages paired, at 102.7%.
    let (runs, a_units) = match sidecar_path {
        Some(path) => runs_from_sidecar(path, &a_members, &b_members, a.len())?,
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
            on_pages(slot_scores, &a_slots, &b_slots)
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

    let (table, figures) = report_summary(&runs, a_units);
    print!("{table}");
    eprintln!("{figures}");
    #[expect(
        clippy::cast_precision_loss,
        reason = "html.len() is the byte length of one generated report; f64 stops \
                  representing every integer exactly above 2^53 (~8 petabytes), and no \
                  report this tool writes is within many orders of magnitude of that. \
                  The lossless-From route is closed too: html.len() is usize, and \
                  std has no From<usize> for f64 — usize's width is \
                  platform-dependent, unlike u32/u16/u8, so no such impl exists."
    )]
    let html_mb = html.len() as f64 / 1_048_576.0;
    eprintln!(
        "wrote {out} ({html_mb:.1} MB) | {} ambiguous pairings to review",
        ambiguous_pairings(&scored)
    );
    Ok(())
}

/// Slot-indexed confidence, mapped back onto the pages the report shows.
///
/// A run out of a sidecar addresses slots, and an edition with seven spreads
/// has seven more slots than pages — but the report shows thumbnails, and a
/// thumbnail is of a page. Both ends of a pairing move: the score's own slot
/// becomes the page holding it, and its counterpart slot becomes the page
/// holding *that*. Leaving either as a slot index puts a page's score against
/// another page's picture, off by the number of spreads before it.
///
/// A score whose slot is not in the table is dropped rather than guessed at.
fn on_pages(
    slot_scores: Vec<taiyaku_core::PageConfidence>,
    a_slots: &[Slot],
    b_slots: &[Slot],
) -> Vec<taiyaku_core::PageConfidence> {
    slot_scores
        .into_iter()
        .filter_map(|s| {
            Some(taiyaku_core::PageConfidence {
                page: a_slots.get(s.page as usize)?.page,
                counterpart: s
                    .counterpart
                    .and_then(|c| b_slots.get(c as usize))
                    .map(|slot| slot.page),
                ..s
            })
        })
        .collect()
}

/// How close the second-best counterpart came before a pairing counts as
/// worth a person's eye.
const AMBIGUOUS_MARGIN: f32 = 0.05;

/// Pairings the report asks someone to look at: paired, and paired only just.
///
/// An unpaired page is not ambiguous — there was no second-best for it to be
/// close to — so the count is over pairings rather than over pages, and the
/// number is printed as "to review" beside a file somebody is about to open.
fn ambiguous_pairings(scored: &[taiyaku_core::PageConfidence]) -> usize {
    scored
        .iter()
        .filter(|s| s.counterpart.is_some() && s.margin < AMBIGUOUS_MARGIN)
        .count()
}

/// A page count, as `Divergence::between` takes it.
///
/// # Panics
///
/// Panics if `n` exceeds `u32::MAX`. It cannot here: every call site derives
/// `n` from `.len()` of one spoke's fingerprinted pages, a count bounded by
/// what a physical book can contain — nowhere near `u32::MAX`.
fn page_count(n: usize) -> u32 {
    u32::try_from(n).expect("page count exceeds u32::MAX")
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
    let measured: Vec<((usize, usize), PairReport)> = ordered_pairs(spokes.len())
        .into_iter()
        .map(|(i, j)| {
            let direct = solve(&spoke_hashes[i], &spoke_hashes[j]);
            let composed = compose(&to_hub[i], &from_hub[j]);
            let report = PairReport {
                pages: spoke_hashes[i].len(),
                divergence: Divergence::between(
                    &direct,
                    &composed,
                    page_count(spoke_hashes[i].len()),
                ),
                composed: taiyaku_core::verdict(&composed, spoke_hashes[i].len()),
                worse_leg: taiyaku_core::verdict(&to_hub[i], spoke_hashes[i].len())
                    .min(taiyaku_core::verdict(&from_hub[j], hub_hashes.len())),
            };
            ((i, j), report)
        })
        .collect();

    for ((i, j), report) in &measured {
        let (pages, d) = (report.pages, report.divergence);
        #[expect(
            clippy::cast_precision_loss,
            reason = "pages is an edition's page count, bounded by what a physical book can \
                      contain; f64 only loses precision above 2^53. No From<usize> for f64 \
                      exists either — usize's width is platform-dependent, unlike u32/u16/u8."
        )]
        let pages_f64 = pages as f64;
        println!(
            "{i}->{j:<7} {pages:>6} {:>7} {:>7.1}% {:>8.1}% {:>8.1}% {:>21}",
            d.direct_paired,
            100.0 * f64::from(d.loss) / pages_f64,
            100.0 * f64::from(d.disagree) / pages_f64,
            100.0 * f64::from(d.inverse) / pages_f64,
            report.verdict_column()
        );
    }

    // Composition can merge two fragmented runs into one clean one, so judging
    // the composed runs is not the same as taking the worse of the two legs.
    // `Sidecar::trust` takes the worse leg; this counts how often that is the
    // more pessimistic answer, which is what decides whether it should.
    let verdict_splits = measured
        .iter()
        .filter(|(_, report)| report.verdict_split())
        .count();
    let total_pages: usize = measured.iter().map(|(_, report)| report.pages).sum();
    let totals = measured
        .iter()
        .fold(Divergence::default(), |sum, (_, report)| {
            sum.plus(report.divergence)
        });

    #[expect(
        clippy::cast_precision_loss,
        reason = "total_pages sums a handful of editions' page counts, each bounded by what \
                  a physical book can contain; even summed, they come nowhere near 2^53. \
                  No From<usize> for f64 exists either — usize's width is platform-dependent."
    )]
    let total_pages_f64 = total_pages as f64;
    eprintln!(
        "\nacross {total_pages} page comparisons: loss {:.2}% | disagreement {:.2}% | inverse loss {:.2}%",
        100.0 * f64::from(totals.loss) / total_pages_f64,
        100.0 * f64::from(totals.disagree) / total_pages_f64,
        100.0 * f64::from(totals.inverse) / total_pages_f64,
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

/// Every ordered pair of distinct spokes.
///
/// Ordered, not unordered: `i -> j` composes through the hub as `i -> hub`
/// then `hub -> j`, and `j -> i` uses the other two legs, so the two are
/// different measurements and the table reports both. A star of `n` spokes
/// gives `n * (n - 1)` of them, which is the number the header prints as what
/// all-pairs would have cost.
fn ordered_pairs(spokes: usize) -> Vec<(usize, usize)> {
    (0..spokes)
        .flat_map(|i| (0..spokes).map(move |j| (i, j)))
        .filter(|(i, j)| i != j)
        .collect()
}

/// How a direct alignment and a composed one differ, page by page.
///
/// The four counts are not slices of one number: a page can be paired by the
/// direct solve and lost by composition, paired by composition and missed by
/// the direct solve, or paired by both to different counterparts. Only the
/// first is a cost of the star, which is why they are counted apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Divergence {
    /// Pages the direct solve pairs at all.
    direct_paired: u32,
    /// Pages the direct solve pairs and composition does not — the number that
    /// decides whether hub-and-spoke is worth what it saves.
    loss: u32,
    /// Pages composition pairs and the direct solve does not.
    inverse: u32,
    /// Pages both pair, to different counterparts.
    disagree: u32,
}

impl Divergence {
    fn between(direct: &[Run], composed: &[Run], pages: u32) -> Self {
        let mut found = Self::default();
        for page in 0..pages {
            let (d, c) = (counterpart(direct, page), counterpart(composed, page));
            if d.is_some() {
                found.direct_paired += 1;
            }
            match (d, c) {
                (Some(_), None) => found.loss += 1,
                (None, Some(_)) => found.inverse += 1,
                (Some(x), Some(y)) if x != y => found.disagree += 1,
                _ => {}
            }
        }
        found
    }

    /// Running totals across the whole star.
    fn plus(self, other: Self) -> Self {
        Self {
            direct_paired: self.direct_paired + other.direct_paired,
            loss: self.loss + other.loss,
            inverse: self.inverse + other.inverse,
            disagree: self.disagree + other.disagree,
        }
    }
}

/// One row of the topology table: what composition cost, and what the two ways
/// of judging it said.
#[derive(Debug, Clone, Copy)]
struct PairReport {
    pages: usize,
    divergence: Divergence,
    /// The verdict on the composed runs, judged as a whole.
    composed: Verdict,
    /// What `Sidecar::trust` would say: the worse of the two legs.
    worse_leg: Verdict,
}

impl PairReport {
    /// Whether judging the composition disagrees with taking the worse leg.
    fn verdict_split(&self) -> bool {
        self.composed != self.worse_leg
    }

    /// The verdict column: one word where the two rules agree, and both words
    /// where they do not — the disagreement is the measurement.
    fn verdict_column(&self) -> String {
        if self.verdict_split() {
            format!(
                "{} != {}",
                stats::verdict_word(self.composed),
                stats::verdict_word(self.worse_leg)
            )
        } else {
            stats::verdict_word(self.composed).to_string()
        }
    }
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
    Ok(if relative.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        relative
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
    let [path, from, to, ..] = args else {
        return Err("usage: taiyaku lookup <taiyaku.json> <from> <to> <page>...".into());
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
                let verdict = route_verdict(&sidecar, via, from, to);
                let how = route_note(via, from, to);
                match verdict {
                    Some(v) => eprintln!(
                        "  alignment {from} -> {to}: {}, average run {:.1}{how}",
                        stats::verdict_word(v),
                        taiyaku_core::average_run_length(found.runs())
                    ),
                    None => {
                        eprintln!("  alignment {from} -> {to}: this document does not say{how}");
                    }
                }
            }
        },
        // Nothing to say about the pairing — but if an edge is there and
        // unreadable, `route` swallowed the reason. Those messages are written
        // for whoever is hand-editing the file, and this is the only place they
        // can reach them.
        None => match sidecar
            .edge_between(from, to)
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
            "--cache",
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
    // Read before `load_works` so `serve x.json --cache` with nothing after
    // it is refused for the reason that is actually wrong, rather than for a
    // sidecar that may also be missing.
    //
    // It names the *root*, and `taiyaku-serve` derives `covers` and `thumbs`
    // under it — the same convention `align --cache .taiyaku-cache` uses.
    // Naming the covers directory instead would put the tiles in a sibling of
    // whatever was typed rather than inside it.
    let cache = option_value(args, "--cache", "a directory")?.map(PathBuf::from);
    // Throws away every work's remembered Stump resolutions before this start
    // resolves them again. The escape hatch for a book added to Stump after a
    // work first resolved, which nothing here notices on its own.
    let refresh_stump = flag_present(args, "--refresh-stump");

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
        .block_on(taiyaku_serve::serve(
            works,
            addr,
            stump,
            log,
            refresh_stump,
            cache,
        ))
}

/// How believable a route is, which depends on how it was reached.
///
/// A composed path is only as good as its weaker leg, which is what `trust`
/// reports; a stored edge — either orientation of one — answers for itself
/// through `quality_of`. Asking `quality_of` about a composed pair would find
/// no edge and report that the document does not say, on a pairing the
/// document says plenty about.
fn route_verdict(sidecar: &Sidecar, via: &Via, from: &str, to: &str) -> Option<Verdict> {
    match via {
        Via::Hub(_) => sidecar.trust(from, to),
        _ => sidecar.quality_of(from, to).map(|q| q.verdict),
    }
}

/// How a route was reached, as the line names it.
///
/// Empty for a direct edge, because there is nothing to explain: the reader
/// asked for `from -> to` and that is what was stored. The other two are worth
/// a clause each — an inverted edge means the runs were read backwards, and a
/// composed one means no solver ever checked this pair.
fn route_note(via: &Via, from: &str, to: &str) -> String {
    match via {
        Via::Inverted => format!(", from the stored {to} -> {from} edge"),
        Via::Hub(hub) => format!(", composed through {hub}"),
        _ => String::new(),
    }
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

    let (hashes, wide) = slot_hashes(&described, direction);
    Ok((hashes, taiyaku_core::expand(&wide, direction), hits))
}

/// One hash per slot, splitting the pages that read as joined spreads, and the
/// flags saying which pages were split.
///
/// Whether a page is a spread is decided against the edition it sits in, so it
/// cannot be cached per member: the baseline has to be taken over the whole
/// flattened sequence, which is why this takes the edition rather than a page.
///
/// A page can be wide enough to read as a spread and still have no halves
/// cached — an entry written before halves were stored — and that page
/// contributes its whole hash rather than nothing, which is why the two
/// conditions are one pattern rather than two nested tests.
fn slot_hashes(
    described: &[cache::CachedPage],
    direction: Direction,
) -> (Vec<taiyaku_core::PageHash>, Vec<bool>) {
    let dimensions: Vec<(u32, u32)> = described.iter().map(|p| (p.width, p.height)).collect();
    let flags = taiyaku_core::spread_flags(&dimensions);

    let mut hashes = Vec::new();
    let mut wide = Vec::new();
    for (page, spread) in described.iter().zip(&flags) {
        if let (true, Some((left, right))) = (*spread, page.halves) {
            // Reading order, matching the slot table.
            match direction {
                Direction::RightToLeft => hashes.extend([right, left]),
                Direction::LeftToRight => hashes.extend([left, right]),
            }
            wide.push(true);
        } else {
            hashes.push(page.hash);
            wide.push(false);
        }
    }
    (hashes, wide)
}

/// The parsed and validated flags `align` runs on, resolved once before
/// either edition is read.
///
/// Split out of `align` itself so that ~130 lines of argument checking —
/// none of which touches an archive or a solver — are not counted as part of
/// the function that does. `align_flags` reads only `args`; `align` reads
/// only an already-validated `AlignFlags`.
struct AlignFlags<'a> {
    a_members: Vec<PathBuf>,
    b_members: Vec<PathBuf>,
    split_spreads: bool,
    show_stats: bool,
    direct: bool,
    name_a: Option<&'a str>,
    name_b: Option<&'a str>,
    write: Option<&'a str>,
    title: Option<&'a str>,
    cache_root: PathBuf,
}

/// Parse and validate every flag `align` accepts, in the order the checks
/// have always run in. Nothing here reads an archive.
fn align_flags(args: &[String]) -> Result<AlignFlags<'_>, String> {
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
    let split_spreads = !flag_present(args, "--no-split-spreads");
    let show_stats = flag_present(args, "--stats");
    let direct = flag_present(args, "--direct");

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

    Ok(AlignFlags {
        a_members,
        b_members,
        split_spreads,
        show_stats,
        direct,
        name_a,
        name_b,
        write,
        title,
        cache_root,
    })
}

/// Everything the `--write` branch of `align` needs, once solving is done.
///
/// Bundled into one struct rather than passed as thirteen positional
/// arguments — the count `clippy::too_many_arguments` (on by default, not
/// merely pedantic) would refuse — so `write_aligned_sidecar` stays a
/// function of one parameter and `align` stays the one place these values are
/// named.
struct AlignWrite<'a> {
    out: &'a str,
    name_a: Option<&'a str>,
    name_b: Option<&'a str>,
    title: Option<&'a str>,
    direct: bool,
    direction: Direction,
    a_members: &'a [PathBuf],
    b_members: &'a [PathBuf],
    a_slots: &'a [Slot],
    b_slots: &'a [Slot],
    a_slots_hashes: &'a [taiyaku_core::PageHash],
    b_hashes: &'a [taiyaku_core::PageHash],
    runs: &'a [Run],
}

/// A slot index, as `counterpart` takes it.
///
/// # Panics
///
/// Panics if `i` exceeds `u32::MAX`. It cannot here: `i` comes from
/// `.enumerate()` over `a_slots`, so `i < a_slots.len()`, a slot count
/// bounded by what a physical book can contain — nowhere near `u32::MAX`.
fn slot_index(i: usize) -> u32 {
    u32::try_from(i).expect("slot index exceeds u32::MAX")
}

/// Build or merge into the sidecar `align --write` names, from the runs
/// `align` just solved.
///
/// Split out of `align` itself, which otherwise reaches `too_many_lines` on
/// this block alone — the merge-or-create decision below cannot shrink
/// without either hiding a case `docs/sidecar.md` distinguishes, or naming it
/// a second time in a doc comment where a reader has to trust it still
/// matches the code below it.
/// Print the two diagnostics `align` shows on its split-spreads path: the
/// page-to-page pairing table and the runs restated in slot notation.
///
/// Split out of `align` for the same reason as `write_aligned_sidecar`
/// above: this is pure output over `a_slots`, `b_slots` and `runs`, computes
/// nothing the `--write` branch after it needs, and its own two local
/// closures (`show`, `span`) exist only to keep the printing below them
/// short.
fn print_alignment_detail(a_slots: &[Slot], b_slots: &[Slot], runs: &[Run]) {
    let paired: Vec<(&Slot, Option<&Slot>)> = a_slots
        .iter()
        .enumerate()
        .map(|(i, from)| {
            let to = counterpart(runs, slot_index(i)).and_then(|slot| b_slots.get(slot as usize));
            (from, to)
        })
        .collect();
    for (from, to) in &paired {
        if let Some(to) = to {
            println!(
                "{:>4}{:<6} -> {:>4}{}",
                from.page,
                from.half.map(|h| format!(" {h:?}")).unwrap_or_default(),
                to.page,
                to.half.map(|h| format!(" {h:?}")).unwrap_or_default()
            );
        }
    }
    let kinds: Vec<Pairing> = paired
        .iter()
        .map(|(from, to)| pairing_of(**from, to.copied()))
        .collect();
    let (whole, halves, gaps) = tally(&kinds);
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
    for run in runs {
        let a_span = span(a_slots, run.a.start, run.a.end);
        match run.b {
            Some(b) => eprintln!("  {a_span:>22} -> {}", span(b_slots, b.start, b.end)),
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
    eprintln!("{}", stats::verdict_sentence(runs, a_slots.len()));
}

/// The `Alignment` a `--write`'d merge or create records: the solver's floor,
/// the quality summary, and the per-run entries with their margins filled in.
///
/// Split out of `write_aligned_sidecar`, which reaches `too_many_lines` with
/// this construction inlined — reformatting elsewhere in that function is
/// enough to push it over on its own, since this literal's fields do not
/// shrink. `a_name`/`b_name` are taken apart from `w` because they are
/// computed just before this is called, from `w.name_a`/`w.name_b` and
/// `existing`, and are not otherwise part of `AlignWrite`.
fn built_alignment(a_name: &str, b_name: &str, w: &AlignWrite<'_>) -> Result<Alignment, String> {
    Ok(Alignment {
        from: a_name.to_string(),
        to: b_name.to_string(),
        by: Source::Solver,
        // The floor the solver actually used, not a placeholder: it is
        // calibrated from these two editions and a re-solve needs it to be
        // reproducible.
        // Only a calibration the solver accepted, so a refused pair records
        // no solver block rather than a floor that was never used.
        // `calibrate` reports its figures either way; `accepted` is what
        // decides whether they drove an alignment.
        solver: taiyaku_core::calibrate(w.a_slots_hashes, w.b_hashes)
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
            verdict: taiyaku_core::verdict(w.runs, w.a_slots.len()),
            slots: u32::try_from(w.a_slots.len())
                .map_err(|_| format!("{} slots is too many to record", w.a_slots.len()))?,
            paired: taiyaku_core::paired_slots(w.runs),
            avg_run: taiyaku_core::rounded_avg_run(taiyaku_core::average_run_length(w.runs)),
        }),
        runs: {
            // The weakest per-page margin inside each run, which is what
            // the verification report shades on. Storing it means a reader
            // can show confidence without the archives.
            let scored = confidence(w.a_slots_hashes, w.b_hashes, w.runs);
            let mut entries = entries_from_runs(w.runs, w.a_slots, w.b_slots)?;
            for (entry, run) in entries.iter_mut().zip(w.runs) {
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
    })
}

fn write_aligned_sidecar(w: &AlignWrite<'_>) -> Result<(), String> {
    // Read before writing. `--write` used to be a bare `fs::write`, which
    // silently discarded a document that may hold four other editions.
    let existing = existing_sidecar(w.out)?;

    // On a merge the a-side is the edition the document already names, so
    // `--name-a` is optional and, given, must agree. `--direct` has no hub
    // to fall back on and so requires it.
    let a_name = match (&existing, w.name_a) {
        (Some(_), None) if w.direct => {
            return Err("--direct needs --name-a to say which edition the edge starts from".into());
        }
        (Some(sidecar), None) => sidecar.hub.clone(),
        (Some(_), Some(given)) => given.to_string(),
        (None, given) => slug(w.a_members, given),
    };
    let b_name = slug(w.b_members, w.name_b);
    if a_name == b_name {
        return Err(format!(
            "both editions would be named `{a_name}`; pass --name-a and --name-b"
        ));
    }

    // Member paths are recorded relative to the sidecar, so the writer has
    // to know where the sidecar is going before it can describe an edition.
    let beside = sidecar_dir(std::path::Path::new(w.out));
    let a_edition = describe_edition(&label(w.a_members), w.a_members, w.a_slots, beside)?;
    let b_edition = describe_edition(&label(w.b_members), w.b_members, w.b_slots, beside)?;
    let alignment = built_alignment(&a_name, &b_name, w)?;

    let (sidecar, note) = match existing {
        Some(mut sidecar) => {
            if let Some(title) = w.title
                && title != sidecar.work.title
            {
                return Err(format!(
                    "{} is the sidecar for `{}`; --title cannot rename a work on a merge",
                    w.out, sidecar.work.title
                ));
            }
            let anchor = if w.direct {
                Anchor::Direct
            } else {
                Anchor::Hub
            };
            let merged = sidecar.merge_spoke(
                w.direction,
                anchor,
                &a_edition,
                &b_name,
                b_edition,
                alignment,
            )?;
            let verb = match merged {
                Merged::Added => "added".to_string(),
                Merged::Replaced => "replaced".to_string(),
                Merged::CarriedHandRuns(n) => format!("replaced, carrying {n} hand run(s)"),
                Merged::DroppedCredits(n) => {
                    format!("replaced, dropping {n} credits page(s)")
                }
            };
            let note = format!(
                "merged {}: {verb} `{b_name}` | {} editions, {} alignments",
                w.out,
                sidecar.editions.len(),
                sidecar.alignments.len()
            );
            (sidecar, note)
        }
        None => (
            Sidecar {
                taiyaku: taiyaku_core::VERSION,
                work: Work {
                    title: w.title.unwrap_or("Untitled work").to_string(),
                    reading: w.direction,
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
            format!("wrote {}", w.out),
        ),
    };
    write_sidecar(w.out, &sidecar)?;
    eprintln!("{note}");
    Ok(())
}

fn align(args: &[String]) -> Result<(), String> {
    let AlignFlags {
        a_members,
        b_members,
        split_spreads,
        show_stats,
        direct,
        name_a,
        name_b,
        write,
        title,
        cache_root,
    } = align_flags(args)?;

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
    let direction = reading_direction(args);
    let started = std::time::Instant::now();
    let (a_slots_hashes, a_slots, a_hits) = cached_edition(&cache_root, &a_members, direction)?;
    let (b, b_slots, b_hits) = cached_edition(&cache_root, &b_members, direction)?;
    eprintln!(
        "fingerprints in {:?} | {} of {} members from cache",
        started.elapsed(),
        a_hits + b_hits,
        a_members.len() + b_members.len()
    );
    let a_wide = spreads_in(&a_slots);
    let b_wide = spreads_in(&b_slots);
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
    print_alignment_detail(&a_slots, &b_slots, &runs);

    if let Some(out) = write {
        write_aligned_sidecar(&AlignWrite {
            out,
            name_a,
            name_b,
            title,
            direct,
            direction,
            a_members: &a_members,
            b_members: &b_members,
            a_slots: &a_slots,
            b_slots: &b_slots,
            a_slots_hashes: &a_slots_hashes,
            b_hashes: &b,
            runs: &runs,
        })?;
    }
    Ok(())
}

/// How many slots the counterpart search resolved, and to what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pairing {
    /// Two whole pages.
    Whole,
    /// At least one side is half of a joined spread.
    Half,
    /// No counterpart at all.
    Gap,
}

/// What one slot's pairing is, for the tally under the table.
///
/// A pairing counts as involving a spread if *either* side is a half. The two
/// are not symmetric in the data — raws split spreads and English releases
/// join them, and the colour editions join them too — so which side is halved
/// depends on which way round the pair was given, and a rule naming one side
/// would report the same pairing differently in each direction.
fn pairing_of(from: Slot, to: Option<Slot>) -> Pairing {
    match to {
        None => Pairing::Gap,
        Some(to) => {
            if from.half.is_some() || to.half.is_some() {
                Pairing::Half
            } else {
                Pairing::Whole
            }
        }
    }
}

/// The three counts under the table, which have to sum to the slots there are.
///
/// Reported together because they are read together: whole pairings against
/// ones involving a spread half says how much of the alignment turned on the
/// spread detection being right, and neither number means much on its own.
fn tally(pairings: &[Pairing]) -> (usize, usize, usize) {
    let count = |wanted: Pairing| pairings.iter().filter(|p| **p == wanted).count();
    (
        count(Pairing::Whole),
        count(Pairing::Half),
        count(Pairing::Gap),
    )
}

/// How many joined spreads a slot table holds.
///
/// Halves, divided by two: a spread expands to two slots, so counting the
/// halved slots counts each spread twice.
fn spreads_in(slots: &[Slot]) -> usize {
    slots.iter().filter(|s| s.half.is_some()).count() / 2
}

/// The document `--write` will merge into, if there is one.
///
/// Absent is a new document; unreadable is not. `--write` used to be a bare
/// `fs::write`, which silently discarded a file that may hold four other
/// editions — so a read that fails for any reason other than the file not
/// being there has to stop the write, rather than be treated as "nothing to
/// merge" and overwrite whatever is actually at that path.
fn existing_sidecar(out: &str) -> Result<Option<Sidecar>, String> {
    match std::fs::read_to_string(out) {
        Ok(text) => Ok(Some(Sidecar::from_json(&text)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{out}: {e}")),
    }
}

/// A report's summary: the per-run table, and the figure line under it.
///
/// Returned as two strings rather than printed, so both can be asserted. They
/// go to different streams — the run table is the output and the figure line
/// is commentary on it — and the caller keeps them apart, because a survey
/// script reading one must not have to filter out the other.
fn report_summary(runs: &[Run], a_len: usize) -> (String, String) {
    let mut paired = 0u32;
    let mut table = String::new();
    for run in runs {
        match run.b {
            Some(b) => {
                paired += run.a.len();
                let _ = writeln!(
                    table,
                    "{:>4}-{:<4} -> {:>4}-{:<4}  ({} pages, offset {:+})",
                    run.a.start,
                    run.a.end,
                    b.start,
                    b.end,
                    run.a.len(),
                    i64::from(b.start) - i64::from(run.a.start)
                );
            }
            None => {
                let _ = writeln!(
                    table,
                    "{:>4}-{:<4} -> {:^11}  ({} pages)",
                    run.a.start,
                    run.a.end,
                    "gap",
                    run.a.len()
                );
            }
        }
    }
    // Fragmentation, not coverage, is what tells a real alignment from a
    // plausible-looking one. A correct pairing is a few long runs; a spurious
    // one is confetti at roughly the same coverage. Runs per hundred pages is
    // kept here as a diagnostic to show the user, not as the thing judged on:
    // the 18-row survey in docs/notes.md ("The margin survey, and where the
    // refusal threshold sits") retired it in favour of average run length,
    // which `verdict_sentence` reports.
    #[expect(
        clippy::cast_precision_loss,
        reason = "runs.len() cannot exceed a_len (every run consumes at least one A page), \
                  and a_len is a page count off the material, bounded by what a physical \
                  book can contain — nowhere near 2^53, above which f64 stops representing \
                  every integer exactly. The lossless-From route is closed too: \
                  runs.len() is usize, and std has no From<usize> for f64 — usize's \
                  width is platform-dependent, unlike u32/u16/u8, so no such impl exists."
    )]
    let runs_f64 = runs.len() as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a_len is edition A's page count, a fact about the material, bounded by \
                  what a physical book can contain — nowhere near 2^53, above which f64 \
                  stops representing every integer exactly. The lossless-From route is \
                  closed too: a_len is usize, and std has no From<usize> for f64 — \
                  usize's width is platform-dependent, unlike u32/u16/u8, so no such \
                  impl exists."
    )]
    let a_len_f64 = a_len.max(1) as f64;
    let per_hundred = 100.0 * runs_f64 / a_len_f64;
    // The verdict is printed alongside the number because the number alone is
    // misleading: a refusal is one run, which reads as better than any correct
    // alignment on a runs-per-page scale.
    let figures = format!(
        "{} runs | {paired}/{a_len} pages paired ({:.1}%) | {per_hundred:.1} runs per 100 pages | {}",
        runs.len(),
        100.0 * f64::from(paired) / a_len_f64,
        stats::verdict_sentence(runs, a_len)
    );
    (table, figures)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare flag is on when it is there and off when it is not, and that is
    /// the whole of it — but it is the whole of it at every call site, so the
    /// comparison being right once is the comparison being right everywhere.
    #[test]
    fn a_bare_flag_is_on_only_when_it_was_given() {
        let args = |all: &[&str]| all.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();

        assert!(flag_present(&args(&["a.cbz", "--stats"]), "--stats"));
        assert!(!flag_present(&args(&["a.cbz", "--stats"]), "--ltr"));
        assert!(!flag_present(&args(&["a.cbz"]), "--stats"));
        assert!(!flag_present(&[], "--stats"));
        assert!(
            !flag_present(&args(&["--stats-please"]), "--stats"),
            "a longer flag that starts the same is a different flag"
        );
    }

    /// Right to left unless asked otherwise. Everything the corpus holds reads
    /// that way, so the flag is the exception and the default is not a guess.
    #[test]
    fn the_reading_direction_is_right_to_left_unless_the_flag_says_otherwise() {
        let args = |all: &[&str]| all.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();

        assert_eq!(
            reading_direction(&args(&["a.cbz", "b.cbz"])),
            Direction::RightToLeft
        );
        assert_eq!(
            reading_direction(&args(&["a.cbz", "--ltr", "b.cbz"])),
            Direction::LeftToRight
        );
        assert_eq!(reading_direction(&[]), Direction::RightToLeft);
    }

    /// Both directions of every distinct pair, and no self-pairs.
    ///
    /// A star is directional: `i -> j` composes `i -> hub` with `hub -> j`,
    /// and `j -> i` uses the other two legs, so they are different
    /// measurements. The count is also what the header prints as the price of
    /// all-pairs, so it has to be the count of what the table actually holds.
    #[test]
    fn the_star_measures_both_directions_of_every_pair_and_no_self_pair() {
        assert_eq!(
            ordered_pairs(3),
            vec![(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)]
        );
        assert_eq!(ordered_pairs(3).len(), 3 * (3 - 1), "n(n-1), as printed");
        assert_eq!(ordered_pairs(1), vec![], "one spoke has nothing to pair");
        assert_eq!(ordered_pairs(0), vec![]);
    }

    fn run_between(a: (u32, u32), b: (u32, u32)) -> Run {
        Run::paired(
            taiyaku_core::PageRange::new(a.0, a.1),
            taiyaku_core::PageRange::new(b.0, b.1),
        )
    }

    /// The four ways a direct solve and a composed one can differ, all four in
    /// one comparison so that none of them can borrow another's count.
    ///
    /// Pages 0 and 1 pair the same way in both. Page 2 the direct solve pairs
    /// and composition loses. Page 3 composition pairs and the direct solve
    /// misses. Page 4 both pair, to different counterparts. So: four pages
    /// paired directly, one loss, one inverse, one disagreement — four
    /// different numbers, which is what stops any of them being read off
    /// another.
    #[test]
    fn a_divergence_counts_loss_inverse_and_disagreement_apart() {
        let direct = vec![
            run_between((0, 2), (0, 2)),
            Run::gap(taiyaku_core::PageRange::new(3, 3)),
            run_between((4, 4), (4, 4)),
        ];
        let composed = vec![
            run_between((0, 1), (0, 1)),
            Run::gap(taiyaku_core::PageRange::new(2, 2)),
            run_between((3, 4), (9, 10)),
        ];

        let found = Divergence::between(&direct, &composed, 5);

        assert_eq!(
            found,
            Divergence {
                direct_paired: 4,
                loss: 1,
                inverse: 1,
                disagree: 1,
            }
        );

        // Totals across the star are the four sums, each on its own.
        assert_eq!(
            found.plus(Divergence {
                direct_paired: 10,
                loss: 2,
                inverse: 3,
                disagree: 4,
            }),
            Divergence {
                direct_paired: 14,
                loss: 3,
                inverse: 4,
                disagree: 5,
            }
        );
        assert_eq!(found.plus(Divergence::default()), found);
    }

    /// The verdict column is where composition and `trust`'s worse-leg rule
    /// are put side by side, and the disagreement is the measurement the
    /// command exists to take — so agreement prints one word and disagreement
    /// prints both, in that order.
    #[test]
    fn the_verdict_column_names_both_rules_only_when_they_disagree() {
        let report = |composed, worse_leg| PairReport {
            pages: 200,
            divergence: Divergence::default(),
            composed,
            worse_leg,
        };

        let agreed = report(Verdict::Sound, Verdict::Sound);
        assert!(!agreed.verdict_split());
        assert_eq!(agreed.verdict_column(), stats::verdict_word(Verdict::Sound));

        let split = report(Verdict::Sound, Verdict::Fragmented);
        assert!(split.verdict_split());
        assert_eq!(
            split.verdict_column(),
            format!(
                "{} != {}",
                stats::verdict_word(Verdict::Sound),
                stats::verdict_word(Verdict::Fragmented)
            ),
            "composed first, worse leg second"
        );
    }

    /// A composed route is judged by its weaker leg and a stored one answers
    /// for itself, and asking the wrong one of the two is silent in both
    /// directions: `quality_of` on a composed pair finds no edge and says the
    /// document does not say, about a pairing the document says plenty about.
    ///
    /// `en-fan` and `en-mono` are the two spokes of this fixture's star, so
    /// nothing joins them directly and the difference between the two rules is
    /// the difference between an answer and no answer at all.
    #[test]
    fn a_composed_route_is_judged_by_its_weaker_leg_and_a_stored_one_by_itself() {
        let doc = three_editions_one_hand_corrected();

        let composed = route_verdict(&doc, &Via::Hub("raw".into()), "en-fan", "en-mono");
        assert_eq!(
            composed,
            doc.trust("en-fan", "en-mono"),
            "a composed pair is only as good as its worse leg"
        );
        assert!(
            composed.is_some(),
            "the fixture must actually compose, or nothing above can fail"
        );
        assert_eq!(
            doc.quality_of("en-fan", "en-mono"),
            None,
            "and asking for a stored edge between two spokes finds none, \
             which is what makes the arm above load-bearing"
        );

        let stored = route_verdict(&doc, &Via::Direct, "raw", "en-fan");
        assert_eq!(
            stored,
            doc.quality_of("raw", "en-fan").map(|q| q.verdict),
            "a stored edge answers for itself"
        );
        assert!(stored.is_some(), "and this fixture stores one");
        assert_eq!(
            route_verdict(&doc, &Via::Inverted, "en-fan", "raw"),
            doc.quality_of("en-fan", "raw").map(|q| q.verdict),
            "so does the same edge read backwards"
        );
    }

    /// How the route was reached, which is the difference between a number a
    /// solver checked and one nothing ever did.
    #[test]
    fn the_route_note_says_only_what_a_direct_edge_would_not() {
        assert_eq!(route_note(&Via::Direct, "raw", "fan"), "");
        assert_eq!(route_note(&Via::Identity, "raw", "fan"), "");
        assert_eq!(
            route_note(&Via::Inverted, "raw", "fan"),
            ", from the stored fan -> raw edge",
            "inverted names the edge as it is stored, not as it was asked for"
        );
        assert_eq!(
            route_note(&Via::Hub("raw".into()), "fan", "colour"),
            ", composed through raw"
        );
    }

    /// Both ends of a slot-indexed pairing move when it is put back onto
    /// pages, and each moves by a different amount.
    ///
    /// A slots: page 0, then page 1 as two halves, then page 2 — so slot 3 is
    /// page 2. B slots: page 0 as two halves, then pages 1 and 2 — so slot 3
    /// is page 2 there too, but slot 1 is page 0 rather than page 1. Mapping
    /// only one end, or neither, still produces a score for every page and
    /// puts it against the wrong picture.
    #[test]
    fn slot_scores_are_put_back_onto_the_pages_at_both_ends() {
        let a_slots = vec![
            Slot {
                page: 0,
                half: None,
            },
            Slot {
                page: 1,
                half: Some(Half::Right),
            },
            Slot {
                page: 1,
                half: Some(Half::Left),
            },
            Slot {
                page: 2,
                half: None,
            },
        ];
        let b_slots = vec![
            Slot {
                page: 0,
                half: Some(Half::Right),
            },
            Slot {
                page: 0,
                half: Some(Half::Left),
            },
            Slot {
                page: 1,
                half: None,
            },
            Slot {
                page: 2,
                half: None,
            },
        ];
        let score = |page, counterpart| taiyaku_core::PageConfidence {
            page,
            counterpart,
            similarity: 0.9,
            margin: 0.3,
        };

        let on = on_pages(
            vec![score(3, Some(1)), score(0, None), score(9, Some(0))],
            &a_slots,
            &b_slots,
        );

        assert_eq!(
            on.iter()
                .map(|s| (s.page, s.counterpart))
                .collect::<Vec<_>>(),
            vec![(2, Some(0)), (0, None)],
            "slot 3 is page 2 in A and slot 1 is page 0 in B; slot 9 is neither"
        );
        assert_eq!(on[0].margin, 0.3, "the score itself is carried through");
    }

    /// The count printed as "to review", which is a count of pairings rather
    /// than of pages: an unpaired page had no second-best to be close to.
    #[test]
    fn only_a_pairing_with_a_narrow_margin_is_ambiguous() {
        let score = |counterpart, margin| taiyaku_core::PageConfidence {
            page: 0,
            counterpart,
            similarity: 0.9,
            margin,
        };

        // Two below the threshold rather than one: with a single narrow
        // pairing, "below 0.05" and "exactly 0.05" both count one thing, and
        // the test cannot tell a threshold from an equality.
        assert_eq!(
            ambiguous_pairings(&[
                score(Some(1), 0.01),
                score(Some(2), 0.02),
                score(Some(3), 0.9),
                score(None, 0.0),
                score(Some(4), AMBIGUOUS_MARGIN),
            ]),
            2,
            "two narrow pairings; a wide one, a gap and one exactly at the \
             threshold are not ambiguous"
        );
        assert_eq!(ambiguous_pairings(&[]), 0);
    }

    /// The counts under the alignment table. They partition the slots, so a
    /// fixture holding all three kinds is the only one that can tell them
    /// apart — count each kind as "everything that is not this kind" and the
    /// three numbers still sum to the right total.
    #[test]
    fn the_pairing_tally_splits_the_slots_three_ways() {
        let kinds = [
            Pairing::Whole,
            Pairing::Half,
            Pairing::Whole,
            Pairing::Gap,
            Pairing::Whole,
        ];

        assert_eq!(tally(&kinds), (3, 1, 1));
        assert_eq!(
            tally(&kinds).0 + tally(&kinds).1 + tally(&kinds).2,
            kinds.len(),
            "every slot is counted exactly once"
        );
        assert_eq!(tally(&[]), (0, 0, 0));
    }

    /// A joined spread becomes two slots in reading order, and a page that is
    /// not one keeps its own hash. The flags say which happened, because the
    /// slot table is built from them.
    #[test]
    fn a_joined_spread_becomes_two_slot_hashes_in_reading_order() {
        let hash = |n: u64| taiyaku_core::PageHash([n, 0, 0, 0]);
        // Two portraits around one page twice as wide, which is what makes the
        // middle one read as a spread against the edition it sits in.
        let described = vec![
            cache::CachedPage {
                width: 100,
                height: 150,
                hash: hash(1),
                halves: None,
            },
            cache::CachedPage {
                width: 200,
                height: 150,
                hash: hash(2),
                halves: Some((hash(20), hash(21))),
            },
            cache::CachedPage {
                width: 100,
                height: 150,
                hash: hash(3),
                halves: None,
            },
        ];

        let (hashes, wide) = slot_hashes(&described, Direction::RightToLeft);
        assert_eq!(
            hashes,
            vec![hash(1), hash(21), hash(20), hash(3)],
            "manga reads the right half first"
        );
        assert_eq!(wide, vec![false, true, false]);

        let (hashes, _) = slot_hashes(&described, Direction::LeftToRight);
        assert_eq!(
            hashes,
            vec![hash(1), hash(20), hash(21), hash(3)],
            "and a western comic reads the left half first"
        );

        // A page wide enough to read as a spread but with no halves cached —
        // an entry written before halves were stored — keeps its whole hash
        // rather than contributing nothing.
        let mut without = described.clone();
        without[1].halves = None;
        let (hashes, wide) = slot_hashes(&without, Direction::RightToLeft);
        assert_eq!(hashes, vec![hash(1), hash(2), hash(3)]);
        assert_eq!(wide, vec![false, false, false]);
    }

    /// Spreads counted from the slot table, where each one appears twice.
    #[test]
    fn a_spread_is_two_slots_and_is_counted_once() {
        let whole = |page| Slot { page, half: None };
        let half = |page, side| Slot {
            page,
            half: Some(side),
        };

        assert_eq!(
            spreads_in(&[
                whole(0),
                half(1, Half::Right),
                half(1, Half::Left),
                whole(2),
                half(3, Half::Right),
                half(3, Half::Left),
            ]),
            2
        );
        assert_eq!(spreads_in(&[whole(0), whole(1)]), 0);
        assert_eq!(spreads_in(&[]), 0);
    }

    /// A pairing involves a spread if *either* side is a half. Which side is
    /// halved depends on which way round the pair was given — raws split
    /// spreads and English releases join them, and the colour editions join
    /// them too — so a rule naming one side would count the same pairing
    /// differently in each direction.
    #[test]
    fn a_pairing_involves_a_spread_whichever_side_is_the_half() {
        let whole = Slot {
            page: 4,
            half: None,
        };
        let half = Slot {
            page: 7,
            half: Some(Half::Left),
        };

        assert_eq!(pairing_of(whole, Some(whole)), Pairing::Whole);
        assert_eq!(pairing_of(half, Some(whole)), Pairing::Half);
        assert_eq!(pairing_of(whole, Some(half)), Pairing::Half);
        assert_eq!(pairing_of(half, Some(half)), Pairing::Half);
        assert_eq!(pairing_of(whole, None), Pairing::Gap);
        assert_eq!(pairing_of(half, None), Pairing::Gap);
    }

    /// `--write` merges, so it reads first — and a read that fails for any
    /// reason other than the file not being there has to stop the write. It
    /// used to be a bare `fs::write`, which silently discarded a document that
    /// may hold four other editions, and treating every read error as "nothing
    /// to merge" puts that back.
    #[test]
    fn a_sidecar_that_cannot_be_read_stops_the_write_rather_than_being_absent() {
        let root = std::env::temp_dir().join("taiyaku-test-existing-sidecar");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        assert!(
            existing_sidecar(&root.join("nothing-here.json").to_string_lossy())
                .expect("an absent file is a new document")
                .is_none()
        );

        // A directory is readable metadata and unreadable content: the error
        // is not `NotFound`, so it must be reported rather than overwritten.
        let error = existing_sidecar(&root.to_string_lossy())
            .expect_err("a path that is not a file is not an absent file");

        let _ = std::fs::remove_dir_all(&root);
        assert!(
            error.contains(&root.to_string_lossy().to_string()),
            "the refusal must name the path: {error}"
        );
    }

    /// How many members came from cache, which is the difference between a
    /// cold run and a warm one and is reported to say which just happened.
    ///
    /// Counted rather than inferred: a count that never rises reads as an
    /// honest cold run every time, and the only thing wrong is that the
    /// three-orders-of-magnitude saving the cache exists for is invisible.
    #[test]
    fn a_warm_edition_reports_every_member_as_a_cache_hit() {
        let root = std::env::temp_dir().join("taiyaku-test-cached-edition");
        let _ = std::fs::remove_dir_all(&root);
        // The two members hold different pages. Identical ones would share a
        // digest, and the second would be answered from the entry the first
        // just wrote — one hit on a cold run, which is correct behaviour and
        // not what this is measuring.
        let members: Vec<PathBuf> = ["c1", "c2"]
            .iter()
            .enumerate()
            .map(|(chapter, name)| {
                let dir = root.join(name);
                std::fs::create_dir_all(&dir).unwrap();
                for page in 0..2u32 {
                    let chapter = u32::try_from(chapter)
                        .expect("chapter is 0 or 1, from enumerate() over the fixed 2-element array [\"c1\", \"c2\"] above");
                    let seed = page * 53 + chapter * 11;
                    let image = image::GrayImage::from_fn(24, 32, |x, y| {
                        image::Luma([((x * 7 + y * 31 + seed) % 200) as u8])
                    });
                    image.save(dir.join(format!("{page:03}.png"))).unwrap();
                }
                dir
            })
            .collect();
        let cache_root = root.join("cache");

        let cold = cached_edition(&cache_root, &members, Direction::RightToLeft);
        let warm = cached_edition(&cache_root, &members, Direction::RightToLeft);

        let _ = std::fs::remove_dir_all(&root);
        let (cold_hashes, _, cold_hits) = cold.expect("decodes");
        let (warm_hashes, _, warm_hits) = warm.expect("reads the cache");
        assert_eq!(cold_hits, 0, "nothing is cached on the first run");
        assert_eq!(warm_hits, members.len(), "both members on the second");
        assert_eq!(cold_hashes.len(), 4, "two members of two pages each");
        assert_eq!(cold_hashes, warm_hashes);
    }

    /// The summary under a report: a line per run, then the figures.
    ///
    /// Both are returned rather than printed because they go to different
    /// streams and a survey script reads one without the other — and because
    /// neither the paired total nor the runs-per-hundred rate is checkable at
    /// all while they only ever reach a terminal.
    #[test]
    fn a_report_summary_counts_paired_pages_and_the_run_rate() {
        let runs = vec![
            run_between((0, 9), (0, 9)),
            Run::gap(taiyaku_core::PageRange::new(10, 14)),
            run_between((15, 19), (10, 14)),
        ];

        let (table, figures) = report_summary(&runs, 20);

        assert_eq!(table.lines().count(), 3, "one line per run, gaps included");
        assert!(
            table.contains("offset +0") && table.contains("offset -5"),
            "each paired run reports the offset it sits at: {table}"
        );
        assert!(table.contains("gap"), "and a gap says so: {table}");
        assert!(
            figures.starts_with("3 runs | 15/20 pages paired (75.0%) | 15.0 runs per 100 pages"),
            "three runs over twenty pages, fifteen of them paired: {figures}"
        );

        // A hundred pages makes the rate and the run count different numbers,
        // so neither can stand in for the other.
        let (_, figures) = report_summary(&runs, 100);
        assert!(
            figures.contains("3.0 runs per 100 pages"),
            "the rate is per hundred pages, not per alignment: {figures}"
        );

        let (table, figures) = report_summary(&[], 0);
        assert_eq!(table, "", "no runs, no table");
        assert!(
            figures.starts_with("0 runs | 0/0 pages paired (0.0%)"),
            "an empty alignment divides by the floor rather than by zero: {figures}"
        );
    }

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
        .map(ToString::to_string)
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
            .filter_map(Result::ok)
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
        .map(ToString::to_string)
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
        .map(ToString::to_string)
        .collect();

        let err = align(&args).unwrap_err();

        assert_eq!(
            err,
            "edition key `en/fan` is not URL-safe (`/` at position 2); try `en-fan`"
        );
    }

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
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
                let byte = u8::try_from(v % 256)
                    .expect("v % 256 is in 0..256 by construction, which is exactly u8's range");
                image::Luma([byte])
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
            "serve has no flag `--bogus`; its flags are --addr, --cache, --log-requests, \
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
