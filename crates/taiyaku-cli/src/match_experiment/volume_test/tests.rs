use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use taiyaku_core::Verdict;
use taiyaku_read::test_support::{TempDir, cbz_pages, print_page};

use super::super::features::{PairFeatures, volume_score};
use super::super::manifest::{
    Adjudication, Kind, Manifest, Source, Split, TABLE, draft_member, label,
};
use super::super::rule::{GAP, label_name};
use super::super::score::{Scored, scores_tsv};
use super::super::{Log, Material};
use super::bar::{FrozenBar, Pr1Row, derive, pairs_text, read_frozen, read_pr1, sha256_hex};
use super::control::{self, differs, find, fresh_prints, holds_the_pair};
use super::members::{
    PairClass, ROWS, TestMember, all_pairs, committed, draft, number_of, pair_class, to_manifest,
};
use super::run::{
    BAR_FILE, Freeze, GitFacts, Judged, Outcome, PairRow, VERDICTS_PATH, cache_is_unused,
    edition_kind, finished_bar, git_facts, judge, measure, preflight, read_scores, rejudge,
    report_text, score, scores_text, settled_bar, sheets,
};
use super::unread::left_out;

#[test]
fn members_are_the_157_the_rulings_name() {
    let span = |first: u32, last: u32| (first..=last).collect::<Vec<_>>();
    let table = [
        (
            "Raw/Tongari Boushi no Atelier",
            "tongari",
            "raw",
            [span(1, 13), vec![16]].concat(),
        ),
        (
            "Volumes/Tongari Boushi no Atelier",
            "tongari",
            "en",
            span(1, 14),
        ),
        ("Raw/RuriDragon", "ruridragon", "raw", vec![1, 2, 3, 5]),
        ("Volumes/RuriDragon", "ruridragon", "en", span(1, 3)),
        (
            "Volumes/Hunter × Hunter",
            "hunter-x-hunter",
            "bw",
            span(1, 36),
        ),
        (
            "Volumes/Hunter × Hunter (Official Colored) ",
            "hunter-x-hunter",
            "colour",
            span(1, 37),
        ),
        ("Volumes/Parasyte", "parasyte", "bw", span(1, 8)),
        (
            "Volumes/Parasyte - Full Color Collection",
            "parasyte",
            "collection",
            span(1, 8),
        ),
        (
            "Volumes/Parasyte - Full Color (Colored Council)",
            "parasyte",
            "colored-council",
            span(1, 10),
        ),
        ("Volumes/Yu Yu Hakusho", "yu-yu-hakusho", "bw", span(1, 19)),
        (
            "Volumes/Yu Yu Hakusho - Colored (Colored Council)",
            "yu-yu-hakusho",
            "colored-council",
            span(1, 4),
        ),
    ];
    let members = committed();
    assert_eq!(members.len(), 157);
    assert_eq!(ROWS.len(), table.len());
    for (row, (dir, series, edition, numbers)) in ROWS.iter().zip(&table) {
        assert_eq!(
            (row.dir, row.series, row.franchise, row.edition),
            (*dir, *series, *series, *edition)
        );
        assert_eq!(
            (row.kind, row.ltr, row.split),
            (Some(Kind::Volume), false, Split::HeldOut),
            "{dir}"
        );
        let held: Vec<&TestMember> = (members.iter()).filter(|m| m.directory() == *dir).collect();
        for m in &held {
            assert_eq!((m.series.as_str(), m.edition.as_str()), (*series, *edition));
            let name = m.path.rsplit('/').next().expect("a path has a last part");
            assert_eq!(number_of(name), Some(m.number), "{}", m.path);
        }
        let mut found: Vec<u32> = held.iter().map(|m| m.number).collect();
        found.sort_unstable();
        assert_eq!(&found, numbers, "{dir}");
    }
}

#[test]
fn pairs_are_counted_once_with_their_classes() {
    let m = committed();
    let pairs = all_pairs(m.len());
    assert_eq!(pairs.len(), 12_246);
    assert_eq!(pairs.iter().collect::<BTreeSet<_>>().len(), 12_246);
    assert!(pairs.iter().all(|&(i, j)| i < j && j < m.len()));
    let (mut t, mut d, mut s, mut w) = (0, 0, 0, 0);
    for &(i, j) in &pairs {
        let class = pair_class(&m[i], &m[j]);
        assert_eq!(
            class,
            pair_class(&m[j], &m[i]),
            "{} against {}",
            m[i].path,
            m[j].path
        );
        match class {
            PairClass::True => t += 1,
            PairClass::Damaged => d += 1,
            PairClass::DifferentSplit => s += 1,
            PairClass::Wrong => w += 1,
        }
    }
    assert_eq!((t, d, s, w), (59, 1, 236, 11_950));
}

#[test]
fn awkward_names_read_their_numbers() {
    for (name, n) in [
        ("[眞藤雅興] ルリドラゴン 第05巻.cbz", 5),
        ("DLRAW.NET-Ruri Dragon v01s.cbz", 1),
        ("ルリドラゴン v02 DL-Raw.Se.cbz", 2),
        ("Yu Yu Hakusho - v04 - Colored (Colored Council).cbz", 4),
        ("Parasyte - Full Color v10 (2021) (Colored Council).cbz", 10),
        ("Hunter × Hunter (Official Colored) - v37.cbz", 37),
        ("Tongari Boushi no Atelier v16", 16),
    ] {
        assert_eq!(number_of(name), Some(n), "{name}");
    }
    assert_eq!(number_of("Parasyte - Full Color (2021).cbz"), None);
    // The scored manifest's labels rest on this reader, so the fallback stays outside it.
    assert_eq!(
        draft_member("[眞藤雅興] ルリドラゴン 第05巻.cbz", &ROWS[2]).volumes,
        None
    );
}

#[test]
fn pair_classes_by_example() {
    use PairClass::{Damaged as D, DifferentSplit as S, True as T, Wrong as W};
    let cases = [
        (("tongari", "raw", 13), ("tongari", "en", 13), T),
        (("tongari", "raw", 9), ("tongari", "en", 9), T),
        (("tongari", "raw", 9), ("tongari", "en", 8), W),
        (("tongari", "raw", 16), ("tongari", "en", 14), W),
        (("ruridragon", "raw", 1), ("ruridragon", "en", 1), T),
        (("ruridragon", "raw", 5), ("ruridragon", "en", 3), W),
        (
            ("hunter-x-hunter", "colour", 36),
            ("hunter-x-hunter", "bw", 36),
            T,
        ),
        (
            ("hunter-x-hunter", "colour", 35),
            ("hunter-x-hunter", "bw", 35),
            D,
        ),
        (
            ("hunter-x-hunter", "colour", 35),
            ("hunter-x-hunter", "bw", 34),
            W,
        ),
        (
            ("hunter-x-hunter", "colour", 37),
            ("hunter-x-hunter", "bw", 36),
            W,
        ),
        (("parasyte", "bw", 8), ("parasyte", "collection", 8), T),
        (("parasyte", "colored-council", 1), ("parasyte", "bw", 1), S),
        (
            ("parasyte", "colored-council", 8),
            ("parasyte", "collection", 8),
            S,
        ),
        (
            ("parasyte", "colored-council", 10),
            ("parasyte", "bw", 8),
            S,
        ),
        (
            ("parasyte", "colored-council", 1),
            ("parasyte", "colored-council", 2),
            W,
        ),
        (
            ("yu-yu-hakusho", "colored-council", 1),
            ("yu-yu-hakusho", "bw", 1),
            S,
        ),
        (
            ("yu-yu-hakusho", "colored-council", 4),
            ("yu-yu-hakusho", "bw", 19),
            S,
        ),
        (("yu-yu-hakusho", "bw", 1), ("yu-yu-hakusho", "bw", 2), W),
        (("tongari", "raw", 1), ("hunter-x-hunter", "bw", 1), W),
    ];
    let members = committed();
    let find = |(series, edition, number): (&str, &str, u32)| {
        (members.iter())
            .find(|m| m.series == series && m.edition == edition && m.number == number)
            .unwrap_or_else(|| panic!("no member {series} {edition} {number}"))
    };
    for (a, b, class) in cases {
        let (x, y) = (find(a), find(b));
        assert_eq!(pair_class(x, y), class, "{a:?} against {b:?}");
        assert_eq!(pair_class(y, x), class, "{b:?} against {a:?}");
    }
}

#[test]
fn the_colour_directory_keeps_its_trailing_space() {
    let path =
        "Volumes/Hunter × Hunter (Official Colored) /Hunter × Hunter (Official Colored) - v01.cbz";
    assert!(committed().iter().any(|m| m.path == path));
    // Checked apart from the members: editing a row does not regenerate them.
    let dir = "Volumes/Hunter × Hunter (Official Colored) ";
    assert!(ROWS.iter().any(|row| row.dir == dir));
}

#[test]
fn the_series_are_new_and_none_is_left_to_right() {
    for row in &ROWS {
        assert!(
            (TABLE.iter()).all(|scored| scored.dir != row.dir && scored.series != row.series),
            "{}",
            row.dir
        );
    }
    let members = committed();
    let manifest = to_manifest(&members);
    assert!(manifest.adjudications.is_empty());
    assert_eq!(manifest.members.len(), members.len());
    for (m, t) in manifest.members.iter().zip(&members) {
        assert!(!m.ltr, "{}", m.path);
        assert_eq!(m.kind, Kind::Volume, "{}", m.path);
        assert_eq!(m.volumes, Some((t.number, t.number)), "{}", m.path);
        assert_eq!(
            (&m.path, &m.series, &m.franchise, &m.edition),
            (&t.path, &t.series, &t.series, &t.edition)
        );
    }
}

/// Every row's directory under a new library, each holding one numbered
/// volume, so that a case changes only the directory it is about.
fn library() -> TempDir {
    let root = TempDir::new("volume-test-draft");
    for row in &ROWS {
        let dir = root.path().join(row.dir);
        std::fs::create_dir_all(&dir).expect("a row's directory");
        std::fs::write(dir.join("Any v01.cbz"), b"").expect("a member");
    }
    root
}

#[test]
fn draft_keeps_what_is_numbered_and_skips_what_is_no_member() {
    let root = library();
    let dir = root.path().join(ROWS[0].dir);
    std::fs::create_dir(dir.join("Any v02")).expect("a directory member");
    std::fs::write(dir.join(".hidden v03.cbz"), b"").expect("a hidden entry");
    std::fs::write(dir.join("notes v04.txt"), b"").expect("a stray file");

    let members = draft(root.path()).expect("every entry has a number");
    let first: Vec<u32> = (members.iter())
        .filter(|m| m.directory() == ROWS[0].dir)
        .map(|m| m.number)
        .collect();
    assert_eq!(first, [1, 2]);
    assert_eq!(members.len(), ROWS.len() + 1);
    assert!(members.windows(2).all(|w| w[0].path < w[1].path));
}

