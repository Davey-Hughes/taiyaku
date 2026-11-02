use super::*;
use crate::credit_fixture::CreditFixture;

#[test]
fn write_without_a_sidecar_is_refused() {
    let args: Vec<String> = ["a.cbz", "--write"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let err = command(&args).expect_err("refuses");
    assert!(
        err.contains("--write") && err.contains("--sidecar"),
        "the refusal must name both flags: {err}"
    );
}

#[test]
fn a_valued_flag_with_no_value_is_refused_before_any_member_is_read() {
    // O15's rule. The member path does not exist, so a version that resolved
    // options late would fail on a missing file instead of ever refusing.
    let args: Vec<String> = ["no-such-file.cbz", "--sidecar"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let err = command(&args).expect_err("refuses");
    assert!(
        err.contains("--sidecar") && !err.contains("no-such-file"),
        "the flag must be refused before the member is opened: {err}"
    );
}

#[test]
fn a_misspelled_flag_is_refused() {
    // O21's rule.
    let args: Vec<String> = ["a.cbz", "--wrote"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let err = command(&args).expect_err("refuses");
    assert!(err.contains("--wrote"), "the refusal must name it: {err}");
}

#[test]
fn no_members_is_refused_with_the_usage() {
    let err = command(&[]).expect_err("refuses");
    assert!(
        err.contains("credits"),
        "the refusal must show the usage: {err}"
    );
}

#[test]
fn against_without_a_sidecar_is_refused() {
    let args: Vec<String> = ["a.cbz", "--against", "raw"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let err = command(&args).expect_err("refuses");
    assert!(
        err.contains("--against") && err.contains("--sidecar"),
        "the refusal must name both flags: {err}"
    );
}

#[test]
fn emit_fixture_without_a_sidecar_is_refused() {
    let args: Vec<String> = ["a.cbz", "--emit-fixture", "out.json"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let err = command(&args).expect_err("refuses");
    assert!(
        err.contains("--emit-fixture") && err.contains("--sidecar"),
        "the refusal must name both flags: {err}"
    );
}

#[test]
fn no_sidecar_at_all_is_refused_naming_the_flag() {
    let args: Vec<String> = ["a.cbz"].iter().map(ToString::to_string).collect();
    let err = command(&args).expect_err("refuses");
    assert!(err.contains("--sidecar"), "unhelpful: {err}");
}

#[test]
fn a_missing_sidecar_file_is_refused_before_any_member_is_fingerprinted() {
    // I2. `no-such-file.cbz` is never opened: if it were, the refusal
    // would come from `taiyaku_read::member_digest` instead and would
    // name the member rather than the sidecar path.
    let args: Vec<String> = [
        "no-such-file.cbz",
        "--sidecar",
        "/nonexistent-dir/taiyaku.json",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    let err = command(&args).expect_err("refuses");
    assert!(
        err.contains("nonexistent-dir") && !err.contains("no-such-file"),
        "the sidecar path must be refused before the member is opened: {err}"
    );
}

#[test]
fn the_table_widens_to_its_names_and_says_nothing_it_did_not_measure() {
    // No cell in this fixture holds a space, so splitting on whitespace
    // recovers the cells and states the columns without pinning the
    // padding, which is asserted separately below.
    fn cells(line: &str) -> Vec<&str> {
        line.split_whitespace().collect()
    }

    // The `-` in the recurs column is the case that matters: an edition of
    // one member has no recurrence to report, and a `0` there would read as
    // a perfect match.
    let rows = [
        Row {
            page: 95,
            member: "a-very-long-chapter-name.cbz".to_string(),
            local: 51,
            member_pages: 52,
            run: 1,
            at_end: true,
            recurs: Some(14),
            nearest: Some("c4.cbz#29".to_string()),
            credits: true,
        },
        Row {
            page: 1,
            member: "c0.cbz".to_string(),
            local: 1,
            member_pages: 44,
            run: 42,
            at_end: false,
            recurs: None,
            nearest: None,
            credits: false,
        },
    ];
    let out = table(&rows);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "a header and one row each: {out}");
    assert_eq!(cells(lines[0]), HEADERS);
    assert_eq!(
        cells(lines[1]),
        [
            "95",
            "a-very-long-chapter-name.cbz",
            "51/52",
            "1",
            "yes",
            "14",
            "c4.cbz#29",
            "credits"
        ]
    );
    assert_eq!(
        cells(lines[2]),
        ["1", "c0.cbz", "1/44", "42", "no", "-", "-", "-"],
        "an unmeasured recurrence prints as `-`, never as the 0 that would \
             read as a perfect match"
    );

    // The member column is as wide as the widest member, so every column
    // after it lines up rather than being pushed along by one long name.
    // `local` is right-aligned, so it is the right edges that agree.
    let ends_at =
        |line: &str, cell: &str| line.find(cell).expect("cell is in the line") + cell.len();
    assert_eq!(
        ends_at(lines[1], "51/52"),
        ends_at(lines[2], "1/44"),
        "the local column ends in the same place on both rows: {out}"
    );
}

/// An unpaired run of `pages` at `local` within a member, flagged or not.
///
/// The slot range is set equal to the page range: nothing this module does
/// reads it, and a fabricated spread would only invite a reader to trust a
/// number the fixture invented.
fn run(page: u32, local: u32, length: u32, at_member_end: bool) -> UnpairedRun {
    UnpairedRun {
        slots: page..page + length,
        pages: page..page + length,
        member: 0,
        local: local..local + length,
        at_member_end,
    }
}

#[test]
fn only_a_short_run_at_a_members_last_page_is_a_candidate() {
    // All four quadrants of the rule, so neither half can be dropped
    // without a failure. The bound is `MAX_CREDIT_RUN`, not a literal, so
    // this test must move with it.
    let over = MAX_CREDIT_RUN + 1;
    assert!(is_candidate(&run(10, 5, MAX_CREDIT_RUN, true)));
    assert!(
        !is_candidate(&run(10, 5, over, true)),
        "a run past the bound is too long to be a card, wherever it sits"
    );
    assert!(
        !is_candidate(&run(10, 0, MAX_CREDIT_RUN, false)),
        "a member's first page is front matter, and front matter is the work"
    );
    assert!(!is_candidate(&run(10, 0, over, false)));
}

#[test]
fn a_written_list_comes_out_ascending() {
    // Runs handed over out of order, which `unpaired_runs` does not do
    // today and which nothing in the format would catch if a later
    // producer did. The list goes through `to_json` and back rather than
    // being asserted in memory, because "written" is the claim.
    let runs = [
        run(200, 9, 1, true),
        run(7, 3, 1, true),
        run(100, 4, 3, false), // not a candidate: must not appear at all
        run(42, 1, 1, true),
    ];
    let pages = credit_pages(&runs);
    assert_eq!(pages, vec![7, 42, 200], "sorted, and candidates only");

    let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "fan": {
              "label": "fan",
              "members": [{ "path": "c1.cbz", "sha256": "c1", "pages": 300 }],
              "spreads": []
            }
          },
          "hub": "fan",
          "alignments": []
        }"#;
    let mut doc = Sidecar::from_json(text).expect("fixture parses");
    doc.editions
        .get_mut("fan")
        .expect("the fixture has it")
        .credits = pages;

    let written = Sidecar::from_json(&doc.to_json()).expect("round-trips");
    let credits = &written.editions["fan"].credits;
    assert_eq!(credits, &vec![7, 42, 200]);
    assert!(
        credits.windows(2).all(|pair| pair[0] < pair[1]),
        "the written list must ascend: {credits:?}"
    );
}

/// `--write` must not be able to leave a short sidecar behind.
///
/// The same property O2 established for progress and `atomic.rs` owns for
/// everything that writes beside a sidecar: the destination is *replaced* by
/// a rename, never opened for truncation. A crash cannot be staged in a test,
/// so this pins the mechanism that rules it out.
///
/// It is observed by **inode identity** rather than by taking write
/// permission off the existing file. `atomic.rs`'s own test records why
/// that approach was abandoned there: on a filesystem that ignores the
/// read-only bit, or under root, the permission-denied write it
/// distinguishes "replaced" from "truncated" by simply succeeds, and the
/// test prints a skip line libtest captures — a green that measured
/// nothing. A rename gives the path a different inode; an in-place write
/// keeps the same one, whoever is running and whatever the filesystem enforces.
///
/// The fixture carries a **second** edition and an alignment between the
/// two, because that is what is actually at stake: `credits --write`
/// rewrites the whole document to change one list on one edition, so the
/// half-written file this rules out would take the other edition's members
/// and the runs joining them with it.
#[test]
fn writing_a_credits_list_replaces_the_sidecar_rather_than_truncating_it() {
    let dir = taiyaku_read::test_support::TempDir::new("credits-write");
    let path = dir.join("taiyaku.json");
    let sidecar = path.to_str().expect("utf-8 temp path").to_string();

    let mut doc = fan_and_raw();
    std::fs::write(&path, doc.to_json()).expect("the fixture is written");

    #[cfg(unix)]
    let before = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(&path).expect("stat").ino()
    };

    doc.editions
        .get_mut("fan")
        .expect("the fixture has it")
        .credits = vec![3];
    let wrote = write_sidecar(&sidecar, &doc);

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

    // Whole, not half: the new list is there and everything the command
    // never touched came through with it.
    let written = Sidecar::from_json(&read_back).expect("the destination holds a whole document");
    assert_eq!(written.editions["fan"].credits, vec![3]);
    assert_eq!(
        written.editions["raw"].members.len(),
        1,
        "the untouched edition must survive the rewrite"
    );
    assert_eq!(
        written.alignments.len(),
        1,
        "the alignment joining the two must survive the rewrite"
    );
}

/// Distinct hashes at known Hamming distances, built by setting the low
/// `n` bits of an all-zero hash rather than by writing literals nobody can
/// check by eye. `PageHash` has neither `Default` nor a bit-flip method,
/// so this builds the raw `[u64; 4]` words directly instead of widening
/// the fingerprint type's public API for a test's convenience.
fn hash_with_bits_set(n: u32) -> PageHash {
    let mut words = [0u64; 4];
    for bit in 0..n {
        words[(bit / 64) as usize] |= 1 << (bit % 64);
    }
    PageHash(words)
}

#[test]
fn the_nearest_neighbour_is_never_in_the_same_member() {
    // Member 0 holds two pages one bit apart. Member 1 holds one page eight
    // bits away. The within-member pair is closer and must be ignored:
    // a chapter repeating its own page says nothing about recurrence.
    let members = vec![
        vec![hash_with_bits_set(0), hash_with_bits_set(1)],
        vec![hash_with_bits_set(8)],
    ];
    let found = recurrences(&members);
    let page0 = found.iter().find(|r| r.page == 0).expect("page 0 reported");
    assert_eq!(
        page0.distance, 8,
        "the 1-bit neighbour is in the same member"
    );
    assert_eq!(page0.nearest_member, 1);
    assert_eq!(page0.nearest_local, 0);
}

#[test]
fn pages_are_numbered_across_the_whole_edition_not_within_a_member() {
    let members = vec![
        vec![hash_with_bits_set(0), hash_with_bits_set(40)],
        vec![hash_with_bits_set(1)],
    ];
    let found = recurrences(&members);
    assert_eq!(
        found.iter().map(|r| r.page).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "member 1's only page is the edition's page 2"
    );
}

#[test]
fn a_single_member_edition_reports_nothing() {
    // Nothing to recur against. Reporting distance 0 here, or the nearest
    // page within the member, would invent a signal.
    let members = vec![vec![hash_with_bits_set(0), hash_with_bits_set(1)]];
    assert!(recurrences(&members).is_empty());
}

#[test]
fn nearest_member_and_local_are_actually_computed_across_three_members() {
    // Three members, so "the other member" is not a well-defined guess
    // and neither is `1 - mi`. Page 0 (member 0, all-zero hash) is
    // farthest from member 1's single page, farther still from member
    // 2's first page, and nearest of all to member 2's *second* page —
    // a nonzero local index in a member that isn't simply "the other
    // one". Distances 50, 60, and 5 are all distinct, so no tie-break
    // can paper over a wrong answer.
    let members = vec![
        vec![hash_with_bits_set(0)],
        vec![hash_with_bits_set(50)],
        vec![hash_with_bits_set(60), hash_with_bits_set(5)],
    ];
    let found = recurrences(&members);
    let page0 = found.iter().find(|r| r.page == 0).expect("page 0 reported");
    assert_eq!(
        page0.distance, 5,
        "member 2's second page is the true nearest neighbour"
    );
    assert_eq!(
        page0.nearest_member, 2,
        "nearest neighbour is in member 2, not member 1 or `1 - mi`"
    );
    assert_eq!(
        page0.nearest_local, 1,
        "nearest neighbour is member 2's second page, not its first"
    );
}

/// A minimal one-edition document, for the tests below that only care
/// about edition matching and the hub refusal, never about pages.
fn one_edition(name: &str, sha: &str) -> Sidecar {
    let text = format!(
        r#"{{
              "taiyaku": 1,
              "work": {{ "title": "W", "reading": "rtl" }},
              "editions": {{
                "{name}": {{
                  "label": "{name}",
                  "members": [{{ "path": "m.cbz", "sha256": "{sha}", "pages": 1 }}],
                  "spreads": []
                }}
              }},
              "hub": "{name}",
              "alignments": []
            }}"#
    );
    Sidecar::from_json(&text).expect("fixture parses")
}

