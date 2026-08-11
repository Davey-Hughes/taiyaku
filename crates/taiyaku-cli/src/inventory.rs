//! The step `docs/status.md` §2 leaves out: getting from a release directory to
//! the sets worth comparing.
//!
//! On the One Piece collection that was 579 directories down to 141 volume
//! directories, forming 120 volume-editions of which 19 had more than one copy.
//! Both of those decisions are heuristics — a page-count threshold and a volume
//! number parsed from a name — which is exactly why they live here, in their own
//! command, printing what they decided, rather than inside `duplicates` where a
//! wrong guess would be invisible.

use std::path::{Path, PathBuf};

use crate::signature::Signature;

/// The volume number in a directory's name.
///
/// Takes the first digit run immediately preceded — optionally across one space
/// — by `v`, `vol`, `vol.` or `第`, and falls back to the first digit run when
/// none of those appear. Leading zeros are stripped, so `v007`, `v7` and `第7巻`
/// group together.
///
/// The marker is searched for before the fallback is used because a number
/// appearing *before* the volume marker — `[2015] ONE PIECE v077` — would
/// otherwise be read from its first digit run (2015) rather than the volume
/// the name actually carries (77). A year in trailing parentheses, `ONE PIECE
/// v077 (2015)`, does not test this: `077` is already the first digit run
/// there, so the marker scan and a bare first-digit-run rule agree on it
/// regardless of which one runs.
///
/// Deliberately not a regex. `regex` is three crates in a workspace whose short
/// dependency list `docs/status.md` states as a fact about the project, to do a
/// job a scan does; `--pattern` covers what the default misses. If a collection
/// turns up that neither handles, that is evidence for the dependency and it
/// can be added then.
pub fn volume_number(name: &str) -> Option<u32> {
    const MARKERS: [&str; 4] = ["vol.", "vol", "v", "第"];

    let chars: Vec<char> = name.chars().collect();
    let mut fallback = None;
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && chars[at].is_ascii_digit() {
            at += 1;
        }
        let number: Option<u32> = chars[start..at].iter().collect::<String>().parse().ok();
        if fallback.is_none() {
            fallback = number;
        }
        let before = chars[..start].iter().collect::<String>().to_lowercase();
        let before = before.strip_suffix(' ').unwrap_or(&before);
        if MARKERS.iter().any(|marker| before.ends_with(marker)) {
            return number;
        }
    }
    fallback
}

/// A `--pattern` literal with exactly one `{}` where the volume number goes.
#[derive(Debug, Clone)]
pub struct Pattern {
    before: String,
    after: String,
}

impl Pattern {
    pub fn parse(spec: &str) -> Result<Self, String> {
        let (before, after) = spec.split_once("{}").ok_or_else(|| {
            format!("--pattern `{spec}` needs one `{{}}` where the volume number goes")
        })?;
        if after.contains("{}") {
            return Err(format!(
                "--pattern `{spec}` has two `{{}}`; a name carries one volume number"
            ));
        }
        Ok(Self {
            before: before.to_string(),
            after: after.to_string(),
        })
    }

    /// The number where the pattern matches anywhere in the name.
    ///
    /// Scans over `name`'s character boundaries rather than stepping by byte
    /// offsets returned from `str::find`, so an empty `before` — `--pattern
    /// '{}巻'` — cannot land the scan inside a multi-byte character or walk
    /// past the end of the string looking for another empty match.
    pub fn find(&self, name: &str) -> Option<u32> {
        for (start, _) in name.char_indices() {
            if !name[start..].starts_with(&self.before) {
                continue;
            }
            let at = start + self.before.len();
            let digits: String = name[at..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if !digits.is_empty() && name[at + digits.len()..].starts_with(&self.after) {
                return digits.parse().ok();
            }
        }
        None
    }
}

/// One directory that holds a whole edition.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub pages: u32,
}

/// What the walk found.
#[derive(Debug, Clone, Default)]
pub struct Walked {
    pub dirs: usize,
    pub candidates: Vec<Candidate>,
    /// Page counts of directories that held pages and fell short of the
    /// threshold, so the range they spanned can be printed. A threshold that
    /// nearly cut something real is then visible rather than inferred.
    pub excluded: Vec<u32>,
}

