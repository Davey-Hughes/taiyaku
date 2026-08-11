//! The visual verification report.
//!
//! A contact sheet of paired thumbnails, written as one self-contained file.
//! Its whole job is to make a wrong pairing impossible to scroll past, so the
//! design borrows from print production: two editions that do not correspond
//! are two plates out of registration, and each row's gutter carries a gauge
//! that reads as aligned or visibly offset.

use std::fmt::Write as _;

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

/// A thumbnail dimension in pixels, as `thumbnail`'s resize takes it.
///
/// # Panics
///
/// Panics if `v` exceeds `u32::MAX`. It cannot at `thumbnail`'s one call
/// site: `width` is always a small fixed value (`THUMB_WIDTH` (130) is the
/// only width production code passes — main.rs's two `report::prepare`
/// calls — and the two test calls pass 100 and 32, confirmed by `grep -rn
/// 'thumbnail(' crates/`, no caller passes anything else), and
/// `page.height()`/`page.width()` come from real decoded page images, whose
/// dimensions in this tool's corpus run to at most a few thousand pixels per
/// side. `width * page.height() / page.width().max(1)` stays far inside
/// u32's range — a property of the material this tool decodes, not enforced
/// by the code. Check both claims if a caller passing a different width is
/// ever added.
fn thumb_dimension(v: u64) -> u32 {
    u32::try_from(v).expect("thumbnail dimension exceeds u32::MAX")
}

