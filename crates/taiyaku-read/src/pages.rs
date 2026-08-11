//! Assembling an edition's flat page sequence from its members.

use std::{
    fmt::Write as _,
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use crate::naming::natural_cmp;

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "avif", "tif", "tiff",
];

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
/// which reserves the declared size immediately. `docs/notes.md` measured a
/// patched archive reporting 2 GiB for 104 real bytes.
///
/// It is a hint, not a limit, and that is what should size it:
/// `read_to_end` grows the buffer to the true size regardless of what was
/// reserved up front, so a hint that is too small costs one reallocation and
/// nothing else — being tight is free. The number that matters is therefore
/// not "large enough to cover the biggest page the corpus holds" (an earlier
/// version of this constant reasoned that way, and reserved 1 GiB per entry
/// for it — 1,073,741,824 bytes for a 6-byte entry, in the regression test
/// below); it is "large enough that an ordinary page never reallocates,
/// small enough that a lying declared size cannot turn one request into a
/// large reservation." Real pages on this corpus run 100 KB–3 MB
/// (`taiyaku-cli::fingerprint::LARGE_PAGE_BYTES`'s doc comment) and a live
/// client session transferred at 247 KB mean, 2.0 MB largest
/// (`docs/notes.md`). 32 MiB clears all of that with roughly an order of
/// magnitude to spare — and lands on the same number `LARGE_PAGE_BYTES`
/// independently draws as the line between an ordinary page and an unusual
/// one, on this same corpus, for an unrelated decision (parallel vs.
/// sequential decode).
///
/// The corpus does hold pages bigger than 32 MiB — the Color Walk art
/// books' ~430 MB archival TIFFs (`lib.rs`) — and this ceiling is
/// deliberately below them: reading one now costs a few reallocations
/// instead of none, which the hint-not-limit property above makes harmless.
/// Deliberately not `MAX_DECODE_ALLOC` either: that 3 GiB ceiling bounds a
/// *decoded* pixel buffer, and even at the corpus's own worst case
/// (10200×14039, 24-bit) that buffer is only 1.33× the encoded file's
/// bytes — ~430 MB on disk, ~546 MiB decoded — not "several times" as an
/// earlier version of this comment claimed, and not a number this hint
/// should be derived from either way.
///
/// **This clamp is per entry, and `read_archive` collects every page before
/// returning**, so a crafted archive of N lying entries reserves the ceiling
/// N times over, simultaneously, before any of them is freed. At the old
/// 1 GiB ceiling, a 100-entry archive could reserve 100 GiB at once; at
/// 32 MiB the same 100 entries reserve at most 3.125 GiB in aggregate.
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
/// list is the duplication O10 was closed to remove.
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
/// An entry name that is not valid UTF-8 does not fail this: `zip`'s decode is
/// total, not fallible — invalid bytes come back as `U+FFFD`, or are mapped
/// through codepage 437 if the UTF-8 flag is unset — so a mis-encoded entry is
/// garbled, never refused, and still takes its place in the natural-order page
/// sequence.
///
/// # Errors
///
/// Returns an error on the first member, in natural order, that fails: its
/// path is neither a readable directory nor an openable zip-based archive, it
/// names a recognised-but-unsupported container — 7-Zip, RAR, or tar, named
/// as such rather than reported as a corrupt cbz — it holds no entry this
/// crate reads as a page, or one of its pages' bytes cannot be read back: a
/// directory entry removed or made unreadable since listing, or an archive
/// entry whose compressed data does not decode. In the no-pages case, for a
/// directory member, the message also lists the extensions it did hold, so a
/// `.webp`-only release is distinguishable from an empty one; an archive with
/// no page entries just says so.
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
/// An entry name that is not valid UTF-8 does not fail this: `zip`'s decode is
/// total, not fallible — invalid bytes come back as `U+FFFD`, or are mapped
/// through codepage 437 if the UTF-8 flag is unset — so a mis-encoded entry is
/// garbled, never refused, and still takes its place in that member's own
/// natural-order page sequence.
///
/// # Errors
///
/// Returns an error on the first member, in the given order, that fails: its
/// path is neither a readable directory nor an openable zip-based archive, it
/// names a recognised-but-unsupported container — 7-Zip, RAR, or tar, named
/// as such rather than reported as a corrupt cbz — it holds no entry this
/// crate reads as a page, or one of its pages' bytes cannot be read back: a
/// directory entry removed or made unreadable since listing, or an archive
/// entry whose compressed data does not decode. In the no-pages case, for a
/// directory member, the message also lists the extensions it did hold, so a
/// `.webp`-only release is distinguishable from an empty one; an archive with
/// no page entries just says so.
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
    if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    let ext = ext.to_ascii_lowercase();
                    if !extensions.contains(&ext) {
                        extensions.push(ext);
                    }
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
            } else if is_page_name(&base) {
                found.push(path);
            }
        }
    }
    found.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
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
/// Only zip-based archives are supported. Recognising the others is not a step
/// towards reading them — it is so the error can say which format it found
/// rather than reporting a corrupt zip, since the extension is frequently wrong
/// about what is actually inside.
fn unsupported_container(bytes: &[u8]) -> Option<&'static str> {
    const SEVEN_ZIP: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
    const RAR: &[u8] = b"Rar!\x1a\x07";

    if bytes.starts_with(SEVEN_ZIP) {
        Some("a 7-Zip archive")
    } else if bytes.starts_with(RAR) {
        Some("a RAR archive")
    // `get` rather than a length test and an index: the manual bound was
    // `len() > 262`, one past what the slice needs, so a 262-byte buffer
    // carrying the signature read as unrecognised. Unreachable with a real tar
    // — the format's first header block alone is 512 bytes — but the guard now
    // says exactly what it means and has no arithmetic to get wrong.
    } else if bytes.get(257..262) == Some(&b"ustar"[..]) {
        Some("a tar archive")
    } else {
        None
    }
}

