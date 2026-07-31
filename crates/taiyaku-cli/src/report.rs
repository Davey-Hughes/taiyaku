//! The visual verification report.
//!
//! A contact sheet of paired thumbnails, written as one self-contained file.
//! Its whole job is to make a wrong pairing impossible to scroll past, so the
//! design borrows from print production: two editions that do not correspond
//! are two plates out of registration, and each row's gutter carries a gauge
//! that reads as aligned or visibly offset.

use image::DynamicImage;
use rayon::prelude::*;
use taiyaku_core::{PageConfidence, PageHash};

use crate::fingerprint::fingerprint;
use taiyaku_read::Page;

/// Width of a contact-sheet thumbnail, in pixels.
pub const THUMB_WIDTH: u32 = 130;

/// Margin at or above which a pairing needs no second look.
const MARGIN_FIRM: f32 = 0.15;
/// Margin below which the choice between candidates was close to arbitrary.
const MARGIN_WEAK: f32 = 0.05;

/// Decode each page once, then take both the hash and the thumbnail from it.
///
/// Decoding is the expensive step by three orders of magnitude, so the report
/// must not pay for it twice.
pub fn prepare(pages: &[Page], width: u32) -> Result<(Vec<PageHash>, Vec<String>), String> {
    let prepared: Result<Vec<(PageHash, String)>, String> = pages
        .par_iter()
        .map(|page| {
            let decoded =
                image::load_from_memory(&page.bytes).map_err(|e| format!("{}: {e}", page.name))?;
            let hash = fingerprint(&decoded.to_luma8());
            let thumb = thumbnail(&decoded, width)?;
            Ok((hash, thumb))
        })
        .collect();
    Ok(prepared?.into_iter().unzip())
}

/// How a row should read at a glance.
fn verdict(score: &PageConfidence) -> (&'static str, &'static str) {
    match score.counterpart {
        None => ("none", "no counterpart"),
        Some(_) if score.margin < MARGIN_WEAK => ("weak", "ambiguous"),
        Some(_) if score.margin < MARGIN_FIRM => ("check", "worth checking"),
        Some(_) => ("firm", "firm"),
    }
}

/// Encode a page as a thumbnail in a `data:` URI.
pub fn thumbnail(page: &DynamicImage, width: u32) -> Result<String, String> {
    use base64::Engine;

    let height = ((width as u64 * page.height() as u64) / page.width().max(1) as u64).max(1) as u32;
    let small = page.resize_exact(width, height, image::imageops::FilterType::Triangle);

    let mut jpeg = Vec::new();
    // Quality 70: these are read at 130px to judge whether two pages show the
    // same artwork, not to read the dialogue. A whole volume pair still lands
    // around 3.5 MB, which is small enough to open and to keep.
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 70)
        .encode_image(&small.to_rgb8())
        .map_err(|e| e.to_string())?;

    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&jpeg)
    ))
}

