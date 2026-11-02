use super::*;

/// The source is handed one member and an offset inside it; deciding
/// which member that is belongs to `taiyaku_core::Edition`. This test
/// pins the contract: no edition key, no flat index, no arithmetic below
/// this line.
#[test]
fn a_source_serves_one_member_at_an_offset_within_it() {
    let tmp = crate::test_support::Fixture::new("source");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions.get("raw").unwrap();

    let first = LocalLibrary.page(&root, &edition.members[0], 0).unwrap();
    let second_member = LocalLibrary.page(&root, &edition.members[1], 0).unwrap();
    let past_end = LocalLibrary.page(&root, &edition.members[1], 9);

    assert_eq!(first.bytes, b"a1.jpg");
    assert_eq!(
        second_member.bytes, b"b1.jpg",
        "the second member's own first page, not the edition's"
    );
    assert!(past_end.is_err(), "past a member's end must not wrap");
    assert_eq!(
        first.content_type, "image/jpeg",
        "a page carries what it is, not what the route guesses"
    );
}

/// The arithmetic the source does not do has to happen elsewhere: page 2 of
/// this two-member edition is the second member's first page.
#[test]
fn state_walks_a_flat_index_across_member_boundaries() {
    let tmp = crate::test_support::Fixture::new("state-page");
    let state = crate::test_support::state(&tmp);
    let held = state.works.snapshot();
    let work = &held[0];

    let count = work.sidecar().editions.get("raw").unwrap().total_pages();
    let first = state.page(work, "raw", 0).unwrap();
    let across = state.page(work, "raw", 2).unwrap();
    let last = state.page(work, "raw", 3).unwrap();
    let past_end = state.page(work, "raw", 4);
    let no_edition = state.page(work, "nope", 0);

    assert_eq!(count, 4);
    assert_eq!(first.bytes, b"a1.jpg");
    assert_eq!(
        across.bytes, b"b1.jpg",
        "page 2 is the second member's first"
    );
    assert_eq!(last.bytes, b"b2.jpg");

    let past_end = past_end.expect_err("past the end must not silently wrap");
    assert!(
        past_end.contains('4'),
        "must say how many pages there are: {past_end}"
    );

    let no_edition = no_edition.expect_err("an unknown edition is an error");
    assert!(
        no_edition.contains("nope"),
        "must name the edition: {no_edition}"
    );
}

/// Everything but an `<img>` needs the real type: a direct navigation
/// downloads an `application/octet-stream` instead of showing it, and the
/// OPDS feed a third-party reader consumes has to state one. The list is
/// `taiyaku_read`'s own image extensions, matched case-insensitively the
/// same way it matches them, so a page it counts is a page this can name.
#[test]
fn a_page_is_served_as_the_image_type_its_extension_names() {
    for (name, expected) in [
        ("p1.jpg", "image/jpeg"),
        ("p1.jpeg", "image/jpeg"),
        ("p1.PNG", "image/png"),
        ("p1.webp", "image/webp"),
        ("p1.gif", "image/gif"),
        ("p1.bmp", "image/bmp"),
        ("p1.avif", "image/avif"),
        ("p1.tif", "image/tiff"),
        ("Chapter 1/p1.TIFF", "image/tiff"),
    ] {
        assert_eq!(type_from_extension(name), expected, "{name}");
    }
    // Not an image extension taiyaku recognises, and not something to
    // claim a type for on a guess.
    assert_eq!(
        type_from_extension("ComicInfo.xml"),
        "application/octet-stream"
    );
    assert_eq!(type_from_extension("cover"), "application/octet-stream");
    // Bytes that name no format at all fall through to the extension,
    // which is the path every fixture in this crate takes.
    assert_eq!(content_type("p1.jpg", b"not really a jpeg"), "image/jpeg");
}

/// The extension is not evidence, and the corpus says so: some of its page
/// entries are PNG files named `.jpg`. Trusting the name serves a header that
/// is simply false — which a browser `<img>` re-sniffs and forgives, and the
/// OPDS client this type exists for does not. Same rule as `taiyaku_read`'s
/// container sniff, one level down.
#[test]
fn the_bytes_outrank_the_extension_when_they_disagree() {
    let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
    let jpeg = b"\xff\xd8\xff\xe0\x00\x10JFIF";

    assert_eq!(
        content_type("002.jpg", png),
        "image/png",
        "the corpus case: a PNG named .jpg"
    );
    assert_eq!(content_type("002.png", jpeg), "image/jpeg");
    assert_eq!(content_type("002.jpg", jpeg), "image/jpeg");

    assert_eq!(sniffed_type(b"GIF89a\x01\x00"), Some("image/gif"));
    assert_eq!(
        sniffed_type(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
        Some("image/webp")
    );
    assert_eq!(sniffed_type(b"II*\x00\x08\x00\x00\x00"), Some("image/tiff"));
    assert_eq!(sniffed_type(b"\x00\x00\x00 ftypavif"), Some("image/avif"));
    assert_eq!(sniffed_type(b"BM\x36\x00\x00\x00"), Some("image/bmp"));
    // Says nothing rather than guessing, so the name gets its turn.
    assert_eq!(sniffed_type(b"<?xml version=\"1.0\"?>"), None);
    assert_eq!(sniffed_type(b""), None);
}

/// Two of the sniffs need *both* halves of their test, so each is handed a
/// buffer that satisfies only one of them.
///
/// A RIFF container is not necessarily a WebP: `.wav` opens `RIFF` too, and
/// calling one `image/webp` sends a browser a header it will act on. The
/// `ftyp` box is the same shape — plenty of MP4s carry it and are not AVIF.
#[test]
fn a_riff_that_is_not_webp_and_an_ftyp_that_is_not_avif_are_not_images() {
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&[0, 0, 0, 0]);
    wav.extend_from_slice(b"WAVE");
    assert_eq!(sniffed_type(&wav), None, "RIFF alone is not a WebP");

    let mut webp = b"RIFF".to_vec();
    webp.extend_from_slice(&[0, 0, 0, 0]);
    webp.extend_from_slice(b"WEBP");
    assert_eq!(sniffed_type(&webp), Some("image/webp"));

    let mut mp4 = vec![0u8; 4];
    mp4.extend_from_slice(b"ftyp");
    mp4.extend_from_slice(b"isom");
    assert_eq!(sniffed_type(&mp4), None, "an ftyp box alone is not an AVIF");

    let mut avif = vec![0u8; 4];
    avif.extend_from_slice(b"ftyp");
    avif.extend_from_slice(b"avif");
    assert_eq!(sniffed_type(&avif), Some("image/avif"));

    // And the brand alone, with no ftyp box, is nothing either.
    let mut bare = vec![0u8; 8];
    bare.extend_from_slice(b"avif");
    assert_eq!(sniffed_type(&bare), None);
}