#[test]
fn no_matching_edition_is_refused_by_name() {
    let doc = one_edition("fan", "c1");
    let err =
        crate::edition_of_digests(&doc, "taiyaku.json", &["zzz".to_string()]).expect_err("refuses");
    assert!(
        err.contains("taiyaku.json") && err.contains("zzz"),
        "the refusal must name the sidecar and the digest it could not place: {err}"
    );
}

/// A document with four editions: `hub`, one spoke joined to it directly, and a
/// second alignment — `spokeA` to `spokeB` — that touches neither.
fn hub_with_a_spoke_to_spoke_edge() -> Sidecar {
    let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "hub":    { "label": "hub",    "members": [{ "path": "h.cbz", "sha256": "h", "pages": 1 }], "spreads": [] },
            "spokeA": { "label": "spokeA", "members": [{ "path": "a.cbz", "sha256": "a", "pages": 1 }], "spreads": [] },
            "spokeB": { "label": "spokeB", "members": [{ "path": "b.cbz", "sha256": "b", "pages": 1 }], "spreads": [] },
            "spokeC": { "label": "spokeC", "members": [{ "path": "c.cbz", "sha256": "c", "pages": 1 }], "spreads": [] }
          },
          "hub": "hub",
          "alignments": [
            { "from": "spokeA", "to": "spokeB", "by": "solver", "runs": [] },
            { "from": "hub", "to": "spokeC", "by": "solver", "runs": [] }
          ]
        }"#;
    Sidecar::from_json(text).expect("fixture parses")
}

