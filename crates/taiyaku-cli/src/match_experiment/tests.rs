use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use taiyaku_core::{PageHash, PageRange, Run, Verdict};
use taiyaku_read::test_support::{TempDir, cbz_pages};

use super::features::{
    LONG_RUN, PairFeatures, containment_score, features, long_runs, oriented, volume_score,
};
use super::held_out::{
    Against, Bars, EXPLORATORY, HeldRow, Judgement, before_rounds, chapter_in_volume, frozen,
    held_out_list, held_rows, judge, one_freeze, provenance, read_bars, section, verdict,
};
use super::lookup::{
    Cost, Lookup, Print, Row, index, informative, kernel, leaders_json, lookup_all, lookup_class,
    lookup_txt, members_tsv, print_all, print_member, query_report, reaches, unreadable_line,
    votes,
};
use super::manifest::{
    Adjudication, Kind, Label, Manifest, Member, Printed, Renumbering, Source, Split, TABLE,
    Transcription, apply_truth, class, draft_member, explicit_pairs, is_omnibus_name, label,
    scored_pairs, split_of,
};
use super::rule::{MIN_CHAPTER_SLOTS, Rule, ScoredRow, dev_outcome, held_out_failures, report};
use super::score::{
    Scored, ShowLine, Shown, ToJudge, control_verdict, controls, dev_lines, framed, in_gates,
    next_round, read_leaders, render, round_line, sheet_pages, shown, sub_class,
};
use super::sheet::{
    Question, SheetPair, page_jpeg, page_uri, pairing_sheet, slug, truth_extract, truth_render,
};
use super::{
    Log, Rng, SAMPLE, TIMING_HEADER, Timing, check_rows, enough, head, kernel_lines, mount_of,
    nas_root, put, read_only, rebuild_manifest, run, sample, stamp, time_sample, timing_line,
    unescape, unusable, utc,
};

fn hashes(seed: u64, n: usize) -> Vec<PageHash> {
    let mut rng = Rng(seed);
    (0..n).map(|_| rng.hash()).collect()
}

/// `items` in an order fixed by `seed`: each takes a random key and they are
/// sorted by it.
fn shuffled(items: &[PageHash], seed: u64) -> Vec<PageHash> {
    let mut rng = Rng(seed);
    let mut keyed: Vec<(u64, PageHash)> = items.iter().map(|&h| (rng.next(), h)).collect();
    keyed.sort_by_key(|&(key, _)| key);
    keyed.into_iter().map(|(_, h)| h).collect()
}

fn flip(h: PageHash, bits: &[usize]) -> PageHash {
    let mut words = h.0;
    for &bit in bits {
        words[bit / 64] ^= 1 << (bit % 64);
    }
    PageHash(words)
}

fn with_ones(n: usize) -> PageHash {
    flip(PageHash([0; 4]), &(0..n).collect::<Vec<_>>())
}

const BLANK: PageHash = PageHash([0; 4]);

fn range((start, end): (u32, u32)) -> PageRange {
    PageRange { start, end }
}

fn paired(a: (u32, u32), b: (u32, u32)) -> Run {
    Run {
        a: range(a),
        b: Some(range(b)),
    }
}

fn unpaired(a: (u32, u32)) -> Run {
    Run {
        a: range(a),
        b: None,
    }
}

fn close(got: f64, want: f64) -> bool {
    (got - want).abs() < 1e-9
}

/// A run of three is chance; the span reads from the first long run's start
/// in B to the last one's end, inclusive.
#[test]
fn long_runs_counts_only_paired_runs_of_four_or_more() {
    let runs = [
        paired((0, 2), (10, 12)),
        unpaired((3, 3)),
        paired((4, 9), (13, 18)),
        paired((10, 13), (40, 43)),
    ];
    assert_eq!(long_runs(&runs, LONG_RUN), (10, 31));
}

#[test]
fn long_runs_with_no_long_run_is_zero() {
    let runs = [
        paired((0, 2), (0, 2)),
        paired((3, 5), (10, 12)),
        paired((6, 8), (20, 22)),
    ];
    assert_eq!(long_runs(&runs, LONG_RUN), (0, 0));
}

#[test]
fn volume_score_is_long_paired_over_the_longer_side() {
    assert!(close(volume_score(180, 190, 200), 0.9));
    assert!(close(volume_score(0, 0, 0), 0.0));
}

/// A chapter whose long runs fill one stretch of B its own length scores
/// high; the same runs spread over a whole volume do not.
#[test]
fn containment_score_rewards_one_contiguous_stretch() {
    assert!(close(containment_score(18, 20, 20), 0.81));
    assert!(close(containment_score(18, 20, 180), 0.09));
    assert!(close(containment_score(5, 20, 0), 0.0));
}

/// The instrument's first control: a member is its own best match.
#[test]
fn a_member_against_itself_scores_one() {
    let pages = hashes(7, 40);
    let f = features(&pages, &pages);
    assert!(f.volume >= 0.99, "{f:?}");
    assert!(f.containment >= 0.99, "{f:?}");
    assert_eq!(f.verdict, Verdict::Sound, "{f:?}");
}

/// The second control: the same pages in another order still pair a few by
/// chance, in runs too short to count. A reversal would not test this, as it
/// pairs one page at most.
#[test]
fn shuffled_pages_collapse() {
    let pages = hashes(7, 40);
    let f = features(&pages, &shuffled(&pages, 11));
    assert!(f.volume <= 0.10, "{f:?}");
}

/// Review Focus 3: the manifest or the lookup may name either member first.
#[test]
fn a_pair_scores_the_same_whichever_way_it_is_named() {
    let long = hashes(3, 45);
    let short: Vec<PageHash> = long[8..38]
        .iter()
        .map(|&h| flip(h, &[0, 100, 200]))
        .collect();
    let x = ("series/b.cbz", long.as_slice());
    let y = ("series/a.cbz", short.as_slice());
    let [xa, xb] = oriented(x, y);
    let [ya, yb] = oriented(y, x);
    assert_eq!((xa.0, ya.0), ("series/a.cbz", "series/a.cbz"));
    let (one, two) = (features(xa.1, xb.1), features(ya.1, yb.1));
    assert_eq!(one.a_slots, two.a_slots);
    assert_eq!(one.b_slots, two.b_slots);
    assert_eq!(one.verdict, two.verdict);
    assert_eq!(one.separation, two.separation);
    assert_eq!(one.margin, two.margin);
    assert_eq!(one.avg_run.to_bits(), two.avg_run.to_bits());
    assert_eq!(one.paired, two.paired);
    assert_eq!(one.long_paired, two.long_paired);
    assert_eq!(one.span_b, two.span_b);
    assert_eq!(one.volume.to_bits(), two.volume.to_bits());
    assert_eq!(one.containment.to_bits(), two.containment.to_bits());
    assert_eq!(one.a_slots, 30);

    let same = hashes(5, 30);
    let b = ("series/b.cbz", same.as_slice());
    let a = ("series/a.cbz", same.as_slice());
    assert_eq!(oriented(b, a)[0].0, "series/a.cbz");
    assert_eq!(oriented(a, b)[0].0, "series/a.cbz");
}

#[test]
fn the_bar_sits_the_gap_above_the_highest_negative() {
    let outcome = dev_outcome(&[(false, 0.2), (true, 0.9), (false, 0.41), (true, 0.5)]);
    assert!(
        outcome.bar.is_some_and(|bar| close(bar, 0.56)),
        "{outcome:?}"
    );
    assert_eq!(outcome.positives_clearing, 1);
    assert_eq!((outcome.positives, outcome.negatives), (2, 2));
    assert_eq!(outcome.max_negative, Some(0.41));
}

#[test]
fn no_bar_fits_when_a_negative_is_within_the_gap_of_one() {
    let outcome = dev_outcome(&[(false, 0.9), (true, 0.99)]);
    assert_eq!(outcome.bar, None);
    assert_eq!(outcome.positives_clearing, 0);
}

/// "At or above" is a failure: the bar keeps a wrong pair out only below it.
#[test]
fn a_held_out_negative_at_the_bar_fails() {
    let scored = [(false, 0.56), (false, 0.55), (true, 0.9)];
    assert_eq!(held_out_failures(&scored, 0.56), vec![0]);
}

#[test]
fn a_blank_slot_casts_no_vote() {
    assert!(!informative(BLANK));
    assert!(!informative(with_ones(7)));
    assert!(!informative(with_ones(249)));
    assert!(informative(with_ones(8)));
    assert!(informative(with_ones(248)));
}

/// The query's own slots are in the index at distance zero, so a lookup
/// that forgot to exclude them would vote for itself every time.
#[test]
fn each_informative_slot_votes_for_its_nearest_other_member() {
    let one = hashes(10, 12);
    let two = hashes(20, 12);
    let query: Vec<PageHash> = two
        .iter()
        .enumerate()
        .map(|(k, &h)| flip(h, &[k, k + 100]))
        .chain([BLANK])
        .collect();
    let members = vec![query, one, two];
    let idx = index(&members);
    assert_eq!((idx.hashes.len(), idx.owner.len()), (36, 36));
    let got = votes(&idx, 0, &members[0]);
    assert_eq!(got[0], (2, 12));
    assert!(got.iter().all(|&(member, _)| member != 0), "{got:?}");
}

#[test]
fn votes_rank_by_count_then_member_and_a_tie_goes_to_the_earlier_slot() {
    let base = hashes(60, 5);
    let near = |k: usize| flip(base[k], &[1]);
    let members = vec![
        Vec::new(),
        vec![near(0)],
        vec![near(1), near(2)],
        vec![near(3), near(4)],
    ];
    let got = votes(&index(&members), 0, &base);
    assert_eq!(got, vec![(2, 2), (3, 2), (1, 1)]);

    let level = vec![Vec::new(), vec![near(0)], vec![flip(base[0], &[2])]];
    assert_eq!(votes(&index(&level), 0, &base[..1]), vec![(1, 1)]);
}

/// Review Focus 4: an all-blank member asks nothing of the index, though
/// another member holds a near-blank page its slots would sit beside.
#[test]
fn a_query_with_no_informative_slot_has_no_votes() {
    let mut other = hashes(30, 5);
    other.push(with_ones(3));
    let members = vec![vec![BLANK; 5], other];
    assert_eq!(votes(&index(&members), 0, &members[0]), Vec::new());
}

#[test]
fn the_kernel_counts_every_comparison() {
    assert_eq!(kernel(&hashes(40, 3), &hashes(50, 5)).0, 15);
}

fn file_len(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).expect("a written file").len()
}

/// Review Focus 1: a RAR named `.cbz` and a directory with no images are each
/// listed with why they could not be read, and the run goes on to the next.
#[test]
fn an_unreadable_member_is_listed_with_its_reason() {
    let library = TempDir::new("match-unreadable");
    let cache = TempDir::new("match-unreadable-cache");
    std::fs::create_dir_all(library.join("s/empty")).expect("make the empty directory");
    std::fs::write(library.join("s/bad.cbz"), b"Rar!\x1a\x07\x00").expect("write bad.cbz");
    let good = cbz_pages(&library.join("s"), "good.cbz", 2, 1);
    let manifest = Manifest {
        members: ["s/bad.cbz", "s/empty", "s/good.cbz"]
            .map(|path| volume(path, "s", 1))
            .to_vec(),
        adjudications: Vec::new(),
    };

    let (prints, rows) = print_all(&cache, &library, &manifest);
    assert_eq!((prints.len(), rows.len()), (3, 3));
    let paths: Vec<&str> = rows.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(paths, ["s/bad.cbz", "s/empty", "s/good.cbz"]);
    for (i, name) in [(0, "s/bad.cbz"), (1, "s/empty")] {
        assert!(prints[i].is_none(), "{name}");
        let error = rows[i].error.as_deref().unwrap_or_default();
        assert!(error.contains(name), "{name}: {error}");
        assert_eq!((rows[i].pages, rows[i].slots), (0, 0), "{name}");
    }
    assert!(rows[0].error.as_deref().is_some_and(|e| e.contains("RAR")));
    let print = prints[2].as_ref().expect("good.cbz prints");
    assert_eq!(
        (print.pages, print.slots.len(), print.table.len()),
        (2, 2, 2)
    );
    assert_eq!(print.bytes, file_len(&good));
    assert_eq!(
        (&rows[2].error, rows[2].pages, rows[2].slots, rows[2].bytes),
        (&None, 2, 2, print.bytes)
    );

    let tsv = members_tsv("# probe", &rows);
    let lines: Vec<&str> = tsv.lines().collect();
    assert_eq!(lines.len(), 5, "{tsv}");
    assert_eq!(lines[0], "# probe");
    assert_eq!(
        lines[1],
        "path\tkind\tpages\tslots\tbytes\tms\tcache\terror"
    );
    let bad: Vec<&str> = lines[2].split('\t').collect();
    assert_eq!(bad.len(), 8, "{}", lines[2]);
    assert_eq!(&bad[..5], ["s/bad.cbz", "volume", "0", "0", "0"]);
    assert_eq!(bad[6], "-", "{}", lines[2]);
    assert!(bad[7].contains("RAR"), "{}", lines[2]);
    let fine: Vec<&str> = lines[4].split('\t').collect();
    assert_eq!(
        fine[..5],
        ["s/good.cbz", "volume", "2", "2", &print.bytes.to_string()]
    );
    assert_eq!((fine[6], fine[7]), ("miss", ""));
    assert_eq!(rows[2].cache, Some((0, 1)));
}

/// An error spanning lines or holding a tab stays one cell of one line.
#[test]
fn a_members_row_keeps_its_error_in_one_cell() {
    let row = Row {
        path: "s/a.cbz".to_owned(),
        kind: Kind::ChapterPack,
        pages: 0,
        slots: 0,
        bytes: 0,
        ms: 3,
        cache: None,
        error: Some("first\tsecond\nthird".to_owned()),
    };
    let tsv = members_tsv("# probe", std::slice::from_ref(&row));
    assert_eq!(
        tsv.lines().nth(2),
        Some("s/a.cbz\tchapter-pack\t0\t0\t0\t3\t-\tfirst second third")
    );
    let cells = |cache| {
        let printed = Row {
            cache: Some(cache),
            error: None,
            ..row.clone()
        };
        let tsv = members_tsv("# probe", &[printed]);
        let line = tsv.lines().nth(2).unwrap_or_default().to_owned();
        line.split('\t').nth(6).unwrap_or_default().to_owned()
    };
    assert_eq!(
        [cells((0, 2)), cells((2, 2)), cells((1, 2))],
        ["miss", "hit", "1 of 2"]
    );
}

/// A log line for an unreadable member is its error, which already names
/// the member, on one line.
#[test]
fn an_unreadable_line_names_its_member_once_on_one_line() {
    let row = Row {
        path: "s/bad.cbz".to_owned(),
        kind: Kind::Volume,
        pages: 0,
        slots: 0,
        bytes: 0,
        ms: 1,
        cache: None,
        error: Some("/lib/s/bad.cbz: first\nsecond\tthird".to_owned()),
    };
    assert_eq!(
        unreadable_line(&row).as_deref(),
        Some("unreadable: /lib/s/bad.cbz: first second third")
    );
    let printed = Row {
        error: None,
        cache: Some((0, 1)),
        ..row
    };
    assert_eq!(unreadable_line(&printed), None);
}

/// A directory of archives is one member read as its archives back to back,
/// in name order; the archive written first here is the second one read. Its
/// bytes are its archives' alone, though a note sits beside them.
#[test]
fn a_directory_of_archives_prints_as_its_archives_in_name_order() {
    let library = TempDir::new("match-archives");
    let cache = TempDir::new("match-archives-cache");
    let dir = library.join("s/v01-02");
    std::fs::create_dir_all(&dir).expect("make the pack directory");
    let second = cbz_pages(&dir, "v02.cbz", 3, 50);
    let first = cbz_pages(&dir, "v01.cbz", 2, 1);
    std::fs::write(dir.join("notes.txt"), b"not an archive").expect("write beside the archives");
    let alone = |archive: &std::path::Path| {
        let edition = [archive.to_path_buf()];
        crate::cached_edition(&cache, &edition, taiyaku_core::Direction::RightToLeft)
            .expect("an archive prints alone")
            .0
    };

    let print = print_member(&cache, &library, &pack("s/v01-02", "s", 1, 2)).expect("prints");
    assert_eq!(print.slots, [alone(&first), alone(&second)].concat());
    assert_eq!(print.pages, 5);
    assert_eq!(print.bytes, file_len(&first) + file_len(&second));
    assert_eq!((print.hits, print.paths), (0, 2));
    let again = print_member(&cache, &library, &pack("s/v01-02", "s", 1, 2)).expect("prints");
    assert_eq!((again.hits, again.paths), (2, 2));

    let beside = |n: u32, extra: &str| {
        let dir = library.join(format!("s/v{n:02}"));
        std::fs::create_dir_all(dir.join("sub")).expect("make the directory");
        cbz_pages(&dir, &format!("v{n:02}.cbz"), 2, 1);
        std::fs::write(dir.join(extra), b"never decoded").expect("write beside the archive");
        print_member(&cache, &library, &volume(&format!("s/v{n:02}"), "s", n))
    };
    for (n, page) in [(3, "cover.png"), (4, "sub/001.png")] {
        let why = beside(n, page).expect_err("pages beside an archive");
        assert!(
            why.contains(&format!("s/v{n:02}")) && why.contains("loose"),
            "{why}"
        );
    }
    let notes = beside(5, "sub/notes.txt").expect("a subdirectory with no pages is passed over");
    assert_eq!(notes.pages, 2);
}

/// A directory of loose pages stays one path, read as `align` reads it, and
/// its bytes are its files'.
#[test]
fn a_directory_of_loose_pages_prints_as_one_member() {
    let library = TempDir::new("match-loose");
    let cache = TempDir::new("match-loose-cache");
    let cbz = cbz_pages(&library, "pages.cbz", 3, 1);
    let dir = library.join("s/v01");
    let file = std::fs::File::open(&cbz).expect("open the cbz");
    (zip::ZipArchive::new(file).expect("a cbz is a zip"))
        .extract(&dir)
        .expect("unpack the pages");
    let files: u64 = (std::fs::read_dir(&dir).expect("list the pages"))
        .map(|entry| file_len(&entry.expect("a page").path()))
        .sum();

    let print = print_member(&cache, &library, &volume("s/v01", "s", 1)).expect("prints");
    let whole = print_member(&cache, &library, &volume("pages.cbz", "s", 1)).expect("prints");
    assert_eq!((print.pages, &print.slots), (3, &whole.slots));
    assert_eq!(print.bytes, files);
}

/// A page that does not decode names only itself, so the member is added.
#[test]
fn a_page_that_does_not_decode_names_its_member() {
    use std::io::Write as _;
    let library = TempDir::new("match-undecodable");
    let cache = TempDir::new("match-undecodable-cache");
    std::fs::create_dir_all(library.join("s")).expect("make the series directory");
    let file = std::fs::File::create(library.join("s/broken.cbz")).expect("create the cbz");
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file("000.png", zip::write::SimpleFileOptions::default())
        .expect("start the page");
    zip.write_all(b"not an image").expect("write the page");
    zip.finish().expect("finish the cbz");

    let why = print_member(&cache, &library, &volume("s/broken.cbz", "s", 1)).expect_err("broken");
    assert!(
        why.starts_with("s/broken.cbz: ") && why.contains("000.png"),
        "{why}"
    );
}

fn print_of(slots: Vec<PageHash>) -> Print {
    let pages = u32::try_from(slots.len()).expect("a test's slot count fits u32");
    Print {
        slots,
        table: Vec::new(),
        pages,
        bytes: 0,
        hits: 0,
        paths: 1,
    }
}

/// An unreadable member is indexed as holding nothing, so every vote still
/// names a manifest position. Each informative query slot is compared with
/// every indexed slot but the query's own.
#[test]
fn the_lookup_votes_for_every_printed_member_and_counts_its_comparisons() {
    let two = hashes(20, 12);
    let query: Vec<PageHash> = two
        .iter()
        .enumerate()
        .map(|(k, &h)| flip(h, &[k, k + 100]))
        .chain([BLANK])
        .collect();
    let prints = vec![
        Some(print_of(query)),
        None,
        Some(print_of(two)),
        Some(print_of(hashes(10, 6))),
    ];
    let (lookups, cost) = lookup_all(&prints);
    assert_eq!(lookups.len(), 4);
    assert!(lookups[1].is_none());
    let first = lookups[0].as_ref().expect("member 0 is looked up");
    assert_eq!((first.informative, first.votes[0]), (12, (2, 12)));
    assert_eq!(lookups[3].as_ref().map(|l| l.informative), Some(6));
    assert_eq!(cost.indexed, 30);
    assert_eq!(cost.comparisons, 12 * 18 + 12 * 18 + 6 * 24);
    assert_eq!(cost.threads, rayon::current_num_threads());
}

#[test]
fn leaders_keep_each_members_top_ten_with_its_informative_slots() {
    let members: Vec<Member> = (0..13)
        .map(|n| volume(&format!("s/v{n:02}"), "s", n))
        .collect();
    let mut lookups: Vec<Option<Lookup>> = vec![None; 13];
    lookups[0] = Some(Lookup {
        informative: 40,
        votes: (1..=11).map(|j| (j, 20 - j)).collect(),
    });
    lookups[1] = Some(Lookup {
        informative: 0,
        votes: Vec::new(),
    });
    let text = leaders_json("# probe", &members, &lookups);
    let json: serde_json::Value = serde_json::from_str(&text).expect("leaders.json parses");
    assert_eq!(json["stamp"], "# probe");
    let first = &json["members"]["s/v00"];
    assert_eq!(first["informative"], 40);
    let leaders = first["leaders"].as_array().expect("an array of leaders");
    assert_eq!(leaders.len(), 10);
    assert_eq!(leaders[0], serde_json::json!(["s/v01", 19]));
    assert_eq!(leaders[9], serde_json::json!(["s/v10", 10]));
    assert_eq!(
        json["members"]["s/v01"],
        serde_json::json!({"informative": 0, "leaders": []})
    );
    assert!(json["members"].get("s/v02").is_none(), "{text}");
}

#[test]
fn the_lookup_cost_reads_as_slots_comparisons_time_threads_and_rate() {
    let cost = Cost {
        indexed: 30,
        comparisons: 600,
        wall: std::time::Duration::from_millis(250),
        threads: 4,
    };
    assert_eq!(
        lookup_txt("# probe", &cost),
        "# probe\nslots indexed\t30\ncomparisons\t600\nwall seconds\t0.250\nthreads\t4\n\
         comparisons per second\t2400\n"
    );
    let instant = Cost {
        wall: std::time::Duration::ZERO,
        ..cost
    };
    let text = lookup_txt("# probe", &instant);
    assert!(text.ends_with("comparisons per second\tnone\n"), "{text}");
}

#[test]
fn a_share_is_reached_at_its_percentage_and_never_by_no_queries() {
    assert!(reaches(9, 10, 90));
    assert!(!reaches(8, 10, 90));
    assert!(reaches(3, 3, 100));
    assert!(!reaches(0, 0, 90));
}

fn looked_up(informative: u32, votes: &[(u32, u32)]) -> Lookup {
    Lookup {
        informative,
        votes: votes.to_vec(),
    }
}

/// A query's row in a lookup report, split into its cells.
fn report_row<'r>(report: &'r str, path: &str) -> Vec<&'r str> {
    (report.lines())
        .find(|line| line.starts_with(&format!("{path}\t")))
        .unwrap_or_else(|| panic!("no row for {path} in\n{report}"))
        .split('\t')
        .collect()
}

fn has_line(report: &str, want: &str) -> bool {
    report.lines().any(|line| line == want)
}

