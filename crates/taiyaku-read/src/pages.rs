//! Assembling an edition's flat page sequence from its members.
#![cfg_attr(
    not(rar),
    expect(
        clippy::infallible_destructuring_match,
        reason = "a build without RAR opens only zips, so a match on an archive has one arm"
    )
)]

use std::{
    collections::HashSet,
    fmt::Write as _,
    fs::File,
    io::{BufReader, Read, Seek},
    path::{Path, PathBuf},
};

use crate::format::Format;
use crate::naming::natural_cmp;
#[cfg(rar)]
use crate::rar::backend;

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "avif", "tif", "tiff", "jxl",
];

/// What this reader counts as a page in a container of `format`, a
/// `scan_entry.format` token, and which backend opens it. A scan stores it with
/// each file's count and counts again, without hashing, each file whose stored
/// one differs, so a change here re-counts exactly the files of that format,
/// and any other change to which entries are pages must change it too.
/// `None` for every other format: a folder is counted on every walk, a
/// reflowable EPUB by its text, and the rest have no count.
#[must_use]
pub fn reader_signature(format: &str) -> Option<String> {
    (Some(format) == Format::Zip.token())
        .then(|| format!("{format}:{}", IMAGE_EXTENSIONS.join(",")))
}

/// The most a `Vec::with_capacity` hint for one archive entry's raw bytes may
/// reserve, before `read_archive`/`member_page` read the entry for real.
///
/// `entry.size()` is the uncompressed-size field the `zip` crate reads
/// straight from the archive's central directory — metadata the archive
/// declares about itself, never checked against the real bytes until they are
/// inflated. `usize::try_from(entry.size())` alone does not bound it: on every
/// host this crate actually targets (64-bit; it does not compile to wasm) the
/// conversion is infallible, so a `.cbz` that misstates the field — hand-
/// edited, or merely corrupt — sails straight through to `with_capacity`,
/// which reserves the declared size immediately.
///
/// It is a hint, not a limit, and that is what should size it:
/// `read_to_end` grows the buffer to the true size regardless of what was
/// reserved up front, so a hint that is too small costs one reallocation and
/// nothing else — being tight is free. The number that matters is therefore
/// not "large enough to cover the biggest page the corpus holds"; it is
/// "large enough that an ordinary page never reallocates, small enough that a
/// lying declared size cannot turn one request into a large reservation."
/// Real pages on this corpus run 100 KB–3 MB
/// (`taiyaku-cli::fingerprint::LARGE_PAGE_BYTES`'s doc comment). 32 MiB clears
/// that with roughly an order of magnitude to spare — and lands on the same
/// number `LARGE_PAGE_BYTES` independently draws as the line between an
/// ordinary page and an unusual one, on this same corpus, for an unrelated
/// decision (parallel vs. sequential decode).
///
/// The corpus does hold pages bigger than 32 MiB — the Color Walk art
/// books' ~430 MB archival TIFFs (`lib.rs`) — and this ceiling is
/// deliberately below them: reading one costs a few reallocations
/// instead of none, which the hint-not-limit property above makes harmless.
/// Deliberately not `MAX_DECODE_ALLOC` either: that 3 GiB ceiling bounds a
/// *decoded* pixel buffer, and even at the corpus's own worst case
/// (10200×14039, 24-bit) that buffer is only 1.33× the encoded file's
/// bytes — ~430 MB on disk, ~546 MiB decoded — and it is not a number this
/// hint should be derived from either way.
///
/// **This clamp is per entry, and `read_archive` collects every page before
/// returning**, so a crafted archive of N lying entries reserves the ceiling
/// N times over, simultaneously, before any of them is freed: at 32 MiB, 100
/// such entries reserve at most 3.125 GiB in aggregate.
const MAX_ENTRY_CAPACITY_HINT: u64 = 32 * 1024 * 1024;

pub struct Page {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Hand-written so a page prints as its name and size. Deriving it would dump
/// several megabytes of JPEG into any error or test failure that touches one.
impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Page({}, {} bytes)", self.name, self.bytes.len())
    }
}

/// Whether an entry name is a page.
///
/// Public because the duplicate inventory counts pages without reading them and
/// must not carry its own copy of this list — which entries are pages is
/// normative (`docs/sidecar.md`, "How a member becomes pages"), and a second
/// list is a duplication (O10). A change to what this answers must change
/// [`reader_signature`] too, or a zip counted before it keeps its old count.
#[must_use]
pub fn is_page_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Flatten an edition into one page sequence.
///
/// An edition is a list of members, not a single file: one volume of the raw
/// corresponds to five or six chapter archives of a fan translation. Both sides
/// flatten before anything else happens, so volume-against-chapters stops being
/// a special case. Members are ordered naturally rather than as given, because a
/// shell glob sorts `c005.5` ahead of `c005`.
///
/// A zip entry name that is not valid UTF-8 does not fail this: `zip`'s decode
/// cannot fail — a name that is valid UTF-8 is read as UTF-8 whatever the UTF-8
/// flag says, and any other bytes are mapped through codepage 437 — so a
/// mis-encoded entry is garbled, never refused, and still takes its place in
/// the natural-order page sequence.
///
/// # Errors
///
/// Returns an error on the first member, in natural order, that fails: its
/// path is neither a readable directory nor an openable archive, it names a
/// recognised-but-unsupported container — 7-Zip, tar, or RAR in a build
/// without a RAR backend, named as such rather than reported as a corrupt
/// cbz — it holds no entry this crate reads as a page, or one of its pages'
/// bytes cannot be read back: a directory entry removed or made unreadable
/// since listing, or an archive entry whose compressed data does not decode.
/// In the no-pages case, for a directory member, the message also lists the
/// extensions it did hold, so a `.webp`-only release is distinguishable from
/// an empty one; an archive with no page entries just says so.
pub fn read_edition(members: &[PathBuf]) -> Result<Vec<Page>, String> {
    let mut ordered = members.to_vec();
    ordered.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    read_members_in_order(&ordered)
}