#[test]
fn the_hub_refusal_only_offers_editions_that_actually_touch_it() {
    let doc = hub_with_a_spoke_to_spoke_edge();

    let err = resolve_counterpart(&doc, "hub", None).expect_err("refuses");

    assert!(err.contains("hub"), "the refusal must name the hub: {err}");
    assert!(
        err.contains("spokeC"),
        "the edition actually joined to the hub must be offered: {err}"
    );
    assert!(
        !err.contains("spokeA") && !err.contains("spokeB"),
        "a spoke-to-spoke edge that never touches the hub must not be offered as an \
             answer, since it has no edge to the hub for `unpaired_runs` to find: {err}"
    );
}

#[test]
fn the_hub_refusal_ends_cleanly_with_no_alignments_at_all() {
    let doc = one_edition("hub", "h");

    let err = resolve_counterpart(&doc, "hub", None).expect_err("refuses");

    assert!(
        !err.trim_end().ends_with("--against:"),
        "a document with nothing to suggest must not trail off after a bare \
             `--against:`: {err}"
    );
}

#[test]
fn against_is_used_verbatim_even_for_the_hub() {
    // `--against` is a choice the caller made, not a default this
    // function has to justify — so it is returned unchecked, hub or not.
    let doc = one_edition("hub", "h");
    assert_eq!(
        resolve_counterpart(&doc, "hub", Some("elsewhere")).unwrap(),
        "elsewhere"
    );
}

