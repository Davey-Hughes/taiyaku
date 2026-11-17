//! The pages a person reads to settle what names cannot, and the sheets
//! they read them on.
//!
//! A loose chapter's name gives its number but not its volume; the volume's
//! printed contents page does, and a collected comic's indicia lists the
//! issues it collects. `truth_extract` writes the first pages of each volume
//! for someone to transcribe, and `truth_render` sets each named page beside
//! the range transcribed from it for the owner to confirm. `pairing_sheet`
//! is what a person judges an unlabelled pair on: pages and names only, so
//! no figure the measurement produced can lean on the verdict.
//!
//! The sheets are self-contained HTML opened locally, never published:
//! they are page images.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use image::DynamicImage;
use rayon::prelude::*;
use serde::Deserialize;
use taiyaku_read::{MAX_DECODE_ALLOC, member_page, member_pages};

use super::lookup::edition_paths;

/// Small print on a contents page or an indicia has to stay legible to
/// whoever transcribes it, hence the width and a quality above the contact
/// sheet's 70.
const EXTRACT_WIDTH: u32 = 1000;
const QUALITY: u8 = 90;
/// A named page on the truth sheet, beside its transcribed range.
const SHEET_WIDTH: u32 = 900;
const SPREAD_WIDTH: u32 = 2 * SHEET_WIDTH;
/// A pairing is judged on the artwork, as on the contact sheet.
const PAIR_WIDTH: u32 = crate::report::THUMB_WIDTH;
/// B pages shown either side of a pair's stretch, so a stretch that stops
/// short of its chapter or runs past it shows.
const CONTEXT: u32 = 3;

/// The volumes whose first pages are extracted for transcription.
///
/// Where a series' loose chapters run past its volumes, the list goes back
/// from the last held volume to beyond the one estimated to hold the lowest
/// loose chapter, because an estimate can fall short and a page nobody
/// extracted cannot be transcribed.
pub(super) const VOLUMES: &[&str] = &[
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2018).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v02 (2018).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v03 (2018).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v04 (2018).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v05 (2018).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v06 (2019).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v07 (2019).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v08 (2019).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v09 (2019).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v10 (2019).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v11 (2020).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v12 (2020).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v13 (2020).cbz",
    "Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v14 (2020).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v01 (2017).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v02 (2017).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v03 (2017).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v04 (2018).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v05 (2018).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v06 (2018).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v07 (2019).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v08 (2020).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v09 (2021).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v10 (2022).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v11 (2022).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v12 (2023).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v13 (2024).cbz",
    "Volumes/Dungeon Meshi/Dungeon Meshi - v14 (2024).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v01 (2020).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v02 (2020).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v03 (2020).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v04 (2021).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v05 (2022).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v06 (2022).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v07 (2023).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v08 (2023).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v09 (2023).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v10 (2023).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v11 (2024).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v12 (2024).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v13 (2025).cbz",
    "Volumes/Sono Bisuku Doru wa Koi o Suru/Sono Bisuku Doru wa Koi o Suru - v14 (2025).cbz",
    "Volumes/One Piece/One Piece - v109 (2025).cbz",
    "Volumes/One Piece/One Piece - v110 (2025).cbz",
    "Volumes/One Piece/One Piece - v111 (2026).cbz",
    "Volumes/Megami no Cafe Terrace/Megami no Cafe Terrace - v14 (2024).cbz",
    "Volumes/Megami no Cafe Terrace/Megami no Cafe Terrace - v15 (2024).cbz",
    "Volumes/Megami no Cafe Terrace/Megami no Cafe Terrace - v16 (2025).cbz",
    "Volumes/One-Punch Man/One-Punch Man - v27 (2024).cbz",
    "Volumes/One-Punch Man/One-Punch Man - v28 (2024).cbz",
    "Volumes/One-Punch Man/One-Punch Man - v29 (2024).cbz",
    "Volumes/One-Punch Man/One-Punch Man - v30 (2025).cbz",
    "Volumes/One-Punch Man/One-Punch Man - v31 (2025).cbz",
    "Volumes/One-Punch Man/One-Punch Man - v32 (2025).cbz",
    "Volumes/One-Punch Man/One-Punch Man - v33 (2026).cbz",
    "Volumes/[Oshi no Ko]/[Oshi no Ko] - v12 (2025).cbz",
    "Volumes/[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v13.cbz",
    "Raw/[Oshi no Ko]/Oshinoko v13",
    "Raw/[Oshi no Ko]/Oshinoko v13A",
    "Volumes/Kobayashi-san Chi no Meidoragon/Kobayashi-san Chi no Meidoragon - v16 (2025).cbz",
    "Raw/Kobayashi-san Chi no Meidoragon/DLRAW.TO_Kobayashi-san_Maid_Drago_v16",
    "Raw/Kobayashi-san Chi no Meidoragon/DLRAW.TO_Kobayashi-san_Maid_Drago_v17",
    "Volumes/Bocchi the Rock!/Bocchi the Rock! - v06 (2025).cbz",
    "Volumes/Bocchi the Rock!/Bocchi the Rock! - v07 (2025).cbz",
    "Comics/Collected/Invincible/Invincible v01 - Family Matters (2005, 3rd Printing) (digital) (Minutemen-Slayer).cbz",
    "Comics/Collected/Invincible/Invincible v02 - Eight is Enough (2005, 3rd Printing) (digital) (Minutemen-Slayer).cbz",
    "Comics/Collected/Invincible/Invincible v03 - Perfect Strangers (2004) (digital) (Minutemen-Slayer).cbz",
];