/// Each label's place in the lookup, one row per variant, both values of a
/// variant's field included.
#[test]
fn every_label_is_a_lookup_positive_a_negative_or_neither() {
    use Label::{
        ChapterIn, ChapterNotIn, CoverStories, InCollected, InPack, Neighbour, NotInCollected,
        NotInPack, Omnibus, OtherSeries, PackOverlap, SameSeriesOther, SameVolume, Unlabelled,
    };
    let want = [
        (SameVolume { copy: false }, Some(true)),
        (SameVolume { copy: true }, Some(true)),
        (InPack, Some(true)),
        (ChapterIn, Some(true)),
        (InCollected, Some(true)),
        (Neighbour, Some(false)),
        (SameSeriesOther, Some(false)),
        (NotInPack, Some(false)),
        (ChapterNotIn, Some(false)),
        (NotInCollected, Some(false)),
        (OtherSeries { franchise: false }, Some(false)),
        (OtherSeries { franchise: true }, Some(false)),
        (CoverStories, Some(false)),
        (Omnibus, None),
        (PackOverlap, None),
        (Unlabelled, None),
    ];
    for (label, class) in want {
        assert_eq!(lookup_class(label), class, "{label:?}");
    }
}

fn manifest_of(members: Vec<Member>) -> Manifest {
    Manifest {
        members,
        adjudications: Vec::new(),
    }
}

/// P1 fails on the query whose best positive ranks fourth, behind three wrong
/// members, and holds on one ranked third; a margin reads the votes against
/// the best wrong one's. A query with no positive is reported as such, with a
/// margin from its best negative, or none when no negative got a vote.
#[test]
fn a_best_positive_below_the_top_three_fails_p1() {
    let m = manifest_of(vec![
        volume("en/s1", "s", 1),
        volume("raw/s1", "s", 1),
        volume("en/t1", "t", 1),
        volume("en/u1", "u", 1),
        volume("en/w1", "w", 1),
        Member {
            split: Split::HeldOut,
            ..volume("en/h1", "h", 1)
        },
        volume("scan/s1", "s", 1),
    ]);
    let prints: Vec<Option<Print>> = (0..7).map(|k| Some(print_of(hashes(k, 20)))).collect();
    let lookups: Vec<Option<Lookup>> = [
        looked_up(30, &[(2, 9), (3, 7), (4, 5), (1, 3)]),
        looked_up(20, &[(2, 12), (3, 5), (0, 3)]),
        looked_up(10, &[(3, 4)]),
        looked_up(10, &[]),
        looked_up(10, &[]),
        looked_up(10, &[(0, 10)]),
        looked_up(0, &[]),
    ]
    .into_iter()
    .map(Some)
    .collect();
    let report = query_report("# probe", Split::Dev, &m, &prints, &lookups);
    assert_eq!(report.lines().next(), Some("# probe"));
    let want = [
        ["en/s1", "volume", "30", "4", "3", "9", "-0.2000"],
        ["raw/s1", "volume", "20", "3", "3", "12", "-0.4500"],
        ["en/t1", "volume", "10", "no positive", "-", "4", "-0.4000"],
        ["en/u1", "volume", "10", "no positive", "-", "0", "none"],
        ["scan/s1", "volume", "0", "none", "0", "0", "none"],
    ];
    for cells in want {
        assert_eq!(report_row(&report, cells[0]), cells, "{report}");
    }
    assert!(!report.contains("en/h1"), "a held-out query: {report}");
    for line in [
        "queries: 6, with a positive: 3",
        "best positive leads: 0 of 3",
        "depth needed: none, 1 best positive got no vote",
        "P1, every query's best positive in its top 3: 1 of 3 queries with a positive, fails",
        "P2, a member holding the chapter leads at least 90% of chapter queries: \
         0 of 0 chapters with a holder (-), not measured",
        "P2 on chapters of 8 or more slots: 0 of 0 (-)",
    ] {
        assert!(has_line(&report, line), "{line}\n{report}");
    }

    let held_out = query_report("# probe", Split::HeldOut, &m, &prints, &lookups);
    let h1 = ["en/h1", "volume", "10", "no positive", "-", "10", "-1.0000"];
    assert_eq!(report_row(&held_out, "en/h1"), h1, "{held_out}");
    assert!(!held_out.contains("en/s1"), "a dev query: {held_out}");
    for line in [
        "queries: 1, with a positive: 0",
        "depth needed: -",
        "P1, every query's best positive in its top 3: 0 of 0 queries with a positive, \
         not measured",
    ] {
        assert!(has_line(&held_out, line), "{line}\n{held_out}");
    }
}

/// P2 counts every chapter a member holds, however short, and no chapter
/// nothing holds; one led by a volume that does not hold it is a miss.
/// Chapters of 8 or more slots are tallied again on a line of their own. A
/// volume's chapters are its positives too.
#[test]
fn a_chapter_led_by_a_volume_not_holding_it_is_a_p2_miss() {
    let m = manifest_of(vec![
        chapter("ch/c10", "s", Some(2)),
        chapter("ch/c01", "s", Some(1)),
        chapter("ch/c00", "s", Some(1)),
        volume("en/v1", "s", 1),
        volume("en/v2", "s", 2),
        chapter("ch/c30", "s", Some(9)),
    ]);
    let least = usize::try_from(MIN_CHAPTER_SLOTS).expect("a slot count fits usize");
    let prints: Vec<Option<Print>> = [1, 2, 3, 4, 5, 6]
        .into_iter()
        .zip([12, least, least - 1, 100, 100, 12])
        .map(|(seed, n)| Some(print_of(hashes(seed, n))))
        .collect();
    let lookups: Vec<Option<Lookup>> = [
        looked_up(12, &[(3, 8), (4, 4)]),
        looked_up(8, &[(3, 6), (0, 1)]),
        looked_up(7, &[(3, 7)]),
        looked_up(90, &[(4, 30)]),
        looked_up(90, &[]),
        looked_up(12, &[(3, 5)]),
    ]
    .into_iter()
    .map(Some)
    .collect();
    let report = query_report("# probe", Split::Dev, &m, &prints, &lookups);
    let want = [
        ["ch/c10", "chapter", "12", "2", "4", "8", "-0.3333"],
        ["ch/c01", "chapter", "8", "1", "6", "1", "0.6250"],
        ["ch/c00", "chapter", "7", "1", "7", "0", "1.0000"],
        ["en/v1", "volume", "90", "none", "0", "30", "-0.3333"],
        ["en/v2", "volume", "90", "none", "0", "0", "0.0000"],
        [
            "ch/c30",
            "chapter",
            "12",
            "no positive",
            "-",
            "5",
            "-0.4167",
        ],
    ];
    for cells in want {
        assert_eq!(report_row(&report, cells[0]), cells, "{report}");
    }
    for line in [
        "queries: 6, with a positive: 5",
        "best positive leads: 2 of 5",
        "depth needed: none, 2 best positives got no vote",
        "P1, every query's best positive in its top 3: 3 of 5 queries with a positive, fails",
        "P2, a member holding the chapter leads at least 90% of chapter queries: \
         2 of 3 chapters with a holder (66.7%), fails",
        "P2 on chapters of 8 or more slots: 1 of 2 (50.0%)",
    ] {
        assert!(has_line(&report, line), "{line}\n{report}");
    }
}

/// A pack is a positive for the volumes and chapters it holds, whichever is
/// the query, and leads a chapter as its holder. A chapter led by another
/// copy of itself has its best positive first and no holder leading it. An
/// omnibus is neither: it takes a place in the ranking but is no one's best
/// negative.
#[test]
fn a_pack_is_found_by_what_it_holds_and_an_omnibus_by_nothing() {
    let m = Manifest {
        members: vec![
            volume("en/v03", "s", 3),
            pack("raw/v03-04", "s", 3, 4),
            chapter("ch/c20", "s", Some(3)),
            volume("en/v05", "s", 5),
            member("omni/v1", "s-perfect", "s", Kind::Omnibus),
            chapter("scan/c20", "s", Some(3)),
        ],
        adjudications: vec![adjudication("ch/c20", "scan/c20", true)],
    };
    let prints: Vec<Option<Print>> = [1, 2, 3, 4, 5, 6]
        .into_iter()
        .zip([20, 40, 12, 20, 60, 12])
        .map(|(seed, n)| Some(print_of(hashes(seed, n))))
        .collect();
    let lookups: Vec<Option<Lookup>> = [
        looked_up(20, &[(1, 9), (3, 2)]),
        looked_up(30, &[(4, 8), (0, 6), (3, 3)]),
        looked_up(12, &[(1, 7), (0, 4)]),
        looked_up(10, &[(0, 3)]),
        looked_up(0, &[]),
        looked_up(12, &[(2, 9), (1, 2)]),
    ]
    .into_iter()
    .map(Some)
    .collect();
    let report = query_report("# probe", Split::Dev, &m, &prints, &lookups);
    let want = [
        ["en/v03", "volume", "20", "1", "9", "2", "0.3500"],
        ["raw/v03-04", "pack", "30", "2", "6", "3", "0.1000"],
        ["ch/c20", "chapter", "12", "1", "7", "0", "0.5833"],
        ["en/v05", "volume", "10", "no positive", "-", "3", "-0.3000"],
        ["omni/v1", "omnibus", "0", "no positive", "-", "0", "none"],
        ["scan/c20", "chapter", "12", "1", "9", "0", "0.7500"],
    ];
    for cells in want {
        assert_eq!(report_row(&report, cells[0]), cells, "{report}");
    }
    for line in [
        "queries: 6, with a positive: 4",
        "best positive leads: 3 of 4",
        "depth needed: 2",
        "P1, every query's best positive in its top 3: 4 of 4 queries with a positive, holds",
        "P2, a member holding the chapter leads at least 90% of chapter queries: \
         1 of 2 chapters with a holder (50.0%), fails",
        "P2 on chapters of 8 or more slots: 1 of 2 (50.0%)",
    ] {
        assert!(has_line(&report, line), "{line}\n{report}");
    }
}

/// A query whose positives all failed to print says so, apart from one that
/// has none, and is out of P1 either way; one printed positive is enough.
#[test]
fn a_query_whose_positives_all_failed_to_print_says_so() {
    let m = manifest_of(vec![
        volume("en/s1", "s", 1),
        volume("raw/s1", "s", 1),
        volume("en/t1", "t", 1),
        volume("en/s2", "s", 2),
        volume("en/u1", "u", 1),
        volume("raw/u1", "u", 1),
        volume("scan/u1", "u", 1),
    ]);
    let unprinted = [1, 5];
    let prints: Vec<Option<Print>> = (0..7)
        .map(|k| (!unprinted.contains(&k)).then(|| print_of(hashes(k, 20))))
        .collect();
    let lookups = vec![
        Some(looked_up(10, &[(3, 5), (2, 1)])),
        None,
        Some(looked_up(10, &[(0, 2)])),
        Some(looked_up(10, &[(0, 4)])),
        Some(looked_up(10, &[(6, 3)])),
        None,
        Some(looked_up(10, &[(4, 3)])),
    ];
    let report = query_report("# probe", Split::Dev, &m, &prints, &lookups);
    let want = [
        [
            "en/s1",
            "volume",
            "10",
            "positive unprinted",
            "-",
            "5",
            "-0.5000",
        ],
        ["en/t1", "volume", "10", "no positive", "-", "2", "-0.2000"],
        ["en/s2", "volume", "10", "no positive", "-", "4", "-0.4000"],
        ["en/u1", "volume", "10", "1", "3", "0", "0.3000"],
    ];
    for cells in want {
        assert_eq!(report_row(&report, cells[0]), cells, "{report}");
    }
    assert!(report.lines().all(|l| !l.starts_with("raw/")), "{report}");
    for line in [
        "queries: 5, with a positive: 2",
        "queries whose positives all failed to print: 1",
        "P1, every query's best positive in its top 3: 2 of 2 queries with a positive, holds",
    ] {
        assert!(has_line(&report, line), "{line}\n{report}");
    }
}

/// P1 asks for every query: nine best positives of ten in the top 3 fail it.
#[test]
fn p1_fails_at_nine_of_ten_in_the_top_three() {
    let mut members: Vec<Member> = (0..10)
        .map(|k| volume(&format!("e{k}/s1"), "s", 1))
        .collect();
    members.extend([
        volume("x/t1", "t", 1),
        volume("x/u1", "u", 1),
        volume("x/w1", "w", 1),
    ]);
    let prints: Vec<Option<Print>> = (0..13).map(|k| Some(print_of(hashes(k, 20)))).collect();
    let lookups: Vec<Option<Lookup>> = (0..13)
        .map(|k| match k {
            0..=8 => Some(looked_up(20, &[(k + 1, 5)])),
            9 => Some(looked_up(20, &[(10, 5), (11, 4), (12, 3), (0, 2)])),
            _ => Some(looked_up(20, &[])),
        })
        .collect();
    let report = query_report(
        "# probe",
        Split::Dev,
        &manifest_of(members),
        &prints,
        &lookups,
    );
    let line =
        "P1, every query's best positive in its top 3: 9 of 10 queries with a positive, fails";
    assert!(has_line(&report, line), "{line}\n{report}");
}

/// Ten chapters of one volume, `hits` of them led by it and the rest by the
/// next volume. The first has 10 slots, 5 of them blank: it belongs on the
/// 8-slot line by its slot count, though only 5 of its slots are informative.
fn p2_report(hits: u32) -> String {
    let mut members = vec![volume("en/v1", "s", 1), volume("en/v2", "s", 2)];
    members.extend((0..10).map(|k| chapter(&format!("ch/c{k:02}"), "s", Some(1))));
    let half_blank: Vec<PageHash> = hashes(40, 5).into_iter().chain([BLANK; 5]).collect();
    let mut prints = vec![
        Some(print_of(hashes(1, 100))),
        Some(print_of(hashes(2, 100))),
        Some(print_of(half_blank)),
    ];
    prints.extend((1..10).map(|k| Some(print_of(hashes(10 + k, 12)))));
    let mut lookups = vec![Some(looked_up(90, &[])), Some(looked_up(90, &[]))];
    lookups.extend((0..10).map(|k| {
        let votes: &[(u32, u32)] = if k < hits {
            &[(0, 5)]
        } else {
            &[(1, 5), (0, 2)]
        };
        Some(looked_up(if k == 0 { 5 } else { 12 }, votes))
    }));
    query_report(
        "# probe",
        Split::Dev,
        &manifest_of(members),
        &prints,
        &lookups,
    )
}

/// P2's 90%: nine chapters of ten led by a holder hold it, eight fail it.
#[test]
fn p2_holds_at_nine_of_ten_and_fails_at_eight() {
    let nine = p2_report(9);
    for line in [
        "P2, a member holding the chapter leads at least 90% of chapter queries: \
         9 of 10 chapters with a holder (90.0%), holds",
        "P2 on chapters of 8 or more slots: 9 of 10 (90.0%)",
    ] {
        assert!(has_line(&nine, line), "{line}\n{nine}");
    }
    let eight = p2_report(8);
    let line = "P2, a member holding the chapter leads at least 90% of chapter queries: \
                8 of 10 chapters with a holder (80.0%), fails";
    assert!(has_line(&eight, line), "{line}\n{eight}");
}

/// Review Focus 5: a typed path with nothing usable behind it is refused,
/// never read as absent.
#[test]
fn an_unusable_path_says_why() {
    let why = unusable("TAIYAKU_MATCH_LIBRARY", "manga-test", true).expect("relative is refused");
    assert!(why.contains("TAIYAKU_MATCH_LIBRARY"), "{why}");
    assert!(why.contains("absolute"), "{why}");
    assert!(unusable("TAIYAKU_MATCH_EVIDENCE", "/nope", false).is_some());
    assert_eq!(unusable("TAIYAKU_MATCH_LIBRARY", "/abs", true), None);
}

#[test]
fn utc_counts_leap_days_and_skips_the_century_one() {
    assert_eq!(utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(utc(1_700_000_000), "2023-11-14T22:13:20Z");
    assert_eq!(utc(1_772_323_200), "2026-03-01T00:00:00Z");
    assert_eq!(utc(4_107_542_399), "2100-02-28T23:59:59Z");
}

/// A run's log opens with its stamp, before any line the run adds, and a
/// rerun replaces it: line 1 dates the latest run only, and the run's
/// `cache hits` say whether its fingerprints are as new as that.
#[test]
fn a_runs_log_opens_with_the_latest_runs_stamp() {
    let dir = TempDir::new("match-log");
    let path = dir.join("logs/fingerprint.log");
    let mut log = Log::create(&path, "# probe 2026-11-16T00:00:00Z HEAD abc");
    log.line("members: 3");
    drop(log);
    let text = std::fs::read_to_string(&path).expect("the log was written");
    assert_eq!(text, "# probe 2026-11-16T00:00:00Z HEAD abc\nmembers: 3\n");
    drop(Log::create(&path, "# rerun"));
    let text = std::fs::read_to_string(&path).expect("the log was rewritten");
    assert_eq!(
        text, "# rerun\n",
        "the rerun's stamp replaces the first run's"
    );
}

/// The commit a stamp names, from what git said: dirty when the module has
/// uncommitted changes, unknown when either call failed.
#[test]
fn a_stamp_marks_a_module_with_uncommitted_changes() {
    let rev = || Some("abc1234\n".to_owned());
    assert_eq!(head(rev(), Some(String::new())), "abc1234");
    let changed = Some(" M crates/taiyaku-cli/src/match_experiment.rs\n".to_owned());
    assert_eq!(head(rev(), changed), "abc1234+dirty");
    assert_eq!(head(None, Some(String::new())), "unknown");
    assert_eq!(head(rev(), None), "unknown");
    assert_eq!(head(Some("\n".to_owned()), Some(String::new())), "unknown");
}

/// A run refuses rows that are not the manifest's members in its order, and
/// a run where nothing printed.
#[test]
fn a_run_refuses_rows_that_are_not_its_members_or_where_nothing_printed() {
    let m = manifest_of(vec![volume("s/a.cbz", "s", 1), volume("s/b.cbz", "s", 2)]);
    let row = |path: &str, error: Option<&str>| Row {
        path: path.to_owned(),
        kind: Kind::Volume,
        pages: 0,
        slots: 0,
        bytes: 0,
        ms: 0,
        cache: None,
        error: error.map(str::to_owned),
    };
    let (a, b) = (row("s/a.cbz", None), row("s/b.cbz", Some("unreadable")));
    assert_eq!(check_rows(&[a.clone(), b.clone()], &m), Ok(()));
    assert!(
        check_rows(std::slice::from_ref(&a), &m).is_err(),
        "a row missing"
    );
    assert!(
        check_rows(&[b.clone(), a], &m).is_err(),
        "rows out of order"
    );
    let none = check_rows(&[row("s/a.cbz", Some("unreadable")), b], &m);
    assert_eq!(none, Err("no member was fingerprinted".to_owned()));
}

/// `fingerprint_and_lookup`'s run over a fixture: the evidence it writes, its
/// log opening with the stamp, and its cache hits, none on the first run and
/// one per printed member on the second. The fixture lists the run's own log
/// as a member, so that row shows the log existed before any member was read:
/// it fails as no cbz, never as a missing file.
#[test]
fn a_run_writes_its_evidence_and_counts_its_cache_hits() {
    let root = TempDir::new("match-run");
    let (library, ev) = (root.join("library"), root.join("ev"));
    let pack_dir = library.join("s/v02-03");
    std::fs::create_dir_all(library.join("s/empty")).expect("make the empty directory");
    std::fs::create_dir_all(&pack_dir).expect("make the pack directory");
    cbz_pages(&library.join("s"), "good.cbz", 2, 1);
    std::fs::write(library.join("s/bad.cbz"), b"Rar!\x1a\x07\x00").expect("write bad.cbz");
    cbz_pages(&pack_dir, "v02.cbz", 2, 20);
    cbz_pages(&pack_dir, "v03.cbz", 3, 40);
    let own_log = "../ev/logs/fingerprint.log";
    let m = manifest_of(vec![
        volume("s/bad.cbz", "s", 9),
        volume("s/empty", "s", 8),
        volume("s/good.cbz", "s", 1),
        pack("s/v02-03", "s", 2, 3),
        volume(own_log, "s", 7),
    ]);
    let read = |name: &str| {
        std::fs::read_to_string(ev.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    };

    for (round, hits) in [(1, 0), (2, 2)] {
        run(&library, &ev, &m).unwrap_or_else(|e| panic!("run {round}: {e}"));
        let log = read("logs/fingerprint.log");
        let first = log.lines().next().unwrap_or_default();
        assert!(
            first.starts_with("# fingerprint_and_lookup "),
            "run {round}: {log}"
        );
        for name in ["members.tsv", "lookup.txt", "dev/lookup.txt"] {
            assert_eq!(
                read(name).lines().next(),
                Some(first),
                "run {round}: {name}"
            );
        }
        let leaders: serde_json::Value =
            serde_json::from_str(&read("leaders.json")).expect("leaders.json parses");
        assert_eq!(leaders["stamp"], first, "run {round}");
        let summary =
            format!("members: 5, fingerprinted: 2, unreadable: 3 (60.0%), cache hits: {hits}");
        assert!(has_line(&log, &summary), "run {round}: {summary}\n{log}");

        let tsv = read("members.tsv");
        let listed: Vec<&str> = (tsv.lines().skip(2))
            .map(|line| line.split('\t').next().unwrap_or_default())
            .collect();
        let members: Vec<&str> = m.members.iter().map(|x| x.path.as_str()).collect();
        assert_eq!(listed, members, "run {round}");
        let own = report_row(&tsv, own_log);
        assert!(
            own[7].contains("not a readable cbz"),
            "run {round}: {}",
            own[7]
        );
        let bad: Vec<&str> = log.lines().filter(|l| l.contains("bad.cbz")).collect();
        assert_eq!(bad.len(), 1, "run {round}: {log}");
        assert!(
            bad[0].starts_with("unreadable: "),
            "run {round}: {}",
            bad[0]
        );
        assert_eq!(
            bad[0].matches("bad.cbz").count(),
            1,
            "run {round}: {}",
            bad[0]
        );
    }
}

#[test]
fn the_stamp_names_the_test_the_time_and_the_commit() {
    let line = stamp("probe");
    let fields: Vec<&str> = line.split(' ').collect();
    assert_eq!(fields.len(), 5, "{line}");
    assert_eq!(
        (fields[0], fields[1], fields[3]),
        ("#", "probe", "HEAD"),
        "{line}"
    );
    assert!(fields[2].len() == 20 && fields[2].ends_with('Z'), "{line}");
    assert!(!fields[4].is_empty(), "{line}");
}

fn parsed(dir: &str, name: &str) -> Member {
    let row = TABLE
        .iter()
        .find(|r| r.dir == dir)
        .unwrap_or_else(|| panic!("no row {dir}"));
    draft_member(&format!("{dir}/{name}"), row)
}

type Fields<'m> = (
    Kind,
    Option<(u32, u32)>,
    Option<&'m str>,
    Option<&'m str>,
    Option<u32>,
    Option<u32>,
);

fn fields(m: &Member) -> Fields<'_> {
    (
        m.kind,
        m.volumes,
        m.copy.as_deref(),
        m.chapter.as_deref(),
        m.in_volume,
        m.issue,
    )
}