/// A two-member `fan` edition aligned against a one-member `raw` hub,
/// with `c1`'s last page (page 3) unpaired — the same shape
/// `taiyaku-core::credits`'s `a_run_at_a_members_final_index_is_flagged_at_its_end`
/// fixture uses, reused here because `classify` needs a real alignment to
/// call `unpaired_runs` through.
fn fan_and_raw() -> Sidecar {
    let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Reversed Order Co.", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 8 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [
                { "path": "c1.cbz", "sha256": "c1", "pages": 4 },
                { "path": "c2.cbz", "sha256": "c2", "pages": 4 }
              ],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0-2","b":"0-2"},{"a":"3"},{"a":"4-7","b":"4-7"}] }
          ]
        }"#;
    Sidecar::from_json(text).expect("fixture parses")
}

#[test]
fn page_order_comes_from_the_editions_own_member_order_not_the_argument_order() {
    // fan page 3 — c1's last — is the one unpaired page. Handing
    // `classify` the two members' hashes in the opposite order from the
    // document's own `members` list (c2 before c1) must not move that
    // page onto c2, nor change either the row or the printed line.
    let doc = fan_and_raw();
    let c1_hashes: Vec<PageHash> = (0..4).map(hash_with_bits_set).collect();
    let c2_hashes: Vec<PageHash> = (20..24).map(hash_with_bits_set).collect();

    let forward = vec![
        ("c1".to_string(), c1_hashes.clone()),
        ("c2".to_string(), c2_hashes.clone()),
    ];
    let reversed = vec![("c2".to_string(), c2_hashes), ("c1".to_string(), c1_hashes)];

    let (rows_fwd, line_fwd, pages_fwd) =
        classify(&doc, "fan", "raw", &forward, None).expect("classifies");
    let (rows_rev, line_rev, pages_rev) =
        classify(&doc, "fan", "raw", &reversed, None).expect("classifies");

    assert_eq!(
        pages_fwd, pages_rev,
        "the page list must not depend on argument order"
    );
    assert_eq!(
        line_fwd, line_rev,
        "the printed line must not depend on argument order"
    );
    let simplify = |rows: &[Row]| -> Vec<(u32, String)> {
        rows.iter().map(|r| (r.page, r.member.clone())).collect()
    };
    assert_eq!(simplify(&rows_fwd), simplify(&rows_rev));

    let row = rows_fwd
        .iter()
        .find(|r| r.page == 3)
        .expect("page 3 is reported");
    assert_eq!(
        row.member, "c1.cbz",
        "page 3 is c1's own last page in the document's member order"
    );
}