/// The names of an already-open archive's page entries, in reading order.
///
/// Factored out of `archive_page_names` so a caller that already holds a
/// `ZipArchive` — `read_archive`, `member_page` — can order its entries
/// without opening the path a second time. A second open reads whatever file
/// is at that path *now*, which need not be the file the first handle is
/// still reading if the path was replaced in between (an atomic rename-over
/// during a re-download, say). Two handles on two different files agreeing
/// only by name is a silent content mismatch, not a clean error, so this is
/// the one place — for both listing and reading — that decides archive page
/// order.
fn page_names_from(zip: &mut zip::ZipArchive<File>) -> Vec<String> {
    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| {
            let entry = zip.by_index(i).ok()?;
            if !entry.is_file() {
                return None;
            }
            let name = entry.name().to_string();
            // Resource forks and dotfiles are not pages.
            if name.starts_with("__MACOSX") || name.rsplit('/').next()?.starts_with('.') {
                return None;
            }
            is_page_name(&name).then_some(name)
        })
        .collect();
    // Central-directory order is not guaranteed to be page order.
    names.sort_by(|a, b| natural_cmp(a, b));
    names
}

/// Open an archive, naming its format first if taiyaku cannot read it.
///
/// The single door in, so that listing an archive, reading one page of it and
/// reading all of it cannot differ in what they say about a container taiyaku
/// does not support. `archive_page_names` used to go straight to
/// `ZipArchive::new`, which reported a perfectly good RAR file as an unreadable
/// cbz — and `member_pages`, which lists rather than reads, is the path `align`
/// takes when it records a member.
fn open_archive(path: &Path) -> Result<zip::ZipArchive<File>, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;

    // Look at the bytes before handing them to the zip reader, so an archive in
    // another format is named rather than reported as a corrupt zip.
    let mut header = [0u8; 512];
    let read = file.read(&mut header).unwrap_or(0);
    if let Some(kind) = unsupported_container(&header[..read]) {
        return Err(format!(
            "{}: this is {kind}. taiyaku reads zip-based archives and directories of page images; convert it or unpack it first",
            path.display()
        ));
    }
    file.rewind()
        .map_err(|e| format!("{}: {e}", path.display()))?;

    zip::ZipArchive::new(file).map_err(|e| format!("{}: not a readable cbz: {e}", path.display()))
}