/// Render the whole report.
pub fn render(
    a_label: &str,
    b_label: &str,
    a_thumbs: &[String],
    b_thumbs: &[String],
    scored: &[PageConfidence],
    run_count: usize,
) -> String {
    let paired = scored.iter().filter(|s| s.counterpart.is_some()).count();
    let weak = scored.iter().filter(|s| verdict(s).0 == "weak").count();
    let per_hundred = 100.0 * run_count as f64 / scored.len().max(1) as f64;

    let mut strip = String::new();
    for score in scored {
        strip.push_str(&format!(
            "<a class=\"tick {}\" href=\"#p{}\" title=\"page {} — {}\"></a>",
            verdict(score).0,
            score.page,
            score.page,
            verdict(score).1
        ));
    }

    let mut rows = String::new();
    for score in scored {
        let (state, label) = verdict(score);
        let a_thumb = a_thumbs
            .get(score.page as usize)
            .map(String::as_str)
            .unwrap_or("");
        let facing = match score.counterpart {
            Some(j) => format!(
                "<div class=\"plate\"><img src=\"{}\" alt=\"edition B page {j}\" loading=\"lazy\"><span class=\"num b\">{j}</span></div>",
                b_thumbs.get(j as usize).map(String::as_str).unwrap_or("")
            ),
            None => "<div class=\"plate empty\"></div>".to_string(),
        };
        // Offset drives the registration gauge: a firm pairing closes to zero
        // and the two ticks read as one rule.
        // 16px of total travel. Smaller ranges were tried and read as a whisper:
        // the difference between a firm pairing and an ambiguous one has to be
        // obvious in peripheral vision while scrolling, or the gauge is
        // decoration.
        let offset = (((MARGIN_FIRM - score.margin.clamp(0.0, MARGIN_FIRM)) / MARGIN_FIRM) * 16.0)
            .round() as i32;
        // Only say something when there is something to say. A firm pairing
        // needs no word; its gauge is already closed.
        let note = match state {
            "firm" => String::new(),
            _ => format!("<span class=\"verdict\">{label}</span>"),
        };
        let readout = match score.counterpart {
            Some(_) => format!(
                "<span class=\"readout\">{sim:.2}<i>sim</i> {margin:+.2}<i>margin</i></span>",
                sim = score.similarity,
                margin = score.margin
            ),
            // Naming it "rejected" matters: the number is how good the best
            // candidate was, and showing a bare 1.00 beside "no counterpart"
            // reads as a contradiction rather than as the reason for the gap.
            None => format!(
                "<span class=\"readout\">{:.2}<i>best rejected</i></span>",
                score.similarity
            ),
        };
        rows.push_str(&format!(
            r#"<article class="pair {state}" id="p{page}">
<div class="plate"><img src="{a_thumb}" alt="edition A page {page}" loading="lazy"><span class="num a">{page}</span></div>
<div class="gutter" style="--offset:{offset}px"><span class="gauge"></span>{note}{readout}</div>
{facing}
</article>
"#,
            page = score.page,
        ));
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>taiyaku — {a_label} against {b_label}</title>
<style>{css}</style>
</head>
<body>
<header>
<h1>Registration check</h1>
<p class="editions"><span class="side a">A</span> {a_label} <span class="side b">B</span> {b_label}</p>
<dl class="stats">
<div><dt>pages</dt><dd>{total}</dd></div>
<div><dt>paired</dt><dd>{paired}</dd></div>
<div><dt>runs</dt><dd>{run_count}</dd></div>
<div><dt>runs / 100pp</dt><dd>{per_hundred:.1}</dd></div>
<div><dt>ambiguous</dt><dd>{weak}</dd></div>
</dl>
<nav class="strip" aria-label="Jump to a page by confidence">{strip}</nav>
<p class="legend">The gauge in each gutter closes when a pairing is firm and slips apart when the margin collapses. Click the strip to jump.</p>
</header>
<main>
{rows}</main>
</body>
</html>
"#,
        css = CSS,
        total = scored.len(),
    )
}

const CSS: &str = r#"
:root {
  --table: #23262b;
  --table-deep: #1a1d21;
  --stock: #e9e7e2;
  --muted: #8d9199;
  --rule: #444951;
  --plate-a: #3fa7b5;
  --plate-b: #c96a9a;
  --check: #d99a2b;
  --weak: #d1503c;
  --condensed: "Avenir Next Condensed", "Roboto Condensed", "HelveticaNeue-CondensedBold", system-ui, sans-serif;
  --mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--table);
  color: var(--stock);
  font-family: var(--mono);
  font-size: 13px;
}
header {
  position: sticky;
  top: 0;
  z-index: 2;
  padding: 18px 20px 12px;
  background: var(--table-deep);
  border-bottom: 1px solid var(--rule);
}
h1 {
  margin: 0;
  font-family: var(--condensed);
  font-size: 15px;
  font-weight: 600;
  letter-spacing: 0.22em;
  text-transform: uppercase;
}
.editions { margin: 6px 0 0; color: var(--muted); font-size: 12px; }
.side {
  display: inline-block;
  width: 15px; height: 15px; line-height: 15px;
  margin-right: 4px;
  text-align: center;
  font-family: var(--condensed);
  font-weight: 700;
  color: var(--table-deep);
  border-radius: 2px;
}
.side + .side { margin-left: 14px; }
.side.a { background: var(--plate-a); }
.side.b { background: var(--plate-b); }
.stats { display: flex; flex-wrap: wrap; gap: 22px; margin: 12px 0 0; }
.stats div { display: flex; flex-direction: column; gap: 2px; }
.stats dt {
  font-family: var(--condensed);
  font-size: 10px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--muted);
}
.stats dd { margin: 0; font-size: 17px; font-variant-numeric: tabular-nums; }
.strip { display: flex; gap: 1px; height: 24px; margin: 14px 0 0; }
.tick { flex: 1 1 0; min-width: 1px; background: var(--rule); border-radius: 1px; }
.tick:hover, .tick:focus-visible { outline: 1px solid var(--stock); outline-offset: 1px; }
.tick.check { background: var(--check); }
.tick.weak  { background: var(--weak); }
.tick.none  { background: #2f333a; }
.legend { margin: 10px 0 0; color: var(--muted); font-size: 11px; max-width: 62ch; }
/* A contact sheet, so pairs tile: the point is to take in many at once and
   have the eye snag on the ones that are wrong. */
main {
  padding: 14px 20px 64px;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(330px, 1fr));
  gap: 8px;
}
.pair {
  display: grid;
  grid-template-columns: 1fr 92px 1fr;
  align-items: center;
  gap: 6px;
  padding: 7px;
  border: 1px solid transparent;
  border-radius: 3px;
  scroll-margin-top: 210px;
}
.pair.check { border-color: color-mix(in srgb, var(--check) 42%, transparent); }
.pair.weak  { border-color: var(--weak); background: color-mix(in srgb, var(--weak) 12%, transparent); }
.pair.none  { border-color: var(--rule); border-style: dashed; }
.pair:target { outline: 2px solid var(--stock); outline-offset: 1px; }
.plate { position: relative; display: flex; justify-content: flex-end; }
.pair > .plate:last-child { justify-content: flex-start; }
.plate img {
  display: block;
  width: 100%;
  max-width: 110px;
  height: auto;
  background: var(--stock);
  border-radius: 1px;
}
.num {
  position: absolute;
  bottom: 3px;
  padding: 0 4px;
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  color: var(--table-deep);
  border-radius: 2px;
}
.num.a { right: 3px; background: color-mix(in srgb, var(--plate-a) 88%, white); }
.num.b { left: 3px; background: color-mix(in srgb, var(--plate-b) 88%, white); }
/* An absent counterpart takes the same footprint as a page, so a gap reads as
   a hole in the sequence rather than as a missing element of the layout. */