#[test]
fn draft_names_an_entry_it_cannot_number_or_that_repeats_a_number() {
    let root = library();
    let unnumbered = root.path().join(ROWS[1].dir).join("No Number.cbz");
    std::fs::write(&unnumbered, b"").expect("an unnumbered member");
    let why = draft(root.path()).expect_err("no number is refused");
    assert!(why.contains("No Number.cbz"), "{why}");

    let root = library();
    let repeat = root.path().join(ROWS[1].dir).join("Other v01.cbz");
    std::fs::write(&repeat, b"").expect("a repeated number");
    let why = draft(root.path()).expect_err("a repeated number is refused");
    assert!(why.contains(ROWS[1].dir), "{why}");
    assert!(why.contains(" v01.cbz"), "{why}");
}

/// A manifest of these paths and kinds. They share a series so that a row's
/// label cell can be asked of `label` and not written out by hand.
fn manifest_of(entries: &[(&str, Kind)]) -> Manifest {
    let members: Vec<TestMember> = (entries.iter().zip(1..))
        .map(|((path, _), number)| TestMember {
            path: (*path).to_owned(),
            series: "fixture".to_owned(),
            edition: "en".to_owned(),
            number,
        })
        .collect();
    let mut m = to_manifest(&members);
    for (member, (_, kind)) in m.members.iter_mut().zip(entries) {
        member.kind = *kind;
    }
    m
}

/// A row of two 20-slot members, whose V is `long_paired` twentieths.
fn scored_row(m: &Manifest, a: &str, b: &str, class: &str, long_paired: u32) -> Pr1Row {
    let at = |path: &str| {
        (m.members.iter())
            .find(|x| x.path == path)
            .expect("a fixture member")
    };
    Pr1Row {
        a: a.to_owned(),
        b: b.to_owned(),
        label: label_name(label(m, at(a), at(b))),
        class: class.to_owned(),
        verdict: "Sound".to_owned(),
        v_cell: format!("{:.6}", volume_score(long_paired, 20, 20)),
        a_slots: 20,
        b_slots: 20,
        paired: long_paired,
        long_paired,
        runs: 1,
    }
}

#[test]
fn pr1_rows_take_v_from_their_integers() {
    let table = |v_cell: &str| {
        format!(
            "{}a.cbz\tb.cbz\texplicit\tNeighbour\tnegative\t-\t-\t3\t3\tSound\t0.1\t0.1\t1.00\t1\t\
             2\t1\t1\t{v_cell}\t0.1\t0.1\n",
            scores_tsv("# stamp", &Manifest::default(), &[])
        )
    };
    let rows = read_pr1(&table("0.333333")).expect("the cell prints the integers' value");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].v(), 1.0 / 3.0);

    let why = read_pr1(&table("0.333334")).expect_err("a cell that is not that value");
    assert!(
        why.contains("0.333334") && why.contains("0.333333"),
        "{why}"
    );
}

/// The fixture above is written by hand, so only the scorer's own output can
/// show that each column is read from where the scorer puts it.
#[test]
fn pr1_rows_read_the_columns_the_scorer_writes() {
    let m = manifest_of(&[("s/a.cbz", Kind::Volume), ("s/b.cbz", Kind::Volume)]);
    let features = control_features();
    let scored = Scored {
        a: 0,
        b: 1,
        source: Source::Explicit,
        label: label(&m, &m.members[0], &m.members[1]),
        features: features.clone(),
        runs: 7,
        ms: 0.0,
    };
    let rows = read_pr1(&scores_tsv("# stamp", &m, &[scored])).expect("the scorer's own table");
    let row = &rows[0];
    assert_eq!((row.a.as_str(), row.b.as_str()), ("s/a.cbz", "s/b.cbz"));
    let names = label_name(label(&m, &m.members[0], &m.members[1]));
    assert_eq!((&row.label, row.class.as_str()), (&names, "negative"));
    assert_eq!(
        (
            row.a_slots,
            row.b_slots,
            row.paired,
            row.long_paired,
            row.runs
        ),
        (3, 5, 4, 2, 7)
    );
    assert_eq!(differs(row, &features, 7), None);
}

#[test]
fn pr1_tables_of_another_shape_are_refused() {
    let header = scores_tsv("# stamp", &Manifest::default(), &[]);
    assert_eq!(read_pr1(&header), Ok(Vec::new()));
    let moved = header.replacen("a_slots\tb_slots", "b_slots\ta_slots", 1);
    assert!(read_pr1(&moved).is_err(), "a header with its columns moved");
    assert!(read_pr1("").is_err(), "an empty file");
    let short = format!("{header}a.cbz\tb.cbz\n");
    assert!(read_pr1(&short).is_err(), "a row short of its columns");
}

const V1: &str = "Volumes/S/S v01.cbz";
const V2: &str = "Volumes/S/S v02.cbz";
const V3: &str = "Volumes/S/S v03.cbz";
const CHAPTER: &str = "Chapters/S/S c01.cbz";
const PACK: &str = "Volumes/S/S v01-03.cbz";

fn bar_manifest() -> Manifest {
    manifest_of(&[
        (V1, Kind::Volume),
        (V2, Kind::Volume),
        (V3, Kind::Volume),
        (CHAPTER, Kind::Chapter),
        (PACK, Kind::Pack),
    ])
}

#[test]
fn the_bar_is_the_highest_volume_negative_plus_the_gap() {
    let m = bar_manifest();
    let dev = [
        scored_row(&m, CHAPTER, V3, "negative", 16),
        scored_row(&m, V1, V2, "negative", 10),
        scored_row(&m, V2, V3, "positive", 18),
        scored_row(&m, V1, PACK, "negative", 14),
    ];
    let derived = derive(&m, &dev, &[]).expect("a bar fits");
    assert_eq!(derived.max_negative, 0.5);
    assert_eq!(derived.bar, 0.5 + GAP);
    assert_eq!(derived.pair, dev[1]);

    let held_out = [scored_row(&m, V1, V3, "negative", 12)];
    let derived = derive(&m, &dev, &held_out).expect("a bar fits");
    assert_eq!(derived.max_negative, 0.6);
    assert_eq!(derived.bar, 0.6 + GAP);
    assert_eq!(derived.pair, held_out[0]);

    let tied = [
        scored_row(&m, V1, V2, "negative", 10),
        scored_row(&m, V1, V3, "negative", 10),
    ];
    let derived = derive(&m, &tied, &[]).expect("a bar fits");
    assert_eq!(derived.pair, tied[0], "the first of two equal negatives");
}

#[test]
fn the_bar_refuses_what_it_cannot_trust() {
    let tested = "Volumes/Parasyte/Parasyte v01.cbz";
    let tested_chapter = "Volumes/RuriDragon/RuriDragon c01.cbz";
    let spin_off = "Volumes/Hunter × Hunter - Kurapika's Memories/Kurapika v01.cbz";
    let m = manifest_of(&[
        (V1, Kind::Volume),
        (V2, Kind::Volume),
        (CHAPTER, Kind::Chapter),
        (tested, Kind::Volume),
        (tested_chapter, Kind::Chapter),
        (spin_off, Kind::Volume),
    ]);
    let good = scored_row(&m, V1, V2, "negative", 10);

    let stranger = Pr1Row {
        b: "Volumes/S/elsewhere.cbz".to_owned(),
        ..good.clone()
    };
    let why = derive(&m, &[good.clone(), stranger], &[]).expect_err("a path the manifest lacks");
    assert!(
        why.contains("elsewhere.cbz") && why.contains("manifest"),
        "{why}"
    );

    for row in [
        scored_row(&m, V1, tested, "negative", 10),
        scored_row(&m, tested, V1, "negative", 10),
        scored_row(&m, tested_chapter, V1, "negative", 10),
    ] {
        let why = derive(&m, std::slice::from_ref(&good), &[row]).expect_err("a series under test");
        assert!(why.contains("under test"), "{why}");
    }

    let relabelled = Pr1Row {
        label: "SameVolume".to_owned(),
        ..good.clone()
    };
    let why = derive(&m, &[relabelled], &[]).expect_err("a label cell that disagrees");
    assert!(
        why.contains("SameVolume") && why.contains(&good.label),
        "{why}"
    );

    let positive = scored_row(&m, V1, V2, "positive", 10);
    let why = derive(&m, &[positive], &[]).expect_err("no negative at all");
    assert!(why.contains("no bar"), "{why}");

    let high = scored_row(&m, V1, V2, "negative", 18);
    let why = derive(&m, &[high], &[]).expect_err("no bar fits above this negative");
    assert!(why.contains("no bar"), "{why}");

    let other_kind = Pr1Row {
        label: "SameVolume".to_owned(),
        ..scored_row(&m, CHAPTER, V1, "negative", 18)
    };
    let spin_off = scored_row(&m, V1, spin_off, "negative", 10);
    let derived = derive(&m, &[other_kind, spin_off.clone()], &[]).expect("both rows are read");
    assert_eq!(derived.pair, spin_off);
}

const FROZEN: &str = r#"{
    "max_negative": 0.5, "gap": 0.15, "bar": 0.65,
    "pair_a": "a.cbz", "pair_b": "b.cbz",
    "long_paired": 10, "a_slots": 20, "b_slots": 21,
    "pr1_dev_sha256": "dev", "pr1_held_out_sha256": "held",
    "members_sha256": "members", "pairs_sha256": "pairs"
}"#;

#[test]
fn a_frozen_bar_file_reads_whole() {
    let read = read_frozen(FROZEN).expect("every key is there");
    assert_eq!((read.max_negative, read.gap, read.bar), (0.5, 0.15, 0.65));
    assert_eq!(
        (read.pair_a.as_str(), read.pair_b.as_str()),
        ("a.cbz", "b.cbz")
    );
    assert_eq!((read.long_paired, read.a_slots, read.b_slots), (10, 20, 21));
    assert_eq!(
        [
            read.pr1_dev_sha256.as_str(),
            read.pr1_held_out_sha256.as_str(),
            read.members_sha256.as_str(),
            read.pairs_sha256.as_str()
        ],
        ["dev", "held", "members", "pairs"]
    );

    let whole: serde_json::Value = serde_json::from_str(FROZEN).expect("the fixture parses");
    let keys: Vec<String> = whole
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys.len(), 12);
    for key in keys {
        let mut less = whole.clone();
        less.as_object_mut().expect("an object").remove(&key);
        let why = read_frozen(&less.to_string()).expect_err("a missing key");
        assert!(why.contains(&key), "{why}");
    }
    let mut more = whole;
    more["note"] = serde_json::json!("x");
    let why = read_frozen(&more.to_string()).expect_err("an unknown key");
    assert!(why.contains("note"), "{why}");
}

fn control_row() -> Pr1Row {
    Pr1Row {
        a: "a.cbz".to_owned(),
        b: "b.cbz".to_owned(),
        label: "Neighbour".to_owned(),
        class: "negative".to_owned(),
        verdict: "Sound".to_owned(),
        v_cell: "0.400000".to_owned(),
        a_slots: 3,
        b_slots: 5,
        paired: 4,
        long_paired: 2,
        runs: 7,
    }
}