#[test]
fn the_work_title_is_last_so_its_spaces_cannot_shift_another_field() {
    // `Reversed Order Co.` holds two spaces, which is the whole point: the
    // collectors in `scripts/` split on whitespace and index fields
    // positionally, so a title anywhere but the end silently moves every
    // field after it by a count nobody can predict from the format alone.
    let doc = fan_and_raw();
    let fingerprinted = vec![
        ("c1".to_string(), vec![hash_with_bits_set(0); 4]),
        ("c2".to_string(), vec![hash_with_bits_set(0); 4]),
    ];

    let (_, line, _) = classify(&doc, "fan", "raw", &fingerprinted, None).expect("classifies");

    assert!(
        line.starts_with("credits: edition=fan against=raw"),
        "the fixed fields must lead the line: {line}"
    );

    // Every field before `work=` sits at a known index, whatever the title
    // holds. Asserted as the whole prefix rather than as "work is last", so
    // a field inserted after `bound=` fails here rather than drifting.
    let fields: Vec<&str> = line.split_whitespace().collect();
    assert_eq!(
        &fields[..8],
        &[
            "credits:",
            "edition=fan",
            "against=raw",
            "pages=8",
            "slots=8",
            "runs=1",
            "candidates=1",
            "bound=1",
        ],
        "the fixed fields must be positionally stable: {line}"
    );
    assert_eq!(
        line.split_once(" work=").map(|(_, title)| title),
        Some("Reversed Order Co."),
        "the title must run unambiguously to end of line, spaces and all: {line}"
    );
}

#[test]
fn scan_information_is_read_from_comicinfo_and_absent_where_there_is_none() {
    let with = br#"<ComicInfo><Series>X</Series><ScanInformation>Cirno9baka</ScanInformation><PageCount>17</PageCount></ComicInfo>"#;
    assert_eq!(
        scan_information(&[("ComicInfo.xml".into(), with.to_vec())]),
        Some("Cirno9baka".into())
    );
    let without = br#"<ComicInfo><Series>X</Series></ComicInfo>"#;
    assert_eq!(
        scan_information(&[("ComicInfo.xml".into(), without.to_vec())]),
        None,
        "a ComicInfo with no ScanInformation is absent, not empty"
    );
    assert_eq!(
        scan_information(&[("001.jpg".into(), vec![])]),
        None,
        "an archive with no ComicInfo at all is absent"
    );
}

#[test]
fn the_scan_field_precedes_work_so_a_group_name_with_spaces_cannot_shift_it() {
    // `work=` holds spaces and runs to end of line, which is why it is last.
    // A group name holds spaces too - "Psylocke Scans" - so `scan=` cannot be
    // last as well. It is quoted, and it sits before `work=`.
    let line = credits_line(
        "en-fan",
        "raw-mono",
        226,
        231,
        9,
        1,
        Some("Psylocke Scans"),
        "Kaguya-sama v01",
    );
    let scan_at = line.find("scan=").expect("scan= is present");
    let work_at = line.find("work=").expect("work= is present");
    assert!(scan_at < work_at, "scan= must precede work=: {line}");
    assert!(
        line.contains("scan=\"Psylocke Scans\""),
        "a group name with a space must be quoted: {line}"
    );
}

/// A path a flag consumed is not also a member, and a flag is not a member
/// at all.
///
/// The two filters fail in opposite directions and both are silent. Keeping
/// the flags would refuse a real invocation for having no members; keeping
/// only the flag values would try to fingerprint `taiyaku.json`.
#[test]
fn a_path_a_flag_consumed_is_not_also_a_member() {
    let args: Vec<String> = [
        "one.cbz",
        "--sidecar",
        "taiyaku.json",
        "--against",
        "raw",
        "two.cbz",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();

    let members = member_args(&args, &[Some("taiyaku.json"), Some("raw"), None]);

    assert_eq!(
        members,
        vec![PathBuf::from("one.cbz"), PathBuf::from("two.cbz")]
    );
}

/// A three-member fan edition against a one-member raw hub, with fan pages
/// 3 to 5 — the whole back half of `c1` — unpaired.
///
/// `fan_and_raw`'s single unpaired page cannot separate a run's *length*
/// from its arithmetic: a run of one is `4 - 3`, and `4 / 3` is also one.
/// This fixture's run is three pages long, where subtraction, addition and
/// division give 3, 9 and 2.
fn fan_and_raw_with_a_long_run() -> Sidecar {
    let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Long Run", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 10 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [
                { "path": "c1.cbz", "sha256": "c1", "pages": 6 },
                { "path": "c2.cbz", "sha256": "c2", "pages": 4 }
              ],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0-2","b":"0-2"},{"a":"3-5"},{"a":"6-9","b":"6-9"}] }
          ]
        }"#;
    Sidecar::from_json(text).expect("fixture parses")
}

