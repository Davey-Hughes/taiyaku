use super::*;
use taiyaku_read::test_support::TempDir;

/// A single unparsed candidate (name with no digits) is printed in the
/// inventory table, not silently dropped. This is the case the inventory
/// command exists to catch: a candidate that reached the page threshold and
/// then vanished would hide a parse failure.
#[test]
fn a_single_unparsed_candidate_is_reported() {
    let root = TempDir::new("inventory-unparsed");
    // Create one directory with no volume number in its name, above threshold.
    let unparsed = root.join("extras");
    std::fs::create_dir_all(&unparsed).unwrap();
    for page in 0..70 {
        std::fs::write(unparsed.join(format!("{page}.jpg")), b"x").unwrap();
    }

    let walked = walk(&root, 60).unwrap();

    // Verify the candidate was found.
    assert_eq!(walked.candidates.len(), 1);
    assert_eq!(walked.candidates[0].path.file_name().unwrap(), "extras");

    // Grouped by the same function `command` uses, rather than by a copy of
    // it written here: a copy agrees with itself whatever `command` does.
    let groups = group_by_volume(&walked.candidates, None);

    // The unparsed group (None key) exists with one member.
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, None);
    assert_eq!(groups[0].1.len(), 1);

    // And it is printed, though a group of one would be skipped if its
    // name had parsed.
    let (volume, copies) = &groups[0];
    assert!(
        should_print_group(*volume, copies.len()),
        "an unparsed group is printed however few copies it has"
    );
    assert!(
        !should_print_group(Some(1), copies.len()),
        "test setup: a group this size only prints because it is unparsed"
    );
}

/// `settled` counts duplicate sets whose copies all share one signature,
/// and it is printed against `duplicated`, so it can never exceed it.
/// `largest == copies` is trivially true for a group of one, so the gate is
/// on the group's size, not on whether its name parsed.
#[test]
fn a_group_of_one_is_not_a_settled_duplicate() {
    assert!(
        !is_settled(1, 1),
        "one copy agreeing with itself is not a settled duplicate set"
    );
    assert!(is_settled(2, 2), "two copies of one signature");
    assert!(!is_settled(2, 1), "two copies, two signatures");
    assert!(!is_settled(3, 2), "two of three share a signature");
}

#[test]
fn a_pattern_overrides_the_default_parse() {
    let pattern = Pattern::parse("第{}巻").unwrap();

    assert_eq!(pattern.find("第69巻"), Some(69));
    assert_eq!(pattern.find("v69"), None, "the literal must match too");
}

#[test]
fn a_pattern_without_a_placeholder_is_refused() {
    let error = Pattern::parse("v").unwrap_err();

    assert!(error.contains("{}"), "unhelpful: {error}");
}

#[test]
fn a_pattern_with_two_placeholders_is_refused() {
    let error = Pattern::parse("v{}-{}").unwrap_err();

    assert!(error.contains("two"), "unhelpful: {error}");
}

/// An empty prefix on a multi-byte name it must match.
#[test]
fn an_empty_prefix_finds_the_number_before_a_multibyte_suffix() {
    let pattern = Pattern::parse("{}巻").unwrap();

    assert_eq!(pattern.find("第69巻"), Some(69));
}

/// The same empty-prefix pattern on a name with no digits at all finds nothing.
#[test]
fn an_empty_prefix_on_a_name_with_no_digits_finds_nothing() {
    let pattern = Pattern::parse("{}巻").unwrap();

    assert_eq!(pattern.find("extras"), None);
}

/// A placeholder with nothing before or after it, on a short name.
#[test]
fn an_empty_pattern_on_a_short_name_finds_nothing() {
    let pattern = Pattern::parse("{}").unwrap();

    assert_eq!(pattern.find("abc"), None);
}

/// The candidate rule: the deepest directory that still holds a whole
/// edition. Stated that way rather than as "a directory that directly holds
/// images" because `docs/sidecar.md` records that some editions nest — a
/// volume split into chapter directories holds no images of its own, and
/// the chapter directories are not what a person means by a copy.
#[test]
fn a_nested_volume_is_one_candidate_not_seven_and_not_zero() {
    let root = TempDir::new("inventory-nested");
    let volume = root.join("v01");
    for chapter in ["c001", "c002", "c003"] {
        let dir = volume.join(chapter);
        std::fs::create_dir_all(&dir).unwrap();
        for page in 0..4 {
            std::fs::write(dir.join(format!("{page}.jpg")), b"x").unwrap();
        }
    }

    let walked = walk(&root, 10).unwrap();
    let paths: Vec<&Path> = walked.candidates.iter().map(|c| c.path.as_path()).collect();
    assert_eq!(paths, vec![volume.as_path()]);
    assert_eq!(walked.candidates[0].pages, 12);
    assert!(
        walked.excluded.is_empty(),
        "chapters are not excluded copies"
    );
}