fn control_features() -> PairFeatures {
    PairFeatures {
        a_slots: 3,
        b_slots: 5,
        verdict: Verdict::Sound,
        separation: None,
        margin: None,
        avg_run: 0.0,
        paired: 4,
        long_paired: 2,
        span_b: 0,
        volume: 0.4,
        containment: 0.0,
    }
}

#[test]
fn control_compares_every_field() {
    let (row, f) = (control_row(), control_features());
    assert_eq!(differs(&row, &f, 7), None);
    let changed = [
        (
            "V",
            "0.400001",
            Pr1Row {
                v_cell: "0.400001".to_owned(),
                ..row.clone()
            },
        ),
        (
            "a_slots",
            "9",
            Pr1Row {
                a_slots: 9,
                ..row.clone()
            },
        ),
        (
            "b_slots",
            "9",
            Pr1Row {
                b_slots: 9,
                ..row.clone()
            },
        ),
        (
            "paired",
            "9",
            Pr1Row {
                paired: 9,
                ..row.clone()
            },
        ),
        (
            "long_paired",
            "9",
            Pr1Row {
                long_paired: 9,
                ..row.clone()
            },
        ),
        (
            "verdict",
            "Refused",
            Pr1Row {
                verdict: "Refused".to_owned(),
                ..row.clone()
            },
        ),
    ];
    for (field, was, row) in changed {
        let why = differs(&row, &f, 7).unwrap_or_else(|| panic!("{field} alone differs"));
        assert!(why.contains(field) && why.contains(was), "{field}: {why}");
    }
    let why = differs(&row, &f, 9).expect("runs alone differ");
    assert!(
        why.contains("runs") && why.contains('9') && why.contains('7'),
        "{why}"
    );
}

#[test]
fn control_pairs_are_ten_scored_volumes() {
    let m: Manifest =
        serde_json::from_str(include_str!("../manifest.json")).expect("manifest.json parses");
    let pairs = control::pairs();
    assert_eq!(pairs.len(), 10);
    let distinct: BTreeSet<(&str, &str)> = (pairs.iter())
        .map(|(a, b)| (a.min(b).as_str(), a.max(b).as_str()))
        .collect();
    assert_eq!(distinct.len(), 10);
    for path in pairs.iter().flat_map(|(a, b)| [a, b]) {
        let member = (m.members.iter())
            .find(|x| x.path == *path)
            .unwrap_or_else(|| panic!("{path} is not in PR 1's manifest"));
        assert_eq!(member.kind, Kind::Volume, "{path}");
        let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        assert!(
            ROWS.iter().all(|row| row.dir != dir),
            "{path} is under test"
        );
    }
}

#[test]
fn a_control_pair_is_found_whichever_way_round_it_was_written() {
    let row = |a: &str, b: &str| Pr1Row {
        a: a.to_owned(),
        b: b.to_owned(),
        ..control_row()
    };
    let (dev, held_out) = ([row("a", "b")], [row("c", "d")]);
    assert_eq!(find("a", "b", &dev, &held_out), Ok(&dev[0]));
    assert_eq!(find("b", "a", &dev, &held_out), Ok(&dev[0]));
    assert_eq!(find("d", "c", &dev, &held_out), Ok(&held_out[0]));

    let why = find("a", "d", &dev, &held_out).expect_err("a pair PR 1 never scored");
    assert!(why.contains("a and d"), "{why}");
    let twice = [row("b", "a")];
    let why = find("a", "b", &dev, &twice).expect_err("two rows for one pair");
    assert!(why.contains("a and b"), "{why}");
}

#[test]
fn the_control_sample_must_hold_the_pair_that_sets_the_bar() {
    let sample = [
        ("x".to_owned(), "a.cbz".to_owned()),
        ("b.cbz".to_owned(), "a.cbz".to_owned()),
    ];
    assert_eq!(holds_the_pair(&sample, &control_row()), Ok(()));
    let why = holds_the_pair(&sample[..1], &control_row()).expect_err("the pair is not sampled");
    assert!(why.contains("a.cbz") && why.contains("b.cbz"), "{why}");
}

#[test]
fn the_control_refuses_a_fingerprint_it_did_not_take_itself() {
    let library = TempDir::new("volume-test-control-library");
    let cache = TempDir::new("volume-test-control-cache");
    cbz_pages(library.path(), "v01.cbz", 2, 1);
    let m = manifest_of(&[("v01.cbz", Kind::Volume)]);

    let prints = fresh_prints(cache.path(), library.path(), &m, &[0]).expect("a cold cache");
    assert!(prints[0].is_some());
    let why = fresh_prints(cache.path(), library.path(), &m, &[0]).expect_err("a used cache");
    assert!(why.contains("v01.cbz") && why.contains("cache"), "{why}");
}

/// What a frozen bar must say of itself, whatever the file's numbers are.
fn its_own_arithmetic(
    frozen: &FrozenBar,
    members_json: &str,
    members: &[TestMember],
) -> Result<(), String> {
    let sums = [
        ("bar", frozen.bar == frozen.max_negative + GAP),
        ("gap", frozen.gap == GAP),
        (
            "max_negative",
            frozen.max_negative == volume_score(frozen.long_paired, frozen.a_slots, frozen.b_slots),
        ),
        (
            "members_sha256",
            frozen.members_sha256 == sha256_hex(members_json),
        ),
        (
            "pairs_sha256",
            frozen.pairs_sha256 == sha256_hex(&pairs_text(members)),
        ),
    ];
    match sums.iter().find(|(_, holds)| !holds) {
        Some((name, _)) => Err(format!("bar.json's {name} does not hold")),
        None => Ok(()),
    }
}

#[test]
fn the_frozen_bar_is_its_own_arithmetic() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/match_experiment/volume_test/bar.json");
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("{}: {e}", file.display()),
    };
    let frozen = read_frozen(&text).unwrap_or_else(|why| panic!("{why}"));
    println!("frozen bar: {}", frozen.bar);
    its_own_arithmetic(&frozen, include_str!("members.json"), &committed())
        .unwrap_or_else(|why| panic!("{why}"));
}

/// `bar.json` does not exist until the freeze, so the check above is shown
/// to catch each broken sum here.
#[test]
fn a_frozen_bar_that_breaks_its_own_arithmetic_is_caught() {
    let members = committed();
    let members_json = include_str!("members.json");
    let right = || FrozenBar {
        max_negative: 0.5,
        gap: GAP,
        bar: 0.5 + GAP,
        pair_a: "a.cbz".to_owned(),
        pair_b: "b.cbz".to_owned(),
        long_paired: 10,
        a_slots: 20,
        b_slots: 20,
        pr1_dev_sha256: "dev".to_owned(),
        pr1_held_out_sha256: "held".to_owned(),
        members_sha256: sha256_hex(members_json),
        pairs_sha256: sha256_hex(&pairs_text(&members)),
    };
    assert_eq!(its_own_arithmetic(&right(), members_json, &members), Ok(()));
    let broken = [
        (
            "bar",
            FrozenBar {
                bar: 0.7,
                ..right()
            },
        ),
        (
            "gap",
            FrozenBar {
                gap: 0.1,
                ..right()
            },
        ),
        (
            "max_negative",
            FrozenBar {
                long_paired: 11,
                ..right()
            },
        ),
        (
            "members_sha256",
            FrozenBar {
                members_sha256: "x".to_owned(),
                ..right()
            },
        ),
        (
            "pairs_sha256",
            FrozenBar {
                pairs_sha256: "x".to_owned(),
                ..right()
            },
        ),
    ];
    for (name, frozen) in broken {
        let why = its_own_arithmetic(&frozen, members_json, &members).expect_err(name);
        assert!(why.contains(name), "{why}");
    }
}

#[test]
fn the_pairs_text_is_one_line_per_pair_in_order() {
    let member = |path: &str, edition: &str, number| TestMember {
        path: path.to_owned(),
        series: "tongari".to_owned(),
        edition: edition.to_owned(),
        number,
    };
    let members = [
        member("r/v01.cbz", "raw", 1),
        member("e/v01.cbz", "en", 1),
        member("e/v02.cbz", "en", 2),
    ];
    assert_eq!(
        pairs_text(&members),
        "r/v01.cbz\te/v01.cbz\tTrue\nr/v01.cbz\te/v02.cbz\tWrong\ne/v01.cbz\te/v02.cbz\tWrong\n"
    );
}

