//! Which volumes the test holds and what each pair of them is.
//!
//! The classes are the owner's rulings written out, not read from the names:
//! two editions can share a volume number and still divide the story
//! differently, which `manifest::label` would call the same volume.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::super::manifest::{ARCHIVES, DirRow, Kind, Manifest, Member, Split, draft_member};

/// One volume of the test, as `members.json` holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TestMember {
    /// Relative to the library root.
    pub path: String,
    pub series: String,
    pub edition: String,
    pub number: u32,
}

impl TestMember {
    /// The library directory holding this member, spelled as its row spells it.
    pub(super) fn directory(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(dir, _)| dir)
    }
}

/// Every row is `HeldOut` only because a `DirRow` must name a split: nothing in
/// this test reads it.
const fn row(dir: &'static str, series: &'static str, edition: &'static str) -> DirRow {
    DirRow {
        dir,
        series,
        franchise: series,
        edition,
        kind: Some(Kind::Volume),
        ltr: false,
        split: Split::HeldOut,
    }
}

/// The directories under test, named exactly as they are on disk: the colour
/// Hunter × Hunter directory's name ends in a space, and trimmed it finds
/// nothing.
pub(super) const ROWS: [DirRow; 11] = [
    row("Raw/Tongari Boushi no Atelier", "tongari", "raw"),
    row("Volumes/Tongari Boushi no Atelier", "tongari", "en"),
    row("Raw/RuriDragon", "ruridragon", "raw"),
    row("Volumes/RuriDragon", "ruridragon", "en"),
    row("Volumes/Hunter × Hunter", "hunter-x-hunter", "bw"),
    row(
        "Volumes/Hunter × Hunter (Official Colored) ",
        "hunter-x-hunter",
        "colour",
    ),
    row("Volumes/Parasyte", "parasyte", "bw"),
    row(
        "Volumes/Parasyte - Full Color Collection",
        "parasyte",
        "collection",
    ),
    row(
        "Volumes/Parasyte - Full Color (Colored Council)",
        "parasyte",
        "colored-council",
    ),
    row("Volumes/Yu Yu Hakusho", "yu-yu-hakusho", "bw"),
    row(
        "Volumes/Yu Yu Hakusho - Colored (Colored Council)",
        "yu-yu-hakusho",
        "colored-council",
    ),
];

/// The single volume a name gives. The `第05巻` form is read here and not in
/// `draft_member`, because the scored manifest's labels rest on that reader.
pub(super) fn number_of(name: &str) -> Option<u32> {
    (draft_member(name, &ROWS[0]).volumes)
        .filter(|(first, last)| first == last)
        .map(|(first, _)| first)
        .or_else(|| printed_number(name))
}

/// The digits between `第` and `巻`.
fn printed_number(name: &str) -> Option<u32> {
    let (_, after) = name.split_once('第')?;
    let (digits, _) = after.split_once('巻')?;
    (digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

/// Every row's members from names alone, sorted by path. It lists directories
/// and opens no archive, because no page of these series may be read before
/// the bar is frozen. An entry it cannot number is an error, since a member
/// dropped here would silently remove its pairs from the test.
pub(super) fn draft(library: &Path) -> Result<Vec<TestMember>, String> {
    let mut members = Vec::new();
    for row in &ROWS {
        let entries =
            std::fs::read_dir(library.join(row.dir)).map_err(|e| format!("{}: {e}", row.dir))?;
        let mut numbers = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", row.dir))?;
            let os_name = entry.file_name();
            let name = (os_name.to_str())
                .ok_or_else(|| format!("{}/{}: not UTF-8", row.dir, os_name.display()))?;
            let archive = (name.rsplit_once('.'))
                .is_some_and(|(_, ext)| ARCHIVES.contains(&ext.to_ascii_lowercase().as_str()));
            if name.starts_with('.') || !(archive || entry.path().is_dir()) {
                continue;
            }
            let path = format!("{}/{name}", row.dir);
            let number = number_of(name).ok_or_else(|| format!("{path}: no volume number"))?;
            if !numbers.insert(number) {
                return Err(format!(
                    "{path}: volume {number} is already in this directory"
                ));
            }
            members.push(TestMember {
                path,
                series: row.series.to_owned(),
                edition: row.edition.to_owned(),
                number,
            });
        }
    }
    members.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(members)
}

/// The members the test runs on.
pub(super) fn committed() -> Vec<TestMember> {
    serde_json::from_str(include_str!("members.json")).expect("members.json parses")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairClass {
    /// The same volume in two editions.
    True,
    /// The same volume with one copy damaged: outside pass and fail.
    Damaged,
    /// Editions that divide the story differently, so wrong even where the
    /// numbers agree. Kept apart from `Wrong` because the owner has ruled on
    /// these, so no later verdict may relabel one as the same volume.
    DifferentSplit,
    Wrong,
}

/// What a pair is, the same whichever member comes first. The order matters:
/// a damaged pair is also a same-numbered pair of partner editions, so
/// `Damaged` must be asked before `True`.
pub(super) fn pair_class(a: &TestMember, b: &TestMember) -> PairClass {
    if a.series != b.series {
        PairClass::Wrong
    } else if different_split(a, b) {
        PairClass::DifferentSplit
    } else if damaged(a, b) {
        PairClass::Damaged
    } else if same_volume(a, b) {
        PairClass::True
    } else {
        PairClass::Wrong
    }
}

fn editions(a: &TestMember, b: &TestMember, x: &str, y: &str) -> bool {
    (a.edition == x && b.edition == y) || (a.edition == y && b.edition == x)
}

fn different_split(a: &TestMember, b: &TestMember) -> bool {
    let council = |other| editions(a, b, "colored-council", other);
    match a.series.as_str() {
        "parasyte" => council("bw") || council("collection"),
        "yu-yu-hakusho" => council("bw"),
        _ => false,
    }
}

/// Only this pair: the damaged volume against any other volume is still an
/// ordinary wrong pair.
fn damaged(a: &TestMember, b: &TestMember) -> bool {
    a.series == "hunter-x-hunter"
        && editions(a, b, "colour", "bw")
        && a.number == 35
        && b.number == 35
}

fn same_volume(a: &TestMember, b: &TestMember) -> bool {
    let partners = match a.series.as_str() {
        "tongari" | "ruridragon" => editions(a, b, "raw", "en"),
        "hunter-x-hunter" => editions(a, b, "bw", "colour"),
        "parasyte" => editions(a, b, "bw", "collection"),
        _ => false,
    };
    partners && a.number == b.number
}

/// Every pair of `n` members once, as `(i, j)` with `i < j`.
pub(super) fn all_pairs(n: usize) -> Vec<(usize, usize)> {
    (0..n)
        .flat_map(|i| ((i + 1)..n).map(move |j| (i, j)))
        .collect()
}

/// The members as the scoring code takes them.
pub(super) fn to_manifest(members: &[TestMember]) -> Manifest {
    let members = (members.iter())
        .map(|m| Member {
            path: m.path.clone(),
            series: m.series.clone(),
            franchise: m.series.clone(),
            edition: m.edition.clone(),
            kind: Kind::Volume,
            volumes: Some((m.number, m.number)),
            copy: None,
            chapter: None,
            in_volume: None,
            after_volume: None,
            issue: None,
            issues: None,
            ltr: false,
            split: Split::HeldOut,
            truth: None,
        })
        .collect();
    Manifest {
        members,
        adjudications: Vec::new(),
    }
}