/// Directories holding a whole edition: the deepest one that still does.
///
/// A directory is a candidate when its recursive page count reaches
/// `min_pages` and none of its children reach it on their own. That handles the
/// nested case — `docs/sidecar.md` records seven of fifty-seven corpus archives
/// putting chapters in subdirectories — without making each chapter a copy.
///
/// **Its failure mode, named rather than hidden.** A directory holding nothing
/// but material below the threshold has no qualifying child, so it becomes one
/// enormous candidate. That is visible the instant it happens, because the page
/// count is printed and eight thousand pages does not look like a volume.
///
/// Two passes, not one. A single top-down-as-you-go pass would have to decide
/// each chapter's fate — candidate, excluded, or neither — before it can know
/// whether the *parent* volume directory goes on to qualify, since that is only
/// known once every chapter has reported its total back up. Deciding chapter by
/// chapter as the recursion unwound put three sub-threshold chapters in
/// `excluded` moments before the volume that contains them turned out to
/// qualify — the exact case the doc comment above promises does not happen.
/// `totals` therefore only measures, with no side effects beyond the directory
/// count; `decide` walks the same tree a second time, top-down, and only
/// descends into a directory's children when none of them individually
/// qualifies — otherwise it decides right there and does not look further, so a
/// qualifying volume's chapters are never independently judged at all.
///
/// One case this leaves uncounted, plainly stated rather than left to be
/// noticed: when a directory is passed over because a child qualifies, that
/// directory's own loose page files — pages sitting next to the qualifying
/// child rather than inside it — are folded into the parent's `pages` total
/// by `totals` but are never visited by `decide`, so they land in neither
/// `candidates` nor `excluded`. Four stray preview PNGs at a release root did
/// exactly this in the real corpus run; they are correctly not a copy, but
/// the header line's candidate and exclusion counts do not add up to
/// everything on disk as a result.
pub fn walk(root: &Path, min_pages: u32) -> Result<Walked, String> {
    let mut walked = Walked::default();
    let tree = totals(root, &mut walked)?;
    decide(&tree, min_pages, &mut walked);
    walked.candidates.sort_by(|a, b| a.path.cmp(&b.path));
    walked.excluded.sort_unstable();
    Ok(walked)
}

/// One directory's recursive page total, and the same for its subdirectories —
/// gathered with no side effects on `Walked` beyond the directory count, so
/// `decide` can look ahead before committing anything.
#[derive(Debug)]
struct Totals {
    path: PathBuf,
    pages: u32,
    children: Vec<Totals>,
}

/// Recursion ceiling for `totals`, far past any real release tree — the
/// deepest nesting `docs/sidecar.md` records is a volume split into chapter
/// subdirectories, two levels — and there purely to turn a symlink cycle into
/// a named error rather than a stack overflow.
///
/// `path.is_dir()` follows symlinks, so a link back to an ancestor recurses
/// forever. The precise fix is a cycle guard: canonicalize each directory and
/// refuse to revisit one already on the path down. That costs a syscall per
/// directory, which is not worth paying on a 579-directory walk to guard
/// against something this rare — a depth limit catches the same failure for
/// free, at the cost of naming it one call late.
///
/// **Measured, not guessed, and lower than "far above a real tree" alone would
/// suggest.** A self-referential symlink does not actually recurse forever on
/// this platform: Linux caps symlink resolution at 40 hops per lookup
/// (`ELOOP`), and `Path::is_dir()` swallows that error and answers `false`
/// rather than propagating it — so past roughly 40 levels the walk does not
/// crash, it silently treats the cyclic subtree as childless and returns
/// `Ok`, which is the "silent truncation" this limit exists to rule out just
/// as much as the stack overflow. The ceiling has to fire before that
/// happens, not after, so it sits at 32 — comfortably past any real release
/// tree and comfortably short of where the OS starts lying about `is_dir`.
const MAX_DEPTH: u32 = 32;

fn totals(dir: &Path, walked: &mut Walked) -> Result<Totals, String> {
    totals_at(dir, 0, walked)
}

