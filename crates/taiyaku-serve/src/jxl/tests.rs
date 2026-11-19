use std::io::Cursor;

use image::error::LimitErrorKind;
use image::{ColorType, DynamicImage, GenericImageView as _, ImageError, ImageReader, Limits};

use jxl_oxide::jpeg_bitstream::JpegBitstreamData;

use super::*;
use crate::source::{JXL_CODESTREAM, JXL_CONTAINER};
use crate::test_support::jxl_fixture;

/// The most a sample of a lossy page may differ from libjxl's decode of it.
/// The decoders round the same transform differently; a level is the whole of
/// that difference on these pages.
const T: u8 = 1;

/// An allocation limit above `large.jxl`'s output buffer and below the
/// decoder's working memory for it, so only the decoder's own allocations can
/// cross it.
const UNDER_WORKING_MEMORY: u64 = 33_554_432;

fn decode(bytes: &[u8]) -> Result<DynamicImage, ImageError> {
    decode_within(bytes, Limits::default())
}

/// A decode as every caller in the crate makes one: the format guessed from
/// the bytes, the caller's limits handed to the reader.
fn decode_within(bytes: &[u8], limits: Limits) -> Result<DynamicImage, ImageError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .expect("guessing reads from memory");
    reader.limits(limits);
    reader.decode()
}

fn max_alloc(bytes: u64) -> Limits {
    let mut limits = Limits::default();
    limits.max_alloc = Some(bytes);
    limits
}