#[test]
fn sha256_hex_is_the_digest_of_the_text() {
    assert_eq!(
        sha256_hex("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

const AT: f64 = 0.8;

fn pair_row(a: &str, b: &str, class: PairClass, v: f64) -> PairRow {
    PairRow {
        a: a.to_owned(),
        b: b.to_owned(),
        class,
        edition_kind: "-".to_owned(),
        v,
        long_paired: 0,
        a_slots: 10,
        b_slots: 10,
        verdict: "Sound".to_owned(),
        separation: "-".to_owned(),
    }
}

fn ruling(a: &str, b: &str, same: bool) -> Adjudication {
    Adjudication {
        a: a.to_owned(),
        b: b.to_owned(),
        same,
        date: "2026-11-18".to_owned(),
        by: "owner".to_owned(),
        note: "looked at the pages".to_owned(),
    }
}

fn outcome(rows: &[PairRow], verdicts: &[Adjudication]) -> Outcome {
    judge(rows, AT, verdicts)
        .expect("the verdicts stand")
        .outcome
}

#[test]
fn a_wrong_pair_exactly_at_the_bar_fails() {
    let rows = [pair_row("a", "b", PairClass::Wrong, AT)];
    let judged = judge(&rows, AT, &[]).expect("no verdicts to refuse");
    assert_eq!(judged.outcome, Outcome::Fails);
    assert_eq!(judged.at_bar, [0]);
    assert_eq!(judged.wrong, 1);
}

#[test]
fn a_wrong_pair_just_below_the_bar_passes() {
    let rows = [pair_row("a", "b", PairClass::Wrong, f64::next_down(AT))];
    assert_eq!(outcome(&rows, &[]), Outcome::Passes { relabels: 0 });
}

#[test]
fn a_different_split_pair_at_the_bar_fails() {
    let rows = [pair_row("a", "b", PairClass::DifferentSplit, AT)];
    assert_eq!(outcome(&rows, &[]), Outcome::Fails);
}

#[test]
fn no_wrong_pair_is_untested_not_a_pass() {
    let rows = [
        pair_row("a", "b", PairClass::True, 0.9),
        pair_row("c", "d", PairClass::True, 0.1),
    ];
    let judged = judge(&rows, AT, &[]).expect("no verdicts to refuse");
    assert_eq!(judged.outcome, Outcome::Untested);
    assert_eq!(judged.wrong, 0);
}

#[test]
fn true_and_damaged_pairs_never_fail_the_rule() {
    let rows = [
        pair_row("a", "b", PairClass::True, 0.1),
        pair_row("c", "d", PairClass::Damaged, AT),
        pair_row("e", "f", PairClass::Wrong, 0.1),
    ];
    assert_eq!(outcome(&rows, &[]), Outcome::Passes { relabels: 0 });
}

#[test]
fn a_same_verdict_relabels_and_is_counted() {
    let rows = [
        pair_row("a", "b", PairClass::Wrong, 0.1),
        pair_row("c", "d", PairClass::Wrong, AT),
    ];
    let judged = judge(&rows, AT, &[ruling("d", "c", true)]).expect("a verdict at the bar");
    assert_eq!(judged.outcome, Outcome::Passes { relabels: 1 });
    assert_eq!(judged.relabelled, [1]);
    assert_eq!(judged.at_bar, Vec::<usize>::new());
    assert_eq!(judged.wrong, 1, "the relabelled pair is no longer wrong");
}

#[test]
fn a_relabel_that_leaves_no_wrong_pair_is_untested() {
    let rows = [pair_row("a", "b", PairClass::Wrong, AT)];
    assert_eq!(outcome(&rows, &[ruling("a", "b", true)]), Outcome::Untested);
}

#[test]
fn a_different_verdict_keeps_the_failure() {
    for class in [PairClass::Wrong, PairClass::DifferentSplit] {
        let rows = [pair_row("a", "b", class, AT)];
        let judged = judge(&rows, AT, &[ruling("a", "b", false)]).expect("a verdict at the bar");
        assert_eq!(judged.outcome, Outcome::Fails, "{class:?}");
        assert_eq!(judged.at_bar, [0], "{class:?}");
        assert_eq!(judged.relabelled, Vec::<usize>::new(), "{class:?}");
    }
}

#[test]
fn a_verdict_below_the_bar_or_on_a_true_pair_is_refused() {
    let rows = [
        pair_row("low", "x", PairClass::Wrong, f64::next_down(AT)),
        pair_row("true", "x", PairClass::True, AT),
        pair_row("damaged", "x", PairClass::Damaged, AT),
        pair_row("split", "x", PairClass::DifferentSplit, AT),
    ];
    let cases = [
        ("below the bar", ruling("low", "x", false), "below the bar"),
        (
            "below the bar, same",
            ruling("low", "x", true),
            "below the bar",
        ),
        ("a true pair", ruling("true", "x", false), "True"),
        ("a damaged pair", ruling("damaged", "x", true), "Damaged"),
        (
            "a ruled split",
            ruling("split", "x", true),
            "different split",
        ),
        ("no member", ruling("elsewhere", "x", true), "elsewhere"),
    ];
    for (name, verdict, expected) in cases {
        let why = judge(&rows, AT, &[verdict]).expect_err(name);
        assert!(why.contains(expected), "{name}: {why}");
    }
}

#[test]
fn two_verdicts_on_one_pair_are_refused() {
    let rows = [pair_row("a", "b", PairClass::Wrong, AT)];
    let verdicts = [ruling("a", "b", false), ruling("b", "a", true)];
    let why = judge(&rows, AT, &verdicts).expect_err("the pair is ruled twice");
    assert!(why.contains("a and b"), "{why}");
}

const DEV_SCORES: &str = "the dev scores";
const HELD_OUT_SCORES: &str = "the held-out scores";
const CLEAN_STAMP: &str = "# score_volume_test 2026-11-18T00:00:00Z HEAD abc1234";

fn preflight_members() -> Vec<TestMember> {
    vec![
        test_member("Raw/T/T v01.cbz", "raw", 1),
        test_member("Volumes/T/T v01.cbz", "en", 1),
    ]
}

fn test_member(path: &str, edition: &str, number: u32) -> TestMember {
    TestMember {
        path: path.to_owned(),
        series: "tongari".to_owned(),
        edition: edition.to_owned(),
        number,
    }
}

fn frozen_over(members_json: &str, members: &[TestMember]) -> FrozenBar {
    FrozenBar {
        max_negative: 0.5,
        gap: GAP,
        bar: 0.5 + GAP,
        pair_a: "a.cbz".to_owned(),
        pair_b: "b.cbz".to_owned(),
        long_paired: 10,
        a_slots: 20,
        b_slots: 20,
        pr1_dev_sha256: sha256_hex(DEV_SCORES),
        pr1_held_out_sha256: sha256_hex(HELD_OUT_SCORES),
        members_sha256: sha256_hex(members_json),
        pairs_sha256: sha256_hex(&pairs_text(members)),
    }
}

/// A freeze whose scores derive a bar, and the text that bar freezes to.
fn freeze_fixture() -> (Freeze, String) {
    let m = manifest_of(&[("s/a.cbz", Kind::Volume), ("s/b.cbz", Kind::Volume)]);
    let row = Scored {
        a: 0,
        b: 1,
        source: Source::Explicit,
        label: label(&m, &m.members[0], &m.members[1]),
        features: control_features(),
        runs: 7,
        ms: 0.0,
    };
    let members = preflight_members();
    let freeze = Freeze {
        pr1_dev_scores: scores_tsv("# dev", &m, &[row]),
        pr1_held_out_scores: scores_tsv("# held-out", &m, &[]),
        members_json: serde_json::to_string(&members).expect("members serialise"),
        members,
        manifest: m,
    };
    let text = freeze.text().expect("a bar fits");
    (freeze, text)
}

/// A checkout where one commit froze `text` and nothing has happened since.
fn clean_git(text: &str) -> GitFacts {
    GitFacts {
        committed: Some(text.to_owned()),
        touching: Some("abc1234\n".to_owned()),
        commit: Some("abc1234 1795042514".to_owned()),
        status: Some(String::new()),
        later: Some(String::new()),
    }
}

fn bar_text(frozen: &FrozenBar) -> String {
    serde_json::to_string_pretty(frozen).expect("a bar serialises") + "\n"
}

#[test]
fn the_run_refuses_an_unfrozen_or_edited_bar() {
    let (freeze, bar) = freeze_fixture();
    let good = read_frozen(&bar).expect("the fixture's bar");
    let git = clean_git(&bar);
    let run = |compiled: Option<&str>, facts: &GitFacts, stamp: &str| {
        preflight(compiled, &freeze, facts, stamp)
    };

    let (frozen, provenance) = run(Some(&bar), &git, CLEAN_STAMP).expect("a frozen bar");
    assert_eq!(frozen, good);
    assert!(
        provenance.contains("committed") && provenance.contains("abc1234"),
        "{provenance}"
    );
    assert!(provenance.contains("1 commit touches it"), "{provenance}");

    let edited = bar_text(&FrozenBar {
        bar: 0.1,
        ..good.clone()
    });
    let wrong_members = bar_text(&FrozenBar {
        members_sha256: "x".to_owned(),
        ..good.clone()
    });
    let wrong_pairs = bar_text(&FrozenBar {
        pairs_sha256: "x".to_owned(),
        ..good.clone()
    });
    let wrong_dev = bar_text(&FrozenBar {
        pr1_dev_sha256: "x".to_owned(),
        ..good.clone()
    });
    let wrong_held = bar_text(&FrozenBar {
        pr1_held_out_sha256: "x".to_owned(),
        ..good.clone()
    });
    let uncommitted = GitFacts {
        committed: None,
        ..git.clone()
    };
    let twice = GitFacts {
        touching: Some("abc1234\ndef5678\n".to_owned()),
        ..git.clone()
    };
    let dirty = "# score_volume_test 2026-11-18T00:00:00Z HEAD abc1234+dirty";
    let unknown = "# score_volume_test 2026-11-18T00:00:00Z HEAD unknown";
    let cases = [
        ("no file", run(None, &git, CLEAN_STAMP), "bar.json"),
        (
            "edited since HEAD",
            run(Some(&edited), &git, CLEAN_STAMP),
            "differs",
        ),
        (
            "never committed",
            run(Some(&bar), &uncommitted, CLEAN_STAMP),
            "HEAD commits no",
        ),
        (
            "two commits",
            run(Some(&bar), &twice, CLEAN_STAMP),
            "2 commits",
        ),
        (
            "members",
            run(
                Some(&wrong_members),
                &clean_git(&wrong_members),
                CLEAN_STAMP,
            ),
            "members_sha256",
        ),
        (
            "pairs",
            run(Some(&wrong_pairs), &clean_git(&wrong_pairs), CLEAN_STAMP),
            "pairs_sha256",
        ),
        (
            "dev scores",
            run(Some(&wrong_dev), &clean_git(&wrong_dev), CLEAN_STAMP),
            "pr1_dev_sha256",
        ),
        (
            "held-out scores",
            run(Some(&wrong_held), &clean_git(&wrong_held), CLEAN_STAMP),
            "pr1_held_out_sha256",
        ),
        ("a dirty stamp", run(Some(&bar), &git, dirty), "+dirty"),
        (
            "an unknown stamp",
            run(Some(&bar), &git, unknown),
            "unknown",
        ),
    ];
    for (name, result, expected) in cases {
        let why = result.expect_err(name);
        assert!(why.contains(expected), "{name}: {why}");
    }
}

#[test]
fn a_bar_the_rule_does_not_derive_is_refused() {
    let (freeze, bar) = freeze_fixture();
    let good = read_frozen(&bar).expect("the fixture's bar");
    let forged = bar_text(&FrozenBar {
        max_negative: 0.84,
        bar: 0.84 + GAP,
        long_paired: 168,
        a_slots: 200,
        b_slots: 200,
        ..good.clone()
    });
    let reformatted = serde_json::to_string(&good).expect("a bar serialises");
    for text in [&forged, &reformatted] {
        let git = clean_git(text);
        let why = preflight(Some(text), &freeze, &git, CLEAN_STAMP).expect_err("the first run");
        assert!(why.contains("derive"), "{why}");
        let why = finished_bar(text, &freeze, &git).expect_err("a finished run");
        assert!(why.contains("derive"), "{why}");
    }
    assert_eq!(finished_bar(&bar, &freeze, &clean_git(&bar)), Ok(good));
}

#[test]
fn an_uncommitted_change_anywhere_stops_the_run() {
    let (freeze, bar) = freeze_fixture();
    let run = |status: Option<&str>| {
        let git = GitFacts {
            status: status.map(str::to_owned),
            ..clean_git(&bar)
        };
        preflight(Some(&bar), &freeze, &git, CLEAN_STAMP)
    };
    assert!(run(Some("")).is_ok());
    for change in [
        " M crates/taiyaku-serve/src/fingerprint.rs\n",
        "?? crates/taiyaku-read/src/new.rs\n",
    ] {
        let why = run(Some(change)).expect_err(change);
        assert!(why.contains(change.trim()), "{why}");
    }
    let why = run(None).expect_err("git could not answer");
    assert!(why.contains("git could not"), "{why}");
}

#[test]
fn code_committed_since_the_freeze_stops_the_first_run() {
    let (freeze, bar) = freeze_fixture();
    let run = |later: Option<&str>| {
        let git = GitFacts {
            later: later.map(str::to_owned),
            ..clean_git(&bar)
        };
        preflight(Some(&bar), &freeze, &git, CLEAN_STAMP)
    };
    assert!(run(Some("")).is_ok());
    for path in [
        "crates/taiyaku-serve/src/fingerprint.rs",
        "Cargo.toml",
        "Cargo.lock",
        VERDICTS_PATH,
    ] {
        let why = run(Some(&format!("{path}\n"))).expect_err(path);
        assert!(why.contains(path), "{why}");
    }
    let why = run(None).expect_err("git could not answer");
    assert!(why.contains("git could not"), "{why}");
}

#[test]
fn only_the_owners_verdicts_may_follow_the_freeze_into_a_finished_run() {
    let (freeze, bar) = freeze_fixture();
    let read = |later: Option<&str>| {
        let git = GitFacts {
            later: later.map(str::to_owned),
            ..clean_git(&bar)
        };
        finished_bar(&bar, &freeze, &git)
    };
    assert!(read(Some("")).is_ok());
    assert!(read(Some(&format!("{VERDICTS_PATH}\n\n{VERDICTS_PATH}\n"))).is_ok());
    for path in [
        "crates/taiyaku-cli/src/match_experiment/volume_test/run.rs",
        "Cargo.toml",
        "Cargo.lock",
    ] {
        let why = read(Some(&format!("{VERDICTS_PATH}\n{path}\n"))).expect_err(path);
        assert!(why.contains(path) && !why.contains(VERDICTS_PATH), "{why}");
    }
    let why = read(None).expect_err("git could not answer");
    assert!(why.contains("git could not"), "{why}");
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(repository.join(VERDICTS_PATH).is_file());
}

/// A repository laid out as this one is, whose first commit is the freeze.
struct ScratchRepo(TempDir);

impl ScratchRepo {
    const SERVE: &str = "crates/taiyaku-serve/src/lib.rs";

    fn new() -> Self {
        let repo = Self(TempDir::new("volume-test-repo"));
        repo.git(&["init", "-q", "-b", "main"]);
        let bar = format!("crates/taiyaku-cli/{BAR_FILE}");
        for path in [
            bar.as_str(),
            VERDICTS_PATH,
            Self::SERVE,
            "Cargo.toml",
            "Cargo.lock",
            "docs/notes.md",
        ] {
            repo.write(path, path);
        }
        repo.commit("the freeze");
        repo
    }

    fn output(dir: &Path, args: &[&str]) -> Option<String> {
        let out = Command::new("git")
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .args([
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.org",
            ])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .output()
            .expect("git runs");
        (out.status.success()).then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn git(&self, args: &[&str]) {
        let done = Self::output(self.0.path(), args);
        assert!(done.is_some(), "git {args:?}");
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.0.path().join(path);
        std::fs::create_dir_all(file.parent().expect("a directory")).expect("the directory");
        std::fs::write(file, text).expect("the file");
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// The facts as the harness asks for them, from the package's directory.
    fn facts(&self) -> GitFacts {
        let package = self.0.path().join("crates/taiyaku-cli");
        git_facts(|args| Self::output(&package, args))
    }

    fn later(&self) -> Vec<String> {
        let later = self.facts().later.expect("one commit froze the bar");
        later
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn git_is_asked_about_the_whole_repository_from_the_package() {
    let repo = ScratchRepo::new();
    let facts = repo.facts();
    assert_eq!(
        facts.touching.expect("the freeze").trim().lines().count(),
        1
    );
    assert_eq!(
        facts.committed.as_deref(),
        Some(format!("crates/taiyaku-cli/{BAR_FILE}").as_str())
    );
    assert!(facts.commit.is_some());
    assert_eq!(facts.status.as_deref(), Some(""));
    assert_eq!(repo.later(), Vec::<String>::new());

    repo.write("docs/notes.md", "the pre-registration");
    repo.commit("docs alone");
    assert_eq!(repo.later(), Vec::<String>::new(), "docs are not code");

    repo.git(&["config", "status.showUntrackedFiles", "no"]);
    repo.write(ScratchRepo::SERVE, "an edit outside the package");
    repo.write("docs/new.md", "untracked");
    let status = repo.facts().status.expect("the status");
    assert!(status.contains(ScratchRepo::SERVE), "{status}");
    assert!(status.contains("docs/new.md"), "{status}");
}

#[test]
fn every_code_path_a_later_commit_touches_is_listed() {
    for path in [
        ScratchRepo::SERVE,
        "Cargo.toml",
        "Cargo.lock",
        VERDICTS_PATH,
    ] {
        let repo = ScratchRepo::new();
        repo.write(path, "changed");
        repo.commit("later");
        assert_eq!(repo.later(), [path]);
    }
}

#[test]
fn a_merge_cannot_hide_the_code_it_brought_in() {
    let branch = |repo: &ScratchRepo, path: &str| {
        repo.git(&["checkout", "-q", "-b", "side"]);
        repo.write(path, "on the side");
        repo.commit("side");
        repo.git(&["checkout", "-q", "main"]);
        repo.write("docs/main.md", "on main");
        repo.commit("main");
    };

    let edited_in_the_merge = ScratchRepo::new();
    branch(&edited_in_the_merge, "docs/side.md");
    edited_in_the_merge.git(&["merge", "-q", "--no-commit", "--no-ff", "side"]);
    edited_in_the_merge.write(ScratchRepo::SERVE, "changed by the merge itself");
    edited_in_the_merge.commit("merge");
    assert!(
        edited_in_the_merge
            .later()
            .iter()
            .any(|p| p == ScratchRepo::SERVE),
        "{:?}",
        edited_in_the_merge.later()
    );

    let undone_on_the_side = ScratchRepo::new();
    undone_on_the_side.git(&["checkout", "-q", "-b", "side"]);
    undone_on_the_side.write(ScratchRepo::SERVE, "on the side");
    undone_on_the_side.commit("side");
    undone_on_the_side.write(ScratchRepo::SERVE, ScratchRepo::SERVE);
    undone_on_the_side.commit("undone");
    undone_on_the_side.git(&["checkout", "-q", "main"]);
    undone_on_the_side.write("docs/main.md", "on main");
    undone_on_the_side.commit("main");
    undone_on_the_side.git(&["merge", "-q", "--no-ff", "--no-edit", "side"]);
    assert!(
        undone_on_the_side
            .later()
            .iter()
            .any(|p| p == ScratchRepo::SERVE),
        "{:?}",
        undone_on_the_side.later()
    );

    let dropped_by_the_merge = ScratchRepo::new();
    branch(&dropped_by_the_merge, ScratchRepo::SERVE);
    dropped_by_the_merge.git(&["merge", "-q", "-s", "ours", "--no-edit", "side"]);
    assert!(
        dropped_by_the_merge
            .later()
            .iter()
            .any(|p| p == ScratchRepo::SERVE),
        "{:?}",
        dropped_by_the_merge.later()
    );
}

#[test]
fn code_moved_onto_the_verdicts_is_still_listed() {
    let repo = ScratchRepo::new();
    repo.git(&["rm", "-q", VERDICTS_PATH]);
    repo.commit("the verdicts go");
    repo.git(&["mv", ScratchRepo::SERVE, VERDICTS_PATH]);
    repo.commit("code takes their place");
    assert!(
        repo.later().iter().any(|p| p == ScratchRepo::SERVE),
        "{:?}",
        repo.later()
    );
}

#[test]
fn the_run_refuses_a_used_fingerprint_cache() {
    let dir = TempDir::new("volume-test-cache");
    assert_eq!(cache_is_unused(&dir.path().join("absent")), Ok(()));
    assert_eq!(cache_is_unused(dir.path()), Ok(()));
    std::fs::write(dir.path().join("entry"), b"x").expect("a cache entry");
    let why = cache_is_unused(dir.path()).expect_err("a cache holding an entry");
    assert!(why.contains("entry"), "{why}");
}

fn frozen_at(bar: f64) -> FrozenBar {
    FrozenBar {
        max_negative: bar - GAP,
        bar,
        ..frozen_over("[]", &[])
    }
}

/// A member's archive. Two cut from one seed share their first pages, and a
/// different page count keeps their bytes apart, since the fingerprint cache
/// answers for any two members of the same bytes.
fn archive(library: &Path, path: &str, pages: u32, seed: u32) {
    let (dir, name) = path.rsplit_once('/').expect("a path with a directory");
    let dir = library.join(dir);
    std::fs::create_dir_all(&dir).expect("the member's directory");
    cbz_pages(&dir, name, pages, seed);
}

fn run_log(ev: &Path) -> Log {
    Log::create(&ev.join("logs/run.log"), "# fixture")
}

#[test]
fn an_unreadable_member_stops_the_run_and_writes_no_report() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = [
        test_member("Raw/T/T v01.cbz", "raw", 1),
        test_member("Volumes/T/T v01.cbz", "en", 1),
    ];
    archive(library.path(), &members[0].path, 6, 1);
    zip_member(
        library.path(),
        &members[1].path,
        &[(
            "000.png",
            b"lists as a page and decodes as nothing".to_vec(),
        )],
    );

    let mut log = run_log(ev.path());
    let why = score(
        library.path(),
        ev.path(),
        &members,
        &frozen_at(0.85),
        &[],
        &mut log,
    )
    .expect_err("a member cannot be read");
    assert!(why.contains(&members[1].path), "{why}");
    assert!(!ev.path().join("report.txt").exists());
    assert!(!ev.path().join("report-first.txt").exists());
    assert!(!ev.path().join("scores.tsv").exists(), "no pair is scored");
    let table = std::fs::read_to_string(ev.path().join("members.tsv")).expect("members.tsv");
    assert!(table.contains(&members[1].path), "{table}");
}

#[test]
fn a_fingerprint_answered_from_the_cache_stops_the_run_and_writes_no_report() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = [
        test_member("Raw/T/T v01.cbz", "raw", 1),
        test_member("Volumes/T/T v01.cbz", "en", 1),
    ];
    archive(library.path(), &members[0].path, 6, 1);
    archive(library.path(), &members[1].path, 6, 1);

    let mut log = run_log(ev.path());
    let why = score(
        library.path(),
        ev.path(),
        &members,
        &frozen_at(0.85),
        &[],
        &mut log,
    )
    .expect_err("the second member's bytes are the first's");
    assert!(
        why.contains("cache") && why.contains(&members[1].path),
        "{why}"
    );
    assert!(!ev.path().join("scores.tsv").exists(), "no pair is scored");
    assert!(!ev.path().join("report.txt").exists());
    assert!(!ev.path().join("report-first.txt").exists());
}

/// A member of the one edition pair the rulings hold damaged.
fn hunter_member(path: &str, edition: &str) -> TestMember {
    TestMember {
        path: path.to_owned(),
        series: "hunter-x-hunter".to_owned(),
        edition: edition.to_owned(),
        number: 35,
    }
}

/// The lines under a heading, up to the next heading or the verdict.
fn under<'r>(report: &'r str, heading: &str) -> Vec<&'r str> {
    let mut lines = report.lines().skip_while(|line| *line != heading);
    assert_eq!(lines.next(), Some(heading), "no heading in:\n{report}");
    lines
        .take_while(|line| !line.starts_with("## ") && !line.starts_with("verdict: "))
        .filter(|line| !line.is_empty())
        .collect()
}

fn scored_row_of(ev: &Path, a: &str, b: &str) -> PairRow {
    let text = std::fs::read_to_string(ev.join("scores.tsv")).expect("scores.tsv");
    let (_, rows) = read_scores(&text).expect("the run's own table");
    rows.into_iter()
        .find(|row| row.a == a && row.b == b)
        .unwrap_or_else(|| panic!("no row for {a} and {b}"))
}

#[test]
fn the_report_lists_each_true_pair_and_each_damaged_pair() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = [
        test_member("Raw/T/T v01.cbz", "raw", 1),
        test_member("Volumes/T/T v01.cbz", "en", 1),
        hunter_member("Colour/H/H v35.cbz", "colour"),
        hunter_member("Volumes/H/H v35.cbz", "bw"),
    ];
    archive(library.path(), &members[0].path, 10, 100);
    archive(library.path(), &members[1].path, 6, 100);
    archive(library.path(), &members[2].path, 10, 7000);
    archive(library.path(), &members[3].path, 9, 7000);

    let mut log = run_log(ev.path());
    let judged = score(
        library.path(),
        ev.path(),
        &members,
        &frozen_at(0.85),
        &[],
        &mut log,
    )
    .expect("every member reads");
    assert_eq!(judged.outcome, Outcome::Passes { relabels: 0 });

    let report = std::fs::read_to_string(ev.path().join("report.txt")).expect("report.txt");
    let (short, long) = ("Volumes/T/T v01.cbz", "Raw/T/T v01.cbz");
    let t = scored_row_of(ev.path(), short, long);
    assert_eq!((t.long_paired, t.a_slots, t.b_slots), (6, 6, 10));
    assert_eq!(t.edition_kind, "tongari raw-en");
    assert_eq!(
        under(&report, "## true pairs below the bar: 1"),
        [format!(
            "{short} and {long}: True, V 0.600000, long_paired 6, slots 6 and 10, verdict {}, \
             separation {}",
            t.verdict, t.separation
        )]
    );
    let d = scored_row_of(ev.path(), "Volumes/H/H v35.cbz", "Colour/H/H v35.cbz");
    assert_eq!(
        under(&report, "## damaged pairs, apart from the true pairs: 1"),
        [format!(
            "Volumes/H/H v35.cbz and Colour/H/H v35.cbz: Damaged, V 0.900000, long_paired 9, \
             slots 9 and 10, verdict {}, separation {}",
            d.verdict, d.separation
        )]
    );
    let clearing = under(&report, "## true pairs clearing the bar");
    assert_eq!(
        clearing,
        [
            "true pairs at or above the bar: 0 of 1",
            "  tongari raw-en: 0 of 1",
            "  ruridragon raw-en: 0 of 0",
            "  hunter-x-hunter bw-colour: 0 of 0",
            "  parasyte bw-collection: 0 of 0",
        ]
    );
    assert_eq!(
        under(&report, "## pairs by class"),
        ["true: 1", "damaged: 1", "different split: 0", "wrong: 4"]
    );
    assert!(report.starts_with("# score_volume_test "), "{report}");
    assert!(
        report
            .lines()
            .any(|line| line == "frozen bar: 0.85 (exactly 0.85)")
    );
    let headings: Vec<&str> = (report.lines())
        .filter(|line| line.starts_with("## "))
        .collect();
    assert_eq!(
        headings,
        [
            "## pairs by class",
            "## the highest wrong pair",
            "## wrong pairs at or above the bar: 0 of 4",
            "## verdicts: 0, relabels: 0",
            "## true pairs clearing the bar",
            "## true pairs below the bar: 1",
            "## damaged pairs, apart from the true pairs: 1",
            "## the highest different-split pairs",
        ]
    );
    assert_eq!(report.lines().last(), Some("verdict: passes"));
    let first = std::fs::read_to_string(ev.path().join("report-first.txt")).expect("the copy");
    assert_eq!(first, report);
}

/// Two copies of one volume under different numbers, so their pair is a wrong
/// pair at any bar, beside an unrelated volume that keeps a wrong pair low.
fn wrong_pair_at_the_bar(library: &Path) -> [TestMember; 3] {
    let members = [
        test_member("Raw/T/T v01.cbz", "raw", 1),
        test_member("Raw/T/T v02.cbz", "raw", 2),
        test_member("Raw/T/T v03.cbz", "raw", 3),
    ];
    archive(library, &members[0].path, 10, 100);
    archive(library, &members[1].path, 9, 100);
    archive(library, &members[2].path, 10, 7000);
    members
}

#[test]
fn rejudge_keeps_the_first_report() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = wrong_pair_at_the_bar(library.path());
    let frozen = frozen_at(0.85);
    let mut log = run_log(ev.path());
    let first = score(library.path(), ev.path(), &members, &frozen, &[], &mut log)
        .expect("every member reads");
    assert_eq!(first.outcome, Outcome::Fails);
    let before = std::fs::read(ev.path().join("report-first.txt")).expect("the first report");
    assert_eq!(
        before,
        std::fs::read(ev.path().join("report.txt")).expect("report.txt")
    );

    let verdicts = [ruling("Raw/T/T v01.cbz", "Raw/T/T v02.cbz", true)];
    let judged: Judged =
        rejudge(ev.path(), &frozen, &members, &verdicts).expect("the verdict stands");
    assert_eq!(judged.outcome, Outcome::Passes { relabels: 1 });

    let report = std::fs::read_to_string(ev.path().join("report.txt")).expect("report.txt");
    assert_eq!(
        report.lines().last(),
        Some("verdict: passes after 1 relabels")
    );
    assert_eq!(
        under(&report, "## verdicts: 1, relabels: 1"),
        [
            "Raw/T/T v01.cbz and Raw/T/T v02.cbz: same, relabelled (owner, 2026-11-18): looked at \
          the pages"
        ]
    );
    let after = std::fs::read(ev.path().join("report-first.txt")).expect("the first report");
    assert_eq!(after, before, "the first report is never rewritten");
    let first_text = String::from_utf8(after).expect("the report is text");
    assert_eq!(first_text.lines().last(), Some("verdict: fails"));
}

#[test]
fn rejudge_refuses_a_row_whose_printed_v_disagrees() {
    let ev = TempDir::new("volume-test-ev");
    let row = PairRow {
        long_paired: 5,
        v: volume_score(5, 10, 10),
        ..pair_row("a", "b", PairClass::Wrong, 0.0)
    };
    let table = scores_text("# stamp", std::slice::from_ref(&row));
    assert_eq!(read_scores(&table), Ok(("# stamp".to_owned(), vec![row])));

    let edited = table.replace("0.500000", "0.400000");
    assert_ne!(edited, table);
    leave_table(ev.path(), &edited);
    let members = [test_member("a", "raw", 1), test_member("b", "raw", 2)];
    let why =
        rejudge(ev.path(), &frozen_at(0.85), &members, &[]).expect_err("V is not the integers' V");
    assert!(
        why.contains("0.400000") && why.contains("0.500000"),
        "{why}"
    );
    assert!(!ev.path().join("report.txt").exists());
}

#[test]
fn sheets_are_written_for_the_wrong_pairs_no_one_has_ruled_on() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = wrong_pair_at_the_bar(library.path());
    let frozen = frozen_at(0.85);
    let mut log = run_log(ev.path());
    score(library.path(), ev.path(), &members, &frozen, &[], &mut log).expect("every member reads");

    let written = sheets(library.path(), ev.path(), &members, &frozen, &[]).expect("one sheet");
    assert_eq!(written, 1);
    let sheet = ev.path().join("sheets/pair-01.html");
    let html = std::fs::read_to_string(&sheet).expect("the pair's sheet");
    assert!(html.contains("Raw/T/T v01.cbz") && html.contains("Raw/T/T v02.cbz"));
    std::fs::remove_file(&sheet).expect("the sheet");

    let verdicts = [ruling("Raw/T/T v01.cbz", "Raw/T/T v02.cbz", false)];
    let written = sheets(library.path(), ev.path(), &members, &frozen, &verdicts).expect("none");
    assert_eq!(written, 0);
    assert!(!sheet.exists(), "a ruled pair is not asked again");
}

