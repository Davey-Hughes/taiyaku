use super::*;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
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
fn cbz_with_page(dir: &std::path::Path, name: &str, page: &str) -> PathBuf {
    use std::io::Write;
    let path = dir.join(name);
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
    let scratch = taiyaku_read::test_support::TempDir::new("regions-given");
    let a = cbz_with_page(&scratch, "given-a.cbz", "a1.png");
    let b = cbz_with_page(&scratch, "given-b.cbz", "b1.png");

    let digest_a = taiyaku_read::member_digest(&a).expect("digest a");
    let digest_b = taiyaku_read::member_digest(&b).expect("digest b");

    let sidecar_path = scratch.join("regions-given.json");
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