/// One volume as transcribed from its extracted pages.
#[derive(Debug, Deserialize)]
struct Transcribed {
    path: String,
    /// The `pNN` numbers of its contents or indicia pages; none when its
    /// first pages hold neither.
    #[serde(default)]
    pages: Vec<u32>,
    first: Option<f64>,
    last: Option<f64>,
    issues: Option<Vec<u32>>,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Transcription {
    volumes: Vec<Transcribed>,
}

/// What a person is asked of a pair. Where one member can hold the other,
/// whether A lies inside B, since that is what a verdict there decides
/// (`adjudicated`); else whether the two are one book. Either yes is
/// recorded as `same: true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Question {
    Inside,
    Same,
}

/// A pair for a person to judge: `a_pages[i]` is the A page the alignment
/// paired with `b_pages[i]`, and `a_count` and `b_count` are the pages the
/// alignment read from each member. Paths are relative to the library.
#[derive(Debug)]
pub(super) struct SheetPair {
    pub a: String,
    pub b: String,
    pub question: Question,
    pub a_pages: Vec<u32>,
    pub b_pages: Vec<u32>,
    pub a_count: u32,
    pub b_count: u32,
}

/// The first `pages` pages of each volume as `<out>/<slug>/pNN.jpg`, `NN`
/// being the page's index in the member, and `<out>/index.tsv` naming the
/// volume behind each slug.
///
/// A volume that cannot be read is listed in the index with its error and
/// the rest are still written; the error is returned once all are done.
pub(super) fn truth_extract(
    library: &Path,
    out: &Path,
    volumes: &[&str],
    pages: u32,
) -> Result<(), String> {
    let mut slugs = BTreeMap::new();
    for &volume in volumes {
        if let Some(other) = slugs.insert(slug(volume), volume) {
            return Err(format!("{other} and {volume} would share one directory"));
        }
    }
    let written: Vec<Result<u32, String>> = volumes
        .par_iter()
        .map(|&volume| extract_one(library, &out.join(slug(volume)), volume, pages))
        .collect();
    let mut index = format!("{}\nslug\tpages\tpath\n", super::stamp("truth_sheet"));
    let mut failed = Vec::new();
    for (&volume, result) in volumes.iter().zip(&written) {
        let pages = match result {
            Ok(n) => n.to_string(),
            Err(e) => {
                failed.push(e.as_str());
                format!("error: {e}")
            }
        };
        let _ = writeln!(index, "{}\t{pages}\t{volume}", slug(volume));
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join("index.tsv");
    std::fs::write(&path, index).map_err(|e| format!("{}: {e}", path.display()))?;
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

fn extract_one(library: &Path, dir: &Path, volume: &str, pages: u32) -> Result<u32, String> {
    let member = library.join(volume);
    let count = member_pages(&member)?.min(pages);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    (0..count).into_par_iter().try_for_each(|i| {
        let file = dir.join(format!("p{i:02}.jpg"));
        let jpeg = page_jpeg(&member, i, EXTRACT_WIDTH)?;
        std::fs::write(&file, jpeg).map_err(|e| format!("{}: {e}", file.display()))
    })?;
    Ok(count)
}

/// `<out>/sheet.html`: each transcribed volume's named pages beside the
/// range read off them.
pub(super) fn truth_render(library: &Path, out: &Path, transcription: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(transcription)
        .map_err(|e| format!("{}: {e}", transcription.display()))?;
    let parsed: Transcription =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", transcription.display()))?;
    let sections: Vec<String> = parsed
        .volumes
        .par_iter()
        .enumerate()
        .map(|(n, volume)| truth_section(library, n + 1, volume))
        .collect::<Result<_, String>>()?;
    let html = document(
        "Contents pages and indicia",
        "Each volume's contents page or indicia, beside the range transcribed from it. \
         Confirm or correct each range against the page.",
        &super::stamp("truth_sheet"),
        &sections.concat(),
    );
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join("sheet.html");
    std::fs::write(&path, html).map_err(|e| format!("{}: {e}", path.display()))
}

fn truth_section(library: &Path, n: usize, volume: &Transcribed) -> Result<String, String> {
    let member = library.join(&volume.path);
    let mut figures = String::new();
    for &page in &volume.pages {
        let src = sheet_uri(&member, page)?;
        let _ = write!(
            figures,
            "<figure><img src=\"{src}\" alt=\"page {page:02}\"><figcaption>p{page:02}</figcaption></figure>"
        );
    }
    Ok(format!(
        "<section><h2>{n}. {}</h2><div class=\"truth\">{figures}<p class=\"range\">{}</p></div></section>\n",
        escape(&volume.path),
        escape(&transcribed_range(volume))
    ))
}

fn transcribed_range(volume: &Transcribed) -> String {
    let side = |n: Option<f64>| n.map_or_else(|| "?".to_string(), |n| n.to_string());
    let mut parts = Vec::new();
    if volume.first.is_some() || volume.last.is_some() {
        parts.push(format!(
            "Chapters {}–{}",
            side(volume.first),
            side(volume.last)
        ));
    }
    if let Some(issues) = &volume.issues {
        let listed: Vec<String> = issues.iter().map(|i| format!("#{i}")).collect();
        parts.push(format!("Issues {}", listed.join(", ")));
    }
    if let Some(note) = &volume.note {
        parts.push(note.clone());
    }
    if parts.is_empty() {
        "Nothing transcribed".to_string()
    } else {
        parts.join(". ")
    }
}

/// The sheet at `out` (a file: each round of judging writes its own): per
/// pair, every A page beside the B page the alignment paired it with, then
/// the B pages either side of the stretch those B pages span.
pub(super) fn pairing_sheet(library: &Path, out: &Path, pairs: &[SheetPair]) -> Result<(), String> {
    let sections: Vec<String> = pairs
        .par_iter()
        .enumerate()
        .map(|(n, pair)| pair_section(library, n + 1, pair))
        .collect::<Result<_, String>>()?;
    let html = document(
        "Pairs to judge",
        "Each A page beside the B page it was paired with, then the B pages either side \
         of that stretch. Answer each pair's question.",
        &super::stamp("pairing_sheet"),
        &sections.concat(),
    );
    std::fs::write(out, html).map_err(|e| format!("{}: {e}", out.display()))
}

fn pair_section(library: &Path, n: usize, pair: &SheetPair) -> Result<String, String> {
    if pair.a_pages.len() != pair.b_pages.len() || pair.a_pages.is_empty() {
        return Err(format!(
            "{} and {}: {} A pages against {} B pages",
            pair.a,
            pair.b,
            pair.a_pages.len(),
            pair.b_pages.len()
        ));
    }
    let (a, b) = (library.join(&pair.a), library.join(&pair.b));
    // Slots name pages as the alignment read them and the sheet fetches
    // them by its own listing; only equal counts make the two one sequence.
    for (name, member, read) in [(&pair.a, &a, pair.a_count), (&pair.b, &b, pair.b_count)] {
        let listed = page_count(member)?;
        if listed != read {
            return Err(format!(
                "{name}: the alignment read {read} pages and the sheet lists {listed}, so an \
                 index may name another page"
            ));
        }
    }
    let low = pair.b_pages.iter().copied().min().unwrap_or(0);
    let high = pair.b_pages.iter().copied().max().unwrap_or(0);
    let b_count = pair.b_count;
    let before: Vec<u32> = (low.saturating_sub(CONTEXT)..low).collect();
    let after: Vec<u32> = (high + 1..(high + 1 + CONTEXT).min(b_count)).collect();

    let a_uris = uris(&a, &pair.a_pages)?;
    let b_uris = uris(&b, &pair.b_pages)?;
    let mut rows = String::new();
    let pages = pair.a_pages.iter().zip(&pair.b_pages);
    for ((a_page, b_page), (a_uri, b_uri)) in pages.zip(a_uris.iter().zip(&b_uris)) {
        let _ = write!(
            rows,
            "<div class=\"facing\">{}{}</div>",
            figure(a_uri, &format!("A p{a_page}")),
            figure(b_uri, &format!("B p{b_page}"))
        );
    }
    let strip = |pages: &[u32], uris: &[String]| -> String {
        pages
            .iter()
            .zip(uris)
            .map(|(p, uri)| figure(uri, &format!("B p{p}")))
            .collect()
    };
    let around = format!(
        "<div class=\"around\"><span>B before</span>{}<span>B after</span>{}</div>",
        strip(&before, &uris(&b, &before)?),
        strip(&after, &uris(&b, &after)?)
    );
    let (legend, answers) = match pair.question {
        Question::Inside => (
            "Are all of A's pages inside B?",
            ["inside", "not inside", "can't tell"],
        ),
        Question::Same => (
            "Same book, the same drawings?",
            ["same", "different", "can't tell"],
        ),
    };
    let mut verdict = format!("<legend>{legend}</legend>");
    for v in answers {
        let _ = write!(
            verdict,
            "<label><input type=\"radio\" name=\"pair{n}\"> {v}</label>"
        );
    }
    Ok(format!(
        "<section><h2>{n}.</h2><p class=\"names\">A {}<br>B {}</p>{rows}{around}\
         <fieldset class=\"verdict\">{verdict}</fieldset></section>\n",
        escape(&pair.a),
        escape(&pair.b)
    ))
}

fn uris(member: &Path, pages: &[u32]) -> Result<Vec<String>, String> {
    pages
        .par_iter()
        .map(|&page| page_uri(member, page, PAIR_WIDTH))
        .collect()
}

fn figure(src: &str, caption: &str) -> String {
    format!(
        "<figure><img src=\"{src}\" alt=\"{caption}\"><figcaption>{caption}</figcaption></figure>"
    )
}

/// Page `index` of `member` as the sheets embed it.
pub(super) fn page_uri(member: &Path, index: u32, width: u32) -> Result<String, String> {
    Ok(data_uri(&page_jpeg(member, index, width)?))
}

/// A truth-sheet page: a spread twice as wide as a single page, so each of
/// its halves keeps a single page's width and its small print with it.
fn sheet_uri(member: &Path, index: u32) -> Result<String, String> {
    let (page, name) = decode(member, index)?;
    let width = if page.width() > page.height() {
        SPREAD_WIDTH
    } else {
        SHEET_WIDTH
    };
    let jpeg = encode(page, width).map_err(|e| format!("{}: {name}: {e}", member.display()))?;
    Ok(data_uri(&jpeg))
}

/// Page `index` of `member` at `width`, encoded as JPEG.
pub(super) fn page_jpeg(member: &Path, index: u32, width: u32) -> Result<Vec<u8>, String> {
    let (page, name) = decode(member, index)?;
    encode(page, width).map_err(|e| format!("{}: {name}: {e}", member.display()))
}

/// Where page `index` of a member is read from: the member itself, or the
/// archive of a directory of archives that holds it, counted in the order
/// `cached_edition` joins them, so a page its slots name is the page shown.
fn locate(member: &Path, index: u32) -> Result<(PathBuf, u32), String> {
    let parts = edition_paths(member)?;
    if let [only] = parts.as_slice() {
        return Ok((only.clone(), index));
    }
    let mut rest = index;
    for part in parts {
        let pages = member_pages(&part)?;
        if rest < pages {
            return Ok((part, rest));
        }
        rest -= pages;
    }
    Err(format!("{}: no page {index}", member.display()))
}

/// A member's pages, every archive's of a directory of archives.
fn page_count(member: &Path) -> Result<u32, String> {
    let parts = edition_paths(member)?;
    parts.iter().map(|part| member_pages(part)).sum()
}

/// Page `index` of `member` decoded in colour under the crate's decode
/// ceiling, with its entry name for errors.
fn decode(member: &Path, index: u32) -> Result<(DynamicImage, String), String> {
    let (part, local) = locate(member, index)?;
    let page = member_page(&part, local)?;
    let named = |e: &dyn std::fmt::Display| format!("{}: {}: {e}", member.display(), page.name);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::new(Cursor::new(&page.bytes))
        .with_guessed_format()
        .map_err(|e| named(&e))?;
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| named(&e))?;
    Ok((decoded, page.name))
}

/// `page` narrowed to `width` (never widened) and encoded as JPEG: the one
/// encoder every file and sheet here goes through.
fn encode(page: DynamicImage, width: u32) -> Result<Vec<u8>, String> {
    // A height of u32::MAX leaves the width to decide the scale.
    let scaled = if page.width() > width {
        page.resize(width, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        page
    };
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, QUALITY)
        .encode_image(&scaled.to_rgb8())
        .map_err(|e| e.to_string())?;
    Ok(jpeg)
}

fn data_uri(jpeg: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    )
}