#[test]
fn names_parse_into_volumes_packs_chapters_and_copies() {
    use Kind::{Chapter, ChapterPack, Issue, Pack, Volume};
    let oshi = "Raw/[Oshi no Ko]";
    let kaguya = "Raw/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen";
    let kaguya_ch =
        "Chapters/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - Fan Translated";
    let kaguya_name = "Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen";
    let issues = "Comics/Issues/Invincible/Invincible (2003-2018)";

    let v01 = (Volume, Some((1, 1)), None, None, None, None);
    assert_eq!(fields(&parsed(oshi, "Oshinoko v01")), v01);
    assert_eq!(
        fields(&parsed(oshi, "Oshinoko v01-03")),
        (Pack, Some((1, 3)), None, None, None, None)
    );
    assert_eq!(
        fields(&parsed(oshi, "Oshinoko v01b")),
        (Volume, Some((1, 1)), Some("b"), None, None, None)
    );
    assert_eq!(
        fields(&parsed(kaguya, "Kaguya-sama_Zunousen_v01_DC")),
        (Volume, Some((1, 1)), Some("_DC"), None, None, None)
    );
    assert_eq!(
        fields(&parsed(
            "Raw/Kobayashi-san Chi no Meidoragon",
            "DLRAW.TO_Kobayashi-san_Maid_Drago_v01-07A"
        )),
        (Pack, Some((1, 7)), Some("A"), None, None, None)
    );
    let whole = parsed(kaguya_ch, &format!("{kaguya_name} - v16 c152.cbz"));
    assert_eq!(
        (fields(&whole), whole.truth.as_deref()),
        (
            (Chapter, None, None, Some("152"), Some(16), None),
            Some("name")
        )
    );
    let fraction = parsed(kaguya_ch, &format!("{kaguya_name} - v1 c5.5.cbz"));
    assert_eq!(
        (fields(&fraction), fraction.truth.as_deref()),
        ((Chapter, None, None, Some("5.5"), None, None), None)
    );
    let zero = parsed(
        "Chapters/Bocchi the Rock! - Fan Translated",
        "Bocchi the Rock - v1 c0.cbz",
    );
    assert_eq!(
        (fields(&zero), zero.truth.as_deref()),
        ((Chapter, None, None, Some("0"), None, None), None)
    );
    assert_eq!(
        fields(&parsed(
            "Chapters/One Piece",
            "One Piece - c1134 (2024).cbz"
        )),
        (Chapter, None, None, Some("1134"), None, None)
    );
    assert_eq!(
        fields(&parsed(
            "Raw/Zelda no Densetsu - Twilight Princess",
            "ch01-15"
        )),
        (ChapterPack, None, None, Some("1-15"), None, None)
    );
    assert_eq!(
        fields(&parsed(
            issues,
            "Invincible 001 (2003) (digital) (Minutemen-InnerDemons).cbz"
        )),
        (Issue, None, None, None, None, Some(1))
    );
    assert_eq!(
        fields(&parsed(
            "Volumes/[Oshi no Ko]",
            "[Oshi no Ko] - v01 (2023).cbz"
        )),
        v01
    );
}

/// A synthetic member: the directory is the path up to its last `/`.
fn member(path: &str, series: &str, franchise: &str, kind: Kind) -> Member {
    Member {
        path: path.to_owned(),
        series: series.to_owned(),
        franchise: franchise.to_owned(),
        edition: path.split('/').next().unwrap_or_default().to_owned(),
        kind,
        volumes: None,
        copy: None,
        chapter: None,
        in_volume: None,
        after_volume: None,
        issue: None,
        issues: None,
        ltr: false,
        split: Split::Dev,
        truth: None,
    }
}

fn volume(path: &str, series: &str, n: u32) -> Member {
    Member {
        volumes: Some((n, n)),
        ..member(path, series, series, Kind::Volume)
    }
}

fn pack(path: &str, series: &str, first: u32, last: u32) -> Member {
    Member {
        volumes: Some((first, last)),
        ..member(path, series, series, Kind::Pack)
    }
}

fn chapter(path: &str, series: &str, in_volume: Option<u32>) -> Member {
    Member {
        chapter: Some("1".to_owned()),
        in_volume,
        truth: in_volume.map(|_| "name".to_owned()),
        ..member(path, series, series, Kind::Chapter)
    }
}

fn issue(path: &str, n: u32) -> Member {
    Member {
        issue: Some(n),
        ..member(path, "comic", "comic", Kind::Issue)
    }
}

fn collected(path: &str, n: u32, issues: Option<Vec<u32>>) -> Member {
    Member {
        volumes: Some((n, n)),
        issues,
        ..member(path, "comic", "comic", Kind::Collected)
    }
}

/// Features whose short side holds `slots` slots, against 200 unrelated ones.
fn a_side_of(slots: u32) -> PairFeatures {
    let n = usize::try_from(slots).expect("a slot count fits usize");
    features(&hashes(1, n), &hashes(2, 200))
}

/// One pair per `Label` variant, each with the label it must get, the two pack
/// pairs either side of an overlap, and [`gated_and_truthless_pairs`].
fn one_pair_per_label() -> Vec<(Member, Member, Label)> {
    use Label::{
        ChapterIn, ChapterNotIn, CoverStories, InCollected, InPack, Neighbour, NotInCollected,
        NotInPack, Omnibus, OtherSeries, PackOverlap, SameSeriesOther, SameVolume, Unlabelled,
    };
    let spin_off = Member {
        volumes: Some((1, 1)),
        ..member("en/b1", "s-spin", "s", Kind::Volume)
    };
    let omnibus = Member {
        volumes: Some((1, 1)),
        ..member("en/o1", "s-big", "s", Kind::Omnibus)
    };
    let extra = member("en/x1", "s-extra", "s", Kind::Extra);
    vec![
        (
            volume("raw/v1", "s", 1),
            volume("raw/v1b", "s", 1),
            SameVolume { copy: true },
        ),
        (
            volume("en/v1", "s", 1),
            volume("raw/v1", "s", 1),
            SameVolume { copy: false },
        ),
        (volume("en/v1", "s", 1), volume("raw/v2", "s", 2), Neighbour),
        (
            volume("en/v1", "s", 1),
            volume("raw/v3", "s", 3),
            SameSeriesOther,
        ),
        (
            pack("raw/p2-4", "s", 2, 4),
            pack("raw/p1-3", "s", 1, 3),
            PackOverlap,
        ),
        (
            pack("raw/p4-6", "s", 4, 6),
            pack("raw/p1-3", "s", 1, 3),
            SameSeriesOther,
        ),
        (
            pack("en/p1-3", "s", 1, 3),
            pack("raw/p1-3", "s", 1, 3),
            SameVolume { copy: false },
        ),
        (volume("en/v2", "s", 2), pack("raw/p1-3", "s", 1, 3), InPack),
        (
            volume("en/v4", "s", 4),
            pack("raw/p1-3", "s", 1, 3),
            NotInPack,
        ),
        (
            chapter("ch/c1", "s", Some(2)),
            pack("raw/p1-3", "s", 1, 3),
            ChapterIn,
        ),
        (
            chapter("ch/c1", "s", Some(2)),
            volume("en/v3", "s", 3),
            ChapterNotIn,
        ),
        (
            issue("is/i1", 1),
            collected("co/v1", 1, Some(vec![0, 1, 2])),
            InCollected,
        ),
        (
            issue("is/i5", 5),
            collected("co/v1", 1, Some(vec![0, 1, 2])),
            NotInCollected,
        ),
        (
            spin_off,
            volume("en/v1", "s", 1),
            OtherSeries { franchise: true },
        ),
        (
            volume("en/v1", "s", 1),
            volume("en/t1", "t", 1),
            OtherSeries { franchise: false },
        ),
        (extra, volume("en/v1", "s", 1), CoverStories),
        (omnibus, volume("en/v1", "s", 1), Omnibus),
        (
            chapter("ch/c9", "s", None),
            volume("en/v1", "s", 1),
            Unlabelled,
        ),
    ]
    .into_iter()
    .chain(gated_and_truthless_pairs())
    .collect()
}

/// Pairs whose A side a rule's gate reads, and each case the names leave
/// without truth.
fn gated_and_truthless_pairs() -> Vec<(Member, Member, Label)> {
    use Label::{CoverStories, OtherSeries, Unlabelled};
    vec![
        (
            chapter("ch/c1", "s", Some(2)),
            volume("en/t1", "t", 1),
            OtherSeries { franchise: false },
        ),
        (
            issue("is/i1", 1),
            volume("en/v1", "s", 1),
            OtherSeries { franchise: false },
        ),
        (
            chapter("ch/c1", "s", Some(2)),
            member("en/x1", "s-extra", "s", Kind::Extra),
            CoverStories,
        ),
        (
            chapter("ch/t1", "t", Some(1)),
            pack("raw/p1-3", "s", 1, 3),
            OtherSeries { franchise: false },
        ),
        (
            member("raw/ch1-15", "s", "s", Kind::ChapterPack),
            volume("en/v1", "s", 1),
            Unlabelled,
        ),
        (
            member("en/one", "s", "s", Kind::Volume),
            volume("en/v1", "s", 1),
            Unlabelled,
        ),
        (issue("is/i1", 1), collected("co/v2", 2, None), Unlabelled),
    ]
}

#[test]
fn labels_are_symmetric() {
    let mut members: Vec<Member> = Vec::new();
    for (a, b, _) in one_pair_per_label() {
        for m in [a, b] {
            if !members
                .iter()
                .any(|x| x.path == m.path && x.series == m.series)
            {
                members.push(m);
            }
        }
    }
    members.push(issue("is/i1b", 1));
    let m = Manifest {
        adjudications: vec![
            adjudication("en/v1", "raw/v3", true),
            adjudication("ch/c9", "en/v1", true),
        ],
        members: members.clone(),
    };
    assert!(members.len() >= 12, "{} members", members.len());
    let (p13, p24) = (pack("raw/p1-3", "s", 1, 3), pack("raw/p2-4", "s", 2, 4));
    let both_ways = (label(&m, &p13, &p24), label(&m, &p24, &p13));
    assert_eq!(both_ways, (Label::PackOverlap, Label::PackOverlap));
    for a in &members {
        for b in &members {
            assert_eq!(label(&m, a, b), label(&m, b, a), "{} / {}", a.path, b.path);
        }
    }
}

#[test]
fn labels_follow_the_class_rules() {
    use Label::{
        ChapterIn, ChapterNotIn, CoverStories, InCollected, InPack, Neighbour, NotInCollected,
        NotInPack, Omnibus, OtherSeries, PackOverlap, SameSeriesOther, SameVolume, Unlabelled,
    };
    let m = Manifest::default();
    let f = a_side_of(MIN_CHAPTER_SLOTS);
    // Each row: the label, then its class under the volume, chapter and pack rules.
    let want = [
        (SameVolume { copy: true }, Some(true), None, None),
        (SameVolume { copy: false }, Some(true), None, None),
        (Neighbour, Some(false), None, None),
        (SameSeriesOther, Some(false), None, None),
        (PackOverlap, None, None, None),
        (SameSeriesOther, Some(false), None, None),
        (SameVolume { copy: false }, Some(true), None, None),
        (InPack, Some(false), None, Some(true)),
        (NotInPack, Some(false), None, Some(false)),
        (ChapterIn, Some(false), Some(true), None),
        (ChapterNotIn, Some(false), Some(false), None),
        (InCollected, Some(false), Some(true), None),
        (NotInCollected, Some(false), Some(false), None),
        (OtherSeries { franchise: true }, Some(false), None, None),
        (OtherSeries { franchise: false }, Some(false), None, None),
        (CoverStories, Some(false), None, None),
        (Omnibus, None, None, None),
        (Unlabelled, None, None, None),
        (
            OtherSeries { franchise: false },
            Some(false),
            Some(false),
            None,
        ),
        (
            OtherSeries { franchise: false },
            Some(false),
            Some(false),
            None,
        ),
        (CoverStories, Some(false), Some(false), None),
        (
            OtherSeries { franchise: false },
            Some(false),
            Some(false),
            None,
        ),
        (Unlabelled, None, None, None),
        (Unlabelled, None, None, None),
        (Unlabelled, None, None, None),
    ];
    let pairs = one_pair_per_label();
    assert_eq!(pairs.len(), want.len());
    for ((a, b, expected), row) in pairs.iter().zip(want) {
        assert_eq!(*expected, row.0, "the table and the pairs are in one order");
        let got = label(&m, a, b);
        let classes = [Rule::Volume, Rule::Chapter, Rule::Pack].map(|r| class(r, got, a, b, &f));
        assert_eq!(
            (got, classes),
            (row.0, [row.1, row.2, row.3]),
            "{} / {}",
            a.path,
            b.path
        );
    }
}

fn adjudication(a: &str, b: &str, same: bool) -> Adjudication {
    Adjudication {
        a: a.to_owned(),
        b: b.to_owned(),
        same,
        date: "test".to_owned(),
        by: "test".to_owned(),
        note: String::new(),
    }
}

#[test]
fn an_adjudication_overrides_the_name_label() {
    let v1 = volume("en/v1", "s", 1);
    let v3 = volume("raw/v3", "s", 3);
    let c = chapter("ch/c1", "s", Some(2));
    let loose = chapter("ch/c9", "s", None);
    let none = Manifest::default();
    assert_eq!(label(&none, &v1, &v3), Label::SameSeriesOther);
    assert_eq!(label(&none, &c, &v1), Label::ChapterNotIn);
    assert_eq!(label(&none, &loose, &v1), Label::Unlabelled);

    let m = Manifest {
        members: Vec::new(),
        adjudications: vec![
            adjudication("raw/v3", "en/v1", true),
            adjudication("ch/c1", "en/v1", true),
            adjudication("en/v1", "ch/c9", false),
        ],
    };
    assert_eq!(label(&m, &v1, &v3), Label::SameVolume { copy: false });
    assert_eq!(label(&m, &c, &v1), Label::ChapterIn);
    assert_eq!(label(&m, &loose, &v1), Label::ChapterNotIn);
    assert_eq!(
        label(&m, &v1, &c),
        Label::ChapterIn,
        "the volume named first"
    );
    assert_eq!(
        label(&m, &v1, &loose),
        Label::ChapterNotIn,
        "the volume named first"
    );
}

/// A number at either end of a range is inside it and one past an end is not;
/// packs that share a single volume overlap.
#[test]
fn labels_hold_at_both_ends_of_a_range() {
    let m = Manifest::default();
    let p13 = || pack("raw/p1-3", "s", 1, 3);
    let cases = [
        (
            chapter("ch/c3", "s", Some(3)),
            volume("en/v3", "s", 3),
            Label::ChapterIn,
        ),
        (chapter("ch/c1", "s", Some(1)), p13(), Label::ChapterIn),
        (chapter("ch/c3", "s", Some(3)), p13(), Label::ChapterIn),
        (chapter("ch/c4", "s", Some(4)), p13(), Label::ChapterNotIn),
        (volume("en/v1", "s", 1), p13(), Label::InPack),
        (volume("en/v3", "s", 3), p13(), Label::InPack),
        (volume("en/v4", "s", 4), p13(), Label::NotInPack),
        (
            volume("en/v1", "s", 1),
            pack("raw/p2-4", "s", 2, 4),
            Label::NotInPack,
        ),
        (pack("raw/p3-5", "s", 3, 5), p13(), Label::PackOverlap),
    ];
    for (a, b, want) in &cases {
        assert_eq!(label(&m, a, b), *want, "{} / {}", a.path, b.path);
    }
}

#[test]
fn explicit_pairs_hold_both_neighbours_and_no_duplicates() {
    let members = vec![
        volume("en/v1", "s", 1),
        volume("en/v2", "s", 2),
        volume("en/v3", "s", 3),
        volume("raw/v1", "s", 1),
        volume("raw/v2", "s", 2),
        volume("raw/v2b", "s", 2),
        volume("raw/v3", "s", 3),
        pack("raw/p1-2", "s", 1, 2),
    ];
    let at = |path: &str| members.iter().position(|m| m.path == path).unwrap();
    let pair = |x: &str, y: &str| (at(x).min(at(y)), at(x).max(at(y)));
    let m = Manifest {
        members: members.clone(),
        adjudications: Vec::new(),
    };
    let pairs = explicit_pairs(&m);

    for (x, y) in [
        ("en/v2", "raw/v2"),
        ("en/v2", "raw/v1"),
        ("en/v2", "raw/v3"),
        ("raw/v2", "en/v1"),
        ("raw/v2", "en/v3"),
        ("raw/v2", "raw/v2b"),
        ("raw/p1-2", "en/v3"),
    ] {
        assert!(
            pairs.contains(&pair(x, y)),
            "({x}, {y}) missing from {pairs:?}"
        );
    }
    assert!(
        !pairs.contains(&pair("en/v1", "en/v3")),
        "two apart in one edition is not a neighbour"
    );
    assert!(pairs.iter().all(|&(i, j)| i < j), "{pairs:?}");
    assert!(
        pairs.windows(2).all(|w| w[0] < w[1]),
        "sorted and unique: {pairs:?}"
    );
}

/// Rule 3: a pack against the singles inside it and either side of it. Rule 5:
/// a chapter against every edition's vN, every pack holding N, and vN±1. Rule
/// 6: one member of each of the next two series of the split holding vN, and a
/// spin-off against its main series' v01 and vN, a numberless one included.
#[test]
fn explicit_pairs_follow_rules_three_five_and_six() {
    let members = vec![
        volume("en/v1", "s", 1),
        volume("en/v2", "s", 2),
        volume("en/v3", "s", 3),
        volume("en/v4", "s", 4),
        volume("en/v5", "s", 5),
        volume("raw/v2", "s", 2),
        pack("raw/p2-3", "s", 2, 3),
        chapter("ch/c", "s", Some(2)),
        volume("t/v2", "t", 2),
        volume("u/v2", "u", 2),
        volume("w/v2", "w", 2),
        Member {
            volumes: Some((3, 3)),
            ..member("spin/v3", "s-spin", "s", Kind::Volume)
        },
        member("spin/one", "s-spin", "s", Kind::Volume),
    ];
    let at = |path: &str| members.iter().position(|m| m.path == path).unwrap();
    let m = Manifest {
        members: members.clone(),
        adjudications: Vec::new(),
    };
    let pairs = explicit_pairs(&m);
    let has = |x: &str, y: &str| pairs.contains(&(at(x).min(at(y)), at(x).max(at(y))));

    for (x, y) in [
        ("raw/p2-3", "en/v2"),
        ("raw/p2-3", "en/v3"),
        ("raw/p2-3", "raw/v2"),
        ("raw/p2-3", "en/v1"),
        ("raw/p2-3", "en/v4"),
        ("ch/c", "en/v2"),
        ("ch/c", "raw/v2"),
        ("ch/c", "raw/p2-3"),
        ("ch/c", "en/v1"),
        ("ch/c", "en/v3"),
        ("raw/v2", "t/v2"),
        ("raw/v2", "u/v2"),
        ("w/v2", "en/v2"),
        ("spin/v3", "en/v1"),
        ("spin/v3", "en/v3"),
        ("spin/one", "en/v1"),
    ] {
        assert!(has(x, y), "({x}, {y}) missing from {pairs:?}");
    }
    for (x, y) in [("raw/p2-3", "en/v5"), ("ch/c", "en/v4"), ("raw/v2", "w/v2")] {
        assert!(!has(x, y), "({x}, {y}) should not be paired: {pairs:?}");
    }
}

/// A loose chapter: its number from its name, its volume not yet settled.
fn loose(path: &str, series: &str, number: &str) -> Member {
    Member {
        chapter: Some(number.to_owned()),
        ..member(path, series, series, Kind::Chapter)
    }
}

fn printed(path: &str, range: Option<(u32, u32)>) -> Printed {
    Printed {
        path: path.to_owned(),
        first: range.map(|r| r.0),
        last: range.map(|r| r.1),
        issues: None,
    }
}

fn applied(
    members: Vec<Member>,
    volumes: Vec<Printed>,
    renumbered: &[Renumbering],
) -> Result<Manifest, String> {
    let mut m = Manifest {
        members,
        adjudications: Vec::new(),
    };
    apply_truth(&mut m, &Transcription { volumes }, renumbered)?;
    Ok(m)
}

/// A member's `in_volume`, `after_volume` and `truth`.
fn settled<'m>(m: &'m Manifest, path: &str) -> (Option<u32>, Option<u32>, Option<&'m str>) {
    let x = (m.members.iter())
        .find(|x| x.path == path)
        .unwrap_or_else(|| panic!("no member {path}"));
    (x.in_volume, x.after_volume, x.truth.as_deref())
}

/// Each end of each range and one past the last held volume's end; a fraction
/// either side of that end and a zero are extras no range places. Two copies
/// of v2 print one range and the first path names it; a volume may print a
/// single chapter.
#[test]
fn printed_ranges_settle_chapters_at_both_ends_and_after_the_last_volume() {
    let m = applied(
        vec![
            volume("en/v1", "s", 1),
            volume("en/v2", "s", 2),
            volume("raw/v2", "s", 2),
            loose("ch/c1", "s", "1"),
            loose("ch/c4", "s", "4"),
            loose("ch/c5", "s", "5"),
            loose("ch/c8", "s", "8"),
            loose("ch/c9", "s", "9"),
            loose("ch/c8.5", "s", "8.5"),
            loose("ch/c3.5", "s", "3.5"),
            loose("ch/c0", "s", "0"),
            chapter("ch/v1c1", "s", Some(1)),
            volume("o/v1", "o", 1),
            loose("och/c7", "o", "7"),
        ],
        vec![
            printed("raw/v2", Some((5, 8))),
            printed("en/v1", Some((1, 4))),
            printed("en/v2", Some((5, 8))),
            printed("o/v1", Some((7, 7))),
        ],
        &[],
    )
    .expect("a consistent transcription applies");
    let (v1, v2) = (Some("contents:en/v1"), Some("contents:en/v2"));
    for (path, want) in [
        ("ch/c1", (Some(1), None, v1)),
        ("ch/c4", (Some(1), None, v1)),
        ("ch/c5", (Some(2), None, v2)),
        ("ch/c8", (Some(2), None, v2)),
        ("ch/c9", (None, Some(2), v2)),
        ("ch/c8.5", (None, None, None)),
        ("ch/c3.5", (None, None, None)),
        ("ch/c0", (None, None, None)),
        ("ch/v1c1", (Some(1), None, Some("name"))),
        ("och/c7", (Some(1), None, Some("contents:o/v1"))),
    ] {
        assert_eq!(settled(&m, path), want, "{path}");
    }
}

/// Fan chapter N of a renumbered edition is print chapter N + 7: the shift
/// moves each end of each range, a fractional or zero chapter stays an extra,
/// and another edition's chapters keep their own numbers.
#[test]
fn a_renumbered_edition_is_compared_in_print_numbers() {
    let shift = [Renumbering {
        series: "p",
        edition: "fan",
        by: 7,
        tag: "pp",
    }];
    let m = applied(
        vec![
            volume("en/v1", "p", 1),
            volume("en/v2", "p", 2),
            loose("fan/c3", "p", "3"),
            loose("fan/c4", "p", "4"),
            loose("fan/c7", "p", "7"),
            loose("fan/c8", "p", "8"),
            loose("fan/c7.5", "p", "7.5"),
            loose("fan/c0", "p", "0"),
            loose("ch/c4", "p", "4"),
        ],
        vec![
            printed("en/v1", Some((1, 10))),
            printed("en/v2", Some((11, 14))),
        ],
        &shift,
    )
    .expect("a consistent transcription applies");
    let (v1, v2) = (Some("contents:en/v1; pp +7"), Some("contents:en/v2; pp +7"));
    for (path, want) in [
        ("fan/c3", (Some(1), None, v1)),
        ("fan/c4", (Some(2), None, v2)),
        ("fan/c7", (Some(2), None, v2)),
        ("fan/c8", (None, Some(2), v2)),
        ("fan/c7.5", (None, None, None)),
        ("fan/c0", (None, None, None)),
        ("ch/c4", (Some(1), None, Some("contents:en/v1"))),
    ] {
        assert_eq!(settled(&m, path), want, "{path}");
    }
}