/// A row reports the run it came from and the recurrence at *its own* page.
///
/// Both are printed beside each other and neither is asserted anywhere
/// else: the length is arithmetic on a range, and the recurrence is a
/// lookup by page number into a list that holds one entry for every page in
/// the edition — so a lookup that matched the wrong way round still finds
/// something, and prints another page's nearest neighbour beside this one's
/// run.
#[test]
fn a_row_carries_its_runs_length_and_the_recurrence_at_its_own_page() {
    let doc = fan_and_raw_with_a_long_run();
    // `hash_with_bits_set` puts two hashes exactly |a - b| bits apart, so
    // the distances below are chosen rather than observed. Page 0 is 5 bits
    // from c2's first page; page 3 is 1 bit from c2's *third*. A row that
    // took page 0's recurrence would report 5 and `c2.cbz#0`.
    let c1: Vec<PageHash> = [0, 10, 20, 30, 40, 50]
        .iter()
        .map(|n| hash_with_bits_set(*n))
        .collect();
    let c2: Vec<PageHash> = [5, 150, 29, 160]
        .iter()
        .map(|n| hash_with_bits_set(*n))
        .collect();
    let fingerprinted = vec![("c1".to_string(), c1), ("c2".to_string(), c2)];

    let (rows, _, _) = classify(&doc, "fan", "raw", &fingerprinted, None).expect("classifies");

    assert_eq!(rows.len(), 1, "one unpaired run");
    let row = &rows[0];
    assert_eq!(row.page, 3);
    assert_eq!(row.run, 3, "pages 3, 4 and 5");
    assert_eq!(row.recurs, Some(1), "page 3's nearest, not page 0's");
    assert_eq!(row.nearest.as_deref(), Some("c2.cbz#2"));
    assert!(
        !row.credits,
        "three pages is longer than any credits card measured"
    );
}

#[test]
fn the_gotoubun_fixture_reproduces_the_four_candidates_notes_md_records() {
    let fixture = CreditFixture::gotoubun_v01();
    assert_eq!(fixture.work, "gotoubun-v01");
    assert_eq!(fixture.edition, "en-fan");
    assert_eq!(fixture.against, "raw-mono");
    assert_eq!(
        (fixture.pages, fixture.slots),
        (235, 240),
        "docs/notes.md's `credits:` line for gotoubun-v01 en-fan against \
             raw-mono, under \"The two `credits:` lines\""
    );
    assert!(
        !fixture.notes.is_empty(),
        "notes carries why page 43 is unreachable and why c002 local 40 is \
             excluded; a fixture that lost it would lose the reasoning"
    );
    let tally = fixture.tally();
    assert_eq!(
        tally.candidates,
        vec![95, 168, 198, 225],
        "signal 1's candidates on gotoubun-v01 en-fan are fixed by extraction, \
             not by whatever the rule currently emits"
    );
    assert_eq!(tally.hits, 4, "all four candidates are true positives");
    assert_eq!(
        tally.known, 5,
        "five credit pages are known to exist, and page 43 is the one signal 1 \
             cannot reach"
    );
}

#[test]
fn the_kaguya_fixture_reproduces_its_one_candidate_and_it_is_a_false_positive() {
    let fixture = CreditFixture::kaguya_v01();
    let tally = fixture.tally();
    assert_eq!(
        tally.candidates,
        vec![212],
        "c10 19/20 is the one page signal 1 takes here"
    );
    assert_eq!(
        tally.hits, 0,
        "page 212 is the publisher's colour announcement, not the group's matter"
    );
    assert_eq!(tally.known, 0, "this group prints no credit page at all");
}

#[test]
fn the_kobayashi_fixture_reproduces_its_nine_candidates_and_the_run_of_two_it_cannot_reach() {
    let fixture = CreditFixture::kobayashi_v01();
    let tally = fixture.tally();
    assert_eq!(
        tally.candidates,
        vec![16, 31, 46, 59, 74, 91, 104, 119, 134],
        "signal 1's candidates on kobayashi-v01 en-fan are fixed by \
             extraction, not by whatever the rule currently emits"
    );
    assert_eq!(tally.hits, 9, "all nine candidates are true positives");
    assert_eq!(
        tally.known, 10,
        "ten credit pages are known to exist. Page 152 is the tenth and the \
             one signal 1 cannot reach: c10 ends on a run of two, the publisher's \
             greyscale colophon at 151 and the group's card at 152, and \
             MAX_CREDIT_RUN = 1 rejects both"
    );
}