#[test]
fn a_freeze_writes_what_the_run_then_accepts() {
    let (freeze, text) = freeze_fixture();
    let (frozen, _) = preflight(Some(&text), &freeze, &clean_git(&text), CLEAN_STAMP)
        .expect("the run accepts what the freeze wrote");
    assert_eq!(frozen.max_negative, 0.4);
    assert_eq!(frozen.bar, 0.4 + GAP);
    assert_eq!(frozen.gap, GAP);
    assert_eq!(
        (frozen.pair_a.as_str(), frozen.pair_b.as_str()),
        ("s/a.cbz", "s/b.cbz")
    );
    assert_eq!(
        (frozen.long_paired, frozen.a_slots, frozen.b_slots),
        (2, 3, 5)
    );
}

#[test]
fn a_finished_run_is_read_only_against_a_bar_one_commit_froze() {
    let good = frozen_over("[]", &[]);
    let text = serde_json::to_string(&good).expect("a bar serialises");
    let moved = serde_json::to_string(&FrozenBar {
        bar: 0.1,
        ..good.clone()
    })
    .expect("a bar serialises");

    let (frozen, touched) =
        settled_bar(&text, Some(&text), Some("abc1234\n")).expect("one commit froze it");
    assert_eq!(frozen, good);
    assert!(touched.contains("1 commit touches it"), "{touched}");

    let why = settled_bar(&moved, Some(&moved), Some("def5678\nabc1234\n"))
        .expect_err("a later commit moved the bar, and HEAD agrees with the moved text");
    assert!(why.contains("2 commits"), "{why}");
    let cases = [
        (
            "edited since HEAD",
            settled_bar(&moved, Some(&text), Some("abc1234\n")),
            "differs",
        ),
        (
            "never committed",
            settled_bar(&text, None, Some("abc1234\n")),
            "HEAD commits no",
        ),
        (
            "no commit",
            settled_bar(&text, Some(&text), Some("")),
            "no commit touches",
        ),
        (
            "git failed",
            settled_bar(&text, Some(&text), None),
            "could not list",
        ),
    ];
    for (name, result, expected) in cases {
        let why = result.expect_err(name);
        assert!(why.contains(expected), "{name}: {why}");
    }
}

