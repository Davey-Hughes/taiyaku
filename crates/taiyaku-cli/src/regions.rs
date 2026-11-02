//! `taiyaku regions`: propose the text blocks of one edition.
//!
//! The same contract `credits` has, and for the same reason: this is the output
//! of a heuristic, so it proposes and prints, and it writes only under
//! `--write`. It identifies the edition by content — hashing the members it was
//! handed and matching them against the document — rather than being told,
//! which is the rule `verify` established.
//!
//! **It pairs nothing.** Which block corresponds to which block on another
//! edition is derived at read time from the alignment; see the L2 design, §5.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use taiyaku_core::{Axis, Region, RegionDoc, RegionEdition, RegionMember, Sidecar};

use crate::detect::detect_page;

const USAGE: &str = "\
taiyaku regions <member>... --sidecar FILE [--write]

Propose the text blocks of one edition, page by page. Prints what it found;
writes nothing without --write.

There is no --cache: the fingerprint cache holds hashes and dimensions, not
pixels, and the regions document is what makes a second run cheap.";

/// The median block area as a fraction of the page, and the counts around it.
///
/// The median rather than the mean, because the disqualifier the L2 design
/// states is a median and a single page-sized block would drag a mean past it
/// while every other block was fine.
fn median_area(pages: &BTreeMap<u32, Vec<Region>>) -> f64 {
    let mut areas: Vec<f64> = pages
        .values()
        .flatten()
        .map(|region| region.w * region.h)
        .collect();
    if areas.is_empty() {
        return 0.0;
    }
    areas.sort_by(|a, b| a.partial_cmp(b).expect("areas are finite"));
    areas[areas.len() / 2]
}

/// One fixed-field line for a survey to collect.
///
/// **`blocks_per_page` is a mean, not a median.** On this detector's output the
/// two differ sharply and the mean is the *weaker* claim: most pages return
/// zero blocks, so the median is 0 on nearly every edition. Reporting the mean
/// against the bar of 2 is therefore conservative — the median would fail
/// harder. The design names the mean; see its §6.
fn survey_line(edition: &str, pages: &BTreeMap<u32, Vec<Region>>) -> String {
    let blocks: usize = pages.values().map(Vec::len).sum();
    let vertical = pages
        .values()
        .flatten()
        .filter(|r| r.axis == Axis::Vertical)
        .count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "blocks and pages.len() count text-block regions and pages for one edition \
                  this tool proposes regions for; real editions run to at most a few hundred \
                  pages and a similar order of blocks, both far below f32's 2^24 exact-integer \
                  bound, so the conversion loses no precision — a property of the \
                  material, not enforced by the type (both are usize with no cap). The \
                  lossless-From route is closed too: both are usize, and std has no \
                  From<usize> for f32 — usize's width is platform-dependent, unlike \
                  u32/u16/u8, so no such impl exists."
    )]
    let per_page = if pages.is_empty() {
        0.0
    } else {
        blocks as f32 / pages.len() as f32
    };
    format!(
        "regions: edition={edition} pages={} blocks={blocks} v={vertical} h={} \
         median_area={:.4} blocks_per_page={per_page:.2}",
        pages.len(),
        blocks - vertical,
        median_area(pages),
    )
}

/// The per-page table a person judges the proposal by.
fn table(pages: &BTreeMap<u32, Vec<Region>>) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("page  blocks  v     h     median area\n");
    for (page, regions) in pages {
        let vertical = regions.iter().filter(|r| r.axis == Axis::Vertical).count();
        let mut one = BTreeMap::new();
        one.insert(*page, regions.clone());
        let _ = writeln!(
            out,
            "{page:>4}  {:>6}  {vertical:<4}  {:<4}  {:.4}",
            regions.len(),
            regions.len() - vertical,
            median_area(&one),
        );
    }
    out
}

