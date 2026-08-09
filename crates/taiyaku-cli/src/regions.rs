//! `taiyaku regions`: propose the text blocks of one edition.
//!
//! The same contract `credits` has, and for the same reason: this is the output
//! of a heuristic, so it proposes and prints, and it writes only under
//! `--write`. It identifies the edition by content — hashing the members it was
//! handed and matching them against the document — rather than being told,
//! which is the rule `verify` established after taking the first alignment of a
//! five-edition document produced a confident report about a pair nobody asked
//! about.
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
fn median_area(pages: &BTreeMap<u32, Vec<Region>>) -> f32 {
    let mut areas: Vec<f32> = pages
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
/// **`blocks_per_page` is a mean, where the L2 design's disqualifier says
/// "median regions per page".** The two are named here rather than quietly
/// reconciled, because on this detector's output they differ sharply and the
/// mean is the *weaker* claim: most pages return zero blocks, so the median is
/// 0 on nearly every edition while the mean reaches 0.84. Reporting the mean
/// and still failing the bar of 2 on all nine editions is therefore
/// conservative — the median would fail harder. The design is corrected to name
/// the mean; see its §6.
fn survey_line(edition: &str, pages: &BTreeMap<u32, Vec<Region>>) -> String {
    let blocks: usize = pages.values().map(Vec::len).sum();
    let vertical = pages
        .values()
        .flatten()
        .filter(|r| r.axis == Axis::Vertical)
        .count();
    let per_page = match pages.is_empty() {
        true => 0.0,
        false => blocks as f32 / pages.len() as f32,
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
        .map(|(index, regions)| (index as u32, regions))
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
        taiyaku_serve::atomic::write(&path, document.to_json().as_bytes(), "json")?;
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
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn write_without_a_sidecar_is_refused() {
        let err = command(&args(&["a.cbz", "--write"])).expect_err("refuses");
        assert!(err.contains("--sidecar"), "names what is missing: {err}");
    }

    /// O21: a misspelling used to be accepted in silence by every command, and
    /// the refusal has to name the flags that do exist rather than guess.
    #[test]
    fn an_unknown_flag_is_refused_and_the_real_ones_are_named() {
        let err = command(&args(&["a.cbz", "--sidecar", "t.json", "--writ"])).expect_err("refuses");
        assert!(err.contains("--writ"), "names the typo: {err}");
        assert!(err.contains("--write"), "names the real flag: {err}");
    }

    /// The deviation this plan states, pinned rather than left to the prose:
    /// `--cache` cannot take effect here, so it is refused rather than
    /// accepted and ignored — the rule O19, O20 and O22 each closed a defect
    /// to establish.
    #[test]
    fn cache_is_refused_rather_than_accepted_and_ignored() {
        let err =
            command(&args(&["a.cbz", "--sidecar", "t.json", "--cache", "x"])).expect_err("refuses");
        assert!(err.contains("--cache"), "names it: {err}");
    }

    /// O15: a flag whose value is another flag used to be taken as absent.
    #[test]
    fn a_flag_with_no_value_is_refused() {
        let err = command(&args(&["a.cbz", "--sidecar", "--write"])).expect_err("refuses");
        assert!(err.contains("--sidecar"), "{err}");
    }

    #[test]
    fn a_run_with_no_member_prints_usage() {
        let err = command(&args(&["--sidecar", "t.json"])).expect_err("refuses");
        assert!(err.contains("taiyaku regions"), "{err}");
    }

    /// Every field a survey collects, in a fixed order, so a table in notes.md
    /// can be rebuilt rather than retyped. The discipline `--stats`,
    /// `duplicates` and `credits` all follow.
    #[test]
    fn the_survey_line_has_every_field_in_a_fixed_order() {
        let mut pages = BTreeMap::new();
        pages.insert(
            0,
            vec![
                Region {
                    x: 0.1,
                    y: 0.1,
                    w: 0.1,
                    h: 0.2,
                    axis: Axis::Vertical,
                },
                Region {
                    x: 0.5,
                    y: 0.1,
                    w: 0.1,
                    h: 0.2,
                    axis: Axis::Vertical,
                },
            ],
        );
        pages.insert(
            1,
            vec![Region {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.1,
                axis: Axis::Horizontal,
            }],
        );
        let line = survey_line("raw-mono", &pages);
        assert_eq!(
            line,
            "regions: edition=raw-mono pages=2 blocks=3 v=2 h=1 \
             median_area=0.0200 blocks_per_page=1.50"
        );
    }

    /// A survey line for an edition where nothing was found must still be a
    /// well-formed line. An absent row reads as an edition that was not
    /// surveyed, which is the opposite of what it means.
    #[test]
    fn an_edition_with_no_blocks_still_produces_a_line() {
        let mut pages = BTreeMap::new();
        pages.insert(0, Vec::new());
        let line = survey_line("raw-mono", &pages);
        assert!(
            line.starts_with("regions: edition=raw-mono pages=1 blocks=0"),
            "{line}"
        );
        assert!(line.contains("median_area=0.0000"), "{line}");
    }

    /// Write a one-page cbz whose page is a real, decodable image.
    ///
    /// Unlike every other test in this file, `--write` runs the full decode
    /// path — `taiyaku_read::read_edition` then `fingerprint::decode_all` —
    /// so the entry has to be bytes an image decoder accepts, not a stand-in.
    fn cbz_with_page(name: &str, page: &str) -> PathBuf {
        use std::io::Write;
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        let image = image::GrayImage::from_pixel(4, 4, image::Luma([255]));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("a 4x4 gray image always encodes");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("create cbz"));
        zip.start_file(page, zip::write::SimpleFileOptions::default())
            .expect("start entry");
        zip.write_all(&bytes).expect("write page bytes");
        zip.finish().expect("finish cbz");
        path
    }

    /// `docs/sidecar.md` rule 1 is normative: a sidecar's `members` array *is*
    /// the page order, and a reader must not re-sort it — paths are relative
    /// to the sidecar and may point into unrelated trees, so their names need
    /// not sort into reading order at all. `regions` used to derive page order
    /// from natural order of the argv members instead (the idiom `align`'s
    /// own `split_editions` uses for a shell glob), which agrees with the
    /// sidecar only by coincidence.
    ///
    /// The sidecar's own `members` array is set here to `b, a` — the reverse
    /// of natural order — while the command line types `a, b`, natural order
    /// itself. That combination is what a test typing only the reverse order
    /// on the command line (the old fixture's shape) cannot distinguish: a
    /// version that sorts the typed members naturally and a version that uses
    /// the typed order verbatim would *both* produce `a, b` here, and only
    /// reading the sidecar's own order produces the expected `b, a`.
    #[test]
    fn write_records_members_in_the_sidecars_own_order_not_natural_order() {
        let a = cbz_with_page("taiyaku-test-regions-given-a.cbz", "a1.png");
        let b = cbz_with_page("taiyaku-test-regions-given-b.cbz", "b1.png");

        let digest_a = taiyaku_read::member_digest(&a).expect("digest a");
        let digest_b = taiyaku_read::member_digest(&b).expect("digest b");

        let sidecar_path = std::env::temp_dir().join("taiyaku-test-regions-given.json");
        std::fs::write(
            &sidecar_path,
            format!(
                r#"{{
  "taiyaku": 1,
  "work": {{ "title": "t", "reading": "rtl" }},
  "editions": {{
    "raw-mono": {{
      "label": "raw-mono",
      "members": [
        {{ "path": "b", "sha256": "{digest_b}", "pages": 1 }},
        {{ "path": "a", "sha256": "{digest_a}", "pages": 1 }}
      ]
    }}
  }},
  "hub": "raw-mono",
  "alignments": []
}}"#
            ),
        )
        .expect("write sidecar");

        // Typed a, b — natural order — on purpose, so this differs from the
        // sidecar's own b, a.
        let result = command(&args(&[
            a.to_str().expect("a path is utf8"),
            b.to_str().expect("b path is utf8"),
            "--sidecar",
            sidecar_path.to_str().expect("sidecar path is utf8"),
            "--write",
        ]));

        let regions_path = RegionDoc::path_beside(&sidecar_path);
        let written = std::fs::read_to_string(&regions_path);

        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
        let _ = std::fs::remove_file(&sidecar_path);
        let _ = std::fs::remove_file(&regions_path);

        result.expect("command succeeds");
        let doc = RegionDoc::from_json(&written.expect("regions document was written"))
            .expect("regions document parses");
        let got: Vec<&str> = doc.editions["raw-mono"]
            .members
            .iter()
            .map(|m| m.sha256.as_str())
            .collect();

        assert_eq!(
            got,
            [digest_b.as_str(), digest_a.as_str()],
            "the sidecar's own member order is b, a; typed order was a, b (already \
             natural order too), so members must be written b, a — matching neither \
             the typed order nor a natural sort of it"
        );
    }
}