/// `text` as the table a run left, with its digest recorded.
fn leave_table(ev: &Path, text: &str) {
    std::fs::write(ev.join("scores.tsv"), text).expect("the table");
    std::fs::write(
        ev.join("scores.sha256"),
        format!("{}  scores.tsv\n", sha256_hex(text)),
    )
    .expect("the digest");
}

#[test]
fn a_table_that_lost_or_reclassified_a_pair_is_not_judged() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = wrong_pair_at_the_bar(library.path());
    let frozen = frozen_at(0.85);
    let mut log = run_log(ev.path());
    let first = score(library.path(), ev.path(), &members, &frozen, &[], &mut log)
        .expect("every member reads");
    assert_eq!(first.outcome, Outcome::Fails);
    let table = std::fs::read_to_string(ev.path().join("scores.tsv")).expect("the table");
    let caught = (table.lines())
        .find(|line| line.contains("T v01.cbz") && line.contains("T v02.cbz"))
        .expect("the wrong pair's row")
        .to_owned();
    let report = std::fs::read(ev.path().join("report.txt")).expect("the report");

    let cases = [
        (
            "lost",
            table.replace(&format!("{caught}\n"), ""),
            "not in the table",
        ),
        (
            "reclassified",
            table.replace(&caught, &caught.replace("\tWrong\t", "\tDamaged\t")),
            "classed Damaged",
        ),
        ("doubled", format!("{table}{caught}\n"), "twice"),
        (
            "a stranger",
            table.replace("T v03.cbz", "T v04.cbz"),
            "T v04.cbz",
        ),
        (
            "a self-pair",
            format!("{table}{}\n", caught.replace("T v02.cbz", "T v01.cbz")),
            "against itself",
        ),
    ];
    for (name, text, expected) in cases {
        leave_table(ev.path(), &text);
        let why = rejudge(ev.path(), &frozen, &members, &[]).expect_err(name);
        assert!(why.contains(expected), "{name}: {why}");
        let why = sheets(library.path(), ev.path(), &members, &frozen, &[]).expect_err(name);
        assert!(why.contains(expected), "{name}: {why}");
    }
    assert_eq!(
        std::fs::read(ev.path().join("report.txt")).expect("the report"),
        report,
        "a refused table rewrites nothing"
    );
}