/// Flatten an edition whose member order is already decided.
///
/// The sidecar's `members` array *is* the page order — `docs/sidecar.md`
/// rule 1 — and its paths may point into unrelated trees, so sorting them by
/// name can reorder the volume. `read_edition` sorts because its caller hands
/// it shell globs; a caller holding a sidecar must not.
///
/// A zip entry name that is not valid UTF-8 does not fail this: `zip`'s decode
/// cannot fail — a name that is valid UTF-8 is read as UTF-8 whatever the UTF-8
/// flag says, and any other bytes are mapped through codepage 437 — so a
/// mis-encoded entry is garbled, never refused, and still takes its place in
/// that member's own natural-order page sequence.
///
/// # Errors
///
/// Returns an error on the first member, in the given order, that fails: its
/// path is neither a readable directory nor an openable archive, it names a
/// recognised-but-unsupported container — 7-Zip, tar, or RAR in a build
/// without a RAR backend, named as such rather than reported as a corrupt
/// cbz — it holds no entry this crate reads as a page, or one of its pages'
/// bytes cannot be read back: a directory entry removed or made unreadable
/// since listing, or an archive entry whose compressed data does not decode.
/// In the no-pages case, for a directory member, the message also lists the
/// extensions it did hold, so a `.webp`-only release is distinguishable from
/// an empty one; an archive with no page entries just says so.
pub fn read_members_in_order(members: &[PathBuf]) -> Result<Vec<Page>, String> {
    let mut pages = Vec::new();
    for member in members {
        pages.extend(read_member(member)?);
    }
    Ok(pages)
}

/// Read one member, which may be a cbz archive or a directory of page images.
///
/// Directories are not an edge case: most of the raw editions in the test
/// corpus are stored as loose images rather than as archives.
fn read_member(path: &Path) -> Result<Vec<Page>, String> {
    let pages = if path.is_dir() {
        read_directory(path)?
    } else {
        read_archive(path)?
    };
    if pages.is_empty() {
        return Err(format!(
            "{}: no page images{}",
            path.display(),
            skipped(path)
        ));
    }
    Ok(pages)
}

/// Describe what a member held instead of pages, so an empty read is
/// diagnosable rather than merely disappointing.
fn skipped(path: &Path) -> String {
    let mut extensions: Vec<String> = Vec::new();
    if path.is_dir()
        && let Ok(entries) = std::fs::read_dir(path)
    {
        for entry in entries.flatten() {
            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                let ext = ext.to_ascii_lowercase();
                if !extensions.contains(&ext) {
                    extensions.push(ext);
                }
            }
        }
    }
    extensions.sort();
    if extensions.is_empty() {
        String::new()
    } else {
        format!(
            ". It holds {}, none of which taiyaku reads as a page",
            extensions
                .iter()
                .map(|e| format!(".{e}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// The paths of a directory's pages, in reading order, without reading them.
fn directory_page_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = directory_files(root)?
        .into_iter()
        .filter(|path| is_page_name(&path.to_string_lossy()))
        .collect();
    found.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    Ok(found)
}

/// Every file under a directory, in no order, less dotfiles and whatever sits
/// in a dot-directory.
fn directory_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let base = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if base.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    Ok(found)
}

fn read_directory(root: &Path) -> Result<Vec<Page>, String> {
    directory_page_paths(root)?
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(Page {
                name: path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
                bytes,
            })
        })
        .collect()
}

/// Name a container taiyaku cannot read, from its leading bytes.
///
/// Zip-based archives and directories are read, and RAR with a RAR backend.
/// Recognising the others is not a step towards reading them — it is so the
/// error can say which format it found rather than reporting a corrupt zip,
/// since the extension is frequently wrong about what is actually inside.
///
/// **The recognition itself lives in `format.rs`**, because the scanner needs
/// the same answer as a token rather than as prose and two tables of magic
/// numbers would drift. This is the prose half; that is the decision half.
///
/// An EPUB is **not** named here, deliberately. Telling one from a plain zip
/// means opening the archive and reading its `mimetype` entry, and this
/// function runs inside `open_archive` — once per page served. `read_member`
/// already reports a page-less archive as holding no page images.
fn unsupported_container(bytes: &[u8]) -> Option<&'static str> {
    match crate::format::from_header(bytes) {
        crate::Format::SevenZip => Some("a 7-Zip archive"),
        #[cfg(not(rar))]
        crate::Format::Rar => Some("a RAR archive"),
        crate::Format::Tar => Some("a tar archive"),
        crate::Format::Pdf => Some("a PDF"),
        _ => None,
    }
}

/// The names of an already-open archive's page entries, in reading order.
///
/// Takes the open `ZipArchive` rather than a path, so a caller that already
/// holds one — `read_archive`, `member_page` — can order its entries without
/// opening the path a second time. A second open reads whatever file is at
/// that path *now*, which need not be the file the first handle is still
/// reading if the path was replaced in between (an atomic rename-over during
/// a re-download, say). Two handles on two different files agreeing only by
/// name is a silent content mismatch, not a clean error, so this is the one
/// place — for both listing and reading — that decides archive page order.
fn page_names_from<R: Read + Seek>(zip: &zip::ZipArchive<R>) -> Vec<String> {
    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| {
            // From the central directory the open already parsed, never the
            // entry's local header: `by_index` seeks to each local header, a
            // round trip per page on a network share. The shared borrow holds
            // this, since nothing reaches the reader through it. So an entry
            // whose local header is damaged is listed and fails when read,
            // rather than dropping out and moving every later page back one.
            let entry = zip.by_index_data(i).ok()?;
            if !entry.is_file() || !zip_can_read(&entry) {
                return None;
            }
            let name = entry.name().ok()?.into_owned();
            // A change to what this listing skips or reads must change
            // `reader_signature` too, or a zip counted before it keeps its old
            // count.
            is_listed_page(&name).then_some(name)
        })
        .collect();
    // Central-directory order is not guaranteed to be page order.
    names.sort_by(|a, b| natural_cmp(a, b));
    names
}

