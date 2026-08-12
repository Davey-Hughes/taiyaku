//! Scoring against the labelled corpus — a standing check, not an experiment.
//!
//! **Asserts only that it measured something.** Never a recall figure, never a
//! spurious rate: those move when the detector changes, and a measurement that
//! fails when the thing it measures improves is a check pretending to be a
//! measurement. `region_experiment.rs`'s header carries the full argument.
//!
//! **The deliberate asymmetry.** A *missing* truth document skips — the
//! ordinary state before any page has been labelled, which is where this
//! check starts life, since no page has been drawn yet — but a labelled page
//! that fails to score *fails* the test. The first is expected; the second
//! means somebody's labelling work is silently not being measured, which is
//! the exact failure this whole sub-project exists to catch. `score_corpus_page`
//! never panics for that reason: every way it can fail to reach a page comes
//! back `None` rather than aborting the loop, so a run that fails still names
//! every page it failed on rather than just the first.
//!
//! **Only the labelled pages are decoded.** `fingerprint::decode_all` (and, at
//! the single-page grain this module needs, `fingerprint::decode_luma`)
//! decodes whatever it is handed, and a handful of pages per edition are
//! labelled against tens or hundreds available — `score_corpus_page` resolves
//! and decodes exactly one page at a time, by index, so nothing outside the
//! dev set is ever paid for. This is `score::command`'s own rule ("Only the
//! labelled pages are decoded"), restated at a coarser grain: `command` reads
//! one sidecar-identified edition and filters its pages before one
//! `decode_all` call; this check reads several editions across the fixed dev
//! set, so filtering happens per truth-document page instead.
//!
//! **Reuses `score::command`'s shape, not its edition lookup.** `command`
//! identifies an edition by matching member digests against a sidecar, because
//! its caller hands it archives without saying which edition they are.
//! `sample::DEV_EDITIONS` already names the edition key directly, so this
//! module skips that step and reads `truth.editions[edition]` straight —
//! still the same sidecar → edition → filtered decode → score walk, just
//! entered one step later because the edition is already known.
//!
//! **The page sequence comes from the sidecar, not from a glob of the
//! directory.** This used to reach pages through `corpus::members` and
//! `taiyaku_read::read_edition`, which list what happens to be on disk and
//! sort it naturally — and `read_edition`'s own doc says "a caller holding a
//! sidecar must not [sort]". This caller *does* hold one: it tests
//! `sidecar.exists()` before anything else. `docs/sidecar.md` rule 1 is
//! normative — the edition's own `members` array *is* the page order — so
//! `edition_pages` walks that array, exactly as `score::command` does. The two
//! orders coincide on today's corpus, which is precisely why the old form
//! could not be seen to be wrong: drop an extra archive into an edition
//! directory, or replace one, and it scored labels against different pages and
//! printed a plausible report.
//!
//! **And it refuses on `is_stale_for`.** This is the one place in the branch
//! where that guard exists and was not used. Labels drawn against an archive
//! that has since been replaced put a box on the wrong art on the *right*
//! page — `TruthEdition::is_stale_for`'s own doc calls that the sharpest form
//! of the problem — and a standing check that cannot see it is a check that
//! certifies the mistake.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli score_corpus -- --nocapture
//! ```

use std::path::{Path, PathBuf};

use image::GrayImage;

use crate::sample::{DEV_EDITIONS, HELD_OUT_EDITIONS};
use crate::score::{COVERAGE, Report, score_page};

