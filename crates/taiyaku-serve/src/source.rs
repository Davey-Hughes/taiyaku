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
//! every member Stump does not have.

use std::path::Path;

use taiyaku_core::Member;

/// One page, with the content type a client needs to render it.
///
/// The type travels with the bytes because it is exactly what varies by
/// source: locally it comes from sniffing the bytes, falling back to the
/// entry's extension when they do not sniff as anything recognised
/// (`content_type`, below). A trait returning bare bytes cannot express this,
/// so the route would be left guessing — and `application/octet-stream` is a
/// guess an `<img>` forgives and nothing else does.
#[derive(Debug, Clone)]
pub struct ServedPage {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// The bytes, so a page can be handed to `cover::cached_or_generated_page`
/// whole. It reads them to decide whether the page fits, and gives the page
/// back, content type included, when it does (O107).
impl AsRef<[u8]> for ServedPage {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
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
    ///
    /// # Errors
    ///
    /// Returns an error if the page at `within` cannot be produced from
    /// `member`. For `LocalLibrary`, that means the file or directory named
    /// by `root.join(&member.path)` cannot be opened, or holds no page at
    /// that index — the message then adds the sidecar's own recorded page
    /// count, since a member's disagreeing with it means the file was
    /// replaced. **For `CompositeSource` (`composite.rs`), a Stump-backed
    /// member's remote failure is not this method's `Err`.** A transport
    /// error, or a response that does not sniff as an image, is logged to
    /// stderr and the call falls through to `LocalLibrary` unconditionally —
    /// a Stump outage degrades to the local answer rather than being
    /// surfaced to the caller. So for such a member this method fails only
    /// when the local fallback itself does, once Stump has already been
    /// tried and given up on.
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
/// extension is frequently wrong about what is inside. A browser `<img>`
/// re-sniffs and would not care; the OPDS client M3 is for is entitled to
/// believe the header, and a wrong header is worse than one that says nothing.
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
mod tests;
