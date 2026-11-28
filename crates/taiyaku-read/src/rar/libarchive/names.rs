//! How an entry's name is decoded from the bytes libarchive gives for it.
//!
//! A RAR5 stores every name as UTF-8. A RAR4 header under its Unicode flag
//! carries a Unicode name, after the stored one or as the stored one in UTF-8,
//! which libarchive gives as UTF-8, or with no name at all where the bytes it
//! takes for UTF-8 are not, so that compress-tools refuses the entry. Without
//! the flag, a RAR4 stores the bytes of whatever code page the writer's system
//! used, which libarchive gives as they are but for each 0x5C, RAR's separator
//! `\`, which it gives as `/`. Such a name that is not UTF-8 is read as code
//! page 437, as a zip's name without the UTF-8 flag is (`zip`'s own table,
//! which that crate keeps private), so the page keeps its place under a name
//! that maps every byte (O351). A legacy name whose bytes happen to be valid
//! UTF-8 cannot be told from a UTF-8 one, and reads as UTF-8. One whose 0x5C
//! follows a byte above 0x7F never reaches a decoder: that 0x5C may be half of
//! a character, as in Shift-JIS, whether or not the bytes are UTF-8, and the
//! walk refuses the archive (`headers::AMBIGUOUS_NAME`).

/// This rule's part of the backend's signature, so a build that decodes names
/// by another rule counts every RAR again: an archive one rule refuses for its
/// names, another may count.
pub(in crate::rar) const RULE: &str = "rar4-cp437";

/// A RAR4 entry's name: UTF-8 where its bytes are, else code page 437.
/// Never fails, since code page 437 maps every byte.
#[expect(
    clippy::unnecessary_wraps,
    reason = "compress-tools' decode callback returns a Result"
)]
pub(in crate::rar) fn rar4(bytes: &[u8]) -> compress_tools::Result<String> {
    Ok(match std::str::from_utf8(bytes) {
        Ok(name) => name.to_owned(),
        Err(_) => cp437(bytes),
    })
}

/// A RAR5 entry's name, which the format stores as UTF-8: any other is
/// refused, as compress-tools' own decoder refuses it.
pub(in crate::rar) fn utf8(bytes: &[u8]) -> compress_tools::Result<String> {
    Ok(std::str::from_utf8(bytes)?.to_owned())
}

/// `bytes` read as code page 437, whose lower half is ASCII.
pub(in crate::rar) fn cp437(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| match byte.checked_sub(0x80) {
            Some(high) => HIGH[usize::from(high)],
            None => char::from(byte),
        })
        .collect()
}

/// What code page 437 maps 0x80 to 0xFF to, in order.
const HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];