/// What falls below the threshold is counted and its page range reported,
/// so a threshold that nearly cut something real is visible.
#[test]
fn material_below_the_threshold_is_excluded_and_its_range_recorded() {
    let root = TempDir::new("inventory-short");
    for (name, pages) in [("v01", 12u32), ("jump-24", 3), ("jump-31", 5)] {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for page in 0..pages {
            std::fs::write(dir.join(format!("{page}.jpg")), b"x").unwrap();
        }
    }

    let walked = walk(&root, 10).unwrap();
    assert_eq!(walked.candidates.len(), 1);
    assert_eq!(walked.excluded, vec![3, 5]);
}

/// A symlink pointing back at its own directory, which `path.is_dir()` follows
/// (`MAX_DEPTH`'s doc says where that ends without the limit). This must come
/// back as a named error mentioning the depth it gave up at, not a silent
/// truncation.
#[test]
#[cfg(unix)]
fn a_symlink_cycle_errors_naming_the_depth_instead_of_a_truncated_ok() {
    let root = TempDir::new("inventory-cycle");
    std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();

    let mut walked = Walked::default();
    let error = totals(&root, &mut walked).unwrap_err();
    assert!(
        error.contains(&MAX_DEPTH.to_string()),
        "the depth ceiling should be named: {error}"
    );
}

/// The other side of the ceiling, which is the side that decides what the
/// comparison is.
///
/// Both cycle tests nest far past the limit, where `>`, `>=` and `==` all
/// refuse alike. What separates them is a tree that reaches exactly
/// `MAX_DEPTH` and has to be walked in full: a ceiling one level low turns
/// a legal release tree into a named error about a symlink cycle that is
/// not there, and the error would be believed.
#[test]
fn a_directory_nested_exactly_to_the_limit_is_walked_rather_than_refused() {
    let root = TempDir::new("inventory-at-limit");
    // The root is depth zero, so `MAX_DEPTH` directories below it put the
    // deepest one exactly at the ceiling.
    let mut deepest = root.to_path_buf();
    for level in 0..MAX_DEPTH {
        deepest = deepest.join(format!("d{level}"));
    }
    std::fs::create_dir_all(&deepest).unwrap();
    std::fs::write(deepest.join("000.jpg"), b"x").unwrap();

    let mut walked = Walked::default();
    let measured = totals(&root, &mut walked);
    let measured = measured.expect("32 levels is at the ceiling, not past it");
    assert_eq!(measured.pages, 1, "the page at the bottom is counted");
    assert_eq!(
        walked.dirs,
        MAX_DEPTH as usize + 1,
        "every directory on the way down, and the root"
    );
}

/// The same failure without relying on a real symlink, for a ceiling that
/// stays meaningful if the depth limit ever changes: a directory nested
/// past `MAX_DEPTH` errors rather than being silently walked in full.
#[test]
fn a_directory_nested_past_the_limit_errors_naming_the_depth() {
    let root = TempDir::new("inventory-deep");
    let mut deepest = root.to_path_buf();
    for level in 0..(MAX_DEPTH + 5) {
        deepest = deepest.join(format!("d{level}"));
    }
    std::fs::create_dir_all(&deepest).unwrap();

    let mut walked = Walked::default();
    let error = totals(&root, &mut walked).unwrap_err();
    assert!(
        error.contains(&MAX_DEPTH.to_string()),
        "the depth ceiling should be named: {error}"
    );
}

/// Both halves of `find`'s match have to hold at the same position, and the
/// scan must carry on past a place where only one of them does. `v` is an
/// ordinary letter as well as a volume marker, and a name that contains one
/// before the real one is not a name with no number in it.
#[test]
fn a_prefix_not_followed_by_digits_does_not_end_the_scan() {
    let pattern = Pattern::parse("v{}").unwrap();

    assert_eq!(pattern.find("one-piece-v-tankoubon-v069"), Some(69));
    assert_eq!(pattern.find("no-digits-at-all"), None);
}

/// A numbered volume with one copy is not printed: one copy is not a
/// duplicate, and this is a table of duplicates. The unparsed group is the
/// exception, and it is the only one.
#[test]
fn a_numbered_volume_prints_only_once_it_has_more_than_one_copy() {
    assert!(
        !should_print_group(Some(7), 1),
        "one copy of volume 7 is not a duplicate"
    );
    assert!(should_print_group(Some(7), 2));
    assert!(
        should_print_group(None, 1),
        "an unparsed name is printed however few copies it has"
    );
}

/// The range of what fell short, printed twice in two formats from one
/// measurement.
#[test]
fn the_excluded_range_spans_what_fell_short_of_the_threshold() {
    assert_eq!(excluded_span(&[3, 5, 28]), Some((3, 28)));
    assert_eq!(
        excluded_span(&[7]),
        Some((7, 7)),
        "one exclusion is its own range"
    );
    assert_eq!(excluded_span(&[]), None, "nothing fell short");
}

