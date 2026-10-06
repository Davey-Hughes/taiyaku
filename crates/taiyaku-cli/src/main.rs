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
mod library;
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
mod user;
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
use std::sync::Arc;

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
    // Checked before dispatch, and for every command rather than one at a
    // time, so `taiyaku serve --help` answers the same way `taiyaku --help`
    // does: the usage this binary already prints for no arguments at all.
    // A second, command-scoped usage line would drift from this one the way
    // `score.rs`'s own `USAGE` almost did (its doc comment states the two
    // must not disagree); one function printed from two call sites cannot.
    if args.iter().any(|a| a == "--help") {
        print_usage();
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("align") => align(&args[1..]),
        Some("verify") => verify(&args[1..]),
        Some("topology") => topology(&args[1..]),
        Some("lookup") => lookup(&args[1..]),
        Some("serve") => serve(&args[1..]),
        Some("duplicates") => duplicates::command(&args[1..]),
        Some("inventory") => inventory::command(&args[1..]),
        Some("credits") => credits::command(&args[1..]),
        Some("library") => library::command(&args[1..]),
        Some("regions") => regions::command(&args[1..]),
        Some("score") => score::command(&args[1..]),
        Some(other) => Err(format!("unknown command `{other}`")),
        None => {
            print_usage();
            Ok(())
        }
    }
}

