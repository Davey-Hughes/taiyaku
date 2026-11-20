//! What a container is, from its bytes rather than from its name.
//!
//! **Extensions lie in both directions**, which `README.md` records as the
//! reason `taiyaku-read` never trusted one: a zip is frequently named `.cbr`
//! and a real RAR frequently `.cbz`. The scanner's §3 makes that a rule —
//! `scan_entry.format` is sniffed, never derived from a suffix.
//!
//! **Recognising a format is not a step towards reading it.** `Pdf`,
//! `SevenZip`, `Tar` and the fixed-layout EPUB exist so a scan can record what
//! it met and a reader can be told why nothing opens it, and so does `Rar` in
//! a build without a RAR backend. `Zip` and `Dir` are readable as pages, as
//! `Rar` is in a build with a RAR backend, and the reflowable EPUB as text
//! (`book.rs`); [`Format::is_readable`] is the statement of that, and
//! `pages.rs` is what enforces it for pages — `unsupported_container` and
//! `read_member` are where a caller actually meets the refusal, and they answer
//! from the bytes in front of them rather than by asking this enum. What the
//! library admits is decided here, not by what `pages.rs` can open.

use std::io::Read;
use std::path::Path;

/// What a path holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Dir,
    Zip,
    EpubFixed,
    EpubReflow,
    Pdf,
    Rar,
    SevenZip,
    Tar,
    /// A file none of the signatures matched. Recorded rather than skipped,
    /// per §3: a scanner that silently drops what it cannot open is the
    /// failure `inventory` was written against.
    Unknown,
}

// **`Tar` is one more variant than §3's list**, which names
// `zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z | NULL`. It is here
// because `pages.rs` already recognises tar and already names it in an error
// — dropping it would mean this enum recorded a tar as `Unknown` while the
// error message beside it called it a tar archive, which is two answers to
// one question. `003_scanner.sql`'s `format` comment carries the same eight
// values for the same reason.

impl Format {
    /// The containers a library's derive admits as pages, and so what a scan
    /// counts and a fingerprint pass reads. RAR only in a build with a RAR
    /// backend, the one build whose `pages.rs` opens it.
    pub const PAGED: &'static [Self] = &[
        Self::Dir,
        Self::Zip,
        #[cfg(rar)]
        Self::Rar,
    ];

    /// The `scan_entry.format` value, or `None` for a file nothing recognised.
    ///
    /// The strings are the schema's own vocabulary and are not free text; a
    /// change here is a migration, not a rename.
    #[must_use]
    pub fn token(self) -> Option<&'static str> {
        match self {
            Self::Dir => Some("dir"),
            Self::Zip => Some("zip"),
            Self::EpubFixed => Some("epub-fixed"),
            Self::EpubReflow => Some("epub-reflow"),
            Self::Pdf => Some("pdf"),
            Self::Rar => Some("rar"),
            Self::SevenZip => Some("7z"),
            Self::Tar => Some("tar"),
            Self::Unknown => None,
        }
    }

    /// The format whose [`token`](Self::token) is `token`, or `None` for a
    /// string no format writes: how a stored `scan_entry.format` is asked
    /// [`is_readable`](Self::is_readable), or looked for in [`PAGED`](Self::PAGED).
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "dir" => Some(Self::Dir),
            "zip" => Some(Self::Zip),
            "epub-fixed" => Some(Self::EpubFixed),
            "epub-reflow" => Some(Self::EpubReflow),
            "pdf" => Some(Self::Pdf),
            "rar" => Some(Self::Rar),
            "7z" => Some(Self::SevenZip),
            "tar" => Some(Self::Tar),
            _ => None,
        }
    }

    /// Whether the library admits this: as pages ([`PAGED`](Self::PAGED)), or as
    /// an EPUB's text. `pages.rs` opening it does not decide.
    ///
    /// `taiyaku_library::derive` asks it of each stored token through
    /// [`from_token`](Self::from_token), so this is the one list of what a
    /// derive admits.
    ///
    /// **The `false`s are a decision, not a backlog — O90.** [`sniff`]
    /// names eight containers so a scan can say what it met, and this admits
    /// three, or four in a build with a RAR backend; the last `false` is
    /// `Unknown`, which is no container at all. **RAR is refused on licence,
    /// not on difficulty**, so only a build with the `rar-libarchive` feature
    /// admits it, through libarchive's own BSD-2-Clause RAR readers: taiyaku is
    /// GPL-3.0-or-later, and RARLAB's UnRAR source, which `unrar` builds and
    /// `unrar-rs` incorporates, carries a restriction the GPL does not allow a
    /// distributor to add.
    /// `docs/superpowers/specs/2026-09-15-o90-unreadable-attribution-design.md`
    /// §1 and `docs/superpowers/specs/2026-11-17-rar-libarchive-design.md`
    /// ("Readability") are the record.
    ///
    /// A refusal reaches a person as `DeriveReport::unreadable`'s
    /// `by_format`, which counts each refused format under its token.
    #[must_use]
    pub fn is_readable(self) -> bool {
        Self::PAGED.contains(&self) || self == Self::EpubReflow
    }
}