fn png(name: &str) -> DynamicImage {
    image::load_from_memory_with_format(jxl_fixture(name), image::ImageFormat::Png)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The largest difference between two images' samples, or `None` where their
/// sizes or colour types differ and no sample can be set against another.
fn max_difference(decoded: &DynamicImage, reference: &DynamicImage) -> Option<u8> {
    if decoded.dimensions() != reference.dimensions() || decoded.color() != reference.color() {
        return None;
    }
    decoded
        .as_bytes()
        .iter()
        .zip(reference.as_bytes())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
}

/// A decode's outcome in a line, for an assertion's message: the pixels
/// themselves would bury it.
fn outcome(result: &Result<DynamicImage, ImageError>) -> String {
    match result {
        Ok(image) => format!(
            "decoded {}x{} {:?}",
            image.width(),
            image.height(),
            image.color()
        ),
        Err(e) => format!("{e:?}"),
    }
}

#[test]
fn every_fixture_decodes_through_image_reader_as_djxl_does() {
    register();
    assert_eq!(
        png("grey.djxl.png").color(),
        ColorType::L8,
        "the fixture: grey.jxl is one channel"
    );
    assert_eq!(
        png("alpha.djxl.png").color(),
        ColorType::Rgba8,
        "the fixture: alpha.jxl carries alpha"
    );

    for (name, tolerance) in [
        ("lossless", 0),
        ("grey", 0),
        ("large", 0),
        ("alpha", 0),
        ("animated", 0),
        ("lossy", T),
        ("recompressed", T),
    ] {
        let decoded = decode(jxl_fixture(&format!("{name}.jxl")));
        let reference = png(&format!("{name}.djxl.png"));
        let difference = decoded
            .as_ref()
            .ok()
            .and_then(|image| max_difference(image, &reference));
        assert!(
            difference.is_some_and(|d| d <= tolerance),
            "{name}.jxl: {}, djxl {}x{} {:?}; largest difference {difference:?}, allowed {tolerance}",
            outcome(&decoded),
            reference.width(),
            reference.height(),
            reference.color()
        );
    }
}

#[test]
fn a_jpeg_xl_page_is_its_first_frame() {
    register();
    let first = png("animated.frame1.png");
    assert_ne!(
        max_difference(&first, &png("animated.frame2.png")),
        Some(0),
        "the fixture: animated.jxl's two frames differ"
    );

    let decoded = decode(jxl_fixture("animated.jxl"));
    let difference = decoded
        .as_ref()
        .ok()
        .and_then(|image| max_difference(image, &first));
    assert_eq!(
        difference,
        Some(0),
        "animated.jxl reads as its first frame: {}",
        outcome(&decoded)
    );
}

#[test]
fn the_decoder_refuses_a_page_over_the_callers_size_limits() {
    register();
    assert!(
        png("large.djxl.png").width() > 1024,
        "the fixture: large.jxl is wider than the limit"
    );

    let mut limits = Limits::default();
    limits.max_image_width = Some(1024);
    let decoded = decode_within(jxl_fixture("large.jxl"), limits);
    assert!(
        matches!(decoded, Err(ImageError::Limits(_))),
        "large.jxl under a 1024-wide limit: {}",
        outcome(&decoded)
    );
}

#[test]
fn the_decoder_refuses_a_decode_over_the_callers_allocation_limit() {
    register();
    let output = png("large.djxl.png").as_bytes().len();
    assert!(
        u64::try_from(output).unwrap() < UNDER_WORKING_MEMORY,
        "the fixture: large.jxl's {output}-byte output fits the limit, so only the decoder's own \
         memory can cross it"
    );

    let refused = decode_within(jxl_fixture("large.jxl"), max_alloc(UNDER_WORKING_MEMORY));
    assert!(
        matches!(
            &refused,
            Err(ImageError::Limits(e)) if e.kind() == LimitErrorKind::InsufficientMemory
        ),
        "large.jxl under {UNDER_WORKING_MEMORY} bytes: {}",
        outcome(&refused)
    );

    let control = decode_within(jxl_fixture("large.jxl"), max_alloc(MAX_DECODE_ALLOC));
    assert!(
        control.is_ok(),
        "large.jxl under MAX_DECODE_ALLOC: {}",
        outcome(&control)
    );
}

#[test]
fn both_signatures_are_detected() {
    register();
    assert!(
        jxl_fixture("lossy.jxl").starts_with(JXL_CODESTREAM),
        "the fixture: lossy.jxl is a bare codestream"
    );
    assert!(
        jxl_fixture("recompressed.jxl").starts_with(JXL_CONTAINER),
        "the fixture: recompressed.jxl is a container"
    );

    for name in ["lossy.jxl", "recompressed.jxl"] {
        let decoded = decode(jxl_fixture(name));
        assert!(decoded.is_ok(), "{name}: {}", outcome(&decoded));
    }
}

#[test]
fn registering_twice_is_harmless() {
    register();
    register();

    let decoded = decode(jxl_fixture("lossy.jxl"));
    let difference = decoded
        .as_ref()
        .ok()
        .and_then(|image| max_difference(image, &png("lossy.djxl.png")));
    assert!(
        difference.is_some_and(|d| d <= T),
        "lossy.jxl after a second registration: {}; largest difference {difference:?}",
        outcome(&decoded)
    );
}

#[test]
fn a_recompressed_jpeg_is_rebuilt_byte_for_byte() {
    register();
    let page = jxl_fixture("recompressed.jxl");
    assert!(
        page.starts_with(JXL_CONTAINER),
        "the fixture: recompressed.jxl is a container, the one shape that carries \
         reconstruction data"
    );

    let rebuilt = rebuilt_jpeg(page);
    assert!(
        rebuilt.as_deref() == Some(jxl_fixture("recompressed.jpg")),
        "recompressed.jxl rebuilds recompressed.jpg: got {:?} bytes",
        rebuilt.as_ref().map(Vec::len)
    );
}

#[test]
fn a_page_with_no_reconstruction_data_is_not_rebuilt() {
    register();
    let page = jxl_fixture("lossy.jxl");
    assert!(
        page.starts_with(JXL_CODESTREAM),
        "the fixture: lossy.jxl is a bare codestream, with no box to carry reconstruction data"
    );

    assert_eq!(
        rebuilt_jpeg(page).map(|jpeg| jpeg.len()),
        None,
        "lossy.jxl has no JPEG to rebuild"
    );
}

/// The lengths a cut-short copy of `page` is tried at: the header and one
/// byte past it, where a decoder first has something to draw, each sixteenth
/// of the file, and one byte short of the whole.
fn cut_lengths(page: &[u8]) -> Vec<usize> {
    let header = (1..page.len())
        .find(|&length| {
            ImageReader::new(Cursor::new(&page[..length]))
                .with_guessed_format()
                .expect("guessing reads from memory")
                .into_dimensions()
                .is_ok()
        })
        .expect("a header that ends before the file does");
    let mut lengths: Vec<usize> = [header, header + 1, page.len() - 1]
        .into_iter()
        .chain((1..16).map(|sixteenths| page.len() * sixteenths / 16))
        .filter(|&length| length < page.len())
        .collect();
    lengths.sort_unstable();
    lengths.dedup();
    lengths
}

#[test]
fn a_cut_short_page_is_refused_not_drawn_in_part() {
    register();
    for name in ["lossy", "lossless", "grey", "alpha", "recompressed"] {
        let page = jxl_fixture(&format!("{name}.jxl"));
        let whole = decode(page);
        assert!(
            whole.is_ok(),
            "the fixture: the whole of {name}.jxl decodes: {}",
            outcome(&whole)
        );

        for length in cut_lengths(page) {
            let cut = decode(&page[..length]);
            assert!(
                matches!(cut, Err(ImageError::Decoding(_))),
                "{name}.jxl cut to {length} of {} bytes: {}",
                page.len(),
                outcome(&cut)
            );
        }
    }
}

/// An animation's page is its first frame, so a copy cut inside a later frame
/// still holds the whole page; cut anywhere, it is refused or drawn whole.
#[test]
fn a_cut_short_animation_is_refused_or_its_whole_first_frame() {
    register();
    let page = jxl_fixture("animated.jxl");
    let first = png("animated.frame1.png");
    let whole = decode(page);
    assert_eq!(
        whole
            .as_ref()
            .ok()
            .and_then(|image| max_difference(image, &first)),
        Some(0),
        "the fixture: the whole of animated.jxl reads as its first frame: {}",
        outcome(&whole)
    );

    for length in cut_lengths(page) {
        let cut = decode(&page[..length]);
        let refused = matches!(cut, Err(ImageError::Decoding(_)));
        let whole_first_frame = cut
            .as_ref()
            .ok()
            .and_then(|image| max_difference(image, &first))
            == Some(0);
        assert!(
            refused || whole_first_frame,
            "animated.jxl cut to {length} of {} bytes: {}",
            page.len(),
            outcome(&cut)
        );
    }
}

/// `bomb.jxl`, once its test has asserted what it is: recompressed.jxl with
/// one `brob` box of Exif added, a few KB on disk, whose stream inflates past
/// the cap.
fn bomb() -> &'static [u8] {
    let page = jxl_fixture("bomb.jxl");
    assert!(
        page.len() < 8 * 1024,
        "the fixture: bomb.jxl is a few KB, not {} bytes",
        page.len()
    );
    let found = page.windows(8).position(|w| w == b"brobExif");
    assert!(
        found.is_some(),
        "the fixture: bomb.jxl holds a brob box of Exif"
    );
    let start = found.unwrap() - 4;
    let end = start
        + usize::try_from(u32::from_be_bytes(
            page[start..start + 4].try_into().unwrap(),
        ))
        .unwrap();
    assert!(
        [&page[..start], &page[end..]].concat() == jxl_fixture("recompressed.jxl"),
        "the fixture: bomb.jxl is recompressed.jxl with one box added"
    );
    assert!(
        inflates_past_the_cap(&page[start + 12..end]),
        "the fixture: bomb.jxl's box inflates past METADATA_CAP"
    );
    page
}

