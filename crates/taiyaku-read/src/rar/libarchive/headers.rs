//! What a RAR's own block headers declare of each entry, read from the same
//! open file libarchive then reads. libarchive never compares a stored entry's
//! data with its header's CRC32, so the check needs the checksum from here.
//!
//! A block's layout is RARLAB's (rarlab.com/technote.htm), and where the walk
//! stops, skips or calls an entry a regular file it follows libarchive's
//! `rar.c` and `rar5.c`, since positions must line up with libarchive's listing.

use std::io::{Read, Seek, SeekFrom};

use super::{RAR5_SIGNATURE, vint};

const RAR4_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";

/// The most blocks one walk reads, so that a crafted file of tiny blocks costs
/// a fixed amount of memory and time. Far above any book's page count.
const MAX_BLOCKS: usize = 1 << 20;
/// The largest RAR5 block header libarchive accepts.
const MAX_RAR5_HEADER: u64 = 2 * 1024 * 1024;
/// Read at each block's start: nearly every header fits, in one read.
const FIRST_READ: u64 = 512;
/// The most bytes of a vint: 7 bits each, which covers 64 bits.
const MAX_VINT: usize = 10;

const RUNS_PAST_END: &str = "a block runs past the end of the file";
const BAD_HEADER: &str = "a block header cannot be read";
const UNKNOWN_BLOCK: &str = "a block of an unknown type";
const TOO_MANY_BLOCKS: &str = "it holds too many blocks";
const NO_SIGNATURE: &str = "it does not start with a RAR signature";
const LISTING_DIFFERS: &str = "its block headers and its listing differ";

/// What the headers declare of one entry, at its position in the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::rar) struct Header {
    /// The uncompressed size.
    pub(in crate::rar) size: u64,
    /// The CRC32 of the entry's bytes, where one applies to them. An encrypted
    /// entry's is not of its plain bytes, a split entry's is not of the whole
    /// file, and a reference, link or directory has no bytes of its own.
    pub(in crate::rar) crc: Option<u32>,
}

/// Every entry's header, in archive order, and how the walk ended.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::rar) struct Walk {
    pub(in crate::rar) headers: Vec<Header>,
    /// Why the walk did not reach the end of the archive.
    pub(in crate::rar) stopped: Option<&'static str>,
    /// Listing past an entry of a solid archive decodes the entries it passes.
    pub(in crate::rar) solid: bool,
}

impl Walk {
    /// The checksum for the entry at `position` of libarchive's listing, given
    /// the size libarchive lists it with if it is a regular file. An entry with
    /// a checksum to meet must be that same file in the listing.
    pub(in crate::rar) fn expected(
        &self,
        position: usize,
        listed_file: Option<u64>,
    ) -> Result<Option<u32>, &'static str> {
        let Some(header) = self.headers.get(position) else {
            return Err(self.stopped.unwrap_or(LISTING_DIFFERS));
        };
        if header.crc.is_some() && listed_file != Some(header.size) {
            return Err(LISTING_DIFFERS);
        }
        Ok(header.crc)
    }

    /// Whether a listing of `listed` entries saw every entry the walk did.
    pub(in crate::rar) fn finished(&self, listed: usize) -> Result<(), &'static str> {
        match self.stopped {
            Some(cause) => Err(cause),
            None if listed == self.headers.len() => Ok(()),
            None => Err(LISTING_DIFFERS),
        }
    }
}

/// Walks the block headers of `file` and leaves it at its start. Only an I/O
/// failure is an error: a file that cannot be walked says why in `stopped`.
pub(in crate::rar) fn walk<R: Read + Seek>(file: &mut R) -> std::io::Result<Walk> {
    walk_at_most(file, MAX_BLOCKS)
}

/// [`walk`], stopping after `max_blocks` blocks.
pub(in crate::rar) fn walk_at_most<R: Read + Seek>(
    file: &mut R,
    max_blocks: usize,
) -> std::io::Result<Walk> {
    let len = file.seek(SeekFrom::End(0))?;
    let start = read_at(file, 0, 8)?;
    let rar5 = start.starts_with(RAR5_SIGNATURE);
    let mut walk = Walk::default();
    if !rar5 && !start.starts_with(RAR4_SIGNATURE) {
        walk.stopped = Some(NO_SIGNATURE);
        file.rewind()?;
        return Ok(walk);
    }
    let mut at = if rar5 { 8 } else { 7 };
    let mut blocks = 0;
    walk.stopped = loop {
        // A RAR4 need not carry an end block, so the file's end can be the
        // archive's.
        if at == len {
            break None;
        }
        if blocks == max_blocks {
            break Some(TOO_MANY_BLOCKS);
        }
        blocks += 1;
        let step = if rar5 {
            rar5_block(file, at, &mut walk)?
        } else {
            rar4_block(file, at, &mut walk)?
        };
        match step {
            Step::Next(next) if next <= len => at = next,
            Step::Next(_) => break Some(RUNS_PAST_END),
            Step::End => break None,
            Step::Stop(cause) => break Some(cause),
        }
    };
    file.rewind()?;
    Ok(walk)
}