/// A directory name for a volume: its library path, lower-cased, with each
/// run of anything but ASCII letters and digits as one `-`.
pub(super) fn slug(volume: &str) -> String {
    let mut out = String::new();
    for c in volume.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn document(title: &str, lead: &str, stamp: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title}</title><style>{STYLE}</style></head><body>\
         <h1>{title}</h1><p>{lead}</p><p class=\"stamp\">{}</p>\n{body}</body></html>\n",
        escape(stamp)
    )
}

const STYLE: &str = "\
body{font:15px/1.4 system-ui,sans-serif;margin:16px;background:#fff;color:#111}\
section{border-top:1px solid #999;padding:12px 0}\
h2{font-size:15px;margin:0 0 8px;word-break:break-all}\
figure{display:inline-block;margin:0 8px 8px 0;vertical-align:top}\
figure img{display:block;max-width:100%}\
figcaption{font-size:12px;color:#555}\
.truth{display:flex;flex-wrap:wrap;gap:12px;align-items:flex-start}\
.range{font-size:20px;font-weight:600}\
.facing{display:inline-block;margin:0 16px 8px 0;white-space:nowrap}\
.around span{display:inline-block;margin:0 8px;font-size:12px;color:#555}\
.names{font-size:13px;word-break:break-all}\
.verdict{border:1px solid #999;display:inline-block}\
.verdict label{margin-right:16px}\
.stamp{font:12px monospace;color:#555}";