/// A volume without a printed range settles nothing, nor does the end of one
/// when the library holds a later volume, in a pack as well; an indicia gives
/// the issues its edition collects.
#[test]
fn a_volume_without_a_range_settles_nothing_and_an_indicia_lists_issues() {
    let m = applied(
        vec![
            volume("en/v1", "s", 1),
            volume("en/v2", "s", 2),
            loose("ch/c3", "s", "3"),
            loose("ch/c6", "s", "6"),
            loose("ch/c99", "s", "99"),
            volume("q/v1", "q", 1),
            pack("q/p2-3", "q", 2, 3),
            loose("qch/c9", "q", "9"),
            collected("co/v1", 1, None),
            issue("is/i1", 1),
        ],
        vec![
            printed("en/v1", Some((1, 4))),
            printed("en/v2", None),
            printed("q/v1", Some((1, 4))),
            Printed {
                issues: Some(vec![1, 2, 3, 4]),
                ..printed("co/v1", None)
            },
        ],
        &[],
    )
    .expect("a consistent transcription applies");
    assert_eq!(
        settled(&m, "qch/c9"),
        (None, None, None),
        "past q's v1, but a pack holds v3"
    );
    assert_eq!(
        settled(&m, "ch/c3"),
        (Some(1), None, Some("contents:en/v1"))
    );
    assert_eq!(
        settled(&m, "ch/c6"),
        (None, None, None),
        "v2 prints no range"
    );
    assert_eq!(
        settled(&m, "ch/c99"),
        (None, None, None),
        "past v1, but v2 is the last volume held"
    );
    let co = m.members.iter().find(|x| x.path == "co/v1").unwrap();
    assert_eq!(
        (co.issues.as_deref(), co.truth.as_deref()),
        (Some(&[1, 2, 3, 4][..]), Some("indicia:co/v1"))
    );
    assert_eq!(settled(&m, "is/i1"), (None, None, None));
}

/// A transcription that names no member or a pack, prints half a range, an
/// inverted one, one chapter in two volumes or two ranges for one volume, or
/// contradicts a name, is refused with nothing applied.
#[test]
fn a_transcription_that_contradicts_itself_or_a_name_is_refused_whole() {
    let members = vec![
        volume("en/v1", "s", 1),
        volume("en/v2", "s", 2),
        volume("raw/v2", "s", 2),
        pack("raw/p1-2", "s", 1, 2),
        loose("ch/c3", "s", "3"),
        Member {
            chapter: Some("6".to_owned()),
            ..chapter("ch/v1c6", "s", Some(1))
        },
    ];
    let v1 = || printed("en/v1", Some((1, 4)));
    let half = Printed {
        last: None,
        ..printed("en/v2", Some((5, 8)))
    };
    let issues = Printed {
        issues: Some(vec![1]),
        ..v1()
    };
    // Each case with the words its refusal must hold, so that one check
    // refusing in place of another fails it.
    for (volumes, why) in [
        (
            vec![v1(), printed("en/v9", Some((5, 8)))],
            "en/v9: transcribed, but not a member",
        ),
        (vec![v1(), half], "en/v2: printed range"),
        (
            vec![printed("en/v1", Some((4, 1)))],
            "en/v1: printed range Some(4)–Some(1)",
        ),
        (
            vec![printed("raw/p1-2", Some((1, 8)))],
            "raw/p1-2: not a single volume",
        ),
        (
            vec![
                v1(),
                printed("en/v2", Some((7, 8))),
                printed("raw/v2", Some((7, 9))),
            ],
            "en/v2 prints 7–8 and raw/v2 prints 7–9",
        ),
        (
            vec![v1(), printed("en/v2", Some((4, 8)))],
            "en/v1 prints 1–4 and en/v2 prints 4–8",
        ),
        (
            vec![
                printed("en/v1", Some((5, 8))),
                printed("en/v2", Some((1, 4))),
            ],
            "en/v1 prints 5–8 and en/v2 prints 1–4",
        ),
        (
            vec![v1(), printed("en/v2", Some((5, 8)))],
            "ch/v1c6: v1 by name, InVolume(2) by en/v2",
        ),
        (vec![issues], "en/v1: issues on a Volume"),
    ] {
        let mut m = Manifest {
            members: members.clone(),
            adjudications: Vec::new(),
        };
        let got = apply_truth(&mut m, &Transcription { volumes }, &[]);
        assert!(
            got.as_ref().is_err_and(|e| e.contains(why)),
            "{why}: {got:?}"
        );
        assert_eq!(settled(&m, "ch/c3"), (None, None, None), "{why}");
    }
}

/// After the last held volume M, a chapter is in none of the series' volumes
/// or packs, every edition and either way round; a volume past M is not held,
/// so says nothing.
#[test]
fn a_chapter_after_the_last_volume_is_in_none_of_them() {
    let after = Member {
        after_volume: Some(3),
        ..loose("ch/c9", "s", "9")
    };
    let m = Manifest::default();
    for (b, want) in [
        (volume("en/v3", "s", 3), Label::ChapterNotIn),
        (volume("raw/v1", "s", 1), Label::ChapterNotIn),
        (pack("raw/p1-3", "s", 1, 3), Label::ChapterNotIn),
        (loose("ch/c8", "s", "8"), Label::ChapterNotIn),
        (volume("en/v4", "s", 4), Label::Unlabelled),
        (pack("raw/p3-4", "s", 3, 4), Label::Unlabelled),
        (
            volume("en/t3", "t", 3),
            Label::OtherSeries { franchise: false },
        ),
    ] {
        let both = (label(&m, &after, &b), label(&m, &b, &after));
        assert_eq!(both, (want, want), "{}", b.path);
    }
}

#[test]
fn a_chapter_after_the_last_volume_pairs_with_each_vm_and_each_pack_holding_m() {
    let members = vec![
        volume("en/v2", "s", 2),
        volume("en/v3", "s", 3),
        volume("raw/v3", "s", 3),
        pack("raw/p2-3", "s", 2, 3),
        pack("raw/p1-2", "s", 1, 2),
        Member {
            after_volume: Some(3),
            ..loose("ch/c9", "s", "9")
        },
    ];
    let at = |path: &str| members.iter().position(|m| m.path == path).unwrap();
    let m = Manifest {
        members: members.clone(),
        adjudications: Vec::new(),
    };
    let pairs = explicit_pairs(&m);
    let has = |y: &str| pairs.contains(&(at(y).min(at("ch/c9")), at(y).max(at("ch/c9"))));
    for (y, want) in [
        ("en/v3", true),
        ("raw/v3", true),
        ("raw/p2-3", true),
        ("en/v2", false),
        ("raw/p1-2", false),
    ] {
        assert_eq!(has(y), want, "ch/c9 with {y}: {pairs:?}");
    }
}

/// The rebuild writes the draft with the transcription applied where no
/// manifest is yet and over one with no adjudication, the same bytes each
/// time. Over one holding an adjudication it refuses, naming the count and
/// the file, and leaves the file as it was: neither input holds a verdict.
#[test]
fn a_manifest_rebuild_refuses_to_drop_an_adjudication() {
    let dir = TempDir::new("match-rebuild");
    let (draft, truth, out) = (
        dir.join("manifest-draft.json"),
        dir.join("transcription.json"),
        dir.join("manifest.json"),
    );
    let drafted = manifest_of(vec![volume("en/v1", "s", 1), loose("ch/c1", "s", "1")]);
    let mut json = serde_json::to_value(&drafted).expect("a manifest serialises");
    json["stamp"] = "# draft_manifest probe".into();
    put(&draft, &json.to_string());
    let transcription = serde_json::json!({
        "stamp": "# truth_sheet probe",
        "owner": { "date": "2026-11-16" },
        "volumes": [{ "path": "en/v1", "first": 1, "last": 1 }],
    });
    put(&truth, &transcription.to_string());

    assert_eq!(
        rebuild_manifest(&draft, &truth, &out),
        Ok(2),
        "no manifest yet"
    );
    let first = read_ev(&dir, "manifest.json");
    assert_eq!(
        rebuild_manifest(&draft, &truth, &out),
        Ok(2),
        "none adjudicated"
    );
    assert_eq!(
        read_ev(&dir, "manifest.json"),
        first,
        "the same inputs, the same bytes"
    );
    let mut held: Manifest = serde_json::from_str(&first).expect("the rebuild parses");
    assert_eq!(
        settled(&held, "ch/c1"),
        (Some(1), None, Some("contents:en/v1"))
    );

    held.adjudications
        .push(adjudication("ch/c1", "en/v1", true));
    let judged = serde_json::to_string_pretty(&held).expect("a manifest serialises");
    put(&out, &judged);
    assert_eq!(
        rebuild_manifest(&draft, &truth, &out),
        Err(format!(
            "{}: holds 1 adjudication, the owner's verdicts, which neither the draft nor the \
             transcription carries, so a rebuild would drop them; nothing is written",
            out.display()
        ))
    );
    assert_eq!(
        read_ev(&dir, "manifest.json"),
        judged,
        "the refused file is untouched"
    );
}

/// The committed truth every later task scores against, checked on every
/// run. The counts it prints are the ones the pre-registration quotes.
#[test]
fn the_manifest_is_consistent() {
    use std::collections::BTreeMap;
    let started = std::time::Instant::now();
    let m: Manifest =
        serde_json::from_str(include_str!("manifest.json")).expect("manifest.json parses");
    let mut paths = BTreeSet::new();
    let (mut members, mut chapters, mut pairs) =
        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    for x in &m.members {
        assert!(paths.insert(x.path.as_str()), "{} is listed twice", x.path);
        let (inside, after) = (x.in_volume.is_some(), x.after_volume.is_some());
        assert!(!(inside && after), "{}: in a volume and after one", x.path);
        assert!(
            !(inside || after) || x.truth.is_some(),
            "{}: no truth",
            x.path
        );
        let dir = x.path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let row = (TABLE.iter())
            .find(|r| r.dir == dir)
            .unwrap_or_else(|| panic!("{}: its directory is no table row", x.path));
        assert_eq!(
            (x.series.as_str(), x.split),
            (row.series, row.split),
            "{}",
            x.path
        );
        *members
            .entry(format!("{:?} {:?}", x.kind, x.split))
            .or_insert(0) += 1;
        if x.kind == Kind::Chapter {
            let how = match (inside, after, x.truth.as_deref()) {
                (true, _, Some("name")) => "in_volume by name",
                (true, ..) => "in_volume by contents",
                (_, true, _) => "after_volume",
                _ => "unlabelled",
            };
            *chapters.entry(format!("{} {how}", x.series)).or_insert(0) += 1;
        }
    }
    for x in &m.members {
        let Some(truth) = x.truth.as_deref().filter(|t| *t != "name") else {
            continue;
        };
        let source = (truth.strip_prefix("contents:"))
            .or_else(|| truth.strip_prefix("indicia:"))
            .map(|rest| rest.split_once("; ").map_or(rest, |(path, _)| path));
        assert!(
            source.is_some_and(|path| paths.contains(path)),
            "{}: its truth {truth} names no member",
            x.path
        );
    }
    let explicit = explicit_pairs(&m);
    for &(i, j) in &explicit {
        let (a, b) = (&m.members[i], &m.members[j]);
        let got = label(&m, a, b);
        assert_eq!(got, label(&m, b, a), "{} / {}", a.path, b.path);
        *pairs
            .entry(format!("{got:?} {:?}", split_of(a, b)))
            .or_insert(0) += 1;
    }
    for (what, counts) in [
        ("members by kind and split", &members),
        ("chapters by series and truth", &chapters),
        ("explicit pairs by label and split", &pairs),
    ] {
        println!("{what}:");
        for (key, n) in counts {
            println!("{n:7}  {key}");
        }
    }
    println!(
        "{} members, {} explicit pairs, checked in {:?}",
        m.members.len(),
        explicit.len(),
        started.elapsed()
    );
}

#[test]
fn scored_pairs_are_deduplicated_across_sources() {
    let got = scored_pairs(&[(1, 2)], &[(2, 1), (1, 2), (3, 1)]);
    assert_eq!(got, vec![((1, 2), Source::Both), ((1, 3), Source::Leader)]);
}

#[test]
fn class_excludes_short_chapters_and_omnibus_pairs() {
    let c = chapter("ch/c1", "s", Some(2));
    let v = volume("en/v2", "s", 2);
    let short = a_side_of(MIN_CHAPTER_SLOTS - 1);
    let long = a_side_of(MIN_CHAPTER_SLOTS);
    assert_eq!(class(Rule::Chapter, Label::ChapterIn, &c, &v, &short), None);
    assert_eq!(
        class(Rule::Chapter, Label::ChapterIn, &c, &v, &long),
        Some(true)
    );
    assert_eq!(
        class(Rule::Chapter, Label::ChapterIn, &v, &c, &long),
        None,
        "A is a volume"
    );
    assert_eq!(class(Rule::Volume, Label::Omnibus, &v, &v, &long), None);
    assert_eq!(class(Rule::Volume, Label::Unlabelled, &v, &v, &long), None);
    let p = pack("raw/p1-3", "s", 1, 3);
    let other = volume("en/t1", "t", 1);
    let other_series = Label::OtherSeries { franchise: false };
    assert_eq!(
        class(Rule::Pack, other_series, &other, &p, &long),
        Some(false)
    );
    assert_eq!(
        class(Rule::Pack, other_series, &other, &v, &long),
        None,
        "B is not a pack"
    );
}

#[test]
fn omnibus_names_are_recognised() {
    for name in [
        "Volumes/Monster - The Perfect Edition/Monster - The Perfect Edition - v01.cbz",
        "Raw/Monster - Kanzenban/Monster - Kanzenban v01",
        "Volumes/Uzumaki (Deluxe Edition 3-in-1)/Uzumaki (Deluxe Edition 3-in-1) (2018).cbz",
    ] {
        assert!(is_omnibus_name(name), "{name}");
    }
    for name in [
        "Volumes/Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)",
        "Volumes/Series 13-in-1",
        "Volumes/Series 3-in-12",
    ] {
        assert!(!is_omnibus_name(name), "{name}");
    }
    for row in TABLE {
        assert_eq!(
            is_omnibus_name(row.dir),
            row.kind == Some(Kind::Omnibus),
            "{}",
            row.dir
        );
    }
}

#[test]
fn the_held_out_series_are_the_plans() {
    let held_out = |series: &str| {
        ["go-toubun", "zelda-tp", "invincible"].contains(&series)
            || series.starts_with("bocchi")
            || series.starts_with("bisque")
    };
    for row in TABLE {
        assert_eq!(
            row.split == Split::HeldOut,
            held_out(row.series),
            "{}",
            row.dir
        );
    }
    let a = volume("en/v1", "s", 1);
    let b = Member {
        split: Split::HeldOut,
        ..volume("raw/v1", "s", 1)
    };
    assert_eq!(
        (split_of(&a, &a), split_of(&a, &b), split_of(&b, &a)),
        (Split::Dev, Split::HeldOut, Split::HeldOut)
    );
}

const ONE: &str = "Volumes/One v01.cbz";
const TWO: &str = "Volumes/Two v01.cbz";
const LONG: &str = "Volumes/Long v01.cbz";

/// A portrait page wider than any width the sheets give one, so the width
/// each sheet uses shows in its bytes.
const PORTRAIT: (u32, u32) = (1100, 1150);

/// A cbz of `count` pages of `size`. Page `i` of seed `s` is page `i + 1`
/// of seed `s - 1`, as in `cbz_pages`.
fn wide_cbz(dir: &Path, name: &str, count: u32, seed: u32, (width, height): (u32, u32)) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(dir.join(name)).expect("create cbz"));
    for i in 0..count {
        let level = (seed + i) * 101;
        let page = image::GrayImage::from_fn(width, height, |x, y| {
            let v = (x / 7 * 37 + y / 5 * 59 + level) % 256;
            image::Luma([u8::try_from(v).expect("v % 256 fits a byte")])
        });
        let mut bytes = Vec::new();
        page.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("a gray page encodes");
        zip.start_file(
            format!("{i:03}.png"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("start entry");
        zip.write_all(&bytes).expect("write page bytes");
    }
    zip.finish().expect("finish cbz");
}

/// A library holding `ONE` (4 pages), `TWO` (2) and `LONG` (12).
fn sheet_library(tag: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new(&format!("sheet-{tag}"));
    let library = dir.join("library");
    let volumes = library.join("Volumes");
    std::fs::create_dir_all(&volumes).expect("create the fixture library");
    wide_cbz(&volumes, "One v01.cbz", 4, 0, PORTRAIT);
    wide_cbz(&volumes, "Two v01.cbz", 2, 100, PORTRAIT);
    wide_cbz(&volumes, "Long v01.cbz", 12, 200, PORTRAIT);
    (dir, library)
}

fn uri(library: &Path, member: &str, index: u32, width: u32) -> String {
    page_uri(&library.join(member), index, width).expect("a fixture page encodes")
}

/// The sheet with every embedded image cut out, lower-cased: base64 can
/// spell any word by chance, and the words that matter are the ones a
/// person reads.
fn prose(html: &str) -> String {
    let mut rest = html;
    let mut kept = String::new();
    while let Some(start) = rest.find("data:image/jpeg;base64,") {
        kept.push_str(&rest[..start]);
        let tail = &rest[start..];
        rest = &tail[tail.find('"').unwrap_or(tail.len())..];
    }
    kept.push_str(rest);
    kept.to_lowercase()
}

#[test]
fn a_truth_sheet_embeds_each_page_once() {
    let (dir, library) = sheet_library("truth");
    let transcription = dir.join("transcription.json");
    std::fs::write(
        &transcription,
        format!(
            r#"{{"volumes": [
                {{"path": "{ONE}", "pages": [2], "first": 1, "last": 6}},
                {{"path": "{TWO}", "pages": [0, 1], "issues": [1, 2, 3]}}
            ]}}"#
        ),
    )
    .expect("write the transcription");
    let out = dir.join("truth");
    truth_render(&library, &out, &transcription).expect("the sheet renders");
    let html = std::fs::read_to_string(out.join("sheet.html")).expect("the sheet was written");

    let named: [(&str, &[u32], &str); 2] = [
        (ONE, &[2], "Chapters 1–6"),
        (TWO, &[0, 1], "Issues #1, #2, #3"),
    ];
    let every: BTreeSet<String> = [(ONE, 0), (ONE, 1), (ONE, 2), (ONE, 3), (TWO, 0), (TWO, 1)]
        .iter()
        .flat_map(|&(m, i)| [uri(&library, m, i, 900), uri(&library, m, i, 130)])
        .collect();
    assert_eq!(
        every.len(),
        12,
        "a control: no two pages or widths encode alike"
    );
    assert_eq!(html.matches("data:image/jpeg").count(), 3);
    let sections: Vec<&str> = html.split("<section").skip(1).collect();
    assert_eq!(sections.len(), named.len());
    for (section, (member, pages, range)) in sections.iter().zip(named) {
        assert!(
            section.contains(member) && section.contains(range),
            "{member}"
        );
        let images = section.matches("data:image/jpeg").count();
        assert_eq!(images, pages.len(), "{member}");
        for &index in pages {
            let page = uri(&library, member, index, 900);
            assert_eq!(section.matches(&page).count(), 1, "{member} page {index}");
        }
    }
    let words = prose(&html);
    for word in ["score", "bar"] {
        assert!(!words.contains(word), "the sheet says {word}");
    }
}

/// The width of each image a sheet embeds, in order.
fn embedded_widths(html: &str) -> Vec<u32> {
    use base64::Engine as _;
    html.split("data:image/jpeg;base64,")
        .skip(1)
        .map(|rest| {
            let encoded = &rest[..rest.find('"').expect("a closing quote")];
            let jpeg = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .expect("base64");
            image::load_from_memory(&jpeg).expect("a jpeg").width()
        })
        .collect()
}

/// A contents spread is two pages across, so at a single page's width each
/// half's small print would be halved.
#[test]
fn a_spread_on_the_truth_sheet_is_twice_a_pages_width() {
    let (dir, library) = sheet_library("spread");
    wide_cbz(
        &library.join("Volumes"),
        "Spread v01.cbz",
        1,
        300,
        (2000, 1400),
    );
    let transcription = dir.join("transcription.json");
    std::fs::write(
        &transcription,
        format!(
            r#"{{"volumes": [
                {{"path": "Volumes/Spread v01.cbz", "pages": [0], "first": 1, "last": 9}},
                {{"path": "{ONE}", "pages": [1], "first": 1, "last": 6}}
            ]}}"#
        ),
    )
    .expect("write the transcription");
    let out = dir.join("truth");
    truth_render(&library, &out, &transcription).expect("the sheet renders");
    let html = std::fs::read_to_string(out.join("sheet.html")).expect("the sheet was written");
    assert_eq!(embedded_widths(&html), vec![1800, 900]);
}

#[test]
fn extraction_writes_each_volumes_first_pages_and_names_a_volume_it_cannot_read() {
    let (dir, library) = sheet_library("extract");
    let out = dir.join("truth");
    let missing = "Volumes/Missing v09.cbz";
    let err = truth_extract(&library, &out, &[ONE, missing, TWO], 3).expect_err("one is missing");
    assert!(err.contains("Missing v09"), "{err}");

    assert_eq!(slug(ONE), "volumes-one-v01-cbz");
    for (member, written) in [(ONE, 3), (TWO, 2)] {
        let dir = out.join(slug(member));
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .expect("the volume's directory")
            .map(|e| {
                e.expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        let want: Vec<String> = (0..written).map(|i| format!("p{i:02}.jpg")).collect();
        assert_eq!(names, want, "{member}");
        for i in 0..written {
            let bytes = std::fs::read(dir.join(format!("p{i:02}.jpg"))).expect("a page");
            assert_eq!(
                bytes,
                page_jpeg(&library.join(member), i, 1000).expect("encodes")
            );
        }
    }
    let first = image::open(out.join(slug(ONE)).join("p00.jpg")).expect("decodes");
    assert_eq!(first.width(), 1000);

    let index = std::fs::read_to_string(out.join("index.tsv")).expect("the index");
    let lines: Vec<&str> = index.lines().collect();
    assert!(lines[0].starts_with("# truth_sheet "), "{index}");
    assert_eq!(lines[2], format!("{}\t3\t{ONE}", slug(ONE)));
    assert!(
        lines[3].starts_with(&format!("{}\terror: ", slug(missing))),
        "{index}"
    );
    assert_eq!(lines[4], format!("{}\t2\t{TWO}", slug(TWO)));
}

/// Two pairs whose stretches sit at either end of `LONG`, so both ends of
/// the context are clipped to the volume.
#[test]
fn a_pairing_sheet_sets_each_a_page_beside_its_b_page_with_the_b_pages_around_it() {
    let (dir, library) = sheet_library("pairing");
    let out = dir.join("adjudicate-1.html");
    let pairs = [
        SheetPair {
            a: ONE.into(),
            b: LONG.into(),
            question: Question::Inside,
            a_pages: vec![0, 1, 2],
            b_pages: vec![1, 2, 3],
            a_count: 4,
            b_count: 12,
        },
        SheetPair {
            a: ONE.into(),
            b: LONG.into(),
            question: Question::Same,
            a_pages: vec![3],
            b_pages: vec![10],
            a_count: 4,
            b_count: 12,
        },
    ];
    pairing_sheet(&library, &out, &pairs).expect("the sheet renders");
    let html = std::fs::read_to_string(&out).expect("the sheet was written");
    let sections: Vec<&str> = html.split("<section").skip(1).collect();
    assert_eq!(sections.len(), 2);

    for (n, section) in sections.iter().enumerate() {
        let open = section
            .find("<fieldset class=\"verdict\">")
            .expect("a verdict fieldset");
        let close = section[open..]
            .find("</fieldset>")
            .expect("the fieldset closes")
            + open;
        let inside = &section[open..close];
        let (legend, words) = match pairs[n].question {
            Question::Inside => (
                "Are all of A's pages inside B?",
                ["inside", "not inside", "can't tell"],
            ),
            Question::Same => (
                "Same book, the same drawings?",
                ["same", "different", "can't tell"],
            ),
        };
        let shown = format!("<fieldset class=\"verdict\"><legend>{legend}</legend><label>");
        assert!(section[open..].starts_with(&shown), "pair {}", n + 1);
        let radios: Vec<&str> = section.split("<label>").skip(1).collect();
        assert_eq!(radios.len(), 3, "pair {}", n + 1);
        for (radio, word) in radios.iter().zip(words) {
            let want = format!(
                "<input type=\"radio\" name=\"pair{}\"> {word}</label>",
                n + 1
            );
            assert!(radio.starts_with(&want), "pair {}: {radio}", n + 1);
            assert!(
                inside.contains(&format!("<label>{want}")),
                "pair {}: {word} outside",
                n + 1
            );
        }
    }

    let arounds: [&[u32]; 2] = [&[0, 4, 5, 6], &[7, 8, 9, 11]];
    for ((section, pair), around) in sections.iter().zip(&pairs).zip(arounds) {
        assert!(section.contains(ONE) && section.contains(LONG));
        let mut last = 0;
        for (&a, &b) in pair.a_pages.iter().zip(&pair.b_pages) {
            let at_a = section
                .find(&uri(&library, ONE, a, 130))
                .expect("the A page");
            let at_b = section
                .find(&uri(&library, LONG, b, 130))
                .expect("its B page");
            assert!(last <= at_a && at_a < at_b, "A {a} then B {b}, in order");
            last = at_b;
        }
        for &b in around {
            let page = uri(&library, LONG, b, 130);
            assert_eq!(
                section.matches(&page).count(),
                1,
                "B {b} around the stretch"
            );
        }
        let shown = pair.a_pages.len() * 2 + around.len();
        assert_eq!(section.matches("data:image/jpeg").count(), shown);
    }
    let words = prose(&html);
    for word in ["score", "bar"] {
        assert!(!words.contains(word), "the sheet says {word}");
    }
}

/// The one line of `report` starting with `prefix`.
fn line_of<'r>(report: &'r str, prefix: &str) -> &'r str {
    let lines: Vec<&str> = report.lines().filter(|l| l.starts_with(prefix)).collect();
    assert_eq!(lines.len(), 1, "one line starting {prefix:?} in\n{report}");
    lines[0]
}

