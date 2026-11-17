//! The measurement's truth from names: what each member of the test library
//! is, which pairs of members are scored, and what each pair should score as.
//!
//! A member is one entry of a [`TABLE`] directory, an archive or a directory
//! of images. Its kind, numbers and copy suffix come from its name
//! ([`draft_member`]); its series, edition and split come from its row.
//! [`label`] says what a pair is from those fields, an adjudication first, and
//! [`class`] turns a label into one rule's positive or negative.
//!
//! A pair is unordered: [`explicit_pairs`] and [`scored_pairs`] hold each one
//! once, as `(i, j)` with `i < j`, so no list can weigh a pair twice.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    features::PairFeatures,
    rule::{MIN_CHAPTER_SLOTS, Rule},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Kind {
    Volume,
    Pack,
    Omnibus,
    Chapter,
    ChapterPack,
    Issue,
    Collected,
    Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Split {
    Dev,
    HeldOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Member {
    /// Relative to the library root.
    pub path: String,
    pub series: String,
    pub franchise: String,
    pub edition: String,
    pub kind: Kind,
    /// Inclusive.
    pub volumes: Option<(u32, u32)>,
    /// The suffix after the volume marker: `b`, `_DC`, `s`, `A`.
    pub copy: Option<String>,
    pub chapter: Option<String>,
    pub in_volume: Option<u32>,
    /// A chapter printed after this volume, the last its series holds, so in
    /// none of the series' volumes or packs.
    #[serde(default)]
    pub after_volume: Option<u32>,
    pub issue: Option<u32>,
    pub issues: Option<Vec<u32>>,
    pub ltr: bool,
    pub split: Split,
    /// How `in_volume`, `after_volume` or `issues` was set: `name`,
    /// `contents:<path>` or `indicia:<path>`.
    pub truth: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Adjudication {
    pub a: String,
    pub b: String,
    pub same: bool,
    pub date: String,
    pub by: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub members: Vec<Member>,
    pub adjudications: Vec<Adjudication>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) enum Label {
    SameVolume {
        copy: bool,
    },
    Neighbour,
    SameSeriesOther,
    /// Two packs of one series whose ranges overlap but differ: they share whole
    /// volumes without being one book, so no rule can call them right or wrong.
    PackOverlap,
    InPack,
    NotInPack,
    ChapterIn,
    ChapterNotIn,
    InCollected,
    NotInCollected,
    OtherSeries {
        franchise: bool,
    },
    CoverStories,
    Omnibus,
    Unlabelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) enum Source {
    Explicit,
    Leader,
    Both,
}

/// One library directory whose entries are members. `kind: None` takes the
/// kind from each name, as a raw directory mixes volumes, packs and chapters.
#[derive(Debug, Clone, Copy)]
pub(super) struct DirRow {
    pub dir: &'static str,
    pub series: &'static str,
    pub franchise: &'static str,
    pub edition: &'static str,
    pub kind: Option<Kind>,
    pub ltr: bool,
    pub split: Split,
}

const D: Split = Split::Dev;
const H: Split = Split::HeldOut;
const VOL: Option<Kind> = Some(Kind::Volume);
const NAME: Option<Kind> = None;
const CH: Option<Kind> = Some(Kind::Chapter);
const OMNI: Option<Kind> = Some(Kind::Omnibus);

const fn row(
    dir: &'static str,
    series: &'static str,
    franchise: &'static str,
    edition: &'static str,
    kind: Option<Kind>,
    split: Split,
) -> DirRow {
    DirRow {
        dir,
        series,
        franchise,
        edition,
        kind,
        ltr: false,
        split,
    }
}

/// Every directory measured, its names exactly as `ls` prints them. The
/// held-out series are `go-toubun`, `bocchi*`, `zelda-tp`, `bisque*` and
/// `invincible`.
pub(super) const TABLE: &[DirRow] = &[
    row("Volumes/Berserk", "berserk", "berserk", "en", VOL, D),
    row("Raw/Berserk", "berserk", "berserk", "raw", NAME, D),
    row(
        "Volumes/Bisque Doll de Chu ♡",
        "bisque-de-chu",
        "bisque",
        "en",
        VOL,
        H,
    ),
    row(
        "Volumes/Sono Bisuku Doru wa Koi o Suru",
        "bisque",
        "bisque",
        "en",
        VOL,
        H,
    ),
    row(
        "Volumes/Sono Bisuku Doru wa Koi o Suru (Semi-Colored)",
        "bisque",
        "bisque",
        "en-colour",
        VOL,
        H,
    ),
    row(
        "Chapters/Sono Bisque Doll wa Koi wo Suru - Fan Translated",
        "bisque",
        "bisque",
        "en-fan-ch",
        CH,
        H,
    ),
    row("Volumes/Bocchi the Rock!", "bocchi", "bocchi", "en", VOL, H),
    row(
        "Chapters/Bocchi the Rock! - Fan Translated",
        "bocchi",
        "bocchi",
        "en-fan-ch",
        CH,
        H,
    ),
    row(
        "Volumes/Bocchi the Rock! - Anthology Comic",
        "bocchi-anthology",
        "bocchi",
        "en",
        VOL,
        H,
    ),
    row(
        "Volumes/Bocchi the Rock! - Gaiden - Hiroi Kikuri no Fukazake Nikki",
        "bocchi-gaiden",
        "bocchi",
        "en",
        VOL,
        H,
    ),
    row(
        "Volumes/Chainsaw Man",
        "chainsaw-man",
        "chainsaw-man",
        "en",
        VOL,
        D,
    ),
    row(
        "Volumes/Chainsaw Man - Digital Colored Comics",
        "chainsaw-man",
        "chainsaw-man",
        "en-colour",
        VOL,
        D,
    ),
    row(
        "Volumes/Death Note",
        "death-note",
        "death-note",
        "en",
        VOL,
        D,
    ),
    row(
        "Volumes/Death Note (Colored Council)",
        "death-note",
        "death-note",
        "en-colour",
        VOL,
        D,
    ),
    row(
        "Volumes/Dungeon Meshi",
        "dungeon-meshi",
        "dungeon-meshi",
        "en",
        VOL,
        D,
    ),
    row(
        "Chapters/Dungeon Meshi - Fan Translated",
        "dungeon-meshi",
        "dungeon-meshi",
        "en-fan-ch",
        CH,
        D,
    ),
    row(
        "Volumes/Go-Toubun no Hanayome",
        "go-toubun",
        "go-toubun",
        "en",
        VOL,
        H,
    ),
    row(
        "Raw/Go-Toubun no Hanayome",
        "go-toubun",
        "go-toubun",
        "raw",
        NAME,
        H,
    ),
    row(
        "Chapters/Go-Toubun no Hanayome - Fan Translated",
        "go-toubun",
        "go-toubun",
        "en-fan-ch",
        CH,
        H,
    ),
    row(
        "Volumes/Go-Toubun no Hanayome - Full Color Edition",
        "go-toubun",
        "go-toubun",
        "en-colour",
        VOL,
        H,
    ),
    row(
        "Raw/Go-Toubun no Hanayome - Full Color",
        "go-toubun",
        "go-toubun",
        "raw-colour",
        NAME,
        H,
    ),
    row(
        "Volumes/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen",
        "kaguya",
        "kaguya",
        "en",
        VOL,
        D,
    ),
    row(
        "Raw/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen",
        "kaguya",
        "kaguya",
        "raw",
        NAME,
        D,
    ),
    row(
        "Chapters/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - Fan Translated",
        "kaguya",
        "kaguya",
        "en-fan-ch",
        CH,
        D,
    ),
    row(
        "Volumes/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - Full Color",
        "kaguya",
        "kaguya",
        "en-colour",
        VOL,
        D,
    ),
    row(
        "Volumes/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - Official Doujin",
        "kaguya-doujin",
        "kaguya",
        "en",
        VOL,
        D,
    ),
    row(
        "Volumes/Kobayashi-san Chi no Meidoragon",
        "kobayashi",
        "kobayashi",
        "en",
        VOL,
        D,
    ),
    row(
        "Raw/Kobayashi-san Chi no Meidoragon",
        "kobayashi",
        "kobayashi",
        "raw",
        NAME,
        D,
    ),
    row(
        "Chapters/Kobayashi-san cno Meidoragon - Fan Translated",
        "kobayashi",
        "kobayashi",
        "en-fan-ch",
        CH,
        D,
    ),
    row(
        "Volumes/Kobayashi-san Chi no Meidoragon - Lucoa wa Boku no xx Desu",
        "kobayashi-lucoa",
        "kobayashi",
        "en",
        VOL,
        D,
    ),
    row(
        "Volumes/Kobayashi-san Chi no Meidoragon - Okomori Gurashi no Fafnir",
        "kobayashi-fafnir",
        "kobayashi",
        "en",
        VOL,
        D,
    ),
    row(
        "Volumes/Megami no Cafe Terrace",
        "megami",
        "megami",
        "en",
        VOL,
        D,
    ),
    row(
        "Chapters/Megami Cafe no Terrace",
        "megami",
        "megami",
        "en-fan-ch",
        CH,
        D,
    ),
    row("Volumes/Monster", "monster", "monster", "en", VOL, D),
    row("Raw/Monster", "monster", "monster", "raw", NAME, D),
    row(
        "Volumes/Monster - The Perfect Edition",
        "monster-perfect",
        "monster",
        "en",
        OMNI,
        D,
    ),
    row(
        "Raw/Monster - Kanzenban",
        "monster-kanzenban",
        "monster",
        "raw",
        OMNI,
        D,
    ),
    row("Volumes/One Piece", "one-piece", "one-piece", "en", VOL, D),
    row(
        "Chapters/One Piece",
        "one-piece",
        "one-piece",
        "en-fan-ch",
        CH,
        D,
    ),
    row(
        "Volumes/One Piece - Cover Stories - Digital Colored Comics",
        "one-piece-cover-stories",
        "one-piece",
        "en-colour",
        Some(Kind::Extra),
        D,
    ),
    row(
        "Volumes/One-Punch Man",
        "one-punch-man",
        "one-punch-man",
        "en",
        VOL,
        D,
    ),
    row(
        "Chapters/One Punch Man",
        "one-punch-man",
        "one-punch-man",
        "en-fan-ch",
        CH,
        D,
    ),
    row(
        "Volumes/[Oshi no Ko]",
        "oshi-no-ko",
        "oshi-no-ko",
        "en",
        VOL,
        D,
    ),
    row(
        "Raw/[Oshi no Ko]",
        "oshi-no-ko",
        "oshi-no-ko",
        "raw",
        NAME,
        D,
    ),
    row(
        "Chapters/[Oshi no Ko] - Fan Translated",
        "oshi-no-ko",
        "oshi-no-ko",
        "en-fan-ch",
        CH,
        D,
    ),
    row(
        "Volumes/[Oshi no Ko] - Fan Translated",
        "oshi-no-ko",
        "oshi-no-ko",
        "en-fan",
        VOL,
        D,
    ),
    row(
        "Volumes/The Legend of Zelda - Twilight Princess",
        "zelda-tp",
        "zelda-tp",
        "en",
        VOL,
        H,
    ),
    row(
        "Raw/Zelda no Densetsu - Twilight Princess",
        "zelda-tp",
        "zelda-tp",
        "raw",
        NAME,
        H,
    ),
    row(
        "Volumes/Uzumaki (Deluxe Edition 3-in-1)",
        "uzumaki",
        "uzumaki",
        "en",
        OMNI,
        D,
    ),
    row(
        "Volumes/Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)",
        "aot-kui-naki",
        "aot-kui-naki",
        "en-colour",
        VOL,
        D,
    ),
    row("Volumes/Dandadan", "dandadan", "dandadan", "en", VOL, D),
    row(
        "Volumes/Hunter × Hunter - Kurapika's Memories",
        "hunter-x-hunter-kurapikas-memories",
        "hunter-x-hunter-kurapikas-memories",
        "en",
        VOL,
        D,
    ),
    row(
        "Volumes/Kabushikigaisha Magi-Lumière",
        "kabushikigaisha-magi-lumiere",
        "kabushikigaisha-magi-lumiere",
        "en",
        VOL,
        D,
    ),
    row("Volumes/Look Back", "look-back", "look-back", "en", VOL, D),
    row(
        "Volumes/Sayonara, Eri",
        "sayonara-eri",
        "sayonara-eri",
        "en",
        VOL,
        D,
    ),
    row("Volumes/Yotsuba&!", "yotsuba", "yotsuba", "en", VOL, D),
    DirRow {
        dir: "Comics/Issues/Invincible/Invincible (2003-2018)",
        series: "invincible",
        franchise: "invincible",
        edition: "en",
        kind: Some(Kind::Issue),
        ltr: true,
        split: H,
    },
    DirRow {
        dir: "Comics/Collected/Invincible",
        series: "invincible",
        franchise: "invincible",
        edition: "en",
        kind: Some(Kind::Collected),
        ltr: true,
        split: H,
    },
];

pub(super) const ARCHIVES: [&str; 6] = ["cbz", "zip", "cbr", "rar", "cb7", "7z"];
const IMAGES: [&str; 8] = ["jpg", "jpeg", "png", "webp", "gif", "avif", "jxl", "bmp"];

/// A member drafted from its name and its directory's row. A name that gives
/// no number its kind needs leaves that number `None`.
pub(super) fn draft_member(relative: &str, row: &DirRow) -> Member {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    let text = without_parens(stem(name));
    let volume = first(&text, volume_at);
    let chapter = first(&text, chapter_at);
    let chapter_pack = first(&text, chapter_pack_at);
    let kind = match row.kind {
        None if chapter_pack.is_some() => Kind::ChapterPack,
        None if chapter.is_some() => Kind::Chapter,
        None | Some(Kind::Volume) if volume.as_ref().is_some_and(|v| v.first != v.last) => {
            Kind::Pack
        }
        None => Kind::Volume,
        Some(kind) => kind,
    };
    let colour = row.edition == "raw" && name.to_ascii_lowercase().contains("color");
    let mut m = Member {
        path: relative.to_owned(),
        series: row.series.to_owned(),
        franchise: row.franchise.to_owned(),
        edition: (if colour { "raw-colour" } else { row.edition }).to_owned(),
        kind,
        volumes: None,
        copy: None,
        chapter: None,
        in_volume: None,
        after_volume: None,
        issue: None,
        issues: None,
        ltr: row.ltr,
        split: row.split,
        truth: None,
    };
    match kind {
        Kind::Volume | Kind::Pack | Kind::Omnibus | Kind::Collected => {
            if let Some(v) = volume {
                m.volumes = Some((v.first, v.last));
                m.copy = v.copy;
            }
        }
        Kind::Chapter => {
            // A fractional or zero chapter is an extra that a printed volume
            // may leave out or place elsewhere, so its name is not truth.
            let whole = chapter
                .as_deref()
                .is_some_and(|c| !c.contains('.') && c != "0");
            if let Some(v) = volume.filter(|v| whole && v.first == v.last) {
                m.in_volume = Some(v.first);
                m.truth = Some("name".to_owned());
            }
            m.chapter = chapter;
        }
        Kind::ChapterPack => m.chapter = chapter_pack,
        Kind::Issue => m.issue = issue_number(&text, row.dir),
        Kind::Extra => {}
    }
    m
}

fn unclassified(m: &Member) -> bool {
    match m.kind {
        Kind::Volume | Kind::Pack | Kind::Omnibus | Kind::Collected => m.volumes.is_none(),
        Kind::Chapter | Kind::ChapterPack => m.chapter.is_none(),
        Kind::Issue => m.issue.is_none(),
        Kind::Extra => false,
    }
}

fn stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if ARCHIVES.contains(&ext.to_ascii_lowercase().as_str()) => stem,
        _ => name,
    }
}

/// `name` with every parenthesised group blanked, so that a `(2023)` year or a
/// `(2003-2018)` run never reads as a number.
fn without_parens(name: &str) -> String {
    let mut depth = 0_u32;
    (name.chars())
        .map(|c| {
            let inside = depth > 0 || c == '(';
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if inside { ' ' } else { c }
        })
        .collect()
}

#[derive(Debug)]
struct VolumeMark {
    first: u32,
    last: u32,
    copy: Option<String>,
}

fn first<T>(text: &str, at: fn(&[u8], usize) -> Option<T>) -> Option<T> {
    let b = text.as_bytes();
    (0..b.len()).find_map(|i| at(b, i))
}

/// A marker letter starts a token only after a non-alphanumeric byte, so the
/// `v` in `Dev01` or the `c` in `Dc5` is part of a word, not a marker.
fn starts_token(b: &[u8], i: usize) -> bool {
    i == 0 || !b[i - 1].is_ascii_alphanumeric()
}

fn ends_token(b: &[u8], i: usize) -> bool {
    b.get(i).is_none_or(|c| !c.is_ascii_alphanumeric())
}

fn digits_end(b: &[u8], i: usize) -> usize {
    i + b[i..].iter().take_while(|c| c.is_ascii_digit()).count()
}

fn number(b: &[u8], i: usize) -> Option<(u32, usize)> {
    let end = digits_end(b, i);
    let n = std::str::from_utf8(&b[i..end]).ok()?.parse().ok()?;
    Some((n, end))
}

/// `v01`, `v01-03`, and a copy suffix of letters or `_` and letters straight
/// after the number: `v01b`, `v01_DC`, `v01-07A`.
fn volume_at(b: &[u8], i: usize) -> Option<VolumeMark> {
    if b[i] != b'v' || !starts_token(b, i) {
        return None;
    }
    let (first, mut j) = number(b, i + 1)?;
    let mut last = first;
    if b.get(j) == Some(&b'-')
        && let Some((n, k)) = number(b, j + 1)
    {
        (last, j) = (n, k);
    }
    let suffix = j;
    let letters = j + usize::from(b.get(j) == Some(&b'_'));
    let end = letters
        + b[letters..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
    if end > letters {
        j = end;
    }
    let copy = (j > suffix).then(|| String::from_utf8_lossy(&b[suffix..j]).into_owned());
    ends_token(b, j).then_some(VolumeMark { first, last, copy })
}

/// `c152` or `c5.5`, without the leading zeros: `c000` is chapter `0`.
fn chapter_at(b: &[u8], i: usize) -> Option<String> {
    if b[i] != b'c' || !starts_token(b, i) {
        return None;
    }
    let whole = digits_end(b, i + 1);
    let mut end = whole;
    if b.get(whole) == Some(&b'.') && digits_end(b, whole + 1) > whole + 1 {
        end = digits_end(b, whole + 1);
    }
    if whole == i + 1 || !ends_token(b, end) {
        return None;
    }
    let digits = std::str::from_utf8(&b[i + 1..whole])
        .ok()?
        .trim_start_matches('0');
    let fraction = std::str::from_utf8(&b[whole..end]).ok()?;
    Some(format!(
        "{}{fraction}",
        if digits.is_empty() { "0" } else { digits }
    ))
}

fn chapter_pack_at(b: &[u8], i: usize) -> Option<String> {
    if !b[i..].starts_with(b"ch") || !starts_token(b, i) {
        return None;
    }
    let (first, j) = number(b, i + 2)?;
    let (last, k) = (b.get(j) == Some(&b'-'))
        .then(|| number(b, j + 1))
        .flatten()?;
    ends_token(b, k).then(|| format!("{first}-{last}"))
}

/// The number straight after the run's name: `Invincible 001` in the run
/// `Invincible (2003-2018)`.
fn issue_number(text: &str, dir: &str) -> Option<u32> {
    let run = without_parens(dir.rsplit('/').next()?);
    let rest = text.strip_prefix(run.trim())?.trim_start();
    let (n, end) = number(rest.as_bytes(), 0)?;
    ends_token(rest.as_bytes(), end).then_some(n)
}

/// Whether a name marks an omnibus, which collects several ordinary volumes
/// in one book: `(?i)kanzenban|perfect edition|deluxe|\b\d-in-1\b|omnibus`.
pub(super) fn is_omnibus_name(dir_and_file: &str) -> bool {
    let lower = dir_and_file.to_lowercase();
    let b = lower.as_bytes();
    let word = |c: &u8| c.is_ascii_alphanumeric() || *c == b'_';
    let n_in_1 = lower.match_indices("-in-1").any(|(i, token)| {
        let digit = i >= 1 && b[i - 1].is_ascii_digit();
        digit && (i < 2 || !word(&b[i - 2])) && b.get(i + token.len()).is_none_or(|c| !word(c))
    });
    n_in_1
        || ["kanzenban", "perfect edition", "deluxe", "omnibus"]
            .iter()
            .any(|w| lower.contains(w))
}

fn volume_like(kind: Kind) -> bool {
    matches!(kind, Kind::Volume | Kind::Collected)
}

fn directory(m: &Member) -> &str {
    m.path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The pair's label: the first label line its names match, unless an
/// adjudication names the pair, either way round. A pair the names left
/// unlabelled takes either verdict; one they labelled is relabelled only by
/// a "same" (or "inside") verdict, the one relabel the owner may make, so a
/// "different" keeps the names' label.
pub(super) fn label(m: &Manifest, a: &Member, b: &Member) -> Label {
    let from_names = by_names(a, b);
    match verdict_on(m, &a.path, &b.path) {
        Some(j) if j.same || from_names == Label::Unlabelled => adjudicated(j.same, a, b),
        _ => from_names,
    }
}

/// The adjudication naming the pair of paths `a` and `b`, either way round.
pub(super) fn verdict_on<'m>(m: &'m Manifest, a: &str, b: &str) -> Option<&'m Adjudication> {
    (m.adjudications.iter()).find(|j| (j.a == a && j.b == b) || (j.a == b && j.b == a))
}

/// Which member a verdict on the pair says is or is not inside the other:
/// `Some(true)` for `a`, `Some(false)` for `b`, `None` when neither can hold
/// the other and the verdict is same or different.
pub(super) fn held(a: &Member, b: &Member) -> Option<bool> {
    if inside(a, b).is_some() {
        Some(true)
    } else {
        inside(b, a).map(|_| false)
    }
}

type Line = fn(&Member, &Member) -> Option<Label>;

/// Each line is tried both ways round and never answers differently for the
/// two, which is what makes a label the same whichever member comes first.
fn by_names(a: &Member, b: &Member) -> Label {
    const LINES: [Line; 7] = [
        chapter_line,
        issue_line,
        pack_line,
        overlap_line,
        volume_line,
        family_line,
        series_line,
    ];
    (LINES.iter())
        .find_map(|line| line(a, b).or_else(|| line(b, a)))
        .unwrap_or(Label::Unlabelled)
}

fn chapter_line(x: &Member, y: &Member) -> Option<Label> {
    if x.series != y.series {
        return None;
    }
    match (x.kind, y.kind) {
        (Kind::Chapter, Kind::Chapter) | (Kind::Issue, Kind::Issue) => Some(Label::ChapterNotIn),
        (Kind::Chapter, kind) if volume_like(kind) || kind == Kind::Pack => {
            let (first, last) = y.volumes?;
            // A volume numbered past `after_volume` is one the library does
            // not hold, so nothing says whether it prints the chapter.
            if let Some(m) = x.after_volume {
                return (last <= m).then_some(Label::ChapterNotIn);
            }
            let n = x.in_volume?;
            Some(if (first..=last).contains(&n) {
                Label::ChapterIn
            } else {
                Label::ChapterNotIn
            })
        }
        _ => None,
    }
}

fn issue_line(x: &Member, y: &Member) -> Option<Label> {
    if x.series != y.series || x.kind != Kind::Issue || y.kind != Kind::Collected {
        return None;
    }
    let collected = y.issues.as_ref()?.contains(&x.issue?);
    Some(if collected {
        Label::InCollected
    } else {
        Label::NotInCollected
    })
}

fn pack_line(x: &Member, y: &Member) -> Option<Label> {
    if x.series != y.series || !volume_like(x.kind) || y.kind != Kind::Pack {
        return None;
    }
    let ((n, _), (first, last)) = (x.volumes?, y.volumes?);
    Some(if (first..=last).contains(&n) {
        Label::InPack
    } else {
        Label::NotInPack
    })
}

fn overlap_line(x: &Member, y: &Member) -> Option<Label> {
    if x.series != y.series || x.kind != Kind::Pack || y.kind != Kind::Pack {
        return None;
    }
    let (p, q) = (x.volumes?, y.volumes?);
    (p != q && p.0.max(q.0) <= p.1.min(q.1)).then_some(Label::PackOverlap)
}

fn volume_line(x: &Member, y: &Member) -> Option<Label> {
    let alike = (volume_like(x.kind) && volume_like(y.kind))
        || (x.kind == Kind::Pack && y.kind == Kind::Pack);
    if x.series != y.series || !alike {
        return None;
    }
    let (p, q) = (x.volumes?, y.volumes?);
    Some(if p == q {
        Label::SameVolume {
            copy: directory(x) == directory(y),
        }
    } else if p.0 == p.1 && q.0 == q.1 && p.0.abs_diff(q.0) == 1 {
        Label::Neighbour
    } else {
        Label::SameSeriesOther
    })
}

fn family_line(x: &Member, y: &Member) -> Option<Label> {
    let either = |kind| x.kind == kind || y.kind == kind;
    if x.franchise != y.franchise {
        None
    } else if either(Kind::Omnibus) {
        Some(Label::Omnibus)
    } else if either(Kind::Extra) {
        Some(Label::CoverStories)
    } else {
        None
    }
}

fn series_line(x: &Member, y: &Member) -> Option<Label> {
    (x.series != y.series).then(|| Label::OtherSeries {
        franchise: x.franchise == y.franchise,
    })
}

/// The labels saying whether `x` lies inside `y`, for a shape where it can.
fn inside(x: &Member, y: &Member) -> Option<(Label, Label)> {
    let holds_chapters = volume_like(y.kind)
        || y.kind == Kind::Pack
        || (x.kind == Kind::Chapter && y.kind == Kind::ChapterPack);
    match x.kind {
        Kind::Chapter | Kind::ChapterPack if holds_chapters => {
            Some((Label::ChapterIn, Label::ChapterNotIn))
        }
        Kind::Issue if y.kind == Kind::Collected => {
            Some((Label::InCollected, Label::NotInCollected))
        }
        kind if volume_like(kind) && y.kind == Kind::Pack => {
            Some((Label::InPack, Label::NotInPack))
        }
        _ => None,
    }
}

/// A person's answer for a pair the names could not settle: inside or not
/// where one member can hold the other, else the same book or not.
fn adjudicated(same: bool, a: &Member, b: &Member) -> Label {
    if let Some((yes, no)) = inside(a, b).or_else(|| inside(b, a)) {
        return if same { yes } else { no };
    }
    if same {
        Label::SameVolume {
            copy: directory(a) == directory(b),
        }
    } else if a.series == b.series {
        Label::SameSeriesOther
    } else {
        Label::OtherSeries {
            franchise: a.franchise == b.franchise,
        }
    }
}

pub(super) fn split_of(a: &Member, b: &Member) -> Split {
    if a.split == Split::HeldOut || b.split == Split::HeldOut {
        Split::HeldOut
    } else {
        Split::Dev
    }
}

fn single(x: &Member) -> Option<u32> {
    match x.volumes {
        Some((n, last)) if n == last && volume_like(x.kind) => Some(n),
        _ => None,
    }
}

fn holding<'a>(ms: &'a [Member], series: &'a str, n: u32) -> impl Iterator<Item = usize> + 'a {
    (ms.iter().enumerate())
        .filter(move |(_, x)| x.series == series && single(x) == Some(n))
        .map(|(i, _)| i)
}

/// The pairs the plan names before any page is read, numbered as its rules.
pub(super) fn explicit_pairs(m: &Manifest) -> Vec<(usize, usize)> {
    let ms = &m.members;
    let mut pairs = std::collections::BTreeSet::new();
    let mut add = |i: usize, j: usize| {
        if i != j {
            pairs.insert((i.min(j), i.max(j)));
        }
    };
    let series_in = |split: Split| {
        let mut keys: Vec<&str> = ms
            .iter()
            .filter(|x| x.split == split)
            .map(|x| x.series.as_str())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    };
    let (dev, held_out) = (series_in(Split::Dev), series_in(Split::HeldOut));

    for (i, x) in ms.iter().enumerate() {
        // 6, a spin-off: its main series' v01 even when it has no number of its
        // own, and vN when it has one.
        if volume_like(x.kind) && x.series != x.franchise {
            for k in std::iter::once(1).chain(single(x)) {
                holding(ms, &x.franchise, k).take(1).for_each(|j| add(i, j));
            }
        }
        let Some(n) = single(x) else { continue };
        for (j, y) in ms.iter().enumerate().filter(|(_, y)| y.series == x.series) {
            // 1: vN in two editions, and each side's vN against the other's vN±1.
            if y.edition != x.edition && single(y) == Some(n) {
                add(i, j);
                let near = |k: &usize| ms[*k].edition == y.edition;
                for k in n.checked_sub(1).into_iter().chain([n + 1]) {
                    holding(ms, &y.series, k)
                        .filter(near)
                        .for_each(|k| add(i, k));
                }
            }
            // 2: copies of one number in one directory.
            if directory(y) == directory(x) && single(y) == Some(n) {
                add(i, j);
            }
        }
        // 6: the next two series of its split holding vN.
        let same_split = if x.split == Split::Dev {
            &dev
        } else {
            &held_out
        };
        let at = same_split.iter().position(|k| *k == x.series).unwrap_or(0);
        let after = same_split[at + 1..].iter().chain(&same_split[..at]);
        for j in after.filter_map(|k| holding(ms, k, n).next()).take(2) {
            add(i, j);
        }
    }
    for (i, x) in ms.iter().enumerate() {
        for (j, y) in ms.iter().enumerate() {
            let in_range = |n: u32, (first, last): (u32, u32)| (first..=last).contains(&n);
            let held = match (x.kind, x.volumes, x.in_volume) {
                // 3: a pack against its series' singles inside it and either side of it.
                (Kind::Pack, Some((first, last)), _) => {
                    y.series == x.series
                        && single(y).is_some_and(|n| n + 1 >= first && n <= last + 1)
                }
                // 4: an omnibus vN against its franchise's ordinary vN, v(2N−1) and v(2N).
                (Kind::Omnibus, Some((n, _)), _) => {
                    let covered = |v: u32| v == n || v == 2 * n || v + 1 == 2 * n;
                    y.franchise == x.franchise
                        && y.kind == Kind::Volume
                        && single(y).is_some_and(covered)
                }
                // 5: a chapter against its vN±1 in every edition and every pack
                // holding N; one after the last held vM against every vM and
                // every pack holding M.
                (Kind::Chapter, ..) => {
                    let at = (x.in_volume.map(|n| (n, 1))).or(x.after_volume.map(|m| (m, 0)));
                    at.is_some_and(|(n, reach)| {
                        let near = single(y).is_some_and(|v| v.abs_diff(n) <= reach);
                        let pack =
                            y.kind == Kind::Pack && y.volumes.is_some_and(|r| in_range(n, r));
                        y.series == x.series && (near || pack)
                    })
                }
                // 7: a Cover Stories file against every volume of its franchise.
                (Kind::Extra, ..) => y.franchise == x.franchise && y.kind == Kind::Volume,
                // 8: an issue against each collected edition of its series.
                (Kind::Issue, ..) => y.series == x.series && y.kind == Kind::Collected,
                _ => false,
            };
            if held {
                add(i, j);
            }
        }
    }
    pairs.into_iter().collect()
}

/// The explicit pairs and the lookup's leaders as one list, each unordered pair
/// once, with where it came from.
pub(super) fn scored_pairs(
    explicit: &[(usize, usize)],
    leaders: &[(usize, usize)],
) -> Vec<((usize, usize), Source)> {
    let key = |&(i, j): &(usize, usize)| (i.min(j), i.max(j));
    let mut out = std::collections::BTreeMap::new();
    for pair in explicit {
        out.insert(key(pair), Source::Explicit);
    }
    for pair in leaders {
        (out.entry(key(pair)))
            .and_modify(|s| {
                if *s == Source::Explicit {
                    *s = Source::Both;
                }
            })
            .or_insert(Source::Leader);
    }
    out.into_iter().collect()
}

/// What a label counts as under one rule: `Some(true)` a positive,
/// `Some(false)` a negative, `None` outside the rule. `a` is the oriented
/// short side; the pack rule reads `b` too, since only a pack is its B.
pub(super) fn class(
    rule: Rule,
    label: Label,
    a: &Member,
    b: &Member,
    f: &PairFeatures,
) -> Option<bool> {
    use Label as L;
    match rule {
        Rule::Volume => match label {
            L::SameVolume { .. } => Some(true),
            L::Omnibus | L::PackOverlap | L::Unlabelled => None,
            _ => Some(false),
        },
        Rule::Chapter => {
            let short = f.a_slots < MIN_CHAPTER_SLOTS;
            if short || !matches!(a.kind, Kind::Chapter | Kind::Issue) {
                return None;
            }
            match label {
                L::ChapterIn | L::InCollected => Some(true),
                L::ChapterNotIn | L::NotInCollected | L::OtherSeries { .. } | L::CoverStories => {
                    Some(false)
                }
                _ => None,
            }
        }
        Rule::Pack => {
            if a.kind != Kind::Volume || b.kind != Kind::Pack {
                return None;
            }
            match label {
                L::InPack => Some(true),
                L::NotInPack | L::OtherSeries { .. } => Some(false),
                _ => None,
            }
        }
    }
}

/// One volume's printed truth, as transcribed and confirmed by the owner: the
/// chapters its contents page numbers, or the issues its indicia names. A
/// volume with no contents page has neither, and settles nothing.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct Printed {
    pub path: String,
    pub first: Option<u32>,
    pub last: Option<u32>,
    pub issues: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct Transcription {
    pub volumes: Vec<Printed>,
}

/// Chapter files numbered behind the print: file chapter N is printed as
/// chapter N + `by`, so ranges are compared in print numbers.
#[derive(Debug, Clone, Copy)]
pub(super) struct Renumbering {
    pub series: &'static str,
    pub edition: &'static str,
    pub by: u32,
    /// Written into the truth, so the shift shows wherever a label is read.
    pub tag: &'static str,
}

/// One-Punch Man's fan chapter N carries the title of print Punch N+7, as
/// three fan chapters checked against the contents pages show.
pub(super) const RENUMBERED: &[Renumbering] = &[Renumbering {
    series: "one-punch-man",
    edition: "en-fan-ch",
    by: 7,
    tag: "opm",
}];

#[derive(Debug, PartialEq, Eq)]
enum Fact {
    Issues(Vec<u32>),
    InVolume(u32),
    AfterVolume(u32),
}

/// A transcribed volume's printed chapters, inclusive.
#[derive(Debug)]
struct Range<'a> {
    series: &'a str,
    volume: u32,
    first: u32,
    last: u32,
    path: &'a str,
}

