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

/// The member at `path` under its root, counted at `pages`.
fn member(path: &str, pages: u32) -> Member {
    Member {
        path: path.to_owned(),
        sha256: String::new(),
        pages,
    }
}

/// A page past the end of a member that is read names the count the sidecar
/// holds and says the file may have been replaced: the count and the file
/// disagree, and the file is the one that can have moved.
#[test]
fn a_page_past_the_end_of_a_zip_says_the_member_may_have_been_replaced() {
    let tmp = crate::test_support::Fixture::new("source-past-end");
    crate::test_support::work(&tmp);

    let past_end = LocalLibrary
        .page(&tmp, &member("c1.cbz", 7), 7)
        .expect_err("the zip holds two pages");

    assert!(
        past_end.contains("the sidecar records 7 pages for this member"),
        "names the count the sidecar holds: {past_end}"
    );
    assert!(
        past_end.contains("it may have been replaced"),
        "a count the file does not bear out points at the file: {past_end}"
    );
}

/// A RAR counted by a build that reads RAR and asked of one that does not is
/// refused for the build, and the refusal is handed on as it is: the file is
/// as it was counted, so saying it may have been replaced points an operator
/// at the wrong cause. In a build that reads RAR a page past the end is a
/// count the file does not bear out, as it is for a zip.
#[test]
fn a_rar_page_error_blames_a_replaced_file_only_in_a_build_that_reads_rar() {
    let tmp = crate::test_support::Fixture::new("source-rar-refused");
    let fixture = taiyaku_read::test_support::rar_fixtures()
        .into_iter()
        .find(|fixture| fixture.file == "rar5-compressed.rar")
        .expect("the fixture");
    std::fs::copy(&fixture.path, tmp.join("volume.cbr")).expect("copies the fixture");
    let pages = u32::try_from(fixture.pages.len()).expect("a page count");
    let volume = member("volume.cbr", pages);

    if !taiyaku_read::Format::Rar.is_readable() {
        let refused = LocalLibrary
            .page(&tmp, &volume, 0)
            .expect_err("this build reads no RAR");
        assert!(
            refused.contains("built without the rar-libarchive feature"),
            "names the build as the cause: {refused}"
        );
        assert!(
            !refused.contains("replaced") && !refused.contains("sidecar"),
            "and nothing about the file or its count: {refused}"
        );
        return;
    }
    LocalLibrary
        .page(&tmp, &volume, 0)
        .expect("this build reads the RAR's first page");
    let past_end = LocalLibrary
        .page(&tmp, &volume, pages)
        .expect_err("past the RAR's last page");
    assert!(
        past_end.contains(&format!(
            "the sidecar records {pages} pages for this member"
        )) && past_end.contains("it may have been replaced"),
        "a count the file does not bear out points at the file: {past_end}"
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
    let whole = |key: &str| {
        crate::view::EditionView::build(std::sync::Arc::clone(work), key, None)
            .expect("an edition is never hidden from `Access::Every`")
    };
    let first = state.page(&whole("raw"), 0).unwrap();
    let across = state.page(&whole("raw"), 2).unwrap();
    let last = state.page(&whole("raw"), 3).unwrap();
    let past_end = state.page(&whole("raw"), 4);
    let no_edition = state.page(&whole("nope"), 0);

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

/// Both JPEG XL signatures, each by its own assertion: a codestream is two
/// bytes and a container twelve, so a check for one that were loosened to
/// swallow the other would still pass the other's input.
#[test]
fn jpeg_xl_bytes_sniff_as_jpeg_xl() {
    let mut codestream = JXL_CODESTREAM.to_vec();
    codestream.extend_from_slice(&[0x12, 0x34]);
    assert_eq!(
        sniffed_type(&codestream),
        Some("image/jxl"),
        "the codestream signature and bytes after it"
    );

    let mut container = JXL_CONTAINER.to_vec();
    container.extend_from_slice(b"ftyp");
    assert_eq!(
        sniffed_type(&container),
        Some("image/jxl"),
        "the container signature and bytes after it"
    );

    assert_eq!(
        sniffed_type(JXL_CODESTREAM),
        Some("image/jxl"),
        "the two codestream bytes alone"
    );

    let mut broken = container.clone();
    broken[7] ^= 0xff;
    assert_eq!(
        sniffed_type(&broken),
        None,
        "a container signature with its eighth byte changed"
    );

    assert_eq!(
        sniffed_type(b"\xff\xd8\xff\xe0\x00\x10JFIF"),
        Some("image/jpeg"),
        "a JPEG still sniffs as a JPEG"
    );
    assert_eq!(
        sniffed_type(b"\xff\xd8"),
        None,
        "two bytes of a JPEG signature are not a codestream"
    );
    assert_eq!(
        sniffed_type(b"\xff\x0b\x00\x00"),
        None,
        "FF followed by anything but 0A is not a codestream"
    );
    assert_eq!(type_from_extension("p.JXL"), "image/jxl");
}
