//! Where page images come from.
//!
//! One trait, so that running beside Stump and replacing Stump are the same
//! code above this line. `LocalLibrary` reads the filesystem; `StumpLibrary`
//! (M3) proxies Stump's REST API.

use std::path::Path;

use crate::works::Work;

/// One page, with the content type a client needs to render it.
///
/// The type travels with the bytes because it is exactly what varies by
/// source: locally it comes from the entry's extension, and a remote library
/// (M3) will take it from that server's response header. A trait returning
/// bare bytes cannot express either, so the route was left guessing — and
/// `application/octet-stream` is a guess an `<img>` forgives and nothing else
/// does.
#[derive(Debug, Clone)]
pub struct ServedPage {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Where a reader's page images come from.
pub trait PageSource: Send + Sync + std::fmt::Debug {
    /// One page, by its index in the edition's flat sequence.
    fn page(&self, work: &Work, edition: &str, index: u32) -> Result<ServedPage, String>;
    /// How many pages the edition holds.
    fn page_count(&self, work: &Work, edition: &str) -> Result<u32, String>;
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
fn sniffed_type(bytes: &[u8]) -> Option<&'static str> {
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
    fn page(&self, work: &Work, edition: &str, index: u32) -> Result<ServedPage, String> {
        let ed = work
            .sidecar
            .editions
            .get(edition)
            .ok_or_else(|| format!("no edition `{edition}` in {}", work.path.display()))?;
        let root = work.path.parent().unwrap_or(Path::new("."));

        // Offsets come from the recorded page counts rather than from reading
        // each member, so skipping to the right member costs no I/O. The read
        // below is what catches a member that no longer holds what the sidecar
        // says it does.
        let mut offset = 0u32;
        for member in &ed.members {
            if index < offset + member.pages {
                let path = root.join(&member.path);
                let within = index - offset;
                let page = taiyaku_read::member_page(&path, within).map_err(|e| {
                    format!(
                        "{e} — {} records {} pages for this member; it may have been replaced",
                        work.path.display(),
                        member.pages
                    )
                })?;
                return Ok(ServedPage {
                    content_type: content_type(&page.name, &page.bytes).to_string(),
                    bytes: page.bytes,
                });
            }
            offset += member.pages;
        }
        Err(format!(
            "{edition} has {offset} pages; there is no page {index}"
        ))
    }

    fn page_count(&self, work: &Work, edition: &str) -> Result<u32, String> {
        let ed = work
            .sidecar
            .editions
            .get(edition)
            .ok_or_else(|| format!("no edition `{edition}` in {}", work.path.display()))?;
        Ok(ed.members.iter().map(|m| m.pages).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An edition is its members concatenated in array order, so page 2 is the
    /// first page of the second member. Getting the offset wrong shows the
    /// wrong page for everything after the first member, which is exactly the
    /// bug member ordering was fixed to prevent one level up.
    #[test]
    fn a_page_index_walks_across_member_boundaries() {
        let work = crate::test_support::work("taiyaku-test-source");

        let count = LocalLibrary.page_count(&work, "raw").unwrap();
        let first = LocalLibrary.page(&work, "raw", 0).unwrap();
        let across = LocalLibrary.page(&work, "raw", 2).unwrap();
        let last = LocalLibrary.page(&work, "raw", 3).unwrap();
        let past_end = LocalLibrary.page(&work, "raw", 4);

        crate::test_support::cleanup("taiyaku-test-source");
        assert_eq!(count, 4);
        assert_eq!(first.bytes, b"a1.jpg");
        assert_eq!(
            across.bytes, b"b1.jpg",
            "page 2 is the second member's first page"
        );
        assert_eq!(last.bytes, b"b2.jpg");
        assert!(past_end.is_err(), "past the end must not silently wrap");
        assert_eq!(
            first.content_type, "image/jpeg",
            "a page carries what it is, not what the route guesses"
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
}
