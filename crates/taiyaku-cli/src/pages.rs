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

    let mut pages = Vec::new();
    for member in &ordered {
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

fn read_directory(root: &Path) -> Result<Vec<Page>, String> {
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

    found
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

fn read_archive(path: &Path) -> Result<Vec<Page>, String> {
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

    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| format!("{}: not a readable cbz: {e}", path.display()))?;

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

/// How many pages a member holds, without keeping them in memory.
pub fn member_pages(path: &Path) -> Result<u32, String> {
    Ok(read_member(path)?.len() as u32)
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
}