fn scored_row(
    a: &str,
    b: &str,
    label: Label,
    sub: &str,
    class: Option<bool>,
    score: f64,
) -> ScoredRow {
    ScoredRow {
        a: a.to_owned(),
        b: b.to_owned(),
        label,
        sub_class: sub.to_owned(),
        class,
        score,
    }
}

/// The highest negative is found among negatives only, the first of a tie
/// wins, and a positive exactly at the bar clears it.
#[test]
fn a_rules_report_names_the_pair_behind_each_bar() {
    use Label::{Neighbour, OtherSeries, SameVolume, Unlabelled};
    let copy = SameVolume { copy: true };
    let rows = [
        scored_row("s/n1.cbz", "s/n2.cbz", Neighbour, "en", Some(false), 0.20),
        scored_row("s/v01.cbz", "s/raw v01.cbz", copy, "copy", Some(true), 0.90),
        scored_row(
            "s/x v01.cbz",
            "s/x v02.cbz",
            Neighbour,
            "en",
            Some(false),
            0.41,
        ),
        scored_row(
            "s/r v01.cbz",
            "s/e v01.cbz",
            SameVolume { copy: false },
            "raw-en",
            Some(true),
            0.50,
        ),
        scored_row(
            "t/y.cbz",
            "s/y.cbz",
            OtherSeries { franchise: false },
            "en",
            Some(false),
            0.41,
        ),
        scored_row(
            "s/c v02.cbz",
            "s/d v02.cbz",
            copy,
            "copy",
            Some(true),
            0.41 + super::rule::GAP,
        ),
        scored_row("s/z1.cbz", "s/z2.cbz", Neighbour, "en", Some(false), 0.30),
        scored_row("s/u.cbz", "s/w.cbz", Unlabelled, "en", None, 0.45),
    ];
    let text = report(Rule::Volume, &rows);
    assert_eq!(line_of(&text, "## "), "## volume rule, on V");
    assert_eq!(
        line_of(&text, "positives: "),
        "positives: 3, negatives: 4, unlabelled in its gates: 1"
    );
    assert_eq!(
        line_of(&text, "max negative: "),
        "max negative: 0.4100, s/x v01.cbz and s/x v02.cbz (Neighbour)"
    );
    let bar = line_of(&text, "bar: ");
    assert!(bar.starts_with("bar: 0.56 (exactly 0.5"), "{bar}");
    let exact: f64 = (bar
        .trim_start_matches("bar: 0.56 (exactly ")
        .trim_end_matches(')'))
    .parse()
    .unwrap_or_else(|e| panic!("{bar}: {e}"));
    assert_eq!(
        exact.to_bits(),
        (0.41 + super::rule::GAP).to_bits(),
        "{bar}"
    );
    assert_eq!(
        line_of(&text, "positives at or above the bar: "),
        "positives at or above the bar: 2 of 3 (66.7%)"
    );
    assert_eq!(line_of(&text, "  copy: "), "  copy: 2 of 2 (100.0%)");
    assert_eq!(line_of(&text, "  raw-en: "), "  raw-en: 0 of 1 (0.0%)");
    assert_eq!(
        line_of(&text, "labelled negatives at or above 0.50: "),
        "labelled negatives at or above 0.50: 0"
    );
    assert_eq!(
        line_of(&text, "unlabelled at or above the max negative: "),
        "unlabelled at or above the max negative: 1"
    );
    let order: Vec<&str> = (text.lines()).filter(|l| l.starts_with("  ")).collect();
    assert_eq!(
        order,
        ["  copy: 2 of 2 (100.0%)", "  raw-en: 0 of 1 (0.0%)"]
    );
}

/// A cbz whose page `k` is noise drawn from `seeds[k]`: a random grey per
/// cell of the fingerprint's 17 by 16 grid, on a portrait page so no page
/// reads as a spread. Two pages hash alike only
/// when their seeds match, and a chapter shares a volume's pages by sharing
/// its seeds.
fn noise_cbz(dir: &Path, name: &str, seeds: &[u64]) {
    use std::io::Write;
    std::fs::create_dir_all(dir).expect("make the fixture directory");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(dir.join(name)).expect("create cbz"));
    for (k, &seed) in seeds.iter().enumerate() {
        let blocks: Vec<u8> = (0..17 * 16u64)
            .map(|block| {
                let mut z = (seed * 1000 + block).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                (z ^ (z >> 31)).to_be_bytes()[0]
            })
            .collect();
        let page = image::GrayImage::from_fn(17 * 4, 16 * 8, |x, y| {
            let at = usize::try_from(y / 8 * 17 + x / 4).expect("a cell index fits usize");
            image::Luma([blocks[at]])
        });
        let mut bytes = Vec::new();
        page.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("a gray page encodes");
        zip.start_file(
            format!("{k:03}.png"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("start entry");
        zip.write_all(&bytes).expect("write page bytes");
    }
    zip.finish().expect("finish cbz");
}

/// A rule that met no wrong pair claims no bar, and says so in words apart
/// from a bar that would sit above 1.0.
#[test]
fn a_rule_with_no_negatives_claims_no_bar() {
    let outcome = dev_outcome(&[(true, 0.9), (true, 0.5)]);
    assert_eq!(outcome.max_negative, None);
    assert_eq!(outcome.bar, None);
    assert_eq!((outcome.negatives, outcome.positives_clearing), (0, 0));

    let pos = |score| {
        scored_row(
            "s/v01.cbz",
            "s/p.cbz",
            Label::InPack,
            "pack",
            Some(true),
            score,
        )
    };
    let unlabelled = scored_row("s/v02.cbz", "s/q", Label::Unlabelled, "pack", None, 0.0);
    let none = report(Rule::Pack, &[pos(0.9), pos(0.5), unlabelled]);
    assert_eq!(line_of(&none, "## "), "## pack rule, on C");
    assert_eq!(line_of(&none, "max negative: "), "max negative: none");
    assert_eq!(line_of(&none, "bar: "), "bar: no negatives, no bar claimed");
    assert_eq!(
        line_of(&none, "unlabelled at or above the max negative: "),
        "unlabelled at or above the max negative: none, no negative sets the line"
    );

    let neg = scored_row(
        "c/c1.cbz",
        "c/v2.cbz",
        Label::ChapterNotIn,
        "c",
        Some(false),
        0.9,
    );
    let pos = scored_row(
        "c/c1.cbz",
        "c/v1.cbz",
        Label::ChapterIn,
        "c",
        Some(true),
        0.99,
    );
    let unfit = report(Rule::Chapter, &[neg, pos]);
    assert_eq!(line_of(&unfit, "## "), "## chapter rule, on C");
    assert_eq!(
        line_of(&unfit, "bar: "),
        "bar: none fits, 0.9000 + 0.15 is above 1.0"
    );
    for text in [&none, &unfit] {
        assert!(
            !text.lines().any(|l| l.starts_with("positives at or above")),
            "{text}"
        );
    }
}

/// `features` reads A as the shorter side; a caller that forgets to orient
/// is stopped in a debug build rather than scoring the pair the long way.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "A must be the shorter side: 5 slots against 4")]
fn features_refuses_a_longer_a() {
    let _ = features(&hashes(1, 5), &hashes(2, 4));
}

fn slot(page: u32, half: Option<taiyaku_core::Half>) -> taiyaku_core::Slot {
    taiyaku_core::Slot { page, half }
}

/// Slots name pages: a split spread's two halves are two slots of one page,
/// so its pairing with one page shows once, and with two pages twice.
#[test]
fn sheet_pages_name_each_slots_page_once_per_pairing() {
    use taiyaku_core::Half::{Left, Right};
    let a = [
        slot(0, None),
        slot(1, Some(Right)),
        slot(1, Some(Left)),
        slot(2, None),
    ];
    let b = [
        slot(4, None),
        slot(5, None),
        slot(6, None),
        slot(7, Some(Right)),
        slot(7, Some(Left)),
    ];
    let runs = [
        unpaired((0, 0)),
        paired((1, 2), (1, 2)),
        paired((3, 3), (3, 3)),
    ];
    assert_eq!(sheet_pages(&runs, &a, &b), (vec![1, 1, 2], vec![5, 6, 7]));
    let both_halves = [paired((1, 2), (3, 4))];
    assert_eq!(sheet_pages(&both_halves, &a, &b), (vec![1], vec![7]));
    let later = [paired((0, 0), (2, 2)), paired((3, 3), (0, 0))];
    assert_eq!(sheet_pages(&later, &a, &b), (vec![0, 2], vec![6, 4]));
}

fn adjudication_of(a: &str, b: &str, same: bool, note: &str) -> Adjudication {
    Adjudication {
        note: note.to_owned(),
        ..adjudication(a, b, same)
    }
}

/// The next round is one past the highest this split's notes name, whatever
/// order they were recorded in, and another split's rounds do not count.
#[test]
fn the_next_round_follows_the_highest_round_of_its_split() {
    let mut m = manifest_of(Vec::new());
    assert_eq!(next_round(&m, Split::Dev), 1);
    m.adjudications = vec![
        adjudication_of("a", "b", true, "dev round 2"),
        adjudication_of("c", "d", false, "dev round 10: can't tell"),
        adjudication_of("e", "f", false, "dev round 3"),
        adjudication_of("g", "h", true, "held-out round 40"),
        adjudication_of("i", "j", true, "a note naming no round 99"),
    ];
    assert_eq!(next_round(&m, Split::Dev), 11);
    assert_eq!(next_round(&m, Split::HeldOut), 41);
}

/// An unlabelled pair meets the gates `class` sets a labelled one: the
/// chapter rule's slot floor and kinds, the pack rule's volume against a
/// pack, and none for the volume rule.
#[test]
fn an_unlabelled_pair_meets_the_gates_a_labelled_one_does() {
    let ch = chapter("ch/s c1.cbz", "s", None);
    let v = volume("en/s v01.cbz", "s", 1);
    let p = pack("raw/s v01-02", "s", 1, 2);
    let short = a_side_of(MIN_CHAPTER_SLOTS - 1);
    let long = a_side_of(MIN_CHAPTER_SLOTS);
    assert!(!in_gates(Rule::Chapter, &ch, &v, &short));
    assert!(in_gates(Rule::Chapter, &ch, &v, &long));
    assert!(!in_gates(Rule::Chapter, &v, &p, &long));
    assert!(in_gates(Rule::Pack, &v, &p, &long));
    assert!(!in_gates(Rule::Pack, &ch, &p, &long));
    assert!(!in_gates(Rule::Pack, &p, &v, &long));
    for (a, b) in [(&ch, &v), (&v, &p), (&p, &v)] {
        assert!(
            in_gates(Rule::Volume, a, b, &short),
            "{} {}",
            a.path,
            b.path
        );
    }
}

#[test]
fn sub_classes_follow_editions_series_and_packs() {
    let at = |path: &str, edition: &str| Member {
        edition: edition.to_owned(),
        ..volume(path, "s", 1)
    };
    let same = Label::SameVolume { copy: false };
    let copy = Label::SameVolume { copy: true };
    let sub = |label, x: &Member, y: &Member| sub_class(Rule::Volume, label, x, y);
    let (en, raw) = (at("en/s v01.cbz", "en"), at("raw/s v01.cbz", "raw"));
    let colour = at("c/s v01.cbz", "en-colour");
    let fan = at("f/s v01.cbz", "en-fan");
    let raw_colour = at("rc/s v01.cbz", "raw-colour");
    assert_eq!(sub(same, &en, &raw), "raw-en");
    assert_eq!(sub(same, &raw, &en), "raw-en");
    assert_eq!(sub(same, &en, &colour), "colour");
    assert_eq!(sub(same, &raw, &raw_colour), "colour");
    assert_eq!(sub(same, &fan, &colour), "fan");
    assert_eq!(sub(copy, &fan, &fan), "copy");
    assert_eq!(sub(same, &en, &at("en2/s v01.cbz", "en")), "en-en");
    let ch = chapter("ch/k c1.cbz", "kaguya", Some(1));
    assert_eq!(
        sub_class(Rule::Chapter, Label::ChapterIn, &ch, &en),
        "kaguya"
    );
    let comic = issue("i/c 1.cbz", 1);
    assert_eq!(
        sub_class(Rule::Chapter, Label::InCollected, &comic, &en),
        "comics"
    );
    assert_eq!(sub_class(Rule::Pack, Label::InPack, &en, &raw), "pack");
}

/// Features with these scores, for rows that are never aligned.
fn scored_as(pair: (usize, usize), label: Label, slots: u32, scores: (f64, f64)) -> Scored {
    let mut features = a_side_of(slots);
    (features.volume, features.containment) = scores;
    Scored {
        a: pair.0,
        b: pair.1,
        source: Source::Explicit,
        label,
        features,
        runs: 1,
        ms: 0.0,
    }
}

/// An unlabelled pair is shown at its rule's highest labelled negative and
/// not below it, only under a rule whose gates it meets; a labelled negative
/// at 0.5 under the rule it is a negative of; and a judged pair never again.
#[test]
fn the_pairs_shown_are_the_preregistered_ones_and_none_judged() {
    use Label::{ChapterIn, ChapterNotIn, InPack, Neighbour, Unlabelled};
    let m = manifest_of(vec![
        volume("en/s v01.cbz", "s", 1),
        volume("en/s v02.cbz", "s", 2),
        pack("raw/s v01-02", "s", 1, 2),
        chapter("ch/s c1.cbz", "s", Some(1)),
        chapter("ch/s c2.cbz", "s", None),
        chapter("ch/s c3.cbz", "s", None),
    ]);
    let scored = vec![
        scored_as((0, 1), Neighbour, 20, (0.30, 0.9)),
        scored_as((0, 2), InPack, 20, (0.50, 1.0)),
        scored_as((3, 1), ChapterNotIn, 10, (0.10, 0.40)),
        scored_as((3, 0), ChapterIn, 10, (0.10, 1.0)),
        scored_as((4, 1), Unlabelled, 10, (0.10, 0.40)),
        scored_as((4, 0), Unlabelled, 10, (0.10, 0.39)),
        scored_as((5, 1), Unlabelled, 7, (0.05, 1.0)),
        scored_as((5, 2), Unlabelled, 10, (0.50, 0.0)),
    ];
    let lines = dev_lines(&m, &scored);
    let list = shown(&m, &scored, &lines);
    let at: Vec<usize> = list.iter().map(|(n, _)| *n).collect();
    assert_eq!(at, [1, 4, 7]);
    assert_eq!(
        list[0].1,
        ["InPack negative, volume rule 0.5000 at or above 0.50"]
    );
    assert_eq!(
        list[1].1,
        ["unlabelled, chapter rule 0.4000 at or above its max negative 0.4000"]
    );
    assert_eq!(
        list[2].1,
        ["unlabelled, volume rule 0.5000 at or above its max negative 0.5000"]
    );

    let mut judged = m.clone();
    judged.adjudications = vec![adjudication_of(
        "raw/s v01-02",
        "en/s v01.cbz",
        true,
        "dev round 1",
    )];
    let left: Vec<usize> = shown(&judged, &scored, &lines)
        .iter()
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(left, [4, 7]);
}

/// A member whose pages cannot align with themselves fails the first
/// control, and pages that do pass both.
#[test]
fn the_controls_hold_on_real_pages_and_fail_on_blank_ones() {
    let pages = hashes(9, 40);
    let line = controls(&pages[..30], &pages).expect("the controls hold");
    assert!(
        line.starts_with("A against itself: V 1.0000, C 1.0000; "),
        "{line}"
    );
    let blank = vec![PageHash([0; 4]); 30];
    let why = controls(&blank, &pages).expect_err("a blank member cannot align with itself");
    assert!(why.starts_with("A against itself: V 0.0000"), "{why}");
}

/// A member's top three leaders become pairs, the fourth does not, and a
/// leader the manifest does not hold refuses the file.
#[test]
fn leaders_give_each_members_top_three_and_refuse_a_stranger() {
    let m = manifest_of(
        ["a", "b", "c", "d", "e"]
            .map(|path| volume(path, "s", 1))
            .to_vec(),
    );
    let text = r##"{"stamp": "# probe", "members": {
        "b": {"informative": 9, "leaders": [["e", 5], ["a", 4], ["d", 3], ["c", 2]]},
        "d": {"informative": 2, "leaders": [["b", 1]]}}}"##;
    let leaders = read_leaders(text, &m).expect("the leaders read");
    assert_eq!(leaders.stamp, "# probe");
    assert_eq!(leaders.pairs, [(1, 4), (1, 0), (1, 3), (3, 1)]);
    let lookups = &leaders.lookups;
    let b = lookups[1].as_ref().expect("b was looked up");
    assert_eq!(
        (b.informative, b.votes.as_slice()),
        (9, &[(4, 5), (0, 4), (3, 3), (2, 2)][..])
    );
    assert!(lookups[0].is_none() && lookups[2].is_none());
    let stranger = text.replace("[\"c\", 2]", "[\"z\", 2]");
    let why = read_leaders(&stranger, &m).expect_err("z is not a member");
    assert!(why.contains("leaders.json names z"), "{why}");
}

/// A dev series of two volumes in two editions, so two `SameVolume` pairs, the
/// first in manifest order taking the controls, a third volume, a raw pack
/// of the first two as a directory of archives, three chapters and an
/// omnibus, beside a held-out volume whose pages are the first volume's.
/// `v02` opens with `v01`'s last six pages, so wrong pairs score above 0.
/// `raw/S v03.cbz` is a RAR named `.cbz`, so its pairs go unscored and
/// `en/S v03.cbz`, a query whose one positive it is, has none printed.
fn score_fixture(tag: &str) -> (TempDir, PathBuf, PathBuf, Manifest) {
    let root = TempDir::new(tag);
    let (library, ev) = (root.join("library"), root.join("ev"));
    let v1: Vec<u64> = (0..30).collect();
    let v2: Vec<u64> = [(24..30).collect::<Vec<u64>>(), (100..124).collect()].concat();
    std::fs::create_dir_all(library.join("raw")).expect("make the raw directory");
    std::fs::write(library.join("raw/S v03.cbz"), b"Rar!\x1a\x07\x00").expect("write raw v03");
    for (dir, name, seeds) in [
        ("en", "S v01.cbz", v1.clone()),
        ("raw", "S v01.cbz", v1.clone()),
        ("en", "S v02.cbz", v2.clone()),
        ("en", "S v03.cbz", (200..230).collect()),
        ("raw/S v01-02", "S v01.cbz", v1.clone()),
        ("raw/S v01-02", "S v02.cbz", v2.clone()),
        ("ch", "S c001.cbz", v1[2..12].to_vec()),
        ("ch", "S c010.cbz", v2[..10].to_vec()),
        ("ch", "S c020.cbz", v2[10..20].to_vec()),
        (
            "en",
            "S Omnibus v01-02.cbz",
            [v1.clone(), v2.clone()].concat(),
        ),
        ("en", "H v01.cbz", v1),
        ("raw", "S v02.cbz", v2),
    ] {
        noise_cbz(&library.join(dir), name, &seeds);
    }
    let m = manifest_of(vec![
        volume("en/S v01.cbz", "s", 1),
        volume("raw/S v01.cbz", "s", 1),
        volume("en/S v02.cbz", "s", 2),
        volume("en/S v03.cbz", "s", 3),
        pack("raw/S v01-02", "s", 1, 2),
        chapter("ch/S c001.cbz", "s", Some(1)),
        chapter("ch/S c010.cbz", "s", Some(2)),
        chapter("ch/S c020.cbz", "s", None),
        Member {
            volumes: Some((1, 2)),
            ..member("en/S Omnibus v01-02.cbz", "s", "s", Kind::Omnibus)
        },
        Member {
            split: Split::HeldOut,
            ..volume("en/H v01.cbz", "h", 1)
        },
        volume("raw/S v02.cbz", "s", 2),
        volume("raw/S v03.cbz", "s", 3),
    ]);
    let leaders = r##"{"stamp": "# fixture", "members": {
        "en/S v01.cbz": {"informative": 30, "leaders": [["raw/S v01.cbz", 30], ["en/H v01.cbz", 30], ["raw/S v01-02", 30]]},
        "raw/S v01.cbz": {"informative": 30, "leaders": [["en/S v01.cbz", 30], ["raw/S v01-02", 30], ["ch/S c001.cbz", 10]]},
        "en/S v02.cbz": {"informative": 30, "leaders": [["raw/S v01-02", 30], ["ch/S c010.cbz", 10], ["ch/S c020.cbz", 10]]},
        "ch/S c010.cbz": {"informative": 10, "leaders": [["en/S v02.cbz", 10], ["en/S v01.cbz", 6], ["raw/S v01.cbz", 6]]},
        "ch/S c020.cbz": {"informative": 10, "leaders": [["en/S v02.cbz", 10], ["raw/S v01-02", 10], ["en/S Omnibus v01-02.cbz", 10]]},
        "en/S v03.cbz": {"informative": 30, "leaders": [["en/S v02.cbz", 1]]},
        "en/H v01.cbz": {"informative": 30, "leaders": [["en/S v01.cbz", 30]]}}}"##;
    put(&ev.join("leaders.json"), leaders);
    (root, library, ev, m)
}

/// `score_dev` over a fixture: the files it writes, each opening with its
/// stamp.
fn score_dev_once(tag: &str) -> (TempDir, PathBuf, PathBuf, Manifest, String) {
    let (root, library, ev, m) = score_fixture(tag);
    let line = super::score::dev(&library, &ev, &m).unwrap_or_else(|e| panic!("{e}"));
    (root, library, ev, m, line)
}