/// Whether a name is a resource fork or a dotfile, which the archive listing
/// never reads as a page whatever its extension. A folder's walk skips dotfiles
/// itself and asks this only to tell a companion.
fn is_fork_or_dotfile(name: &str) -> bool {
    name.starts_with("__MACOSX")
        || name
            .rsplit('/')
            .next()
            .is_some_and(|base| base.starts_with('.'))
}

/// Whether an archive entry of this name is listed as a page, in every
/// container. Resource forks and dotfiles are not pages.
fn is_listed_page(name: &str) -> bool {
    !is_fork_or_dotfile(name) && is_page_name(name)
}

/// Whether `zip` will open an entry for reading with no password, judged
/// from the central directory alone: the refusals `by_index` makes before it
/// reads a byte of the entry's data. An encrypted entry, one carrying AES
/// settings whatever its flag says, and one in a method `zip` cannot decode
/// are not pages. Listing them would give a locked archive pages that all
/// fail, and move every page after one to a new index.
fn zip_can_read(entry: &zip::read::ZipFileEntry<'_>) -> bool {
    use zip::HasZipMetadata as _;
    #[expect(
        deprecated,
        reason = "`Unsupported` is how `zip` marks a method it has no decoder for"
    )]
    let undecodable = matches!(entry.compression(), zip::CompressionMethod::Unsupported(_));
    !entry.encrypted() && entry.get_metadata().aes_settings().is_none() && !undecodable
}

/// Refuse a file whose leading bytes name a container taiyaku cannot read, or
/// give the format they name.
///
/// Takes a `Read` so a short read is reachable from a test; a local file never
/// comes back short. The header is filled by `format::read_header`, because the
/// tar magic sits at offset 257 and one `read` may stop before it. A read error
/// assumes no header, and the zip reader then gives the answer.
fn refuse_unsupported_container(
    source: &mut impl Read,
    path: &Path,
) -> Result<crate::Format, String> {
    let (header, read) = crate::format::read_header(source).unwrap_or(([0u8; 512], 0));
    let format = crate::format::from_header(&header[..read]);
    match unsupported_container(&header[..read]) {
        // A build with a RAR backend reads this file, so the build is the cause.
        #[cfg(not(rar))]
        Some(kind) if format == crate::Format::Rar => Err(format!(
            "{}: this is {kind}, and this build of taiyaku reads no RAR (it was built without the rar-libarchive feature); convert it to zip or unpack it first",
            path.display()
        )),
        Some(kind) => Err(format!(
            "{}: this is {kind}. taiyaku reads zip-based archives and directories of page images; convert it or unpack it first",
            path.display()
        )),
        None => Ok(format),
    }
}

/// An archive `open_archive` let in: a zip, open, or a RAR, which the backend
/// opens again on each call.
enum Archive {
    Zip(zip::ZipArchive<BufReader<File>>),
    #[cfg(rar)]
    Rar,
}

/// Open an archive, naming its format first if taiyaku cannot read it.
///
/// The single door in, so that listing an archive, reading one page of it and
/// reading all of it cannot differ in what they say about a container taiyaku
/// does not support. Going straight to `ZipArchive::new` would report a
/// perfectly good RAR file as an unreadable cbz — and `member_pages`, which
/// lists rather than reads, is the path `align` takes when it records a member.
fn open_archive(path: &Path) -> Result<Archive, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;

    // Look at the bytes before handing them to the zip reader, so an archive in
    // another format is named rather than reported as a corrupt zip.
    match refuse_unsupported_container(&mut file, path)? {
        #[cfg(rar)]
        crate::Format::Rar => Ok(Archive::Rar),
        _ => {
            file.rewind()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            zip::ZipArchive::new(BufReader::with_capacity(ARCHIVE_BUFFER, file))
                .map(Archive::Zip)
                .map_err(|e| format!("{}: not a readable cbz: {e}", path.display()))
        }
    }
}

/// The buffer every archive is opened through.
///
/// `zip` parses each central-directory entry with several small reads, so
/// an unbuffered file pays reads per page even when no local header is
/// touched, each a round trip on a network share. Through this buffer a
/// central directory that fits in it arrives in one read, whatever the page
/// count. `format::zip_flavour` opens through it too.
pub(crate) const ARCHIVE_BUFFER: usize = 64 * 1024;

/// The names of an archive's page entries, in reading order.
fn archive_page_names(path: &Path) -> Result<Vec<String>, String> {
    match open_archive(path)? {
        Archive::Zip(zip) => Ok(page_names_from(&zip)),
        #[cfg(rar)]
        Archive::Rar => rar_page_names(path),
    }
}