/// What one block means for the walk.
enum Step {
    /// The next block starts at this offset.
    Next(u64),
    /// The archive ends here.
    End,
    Stop(&'static str),
}

/// Up to `len` bytes at `at`, fewer at the end of the file. Grown with what
/// arrives, never sized from a header.
fn read_at<R: Read + Seek>(file: &mut R, at: u64, len: u64) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(at))?;
    let mut bytes = Vec::new();
    file.by_ref().take(len).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The fields of a RAR5 header, read in order.
struct Fields<'a>(&'a [u8]);

impl<'a> Fields<'a> {
    fn vint(&mut self) -> Option<u64> {
        let (value, len) = vint(self.0, MAX_VINT)?;
        self.0 = self.0.get(len..)?;
        Some(value)
    }

    fn u32(&mut self) -> Option<u32> {
        let (bytes, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*bytes))
    }

    fn skip(&mut self, len: u64) -> Option<()> {
        self.0 = self.0.get(usize::try_from(len).ok()?..)?;
        Some(())
    }

    /// The last `len` bytes left, where a header keeps its extra area.
    fn last(&self, len: u64) -> Option<&'a [u8]> {
        let start = self.0.len().checked_sub(usize::try_from(len).ok()?)?;
        self.0.get(start..)
    }
}

fn rar5_block<R: Read + Seek>(file: &mut R, at: u64, walk: &mut Walk) -> std::io::Result<Step> {
    let first = read_at(file, at, FIRST_READ)?;
    // The header's own CRC is libarchive's to check when it reaches the block.
    let Some((size, size_len)) = first.get(4..).and_then(|rest| vint(rest, MAX_VINT)) else {
        return Ok(Step::Stop(BAD_HEADER));
    };
    let header_len = size.saturating_add(size_len as u64);
    if size == 0 || header_len > MAX_RAR5_HEADER {
        return Ok(Step::Stop(BAD_HEADER));
    }
    let total = 4 + header_len;
    let block = if first.len() as u64 >= total {
        first
    } else {
        read_at(file, at, total)?
    };
    let body = usize::try_from(total)
        .ok()
        .and_then(|total| block.get(4 + size_len..total));
    let step = body.and_then(|body| rar5_fields(body, at.saturating_add(total), walk));
    Ok(step.unwrap_or(Step::Stop(BAD_HEADER)))
}

/// The block whose header fields are `body` and whose data starts at
/// `data_start`. `None` where the fields do not parse.
fn rar5_fields(body: &[u8], data_start: u64, walk: &mut Walk) -> Option<Step> {
    const MAIN: u64 = 1;
    const FILE: u64 = 2;
    const SERVICE: u64 = 3;
    const ENCRYPTION: u64 = 4;
    const END: u64 = 5;
    const MARKER: u64 = 0;
    const HAS_EXTRA: u64 = 0x01;
    const HAS_DATA: u64 = 0x02;
    const SKIP_IF_UNKNOWN: u64 = 0x04;
    const CONTINUED_FROM_LAST_VOLUME: u64 = 0x08;
    const SOLID_ARCHIVE: u64 = 0x04;

    let mut fields = Fields(body);
    let kind = fields.vint()?;
    let flags = fields.vint()?;
    let extra = if flags & HAS_EXTRA == 0 {
        0
    } else {
        fields.vint()?
    };
    let data = if flags & HAS_DATA == 0 {
        0
    } else {
        fields.vint()?
    };
    let next = Step::Next(data_start.saturating_add(data));
    match kind {
        MAIN => {
            walk.solid |= fields.vint()? & SOLID_ARCHIVE != 0;
            Some(next)
        }
        FILE => {
            let (header, solid) = rar5_file(&mut fields, flags, extra)?;
            // libarchive lists a split file where it starts, so the block that
            // carries on from the previous volume takes no position.
            if flags & CONTINUED_FROM_LAST_VOLUME == 0 {
                walk.headers.push(header);
            }
            walk.solid |= solid;
            Some(next)
        }
        SERVICE => Some(next),
        ENCRYPTION => Some(Step::Stop(BAD_HEADER)),
        END | MARKER => Some(Step::End),
        _ if flags & SKIP_IF_UNKNOWN != 0 => Some(next),
        _ => Some(Step::Stop(UNKNOWN_BLOCK)),
    }
}

