//! The RAR backend over the system's libarchive, through compress-tools.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use compress_tools::{ArchiveContents, ArchiveIterator, stat};

use super::{Backend, Entry, Visitor};

pub(super) mod headers;

use headers::{Header, Walk};

#[derive(Debug)]
pub(crate) struct Libarchive;

impl Backend for Libarchive {
    fn name(&self) -> &'static str {
        "libarchive"
    }

    // The version pkg-config gave the build (`build.rs`, ruling P2-R5):
    // compress-tools exposes none at run time, and this workspace forbids the
    // unsafe code that would ask libarchive itself.
    fn signature(&self) -> String {
        format!("{}-{}", self.name(), env!("TAIYAKU_LIBARCHIVE_VERSION"))
    }

    fn entries(&self, path: &Path) -> Result<Vec<Entry>, String> {
        let file = File::open(path).map_err(|e| unreadable(path, &e))?;
        Ok(list(&file, path)?.1)
    }

    // Not `uncompress_archive_file`, which hides the declared size a cut-short entry falls below.
    fn read(&self, path: &Path, name: &str) -> Result<Vec<u8>, String> {
        let (file, walk) = open(path)?;
        let mut archive = iterate(&file, path)?;
        let mut position = 0;
        while let Some((found, stat)) = next_entry(&mut archive, path)? {
            let header = listed(&walk, position, &stat, path)?;
            position += 1;
            if found != name || !is_file(&stat) {
                continue;
            }
            let Some(target) = header.target else {
                let bytes = data(&mut archive, path, name, header)?;
                rest_agrees(&mut archive, &walk, position, path)?;
                return Ok(bytes);
            };
            rest_agrees(&mut archive, &walk, position, path)?;
            // The target is behind this entry, so a second pass over the same
            // open file reads it. Two passes at once would share its offset.
            drop(archive);
            return read_at(&file, &walk, target, path, name);
        }
        walk.finished(position).map_err(|e| unreadable(path, &e))?;
        Err(format!("{}: {name}: no such entry", path.display()))
    }

    fn read_listed(
        &self,
        path: &Path,
        pick: &mut dyn FnMut(&[Entry]) -> Result<usize, String>,
    ) -> Result<Vec<u8>, String> {
        let file = File::open(path).map_err(|e| unreadable(path, &e))?;
        let (walk, entries) = list(&file, path)?;
        let position = pick(&entries)?;
        let Some(entry) = entries.get(position) else {
            return Err(format!("{}: no entry at {position}", path.display()));
        };
        if !entry.readable {
            return Err(format!(
                "{}: {}: not a regular file",
                path.display(),
                entry.name
            ));
        }
        read_at(
            &file,
            &walk,
            entry.target.unwrap_or(position),
            path,
            &entry.name,
        )
    }

    // Not the crate's filter, which must be 'static where `wanted` is borrowed:
    // an entry passed over here is skipped by the next header read, as a
    // filtered one is.
    fn visit(
        &self,
        path: &Path,
        wanted: &dyn Fn(&str) -> bool,
        f: &mut Visitor<'_>,
    ) -> Result<(), String> {
        let (file, walk) = open(path)?;
        let mut archive = iterate(&file, path)?;
        // A target's bytes are kept from its own position to its last
        // reference's, so that one pass hands every reference over: a target
        // always comes first.
        let mut last_reference = HashMap::new();
        for (position, header) in walk.headers.iter().enumerate() {
            if let Some(target) = header.target {
                last_reference.insert(target, position);
            }
        }
        let mut kept: HashMap<usize, Vec<u8>> = HashMap::new();
        let mut position = 0;
        while let Some((name, stat)) = next_entry(&mut archive, path)? {
            let header = listed(&walk, position, &stat, path)?;
            let at = position;
            position += 1;
            if !is_file(&stat) {
                continue;
            }
            let wants = wanted(&name);
            if let Some(target) = header.target {
                if wants {
                    let bytes = kept.get(&target).ok_or_else(|| {
                        format!("{}: {name}: its target was not read", path.display())
                    })?;
                    f(&name, bytes)?;
                }
                if last_reference.get(&target) == Some(&at) {
                    kept.remove(&target);
                }
                continue;
            }
            let keep = last_reference.contains_key(&at);
            if wants || keep {
                let bytes = data(&mut archive, path, &name, header)?;
                if wants {
                    f(&name, &bytes)?;
                }
                if keep {
                    kept.insert(at, bytes);
                }
            }
        }
        walk.finished(position).map_err(|e| unreadable(path, &e))
    }
}