/// The whole-binary usage: every command's shape, one line each, printed for
/// no arguments and for `--help` anywhere on the line.
fn print_usage() {
    println!("taiyaku {}", env!("CARGO_PKG_VERSION"));
    println!("usage: taiyaku align  <a-member>... -- <b-member>...");
    println!("       taiyaku verify <a-member>... -- <b-member>... [--out FILE] [--sidecar FILE]");
    println!("       taiyaku topology <hub>... -- <spoke>... -- <spoke>...");
    println!("       taiyaku lookup <taiyaku.json> <from> <to> <page>...");
    println!(
        "       taiyaku serve  <taiyaku.json>... [--addr HOST:PORT] [--db PATH] \
         [--stump-url URL] [--stump-key KEY] [--log-requests FILE] [--refresh-stump] \
         [--cache DIR] [--trusted-proxy CIDR]... [--secure-cookies] \
         [--insecure-cookies] [--allow-web-setup] [--web DIR]"
    );
    println!("       taiyaku library import <taiyaku.json> [--db PATH]");
    println!(
        "       taiyaku library user promote <name> [--role admin|user] [--db PATH]  \
         (the first account; prompts for a password)"
    );
    println!(
        "       taiyaku library user add <name> | user password <name> | user list  [--db PATH]"
    );
    println!(
        "       taiyaku library key create <user> --name DEVICE | key list <user> | \
         key revoke <user> <id>  [--db PATH]"
    );
    println!(
        "       taiyaku library backup [--out DIR] [--keep N] [--db PATH]  \
         (--out writes where the server will not list it)"
    );
    println!(
        "       taiyaku library export <work>|--all [--dir DIR|--in-place] [--user NAME] [--db PATH]"
    );
    println!("       taiyaku library libraries [--db PATH]");
    println!(
        "       taiyaku library scan  [--add PATH] [--library NAME | --root PATH] [--no-derive] \
         [--db PATH]  (--add files a folder under --library, creating it; alone, as a \
         library of its own)"
    );
    println!(
        "       taiyaku library user grant-library|revoke-library <user> <library> [--db PATH]"
    );
    println!("       taiyaku library rules [--export | --import FILE] [--db PATH]");
    println!(
        "       taiyaku library proposals [--min-score N] [--accept ID] [--dismiss ID] \
         [--unaccept ID] [--reopen ID] [--db PATH]"
    );
    println!("       taiyaku duplicates <copy>... -- <copy>... [--against <member>...] [--ltr]");
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

/// Refuse a bare word given to a command that has nothing to do with one.
///
/// **A silently ignored argument is the worst of the three answers.**
/// `taiyaku library scan /srv/manga` reads as "scan this path"; it used to
/// scan every root the library already knows, print a report about all of
/// them, and say nothing at all about the path — so a person adding a new
/// share saw a plausible-looking scan of somebody else's directories and no
/// hint that theirs had not been touched. `--add` and `--root` are how a path
/// reaches this command, and naming them is what makes the refusal useful
/// rather than merely correct.
///
/// `with_values` names the flags that take a value, so the value is not
/// mistaken for a stray word — [`option_value`] reads `--db PATH` as two
/// arguments, not one.
///
/// # Errors
///
/// Returns an error naming the first bare word and `instead`, which should
/// say what the caller probably meant to type.
pub(crate) fn reject_positional_arguments(
    command: &str,
    args: &[String],
    with_values: &[&str],
    instead: &str,
) -> Result<(), String> {
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg.starts_with("--") {
            if with_values.contains(&arg.as_str()) {
                rest.next();
            }
            continue;
        }
        return Err(format!(
            "{command} takes no arguments, and `{arg}` was given as one — {instead}"
        ));
    }
    Ok(())
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
/// Through `taiyaku_read::atomic::write`, never `fs::write`. `fs::write` opens
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
    taiyaku_read::atomic::write(Path::new(out), sidecar.to_json().as_bytes(), "json")
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
            "--allow-web-setup",
            "--cache",
            "--db",
            "--insecure-cookies",
            "--log-requests",
            "--refresh-stump",
            "--secure-cookies",
            "--stump-key",
            "--stump-url",
            "--trusted-proxy",
            "--web",
        ],
    )?;
    let paths: Vec<PathBuf> = args
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    // **A sidecar named after a flag is a mistake, not an empty invocation.**
    // `paths` is `take_while(|a| !a.starts_with("--"))`, so
    // `serve --db lib.db work/taiyaku.json` yields NO paths — and once an
    // empty list meant "serve the whole library", that command line silently
    // served everything instead of the one work it named. Before spec C1 §5.3
    // it failed with "needs at least one taiyaku.json". Found by the
    // whole-branch review.
    // The rule is deliberately simple rather than clever: a sidecar is a
    // `.json` file by definition, so an argument ending in `.json` when no
    // path was read is a sidecar that landed in the wrong place. Trying to
    // tell a flag's *value* from a stray positional would need a table of
    // which flags take one, and would be wrong the next time a flag is added.
    if paths.is_empty()
        && let Some(stray) = args.iter().find(|a| a.ends_with(".json"))
    {
        return Err(format!(
            "`{stray}` looks like a sidecar but follows a flag, so it was not read as one. \
             Put paths before the flags: `taiyaku serve {stray} --db PATH`, or give no paths \
             at all to serve the whole library"
        ));
    }

    // **No paths is now an invocation rather than a mistake** — spec C1 §5.3.
    // The scanner produces works with no `sidecar_path` at all, so there is
    // nothing to put on a command line; `serve --db` is how a browse UI's
    // `/read/{work}` link reaches one. `serve_works` answers for both cases,
    // including the refusal when the library turns out to hold nothing.
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
    let auth = serve_auth(args, std::env::var("TAIYAKU_TRUSTED_PROXY").ok().as_deref())?;
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
    // Checked before the library is opened, so a mistyped `--web` is refused
    // for what is actually wrong.
    let web = web_root(option_value(args, "--web", "a directory")?)?;

    // `taiyaku serve <sidecar>...` keeps working, and now means it: every
    // path it is handed is imported into the real, on-disk library — through
    // `library::import_canonical`, the same function `taiyaku library
    // import` calls, so a work this command imports and one a person imports
    // by hand agree on what row it is (see `library.rs`'s own doc comment on
    // the placeholder-path hazard this closes). This is the migration path
    // for an existing install, and it is what keeps `serve/src/corpus.rs`'s
    // three corpus-gated tests alive — they call `taiyaku_serve::load_works`
    // directly rather than through this function, so they are untouched by
    // this change.
    let (db_library, works) = serve_works(&paths, option_value(args, "--db", "a path")?)?;
    for work in works.snapshot().iter() {
        let sidecar = work.sidecar();
        eprintln!(
            "{} | {} editions | {}",
            work.id,
            sidecar.editions.len(),
            sidecar.work.title
        );
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("{e}"))?;
    let outcome = runtime.block_on(taiyaku_serve::serve(
        Arc::new(db_library),
        works,
        taiyaku_serve::ServeOptions {
            addr: addr.to_string(),
            stump,
            log_requests: log,
            refresh_stump,
            cache,
            auth,
            web,
        },
    ));
    // **Not left to `Drop`**, which waits for every blocking task to finish. A
    // scan started from the web UI is a detached `spawn_blocking`, and dropping
    // the runtime under one holds the process until the scan ends.
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN);
    outcome
}