fn totals_at(dir: &Path, depth: u32, walked: &mut Walked) -> Result<Totals, String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "{}: nested past {MAX_DEPTH} directories deep; a symlink cycle is the likely cause",
            dir.display()
        ));
    }
    walked.dirs += 1;
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut here, mut child_paths) = (0u32, Vec::new());
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            child_paths.push(path);
        } else if taiyaku_read::is_page_name(&name) {
            here += 1;
        }
    }

    child_paths.sort();
    let mut children = Vec::new();
    let mut pages = here;
    for child in child_paths {
        let child = totals_at(&child, depth + 1, walked)?;
        pages += child.pages;
        children.push(child);
    }
    Ok(Totals {
        path: dir.to_path_buf(),
        pages,
        children,
    })
}

/// Decide one directory's fate — candidate, excluded, or neither — using
/// totals every directory in the tree already reported.
///
/// A directory whose own total reaches `min_pages` is decided at *this* level
/// and no further: its children are already folded into `pages` and are not
/// visited again, which is what keeps a nested volume's chapters out of both
/// `candidates` and `excluded`. Only when none of a directory's children
/// individually qualifies is that directory's own total the one that counts;
/// otherwise the decision belongs to whichever descendant does qualify, so this
/// recurses into every child instead.
fn decide(node: &Totals, min_pages: u32, walked: &mut Walked) {
    let child_qualifies = node.children.iter().any(|child| child.pages >= min_pages);
    if child_qualifies {
        for child in &node.children {
            decide(child, min_pages, walked);
        }
        return;
    }

    // A directory holding no pages anywhere is a container, neither a candidate
    // nor an exclusion — counting it as excluded would fill the reported range
    // with zeroes and hide what actually fell short.
    if node.pages == 0 {
        return;
    }
    if node.pages >= min_pages {
        walked.candidates.push(Candidate {
            path: node.path.clone(),
            pages: node.pages,
        });
    } else {
        walked.excluded.push(node.pages);
    }
}

/// Above the largest thing the One Piece run excluded (28 pages) and well below
/// the smallest real volume in the corpus (v092's all-spread copy at 101).
const DEFAULT_MIN_PAGES: u32 = 60;

/// Decide which groups to print in the inventory table.
///
/// Numbered volumes are printed only when duplicated (2+ copies).
/// The unparsed group (volume=None) is printed whenever it exists, since a
/// candidate that reached the threshold and then vanished from the output is
/// the failure this command exists to prevent.
fn should_print_group(volume: Option<u32>, copies_len: usize) -> bool {
    let is_unparsed = volume.is_none();
    is_unparsed || copies_len >= 2
}

/// Whether a group counts toward `settled`: all of its copies share one
/// signature, **and** there is more than one copy for that to be a statement
/// about.
///
/// `settled` is printed against `duplicated` as its denominator, so the two
/// have to count the same kind of thing. A group of one satisfies
/// `largest == copies` trivially, by agreeing with itself, and counting it
/// printed `duplicated=1 settled=2` — a fraction of more than one whole.
/// Single-copy numbered groups never reached the counter because they are not
/// printed at all; the unparsed group always is, so what the counter reported
/// turned on whether a name parsed, which is nothing to do with signatures.
fn is_settled(copies: usize, largest: usize) -> bool {
    copies >= 2 && largest == copies
}

/// The span of what fell short of the threshold, or nothing if nothing did.
///
/// `walk` sorts `excluded`, so the ends of the slice are the ends of the range.
/// Returned as a pair rather than as a formatted string because the header line
/// and the summary line print it two different ways — `(3-28 pages)` and
/// `3-28` — and a range that appeared in one but not the other would be a
/// discrepancy with no explanation.
fn excluded_span(excluded: &[u32]) -> Option<(u32, u32)> {
    match (excluded.first(), excluded.last()) {
        (Some(low), Some(high)) => Some((*low, *high)),
        _ => None,
    }
}

/// Candidates grouped by the volume number their directory name carries,
/// unparsed names last.
///
/// Grouping is by value rather than by a map, because the group with no number
/// has to exist as a group: a candidate whose name yields nothing is reported
/// under `?` rather than dropped, and a dropped candidate is the silence this
/// command exists to prevent.
fn group_by_volume<'a>(
    candidates: &'a [Candidate],
    pattern: Option<&Pattern>,
) -> Vec<(Option<u32>, Vec<&'a Candidate>)> {
    let mut groups: Vec<(Option<u32>, Vec<&Candidate>)> = Vec::new();
    for candidate in candidates {
        let name = candidate
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let number = match pattern {
            Some(pattern) => pattern.find(&name),
            None => volume_number(&name),
        };
        match groups.iter_mut().find(|(volume, _)| *volume == number) {
            Some((_, copies)) => copies.push(candidate),
            None => groups.push((number, vec![candidate])),
        }
    }
    groups.sort_by_key(|(volume, _)| volume.unwrap_or(u32::MAX));
    groups
}