fn read_archive(path: &Path) -> Result<Vec<Page>, String> {
    let mut zip = match open_archive(path)? {
        Archive::Zip(zip) => zip,
        #[cfg(rar)]
        Archive::Rar => {
            let names = rar_page_names(path)?;
            return rar_visit_keeping(path, &names, |name, bytes| Page {
                name: name.to_owned(),
                bytes: bytes.to_vec(),
            });
        }
    };
    let names = page_names_from(&zip);

    names
        .into_iter()
        .map(|name| {
            let mut entry = zip
                .by_name(&name)
                .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
            // `entry.size()` is unverified archive metadata a hostile or
            // corrupt .cbz can misstate arbitrarily — see
            // `MAX_ENTRY_CAPACITY_HINT`'s doc comment. Clamped rather than
            // passed straight to `with_capacity`, so the reservation this
            // hint drives is bounded by construction; `read_to_end` still
            // grows the buffer to the real size regardless of the hint. The
            // clamp is what makes the cast that follows provably safe —
            // clippy verifies it against `MAX_ENTRY_CAPACITY_HINT`'s value
            // rather than needing to be told so.
            let hint = entry.size().min(MAX_ENTRY_CAPACITY_HINT) as usize;
            let mut bytes = Vec::with_capacity(hint);
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
            Ok(Page { name, bytes })
        })
        .collect()
}

/// A member's page count, as `member_pages` reports it.
///
/// # Panics
///
/// Panics if `n` exceeds `u32::MAX`. It cannot in this crate: `n` is
/// `names.len()`, the number of page-image entries `member_page_names`
/// actually found — real directory entries `directory_page_paths` walked, or
/// archive entries `page_names_from` read out of a zip's already-parsed
/// central directory. Unlike `MAX_ENTRY_CAPACITY_HINT`'s `entry.size()`,
/// which one unchecked field can misstate arbitrarily, this count cannot be
/// inflated without the directory or archive actually holding that many
/// entries: reaching `u32::MAX` would take either that many real directory
/// entries or a zip whose central directory alone ran past 100 GB, the same
/// physical-limit argument `taiyaku-core::solve`'s `page_index` makes for a
/// page count. Panicking rather than clamping is deliberate for the same
/// reason `page_index` panics rather than clamps: a clamp here would report
/// a wrong page count for the edition, exactly the silently-wrong result a
/// clamp was rejected for elsewhere on this branch.
fn page_count(n: usize) -> u32 {
    u32::try_from(n).expect("page count exceeds u32::MAX")
}

/// The names of a member's pages, in reading order, without reading any bytes.
///
/// For a zip this reads only the central directory, for a RAR its entry
/// headers, and for a directory the listing. That is what makes serving one
/// page of a 195-page volume cost one page rather than the volume.
///
/// A zip entry name that is not valid UTF-8 does not fail this: `zip`'s decode
/// cannot fail — a name that is valid UTF-8 is read as UTF-8 whatever the UTF-8
/// flag says, and any other bytes are mapped through codepage 437 — so a
/// mis-encoded entry is garbled, never refused, and the garbled name is what
/// comes back in this listing.
///
/// # Errors
///
/// For a directory, returns an error if `path`, or a directory nested under
/// it, cannot be listed. For an archive, returns an error if the file cannot
/// be opened, if its leading bytes identify a recognised-but-unsupported
/// container — 7-Zip, tar, or RAR in a build without a RAR backend, named as
/// such rather than reported as a corrupt cbz — or if it parses as neither a
/// zip archive nor a RAR archive the backend can list.
pub fn member_page_names(path: &Path) -> Result<Vec<String>, String> {
    if path.is_dir() {
        Ok(directory_page_paths(path)?
            .iter()
            .map(|p| {
                p.strip_prefix(path)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string()
            })
            .collect())
    } else {
        archive_page_names(path)
    }
}

/// One page, addressed by its index within the member.
///
/// The name comes back with the bytes because the caller cannot recover it:
/// `taiyaku-serve` needs the extension to say what it is serving, and asking
/// for the listing again to find out would read the archive twice.
///
/// A zip entry name that is not valid UTF-8 does not fail this either: `zip`'s
/// decode cannot fail — a name that is valid UTF-8 is read as UTF-8 whatever
/// the UTF-8 flag says, and any other bytes are mapped through codepage 437 —
/// so it comes back garbled rather than refused, at whatever `index` its
/// position in the natural-order sort gives it.
///
/// # Errors
///
/// Returns an error if `path` cannot be opened as a directory or an
/// archive — including a recognised-but-unsupported container such as
/// 7-Zip, tar, or RAR in a build without a RAR backend, named as such — if
/// `index` is at or past the member's page count (`"no page {index}"`), or
/// if the located page's bytes cannot be read back: a directory entry
/// removed or made unreadable since listing, or an archive entry whose
/// compressed data does not decode.
pub fn member_page(path: &Path, index: u32) -> Result<Page, String> {
    let i = index as usize;
    if path.is_dir() {
        let paths = directory_page_paths(path)?;
        let page = paths
            .get(i)
            .ok_or_else(|| format!("{}: no page {index}", path.display()))?;
        let bytes = std::fs::read(page).map_err(|e| format!("{}: {e}", page.display()))?;
        return Ok(Page {
            name: page
                .strip_prefix(path)
                .unwrap_or(page)
                .to_string_lossy()
                .to_string(),
            bytes,
        });
    }
    let archive = open_archive(path)?;
    let names = match &archive {
        Archive::Zip(zip) => page_names_from(zip),
        #[cfg(rar)]
        Archive::Rar => rar_page_names(path)?,
    };
    let name = names
        .get(i)
        .cloned()
        .ok_or_else(|| format!("{}: no page {index}", path.display()))?;
    match archive {
        Archive::Zip(mut zip) => entry_page(&mut zip, path, name),
        #[cfg(rar)]
        Archive::Rar => rar_page(path, name),
    }
}