#[test]
fn a_table_naming_the_wrong_edition_kind_is_not_judged() {
    let ev = TempDir::new("volume-test-ev");
    let members = preflight_members();
    let row = PairRow {
        v: volume_score(0, 10, 10),
        ..pair_row(&members[0].path, &members[1].path, PairClass::True, 0.0)
    };
    leave_table(
        ev.path(),
        &scores_text("# stamp", std::slice::from_ref(&row)),
    );
    let why = rejudge(ev.path(), &frozen_at(0.85), &members, &[]).expect_err("the kind is `-`");
    assert!(why.contains("tongari raw-en"), "{why}");
}

#[test]
fn a_table_the_run_did_not_leave_is_not_judged() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = wrong_pair_at_the_bar(library.path());
    let frozen = frozen_at(0.85);
    let mut log = run_log(ev.path());
    score(library.path(), ev.path(), &members, &frozen, &[], &mut log).expect("every member reads");
    let table = std::fs::read_to_string(ev.path().join("scores.tsv")).expect("the table");

    std::fs::write(ev.path().join("scores.tsv"), format!("{table}\n")).expect("an edit");
    let why = rejudge(ev.path(), &frozen, &members, &[]).expect_err("the digest differs");
    assert!(why.contains("scores.sha256"), "{why}");
    let why = sheets(library.path(), ev.path(), &members, &frozen, &[]).expect_err("the digest");
    assert!(why.contains("scores.sha256"), "{why}");

    std::fs::write(ev.path().join("scores.tsv"), &table).expect("the table again");
    rejudge(ev.path(), &frozen, &members, &[]).expect("the run's own table");
    std::fs::remove_file(ev.path().join("scores.sha256")).expect("the digest");
    let why = rejudge(ev.path(), &frozen, &members, &[]).expect_err("no digest");
    assert!(why.contains("scores.sha256"), "{why}");
}

#[test]
fn a_second_scoring_is_refused_before_any_page_is_read() {
    for name in [
        "scores.tsv",
        "scores.sha256",
        "report.txt",
        "report-first.txt",
    ] {
        let (library, ev) = (
            TempDir::new("volume-test-lib"),
            TempDir::new("volume-test-ev"),
        );
        let members = wrong_pair_at_the_bar(library.path());
        std::fs::write(ev.path().join(name), "the first run's").expect("a file of the first run");

        let mut log = run_log(ev.path());
        let why = score(
            library.path(),
            ev.path(),
            &members,
            &frozen_at(0.85),
            &[],
            &mut log,
        )
        .expect_err(name);
        assert!(why.contains(name), "{why}");
        assert_eq!(
            std::fs::read_to_string(ev.path().join(name)).expect("the file"),
            "the first run's"
        );
        assert!(
            !ev.path().join("fp-cache").exists(),
            "{name}: a page was read"
        );
        assert!(!ev.path().join("members.tsv").exists(), "{name}");
    }
}

#[test]
fn a_verdict_before_the_first_report_is_refused() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = wrong_pair_at_the_bar(library.path());
    let verdicts = [ruling("Raw/T/T v01.cbz", "Raw/T/T v02.cbz", true)];
    let mut log = run_log(ev.path());
    let why = score(
        library.path(),
        ev.path(),
        &members,
        &frozen_at(0.85),
        &verdicts,
        &mut log,
    )
    .expect_err("a verdict came before the report");
    assert!(why.contains("verdicts.json"), "{why}");
    assert!(!ev.path().join("fp-cache").exists(), "a page was read");
    assert!(!ev.path().join("report-first.txt").exists());
}

/// A member's archive holding exactly these entries.
fn zip_member(library: &Path, path: &str, entries: &[(&str, Vec<u8>)]) {
    let file = library.join(path);
    std::fs::create_dir_all(file.parent().expect("a directory")).expect("the directory");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).expect("the archive"));
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("an entry");
        zip.write_all(bytes).expect("the entry's bytes");
    }
    zip.finish().expect("the archive closes");
}

/// Six pages the reader decodes, then `extra`.
fn pages_and(extra: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
    let pages = (0..6u32).map(|i| {
        let page = print_page(64, 64, [0, 0, 0, 0], i * 50);
        (format!("{i:03}.png"), page)
    });
    pages
        .chain(
            extra
                .iter()
                .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec())),
        )
        .collect()
}

fn zip_of(library: &Path, path: &str, entries: &[(String, Vec<u8>)]) {
    let named: Vec<(&str, Vec<u8>)> = (entries.iter())
        .map(|(name, bytes)| (name.as_str(), bytes.clone()))
        .collect();
    zip_member(library, path, &named);
}

const MEMBER: &str = "Raw/T/T v01.cbz";

fn left_out_of(library: &Path) -> Result<Vec<usize>, String> {
    left_out(library, &[test_member(MEMBER, "raw", 1)])
}

#[test]
fn an_entry_that_may_be_a_page_is_refused_by_name_or_by_its_bytes() {
    let text: &[u8] = b"nothing an image opens with";
    let cases: [(&str, &[u8]); 20] = [
        ("006.heic", text),
        ("006.HEIC", text),
        ("006.heif", text),
        ("006.jp2", text),
        ("006.jfif", text),
        ("nested.cbz", text),
        ("nested.rar", text),
        ("a.bin", b"\xFF\xD8\xFF\xE0 jpeg"),
        ("b.bin", b"\x89PNG\r\n\x1a\n"),
        ("c.bin", b"GIF89a"),
        ("d.bin", b"BM bitmap"),
        ("e.bin", b"II*\0 tiff"),
        ("f.bin", b"MM\0* tiff"),
        ("g.bin", b"RIFF\0\0\0\0WEBPVP8 "),
        ("h.bin", b"\xFF\x0A jxl"),
        ("i.bin", b"\0\0\0\x0CJXL \r\n\x87\n"),
        ("j.bin", b"\0\0\0\x0CjP  \r\n\x87\n"),
        ("k.bin", b"\xFF\x4F\xFF\x51"),
        ("l.bin", b"\0\0\0\x18ftypheic"),
        ("m", b"\0\0\0\x18ftypavif"),
    ];
    for (name, bytes) in cases {
        let library = TempDir::new("volume-test-lib");
        zip_of(library.path(), MEMBER, &pages_and(&[(name, bytes)]));
        let why = left_out_of(library.path()).expect_err(name);
        assert!(why.contains(&format!("{MEMBER}: {name}")), "{why}");
    }
}

#[test]
fn a_locked_entry_is_refused_whatever_it_is_called() {
    for name in ["006.png", "notes.bin"] {
        let library = TempDir::new("volume-test-lib");
        let file = library.path().join(MEMBER);
        std::fs::create_dir_all(file.parent().expect("a directory")).expect("the directory");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).expect("the archive"));
        let plain = zip::write::SimpleFileOptions::default();
        zip.start_file("000.png", plain).expect("a page");
        zip.write_all(&print_page(64, 64, [0, 0, 0, 0], 1))
            .expect("the page");
        let locked = plain.with_aes_encryption(zip::AesMode::Aes256, "password");
        zip.start_file(name, locked).expect("a locked entry");
        zip.write_all(b"unseen").expect("its bytes");
        zip.finish().expect("the archive closes");
        let why = left_out_of(library.path()).expect_err(name);
        assert!(why.contains(name), "{why}");
    }
}

#[test]
fn entries_that_are_not_images_are_not_refused() {
    let library = TempDir::new("volume-test-lib");
    let entries = pages_and(&[
        ("ComicInfo.xml", b"<ComicInfo/>"),
        ("__MACOSX/._000.png", b"\x89PNG resource fork"),
        (".DS_Store", b"\0\0\0\x01Bud1"),
        ("credits.dat", b"plain words"),
    ]);
    zip_of(library.path(), MEMBER, &entries);
    assert_eq!(left_out_of(library.path()), Ok(vec![1]));
}

#[test]
fn a_directory_member_is_held_to_the_same_rule() {
    let loose = |extra: &str, bytes: &[u8]| {
        let library = TempDir::new("volume-test-lib");
        let dir = library.path().join(MEMBER);
        std::fs::create_dir_all(&dir).expect("the member");
        std::fs::write(dir.join("000.png"), print_page(64, 64, [0, 0, 0, 0], 1)).expect("a page");
        std::fs::write(dir.join(extra), bytes).expect("the entry");
        left_out_of(library.path())
    };
    assert_eq!(loose("ComicInfo.xml", b"<ComicInfo/>"), Ok(vec![0]));
    assert_eq!(loose("credits.dat", b"plain words"), Ok(vec![1]));
    let why = loose("001.heic", b"pixels").expect_err("a page the reader skips");
    assert!(why.contains("001.heic"), "{why}");
    let why = loose("001.bin", b"\xFF\xD8\xFF\xE0").expect_err("a jpeg by its bytes");
    assert!(why.contains("001.bin"), "{why}");

    let archives = |stray: Option<&str>, inner: &[(&str, &[u8])]| {
        let library = TempDir::new("volume-test-lib");
        zip_of(
            library.path(),
            &format!("{MEMBER}/a.cbz"),
            &pages_and(inner),
        );
        if let Some(stray) = stray {
            let file = library.path().join(MEMBER).join(stray);
            std::fs::create_dir_all(file.parent().expect("a directory")).expect("the directory");
            std::fs::write(file, b"pixels").expect("a stray file");
        }
        left_out_of(library.path())
    };
    assert_eq!(archives(None, &[]), Ok(vec![0]));
    let why = archives(None, &[("006.heic", b"pixels")]).expect_err("inside an archive");
    assert!(why.contains("006.heic"), "{why}");
    let why = archives(Some("006.heic"), &[]).expect_err("beside the archives");
    assert!(why.contains("006.heic"), "{why}");
    let why = archives(Some("sub/006.heic"), &[]).expect_err("below the archives");
    assert!(why.contains("sub/006.heic"), "{why}");
}