const SEVEN_ZIP: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
const RAR: &[u8] = b"Rar!\x1a\x07";
const ZIP: &[u8] = b"PK\x03\x04";
const PDF: &[u8] = b"%PDF-";

/// What is at `path`.
///
/// # Errors
///
/// Returns an error naming the path if it cannot be opened or read. A file
/// that is readable and matches nothing is [`Format::Unknown`], which is a
/// successful answer rather than an error.
pub fn sniff(path: &Path) -> Result<Format, String> {
    if path.is_dir() {
        return Ok(Format::Dir);
    }
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (header, read) = read_header(&mut file).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(match from_header(&header[..read]) {
        // The only branch that needs more than the header, and so the only
        // one that costs a second open. See `zip_flavour`.
        Format::Zip => zip_flavour(path),
        other => other,
    })
}

/// Fill a 512-byte header from `source`, and say how much of it is real.
///
/// **A loop, not one `read`.** `Read::read` is allowed to return fewer bytes
/// than the buffer holds even when more are available, and over a network
/// filesystem it does — this project's own library is mounted over nfs4.
/// Every signature but one lives in the first six bytes and survives a short
/// read; the tar magic sits at offset 257, so a single `read` returning 256
/// bytes would silently turn every tar into [`Format::Unknown`] while leaving
/// the other five answers correct, which is the shape of failure nobody
/// notices. `read_exact` is the wrong call for the same job: a file shorter
/// than 512 bytes is an ordinary case here — a stub, a truncated download —
/// not an error. `n == 0` is end of file, and the returned length is what
/// separates "the file ends here" from "the buffer is still zeroes".
///
/// **Taken as a `Read` rather than reached through a path**, for the reason
/// `lib.rs` splits `require_fts5_with_probe` from `require_fts5` and
/// `watch.rs` splits `report_line_against` from `report_line`: no fixture
/// this test suite can write to a local filesystem will make `File::read`
/// come back short, so the property is unreachable from a path-shaped test
/// and reachable from this one.
///
/// # Errors
///
/// Returns the underlying read error, unwrapped — [`sniff`] is what names
/// the path. `pages.rs`'s `open_archive` reads its header through this too, so
/// it and [`sniff`] cannot disagree about a tar.
pub(crate) fn read_header(source: &mut impl Read) -> std::io::Result<([u8; 512], usize)> {
    let mut header = [0u8; 512];
    let mut read = 0;
    while read < header.len() {
        let n = source.read(&mut header[read..])?;
        if n == 0 {
            break;
        }
        read += n;
    }
    Ok((header, read))
}