/// Copies of one volume land in one group; a name that parses to nothing
/// gets its own group, at the end, rather than joining somebody else's.
#[test]
fn candidates_group_by_volume_number_with_unparsed_names_last() {
    let candidates: Vec<Candidate> = ["v02-a", "extras", "v01-a", "v01-b"]
        .iter()
        .map(|name| Candidate {
            path: PathBuf::from(name),
            pages: 100,
        })
        .collect();

    let grouped: Vec<(Option<u32>, usize)> = group_by_volume(&candidates, None)
        .iter()
        .map(|(volume, copies)| (*volume, copies.len()))
        .collect();

    assert_eq!(grouped, vec![(Some(1), 2), (Some(2), 1), (None, 1)]);
}

/// What the signature column counts: the size of the largest set of copies
/// that agree, not the size of any other set.
///
/// Three distinct copies is the case that separates it from its inverse.
/// With two copies of one signature and one of another, "how many agree"
/// and "how many disagree with the odd one out" are both 2, and a test
/// built on that shape cannot tell the two apart.
#[test]
fn agreement_counts_the_copies_sharing_the_most_common_signature() {
    let digests = |all: &[&str]| all.iter().map(|d| (*d).to_string()).collect::<Vec<_>>();

    assert_eq!(agreement(&digests(&["a", "b", "c"])), 1, "all distinct");
    assert_eq!(agreement(&digests(&["a", "a", "b", "c"])), 2, "two of four");
    assert_eq!(agreement(&digests(&["a", "a", "a"])), 3, "identical");
    assert_eq!(agreement(&[]), 1, "nothing to disagree about");
}

/// The signature column, which is the whole verdict on a duplicate set.
#[test]
fn the_signature_column_says_what_the_copies_agree_on() {
    let row = |copies, largest| Row {
        volume: Some(1),
        copies,
        largest,
        paths: Vec::new(),
    };

    assert_eq!(row(3, 1).column(), "distinct");
    assert_eq!(row(3, 3).column(), "identical");
    assert_eq!(row(3, 2).column(), "2 of 3");
}

/// The footer's two numbers, and the relationship between them: `settled`
/// is printed against `duplicated` as its denominator, so a table where it
/// exceeded that would be reporting a fraction of more than one whole.
#[test]
fn the_footer_counts_duplicated_sets_and_the_settled_ones_among_them() {
    let row = |copies, largest| Row {
        volume: Some(1),
        copies,
        largest,
        paths: Vec::new(),
    };
    let table = [
        row(1, 1), // the unparsed group: printed, but not a duplicate
        row(2, 2), // two copies of one signature: duplicated and settled
        row(3, 2), // three copies, two agreeing: duplicated, not settled
    ];

    assert_eq!(counts(&table), (2, 1));
    assert_eq!(counts(&[]), (0, 0));
}

/// A group that is not printed is not signed and not counted. Reading the
/// pages of every single-copy volume to decide it is not a duplicate is
/// work for a line nobody sees.
#[test]
fn only_the_groups_worth_printing_become_rows() {
    let root = TempDir::new("inventory-rows");
    // Two identical copies of volume 1, one of volume 2, one unparsed.
    for name in ["v01-a", "v01-b", "v02-only", "extras"] {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for page in 0..12 {
            std::fs::write(dir.join(format!("{page:03}.jpg")), b"x").unwrap();
        }
    }

    let walked = walk(&root, 10).unwrap();
    let groups = group_by_volume(&walked.candidates, None);
    let table = rows(&groups).unwrap();
    assert_eq!(groups.len(), 3, "volume 2 is a group, it is just not a row");
    let shape: Vec<(Option<u32>, usize, usize)> = table
        .iter()
        .map(|row| (row.volume, row.copies, row.largest))
        .collect();
    assert_eq!(shape, vec![(Some(1), 2, 2), (None, 1, 1)]);
    assert_eq!(counts(&table), (1, 1), "one duplicate set, and it agrees");
}

/// The first argument is the directory to walk, and it is recognised by
/// *not* being a flag. A filter that kept flags instead would refuse every
/// real invocation with "needs a directory to walk", which reads as the
/// user's mistake rather than the command's.
#[test]
fn the_command_walks_the_directory_it_is_given() {
    let root = TempDir::new("inventory-command");
    for name in ["v01-a", "v01-b"] {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for page in 0..12 {
            std::fs::write(dir.join(format!("{page:03}.jpg")), b"x").unwrap();
        }
    }

    let walked = command(&[
        root.to_string_lossy().to_string(),
        "--min-pages".to_string(),
        "10".to_string(),
    ]);
    let refused = command(&["--min-pages".to_string(), "10".to_string()]);
    walked.expect("an argument that is not a flag is the directory to walk");
    assert!(
        refused.unwrap_err().contains("needs a directory"),
        "a run with no directory says so"
    );
}
