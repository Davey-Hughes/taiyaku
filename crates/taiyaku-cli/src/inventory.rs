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
mod tests;