fn read_ev(ev: &Path, name: &str) -> String {
    std::fs::read_to_string(ev.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// `scores.tsv` holds each dev pair once, its A the shorter side, no
/// held-out pair, and a row's cells in the header's order.
#[test]
fn score_dev_writes_each_dev_pair_once_oriented() {
    let (_root, _library, ev, _m, _line) = score_dev_once("match-score-tsv");
    let tsv = read_ev(&ev, "dev/scores.tsv");
    let rules = read_ev(&ev, "dev/rules.txt");
    let first = tsv.lines().next().unwrap_or_default();
    assert!(first.starts_with("# score_dev "), "{first}");
    assert_eq!(rules.lines().next(), Some(first));
    let header: Vec<&str> = tsv.lines().nth(1).unwrap_or_default().split('\t').collect();
    assert_eq!(header.len(), 20, "{header:?}");
    let rows: Vec<Vec<&str>> = (tsv.lines().skip(2))
        .map(|l| l.split('\t').collect())
        .collect();
    let mut named = BTreeSet::new();
    for row in &rows {
        assert_eq!(row.len(), 20, "{row:?}");
        assert!(
            named.insert((row[0], row[1])) && !named.contains(&(row[1], row[0])),
            "{row:?}"
        );
        assert!(
            !row[0].contains("en/H") && !row[1].contains("en/H"),
            "{row:?}"
        );
        let (a, b): (u32, u32) = (
            row[7].parse().expect("a_slots"),
            row[8].parse().expect("b_slots"),
        );
        assert!(a < b || (a == b && row[0] < row[1]), "{row:?}");
    }
    let pairs = line_of(&rules, "pairs: ");
    let counts = line_of(&rules, "scored: ");
    let unscored: usize = (counts.rsplit(": ").next().unwrap_or_default())
        .parse()
        .unwrap_or_else(|e| panic!("{counts}: {e}"));
    assert_eq!(
        counts,
        format!(
            "scored: {}, not scored as a member did not print: {unscored}",
            rows.len()
        )
    );
    assert!(unscored > 0, "raw v03's pairs go unscored");
    assert!(
        pairs.starts_with(&format!("pairs: {}, ", rows.len() + unscored)),
        "{pairs}"
    );
    assert!(
        !tsv.contains("raw/S v03.cbz"),
        "no row for an unreadable member"
    );
    let row = |a: &str, b: &str| {
        (rows.iter())
            .find(|r| r[0] == a && r[1] == b)
            .unwrap_or_else(|| panic!("no row {a} {b}\n{tsv}"))
    };
    let same = row("en/S v01.cbz", "raw/S v01.cbz");
    assert_eq!(
        &same[2..10],
        [
            "both",
            "SameVolume",
            "positive",
            "-",
            "-",
            "30",
            "30",
            "Sound"
        ]
    );
    assert_eq!(
        (same[13], same[14], same[17], same[18]),
        ("30", "1", "1.000000", "1.000000")
    );
    let later = row("en/S v02.cbz", "raw/S v02.cbz");
    assert_eq!(
        &later[3..5],
        ["SameVolume", "positive"],
        "the second SameVolume pair"
    );
    let unlabelled = row("ch/S c020.cbz", "en/S v02.cbz");
    assert_eq!(
        &unlabelled[2..7],
        ["leader", "Unlabelled", "unlabelled", "unlabelled", "-"]
    );
    assert_eq!(
        row("ch/S c010.cbz", "en/S v01.cbz")[3..7],
        ["ChapterNotIn", "negative", "negative", "-"]
    );
    assert_eq!(
        row("en/S v03.cbz", "raw/S v01-02")[3..7],
        ["NotInPack", "negative", "-", "negative"]
    );
}

/// `rules.txt` holds the controls, each rule's section with its max
/// negative, the omnibus check, the adjudications so far and the lookup's
/// predictions.
#[test]
fn score_dev_reports_the_controls_and_each_rule() {
    let (_root, _library, ev, _m, line) = score_dev_once("match-score-rules");
    let rules = read_ev(&ev, "dev/rules.txt");
    assert_eq!(
        line_of(&rules, "controls on "),
        "controls on en/S v01.cbz and raw/S v01.cbz: A against itself: V 1.0000, C 1.0000; \
         A against B shuffled: V 0.0000: hold"
    );
    for (rule, max) in [
        ("## volume rule, on V", "0.5000"),
        ("## chapter rule, on C", "0.6000"),
        ("## pack rule, on C", "0.0000"),
    ] {
        let section = (rules.split("\n## ").find(|s| s.starts_with(&rule[3..])))
            .unwrap_or_else(|| panic!("no {rule} in\n{rules}"));
        assert_eq!(section.lines().next(), Some(&rule[3..]));
        let max_line = line_of(section, "max negative: ");
        assert!(
            max_line.starts_with(&format!("max negative: {max}, ")),
            "{rule}: {max_line}"
        );
        assert_eq!(
            section.matches("other features, descriptively:").count(),
            1,
            "{rule}"
        );
    }
    assert_eq!(
        line_of(&rules, "is_omnibus_name marks "),
        "is_omnibus_name marks 1 of 1 omnibus members and 0 other members"
    );
    let omnibus: Vec<&str> = (rules.lines())
        .filter(|l| l.contains("\ten/S Omnibus v01-02.cbz\t"))
        .collect();
    for row in [
        "en/S v01.cbz\ten/S Omnibus v01-02.cbz\t0.5000\t0.8333",
        "ch/S c020.cbz\ten/S Omnibus v01-02.cbz\t0.1667\t1.0000",
    ] {
        assert!(omnibus.contains(&row), "{row} in {omnibus:#?}");
    }
    assert_eq!(
        line_of(&rules, "adjudications of dev pairs: "),
        "adjudications of dev pairs: 0, same: 0, relabels of a labelled pair: 0, rounds: 0"
    );
    assert_eq!(line_of(&rules, "round 1: "), line);
    assert_eq!(line_of(&rules, "leaders.json: "), "leaders.json: # fixture");
    let read = line_of(&rules, "members read: ");
    assert!(read.contains(", unreadable: 1, "), "{read}");
    let unreadable = line_of(&rules, "unreadable: ");
    assert!(unreadable.contains("raw/S v03.cbz"), "{unreadable}");
    assert_eq!(
        line_of(&rules, "queries whose positives all failed to print: "),
        "queries whose positives all failed to print: 1"
    );
    assert!(line_of(&rules, "P1, ").ends_with("holds"), "{rules}");
    assert_eq!(rules.lines().filter(|l| l.starts_with("P2")).count(), 2);
}

/// The round lists the pairs the rules show, with their pages, renders as
/// a sheet, and once judged leaves the next round empty, the one verdict
/// that changed a label counted as a relabel.
#[test]
fn score_dev_lists_a_round_and_empties_it_once_judged() {
    let (_root, library, ev, mut m, line) = score_dev_once("match-score-round");
    assert_eq!(line, "round 1: 6 pairs to judge, in adjudicate-1.json");
    let read = |name: &str| read_ev(&ev, name);
    let round: ToJudge =
        serde_json::from_str(&read("dev/adjudicate-1.json")).expect("the round parses");
    let first = read("dev/scores.tsv")
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(round.stamp, first);
    assert_eq!((round.round, round.cap), (1, 40));
    let shown: Vec<(&str, &str)> = round
        .pairs
        .iter()
        .map(|p| (p.a.as_str(), p.b.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("en/S v01.cbz", "raw/S v01-02"),
            ("ch/S c010.cbz", "en/S v01.cbz"),
            ("raw/S v01.cbz", "raw/S v01-02"),
            ("ch/S c010.cbz", "raw/S v01.cbz"),
            ("ch/S c020.cbz", "en/S v02.cbz"),
            ("ch/S c020.cbz", "raw/S v01-02"),
        ]
    );
    for pair in &round.pairs {
        assert_eq!(pair.question, Question::Inside, "{} {}", pair.a, pair.b);
    }
    let in_pack = "InPack negative, volume rule 0.5000 at or above 0.50";
    assert_eq!(round.pairs[0].why, [in_pack]);
    let not_in = "ChapterNotIn negative, chapter rule 0.6000 at or above 0.50";
    assert_eq!(round.pairs[1].why, [not_in]);
    let c020 = &round.pairs[4];
    assert_eq!(c020.a_pages, (0..10).collect::<Vec<u32>>());
    assert_eq!(c020.b_pages, (10..20).collect::<Vec<u32>>());
    let unlabelled = "unlabelled, chapter rule 1.0000 at or above its max negative 0.6000";
    assert_eq!(c020.why, [unlabelled]);
    assert_eq!(round.pairs[5].b_pages, (40..50).collect::<Vec<u32>>());
    assert_eq!((round.pairs[5].a_count, round.pairs[5].b_count), (10, 60));

    let sheet = render(&library, &ev.join("dev")).expect("the round renders");
    assert_eq!(sheet, Some(ev.join("dev/adjudicate-1.html")));
    let html = read("dev/adjudicate-1.html");
    let sections: Vec<&str> = html.split("<section>").skip(1).collect();
    assert_eq!(sections.len(), 6);
    for (section, pair) in sections.iter().zip(&round.pairs) {
        assert!(
            section.contains(&format!("A {}<br>B {}", pair.a, pair.b)),
            "{}",
            pair.a
        );
    }
    let archive = library.join("raw/S v01-02/S v02.cbz");
    let page = page_uri(&archive, 10, 130).expect("the pack's page 40 encodes");
    let shown = format!("<img src=\"{page}\" alt=\"B p40\">");
    assert!(
        sections[5].contains(&shown),
        "the pack's page 40 is its v02's page 10"
    );

    let verdicts = [false, true, true, false, true, false];
    m.adjudications = (round.pairs.iter().zip(verdicts))
        .map(|(p, same)| adjudication_of(&p.a, &p.b, same, "dev round 1"))
        .collect();
    let line = super::score::dev(&library, &ev, &m).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(line, "round 2: 0 pairs to judge, in adjudicate-2.json");
    let rules = read("dev/rules.txt");
    assert_eq!(
        line_of(&rules, "adjudications of dev pairs: "),
        "adjudications of dev pairs: 6, same: 3, relabels of a labelled pair: 1, rounds: 1"
    );
    let tsv = read("dev/scores.tsv");
    for (a, b, want) in [
        ("en/S v01.cbz", "raw/S v01-02", "InPack"),
        ("ch/S c010.cbz", "en/S v01.cbz", "ChapterIn"),
        ("ch/S c010.cbz", "raw/S v01.cbz", "ChapterNotIn"),
        ("ch/S c020.cbz", "en/S v02.cbz", "ChapterIn"),
        ("ch/S c020.cbz", "raw/S v01-02", "ChapterNotIn"),
    ] {
        let row: Vec<&str> = (tsv.lines())
            .map(|l| l.split('\t').collect::<Vec<&str>>())
            .find(|r| r[0] == a && r[1] == b)
            .unwrap_or_else(|| panic!("no row {a} {b}"));
        assert_eq!(row[3], want, "{a} {b}");
    }
    assert_eq!(render(&library, &ev.join("dev")), Ok(None));
}

/// A member of a directory of archives is paged as `cached_edition` joins
/// it: the sheet shows the archive page each joined index names, the
/// context either side included, across the seam between the archives.
#[test]
fn a_pairing_sheet_reads_a_directory_of_archives_in_joined_order() {
    let (dir, library) = sheet_library("archives");
    let packed = library.join("Volumes/Pack v01-02");
    std::fs::create_dir_all(&packed).expect("make the pack directory");
    wide_cbz(&packed, "P v02.cbz", 4, 400, PORTRAIT);
    wide_cbz(&packed, "P v01.cbz", 4, 300, PORTRAIT);
    let out = dir.join("adjudicate-1.html");
    let pair = SheetPair {
        a: ONE.into(),
        b: "Volumes/Pack v01-02".into(),
        question: Question::Inside,
        a_pages: vec![0, 1],
        b_pages: vec![3, 4],
        a_count: 4,
        b_count: 8,
    };
    pairing_sheet(&library, &out, &[pair]).expect("the sheet renders");
    let html = std::fs::read_to_string(&out).expect("the sheet was written");
    let joined = |k: u32| {
        let (archive, local) = if k < 4 {
            ("P v01.cbz", k)
        } else {
            ("P v02.cbz", k - 4)
        };
        uri(
            &library,
            &format!("Volumes/Pack v01-02/{archive}"),
            local,
            130,
        )
    };
    for k in 0..8 {
        let shown = format!("<img src=\"{}\" alt=\"B p{k}\">", joined(k));
        assert_eq!(html.matches(&shown).count(), 1, "joined page {k}");
    }
    assert_eq!(html.matches("data:image/jpeg").count(), 2 * 2 + 6);
}

/// The omnibus check counts a member its name marks but its kind does not,
/// and one its kind marks but its name does not, and names both.
#[test]
fn the_omnibus_check_names_a_member_marked_one_way_only() {
    let omnibus = |path: &str| Member {
        volumes: Some((1, 2)),
        ..member(path, "s", "s", Kind::Omnibus)
    };
    let m = manifest_of(vec![
        omnibus("en/S Omnibus v01.cbz"),
        omnibus("en/S Big v01.cbz"),
        volume("en/S Deluxe v02.cbz", "s", 2),
        volume("en/S v03.cbz", "s", 3),
        Member {
            split: Split::HeldOut,
            ..volume("en/H Deluxe v01.cbz", "h", 1)
        },
    ]);
    let text = super::score::omnibus(&m, Split::Dev, &[]);
    assert_eq!(
        line_of(&text, "is_omnibus_name marks "),
        "is_omnibus_name marks 1 of 2 omnibus members and 1 other members"
    );
    let named: Vec<&str> = text.lines().filter(|l| l.starts_with("  ")).collect();
    assert_eq!(
        named,
        [
            "  by name or by kind, not both: en/S Big v01.cbz",
            "  by name or by kind, not both: en/S Deluxe v02.cbz"
        ]
    );
}

#[test]
fn the_median_time_takes_the_middle_or_the_mean_of_the_two_middles() {
    use super::score::median;
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
    assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
}

/// A positive equal to the highest negative is not above it, and a feature
/// with no negative says so rather than claiming a line.
#[test]
fn a_feature_line_counts_positives_strictly_above_the_highest_negative() {
    use super::rule::feature_line;
    let values = [
        (false, 2.0),
        (true, 5.0),
        (false, 5.0),
        (true, 5.5),
        (true, 4.0),
    ];
    assert_eq!(
        feature_line("avg_run", &values),
        "  avg_run: highest negative 5.0000, positives above it 1 of 3 (33.3%)"
    );
    assert_eq!(
        feature_line("margin", &[(true, 0.4)]),
        "  margin: no negative"
    );
}

/// The pre-registration lets the owner relabel a labelled pair only on
/// "same drawings": a verdict against a pair the names labelled keeps their
/// label, while a pair the names left unlabelled takes either verdict.
#[test]
fn a_labelled_pair_is_relabelled_only_by_a_same_verdict() {
    let v1 = volume("en/v1", "s", 1);
    let v2 = volume("en/v2", "s", 2);
    let p = pack("raw/v01-02", "s", 1, 2);
    let c = chapter("ch/c1", "s", Some(2));
    let loose = chapter("ch/c9", "s", None);
    let m = Manifest {
        members: Vec::new(),
        adjudications: vec![
            adjudication("en/v1", "raw/v01-02", false),
            adjudication("en/v2", "en/v1", false),
            adjudication("ch/c1", "en/v1", true),
            adjudication("en/v1", "ch/c9", false),
        ],
    };
    assert_eq!(label(&m, &v1, &p), Label::InPack, "not inside keeps InPack");
    assert_eq!(label(&m, &p, &v1), Label::InPack, "the pack named first");
    assert_eq!(
        label(&m, &v1, &v2),
        Label::Neighbour,
        "different keeps Neighbour"
    );
    assert_eq!(
        label(&m, &c, &v1),
        Label::ChapterIn,
        "inside relabels ChapterNotIn"
    );
    assert_eq!(
        label(&m, &loose, &v1),
        Label::ChapterNotIn,
        "unlabelled takes the no"
    );
}

/// A pair where one member can hold the other is asked about with the held
/// member as A, its pages with it, whichever way it was scored; any other
/// pair is asked whether it is one book, as scored. Member `k`'s fixture
/// page is `100 + k`, so a page left on the wrong side shows.
#[test]
fn a_pair_is_asked_whether_the_held_member_is_inside() {
    let a_page = |k: usize| 100 + u32::try_from(k).expect("a small index");
    let m = manifest_of(vec![
        volume("en/v1", "s", 1),
        pack("raw/v01-02", "s", 1, 2),
        chapter("ch/c1", "s", None),
        volume("en/v2", "s", 2),
        issue("c/i1", 1),
        collected("c/v1", 1, Some(vec![1])),
    ]);
    let ask = |a: usize, b: usize| {
        let asked = framed(&m, a, b, (vec![a_page(a)], vec![a_page(b)]));
        (
            asked.a,
            asked.b,
            asked.question,
            asked.a_pages,
            asked.b_pages,
        )
    };
    let inside = Question::Inside;
    assert_eq!(ask(0, 1), (0, 1, inside, vec![100], vec![101]));
    assert_eq!(ask(1, 0), (0, 1, inside, vec![100], vec![101]));
    assert_eq!(ask(3, 2), (2, 3, inside, vec![102], vec![103]));
    assert_eq!(ask(5, 4), (4, 5, inside, vec![104], vec![105]));
    assert_eq!(ask(0, 3), (0, 3, Question::Same, vec![100], vec![103]));
    assert_eq!(ask(3, 0), (3, 0, Question::Same, vec![103], vec![100]));
}

/// The sheet shows a member's pages by its own listing, the alignment by
/// the pages it read: where the two counts differ a page index may name
/// another page, so the sheet refuses, naming the member and both counts.
#[test]
fn a_pairing_sheet_refuses_a_member_whose_pages_were_counted_otherwise() {
    let (dir, library) = sheet_library("parity");
    let out = dir.join("adjudicate-1.html");
    let pair = |a_count, b_count| SheetPair {
        a: ONE.into(),
        b: LONG.into(),
        question: Question::Same,
        a_pages: vec![0],
        b_pages: vec![1],
        a_count,
        b_count,
    };
    let why = pairing_sheet(&library, &out, &[pair(5, 12)]).expect_err("A read 5, lists 4");
    assert_eq!(
        why,
        format!(
            "{ONE}: the alignment read 5 pages and the sheet lists 4, so an index may name another page"
        )
    );
    let why = pairing_sheet(&library, &out, &[pair(4, 11)]).expect_err("B read 11, lists 12");
    assert_eq!(
        why,
        format!(
            "{LONG}: the alignment read 11 pages and the sheet lists 12, so an index may name another page"
        )
    );
    assert!(!out.exists(), "a refused sheet is not written");
    pairing_sheet(&library, &out, &[pair(4, 12)]).expect("the counts agree");
}

#[test]
fn a_round_of_forty_is_plain_and_one_of_forty_one_is_flagged() {
    assert_eq!(
        round_line(3, 40),
        "round 3: 40 pairs to judge, in adjudicate-3.json"
    );
    assert_eq!(
        round_line(3, 41),
        "round 3: 41 pairs to judge, in adjudicate-3.json, over the 40 a round holds: the \
         controller splits it"
    );
}

/// A round holds at most 40 pairs: `render` sets out 40 and refuses 41,
/// naming the count, before it reads a page.
#[test]
fn render_sets_out_forty_pairs_and_refuses_forty_one() {
    let root = TempDir::new("match-render-cap");
    let (library, dir) = (root.join("library"), root.join("dev"));
    noise_cbz(&library.join("v"), "A.cbz", &[1, 2, 3, 4]);
    noise_cbz(&library.join("v"), "B.cbz", &[1, 2, 3, 4]);
    let pair = Shown {
        a: "v/A.cbz".into(),
        b: "v/B.cbz".into(),
        question: Question::Same,
        label: "Unlabelled".into(),
        why: Vec::new(),
        a_pages: vec![0],
        b_pages: vec![0],
        a_count: 4,
        b_count: 4,
    };
    let write = |n: usize| {
        let listed = ToJudge {
            stamp: "# probe".into(),
            round: 1,
            cap: 40,
            pairs: vec![pair.clone(); n],
        };
        let json = serde_json::to_string(&listed).expect("a round serialises");
        put(&dir.join("adjudicate-1.json"), &json);
    };
    write(40);
    let sheet = render(&library, &dir).expect("40 pairs render");
    assert_eq!(sheet, Some(dir.join("adjudicate-1.html")));
    let html = read_ev(&dir, "adjudicate-1.html");
    assert_eq!(html.matches("<section>").count(), 40);
    std::fs::remove_file(dir.join("adjudicate-1.html")).expect("remove the sheet");
    write(41);
    let why = render(&library, &dir).expect_err("41 pairs are refused");
    assert_eq!(
        why,
        format!(
            "{}: 41 pairs, over the 40 a round holds; split it into rounds of at most 40",
            dir.join("adjudicate-1.json").display()
        )
    );
    assert!(!dir.join("adjudicate-1.html").exists(), "no sheet for 41");
}

/// The controls hold at V and C of exactly 0.99 on A against itself and V
/// of exactly 0.10 against B shuffled, and fail just past either bound, on
/// V or C alone.
#[test]
fn the_controls_hold_at_their_bounds_and_fail_just_past_them() {
    let with = |v: f64, c: f64| {
        let mut f = a_side_of(10);
        (f.volume, f.containment) = (v, c);
        f
    };
    let verdict = |own: (f64, f64), shuffled: f64| {
        control_verdict(&with(own.0, own.1), &with(shuffled, 0.0)).is_ok()
    };
    assert!(verdict((0.99, 0.99), 0.10), "at both bounds");
    assert!(!verdict((0.95, 1.0), 0.0), "V below 0.99");
    assert!(!verdict((1.0, 0.95), 0.0), "C below 0.99");
    assert!(!verdict((0.9899, 1.0), 0.0), "V just below 0.99");
    assert!(!verdict((1.0, 1.0), 0.30), "shuffled V above 0.10");
    assert!(!verdict((1.0, 1.0), 0.1001), "shuffled V just above 0.10");
}

/// A verdict naming a path the manifest does not hold, say a mistyped one,
/// stops the run and is named, rather than dropping out of the counts and
/// leaving its pair to come back next round.
#[test]
fn score_dev_refuses_a_verdict_on_a_path_the_manifest_does_not_hold() {
    let (_root, library, ev, mut m) = score_fixture("match-score-stranger");
    m.adjudications = vec![
        adjudication_of("en/S v01.cbz", "raw/S v01-02", true, "dev round 1"),
        adjudication_of("en/S v99.cbz", "raw/S v01-02", false, "dev round 1"),
        adjudication_of("raw/S v01.cbz", "ch/S c999.cbz", false, "dev round 1"),
    ];
    let why = super::score::dev(&library, &ev, &m).expect_err("two verdicts name strangers");
    assert_eq!(
        why,
        "adjudications name paths the manifest does not hold: en/S v99.cbz, ch/S c999.cbz"
    );
    assert!(!ev.join("dev/scores.tsv").exists(), "nothing is scored");
}

/// The line an unlabelled pair is shown at is the caller's: on dev each
/// rule's highest labelled negative, and on held-out the frozen bar, which
/// can sit above or below it. A rule with no line shows no unlabelled pair.
#[test]
fn the_show_line_is_the_callers() {
    use Label::{ChapterIn, ChapterNotIn, InPack, Neighbour, Unlabelled};
    let m = manifest_of(vec![
        volume("en/s v01.cbz", "s", 1),
        volume("en/s v02.cbz", "s", 2),
        pack("raw/s v01-02", "s", 1, 2),
        chapter("ch/s c1.cbz", "s", Some(1)),
        chapter("ch/s c2.cbz", "s", None),
        chapter("ch/s c3.cbz", "s", None),
    ]);
    let scored = vec![
        scored_as((0, 1), Neighbour, 20, (0.30, 0.9)),
        scored_as((0, 2), InPack, 20, (0.50, 1.0)),
        scored_as((3, 1), ChapterNotIn, 10, (0.10, 0.40)),
        scored_as((3, 0), ChapterIn, 10, (0.10, 1.0)),
        scored_as((4, 1), Unlabelled, 10, (0.10, 0.40)),
        scored_as((4, 0), Unlabelled, 10, (0.10, 0.39)),
        scored_as((5, 1), Unlabelled, 7, (0.05, 1.0)),
        scored_as((5, 2), Unlabelled, 10, (0.50, 0.0)),
    ];
    let lines = dev_lines(&m, &scored);
    let at: Vec<(Rule, Option<f64>)> = lines.iter().map(|l| (l.rule, l.at)).collect();
    assert_eq!(
        at,
        [
            (Rule::Volume, Some(0.50)),
            (Rule::Chapter, Some(0.40)),
            (Rule::Pack, None)
        ]
    );
    let frozen = [
        ShowLine {
            rule: Rule::Volume,
            at: Some(0.60),
            what: "its frozen bar",
        },
        ShowLine {
            rule: Rule::Chapter,
            at: Some(0.35),
            what: "its frozen bar",
        },
    ];
    let list = shown(&m, &scored, &frozen);
    let at: Vec<usize> = list.iter().map(|(n, _)| *n).collect();
    assert_eq!(at, [1, 4, 5]);
    assert_eq!(
        list[2].1,
        ["unlabelled, chapter rule 0.3900 at or above its frozen bar 0.3500"]
    );
}

/// `bars.json` reads as the bars dev froze, a `null` bar as none fitting; a
/// file missing a bar is refused rather than read as one that never fit.
#[test]
fn the_frozen_bars_are_read_whole_and_verbatim() {
    let bars = read_bars(include_str!("bars.json")).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        bars,
        Bars {
            volume: Some(0.914_705_882_352_941_1),
            chapter: None,
            pack: Some(0.495_655_848_546_315_06),
            chapter_in_volume_exploratory: Some(0.610_227_272_727_272_7),
            manifest_commit: "48577b95".to_owned(),
        }
    );
    let without_pack = r#"{"volume": 0.9, "chapter": null,
        "chapter_in_volume_exploratory": 0.6, "manifest_commit": "x"}"#;
    assert_eq!(
        read_bars(without_pack),
        Err("bars.json lacks pack".to_owned())
    );
    let extra = r#"{"volume": 0.9, "chapter": null, "pack": 0.4,
        "chapter_in_volume_exploratory": 0.6, "manifest_commit": "x", "gap": 0.15}"#;
    let why = read_bars(extra).expect_err("an unknown key is refused");
    assert!(why.contains("unknown field `gap`"), "{why}");
}

