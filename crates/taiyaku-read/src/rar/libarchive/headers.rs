//! What a RAR's own block headers declare of each entry, read from the same
//! open file libarchive then reads. libarchive never compares a stored entry's
//! data with its header's CRC32 or BLAKE2 hash, so the check needs the
//! checksum from here.
//! The walk also stops at what libarchive lists but cannot read whole, so that
//! the archive is refused where it is listed.
//!
//! A block's layout is RARLAB's (rarlab.com/technote.htm), and where the walk
//! stops, skips or calls an entry a regular file it follows libarchive's
//! `rar.c` and `rar5.c`, since positions must line up with libarchive's listing.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use super::{RAR5_SIGNATURE, vint};

const RAR4_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";

/// The most blocks one walk reads, so that a crafted file of tiny blocks costs
/// bounded time and memory: what the walk keeps of a block is of a fixed size,
/// but for the name of each regular RAR5 file, which the file itself holds
/// byte for byte. Far above any book's page count.
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
/// A volume holds part of its set's pages and may begin or end with part of
/// one, so it is refused rather than counted alone.
const ONE_VOLUME: &str = "it is one volume of a multi-volume set";
/// libarchive lists an encrypted entry as it lists any other, then fails on or
/// misreads its data, so the archive is refused where it is listed.
const ENCRYPTED_ENTRY: &str = "an entry is encrypted";
/// libarchive's RAR4 reader decodes RAR 2.9's compression alone and reads no
/// entry's version, so it fails on, or misreads, an entry RAR 2.0 or RAR 1.5
/// compressed.
const UNDECODED: &str =
    "an entry uses RAR 2.0 compression or another method libarchive does not decode";
/// rar writes a reference after its target, and unrar reads one only from an
/// entry it has already extracted. Deleting the target leaves the reference
/// naming nothing, and adding a file of that name again puts it after the
/// reference, though it may hold other bytes.
const DANGLING: &str =
    "an entry is a reference or hard link, and its target is not an earlier entry of that name";
/// rar writes a reference with its target's size. One declaring another was
/// written of other bytes, and its target's would read as a page of a size it
/// does not declare.
const RESIZED: &str = "an entry is a reference or hard link, and its size is not its target's";
/// A RAR4 name with no Unicode form reaches taiyaku as the bytes of its
/// writer's code page, with each 0x5C turned into `/` by libarchive, which
/// reads it as RAR's `\`. After a byte above 0x7F that 0x5C may instead be
/// the second byte of a character, as in Shift-JIS's 表 (95 5C), and reading it
/// as a separator can move a page into a folder of no such name or, as in
/// `表.png`, leave it a hidden file that is no page. A zip keeps such a name
/// whole; the walk cannot tell which it is, so the archive is refused rather
/// than counted short.
const AMBIGUOUS_NAME: &str = "an entry's name, in a legacy code page, holds a byte that may be half of a character or a path separator";
/// libarchive lists a junction, or a redirection of a type it does not know,
/// by its attributes: as a regular file it gives no bytes for.
const UNREAD_REDIRECTION: &str =
    "an entry is a junction or a redirection of a type libarchive does not read";
/// libarchive lists such an entry by its attributes and refuses to read its
/// data, where it does not refuse the entry as it lists it.
const MARKED_DIRECTORY: &str = "an entry's header marks it a directory and its attributes a file";

/// What the headers declare of one entry, at its position in the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::rar) struct Header {
    /// The uncompressed size.
    pub(in crate::rar) size: u64,
    /// The CRC32 of the entry's bytes, where one applies to them. A split
    /// entry's is not of the whole file, and a reference, link or directory
    /// has no bytes of its own.
    pub(in crate::rar) crc: Option<u32>,
    /// The BLAKE2sp hash of the entry's bytes that a RAR5 made with
    /// `rar -htb` declares in place of the CRC32, by the CRC32's rule. Boxed,
    /// as most entries have none and a walk holds up to `MAX_BLOCKS` headers.
    pub(in crate::rar) blake2: Option<Box<[u8; 32]>>,
    /// For a hard link or file reference, the position of the earlier entry
    /// whose bytes it reads: the first regular file of the name its record
    /// gives, or that file's own target where it is one too, so that a target
    /// always holds bytes of its own.
    pub(in crate::rar) target: Option<usize>,
}