#[test]
fn the_kobayashi_v03_fixture_reproduces_its_eight_candidates_and_the_two_it_cannot_reach() {
    let fixture = CreditFixture::kobayashi_v03();
    let tally = fixture.tally();
    assert_eq!(
        tally.candidates,
        vec![20, 35, 48, 61, 76, 106, 121, 136],
        "signal 1's candidates on kobayashi-v03 en-fan are fixed by \
             extraction, not by whatever the rule currently emits"
    );
    assert_eq!(tally.hits, 8, "all eight candidates are true positives");
    assert_eq!(
        tally.known, 10,
        "ten credit pages are known to exist. Pages 91 and 152 are the two \
             signal 1 cannot reach, and NOT for kobayashi-v01's reason: neither \
             sits in any unpaired run at all, so no bound could admit them. The \
             solver paired both, and signal 1 only ever reads unpaired runs. \
             That is gotoubun-v01 page 43's ceiling, and these are its second \
             and third measured instances — the case where a credits page \
             happens to resemble something in the other edition well enough to \
             be matched"
    );
}

#[test]
fn the_kaguya_v08_fixture_reproduces_its_one_candidate_and_it_is_a_false_positive() {
    let fixture = CreditFixture::kaguya_v08();
    let tally = fixture.tally();
    assert_eq!(
        tally.candidates,
        vec![37],
        "signal 1's one candidate on kaguya-v08 en-fan is fixed by \
             extraction, not by whatever the rule currently emits"
    );
    assert_eq!(
        tally.hits, 0,
        "page 37 is signal 1's second false positive ever — this edition, \
             worked by three scanlation groups, carries no credit page at all"
    );
    assert_eq!(tally.known, 0, "no group prints a credit page here");
}

/// `domestic-v01` is a direct counterexample to `UnpairedRun::at_member_end`'s
/// own doc comment, not merely a miss.
///
/// That comment says the first-index version of the position rule "is zero
/// for four — what accumulates at a member's start is front matter",
/// and here it is five for five: all five of this edition's known credit
/// pages sit at the *start* of a chapter, and signal 1's rule reads only a
/// member's *last* index, so it structurally cannot see any of them.
/// Candidates is empty, not merely short of five.
#[test]
fn the_domestic_v01_fixture_is_a_direct_counterexample_to_the_first_index_claim() {
    let fixture = CreditFixture::domestic_v01();
    assert_eq!(fixture.work, "domestic-v01");
    assert_eq!(fixture.against, "en-official");

    assert!(
        fixture.runs().iter().all(|r| !r.at_member_end),
        "every one of this fixture's unpaired runs ends short of its \
             member's last index, which is *why* signal 1 finds nothing here: \
             is_candidate reads at_member_end and nothing else"
    );

    let tally = fixture.tally();
    assert_eq!(
        tally.candidates,
        Vec::<u32>::new(),
        "signal 1 proposes zero candidates on domestic-v01 en-fan"
    );
    assert_eq!(tally.hits, 0, "zero candidates, so zero hits");
    assert_eq!(
        tally.known, 5,
        "five credit pages are known by extraction: 2, 54, 95, 130 and 157 \
             — direct counterexample to UnpairedRun's doc comment, which was \
             measured on gotoubun-v01 alone and says the first-index version of \
             the rule 'is zero for four'. Here it is five for five at a \
             member's start"
    );
    assert_eq!(
        fixture.credit_pages,
        vec![2, 54, 95, 130, 157],
        "the five pages themselves, in ascending order"
    );
}

/// `solo-leveling-v01` is the first edition in the corpus where signal
/// 1's input does not exist, rather than its rule failing to reach a page
/// it can see.
///
/// The alignment is `REFUSED`, because vertical scroll strips do not
/// correspond to print pages, so there are no meaningful unpaired runs for
/// signal 1 to read at all. Signal 1 proposes none of this edition's known
/// credit pages. A future reader seeing "0 candidates" beside known credit
/// pages, without this context, would reasonably mistake it for the rule
/// breaking; it is instead the absence of the input the rule needs.
#[test]
fn the_solo_leveling_v01_fixture_has_no_alignment_for_signal_1_to_read() {
    let fixture = CreditFixture::solo_leveling_v01();
    let tally = fixture.tally();
    assert_eq!(
        tally.candidates,
        Vec::<u32>::new(),
        "signal 1 proposes zero candidates on solo-leveling-v01 en-fan — \
             not because its rule misses these nine pages, but because the \
             alignment underneath it is REFUSED (0 of 321 slots paired), so \
             there is no meaningful unpaired-run input for signal 1 to read"
    );
    assert_eq!(tally.hits, 0, "zero candidates, so zero hits");
    assert_eq!(
        tally.known, 9,
        "nine credit pages are known by extraction, by far the densest \
             edition in the corpus: THREE per chapter, at local 0, n-2 and \
             n-1 — a staff banner at each chapter's start, a patron thank-you \
             listing donor tiers, and a recruiting card at each chapter's end. \
             Six were found first and the patron pages only later, when a \
             threshold sweep flagged them as false positives and they turned \
             out not to be. This edition has yielded more every time it was \
             looked at, which is why the unswept editions' credit_pages are \
             floors rather than counts"
    );
}