/// One edition's pages, in the sidecar's own member order, or an error saying
/// why the labels drawn on them cannot be trusted against what is on disk.
///
/// Three separate refusals, each of which the globbing form it replaced was
/// blind to: a sidecar that does not name this edition, a member that will not
/// read, and — the one this branch's own guard exists for — a member list
/// whose bytes are not the bytes the labels were drawn against.
///
/// Member paths are relative to the sidecar, per `docs/sidecar.md`, so they
/// are joined to its directory rather than to the edition directory: a member
/// may legitimately point into an unrelated tree, and its name need not sort
/// into reading order at all.
fn edition_pages(
    sidecar: &Path,
    edition: &str,
    labelled: &taiyaku_core::TruthEdition,
) -> Result<Vec<taiyaku_read::Page>, String> {
    let text =
        std::fs::read_to_string(sidecar).map_err(|e| format!("{}: {e}", sidecar.display()))?;
    let doc = taiyaku_core::Sidecar::from_json(&text)?;
    let Some(target) = doc.editions.get(edition) else {
        return Err(format!(
            "{} names no edition `{edition}`",
            sidecar.display()
        ));
    };
    let beside = sidecar.parent().unwrap_or_else(|| Path::new("."));
    let members: Vec<PathBuf> = target
        .members
        .iter()
        .map(|member| beside.join(&member.path))
        .collect();

    // Hashed off disk rather than read out of the sidecar. The sidecar's own
    // recorded digests would agree with the truth document even after the file
    // they name had been swapped underneath both, which is the case that puts
    // a box on the wrong art.
    let mut digests = Vec::new();
    for member in &members {
        digests.push(taiyaku_read::member_digest(member)?);
    }
    if labelled.is_stale_for(&digests) {
        return Err(format!(
            "the labels were drawn against different files from the {} member(s) `{edition}` \
             names in {}; re-label or restore the archives",
            members.len(),
            sidecar.display()
        ));
    }

    taiyaku_read::read_members_in_order(&members)
}

/// Decode one already-resolved page, or `None` naming why not.
///
/// A page index past the end and bytes that will not decode both come back
/// `None` rather than propagating an error or panicking. The caller treats a
/// `None` for a page somebody labelled as a page that must have been scored
/// and was not, which is what turns a resolution failure into the module
/// header's asymmetry: labels silently not being measured is the failure this
/// whole check exists to catch, so it must fail the test rather than merely
/// print a line and move on.
fn decode_labelled_page(
    pages: &[taiyaku_read::Page],
    work: &str,
    edition: &str,
    page: u32,
) -> Option<GrayImage> {
    let Some(found) = pages.get(page as usize) else {
        eprintln!(
            "{work}/{edition} page {page}: past the end ({} pages read)",
            pages.len()
        );
        return None;
    };
    match crate::fingerprint::decode_luma(&found.bytes) {
        Ok(image) => Some(image),
        Err(e) => {
            eprintln!("{work}/{edition} page {page} ({}): {e}", found.name);
            None
        }
    }
}

/// Score `detect_page` against every page labelled in the dev set.
///
/// See the module header for what is asserted (that it measured something)
/// and what deliberately is not (any recall or spurious figure), and for the
/// asymmetry between a missing truth document and a labelled page that fails
/// to resolve.
#[test]
fn scoring_the_dev_set_produces_a_report_over_every_labelled_page() {
    let Some(root) = crate::corpus::corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };

    let mut report = Report::default();
    let mut contributed: Vec<&str> = Vec::new();
    let mut labelled_pages = 0usize;
    let mut scored_pages = 0usize;

    for &(work, edition) in DEV_EDITIONS {
        let sidecar = root.join(work).join("taiyaku.json");
        if !sidecar.exists() {
            eprintln!("{work}: no sidecar; not yet labelled");
            continue;
        }
        let truth_path = taiyaku_core::TruthDoc::path_beside(&sidecar);
        let Ok(text) = std::fs::read_to_string(&truth_path) else {
            eprintln!("{work}/{edition}: no truth document; not yet labelled");
            continue;
        };
        let doc = taiyaku_core::TruthDoc::from_json(&text).expect("truth document parses");
        let Some(labelled) = doc.editions.get(edition) else {
            eprintln!("{work}/{edition}: truth document carries no labels for this edition");
            continue;
        };
        if labelled.pages.is_empty() {
            eprintln!("{work}/{edition}: truth document has no labelled pages yet");
            continue;
        }

        // Counted as a contributor here, before the edition is resolved, and
        // not after the loop below. Somebody has labelled this edition, so the
        // run is no longer the "nothing drawn yet" case that skips — and if
        // this were pushed only on the way out, an edition that failed to
        // resolve at all would `continue` past it and leave `contributed`
        // empty, which turns the assertions below into a clean skip. A green
        // run that measured nothing is the one outcome this check must not
        // have.
        contributed.push(work);
        labelled_pages += labelled.pages.len();

        let pages = match edition_pages(&sidecar, edition, labelled) {
            Ok(pages) => pages,
            Err(e) => {
                eprintln!("{work}/{edition}: {e}");
                continue;
            }
        };

        for (&page, drawn) in &labelled.pages {
            let Some(image) = decode_labelled_page(&pages, work, edition, page) else {
                eprintln!(
                    "{work}/{edition} page {page}: labelled but could not be resolved to an \
                     image — see the reason above"
                );
                continue;
            };
            let found = crate::detect::detect_page(&image);
            report.add_page(drawn, &score_page(drawn, &found, COVERAGE));
            scored_pages += 1;
        }
    }

    // Skipping cleanly before any labelling exists, rather than failing: this
    // check ships ahead of the labelling and the drawing happens afterwards.
    if contributed.is_empty() {
        eprintln!("no truth documents found; skipping until the dev set is labelled");
        return;
    }

    print!("{}", report.render());

    assert_eq!(
        scored_pages,
        labelled_pages,
        "every page somebody labelled must be scored; {} were not — see the reasons printed \
         above",
        labelled_pages - scored_pages
    );
    assert!(
        report.spurious_per_page().is_finite(),
        "every figure is finite"
    );
    assert!(report.pages > 0, "the report covers at least one page");
}