/// Every entry's header, in archive order, and how the walk ended.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::rar) struct Walk {
    pub(in crate::rar) headers: Vec<Header>,
    /// Why the walk did not reach the end of the archive: a block it cannot
    /// read, or one that refuses the archive, as a volume's main header or an
    /// encrypted entry does.
    pub(in crate::rar) stopped: Option<&'static str>,
    /// Listing past an entry of a solid archive decodes the entries it passes.
    pub(in crate::rar) solid: bool,
    /// It starts with a RAR4 signature, so a name may be in a legacy code page.
    pub(in crate::rar) rar4: bool,
}

impl Walk {
    /// The header of the entry at `position` of libarchive's listing, given
    /// the size libarchive lists it with if it is a regular file. An entry with
    /// a checksum to meet, a CRC32 or a BLAKE2 hash, must be that same file in
    /// the listing.
    pub(in crate::rar) fn listed(
        &self,
        position: usize,
        listed_file: Option<u64>,
    ) -> Result<&Header, &'static str> {
        let Some(header) = self.headers.get(position) else {
            return Err(self.differs());
        };
        let checked = header.crc.is_some() || header.blake2.is_some();
        if checked && listed_file != Some(header.size) {
            return Err(LISTING_DIFFERS);
        }
        Ok(header)
    }

    /// The header of the bytes `header`'s entry reads: its target's, or its own.
    pub(in crate::rar) fn bytes_of<'a>(&'a self, header: &'a Header) -> &'a Header {
        header
            .target
            .and_then(|target| self.headers.get(target))
            .unwrap_or(header)
    }

    /// Why a listing and the walk do not hold the same entries: the cause the
    /// walk stopped for, if it stopped, or else that the two differ.
    pub(in crate::rar) fn differs(&self) -> &'static str {
        self.stopped.unwrap_or(LISTING_DIFFERS)
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
    walk.rar4 = !rar5;
    let mut at = if rar5 { 8 } else { 7 };
    let mut blocks = 0;
    let mut earlier = Earlier::new();
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
            rar5_block(file, at, &mut walk, &mut earlier)?
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

/// The first regular file of each RAR5 name the walk has passed, by position:
/// where a hard link or file reference finds its target. RAR4 has neither.
/// Each name is held whole, so these come to no more than the file's size.
type Earlier = HashMap<Vec<u8>, usize>;

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

    fn take(&mut self, len: u64) -> Option<&'a [u8]> {
        let (taken, rest) = self.0.split_at_checked(usize::try_from(len).ok()?)?;
        self.0 = rest;
        Some(taken)
    }

    /// The last `len` bytes left, where a header keeps its extra area.
    fn last(&self, len: u64) -> Option<&'a [u8]> {
        let start = self.0.len().checked_sub(usize::try_from(len).ok()?)?;
        self.0.get(start..)
    }
}

fn rar5_block<R: Read + Seek>(
    file: &mut R,
    at: u64,
    walk: &mut Walk,
    earlier: &mut Earlier,
) -> std::io::Result<Step> {
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
    let step = body.and_then(|body| rar5_fields(body, at.saturating_add(total), walk, earlier));
    Ok(step.unwrap_or(Step::Stop(BAD_HEADER)))
}

/// The block whose header fields are `body` and whose data starts at
/// `data_start`. `None` where the fields do not parse.
fn rar5_fields(
    body: &[u8],
    data_start: u64,
    walk: &mut Walk,
    earlier: &mut Earlier,
) -> Option<Step> {
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
    const VOLUME: u64 = 0x01;
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
            let archive_flags = fields.vint()?;
            if archive_flags & VOLUME != 0 {
                return Some(Step::Stop(ONE_VOLUME));
            }
            walk.solid |= archive_flags & SOLID_ARCHIVE != 0;
            Some(next)
        }
        FILE => {
            let file = rar5_file(&mut fields, flags, extra)?;
            if file.encrypted {
                return Some(Step::Stop(ENCRYPTED_ENTRY));
            }
            if let Some(cause) = file.unreadable {
                return Some(Step::Stop(cause));
            }
            walk.solid |= file.solid;
            // libarchive lists a split file where it starts, so the block that
            // carries on from the previous volume takes no position.
            if flags & CONTINUED_FROM_LAST_VOLUME != 0 {
                return Some(next);
            }
            let mut header = file.header;
            if let Some(name) = file.target {
                let Some(&target) = earlier.get(name) else {
                    return Some(Step::Stop(DANGLING));
                };
                let onward = walk.headers.get(target).and_then(|target| target.target);
                let target = onward.unwrap_or(target);
                if walk
                    .headers
                    .get(target)
                    .is_some_and(|target| target.size != header.size)
                {
                    return Some(Step::Stop(RESIZED));
                }
                header.target = Some(target);
            }
            if file.regular && !earlier.contains_key(file.name) {
                earlier.insert(file.name.to_vec(), walk.headers.len());
            }
            walk.headers.push(header);
            Some(next)
        }
        SERVICE => Some(next),
        ENCRYPTION => Some(Step::Stop(BAD_HEADER)),
        END | MARKER => Some(Step::End),
        _ if flags & SKIP_IF_UNKNOWN != 0 => Some(next),
        _ => Some(Step::Stop(UNKNOWN_BLOCK)),
    }
}