/// A file block's header and whether its data continues a solid stream.
fn rar5_file(fields: &mut Fields<'_>, block_flags: u64, extra: u64) -> Option<(Header, bool)> {
    const CONTINUES_IN_NEXT_VOLUME: u64 = 0x10;
    const HAS_MTIME: u64 = 0x02;
    const HAS_CRC: u64 = 0x04;
    const SOLID: u64 = 0x40;
    const WINDOWS: u64 = 0;
    const UNIX: u64 = 1;

    let file_flags = fields.vint()?;
    let size = fields.vint()?;
    let attributes = fields.vint()?;
    if file_flags & HAS_MTIME != 0 {
        fields.u32()?;
    }
    let crc = if file_flags & HAS_CRC == 0 {
        None
    } else {
        Some(fields.u32()?)
    };
    let compression = fields.vint()?;
    let host = fields.vint()?;
    let name_len = fields.vint()?;
    fields.skip(name_len)?;
    let own_bytes = rar5_extra_leaves_bytes_own(fields.last(extra)?)?;
    let regular = match host {
        WINDOWS => attributes & 0x10 == 0,
        UNIX => is_regular_mode(attributes),
        _ => false,
    };
    let checked = regular && own_bytes && block_flags & CONTINUES_IN_NEXT_VOLUME == 0;
    let header = Header {
        size,
        crc: crc.filter(|_| checked),
    };
    Some((header, compression & SOLID != 0))
}

/// Whether no record of a file's extra area encrypts its data or redirects it
/// to another entry. `None` where a record does not fit the area.
fn rar5_extra_leaves_bytes_own(mut area: &[u8]) -> Option<bool> {
    const ENCRYPTION: u64 = 1;
    const REDIRECTION: u64 = 5;

    let mut own = true;
    while !area.is_empty() {
        let (size, size_len) = vint(area, MAX_VINT)?;
        let rest = area.get(size_len..)?;
        let record = rest.get(..usize::try_from(size).ok()?)?;
        let (kind, _) = vint(record, MAX_VINT)?;
        own &= kind != ENCRYPTION && kind != REDIRECTION;
        area = rest.get(record.len()..)?;
    }
    Some(own)
}

fn rar4_block<R: Read + Seek>(file: &mut R, at: u64, walk: &mut Walk) -> std::io::Result<Step> {
    let block = read_at(file, at, FIRST_READ)?;
    Ok(rar4_fields(&block, at, walk).unwrap_or(Step::Stop(BAD_HEADER)))
}

/// The block at `at` that starts with `block`. `None` where its fixed fields
/// are cut off or too short for its type.
fn rar4_fields(block: &[u8], at: u64, walk: &mut Walk) -> Option<Step> {
    const MARK: u8 = 0x72;
    const MAIN: u8 = 0x73;
    const FILE: u8 = 0x74;
    const COMMENT: u8 = 0x75;
    const SIGN: u8 = 0x79;
    const SERVICE: u8 = 0x7a;
    const END: u8 = 0x7b;
    const HAS_ADD_SIZE: u16 = 0x8000;
    const LARGE: u16 = 0x0100;
    /// The smallest main header libarchive accepts.
    const MAIN_SIZE: u64 = 13;
    /// The base block and the file fields every file block carries.
    const FILE_SIZE: u64 = 32;
    /// With the high halves of the two sizes.
    const LARGE_FILE_SIZE: u64 = 40;

    let flags = u16_at(block, 3)?;
    let size = u64::from(u16_at(block, 5)?);
    let after = |data: u64| Step::Next(at.saturating_add(size).saturating_add(data));
    match *block.get(2)? {
        MARK => (block.get(..7)? == RAR4_SIGNATURE).then_some(Step::Next(at.saturating_add(7))),
        MAIN => (size >= MAIN_SIZE).then(|| after(0)),
        kind @ (FILE | SERVICE) => {
            let large = flags & LARGE != 0;
            if size < if large { LARGE_FILE_SIZE } else { FILE_SIZE } {
                return None;
            }
            let high = |offset| {
                if large {
                    u32_at(block, offset).map(u64::from)
                } else {
                    Some(0)
                }
            };
            let packed = u64::from(u32_at(block, 7)?) | high(32)? << 32;
            if kind == FILE {
                let unpacked = u64::from(u32_at(block, 11)?) | high(36)? << 32;
                walk.headers.push(rar4_file(block, flags, unpacked)?);
            }
            Some(after(packed))
        }
        COMMENT..=SIGN if flags & HAS_ADD_SIZE == 0 => (size >= 7).then(|| after(0)),
        COMMENT..=SIGN if size >= 11 => Some(after(u64::from(u32_at(block, 7)?))),
        END => Some(Step::End),
        COMMENT..=SIGN => None,
        _ => Some(Step::Stop(UNKNOWN_BLOCK)),
    }
}

/// A file block's header, whose unpacked size is `size`.
fn rar4_file(block: &[u8], flags: u16, size: u64) -> Option<Header> {
    const SPLIT: u16 = 0x01 | 0x02;
    const ENCRYPTED: u16 = 0x04;

    let attributes = u32_at(block, 28)?;
    // libarchive takes the file type from the attributes by the host's rules.
    let regular = match *block.get(15)? {
        0..=2 => attributes & 0x10 == 0,
        3..=5 => is_regular_mode(u64::from(attributes)),
        _ => false,
    };
    let checked = regular && flags & (SPLIT | ENCRYPTED) == 0;
    let crc = u32_at(block, 16)?;
    Some(Header {
        size,
        crc: checked.then_some(crc),
    })
}

/// Unix mode bits that name a regular file.
fn is_regular_mode(mode: u64) -> bool {
    mode & 0o170_000 == 0o100_000
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
