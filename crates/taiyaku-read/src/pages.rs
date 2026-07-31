//! Assembling an edition's flat page sequence from its members.

use std::{
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use crate::naming::natural_cmp;

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "avif", "tif", "tiff",
];

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

fn is_image(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Flatten an edition into one page sequence.
///
/// An edition is a list of members, not a single file: one volume of the raw
/// corresponds to five or six chapter archives of a fan translation. Both sides
/// flatten before anything else happens, so volume-against-chapters stops being
/// a special case. Members are ordered naturally rather than as given, because a
/// shell glob sorts `c005.5` ahead of `c005`.
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
    match extensions.is_empty() {
        true => String::new(),
        false => format!(
            ". It holds {}, none of which taiyaku reads as a page",
            extensions
                .iter()
                .map(|e| format!(".{e}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
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
            } else if is_image(&base) {
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
    } else if bytes.len() > 262 && &bytes[257..262] == b"ustar" {
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
            is_image(&name).then_some(name)
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
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
            Ok(Page { name, bytes })
        })
        .collect()
}

/// The names of a member's pages, in reading order, without reading any bytes.
///
/// For an archive this reads only the central directory; for a directory only
/// the listing. That is what makes serving one page of a 195-page volume cost
/// one page rather than the volume.
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
    let mut bytes = Vec::with_capacity(entry.size() as usize);
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
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// How many pages a member holds, without reading any of them.
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
    Ok(names.len() as u32)
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
}