/// What a RAR5 file block declares.
struct Rar5File<'a> {
    header: Header,
    /// Its data continues a solid stream.
    solid: bool,
    encrypted: bool,
    /// Why libarchive lists it as a regular file that no read gives whole.
    unreadable: Option<&'static str>,
    name: &'a [u8],
    /// libarchive lists it as a regular file with bytes to read, its own or
    /// its target's.
    regular: bool,
    /// The name of the entry whose bytes it reads, for a hard link or file
    /// reference.
    target: Option<&'a [u8]>,
}

fn rar5_file<'a>(fields: &mut Fields<'a>, block_flags: u64, extra: u64) -> Option<Rar5File<'a>> {
    const CONTINUES_IN_NEXT_VOLUME: u64 = 0x10;
    const DIRECTORY: u64 = 0x01;
    const HAS_MTIME: u64 = 0x02;
    const HAS_CRC: u64 = 0x04;
    const SOLID: u64 = 0x40;
    const WINDOWS: u64 = 0;
    const UNIX: u64 = 1;
    const UNIX_SYMLINK: u64 = 1;
    const WINDOWS_SYMLINK: u64 = 2;
    const HARD_LINK: u64 = 4;
    const FILE_COPY: u64 = 5;

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
    let name = fields.take(name_len)?;
    let records = rar5_extra(fields.last(extra)?)?;
    let regular = match host {
        WINDOWS => attributes & 0x10 == 0,
        UNIX => is_regular_mode(attributes),
        _ => false,
    };
    // libarchive lists any other redirection as a link, or as a file it
    // gives no bytes for.
    let target = records
        .redirect
        .and_then(|(kind, target)| matches!(kind, HARD_LINK | FILE_COPY).then_some(target));
    let redirected = records.redirect.is_some();
    let symlink = records
        .redirect
        .is_some_and(|(kind, _)| matches!(kind, UNIX_SYMLINK | WINDOWS_SYMLINK));
    // libarchive lists a symbolic link as a link and any other entry by its
    // attributes, so these are files to it that no read gives whole.
    let unreadable = if !regular || symlink {
        None
    } else if file_flags & DIRECTORY != 0 {
        Some(MARKED_DIRECTORY)
    } else if redirected && target.is_none() {
        Some(UNREAD_REDIRECTION)
    } else {
        None
    };
    let checked = regular && !redirected && block_flags & CONTINUES_IN_NEXT_VOLUME == 0;
    Some(Rar5File {
        header: Header {
            size,
            crc: crc.filter(|_| checked),
            blake2: records.blake2.filter(|_| checked).map(Box::new),
            target: None,
        },
        solid: compression & SOLID != 0,
        encrypted: records.encrypted,
        unreadable,
        name,
        regular: regular && (!redirected || target.is_some()),
        target,
    })
}

/// What the records of a file's extra area say of its data.
#[derive(Default)]
struct Extra<'a> {
    encrypted: bool,
    /// The BLAKE2sp hash a file hash record gives.
    blake2: Option<[u8; 32]>,
    /// The redirection record's type and the name it gives: the entry's bytes
    /// are another entry's, or it is a link.
    redirect: Option<(u64, &'a [u8])>,
}

/// `None` where a record does not fit the area, or its fields do not fit it,
/// and where a file hash record gives a hash of a type other than BLAKE2sp,
/// which libarchive refuses as it lists the entry, or holds more than its
/// hash.
fn rar5_extra(mut area: &[u8]) -> Option<Extra<'_>> {
    const ENCRYPTION: u64 = 1;
    const HASH: u64 = 2;
    const REDIRECTION: u64 = 5;
    const BLAKE2SP: u64 = 0;

    let mut extra = Extra::default();
    while !area.is_empty() {
        let (size, size_len) = vint(area, MAX_VINT)?;
        let rest = area.get(size_len..)?;
        let record = rest.get(..usize::try_from(size).ok()?)?;
        let mut fields = Fields(record);
        match fields.vint()? {
            ENCRYPTION => extra.encrypted = true,
            HASH => {
                if fields.vint()? != BLAKE2SP {
                    return None;
                }
                extra.blake2 = Some(*fields.take(32)?.first_chunk()?);
                // libarchive reads what follows the hash as further records,
                // where the walk would step over it.
                if !fields.0.is_empty() {
                    return None;
                }
            }
            REDIRECTION => {
                let kind = fields.vint()?;
                let _flags = fields.vint()?;
                let name_len = fields.vint()?;
                extra.redirect = Some((kind, fields.take(name_len)?));
            }
            _ => {}
        }
        area = rest.get(record.len()..)?;
    }
    Some(extra)
}