/// One page of the archive at `path`, by the name [`member_page_names`]
/// listed it under, for a caller that keeps the listing: the entry is found
/// by that name, in a zip's central directory or a RAR's entry headers,
/// without sorting the pages again, and reads as [`member_page`] reads it at
/// the name's index.
///
/// # Errors
///
/// Returns an error if `path` cannot be opened as an archive — a directory
/// among them, whose pages only a listing finds — including a
/// recognised-but-unsupported container named as such, if it parses as
/// neither a zip archive nor a RAR archive the backend can list, if the
/// archive holds no entry of that name, or if its compressed data does not
/// decode.
pub fn archive_page(path: &Path, name: &str) -> Result<Page, String> {
    match open_archive(path)? {
        Archive::Zip(mut zip) => entry_page(&mut zip, path, name.to_string()),
        // Listed first, so it is refused as the listings refuse it: a read by
        // name stops at its entry, before a cut or a refused entry after it.
        #[cfg(rar)]
        Archive::Rar => {
            rar_entries(path)?;
            rar_page(path, name.to_string())
        }
    }
}

/// The entry `name` of `zip`, opened from `path`, read whole.
fn entry_page(
    zip: &mut zip::ZipArchive<BufReader<File>>,
    path: &Path,
    name: String,
) -> Result<Page, String> {
    let mut entry = zip
        .by_name(&name)
        .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
    // Same reasoning as `read_archive` above: `entry.size()` is unverified
    // archive metadata, clamped to `MAX_ENTRY_CAPACITY_HINT` so the hint it
    // drives is bounded by construction; `read_to_end` still grows the
    // buffer to fit the real size regardless. The clamp is what makes the
    // cast provably safe, so clippy verifies it rather than needing an
    // `#[expect]` to be told so.
    let hint = entry.size().min(MAX_ENTRY_CAPACITY_HINT) as usize;
    let mut bytes = Vec::with_capacity(hint);
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
    Ok(Page { name, bytes })
}