/// Read the document beside the sidecar, or start an empty one.
///
/// A document that will not parse is an error rather than an empty start: this
/// command rewrites one edition's entry and leaves the others alone, so
/// silently starting fresh would delete every other edition's regions.
fn load_beside(path: &Path) -> Result<RegionDoc, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => RegionDoc::from_json(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RegionDoc::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// A page index, as `found`'s `BTreeMap<u32, Vec<Region>>` keys it.
///
/// # Panics
///
/// Panics if `i` exceeds `u32::MAX`. It cannot here: `i` comes from
/// `.enumerate()` over `detected`, one entry per page of the edition being
/// processed; real editions run to at most a few hundred pages, nowhere
/// near `u32::MAX`.
fn page_index(i: usize) -> u32 {
    u32::try_from(i).expect("page index exceeds u32::MAX")
}

pub fn command(args: &[String]) -> Result<(), String> {
    // No `--cache`, deliberately: a flag that cannot take effect is refused
    // rather than accepted (O19, O20, O22), the fingerprint cache holds hashes
    // and dimensions rather than pixels, and the regions document is already
    // the durable artefact a second cache would duplicate.
    const KNOWN: &[&str] = &["--sidecar", "--write"];
    crate::reject_unknown_flags("regions", args, KNOWN)?;

    // Every valued flag resolves before a member is opened, so a typo does not
    // cost the seconds a decode pass takes.
    let sidecar = crate::option_value(args, "--sidecar", "a path to a taiyaku.json")?;
    let write = args.iter().any(|a| a == "--write");

    if write && sidecar.is_none() {
        return Err(
            "--write needs --sidecar: the regions document is named after the sidecar, \
             and there is nothing to name it after"
                .into(),
        );
    }

    let members: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| Some(a.as_str()) != sidecar)
        .map(PathBuf::from)
        .collect();
    if members.is_empty() {
        return Err(USAGE.into());
    }

    // Refused rather than ignored, the rule `align` applies to every flag that
    // cannot take effect. Regions are recorded against an edition, an edition
    // is identified by content against a document, and without the document
    // there is nothing to identify against.
    let Some(sidecar) = sidecar else {
        return Err(
            "regions needs --sidecar: blocks are recorded against an edition, and the \
             sidecar is what says which edition these members are"
                .into(),
        );
    };

    // Read and parse before a single page is decoded. A mistyped path is the
    // same class of mistake as a wrong flag and belongs on the same side of the
    // cost line.
    let text = std::fs::read_to_string(sidecar).map_err(|e| format!("{sidecar}: {e}"))?;
    let doc = Sidecar::from_json(&text)?;

    // Content, not order: `edition_of_digests` compares the sorted sha256 set,
    // so the order these happened to be typed in does not matter for
    // identification. Order matters afterwards, which is the next step.
    let mut digests = Vec::new();
    for member in &members {
        digests.push(taiyaku_read::member_digest(member)?);
    }
    let edition = crate::edition_of_digests(&doc, sidecar, &digests)?;

    // `docs/sidecar.md` rule 1 is normative: the matched edition's own
    // `members` array *is* the page order, and it must not be re-sorted —
    // paths are relative to the sidecar and may point into unrelated trees,
    // so their names need not sort into reading order at all. This is the
    // idiom `credits`'s `classify` already follows: identify the edition by
    // content first, then walk the document's own member order to place the
    // argv paths and their digests into it, rather than flattening a natural
    // sort of the argv — which is right only for a shell glob
    // (`taiyaku_read::read_edition`'s job) and wrong for a sidecar's array.
    let target = doc
        .editions
        .get(&edition)
        .expect("edition_of_digests matched an edition of this document");
    let by_digest: BTreeMap<&str, &PathBuf> = digests
        .iter()
        .zip(&members)
        .map(|(sha256, path)| (sha256.as_str(), path))
        .collect();
    let mut ordered_members: Vec<PathBuf> = Vec::new();
    let mut ordered_digests: Vec<String> = Vec::new();
    for member in &target.members {
        let path = *by_digest
            .get(member.sha256.as_str())
            .expect("edition_of_digests matched this exact set of digests");
        ordered_members.push(path.clone());
        ordered_digests.push(member.sha256.clone());
    }

    let pages = taiyaku_read::read_members_in_order(&ordered_members)?;
    let detected = crate::fingerprint::decode_all(&pages, |_, luma| detect_page(luma))?;

    let found: BTreeMap<u32, Vec<Region>> = detected
        .into_iter()
        .enumerate()
        .map(|(index, regions)| (page_index(index), regions))
        .collect();

    eprint!("{}", table(&found));

    if write {
        let path = RegionDoc::path_beside(Path::new(sidecar));
        let mut document = load_beside(&path)?;
        // Wholesale for this edition, and untouched for every other: the field
        // holds no hand entries yet, so a re-run after a constant changes must
        // leave nothing stale behind — and must not take the neighbours with it.
        document.editions.insert(
            edition.clone(),
            RegionEdition {
                members: ordered_digests
                    .iter()
                    .map(|sha256| RegionMember {
                        sha256: sha256.clone(),
                    })
                    .collect(),
                pages: found.clone(),
            },
        );
        taiyaku_read::atomic::write(&path, document.to_json().as_bytes(), "json")?;
        let blocks: usize = found.values().map(Vec::len).sum();
        eprintln!(
            "wrote {blocks} block(s) for `{edition}` to {}",
            path.display()
        );
    }

    // Printed only once whatever `--write` promised has happened, so a failed
    // run never leaves a well-formed `regions:` line on stdout for a survey to
    // collect. The same rule `credits` states at its own final line.
    println!("{}", survey_line(&edition, &found));

    Ok(())
}

#[cfg(test)]
mod tests;