/// libarchive reading the file a walk was taken of. Over a shared borrow, so
/// that one open can be read through more than once, one pass at a time. Each
/// pass starts at the file's start because compress-tools'
/// `ArchiveIterator::new` seeks its source to 0 before libarchive reads it.
/// A pass that began where the last one stopped would find no archive there,
/// or other entries than the walk's at their positions, and be refused, since
/// each entry a pass meets is held to the walk's header at its position.
type Archive<'f> = ArchiveIterator<&'f File>;

/// The file and the walk of its headers, so that one open serves the walk and
/// every pass of libarchive: a file renamed over `path` meanwhile cannot pair
/// one archive's checksums with another's data.
fn open(path: &Path) -> Result<(File, Walk), String> {
    let file = File::open(path).map_err(|e| unreadable(path, &e))?;
    let walk = headers::walk(&mut &file).map_err(|e| unreadable(path, &e))?;
    Ok((file, walk))
}

/// Every entry of the archive open as `file`, each met with its header as a
/// read meets it, so that a listing refuses what every read of the archive
/// would; and the walk it was met with.
fn list(file: &File, path: &Path) -> Result<(Walk, Vec<Entry>), String> {
    let cut = lacks_end_block(&mut &*file).map_err(|e| unreadable(path, &e))?;
    let walk = headers::walk(&mut &*file).map_err(|e| unreadable(path, &e))?;
    let mut archive = iterate(file, path)?;
    let mut entries = Vec::new();
    while let Some((name, stat)) = next_entry(&mut archive, path)? {
        let header = listed(&walk, entries.len(), &stat, path)?;
        let bytes = walk.bytes_of(header);
        entries.push(Entry {
            name,
            size: bytes.size,
            readable: is_file(&stat),
            crc: bytes.crc,
            target: header.target,
        });
    }
    if cut {
        return Err(unreadable(
            path,
            &"it does not end with an end-of-archive block",
        ));
    }
    walk.finished(entries.len())
        .map_err(|e| unreadable(path, &e))?;
    Ok((walk, entries))
}

/// The bytes of the entry at `source`, which holds bytes of its own, from a
/// new pass over `file`, refused under `name`: the entry's, or that of a
/// reference whose target it is.
fn read_at(
    file: &File,
    walk: &Walk,
    source: usize,
    path: &Path,
    name: &str,
) -> Result<Vec<u8>, String> {
    let mut archive = iterate(file, path)?;
    let mut position = 0;
    while let Some((_, stat)) = next_entry(&mut archive, path)? {
        let header = listed(walk, position, &stat, path)?;
        if position == source {
            return data(&mut archive, path, name, header);
        }
        position += 1;
    }
    Err(unreadable(path, &walk.differs()))
}

/// The walk's header for the entry listed at `position`, whose CRC32, if any,
/// that entry's bytes must meet.
fn listed<'a>(
    walk: &'a Walk,
    position: usize,
    stat: &stat,
    path: &Path,
) -> Result<&'a Header, String> {
    let listed_file = is_file(stat).then(|| declared_size(stat));
    walk.listed(position, listed_file)
        .map_err(|e| unreadable(path, &e))
}

/// Holds the listing past the entry just read to the walk. In a solid archive
/// only the walk's own end is held to: listing on would decode every entry
/// after this one, which reading one page must not do.
fn rest_agrees(
    archive: &mut Archive<'_>,
    walk: &Walk,
    mut position: usize,
    path: &Path,
) -> Result<(), String> {
    if walk.solid {
        return walk.stopped.map_or(Ok(()), |e| Err(unreadable(path, &e)));
    }
    while let Some((_, stat)) = next_entry(archive, path)? {
        listed(walk, position, &stat, path)?;
        position += 1;
    }
    walk.finished(position).map_err(|e| unreadable(path, &e))
}

fn iterate<'f>(file: &'f File, path: &Path) -> Result<Archive<'f>, String> {
    ArchiveIterator::from_read(file).map_err(|e| unreadable(path, &e))
}

fn unreadable(path: &Path, cause: &dyn std::fmt::Display) -> String {
    format!("{}: not a readable RAR archive: {cause}", path.display())
}

/// The next entry's name and header. Its data is left unread, and reading the
/// header after it skips that data.
fn next_entry(archive: &mut Archive<'_>, path: &Path) -> Result<Option<(String, stat)>, String> {
    match archive.next_header() {
        Some(ArchiveContents::StartOfEntry(name, stat)) => Ok(Some((name, stat))),
        Some(ArchiveContents::Err(e)) => Err(unreadable(path, &e)),
        _ => Ok(None),
    }
}