/// Counted by streaming, apart from the code under test, and no further than
/// one byte past the cap.
fn inflates_past_the_cap(compressed: &[u8]) -> bool {
    let mut stream = brotli_decompressor::Decompressor::new(compressed, 4096)
        .take(u64::try_from(METADATA_CAP).unwrap() + 1);
    let inflated = std::io::copy(&mut stream, &mut std::io::sink()).expect("a whole brotli stream");
    inflated > u64::try_from(METADATA_CAP).unwrap()
}

fn refused_over_the_cap<T>(result: &Result<T, ImageError>) -> bool {
    matches!(
        result,
        Err(ImageError::Limits(e)) if e.kind() == LimitErrorKind::InsufficientMemory
    )
}

#[test]
fn a_page_whose_metadata_inflates_past_the_cap_is_refused() {
    register();
    let decoded = decode(bomb());
    assert!(
        refused_over_the_cap(&decoded),
        "bomb.jxl: {}",
        outcome(&decoded)
    );
}

/// Refused, not read: the hook checks the boxes before it builds the decoder
/// that `into_dimensions` asks, because jxl-oxide inflates the boxes ahead of
/// the codestream while it reads the header.
#[test]
fn the_size_of_a_page_whose_metadata_inflates_past_the_cap_is_refused() {
    register();
    let dimensions = ImageReader::new(Cursor::new(bomb()))
        .with_guessed_format()
        .expect("guessing reads from memory")
        .into_dimensions();
    assert!(
        refused_over_the_cap(&dimensions),
        "bomb.jxl's dimensions: {dimensions:?}"
    );
}

