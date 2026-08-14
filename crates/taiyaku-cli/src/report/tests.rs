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
