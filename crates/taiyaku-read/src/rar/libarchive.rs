//! The RAR backend over the system's libarchive, through compress-tools.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use compress_tools::{ArchiveContents, ArchiveIterator, stat};

use super::{Backend, Entry, Visitor};

pub(super) mod headers;

use headers::Walk;

#[derive(Debug)]
pub(crate) struct Libarchive;

impl Backend for Libarchive {
    fn name(&self) -> &'static str {
        "libarchive"
    }

    fn entries(&self, path: &Path) -> Result<Vec<Entry>, String> {
        let mut file = File::open(path).map_err(|e| unreadable(path, &e))?;
        let cut = lacks_end_block(&mut file).map_err(|e| unreadable(path, &e))?;
        let mut archive = iterate(file, path)?;
        let mut entries = Vec::new();
        while let Some((name, stat)) = next_entry(&mut archive, path)? {
            entries.push(Entry {
                name,
                size: declared_size(&stat),
                readable: is_file(&stat),
            });
        }
        if cut {
            return Err(unreadable(
                path,
                &"it does not end with an end-of-archive block",
            ));
        }
        Ok(entries)
    }

    // Not `uncompress_archive_file`, which hides the declared size a cut-short entry falls below.
    fn read(&self, path: &Path, name: &str) -> Result<Vec<u8>, String> {
        let (mut archive, walk) = open(path)?;
        let mut position = 0;
        while let Some((found, stat)) = next_entry(&mut archive, path)? {
            let crc = expected_crc(&walk, position, &stat, path)?;
            position += 1;
            if found == name {
                let bytes = data(&mut archive, path, name, declared_size(&stat), crc)?;
                rest_agrees(&mut archive, &walk, position, path)?;
                return Ok(bytes);
            }
        }
        walk.finished(position).map_err(|e| unreadable(path, &e))?;
        Err(format!("{}: {name}: no such entry", path.display()))
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
        let (mut archive, walk) = open(path)?;
        let mut position = 0;
        while let Some((name, stat)) = next_entry(&mut archive, path)? {
            let crc = expected_crc(&walk, position, &stat, path)?;
            position += 1;
            if is_file(&stat) && wanted(&name) {
                let bytes = data(&mut archive, path, &name, declared_size(&stat), crc)?;
                f(&name, &bytes)?;
            }
        }
        walk.finished(position).map_err(|e| unreadable(path, &e))
    }
}

type Archive = ArchiveIterator<File>;

/// The archive and the walk of its headers, both from one open of the file,
/// so that a file renamed over `path` meanwhile cannot pair one archive's
/// checksums with another's data.
fn open(path: &Path) -> Result<(Archive, Walk), String> {
    let mut file = File::open(path).map_err(|e| unreadable(path, &e))?;
    let walk = headers::walk(&mut file).map_err(|e| unreadable(path, &e))?;
    Ok((iterate(file, path)?, walk))
}

/// The CRC32 the entry listed at `position` must meet, if any.
fn expected_crc(
    walk: &Walk,
    position: usize,
    stat: &stat,
    path: &Path,
) -> Result<Option<u32>, String> {
    let listed_file = is_file(stat).then(|| declared_size(stat));
    walk.expected(position, listed_file)
        .map_err(|e| unreadable(path, &e))
}

/// Holds the listing past the entry just read to the walk. In a solid archive
/// only the walk's own end is held to: listing on would decode every entry
/// after this one, which reading one page must not do.
fn rest_agrees(
    archive: &mut Archive,
    walk: &Walk,
    mut position: usize,
    path: &Path,
) -> Result<(), String> {
    if walk.solid {
        return walk.stopped.map_or(Ok(()), |e| Err(unreadable(path, &e)));
    }
    while let Some((_, stat)) = next_entry(archive, path)? {
        expected_crc(walk, position, &stat, path)?;
        position += 1;
    }
    walk.finished(position).map_err(|e| unreadable(path, &e))
}

fn iterate(file: File, path: &Path) -> Result<Archive, String> {
    ArchiveIterator::from_read(file).map_err(|e| unreadable(path, &e))
}

fn unreadable(path: &Path, cause: &dyn std::fmt::Display) -> String {
    format!("{}: not a readable RAR archive: {cause}", path.display())
}

/// The next entry's name and header. Its data is left unread, and reading the
/// header after it skips that data.
fn next_entry(archive: &mut Archive, path: &Path) -> Result<Option<(String, stat)>, String> {
    match archive.next_header() {
        Some(ArchiveContents::StartOfEntry(name, stat)) => Ok(Some((name, stat))),
        Some(ArchiveContents::Err(e)) => Err(unreadable(path, &e)),
        _ => Ok(None),
    }
}

/// The current entry's bytes, refused unless they come to `size`, there are
/// some and they meet `crc`: an entry cut short by a truncated archive ends
/// early with no error, and libarchive reads a stored entry's data unchecked.
fn data(
    archive: &mut Archive,
    path: &Path,
    name: &str,
    size: u64,
    crc: Option<u32>,
) -> Result<Vec<u8>, String> {
    // Grown with the data, never sized from a header: MAX_ENTRY_CAPACITY_HINT has nothing to clamp.
    let mut bytes = Vec::new();
    for item in archive.by_ref() {
        match item {
            ArchiveContents::DataChunk(chunk) => bytes.extend_from_slice(&chunk),
            ArchiveContents::Err(e) => return Err(format!("{}: {name}: {e}", path.display())),
            ArchiveContents::EndOfEntry | ArchiveContents::StartOfEntry(..) => break,
        }
    }
    if bytes.len() as u64 != size {
        return Err(format!(
            "{}: {name}: {} bytes where its header gives {size}",
            path.display(),
            bytes.len()
        ));
    }
    // A file reference or hard link lists as a regular file of size 0 and
    // yields no bytes, as an empty file does: all three are refused, so a
    // reference is never an empty page (O381).
    if size == 0 {
        return Err(format!("{}: {name}: no data", path.display()));
    }
    if crc.is_some_and(|crc| crc != crc32fast::hash(&bytes)) {
        return Err(format!(
            "{}: {name}: its data does not match the CRC32 in its header",
            path.display()
        ));
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
fn lacks_end_block(file: &mut File) -> std::io::Result<bool> {
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