/// Every `sample.rs` entry resolves to a real directory under `TAIYAKU_CORPUS`.
///
/// Written after the plan that produced `sample.rs` listed three
/// `HELD_OUT_EDITIONS` paths that did not exist on disk —
/// `bocchi/raw-v01-src-a`/`-b`/`-c`, when the real directories are
/// `raw-v01-src-hamaji`, `raw-v01-src-krc` and `raw-v01-src-mangazip` — and
/// nothing would have caught that until someone sat down to label them.
/// Checks all fifteen entries in one run rather than stopping at the first
/// wrong one, because a second wrong path in the same run is a second thing
/// somebody has to go fix, and finding it on a later run costs another whole
/// label-then-discover cycle.
#[test]
fn every_sample_edition_resolves_to_a_directory_on_disk() {
    let Some(root) = crate::corpus::corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };

    let missing: Vec<String> = DEV_EDITIONS
        .iter()
        .chain(HELD_OUT_EDITIONS)
        .filter_map(|&(work, edition)| {
            let dir = root.join(work).join(edition);
            (!dir.is_dir()).then(|| format!("{work}/{edition} ({})", dir.display()))
        })
        .collect();

    eprintln!(
        "checked {} sample.rs entries under {}",
        DEV_EDITIONS.len() + HELD_OUT_EDITIONS.len(),
        root.display()
    );
    assert!(
        missing.is_empty(),
        "these sample.rs entries do not resolve to a directory: {missing:?}"
    );
}

// ---------------------------------------------------------------------------
// `edition_pages`, against fixtures rather than the corpus.
//
// These need no `TAIYAKU_CORPUS`, deliberately. The two checks above skip
// entirely without it — and nothing has been labelled on the corpus yet, so
// even with it the whole first check skips today. A guard that only runs once
// somebody has spent hours labelling is a guard that is discovered to be wrong
// at the worst possible moment.

/// A one-page cbz whose page is a real, decodable image.
///
/// Restated rather than shared with `score.rs`'s and `regions.rs`'s identical
/// fixtures: all three are three lines of `zip` around a 4x4 image, and a
/// shared one would have to live in a module compiled for every build just to
/// be reachable from three `#[cfg(test)]` ones.
fn cbz_with_page(path: &Path, page: &str) {
    use std::io::Write;
    let image = image::GrayImage::from_pixel(4, 4, image::Luma([255]));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("a 4x4 gray image always encodes");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).expect("create cbz"));
    zip.start_file(page, zip::write::SimpleFileOptions::default())
        .expect("start entry");
    zip.write_all(&bytes).expect("write page bytes");
    zip.finish().expect("finish cbz");
}