/// The names of an archive's page entries, in reading order.
fn archive_page_names(path: &Path) -> Result<Vec<String>, String> {
    Ok(page_names_from(&mut open_archive(path)?))
}

fn read_archive(path: &Path) -> Result<Vec<Page>, String> {
    let mut zip = open_archive(path)?;
    let names = page_names_from(&mut zip);

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
/// For an archive this reads only the central directory; for a directory only
/// the listing. That is what makes serving one page of a 195-page volume cost
/// one page rather than the volume.
///
/// An entry name that is not valid UTF-8 does not fail this: `zip`'s decode is
/// total, not fallible — invalid bytes come back as `U+FFFD`, or are mapped
/// through codepage 437 if the UTF-8 flag is unset — so a mis-encoded entry is
/// garbled, never refused, and the garbled name is what comes back in this
/// listing.
///
/// # Errors
///
/// For a directory, returns an error if `path`, or a directory nested under
/// it, cannot be listed. For an archive, returns an error if the file cannot
/// be opened, if its leading bytes identify a recognised-but-unsupported
/// container — 7-Zip, RAR, or tar, named as such rather than reported as a
/// corrupt cbz — or if it does not parse as a zip archive.
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
/// An entry name that is not valid UTF-8 does not fail this either: `zip`'s
/// decode is total, not fallible — invalid bytes come back as `U+FFFD`, or
/// are mapped through codepage 437 if the UTF-8 flag is unset — so it comes
/// back garbled rather than refused, at whatever `index` its position in the
/// natural-order sort gives it.
///
/// # Errors
///
/// Returns an error if `path` cannot be opened as a directory or an
/// archive — including a recognised-but-unsupported container such as
/// 7-Zip, RAR, or tar, named as such — if `index` is at or past the
/// member's page count (`"no page {index}"`), or if the located page's
/// bytes cannot be read back: a directory entry removed or made unreadable
/// since listing, or an archive entry whose compressed data does not
/// decode.
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
    let mut zip = open_archive(path)?;
    let names = page_names_from(&mut zip);
    let name = names
        .get(i)
        .cloned()
        .ok_or_else(|| format!("{}: no page {index}", path.display()))?;
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
/// An entry name that is not valid UTF-8 does not fail this: `zip`'s decode is
/// total, not fallible — invalid bytes come back as `U+FFFD`, or are mapped
/// through codepage 437 if the UTF-8 flag is unset — so a mis-encoded entry is
/// garbled, never refused, and if its extension still matches, it still
/// counts toward the total.
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
/// zip central directory, which records the uncompressed size. Uncompressed on
/// both sides deliberately: a directory of loose images and a `.cbz` of the
/// same scan then produce the same list, and seeing through packaging is the
/// whole job of the signature this feeds.
///
/// A listing, not a read — the property that lets an inventory run over a NAS
/// and decide what is worth copying.
///
/// An entry name that is not valid UTF-8 does not fail this: `zip`'s decode is
/// total, not fallible — invalid bytes come back as `U+FFFD`, or are mapped
/// through codepage 437 if the UTF-8 flag is unset — so a mis-encoded entry is
/// garbled, never refused, and its size is still listed at the position the
/// garbled name sorts to.
///
/// # Errors
///
/// For a directory, returns an error if it, or a directory nested under it,
/// cannot be listed, or if a page file's metadata cannot be read — removed
/// or made unreadable since listing. For an archive, returns an error if it
/// cannot be opened, if its leading bytes identify a recognised-but-
/// unsupported container — 7-Zip, RAR, or tar, named as such — if it does
/// not parse as a zip archive, or if a listed entry cannot be looked up
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
    let mut zip = open_archive(path)?;
    let names = page_names_from(&mut zip);
    names
        .iter()
        .map(|name| {
            zip.by_name(name)
                .map(|entry| entry.size())
                .map_err(|e| format!("{}: {name}: {e}", path.display()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A member that yields no pages is almost always a mistake, and saying
    /// nothing about it is the worst possible response. Pointing a directory of
    /// archival TIFFs at the aligner reported `A: 0 pages` and carried on
    /// cheerfully, because `.tif` was not in the recognised list — the files
    /// were there, readable, and silently ignored. The error has to name what
    /// it found instead.
    #[test]
    fn a_member_with_no_pages_says_what_it_found_instead() {
        let dir = std::env::temp_dir().join("taiyaku-test-nopages");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cover.psd"), b"not an image we read").unwrap();
        std::fs::write(dir.join("notes.txt"), b"hello").unwrap();

        let error = read_member(&dir).unwrap_err();

        let _ = std::fs::remove_dir_all(&dir);
        assert!(error.contains("no page images"), "unhelpful: {error}");
        assert!(
            error.contains("psd"),
            "should name what it skipped: {error}"
        );
    }

    /// The message someone actually sees. "invalid Zip archive: could not find
    /// EOCD" tells a person holding a perfectly good 7-Zip file nothing they
    /// can act on; naming the format tells them exactly what happened and that
    /// converting it will work.
    #[test]
    fn reading_a_seven_zip_archive_says_so() {
        let path = std::env::temp_dir().join("taiyaku-test-container.cb7");
        std::fs::write(&path, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04]).unwrap();

        let error = read_member(&path).unwrap_err();

        let _ = std::fs::remove_file(&path);
        assert!(error.contains("7-Zip"), "unhelpful error: {error}");
    }

    /// Extensions lie in both directions: a zip is often named `.cbr`, and a
    /// real 7-Zip archive is often named `.cbz`. Reporting "invalid Zip
    /// archive: could not find EOCD" to someone holding a perfectly good
    /// 7-Zip file tells them nothing they can act on, so the leading bytes
    /// decide what the error says.
    #[test]
    fn an_unreadable_container_is_named_by_its_leading_bytes() {
        let seven_zip = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04];
        let rar5 = b"Rar!\x1a\x07\x01\x00";
        let rar4 = b"Rar!\x1a\x07\x00";
        let mut tar = vec![0u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        let zip = [0x50, 0x4B, 0x03, 0x04, 0x14, 0x00];

        assert_eq!(unsupported_container(&seven_zip), Some("a 7-Zip archive"));
        assert_eq!(unsupported_container(rar5), Some("a RAR archive"));
        assert_eq!(unsupported_container(rar4), Some("a RAR archive"));
        assert_eq!(unsupported_container(&tar), Some("a tar archive"));
        // Zip is supported, so it is not something to complain about.
        assert_eq!(unsupported_container(&zip), None);
    }

    /// The signature sits at bytes 257..262, so 262 bytes is exactly enough to
    /// read it. The old guard asked for 263 and called a 262-byte buffer
    /// unrecognised; `get` asks for what it reads.
    ///
    /// No real tar is this short — one header block is 512 bytes, which is why
    /// this was never felt — but a bound that is one out is worth removing
    /// rather than pinning, and a slice guard written as arithmetic is a thing
    /// a test has to keep honest.
    #[test]
    fn the_tar_signature_is_read_from_the_shortest_buffer_that_holds_it() {
        let mut exact = vec![0u8; 262];
        exact[257..262].copy_from_slice(b"ustar");
        assert_eq!(unsupported_container(&exact), Some("a tar archive"));

        let mut one_short = vec![0u8; 261];
        one_short[256..261].copy_from_slice(b"ustar");
        assert_eq!(
            unsupported_container(&one_short),
            None,
            "too short to hold the signature at its offset, whatever it contains"
        );
    }

    /// A page prints as its name and size. Deriving `Debug` would dump several
    /// megabytes of JPEG into any failure that touches one, which is why it is
    /// hand-written — and a hand-written impl is code, so it can be wrong.
    #[test]
    fn a_page_prints_as_its_name_and_size_rather_than_its_bytes() {
        let page = Page {
            name: "p1.jpg".into(),
            bytes: vec![0xFF, 0xD8, 0xFF, 0xE0],
        };
        assert_eq!(format!("{page:?}"), "Page(p1.jpg, 4 bytes)");
    }

    /// Two independent reasons to skip an entry, and they must stay
    /// independent. A resource fork is `__MACOSX/…` whether or not its
    /// basename begins with a dot, and a dotfile is a dotfile wherever it
    /// sits — so the test needs one of each that the *other* rule would keep.
    ///
    /// `__MACOSX/p1.jpg` is the discriminating one: the usual resource fork is
    /// `__MACOSX/._p1.jpg`, which both rules reject, and an archive holding
    /// only those cannot tell the two conditions apart.
    #[test]
    fn a_resource_fork_and_a_dotfile_are_each_skipped_on_their_own_account() {
        let path = cbz(
            "taiyaku-test-skips.cbz",
            &[
                "__MACOSX/p1.jpg",
                ".hidden.jpg",
                "__MACOSX/._p2.jpg",
                "p1.jpg",
                "p2.jpg",
            ],
        );

        let pages = read_member(&path).unwrap();

        let _ = std::fs::remove_file(&path);
        let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["p1.jpg", "p2.jpg"],
            "a fork with an ordinary basename and a dotfile outside a fork are both skipped"
        );
    }

    /// The digest is what tells a reader the archive was replaced, so a stub
    /// returning a constant would report every member as unchanged forever.
    /// Pinned against a hash computed outside this crate rather than against
    /// `Sha256` called a second time here, which would agree with any bug both
    /// copies shared.
    ///
    /// A directory hashes its pages' bytes in order, not the directory itself,
    /// so the same bytes split across two files must not collide with them
    /// concatenated in one.
    #[test]
    fn a_members_digest_is_the_hash_of_its_content() {
        let file = std::env::temp_dir().join("taiyaku-test-digest.bin");
        std::fs::write(&file, b"taiyaku page").unwrap();
        let got = member_digest(&file).unwrap();
        let _ = std::fs::remove_file(&file);
        assert_eq!(
            got, "e7a8b30f8c6591c8e3341e1f6b0b4f6658cfe5b09b048dafabeaf9ef113622bd",
            "sha256 of the file's bytes"
        );

        let dir = std::env::temp_dir().join("taiyaku-test-digest-dir");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1.jpg"), b"taiyaku ").unwrap();
        std::fs::write(dir.join("2.jpg"), b"pageone").unwrap();
        let got = member_digest(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            got, "ce663f7c43b7eb8989b012dfa7c8183eb4e052b8ac0f178b87132354b02b8d72",
            "sha256 over the pages' bytes in reading order, joined"
        );
    }

    /// Write a cbz whose entries are stored in the order given.
    fn cbz(name: &str, entries: &[&str]) -> PathBuf {
        use std::io::Write;
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for entry in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(entry.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    /// Two rules a reader of the sidecar has to implement identically or every
    /// lookup is wrong, so they are guarantees of the format rather than
    /// details of this reader — see `docs/sidecar.md`, "How a member becomes
    /// pages".
    ///
    /// Central-directory order is whatever the packer happened to write: 32 of
    /// the corpus's 57 cbz archives store their entries in an order that is not
    /// the name order, so a reader iterating entries as stored gets a different
    /// page sequence for over half of them. And 12 of the 57 carry a
    /// `ComicInfo.xml`, which is not a page and must not shift the numbering.
    #[test]
    fn entries_are_pages_in_name_order_whatever_order_they_were_stored_in() {
        let path = cbz(
            "taiyaku-test-order.cbz",
            &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
        );

        let pages = read_member(&path).unwrap();

        let _ = std::fs::remove_file(&path);
        let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["p1.jpg", "p2.jpg", "p10.jpg"]);
    }

    /// Sorting on the basename rather than the whole path scatters a
    /// chapter-foldered edition beyond recovery, and the corpus holds the case:
    /// Go-Toubun's Full Colour edition nests pages under `Chapter 15 - …/` and
    /// carries a `ZBlank (1).jpg` in each folder. On the basename every
    /// chapter's blank sorts to the front and the chapters interleave; 2 of the
    /// 7 nested archives measured come out differently the two ways.
    #[test]
    fn nested_pages_sort_on_the_whole_path_not_the_basename() {
        let path = cbz(
            "taiyaku-test-nested.cbz",
            &[
                "Chapter 2/001.jpg",
                "Chapter 1/ZBlank (1).jpg",
                "Chapter 1/001.jpg",
            ],
        );

        let pages = read_member(&path).unwrap();

        let _ = std::fs::remove_file(&path);
        let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Chapter 1/001.jpg",
                "Chapter 1/ZBlank (1).jpg",
                "Chapter 2/001.jpg"
            ],
            "chapter 1 must finish before chapter 2 begins"
        );
    }

    /// What `serve` does on every page request. Reading the whole edition to
    /// answer "give me page 47" is hundreds of megabytes for one JPEG, so the
    /// index has to be addressable directly — and it has to land on the same
    /// page `read_member` would give at that position, or the server and the
    /// aligner disagree about what page 47 is.
    #[test]
    fn a_page_reads_by_index_in_the_order_read_member_gives() {
        let path = cbz(
            "taiyaku-test-byindex.cbz",
            &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
        );

        let names = member_page_names(&path).unwrap();
        let second = member_page(&path, 1).unwrap();
        let whole = read_member(&path).unwrap();

        let _ = std::fs::remove_file(&path);
        assert_eq!(names, ["p1.jpg", "p2.jpg", "p10.jpg"]);
        assert_eq!(second.name, "p2.jpg");
        assert_eq!(second.bytes, b"p2.jpg");
        assert_eq!(
            names,
            whole.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "names must agree with what read_member sequences"
        );
    }

    /// `docs/sidecar.md` rule 1: a sidecar's `members` array *is* the page
    /// order and must not be re-sorted, because its paths are relative to the
    /// sidecar and may point into unrelated trees. `read_edition` sorts — it is
    /// handed shell globs, which are not in reading order — so a caller holding
    /// a sidecar needs the other one, and the two must be demonstrably
    /// different or the second is just a renamed call.
    ///
    /// The seam check in `taiyaku-serve` made exactly this mistake: it rebuilt
    /// a sidecar's editions with `read_edition`, so on any work whose array
    /// order is not its natural order it would have reported a disagreement
    /// with neither side wrong.
    #[test]
    fn a_sidecars_member_order_is_the_page_order_and_is_not_re_sorted() {
        // Named so that natural order is the reverse of the order given.
        let b = cbz("taiyaku-test-given-b.cbz", &["b1.jpg"]);
        let a = cbz("taiyaku-test-given-a.cbz", &["a1.jpg"]);
        let as_given = vec![b.clone(), a.clone()];

        let in_order = read_members_in_order(&as_given).unwrap();
        let sorted = read_edition(&as_given).unwrap();

        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
        assert_eq!(
            in_order.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["b1.jpg", "a1.jpg"],
            "the array order is the page order"
        );
        assert_eq!(
            sorted.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["a1.jpg", "b1.jpg"],
            "read_edition sorts, which is right for a glob and wrong for a sidecar"
        );
    }

    /// `member_pages` is the path `align` takes to record a member's page
    /// count, and it lists rather than reads. Listing used to go straight to
    /// `ZipArchive::new`, so a RAR file — the single most common thing to find
    /// behind a `.cbr`, or behind a `.cbz` that lies — came back as "not a
    /// readable cbz" while reading the same file said "this is a RAR archive".
    /// Same file, same tool, two answers, and the useful one was on the path
    /// nobody took.
    #[test]
    fn counting_a_rar_archives_pages_names_the_format() {
        let path = std::env::temp_dir().join("taiyaku-test-count-container.cbz");
        std::fs::write(&path, b"Rar!\x1a\x07\x01\x00").unwrap();

        let counted = member_pages(&path).unwrap_err();
        let listed = member_page_names(&path).unwrap_err();
        let addressed = member_page(&path, 0).unwrap_err();

        let _ = std::fs::remove_file(&path);
        for error in [&counted, &listed, &addressed] {
            assert!(error.contains("RAR"), "unhelpful error: {error}");
        }
    }

    /// Every ordering test above builds a cbz; the directory branches of
    /// `member_page_names` and `member_page` had no direct coverage of their
    /// own, including `strip_prefix`, which turns an absolute path into a
    /// member-relative name and is new code rather than a move of Task 2's.
    /// Without this test, `strip_prefix` regressing to leave the temp-dir
    /// prefix in page names, or a nested directory sorting on the wrong key,
    /// would still pass every other test in this file.
    #[test]
    fn a_directory_page_reads_by_index_in_the_order_read_member_gives() {
        let dir = std::env::temp_dir().join("taiyaku-test-dir-byindex");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Chapter 1")).unwrap();
        std::fs::create_dir_all(dir.join("Chapter 2")).unwrap();
        std::fs::write(dir.join("Chapter 2/001.jpg"), b"Chapter 2/001.jpg").unwrap();
        std::fs::write(
            dir.join("Chapter 1/ZBlank (1).jpg"),
            b"Chapter 1/ZBlank (1).jpg",
        )
        .unwrap();
        std::fs::write(dir.join("Chapter 1/001.jpg"), b"Chapter 1/001.jpg").unwrap();

        let names = member_page_names(&dir).unwrap();
        let second = member_page(&dir, 1).unwrap();
        let whole = read_member(&dir).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            names,
            [
                "Chapter 1/001.jpg",
                "Chapter 1/ZBlank (1).jpg",
                "Chapter 2/001.jpg"
            ],
            "chapter 1 must finish before chapter 2 begins, and names must be member-relative"
        );
        assert_eq!(
            second.name, "Chapter 1/ZBlank (1).jpg",
            "a directory page's name must be member-relative too"
        );
        assert_eq!(second.bytes, b"Chapter 1/ZBlank (1).jpg");
        assert_eq!(
            names,
            whole.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "names must agree with what read_member sequences"
        );
    }

    /// The signature a duplicate set is settled by is a list of page sizes, so
    /// this has to agree with `read_edition` on *which* entries are pages, on
    /// what order they come in, and on how big each one is. Uncompressed, not
    /// as stored: an archive and a directory holding one scan must produce the
    /// same list, which is the whole point of signing pages rather than files.
    #[test]
    fn page_sizes_are_uncompressed_in_reading_order_and_skip_non_pages() {
        let path = cbz(
            "taiyaku-test-sizes.cbz",
            &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
        );

        let pages = read_member(&path).unwrap();
        let sizes = member_page_sizes(&path).unwrap();

        let _ = std::fs::remove_file(&path);
        let expected: Vec<u64> = pages.iter().map(|p| p.bytes.len() as u64).collect();
        assert_eq!(sizes, expected, "sizes must match the bytes actually read");
        assert_eq!(sizes.len(), 3, "ComicInfo.xml is not a page");
    }

    /// The property the duplicate signature rests on: repackaging a scan from
    /// loose images into a cbz must not change what it signs as.
    #[test]
    fn a_directory_and_an_archive_of_the_same_pages_size_identically() {
        let dir = std::env::temp_dir().join("taiyaku-test-sizes-dir");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for entry in ["p1.jpg", "p2.jpg", "p10.jpg"] {
            std::fs::write(dir.join(entry), entry.as_bytes()).unwrap();
        }
        // Stored in a different order, as a repacker would.
        let archive = cbz(
            "taiyaku-test-sizes-arc.cbz",
            &["p10.jpg", "p2.jpg", "p1.jpg"],
        );

        let from_dir = member_page_sizes(&dir).unwrap();
        let from_archive = member_page_sizes(&archive).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&archive);
        assert_eq!(from_dir, from_archive);
        assert_eq!(from_dir, vec![6, 6, 7], "p1.jpg, p2.jpg, p10.jpg by length");
    }

    /// Build a cbz and patch its first entry's central-directory
    /// uncompressed-size field to an outrageous lie — the same thing a
    /// hostile or corrupt `.cbz` would carry — rather than reimplementing
    /// the read path against a mock. Shared by both capacity-ceiling tests
    /// below, so `read_archive`'s and `member_page`'s identical clamp are
    /// proven against identically-constructed evidence.
    ///
    /// The central directory's uncompressed-size field sits at offset 24 of
    /// the 46-byte fixed header that follows the `PK\x01\x02` signature
    /// (APPNOTE.TXT §4.3.12). Patched, not the local file header, because
    /// `entry.size()` reads the central directory — patching only that one
    /// leaves the entry's actual stored bytes untouched, so a correct read
    /// is still possible despite the lie.
    fn lying_size_cbz(name: &str, entries: &[&str]) -> PathBuf {
        let path = cbz(name, entries);
        let mut raw = std::fs::read(&path).unwrap();
        let central_directory = raw
            .windows(4)
            .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
            .expect("a central directory file header");
        let size_field = central_directory + 24;
        // Same declared size as the probe docs/notes.md records: ~2 GiB for
        // an entry whose real content is a handful of bytes.
        raw[size_field..size_field + 4].copy_from_slice(&0x7FFF_FFF0u32.to_le_bytes());
        std::fs::write(&path, &raw).unwrap();
        path
    }

    /// A literal, not `MAX_ENTRY_CAPACITY_HINT` itself. An earlier version of
    /// this test asserted `capacity() <= MAX_ENTRY_CAPACITY_HINT`, which
    /// passes for *any* ceiling at or above the declared lie — raise the
    /// constant to 4 GiB and the assertion still goes green, having proved
    /// nothing about whether the clamp does anything useful. This bound sits
    /// comfortably above today's 32 MiB ceiling (allocator rounding, and
    /// room to retune the ceiling within its own order of magnitude without
    /// editing this test) and 30-fold below the ~2 GiB lie, so it still
    /// catches a regression that quietly enlarges the ceiling or drops the
    /// clamp outright.
    ///
    /// Verified against the failure it exists to catch: with `.min(...)`
    /// deleted from the call site, both tests below failed with
    /// `capacity() == 2147483632` (the declared lie itself), before the
    /// clamp was restored.
    const RESERVATION_CEILING: usize = 64 * 1024 * 1024;

    /// The path `read_archive` takes: every page in the member, collected at
    /// once. See `MAX_ENTRY_CAPACITY_HINT`'s doc comment for why that makes
    /// the per-entry ceiling matter in aggregate too.
    #[test]
    fn a_declared_size_far_past_reality_does_not_reserve_past_the_ceiling() {
        let path = lying_size_cbz("taiyaku-test-lying-size.cbz", &["p1.jpg"]);

        let pages = read_member(&path).unwrap();

        let _ = std::fs::remove_file(&path);
        assert_eq!(pages.len(), 1);
        assert_eq!(
            pages[0].bytes, b"p1.jpg",
            "the real bytes must still be read correctly despite the lie"
        );
        assert!(
            pages[0].bytes.capacity() < RESERVATION_CEILING,
            "a declared ~2 GiB must not reserve anywhere near it; got {} bytes reserved",
            pages[0].bytes.capacity()
        );
    }

    /// The path `taiyaku-serve` actually calls on every page request: one
    /// entry, addressed by index, never the whole archive. `read_archive`
    /// and `member_page` carry the identical
    /// `entry.size().min(MAX_ENTRY_CAPACITY_HINT)` clamp, but until this
    /// test only the `read_archive` side had a regression test — a change
    /// that broke the clamp on `member_page` alone (a copy-paste that
    /// dropped the `.min()`, say) would have shipped with the suite green,
    /// on the one path a server request actually reaches.
    #[test]
    fn a_lying_member_page_does_not_reserve_past_the_ceiling_either() {
        let path = lying_size_cbz("taiyaku-test-lying-size-member-page.cbz", &["p1.jpg"]);

        let page = member_page(&path, 0).unwrap();

        let _ = std::fs::remove_file(&path);
        assert_eq!(
            page.bytes, b"p1.jpg",
            "the real bytes must still be read correctly despite the lie"
        );
        assert!(
            page.bytes.capacity() < RESERVATION_CEILING,
            "a declared ~2 GiB must not reserve anywhere near it; got {} bytes reserved",
            page.bytes.capacity()
        );
    }
}