/// The decision every branch but one can make from the leading bytes alone.
///
/// **The EPUB refinement is deliberately not here**, and that is a
/// performance constraint rather than a tidiness preference. `pages.rs`'s
/// `unsupported_container` calls this on a header it already holds, from
/// inside `open_archive` — and `open_archive` runs on `member_page`, which is
/// **once per page served**, as well as on `member_page_names` (so on
/// `member_pages`), `member_page_sizes`, `member_entry` and `read_member`. A
/// version of this function that opened the archive to look for a `mimetype`
/// entry would double the open count on that path to answer a question the
/// error message does not need: an EPUB handed to `taiyaku align` is a zip
/// holding no pages, and `read_member` already reports it as holding no page
/// images. So this stays header-only and total, and [`sniff`] — which runs
/// once per file per scan, not once per page — does the refinement.
///
/// **The per-page claim is `member_page`'s alone.** `member_digest` is not a
/// caller: for a file it hashes the bytes with `std::fs::read` and never
/// opens an archive at all. `member_page` really is the hot path this
/// reasoning is about and the argument stands on it: it is the call the
/// reader makes for every page it displays.
///
/// **The tar test comes before the zip test, and the order is load-bearing.**
/// A tar's first 100 bytes are its first member's *filename*, and a tar
/// filename may hold any byte — including the four a zip header starts with.
/// Such a file is a tar that also answers `starts_with(ZIP)`, and checking
/// zip first hands it to the zip reader, which cannot open it.
/// `a_tar_whose_first_entry_name_begins_with_a_zip_signature_is_still_a_tar`
/// is that file, built and checked rather than argued about. The reverse
/// collision cannot happen: `ustar` at offset 257 of a real zip would have to
/// land inside the first local file header's name or data, and the tar test
/// is the more specific of the two.
pub(crate) fn from_header(bytes: &[u8]) -> Format {
    if bytes.starts_with(SEVEN_ZIP) {
        Format::SevenZip
    } else if bytes.starts_with(RAR) {
        Format::Rar
    } else if bytes.starts_with(PDF) {
        Format::Pdf
    } else if bytes.get(257..262) == Some(&b"ustar"[..]) {
        Format::Tar
    } else if bytes.starts_with(ZIP) {
        Format::Zip
    } else {
        Format::Unknown
    }
}

/// A zip is a zip unless it declares itself an EPUB, in which case the
/// question is which kind.
///
/// **What this establishes and what it does not.** An EPUB declares its
/// layout in the OPF package document, as
/// `<meta property="rendition:layout">pre-paginated</meta>`. This looks for
/// that string in every `.opf` entry rather than parsing XML — the workspace
/// has no XML dependency and adding one to answer a question nothing yet
/// reads would be the wrong trade. So it is a substring test: an OPF that
/// mentioned `pre-paginated` in a comment or a title would be misread as
/// fixed-layout. Nothing opens either variant (§10), so the cost of being
/// wrong here today is one wrong token in a column no reader consumes; when
/// spec H makes fixed-layout EPUB a real member format, this is the function
/// that needs a parser, and this paragraph is the note saying so.
///
/// A zip whose central directory will not open — a truncated download — is
/// [`Format::Zip`] on the strength of its signature. The scan records what
/// the bytes say; whether the archive is *sound* is a page-count question
/// and `member_pages` answers it separately.
fn zip_flavour(path: &Path) -> Format {
    let Ok(file) = std::fs::File::open(path) else {
        return Format::Zip;
    };
    let buffered = std::io::BufReader::with_capacity(crate::pages::ARCHIVE_BUFFER, file);
    let Ok(mut zip) = zip::ZipArchive::new(buffered) else {
        return Format::Zip;
    };
    let mut mimetype = String::new();
    if let Ok(mut entry) = zip.by_name("mimetype") {
        let _ = entry.read_to_string(&mut mimetype);
    }
    if mimetype.trim() != "application/epub+zip" {
        return Format::Zip;
    }
    let opf_names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .filter(|name| name.ends_with(".opf"))
        .map(std::borrow::Cow::into_owned)
        .collect();
    for name in opf_names {
        let mut body = String::new();
        if let Ok(mut entry) = zip.by_name(&name) {
            let _ = entry.read_to_string(&mut body);
        }
        if body.contains("pre-paginated") {
            return Format::EpubFixed;
        }
    }
    Format::EpubReflow
}

#[cfg(test)]
mod tests;