/// How long a stopping server waits for blocking tasks before it exits anyway.
/// Added to `taiyaku_serve::SHUTDOWN_GRACE`, it must stay inside the container
/// runtime's stop timeout.
const RUNTIME_SHUTDOWN: std::time::Duration = std::time::Duration::from_secs(2);

/// `--trusted-proxy`, `--insecure-cookies` and `--allow-web-setup`, read into
/// what `serve` needs (spec A2 §5 and §6).
///
/// **Configuring any trusted proxy implies `Secure` on the session cookie.** A
/// proxy in front means TLS terminated there, and a cookie without `Secure`
/// travels in clear over any non-TLS hop behind it. `--insecure-cookies` is the
/// deliberate opt-out for a plain-HTTP deployment on a private network — a
/// separate flag rather than a silent default, because the failure it prevents
/// is invisible: the cookie simply works, and is simply readable.
///
/// `from_environment` is `TAIYAKU_TRUSTED_PROXY`, read by the caller so that
/// this function's tests never depend on the environment they run in.
///
/// # Errors
///
/// Returns an error if a `--trusted-proxy` value is not an address or a CIDR
/// range, naming which one, or if `TAIYAKU_TRUSTED_PROXY` holds one that is
/// not, naming the variable.
fn serve_auth(
    args: &[String],
    from_environment: Option<&str>,
) -> Result<taiyaku_serve::session::AuthConfig, String> {
    let mut trusted_proxies = Vec::new();
    // Repeatable, so it is read by scanning rather than through
    // `option_value` — one household can sit behind two proxies, and a flag
    // that silently kept only the last would be the kind of quiet wrong answer
    // this project's flag handling exists to refuse.
    for (index, arg) in args.iter().enumerate() {
        if arg != "--trusted-proxy" {
            continue;
        }
        let value = args
            .get(index + 1)
            .filter(|next| !next.starts_with("--"))
            .ok_or("--trusted-proxy needs a CIDR range or an address")?;
        trusted_proxies.push(
            value
                .parse()
                .map_err(|why| format!("--trusted-proxy {why}"))?,
        );
    }

    // **The environment is the container's way in, and a flag still wins.** An
    // image's command is fixed, so a deployment names its proxy in
    // `TAIYAKU_TRUSTED_PROXY`; any `--trusted-proxy` replaces that list rather
    // than adding to it, the same precedence `--stump-url` holds over
    // `TAIYAKU_STUMP_URL`.
    if trusted_proxies.is_empty()
        && let Some(listed) = from_environment
    {
        for value in listed
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|value| !value.is_empty())
        {
            trusted_proxies.push(
                value
                    .parse()
                    .map_err(|why| format!("TAIYAKU_TRUSTED_PROXY {why}"))?,
            );
        }
    }

    // **`Secure` cannot be derived from `--trusted-proxy` alone**, which is what
    // shipped and what the whole-branch review caught on 2026-08-18. That flag
    // is about believing `X-Forwarded-For`, and the ordinary TLS deployment —
    // `caddy reverse_proxy 127.0.0.1:8484` in front of a loopback server —
    // needs no forwarded address at all. It would therefore have configured
    // neither flag and sent a thirty-day session token with no `Secure`, so one
    // plain-HTTP request to the same host puts it in cleartext, without anybody
    // passing the documented opt-out.
    //
    // So a trusted proxy still *implies* `Secure`, and `--secure-cookies` says
    // it outright for the deployment that has TLS in front and nothing to
    // forward. `--insecure-cookies` overrides both, and is still the only way
    // to end up without it deliberately.
    let insecure = flag_present(args, "--insecure-cookies");
    let secure = flag_present(args, "--secure-cookies");
    Ok(taiyaku_serve::session::AuthConfig {
        secure_cookies: (secure || !trusted_proxies.is_empty()) && !insecure,
        allow_web_setup: flag_present(args, "--allow-web-setup"),
        trusted_proxies,
    })
}