/// The bars are frozen only when HEAD commits the very text compiled in,
/// and the log names the commit that froze them, in UTC like a stamp.
#[test]
fn bars_count_as_frozen_only_as_head_commits_them() {
    let text = "{\"volume\": 0.9}\n";
    assert_eq!(frozen(text, Some(text)), Ok(()));
    let edited = frozen(text, Some("{\"volume\": 0.8}\n")).expect_err("an edited bar");
    assert!(
        edited.starts_with("bars.json differs from the one HEAD commits"),
        "{edited}"
    );
    let none = frozen(text, None).expect_err("an uncommitted bar");
    assert_eq!(none, "HEAD commits no bars.json, so no bar is frozen");
    assert_eq!(
        provenance(Some("abc1234 1000000000\n")),
        "bars.json: committed 2001-09-09T01:46:40Z in abc1234"
    );
    for unread in [None, Some(""), Some("abc1234"), Some("abc1234 soon")] {
        assert_eq!(
            provenance(unread),
            "bars.json: its commit could not be read",
            "{unread:?}"
        );
    }
}

/// A held-out row: `names` is the label the names alone give, `label` the
/// one the pair has now, as a verdict may have changed it.
fn held(
    names: (Label, Option<bool>),
    now: (Label, Option<bool>),
    score: f64,
    sub: &str,
) -> HeldRow {
    HeldRow {
        row: scored_row("x/a", "x/b", now.0, sub, now.1, score),
        names: names.0,
        names_class: names.1,
    }
}

/// Against a bar of 0.56, a pair at 0.56 is at the bar and one at 0.5599 is
/// below it: a labelled negative at it fails, an unlabelled pair at it with
/// no verdict counts as different, and a negative a verdict moved out of the
/// negatives, positive or out of the rule, is a relabel the result rests on.
#[test]
fn a_held_out_negative_at_the_frozen_bar_fails_and_one_just_below_does_not() {
    use Label::{ChapterIn, ChapterNotIn, Neighbour, NotInPack, SameVolume, Unlabelled};
    let (pos, neg, out) = (Some(true), Some(false), None);
    let rows = [
        held((Neighbour, neg), (Neighbour, neg), 0.56, "s"),
        held((Neighbour, neg), (Neighbour, neg), 0.5599, "s"),
        held((Unlabelled, out), (Unlabelled, out), 0.56, "s"),
        held((Unlabelled, out), (Unlabelled, out), 0.5599, "s"),
        held((ChapterIn, pos), (ChapterIn, pos), 0.56, "s"),
        held((ChapterIn, pos), (ChapterIn, pos), 0.5599, "s"),
        held((ChapterNotIn, neg), (ChapterIn, pos), 0.56, "s"),
        held((ChapterNotIn, neg), (ChapterIn, pos), 0.5599, "s"),
        held((ChapterNotIn, neg), (ChapterNotIn, neg), 0.56, "s"),
        held(
            (NotInPack, neg),
            (SameVolume { copy: false }, out),
            0.56,
            "s",
        ),
    ];
    let counts = Judgement {
        positives: 4,
        negatives: 3,
        unlabelled: 2,
        ..Judgement::default()
    };
    assert_eq!(
        judge(&rows, Some(0.56)),
        Judgement {
            bar: Some(0.56),
            failures: vec![0, 8],
            unjudged: vec![2],
            relabels: vec![6, 9],
            clearing: 2,
            ..counts.clone()
        }
    );
    assert_eq!(
        judge(&rows, None),
        Judgement {
            bar: None,
            ..counts
        }
    );
}

/// Any failure fails, an unlabelled pair with no verdict included; with no
/// negative the rule is untested; a pass resting on relabels says how many;
/// and a rule with no bar is not tested whatever its rows hold.
#[test]
fn a_held_out_verdict_is_a_pass_only_with_a_negative_and_no_failure() {
    let base = Judgement {
        bar: Some(0.5),
        negatives: 3,
        ..Judgement::default()
    };
    let with = |change: &dyn Fn(&mut Judgement)| {
        let mut j = base.clone();
        change(&mut j);
        verdict(&j)
    };
    assert_eq!(with(&|_| {}), "passes");
    assert_eq!(with(&|j| j.failures = vec![1]), "fails");
    assert_eq!(with(&|j| j.unjudged = vec![0]), "fails");
    assert_eq!(with(&|j| (j.negatives, j.unjudged) = (0, vec![0])), "fails");
    assert_eq!(
        with(&|j| (j.failures, j.relabels) = (vec![1], vec![2])),
        "fails"
    );
    assert_eq!(
        with(&|j| j.relabels = vec![1, 2]),
        "passes after 2 relabels"
    );
    assert_eq!(
        with(&|j| j.negatives = 0),
        "untested on held-out, no held-out negative"
    );
    assert_eq!(
        with(&|j| (j.negatives, j.relabels) = (0, vec![1])),
        "untested on held-out, no held-out negative"
    );
    assert_eq!(
        with(&|j| (j.bar, j.failures) = (None, vec![1])),
        "no bar, not tested"
    );
}

/// A section names the frozen bar exactly, the first of two equal highest
/// negatives, each failure, each pair awaiting a verdict and each relabel,
/// and the positives clearing by sub-class; with no bar, the positives
/// strictly above the highest negative instead.
#[test]
fn a_held_out_section_names_each_failure_and_relabel() {
    use Label::{ChapterIn, ChapterNotIn, Neighbour, OtherSeries, SameVolume, Unlabelled};
    let row = |a: &str, label: Label, class: Option<bool>, score: f64, sub: &str| HeldRow {
        row: scored_row(
            &format!("{a}/a"),
            &format!("{a}/b"),
            label,
            sub,
            class,
            score,
        ),
        names: label,
        names_class: class,
    };
    let same = SameVolume { copy: false };
    let other = OtherSeries { franchise: false };
    let rows = [
        row("p", same, Some(true), 0.56, "fan"),
        row("q", same, Some(true), 0.5599, "raw-en"),
        row("n", Neighbour, Some(false), 0.56, "s"),
        row("o", other, Some(false), 0.56, "s"),
        row("u", Unlabelled, None, 0.56, "s"),
        HeldRow {
            names: ChapterNotIn,
            names_class: Some(false),
            ..row("r", ChapterIn, Some(true), 0.9, "raw-en")
        },
        row("m", Neighbour, Some(false), 0.5599, "s"),
    ];
    let text = section("volume rule, on V", &rows, Some(0.56));
    assert_eq!(
        text.lines().collect::<Vec<&str>>(),
        [
            "## volume rule, on V",
            "frozen bar: 0.56 (exactly 0.56)",
            "positives: 3, negatives: 3, unlabelled in its gates: 1",
            "highest negative: 0.5600, n/a and n/b (Neighbour)",
            "failures: 3, labelled negatives at or above the bar: 2, unlabelled at or above it \
             with no verdict, each counted as different: 1",
            "  failure: 0.5600, n/a and n/b (Neighbour)",
            "  failure: 0.5600, o/a and o/b (OtherSeries)",
            "  no verdict, counted as different: 0.5600, u/a and u/b",
            "relabels the result rests on: 1",
            "  relabelled: 0.9000, r/a and r/b (ChapterNotIn by its names, now ChapterIn)",
            "positives at or above the bar: 2 of 3 (66.7%)",
            "  fan: 1 of 1 (100.0%)",
            "  raw-en: 1 of 2 (50.0%)",
            "verdict on held-out: fails",
        ]
    );
    let text = section("chapter rule, on C", &rows, None);
    assert_eq!(
        text.lines().collect::<Vec<&str>>(),
        [
            "## chapter rule, on C",
            "frozen bar: none, as none fit on dev",
            "positives: 3, negatives: 3, unlabelled in its gates: 1",
            "highest negative: 0.5600, n/a and n/b (Neighbour)",
            "failures: no bar",
            "positives at or above the bar: no bar",
            "positives above the highest negative: 1 of 3 (33.3%)",
            "verdict on held-out: no bar, not tested",
        ]
    );
}

/// The exploratory line reads a manga chapter, as A, against a volume, an
/// omnibus or a pack: never an issue, and never a chapter against a chapter,
/// a chapter pack, a collected edition or Cover Stories.
#[test]
fn the_chapter_in_volume_line_reads_a_manga_chapter_against_a_book() {
    let ch = chapter("ch/s c1.cbz", "s", Some(1));
    let of = |kind: Kind| member("x/s", "s", "s", kind);
    for (b, want) in [
        (Kind::Volume, true),
        (Kind::Omnibus, true),
        (Kind::Pack, true),
        (Kind::Chapter, false),
        (Kind::ChapterPack, false),
        (Kind::Collected, false),
        (Kind::Extra, false),
        (Kind::Issue, false),
    ] {
        assert_eq!(chapter_in_volume(&ch, &of(b)), want, "{b:?}");
    }
    for b in [Kind::Volume, Kind::Pack, Kind::Collected] {
        assert!(
            !chapter_in_volume(&of(Kind::Issue), &of(b)),
            "an issue against {b:?}"
        );
    }
    assert!(
        !chapter_in_volume(&of(Kind::Volume), &ch),
        "A is the chapter"
    );
}

/// The predictions before a split's rounds drop that split's verdicts only,
/// a pair of a dev and a held-out member being held-out.
#[test]
fn the_predictions_before_any_round_drop_only_that_splits_verdicts() {
    let held = |x: Member| Member {
        split: Split::HeldOut,
        ..x
    };
    let mut m = manifest_of(vec![
        volume("en/s v01", "s", 1),
        volume("raw/s v01", "s", 1),
        held(volume("en/h v01", "h", 1)),
        held(volume("raw/h v01", "h", 1)),
    ]);
    m.adjudications = vec![
        adjudication_of("en/s v01", "raw/s v01", true, "dev round 1"),
        adjudication_of("en/h v01", "raw/h v01", true, "held-out round 1"),
        adjudication_of("en/s v01", "raw/h v01", false, "held-out round 1"),
    ];
    let kept = |split: Split| -> Vec<(String, String)> {
        (before_rounds(&m, split).adjudications.iter())
            .map(|j| (j.a.clone(), j.b.clone()))
            .collect()
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    assert_eq!(kept(Split::HeldOut), [pair("en/s v01", "raw/s v01")]);
    assert_eq!(
        kept(Split::Dev),
        [pair("en/h v01", "raw/h v01"), pair("en/s v01", "raw/h v01")]
    );
    assert_eq!(before_rounds(&m, Split::HeldOut).members.len(), 4);
}

fn test_bars(volume: f64, pack: f64, line: f64) -> Bars {
    Bars {
        volume: Some(volume),
        chapter: None,
        pack: Some(pack),
        chapter_in_volume_exploratory: Some(line),
        manifest_commit: "fixture".to_owned(),
    }
}

/// First the pairs a verdict can change a result on: an unlabelled pair at
/// or above a frozen bar or the line, or a labelled negative at 0.50 or more
/// and at or above one. Then the labelled negatives at 0.50 or more below
/// them all. A pack negative at 0.497, over the 0.4957 bar but under 0.50, is
/// not shown and fails as labelled; one at 0.50 leads. Each part keeps the
/// scored order, which puts two of the second part first; no positive, no
/// pair below its line and no judged pair shows.
#[test]
fn the_held_out_list_leads_with_the_pairs_that_can_change_a_result() {
    use Label::{ChapterIn, ChapterNotIn, InPack, Neighbour, NotInPack, Unlabelled};
    let mut m = manifest_of(vec![
        volume("en/s v01.cbz", "s", 1),
        volume("en/s v02.cbz", "s", 2),
        pack("raw/s v01-02", "s", 1, 2),
        chapter("ch/s c1.cbz", "s", Some(1)),
        chapter("ch/s c2.cbz", "s", None),
        chapter("ch/s c3.cbz", "s", None),
        chapter("ch/s c4.cbz", "s", None),
        volume("en/s v03.cbz", "s", 3),
        volume("en/s v04.cbz", "s", 4),
    ]);
    let scored = vec![
        scored_as((0, 1), Neighbour, 20, (0.6, 0.0)),
        scored_as((0, 2), InPack, 20, (0.5, 1.0)),
        scored_as((4, 0), Unlabelled, 10, (0.95, 0.2)),
        scored_as((3, 1), ChapterNotIn, 10, (0.1, 0.7)),
        scored_as((3, 4), ChapterNotIn, 10, (0.1, 0.8)),
        scored_as((7, 2), NotInPack, 20, (0.1, 0.497)),
        scored_as((5, 0), Unlabelled, 10, (0.1, 0.6)),
        scored_as((5, 1), Unlabelled, 10, (0.1, 0.5999)),
        scored_as((3, 0), ChapterIn, 10, (0.1, 1.0)),
        scored_as((6, 0), Unlabelled, 7, (0.1, 1.0)),
        scored_as((4, 1), Unlabelled, 10, (0.1, 0.9)),
        scored_as((8, 2), NotInPack, 20, (0.1, 0.50)),
    ];
    m.adjudications = vec![adjudication_of(
        "en/s v02.cbz",
        "ch/s c2.cbz",
        true,
        "held-out round 1",
    )];
    let bars = test_bars(0.9, 0.4957, 0.6);
    let (list, leading) = held_out_list(&m, &scored, &bars);
    let at: Vec<usize> = list.iter().map(|(n, _)| *n).collect();
    assert_eq!(at, [2, 3, 6, 11, 0, 1, 4]);
    assert_eq!(leading, 4);
    let line = format!("at or above its frozen bar 0.6000 {EXPLORATORY}");
    let negative = format!("ChapterNotIn negative, chapter in volume 0.7000 {line}");
    let unlabelled = format!("unlabelled, chapter in volume 0.6000 {line}");
    let want: [&[&str]; 7] = [
        &["unlabelled, volume rule 0.9500 at or above its frozen bar 0.9000"],
        &[
            "ChapterNotIn negative, chapter rule 0.7000 at or above 0.50",
            negative.as_str(),
        ],
        &[unlabelled.as_str()],
        &[
            "NotInPack negative, pack rule 0.5000 at or above its frozen bar 0.4957",
            "NotInPack negative, pack rule 0.5000 at or above 0.50",
        ],
        &["Neighbour negative, volume rule 0.6000 at or above 0.50"],
        &["InPack negative, volume rule 0.5000 at or above 0.50"],
        &["ChapterNotIn negative, chapter rule 0.8000 at or above 0.50"],
    ];
    for ((n, why), want) in list.iter().zip(want) {
        assert_eq!(why, want, "pair {n}");
    }
    let pack: Vec<(usize, HeldRow)> = held_rows(Against::Rule(Rule::Pack), &m, &scored);
    let rows: Vec<HeldRow> = pack.iter().map(|(_, r)| r.clone()).collect();
    let failing: Vec<usize> = (judge(&rows, bars.pack).failures.iter())
        .map(|&i| pack[i].0)
        .collect();
    assert_eq!(failing, [5, 11]);
    let text = section("pack rule, on C", &rows, bars.pack);
    for want in [
        "  failure: 0.4970, en/s v03.cbz and raw/s v01-02 (NotInPack)",
        "  failure: 0.5000, en/s v04.cbz and raw/s v01-02 (NotInPack)",
    ] {
        assert!(text.lines().any(|l| l == want), "{want} in\n{text}");
    }
}

/// A held-out fixture: series `h` in two editions, a raw pack of its v01 and
/// v02, three chapters and the comics, beside a dev pair it must not score.
/// `c010` is named into v01 but holds v02's first ten pages, and `c020` has
/// no volume, so the chapter-in-volume line meets a labelled negative and
/// two unlabelled pairs at 1.0, and `c010` against v01 sits on the line.
fn held_out_fixture(tag: &str) -> (TempDir, PathBuf, PathBuf, Manifest) {
    let root = TempDir::new(tag);
    let (library, ev) = (root.join("library"), root.join("ev"));
    let v1: Vec<u64> = (500..530).collect();
    let v2: Vec<u64> = [(524..530).collect::<Vec<u64>>(), (600..624).collect()].concat();
    for (dir, name, seeds) in [
        ("en", "S v01.cbz", (0..30).collect::<Vec<u64>>()),
        ("raw", "S v01.cbz", (0..30).collect()),
        ("en", "H v01.cbz", v1.clone()),
        ("raw", "H v01.cbz", v1.clone()),
        ("en", "H v02.cbz", v2.clone()),
        ("en", "H v03.cbz", (700..730).collect()),
        ("raw/H v01-02", "H v01.cbz", v1.clone()),
        ("raw/H v01-02", "H v02.cbz", v2.clone()),
        ("ch", "H c001.cbz", v1[2..12].to_vec()),
        ("ch", "H c010.cbz", v2[..10].to_vec()),
        ("ch", "H c020.cbz", v2[10..20].to_vec()),
        ("is", "C 001.cbz", (800..810).collect()),
        ("is", "C 003.cbz", (900..910).collect()),
        ("col", "C v01.cbz", (800..820).collect()),
    ] {
        noise_cbz(&library.join(dir), name, &seeds);
    }
    let held = |x: Member| Member {
        split: Split::HeldOut,
        ..x
    };
    let m = manifest_of(vec![
        volume("en/S v01.cbz", "s", 1),
        volume("raw/S v01.cbz", "s", 1),
        held(volume("en/H v01.cbz", "h", 1)),
        held(volume("raw/H v01.cbz", "h", 1)),
        held(volume("en/H v02.cbz", "h", 2)),
        held(volume("en/H v03.cbz", "h", 3)),
        held(pack("raw/H v01-02", "h", 1, 2)),
        held(chapter("ch/H c001.cbz", "h", Some(1))),
        held(chapter("ch/H c010.cbz", "h", Some(1))),
        held(chapter("ch/H c020.cbz", "h", None)),
        held(issue("is/C 001.cbz", 1)),
        held(issue("is/C 003.cbz", 3)),
        held(collected("col/C v01.cbz", 1, Some(vec![1, 2]))),
    ]);
    let leaders = r##"{"stamp": "# held-out fixture", "members": {
        "en/H v01.cbz": {"informative": 30, "leaders": [["raw/H v01.cbz", 30], ["raw/H v01-02", 30], ["en/S v01.cbz", 1]]},
        "ch/H c001.cbz": {"informative": 10, "leaders": [["en/H v01.cbz", 10], ["raw/H v01.cbz", 10], ["raw/H v01-02", 10]]},
        "ch/H c010.cbz": {"informative": 10, "leaders": [["en/H v02.cbz", 10], ["raw/H v01-02", 10], ["en/H v01.cbz", 6]]},
        "ch/H c020.cbz": {"informative": 10, "leaders": [["en/H v02.cbz", 10], ["raw/H v01-02", 10], ["en/H v01.cbz", 1]]},
        "is/C 001.cbz": {"informative": 10, "leaders": [["col/C v01.cbz", 10]]},
        "en/S v01.cbz": {"informative": 30, "leaders": [["raw/S v01.cbz", 30]]}}}"##;
    put(&ev.join("leaders.json"), leaders);
    (root, library, ev, m)
}

fn score_held_out_once(library: &Path, ev: &Path, m: &Manifest) -> String {
    let bars = test_bars(0.55, 0.4, 0.6);
    super::held_out::run(library, ev, m, &bars, "bars.json: fixture")
        .unwrap_or_else(|e| panic!("{e}"))
}

/// One section of `rules.txt`, by the start of its heading.
fn section_of<'r>(rules: &'r str, heading: &str) -> &'r str {
    let found: Vec<&str> = (rules.split("\n## ").skip(1))
        .filter(|s| s.starts_with(heading))
        .collect();
    assert_eq!(found.len(), 1, "one section {heading:?} in\n{rules}");
    found[0]
}