/// A work directory holding an edition of two archives, `b` then `a` in the
/// sidecar — the reverse of what a glob of the directory sorts to — plus a
/// third archive on disk that the sidecar does not name.
///
/// Returns the sidecar path and the two digests, in the sidecar's own order.
fn fixture(name: &str) -> (PathBuf, [String; 2]) {
    let root = std::env::temp_dir().join(format!("taiyaku-test-score-corpus-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("e");
    std::fs::create_dir_all(&dir).expect("create an edition directory");
    cbz_with_page(&dir.join("a.cbz"), "a1.png");
    cbz_with_page(&dir.join("b.cbz"), "b1.png");
    // Never named by the sidecar. A glob of `e/` picks it up; the sidecar's
    // own member array does not, and page order and page count must both be
    // the sidecar's.
    cbz_with_page(&dir.join("zz.cbz"), "zz1.png");

    let digest_a = taiyaku_read::member_digest(&dir.join("a.cbz")).expect("digest a");
    let digest_b = taiyaku_read::member_digest(&dir.join("b.cbz")).expect("digest b");
    assert_ne!(digest_a, digest_b, "or no order can be told from another");

    let sidecar = root.join("taiyaku.json");
    std::fs::write(
        &sidecar,
        format!(
            r#"{{
  "taiyaku": 1,
  "work": {{ "title": "t", "reading": "rtl" }},
  "editions": {{
    "e": {{ "label": "e", "members": [
      {{ "path": "e/b.cbz", "sha256": "{digest_b}", "pages": 1 }},
      {{ "path": "e/a.cbz", "sha256": "{digest_a}", "pages": 1 }}
    ] }}
  }},
  "hub": "e",
  "alignments": []
}}"#
        ),
    )
    .expect("write sidecar");
    (sidecar, [digest_b, digest_a])
}

/// The labelled edition, with `members` as the browser writes them.
fn labelled_against(digests: &[&str]) -> taiyaku_core::TruthEdition {
    let members: Vec<String> = digests
        .iter()
        .map(|sha256| format!(r#"{{ "sha256": "{sha256}" }}"#))
        .collect();
    let json = format!(
        r#"{{ "taiyaku_truth": 1, "editions": {{ "e": {{
           "members": [{}], "pages": {{ "0": [] }} }} }} }}"#,
        members.join(", ")
    );
    taiyaku_core::TruthDoc::from_json(&json)
        .expect("truth document parses")
        .editions
        .remove("e")
        .expect("the fixture names edition e")
}

/// The page sequence is the sidecar's member array, not a sort of what is in
/// the directory.
///
/// `read_edition`'s own doc says "a caller holding a sidecar must not [sort]",
/// and this caller holds one. Two things bite here at once, and both were
/// silently wrong: a glob returns `a, b, zz` — sorted, and three members
/// where the sidecar names two — while the sidecar says `b, a`. On today's
/// corpus the two orders happen to coincide, which is exactly why nothing
/// would have caught this until an archive was added or replaced.
#[test]
fn the_page_order_is_the_sidecars_and_an_unnamed_archive_is_not_read() {
    let (sidecar, [digest_b, digest_a]) = fixture("order");
    let labelled = labelled_against(&[&digest_b, &digest_a]);

    let pages = edition_pages(&sidecar, "e", &labelled).expect("the fixture resolves");
    let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();
    let _ = std::fs::remove_dir_all(sidecar.parent().expect("the sidecar has a parent"));

    assert_eq!(
        names,
        ["b1.png", "a1.png"],
        "the sidecar's own member order, and only the members it names"
    );
}

/// And the guard this branch built is finally used here: labels drawn against
/// bytes that are no longer on disk are refused rather than scored.
///
/// The sharpest form of the staleness problem — a box on the wrong art on the
/// *right* page (`TruthEdition::is_stale_for`). This check never called it,
/// so a replaced archive produced a plausible report and nothing in it could
/// have said so.
#[test]
fn labels_drawn_against_a_replaced_archive_are_refused_rather_than_scored() {
    let (sidecar, [digest_b, _]) = fixture("stale");
    let elsewhere = "0000000000000000000000000000000000000000000000000000000000000000";
    let labelled = labelled_against(&[&digest_b, elsewhere]);

    let err = edition_pages(&sidecar, "e", &labelled).expect_err("the second member was replaced");
    let _ = std::fs::remove_dir_all(sidecar.parent().expect("the sidecar has a parent"));

    assert!(err.contains("drawn against different files"), "{err}");
    assert!(err.contains('e'), "and names the edition: {err}");
}