/// How many copies share the most common signature among them.
fn agreement(digests: &[String]) -> usize {
    digests
        .iter()
        .map(|one| digests.iter().filter(|other| *other == one).count())
        .max()
        .unwrap_or(1)
}

/// One line of the inventory table, and everything the footer counts from it.
struct Row {
    volume: Option<u32>,
    copies: usize,
    /// How many of this group's copies share the most common signature.
    largest: usize,
    paths: Vec<String>,
}

impl Row {
    /// The signature column: what this group's copies agree on.
    fn column(&self) -> String {
        match self.largest {
            1 => "distinct".to_string(),
            n if n == self.copies => "identical".to_string(),
            n => format!("{n} of {}", self.copies),
        }
    }
}

/// The rows the table prints, in group order.
///
/// Signatures are taken only for groups that print. A release directory whose
/// volumes are mostly single copies — 120 volume-editions of which 19 had more
/// than one copy, on the One Piece collection — would otherwise pay to sign
/// every one of them to produce a line nobody sees.
fn rows(groups: &[(Option<u32>, Vec<&Candidate>)]) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (volume, copies) in groups {
        if !should_print_group(*volume, copies.len()) {
            continue;
        }
        let digests: Vec<String> = copies
            .iter()
            .map(|c| Signature::of(std::slice::from_ref(&c.path)).map(|s| s.digest))
            .collect::<Result<_, _>>()?;
        rows.push(Row {
            volume: *volume,
            copies: copies.len(),
            largest: agreement(&digests),
            // The page count travels with its path rather than staying buried
            // in `Candidate` — it is the number a person checks a signature
            // verdict against, so it belongs on the same line as the copy it
            // describes.
            paths: copies
                .iter()
                .map(|c| format!("{} ({} pages)", c.path.to_string_lossy(), c.pages))
                .collect(),
        });
    }
    Ok(rows)
}

/// What the footer reports: how many printed groups hold more than one copy,
/// and how many of those are settled.
///
/// Counted over the rows rather than over every group, so the two numbers
/// describe the table the reader is looking at. `settled` is printed against
/// `duplicated` as its denominator and can never exceed it.
fn counts(rows: &[Row]) -> (usize, usize) {
    (
        rows.iter().filter(|row| row.copies >= 2).count(),
        rows.iter()
            .filter(|row| is_settled(row.copies, row.largest))
            .count(),
    )
}