/// `--web DIR`: a built web app to serve beside the API.
///
/// **Refused at startup when `DIR` holds no `index.html`**, rather than served.
/// Otherwise every page of the app would answer 404 from a server that looks
/// healthy in every other way, and the mistake — naming `web/` rather than
/// `web/dist`, or a tree nobody built — is one a person makes once and should
/// hear about at once.
///
/// # Errors
///
/// When `DIR` has no `index.html` in it, naming the directory and the command
/// that builds one.
fn web_root(dir: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(dir) = dir else {
        return Ok(None);
    };
    let dir = PathBuf::from(dir);
    if !dir.join("index.html").is_file() {
        return Err(format!(
            "--web {}: there is no index.html in it. Build the app with \
             `pnpm -C web run build` and name `web/dist`",
            dir.display()
        ));
    }
    Ok(Some(dir))
}

/// Canonicalise every sidecar path, open the resolved library, import each
/// one, and build the `Work`s `serve` passes to `taiyaku_serve::serve`.
///
/// Split out of `serve` itself so this — the part that touches a real
/// filesystem and a real database — is directly testable without ever
/// reaching `tokio::runtime::Runtime::new()` and binding a port, the same
/// reason `align_flags` is split out of `align`.
///
/// Every path is validated first — canonicalised, which requires it to exist
/// — before the library is touched at all, so a mistyped path still fails
/// exactly where it always has (`serve_reaches_load_works_with_...`'s own
/// tests pin this) rather than as a side effect of provisioning a database
/// nobody asked for yet.
///
/// **One sidecar that cannot be imported is skipped, not fatal.** It used to
/// propagate with `?` from inside the loop, so a single hand-edited
/// `taiyaku.json` — a renamed edition the `hub` no longer names, a truncated
/// file — took the whole server down and with it every other work on the
/// command line, which is not a trade a reader would make (review 2, I3).
/// Each failure is named on stderr as it happens; the start only refuses when
/// *nothing* could be imported, since there is then no library to serve.
///
/// **An empty library, reached with no paths at all, is not refused.** It
/// used to be — but spec C1 §5.3's "no paths means the whole library" made
/// that refusal exactly the thing standing in front of the two first-run
/// states the browse UI's `EmptyState.tsx` exists to show: no roots
/// configured, and roots configured but nothing scanned yet. The server
/// exited before either screen could render, so `GET /api/library/status`,
/// added to tell those two states apart, could never be exercised in the
/// state it was built for. Found by trying to serve an empty database in a
/// browser. A start against an empty library now prints a note to stderr and
/// goes on to bind — the guidance that used to be the fatal error now lives
/// on the screen where a person can act on it.
///
/// # Errors
///
/// Returns an error if any path cannot be canonicalised, if `db_flag`
/// resolves to nowhere (see `library::resolve_db_path`), if the library
/// cannot be opened, or — when paths were named on the command line — if
/// **every** one of them failed to import (see `library::import_canonical`),
/// or, with no paths, if the library cannot be read at all (see
/// `Works::library`, which leaves out and names a work that will not project
/// rather than refusing). An empty library reached with no paths at all is not
/// an error; see above.
fn serve_works(
    paths: &[PathBuf],
    db_flag: Option<&str>,
) -> Result<(taiyaku_library::Library, taiyaku_serve::Works), String> {
    // **No paths means the whole library** — spec C1 §5.3. Taken before the
    // canonicalisation below, which has nothing to canonicalise.
    if paths.is_empty() {
        let db = library::resolve_db_path(db_flag)?;
        let db_library =
            taiyaku_library::Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
        // `Works::library` has already printed why it failed, through the
        // registry's own report, so the error returned here only says where
        // to look. Returning its text would print the same line twice, once
        // there and once from `main`.
        let works = taiyaku_serve::Works::library(&db_library)
            .map_err(|_| "the library could not be read, for the reason given above".to_string())?;
        if works.is_empty() {
            // Not fatal — see `serve_works`'s own doc comment. The browse
            // UI's `EmptyState.tsx` is what actually tells "no roots" and
            // "roots but unscanned" apart and says what to run for each; this
            // is only for the person watching the terminal it started from.
            eprintln!(
                "taiyaku: {} holds no works yet. The browse screen will say what to do next — \
                 `taiyaku library scan --add PATH` if nothing has been added, or `taiyaku \
                 library scan` if a root is already configured but has not been read",
                db.display()
            );
        }
        return Ok((db_library, works));
    }

    let canonical_paths: Vec<PathBuf> = paths
        .iter()
        .map(|p| library::canonical_sidecar_path(p))
        .collect::<Result<_, _>>()?;

    let db = library::resolve_db_path(db_flag)?;
    let db_library =
        taiyaku_library::Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let mut works = Vec::with_capacity(canonical_paths.len());
    let mut imported: Vec<(i64, PathBuf)> = Vec::new();
    let mut refused = 0usize;
    for path in &canonical_paths {
        match serve_work(&db_library, path, &mut imported) {
            Ok(Some(work)) => works.push(work),
            // The same work named twice on this command line — see
            // `serve_work`, which owns that rule and names both paths.
            Ok(None) => {}
            Err(e) => {
                refused += 1;
                eprintln!("taiyaku: {}: not served: {e}", path.display());
            }
        }
    }
    if works.is_empty() {
        return Err(format!(
            "no work could be served: all {refused} sidecar(s) failed to import, \
             each named above"
        ));
    }
    Ok((db_library, taiyaku_serve::Works::fixed(works)))
}