#[test]
fn the_corpus_figures_are_twenty_three_of_twenty_five_precision_and_thirteen_of_fifteen_recall() {
    let mut candidates = 0usize;
    let mut hits = 0usize;
    let mut known_swept = 0usize;
    let mut hits_swept = 0usize;
    for fixture in CreditFixture::all() {
        let tally = fixture.tally();
        candidates += tally.candidates.len();
        hits += tally.hits;
        if tally.swept {
            known_swept += tally.known;
            hits_swept += tally.hits;
        }
    }
    assert_eq!(
        (hits, candidates),
        (23, 25),
        "precision: twenty-three of twenty-five candidates across the corpus \
             are true positives — wave 2 adds dungeon-meshi-v01's two true-positive \
             candidates at 40 and 65, while frieren-v01 proposes nothing and \
             domestic-v01 proposes nothing, so neither moves this figure. No wave-2 \
             edition adds a false positive beyond kaguya-v01's and kaguya-v08's. \
             Wave 3 adds solo-leveling-v01, which proposes nothing either — its \
             alignment is REFUSED, so signal 1 has no input to read — and so it \
             does not move this figure"
    );
    assert_eq!(
        (hits_swept, known_swept),
        (13, 15),
        "recall, over swept editions only: thirteen of the fifteen known \
             credit pages are found — gotoubun's paired page 43 and kobayashi-v01's \
             run-of-two page 152 are the two signal 1 cannot reach. All three wave-2 \
             editions carry \"swept\": false (design's §8) so none of them move this \
             figure, even domestic-v01's five known-but-unreached pages. \
             solo-leveling-v01 also carries \"swept\": false, so its six known \
             pages are excluded here too, despite none being reachable"
    );
}

#[test]
fn work_of_derives_the_corpus_directory_not_the_display_title() {
    // The value the checked-in fixtures key on: `work` is the corpus directory
    // slug, never the human display title (`doc.work.title` holds
    // `Go-Toubun no Hanayome v01`, an unrelated string with spaces in it).
    assert_eq!(work_of("corpus/gotoubun-v01/taiyaku.json"), "gotoubun-v01");
    assert_eq!(work_of("corpus/kaguya-v01/taiyaku.json"), "kaguya-v01");
    assert_eq!(
        work_of("/abs/path/to/corpus/gotoubun-v01/taiyaku.json"),
        "gotoubun-v01",
        "an absolute path derives the same slug from its immediate parent"
    );
}

#[test]
fn work_of_falls_back_to_the_whole_path_with_no_usable_parent() {
    // A bare filename with no directory component, and a path whose
    // parent is the filesystem root: neither has a directory-name
    // component to hand back. The fallback must stay non-empty and
    // visibly wrong rather than silently emit "".
    assert_eq!(work_of("taiyaku.json"), "taiyaku.json");
    assert_eq!(work_of("/taiyaku.json"), "/taiyaku.json");
}

#[test]
fn an_emitted_fixture_leaves_the_verdicts_empty_for_a_person_to_fill() {
    let runs = vec![run(95, 51, 1, true), run(1, 1, 42, false)];
    let json = emit_fixture_json("gotoubun-v01", "en-fan", "raw-mono", 235, 240, &runs);
    let back: crate::credit_fixture::CreditFixture =
        serde_json::from_str(&json).expect("an emitted fixture must parse as one");
    assert_eq!(back.runs.len(), 2, "every run is emitted, candidate or not");
    assert!(
        back.credit_pages.is_empty(),
        "the tool must not supply the verdicts it is being measured against"
    );
    assert!(
        !back.swept,
        "an edition is not swept until someone sweeps it"
    );
    assert!(
        back.runs.iter().all(|r| r.slots.is_some()),
        "an emitted run knows its slot range, unlike a hand-written one"
    );
}

#[test]
fn emit_fixture_with_no_value_is_refused_before_any_member_is_read() {
    let err = command(&["--emit-fixture".into()]).unwrap_err();
    assert!(
        err.contains("--emit-fixture"),
        "the refusal must name the flag: {err}"
    );
}
