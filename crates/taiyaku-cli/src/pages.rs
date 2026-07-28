//! Assembling an edition's flat page sequence from its members.

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use crate::naming::natural_cmp;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"];

pub struct Page {
    pub name: String,
    pub bytes: Vec<u8>,
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
    if path.is_dir() {
        read_directory(path)
    } else {
        read_archive(path)
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

fn read_archive(path: &Path) -> Result<Vec<Page>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
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
