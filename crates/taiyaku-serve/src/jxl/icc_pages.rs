//! Hand-built JPEG XL codestreams whose header asks for an embedded colour
//! profile of a chosen encoded size, and the containers that carry them. A
//! page declaring a profile ends inside the profile's stream: read with no
//! check ahead of it, jxl-oxide fills the declared size, then runs out of bits.
//! An encoded profile is built too, as jxl-oxide holds one once it has read it.

use crate::source::JXL_CONTAINER;

/// A bare codestream whose header wants a profile of `enc_size` encoded bytes.
pub(super) fn declaring(enc_size: u64) -> Vec<u8> {
    page(true, enc_size)
}

/// A bare codestream whose header wants no profile, sRGB by default, followed
/// by the bits that would declare the largest profile jxl-oxide accepts: a
/// reader that took them for a profile's size would refuse the page.
pub(super) fn wanting_no_profile() -> Vec<u8> {
    page(false, 1 << 28)
}

/// `codestream` in a container, whole in one `jxlc` box.
pub(super) fn in_jxlc(codestream: &[u8]) -> Vec<u8> {
    [JXL_CONTAINER, &boxed(*b"jxlc", codestream)].concat()
}

/// `codestream` in a container, split across three `jxlp` boxes so the header
/// runs across the first two and the profile's size across the last two.
pub(super) fn in_jxlp(codestream: &[u8]) -> Vec<u8> {
    let parts = [
        (0_u32, &codestream[..4]),
        (1, &codestream[4..8]),
        // The high bit marks the last part.
        (0x8000_0002, &codestream[8..]),
    ];
    let mut page = JXL_CONTAINER.to_vec();
    for (index, part) in parts {
        page.extend(boxed(*b"jxlp", &[&index.to_be_bytes()[..], part].concat()));
    }
    page
}

/// `codestream` as each shape of page jxl-oxide reads, named for a message.
pub(super) fn every_shape(codestream: &[u8]) -> [(&'static str, Vec<u8>); 3] {
    [
        ("bare", codestream.to_vec()),
        ("jxlc", in_jxlc(codestream)),
        ("jxlp", in_jxlp(codestream)),
    ]
}

/// An encoded profile of `len` bytes that decodes to the largest profile
/// jxl-color writes from that many: after the sizes, every command is a tag
/// code that writes three tag entries and reads no data. The header's first
/// bytes give the profile the size it decodes to, so the parse after the
/// decode reads every entry too.
pub(super) fn expanding_profile(len: usize) -> Vec<u8> {
    // The largest decoded size jxl-color allows, so no entry runs past it.
    const DECLARED: u32 = 1 << 28;
    let mut commands = len;
    loop {
        // Tag code 2 writes rTRC, gTRC and bTRC, twelve bytes each.
        let entries = 3 * commands;
        let decoded = u32::try_from(128 + 4 + 12 * entries).expect("the profile is under 4 GiB");
        let mut tag_commands = varint(entries as u64 + 1);
        tag_commands.resize(tag_commands.len() + commands, 2);
        let mut profile = varint(DECLARED.into());
        profile.extend(varint(tag_commands.len() as u64));
        profile.extend(tag_commands);
        // jxl-color adds each header byte to the one it predicts, and predicts
        // the first four, the profile's size, from the declared size.
        let mut header = [0_u8; 128];
        for ((byte, wanted), predicted) in header
            .iter_mut()
            .zip(decoded.to_be_bytes())
            .zip(DECLARED.to_be_bytes())
        {
            *byte = wanted.wrapping_sub(predicted);
        }
        profile.extend(header);
        if profile.len() <= len {
            // Data no command reads.
            profile.resize(len, 0);
            return profile;
        }
        commands -= profile.len() - len;
    }
}

/// A value as jxl-color's profile stream holds one: seven bits a byte, least
/// significant first, the high bit set on every byte but the last.
fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let low = u8::try_from(value & 0x7f).expect("seven bits fit in a byte");
        value >>= 7;
        if value == 0 {
            bytes.push(low);
            return bytes;
        }
        bytes.push(low | 0x80);
    }
}

fn boxed(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = u32::try_from(payload.len() + 8).expect("a hand-built box is small");
    [&size.to_be_bytes()[..], &kind, payload].concat()
}

/// Bits as a codestream holds them: least significant first, each field in
/// its own width, the last byte zero-padded.
#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    len: usize,
}

impl Bits {
    fn put(&mut self, value: u64, width: usize) {
        for i in 0..width {
            if self.len.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if (value >> i) & 1 == 1 {
                *self.bytes.last_mut().expect("a byte was pushed") |= 1 << (self.len % 8);
            }
            self.len += 1;
        }
    }

    /// A U64 in its widest form, which holds every size these pages declare:
    /// twelve bits, then a byte at a time, each behind a bit that says it follows.
    fn put_u64(&mut self, value: u64) {
        self.put(3, 2);
        self.put(value & 0xfff, 12);
        let mut rest = value >> 12;
        while rest != 0 {
            self.put(1, 1);
            self.put(rest & 0xff, 8);
            rest >>= 8;
        }
        self.put(0, 1);
    }
}

/// The page: an image header, then the start of the profile's stream.
fn page(wants_profile: bool, enc_size: u64) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.put(0x0aff, 16);
    // The size: with div8 set, the height is stored as its multiple of eight
    // less one, so 0 is 8 px high; ratio 1 makes it as wide as it is high.
    bits.put(1, 1);
    bits.put(0, 5);
    bits.put(1, 3);
    // The metadata, not all default: no extra fields, 8-bit integer samples,
    // 16-bit modular buffers, no extra channels, not XYB.
    bits.put(0, 1);
    bits.put(0, 1);
    bits.put(0, 1);
    bits.put(0, 2);
    bits.put(1, 1);
    bits.put(0, 2);
    bits.put(0, 1);
    if wants_profile {
        // The colour encoding: not all default, an ICC profile wanted, RGB.
        bits.put(0, 1);
        bits.put(1, 1);
        bits.put(0, 2);
    } else {
        bits.put(1, 1);
    }
    // No extensions, the default transform: the header ends here.
    bits.put(0, 2);
    bits.put(1, 1);
    bits.put_u64(enc_size);
    // The profile's entropy code: no LZ77, every context in one cluster, one
    // prefix code whose tokens are their own values, over an alphabet with
    // room for the symbols below, four of them at two bits each.
    bits.put(0, 1);
    bits.put(1, 1);
    bits.put(0, 2);
    bits.put(1, 1);
    bits.put(15, 4);
    bits.put(1, 1);
    bits.put(7, 4);
    bits.put(0, 7);
    bits.put(1, 2);
    bits.put(3, 2);
    for symbol in [0, 1, 2, 128] {
        bits.put(symbol, 8);
    }
    bits.put(0, 1);
    // The stream's first symbols: a decoded size and an empty command stream,
    // then zeros. They pass every check jxl-oxide makes before it fills the
    // declared size, so only a check ahead of it keeps the fill from happening.
    for symbol in [128, 128, 128, 128, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
        bits.put(code_of(symbol), 2);
    }
    bits.bytes
}

/// A symbol's two-bit code, as the reader takes it least significant bit first.
fn code_of(symbol: u8) -> u64 {
    match symbol {
        0 => 0b00,
        1 => 0b10,
        2 => 0b01,
        128 => 0b11,
        _ => unreachable!("the prefix code has four symbols"),
    }
}
