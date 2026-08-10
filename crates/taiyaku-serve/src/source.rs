//! Where page images come from.
//!
//! One trait, so that running beside Stump and replacing Stump are the same
//! code above this line. **Two implementations, not three.** `LocalLibrary`
//! (below) reads the filesystem. There is no standalone `StumpLibrary`: the
//! design's third implementation was never built, because it could not have
//! answered anything on its own — resolving a member to a Stump book needs
//! the resolution map, which only the composite holds. What ships instead is
//! `CompositeSource` (`composite.rs`), whose Stump-backed branch speaks
//! Stump's GraphQL and REST APIs inline and falls back to `LocalLibrary` for
//! every member Stump does not have. Whole-branch review, M3: the design and
//! seven places in the docs and code still named the unbuilt type; this is
//! the correction.

use std::path::Path;

use taiyaku_core::Member;

/// One page, with the content type a client needs to render it.
///
/// The type travels with the bytes because it is exactly what varies by
/// source: locally it comes from sniffing the bytes, falling back to the
/// entry's extension when they do not sniff as anything recognised
/// (`content_type`, below). A trait returning bare bytes cannot express this,
/// so the route was left guessing — and `application/octet-stream` is a
/// guess an `<img>` forgives and nothing else does.
#[derive(Debug, Clone)]
pub struct ServedPage {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Where a reader's page images come from.
///
/// One member at a time, deliberately. Which member holds page N of an
/// edition is `taiyaku_core::Edition::locate`, and it is the same answer for
/// every source — so a source that computed it would be a second derivation of
/// a normative rule. What genuinely varies below this line is only where one
/// member's bytes live, which is why that is all this trait asks for.
pub trait PageSource: Send + Sync + std::fmt::Debug {
    /// One page of one member, by its offset within that member.
    ///
    /// `root` is the directory the sidecar sits in; a member's `path` is
    /// relative to it — `docs/sidecar.md`.
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String>;
}

/// What to serve a page as, from the name of the entry it came out of.
///
/// The extensions are the ones `taiyaku_read` already recognises as pages, so
/// the fallback is reached only by an entry taiyaku would not have counted as
/// a page in the first place. It stays rather than becoming an error because
/// serving bytes a client may still be able to sniff beats serving nothing.
fn type_from_extension(name: &str) -> &'static str {
    let extension = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        Some("tif" | "tiff") => "image/tiff",
        _ => "application/octet-stream",
    }
}

/// What the bytes say they are, where they say it unambiguously.
///
/// Same reasoning as `taiyaku_read`'s container sniff, one level down: the
/// extension is frequently wrong about what is inside. Measured rather than
/// assumed — **114 of the 8,637 page entries in the corpus are PNG files named
/// `.jpg`**, all in one fan-translated chapter of Go-Toubun v01. A browser
/// `<img>` re-sniffs and would not care; the OPDS client M3 is for is entitled
/// to believe the header, and a header that is wrong 1.3% of the time is worse
/// than one that says nothing.
pub(crate) fn sniffed_type(bytes: &[u8]) -> Option<&'static str> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("image/gif")
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else if starts(b"II*\x00") || starts(b"MM\x00*") {
        Some("image/tiff")
    } else if bytes.get(4..8) == Some(b"ftyp")
        && matches!(bytes.get(8..12), Some(b"avif" | b"avis"))
    {
        Some("image/avif")
    } else if starts(b"BM") {
        // Two bytes, so it goes last: everything with a longer magic has
        // already had its say.
        Some("image/bmp")
    } else {
        None
    }
}

/// What to serve a page as. The bytes decide where they can, the name where
/// they cannot.
fn content_type(name: &str, bytes: &[u8]) -> &'static str {
    sniffed_type(bytes).unwrap_or_else(|| type_from_extension(name))
}

/// Pages read from the filesystem, relative to the sidecar.
#[derive(Debug, Clone, Copy)]
pub struct LocalLibrary;

impl PageSource for LocalLibrary {
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String> {
        let path = root.join(&member.path);
        let page = taiyaku_read::member_page(&path, within).map_err(|e| {
            format!(
                "{e} — the sidecar records {} pages for this member; it may have been replaced",
                member.pages
            )
        })?;
        Ok(ServedPage {
            content_type: content_type(&page.name, &page.bytes).to_string(),
            bytes: page.bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The source is handed one member and an offset inside it; deciding
    /// which member that is belongs to `taiyaku_core::Edition`. This test
    /// pins the narrowed contract: no edition key, no flat index, no
    /// arithmetic below this line.
    #[test]
    fn a_source_serves_one_member_at_an_offset_within_it() {
        let work = crate::test_support::work("taiyaku-test-source");
        let root = work.path.parent().unwrap().to_path_buf();
        let sidecar = work.sidecar();
        let edition = sidecar.editions.get("raw").unwrap();

        let first = LocalLibrary.page(&root, &edition.members[0], 0).unwrap();
        let second_member = LocalLibrary.page(&root, &edition.members[1], 0).unwrap();
        let past_end = LocalLibrary.page(&root, &edition.members[1], 9);

        crate::test_support::cleanup("taiyaku-test-source");
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

    /// The arithmetic the source no longer does still has to happen, and it
    /// has to give the same answers: page 2 of this two-member edition is the
    /// second member's first page. This is the same property the old test
    /// asserted, now checked one level up.
    #[test]
    fn state_walks_a_flat_index_across_member_boundaries() {
        let state = crate::test_support::state("taiyaku-test-state-page");
        let work = &state.works[0];

        let count = work.sidecar().editions.get("raw").unwrap().total_pages();
        let first = state.page(work, "raw", 0).unwrap();
        let across = state.page(work, "raw", 2).unwrap();
        let last = state.page(work, "raw", 3).unwrap();
        let past_end = state.page(work, "raw", 4);
        let no_edition = state.page(work, "nope", 0);

        crate::test_support::cleanup("taiyaku-test-state-page");
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

    /// The extension is not evidence, and the corpus says so: 114 of its 8,637
    /// page entries are PNG files named `.jpg`, all in one fan-translated
    /// chapter of the primary volume. Trusting the name serves 1.3% of that
    /// library under a header that is simply false — which a browser `<img>`
    /// re-sniffs and forgives, and the OPDS client this type exists for does
    /// not. Same rule as `taiyaku_read`'s container sniff, one level down.
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

    /// Two of the sniffs need *both* halves of their test, and `&&` becoming
    /// `||` in either survived every existing case — because every buffer the
    /// other tests hand it satisfies both halves or neither.
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
}