/// Content hash of a member, so a reader can tell the archive was replaced.
///
/// Over the file for an archive, and over every page in order for a directory —
/// a directory has no single file to hash, and hashing the pages is what
/// actually identifies the content.
///
/// # Errors
///
/// For a directory, returns an error if it, or a directory nested under it,
/// cannot be listed, or if one of its page files cannot be read back —
/// removed or made unreadable since listing. A directory that lists cleanly
/// but holds no pages is *not* an error here: it hashes to the digest of
/// nothing, because this reads pages directly rather than through
/// `read_member`'s emptiness check. For a single file, returns an error if
/// it cannot be read.
pub fn member_digest(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    if path.is_dir() {
        for page in read_directory(path)? {
            hasher.update(&page.bytes);
        }
    } else {
        hasher.update(std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?);
    }
    Ok(hasher.finalize().iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

/// How many pages a member holds, without reading any of them.
///
/// A zip entry name that is not valid UTF-8 does not fail this: `zip`'s decode
/// cannot fail — a name that is valid UTF-8 is read as UTF-8 whatever the UTF-8
/// flag says, and any other bytes are mapped through codepage 437 — so a
/// mis-encoded entry is garbled, never refused, and if its extension still
/// matches, it still counts toward the total.
///
/// # Errors
///
/// Returns an error if the member cannot be listed — the same conditions as
/// `member_page_names`: an unreadable directory, an unopenable archive, a
/// recognised-but-unsupported container named as such, or an archive that
/// does not parse. If the member opens and lists fine but holds no entry
/// this crate reads as a page, returns `read_member`'s error for it, which —
/// for a directory — names the extensions found instead, so a `.webp`-only
/// release reads differently from an empty one.
pub fn member_pages(path: &Path) -> Result<u32, String> {
    let names = member_page_names(path)?;
    if names.is_empty() {
        // A member that opened fine and holds nothing taiyaku reads as a page.
        // `read_member` is the only place that names what it held instead, so
        // borrow its diagnostic rather than reporting a bare zero. A container
        // taiyaku cannot open at all is named by `open_archive` above, before
        // this line is reached.
        read_member(path)?;
    }
    Ok(page_count(names.len()))
}

/// The uncompressed byte size of each of a member's pages, in reading order,
/// without reading any of them.
///
/// Directory pages are sized from their metadata and archive entries from the
/// zip central directory or the RAR entry headers, which record the
/// uncompressed size. Uncompressed on both sides deliberately: a directory of
/// loose images and a `.cbz` of the same scan then produce the same list, and
/// seeing through packaging is the whole job of the signature this feeds.
///
/// A listing, not a read — the property that lets an inventory run over a NAS
/// and decide what is worth copying.
///
/// A zip entry name that is not valid UTF-8 does not fail this: `zip`'s decode
/// cannot fail — a name that is valid UTF-8 is read as UTF-8 whatever the UTF-8
/// flag says, and any other bytes are mapped through codepage 437 — so a
/// mis-encoded entry is garbled, never refused, and its size is still listed at
/// the position the garbled name sorts to.
///
/// # Errors
///
/// For a directory, returns an error if it, or a directory nested under it,
/// cannot be listed, or if a page file's metadata cannot be read — removed
/// or made unreadable since listing. For an archive, returns an error if it
/// cannot be opened, if its leading bytes identify a recognised-but-
/// unsupported container — 7-Zip, tar, or RAR in a build without a RAR
/// backend, named as such — if it parses as neither a zip archive nor a RAR
/// archive the backend can list, or if a listed entry cannot be looked up
/// again by name.
pub fn member_page_sizes(path: &Path) -> Result<Vec<u64>, String> {
    if path.is_dir() {
        return directory_page_paths(path)?
            .iter()
            .map(|page| {
                std::fs::metadata(page)
                    .map(|meta| meta.len())
                    .map_err(|e| format!("{}: {e}", page.display()))
            })
            .collect();
    }
    let mut zip = match open_archive(path)? {
        Archive::Zip(zip) => zip,
        #[cfg(rar)]
        Archive::Rar => {
            let (pages, _) = rar_pages(path)?;
            return Ok(pages.iter().map(|page| page.size).collect());
        }
    };
    let names = page_names_from(&zip);
    names
        .iter()
        .map(|name| {
            zip.by_name(name)
                .map(|entry| entry.size())
                .map_err(|e| format!("{}: {name}: {e}", path.display()))
        })
        .collect()
}

/// One page entry, described without being decoded.
///
/// The triple a `STORE` zip's local header needs, and the reason this exists
/// beside `member_page_names` and `member_page_sizes` rather than replacing
/// them: those two are listings, and this one hashes. For a **zip** all three
/// come out of the central directory, so its page data is never touched; a
/// **directory** has no crc anywhere and a **RAR**'s backend gives none, so
/// each page is read once to compute it. Spec C2 §4 records why paying that on
/// directory members beat the alternative (data descriptors) and why folding
/// the listings onto this would have been a regression, not a de-duplication.
///
/// `size` is the **uncompressed** size on both sides, which is what a `STORE`
/// output writes and what makes a directory of loose images and a `.cbz` of the
/// same scan produce the same list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEntry {
    pub name: String,
    pub size: u64,
    pub crc32: u32,
}

/// One directory page, described and read together: the member-relative
/// name, the entry built from the bytes actually read, and those bytes
/// themselves.
///
/// Shared by `member_page_entries` and `for_each_member_page`'s directory
/// branches. Not shared with `member_page_names`, `member_page_sizes` or
/// `member_pages` — those three are deliberately separate listings that never
/// read a page's bytes, and folding them in here would cost every scanner call
/// a read it does not need.
fn directory_page_entry(member: &Path, page: &Path) -> Result<(PageEntry, Vec<u8>), String> {
    let name = page
        .strip_prefix(member)
        .unwrap_or(page)
        .to_string_lossy()
        .to_string();
    // Read rather than `metadata` + read: the size that goes into the header
    // and the bytes that were hashed then provably come from the same read,
    // where a stat first would leave a window in which the file changed size
    // between the two.
    let bytes = std::fs::read(page).map_err(|e| format!("{}: {e}", page.display()))?;
    let entry = PageEntry {
        name,
        size: bytes.len() as u64,
        crc32: crc32fast::hash(&bytes),
    };
    Ok((entry, bytes))
}

/// Every page of a member, described in reading order.
///
/// The name is the same one `member_page_names` reports — whole path relative
/// to the member for a directory, the bare entry name for an archive — and a
/// zip entry name not valid UTF-8 arrives garbled rather than refused, for
/// the reason that function's doc comment gives.
///
/// # Errors
///
/// For a directory, returns an error if it, or a directory nested under it,
/// cannot be listed, or if a page cannot be read back — removed or made
/// unreadable since listing. For an archive, returns an error if it cannot be
/// opened, if its leading bytes identify a recognised-but-unsupported
/// container — 7-Zip, tar, or RAR in a build without a RAR backend, named as
/// such — if it parses as neither a zip archive nor a RAR archive the backend
/// can list, or if a listed entry cannot be looked up again by name or, in a
/// RAR, read.
pub fn member_page_entries(path: &Path) -> Result<Vec<PageEntry>, String> {
    if path.is_dir() {
        return directory_page_paths(path)?
            .iter()
            .map(|page| directory_page_entry(path, page).map(|(entry, _bytes)| entry))
            .collect();
    }
    let mut zip = match open_archive(path)? {
        Archive::Zip(zip) => zip,
        #[cfg(rar)]
        Archive::Rar => return rar_visit_keeping(path, &rar_page_names(path)?, rar_page_entry),
    };
    let names = page_names_from(&zip);
    names
        .iter()
        .map(|name| {
            let entry = zip
                .by_name(name)
                .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
            Ok(PageEntry {
                name: name.clone(),
                size: entry.size(),
                crc32: entry.crc32(),
            })
        })
        .collect()
}

/// A page-visiting closure: one page's entry and bytes in, an early-stop
/// decision out. Named so `for_each_member_page`'s signature reads as a type
/// rather than as the unnamed `dyn FnMut(...)` clippy flags as too complex to
/// inline — the same fix `taiyaku-cli::duplicates::Fingerprint` uses for the
/// identical complaint on an identically-shaped closure type.
type PageVisitor<'a> = dyn FnMut(&PageEntry, &[u8]) -> Result<(), String> + 'a;

/// Every page of a member, handed to `f` one at a time, in reading order.
///
/// **One page in memory at a time, and one archive open for a zip.** A RAR is
/// opened twice, or once more per page where it does not store its pages in
/// reading order. `member_page` opens per index, right for a route
/// serving one page and wrong for a download reading two hundred of them: this
/// is that loop with the open hoisted out. `read_edition` holds *every* page.
///
/// The `PageEntry` travels with the bytes so a caller that planned an output
/// from `member_page_entries` can check the member still matches as it
/// streams — on both `size` and `crc32`. That is true of `crc32` on its own
/// regardless: `zip`'s `Crc32Reader` validates it against the real bytes on
/// every read, so a lying crc32 fails inside `read_to_end` before this
/// function ever sees the entry. It is true of `size` too only because this
/// function, for an archive, reports the byte count `read_to_end` actually
/// produced rather than the central directory's declared `uncompressed_size`.
/// `zip` fails a read that runs past that field, but an entry that declares
/// more than it holds reads back whole with a good crc32. `member_page_entries`
/// reports the declared size, because reading the bytes to verify it would
/// destroy the property that function exists for. So the two deliberately
/// disagree on an archive that overstates its own size: a plan built from
/// `member_page_entries` and a stream read through this function are then
/// comparable on `size` as well as `crc32`, and an archive replaced — or
/// corrupted — mid-download is detectable rather than producing a file whose
/// local header states a length its payload does not have.
///
/// **`f`'s `Err` stops the walk on the spot** and is returned unchanged. A
/// client that hung up must not cost the rest of the volume.
///
/// # Errors
///
/// The errors of [`member_page_entries`], plus whatever `f` returns, plus — for
/// an archive — an entry that does not decode or runs past its declared size.
pub fn for_each_member_page(path: &Path, f: &mut PageVisitor<'_>) -> Result<(), String> {
    if path.is_dir() {
        for page in directory_page_paths(path)? {
            let (entry, bytes) = directory_page_entry(path, &page)?;
            f(&entry, &bytes)?;
        }
        return Ok(());
    }

    let mut zip = match open_archive(path)? {
        Archive::Zip(zip) => zip,
        #[cfg(rar)]
        Archive::Rar => return rar_for_each_page(path, f),
    };
    let names = page_names_from(&zip);
    for name in names {
        let mut handle = zip
            .by_name(&name)
            .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
        // crc32 is read now but not trusted yet: `read_to_end` below validates
        // it against the real bytes (`zip`'s `Crc32Reader`) and errors on a
        // mismatch, so by the time `f` sees it, it is proven rather than
        // merely declared.
        let crc32 = handle.crc32();
        // `size()` is unverified archive metadata — see
        // `MAX_ENTRY_CAPACITY_HINT`'s doc comment — so it drives a bounded
        // hint here, never the entry `f` receives. `read_to_end` grows the
        // buffer to the real size regardless of what was reserved.
        let hint = handle.size().min(MAX_ENTRY_CAPACITY_HINT) as usize;
        let mut bytes = Vec::with_capacity(hint);
        handle
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
        drop(handle);
        // Built from the bytes just read, not `handle.size()`'s declared
        // figure — see this function's doc comment for why the two must be
        // able to disagree.
        let entry = PageEntry {
            name: name.clone(),
            size: bytes.len() as u64,
            crc32,
        };
        f(&entry, &bytes)?;
    }
    Ok(())
}

/// One named entry's raw bytes, read directly rather than filtered through
/// `is_page_name` — for a member's metadata, `ComicInfo.xml` above all, which
/// `page_names_from` deliberately excludes from the page sequence and which no
/// page-reading function in this module can see.
///
/// Matched case-insensitively and only at the member's own level — an archive
/// entry's bare name, or a directory's direct child — since that is where
/// every `ComicInfo.xml` this crate has read sits: beside the pages, never
/// nested under them the way `directory_page_paths` walks to find images.
///
/// `Ok(None)`, never an error, means the member opened fine but holds no entry
/// of that name — the common case, since most members carry no `ComicInfo.xml`
/// at all, and that absence is not a failure to read them.
///
/// # Errors
///
/// For a directory, returns an error if it cannot be listed, or if the
/// matched entry cannot be read back. For an archive, returns an error if it
/// cannot be opened, if its leading bytes identify a recognised-but-
/// unsupported container — 7-Zip, tar, or RAR in a build without a RAR
/// backend, named as such rather than reported as a corrupt cbz — if it parses
/// as neither a zip archive nor a RAR archive the backend can list, or if the
/// matched entry cannot be read back.
pub fn member_entry(path: &Path, name: &str) -> Result<Option<Vec<u8>>, String> {
    if path.is_dir() {
        let dir = std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for entry in dir {
            let entry = entry.map_err(|e| format!("{}: {e}", path.display()))?;
            let matches = entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name);
            if matches && entry.path().is_file() {
                let bytes = std::fs::read(entry.path())
                    .map_err(|e| format!("{}: {e}", entry.path().display()))?;
                return Ok(Some(bytes));
            }
        }
        return Ok(None);
    }

    let mut zip = match open_archive(path)? {
        Archive::Zip(zip) => zip,
        #[cfg(rar)]
        Archive::Rar => {
            let matched = rar_entries(path)?
                .into_iter()
                .find(|entry| entry.readable && entry.name.eq_ignore_ascii_case(name));
            return matched
                .map(|entry| backend().read(path, &entry.name))
                .transpose();
        }
    };
    let matched = (0..zip.len()).find_map(|i| {
        let entry = zip.by_index(i).ok()?;
        let entry_name = entry.name().ok()?;
        (entry.is_file() && entry_name.eq_ignore_ascii_case(name)).then(|| entry_name.into_owned())
    });
    let Some(matched) = matched else {
        return Ok(None);
    };
    let mut entry = zip
        .by_name(&matched)
        .map_err(|e| format!("{}: {matched}: {e}", path.display()))?;
    // Same reasoning as `read_archive` and `member_page` above: `entry.size()`
    // is unverified archive metadata, clamped to `MAX_ENTRY_CAPACITY_HINT` so
    // the hint it drives is bounded by construction.
    let hint = entry.size().min(MAX_ENTRY_CAPACITY_HINT) as usize;
    let mut bytes = Vec::with_capacity(hint);
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {matched}: {e}", path.display()))?;
    Ok(Some(bytes))
}