fn rar4_block<R: Read + Seek>(file: &mut R, at: u64, walk: &mut Walk) -> std::io::Result<Step> {
    let mut block = read_at(file, at, FIRST_READ)?;
    // A header of more, a file's with a long name, is read whole: the walk
    // reads the name.
    if let Some(size) = u16_at(&block, 5)
        .map(u64::from)
        .filter(|&size| size > FIRST_READ)
    {
        block = read_at(file, at, size)?;
    }
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
    const VOLUME: u16 = 0x0001;
    const ENCRYPTED: u16 = 0x0004;
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
        MAIN if size < MAIN_SIZE => None,
        MAIN if flags & VOLUME != 0 => Some(Step::Stop(ONE_VOLUME)),
        MAIN => Some(after(0)),
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
                if flags & ENCRYPTED != 0 {
                    return Some(Step::Stop(ENCRYPTED_ENTRY));
                }
                let unpacked = u64::from(u32_at(block, 11)?) | high(36)? << 32;
                let header = block.get(..usize::try_from(size).ok()?)?;
                match rar4_file(header, flags, unpacked)? {
                    Ok(header) => walk.headers.push(header),
                    Err(cause) => return Some(Step::Stop(cause)),
                }
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

/// A file block's header, whose unpacked size is `size`, or why the archive
/// is refused at it. `None` where its name runs past `block`, the header
/// whole, which libarchive refuses too.
fn rar4_file(block: &[u8], flags: u16, size: u64) -> Option<Result<Header, &'static str>> {
    const SPLIT: u16 = 0x01 | 0x02;
    const LARGE: u16 = 0x0100;
    const UNICODE: u16 = 0x0200;
    const STORED: u8 = 0x30;
    const RAR29: u8 = 29;
    const RAR29_METHODS: std::ops::RangeInclusive<u8> = 0x31..=0x35;

    let attributes = u32_at(block, 28)?;
    // libarchive takes the file type from the attributes by the host's rules.
    let regular = match *block.get(15)? {
        0..=2 => attributes & 0x10 == 0,
        3..=5 => is_regular_mode(u64::from(attributes)),
        _ => false,
    };
    // libarchive copies a stored entry's bytes whatever version it names, and
    // asks its decoder for none of an entry of no bytes. A directory or a link
    // is no page, and refuses nothing however it is compressed.
    let version = *block.get(24)?;
    let method = *block.get(25)?;
    let decodes = method == STORED || (version == RAR29 && RAR29_METHODS.contains(&method));
    if regular && size > 0 && !decodes {
        return Some(Err(UNDECODED));
    }
    let name_at = if flags & LARGE == 0 { 32 } else { 40 };
    let name = block.get(name_at..name_at + usize::from(u16_at(block, 26)?))?;
    // Under the Unicode flag the name is Unicode: one that follows the stored
    // name after a zero byte, or, with none, the stored name read as UTF-8,
    // in which a 0x5C is always the separator. Without it, the stored name is
    // read as far as a zero byte.
    let stored = name.split(|&byte| byte == 0).next().unwrap_or_default();
    if flags & UNICODE == 0 && ambiguous(stored) {
        return Some(Err(AMBIGUOUS_NAME));
    }
    let checked = regular && flags & SPLIT == 0;
    let crc = u32_at(block, 16)?;
    Some(Ok(Header {
        size,
        crc: checked.then_some(crc),
        blake2: None,
        target: None,
    }))
}

/// A RAR4 name with no Unicode form that holds a 0x5C right after a byte
/// above 0x7F: [`AMBIGUOUS_NAME`]. Such a name may be valid UTF-8 and still be
/// Shift-JIS, whose first bytes 0x81 to 0x9F are UTF-8's continuation bytes
/// too, so its bytes do not settle it. A 0x5C after a byte of ASCII, or at the
/// start, is no second byte of a character in any code page that has them,
/// so it is the separator libarchive reads it as.
fn ambiguous(name: &[u8]) -> bool {
    name.windows(2)
        .any(|pair| pair[0] > 0x7f && pair[1] == b'\\')
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