/// Sets what the printed pages settle: each indicia's issues on its collected
/// edition, `in_volume` on each whole loose chapter a printed range holds, and
/// `after_volume` on each one past the end of its series' last held volume.
/// A fractional or zero chapter is an extra a volume may print anywhere, its
/// last pages included, or not at all, so neither is set on it. A
/// transcription that contradicts itself or a name is refused, and nothing is
/// applied.
pub(super) fn apply_truth(
    m: &mut Manifest,
    t: &Transcription,
    renumbered: &[Renumbering],
) -> Result<(), String> {
    for (i, fact, truth) in facts(&m.members, t, renumbered)? {
        let x = &mut m.members[i];
        match fact {
            Fact::Issues(issues) => x.issues = Some(issues),
            Fact::InVolume(n) => x.in_volume = Some(n),
            Fact::AfterVolume(n) => x.after_volume = Some(n),
        }
        x.truth = Some(truth);
    }
    Ok(())
}

fn facts(
    ms: &[Member],
    t: &Transcription,
    renumbered: &[Renumbering],
) -> Result<Vec<(usize, Fact, String)>, String> {
    let mut out = Vec::new();
    let mut ranges = Vec::new();
    for p in &t.volumes {
        let path = p.path.as_str();
        let i = (ms.iter().position(|x| x.path == path))
            .ok_or_else(|| format!("{path}: transcribed, but not a member"))?;
        let x = &ms[i];
        let volume = single(x).ok_or_else(|| format!("{path}: not a single volume"))?;
        if let Some(issues) = &p.issues {
            if x.kind != Kind::Collected {
                return Err(format!("{path}: issues on a {:?}", x.kind));
            }
            out.push((i, Fact::Issues(issues.clone()), format!("indicia:{path}")));
        }
        match (p.first, p.last) {
            (Some(first), Some(last)) if first <= last => ranges.push(Range {
                series: &x.series,
                volume,
                first,
                last,
                path,
            }),
            (None, None) => {}
            (first, last) => return Err(format!("{path}: printed range {first:?}–{last:?}")),
        }
    }
    in_volume_order(&ranges)?;
    // Each series' last held volume M, with the range ending it: a chapter
    // printed after that end is in no volume the library holds.
    let mut ends: std::collections::BTreeMap<&str, &Range> = std::collections::BTreeMap::new();
    for r in &ranges {
        let held =
            |y: &&Member| y.series == r.series && (volume_like(y.kind) || y.kind == Kind::Pack);
        let last_held = ms
            .iter()
            .filter(held)
            .filter_map(|y| y.volumes)
            .map(|v| v.1)
            .max();
        if last_held == Some(r.volume) {
            let e = ends.entry(r.series).or_insert(r);
            if r.path < e.path {
                *e = r;
            }
        }
    }
    for (i, x) in ms.iter().enumerate() {
        let Some(chapter) = x.chapter.as_deref().filter(|_| x.kind == Kind::Chapter) else {
            continue;
        };
        let shift = (renumbered.iter()).find(|r| r.series == x.series && r.edition == x.edition);
        let by = shift.map_or(0, |r| r.by);
        let tag = shift.map_or_else(String::new, |r| format!("; {} +{}", r.tag, r.by));
        let Some(n) = (chapter.parse::<u32>().ok())
            .filter(|&n| n > 0)
            .map(|n| n + by)
        else {
            continue;
        };
        let holder = (ranges.iter())
            .filter(|r| r.series == x.series && r.first <= n && n <= r.last)
            .min_by_key(|r| r.path);
        let fact = match ends.get(x.series.as_str()) {
            Some(r) if n > r.last => Some((Fact::AfterVolume(r.volume), *r)),
            _ => holder.map(|r| (Fact::InVolume(r.volume), r)),
        };
        match (x.in_volume, fact) {
            (Some(named), Some((fact, r))) if fact != Fact::InVolume(named) => {
                return Err(format!(
                    "{}: v{named} by name, {fact:?} by {}",
                    x.path, r.path
                ));
            }
            (None, Some((fact, r))) => out.push((i, fact, format!("contents:{}{tag}", r.path))),
            _ => {}
        }
    }
    Ok(out)
}