/// The file entries of a member that are neither a page nor a companion, in
/// natural order: a page in a format this crate does not read, an entry `zip`
/// will not open, a file of a type it does not know. Each is something a later
/// reader may read, so a file holding one has not been read whole.
///
/// A companion is a resource fork or dotfile the listing skips, or a file of a
/// type in [`COMPANION_EXTENSIONS`], at any depth. Each comes back named as
/// [`member_page_names`] names a page: an archive entry's name, a directory
/// file's path under the member.
///
/// # Errors
///
/// As [`member_page_names`]: a directory that cannot be listed, or an archive
/// that cannot be opened, is a container this crate does not read, or parses
/// as neither a zip archive nor a RAR archive the backend can list.
pub fn member_unread_entries(path: &Path) -> Result<Vec<String>, String> {
    let mut unread: Vec<String> = if path.is_dir() {
        directory_files(path)?
            .iter()
            .filter(|file| !is_page_name(&file.to_string_lossy()))
            .map(|file| {
                file.strip_prefix(path)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .to_string()
            })
            .filter(|name| !is_companion(name))
            .collect()
    } else {
        match open_archive(path)? {
            Archive::Zip(zip) => {
                let pages: HashSet<String> = page_names_from(&zip).into_iter().collect();
                (0..zip.len())
                    .filter_map(|i| {
                        let entry = zip.by_index_data(i).ok()?;
                        let name = entry.name().ok()?.into_owned();
                        (entry.is_file() && !pages.contains(&name) && !is_companion(&name))
                            .then_some(name)
                    })
                    .collect()
            }
            #[cfg(rar)]
            Archive::Rar => rar_unread_entries(path)?,
        }
    };
    unread.sort_by(|a, b| natural_cmp(a, b));
    Ok(unread)
}