#[test]
fn a_member_that_will_not_open_is_refused_before_any_page_is_read() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = preflight_members();
    archive(library.path(), &members[0].path, 6, 1);
    let corrupt = library.path().join(&members[1].path);
    std::fs::create_dir_all(corrupt.parent().expect("a directory")).expect("the directory");
    std::fs::write(&corrupt, b"not an archive").expect("a corrupt member");

    let mut log = run_log(ev.path());
    let why = score(
        library.path(),
        ev.path(),
        &members,
        &frozen_at(0.85),
        &[],
        &mut log,
    )
    .expect_err("a member cannot be listed");
    assert!(why.contains(&members[1].path), "{why}");
    assert!(!ev.path().join("fp-cache").exists(), "a page was read");
}

#[test]
fn the_members_table_counts_what_was_passed_over() {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let members = preflight_members();
    let extra: [(&str, &[u8]); 2] = [
        ("ComicInfo.xml", b"<ComicInfo/>"),
        ("credits.dat", b"plain words"),
    ];
    zip_of(library.path(), &members[0].path, &pages_and(&extra));
    archive(library.path(), &members[1].path, 6, 100);

    let mut log = run_log(ev.path());
    score(
        library.path(),
        ev.path(),
        &members,
        &frozen_at(0.85),
        &[],
        &mut log,
    )
    .expect("nothing that may be a page was passed over");
    let table = std::fs::read_to_string(ev.path().join("members.tsv")).expect("members.tsv");
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(
        lines[1],
        "path\tkind\tpages\tslots\tbytes\tms\tcache\terror\tskipped"
    );
    let cells = |line: &str| line.split('\t').map(str::to_owned).collect::<Vec<_>>();
    let (first, second) = (cells(lines[2]), cells(lines[3]));
    assert_eq!(
        (first[0].as_str(), first[2].as_str()),
        (members[0].path.as_str(), "6")
    );
    assert_eq!(first.last().map(String::as_str), Some("1"));
    assert_eq!(second.last().map(String::as_str), Some("0"));
    assert_eq!(lines.len(), 4);
}

/// The report over hand-made rows held to [`AT`].
fn report_of(rows: &[PairRow]) -> String {
    let judged = judge(rows, AT, &[]).expect("no verdicts to refuse");
    report_text("# fixture\n", &frozen_at(AT), rows, &[], &judged)
}

#[test]
fn the_report_names_the_highest_wrong_pair_wherever_it_sits() {
    for scores in [[0.6, 0.3, 0.2], [0.3, 0.6, 0.2], [0.2, 0.3, 0.6]] {
        let rows: Vec<PairRow> = (scores.iter().enumerate())
            .map(|(i, &v)| pair_row(&format!("a{i}"), "b", PairClass::Wrong, v))
            .collect();
        let highest = rows.iter().find(|row| row.v == 0.6).expect("the highest");
        let report = report_of(&rows);
        assert_eq!(
            under(&report, "## the highest wrong pair"),
            [format!(
                "{} and b: Wrong, V 0.600000, long_paired 0, slots 10 and 10, verdict Sound, \
                 separation -",
                highest.a
            )]
        );
    }
}

#[test]
fn the_report_lists_the_highest_different_splits_first() {
    let rows: Vec<PairRow> = (0..12u32)
        .map(|i| {
            let v = f64::from(i) / 100.0;
            pair_row(&format!("s{i:02}"), "b", PairClass::DifferentSplit, v)
        })
        .collect();
    let report = report_of(&rows);
    let named: Vec<&str> = (under(&report, "## the highest different-split pairs").iter())
        .map(|line| line.split(' ').next().expect("a pair's first name"))
        .collect();
    assert_eq!(
        named,
        [
            "s11", "s10", "s09", "s08", "s07", "s06", "s05", "s04", "s03", "s02"
        ]
    );
}

#[test]
fn a_true_pair_exactly_at_the_bar_clears_it_on_its_kinds_own_line() {
    let kinded = |a: &str, kind: &str, v| PairRow {
        edition_kind: kind.to_owned(),
        ..pair_row(a, "b", PairClass::True, v)
    };
    let rows = [
        kinded("t1", "tongari raw-en", AT),
        kinded("t2", "tongari raw-en", f64::next_down(AT)),
        kinded("r1", "ruridragon raw-en", AT),
        kinded("h1", "hunter-x-hunter bw-colour", 0.1),
        kinded("p1", "parasyte bw-collection", AT),
    ];
    let report = report_of(&rows);
    assert_eq!(
        under(&report, "## true pairs clearing the bar"),
        [
            "true pairs at or above the bar: 3 of 5",
            "  tongari raw-en: 1 of 2",
            "  ruridragon raw-en: 1 of 1",
            "  hunter-x-hunter bw-colour: 0 of 1",
            "  parasyte bw-collection: 1 of 1",
        ]
    );
    assert_eq!(under(&report, "## true pairs below the bar: 2").len(), 2);
}

#[test]
fn the_four_edition_kinds_hold_every_true_pair() {
    let m = committed();
    let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
    for (i, j) in all_pairs(m.len()) {
        if pair_class(&m[i], &m[j]) == PairClass::True {
            *kinds.entry(edition_kind(&m[i], &m[j])).or_default() += 1;
        }
    }
    let counted: Vec<(&str, u32)> = kinds.iter().map(|(k, n)| (k.as_str(), *n)).collect();
    assert_eq!(
        counted,
        [
            ("hunter-x-hunter bw-colour", 35),
            ("parasyte bw-collection", 8),
            ("ruridragon raw-en", 3),
            ("tongari raw-en", 13),
        ]
    );
}

/// A real run's fixture: a library, an evidence directory and a sound freeze.
struct RealRun {
    _dirs: (TempDir, TempDir),
    material: Material,
    freeze: Freeze,
    text: String,
}

fn real_run() -> RealRun {
    let (library, ev) = (
        TempDir::new("volume-test-lib"),
        TempDir::new("volume-test-ev"),
    );
    let (freeze, text) = freeze_fixture();
    archive(library.path(), &freeze.members[0].path, 10, 100);
    archive(library.path(), &freeze.members[1].path, 6, 100);
    let material = Material {
        library: library.path().to_path_buf(),
        evidence: ev.path().to_path_buf(),
    };
    RealRun {
        _dirs: (library, ev),
        material,
        freeze,
        text,
    }
}

impl RealRun {
    fn measured(
        &self,
        verdicts: &[Adjudication],
        control: impl FnOnce(&mut Log) -> Result<(), String>,
    ) -> Result<Judged, String> {
        measure(
            &self.material,
            Some(&self.text),
            &self.freeze,
            &clean_git(&self.text),
            CLEAN_STAMP,
            verdicts,
            control,
        )
    }

    fn evidence(&self, name: &str) -> PathBuf {
        self.material.evidence.join(name)
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.evidence("logs/volume-test.log")).expect("the run's log")
    }
}

#[test]
fn the_real_run_takes_its_steps_in_order() {
    let run = real_run();
    let cache = run.evidence("fp-cache");
    let mut controls = 0;
    let judged = run
        .measured(&[], |log| {
            controls += 1;
            assert!(!cache.exists(), "a test page was read before the control");
            log.line("the control ran");
            Ok(())
        })
        .expect("the run finishes");
    assert_eq!(controls, 1);
    assert_eq!(judged.outcome, Outcome::Untested);

    let log = run.log();
    let at = |text: &str| {
        log.find(text)
            .unwrap_or_else(|| panic!("{text} in:\n{log}"))
    };
    let steps = [
        at(CLEAN_STAMP),
        at("bar.json: 1 commit touches it"),
        at("frozen over: "),
        at("the control ran"),
        at("members: 2, fingerprinted: 2, unreadable: 0, cache hits: 0"),
        at("verdict: untested"),
    ];
    assert!(steps.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
    assert_eq!(log.lines().next(), Some(CLEAN_STAMP));
}

#[test]
fn a_refused_run_never_reaches_the_control_or_the_log() {
    let refused = |run: &RealRun, result: Result<Judged, String>, controls: u32, expected: &str| {
        let why = result.expect_err(expected);
        assert!(why.contains(expected), "{why}");
        assert_eq!(controls, 0, "{expected}: the control ran");
        assert!(!run.evidence("logs/volume-test.log").exists(), "{expected}");
    };

    let run = real_run();
    let mut controls = 0;
    let result = measure(
        &run.material,
        None,
        &run.freeze,
        &clean_git(&run.text),
        CLEAN_STAMP,
        &[],
        |_| {
            controls += 1;
            Ok(())
        },
    );
    refused(&run, result, controls, "bar.json does not exist");

    let run = real_run();
    std::fs::create_dir_all(run.evidence("fp-cache")).expect("the cache");
    std::fs::write(run.evidence("fp-cache/entry"), b"x").expect("an entry");
    let mut controls = 0;
    let result = run.measured(&[], |_| {
        controls += 1;
        Ok(())
    });
    refused(
        &run,
        result,
        controls,
        "fingerprints would not all be its own",
    );

    let run = real_run();
    let verdicts = [ruling("a", "b", false)];
    let mut controls = 0;
    let result = run.measured(&verdicts, |_| {
        controls += 1;
        Ok(())
    });
    refused(&run, result, controls, "verdicts.json");
}

#[test]
fn a_second_run_leaves_the_first_runs_log_as_it_was() {
    let run = real_run();
    std::fs::create_dir_all(run.evidence("logs")).expect("the logs");
    std::fs::write(run.evidence("logs/volume-test.log"), "the first run's log").expect("a log");
    std::fs::write(run.evidence("report-first.txt"), "verdict: fails\n").expect("a report");

    let mut controls = 0;
    let why = run
        .measured(&[], |_| {
            controls += 1;
            Ok(())
        })
        .expect_err("the test was scored already");
    assert!(why.contains("report-first.txt"), "{why}");
    assert_eq!(controls, 0);
    assert_eq!(run.log(), "the first run's log");
}

#[test]
fn a_failed_control_ends_the_run_before_any_test_page() {
    let run = real_run();
    let why = (run.measured(&[], |_| Err("two pairs differ".to_owned())))
        .expect_err("the control failed");
    assert_eq!(why, "two pairs differ");
    assert!(run.log().contains("control failed: two pairs differ"));
    for name in ["fp-cache", "members.tsv", "scores.tsv", "report.txt"] {
        assert!(!run.evidence(name).exists(), "{name}");
    }
}

#[test]
fn a_volume_that_would_be_read_short_stops_the_run_after_the_control() {
    let run = real_run();
    let member = run.freeze.members[0].path.clone();
    zip_of(
        &run.material.library,
        &member,
        &pages_and(&[("006.heic", b"pixels")]),
    );

    let mut controls = 0;
    let why = run
        .measured(&[], |_| {
            controls += 1;
            Ok(())
        })
        .expect_err("a page would be skipped");
    assert!(why.contains("006.heic"), "{why}");
    assert_eq!(controls, 1, "the refusal comes after the control");
    assert!(run.log().contains("the run stopped: "), "{}", run.log());
    assert!(
        !run.evidence("fp-cache").exists(),
        "a page was fingerprinted"
    );
    assert!(!run.evidence("members.tsv").exists());
}