#[test]
fn a_page_whose_metadata_inflates_past_the_cap_is_not_rebuilt() {
    register();
    let rebuilt = rebuilt_jpeg(bomb());
    assert!(
        rebuilt.is_none(),
        "bomb.jxl rebuilt to {:?} bytes",
        rebuilt.as_ref().map(Vec::len)
    );
}

/// recompressed.jxl with the compressed stream in its `jbrd` box swapped for
/// bomb.jxl's, behind the same header, so jxl-oxide inflates it as the data a
/// rebuild needs.
fn with_a_bomb_in_its_reconstruction_data() -> Vec<u8> {
    let bomb = bomb();
    let stream_at = bomb.windows(8).position(|w| w == b"brobExif").unwrap() + 8;
    let stream_end = stream_at - 12
        + usize::try_from(u32::from_be_bytes(
            bomb[stream_at - 12..stream_at - 8].try_into().unwrap(),
        ))
        .unwrap();
    let page = jxl_fixture("recompressed.jxl");
    let start = page.windows(4).position(|w| w == b"jbrd").unwrap() - 4;
    let end = start
        + usize::try_from(u32::from_be_bytes(
            page[start..start + 4].try_into().unwrap(),
        ))
        .unwrap();
    let payload = &page[start + 8..end];
    // jxl-jbr's own reading: the shortest prefix it takes a header from.
    let header = (1..=payload.len())
        .find(|&n| matches!(JpegBitstreamData::try_parse(&payload[..n]), Ok(Some(_))))
        .expect("recompressed.jxl's jbrd header parses");
    let jbrd = [&payload[..header], &bomb[stream_at..stream_end]].concat();
    let size = u32::try_from(jbrd.len() + 8).unwrap().to_be_bytes();
    [&page[..start], &size, b"jbrd", &jbrd, &page[end..]].concat()
}

#[test]
fn a_page_whose_reconstruction_data_inflates_past_the_cap_is_refused() {
    register();
    let page = with_a_bomb_in_its_reconstruction_data();
    assert!(
        !page.windows(4).any(|w| w == b"brob"),
        "the fixture: the stream is in the jbrd box alone"
    );

    let decoded = decode(&page);
    assert!(
        refused_over_the_cap(&decoded),
        "a jbrd box over the cap: {}",
        outcome(&decoded)
    );
}