/// Import one canonical path and build the `Work` for it, or `None` when this
/// command line has already served the work that path imports to.
///
/// `imported` is the work ids built so far, each with the path that built it,
/// and is appended to here. The same row must not back two `Work`s: `Work` is
/// deliberately not `Clone` because two of them over one document could
/// correct it twice and see two different answers, so the second mention of
/// one work is skipped rather than served.
///
/// **The skip is announced, naming both paths.** Two different files can
/// import to one work — that is the whole point of matching on content, and
/// `taiyaku serve raw/taiyaku.json raw/../raw/taiyaku.json` is not the only
/// way to reach it: a copy of a library under a second path imports to the
/// same row by `content_key`. Dropping the second silently meant a person
/// passed two paths, got one work, and had nothing to tell them which of the
/// two the server was actually reading — the same failure `load_works`'
/// refusal already names both sides of. It is a notice rather than a refusal
/// because the outcome is correct: one row, one `Work`, everything the
/// command line asked for is being served.
///
/// Split out of `serve_works`' loop so that one fallible unit — import,
/// slug, projection — has one `Err` for the caller to skip past, rather than
/// three `?`s that each abort the whole start.
fn serve_work(
    db_library: &taiyaku_library::Library,
    path: &Path,
    imported: &mut Vec<(i64, PathBuf)>,
) -> Result<Option<taiyaku_serve::Work>, String> {
    let (work_id, _outcomes, migration) = library::import_canonical(db_library, path)?;
    for line in library::malformed_sibling_warnings(path, &migration) {
        eprintln!("{line}");
    }
    let summary = library::migration_summary(&migration);
    if !summary.is_empty() {
        eprintln!("taiyaku: {}: imported{summary}", path.display());
    }
    if let Some((_, first)) = imported.iter().find(|(id, _)| *id == work_id) {
        eprintln!("{}", already_served_notice(path, first));
        return Ok(None);
    }
    imported.push((work_id, path.to_path_buf()));
    let conn = db_library.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [work_id], |row| {
            row.get(0)
        })
        .map_err(|e| format!("reading the imported work's slug: {e}"))?;
    let sidecar = taiyaku_library::project::sidecar(&conn, work_id)?;
    drop(conn);
    Ok(Some(taiyaku_serve::Work::new(
        slug,
        path.to_path_buf(),
        sidecar,
    )))
}

/// What `serve_work` says on stderr when a path imports to a work already
/// being served from another path.
///
/// A pure function, and a separate one, for the reason `skipping_notice`
/// (`taiyaku-serve/src/lib.rs`) is: the property worth pinning is that the
/// sentence names **both** paths, and a message built inline inside a branch
/// that returns `Ok(None)` can only be tested by capturing stderr. Naming one
/// side would be almost as bad as naming neither — "this path was skipped"
/// leaves the reader with no way to know which file the server is actually
/// reading.
fn already_served_notice(path: &Path, first: &Path) -> String {
    format!(
        "taiyaku: {}: already served as {} — both import to the same work, so \
         it is served once, from the first",
        path.display(),
        first.display()
    )
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
                // `merge_spoke` itself never returns this — see its own doc
                // comment — so this call site can never actually reach the
                // arm. Only `taiyaku_library::import::reimport` constructs
                // it, catching a `merge_spoke` refusal rather than
                // propagating it, which is not what this command does: a
                // refusal here reaches the caller through the `?` two lines
                // above instead. Handled anyway because the match must be
                // exhaustive over every `Merged` this crate can see.
                Merged::DroppedHandRuns(n) => {
                    format!("replaced, dropping {n} stale hand run(s)")
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
mod tests;