pub fn command(args: &[String]) -> Result<(), String> {
    crate::reject_unknown_flags("inventory", args, &["--min-pages", "--pattern"])?;
    let root = PathBuf::from(
        args.first()
            .filter(|a| !a.starts_with("--"))
            .ok_or("needs a directory to walk")?,
    );
    let min_pages = match crate::option_value(args, "--min-pages", "a number of pages")? {
        Some(given) => given
            .parse::<u32>()
            .map_err(|_| format!("--min-pages `{given}` is not a number of pages"))?,
        None => DEFAULT_MIN_PAGES,
    };
    let pattern = match crate::option_value(args, "--pattern", "a pattern")? {
        Some(spec) => Some(Pattern::parse(spec)?),
        None => None,
    };

    let walked = walk(&root, min_pages)?;
    let span = excluded_span(&walked.excluded);
    let range = match span {
        Some((low, high)) => format!(" ({low}-{high} pages)"),
        None => String::new(),
    };
    eprintln!(
        "walked {} directories | {} candidates | excluded {}{range}",
        walked.dirs,
        walked.candidates.len(),
        walked.excluded.len()
    );

    let groups = group_by_volume(&walked.candidates, pattern.as_ref());
    let table = rows(&groups)?;
    let (duplicated, settled) = counts(&table);

    eprintln!("\nvol  copies  signature   paths");
    for row in &table {
        eprintln!(
            "{:<4} {:<7} {:<11} {}",
            match row.volume {
                Some(number) => format!("{number:03}"),
                None => "?".to_string(),
            },
            row.copies,
            row.column(),
            row.paths.join(", ")
        );
    }

    println!(
        "inventory: dirs={} candidates={} volumes={} duplicated={duplicated} settled={settled} \
         excluded={} excluded-pages={}",
        walked.dirs,
        walked.candidates.len(),
        groups.len(),
        walked.excluded.len(),
        match span {
            Some((low, high)) => format!("{low}-{high}"),
            None => "-".to_string(),
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_number_is_taken_from_the_marker_that_precedes_it() {
        assert_eq!(volume_number("v069"), Some(69));
        assert_eq!(volume_number("v095-colour-c0"), Some(95));
        assert_eq!(volume_number("第69巻"), Some(69));
        assert_eq!(volume_number("vol. 7"), Some(7));
        assert_eq!(volume_number("raw-v01-src-a"), Some(1));
    }

    /// With no marker, the first digit run is the best available guess.
    #[test]
    fn a_bare_number_still_groups() {
        assert_eq!(volume_number("069"), Some(69));
    }

    /// A trailing year does not distinguish the marker scan from a bare
    /// first-digit-run rule: `077` is already the first digit run here, so
    /// both agree on `77` regardless of which one runs. Kept as a check that
    /// the marker still matches this shape, not as the case that justifies
    /// searching for it first — see the next test for that.
    #[test]
    fn a_year_in_the_name_does_not_win_over_the_volume() {
        assert_eq!(volume_number("ONE PIECE v077 (2015)"), Some(77));
    }

    /// The case that actually distinguishes the marker scan from a bare
    /// first-digit-run rule, and the reason the marker is searched for before
    /// falling back: a number appearing *before* the volume marker. The
    /// marker scan finds `77`; a bare first-digit-run rule would return `2015`
    /// instead. Sabotage-checked: with `MARKERS` and the preceding-text check
    /// removed so `volume_number` falls straight through to the first digit
    /// run, this assertion fails (`2015` instead of `77`), confirming the
    /// marker scan is what this test exercises.
    #[test]
    fn a_number_before_the_marker_does_not_win_over_the_volume() {
        assert_eq!(volume_number("[2015] ONE PIECE v077"), Some(77));
    }

    /// A candidate that reached the page threshold and then vanished from the
    /// output is the silence this repository exists to avoid. When a single
    /// unparsed candidate is present, it must appear in the output — a name
    /// without a volume number indicates a parse failure that must be visible.
    #[test]
    fn a_name_with_no_digits_has_no_number_rather_than_a_wrong_one() {
        assert_eq!(volume_number("extras"), None);
    }

    /// A single unparsed candidate (name with no digits) is printed in the
    /// inventory table, not silently dropped. This is the case the inventory
    /// command exists to catch: a candidate that reached the page threshold and
    /// then vanished would hide a parse failure.
    #[test]
    fn a_single_unparsed_candidate_is_reported() {
        let root = std::env::temp_dir().join("taiyaku-test-inventory-unparsed");
        let _ = std::fs::remove_dir_all(&root);
        // Create one directory with no volume number in its name, above threshold.
        let unparsed = root.join("extras");
        std::fs::create_dir_all(&unparsed).unwrap();
        for page in 0..70 {
            std::fs::write(unparsed.join(format!("{page}.jpg")), b"x").unwrap();
        }

        let walked = walk(&root, 60).unwrap();
        let _ = std::fs::remove_dir_all(&root);

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
    /// and it is printed against `duplicated`, so it can never exceed it. A
    /// group of one used to count: `largest == copies.len()` is trivially true
    /// there, and a real run over four directories printed
    /// `duplicated=1 settled=2` because the single unparsed group reached the
    /// counter. The gate is on the group's size, not on whether its name
    /// parsed.
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

    /// An empty prefix on a multi-byte name it must match — `find` used to
    /// panic here (`byte index 1 is not a char boundary`) because the old
    /// byte-stepping scan advanced one byte into `第` instead of past it.
    #[test]
    fn an_empty_prefix_finds_the_number_before_a_multibyte_suffix() {
        let pattern = Pattern::parse("{}巻").unwrap();

        assert_eq!(pattern.find("第69巻"), Some(69));
    }

    /// The same empty-prefix pattern on a name with no digits at all: `find`
    /// used to walk past the end of the string here rather than return
    /// `None`.
    #[test]
    fn an_empty_prefix_on_a_name_with_no_digits_finds_nothing() {
        let pattern = Pattern::parse("{}巻").unwrap();

        assert_eq!(pattern.find("extras"), None);
    }

    /// A placeholder with nothing before or after it: `find` used to panic
    /// (`byte index 4 is out of bounds for string of length 3`) once the scan
    /// walked past the end of `"abc"` looking for another empty match.
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
        let root = std::env::temp_dir().join("taiyaku-test-inventory-nested");
        let _ = std::fs::remove_dir_all(&root);
        let volume = root.join("v01");
        for chapter in ["c001", "c002", "c003"] {
            let dir = volume.join(chapter);
            std::fs::create_dir_all(&dir).unwrap();
            for page in 0..4 {
                std::fs::write(dir.join(format!("{page}.jpg")), b"x").unwrap();
            }
        }

        let walked = walk(&root, 10).unwrap();

        let _ = std::fs::remove_dir_all(&root);
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
        let root = std::env::temp_dir().join("taiyaku-test-inventory-short");
        let _ = std::fs::remove_dir_all(&root);
        for (name, pages) in [("v01", 12u32), ("jump-24", 3), ("jump-31", 5)] {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            for page in 0..pages {
                std::fs::write(dir.join(format!("{page}.jpg")), b"x").unwrap();
            }
        }

        let walked = walk(&root, 10).unwrap();

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(walked.candidates.len(), 1);
        assert_eq!(walked.excluded, vec![3, 5]);
    }

    /// A symlink pointing back at its own directory: `path.is_dir()` follows
    /// it, so without the depth limit `totals` would recurse until the stack
    /// overflows rather than return an error. This must come back as a named
    /// error mentioning the depth it gave up at, not a panic and not a
    /// silent truncation.
    #[test]
    #[cfg(unix)]
    fn a_symlink_cycle_errors_naming_the_depth_instead_of_overflowing() {
        let root = std::env::temp_dir().join("taiyaku-test-inventory-cycle");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();

        let mut walked = Walked::default();
        let error = totals(&root, &mut walked).unwrap_err();

        let _ = std::fs::remove_dir_all(&root);
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
        let root = std::env::temp_dir().join("taiyaku-test-inventory-at-limit");
        let _ = std::fs::remove_dir_all(&root);
        // The root is depth zero, so `MAX_DEPTH` directories below it put the
        // deepest one exactly at the ceiling.
        let mut deepest = root.clone();
        for level in 0..MAX_DEPTH {
            deepest = deepest.join(format!("d{level}"));
        }
        std::fs::create_dir_all(&deepest).unwrap();
        std::fs::write(deepest.join("000.jpg"), b"x").unwrap();

        let mut walked = Walked::default();
        let measured = totals(&root, &mut walked);

        let _ = std::fs::remove_dir_all(&root);
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
        let root = std::env::temp_dir().join("taiyaku-test-inventory-deep");
        let _ = std::fs::remove_dir_all(&root);
        let mut deepest = root.clone();
        for level in 0..(MAX_DEPTH + 5) {
            deepest = deepest.join(format!("d{level}"));
        }
        std::fs::create_dir_all(&deepest).unwrap();

        let mut walked = Walked::default();
        let error = totals(&root, &mut walked).unwrap_err();

        let _ = std::fs::remove_dir_all(&root);
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
    /// work for a line nobody sees — 101 of the One Piece collection's 120
    /// volume-editions.
    #[test]
    fn only_the_groups_worth_printing_become_rows() {
        let root = std::env::temp_dir().join("taiyaku-test-inventory-rows");
        let _ = std::fs::remove_dir_all(&root);
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

        let _ = std::fs::remove_dir_all(&root);
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
        let root = std::env::temp_dir().join("taiyaku-test-inventory-command");
        let _ = std::fs::remove_dir_all(&root);
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

        let _ = std::fs::remove_dir_all(&root);
        walked.expect("an argument that is not a flag is the directory to walk");
        assert!(
            refused.unwrap_err().contains("needs a directory"),
            "a run with no directory says so"
        );
    }
}