/// The types, in lower case, of the files a release carries beside its pages
/// that are not images; `ComicInfo.xml`, `Thumbs.db` and `desktop.ini` are
/// among them by type. An image type the reader cannot decode yet stays out,
/// so a file holding one is partial and read again once the reader learns it.
const COMPANION_EXTENSIONS: &[&str] = &[
    "txt", "nfo", "url", "sfv", "md5", "sha1", "xml", "json", "htm", "html", "md", "rtf", "ini",
    "db", "ds_store",
];

/// Whether an entry is one a member carries beside its pages that the reader
/// knows is not a page.
fn is_companion(name: &str) -> bool {
    is_fork_or_dotfile(name)
        || Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| COMPANION_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Every entry of a RAR, refusing one that holds none: libarchive lists a
/// file cut short after its signature as an empty archive.
#[cfg(rar)]
fn rar_entries(path: &Path) -> Result<Vec<crate::rar::Entry>, String> {
    let entries = backend().entries(path)?;
    if entries.is_empty() {
        return Err(format!(
            "{}: not a readable RAR archive: no entries",
            path.display()
        ));
    }
    Ok(entries)
}

/// A RAR's page entries in reading order, by the rules a zip's follow, and
/// whether the archive already stores them in that order.
#[cfg(rar)]
fn rar_pages(path: &Path) -> Result<(Vec<crate::rar::Entry>, bool), String> {
    let mut pages: Vec<_> = rar_entries(path)?
        .into_iter()
        .filter(|entry| entry.readable && is_listed_page(&entry.name))
        .collect();
    let stored_in_order = pages.is_sorted_by(|a, b| natural_cmp(&a.name, &b.name).is_le());
    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    Ok((pages, stored_in_order))
}

/// A RAR's regular files that are neither a page nor a companion.
#[cfg(rar)]
fn rar_unread_entries(path: &Path) -> Result<Vec<String>, String> {
    Ok(rar_entries(path)?
        .into_iter()
        .filter(|entry| {
            entry.readable && !is_listed_page(&entry.name) && !is_companion(&entry.name)
        })
        .map(|entry| entry.name)
        .collect())
}

/// The names of a RAR's pages, in reading order.
#[cfg(rar)]
fn rar_page_names(path: &Path) -> Result<Vec<String>, String> {
    let (pages, _) = rar_pages(path)?;
    Ok(pages.into_iter().map(|page| page.name).collect())
}

#[cfg(rar)]
fn rar_page(path: &Path, name: String) -> Result<Page, String> {
    let bytes = backend().read(path, &name)?;
    Ok(Page { name, bytes })
}

/// Described from the bytes the backend handed over, since its listing gives
/// no crc32.
#[cfg(rar)]
fn rar_page_entry(name: &str, bytes: &[u8]) -> PageEntry {
    PageEntry {
        name: name.to_owned(),
        size: bytes.len() as u64,
        crc32: crc32fast::hash(bytes),
    }
}

/// What `keep` makes of each page in `names`, from one visit, in the order of
/// `names`. A page the visit never handed over is an error.
#[cfg(rar)]
fn rar_visit_keeping<T>(
    path: &Path,
    names: &[String],
    keep: impl Fn(&str, &[u8]) -> T,
) -> Result<Vec<T>, String> {
    let wanted: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
    let mut kept = std::collections::HashMap::new();
    backend().visit(path, &|name| wanted.contains(name), &mut |name, bytes| {
        kept.entry(name.to_owned())
            .or_insert_with(|| keep(name, bytes));
        Ok(())
    })?;
    names
        .iter()
        .map(|name| {
            kept.remove(name)
                .ok_or_else(|| format!("{}: {name}: not read", path.display()))
        })
        .collect()
}

#[cfg(rar)]
fn rar_for_each_page(path: &Path, f: &mut PageVisitor<'_>) -> Result<(), String> {
    let (pages, stored_in_order) = rar_pages(path)?;
    // Either way one page is in memory at a time. A visit decodes each page
    // once, but hands them over in archive order; a read per name keeps the
    // reading order, and in a solid archive decodes every entry before it.
    if stored_in_order {
        let wanted: std::collections::HashSet<&str> =
            pages.iter().map(|page| page.name.as_str()).collect();
        let mut handed = 0;
        backend().visit(path, &|name| wanted.contains(name), &mut |name, bytes| {
            handed += 1;
            f(&rar_page_entry(name, bytes), bytes)
        })?;
        return match pages.get(handed) {
            Some(page) => Err(format!("{}: {}: not read", path.display(), page.name)),
            None => Ok(()),
        };
    }
    for page in pages {
        let bytes = backend().read(path, &page.name)?;
        f(&rar_page_entry(&page.name, &bytes), &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(all(test, rar))]
mod rar_tests;