/// The bytes of the entry the archive is at, which `header` declares, refused
/// unless they come to its size and meet its CRC32 where it gives one: an
/// entry cut short by a truncated archive ends early with no error, and
/// libarchive reads a stored entry's data unchecked. The walk's size, not
/// libarchive's, since libarchive gives a redirected entry none.
fn data(
    archive: &mut Archive<'_>,
    path: &Path,
    name: &str,
    header: &Header,
) -> Result<Vec<u8>, String> {
    let size = header.size;
    // An empty file is not asked for: libarchive fails to read one in a solid
    // RAR5, and passing over it reads on to the next entry whole.
    let bytes = if size == 0 {
        Vec::new()
    } else {
        drain(archive, path, name)?
    };
    if bytes.len() as u64 != size {
        return Err(format!(
            "{}: {name}: {} bytes where its header gives {size}",
            path.display(),
            bytes.len()
        ));
    }
    if header.crc.is_some_and(|crc| crc != crc32fast::hash(&bytes)) {
        return Err(format!(
            "{}: {name}: its data does not match the CRC32 in its header",
            path.display()
        ));
    }
    Ok(bytes)
}

/// What libarchive gives of the current entry's data.
fn drain(archive: &mut Archive<'_>, path: &Path, name: &str) -> Result<Vec<u8>, String> {
    // Grown with the data, never sized from a header: MAX_ENTRY_CAPACITY_HINT has nothing to clamp.
    let mut bytes = Vec::new();
    for item in archive.by_ref() {
        match item {
            ArchiveContents::DataChunk(chunk) => bytes.extend_from_slice(&chunk),
            ArchiveContents::Err(e) => return Err(format!("{}: {name}: {e}", path.display())),
            ArchiveContents::EndOfEntry | ArchiveContents::StartOfEntry(..) => break,
        }
    }
    Ok(bytes)
}

const RAR5_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x01\x00";
const END_OF_ARCHIVE: u64 = 5;
/// Room for the end block and the zero bytes rar pads a non-last volume with.
const TAIL: u64 = 4096;

/// Whether `file` is a RAR5 that does not end with its end-of-archive block,
/// read from its tail; `file` is left at its start. Checked here because
/// libarchive reads a block header cut short as the end of the archive, so a
/// cut file lists short with no error. The rules are RARLAB's RAR5 format
/// description (rarlab.com/technote.htm), with libarchive's `rar5.c` as the
/// reference. A RAR4 need not end with an end block, so it is not checked.
fn lacks_end_block<R: Read + Seek>(file: &mut R) -> std::io::Result<bool> {
    let mut signature = Vec::new();
    file.by_ref().take(8).read_to_end(&mut signature)?;
    let lacks = signature == RAR5_SIGNATURE && {
        let start = file.seek(SeekFrom::End(0))?.saturating_sub(TAIL);
        file.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::new();
        file.by_ref().take(TAIL).read_to_end(&mut tail)?;
        !ends_with_end_block(&tail)
    };
    file.rewind()?;
    Ok(lacks)
}

/// Whether `tail`, less the zero bytes it ends with, ends with an
/// end-of-archive block: the CRC32 of the rest, the size of the rest as a vint
/// of at most 3 bytes, then the block type.
pub(super) fn ends_with_end_block(tail: &[u8]) -> bool {
    let unpadded = tail.len() - tail.iter().rev().take_while(|&&byte| byte == 0).count();
    (0..unpadded).any(|start| {
        let Some((crc, rest)) = tail[start..].split_first_chunk::<4>() else {
            return false;
        };
        let Some((size, size_len)) = vint(rest, 3) else {
            return false;
        };
        let block = usize::try_from(size)
            .ok()
            .and_then(|size| rest.get(..size_len + size));
        block.is_some_and(|block| {
            start + 4 + block.len() >= unpadded
                && vint(&block[size_len..], 10).is_some_and(|(kind, _)| kind == END_OF_ARCHIVE)
                && crc32fast::hash(block) == u32::from_le_bytes(*crc)
        })
    })
}

/// The vint `bytes` starts with, if it ends within `max` bytes: its value and
/// length. Each byte gives 7 bits, lowest first, and a set high bit means more.
fn vint(bytes: &[u8], max: usize) -> Option<(u64, usize)> {
    let mut value = 0;
    for (i, &byte) in bytes.iter().take(max).enumerate() {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

/// The mode's file-type bits name a regular file.
fn is_file(stat: &stat) -> bool {
    const TYPE: u32 = 0o170_000;
    const REGULAR: u32 = 0o100_000;
    stat.st_mode & TYPE == REGULAR
}

/// Only a corrupt header gives a negative size; as 0, it fails the size check
/// for any data.
fn declared_size(stat: &stat) -> u64 {
    u64::try_from(stat.st_size).unwrap_or(0)
}