/// A page of `brob` boxes of zeros from make.sh, once its test has asserted
/// what each box inflates to. The boxes are found and counted apart from the
/// code under test.
fn metadata_page(name: &str, inflated: &[usize]) -> &'static [u8] {
    let page = jxl_fixture(name);
    assert!(
        page.len() < 8 * 1024,
        "the fixture: {name} is a few KB, not {} bytes",
        page.len()
    );
    let boxes: Vec<usize> = page
        .windows(4)
        .enumerate()
        .filter(|(_, kind)| *kind == b"brob")
        .map(|(at, _)| {
            let start = at - 4;
            let size = u32::from_be_bytes(page[start..at].try_into().unwrap());
            let stream = &page[at + 8..start + usize::try_from(size).unwrap()];
            let mut inflate = brotli_decompressor::Decompressor::new(stream, 4096);
            usize::try_from(
                std::io::copy(&mut inflate, &mut std::io::sink()).expect("a whole brotli stream"),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        boxes, inflated,
        "the fixture: what {name}'s brob boxes inflate to"
    );
    page
}

const MIB: usize = 1024 * 1024;

#[test]
fn a_page_whose_metadata_is_well_under_the_cap_decodes() {
    register();
    let page = metadata_page("metadata-two-4mib.jxl", &[4 * MIB, 4 * MIB]);
    let decoded = decode(page);
    let difference = decoded
        .as_ref()
        .ok()
        .and_then(|image| max_difference(image, &png("recompressed.djxl.png")));
    assert!(
        difference.is_some_and(|d| d <= T),
        "metadata-two-4mib.jxl: {}; largest difference {difference:?}",
        outcome(&decoded)
    );
}

#[test]
fn a_page_whose_metadata_is_well_under_the_cap_is_rebuilt() {
    register();
    let page = metadata_page("metadata-two-4mib.jxl", &[4 * MIB, 4 * MIB]);
    let rebuilt = rebuilt_jpeg(page);
    assert!(
        rebuilt.as_deref() == Some(jxl_fixture("recompressed.jpg")),
        "metadata-two-4mib.jxl rebuilds recompressed.jpg: got {:?} bytes",
        rebuilt.as_ref().map(Vec::len)
    );
}

#[test]
fn metadata_that_inflates_to_exactly_the_cap_is_accepted() {
    register();
    let page = metadata_page("metadata-at-cap.jxl", &[METADATA_CAP]);
    let decoded = decode(page);
    assert!(
        decoded.is_ok(),
        "metadata-at-cap.jxl: {}",
        outcome(&decoded)
    );
}

#[test]
fn metadata_that_inflates_to_one_byte_past_the_cap_is_refused() {
    register();
    let page = metadata_page("metadata-past-cap.jxl", &[METADATA_CAP + 1]);
    let decoded = decode(page);
    assert!(
        refused_over_the_cap(&decoded),
        "metadata-past-cap.jxl: {}",
        outcome(&decoded)
    );
}

/// The cap is on all of a page's boxes together: two that each fit under it
/// alone are refused when together they do not.
#[test]
fn two_boxes_each_under_the_cap_are_refused_together_over_it() {
    register();
    let sizes = [9 * MIB, 9 * MIB];
    let page = metadata_page("metadata-two-9mib.jxl", &sizes);
    // At run time, not in a `const` block: a cap changed so this fails would
    // otherwise stop the tests compiling, which a mutation sweep counts as a
    // mutant that cannot be built rather than one the tests caught.
    assert!(
        sizes.iter().all(|&size| size <= METADATA_CAP)
            && sizes.iter().sum::<usize>() > METADATA_CAP,
        "the fixture: each box fits under the cap and the two together do not"
    );
    let decoded = decode(page);
    assert!(
        refused_over_the_cap(&decoded),
        "metadata-two-9mib.jxl: {}",
        outcome(&decoded)
    );
}

/// An empty box is a whole box, and the boxes after it are still counted:
/// bomb.jxl with an empty `free` box put in after ftyp, ahead of its `brob`.
#[test]
fn an_empty_box_ahead_of_a_bomb_does_not_hide_it() {
    register();
    let bomb = bomb();
    let empty = [0, 0, 0, 8, b'f', b'r', b'e', b'e'];
    let page = [&bomb[..32], &empty, &bomb[32..]].concat();
    assert!(
        &bomb[16..20] == b"ftyp" && page[32..40] == empty && &page[44..48] == b"brob",
        "the fixture: the empty box sits between ftyp and the brob box"
    );

    let decoded = decode(&page);
    assert!(
        refused_over_the_cap(&decoded),
        "bomb.jxl behind an empty box: {}",
        outcome(&decoded)
    );
}
