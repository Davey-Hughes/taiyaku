//! What a member holds that the reader would pass over without a word.
//!
//! The reader takes the entries it can decode as pages and leaves the rest, so
//! a volume with some pages in another format fingerprints as a shorter volume
//! and scores lower, and nothing in the run would show it. That is refused
//! before the first fingerprint, while the one scoring is still unspent.

use std::fs::File;
use std::io::{BufReader, Read as _};
use std::path::Path;

use taiyaku_read::{is_page_name, member_unread_entries};

use super::super::lookup::edition_paths;
use super::super::manifest::ARCHIVES;
use super::members::TestMember;

/// Image types the reader has no decoder for; [`is_page_name`] holds the ones
/// it has. A name is only one sign of an image, so [`is_image`] asks the bytes.
const OTHER_IMAGE_TYPES: [&str; 31] = [
    "jxl", "heic", "heif", "hif", "jp2", "j2k", "j2c", "jpx", "jpf", "jfif", "pjpeg", "pjp",
    "apng", "ico", "psd", "tga", "dng", "svg", "pcx", "pnm", "ppm", "pgm", "pbm", "exr", "hdr",
    "qoi", "dds", "raw", "cr2", "nef", "arw",
];

/// Enough of an entry to hold every signature [`is_image`] looks for.
const HEAD: u64 = 32;

/// Whether these first bytes open an image, whatever the entry is called.
fn is_image(head: &[u8]) -> bool {
    const OPENINGS: [&[u8]; 10] = [
        b"\xFF\xD8\xFF",
        b"\x89PNG",
        b"GIF8",
        b"BM",
        b"II*\0",
        b"MM\0*",
        b"\xFF\x0A",
        b"\0\0\0\x0CJXL ",
        b"\0\0\0\x0CjP  ",
        b"\xFF\x4F\xFF\x51",
    ];
    let webp = head.starts_with(b"RIFF") && head.get(8..12) == Some(&b"WEBP"[..]);
    // HEIF and AVIF open with a box of this type after its four-byte length.
    let boxed = head.get(4..8) == Some(&b"ftyp"[..]);
    webp || boxed || OPENINGS.iter().any(|opening| head.starts_with(opening))
}

/// The first bytes of `name` in the member at `path`, or `None` where they
/// cannot be had, as from an encrypted entry.
fn head_of(path: &Path, name: &str) -> Option<Vec<u8>> {
    let mut head = Vec::new();
    if path.is_dir() {
        let file = File::open(path.join(name)).ok()?;
        file.take(HEAD).read_to_end(&mut head).ok()?;
    } else {
        let file = BufReader::new(File::open(path).ok()?);
        let mut zip = zip::ZipArchive::new(file).ok()?;
        let entry = zip.by_name(name).ok()?;
        entry.take(HEAD).read_to_end(&mut head).ok()?;
    }
    Some(head)
}

/// Whether a passed-over entry could hold pages. Bytes that cannot be read
/// count as pages: the run cannot show they are not, and a needless stop costs
/// far less than a volume scored short.
fn may_hold_pages(name: &str, head: Option<&[u8]>) -> bool {
    let extension = (name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let extension = extension.as_str();
    is_page_name(name)
        || OTHER_IMAGE_TYPES.contains(&extension)
        || ARCHIVES.contains(&extension)
        || head.is_none_or(is_image)
}

/// Every entry passed over in the member at `root`, and whether it could hold
/// pages. A directory of archives is listed archive by archive, and then for
/// what lies beside them, which no archive's listing would name.
fn passed_over(root: &Path) -> Result<Vec<(String, bool)>, String> {
    let edition = edition_paths(root)?;
    let mut found = Vec::new();
    let mut note = |within: &Path, name: String| {
        let pages = may_hold_pages(&name, head_of(within, &name).as_deref());
        found.push((name, pages));
    };
    for path in &edition {
        for name in member_unread_entries(path)? {
            note(path, name);
        }
    }
    if edition.as_slice() != [root] {
        for name in member_unread_entries(root)? {
            if !edition.contains(&root.join(&name)) {
                note(root, name);
            }
        }
    }
    Ok(found)
}

/// How many entries of each member the reader passes over that cannot be
/// pages, in member order, or a refusal naming every entry that could be.
pub(super) fn left_out(library: &Path, members: &[TestMember]) -> Result<Vec<usize>, String> {
    let mut counts = Vec::with_capacity(members.len());
    let mut refused = Vec::new();
    for member in members {
        let (pages, others): (Vec<_>, Vec<_>) = passed_over(&library.join(&member.path))?
            .into_iter()
            .partition(|(_, pages)| *pages);
        counts.push(others.len());
        refused.extend(
            pages
                .into_iter()
                .map(|(name, _)| format!("{}: {name}", member.path)),
        );
    }
    if refused.is_empty() {
        Ok(counts)
    } else {
        Err(format!(
            "the reader would pass over {} entries that may be pages, so their volumes would be \
             scored short: {}",
            refused.len(),
            refused.join(", ")
        ))
    }
}