/// Held-out scores only held-out pairs, a dev member's included, and reads
/// each rule against the frozen bar it is given, never one from its own
/// negatives. Its log opens with its stamp and the commit that froze the
/// bars, then each rule's verdict.
#[test]
fn score_held_out_judges_against_the_frozen_bars() {
    let (_root, library, ev, m) = held_out_fixture("match-held-out");
    let line = score_held_out_once(&library, &ev, &m);
    assert_eq!(line, "round 1: 6 pairs to judge, in adjudicate-1.json");
    let tsv = read_ev(&ev, "held-out/scores.tsv");
    let rules = read_ev(&ev, "held-out/rules.txt");
    let stamp = tsv.lines().next().unwrap_or_default();
    assert!(stamp.starts_with("# score_held_out "), "{stamp}");
    let rows: Vec<Vec<&str>> = (tsv.lines().skip(2))
        .map(|l| l.split('\t').collect())
        .collect();
    let named = |a: &str, b: &str| rows.iter().any(|r| r[0] == a && r[1] == b);
    assert!(
        !named("en/S v01.cbz", "raw/S v01.cbz"),
        "a dev pair is not held out"
    );
    assert!(
        named("en/H v01.cbz", "en/S v01.cbz"),
        "a held-out pair with a dev member"
    );
    assert_eq!(rows.len(), 22, "{tsv}");

    let log = read_ev(&ev, "logs/score-held-out.log");
    let tag = format!(" {EXPLORATORY}");
    let bars = format!(
        "frozen bars, manifest fixture: volume 0.55, chapter none, pack 0.4, chapter in volume \
         0.6{tag}"
    );
    let exploratory =
        format!("chapter in volume: fails; failures 3; positives at or above the bar 6 of 6{tag}");
    assert_eq!(
        log.lines().collect::<Vec<&str>>(),
        [
            stamp,
            "bars.json: fixture",
            bars.as_str(),
            "volume rule: passes; failures 0; positives at or above the bar 1 of 1",
            "chapter rule: no bar, not tested; failures no bar; positives at or above the bar no bar",
            "pack rule: passes; failures 0; positives at or above the bar 3 of 3",
            exploratory.as_str(),
            line.as_str(),
            line_of(&rules, "order: "),
        ]
    );
    for file in ["held-out/rules.txt", "held-out/lookup.txt"] {
        assert_eq!(read_ev(&ev, file).lines().next(), Some(stamp), "{file}");
    }
    assert_eq!(line_of(&rules, "bars.json: "), "bars.json: fixture");

    let volume = section_of(&rules, "volume rule, on V");
    assert_eq!(
        line_of(volume, "frozen bar: "),
        "frozen bar: 0.55 (exactly 0.55)"
    );
    assert_eq!(
        line_of(volume, "highest negative: "),
        "highest negative: 0.5000, en/H v01.cbz and raw/H v01-02 (InPack)"
    );
    let manga = section_of(&rules, "manga chapter rule, on C");
    assert_eq!(
        line_of(manga, "positives: "),
        "positives: 6, negatives: 2, unlabelled in its gates: 3"
    );
    assert_eq!(
        line_of(manga, "verdict on held-out: "),
        "verdict on held-out: no bar, not tested"
    );
    let comics = section_of(&rules, "comics, every pair whose A is an issue");
    for want in [
        "positives: 1, negatives: 1, unlabelled in its gates: 0",
        "highest negative: 0.0000, is/C 003.cbz and col/C v01.cbz (NotInCollected)",
        "positives above the highest negative: 1 of 1 (100.0%)",
        "pairs whose A is an issue: 2, in the chapter rule's gates: 2",
        "with no frozen chapter bar, issues are suggestions only whatever the comics score: \
         reported descriptively",
    ] {
        assert!(comics.lines().any(|l| l == want), "{want} in\n{comics}");
    }
    let pack = section_of(&rules, "pack rule, on C");
    assert_eq!(
        line_of(pack, "verdict on held-out: "),
        "verdict on held-out: passes"
    );
    let features = "other features, descriptively:";
    assert_eq!(comics.matches(features).count(), 0, "{comics}");
    let chapter = section_of(&rules, "chapter rule, all its pairs, manga and comics");
    assert_eq!(chapter.lines().nth(1), Some(features), "{chapter}");
    assert_eq!(chapter.matches(features).count(), 1, "{chapter}");
}

/// Every line about the exploratory line is tagged, and the round lists the
/// pairs that can change a result first, each part in scored order, and
/// renders as a sheet.
#[test]
fn score_held_out_tags_the_exploratory_line_and_lists_a_round() {
    let (_root, library, ev, m) = held_out_fixture("match-held-out-round");
    score_held_out_once(&library, &ev, &m);
    let rules = read_ev(&ev, "held-out/rules.txt");
    let stamp = rules.lines().next().unwrap_or_default();
    let tag = format!(" {EXPLORATORY}");
    let exploratory = section_of(&rules, "chapter in volume, on C");
    for l in exploratory.lines().filter(|l| !l.is_empty()) {
        assert!(l.ends_with(&tag), "{l}");
    }
    for want in [
        "frozen bar: 0.60 (exactly 0.6)",
        "positives: 6, negatives: 2, unlabelled in its gates: 3",
        "  failure: 1.0000, ch/H c010.cbz and en/H v02.cbz (ChapterNotIn)",
        "  no verdict, counted as different: 1.0000, ch/H c020.cbz and en/H v02.cbz",
        "  no verdict, counted as different: 1.0000, ch/H c020.cbz and raw/H v01-02",
        "positives at or above the bar: 6 of 6 (100.0%)",
        "verdict on held-out: fails",
    ] {
        let want = format!("{want}{tag}");
        assert!(
            exploratory.lines().any(|l| l == want),
            "{want} in\n{exploratory}"
        );
    }

    let round: ToJudge =
        serde_json::from_str(&read_ev(&ev, "held-out/adjudicate-1.json")).expect("it parses");
    assert_eq!((round.stamp.as_str(), round.round), (stamp, 1));
    let shown: Vec<(&str, &str)> = (round.pairs.iter())
        .map(|p| (p.a.as_str(), p.b.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("ch/H c010.cbz", "en/H v02.cbz"),
            ("ch/H c020.cbz", "en/H v02.cbz"),
            ("ch/H c020.cbz", "raw/H v01-02"),
            ("en/H v01.cbz", "raw/H v01-02"),
            ("raw/H v01.cbz", "raw/H v01-02"),
            ("is/C 001.cbz", "col/C v01.cbz"),
        ]
    );
    let inside = "InCollected negative, volume rule 0.5000 at or above 0.50";
    assert_eq!(round.pairs[5].why, [inside]);
    assert_eq!(
        line_of(&rules, "order: "),
        format!(
            "order: the first 3 can change a result, each an unlabelled pair at or above a frozen \
             bar or the chapter-in-volume line {EXPLORATORY}, or a labelled negative at or above \
             0.50 and one of them; the other 3 are labelled negatives at or above 0.50 below every \
             bar and the line, which no verdict can make a failure; each part in scored order"
        )
    );
    let sheet = render(&library, &ev.join("held-out")).expect("the round renders");
    assert_eq!(sheet, Some(ev.join("held-out/adjudicate-1.html")));
    let html = read_ev(&ev, "held-out/adjudicate-1.html");
    assert_eq!(html.split("<section>").skip(1).count(), 6);
}

/// The fixture's first round judged: `inside` for the three pairs that can
/// change a result, or the verdicts given, recorded as `held-out round 1`,
/// and the run repeated.
fn held_out_judged(tag: &str, verdicts: [bool; 3]) -> (PathBuf, String, TempDir) {
    let (root, library, ev, mut m) = held_out_fixture(tag);
    score_held_out_once(&library, &ev, &m);
    let round: ToJudge =
        serde_json::from_str(&read_ev(&ev, "held-out/adjudicate-1.json")).expect("it parses");
    m.adjudications = (round.pairs.iter().zip(verdicts))
        .map(|(p, same)| adjudication_of(&p.a, &p.b, same, "held-out round 1"))
        .collect();
    let line = score_held_out_once(&library, &ev, &m);
    (ev, line, root)
}

/// A labelled negative at the line judged inside is a relabel the line's
/// pass rests on, and two unlabelled pairs judged inside become positives;
/// the volume rule, where the relabelled pair stays a negative, passes
/// plainly. The predictions are given as adjudicated and before the round,
/// and `lookup.txt` keeps the latter.
#[test]
fn score_held_out_counts_a_relabel_the_pass_rests_on() {
    let (ev, line, _root) = held_out_judged("match-held-out-relabel", [true; 3]);
    assert_eq!(line, "round 2: 3 pairs to judge, in adjudicate-2.json");
    let rules = read_ev(&ev, "held-out/rules.txt");
    assert_eq!(
        line_of(&rules, "adjudications of held-out pairs: "),
        "adjudications of held-out pairs: 3, same: 3, relabels of a labelled pair: 1, rounds: 1"
    );
    let tag = format!(" {EXPLORATORY}");
    let exploratory = section_of(&rules, "chapter in volume, on C");
    for want in [
        "relabels the result rests on: 1",
        "  relabelled: 1.0000, ch/H c010.cbz and en/H v02.cbz (ChapterNotIn by its names, now \
         ChapterIn)",
        "positives at or above the bar: 9 of 9 (100.0%)",
        "verdict on held-out: passes after 1 relabels",
    ] {
        let want = format!("{want}{tag}");
        assert!(
            exploratory.lines().any(|l| l == want),
            "{want} in\n{exploratory}"
        );
    }
    let volume = section_of(&rules, "volume rule, on V");
    assert_eq!(
        line_of(volume, "relabels the result rests on: "),
        "relabels the result rests on: 0"
    );
    assert_eq!(
        line_of(volume, "verdict on held-out: "),
        "verdict on held-out: passes"
    );
    let order = line_of(&rules, "order: ");
    assert!(
        order.starts_with("order: the first 0 can change a result, "),
        "{order}"
    );
    assert!(
        order.contains("; the other 3 are labelled negatives at or above 0.50 "),
        "{order}"
    );

    let p2 = |when: &str, report: &str| line_of(report, &format!("{when}P2, ")).to_owned();
    assert!(
        p2("as adjudicated: ", &rules).ends_with(": 3 of 3 chapters with a holder (100.0%), holds"),
        "{rules}"
    );
    assert!(
        p2("before any held-out round: ", &rules)
            .ends_with(": 1 of 2 chapters with a holder (50.0%), fails"),
        "{rules}"
    );
    let lookup = read_ev(&ev, "held-out/lookup.txt");
    assert!(
        p2("", &lookup).ends_with(": 1 of 2 chapters with a holder (50.0%), fails"),
        "{lookup}"
    );
    let p1 = |when: &str| line_of(&rules, &format!("{when}P1, ")).to_owned();
    assert!(p1("as adjudicated: ").ends_with(": 5 of 5 queries with a positive, holds"));
    assert!(p1("before any held-out round: ").ends_with(": 4 of 4 queries with a positive, holds"));
}

/// "Not inside" keeps a labelled negative a failure, and turns an unlabelled
/// pair at the line into one; neither is a relabel.
#[test]
fn score_held_out_fails_a_line_on_a_different_verdict() {
    let (ev, _line, _root) = held_out_judged("match-held-out-different", [false, true, false]);
    let rules = read_ev(&ev, "held-out/rules.txt");
    assert_eq!(
        line_of(&rules, "adjudications of held-out pairs: "),
        "adjudications of held-out pairs: 3, same: 1, relabels of a labelled pair: 0, rounds: 1"
    );
    let exploratory = section_of(&rules, "chapter in volume, on C");
    let tagged: Vec<String> = [
        "failures: 2, labelled negatives at or above the bar: 2, unlabelled at or above it with \
         no verdict, each counted as different: 0",
        "  failure: 1.0000, ch/H c010.cbz and en/H v02.cbz (ChapterNotIn)",
        "  failure: 1.0000, ch/H c020.cbz and raw/H v01-02 (ChapterNotIn)",
        "relabels the result rests on: 0",
        "verdict on held-out: fails",
    ]
    .iter()
    .map(|want| format!("{want} {EXPLORATORY}"))
    .collect();
    let lines: Vec<&str> = exploratory.lines().collect();
    let at = (lines.iter())
        .position(|l| l.starts_with("failures: "))
        .unwrap_or_else(|| panic!("{exploratory}"));
    assert_eq!(lines[at..at + 4], tagged[..4]);
    assert_eq!(lines.last().copied(), Some(tagged[4].as_str()));
}

/// A pair a verdict took out of the rule, two chapters judged the same,
/// stays among the rule's held-out rows as a relabel its result rests on;
/// a pair outside the rule by names and label, or an unlabelled one outside
/// its gates, is not a row. The exploratory line keeps a chapter against a
/// volume only.
#[test]
fn held_rows_keep_a_pair_a_verdict_took_out_of_the_rule() {
    use Label::{ChapterIn, ChapterNotIn, Neighbour, SameVolume, Unlabelled};
    let m = manifest_of(vec![
        volume("en/s v01.cbz", "s", 1),
        volume("en/s v02.cbz", "s", 2),
        chapter("ch/s c1.cbz", "s", Some(1)),
        chapter("ch/s c2.cbz", "s", Some(1)),
        chapter("ch/s c3.cbz", "s", None),
    ]);
    let scored = vec![
        scored_as((2, 3), SameVolume { copy: false }, 10, (0.9, 0.9)),
        scored_as((0, 1), Neighbour, 20, (0.6, 0.6)),
        scored_as((4, 0), Unlabelled, 7, (0.1, 1.0)),
        scored_as((2, 0), ChapterIn, 10, (0.1, 1.0)),
        scored_as((4, 1), Unlabelled, 10, (0.1, 0.8)),
    ];
    let rows = held_rows(Against::Rule(Rule::Chapter), &m, &scored);
    let at: Vec<usize> = rows.iter().map(|(n, _)| *n).collect();
    assert_eq!(at, [0, 3, 4]);
    let out = &rows[0].1;
    assert_eq!(
        (out.names, out.names_class, out.row.class),
        (ChapterNotIn, Some(false), None)
    );
    let held: Vec<HeldRow> = rows.into_iter().map(|(_, r)| r).collect();
    let j = judge(&held, Some(0.85));
    assert_eq!(
        (j.relabels, j.unjudged, j.negatives, j.unlabelled),
        (vec![0], Vec::<usize>::new(), 0, 1)
    );
    let line: Vec<usize> = (held_rows(Against::ChapterInVolume, &m, &scored).iter())
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(line, [3, 4]);
}

/// The log opens with the stamp, the commit that froze the bars and the
/// bars before any pair is read, so a run that stops early still dates them.
#[test]
fn score_held_out_opens_its_log_before_reading_any_pair() {
    let root = TempDir::new("match-held-out-log");
    let (library, ev) = (root.join("library"), root.join("ev"));
    let m = manifest_of(vec![volume("en/H v01.cbz", "h", 1)]);
    let bars = test_bars(0.5, 0.4, 0.6);
    let why = super::held_out::run(&library, &ev, &m, &bars, "bars.json: fixture")
        .expect_err("there is no leaders.json");
    assert!(why.contains("leaders.json"), "{why}");
    let log = read_ev(&ev, "logs/score-held-out.log");
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 3, "{log}");
    assert!(lines[0].starts_with("# score_held_out "), "{log}");
    assert_eq!(lines[1], "bars.json: fixture");
    assert!(
        lines[2].starts_with("frozen bars, manifest fixture: "),
        "{log}"
    );
}

/// The run goes ahead only when exactly one commit touches `bars.json`. A
/// later edit, even committed, would leave HEAD agreeing with the compiled
/// bars while they are no longer the frozen ones.
#[test]
fn bars_json_must_be_touched_by_one_commit_alone() {
    let first = "89abcdef0123456789abcdef0123456789abcdef";
    let other = "0123456789abcdef0123456789abcdef01234567";
    assert_eq!(
        one_freeze(Some(&format!("{first}\n"))),
        Ok(format!("bars.json: 1 commit touches it, {first}"))
    );
    assert_eq!(
        one_freeze(Some(&format!("{other}\n{first}\n"))),
        Err(format!(
            "2 commits touch bars.json ({other}, {first}), so its bars changed after the first \
             froze them"
        ))
    );
    for empty in [Some(""), Some("\n")] {
        assert_eq!(
            one_freeze(empty),
            Err("no commit touches bars.json, so no bar is frozen".to_owned()),
            "{empty:?}"
        );
    }
    assert_eq!(
        one_freeze(None),
        Err("git could not list the commits touching bars.json".to_owned())
    );
}

/// The compiled-in sample is the registered one: 26 entries, each classed
/// by its top directory, so the Perfect Edition omnibus counts as `volume`.
#[test]
fn the_nas_sample_is_the_registered_one_classed_by_top_directory() {
    use std::collections::BTreeMap;
    let dirs = [
        ("volume", "Manga/Volumes/"),
        ("raw", "Manga/Raw/"),
        ("chapter", "Manga/Chapters/"),
        ("issue", "Comics/Issues/"),
        ("collected", "Comics/Collected/"),
    ];
    let entries = sample(SAMPLE);
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for &(class, path) in &entries {
        let dir = (dirs.iter().find(|(c, _)| *c == class))
            .map_or_else(|| panic!("{path}: no class {class}"), |(_, dir)| *dir);
        assert!(path.starts_with(dir), "{class} {path}");
        *counts.entry(class).or_default() += 1;
    }
    let want = [
        ("chapter", 12),
        ("collected", 1),
        ("issue", 2),
        ("raw", 5),
        ("volume", 6),
    ];
    assert_eq!(counts, BTreeMap::from(want));
    let paths: BTreeSet<&str> = entries.iter().map(|&(_, path)| path).collect();
    assert_eq!(paths.len(), 26, "no entry twice");
    let omnibus =
        "Manga/Volumes/Monster - The Perfect Edition/Monster - The Perfect Edition - v01.cbz";
    assert!(entries.contains(&("volume", omnibus)));
}

#[test]
#[should_panic(expected = "no tab")]
fn a_sample_line_with_no_tab_panics() {
    sample("volume\tManga/Volumes/a.cbz\nvolume Manga/Volumes/b.cbz\n");
}

/// Review Focus 5 for `nas_timing`: unset skips, and a relative root panics
/// naming its variable.
#[test]
fn a_nas_root_is_skipped_unset_and_refused_relative() {
    assert_eq!(nas_root(None), None);
    let dir = TempDir::new("match-nas-root");
    let abs = dir.to_string_lossy().into_owned();
    assert_eq!(nas_root(Some(abs.clone())), Some(PathBuf::from(abs)));
    let refused = std::panic::catch_unwind(|| nas_root(Some("media".to_owned())));
    let why = refused.expect_err("a relative root panics");
    let why = (why.downcast_ref::<String>()).map_or("", String::as_str);
    assert!(why.contains("TAIYAKU_MATCH_NAS_ROOT=media"), "{why}");
}

/// Four fifths is enough, and one entry short of it is not.
#[test]
fn the_run_needs_four_fifths_of_the_sample() {
    assert!(enough(4, 5));
    assert!(!enough(3, 5));
    assert!(enough(21, 26));
    assert!(!enough(20, 26));
    assert!(!enough(0, 0));
}

/// The root's mount is the deepest point holding it by whole components,
/// the later of two at one point, with `/proc/mounts`' escapes read.
#[test]
fn the_roots_mount_is_the_deepest_and_latest_holding_it() {
    let mounts = "/dev/sda1 / ext4 rw,relatime 0 0\n\
                  tmpfs /med tmpfs rw 0 0\n\
                  shfs /mnt/user/media\\0401 fuse.shfs rw,noatime 0 0\n\
                  shfs /media fuse.shfs rw,noatime 0 0\n\
                  shfs /media fuse.shfs ro,noatime 0 0\n";
    let of = |root: &str| mount_of(mounts, Path::new(root)).unwrap_or_default();
    assert_eq!(of("/media"), "shfs /media fuse.shfs ro,noatime 0 0");
    assert_eq!(of("/media/Manga"), "shfs /media fuse.shfs ro,noatime 0 0");
    assert_eq!(of("/medium"), "/dev/sda1 / ext4 rw,relatime 0 0");
    assert!(of("/mnt/user/media 1/Comics").starts_with("shfs /mnt/user/media\\0401 "));
    assert_eq!(mount_of("", Path::new("/media")), None);
    assert_eq!(unescape("a\\040b\\011c\\134d\\0"), "a b\tc\\d\\0");
}

#[test]
fn a_mount_is_read_only_by_its_own_ro_option() {
    assert!(read_only("shfs /media fuse.shfs ro,noatime 0 0"));
    assert!(read_only("shfs /media fuse.shfs nosuid,ro 0 0"));
    assert!(!read_only("/dev/sda1 / ext4 rw,errors=remount-ro 0 0"));
    assert!(!read_only("/dev/sda1 / ext4 rw,relatime 0 0"));
    assert!(!read_only("short line"));
}

/// Each entry is a timing line, `MISSING` or `UNREADABLE` with why, and the
/// run goes on: a directory of archives sums its archives' pages and bytes.
#[test]
fn every_sample_entry_is_timed_missing_or_unreadable() {
    let root = TempDir::new("match-nas-sample");
    let volumes = root.join("Manga/Volumes/s");
    let pack = root.join("Manga/Raw/s/v01-02");
    let issues = root.join("Comics/Issues");
    for dir in [&volumes, &pack, &issues] {
        std::fs::create_dir_all(dir).expect("make the directory");
    }
    let good = cbz_pages(&volumes, "v01.cbz", 3, 1);
    let first = cbz_pages(&pack, "v01.cbz", 2, 20);
    let second = cbz_pages(&pack, "v02.cbz", 4, 40);
    std::fs::write(issues.join("bad.cbz"), b"Rar!\x1a\x07\x00").expect("write bad.cbz");
    let entries = [
        ("volume", "Manga/Volumes/s/v01.cbz"),
        ("raw", "Manga/Raw/s/v01-02"),
        ("issue", "Comics/Issues/bad.cbz"),
        ("chapter", "Manga/Chapters/gone.cbz"),
    ];

    let mut lines = Vec::new();
    let done = time_sample(&root, &entries, &mut |line| lines.push(line));
    assert_eq!(done, 2);
    assert_eq!(lines.len(), 5, "{lines:#?}");
    assert_eq!(
        lines[0],
        "class\tpath\tbytes\tpages\tdigest ms\tre-read ms\tfingerprint ms\tloadavg after"
    );
    let timed = |line: &str, class: &str, path: &str, bytes: u64, pages: usize| {
        let cells: Vec<&str> = line.split('\t').collect();
        assert_eq!(cells.len(), 8, "{line}");
        let (want_bytes, want_pages) = (bytes.to_string(), pages.to_string());
        assert_eq!(
            cells[..4],
            [class, path, &want_bytes, &want_pages],
            "{line}"
        );
        for ms in &cells[4..7] {
            assert!(ms.parse::<f64>().is_ok_and(|ms| ms >= 0.0), "{line}");
        }
        assert!(!cells[7].is_empty(), "{line}");
    };
    timed(&lines[1], "volume", entries[0].1, file_len(&good), 3);
    let both = file_len(&first) + file_len(&second);
    timed(&lines[2], "raw", entries[1].1, both, 6);
    assert!(
        lines[3].starts_with("UNREADABLE Comics/Issues/bad.cbz: ") && lines[3].contains("RAR"),
        "{}",
        lines[3]
    );
    assert_eq!(lines[4], "MISSING Manga/Chapters/gone.cbz");
}

/// Each phase lands under its own header cell: the extrapolation reads the
/// digest column as the cold read and the fingerprint column as the decode,
/// and three distinct durations show any two swapped.
#[test]
fn a_timing_line_puts_each_phase_under_its_own_header() {
    use std::time::Duration;
    let t = Timing {
        bytes: 1_000,
        pages: 7,
        digest: Duration::from_micros(1_500),
        read: Duration::from_millis(20),
        describe: Duration::from_millis(300),
    };
    let line = timing_line("raw", "Manga/Raw/s/v01", &t, "1.00 2.00 3.00");
    let cells: Vec<(&str, &str)> = TIMING_HEADER.split('\t').zip(line.split('\t')).collect();
    assert_eq!(
        cells,
        [
            ("class", "raw"),
            ("path", "Manga/Raw/s/v01"),
            ("bytes", "1000"),
            ("pages", "7"),
            ("digest ms", "1.5"),
            ("re-read ms", "20.0"),
            ("fingerprint ms", "300.0"),
            ("loadavg after", "1.00 2.00 3.00"),
        ]
    );
    assert_eq!(line.split('\t').count(), 8, "{line}");
}

/// The kernel's lines name its size and count every comparison it made.
#[test]
fn the_kernel_lines_count_queries_times_stored() {
    let text = kernel_lines(3, 5);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[..3],
        [
            "kernel queries\t3",
            "kernel stored\t5",
            "kernel comparisons\t15"
        ]
    );
    assert!(lines[3].starts_with("kernel seconds\t"), "{text}");
    assert!(
        lines[4].starts_with("kernel comparisons per second\t"),
        "{text}"
    );
    assert_eq!(lines.len(), 5, "{text}");
}