/// Encode a page as a thumbnail in a `data:` URI.
pub fn thumbnail(page: &DynamicImage, width: u32) -> Result<String, String> {
    use base64::Engine;

    let height = thumb_dimension(
        ((u64::from(width) * u64::from(page.height())) / u64::from(page.width().max(1))).max(1),
    );
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
    #[expect(
        clippy::cast_precision_loss,
        reason = "run_count and scored.len() count runs and pages for this report; real \
                  editions run to at most a few hundred pages and runs, far below f64's 2^53 \
                  exact-integer bound, so the conversion loses no precision — a property of \
                  the material (both are usize with no cap; no From<usize> for f64 exists, \
                  since usize's width is platform-dependent)."
    )]
    let per_hundred = 100.0 * run_count as f64 / scored.len().max(1) as f64;

    let mut strip = String::new();
    for score in scored {
        let _ = write!(
            strip,
            "<a class=\"tick {}\" href=\"#p{}\" title=\"page {} — {}\"></a>",
            verdict(score).0,
            score.page,
            score.page,
            verdict(score).1
        );
    }

    let mut rows = String::new();
    for score in scored {
        let (state, label) = verdict(score);
        let a_thumb = a_thumbs.get(score.page as usize).map_or("", String::as_str);
        let facing = match score.counterpart {
            Some(j) => format!(
                "<div class=\"plate\"><img src=\"{}\" alt=\"edition B page {j}\" loading=\"lazy\"><span class=\"num b\">{j}</span></div>",
                b_thumbs.get(j as usize).map_or("", String::as_str)
            ),
            None => "<div class=\"plate empty\"></div>".to_string(),
        };
        // Offset drives the registration gauge: a firm pairing closes to zero
        // and the two ticks read as one rule.
        // 16px of total travel. Smaller ranges were tried and read as a whisper:
        // the difference between a firm pairing and an ambiguous one has to be
        // obvious in peripheral vision while scrolling, or the gauge is
        // decoration.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "score.margin.clamp(0.0, MARGIN_FIRM) bounds the clamped value to \
                      [0.0, MARGIN_FIRM] right here, so (MARGIN_FIRM - clamped) / MARGIN_FIRM \
                      is in [0.0, 1.0] and the rounded result of that times 16.0 is an integer \
                      in 0..=16, well inside i32's range — enforced by the .clamp call \
                      on this same line. A try_from helper does not fit: the source is f32, \
                      which TryFrom does not convert from."
        )]
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
        let _ = writeln!(
            rows,
            r#"<article class="pair {state}" id="p{page}">
<div class="plate"><img src="{a_thumb}" alt="edition A page {page}" loading="lazy"><span class="num a">{page}</span></div>
<div class="gutter" style="--offset:{offset}px"><span class="gauge"></span>{note}{readout}</div>
{facing}
</article>"#,
            page = score.page,
        );
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

    fn scored(page: u32, counterpart: Option<u32>, similarity: f32, margin: f32) -> PageConfidence {
        PageConfidence {
            page,
            counterpart,
            similarity,
            margin,
        }
    }

    /// Four bands, and the two thresholds between them. Nothing asserted any of
    /// it: `verdict` could return `("xyzzy", "xyzzy")` for every page, and both
    /// margin comparisons could be inverted, with the suite green.
    ///
    /// The boundaries are tested *exactly at* the constants, because both are
    /// strict `<` — a margin of exactly `MARGIN_WEAK` is not weak — and that is
    /// the distinction every `<` → `<=` mutant turns on.
    #[test]
    fn a_verdict_bands_on_the_margin_and_the_boundaries_are_exclusive() {
        assert_eq!(
            verdict(&scored(0, None, 0.9, 0.9)).0,
            "none",
            "no counterpart wins over any margin"
        );

        assert_eq!(verdict(&scored(0, Some(1), 0.9, 0.0)).0, "weak");
        assert_eq!(
            verdict(&scored(0, Some(1), 0.9, MARGIN_WEAK - 0.001)).0,
            "weak"
        );
        assert_eq!(
            verdict(&scored(0, Some(1), 0.9, MARGIN_WEAK)).0,
            "check",
            "exactly at the weak threshold is no longer weak: the test is `<`"
        );
        assert_eq!(
            verdict(&scored(0, Some(1), 0.9, MARGIN_FIRM - 0.001)).0,
            "check"
        );
        assert_eq!(
            verdict(&scored(0, Some(1), 0.9, MARGIN_FIRM)).0,
            "firm",
            "and exactly at the firm threshold is firm"
        );
        assert_eq!(verdict(&scored(0, Some(1), 0.9, 1.0)).0, "firm");

        // The second element is the wording a reader sees, and is not the
        // first: a stub returning the same string twice must fail.
        assert_eq!(verdict(&scored(0, None, 0.9, 0.0)).1, "no counterpart");
        assert_eq!(verdict(&scored(0, Some(1), 0.9, 0.0)).1, "ambiguous");
        assert_eq!(verdict(&scored(0, Some(1), 0.9, 0.10)).1, "worth checking");
        assert_ne!(
            verdict(&scored(0, Some(1), 0.9, 0.0)).0,
            verdict(&scored(0, Some(1), 0.9, 0.0)).1,
            "the class and the wording are different strings"
        );
    }

    /// The gauge is the one thing in the report a reader judges at a glance
    /// while scrolling, and it is pure arithmetic on the margin: it must close
    /// to zero when the pairing is firm and open to its full travel when the
    /// margin collapses. Every operator in it survived.
    ///
    /// Asserted through `render` rather than on an extracted helper because the
    /// clamp, the divide and the scale are written inline there, and it is the
    /// emitted `--offset` a stylesheet actually reads.
    #[test]
    fn the_gauge_closes_on_a_firm_pairing_and_opens_as_the_margin_collapses() {
        let offset_for = |margin: f32| {
            let html = render(
                "A",
                "B",
                &["a".to_string()],
                &["b".to_string()],
                &[scored(0, Some(0), 0.9, margin)],
                1,
            );
            let at = html.find("--offset:").expect("an offset is emitted");
            let rest = &html[at + "--offset:".len()..];
            rest[..rest.find("px").expect("in px")]
                .parse::<i32>()
                .expect("an integer")
        };

        assert_eq!(
            offset_for(MARGIN_FIRM),
            0,
            "a firm pairing closes the gauge"
        );
        assert_eq!(offset_for(1.0), 0, "and anything beyond firm stays closed");
        assert_eq!(
            offset_for(0.0),
            16,
            "a collapsed margin opens the full travel"
        );
        assert_eq!(
            offset_for(MARGIN_FIRM / 2.0),
            8,
            "and it is linear between the two"
        );
    }

    /// The header counts, and the one that is a rate rather than a tally.
    /// `render` could return `"xyzzy"` outright; `paired`'s `is_some` could be
    /// inverted; and every operator in `100 * runs / pages` survived, including
    /// `*` → `+`, which reads plausibly and is wrong at every input but one.
    #[test]
    fn the_header_counts_the_pages_the_pairings_and_the_run_rate() {
        let html = render(
            "raw",
            "en",
            std::array::from_fn::<String, 4, _>(|_| "a".into()).as_slice(),
            std::array::from_fn::<String, 4, _>(|_| "b".into()).as_slice(),
            &[
                scored(0, Some(0), 0.9, 0.9),
                scored(1, Some(1), 0.9, 0.9),
                scored(2, None, 0.4, 0.0),
                scored(3, Some(3), 0.9, 0.01),
            ],
            2,
        );

        assert!(html.contains("<dt>pages</dt><dd>4</dd>"), "{html}");
        assert!(
            html.contains("<dt>paired</dt><dd>3</dd>"),
            "three of the four have a counterpart"
        );
        assert!(html.contains("<dt>runs</dt><dd>2</dd>"));
        // 100 * 2 / 4. `+` gives 102, `%` gives 0, `/` gives 12.5 — all visible.
        assert!(
            html.contains("<dt>runs / 100pp</dt><dd>50.0</dd>"),
            "two runs over four pages is 50 per hundred: {html}"
        );
        assert!(
            html.contains("<dt>ambiguous</dt><dd>1</dd>"),
            "only the margin-0.01 page is weak"
        );
        assert!(
            html.contains("raw") && html.contains("en"),
            "both labels appear"
        );
    }

    /// A firm pairing gets no word beside its gauge — the gauge already says
    /// it — and every other verdict does. Deleting the `"firm"` arm makes the
    /// report shout "firm" beside every confident row, which is exactly the
    /// noise the arm exists to remove.
    #[test]
    fn only_an_unfirm_pairing_carries_a_word_beside_its_gauge() {
        let one = |margin: f32| {
            render(
                "A",
                "B",
                &["a".to_string()],
                &["b".to_string()],
                &[scored(0, Some(0), 0.9, margin)],
                1,
            )
        };

        assert!(
            !one(0.9).contains("class=\"verdict\""),
            "a firm pairing says nothing"
        );
        assert!(one(0.0).contains(">ambiguous<"), "a weak one names itself");
        assert!(one(0.10).contains(">worth checking<"));
    }

    /// A gap shows the score of the candidate it *rejected*, labelled as such.
    /// A bare similarity beside "no counterpart" reads as a contradiction
    /// rather than as the reason for the gap, which is why the wording is
    /// pinned and not just the number.
    #[test]
    fn a_gap_reports_the_best_rejected_candidate_rather_than_a_bare_score() {
        let html = render(
            "A",
            "B",
            &["a".to_string()],
            &["b".to_string()],
            &[scored(0, None, 0.42, 0.0)],
            0,
        );

        assert!(html.contains("best rejected"), "{html}");
        assert!(html.contains("0.42"));
        assert!(html.contains("plate empty"), "and no facing page is drawn");
    }

    /// `prepare` decodes each page once and takes both the hash and the
    /// thumbnail from that one decode — decoding being expensive by three
    /// orders of magnitude. It could return two empty vectors instead, and the
    /// report would render with no thumbnails and every page unhashed.
    ///
    /// Two pages that differ, so the hashes must differ too: a `prepare` that
    /// decoded once and reused the result for every page would pass a
    /// length-only check.
    #[test]
    fn prepare_returns_one_hash_and_one_thumbnail_per_page_in_order() {
        // Split on different axes, not merely shaded differently: the
        // fingerprint is perceptual and runs on `to_luma8`, so two smooth
        // gradients that differ only in one channel hash identically —
        // measured, both `PageHash([0, 0, 0, 0])`. Structure is what it sees.
        let encode = |vertical: bool| {
            let img = image::RgbImage::from_fn(64, 96, |x, y| {
                let dark = if vertical { x < 32 } else { y < 48 };
                let v = if dark { 0u8 } else { 255u8 };
                image::Rgb([v, v, v])
            });
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut out, image::ImageFormat::Png)
                .unwrap();
            out.into_inner()
        };
        let pages = [
            Page {
                name: "p1.png".into(),
                bytes: encode(true),
            },
            Page {
                name: "p2.png".into(),
                bytes: encode(false),
            },
        ];

        let (hashes, thumbs) = prepare(&pages, 32).expect("both pages decode");

        assert_eq!(hashes.len(), 2, "one hash per page");
        assert_eq!(thumbs.len(), 2, "and one thumbnail per page");
        assert_ne!(
            hashes[0], hashes[1],
            "two different pages must not hash alike"
        );
        for thumb in &thumbs {
            assert!(
                thumb.starts_with("data:image/jpeg;base64,"),
                "a self-contained thumbnail: {}",
                &thumb[..thumb.len().min(40)]
            );
        }
        assert_ne!(thumbs[0], thumbs[1], "and they are of different pages");
    }

    /// An undecodable page names itself rather than failing anonymously — the
    /// whole report is one file built from every page, so "which one" is the
    /// only actionable part of the message.
    #[test]
    fn a_page_that_does_not_decode_names_itself() {
        let pages = [Page {
            name: "broken.jpg".into(),
            bytes: b"not an image".to_vec(),
        }];

        let error = prepare(&pages, 32).expect_err("undecodable");

        assert!(error.contains("broken.jpg"), "must name the page: {error}");
    }
}