/// Within a series a later volume prints later chapters, and copies of one
/// volume print one range; anything else is a misreading.
fn in_volume_order(ranges: &[Range]) -> Result<(), String> {
    for a in ranges {
        for b in ranges.iter().filter(|b| b.series == a.series) {
            let fits = match a.volume.cmp(&b.volume) {
                std::cmp::Ordering::Less => a.last < b.first,
                std::cmp::Ordering::Equal => (a.first, a.last) == (b.first, b.last),
                std::cmp::Ordering::Greater => true,
            };
            if !fits {
                return Err(format!(
                    "{} prints {}–{} and {} prints {}–{}",
                    a.path, a.first, a.last, b.path, b.first, b.last
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
pub(super) struct Draft {
    pub manifest: Manifest,
    /// Members per table directory, in table order.
    pub counts: Vec<(&'static str, usize)>,
    /// Members whose name gave no number their kind needs.
    pub unclassified: Vec<String>,
    /// Entries of a table directory that are not members, each with why.
    pub skipped: Vec<String>,
    pub missing: Vec<&'static str>,
    /// Directories at a table level that no row names.
    pub untabled: Vec<String>,
}

/// Every table directory's members, drafted from names alone: no archive is
/// opened. What is not a member is listed rather than failed, to be read
/// against `ls`.
pub(super) fn draft(library: &Path) -> Draft {
    let mut d = Draft::default();
    for row in TABLE {
        let Ok(entries) = std::fs::read_dir(library.join(row.dir)) else {
            d.missing.push(row.dir);
            continue;
        };
        let before = d.manifest.members.len();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    d.skipped.push(format!("{}: {e}", row.dir));
                    continue;
                }
            };
            let os_name = entry.file_name();
            let Some(name) = os_name.to_str() else {
                d.skipped.push(format!(
                    "{}/{}: not UTF-8",
                    row.dir,
                    os_name.to_string_lossy()
                ));
                continue;
            };
            let relative = format!("{}/{name}", row.dir);
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
            let is = |list: &[&str]| ext.as_deref().is_some_and(|e| list.contains(&e));
            let why = if name.starts_with('.') {
                "hidden"
            } else if entry.path().is_dir() || is(&ARCHIVES) {
                let m = draft_member(&relative, row);
                if unclassified(&m) {
                    d.unclassified.push(format!("{relative} ({:?})", m.kind));
                }
                d.manifest.members.push(m);
                continue;
            } else if is(&IMAGES) {
                "loose image"
            } else {
                "not an archive"
            };
            d.skipped.push(format!("{relative}: {why}"));
        }
        d.counts.push((row.dir, d.manifest.members.len() - before));
    }
    d.manifest.members.sort_by(|a, b| a.path.cmp(&b.path));
    d.unclassified.sort();
    d.skipped.sort();
    d.untabled = untabled(library);
    d
}

fn untabled(library: &Path) -> Vec<String> {
    let levels = [
        ("Volumes", 1),
        ("Raw", 1),
        ("Chapters", 1),
        ("Comics/Collected", 1),
        ("Comics/Issues", 2),
    ];
    let mut out = Vec::new();
    for (top, depth) in levels {
        let mut dirs = vec![top.to_owned()];
        for _ in 0..depth {
            dirs = dirs.iter().flat_map(|dir| subdirs(library, dir)).collect();
        }
        out.extend(
            dirs.into_iter()
                .filter(|dir| TABLE.iter().all(|row| row.dir != dir)),
        );
    }
    out.sort();
    out
}

fn subdirs(library: &Path, dir: &str) -> Vec<String> {
    (std::fs::read_dir(library.join(dir))
        .into_iter()
        .flatten()
        .flatten())
    .filter(|e| e.path().is_dir())
    .filter_map(|e| e.file_name().to_str().map(|name| format!("{dir}/{name}")))
    .collect()
}