.plate.empty {
  justify-content: flex-start;
  width: 110px;
  aspect-ratio: 5 / 7;
  border: 1px dashed var(--rule);
  border-radius: 2px;
  background: repeating-linear-gradient(
    -45deg, transparent 0 6px, color-mix(in srgb, var(--rule) 28%, transparent) 6px 7px);
}
.gutter { display: flex; flex-direction: column; align-items: center; gap: 4px; text-align: center; }
/* The signature: two ticks that read as one rule when the two editions are in
   register, and slip apart as the margin collapses. */
.gauge { display: block; width: 100%; height: 20px; position: relative; }
.gauge::before, .gauge::after {
  content: "";
  position: absolute;
  height: 2px;
  width: 46%;
  top: 9px;
  background: var(--muted);
}
.gauge::before { left: 0; transform: translateY(calc(var(--offset) * -0.5)); }
.gauge::after  { right: 0; transform: translateY(calc(var(--offset) * 0.5)); }
.pair.check .gauge::before, .pair.check .gauge::after { background: var(--check); }
.pair.weak .gauge::before, .pair.weak .gauge::after { background: var(--weak); }
.pair.none .gauge::before, .pair.none .gauge::after { background: var(--rule); width: 22%; }
.verdict {
  font-family: var(--condensed);
  font-size: 10px;
  letter-spacing: 0.13em;
  text-transform: uppercase;
  color: var(--muted);
}
.pair.weak .verdict { color: var(--weak); }
.pair.check .verdict { color: var(--check); }
.readout {
  font-size: 10px;
  line-height: 1.45;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}
.readout i { font-style: normal; opacity: 0.62; margin-left: 3px; }
.readout i::after { content: "\A"; white-space: pre; }
@media (max-width: 720px) {
  main { grid-template-columns: 1fr; }
}
@media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// The report has to be one file. An alignment you can only inspect with a
    /// directory of loose images beside it is one nobody will inspect, and the
    /// whole point is that wrong alignments get seen.
    #[test]
    fn a_thumbnail_is_a_self_contained_data_uri_at_the_requested_width() {
        let page = DynamicImage::new_rgb8(800, 1200);

        let uri = thumbnail(&page, 100).expect("thumbnail");

        let payload = uri
            .strip_prefix("data:image/jpeg;base64,")
            .expect("data uri with a jpeg payload");
        let bytes = base64_decode(payload).expect("valid base64");
        let decoded = image::load_from_memory(&bytes).expect("decodable jpeg");
        assert_eq!(decoded.width(), 100);
        // Aspect preserved: 800x1200 scaled to width 100 is 150 tall.
        assert_eq!(decoded.height(), 150);
    }

    fn base64_decode(text: &str) -> Result<Vec<u8>, base64::DecodeError> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(text)
    }
}
